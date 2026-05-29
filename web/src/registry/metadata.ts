// Metadata column helpers (#171) — the registry CSV `metadata` column
// holds a JSON object of type-specific key-value pairs.

export type MetadataValue = string | number | boolean | null | MetadataValue[] | { [k: string]: MetadataValue };

/**
 * Parse the metadata JSON string into a flat key-value object.
 * Returns an empty object for empty/invalid input — the registry must
 * stay viewable even if one row has malformed metadata.
 */
export function parseMetadata(raw: string | undefined): Record<string, MetadataValue> {
  if (!raw || !raw.trim()) return {};
  try {
    const parsed = JSON.parse(raw);
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      return parsed as Record<string, MetadataValue>;
    }
    return {};
  } catch {
    return {};
  }
}

/**
 * Serialize a metadata object to a single-line JSON string for CSV
 * storage. Returns "" for an empty object so the column stays blank.
 * Keys are sorted for deterministic diffs (matches the Rust BTreeMap).
 */
export function serializeMetadata(meta: Record<string, MetadataValue>): string {
  const keys = Object.keys(meta);
  if (keys.length === 0) return "";
  const sorted: Record<string, MetadataValue> = {};
  for (const k of keys.sort()) sorted[k] = meta[k];
  return JSON.stringify(sorted);
}
