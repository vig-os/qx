//! nx75 — the qx anchor font, PARITY-DISPATCHED OPTICAL
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

/// All 31 v1 (even-k, KERNEL-PULL) glyphs, in [`ALPHABET`] order.
pub static GLYPHS_V1: [Glyph; 31] = [
    Glyph {
        ch: '2',
        anchors: &[
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b0100,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 2,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 3,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 4,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 4,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 0,
                b: 3,
                diag: true,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 4,
                diag: true,
            },
            Edge {
                a: 4,
                b: 5,
                diag: false,
            },
            Edge {
                a: 5,
                b: 7,
                diag: true,
            },
            Edge {
                a: 6,
                b: 7,
                diag: false,
            },
            Edge {
                a: 6,
                b: 8,
                diag: true,
            },
            Edge {
                a: 8,
                b: 9,
                diag: true,
            },
            Edge {
                a: 9,
                b: 10,
                diag: false,
            },
            Edge {
                a: 10,
                b: 11,
                diag: false,
            },
            Edge {
                a: 11,
                b: 12,
                diag: false,
            },
            Edge {
                a: 12,
                b: 13,
                diag: false,
            },
            Edge {
                a: 13,
                b: 14,
                diag: false,
            },
        ],
        ink_bits: &[70, 259, 567],
    },
    Glyph {
        ch: '3',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b0010,
            },
            Anchor {
                r: 1,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 2,
                c: 2,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 3,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b0001,
            },
            Anchor {
                r: 5,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 3,
                quad_mask: 0b0000,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 3,
                diag: false,
            },
            Edge {
                a: 3,
                b: 4,
                diag: false,
            },
            Edge {
                a: 4,
                b: 5,
                diag: true,
            },
            Edge {
                a: 5,
                b: 6,
                diag: true,
            },
            Edge {
                a: 6,
                b: 7,
                diag: true,
            },
            Edge {
                a: 7,
                b: 8,
                diag: true,
            },
            Edge {
                a: 8,
                b: 10,
                diag: false,
            },
            Edge {
                a: 9,
                b: 11,
                diag: true,
            },
            Edge {
                a: 10,
                b: 13,
                diag: true,
            },
            Edge {
                a: 11,
                b: 12,
                diag: false,
            },
            Edge {
                a: 12,
                b: 13,
                diag: false,
            },
        ],
        ink_bits: &[67, 244, 531],
    },
    Glyph {
        ch: '4',
        anchors: &[
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b0010,
            },
            Anchor {
                r: 1,
                c: 2,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 1,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 2,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 3,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 3,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 2,
                diag: false,
            },
            Edge {
                a: 0,
                b: 1,
                diag: true,
            },
            Edge {
                a: 1,
                b: 3,
                diag: true,
            },
            Edge {
                a: 2,
                b: 4,
                diag: false,
            },
            Edge {
                a: 3,
                b: 5,
                diag: true,
            },
            Edge {
                a: 4,
                b: 6,
                diag: false,
            },
            Edge {
                a: 5,
                b: 7,
                diag: false,
            },
            Edge {
                a: 6,
                b: 10,
                diag: false,
            },
            Edge {
                a: 7,
                b: 8,
                diag: false,
            },
            Edge {
                a: 8,
                b: 9,
                diag: false,
            },
            Edge {
                a: 9,
                b: 10,
                diag: false,
            },
            Edge {
                a: 10,
                b: 11,
                diag: false,
            },
            Edge {
                a: 10,
                b: 12,
                diag: false,
            },
            Edge {
                a: 12,
                b: 13,
                diag: false,
            },
        ],
        ink_bits: &[61, 233, 516],
    },
    Glyph {
        ch: '5',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 3,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 3,
                quad_mask: 0b0000,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 0,
                b: 5,
                diag: false,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 3,
                diag: false,
            },
            Edge {
                a: 3,
                b: 4,
                diag: false,
            },
            Edge {
                a: 5,
                b: 6,
                diag: false,
            },
            Edge {
                a: 6,
                b: 7,
                diag: false,
            },
            Edge {
                a: 7,
                b: 8,
                diag: false,
            },
            Edge {
                a: 8,
                b: 9,
                diag: false,
            },
            Edge {
                a: 9,
                b: 10,
                diag: true,
            },
            Edge {
                a: 10,
                b: 11,
                diag: false,
            },
            Edge {
                a: 11,
                b: 13,
                diag: false,
            },
            Edge {
                a: 12,
                b: 14,
                diag: true,
            },
            Edge {
                a: 13,
                b: 16,
                diag: true,
            },
            Edge {
                a: 14,
                b: 15,
                diag: false,
            },
            Edge {
                a: 15,
                b: 16,
                diag: false,
            },
        ],
        ink_bits: &[74, 282, 624],
    },
    Glyph {
        ch: '6',
        anchors: &[
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 3,
                quad_mask: 0b0000,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 0,
                b: 2,
                diag: true,
            },
            Edge {
                a: 2,
                b: 3,
                diag: true,
            },
            Edge {
                a: 3,
                b: 4,
                diag: false,
            },
            Edge {
                a: 4,
                b: 5,
                diag: false,
            },
            Edge {
                a: 4,
                b: 8,
                diag: false,
            },
            Edge {
                a: 5,
                b: 6,
                diag: false,
            },
            Edge {
                a: 6,
                b: 7,
                diag: false,
            },
            Edge {
                a: 7,
                b: 9,
                diag: true,
            },
            Edge {
                a: 8,
                b: 10,
                diag: false,
            },
            Edge {
                a: 9,
                b: 11,
                diag: false,
            },
            Edge {
                a: 10,
                b: 12,
                diag: true,
            },
            Edge {
                a: 11,
                b: 14,
                diag: true,
            },
            Edge {
                a: 12,
                b: 13,
                diag: false,
            },
            Edge {
                a: 13,
                b: 14,
                diag: false,
            },
        ],
        ink_bits: &[70, 260, 570],
    },
    Glyph {
        ch: '7',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 2,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 4,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 3,
                diag: false,
            },
            Edge {
                a: 3,
                b: 4,
                diag: false,
            },
            Edge {
                a: 4,
                b: 5,
                diag: false,
            },
            Edge {
                a: 5,
                b: 6,
                diag: true,
            },
            Edge {
                a: 6,
                b: 7,
                diag: true,
            },
            Edge {
                a: 7,
                b: 8,
                diag: true,
            },
            Edge {
                a: 8,
                b: 9,
                diag: false,
            },
            Edge {
                a: 9,
                b: 10,
                diag: false,
            },
        ],
        ink_bits: &[50, 188, 414],
    },
    Glyph {
        ch: '8',
        anchors: &[
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 2,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 3,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 3,
                quad_mask: 0b0000,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 0,
                b: 3,
                diag: true,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 4,
                diag: true,
            },
            Edge {
                a: 3,
                b: 5,
                diag: false,
            },
            Edge {
                a: 4,
                b: 6,
                diag: false,
            },
            Edge {
                a: 5,
                b: 7,
                diag: true,
            },
            Edge {
                a: 6,
                b: 9,
                diag: true,
            },
            Edge {
                a: 7,
                b: 8,
                diag: false,
            },
            Edge {
                a: 7,
                b: 10,
                diag: true,
            },
            Edge {
                a: 8,
                b: 9,
                diag: false,
            },
            Edge {
                a: 9,
                b: 11,
                diag: true,
            },
            Edge {
                a: 10,
                b: 12,
                diag: false,
            },
            Edge {
                a: 11,
                b: 13,
                diag: false,
            },
            Edge {
                a: 12,
                b: 14,
                diag: true,
            },
            Edge {
                a: 13,
                b: 16,
                diag: true,
            },
            Edge {
                a: 14,
                b: 15,
                diag: false,
            },
            Edge {
                a: 15,
                b: 16,
                diag: false,
            },
        ],
        ink_bits: &[84, 308, 672],
    },
    Glyph {
        ch: '9',
        anchors: &[
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 2,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b0000,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 0,
                b: 3,
                diag: true,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 4,
                diag: true,
            },
            Edge {
                a: 3,
                b: 5,
                diag: false,
            },
            Edge {
                a: 4,
                b: 6,
                diag: false,
            },
            Edge {
                a: 5,
                b: 7,
                diag: true,
            },
            Edge {
                a: 6,
                b: 10,
                diag: false,
            },
            Edge {
                a: 7,
                b: 8,
                diag: false,
            },
            Edge {
                a: 8,
                b: 9,
                diag: false,
            },
            Edge {
                a: 9,
                b: 10,
                diag: false,
            },
            Edge {
                a: 10,
                b: 11,
                diag: false,
            },
            Edge {
                a: 11,
                b: 12,
                diag: true,
            },
            Edge {
                a: 12,
                b: 14,
                diag: true,
            },
            Edge {
                a: 13,
                b: 14,
                diag: false,
            },
        ],
        ink_bits: &[70, 260, 570],
    },
    Glyph {
        ch: 'A',
        anchors: &[
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 4,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 0,
                b: 3,
                diag: true,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 4,
                diag: true,
            },
            Edge {
                a: 3,
                b: 5,
                diag: false,
            },
            Edge {
                a: 4,
                b: 6,
                diag: false,
            },
            Edge {
                a: 5,
                b: 7,
                diag: false,
            },
            Edge {
                a: 6,
                b: 8,
                diag: false,
            },
            Edge {
                a: 7,
                b: 9,
                diag: false,
            },
            Edge {
                a: 8,
                b: 13,
                diag: false,
            },
            Edge {
                a: 9,
                b: 10,
                diag: false,
            },
            Edge {
                a: 9,
                b: 14,
                diag: false,
            },
            Edge {
                a: 10,
                b: 11,
                diag: false,
            },
            Edge {
                a: 11,
                b: 12,
                diag: false,
            },
            Edge {
                a: 12,
                b: 13,
                diag: false,
            },
            Edge {
                a: 13,
                b: 15,
                diag: false,
            },
            Edge {
                a: 14,
                b: 16,
                diag: false,
            },
            Edge {
                a: 15,
                b: 17,
                diag: false,
            },
        ],
        ink_bits: &[76, 296, 660],
    },
    Glyph {
        ch: 'B',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 3,
                quad_mask: 0b0000,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 0,
                b: 4,
                diag: false,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 3,
                diag: false,
            },
            Edge {
                a: 3,
                b: 5,
                diag: true,
            },
            Edge {
                a: 4,
                b: 6,
                diag: false,
            },
            Edge {
                a: 5,
                b: 7,
                diag: false,
            },
            Edge {
                a: 6,
                b: 8,
                diag: false,
            },
            Edge {
                a: 7,
                b: 11,
                diag: true,
            },
            Edge {
                a: 8,
                b: 9,
                diag: false,
            },
            Edge {
                a: 8,
                b: 12,
                diag: false,
            },
            Edge {
                a: 9,
                b: 10,
                diag: false,
            },
            Edge {
                a: 10,
                b: 11,
                diag: false,
            },
            Edge {
                a: 11,
                b: 13,
                diag: true,
            },
            Edge {
                a: 12,
                b: 14,
                diag: false,
            },
            Edge {
                a: 13,
                b: 15,
                diag: false,
            },
            Edge {
                a: 14,
                b: 16,
                diag: false,
            },
            Edge {
                a: 15,
                b: 19,
                diag: true,
            },
            Edge {
                a: 16,
                b: 17,
                diag: false,
            },
            Edge {
                a: 17,
                b: 18,
                diag: false,
            },
            Edge {
                a: 18,
                b: 19,
                diag: false,
            },
        ],
        ink_bits: &[88, 338, 750],
    },
    Glyph {
        ch: 'C',
        anchors: &[
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b1000,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 4,
                quad_mask: 0b0010,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 3,
                quad_mask: 0b0000,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 0,
                b: 3,
                diag: true,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 4,
                diag: true,
            },
            Edge {
                a: 3,
                b: 5,
                diag: false,
            },
            Edge {
                a: 5,
                b: 6,
                diag: false,
            },
            Edge {
                a: 6,
                b: 7,
                diag: false,
            },
            Edge {
                a: 7,
                b: 8,
                diag: false,
            },
            Edge {
                a: 8,
                b: 10,
                diag: true,
            },
            Edge {
                a: 9,
                b: 12,
                diag: true,
            },
            Edge {
                a: 10,
                b: 11,
                diag: false,
            },
            Edge {
                a: 11,
                b: 12,
                diag: false,
            },
        ],
        ink_bits: &[60, 222, 486],
    },
    Glyph {
        ch: 'D',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b0000,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 0,
                b: 3,
                diag: false,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 4,
                diag: true,
            },
            Edge {
                a: 3,
                b: 5,
                diag: false,
            },
            Edge {
                a: 4,
                b: 6,
                diag: true,
            },
            Edge {
                a: 5,
                b: 7,
                diag: false,
            },
            Edge {
                a: 6,
                b: 8,
                diag: false,
            },
            Edge {
                a: 7,
                b: 9,
                diag: false,
            },
            Edge {
                a: 8,
                b: 10,
                diag: false,
            },
            Edge {
                a: 9,
                b: 11,
                diag: false,
            },
            Edge {
                a: 10,
                b: 12,
                diag: true,
            },
            Edge {
                a: 11,
                b: 13,
                diag: false,
            },
            Edge {
                a: 12,
                b: 15,
                diag: true,
            },
            Edge {
                a: 13,
                b: 14,
                diag: false,
            },
            Edge {
                a: 14,
                b: 15,
                diag: false,
            },
        ],
        ink_bits: &[72, 272, 600],
    },
    Glyph {
        ch: 'E',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 4,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 0,
                b: 5,
                diag: false,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 3,
                diag: false,
            },
            Edge {
                a: 3,
                b: 4,
                diag: false,
            },
            Edge {
                a: 5,
                b: 6,
                diag: false,
            },
            Edge {
                a: 6,
                b: 7,
                diag: false,
            },
            Edge {
                a: 7,
                b: 8,
                diag: false,
            },
            Edge {
                a: 7,
                b: 11,
                diag: false,
            },
            Edge {
                a: 8,
                b: 9,
                diag: false,
            },
            Edge {
                a: 9,
                b: 10,
                diag: false,
            },
            Edge {
                a: 11,
                b: 12,
                diag: false,
            },
            Edge {
                a: 12,
                b: 13,
                diag: false,
            },
            Edge {
                a: 13,
                b: 14,
                diag: false,
            },
            Edge {
                a: 14,
                b: 15,
                diag: false,
            },
            Edge {
                a: 15,
                b: 16,
                diag: false,
            },
            Edge {
                a: 16,
                b: 17,
                diag: false,
            },
        ],
        ink_bits: &[72, 288, 648],
    },
    Glyph {
        ch: 'F',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 0,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 0,
                b: 5,
                diag: false,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 3,
                diag: false,
            },
            Edge {
                a: 3,
                b: 4,
                diag: false,
            },
            Edge {
                a: 5,
                b: 6,
                diag: false,
            },
            Edge {
                a: 6,
                b: 7,
                diag: false,
            },
            Edge {
                a: 7,
                b: 8,
                diag: false,
            },
            Edge {
                a: 7,
                b: 11,
                diag: false,
            },
            Edge {
                a: 8,
                b: 9,
                diag: false,
            },
            Edge {
                a: 9,
                b: 10,
                diag: false,
            },
            Edge {
                a: 11,
                b: 12,
                diag: false,
            },
            Edge {
                a: 12,
                b: 13,
                diag: false,
            },
        ],
        ink_bits: &[56, 224, 504],
    },
    Glyph {
        ch: 'G',
        anchors: &[
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b1000,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 4,
                quad_mask: 0b0111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 0,
                b: 3,
                diag: true,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 4,
                diag: true,
            },
            Edge {
                a: 3,
                b: 5,
                diag: false,
            },
            Edge {
                a: 5,
                b: 6,
                diag: false,
            },
            Edge {
                a: 6,
                b: 10,
                diag: false,
            },
            Edge {
                a: 7,
                b: 8,
                diag: false,
            },
            Edge {
                a: 8,
                b: 9,
                diag: false,
            },
            Edge {
                a: 9,
                b: 11,
                diag: false,
            },
            Edge {
                a: 10,
                b: 12,
                diag: false,
            },
            Edge {
                a: 11,
                b: 13,
                diag: false,
            },
            Edge {
                a: 12,
                b: 14,
                diag: true,
            },
            Edge {
                a: 13,
                b: 17,
                diag: false,
            },
            Edge {
                a: 14,
                b: 15,
                diag: false,
            },
            Edge {
                a: 15,
                b: 16,
                diag: false,
            },
            Edge {
                a: 16,
                b: 17,
                diag: false,
            },
        ],
        ink_bits: &[78, 297, 657],
    },
    Glyph {
        ch: 'H',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 4,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 2,
                diag: false,
            },
            Edge {
                a: 1,
                b: 3,
                diag: false,
            },
            Edge {
                a: 2,
                b: 4,
                diag: false,
            },
            Edge {
                a: 3,
                b: 5,
                diag: false,
            },
            Edge {
                a: 4,
                b: 6,
                diag: false,
            },
            Edge {
                a: 5,
                b: 10,
                diag: false,
            },
            Edge {
                a: 6,
                b: 7,
                diag: false,
            },
            Edge {
                a: 6,
                b: 11,
                diag: false,
            },
            Edge {
                a: 7,
                b: 8,
                diag: false,
            },
            Edge {
                a: 8,
                b: 9,
                diag: false,
            },
            Edge {
                a: 9,
                b: 10,
                diag: false,
            },
            Edge {
                a: 10,
                b: 12,
                diag: false,
            },
            Edge {
                a: 11,
                b: 13,
                diag: false,
            },
            Edge {
                a: 12,
                b: 14,
                diag: false,
            },
            Edge {
                a: 13,
                b: 15,
                diag: false,
            },
            Edge {
                a: 14,
                b: 16,
                diag: false,
            },
        ],
        ink_bits: &[68, 272, 612],
    },
    Glyph {
        ch: 'J',
        anchors: &[
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b0001,
            },
            Anchor {
                r: 5,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b0000,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 1,
                b: 3,
                diag: false,
            },
            Edge {
                a: 3,
                b: 4,
                diag: false,
            },
            Edge {
                a: 4,
                b: 5,
                diag: false,
            },
            Edge {
                a: 5,
                b: 6,
                diag: false,
            },
            Edge {
                a: 6,
                b: 8,
                diag: false,
            },
            Edge {
                a: 7,
                b: 9,
                diag: true,
            },
            Edge {
                a: 8,
                b: 10,
                diag: true,
            },
            Edge {
                a: 9,
                b: 10,
                diag: false,
            },
        ],
        ink_bits: &[48, 183, 405],
    },
    Glyph {
        ch: 'K',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b0010,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 2,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 2,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 4,
                quad_mask: 0b1000,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 2,
                diag: false,
            },
            Edge {
                a: 1,
                b: 3,
                diag: true,
            },
            Edge {
                a: 2,
                b: 4,
                diag: false,
            },
            Edge {
                a: 3,
                b: 5,
                diag: true,
            },
            Edge {
                a: 4,
                b: 6,
                diag: false,
            },
            Edge {
                a: 5,
                b: 7,
                diag: true,
            },
            Edge {
                a: 6,
                b: 7,
                diag: false,
            },
            Edge {
                a: 6,
                b: 8,
                diag: false,
            },
            Edge {
                a: 7,
                b: 9,
                diag: true,
            },
            Edge {
                a: 8,
                b: 10,
                diag: false,
            },
            Edge {
                a: 9,
                b: 11,
                diag: true,
            },
            Edge {
                a: 10,
                b: 12,
                diag: false,
            },
            Edge {
                a: 11,
                b: 13,
                diag: true,
            },
        ],
        ink_bits: &[68, 248, 540],
    },
    Glyph {
        ch: 'M',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1101,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b0010,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 1,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 2,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 2,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 4,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 2,
                diag: false,
            },
            Edge {
                a: 0,
                b: 3,
                diag: true,
            },
            Edge {
                a: 1,
                b: 5,
                diag: false,
            },
            Edge {
                a: 1,
                b: 4,
                diag: true,
            },
            Edge {
                a: 2,
                b: 6,
                diag: false,
            },
            Edge {
                a: 3,
                b: 7,
                diag: true,
            },
            Edge {
                a: 4,
                b: 7,
                diag: true,
            },
            Edge {
                a: 5,
                b: 8,
                diag: false,
            },
            Edge {
                a: 6,
                b: 9,
                diag: false,
            },
            Edge {
                a: 7,
                b: 10,
                diag: false,
            },
            Edge {
                a: 8,
                b: 11,
                diag: false,
            },
            Edge {
                a: 9,
                b: 12,
                diag: false,
            },
            Edge {
                a: 11,
                b: 13,
                diag: false,
            },
            Edge {
                a: 12,
                b: 14,
                diag: false,
            },
            Edge {
                a: 13,
                b: 15,
                diag: false,
            },
            Edge {
                a: 14,
                b: 16,
                diag: false,
            },
            Edge {
                a: 15,
                b: 17,
                diag: false,
            },
        ],
        ink_bits: &[78, 300, 666],
    },
    Glyph {
        ch: 'N',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 2,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 3,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 4,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 2,
                diag: false,
            },
            Edge {
                a: 1,
                b: 3,
                diag: false,
            },
            Edge {
                a: 2,
                b: 4,
                diag: false,
            },
            Edge {
                a: 2,
                b: 5,
                diag: true,
            },
            Edge {
                a: 3,
                b: 6,
                diag: false,
            },
            Edge {
                a: 4,
                b: 7,
                diag: false,
            },
            Edge {
                a: 5,
                b: 8,
                diag: true,
            },
            Edge {
                a: 6,
                b: 9,
                diag: false,
            },
            Edge {
                a: 7,
                b: 10,
                diag: false,
            },
            Edge {
                a: 8,
                b: 11,
                diag: true,
            },
            Edge {
                a: 9,
                b: 12,
                diag: false,
            },
            Edge {
                a: 10,
                b: 13,
                diag: false,
            },
            Edge {
                a: 11,
                b: 14,
                diag: true,
            },
            Edge {
                a: 12,
                b: 14,
                diag: false,
            },
            Edge {
                a: 13,
                b: 15,
                diag: false,
            },
            Edge {
                a: 14,
                b: 16,
                diag: false,
            },
        ],
        ink_bits: &[74, 284, 630],
    },
    Glyph {
        ch: 'P',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 0,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 0,
                b: 4,
                diag: false,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 3,
                diag: false,
            },
            Edge {
                a: 3,
                b: 5,
                diag: true,
            },
            Edge {
                a: 4,
                b: 6,
                diag: false,
            },
            Edge {
                a: 5,
                b: 7,
                diag: false,
            },
            Edge {
                a: 6,
                b: 8,
                diag: false,
            },
            Edge {
                a: 7,
                b: 11,
                diag: true,
            },
            Edge {
                a: 8,
                b: 9,
                diag: false,
            },
            Edge {
                a: 8,
                b: 12,
                diag: false,
            },
            Edge {
                a: 9,
                b: 10,
                diag: false,
            },
            Edge {
                a: 10,
                b: 11,
                diag: false,
            },
            Edge {
                a: 12,
                b: 13,
                diag: false,
            },
            Edge {
                a: 13,
                b: 14,
                diag: false,
            },
        ],
        ink_bits: &[64, 248, 552],
    },
    Glyph {
        ch: 'Q',
        anchors: &[
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 2,
                quad_mask: 0b0001,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 4,
                quad_mask: 0b1000,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 0,
                b: 3,
                diag: true,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 4,
                diag: true,
            },
            Edge {
                a: 3,
                b: 5,
                diag: false,
            },
            Edge {
                a: 4,
                b: 6,
                diag: false,
            },
            Edge {
                a: 5,
                b: 7,
                diag: false,
            },
            Edge {
                a: 6,
                b: 8,
                diag: false,
            },
            Edge {
                a: 7,
                b: 9,
                diag: false,
            },
            Edge {
                a: 8,
                b: 11,
                diag: false,
            },
            Edge {
                a: 9,
                b: 12,
                diag: false,
            },
            Edge {
                a: 10,
                b: 13,
                diag: true,
            },
            Edge {
                a: 11,
                b: 13,
                diag: true,
            },
            Edge {
                a: 12,
                b: 14,
                diag: true,
            },
            Edge {
                a: 13,
                b: 16,
                diag: true,
            },
            Edge {
                a: 13,
                b: 15,
                diag: true,
            },
            Edge {
                a: 14,
                b: 15,
                diag: false,
            },
        ],
        ink_bits: &[82, 302, 660],
    },
    Glyph {
        ch: 'R',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 2,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 4,
                quad_mask: 0b1000,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 0,
                b: 4,
                diag: false,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 3,
                diag: false,
            },
            Edge {
                a: 3,
                b: 5,
                diag: true,
            },
            Edge {
                a: 4,
                b: 6,
                diag: false,
            },
            Edge {
                a: 5,
                b: 7,
                diag: false,
            },
            Edge {
                a: 6,
                b: 8,
                diag: false,
            },
            Edge {
                a: 7,
                b: 11,
                diag: true,
            },
            Edge {
                a: 8,
                b: 9,
                diag: false,
            },
            Edge {
                a: 8,
                b: 12,
                diag: false,
            },
            Edge {
                a: 9,
                b: 10,
                diag: false,
            },
            Edge {
                a: 9,
                b: 13,
                diag: true,
            },
            Edge {
                a: 10,
                b: 11,
                diag: false,
            },
            Edge {
                a: 12,
                b: 14,
                diag: false,
            },
            Edge {
                a: 13,
                b: 15,
                diag: true,
            },
            Edge {
                a: 14,
                b: 16,
                diag: false,
            },
            Edge {
                a: 15,
                b: 17,
                diag: true,
            },
        ],
        ink_bits: &[81, 305, 672],
    },
    Glyph {
        ch: 'S',
        anchors: &[
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 3,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 3,
                quad_mask: 0b0000,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 0,
                b: 4,
                diag: true,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 3,
                diag: false,
            },
            Edge {
                a: 4,
                b: 5,
                diag: false,
            },
            Edge {
                a: 5,
                b: 6,
                diag: true,
            },
            Edge {
                a: 6,
                b: 7,
                diag: false,
            },
            Edge {
                a: 7,
                b: 8,
                diag: false,
            },
            Edge {
                a: 8,
                b: 9,
                diag: true,
            },
            Edge {
                a: 9,
                b: 10,
                diag: false,
            },
            Edge {
                a: 10,
                b: 14,
                diag: true,
            },
            Edge {
                a: 11,
                b: 12,
                diag: false,
            },
            Edge {
                a: 12,
                b: 13,
                diag: false,
            },
            Edge {
                a: 13,
                b: 14,
                diag: false,
            },
        ],
        ink_bits: &[68, 256, 564],
    },
    Glyph {
        ch: 'T',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 3,
                diag: false,
            },
            Edge {
                a: 2,
                b: 5,
                diag: false,
            },
            Edge {
                a: 3,
                b: 4,
                diag: false,
            },
            Edge {
                a: 5,
                b: 6,
                diag: false,
            },
            Edge {
                a: 6,
                b: 7,
                diag: false,
            },
            Edge {
                a: 7,
                b: 8,
                diag: false,
            },
            Edge {
                a: 8,
                b: 9,
                diag: false,
            },
            Edge {
                a: 9,
                b: 10,
                diag: false,
            },
        ],
        ink_bits: &[44, 176, 396],
    },
    Glyph {
        ch: 'U',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 3,
                quad_mask: 0b0000,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 2,
                diag: false,
            },
            Edge {
                a: 1,
                b: 3,
                diag: false,
            },
            Edge {
                a: 2,
                b: 4,
                diag: false,
            },
            Edge {
                a: 3,
                b: 5,
                diag: false,
            },
            Edge {
                a: 4,
                b: 6,
                diag: false,
            },
            Edge {
                a: 5,
                b: 7,
                diag: false,
            },
            Edge {
                a: 6,
                b: 8,
                diag: false,
            },
            Edge {
                a: 7,
                b: 9,
                diag: false,
            },
            Edge {
                a: 8,
                b: 10,
                diag: false,
            },
            Edge {
                a: 9,
                b: 11,
                diag: false,
            },
            Edge {
                a: 10,
                b: 12,
                diag: true,
            },
            Edge {
                a: 11,
                b: 14,
                diag: true,
            },
            Edge {
                a: 12,
                b: 13,
                diag: false,
            },
            Edge {
                a: 13,
                b: 14,
                diag: false,
            },
        ],
        ink_bits: &[64, 248, 552],
    },
    Glyph {
        ch: 'V',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b0000,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 2,
                diag: false,
            },
            Edge {
                a: 1,
                b: 3,
                diag: false,
            },
            Edge {
                a: 2,
                b: 4,
                diag: false,
            },
            Edge {
                a: 3,
                b: 5,
                diag: false,
            },
            Edge {
                a: 4,
                b: 6,
                diag: false,
            },
            Edge {
                a: 5,
                b: 7,
                diag: false,
            },
            Edge {
                a: 6,
                b: 8,
                diag: false,
            },
            Edge {
                a: 7,
                b: 9,
                diag: false,
            },
            Edge {
                a: 8,
                b: 10,
                diag: true,
            },
            Edge {
                a: 9,
                b: 11,
                diag: true,
            },
            Edge {
                a: 10,
                b: 12,
                diag: true,
            },
            Edge {
                a: 11,
                b: 12,
                diag: true,
            },
        ],
        ink_bits: &[60, 224, 492],
    },
    Glyph {
        ch: 'W',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 2,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 3,
                quad_mask: 0b0000,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 2,
                diag: false,
            },
            Edge {
                a: 1,
                b: 3,
                diag: false,
            },
            Edge {
                a: 2,
                b: 4,
                diag: false,
            },
            Edge {
                a: 3,
                b: 5,
                diag: false,
            },
            Edge {
                a: 4,
                b: 6,
                diag: false,
            },
            Edge {
                a: 5,
                b: 8,
                diag: false,
            },
            Edge {
                a: 6,
                b: 9,
                diag: false,
            },
            Edge {
                a: 7,
                b: 10,
                diag: false,
            },
            Edge {
                a: 8,
                b: 11,
                diag: false,
            },
            Edge {
                a: 9,
                b: 12,
                diag: false,
            },
            Edge {
                a: 10,
                b: 13,
                diag: false,
            },
            Edge {
                a: 11,
                b: 14,
                diag: false,
            },
            Edge {
                a: 12,
                b: 15,
                diag: true,
            },
            Edge {
                a: 13,
                b: 16,
                diag: true,
            },
            Edge {
                a: 13,
                b: 15,
                diag: true,
            },
            Edge {
                a: 14,
                b: 16,
                diag: true,
            },
        ],
        ink_bits: &[76, 290, 642],
    },
    Glyph {
        ch: 'X',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 2,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 2,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 4,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 4,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 4,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 2,
                diag: false,
            },
            Edge {
                a: 1,
                b: 3,
                diag: false,
            },
            Edge {
                a: 2,
                b: 4,
                diag: true,
            },
            Edge {
                a: 3,
                b: 5,
                diag: true,
            },
            Edge {
                a: 4,
                b: 6,
                diag: true,
            },
            Edge {
                a: 5,
                b: 6,
                diag: true,
            },
            Edge {
                a: 6,
                b: 8,
                diag: true,
            },
            Edge {
                a: 6,
                b: 7,
                diag: true,
            },
            Edge {
                a: 7,
                b: 9,
                diag: true,
            },
            Edge {
                a: 8,
                b: 10,
                diag: true,
            },
            Edge {
                a: 9,
                b: 11,
                diag: false,
            },
            Edge {
                a: 10,
                b: 12,
                diag: false,
            },
        ],
        ink_bits: &[68, 244, 528],
    },
    Glyph {
        ch: 'Y',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 2,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 3,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 3,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 4,
                c: 2,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 2,
                diag: false,
            },
            Edge {
                a: 1,
                b: 3,
                diag: false,
            },
            Edge {
                a: 2,
                b: 4,
                diag: false,
            },
            Edge {
                a: 3,
                b: 5,
                diag: false,
            },
            Edge {
                a: 4,
                b: 6,
                diag: true,
            },
            Edge {
                a: 5,
                b: 7,
                diag: true,
            },
            Edge {
                a: 6,
                b: 8,
                diag: true,
            },
            Edge {
                a: 7,
                b: 8,
                diag: true,
            },
            Edge {
                a: 8,
                b: 9,
                diag: false,
            },
            Edge {
                a: 9,
                b: 10,
                diag: false,
            },
        ],
        ink_bits: &[52, 194, 426],
    },
    Glyph {
        ch: 'Z',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 2,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 4,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 4,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 3,
                diag: false,
            },
            Edge {
                a: 3,
                b: 4,
                diag: false,
            },
            Edge {
                a: 4,
                b: 5,
                diag: false,
            },
            Edge {
                a: 5,
                b: 6,
                diag: true,
            },
            Edge {
                a: 6,
                b: 7,
                diag: true,
            },
            Edge {
                a: 7,
                b: 8,
                diag: true,
            },
            Edge {
                a: 8,
                b: 9,
                diag: true,
            },
            Edge {
                a: 9,
                b: 10,
                diag: false,
            },
            Edge {
                a: 10,
                b: 11,
                diag: false,
            },
            Edge {
                a: 11,
                b: 12,
                diag: false,
            },
            Edge {
                a: 12,
                b: 13,
                diag: false,
            },
            Edge {
                a: 13,
                b: 14,
                diag: false,
            },
        ],
        ink_bits: &[68, 256, 564],
    },
];

/// All 31 v2 (odd-k, CONNECTION-KERNEL) glyphs, in [`ALPHABET`] order.
pub static GLYPHS_V2: [Glyph; 31] = [
    Glyph {
        ch: '2',
        anchors: &[
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b0001,
            },
            Anchor {
                r: 6,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 4,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 0,
                b: 3,
                diag: true,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 4,
                diag: true,
            },
            Edge {
                a: 4,
                b: 5,
                diag: false,
            },
            Edge {
                a: 5,
                b: 7,
                diag: true,
            },
            Edge {
                a: 6,
                b: 7,
                diag: false,
            },
            Edge {
                a: 6,
                b: 8,
                diag: true,
            },
            Edge {
                a: 8,
                b: 9,
                diag: true,
            },
            Edge {
                a: 9,
                b: 10,
                diag: false,
            },
            Edge {
                a: 10,
                b: 11,
                diag: false,
            },
            Edge {
                a: 11,
                b: 12,
                diag: false,
            },
            Edge {
                a: 12,
                b: 13,
                diag: false,
            },
            Edge {
                a: 13,
                b: 14,
                diag: false,
            },
        ],
        ink_bits: &[165, 475],
    },
    Glyph {
        ch: '3',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 2,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 3,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 3,
                diag: false,
            },
            Edge {
                a: 3,
                b: 4,
                diag: false,
            },
            Edge {
                a: 4,
                b: 5,
                diag: true,
            },
            Edge {
                a: 5,
                b: 6,
                diag: true,
            },
            Edge {
                a: 6,
                b: 7,
                diag: true,
            },
            Edge {
                a: 7,
                b: 8,
                diag: true,
            },
            Edge {
                a: 8,
                b: 10,
                diag: false,
            },
            Edge {
                a: 9,
                b: 11,
                diag: true,
            },
            Edge {
                a: 10,
                b: 13,
                diag: true,
            },
            Edge {
                a: 11,
                b: 12,
                diag: false,
            },
            Edge {
                a: 12,
                b: 13,
                diag: false,
            },
        ],
        ink_bits: &[158, 456],
    },
    Glyph {
        ch: '4',
        anchors: &[
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 2,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 1,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 2,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 3,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 2,
                diag: false,
            },
            Edge {
                a: 0,
                b: 1,
                diag: true,
            },
            Edge {
                a: 1,
                b: 3,
                diag: true,
            },
            Edge {
                a: 2,
                b: 4,
                diag: false,
            },
            Edge {
                a: 3,
                b: 5,
                diag: true,
            },
            Edge {
                a: 4,
                b: 6,
                diag: false,
            },
            Edge {
                a: 5,
                b: 7,
                diag: false,
            },
            Edge {
                a: 6,
                b: 10,
                diag: false,
            },
            Edge {
                a: 7,
                b: 8,
                diag: false,
            },
            Edge {
                a: 8,
                b: 9,
                diag: false,
            },
            Edge {
                a: 9,
                b: 10,
                diag: false,
            },
            Edge {
                a: 10,
                b: 11,
                diag: false,
            },
            Edge {
                a: 10,
                b: 12,
                diag: false,
            },
            Edge {
                a: 12,
                b: 13,
                diag: false,
            },
        ],
        ink_bits: &[141, 400],
    },
    Glyph {
        ch: '5',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 3,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 0,
                b: 5,
                diag: false,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 3,
                diag: false,
            },
            Edge {
                a: 3,
                b: 4,
                diag: false,
            },
            Edge {
                a: 5,
                b: 6,
                diag: false,
            },
            Edge {
                a: 6,
                b: 7,
                diag: false,
            },
            Edge {
                a: 7,
                b: 8,
                diag: false,
            },
            Edge {
                a: 8,
                b: 9,
                diag: false,
            },
            Edge {
                a: 9,
                b: 10,
                diag: true,
            },
            Edge {
                a: 10,
                b: 11,
                diag: false,
            },
            Edge {
                a: 11,
                b: 13,
                diag: false,
            },
            Edge {
                a: 12,
                b: 14,
                diag: true,
            },
            Edge {
                a: 13,
                b: 16,
                diag: true,
            },
            Edge {
                a: 14,
                b: 15,
                diag: false,
            },
            Edge {
                a: 15,
                b: 16,
                diag: false,
            },
        ],
        ink_bits: &[168, 475],
    },
    Glyph {
        ch: '6',
        anchors: &[
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 3,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 0,
                b: 2,
                diag: true,
            },
            Edge {
                a: 2,
                b: 3,
                diag: true,
            },
            Edge {
                a: 3,
                b: 4,
                diag: false,
            },
            Edge {
                a: 4,
                b: 5,
                diag: false,
            },
            Edge {
                a: 4,
                b: 8,
                diag: false,
            },
            Edge {
                a: 5,
                b: 6,
                diag: false,
            },
            Edge {
                a: 6,
                b: 7,
                diag: false,
            },
            Edge {
                a: 7,
                b: 9,
                diag: true,
            },
            Edge {
                a: 8,
                b: 10,
                diag: false,
            },
            Edge {
                a: 9,
                b: 11,
                diag: false,
            },
            Edge {
                a: 10,
                b: 12,
                diag: true,
            },
            Edge {
                a: 11,
                b: 14,
                diag: true,
            },
            Edge {
                a: 12,
                b: 13,
                diag: false,
            },
            Edge {
                a: 13,
                b: 14,
                diag: false,
            },
        ],
        ink_bits: &[165, 475],
    },
    Glyph {
        ch: '7',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 4,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 3,
                diag: false,
            },
            Edge {
                a: 3,
                b: 4,
                diag: false,
            },
            Edge {
                a: 4,
                b: 5,
                diag: false,
            },
            Edge {
                a: 5,
                b: 6,
                diag: true,
            },
            Edge {
                a: 6,
                b: 7,
                diag: true,
            },
            Edge {
                a: 7,
                b: 8,
                diag: true,
            },
            Edge {
                a: 8,
                b: 9,
                diag: false,
            },
            Edge {
                a: 9,
                b: 10,
                diag: false,
            },
        ],
        ink_bits: &[117, 335],
    },
    Glyph {
        ch: '8',
        anchors: &[
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 3,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 0,
                b: 3,
                diag: true,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 4,
                diag: true,
            },
            Edge {
                a: 3,
                b: 5,
                diag: false,
            },
            Edge {
                a: 4,
                b: 6,
                diag: false,
            },
            Edge {
                a: 5,
                b: 7,
                diag: true,
            },
            Edge {
                a: 6,
                b: 9,
                diag: true,
            },
            Edge {
                a: 7,
                b: 8,
                diag: false,
            },
            Edge {
                a: 7,
                b: 10,
                diag: true,
            },
            Edge {
                a: 8,
                b: 9,
                diag: false,
            },
            Edge {
                a: 9,
                b: 11,
                diag: true,
            },
            Edge {
                a: 10,
                b: 12,
                diag: false,
            },
            Edge {
                a: 11,
                b: 13,
                diag: false,
            },
            Edge {
                a: 12,
                b: 14,
                diag: true,
            },
            Edge {
                a: 13,
                b: 16,
                diag: true,
            },
            Edge {
                a: 14,
                b: 15,
                diag: false,
            },
            Edge {
                a: 15,
                b: 16,
                diag: false,
            },
        ],
        ink_bits: &[199, 577],
    },
    Glyph {
        ch: '9',
        anchors: &[
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 0,
                b: 3,
                diag: true,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 4,
                diag: true,
            },
            Edge {
                a: 3,
                b: 5,
                diag: false,
            },
            Edge {
                a: 4,
                b: 6,
                diag: false,
            },
            Edge {
                a: 5,
                b: 7,
                diag: true,
            },
            Edge {
                a: 6,
                b: 10,
                diag: false,
            },
            Edge {
                a: 7,
                b: 8,
                diag: false,
            },
            Edge {
                a: 8,
                b: 9,
                diag: false,
            },
            Edge {
                a: 9,
                b: 10,
                diag: false,
            },
            Edge {
                a: 10,
                b: 11,
                diag: false,
            },
            Edge {
                a: 11,
                b: 12,
                diag: true,
            },
            Edge {
                a: 12,
                b: 14,
                diag: true,
            },
            Edge {
                a: 13,
                b: 14,
                diag: false,
            },
        ],
        ink_bits: &[165, 475],
    },
    Glyph {
        ch: 'A',
        anchors: &[
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 4,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 0,
                b: 3,
                diag: true,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 4,
                diag: true,
            },
            Edge {
                a: 3,
                b: 5,
                diag: false,
            },
            Edge {
                a: 4,
                b: 6,
                diag: false,
            },
            Edge {
                a: 5,
                b: 7,
                diag: false,
            },
            Edge {
                a: 6,
                b: 8,
                diag: false,
            },
            Edge {
                a: 7,
                b: 9,
                diag: false,
            },
            Edge {
                a: 8,
                b: 13,
                diag: false,
            },
            Edge {
                a: 9,
                b: 10,
                diag: false,
            },
            Edge {
                a: 9,
                b: 14,
                diag: false,
            },
            Edge {
                a: 10,
                b: 11,
                diag: false,
            },
            Edge {
                a: 11,
                b: 12,
                diag: false,
            },
            Edge {
                a: 12,
                b: 13,
                diag: false,
            },
            Edge {
                a: 13,
                b: 15,
                diag: false,
            },
            Edge {
                a: 14,
                b: 16,
                diag: false,
            },
            Edge {
                a: 15,
                b: 17,
                diag: false,
            },
        ],
        ink_bits: &[174, 490],
    },
    Glyph {
        ch: 'B',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 3,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 0,
                b: 4,
                diag: false,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 3,
                diag: false,
            },
            Edge {
                a: 3,
                b: 5,
                diag: true,
            },
            Edge {
                a: 4,
                b: 6,
                diag: false,
            },
            Edge {
                a: 5,
                b: 7,
                diag: false,
            },
            Edge {
                a: 6,
                b: 8,
                diag: false,
            },
            Edge {
                a: 7,
                b: 11,
                diag: true,
            },
            Edge {
                a: 8,
                b: 9,
                diag: false,
            },
            Edge {
                a: 8,
                b: 12,
                diag: false,
            },
            Edge {
                a: 9,
                b: 10,
                diag: false,
            },
            Edge {
                a: 10,
                b: 11,
                diag: false,
            },
            Edge {
                a: 11,
                b: 13,
                diag: true,
            },
            Edge {
                a: 12,
                b: 14,
                diag: false,
            },
            Edge {
                a: 13,
                b: 15,
                diag: false,
            },
            Edge {
                a: 14,
                b: 16,
                diag: false,
            },
            Edge {
                a: 15,
                b: 19,
                diag: true,
            },
            Edge {
                a: 16,
                b: 17,
                diag: false,
            },
            Edge {
                a: 17,
                b: 18,
                diag: false,
            },
            Edge {
                a: 18,
                b: 19,
                diag: false,
            },
        ],
        ink_bits: &[203, 576],
    },
    Glyph {
        ch: 'C',
        anchors: &[
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 3,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 0,
                b: 3,
                diag: true,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 4,
                diag: true,
            },
            Edge {
                a: 3,
                b: 5,
                diag: false,
            },
            Edge {
                a: 5,
                b: 6,
                diag: false,
            },
            Edge {
                a: 6,
                b: 7,
                diag: false,
            },
            Edge {
                a: 7,
                b: 8,
                diag: false,
            },
            Edge {
                a: 8,
                b: 10,
                diag: true,
            },
            Edge {
                a: 9,
                b: 12,
                diag: true,
            },
            Edge {
                a: 10,
                b: 11,
                diag: false,
            },
            Edge {
                a: 11,
                b: 12,
                diag: false,
            },
        ],
        ink_bits: &[141, 405],
    },
    Glyph {
        ch: 'D',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 0,
                b: 3,
                diag: false,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 4,
                diag: true,
            },
            Edge {
                a: 3,
                b: 5,
                diag: false,
            },
            Edge {
                a: 4,
                b: 6,
                diag: true,
            },
            Edge {
                a: 5,
                b: 7,
                diag: false,
            },
            Edge {
                a: 6,
                b: 8,
                diag: false,
            },
            Edge {
                a: 7,
                b: 9,
                diag: false,
            },
            Edge {
                a: 8,
                b: 10,
                diag: false,
            },
            Edge {
                a: 9,
                b: 11,
                diag: false,
            },
            Edge {
                a: 10,
                b: 12,
                diag: true,
            },
            Edge {
                a: 11,
                b: 13,
                diag: false,
            },
            Edge {
                a: 12,
                b: 15,
                diag: true,
            },
            Edge {
                a: 13,
                b: 14,
                diag: false,
            },
            Edge {
                a: 14,
                b: 15,
                diag: false,
            },
        ],
        ink_bits: &[168, 480],
    },
    Glyph {
        ch: 'E',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 4,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 0,
                b: 5,
                diag: false,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 3,
                diag: false,
            },
            Edge {
                a: 3,
                b: 4,
                diag: false,
            },
            Edge {
                a: 5,
                b: 6,
                diag: false,
            },
            Edge {
                a: 6,
                b: 7,
                diag: false,
            },
            Edge {
                a: 7,
                b: 8,
                diag: false,
            },
            Edge {
                a: 7,
                b: 11,
                diag: false,
            },
            Edge {
                a: 8,
                b: 9,
                diag: false,
            },
            Edge {
                a: 9,
                b: 10,
                diag: false,
            },
            Edge {
                a: 11,
                b: 12,
                diag: false,
            },
            Edge {
                a: 12,
                b: 13,
                diag: false,
            },
            Edge {
                a: 13,
                b: 14,
                diag: false,
            },
            Edge {
                a: 14,
                b: 15,
                diag: false,
            },
            Edge {
                a: 15,
                b: 16,
                diag: false,
            },
            Edge {
                a: 16,
                b: 17,
                diag: false,
            },
        ],
        ink_bits: &[162, 450],
    },
    Glyph {
        ch: 'F',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 0,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 0,
                b: 5,
                diag: false,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 3,
                diag: false,
            },
            Edge {
                a: 3,
                b: 4,
                diag: false,
            },
            Edge {
                a: 5,
                b: 6,
                diag: false,
            },
            Edge {
                a: 6,
                b: 7,
                diag: false,
            },
            Edge {
                a: 7,
                b: 8,
                diag: false,
            },
            Edge {
                a: 7,
                b: 11,
                diag: false,
            },
            Edge {
                a: 8,
                b: 9,
                diag: false,
            },
            Edge {
                a: 9,
                b: 10,
                diag: false,
            },
            Edge {
                a: 11,
                b: 12,
                diag: false,
            },
            Edge {
                a: 12,
                b: 13,
                diag: false,
            },
        ],
        ink_bits: &[126, 350],
    },
    Glyph {
        ch: 'G',
        anchors: &[
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 4,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 0,
                b: 3,
                diag: true,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 4,
                diag: true,
            },
            Edge {
                a: 3,
                b: 5,
                diag: false,
            },
            Edge {
                a: 5,
                b: 6,
                diag: false,
            },
            Edge {
                a: 6,
                b: 10,
                diag: false,
            },
            Edge {
                a: 7,
                b: 8,
                diag: false,
            },
            Edge {
                a: 8,
                b: 9,
                diag: false,
            },
            Edge {
                a: 9,
                b: 11,
                diag: false,
            },
            Edge {
                a: 10,
                b: 12,
                diag: false,
            },
            Edge {
                a: 11,
                b: 13,
                diag: false,
            },
            Edge {
                a: 12,
                b: 14,
                diag: true,
            },
            Edge {
                a: 13,
                b: 17,
                diag: false,
            },
            Edge {
                a: 14,
                b: 15,
                diag: false,
            },
            Edge {
                a: 15,
                b: 16,
                diag: false,
            },
            Edge {
                a: 16,
                b: 17,
                diag: false,
            },
        ],
        ink_bits: &[180, 510],
    },
    Glyph {
        ch: 'H',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 4,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 2,
                diag: false,
            },
            Edge {
                a: 1,
                b: 3,
                diag: false,
            },
            Edge {
                a: 2,
                b: 4,
                diag: false,
            },
            Edge {
                a: 3,
                b: 5,
                diag: false,
            },
            Edge {
                a: 4,
                b: 6,
                diag: false,
            },
            Edge {
                a: 5,
                b: 10,
                diag: false,
            },
            Edge {
                a: 6,
                b: 7,
                diag: false,
            },
            Edge {
                a: 6,
                b: 11,
                diag: false,
            },
            Edge {
                a: 7,
                b: 8,
                diag: false,
            },
            Edge {
                a: 8,
                b: 9,
                diag: false,
            },
            Edge {
                a: 9,
                b: 10,
                diag: false,
            },
            Edge {
                a: 10,
                b: 12,
                diag: false,
            },
            Edge {
                a: 11,
                b: 13,
                diag: false,
            },
            Edge {
                a: 12,
                b: 14,
                diag: false,
            },
            Edge {
                a: 13,
                b: 15,
                diag: false,
            },
            Edge {
                a: 14,
                b: 16,
                diag: false,
            },
        ],
        ink_bits: &[153, 425],
    },
    Glyph {
        ch: 'J',
        anchors: &[
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 1,
                b: 3,
                diag: false,
            },
            Edge {
                a: 3,
                b: 4,
                diag: false,
            },
            Edge {
                a: 4,
                b: 5,
                diag: false,
            },
            Edge {
                a: 5,
                b: 6,
                diag: false,
            },
            Edge {
                a: 6,
                b: 8,
                diag: false,
            },
            Edge {
                a: 7,
                b: 9,
                diag: true,
            },
            Edge {
                a: 8,
                b: 10,
                diag: true,
            },
            Edge {
                a: 9,
                b: 10,
                diag: false,
            },
        ],
        ink_bits: &[111, 315],
    },
    Glyph {
        ch: 'K',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 2,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 2,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 4,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 2,
                diag: false,
            },
            Edge {
                a: 1,
                b: 3,
                diag: true,
            },
            Edge {
                a: 2,
                b: 4,
                diag: false,
            },
            Edge {
                a: 3,
                b: 5,
                diag: true,
            },
            Edge {
                a: 4,
                b: 6,
                diag: false,
            },
            Edge {
                a: 5,
                b: 7,
                diag: true,
            },
            Edge {
                a: 6,
                b: 7,
                diag: false,
            },
            Edge {
                a: 6,
                b: 8,
                diag: false,
            },
            Edge {
                a: 7,
                b: 9,
                diag: true,
            },
            Edge {
                a: 8,
                b: 10,
                diag: false,
            },
            Edge {
                a: 9,
                b: 11,
                diag: true,
            },
            Edge {
                a: 10,
                b: 12,
                diag: false,
            },
            Edge {
                a: 11,
                b: 13,
                diag: true,
            },
        ],
        ink_bits: &[161, 466],
    },
    Glyph {
        ch: 'M',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 1,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 3,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 4,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 2,
                diag: false,
            },
            Edge {
                a: 0,
                b: 3,
                diag: true,
            },
            Edge {
                a: 1,
                b: 5,
                diag: false,
            },
            Edge {
                a: 1,
                b: 4,
                diag: true,
            },
            Edge {
                a: 2,
                b: 6,
                diag: false,
            },
            Edge {
                a: 3,
                b: 7,
                diag: true,
            },
            Edge {
                a: 4,
                b: 7,
                diag: true,
            },
            Edge {
                a: 5,
                b: 8,
                diag: false,
            },
            Edge {
                a: 6,
                b: 9,
                diag: false,
            },
            Edge {
                a: 7,
                b: 10,
                diag: false,
            },
            Edge {
                a: 8,
                b: 11,
                diag: false,
            },
            Edge {
                a: 9,
                b: 12,
                diag: false,
            },
            Edge {
                a: 11,
                b: 13,
                diag: false,
            },
            Edge {
                a: 12,
                b: 14,
                diag: false,
            },
            Edge {
                a: 13,
                b: 15,
                diag: false,
            },
            Edge {
                a: 14,
                b: 16,
                diag: false,
            },
            Edge {
                a: 15,
                b: 17,
                diag: false,
            },
        ],
        ink_bits: &[176, 496],
    },
    Glyph {
        ch: 'N',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 2,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 3,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 4,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 4,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 2,
                diag: false,
            },
            Edge {
                a: 1,
                b: 3,
                diag: false,
            },
            Edge {
                a: 2,
                b: 4,
                diag: false,
            },
            Edge {
                a: 2,
                b: 5,
                diag: true,
            },
            Edge {
                a: 3,
                b: 6,
                diag: false,
            },
            Edge {
                a: 4,
                b: 7,
                diag: false,
            },
            Edge {
                a: 5,
                b: 8,
                diag: true,
            },
            Edge {
                a: 6,
                b: 9,
                diag: false,
            },
            Edge {
                a: 7,
                b: 10,
                diag: false,
            },
            Edge {
                a: 8,
                b: 11,
                diag: true,
            },
            Edge {
                a: 9,
                b: 12,
                diag: false,
            },
            Edge {
                a: 10,
                b: 13,
                diag: false,
            },
            Edge {
                a: 11,
                b: 14,
                diag: true,
            },
            Edge {
                a: 12,
                b: 14,
                diag: false,
            },
            Edge {
                a: 13,
                b: 15,
                diag: false,
            },
            Edge {
                a: 14,
                b: 16,
                diag: false,
            },
        ],
        ink_bits: &[171, 485],
    },
    Glyph {
        ch: 'P',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 0,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 0,
                b: 4,
                diag: false,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 3,
                diag: false,
            },
            Edge {
                a: 3,
                b: 5,
                diag: true,
            },
            Edge {
                a: 4,
                b: 6,
                diag: false,
            },
            Edge {
                a: 5,
                b: 7,
                diag: false,
            },
            Edge {
                a: 6,
                b: 8,
                diag: false,
            },
            Edge {
                a: 7,
                b: 11,
                diag: true,
            },
            Edge {
                a: 8,
                b: 9,
                diag: false,
            },
            Edge {
                a: 8,
                b: 12,
                diag: false,
            },
            Edge {
                a: 9,
                b: 10,
                diag: false,
            },
            Edge {
                a: 10,
                b: 11,
                diag: false,
            },
            Edge {
                a: 12,
                b: 13,
                diag: false,
            },
            Edge {
                a: 13,
                b: 14,
                diag: false,
            },
        ],
        ink_bits: &[147, 415],
    },
    Glyph {
        ch: 'Q',
        anchors: &[
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 4,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 0,
                b: 3,
                diag: true,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 4,
                diag: true,
            },
            Edge {
                a: 3,
                b: 5,
                diag: false,
            },
            Edge {
                a: 4,
                b: 6,
                diag: false,
            },
            Edge {
                a: 5,
                b: 7,
                diag: false,
            },
            Edge {
                a: 6,
                b: 8,
                diag: false,
            },
            Edge {
                a: 7,
                b: 9,
                diag: false,
            },
            Edge {
                a: 8,
                b: 11,
                diag: false,
            },
            Edge {
                a: 9,
                b: 12,
                diag: false,
            },
            Edge {
                a: 10,
                b: 13,
                diag: true,
            },
            Edge {
                a: 11,
                b: 13,
                diag: true,
            },
            Edge {
                a: 12,
                b: 14,
                diag: true,
            },
            Edge {
                a: 13,
                b: 16,
                diag: true,
            },
            Edge {
                a: 13,
                b: 15,
                diag: true,
            },
            Edge {
                a: 14,
                b: 15,
                diag: false,
            },
        ],
        ink_bits: &[191, 549],
    },
    Glyph {
        ch: 'R',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 2,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 4,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 0,
                b: 4,
                diag: false,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 3,
                diag: false,
            },
            Edge {
                a: 3,
                b: 5,
                diag: true,
            },
            Edge {
                a: 4,
                b: 6,
                diag: false,
            },
            Edge {
                a: 5,
                b: 7,
                diag: false,
            },
            Edge {
                a: 6,
                b: 8,
                diag: false,
            },
            Edge {
                a: 7,
                b: 11,
                diag: true,
            },
            Edge {
                a: 8,
                b: 9,
                diag: false,
            },
            Edge {
                a: 8,
                b: 12,
                diag: false,
            },
            Edge {
                a: 9,
                b: 10,
                diag: false,
            },
            Edge {
                a: 9,
                b: 13,
                diag: true,
            },
            Edge {
                a: 10,
                b: 11,
                diag: false,
            },
            Edge {
                a: 12,
                b: 14,
                diag: false,
            },
            Edge {
                a: 13,
                b: 15,
                diag: true,
            },
            Edge {
                a: 14,
                b: 16,
                diag: false,
            },
            Edge {
                a: 15,
                b: 17,
                diag: true,
            },
        ],
        ink_bits: &[189, 540],
    },
    Glyph {
        ch: 'S',
        anchors: &[
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 3,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 0,
                b: 4,
                diag: true,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 3,
                diag: false,
            },
            Edge {
                a: 4,
                b: 5,
                diag: false,
            },
            Edge {
                a: 5,
                b: 6,
                diag: true,
            },
            Edge {
                a: 6,
                b: 7,
                diag: false,
            },
            Edge {
                a: 7,
                b: 8,
                diag: false,
            },
            Edge {
                a: 8,
                b: 9,
                diag: true,
            },
            Edge {
                a: 9,
                b: 10,
                diag: false,
            },
            Edge {
                a: 10,
                b: 14,
                diag: true,
            },
            Edge {
                a: 11,
                b: 12,
                diag: false,
            },
            Edge {
                a: 12,
                b: 13,
                diag: false,
            },
            Edge {
                a: 13,
                b: 14,
                diag: false,
            },
        ],
        ink_bits: &[159, 455],
    },
    Glyph {
        ch: 'T',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 3,
                diag: false,
            },
            Edge {
                a: 2,
                b: 5,
                diag: false,
            },
            Edge {
                a: 3,
                b: 4,
                diag: false,
            },
            Edge {
                a: 5,
                b: 6,
                diag: false,
            },
            Edge {
                a: 6,
                b: 7,
                diag: false,
            },
            Edge {
                a: 7,
                b: 8,
                diag: false,
            },
            Edge {
                a: 8,
                b: 9,
                diag: false,
            },
            Edge {
                a: 9,
                b: 10,
                diag: false,
            },
        ],
        ink_bits: &[99, 275],
    },
    Glyph {
        ch: 'U',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 3,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 2,
                diag: false,
            },
            Edge {
                a: 1,
                b: 3,
                diag: false,
            },
            Edge {
                a: 2,
                b: 4,
                diag: false,
            },
            Edge {
                a: 3,
                b: 5,
                diag: false,
            },
            Edge {
                a: 4,
                b: 6,
                diag: false,
            },
            Edge {
                a: 5,
                b: 7,
                diag: false,
            },
            Edge {
                a: 6,
                b: 8,
                diag: false,
            },
            Edge {
                a: 7,
                b: 9,
                diag: false,
            },
            Edge {
                a: 8,
                b: 10,
                diag: false,
            },
            Edge {
                a: 9,
                b: 11,
                diag: false,
            },
            Edge {
                a: 10,
                b: 12,
                diag: true,
            },
            Edge {
                a: 11,
                b: 14,
                diag: true,
            },
            Edge {
                a: 12,
                b: 13,
                diag: false,
            },
            Edge {
                a: 13,
                b: 14,
                diag: false,
            },
        ],
        ink_bits: &[147, 415],
    },
    Glyph {
        ch: 'V',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 2,
                diag: false,
            },
            Edge {
                a: 1,
                b: 3,
                diag: false,
            },
            Edge {
                a: 2,
                b: 4,
                diag: false,
            },
            Edge {
                a: 3,
                b: 5,
                diag: false,
            },
            Edge {
                a: 4,
                b: 6,
                diag: false,
            },
            Edge {
                a: 5,
                b: 7,
                diag: false,
            },
            Edge {
                a: 6,
                b: 8,
                diag: false,
            },
            Edge {
                a: 7,
                b: 9,
                diag: false,
            },
            Edge {
                a: 8,
                b: 10,
                diag: true,
            },
            Edge {
                a: 9,
                b: 11,
                diag: true,
            },
            Edge {
                a: 10,
                b: 12,
                diag: true,
            },
            Edge {
                a: 11,
                b: 12,
                diag: true,
            },
        ],
        ink_bits: &[140, 401],
    },
    Glyph {
        ch: 'W',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 4,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 2,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 3,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 2,
                diag: false,
            },
            Edge {
                a: 1,
                b: 3,
                diag: false,
            },
            Edge {
                a: 2,
                b: 4,
                diag: false,
            },
            Edge {
                a: 3,
                b: 5,
                diag: false,
            },
            Edge {
                a: 4,
                b: 6,
                diag: false,
            },
            Edge {
                a: 5,
                b: 8,
                diag: false,
            },
            Edge {
                a: 6,
                b: 9,
                diag: false,
            },
            Edge {
                a: 7,
                b: 10,
                diag: false,
            },
            Edge {
                a: 8,
                b: 11,
                diag: false,
            },
            Edge {
                a: 9,
                b: 12,
                diag: false,
            },
            Edge {
                a: 10,
                b: 13,
                diag: false,
            },
            Edge {
                a: 11,
                b: 14,
                diag: false,
            },
            Edge {
                a: 12,
                b: 15,
                diag: true,
            },
            Edge {
                a: 13,
                b: 16,
                diag: true,
            },
            Edge {
                a: 13,
                b: 15,
                diag: true,
            },
            Edge {
                a: 14,
                b: 16,
                diag: true,
            },
        ],
        ink_bits: &[174, 493],
    },
    Glyph {
        ch: 'X',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 2,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 4,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 4,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 5,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 4,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 2,
                diag: false,
            },
            Edge {
                a: 1,
                b: 3,
                diag: false,
            },
            Edge {
                a: 2,
                b: 4,
                diag: true,
            },
            Edge {
                a: 3,
                b: 5,
                diag: true,
            },
            Edge {
                a: 4,
                b: 6,
                diag: true,
            },
            Edge {
                a: 5,
                b: 6,
                diag: true,
            },
            Edge {
                a: 6,
                b: 8,
                diag: true,
            },
            Edge {
                a: 6,
                b: 7,
                diag: true,
            },
            Edge {
                a: 7,
                b: 9,
                diag: true,
            },
            Edge {
                a: 8,
                b: 10,
                diag: true,
            },
            Edge {
                a: 9,
                b: 11,
                diag: false,
            },
            Edge {
                a: 10,
                b: 12,
                diag: false,
            },
        ],
        ink_bits: &[161, 469],
    },
    Glyph {
        ch: 'Y',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 3,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 3,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 4,
                c: 2,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 2,
                diag: false,
            },
            Edge {
                a: 1,
                b: 3,
                diag: false,
            },
            Edge {
                a: 2,
                b: 4,
                diag: false,
            },
            Edge {
                a: 3,
                b: 5,
                diag: false,
            },
            Edge {
                a: 4,
                b: 6,
                diag: true,
            },
            Edge {
                a: 5,
                b: 7,
                diag: true,
            },
            Edge {
                a: 6,
                b: 8,
                diag: true,
            },
            Edge {
                a: 7,
                b: 8,
                diag: true,
            },
            Edge {
                a: 8,
                b: 9,
                diag: false,
            },
            Edge {
                a: 9,
                b: 10,
                diag: false,
            },
        ],
        ink_bits: &[122, 351],
    },
    Glyph {
        ch: 'Z',
        anchors: &[
            Anchor {
                r: 0,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 0,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 1,
                c: 4,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 2,
                c: 3,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 3,
                c: 2,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 4,
                c: 1,
                quad_mask: 0b0000,
            },
            Anchor {
                r: 5,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 0,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 1,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 2,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 3,
                quad_mask: 0b1111,
            },
            Anchor {
                r: 6,
                c: 4,
                quad_mask: 0b1111,
            },
        ],
        edges: &[
            Edge {
                a: 0,
                b: 1,
                diag: false,
            },
            Edge {
                a: 1,
                b: 2,
                diag: false,
            },
            Edge {
                a: 2,
                b: 3,
                diag: false,
            },
            Edge {
                a: 3,
                b: 4,
                diag: false,
            },
            Edge {
                a: 4,
                b: 5,
                diag: false,
            },
            Edge {
                a: 5,
                b: 6,
                diag: true,
            },
            Edge {
                a: 6,
                b: 7,
                diag: true,
            },
            Edge {
                a: 7,
                b: 8,
                diag: true,
            },
            Edge {
                a: 8,
                b: 9,
                diag: true,
            },
            Edge {
                a: 9,
                b: 10,
                diag: false,
            },
            Edge {
                a: 10,
                b: 11,
                diag: false,
            },
            Edge {
                a: 11,
                b: 12,
                diag: false,
            },
            Edge {
                a: 12,
                b: 13,
                diag: false,
            },
            Edge {
                a: 13,
                b: 14,
                diag: false,
            },
        ],
        ink_bits: &[159, 455],
    },
];
