import { describe, expect, it } from "vitest";
import type { PrintData } from "../protocol";
import { planPages, renderPrintDocument, svgSizeMm } from "./render";

// A Print response fixture in the engine's shape: two labels, each SVG
// carrying mm-dimensioned width/height attributes.
const printResponse: PrintData = {
  labels: [
    {
      id: "PQ7G2MNVX4KH9T",
      svg: '<svg xmlns="http://www.w3.org/2000/svg" width="32mm" height="8mm" viewBox="0 0 32 8"><text>PQ7G2MNVX4KH9T</text></svg>',
    },
    {
      id: "W3JD8RST2UVKXM",
      svg: '<svg xmlns="http://www.w3.org/2000/svg" width="8mm" height="32mm" viewBox="0 0 8 32"><text>W3JD8RST2UVKXM</text></svg>',
    },
  ],
  size_mm: 8,
  chars: "44",
  warning: null,
};

describe("svgSizeMm", () => {
  it("reads mm dimensions off the svg attributes", () => {
    expect(svgSizeMm(printResponse.labels[0]!.svg, 8)).toEqual({ widthMm: 32, heightMm: 8 });
  });

  it("falls back to a size_mm square without mm attributes", () => {
    expect(svgSizeMm("<svg viewBox='0 0 10 10'></svg>", 8)).toEqual({
      widthMm: 8,
      heightMm: 8,
    });
  });
});

describe("planPages", () => {
  it("expands each label into copies pages with its own dimensions", () => {
    const pages = planPages(printResponse, 3);
    expect(pages).toHaveLength(6);
    expect(pages.filter((p) => p.widthMm === 32 && p.heightMm === 8)).toHaveLength(3);
    expect(pages.filter((p) => p.widthMm === 8 && p.heightMm === 32)).toHaveLength(3);
  });
});

describe("renderPrintDocument", () => {
  it("emits one margin-0 @page rule per unique dimension and one div per page", () => {
    const html = renderPrintDocument(planPages(printResponse, 2));
    expect(html).toContain("@page pg0 { size: 32.000mm 8.000mm; margin: 0; }");
    expect(html).toContain("@page pg1 { size: 8.000mm 32.000mm; margin: 0; }");
    const labelDivs = html.match(/<div class="label /g) ?? [];
    expect(labelDivs).toHaveLength(4);
  });

  it("embeds the label SVGs and triggers print on load", () => {
    const html = renderPrintDocument(planPages(printResponse, 1));
    expect(html).toContain("PQ7G2MNVX4KH9T");
    expect(html).toContain("W3JD8RST2UVKXM");
    expect(html).toContain("window.print()");
  });
});
