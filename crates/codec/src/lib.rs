//! `part-registry-codec` — QR encoder, decoder, and SVG label rendering.
//!
//! Strangler-fig step 1 per ADR-017 §"Strangler-fig migration sequence".
//! Wraps the `qrcode` 0.14 crate (placeholder for the eventual
//! `qrcode-rust2` swap — see ADR-017 §References) for encode and
//! `rxing` 0.9 for decode. SVG output is functionally equivalent to
//! the Python reference (`label.py`); the QR bit-matrix differs by a
//! one-time mask-selection diff (accepted in ADR-017 §Consequences).
//!
//! ## Module layout
//! - [`qr`] — encode + decode primitives (Standard QR V1 / Micro QR M4)
//! - [`format`] — `TextFormat` enum and the recommend/check helpers
//!   ported from `label.py:168-206`
//! - [`svg`] — mm-native SVG label rendering primitives
//!   (`label.py:97-251`)
//! - [`px`] — px-true device-pixel renderer + job-uniformity pass
//!   (ADR-031 §2–§4; obligation `px-true-qr-render`)
//! - [`glyphs`] — first-party 5×7 bitmap glyph table for the px-true
//!   id-text (ADR-031 §8 "glyphs ARE modules")
//! - [`symbology`] — the `<family>[-<version>][-<ec>]` type grammar +
//!   auto-fit resolution (ADR-031 §8 print contracts)
//!
//! ## wasm32
//! The decoder compiles to `wasm32-unknown-unknown` against rxing 0.9
//! (verified 2026-05-11). The codec keeps the decoder unconditionally
//! available so callers do not branch on `target_arch`. If a future
//! rxing release regresses on wasm32, gate [`qr::decode_qr`] behind
//! `#[cfg(not(target_arch = "wasm32"))]` rather than vendoring around
//! it — the wasm crate (`crates/wasm/`) does not call the decoder
//! today (FE keeps `zxing-wasm` until the ADR-017 step 8 A/B passes),
//! so dropping it from the wasm bundle is a free win when it becomes
//! necessary.

#![forbid(unsafe_code)]

use thiserror::Error;

pub mod format;
pub mod glyphs;
pub mod px;
pub mod qr;
pub mod svg;
pub mod symbology;

pub use format::{check_format_warning, recommend_format, TextFormat};
pub use px::{fill_to_max, render_label_px, Padding, PaddingMode, PxLabel};
#[cfg(feature = "decoder")]
pub use qr::decode_qr;
pub use qr::{encode, encode_pinned, QrMatrix};
pub use svg::{render, render_flag, render_horz, render_vert, Layout};
pub use symbology::{Ec, Family, ResolvedSymbology, Symbology};

/// Errors surfaced by the codec.
///
/// The variants mirror the three phases of the pipeline (encode →
/// render → decode) so callers can branch on which step failed
/// without parsing message strings. `Unsupported` marks requests the
/// codec recognizes but does not implement yet (e.g. the px-true flag
/// layout, ADR-031 §5) so callers can map it to their own
/// "unsupported" taxonomy rather than treating it as a failure.
#[derive(Debug, Error)]
pub enum CodecError {
    #[error("encode failed: {0}")]
    Encode(String),
    #[error("decode failed: {0}")]
    Decode(String),
    #[error("render failed: {0}")]
    Render(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
}

/// Render an SVG label, dispatching by layout. Mirrors `label.py`'s
/// `render()` signature so callers porting from Python find the same
/// surface.
///
/// Returns `Result<String, CodecError>` for parity with the rest of
/// the crate's API; the underlying renderer is infallible today
/// because all geometry math is bounded by the layout enum and
/// `f64` inputs, but keeping a `Result` here means a future renderer
/// that *can* fail (e.g. one that validates inputs) does not require
/// a breaking signature change.
pub fn render_label(
    canonical: &str,
    layout: Layout,
    size_mm: f64,
    fmt: TextFormat,
    micro: bool,
) -> Result<String, CodecError> {
    Ok(render(canonical, layout, size_mm, fmt, micro))
}
