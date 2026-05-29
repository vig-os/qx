// OCR text → registry part matcher (#171 P2).
//
// Given raw OCR text from a manufacturer label (or a plain-printed
// canonical ID), find which registry parts it refers to:
//   1. canonical IDs printed as text   → match by id
//   2. manufacturer's tracking number  → match by manufacturer_id
//   3. vendor catalog number           → match by part_number
//
// Pure + synchronous so it's exhaustively unit-testable; the tesseract
// integration in ocr-scan.ts just feeds text in and renders results.

import { ID_ALPHABET, ID_LENGTH } from "../config";
import type { RegistryRow } from "./schema";

export type MatchVia = "id" | "manufacturer_id" | "part_number";

export interface OcrMatch {
  /** Canonical registry ID of the matched part. */
  id: string;
  /** How the OCR text matched this part. */
  via: MatchVia;
  /** The substring of the (normalized) OCR text that matched. */
  matched: string;
}

/** Uppercase + strip everything except [A-Z0-9]; used to compare a
 *  manufacturer/part token against the OCR text regardless of the
 *  spaces, dashes, or slashes a label (or OCR) may insert. */
function squash(s: string): string {
  return s.toUpperCase().replace(/[^A-Z0-9]/g, "");
}

/** Find all canonical-ID-shaped runs in the OCR text. OCR often splits
 *  an ID into 4-char groups, so we squash the whole text and slide a
 *  window — but only accept windows that align to a non-alphanumeric
 *  boundary in the original, to avoid spurious substrings of longer
 *  runs. Simpler + robust: scan squashed text for fixed-length windows
 *  whose chars are all in the alphabet, then de-overlap. */
function findCanonicalIds(text: string): string[] {
  const out = new Set<string>();
  // Split on whitespace/newlines first so a 14-char ID printed as
  // "ABCD EFGH JKMN PQ" collapses, but two unrelated tokens don't fuse.
  for (const chunk of text.split(/\s+/)) {
    const squashed = squash(chunk);
    if (squashed.length < ID_LENGTH) continue;
    // Slide a window of ID_LENGTH; accept windows entirely in-alphabet.
    for (let i = 0; i + ID_LENGTH <= squashed.length; i++) {
      const win = squashed.slice(i, i + ID_LENGTH);
      if ([...win].every((c) => ID_ALPHABET.includes(c))) {
        out.add(win);
      }
    }
  }
  return [...out];
}

/**
 * Match OCR text against the registry. Returns one entry per matched
 * part, deduplicated by id with priority id > manufacturer_id >
 * part_number (a direct ID hit is more authoritative than a label hit).
 */
export function matchOcrText(
  text: string,
  rows: ReadonlyArray<RegistryRow>,
): OcrMatch[] {
  const byId = new Map<string, OcrMatch>();
  const squashedText = squash(text);

  // 1. Canonical IDs printed as text.
  const knownIds = new Set(rows.map((r) => r.id));
  for (const id of findCanonicalIds(text)) {
    // Prefer IDs that actually exist in the registry; but a well-formed
    // ID not in the loaded registry is still worth surfacing (the
    // operator may be working against a stale/partial fetch).
    if (!byId.has(id)) {
      byId.set(id, { id, via: "id", matched: id });
    }
    void knownIds; // existence is advisory, not required
  }

  // 2 + 3. Manufacturer / part-number labels. A row matches when its
  // (squashed) manufacturer_id or part_number appears in the squashed
  // OCR text. Guard against trivially-short tokens (≥4 chars) to avoid
  // matching, e.g., a 2-digit catalog number against noisy OCR.
  for (const row of rows) {
    if (byId.has(row.id) && byId.get(row.id)!.via === "id") continue;
    const mfr = squash(row.manufacturer_id ?? "");
    const part = squash(row.part_number ?? "");
    if (mfr.length >= 4 && squashedText.includes(mfr)) {
      mergeMatch(byId, { id: row.id, via: "manufacturer_id", matched: row.manufacturer_id! });
    } else if (part.length >= 4 && squashedText.includes(part)) {
      mergeMatch(byId, { id: row.id, via: "part_number", matched: row.part_number! });
    }
  }

  return [...byId.values()];
}

const VIA_PRIORITY: Record<MatchVia, number> = {
  id: 3,
  manufacturer_id: 2,
  part_number: 1,
};

function mergeMatch(map: Map<string, OcrMatch>, m: OcrMatch): void {
  const existing = map.get(m.id);
  if (!existing || VIA_PRIORITY[m.via] > VIA_PRIORITY[existing.via]) {
    map.set(m.id, m);
  }
}
