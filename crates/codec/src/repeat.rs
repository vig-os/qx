//! Repeat primitives — composing the rendered label into N copies
//! (ADR-031 §10, 2026-06-12).
//!
//! The repeat module is ORTHOGONAL to the single-label renderer:
//! it takes a rendered [`PxLabel`] and composes copies along an axis
//! into a new label. The whole label may also be rotated 0/90/180/270
//! BEFORE repeating; 90/270 swap the label's width/height (exact —
//! all coords are integers under right-angle rotations).
//!
//! Semantics (px path only — the mm renderer stays untouched):
//!
//! - `--repeat <n|fill>` — how many copies (`fill` = max that fit
//!   `--length` with `--repeat-gap` as the floor).
//! - `--repeat-axis along|across` — `along` = the canvas's long/flow
//!   axis (horz canvas = horizontal, vert canvas = vertical); `across`
//!   = the orthogonal one (multi-up rows).
//! - `--length <N>[px|mm]` — required for `fill` and for derived gaps.
//! - `--spacing linear|cyclic` — linear has `n-1` gaps between
//!   copies, cyclic has `n` gaps (closed loops; n=2 lands copies on
//!   opposite sides of a cable).
//! - `--repeat-gap <N>[px|mm]` — explicit gap floor (linear floor for
//!   `fill`, exact gap when `--length` is omitted).
//! - `--repeat-orient same|alternate` — alternate rotates every
//!   second copy 180° (label flipped — same right-angle math).
//! - `--rotate 0|90|180|270` — whole-label rotation BEFORE repeating.
//! - `--length-excess <N>[px|mm]` + `--excess-at start|end` — a BLANK
//!   leader (start) or trailing wrap (end). Physical semantics per §10
//!   are documentation-only here.
//!
//! Errors carry FEASIBLE alternatives: `n·w > length` reports what
//! would fit at the given gap, and what gap would be needed for the
//! requested n.

use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

use crate::px::PxLabel;
use crate::CodecError;

/// Axis of the repeat: `along` follows the canvas's flow direction
/// (horz = horizontal, vert = vertical); `across` is the orthogonal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RepeatAxis {
    /// The canvas's long/flow axis (horz canvas = horizontal,
    /// vert canvas = vertical). Default.
    #[default]
    Along,
    /// The orthogonal axis (multi-up rows).
    Across,
}

impl RepeatAxis {
    fn name(self) -> &'static str {
        match self {
            RepeatAxis::Along => "along",
            RepeatAxis::Across => "across",
        }
    }
}

/// Inter-copy spacing model. `linear` has `n-1` gaps; `cyclic` has
/// `n` gaps (closed loops).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Spacing {
    /// `n-1` gaps between `n` copies (open strip).
    #[default]
    Linear,
    /// `n` gaps — copies repeat around a closed loop; n=2 lands on
    /// opposite sides of a cable.
    Cyclic,
}

impl Spacing {
    fn name(self) -> &'static str {
        match self {
            Spacing::Linear => "linear",
            Spacing::Cyclic => "cyclic",
        }
    }
}

/// Per-copy orientation. `same` = every copy oriented identically,
/// `alternate` = every second copy rotated 180° (for cable flags
/// where readers come from both sides).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Orient {
    /// Every copy identical.
    #[default]
    Same,
    /// Every second copy rotated 180°.
    Alternate,
}

impl Orient {
    fn name(self) -> &'static str {
        match self {
            Orient::Same => "same",
            Orient::Alternate => "alternate",
        }
    }
}

/// Whole-label rotation in right angles (applied BEFORE the repeat
/// composition). 90/270 swap label width/height.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rotate {
    #[default]
    R0,
    R90,
    R180,
    R270,
}

impl Rotate {
    /// Build from a degree value; only 0/90/180/270 are valid.
    pub fn from_deg(d: u32) -> Result<Self, String> {
        match d {
            0 => Ok(Rotate::R0),
            90 => Ok(Rotate::R90),
            180 => Ok(Rotate::R180),
            270 => Ok(Rotate::R270),
            _ => Err(format!(
                "rotate {d}: only 0, 90, 180, 270 supported (right angles)"
            )),
        }
    }

    fn deg(self) -> u32 {
        match self {
            Rotate::R0 => 0,
            Rotate::R90 => 90,
            Rotate::R180 => 180,
            Rotate::R270 => 270,
        }
    }
}

/// `--repeat <n|fill>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepeatCount {
    /// Explicit count.
    N(u32),
    /// Fit as many copies as possible into `--length` honoring the
    /// `--repeat-gap` floor.
    Fill,
}

/// Where the `--length-excess` BLANK zone sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExcessAt {
    /// `start` — under-wrap leader (opaque media: the leader sits
    /// under the print, sealing it from the side it's applied from).
    Start,
    /// `end` — self-laminating tail (clear media: the tail wraps OVER
    /// the print after one circumference).
    End,
}

impl ExcessAt {
    fn name(self) -> &'static str {
        match self {
            ExcessAt::Start => "start",
            ExcessAt::End => "end",
        }
    }
}

/// Repeat options. `length_px` is required when count is `Fill` or
/// when no explicit gap is given.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepeatOpts {
    pub count: RepeatCount,
    pub axis: RepeatAxis,
    pub spacing: Spacing,
    pub orient: Orient,
    pub rotate: Rotate,
    /// Required for `fill` and for deriving gaps when no
    /// `--repeat-gap` is given.
    pub length_px: Option<u32>,
    /// Explicit inter-copy gap floor in px. When None, derived from
    /// `length_px` and `n` per `spacing`.
    pub gap_px: Option<u32>,
    /// `--length-excess` blank zone (px).
    pub excess_px: u32,
    /// Side the excess lives on.
    pub excess_at: ExcessAt,
}

impl Default for RepeatOpts {
    fn default() -> Self {
        Self {
            count: RepeatCount::N(1),
            axis: RepeatAxis::default(),
            spacing: Spacing::default(),
            orient: Orient::default(),
            rotate: Rotate::default(),
            length_px: None,
            gap_px: None,
            excess_px: 0,
            excess_at: ExcessAt::End,
        }
    }
}

/// Resolved repeat geometry — what the receipt records.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepeatResolved {
    /// Final copy count (after Fill resolution).
    pub n: u32,
    /// Final inter-copy gap in px (after derivation).
    pub gap_px: u32,
    /// Axis name.
    pub axis: String,
    /// Orient name.
    pub orient: String,
    /// Rotate degrees (0/90/180/270).
    pub rotate: u32,
    /// Spacing name.
    pub spacing: String,
    /// Length excess blank zone in px.
    pub excess_px: u32,
    /// `start` | `end`.
    pub excess_at: String,
    /// The resolved length in px the strip occupies along the axis
    /// (including the excess zone if any). Useful for sanity.
    pub length_px: u32,
}

/// One-glyph cell px floor for the `fill` gap default — picked as a
/// sensible minimum that prevents zero-gap stamping. Documented
/// constant the caller can override via `gap_px`.
pub const FILL_GAP_FLOOR_PX: u32 = 1;

/// Rotate the label per `rotate` (right-angle exact). Returns the
/// pre-repeat composed PxLabel with swapped dims when rotated 90/270.
///
/// Implementation strategy: wrap the original SVG body in a transform
/// group that applies the rotation. We re-emit the outer `<svg>` with
/// the new (possibly swapped) dimensions.
fn rotate_label(label: &PxLabel, rotate: Rotate) -> (String, u32, u32) {
    let body = svg_inner(&label.svg);
    let (w, h) = (label.width_px, label.height_px);
    match rotate {
        Rotate::R0 => (body.to_string(), w, h),
        Rotate::R90 => {
            // rotate 90 around origin then translate so the result
            // sits inside the new (h x w) canvas
            let inner = format!(
                "<g transform=\"rotate(90) translate(0,-{w})\">{body}</g>",
                w = w,
                body = body
            );
            (inner, h, w)
        }
        Rotate::R180 => {
            let inner = format!(
                "<g transform=\"rotate(180) translate(-{w},-{h})\">{body}</g>",
                w = w,
                h = h,
                body = body
            );
            (inner, w, h)
        }
        Rotate::R270 => {
            let inner = format!(
                "<g transform=\"rotate(270) translate(-{h},0)\">{body}</g>",
                h = h,
                body = body
            );
            (inner, h, w)
        }
    }
}

/// Extract inside-of-svg content (between `<svg ...>` and `</svg>`).
fn svg_inner(svg: &str) -> &str {
    let start = match svg.find('>') {
        Some(i) => i + 1,
        None => return "",
    };
    let end = svg.rfind("</svg>").unwrap_or(svg.len());
    svg[start..end].trim_matches('\n')
}

/// Compose the labels via [`RepeatOpts`]. Returns a new SVG document
/// + canvas dims + the resolved repeat receipt object.
///
/// Errors:
/// - [`CodecError::Render`] when the requested `n` does not fit
///   `length_px` (linear: `n·w + (n-1)·gap ≤ length`; cyclic:
///   `n·w + n·gap ≤ length` for given gap, or `n·w ≤ length` for
///   derived gap). Carries feasible alternatives.
/// - [`CodecError::Render`] when `Fill` is requested without
///   `length_px`, or when no gap can be derived (no length, n>1).
pub fn compose(label: &PxLabel, opts: &RepeatOpts) -> Result<RepeatComposed, CodecError> {
    // Step 1: rotate the source label.
    let (rotated_body, w, h) = rotate_label(label, opts.rotate);

    // The per-copy size along the repeat axis.
    let (copy_main, copy_cross) = match opts.axis {
        RepeatAxis::Along => (w, h),
        RepeatAxis::Across => (h, w),
    };

    // Step 2: resolve n + gap from (count, length, gap, spacing).
    let (n, gap_px, length_used) =
        resolve_n_and_gap(opts, copy_main).map_err(CodecError::Render)?;

    // Step 3: derive the strip's geometry.
    // Total length along axis = excess + content (n copies + gaps).
    let content_len = match opts.spacing {
        Spacing::Linear => n * copy_main + n.saturating_sub(1) * gap_px,
        Spacing::Cyclic => n * copy_main + n * gap_px,
    };
    let total_main = content_len + opts.excess_px;
    // Allow opts.length_px to override (e.g. cyclic with explicit
    // length).
    let total_main = opts
        .length_px
        .map(|l| l.max(total_main))
        .unwrap_or(total_main);

    let (canvas_w, canvas_h) = match opts.axis {
        RepeatAxis::Along => (total_main, copy_cross),
        RepeatAxis::Across => (copy_cross, total_main),
    };

    // Step 4: place copies. The excess sits at start or end.
    let lead_offset = match opts.excess_at {
        ExcessAt::Start => opts.excess_px,
        ExcessAt::End => 0,
    };

    let mut groups = String::new();
    let _ = write!(groups, "<g shape-rendering=\"crispEdges\">");
    for i in 0..n {
        let lead = lead_offset + i * (copy_main + gap_px);
        let (dx, dy) = match opts.axis {
            RepeatAxis::Along => (lead, 0),
            RepeatAxis::Across => (0, lead),
        };
        // alternate = every odd copy rotated 180 within its own box
        let body = if matches!(opts.orient, Orient::Alternate) && i % 2 == 1 {
            format!(
                "<g transform=\"translate({dx},{dy})\">\
                    <g transform=\"rotate(180) translate(-{w},-{h})\">{rotated_body}</g>\
                 </g>",
                dx = dx,
                dy = dy,
                w = w,
                h = h,
                rotated_body = rotated_body
            )
        } else {
            format!(
                "<g transform=\"translate({dx},{dy})\">{rotated_body}</g>",
                dx = dx,
                dy = dy,
                rotated_body = rotated_body
            )
        };
        groups.push_str(&body);
    }
    groups.push_str("</g>");

    // Background rect on the new canvas — honor the source's bg.
    let bg = &label.receipt.bg;
    let bg_rect = if bg == "none" {
        String::new()
    } else {
        format!(
            "<rect width=\"{canvas_w}\" height=\"{canvas_h}\" fill=\"{bg}\"/>",
            canvas_w = canvas_w,
            canvas_h = canvas_h,
            bg = bg
        )
    };

    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{canvas_w}\" height=\"{canvas_h}\" \
viewBox=\"0 0 {canvas_w} {canvas_h}\">{bg_rect}{groups}</svg>\n"
    );

    let resolved = RepeatResolved {
        n,
        gap_px,
        axis: opts.axis.name().into(),
        orient: opts.orient.name().into(),
        rotate: opts.rotate.deg(),
        spacing: opts.spacing.name().into(),
        excess_px: opts.excess_px,
        excess_at: opts.excess_at.name().into(),
        length_px: total_main,
    };
    let _ = length_used;
    Ok(RepeatComposed {
        svg,
        width_px: canvas_w,
        height_px: canvas_h,
        resolved,
    })
}

/// Result of [`compose`].
#[derive(Clone, Debug)]
pub struct RepeatComposed {
    pub svg: String,
    pub width_px: u32,
    pub height_px: u32,
    pub resolved: RepeatResolved,
}

/// Resolve `n` and `gap_px` from the input options. Returns
/// `(n, gap, length_used)` on success.
fn resolve_n_and_gap(opts: &RepeatOpts, copy_main: u32) -> Result<(u32, u32, u32), String> {
    if copy_main == 0 {
        return Err("repeat: source label has zero size on the repeat axis".into());
    }
    match opts.count {
        RepeatCount::N(n) => {
            if n == 0 {
                return Err("repeat: n must be >= 1".into());
            }
            if n == 1 {
                let gap = opts.gap_px.unwrap_or(0);
                return Ok((1, gap, copy_main + opts.excess_px));
            }
            // Two cases: explicit gap, or derive from length.
            if let Some(gap) = opts.gap_px {
                let need_content = match opts.spacing {
                    Spacing::Linear => n * copy_main + (n - 1) * gap,
                    Spacing::Cyclic => n * copy_main + n * gap,
                };
                if let Some(len) = opts.length_px {
                    let content_room = len.saturating_sub(opts.excess_px);
                    if need_content > content_room {
                        return Err(format!(
                            "repeat: {n} copies @ {copy_main}px with gap {gap}px \
                             ({spacing}) needs {need_content}px; have {content_room}px \
                             (length {len}px − excess {ex}px). \
                             Feasible: fit {fit} copies at this gap, or use gap \
                             {dgap}px for {n} copies",
                            spacing = opts.spacing.name(),
                            ex = opts.excess_px,
                            fit = max_fit(content_room, copy_main, gap, opts.spacing),
                            dgap = derived_gap(content_room, copy_main, n, opts.spacing),
                        ));
                    }
                }
                Ok((n, gap, need_content + opts.excess_px))
            } else {
                // Derive gap from length.
                let len = opts.length_px.ok_or_else(|| {
                    format!(
                        "repeat: {n} copies needs either an explicit --repeat-gap or \
                         a --length to derive the gap from"
                    )
                })?;
                let content_room = len.saturating_sub(opts.excess_px);
                let copies_main = n * copy_main;
                if copies_main > content_room {
                    return Err(format!(
                        "repeat: {n} copies @ {copy_main}px need {copies_main}px just \
                         for the copies; content room is {content_room}px \
                         (length {len}px − excess {ex}px). \
                         Feasible: at most {fit} copies fit at zero gap",
                        ex = opts.excess_px,
                        fit = content_room / copy_main,
                    ));
                }
                let gap = derived_gap(content_room, copy_main, n, opts.spacing);
                Ok((n, gap, content_room + opts.excess_px))
            }
        }
        RepeatCount::Fill => {
            let len = opts.length_px.ok_or_else(|| {
                "repeat: fill requires --length to know how many copies fit".to_string()
            })?;
            let gap_floor = opts.gap_px.unwrap_or(FILL_GAP_FLOOR_PX);
            let content_room = len.saturating_sub(opts.excess_px);
            if content_room < copy_main {
                return Err(format!(
                    "repeat: fill: content room {content_room}px (length {len} \
                     − excess {ex}) cannot hold one {copy_main}px copy",
                    ex = opts.excess_px,
                ));
            }
            let n = max_fit(content_room, copy_main, gap_floor, opts.spacing).max(1);
            Ok((n, gap_floor, content_room + opts.excess_px))
        }
    }
}

/// Max copies fitting `content_room` at `copy_main` width with gap
/// floor `gap` and the chosen spacing.
fn max_fit(content_room: u32, copy_main: u32, gap: u32, spacing: Spacing) -> u32 {
    match spacing {
        Spacing::Linear => {
            // n·w + (n-1)·g ≤ R → n ≤ (R + g) / (w + g)
            if copy_main + gap == 0 {
                return 0;
            }
            (content_room + gap) / (copy_main + gap)
        }
        Spacing::Cyclic => {
            // n·w + n·g ≤ R → n ≤ R / (w + g)
            if copy_main + gap == 0 {
                return 0;
            }
            content_room / (copy_main + gap)
        }
    }
}

/// Derived gap for n copies fitting content_room exactly per spacing.
fn derived_gap(content_room: u32, copy_main: u32, n: u32, spacing: Spacing) -> u32 {
    let total_copies = n * copy_main;
    if total_copies >= content_room {
        return 0;
    }
    let slack = content_room - total_copies;
    match spacing {
        Spacing::Linear => {
            if n <= 1 {
                0
            } else {
                slack / (n - 1)
            }
        }
        Spacing::Cyclic => slack / n,
    }
}

/// Deprecated `--layout flag` + `--cable-od D` sugar (ADR-031 §10):
/// expands to repeat 2 / spacing linear / gap = (2·2mm + π·D mm)
/// at the given dpi / orient alternate.
///
/// Returns the equivalent [`RepeatOpts`] (with a warning string the
/// caller should bubble back to the user).
pub fn deprecated_flag_sugar(cable_od_mm: f64, dpi: f64) -> (RepeatOpts, String) {
    // 2·margin_mm + π·od_mm; margin_mm = 2 (canonical cable flag).
    let gap_mm = 2.0 * 2.0 + std::f64::consts::PI * cable_od_mm;
    let gap_px = (gap_mm / 25.4 * dpi).round().max(0.0) as u32;
    let opts = RepeatOpts {
        count: RepeatCount::N(2),
        axis: RepeatAxis::Along,
        spacing: Spacing::Linear,
        orient: Orient::Alternate,
        rotate: Rotate::R0,
        length_px: None,
        gap_px: Some(gap_px),
        excess_px: 0,
        excess_at: ExcessAt::End,
    };
    let warning = format!(
        "layout=flag + cable_od={cable_od_mm}mm is deprecated sugar over \
         --repeat 2 --spacing linear --repeat-gap {gap_px}px \
         --repeat-orient alternate (gap = 2·2mm + π·{cable_od_mm}mm at \
         {dpi}dpi)"
    );
    (opts, warning)
}

// ---------- tests ----------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::receipt::Receipt;

    fn fake_label(w: u32, h: u32) -> PxLabel {
        // A minimal PxLabel for repeat-composition tests — the source
        // SVG is a single black square the size of the label.
        let svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" \
viewBox=\"0 0 {w} {h}\"><rect width=\"{w}\" height=\"{h}\" fill=\"white\"/>\
<g fill=\"black\" shape-rendering=\"crispEdges\"><rect x=\"0\" y=\"0\" width=\"{w}\" height=\"{h}\"/></g></svg>"
        );
        let receipt = Receipt {
            id: "TESTID".into(),
            payload: "qr".into(),
            symbology: "micro-m4-m".into(),
            size_px: w,
            padding: [0, 0, 0, 0],
            padding_mode: "overlap".into(),
            size_mode: "exact".into(),
            qr_px: w,
            module_px: 1,
            glyph_px: 0,
            fg: "black".into(),
            bg: "white".into(),
            font: "nx75".into(),
            generator: "test".into(),
        };
        PxLabel {
            svg,
            width_px: w,
            height_px: h,
            qr_px: w,
            module_px: 1,
            modules: w,
            data_px: w,
            glyph_px: 0,
            glyph_cell: 7,
            white: crate::Padding::uniform(0),
            padding_mode: crate::PaddingMode::Overlap,
            symbology: "micro-m4-m".into(),
            receipt,
        }
    }

    // ---------- math table ----------

    #[test]
    fn linear_n_with_explicit_gap() {
        let l = fake_label(20, 10);
        let opts = RepeatOpts {
            count: RepeatCount::N(3),
            gap_px: Some(5),
            length_px: Some(100),
            ..Default::default()
        };
        let c = compose(&l, &opts).expect("composes");
        // 3·20 + 2·5 = 70; canvas length = max(70, 100) = 100
        assert_eq!(c.width_px, 100);
        assert_eq!(c.height_px, 10);
        assert_eq!(c.resolved.n, 3);
        assert_eq!(c.resolved.gap_px, 5);
        assert_eq!(c.resolved.spacing, "linear");
    }

    #[test]
    fn cyclic_n_derived_gap_from_length() {
        let l = fake_label(20, 10);
        let opts = RepeatOpts {
            count: RepeatCount::N(4),
            spacing: Spacing::Cyclic,
            length_px: Some(100),
            gap_px: None,
            ..Default::default()
        };
        let c = compose(&l, &opts).expect("composes");
        // cyclic: 4·20 + 4·g = 100 -> g = 5
        assert_eq!(c.resolved.n, 4);
        assert_eq!(c.resolved.gap_px, 5);
        assert_eq!(c.resolved.spacing, "cyclic");
    }

    #[test]
    fn linear_n_derived_gap_from_length() {
        let l = fake_label(20, 10);
        let opts = RepeatOpts {
            count: RepeatCount::N(3),
            spacing: Spacing::Linear,
            length_px: Some(100),
            gap_px: None,
            ..Default::default()
        };
        let c = compose(&l, &opts).expect("composes");
        // linear: 3·20 + 2·g = 100 -> g = 20
        assert_eq!(c.resolved.gap_px, 20);
    }

    #[test]
    fn fill_count_resolved_from_length() {
        let l = fake_label(20, 10);
        let opts = RepeatOpts {
            count: RepeatCount::Fill,
            spacing: Spacing::Linear,
            length_px: Some(100),
            gap_px: Some(5),
            ..Default::default()
        };
        let c = compose(&l, &opts).expect("composes");
        // (100 + 5) / (20 + 5) = 4
        assert_eq!(c.resolved.n, 4);
        assert_eq!(c.resolved.gap_px, 5);
    }

    #[test]
    fn fill_cyclic_count() {
        let l = fake_label(20, 10);
        let opts = RepeatOpts {
            count: RepeatCount::Fill,
            spacing: Spacing::Cyclic,
            length_px: Some(100),
            gap_px: Some(5),
            ..Default::default()
        };
        let c = compose(&l, &opts).expect("composes");
        // cyclic fill: 100 / (20+5) = 4
        assert_eq!(c.resolved.n, 4);
    }

    #[test]
    fn excess_at_end() {
        let l = fake_label(20, 10);
        let opts = RepeatOpts {
            count: RepeatCount::N(2),
            spacing: Spacing::Linear,
            gap_px: Some(5),
            excess_px: 30,
            excess_at: ExcessAt::End,
            ..Default::default()
        };
        let c = compose(&l, &opts).expect("composes");
        // content = 2·20+5 = 45; +excess 30 = 75
        assert_eq!(c.width_px, 75);
        assert_eq!(c.resolved.excess_at, "end");
        // first copy at x=0
        assert!(c.svg.contains("translate(0,0)"));
    }

    #[test]
    fn excess_at_start_shifts_first_copy() {
        let l = fake_label(20, 10);
        let opts = RepeatOpts {
            count: RepeatCount::N(1),
            excess_px: 30,
            excess_at: ExcessAt::Start,
            ..Default::default()
        };
        let c = compose(&l, &opts).expect("composes");
        // First copy starts at x=30
        assert!(c.svg.contains("translate(30,0)"));
        assert_eq!(c.resolved.excess_at, "start");
    }

    #[test]
    fn rotate_90_swaps_dims() {
        let l = fake_label(20, 10);
        let opts = RepeatOpts {
            count: RepeatCount::N(1),
            rotate: Rotate::R90,
            ..Default::default()
        };
        let c = compose(&l, &opts).expect("composes");
        // 90 rotation: 20x10 -> 10x20
        assert_eq!(c.width_px, 10);
        assert_eq!(c.height_px, 20);
        assert_eq!(c.resolved.rotate, 90);
    }

    #[test]
    fn rotate_270_swaps_dims() {
        let l = fake_label(20, 10);
        let opts = RepeatOpts {
            count: RepeatCount::N(1),
            rotate: Rotate::R270,
            ..Default::default()
        };
        let c = compose(&l, &opts).expect("composes");
        assert_eq!((c.width_px, c.height_px), (10, 20));
        assert_eq!(c.resolved.rotate, 270);
    }

    #[test]
    fn rotate_180_keeps_dims() {
        let l = fake_label(20, 10);
        let opts = RepeatOpts {
            count: RepeatCount::N(1),
            rotate: Rotate::R180,
            ..Default::default()
        };
        let c = compose(&l, &opts).expect("composes");
        assert_eq!((c.width_px, c.height_px), (20, 10));
    }

    #[test]
    fn across_axis_repeats_vertically_on_horz_label() {
        let l = fake_label(20, 10);
        let opts = RepeatOpts {
            count: RepeatCount::N(3),
            axis: RepeatAxis::Across,
            gap_px: Some(2),
            ..Default::default()
        };
        let c = compose(&l, &opts).expect("composes");
        // along-axis = width unchanged (20); across-axis = height
        assert_eq!(c.width_px, 20);
        // 3·10 + 2·2 = 34
        assert_eq!(c.height_px, 34);
    }

    #[test]
    fn alternate_orient_rotates_odd_copies_180() {
        let l = fake_label(20, 10);
        let opts = RepeatOpts {
            count: RepeatCount::N(2),
            gap_px: Some(0),
            orient: Orient::Alternate,
            ..Default::default()
        };
        let c = compose(&l, &opts).expect("composes");
        assert!(c.svg.contains("rotate(180)"));
        assert_eq!(c.resolved.orient, "alternate");
    }

    #[test]
    fn n_eq_1_no_gap_no_alternate() {
        let l = fake_label(20, 10);
        let opts = RepeatOpts {
            count: RepeatCount::N(1),
            ..Default::default()
        };
        let c = compose(&l, &opts).expect("composes");
        assert_eq!(c.resolved.n, 1);
        assert_eq!(c.width_px, 20);
        assert_eq!(c.height_px, 10);
    }

    // ---------- error paths ----------

    #[test]
    fn infeasible_n_quotes_feasible_alternatives() {
        let l = fake_label(20, 10);
        let opts = RepeatOpts {
            count: RepeatCount::N(10),
            gap_px: Some(5),
            length_px: Some(100),
            ..Default::default()
        };
        let err = compose(&l, &opts).expect_err("infeasible");
        let msg = err.to_string();
        assert!(msg.contains("Feasible"), "got: {msg}");
        // (100 + 5) / 25 = 4 copies at gap 5
        assert!(msg.contains("fit 4 copies"), "got: {msg}");
    }

    #[test]
    fn fill_without_length_errors() {
        let l = fake_label(20, 10);
        let opts = RepeatOpts {
            count: RepeatCount::Fill,
            length_px: None,
            ..Default::default()
        };
        let err = compose(&l, &opts).expect_err("fill needs length");
        assert!(err.to_string().contains("--length"), "got: {err}");
    }

    #[test]
    fn n_gt_1_no_gap_no_length_errors() {
        let l = fake_label(20, 10);
        let opts = RepeatOpts {
            count: RepeatCount::N(3),
            gap_px: None,
            length_px: None,
            ..Default::default()
        };
        let err = compose(&l, &opts).expect_err("need gap or length");
        assert!(err.to_string().contains("--repeat-gap"), "got: {err}");
    }

    // ---------- deprecated flag sugar ----------

    #[test]
    fn deprecated_flag_sugar_expands_correctly() {
        let (opts, warn) = deprecated_flag_sugar(6.0, 300.0);
        assert_eq!(opts.count, RepeatCount::N(2));
        assert_eq!(opts.spacing, Spacing::Linear);
        assert_eq!(opts.orient, Orient::Alternate);
        // gap = (4 + π·6) mm at 300dpi = (4 + 18.849...) mm
        // = 22.849 mm = 22.849/25.4*300 = ~269.9px
        let expected = (((2.0 * 2.0 + std::f64::consts::PI * 6.0) / 25.4 * 300.0).round()) as u32;
        assert_eq!(opts.gap_px, Some(expected));
        assert!(warn.contains("deprecated"));
        assert!(warn.contains("--repeat 2"));
    }

    // ---------- Rotate parsing ----------

    #[test]
    fn rotate_from_deg_valid_angles() {
        assert!(Rotate::from_deg(0).is_ok());
        assert!(Rotate::from_deg(90).is_ok());
        assert!(Rotate::from_deg(180).is_ok());
        assert!(Rotate::from_deg(270).is_ok());
    }

    #[test]
    fn rotate_from_deg_rejects_non_right_angles() {
        for d in [45_u32, 89, 91, 360] {
            let err = Rotate::from_deg(d).expect_err("not right");
            assert!(err.contains("right angles"), "got: {err}");
        }
    }
}
