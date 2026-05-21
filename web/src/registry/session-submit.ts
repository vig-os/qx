// Session-aware submit — creates a single PR with all session
// operations (mints, binds, edits, voids).
//
// Extends the existing submit.ts pipeline rather than replacing it.
// The core GitHub API helpers (ghFetch, parseCsv, etc.) stay in
// submit.ts; this module adds session-to-CSV application logic.

import type { Session } from "./session";
import type { QueuedBind, QueuedEdit, QueueItem } from "./queue";
import { parseCsv, serialiseCsv } from "./submit";
import { summarizeSession } from "./session";

/**
 * Convert session items into the QueueItem format that submitBatch
 * understands, plus produce mint rows to prepend to the CSV.
 */
export function sessionToSubmitPayload(session: Session): {
  /** Queue items for binds/edits/voids (applied to existing rows). */
  queueItems: QueueItem[];
  /** Mint items that need new rows added to the CSV. */
  mintRows: Array<{ id: string; batch: string; notes: string; mintedAt: string }>;
  /** Human-readable summary for PR body. */
  summary: string;
} {
  const queueItems: QueueItem[] = [];
  const mintRows: Array<{ id: string; batch: string; notes: string; mintedAt: string }> = [];

  for (const item of session.items) {
    switch (item.kind) {
      case "mint":
        mintRows.push({
          id: item.id,
          batch: item.batch,
          notes: item.notes,
          mintedAt: item.createdAt,
        });
        break;

      case "bind":
        queueItems.push({
          kind: "bind",
          id: item.id,
          queued_at: item.createdAt,
          type: item.fields.type ?? "",
          description: item.fields.description ?? "",
          vendor: item.fields.vendor ?? "",
          part_number: item.fields.part_number ?? "",
          location: item.fields.location ?? "",
          notes: item.fields.notes ?? "",
        } as QueuedBind);
        break;

      case "edit":
        queueItems.push({
          kind: "edit",
          id: item.id,
          queued_at: item.createdAt,
          before: { ...item.before },
          changes: { ...item.changes },
        } as QueuedEdit);
        break;

      case "void":
        queueItems.push({
          kind: "edit",
          id: item.id,
          queued_at: item.createdAt,
          before: { status: "", notes: "" },
          changes: {
            status: "void",
            notes: `[voided ${item.createdAt}] ${item.reason}`,
          },
        } as QueuedEdit);
        break;
    }
  }

  const stats = summarizeSession(session);
  const summary = stats.label;

  return { queueItems, mintRows, summary };
}

/**
 * Build the commit message and PR body for a session submit.
 */
export function buildSessionCommitMessage(session: Session): {
  commitMessage: string;
  prBody: string;
} {
  const stats = summarizeSession(session);
  const parts: string[] = [];
  if (stats.mints > 0) parts.push(`${stats.mints} mint${stats.mints > 1 ? "s" : ""}`);
  if (stats.binds > 0) parts.push(`${stats.binds} bind${stats.binds > 1 ? "s" : ""}`);
  if (stats.edits > 0) parts.push(`${stats.edits} edit${stats.edits > 1 ? "s" : ""}`);
  if (stats.voids > 0) parts.push(`${stats.voids} void${stats.voids > 1 ? "s" : ""}`);

  const commitMessage = `registry: ${parts.join(" + ")} via web UI`;

  const ids = session.items
    .slice(0, 10)
    .map((i) => i.id)
    .join(", ");

  const prBody =
    `Proposed by the part-registry web UI.\n\n` +
    `**Changes:** ${parts.join(", ")}\n` +
    `**IDs:** ${ids}${session.items.length > 10 ? ` (+${session.items.length - 10} more)` : ""}\n\n` +
    `_Automated PR — CI will validate._`;

  return { commitMessage, prBody };
}

/**
 * Apply mint rows to a CSV text, inserting new unbound rows.
 * Returns the modified CSV text.
 */
export function applyMints(
  csvText: string,
  mints: Array<{ id: string; batch: string; notes: string; mintedAt: string }>,
  operator?: string,
): string {
  if (mints.length === 0) return csvText;

  const { header, rows } = parseCsv(csvText);
  const headerCols = header.split(",").map((c) => c.trim());

  for (const mint of mints) {
    if (rows.has(mint.id)) continue; // already exists — skip

    const obj: Record<string, string> = {};
    for (const col of headerCols) {
      obj[col] = "";
    }
    obj.id = mint.id;
    obj.status = "unbound";
    obj.minted_at = mint.mintedAt;
    if (operator) obj.minted_by = operator;
    obj.batch = mint.batch;
    obj.notes = mint.notes;

    const line = headerCols
      .map((col) => {
        const val = obj[col] ?? "";
        if (val.includes(",") || val.includes('"') || val.includes("\n")) {
          return `"${val.replace(/"/g, '""')}"`;
        }
        return val;
      })
      .join(",");

    rows.set(mint.id, line);
  }

  return serialiseCsv(header, rows);
}
