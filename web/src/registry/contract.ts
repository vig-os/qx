import contractData from "@registry-contract";

export interface ValidationRules {
  required?: boolean;
  pattern?: string;
  maxLength?: number;
  min?: number;
  max?: number;
}

export interface ContractField {
  key: string;
  label: string;
  type: "string" | "dropdown" | "yes-no" | "date" | "number" | "json";
  editable: boolean;
  meaningfulFrom?: string;
  options?: string[];
  on_unknown?: "warn" | "block";
  validation?: ValidationRules;
}

/** Type-specific field definition (#171). Rendered in the bind form
 *  when the operator's selected `type` matches a typeFields key. */
export interface TypeField {
  key: string;
  label: string;
  type: "string" | "dropdown" | "yes-no" | "date" | "number";
  options?: string[];
  unit?: string;
  validation?: ValidationRules;
}

export interface RegistryContract {
  schema_version: number;
  id: {
    alphabet: string;
    canonicalLength: number;
    prefixLength: number;
    legacyCanonicalLength?: number;
  };
  statuses: string[];
  fields: ContractField[];
  /** Per-type metadata field definitions (#171). Keyed by part type. */
  typeFields?: Record<string, TypeField[]>;
}

/**
 * Graceful fallback: if the contract data is somehow malformed at
 * runtime, fall back to a minimal contract that treats every CSV
 * column header as a plain string field. This prevents a blank page.
 */
function parseContract(data: unknown): RegistryContract {
  const d = data as Record<string, unknown>;
  // Validate the minimum required shape.
  if (
    d &&
    typeof d === "object" &&
    Array.isArray(d.fields) &&
    d.fields.length > 0 &&
    Array.isArray(d.statuses) &&
    d.id &&
    typeof d.id === "object"
  ) {
    return d as unknown as RegistryContract;
  }
  // Fallback: minimal contract so the app doesn't crash.
  console.warn(
    "registry-contract.json failed validation; using fallback contract. " +
    "Fields will render as plain text inputs.",
  );
  return {
    schema_version: 0,
    id: {
      alphabet: "23456789ABCDEFGHJKMNPQRSTUVWXYZ",
      canonicalLength: 14,
      prefixLength: 8,
    },
    statuses: ["unbound", "bound", "void"],
    fields: [],
  };
}

export const REGISTRY_CONTRACT = parseContract(contractData);
