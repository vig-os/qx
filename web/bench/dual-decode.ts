// Dual-engine decode.
//
// TWO paths, deliberately:
//
//  - LIVE overlay  → the production `barcode-detector` ponyfill on the video
//    element directly (color frames, smooth markup) — the v1 feel.
//  - A/B sampler    → both engines fed BYTE-IDENTICAL grayscale luma from one
//    `ImageData`, so divergence is the DECODER, not a JPEG re-encode or a
//    different input pipeline:
//      · zxing : `readBarcodes(grayImageData)` (zxing-wasm/reader)
//      · rxing : `decode_luma(w, h, luma)`     (Rust, crates/wasm)

import {
  prepareZXingModule as prepareReader,
  readBarcodes,
  ZXING_WASM_VERSION,
  type ReaderOptions,
} from "zxing-wasm/reader";

import initRxingWasm, { decode_luma } from "./wasm/qx_decoder";

export interface Point2D {
  x: number;
  y: number;
}

export interface EngineResult {
  hit: boolean;
  value: string | null;
  ms: number;
}

export interface FrameResult {
  zxing: EngineResult;
  rxing: EngineResult;
  agree: boolean;
  diverge: boolean;
}

const READER_OPTS: ReaderOptions = {
  tryHarder: true,
  formats: ["QRCode", "MicroQRCode", "DataMatrix"],
  maxNumberOfSymbols: 1,
};

interface ZxingDetector {
  detect(src: CanvasImageSource): Promise<
    Array<{ rawValue: string; cornerPoints: Point2D[] }>
  >;
}

let detector: ZxingDetector | null = null; // live (ponyfill)
let ready = false; // A/B (reader + rxing)

export interface InitInfo {
  zxingVersion: string;
}

export async function initEngines(): Promise<InitInfo> {
  const wasmUrl = (await import("zxing-wasm/reader/zxing_reader.wasm?url"))
    .default;
  const ownOrigin = {
    overrides: {
      locateFile: (path: string, prefix: string) =>
        path.endsWith(".wasm") ? wasmUrl : prefix + path,
    },
  };

  // Live decoder: the production scanner's ponyfill (color video frames).
  const ponyfill = await import("barcode-detector/ponyfill");
  ponyfill.prepareZXingModule(ownOrigin);
  detector = new ponyfill.BarcodeDetector({
    formats: ["qr_code", "micro_qr_code", "data_matrix"],
  }) as unknown as ZxingDetector;

  // A/B zxing: the lower-level reader, fed identical luma (own-origin wasm).
  prepareReader(ownOrigin);

  await initRxingWasm();
  ready = true;
  return { zxingVersion: ZXING_WASM_VERSION };
}

/** Fast zxing-only detect for the LIVE overlay — the video element straight
 *  into the production ponyfill, every animation frame. */
export async function liveDetectZxing(
  src: CanvasImageSource,
): Promise<{ value: string; corners: Point2D[] } | null> {
  if (!detector) return null;
  try {
    const matches = await detector.detect(src);
    const hit = matches.find((m) => m.rawValue);
    if (hit) return { value: hit.rawValue, corners: hit.cornerPoints };
  } catch {
    /* partial frame */
  }
  return null;
}

export function normalize(raw: string | null): string | null {
  if (!raw) return null;
  return raw.trim().toUpperCase().replace(/[-\s]/g, "");
}

/** Rec.601 luma from RGBA, one byte per pixel. The SAME buffer goes to both
 *  engines — zxing via a grayscale ImageData, rxing via `decode_luma`. */
function toLuma(img: ImageData): Uint8Array {
  const { data, width, height } = img;
  const luma = new Uint8Array(width * height);
  for (let i = 0, p = 0; i < luma.length; i++, p += 4) {
    luma[i] = (data[p] * 77 + data[p + 1] * 150 + data[p + 2] * 29) >> 8;
  }
  return luma;
}

function lumaToImageData(luma: Uint8Array, w: number, h: number): ImageData {
  const rgba = new Uint8ClampedArray(w * h * 4);
  for (let i = 0, p = 0; i < luma.length; i++, p += 4) {
    rgba[p] = rgba[p + 1] = rgba[p + 2] = luma[i];
    rgba[p + 3] = 255;
  }
  return new ImageData(rgba, w, h);
}

/** Apples-to-apples A/B: both decoders read the identical grayscale image. */
export async function decodeBoth(img: ImageData): Promise<FrameResult> {
  if (!ready) throw new Error("initEngines() not awaited");
  const luma = toLuma(img);
  const gray = lumaToImageData(luma, img.width, img.height);

  const z0 = performance.now();
  let zVal: string | null = null;
  try {
    const res = await readBarcodes(gray, READER_OPTS);
    const hit = res.find((r) => r.text);
    if (hit) zVal = hit.text;
  } catch {
    /* miss */
  }
  const zMs = performance.now() - z0;

  const r0 = performance.now();
  let rVal: string | null = null;
  try {
    rVal = decode_luma(img.width, img.height, luma) ?? null;
  } catch {
    /* miss */
  }
  const rMs = performance.now() - r0;

  const zn = normalize(zVal);
  const rn = normalize(rVal);
  return {
    zxing: { hit: zn !== null, value: zVal, ms: zMs },
    rxing: { hit: rn !== null, value: rVal, ms: rMs },
    agree: zn !== null && zn === rn,
    diverge: (zn === null) !== (rn === null),
  };
}
