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

import { ID_LENGTH, ID_REGEX } from "../config";
import { FIELDS, STATUSES, type RegistryRow, type Status } from "../registry/schema";
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

  root.append(
    formRow([searchInput, scanBtn]),
    statusBar,
  );

  // ---------- table ----------
  const tableWrap = el("div", { class: "lookup__table-wrap" });
  const table = el("table", { class: "data lookup__table" });
  const thead = el("thead");
  const headRow = el("tr");
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

  const renderRows = () => {
    tbody.innerHTML = "";
    detailCell.innerHTML = "";

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

    if (rows.length === 0) {
      const td = el("td", { colspan: String(COLUMNS.length + 1), class: "muted" });
      td.append(
        all.length === 0
          ? "Registry is empty. Mint some IDs via the CLI first."
          : "No matches.",
      );
      tbody.append(el("tr", {}, td));
      return;
    }

    for (const row of rows) {
      const tr = el("tr", { "data-id": row.id, class: `status-${row.status}` });
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
          cell = el("td", {}, value || el("span", { class: "muted" }, "—"));
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
  };

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
        value || el("span", { class: "muted" }, "—"),
      ),
    );
  }
  wrap.append(dl);

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
