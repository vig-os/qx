import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { AppContext } from "../core/types";
import type { Registry, RegistryQuery } from "../registry/registry";
import type { RegistryRow } from "../registry/schema";
import { printTab, fmtId, PRINTER_DPI, MM_PER_INCH, PX_TO_MM } from "./print";

const PLAN_KEY = "part-registry.print-plan";

const boundRow: RegistryRow = {
  id: "K7M3PQ9RT5VAXY",
  status: "bound",
  minted_at: "2026-05-08T12:00:00+00:00",
  batch: "B-2026-05-08",
  bound_at: "2026-05-08T12:30:00+00:00",
  type: "PT100",
  description: "Supply temperature sensor",
  vendor: "TC Direct",
  part_number: "402-141",
  location: "cooling loop / supply-T",
  notes: "bench fixture",
  minted_by: "",
  bound_by: "",
  last_edited_at: "",
  last_edited_by: "",
};

const secondRow: RegistryRow = {
  id: "A1B2C3D4E5F6GH",
  status: "unbound",
  minted_at: "2026-05-09T10:00:00+00:00",
  batch: "B-2026-05-08",
  bound_at: "",
  type: "",
  description: "",
  vendor: "",
  part_number: "",
  location: "",
  notes: "",
  minted_by: "",
  bound_by: "",
  last_edited_at: "",
  last_edited_by: "",
};

function makeRegistry(rows: RegistryRow[]): Registry {
  return {
    async load() {},
    all: () => rows,
    find(query: RegistryQuery) {
      if (query.id) return rows.filter((row) => row.id === query.id);
      if (query.prefix) return rows.filter((row) => row.id.startsWith(query.prefix!));
      if (query.batch) return rows.filter((row) => row.batch === query.batch);
      return rows;
    },
    findById(id: string) {
      return rows.find((row) => row.id === id);
    },
    batches() {
      return [...new Set(rows.map((row) => row.batch))];
    },
  };
}

function makeContext(rows: RegistryRow[] = [boundRow]): AppContext {
  return {
    registry: makeRegistry(rows),
    showTab: vi.fn(),
    showPart: vi.fn(),
    getRoute: () => ({ kind: "home" }),
  };
}

function makeLocalStorage() {
  const store = new Map<string, string>();
  return {
    getItem(key: string) {
      return store.get(key) ?? null;
    },
    setItem(key: string, value: string) {
      store.set(key, value);
    },
    removeItem(key: string) {
      store.delete(key);
    },
  };
}

describe("printTab", () => {
  beforeEach(() => {
    vi.stubGlobal("localStorage", makeLocalStorage());
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("renders saved plan rows using the full 4-4-4-2 visible ID format", () => {
    localStorage.setItem(
      PLAN_KEY,
      JSON.stringify([
        {
          id: boundRow.id,
          layoutId: "horz",
          size: 11,
          copies: 2,
          extras: {},
        },
      ]),
    );

    const container = document.createElement("div");
    printTab.mount(container, makeContext());

    expect(container.textContent).toContain("K7M3-PQ9R-T5VA-XY");
    expect(container.textContent).toContain("1 item(s) · 2 label(s) total.");
  });

  it("renders ID cell with title attribute containing the raw canonical ID", () => {
    localStorage.setItem(
      PLAN_KEY,
      JSON.stringify([
        {
          id: boundRow.id,
          layoutId: "horz",
          size: 11,
          copies: 1,
          extras: {},
        },
      ]),
    );

    const container = document.createElement("div");
    printTab.mount(container, makeContext());

    const idCell = container.querySelector(".id-cell");
    expect(idCell).not.toBeNull();
    expect(idCell!.getAttribute("title")).toBe("K7M3PQ9RT5VAXY");
  });
});

describe("fmtId", () => {
  it("returns full 14-char ID in 4-4-4-2 grouping", () => {
    expect(fmtId("ABCDEFGHJKMNPQ")).toBe("ABCD-EFGH-JKMN-PQ");
  });

  it("returns full 14-char ID for realistic input", () => {
    expect(fmtId("K7M3PQ9RT5VAXY")).toBe("K7M3-PQ9R-T5VA-XY");
  });

  it("returns short IDs unchanged", () => {
    expect(fmtId("ABCD")).toBe("ABCD");
    expect(fmtId("")).toBe("");
  });
});

describe("px-to-mm conversion", () => {
  it("uses correct DPI and conversion factor", () => {
    expect(PRINTER_DPI).toBe(300);
    expect(MM_PER_INCH).toBe(25.4);
    expect(PX_TO_MM).toBeCloseTo(25.4 / 300, 10);
  });

  it("converts 100 px to correct mm value", () => {
    const px = 100;
    const mm = px * PX_TO_MM;
    // 100 px at 300 DPI = 100/300 inches = 1/3 inch = 25.4/3 mm ≈ 8.467 mm
    expect(mm).toBeCloseTo(8.4667, 3);
  });

  it("converts 300 px to exactly 25.4 mm (1 inch)", () => {
    const mm = 300 * PX_TO_MM;
    expect(mm).toBeCloseTo(25.4, 10);
  });
});

describe("bulk-add from batch filtering", () => {
  beforeEach(() => {
    vi.stubGlobal("localStorage", makeLocalStorage());
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("mock registry returns correct rows for batch filter", () => {
    const ctx = makeContext([boundRow, secondRow]);

    // Both rows share batch "B-2026-05-08".
    const matched = ctx.registry.find({ batch: "B-2026-05-08" });
    expect(matched).toHaveLength(2);
    expect(matched.map((r) => r.id)).toEqual([
      "K7M3PQ9RT5VAXY",
      "A1B2C3D4E5F6GH",
    ]);
  });

  it("returns empty for non-existent batch", () => {
    const ctx = makeContext([boundRow, secondRow]);
    const matched = ctx.registry.find({ batch: "B-NONEXISTENT" });
    expect(matched).toHaveLength(0);
  });

  it("batches() returns deduplicated list", () => {
    const ctx = makeContext([boundRow, secondRow]);
    expect(ctx.registry.batches()).toEqual(["B-2026-05-08"]);
  });
});
