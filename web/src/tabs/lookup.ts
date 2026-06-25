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
import { getConfig } from "../config/deploy-config";
import { FIELDS, REGISTRY_FIELD_KEYS, type RegistryRow, type Status } from "../registry/schema";
import { REGISTRY_CONTRACT } from "../registry/contract";
import { parseComponents, isAssembly, buildParentMap } from "../registry/assembly-graph";
import { parseMetadata } from "../registry/metadata";
import { appendEdit, appendVoid, appendBind } from "../registry/queue";
import { addMint, getSessionSync } from "../registry/session";
import { planAssembly, validateAssembly } from "../registry/assembly-create";
import type { AppContext, Tab } from "../core/types";
import { normalizeCanonicalId } from "../routing/route";
import {
  events,
  EVENT_REPRINT_REQUEST,
  type ReprintRequest,
} from "../core/events";
import { el, button, input, formRow } from "../ui/dom";
import { icon } from "../ui/icons";
import { openModal } from "../ui/components/modal";
import { openScanner, type ScanStatus } from "../ui/scanner";
import { loadPlan, savePlan } from "./print";

// All possible columns from the contract. `json`-type fields (metadata)
// are excluded — they render as a parsed Properties section in the
// detail card (#171), not as a raw-JSON table column.
const ALL_COLUMNS: { key: string; label: string }[] = FIELDS.filter(
  (f) => f.type !== "json",
).map((f) => ({
  key: f.key,
  label: f.label,
}));

// Default visible columns — at-a-glance density. Users can toggle more.
const DEFAULT_VISIBLE = new Set([
  "id", "status", "type", "vendor", "batch", "location",
]);

const COLS_KEY = "qx.lookup.columns";

function loadVisibleColumns(): Set<string> {
  try {
    const raw = localStorage.getItem(COLS_KEY);
    if (raw) {
      const arr = JSON.parse(raw) as string[];
      if (Array.isArray(arr) && arr.length > 0) return new Set(arr);
    }
  } catch { /* ignore */ }
  return new Set(DEFAULT_VISIBLE);
}

function saveVisibleColumns(cols: Set<string>): void {
  localStorage.setItem(COLS_KEY, JSON.stringify([...cols]));
}

let visibleColumns = loadVisibleColumns();

function getColumns(): { key: string; label: string }[] {
  return ALL_COLUMNS.filter((c) => visibleColumns.has(c.key));
}

// #93: sortable columns
type SortDir = "asc" | "desc" | "none";
interface SortState {
  key: string;
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

// #93: filters are multi-select — each key maps to a set of selected
// values (empty set = no filter). Status is just another such filter.
type StatusSet = Set<Status>;
type ColumnFilters = Record<FilterKey, Set<string>>;

function emptyColumnFilters(): ColumnFilters {
  return { vendor: new Set(), location: new Set(), type: new Set(), batch: new Set() };
}

// #93: read filter state from URL search params (values comma-joined)
function readFilterParams(): {
  q: string;
  status: StatusSet;
  filters: ColumnFilters;
} {
  const params = new URLSearchParams(window.location.search);
  const filters = emptyColumnFilters();
  const splitParam = (v: string | null): string[] =>
    (v ?? "").split(",").map((s) => s.trim()).filter(Boolean);
  for (const k of FILTER_KEYS) {
    filters[k] = new Set(splitParam(params.get(k)));
  }
  const STATUSES_SET = new Set<Status>(["unbound", "bound", "void"]);
  const status = new Set(
    splitParam(params.get("status")).filter((s): s is Status => STATUSES_SET.has(s as Status)),
  );
  return { q: params.get("q") ?? "", status, filters };
}

// #93: write filter state to URL without navigation
function writeFilterParams(q: string, status: StatusSet, filters: ColumnFilters): void {
  const params = new URLSearchParams();
  if (q) params.set("q", q);
  if (status.size > 0) params.set("status", [...status].join(","));
  for (const k of FILTER_KEYS) {
    if (filters[k].size > 0) params.set(k, [...filters[k]].join(","));
  }
  const qs = params.toString();
  const url = window.location.pathname + (qs ? `?${qs}` : "");
  history.replaceState(null, "", url);
}

// #93: unique non-empty values for a field across the registry
function uniqueValues(rows: RegistryRow[], key: string): string[] {
  const set = new Set<string>();
  for (const r of rows) {
    const v = (r as unknown as Record<string, string>)[key];
    if (v) set.add(v);
  }
  return [...set].sort();
}

/**
 * A multi-select filter dropdown (checkbox list behind a labelled
 * button) — the shared control for Status and the column filters.
 * Mutates `selected` directly; calls `onChange` after any toggle.
 * Returns the wrapper plus a `refresh()` that re-syncs the button label
 * and checkboxes to `selected` (e.g. after a dashboard click or Clear).
 */
function makeFilterDropdown(
  label: string,
  getOptions: () => string[],
  selected: Set<string>,
  onChange: () => void,
): { wrap: HTMLElement; refresh: () => void } {
  const wrap = el("div", { class: "lookup__filter-dd" });
  const toggle = button({ class: "outline small lookup__filter-dd-btn", type: "button" });
  const menu = el("div", { class: "lookup__filter-dd-menu" });
  menu.style.display = "none";

  const syncLabel = () => {
    toggle.textContent = "";
    toggle.append(
      `${label}`,
      selected.size > 0
        ? el("span", { class: "lookup__filter-dd-count" }, ` ${selected.size}`)
        : "",
      el("span", { class: "lookup__filter-dd-caret" }, " ▾"),
    );
    toggle.classList.toggle("lookup__filter-dd-btn--active", selected.size > 0);
  };

  const buildMenu = () => {
    menu.innerHTML = "";
    const opts = getOptions();
    if (opts.length === 0) {
      menu.append(el("p", { class: "muted small", style: "margin:4px 8px;" }, "No values"));
      return;
    }
    for (const opt of opts) {
      const row = el("label", { class: "lookup__filter-dd-opt", "data-value": opt });
      const cb = document.createElement("input");
      cb.type = "checkbox";
      cb.checked = selected.has(opt);
      cb.addEventListener("change", () => {
        if (cb.checked) selected.add(opt);
        else selected.delete(opt);
        syncLabel();
        onChange();
      });
      row.append(cb, ` ${opt}`);
      menu.append(row);
    }
  };

  toggle.addEventListener("click", (e) => {
    e.stopPropagation();
    const showing = menu.style.display !== "none";
    if (!showing) buildMenu();
    menu.style.display = showing ? "none" : "block";
  });
  document.addEventListener("click", () => { menu.style.display = "none"; });
  menu.addEventListener("click", (e) => e.stopPropagation());

  const refresh = () => {
    syncLabel();
    if (menu.style.display !== "none") buildMenu();
  };

  syncLabel();
  wrap.append(toggle, menu);
  return { wrap, refresh };
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
  const statusFilter: StatusSet = savedParams.status;
  const columnFilters: ColumnFilters = savedParams.filters;
  let sortState: SortState = { key: "id", dir: "none" };
  let viewMode: "table" | "dashboard" = "table";
  // refresh() handles for the filter dropdowns, so dashboard click-throughs
  // and Clear can re-sync the controls.
  const filterRefresh = new Map<"status" | FilterKey, () => void>();

  // ---------- toolbar ----------
  const searchInput = input({
    type: "search",
    placeholder: "Fuzzy search (id, type, vendor, batch, notes...)",
    autocomplete: "off",
    class: "lookup__search",
  });
  if (savedParams.q) searchInput.value = savedParams.q;

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

  // #176: OCR text scan — photograph a manufacturer label (or a plain-
  // printed ID) to *find* the matching registry part. Recognition lives
  // in Lookup; the result fills the search box. (Creating a new part
  // from a label is "Mint from label" in the Bind tab.)
  const scanTextBtn = button(
    { class: "icon-only", title: "Find a part by photographing its label" },
    icon("scan-text"),
  );
  scanTextBtn.addEventListener("click", async () => {
    try {
      const { openOcrScan } = await import("../ui/ocr-scan");
      const ids = await openOcrScan({
        rows: ctx.registry.all(),
        resolveStatus: (canonical): ScanStatus => {
          const row = ctx.registry.findById(canonical);
          if (!row) return "unknown";
          if (row.status === "unbound") return "unbound";
          return "bound";
        },
      });
      if (ids.length > 0) {
        // One match → jump to it; multiple → search the first.
        searchInput.value = ids[0];
        renderView();
      }
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
  const exportCsvBtn = button({ class: "outline" }, icon("download"), " Export CSV");

  // Combine selected parts into a new minted assembly. Composition only:
  // a fresh ID references the selection via `components`; the selected
  // parts are unchanged. Gated on mint + bind being enabled, since the
  // action queues a mint and a bind.
  const features = (() => {
    try {
      return getConfig().features;
    } catch {
      return undefined;
    }
  })();
  const canAssemble =
    !features ||
    (features.enableMintTab !== false && features.enableBindTab !== false);
  const assembleBtn = button(
    { class: "secondary", disabled: "true" },
    icon("plus"),
    " Combine into assembly",
  );

  // #93: unified multi-select filter bar — Status + one dropdown per
  // column key. Status is just another filter (replaces the chip row).
  const filterBar = el("div", { class: "lookup__filter-bar" });

  const statusDd = makeFilterDropdown(
    "Status",
    () => ["unbound", "bound", "void"],
    statusFilter as Set<string>,
    () => renderView(),
  );
  filterRefresh.set("status", statusDd.refresh);
  filterBar.append(statusDd.wrap);

  for (const fk of FILTER_KEYS) {
    const label = fk.charAt(0).toUpperCase() + fk.slice(1);
    const dd = makeFilterDropdown(
      label,
      () => uniqueValues(all, fk),
      columnFilters[fk],
      () => renderView(),
    );
    filterRefresh.set(fk, dd.refresh);
    filterBar.append(dd.wrap);
  }

  const clearFiltersBtn = button({ class: "outline small lookup__clear-filters" }, "Clear filters");
  clearFiltersBtn.addEventListener("click", () => {
    statusFilter.clear();
    for (const fk of FILTER_KEYS) columnFilters[fk].clear();
    searchInput.value = "";
    for (const r of filterRefresh.values()) r();
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

  // Column picker — toggle which fields are visible in the table
  const colPickerWrap = el("div", { class: "lookup__col-picker", style: "position:relative;display:inline-block;" });
  const colPickerBtn = button({ class: "outline small" }, icon("settings"), " Columns");
  const colPickerDropdown = el("div", {
    class: "col-picker-dropdown",
    style: "display:none;position:absolute;right:0;top:100%;z-index:10;background:var(--bg-elev);border:1px solid var(--border);border-radius:var(--radius);padding:8px;min-width:180px;max-height:300px;overflow-y:auto;box-shadow:0 4px 12px rgba(0,0,0,0.1);",
  });

  const buildColPicker = () => {
    colPickerDropdown.innerHTML = "";
    for (const col of ALL_COLUMNS) {
      const label = el("label", { style: "display:flex;align-items:center;gap:4px;padding:2px 0;cursor:pointer;font-size:13px;" });
      const cb = document.createElement("input");
      cb.type = "checkbox";
      cb.checked = visibleColumns.has(col.key);
      // id and status are always visible
      if (col.key === "id" || col.key === "status") {
        cb.disabled = true;
        cb.checked = true;
      }
      cb.addEventListener("change", () => {
        if (cb.checked) visibleColumns.add(col.key);
        else visibleColumns.delete(col.key);
        saveVisibleColumns(visibleColumns);
        renderView();
      });
      label.append(cb, ` ${col.label}`);
      colPickerDropdown.append(label);
    }
  };
  buildColPicker();

  colPickerBtn.addEventListener("click", (e) => {
    e.stopPropagation();
    const showing = colPickerDropdown.style.display !== "none";
    colPickerDropdown.style.display = showing ? "none" : "block";
  });
  // Close on outside click
  document.addEventListener("click", () => {
    colPickerDropdown.style.display = "none";
  });
  colPickerDropdown.addEventListener("click", (e) => e.stopPropagation());

  colPickerWrap.append(colPickerBtn, colPickerDropdown);

  root.append(
    formRow([searchInput, scanBtn, scanTextBtn]),
    filterBar,
    formRow(
      canAssemble
        ? [reprintSelBtn, assembleBtn, exportCsvBtn, viewToggle, colPickerWrap]
        : [reprintSelBtn, exportCsvBtn, viewToggle, colPickerWrap],
    ),
  );

  // ---------- containers ----------
  const selectedIds = new Set<string>();
  const contentContainer = el("div", { class: "lookup__content" });
  root.append(contentContainer);

  const detailCell = el("div", { class: "lookup__detail" });
  root.append(detailCell);

  // Fuse index
  const fuse = new Fuse(all, {
    keys: ["id", "type", "vendor", "batch", "location", "notes", "description", "part_number", "manufacturer_id"],
    threshold: 0.4,
    ignoreLocation: true,
  });

  // Track currently visible rows for CSV export and select-all.
  let visibleRows: RegistryRow[] = [];

  const updateReprintBtn = () => {
    (reprintSelBtn as HTMLButtonElement).disabled = selectedIds.size === 0;
    // Assembly needs at least two parts to combine.
    (assembleBtn as HTMLButtonElement).disabled = selectedIds.size < 2;
    // Update export button label to reflect selection
    exportCsvBtn.innerHTML = "";
    exportCsvBtn.append(
      icon("download"),
      selectedIds.size > 0
        ? ` Export ${selectedIds.size} selected`
        : " Export CSV",
    );
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
        // Tokenized AND search: every whitespace-separated word must
        // fuzzy-match at least one field (OR across fields); a row is
        // returned only if ALL words hit — possibly in different fields.
        // Fuse alone treats the whole query as one pattern against a
        // single field, so multi-word cross-field queries ("pt clean")
        // otherwise fail. Rank is preserved by the first word's score.
        const words = q.split(/\s+/).filter(Boolean);
        if (words.length <= 1) {
          rows = fuse.search(q).map((r) => r.item);
        } else {
          const idSets = words.map(
            (w) => new Set(fuse.search(w).map((r) => r.item.id)),
          );
          rows = fuse
            .search(words[0])
            .map((r) => r.item)
            .filter((item) => idSets.every((s) => s.has(item.id)));
        }
      }
    }
    // Multi-select: a row matches a filter when its value is in the
    // selected set (empty set = no constraint).
    if (statusFilter.size > 0) {
      rows = rows.filter((r) => statusFilter.has(r.status as Status));
    }
    for (const fk of FILTER_KEYS) {
      const sel = columnFilters[fk];
      if (sel.size > 0) {
        rows = rows.filter(
          (r) => sel.has((r as unknown as Record<string, string>)[fk]),
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

    for (const col of getColumns()) {
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
      if (all.length === 0) {
        // Full empty state — no parts at all
        const emptyWrap = el("tr");
        const emptyTd = el("td", { colspan: String(getColumns().length + 2) });
        const emptyState = el("div", { class: "empty-state" });
        emptyState.append(
          el("div", { class: "empty-state__icon" }, "📦"),
          el("h3", { class: "empty-state__title" }, "No parts registered yet"),
          el("p", { class: "empty-state__hint muted" }, "Generate your first IDs in the Mint tab, then bind them here."),
        );
        const mintBtn = button({ class: "primary" }, "Go to Mint");
        mintBtn.addEventListener("click", () => ctx.showTab("mint"));
        emptyState.append(mintBtn);
        emptyTd.append(emptyState);
        emptyWrap.append(emptyTd);
        tbody.append(emptyWrap);
      } else {
        const td = el("td", { colspan: String(getColumns().length + 2), class: "muted" });
        td.append("No matches.");
        tbody.append(el("tr", {}, td));
      }
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

      for (const col of getColumns()) {
        const value = row[col.key] ?? "";
        let cell: HTMLElement;
        if (col.key === "id") {
          cell = el("td", { class: "id-cell" });
          cell.append(fmtId(row.id));
          if (isAssembly(row)) {
            const count = parseComponents(row.components).length;
            cell.append(el("span", { class: "assembly-badge", title: `Assembly with ${count} component(s)` }, `[${count}]`));
          }
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
          // Format date fields as human-readable
          const fieldDef = FIELDS.find((f) => f.key === col.key);
          const display = value
            ? (fieldDef?.type === "date" && value.includes("T")
              ? new Date(value).toLocaleDateString()
              : value)
            : "";
          cell = el("td", {}, display || el("span", { class: "muted" }, "\u2014"));
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
        showDetailModal(row, ctx);
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
        statusFilter.clear();
        statusFilter.add(s);
        filterRefresh.get("status")?.();
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
            columnFilters[gk.key].clear();
            columnFilters[gk.key].add(groupVal);
            filterRefresh.get(gk.key)?.();
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
        statusFilter.clear();
        statusFilter.add(s);
        filterRefresh.get("status")?.();
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

    // Table row count indicator
    const countLabel = el(
      "div",
      { class: "lookup__table-count muted small" },
      rows.length === all.length
        ? `${rows.length} parts`
        : `Showing ${rows.length} of ${all.length} parts`,
    );
    contentContainer.append(countLabel);

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

  // Combine selected into a new assembly.
  assembleBtn.addEventListener("click", () => {
    if (selectedIds.size < 2) return;
    const componentRows = [...selectedIds]
      .map((id) => ctx.registry.findById(id))
      .filter((r): r is RegistryRow => r != null);
    showAssemblyModal(componentRows, ctx);
  });

  // Issue #94: Export filtered view as CSV download.
  exportCsvBtn.addEventListener("click", () => {
    // Export selected IDs if any are checked, otherwise all visible rows.
    const rows = selectedIds.size > 0
      ? visibleRows.filter((r) => selectedIds.has(r.id))
      : visibleRows;
    if (rows.length === 0) return;
    const keys = REGISTRY_FIELD_KEYS;
    const csvHeader = keys.join(",");
    const lines = rows.map((row) =>
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
    const suffix = selectedIds.size > 0 ? `${selectedIds.size}-selected` : "all";
    a.download = `registry-export-${suffix}-${new Date().toISOString().slice(0, 10)}.csv`;
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
    if (row) showDetailModal(row, ctx);
  }

  return root;
}

// Fields the operator can edit from the Lookup detail card.
// `status` is editable here (not in the bind form) because mid-life
// status changes ("mark void") are an edit-only operation per #6.
const EDIT_FIELD_KEYS: string[] = [
  "status",
  "type",
  "description",
  "vendor",
  "part_number",
  "location",
  "notes",
];

/** Show the detail view in a modal overlay instead of inline below the table. */
function showDetailModal(row: RegistryRow, ctx: AppContext): void {
  // Replace any existing modal.
  document.querySelector(".detail-modal-overlay")?.remove();
  openModal({
    overlayClass: "detail-modal-overlay",
    cardClass: "detail-modal",
    ariaLabel: `Part ${row.id}`,
    body: renderDetailView(row, ctx),
  });
}

/** Modal to combine selected parts into a new minted assembly. */
function showAssemblyModal(componentRows: RegistryRow[], ctx: AppContext): void {
  document.querySelector(".detail-modal-overlay")?.remove();
  openModal({
    overlayClass: "detail-modal-overlay",
    cardClass: "detail-modal",
    ariaLabel: "Combine into assembly",
    body: (close) => renderAssemblyForm(componentRows, ctx, close),
  });
}

function renderAssemblyForm(
  componentRows: RegistryRow[],
  ctx: AppContext,
  close: () => void,
): HTMLElement {
  const wrap = el("div", { class: "row-detail row-detail--assembly" });
  wrap.append(el("h3", {}, "Combine into assembly"));
  wrap.append(
    el(
      "p",
      { class: "muted small" },
      "A new part ID will be minted with the selected parts as its components. " +
        "The selected parts are unchanged and stay individually valid. " +
        "The mint and its components are queued together — submit the session to open a single PR.",
    ),
  );

  // Validate against the committed registry PLUS anything already pending
  // in this session that the registry can't see yet: reserved mint IDs
  // (so a new ID can't collide with one queued but unsubmitted), and
  // pending assembly binds (so a child already claimed by an unsubmitted
  // assembly is rejected here rather than bouncing off the data-repo CI).
  const sessionItems = getSessionSync()?.items ?? [];
  const reserved = new Set(
    sessionItems.filter((i) => i.kind === "mint").map((i) => i.id),
  );
  const pendingAssemblyRows = sessionItems
    .filter(
      (i): i is Extract<typeof i, { kind: "bind" }> =>
        i.kind === "bind" && !!i.fields.components,
    )
    .map(
      (i) =>
        ({
          id: i.id,
          status: "unbound",
          components: i.fields.components,
        }) as unknown as RegistryRow,
    );
  const rows = [...ctx.registry.all(), ...pendingAssemblyRows];
  const componentIds = componentRows.map((r) => r.id);
  const plan = planAssembly({ componentIds }, rows, reserved);
  const validation = validateAssembly(plan.assemblyId, componentIds, rows);

  // Component chips
  wrap.append(el("h4", {}, `Components (${componentRows.length})`));
  const chips = el("div", { class: "component-chips" });
  for (const r of componentRows) {
    chips.append(
      el(
        "span",
        {
          class: `component-chip component-chip--${r.status}`,
          title: `${r.type || r.description || r.id} (${r.status})`,
        },
        fmtId(r.id),
      ),
    );
  }
  wrap.append(chips);

  // Optional metadata for the new assembly.
  const form = el("form", { class: "row-detail__form" });
  const descInput = input({ type: "text", value: "", placeholder: "Description (optional)" });
  const typeInput = input({ type: "text", value: "", placeholder: "Type (optional)" });
  descInput.classList.add("row-detail__input");
  typeInput.classList.add("row-detail__input");
  const descLabel = el("label", { class: "row-detail__field" }, el("span", { class: "row-detail__label" }, "Description"));
  descLabel.append(descInput);
  const typeLabel = el("label", { class: "row-detail__field" }, el("span", { class: "row-detail__label" }, "Type"));
  typeLabel.append(typeInput);
  form.append(descLabel, typeLabel);
  wrap.append(form);

  const errMsg = el("p", { class: "row-detail__error error small" });
  if (!validation.valid) errMsg.append(validation.errors.join(" "));
  wrap.append(errMsg);

  const createBtn = button({ class: "primary", type: "button" }, icon("plus"), " Create assembly");
  if (!validation.valid) (createBtn as HTMLButtonElement).disabled = true;
  createBtn.addEventListener("click", async () => {
    if (!validation.valid) return;
    (createBtn as HTMLButtonElement).disabled = true;
    await addMint(plan.assemblyId, plan.batch, plan.notes);
    await appendBind({
      id: plan.assemblyId,
      type: typeInput.value.trim(),
      description: descInput.value.trim(),
      vendor: "",
      part_number: "",
      location: "",
      notes: "",
      components: plan.serializedComponents,
      manufacturer_id: "",
      metadata: "",
    });
    close();
    ctx.showTab("bind");
  });

  const cancelBtn = button({ type: "button" }, "Cancel");
  cancelBtn.addEventListener("click", close);
  wrap.append(formRow([createBtn, cancelBtn]));
  return wrap;
}

function renderDetailView(row: RegistryRow, ctx: AppContext): HTMLElement {
  const wrap = el("div", { class: "row-detail" });
  wrap.append(el("h3", { class: "row-detail__id" }, fmtId(row.id)));
  // metadata (#171) is rendered as its own Properties section below;
  // exclude it from the flat field list so we don't dump raw JSON.
  const AUDIT_KEYS = new Set(["minted_by", "bound_by", "last_edited_at", "last_edited_by", "metadata"]);
  const dl = el("dl");
  for (const f of FIELDS) {
    if (AUDIT_KEYS.has(f.key)) continue;
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

  // #171: Properties section \u2014 parsed type-specific metadata
  const props = parseMetadata(row.metadata);
  const propKeys = Object.keys(props);
  if (propKeys.length > 0) {
    const propSection = el("div", { class: "row-detail__properties" });
    propSection.append(el("h4", {}, "Properties"));
    const propDl = el("dl", { class: "row-detail__properties-dl" });
    const typeFields = REGISTRY_CONTRACT.typeFields?.[row.type ?? ""] ?? [];
    for (const key of propKeys) {
      const def = typeFields.find((tf) => tf.key === key);
      const label = def ? `${def.label}${def.unit ? ` (${def.unit})` : ""}` : key;
      const raw = props[key];
      const display = typeof raw === "object" ? JSON.stringify(raw) : String(raw);
      propDl.append(el("dt", {}, label));
      propDl.append(el("dd", {}, display));
    }
    propSection.append(propDl);
    wrap.append(propSection);
  }

  // #168: Components section — shown for assemblies
  const childIds = parseComponents(row.components);
  if (childIds.length > 0) {
    const compSection = el("div", { class: "row-detail__components" });
    compSection.append(el("h4", {}, `Components (${childIds.length})`));
    const compList = el("div", { class: "component-chips" });
    for (const childId of childIds) {
      const childRow = ctx.registry.findById(childId);
      const chipClass = childRow
        ? `component-chip component-chip--${childRow.status}`
        : "component-chip component-chip--unknown";
      const chipEl = el("a", {
        class: chipClass,
        href: "#",
        title: childRow
          ? `${childRow.type || childRow.description || childId} (${childRow.status})`
          : `${childId} (not in registry)`,
      }, fmtId(childId));
      chipEl.addEventListener("click", (e) => {
        e.preventDefault();
        ctx.showPart(childId);
        ctx.showTab("lookup");
      });
      compList.append(chipEl);
    }
    compSection.append(compList);
    wrap.append(compSection);
  }

  // #168: Reverse lookup — show if this part is a component of an assembly
  const parentMap = buildParentMap(ctx.registry.all());
  const parentId = parentMap.get(row.id);
  if (parentId) {
    const parentRow = ctx.registry.findById(parentId);
    const parentLink = el("a", { href: "#", class: "row-detail__parent-link" },
      `Part of: ${fmtId(parentId)}${parentRow ? ` (${parentRow.type || parentRow.description || ""})` : ""}`);
    parentLink.addEventListener("click", (e) => {
      e.preventDefault();
      ctx.showPart(parentId);
      ctx.showTab("lookup");
    });
    wrap.append(el("div", { class: "row-detail__parent" }, parentLink));
  }

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
      { class: "destructive row-detail__void" },
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

  const confirmBtn = button({ class: "destructive row-detail__void" }, "Confirm void");
  confirmBtn.addEventListener("click", async () => {
    const reason = reasonTextarea.value.trim();
    if (!reason) {
      errMsg.textContent = "A reason is required to void a part.";
      return;
    }
    await appendVoid(row.id, reason);
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
  const inputs = new Map<string, HTMLInputElement | HTMLSelectElement>();

  for (const key of EDIT_FIELD_KEYS) {
    const fieldDef = FIELDS.find((f) => f.key === key);
    const label = fieldDef?.label ?? key;
    const value = (row as unknown as Record<string, string>)[key] ?? "";

    const labelEl = el("label", { class: "row-detail__field" });
    labelEl.append(el("span", { class: "row-detail__label" }, label));

    let field: HTMLInputElement | HTMLSelectElement;
    if (fieldDef && fieldDef.type === "dropdown" && fieldDef.options) {
      if (fieldDef.on_unknown === "warn") {
        // Allow free text with datalist suggestions.
        field = input({ type: "text", value });
        const listId = `dl-edit-${key}`;
        const datalist = document.createElement("datalist");
        datalist.id = listId;
        for (const opt of fieldDef.options) {
          const o = document.createElement("option");
          o.value = opt;
          datalist.append(o);
        }
        field.setAttribute("list", listId);
        // Attach datalist after field is in DOM.
        requestAnimationFrame(() => {
          if (field.parentElement && !field.parentElement.querySelector(`#${listId}`)) {
            field.parentElement.append(datalist);
          }
        });
      } else {
        const select = document.createElement("select");
        for (const opt of fieldDef.options) {
          const o = document.createElement("option");
          o.value = opt;
          o.textContent = opt;
          if (opt === value) o.selected = true;
          select.append(o);
        }
        // Show current value even if not in options.
        if (value && !fieldDef.options.includes(value)) {
          const o = document.createElement("option");
          o.value = value;
          o.textContent = value;
          o.selected = true;
          select.append(o);
        }
        field = select;
      }
    } else if (fieldDef && fieldDef.type === "date") {
      field = input({ type: "date", value: value.slice(0, 10) });
    } else if (fieldDef && fieldDef.type === "number") {
      field = input({ type: "number", value });
      if (fieldDef.validation?.min != null) field.min = String(fieldDef.validation.min);
      if (fieldDef.validation?.max != null) field.max = String(fieldDef.validation.max);
    } else if (fieldDef && fieldDef.type === "yes-no") {
      field = document.createElement("input");
      field.type = "checkbox";
      (field as HTMLInputElement).checked = value === "true" || value === "yes" || value === "1";
      field.value = (field as HTMLInputElement).checked ? "yes" : "no";
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
  saveBtn.addEventListener("click", async () => {
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
    await appendEdit(row.id, before, changes);
    ctx.showTab("bind");
  });

  const cancelBtn = button({ type: "button" }, "Cancel");
  cancelBtn.addEventListener("click", () => {
    wrap.replaceWith(renderDetailView(row, ctx));
  });

  wrap.append(formRow([saveBtn, cancelBtn]));
  return wrap;
}
