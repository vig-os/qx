//! Golden-output parity test: Rust `crates/codec::render_label` vs
//! the retired Python `label.py`'s `render`.
//!
//! The goldens under `tests/golden/` are the verbatim SVGs produced by
//! `label.py` (segno encoder) immediately before its deletion in
//! ADR-017 step 9 — generated 2026-06-12 with
//! `uv run python -c "import label; label.render(...)"` for each
//! canonical (id, layout, size, fmt, micro) tuple. They are checked in
//! so byte-for-byte parity evidence stays executable without a Python
//! runtime.
//!
//! Per ADR-017 §Consequences the QR encoder produces a one-mask-
//! different bit-matrix from segno, so byte-equivalence of the
//! `<rect>` block is impossible. The remaining structural surface
//! must still match byte-for-byte:
//!
//! - `<svg>` root: same `width`, `height`, `viewBox`.
//! - `<text>` rows: same `x`, `y`, `font-family`, `font-size`,
//!   `text-anchor`, `font-weight`, content.
//! - Wrap-zone overlay (flag layout): same `<rect>` outline + `<text>`.

use std::path::PathBuf;

use part_registry_codec::{render, Layout, TextFormat};

const FIXED_ID: &str = "K7M3PQ9RT5VAXY";

/// Load a checked-in `label.py` golden SVG by file name.
fn golden(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("golden {} unreadable: {e}", path.display()))
}

/// Extract every `<text>` element with `fill="#000"` verbatim. The
/// QR `<rect>` block is excluded (differs by mask selection per
/// ADR-017); the wrap-zone overlay's stroked rect + grey text are
/// included (they are pixel-equivalent).
fn structural_lines(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    // 1. Root <svg ...> opening tag.
    if let Some(end) = svg.find('>') {
        out.push(svg[..=end].into());
    }
    // 2. Every <text> element (ID rows + wrap-zone label).
    let mut rest = svg;
    while let Some(start) = rest.find("<text") {
        rest = &rest[start..];
        if let Some(close) = rest.find("</text>") {
            let body = &rest[..close + "</text>".len()];
            out.push(body.into());
            rest = &rest[close + "</text>".len()..];
        } else {
            break;
        }
    }
    // 3. The wrap-zone stroke rect (flag layout only — has
    //    `stroke="#888"`).
    let mut rest = svg;
    while let Some(start) = rest.find("<rect") {
        rest = &rest[start..];
        if let Some(close) = rest.find("/>") {
            let body = &rest[..close + 2];
            if body.contains("stroke=\"#888\"") {
                out.push(body.into());
            }
            rest = &rest[close + 2..];
        } else {
            break;
        }
    }
    out
}

fn assert_parity(rust_svg: &str, golden_name: &str) {
    let py = golden(golden_name);
    let rs_lines = structural_lines(rust_svg);
    let py_lines = structural_lines(&py);
    assert_eq!(
        rs_lines, py_lines,
        "structural diff vs {golden_name}:\n  rust = {rs_lines:?}\n  py = {py_lines:?}"
    );
}

#[test]
fn parity_horz_11mm_4_4_4() {
    let rs = render(
        FIXED_ID,
        Layout::Horz,
        11.0,
        TextFormat::FourFourFour,
        false,
    );
    assert_parity(&rs, "label_horz_11mm_444.svg");
}

#[test]
fn parity_vert_8mm_4_4() {
    let rs = render(FIXED_ID, Layout::Vert, 8.0, TextFormat::FourFour, false);
    assert_parity(&rs, "label_vert_8mm_44.svg");
}

#[test]
fn parity_vert_6mm_micro_4_4() {
    // Micro QR M4 at 6mm — small-label tier.
    let rs = render(FIXED_ID, Layout::Vert, 6.0, TextFormat::FourFour, true);
    assert_parity(&rs, "label_vert_6mm_micro_44.svg");
}

#[test]
fn parity_flag_d6_11mm_4_4_4() {
    let rs = render(
        FIXED_ID,
        Layout::Flag { cable_od_mm: 6.0 },
        11.0,
        TextFormat::FourFourFour,
        false,
    );
    assert_parity(&rs, "label_flag_d6_11mm_444.svg");
}

#[test]
fn parity_horz_5_5_4_full_canonical() {
    let rs = render(
        FIXED_ID,
        Layout::Horz,
        14.0,
        TextFormat::FiveFiveFour,
        false,
    );
    assert_parity(&rs, "label_horz_14mm_554.svg");
}

#[test]
fn rust_label_qr_round_trips_through_decoder() {
    // Lock the cardinal invariant: encode (Standard QR V1) -> decode
    // -> original canonical ID. This is the load-bearing parity
    // property; the QR mask bit-diff with segno is irrelevant to
    // scanability.
    use image::{DynamicImage, ImageBuffer, Luma};
    use part_registry_codec::{decode_qr, encode};
    let matrix = encode(FIXED_ID, false).unwrap();
    let qz = matrix.quiet_zone();
    let total = matrix.total_modules();
    let module_px = 12u32;
    let dim = total as u32 * module_px;
    let mut img = ImageBuffer::from_pixel(dim, dim, Luma([255u8]));
    for r in 0..matrix.size {
        for c in 0..matrix.size {
            if matrix.get(r, c) {
                let x0 = (c + qz) as u32 * module_px;
                let y0 = (r + qz) as u32 * module_px;
                for dy in 0..module_px {
                    for dx in 0..module_px {
                        img.put_pixel(x0 + dx, y0 + dy, Luma([0u8]));
                    }
                }
            }
        }
    }
    let mut png = Vec::new();
    DynamicImage::ImageLuma8(img)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let decoded = decode_qr(&png).expect("decode succeeds");
    assert_eq!(decoded, FIXED_ID);
}
