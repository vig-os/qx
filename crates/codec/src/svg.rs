//! mm-native SVG label renderer.
//!
//! Ported from `label.py:95-251`. Output is functionally equivalent
//! to the Python reference — same payload, same text-block sizing
//! math, same font, same viewBox — but the QR module pattern differs
//! by the one-time mask-selection diff accepted in
//! ADR-017 §Consequences.

use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

use crate::format::TextFormat;
use crate::qr::{encode, QrMatrix};

/// Consolas at 0.55 advance ratio is true monospace and fills the
/// 4/4 format square with zero horizontal margin (label.py:144).
pub const FONT_FAMILY: &str = "Consolas, monospace";

/// Layout dispatcher. Matches `label.py:render`'s `layout` arg.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    /// QR on top of text. Aspect 1:2.
    Vert,
    /// QR left of text. Aspect 2:1.
    Horz,
    /// `horz` mirrored around a cable-wrap zone. Aspect (4+wrap):1.
    Flag {
        /// Cable outer diameter in mm. The wrap-zone width is
        /// `π · cable_od · 1.1` (`label.py:231`).
        cable_od_mm: f64,
    },
}

// ---------- low-level primitives ----------

/// Wrap an SVG body in an mm-native `<svg>` root. Mirrors
/// `label.py:svg_wrap` (lines 97-103).
fn svg_wrap(w_mm: f64, h_mm: f64, body: &str) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" \
width=\"{w:.3}mm\" height=\"{h:.3}mm\" \
viewBox=\"0 0 {w:.3} {h:.3}\">\n{body}\n</svg>\n",
        w = w_mm,
        h = h_mm,
        body = body,
    )
}

/// Render a QR matrix as a `size × size` square of `<rect>` modules
/// starting at `(x, y)`. Mirrors `label.py:qr_block` (lines 106-132).
fn qr_block(matrix: &QrMatrix, x: f64, y: f64, size: f64) -> String {
    let border = matrix.quiet_zone();
    let n_modules = matrix.total_modules();
    let module = size / n_modules as f64;
    let mut out = String::with_capacity(matrix.size * matrix.size * 64);
    for r in 0..matrix.size {
        for c in 0..matrix.size {
            if matrix.get(r, c) {
                let rx = x + (c + border) as f64 * module;
                let ry = y + (r + border) as f64 * module;
                let _ = writeln!(
                    out,
                    "<rect x=\"{rx:.3}\" y=\"{ry:.3}\" \
width=\"{module:.3}\" height=\"{module:.3}\" fill=\"#000\"/>"
                );
            }
        }
    }
    // Trim trailing newline so the join() pattern in callers matches
    // Python's `"\n".join(...)` output exactly.
    out.pop();
    out
}

/// Render text rows into a `size × size` square at `(x, y)`. Mirrors
/// `label.py:text_block` (lines 135-163). The sizing math is the
/// load-bearing parity invariant:
///
/// ```text
/// inner_h = size * 0.92
/// font    = inner_h / (n_rows + 0.2 * (n_rows - 1))
/// gap     = font * 0.2
/// ```
fn text_block(canonical: &str, x: f64, y: f64, size: f64, fmt: TextFormat) -> String {
    let rows = fmt.split(canonical);
    let n_rows = rows.len() as f64;
    let inner_h = size * 0.92;
    let font = inner_h / (n_rows + 0.2 * (n_rows - 1.0));
    let gap = font * 0.2;
    let cx = x + size / 2.0;
    let y0 = y + (size - inner_h) / 2.0 + font * 0.85;

    let mut parts = Vec::with_capacity(rows.len());
    for (i, row) in rows.iter().enumerate() {
        let ty = y0 + i as f64 * (font + gap);
        // font-weight bold, no stroke — see label.py:154-156.
        // Stroke on small text rasterises with a ghost-halo on print
        // (commit 9328cd7); leaning on bold + larger glyph instead.
        parts.push(format!(
            "<text x=\"{cx:.3}\" y=\"{ty:.3}\" \
font-family=\"{FONT_FAMILY}\" \
font-weight=\"bold\" font-size=\"{font:.3}\" \
text-anchor=\"middle\" fill=\"#000\">{row}</text>"
        ));
    }
    parts.join("\n")
}

// ---------- layouts ----------

/// Render a vertical label (QR on top of text). Mirrors
/// `label.py:render_vert`.
pub fn render_vert(canonical: &str, size_mm: f64, fmt: TextFormat, micro: bool) -> String {
    let matrix = encode(canonical, micro).expect("QR encode of canonical ID never fails");
    let qr = qr_block(&matrix, 0.0, 0.0, size_mm);
    let text = text_block(canonical, 0.0, size_mm, size_mm, fmt);
    let body = format!("{qr}\n{text}");
    svg_wrap(size_mm, 2.0 * size_mm, &body)
}

/// Render a horizontal label (QR left of text). Mirrors
/// `label.py:render_horz`.
pub fn render_horz(canonical: &str, size_mm: f64, fmt: TextFormat, micro: bool) -> String {
    let matrix = encode(canonical, micro).expect("QR encode of canonical ID never fails");
    let qr = qr_block(&matrix, 0.0, 0.0, size_mm);
    let text = text_block(canonical, size_mm, 0.0, size_mm, fmt);
    let body = format!("{qr}\n{text}");
    svg_wrap(2.0 * size_mm, size_mm, &body)
}

/// Render a cable-flag label: `horz` mirrored around a cable-wrap zone.
/// Mirrors `label.py:render_flag` (lines 229-251).
pub fn render_flag(
    canonical: &str,
    size_mm: f64,
    cable_od_mm: f64,
    fmt: TextFormat,
    micro: bool,
) -> String {
    let matrix = encode(canonical, micro).expect("QR encode of canonical ID never fails");
    let horz_w = 2.0 * size_mm;
    let wrap_w = std::f64::consts::PI * cable_od_mm * 1.1;
    let total_w = 2.0 * horz_w + wrap_w;

    let left_qr = qr_block(&matrix, 0.0, 0.0, size_mm);
    let left_text = text_block(canonical, size_mm, 0.0, size_mm, fmt);
    let left = format!("{left_qr}\n{left_text}");

    let rx = horz_w + wrap_w;
    let right_text = text_block(canonical, rx, 0.0, size_mm, fmt);
    let right_qr = qr_block(&matrix, rx + size_mm, 0.0, size_mm);
    let right = format!("{right_text}\n{right_qr}");

    let wrap = format!(
        "<rect x=\"{horz_w:.3}\" y=\"0\" width=\"{wrap_w:.3}\" height=\"{size_mm:.3}\" \
fill=\"none\" stroke=\"#888\" stroke-width=\"0.1\" stroke-dasharray=\"0.6,0.6\"/>\n\
<text x=\"{wx:.3}\" y=\"{wy:.3}\" \
font-family=\"monospace\" font-size=\"1.5\" \
text-anchor=\"middle\" fill=\"#888\">wrap d{cable_od_mm}</text>",
        wx = horz_w + wrap_w / 2.0,
        wy = size_mm / 2.0 + 0.5,
    );

    let body = [left.as_str(), wrap.as_str(), right.as_str()].join("\n");
    svg_wrap(total_w, size_mm, &body)
}

/// Dispatch by layout. Mirrors `label.py:render`.
pub fn render(
    canonical: &str,
    layout: Layout,
    size_mm: f64,
    fmt: TextFormat,
    micro: bool,
) -> String {
    match layout {
        Layout::Vert => render_vert(canonical, size_mm, fmt, micro),
        Layout::Horz => render_horz(canonical, size_mm, fmt, micro),
        Layout::Flag { cable_od_mm } => render_flag(canonical, size_mm, cable_od_mm, fmt, micro),
    }
}

// ---------- tests ----------

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixed fixture mirroring `test_labels.py:35`. 14-char canonical
    /// from the ADR-012 alphabet.
    const FIXED_ID: &str = "K7M3PQ9RT5VAXY";

    #[cfg(feature = "decoder")]
    use crate::qr::decode_qr;
    use crate::qr::{encode, QrMatrix};

    /// Rasterise a `QrMatrix` to a PNG byte buffer at the given
    /// per-module pixel size. Test-only helper: production code does
    /// not need a raster of the matrix (SVG is the output surface).
    #[cfg(feature = "decoder")]
    fn rasterise_matrix(matrix: &QrMatrix, module_px: u32) -> Vec<u8> {
        use image::{DynamicImage, ImageBuffer, Luma};
        let qz = matrix.quiet_zone();
        let total = matrix.total_modules();
        let dim = (total as u32) * module_px;
        let mut img = ImageBuffer::from_pixel(dim, dim, Luma([255u8]));
        for r in 0..matrix.size {
            for c in 0..matrix.size {
                if matrix.get(r, c) {
                    let x0 = ((c + qz) as u32) * module_px;
                    let y0 = ((r + qz) as u32) * module_px;
                    for dy in 0..module_px {
                        for dx in 0..module_px {
                            img.put_pixel(x0 + dx, y0 + dy, Luma([0u8]));
                        }
                    }
                }
            }
        }
        let mut out = Vec::new();
        DynamicImage::ImageLuma8(img)
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .expect("png write");
        out
    }

    // ---------- 1. QR roundtrip — the cardinal invariant ----------

    #[cfg(feature = "decoder")]
    #[test]
    fn standard_qr_roundtrip() {
        let matrix = encode(FIXED_ID, false).unwrap();
        // Standard QR V1 is 21×21 modules; ISO/IEC 18004 §6.3.3.
        assert_eq!(matrix.size, 21);
        assert!(!matrix.micro);
        let png = rasterise_matrix(&matrix, 12);
        let decoded = decode_qr(&png).expect("rxing decodes standard QR");
        assert_eq!(decoded, FIXED_ID);
    }

    #[cfg(feature = "decoder")]
    #[test]
    fn micro_qr_roundtrip() {
        let matrix = encode(FIXED_ID, true).unwrap();
        // Micro QR M4 is 17×17 modules; ISO/IEC 18004 §6.3.3.
        assert_eq!(matrix.size, 17);
        assert!(matrix.micro);
        let png = rasterise_matrix(&matrix, 16);
        let decoded = decode_qr(&png).expect("rxing decodes micro QR");
        assert_eq!(decoded, FIXED_ID);
    }

    // ---------- 2. All format × layout combinations ----------

    fn all_formats() -> [TextFormat; 3] {
        [
            TextFormat::FourFour,
            TextFormat::FourFourFour,
            TextFormat::FiveFiveFour,
        ]
    }

    fn all_layouts() -> [Layout; 3] {
        [
            Layout::Vert,
            Layout::Horz,
            Layout::Flag { cable_od_mm: 6.0 },
        ]
    }

    #[test]
    fn every_format_layout_combination_renders_parseable_svg() {
        for size in [6.0_f64, 8.0, 11.0] {
            for fmt in all_formats() {
                for layout in all_layouts() {
                    let svg = render(FIXED_ID, layout, size, fmt, false);
                    let doc = roxmltree::Document::parse(&svg).unwrap_or_else(|e| {
                        panic!("SVG not well-formed for {layout:?}/{fmt:?}@{size}: {e}")
                    });
                    let root = doc.root_element();
                    assert_eq!(root.tag_name().name(), "svg");
                    // Each text row is one <text> element. Flag mirrors,
                    // so flag = 2× n_rows + 1 (the wrap-zone label).
                    let n_text = doc
                        .descendants()
                        .filter(|n| n.tag_name().name() == "text")
                        .count();
                    let expected = match layout {
                        Layout::Vert | Layout::Horz => fmt.n_rows(),
                        Layout::Flag { .. } => 2 * fmt.n_rows() + 1,
                    };
                    assert_eq!(
                        n_text, expected,
                        "text-element count mismatch {layout:?}/{fmt:?}@{size}"
                    );
                }
            }
        }
    }

    // ---------- 3. Text is a prefix of the canonical ID ----------

    /// Mirrors `test_labels.py:_TEXT_RE` — concatenates contents of
    /// every `<text … fill="#000">…</text>` in document order. Hand-
    /// rolled rather than regex to keep the dev-dep set tiny.
    fn extract_black_text(svg: &str) -> String {
        let mut out = String::new();
        let mut rest = svg;
        while let Some(start) = rest.find("<text") {
            rest = &rest[start..];
            let Some(open_end) = rest.find('>') else {
                break;
            };
            let attrs = &rest[..open_end];
            rest = &rest[open_end + 1..];
            let Some(close) = rest.find("</text>") else {
                break;
            };
            let body = &rest[..close];
            rest = &rest[close + "</text>".len()..];
            if attrs.contains("fill=\"#000\"") {
                out.push_str(body);
            }
        }
        out
    }

    #[test]
    fn displayed_text_is_prefix_of_canonical() {
        for size in [6.0_f64, 8.0, 11.0] {
            for fmt in all_formats() {
                for layout in all_layouts() {
                    let svg = render(FIXED_ID, layout, size, fmt, false);
                    let displayed = extract_black_text(&svg);
                    let prefix = match layout {
                        Layout::Flag { .. } => {
                            // text block rendered twice
                            let half = displayed.len() / 2;
                            assert_eq!(
                                &displayed[..half],
                                &displayed[half..],
                                "flag text not mirrored: {displayed:?}"
                            );
                            displayed[..half].to_string()
                        }
                        _ => displayed,
                    };
                    assert!(
                        FIXED_ID.starts_with(&prefix),
                        "text {prefix:?} not prefix of canonical for {layout:?}/{fmt:?}@{size}"
                    );
                }
            }
        }
    }

    // ---------- 4. Format auto-selection ----------

    #[test]
    fn recommend_format_size_tiers() {
        use crate::format::recommend_format;
        let (fmt, warn) = recommend_format(6.0);
        assert_eq!(fmt, TextFormat::FourFour);
        assert!(warn.is_none(), "6mm should not warn, got {warn:?}");

        let (fmt, warn) = recommend_format(11.0);
        assert_eq!(fmt, TextFormat::FourFourFour);
        assert!(warn.is_none(), "11mm should not warn, got {warn:?}");

        let (fmt, warn) = recommend_format(4.0);
        assert_eq!(fmt, TextFormat::FourFour);
        let w = warn.expect("4mm must warn");
        assert!(
            w.contains("size < 5mm"),
            "expected size<5mm warning, got {w:?}"
        );
    }

    // ---------- 5. Format warning ----------

    #[test]
    fn check_format_warning_flags_suboptimal_choices() {
        use crate::format::check_format_warning;
        // 6mm + 4/4/4 → below 'comfortable' tier
        let w = check_format_warning(6.0, TextFormat::FourFourFour);
        assert!(w.is_some(), "6mm + 4/4/4 should warn");
        // 11mm + 4/4 → overkill
        let w = check_format_warning(11.0, TextFormat::FourFour);
        assert!(w.is_some(), "11mm + 4/4 should warn");
        // 8mm + 4/4 → optimal, no warning
        let w = check_format_warning(8.0, TextFormat::FourFour);
        assert!(w.is_none(), "8mm + 4/4 should not warn, got {w:?}");
    }

    // ---------- 6. mm-native viewBox / width / height ----------

    #[test]
    fn viewbox_matches_layout_dimensions() {
        let s = 11.0_f64;
        let v = render(FIXED_ID, Layout::Vert, s, TextFormat::FourFourFour, false);
        assert!(v.contains(&format!("width=\"{:.3}mm\"", s)), "vert width");
        assert!(
            v.contains(&format!("height=\"{:.3}mm\"", 2.0 * s)),
            "vert height"
        );
        assert!(
            v.contains(&format!("viewBox=\"0 0 {:.3} {:.3}\"", s, 2.0 * s)),
            "vert viewBox"
        );

        let h = render(FIXED_ID, Layout::Horz, s, TextFormat::FourFourFour, false);
        assert!(
            h.contains(&format!("width=\"{:.3}mm\"", 2.0 * s)),
            "horz width"
        );
        assert!(h.contains(&format!("height=\"{:.3}mm\"", s)), "horz height");
        assert!(
            h.contains(&format!("viewBox=\"0 0 {:.3} {:.3}\"", 2.0 * s, s)),
            "horz viewBox"
        );

        let cable_od = 6.0_f64;
        let f = render(
            FIXED_ID,
            Layout::Flag {
                cable_od_mm: cable_od,
            },
            s,
            TextFormat::FourFourFour,
            false,
        );
        let wrap_w = std::f64::consts::PI * cable_od * 1.1;
        let total_w = 4.0 * s + wrap_w;
        assert!(
            f.contains(&format!("width=\"{:.3}mm\"", total_w)),
            "flag width"
        );
        assert!(f.contains(&format!("height=\"{:.3}mm\"", s)), "flag height");
    }
}
