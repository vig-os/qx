// FE view of the shared registry contract.
//
// Runtime schema facts come from `schema/registry-contract.json`, which
// is shared with the Python tooling. This module adds FE-specific types
// on top of that contract so forms and tables can stay typed.

import { REGISTRY_CONTRACT, type ValidationRules } from "./contract";

export type Status = "unbound" | "bound" | "void";
export const STATUSES = REGISTRY_CONTRACT.statuses as readonly Status[];

// Dynamic row type — fields are driven by the contract at runtime.
// Individual fields are always strings (CSV origin); the contract
// metadata (type, validation) guides rendering and validation.
export type RegistryRow = Record<string, string>;

// Field display + validation metadata — shared by table view and bind
// form, so adding a column adds it to both views with one edit.
export interface FieldDef {
  key: string;
  label: string;
  // Editable on bind form? (id/minted_at/batch are immutable post-mint.)
  editable: boolean;
  // Status that this field becomes meaningful at.
  meaningfulFrom?: Status;
  // Field type from contract — drives input rendering.
  type: "string" | "dropdown" | "yes-no" | "date" | "number" | "json";
  // Dropdown options from contract.
  options?: string[];
  // Behaviour when value is not in options.
  on_unknown?: "warn" | "block";
  // Validation rules from contract.
  validation?: ValidationRules;
}

export const FIELDS: readonly FieldDef[] = REGISTRY_CONTRACT.fields.map((field) => ({
  key: field.key,
  label: field.label,
  editable: field.editable,
  meaningfulFrom: field.meaningfulFrom as Status | undefined,
  type: field.type,
  options: field.options,
  on_unknown: field.on_unknown,
  validation: field.validation,
}));

export const REGISTRY_FIELD_KEYS = FIELDS.map((f) => f.key);
