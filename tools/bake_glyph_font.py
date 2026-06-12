#!/usr/bin/env -S uv run
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Bake crates/codec/src/glyph_font.rs from design/glyph-font.v1.json.

nx75 — the part-registry anchor font. The design file carries, per
glyph, a 7x5 anchor bitmap ("px"), edge overrides ("conn",
"r1,c1-r2,c2" -> bool) and kernel overrides ("kern", "r,c" ->
[tl,tr,bl,br]). This baker RESOLVES everything the renderer would
otherwise re-derive — active edges, per-anchor kernels, band-owned
pass-through anchors, per-edge ink-balance signs — and emits a pure
const-data Rust module plus per-glyph ink-pixel checksums for
k = 2, 3, 4, 6 computed by the same raster law the Rust renderer
implements (crates/codec/src/px.rs).

Semantics match the reference implementation (the JS inside
tools/font_editor_gen.py) bit for bit:

- candidate edges: 8-adjacent anchor pairs scanned row-major over
  dirs (0,1) (1,0) (1,1) (1,-1); orthogonal edges default ON, a
  diagonal defaults ON iff both bridge cells are white; "conn"
  overrides win either way
- kernels: a "kern" override wins; else any active orthogonal edge
  OR an isolated anchor yields the full square [1,1,1,1]; else the
  bare diamond [0,0,0,0]
- band-owned anchors: exactly two collinear DIAGONAL edges — no
  rest-stamp (the constant-derivative law)
- diagonal tips: an anchor with exactly ONE active edge and that
  edge diagonal gets a corners-only rest-stamp (no L1 diamond term)
- out_sign per diagonal edge: the sign of the ink balance over all
  anchors against the edge line's normal in the edge's CANONICAL
  a->b frame (a < b lexicographically, as stored), computed once at
  bake time; the renderer turns it into outSign = +1 at balance > 0
  else -1 (outside = the negative-dsig side) and oneSided =
  balance != 0, and BOTH half-sweeps measure dsig with the same
  canonical normal and outSign — never recomputed from the
  half-sweep direction, so a centered band cannot flip its outer
  side at the edge midpoint

The output is deterministic (alphabet order, no timestamps):
re-running on the same design file yields a byte-identical module.
`--check` regenerates to memory and diffs against the checked-in
file — the CI drift gate for the baked font.
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path

ALPHABET = "23456789ABCDEFGHJKMNPQRSTUVWXYZ"
ROWS, COLS = 7, 5
CHECKSUM_KS = (2, 3, 4, 6)

REPO_ROOT = Path(__file__).resolve().parent.parent
DESIGN = REPO_ROOT / "design" / "glyph-font.v1.json"
DEFAULT_OUT = REPO_ROOT / "crates" / "codec" / "src" / "glyph_font.rs"

SQRT2 = math.sqrt(2.0)


def at(px: list[list[int]], r: int, c: int) -> int:
    return px[r][c] if 0 <= r < ROWS and 0 <= c < COLS else 0


def resolve_glyph(g: dict) -> tuple[list[dict], list[dict]]:
    """Resolve one design-file glyph into baked anchors + edges.

    Returns (anchors, edges); anchors are dicts with r, c,
    corner_mask, band_owned, has_stamp, diag_tip; edges are dicts
    with a, b (anchor indices), diag, bal_sign.
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

    # Band-owned anchors: exactly two collinear diagonal edges.
    incident: dict[tuple[int, int], list] = {}
    for a, b, diag in edges:
        incident.setdefault(a, []).append((a, b, diag))
        incident.setdefault(b, []).append((a, b, diag))
    band_owned = set()
    diag_tip = set()
    for rc, es in incident.items():
        if len(es) == 2 and es[0][2] and es[1][2]:
            d0 = (es[0][1][0] - es[0][0][0], es[0][1][1] - es[0][0][1])
            d1 = (es[1][1][0] - es[1][0][0], es[1][1][1] - es[1][0][1])
            if abs(d0[0] * d1[1] - d0[1] * d1[0]) < 1e-9:
                band_owned.add(rc)
        # Pure diagonal tip: exactly one active edge, diagonal — its
        # rest-stamp is corners-only (no L1 diamond term)
        if len(es) == 1 and es[0][2]:
            diag_tip.add(rc)

    # Kernels: override wins; else orth-touching or isolated anchors
    # are the full square, pure-diagonal anchors the bare diamond.
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
        {
            "r": r,
            "c": c,
            "corner_mask": kernel((r, c)),
            "band_owned": (r, c) in band_owned,
            "has_stamp": (r, c) not in band_owned,
            "diag_tip": (r, c) in diag_tip,
        }
        for r, c in anchors
    ]

    # Per-edge ink-balance sign in the edge's CANONICAL a->b frame —
    # both half-sweeps reuse it. Scale-invariant, so it bakes once:
    # anchors on the edge line are within the 1e-9 threshold and
    # never vote.
    baked_edges = []
    for a, b, diag in edges:
        bal_sign = 0
        if diag:
            ax, ay = a[1] + 0.5, a[0] + 0.5
            bx, by = b[1] + 0.5, b[0] + 0.5
            ln = math.sqrt((bx - ax) ** 2 + (by - ay) ** 2)
            nx, ny = -(by - ay) / ln, (bx - ax) / ln
            bal = 0
            for rr, cc in anchors:
                d = (cc + 0.5 - ax) * nx + (rr + 0.5 - ay) * ny
                if abs(d) > 1e-9:
                    bal += 1 if d > 0 else -1
            bal_sign = (bal > 0) - (bal < 0)
        baked_edges.append(
            {"a": index[a], "b": index[b], "diag": diag, "bal_sign": bal_sign}
        )
    return baked_anchors, baked_edges


def kern_covers(mask: int, dx: float, dy: float, half: float) -> bool:
    if abs(dx) + abs(dy) <= half:
        return True
    if abs(dx) > half or abs(dy) > half:
        return False
    ci = (0 if dy < 0 else 2) + (0 if dx < 0 else 1)
    return bool(mask >> ci & 1)


def raster(anchors: list[dict], edges: list[dict], k: int) -> int:
    """Ink-pixel count of the 5k x 7k raster — the renderer's law.

    This mirrors crates/codec/src/px.rs raster_glyph operation for
    operation (same expressions, same order) so the baked checksums
    lock the Rust renderer bit for bit.
    """
    half = k / 2.0
    # Diagonal band windows: k anti-diagonal rows total, with k=3
    # gaining one bonus row on the outside. A one-sided band hugs the
    # outside of the anchor line — its inner boundary IS the line; a
    # centered band splits the rows inner/outer by parity.
    odd = k % 2 == 1
    rows_one = k + (1 if k == 3 else 0)
    lo_one = -((rows_one - (1.0 if odd else 0.5)) / SQRT2 + 1e-6)
    inner = (k - 1) / 2.0 if odd else k / 2.0 - 1.0
    outer = ((k - 1) / 2.0 if odd else k / 2.0) + (1.0 if k == 3 else 0.0)
    lo_cen = -(outer / SQRT2 + 1e-6)
    hi_cen = inner / SQRT2 + 1e-6

    allowed = {(a["r"], a["c"]) for a in anchors}
    for e in edges:
        if e["diag"]:
            a, b = anchors[e["a"]], anchors[e["b"]]
            allowed.add((a["r"], b["c"]))
            allowed.add((b["r"], a["c"]))

    sweeps = []
    for e in edges:
        a, b = anchors[e["a"]], anchors[e["b"]]
        nx = ny = 0.0
        out = 1.0
        one_sided = False
        if e["diag"]:
            # Canonical a->b frame: one normal and one outSign for
            # BOTH half-sweeps — outside is the negative-dsig side
            ax0, ay0 = (a["c"] + 0.5) * k, (a["r"] + 0.5) * k
            bx0, by0 = (b["c"] + 0.5) * k, (b["r"] + 0.5) * k
            ln = math.sqrt((bx0 - ax0) ** 2 + (by0 - ay0) ** 2)
            nx, ny = -(by0 - ay0) / ln, (bx0 - ax0) / ln
            out = 1.0 if e["bal_sign"] > 0 else -1.0
            one_sided = e["bal_sign"] != 0
        for me, ot in ((a, b), (b, a)):
            ax, ay = (me["c"] + 0.5) * k, (me["r"] + 0.5) * k
            mx = ((me["c"] + ot["c"]) / 2 + 0.5) * k
            my = ((me["r"] + ot["r"]) / 2 + 0.5) * k
            sweeps.append(
                (ax, ay, mx - ax, my - ay, e["diag"], nx, ny, out, one_sided)
            )
    stamps = [
        ((a["c"] + 0.5) * k, (a["r"] + 0.5) * k, a["corner_mask"], a["diag_tip"])
        for a in anchors
        if a["has_stamp"]
    ]

    ink = 0
    for j in range(ROWS * k):
        cr = j // k
        y = j + 0.5
        for i in range(COLS * k):
            if (cr, i // k) not in allowed:
                continue
            x = i + 0.5
            on = False
            for ax, ay, vx, vy, diag, nx, ny, out, one_sided in sweeps:
                l2 = vx * vx + vy * vy
                t = max(0.0, min(1.0, ((x - ax) * vx + (y - ay) * vy) / l2))
                if t <= 0.0:
                    continue
                if diag:
                    dsig = ((x - ax) * nx + (y - ay) * ny) * out
                    if k <= 2:
                        hit = abs(dsig) <= half + 1e-6
                    elif one_sided:
                        hit = lo_one <= dsig <= 1e-6
                    else:
                        hit = lo_cen <= dsig <= hi_cen
                    if hit:
                        on = True
                        break
                else:
                    dx = x - (ax + t * vx)
                    dy = y - (ay + t * vy)
                    if math.sqrt(dx * dx + dy * dy) <= half:
                        on = True
                        break
            if not on:
                for sx, sy, mask, tip in stamps:
                    dx, dy = x - sx, y - sy
                    if tip:
                        # Pure diagonal tip: corners-only endplate —
                        # the band end is the cap, the chip is the
                        # outward block, no L1 diamond term
                        if abs(dx) <= half and abs(dy) <= half:
                            ci = (0 if dy < 0 else 2) + (0 if dx < 0 else 1)
                            if mask >> ci & 1:
                                on = True
                                break
                    elif kern_covers(mask, dx, dy, half):
                        on = True
                        break
            if on:
                ink += 1
    return ink


HEADER = '''\
//! nx75 — the part-registry anchor font.
//!
//! A first-party 7x5 ANCHOR font for the nano14 id alphabet: each
//! glyph is a set of anchors on a 7-row x 5-column cell grid joined
//! by orthogonal and diagonal edges, rasterised at any integer scale
//! k by the [`crate::px`] sweep law (anchor kernels pulled along
//! half-edges to the edge midpoints, rest-stamps at the anchors,
//! cell-clipped to the anchor cells plus diagonal bridge cells).
//!
//! Design rules:
//! 1. Diagonal-touching anchors are diamond.
//! 2. Orth-only anchors are square.
//! 3. Diagonal tips keep the outward corner.
//! 4. Diagonals carry into their corner anchor, orth stubs yield.
//!
//! GENERATED FILE — DO NOT EDIT BY HAND.
//! Generated from `design/glyph-font.v1.json` (the source of truth,
//! authored in the labels/typography-bench font editor) by
//! `tools/bake_glyph_font.py`, which resolves edges, kernels,
//! band-owned anchors and outward signs at bake time.
//! Drift gate: `uv run tools/bake_glyph_font.py --check`

/// Glyph cell height in anchor cells.
pub const GRID_ROWS: u32 = 7;
/// Glyph cell width in anchor cells.
pub const GRID_COLS: u32 = 5;
/// The nano14 id alphabet the font covers (ADR-012: no `0/O/1/I/L`).
pub const ALPHABET: &str = "23456789ABCDEFGHJKMNPQRSTUVWXYZ";
/// Scales with baked ink checksums, in `Glyph::ink_bits` order.
pub const CHECKSUM_KS: [u32; 4] = [2, 3, 4, 6];

/// One anchor of a glyph on the 7x5 grid.
#[derive(Clone, Copy, Debug)]
pub struct Anchor {
    /// Grid row, 0 at the top.
    pub r: u8,
    /// Grid column, 0 at the left.
    pub c: u8,
    /// Rest-stamp kernel corners: bit 0 top-left, bit 1 top-right,
    /// bit 2 bottom-left, bit 3 bottom-right — 0b1111 is the full
    /// square kernel, 0b0000 the bare diamond.
    pub corner_mask: u8,
    /// Pass-through diagonal anchor (exactly two collinear diagonal
    /// edges): its cells are band-owned and it gets NO rest-stamp
    /// (the constant-derivative law).
    pub band_owned: bool,
    /// Whether the rest-stamp applies at this anchor — every anchor
    /// that is not band-owned, isolated anchors included.
    pub has_stamp: bool,
    /// Pure diagonal tip (exactly one active edge, diagonal): its
    /// rest-stamp is corners-only — covered iff |dx| <= k/2 and
    /// |dy| <= k/2 and the quadrant's corner bit is set, with NO L1
    /// diamond term (the band end is the cap, the chip the outward
    /// block).
    pub diag_tip: bool,
}

/// One ACTIVE edge between two anchors (inactive candidates are
/// resolved away at bake time).
#[derive(Clone, Copy, Debug)]
pub struct Edge {
    /// First endpoint, as an index into the glyph's anchor list (the
    /// grid-scan cell the edge was discovered from).
    pub a: u8,
    /// Second endpoint index (the 8-neighbour).
    pub b: u8,
    /// Diagonal edge (both row and column step).
    pub diag: bool,
    /// Sign of the glyph's ink balance against the edge line's
    /// normal in the edge's CANONICAL a->b frame, in -1/0/+1,
    /// computed once at bake time. The renderer derives one outward
    /// sign per edge from it — `+1` at balance > 0, else `-1`, with
    /// outside the negative-dsig side — plus `one_sided = balance
    /// != 0`, and BOTH half-sweeps measure dsig with the same
    /// canonical normal and sign (never recomputed from the
    /// half-sweep direction). Always 0 for orthogonal edges (unused
    /// there).
    pub out_sign: i8,
}

/// One nx75 glyph: anchors, active edges and baked ink checksums.
#[derive(Clone, Copy, Debug)]
pub struct Glyph {
    /// The nano14 character this glyph renders.
    pub ch: char,
    /// Anchors in grid-scan order (row-major).
    pub anchors: &'static [Anchor],
    /// Active edges, endpoints as indices into `anchors`.
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
            out.append(f"                corner_mask: 0b{a['corner_mask']:04b},\n")
            out.append(f"                band_owned: {str(a['band_owned']).lower()},\n")
            out.append(f"                has_stamp: {str(a['has_stamp']).lower()},\n")
            out.append(f"                diag_tip: {str(a['diag_tip']).lower()},\n")
            out.append("            },\n")
        out.append("        ],\n")
        out.append("        edges: &[\n")
        for e in edges:
            out.append("            Edge {\n")
            out.append(f"                a: {e['a']},\n")
            out.append(f"                b: {e['b']},\n")
            out.append(f"                diag: {str(e['diag']).lower()},\n")
            out.append(f"                out_sign: {e['bal_sign']},\n")
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
