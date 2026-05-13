// Bind tab — queue binds + edits locally as a table; submit-as-PR is
// stubbed (issue #5).
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

import { ID_REGEX } from "../config";
import { FIELDS } from "../registry/schema";
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
import { openScanner, type ScanStatus } from "../ui/scanner";

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
      { class: "muted" },
      "Scan a QR (camera icon in the empty row) or paste an ID to queue a new bind. " +
        "Edits arrive from the Lookup tab — click any row's Edit button there to queue an edit. " +
        "Submit-as-PR is stubbed for the spike (issue #5); the queue is real and persists across reloads.",
    ),
  );

  const submitBtn = button({ class: "primary" }, icon("plus"), " Submit batch (stub)");
  const clearBtn = button({}, icon("trash"), " Clear queue");
  const summaryEl = el("span", { class: "queue-summary muted small" });

  const tableContainer = el("div", {});

  const renderTable = () => {
    tableContainer.innerHTML = "";
    const queue = loadQueue();

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

  submitBtn.addEventListener("click", () => {
    const q = loadQueue();
    if (q.length === 0) {
      alert("Queue is empty.");
      return;
    }
    const summary = summarizeQueue(q);
    // STUB — real GitHub OAuth + REST API path is issue #5.
    console.log(`Pending ${summary.label} batch (would be one PR):`, q);
    alert(
      `${summary.total} item(s) (${summary.binds} bind, ${summary.edits} edit) ` +
        `would be submitted as one ${summary.label} PR.\n\nSee issue #5 for the OAuth + REST integration.`,
    );
  });

  clearBtn.addEventListener("click", () => {
    if (loadQueue().length === 0) return;
    if (!confirm("Clear the bind queue without submitting?")) return;
    clearQueue();
    renderTable();
  });

  root.append(formRow([submitBtn, clearBtn, summaryEl]), tableContainer);
  return root;
}

function renderBindRow(
  item: QueuedBind,
  index: number,
  onChange: () => void,
): HTMLElement {
  const tr = el("tr", { class: "queue-row queue-row--bind", "data-kind": "bind", "data-id": item.id });
  tr.append(el("td", {}, el("span", { class: "chip chip--kind chip--bind" }, "bind")));
  tr.append(el("td", { class: "id-cell" }, fmtId(item.id)));

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

function renderEntryRow(ctx: AppContext, onAdd: () => void): HTMLElement {
  const tr = el("tr", { class: "entry-row" });
  // Spacer for the Kind column so columns line up.
  tr.append(el("td", {}, el("span", { class: "muted small" }, "new")));

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
    const entry: Omit<QueuedBind, "kind" | "queued_at"> = {
      id,
      ...emptyBindFields(),
    };
    for (const [k, inp] of fieldInputs) {
      (entry as unknown as Record<string, string>)[k] = inp.value;
    }
    appendBind(entry);
    onAdd();
  });
  tr.append(el("td", { class: "row-actions" }, addBtn));

  return tr;
}
