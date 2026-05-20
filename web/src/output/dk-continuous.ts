// Continuous DK roll, page-per-label, printer auto-cuts.
//
// This is the default mode and the original behavior of the Print tab
// before OutputMode was introduced. Each (id × layout × size) item
// expands into `copies` pages, each sized to the layout's measured
// (widthMm × heightMm). The QL-820NWBc's driver auto-cuts between
// pages on continuous DK tape.
//
// Implementation note: browsers don't all honor per-element @page
// dimensions, so we group pages by exact (w, h) and emit one named
// @page rule per unique dimension. Auto-cut still happens between
// every page break.

import type {
  OutputMode,
  OutputModeField,
  PlanItem,
  PlannedPage,
} from "../core/types";
import { getLayout } from "../layouts";
import { planItemToOpts } from "./plan-opts";

export const dkContinuousMode: OutputMode = {
  id: "dk-continuous",
  label: "DK continuous (auto-cut)",
  description:
    "One page per label on continuous DK tape. Printer auto-cuts between. Default for Brother QL-820NWBc.",

  optionFields(): OutputModeField[] {
    return [];
  },

  plan(items: PlanItem[]): PlannedPage[] {
    const pages: PlannedPage[] = [];
    for (const item of items) {
      const layout = getLayout(item.layoutId);
      if (!layout) continue;
      const opts = planItemToOpts(item);
      const dim = layout.measure(opts);
      const svg = layout.renderSvg(item.id, opts);
      for (let i = 0; i < item.copies; i++) {
        pages.push({
          widthMm: dim.widthMm,
          heightMm: dim.heightMm,
          bodyHtml: svg,
          labelCount: 1,
        });
      }
    }
    return pages;
  },

  renderPrintHtml(pages: PlannedPage[]): string {
    // Group by exact dimensions, one @page rule per group.
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
      ".label { page-break-after: always; break-after: page; overflow: hidden; }",
      ".label:last-child { page-break-after: auto; break-after: auto; }",
      "svg { display: block; }",
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
          .map((p) => `<div class="label ${className}">${p.bodyHtml}</div>`)
          .join("\n"),
      );
    }

    return `<!doctype html>
<html><head><meta charset="utf-8"><title>Print labels</title>
<style>${styleParts.join("\n")}</style>
</head>
<body onload="window.print(); setTimeout(() => window.close(), 500);">
${sections.join("\n")}
</body></html>`;
  },
};
