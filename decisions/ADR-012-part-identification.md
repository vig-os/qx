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
| Nano-id, no-lookalike alphabet, ID-only QR | Compact (12 chars), collision-free at lab scale, decoupled from any service or domain, works offline | Plain-text payload means the user has to use a scan-aware app to resolve it (cannot be tapped open from a generic camera) | **Chosen** |

## Decision

**Twelve-character nano-id** drawn from the 32-character no-lookalike alphabet
`23456789ABCDEFGHJKMNPQRSTUVWXYZ` (Crockford-style: no `0`/`O`, no `1`/`I`/`L`).

- **Canonical form** (QR payload, registry primary key, label display):
  the 12-char raw string, e.g. `K7M3PQ9RT5VA`. The same 12 chars appear
  in the QR and on the human-readable text — no truncation, no
  prefix-as-nickname.
- **Human-readable form**: the canonical 12 chars laid out as **three
  rows of four** (4/4/4 grouping, Courier monospace), e.g.
  ```
  K7M3
  PQ9R
  T5VA
  ```
  Reads aloud cleanly in three groups; transcribes onto a workbench
  notebook without ambiguity; matches the QR character-for-character.
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
| 8 | 1.1×10¹² | ~0 | ~0 | 0.45 % | 36 % |
| **12** | **1.2×10¹⁸** | **~0** | **~0** | **~0** | **~0** |

Twelve chars is collision-free even at 10⁶ lifetime IDs — comfortable
margin for any plausible expansion of the project.

### Label geometry

A label is two **equal-size square blocks**: the QR block and the
4/4/4 text block. Each block is `size × size`. The label is therefore
fixed at aspect ratio 2:1 or 1:2 — nothing else.

| Layout | Arrangement | Label dims | Use |
|---|---|---|---|
| `horz` | QR left, text right | `2*size × size` | Default; flat horizontal surfaces |
| `vert` | QR top, text below | `size × 2*size` | Narrow vertical strips: PCB silkscreen channels, cable runs |
| `flag` | `horz` mirrored across a cable wrap zone | `(4*size + π·OD·1.1) × size` | Cable flag tags; double-sided readable when wrapped |

A **single `--size <mm>`** parameter (the short side) controls
everything: QR module = `size / (n_modules + 2·border) ≈ size / 29`;
font size = `(size·0.8) / 3.6`. No separate width / height / module / font
flags — the geometry is fully determined by one number.

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
plus a silkscreen of the 4/4/4 text block alone (no QR) on the PCB.

## Rationale

**Permanence first.** ID-only QR means the label is a forever-stable
artifact, independent of any domain, server, or schema decision. Every
other alternative either coupled the label to today's infrastructure
(URL-in-QR) or to today's organizational structure (sequential numbering,
type prefixes). When the lookup app is replaced — and it will be — labels
keep working.

**Full 12 chars on the label, always.** An earlier sketch displayed
only the first 8 chars on the label as a `XXXX-XXXX` "nickname" (with
the canonical 12 in the QR), borrowing the git short-SHA pattern. The
final design displays all 12 as 4/4/4 because it removes a class of
ambiguity (no rare-but-possible prefix collisions on the human form),
makes the label match the QR character-for-character, and the cost is
only ~50 % more text-block area — which the equal-block geometry
absorbs cleanly. The visual disambiguation property — that the
technician holds the part — is preserved as defense-in-depth but is
no longer load-bearing for the design.

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
  - `system-design/parts/registry.csv` — append-only canonical record:
    `id, status, minted_at, batch, bound_at, type, description, vendor,
    part_number, location, notes`. Status is `unbound`, `bound`, or `void`.
  - `system-design/parts/mint.py` — batch-mint N IDs, append unbound rows,
    emit per-ID SVG labels in `vert`, `horz`, or `flag` layout. Single
    `--size` parameter (or `--tape` shorthand) controls all geometry.
  - `system-design/parts/bind.py` — flip status `unbound → bound`, fill
    metadata. Accepts full 12-char ID or any prefix ≥ 8 chars; on
    collision, prints all matches and refuses to bind without
    disambiguation.
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

## References

- `system-design/parts/` — registry CSV and tooling.
- nano-id specification: <https://github.com/ai/nanoid>
- segno (Python QR): <https://segno.readthedocs.io>
- Crockford base32 (no-lookalike alphabet rationale):
  <https://www.crockford.com/base32.html>
