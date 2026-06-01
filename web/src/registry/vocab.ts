// Controlled-vocabulary store (PR3). Backs the vendor/location comboboxes.
//
// Suggestions for a vocab field are the union of, in priority order:
//   1. the data-repo vocabulary file (vocabularies/<field>.json) — the
//      canonical, reviewable list (loaded via setFetchedVocab),
//   2. the contract's seed `options` (initial defaults),
//   3. values already present in the loaded registry (resilience), and
//   4. values created this session (staged), so a new vendor typed in one
//      row is suggested in the next.
//
// Values created via the combobox's "create" path are staged here; on submit
// the new entries are written back to vocabularies/<field>.json so the
// addition shows up in the review PR (see registry/vocab-files.ts).

import { REGISTRY_CONTRACT } from "./contract";
import { getSessionSync } from "./session";
import type { AppContext } from "../core/types";

/** Fields that use a controlled vocabulary with a create-new affordance. */
export const VOCAB_FIELDS = ["vendor", "location"] as const;
export type VocabField = (typeof VOCAB_FIELDS)[number];

// Values created this session, awaiting write-back, keyed by field.
const staged: Record<string, Set<string>> = {};
// Vocab fetched from the data repo, keyed by field (empty until loaded).
const fetched: Record<string, string[]> = {};

export function setFetchedVocab(field: string, values: string[]): void {
  fetched[field] = values;
}

export function stageVocabValue(field: string, value: string): void {
  const v = value.trim();
  if (!v) return;
  (staged[field] ??= new Set<string>()).add(v);
}

export function getStagedVocab(field: string): string[] {
  return [...(staged[field] ?? [])];
}

export function clearStagedVocab(): void {
  for (const k of Object.keys(staged)) delete staged[k];
}

function contractSeed(field: string): string[] {
  const f = REGISTRY_CONTRACT.fields.find((x) => x.key === field);
  return ((f as { options?: string[] } | undefined)?.options) ?? [];
}

/** Sorted, de-duplicated suggestions for a vocab field. */
export function fieldVocabOptions(ctx: AppContext, field: string): string[] {
  const set = new Set<string>();
  for (const v of fetched[field] ?? []) set.add(v);
  for (const v of contractSeed(field)) set.add(v);
  for (const row of ctx.registry.all()) {
    const v = (row as unknown as Record<string, string>)[field];
    if (v) set.add(v);
  }
  for (const v of staged[field] ?? []) set.add(v);
  return [...set].sort((a, b) => a.localeCompare(b));
}

/** Candidate IDs for the components multiselect: every known part plus
 *  session-pending mints (so a same-session mint can be a BOM component). */
export function componentCandidates(ctx: AppContext): string[] {
  const set = new Set<string>();
  for (const row of ctx.registry.all()) set.add(row.id);
  const sess = getSessionSync();
  if (sess) {
    for (const item of sess.items) {
      if (item.kind === "mint") set.add(item.id);
    }
  }
  return [...set];
}
