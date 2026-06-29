// Dual-engine decode: feed ONE captured frame to both decoders and time
// each. This is the honest rxing-vs-zxing A/B — same pixels, same moment.
//
//  - zxing-wasm  : via the `barcode-detector` ponyfill (the production
//                  scanner's decoder), `detect(canvas)` → cornerPoints.
//  - rxing (Rust): the `decode_image` wasm facade (crates/wasm, built with
//                  --features decoder), fed the frame as JPEG bytes.

import initRxingWasm, { decode_image } from "./wasm/qx_decoder";

export interface Point2D {
  x: number;
  y: number;
}

export interface EngineResult {
  hit: boolean;
  value: string | null;
  ms: number;
  /** zxing-only: detected quad for the overlay. */
  corners?: Point2D[];
}

export interface FrameResult {
  zxing: EngineResult;
  rxing: EngineResult;
  /** Both decoded the same payload. */
  agree: boolean;
  /** Exactly one engine decoded (the interesting divergence). */
  diverge: boolean;
}

interface ZxingDetector {
  detect(src: CanvasImageSource): Promise<
    Array<{ rawValue: string; cornerPoints: Point2D[] }>
  >;
}

let detector: ZxingDetector | null = null;
let rxingReady = false;

export interface InitInfo {
  zxingVersion: string;
}

/** Load both decoders. zxing from the production polyfill path (own-origin
 *  wasm), rxing from the bench-only decoder bundle. */
export async function initEngines(): Promise<InitInfo> {
  const ponyfill = await import("barcode-detector/ponyfill");
  const wasmUrl = (await import("zxing-wasm/reader/zxing_reader.wasm?url"))
    .default;
  ponyfill.prepareZXingModule({
    overrides: {
      locateFile: (path: string, prefix: string) =>
        path.endsWith(".wasm") ? wasmUrl : prefix + path,
    },
  });
  detector = new ponyfill.BarcodeDetector({
    formats: ["qr_code", "micro_qr_code", "data_matrix"],
  }) as unknown as ZxingDetector;

  await initRxingWasm();
  rxingReady = true;

  return { zxingVersion: ponyfill.ZXING_WASM_VERSION };
}

/** Normalise a decoded payload so the two engines are compared on the same
 *  footing (trim, uppercase, strip separators — matches the production
 *  `canonicalizeId` shape closely enough for agreement scoring). */
export function normalize(raw: string | null): string | null {
  if (!raw) return null;
  return raw.trim().toUpperCase().replace(/[-\s]/g, "");
}

function canvasToJpegBytes(canvas: HTMLCanvasElement): Promise<Uint8Array> {
  return new Promise((resolve, reject) => {
    canvas.toBlob(
      (blob) => {
        if (!blob) return reject(new Error("toBlob failed"));
        blob.arrayBuffer().then((b) => resolve(new Uint8Array(b)), reject);
      },
      "image/jpeg",
      0.92,
    );
  });
}

/** Decode one frame through both engines. Caller passes a canvas already
 *  painted with the frame so both see identical pixels. */
export async function decodeBoth(canvas: HTMLCanvasElement): Promise<FrameResult> {
  if (!detector || !rxingReady) throw new Error("initEngines() not awaited");

  // --- zxing ---
  const z0 = performance.now();
  let zVal: string | null = null;
  let zCorners: Point2D[] | undefined;
  try {
    const matches = await detector.detect(canvas);
    const hit = matches.find((m) => m.rawValue);
    if (hit) {
      zVal = hit.rawValue;
      zCorners = hit.cornerPoints;
    }
  } catch {
    // partial / undecodable frame — counts as a miss
  }
  const zMs = performance.now() - z0;

  // --- rxing (same frame, as JPEG bytes) ---
  const r0 = performance.now();
  let rVal: string | null = null;
  try {
    const bytes = await canvasToJpegBytes(canvas);
    rVal = decode_image(bytes) ?? null;
  } catch {
    // decode error — miss
  }
  const rMs = performance.now() - r0;

  const zn = normalize(zVal);
  const rn = normalize(rVal);
  const agree = zn !== null && zn === rn;
  const diverge = (zn === null) !== (rn === null);

  return {
    zxing: { hit: zn !== null, value: zVal, ms: zMs, corners: zCorners },
    rxing: { hit: rn !== null, value: rVal, ms: rMs },
    agree,
    diverge,
  };
}
