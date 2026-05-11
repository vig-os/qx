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

## Operator docs

Bench-side documentation for technicians using the SPA day-to-day
lives in [`docs/`](docs/):

- [`docs/operator-cheatsheet.md`](docs/operator-cheatsheet.md) —
  one-page lab-floor reference: ID format, daily scan/bind/print
  workflow, Brother QL-820NWBc tape mapping, jam recovery, bug
  reporting. Pin near the printer.
- [`docs/quickstart.md`](docs/quickstart.md) — narrative onboarding
  for a new operator: install the PWA on phone, first scan, first
  bind, first print.

In-app help (inline help icons, dismissable quickstart panel,
printer-friendly cheatsheet route inside the SPA) is tracked in
[issue #8](../../issues/8).

## Tests

```bash
uv sync --group dev
uv run pytest test_labels.py -v
```

The roundtrip suite verifies the critical invariant — **QR-decoded
payload === displayed text === canonical ID** — across every layout
and size combination. Requires `rsvg-convert` (`brew install librsvg`).

## Validators

`crates/validators/` is the shared rule set that both CI and the FE
(via WASM) use to gate writes to the data repo's `registry.csv`. See
[ADR-013](decisions/ADR-013-parts-registry-web-app.md) §"Shared
validation" and [ADR-016](decisions/ADR-016-pr-diff-policy-enforcement.md)
for the policy story.

```bash
# Run the rule-set test suite
cargo test -p part-registry-validators

# Run the legacy Python validators (still parity-tested)
uv run python -m validators /path/to/data-repo/registry.csv
uv run python -m validators /path/to/data-repo/registry.csv --base /path/to/base/registry.csv
```

Rules encoded:

- Header schema and per-row schema (required fields, status enum,
  canonical 14-char ID regex from ADR-012's no-lookalike alphabet).
- Per-status field constraints (`bound` rows must carry `bound_at`;
  `unbound` rows must not carry `type` / `location` / `bound_at`).
- ID uniqueness and sort stability — re-sorting by `id` ascending must
  equal the file, so diffs only show the rows actually changing.
- Status transitions (with `--base`): `unbound → bound`,
  `bound → bound` (rebind), `* → void`. No back-transitions, no
  `void → bound`. New rows must be born `unbound` or `bound`.

Per #35: the diff-vs-base policy CI lives on the **data** repo
(`exo-pet/exopet-registry[-sandbox]`), not on this code repo. The code
repo's `rust.yml` runs unit + conformance tests; the data repo's
workflow runs the same `crates/validators/` binary against each PR.

## Data repos

Per [ADR-019](decisions/ADR-019-proposal-sink-port.md) and #35, code
and data live in separate repositories so the code can stay
open-source while operator data stays scoped to its registry:

| Repo | What | Visibility |
|---|---|---|
| `MorePET/part-registry` (this) | Rust + Python + FE source, ADRs, examples | Public |
| `exo-pet/exopet-registry` | Production registry data (audit-of-record) | Private (planned; currently public until org upgrade) |
| `exo-pet/exopet-registry-sandbox` | Throwaway sandbox for experimentation | Public |

CLI binaries resolve the target data repo from
`PART_REGISTRY__REPO__DATA_REPO_URL` (defaults to the sandbox so a
vanilla `cargo run` never writes to the audit-of-record registry).
The clone lives at `$XDG_DATA_HOME/part-registry/<owner>-<repo>/` —
see `crates/config/src/lib.rs:resolve_data_path`.

## Files

- `mint.py` / `label.py` / `bind.py` — legacy Python CLIs. Parity
  targets for the Rust binaries; deletion gated on operator review.
- `test_labels.py` — pytest roundtrip suite
- `validators/` — legacy Python validators (parity-tested against
  `crates/validators/`)
- `crates/` — Rust workspace (see workspace `Cargo.toml`)
- `web/` — Vite SPA + WASM façade over `crates/codec` + `crates/validators`
- `decisions/` — ADRs and decision log
- `examples/` — reference label renderings used by the parity tests

## Status

- **Phase 1** (this repo): CLI tooling — functional today.
- **Phase 2** (planned): GH Pages SPA + WASM DuckDB + camera scanner
  + PR-driven binds + page-per-label printing. Tracked in
  [issues](../../issues).
