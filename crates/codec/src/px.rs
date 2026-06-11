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
//! - The id-text is **bitmap typography rendered as rects** (§8
//!   typography verdict, 2026-06-11, superseding the same-day
//!   5×7-only call): no `<text>`, no fonts, no rasterizer variance.
//!   For `horz` the renderer picks a vendored Spleen cell
//!   ([`crate::glyphs_spleen`], cells {12, 16, 24, 32, 64}) by the
//!   better-res law — `nominal = rows·cell·k` fitted closest into the
//!   label's **overall** controlling dimension (`size_px`, not
//!   `data_px`); ties favor the larger cell at lower `k`, because
//!   native resolution beats integer upscaling. Rows draw from the
//!   cell's alphabet-wide cap-ink band at integer scale `k` — every
//!   glyph pixel a k-px `<rect>` in the same crispEdges group as the
//!   QR modules; per-char advance is `dwidth·k`. The vertical slack
//!   (`size − cap ink`) distributes as top/between/bottom gaps:
//!   `top = slack/3`, the rest split evenly across the between-gaps
//!   and the bottom with the remainder on the bottom edge —
//!   deterministic, mirroring the validated Spleen-v2 prototype.
//!   Glyph px is hereby **decoupled** from module px (a k=1 native
//!   cell may be finer than the QR modules — the one-lattice
//!   principle holds per element, not across them).
//! - The first-party 5×7 table ([`crate::glyphs`]) is the documented
//!   **floor**: `horz` labels too small for the 12-cell at k=1
//!   (`rows·12 > size`) fall back to the pre-verdict g-law (largest
//!   integer `g` with the block `rows·7g + (rows−1)·g` inside
//!   `data_px`, capped at `module_px`; advance `6g`, row pitch `8g`,
//!   block centered in the module part). The `vert` layout stays on
//!   that floor path entirely for now — the verdict specifies the
//!   selection against the controlling budget and `horz` is the
//!   shipping layout; vert moves over once its width-bound selection
//!   is benched (documented choice, ADR-031 §8).
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

use crate::format::TextFormat;
use crate::glyphs;
use crate::glyphs_spleen::{self, SpleenCell};
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
    /// Device px per bitmap-glyph dot (§8 typography). On the Spleen
    /// path this is the integer scale `k` of the selected cell —
    /// decoupled from `module_px`; on the 5×7 floor path it is the
    /// pre-verdict `g` (largest integer keeping the block inside the
    /// module part, capped at `module_px`).
    pub glyph_px: u32,
    /// The selected glyph cell (§8 typography verdict): 12, 16, 24,
    /// 32, or 64 for the vendored Spleen cells, or 7 for the
    /// first-party 5×7 floor (tiny `horz` labels and the `vert`
    /// layout, which stays on the floor path for now).
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

/// The §8 better-res Spleen selection: for each vendored cell and
/// integer scale `k ≥ 1`, the nominal block `n_rows·cell·k` is fitted
/// closest into `budget` (the label's OVERALL controlling dimension);
/// ties favor the larger cell at the lower `k` — native resolution
/// beats integer upscaling. `None` when even the 12-cell at k=1
/// overflows (`n_rows·12 > budget`): the caller falls back to the 5×7
/// floor.
fn select_spleen(n_rows: u32, budget: u32) -> Option<(&'static SpleenCell, u32)> {
    let mut best: Option<(&'static SpleenCell, u32, u32)> = None;
    for cell in glyphs_spleen::CELLS {
        let unit = n_rows * cell.cell;
        if unit > budget {
            continue;
        }
        let k = budget / unit;
        let nominal = unit * k;
        let better = match best {
            None => true,
            Some((b, _, n)) => nominal > n || (nominal == n && cell.cell > b.cell),
        };
        if better {
            best = Some((cell, k, nominal));
        }
    }
    best.map(|(cell, k, _)| (cell, k))
}

/// The §8 slack distribution, mirroring the validated Spleen-v2
/// prototype: of the vertical slack (`budget − cap ink`) the top gap
/// takes `slack/3` and the rest splits evenly across the `n_rows − 1`
/// between-gaps and the bottom, with the integer remainder landing on
/// the bottom edge — deterministic, top → between(s) → bottom.
/// Returns `(top, between, bottom)`.
fn spleen_gaps(slack: u32, n_rows: u32) -> (u32, u32, u32) {
    let top = slack / 3;
    let rest = slack - top;
    let between = rest / n_rows;
    let bottom = rest - (n_rows - 1) * between;
    (top, between, bottom)
}

/// Append `c`'s Spleen cap-band ink to `out` as `k`-px rects, band
/// top-left at `(x0, y0)` — the same `<rect>` emitter shape the QR
/// modules use. Returns the advance, `dwidth·k`.
///
/// Errors with [`CodecError::Render`] for a char outside the nano14
/// alphabet (defensive: nano14 payloads cannot contain one).
fn write_spleen_rects(
    out: &mut String,
    cell: &SpleenCell,
    c: char,
    x0: u32,
    y0: u32,
    k: u32,
) -> Result<u32, CodecError> {
    let (dwidth, rows) = cell.glyph(c).ok_or_else(|| {
        CodecError::Render(format!(
            "no spleen glyph for {c:?}: the vendored {cell}-cell table covers \
             the nano14 alphabet {alphabet}",
            cell = cell.cell,
            alphabet = glyphs::ALPHABET,
        ))
    })?;
    for (ry, row) in rows.iter().enumerate() {
        for rx in 0..dwidth {
            if (row >> (dwidth - 1 - rx)) & 1 == 1 {
                let x = x0 + rx * k;
                let y = y0 + ry as u32 * k;
                let _ = write!(
                    out,
                    "<rect x=\"{x}\" y=\"{y}\" width=\"{k}\" height=\"{k}\"/>"
                );
            }
        }
    }
    Ok(dwidth * k)
}

/// The 5×7 floor geometry (the pre-verdict g-law): all values in
/// device px, derived from the module part. See the module docs.
struct FloorBlock {
    glyph_px: u32,
    block_h: u32,
    advance: u32,
    row_pitch: u32,
    max_row_w: u32,
}

/// Deduce the 5×7 floor block for `layout`: the glyph pixel `g` is
/// the largest integer keeping the id-text block — `n_rows` rows of
/// 7g glyphs with 1g gaps between rows — inside `data_px`, capped at
/// `module_px` so text dots equal QR dots when they fit. In `vert`
/// the rows must also fit the module-part width.
fn floor_block(
    layout: Layout,
    data_px: u32,
    module_px: u32,
    n_rows: u32,
    max_chars: u32,
) -> Result<FloorBlock, CodecError> {
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
    Ok(FloorBlock {
        glyph_px,
        block_h: block_units * glyph_px,
        advance: 6 * glyph_px,
        row_pitch: 8 * glyph_px,
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
///   outside the [`crate::glyphs`] alphabet.
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
    if matches!(layout, Layout::Flag { .. }) {
        return Err(CodecError::Unsupported(
            "flag layout has no px-true geometry yet (ADR-031 §5); \
             use horz or vert, or unit=mm for flag"
                .into(),
        ));
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
    // Controlling axis: floors + remainder, extra px on the
    // bottom/right edge (deterministic). Non-controlling sides sit at
    // their floors exactly — that dimension is an output, not a budget.
    let floor_a = padding_mode.floor_px(pad_a, quiet, module_px);
    let floor_b = padding_mode.floor_px(pad_b, quiet, module_px);
    let rem = size_px - data_px - floor_a - floor_b;
    let (white_a, white_b) = (floor_a + rem / 2, floor_b + rem / 2 + rem % 2);
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

    // §8 typography verdict: horz selects a vendored Spleen cell by
    // the better-res law against the OVERALL controlling dimension —
    // the 5×7 floor catches horz labels too small for the 12-cell at
    // k=1 and the whole vert layout (documented choice, module docs).
    let rows = text_format.split(canonical);
    let n_rows = rows.len() as u32;
    let max_chars = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0) as u32;
    let spleen = match layout {
        Layout::Horz => select_spleen(n_rows, size_px),
        _ => None,
    };
    let floor = match spleen {
        Some(_) => None,
        None => Some(floor_block(layout, data_px, module_px, n_rows, max_chars)?),
    };
    let (glyph_px, glyph_cell) = match (spleen, &floor) {
        (Some((cell, k)), _) => (k, cell.cell),
        (None, Some(f)) => (f.glyph_px, glyphs::GLYPH_ROWS),
        (None, None) => unreachable!("either the spleen or the floor path is computed"),
    };

    // Module rects at their canvas offsets — every coordinate an
    // integer device px. The quiet zone is not drawn; the white
    // background supplies it (white ≥ quiet·m is structural under
    // overlap/additive; under clip the printer's intrinsic margins
    // supply it physically).
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
    let (width_px, height_px) = match layout {
        Layout::Horz => {
            // Height is EXACTLY size_px; the text sits right of the QR
            // at the clamped gap.
            let tx = white.left + data_px + gap;
            if let Some((cell, k)) = spleen {
                // §8 Spleen path: rows draw from the cell's cap-ink
                // band at scale k; the vertical slack distributes
                // top/between/bottom across the FULL canvas height and
                // the canvas ends at the widest row (the sum of the
                // dwidth advances) plus the right white floor.
                let ink_row = cell.band * k;
                let slack = size_px - n_rows * ink_row;
                let (top, between, _bottom) = spleen_gaps(slack, n_rows);
                let mut max_row_w = 0;
                let mut y0 = top;
                for row in &rows {
                    let mut x0 = tx;
                    for ch in row.chars() {
                        x0 += write_spleen_rects(&mut rects, cell, ch, x0, y0, k)?;
                    }
                    max_row_w = max_row_w.max(x0 - tx);
                    y0 += ink_row + between;
                }
                (tx + max_row_w + white.right, size_px)
            } else {
                // 5×7 floor: the block centers vertically in the
                // module part span and the canvas ends exactly at the
                // widest row plus the right white floor — no trailing
                // white.
                let f = floor.as_ref().expect("floor computed when spleen is None");
                let ty0 = white.top + (data_px - f.block_h) / 2;
                for (ri, row) in rows.iter().enumerate() {
                    let y0 = ty0 + ri as u32 * f.row_pitch;
                    for (ci, ch) in row.chars().enumerate() {
                        let x0 = tx + ci as u32 * f.advance;
                        glyphs::write_glyph_rects(&mut rects, ch, x0, y0, f.glyph_px)?;
                    }
                }
                (tx + f.max_row_w + white.right, size_px)
            }
        }
        Layout::Vert => {
            // Width is EXACTLY size_px; the text sits below the QR at
            // the clamped gap, each row centers horizontally in the
            // module part span, and the canvas ends exactly at the
            // block bottom plus the bottom white floor. Vert stays on
            // the 5×7 floor path (module docs).
            let f = floor.as_ref().expect("vert always computes the floor");
            let ty0 = white.top + data_px + gap;
            for (ri, row) in rows.iter().enumerate() {
                let chars = row.chars().count() as u32;
                let row_w = (chars * f.advance).saturating_sub(f.glyph_px);
                let x_row = white.left + (data_px - row_w) / 2;
                let y0 = ty0 + ri as u32 * f.row_pitch;
                for (ci, ch) in row.chars().enumerate() {
                    let x0 = x_row + ci as u32 * f.advance;
                    glyphs::write_glyph_rects(&mut rects, ch, x0, y0, f.glyph_px)?;
                }
            }
            (size_px, ty0 + f.block_h + white.bottom)
        }
        Layout::Flag { .. } => unreachable!("rejected above"),
    };

    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width_px}\" height=\"{height_px}\" \
viewBox=\"0 0 {width_px} {height_px}\">\
<rect width=\"{width_px}\" height=\"{height_px}\" fill=\"white\"/>\
<g fill=\"black\" shape-rendering=\"crispEdges\">{rects}</g>\
</svg>\n",
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
        glyph_cell,
        white,
        padding_mode,
        symbology: resolved.compact(),
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
        l.svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{target_w}\" height=\"{target_h}\" \
viewBox=\"0 0 {target_w} {target_h}\">\
<rect width=\"{target_w}\" height=\"{target_h}\" fill=\"white\"/>\
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
        // Typography is DECOUPLED from module px under the §8 verdict:
        // the 64px budget selects the 16×32 Spleen cell at k=1
        // regardless of what the symbology's module deduction yields.
        assert_eq!((l.glyph_cell, l.glyph_px), (32, 1));
        // The cap-ink rows span the FULL canvas under the slack law:
        // ink 2·24 = 48, slack 16 → top 5 / between 5 / bottom 6, and
        // row 2 of FIXED_ID holds the descending Q, so the last ink
        // row touches the bottom gap exactly.
        let (_, glyph_dots) = split_rects(&l, Layout::Horz);
        assert_eq!(glyph_dots.iter().map(|r| r.1).min(), Some(5), "top gap");
        assert_eq!(
            glyph_dots.iter().map(|r| r.1 + r.3).max(),
            Some(58),
            "5 + 24 + 5 + 24 = 64 − bottom 6"
        );
        let m4 = render_mode(64, 0, true, PaddingMode::Clip);
        assert_eq!(m4.module_px, 3, "M4 (17 modules) reaches only 3px");
        assert_eq!(
            (m4.glyph_cell, m4.glyph_px),
            (32, 1),
            "same budget, same cell — typography no longer follows m"
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
        // 15 → tx = 6 + 51 + 15 = 72; the first Spleen ink rect sits
        // the cell's left bearing (16×32 at k=1: 2 dots) inside that.
        assert_eq!((l.glyph_cell, l.glyph_px), (32, 1));
        let (_, glyph_dots) = split_rects(&l, Layout::Horz);
        assert_eq!(
            glyph_dots.iter().map(|r| r.0).min(),
            Some(72 + spleen_left_bearing(32, TextFormat::FourFour)),
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

    /// Total ink dots of FIXED_ID's text rows under `fmt` — the glyph
    /// side of the rect-count ledger, counted from the table the
    /// label selected: the vendored Spleen cell for `glyph_cell` in
    /// {12, 16, 24, 32, 64}, the first-party 5×7 floor for 7.
    fn expected_ink(fmt: TextFormat, glyph_cell: u32) -> usize {
        fmt.split(FIXED_ID)
            .iter()
            .flat_map(|r| r.chars())
            .map(|c| match glyphs_spleen::cell(glyph_cell) {
                Some(cell) => cell.ink_bits(c).expect("nano14 alphabet") as usize,
                None => crate::glyphs::ink_bits(c).expect("nano14 alphabet") as usize,
            })
            .sum()
    }

    /// Leftmost ink column, in glyph dots, across the first chars of
    /// FIXED_ID's rows under `fmt` in the selected Spleen cell. The
    /// Spleen glyphs carry a left bearing inside their dwidth box, so
    /// the first ink rect sits at `tx + bearing·k` (the 5×7 floor
    /// glyphs ink column 0, hence bearing 0 on that path).
    fn spleen_left_bearing(glyph_cell: u32, fmt: TextFormat) -> u32 {
        let Some(cell) = glyphs_spleen::cell(glyph_cell) else {
            return 0;
        };
        fmt.split(FIXED_ID)
            .iter()
            .filter_map(|r| r.chars().next())
            .map(|c| {
                let (dwidth, rows) = cell.glyph(c).expect("nano14 alphabet");
                rows.iter()
                    .filter(|r| **r != 0)
                    .map(|r| dwidth - (32 - r.leading_zeros()))
                    .min()
                    .expect("glyph has ink")
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
            // The Spleen cells carry a left bearing inside the dwidth
            // box, so the first ink rect sits bearing·k after tx.
            let bearing = spleen_left_bearing(l.glyph_cell, TextFormat::FourFour) * l.glyph_px;
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
        let bearing = spleen_left_bearing(l.glyph_cell, TextFormat::FourFour) * l.glyph_px;
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
    fn spleen_selection_better_res_table() {
        // (budget, rows) → (cell, k): the max nominal rows·cell·k that
        // fits the budget wins; nominal ties break toward the larger
        // cell at the lower k (native resolution beats upscaling).
        let cases = [
            // 16-cell k=2 ties the 32-cell k=1 at nominal 64 → 32.
            (64_u32, 2_u32, Some((32_u32, 1_u32))),
            // 12-cell k=2 ties the 24-cell k=1 at nominal 48 → 24.
            (60, 2, Some((24, 1))),
            // 32-cell k=2 ties the 64-cell k=1 at nominal 128 → 64.
            (128, 2, Some((64, 1))),
            // 2·12 = 24 > 20: even the 12-cell at k=1 overflows → the
            // 5×7 floor.
            (20, 2, None),
            // Three-way nominal-96 tie (12·k4 / 16·k3 / 24·k2); the
            // 32-cell reaches only 64 here → 24 at k=2.
            (100, 2, Some((24, 2))),
            // 3 rows: 16·k2 and 32·k1 tie at nominal 96 → 32.
            (100, 3, Some((32, 1))),
        ];
        for (budget, rows, want) in cases {
            let got = select_spleen(rows, budget).map(|(c, k)| (c.cell, k));
            assert_eq!(got, want, "budget {budget} rows {rows}");
        }
    }

    #[test]
    fn spleen_selection_rides_the_rendered_label() {
        // The §8 selection through the public API: the budget is the
        // OVERALL controlling dimension, independent of module px
        // (clip/pad 0 micro so every size renders).
        for (size, cell, k) in [(64_u32, 32_u32, 1_u32), (60, 24, 1), (128, 64, 1)] {
            let l = render_mode(size, 0, true, PaddingMode::Clip);
            assert_eq!((l.glyph_cell, l.glyph_px), (cell, k), "size {size}");
            assert_eq!(l.height_px, size, "exact canvas");
        }
        // Below the 12-cell the 5×7 g-law takes over: clip@20 → m=1,
        // module part 17, block 15g ≤ 17 → g=1.
        let tiny = render_mode(20, 0, true, PaddingMode::Clip);
        assert_eq!((tiny.glyph_cell, tiny.glyph_px), (7, 1), "the floor");
        let (_, glyph_dots) = split_rects(&tiny, Layout::Horz);
        assert_eq!(glyph_dots.len(), expected_ink(TextFormat::FourFour, 7));
    }

    #[test]
    fn spleen_slack_distribution_is_deterministic() {
        // top = slack/3, the rest split across the between-gaps and
        // the bottom, integer remainder on the bottom edge — the
        // validated prototype's arithmetic, generalized to 3 rows.
        assert_eq!(spleen_gaps(16, 2), (5, 5, 6));
        assert_eq!(spleen_gaps(19, 2), (6, 6, 7));
        assert_eq!(spleen_gaps(0, 2), (0, 0, 0));
        assert_eq!(spleen_gaps(28, 3), (9, 6, 7));
        // Gaps + ink always reconstruct the budget exactly.
        for slack in 0..96_u32 {
            for n_rows in [1_u32, 2, 3] {
                let (top, between, bottom) = spleen_gaps(slack, n_rows);
                assert_eq!(
                    top + (n_rows - 1) * between + bottom,
                    slack,
                    "slack {slack} rows {n_rows}"
                );
            }
        }
    }

    #[test]
    fn floor_glyph_px_is_maximal_inside_the_module_part() {
        // The pre-verdict g-law still governs the floor path. horz, 3
        // rows below the 36px 12-cell threshold: clip@35 → m=2, data
        // 34; the block needs 23g ≤ 34 → g=1 (the m cap not binding).
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
        // vert stays on the floor at EVERY size (documented choice):
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
                assert_eq!(l.glyph_cell, 7, "vert is the documented floor");
                let want = (l.data_px / (8 * rows - 1))
                    .min(l.data_px / (6 * chars - 1))
                    .min(l.module_px);
                assert_eq!(l.glyph_px, want, "{fmt:?} size {size}");
                assert!(l.glyph_px >= 1);
            }
        }
    }

    #[test]
    fn horz_spleen_rows_distribute_the_slack_across_the_canvas() {
        // 64/2 micro 44: m=3, data 51, white (6,6,7,6), tx = 6 + 51 +
        // max(round(1.5·6), 6) = 66. Budget 64 selects 16×32 @ k=1:
        // cap band 24, ink 48, slack 16 → top 5 / between 5 / bottom 6.
        let l = render(64, true);
        assert_eq!((l.glyph_cell, l.glyph_px), (32, 1));
        let (_, glyph_dots) = split_rects(&l, Layout::Horz);
        assert_eq!(glyph_dots.len(), expected_ink(TextFormat::FourFour, 32));
        assert_eq!(glyph_dots.iter().map(|r| r.1).min(), Some(5), "top gap");
        // Row 2 (PQ9R) holds the descending Q, which inks the band's
        // last row: 5 + 24 + 5 + 24 = 58 = 64 − bottom 6.
        assert_eq!(glyph_dots.iter().map(|r| r.1 + r.3).max(), Some(58));
        // Every dot is k-sized on the k lattice anchored at (66, 5) —
        // trivially so at k=1; the left bearing keeps min x at 66 + 2.
        for (_, _, w, h) in &glyph_dots {
            assert_eq!((*w, *h), (1, 1), "dot == glyph_px == k");
        }
        assert_eq!(
            glyph_dots.iter().map(|r| r.0).min(),
            Some(66 + spleen_left_bearing(32, TextFormat::FourFour)),
        );
        // Exact width: the widest row is the sum of the dwidth
        // advances (4·16·1 = 64) — the advance carries the cell's own
        // inter-char spacing, so the canvas is 66 + 64 + 6 = 136.
        assert_eq!(l.width_px, 66 + 64 + l.white.right);
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
                // horz at 100px rides the Spleen path; vert stays on
                // the 5×7 floor — the ledger counts from whichever
                // table glyph_cell names.
                match layout {
                    Layout::Horz => assert_ne!(l.glyph_cell, 7, "{spec} {fmt:?}"),
                    _ => assert_eq!(l.glyph_cell, 7, "{spec} {fmt:?}"),
                }
                let (qr, glyph_dots) = split_rects(&l, layout);
                assert_eq!(qr.len(), dark_modules(spec), "{spec} {layout:?}");
                assert_eq!(
                    glyph_dots.len(),
                    expected_ink(fmt, l.glyph_cell),
                    "{spec} {fmt:?} {layout:?}"
                );
                for (_, _, w, h) in &glyph_dots {
                    assert_eq!((*w, *h), (l.glyph_px, l.glyph_px), "dot == glyph_px");
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
            Layout::Flag { cable_od_mm: 6.0 },
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
            for (_, _, w, h) in glyph_dots {
                assert_eq!((w, h), (l.glyph_px, l.glyph_px), "dot == glyph_px");
            }
            assert!(l.svg.contains("shape-rendering=\"crispEdges\""));
        }
    }

    // ---------- vert: glyph block below the QR, centered ----------

    #[test]
    fn vert_glyph_block_sits_below_the_qr_centered() {
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
        assert_eq!(l.glyph_cell, 7, "vert stays on the 5×7 floor");
        let ty0 = l.white.top + l.data_px + 9;
        let block_h = 15 * l.glyph_px;
        let (_, glyph_dots) = split_rects(&l, Layout::Vert);
        assert_eq!(glyph_dots.len(), expected_ink(TextFormat::FourFour, 7));
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
        // Rows center in the module part span: 4·12 − 2 = 46 wide →
        // x starts at 6 + (51 − 46)/2 = 8.
        assert_eq!(glyph_dots.iter().map(|r| r.0).min(), Some(8));
    }

    // ---------- the ADR-031 §8 worked example, full geometry ----------

    #[test]
    fn horz_67px_worked_example_geometry() {
        // size 67 / pad 2, overlap micro: m=3 (51 + 2·max(2,6) = 63),
        // data 51, controlling floors 6/6, remainder 4 lands as top 8
        // and bottom 8, non-controlling sides at their floors (6/6).
        // gap = max(round(1.5·6), 6) = 9 → text at x = 6+51+9 = 66.
        // Typography: budget 67 → 16×32 @ k=1 (nominal 64), cap band
        // 24, ink 48, slack 19 → top 6 / between 6 / bottom 7; widest
        // row 4·16 = 64 → width 66 + 64 + 6 = 136.
        let l = render_pad(67, 2, true);
        assert_eq!((l.width_px, l.height_px), (136, 67));
        assert_eq!((l.data_px, l.module_px), (51, 3));
        assert_eq!((l.glyph_cell, l.glyph_px), (32, 1));
        assert_eq!(l.white, Padding::sides(8, 6, 8, 6));
        assert_eq!((l.qr_px, l.modules), (63, 21));
        let (qr, glyph_dots) = split_rects(&l, Layout::Horz);
        assert_eq!(qr.len(), dark_modules("micro"), "QR side of the ledger");
        assert_eq!(glyph_dots.len(), expected_ink(TextFormat::FourFour, 32));
        assert_eq!(
            glyph_dots.iter().map(|r| r.0).min(),
            Some(66 + spleen_left_bearing(32, TextFormat::FourFour)),
            "tx + left bearing"
        );
        assert_eq!(glyph_dots.iter().map(|r| r.1).min(), Some(6), "top gap");
        // Row 2 (PQ9R) holds the descending Q, which inks the band's
        // last row: 6 + 24 + 6 + 24 = 60 = 67 − bottom 7.
        assert_eq!(
            glyph_dots.iter().map(|r| r.1 + r.3).max(),
            Some(60),
            "bottom gap"
        );
        assert!(
            glyph_dots.iter().map(|r| r.0 + r.2).max() <= Some(l.width_px - l.white.right),
            "ink never crosses the right white floor"
        );
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
