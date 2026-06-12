# ADR-031 — Label rendering + structured print-request model

- Status: Accepted
- Date: 2026-06-10
- Component / area: `crates/codec` (rendering) + `crates/app` (the
  `Request::Print` shape) + the `pr` CLI fast-path. Refines how the
  `render-label` / print operation works under ADR-030.
- Reviewers: Lars Gerchow (accepted 2026-06-11)
- Related: ADR-012 (part ID scheme), ADR-015 (print-event log), ADR-017
  (Rust core / codec SSOT), ADR-019 (proposal sink — mint timing),
  ADR-022 (audit/print events), ADR-030 (shells + command protocol)
- Feeds: `decisions/explorations/operations-catalog.md` (§C print model)

## Context

Today label rendering emits **one SVG file per ID** with millimetre
sizing (`label.py`, `crates/codec`), and the web app has its own
browser print pipeline. ADR-030 reclassifies printing as a core
**operation** (`render-label`, universal) plus a shell **capability**
(`deliver-print`, per-shell I/O). That leaves the *request shape* and the
*rendering discipline* unspecified, and four concrete requirements have
surfaced:

1. **Physical fidelity.** On a 300-dpi thermal head, a QR module whose
   edge falls on a sub-pixel boundary is rendered as a merged or dropped
   dot. For Micro QR (e.g. M2 = 13×13 modules) this corrupts the symbol
   and breaks decode. Modules must be a whole number of device pixels.
2. **Job uniformity.** A print job's label set may contain QR symbols of different
   module counts (Micro vs Standard, or different payloads) → different
   pixel footprints. Labels should share one physical footprint so a
   strip of them looks and feeds uniformly.
3. **Ergonomics.** The bench wants a single command that mints *and*
   prints N fresh IDs, not a mint step then a separate render step.
4. **Correctness vs latency.** Minting is a registry change (a proposal,
   ADR-019). The bench cannot wait on PR/CI latency before physical
   labels come out of the printer.

These are rendering + request-protocol decisions, distinct from ADR-030's
architecture, hence their own ADR.

## Alternatives considered

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| **Status quo** — one SVG file per ID, mm sizing, render-only | Already works | No px-true guarantee (sub-pixel modules); no batch uniformity; no mint+print fast-path; print is a separate manual step | Rejected |
| **mm-only sizing, scale at print time** | Physical-size accurate by construction | The device-pixel snapping that prevents malformed modules happens (or fails to) in the print driver, outside our control | Rejected — fidelity must be guaranteed at render time |
| **px-true rendering anchored to device dots + structured `Request::Print` + fast-path CLI + optimistic-with-pre-flight mint** | Guarantees whole-pixel modules; batch-uniform; one bench command; bench never blocks on CI | Renames some `label.py` flags; needs a pre-flight read of `main`; raster (png/pdf) needs a converter | **Chosen** |
| **Print only after the mint proposal merges** | No orphan physical labels ever | Bench blocks on PR/CI latency for every print — unusable at a labeling station | Rejected (kept as a `--strict` opt-in, see Open questions) |

## Decision

### 1. Printing is a structured request

A `Request::Print` variant (serde, in `crates/app`) carries the whole
job as data — selection (the shared `Selection` type, §7 — `mint_count`
was generalized out: `-n` composes `Create→Print`), layout,
sizing, padding, and output format. Every shell builds the same request;
the CLI fast-path is sugar over it.

Fast-path example:

```
pr -p -n 10 --size 52 --unit px --format horz --chars 44 --padding 2
```

→ mint 10 fresh IDs and print them, QR-left/text-right, 52 px tall,
human ID as two rows of four, ≥2 px padding.

### 2. px-true rendering (the load-bearing rule)

**`--size` is the exact output canvas** (the label's controlling
dimension in device px); **`--padding` is the minimum padding**; the
module size is **deduced**:

```
available  = size_px - 2 * padding_min_px
module_px  = floor(available / N)          # N = modules incl. quiet zone
           → ERROR if module_px < 1        # chosen QR/payload cannot fit
symbol_px  = N * module_px                  # ⇒ symbol_px % N == 0, always
actual_pad = (size_px - symbol_px) / 2      # absorbs the remainder; ≥ floor
```

The output is **exactly** the requested size; every module is an
identical whole number of device pixels (no fractional edges, no
malformed dots); the remainder distributes into padding — which is
*why* §4 defines padding as a floor. An impossible fit (the payload's
symbol can't reach 1 px/module inside the padding floor) is a hard
**error with a minimum-size hint**, never a silently degraded render.
Worked example: `size 64, padding 2` → available 60, Micro QR M4
(17+2·2 = 21 modules) → `module_px 2`, symbol 42 px, actual padding
11 px — a 64 px output. This generalizes the pixel-grid discipline
proven in `tools/printer_test_62mm.py` into a first-class codec mode.

### 3. Sizing, units, DPI

- **Native unit = device pixels** (printer dots).
- `--unit mm --dpi <n>` converts mm → px at `n` dots/inch (default DPI
  comes from the configured printer profile, e.g. Brother QL ≈ 300 dpi),
  then snaps to the px-true grid → physical size is exact to the dot.
- `--unit px` is direct.

Physical fidelity is guaranteed regardless of how size was expressed.

### 4. Padding is a floor; the job fills to max footprint

`--padding` is a **minimum** (device px). When rendering a print job's
label set, every label is padded so all share the job's **largest QR
footprint** — the labels come out physically uniform, with `--padding`
as the smallest allowed gap. (This is a per-print-job property — not the
registry `batch` field, which ADR-035 retires.)

### 5. Layout / text / output flags

Adopt the user's naming (a rename from `label.py`):

| Flag | Meaning | Values |
|---|---|---|
| `--format` | geometry | `horz` (QR left, text right) · `vert` · `flag` |
| `--chars` | human-ID grouping | `44` (two rows of 4) · `444` · `554` |
| `--emit` | output file format | `svg` (native) · `png` · `pdf` |
| `--size` / `--unit` / `--dpi` | sizing | px (native) / mm+dpi |
| `--padding` | min gap (px) | integer |
| `-n <N>` | mint N fresh + print | with `-p` |
| `-p` | print (existing selection, or minted with `-n`) | — |

`svg` is the canonical render; `png`/`pdf` are rasterized from it.

### 6. Mint+print timing — optimistic with pre-flight

When `-n` mints as part of printing:

1. **Pre-flight:** fetch the contract + current registry from `main`
   (read-only — raw fetch for a `github:` locator, working-copy read for
   `file://`) and run the **same Rust validators** locally (the core is
   shared, ADR-017/030, so this is authoritative, not a guess).
2. **Mint optimistically:** allocate IDs (nano-id entropy makes a
   same-instant collision between two processes negligible, and the
   pre-flight checks against current `main`).
3. **Print immediately** — the bench never blocks.
4. The registry update flows **in parallel**: a PR for `github:`, a
   direct commit for `file://`. An abandoned/rejected proposal leaves
   only harmless `unbound` IDs, which are voidable.

A `--strict` flag (Open questions) inverts step 3 to print-after-land for
callers that need zero orphan labels.

### 7. Generalized request shape (2026-06-11 pass, per ADR-035 §0)

- **`Print{collection, selection, …}`** — printing is not parts-only:
  any id-bearing entity (locations, containers) is sticker-able. The
  parts preset ships the current label block verbatim (day-1 behavior
  unchanged); non-parts label presets are deferred until requested.
- **`selection` is the shared `Selection` type** (`Ids | Filter`, where
  `Filter` is *the* `List` filter — one grammar, no drift).
  `mint_count` leaves the request: the fused `-n 10` bench command is
  CLI sugar composing `Create{parts, n}` → `Print{ids}` — the §6
  pre-flight/optimistic timing and the one-stamp-per-mint-event
  invariant attach to the `Create` leg, where they belong. (Non-CLI
  shells make two protocol calls; `app` may keep a thin compose
  wrapper.)
- **Human-form groupings come from the id-scheme declaration**, not
  global flags: `nano14` declares `44|444|554`; an external scheme
  (`udi:`) declares its own or none. `--chars` is sugar over that
  declaration; the label block (which fields render, default layout)
  lives in the collection descriptor (ADR-035 §1a). Longer
  external-scheme payloads may not fit Micro QR — the render degrades
  honestly to Standard QR or errors with a sizing hint.
- **Print formats are named presets in the descriptor's render block**
  (2026-06-11): e.g. `cable-52px = {layout: horz, size: 52px, chars:
  44, padding: 2, output_mode}` — named, versioned with the contract,
  CODEOWNERS-gated (they encode regulated legibility decisions). The
  fast-path flags resolve to / override a preset; print events record
  `{preset, resolved params}` — resolved params are the evidence
  (stands alone even if the preset later changes), preset + contract
  version are the provenance. Layout *algorithms* stay code (one meta
  level). **Not an id'd collection** — formats are render structure
  (single-home rule, ADR-034), referenced only by audit events;
  *promotion trigger:* if formats ever need an operator-paced lifecycle
  or references from entities, they promote to a collection.

## Rationale

**px-true is correctness, not polish.** Sub-pixel module edges on a
thermal head merge or drop dots; for a 13-module Micro QR that is the
difference between a scannable and a dead label. `symbol_px % N == 0` is
the exact, printer-agnostic rule, and deriving `module_px` by floor-div
makes it hold by construction.

**Optimistic mint is safe *because* the core is shared.** The client runs
the same validators against fresh `main` state in pre-flight, so printing
before the proposal lands is a checked optimism, not a gamble — this is a
direct dividend of the one-core thesis (ADR-017/030). Blocking the bench
on CI latency (the rejected alternative) would make the labeling station
unusable.

**Structured request + fast-path** keeps printing a first-class
`Request` (so it appears in every shell and in the ADR-030 §8 parity
matrix) while giving the bench a terse one-liner.

## Consequences

- **codec** gains a px-true module-snapping render mode and a
  batch-footprint-fill pass (generalize `printer_test_62mm.py`).
- **`Request::Print`** enters the `crates/app` command enum → it is an Op
  in the ADR-030 §8 op×spoke parity matrix; `deliver-print` stays a shell
  capability.
- **Flag rename** from `label.py`: geometry is `--format` (was
  `--layout`), grouping is `--chars` (was `--format`), output is
  `--emit`. The Python CLI stays as-is until retired (ADR-017
  strangler-fig); the new names live on `pr`.
- **Pre-flight** needs a read path to `main`: raw-content fetch for
  `github:`, working-copy read for `file://`.
- **Raster output** (`png`/`pdf`) needs a converter; whether that lives
  in the core or stays a CLI-only concern is an Open question.
- **Print-event audit** (ADR-015/022) is written from the same request,
  recording layout/size/copies/operator as today.

### 8. Print contracts (2026-06-11 — SSOT/DRY across every print type)

The §2 workflow is not micro-QR-specific: it is the first instance of a
**print contract** — a named parameterization the tool implements once
and registries declare:

- **One deduction engine.** The §2 math (`size`, `padding_min`, `N` →
  `module_px` | ERROR) is written **once**; a symbology contributes only
  its module count `N` (incl. quiet zone): `micro-qr` M4 → 21,
  standard `qr` V1 → 25, future symbologies likewise. No per-type
  re-implementation of the sizing/error logic.
- **Groupings are data, not code.** A chars grouping is a vector of row
  lengths — `44` = `[4,4]`, `444` = `[4,4,4]`, `554` = `[5,5,4]` — and
  **one** text renderer consumes the vector. The id-scheme declares
  which vectors are legal (`nano14` declares all three; ADR-035 §0
  typed ids); adding a grouping is a declaration, not a renderer.
- **The id-text payload shares the QR's sizing system** (2026-06-11):
  the text block spans exactly the QR's *module part* (co-sized), and
  font height + row gaps are **integer multiples of `module_px`** — the
  whole payload lives on one device-pixel lattice, deduced once from
  `size`/`padding` (§2). No second sizing dialect: change the canvas
  and symbol + typography scale together, both grid-crisp.
- **Padding references the MODULE part; the quiet zone counts toward
  it** (2026-06-11): the deduction maximizes `m` subject to
  `data·m + 2·max(padding_min, quiet·m) ≤ size` (micro M4: data 17,
  quiet 2). The quiet zone's whitespace satisfies padding — it is not
  double-counted outside it — and `white ≥ quiet·m` is structural, so
  padding can never starve decodability. Worked example: size 64 /
  pad 2 → m=3 (17·3 + 2·max(2,6) = 63 ≤ 64), module part 51px,
  uniform white ≈ 6–7px.
- **Uniform padding, derived gap** (2026-06-11): the actual white is
  the SAME on all four canvas edges (remainder absorbed uniformly),
  and the QR→text gap is **1.5 × the actual padding** — both derived
  from the one deduction, no independent layout constants.
- **`padding_mode` flag** (2026-06-11): `overlap` (default) = the
  geometry above — the quiet zone counts toward outside padding,
  because printers contribute intrinsic unprintable margins
  before/after/beside the label, so the device already donates outer
  white and the label spends its pixels on modules. `additive` = the
  quiet zone is excluded from outside padding
  (`(data + 2·quiet)·m + 2·pad ≤ size`) — for full-bleed/die-cut
  contexts where the canvas edge is the physical edge. Forward hook:
  printer *profiles* (§3) later declare their intrinsic margins so the
  deduction credits device white explicitly instead of the operator
  choosing a mode by feel. **Margins are PER-SIDE** (2026-06-11:
  confirmed on hardware — the printer prints asymmetrically): profiles
  declare {leading, trailing, left, right} in tape coordinates, the
  per-axis deduction credits each side independently, and `--align` +
  per-side `--padding` are the operator-level escapes until the
  profile carries measured numbers.
- **Payloads stay opaque ids — the security model is
  symbology-independent** (2026-06-11): no symbology ever encodes
  data, only the registry id ('data in matrix' is the anti-pattern: a
  forged/wrong label carries its own unverifiable truth and no
  checkpoint exists to catch it). The BIND is the certified roundtrip
  — scan the physically applied label, resolve against the registry,
  land the association as a reviewed/audited PR — so a wrong sticker
  fails at the ceremony where physical meets digital, and every later
  scan resolves to current (revocable, access-controlled) registry
  truth. Symbology choice (qr/micro/dm) is therefore purely physical
  ergonomics inside this contract: DM earns its place for tiny/curved/
  DPM contexts (1-module quiet zone, denser) while qr/micro stays
  default for native phone decoding. Roundtrip hardenings to wire into
  bind: print-provenance (scanned id must exist in the print log) and
  duplicate-bind detection (same id bound twice = cloned-sticker
  alarm).
- **Embedded bitmap-grid typography — glyphs ARE modules**
  (2026-06-11): replace `font-family="monospace"` (rasterizer-dependent
  — every renderer substitutes its own font and antialiasing) with an
  embedded open-source bitmap monospace (Spleen, BSD-2-Clause, 5×8
  cell; Cozette/MIT the fallback candidate), our ~36-glyph alphabet as
  a const bit-table in the codec. Glyphs render through the SAME
  module-rect emitter as the QR — `<text>` leaves the SVG entirely;
  the label becomes one deterministic binary raster on the module
  lattice, identical across rsvg/browser/printer/wasm. Glyph pixel
  `g = module_px` when it fits (text dots = QR dots); otherwise snap
  down to the next integer `g` that fits, centrally aligned in the
  block. Exact fit at the default: 5×8 cell, `44` grouping →
  8m + 1m gap + 8m = 17m = the M4 module part, zero remainder.
  License rides in the SOUP inventory. **Bench verdict (2026-06-11,
  printed + judged on hardware): the 5×7 const table WINS across the
  entire practical ink range (1.2–5mm)** — the considered tier 2
  (embedded outline font + pure-Rust rasterizer + 50% threshold at
  device resolution) never crosses over and is therefore NOT BUILT: no
  font bytes, no rasterizer dependency, no extra SOUP rows. The 5×7
  table is the px print contract's typography, period. Bench artifacts:
  `labels/typography-bench/bench3-*` (coarse 5×7 vs threshold-
  rasterized JetBrainsMono vs AA control, lookalike pairs included).
  **FINAL typography (2026-06-12): nx75 — the part-registry anchor
  font.** Supersedes the Spleen multi-cell verdict (Spleen removed
  from runtime and SOUP; generator retained as bench tooling). The
  font is first-party, interactively authored: source of truth is
  `design/glyph-font.v1.json` (31 glyphs: 7x5 anchor pixels, edge
  overrides, per-anchor corner-kernels) clicked in the checked-in
  editor (`tools/font_editor_gen.py`, which doubles as the REFERENCE
  render implementation). Design grammar mined from the authored data
  — four rules: (1) diagonal-touching anchors are diamond; (2)
  orth-only anchors square; (3) diagonal tips keep the outward corner;
  (4) diagonals carry into their corner anchor, orth stubs yield.
  Render law (every clause traceable to a printed/pointed-at
  artifact): half-edge kernel sweeps to edge midpoints; orth bodies
  width k; diagonal bands k anti-diagonal rows with parity remainder
  and the k=3 bonus row on the OUTSIDE; k<=2 floor = full
  perpendicular width; near ink (nonzero balance, canonical edge
  frame) bands hug the outside of the anchor line — overshoot
  structurally impossible, no guards; band-owned pass-through anchors
  carry no stamp (constant-derivative law); pure diagonal tips cap
  corners-only; cell mask clips. Baked via `tools/bake_glyph_font.py`
  (drift-gated --check) into `crates/codec/src/glyph_font.rs` with
  per-glyph ink checksums at k=2/3/4/6 locking the Rust renderer to
  the reference bit-for-bit (cross-language A/B verified at zero
  pixel mismatches).
  Supersession trigger: label formats with ink heights well beyond
  5mm where letterform aesthetics matter — re-run the bench before
  reconsidering tier 2. **Typography verdict (2026-06-11, superseding
  the same-day 5×7-only call after the Spleen-v2 prototype): Spleen
  multi-cell, better-res selection.** Cells {6×12, 8×16, 12×24,
  16×32, 32×64} (BSD-2; the 5×8 is excluded), vendored as const
  bit-tables in the codec. Selection: nominal block = rows·cell·k
  fitted closest into the OVERALL label size; ties favor the larger
  cell at lower k (native resolution beats integer upscaling). The
  slack between cap ink and the block is **JUSTIFIED to the
  module-part span** (2026-06-11 refinement: outer white clipped —
  top row flush with the QR's top edge, bottom row flush with its
  bottom, slack absorbed between rows only; the text column mirrors
  the QR's vertical extent — the co-sizing rule preserved). Worked
  example (validated `labels/px64-clip-m3l-spleen-v3/`): 64px label,
  `44` grouping → 16×32 @ k=1, nominal 64/64, ~24px cap rows, top
  flush, between 16, bottom flush. The
  first-party 5×7 table remains the floor for blocks too small for
  the 12-cell. Glyph px is hereby decoupled from module px (a k=1
  native cell may be finer than the QR modules — the one-lattice
  principle holds per element, not across them). SOUP row: Spleen,
  BSD-2, notice retained in glyphs source.
  **ID-optimized by ownership**:
  nano14 already excludes `0/O/1/I/L` at the alphabet level; the
  remaining lookalike pairs (`8/B`, `5/S`, `2/Z`, `6/G`, `U/V`) are
  hand-tunable in our bit-table — the embedded micro-font is a
  registry artifact tuned to exactly this alphabet. For VECTOR
  contexts (webapp UI, large labels, exports): **Atkinson Hyperlegible
  Mono** (OFL, Braille Institute — purpose-built character
  disambiguation; B612 Mono the avionics-grade runner-up), named and
  bundled explicitly — the generic `monospace` family (renderer-
  substituted, em-box ambiguity, two bugs caught on hardware
  2026-06-11) is retired everywhere.
- **Symbology version + EC level are contract parameters, not
  hardcodes** (2026-06-11): today M4/EC-M is fixed; for the nano14
  payload the feasible space is exactly M4-L, M4-M, M3-L (M1/M2 can't
  hold 14 alnum; M4-Q caps at 13). Expose `ec: l|m` (+ auto version):
  M3-L has 15 data modules vs M4's 17, so the §8 deduction yields
  bigger dots at the same canvas (clip@64: 4px vs 3px, +33%) at the
  cost of ~7% vs ~15% codeword correction — on thermal media, dot
  fidelity often binds before damage tolerance, so M3-L may decode
  better; the printer A/B decides, the contract offers both. The
  symbology contributes only its module counts to the one deduction
  engine, so this is a declaration, not new math.
- **`padding_mode: clip` — the maximizer** (2026-06-11): the digital
  artifact carries ZERO embedded quiet zone; `m = floor((size −
  2·pad_min)/data)` with pad defaulting 0, so modules fill the canvas
  maximally (size 68, M4: clip → 4px modules/68px vs overlap → 3px/63 —
  a third bigger). Rationale: the printer's intrinsic unreducible
  white (cut-feed margin ≈1.5mm, unprintable side margins) IS a quiet
  zone supplied by the hardware — the spec requires contrast area at
  scan time, not pixels in the file. Safety stays declared, not
  assumed: the printer profile's intrinsic margins are checked against
  `quiet·m` at the configured dpi; the renderer warns when the
  physical context can't cover the safe space (die-cut sides,
  dark mounting surfaces at the cut edge). Clip is the explicit
  escape hatch; overlap stays the context-free default.
- **Safe-space (quiet zone) clipping invariants** (2026-06-11): the
  quiet zone is unclippable in the digital artifact by construction —
  `white_side ≥ quiet·m` per side (overlap) / quiet zone inside the
  placed symbol (additive) — and the text side carries an explicit
  clamp, `gap ≥ max(round(1.5·white), quiet·m)`, so per-side padding
  can never let typography invade the safe space. The residual risk is
  PHYSICAL: cut/feed jitter (±1mm ≈ ±11px on a QL) can clip a
  quiet zone that touches the cut line. Printer profiles (§3) declare
  cut tolerance; the renderer credits it in the slack math and emits a
  legibility-tier WARNING when quiet-zone slack < tolerance
  ("increase padding or use additive mode") — the cut edge gets no
  intrinsic-margin credit, because the cut is at the label.
- **`--size <N>[px|mm]` — the unit rides the value** (2026-06-11):
  `--size 64px` | `--size 8mm` | `--size 8` (bare = mm, preserving the
  default); one clap value parser expands the suffix into the
  protocol's explicit `{unit, size_px|size_mm}` fields (wire stays
  explicit; terseness is CLI sugar). This retires the redundant
  `--unit px --size-px 64` pair — §1's original example reads
  accordingly. px is integer-only; mm accepts fractions.
- **`--align start|center|end` — slack-axis alignment** (2026-06-11):
  wherever the canvas exceeds the content (fill_to_max batch
  uniformity — center-hardcoded today; tape-width canvases; future
  fixed `--width`), alignment places the content block along the slack
  axis. One logical pair with physical aliases (`left`/`top` → start,
  `right`/`bottom` → end); the layout interprets the axis (horz slack
  is horizontal, vert slack vertical). Default center. Composes with
  the cut-tolerance story: aligning AWAY from the cut edge donates the
  slack to the risky side.
- **`--size-mode exact|snap` — auto-padding is optional**
  (2026-06-11): `exact` (default, the §2 corrected law) holds the
  canvas at the requested size and distributes the lattice remainder
  into auto padding; `snap` treats size as an UPPER BOUND and the
  canvas snaps down to the content lattice — deduced geometry plus
  declared padding floors, remainder omitted (M3-L clip @ ≤64 → a
  60px canvas, no scrap white). Gap clamp and per-side floors apply
  identically in both modes. `exact` for batch uniformity and
  fixed-slot placement; `snap` for the tightest artifact (minimal
  feed; printers whose intrinsic margins make remainder white
  pointless). Snap is the pre-correction snap-down behavior returned
  as an explicit opt-in instead of a default misreading.
- **Combinations are confirmed, not assumed** (2026-06-11): the option
  set validates AS A WHOLE, three tiers — ERROR for infeasible combos,
  always carrying the feasible alternatives (m4-q + 14 chars → the
  feasibility list; 554 + unfittable block; flag + px); WARN for
  legal-but-risky (clip + align toward the cut edge; modules below the
  calibrated legibility floor; gap clamped at its quiet-zone minimum);
  SILENT for confirmed combos, with the resolved parameters echoed in
  the response as the confirmation receipt. Test-side: pairwise
  combinatorial sweep over the option space — golden geometry for
  valid cells (byte-exact under bitmap typography), exact error text
  for invalid, warning presence for risky (the print contract's
  `combination` kind in the op×shell×kind test matrix).
- **Per-side padding, CSS shorthand** (2026-06-11): `--padding`
  accepts `2` | `2,6` | `2,6,4,6` (all / vertical,horizontal /
  top,right,bottom,left — CSS clockwise), parsed by one custom clap
  value parser into `Padding{t,r,b,l}`; the protocol mirrors it as
  serde-untagged `2 | [2,6] | [2,6,4,6]` — one type, one expansion
  rule shared by CLI and wire. Uniform stays the default; asymmetric
  values are the printer-intrinsic-margin escape (leader strip ≠ side
  margins). The deduction generalizes per axis: each side
  independently satisfies `white_side ≥ max(pad_side, quiet·m)`
  (overlap mode), deterministic remainder distribution on top of the
  floors. Lands as a compatible wrapper after the uniform geometry.
- **Offered vs allowed is contract-land** (the ADR-033/034/035
  split): the tool ships print-contract *implementations*; a
  collection descriptor's **render block declares** which contracts +
  named presets the registry offers; the **manifest gates** whether
  `Print` is enabled at all (op×collection grain). Same single-home
  rule as every other render concern — no print capability is decided
  in shell code.

### 9. Scope cut: printers are downstream (2026-06-12)

**This repo delivers px-accurate render payloads; printing them is
someone else's job.** All printer-physical concerns are OUT of scope:
printer profiles (per-side intrinsic margins, cut tolerance,
device-dpi defaults), cut-clipping warnings, and calibration
workflows — every §3/§8 forward-hook to "printer profiles" is hereby
retired unbuilt. This re-asserts ADR-030's own split (render-label =
core operation; deliver-print = a consumer capability) at full
strength. What remains in scope is pure geometry: px-true rendering,
padding modes (incl. clip — the consumer decides whether their device
donates white), `--unit mm --dpi <n>` as plain unit arithmetic with no
device registry behind it, and combination validation over geometric
(not physical) constraints. Device-specific comments in code ("≈
Brother QL") are to be neutralized as encountered.

## Corrections

> **2026-06-11:** §2 as first accepted read `--size` as the *QR symbol
> target* and snapped the symbol down (64 → 63 px output). Hardware
> testing the same day surfaced the intended semantics: **size is the
> exact output canvas**, padding is the floor, and the module size is
> *deduced* from `(size − 2·padding_min) / N`, erroring when the chosen
> QR/payload cannot fit. §2 rewritten above (worked example: 64/2 →
> 42 px symbol, 11 px actual padding, 64 px output); this entry
> preserves the original misreading for audit.

## Open questions / supersession triggers

- **Default DPI per printer** — lives in the printer profile (config);
  enumerate supported models.
- **Raster in core vs CLI** — does `--emit png|pdf` rasterize inside the
  core (portable, heavier WASM) or stay a CLI/server concern (current
  `rsvg-convert` path)? Decide when a non-CLI shell needs raster.
- **Multi-up sheets** — `compose-sheet` (Python-only today) becomes
  "pack M px-true labels into a sheet grid"; layering it on this
  single-label primitive is a follow-up, likely its own scope.
- **`--strict` print-after-land** — confirm it's wanted and define its
  wait/timeout semantics.

## References

- ADR-012 — Part identification (nano-id, statuses)
- ADR-015 — Print-event log
- ADR-017 — Rust core, codec as SSOT
- ADR-019 — Proposal sink (mint → proposal)
- ADR-030 — Multi-tier shells + command protocol
- `tools/printer_test_62mm.py` — the proven pixel-grid render (11.226 px/mm)
- `crates/codec/src/svg.rs`, `crates/codec/src/qr.rs` — current renderer
- `decisions/explorations/operations-catalog.md` §C — print model
