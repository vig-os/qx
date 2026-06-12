// DK strip + crop marks output-mode tests (#7 + #11).

import { describe, expect, it } from "vitest";

import type { PlanItem } from "../core/types";
import { dkStripMode } from "./dk-strip";

const ITEMS: PlanItem[] = [
  { id: "K7M3PQ9RT5VAXY", layoutId: "horz", size: 11, copies: 2, extras: {} },
  { id: "2A3B4C5D6E7FGH", layoutId: "horz", size: 11, copies: 1, extras: {} },
];

describe("dkStripMode", () => {
  it("produces a single strip page covering every label when labelsPerStrip=0", () => {
    const pages = dkStripMode.plan(ITEMS, {
      gap: 2,
      cropMarks: "1",
      labelsPerStrip: 0,
    });
    expect(pages).toHaveLength(1);
    expect(pages[0].labelCount).toBe(3);
  });

  it("splits into separate strip pages when labelsPerStrip is set", () => {
    const pages = dkStripMode.plan(ITEMS, {
      gap: 2,
      cropMarks: "1",
      labelsPerStrip: 2,
    });
    expect(pages).toHaveLength(2);
    expect(pages[0].labelCount).toBe(2);
    expect(pages[1].labelCount).toBe(1);
  });

  it("emits a crop-mark divider between labels when cropMarks=1, not when 0", () => {
    const withMarks = dkStripMode.plan(ITEMS, {
      gap: 2,
      cropMarks: "1",
      labelsPerStrip: 0,
    });
    const noMarks = dkStripMode.plan(ITEMS, {
      gap: 2,
      cropMarks: "0",
      labelsPerStrip: 0,
    });
    // 3 labels → 2 boundaries when cropMarks on.
    const cropCount = (withMarks[0].bodyHtml.match(/class="crop"/g) ?? []).length;
    expect(cropCount).toBe(2);
    expect(noMarks[0].bodyHtml).not.toContain('class="crop"');
  });

  it("strip width = sum(labelWidths) + gap × (n-1)", () => {
    // Make sure gap is reflected in the page geometry.
    const tightPages = dkStripMode.plan(ITEMS, {
      gap: 0,
      cropMarks: "0",
      labelsPerStrip: 0,
    });
    const widePages = dkStripMode.plan(ITEMS, {
      gap: 5,
      cropMarks: "0",
      labelsPerStrip: 0,
    });
    // 3 labels → 2 gaps. Wide page should be 10mm wider.
    expect(widePages[0].widthMm).toBeCloseTo(tightPages[0].widthMm + 10, 3);
  });

  it("returns [] for an empty plan", () => {
    expect(dkStripMode.plan([], {})).toEqual([]);
  });

  it("renderPrintHtml emits one @page rule per unique (w, h)", () => {
    const pages = dkStripMode.plan(ITEMS, {
      gap: 2,
      cropMarks: "1",
      labelsPerStrip: 0,
    });
    const html = dkStripMode.renderPrintHtml(pages);
    expect(html).toContain("<!doctype html>");
    expect(html).toContain("window.print()");
    expect((html.match(/@page pg\d+/g) ?? []).length).toBeGreaterThanOrEqual(1);
  });
});
