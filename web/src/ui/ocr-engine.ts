// Shared OCR engine (#176 P1) — lazy tesseract.js worker, image
// preprocessing, and word-level recognition. Used by both the
// recognize-existing-part scanner (ocr-scan.ts) and the mint-from-label
// extractor (ocr-extract-scan.ts).
//
// Preprocessing (grayscale + contrast stretch) and word-confidence
// capture are the two cheap accuracy wins the #176 review flagged:
// small-font industrial labels OCR poorly raw, and surfacing per-word
// confidence lets the UI dim low-confidence tokens.

export const SNAPSHOT_MAX_PX = 1600; // long-edge ceiling — OCR likes detail

export interface OcrWord {
  text: string;
  /** tesseract per-word confidence, 0–100. */
  confidence: number;
}

export interface OcrResult {
  text: string;
  words: OcrWord[];
}

let workerPromise: Promise<import("tesseract.js").Worker> | null = null;

async function getWorker(): Promise<import("tesseract.js").Worker> {
  if (!workerPromise) {
    workerPromise = (async () => {
      const { createWorker } = await import("tesseract.js");
      return createWorker("eng");
    })();
  }
  return workerPromise;
}

/**
 * Grayscale + linear contrast stretch, in place on the 2D context.
 * Cheap and dependency-free; markedly improves OCR on low-contrast
 * engraved/etched labels. No-op if the context can't be read.
 */
export function preprocessForOcr(canvas: HTMLCanvasElement): void {
  const cx = canvas.getContext("2d", { willReadFrequently: true });
  if (!cx) return;
  let img: ImageData;
  try {
    img = cx.getImageData(0, 0, canvas.width, canvas.height);
  } catch {
    return; // tainted canvas etc. — skip, recognize the raw image
  }
  const d = img.data;

  // Pass 1: grayscale (luma) + track min/max for the stretch.
  let min = 255;
  let max = 0;
  for (let i = 0; i < d.length; i += 4) {
    const g = Math.round(0.299 * d[i] + 0.587 * d[i + 1] + 0.114 * d[i + 2]);
    d[i] = d[i + 1] = d[i + 2] = g;
    if (g < min) min = g;
    if (g > max) max = g;
  }

  // Pass 2: stretch [min,max] → [0,255] (skip if already full-range or flat).
  const range = max - min;
  if (range > 0 && range < 255) {
    const scale = 255 / range;
    for (let i = 0; i < d.length; i += 4) {
      const v = Math.round((d[i] - min) * scale);
      d[i] = d[i + 1] = d[i + 2] = v;
    }
  }
  cx.putImageData(img, 0, 0);
}

/** Minimal shape we read off tesseract's result — the word-level array
 *  moved around between versions, so we read it defensively rather than
 *  binding to the package's exported types. */
interface RawWord { text?: string; confidence?: number }

function extractWords(data: unknown): OcrWord[] {
  const d = data as { words?: RawWord[]; text?: string };
  if (Array.isArray(d.words) && d.words.length > 0) {
    return d.words
      .map((w) => ({ text: (w.text ?? "").trim(), confidence: w.confidence ?? 0 }))
      .filter((w) => w.text.length > 0);
  }
  // Fallback: tokenize the full text. No per-word confidence available,
  // so mark 100 (not dimmed) — the operator still gets assignable chips.
  return (d.text ?? "")
    .split(/\s+/)
    .map((t) => t.trim())
    .filter((t) => t.length > 0)
    .map((text) => ({ text, confidence: 100 }));
}

/**
 * Recognize text from a canvas, returning the full text plus per-word
 * entries with confidence. Runs preprocessing first.
 */
export async function recognize(canvas: HTMLCanvasElement): Promise<OcrResult> {
  preprocessForOcr(canvas);
  const worker = await getWorker();
  const { data } = await worker.recognize(canvas);
  return { text: data.text ?? "", words: extractWords(data) };
}

/**
 * Draw an image file onto a fresh canvas, scaled so its long edge is at
 * most SNAPSHOT_MAX_PX. Returns the canvas, or rejects if the image
 * can't be loaded.
 */
export async function fileToCanvas(file: File): Promise<HTMLCanvasElement> {
  const img = new Image();
  const url = URL.createObjectURL(file);
  try {
    await new Promise<void>((res, rej) => {
      img.onload = () => res();
      img.onerror = () => rej(new Error("Failed to load image"));
      img.src = url;
    });
    const longEdge = Math.max(img.naturalWidth, img.naturalHeight);
    const scale = longEdge > SNAPSHOT_MAX_PX ? SNAPSHOT_MAX_PX / longEdge : 1;
    const cw = Math.round(img.naturalWidth * scale);
    const ch = Math.round(img.naturalHeight * scale);
    const canvas = document.createElement("canvas");
    canvas.width = cw;
    canvas.height = ch;
    const cx = canvas.getContext("2d");
    if (cx) cx.drawImage(img, 0, 0, cw, ch);
    return canvas;
  } finally {
    URL.revokeObjectURL(url);
  }
}
