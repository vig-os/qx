// Session-aware registry view — merges committed CSV data with
// pending session items so the operator sees a unified picture.
//
// Minted IDs from the session appear as `status: "unbound"` rows.
// Bound IDs show the updated fields. Voided IDs show `status: "void"`.
// Edited IDs show the modified fields.
//
// Each pending row gets a `__pending: "true"` marker so the UI can
// render visual indicators (italic text, "pending" badge).

import type { Session, SessionItem } from "./session";
import type { RegistryRow } from "./schema";

/**
 * Merge committed registry rows with pending session items.
 *
 * Returns a new array with pending items applied on top of committed
 * data. Rows modified by the session get `__pending: "true"` set.
 */
export function mergedRegistryRows(
  committed: RegistryRow[],
  session: Session,
): RegistryRow[] {
  // Index committed rows by id for fast lookup + mutation
  const byId = new Map<string, RegistryRow>();
  for (const row of committed) {
    // Clone so we don't mutate the original
    byId.set(row.id, { ...row });
  }

  // Track IDs that have pending changes
  const pendingIds = new Set<string>();

  for (const item of session.items) {
    applyItem(byId, pendingIds, item);
  }

  // Mark pending rows
  const result: RegistryRow[] = [];
  for (const row of byId.values()) {
    if (pendingIds.has(row.id)) {
      result.push({ ...row, __pending: "true" });
    } else {
      result.push(row);
    }
  }

  return result;
}

function applyItem(
  byId: Map<string, RegistryRow>,
  pendingIds: Set<string>,
  item: SessionItem,
): void {
  switch (item.kind) {
    case "mint": {
      // Add a new unbound row if the ID doesn't already exist
      if (!byId.has(item.id)) {
        byId.set(item.id, {
          id: item.id,
          status: "unbound",
          minted_at: item.createdAt,
          batch: item.batch,
          notes: item.notes,
        });
      }
      pendingIds.add(item.id);
      break;
    }

    case "bind": {
      const existing = byId.get(item.id);
      if (existing) {
        // Apply bind fields on top of existing row
        for (const [key, value] of Object.entries(item.fields)) {
          if (value) existing[key] = value;
        }
        existing.status = "bound";
        if (!existing.bound_at) {
          existing.bound_at = item.createdAt;
        }
      }
      pendingIds.add(item.id);
      break;
    }

    case "edit": {
      const existing = byId.get(item.id);
      if (existing) {
        for (const [key, value] of Object.entries(item.changes)) {
          if (value !== undefined) {
            existing[key] = value;
          }
        }
      }
      pendingIds.add(item.id);
      break;
    }

    case "void": {
      const existing = byId.get(item.id);
      if (existing) {
        existing.status = "void";
        existing.notes = `[voided ${item.createdAt}] ${item.reason}`;
      }
      pendingIds.add(item.id);
      break;
    }
  }
}

/**
 * Check whether any IDs in a print plan reference session-only
 * (not yet committed) items.
 */
export function uncommittedPrintIds(
  planIds: string[],
  committed: RegistryRow[],
  session: Session,
): string[] {
  const committedIds = new Set(committed.map((r) => r.id));
  const sessionOnlyIds = new Set<string>();

  for (const item of session.items) {
    if (item.kind === "mint" && !committedIds.has(item.id)) {
      sessionOnlyIds.add(item.id);
    }
  }

  return planIds.filter((id) => sessionOnlyIds.has(id));
}
