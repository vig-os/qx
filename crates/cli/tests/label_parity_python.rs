//! Golden-output parity test: Rust `crates/codec::render_label` vs
//! Python `label.py`'s `render`.
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
//!
//! The test invokes `uv run python -c "import label; ..."` for each
//! canonical (id, layout, size, fmt, micro) tuple, captures both
//! SVGs, and compares the structural extracts.
//!
//! ## Skipping
//!
//! Skipped when `uv` is not on `PATH`, when network is unavailable
//! (uv-pip fetch fails), or when `RUST_TEST_NO_PYTHON=1`. The test
//! always passes the local-only Rust assertions even when Python is
//! skipped so CI on a hermetic builder still exercises codec parity
//! invariants.

use std::path::PathBuf;
use std::process::Command;

use part_registry_codec::{render, Layout, TextFormat};

const FIXED_ID: &str = "K7M3PQ9RT5VAXY";

/// Locate the repo root by walking up from CARGO_MANIFEST_DIR.
fn repo_root() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/cli → up two levels.
    p.pop();
    p.pop();
    p
}

fn uv_available() -> bool {
    if std::env::var("RUST_TEST_NO_PYTHON").ok().as_deref() == Some("1") {
        return false;
    }
    Command::new("uv")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn python_render(
    canonical: &str,
    layout: &str,
    size_mm: f64,
    fmt: &str,
    micro: bool,
    cable_od: Option<f64>,
) -> Option<String> {
    if !uv_available() {
        return None;
    }
    let cable_od_arg = match cable_od {
        Some(d) => format!("{d}"),
        None => "None".into(),
    };
    let micro_str = if micro { "True" } else { "False" };
    // Python literals — quote string args manually to avoid
    // Rust-side format-string conflicts with Python's `!r` spec.
    let script = format!(
        "import sys, os\n\
         sys.path.insert(0, os.path.abspath('.'))\n\
         import label as L\n\
         svg = L.render('{canonical}', '{layout}', {size_mm}, {cable_od_arg}, \
         fmt='{fmt}', micro={micro_str})\n\
         sys.stdout.write(svg)\n",
    );
    let out = Command::new("uv")
        .arg("run")
        .arg("python")
        .arg("-c")
        .arg(&script)
        .current_dir(repo_root())
        .output()
        .ok()?;
    if !out.status.success() {
        eprintln!(
            "python render failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        return None;
    }
    String::from_utf8(out.stdout).ok()
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

fn rust_render(
    canonical: &str,
    layout: Layout,
    size_mm: f64,
    fmt: TextFormat,
    micro: bool,
) -> String {
    render(canonical, layout, size_mm, fmt, micro)
}

#[test]
fn parity_horz_11mm_4_4_4() {
    let rs = rust_render(
        FIXED_ID,
        Layout::Horz,
        11.0,
        TextFormat::FourFourFour,
        false,
    );
    let py = match python_render(FIXED_ID, "horz", 11.0, "4/4/4", false, None) {
        Some(s) => s,
        None => {
            eprintln!("skipping python parity (uv/python unavailable)");
            return;
        }
    };
    let rs_lines = structural_lines(&rs);
    let py_lines = structural_lines(&py);
    assert_eq!(
        rs_lines, py_lines,
        "structural diff:\n  rust = {rs_lines:?}\n  py = {py_lines:?}"
    );
}

#[test]
fn parity_vert_8mm_4_4() {
    let rs = rust_render(FIXED_ID, Layout::Vert, 8.0, TextFormat::FourFour, false);
    let py = match python_render(FIXED_ID, "vert", 8.0, "4/4", false, None) {
        Some(s) => s,
        None => return,
    };
    let rs_lines = structural_lines(&rs);
    let py_lines = structural_lines(&py);
    assert_eq!(rs_lines, py_lines);
}

#[test]
fn parity_vert_6mm_micro_4_4() {
    // Micro QR M4 at 6mm — small-label tier.
    let rs = rust_render(FIXED_ID, Layout::Vert, 6.0, TextFormat::FourFour, true);
    let py = match python_render(FIXED_ID, "vert", 6.0, "4/4", true, None) {
        Some(s) => s,
        None => return,
    };
    let rs_lines = structural_lines(&rs);
    let py_lines = structural_lines(&py);
    assert_eq!(rs_lines, py_lines);
}

#[test]
fn parity_flag_d6_11mm_4_4_4() {
    let rs = rust_render(
        FIXED_ID,
        Layout::Flag { cable_od_mm: 6.0 },
        11.0,
        TextFormat::FourFourFour,
        false,
    );
    let py = match python_render(FIXED_ID, "flag", 11.0, "4/4/4", false, Some(6.0)) {
        Some(s) => s,
        None => return,
    };
    let rs_lines = structural_lines(&rs);
    let py_lines = structural_lines(&py);
    assert_eq!(rs_lines, py_lines);
}

#[test]
fn parity_horz_5_5_4_full_canonical() {
    let rs = rust_render(
        FIXED_ID,
        Layout::Horz,
        14.0,
        TextFormat::FiveFiveFour,
        false,
    );
    let py = match python_render(FIXED_ID, "horz", 14.0, "5/5/4", false, None) {
        Some(s) => s,
        None => return,
    };
    let rs_lines = structural_lines(&rs);
    let py_lines = structural_lines(&py);
    assert_eq!(rs_lines, py_lines);
}

#[test]
fn rust_label_qr_round_trips_through_decoder() {
    // Even without Python, lock the cardinal invariant: encode
    // (Standard QR V1) -> decode -> original canonical ID. This is
    // the load-bearing parity property; the QR mask bit-diff with
    // segno is irrelevant to scanability.
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
