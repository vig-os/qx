// DK continuous strip + crop marks (#7 + #11): emit one long page
// containing every label concatenated horizontally with a configurable
// gap and a dashed cut line at each boundary. Useful when the operator
// wants to print the whole batch as a single tape segment and cut by
// hand (e.g. for archiving a strip of related sensors together).
//
// Output: one page per "strip" — the operator can configure
// `labelsPerStrip` to split a long job into multiple pages.

import type {
  OutputMode,
  OutputModeField,
  PlanItem,
  PlannedPage,
} from "../core/types";
import { getLayout } from "../layouts";
import { planItemToOpts } from "./plan-opts";

interface RenderedLabel {
  svg: string;
  widthMm: number;
  heightMm: number;
}

function readNumber(v: number | string | undefined, fallback: number): number {
  if (typeof v === "number") return v;
  if (typeof v === "string" && v !== "") {
    const n = parseFloat(v);
    if (!Number.isNaN(n)) return n;
  }
  return fallback;
}

function readBool(v: number | string | undefined, fallback: boolean): boolean {
  if (typeof v === "number") return v !== 0;
  if (typeof v === "string") return v === "1" || v === "true";
  return fallback;
}

function expandToLabels(items: PlanItem[]): RenderedLabel[] {
  const out: RenderedLabel[] = [];
  for (const item of items) {
    const layout = getLayout(item.layoutId);
    if (!layout) continue;
    const opts = planItemToOpts(item);
    const dim = layout.measure(opts);
    const svg = layout.renderSvg(item.id, opts);
    for (let i = 0; i < item.copies; i++) {
      out.push({ svg, widthMm: dim.widthMm, heightMm: dim.heightMm });
    }
  }
  return out;
}

function renderStripBody(
  labels: RenderedLabel[],
  gap: number,
  cropMarks: boolean,
  stripHeight: number,
): string {
  const parts: string[] = [];
  let xMm = 0;
  for (let i = 0; i < labels.length; i++) {
    const lbl = labels[i];
    parts.push(
      `<div class="slot" style="left:${xMm.toFixed(3)}mm;top:0;` +
        `width:${lbl.widthMm.toFixed(3)}mm;height:${stripHeight.toFixed(3)}mm;">` +
        lbl.svg +
        `</div>`,
    );
    if (cropMarks && i < labels.length - 1) {
      const cutX = xMm + lbl.widthMm + gap / 2;
      parts.push(
        `<div class="crop" style="left:${cutX.toFixed(3)}mm;top:0;` +
          `height:${stripHeight.toFixed(3)}mm;"></div>`,
      );
    }
    xMm += lbl.widthMm + gap;
  }
  return parts.join("\n");
}

export const dkStripMode: OutputMode = {
  id: "dk-strip",
  label: "DK strip + crop marks",
  description:
    "Concatenate every label horizontally as one long strip with a dashed cut line between each. Cut manually with scissors after printing.",

  optionFields(): OutputModeField[] {
    return [
      {
        key: "gap",
        label: "Gap (mm)",
        type: "number",
        default: 2,
        min: 0,
        max: 20,
        step: 0.5,
        hint: "Whitespace between adjacent labels.",
      },
      {
        key: "cropMarks",
        label: "Crop marks",
        type: "select",
        default: "1",
        options: [
          { value: "1", label: "On" },
          { value: "0", label: "Off" },
        ],
        hint: "Dashed vertical cut line between labels.",
      },
      {
        key: "labelsPerStrip",
        label: "Labels / strip",
        type: "number",
        default: 0,
        min: 0,
        max: 200,
        step: 1,
        hint: "0 = one strip with all labels. Otherwise split into pages.",
      },
    ];
  },

  plan(items: PlanItem[], opts: Record<string, number | string>): PlannedPage[] {
    const gap = Math.max(0, readNumber(opts.gap, 2));
    const cropMarks = readBool(opts.cropMarks, true);
    const labelsPerStrip = Math.max(0, Math.floor(readNumber(opts.labelsPerStrip, 0)));

    const labels = expandToLabels(items);
    if (labels.length === 0) return [];

    const chunks: RenderedLabel[][] = [];
    if (labelsPerStrip === 0) {
      chunks.push(labels);
    } else {
      for (let i = 0; i < labels.length; i += labelsPerStrip) {
        chunks.push(labels.slice(i, i + labelsPerStrip));
      }
    }

    const pages: PlannedPage[] = [];
    for (const chunk of chunks) {
      const stripHeight = Math.max(...chunk.map((l) => l.heightMm));
      const stripWidth =
        chunk.reduce((acc, l) => acc + l.widthMm, 0) + gap * (chunk.length - 1);
      pages.push({
        widthMm: Math.max(stripWidth, 1),
        heightMm: Math.max(stripHeight, 1),
        bodyHtml: renderStripBody(chunk, gap, cropMarks, stripHeight),
        labelCount: chunk.length,
      });
    }
    return pages;
  },

  renderPrintHtml(pages: PlannedPage[]): string {
    // Each strip is its own page sized to that strip's bounding box,
    // so the printer's auto-cut between pages naturally cuts between
    // strips even on continuous tape.
    const dimsKey = (p: PlannedPage) =>
      `${p.widthMm.toFixed(3)}x${p.heightMm.toFixed(3)}`;
    const groups = new Map<string, PlannedPage[]>();
    for (const p of pages) {
      const k = dimsKey(p);
      if (!groups.has(k)) groups.set(k, []);
      groups.get(k)!.push(p);
    }

    const styleParts: string[] = [
      "html, body { margin: 0; padding: 0; }",
      ".strip { position: relative; page-break-after: always; break-after: page; overflow: hidden; }",
      ".strip:last-child { page-break-after: auto; break-after: auto; }",
      ".slot { position: absolute; overflow: hidden; }",
      ".slot > svg { width: 100%; height: 100%; display: block; }",
      ".crop { position: absolute; width: 0; border-left: 0.18mm dashed #888; pointer-events: none; }",
    ];
    const sections: string[] = [];
    let i = 0;
    for (const [, items] of groups) {
      const className = `pg${i++}`;
      const w = items[0].widthMm.toFixed(3);
      const h = items[0].heightMm.toFixed(3);
      styleParts.push(
        `@page ${className} { size: ${w}mm ${h}mm; margin: 0; }`,
        `.${className} { page: ${className}; width: ${w}mm; height: ${h}mm; }`,
      );
      sections.push(
        items
          .map((p) => `<div class="strip ${className}">${p.bodyHtml}</div>`)
          .join("\n"),
      );
    }

    return `<!doctype html>
<html><head><meta charset="utf-8"><title>Print labels (strip)</title>
<style>${styleParts.join("\n")}</style>
</head>
<body onload="window.print(); setTimeout(() => window.close(), 500);">
${sections.join("\n")}
</body></html>`;
  },
};
