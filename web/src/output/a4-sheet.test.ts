// A4 sticker sheet output mode tests (#11).

import { describe, expect, it } from "vitest";

import type { PlanItem } from "../core/types";
import { a4SheetMode } from "./a4-sheet";

const ITEMS: PlanItem[] = [
  { id: "K7M3PQ9RT5VAXY", layoutId: "horz", size: 11, copies: 3, extras: {} },
  { id: "2A3B4C5D6E7FGH", layoutId: "vert", size: 11, copies: 2, extras: {} },
];

describe("a4SheetMode", () => {
  it("packs (3+2)=5 labels into one A4 page when rows×cols ≥ 5", () => {
    const pages = a4SheetMode.plan(ITEMS, {
      paper: "a4",
      rows: 3,
      cols: 3,
      marginX: 8,
      marginY: 12,
      padding: 1.5,
      halign: "center",
      valign: "center",
      cutGuides: "1",
    });
    expect(pages).toHaveLength(1);
    expect(pages[0].widthMm).toBeCloseTo(210, 3);
    expect(pages[0].heightMm).toBeCloseTo(297, 3);
    expect(pages[0].labelCount).toBe(5);
  });

  it("splits into multiple pages when the per-page capacity is exceeded", () => {
    const pages = a4SheetMode.plan(ITEMS, {
      paper: "a4",
      rows: 2,
      cols: 2, // capacity 4 < 5 labels
      marginX: 8,
      marginY: 12,
      padding: 1.5,
      halign: "center",
      valign: "center",
      cutGuides: "1",
    });
    expect(pages).toHaveLength(2);
    expect(pages[0].labelCount).toBe(4);
    expect(pages[1].labelCount).toBe(1);
  });

  it("selects Letter paper geometry when paper=letter", () => {
    const pages = a4SheetMode.plan(ITEMS, {
      paper: "letter",
      rows: 3,
      cols: 3,
      marginX: 8,
      marginY: 12,
      padding: 1.5,
      halign: "center",
      valign: "center",
      cutGuides: "1",
    });
    expect(pages).toHaveLength(1);
    expect(pages[0].widthMm).toBeCloseTo(215.9, 3);
    expect(pages[0].heightMm).toBeCloseTo(279.4, 3);
  });

  it("emits cut-guide divs when cutGuides=1 and not when 0", () => {
    const opts = {
      paper: "a4",
      rows: 2,
      cols: 2,
      marginX: 8,
      marginY: 12,
      padding: 1.5,
      halign: "center",
      valign: "center",
    };
    const on = a4SheetMode.plan(ITEMS, { ...opts, cutGuides: "1" });
    const off = a4SheetMode.plan(ITEMS, { ...opts, cutGuides: "0" });
    expect(on[0].bodyHtml).toContain("cell-guide");
    expect(off[0].bodyHtml).not.toContain("cell-guide");
  });

  it("returns [] for an empty plan", () => {
    expect(a4SheetMode.plan([], {})).toEqual([]);
  });

  it("renderPrintHtml produces a standalone printable doc", () => {
    const pages = a4SheetMode.plan(ITEMS, {
      paper: "a4",
      rows: 3,
      cols: 3,
      marginX: 8,
      marginY: 12,
      padding: 1.5,
      halign: "center",
      valign: "center",
      cutGuides: "1",
    });
    const html = a4SheetMode.renderPrintHtml(pages);
    expect(html).toContain("<!doctype html>");
    expect(html).toContain("window.print()");
    expect(html).toContain("@page");
    expect(html).toContain("210");
  });
});
