// OCR text → suggested mint+bind field values (#176 P1).
//
// Heuristic, REVERSIBLE field extraction from a manufacturer label's
// OCR text. Per the #176 review: operator-assisted assignment is the
// spine; this regex layer is a *suggestion* the operator confirms or
// overrides — never an authority. Every suggestion is editable in the
// mint-from-label form, and a wrong guess must look like a guess.
//
// Pure + synchronous → exhaustively unit-testable. The overlay in
// ui/ocr-extract-scan.ts feeds OCR text in and renders the suggestions
// as pre-filled (but visibly "guessed") inputs.

/** A bind field we try to pre-fill from a label. */
export type ExtractField =
  | "manufacturer_id"
  | "part_number"
  | "type"
  | "description"
  | "vendor";

export interface FieldSuggestion {
  field: ExtractField;
  value: string;
  /** The label token that triggered the match (for UI provenance), or
   *  "" when inferred without an explicit label. */
  via: string;
}

// Labelled-pattern rules. Each maps a set of label aliases to a target
// field. Order matters: earlier rules win a token if it would match
// multiple (e.g. "S/N" → manufacturer_id before a bare number rule).
interface LabelRule {
  field: ExtractField;
  /** Label aliases as they might appear before a ":" or whitespace. */
  labels: string[];
}

const LABEL_RULES: LabelRule[] = [
  { field: "manufacturer_id", labels: ["s/n", "sn", "serial", "serial no", "serialno", "ser"] },
  { field: "part_number", labels: ["p/n", "pn", "part no", "partno", "part number", "part", "cat", "cat no", "catalog", "catalogue", "ref"] },
  { field: "type", labels: ["model", "type", "mdl"] },
  { field: "vendor", labels: ["mfr", "mfg", "manufacturer", "made by", "maker", "vendor", "brand"] },
  { field: "description", labels: ["desc", "description", "product"] },
];

// Normalize a label token for comparison: lowercase, collapse internal
// whitespace, strip a trailing colon.
function normLabel(s: string): string {
  return s.toLowerCase().replace(/\s+/g, " ").replace(/:\s*$/, "").trim();
}

const LABEL_LOOKUP: Map<string, ExtractField> = (() => {
  const m = new Map<string, ExtractField>();
  for (const rule of LABEL_RULES) {
    for (const label of rule.labels) {
      // First rule to claim a label wins (LABEL_RULES order = priority).
      if (!m.has(label)) m.set(label, rule.field);
    }
  }
  return m;
})();

/** Strip surrounding noise from an extracted value: trim, drop wrapping
 *  quotes/brackets, collapse internal whitespace. */
function cleanValue(v: string): string {
  return v.trim().replace(/^["'(\[]+|["')\]]+$/g, "").replace(/\s{2,}/g, " ").trim();
}

/**
 * Extract field suggestions from OCR label text. Strategy:
 *   - Scan each line for a "Label: value" or "Label value" shape where
 *     Label matches a known alias. The value is the rest of the line
 *     (or the next whitespace-delimited token if the line is just the
 *     label).
 *   - First match per field wins (labels are usually unique on a label).
 *
 * Returns at most one suggestion per field. Empty/garbage values are
 * dropped. Never throws.
 */
export function extractFields(text: string): FieldSuggestion[] {
  const out = new Map<ExtractField, FieldSuggestion>();
  if (!text) return [];

  const lines = text.split(/\r?\n/);
  for (const rawLine of lines) {
    const line = rawLine.trim();
    if (!line) continue;

    // Try "Label: value" first (explicit colon), then "Label value".
    const colonIdx = line.indexOf(":");
    if (colonIdx > 0) {
      const label = normLabel(line.slice(0, colonIdx));
      const field = LABEL_LOOKUP.get(label);
      if (field && !out.has(field)) {
        const value = cleanValue(line.slice(colonIdx + 1));
        if (value) out.set(field, { field, value, via: label });
        continue;
      }
    }

    // "Label value" with no colon — match the longest leading alias.
    const lower = line.toLowerCase();
    for (const [label, field] of LABEL_LOOKUP) {
      if (out.has(field)) continue;
      // Require the alias to be a whole leading token (followed by space).
      if (lower.startsWith(label + " ")) {
        const value = cleanValue(line.slice(label.length));
        if (value) out.set(field, { field, value, via: label });
        break;
      }
    }
  }

  return [...out.values()];
}
