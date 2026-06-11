//! Embedded 5×7 bitmap glyphs — typography on the module lattice
//! (ADR-031 §8 "glyphs ARE modules").
//!
//! This is a **first-party, hand-authored** table of classic 5×7
//! dot-matrix letterforms covering exactly the nano14 alphabet
//! ([`ALPHABET`], 31 chars — `0/O/1/I/L` are excluded at the id-scheme
//! level, ADR-012). Owning the bits means the remaining lookalike
//! pairs (`8/B`, `5/S`, `2/Z`, `6/G`) stay hand-tunable for this exact
//! alphabet; there is no third-party font dependency and nothing for a
//! rasterizer to substitute. The table is the hardware-validated
//! ADR-031 §8 prototype (printed + verified 2026-06-11),
//! transliterated bit-for-bit.
//!
//! Glyphs render through the same `<rect>` emitter as the QR modules
//! ([`write_glyph_rects`]), so the whole label is one deterministic
//! binary raster — identical across rsvg/browser/printer/wasm.

use std::fmt::Write as _;

use crate::CodecError;

/// Glyph cell height in dots.
pub const GLYPH_ROWS: u32 = 7;
/// Glyph cell width in dots.
pub const GLYPH_COLS: u32 = 5;
/// The nano14 id alphabet this table covers (ADR-012: no `0/O/1/I/L`).
pub const ALPHABET: &str = "23456789ABCDEFGHJKMNPQRSTUVWXYZ";

/// The 7 rows of `c`'s 5×7 bitmap, top to bottom; bit 4 is the
/// leftmost column (the literals read like the glyph). `None` for any
/// char outside [`ALPHABET`].
pub fn glyph(c: char) -> Option<&'static [u8; GLYPH_ROWS as usize]> {
    let rows: &'static [u8; GLYPH_ROWS as usize] = match c {
        '2' => &[
            0b01110, 0b10001, 0b00001, 0b00110, 0b01000, 0b10000, 0b11111,
        ],
        '3' => &[
            0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110,
        ],
        '4' => &[
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => &[
            0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
        ],
        '6' => &[
            0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => &[
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => &[
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => &[
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100,
        ],
        'A' => &[
            0b01110, 0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001,
        ],
        'B' => &[
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        'C' => &[
            0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110,
        ],
        'D' => &[
            0b11100, 0b10010, 0b10001, 0b10001, 0b10001, 0b10010, 0b11100,
        ],
        'E' => &[
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        'F' => &[
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'G' => &[
            0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111,
        ],
        'H' => &[
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'J' => &[
            0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100,
        ],
        'K' => &[
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'M' => &[
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
        'N' => &[
            0b10001, 0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001,
        ],
        'P' => &[
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'Q' => &[
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
        'R' => &[
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'S' => &[
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'T' => &[
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'U' => &[
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'V' => &[
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
        'W' => &[
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010,
        ],
        'X' => &[
            0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ],
        'Y' => &[
            0b10001, 0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100,
        ],
        'Z' => &[
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        _ => return None,
    };
    Some(rows)
}

/// Number of ink dots in `c`'s glyph — the typography side of a
/// rect-count ledger. `None` for any char outside [`ALPHABET`].
pub fn ink_bits(c: char) -> Option<u32> {
    glyph(c).map(|rows| rows.iter().map(|r| r.count_ones()).sum())
}

/// Append the ink rects of `c` to `out`, cell top-left at `(x0, y0)`,
/// `g` device px per glyph dot — the same `<rect>` emitter shape the
/// QR modules use, so glyphs and modules share one fill group and one
/// integer-px lattice.
///
/// Errors with [`CodecError::Render`] for a char outside [`ALPHABET`]
/// (defensive: nano14 payloads cannot contain one).
pub fn write_glyph_rects(
    out: &mut String,
    c: char,
    x0: u32,
    y0: u32,
    g: u32,
) -> Result<(), CodecError> {
    let rows = glyph(c).ok_or_else(|| {
        CodecError::Render(format!(
            "no bitmap glyph for {c:?}: the embedded 5x7 table covers \
             the nano14 alphabet {ALPHABET}"
        ))
    })?;
    for (ry, row) in rows.iter().enumerate() {
        for rx in 0..GLYPH_COLS {
            if (row >> (GLYPH_COLS - 1 - rx)) & 1 == 1 {
                let x = x0 + rx * g;
                let y = y0 + ry as u32 * g;
                let _ = write!(
                    out,
                    "<rect x=\"{x}\" y=\"{y}\" width=\"{g}\" height=\"{g}\"/>"
                );
            }
        }
    }
    Ok(())
}

// ---------- tests ----------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_covers_exactly_the_nano14_alphabet() {
        assert_eq!(ALPHABET.chars().count(), 31);
        for c in ALPHABET.chars() {
            assert!(glyph(c).is_some(), "glyph for {c}");
        }
        for c in ['0', 'O', '1', 'I', 'L', ' ', '-', 'a'] {
            assert!(glyph(c).is_none(), "{c} is outside the alphabet");
        }
    }

    #[test]
    fn rows_fit_five_columns_and_match_the_validated_checksum() {
        // 472 total ink dots — computed from the hardware-validated
        // prototype table, so a transliteration slip cannot pass.
        let mut total = 0;
        for c in ALPHABET.chars() {
            for row in glyph(c).expect("in alphabet") {
                assert!(*row < 1 << GLYPH_COLS, "{c} row {row:#07b} fits 5 cols");
            }
            total += ink_bits(c).expect("in alphabet");
        }
        assert_eq!(total, 472, "table checksum");
        // Spot anchors on the tunable lookalike pairs.
        assert_eq!(ink_bits('8'), Some(17));
        assert_eq!(ink_bits('B'), Some(20));
        assert_eq!(ink_bits('5'), Some(17));
        assert_eq!(ink_bits('S'), Some(15));
    }

    #[test]
    fn rects_land_on_the_g_lattice() {
        // T: row 0 fully inked, then the center column — 11 dots.
        let mut out = String::new();
        write_glyph_rects(&mut out, 'T', 10, 20, 3).expect("T renders");
        assert_eq!(out.matches("<rect").count(), 11);
        assert!(out.starts_with("<rect x=\"10\" y=\"20\" width=\"3\" height=\"3\"/>"));
        // Center column, bottom row: x = 10 + 2·3, y = 20 + 6·3.
        assert!(out.ends_with("<rect x=\"16\" y=\"38\" width=\"3\" height=\"3\"/>"));
    }

    #[test]
    fn unknown_char_names_the_char_and_the_alphabet() {
        let mut out = String::new();
        let err = write_glyph_rects(&mut out, 'O', 0, 0, 1).expect_err("O has no glyph");
        let msg = err.to_string();
        assert!(matches!(err, CodecError::Render(_)));
        assert!(msg.contains("'O'"), "char named: {msg}");
        assert!(msg.contains(ALPHABET), "alphabet listed: {msg}");
        assert!(out.is_empty(), "nothing emitted on error");
    }
}
