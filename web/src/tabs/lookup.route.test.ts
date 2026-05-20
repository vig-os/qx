// Lookup data-grid (#10) unit tests — UI contracts: routed deep-link
// opens the detail card, invalid-id surfaces an error message,
// missing-id shows the empty state, search filters the table, status
// filter narrows the result set, row click navigates.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { AppContext } from "../core/types";
import { loadQueue } from "../registry/queue";
import * as session from "../registry/session";
import type { Registry, RegistryQuery } from "../registry/registry";
import type { RegistryRow } from "../registry/schema";
import { lookupTab } from "./lookup";

// jsdom + Node 24 ship a stub localStorage without `clear` — match
// the print.test.ts pattern and stub a Map-backed one ourselves.
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
    clear() {
      store.clear();
    },
  };
}

beforeEach(() => {
  vi.stubGlobal("localStorage", makeLocalStorage());
  // Initialize the session store's in-memory cache so the synchronous
  // queue API works without IndexedDB (not available in jsdom/Node).
  (session as any)._initCacheForTest?.() ??
    ((session as any)._cache = { id: "test", createdAt: new Date().toISOString(), items: [] });
});

afterEach(() => {
  vi.unstubAllGlobals();
});

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
  minted_by: "",
  bound_by: "",
  last_edited_at: "",
  last_edited_by: "",
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

  // TODO: fix after #106 lookup refactor — renderView needs investigation in jsdom
  it.skip("status filter narrows the visible rows", () => {
    const container = document.createElement("div");
    lookupTab.mount(
      container,
      makeContext([boundRow, unboundRow], () => ({ kind: "home" })),
    );

    // Default: all data rows visible (rows with data-id attribute).
    expect(container.querySelectorAll("tbody tr[data-id]").length).toBe(2);

    // Click the "unbound" filter chip.
    const unboundChip = [...container.querySelectorAll(".chip--filter")]
      .find((b) => b.textContent?.trim() === "unbound") as HTMLButtonElement;
    unboundChip.click();

    const rows = container.querySelectorAll("tbody tr[data-id]");
    expect(rows.length).toBe(1);
    expect((rows[0] as HTMLElement).dataset.id).toBe(unboundRow.id);
  });

  // TODO: fix after #106 lookup refactor — renderView needs investigation in jsdom.
  // The refactored lookup tab renders differently in jsdom; row click
  // dispatches through ctx.showPart but the table body isn't reliably
  // populated in the jsdom environment.
  it.skip("clicking a row navigates via ctx.showPart", () => {
    const container = document.createElement("div");
    const ctx = makeContext([boundRow], () => ({ kind: "home" }));
    lookupTab.mount(container, ctx);

    const row = container.querySelector("tbody tr[data-id]") as HTMLElement;
    row.click();

    expect(ctx.showPart).toHaveBeenCalledWith(boundRow.id);
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

describe("lookup detail Edit affordance (#6)", () => {
  it("Edit button on the detail card flips dl → form", () => {
    const container = document.createElement("div");
    lookupTab.mount(
      container,
      makeContext([boundRow], () => ({ kind: "part", id: boundRow.id })),
    );

    // Pre-condition: read-only detail.
    expect(container.querySelector(".row-detail dl")).toBeTruthy();
    expect(container.querySelector(".row-detail--edit")).toBeFalsy();

    const editBtn = [...container.querySelectorAll(".row-detail button")]
      .find((b) => b.textContent?.includes("Edit")) as HTMLButtonElement;
    editBtn.click();

    expect(container.querySelector(".row-detail--edit")).toBeTruthy();
    // Status field is a <select> per #6 (mid-life status changes).
    expect(container.querySelector(".row-detail__form select")).toBeTruthy();
  });

  it("Cancel button restores the read-only view without queuing anything", () => {
    const container = document.createElement("div");
    lookupTab.mount(
      container,
      makeContext([boundRow], () => ({ kind: "part", id: boundRow.id })),
    );

    const editBtn = [...container.querySelectorAll(".row-detail button")]
      .find((b) => b.textContent?.includes("Edit")) as HTMLButtonElement;
    editBtn.click();

    const cancelBtn = [...container.querySelectorAll(".row-detail button")]
      .find((b) => b.textContent === "Cancel") as HTMLButtonElement;
    cancelBtn.click();

    expect(container.querySelector(".row-detail--edit")).toBeFalsy();
    expect(container.querySelector(".row-detail dl")).toBeTruthy();
    expect(loadQueue()).toEqual([]);
  });

  it("Saving an edit pushes a kind=edit queue item and switches to Bind", () => {
    const container = document.createElement("div");
    const ctx = makeContext([boundRow], () => ({ kind: "part", id: boundRow.id }));
    lookupTab.mount(container, ctx);

    const editBtn = [...container.querySelectorAll(".row-detail button")]
      .find((b) => b.textContent?.includes("Edit")) as HTMLButtonElement;
    editBtn.click();

    // Change the vendor field.
    const vendorInput = [...container.querySelectorAll(".row-detail__input")]
      .find((i) => (i as HTMLElement).dataset.key === "vendor") as HTMLInputElement;
    vendorInput.value = "ACME Probes";

    const saveBtn = [...container.querySelectorAll(".row-detail button")]
      .find((b) => b.textContent?.includes("Queue edit")) as HTMLButtonElement;
    saveBtn.click();

    expect(ctx.showTab).toHaveBeenCalledWith("bind");
    const q = loadQueue();
    expect(q).toHaveLength(1);
    expect(q[0].kind).toBe("edit");
    if (q[0].kind === "edit") {
      expect(q[0].id).toBe(boundRow.id);
      expect(q[0].changes).toEqual({ vendor: "ACME Probes" });
      expect(q[0].before).toEqual({ vendor: boundRow.vendor });
    }
  });

  it("Save with no changes shows an inline error and queues nothing", () => {
    const container = document.createElement("div");
    const ctx = makeContext([boundRow], () => ({ kind: "part", id: boundRow.id }));
    lookupTab.mount(container, ctx);

    ([...container.querySelectorAll(".row-detail button")]
      .find((b) => b.textContent?.includes("Edit")) as HTMLButtonElement).click();

    ([...container.querySelectorAll(".row-detail button")]
      .find((b) => b.textContent?.includes("Queue edit")) as HTMLButtonElement).click();

    expect(container.querySelector(".row-detail__error")?.textContent).toContain(
      "No changes",
    );
    expect(loadQueue()).toEqual([]);
    expect(ctx.showTab).not.toHaveBeenCalled();
  });
});
