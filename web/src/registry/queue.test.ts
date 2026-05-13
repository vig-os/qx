// Queue migration + bind/edit shape tests (#6).

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
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("queue migration", () => {
  it("treats legacy (un-kinded) items as binds", () => {
    // Pre-#6 shape: no `kind` field.
    const legacy = [
      {
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
    const q = loadQueue();
    expect(q).toHaveLength(1);
    expect(q[0].kind).toBe("bind");
    expect(q[0].id).toBe("ABCDEFGHJKMNPQ");
  });

  it("drops malformed entries", () => {
    localStorage.setItem(
      "part-registry.bind-queue",
      JSON.stringify([null, { foo: 1 }, "string"]),
    );
    expect(loadQueue()).toEqual([]);
  });

  it("returns [] on parse error", () => {
    localStorage.setItem("part-registry.bind-queue", "not json");
    expect(loadQueue()).toEqual([]);
  });
});

describe("appendBind / appendEdit / removeAt", () => {
  it("appends bind items with kind=bind and a queued_at timestamp", () => {
    appendBind({
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

  it("appends edit items carrying before+changes", () => {
    appendEdit(
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

  it("removeAt removes the right item even with mixed kinds", () => {
    appendBind({
      id: "ABCDEFGHJKMNPQ",
      type: "",
      description: "",
      vendor: "",
      part_number: "",
      location: "",
      notes: "",
    });
    appendEdit("ABCDEFGHJKMNPR", {}, { location: "lab" });
    appendBind({
      id: "ABCDEFGHJKMNPS",
      type: "",
      description: "",
      vendor: "",
      part_number: "",
      location: "",
      notes: "",
    });
    removeAt(1);
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
  it("clearQueue empties the store", () => {
    appendBind({
      id: "ABCDEFGHJKMNPQ",
      type: "",
      description: "",
      vendor: "",
      part_number: "",
      location: "",
      notes: "",
    });
    clearQueue();
    expect(loadQueue()).toEqual([]);
  });

  it("saveQueue accepts the loaded shape verbatim", () => {
    appendEdit("ABCDEFGHJKMNPQ", { vendor: "" }, { vendor: "TC" });
    const q = loadQueue();
    saveQueue(q);
    expect(loadQueue()).toEqual(q);
  });
});
