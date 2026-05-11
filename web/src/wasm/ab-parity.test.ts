// A/B parity test: Rust WASM encoder → zxing-wasm decoder.
//
// Closes the foundation-issue-#33 acceptance criterion that the
// Rust-encoded QR matrix round-trips through the same decoder the
// FE actually scans with at runtime (zxing-wasm). The harness runs
// in vitest + jsdom; the WASM bundle is preloaded by
// `src/wasm/test-setup.ts`.
//
// Real-corpus gap (declared up-front, TODO):
// =========================================
// The acceptance criterion calls for an A/B run against a
// "real-world Micro QR scan corpus." No such corpus has been
// captured in-repo yet — Brother QL-820NWBc + iPhone camera test
// images are stored on the bench laptop, not in `/`. This harness
// covers the synthetic round-trip half (encoder → ImageData →
// zxing-wasm) for 6+ canonical IDs across both Standard and Micro
// QR; capturing a printed-scan corpus is a follow-up. File:
// #TODO-corpus (to be opened after PR lands).

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it, beforeAll } from "vitest";
import { prepareZXingModule, readBarcodes } from "zxing-wasm/reader";

import { renderLabel } from "./loader";

const HERE = dirname(fileURLToPath(import.meta.url));
const ZXING_WASM_PATH = resolve(
  HERE,
  "../../node_modules/zxing-wasm/dist/reader/zxing_reader.wasm",
);

// ---------- 6 canonical fixtures (ADR-012 alphabet) ----------

const FIXTURES: ReadonlyArray<{ id: string; micro: boolean }> = [
  { id: "K7M3PQ9RT5VAXY", micro: false },
  { id: "K7M3PQ9RT5VAXY", micro: true },
  { id: "ABCDEFGHJKMNPQ", micro: false },
  { id: "ABCDEFGHJKMNPQ", micro: true },
  { id: "RSTUVWXY234567", micro: false },
  { id: "89BCDFGHJKMNPQ", micro: true },
];

const LAYOUTS = ["vert", "horz", "flag"] as const;
const FORMATS = ["4/4", "4/4/4", "5/5/4"] as const;

// ---------- helpers ----------

/**
 * Parse a vert-layout SVG into a QR pixel buffer. The codec emits
 * `<rect>` modules at uniform spacing in the top half of the SVG;
 * we sample each `<rect>` once to recover the on/off bit, then
 * rasterise to an RGBA ImageData buffer at `MODULE_PX` per module.
 *
 * The decoder works on luminance — RGB white (255) + black (0) is
 * enough. We frame the matrix with a generous quiet zone to make
 * sure zxing's locator doesn't miss the finder pattern.
 */
function svgToImageData(svg: string): ImageData {
  // Pull out the top-half rects: those are the QR (vert layout
  // emits QR at y < size_mm, text at y ≥ size_mm).
  const matches = [
    ...svg.matchAll(
      /<rect x="([\d.]+)" y="([\d.]+)" width="([\d.]+)" height="([\d.]+)" fill="#000"\/>/g,
    ),
  ];
  if (matches.length === 0) throw new Error("svgToImageData: no rects");
  const rects = matches.map((m) => ({
    x: parseFloat(m[1]),
    y: parseFloat(m[2]),
    w: parseFloat(m[3]),
    h: parseFloat(m[4]),
  }));
  // The QR module side is the rect width (uniform).
  const moduleSize = rects[0].w;
  // QR is at (x ∈ [0, size), y ∈ [0, size)). Use the size derived
  // from the largest x|y + moduleSize.
  const qrRects = rects.filter((r) => r.w === moduleSize && r.h === moduleSize);
  const maxX = Math.max(...qrRects.map((r) => r.x));
  const maxY = Math.max(...qrRects.map((r) => r.y));
  // Quiet zone is included in the SVG positions — find min.
  const minX = Math.min(...qrRects.map((r) => r.x));
  const minY = Math.min(...qrRects.map((r) => r.y));
  // Render to a pixel buffer: 1 module → MODULE_PX × MODULE_PX
  // pixels with a 4-module pure-white quiet zone around the whole
  // matrix. The "matrix" here already contains the codec's
  // emitted quiet zone, so we just stamp it onto a slightly larger
  // canvas to be safe with zxing's locator.
  const MODULE_PX = 16;
  const QUIET_PX = MODULE_PX * 4;
  const matrixW = Math.round((maxX - minX) / moduleSize) + 1;
  const matrixH = Math.round((maxY - minY) / moduleSize) + 1;
  const pxW = matrixW * MODULE_PX + 2 * QUIET_PX;
  const pxH = matrixH * MODULE_PX + 2 * QUIET_PX;
  const data = new Uint8ClampedArray(pxW * pxH * 4);
  // Fill white.
  for (let i = 0; i < data.length; i += 4) {
    data[i] = 255;
    data[i + 1] = 255;
    data[i + 2] = 255;
    data[i + 3] = 255;
  }
  // Stamp each black module.
  for (const r of qrRects) {
    const c = Math.round((r.x - minX) / moduleSize);
    const rr = Math.round((r.y - minY) / moduleSize);
    const px0 = QUIET_PX + c * MODULE_PX;
    const py0 = QUIET_PX + rr * MODULE_PX;
    for (let dy = 0; dy < MODULE_PX; dy++) {
      for (let dx = 0; dx < MODULE_PX; dx++) {
        const i = ((py0 + dy) * pxW + (px0 + dx)) * 4;
        data[i] = 0;
        data[i + 1] = 0;
        data[i + 2] = 0;
      }
    }
  }
  // Polyfill ImageData if jsdom doesn't provide it.
  if (typeof ImageData === "undefined") {
    return { data, width: pxW, height: pxH, colorSpace: "srgb" } as ImageData;
  }
  return new ImageData(data, pxW, pxH);
}

// ---------- setup ----------

beforeAll(async () => {
  const bytes = readFileSync(ZXING_WASM_PATH);
  prepareZXingModule({
    overrides: {
      // Hand zxing the bytes directly so it doesn't try to fetch a
      // URL — jsdom + node can't resolve relative `.wasm` URLs.
      wasmBinary: bytes.buffer.slice(
        bytes.byteOffset,
        bytes.byteOffset + bytes.byteLength,
      ),
    },
  });
});

// ---------- the test ----------

describe("Rust WASM encoder ↔ zxing-wasm decoder parity", () => {
  for (const fixture of FIXTURES) {
    it(`round-trips ${fixture.id} (${fixture.micro ? "micro" : "standard"} QR)`, async () => {
      // Vert layout makes the QR easy to locate at (0, 0).
      const svg = await renderLabel(fixture.id, "vert", 24, "4/4", {
        micro: fixture.micro,
      });
      const imageData = svgToImageData(svg);
      const results = await readBarcodes(imageData, {
        formats: fixture.micro ? ["MicroQRCode"] : ["QRCode"],
        tryHarder: true,
      });
      expect(results.length).toBeGreaterThan(0);
      const text = results[0].text;
      expect(text).toBe(fixture.id);
    });
  }

  it("covers all 3 layouts × all 3 formats end-to-end (encoder only)", async () => {
    // Sanity: every (layout, format) combo emits a parseable SVG
    // with rects + text. We don't decode for every combo to keep
    // the suite fast; the per-fixture decode test above covers
    // both Standard + Micro QR variants of vert/4/4. Horz and
    // flag use the same encoder under the hood (qr_block).
    for (const layout of LAYOUTS) {
      for (const format of FORMATS) {
        const svg = await renderLabel("K7M3PQ9RT5VAXY", layout, 11, format);
        expect(svg).toMatch(/^<svg/);
        expect(svg).toContain("fill=\"#000\"");
      }
    }
  });
});
