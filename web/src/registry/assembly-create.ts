// Assembly creation — pure logic for combining selected parts into a
// new minted assembly (composition, not supersession). The selected
// parts stay "correct for themselves"; a fresh ID is minted whose
// `components` field references them. DOM-free so it can be unit-tested
// and reused outside the Lookup tab.

import { ID_ALPHABET, ID_LENGTH } from "../config";
import type { RegistryRow } from "./schema";
import { serializeComponents, validateComponents } from "./assembly-graph";
import { mintUniqueId } from "./mint-id";

export interface AssemblyDraft {
  componentIds: string[];
  description?: string;
  type?: string;
  batch?: string;
  notes?: string;
}

export interface AssemblyPlan {
  /** The freshly minted, collision-free assembly ID. */
  assemblyId: string;
  /** De-duplicated component IDs. */
  componentIds: string[];
  /** Sorted, semicolon-separated components string for the CSV. */
  serializedComponents: string;
  description: string;
  type: string;
  batch: string;
  notes: string;
}

export interface AssemblyValidation {
  valid: boolean;
  errors: string[];
}

/** Default batch label for assemblies, grouping by mint date. */
export function defaultAssemblyBatch(now: Date = new Date()): string {
  const yyyy = now.getFullYear();
  const mm = String(now.getMonth() + 1).padStart(2, "0");
  const dd = String(now.getDate()).padStart(2, "0");
  return `ASM-${yyyy}-${mm}-${dd}`;
}

/**
 * Validate a proposed assembly. Requires at least two distinct
 * components and reuses the #168 component rules (must exist, not be
 * void, not already belong to another assembly, no self-reference).
 *
 * `assemblyId` is the prospective parent — pass the already-minted ID
 * from `planAssembly` so the self-reference check is meaningful.
 */
export function validateAssembly(
  assemblyId: string,
  componentIds: string[],
  rows: ReadonlyArray<RegistryRow>,
): AssemblyValidation {
  const unique = [...new Set(componentIds)];
  const errors: string[] = [];
  if (unique.length < 2) {
    errors.push("Select at least two parts to combine into an assembly.");
  }
  const componentCheck = validateComponents(assemblyId, unique, rows);
  errors.push(...componentCheck.errors);
  return { valid: errors.length === 0, errors };
}

/**
 * Build the plan for a new assembly: mint a unique ID (avoiding both
 * existing registry rows and any `reserved` IDs already pending in the
 * session), and serialize the components. Pure — performs no I/O.
 */
export function planAssembly(
  draft: AssemblyDraft,
  rows: ReadonlyArray<RegistryRow>,
  reserved: ReadonlySet<string> = new Set(),
): AssemblyPlan {
  const componentIds = [...new Set(draft.componentIds)];
  const taken = new Set<string>(reserved);
  for (const row of rows) taken.add(row.id);

  const assemblyId = mintUniqueId(
    ID_ALPHABET,
    ID_LENGTH,
    (id) => !taken.has(id),
  );

  return {
    assemblyId,
    componentIds,
    serializedComponents: serializeComponents(componentIds),
    description: draft.description?.trim() ?? "",
    type: draft.type?.trim() ?? "",
    batch: draft.batch?.trim() || defaultAssemblyBatch(),
    notes: draft.notes?.trim() ?? "",
  };
}
