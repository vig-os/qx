//! px-true label renderer (ADR-031 §2–§4).
//!
//! The mm-native renderer ([`crate::svg`]) is physically sized but lets
//! the print driver decide where module edges fall; on a thermal head a
//! sub-pixel module edge merges or drops dots and can kill a Micro QR.
//! This module renders in **device pixels** with the ADR-031 §2 law
//! (corrected 2026-06-11 — see the ADR's Corrections entry): the
//! caller asks for an **exact output canvas** and a **padding floor**;
//! the module size is *deduced*:
//!
//! ```text
//! available  = size_px - 2 * padding_min_px
//! module_px  = floor(available / N)       N = modules incl. quiet zone
//!            → ERROR if module_px < 1     (payload/QR cannot fit)
//! symbol_px  = N * module_px              ⇒ symbol_px % N == 0, always
//! actual_pad = (size_px - symbol_px) / 2  (absorbs the remainder; ≥ floor)
//! ```
//!
//! The label's controlling dimension (height for `horz`, width for
//! `vert`) is **exactly** `size_px`; the remainder distributes into
//! padding — which is *why* §4 defines padding as a floor. Every QR
//! `<rect>` lands at integer px on a `crispEdges` grid — the structure
//! proven on hardware by `tools/printer_test_62mm.py` (696 px = 62 mm
//! tape) and the ADR-031 fast-path prototype. Geometry constants
//! generalize that prototype: QR→text gap = `2·module_px`, inter-row
//! gap = `module_px + 1`, font size = `qr_px / n_rows − (module_px +
//! 1)`, per-char advance = `0.725·font` (monospace), all in integer px.
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

/// One px-true rendered label.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PxLabel {
    /// The SVG document; width/height/viewBox in device px.
    pub svg: String,
    /// Canvas width in device px.
    pub width_px: u32,
    /// Canvas height in device px.
    pub height_px: u32,
    /// Rendered QR symbol edge in device px (= `modules * module_px`).
    pub qr_px: u32,
    /// Pixels per QR module (integer by the §2 law).
    pub module_px: u32,
    /// Symbol modules per edge, quiet zone included (Micro QR M4:
    /// 17 + 2·2 = 21; Standard V1: 21 + 2·4 = 29).
    pub modules: u32,
}

/// Render one px-true label.
///
/// `size_px` is the **exact output canvas** along the label's
/// controlling dimension (height for `horz`, width for `vert`);
/// `padding_min_px` is the **minimum** margin (the §4 floor). The
/// module size is deduced — `floor((size_px − 2·padding_min_px) / N)` —
/// and the remainder distributes into the actual padding, so the
/// controlling dimension always comes out exactly `size_px`.
///
/// Errors:
/// - [`CodecError::Render`] when the chosen QR/payload cannot fit one
///   device pixel per module inside the padding floor; the message
///   carries the minimum workable size and the next module-size
///   thresholds (`size ≥ N·m + 2·padding`).
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
) -> Result<PxLabel, CodecError> {
    if matches!(layout, Layout::Flag { .. }) {
        return Err(CodecError::Unsupported(
            "flag layout has no px-true geometry yet (ADR-031 §5); \
             use horz or vert, or unit=mm for flag"
                .into(),
        ));
    }
    let matrix = encode(canonical, micro)?;
    let modules = matrix.total_modules() as u32;
    let available = size_px.saturating_sub(2 * padding_min_px);
    let module_px = available / modules;
    if module_px < 1 {
        return Err(CodecError::Render(format!(
            "size {size_px}px with padding {padding_min_px}px leaves {available}px for a \
             {modules}-module symbol (quiet zone included) — cannot fit 1px per module; \
             minimum size is {min1}px (1px modules), {min2}px reaches 2px, {min3}px \
             reaches 3px",
            min1 = modules + 2 * padding_min_px,
            min2 = 2 * modules + 2 * padding_min_px,
            min3 = 3 * modules + 2 * padding_min_px,
        )));
    }
    let qr_px = modules * module_px;
    // The remainder beyond the floor distributes into the actual
    // padding (top/left get the floor'd half; the controlling
    // dimension stays exactly size_px).
    let pad = (size_px - qr_px) / 2;

    // Generalized prototype constants (see module docs).
    let qr_text_gap = 2 * module_px;
    let row_gap = module_px + 1;
    let rows = text_format.split(canonical);
    let n_rows = rows.len() as u32;
    let font_px = (qr_px / n_rows).saturating_sub(module_px + 1).max(1);
    let max_chars = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0) as u32;
    // Monospace advance ≈ 0.725 · font per glyph (= 29/40, integer math).
    let text_w = font_px * 29 * max_chars / 40;

    let (width_px, height_px, qr_x, qr_y, text_lines) = match layout {
        Layout::Horz => {
            // Height is EXACTLY size_px; the QR centers in it (the
            // remainder lands below). Width derives from content with
            // the same actual padding on both ends.
            let h = size_px;
            let qr_y = pad;
            let tx = pad + qr_px + qr_text_gap;
            let w = tx + text_w + pad;
            let lines: Vec<String> = rows
                .iter()
                .enumerate()
                .map(|(i, row)| {
                    let ty = qr_y + font_px + i as u32 * (font_px + row_gap);
                    format!("<text x=\"{tx}\" y=\"{ty}\">{row}</text>")
                })
                .collect();
            (w, h, pad, qr_y, lines)
        }
        Layout::Vert => {
            // Width is EXACTLY size_px; QR and text center in it.
            // Cap the font so the text never escapes the canvas.
            let w = size_px;
            let avail_w = size_px.saturating_sub(2 * padding_min_px).max(1);
            let font_fit = if max_chars > 0 {
                (avail_w * 40 / (29 * max_chars)).max(1)
            } else {
                font_px
            };
            let font_px = font_px.min(font_fit);
            let row_gap = module_px + 1;
            let text_h = n_rows * font_px + (n_rows - 1) * row_gap;
            // pad = (size_px − qr_px) / 2, so the QR centers at pad.
            let qr_x = pad;
            let h = pad + qr_px + qr_text_gap + text_h + pad;
            let cx = w / 2;
            let ty0 = pad + qr_px + qr_text_gap;
            let lines: Vec<String> = rows
                .iter()
                .enumerate()
                .map(|(i, row)| {
                    let ty = ty0 + font_px + i as u32 * (font_px + row_gap);
                    format!("<text x=\"{cx}\" y=\"{ty}\" text-anchor=\"middle\">{row}</text>")
                })
                .collect();
            (w, h, qr_x, pad, lines)
        }
        Layout::Flag { .. } => unreachable!("rejected above"),
    };

    // Module rects — every coordinate an integer device px.
    let qz = matrix.quiet_zone() as u32;
    let mut rects = String::with_capacity(matrix.size * matrix.size * 48);
    for r in 0..matrix.size {
        for c in 0..matrix.size {
            if matrix.get(r, c) {
                let x = qr_x + (c as u32 + qz) * module_px;
                let y = qr_y + (r as u32 + qz) * module_px;
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
    })
}

/// Pad every label in a print job to the job's largest footprint
/// (ADR-031 §4): the uniform canvas is at least the batch's largest
/// label *and* at least `largest qr_px + 2 * padding_px`, so
/// `padding_px` stays the smallest allowed gap around the biggest
/// symbol. Smaller labels are centered (integer offsets — the px grid
/// is preserved).
pub fn fill_to_max(labels: &mut [PxLabel], padding_px: u32) {
    let Some(max_qr) = labels.iter().map(|l| l.qr_px).max() else {
        return;
    };
    let floor = max_qr + 2 * padding_px;
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

    /// Micro QR M4: 17 modules + 2·2 quiet zone.
    const MICRO_N: u32 = 21;
    /// Standard QR V1: 21 modules + 2·4 quiet zone.
    const STANDARD_N: u32 = 29;

    fn render_pad(size: u32, pad_min: u32, micro: bool) -> PxLabel {
        render_label_px(
            FIXED_ID,
            Layout::Horz,
            size,
            TextFormat::FourFour,
            micro,
            pad_min,
        )
        .expect("px render succeeds")
    }

    fn render(size: u32, micro: bool) -> PxLabel {
        render_pad(size, 2, micro)
    }

    // ---------- the corrected §2 law: size = exact canvas, padding =
    // floor, module DEDUCED ----------

    #[test]
    fn micro_module_is_deduced_from_canvas_minus_padding_floor() {
        // (size, pad_min, expected module_px, expected qr_px) for N=21
        // — the hardware-validated boundary table (2026-06-11).
        let cases = [
            (64, 2, 2, 42), // available 60 -> 2px modules
            (64, 1, 2, 42), // available 62 still misses 63
            (64, 0, 3, 63), // available 64 -> 3px modules
            (65, 1, 3, 63), // first 3px size at pad 1
            (67, 2, 3, 63), // first 3px size at pad 2
            (84, 0, 4, 84), // exact fit, zero remainder
            (25, 2, 1, 21), // minimum workable at pad 2
        ];
        for (size, pad_min, module_px, qr_px) in cases {
            let l = render_pad(size, pad_min, true);
            assert_eq!(l.modules, MICRO_N, "size {size}/pad {pad_min}");
            assert_eq!(l.module_px, module_px, "size {size}/pad {pad_min}");
            assert_eq!(l.qr_px, qr_px, "size {size}/pad {pad_min}");
            assert_eq!(l.qr_px % l.modules, 0, "size {size}/pad {pad_min}");
            // The controlling dimension is EXACTLY the requested size.
            assert_eq!(l.height_px, size, "horz height == size exactly");
            // Actual padding absorbed the remainder and kept the floor.
            assert!(
                (size - qr_px) / 2 >= pad_min,
                "actual pad >= floor for size {size}/pad {pad_min}"
            );
        }
    }

    #[test]
    fn standard_module_is_deduced_the_same_way() {
        // N = 29 (21 + 2·4 quiet zone).
        for (size, pad_min) in [(29_u32, 0_u32), (64, 0), (64, 2), (100, 2), (300, 4)] {
            let l = render_label_px(
                FIXED_ID,
                Layout::Horz,
                size,
                TextFormat::FourFour,
                false,
                pad_min,
            )
            .expect("renders");
            let available = size - 2 * pad_min;
            assert_eq!(l.modules, STANDARD_N);
            assert_eq!(l.module_px, available / STANDARD_N, "size {size}");
            assert_eq!(l.qr_px % l.modules, 0);
            assert_eq!(l.height_px, size, "exact canvas");
        }
    }

    #[test]
    fn vert_layout_width_is_exactly_the_requested_size() {
        let l = render_label_px(FIXED_ID, Layout::Vert, 64, TextFormat::FourFour, true, 0)
            .expect("renders");
        assert_eq!(l.width_px, 64, "vert width == size exactly");
        assert_eq!((l.module_px, l.qr_px), (3, 63));
    }

    // ---------- impossible fit errors with boundary hints ----------

    #[test]
    fn impossible_fit_errors_with_boundary_hints() {
        // size 22 / pad 1: available 20 < 21 modules.
        let err = render_label_px(FIXED_ID, Layout::Horz, 22, TextFormat::FourFour, true, 1)
            .expect_err("cannot fit");
        let msg = err.to_string();
        assert!(matches!(err, CodecError::Render(_)));
        // Hints: min for 1px = 21 + 2 = 23; 2px = 44; 3px = 65.
        assert!(msg.contains("23px"), "1px-module hint, got: {msg}");
        assert!(msg.contains("44px"), "2px-module hint, got: {msg}");
        assert!(msg.contains("65px"), "3px-module hint, got: {msg}");

        let err = render_label_px(FIXED_ID, Layout::Vert, 28, TextFormat::FourFour, false, 0)
            .expect_err("28px cannot fit 29 modules");
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
            let l = render_label_px(FIXED_ID, layout, 100, TextFormat::FiveFiveFour, micro, 3)
                .expect("renders");
            let doc = roxmltree::Document::parse(&l.svg).expect("well-formed SVG");
            let mut n_rects = 0;
            for rect in doc
                .descendants()
                .filter(|n| n.tag_name().name() == "rect" && n.attribute("fill").is_none())
            {
                n_rects += 1;
                for attr in ["x", "y", "width", "height"] {
                    let v = rect.attribute(attr).expect("rect attr present");
                    let parsed: u32 = v
                        .parse()
                        .unwrap_or_else(|_| panic!("{attr}={v:?} is not an integer px coordinate"));
                    let _ = parsed;
                }
                assert_eq!(
                    rect.attribute("width"),
                    Some(l.module_px.to_string().as_str()),
                    "module rect width == module_px"
                );
            }
            assert!(n_rects > 0, "QR rects present for {layout:?}");
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
            let l = render_label_px(FIXED_ID, Layout::Horz, 64, fmt, true, 2).expect("renders");
            assert_eq!(text_rows(&l.svg), expected, "format {fmt:?}");
        }
    }

    #[test]
    fn vert_layout_renders_text_rows_below_the_qr() {
        let l = render_label_px(FIXED_ID, Layout::Vert, 64, TextFormat::FourFour, true, 2)
            .expect("renders");
        assert_eq!(text_rows(&l.svg), ["K7M3", "PQ9R"]);
        // Text baselines sit below the QR block.
        let doc = roxmltree::Document::parse(&l.svg).expect("well-formed SVG");
        for text in doc.descendants().filter(|n| n.tag_name().name() == "text") {
            let y: u32 = text.attribute("y").expect("y").parse().expect("integer y");
            assert!(y > 2 + l.qr_px, "baseline {y} below QR bottom");
        }
    }

    // ---------- the validated prototype shape (64px / 44 / pad 2) ----------

    #[test]
    fn horz_67px_matches_the_hardware_validated_geometry() {
        // /tmp/adr031_labels.py, printed + scanned 2026-06-11:
        // 151×67 px canvas, QR 63px @ 3px modules, text at x=71.
        // Under the corrected semantics that geometry is requested as
        // size 67 / padding 2 (available 63 → 3px modules).
        let l = render(67, true);
        assert_eq!((l.width_px, l.height_px), (151, 67));
        assert_eq!((l.qr_px, l.module_px, l.modules), (63, 3, 21));
        assert!(l.svg.contains("<text x=\"71\" y=\"29\">K7M3</text>"));
        assert!(l.svg.contains("<text x=\"71\" y=\"60\">PQ9R</text>"));
    }

    // ---------- fill_to_max: batch uniformity, padding floor ----------

    #[test]
    fn fill_to_max_makes_a_mixed_batch_uniform() {
        let mut labels = vec![
            render_pad(64, 0, true), // 63px micro symbol
            render(100, false),      // 87px standard symbol
            render(33, false),       // 29px standard symbol (avail 29)
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
            assert!(l.width_px >= l.qr_px + 4, "padding floor kept");
            assert!(l.height_px >= l.qr_px + 4, "padding floor kept");
            roxmltree::Document::parse(&l.svg).expect("padded SVG well-formed");
            assert_eq!(
                l.svg.matches("<svg").count(),
                1,
                "re-wrap must not nest <svg> roots"
            );
        }
        let max_qr = labels.iter().map(|l| l.qr_px).max().expect("non-empty");
        assert!(w >= max_qr + 4 && h >= max_qr + 4, "floor vs largest QR");
    }

    #[test]
    fn fill_to_max_centers_on_integer_offsets() {
        let small = render(25, true); // 21px symbol — the minimum at pad 2
        let big = render(212, true); // 189px symbol (available 208 → 9px modules)
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
