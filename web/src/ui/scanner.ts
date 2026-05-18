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

// ---- Pure ID helpers (exported for testing) ----

/** Strip dashes and uppercase a raw QR payload to produce the
 *  canonical registry ID. Idempotent. */
export function canonicalizeId(raw: string): string {
  return raw.toUpperCase().replace(/-/g, "");
}

/** Format a canonical ID for human display: groups of 4 separated by
 *  dashes (e.g. "K7M3-PQ9R-T5VA-XY"). Short IDs (< 12 chars) are
 *  returned as-is. */
export function formatIdDashed(canonical: string): string {
  if (canonical.length < 12) return canonical;
  return `${canonical.slice(0, 4)}-${canonical.slice(4, 8)}-${canonical.slice(8, 12)}${canonical.length > 12 ? "-" + canonical.slice(12) : ""}`;
}

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

/** Open the scanner in multi-pick mode. The operator captures a
 *  snapshot, taps polygons to select/deselect, and hits "Done" to
 *  commit the selection. Returns canonical IDs of all selected codes.
 *
 *  Throws `ScanUnsupportedError` if camera / WASM unavailable.
 *  Throws a generic Error if the user cancels. */
export async function openScannerMulti(
  opts: Omit<ScanOptions, "multi"> = {},
): Promise<string[]> {
  const decoder = await ensureDecoder();
  return openScannerMultiPick(decoder, { ...opts, multi: true });
}

export async function openScannerWithDetail(
  opts: ScanOptions = {},
): Promise<ScanResult> {
  const decoder = await ensureDecoder();
  return openScannerWithDecoder(decoder, opts);
}

async function ensureDecoder(): Promise<DecoderHandle> {
  if (typeof navigator === "undefined" || !navigator.mediaDevices?.getUserMedia) {
    showUnsupported(
      "This browser doesn't expose a camera API.",
      "getUserMedia is required for QR scanning. Try a recent Chrome, Firefox, Safari, or Edge.",
    );
    throw new ScanUnsupportedError("getUserMedia unavailable");
  }
  return ensureDecoderOnly();
}

/** Load the decoder without requiring camera access (for image upload). */
async function ensureDecoderOnly(): Promise<DecoderHandle> {
  try {
    return await loadDecoder();
  } catch (e) {
    const msg = (e as Error)?.message ?? String(e);
    showUnsupported("The QR decoder failed to load.", `Detail: ${msg}`);
    throw new ScanUnsupportedError("decoder load failed: " + msg);
  }
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
            payload: canonicalizeId(hit.rawValue),
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
          payload: canonicalizeId(m.rawValue),
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

// ---------- Multi-pick scanner ----------
//
// Same capture-snapshot UX, but tapping a polygon toggles selection
// instead of immediately resolving. A chip tray shows selected IDs,
// each removable. "Done" commits the selection.

async function openScannerMultiPick(
  decoder: DecoderHandle,
  opts: ScanOptions,
): Promise<string[]> {
  const ui = makeOverlay(decoder.badge, true);

  // Selection state: canonical ID -> DetectorMatch (for re-rendering).
  const selected = new Map<string, DetectorMatch>();

  // Chip tray: sits between the snapshot and the action bar.
  const chipTray = el("div", { class: "scan-overlay__chips" });
  // Insert before actions bar.
  ui.chipsSlot.append(chipTray);

  // Done button — disabled until at least one code is selected.
  const doneBtn = button({ class: "scan-overlay__done primary" }, icon("plus"), " Done");
  doneBtn.disabled = true;
  ui.actionsSlot.prepend(doneBtn);

  const renderChips = () => {
    chipTray.innerHTML = "";
    for (const [id, _m] of selected) {
      const status: ScanStatus = opts.resolveStatus
        ? opts.resolveStatus(id)
        : "unbound";
      const chip = el("span", { class: `scan-chip scan-chip--${status}` });
      const label = formatIdDashed(id);
      chip.append(document.createTextNode(label));
      const removeBtn = button({ class: "scan-chip__remove", title: "Remove" }, icon("x", { size: 12 }));
      removeBtn.addEventListener("click", (e) => {
        e.stopPropagation();
        selected.delete(id);
        renderChips();
        // Re-render polygon selection state.
        updatePolygonSelection();
      });
      chip.append(removeBtn);
      chipTray.append(chip);
    }
    doneBtn.disabled = selected.size === 0;
  };

  // Track all polygon groups so we can toggle their selected class.
  let polygonGroups: Array<{ canonical: string; group: SVGGElement }> = [];

  const updatePolygonSelection = () => {
    for (const pg of polygonGroups) {
      if (selected.has(pg.canonical)) {
        pg.group.classList.add("scan-hit--selected");
      } else {
        pg.group.classList.remove("scan-hit--selected");
      }
    }
  };

  let stream: MediaStream | undefined;
  return new Promise<string[]>((resolve, reject) => {
    let resolved = false;
    let raf = 0;
    let mode: "live" | "snapshot" = "live";

    const stopStream = () => {
      stream?.getTracks().forEach((t) => t.stop());
      stream = undefined;
    };
    const finish = (err: Error | null, value?: string[]) => {
      if (resolved) return;
      resolved = true;
      cancelAnimationFrame(raf);
      stopStream();
      ui.close();
      if (err) reject(err);
      else resolve(value ?? []);
    };

    ui.cancel.addEventListener("click", () => finish(new Error("scan cancelled")));
    doneBtn.addEventListener("click", () => {
      finish(null, [...selected.keys()]);
    });

    const tick = async () => {
      if (resolved || mode !== "live") return;
      try {
        // In multi-pick live mode, just keep the stream running — no
        // auto-pick. The operator explicitly captures.
        await decoder.detect(ui.video);
      } catch {
        // ignore
      }
      raf = requestAnimationFrame(tick);
    };

    const startLive = () => {
      mode = "live";
      ui.showLive();
      polygonGroups = [];
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

    const onCapture = async () => {
      if (mode !== "live") return;
      const vw = ui.video.videoWidth;
      const vh = ui.video.videoHeight;
      if (!vw || !vh) return;
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
      cancelAnimationFrame(raf);
      stopStream();
      mode = "snapshot";
      ui.showSnapshot(canvas);

      let matches: DetectorMatch[] = [];
      try {
        matches = await decoder.detect(canvas);
      } catch {
        matches = [];
      }

      // Render polygons — tap toggles selection instead of resolving.
      polygonGroups = [];
      ui.renderSnapshotMatches(canvas, matches, opts.resolveStatus, (m) => {
        const canonical = canonicalizeId(m.rawValue);
        if (selected.has(canonical)) {
          selected.delete(canonical);
        } else {
          selected.set(canonical, m);
        }
        renderChips();
        updatePolygonSelection();
      });

      // Build the polygon group index for selection state tracking.
      // The SVG groups were just appended by renderSnapshotMatches;
      // query them from the DOM.
      const svg = ui.snapshotStill.querySelector(".scan-overlay__hits");
      if (svg) {
        const groups = svg.querySelectorAll(".scan-hit");
        let i = 0;
        for (const m of matches) {
          const canonical = canonicalizeId(m.rawValue);
          const g = groups[i] as SVGGElement | undefined;
          if (g) {
            polygonGroups.push({ canonical, group: g });
          }
          i++;
        }
      }
      // Apply initial selection state (IDs may have been selected from
      // a previous capture in the same session).
      updatePolygonSelection();
    };

    ui.capture?.addEventListener("click", () => void onCapture());
    ui.retake?.addEventListener("click", () => {
      if (mode !== "snapshot") return;
      startLive();
    });

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
  /** Container where multi-pick chips are inserted. */
  chipsSlot: HTMLElement;
  /** Container for action buttons (Done can be prepended here). */
  actionsSlot: HTMLElement;
  /** The still-image wrapper (for querying SVG groups). */
  snapshotStill: HTMLElement;
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

  // Chips slot: sits between the snapshot and the action bar in
  // multi-pick mode. The multi-pick caller inserts its chip tray here.
  const chipsSlot = el("div", { class: "scan-overlay__chips-slot" });

  overlay.append(video, snapshotWrap, chipsSlot, badge, actions);
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
      const canonical = canonicalizeId(m.rawValue);
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
      // if it matches our ID shape; raw value otherwise.
      const caption = formatIdDashed(canonical) || m.rawValue;
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
    chipsSlot,
    actionsSlot: actions,
    snapshotStill: snapshotImgWrap,
    showLive,
    showSnapshot,
    renderSnapshotMatches,
    close: () => overlay.remove(),
  };
}

// ---------- Image upload batch scan (#99) ----------
//
// Opens a modal with a drag-and-drop / file-input zone. The operator
// uploads a photo, we run detect() on it, render polygons over the
// decoded QR codes (green = decoded, red = failed), and let the
// operator select which IDs to commit. Returns canonical IDs.

export interface ImageScanOptions {
  resolveStatus?: (canonical: string) => ScanStatus;
}

export async function openImageScan(
  opts: ImageScanOptions = {},
): Promise<string[]> {
  const decoder = await ensureDecoderOnly();
  return openImageScanWithDecoder(decoder, opts);
}

async function openImageScanWithDecoder(
  decoder: DecoderHandle,
  opts: ImageScanOptions,
): Promise<string[]> {
  return new Promise<string[]>((resolve, reject) => {
    let resolved = false;

    // Selection state.
    const selected = new Map<string, DetectorMatch>();

    // Build the overlay.
    const overlay = el(
      "div",
      { class: "scan-overlay scan-overlay--image" },
    );

    // Drop zone (shown initially).
    const dropZone = el("div", { class: "image-scan__dropzone" });
    const dropLabel = el("p", { class: "image-scan__drop-label" }, "Drop a photo here");
    const dropHint = el("p", { class: "muted small" }, "or click below to choose a file");
    const fileInput = document.createElement("input");
    fileInput.type = "file";
    fileInput.accept = "image/*";
    fileInput.style.display = "none";
    const chooseBtn = button({ class: "primary" }, icon("upload"), " Choose image");
    chooseBtn.addEventListener("click", () => fileInput.click());
    dropZone.append(dropLabel, dropHint, chooseBtn, fileInput);

    // Image display area (hidden until an image is loaded).
    const imageWrap = el("div", { class: "image-scan__preview" });
    const stillWrap = el("div", { class: "scan-overlay__still" });
    const noCodesHint = el(
      "div",
      { class: "scan-overlay__hint" },
      "No QR codes detected in this image.",
    );
    noCodesHint.style.display = "none";
    imageWrap.append(stillWrap, noCodesHint);
    imageWrap.style.display = "none";

    // Chip tray.
    const chipTray = el("div", { class: "scan-overlay__chips" });

    // Action bar.
    const actions = el("div", { class: "scan-overlay__actions" });
    const cancelBtn = button({ class: "scan-overlay__cancel" }, icon("x"), " Cancel");
    const doneBtn = button({ class: "scan-overlay__done primary" }, icon("plus"), " Add all to queue");
    doneBtn.disabled = true;
    const retakeBtn = button({}, icon("upload"), " Choose another");
    retakeBtn.style.display = "none";
    // Manual ID entry.
    const manualBtn = button({}, icon("edit"), " Add unrecognized");
    manualBtn.style.display = "none";
    actions.append(doneBtn, manualBtn, retakeBtn, cancelBtn);

    // Badge.
    const badge = el("div", { class: "scan-overlay__badge" }, decoder.badge);

    overlay.append(dropZone, imageWrap, el("div", { class: "scan-overlay__chips-slot" }, chipTray), badge, actions);
    document.body.append(overlay);

    const finish = (err: Error | null, value?: string[]) => {
      if (resolved) return;
      resolved = true;
      overlay.remove();
      if (err) reject(err);
      else resolve(value ?? []);
    };

    cancelBtn.addEventListener("click", () => finish(new Error("scan cancelled")));
    doneBtn.addEventListener("click", () => finish(null, [...selected.keys()]));

    // Track polygon groups for selection toggling.
    let polygonGroups: Array<{ canonical: string; group: SVGGElement }> = [];

    const updatePolygonSelection = () => {
      for (const pg of polygonGroups) {
        pg.group.classList.toggle("scan-hit--selected", selected.has(pg.canonical));
      }
    };

    const renderChips = () => {
      chipTray.innerHTML = "";
      for (const [id] of selected) {
        const status: ScanStatus = opts.resolveStatus
          ? opts.resolveStatus(id)
          : "unbound";
        const chip = el("span", { class: `scan-chip scan-chip--${status}` });
        chip.append(document.createTextNode(formatIdDashed(id)));
        const removeBtn = button(
          { class: "scan-chip__remove", title: "Remove" },
          icon("x", { size: 12 }),
        );
        removeBtn.addEventListener("click", (e) => {
          e.stopPropagation();
          selected.delete(id);
          renderChips();
          updatePolygonSelection();
        });
        chip.append(removeBtn);
        chipTray.append(chip);
      }
      doneBtn.disabled = selected.size === 0;
      doneBtn.textContent = "";
      doneBtn.append(
        icon("plus"),
        ` Add ${selected.size || "all"} to queue`,
      );
    };

    // Manual entry button.
    manualBtn.addEventListener("click", () => {
      const raw = prompt("Enter an ID manually:");
      if (!raw) return;
      const canonical = canonicalizeId(raw);
      if (!canonical) return;
      selected.set(canonical, { rawValue: raw, format: "manual", cornerPoints: [] });
      renderChips();
    });

    // Process an image file.
    const processFile = async (file: File) => {
      const img = new Image();
      const url = URL.createObjectURL(file);
      img.src = url;
      await new Promise<void>((res, rej) => {
        img.onload = () => res();
        img.onerror = () => rej(new Error("Failed to load image"));
      });

      // Draw to canvas (scale down for decoder speed).
      const longEdge = Math.max(img.naturalWidth, img.naturalHeight);
      const scale = longEdge > SNAPSHOT_MAX_PX ? SNAPSHOT_MAX_PX / longEdge : 1;
      const cw = Math.round(img.naturalWidth * scale);
      const ch = Math.round(img.naturalHeight * scale);
      const canvas = document.createElement("canvas");
      canvas.width = cw;
      canvas.height = ch;
      const cx = canvas.getContext("2d");
      if (!cx) return;
      cx.drawImage(img, 0, 0, cw, ch);
      URL.revokeObjectURL(url);

      // Switch from drop zone to preview.
      dropZone.style.display = "none";
      imageWrap.style.display = "";
      retakeBtn.style.display = "";
      manualBtn.style.display = "";

      // Show the still while decoding.
      stillWrap.innerHTML = "";
      canvas.classList.add("scan-overlay__still-canvas");
      stillWrap.append(canvas);

      let matches: DetectorMatch[] = [];
      try {
        matches = await decoder.detect(canvas);
      } catch {
        matches = [];
      }

      if (matches.length === 0) {
        noCodesHint.style.display = "";
      } else {
        noCodesHint.style.display = "none";
      }

      // Auto-select all decoded IDs.
      for (const m of matches) {
        if (m.rawValue) {
          const canonical = canonicalizeId(m.rawValue);
          selected.set(canonical, m);
        }
      }
      renderChips();

      // Render SVG polygons.
      polygonGroups = [];
      if (matches.length > 0) {
        const svg = document.createElementNS(SVG_NS, "svg");
        svg.setAttribute("class", "scan-overlay__hits");
        svg.setAttribute("viewBox", `0 0 ${canvas.width} ${canvas.height}`);
        svg.setAttribute("preserveAspectRatio", "none");

        for (const m of matches) {
          const canonical = m.rawValue ? canonicalizeId(m.rawValue) : "";
          const decoded = !!m.rawValue;
          const status: ScanStatus = decoded
            ? (opts.resolveStatus ? opts.resolveStatus(canonical) : "unbound")
            : "unknown";
          const points = m.cornerPoints
            .map((p) => `${p.x.toFixed(1)},${p.y.toFixed(1)}`)
            .join(" ");

          const group = document.createElementNS(SVG_NS, "g");
          group.setAttribute("class", `scan-hit scan-hit--${status}`);
          group.style.cursor = "pointer";

          if (decoded) {
            group.addEventListener("click", () => {
              if (selected.has(canonical)) {
                selected.delete(canonical);
              } else {
                selected.set(canonical, m);
              }
              renderChips();
              updatePolygonSelection();
            });
          }

          if (points) {
            const poly = document.createElementNS(SVG_NS, "polygon");
            poly.setAttribute("points", points);
            poly.setAttribute("class", "scan-hit__poly");
            group.append(poly);
          }

          const caption = decoded
            ? formatIdDashed(canonical) || m.rawValue
            : "???";
          if (m.cornerPoints.length > 0) {
            const tx =
              m.cornerPoints.reduce((s, p) => s + p.x, 0) / m.cornerPoints.length;
            const tyTop = Math.min(...m.cornerPoints.map((p) => p.y));
            const ty = tyTop > canvas.height * 0.1 ? tyTop - 10 : tyTop + 24;
            const text = document.createElementNS(SVG_NS, "text");
            text.setAttribute("x", tx.toFixed(1));
            text.setAttribute("y", ty.toFixed(1));
            text.setAttribute("class", "scan-hit__caption");
            text.setAttribute("text-anchor", "middle");
            text.textContent = caption;
            group.append(text);
          }

          svg.append(group);
          if (decoded) {
            polygonGroups.push({ canonical, group });
          }
        }
        stillWrap.append(svg);
      }
      updatePolygonSelection();
    };

    // File input handler.
    fileInput.addEventListener("change", () => {
      const file = fileInput.files?.[0];
      if (file) void processFile(file);
    });

    // Retake: reset to drop zone.
    retakeBtn.addEventListener("click", () => {
      imageWrap.style.display = "none";
      dropZone.style.display = "";
      retakeBtn.style.display = "none";
      manualBtn.style.display = "none";
      stillWrap.innerHTML = "";
      noCodesHint.style.display = "none";
      selected.clear();
      polygonGroups = [];
      renderChips();
      fileInput.value = "";
    });

    // Drag and drop.
    dropZone.addEventListener("dragover", (e) => {
      e.preventDefault();
      dropZone.classList.add("image-scan__dropzone--active");
    });
    dropZone.addEventListener("dragleave", () => {
      dropZone.classList.remove("image-scan__dropzone--active");
    });
    dropZone.addEventListener("drop", (e) => {
      e.preventDefault();
      dropZone.classList.remove("image-scan__dropzone--active");
      const file = e.dataTransfer?.files[0];
      if (file && file.type.startsWith("image/")) {
        void processFile(file);
      }
    });
  });
}

// ---------- Continuous rolling scanner (#100) ----------
//
// Like the existing scanner but accumulates all detected IDs instead
// of resolving on first hit. Shows a running chip tray and a count
// badge. "Done" commits the accumulated set.

export interface RollingScanOptions {
  resolveStatus?: (canonical: string) => ScanStatus;
}

export async function openScannerRolling(
  opts: RollingScanOptions = {},
): Promise<string[]> {
  const decoder = await ensureDecoder();
  return openScannerRollingWithDecoder(decoder, opts);
}

/** Duration (ms) for the "newly detected" flash highlight. */
const ROLLING_FLASH_MS = 800;

async function openScannerRollingWithDecoder(
  decoder: DecoderHandle,
  opts: RollingScanOptions,
): Promise<string[]> {
  const ui = makeOverlay(decoder.badge, false);
  // Repurpose the overlay for rolling mode.
  (ui.video.parentElement as HTMLElement)?.classList.add("scan-overlay--rolling");

  // Accumulated IDs.
  const accumulated = new Map<string, DetectorMatch>();
  // Recently-flashed IDs (for blue highlight).
  const recentFlash = new Set<string>();

  // Chip tray.
  const chipTray = el("div", { class: "scan-overlay__chips" });
  ui.chipsSlot.append(chipTray);

  // Count badge.
  const countBadge = el("div", { class: "rolling-scan__count" }, "0 scanned");
  ui.chipsSlot.append(countBadge);

  // Done button.
  const doneBtn = button({ class: "scan-overlay__done primary" }, icon("check"), " Done (0)");
  doneBtn.disabled = true;
  ui.actionsSlot.prepend(doneBtn);

  const renderChips = () => {
    chipTray.innerHTML = "";
    for (const [id] of accumulated) {
      const status: ScanStatus = opts.resolveStatus
        ? opts.resolveStatus(id)
        : "unbound";
      const isRecent = recentFlash.has(id);
      const chip = el("span", {
        class: `scan-chip scan-chip--${isRecent ? "queued" : status}${isRecent ? " scan-chip--flash" : ""}`,
      });
      chip.append(document.createTextNode(formatIdDashed(id)));
      const removeBtn = button(
        { class: "scan-chip__remove", title: "Remove" },
        icon("x", { size: 12 }),
      );
      removeBtn.addEventListener("click", (e) => {
        e.stopPropagation();
        accumulated.delete(id);
        renderChips();
      });
      chip.append(removeBtn);
      chipTray.append(chip);
    }
    countBadge.textContent = `${accumulated.size} scanned`;
    doneBtn.disabled = accumulated.size === 0;
    doneBtn.textContent = "";
    doneBtn.append(icon("check"), ` Done (${accumulated.size})`);
  };

  let stream: MediaStream | undefined;
  return new Promise<string[]>((resolve, reject) => {
    let resolved = false;
    let raf = 0;

    const stopStream = () => {
      stream?.getTracks().forEach((t) => t.stop());
      stream = undefined;
    };
    const finish = (err: Error | null, value?: string[]) => {
      if (resolved) return;
      resolved = true;
      cancelAnimationFrame(raf);
      stopStream();
      ui.close();
      if (err) reject(err);
      else resolve(value ?? []);
    };

    ui.cancel.addEventListener("click", () => finish(new Error("scan cancelled")));
    doneBtn.addEventListener("click", () => finish(null, [...accumulated.keys()]));

    // Continuous polling tick — accumulate instead of resolve.
    const tick = async () => {
      if (resolved) return;
      try {
        const matches = await decoder.detect(ui.video);
        let changed = false;
        for (const m of matches) {
          if (!m.rawValue) continue;
          const canonical = canonicalizeId(m.rawValue);
          if (accumulated.has(canonical)) continue;
          // New detection!
          accumulated.set(canonical, m);
          changed = true;
          // Flash highlight.
          recentFlash.add(canonical);
          setTimeout(() => {
            recentFlash.delete(canonical);
            if (!resolved) renderChips();
          }, ROLLING_FLASH_MS);
        }
        if (changed) renderChips();
      } catch {
        // Decoder throws on partial frames; keep polling.
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
