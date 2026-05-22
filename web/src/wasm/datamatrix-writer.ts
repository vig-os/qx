// DataMatrix encoder via zxing-wasm's writer module.
//
// Lazy-loaded on first use so the writer WASM binary (~631 KB) doesn't
// penalise cold page loads. The writer is separate from the reader
// WASM — both come from the same zxing-wasm package.
//
// The zxing-wasm writer API is async (WASM init), so we provide:
//   - `renderDataMatrixAsync(id, sizeMm, opts)` — async, returns SVG
//   - `renderDataMatrixSync(id, sizeMm, opts)` — sync, returns cached
//     SVG or a placeholder. Call `preRender(id)` first.
//   - `preloadWriter()` — fire-and-forget init during app boot

type WriterModule = typeof import("zxing-wasm/writer");
let writerPromise: Promise<WriterModule> | null = null;
let writerModule: WriterModule | null = null;

async function ensureWriter(): Promise<WriterModule> {
  if (writerModule) return writerModule;
  if (writerPromise) return writerPromise;
  writerPromise = (async () => {
    const writer = await import("zxing-wasm/writer");
    const wasmUrl = (
      await import("zxing-wasm/writer/zxing_writer.wasm?url")
    ).default;
    writer.prepareZXingModule({
      overrides: {
        locateFile: (path: string, prefix: string) =>
          path.endsWith(".wasm") ? wasmUrl : prefix + path,
      },
    });
    // Warm up the module by writing a dummy barcode
    await writer.writeBarcode("TEST", { format: "DataMatrix", scale: 1 });
    writerModule = writer;
    return writer;
  })();
  writerPromise.catch(() => {
    writerPromise = null;
  });
  return writerPromise;
}

// ---- SVG cache for synchronous access ----

const svgCache = new Map<string, string>();

function cacheKey(id: string, sizeMm: number, showText: boolean): string {
  return `${id}:${sizeMm}:${showText ? "t" : "n"}`;
}

/**
 * Render a DataMatrix label SVG (async). Caches the result for sync access.
 */
export async function renderDataMatrixAsync(
  canonical: string,
  sizeMm: number,
  showText: boolean,
): Promise<string> {
  const key = cacheKey(canonical, sizeMm, showText);
  const cached = svgCache.get(key);
  if (cached) return cached;

  const writer = await ensureWriter();
  const result = await writer.writeBarcode(canonical, {
    format: "DataMatrix",
    scale: 1,
    addQuietZones: true,
    addHRT: false,
  });

  if (result.error) {
    throw new Error(`DataMatrix encode failed: ${result.error}`);
  }

  const svg = buildLabelSvg(result.svg, canonical, sizeMm, showText);
  svgCache.set(key, svg);
  return svg;
}

/**
 * Synchronous DataMatrix label SVG. Returns cached result or a
 * placeholder. The caller should call `renderDataMatrixAsync` first
 * (e.g. in the live preview debounce) to populate the cache.
 */
export function renderDataMatrixSync(
  canonical: string,
  sizeMm: number,
  showText: boolean,
): string {
  const key = cacheKey(canonical, sizeMm, showText);
  return svgCache.get(key) ?? placeholderSvg(sizeMm, showText);
}

/** Pre-load the writer WASM (fire-and-forget). */
export function preloadWriter(): void {
  void ensureWriter();
}

/** Whether the writer WASM is fully loaded. */
export function isWriterReady(): boolean {
  return writerModule !== null;
}

/** Invalidate cache (e.g. when settings change). */
export function clearCache(): void {
  svgCache.clear();
}

// ---- SVG composition ----

function buildLabelSvg(
  rawSvg: string,
  canonical: string,
  sizeMm: number,
  showText: boolean,
): string {
  // Parse the viewBox from zxing-wasm's SVG output
  const viewBoxMatch = rawSvg.match(/viewBox="([^"]+)"/);
  const innerContent = rawSvg
    .replace(/<\?xml[^?]*\?>/, "")
    .replace(/<svg[^>]*>/, "")
    .replace(/<\/svg>/, "");

  let svgW = 20;
  let svgH = 20;
  if (viewBoxMatch) {
    const parts = viewBoxMatch[1].split(/\s+/).map(Number);
    svgW = parts[2] ?? 20;
    svgH = parts[3] ?? 20;
  }

  if (!showText) {
    // Code only — square label
    return [
      `<svg xmlns="http://www.w3.org/2000/svg"`,
      `  width="${sizeMm}mm" height="${sizeMm}mm"`,
      `  viewBox="0 0 ${svgW} ${svgH}">`,
      innerContent,
      `</svg>`,
    ].join("");
  }

  // Horizontal layout: DataMatrix left, text right (2:1 aspect)
  const textRows = formatIdRows(canonical, sizeMm);
  const totalW = svgW * 2;
  const fontSize = svgH / (textRows.length + 1);
  const textX = svgW * 1.1;

  const textSvg = textRows
    .map((row, i) => {
      const y = (svgH / (textRows.length + 1)) * (i + 1) + fontSize * 0.35;
      return `<text x="${textX}" y="${y}" font-family="Consolas, monospace" font-weight="bold" font-size="${fontSize}" fill="#000">${row}</text>`;
    })
    .join("");

  return [
    `<svg xmlns="http://www.w3.org/2000/svg"`,
    `  width="${sizeMm * 2}mm" height="${sizeMm}mm"`,
    `  viewBox="0 0 ${totalW} ${svgH}">`,
    innerContent,
    textSvg,
    `</svg>`,
  ].join("");
}

function placeholderSvg(sizeMm: number, showText: boolean): string {
  const w = showText ? sizeMm * 2 : sizeMm;
  return [
    `<svg xmlns="http://www.w3.org/2000/svg"`,
    `  width="${w}mm" height="${sizeMm}mm"`,
    `  viewBox="0 0 100 ${showText ? 50 : 100}">`,
    `<rect width="100" height="${showText ? 50 : 100}" fill="#f0f0f0" rx="2"/>`,
    `<text x="50" y="${showText ? 25 : 50}" text-anchor="middle" font-size="8" fill="#999">Loading DataMatrix...</text>`,
    `</svg>`,
  ].join("");
}

function formatIdRows(canonical: string, sizeMm: number): string[] {
  if (sizeMm >= 10) {
    return [
      canonical.slice(0, 4),
      canonical.slice(4, 8),
      canonical.slice(8, 12),
    ];
  }
  return [canonical.slice(0, 4), canonical.slice(4, 8)];
}
