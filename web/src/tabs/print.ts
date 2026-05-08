// Print tab — job-composer (issue #11 MVP).
//
// A print job is a list of items, each item a tuple
// `(id, layoutId, size, copies, extras)`. The user composes the list
// from any combination of:
//   - scanning a QR (one item)
//   - typing an ID and selecting layout/size (one item)
//   - bulk-adding from a registry batch + chosen layout (N items)
//   - hand-off from the Lookup tab's "Reprint" button (pre-fills one item)
//
// The plan persists in localStorage so the operator doesn't lose work
// across reloads. Print iterates the full plan as one page-per-label
// document so the printer auto-cuts between (the QL-820NWBc default).
//
// Output modes other than "DK continuous + auto-cut" — A4 sheet,
// strip-with-crop-marks, die-cut alignment — are explicit follow-ups
// (#7, #11 stretch goals).

import { DEFAULT_SIZE_MM, TAPE_SIZES } from "../config";
import type { AppContext, LayoutOptions, Tab } from "../core/types";
import { allLayouts, getLayout } from "../layouts";
import {
  events,
  EVENT_REPRINT_REQUEST,
  type ReprintRequest,
} from "../core/events";
import {
  el,
  button,
  input,
  select,
  formRow,
  number as numberInput,
} from "../ui/dom";
import { icon } from "../ui/icons";
import { openScanner } from "../ui/scanner";

interface JobItem {
  id: string;
  layoutId: string;
  size: number;
  copies: number;
  extras: Record<string, number>;
}

const PLAN_KEY = "part-registry.print-plan";

function loadPlan(): JobItem[] {
  try {
    const raw = localStorage.getItem(PLAN_KEY);
    if (!raw) return [];
    return JSON.parse(raw) as JobItem[];
  } catch {
    return [];
  }
}

function savePlan(plan: JobItem[]): void {
  localStorage.setItem(PLAN_KEY, JSON.stringify(plan));
}

// Cross-tab handoff: Lookup's "Reprint" emits ReprintRequest.
// We accept default layout/size for the pre-fill and let the user
// adjust before printing.
let pendingReprint: string[] = [];
events.on<ReprintRequest>(EVENT_REPRINT_REQUEST, (req) => {
  pendingReprint = [...req.ids];
});

export const printTab: Tab = {
  id: "print",
  label: "Print",
  mount(container, ctx) {
    container.innerHTML = "";
    container.append(buildUI(ctx));
  },
};

function buildUI(ctx: AppContext): HTMLElement {
  const root = el("div", { class: "tab tab--print" });
  root.append(el("h2", {}, "Print"));
  root.append(
    el(
      "p",
      { class: "muted" },
      "Compose a print job: add (ID × layout × size × copies) rows. The printer auto-cuts between pages on continuous DK tape.",
    ),
  );

  // Pre-fill any pending reprint as one row at default layout/size.
  if (pendingReprint.length > 0) {
    const plan = loadPlan();
    for (const id of pendingReprint) {
      plan.push({
        id,
        layoutId: "horz",
        size: DEFAULT_SIZE_MM,
        copies: 1,
        extras: {},
      });
    }
    savePlan(plan);
    pendingReprint = [];
  }

  const summary = el("div", { class: "muted small" });
  const tableWrap = el("div");
  const previewArea = el("div", { class: "label-preview" });

  const renderPlan = () => {
    const plan = loadPlan();
    summary.textContent = planSummary(plan);
    tableWrap.innerHTML = "";
    tableWrap.append(renderTable(ctx, plan, () => renderPlan()));
  };
  renderPlan();

  // Bulk-add from a registry batch.
  const bulkBtn = button({}, icon("plus"), " Bulk add from batch…");
  bulkBtn.addEventListener("click", () => {
    const wrap = el("div", { class: "bulk-add" });
    const batchSel = select([
      { value: "", label: "— pick batch —" },
      ...ctx.registry.batches().map((b) => ({ value: b, label: b })),
    ]);
    const layoutSel = select(
      allLayouts().map((l) => ({ value: l.id, label: l.label })),
    );
    layoutSel.value = "horz";
    const tapeSel = makeTapeSelect();
    const sizeIn = numberInput({ value: DEFAULT_SIZE_MM, min: 4, max: 100, step: 0.5 });
    tapeSel.addEventListener("change", () => {
      if (tapeSel.value) sizeIn.value = String(TAPE_SIZES[tapeSel.value]);
    });
    const copiesIn = numberInput({ value: 1, min: 1, max: 100, step: 1 });
    const cableOdIn = numberInput({ value: 6, min: 1, max: 50, step: 0.5 });
    const cableOdLabel = el("label", { class: "muted small" }, "Cable OD (mm)");
    const cableOdRow = formRow([cableOdLabel, cableOdIn]);
    const updateExtras = () => {
      const layout = getLayout(layoutSel.value);
      const showCableOd = layout?.optionFields?.().some((f) => f.key === "cableOd") ?? false;
      cableOdRow.style.display = showCableOd ? "" : "none";
    };
    layoutSel.addEventListener("change", updateExtras);
    updateExtras();

    const confirm = button({ class: "primary" }, icon("plus"), " Add to plan");
    const cancel = button({}, icon("x"), " Cancel");
    cancel.addEventListener("click", () => wrap.remove());
    confirm.addEventListener("click", () => {
      if (!batchSel.value) {
        alert("Pick a batch.");
        return;
      }
      const rows = ctx.registry.find({ batch: batchSel.value });
      if (rows.length === 0) {
        alert("Empty batch.");
        return;
      }
      const layout = getLayout(layoutSel.value);
      const extras: Record<string, number> = {};
      if (layout?.optionFields?.().some((f) => f.key === "cableOd")) {
        extras.cableOd = parseFloat(cableOdIn.value);
      }
      const plan = loadPlan();
      for (const r of rows) {
        plan.push({
          id: r.id,
          layoutId: layoutSel.value,
          size: parseFloat(sizeIn.value),
          copies: parseInt(copiesIn.value, 10),
          extras,
        });
      }
      savePlan(plan);
      wrap.remove();
      renderPlan();
    });

    wrap.append(
      el("h3", {}, `Bulk add from batch`),
      formRow([el("label", {}, "Batch"), batchSel]),
      formRow([el("label", {}, "Layout"), layoutSel]),
      formRow([el("label", {}, "Tape"), tapeSel, el("label", {}, "Size (mm)"), sizeIn]),
      cableOdRow,
      formRow([el("label", {}, "Copies / ID"), copiesIn]),
      formRow([confirm, cancel]),
    );
    root.insertBefore(wrap, tableWrap);
  });

  const clearBtn = button({}, icon("trash"), " Clear plan");
  clearBtn.addEventListener("click", () => {
    if (loadPlan().length === 0) return;
    if (!confirm("Clear the print plan?")) return;
    savePlan([]);
    renderPlan();
  });

  const previewBtn = button({}, icon("search"), " Preview");
  const printBtn = button({ class: "primary" }, icon("printer"), " Print");

  previewBtn.addEventListener("click", () => {
    previewArea.innerHTML = "";
    const plan = loadPlan();
    if (plan.length === 0) {
      previewArea.append(el("p", { class: "muted" }, "Plan is empty."));
      return;
    }
    const sample = plan.slice(0, 8);
    for (const item of sample) {
      const layout = getLayout(item.layoutId);
      if (!layout) continue;
      const wrap = el("div", { class: "label-preview__item" });
      wrap.innerHTML = layout.renderSvg(item.id, jobItemToOpts(item));
      wrap.append(
        el(
          "div",
          { class: "muted small" },
          `${item.id} · ${item.layoutId} · ${item.size}mm × ${item.copies}`,
        ),
      );
      previewArea.append(wrap);
    }
    if (plan.length > sample.length) {
      previewArea.append(
        el(
          "div",
          { class: "muted small" },
          `… ${plan.length - sample.length} more (printed in full).`,
        ),
      );
    }
  });

  printBtn.addEventListener("click", () => {
    const plan = loadPlan();
    if (plan.length === 0) {
      alert("Plan is empty.");
      return;
    }
    openPrintWindow(plan);
  });

  root.append(
    formRow([bulkBtn, clearBtn]),
    summary,
    tableWrap,
    formRow([previewBtn, printBtn]),
    previewArea,
  );
  return root;
}

function planSummary(plan: JobItem[]): string {
  const totalLabels = plan.reduce((acc, it) => acc + it.copies, 0);
  if (plan.length === 0) return "Plan is empty.";
  return `${plan.length} item(s) · ${totalLabels} label(s) total.`;
}

function jobItemToOpts(item: JobItem): LayoutOptions {
  return { size: item.size, extra: { ...item.extras } };
}

function makeTapeSelect(): HTMLSelectElement {
  return select([
    { value: "", label: "— custom mm —" },
    ...Object.keys(TAPE_SIZES).map((k) => ({ value: k, label: k })),
  ]);
}

function renderTable(
  ctx: AppContext,
  plan: JobItem[],
  onChange: () => void,
): HTMLElement {
  const table = el("table", { class: "data" });
  const thead = el("thead");
  const tr = el("tr");
  for (const h of ["ID", "Layout", "Size", "Extras", "Copies", ""]) {
    tr.append(el("th", {}, h));
  }
  thead.append(tr);
  table.append(thead);

  const tbody = el("tbody");
  for (let i = 0; i < plan.length; i++) {
    tbody.append(renderJobRow(plan[i], i, onChange));
  }
  tbody.append(renderEntryRow(ctx, onChange));
  table.append(tbody);
  return table;
}

function renderJobRow(item: JobItem, index: number, onChange: () => void): HTMLElement {
  const tr = el("tr");

  const idCell = el("td", { class: "id-cell" }, fmtId(item.id));
  tr.append(idCell);

  const layoutSel = select(
    allLayouts().map((l) => ({ value: l.id, label: l.label })),
  );
  layoutSel.value = item.layoutId;
  const layoutCell = el("td");
  layoutCell.append(layoutSel);
  tr.append(layoutCell);

  const sizeIn = numberInput({ value: item.size, min: 4, max: 100, step: 0.5 });
  tr.append(el("td", {}, sizeIn));

  // Extras cell: cableOd input visible only when layout is flag.
  const extrasCell = el("td");
  const cableOdIn = numberInput({
    value: item.extras.cableOd ?? 6,
    min: 1,
    max: 50,
    step: 0.5,
  });
  cableOdIn.title = "Cable OD (mm)";
  const updateExtras = () => {
    const layout = getLayout(layoutSel.value);
    const wantCableOd = layout?.optionFields?.().some((f) => f.key === "cableOd") ?? false;
    extrasCell.innerHTML = "";
    if (wantCableOd) extrasCell.append(cableOdIn);
  };
  updateExtras();
  tr.append(extrasCell);

  const copiesIn = numberInput({ value: item.copies, min: 1, max: 100, step: 1 });
  tr.append(el("td", {}, copiesIn));

  const trashBtn = button({ class: "icon-only", title: "Remove" }, icon("trash"));
  trashBtn.addEventListener("click", () => {
    const plan = loadPlan();
    plan.splice(index, 1);
    savePlan(plan);
    onChange();
  });
  tr.append(el("td", { class: "row-actions" }, trashBtn));

  // Persist any field change.
  const persist = () => {
    const plan = loadPlan();
    const target = plan[index];
    if (!target) return;
    target.layoutId = layoutSel.value;
    target.size = parseFloat(sizeIn.value) || target.size;
    target.copies = Math.max(1, parseInt(copiesIn.value, 10) || target.copies);
    const layout = getLayout(target.layoutId);
    const wantCableOd = layout?.optionFields?.().some((f) => f.key === "cableOd") ?? false;
    target.extras = wantCableOd ? { cableOd: parseFloat(cableOdIn.value) || 6 } : {};
    savePlan(plan);
  };
  layoutSel.addEventListener("change", () => {
    persist();
    updateExtras();
  });
  for (const inp of [sizeIn, copiesIn, cableOdIn]) {
    inp.addEventListener("change", persist);
  }

  return tr;
}

function renderEntryRow(ctx: AppContext, onAdd: () => void): HTMLElement {
  const tr = el("tr", { class: "entry-row" });
  const idIn = input({
    type: "text",
    placeholder: "12-char ID",
    autocapitalize: "characters",
  });
  const scanBtn = button({ class: "icon-only", title: "Scan QR" }, icon("camera"));
  scanBtn.addEventListener("click", async () => {
    try {
      const v = await openScanner();
      idIn.value = v.toUpperCase().replace(/-/g, "");
      idIn.focus();
    } catch {
      /* cancelled */
    }
  });
  const idWrap = el("div", { style: "display:flex; gap:4px;" });
  idWrap.append(idIn, scanBtn);
  tr.append(el("td", { class: "id-cell" }, idWrap));

  const layoutSel = select(
    allLayouts().map((l) => ({ value: l.id, label: l.label })),
  );
  layoutSel.value = "horz";
  tr.append(el("td", {}, layoutSel));

  const sizeIn = numberInput({ value: DEFAULT_SIZE_MM, min: 4, max: 100, step: 0.5 });
  tr.append(el("td", {}, sizeIn));

  const cableOdIn = numberInput({ value: 6, min: 1, max: 50, step: 0.5 });
  cableOdIn.title = "Cable OD (mm)";
  const extrasCell = el("td");
  const updateExtras = () => {
    const layout = getLayout(layoutSel.value);
    const wantCableOd = layout?.optionFields?.().some((f) => f.key === "cableOd") ?? false;
    extrasCell.innerHTML = "";
    if (wantCableOd) extrasCell.append(cableOdIn);
  };
  layoutSel.addEventListener("change", updateExtras);
  updateExtras();
  tr.append(extrasCell);

  const copiesIn = numberInput({ value: 1, min: 1, max: 100, step: 1 });
  tr.append(el("td", {}, copiesIn));

  const addBtn = button({ class: "icon-only primary", title: "Add to plan" }, icon("plus"));
  addBtn.addEventListener("click", () => {
    const id = idIn.value.trim().toUpperCase().replace(/-/g, "");
    if (id.length !== 12) {
      alert("ID must be 12 characters.");
      return;
    }
    const layout = getLayout(layoutSel.value);
    const wantCableOd = layout?.optionFields?.().some((f) => f.key === "cableOd") ?? false;
    const plan = loadPlan();
    plan.push({
      id,
      layoutId: layoutSel.value,
      size: parseFloat(sizeIn.value),
      copies: parseInt(copiesIn.value, 10),
      extras: wantCableOd ? { cableOd: parseFloat(cableOdIn.value) } : {},
    });
    savePlan(plan);
    idIn.value = "";
    onAdd();
  });
  tr.append(el("td", { class: "row-actions" }, addBtn));

  // Existence-check: warn if the entered ID isn't in the registry. Doesn't
  // block — operator may be adding an ID that hasn't synced yet.
  idIn.addEventListener("blur", () => {
    const id = idIn.value.trim().toUpperCase().replace(/-/g, "");
    if (id.length !== 12) return;
    if (!ctx.registry.findById(id)) {
      idIn.title = `${id} is not in the loaded registry.`;
      idIn.style.borderColor = "var(--warn)";
    } else {
      idIn.title = "";
      idIn.style.borderColor = "";
    }
  });

  return tr;
}

function fmtId(id: string): string {
  if (id.length !== 12) return id;
  return `${id.slice(0, 4)}-${id.slice(4, 8)}-${id.slice(8, 12)}`;
}

// Open a print-only window with one @page per label so the printer
// auto-cuts between. Each plan item expands into `copies` pages, each
// sized to that item's layout × size.
function openPrintWindow(plan: JobItem[]): void {
  // Expand into a flat list of {svg, w, h} to preserve mixed-page-size
  // behavior — each label gets its own @page rule via :nth-of-type CSS.
  interface Page {
    svg: string;
    widthMm: number;
    heightMm: number;
  }
  const pages: Page[] = [];
  for (const item of plan) {
    const layout = getLayout(item.layoutId);
    if (!layout) continue;
    const opts = jobItemToOpts(item);
    const dim = layout.measure(opts);
    const svg = layout.renderSvg(item.id, opts);
    for (let i = 0; i < item.copies; i++) {
      pages.push({ svg, widthMm: dim.widthMm, heightMm: dim.heightMm });
    }
  }
  if (pages.length === 0) {
    alert("Nothing to print.");
    return;
  }

  // CSS: per-page @page sizing. Browsers don't all honor per-element
  // @page size, so we generate one stylesheet per *unique* dimension
  // and group labels with the same dimension into separate sections —
  // each section gets one @page rule. Auto-cut still happens between
  // every page break.
  const dimsKey = (p: Page) => `${p.widthMm.toFixed(3)}x${p.heightMm.toFixed(3)}`;
  const groups = new Map<string, Page[]>();
  for (const p of pages) {
    const k = dimsKey(p);
    if (!groups.has(k)) groups.set(k, []);
    groups.get(k)!.push(p);
  }

  // Build sections; each section has its own @page-named class with a
  // @page-at-class rule.
  const styleParts: string[] = [
    "html, body { margin: 0; padding: 0; }",
    `.label { page-break-after: always; break-after: page; overflow: hidden; }`,
    `.label:last-child { page-break-after: auto; break-after: auto; }`,
    `svg { display: block; }`,
  ];
  const sections: string[] = [];
  let i = 0;
  for (const [key, items] of groups) {
    const className = `pg${i++}`;
    const w = items[0].widthMm.toFixed(3);
    const h = items[0].heightMm.toFixed(3);
    styleParts.push(
      `@page ${className} { size: ${w}mm ${h}mm; margin: 0; }`,
      `.${className} { page: ${className}; width: ${w}mm; height: ${h}mm; }`,
    );
    void key;
    sections.push(
      items
        .map((p) => `<div class="label ${className}">${p.svg}</div>`)
        .join("\n"),
    );
  }

  const html = `<!doctype html>
<html><head><meta charset="utf-8"><title>Print labels</title>
<style>${styleParts.join("\n")}</style>
</head>
<body onload="window.print(); setTimeout(() => window.close(), 500);">
${sections.join("\n")}
</body></html>`;

  const w = window.open("", "_blank", "width=400,height=600");
  if (!w) {
    alert("Pop-up blocked — allow pop-ups for this site to print.");
    return;
  }
  w.document.open();
  w.document.write(html);
  w.document.close();
}

