#!/usr/bin/env -S uv run
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Bake crates/codec/src/glyph_font.rs from design/glyph-font.v2.json.

nx75 v2 — the part-registry anchor font, CONNECTION-KERNEL model.
The design file carries, per glyph, a 7x5 anchor bitmap ("px"), edge
overrides ("conn", "r1,c1-r2,c2" -> bool) and kernel overrides
("kern", "r,c" -> [tl,tr,bl,br]). This baker resolves the active
edges and per-anchor kernels and emits a pure const-data Rust module
plus per-glyph ink-pixel checksums for k = 2, 3, 4, 6 computed by
the same raster law the Rust renderer implements
(crates/codec/src/px.rs).

Semantics match the reference implementation (the JS inside
tools/font_editor_gen.py) bit for bit:

- candidate edges: 8-adjacent anchor pairs scanned row-major over
  dirs (0,1) (1,0) (1,1) (1,-1); orthogonal edges default ON, a
  diagonal defaults ON iff both bridge cells are white; "conn"
  overrides win either way
- kernels: a "kern" override wins; else any active orthogonal edge
  OR an isolated anchor yields the full square [1,1,1,1]; else the
  bare quadrant-less node [0,0,0,0]

The render is the union of three stamp types — no masks, no sweeps,
no windows, no outward signs:

1. STRAIGHT connection: a k-wide rectangle between the two node
   centers inclusive (px center within [c1_center, c2_center]
   longitudinally, within k/2 of the line transversely)
2. DIAGONAL connection: at the shared cell corner
   (cx = max(c1,c2)*k, cy = max(r1,r2)*k), pixels with
   L1(p - corner) <= k + eps AND anti-diagonal index |dx-dy| (when
   the edge direction has dr == dc) or |dx+dy| (otherwise)
   <= k-1 + eps
3. NODE quadrants: each anchor-cell pixel not already painted is
   painted iff its quadrant bit is set in the resolved kernel
   (quadrant = (dy<0?0:2)+(dx<0?0:1) relative to the cell center)

The stamps are bounded by construction, so no clip mask exists.

The output is deterministic (alphabet order, no timestamps):
re-running on the same design file yields a byte-identical module.
`--check` regenerates to memory and diffs against the checked-in
file — the CI drift gate for the baked font.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ALPHABET = "23456789ABCDEFGHJKMNPQRSTUVWXYZ"
ROWS, COLS = 7, 5
CHECKSUM_KS = (2, 3, 4, 6)

REPO_ROOT = Path(__file__).resolve().parent.parent
DESIGN = REPO_ROOT / "design" / "glyph-font.v2.json"
DEFAULT_OUT = REPO_ROOT / "crates" / "codec" / "src" / "glyph_font.rs"


def at(px: list[list[int]], r: int, c: int) -> int:
    return px[r][c] if 0 <= r < ROWS and 0 <= c < COLS else 0


def resolve_glyph(g: dict) -> tuple[list[dict], list[dict]]:
    """Resolve one design-file glyph into baked anchors + edges.

    Returns (anchors, edges); anchors are dicts with r, c, quad_mask;
    edges are dicts with a, b (anchor indices) and diag.
    """
    px = g["px"]
    conn = g.get("conn", {})
    kern = g.get("kern", {})

    anchors = [(r, c) for r in range(ROWS) for c in range(COLS) if px[r][c]]
    index = {rc: i for i, rc in enumerate(anchors)}

    # Candidate edges in scan order; overrides win over the defaults.
    edges = []
    for r in range(ROWS):
        for c in range(COLS):
            if not px[r][c]:
                continue
            for dr, dc in ((0, 1), (1, 0), (1, 1), (1, -1)):
                if not at(px, r + dr, c + dc):
                    continue
                diag = dr != 0 and dc != 0
                default = True
                if diag:
                    default = not (at(px, r, c + dc) or at(px, r + dr, c))
                key = f"{r},{c}-{r + dr},{c + dc}"
                on = bool(conn[key]) if key in conn else default
                if on:
                    edges.append(((r, c), (r + dr, c + dc), diag))

    incident: dict[tuple[int, int], list] = {}
    for a, b, diag in edges:
        incident.setdefault(a, []).append((a, b, diag))
        incident.setdefault(b, []).append((a, b, diag))

    # Kernels: override wins; else any active orthogonal edge or an
    # isolated anchor yields the full square, else all quadrants off.
    def kernel(rc: tuple[int, int]) -> int:
        k = kern.get(f"{rc[0]},{rc[1]}")
        if k is not None:
            corners = [int(bool(v)) for v in k]
        else:
            es = incident.get(rc, [])
            orth = any(not diag for _, _, diag in es)
            corners = [1, 1, 1, 1] if orth or not es else [0, 0, 0, 0]
        return corners[0] | corners[1] << 1 | corners[2] << 2 | corners[3] << 3

    baked_anchors = [
        {"r": r, "c": c, "quad_mask": kernel((r, c))} for r, c in anchors
    ]
    baked_edges = [
        {"a": index[a], "b": index[b], "diag": diag} for a, b, diag in edges
    ]
    return baked_anchors, baked_edges


def raster(anchors: list[dict], edges: list[dict], k: int) -> int:
    """Ink-pixel count of the 5k x 7k raster — the renderer's law."""
    return sum(v for row in raster_image(anchors, edges, k) for v in row)


def raster_image(anchors: list[dict], edges: list[dict], k: int) -> list[list[int]]:
    """The full 5k x 7k ink bitmap, row-major — the renderer's law.

    This mirrors the reference JS raster() in tools/font_editor_gen.py
    and crates/codec/src/px.rs raster_glyph expression for expression,
    so the baked checksums lock all three implementations bit for bit.
    """
    half = k / 2.0
    cell = {(a["r"], a["c"]): a["quad_mask"] for a in anchors}

    stamps = []
    for e in edges:
        a, b = anchors[e["a"]], anchors[e["b"]]
        if not e["diag"]:
            x1 = (min(a["c"], b["c"]) + 0.5) * k
            x2 = (max(a["c"], b["c"]) + 0.5) * k
            y1 = (min(a["r"], b["r"]) + 0.5) * k
            y2 = (max(a["r"], b["r"]) + 0.5) * k
            stamps.append(("rect", x1, x2, y1, y2))
        else:
            cx = max(a["c"], b["c"]) * k
            cy = max(a["r"], b["r"]) * k
            # Anti-diagonal index sign: direction (1,1) -> dx-dy,
            # else dx+dy
            same_sign = (b["r"] - a["r"]) == (b["c"] - a["c"])
            stamps.append(("diam", cx, cy, same_sign))

    img = [[0] * (COLS * k) for _ in range(ROWS * k)]
    for j in range(ROWS * k):
        y = j + 0.5
        for i in range(COLS * k):
            x = i + 0.5
            on = False
            for s in stamps:
                if s[0] == "rect":
                    _, x1, x2, y1, y2 = s
                    if y1 == y2:
                        # Horizontal: px centers between the node
                        # centers inclusive, k-wide perpendicular
                        if x >= x1 - 1e-9 and x <= x2 + 1e-9 and abs(y - y1) <= half:
                            on = True
                            break
                    else:
                        if y >= y1 - 1e-9 and y <= y2 + 1e-9 and abs(x - x1) <= half:
                            on = True
                            break
                else:
                    # Corner diamond (radius k, reaches both node
                    # centers) clipped to the k-row perpendicular
                    # band: chains render constant-width, single
                    # corners become k-wide chamfers
                    _, cx, cy, same_sign = s
                    dx, dy = x - cx, y - cy
                    anti = abs(dx - dy) if same_sign else abs(dx + dy)
                    if abs(dx) + abs(dy) <= k + 1e-9 and anti <= k - 1 + 1e-9:
                        on = True
                        break
            if not on:
                mask = cell.get((j // k, i // k))
                if mask is not None:
                    cc, cr = i // k, j // k
                    dx = x - (cc + 0.5) * k
                    dy = y - (cr + 0.5) * k
                    ci = (0 if dy < 0 else 2) + (0 if dx < 0 else 1)
                    if mask >> ci & 1:
                        on = True
            if on:
                img[j][i] = 1
    return img


HEADER = '''\
//! nx75 v2 — the part-registry anchor font.
//!
//! A first-party 7x5 ANCHOR font for the nano14 id alphabet, baked
//! under the CONNECTION-KERNEL model: each glyph is a set of anchor
//! nodes on a 7-row x 5-column cell grid joined by orthogonal and
//! diagonal connections, rasterised at any integer scale k by
//! [`crate::px`] as the union of three stamp types — straight
//! connections (a k-wide rectangle between the two node centers
//! inclusive), diagonal connections (an L1 diamond of radius k at
//! the shared cell corner, clipped to the k-row anti-diagonal band)
//! and node quadrants (each anchor-cell pixel not already painted is
//! painted iff its quadrant bit is set in the anchor's kernel). The
//! stamps are bounded by construction — no clip mask exists.
//!
//! GENERATED FILE — DO NOT EDIT BY HAND.
//! Generated from `design/glyph-font.v2.json` (the source of truth,
//! authored in the labels/typography-bench font editor) by
//! `tools/bake_glyph_font.py`, which resolves active edges and
//! per-anchor quadrant kernels at bake time.
//! Drift gate: `uv run tools/bake_glyph_font.py --check`

/// Glyph cell height in anchor cells.
pub const GRID_ROWS: u32 = 7;
/// Glyph cell width in anchor cells.
pub const GRID_COLS: u32 = 5;
/// The nano14 id alphabet the font covers (ADR-012: no `0/O/1/I/L`).
pub const ALPHABET: &str = "23456789ABCDEFGHJKMNPQRSTUVWXYZ";
/// Scales with baked ink checksums, in `Glyph::ink_bits` order.
pub const CHECKSUM_KS: [u32; 4] = [2, 3, 4, 6];

/// One anchor node of a glyph on the 7x5 grid.
#[derive(Clone, Copy, Debug)]
pub struct Anchor {
    /// Grid row, 0 at the top.
    pub r: u8,
    /// Grid column, 0 at the left.
    pub c: u8,
    /// Resolved kernel quadrants: bit 0 top-left, bit 1 top-right,
    /// bit 2 bottom-left, bit 3 bottom-right — 0b1111 paints the
    /// full cell, 0b0000 leaves the node to its connections.
    pub quad_mask: u8,
}

/// One ACTIVE connection between two anchors (inactive candidates
/// are resolved away at bake time).
#[derive(Clone, Copy, Debug)]
pub struct Edge {
    /// First endpoint, as an index into the glyph's anchor list (the
    /// grid-scan cell the edge was discovered from).
    pub a: u8,
    /// Second endpoint index (the 8-neighbour).
    pub b: u8,
    /// Diagonal connection (both row and column step).
    pub diag: bool,
}

/// One nx75 glyph: anchors, active edges and baked ink checksums.
#[derive(Clone, Copy, Debug)]
pub struct Glyph {
    /// The nano14 character this glyph renders.
    pub ch: char,
    /// Anchors in grid-scan order (row-major).
    pub anchors: &'static [Anchor],
    /// Active connections, endpoints as indices into `anchors`.
    pub edges: &'static [Edge],
    /// Ink-pixel counts of the rasterised glyph at the scales in
    /// [`CHECKSUM_KS`], in order — the bake-time checksums the codec
    /// test suite locks the Rust renderer against.
    pub ink_bits: [u32; 4],
}

/// The glyph record for `c`, or `None` outside [`ALPHABET`].
pub fn glyph(c: char) -> Option<&'static Glyph> {
    GLYPHS.iter().find(|g| g.ch == c)
}

/// All 31 glyphs, in [`ALPHABET`] order.
pub static GLYPHS: [Glyph; 31] = [
'''

FOOTER = "];\n"


def emit(data: dict) -> str:
    out = [HEADER]
    for ch in ALPHABET:
        anchors, edges = resolve_glyph(data[ch])
        sums = [raster(anchors, edges, k) for k in CHECKSUM_KS]
        out.append("    Glyph {\n")
        out.append(f"        ch: '{ch}',\n")
        out.append("        anchors: &[\n")
        for a in anchors:
            out.append("            Anchor {\n")
            out.append(f"                r: {a['r']},\n")
            out.append(f"                c: {a['c']},\n")
            out.append(f"                quad_mask: 0b{a['quad_mask']:04b},\n")
            out.append("            },\n")
        out.append("        ],\n")
        out.append("        edges: &[\n")
        for e in edges:
            out.append("            Edge {\n")
            out.append(f"                a: {e['a']},\n")
            out.append(f"                b: {e['b']},\n")
            out.append(f"                diag: {str(e['diag']).lower()},\n")
            out.append("            },\n")
        out.append("        ],\n")
        out.append(f"        ink_bits: [{', '.join(str(s) for s in sums)}],\n")
        out.append("    },\n")
    out.append(FOOTER)
    return "".join(out)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--design", type=Path, default=DESIGN)
    ap.add_argument("--out", type=Path, default=DEFAULT_OUT)
    ap.add_argument(
        "--check",
        action="store_true",
        help="regenerate to memory and diff against the checked-in file",
    )
    args = ap.parse_args()

    data = json.loads(args.design.read_text())
    if set(data) != set(ALPHABET):
        missing = sorted(set(ALPHABET) - set(data))
        extra = sorted(set(data) - set(ALPHABET))
        sys.exit(f"design file alphabet mismatch: missing {missing}, extra {extra}")
    for ch, g in data.items():
        px = g["px"]
        if len(px) != ROWS or any(len(row) != COLS for row in px):
            sys.exit(f"glyph {ch}: px is not {ROWS}x{COLS}")

    generated = emit(data)
    if args.check:
        on_disk = args.out.read_text() if args.out.exists() else ""
        if on_disk != generated:
            print(f"DRIFT: {args.out} does not match design/{args.design.name}")
            print("rerun: uv run tools/bake_glyph_font.py")
            return 1
        print(f"ok: {args.out} matches {args.design}")
        return 0
    args.out.write_text(generated)
    print(f"-> {args.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
