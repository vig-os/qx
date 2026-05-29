// ID generation primitives — pure, DOM-free, reusable across tabs.
//
// Lives in registry/ (not tabs/) so non-UI code (e.g. assembly
// creation) can mint IDs without importing a tab module.

/** Generate a single random ID from the given alphabet. */
export function generateId(alphabet: string, length: number): string {
  const bytes = new Uint8Array(length);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (b) => alphabet[b % alphabet.length]).join("");
}

/** Generate `count` unique random IDs (unique within the batch only). */
export function generateIds(
  count: number,
  alphabet: string,
  length: number,
): string[] {
  const ids = new Set<string>();
  // Guard against infinite loops if the alphabet/length space is too
  // small (shouldn't happen with the real 14-char alphabet, but be
  // safe in tests).
  let attempts = 0;
  const maxAttempts = count * 10;
  while (ids.size < count && attempts < maxAttempts) {
    ids.add(generateId(alphabet, length));
    attempts++;
  }
  return [...ids];
}

/**
 * Generate a single ID that passes the `isFree` predicate — used to
 * mint an ID guaranteed not to collide with existing registry rows or
 * other pending session mints. Collisions are astronomically unlikely
 * with the real alphabet, but checking is cheap and correct.
 */
export function mintUniqueId(
  alphabet: string,
  length: number,
  isFree: (id: string) => boolean,
): string {
  for (let i = 0; i < 1000; i++) {
    const id = generateId(alphabet, length);
    if (isFree(id)) return id;
  }
  throw new Error("Unable to generate a collision-free ID");
}
