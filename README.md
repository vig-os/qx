# part-registry

Per-instance physical identification for hardware parts: nano-id canonical
IDs, QR labels, mint-then-bind workflow. Designed for permanence — labels
outlive servers and orgs.

> Public to bootstrap quickly. Will move private once the registry holds
> operational data — though the design intent (per [ADR-013](decisions/ADR-013-parts-registry-web-app.md))
> is that registry data — hardware IDs + locations + types — is generally
> not commercially sensitive, so public may end up being the steady state.

## ID scheme

12-character nano-id from `23456789ABCDEFGHJKMNPQRSTUVWXYZ`
(no `0`/`O`/`1`/`I`/`L`). Same 12 chars in the QR and on the label, displayed
as 3 rows × 4. See [`decisions/ADR-012`](decisions/ADR-012-part-identification.md)
for rationale; [`ADR-013`](decisions/ADR-013-parts-registry-web-app.md) for
the planned web app architecture.

## Workflow (CLI)

Three verbs, three scripts:

```bash
# 1. mint — create IDs, append to registry. No labels.
uv run mint.py --count 50 --batch B-2026-05-sdmd

# 2. label — render SVG labels for IDs already in the registry
uv run label.py --batch B-2026-05-sdmd --layout horz --tape dk-12
uv run label.py --id K7M3PQ9RT5VA --layout vert --size 8
uv run label.py --status unbound --layout flag --size 11 --cable-od 6
# Each successful render also appends a row per ID to print_log.csv
# (audit trail; see ADR-015). Pass --no-log for ad-hoc renders that
# aren't real prints, --operator <name> to override $USER.

# 3. bind — attach an ID to a real part (full ID or ≥ 8-char prefix)
uv run bind.py K7M3PQ9RT5VA \
    --type "PT100 1/3 DIN class B, 4-wire" \
    --vendor "TC Direct" --part-number "402-141" \
    --location "sdmd_v2 / cooling-loop / supply-T"
```

## Layouts

A label is two equal-size square blocks: QR + 4/4/4 text. One parameter
(`--size <mm>`) sets the short side; everything else is derived.

| `--layout` | Arrangement | Label dims | Use |
|---|---|---|---|
| `horz` | QR left, text right | `2·size × size` | Default flat surfaces |
| `vert` | QR top, text below | `size × 2·size` | Narrow strips, PCB silkscreen, cables |
| `flag` | `horz` mirrored across wrap zone | `(4·size + π·OD·1.1) × size` | Cable wrap tags, double-sided |

`--tape <preset>` is shorthand for the printable-height of common label tapes:

| Preset | Printer family | Roll | Short-side (mm) |
|---|---|---|---|
| `pt-9` … `pt-36` | Brother P-touch (TZe) | PT-D series | 6.5 / 9 / 12 / 18 / 28 |
| `dk-12` / `-29` / `-38` / `-62` | Brother QL (DK) | QL-820NWBc | 10 / 25 / 33 / 56 |

See [`examples/gallery.png`](examples/gallery.png) for renderings at
common sizes.

## Printing on Brother QL-820NWBc

The QL-820NWBc has a hardware auto-cut unit between print jobs / pages
and supports **AirPrint** over Wi-Fi. The simplest workflow:

```bash
# Render labels at the right tape size:
uv run label.py --batch B-2026-05-sdmd --layout horz --tape dk-12

# Convert SVGs to single-page PDFs (printer auto-cuts between pages/jobs):
cd labels/B-2026-05-sdmd-horz-sdk-12/
for f in *.svg; do rsvg-convert -f pdf "$f" -o "${f%.svg}.pdf"; done

# Print all of them — one cut per file:
lp -d Brother_QL_820NWBc *.pdf
```

To find your printer name: `lpstat -p`. If the Brother isn't listed,
add it via System Settings → Printers (macOS) — it will be discovered
on the LAN automatically (Bonjour). On iOS/iPadOS, AirPrint discovers
it from the system print sheet directly; no app install needed.

For one-off ad-hoc printing without the CLI: open any `.svg` in a viewer
or browser, Cmd+P, pick the Brother in the dialog.

## Tests

```bash
uv sync --group dev
uv run pytest test_labels.py -v
```

The roundtrip suite verifies the critical invariant — **QR-decoded
payload === displayed text === canonical ID** — across every layout
and size combination. Requires `rsvg-convert` (`brew install librsvg`).

## Validators

`validators/` is the shared rule set that both CI and the FE (via Pyodide)
use to gate writes to `registry.csv`. Pure stdlib, no external deps —
see [ADR-013](decisions/ADR-013-parts-registry-web-app.md) §"Shared
validation" for why.

```bash
# Local: validate the working-copy registry
uv run python -m validators registry.csv

# Local: also enforce the diff-vs-base rules (status transitions, etc.)
uv run python -m validators registry.csv --base /path/to/main/registry.csv

# Run the rule-set test suite
uv run pytest validators/test_validators.py -v
```

Rules encoded:

- Header schema and per-row schema (required fields, status enum,
  canonical 12-char ID regex from ADR-012's no-lookalike alphabet).
- Per-status field constraints (`bound` rows must carry `bound_at`;
  `unbound` rows must not carry `type` / `location` / `bound_at`).
- ID uniqueness and sort stability — re-sorting by `id` ascending must
  equal the file, so diffs only show the rows actually changing.
- Status transitions (with `--base`): `unbound → bound`,
  `bound → bound` (rebind), `* → void`. No back-transitions, no
  `void → bound`. New rows must be born `unbound` or `bound`.

CI runs the same module on every PR via `.github/workflows/validate.yml`,
fetching the merge-base copy of `registry.csv` for the diff rules.

## Files

- `mint.py` — generate IDs, append rows. **No SVGs.**
- `label.py` — render SVG labels for IDs already in the registry.
  Selectable by `--id`, `--batch`, or `--status`.
- `bind.py` — flip `unbound → bound`, fill metadata
- `test_labels.py` — pytest roundtrip suite
- `validators/` — shared CI + FE registry rule set (stdlib only)
- `registry.csv` — canonical record (sorted by ID; see ADR-013 for the
  sort-stability invariant)
- `print_log.csv` — append-only audit trail of every label print
  (sorted by `printed_at`; one row per ID per print event; see
  [ADR-015](decisions/ADR-015-print-event-log.md))
- `decisions/` — ADRs and decision log
- `examples/` — reference renderings at common sizes
- `labels/` — generated SVG/PDF labels (gitignored)

## Status

- **Phase 1** (this repo): CLI tooling — functional today.
- **Phase 2** (planned): GH Pages SPA + WASM DuckDB + camera scanner
  + PR-driven binds + page-per-label printing. Tracked in
  [issues](../../issues).
