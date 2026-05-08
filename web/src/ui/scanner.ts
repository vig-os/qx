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

import { el, button } from "./dom";
import { icon } from "./icons";

interface ScanResult {
  payload: string;
  format: string;
}

interface DecoderHandle {
  detect(source: CanvasImageSource): Promise<{ rawValue: string; format: string }[]>;
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
export async function openScanner(): Promise<string> {
  const result = await openScannerWithDetail();
  return result.payload;
}

export async function openScannerWithDetail(): Promise<ScanResult> {
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

  return openScannerWithDecoder(decoder);
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
      detect: (src) => detector.detect(src) as Promise<{ rawValue: string; format: string }[]>,
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

async function openScannerWithDecoder(decoder: DecoderHandle): Promise<ScanResult> {
  const ui = makeOverlay(decoder.badge);

  let stream: MediaStream | undefined;
  return new Promise<ScanResult>((resolve, reject) => {
    let resolved = false;
    let raf = 0;
    const finish = (err: Error | null, value?: ScanResult) => {
      if (resolved) return;
      resolved = true;
      cancelAnimationFrame(raf);
      stream?.getTracks().forEach((t) => t.stop());
      ui.close();
      if (err) reject(err);
      else if (value) resolve(value);
    };
    ui.cancel.addEventListener("click", () => finish(new Error("scan cancelled")));

    const tick = async () => {
      if (resolved) return;
      try {
        const matches = await decoder.detect(ui.video);
        const hit = matches.find((m) => m.rawValue);
        if (hit) {
          finish(null, {
            payload: hit.rawValue.toUpperCase(),
            format: hit.format,
          });
          return;
        }
      } catch {
        // Decoder occasionally throws on un-decodable / partial frames;
        // keep polling.
      }
      raf = requestAnimationFrame(tick);
    };

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
  });
}

// ---------- Overlay UI ----------

interface OverlayHandle {
  video: HTMLVideoElement;
  badge: HTMLElement;
  cancel: HTMLButtonElement;
  close: () => void;
}

function makeOverlay(badgeText: string): OverlayHandle {
  const overlay = el("div", { class: "scan-overlay" });
  const video = el("video", {
    class: "scan-overlay__video",
    playsinline: "",
    autoplay: "",
    muted: "",
  }) as HTMLVideoElement;
  const badge = el("div", { class: "scan-overlay__badge" }, badgeText);
  const cancel = button({ class: "scan-overlay__cancel" }, icon("x"), " Cancel");
  overlay.append(video, badge, cancel);
  document.body.append(overlay);
  return {
    video,
    badge,
    cancel,
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
