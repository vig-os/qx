// Shared queue infrastructure for Bind + Edit (#6).
//
// Per ADR-014 §Consequences: lookup edit reuses the bind queue rather
// than building a separate submit pipeline. Both produce CSV-row
// changes; the queue + submit doesn't care about origin.
//
// As of #115/#117 this module is a thin facade over the session store.
// The synchronous API is preserved for callers that haven't migrated
// to async yet — it reads from the in-memory session cache.

import type { RegistryRow, Status } from "./schema";
import {
  getSessionSync,
  saveSession,
  addBind as sessionAddBind,
  addEdit as sessionAddEdit,
  addVoid as sessionAddVoid,
  removeItemAt as sessionRemoveAt,
  type SessionItem,
  type SessionBind,
  type SessionEdit,
  type SessionVoid,
} from "./session";

export type EditableKey =
  | "type"
  | "description"
  | "vendor"
  | "part_number"
  | "location"
  | "notes"
  | "components"
  | "manufacturer_id"
  | "metadata"
  | "status";

export interface QueuedBind {
  kind: "bind";
  id: string;
  queued_at: string;
  type: string;
  description: string;
  vendor: string;
  part_number: string;
  location: string;
  notes: string;
  components: string;
  manufacturer_id: string;
  /** Type-specific metadata as a JSON object string (#171). */
  metadata: string;
}

export interface QueuedEdit {
  kind: "edit";
  id: string;
  queued_at: string;
  // The values from the registry at the time the edit was queued —
  // kept so the table can show before/after without re-reading the
  // registry on every render.
  before: Partial<RegistryRow>;
  // Just the fields the operator changed.
  changes: Partial<RegistryRow>;
}

export type QueueItem = QueuedBind | QueuedEdit;

// ---- Session → QueueItem adapters ----

function sessionItemToQueueItem(item: SessionItem): QueueItem | null {
  switch (item.kind) {
    case "bind":
      return {
        kind: "bind",
        id: item.id,
        queued_at: item.createdAt,
        type: (item as SessionBind).fields.type ?? "",
        description: (item as SessionBind).fields.description ?? "",
        vendor: (item as SessionBind).fields.vendor ?? "",
        part_number: (item as SessionBind).fields.part_number ?? "",
        location: (item as SessionBind).fields.location ?? "",
        notes: (item as SessionBind).fields.notes ?? "",
        components: (item as SessionBind).fields.components ?? "",
        manufacturer_id: (item as SessionBind).fields.manufacturer_id ?? "",
        metadata: (item as SessionBind).fields.metadata ?? "",
      };
    case "edit":
      return {
        kind: "edit",
        id: item.id,
        queued_at: item.createdAt,
        before: { ...(item as SessionEdit).before } as Partial<RegistryRow>,
        changes: { ...(item as SessionEdit).changes } as Partial<RegistryRow>,
      };
    case "void":
      return {
        kind: "edit",
        id: item.id,
        queued_at: item.createdAt,
        before: { status: undefined as unknown as Status, notes: "" },
        changes: {
          status: "void" as Status,
          notes: `[voided ${item.createdAt}] ${(item as SessionVoid).reason}`,
        },
      };
    case "mint":
      // Mint items are not queue items in the old sense — they're
      // handled by the session submit flow. Return null to filter.
      return null;
  }
}

/**
 * Load queue items from the session (synchronous — reads cached session).
 * Excludes mint items since the old queue never had those.
 */
export function loadQueue(): QueueItem[] {
  const session = getSessionSync();
  if (!session) return [];
  return session.items
    .map(sessionItemToQueueItem)
    .filter((x): x is QueueItem => x !== null);
}

export function saveQueue(q: QueueItem[]): void {
  // This is called by bind.ts inline-edit persistence. We need to
  // rebuild the session items from the queue items, preserving any
  // mint items that aren't in the queue view.
  const session = getSessionSync();
  if (!session) return;

  // Keep mint items (they're not exposed via loadQueue)
  const mintItems = session.items.filter((i) => i.kind === "mint");

  // Convert queue items back to session items
  const newItems: SessionItem[] = [...mintItems];
  for (const item of q) {
    if (item.kind === "bind") {
      newItems.push({
        kind: "bind",
        id: item.id,
        fields: {
          type: item.type,
          description: item.description,
          vendor: item.vendor,
          part_number: item.part_number,
          location: item.location,
          notes: item.notes,
          components: item.components,
          manufacturer_id: item.manufacturer_id,
          metadata: item.metadata,
        },
        createdAt: item.queued_at,
      });
    } else {
      // Check if this was originally a void
      const isVoid = item.changes.status === "void";
      if (isVoid) {
        const notesVal = String(item.changes.notes ?? "");
        const reasonMatch = notesVal.match(/^\[voided [^\]]+\]\s*(.*)/);
        newItems.push({
          kind: "void",
          id: item.id,
          reason: reasonMatch ? reasonMatch[1] : notesVal,
          createdAt: item.queued_at,
        });
      } else {
        newItems.push({
          kind: "edit",
          id: item.id,
          before: { ...item.before } as Record<string, string>,
          changes: { ...item.changes } as Record<string, string>,
          createdAt: item.queued_at,
        });
      }
    }
  }

  session.items = newItems;
  // Fire-and-forget async save — the in-memory cache is already updated
  void saveSession(session);
}

export async function appendBind(entry: Omit<QueuedBind, "kind" | "queued_at">): Promise<void> {
  const fields: Record<string, string> = {};
  for (const key of ["type", "description", "vendor", "part_number", "location", "notes", "components", "manufacturer_id", "metadata"] as const) {
    if (entry[key]) fields[key] = entry[key];
  }
  await sessionAddBind(entry.id, fields);
}

export async function appendEdit(
  id: string,
  before: Partial<RegistryRow>,
  changes: Partial<RegistryRow>,
): Promise<void> {
  await sessionAddEdit(
    id,
    { ...before } as Record<string, string>,
    { ...changes } as Record<string, string>,
  );
}

export async function removeAt(index: number): Promise<void> {
  // The index in loadQueue() excludes mints, so we need to map back
  // to the session index.
  const session = getSessionSync();
  if (!session) return;

  let queueIdx = 0;
  for (let i = 0; i < session.items.length; i++) {
    if (session.items[i].kind === "mint") continue;
    if (queueIdx === index) {
      await sessionRemoveAt(i);
      return;
    }
    queueIdx++;
  }
}

export async function clearQueue(): Promise<void> {
  // Clear only bind/edit/void items, keep mints
  const session = getSessionSync();
  if (!session) return;
  session.items = session.items.filter((i) => i.kind === "mint");
  await saveSession(session);
}

export async function appendVoid(id: string, reason: string): Promise<void> {
  await sessionAddVoid(id, reason);
}

export function summarizeQueue(q: QueueItem[]): {
  binds: number;
  edits: number;
  total: number;
  label: string;
} {
  const binds = q.filter((x) => x.kind === "bind").length;
  const edits = q.filter((x) => x.kind === "edit").length;
  let label: string;
  if (binds > 0 && edits > 0) label = "bind+edit";
  else if (edits > 0) label = "edit";
  else label = "bind";
  return { binds, edits, total: q.length, label };
}
