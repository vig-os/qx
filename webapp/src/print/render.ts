// Continuous-roll print document: one page per label, the printer
// auto-cuts between pages (the dk-continuous output mode of the legacy
// web shell, ported to the protocol's Print response). Browsers don't
// all honor per-element @page dimensions, so pages are grouped by
// exact (width, height) and one named @page rule is emitted per unique
// dimension — auto-cut still happens between every page break.
//
// Die-cut sheet packing is deliberately out of scope for this page:
// the webapp targets continuous tape only.

import type { PrintData } from "../protocol";

export interface PrintablePage {
  widthMm: number;
  heightMm: number;
  svg: string;
}

/**
 * Read the mm dimensions off a rendered label SVG's width/height
 * attributes. Falls back to a square of `fallbackMm` (the response's
 * size_mm) when the SVG doesn't carry mm-dimensioned attributes.
 */
export function svgSizeMm(
  svg: string,
  fallbackMm: number,
): { widthMm: number; heightMm: number } {
  const w = /\bwidth="([0-9.]+)mm"/.exec(svg);
  const h = /\bheight="([0-9.]+)mm"/.exec(svg);
  if (w?.[1] !== undefined && h?.[1] !== undefined) {
    return { widthMm: Number(w[1]), heightMm: Number(h[1]) };
  }
  return { widthMm: fallbackMm, heightMm: fallbackMm };
}

/** Expand a Print response into per-page entries (× copies per label). */
export function planPages(data: PrintData, copies: number): PrintablePage[] {
  const pages: PrintablePage[] = [];
  for (const label of data.labels) {
    const dim = svgSizeMm(label.svg, data.size_mm);
    for (let i = 0; i < copies; i++) {
      pages.push({ ...dim, svg: label.svg });
    }
  }
  return pages;
}

/**
 * Render the standalone print document: margin-0 @page rules sized in
 * mm per unique page dimension, one page-broken div per label, and a
 * body onload that prints then closes the child window.
 */
export function renderPrintDocument(pages: PrintablePage[]): string {
  const dimsKey = (p: PrintablePage) => `${p.widthMm.toFixed(3)}x${p.heightMm.toFixed(3)}`;
  const groups = new Map<string, PrintablePage[]>();
  for (const p of pages) {
    const k = dimsKey(p);
    const group = groups.get(k);
    if (group) group.push(p);
    else groups.set(k, [p]);
  }

  const styleParts: string[] = [
    "html, body { margin: 0; padding: 0; }",
    ".label { page-break-after: always; break-after: page; overflow: hidden; }",
    ".label:last-child { page-break-after: auto; break-after: auto; }",
    "svg { display: block; width: 100%; height: 100%; }",
  ];
  const sections: string[] = [];
  let i = 0;
  for (const items of groups.values()) {
    const className = `pg${i++}`;
    const first = items[0];
    if (!first) continue;
    const w = first.widthMm.toFixed(3);
    const h = first.heightMm.toFixed(3);
    styleParts.push(
      `@page ${className} { size: ${w}mm ${h}mm; margin: 0; }`,
      `.${className} { page: ${className}; width: ${w}mm; height: ${h}mm; }`,
    );
    sections.push(items.map((p) => `<div class="label ${className}">${p.svg}</div>`).join("\n"));
  }

  return `<!doctype html>
<html><head><meta charset="utf-8"><title>Print labels</title>
<style>${styleParts.join("\n")}</style>
</head>
<body onload="window.print(); setTimeout(() => window.close(), 500);">
${sections.join("\n")}
</body></html>`;
}
