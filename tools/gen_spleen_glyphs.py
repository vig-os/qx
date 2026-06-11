#!/usr/bin/env -S uv run
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Generate crates/codec/src/glyphs_spleen.rs from the Spleen BDF faces.

ADR-031 §8 typography verdict (2026-06-11): the px-true id-text uses
Spleen multi-cell bitmap typography — cells 6x12 / 8x16 / 12x24 /
16x32 / 32x64 vendored as const bit tables covering exactly the
31-char nano14 alphabet (0/O/1/I/L excluded at the id-scheme level,
ADR-012). Per cell the glyphs are cropped to the ALPHABET-WIDE cap-ink
band: the union ink bounding rows over all 31 glyphs, so every glyph
in a cell shares one stored top edge and one stored height and the
renderer lays rows out from real ink instead of em-box guesses.

The output is deterministic (fixed cell order, alphabet order, no
timestamps): re-running the script on the same Spleen release yields a
byte-identical file. `--check` regenerates to memory and diffs against
the checked-in file — the CI drift gate for the vendored tables.

Spleen 2.1.0 is (c) 2018-2024 Frederic Cambus, BSD-2-Clause; the
notice rides in the generated file and the SOUP inventory carries the
dependency row (soup/inventory.toml).
"""

from __future__ import annotations

import argparse
import glob
import os
import sys
from pathlib import Path

ALPHABET = "23456789ABCDEFGHJKMNPQRSTUVWXYZ"

# Nominal cell heights (the §8 selection unit) and the BDF face names.
# The 5x8 face exists upstream but is excluded by the verdict — the
# first-party 5x7 table (crates/codec/src/glyphs.rs) is the floor.
CELLS = [
    ("6x12", 12),
    ("8x16", 16),
    ("12x24", 24),
    ("16x32", 32),
    ("32x64", 64),
]

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_OUT = REPO_ROOT / "crates" / "codec" / "src" / "glyphs_spleen.rs"

LICENSE_NOTICE = """\
Copyright (c) 2018-2024, Frederic Cambus
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions
are met:

  * Redistributions of source code must retain the above copyright
    notice, this list of conditions and the following disclaimer.

  * Redistributions in binary form must reproduce the above
    copyright notice, this list of conditions and the following
    disclaimer in the documentation and/or other materials provided
    with the distribution.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS
FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE
COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT,
INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING,
BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES;
LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN
ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
POSSIBILITY OF SUCH DAMAGE.\
"""


def find_font_dir(cli_arg: str | None) -> Path:
    """Resolve the directory holding the spleen-*.bdf faces.

    Precedence: --font-dir, then $SPLEEN_BDF_DIR, then a glob over the
    local nix store for a spleen-2.1.0 output. The resolved path is an
    INPUT only — it never appears in the generated file (provenance is
    recorded as name + version, per the ADR-031 §8 SOUP note).
    """
    candidates: list[Path] = []
    if cli_arg:
        candidates.append(Path(cli_arg))
    env = os.environ.get("SPLEEN_BDF_DIR")
    if env:
        candidates.append(Path(env))
    candidates.extend(
        Path(p) for p in sorted(glob.glob("/nix/store/*-spleen-2.1.0/share/fonts/misc"))
    )
    for c in candidates:
        if (c / "spleen-6x12.bdf").is_file():
            return c
    sys.exit(
        "error: no Spleen 2.1.0 BDF directory found; pass --font-dir or set "
        "SPLEEN_BDF_DIR (e.g. $(nix build --print-out-paths nixpkgs#spleen)"
        "/share/fonts/misc)"
    )


def parse_bdf(path: Path) -> tuple[dict[int, dict], int, int]:
    """Minimal BDF reader: per-encoding DWIDTH/BBX/BITMAP plus the font
    ascent/descent. Mirrors the validated prototype parser
    (/tmp/spleen_v2_labels.py at the time of the verdict)."""
    glyphs: dict[int, dict] = {}
    cur: dict | None = None
    bits: list[str] = []
    in_bitmap = False
    ascent = descent = 0
    for line in path.read_text(errors="replace").splitlines():
        tokens = line.split()
        if not tokens:
            continue
        key = tokens[0]
        if key == "FONT_ASCENT":
            ascent = int(tokens[1])
        elif key == "FONT_DESCENT":
            descent = int(tokens[1])
        elif key == "ENCODING":
            cur = {"enc": int(tokens[1])}
        elif key == "DWIDTH" and cur is not None:
            cur["dwidth"] = int(tokens[1])
        elif key == "BBX" and cur is not None:
            cur["w"], cur["h"], cur["xoff"], cur["yoff"] = map(int, tokens[1:5])
        elif key == "BITMAP":
            in_bitmap, bits = True, []
        elif key == "ENDCHAR":
            if cur is not None and "enc" in cur:
                cur["bits"] = bits
                glyphs[cur["enc"]] = cur
            cur, in_bitmap = None, False
        elif in_bitmap:
            bits.append(line.strip())
    return glyphs, ascent, descent


def build_cell(font_dir: Path, face: str) -> dict:
    """Render each alphabet glyph onto the full (ascent+descent) cell,
    compute the alphabet-wide cap-ink band, and crop every glyph to it.

    Row encoding matches the first-party 5x7 table convention: bit
    `dwidth - 1` is the LEFTMOST column, so the binary literals read
    like the glyph.
    """
    glyphs, ascent, descent = parse_bdf(font_dir / f"spleen-{face}.bdf")
    height = ascent + descent
    full: dict[str, tuple[int, list[int]]] = {}
    for ch in ALPHABET:
        g = glyphs[ord(ch)]
        dwidth = g["dwidth"]
        rows = [0] * height
        top = ascent - g["yoff"] - g["h"]
        for ry, hexrow in enumerate(g["bits"]):
            value, nbits = int(hexrow, 16), len(hexrow) * 4
            acc = 0
            for rx in range(g["w"]):
                if (value >> (nbits - 1 - rx)) & 1:
                    acc |= 1 << (dwidth - 1 - (g["xoff"] + rx))
            rows[top + ry] = acc
        full[ch] = (dwidth, rows)
    band_top = min(
        min(i for i, r in enumerate(rows) if r) for _, rows in full.values()
    )
    band_bot = (
        max(max(i for i, r in enumerate(rows) if r) for _, rows in full.values()) + 1
    )
    cropped = {
        ch: (dwidth, rows[band_top:band_bot]) for ch, (dwidth, rows) in full.items()
    }
    ink = sum(
        bin(r).count("1") for _, rows in cropped.values() for r in rows
    )
    return {"band": band_bot - band_top, "glyphs": cropped, "ink": ink}


def emit(cells: dict[int, dict]) -> str:
    """Render the full glyphs_spleen.rs source. Deterministic: fixed
    cell order, alphabet order, no timestamps or input paths."""
    doc_license = "\n".join(
        f"//! {line}".rstrip() for line in LICENSE_NOTICE.splitlines()
    )
    cell_refs = ", ".join(f"&CELL_{c}" for c in sorted(cells))
    out: list[str] = []
    out.append(
        f"""\
//! Spleen multi-cell bitmap glyph tables — the ADR-031 §8 typography
//! verdict (2026-06-11). Cells 6x12 / 8x16 / 12x24 / 16x32 / 32x64
//! vendored as const bit tables covering exactly the nano14 alphabet
//! (31 chars; `0/O/1/I/L` are excluded at the id-scheme level,
//! ADR-012). Each cell is cropped to its ALPHABET-WIDE cap-ink band —
//! the union ink bounding rows over all 31 glyphs — so every glyph in
//! a cell shares one top edge and one stored height ([`SpleenCell::band`])
//! and the px renderer lays text rows out from real ink, not em-box
//! guesses. The first-party 5x7 table ([`crate::glyphs`]) remains the
//! floor for blocks too small for the 12-cell at scale 1.
//!
//! GENERATED FILE — DO NOT EDIT BY HAND.
//! Regenerate: `uv run tools/gen_spleen_glyphs.py`
//! Drift gate: `uv run tools/gen_spleen_glyphs.py --check`
//!
//! Provenance: Spleen 2.1.0, <https://github.com/fcambus/spleen>
//! (BDF faces; the 5x8 face is excluded by the verdict).
//!
//! Spleen license (BSD-2-Clause), retained per the SOUP inventory row
//! in `soup/inventory.toml`:
//!
{doc_license}

/// One vendored Spleen cell, cropped to the alphabet-wide cap-ink
/// band. `cell` is the nominal cell height the ADR-031 §8 selection
/// law works in (`nominal = rows * cell * k`); `band` is the stored
/// ink height every glyph in the cell shares.
pub struct SpleenCell {{
    /// Nominal cell height in glyph px (12, 16, 24, 32, or 64).
    pub cell: u32,
    /// Cap-ink band height: every glyph stores exactly this many rows.
    pub band: u32,
    /// Per glyph, in alphabet (= char) order: the char, its BDF
    /// dwidth (the per-char advance in glyph px), and its `band` bit
    /// rows top to bottom. Bit `dwidth - 1` is the leftmost column,
    /// so the literals read like the glyph.
    glyphs: &'static [(char, u32, &'static [u32])],
}}

impl SpleenCell {{
    /// The dwidth (advance) and cap-band bit rows of `c`. `None` for
    /// any char outside the nano14 alphabet ([`crate::glyphs::ALPHABET`]).
    pub fn glyph(&self, c: char) -> Option<(u32, &'static [u32])> {{
        self.glyphs
            .binary_search_by_key(&c, |&(g, _, _)| g)
            .ok()
            .map(|i| (self.glyphs[i].1, self.glyphs[i].2))
    }}

    /// Number of ink dots in `c`'s cap-band glyph — the typography
    /// side of a rect-count ledger. `None` outside the alphabet.
    pub fn ink_bits(&self, c: char) -> Option<u32> {{
        self.glyph(c)
            .map(|(_, rows)| rows.iter().map(|r| r.count_ones()).sum())
    }}
}}

/// The five cells, ascending — the ADR-031 §8 better-res selection
/// iterates these and breaks nominal ties toward the larger cell.
pub const CELLS: [&SpleenCell; {len(cells)}] = [{cell_refs}];

/// The cell table whose nominal height is `cell`, if vendored.
pub fn cell(cell: u32) -> Option<&'static SpleenCell> {{
    CELLS.iter().copied().find(|c| c.cell == cell)
}}
"""
    )
    for cell_h in sorted(cells):
        data = cells[cell_h]
        out.append(
            f"""
/// Spleen {cell_h // 2}x{cell_h}, cap-ink band {data['band']} rows.
pub const CELL_{cell_h}: SpleenCell = SpleenCell {{
    cell: {cell_h},
    band: {data['band']},
    glyphs: GLYPHS_{cell_h},
}};

/// Total ink dots across the 31 glyphs of [`CELL_{cell_h}`] — the
/// vendored-table drift checksum, recomputed by the generator.
pub const INK_BITS_{cell_h}: u32 = {data['ink']};

#[rustfmt::skip]
const GLYPHS_{cell_h}: &[(char, u32, &[u32])] = &[
"""
        )
        for ch in ALPHABET:
            dwidth, rows = data["glyphs"][ch]
            out.append(f"    ('{ch}', {dwidth}, &[\n")
            for row in rows:
                out.append(f"        0b{row:0{dwidth}b},\n")
            out.append("    ]),\n")
        out.append("];\n")
    out.append(
        """
#[cfg(test)]
mod tests {
    use super::*;
    use crate::glyphs::ALPHABET;

    #[test]
    fn every_cell_covers_exactly_the_nano14_alphabet_in_order() {
        for cell in CELLS {
            assert_eq!(cell.glyphs.len(), ALPHABET.chars().count());
            let stored: String = cell.glyphs.iter().map(|&(c, _, _)| c).collect();
            assert_eq!(stored, ALPHABET, "cell {} char order", cell.cell);
            for c in ALPHABET.chars() {
                assert!(cell.glyph(c).is_some(), "cell {} glyph {c}", cell.cell);
            }
            for c in ['0', 'O', '1', 'I', 'L', ' ', 'a'] {
                assert!(cell.glyph(c).is_none(), "{c} is outside the alphabet");
            }
        }
    }

    #[test]
    fn band_geometry_holds_and_ink_totals_match_the_checksums() {
        let expected = [
            INK_BITS_12,
            INK_BITS_16,
            INK_BITS_24,
            INK_BITS_32,
            INK_BITS_64,
        ];
        for (cell, want) in CELLS.iter().zip(expected) {
            let mut total = 0;
            let mut top_inked = false;
            let mut bottom_inked = false;
            for c in ALPHABET.chars() {
                let (dwidth, rows) = cell.glyph(c).expect("in alphabet");
                assert_eq!(dwidth, cell.cell / 2, "Spleen cells are monospace");
                assert_eq!(rows.len() as u32, cell.band, "cell {} {c}", cell.cell);
                for row in rows {
                    assert!(
                        u64::from(*row) < 1u64 << dwidth,
                        "cell {} {c} row fits {dwidth} columns",
                        cell.cell
                    );
                }
                top_inked |= rows[0] != 0;
                bottom_inked |= rows[rows.len() - 1] != 0;
                total += cell.ink_bits(c).expect("in alphabet");
            }
            assert!(
                top_inked && bottom_inked,
                "cell {} band is tight",
                cell.cell
            );
            assert_eq!(total, want, "cell {} ink checksum", cell.cell);
        }
    }
}
"""
    )
    return "".join(out)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--font-dir", help="directory holding the spleen-*.bdf faces")
    ap.add_argument(
        "--out",
        type=Path,
        default=DEFAULT_OUT,
        help=f"output path (default: {DEFAULT_OUT})",
    )
    ap.add_argument(
        "--check",
        action="store_true",
        help="regenerate to memory and diff against the checked-in file",
    )
    args = ap.parse_args()

    font_dir = find_font_dir(args.font_dir)
    cells = {cell_h: build_cell(font_dir, face) for face, cell_h in CELLS}
    text = emit(cells)

    summary = ", ".join(
        f"{c}: band {cells[c]['band']} ink {cells[c]['ink']}" for c in sorted(cells)
    )
    if args.check:
        on_disk = args.out.read_text() if args.out.is_file() else ""
        if on_disk != text:
            sys.stderr.write(
                f"DRIFT: {args.out} does not match the generator output; "
                "rerun `uv run tools/gen_spleen_glyphs.py`\n"
            )
            return 1
        sys.stdout.write(f"ok: {args.out} is current ({summary})\n")
        return 0
    args.out.write_text(text)
    sys.stdout.write(f"wrote {args.out} ({summary})\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
