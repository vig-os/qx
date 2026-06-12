// Session-registry merge tests (#115).

import { describe, expect, it } from "vitest";
import { mergedRegistryRows, uncommittedPrintIds } from "./session-registry";
import type { Session } from "./session";
import type { RegistryRow } from "./schema";

function makeSession(items: Session["items"]): Session {
  return {
    id: "test-session",
    createdAt: "2026-05-18T00:00:00Z",
    items,
  };
}

describe("mergedRegistryRows", () => {
  const committed: RegistryRow[] = [
    { id: "ID1", status: "unbound", batch: "B1", notes: "" },
    { id: "ID2", status: "bound", batch: "B1", notes: "" },
  ];

  it("returns committed rows unchanged when session is empty", () => {
    const result = mergedRegistryRows(committed, makeSession([]));
    expect(result).toHaveLength(2);
    expect(result[0].__pending).toBeUndefined();
    expect(result[1].__pending).toBeUndefined();
  });

  it("adds minted IDs as unbound rows with __pending", () => {
    const session = makeSession([
      { kind: "mint", id: "NEW1", batch: "B2", notes: "fresh", createdAt: "2026-05-18T01:00:00Z" },
    ]);
    const result = mergedRegistryRows(committed, session);
    expect(result).toHaveLength(3);
    const newRow = result.find((r) => r.id === "NEW1");
    expect(newRow).toBeDefined();
    expect(newRow!.status).toBe("unbound");
    expect(newRow!.__pending).toBe("true");
    expect(newRow!.batch).toBe("B2");
  });

  it("applies bind operations and marks pending", () => {
    const session = makeSession([
      { kind: "bind", id: "ID1", fields: { type: "PT100", vendor: "TC" }, createdAt: "2026-05-18T01:00:00Z" },
    ]);
    const result = mergedRegistryRows(committed, session);
    const row = result.find((r) => r.id === "ID1");
    expect(row!.status).toBe("bound");
    expect(row!.type).toBe("PT100");
    expect(row!.__pending).toBe("true");
  });

  it("applies edit operations", () => {
    const session = makeSession([
      { kind: "edit", id: "ID2", before: { notes: "" }, changes: { notes: "updated" }, createdAt: "2026-05-18T01:00:00Z" },
    ]);
    const result = mergedRegistryRows(committed, session);
    const row = result.find((r) => r.id === "ID2");
    expect(row!.notes).toBe("updated");
    expect(row!.__pending).toBe("true");
  });

  it("applies void operations", () => {
    const session = makeSession([
      { kind: "void", id: "ID1", reason: "damaged", createdAt: "2026-05-18T01:00:00Z" },
    ]);
    const result = mergedRegistryRows(committed, session);
    const row = result.find((r) => r.id === "ID1");
    expect(row!.status).toBe("void");
    expect(row!.__pending).toBe("true");
  });

  it("does not mutate original committed rows", () => {
    const original = { ...committed[0] };
    const session = makeSession([
      { kind: "bind", id: "ID1", fields: { type: "PT100" }, createdAt: "2026-05-18T01:00:00Z" },
    ]);
    mergedRegistryRows(committed, session);
    expect(committed[0]).toEqual(original);
  });
});

describe("uncommittedPrintIds", () => {
  const committed: RegistryRow[] = [
    { id: "ID1", status: "unbound", batch: "B1", notes: "" },
  ];

  it("returns empty for plan with all committed IDs", () => {
    const session = makeSession([]);
    expect(uncommittedPrintIds(["ID1"], committed, session)).toEqual([]);
  });

  it("returns IDs that exist only in session mints", () => {
    const session = makeSession([
      { kind: "mint", id: "NEW1", batch: "B2", notes: "", createdAt: "2026-05-18T01:00:00Z" },
    ]);
    expect(uncommittedPrintIds(["ID1", "NEW1"], committed, session)).toEqual(["NEW1"]);
  });

  it("does not flag IDs from non-mint session items", () => {
    const session = makeSession([
      { kind: "bind", id: "ID1", fields: {}, createdAt: "2026-05-18T01:00:00Z" },
    ]);
    expect(uncommittedPrintIds(["ID1"], committed, session)).toEqual([]);
  });
});
