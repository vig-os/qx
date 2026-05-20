import { describe, expect, it } from "vitest";

import type { PlanItem } from "../core/types";
import { dk1201DiecutMode } from "./dk-1201-diecut";

const ITEMS: PlanItem[] = [
  {
    id: "K7M3PQ9RT5VAXY",
    layoutId: "horz",
    size: 11,
    copies: 1,
    extras: {},
  },
  {
    id: "2A3B4C5D6E7FGH",
    layoutId: "vert",
    size: 11,
    copies: 1,
    extras: {},
  },
];

describe("DK-1201 FE print use case", () => {
  it("generates a printable sheet plan with 14-char canonical IDs and auto-format visible text", () => {
    const pages = dk1201DiecutMode.plan(ITEMS, {
      rows: 2,
      cols: 2,
      padding: 1,
      halign: "center",
      valign: "center",
      cutGuides: "1",
    });

    expect(pages).toHaveLength(1);
    expect(pages[0].labelCount).toBe(2);
    // Both IDs' first 4 chars visible
    expect(pages[0].bodyHtml).toContain("K7M3");
    expect(pages[0].bodyHtml).toContain("PQ9R");
    expect(pages[0].bodyHtml).toContain("2A3B");
    expect(pages[0].bodyHtml).toContain("4C5D");
    // Auto-format at 11 mm = 4/4/4 (12 chars), so T5VA and 6E7F are visible
    expect(pages[0].bodyHtml).toContain("T5VA");
    expect(pages[0].bodyHtml).toContain("6E7F");
    // Only the final 2 chars (XY, GH) are truncated in 4/4/4
    expect(pages[0].bodyHtml).not.toContain("XY");
    expect(pages[0].bodyHtml).not.toContain("GH");
  });

  it("emits a standalone print document that can be printed or saved by the browser", () => {
    const pages = dk1201DiecutMode.plan(ITEMS, {
      rows: 2,
      cols: 2,
      padding: 1,
      halign: "center",
      valign: "center",
      cutGuides: "1",
    });

    const html = dk1201DiecutMode.renderPrintHtml(pages);
    expect(html).toContain("<!doctype html>");
    expect(html).toContain("window.print()");
    expect(html).toContain("Print labels (DK-1201)");
    expect(html).toContain("K7M3");
    expect(html).toContain("PQ9R");
  });
});
