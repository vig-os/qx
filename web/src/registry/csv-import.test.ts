import { describe, it, expect } from "vitest";
import {
  parseDelimited,
  escapeInjection,
  autoDetectMapping,
  buildImportedRows,
  parseTargetValue,
  targetOptions,
} from "./csv-import";

describe("parseDelimited", () => {
  it("parses comma-separated with a header row", () => {
    const t = parseDelimited("vendor,part_number\nOmega,402-141\nSwagelok,SS-400");
    expect(t.headers).toEqual(["vendor", "part_number"]);
    expect(t.rows).toEqual([["Omega", "402-141"], ["Swagelok", "SS-400"]]);
  });

  it("auto-detects tab delimiter (clipboard TSV)", () => {
    const t = parseDelimited("vendor\tpart_number\nOmega\t402-141");
    expect(t.headers).toEqual(["vendor", "part_number"]);
    expect(t.rows).toEqual([["Omega", "402-141"]]);
  });

  it("auto-detects semicolon delimiter", () => {
    const t = parseDelimited("vendor;loc\nOmega;Lab-1");
    expect(t.headers).toEqual(["vendor", "loc"]);
    expect(t.rows).toEqual([["Omega", "Lab-1"]]);
  });

  it("handles quoted fields with embedded commas", () => {
    const t = parseDelimited('vendor,notes\nOmega,"a, b, c"');
    expect(t.rows[0]).toEqual(["Omega", "a, b, c"]);
  });

  it("pads short rows to header width and counts them ragged", () => {
    const t = parseDelimited("a,b,c\n1,2");
    expect(t.rows[0]).toEqual(["1", "2", ""]);
    expect(t.raggedRows).toBe(1);
  });

  it("truncates rows wider than the header and counts them ragged", () => {
    const t = parseDelimited("a,b\n1,2,3");
    expect(t.rows[0]).toEqual(["1", "2"]); // column 3 dropped
    expect(t.raggedRows).toBe(1);
  });

  it("reports raggedRows=0 when every row matches the header width", () => {
    const t = parseDelimited("a,b\n1,2\n3,4");
    expect(t.raggedRows).toBe(0);
  });

  it("keeps duplicate headers as separate positional columns", () => {
    const t = parseDelimited("a,a\n1,2");
    expect(t.headers).toEqual(["a", "a"]);
    expect(t.rows[0]).toEqual(["1", "2"]);
  });

  it("parses a single-column paste (no delimiter)", () => {
    const t = parseDelimited("id\n89ABCDEFGHJKMN\nABCDEFGHJKMNPQ");
    expect(t.headers).toEqual(["id"]);
    expect(t.rows).toEqual([["89ABCDEFGHJKMN"], ["ABCDEFGHJKMNPQ"]]);
  });

  it("handles CRLF line endings without leaking \\r into cells", () => {
    const t = parseDelimited("vendor,loc\r\nOmega,Lab-1\r\n");
    expect(t.headers).toEqual(["vendor", "loc"]);
    expect(t.rows).toEqual([["Omega", "Lab-1"]]);
  });

  it("returns no data rows for a header-only paste", () => {
    const t = parseDelimited("vendor,loc");
    expect(t.headers).toEqual(["vendor", "loc"]);
    expect(t.rows).toEqual([]);
  });

  it("skips blank lines and trims headers", () => {
    const t = parseDelimited("  vendor , loc \n\nOmega,Lab-1\n\n");
    expect(t.headers).toEqual(["vendor", "loc"]);
    expect(t.rows).toEqual([["Omega", "Lab-1"]]);
  });

  it("returns empty for empty input", () => {
    expect(parseDelimited("")).toEqual({ headers: [], rows: [], raggedRows: 0 });
    expect(parseDelimited("   \n  ")).toEqual({ headers: [], rows: [], raggedRows: 0 });
  });
});

describe("escapeInjection", () => {
  it("escapes leading formula chars", () => {
    expect(escapeInjection("=SUM(A1)")).toBe("'=SUM(A1)");
    expect(escapeInjection("+1")).toBe("'+1");
    expect(escapeInjection("@cmd")).toBe("'@cmd");
  });

  it("escapes a leading dash followed by a non-number", () => {
    expect(escapeInjection("-2+3")).toBe("'-2+3");
    expect(escapeInjection("-cmd")).toBe("'-cmd");
  });

  it("leaves legitimate negative numbers alone", () => {
    expect(escapeInjection("-5")).toBe("-5");
    expect(escapeInjection("-3.14")).toBe("-3.14");
  });

  it("leaves ordinary values alone", () => {
    expect(escapeInjection("Omega")).toBe("Omega");
    expect(escapeInjection("402-141")).toBe("402-141");
    expect(escapeInjection("")).toBe("");
  });
});

describe("autoDetectMapping", () => {
  it("maps common synonyms", () => {
    const m = autoDetectMapping(["P/N", "Mfr", "Vendor", "Location", "Notes"]);
    expect(m).toEqual([
      "field:part_number",
      "field:manufacturer_id",
      "field:vendor",
      "field:location",
      "field:notes",
    ]);
  });

  it("maps exact registry field keys", () => {
    expect(autoDetectMapping(["part_number", "manufacturer_id"]))
      .toEqual(["field:part_number", "field:manufacturer_id"]);
  });

  it("maps a typeFields property by key", () => {
    // resistance_0c is a PT100 typeField in the contract
    const m = autoDetectMapping(["resistance_0c"]);
    expect(m[0]).toBe("metadata:resistance_0c");
  });

  it("ignores unrecognized headers", () => {
    expect(autoDetectMapping(["whatever", ""])).toEqual(["ignore", "ignore"]);
  });

  it("is case/punctuation insensitive", () => {
    expect(autoDetectMapping(["Part No.", "S/N"]))
      .toEqual(["field:part_number", "field:manufacturer_id"]);
  });
});

describe("parseTargetValue / targetOptions", () => {
  it("round-trips field and metadata values", () => {
    expect(parseTargetValue("field:vendor")).toEqual({ kind: "field", key: "vendor" });
    expect(parseTargetValue("metadata:resistance_0c")).toEqual({ kind: "metadata", key: "resistance_0c" });
    expect(parseTargetValue("ignore")).toEqual({ kind: "ignore" });
    expect(parseTargetValue("")).toEqual({ kind: "ignore" });
  });

  it("treats malformed target values as ignore", () => {
    expect(parseTargetValue("field")).toEqual({ kind: "ignore" });   // no colon
    expect(parseTargetValue("metadata:")).toEqual({ kind: "ignore" }); // empty key
    expect(parseTargetValue("bogus:x")).toEqual({ kind: "ignore" });  // unknown kind
    expect(parseTargetValue(":vendor")).toEqual({ kind: "ignore" });  // empty kind
  });

  it("a value mapped to a metadata key serializes (not a top-level field)", () => {
    const rows = buildImportedRows({ rows: [["100"]] }, ["metadata:resistance_0c"]);
    expect(rows[0].fields.resistance_0c).toBeUndefined();
    expect(JSON.parse(rows[0].fields.metadata)).toEqual({ resistance_0c: "100" });
  });

  it("offers ignore + every importable field + flattened metadata props", () => {
    const opts = targetOptions();
    expect(opts[0].value).toBe("ignore");
    expect(opts.some((o) => o.value === "field:id")).toBe(true);
    expect(opts.some((o) => o.value === "field:manufacturer_id")).toBe(true);
    expect(opts.some((o) => o.value === "metadata:resistance_0c")).toBe(true);
  });
});

describe("buildImportedRows", () => {
  const VALID_ID = "3456ABCDEFGHJK"; // matches ID_REGEX (14 chars)

  it("mints a fresh ID when no canonical id column is mapped", () => {
    const table = { headers: ["vendor", "pn"], rows: [["Omega", "402-141"]] };
    const rows = buildImportedRows(table, ["field:vendor", "field:part_number"]);
    expect(rows).toHaveLength(1);
    expect(rows[0].mint).toBe(true);
    expect(rows[0].id).toBe("");
    expect(rows[0].fields).toEqual({ vendor: "Omega", part_number: "402-141" });
  });

  it("is bind-only when a column holds a valid canonical ID", () => {
    const table = { headers: ["id", "vendor"], rows: [[VALID_ID, "Omega"]] };
    const rows = buildImportedRows(table, ["field:id", "field:vendor"]);
    expect(rows[0].mint).toBe(false);
    expect(rows[0].id).toBe(VALID_ID);
    expect(rows[0].fields).toEqual({ vendor: "Omega" });
  });

  it("normalizes a dashed/spaced canonical ID", () => {
    const table = { headers: ["id"], rows: [["3456-ABCD-EFGH-JK"]] };
    const rows = buildImportedRows(table, ["field:id"]);
    expect(rows[0].id).toBe(VALID_ID);
    expect(rows[0].mint).toBe(false);
  });

  it("mints when the id column value is not a valid canonical ID", () => {
    const table = { headers: ["id", "vendor"], rows: [["NOT-AN-ID", "Omega"]] };
    const rows = buildImportedRows(table, ["field:id", "field:vendor"]);
    expect(rows[0].mint).toBe(true);
    expect(rows[0].id).toBe("");
  });

  it("serializes metadata-mapped columns into the metadata JSON field", () => {
    const table = {
      headers: ["type", "resistance_0c", "accuracy_class"],
      rows: [["PT100", "100", "A"]],
    };
    const rows = buildImportedRows(table, [
      "field:type",
      "metadata:resistance_0c",
      "metadata:accuracy_class",
    ]);
    expect(rows[0].fields.type).toBe("PT100");
    // Parse + compare (don't couple to key-ordering/quoting).
    expect(JSON.parse(rows[0].fields.metadata)).toEqual({
      resistance_0c: "100",
      accuracy_class: "A",
    });
  });

  it("last-mapped column wins when two columns map to the same field", () => {
    const table = { rows: [["first", "second"]] };
    const rows = buildImportedRows(table, ["field:vendor", "field:vendor"]);
    expect(rows[0].fields.vendor).toBe("second");
  });

  it("captures batch for mint and ignores ignored columns", () => {
    const table = {
      headers: ["batch", "vendor", "junk"],
      rows: [["B-2026", "Omega", "x"]],
    };
    const rows = buildImportedRows(table, ["field:batch", "field:vendor", "ignore"]);
    expect(rows[0].batch).toBe("B-2026");
    expect(rows[0].fields).toEqual({ vendor: "Omega" });
  });

  it("escapes injection in field values", () => {
    const table = { headers: ["notes"], rows: [["=HYPERLINK(evil)"]] };
    const rows = buildImportedRows(table, ["field:notes"]);
    expect(rows[0].fields.notes).toBe("'=HYPERLINK(evil)");
  });

  it("skips empty cells (no empty-string fields)", () => {
    const table = { headers: ["vendor", "notes"], rows: [["Omega", ""]] };
    const rows = buildImportedRows(table, ["field:vendor", "field:notes"]);
    expect(rows[0].fields).toEqual({ vendor: "Omega" });
  });
});
