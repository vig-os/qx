// Shared QR-camera scan helper. Lookup, Bind, Print all use this.
//
// Decoder: zxing-wasm (Apache-2.0, ~1 MB raw / ~420 KB gzipped) wrapped
// by `barcode-detector` (MIT) — the WASM port of ZXing-C++ exposed
// behind a `BarcodeDetector`-shaped API. We use it *unconditionally*,
// not as a fallback to the native API, because:
//
//  - Native BarcodeDetector availability is patchy (no Firefox, no
//    desktop Safari, partial on iOS Safari, present on Chrome).
//  - Native Micro QR advertisement is inconsistent: Chrome/Android
//    typically lists only `qr_code` and does *not* decode Micro QR;
//    iOS reports `qr_code` but the underlying VNDetectBarcodesRequest
//    decodes Micro QR transparently. We can't tell from feature
//    detection alone whether a payload like our M4 labels will be
//    picked up.
//
// Going through one decoder implementation everywhere removes the
// platform matrix and guarantees Micro QR (M1–M4) + Standard QR work
// on Chrome / Firefox / Safari / Edge, desktop and mobile.
//
// The wasm binary is bundled with the site and served from our own
// origin (Vite emits a hashed file under /assets/), so there is no
// third-party CDN dependency at runtime — important for GH Pages CSP
// and for the planned PWA / offline use case.
//
// The polyfill module is dynamic-imported on first scan so the cold
// page load isn't penalised for the camera dependency.
//
// ## Two scan modes
//
// `openScanner()` (no args) keeps the legacy single-pick live flow:
// poll the live preview, return the first decoded payload.
//
// `openScanner({ multi: true, resolveStatus })` activates the snapshot
// flow (issue #20):
//   1. Live preview, with a "Capture" button alongside Cancel.
//   2. Capture freezes the current video frame, stops the stream,
//      runs `detect()` once on the still image — the ponyfill returns
//      `cornerPoints` per match, which we use to draw tappable SVG
//      polygons over the still.
//   3. Each polygon is colour-coded by the consumer's `resolveStatus`
//      callback so the operator sees, on a bench of stickered parts,
//      "what's left to do" at a glance.
//   4. Tap a polygon → resolves with that payload. "Retake" resumes
//      the live stream.

import { el, button } from "./dom";
import { icon } from "./icons";

interface ScanResult {
  payload: string;
  format: string;
}

/** Status of a detected ID, for colour-coding overlays in snapshot
 *  mode. Consumers compute this synchronously per detected payload. */
export type ScanStatus = "bound" | "unbound" | "queued" | "unknown";

export interface ScanOptions {
  /** Per-detection status lookup. Runs synchronously per detected
   *  code in the snapshot view; keep it cheap (Map / object lookup,
   *  no async, no fetch). */
  resolveStatus?: (canonical: string) => ScanStatus;
  /** When true, after the operator hits "Capture" the scanner shows
   *  every detected code as a tappable polygon and the operator picks
   *  one. Default false (legacy single-pick live flow). */
  multi?: boolean;
}

interface Point2D {
  x: number;
  y: number;
}

interface DetectorMatch {
  rawValue: string;
  format: string;
  cornerPoints: Point2D[];
}

interface DecoderHandle {
  detect(source: CanvasImageSource): Promise<DetectorMatch[]>;
  /** Human label for the scanner overlay badge. */
  badge: string;
}

class ScanUnsupportedError extends Error {
  constructor(public readonly reason: string) {
    super(reason);
    this.name = "ScanUnsupportedError";
  }
}

/** Open the scanner UI; resolve with the decoded payload string.
 *
 * Throws `ScanUnsupportedError` if the platform can't access the
 * camera or the WASM decoder fails to load. */
export async function openScanner(opts: ScanOptions = {}): Promise<string> {
  const result = await openScannerWithDetail(opts);
  return result.payload;
}

export async function openScannerWithDetail(
  opts: ScanOptions = {},
): Promise<ScanResult> {
  if (typeof navigator === "undefined" || !navigator.mediaDevices?.getUserMedia) {
    showUnsupported(
      "This browser doesn't expose a camera API.",
      "getUserMedia is required for QR scanning. Try a recent Chrome, Firefox, Safari, or Edge.",
    );
    throw new ScanUnsupportedError("getUserMedia unavailable");
  }

  let decoder: DecoderHandle;
  try {
    decoder = await loadDecoder();
  } catch (e) {
    const msg = (e as Error)?.message ?? String(e);
    showUnsupported("The QR decoder failed to load.", `Detail: ${msg}`);
    throw new ScanUnsupportedError("decoder load failed: " + msg);
  }

  return openScannerWithDecoder(decoder, opts);
}

// ---------- Decoder loader ----------

let cachedDecoderPromise: Promise<DecoderHandle> | null = null;

function loadDecoder(): Promise<DecoderHandle> {
  if (cachedDecoderPromise) return cachedDecoderPromise;
  cachedDecoderPromise = (async (): Promise<DecoderHandle> => {
    // Lazy-load the WASM-backed BarcodeDetector ponyfill on first use.
    const ponyfill = await import("barcode-detector/ponyfill");
    // Ship the wasm binary from our own origin (Vite hashes the asset).
    // Default behaviour would fetch from jsDelivr; we override so the
    // site is self-contained.
    const wasmUrl = (await import("zxing-wasm/reader/zxing_reader.wasm?url"))
      .default;
    ponyfill.prepareZXingModule({
      overrides: {
        locateFile: (path: string, prefix: string) =>
          path.endsWith(".wasm") ? wasmUrl : prefix + path,
      },
    });
    const detector = new ponyfill.BarcodeDetector({
      formats: ["qr_code", "micro_qr_code"],
    });
    const version = ponyfill.ZXING_WASM_VERSION;
    return {
      detect: (src) =>
        detector.detect(src) as Promise<DetectorMatch[]>,
      badge: `QR + Micro QR (zxing-wasm ${version})`,
    };
  })();
  // If the load fails, drop the cache so a retry tries again.
  cachedDecoderPromise.catch(() => {
    cachedDecoderPromise = null;
  });
  return cachedDecoderPromise;
}

// ---------- Camera + decoder loop ----------

// Cap the snapshot canvas long-axis at this many pixels. ZXing slows
// noticeably on full-resolution phone frames (3000+ px); 1280 px is
// plenty for QR/Micro QR detection and keeps detect() snappy.
const SNAPSHOT_MAX_PX = 1280;

async function openScannerWithDecoder(
  decoder: DecoderHandle,
  opts: ScanOptions,
): Promise<ScanResult> {
  const ui = makeOverlay(decoder.badge, !!opts.multi);

  let stream: MediaStream | undefined;
  return new Promise<ScanResult>((resolve, reject) => {
    let resolved = false;
    let raf = 0;
    let mode: "live" | "snapshot" = "live";

    const stopStream = () => {
      stream?.getTracks().forEach((t) => t.stop());
      stream = undefined;
    };
    const finish = (err: Error | null, value?: ScanResult) => {
      if (resolved) return;
      resolved = true;
      cancelAnimationFrame(raf);
      stopStream();
      ui.close();
      if (err) reject(err);
      else if (value) resolve(value);
    };
    ui.cancel.addEventListener("click", () => finish(new Error("scan cancelled")));

    // ----- live polling tick -----
    const tick = async () => {
      if (resolved || mode !== "live") return;
      try {
        const matches = await decoder.detect(ui.video);
        const hit = matches.find((m) => m.rawValue);
        if (hit && !opts.multi) {
          // Legacy single-pick: take the first decode and exit.
          finish(null, {
            payload: hit.rawValue.toUpperCase(),
            format: hit.format,
          });
          return;
        }
        // multi=true: don't auto-pick from live frames — the operator
        // explicitly hits Capture so they get to choose.
      } catch {
        // Decoder occasionally throws on un-decodable / partial frames;
        // keep polling.
      }
      raf = requestAnimationFrame(tick);
    };

    // ----- start / restart live preview -----
    const startLive = () => {
      mode = "live";
      ui.showLive();
      navigator.mediaDevices
        .getUserMedia({ video: { facingMode: "environment" } })
        .then((s) => {
          stream = s;
          ui.video.srcObject = s;
          ui.video.onloadedmetadata = () => {
            void ui.video.play();
            raf = requestAnimationFrame(tick);
          };
        })
        .catch((e) => finish(e as Error));
    };

    // ----- capture: freeze frame, run detect, render polygons -----
    const onCapture = async () => {
      if (mode !== "live") return;
      // Pull current frame dimensions from the video.
      const vw = ui.video.videoWidth;
      const vh = ui.video.videoHeight;
      if (!vw || !vh) return;
      // Scale to a max long edge for decoder speed.
      const longEdge = Math.max(vw, vh);
      const scale = longEdge > SNAPSHOT_MAX_PX ? SNAPSHOT_MAX_PX / longEdge : 1;
      const cw = Math.round(vw * scale);
      const ch = Math.round(vh * scale);
      const canvas = document.createElement("canvas");
      canvas.width = cw;
      canvas.height = ch;
      const cx = canvas.getContext("2d");
      if (!cx) return;
      cx.drawImage(ui.video, 0, 0, cw, ch);
      // Stop the camera now that we've grabbed a frame.
      cancelAnimationFrame(raf);
      stopStream();
      mode = "snapshot";

      // Render the still while we run detect; gives the operator a
      // visible "I'm thinking" frame.
      ui.showSnapshot(canvas);

      let matches: DetectorMatch[] = [];
      try {
        matches = await decoder.detect(canvas);
      } catch {
        matches = [];
      }
      // Render polygons over the still.
      ui.renderSnapshotMatches(canvas, matches, opts.resolveStatus, (m) => {
        finish(null, {
          payload: m.rawValue.toUpperCase(),
          format: m.format,
        });
      });
    };

    if (opts.multi) {
      ui.capture?.addEventListener("click", () => void onCapture());
      ui.retake?.addEventListener("click", () => {
        if (mode !== "snapshot") return;
        startLive();
      });
    }

    startLive();
  });
}

// ---------- Overlay UI ----------

interface OverlayHandle {
  video: HTMLVideoElement;
  badge: HTMLElement;
  cancel: HTMLButtonElement;
  /** Snapshot mode only. */
  capture?: HTMLButtonElement;
  retake?: HTMLButtonElement;
  /** Show live-preview chrome (video, capture button); hide snapshot. */
  showLive: () => void;
  /** Switch to snapshot view, drawing the canvas as the background. */
  showSnapshot: (canvas: HTMLCanvasElement) => void;
  /** Draw tappable polygons over the snapshot. */
  renderSnapshotMatches: (
    canvas: HTMLCanvasElement,
    matches: DetectorMatch[],
    resolveStatus: ((id: string) => ScanStatus) | undefined,
    onPick: (m: DetectorMatch) => void,
  ) => void;
  close: () => void;
}

const SVG_NS = "http://www.w3.org/2000/svg";

function makeOverlay(badgeText: string, multi: boolean): OverlayHandle {
  const overlay = el(
    "div",
    { class: multi ? "scan-overlay scan-overlay--snapshot" : "scan-overlay" },
  );
  const video = el("video", {
    class: "scan-overlay__video",
    playsinline: "",
    autoplay: "",
    muted: "",
  }) as HTMLVideoElement;
  const badge = el("div", { class: "scan-overlay__badge" }, badgeText);

  // Snapshot stage: hidden until Capture is pressed.
  const snapshotWrap = el("div", { class: "scan-overlay__snapshot" });
  const snapshotImgWrap = el("div", { class: "scan-overlay__still" });
  const snapshotEmpty = el(
    "div",
    { class: "scan-overlay__hint" },
    "No QR codes detected. Try Retake.",
  );
  snapshotEmpty.style.display = "none";
  snapshotWrap.append(snapshotImgWrap, snapshotEmpty);
  snapshotWrap.style.display = "none";

  // Action bar — content depends on mode.
  const actions = el("div", { class: "scan-overlay__actions" });
  const cancel = button({ class: "scan-overlay__cancel" }, icon("x"), " Cancel");
  let capture: HTMLButtonElement | undefined;
  let retake: HTMLButtonElement | undefined;
  if (multi) {
    capture = button(
      { class: "scan-overlay__capture primary" },
      icon("camera"),
      " Capture",
    );
    retake = button(
      { class: "scan-overlay__retake" },
      icon("reprint"),
      " Retake",
    );
    retake.style.display = "none";
    actions.append(retake, capture, cancel);
  } else {
    actions.append(cancel);
  }

  overlay.append(video, snapshotWrap, badge, actions);
  document.body.append(overlay);

  const showLive = () => {
    snapshotImgWrap.innerHTML = "";
    snapshotWrap.style.display = "none";
    video.style.display = "";
    if (capture) capture.style.display = "";
    if (retake) retake.style.display = "none";
    snapshotEmpty.style.display = "none";
  };

  const showSnapshot = (canvas: HTMLCanvasElement) => {
    video.style.display = "none";
    snapshotImgWrap.innerHTML = "";
    canvas.classList.add("scan-overlay__still-canvas");
    snapshotImgWrap.append(canvas);
    snapshotWrap.style.display = "";
    if (capture) capture.style.display = "none";
    if (retake) retake.style.display = "";
    snapshotEmpty.style.display = "none";
  };

  const renderSnapshotMatches: OverlayHandle["renderSnapshotMatches"] = (
    canvas,
    matches,
    resolveStatus,
    onPick,
  ) => {
    if (matches.length === 0) {
      snapshotEmpty.style.display = "";
      return;
    }
    // Build SVG over the canvas using the canvas's natural pixel
    // coordinate system as the SVG viewBox. The CSS layer scales the
    // canvas + SVG together so cornerPoints map exactly.
    const svg = document.createElementNS(SVG_NS, "svg");
    svg.setAttribute("class", "scan-overlay__hits");
    svg.setAttribute("viewBox", `0 0 ${canvas.width} ${canvas.height}`);
    svg.setAttribute("preserveAspectRatio", "none");

    for (const m of matches) {
      const canonical = m.rawValue.toUpperCase().replace(/-/g, "");
      const status: ScanStatus = resolveStatus
        ? resolveStatus(canonical)
        : "unbound";
      const points = m.cornerPoints
        .map((p) => `${p.x.toFixed(1)},${p.y.toFixed(1)}`)
        .join(" ");

      const group = document.createElementNS(SVG_NS, "g");
      group.setAttribute("class", `scan-hit scan-hit--${status}`);
      // Tappable: pointer-events hand to the group so the polygon and
      // caption both register clicks.
      group.style.cursor = "pointer";
      group.addEventListener("click", () => onPick(m));

      const poly = document.createElementNS(SVG_NS, "polygon");
      poly.setAttribute("points", points);
      poly.setAttribute("class", "scan-hit__poly");
      group.append(poly);

      // Caption near the top of the polygon: 4-4-4 dashed canonical ID
      // if it matches our 12-char shape; raw value otherwise.
      const caption =
        canonical.length === 12
          ? `${canonical.slice(0, 4)}-${canonical.slice(4, 8)}-${canonical.slice(8, 12)}`
          : canonical || m.rawValue;
      const tx =
        m.cornerPoints.reduce((s, p) => s + p.x, 0) / m.cornerPoints.length;
      const tyTop = Math.min(...m.cornerPoints.map((p) => p.y));
      // Caption goes above the polygon when there's room, otherwise
      // below the centroid.
      const ty = tyTop > canvas.height * 0.1 ? tyTop - 10 : tyTop + 24;
      const text = document.createElementNS(SVG_NS, "text");
      text.setAttribute("x", tx.toFixed(1));
      text.setAttribute("y", ty.toFixed(1));
      text.setAttribute("class", "scan-hit__caption");
      text.setAttribute("text-anchor", "middle");
      text.textContent = caption;
      group.append(text);

      svg.append(group);
    }
    snapshotImgWrap.append(svg);
  };

  return {
    video,
    badge,
    cancel,
    capture,
    retake,
    showLive,
    showSnapshot,
    renderSnapshotMatches,
    close: () => overlay.remove(),
  };
}

function showUnsupported(headline: string, hint: string): void {
  const overlay = el("div", { class: "scan-overlay scan-overlay--error" });
  const card = el("div", { class: "scan-overlay__card" });
  card.append(
    el("h3", { class: "scan-overlay__headline" }, "Scanner unavailable"),
    el("p", {}, headline),
    el("p", { class: "muted small" }, hint),
  );
  const ok = button({ class: "primary" }, icon("x"), " Close");
  ok.addEventListener("click", () => overlay.remove());
  card.append(ok);
  overlay.append(card);
  document.body.append(overlay);
}
