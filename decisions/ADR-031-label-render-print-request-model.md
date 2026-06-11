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

A QR symbol of `N×N` modules (quiet zone included) is rendered with an
**integer module pixel size**:

```
module_px = floor(target_qr_px / N)        # N = modules incl. quiet zone
symbol_px = N * module_px                   # ⇒ symbol_px % N == 0, always
```

Every module is therefore an identical whole number of device pixels —
no fractional edges, no malformed dots. `module_px ≥ 1` is required (if
the requested size can't fit one pixel per module, the render errors with
a minimum-size hint rather than producing an unscannable symbol). This
generalizes the pixel-grid discipline already proven in
`tools/printer_test_62mm.py` into a first-class codec mode.

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
