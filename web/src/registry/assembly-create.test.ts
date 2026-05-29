// Tests for assembly creation (composition: combine selected parts
// into a new minted assembly).

import { describe, expect, it } from "vitest";

import { ID_ALPHABET, ID_LENGTH, ID_REGEX } from "../config";
import type { RegistryRow } from "./schema";
import { mintUniqueId } from "./mint-id";
import {
  defaultAssemblyBatch,
  planAssembly,
  validateAssembly,
} from "./assembly-create";

function row(id: string, status: string, components = ""): RegistryRow {
  return { id, status, components } as unknown as RegistryRow;
}

const A = "AAAAAAAAAAAAAA";
const B = "BBBBBBBBBBBBBB";
const C = "CCCCCCCCCCCCCC";
const VOIDED = "DDDDDDDDDDDDDD";
const PARENT = "EEEEEEEEEEEEEE";

const rows: RegistryRow[] = [
  row(A, "bound"),
  row(B, "bound"),
  row(C, "bound"),
  row(VOIDED, "void"),
  row(PARENT, "bound", C), // C already belongs to PARENT
];

describe("mintUniqueId", () => {
  it("returns an id that satisfies isFree", () => {
    const id = mintUniqueId(ID_ALPHABET, ID_LENGTH, () => true);
    expect(id).toHaveLength(ID_LENGTH);
    expect(id).toMatch(ID_REGEX);
  });

  it("avoids taken ids in a constrained space", () => {
    // Space {AA, AB, BA, BB}; only BB is free → must return it.
    const taken = new Set(["AA", "AB", "BA"]);
    const id = mintUniqueId("AB", 2, (x) => !taken.has(x));
    expect(id).toBe("BB");
  });

  it("throws if no free id can be found", () => {
    expect(() => mintUniqueId("AB", 2, () => false)).toThrow();
  });
});

describe("validateAssembly", () => {
  it("requires at least two components", () => {
    const res = validateAssembly("NEWASSEMBLY001", [A], rows);
    expect(res.valid).toBe(false);
    expect(res.errors.join(" ")).toMatch(/at least two/i);
  });

  it("treats duplicates as a single component", () => {
    const res = validateAssembly("NEWASSEMBLY001", [A, A], rows);
    // De-duped to one → still below the minimum of two.
    expect(res.valid).toBe(false);
    expect(res.errors.join(" ")).toMatch(/at least two/i);
  });

  it("rejects a voided component", () => {
    const res = validateAssembly("NEWASSEMBLY001", [A, VOIDED], rows);
    expect(res.valid).toBe(false);
    expect(res.errors.join(" ")).toMatch(/void/i);
  });

  it("rejects a component already in another assembly", () => {
    const res = validateAssembly("NEWASSEMBLY001", [A, C], rows);
    expect(res.valid).toBe(false);
    expect(res.errors.join(" ")).toMatch(/already a component/i);
  });

  it("rejects an unknown component", () => {
    const res = validateAssembly("NEWASSEMBLY001", [A, "ZZZZZZZZZZZZZZ"], rows);
    expect(res.valid).toBe(false);
    expect(res.errors.join(" ")).toMatch(/not found/i);
  });

  it("accepts two distinct bound, unparented components", () => {
    const res = validateAssembly("NEWASSEMBLY001", [A, B], rows);
    expect(res.valid).toBe(true);
    expect(res.errors).toHaveLength(0);
  });
});

describe("planAssembly", () => {
  it("mints a unique id absent from the registry and reserved set", () => {
    const reserved = new Set(["RESERVED000001"]);
    const plan = planAssembly({ componentIds: [A, B] }, rows, reserved);
    expect(plan.assemblyId).toMatch(ID_REGEX);
    expect(rows.some((r) => r.id === plan.assemblyId)).toBe(false);
    expect(reserved.has(plan.assemblyId)).toBe(false);
  });

  it("serializes components sorted and de-duplicated", () => {
    const plan = planAssembly({ componentIds: [B, A, A] }, rows);
    // De-dupe preserves insertion order; serialization sorts.
    expect(plan.componentIds).toEqual([B, A]);
    expect(plan.serializedComponents).toBe([A, B].join(";"));
  });

  it("defaults the batch label and trims metadata", () => {
    const plan = planAssembly(
      { componentIds: [A, B], description: "  Power module  ", type: "  PSU  " },
      rows,
    );
    expect(plan.description).toBe("Power module");
    expect(plan.type).toBe("PSU");
    expect(plan.batch).toBe(defaultAssemblyBatch());
  });

  it("honors an explicit batch", () => {
    const plan = planAssembly({ componentIds: [A, B], batch: "MY-BATCH" }, rows);
    expect(plan.batch).toBe("MY-BATCH");
  });
});

describe("defaultAssemblyBatch", () => {
  it("formats as ASM-YYYY-MM-DD", () => {
    // Local-time constructor avoids timezone-dependent date rollover.
    expect(defaultAssemblyBatch(new Date(2026, 4, 29, 12, 0, 0))).toBe("ASM-2026-05-29");
  });
});
