// Shared queue infrastructure for Bind + Edit (#6).
//
// Per ADR-014 §Consequences: lookup edit reuses the bind queue rather
// than building a separate submit pipeline. Both produce CSV-row
// changes; the queue + submit doesn't care about origin.

import type { RegistryRow } from "./schema";
import { events, EVENT_QUEUE_CHANGED } from "../core/events";

const QUEUE_KEY = "part-registry.bind-queue";

export type EditableKey =
  | "type"
  | "description"
  | "vendor"
  | "part_number"
  | "location"
  | "notes"
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

function migrate(raw: unknown): QueueItem[] {
  if (!Array.isArray(raw)) return [];
  return raw
    .map((entry: unknown): QueueItem | null => {
      if (!entry || typeof entry !== "object") return null;
      const e = entry as Record<string, unknown>;
      // Legacy items (pre-#6) have no `kind` — they were all binds.
      if (typeof e.kind !== "string") {
        if (typeof e.id !== "string") return null;
        return {
          kind: "bind",
          id: e.id,
          queued_at: typeof e.queued_at === "string" ? e.queued_at : "",
          type: String(e.type ?? ""),
          description: String(e.description ?? ""),
          vendor: String(e.vendor ?? ""),
          part_number: String(e.part_number ?? ""),
          location: String(e.location ?? ""),
          notes: String(e.notes ?? ""),
        };
      }
      return entry as QueueItem;
    })
    .filter((x): x is QueueItem => x !== null);
}

export function loadQueue(): QueueItem[] {
  try {
    const raw = localStorage.getItem(QUEUE_KEY);
    if (!raw) return [];
    return migrate(JSON.parse(raw));
  } catch {
    return [];
  }
}

export function saveQueue(q: QueueItem[]): void {
  localStorage.setItem(QUEUE_KEY, JSON.stringify(q));
  events.emit(EVENT_QUEUE_CHANGED, { count: q.length });
}

export function appendBind(entry: Omit<QueuedBind, "kind" | "queued_at">): void {
  const q = loadQueue();
  q.push({
    kind: "bind",
    queued_at: new Date().toISOString(),
    ...entry,
  });
  saveQueue(q);
}

export function appendEdit(
  id: string,
  before: Partial<RegistryRow>,
  changes: Partial<RegistryRow>,
): void {
  const q = loadQueue();
  q.push({
    kind: "edit",
    id,
    queued_at: new Date().toISOString(),
    before,
    changes,
  });
  saveQueue(q);
}

export function removeAt(index: number): void {
  const q = loadQueue();
  if (index < 0 || index >= q.length) return;
  q.splice(index, 1);
  saveQueue(q);
}

export function clearQueue(): void {
  saveQueue([]);
}

export function appendVoid(
  id: string,
  before: Partial<RegistryRow>,
  reason: string,
): void {
  const ts = new Date().toISOString();
  const notesValue = `[voided ${ts}] ${reason}`;
  appendEdit(id, before, { status: "void" as RegistryRow["status"], notes: notesValue });
}

export function summarizeQueue(q: QueueItem[]): {
  binds: number;
  edits: number;
  voids: number;
  total: number;
  label: string;
} {
  const binds = q.filter((x) => x.kind === "bind").length;
  const voids = q.filter(
    (x) => x.kind === "edit" && (x as QueuedEdit).changes.status === "void",
  ).length;
  const edits = q.filter((x) => x.kind === "edit").length - voids;
  const parts: string[] = [];
  if (binds > 0) parts.push("bind");
  if (edits > 0) parts.push("edit");
  if (voids > 0) parts.push("void");
  const label = parts.length > 0 ? parts.join("+") : "bind";
  return { binds, edits, voids, total: q.length, label };
}
