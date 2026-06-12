// SSoT test helper — all field lists, statuses, and ID constants
// come from the shared registry contract so tests drift-detect
// automatically when the schema changes.

import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const contract = require('../../../../schema/registry-contract.json') as {
  schema_version: number;
  id: { alphabet: string; canonicalLength: number; prefixLength: number; legacyCanonicalLength: number };
  statuses: string[];
  fields: Array<{ key: string; label: string; type: string; editable: boolean; meaningfulFrom?: string; options?: string[]; on_unknown?: string; validation?: Record<string, unknown> }>;
};

export { contract };

/** Every field key in column order. */
export const FIELD_KEYS = contract.fields.map((f) => f.key);

/** Only the editable field keys. */
export const EDITABLE_KEYS = contract.fields
  .filter((f) => f.editable)
  .map((f) => f.key);

/** Canonical status list. */
export const STATUSES = contract.statuses;

/** Canonical ID alphabet per ADR-012. */
export const ID_ALPHABET = contract.id.alphabet;

/** Canonical ID length (14 chars). */
export const ID_LENGTH = contract.id.canonicalLength;

/**
 * Build a CSV header line from the contract field keys.
 * Matches the REGISTRY_HEADER constant in smoke.spec.ts.
 */
export const REGISTRY_HEADER =
  contract.fields.map((f) => f.key).join(',') + '\n';
