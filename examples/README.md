# Label examples

Reference renderings at common sizes, generated with the legacy Python
`mint.py` + `label.py` (retired per ADR-017 step 9; the Rust `mint` /
`label` binaries in `crates/cli` are parity-tested against their output).
Both SVG (engraver/printer-ready, mm-native) and PNG (quick inspection)
are included.

See [`gallery.png`](gallery.png) for a single-image overview.

The IDs in these examples were minted purely for the gallery and are
**not** in `registry.csv`.

## Variants

Naming convention: `<layout>-<spec>` where `<spec>` is `s<mm>`,
`pt-<tape>`, or `d<cable_OD>`.

| Folder | Layout | `--size` / preset | Label dims |
|---|---|---|---|
| `horz-s11/` | horz | `--size 11` (default) | 22 × 11 mm |
| `horz-pt12/` | horz | `--tape pt-12` | 18 × 9 mm |
| `horz-pt24/` | horz | `--tape pt-24` | 36 × 18 mm |
| `vert-s6/` | vert | `--size 6` | 6 × 12 mm |
| `vert-s8/` | vert | `--size 8` | 8 × 16 mm |
| `vert-pt12/` | vert | `--tape pt-12` | 9 × 18 mm |
| `vert-pt24/` | vert | `--tape pt-24` | 18 × 36 mm |
| `flag-d4/` | flag | `--cable-od 4 --size 11` | 57.8 × 11 mm |
| `flag-d8/` | flag | `--cable-od 8 --size 11` | 71.6 × 11 mm |
| `flag-d12/` | flag | `--cable-od 12 --size 11` | 85.5 × 11 mm |

Flag width = `4 × size + π × cable_OD × 1.1` (10 % overlap is the
wrap-zone tolerance).
