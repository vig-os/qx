// Queue migration + bind/edit shape tests (#6).
//
// As of #115/#117 the queue module is a facade over the session store.
// Tests initialize the session store's in-memory cache before
// exercising the synchronous queue API.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  appendBind,
  appendEdit,
  clearQueue,
  loadQueue,
  removeAt,
  saveQueue,
  summarizeQueue,
  type QueueItem,
} from "./queue";

// Initialize the session store's in-memory cache so the synchronous
// queue API works without IndexedDB (not available in jsdom/Node).
import * as session from "./session";

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

beforeEach(async () => {
  vi.stubGlobal("localStorage", makeLocalStorage());
  // IndexedDB not available in test — session falls back to localStorage.
  // Force-load session to populate the in-memory cache.
  try {
    await session.loadSession();
  } catch {
    // If IndexedDB is missing, the catch in loadSession handles it
    // and falls back to localStorage. The cache should be populated.
  }
  // Ensure we start clean
  await session.clearSession();
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("appendBind / appendEdit / removeAt", () => {
  it("appends bind items with kind=bind and a queued_at timestamp", async () => {
    await appendBind({
      id: "ABCDEFGHJKMNPQ",
      type: "PT100",
      description: "",
      vendor: "TC",
      part_number: "",
      location: "",
      notes: "",
    });
    const q = loadQueue();
    expect(q).toHaveLength(1);
    expect(q[0].kind).toBe("bind");
    expect(q[0].queued_at).toMatch(/^\d{4}-\d{2}-\d{2}T/);
  });

  it("appends edit items carrying before+changes", async () => {
    await appendEdit(
      "ABCDEFGHJKMNPQ",
      { vendor: "TC", location: "" },
      { vendor: "TC Direct", location: "supply" },
    );
    const q = loadQueue();
    expect(q).toHaveLength(1);
    expect(q[0].kind).toBe("edit");
    if (q[0].kind === "edit") {
      expect(q[0].changes).toEqual({ vendor: "TC Direct", location: "supply" });
      expect(q[0].before).toEqual({ vendor: "TC", location: "" });
    }
  });

  it("removeAt removes the right item even with mixed kinds", async () => {
    await appendBind({
      id: "ABCDEFGHJKMNPQ",
      type: "",
      description: "",
      vendor: "",
      part_number: "",
      location: "",
      notes: "",
    });
    await appendEdit("ABCDEFGHJKMNPR", {}, { location: "lab" });
    await appendBind({
      id: "ABCDEFGHJKMNPS",
      type: "",
      description: "",
      vendor: "",
      part_number: "",
      location: "",
      notes: "",
    });
    await removeAt(1);
    const q = loadQueue();
    expect(q.map((x) => x.id)).toEqual(["ABCDEFGHJKMNPQ", "ABCDEFGHJKMNPS"]);
  });
});

describe("summarizeQueue", () => {
  it("labels pure-bind, pure-edit, and mixed queues distinctly", () => {
    const bind: QueueItem = {
      kind: "bind",
      id: "A",
      queued_at: "",
      type: "",
      description: "",
      vendor: "",
      part_number: "",
      location: "",
      notes: "",
    };
    const edit: QueueItem = {
      kind: "edit",
      id: "B",
      queued_at: "",
      before: {},
      changes: { location: "lab" },
    };
    expect(summarizeQueue([])).toMatchObject({ label: "bind", total: 0 });
    expect(summarizeQueue([bind])).toMatchObject({ label: "bind", binds: 1, edits: 0 });
    expect(summarizeQueue([edit])).toMatchObject({ label: "edit", binds: 0, edits: 1 });
    expect(summarizeQueue([bind, edit])).toMatchObject({
      label: "bind+edit",
      binds: 1,
      edits: 1,
    });
  });
});

describe("clearQueue + saveQueue round-trip", () => {
  it("clearQueue empties the store", async () => {
    await appendBind({
      id: "ABCDEFGHJKMNPQ",
      type: "",
      description: "",
      vendor: "",
      part_number: "",
      location: "",
      notes: "",
    });
    await clearQueue();
    expect(loadQueue()).toEqual([]);
  });

  it("saveQueue accepts the loaded shape verbatim", async () => {
    await appendEdit("ABCDEFGHJKMNPQ", { vendor: "" }, { vendor: "TC" });
    const q = loadQueue();
    saveQueue(q);
    // saveQueue uses fire-and-forget internally (rebuilds from queue items)
    await new Promise((r) => setTimeout(r, 10));
    expect(loadQueue()).toEqual(q);
  });
});

describe("session migration", () => {
  it("migrates old localStorage bind queue items to session", async () => {
    // Simulate old queue format
    const legacy = [
      {
        kind: "bind",
        id: "ABCDEFGHJKMNPQ",
        queued_at: "2026-05-08T12:00:00Z",
        type: "PT100",
        description: "",
        vendor: "TC",
        part_number: "",
        location: "",
        notes: "",
      },
    ];
    localStorage.setItem("part-registry.bind-queue", JSON.stringify(legacy));

    const migrated = await session.migrateOldQueue();
    expect(migrated).toBe(1);

    const q = loadQueue();
    expect(q).toHaveLength(1);
    expect(q[0].kind).toBe("bind");
    expect(q[0].id).toBe("ABCDEFGHJKMNPQ");

    // Old key should be cleared
    expect(localStorage.getItem("part-registry.bind-queue")).toBeNull();
  });
});
