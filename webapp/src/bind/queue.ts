// Bind queue: locally queued bind intents, persisted in localStorage.
// Pure list operations (enqueue/dequeue/settle) live here so the page
// component stays thin and the logic is unit-testable. Submission is
// the page's job — one Transition{to: "bound", fields} per item.

import type { ProtocolError, Response } from "../protocol";

export const BIND_QUEUE_KEY = "webapp.bind-queue";

export interface BindQueueItem {
  id: string;
  fields: Record<string, string>;
  /** ISO 8601 stamp of when the item entered the queue. */
  queued_at: string;
  /** Outcome of the last submit attempt; null until one fails. */
  error: ProtocolError | null;
}

function isItem(v: unknown): v is BindQueueItem {
  if (typeof v !== "object" || v === null) return false;
  const o = v as Record<string, unknown>;
  return (
    typeof o["id"] === "string" &&
    typeof o["queued_at"] === "string" &&
    typeof o["fields"] === "object" &&
    o["fields"] !== null
  );
}

/** Read the queue; malformed/absent storage yields an empty queue. */
export function loadQueue(storage: Pick<Storage, "getItem"> = localStorage): BindQueueItem[] {
  let parsed: unknown;
  try {
    const raw = storage.getItem(BIND_QUEUE_KEY);
    if (raw == null) return [];
    parsed = JSON.parse(raw);
  } catch {
    return [];
  }
  if (!Array.isArray(parsed)) return [];
  return parsed.filter(isItem);
}

export function saveQueue(
  items: BindQueueItem[],
  storage: Pick<Storage, "setItem"> = localStorage,
): void {
  storage.setItem(BIND_QUEUE_KEY, JSON.stringify(items));
}

/**
 * Add an item; re-queueing an id replaces the existing entry (one
 * pending bind per id) and clears any stale error.
 */
export function enqueue(
  items: BindQueueItem[],
  id: string,
  fields: Record<string, string>,
  queuedAt: string = new Date().toISOString(),
): BindQueueItem[] {
  const next: BindQueueItem = { id, fields, queued_at: queuedAt, error: null };
  const without = items.filter((i) => i.id !== id);
  return [...without, next];
}

export function dequeue(items: BindQueueItem[], id: string): BindQueueItem[] {
  return items.filter((i) => i.id !== id);
}

/**
 * Fold one submit outcome into the queue: a protocol Ok removes the
 * item; a protocol error keeps it queued and records the error
 * verbatim (kind + message) for honest display.
 */
export function settle(items: BindQueueItem[], id: string, res: Response): BindQueueItem[] {
  if (res.ok) return dequeue(items, id);
  return items.map((i) => (i.id === id ? { ...i, error: res.error } : i));
}
