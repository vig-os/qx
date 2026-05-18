// Lookup tab — searchable data-grid over the registry (issue #10).
//
// Per ADR-013 + ADR-014 §Consequences: the Lookup tab is the operator's
// primary view. This implementation:
//
//   - top toolbar with fuzzy search + status filter + scan button
//   - table view (sticky header) with every part, status-coloured
//   - row click expands a detail card inline (with Reprint action +
//     a deep-link via `ctx.showPart`)
//   - works for the 0-row case (empty registry → friendly empty state)
//
// Inline edit ships in PR-D (#6) via the bind queue.

import Fuse from "fuse.js";

import { ID_LENGTH, ID_REGEX, DATA_REPO_SLUG, DEFAULT_BRANCH, DEFAULT_SIZE_MM } from "../config";
import { FIELDS, STATUSES, REGISTRY_FIELD_KEYS, type RegistryRow, type Status } from "../registry/schema";
import { appendEdit } from "../registry/queue";
import type { AppContext, Tab } from "../core/types";
import { normalizeCanonicalId } from "../routing/route";
import {
  events,
  EVENT_REPRINT_REQUEST,
  type ReprintRequest,
} from "../core/events";
import { el, button, input, formRow } from "../ui/dom";
import { icon } from "../ui/icons";
import { openScanner, type ScanStatus } from "../ui/scanner";
import { loadPlan, savePlan } from "./print";

type StatusFilter = "all" | Status;

// Columns surfaced in the table view. Subset of `FIELDS` chosen for
// at-a-glance density: id + status + the discriminating metadata
// fields. Edit / Reprint live in the row action cell.
const COLUMNS: { key: keyof RegistryRow; label: string }[] = [
  { key: "id", label: "ID" },
  { key: "status", label: "Status" },
  { key: "type", label: "Type" },
  { key: "vendor", label: "Vendor" },
  { key: "batch", label: "Batch" },
  { key: "location", label: "Location" },
];

function fmtId(id: string): string {
  // 4-4-4 grouping for display; underlying value stays canonical.
  if (id.length < 12) return id;
  return `${id.slice(0, 4)}-${id.slice(4, 8)}-${id.slice(8, 12)}${
    id.length > 12 ? "-" + id.slice(12) : ""
  }`;
}

export const lookupTab: Tab = {
  id: "lookup",
  label: "Lookup",
  mount(container, ctx) {
    container.innerHTML = "";
    container.append(buildUI(ctx));
  },
};

function buildUI(ctx: AppContext): HTMLElement {
  const root = el("div", { class: "tab tab--lookup" });
  const header = el("h2", {}, "Lookup");
  root.append(header);

  // ---------- toolbar ----------
  const searchInput = input({
    type: "search",
    placeholder: "Fuzzy search (id, type, vendor, batch, notes…)",
    autocomplete: "off",
    class: "lookup__search",
  });

  const statusBtns = new Map<StatusFilter, HTMLButtonElement>();
  let statusFilter: StatusFilter = "all";
  const statusBar = el("div", { class: "lookup__status-filter" });
  for (const s of ["all", "unbound", "bound", "void"] as const) {
    const btn = button({ class: `chip chip--filter ${s === "all" ? "active" : ""}` }, s);
    btn.addEventListener("click", () => {
      statusFilter = s;
      for (const [k, b] of statusBtns) {
        b.classList.toggle("active", k === s);
      }
      renderRows();
    });
    statusBtns.set(s, btn);
    statusBar.append(btn);
  }

  const scanBtn = button(
    { class: "icon-only", title: "Scan QR with camera" },
    icon("camera"),
  );
  scanBtn.addEventListener("click", async () => {
    try {
      const text = await openScanner({
        multi: true,
        resolveStatus: (canonical): ScanStatus => {
          const row = ctx.registry.findById(canonical);
          if (!row) return "unknown";
          if (row.status === "unbound") return "unbound";
          return "bound";
        },
      });
      searchInput.value = text;
      renderRows();
    } catch {
      /* cancelled */
    }
  });

  // Issue #91: reprint selected + Issue #94: export CSV
  const reprintSelBtn = button(
    { class: "secondary", disabled: "true" },
    icon("reprint"),
    " Reprint selected",
  );
  const exportCsvBtn = button({}, icon("download"), " Export CSV");

  root.append(
    formRow([searchInput, scanBtn]),
    statusBar,
    formRow([reprintSelBtn, exportCsvBtn]),
  );

  // ---------- table ----------
  const selectedIds = new Set<string>();

  const tableWrap = el("div", { class: "lookup__table-wrap" });
  const table = el("table", { class: "data lookup__table" });
  const thead = el("thead");
  const headRow = el("tr");
  // Issue #91: "select all visible" checkbox
  const selectAllCb = document.createElement("input");
  selectAllCb.type = "checkbox";
  selectAllCb.title = "Select all visible";
  const selectAllTh = el("th", { class: "lookup__th-select" });
  selectAllTh.append(selectAllCb);
  headRow.append(selectAllTh);
  for (const col of COLUMNS) headRow.append(el("th", {}, col.label));
  headRow.append(el("th", { class: "lookup__th-actions" }, ""));
  thead.append(headRow);
  table.append(thead);
  const tbody = el("tbody");
  table.append(tbody);
  tableWrap.append(table);
  root.append(tableWrap);

  const detailCell = el("div", { class: "lookup__detail" });
  root.append(detailCell);

  // Fuse index is rebuilt whenever the registry slice we're showing
  // changes — but the registry itself doesn't mutate during a session
  // (writes go through PR submission), so building once is enough.
  const all = ctx.registry.all();
  const fuse = new Fuse(all, {
    keys: ["id", "type", "vendor", "batch", "location", "notes", "description", "part_number"],
    threshold: 0.4,
    ignoreLocation: true,
  });

  // Track currently visible rows for CSV export and select-all.
  let visibleRows: RegistryRow[] = [];

  const updateReprintBtn = () => {
    (reprintSelBtn as HTMLButtonElement).disabled = selectedIds.size === 0;
  };

  const renderRows = () => {
    tbody.innerHTML = "";
    detailCell.innerHTML = "";
    selectedIds.clear();
    selectAllCb.checked = false;
    updateReprintBtn();

    const q = searchInput.value.trim();
    let rows: RegistryRow[];
    if (!q) {
      rows = all;
    } else {
      const norm = normalizeCanonicalId(q);
      const looksLikeId = ID_REGEX.test(norm) && norm.length === ID_LENGTH;
      if (looksLikeId) {
        const exact = ctx.registry.findById(norm);
        rows = exact ? [exact] : [];
      } else {
        rows = fuse.search(q).map((r) => r.item);
      }
    }
    if (statusFilter !== "all") {
      rows = rows.filter((r) => r.status === statusFilter);
    }

    visibleRows = rows;

    if (rows.length === 0) {
      const td = el("td", { colspan: String(COLUMNS.length + 2), class: "muted" });
      td.append(
        all.length === 0
          ? "Registry is empty. Mint some IDs via the CLI first."
          : "No matches.",
      );
      tbody.append(el("tr", {}, td));
      return;
    }

    const rowCheckboxes: HTMLInputElement[] = [];
    for (const row of rows) {
      const tr = el("tr", { "data-id": row.id, class: `status-${row.status}` });

      // Issue #91: row selection checkbox
      const cb = document.createElement("input");
      cb.type = "checkbox";
      cb.addEventListener("click", (e) => e.stopPropagation());
      cb.addEventListener("change", () => {
        if (cb.checked) selectedIds.add(row.id);
        else selectedIds.delete(row.id);
        selectAllCb.checked = selectedIds.size === rows.length && rows.length > 0;
        updateReprintBtn();
      });
      const cbTd = el("td");
      cbTd.append(cb);
      tr.append(cbTd);
      rowCheckboxes.push(cb);

      for (const col of COLUMNS) {
        const value = row[col.key] ?? "";
        let cell: HTMLElement;
        if (col.key === "id") {
          cell = el("td", { class: "id-cell" });
          cell.append(fmtId(row.id));
        } else if (col.key === "status") {
          cell = el("td");
          cell.append(el("span", { class: `chip chip--status chip--${row.status}` }, row.status));
        } else {
          cell = el("td", {}, value || el("span", { class: "muted" }, "\u2014"));
        }
        tr.append(cell);
      }
      const reprintBtn = button(
        { class: "icon-only", title: "Reprint label" },
        icon("reprint"),
      );
      reprintBtn.addEventListener("click", (e) => {
        e.stopPropagation();
        events.emit<ReprintRequest>(EVENT_REPRINT_REQUEST, { ids: [row.id] });
        ctx.showTab("print");
      });
      tr.append(el("td", { class: "row-actions" }, reprintBtn));
      tr.addEventListener("click", () => {
        ctx.showPart(row.id);
        detailCell.innerHTML = "";
        detailCell.append(renderDetailView(row, ctx));
      });
      tbody.append(tr);
    }

    // Wire up "select all" checkbox
    selectAllCb.onchange = () => {
      for (let i = 0; i < rows.length; i++) {
        rowCheckboxes[i].checked = selectAllCb.checked;
        if (selectAllCb.checked) selectedIds.add(rows[i].id);
        else selectedIds.delete(rows[i].id);
      }
      updateReprintBtn();
    };
  };

  // Issue #91: Reprint selected — write selected IDs to print plan, switch to Print tab.
  reprintSelBtn.addEventListener("click", () => {
    if (selectedIds.size === 0) return;
    const plan = loadPlan();
    for (const id of selectedIds) {
      plan.push({
        id,
        layoutId: "horz",
        size: DEFAULT_SIZE_MM,
        copies: 1,
        extras: {},
      });
    }
    savePlan(plan);
    ctx.showTab("print");
  });

  // Issue #94: Export filtered view as CSV download.
  exportCsvBtn.addEventListener("click", () => {
    if (visibleRows.length === 0) return;
    const keys = REGISTRY_FIELD_KEYS;
    const header = keys.join(",");
    const lines = visibleRows.map((row) =>
      keys
        .map((k) => {
          const v = (row as unknown as Record<string, string>)[k] ?? "";
          // Escape fields containing commas, quotes, or newlines.
          if (/[,"\n]/.test(v)) return `"${v.replace(/"/g, '""')}"`;
          return v;
        })
        .join(","),
    );
    const csv = [header, ...lines].join("\n") + "\n";
    const blob = new Blob([csv], { type: "text/csv;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `registry-export-${new Date().toISOString().slice(0, 10)}.csv`;
    document.body.append(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
  });

  searchInput.addEventListener("input", renderRows);

  // Deep-link: if URL is /<ID>, open the detail card directly.
  const route = ctx.getRoute();
  if (route.kind === "part") {
    searchInput.value = route.id;
  }
  renderRows();
  if (route.kind === "part") {
    const row = ctx.registry.findById(route.id);
    if (row) detailCell.append(renderDetailView(row, ctx));
  }

  return root;
}

// Fields the operator can edit from the Lookup detail card.
// `status` is editable here (not in the bind form) because mid-life
// status changes ("mark void") are an edit-only operation per #6.
const EDIT_FIELD_KEYS: (keyof RegistryRow)[] = [
  "status",
  "type",
  "description",
  "vendor",
  "part_number",
  "location",
  "notes",
];

function renderDetailView(row: RegistryRow, ctx: AppContext): HTMLElement {
  const wrap = el("div", { class: "row-detail" });
  wrap.append(el("h3", { class: "row-detail__id" }, fmtId(row.id)));
  const dl = el("dl");
  for (const f of FIELDS) {
    const value = (row as unknown as Record<string, string>)[f.key] ?? "";
    dl.append(el("dt", {}, f.label));
    dl.append(
      el(
        "dd",
        {},
        value || el("span", { class: "muted" }, "\u2014"),
      ),
    );
  }
  wrap.append(dl);

  // Issue #95: Audit trail — surface provenance fields prominently.
  const auditFields: { label: string; value: string }[] = [
    { label: "Minted by", value: row.minted_by },
    { label: "Bound by", value: row.bound_by },
    { label: "Last edited at", value: row.last_edited_at },
    { label: "Last edited by", value: row.last_edited_by },
  ];
  const auditDl = el("dl", { class: "row-detail__audit" });
  for (const af of auditFields) {
    auditDl.append(el("dt", {}, af.label));
    auditDl.append(
      el("dd", {}, af.value || el("span", { class: "muted" }, "\u2014")),
    );
  }
  wrap.append(el("h4", { class: "row-detail__audit-heading" }, "Audit trail"));
  wrap.append(auditDl);

  const historyLink = el("a", {
    href: `https://github.com/${DATA_REPO_SLUG}/commits/${DEFAULT_BRANCH}/registry.csv`,
    target: "_blank",
    rel: "noopener",
    class: "row-detail__history",
  }, "View history");
  wrap.append(historyLink);

  const editBtn = button(
    { class: "secondary row-detail__edit" },
    icon("plus"),
    " Edit",
  );
  editBtn.addEventListener("click", () => {
    const replacement = renderDetailEdit(row, ctx);
    wrap.replaceWith(replacement);
  });

  const reprintBtn = button(
    { class: "primary" },
    icon("reprint"),
    " Reprint label",
  );
  reprintBtn.addEventListener("click", () => {
    events.emit<ReprintRequest>(EVENT_REPRINT_REQUEST, { ids: [row.id] });
  });
  wrap.append(formRow([editBtn, reprintBtn]));
  return wrap;
}

function renderDetailEdit(row: RegistryRow, ctx: AppContext): HTMLElement {
  const wrap = el("div", { class: "row-detail row-detail--edit" });
  wrap.append(el("h3", { class: "row-detail__id" }, fmtId(row.id)));

  const form = el("form", { class: "row-detail__form" });
  const inputs = new Map<keyof RegistryRow, HTMLInputElement | HTMLSelectElement>();

  for (const key of EDIT_FIELD_KEYS) {
    const fieldDef = FIELDS.find((f) => f.key === key);
    const label = fieldDef?.label ?? key;
    const value = (row as unknown as Record<string, string>)[key] ?? "";

    const labelEl = el("label", { class: "row-detail__field" });
    labelEl.append(el("span", { class: "row-detail__label" }, label));

    let field: HTMLInputElement | HTMLSelectElement;
    if (key === "status") {
      const select = document.createElement("select");
      for (const s of STATUSES) {
        const opt = document.createElement("option");
        opt.value = s;
        opt.textContent = s;
        if (s === row.status) opt.selected = true;
        select.append(opt);
      }
      field = select;
    } else {
      field = input({ type: "text", value });
    }
    field.classList.add("row-detail__input");
    field.dataset.key = key;
    inputs.set(key, field);
    labelEl.append(field);
    form.append(labelEl);
  }
  wrap.append(form);

  const errMsg = el("p", { class: "row-detail__error muted small" });
  wrap.append(errMsg);

  const saveBtn = button({ class: "primary", type: "button" }, icon("plus"), " Queue edit");
  saveBtn.addEventListener("click", () => {
    const changes: Partial<RegistryRow> = {};
    const before: Partial<RegistryRow> = {};
    for (const key of EDIT_FIELD_KEYS) {
      const field = inputs.get(key);
      if (!field) continue;
      const newVal = field.value;
      const oldVal = (row as unknown as Record<string, string>)[key] ?? "";
      if (newVal !== oldVal) {
        (changes as Record<string, string>)[key] = newVal;
        (before as Record<string, string>)[key] = oldVal;
      }
    }
    if (Object.keys(changes).length === 0) {
      errMsg.textContent = "No changes to queue.";
      return;
    }
    // Guardrail per #6: void → bound is a privileged transition.
    if (row.status === "void" && changes.status && changes.status !== "void") {
      if (!confirm(
        `${row.id} is voided. Re-binding a voided ID requires the back-office --force ` +
          `equivalent (not implemented in the FE). Queue anyway?`,
      )) {
        return;
      }
    }
    appendEdit(row.id, before, changes);
    ctx.showTab("bind");
  });

  const cancelBtn = button({ type: "button" }, "Cancel");
  cancelBtn.addEventListener("click", () => {
    wrap.replaceWith(renderDetailView(row, ctx));
  });

  wrap.append(formRow([saveBtn, cancelBtn]));
  return wrap;
}
