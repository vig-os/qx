// Assembly graph utilities (#168) — BFS traversal, cycle detection,
// and reverse parent lookup for component relationships.
//
// Operates on the flat registry rows. No external dependencies.
// The graph is single-level today (an assembly's components cannot
// themselves be assemblies), but the traversal handles arbitrary
// depth for forward compatibility.

import type { RegistryRow } from "./schema";

/** Parse the semicolon-separated components string into an ID array. */
export function parseComponents(raw: string | undefined): string[] {
  if (!raw) return [];
  return raw
    .split(";")
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

/** Serialize component IDs to a sorted semicolon-separated string. */
export function serializeComponents(ids: string[]): string {
  return [...ids].sort().join(";");
}

/** True when the row has at least one component. */
export function isAssembly(row: RegistryRow): boolean {
  return parseComponents(row.components).length > 0;
}

/**
 * Build a reverse lookup: child ID → parent ID.
 * If a child appears in multiple assemblies, the last one wins
 * (shouldn't happen with CI validation, but degrade gracefully).
 */
export function buildParentMap(
  rows: ReadonlyArray<RegistryRow>,
): Map<string, string> {
  const map = new Map<string, string>();
  for (const row of rows) {
    const children = parseComponents(row.components);
    for (const childId of children) {
      map.set(childId, row.id);
    }
  }
  return map;
}

/**
 * BFS: collect all descendant IDs starting from a root.
 * Returns the ordered list of IDs visited (excluding the root).
 */
export function descendants(
  rootId: string,
  rows: ReadonlyArray<RegistryRow>,
): string[] {
  const byId = new Map(rows.map((r) => [r.id, r]));
  const visited = new Set<string>();
  const queue = [rootId];
  const result: string[] = [];

  while (queue.length > 0) {
    const current = queue.shift()!;
    if (visited.has(current)) continue;
    visited.add(current);

    const row = byId.get(current);
    if (!row) continue;

    for (const childId of parseComponents(row.components)) {
      if (!visited.has(childId)) {
        result.push(childId);
        queue.push(childId);
      }
    }
  }
  return result;
}

export interface CycleError {
  /** The ID that forms a cycle (appears as both parent and descendant). */
  id: string;
  /** The chain of IDs from the root to the cycle point. */
  path: string[];
}

/**
 * Detect cycles in the component graph. Returns all cycles found.
 * Uses DFS with a path stack for cycle detection.
 */
export function detectCycles(
  rows: ReadonlyArray<RegistryRow>,
): CycleError[] {
  const byId = new Map(rows.map((r) => [r.id, r]));
  const errors: CycleError[] = [];
  const globalVisited = new Set<string>();

  for (const row of rows) {
    if (globalVisited.has(row.id)) continue;
    if (!isAssembly(row)) continue;

    // DFS with path tracking
    const stack: Array<{ id: string; path: string[] }> = [
      { id: row.id, path: [row.id] },
    ];
    const localVisited = new Set<string>();

    while (stack.length > 0) {
      const { id, path } = stack.pop()!;
      if (localVisited.has(id)) {
        errors.push({ id, path });
        continue;
      }
      localVisited.add(id);
      globalVisited.add(id);

      const r = byId.get(id);
      if (!r) continue;
      for (const childId of parseComponents(r.components)) {
        stack.push({ id: childId, path: [...path, childId] });
      }
    }
  }
  return errors;
}

export interface ComponentValidation {
  valid: boolean;
  errors: string[];
}

/**
 * Validate a proposed components list against the registry.
 * Checks: existence, not void, not self-referencing, not already
 * a child of another assembly, no cycles.
 */
export function validateComponents(
  parentId: string,
  componentIds: string[],
  rows: ReadonlyArray<RegistryRow>,
): ComponentValidation {
  const byId = new Map(rows.map((r) => [r.id, r]));
  const parentMap = buildParentMap(rows);
  const errors: string[] = [];

  for (const childId of componentIds) {
    // Self-reference
    if (childId === parentId) {
      errors.push(`${childId}: a part cannot contain itself`);
      continue;
    }

    // Existence
    const child = byId.get(childId);
    if (!child) {
      errors.push(`${childId}: not found in registry`);
      continue;
    }

    // Void
    if (child.status === "void") {
      errors.push(`${childId}: voided — cannot be a component`);
      continue;
    }

    // Already parented (by a different assembly)
    const existingParent = parentMap.get(childId);
    if (existingParent && existingParent !== parentId) {
      errors.push(
        `${childId}: already a component of ${existingParent}`,
      );
    }
  }

  return { valid: errors.length === 0, errors };
}
