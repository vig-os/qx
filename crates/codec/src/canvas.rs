//! Canvas group geometry (ADR-031 §10, 2026-06-12).
//!
//! Stage 2 surface: a payload of the form
//! `[c <W>x<H><unit>: leaf@(x,y)[@size] ...]` declares a fixed-zone
//! canvas with leaves at explicit positions (die-cut / fixed-slot
//! stock). This module RESOLVES positions/sizes to device px,
//! VALIDATES that every child sits inside the canvas (ERROR on
//! overflow, message names the overflowing child + the overflow
//! px), and DETECTS overlaps (WARN — except qr-over-qr which is
//! ERROR).
//!
//! Actual rendering of nested leaves at arbitrary positions is a
//! separate concern from validation. The engine surfaces canvas
//! validation results and a resolved-tree receipt; full canvas
//! render (bitmap composition into one SVG) lands when the
//! consumer needs it.

use serde::{Deserialize, Serialize};

use crate::payload::{CanvasChild, CanvasDim, Element, NodeSize};

/// One resolved canvas child — coordinates + size in device px, with
/// the element preserved.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedChild {
    pub element: Element,
    pub x_px: u32,
    pub y_px: u32,
    /// Inferred or explicit width in device px (default 0 = sentinel
    /// for "no explicit size, content-derived").
    pub w_px: u32,
    pub h_px: u32,
}

/// Result of validating a canvas group.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasResolved {
    /// Canvas dims in device px.
    pub width_px: u32,
    pub height_px: u32,
    pub children: Vec<ResolvedChild>,
    /// Overlap warnings (non-qr-over-qr; qr-over-qr is an error,
    /// surfaces before this struct is built).
    pub overlaps: Vec<String>,
}

/// Convert a [`CanvasDim`] to px at the given `dpi`. `dpi` is needed
/// when the dim rides `mm`.
pub fn dim_to_px(dim: CanvasDim, dpi: f64) -> u32 {
    match dim {
        CanvasDim::Px { px } => px,
        CanvasDim::Mm { mm } => (f64::from(mm) / 25.4 * dpi).round() as u32,
    }
}

/// Convert a [`NodeSize`] (when explicit) to px at the given `dpi`.
/// `None` for `Flex` (canvas children should not be flex-sized).
pub fn size_to_px(size: NodeSize, dpi: f64) -> Option<u32> {
    match size {
        NodeSize::Px { px } => Some(px),
        NodeSize::Mm { mm } => Some((f64::from(mm) / 25.4 * dpi).round() as u32),
        NodeSize::Flex { .. } => None,
    }
}

/// Resolve a canvas group's geometry. Returns ERROR when a child
/// overflows the canvas (message names the child + overflow px) or
/// when two qr-bearing children overlap. Non-qr-over-qr overlaps are
/// collected into [`CanvasResolved::overlaps`] as warnings.
///
/// Stage 2 size policy: when a child carries no explicit size, the
/// resolved width/height is recorded as 0 — a sentinel for
/// "content-derived, deferred". Bounds checks against zero-size are
/// trivially satisfied. Overlap detection only fires on children
/// with explicit non-zero sizes.
pub fn resolve_canvas(
    width: CanvasDim,
    height: CanvasDim,
    children: &[CanvasChild],
    dpi: f64,
) -> Result<CanvasResolved, String> {
    let w = dim_to_px(width, dpi);
    let h = dim_to_px(height, dpi);

    let mut resolved = Vec::with_capacity(children.len());
    for (i, c) in children.iter().enumerate() {
        let x = dim_to_px(c.x, dpi);
        let y = dim_to_px(c.y, dpi);
        let size_px = c.size.and_then(|s| size_to_px(s, dpi)).unwrap_or(0);
        // For QR / id / space, the size attribute is interpreted as
        // the main-axis extent — for a canvas, treat it as a square
        // bounding box for validation (stage 2 sizing details are
        // refined as actual render lands).
        let (w_box, h_box) = (size_px, size_px);
        // Bounds check: a child with a known size must fit; a
        // zero-size child only needs its origin inside the canvas.
        if x > w {
            return Err(format!(
                "canvas: child #{i} ({el}) x={x}px is outside canvas \
                 width {w}px (overflow {ov}px)",
                el = element_name(&c.element),
                ov = x - w,
            ));
        }
        if y > h {
            return Err(format!(
                "canvas: child #{i} ({el}) y={y}px is outside canvas \
                 height {h}px (overflow {ov}px)",
                el = element_name(&c.element),
                ov = y - h,
            ));
        }
        if size_px > 0 {
            if x + w_box > w {
                return Err(format!(
                    "canvas: child #{i} ({el}) at x={x} size {size_px}px \
                     overflows canvas width {w}px (overflow {ov}px)",
                    el = element_name(&c.element),
                    ov = (x + w_box) - w,
                ));
            }
            if y + h_box > h {
                return Err(format!(
                    "canvas: child #{i} ({el}) at y={y} size {size_px}px \
                     overflows canvas height {h}px (overflow {ov}px)",
                    el = element_name(&c.element),
                    ov = (y + h_box) - h,
                ));
            }
        }
        resolved.push(ResolvedChild {
            element: c.element.clone(),
            x_px: x,
            y_px: y,
            w_px: w_box,
            h_px: h_box,
        });
    }

    // Overlap pass: O(n²) with explicit sizes only. qr-over-qr is
    // an ERROR (the §10 contract); other overlaps are WARN.
    let mut overlaps = Vec::new();
    for i in 0..resolved.len() {
        for j in (i + 1)..resolved.len() {
            if !rects_overlap(&resolved[i], &resolved[j]) {
                continue;
            }
            let a = &resolved[i];
            let b = &resolved[j];
            if is_qr(&a.element) && is_qr(&b.element) {
                return Err(format!(
                    "canvas: qr-over-qr overlap — children #{i} and #{j} \
                     both place a QR symbol at overlapping rects"
                ));
            }
            overlaps.push(format!(
                "canvas: child #{i} ({a_el}) overlaps child #{j} ({b_el})",
                a_el = element_name(&a.element),
                b_el = element_name(&b.element),
            ));
        }
    }

    Ok(CanvasResolved {
        width_px: w,
        height_px: h,
        children: resolved,
        overlaps,
    })
}

fn rects_overlap(a: &ResolvedChild, b: &ResolvedChild) -> bool {
    if a.w_px == 0 || a.h_px == 0 || b.w_px == 0 || b.h_px == 0 {
        return false;
    }
    let a_x2 = a.x_px + a.w_px;
    let a_y2 = a.y_px + a.h_px;
    let b_x2 = b.x_px + b.w_px;
    let b_y2 = b.y_px + b.h_px;
    !(a_x2 <= b.x_px || b_x2 <= a.x_px || a_y2 <= b.y_px || b_y2 <= a.y_px)
}

fn is_qr(e: &Element) -> bool {
    matches!(e, Element::Qr { .. })
}

fn element_name(e: &Element) -> &'static str {
    match e {
        Element::Qr { .. } => "qr",
        Element::Id { .. } => "id",
        Element::Space { .. } => "space",
    }
}

// ---------- tests ----------

#[cfg(test)]
mod tests {
    use super::*;

    fn child(el: Element, x: u32, y: u32, size: Option<u32>) -> CanvasChild {
        CanvasChild {
            element: el,
            x: CanvasDim::Px { px: x },
            y: CanvasDim::Px { px: y },
            size: size.map(|n| NodeSize::Px { px: n }),
        }
    }

    fn qr() -> Element {
        Element::Qr { symbology: None }
    }
    fn id() -> Element {
        Element::Id {
            grouping: None,
            id_chars: None,
        }
    }

    #[test]
    fn resolves_dims_and_positions_in_px() {
        let children = vec![child(qr(), 0, 0, Some(32)), child(id(), 40, 0, Some(16))];
        let r = resolve_canvas(
            CanvasDim::Px { px: 100 },
            CanvasDim::Px { px: 50 },
            &children,
            300.0,
        )
        .unwrap();
        assert_eq!(r.width_px, 100);
        assert_eq!(r.height_px, 50);
        assert_eq!(r.children[0].x_px, 0);
        assert_eq!(r.children[1].x_px, 40);
    }

    #[test]
    fn mm_dims_convert_at_dpi() {
        let children = vec![child(qr(), 0, 0, Some(10))];
        let r = resolve_canvas(
            CanvasDim::Mm { mm: 25 },
            CanvasDim::Mm { mm: 25 },
            &children,
            254.0, // 1mm = 10px
        )
        .unwrap();
        assert_eq!(r.width_px, 250);
        assert_eq!(r.height_px, 250);
    }

    #[test]
    fn x_overflow_errors_with_child_name_and_overflow_px() {
        let children = vec![child(qr(), 90, 0, Some(20))];
        let err = resolve_canvas(
            CanvasDim::Px { px: 100 },
            CanvasDim::Px { px: 100 },
            &children,
            300.0,
        )
        .expect_err("overflows");
        assert!(err.contains("qr"), "names child: {err}");
        assert!(err.contains("overflow 10px"), "names overflow: {err}");
    }

    #[test]
    fn origin_outside_canvas_errors() {
        let children = vec![child(qr(), 200, 0, None)];
        let err = resolve_canvas(
            CanvasDim::Px { px: 100 },
            CanvasDim::Px { px: 100 },
            &children,
            300.0,
        )
        .expect_err("origin out");
        assert!(err.contains("x=200"), "got: {err}");
    }

    #[test]
    fn qr_over_qr_overlap_is_error() {
        let children = vec![child(qr(), 0, 0, Some(50)), child(qr(), 30, 30, Some(50))];
        let err = resolve_canvas(
            CanvasDim::Px { px: 100 },
            CanvasDim::Px { px: 100 },
            &children,
            300.0,
        )
        .expect_err("qr-over-qr");
        assert!(err.contains("qr-over-qr"), "got: {err}");
    }

    #[test]
    fn qr_over_id_overlap_is_warning() {
        let children = vec![child(qr(), 0, 0, Some(50)), child(id(), 30, 30, Some(50))];
        let r = resolve_canvas(
            CanvasDim::Px { px: 100 },
            CanvasDim::Px { px: 100 },
            &children,
            300.0,
        )
        .unwrap();
        assert_eq!(r.overlaps.len(), 1);
        assert!(r.overlaps[0].contains("overlaps"), "got: {:?}", r.overlaps);
    }

    #[test]
    fn no_overlap_between_disjoint_rects() {
        let children = vec![child(qr(), 0, 0, Some(20)), child(qr(), 50, 0, Some(20))];
        let r = resolve_canvas(
            CanvasDim::Px { px: 100 },
            CanvasDim::Px { px: 100 },
            &children,
            300.0,
        )
        .unwrap();
        assert!(r.overlaps.is_empty());
    }
}
