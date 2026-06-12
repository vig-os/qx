import { describe, it, expect } from "vitest";
import { matchOcrText } from "./ocr-match";
import type { RegistryRow } from "./schema";

function row(partial: Partial<RegistryRow> & { id: string }): RegistryRow {
  return { status: "bound", ...partial } as RegistryRow;
}

const REGISTRY: RegistryRow[] = [
  row({ id: "3456ABCDEFGHJK", manufacturer_id: "SN-PT-0042", part_number: "402-141" }),
  row({ id: "456789ABCDEFGH", manufacturer_id: "SWG-SS400-99", part_number: "SS-400-1-4" }),
  row({ id: "BCDEFGHJKMNPQR", manufacturer_id: "", part_number: "PX309-150GI" }),
];

describe("matchOcrText — canonical IDs", () => {
  it("matches a plain-printed canonical ID verbatim", () => {
    const m = matchOcrText("part 3456ABCDEFGHJK label", REGISTRY);
    expect(m).toEqual([{ id: "3456ABCDEFGHJK", via: "id", matched: "3456ABCDEFGHJK" }]);
  });

  it("matches an ID printed in 4-char groups with spaces", () => {
    const m = matchOcrText("3456 ABCD EFGH JK", REGISTRY);
    // "3456 ABCD EFGH JK" squashes per-chunk, not across whitespace, so
    // grouped IDs only collapse when in one whitespace-free token.
    expect(m.find((x) => x.id === "3456ABCDEFGHJK")).toBeUndefined();
  });

  it("matches an ID printed with dashes (single token)", () => {
    const m = matchOcrText("ID: 3456-ABCD-EFGH-JK", REGISTRY);
    expect(m).toContainEqual({ id: "3456ABCDEFGHJK", via: "id", matched: "3456ABCDEFGHJK" });
  });

  it("surfaces a well-formed ID even if not in the registry", () => {
    const m = matchOcrText("EFGHJKMNPQRSTU", REGISTRY);
    expect(m).toContainEqual({ id: "EFGHJKMNPQRSTU", via: "id", matched: "EFGHJKMNPQRSTU" });
  });
});

describe("matchOcrText — manufacturer/part labels", () => {
  it("matches by manufacturer_id", () => {
    const m = matchOcrText("Serial: SN-PT-0042\nMade in DE", REGISTRY);
    expect(m).toContainEqual({
      id: "3456ABCDEFGHJK",
      via: "manufacturer_id",
      matched: "SN-PT-0042",
    });
  });

  it("matches by manufacturer_id ignoring punctuation/case", () => {
    const m = matchOcrText("snpt0042", REGISTRY);
    expect(m).toContainEqual({
      id: "3456ABCDEFGHJK",
      via: "manufacturer_id",
      matched: "SN-PT-0042",
    });
  });

  it("matches by part_number when no manufacturer_id hit", () => {
    const m = matchOcrText("Honeywell PX309-150GI pressure", REGISTRY);
    expect(m).toContainEqual({
      id: "BCDEFGHJKMNPQR",
      via: "part_number",
      matched: "PX309-150GI",
    });
  });

  it("prefers manufacturer_id over part_number on the same row", () => {
    // Text contains both SWG-SS400-99 (mfr) and SS-400-1-4 (part) of row 2.
    const m = matchOcrText("SWG-SS400-99 / SS-400-1-4", REGISTRY);
    const hit = m.find((x) => x.id === "456789ABCDEFGH");
    expect(hit?.via).toBe("manufacturer_id");
  });
});

describe("matchOcrText — guards & dedup", () => {
  it("returns nothing for unrelated text", () => {
    expect(matchOcrText("just some random words", REGISTRY)).toEqual([]);
  });

  it("does not match very short part numbers (< 4 squashed chars)", () => {
    const reg = [row({ id: "3456ABCDEFGHJK", part_number: "A1" })];
    expect(matchOcrText("the value is A1 here", reg)).toEqual([]);
  });

  it("dedups: one entry per part even if matched multiple ways", () => {
    const m = matchOcrText("3456ABCDEFGHJK SN-PT-0042", REGISTRY);
    const forRow = m.filter((x) => x.id === "3456ABCDEFGHJK");
    expect(forRow).toHaveLength(1);
    // Direct ID hit wins over manufacturer hit.
    expect(forRow[0].via).toBe("id");
  });

  it("handles empty text and empty registry", () => {
    expect(matchOcrText("", REGISTRY)).toEqual([]);
    expect(matchOcrText("anything", [])).toEqual([]);
  });
});
