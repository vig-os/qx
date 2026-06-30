//! SOUP harness H2 — Micro QR roundtrip + matrix-fingerprint golden
//! hash per IEC 62304.
//!
//! Validates the `qrcode` (encoder) and `rxing` (decoder) SOUP
//! dependencies by:
//!
//! 1. Encoding a fixed corpus of canonical IDs in both Standard QR and
//!    Micro QR M4, rasterising the matrix to PNG, decoding with rxing,
//!    and asserting the decoded text matches the input.
//!
//! 2. Hashing the SVG label output (SHA-256) for each corpus entry and
//!    comparing against committed golden hashes. A mismatch signals a
//!    rendering regression (e.g. a SOUP upgrade changed the QR mask
//!    selection or the SVG geometry).

use image::{DynamicImage, ImageBuffer, Luma};
use sha2::{Digest, Sha256};

use qx_codec::qr::{encode, QrMatrix};
use qx_codec::svg::Layout;
use qx_codec::{decode_qr, encode_pinned, render, Ec, Family, TextFormat};

/// Fixed corpus of 5 canonical IDs drawn from the ADR-012 alphabet.
const CORPUS: [&str; 5] = [
    "K7M3PQ9RT5VAXY",
    "23456789ABCDEF",
    "GHJKMNPQRSTUVW",
    "XY23456789ABCD",
    "EFGHJKMNPQRSTU",
];

// ------------------------------------------------------------------
// Helpers
// ------------------------------------------------------------------

/// Rasterise a `QrMatrix` to a PNG byte buffer at the given per-module
/// pixel size. Identical to the helper in `crates/codec/src/svg.rs::tests`.
fn rasterise_matrix(matrix: &QrMatrix, module_px: u32) -> Vec<u8> {
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

/// SHA-256 hex digest of a byte slice.
fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

// ------------------------------------------------------------------
// 1. QR roundtrip — encode, rasterise, decode, assert match
// ------------------------------------------------------------------

#[test]
fn standard_qr_roundtrip_corpus() {
    for id in &CORPUS {
        let matrix = encode(id, false).expect("standard QR encode succeeds");
        let png = rasterise_matrix(&matrix, 12);
        let decoded = decode_qr(&png)
            .unwrap_or_else(|e| panic!("rxing failed to decode standard QR for {id}: {e}"));
        assert_eq!(&decoded, id, "standard QR roundtrip mismatch for {id}");
    }
}

#[test]
fn micro_qr_roundtrip_corpus() {
    for id in &CORPUS {
        let matrix = encode(id, true).expect("micro QR M4 encode succeeds");
        let png = rasterise_matrix(&matrix, 16);
        let decoded = decode_qr(&png)
            .unwrap_or_else(|e| panic!("rxing failed to decode micro QR for {id}: {e}"));
        assert_eq!(&decoded, id, "micro QR M4 roundtrip mismatch for {id}");
    }
}

/// Issue #211 regression guard: **every** 14-char nano14 id must encode in
/// Micro QR **M3-L** (the compact label symbology) and round-trip through
/// rxing. The deterministic 5000-id sweep catches the suboptimal-
/// segmentation class (false `DataTooLong`) that the qrcode2 Annex-J fork
/// fixes; a 1-in-25 sample is decoded to also catch matrix-correctness
/// regressions. If this fails after a qrcode2 bump, the segmentation fix
/// regressed or the `[patch.crates-io]` pin was dropped before upstreaming.
#[test]
fn micro_m3l_holds_and_roundtrips_nano14_corpus() {
    const ALPHABET: &[u8] = b"23456789ABCDEFGHJKMNPQRSTUVWXYZ"; // nano14, 31 syms
    let mut state: u64 = 0x9E3779B97F4A7C15;
    let mut next = || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (state >> 33) as usize
    };
    for i in 0..5000 {
        let id: String = (0..14)
            .map(|_| ALPHABET[next() % ALPHABET.len()] as char)
            .collect();
        let matrix = encode_pinned(&id, Family::Micro, 3, Ec::L)
            .unwrap_or_else(|e| panic!("micro-m3-l encode failed for nano14 id {id}: {e}"));
        assert_eq!(matrix.size, 15, "M3 is 15 data modules");
        if i % 25 == 0 {
            let png = rasterise_matrix(&matrix, 12);
            let decoded = decode_qr(&png)
                .unwrap_or_else(|e| panic!("rxing failed to decode micro-m3-l for {id}: {e}"));
            assert_eq!(decoded, id, "micro-m3-l roundtrip mismatch for {id}");
        }
    }
}

// ------------------------------------------------------------------
// 2. Golden hash — SHA-256 of SVG output for regression detection
// ------------------------------------------------------------------

/// Golden hashes for the standard-QR vertical-layout SVG at 11mm,
/// TextFormat::FourFourFour. One entry per corpus ID.
///
/// Regenerate by running the test with `SOUP_H2_PRINT_GOLDEN=1` and
/// committing the printed hashes.
const GOLDEN_STANDARD: [&str; 5] = [
    "3cacc4b9224fb637db9fa7c36c329589248ffb328c97daf1f262e962792ff50d",
    "4f2f7941367c1ad3ba5a2a2d55bc9c22ac89ff92fcf216f97399588a777fee4e",
    "205426ec8abd8f561cb18bae1bcd5dd8c3b72fb9082ed62262fb7b0bfdd4f915",
    // XY23456789ABCD shifted with the qrcode2 Annex-J fork (#211): the
    // `23456789` run now encodes as a numeric segment (fewer bits) -> a
    // different, valid module matrix.
    "705de3be3ae3f1924acaa27cc2723263a9c72df4af79e005b2146a1455a7671c",
    "35cdca4445621946e66df57a22c0f507b667d03b35746b3fa6acb38ff66bd72e",
];

/// Golden hashes for the micro-QR vertical-layout SVG at 11mm,
/// TextFormat::FourFourFour.
const GOLDEN_MICRO: [&str; 5] = [
    "e42643baeb48b90052b5fbbb09941b3ee6db28bc21a07f61efb295b877e7c2e6",
    "ae4edbfa90ef2cee98d82ce44861b586121c4141502d65446b65867b92e84cf1",
    "9ca010942f2e281922af0a1969656303dafec5bd55b0e4792b7adf243e99de5f",
    "46cc46744eaf5df038ee4569374911008b90797114edcd9331dad6bce7c1467b",
    "7a533b9085767ee43be0d870200a34f688b162a3b4ff84d8a473500949fdf11e",
];

#[test]
fn golden_hash_standard_qr_svg() {
    let print_golden = std::env::var("SOUP_H2_PRINT_GOLDEN").is_ok();

    for (i, id) in CORPUS.iter().enumerate() {
        let svg = render(id, Layout::Vert, 11.0, TextFormat::FourFourFour, false);
        let hash = sha256_hex(svg.as_bytes());

        if print_golden {
            eprintln!("GOLDEN_STANDARD[{i}] ({id}): \"{hash}\",");
            continue;
        }

        assert_eq!(
            hash, GOLDEN_STANDARD[i],
            "golden hash regression for standard QR SVG of corpus[{i}] = {id}\n\
             expected: {}\n\
             got:      {hash}\n\
             If this is intentional (SOUP upgrade), re-run with \
             SOUP_H2_PRINT_GOLDEN=1 and update the golden array.",
            GOLDEN_STANDARD[i],
        );
    }
}

#[test]
fn golden_hash_micro_qr_svg() {
    let print_golden = std::env::var("SOUP_H2_PRINT_GOLDEN").is_ok();

    for (i, id) in CORPUS.iter().enumerate() {
        let svg = render(id, Layout::Vert, 11.0, TextFormat::FourFourFour, true);
        let hash = sha256_hex(svg.as_bytes());

        if print_golden {
            eprintln!("GOLDEN_MICRO[{i}] ({id}): \"{hash}\",");
            continue;
        }

        assert_eq!(
            hash, GOLDEN_MICRO[i],
            "golden hash regression for micro QR SVG of corpus[{i}] = {id}\n\
             expected: {}\n\
             got:      {hash}\n\
             If this is intentional (SOUP upgrade), re-run with \
             SOUP_H2_PRINT_GOLDEN=1 and update the golden array.",
            GOLDEN_MICRO[i],
        );
    }
}
