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
        ink_bits: [70, 165, 300, 690],
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
        ink_bits: [67, 158, 288, 663],
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
        ink_bits: [61, 141, 254, 579],
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
        ink_bits: [73, 168, 302, 687],
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
        ink_bits: [70, 165, 300, 690],
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
        ink_bits: [50, 117, 212, 486],
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
        ink_bits: [84, 199, 364, 840],
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
        ink_bits: [70, 165, 300, 690],
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
        ink_bits: [76, 174, 312, 708],
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
        ink_bits: [88, 203, 360, 816],
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
        ink_bits: [60, 141, 256, 588],
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
        ink_bits: [72, 168, 304, 696],
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
        ink_bits: [72, 162, 288, 648],
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
        ink_bits: [56, 126, 224, 504],
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
        ink_bits: [78, 180, 324, 738],
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
        ink_bits: [68, 153, 272, 612],
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
        ink_bits: [48, 111, 200, 456],
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
        ink_bits: [68, 161, 294, 678],
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
        ink_bits: [78, 179, 322, 732],
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
        ink_bits: [74, 171, 308, 702],
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
        ink_bits: [64, 147, 264, 600],
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
        ink_bits: [82, 191, 348, 798],
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
        ink_bits: [81, 189, 342, 783],
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
        ink_bits: [68, 159, 288, 660],
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
        ink_bits: [44, 99, 176, 396],
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
        ink_bits: [64, 147, 264, 600],
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
        ink_bits: [60, 140, 254, 582],
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
        ink_bits: [76, 174, 314, 714],
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
        ink_bits: [68, 161, 296, 684],
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
        ink_bits: [52, 122, 222, 510],
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
        ink_bits: [68, 159, 288, 660],
    },
];
