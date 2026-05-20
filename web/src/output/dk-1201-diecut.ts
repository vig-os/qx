// DK-1201 die-cut mode: pack multiple labels onto a single 25 × 80 mm
// printable area on Brother DK-1201 stock (29 × 90 mm physical).
//
// The printer aligns each print to the next die-cut boundary, so each
// page in the print job is exactly one die-cut label (29 × 90 mm with
// a 2 mm margin → 25 × 80 mm content area). We arrange the JobItems'
// rendered labels into a `rows × cols` grid inside that content area,
// then emit one @page per filled die-cut and a final partial page if
// the label count isn't a multiple of `rows × cols`.
//
// Sizing rule — auto-fit (preserve layout aspect):
//   Each cell has uniform dimensions (cellW × cellH) = (printable_w /
//   cols, printable_h / rows). Inside each cell we scale the label's
//   intrinsic SVG (from `layout.measure(opts)`) uniformly so it fits
//   `cellW - 2*padding` and `cellH - 2*padding`, preserving aspect.
//   The user's JobItem.size becomes the layout's *intrinsic* size; the
//   on-paper size is whatever fits. This keeps QR squares square and
//   text legible regardless of grid density.
//
// Alignment — 9-cell grid:
//   `halign` ∈ {start, center, end}, `valign` ∈ {start, center, end}.
//   Default center/center. Padding (mm) carves a uniform margin off
//   each cell before placement; the label is placed at the chosen
//   anchor inside the padded box.
//
// Page geometry:
//   @page is set to 29 × 90 mm with a 2 mm margin so the printer's
//   driver hits the die-cut. The content <div> is 25 × 80 mm
//   (printable area). Cells are uniformly tiled inside that div with
//   absolute mm positioning. A light dashed outline is drawn around
//   each cell when the user enables `cutGuides`, matching the Print
//   tab's preview.

import type {
  OutputMode,
  OutputModeField,
  PlanItem,
  PlannedPage,
} from "../core/types";
import { getLayout } from "../layouts";
import { planItemToOpts } from "./plan-opts";

// Physical and printable dimensions for DK-1201 (Brother spec).
export const DK1201_PHYSICAL_W_MM = 29;
export const DK1201_PHYSICAL_H_MM = 90;
export const DK1201_PRINTABLE_W_MM = 25;
export const DK1201_PRINTABLE_H_MM = 80;
const DK1201_MARGIN_MM = 2; // (29 - 25) / 2 = (90 - 80) / 2

// One rendered label slot ready for placement in a cell.
interface Slot {
  svg: string;
  // Intrinsic mm dimensions of the label's SVG (from layout.measure).
  intrinsicW: number;
  intrinsicH: number;
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

type Align = "start" | "center" | "end";

function readAlign(v: number | string | undefined, fallback: Align): Align {
  if (v === "start" || v === "center" || v === "end") return v;
  return fallback;
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

interface Placed {
  // Absolute mm position within the 25×80 content div.
  xMm: number;
  yMm: number;
  // Drawn dimensions (after auto-fit scaling).
  drawnW: number;
  drawnH: number;
  // Cell rect for the optional cut-guide outline.
  cellX: number;
  cellY: number;
  cellW: number;
  cellH: number;
  svg: string;
}

function placeOnPage(
  slots: Slot[],
  rows: number,
  cols: number,
  padding: number,
  halign: Align,
  valign: Align,
): Placed[] {
  const cellW = DK1201_PRINTABLE_W_MM / cols;
  const cellH = DK1201_PRINTABLE_H_MM / rows;
  const innerW = Math.max(0, cellW - 2 * padding);
  const innerH = Math.max(0, cellH - 2 * padding);

  const placed: Placed[] = [];
  for (let i = 0; i < slots.length; i++) {
    const slot = slots[i];
    const r = Math.floor(i / cols);
    const c = i % cols;
    const cellX = c * cellW;
    const cellY = r * cellH;
    const innerX = cellX + padding;
    const innerY = cellY + padding;

    // Auto-fit: uniform scale to fit innerW × innerH, preserve aspect.
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
      // Dashed outline of the full cell — positioned within the 25×80 div.
      parts.push(
        `<div class="cell-guide" style="left:${p.cellX.toFixed(3)}mm;` +
          `top:${p.cellY.toFixed(3)}mm;width:${p.cellW.toFixed(3)}mm;` +
          `height:${p.cellH.toFixed(3)}mm;"></div>`,
      );
    }
    // Wrap the SVG in a div sized to drawnW × drawnH; the SVG has its
    // own mm width/height and a 0..intrinsicW viewBox, so setting the
    // outer div to drawnW × drawnH visually scales it uniformly. We
    // override the inner SVG sizing via inline style on the wrapper.
    parts.push(
      `<div class="slot" style="left:${p.xMm.toFixed(3)}mm;` +
        `top:${p.yMm.toFixed(3)}mm;width:${p.drawnW.toFixed(3)}mm;` +
        `height:${p.drawnH.toFixed(3)}mm;">${p.svg}</div>`,
    );
  }
  return parts.join("\n");
}

export const dk1201DiecutMode: OutputMode = {
  id: "dk-1201-diecut",
  label: "DK-1201 die-cut (29 × 90 mm)",
  description:
    "Pack a rows × cols grid onto each DK-1201 die-cut label (25 × 80 mm printable). Auto-fits each label to its cell, preserving layout aspect.",

  optionFields(): OutputModeField[] {
    return [
      { key: "rows", label: "Rows", type: "number", default: 2, min: 1, max: 12, step: 1 },
      { key: "cols", label: "Cols", type: "number", default: 4, min: 1, max: 12, step: 1 },
      { key: "padding", label: "Padding (mm)", type: "number", default: 1, min: 0, max: 10, step: 0.5 },
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
        label: "Cell outlines",
        type: "select",
        default: "1",
        options: [
          { value: "1", label: "On" },
          { value: "0", label: "Off" },
        ],
        hint: "Light dashed outline of each cell — cosmetic, prints lightly.",
      },
    ];
  },

  plan(items: PlanItem[], opts: Record<string, number | string>): PlannedPage[] {
    const rows = Math.max(1, Math.floor(readNumber(opts.rows, 2)));
    const cols = Math.max(1, Math.floor(readNumber(opts.cols, 4)));
    const padding = Math.max(0, readNumber(opts.padding, 1));
    const halign = readAlign(opts.halign, "center");
    const valign = readAlign(opts.valign, "center");
    const cutGuides = readBool(opts.cutGuides, true);
    const perPage = rows * cols;

    const slots = expandToSlots(items);
    if (slots.length === 0) return [];

    const pages: PlannedPage[] = [];
    for (let i = 0; i < slots.length; i += perPage) {
      const pageSlots = slots.slice(i, i + perPage);
      const placed = placeOnPage(pageSlots, rows, cols, padding, halign, valign);
      pages.push({
        widthMm: DK1201_PHYSICAL_W_MM,
        heightMm: DK1201_PHYSICAL_H_MM,
        bodyHtml: renderPageBody(placed, cutGuides),
        labelCount: pageSlots.length,
      });
    }
    return pages;
  },

  renderPrintHtml(pages: PlannedPage[]): string {
    const styleParts = [
      "html, body { margin: 0; padding: 0; }",
      `@page { size: ${DK1201_PHYSICAL_W_MM}mm ${DK1201_PHYSICAL_H_MM}mm; ` +
        `margin: ${DK1201_MARGIN_MM}mm; }`,
      ".sheet { position: relative; " +
        `width: ${DK1201_PRINTABLE_W_MM}mm; height: ${DK1201_PRINTABLE_H_MM}mm; ` +
        "page-break-after: always; break-after: page; overflow: hidden; }",
      ".sheet:last-child { page-break-after: auto; break-after: auto; }",
      ".slot { position: absolute; overflow: hidden; }",
      // SVG inside each slot fills its sized wrapper, ignoring its own
      // mm width/height — preserveAspectRatio in the SVG keeps it
      // correct since we computed drawnW/drawnH already preserving the
      // aspect ratio.
      ".slot > svg { width: 100%; height: 100%; display: block; }",
      ".cell-guide { position: absolute; box-sizing: border-box; " +
        "border: 0.15mm dashed #bbb; pointer-events: none; }",
    ];
    const body = pages
      .map((p) => `<div class="sheet">${p.bodyHtml}</div>`)
      .join("\n");
    return `<!doctype html>
<html><head><meta charset="utf-8"><title>Print labels (DK-1201)</title>
<style>${styleParts.join("\n")}</style>
</head>
<body onload="window.print(); setTimeout(() => window.close(), 500);">
${body}
</body></html>`;
  },
};
