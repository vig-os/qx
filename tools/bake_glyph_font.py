#!/usr/bin/env -S uv run
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Bake crates/codec/src/glyph_font.rs from BOTH nx75 design files.

nx75 — the part-registry anchor font, PARITY-DISPATCHED OPTICAL
MASTERS. Two masters of the same 7x5 anchor design ship side by side
and the renderer picks per glyph scale k:

- EVEN k  -> v1 master (design/glyph-font.v1.json) under the
  KERNEL-PULL law — the user judged v1 best at even scales
- ODD k   -> v2 master (design/glyph-font.v2.json) under the
  CONNECTION-KERNEL law — judged best at odd scales

Both design files carry, per glyph, a 7x5 anchor bitmap ("px"), edge
overrides ("conn", "r1,c1-r2,c2" -> bool) and kernel overrides
("kern", "r,c" -> [tl,tr,bl,br]). Resolution is the SAME for both
masters:

- candidate edges: 8-adjacent anchor pairs scanned row-major over
  dirs (0,1) (1,0) (1,1) (1,-1); orthogonal edges default ON, a
  diagonal defaults ON iff both bridge cells are white; "conn"
  overrides win either way
- kernels: a "kern" override wins; else any active orthogonal edge
  OR an isolated anchor yields the full square [1,1,1,1]; else the
  bare quadrant-less node [0,0,0,0]

The two RASTER laws differ:

V1 — KERNEL-PULL (the TRUE v1 renderer, /tmp/true_v1.cjs verbatim):
each active edge contributes TWO sweeps — each endpoint's kernel
swept from that anchor's center to the edge MIDPOINT; isolated
anchors contribute one stationary sweep at rest. A pixel center p is
ink iff some sweep covers it: t = clamp(proj(p onto sweep), 0, 1),
d = p - lerp(t), covered iff L1(d) <= k/2 OR (|dx| <= k/2 AND
|dy| <= k/2 AND the quadrant's corner bit is set). A cell mask
clips everything: only anchor cells plus the two bridge cells of
each active diagonal edge may carry ink.

V2 — CONNECTION-KERNEL (crates/codec/src/px.rs raster_v2, the
editor JS): the union of three bounded stamps — straight
connections (a k-wide rectangle between the two node centers
inclusive), diagonal connections (an L1 diamond of radius k at the
shared cell corner, clipped to the k-row anti-diagonal band via
|dx-dy| when dr == dc else |dx+dy| <= k-1 + eps) and node quadrants
(each anchor-cell pixel not already painted is painted iff its
quadrant bit is set in the resolved kernel). No mask exists.

Checksums lock each master at its OWN parity: v1 at k = 2, 4, 6;
v2 at k = 3, 5 — the scales each master actually renders at.

The output is deterministic (alphabet order, no timestamps):
re-running on the same design files yields a byte-identical module.
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
CHECKSUM_KS_V1 = (2, 4, 6)
CHECKSUM_KS_V2 = (3, 5)

REPO_ROOT = Path(__file__).resolve().parent.parent
DESIGN_V1 = REPO_ROOT / "design" / "glyph-font.v1.json"
DESIGN_V2 = REPO_ROOT / "design" / "glyph-font.v2.json"
DEFAULT_OUT = REPO_ROOT / "crates" / "codec" / "src" / "glyph_font.rs"


def at(px: list[list[int]], r: int, c: int) -> int:
    return px[r][c] if 0 <= r < ROWS and 0 <= c < COLS else 0


def resolve_glyph(g: dict) -> tuple[list[dict], list[dict]]:
    """Resolve one design-file glyph into baked anchors + edges.

    Returns (anchors, edges); anchors are dicts with r, c, quad_mask;
    edges are dicts with a, b (anchor indices) and diag. The
    resolution law is shared by both masters (see module docstring).
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


def kern_covers(mask: int, dx: float, dy: float, half: float) -> bool:
    """V1 kernel coverage at offset (dx, dy) from the swept center."""
    if abs(dx) + abs(dy) <= half:
        return True
    if abs(dx) > half or abs(dy) > half:
        return False
    ci = (0 if dy < 0 else 2) + (0 if dx < 0 else 1)
    return bool(mask >> ci & 1)


def raster_image_v1(anchors: list[dict], edges: list[dict], k: int) -> list[list[int]]:
    """The v1 KERNEL-PULL law — /tmp/true_v1.cjs mirrored op for op.

    Each active edge contributes a sweep per endpoint (that anchor's
    kernel, anchor center -> edge midpoint); isolated anchors rest in
    place. The cell mask (anchor cells + bridge cells of active
    diagonal edges) clips everything.
    """
    half = k / 2.0
    allowed = {(a["r"], a["c"]) for a in anchors}
    sweeps = []
    inked = set()
    for e in edges:
        a, b = anchors[e["a"]], anchors[e["b"]]
        if e["diag"]:
            allowed.add((a["r"], b["c"]))
            allowed.add((b["r"], a["c"]))
        for me, other in ((a, b), (b, a)):
            ax = (me["c"] + 0.5) * k
            ay = (me["r"] + 0.5) * k
            mx = ((me["c"] + other["c"]) / 2 + 0.5) * k
            my = ((me["r"] + other["r"]) / 2 + 0.5) * k
            sweeps.append((ax, ay, mx - ax, my - ay, me["quad_mask"]))
            inked.add((me["r"], me["c"]))
    for a in anchors:
        if (a["r"], a["c"]) not in inked:
            sweeps.append(
                ((a["c"] + 0.5) * k, (a["r"] + 0.5) * k, 0.0, 0.0, a["quad_mask"])
            )

    img = [[0] * (COLS * k) for _ in range(ROWS * k)]
    for j in range(ROWS * k):
        y = j + 0.5
        for i in range(COLS * k):
            if (j // k, i // k) not in allowed:
                continue
            x = i + 0.5
            for ax, ay, vx, vy, mask in sweeps:
                l2 = vx * vx + vy * vy
                t = 0.0 if l2 == 0 else max(0.0, min(1.0, ((x - ax) * vx + (y - ay) * vy) / l2))
                if kern_covers(mask, x - (ax + t * vx), y - (ay + t * vy), half):
                    img[j][i] = 1
                    break
    return img


def raster_image_v2(anchors: list[dict], edges: list[dict], k: int) -> list[list[int]]:
    """The v2 CONNECTION-KERNEL law, the full 5k x 7k ink bitmap.

    This mirrors the reference JS raster() in tools/font_editor_gen.py
    and crates/codec/src/px.rs raster_v2 expression for expression, so
    the baked checksums lock all three implementations bit for bit.
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


def ink_count(img: list[list[int]]) -> int:
    return sum(v for row in img for v in row)


HEADER = '''\
//! nx75 — the part-registry anchor font, PARITY-DISPATCHED OPTICAL
//! MASTERS.
//!
//! A first-party 7x5 ANCHOR font for the nano14 id alphabet shipping
//! TWO masters of the same design, dispatched per glyph scale k by
//! [`crate::px`]:
//!
//! - EVEN k renders [`GLYPHS_V1`] under the KERNEL-PULL law (each
//!   active edge sweeps each endpoint's quadrant kernel from the
//!   anchor center to the edge midpoint; isolated anchors rest in
//!   place; a cell mask of anchor cells plus diagonal bridge cells
//!   clips all ink) — the master judged best at even scales.
//! - ODD k renders [`GLYPHS_V2`] under the CONNECTION-KERNEL law
//!   (the union of three bounded stamps: straight-connection
//!   rectangles, corner L1 diamonds clipped to the anti-diagonal
//!   band, and node quadrants; no mask) — the master judged best at
//!   odd scales.
//!
//! Both masters resolve edges and kernels identically at bake time —
//! only the design data and the raster law differ. Checksums lock
//! each master at its own parity: v1 at [`CHECKSUM_KS_V1`], v2 at
//! [`CHECKSUM_KS_V2`].
//!
//! GENERATED FILE — DO NOT EDIT BY HAND.
//! Generated from `design/glyph-font.v1.json` and
//! `design/glyph-font.v2.json` (the sources of truth, authored in
//! the labels/typography-bench font editor) by
//! `tools/bake_glyph_font.py`, which resolves active edges and
//! per-anchor quadrant kernels at bake time.
//! Drift gate: `uv run tools/bake_glyph_font.py --check`

/// Glyph cell height in anchor cells.
pub const GRID_ROWS: u32 = 7;
/// Glyph cell width in anchor cells.
pub const GRID_COLS: u32 = 5;
/// The nano14 id alphabet the font covers (ADR-012: no `0/O/1/I/L`).
pub const ALPHABET: &str = "23456789ABCDEFGHJKMNPQRSTUVWXYZ";
/// Scales with baked v1 ink checksums (even — the parity v1 renders
/// at), in `Glyph::ink_bits` order.
pub const CHECKSUM_KS_V1: [u32; 3] = [2, 4, 6];
/// Scales with baked v2 ink checksums (odd — the parity v2 renders
/// at), in `Glyph::ink_bits` order.
pub const CHECKSUM_KS_V2: [u32; 2] = [3, 5];

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
    /// Ink-pixel counts of the rasterised glyph at this master's
    /// checksum scales ([`CHECKSUM_KS_V1`] for v1, [`CHECKSUM_KS_V2`]
    /// for v2), in order — the bake-time checksums the codec test
    /// suite locks the Rust renderer against.
    pub ink_bits: &'static [u32],
}

/// The v1 (even-k, kernel-pull) glyph record for `c`, or `None`
/// outside [`ALPHABET`].
pub fn glyph_v1(c: char) -> Option<&'static Glyph> {
    GLYPHS_V1.iter().find(|g| g.ch == c)
}

/// The v2 (odd-k, connection-kernel) glyph record for `c`, or `None`
/// outside [`ALPHABET`].
pub fn glyph_v2(c: char) -> Option<&'static Glyph> {
    GLYPHS_V2.iter().find(|g| g.ch == c)
}
'''


def emit_table(name: str, doc: str, data: dict, raster, checksum_ks) -> str:
    out = [f"\n/// {doc}\npub static {name}: [Glyph; 31] = [\n"]
    for ch in ALPHABET:
        anchors, edges = resolve_glyph(data[ch])
        sums = [ink_count(raster(anchors, edges, k)) for k in checksum_ks]
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
        out.append(f"        ink_bits: &[{', '.join(str(s) for s in sums)}],\n")
        out.append("    },\n")
    out.append("];\n")
    return "".join(out)


def emit(v1: dict, v2: dict) -> str:
    return (
        HEADER
        + emit_table(
            "GLYPHS_V1",
            "All 31 v1 (even-k, KERNEL-PULL) glyphs, in [`ALPHABET`] order.",
            v1,
            raster_image_v1,
            CHECKSUM_KS_V1,
        )
        + emit_table(
            "GLYPHS_V2",
            "All 31 v2 (odd-k, CONNECTION-KERNEL) glyphs, in [`ALPHABET`] order.",
            v2,
            raster_image_v2,
            CHECKSUM_KS_V2,
        )
    )


def load_design(path: Path) -> dict:
    data = json.loads(path.read_text())
    if set(data) != set(ALPHABET):
        missing = sorted(set(ALPHABET) - set(data))
        extra = sorted(set(data) - set(ALPHABET))
        sys.exit(
            f"{path.name}: design file alphabet mismatch: "
            f"missing {missing}, extra {extra}"
        )
    for ch, g in data.items():
        px = g["px"]
        if len(px) != ROWS or any(len(row) != COLS for row in px):
            sys.exit(f"{path.name}: glyph {ch}: px is not {ROWS}x{COLS}")
    return data


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--design-v1", type=Path, default=DESIGN_V1)
    ap.add_argument("--design-v2", type=Path, default=DESIGN_V2)
    ap.add_argument("--out", type=Path, default=DEFAULT_OUT)
    ap.add_argument(
        "--check",
        action="store_true",
        help="regenerate to memory and diff against the checked-in file",
    )
    args = ap.parse_args()

    v1 = load_design(args.design_v1)
    v2 = load_design(args.design_v2)

    generated = emit(v1, v2)
    if args.check:
        on_disk = args.out.read_text() if args.out.exists() else ""
        if on_disk != generated:
            print(
                f"DRIFT: {args.out} does not match "
                f"design/{args.design_v1.name} + design/{args.design_v2.name}"
            )
            print("rerun: uv run tools/bake_glyph_font.py")
            return 1
        print(f"ok: {args.out} matches {args.design_v1} + {args.design_v2}")
        return 0
    args.out.write_text(generated)
    print(f"-> {args.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
