//! px-true label renderer (ADR-031 §2–§4, final geometry per §8).
//!
//! The mm-native renderer ([`crate::svg`]) is physically sized but lets
//! the print driver decide where module edges fall; on a thermal head a
//! sub-pixel module edge merges or drops dots and can kill a Micro QR.
//! This module renders in **device pixels** with the ADR-031 §8 law
//! (2026-06-11): the caller asks for an **exact output canvas** and a
//! **padding floor**; padding references the QR's *module part* (the
//! data modules, quiet zone excluded) and the module size is *deduced*
//! per [`PaddingMode`]:
//!
//! ```text
//! overlap  (default): max m with data·m + 2·max(padding_min, quiet·m) ≤ size
//! additive:           max m with (data + 2·quiet)·m + 2·padding_min   ≤ size
//!          → ERROR if no m ≥ 1 fits (payload/QR cannot fit)
//! data_px  = data · m                  (the module part)
//! white    = (size − data_px) / 2      (canvas edge → module part)
//! ```
//!
//! In `overlap` mode the quiet zone's whitespace satisfies padding
//! (printers donate intrinsic outer margins, so the label spends its
//! pixels on modules); `white ≥ quiet·m` is structural either way, so
//! padding can never starve decodability. In `additive` mode the quiet
//! zone is excluded from outside padding (full-bleed/die-cut contexts).
//!
//! Geometry, all derived from that one deduction (§8):
//! - The label's controlling dimension (height for `horz`, width for
//!   `vert`) is **exactly** `size`; the actual `white` is the SAME on
//!   all four canvas edges (an odd remainder leaves its extra pixel on
//!   the bottom/right edge — deterministic).
//! - QR→text gap = `round(1.5 · white)`.
//! - The id-text block is co-sized with the module part (spans exactly
//!   `data_px`, top-aligned with it in `horz`) and lives on the same
//!   module grid: font height and inter-row gaps are integer multiples
//!   of `m` (row gap = one module); any remainder stays at the bottom
//!   of the block. Monospace advance ≈ `29/40 · font` per glyph.
//! - Modules draw at their canvas offsets on a `crispEdges` grid; the
//!   quiet zone is **not** drawn — the white background supplies it.
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
use crate::qr::encode;
use crate::svg::Layout;
use crate::CodecError;

/// How the quiet zone counts toward the outside padding floor
/// (ADR-031 §8).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PaddingMode {
    /// The quiet zone counts toward outside padding — the deduction
    /// maximizes `m` subject to `data·m + 2·max(padding_min, quiet·m)
    /// ≤ size`. Default: printers contribute intrinsic unprintable
    /// margins, so the device already donates outer white.
    #[default]
    Overlap,
    /// The quiet zone is excluded from outside padding —
    /// `(data + 2·quiet)·m + 2·padding_min ≤ size` — for
    /// full-bleed/die-cut contexts where the canvas edge is the
    /// physical edge.
    Additive,
}

impl PaddingMode {
    fn name(self) -> &'static str {
        match self {
            PaddingMode::Overlap => "overlap",
            PaddingMode::Additive => "additive",
        }
    }

    /// Minimum canvas (controlling dimension) for `m` px/module given
    /// a `data`-module symbol with a `quiet`-module quiet zone.
    fn min_size(self, data: u32, quiet: u32, padding_min_px: u32, m: u32) -> u64 {
        let (data, quiet, pad, m) = (
            u64::from(data),
            u64::from(quiet),
            u64::from(padding_min_px),
            u64::from(m),
        );
        match self {
            PaddingMode::Overlap => data * m + 2 * pad.max(quiet * m),
            PaddingMode::Additive => (data + 2 * quiet) * m + 2 * pad,
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
    /// (= `modules * module_px`; the quiet zone itself renders as
    /// background white inside `white_px`).
    pub qr_px: u32,
    /// Pixels per QR module (integer by the §8 law).
    pub module_px: u32,
    /// Symbol modules per edge, quiet zone included (Micro QR M4:
    /// 17 + 2·2 = 21; Standard V1: 21 + 2·4 = 29).
    pub modules: u32,
    /// The module part (data modules only) in device px
    /// (= `data modules * module_px`).
    pub data_px: u32,
    /// Uniform actual padding: canvas edge → module part, same on all
    /// four edges of the standalone render (an odd remainder adds one
    /// extra px on the bottom/right of the controlling dimension).
    pub white_px: u32,
    /// The quiet-zone accounting the deduction ran under.
    pub padding_mode: PaddingMode,
}

/// Largest `m` (px/module) that fits, or 0 when even 1 px/module
/// cannot fit.
fn deduce_module_px(
    size_px: u32,
    padding_min_px: u32,
    data: u32,
    quiet: u32,
    mode: PaddingMode,
) -> u32 {
    let mut module_px = 0;
    let mut m = 1;
    while mode.min_size(data, quiet, padding_min_px, m) <= u64::from(size_px) {
        module_px = m;
        m += 1;
    }
    module_px
}

/// Largest font on the module grid for a `rows`-row text block
/// co-sized with a `data`-module part: font = `k·m`, row gap = `m`
/// (one module), with `rows·k + (rows − 1) ≤ data` so the block never
/// exceeds `data_px`; any remainder stays at the bottom of the block.
fn grid_font_px(data: u32, rows: u32, module_px: u32) -> u32 {
    let k = (data.saturating_sub(rows - 1) / rows).max(1);
    k * module_px
}

/// QR→text gap = `round(1.5 · white)`, half rounding up (§8).
fn qr_text_gap(white: u32) -> u32 {
    white + white.div_ceil(2)
}

/// Render one px-true label.
///
/// `size_px` is the **exact output canvas** along the label's
/// controlling dimension (height for `horz`, width for `vert`);
/// `padding_min_px` is the **minimum** canvas-edge → module-part
/// margin (the §4 floor), with the quiet zone counting toward it per
/// `padding_mode` (§8). The module size is deduced (see module docs)
/// and the remainder distributes uniformly into the actual `white`, so
/// the controlling dimension always comes out exactly `size_px`.
///
/// Errors:
/// - [`CodecError::Render`] when the chosen QR/payload cannot fit one
///   device pixel per module inside the padding floor; the message
///   carries the minimum workable sizes for 1/2/3 px modules under the
///   active `padding_mode`.
/// - [`CodecError::Unsupported`] for [`Layout::Flag`] — the px-true
///   flag geometry (wrap-zone width in device px) is not specified yet;
///   ADR-031 §5 lists it, this renderer covers `horz`/`vert` first.
pub fn render_label_px(
    canonical: &str,
    layout: Layout,
    size_px: u32,
    text_format: TextFormat,
    micro: bool,
    padding_min_px: u32,
    padding_mode: PaddingMode,
) -> Result<PxLabel, CodecError> {
    if matches!(layout, Layout::Flag { .. }) {
        return Err(CodecError::Unsupported(
            "flag layout has no px-true geometry yet (ADR-031 §5); \
             use horz or vert, or unit=mm for flag"
                .into(),
        ));
    }
    let matrix = encode(canonical, micro)?;
    let data = matrix.size as u32;
    let quiet = matrix.quiet_zone() as u32;
    let modules = matrix.total_modules() as u32;
    let module_px = deduce_module_px(size_px, padding_min_px, data, quiet, padding_mode);
    if module_px < 1 {
        return Err(CodecError::Render(format!(
            "size {size_px}px with padding {padding_min_px}px ({mode} mode) cannot fit \
             1px per module for a {data}-module symbol with a {quiet}-module quiet zone; \
             minimum size is {min1}px (1px modules), {min2}px reaches 2px, {min3}px \
             reaches 3px",
            mode = padding_mode.name(),
            min1 = padding_mode.min_size(data, quiet, padding_min_px, 1),
            min2 = padding_mode.min_size(data, quiet, padding_min_px, 2),
            min3 = padding_mode.min_size(data, quiet, padding_min_px, 3),
        )));
    }
    let qr_px = modules * module_px;
    let data_px = data * module_px;
    // Uniform actual padding, canvas edge → module part; the floor'd
    // half goes top/left, an odd remainder leaves its extra pixel on
    // the bottom/right of the controlling dimension.
    let white = (size_px - data_px) / 2;
    let gap = qr_text_gap(white);

    // Text on the module grid (§8): block co-sized with the module
    // part, font = k·m, row gap = m.
    let rows = text_format.split(canonical);
    let n_rows = rows.len() as u32;
    let font_px = grid_font_px(data, n_rows, module_px);
    let row_gap = module_px;
    let max_chars = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0) as u32;
    // Monospace advance ≈ 0.725 · font per glyph (= 29/40, integer math).
    let text_w = font_px * 29 * max_chars / 40;

    let (width_px, height_px, text_lines) = match layout {
        Layout::Horz => {
            // Height is EXACTLY size_px; the module part sits `white`
            // from the top/left and the text block top-aligns with it.
            let tx = white + data_px + gap;
            let w = tx + text_w + white;
            let lines: Vec<String> = rows
                .iter()
                .enumerate()
                .map(|(i, row)| {
                    let ty = white + font_px + i as u32 * (font_px + row_gap);
                    format!("<text x=\"{tx}\" y=\"{ty}\">{row}</text>")
                })
                .collect();
            (w, size_px, lines)
        }
        Layout::Vert => {
            // Width is EXACTLY size_px; the text column stays inside
            // the module part's width, so cap the font (still a
            // multiple of m — snapped down on the grid).
            let font_px = if max_chars > 0 {
                let fit = data_px * 40 / (29 * max_chars);
                font_px.min((fit / module_px).max(1) * module_px)
            } else {
                font_px
            };
            let text_h = n_rows * font_px + (n_rows - 1) * row_gap;
            let ty0 = white + data_px + gap;
            let h = ty0 + text_h + white;
            let cx = size_px / 2;
            let lines: Vec<String> = rows
                .iter()
                .enumerate()
                .map(|(i, row)| {
                    let ty = ty0 + font_px + i as u32 * (font_px + row_gap);
                    format!("<text x=\"{cx}\" y=\"{ty}\" text-anchor=\"middle\">{row}</text>")
                })
                .collect();
            (size_px, h, lines)
        }
        Layout::Flag { .. } => unreachable!("rejected above"),
    };

    // Module rects at their canvas offsets — every coordinate an
    // integer device px. The quiet zone is not drawn; the white
    // background inside `white` supplies it (white ≥ quiet·m is
    // structural under both modes).
    let mut rects = String::with_capacity(matrix.size * matrix.size * 48);
    for r in 0..matrix.size {
        for c in 0..matrix.size {
            if matrix.get(r, c) {
                let x = white + c as u32 * module_px;
                let y = white + r as u32 * module_px;
                let _ = write!(
                    rects,
                    "<rect x=\"{x}\" y=\"{y}\" width=\"{module_px}\" height=\"{module_px}\"/>"
                );
            }
        }
    }

    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width_px}\" height=\"{height_px}\" \
viewBox=\"0 0 {width_px} {height_px}\">\
<rect width=\"{width_px}\" height=\"{height_px}\" fill=\"white\"/>\
<g fill=\"black\" shape-rendering=\"crispEdges\">{rects}</g>\
<g font-family=\"monospace\" font-size=\"{font_px}\" fill=\"black\">{texts}</g>\
</svg>\n",
        texts = text_lines.join(""),
    );

    Ok(PxLabel {
        svg,
        width_px,
        height_px,
        qr_px,
        module_px,
        modules,
        data_px,
        white_px: white,
        padding_mode,
    })
}

/// Pad every label in a print job to the job's largest footprint
/// (ADR-031 §4): the uniform canvas is at least the batch's largest
/// label *and* at least `largest data_px + 2 * padding_px` — padding
/// keeps its §8 white semantics (canvas edge → module part), so
/// `padding_px` stays the smallest allowed canvas→module distance
/// around the biggest module part. Smaller labels are centered
/// (integer offsets — the px grid is preserved); the extra margin sits
/// outside each label's own uniform `white_px`.
pub fn fill_to_max(labels: &mut [PxLabel], padding_px: u32) {
    let Some(max_data) = labels.iter().map(|l| l.data_px).max() else {
        return;
    };
    let floor = max_data + 2 * padding_px;
    let target_w = labels
        .iter()
        .map(|l| l.width_px)
        .max()
        .unwrap_or(0)
        .max(floor);
    let target_h = labels
        .iter()
        .map(|l| l.height_px)
        .max()
        .unwrap_or(0)
        .max(floor);
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

    fn render_mode(size: u32, pad_min: u32, micro: bool, mode: PaddingMode) -> PxLabel {
        render_label_px(
            FIXED_ID,
            Layout::Horz,
            size,
            TextFormat::FourFour,
            micro,
            pad_min,
            mode,
        )
        .expect("px render succeeds")
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
            // The controlling dimension is EXACTLY the requested size.
            assert_eq!(l.height_px, size, "horz height == size exactly");
            // Uniform white absorbed the remainder, kept the floor and
            // the structural quiet-zone minimum.
            assert_eq!(l.white_px, (size - data_px) / 2, "size {size}");
            assert!(
                l.white_px >= pad_min.max(MICRO_QUIET * m),
                "white {} >= max(pad {pad_min}, quiet·m {})",
                l.white_px,
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
            assert_eq!(l.white_px, (size - data_px) / 2);
            // Additive: the quiet zone sits inside white but does NOT
            // satisfy the padding floor.
            assert!(
                l.white_px >= MICRO_QUIET * m + pad_min,
                "white {} >= quiet·m + pad",
                l.white_px,
            );
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
            let l = render_label_px(
                FIXED_ID,
                Layout::Horz,
                size,
                TextFormat::FourFour,
                false,
                pad_min,
                PaddingMode::Overlap,
            )
            .expect("renders");
            assert_eq!(l.modules, STANDARD_TOTAL);
            assert_eq!(l.module_px, m, "size {size}/pad {pad_min}");
            assert_eq!(l.data_px, data_px);
            assert_eq!(l.data_px, STANDARD_DATA * m);
            assert_eq!(l.height_px, size, "exact canvas");
            assert!(l.white_px >= pad_min.max(STANDARD_QUIET * m));
        }
    }

    // ---------- uniform white, gap law, module placement ----------

    /// All non-background `<rect>` x/y/width/height values (the
    /// background rect carries `fill`, modules do not).
    fn module_rects(svg: &str) -> Vec<(u32, u32, u32, u32)> {
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

    #[test]
    fn white_is_uniform_and_quiet_zone_is_not_drawn() {
        // 64/2 overlap micro: m=3, data 51, white 6 (remainder px on
        // the bottom edge of the controlling dimension).
        let l = render(64, true);
        assert_eq!((l.module_px, l.data_px, l.white_px), (3, 51, 6));
        let rects = module_rects(&l.svg);
        assert!(!rects.is_empty());
        let min_x = rects.iter().map(|r| r.0).min().expect("rects");
        let min_y = rects.iter().map(|r| r.1).min().expect("rects");
        let max_y = rects.iter().map(|r| r.1 + r.3).max().expect("rects");
        // Same white on left and top; every module inside the module
        // part — nothing rendered in the quiet zone.
        assert_eq!(min_x, l.white_px, "left white == white_px");
        assert_eq!(min_y, l.white_px, "top white == white_px");
        assert_eq!(max_y, l.white_px + l.data_px, "module part bottom");
        for (x, y, w, h) in &rects {
            assert_eq!((*w, *h), (l.module_px, l.module_px));
            assert!(x + w <= l.white_px + l.data_px, "inside module part");
            assert!(y + h <= l.white_px + l.data_px, "inside module part");
            // On the module grid.
            assert_eq!((x - l.white_px) % l.module_px, 0);
            assert_eq!((y - l.white_px) % l.module_px, 0);
        }
        // Bottom edge absorbs the odd remainder: 64 − (6 + 51) = 7.
        assert_eq!(l.height_px - (l.white_px + l.data_px), 7);
    }

    #[test]
    fn horz_gap_is_one_and_a_half_white() {
        for (size, pad) in [(64_u32, 2_u32), (67, 2), (100, 0), (212, 5)] {
            let l = render_pad(size, pad, true);
            let gap = l.white_px + l.white_px.div_ceil(2);
            let expected_tx = l.white_px + l.data_px + gap;
            let doc = roxmltree::Document::parse(&l.svg).expect("well-formed");
            for text in doc.descendants().filter(|n| n.tag_name().name() == "text") {
                let x: u32 = text.attribute("x").expect("x").parse().expect("integer x");
                assert_eq!(
                    x, expected_tx,
                    "size {size}/pad {pad}: text x = white + data + 1.5·white"
                );
            }
        }
    }

    #[test]
    fn vert_layout_width_is_exactly_the_requested_size() {
        let l = render_label_px(
            FIXED_ID,
            Layout::Vert,
            64,
            TextFormat::FourFour,
            true,
            0,
            PaddingMode::Overlap,
        )
        .expect("renders");
        assert_eq!(l.width_px, 64, "vert width == size exactly");
        assert_eq!((l.module_px, l.data_px, l.white_px), (3, 51, 6));
        let rects = module_rects(&l.svg);
        let min_x = rects.iter().map(|r| r.0).min().expect("rects");
        let min_y = rects.iter().map(|r| r.1).min().expect("rects");
        assert_eq!((min_x, min_y), (6, 6), "left white == top white");
    }

    // ---------- text on the module grid ----------

    fn font_size(svg: &str) -> u32 {
        let doc = roxmltree::Document::parse(svg).expect("well-formed SVG");
        doc.descendants()
            .find_map(|n| n.attribute("font-size"))
            .expect("font-size present")
            .parse()
            .expect("integer font size")
    }

    #[test]
    fn text_block_is_co_sized_on_the_module_grid() {
        let formats = [
            (TextFormat::FourFour, 2_u32),
            (TextFormat::FourFourFour, 3),
            (TextFormat::FiveFiveFour, 3),
        ];
        for (size, micro) in [(64_u32, true), (100, true), (100, false), (300, false)] {
            for (fmt, rows) in formats {
                let l = render_label_px(
                    FIXED_ID,
                    Layout::Horz,
                    size,
                    fmt,
                    micro,
                    2,
                    PaddingMode::Overlap,
                )
                .expect("renders");
                let f = font_size(&l.svg);
                assert_eq!(f % l.module_px, 0, "font is k·m ({fmt:?}, size {size})");
                let block = rows * f + (rows - 1) * l.module_px;
                assert!(
                    block <= l.data_px,
                    "block {block} <= data_px {} ({fmt:?}, size {size})",
                    l.data_px,
                );
            }
        }
        // The exact-fit cases: micro 2 rows → 2·8m + m = 17m = data_px.
        let l = render(64, true);
        assert_eq!(font_size(&l.svg), 24, "k = (17−1)/2 = 8 → 8·3");
        assert_eq!(2 * 24 + 3, l.data_px, "block == data_px exactly");
    }

    #[test]
    fn horz_text_top_aligns_with_the_module_part() {
        // 64/2 micro 44: white 6, font 24, row gap 3 → baselines at
        // 6+24=30 and 30+27=57; the block spans exactly data_px.
        let l = render(64, true);
        assert!(
            l.svg.contains("<text x=\"66\" y=\"30\">K7M3</text>"),
            "{}",
            l.svg
        );
        assert!(
            l.svg.contains("<text x=\"66\" y=\"57\">PQ9R</text>"),
            "{}",
            l.svg
        );
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
            true,
            0,
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
            true,
            2,
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
            false,
            0,
            PaddingMode::Overlap,
        )
        .expect_err("28px cannot fit standard V1");
        assert!(err.to_string().contains("29px"), "got: {}", err);
    }

    // ---------- flag is a documented Unsupported ----------

    #[test]
    fn flag_layout_is_unsupported_in_px_mode() {
        let err = render_label_px(
            FIXED_ID,
            Layout::Flag { cable_od_mm: 6.0 },
            64,
            TextFormat::FourFour,
            true,
            2,
            PaddingMode::Overlap,
        )
        .expect_err("flag has no px geometry yet");
        assert!(matches!(err, CodecError::Unsupported(_)), "got: {err:?}");
    }

    // ---------- integer-px grid ----------

    #[test]
    fn every_qr_rect_sits_on_the_integer_px_grid() {
        for (layout, micro) in [
            (Layout::Horz, true),
            (Layout::Horz, false),
            (Layout::Vert, true),
            (Layout::Vert, false),
        ] {
            let l = render_label_px(
                FIXED_ID,
                layout,
                100,
                TextFormat::FiveFiveFour,
                micro,
                3,
                PaddingMode::Overlap,
            )
            .expect("renders");
            let rects = module_rects(&l.svg);
            assert!(!rects.is_empty(), "QR rects present for {layout:?}");
            for (_, _, w, h) in rects {
                assert_eq!((w, h), (l.module_px, l.module_px), "rect == module_px");
            }
            assert!(l.svg.contains("shape-rendering=\"crispEdges\""));
        }
    }

    // ---------- layouts carry the expected text rows ----------

    fn text_rows(svg: &str) -> Vec<String> {
        let doc = roxmltree::Document::parse(svg).expect("well-formed SVG");
        doc.descendants()
            .filter(|n| n.tag_name().name() == "text")
            .map(|n| n.text().unwrap_or_default().to_string())
            .collect()
    }

    #[test]
    fn horz_layout_renders_text_rows_per_format() {
        let cases: [(TextFormat, &[&str]); 3] = [
            (TextFormat::FourFour, &["K7M3", "PQ9R"]),
            (TextFormat::FourFourFour, &["K7M3", "PQ9R", "T5VA"]),
            (TextFormat::FiveFiveFour, &["K7M3P", "Q9RT5", "VAXY"]),
        ];
        for (fmt, expected) in cases {
            let l = render_label_px(
                FIXED_ID,
                Layout::Horz,
                64,
                fmt,
                true,
                2,
                PaddingMode::Overlap,
            )
            .expect("renders");
            assert_eq!(text_rows(&l.svg), expected, "format {fmt:?}");
        }
    }

    #[test]
    fn vert_layout_renders_text_rows_below_the_qr() {
        let l = render_label_px(
            FIXED_ID,
            Layout::Vert,
            64,
            TextFormat::FourFour,
            true,
            2,
            PaddingMode::Overlap,
        )
        .expect("renders");
        assert_eq!(text_rows(&l.svg), ["K7M3", "PQ9R"]);
        // Text baselines sit below the module part by at least the
        // 1.5·white gap; the font stays on the module grid.
        let gap = l.white_px + l.white_px.div_ceil(2);
        assert_eq!(font_size(&l.svg) % l.module_px, 0);
        let doc = roxmltree::Document::parse(&l.svg).expect("well-formed SVG");
        for text in doc.descendants().filter(|n| n.tag_name().name() == "text") {
            let y: u32 = text.attribute("y").expect("y").parse().expect("integer y");
            assert!(
                y > l.white_px + l.data_px + gap,
                "baseline {y} below QR + gap"
            );
        }
    }

    // ---------- the ADR-031 §8 worked example, full geometry ----------

    #[test]
    fn horz_67px_worked_example_geometry() {
        // size 67 / pad 2, overlap micro: m=3 (51 + 2·max(2,6) = 63),
        // data 51, white (67−51)/2 = 8, gap round(1.5·8) = 12, font
        // 8·3 = 24 → text at x = 8+51+12 = 71, baselines 32 / 59,
        // width 71 + 69 + 8 = 148.
        let l = render_pad(67, 2, true);
        assert_eq!((l.width_px, l.height_px), (148, 67));
        assert_eq!((l.data_px, l.module_px, l.white_px), (51, 3, 8));
        assert_eq!((l.qr_px, l.modules), (63, 21));
        assert!(
            l.svg.contains("<text x=\"71\" y=\"32\">K7M3</text>"),
            "{}",
            l.svg
        );
        assert!(
            l.svg.contains("<text x=\"71\" y=\"59\">PQ9R</text>"),
            "{}",
            l.svg
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

        fill_to_max(&mut labels, 2);

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
        fill_to_max(&mut labels, 2);
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
    fn fill_to_max_on_empty_slice_is_a_no_op() {
        let mut labels: Vec<PxLabel> = Vec::new();
        fill_to_max(&mut labels, 2);
        assert!(labels.is_empty());
    }
}
