// Lookup tab — searchable data-grid over the registry (issue #10).
//
// Per ADR-013 + ADR-014 Consequences: the Lookup tab is the operator's
// primary view. This implementation:
//
//   - top toolbar with fuzzy search + status filter + scan button
//   - structured column filters (vendor, location, type, batch) #93
//   - sortable column headers #93
//   - filter deep-link via URL params #93
//   - table / dashboard view toggle #98
//   - void workflow from detail card #96
//   - table view (sticky header) with every part, status-coloured
//   - row click expands a detail card inline (with Reprint action +
//     a deep-link via `ctx.showPart`)
//   - works for the 0-row case (empty registry -> friendly empty state)
//
// Inline edit ships in PR-D (#6) via the bind queue.

import Fuse from "fuse.js";

import { ID_LENGTH, ID_REGEX, DATA_REPO_SLUG, DEFAULT_BRANCH, DEFAULT_SIZE_MM } from "../config";
import { FIELDS, STATUSES, REGISTRY_FIELD_KEYS, type RegistryRow, type Status } from "../registry/schema";
import { appendEdit, appendVoid } from "../registry/queue";
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

// #93: sortable columns
type SortDir = "asc" | "desc" | "none";
interface SortState {
  key: keyof RegistryRow;
  dir: SortDir;
}

// #93: structured filter keys
const FILTER_KEYS = ["vendor", "location", "type", "batch"] as const;
type FilterKey = (typeof FILTER_KEYS)[number];

function fmtId(id: string): string {
  // 4-4-4 grouping for display; underlying value stays canonical.
  if (id.length < 12) return id;
  return `${id.slice(0, 4)}-${id.slice(4, 8)}-${id.slice(8, 12)}${
    id.length > 12 ? "-" + id.slice(12) : ""
  }`;
}

// #93: read filter state from URL search params
function readFilterParams(): {
  q: string;
  status: StatusFilter;
  filters: Record<FilterKey, string>;
} {
  const params = new URLSearchParams(window.location.search);
  const filters = {} as Record<FilterKey, string>;
  for (const k of FILTER_KEYS) {
    filters[k] = params.get(k) ?? "";
  }
  return {
    q: params.get("q") ?? "",
    status: (params.get("status") as StatusFilter) || "all",
    filters,
  };
}

// #93: write filter state to URL without navigation
function writeFilterParams(
  q: string,
  status: StatusFilter,
  filters: Record<FilterKey, string>,
): void {
  const params = new URLSearchParams();
  if (q) params.set("q", q);
  if (status !== "all") params.set("status", status);
  for (const k of FILTER_KEYS) {
    if (filters[k]) params.set(k, filters[k]);
  }
  const qs = params.toString();
  const url = window.location.pathname + (qs ? `?${qs}` : "");
  history.replaceState(null, "", url);
}

// #93: unique non-empty values for a field across the registry
function uniqueValues(rows: RegistryRow[], key: keyof RegistryRow): string[] {
  const set = new Set<string>();
  for (const r of rows) {
    const v = (r as unknown as Record<string, string>)[key];
    if (v) set.add(v);
  }
  return [...set].sort();
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

  const all = ctx.registry.all();

  // ---------- state ----------
  const savedParams = readFilterParams();
  let statusFilter: StatusFilter = savedParams.status;
  const columnFilters: Record<FilterKey, string> = { ...savedParams.filters };
  let sortState: SortState = { key: "id", dir: "none" };
  let viewMode: "table" | "dashboard" = "table";

  // ---------- toolbar ----------
  const searchInput = input({
    type: "search",
    placeholder: "Fuzzy search (id, type, vendor, batch, notes...)",
    autocomplete: "off",
    class: "lookup__search",
  });
  if (savedParams.q) searchInput.value = savedParams.q;

  const statusBtns = new Map<StatusFilter, HTMLButtonElement>();
  const statusBar = el("div", { class: "lookup__status-filter" });
  for (const s of ["all", "unbound", "bound", "void"] as const) {
    const btn = button(
      { class: `chip chip--filter ${s === statusFilter ? "active" : ""}` },
      s,
    );
    btn.addEventListener("click", () => {
      statusFilter = s;
      for (const [k, b] of statusBtns) {
        b.classList.toggle("active", k === s);
      }
      renderView();
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
      renderView();
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

  // #93: structured filter dropdowns
  const filterBar = el("div", { class: "lookup__filter-bar" });
  const filterSelects = new Map<FilterKey, HTMLSelectElement>();
  for (const fk of FILTER_KEYS) {
    const label = fk.charAt(0).toUpperCase() + fk.slice(1);
    const sel = document.createElement("select");
    sel.className = "lookup__filter-select";
    sel.title = `Filter by ${label}`;
    const defaultOpt = document.createElement("option");
    defaultOpt.value = "";
    defaultOpt.textContent = `All ${label}s`;
    sel.append(defaultOpt);
    for (const v of uniqueValues(all, fk)) {
      const opt = document.createElement("option");
      opt.value = v;
      opt.textContent = v;
      if (columnFilters[fk] === v) opt.selected = true;
      sel.append(opt);
    }
    sel.addEventListener("change", () => {
      columnFilters[fk] = sel.value;
      renderView();
    });
    filterSelects.set(fk, sel);
    filterBar.append(sel);
  }
  const clearFiltersBtn = button({ class: "small" }, "Clear filters");
  clearFiltersBtn.addEventListener("click", () => {
    for (const fk of FILTER_KEYS) {
      columnFilters[fk] = "";
      const sel = filterSelects.get(fk);
      if (sel) sel.value = "";
    }
    searchInput.value = "";
    statusFilter = "all";
    for (const [k, b] of statusBtns) b.classList.toggle("active", k === "all");
    renderView();
  });
  filterBar.append(clearFiltersBtn);

  // #98: view mode toggle
  const viewToggle = el("div", { class: "lookup__view-toggle" });
  const tableToggleBtn = button({ class: "chip chip--filter active" }, "Table");
  const dashToggleBtn = button({ class: "chip chip--filter" }, "Dashboard");
  tableToggleBtn.addEventListener("click", () => {
    viewMode = "table";
    tableToggleBtn.classList.add("active");
    dashToggleBtn.classList.remove("active");
    renderView();
  });
  dashToggleBtn.addEventListener("click", () => {
    viewMode = "dashboard";
    dashToggleBtn.classList.add("active");
    tableToggleBtn.classList.remove("active");
    renderView();
  });
  viewToggle.append(tableToggleBtn, dashToggleBtn);

  root.append(
    formRow([searchInput, scanBtn]),
    statusBar,
    filterBar,
    formRow([reprintSelBtn, exportCsvBtn, viewToggle]),
  );

  // ---------- containers ----------
  const selectedIds = new Set<string>();
  const contentContainer = el("div", { class: "lookup__content" });
  root.append(contentContainer);

  const detailCell = el("div", { class: "lookup__detail" });
  root.append(detailCell);

  // Fuse index
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

  // ---------- compute filtered + sorted rows ----------
  function computeRows(): RegistryRow[] {
    const q = searchInput.value.trim();
    let rows: RegistryRow[];
    if (!q) {
      rows = [...all];
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
    // #93: apply structured filters
    for (const fk of FILTER_KEYS) {
      const fv = columnFilters[fk];
      if (fv) {
        rows = rows.filter(
          (r) => (r as unknown as Record<string, string>)[fk] === fv,
        );
      }
    }
    // #93: sort
    if (sortState.dir !== "none") {
      const key = sortState.key;
      const dir = sortState.dir === "asc" ? 1 : -1;
      rows.sort((a, b) => {
        const va = (a as unknown as Record<string, string>)[key] ?? "";
        const vb = (b as unknown as Record<string, string>)[key] ?? "";
        return va < vb ? -dir : va > vb ? dir : 0;
      });
    }
    return rows;
  }

  // ---------- build table header with sort indicators (#93) ----------
  function buildTableHead(): HTMLElement {
    const thead = el("thead");
    const headRow = el("tr");

    // select-all checkbox
    const selectAllCb = document.createElement("input");
    selectAllCb.type = "checkbox";
    selectAllCb.title = "Select all visible";
    const selectAllTh = el("th", { class: "lookup__th-select" });
    selectAllTh.append(selectAllCb);
    headRow.append(selectAllTh);

    for (const col of COLUMNS) {
      const th = el("th", { class: "lookup__th-sortable" });
      const sortIndicator =
        sortState.key === col.key && sortState.dir !== "none"
          ? sortState.dir === "asc"
            ? " \u25B2"
            : " \u25BC"
          : "";
      th.textContent = col.label + sortIndicator;
      th.style.cursor = "pointer";
      th.addEventListener("click", () => {
        if (sortState.key === col.key) {
          // cycle: asc -> desc -> none
          sortState.dir =
            sortState.dir === "asc"
              ? "desc"
              : sortState.dir === "desc"
                ? "none"
                : "asc";
        } else {
          sortState = { key: col.key, dir: "asc" };
        }
        renderView();
      });
      headRow.append(th);
    }
    headRow.append(el("th", { class: "lookup__th-actions" }, ""));
    thead.append(headRow);

    // Store selectAllCb reference for wiring in renderTableBody
    (thead as unknown as Record<string, unknown>)._selectAllCb = selectAllCb;
    return thead;
  }

  // ---------- render table body ----------
  function renderTableBody(
    rows: RegistryRow[],
    tbody: HTMLElement,
    selectAllCb: HTMLInputElement,
  ): void {
    tbody.innerHTML = "";
    selectedIds.clear();
    selectAllCb.checked = false;
    updateReprintBtn();

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

      const cb = document.createElement("input");
      cb.type = "checkbox";
      cb.addEventListener("click", (e) => e.stopPropagation());
      cb.addEventListener("change", () => {
        if (cb.checked) selectedIds.add(row.id);
        else selectedIds.delete(row.id);
        selectAllCb.checked =
          selectedIds.size === rows.length && rows.length > 0;
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
          cell.append(
            el(
              "span",
              { class: `chip chip--status chip--${row.status}` },
              row.status,
            ),
          );
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

    selectAllCb.onchange = () => {
      for (let i = 0; i < rows.length; i++) {
        rowCheckboxes[i].checked = selectAllCb.checked;
        if (selectAllCb.checked) selectedIds.add(rows[i].id);
        else selectedIds.delete(rows[i].id);
      }
      updateReprintBtn();
    };
  }

  // ---------- #98: dashboard view ----------
  function renderDashboard(rows: RegistryRow[]): HTMLElement {
    const dash = el("div", { class: "lookup__dashboard" });

    // Summary cards
    const summaryRow = el("div", { class: "dashboard__summary" });
    const totalCard = el("article", { class: "dashboard__card" });
    totalCard.append(
      el("h4", {}, "Total"),
      el("p", { class: "dashboard__number" }, String(rows.length)),
    );
    summaryRow.append(totalCard);

    // Status breakdown
    const statusCounts: Record<string, number> = {};
    for (const r of rows) {
      statusCounts[r.status] = (statusCounts[r.status] || 0) + 1;
    }
    for (const s of ["unbound", "bound", "void"] as Status[]) {
      const count = statusCounts[s] || 0;
      const card = el("article", { class: `dashboard__card dashboard__card--${s}` });
      card.append(
        el("h4", {}, s.charAt(0).toUpperCase() + s.slice(1)),
        el("p", { class: "dashboard__number" }, String(count)),
      );
      card.style.cursor = "pointer";
      card.addEventListener("click", () => {
        statusFilter = s;
        for (const [k, b] of statusBtns) b.classList.toggle("active", k === s);
        viewMode = "table";
        tableToggleBtn.classList.add("active");
        dashToggleBtn.classList.remove("active");
        renderView();
      });
      summaryRow.append(card);
    }

    // Batch count card
    const batchSet = new Set(rows.map((r) => r.batch).filter(Boolean));
    const batchCard = el("article", { class: "dashboard__card" });
    batchCard.append(
      el("h4", {}, "Batches"),
      el("p", { class: "dashboard__number" }, String(batchSet.size)),
    );
    summaryRow.append(batchCard);
    dash.append(summaryRow);

    // Group-by sections
    const groupKeys: { key: FilterKey; label: string }[] = [
      { key: "batch", label: "Batch" },
      { key: "location", label: "Location" },
      { key: "vendor", label: "Vendor" },
    ];

    for (const gk of groupKeys) {
      const groups = new Map<string, number>();
      for (const r of rows) {
        const v = (r as unknown as Record<string, string>)[gk.key] || "(empty)";
        groups.set(v, (groups.get(v) || 0) + 1);
      }
      if (groups.size === 0) continue;

      const section = el("div", { class: "dashboard__section" });
      section.append(el("h3", {}, `By ${gk.label}`));

      const sorted = [...groups.entries()].sort((a, b) => b[1] - a[1]);
      for (const [groupVal, count] of sorted) {
        const pct = rows.length > 0 ? Math.round((count / rows.length) * 100) : 0;
        const bar = el("div", { class: "dashboard__bar-row" });
        const labelEl = el("span", { class: "dashboard__bar-label" }, groupVal);
        labelEl.style.cursor = "pointer";
        labelEl.addEventListener("click", () => {
          if (groupVal !== "(empty)") {
            columnFilters[gk.key] = groupVal;
            const sel = filterSelects.get(gk.key);
            if (sel) sel.value = groupVal;
          }
          viewMode = "table";
          tableToggleBtn.classList.add("active");
          dashToggleBtn.classList.remove("active");
          renderView();
        });
        const track = el("div", { class: "dashboard__bar-track" });
        const fill = el("div", { class: "dashboard__bar-fill" });
        fill.style.width = `${pct}%`;
        track.append(fill);
        const countEl = el("span", { class: "dashboard__bar-count muted small" }, `${count} (${pct}%)`);
        bar.append(labelEl, track, countEl);
        section.append(bar);
      }
      dash.append(section);
    }

    // Status group with progress bars
    const statusSection = el("div", { class: "dashboard__section" });
    statusSection.append(el("h3", {}, "By Status"));
    for (const s of ["unbound", "bound", "void"] as Status[]) {
      const count = statusCounts[s] || 0;
      const pct = rows.length > 0 ? Math.round((count / rows.length) * 100) : 0;
      const bar = el("div", { class: "dashboard__bar-row" });
      const labelEl = el("span", { class: "dashboard__bar-label" }, s);
      labelEl.style.cursor = "pointer";
      labelEl.addEventListener("click", () => {
        statusFilter = s;
        for (const [k, b] of statusBtns) b.classList.toggle("active", k === s);
        viewMode = "table";
        tableToggleBtn.classList.add("active");
        dashToggleBtn.classList.remove("active");
        renderView();
      });
      const track = el("div", { class: "dashboard__bar-track" });
      const fill = el("div", { class: `dashboard__bar-fill dashboard__bar-fill--${s}` });
      fill.style.width = `${pct}%`;
      track.append(fill);
      const countEl = el("span", { class: "dashboard__bar-count muted small" }, `${count} (${pct}%)`);
      bar.append(labelEl, track, countEl);
      statusSection.append(bar);
    }
    dash.append(statusSection);

    return dash;
  }

  // ---------- main render ----------
  const renderView = () => {
    contentContainer.innerHTML = "";
    detailCell.innerHTML = "";

    // #93: sync URL
    writeFilterParams(searchInput.value.trim(), statusFilter, columnFilters);

    const rows = computeRows();
    visibleRows = rows;

    if (viewMode === "dashboard") {
      contentContainer.append(renderDashboard(rows));
      return;
    }

    // Table view
    const tableWrap = el("div", { class: "lookup__table-wrap" });
    const table = el("table", { class: "data lookup__table" });
    const thead = buildTableHead();
    const selectAllCb = (thead as unknown as Record<string, unknown>)
      ._selectAllCb as HTMLInputElement;
    table.append(thead);
    const tbody = el("tbody");
    table.append(tbody);
    tableWrap.append(table);
    contentContainer.append(tableWrap);

    renderTableBody(rows, tbody, selectAllCb);
  };

  // Issue #91: Reprint selected
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
    const csvHeader = keys.join(",");
    const lines = visibleRows.map((row) =>
      keys
        .map((k) => {
          const v = (row as unknown as Record<string, string>)[k] ?? "";
          if (/[,"\n]/.test(v)) return `"${v.replace(/"/g, '""')}"`;
          return v;
        })
        .join(","),
    );
    const csv = [csvHeader, ...lines].join("\n") + "\n";
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

  searchInput.addEventListener("input", renderView);

  // Deep-link: if URL is /<ID>, open the detail card directly.
  const route = ctx.getRoute();
  if (route.kind === "part") {
    searchInput.value = route.id;
  }
  renderView();
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

  // Issue #95: Audit trail
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

  // #96: Void button — only for bound/unbound parts (not already void)
  const actionChildren: (HTMLElement | null)[] = [editBtn, reprintBtn];
  if (row.status !== "void") {
    const voidBtn = button(
      { class: "row-detail__void" },
      "Void",
    );
    voidBtn.addEventListener("click", () => {
      // Replace detail view with void confirmation
      wrap.replaceWith(renderVoidConfirm(row, ctx));
    });
    actionChildren.push(voidBtn);
  }
  wrap.append(formRow(actionChildren));
  return wrap;
}

// #96: Void confirmation UI
function renderVoidConfirm(row: RegistryRow, ctx: AppContext): HTMLElement {
  const wrap = el("div", { class: "row-detail row-detail--void-confirm" });
  wrap.append(
    el("h3", { class: "row-detail__id" }, fmtId(row.id)),
    el("p", { class: "error" }, `Are you sure you want to void ${fmtId(row.id)}?`),
    el("p", { class: "muted small" }, "This will queue a status change to 'void'. The change must be submitted via the Bind tab."),
  );

  const reasonLabel = el("label", {});
  reasonLabel.append(el("span", {}, "Reason (required):"));
  const reasonTextarea = document.createElement("textarea");
  reasonTextarea.className = "row-detail__input";
  reasonTextarea.rows = 3;
  reasonTextarea.placeholder = "Why is this part being voided?";
  reasonLabel.append(reasonTextarea);
  wrap.append(reasonLabel);

  const errMsg = el("p", { class: "row-detail__error muted small" });
  wrap.append(errMsg);

  const confirmBtn = button({ class: "row-detail__void" }, "Confirm void");
  confirmBtn.addEventListener("click", () => {
    const reason = reasonTextarea.value.trim();
    if (!reason) {
      errMsg.textContent = "A reason is required to void a part.";
      return;
    }
    appendVoid(row.id, reason);
    ctx.showTab("bind");
  });

  const cancelBtn = button({ type: "button" }, "Cancel");
  cancelBtn.addEventListener("click", () => {
    wrap.replaceWith(renderDetailView(row, ctx));
  });

  wrap.append(formRow([confirmBtn, cancelBtn]));
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
    // Guardrail per #6: void -> bound is a privileged transition.
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
