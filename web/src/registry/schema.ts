// FE view of the shared registry contract.
//
// Runtime schema facts come from `schema/registry-contract.json`, which
// is shared with the Python tooling. This module adds FE-specific types
// on top of that contract so forms and tables can stay typed.

import { REGISTRY_CONTRACT } from "./contract";

export type Status = "unbound" | "bound" | "void";
export const STATUSES = REGISTRY_CONTRACT.statuses as readonly Status[];

export interface RegistryRow {
  id: string;
  status: Status;
  minted_at: string;
  batch: string;
  bound_at: string;
  type: string;
  description: string;
  vendor: string;
  part_number: string;
  location: string;
  notes: string;
}

// Field display metadata — shared by table view and bind form, so
// adding a column adds it to both views with one edit.
export interface FieldDef {
  key: keyof RegistryRow;
  label: string;
  // Editable on bind form? (id/minted_at/batch are immutable post-mint.)
  editable: boolean;
  // Status that this field becomes meaningful at.
  meaningfulFrom?: Status;
}

export const FIELDS: readonly FieldDef[] = REGISTRY_CONTRACT.fields.map((field) => ({
  key: field.key as keyof RegistryRow,
  label: field.label,
  editable: field.editable,
  meaningfulFrom: field.meaningfulFrom as Status | undefined,
}));

export const REGISTRY_FIELD_KEYS = FIELDS.map((f) => f.key);
