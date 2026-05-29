// Bulk CSV/TSV import → mint+bind rows (#176 P0).
//
// Pure, synchronous logic — parsing, CSV-injection escaping, column
// auto-detection, and row → ImportedRow conversion. The modal UI in
// ui/import-modal.ts drives this; the session queue is the sink.
//
// Design (from #176 review consolidation):
//   - reuse the bind queue as the staging table (no new store)
//   - auto-detect column mapping, operator confirms before commit
//   - a source column mapped to `id` holding a canonical ID → bind-only;
//     otherwise the row mints a fresh ID then binds
//   - escape CSV-injection (leading = + @ -) AND show the escaped value

import Papa from "papaparse";

import { ID_REGEX } from "../config";
import { REGISTRY_CONTRACT } from "./contract";
import { serializeMetadata, type MetadataValue } from "./metadata";

// ---- Target fields ----

/** Where a source column can map to. */
export type ImportTarget =
  | { kind: "field"; key: string } // top-level: id, batch, type, vendor, …
  | { kind: "metadata"; key: string } // a typeFields property → metadata JSON
  | { kind: "ignore" };

export interface TargetOption {
  /** Stable value for the mapping dropdown, e.g. "field:vendor" or
   *  "metadata:resistance_0c" or "ignore". */
  value: string;
  label: string;
  target: ImportTarget;
}

/** Top-level fields a bulk import may populate. `id`/`batch` are
 *  mint-level; the rest are bind fields. Excludes immutable/audit
 *  columns (minted_at, *_by, status) and the json metadata column
 *  (its keys are exposed individually via typeFields). */
const IMPORTABLE_FIELDS: { key: string; label: string }[] = [
  { key: "id", label: "ID (existing canonical)" },
  { key: "batch", label: "Batch" },
  { key: "type", label: "Type" },
  { key: "description", label: "Description" },
  { key: "vendor", label: "Vendor" },
  { key: "part_number", label: "Part number" },
  { key: "manufacturer_id", label: "Manufacturer ID" },
  { key: "location", label: "Location" },
  { key: "notes", label: "Notes" },
  { key: "components", label: "Components" },
];

/** All mapping targets the operator can choose, including each
 *  type-specific metadata property flattened from the contract's
 *  typeFields (deduplicated across types by key). */
export function targetOptions(): TargetOption[] {
  const opts: TargetOption[] = [
    { value: "ignore", label: "— Ignore —", target: { kind: "ignore" } },
  ];
  for (const f of IMPORTABLE_FIELDS) {
    opts.push({ value: `field:${f.key}`, label: f.label, target: { kind: "field", key: f.key } });
  }
  const seen = new Set<string>();
  const typeFields = REGISTRY_CONTRACT.typeFields ?? {};
  for (const fields of Object.values(typeFields)) {
    for (const tf of fields) {
      if (seen.has(tf.key)) continue;
      seen.add(tf.key);
      opts.push({
        value: `metadata:${tf.key}`,
        label: `Property: ${tf.label}${tf.unit ? ` (${tf.unit})` : ""}`,
        target: { kind: "metadata", key: tf.key },
      });
    }
  }
  return opts;
}

/** Resolve a dropdown value back to its ImportTarget. Malformed values
 *  (no key, unknown kind) resolve to "ignore" rather than producing a
 *  field with an undefined/empty key. */
export function parseTargetValue(value: string): ImportTarget {
  if (value === "ignore" || value === "") return { kind: "ignore" };
  const idx = value.indexOf(":");
  if (idx <= 0) return { kind: "ignore" };
  const kind = value.slice(0, idx);
  const key = value.slice(idx + 1);
  if (!key) return { kind: "ignore" };
  if (kind === "metadata") return { kind: "metadata", key };
  if (kind === "field") return { kind: "field", key };
  return { kind: "ignore" };
}

// ---- Parsing ----

export interface ParsedTable {
  headers: string[];
  rows: string[][];
  /** Count of source rows whose column count differed from the header
   *  width (padded if short, truncated if long). >0 → surface a warning
   *  so silent column drop/gain is visible (#176 hardening). */
  raggedRows: number;
}

/**
 * Parse pasted/uploaded delimited text. Auto-sniffs the delimiter
 * (tab vs comma vs semicolon) via papaparse. Treats the first
 * non-empty line as headers. Returns header names + body rows
 * (ragged rows are padded/truncated to the header width).
 */
export function parseDelimited(text: string): ParsedTable {
  const result = Papa.parse<string[]>(text.trim(), {
    delimiter: "", // auto-detect
    skipEmptyLines: "greedy",
  });
  const data = (result.data as string[][]).filter((r) => r.length > 0);
  if (data.length === 0) return { headers: [], rows: [], raggedRows: 0 };
  const headers = data[0].map((h) => h.trim());
  const width = headers.length;
  let raggedRows = 0;
  const rows = data.slice(1).map((r) => {
    if (r.length !== width) raggedRows++;
    const padded = r.slice(0, width);
    while (padded.length < width) padded.push("");
    return padded.map((c) => c ?? "");
  });
  return { headers, rows, raggedRows };
}

// ---- CSV-injection escaping ----

const INJECTION_PREFIXES = ["=", "+", "@", "\t", "\r"];
// A leading "-" is dangerous (it can start a formula like "-2+3", which
// evaluates to 1) UNLESS the whole value is a plain number ("-5",
// "-3.14"). So escape leading "-" unless the entire string parses as a
// finite number.
function isFormulaInjection(value: string): boolean {
  if (value.length === 0) return false;
  const first = value[0];
  if (INJECTION_PREFIXES.includes(first)) return true;
  if (first === "-") {
    return !(value.trim() !== "" && Number.isFinite(Number(value)));
  }
  return false;
}

/** Neutralize spreadsheet formula injection by prefixing a single
 *  quote. The escaped value is what gets stored AND shown in preview
 *  (never silently stripped). */
export function escapeInjection(value: string): string {
  return isFormulaInjection(value) ? `'${value}` : value;
}

// ---- Column auto-detection ----

const SYNONYMS: Record<string, string> = {
  // normalized header → target value
  id: "field:id",
  partid: "field:id",
  registryid: "field:id",
  canonicalid: "field:id",
  batch: "field:batch",
  lot: "field:batch",
  lotno: "field:batch",
  type: "field:type",
  category: "field:type",
  kind: "field:type",
  description: "field:description",
  desc: "field:description",
  vendor: "field:vendor",
  supplier: "field:vendor",
  manufacturer: "field:manufacturer_id",
  mfr: "field:manufacturer_id",
  mfg: "field:manufacturer_id",
  maker: "field:manufacturer_id",
  manufacturerid: "field:manufacturer_id",
  serial: "field:manufacturer_id",
  serialno: "field:manufacturer_id",
  sn: "field:manufacturer_id",
  partnumber: "field:part_number",
  partno: "field:part_number",
  pn: "field:part_number",
  partnum: "field:part_number",
  catalog: "field:part_number",
  catalogno: "field:part_number",
  location: "field:location",
  loc: "field:location",
  bin: "field:location",
  shelf: "field:location",
  notes: "field:notes",
  note: "field:notes",
  comment: "field:notes",
  comments: "field:notes",
  components: "field:components",
  children: "field:components",
};

function normalizeHeader(h: string): string {
  return h.toLowerCase().replace(/[^a-z0-9]/g, "");
}

/**
 * Auto-detect a mapping target value for each header. Strategy:
 *   1. exact synonym hit on the normalized header
 *   2. normalized header equals a metadata property key
 *   3. normalized header equals an importable field key
 *   4. otherwise "ignore"
 * Returns one dropdown value per header (aligned by index).
 */
export function autoDetectMapping(headers: string[]): string[] {
  const opts = targetOptions();
  const metaByNorm = new Map<string, string>();
  const fieldByNorm = new Map<string, string>();
  for (const o of opts) {
    if (o.target.kind === "metadata") metaByNorm.set(normalizeHeader(o.target.key), o.value);
    if (o.target.kind === "field") fieldByNorm.set(normalizeHeader(o.target.key), o.value);
  }
  return headers.map((h) => {
    const norm = normalizeHeader(h);
    if (!norm) return "ignore";
    if (SYNONYMS[norm]) return SYNONYMS[norm];
    if (metaByNorm.has(norm)) return metaByNorm.get(norm)!;
    if (fieldByNorm.has(norm)) return fieldByNorm.get(norm)!;
    return "ignore";
  });
}

// ---- Row → ImportedRow ----

export interface ImportedRow {
  /** Canonical ID if the row carries one; "" if it must be minted. */
  id: string;
  /** True → generate a fresh ID + emit a mint record. */
  mint: boolean;
  /** Mint batch (only meaningful when mint=true). */
  batch: string;
  /** Bind fields, including a serialized `metadata` JSON string when
   *  any metadata-mapped columns had values. Values are injection-escaped. */
  fields: Record<string, string>;
}

/**
 * Convert a parsed table + a per-column mapping into ImportedRows.
 * Classification: a column mapped to `id` holding a value matching
 * ID_REGEX → bind-only (mint=false); any other case → mint a fresh ID.
 */
export function buildImportedRows(
  table: Pick<ParsedTable, "rows">,
  mapping: string[],
): ImportedRow[] {
  const targets = mapping.map(parseTargetValue);
  return table.rows.map((row) => {
    const fields: Record<string, string> = {};
    const meta: Record<string, MetadataValue> = {};
    let id = "";
    let batch = "";

    for (let c = 0; c < targets.length; c++) {
      const t = targets[c];
      const raw = (row[c] ?? "").trim();
      if (t.kind === "ignore" || raw === "") continue;
      const val = escapeInjection(raw);
      if (t.kind === "metadata") {
        meta[t.key] = val;
      } else if (t.key === "id") {
        id = raw.toUpperCase().replace(/[\s-]/g, "");
      } else if (t.key === "batch") {
        batch = val;
      } else {
        fields[t.key] = val;
      }
    }

    if (Object.keys(meta).length > 0) {
      fields.metadata = serializeMetadata(meta);
    }

    const isCanonical = ID_REGEX.test(id);
    return {
      id: isCanonical ? id : "",
      mint: !isCanonical,
      batch,
      fields,
    };
  });
}
