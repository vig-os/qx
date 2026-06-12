//! Id-set solver: fix-two-derive-one over the id-text block's three
//! knobs (ADR-031 §10, 2026-06-12).
//!
//! Three knobs name the id-text geometry inside a per-layout text
//! budget (horz: module-part height; vert: module-part width; id-only
//! payload: whole canvas along the layout axis):
//!
//! - `chars` — how many id characters to show (the `--id-chars` knob,
//!   with nano14's natural slots at 8 and 14).
//! - `rows` — how the chars are split across rows (balanced split, so
//!   14 chars / 3 rows -> `[5,5,4]`).
//! - `g` — glyph scale in device px (the nx75 7-row anchor cell at
//!   integer scale `k = g`).
//!
//! The block geometry on the budget axis is:
//!
//! ```text
//! budget = rows·7g + (rows-1)·g = (8·rows - 1)·g
//! ```
//!
//! The solver lets the operator fix any TWO and DERIVES the third
//! (§10: "given the per-layout text budget …, fix any two of
//! {chars-arrangement, rows, glyph-size} -> derive the third"). All
//! three given -> VALIDATE (must satisfy the budget); fewer than two
//! given -> AUTO (maximize g, then minimize rows).
//!
//! Infeasible -> a Validation-grade error that quotes the NEAREST
//! FEASIBLE TRIPLE (§10 example: "rows 2 @ 28px needs 96px; have 60 —
//! feasible: rows 2 @ 18px or rows 3 @ 28px").

use serde::{Deserialize, Serialize};

/// What the solver produces (and what the engine + receipt agree on).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdBlock {
    /// How many id characters render.
    pub chars: u32,
    /// How many rows the chars split across (balanced).
    pub rows: u32,
    /// Glyph scale `g` in device px (nx75 7-row anchor cell at
    /// integer `k`).
    pub glyph_px: u32,
}

impl IdBlock {
    /// Budget consumption: `(8·rows - 1)·g` along the layout axis.
    pub fn budget_px(&self) -> u32 {
        (8 * self.rows - 1) * self.glyph_px
    }

    /// Balanced grouping along the rows: 14 chars / 3 rows -> `[5,5,4]`.
    pub fn grouping(&self) -> Vec<u32> {
        if self.rows == 0 {
            return Vec::new();
        }
        let base = self.chars / self.rows;
        let extra = self.chars % self.rows;
        (0..self.rows)
            .map(|i| if i < extra { base + 1 } else { base })
            .collect()
    }

    /// Stringified balanced grouping (e.g. `"554"` for chars=14, rows=3).
    pub fn grouping_str(&self) -> String {
        self.grouping()
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .concat()
    }
}

/// Inputs to the solver (`None` = "let it derive me").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Inputs {
    /// Number of id characters to show.
    pub chars: Option<u32>,
    /// Number of rows.
    pub rows: Option<u32>,
    /// Glyph scale `g` in device px.
    pub glyph_px: Option<u32>,
    /// Cap on `g` — the canonical g-law cap `min(module_px, …)`.
    /// `None` means no cap (id-only payload covers the whole canvas).
    pub glyph_px_cap: Option<u32>,
    /// Maximum chars the underlying id supplies (typically 14 for
    /// nano14). The chars knob can never exceed this.
    pub chars_max: u32,
}

/// Solver errors carry the §10 nearest-feasible hint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SolverError {
    pub message: String,
}

impl std::fmt::Display for SolverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SolverError {}

/// Solve the id block against a fixed text budget (device px along the
/// layout axis). Returns the resolved [`IdBlock`] or a Validation
/// error quoting the nearest feasible triple.
pub fn solve(inputs: Inputs, budget_px: u32) -> Result<IdBlock, SolverError> {
    if inputs.chars_max == 0 {
        return Err(SolverError {
            message: "id solver: chars_max must be >= 1".into(),
        });
    }
    // Tier 1: all three given -> validate.
    if let (Some(c), Some(r), Some(g)) = (inputs.chars, inputs.rows, inputs.glyph_px) {
        return validate_triple(c, r, g, budget_px, &inputs);
    }
    // Tier 2: two given -> derive the third.
    if let (Some(r), Some(g)) = (inputs.rows, inputs.glyph_px) {
        return derive_chars(r, g, budget_px, &inputs);
    }
    if let (Some(c), Some(g)) = (inputs.chars, inputs.glyph_px) {
        return derive_rows(c, g, budget_px, &inputs);
    }
    if let (Some(c), Some(r)) = (inputs.chars, inputs.rows) {
        return derive_glyph(c, r, budget_px, &inputs);
    }
    // Tier 3: one or zero given -> AUTO. Per §10: "maximize g, then
    // minimize rows" (smaller-row + bigger-glyph wins ties).
    auto(inputs, budget_px)
}

fn cap_g(g: u32, inputs: &Inputs) -> u32 {
    match inputs.glyph_px_cap {
        Some(c) => g.min(c),
        None => g,
    }
}

fn validate_triple(
    chars: u32,
    rows: u32,
    glyph_px: u32,
    budget_px: u32,
    inputs: &Inputs,
) -> Result<IdBlock, SolverError> {
    if chars == 0 || rows == 0 || glyph_px == 0 {
        return Err(infeasible(
            "all three of chars/rows/g must be >= 1",
            chars,
            rows,
            glyph_px,
            budget_px,
            inputs,
        ));
    }
    if chars > inputs.chars_max {
        return Err(infeasible(
            &format!(
                "id-chars {chars} exceeds available id length {}",
                inputs.chars_max
            ),
            chars,
            rows,
            glyph_px,
            budget_px,
            inputs,
        ));
    }
    if rows > chars {
        return Err(infeasible(
            &format!("rows {rows} exceeds chars {chars}"),
            chars,
            rows,
            glyph_px,
            budget_px,
            inputs,
        ));
    }
    if let Some(cap) = inputs.glyph_px_cap {
        if glyph_px > cap {
            return Err(infeasible(
                &format!("g {glyph_px}px exceeds the g-law cap {cap}px"),
                chars,
                rows,
                glyph_px,
                budget_px,
                inputs,
            ));
        }
    }
    let need = (8 * rows - 1) * glyph_px;
    if need > budget_px {
        return Err(infeasible(
            &format!("rows {rows} @ {glyph_px}px needs {need}px; have {budget_px}"),
            chars,
            rows,
            glyph_px,
            budget_px,
            inputs,
        ));
    }
    Ok(IdBlock {
        chars,
        rows,
        glyph_px,
    })
}

fn derive_chars(
    rows: u32,
    glyph_px: u32,
    budget_px: u32,
    inputs: &Inputs,
) -> Result<IdBlock, SolverError> {
    if rows == 0 || glyph_px == 0 {
        return Err(infeasible(
            "rows and g must be >= 1",
            inputs.chars.unwrap_or(0),
            rows,
            glyph_px,
            budget_px,
            inputs,
        ));
    }
    let need = (8 * rows - 1) * glyph_px;
    if need > budget_px {
        return Err(infeasible(
            &format!("rows {rows} @ {glyph_px}px needs {need}px; have {budget_px}"),
            inputs.chars.unwrap_or(inputs.chars_max),
            rows,
            glyph_px,
            budget_px,
            inputs,
        ));
    }
    if let Some(cap) = inputs.glyph_px_cap {
        if glyph_px > cap {
            return Err(infeasible(
                &format!("g {glyph_px}px exceeds the g-law cap {cap}px"),
                inputs.chars.unwrap_or(inputs.chars_max),
                rows,
                glyph_px,
                budget_px,
                inputs,
            ));
        }
    }
    // Chars derive: fill the rows with as many chars as the id supplies
    // (rows can hold any positive number; the chars knob caps at
    // chars_max).
    Ok(IdBlock {
        chars: inputs.chars_max,
        rows,
        glyph_px,
    })
}

fn derive_rows(
    chars: u32,
    glyph_px: u32,
    budget_px: u32,
    inputs: &Inputs,
) -> Result<IdBlock, SolverError> {
    if chars == 0 || glyph_px == 0 {
        return Err(infeasible(
            "chars and g must be >= 1",
            chars,
            inputs.rows.unwrap_or(0),
            glyph_px,
            budget_px,
            inputs,
        ));
    }
    if chars > inputs.chars_max {
        return Err(infeasible(
            &format!(
                "id-chars {chars} exceeds available id length {}",
                inputs.chars_max
            ),
            chars,
            inputs.rows.unwrap_or(0),
            glyph_px,
            budget_px,
            inputs,
        ));
    }
    if let Some(cap) = inputs.glyph_px_cap {
        if glyph_px > cap {
            return Err(infeasible(
                &format!("g {glyph_px}px exceeds the g-law cap {cap}px"),
                chars,
                inputs.rows.unwrap_or(0),
                glyph_px,
                budget_px,
                inputs,
            ));
        }
    }
    // Per §10 "maximize g, minimize rows" — chars is fixed, so we want
    // the SMALLEST `rows` that fits the block in the budget.
    for r in 1..=chars {
        let need = (8 * r - 1) * glyph_px;
        if need <= budget_px {
            return Ok(IdBlock {
                chars,
                rows: r,
                glyph_px,
            });
        }
    }
    Err(infeasible(
        &format!("chars {chars} @ {glyph_px}px does not fit budget {budget_px}px in any row count"),
        chars,
        1,
        glyph_px,
        budget_px,
        inputs,
    ))
}

fn derive_glyph(
    chars: u32,
    rows: u32,
    budget_px: u32,
    inputs: &Inputs,
) -> Result<IdBlock, SolverError> {
    if chars == 0 || rows == 0 {
        return Err(infeasible(
            "chars and rows must be >= 1",
            chars,
            rows,
            inputs.glyph_px.unwrap_or(0),
            budget_px,
            inputs,
        ));
    }
    if chars > inputs.chars_max {
        return Err(infeasible(
            &format!(
                "id-chars {chars} exceeds available id length {}",
                inputs.chars_max
            ),
            chars,
            rows,
            inputs.glyph_px.unwrap_or(0),
            budget_px,
            inputs,
        ));
    }
    if rows > chars {
        return Err(infeasible(
            &format!("rows {rows} exceeds chars {chars}"),
            chars,
            rows,
            inputs.glyph_px.unwrap_or(0),
            budget_px,
            inputs,
        ));
    }
    let units = 8 * rows - 1;
    let g = cap_g(budget_px / units, inputs);
    if g < 1 {
        return Err(infeasible(
            &format!(
                "rows {rows} needs at least {units}px @ 1px/glyph dot; budget is {budget_px}px"
            ),
            chars,
            rows,
            1,
            budget_px,
            inputs,
        ));
    }
    Ok(IdBlock {
        chars,
        rows,
        glyph_px: g,
    })
}

fn auto(inputs: Inputs, budget_px: u32) -> Result<IdBlock, SolverError> {
    // Per §10: maximize g, minimize rows. With chars given but rows
    // free: pick the row count that maximizes g. With chars free too:
    // default to the full id and the same rule.
    let chars = inputs.chars.unwrap_or(inputs.chars_max);
    if chars == 0 {
        return Err(infeasible(
            "no id characters to render",
            0,
            1,
            1,
            budget_px,
            &inputs,
        ));
    }
    // Try row counts smallest-first; the first that yields g >= 1
    // wins.
    for r in 1..=chars {
        let units = 8 * r - 1;
        let g = cap_g(budget_px / units, &inputs);
        if g >= 1 {
            // Keep walking until the *next* row count drops g (so we
            // pick the smallest rows for the maximal g).
            let mut best = (r, g);
            for r2 in (r + 1)..=chars {
                let g2 = cap_g(budget_px / (8 * r2 - 1), &inputs);
                if g2 > best.1 {
                    best = (r2, g2);
                }
            }
            // Among ties (g equal), the smallest row count wins —
            // already what the loop selects (best is only replaced on
            // strict >).
            return Ok(IdBlock {
                chars,
                rows: best.0,
                glyph_px: best.1,
            });
        }
    }
    Err(infeasible(
        &format!("chars {chars} cannot fit budget {budget_px}px even at 1 row, 1px/glyph dot"),
        chars,
        1,
        1,
        budget_px,
        &inputs,
    ))
}

/// Construct the §10 nearest-feasible-triple message.
fn infeasible(
    why: &str,
    chars: u32,
    rows: u32,
    glyph_px: u32,
    budget_px: u32,
    inputs: &Inputs,
) -> SolverError {
    // Candidate 1: same rows, biggest g that fits.
    let g1 = if rows >= 1 {
        Some(cap_g(budget_px / (8 * rows.max(1) - 1), inputs))
    } else {
        None
    };
    // Candidate 2: same g, smallest rows that fit.
    let mut r2 = None;
    if glyph_px >= 1 {
        let max_r = inputs.chars_max.max(chars).max(1);
        for r in 1..=max_r {
            if (8 * r - 1) * glyph_px <= budget_px {
                r2 = Some(r);
                break;
            }
        }
    }
    let mut hints: Vec<String> = Vec::new();
    if let Some(g) = g1 {
        if g >= 1 && g != glyph_px {
            hints.push(format!("rows {rows} @ {g}px"));
        }
    }
    if let Some(r) = r2 {
        if r != rows {
            hints.push(format!("rows {r} @ {glyph_px}px"));
        }
    }
    let hint = if hints.is_empty() {
        // Fall back to bumping the budget: smallest size that would
        // hold the requested triple.
        let need = (8 * rows.max(1) - 1) * glyph_px.max(1);
        format!("increase the size: rows {rows} @ {glyph_px}px needs {need}px")
    } else {
        format!("feasible: {}", hints.join(" or "))
    };
    SolverError {
        message: format!("id solver: {why} — {hint}"),
    }
}

// ---------- tests ----------

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(c: Option<u32>, r: Option<u32>, g: Option<u32>) -> Inputs {
        Inputs {
            chars: c,
            rows: r,
            glyph_px: g,
            glyph_px_cap: None,
            chars_max: 14,
        }
    }

    #[test]
    fn validate_triple_ok() {
        // 14 chars, 3 rows, g=3 -> 3·(8·3 - 1) px? No, (8·3 - 1)·3 = 69
        let b = solve(inputs(Some(14), Some(3), Some(2)), 60).unwrap();
        assert_eq!(
            b,
            IdBlock {
                chars: 14,
                rows: 3,
                glyph_px: 2
            }
        );
        assert_eq!(b.grouping_str(), "554");
    }

    #[test]
    fn derive_g_from_chars_rows() {
        // 14 chars / 2 rows / budget 51px: budget/units = 51/15 = 3.
        let b = solve(inputs(Some(14), Some(2), None), 51).unwrap();
        assert_eq!(
            b,
            IdBlock {
                chars: 14,
                rows: 2,
                glyph_px: 3
            }
        );
        assert_eq!(b.grouping_str(), "77");
    }

    #[test]
    fn derive_rows_from_chars_g_takes_smallest_rows() {
        // 14 chars / g=2 / budget 60px: rows=1 needs 7·2 = 14 ≤ 60 ok.
        let b = solve(inputs(Some(14), None, Some(2)), 60).unwrap();
        assert_eq!(b.rows, 1);
    }

    #[test]
    fn derive_chars_fills_to_id_max() {
        // rows=2, g=3, budget 60 — chars not given -> chars = chars_max.
        let b = solve(inputs(None, Some(2), Some(3)), 60).unwrap();
        assert_eq!(b.chars, 14);
    }

    #[test]
    fn auto_maximizes_g_then_minimizes_rows() {
        // Budget 60 yields g per rows: rows=1 g=8, rows=2 g=4,
        // rows=3 g=2 — max g at rows=1.
        let b = solve(inputs(None, None, None), 60).unwrap();
        assert_eq!(b.rows, 1);
        assert_eq!(b.glyph_px, 8);
        assert_eq!(b.chars, 14);
    }

    #[test]
    fn auto_respects_g_cap() {
        // module_px caps glyph_px at 3 even though raw budget could
        // host bigger g.
        let mut inp = inputs(None, None, None);
        inp.glyph_px_cap = Some(3);
        let b = solve(inp, 60).unwrap();
        assert_eq!(b.glyph_px, 3);
    }

    #[test]
    fn infeasible_validate_quotes_nearest_triple() {
        // rows 2 @ 28px needs 15·28 = 420; have 60. Same-rows feasible
        // g = 60/15 = 4. Same-g feasible rows: 8·r - 1 ≤ 60/28 = 2 →
        // no r ≥ 1 with 8r - 1 ≤ 2. Only one hint.
        let err = solve(inputs(Some(14), Some(2), Some(28)), 60).expect_err("infeasible");
        let m = err.message;
        assert!(
            m.contains("rows 2 @ 28px needs"),
            "expected need-quote: {m}"
        );
        assert!(m.contains("rows 2 @ 4px"), "expected nearest: {m}");
    }

    #[test]
    fn infeasible_with_both_hints() {
        // Clean two-hint case used by the assertion below:
        //   rows 2 @ 10 needs 150 px, budget 100
        //   same-rows nearest g  = floor(100/15) = 6
        //   same-g nearest rows  = floor((100/10 + 1) / 8) = r=1
        // (worked-out commentary kept terse so it doesn't terminate
        // in punctuation per the comment gate).
        let err = solve(inputs(Some(14), Some(2), Some(10)), 100).expect_err("infeasible");
        let m = err.message;
        assert!(m.contains("rows 2 @ 6px"), "nearest g: {m}");
        assert!(m.contains("rows 1 @ 10px"), "nearest rows: {m}");
    }

    #[test]
    fn grouping_balanced_14_3_is_554() {
        let b = IdBlock {
            chars: 14,
            rows: 3,
            glyph_px: 2,
        };
        assert_eq!(b.grouping(), vec![5, 5, 4]);
        assert_eq!(b.grouping_str(), "554");
    }

    #[test]
    fn grouping_balanced_8_2_is_44() {
        let b = IdBlock {
            chars: 8,
            rows: 2,
            glyph_px: 2,
        };
        assert_eq!(b.grouping(), vec![4, 4]);
        assert_eq!(b.grouping_str(), "44");
    }
}
