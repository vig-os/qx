// Session store tests (#115, #117).
//
// IndexedDB is not available in jsdom/Node — the session store falls
// back to localStorage automatically. These tests verify the fallback
// path and the public API.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  loadSession,
  clearSession,
  addMint,
  addBind,
  addEdit,
  addVoid,
  removeItemAt,
  summarizeSession,
  migrateOldQueue,
  getSessionSync,
} from "./session";

function makeLocalStorage() {
  const store = new Map<string, string>();
  return {
    getItem(key: string) { return store.get(key) ?? null; },
    setItem(key: string, value: string) { store.set(key, value); },
    removeItem(key: string) { store.delete(key); },
    clear() { store.clear(); },
  };
}

beforeEach(async () => {
  vi.stubGlobal("localStorage", makeLocalStorage());
  // Seed the in-memory cache
  await clearSession();
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("session lifecycle", () => {
  it("starts with an empty session", async () => {
    const session = await loadSession();
    expect(session.items).toHaveLength(0);
    expect(session.id).toBeTruthy();
    expect(session.createdAt).toBeTruthy();
  });

  it("addMint adds a mint item", async () => {
    await addMint("ABCDEFGHJKMNPQ", "B-2026-05-18", "test notes");
    const session = await loadSession();
    expect(session.items).toHaveLength(1);
    expect(session.items[0].kind).toBe("mint");
    if (session.items[0].kind === "mint") {
      expect(session.items[0].id).toBe("ABCDEFGHJKMNPQ");
      expect(session.items[0].batch).toBe("B-2026-05-18");
      expect(session.items[0].notes).toBe("test notes");
    }
  });

  it("addBind adds a bind item", async () => {
    await addBind("ABCDEFGHJKMNPQ", { type: "PT100", vendor: "TC" });
    const session = await loadSession();
    expect(session.items).toHaveLength(1);
    expect(session.items[0].kind).toBe("bind");
  });

  it("addEdit adds an edit item", async () => {
    await addEdit("ABCDEFGHJKMNPQ", { vendor: "TC" }, { vendor: "TC Direct" });
    const session = await loadSession();
    expect(session.items).toHaveLength(1);
    expect(session.items[0].kind).toBe("edit");
  });

  it("addVoid adds a void item", async () => {
    await addVoid("ABCDEFGHJKMNPQ", "damaged");
    const session = await loadSession();
    expect(session.items).toHaveLength(1);
    expect(session.items[0].kind).toBe("void");
  });

  it("removeItemAt removes the right item", async () => {
    await addMint("ID1", "B1", "");
    await addBind("ID2", { type: "PT100" });
    await addMint("ID3", "B1", "");
    await removeItemAt(1);
    const session = await loadSession();
    expect(session.items).toHaveLength(2);
    expect(session.items.map((i) => i.id)).toEqual(["ID1", "ID3"]);
  });

  it("clearSession resets to empty", async () => {
    await addMint("ID1", "B1", "");
    await addBind("ID2", {});
    await clearSession();
    const session = await loadSession();
    expect(session.items).toHaveLength(0);
  });
});

describe("summarizeSession", () => {
  it("summarizes mixed session correctly", async () => {
    await addMint("ID1", "B1", "");
    await addMint("ID2", "B1", "");
    await addBind("ID3", {});
    await addEdit("ID4", {}, {});
    await addVoid("ID5", "broken");

    const session = await loadSession();
    const stats = summarizeSession(session);
    expect(stats.mints).toBe(2);
    expect(stats.binds).toBe(1);
    expect(stats.edits).toBe(1);
    expect(stats.voids).toBe(1);
    expect(stats.total).toBe(5);
    expect(stats.label).toBe("2 mints, 1 bind, 1 edit, 1 void");
  });

  it("returns 'empty' label for empty session", async () => {
    const session = await loadSession();
    const stats = summarizeSession(session);
    expect(stats.label).toBe("empty");
    expect(stats.total).toBe(0);
  });
});

describe("getSessionSync", () => {
  it("returns the cached session after loadSession", async () => {
    await addMint("ID1", "B1", "");
    const sync = getSessionSync();
    expect(sync).not.toBeNull();
    expect(sync!.items).toHaveLength(1);
  });
});

describe("migrateOldQueue", () => {
  it("migrates old bind queue items", async () => {
    const oldItems = [
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
      {
        kind: "edit",
        id: "ABCDEFGHJKMNPR",
        queued_at: "2026-05-08T13:00:00Z",
        before: { vendor: "TC" },
        changes: { vendor: "TC Direct" },
      },
    ];
    localStorage.setItem("part-registry.bind-queue", JSON.stringify(oldItems));

    const count = await migrateOldQueue();
    expect(count).toBe(2);

    const session = await loadSession();
    expect(session.items).toHaveLength(2);
    expect(session.items[0].kind).toBe("bind");
    expect(session.items[1].kind).toBe("edit");

    // Old key cleared
    expect(localStorage.getItem("part-registry.bind-queue")).toBeNull();
  });

  it("handles empty old queue gracefully", async () => {
    const count = await migrateOldQueue();
    expect(count).toBe(0);
  });

  it("handles malformed old queue", async () => {
    localStorage.setItem("part-registry.bind-queue", "not json");
    const count = await migrateOldQueue();
    expect(count).toBe(0);
  });
});
