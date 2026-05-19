// Bind tab — queue binds + edits locally as a table; submit creates a
// PR against the data repo via the GitHub REST API (issue #5).
//
// UX shape: a table where each row is either
//   - a queued bind: fully inline-editable (type / description /
//     vendor / part_number / location / notes), or
//   - a queued edit (from the Lookup detail card, #6): read-only
//     before/after diff of the changed fields.
// Delete via trash icon. The bottom row is an "empty entry" row whose
// ID field has a camera-icon button to scan a QR — that's the primary
// entry point for new binds. Manual paste is also supported. Edits
// arrive from the Lookup tab; they're not added from here.

import { FIELDS, type RegistryRow } from "../registry/schema";
import { runPreflight, type QueueItem } from "../registry/preflight";
import {
  appendBind,
  clearQueue,
  loadQueue,
  removeAt,
  saveQueue,
  summarizeQueue,
  type EditableKey,
  type QueuedBind,
  type QueuedEdit,
} from "../registry/queue";
import type { AppContext, Tab } from "../core/types";
import { el, button, input, formRow } from "../ui/dom";
import { icon } from "../ui/icons";
import {
  openScannerMulti,
  openImageScan,
  openScannerRolling,
  type ScanStatus,
} from "../ui/scanner";
import type { Action, AuthDecision } from "../wasm/loader";
import {
  submitBatch,
  promptForToken,
  getStoredToken,
  clearToken,
  SubmitError,
} from "../registry/submit";
import { DATA_REPO_SLUG } from "../config";

function emptyBindFields(): Pick<
  QueuedBind,
  "type" | "description" | "vendor" | "part_number" | "location" | "notes"
> {
  return {
    type: "",
    description: "",
    vendor: "",
    part_number: "",
    location: "",
    notes: "",
  };
}

function fmtId(id: string): string {
  // 4-4-4 grouping for display; underlying value stays canonical.
  if (id.length < 12) return id;
  return `${id.slice(0, 4)}-${id.slice(4, 8)}-${id.slice(8, 12)}${
    id.length > 12 ? "-" + id.slice(12) : ""
  }`;
}

export const bindTab: Tab = {
  id: "bind",
  label: "Bind",
  mount(container, ctx) {
    container.innerHTML = "";
    container.append(buildUI(ctx));
  },
};

function buildUI(ctx: AppContext): HTMLElement {
  const root = el("div", { class: "tab tab--bind" });
  root.append(el("h2", {}, "Bind / edit queue"));
  root.append(
    el(
      "p",
      { class: "muted small" },
      "Click \"+\u00a0Add row\" to queue a new bind (or scan QR codes). " +
        "Edits arrive from the Lookup tab. " +
        "Submit creates a PR against the data repo.",
    ),
  );

  const submitBtn = button({ class: "primary" }, icon("plus"), " Submit batch");
  const clearBtn = button({ class: "destructive" }, icon("trash"), " Clear queue");
  const summaryEl = el("span", { class: "queue-summary muted small" });

  // ADR-016 §"FE preflight" + issue #23: every queue mutation re-runs
  // the same classify + policy engine the CI gate runs, advisory only.
  const preflightContainer = el("div", { class: "preflight" });

  const tableContainer = el("div", {});

  const refreshPreflight = (queue: ReadonlyArray<QueuedBind | QueuedEdit>) => {
    preflightContainer.innerHTML = "";
    if (queue.length === 0) return;
    try {
      const registry = buildRegistryMap(ctx);
      const items: QueueItem[] = queue
        .filter((q): q is QueuedBind => q.kind === "bind")
        .map((q) => ({
          id: q.id,
          kind: "bind" as const,
          fields: {
            type: q.type,
            description: q.description,
            vendor: q.vendor,
            part_number: q.part_number,
            location: q.location,
            notes: q.notes,
          },
        }));
      const result = runPreflight(items, registry);
      preflightContainer.append(renderPreflight(result));
      // Block submit on policy block OR unknown-id (FE-local).
      const blocked =
        result.decision.kind === "block" ||
        result.localIssues.some((i) => i.kind === "unknown_id");
      (submitBtn as HTMLButtonElement).disabled = blocked;
      submitBtn.setAttribute("data-preflight", blocked ? "blocked" : "ok");
    } catch (e) {
      // WASM not ready or shape mismatch — degrade silently with a hint.
      const msg = (e as Error).message ?? String(e);
      preflightContainer.append(
        el(
          "p",
          { class: "muted small", "data-preflight": "error" },
          `Preflight unavailable: ${msg}`,
        ),
      );
    }
  };

  const renderTable = () => {
    tableContainer.innerHTML = "";
    const queue = loadQueue();
    refreshPreflight(queue);

    const summary = summarizeQueue(queue);
    summaryEl.textContent =
      queue.length === 0
        ? ""
        : `${summary.total} item(s): ${summary.binds} bind, ${summary.edits} edit`;

    const table = el("table", { class: "data bind-queue" });
    const thead = el("thead");
    const tr = el("tr");
    tr.append(el("th", {}, "Kind"));
    tr.append(el("th", {}, "ID"));
    for (const f of FIELDS.filter((f) => f.editable)) {
      tr.append(el("th", {}, f.label));
    }
    tr.append(el("th", {}, "Queued"));
    tr.append(el("th", {}, ""));
    thead.append(tr);
    table.append(thead);

    const tbody = el("tbody");

    for (let i = 0; i < queue.length; i++) {
      const item = queue[i];
      if (item.kind === "bind") {
        tbody.append(renderBindRow(item, i, renderTable));
      } else {
        tbody.append(renderEditRow(item, i, renderTable));
      }
    }

    // Bottom "new bind" row.
    tbody.append(renderEntryRow(ctx, renderTable));

    table.append(tbody);
    tableContainer.append(table);

    if (queue.length === 0) {
      tableContainer.append(
        el("p", { class: "muted small" }, "Queue is empty. Add a bind below, or queue an edit from the Lookup tab."),
      );
    }
  };

  renderTable();

  submitBtn.addEventListener("click", async () => {
    const q = loadQueue();
    if (q.length === 0) {
      alert("Queue is empty.");
      return;
    }

    // Ensure we have a PAT.
    let token = getStoredToken();
    if (!token) {
      token = promptForToken();
      if (!token) return; // cancelled
    }

    const summary = summarizeQueue(q);
    if (
      !confirm(
        `Submit ${summary.total} item(s) (${summary.binds} bind, ${summary.edits} edit) ` +
          `as a PR to ${DATA_REPO_SLUG}?`,
      )
    ) {
      return;
    }

    submitBtn.disabled = true;
    submitBtn.textContent = "Submitting\u2026";

    try {
      const result = await submitBatch(q, token, DATA_REPO_SLUG);
      clearQueue();
      renderTable();
      alert(`PR #${result.prNumber} created.\n\n${result.prUrl}`);
    } catch (e) {
      const msg = e instanceof SubmitError
        ? `Submit failed at step "${e.step}": ${e.message}`
        : `Submit failed: ${(e as Error).message}`;
      console.error("Submit error:", e);

      // If 401/403, the token is probably bad — offer to re-enter.
      if (e instanceof SubmitError && (e.status === 401 || e.status === 403)) {
        if (confirm(`${msg}\n\nThe token may be invalid. Clear it and enter a new one?`)) {
          clearToken();
        }
      } else {
        alert(msg);
      }
    } finally {
      submitBtn.disabled = false;
      submitBtn.textContent = "";
      submitBtn.append(icon("plus"), " Submit batch");
    }
  });

  clearBtn.addEventListener("click", () => {
    if (loadQueue().length === 0) return;
    if (!confirm("Clear the bind queue without submitting?")) return;
    clearQueue();
    renderTable();
  });

  // ---- Batch scan buttons (image upload #99, rolling scan #100) ----

  const makeResolveStatus = (): ((canonical: string) => ScanStatus) => {
    const queuedIds = new Set(loadQueue().map((q) => q.id));
    return (canonical): ScanStatus => {
      if (queuedIds.has(canonical)) return "queued";
      const row = ctx.registry.findById(canonical);
      if (!row) return "unknown";
      if (row.status === "unbound") return "unbound";
      return "bound";
    };
  };

  const addScannedIds = (ids: string[]) => {
    if (ids.length === 0) return;
    const queuedIds = new Set(loadQueue().map((q) => q.id));
    for (const id of ids) {
      if (queuedIds.has(id)) continue;
      appendBind({ id, ...emptyBindFields() });
    }
    renderTable();
  };

  const uploadBtn = button({ class: "secondary" }, icon("upload"), " Upload image");
  uploadBtn.addEventListener("click", async () => {
    try {
      const ids = await openImageScan({
        resolveStatus: makeResolveStatus(),
      });
      addScannedIds(ids);
    } catch {
      /* user cancelled */
    }
  });

  const rollingBtn = button({ class: "secondary" }, icon("list-checks"), " Rolling scan");
  rollingBtn.addEventListener("click", async () => {
    try {
      const ids = await openScannerRolling({
        resolveStatus: makeResolveStatus(),
      });
      addScannedIds(ids);
    } catch {
      /* user cancelled */
    }
  });

  // #92: Repeat mode toggle
  const repeatLabel = el("label", { class: "repeat-mode-toggle" });
  const repeatCb = document.createElement("input");
  repeatCb.type = "checkbox";
  repeatCb.checked = repeatMode;
  repeatCb.addEventListener("change", () => {
    repeatMode = repeatCb.checked;
    if (!repeatMode) lastBindFields = null;
  });
  repeatLabel.append(repeatCb, " Repeat mode (preserve metadata fields)");

  root.append(
    formRow([submitBtn, clearBtn, summaryEl]),
    formRow([uploadBtn, rollingBtn]),
    formRow([repeatLabel]),
    preflightContainer,
    tableContainer,
  );
  return root;
}

// ---------------------------------------------------------------------
// Preflight render (issue #23)
// ---------------------------------------------------------------------

function buildRegistryMap(ctx: AppContext): Map<string, RegistryRow> {
  const map = new Map<string, RegistryRow>();
  for (const row of ctx.registry.all()) {
    map.set(row.id, row);
  }
  return map;
}

function renderPreflight(result: {
  actions: Action[];
  decision: AuthDecision;
  localIssues: Array<{ kind: string; id: string; message: string }>;
}): HTMLElement {
  const wrap = el("div", {
    class: "preflight-card",
    "data-preflight-decision": result.decision.kind,
  });
  // Banner.
  const banner = el("div", { class: `preflight-banner kind-${result.decision.kind}` });
  banner.append(
    el("strong", {}, decisionLabel(result.decision)),
    el("span", { class: "muted" }, " — preflight (advisory; CI is authoritative per ADR-016)"),
  );
  wrap.append(banner);

  if ("reason" in result.decision && result.decision.reason) {
    wrap.append(el("p", { class: "muted small" }, result.decision.reason));
  }
  if ("approver_role" in result.decision && result.decision.approver_role) {
    wrap.append(
      el(
        "p",
        { class: "muted small" },
        `Needs ${result.decision.approver_role} elevation claim.`,
      ),
    );
  }

  // Actions chips.
  if (result.actions.length > 0) {
    const chips = el("div", { class: "chips" });
    for (const a of result.actions) {
      chips.append(
        el("span", { class: `chip chip--${a.kind}` }, a.kind),
      );
    }
    wrap.append(chips);
  }

  // Local issues (unknown-id, duplicate).
  if (result.localIssues.length > 0) {
    const ul = el("ul", { class: "preflight-issues" });
    for (const issue of result.localIssues) {
      ul.append(
        el(
          "li",
          { class: `issue issue--${issue.kind}` },
          `${issue.kind}: ${issue.message}`,
        ),
      );
    }
    wrap.append(ul);
  }
  return wrap;
}

function decisionLabel(d: AuthDecision): string {
  switch (d.kind) {
    case "allow":
      return "Allow";
    case "warn":
      return "Warning";
    case "requires_elevation":
      return "Requires elevation";
    case "block":
      return "Blocked";
  }
}

function renderBindRow(
  item: QueuedBind,
  index: number,
  onChange: () => void,
): HTMLElement {
  const tr = el("tr", { class: "queue-row queue-row--bind", "data-kind": "bind", "data-id": item.id });
  tr.append(el("td", {}, el("span", { class: "chip chip--kind chip--bind" }, "bind")));

  // Editable ID cell — supports blank rows added via "+" button
  const idCell = el("td", { class: "id-cell" });
  const idInp = input({ type: "text", value: item.id ? fmtId(item.id) : "", placeholder: "ID (14-char)", autocapitalize: "characters" });
  idInp.addEventListener("change", () => {
    const raw = idInp.value.trim().toUpperCase().replace(/-/g, "");
    const queue = loadQueue();
    const current = queue[index];
    if (current && current.kind === "bind") {
      current.id = raw;
      saveQueue(queue);
      tr.dataset.id = raw;
    }
  });
  idCell.append(idInp);
  tr.append(idCell);

  for (const f of FIELDS.filter((f) => f.editable)) {
    const cell = el("td");
    const key = f.key as EditableKey;
    const inp = input({ type: "text", value: (item as unknown as Record<string, string>)[key] ?? "" });
    inp.addEventListener("change", () => {
      const queue = loadQueue();
      const current = queue[index];
      if (current && current.kind === "bind") {
        (current as unknown as Record<string, string>)[key] = inp.value;
        saveQueue(queue);
        // #92: track last-edited fields for repeat mode
        if (repeatMode) {
          if (!lastBindFields) lastBindFields = {} as Record<EditableKey, string>;
          lastBindFields[key] = inp.value;
        }
      }
    });
    cell.append(inp);
    tr.append(cell);
  }

  tr.append(
    el(
      "td",
      { class: "muted small" },
      new Date(item.queued_at).toLocaleString(),
    ),
  );

  const trashBtn = button({ class: "icon-only", title: "Remove from queue" }, icon("trash"));
  trashBtn.addEventListener("click", () => {
    removeAt(index);
    onChange();
  });
  tr.append(el("td", { class: "row-actions" }, trashBtn));

  return tr;
}

function renderEditRow(
  item: QueuedEdit,
  index: number,
  onChange: () => void,
): HTMLElement {
  const tr = el("tr", { class: "queue-row queue-row--edit", "data-kind": "edit", "data-id": item.id });
  tr.append(el("td", {}, el("span", { class: "chip chip--kind chip--edit" }, "edit")));
  tr.append(el("td", { class: "id-cell" }, fmtId(item.id)));

  for (const f of FIELDS.filter((f) => f.editable)) {
    const key = f.key as EditableKey;
    const cell = el("td");
    const hasChange = key in item.changes;
    if (hasChange) {
      const before = (item.before as Record<string, string>)[key] ?? "";
      const after = (item.changes as Record<string, string>)[key] ?? "";
      const diffWrap = el("div", { class: "field-diff" });
      diffWrap.append(
        el("span", { class: "field-diff__before" }, before || el("em", { class: "muted" }, "—")),
        el("span", { class: "field-diff__arrow" }, "→"),
        el("span", { class: "field-diff__after" }, after || el("em", { class: "muted" }, "—")),
      );
      cell.append(diffWrap);
    } else {
      cell.append(el("span", { class: "muted" }, "—"));
    }
    tr.append(cell);
  }

  tr.append(
    el(
      "td",
      { class: "muted small" },
      new Date(item.queued_at).toLocaleString(),
    ),
  );

  const trashBtn = button({ class: "icon-only", title: "Remove from queue" }, icon("trash"));
  trashBtn.addEventListener("click", () => {
    removeAt(index);
    onChange();
  });
  tr.append(el("td", { class: "row-actions" }, trashBtn));

  return tr;
}

// #92: Repeat mode state — module-level so it persists across re-renders
let repeatMode = false;
let lastBindFields: Record<EditableKey, string> | null = null;

function renderEntryRow(ctx: AppContext, onAdd: () => void): HTMLElement {
  const tr = el("tr", { class: "entry-row" });

  // "+" button spans the full row — clicking creates a blank bind row
  const editableCount = FIELDS.filter((f) => f.editable).length;
  // +1 Kind, +1 ID, +editableCount fields, +1 Queued, +1 actions = editableCount + 4
  const totalCols = editableCount + 4;
  const addCell = el("td", { colspan: String(totalCols), style: "text-align: center;" });

  const addBlankBtn = button({ class: "secondary small", title: "Add blank row to queue" }, icon("plus"), " Add row");
  addBlankBtn.addEventListener("click", () => {
    const fields = repeatMode && lastBindFields
      ? { ...lastBindFields }
      : emptyBindFields();
    appendBind({ id: "", ...fields });
    onAdd();
    // Focus the ID input of the newly added row (last queue row)
    requestAnimationFrame(() => {
      const rows = document.querySelectorAll<HTMLElement>(".queue-row--bind");
      const lastRow = rows[rows.length - 1];
      const idInp = lastRow?.querySelector<HTMLInputElement>(".id-cell input");
      if (idInp) idInp.focus();
    });
  });

  const scanBtn = button({ class: "secondary small", title: "Scan QR" }, icon("camera"), " Scan");
  scanBtn.addEventListener("click", async () => {
    try {
      const queuedIds = new Set(loadQueue().map((q) => q.id));
      const ids = await openScannerMulti({
        resolveStatus: (canonical): ScanStatus => {
          if (queuedIds.has(canonical)) return "queued";
          const row = ctx.registry.findById(canonical);
          if (!row) return "unknown";
          if (row.status === "unbound") return "unbound";
          return "bound";
        },
      });
      if (ids.length === 0) return;
      for (const id of ids) {
        if (queuedIds.has(id)) continue;
        const fields = repeatMode && lastBindFields
          ? { ...lastBindFields }
          : emptyBindFields();
        appendBind({ id, ...fields });
      }
      onAdd();
    } catch {
      /* user cancelled */
    }
  });

  addCell.append(addBlankBtn, " ", scanBtn);
  tr.append(addCell);
  return tr;
}
