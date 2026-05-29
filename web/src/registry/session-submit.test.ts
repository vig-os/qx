// Session-submit tests (#115).

import { describe, expect, it } from "vitest";
import {
  sessionToSubmitPayload,
  applyMints,
  buildSessionCommitMessage,
} from "./session-submit";
import type { Session } from "./session";

function makeSession(items: Session["items"]): Session {
  return {
    id: "test-session",
    createdAt: "2026-05-18T00:00:00Z",
    items,
  };
}

describe("sessionToSubmitPayload", () => {
  it("separates mints from binds/edits/voids", () => {
    const session = makeSession([
      { kind: "mint", id: "M1", batch: "B1", notes: "", createdAt: "2026-05-18T01:00:00Z" },
      { kind: "bind", id: "B1", fields: { type: "PT100" }, createdAt: "2026-05-18T01:00:00Z" },
      { kind: "edit", id: "E1", before: {}, changes: { notes: "x" }, createdAt: "2026-05-18T01:00:00Z" },
      { kind: "void", id: "V1", reason: "broken", createdAt: "2026-05-18T01:00:00Z" },
    ]);

    const { queueItems, mintRows, summary: _summary } = sessionToSubmitPayload(session);
    expect(mintRows).toHaveLength(1);
    expect(mintRows[0].id).toBe("M1");
    expect(queueItems).toHaveLength(3);
    expect(queueItems[0].kind).toBe("bind");
    expect(queueItems[1].kind).toBe("edit");
    expect(queueItems[2].kind).toBe("edit"); // void becomes edit
  });

  it("handles empty session", () => {
    const { queueItems, mintRows } = sessionToSubmitPayload(makeSession([]));
    expect(queueItems).toHaveLength(0);
    expect(mintRows).toHaveLength(0);
  });

  it("preserves all bind fields through the session→submit conversion", () => {
    // Regression: every bind field carried in the session must survive
    // into the QueueItem, otherwise it's silently dropped before reaching
    // the CSV. components (#168) and manufacturer_id/metadata (#171) were
    // both lost this way until carried through explicitly.
    const session = makeSession([
      {
        kind: "bind",
        id: "ASM1",
        fields: {
          description: "Module",
          components: "CHILD000000001;CHILD000000002",
          manufacturer_id: "MFR-42",
          metadata: '{"resistance":"100"}',
        },
        createdAt: "2026-05-18T01:00:00Z",
      },
    ]);
    const { queueItems } = sessionToSubmitPayload(session);
    expect(queueItems).toHaveLength(1);
    expect(queueItems[0].kind).toBe("bind");
    const bind = queueItems[0] as {
      components: string;
      manufacturer_id: string;
      metadata: string;
    };
    expect(bind.components).toBe("CHILD000000001;CHILD000000002");
    expect(bind.manufacturer_id).toBe("MFR-42");
    expect(bind.metadata).toBe('{"resistance":"100"}');
  });
});

describe("applyMints", () => {
  const csv = "id,status,minted_at,batch,notes\nEXIST1,bound,2026-01-01T00:00:00Z,B0,\n";

  it("adds new rows for minted IDs", () => {
    const result = applyMints(csv, [
      { id: "NEW1", batch: "B1", notes: "fresh", mintedAt: "2026-05-18T01:00:00Z" },
    ]);
    expect(result).toContain("NEW1");
    expect(result).toContain("unbound");
    expect(result).toContain("B1");
    // Original row still present
    expect(result).toContain("EXIST1");
  });

  it("skips IDs that already exist in CSV", () => {
    const result = applyMints(csv, [
      { id: "EXIST1", batch: "B1", notes: "", mintedAt: "2026-05-18T01:00:00Z" },
    ]);
    // Should have exactly the same number of data lines
    const lines = result.trim().split("\n");
    expect(lines).toHaveLength(2); // header + 1 existing row
  });

  it("handles empty mint list", () => {
    expect(applyMints(csv, [])).toBe(csv);
  });
});

describe("buildSessionCommitMessage", () => {
  it("builds commit message with all operation types", () => {
    const session = makeSession([
      { kind: "mint", id: "M1", batch: "B1", notes: "", createdAt: "2026-05-18T01:00:00Z" },
      { kind: "mint", id: "M2", batch: "B1", notes: "", createdAt: "2026-05-18T01:00:00Z" },
      { kind: "bind", id: "B1", fields: {}, createdAt: "2026-05-18T01:00:00Z" },
      { kind: "void", id: "V1", reason: "bad", createdAt: "2026-05-18T01:00:00Z" },
    ]);

    const { commitMessage, prBody } = buildSessionCommitMessage(session);
    expect(commitMessage).toContain("2 mints");
    expect(commitMessage).toContain("1 bind");
    expect(commitMessage).toContain("1 void");
    expect(commitMessage).toContain("via web UI");
    expect(prBody).toContain("Changes:");
    expect(prBody).toContain("IDs:");
  });
});
