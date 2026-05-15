// Unit tests for the pure CSV transformation helpers in submit.ts.
//
// These exercise parseCsv, serialiseCsv, splitCsvLine, applyBind, and
// applyEdit in isolation — no GitHub API, no network, no localStorage.

import { describe, it, expect } from "vitest";
import {
  parseCsv,
  serialiseCsv,
  splitCsvLine,
  applyBind,
  applyEdit,
} from "./submit";
import type { QueuedBind, QueuedEdit } from "./queue";

// -- splitCsvLine ---------------------------------------------------------

describe("splitCsvLine", () => {
  it("splits a simple comma-separated line", () => {
    expect(splitCsvLine("a,b,c")).toEqual(["a", "b", "c"]);
  });

  it("handles quoted fields containing commas", () => {
    expect(splitCsvLine('a,"b,c",d')).toEqual(["a", "b,c", "d"]);
  });

  it("handles escaped double quotes inside quoted fields", () => {
    expect(splitCsvLine('a,"say ""hi""",c')).toEqual(["a", 'say "hi"', "c"]);
  });

  it("handles empty fields", () => {
    expect(splitCsvLine("a,,c,")).toEqual(["a", "", "c", ""]);
  });

  it("handles a single-field line", () => {
    expect(splitCsvLine("only")).toEqual(["only"]);
  });

  it("handles quoted field with newline", () => {
    expect(splitCsvLine('"line1\nline2",b')).toEqual(["line1\nline2", "b"]);
  });
});

// -- parseCsv / serialiseCsv roundtrip ------------------------------------

describe("parseCsv", () => {
  it("parses header and rows keyed by first column", () => {
    const csv = "id,status,batch\nABC,unbound,B-001\nDEF,bound,B-002\n";
    const { header, rows } = parseCsv(csv);
    expect(header).toBe("id,status,batch");
    expect(rows.size).toBe(2);
    expect(rows.get("ABC")).toBe("ABC,unbound,B-001");
    expect(rows.get("DEF")).toBe("DEF,bound,B-002");
  });

  it("skips blank lines", () => {
    const csv = "id,status\nABC,unbound\n\n\nDEF,bound\n";
    const { rows } = parseCsv(csv);
    expect(rows.size).toBe(2);
  });

  it("normalises CRLF to LF", () => {
    const csv = "id,status\r\nABC,unbound\r\n";
    const { header, rows } = parseCsv(csv);
    expect(header).toBe("id,status");
    expect(rows.size).toBe(1);
  });
});

describe("serialiseCsv", () => {
  it("joins header and rows with trailing newline", () => {
    const rows = new Map([
      ["A", "A,unbound"],
      ["B", "B,bound"],
    ]);
    const result = serialiseCsv("id,status", rows);
    expect(result).toBe("id,status\nA,unbound\nB,bound\n");
  });
});

describe("parseCsv / serialiseCsv roundtrip", () => {
  it("roundtrips registry CSV through parse + serialise", () => {
    const original =
      "id,status,minted_at,batch\nABC,unbound,2026-01-01T00:00:00Z,B-001\nDEF,bound,2026-01-02T00:00:00Z,B-002\n";
    const { header, rows } = parseCsv(original);
    const result = serialiseCsv(header, rows);
    expect(result).toBe(original);
  });
});

// -- applyBind ------------------------------------------------------------

describe("applyBind", () => {
  const HEADER_COLS = [
    "id",
    "status",
    "minted_at",
    "batch",
    "bound_at",
    "type",
    "description",
    "vendor",
    "part_number",
    "location",
    "notes",
    "last_edited_at",
  ];

  function makeRows(...entries: [string, string][]): Map<string, string> {
    return new Map(entries);
  }

  it("sets status to bound and populates metadata fields", () => {
    const rows = makeRows([
      "ABC",
      "ABC,unbound,2026-01-01T00:00:00Z,B-001,,,,,,,,",
    ]);
    const bind: QueuedBind = {
      kind: "bind",
      id: "ABC",
      queued_at: "2026-05-01T00:00:00Z",
      type: "PT100",
      description: "sensor",
      vendor: "Acme",
      part_number: "X-42",
      location: "loop A",
      notes: "test note",
    };
    applyBind(HEADER_COLS, rows, bind);
    const line = rows.get("ABC")!;
    const fields = splitCsvLine(line);
    expect(fields[1]).toBe("bound"); // status
    expect(fields[5]).toBe("PT100"); // type
    expect(fields[6]).toBe("sensor"); // description
    expect(fields[7]).toBe("Acme"); // vendor
    expect(fields[8]).toBe("X-42"); // part_number
    expect(fields[9]).toBe("loop A"); // location
    expect(fields[10]).toBe("test note"); // notes
  });

  it("skips unknown IDs silently", () => {
    const rows = makeRows(["ABC", "ABC,unbound,,,,,,,,,,"]);
    const bind: QueuedBind = {
      kind: "bind",
      id: "NOPE",
      queued_at: "",
      type: "",
      description: "",
      vendor: "",
      part_number: "",
      location: "",
      notes: "",
    };
    applyBind(HEADER_COLS, rows, bind);
    // Row unchanged.
    expect(rows.size).toBe(1);
    expect(rows.get("ABC")).toBe("ABC,unbound,,,,,,,,,,");
  });

  it("preserves existing bound_at if already set", () => {
    const rows = makeRows([
      "ABC",
      "ABC,unbound,2026-01-01T00:00:00Z,B-001,2025-12-01T00:00:00Z,,,,,,,",
    ]);
    const bind: QueuedBind = {
      kind: "bind",
      id: "ABC",
      queued_at: "",
      type: "PT100",
      description: "",
      vendor: "",
      part_number: "",
      location: "",
      notes: "",
    };
    applyBind(HEADER_COLS, rows, bind);
    const fields = splitCsvLine(rows.get("ABC")!);
    // bound_at should keep the existing value.
    expect(fields[4]).toBe("2025-12-01T00:00:00Z");
  });
});

// -- applyEdit ------------------------------------------------------------

describe("applyEdit", () => {
  const HEADER_COLS = [
    "id",
    "status",
    "minted_at",
    "batch",
    "bound_at",
    "type",
    "description",
    "vendor",
    "part_number",
    "location",
    "notes",
    "last_edited_at",
  ];

  it("modifies only the changed fields", () => {
    const rows = new Map([
      [
        "ABC",
        "ABC,bound,2026-01-01T00:00:00Z,B-001,,PT100,old desc,Acme,X-42,loop A,note,",
      ],
    ]);
    const edit: QueuedEdit = {
      kind: "edit",
      id: "ABC",
      queued_at: "",
      before: { description: "old desc" },
      changes: { description: "new desc", location: "loop B" },
    };
    applyEdit(HEADER_COLS, rows, edit);
    const fields = splitCsvLine(rows.get("ABC")!);
    expect(fields[6]).toBe("new desc"); // description updated
    expect(fields[9]).toBe("loop B"); // location updated
    expect(fields[5]).toBe("PT100"); // type unchanged
    expect(fields[7]).toBe("Acme"); // vendor unchanged
  });

  it("skips unknown IDs silently", () => {
    const rows = new Map<string, string>();
    const edit: QueuedEdit = {
      kind: "edit",
      id: "NOPE",
      queued_at: "",
      before: {},
      changes: { description: "x" },
    };
    applyEdit(HEADER_COLS, rows, edit);
    expect(rows.size).toBe(0);
  });

  it("sets last_edited_at on every edit", () => {
    const rows = new Map([
      ["ABC", "ABC,bound,2026-01-01T00:00:00Z,B-001,,PT100,desc,,,,note,"],
    ]);
    const edit: QueuedEdit = {
      kind: "edit",
      id: "ABC",
      queued_at: "",
      before: {},
      changes: { notes: "updated" },
    };
    applyEdit(HEADER_COLS, rows, edit);
    const fields = splitCsvLine(rows.get("ABC")!);
    // last_edited_at (index 11) should be a non-empty ISO string.
    expect(fields[11]).toBeTruthy();
    expect(fields[11]).toContain("T");
  });
});
