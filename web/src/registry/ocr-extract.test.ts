import { describe, it, expect } from "vitest";
import { extractFields, type FieldSuggestion } from "./ocr-extract";

function byField(s: FieldSuggestion[]): Record<string, string> {
  return Object.fromEntries(s.map((x) => [x.field, x.value]));
}

describe("extractFields — labelled patterns", () => {
  it("extracts P/N: and S/N: into part_number and manufacturer_id", () => {
    const m = byField(extractFields("P/N: 402-141\nS/N: SN-PT-0042"));
    expect(m.part_number).toBe("402-141");
    expect(m.manufacturer_id).toBe("SN-PT-0042");
  });

  it("extracts Model: and Mfr: into type and vendor", () => {
    const m = byField(extractFields("Model: PT100\nMfr: Omega"));
    expect(m.type).toBe("PT100");
    expect(m.vendor).toBe("Omega");
  });

  it("handles 'Label value' with no colon", () => {
    const m = byField(extractFields("PN 402-141\nSerial SN-99"));
    expect(m.part_number).toBe("402-141");
    expect(m.manufacturer_id).toBe("SN-99");
  });

  it("is case-insensitive on the label", () => {
    const m = byField(extractFields("p/n: ABC\nMODEL: XYZ"));
    expect(m.part_number).toBe("ABC");
    expect(m.type).toBe("XYZ");
  });

  it("strips wrapping quotes/brackets and collapses whitespace", () => {
    const m = byField(extractFields('Description: "Temperature   sensor"'));
    expect(m.description).toBe("Temperature sensor");
  });

  it("takes the first match per field", () => {
    const m = byField(extractFields("P/N: FIRST\nP/N: SECOND"));
    expect(m.part_number).toBe("FIRST");
  });

  it("records the matched label as `via`", () => {
    const s = extractFields("Manufacturer: Swagelok");
    expect(s).toContainEqual({ field: "vendor", value: "Swagelok", via: "manufacturer" });
  });
});

describe("extractFields — priority & disambiguation", () => {
  it("maps S/N to manufacturer_id, not part_number", () => {
    const m = byField(extractFields("S/N: 12345"));
    expect(m.manufacturer_id).toBe("12345");
    expect(m.part_number).toBeUndefined();
  });

  it("a full label distinguishes serial from part", () => {
    const m = byField(extractFields("Serial No: A1\nPart No: B2"));
    expect(m.manufacturer_id).toBe("A1");
    expect(m.part_number).toBe("B2");
  });
});

describe("extractFields — guards", () => {
  it("returns nothing for label-only lines with no value", () => {
    expect(extractFields("P/N:\nS/N:")).toEqual([]);
  });

  it("returns nothing for unlabelled noise", () => {
    expect(extractFields("just some text\nmade in germany 2024")).toEqual([]);
  });

  it("handles empty input", () => {
    expect(extractFields("")).toEqual([]);
  });

  it("ignores a label substring that isn't a leading token", () => {
    // "spn" should not match the "pn" alias mid-word.
    expect(byField(extractFields("xpn 999")).part_number).toBeUndefined();
  });
});
