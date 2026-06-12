//! px-true label renderer (ADR-031 §2–§4, final geometry per §8).
//!
//! The mm-native renderer ([`crate::svg`]) is physically sized but lets
//! the print driver decide where module edges fall; on a thermal head a
//! sub-pixel module edge merges or drops dots and can kill a Micro QR.
//! This module renders in **device pixels** with the ADR-031 §8 law
//! (2026-06-11): the caller asks for an **exact output canvas**, a
//! **per-side padding floor** ([`Padding`], CSS clockwise), and a
//! [`crate::Symbology`]; padding references the QR's *module part*
//! (the data modules, quiet zone excluded) and the module size is
//! *deduced* per [`PaddingMode`], per axis:
//!
//! ```text
//! floor_side = max(pad_side, quiet·m)   (overlap, default)
//!              quiet·m + pad_side       (additive)
//!              pad_side                 (clip)
//! controlling axis (height for horz, width for vert):
//!   max m with data·m + floor_a + floor_b ≤ size  → ERROR if no m ≥ 1
//!   remainder distributes on top of the floors (extra px bottom/right)
//! non-controlling sides: white_side = floor_side exactly
//! ```
//!
//! In `overlap` mode the quiet zone's whitespace satisfies padding
//! (printers donate intrinsic outer margins, so the label spends its
//! pixels on modules); `white ≥ quiet·m` is structural there. In
//! `additive` mode the quiet zone is excluded from outside padding
//! (full-bleed/die-cut contexts). In `clip` mode the artifact reserves
//! no quiet zone at all — and the text side carries the §8 safe-space
//! clamp, `gap = max(round(1.5·white_side), quiet·m)`, so typography
//! can never invade the safe space regardless of mode or padding.
//!
//! Geometry, all derived from that one deduction (§8):
//! - The label's controlling dimension (height for `horz`, width for
//!   `vert`) is **exactly** `size`; an odd remainder leaves its extra
//!   pixel on the bottom/right edge — deterministic.
//! - QR→text gap = `max(round(1.5 · white), quiet·m)` over the white
//!   on the QR's text side (right for `horz`, bottom for `vert`).
//! - The id-text is **bitmap typography rendered as rects**: no
//!   `<text>`, no fonts, no rasterizer variance. The glyphs are the
//!   first-party **nx75 anchor font** ([`crate::glyph_font`]) in its
//!   PARITY-DISPATCHED OPTICAL MASTERS form, rasterised here at
//!   integer scale `k = glyph_px` (see [`raster_glyph`]): EVEN k
//!   renders the v1 master (`design/glyph-font.v1.json`) under the
//!   KERNEL-PULL law ([`raster_v1`]) and ODD k renders the v2 master
//!   (`design/glyph-font.v2.json`) under the CONNECTION-KERNEL law
//!   ([`raster_v2`]) — the master each parity was judged best at.
//!   Ink emits as horizontal-run `<rect>`s in the same crispEdges
//!   group as the QR modules.
//! - Text-block placement is the g-law: the glyph scale `g` is the
//!   largest integer with the block `rows·7g + (rows−1)·g` inside
//!   `data_px`, capped at `module_px` (text dots never coarser than
//!   QR dots); advance `6g`, row pitch `8g`. `horz` centers the
//!   block vertically in the module part; `vert` centers each row
//!   horizontally in it.
//! - Modules and glyphs draw at their canvas offsets on a `crispEdges`
//!   grid; the quiet zone is **not** drawn — the white background
//!   supplies it.
//!
//! Integer-px structure proven on hardware by
//! `tools/printer_test_62mm.py` (696 px = 62 mm tape) and the ADR-031
//! fast-path prototype.
//!
//! [`fill_to_max`] additionally grows every label in a job to the
//! batch's largest footprint so a mixed batch (Micro + Standard,
//! different payloads) comes out physically uniform (ADR-031 §4).

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::color::{default_bg, default_fg, Color};
use crate::format::TextFormat;
use crate::glyph_font::{self, Glyph};
use crate::receipt::{metadata_element, Receipt};
use crate::solver::IdBlock;
use crate::svg::Layout;
use crate::symbology::Symbology;
use crate::CodecError;

/// Per-side padding floor in device px, CSS clockwise (ADR-031 §8).
/// Each side is a *minimum* canvas-edge → module-part margin; the
/// quiet zone counts toward it per [`PaddingMode`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Padding {
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
    pub left: u32,
}

impl Padding {
    /// `2` — the same floor on all four sides.
    pub const fn uniform(all: u32) -> Self {
        Self::sides(all, all, all, all)
    }

    /// `2,6` — vertical (top/bottom), horizontal (right/left).
    pub const fn axes(vertical: u32, horizontal: u32) -> Self {
        Self::sides(vertical, horizontal, vertical, horizontal)
    }

    /// `2,6,4,6` — top, right, bottom, left (CSS clockwise).
    pub const fn sides(top: u32, right: u32, bottom: u32, left: u32) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }
}

/// How the quiet zone counts toward the outside padding floor
/// (ADR-031 §8).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PaddingMode {
    /// The quiet zone counts toward outside padding — each side's
    /// white floor is `max(pad_side, quiet·m)`. Default: printers
    /// contribute intrinsic unprintable margins, so the device already
    /// donates outer white.
    #[default]
    Overlap,
    /// The quiet zone is excluded from outside padding — each side's
    /// white floor is `quiet·m + pad_side` — for full-bleed/die-cut
    /// contexts where the canvas edge is the physical edge.
    Additive,
    /// The maximizer (ADR-031 §8): the artifact reserves NO quiet zone
    /// at all — each side's white floor is `pad_side` alone — because
    /// the printer's intrinsic unreducible white (cut-feed margin,
    /// unprintable side margins) supplies the safe space physically.
    /// The decode guarantee transfers to the declared physical context
    /// (printer profiles verify `intrinsic margin ≥ quiet·m` once they
    /// land).
    Clip,
}

impl PaddingMode {
    fn name(self) -> &'static str {
        match self {
            PaddingMode::Overlap => "overlap",
            PaddingMode::Additive => "additive",
            PaddingMode::Clip => "clip",
        }
    }

    /// One side's white floor in px for `m` px/module under a
    /// `quiet`-module quiet zone.
    fn floor_px(self, pad_side: u32, quiet: u32, m: u32) -> u32 {
        let quiet_px = quiet.saturating_mul(m);
        match self {
            PaddingMode::Overlap => pad_side.max(quiet_px),
            PaddingMode::Additive => quiet_px.saturating_add(pad_side),
            PaddingMode::Clip => pad_side,
        }
    }

    /// Minimum canvas (controlling dimension) for `m` px/module given
    /// a `data`-module symbol, a `quiet`-module quiet zone, and the
    /// two padding floors of the controlling axis.
    fn min_size(self, data: u32, quiet: u32, pad_a: u32, pad_b: u32, m: u32) -> u64 {
        u64::from(data) * u64::from(m)
            + u64::from(self.floor_px(pad_a, quiet, m))
            + u64::from(self.floor_px(pad_b, quiet, m))
    }
}

/// Whether `size_px` is the EXACT canvas (the §2/§8 law) or an UPPER
/// BOUND that the canvas snaps down from (ADR-031 §8 `--size-mode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SizeMode {
    /// `exact` (default) — the canvas holds the requested size; the
    /// lattice remainder distributes into auto padding. Required for
    /// batch uniformity and fixed-slot placement (§8).
    #[default]
    Exact,
    /// `snap` — `size_px` is an UPPER BOUND; the canvas snaps DOWN to
    /// the content lattice (deduced module geometry + declared padding
    /// floors, remainder omitted; §8).
    Snap,
}

impl SizeMode {
    fn name(self) -> &'static str {
        match self {
            SizeMode::Exact => "exact",
            SizeMode::Snap => "snap",
        }
    }
}

/// The render-payload arrangement (stage 1: which elements ride and in
/// what order). Stage 1 supports the three layout cases exhaustively:
/// `qr` only, `id` only, and the two qr+id arrangements.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PayloadShape {
    /// `qr id` — QR first along the axis, id text second.
    #[default]
    QrId,
    /// `id qr` — id text first, QR second.
    IdQr,
    /// `qr` — QR only; no id text.
    QrOnly,
    /// `id` — id text only; the canvas is the text budget (no QR).
    IdOnly,
}

/// Render options for [`render_label_px_with_opts`].
#[derive(Clone, Debug)]
pub struct RenderOpts {
    /// Foreground (modules + glyph ink).
    pub fg: Color,
    /// Background. `bg.is_none()` omits the background rect.
    pub bg: Color,
    /// `exact` (default) vs `snap`.
    pub size_mode: SizeMode,
    /// Payload shape (stage 1).
    pub shape: PayloadShape,
    /// When `Some`, overrides the text_format-derived rows/g (the
    /// fix-two-derive-one solver's output rides through here).
    pub id_block: Option<IdBlock>,
    /// Embed the receipt as an SVG `<metadata>` element. Default true.
    pub embed_metadata: bool,
}

impl Default for RenderOpts {
    fn default() -> Self {
        Self {
            fg: default_fg(),
            bg: default_bg(),
            size_mode: SizeMode::Exact,
            shape: PayloadShape::QrId,
            id_block: None,
            embed_metadata: true,
        }
    }
}

/// One px-true rendered label.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PxLabel {
    /// The SVG document; width/height/viewBox in device px.
    pub svg: String,
    /// Canvas width in device px.
    pub width_px: u32,
    /// Canvas height in device px.
    pub height_px: u32,
    /// Symbol footprint incl. quiet zone, in device px
    /// (= `modules * module_px`; under `overlap`/`additive` the quiet
    /// zone renders as background white inside the actual `white`).
    pub qr_px: u32,
    /// Pixels per QR module (integer by the §8 law).
    pub module_px: u32,
    /// Symbol modules per edge, quiet zone included (Micro QR M4:
    /// 17 + 2·2 = 21; Standard V1: 21 + 2·4 = 29).
    pub modules: u32,
    /// The module part (data modules only) in device px
    /// (= `data modules * module_px`).
    pub data_px: u32,
    /// Device px per nx75 anchor cell — the raster scale `k` and the
    /// g-law result (largest integer keeping the block inside the
    /// module part, capped at `module_px`).
    pub glyph_px: u32,
    /// The glyph cell height in anchor rows — always 7 for the nx75
    /// anchor font ([`crate::glyph_font::GRID_ROWS`]); kept for
    /// response-shape stability.
    pub glyph_cell: u32,
    /// Actual per-side white, canvas edge → module part: the floors
    /// plus the controlling axis's remainder (extra px bottom/right).
    pub white: Padding,
    /// The quiet-zone accounting the deduction ran under.
    pub padding_mode: PaddingMode,
    /// The RESOLVED symbology this label encodes, canonical compact
    /// form (e.g. `micro-m4-m`) — version/EC pins or auto-fit results
    /// are evidence, not guesses (ADR-031 §8).
    pub symbology: String,
    /// The artifact receipt — the SAME object the protocol response
    /// carries and the SVG `<metadata>` element inscribes (ADR-031
    /// §10). Built once in [`render_label_px_with_opts`], so the wire
    /// receipt and the inscribed metadata can never drift.
    pub receipt: Receipt,
}

/// Largest `m` (px/module) that fits the controlling axis, or 0 when
/// even 1 px/module cannot fit.
fn deduce_module_px(
    size_px: u32,
    pad_a: u32,
    pad_b: u32,
    data: u32,
    quiet: u32,
    mode: PaddingMode,
) -> u32 {
    let mut module_px = 0;
    let mut m = 1;
    while mode.min_size(data, quiet, pad_a, pad_b, m) <= u64::from(size_px) {
        module_px = m;
        m += 1;
    }
    module_px
}

/// QR→text gap over the white on the QR's text side:
/// `max(round(1.5 · white), quiet·m)`, half rounding up — the §8
/// safe-space clamp (typography can never invade the quiet zone, even
/// under `clip` where the white floor may be 0).
fn qr_text_gap(white_side: u32, quiet: u32, module_px: u32) -> u32 {
    (white_side + white_side.div_ceil(2)).max(quiet * module_px)
}

/// One render stamp of the nx75 v2 connection-kernel law: the ink a
/// single active connection owns, independent of every other stamp
/// (the render is a pure union).
enum Stamp {
    /// Straight connection: a k-wide rectangle between the two node
    /// centers inclusive — `horiz` picks the longitudinal axis (it
    /// is `a.r == b.r`, baked from integers, no float compare).
    Rect {
        x1: f64,
        x2: f64,
        y1: f64,
        y2: f64,
        horiz: bool,
    },
    /// Diagonal connection: an L1 diamond of radius k at the shared
    /// cell corner, clipped to the k-row anti-diagonal band —
    /// `same_sign` is true when the edge direction has dr == dc, so
    /// the band index is |dx-dy| there and |dx+dy| otherwise.
    Diam { cx: f64, cy: f64, same_sign: bool },
}

/// One sweep of the nx75 v1 KERNEL-PULL law: a quadrant kernel
/// dragged along the segment anchor center → edge midpoint (or held
/// at rest, `vx = vy = 0`, for an isolated anchor).
struct Sweep {
    ax: f64,
    ay: f64,
    vx: f64,
    vy: f64,
    kern: u8,
}

/// V1 kernel coverage at offset `(dx, dy)` from the swept center:
/// inside the L1 diamond of radius `half` always; inside the square
/// `|dx|, |dy| ≤ half` only where the quadrant's corner bit is set.
fn kern_covers(mask: u8, dx: f64, dy: f64, half: f64) -> bool {
    if dx.abs() + dy.abs() <= half {
        return true;
    }
    if dx.abs() > half || dy.abs() > half {
        return false;
    }
    let ci = if dy < 0.0 { 0 } else { 2 } + if dx < 0.0 { 0 } else { 1 };
    mask >> ci & 1 == 1
}

/// Rasterise one nx75 V1 glyph at integer scale `k` into a row-major
/// `5k × 7k` ink bitmap — the EVEN-k optical master, matching the
/// TRUE v1 reference renderer (`/tmp/true_v1.cjs`, mirrored into
/// `tools/bake_glyph_font.py`) and the bake-time checksums bit for
/// bit ([`crate::glyph_font::GLYPHS_V1`]).
///
/// The KERNEL-PULL law:
/// 1. Every active edge contributes TWO sweeps: each endpoint's
///    quadrant kernel swept from that anchor's center to the edge
///    MIDPOINT.
/// 2. Every anchor touched by no active edge contributes one
///    stationary sweep at rest.
/// 3. A pixel center `p` is ink iff some sweep covers it:
///    `t = clamp(proj(p onto the sweep segment), 0, 1)` and
///    [`kern_covers`]`(kern, p − lerp(t), k/2)` holds.
/// 4. A cell mask clips everything: only anchor cells plus the two
///    bridge cells of each active diagonal edge may carry ink.
fn raster_v1(g: &Glyph, k: u32) -> Vec<bool> {
    let kf = f64::from(k);
    let half = kf / 2.0;

    const CELLS: usize = (glyph_font::GRID_ROWS * glyph_font::GRID_COLS) as usize;
    let mut allowed = [false; CELLS];
    for a in g.anchors {
        allowed[(a.r as usize) * glyph_font::GRID_COLS as usize + a.c as usize] = true;
    }

    let mut sweeps = Vec::with_capacity(2 * g.edges.len() + g.anchors.len());
    let mut inked = vec![false; g.anchors.len()];
    for e in g.edges {
        let a = &g.anchors[e.a as usize];
        let b = &g.anchors[e.b as usize];
        if e.diag {
            allowed[(a.r as usize) * glyph_font::GRID_COLS as usize + b.c as usize] = true;
            allowed[(b.r as usize) * glyph_font::GRID_COLS as usize + a.c as usize] = true;
        }
        for (me, other) in [(a, b), (b, a)] {
            let ax = (f64::from(me.c) + 0.5) * kf;
            let ay = (f64::from(me.r) + 0.5) * kf;
            let mx = ((f64::from(me.c) + f64::from(other.c)) / 2.0 + 0.5) * kf;
            let my = ((f64::from(me.r) + f64::from(other.r)) / 2.0 + 0.5) * kf;
            sweeps.push(Sweep {
                ax,
                ay,
                vx: mx - ax,
                vy: my - ay,
                kern: me.quad_mask,
            });
        }
        inked[e.a as usize] = true;
        inked[e.b as usize] = true;
    }
    for (i, a) in g.anchors.iter().enumerate() {
        if !inked[i] {
            sweeps.push(Sweep {
                ax: (f64::from(a.c) + 0.5) * kf,
                ay: (f64::from(a.r) + 0.5) * kf,
                vx: 0.0,
                vy: 0.0,
                kern: a.quad_mask,
            });
        }
    }

    let w = (glyph_font::GRID_COLS * k) as usize;
    let h = (glyph_font::GRID_ROWS * k) as usize;
    let mut img = vec![false; w * h];
    for j in 0..h {
        let cr = j as u32 / k;
        let y = j as f64 + 0.5;
        for i in 0..w {
            let cc = i as u32 / k;
            if !allowed[(cr * glyph_font::GRID_COLS + cc) as usize] {
                continue;
            }
            let x = i as f64 + 0.5;
            for s in &sweeps {
                let l2 = s.vx * s.vx + s.vy * s.vy;
                let t = if l2 == 0.0 {
                    0.0
                } else {
                    (((x - s.ax) * s.vx + (y - s.ay) * s.vy) / l2).clamp(0.0, 1.0)
                };
                if kern_covers(s.kern, x - (s.ax + t * s.vx), y - (s.ay + t * s.vy), half) {
                    img[j * w + i] = true;
                    break;
                }
            }
        }
    }
    img
}

/// Rasterise one nx75 V2 glyph at integer scale `k` into a row-major
/// `5k × 7k` ink bitmap — the ODD-k optical master, matching the
/// reference implementation (the editor JS) and the bake-time
/// checksums bit for bit ([`crate::glyph_font::GLYPHS_V2`]).
///
/// The CONNECTION-KERNEL law, a union of three stamp types:
/// 1. STRAIGHT connection: pixels whose center lies between the two
///    node centers inclusive longitudinally and within k/2 of the
///    line transversely — a k-wide rectangle.
/// 2. DIAGONAL connection: at the shared cell corner
///    (`cx = max(c1,c2)·k`, `cy = max(r1,r2)·k`), pixels with
///    `L1(p − corner) ≤ k + eps` and anti-diagonal index `|dx−dy|`
///    (when the edge direction has dr == dc) or `|dx+dy|` otherwise
///    `≤ k−1 + eps` — chains render constant-width, single corners
///    become k-wide chamfers.
/// 3. NODE quadrants: each anchor-cell pixel not already painted is
///    painted iff its quadrant bit is set in the anchor's baked
///    kernel (quadrant = `(dy<0?0:2)+(dx<0?0:1)` relative to the
///    cell center).
///
/// That is the entire law — the stamps are bounded by construction,
/// so no clip mask exists.
fn raster_v2(g: &Glyph, k: u32) -> Vec<bool> {
    let kf = f64::from(k);
    let half = kf / 2.0;

    let mut cell: [Option<u8>; (glyph_font::GRID_ROWS * glyph_font::GRID_COLS) as usize] =
        [None; (glyph_font::GRID_ROWS * glyph_font::GRID_COLS) as usize];
    for a in g.anchors {
        cell[(a.r as usize) * glyph_font::GRID_COLS as usize + a.c as usize] = Some(a.quad_mask);
    }

    let mut stamps = Vec::with_capacity(g.edges.len());
    for e in g.edges {
        let a = &g.anchors[e.a as usize];
        let b = &g.anchors[e.b as usize];
        if !e.diag {
            stamps.push(Stamp::Rect {
                x1: (f64::from(a.c.min(b.c)) + 0.5) * kf,
                x2: (f64::from(a.c.max(b.c)) + 0.5) * kf,
                y1: (f64::from(a.r.min(b.r)) + 0.5) * kf,
                y2: (f64::from(a.r.max(b.r)) + 0.5) * kf,
                horiz: a.r == b.r,
            });
        } else {
            // Anti-diagonal index sign: direction (1,1) -> dx-dy,
            // else dx+dy
            let same_sign = i16::from(b.r) - i16::from(a.r) == i16::from(b.c) - i16::from(a.c);
            stamps.push(Stamp::Diam {
                cx: f64::from(a.c.max(b.c)) * kf,
                cy: f64::from(a.r.max(b.r)) * kf,
                same_sign,
            });
        }
    }

    let w = (glyph_font::GRID_COLS * k) as usize;
    let h = (glyph_font::GRID_ROWS * k) as usize;
    let mut img = vec![false; w * h];
    for j in 0..h {
        let cr = j as u32 / k;
        let y = j as f64 + 0.5;
        for i in 0..w {
            let x = i as f64 + 0.5;
            let mut on = false;
            for s in &stamps {
                match *s {
                    Stamp::Rect {
                        x1,
                        x2,
                        y1,
                        y2,
                        horiz,
                    } => {
                        // Px centers between the node centers
                        // inclusive, k-wide perpendicular
                        if horiz {
                            if x >= x1 - 1e-9 && x <= x2 + 1e-9 && (y - y1).abs() <= half {
                                on = true;
                                break;
                            }
                        } else if y >= y1 - 1e-9 && y <= y2 + 1e-9 && (x - x1).abs() <= half {
                            on = true;
                            break;
                        }
                    }
                    Stamp::Diam { cx, cy, same_sign } => {
                        // Corner diamond (radius k, reaches both
                        // node centers) clipped to the k-row
                        // anti-diagonal band
                        let dx = x - cx;
                        let dy = y - cy;
                        let anti = if same_sign {
                            (dx - dy).abs()
                        } else {
                            (dx + dy).abs()
                        };
                        if dx.abs() + dy.abs() <= kf + 1e-9 && anti <= kf - 1.0 + 1e-9 {
                            on = true;
                            break;
                        }
                    }
                }
            }
            if !on {
                let cc = i as u32 / k;
                if let Some(mask) = cell[(cr * glyph_font::GRID_COLS + cc) as usize] {
                    let dx = x - (f64::from(cc) + 0.5) * kf;
                    let dy = y - (f64::from(cr) + 0.5) * kf;
                    let ci = if dy < 0.0 { 0 } else { 2 } + if dx < 0.0 { 0 } else { 1 };
                    if mask >> ci & 1 == 1 {
                        on = true;
                    }
                }
            }
            if on {
                img[j * w + i] = true;
            }
        }
    }
    img
}

/// Rasterise the nx75 glyph for `c` at integer scale `k` — THE
/// parity dispatch of the optical-masters design (`None` outside the
/// nano14 alphabet):
///
/// - `k % 2 == 0` → the v1 master under the KERNEL-PULL law
///   ([`raster_v1`] on [`glyph_font::glyph_v1`])
/// - `k % 2 == 1` → the v2 master under the CONNECTION-KERNEL law
///   ([`raster_v2`] on [`glyph_font::glyph_v2`])
///
/// The user judged v1 best at even scales and v2 best at odd ones,
/// so each scale renders the master tuned for it.
fn raster_glyph(c: char, k: u32) -> Option<Vec<bool>> {
    if k.is_multiple_of(2) {
        glyph_font::glyph_v1(c).map(|g| raster_v1(g, k))
    } else {
        glyph_font::glyph_v2(c).map(|g| raster_v2(g, k))
    }
}

/// Append `c`'s nx75 ink to `out`, glyph box top-left at `(x0, y0)`,
/// rasterised at scale `k` ([`raster_glyph`]) and emitted as
/// horizontal-run `<rect>`s (height 1 device px) — the same emitter
/// shape the QR modules use, so glyphs and modules share one fill
/// group and one integer-px lattice.
///
/// Errors with [`CodecError::Render`] for a char outside the nano14
/// alphabet (defensive: nano14 payloads cannot contain one).
fn write_glyph_rects(
    out: &mut String,
    c: char,
    x0: u32,
    y0: u32,
    k: u32,
) -> Result<(), CodecError> {
    let img = raster_glyph(c, k).ok_or_else(|| {
        CodecError::Render(format!(
            "no nx75 glyph for {c:?}: the baked anchor font covers \
             the nano14 alphabet {alphabet}",
            alphabet = glyph_font::ALPHABET,
        ))
    })?;
    let w = (glyph_font::GRID_COLS * k) as usize;
    let h = (glyph_font::GRID_ROWS * k) as usize;
    for j in 0..h {
        let row = &img[j * w..(j + 1) * w];
        let mut i = 0;
        while i < w {
            if !row[i] {
                i += 1;
                continue;
            }
            let start = i;
            while i < w && row[i] {
                i += 1;
            }
            let x = x0 + start as u32;
            let y = y0 + j as u32;
            let run = (i - start) as u32;
            let _ = write!(
                out,
                "<rect x=\"{x}\" y=\"{y}\" width=\"{run}\" height=\"1\"/>"
            );
        }
    }
    Ok(())
}

/// The id-text block geometry (the g-law): all values in device px,
/// derived from the module part. See the module docs.
///
/// Placement is **justified** to the module-part extent (ADR-031 §8).
/// HORZ stretches the block vertically: first row's top at the
/// module-part top, last row's bottom at its bottom (or centered when
/// `rows == 1`). VERT stretches each row horizontally: first glyph's
/// left at the module-part left, last glyph's right at its right (or
/// centered when `chars == 1`); rows below the QR keep their
/// `8g`-pitch step. `block_h` is the **natural** block height
/// `(8·rows - 1)·g` — the extent of the rows below the QR in VERT,
/// used to size the canvas there. HORZ no longer reads it: the
/// justified row offsets carry the data_px height directly.
struct TextBlock {
    glyph_px: u32,
    block_h: u32,
    advance: u32,
    max_row_w: u32,
}

/// Distribute the integer-px slack `data_px - n * item_size` across
/// the `n - 1` inter-item gaps and return each item's LEAD offset
/// from the module-part start. Deterministic: the base gap is
/// `slack / (n - 1)`, and the first `slack % (n - 1)` gaps get one
/// extra pixel (so the last item's trailing edge lands exactly at
/// `data_px`). For `n == 1` the single item is centered in
/// `data_px`. Callers must guarantee `data_px >= n * item_size`
/// (the g-law's `text_block` enforces this by flooring `g`).
fn justified_offsets(data_px: u32, n: u32, item_size: u32) -> Vec<u32> {
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![data_px.saturating_sub(item_size) / 2];
    }
    let slack = data_px - n * item_size;
    let gaps = n - 1;
    let base = slack / gaps;
    let extra = slack % gaps;
    let mut offsets = Vec::with_capacity(n as usize);
    let mut cursor = 0_u32;
    for i in 0..n {
        offsets.push(cursor);
        // First `extra` gaps absorb one extra px each.
        let gap = base + if i < extra { 1 } else { 0 };
        cursor += item_size + gap;
    }
    offsets
}

/// Deduce the id-text block for `layout`: the glyph scale `g` is the
/// largest integer keeping the block — `n_rows` rows of 7g glyph
/// boxes with 1g gaps between rows — inside `data_px`, capped at
/// `module_px` so text dots are never coarser than QR dots. In
/// `vert` the rows must also fit the module-part width.
fn text_block(
    layout: Layout,
    data_px: u32,
    module_px: u32,
    n_rows: u32,
    max_chars: u32,
) -> Result<TextBlock, CodecError> {
    // Block height in g-units: 8 per row minus the missing last gap.
    let block_units = 8 * n_rows - 1;
    // Widest-row width in g-units: 6 per char (5 glyph + 1 spacing)
    // minus the missing trailing spacing.
    let row_units = (6 * max_chars).saturating_sub(1).max(1);
    let mut glyph_px = (data_px / block_units).min(module_px);
    if matches!(layout, Layout::Vert) {
        // Below the QR the rows must also fit the module-part width.
        glyph_px = glyph_px.min(data_px / row_units);
    }
    if glyph_px < 1 {
        let (need, what) = if data_px / block_units < 1 {
            (
                block_units,
                format!("a {n_rows}-row id-text block (7px glyph rows + 1px row gaps)"),
            )
        } else {
            (
                row_units,
                format!("a {max_chars}-char id-text row (5px glyphs + 1px spacing)"),
            )
        };
        return Err(CodecError::Render(format!(
            "module part {data_px}px cannot fit {what}: it needs at least \
             {need}px at 1px per glyph dot; use fewer rows via --chars or \
             a larger size"
        )));
    }
    // Natural block height in device px: `(8·rows − 1)·g`. HORZ
    // ignores this — justified row offsets carry the data_px height —
    // and VERT uses it to size the canvas below the QR.
    Ok(TextBlock {
        glyph_px,
        block_h: block_units * glyph_px,
        advance: 6 * glyph_px,
        max_row_w: (max_chars * 6 * glyph_px).saturating_sub(glyph_px),
    })
}

/// Render one px-true label.
///
/// `size_px` is the **exact output canvas** along the label's
/// controlling dimension (height for `horz`, width for `vert`);
/// `padding` carries the per-side **minimum** canvas-edge →
/// module-part margins (the §4 floor, CSS clockwise), with the quiet
/// zone counting toward them per `padding_mode` (§8). The symbology
/// resolves against the payload first (auto-fit version/EC where
/// unpinned), then the module size is deduced on the controlling axis
/// (see module docs); the remainder distributes on top of that axis's
/// floors while the non-controlling sides sit at their floors exactly,
/// so the controlling dimension always comes out exactly `size_px`.
///
/// Errors:
/// - [`CodecError::Encode`] when the payload does not fit the
///   requested symbology; the message carries the feasible
///   version/EC space for the payload (ADR-031 §8).
/// - [`CodecError::Render`] when the resolved symbol cannot fit one
///   device pixel per module inside the padding floors; the message
///   carries the minimum workable sizes for 1/2/3 px modules under the
///   active `padding_mode`. Also when the module part cannot fit the
///   id-text block at one device px per glyph dot (the message
///   suggests fewer rows via `--chars` or a larger size), and — purely
///   defensively, nano14 payloads cannot trigger it — for a char
///   outside the [`crate::glyph_font`] alphabet.
/// - [`CodecError::Unsupported`] for [`Layout::Flag`] — the px-true
///   flag geometry (wrap-zone width in device px) is not specified yet;
///   ADR-031 §5 lists it, this renderer covers `horz`/`vert` first.
pub fn render_label_px(
    canonical: &str,
    layout: Layout,
    size_px: u32,
    text_format: TextFormat,
    symbology: &Symbology,
    padding: Padding,
    padding_mode: PaddingMode,
) -> Result<PxLabel, CodecError> {
    // Test-facing wrapper around the rich API: default options keep
    // bit-for-bit parity with the pre-ADR-031-§10 behavior (no
    // metadata, default colors, exact mode, qr+id shape). The receipt
    // module is wired in via `RenderOpts` for callers that want it.
    let opts = RenderOpts {
        embed_metadata: false,
        ..Default::default()
    };
    render_label_px_with_opts(
        canonical,
        layout,
        size_px,
        text_format,
        symbology,
        padding,
        padding_mode,
        &opts,
    )
}

/// Render one px-true label with full ADR-031 §10 options (colors,
/// size-mode, payload-shape, optional id-block override, metadata
/// receipt).
///
/// Behavior is identical to [`render_label_px`] when `opts` matches
/// [`RenderOpts::default`] except `embed_metadata` (the rich entry
/// inscribes the receipt; the legacy entry does not).
#[allow(clippy::too_many_arguments)]
pub fn render_label_px_with_opts(
    canonical: &str,
    layout: Layout,
    size_px: u32,
    text_format: TextFormat,
    symbology: &Symbology,
    padding: Padding,
    padding_mode: PaddingMode,
    opts: &RenderOpts,
) -> Result<PxLabel, CodecError> {
    if matches!(layout, Layout::Flag { .. }) {
        return Err(CodecError::Unsupported(
            "flag layout has no px-true geometry yet (ADR-031 §5); \
             use horz or vert, or unit=mm for flag"
                .into(),
        ));
    }
    if matches!(opts.shape, PayloadShape::IdOnly) {
        return render_id_only(
            canonical,
            layout,
            size_px,
            text_format,
            padding,
            padding_mode,
            opts,
        );
    }
    let (resolved, matrix) = symbology.resolve(canonical)?;
    let data = matrix.size as u32;
    let quiet = resolved.quiet_modules();
    let modules = data + 2 * quiet;
    // The controlling axis: vertical (top/bottom) for horz, horizontal
    // (left/right) for vert.
    let (pad_a, pad_b) = match layout {
        Layout::Horz => (padding.top, padding.bottom),
        Layout::Vert => (padding.left, padding.right),
        Layout::Flag { .. } => unreachable!("rejected above"),
    };
    let module_px = deduce_module_px(size_px, pad_a, pad_b, data, quiet, padding_mode);
    if module_px < 1 {
        let pad_desc = if pad_a == pad_b {
            format!("{pad_a}px")
        } else {
            format!("{pad_a}px/{pad_b}px")
        };
        return Err(CodecError::Render(format!(
            "size {size_px}px with padding {pad_desc} ({mode} mode) cannot fit \
             1px per module for a {data}-module {sym} symbol with a {quiet}-module quiet zone; \
             minimum size is {min1}px (1px modules), {min2}px reaches 2px, {min3}px \
             reaches 3px",
            mode = padding_mode.name(),
            sym = resolved.compact(),
            min1 = padding_mode.min_size(data, quiet, pad_a, pad_b, 1),
            min2 = padding_mode.min_size(data, quiet, pad_a, pad_b, 2),
            min3 = padding_mode.min_size(data, quiet, pad_a, pad_b, 3),
        )));
    }
    let qr_px = modules * module_px;
    let data_px = data * module_px;
    // Controlling axis: floors + remainder (exact mode) or floors
    // only (snap mode — the §8 `--size-mode snap` rule: canvas snaps
    // DOWN to the content lattice, remainder omitted). The
    // non-controlling sides always sit at their floors exactly.
    let floor_a = padding_mode.floor_px(pad_a, quiet, module_px);
    let floor_b = padding_mode.floor_px(pad_b, quiet, module_px);
    let (white_a, white_b) = match opts.size_mode {
        SizeMode::Exact => {
            let rem = size_px - data_px - floor_a - floor_b;
            (floor_a + rem / 2, floor_b + rem / 2 + rem % 2)
        }
        SizeMode::Snap => (floor_a, floor_b),
    };
    let white = match layout {
        Layout::Horz => Padding {
            top: white_a,
            bottom: white_b,
            left: padding_mode.floor_px(padding.left, quiet, module_px),
            right: padding_mode.floor_px(padding.right, quiet, module_px),
        },
        Layout::Vert => Padding {
            left: white_a,
            right: white_b,
            top: padding_mode.floor_px(padding.top, quiet, module_px),
            bottom: padding_mode.floor_px(padding.bottom, quiet, module_px),
        },
        Layout::Flag { .. } => unreachable!("rejected above"),
    };
    // §8 safe-space clamp on the QR's text side: right of the symbol
    // in horz, below it in vert.
    let gap = match layout {
        Layout::Horz => qr_text_gap(white.right, quiet, module_px),
        Layout::Vert => qr_text_gap(white.bottom, quiet, module_px),
        Layout::Flag { .. } => unreachable!("rejected above"),
    };

    let show_text = !matches!(opts.shape, PayloadShape::QrOnly);
    // The g-law text block (module docs) — but the §10 solver may
    // override rows/g when --id-chars/--rows/--id-size are present.
    let (rows, n_rows, block_opt) = if show_text {
        let (text_rows, n_rows, _max_chars, block) = resolve_text_block(
            canonical,
            text_format,
            layout,
            data_px,
            module_px,
            opts.id_block,
        )?;
        (text_rows, n_rows, Some(block))
    } else {
        (Vec::new(), 0, None)
    };
    let glyph_px = block_opt.as_ref().map(|b| b.glyph_px).unwrap_or(0);

    // Module rects at their canvas offsets — every coordinate an
    // integer device px. The quiet zone is not drawn; the bg supplies
    // it (white ≥ quiet·m is structural under overlap/additive).
    let mut rects = String::with_capacity(matrix.size * matrix.size * 48);
    for r in 0..matrix.size {
        for c in 0..matrix.size {
            if matrix.get(r, c) {
                let x = white.left + c as u32 * module_px;
                let y = white.top + r as u32 * module_px;
                let _ = write!(
                    rects,
                    "<rect x=\"{x}\" y=\"{y}\" width=\"{module_px}\" height=\"{module_px}\"/>"
                );
            }
        }
    }

    // Glyph rects join the SAME crispEdges fill group as the modules:
    // the whole label is one deterministic binary raster.
    let (width_px, height_px) = match (layout, block_opt) {
        (Layout::Horz, Some(block)) => {
            let tx = white.left + data_px + gap;
            let row_offsets = justified_offsets(data_px, n_rows, 7 * glyph_px);
            for (ri, row) in rows.iter().enumerate() {
                let y0 = white.top + row_offsets[ri];
                for (ci, ch) in row.chars().enumerate() {
                    let x0 = tx + ci as u32 * block.advance;
                    write_glyph_rects(&mut rects, ch, x0, y0, glyph_px)?;
                }
            }
            // Exact mode keeps the canvas at size_px; snap mode lets
            // the controlling axis shrink to the content lattice.
            let h = match opts.size_mode {
                SizeMode::Exact => size_px,
                SizeMode::Snap => data_px + floor_a + floor_b,
            };
            (tx + block.max_row_w + white.right, h)
        }
        (Layout::Vert, Some(block)) => {
            let ty0 = white.top + data_px + gap;
            let row_pitch = 8 * glyph_px;
            for (ri, row) in rows.iter().enumerate() {
                let row_chars: Vec<char> = row.chars().collect();
                let chars = row_chars.len() as u32;
                let char_offsets = justified_offsets(data_px, chars, 5 * glyph_px);
                let y0 = ty0 + ri as u32 * row_pitch;
                for (ci, ch) in row_chars.iter().enumerate() {
                    let x0 = white.left + char_offsets[ci];
                    write_glyph_rects(&mut rects, *ch, x0, y0, glyph_px)?;
                }
            }
            let w = match opts.size_mode {
                SizeMode::Exact => size_px,
                SizeMode::Snap => data_px + floor_a + floor_b,
            };
            (w, ty0 + block.block_h + white.bottom)
        }
        // qr-only shape: the canvas is just the symbol + white floors.
        (Layout::Horz, None) => {
            let h = match opts.size_mode {
                SizeMode::Exact => size_px,
                SizeMode::Snap => data_px + floor_a + floor_b,
            };
            (white.left + data_px + white.right, h)
        }
        (Layout::Vert, None) => {
            let w = match opts.size_mode {
                SizeMode::Exact => size_px,
                SizeMode::Snap => data_px + floor_a + floor_b,
            };
            (w, white.top + data_px + white.bottom)
        }
        (Layout::Flag { .. }, _) => unreachable!("rejected above"),
    };

    let receipt = receipt_for(
        canonical,
        &resolved.compact(),
        size_px,
        padding,
        padding_mode,
        opts.size_mode,
        qr_px,
        module_px,
        glyph_px,
        &opts.fg,
        &opts.bg,
        payload_canonical_for(&opts.shape, &text_format, opts.id_block),
    );
    let svg = build_svg(
        width_px,
        height_px,
        &opts.bg,
        &opts.fg,
        &rects,
        opts,
        || receipt.clone(),
    );

    Ok(PxLabel {
        svg,
        width_px,
        height_px,
        qr_px,
        module_px,
        modules,
        data_px,
        glyph_px,
        glyph_cell: glyph_font::GRID_ROWS,
        white,
        padding_mode,
        symbology: resolved.compact(),
        receipt,
    })
}

/// Resolve the id-text block under either the g-law (no solver
/// override) or the §10 fix-two-derive-one solver result. Returns
/// `(rows, n_rows, max_chars, TextBlock)` for the placement loops.
fn resolve_text_block(
    canonical: &str,
    text_format: TextFormat,
    layout: Layout,
    data_px: u32,
    module_px: u32,
    override_block: Option<IdBlock>,
) -> Result<(Vec<String>, u32, u32, TextBlock), CodecError> {
    if let Some(b) = override_block {
        // The solver already chose rows + glyph_px against the
        // budget. We hand-build the row strings from the canonical id
        // prefix using the balanced grouping.
        let groups = b.grouping();
        let mut idx = 0;
        let chars: Vec<char> = canonical.chars().collect();
        let mut rows: Vec<String> = Vec::with_capacity(groups.len());
        for g in &groups {
            let end = (idx + *g as usize).min(chars.len());
            rows.push(chars[idx..end].iter().collect());
            idx = end;
        }
        let n_rows = b.rows;
        let max_chars = groups.iter().copied().max().unwrap_or(0);
        let block = TextBlock {
            glyph_px: b.glyph_px,
            block_h: (8 * n_rows - 1) * b.glyph_px,
            advance: 6 * b.glyph_px,
            max_row_w: (max_chars * 6 * b.glyph_px).saturating_sub(b.glyph_px),
        };
        return Ok((rows, n_rows, max_chars, block));
    }
    let rows = text_format.split(canonical);
    let n_rows = rows.len() as u32;
    let max_chars = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0) as u32;
    let block = text_block(layout, data_px, module_px, n_rows, max_chars)?;
    Ok((rows, n_rows, max_chars, block))
}

/// Build the canonical payload string for the response/metadata
/// receipt (stage 1: a flat list reflecting the resolved shape +
/// id-block).
fn payload_canonical_for(
    shape: &PayloadShape,
    _fmt: &TextFormat,
    id_block: Option<IdBlock>,
) -> String {
    let id_part = match id_block {
        Some(b) => format!("id:{}", b.grouping_str()),
        None => "id".into(),
    };
    match shape {
        PayloadShape::QrId => format!("qr {id_part}"),
        PayloadShape::IdQr => format!("{id_part} qr"),
        PayloadShape::QrOnly => "qr".into(),
        PayloadShape::IdOnly => id_part,
    }
}

/// Construct the SSOT [`Receipt`] for both the response payload and
/// the inscribed SVG metadata (ADR-031 §10: single constructor).
#[allow(clippy::too_many_arguments)]
fn receipt_for(
    canonical: &str,
    symbology: &str,
    size_px: u32,
    padding: Padding,
    padding_mode: PaddingMode,
    size_mode: SizeMode,
    qr_px: u32,
    module_px: u32,
    glyph_px: u32,
    fg: &Color,
    bg: &Color,
    payload_canonical: String,
) -> Receipt {
    Receipt {
        id: canonical.into(),
        payload: payload_canonical,
        symbology: symbology.into(),
        size_px,
        padding: [padding.top, padding.right, padding.bottom, padding.left],
        padding_mode: padding_mode.name().into(),
        size_mode: size_mode.name().into(),
        qr_px,
        module_px,
        glyph_px,
        fg: fg.svg.clone(),
        bg: bg.svg.clone(),
        font: "nx75".into(),
        generator: crate::receipt::generator(),
        // The repeat composer enriches this on composed strips.
        repeat: None,
    }
}

/// Assemble the SVG document: bg rect (omitted when `bg.is_none()`),
/// the content fill group with `fg`, and optionally the metadata
/// receipt before the content (so it sits inside the root in a
/// well-known location).
fn build_svg<F: FnOnce() -> Receipt>(
    width_px: u32,
    height_px: u32,
    bg: &Color,
    fg: &Color,
    rects: &str,
    opts: &RenderOpts,
    build_receipt: F,
) -> String {
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width_px}\" height=\"{height_px}\" \
viewBox=\"0 0 {width_px} {height_px}\">",
    );
    if opts.embed_metadata {
        let r = build_receipt();
        out.push_str(&metadata_element(&r));
    }
    if !bg.is_none() {
        let _ = write!(
            out,
            "<rect width=\"{width_px}\" height=\"{height_px}\" fill=\"{}\"/>",
            bg.svg
        );
    }
    let _ = writeln!(
        out,
        "<g fill=\"{}\" shape-rendering=\"crispEdges\">{rects}</g></svg>",
        fg.svg
    );
    out
}

/// `id`-only payload (ADR-031 §10): no QR — the id-text block fills
/// the whole canvas budget along the layout axis. Solver mode is
/// strongly recommended; without a solver override we fall through to
/// the g-law over the full canvas budget.
fn render_id_only(
    canonical: &str,
    layout: Layout,
    size_px: u32,
    text_format: TextFormat,
    padding: Padding,
    padding_mode: PaddingMode,
    opts: &RenderOpts,
) -> Result<PxLabel, CodecError> {
    // For id-only, the budget IS the whole canvas (minus the per-side
    // pad floors — clip mode reaches the widest).
    let (pad_a, pad_b) = match layout {
        Layout::Horz => (padding.top, padding.bottom),
        Layout::Vert => (padding.left, padding.right),
        Layout::Flag { .. } => unreachable!("flag rejected upstream"),
    };
    let budget = size_px.saturating_sub(pad_a + pad_b);
    if budget < 7 {
        return Err(CodecError::Render(format!(
            "id-only payload: budget {budget}px (size {size_px}, pad {pad_a}/{pad_b}) cannot fit \
             one 7px glyph row"
        )));
    }
    // Build a synthetic IdBlock either from the solver or the
    // text_format default.
    let block = match opts.id_block {
        Some(b) => b,
        None => {
            // g-law over budget: rows from text_format, g = budget /
            // ((8·rows - 1)) and bounded by chars-width.
            let rows = text_format.split(canonical);
            let n_rows = rows.len() as u32;
            let max_chars = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0) as u32;
            let units = 8 * n_rows - 1;
            let row_units = (6 * max_chars).saturating_sub(1).max(1);
            let g = (budget / units).min(budget / row_units).max(1);
            let total_chars: u32 = rows.iter().map(|r| r.chars().count() as u32).sum();
            IdBlock {
                chars: total_chars,
                rows: n_rows,
                glyph_px: g,
            }
        }
    };
    let rows_split = {
        let groups = block.grouping();
        let chars: Vec<char> = canonical.chars().collect();
        let mut idx = 0;
        let mut out: Vec<String> = Vec::with_capacity(groups.len());
        for g in &groups {
            let end = (idx + *g as usize).min(chars.len());
            out.push(chars[idx..end].iter().collect());
            idx = end;
        }
        out
    };
    let glyph_px = block.glyph_px;
    let n_rows = block.rows;
    let max_chars = block.grouping().into_iter().max().unwrap_or(0);
    let block_h = (8 * n_rows - 1) * glyph_px;
    let row_w = max_chars * 6 * glyph_px - glyph_px;

    let mut rects = String::new();
    let (width_px, height_px) = match layout {
        Layout::Horz => {
            // The id-block runs the full canvas width as 1 row by
            // default; for multi-row, the rows justify vertically.
            let y_top = pad_a;
            let row_offsets = justified_offsets(budget, n_rows, 7 * glyph_px);
            for (ri, row) in rows_split.iter().enumerate() {
                let y0 = y_top + row_offsets[ri];
                for (ci, ch) in row.chars().enumerate() {
                    let x0 = padding.left + ci as u32 * 6 * glyph_px;
                    write_glyph_rects(&mut rects, ch, x0, y0, glyph_px)?;
                }
            }
            (padding.left + row_w + padding.right, size_px)
        }
        Layout::Vert => {
            let row_pitch = 8 * glyph_px;
            for (ri, row) in rows_split.iter().enumerate() {
                let row_chars: Vec<char> = row.chars().collect();
                let chars_n = row_chars.len() as u32;
                let char_offsets = justified_offsets(budget, chars_n, 5 * glyph_px);
                let y0 = padding.top + ri as u32 * row_pitch;
                for (ci, ch) in row_chars.iter().enumerate() {
                    let x0 = padding.left + char_offsets[ci];
                    write_glyph_rects(&mut rects, *ch, x0, y0, glyph_px)?;
                }
            }
            (size_px, padding.top + block_h + padding.bottom)
        }
        Layout::Flag { .. } => unreachable!("flag rejected upstream"),
    };

    let receipt = receipt_for(
        canonical,
        "none",
        size_px,
        padding,
        padding_mode,
        opts.size_mode,
        0,
        0,
        glyph_px,
        &opts.fg,
        &opts.bg,
        payload_canonical_for(&opts.shape, &text_format, Some(block)),
    );
    let svg = build_svg(
        width_px,
        height_px,
        &opts.bg,
        &opts.fg,
        &rects,
        opts,
        || receipt.clone(),
    );
    Ok(PxLabel {
        svg,
        width_px,
        height_px,
        qr_px: 0,
        module_px: 0,
        modules: 0,
        data_px: 0,
        glyph_px,
        glyph_cell: glyph_font::GRID_ROWS,
        white: padding,
        padding_mode,
        symbology: "none".into(),
        receipt,
    })
}

/// Pad every label in a print job to the job's largest footprint
/// (ADR-031 §4): the uniform canvas is at least the batch's largest
/// label *and* at least `largest data_px + opposing padding floors`
/// per dimension — padding keeps its §8 white semantics (canvas edge →
/// module part), so each side stays the smallest allowed
/// canvas→module distance around the biggest module part. Smaller
/// labels are centered (integer offsets — the px grid is preserved);
/// the extra margin sits outside each label's own white.
pub fn fill_to_max(labels: &mut [PxLabel], padding: Padding) {
    let Some(max_data) = labels.iter().map(|l| l.data_px).max() else {
        return;
    };
    let floor_w = max_data + padding.left + padding.right;
    let floor_h = max_data + padding.top + padding.bottom;
    let target_w = labels
        .iter()
        .map(|l| l.width_px)
        .max()
        .unwrap_or(0)
        .max(floor_w);
    let target_h = labels
        .iter()
        .map(|l| l.height_px)
        .max()
        .unwrap_or(0)
        .max(floor_h);
    for l in labels.iter_mut() {
        if l.width_px == target_w && l.height_px == target_h {
            continue;
        }
        let dx = (target_w - l.width_px) / 2;
        let dy = (target_h - l.height_px) / 2;
        let inner = inner_body(&l.svg);
        // Preserve the label's bg color in the uniform outer canvas
        // (the receipt's bg rides the truth; `none` ⇒ no rect).
        let bg = &l.receipt.bg;
        let bg_rect = if bg == "none" {
            String::new()
        } else {
            format!("<rect width=\"{target_w}\" height=\"{target_h}\" fill=\"{bg}\"/>")
        };
        l.svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{target_w}\" height=\"{target_h}\" \
viewBox=\"0 0 {target_w} {target_h}\">\
{bg_rect}\
<g transform=\"translate({dx},{dy})\">{inner}</g>\
</svg>\n"
        );
        l.width_px = target_w;
        l.height_px = target_h;
    }
}

/// Body of an `<svg>…</svg>` document this module produced (everything
/// between the root open tag and the closing tag). The input is always
/// our own single-root output, so plain string slicing is sound here —
/// no XML parser needed.
fn inner_body(svg: &str) -> &str {
    let start = svg.find('>').map(|i| i + 1).unwrap_or(0);
    let end = svg.rfind("</svg>").unwrap_or(svg.len());
    svg[start..end].trim_matches('\n')
}

// ---------- tests ----------

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixed fixture mirroring `svg.rs` / `test_labels.py:35`.
    const FIXED_ID: &str = "K7M3PQ9RT5VAXY";

    /// Micro QR M4: 17 data modules, 2-module quiet zone.
    const MICRO_DATA: u32 = 17;
    const MICRO_QUIET: u32 = 2;
    const MICRO_TOTAL: u32 = 21;
    /// Standard QR V1: 21 data modules, 4-module quiet zone.
    const STANDARD_DATA: u32 = 21;
    const STANDARD_QUIET: u32 = 4;
    const STANDARD_TOTAL: u32 = 29;

    fn sym(spec: &str) -> Symbology {
        spec.parse().expect("symbology parses")
    }

    fn render_spec(
        spec: &str,
        layout: Layout,
        size: u32,
        padding: Padding,
        mode: PaddingMode,
    ) -> PxLabel {
        render_label_px(
            FIXED_ID,
            layout,
            size,
            TextFormat::FourFour,
            &sym(spec),
            padding,
            mode,
        )
        .expect("px render succeeds")
    }

    fn render_mode(size: u32, pad_min: u32, micro: bool, mode: PaddingMode) -> PxLabel {
        let spec = if micro { "micro" } else { "qr" };
        render_spec(spec, Layout::Horz, size, Padding::uniform(pad_min), mode)
    }

    fn render_pad(size: u32, pad_min: u32, micro: bool) -> PxLabel {
        render_mode(size, pad_min, micro, PaddingMode::Overlap)
    }

    fn render(size: u32, micro: bool) -> PxLabel {
        render_pad(size, 2, micro)
    }

    // ---------- the §8 law: size = exact canvas, padding references
    // the module part, module DEDUCED per padding_mode ----------

    #[test]
    fn overlap_micro_boundary_table() {
        // max m with 17·m + 2·max(pad, 2·m) <= size — the ADR-031 §8
        // worked examples (2026-06-11).
        let cases = [
            // (size, pad_min, m, data_px)
            (64, 2, 3, 51), // 51 + 2·max(2,6) = 63 <= 64
            (64, 0, 3, 51), // quiet zone alone bounds: 51 + 12 = 63
            (64, 7, 2, 34), // m=3 needs 51 + 14 = 65 > 64; m=2: 48
            (21, 0, 1, 17), // 17 + 2·2 = 21 — minimum workable
            (67, 2, 3, 51), // 63 <= 67, m=4 needs 84
        ];
        for (size, pad_min, m, data_px) in cases {
            let l = render_pad(size, pad_min, true);
            assert_eq!(l.modules, MICRO_TOTAL, "size {size}/pad {pad_min}");
            assert_eq!(l.module_px, m, "size {size}/pad {pad_min}");
            assert_eq!(l.data_px, data_px, "size {size}/pad {pad_min}");
            assert_eq!(l.data_px, MICRO_DATA * m, "module part");
            assert_eq!(l.qr_px, MICRO_TOTAL * m, "incl-quiet footprint");
            assert_eq!(l.padding_mode, PaddingMode::Overlap);
            assert_eq!(l.symbology, "micro-m4-m", "auto-fit resolves M4-M");
            // The controlling dimension is EXACTLY the requested size.
            assert_eq!(l.height_px, size, "horz height == size exactly");
            // The controlling axis absorbed the remainder on top of the
            // floors; the structural quiet-zone minimum held.
            let floor = pad_min.max(MICRO_QUIET * m);
            assert_eq!(l.white.top, (size - data_px) / 2, "size {size}");
            assert_eq!(l.white.top + l.white.bottom, size - data_px, "size {size}");
            assert!(l.white.bottom >= l.white.top, "extra px goes bottom");
            // Non-controlling sides sit at their floors exactly.
            assert_eq!((l.white.left, l.white.right), (floor, floor));
            assert!(
                l.white.top >= floor,
                "white {} >= max(pad {pad_min}, quiet·m {})",
                l.white.top,
                MICRO_QUIET * m,
            );
        }
    }

    #[test]
    fn additive_micro_boundary_table() {
        // max m with (17 + 4)·m + 2·pad <= size.
        let cases = [
            // (size, pad_min, m, data_px)
            (64, 2, 2, 34), // 21·2 + 4 = 46 <= 64; m=3 needs 67
            (63, 0, 3, 51), // 21·3 = 63 exactly
            (25, 2, 1, 17), // 21 + 4 = 25 — minimum workable
        ];
        for (size, pad_min, m, data_px) in cases {
            let l = render_mode(size, pad_min, true, PaddingMode::Additive);
            assert_eq!(l.module_px, m, "size {size}/pad {pad_min}");
            assert_eq!(l.data_px, data_px, "size {size}/pad {pad_min}");
            assert_eq!(l.padding_mode, PaddingMode::Additive);
            assert_eq!(l.height_px, size, "exact canvas");
            assert_eq!(l.white.top, (size - data_px) / 2);
            // Additive: the quiet zone sits inside white but does NOT
            // satisfy the padding floor — on every side.
            let floor = MICRO_QUIET * m + pad_min;
            assert!(
                l.white.top >= floor,
                "white {} >= quiet·m + pad",
                l.white.top
            );
            assert_eq!((l.white.left, l.white.right), (floor, floor));
        }
    }

    #[test]
    fn clip_mode_maximizes_modules() {
        // No embedded quiet zone: max m with 17·m + 2·pad <= size —
        // the printer's intrinsic margins supply the safe space.
        let cases = [
            // (size, pad_min, m, data_px)
            (64, 0, 3, 51), // floor(64/17) = 3 (white 6 = remainder only)
            (68, 0, 4, 68), // exact fit, ZERO white — vs overlap m=3
            (35, 0, 2, 34), // overlap at 35 would only reach m=1
            (64, 2, 3, 51), // 17·3 + 4 = 55 <= 64
        ];
        for (size, pad_min, m, data_px) in cases {
            let l = render_mode(size, pad_min, true, PaddingMode::Clip);
            assert_eq!(l.module_px, m, "size {size}/pad {pad_min}");
            assert_eq!(l.data_px, data_px, "size {size}/pad {pad_min}");
            assert_eq!(l.padding_mode, PaddingMode::Clip);
            assert_eq!(l.height_px, size, "exact canvas");
            // Non-controlling floors are the bare pads under clip.
            assert_eq!((l.white.left, l.white.right), (pad_min, pad_min));
            // Clip beats or matches overlap at every size.
            let o = render_mode(size, pad_min, true, PaddingMode::Overlap);
            assert!(l.module_px >= o.module_px, "clip >= overlap at {size}");
        }
    }

    #[test]
    fn overlap_standard_boundary_table() {
        // Standard V1: max m with 21·m + 2·max(pad, 4·m) <= size.
        let cases = [
            // (size, pad_min, m, data_px)
            (64, 2, 2, 42),    // 42 + 2·max(2,8) = 58 <= 64
            (29, 0, 1, 21),    // 21 + 8 = 29 — minimum workable
            (100, 2, 3, 63),   // 63 + 24 = 87; m=4 needs 116
            (300, 4, 10, 210), // 210 + 80 = 290; m=11 needs 319
        ];
        for (size, pad_min, m, data_px) in cases {
            let l = render_pad(size, pad_min, false);
            assert_eq!(l.modules, STANDARD_TOTAL);
            assert_eq!(l.module_px, m, "size {size}/pad {pad_min}");
            assert_eq!(l.data_px, data_px);
            assert_eq!(l.data_px, STANDARD_DATA * m);
            assert_eq!(l.symbology, "qr-v1-m", "auto-fit resolves V1-M");
            assert_eq!(l.height_px, size, "exact canvas");
            assert!(l.white.top >= pad_min.max(STANDARD_QUIET * m));
        }
    }

    // ---------- symbology pins flow into the one deduction ----------

    #[test]
    fn m3_l_pin_yields_bigger_dots_clip_at_64_is_4px() {
        // The ADR-031 §8 A/B: M3 contributes 15 data modules to the
        // SAME deduction engine, so clip@64 yields floor(64/15) = 4px
        // modules where M4 reaches only 3px.
        let l = render_spec(
            "micro-m3-l",
            Layout::Horz,
            64,
            Padding::uniform(0),
            PaddingMode::Clip,
        );
        assert_eq!(l.symbology, "micro-m3-l");
        assert_eq!(l.module_px, 4, "clip@64 on 15 modules → 4px");
        assert_eq!(l.data_px, 60);
        assert_eq!(l.modules, 15 + 2 * MICRO_QUIET);
        assert_eq!(l.height_px, 64, "exact canvas");
        // Remainder 4 splits 2/2 on the controlling axis.
        assert_eq!((l.white.top, l.white.bottom), (2, 2));
        // The g-law follows the module part: a 2-row block needs 15g
        // ≤ data 60 → g=4 (the module_px cap also allows 4), so the
        // bigger dots carry into the typography too.
        assert_eq!((l.glyph_cell, l.glyph_px), (7, 4));
        // The block centers in the module part: 15·4 = 60 fills it
        // exactly, so the top ink row sits at white.top + bearing of
        // the rows' topmost ink.
        let (_, glyph_dots) = split_rects(&l, Layout::Horz);
        assert_eq!(
            glyph_area(&glyph_dots),
            expected_ink(TextFormat::FourFour, 4),
            "ink-area ledger at k=4"
        );
        let m4 = render_mode(64, 0, true, PaddingMode::Clip);
        assert_eq!(m4.module_px, 3, "M4 (17 modules) reaches only 3px");
        assert_eq!(
            (m4.glyph_cell, m4.glyph_px),
            (7, 3),
            "data 51 → 15g ≤ 51 caps g at 3 alongside m"
        );
    }

    #[test]
    fn m3_l_overlap_at_64() {
        // Overlap: 15·m + 2·max(2, 2m) ≤ 64 → m=3 (45 + 12 = 57).
        let l = render_spec(
            "micro-m3-l",
            Layout::Horz,
            64,
            Padding::uniform(2),
            PaddingMode::Overlap,
        );
        assert_eq!((l.module_px, l.data_px), (3, 45));
        assert_eq!(l.symbology, "micro-m3-l");
    }

    #[test]
    fn infeasible_symbology_pin_surfaces_the_feasibility_hint() {
        // M4-Q caps at 13 alnum chars — the §8 feasibility error rides
        // through the px renderer untouched.
        let err = render_label_px(
            FIXED_ID,
            Layout::Horz,
            64,
            TextFormat::FourFour,
            &sym("micro-m4-q"),
            Padding::uniform(2),
            PaddingMode::Overlap,
        )
        .expect_err("M4-Q cannot hold 14 alnum chars");
        let msg = err.to_string();
        assert!(matches!(err, CodecError::Encode(_)), "got {err:?}");
        assert!(msg.contains("M4-Q caps at 13 alnum chars"), "got: {msg}");
        assert!(msg.contains("micro-m3-l"), "feasible space, got: {msg}");
    }

    // ---------- per-side padding: floors per axis, CSS clockwise ----------

    #[test]
    fn per_side_padding_floors_each_side_independently() {
        // horz, overlap, top 2 / right 10 / bottom 6 / left 4:
        // controlling (vertical) floors max(2,2m) + max(6,2m):
        // m=3 → 51 + 6 + 6 = 63 ≤ 64; m=4 → 68 + 8 + 8 = 84. rem 1.
        let l = render_spec(
            "micro",
            Layout::Horz,
            64,
            Padding::sides(2, 10, 6, 4),
            PaddingMode::Overlap,
        );
        assert_eq!((l.module_px, l.data_px), (3, 51));
        assert_eq!((l.white.top, l.white.bottom), (6, 7), "floors + rem→bottom");
        // Non-controlling: left max(4,6) = 6, right max(10,6) = 10.
        assert_eq!((l.white.left, l.white.right), (6, 10));
        assert_eq!(l.height_px, 64, "exact canvas");
        // Gap follows the RIGHT side's white: max(round(1.5·10), 6) =
        // 15 → tx = 6 + 51 + 15 = 72; the first ink rect sits the
        // rows' left bearing inside that.
        assert_eq!((l.glyph_cell, l.glyph_px), (7, 3));
        let (_, glyph_dots) = split_rects(&l, Layout::Horz);
        assert_eq!(
            glyph_dots.iter().map(|r| r.0).min(),
            Some(72 + left_bearing(TextFormat::FourFour, 3)),
            "tx + left bearing"
        );
    }

    #[test]
    fn per_side_additive_floors_add_quiet_to_each_pad() {
        // additive, top 1 / right 2 / bottom 3 / left 4, size 64:
        // controlling: 17m + (2m+1) + (2m+3) ≤ 64 → 21m + 4 ≤ 64 → m=2.
        let l = render_spec(
            "micro",
            Layout::Horz,
            64,
            Padding::sides(1, 2, 3, 4),
            PaddingMode::Additive,
        );
        assert_eq!(l.module_px, 2);
        // floors: top 4+1=5, bottom 4+3=7; rem = 64−34−12 = 18 → 9/9.
        assert_eq!((l.white.top, l.white.bottom), (14, 16));
        assert_eq!((l.white.left, l.white.right), (8, 6));
    }

    #[test]
    fn vert_controlling_axis_is_horizontal() {
        // vert, overlap, left 0 / right 9, size 64: 17m + max(0,2m) +
        // max(9,2m) ≤ 64 → m=3: 51 + 6 + 9 = 66 > 64 → m=2: 34+4+9=47.
        let l = render_spec(
            "micro",
            Layout::Vert,
            64,
            Padding::sides(0, 9, 0, 0),
            PaddingMode::Overlap,
        );
        assert_eq!(l.module_px, 2);
        assert_eq!(l.width_px, 64, "vert width == size exactly");
        // rem = 64 − 34 − 4 − 9 = 17 → left 4+8=12, right 9+8+1=18.
        assert_eq!((l.white.left, l.white.right), (12, 18));
        // Vertical sides at their floors (quiet·m = 4).
        assert_eq!((l.white.top, l.white.bottom), (4, 4));
    }

    // ---------- uniform white, gap law, module placement ----------

    type Rect = (u32, u32, u32, u32);

    /// All non-background `<rect>` x/y/width/height values (the
    /// background rect carries `fill`, content rects do not).
    fn content_rects(svg: &str) -> Vec<Rect> {
        let doc = roxmltree::Document::parse(svg).expect("well-formed SVG");
        doc.descendants()
            .filter(|n| n.tag_name().name() == "rect" && n.attribute("fill").is_none())
            .map(|r| {
                let attr = |k: &str| -> u32 {
                    let v = r.attribute(k).expect("rect attr present");
                    v.parse()
                        .unwrap_or_else(|_| panic!("{k}={v:?} is not an integer px coordinate"))
                };
                (attr("x"), attr("y"), attr("width"), attr("height"))
            })
            .collect()
    }

    /// Partition content rects into (QR modules, glyph dots): the
    /// bitmap id-text lives strictly right of the module part in
    /// `horz` and strictly below it in `vert`.
    fn split_rects(l: &PxLabel, layout: Layout) -> (Vec<Rect>, Vec<Rect>) {
        content_rects(&l.svg)
            .into_iter()
            .partition(|r| match layout {
                Layout::Horz => r.0 < l.white.left + l.data_px,
                Layout::Vert => r.1 < l.white.top + l.data_px,
                Layout::Flag { .. } => unreachable!("px mode rejects flag"),
            })
    }

    /// Dark-module count of FIXED_ID's symbol under `spec` — the QR
    /// side of the rect-count ledger.
    fn dark_modules(spec: &str) -> usize {
        let (_, matrix) = sym(spec).resolve(FIXED_ID).expect("resolves");
        (0..matrix.size)
            .map(|r| (0..matrix.size).filter(|&c| matrix.get(r, c)).count())
            .sum()
    }

    /// Ink-pixel count of `c`'s nx75 raster at scale `k` — recomputed
    /// through the parity dispatch ([`raster_glyph`]), not read from
    /// the baked checksums.
    fn glyph_ink(c: char, k: u32) -> usize {
        let img = raster_glyph(c, k).expect("nano14 alphabet");
        img.iter().filter(|&&p| p).count()
    }

    /// Total ink pixels of FIXED_ID's text rows under `fmt` at scale
    /// `k` — the glyph side of the ink-area ledger.
    fn expected_ink(fmt: TextFormat, k: u32) -> usize {
        fmt.split(FIXED_ID)
            .iter()
            .flat_map(|r| r.chars())
            .map(|c| glyph_ink(c, k))
            .sum()
    }

    /// Total glyph ink area in px² from the emitted run rects.
    fn glyph_area(rects: &[Rect]) -> usize {
        rects.iter().map(|r| (r.2 * r.3) as usize).sum()
    }

    /// Leftmost ink column, in device px, across the first chars of
    /// FIXED_ID's rows under `fmt` at scale `k`: the first ink rect
    /// sits at `tx + bearing` (nx75 glyphs may rest their leftmost
    /// anchors off column 0).
    fn left_bearing(fmt: TextFormat, k: u32) -> u32 {
        let w = (glyph_font::GRID_COLS * k) as usize;
        fmt.split(FIXED_ID)
            .iter()
            .filter_map(|r| r.chars().next())
            .map(|c| {
                let img = raster_glyph(c, k).expect("nano14 alphabet");
                (0..w)
                    .find(|&i| img.iter().skip(i).step_by(w).any(|&p| p))
                    .expect("glyph has ink") as u32
            })
            .min()
            .expect("rows present")
    }

    #[test]
    fn white_floors_hold_and_quiet_zone_is_not_drawn() {
        // 64/2 overlap micro: m=3, data 51, white top/left 6
        // (remainder px on the bottom edge of the controlling
        // dimension).
        let l = render(64, true);
        assert_eq!((l.module_px, l.data_px), (3, 51));
        assert_eq!(l.white, Padding::sides(6, 6, 7, 6));
        let (rects, glyph_dots) = split_rects(&l, Layout::Horz);
        assert!(!rects.is_empty() && !glyph_dots.is_empty());
        let min_x = rects.iter().map(|r| r.0).min().expect("rects");
        let min_y = rects.iter().map(|r| r.1).min().expect("rects");
        let max_y = rects.iter().map(|r| r.1 + r.3).max().expect("rects");
        // Module part offset by the left/top white; every module inside
        // the module part — nothing rendered in the quiet zone.
        assert_eq!(min_x, l.white.left, "left offset == white.left");
        assert_eq!(min_y, l.white.top, "top offset == white.top");
        assert_eq!(max_y, l.white.top + l.data_px, "module part bottom");
        for (x, y, w, h) in &rects {
            assert_eq!((*w, *h), (l.module_px, l.module_px));
            assert!(x + w <= l.white.left + l.data_px, "inside module part");
            assert!(y + h <= l.white.top + l.data_px, "inside module part");
            // On the module grid.
            assert_eq!((x - l.white.left) % l.module_px, 0);
            assert_eq!((y - l.white.top) % l.module_px, 0);
        }
        // Bottom edge absorbs the odd remainder: 64 − (6 + 51) = 7.
        assert_eq!(l.height_px - (l.white.top + l.data_px), 7);
    }

    #[test]
    fn horz_gap_is_clamped_one_and_a_half_right_white() {
        for (size, pad) in [(64_u32, 2_u32), (67, 2), (100, 0), (212, 5)] {
            let l = render_pad(size, pad, true);
            // gap = max(round(1.5·white.right), quiet·m), half up.
            let gap = (l.white.right + l.white.right.div_ceil(2)).max(MICRO_QUIET * l.module_px);
            let expected_tx = l.white.left + l.data_px + gap;
            // nx75 glyphs may rest their leftmost ink off column 0,
            // so the first ink rect sits the bearing after tx.
            let bearing = left_bearing(TextFormat::FourFour, l.glyph_px);
            let (_, glyph_dots) = split_rects(&l, Layout::Horz);
            let min_x = glyph_dots.iter().map(|r| r.0).min().expect("glyph dots");
            assert_eq!(
                min_x,
                expected_tx + bearing,
                "size {size}/pad {pad}: text starts at white.left + data + gap (+ bearing)"
            );
        }
    }

    #[test]
    fn clip_gap_clamps_to_the_quiet_zone() {
        // Clip with pad 0: white.right = 0, so 1.5·white would be 0 —
        // the §8 safe-space clamp keeps the text quiet·m away.
        let l = render_mode(64, 0, true, PaddingMode::Clip);
        assert_eq!((l.module_px, l.white.right), (3, 0));
        let gap = MICRO_QUIET * l.module_px;
        let tx = l.white.left + l.data_px + gap;
        let bearing = left_bearing(TextFormat::FourFour, l.glyph_px);
        let (_, glyph_dots) = split_rects(&l, Layout::Horz);
        assert_eq!(
            glyph_dots.iter().map(|r| r.0).min(),
            Some(tx + bearing),
            "text clamped {gap}px off the symbol (+ glyph left bearing)"
        );
    }

    #[test]
    fn vert_layout_width_is_exactly_the_requested_size() {
        let l = render_spec(
            "micro",
            Layout::Vert,
            64,
            Padding::uniform(0),
            PaddingMode::Overlap,
        );
        assert_eq!(l.width_px, 64, "vert width == size exactly");
        assert_eq!((l.module_px, l.data_px), (3, 51));
        assert_eq!((l.white.left, l.white.top), (6, 6));
        let (rects, _) = split_rects(&l, Layout::Vert);
        let min_x = rects.iter().map(|r| r.0).min().expect("rects");
        let min_y = rects.iter().map(|r| r.1).min().expect("rects");
        assert_eq!((min_x, min_y), (6, 6), "left white == top white");
    }

    // ---------- bitmap typography on the module lattice (§8) ----------

    #[test]
    fn px_svg_carries_no_text_elements_or_fonts() {
        // The whole label is one binary raster: glyphs share the QR's
        // crispEdges fill group and nothing references a font.
        for layout in [Layout::Horz, Layout::Vert] {
            for fmt in [
                TextFormat::FourFour,
                TextFormat::FourFourFour,
                TextFormat::FiveFiveFour,
            ] {
                let l = render_label_px(
                    FIXED_ID,
                    layout,
                    100,
                    fmt,
                    &sym("micro"),
                    Padding::uniform(2),
                    PaddingMode::Overlap,
                )
                .expect("renders");
                assert!(!l.svg.contains("<text"), "no <text> ({layout:?}, {fmt:?})");
                assert!(!l.svg.contains("font-family"), "no font ({layout:?})");
                assert!(!l.svg.contains("font-size"), "no font size ({layout:?})");
                assert_eq!(l.svg.matches("<g ").count(), 1, "one fill group");
            }
        }
    }

    #[test]
    fn nx75_v1_checksums_lock_the_even_k_renderer_to_the_bake() {
        // 31 glyphs × k ∈ {2, 4, 6}: the Rust kernel-pull raster
        // must reproduce the bake-time ink counts exactly — any
        // drift in the sweep law (or the baked v1 data) trips here.
        for g in &glyph_font::GLYPHS_V1 {
            assert_eq!(g.ink_bits.len(), glyph_font::CHECKSUM_KS_V1.len());
            for (i, &k) in glyph_font::CHECKSUM_KS_V1.iter().enumerate() {
                assert_eq!(k % 2, 0, "v1 checksums sit on the EVEN parity");
                let got = raster_v1(g, k).iter().filter(|&&p| p).count() as u32;
                assert_eq!(got, g.ink_bits[i], "v1 glyph {} at k={k}", g.ch);
            }
        }
    }

    #[test]
    fn nx75_v2_checksums_lock_the_odd_k_renderer_to_the_bake() {
        // 31 glyphs × k ∈ {3, 5}: the Rust connection-kernel raster
        // must reproduce the bake-time ink counts exactly — any
        // drift in the stamp law (or the baked v2 data) trips here.
        for g in &glyph_font::GLYPHS_V2 {
            assert_eq!(g.ink_bits.len(), glyph_font::CHECKSUM_KS_V2.len());
            for (i, &k) in glyph_font::CHECKSUM_KS_V2.iter().enumerate() {
                assert_eq!(k % 2, 1, "v2 checksums sit on the ODD parity");
                let got = raster_v2(g, k).iter().filter(|&&p| p).count() as u32;
                assert_eq!(got, g.ink_bits[i], "v2 glyph {} at k={k}", g.ch);
            }
        }
    }

    /// Render `c` through the parity dispatch and format it as rows
    /// of `#`/`.` for the bitmap sanity fixtures.
    fn raster_rows(c: char, k: u32) -> Vec<String> {
        let img = raster_glyph(c, k).expect("nano14 alphabet");
        let w = (glyph_font::GRID_COLS * k) as usize;
        img.chunks(w)
            .map(|row| row.iter().map(|&p| if p { '#' } else { '.' }).collect())
            .collect()
    }

    #[test]
    fn parity_dispatch_even_k_matches_the_true_v1_kernel_pull_images() {
        // K and 8 at k=4 pinned to the TRUE v1 reference renderer's
        // output (/tmp/true_v1.cjs law, cross-checked against the
        // Python baker over all 93 v1 images) — NOT the band-law of
        // older commits. Locks the dispatch (even k → v1 master) AND
        // the full bitmap, beyond what ink counts can.
        let expected_k4 = [
            "####.............###",
            "####............####",
            "####...........#####",
            "####..........#####.",
            "####.........#####..",
            "####........#####...",
            "####.......#####....",
            "####......#####.....",
            "####.....#####......",
            "####....#####.......",
            "####...#####........",
            "####..#####.........",
            "##########..........",
            "#########...........",
            "#########...........",
            "##########..........",
            "####..#####.........",
            "####...#####........",
            "####....#####.......",
            "####.....#####......",
            "####......#####.....",
            "####.......#####....",
            "####........#####...",
            "####.........#####..",
            "####..........#####.",
            "####...........#####",
            "####............####",
            "####.............###",
        ];
        assert_eq!(raster_rows('K', 4), expected_k4, "K at k=4 is v1");
        let expected_84 = [
            ".....##########.....",
            "....############....",
            "...##############...",
            "..################..",
            ".#####........#####.",
            "#####..........#####",
            "####............####",
            "####............####",
            "####............####",
            "####............####",
            "#####..........#####",
            ".#####........#####.",
            "..################..",
            "...##############...",
            "...##############...",
            "..################..",
            ".#####........#####.",
            "#####..........#####",
            "####............####",
            "####............####",
            "####............####",
            "####............####",
            "#####..........#####",
            ".#####........#####.",
            "..################..",
            "...##############...",
            "....############....",
            ".....##########.....",
        ];
        assert_eq!(raster_rows('8', 4), expected_84, "8 at k=4 is v1");
    }

    #[test]
    fn parity_dispatch_odd_k_matches_the_v2_connection_kernel_images() {
        // K and 8 at k=3 pinned to the v2 connection-kernel
        // reference (the editor JS law, cross-checked against the
        // Python baker over all 62 v2 images). Locks the dispatch
        // (odd k → v2 master) AND the full bitmap.
        let expected_k3 = [
            "###.........###",
            "###........####",
            "###.......#####",
            "###......#####.",
            "###.....#####..",
            "###....#####...",
            "###...#####....",
            "###..#####.....",
            "###.#####......",
            "########.......",
            "#######........",
            "########.......",
            "###.#####......",
            "###..#####.....",
            "###...#####....",
            "###....#####...",
            "###.....#####..",
            "###......#####.",
            "###.......#####",
            "###........####",
            "###.........###",
        ];
        assert_eq!(raster_rows('K', 3), expected_k3, "K at k=3 is v2");
        let expected_83 = [
            "...#########...",
            "..###########..",
            ".#############.",
            "#####.....#####",
            "####.......####",
            "###.........###",
            "###.........###",
            "####.......####",
            "#####.....#####",
            ".#############.",
            "..###########..",
            ".#############.",
            "#####.....#####",
            "####.......####",
            "###.........###",
            "###.........###",
            "####.......####",
            "#####.....#####",
            ".#############.",
            "..###########..",
            "...#########...",
        ];
        assert_eq!(raster_rows('8', 3), expected_83, "8 at k=3 is v2");
    }

    #[test]
    fn nx75_covers_exactly_the_nano14_alphabet() {
        assert_eq!(glyph_font::ALPHABET.chars().count(), 31);
        assert_eq!(glyph_font::GLYPHS_V1.len(), 31);
        assert_eq!(glyph_font::GLYPHS_V2.len(), 31);
        for c in glyph_font::ALPHABET.chars() {
            for (g, master) in [
                (glyph_font::glyph_v1(c), "v1"),
                (glyph_font::glyph_v2(c), "v2"),
            ] {
                let g = g.unwrap_or_else(|| panic!("{c} in the {master} master"));
                assert_eq!(g.ch, c);
                assert!(!g.anchors.is_empty(), "{c} has anchors in {master}");
            }
        }
        for c in ['0', 'O', '1', 'I', 'L', ' ', '-', 'a'] {
            assert!(glyph_font::glyph_v1(c).is_none(), "{c} outside v1");
            assert!(glyph_font::glyph_v2(c).is_none(), "{c} outside v2");
        }
    }

    #[test]
    fn nx75_unknown_char_names_the_char_and_the_alphabet() {
        let mut out = String::new();
        let err = write_glyph_rects(&mut out, 'O', 0, 0, 2).expect_err("O has no glyph");
        let msg = err.to_string();
        assert!(matches!(err, CodecError::Render(_)));
        assert!(msg.contains("'O'"), "char named: {msg}");
        assert!(msg.contains(glyph_font::ALPHABET), "alphabet listed: {msg}");
        assert!(out.is_empty(), "nothing emitted on error");
    }

    #[test]
    fn nx75_runs_emit_inside_the_glyph_box() {
        // The run emitter reproduces the raster exactly: per-row
        // x-sorted runs, height 1, never crossing the 5k box.
        for (c, k) in [('K', 2_u32), ('Q', 3), ('W', 4), ('2', 6)] {
            let mut out = String::new();
            write_glyph_rects(&mut out, c, 10, 20, k).expect("renders");
            let svg = format!("<svg xmlns=\"http://www.w3.org/2000/svg\"><g>{out}</g></svg>");
            let rects = content_rects(&svg);
            assert_eq!(
                rects.iter().map(|r| (r.2 * r.3) as usize).sum::<usize>(),
                glyph_ink(c, k),
                "{c} at k={k}: run area == raster ink"
            );
            for (x, y, w, h) in rects {
                assert_eq!(h, 1, "runs are 1px tall");
                assert!(x >= 10 && x + w <= 10 + 5 * k, "{c} inside 5k width");
                assert!(y >= 20 && y < 20 + 7 * k, "{c} inside 7k height");
            }
        }
    }

    #[test]
    fn g_law_rides_the_rendered_label() {
        // The g-law through the public API (clip/pad 0 micro so every
        // size renders): g = min(data/15, m) for the 2-row block.
        for (size, m, g) in [(64_u32, 3_u32, 3_u32), (60, 3, 3), (128, 7, 7), (20, 1, 1)] {
            let l = render_mode(size, 0, true, PaddingMode::Clip);
            assert_eq!(l.module_px, m, "size {size}");
            assert_eq!((l.glyph_cell, l.glyph_px), (7, g), "size {size}");
            assert_eq!(l.height_px, size, "exact canvas");
            let (_, glyph_dots) = split_rects(&l, Layout::Horz);
            assert_eq!(
                glyph_area(&glyph_dots),
                expected_ink(TextFormat::FourFour, g),
                "ink-area ledger at size {size}"
            );
        }
    }

    #[test]
    fn glyph_px_is_maximal_inside_the_module_part() {
        // The g-law. horz, 3 rows: clip@35 → m=2, data 34; the block
        // needs 23g ≤ 34 → g=1 (the m cap not binding).
        let l = render_label_px(
            FIXED_ID,
            Layout::Horz,
            35,
            TextFormat::FiveFiveFour,
            &sym("micro"),
            Padding::uniform(0),
            PaddingMode::Clip,
        )
        .expect("renders");
        assert_eq!((l.glyph_cell, l.glyph_px), (7, 1));
        // vert adds the row-width cap:
        // g = min(data/block_units, data/row_units, module_px).
        let formats = [
            (TextFormat::FourFour, 2_u32, 4_u32),
            (TextFormat::FourFourFour, 3, 4),
            (TextFormat::FiveFiveFour, 3, 5),
        ];
        for size in [64_u32, 100, 300] {
            for (fmt, rows, chars) in formats {
                let l = render_label_px(
                    FIXED_ID,
                    Layout::Vert,
                    size,
                    fmt,
                    &sym("micro"),
                    Padding::uniform(2),
                    PaddingMode::Overlap,
                )
                .expect("renders");
                assert_eq!(l.glyph_cell, 7, "the nx75 cell is 7 rows");
                let want = (l.data_px / (8 * rows - 1))
                    .min(l.data_px / (6 * chars - 1))
                    .min(l.module_px);
                assert_eq!(l.glyph_px, want, "{fmt:?} size {size}");
                assert!(l.glyph_px >= 1);
            }
        }
    }

    #[test]
    fn horz_block_justifies_to_the_module_part() {
        // 64/2 micro 44: m=3, data 51, white (6,6,7,6), tx = 6 + 51 +
        // max(round(1.5·6), 6) = 66. g = min(51/15, 3) = 3. Rows = 2
        // JUSTIFY (ADR-031 §8): first row's top at the module-part
        // top (y = white.top = 6), last row's bottom at its bottom
        // (y = 6 + 51 = 57). slack = 51 − 2·7g = 9 lands in the one
        // inter-row gap, so row 1 top = 6 + 7g + 9 = 36.
        let l = render(64, true);
        assert_eq!((l.glyph_cell, l.glyph_px), (7, 3));
        let (_, glyph_dots) = split_rects(&l, Layout::Horz);
        assert_eq!(
            glyph_area(&glyph_dots),
            expected_ink(TextFormat::FourFour, 3)
        );
        assert_eq!(glyph_dots.iter().map(|r| r.1).min(), Some(6), "block top");
        assert_eq!(
            glyph_dots.iter().map(|r| r.1 + r.3).max(),
            Some(57),
            "block bottom = module-part bottom"
        );
        // FIXED_ID's first chars (K, P) ink column 0, so the first
        // ink rect sits exactly at tx.
        assert_eq!(left_bearing(TextFormat::FourFour, 3), 0);
        assert_eq!(glyph_dots.iter().map(|r| r.0).min(), Some(66));
        // Exact width: the widest row is 4 advances of 6g minus the
        // trailing g gap (69), so the canvas is 66 + 69 + 6 = 141.
        assert_eq!(l.width_px, 66 + 69 + l.white.right);
        assert!(
            glyph_dots.iter().map(|r| r.0 + r.2).max() <= Some(l.width_px - l.white.right),
            "ink never crosses the right white floor"
        );
    }

    #[test]
    fn rect_count_is_dark_modules_plus_glyph_ink() {
        for (spec, fmt) in [
            ("micro", TextFormat::FourFour),
            ("micro", TextFormat::FourFourFour),
            ("micro", TextFormat::FiveFiveFour),
            ("qr", TextFormat::FourFour),
        ] {
            for layout in [Layout::Horz, Layout::Vert] {
                let l = render_label_px(
                    FIXED_ID,
                    layout,
                    100,
                    fmt,
                    &sym(spec),
                    Padding::uniform(2),
                    PaddingMode::Overlap,
                )
                .expect("renders");
                // Both layouts draw nx75 at the label's g — the QR
                // side counts rects, the glyph side counts ink AREA
                // (the emitter merges horizontal runs).
                assert_eq!(l.glyph_cell, 7, "{spec} {fmt:?}");
                let (qr, glyph_dots) = split_rects(&l, layout);
                assert_eq!(qr.len(), dark_modules(spec), "{spec} {layout:?}");
                assert_eq!(
                    glyph_area(&glyph_dots),
                    expected_ink(fmt, l.glyph_px),
                    "{spec} {fmt:?} {layout:?}"
                );
                for (_, _, _, h) in &glyph_dots {
                    assert_eq!(*h, 1, "glyph runs are 1px tall");
                }
            }
        }
    }

    #[test]
    fn glyph_block_that_cannot_fit_errors_with_a_chars_hint() {
        // micro at 25/2 overlap → m=1, module part 17px; a 3-row block
        // needs 7+1+7+1+7 = 23px at 1px per glyph dot.
        let err = render_label_px(
            FIXED_ID,
            Layout::Horz,
            25,
            TextFormat::FiveFiveFour,
            &sym("micro"),
            Padding::uniform(2),
            PaddingMode::Overlap,
        )
        .expect_err("17px module part cannot hold a 3-row block");
        let msg = err.to_string();
        assert!(matches!(err, CodecError::Render(_)), "got {err:?}");
        assert!(msg.contains("3-row"), "rows named: {msg}");
        assert!(msg.contains("23px"), "minimum named: {msg}");
        assert!(msg.contains("--chars"), "fewer-rows hint: {msg}");

        // vert: the width cap binds — a 4-char row is 23px wide at
        // g=1 but the module part is 17px.
        let err = render_label_px(
            FIXED_ID,
            Layout::Vert,
            21,
            TextFormat::FourFour,
            &sym("micro"),
            Padding::uniform(0),
            PaddingMode::Overlap,
        )
        .expect_err("17px module part cannot hold a 4-char row");
        let msg = err.to_string();
        assert!(matches!(err, CodecError::Render(_)), "got {err:?}");
        assert!(msg.contains("4-char"), "row width named: {msg}");
        assert!(msg.contains("23px"), "minimum named: {msg}");
    }

    // ---------- impossible fit errors with mode-aware hints ----------

    #[test]
    fn impossible_fit_errors_with_boundary_hints_per_mode() {
        // Overlap micro, size 20 / pad 0: m=1 needs 17 + 2·2 = 21.
        let err = render_label_px(
            FIXED_ID,
            Layout::Horz,
            20,
            TextFormat::FourFour,
            &sym("micro"),
            Padding::uniform(0),
            PaddingMode::Overlap,
        )
        .expect_err("cannot fit");
        let msg = err.to_string();
        assert!(matches!(err, CodecError::Render(_)));
        assert!(msg.contains("overlap"), "active mode named, got: {msg}");
        // Mins under overlap: m=1 → 21; m=2 → 34+8=42; m=3 → 51+12=63.
        assert!(msg.contains("21px"), "1px-module hint, got: {msg}");
        assert!(msg.contains("42px"), "2px-module hint, got: {msg}");
        assert!(msg.contains("63px"), "3px-module hint, got: {msg}");

        // Additive micro, size 24 / pad 2: m=1 needs 21 + 4 = 25.
        let err = render_label_px(
            FIXED_ID,
            Layout::Horz,
            24,
            TextFormat::FourFour,
            &sym("micro"),
            Padding::uniform(2),
            PaddingMode::Additive,
        )
        .expect_err("cannot fit");
        let msg = err.to_string();
        assert!(msg.contains("additive"), "active mode named, got: {msg}");
        // Mins under additive: 21m + 4 → 25 / 46 / 67.
        assert!(msg.contains("25px"), "1px-module hint, got: {msg}");
        assert!(msg.contains("46px"), "2px-module hint, got: {msg}");
        assert!(msg.contains("67px"), "3px-module hint, got: {msg}");

        // Standard overlap, 28px < 21 + 2·4 = 29.
        let err = render_label_px(
            FIXED_ID,
            Layout::Vert,
            28,
            TextFormat::FourFour,
            &sym("qr"),
            Padding::uniform(0),
            PaddingMode::Overlap,
        )
        .expect_err("28px cannot fit standard V1");
        assert!(err.to_string().contains("29px"), "got: {}", err);
    }

    #[test]
    fn asymmetric_impossible_fit_names_both_pads() {
        // Overlap micro, size 22, top 0 / bottom 9: m=1 needs
        // 17 + max(0,2) + max(9,2) = 28.
        let err = render_label_px(
            FIXED_ID,
            Layout::Horz,
            22,
            TextFormat::FourFour,
            &sym("micro"),
            Padding::sides(0, 0, 9, 0),
            PaddingMode::Overlap,
        )
        .expect_err("cannot fit");
        let msg = err.to_string();
        assert!(msg.contains("0px/9px"), "both pads named, got: {msg}");
        assert!(msg.contains("28px"), "1px-module hint, got: {msg}");
    }

    // ---------- flag is a documented Unsupported ----------

    #[test]
    fn flag_layout_is_unsupported_in_px_mode() {
        let err = render_label_px(
            FIXED_ID,
            Layout::Flag {
                cable_od_mm: 6.0,
                no_markers: false,
                alignment_line: false,
            },
            64,
            TextFormat::FourFour,
            &sym("micro"),
            Padding::uniform(2),
            PaddingMode::Overlap,
        )
        .expect_err("flag has no px geometry yet");
        assert!(matches!(err, CodecError::Unsupported(_)), "got: {err:?}");
    }

    // ---------- integer-px grid ----------

    #[test]
    fn every_rect_sits_on_the_integer_px_grid() {
        for (layout, spec) in [
            (Layout::Horz, "micro"),
            (Layout::Horz, "qr"),
            (Layout::Vert, "micro"),
            (Layout::Vert, "qr"),
        ] {
            let l = render_label_px(
                FIXED_ID,
                layout,
                100,
                TextFormat::FiveFiveFour,
                &sym(spec),
                Padding::uniform(3),
                PaddingMode::Overlap,
            )
            .expect("renders");
            let (qr, glyph_dots) = split_rects(&l, layout);
            assert!(!qr.is_empty(), "QR rects present for {layout:?}");
            assert!(!glyph_dots.is_empty(), "glyph dots present for {layout:?}");
            for (_, _, w, h) in qr {
                assert_eq!((w, h), (l.module_px, l.module_px), "rect == module_px");
            }
            let ink: usize = glyph_dots.iter().map(|r| (r.2 * r.3) as usize).sum();
            assert_eq!(
                ink,
                expected_ink(TextFormat::FiveFiveFour, l.glyph_px),
                "glyph ink area for {layout:?}"
            );
            for (_, _, w, h) in glyph_dots {
                assert_eq!(h, 1, "glyph runs are 1px tall");
                assert!(w >= 1 && w <= 5 * l.glyph_px, "run inside the glyph box");
            }
            assert!(l.svg.contains("shape-rendering=\"crispEdges\""));
        }
    }

    // ---------- vert: glyph block below the QR, centered ----------

    #[test]
    fn vert_glyph_block_sits_below_the_qr_width_justified() {
        let l = render_spec(
            "micro",
            Layout::Vert,
            64,
            Padding::uniform(2),
            PaddingMode::Overlap,
        );
        // Controlling axis is horizontal: m=3, data 51, white left 6 /
        // right 7 (remainder px right), top/bottom at their floors 6.
        // gap = max(round(1.5·6), 6) = 9. The row-width cap binds: a
        // 4-char row is 23g wide, 23·3 > 51, so g=2 (block cap is 3).
        assert_eq!((l.module_px, l.data_px, l.glyph_px), (3, 51, 2));
        assert_eq!(l.glyph_cell, 7, "the nx75 cell is 7 rows");
        let ty0 = l.white.top + l.data_px + 9;
        // Below the QR rows keep their 8g pitch — `block_h` stays
        // (8·rows − 1)·g = 15g, so the canvas height is unchanged.
        let block_h = 15 * l.glyph_px;
        let (_, glyph_dots) = split_rects(&l, Layout::Vert);
        assert_eq!(
            glyph_area(&glyph_dots),
            expected_ink(TextFormat::FourFour, 2)
        );
        assert_eq!(
            glyph_dots.iter().map(|r| r.1).min(),
            Some(ty0),
            "block top = qr bottom + gap"
        );
        // Exact height: block bottom + the bottom white floor, no
        // trailing white.
        assert_eq!(l.height_px, ty0 + block_h + l.white.bottom);
        assert_eq!(
            glyph_dots.iter().map(|r| r.1 + r.3).max(),
            Some(ty0 + block_h),
            "last ink row touches the bottom white floor"
        );
        // Each row JUSTIFIES to the module part width (ADR-031 §8):
        // first glyph's left at the module-part left, so the first
        // ink column sits at white.left + left_bearing.
        assert_eq!(
            glyph_dots.iter().map(|r| r.0).min(),
            Some(l.white.left + left_bearing(TextFormat::FourFour, 2)),
            "first glyph left at module-part left"
        );
        // FIXED_ID's first chars (K, P) ink column 0, so the leftmost
        // ink rect sits exactly at white.left.
        assert_eq!(left_bearing(TextFormat::FourFour, 2), 0);
        // And the last glyph's right edge lands at the module-part
        // right edge: white.left + data_px = 6 + 51 = 57. '3' and 'R'
        // (the last chars of FIXED_ID's two FourFour rows) both ink
        // the rightmost glyph column, so the rightmost ink rect's
        // right edge meets the module-part right edge exactly.
        assert_eq!(
            glyph_dots.iter().map(|r| r.0 + r.2).max(),
            Some(l.white.left + l.data_px),
            "last glyph right at module-part right"
        );
    }

    // ---------- the ADR-031 §8 worked example, full geometry ----------

    #[test]
    fn horz_67px_worked_example_geometry() {
        // size 67 / pad 2, overlap micro: m=3 (51 + 2·max(2,6) = 63),
        // data 51, controlling floors 6/6, remainder 4 lands as top 8
        // and bottom 8, non-controlling sides at their floors (6/6).
        // gap = max(round(1.5·6), 6) = 9 → text at x = 6+51+9 = 66.
        // Typography: g = min(51/15, 3) = 3. Rows JUSTIFY: row 0 top
        // = white.top = 8, row 1 bottom = 8 + 51 = 59. slack = 9 lands
        // in the inter-row gap. Widest row 4·18 − 3 = 69 → width
        // 66 + 69 + 6 = 141.
        let l = render_pad(67, 2, true);
        assert_eq!((l.width_px, l.height_px), (141, 67));
        assert_eq!((l.data_px, l.module_px), (51, 3));
        assert_eq!((l.glyph_cell, l.glyph_px), (7, 3));
        assert_eq!(l.white, Padding::sides(8, 6, 8, 6));
        assert_eq!((l.qr_px, l.modules), (63, 21));
        let (qr, glyph_dots) = split_rects(&l, Layout::Horz);
        assert_eq!(qr.len(), dark_modules("micro"), "QR side of the ledger");
        assert_eq!(
            glyph_area(&glyph_dots),
            expected_ink(TextFormat::FourFour, 3)
        );
        assert_eq!(
            glyph_dots.iter().map(|r| r.0).min(),
            Some(66 + left_bearing(TextFormat::FourFour, 3)),
            "tx + left bearing"
        );
        assert_eq!(glyph_dots.iter().map(|r| r.1).min(), Some(8), "block top");
        // Both rows ink their full 7g boxes, so the last ink row is
        // the block bottom = white.top + data_px = 8 + 51 = 59.
        assert_eq!(
            glyph_dots.iter().map(|r| r.1 + r.3).max(),
            Some(59),
            "block bottom = module-part bottom"
        );
        assert!(
            glyph_dots.iter().map(|r| r.0 + r.2).max() <= Some(l.width_px - l.white.right),
            "ink never crosses the right white floor"
        );
    }

    // ---------- ADR-031 §8 justify: rows flush to the module part,
    // slack distributes across the gaps ----------

    /// 4/4/4 at micro-m3-l clip@64 horz — rows = 3 with slack 18 in
    /// two gaps. Row 0 top at module-part top, row 2 bottom at
    /// module-part bottom, both inter-row gaps equal to 9px.
    #[test]
    fn horz_444_clip_64_m3l_distributes_slack_evenly() {
        // m3-l clip@64 pad 0: m=4, data_px=60, white top/bottom=2/2,
        // left/right=0/0. Gap is clamped to quiet·m = 8 (clip mode
        // floors the right white to 0). For 3 rows of 4 chars:
        // block_units = 23, g = min(60/23, 4) = 2 → 7g = 14.
        // slack = 60 − 3·14 = 18 → base 9, extra 0, gaps 9/9.
        let l = render_label_px(
            FIXED_ID,
            Layout::Horz,
            64,
            TextFormat::FourFourFour,
            &sym("micro-m3-l"),
            Padding::uniform(0),
            PaddingMode::Clip,
        )
        .expect("renders");
        assert_eq!((l.module_px, l.data_px), (4, 60));
        assert_eq!(l.glyph_px, 2);
        assert_eq!((l.white.top, l.white.bottom), (2, 2));
        let (_, glyph_dots) = split_rects(&l, Layout::Horz);
        // FIXED_ID's 4/4/4 rows are "K7M3", "PQ9R", "T5VA"; all three
        // ink the top and bottom raster rows, so the y-extent of the
        // ink matches the placed row boxes exactly.
        let top = l.white.top;
        // Row 0 ink box [top, top+14), row 1 [top+23, top+37), row 2
        // [top+46, top+60). Last ink row = top + 60 = 62.
        assert_eq!(glyph_dots.iter().map(|r| r.1).min(), Some(top));
        assert_eq!(
            glyph_dots.iter().map(|r| r.1 + r.3).max(),
            Some(top + l.data_px),
            "last row bottom = module-part bottom"
        );
        // The middle row sits at y = top + 7g + first gap = top + 14
        // + 9 = top + 23. The 4/4/4 split has row 1 = "PQ9R" — every
        // glyph inks raster row 0, so the row's top y is observable.
        let row1_top = glyph_dots
            .iter()
            .map(|r| r.1)
            .filter(|&y| y > top + 13 && y < top + 23 + 14)
            .min()
            .expect("row 1 ink present");
        assert_eq!(row1_top, top + 23, "row 1 top after a 9px gap");
        // Sanity: both gaps are 9px (top+14..top+23 and top+37..top+46).
        let row0_bottom = top + 14;
        let row1_bottom = top + 23 + 14;
        let row2_top = top + 46;
        assert_eq!(row1_top - row0_bottom, 9, "gap 0 = 9");
        assert_eq!(row2_top - row1_bottom, 9, "gap 1 = 9");
    }

    /// 5/5/4 at the same clip@64 m3-l — also 3 rows, so the same
    /// 9/9 gap split. The chars per row differ but the row pitch
    /// only depends on rows, g, and data_px.
    #[test]
    fn horz_554_clip_64_m3l_distributes_slack_evenly() {
        let l = render_label_px(
            FIXED_ID,
            Layout::Horz,
            64,
            TextFormat::FiveFiveFour,
            &sym("micro-m3-l"),
            Padding::uniform(0),
            PaddingMode::Clip,
        )
        .expect("renders");
        assert_eq!((l.module_px, l.data_px, l.glyph_px), (4, 60, 2));
        let (_, glyph_dots) = split_rects(&l, Layout::Horz);
        let top = l.white.top;
        assert_eq!(glyph_dots.iter().map(|r| r.1).min(), Some(top));
        assert_eq!(
            glyph_dots.iter().map(|r| r.1 + r.3).max(),
            Some(top + l.data_px),
            "last row bottom = module-part bottom"
        );
        // Row 1 starts after row 0 + gap 0 = top + 14 + 9 = top + 23.
        let row1_top = glyph_dots
            .iter()
            .map(|r| r.1)
            .filter(|&y| y > top + 13 && y < top + 37)
            .min()
            .expect("row 1 ink present");
        assert_eq!(row1_top, top + 23, "row 1 top after a 9px gap");
    }

    /// 4/4 at micro-m3-l clip@64 — exact fit (15·4 = 60 = data_px):
    /// slack 0, the gap stays at g and the SVG is byte-identical to
    /// the pre-justify renderer.
    #[test]
    fn horz_44_clip_64_m3l_exact_fit_unchanged() {
        let l = render_label_px(
            FIXED_ID,
            Layout::Horz,
            64,
            TextFormat::FourFour,
            &sym("micro-m3-l"),
            Padding::uniform(0),
            PaddingMode::Clip,
        )
        .expect("renders");
        assert_eq!((l.module_px, l.data_px, l.glyph_px), (4, 60, 4));
        // The exact-fit byte-identical regression: data_px = rows·7g
        // + (rows−1)·g, so slack = 0 and every row sits at its old
        // (8g-pitch, centered-block) offset.
        let top = l.white.top;
        let (_, glyph_dots) = split_rects(&l, Layout::Horz);
        // Old geometry: ty0 = white.top + (data_px − 15g)/2 = top,
        // row 0 at top, row 1 at top + 8g = top + 32. The g-floor
        // gap (g = 4) sits between row 0 bottom (top + 28) and row 1
        // top (top + 32).
        assert_eq!(glyph_dots.iter().map(|r| r.1).min(), Some(top));
        let row1_top = glyph_dots
            .iter()
            .map(|r| r.1)
            .filter(|&y| y >= top + 28)
            .min()
            .expect("row 1 ink present");
        assert_eq!(row1_top, top + 32, "row 1 at 8g pitch");
        // Last ink row sits at the module-part bottom (= top + 60).
        assert_eq!(
            glyph_dots.iter().map(|r| r.1 + r.3).max(),
            Some(top + l.data_px),
        );
    }

    /// vert width-flush equivalent: each row's first glyph sits at
    /// the module-part left, the last glyph's right at the module-part
    /// right, and the inter-char slack distributes the same way.
    #[test]
    fn vert_44_clip_64_m3l_row_justifies_to_the_module_part() {
        // micro-m3-l clip@64 vert pad 0: controlling axis horizontal,
        // m=4, data_px=60. The vert row-width cap binds: g = min(
        // 60/15, 60/23, 4) = min(4, 2, 4) = 2. For 4 chars of 5g=10,
        // slack = 60 − 40 = 20 → base 6, extra 2, gaps 7/7/6.
        let l = render_label_px(
            FIXED_ID,
            Layout::Vert,
            64,
            TextFormat::FourFour,
            &sym("micro-m3-l"),
            Padding::uniform(0),
            PaddingMode::Clip,
        )
        .expect("renders");
        assert_eq!((l.module_px, l.data_px, l.glyph_px), (4, 60, 2));
        let (_, glyph_dots) = split_rects(&l, Layout::Vert);
        // First glyph's leftmost ink sits at white.left + the row's
        // left bearing; FIXED_ID's first chars (K, P) ink column 0,
        // so the leftmost ink rect sits exactly at white.left.
        assert_eq!(left_bearing(TextFormat::FourFour, 2), 0);
        assert_eq!(
            glyph_dots.iter().map(|r| r.0).min(),
            Some(l.white.left),
            "first glyph left at module-part left"
        );
        // The last char's right column lands at white.left + data_px
        // (both '3' and 'R' ink the rightmost glyph column).
        assert_eq!(
            glyph_dots.iter().map(|r| r.0 + r.2).max(),
            Some(l.white.left + l.data_px),
            "last glyph right at module-part right"
        );
        // Char 1 lead = c0 + 5g + gap0 = 10 + 7 = 17 from white.left,
        // and the gap between char 0 (ends at white.left + 10) and
        // char 1 (starts at white.left + 17) is empty. Assert the gap
        // stays ink-free on row 0 (y in the row's 7g-high box at ty0).
        let ty0 = l.white.top + l.data_px + 8; // gap clamp = quiet·m = 8
        let row0_dots = glyph_dots
            .iter()
            .filter(|r| r.1 >= ty0 && r.1 < ty0 + 7 * l.glyph_px);
        let gap_start = l.white.left + 10;
        let gap_end = l.white.left + 17;
        for r in row0_dots {
            let x_end = r.0 + r.2;
            assert!(
                x_end <= gap_start || r.0 >= gap_end,
                "row 0 ink in the inter-char gap: rect {r:?}"
            );
        }
    }

    // ---------- fill_to_max: batch uniformity, padding floor ----------

    #[test]
    fn fill_to_max_makes_a_mixed_batch_uniform() {
        let mut labels = vec![
            render_pad(64, 0, true), // 51px micro module part
            render(100, false),      // 63px standard module part
            render(64, false),       // 42px standard module part
        ];
        let dims_before: Vec<(u32, u32)> =
            labels.iter().map(|l| (l.width_px, l.height_px)).collect();
        assert!(
            dims_before.windows(2).any(|w| w[0] != w[1]),
            "fixture must start non-uniform"
        );

        fill_to_max(&mut labels, Padding::uniform(2));

        let (w, h) = (labels[0].width_px, labels[0].height_px);
        for l in &labels {
            assert_eq!((l.width_px, l.height_px), (w, h), "uniform footprint");
            assert!(l.width_px >= l.data_px + 4, "white floor kept");
            assert!(l.height_px >= l.data_px + 4, "white floor kept");
            roxmltree::Document::parse(&l.svg).expect("padded SVG well-formed");
            assert_eq!(
                l.svg.matches("<svg").count(),
                1,
                "re-wrap must not nest <svg> roots"
            );
        }
        let max_data = labels.iter().map(|l| l.data_px).max().expect("non-empty");
        assert!(
            w >= max_data + 4 && h >= max_data + 4,
            "floor vs largest module part"
        );
    }

    #[test]
    fn fill_to_max_centers_on_integer_offsets() {
        let small = render(25, true); // 17px module part — minimum at pad 2
        let big = render(212, true); // 21·10 = 210 <= 212 → 170px module part
        assert_eq!((small.module_px, big.module_px), (1, 10));
        let mut labels = vec![small, big.clone()];
        fill_to_max(&mut labels, Padding::uniform(2));
        // The big label already had the max dims; unchanged.
        assert_eq!(labels[1].svg, big.svg);
        // The small one was re-wrapped with an integer translate.
        let doc = roxmltree::Document::parse(&labels[0].svg).expect("well-formed");
        let g = doc
            .descendants()
            .find(|n| n.tag_name().name() == "g" && n.attribute("transform").is_some())
            .expect("translate group present");
        let t = g.attribute("transform").expect("transform");
        assert!(t.starts_with("translate("), "got {t:?}");
        let nums: Vec<u32> = t
            .trim_start_matches("translate(")
            .trim_end_matches(')')
            .split(',')
            .map(|s| s.trim().parse().expect("integer offset"))
            .collect();
        assert_eq!(nums.len(), 2);
    }

    #[test]
    fn fill_to_max_respects_asymmetric_padding_floors() {
        let mut labels = vec![render(64, true)];
        // Floors: width ≥ data + left + right = 51 + 30; height ≥
        // data + top + bottom = 51 + 3.
        fill_to_max(&mut labels, Padding::sides(1, 10, 2, 20));
        assert!(
            labels[0].width_px >= 81,
            "width floor: {}",
            labels[0].width_px
        );
        assert!(
            labels[0].height_px >= 54,
            "height floor: {}",
            labels[0].height_px
        );
    }

    #[test]
    fn fill_to_max_on_empty_slice_is_a_no_op() {
        let mut labels: Vec<PxLabel> = Vec::new();
        fill_to_max(&mut labels, Padding::uniform(2));
        assert!(labels.is_empty());
    }
}
