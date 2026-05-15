// Proposal broker — browser-side GitHub PR creation (issue #5).
//
// Submits queued registry mutations as a single PR against the data
// repo's registry.csv. Uses the GitHub REST API directly via fetch()
// — no SDK, no server-side broker. The operator supplies a fine-
// grained PAT with `contents:write` + `pull_requests:write` on the
// data repo.
//
// Flow:
//   1. GET  /repos/{owner}/{repo}/contents/registry.csv  → base SHA + CSV text
//   2. Apply queue edits to produce a new CSV
//   3. POST /repos/{owner}/{repo}/git/refs                → create branch
//   4. PUT  /repos/{owner}/{repo}/contents/registry.csv   → commit modified CSV
//   5. POST /repos/{owner}/{repo}/pulls                   → open PR
//
// The result is returned to the caller (bind.ts) which shows
// success/failure feedback and clears the queue on success.

import type { QueuedBind, QueuedEdit, QueueItem } from "./queue";

export interface SubmitResult {
  prUrl: string;
  prNumber: number;
}

export class SubmitError extends Error {
  constructor(
    message: string,
    public readonly step: string,
    public readonly status?: number,
  ) {
    super(message);
    this.name = "SubmitError";
  }
}

// ---- localStorage PAT management ----

const PAT_KEY = "part-registry.github-pat";

export function getStoredToken(): string | null {
  try {
    return localStorage.getItem(PAT_KEY);
  } catch {
    return null;
  }
}

export function storeToken(token: string): void {
  localStorage.setItem(PAT_KEY, token);
}

export function clearToken(): void {
  localStorage.removeItem(PAT_KEY);
}

/** Prompt the operator for a GitHub PAT. Returns the token or null
 *  if cancelled. Uses a simple `prompt()` — good enough for a bench
 *  tool; a proper OAuth flow is the future (#5 GitHub App path). */
export function promptForToken(): string | null {
  const existing = getStoredToken();
  const token = prompt(
    "Enter a GitHub Personal Access Token (fine-grained) with contents:write + pull_requests:write on the data repo." +
      (existing ? "\n\nA token is already saved. Leave blank to keep it, or paste a new one." : ""),
    "",
  );
  if (token === null) return null; // cancelled
  if (token.trim() === "" && existing) return existing;
  if (token.trim() === "") return null;
  storeToken(token.trim());
  return token.trim();
}

// ---- GitHub REST API helpers ----

interface GitHubFileResponse {
  sha: string;
  content: string;
  encoding: string;
}

async function ghFetch(
  url: string,
  token: string,
  init?: RequestInit,
): Promise<Response> {
  const res = await fetch(url, {
    ...init,
    headers: {
      Accept: "application/vnd.github+json",
      Authorization: `Bearer ${token}`,
      "X-GitHub-Api-Version": "2022-11-28",
      ...(init?.headers ?? {}),
    },
  });
  return res;
}

// ---- CSV manipulation ----

/** Parse CSV text into header + rows. Lightweight — no need for
 *  papaparse here since we're working with a well-formed registry CSV
 *  that we control. We do need to preserve the exact header line. */
export function parseCsv(text: string): { header: string; rows: Map<string, string> } {
  const lines = text.replace(/\r\n/g, "\n").split("\n");
  const header = lines[0] ?? "";
  const rows = new Map<string, string>();
  for (let i = 1; i < lines.length; i++) {
    const line = lines[i];
    if (!line.trim()) continue;
    // The id is always the first column.
    const id = line.split(",")[0]?.trim() ?? "";
    if (id) rows.set(id, line);
  }
  return { header, rows };
}

/** Re-serialise the header + rows back to CSV text. */
export function serialiseCsv(header: string, rows: Map<string, string>): string {
  const lines = [header, ...rows.values()];
  // Ensure trailing newline.
  return lines.join("\n") + "\n";
}

/** Apply queued binds + edits to the CSV row map. Returns the
 *  modified CSV text. */
function applyQueue(
  csvText: string,
  queue: ReadonlyArray<QueueItem>,
): string {
  const { header, rows } = parseCsv(csvText);
  const headerCols = header.split(",").map((c) => c.trim());

  for (const item of queue) {
    if (item.kind === "bind") {
      applyBind(headerCols, rows, item);
    } else {
      applyEdit(headerCols, rows, item);
    }
  }
  return serialiseCsv(header, rows);
}

function parseRow(headerCols: string[], line: string): Record<string, string> {
  const vals = splitCsvLine(line);
  const obj: Record<string, string> = {};
  for (let i = 0; i < headerCols.length; i++) {
    obj[headerCols[i]] = vals[i] ?? "";
  }
  return obj;
}

function rowToLine(headerCols: string[], obj: Record<string, string>): string {
  return headerCols.map((col) => escapeCsvField(obj[col] ?? "")).join(",");
}

function escapeCsvField(val: string): string {
  if (val.includes(",") || val.includes('"') || val.includes("\n")) {
    return `"${val.replace(/"/g, '""')}"`;
  }
  return val;
}

/** Split a CSV line respecting quoted fields. */
export function splitCsvLine(line: string): string[] {
  const fields: string[] = [];
  let current = "";
  let inQuotes = false;
  for (let i = 0; i < line.length; i++) {
    const ch = line[i];
    if (inQuotes) {
      if (ch === '"') {
        if (line[i + 1] === '"') {
          current += '"';
          i++; // skip escaped quote
        } else {
          inQuotes = false;
        }
      } else {
        current += ch;
      }
    } else {
      if (ch === '"') {
        inQuotes = true;
      } else if (ch === ",") {
        fields.push(current);
        current = "";
      } else {
        current += ch;
      }
    }
  }
  fields.push(current);
  return fields;
}

export function applyBind(
  headerCols: string[],
  rows: Map<string, string>,
  bind: QueuedBind,
): void {
  const existing = rows.get(bind.id);
  if (!existing) return; // ID not in registry — skip (preflight would have warned)
  const obj = parseRow(headerCols, existing);
  obj.status = "bound";
  obj.bound_at = obj.bound_at || new Date().toISOString();
  if (bind.type) obj.type = bind.type;
  if (bind.description) obj.description = bind.description;
  if (bind.vendor) obj.vendor = bind.vendor;
  if (bind.part_number) obj.part_number = bind.part_number;
  if (bind.location) obj.location = bind.location;
  if (bind.notes) obj.notes = bind.notes;
  obj.last_edited_at = new Date().toISOString();
  rows.set(bind.id, rowToLine(headerCols, obj));
}

export function applyEdit(
  headerCols: string[],
  rows: Map<string, string>,
  edit: QueuedEdit,
): void {
  const existing = rows.get(edit.id);
  if (!existing) return;
  const obj = parseRow(headerCols, existing);
  for (const [key, value] of Object.entries(edit.changes)) {
    if (value !== undefined) {
      obj[key] = value;
    }
  }
  obj.last_edited_at = new Date().toISOString();
  rows.set(edit.id, rowToLine(headerCols, obj));
}

// ---- Main submit flow ----

export async function submitBatch(
  queue: ReadonlyArray<QueueItem>,
  token: string,
  dataRepoSlug: string,
): Promise<SubmitResult> {
  const apiBase = `https://api.github.com/repos/${dataRepoSlug}`;

  // 1. Fetch current registry.csv from main.
  const fileRes = await ghFetch(
    `${apiBase}/contents/registry.csv?ref=main`,
    token,
  );
  if (!fileRes.ok) {
    const body = await fileRes.text();
    throw new SubmitError(
      `Failed to read registry.csv: ${fileRes.status} ${body}`,
      "read-csv",
      fileRes.status,
    );
  }
  const fileData = (await fileRes.json()) as GitHubFileResponse;
  const csvBytes = Uint8Array.from(
    atob(fileData.content.replace(/\n/g, "")),
    (c) => c.charCodeAt(0),
  );
  const csvText = new TextDecoder().decode(csvBytes);

  // 2. Apply queue edits to produce new CSV.
  const newCsv = applyQueue(csvText, queue);

  // 3. Get the SHA of main's HEAD so we can create a branch from it.
  const mainRefRes = await ghFetch(`${apiBase}/git/ref/heads/main`, token);
  if (!mainRefRes.ok) {
    throw new SubmitError(
      `Failed to read main ref: ${mainRefRes.status}`,
      "read-main-ref",
      mainRefRes.status,
    );
  }
  const mainRef = (await mainRefRes.json()) as {
    object: { sha: string };
  };
  const baseSha = mainRef.object.sha;

  // 4. Create a new branch.
  const ts = Date.now();
  const branchName = `registry-proposal/${ts}`;
  const createRefRes = await ghFetch(`${apiBase}/git/refs`, token, {
    method: "POST",
    body: JSON.stringify({
      ref: `refs/heads/${branchName}`,
      sha: baseSha,
    }),
  });
  if (!createRefRes.ok) {
    const body = await createRefRes.text();
    throw new SubmitError(
      `Failed to create branch: ${createRefRes.status} ${body}`,
      "create-branch",
      createRefRes.status,
    );
  }

  // 5. Commit modified CSV to the branch.
  const bindCount = queue.filter((q) => q.kind === "bind").length;
  const editCount = queue.filter((q) => q.kind === "edit").length;
  const parts: string[] = [];
  if (bindCount > 0) parts.push(`${bindCount} bind${bindCount > 1 ? "s" : ""}`);
  if (editCount > 0) parts.push(`${editCount} edit${editCount > 1 ? "s" : ""}`);
  const commitMessage = `registry: ${parts.join(" + ")} via web UI`;

  const encoded = btoa(
    String.fromCharCode(...new TextEncoder().encode(newCsv)),
  );
  const putRes = await ghFetch(
    `${apiBase}/contents/registry.csv`,
    token,
    {
      method: "PUT",
      body: JSON.stringify({
        message: commitMessage,
        content: encoded,
        sha: fileData.sha,
        branch: branchName,
      }),
    },
  );
  if (!putRes.ok) {
    const body = await putRes.text();
    throw new SubmitError(
      `Failed to commit CSV: ${putRes.status} ${body}`,
      "commit-csv",
      putRes.status,
    );
  }

  // 6. Create PR.
  const ids = queue
    .slice(0, 10)
    .map((q) => q.id)
    .join(", ");
  const prBody =
    `Proposed by the part-registry web UI.\n\n` +
    `**Changes:** ${parts.join(", ")}\n` +
    `**IDs:** ${ids}${queue.length > 10 ? ` (+${queue.length - 10} more)` : ""}\n\n` +
    `_Automated PR — CI will validate._`;

  const prRes = await ghFetch(`${apiBase}/pulls`, token, {
    method: "POST",
    body: JSON.stringify({
      title: commitMessage,
      head: branchName,
      base: "main",
      body: prBody,
    }),
  });
  if (!prRes.ok) {
    const body = await prRes.text();
    throw new SubmitError(
      `Failed to create PR: ${prRes.status} ${body}`,
      "create-pr",
      prRes.status,
    );
  }
  const prData = (await prRes.json()) as {
    html_url: string;
    number: number;
  };
  return { prUrl: prData.html_url, prNumber: prData.number };
}
