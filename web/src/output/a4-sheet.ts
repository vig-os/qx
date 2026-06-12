// A4 / Letter sticker sheet mode (#11): pack labels into a grid on
// office paper, with configurable margins, cell padding, and cut
// guides. Useful for non-Brother flows (inkjet/laser → cut by hand
// or pre-die-cut Avery stock).
//
// Behavior mirrors `dk-1201-diecut`: each slot is uniformly scaled to
// fit `(cellW - 2*padding) × (cellH - 2*padding)` preserving aspect,
// and placed at the chosen anchor inside the padded cell. Adds:
//   - paper-size selector: A4 (210×297) or Letter (215.9×279.4)
//   - page margins (uniform mm on all sides)
//   - cut guides default On (operator usually scissors-cuts these)

import type {
  OutputMode,
  OutputModeField,
  PlanItem,
  PlannedPage,
} from "../core/types";
import { getLayout } from "../layouts";
import { planItemToOpts } from "./plan-opts";

const PAPER_SIZES: Record<string, { w: number; h: number }> = {
  a4: { w: 210, h: 297 },
  letter: { w: 215.9, h: 279.4 },
};

type Align = "start" | "center" | "end";

interface Slot {
  svg: string;
  intrinsicW: number;
  intrinsicH: number;
}

interface Placed {
  xMm: number;
  yMm: number;
  drawnW: number;
  drawnH: number;
  cellX: number;
  cellY: number;
  cellW: number;
  cellH: number;
  svg: string;
}

function readNumber(v: number | string | undefined, fallback: number): number {
  if (typeof v === "number") return v;
  if (typeof v === "string" && v !== "") {
    const n = parseFloat(v);
    if (!Number.isNaN(n)) return n;
  }
  return fallback;
}

function readAlign(v: number | string | undefined, fallback: Align): Align {
  if (v === "start" || v === "center" || v === "end") return v;
  return fallback;
}

function readBool(v: number | string | undefined, fallback: boolean): boolean {
  if (typeof v === "number") return v !== 0;
  if (typeof v === "string") return v === "1" || v === "true";
  return fallback;
}

function expandToSlots(items: PlanItem[]): Slot[] {
  const slots: Slot[] = [];
  for (const item of items) {
    const layout = getLayout(item.layoutId);
    if (!layout) continue;
    const opts = planItemToOpts(item);
    const dim = layout.measure(opts);
    const svg = layout.renderSvg(item.id, opts);
    for (let i = 0; i < item.copies; i++) {
      slots.push({ svg, intrinsicW: dim.widthMm, intrinsicH: dim.heightMm });
    }
  }
  return slots;
}

function placeOnPage(
  slots: Slot[],
  rows: number,
  cols: number,
  printableW: number,
  printableH: number,
  marginX: number,
  marginY: number,
  padding: number,
  halign: Align,
  valign: Align,
): Placed[] {
  const cellW = printableW / cols;
  const cellH = printableH / rows;
  const innerW = Math.max(0, cellW - 2 * padding);
  const innerH = Math.max(0, cellH - 2 * padding);

  const placed: Placed[] = [];
  for (let i = 0; i < slots.length; i++) {
    const slot = slots[i];
    const r = Math.floor(i / cols);
    const c = i % cols;
    const cellX = marginX + c * cellW;
    const cellY = marginY + r * cellH;
    const innerX = cellX + padding;
    const innerY = cellY + padding;

    const scale = Math.min(innerW / slot.intrinsicW, innerH / slot.intrinsicH);
    const drawnW = slot.intrinsicW * scale;
    const drawnH = slot.intrinsicH * scale;

    const slack = (axisInner: number, drawn: number, align: Align) => {
      const free = axisInner - drawn;
      if (align === "start") return 0;
      if (align === "end") return free;
      return free / 2;
    };
    const xMm = innerX + slack(innerW, drawnW, halign);
    const yMm = innerY + slack(innerH, drawnH, valign);

    placed.push({
      xMm,
      yMm,
      drawnW,
      drawnH,
      cellX,
      cellY,
      cellW,
      cellH,
      svg: slot.svg,
    });
  }
  return placed;
}

function renderPageBody(placed: Placed[], cutGuides: boolean): string {
  const parts: string[] = [];
  for (const p of placed) {
    if (cutGuides) {
      parts.push(
        `<div class="cell-guide" style="left:${p.cellX.toFixed(3)}mm;` +
          `top:${p.cellY.toFixed(3)}mm;width:${p.cellW.toFixed(3)}mm;` +
          `height:${p.cellH.toFixed(3)}mm;"></div>`,
      );
    }
    parts.push(
      `<div class="slot" style="left:${p.xMm.toFixed(3)}mm;` +
        `top:${p.yMm.toFixed(3)}mm;width:${p.drawnW.toFixed(3)}mm;` +
        `height:${p.drawnH.toFixed(3)}mm;">${p.svg}</div>`,
    );
  }
  return parts.join("\n");
}

export const a4SheetMode: OutputMode = {
  id: "a4-sticker-sheet",
  label: "Sticker sheet (A4 / Letter)",
  description:
    "Pack a rows × cols grid onto A4 or Letter office paper. Auto-fits each label to its cell. Cut guides default on for scissors-cut workflows.",

  optionFields(): OutputModeField[] {
    return [
      {
        key: "paper",
        label: "Paper",
        type: "select",
        default: "a4",
        options: [
          { value: "a4", label: "A4 (210 × 297 mm)" },
          { value: "letter", label: "Letter (215.9 × 279.4 mm)" },
        ],
      },
      { key: "rows", label: "Rows", type: "number", default: 10, min: 1, max: 40, step: 1 },
      { key: "cols", label: "Cols", type: "number", default: 3, min: 1, max: 20, step: 1 },
      { key: "marginX", label: "Margin X (mm)", type: "number", default: 8, min: 0, max: 30, step: 0.5 },
      { key: "marginY", label: "Margin Y (mm)", type: "number", default: 12, min: 0, max: 30, step: 0.5 },
      { key: "padding", label: "Cell padding (mm)", type: "number", default: 1.5, min: 0, max: 10, step: 0.5 },
      {
        key: "halign",
        label: "H-align",
        type: "select",
        default: "center",
        options: [
          { value: "start", label: "Left" },
          { value: "center", label: "Center" },
          { value: "end", label: "Right" },
        ],
      },
      {
        key: "valign",
        label: "V-align",
        type: "select",
        default: "center",
        options: [
          { value: "start", label: "Top" },
          { value: "center", label: "Middle" },
          { value: "end", label: "Bottom" },
        ],
      },
      {
        key: "cutGuides",
        label: "Cut guides",
        type: "select",
        default: "1",
        options: [
          { value: "1", label: "On" },
          { value: "0", label: "Off" },
        ],
        hint: "Dashed cell outlines for hand-cutting.",
      },
    ];
  },

  plan(items: PlanItem[], opts: Record<string, number | string>): PlannedPage[] {
    const paperKey = typeof opts.paper === "string" ? opts.paper : "a4";
    const paper = PAPER_SIZES[paperKey] ?? PAPER_SIZES.a4;
    const rows = Math.max(1, Math.floor(readNumber(opts.rows, 10)));
    const cols = Math.max(1, Math.floor(readNumber(opts.cols, 3)));
    const marginX = Math.max(0, readNumber(opts.marginX, 8));
    const marginY = Math.max(0, readNumber(opts.marginY, 12));
    const padding = Math.max(0, readNumber(opts.padding, 1.5));
    const halign = readAlign(opts.halign, "center");
    const valign = readAlign(opts.valign, "center");
    const cutGuides = readBool(opts.cutGuides, true);

    const printableW = Math.max(0, paper.w - 2 * marginX);
    const printableH = Math.max(0, paper.h - 2 * marginY);
    const perPage = rows * cols;

    const slots = expandToSlots(items);
    if (slots.length === 0) return [];

    const pages: PlannedPage[] = [];
    for (let i = 0; i < slots.length; i += perPage) {
      const pageSlots = slots.slice(i, i + perPage);
      const placed = placeOnPage(
        pageSlots,
        rows,
        cols,
        printableW,
        printableH,
        marginX,
        marginY,
        padding,
        halign,
        valign,
      );
      pages.push({
        widthMm: paper.w,
        heightMm: paper.h,
        bodyHtml: renderPageBody(placed, cutGuides),
        labelCount: pageSlots.length,
      });
    }
    return pages;
  },

  renderPrintHtml(pages: PlannedPage[]): string {
    const first = pages[0];
    const w = first ? first.widthMm.toFixed(3) : "210";
    const h = first ? first.heightMm.toFixed(3) : "297";
    const styleParts = [
      "html, body { margin: 0; padding: 0; }",
      `@page { size: ${w}mm ${h}mm; margin: 0; }`,
      ".sheet { position: relative; " +
        `width: ${w}mm; height: ${h}mm; ` +
        "page-break-after: always; break-after: page; overflow: hidden; }",
      ".sheet:last-child { page-break-after: auto; break-after: auto; }",
      ".slot { position: absolute; overflow: hidden; }",
      ".slot > svg { width: 100%; height: 100%; display: block; }",
      ".cell-guide { position: absolute; box-sizing: border-box; " +
        "border: 0.15mm dashed #bbb; pointer-events: none; }",
    ];
    const body = pages
      .map((p) => `<div class="sheet">${p.bodyHtml}</div>`)
      .join("\n");
    return `<!doctype html>
<html><head><meta charset="utf-8"><title>Print labels (sheet)</title>
<style>${styleParts.join("\n")}</style>
</head>
<body onload="window.print(); setTimeout(() => window.close(), 500);">
${body}
</body></html>`;
  },
};
