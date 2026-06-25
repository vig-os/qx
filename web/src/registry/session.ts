// Unified session store — IndexedDB-backed, with localStorage fallback.
//
// Replaces the separate bind queue (localStorage) and ephemeral mint
// results with a single persistent store that survives tab crashes
// (#115, #117).
//
// On first load, any existing localStorage bind queue items are
// migrated into the session and the old key is cleared.

import { events, EVENT_QUEUE_CHANGED } from "../core/events";

// ---- Types ----

export interface Session {
  id: string;
  createdAt: string;
  items: SessionItem[];
}

export type SessionItem =
  | SessionMint
  | SessionBind
  | SessionEdit
  | SessionVoid;

export interface SessionMint {
  kind: "mint";
  id: string;
  batch: string;
  notes: string;
  createdAt: string;
}

export interface SessionBind {
  kind: "bind";
  id: string;
  fields: Record<string, string>;
  createdAt: string;
}

export interface SessionEdit {
  kind: "edit";
  id: string;
  changes: Record<string, string>;
  before: Record<string, string>;
  createdAt: string;
}

export interface SessionVoid {
  kind: "void";
  id: string;
  reason: string;
  createdAt: string;
}

// ---- IndexedDB wrapper ----

const DB_NAME = "qx";
const STORE_NAME = "session";
const SESSION_KEY = "current";

// localStorage fallback key
const LS_SESSION_KEY = "qx.session";

// Old bind queue key (for migration)
const OLD_QUEUE_KEY = "qx.bind-queue";

let dbPromise: Promise<IDBDatabase> | null = null;

function openDb(): Promise<IDBDatabase> {
  if (dbPromise) return dbPromise;
  dbPromise = new Promise<IDBDatabase>((resolve, reject) => {
    try {
      const request = indexedDB.open(DB_NAME, 1);
      request.onupgradeneeded = () => {
        const db = request.result;
        if (!db.objectStoreNames.contains(STORE_NAME)) {
          db.createObjectStore(STORE_NAME);
        }
      };
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => {
        dbPromise = null;
        reject(request.error);
      };
    } catch (e) {
      dbPromise = null;
      reject(e);
    }
  });
  return dbPromise;
}

function freshSession(): Session {
  return {
    id: crypto.randomUUID(),
    createdAt: new Date().toISOString(),
    items: [],
  };
}

// ---- localStorage fallback ----

function loadSessionFromLocalStorage(): Session {
  try {
    const raw = localStorage.getItem(LS_SESSION_KEY);
    if (!raw) return freshSession();
    return JSON.parse(raw) as Session;
  } catch {
    return freshSession();
  }
}

function saveSessionToLocalStorage(session: Session): void {
  try {
    localStorage.setItem(LS_SESSION_KEY, JSON.stringify(session));
  } catch {
    // localStorage full or unavailable — silent degrade
  }
}

// ---- IndexedDB operations ----

async function loadSessionFromIDB(): Promise<Session | null> {
  const db = await openDb();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, "readonly");
    const store = tx.objectStore(STORE_NAME);
    const req = store.get(SESSION_KEY);
    req.onsuccess = () => resolve(req.result as Session | null);
    req.onerror = () => reject(req.error);
  });
}

async function saveSessionToIDB(session: Session): Promise<void> {
  const db = await openDb();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, "readwrite");
    const store = tx.objectStore(STORE_NAME);
    const req = store.put(session, SESSION_KEY);
    req.onsuccess = () => resolve();
    req.onerror = () => reject(req.error);
  });
}

async function clearSessionFromIDB(): Promise<void> {
  const db = await openDb();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, "readwrite");
    const store = tx.objectStore(STORE_NAME);
    const req = store.delete(SESSION_KEY);
    req.onsuccess = () => resolve();
    req.onerror = () => reject(req.error);
  });
}

// ---- In-memory cache ----
//
// The session is loaded once and cached. All mutations go through
// the exported helpers which update both the cache and storage.

let cached: Session | null = null;

// ---- Public API ----

/**
 * Load the current session. On first call, tries IndexedDB first,
 * then localStorage fallback. Subsequent calls return the cached
 * in-memory copy.
 */
export async function loadSession(): Promise<Session> {
  if (cached) return cached;

  try {
    const fromIDB = await loadSessionFromIDB();
    if (fromIDB) {
      cached = fromIDB;
      return cached;
    }
  } catch {
    // IndexedDB unavailable — fall through to localStorage
  }

  cached = loadSessionFromLocalStorage();
  return cached;
}

/**
 * Return the cached session synchronously. Returns null if
 * `loadSession()` hasn't been called yet. Useful for synchronous
 * code paths that know the session has been loaded at boot.
 */
export function getSessionSync(): Session | null {
  return cached;
}

/**
 * Persist the session to both IndexedDB and localStorage.
 */
export async function saveSession(session: Session): Promise<void> {
  cached = session;

  // Always write to localStorage as a fallback
  saveSessionToLocalStorage(session);

  try {
    await saveSessionToIDB(session);
  } catch {
    // IndexedDB write failed — localStorage fallback is already saved
  }

  // Emit event so badges / UI update
  events.emit(EVENT_QUEUE_CHANGED, { count: session.items.length });
}

/**
 * Clear the session entirely (e.g. after successful submit or
 * operator discard).
 */
export async function clearSession(): Promise<void> {
  cached = freshSession();

  saveSessionToLocalStorage(cached);

  try {
    await clearSessionFromIDB();
  } catch {
    // silent
  }

  events.emit(EVENT_QUEUE_CHANGED, { count: 0 });
}

// ---- Mutation helpers ----

export async function addMint(
  id: string,
  batch: string,
  notes: string,
): Promise<void> {
  const session = await loadSession();
  session.items.push({
    kind: "mint",
    id,
    batch,
    notes,
    createdAt: new Date().toISOString(),
  });
  await saveSession(session);
}

export async function addBind(
  id: string,
  fields: Record<string, string>,
): Promise<void> {
  const session = await loadSession();
  session.items.push({
    kind: "bind",
    id,
    fields,
    createdAt: new Date().toISOString(),
  });
  await saveSession(session);
}

/**
 * Append many items in a single read-modify-write (#176). Bulk import
 * builds N mint+bind pairs; calling addMint/addBind per row would
 * re-serialize the whole session ~2N times (O(n²)). This does one
 * load → push-all → save. Items must already carry their `createdAt`.
 */
export async function addItems(items: SessionItem[]): Promise<void> {
  if (items.length === 0) return;
  const session = await loadSession();
  session.items.push(...items);
  await saveSession(session);
}

export async function addEdit(
  id: string,
  before: Record<string, string>,
  changes: Record<string, string>,
): Promise<void> {
  const session = await loadSession();
  session.items.push({
    kind: "edit",
    id,
    before,
    changes,
    createdAt: new Date().toISOString(),
  });
  await saveSession(session);
}

export async function addVoid(id: string, reason: string): Promise<void> {
  const session = await loadSession();
  session.items.push({
    kind: "void",
    id,
    reason,
    createdAt: new Date().toISOString(),
  });
  await saveSession(session);
}

export async function removeItemAt(index: number): Promise<void> {
  const session = await loadSession();
  if (index < 0 || index >= session.items.length) return;
  session.items.splice(index, 1);
  await saveSession(session);
}

/**
 * Summarize session items for display.
 */
export function summarizeSession(session: Session): {
  mints: number;
  binds: number;
  edits: number;
  voids: number;
  total: number;
  label: string;
} {
  const mints = session.items.filter((i) => i.kind === "mint").length;
  const binds = session.items.filter((i) => i.kind === "bind").length;
  const edits = session.items.filter((i) => i.kind === "edit").length;
  const voids = session.items.filter((i) => i.kind === "void").length;
  const total = session.items.length;

  const parts: string[] = [];
  if (mints > 0) parts.push(`${mints} mint${mints > 1 ? "s" : ""}`);
  if (binds > 0) parts.push(`${binds} bind${binds > 1 ? "s" : ""}`);
  if (edits > 0) parts.push(`${edits} edit${edits > 1 ? "s" : ""}`);
  if (voids > 0) parts.push(`${voids} void${voids > 1 ? "s" : ""}`);
  const label = parts.join(", ") || "empty";

  return { mints, binds, edits, voids, total, label };
}

// ---- Migration ----

/**
 * Migrate old localStorage bind queue items into the session.
 * Called once on app boot. Clears the old key after migration.
 */
export async function migrateOldQueue(): Promise<number> {
  let raw: string | null = null;
  try {
    raw = localStorage.getItem(OLD_QUEUE_KEY);
  } catch {
    return 0;
  }
  if (!raw) return 0;

  let items: unknown[];
  try {
    items = JSON.parse(raw);
  } catch {
    return 0;
  }
  if (!Array.isArray(items) || items.length === 0) return 0;

  const session = await loadSession();
  let migrated = 0;

  for (const entry of items) {
    if (!entry || typeof entry !== "object") continue;
    const e = entry as Record<string, unknown>;
    const kind = typeof e.kind === "string" ? e.kind : "bind";
    const ts = typeof e.queued_at === "string" ? e.queued_at : new Date().toISOString();

    if (kind === "edit") {
      session.items.push({
        kind: "edit",
        id: String(e.id ?? ""),
        before: (e.before as Record<string, string>) ?? {},
        changes: (e.changes as Record<string, string>) ?? {},
        createdAt: ts,
      });
      migrated++;
    } else {
      // Legacy bind item
      const fields: Record<string, string> = {};
      for (const key of ["type", "description", "vendor", "part_number", "location", "notes"]) {
        if (typeof e[key] === "string" && e[key]) {
          fields[key] = e[key] as string;
        }
      }
      session.items.push({
        kind: "bind",
        id: String(e.id ?? ""),
        fields,
        createdAt: ts,
      });
      migrated++;
    }
  }

  if (migrated > 0) {
    await saveSession(session);
  }

  // Clear old key
  try {
    localStorage.removeItem(OLD_QUEUE_KEY);
  } catch {
    // silent
  }

  return migrated;
}
