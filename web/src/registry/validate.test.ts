// Validation tests driven by schema/validation-fixtures.json.
//
// Each fixture specifies a field key, a value, and an expected outcome
// (ok | error | warning) plus the rule that should trigger.

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

import { FIELDS, type FieldDef } from "./schema";
import { validateField } from "./validate";

interface Fixture {
  field: string;
  value: string;
  expect: "ok" | "error" | "warning";
  rule?: string;
}

const fixtures: Fixture[] = JSON.parse(
  readFileSync(
    resolve(import.meta.dirname, "../../../schema/validation-fixtures.json"),
    "utf-8",
  ),
);

function findField(key: string): FieldDef {
  const f = FIELDS.find((f) => f.key === key);
  if (!f) throw new Error(`Unknown field key in fixture: ${key}`);
  return f;
}

describe("validateField (contract-driven fixtures)", () => {
  for (const fixture of fixtures) {
    const label = `${fixture.field}=${JSON.stringify(fixture.value)} => ${fixture.expect}${fixture.rule ? ` (${fixture.rule})` : ""}`;
    it(label, () => {
      const fieldDef = findField(fixture.field);
      const errors = validateField(fixture.field, fixture.value, fieldDef);

      if (fixture.expect === "ok") {
        // No errors of any severity, OR only warnings when we expected ok.
        const blocking = errors.filter((e) => e.severity === "error");
        expect(blocking).toEqual([]);
        // For "ok", there should be no warnings either.
        expect(errors).toEqual([]);
      } else if (fixture.expect === "error") {
        const blocking = errors.filter((e) => e.severity === "error");
        expect(blocking.length).toBeGreaterThan(0);
        if (fixture.rule) {
          expect(blocking.some((e) => e.rule === fixture.rule)).toBe(true);
        }
      } else if (fixture.expect === "warning") {
        const warnings = errors.filter((e) => e.severity === "warning");
        expect(warnings.length).toBeGreaterThan(0);
        if (fixture.rule) {
          expect(warnings.some((e) => e.rule === fixture.rule)).toBe(true);
        }
        // No blocking errors.
        const blocking = errors.filter((e) => e.severity === "error");
        expect(blocking).toEqual([]);
      }
    });
  }
});

describe("validateField (unit)", () => {
  it("returns empty array for valid optional string field", () => {
    const fieldDef = findField("notes");
    expect(validateField("notes", "hello", fieldDef)).toEqual([]);
  });

  it("returns empty array for empty optional field", () => {
    const fieldDef = findField("notes");
    expect(validateField("notes", "", fieldDef)).toEqual([]);
  });

  it("returns error for required field with empty value", () => {
    const fieldDef = findField("id");
    const errs = validateField("id", "", fieldDef);
    expect(errs).toHaveLength(1);
    expect(errs[0].severity).toBe("error");
    expect(errs[0].rule).toBe("required");
  });

  it("returns error for pattern mismatch", () => {
    const fieldDef = findField("id");
    const errs = validateField("id", "bad!", fieldDef);
    expect(errs.some((e) => e.rule === "pattern")).toBe(true);
  });

  it("returns warning for unknown vendor (on_unknown: warn)", () => {
    const fieldDef = findField("vendor");
    const errs = validateField("vendor", "NewVendor", fieldDef);
    expect(errs).toHaveLength(1);
    expect(errs[0].severity).toBe("warning");
    expect(errs[0].rule).toBe("options");
  });

  it("returns error for unknown status (on_unknown: block)", () => {
    const fieldDef = findField("status");
    const errs = validateField("status", "invalid", fieldDef);
    expect(errs).toHaveLength(1);
    expect(errs[0].severity).toBe("error");
    expect(errs[0].rule).toBe("options");
  });
});
