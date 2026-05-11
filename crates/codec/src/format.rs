//! Text-format enum + recommendation helpers.
//!
//! Ported from `label.py:74-206`. The three formats mirror the
//! Python `FORMATS` dict; `recommend_format` and `check_format_warning`
//! reproduce the legibility-tier logic verbatim so the Rust CLI emits
//! the same warning text the Python CLI does today.

use serde::{Deserialize, Serialize};

/// Text-row split per ADR-012 ID scheme. Corresponds to the Python
/// `FORMATS` dict keys (`"4/4"`, `"4/4/4"`, `"5/5/4"`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextFormat {
    /// 8 chars in 2 rows (4 + 4). Recommended for sizes < 10 mm.
    FourFour,
    /// 12 chars in 3 rows (4 + 4 + 4). Recommended for sizes >= 10 mm.
    FourFourFour,
    /// 14 chars in 3 rows (5 + 5 + 4). Shows the full canonical ID.
    FiveFiveFour,
}

impl TextFormat {
    /// Characters per row for this format, in document order.
    pub fn chars_per_row(self) -> &'static [usize] {
        match self {
            TextFormat::FourFour => &[4, 4],
            TextFormat::FourFourFour => &[4, 4, 4],
            TextFormat::FiveFiveFour => &[5, 5, 4],
        }
    }

    /// Number of text rows.
    pub fn n_rows(self) -> usize {
        self.chars_per_row().len()
    }

    /// Human-readable name matching `label.py`'s `--format` CLI flag.
    pub fn as_str(self) -> &'static str {
        match self {
            TextFormat::FourFour => "4/4",
            TextFormat::FourFourFour => "4/4/4",
            TextFormat::FiveFiveFour => "5/5/4",
        }
    }

    /// Split a canonical ID into rows. Mirrors `label.py:split_format`.
    ///
    /// If `canonical` is shorter than the format demands, the missing
    /// chars are simply absent from the trailing row(s) — matching
    /// Python's slice semantics. Callers should pass full-length IDs
    /// in production; the slack here exists so the text-prefix
    /// invariant (`canonical.startswith(displayed_text)`) holds for
    /// every format/length combination the test suite covers.
    pub fn split(self, canonical: &str) -> Vec<String> {
        let chars: Vec<char> = canonical.chars().collect();
        let mut rows = Vec::with_capacity(self.n_rows());
        let mut idx = 0;
        for &n in self.chars_per_row() {
            let end = (idx + n).min(chars.len());
            rows.push(chars[idx..end].iter().collect());
            idx = end;
        }
        rows
    }
}

/// Recommended format for a label of `size_mm` mm. Mirrors
/// `label.py:recommend_format` (lines 168-186).
///
/// Returns `(format, warning)`. The warning is `Some(_)` only at very
/// small sizes (< 5 mm) where even the most compact format still
/// produces sub-1.5 mm glyphs (below the "readable" legibility tier
/// per ADR-012).
pub fn recommend_format(size_mm: f64) -> (TextFormat, Option<String>) {
    if size_mm < 8.0 {
        let warn = if size_mm < 5.0 {
            Some(
                "size < 5mm: even 4/4 font < 1.5mm (below 'readable'). \
                 Consider a larger label."
                    .to_string(),
            )
        } else {
            None
        };
        return (TextFormat::FourFour, warn);
    }
    if size_mm < 10.0 {
        return (TextFormat::FourFour, None);
    }
    (TextFormat::FourFourFour, None)
}

/// Warning string if `fmt` is sub-optimal for `size_mm`. Mirrors
/// `label.py:check_format_warning` (lines 189-206).
///
/// Returns `None` when the choice is reasonable for the size tier.
pub fn check_format_warning(size_mm: f64, fmt: TextFormat) -> Option<String> {
    if size_mm < 5.0 && fmt != TextFormat::FourFour {
        return Some(format!(
            "format {fmt} at {size_mm}mm: font < 1.3mm (below 'readable'). \
             Use --format 4/4 for this size.",
            fmt = fmt.as_str(),
            size_mm = format_size(size_mm),
        ));
    }
    if (5.0..8.0).contains(&size_mm) && fmt != TextFormat::FourFour {
        return Some(format!(
            "format {fmt} at {size_mm}mm: font < 1.9mm (below 'comfortable'). \
             Consider --format 4/4.",
            fmt = fmt.as_str(),
            size_mm = format_size(size_mm),
        ));
    }
    if size_mm >= 10.0 && fmt == TextFormat::FourFour {
        return Some(format!(
            "format 4/4 at {size_mm}mm: font > 4mm (overkill, wastes space). \
             Consider --format 4/4/4 or 5/5/4.",
            size_mm = format_size(size_mm),
        ));
    }
    None
}

/// Match Python's `f"{size_mm}"` formatting for warning strings —
/// integer-valued floats print without a trailing `.0` so the warning
/// text is identical to `label.py`'s output.
fn format_size(size_mm: f64) -> String {
    if size_mm.fract() == 0.0 {
        format!("{:.1}", size_mm)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
            .parse::<i64>()
            .map(|i| i.to_string())
            .unwrap_or_else(|_| format!("{size_mm}"))
    } else {
        format!("{size_mm}")
    }
}
