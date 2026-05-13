// Bind tab — queue binds locally as a table; submit-as-PR is stubbed
// (issue #5).
//
// UX shape: a table where each row is a queued bind, fully inline-
// editable (type / description / vendor / part_number / location /
// notes). Delete via trash icon. The bottom row is an "empty entry"
// row whose ID field has a camera-icon button to scan a QR — that's
// the primary entry point. Manual paste is also supported.

import { ID_REGEX } from "../config";
import { FIELDS, type RegistryRow } from "../registry/schema";
import { runPreflight, type QueueItem } from "../registry/preflight";
import type { AppContext, Tab } from "../core/types";
import { el, button, input, formRow } from "../ui/dom";
import { icon } from "../ui/icons";
import { openScanner, type ScanStatus } from "../ui/scanner";
import type { Action, AuthDecision } from "../wasm/loader";

const QUEUE_KEY = "part-registry.bind-queue";

type EditableKey =
  | "type"
  | "description"
  | "vendor"
  | "part_number"
  | "location"
  | "notes";

interface QueuedBind {
  id: string;
  queued_at: string;
  type: string;
  description: string;
  vendor: string;
  part_number: string;
  location: string;
  notes: string;
}

function emptyEntry(): Omit<QueuedBind, "id" | "queued_at"> {
  return {
    type: "",
    description: "",
    vendor: "",
    part_number: "",
    location: "",
    notes: "",
  };
}

function loadQueue(): QueuedBind[] {
  try {
    const raw = localStorage.getItem(QUEUE_KEY);
    if (!raw) return [];
    return JSON.parse(raw) as QueuedBind[];
  } catch {
    return [];
  }
}

function saveQueue(q: QueuedBind[]): void {
  localStorage.setItem(QUEUE_KEY, JSON.stringify(q));
}

function fmtId(id: string): string {
  // 4-4-4 grouping for display; underlying value stays canonical.
  if (id.length < 12) return id;
  return `${id.slice(0, 4)}-${id.slice(4, 8)}-${id.slice(8, 12)}${id.length > 12 ? '-' + id.slice(12) : ''}`;
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
  root.append(el("h2", {}, "Bind queue"));
  root.append(
    el(
      "p",
      { class: "muted" },
      "Scan a QR (camera icon in the empty row) or paste an ID, fill the metadata, queue. Submit-as-PR is stubbed for the spike (issue #5); the queue is real and persists across reloads.",
    ),
  );

  const submitBtn = button({ class: "primary" }, icon("plus"), " Submit batch (stub)");
  const clearBtn = button({}, icon("trash"), " Clear queue");

  // ADR-016 §"FE preflight" + issue #23: every queue mutation re-runs
  // the same classify + policy engine the CI gate runs, advisory only.
  const preflightContainer = el("div", { class: "preflight" });

  const tableContainer = el("div", {});

  const refreshPreflight = (queue: QueuedBind[]) => {
    preflightContainer.innerHTML = "";
    if (queue.length === 0) return;
    try {
      const registry = buildRegistryMap(ctx);
      const items: QueueItem[] = queue.map((q) => ({
        id: q.id,
        kind: "bind",
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
    const table = el("table", { class: "data" });
    const thead = el("thead");
    const tr = el("tr");
    tr.append(el("th", {}, "ID"));
    for (const f of FIELDS.filter((f) => f.editable)) {
      tr.append(el("th", {}, f.label));
    }
    tr.append(el("th", {}, "Queued"));
    tr.append(el("th", {}, ""));
    thead.append(tr);
    table.append(thead);

    const tbody = el("tbody");

    // Existing queued rows: editable inline.
    for (let i = 0; i < queue.length; i++) {
      tbody.append(renderQueueRow(queue[i], i, () => renderTable()));
    }

    // Bottom "new entry" row.
    tbody.append(renderEntryRow(ctx, () => renderTable()));

    table.append(tbody);
    tableContainer.append(table);

    if (queue.length === 0) {
      tableContainer.append(
        el("p", { class: "muted small" }, "Queue is empty. Add a row below."),
      );
    }
  };

  renderTable();

  submitBtn.addEventListener("click", () => {
    const q = loadQueue();
    if (q.length === 0) {
      alert("Queue is empty.");
      return;
    }
    // STUB — real GitHub OAuth + REST API path is issue #5.
    console.log("Pending binds (would be POSTed as a single PR):", q);
    alert(
      `${q.length} bind(s) would be submitted as one PR.\n\nSee issue #5 for the OAuth + REST integration.`,
    );
  });

  clearBtn.addEventListener("click", () => {
    if (loadQueue().length === 0) return;
    if (!confirm("Clear the bind queue without submitting?")) return;
    saveQueue([]);
    renderTable();
  });

  root.append(
    formRow([submitBtn, clearBtn]),
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

function renderQueueRow(item: QueuedBind, index: number, onChange: () => void): HTMLElement {
  const tr = el("tr");
  tr.append(el("td", { class: "id-cell" }, fmtId(item.id)));

  for (const f of FIELDS.filter((f) => f.editable)) {
    const cell = el("td");
    const key = f.key as EditableKey;
    const inp = input({ type: "text", value: item[key] });
    inp.addEventListener("change", () => {
      const queue = loadQueue();
      if (queue[index]) {
        queue[index][key] = inp.value;
        saveQueue(queue);
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
    const queue = loadQueue();
    queue.splice(index, 1);
    saveQueue(queue);
    onChange();
  });
  tr.append(el("td", { class: "row-actions" }, trashBtn));

  return tr;
}

function renderEntryRow(ctx: AppContext, onAdd: () => void): HTMLElement {
  const tr = el("tr", { class: "entry-row" });
  const idInput = input({
    type: "text",
    placeholder: "ID (14-char)",
    autocapitalize: "characters",
  });
  const scanBtn = button({ class: "icon-only", title: "Scan QR" }, icon("camera"));
  scanBtn.addEventListener("click", async () => {
    try {
      // Snapshot multi-pick: greys out IDs already in the queue
      // (so the operator can see at a glance which on-bench parts
      // are still un-queued) and reds out IDs not in the registry.
      const queuedIds = new Set(loadQueue().map((q) => q.id));
      const v = await openScanner({
        multi: true,
        resolveStatus: (canonical): ScanStatus => {
          if (queuedIds.has(canonical)) return "queued";
          const row = ctx.registry.findById(canonical);
          if (!row) return "unknown";
          if (row.status === "unbound") return "unbound";
          // bound / void — operator can technically re-bind, but
          // visually deprioritise.
          return "bound";
        },
      });
      idInput.value = v.toUpperCase().replace(/-/g, "");
      idInput.focus();
    } catch {
      /* user cancelled */
    }
  });
  const idCell = el("td", { class: "id-cell" });
  const idWrap = el("div", { style: "display:flex; gap:4px;" });
  idWrap.append(idInput, scanBtn);
  idCell.append(idWrap);
  tr.append(idCell);

  const fieldInputs = new Map<EditableKey, HTMLInputElement>();
  for (const f of FIELDS.filter((f) => f.editable)) {
    const inp = input({ type: "text", placeholder: f.label });
    fieldInputs.set(f.key as EditableKey, inp);
    tr.append(el("td", {}, inp));
  }

  tr.append(el("td", {}, ""));

  const addBtn = button({ class: "icon-only primary", title: "Queue this bind" }, icon("plus"));
  addBtn.addEventListener("click", () => {
    const id = idInput.value.trim().toUpperCase().replace(/-/g, "");
    if (!ID_REGEX.test(id)) {
      alert("ID must be 14 chars from the canonical alphabet.");
      return;
    }
    const existing = ctx.registry.findById(id);
    if (existing && existing.status === "void") {
      alert(`${id} is voided. Cannot bind.`);
      return;
    }
    if (!existing && !confirm(`${id} is not in the loaded registry. Queue anyway?`)) {
      return;
    }
    const queue = loadQueue();
    const entry: QueuedBind = {
      id,
      queued_at: new Date().toISOString(),
      ...emptyEntry(),
    };
    for (const [k, inp] of fieldInputs) entry[k] = inp.value;
    queue.push(entry);
    saveQueue(queue);
    onAdd();
  });
  tr.append(el("td", { class: "row-actions" }, addBtn));

  return tr;
}
