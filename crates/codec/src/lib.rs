//! `part-registry-codec` — QR encoder, decoder, and SVG label rendering.
//!
//! Foundation scaffold per ADR-017 §"Workspace shape". Wraps
//! `qrcode-rust2` for encode (dormant placeholder `qrcode` 0.14 today
//! per the root Cargo.toml note) and `rxing` for decode. Production
//! logic is intentionally absent at scaffold time — see
//! ADR-017 §"Strangler-fig migration sequence" step 1.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CodecError {
    #[error("encode failed: {0}")]
    Encode(String),
    #[error("decode failed: {0}")]
    Decode(String),
    #[error("render failed: {0}")]
    Render(String),
}

/// Output of the QR encoder. Concrete bit-matrix representation is
/// deferred until the encoder swap (ADR-017 step 1) so the consumer
/// API can settle without committing to a private upstream type.
#[derive(Clone, Debug)]
pub struct QrMatrix {
    pub size: u32,
    pub modules: Vec<bool>,
}

/// Layout per `label.py:74-78`. Covered by ADR-021 §"Migration audit"
/// — currently fixed constants in this crate, candidate for config
/// later.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    Vert,
    Horz,
    Flag,
}

/// Text-row split per ADR-012 ID scheme. MVP fixed; configurability
/// deferred per ADR-021 §"Migration audit".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextFormat {
    FourFour,
    FourFourFour,
    FiveFiveFour,
}

/// Encode a payload as a Standard or Micro QR matrix.
pub fn encode(_payload: &str, _micro: bool) -> Result<QrMatrix, CodecError> {
    unimplemented!("foundation scaffold; ADR-017 step 1")
}

/// Render an SVG label for the canonical part ID.
pub fn render_label(
    _canonical: &str,
    _layout: Layout,
    _size_mm: f64,
    _format: TextFormat,
) -> Result<String, CodecError> {
    unimplemented!("foundation scaffold; ADR-017 step 1")
}

/// Decode a QR or DataMatrix image, returning the embedded payload.
pub fn decode(_image: &[u8]) -> Result<String, CodecError> {
    unimplemented!("foundation scaffold; ADR-017 step 1")
}
