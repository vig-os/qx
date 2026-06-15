//! QR encode + decode.
//!
//! Encoder: `qrcode2` (sorairolake fork of `qrcode`; ADR-017/ADR-028
//! swap — maintained, on crates.io, dual MIT/Apache, adds Micro QR M3
//! + rMQR; resolves the `qrcode` 0.14 dormancy). NOTE: like its
//! ancestor it does NOT truncate the Micro-QR terminator, so M3-L
//! rejects ~3% of 14-char ids with poor digit structure (issue #211);
//! mitigated by the mint M3-L fence + `pr print --verify`. Version + EC
//! level are contract parameters (ADR-031
//! §8): [`encode_pinned`] takes them explicitly; [`encode`] keeps the
//! pre-contract defaults (Standard QR V1 / Micro QR M4, both EC M)
//! for the mm-native render path.
//!
//! Decoder: `rxing` 0.9 — supports both Standard QR and Micro QR.
//!
//! Quiet-zone constants match `label.py:67-70` (Standard: 4 modules,
//! Micro: 2 modules — both spec minima).

use qrcode2::types::{Color, EcLevel, Version};
use qrcode2::QrCode;

use crate::symbology::{Ec, Family};
use crate::CodecError;

/// Quiet-zone width in modules for Standard QR. ISO/IEC 18004 §6.3.8.
pub const QR_BORDER_STANDARD: usize = 4;
/// Quiet-zone width in modules for Micro QR. ISO/IEC 18004 §6.3.8.
pub const QR_BORDER_MICRO: usize = 2;

/// A square QR module matrix. `modules[r * size + c]` is `true` for
/// dark modules, `false` for light. Quiet-zone modules are *not*
/// included in `modules`; callers add the quiet zone at render time
/// based on [`is_micro`](Self::is_micro).
#[derive(Clone, Debug)]
pub struct QrMatrix {
    /// Side length in modules (excluding quiet zone).
    pub size: usize,
    /// Row-major module bits, length = `size * size`.
    pub modules: Vec<bool>,
    /// True for Micro QR, false for Standard QR. Used by renderers to
    /// pick the correct quiet-zone width.
    pub micro: bool,
}

impl QrMatrix {
    /// Quiet-zone width (in modules) appropriate for this matrix.
    pub fn quiet_zone(&self) -> usize {
        if self.micro {
            QR_BORDER_MICRO
        } else {
            QR_BORDER_STANDARD
        }
    }

    /// Total side length including the quiet zone (in modules).
    pub fn total_modules(&self) -> usize {
        self.size + 2 * self.quiet_zone()
    }

    /// Returns the module at `(row, col)` (0-indexed, no quiet zone).
    pub fn get(&self, row: usize, col: usize) -> bool {
        self.modules[row * self.size + col]
    }

    /// View `modules` as nested `Vec<Vec<bool>>` for callers that
    /// prefer a 2D shape. The flat row-major representation is the
    /// internal canonical form; this is a convenience.
    pub fn as_rows(&self) -> Vec<Vec<bool>> {
        (0..self.size)
            .map(|r| (0..self.size).map(|c| self.get(r, c)).collect())
            .collect()
    }
}

/// Encode a payload as a Standard QR (V1, EC M) or Micro QR (M4, EC M)
/// matrix — the pre-contract defaults, kept for the mm-native render
/// path. The px-true contract path resolves a [`Family`]/version/EC
/// triple and calls [`encode_pinned`].
///
/// Returns the bare module grid; callers (the SVG renderer, the PNG
/// rasteriser used in tests) handle the quiet zone.
pub fn encode(payload: &str, micro: bool) -> Result<QrMatrix, CodecError> {
    if micro {
        encode_pinned(payload, Family::Micro, 4, Ec::M)
    } else {
        encode_pinned(payload, Family::Qr, 1, Ec::M)
    }
}

/// Encode at a pinned (family, version, EC) — version + EC are
/// contract parameters, not hardcodes (ADR-031 §8). Feasibility is the
/// encoder's verdict: an oversized payload comes back as
/// [`CodecError::Encode`] (which [`crate::Symbology::resolve`] maps to
/// the feasibility hint).
pub fn encode_pinned(
    payload: &str,
    family: Family,
    version: u8,
    ec: Ec,
) -> Result<QrMatrix, CodecError> {
    let version = match family {
        Family::Micro => Version::Micro(i16::from(version)),
        Family::Qr => Version::Normal(i16::from(version)),
    };
    let level = match ec {
        Ec::L => EcLevel::L,
        Ec::M => EcLevel::M,
        Ec::Q => EcLevel::Q,
        Ec::H => EcLevel::H,
    };
    let micro = family == Family::Micro;
    let code = QrCode::with_version(payload.as_bytes(), version, level)
        .map_err(|e| CodecError::Encode(format!("{e:?}")))?;
    let size = code.width();
    let colors = code.into_colors();
    // qrcode2's Color is `Light` or `Dark`; map to `bool` so consumers
    // do not have to pull in the `qrcode2` types.
    let modules: Vec<bool> = colors.into_iter().map(|c| c != Color::Light).collect();
    Ok(QrMatrix {
        size,
        modules,
        micro,
    })
}

/// Decode a PNG image into the embedded QR payload.
///
/// Tries Standard QR first, then Micro QR — `rxing`'s
/// `MultiFormatReader` defaults pick the first decoder that succeeds.
/// The function deliberately takes PNG bytes (not a pre-decoded
/// luma buffer) so callers do not have to pull in the `image` crate
/// to use the decoder.
///
/// Gated behind the `decoder` feature (default-on). Disable via
/// `default-features = false` to drop the ~1.4 MB rxing dependency
/// from the build — see foundation issue #33 / `crates/wasm/Cargo.toml`
/// for the size-sensitive consumer that uses this opt-out.
#[cfg(feature = "decoder")]
pub fn decode_qr(image_png: &[u8]) -> Result<String, CodecError> {
    use rxing::{BarcodeFormat, DecodeHints};
    let mut hints = DecodeHints {
        PossibleFormats: Some(
            [
                BarcodeFormat::QR_CODE,
                BarcodeFormat::MICRO_QR_CODE,
                BarcodeFormat::DATA_MATRIX,
            ]
            .into_iter()
            .collect(),
        ),
        TryHarder: Some(true),
        ..DecodeHints::default()
    };
    let result = rxing::helpers::detect_in_buffer_with_hints(image_png, None, &mut hints)
        .map_err(|e| CodecError::Decode(format!("rxing: {e:?}")))?;
    Ok(result.getText().to_string())
}
