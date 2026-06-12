// Field-level validation driven by the contract's per-field rules.
//
// Three primitives:
//   - required: value must be non-empty
//   - pattern: value must match a regex
//   - options + on_unknown: value must be in the allowed set (or warn)

import type { FieldDef } from "./schema";

export interface ValidationError {
  field: string;
  rule: string;
  message: string;
  severity: "error" | "warning";
}

/**
 * Validate a single field value against its contract-defined rules.
 * Returns an array of validation errors (empty = valid).
 */
export function validateField(
  key: string,
  value: string,
  fieldDef: FieldDef,
): ValidationError[] {
  const errors: ValidationError[] = [];
  const v = fieldDef.validation;

  // required
  if (v?.required && !value.trim()) {
    errors.push({
      field: key,
      rule: "required",
      message: `${fieldDef.label} is required`,
      severity: "error",
    });
    // If required and empty, skip further checks — they'd be noise.
    return errors;
  }

  // Skip remaining checks if value is empty (field is optional).
  if (!value.trim()) return errors;

  // pattern
  if (v?.pattern) {
    const re = new RegExp(v.pattern);
    if (!re.test(value)) {
      errors.push({
        field: key,
        rule: "pattern",
        message: `${fieldDef.label} does not match the required format`,
        severity: "error",
      });
    }
  }

  // maxLength
  if (v?.maxLength != null && value.length > v.maxLength) {
    errors.push({
      field: key,
      rule: "maxLength",
      message: `${fieldDef.label} must be at most ${v.maxLength} characters`,
      severity: "error",
    });
  }

  // min / max (for number-typed fields)
  if (fieldDef.type === "number") {
    const num = Number(value);
    if (!Number.isNaN(num)) {
      if (v?.min != null && num < v.min) {
        errors.push({
          field: key,
          rule: "min",
          message: `${fieldDef.label} must be at least ${v.min}`,
          severity: "error",
        });
      }
      if (v?.max != null && num > v.max) {
        errors.push({
          field: key,
          rule: "max",
          message: `${fieldDef.label} must be at most ${v.max}`,
          severity: "error",
        });
      }
    }
  }

  // options + on_unknown
  if (fieldDef.options && fieldDef.options.length > 0) {
    if (!fieldDef.options.includes(value)) {
      const severity = fieldDef.on_unknown === "block" ? "error" : "warning";
      errors.push({
        field: key,
        rule: "options",
        message: `${fieldDef.label}: "${value}" is not a known option`,
        severity,
      });
    }
  }

  return errors;
}
