//! In-core SVG rasterisation for `pr print --emit png|jpeg|pdf`
//! (ADR-031 §8). Resolves the ADR's "raster in core vs CLI" open
//! question in favour of an in-process path: no external
//! `rsvg-convert`, deterministic across operator machines.
//!
//! Gated behind the `raster` cargo feature (default-on for the `pr`
//! binary, off under `--no-default-features` for the lean CI build).
//! The wasm façade (`crates/wasm`) does NOT depend on this crate, so
//! the FE bundle is untouched (foundation #33); browsers rasterise
//! SVG via `<canvas>.toBlob` instead.
//!
//! Each function parses the SVG with the usvg re-exported by its own
//! backend crate (`resvg` for raster, `svg2pdf` for PDF), so there is
//! no cross-crate usvg version coupling to keep in sync.

/// Raster output formats understood by `--emit`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Emit {
    Svg,
    Png,
    Jpeg,
    Pdf,
}

impl Emit {
    /// Parse the `--emit` value. `svg` needs no raster backend.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "svg" => Ok(Emit::Svg),
            "png" => Ok(Emit::Png),
            "jpeg" | "jpg" => Ok(Emit::Jpeg),
            "pdf" => Ok(Emit::Pdf),
            other => Err(format!(
                "unknown --emit {other:?}; expected svg | png | jpeg | pdf"
            )),
        }
    }

    /// File extension for this format.
    pub fn ext(self) -> &'static str {
        match self {
            Emit::Svg => "svg",
            Emit::Png => "png",
            Emit::Jpeg => "jpeg",
            Emit::Pdf => "pdf",
        }
    }
}

/// Render an SVG document to the bytes of `emit`'s format.
///
/// `Emit::Svg` returns the SVG bytes unchanged. `png|jpeg|pdf` require
/// the `raster` feature; without it they return an error so the caller
/// can surface a clear "rebuild with --features raster" message.
#[cfg(feature = "raster")]
pub fn render(svg: &str, emit: Emit) -> Result<Vec<u8>, String> {
    match emit {
        Emit::Svg => Ok(svg.as_bytes().to_vec()),
        Emit::Png => svg_to_png(svg),
        Emit::Jpeg => svg_to_jpeg(svg, 92),
        Emit::Pdf => svg_to_pdf(svg),
    }
}

/// Without the `raster` feature only SVG pass-through is available.
#[cfg(not(feature = "raster"))]
pub fn render(svg: &str, emit: Emit) -> Result<Vec<u8>, String> {
    match emit {
        Emit::Svg => Ok(svg.as_bytes().to_vec()),
        other => Err(format!(
            "--emit {} needs in-core raster, which this build lacks; \
             rebuild `pr` with --features raster (default-on) or emit svg",
            other.ext()
        )),
    }
}

#[cfg(feature = "raster")]
fn pixmap(svg: &str) -> Result<resvg::tiny_skia::Pixmap, String> {
    use resvg::{tiny_skia, usvg};
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_str(svg, &opt).map_err(|e| format!("parse svg: {e}"))?;
    let size = tree.size().to_int_size();
    // The label renderer emits device-px (`--size 64px`) or mm
    // dimensions; we raster at the SVG's intrinsic px size (1:1 for
    // px-true labels, exact module fidelity for QR scannability).
    let mut pixmap = tiny_skia::Pixmap::new(size.width().max(1), size.height().max(1))
        .ok_or_else(|| "allocate pixmap".to_string())?;
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    Ok(pixmap)
}

#[cfg(feature = "raster")]
fn svg_to_png(svg: &str) -> Result<Vec<u8>, String> {
    pixmap(svg)?
        .encode_png()
        .map_err(|e| format!("encode png: {e}"))
}

#[cfg(feature = "raster")]
fn svg_to_jpeg(svg: &str, quality: u8) -> Result<Vec<u8>, String> {
    use image::{codecs::jpeg::JpegEncoder, ExtendedColorType, ImageEncoder};
    let pm = pixmap(svg)?;
    let (w, h) = (pm.width(), pm.height());
    // tiny-skia stores premultiplied RGBA; JPEG has no alpha, so
    // composite over white: out = premul_c + (1 - a) * 255, where the
    // stored channel already equals c*a and (255 - a) == (1 - a)*255.
    let data = pm.data();
    let mut rgb = Vec::with_capacity((w as usize) * (h as usize) * 3);
    for px in data.chunks_exact(4) {
        let inv = 255u16 - px[3] as u16;
        rgb.push((px[0] as u16 + inv).min(255) as u8);
        rgb.push((px[1] as u16 + inv).min(255) as u8);
        rgb.push((px[2] as u16 + inv).min(255) as u8);
    }
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, quality)
        .write_image(&rgb, w, h, ExtendedColorType::Rgb8)
        .map_err(|e| format!("encode jpeg: {e}"))?;
    Ok(out)
}

#[cfg(feature = "raster")]
fn svg_to_pdf(svg: &str) -> Result<Vec<u8>, String> {
    let opt = svg2pdf::usvg::Options::default();
    let tree = svg2pdf::usvg::Tree::from_str(svg, &opt).map_err(|e| format!("parse svg: {e}"))?;
    svg2pdf::to_pdf(
        &tree,
        svg2pdf::ConversionOptions::default(),
        svg2pdf::PageOptions::default(),
    )
    .map_err(|e| format!("convert pdf: {e}"))
}

#[cfg(all(test, feature = "raster"))]
mod tests {
    use super::*;

    // A 1-rect SVG is enough to exercise each backend end to end.
    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8" fill="#000"/></svg>"##;

    #[test]
    fn png_has_signature() {
        let out = render(SVG, Emit::Png).unwrap();
        assert_eq!(&out[..8], b"\x89PNG\r\n\x1a\n", "PNG magic");
    }

    #[test]
    fn jpeg_has_soi_marker() {
        let out = render(SVG, Emit::Jpeg).unwrap();
        assert_eq!(&out[..2], &[0xFF, 0xD8], "JPEG SOI");
    }

    #[test]
    fn pdf_has_header() {
        let out = render(SVG, Emit::Pdf).unwrap();
        assert_eq!(&out[..5], b"%PDF-", "PDF header");
    }

    #[test]
    fn svg_passes_through() {
        let out = render(SVG, Emit::Svg).unwrap();
        assert_eq!(out, SVG.as_bytes());
    }
}
