// Data-repo vocabulary files (PR3). The controlled vocabularies for vendor /
// location live as structured JSON next to registry.csv in the data repo:
//
//   vocabularies/vendors.json    [{ "name": "Digi-Key" }, ...]
//   vocabularies/locations.json  [{ "name": "Lab A / Shelf 3" }, ...]
//
// JSON (not flat text) so an entry is expandable later — a vendor could carry
// { name, url, aliases }, a location a hierarchy — without a format change.
// On load they enrich the combobox suggestions; on submit, values created
// this session are merged in and committed alongside registry.csv, so the
// addition shows up in the review PR.

import { DATA_REPO_SLUG, DEFAULT_BRANCH } from "../config";
import { VOCAB_FIELDS, setFetchedVocab, type VocabField } from "./vocab";

export interface VocabEntry {
  name: string;
  [extra: string]: unknown;
}

export const VOCAB_PATH: Record<VocabField, string> = {
  vendor: "vocabularies/vendors.json",
  location: "vocabularies/locations.json",
};

function rawUrl(path: string): string {
  return `https://raw.githubusercontent.com/${DATA_REPO_SLUG}/${DEFAULT_BRANCH}/${path}`;
}

/** Parse a vocab file's text into entries; tolerant of malformed content. */
export function parseVocab(text: string): VocabEntry[] {
  try {
    const data = JSON.parse(text);
    if (!Array.isArray(data)) return [];
    return data.filter(
      (e): e is VocabEntry => e != null && typeof (e as { name?: unknown }).name === "string",
    );
  } catch {
    return [];
  }
}

/** Merge new names into existing entries. Returns the merged, name-sorted
 *  list, or null when nothing is new (so callers can skip a no-op commit). */
export function mergeVocab(existing: VocabEntry[], newNames: string[]): VocabEntry[] | null {
  const have = new Set(existing.map((e) => e.name));
  const additions = newNames.map((n) => n.trim()).filter((n) => n && !have.has(n));
  if (additions.length === 0) return null;
  const merged = [...existing, ...additions.map((name) => ({ name }))];
  merged.sort((a, b) => a.name.localeCompare(b.name));
  return merged;
}

/** Stable, pretty serialization (trailing newline) for clean PR diffs. */
export function serializeVocab(entries: VocabEntry[]): string {
  return JSON.stringify(entries, null, 2) + "\n";
}

/** Fetch the data-repo vocabularies and enrich the combobox suggestions.
 *  Best-effort: a missing file (404) or network error degrades to the
 *  contract seeds + registry-derived values. */
export async function loadVocabularies(): Promise<void> {
  await Promise.all(
    VOCAB_FIELDS.map(async (field) => {
      try {
        const res = await fetch(rawUrl(VOCAB_PATH[field]), { cache: "no-store" });
        if (!res.ok) return;
        setFetchedVocab(field, parseVocab(await res.text()).map((e) => e.name));
      } catch {
        /* network/parse — rely on seeds */
      }
    }),
  );
}
