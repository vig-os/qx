//! The scan processor (ADR-032 §2): ONE pipeline
//! `FrameSource → decode → resolve → RollingAccumulator → Sink`.
//!
//! The middle is shared and DRY — every shell (CLI camera, web getUserMedia,
//! a replay fixture) runs the identical decode→resolve→accumulate core.
//! **Sources and sinks are the only per-shell parts**: a shell implements
//! [`FrameSource`] (where frames come from) and [`Sink`] (what to do with a
//! resolved id), and [`process`] wires them through the shared core.
//!
//! The [`RollingAccumulator`] debounces a live scanner: the same code stays
//! in view across many frames, so a naive pipeline would emit it dozens of
//! times. The accumulator emits each id once until it leaves the rolling
//! window, then a re-presentation emits again.

use std::collections::VecDeque;

/// A source of camera/video frames as encoded image bytes (PNG/JPEG). The
/// only per-shell input part. Returns `None` when the stream ends.
pub trait FrameSource {
    fn next_frame(&mut self) -> Option<Vec<u8>>;
}

/// A sink for resolved entity ids. The only per-shell output part (append
/// to a queue, POST to an endpoint, print to stdout, …).
pub trait Sink {
    fn emit(&mut self, id: &str);
}

/// Resolve a decoded QR payload to an entity id (ADR-032 §2). The payload
/// is the id (optionally `qx:`-prefixed or a trailing-path URL); resolution
/// strips known wrappers and trims. `None` for an empty/uninterpretable
/// payload.
pub fn resolve(payload: &str) -> Option<String> {
    let p = payload.trim();
    // A URL form (`https://…/<id>`) resolves to its last path segment.
    let core = p.rsplit('/').next().unwrap_or(p);
    let core = core.strip_prefix("qx:").unwrap_or(core).trim();
    if core.is_empty() {
        None
    } else {
        Some(core.to_string())
    }
}

/// Debounces repeated decodes of the same id across consecutive frames
/// (ADR-032 §2). An id is emitted once, then suppressed until it ages out
/// of the rolling window of the last `window` distinct ids — so a code
/// removed and re-presented emits again.
#[derive(Debug)]
pub struct RollingAccumulator {
    window: usize,
    recent: VecDeque<String>,
}

impl RollingAccumulator {
    pub fn new(window: usize) -> Self {
        Self {
            window: window.max(1),
            recent: VecDeque::new(),
        }
    }

    /// Offer an id; returns `true` if it is newly accepted (should be
    /// emitted), `false` if it is a still-in-window duplicate.
    pub fn accept(&mut self, id: &str) -> bool {
        if self.recent.iter().any(|x| x == id) {
            return false;
        }
        self.recent.push_back(id.to_string());
        while self.recent.len() > self.window {
            self.recent.pop_front();
        }
        true
    }
}

/// Run the shared scan core over a [`FrameSource`], emitting each resolved,
/// debounced id to the [`Sink`]. Frames that fail to decode or resolve are
/// skipped (a live camera yields many empty/blurred frames). Returns the
/// number of ids emitted.
#[cfg(feature = "decoder")]
pub fn process<F: FrameSource, S: Sink>(
    source: &mut F,
    sink: &mut S,
    acc: &mut RollingAccumulator,
) -> usize {
    let mut emitted = 0;
    while let Some(frame) = source.next_frame() {
        let Ok(payload) = crate::qr::decode_qr(&frame) else {
            continue;
        };
        let Some(id) = resolve(&payload) else {
            continue;
        };
        if acc.accept(&id) {
            sink.emit(&id);
            emitted += 1;
        }
    }
    emitted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_strips_wrappers() {
        assert_eq!(
            resolve("  ABCDEFGHJKMNPQ  ").as_deref(),
            Some("ABCDEFGHJKMNPQ")
        );
        assert_eq!(
            resolve("qx:ABCDEFGHJKMNPQ").as_deref(),
            Some("ABCDEFGHJKMNPQ")
        );
        assert_eq!(
            resolve("https://reg.example/p/ABCDEFGHJKMNPQ").as_deref(),
            Some("ABCDEFGHJKMNPQ")
        );
        assert_eq!(resolve("   "), None);
    }

    #[test]
    fn accumulator_debounces_within_window_and_re_emits_after() {
        let mut acc = RollingAccumulator::new(2);
        assert!(acc.accept("A")); // new
        assert!(!acc.accept("A")); // still in window → suppressed
        assert!(acc.accept("B")); // new
        assert!(acc.accept("C")); // new — evicts A (window=2)
        assert!(acc.accept("A")); // A aged out → emits again
    }

    // ---- Replay-fixture conformance (ADR-032 §2) ----

    #[cfg(feature = "decoder")]
    fn qr_png(payload: &str) -> Vec<u8> {
        use image::{DynamicImage, ImageBuffer, Luma};
        let matrix = crate::qr::encode(payload, false).expect("encode");
        let qz = matrix.quiet_zone();
        let total = matrix.total_modules();
        let module_px = 8u32;
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
            .expect("png");
        out
    }

    struct VecFrames(std::vec::IntoIter<Vec<u8>>);
    impl FrameSource for VecFrames {
        fn next_frame(&mut self) -> Option<Vec<u8>> {
            self.0.next()
        }
    }

    #[derive(Default)]
    struct CapturingSink(Vec<String>);
    impl Sink for CapturingSink {
        fn emit(&mut self, id: &str) {
            self.0.push(id.to_string());
        }
    }

    #[cfg(feature = "decoder")]
    #[test]
    fn replay_fixture_decodes_resolves_and_debounces() {
        const A: &str = "K7M3PQ9RT5VAXY";
        const B: &str = "ABCDEFGHJKMNPQ";
        // A live-scanner trace: A held across 3 frames, one blank frame,
        // then B held across 2 frames. The one processor must emit A once
        // then B once — the per-frame duplicates are debounced.
        let frames = vec![
            qr_png(A),
            qr_png(A),
            qr_png(A),
            vec![0u8; 16], // undecodable (blurred) frame — skipped
            qr_png(B),
            qr_png(B),
        ];
        let mut source = VecFrames(frames.into_iter());
        let mut sink = CapturingSink::default();
        let mut acc = RollingAccumulator::new(8);
        let emitted = process(&mut source, &mut sink, &mut acc);
        assert_eq!(emitted, 2, "two distinct codes, debounced: {:?}", sink.0);
        assert_eq!(sink.0, vec![A.to_string(), B.to_string()]);
    }
}
