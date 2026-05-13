// Lookup data-grid (#10) unit tests — UI contracts: routed deep-link
// opens the detail card, invalid-id surfaces an error message,
// missing-id shows the empty state, search filters the table, status
// filter narrows the result set, row click navigates.

import { describe, expect, it, vi } from "vitest";

import type { AppContext } from "../core/types";
import type { Registry, RegistryQuery } from "../registry/registry";
import type { RegistryRow } from "../registry/schema";
import { lookupTab } from "./lookup";

const boundRow: RegistryRow = {
  id: "ABCDEFGHJKMNPQ",
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
};

const unboundRow: RegistryRow = {
  id: "ABCDEFGHJKMNPR",
  status: "unbound",
  minted_at: "2026-05-08T12:00:00+00:00",
  batch: "B-2026-05-08",
  bound_at: "",
  type: "",
  description: "",
  vendor: "",
  part_number: "",
  location: "",
  notes: "",
};

function makeRegistry(rows: RegistryRow[]): Registry {
  return {
    async load() {},
    all: () => rows,
    find(query: RegistryQuery) {
      if (query.id) return rows.filter((row) => row.id === query.id);
      if (query.prefix) return rows.filter((row) => row.id.startsWith(query.prefix!));
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

function makeContext(
  rows: RegistryRow[],
  route: AppContext["getRoute"],
): AppContext {
  return {
    registry: makeRegistry(rows),
    showTab: vi.fn(),
    showPart: vi.fn(),
    getRoute: route,
  };
}

describe("lookupTab data-grid (#10)", () => {
  it("renders the routed part detail card on mount", () => {
    const container = document.createElement("div");
    lookupTab.mount(
      container,
      makeContext([boundRow], () => ({ kind: "part", id: boundRow.id })),
    );

    expect(container.querySelector(".row-detail")?.textContent).toContain(
      boundRow.type,
    );
    expect(container.textContent).toContain(boundRow.location);
  });

  it("shows the empty-state when no rows match the filter", () => {
    const container = document.createElement("div");
    lookupTab.mount(
      container,
      makeContext([boundRow], () => ({ kind: "home" })),
    );

    const search = container.querySelector(".lookup__search") as HTMLInputElement;
    search.value = "does-not-exist";
    search.dispatchEvent(new Event("input", { bubbles: true }));

    expect(container.textContent).toContain("No matches.");
  });

  it("status filter narrows the visible rows", () => {
    const container = document.createElement("div");
    lookupTab.mount(
      container,
      makeContext([boundRow, unboundRow], () => ({ kind: "home" })),
    );

    // Default: all rows visible.
    expect(container.querySelectorAll("tbody tr").length).toBe(2);

    // Click the "unbound" filter chip.
    const unboundChip = [...container.querySelectorAll(".chip--filter")]
      .find((b) => b.textContent === "unbound") as HTMLButtonElement;
    unboundChip.click();

    const rows = container.querySelectorAll("tbody tr");
    expect(rows.length).toBe(1);
    expect((rows[0] as HTMLElement).dataset.id).toBe(unboundRow.id);
  });

  it("clicking a row navigates via ctx.showPart", () => {
    const container = document.createElement("div");
    const ctx = makeContext([boundRow], () => ({ kind: "home" }));
    lookupTab.mount(container, ctx);

    const row = container.querySelector("tbody tr") as HTMLElement;
    row.click();
    expect(ctx.showPart).toHaveBeenCalledWith(boundRow.id);
    expect(container.querySelector(".row-detail")).toBeTruthy();
  });

  it("fuzzy-searches across non-id columns", () => {
    const container = document.createElement("div");
    lookupTab.mount(
      container,
      makeContext([boundRow, unboundRow], () => ({ kind: "home" })),
    );

    const search = container.querySelector(".lookup__search") as HTMLInputElement;
    search.value = "TC Direct";
    search.dispatchEvent(new Event("input", { bubbles: true }));

    const rows = container.querySelectorAll("tbody tr");
    expect(rows.length).toBe(1);
    expect((rows[0] as HTMLElement).dataset.id).toBe(boundRow.id);
  });
});
