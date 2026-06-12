# ADR-012 — Part identification: nano-id + QR labels with mint-then-bind workflow

- Status: Proposed
- Date: 2026-05-08
- Component / area: Physical part identification (cross-cutting — applies to
  every individual physical instance: sensors, fittings, PT100s, modules,
  cables, sub-assemblies). Tooling under `system-design/parts/`.

## Context

Every physical instance in the project — not just every part type, but every
individual unit — needs a permanent identifier so that calibration history,
batch lineage, location, and incident reports can be attached to *this
specific PT100*, not just "a PT100." This becomes load-bearing the moment
two parts of the same type diverge (one was recalibrated, one wasn't; one
was used in a failed run, one wasn't), which happens early.

Three constraints shape the scheme:

1. **Permanence beats convenience.** A label engraved on a part in 2026
   needs to resolve in 2036, after the registry tooling, the lookup site,
   and possibly the company name have been replaced at least once. Anything
   we encode into the physical label that depends on a server, a domain,
   or a schema is a future liability.
2. **Mint-then-bind, not type-known-at-mint.** The working pattern is to
   batch-print or batch-engrave a roll of generic stickers/tags first, then
   slap them on parts as parts arrive, and *register* the binding
   (this ID ↔ this part type ↔ this location) afterwards. This rules out
   any scheme where the ID encodes the part type.
3. **Two failure modes for the label**: (a) QR scratched / oily / out of
   camera range — human-readable text becomes the fallback; (b) human
   text smudged / engraving worn — QR scan recovers it. Same canonical ID
   in both, no second registry lookup.

## Alternatives considered

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Sequential numeric IDs (`PART-00001`) | Human-readable, compact | Requires central counter authority; doesn't survive distributed batch minting; no offline mint workflow | Rejected: incompatible with batched mint-then-bind |
| Type-prefixed IDs (`PT-00042`) | Self-describing on the label | Requires part type known at mint time; type can change post-bind (re-purposed PT100); still needs central counter per type | Rejected: mint-then-bind blocks this |
| UUID v4 (32 hex, dashes) | Standard, library support everywhere | 36 chars including dashes — too long for engraving on small parts; hex alphabet contains `0`/`O` ambiguity at small sizes | Rejected: form factor |
| URL-in-QR (`https://parts.exoma.local/<id>`) | Scan opens a phone-tap-to-lookup page | Couples physical labels to a domain decision made today; if the domain or service ever changes, every existing label points to nothing; QR is ~30 % denser too | Rejected: violates permanence constraint. Modern browsers can scan camera QR → lookup via `getUserMedia`, so the URL in the label is unnecessary. |
| Nano-id, no-lookalike alphabet, ID-only QR | Compact (14 chars), collision-free at lab scale, decoupled from any service or domain, works offline | Plain-text payload means the user has to use a scan-aware app to resolve it (cannot be tapped open from a generic camera) | **Chosen** |

## Decision

**Fourteen-character nano-id** drawn from the 32-character no-lookalike alphabet
`23456789ABCDEFGHJKMNPQRSTUVWXYZ` (Crockford-style: no `0`/`O`, no `1`/`I`/`L`).

- **Canonical form** (QR payload, registry primary key):
  the 14-char raw string, e.g. `K7M3PQ9RT5VAXY`. The QR encodes all 14
  characters — no truncation.
- **Human-readable form**: displayed text is a **prefix** of the canonical
  ID, laid out in a format chosen by size (see §Label geometry below).
  The displayed prefix is unambiguous at small sizes; the full canonical
  is always recoverable by QR scan.
- **QR encodes only the ID** — no URL, no metadata. Lookup is the job of
  a separate, replaceable web app behind authentication (deferred to a
  later phase).
- **Workflow**: mint a batch of unbound IDs → print/engrave the labels →
  apply to parts → bind (register what each ID is on) afterwards.

### ID space and collision math

Birthday-bound collision probability `P ≈ N² / (2·32^L)` for `N` minted IDs,
length `L`:

| Length | Space | 1k parts | 10k | 100k | 1M |
|---|---|---|---|---|---|
| 8 (display prefix) | 1.1×10¹² | ~0 | ~0 | 0.45 % | 36 % |
| **14 (canonical)** | **1.2×10²¹** | **~0** | **~0** | **~0** | **~0** |

Fourteen chars is collision-free even at 10⁷ lifetime IDs. Display prefixes
of 8 chars have a collision once per ~200k parts at the birthday bound —
triaged by the operator (physical part in hand, status context, QR scan
resolves uniquely). See §Collision triage below.

### Collision triage

The 8-char display prefix is not required to be unique. Collisions are
resolved by the operator:

1. **Bind context**: the operator is binding an unbound ID. If the typed
   prefix resolves to multiple matches, the UI shows all candidates with
   status + metadata. The operator picks. QR scan resolves uniquely.
2. **Lookup context**: the operator is finding a part. Multiple matches
   are shown; the operator disambiguates by physical context.
3. **Status guard**: binding an already-bound ID warns regardless of
   collision — this is a status transition check, not a disambiguation.

The collision rate at 8 chars (32⁸ = 1.1×10¹²) is negligible for our
scale. At 10k parts, P(collision) ≈ 0.00005. Triage is a UX concern,
not a correctness one.

### Font: Consolas monospace

**Consolas** is the label font. Measured via Pillow `textbbox()` against
the actual TTF (`~/Downloads/Consolas/consolas.ttf`):

- Every character in the no-lookalike alphabet has identical advance width:
  **0.55 × font_size** (true monospace, no variation across the 32 chars).
- x-height / cap-height ratio: **0.56** — highest among the candidates
  (Courier New 0.52, SF Mono 0.55, IBM Plex Mono 0.54, Roboto Mono 0.55).
- Higher x-height ratio = more legible per mm of font size.

This makes Consolas the best choice for small-print thermal labels:
maximum legibility at minimum font size, and predictable horizontal fit
(exact 0.55 ratio, no per-character variance).

See `tools/layout_analysis.py` for the measurement script and full data.

### Label geometry

A label is two **equal-size square blocks**: the QR block and the
text block. Each block is `size × size`. The label is therefore
fixed at aspect ratio 2:1 or 1:2 — nothing else.

| Layout | Arrangement | Label dims | Use |
|---|---|---|---|
| `horz` | QR left, text right | `2*size × size` | Default; flat horizontal surfaces |
| `vert` | QR top, text below | `size × 2*size` | Narrow vertical strips: PCB silkscreen channels, cable runs |
| `flag` | `horz` mirrored across a cable wrap zone | `(4*size + π·OD·1.1) × size` | Cable flag tags; double-sided readable when wrapped |

A **single `--size <mm>`** parameter (the short side) controls
everything. The text block uses one of three formats, selected by
size tier (or overridden explicitly):

| Format | Rows | Chars displayed | Font tier at 8mm | Use |
|---|---|---|---|---|
| `4/4` | 2 | 8 | 3.35mm (easy) | Default; sizes ≤ 8mm |
| `4/4/4` | 3 | 12 | 2.16mm (comfortable) | Sizes ≥ 10mm |
| `5/5/4` | 3 | 14 | 2.16mm (comfortable) | Sizes ≥ 12mm, full canonical |

Format selection is driven by legibility: 2-row formats give ~35% bigger
font than 3-row formats at the same size. The jump from 2→3 rows is the
big penalty. Below 8mm, 4/4 is the only format that reaches "easy" legibility.
At 10mm+, 4/4/4 reaches "easy" and shows 50% more chars.

**Horizontal fit is exact for 4/4**: 4 chars × 0.55 advance ratio = 2.2,
which equals the vertical divisor for 2 rows (2f + 0.2f = 2.2f). The
4-char row fills the square precisely with zero horizontal margin. This
is not a coincidence — the math works out because Consolas is true
monospace at 0.55.

**2/2 and 3/3 formats are strictly dominated by 4/4**: same font size
(same vertical constraint for 2 rows), fewer characters. No reason to use
them. 4/4 always wins for 2-row layouts.

**Size-based format recommendation** with warning system:

| Size range | Recommended | Warn if using |
|---|---|---|
| < 5mm | `4/4` | `4/4/4` (font < 1.5mm, below "readable") |
| 5–7mm | `4/4` | `4/4/4` (font < 1.9mm, below "comfortable") |
| 8mm | either | none (4/4 font=3.35mm easy, 4/4/4 font=2.16mm comfortable) |
| 10mm+ | `4/4/4` or `5/5/4` | `4/4` (font > 4mm, overkill, wastes space) |

`--tape pt-9 | pt-12 | pt-18 | pt-24 | pt-36` is shorthand for a Brother
P-touch printable height (sets `--size` to 6.5 / 9 / 12 / 18 / 28 mm).

All templates emit SVG with explicit `mm` units (`width`/`height` in mm,
`viewBox` in mm so 1 user unit = 1 mm) so engraver software (LightBurn,
EZCAD) and label printers receive true physical dimensions without
rescaling.

### Tight-fit validation: SiPM module backside

The SiPM module backside has two SAMTEC `ST4-40-1.00-L-D-P-TR`
connectors at 12.80 mm center-to-center (≈ 10.4 mm inner-to-inner). The
available silkscreen window between them is roughly **6–8 mm wide ×
~17 mm long**. With the equal-block geometry, a vert label of size
`s` occupies `s × 2s`, so:

| `--size` | Label (mm) | QR module (mm) | Notes |
|---|---|---|---|
| 6 | 6 × 12 | 0.21 | Fits 6 mm-wide window with 5 mm vertical headroom |
| 8 | 8 × 16 | 0.28 | Fits 8 mm-wide window — recommended |

`mint.py --layout vert --size 8` is the recommended SiPM-backside
configuration. If the PCB layout cannot give 8 mm clear width between
connectors, fall back to fiber-laser-marked metal tag on the housing
plus a silkscreen of the 4/4 text block alone (no QR) on the PCB.

## Rationale

**Permanence first.** ID-only QR means the label is a forever-stable
artifact, independent of any domain, server, or schema decision. Every
other alternative either coupled the label to today's infrastructure
(URL-in-QR) or to today's organizational structure (sequential numbering,
type prefixes). When the lookup app is replaced — and it will be — labels
keep working.

**14-char canonical, 8-char display.** The original 12-char design was
conservative — the full ID was displayed as 4/4/4 to guarantee uniqueness
on the human form. Reconsideration with measured data shows:

- Micro QR M4 at error correction M holds 14 alphanumeric chars (2-char
  headroom was unused). Going to 14 chars costs nothing — same QR symbol
  size (17×17 modules, M4 footprint).
- Collision space jumps from 32¹² ≈ 1.2×10¹⁸ to 32¹⁴ ≈ 1.2×10²¹
  (3 orders of magnitude).
- Displaying only 8 chars (4/4) doubles the font size vs 4/4/4 at the
  same label size (2-row vs 3-row vertical constraint).
- Collisions at 8 chars are negligible at our scale (P ≈ 0.00005 at 10k
  parts) and triaged by operator context (physical part in hand, status,
  QR scan resolves uniquely).

**Consolas over Courier.** Courier New was the baseline choice (widely
available, no-lookalike friendly). Measurement shows Consolas has a
higher x-height ratio (0.56 vs 0.52) and true monospace advance (0.55
per character, zero variance). Courier New's wider characters (0.62
advance ratio) actually cause horizontal overflow at 4/4 on small sizes.
Consolas fits exactly.

**Mint-then-bind matches the physical workflow.** Stickers and engraved
tags are fungible until applied. Printing a roll of 200 unbound IDs and
binding them as parts arrive is operationally simpler than minting an ID
each time a part is registered, and it lets us pre-engrave bare-metal
tags in batches at the laser without knowing the part list in advance.

**No-lookalike alphabet.** A label that gets read aloud over a workbench
or transcribed into a notebook needs an alphabet without `0`/`O`/`1`/`I`/`L`
ambiguity. The 32-character Crockford-style set is unambiguous, fits
power-of-two ID-space math cleanly, and survives engraving wear better
than a confusable alphabet.

**Equal-block geometry is the KISS choice.** Allowing arbitrary widths,
heights, font sizes, and module sizes per layout creates a configuration
matrix with bad combinations (font too large for height, module too
small for the engraver). Constraining the label to two equal squares
collapses all of that to a single `--size` parameter; every derived
dimension follows. There is exactly one degree of freedom for the
operator to set, and it directly maps to the only physical constraint
that matters (the short side of the available surface).

## Consequences

- **Tooling commitment** (phase 1, this ADR):
  - `registry.csv` — append-only canonical record:
    `id, status, minted_at, batch, bound_at, type, description, vendor,
    part_number, location, notes`. Status is `unbound`, `bound`, or `void`.
  - `mint.py` — batch-mint N IDs, append unbound rows,
    emit per-ID SVG labels in `vert`, `horz`, or `flag` layout. Single
    `--size` parameter (or `--tape` shorthand) controls all geometry.
    ID length is 14 chars.
  - `label.py` — render SVG labels. Text format selectable via
    `--format 4/4 | 4/4/4 | 5/5/4` (default: auto by size tier).
    Font is Consolas. Warns when the chosen format gives sub-optimal
    legibility for the size.
  - `bind.py` — flip status `unbound → bound`, fill metadata. Accepts
    full 14-char ID or any prefix ≥ 8 chars; on collision, prints all
    matches and refuses to bind without disambiguation.
  - Python deps: `nanoid`, `segno`. Added to `pyproject.toml`.
- **Web app deferred** (phase 2): auth-walled lookup/management site
  with in-browser QR scanning via `getUserMedia` + a JS QR decoder
  (e.g. `@zxing/browser`). Source of truth remains the CSV until scale
  forces SQLite + a small backend; CSV stays as the export format
  regardless.
- **IDs are never reused.** A spoiled or destroyed sticker becomes a
  `void` row in the registry; the ID is retired but auditable. This
  matters if a label is ever found loose and needs to be traced.
- **Process discipline.** Bind step must happen *the same shift* the part
  is installed. Drift between physical install and registry entry is the
  failure mode that destroys the trace. Cheap mitigation: the bind tool
  is a phone-friendly form (phase 2); for now, a CLI on the workshop
  laptop.
- **Engraving-vs-printing parity.** Same SVG template, same canonical ID;
  the choice between sticker and laser-engraved tag is purely a
  durability / surface decision, not a registry decision. Engravings on
  parts exposed to coolant, solvents, or heat (per ADR-002 / ADR-003 /
  ADR-004 environment); stickers on everything else.
- **Phase 2 trigger.** Move from CSV to SQLite + auth-walled web app
  when (a) the registry exceeds ~2 000 rows and CSV diff/merge becomes
  painful, *or* (b) more than one person is binding parts concurrently,
  *or* (c) phone-based lab-floor lookup becomes the primary workflow.

## Open questions / supersession triggers

- **Engraver toolchain validation.** SVG-with-mm units is widely supported
  but not universal. If the chosen laser/marking system requires DXF or
  G-code, `mint.py` gains an exporter. Not blocking for phase 1
  (sticker-printable templates work today).
- **Tamper / authenticity.** This ADR does not address whether labels
  need to be tamper-evident or cryptographically bound to the part (e.g.
  for regulated-device traceability). If EXOPET parts ever become subject
  to UDI / EUDAMED traceability requirements, this ADR is superseded by
  one that integrates with that scheme.
- **Auth model for phase 2.** Tailscale-fronted vs OIDC vs basic auth is
  a separate decision when the web app is built; out of scope here.
- **Font availability on target systems.** Consolas is not shipped with
  Linux or iOS. SVG text fallbacks to `monospace` if Consolas is not
  installed. The QR block is unaffected. For engraving (SVG → raster →
  G-code), the font is baked into the raster at render time — no runtime
  font dependency. For printer drivers that substitute fonts, the QR
  block is the load-bearing element.

## Corrections

> **2026-06-11:** the alphabet was described as "32-character" with
> power-of-two ID-space math — it is in fact **31 characters**
> (8 digits + 23 letters), and 31 is not a power of two. True Crockford
> base32 keeps `0`/`1` (32 symbols) and decode-maps lookalikes
> (`O→0`, `I/L→1`); ours is *stricter* — the lookalikes are dropped
> entirely, making the set 31-ary. Capacity math: 31¹⁴ ≈ 7.6×10²⁰
> (~2⁶⁹), birthday-safe past 10⁹ ids; mint is additionally
> uniqueness-checked, so collision safety is structural, not only
> probabilistic.

## Refinements

> **2026-06-11:** constraint re-examined when owned bitmap typography
> (ADR-031 §8) made `0/O`-class glyphs visually distinguishable on
> labels: **the exclusion stays.** Typography fixes only the reading
> channel — spoken, handwritten, and retyped ids inherit ambiguity no
> font can remove. Structurally, exclusion is also what keeps forgiving
> input possible (`O` can never be valid, so it is always detectably a
> typo); admitting `0` and `O` as distinct symbols would convert every
> confusion into a potential wrong-but-valid resolution. The gain would
> be +4.4% entropy/char — provably unneeded. Residual in-alphabet pairs
> (`8/B`, `5/S`, `2/Z`, `6/G`) are handled by the owned bitmap glyphs
> (ADR-031 §8) — alphabet excludes the catastrophic, typography
> sharpens the residual.

## References

- `label.py` — SVG label renderer, text block geometry.
- `mint.py` — ID generator, registry appender.
- `tools/layout_analysis.py` — measurement script: Pillow textbbox
  against Consolas TTF, computes font size, horizontal fit, utilization,
  and legibility tiers for all (format, font, size) combinations.
- nano-id specification: <https://github.com/ai/nanoid>
- segno (Python QR): <https://segno.readthedocs.io>
- Crockford base32 (no-lookalike alphabet rationale):
  <https://www.crockford.com/base32.html>
