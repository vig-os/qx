// Print tab — job-composer (issue #11 MVP) + paper-format selector.
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
// across reloads. The selected output mode (paper format) decides how
// the plan is turned into pages — see src/output/.
//
// The default mode is `dk-continuous` (one page per label, printer
// auto-cuts). The DK-1201 die-cut mode packs a configurable rows × cols
// grid onto each 25 × 80 mm die-cut sheet.

import { DEFAULT_SIZE_MM, TAPE_SIZES } from "../config";
import type {
  AppContext,
  OutputMode,
  OutputModeField,
  PlanItem,
  Tab,
} from "../core/types";
import { allLayouts, getLayout } from "../layouts";
import { allOutputModes, getOutputMode } from "../output";
import {
  events,
  EVENT_REPRINT_REQUEST,
  EVENT_PLAN_CHANGED,
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
import { isWasmReady, renderLabelSync } from "../wasm/loader";

export interface JobItem {
  id: string;
  layoutId: string;
  size: number;
  copies: number;
  extras: Record<string, number>;
}

const PLAN_KEY = "part-registry.print-plan";
const MODE_KEY = "part-registry.print-output-mode";
const MODE_OPTS_KEY = "part-registry.print-output-mode-opts";
const DEFAULT_MODE_ID = "dk-continuous";

/** Brother QL-820NWBc prints at 300 DPI (dots per inch). */
export const PRINTER_DPI = 300;
export const MM_PER_INCH = 25.4;
/** 1 printer pixel = MM_PER_INCH / PRINTER_DPI mm. */
export const PX_TO_MM = MM_PER_INCH / PRINTER_DPI;

export function loadPlan(): JobItem[] {
  try {
    const raw = localStorage.getItem(PLAN_KEY);
    if (!raw) return [];
    return JSON.parse(raw) as JobItem[];
  } catch {
    return [];
  }
}

export function savePlan(plan: JobItem[]): void {
  localStorage.setItem(PLAN_KEY, JSON.stringify(plan));
  events.emit(EVENT_PLAN_CHANGED, { count: plan.length });
}

function loadModeId(): string {
  return localStorage.getItem(MODE_KEY) || DEFAULT_MODE_ID;
}

function saveModeId(id: string): void {
  localStorage.setItem(MODE_KEY, id);
}

type ModeOpts = Record<string, number | string>;

function loadModeOpts(modeId: string): ModeOpts {
  try {
    const raw = localStorage.getItem(`${MODE_OPTS_KEY}:${modeId}`);
    return raw ? (JSON.parse(raw) as ModeOpts) : {};
  } catch {
    return {};
  }
}

function saveModeOpts(modeId: string, opts: ModeOpts): void {
  localStorage.setItem(`${MODE_OPTS_KEY}:${modeId}`, JSON.stringify(opts));
}

function defaultsFor(mode: OutputMode): ModeOpts {
  const out: ModeOpts = {};
  for (const f of mode.optionFields()) {
    out[f.key] = f.default;
  }
  return out;
}

function jobItemToPlanItem(item: JobItem): PlanItem {
  return {
    id: item.id,
    layoutId: item.layoutId,
    size: item.size,
    copies: item.copies,
    extras: { ...item.extras },
  };
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
      "Compose a print job: add (ID × layout × size × copies) rows. Pick a paper format below — DK continuous auto-cuts between labels; DK-1201 die-cut packs a grid onto each die-cut.",
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
  const livePreviewArea = el("div", { class: "label-preview label-preview--live" });
  const previewArea = el("div", { class: "label-preview" });
  const modeOptsArea = el("div", { class: "form-row" });
  const planSummaryEl = el("div", { class: "muted small" });

  // Issue #87: debounced live SVG preview of the first plan item.
  let livePreviewTimer: ReturnType<typeof setTimeout> | undefined;
  const refreshLivePreview = () => {
    clearTimeout(livePreviewTimer);
    livePreviewTimer = setTimeout(() => {
      requestAnimationFrame(() => {
        livePreviewArea.innerHTML = "";
        const plan = loadPlan();
        if (plan.length === 0) return;
        const first = plan[0];
        if (!isWasmReady()) {
          livePreviewArea.append(
            el("p", { class: "muted small" }, "Loading preview..."),
          );
          return;
        }
        try {
          const layout = getLayout(first.layoutId);
          if (!layout) return;
          const wrap = el("div", { class: "label-preview__item" });
          wrap.innerHTML = layout.renderSvg(first.id, {
            size: first.size,
            extra: { ...first.extras },
          });
          wrap.append(
            el(
              "div",
              { class: "muted small" },
              `Live preview: ${fmtId(first.id)} \u00b7 ${first.layoutId} \u00b7 ${first.size}mm`,
            ),
          );
          livePreviewArea.append(wrap);
        } catch {
          livePreviewArea.append(
            el("p", { class: "muted small" }, "Preview unavailable."),
          );
        }
      });
    }, 200);
  };

  // Output mode state.
  let activeModeId = loadModeId();
  if (!getOutputMode(activeModeId)) activeModeId = DEFAULT_MODE_ID;
  let activeModeOpts: ModeOpts = {
    ...defaultsFor(getOutputMode(activeModeId)!),
    ...loadModeOpts(activeModeId),
  };

  const modeSel = select(
    allOutputModes().map((m) => ({ value: m.id, label: m.label })),
  );
  modeSel.value = activeModeId;

  const renderModeOpts = () => {
    const mode = getOutputMode(activeModeId);
    modeOptsArea.innerHTML = "";
    if (!mode) return;
    const fields = mode.optionFields();
    if (fields.length === 0) {
      modeOptsArea.append(
        el("span", { class: "muted small" }, mode.description),
      );
      return;
    }
    for (const f of fields) {
      modeOptsArea.append(buildModeOptField(f, activeModeOpts, () => {
        saveModeOpts(activeModeId, activeModeOpts);
        refreshPreview();
        refreshPlanSummary();
      }));
    }
  };

  const refreshPlanSummary = () => {
    const plan = loadPlan();
    const mode = getOutputMode(activeModeId);
    if (!mode) {
      planSummaryEl.textContent = "";
      return;
    }
    const planItems = plan.map(jobItemToPlanItem);
    let pageCount = 0;
    let labelCount = 0;
    try {
      const pages = mode.plan(planItems, activeModeOpts);
      pageCount = pages.length;
      labelCount = pages.reduce((acc, p) => acc + (p.labelCount ?? 1), 0);
    } catch {
      // Mode might throw on invalid opts — leave summary blank.
    }
    if (plan.length === 0) {
      planSummaryEl.textContent = "";
      return;
    }
    if (mode.id === "dk-1201-diecut") {
      const rows = Math.max(1, Math.floor(toNum(activeModeOpts.rows, 2)));
      const cols = Math.max(1, Math.floor(toNum(activeModeOpts.cols, 4)));
      planSummaryEl.textContent =
        `Output: ${pageCount} die-cut sheet(s) of ${rows}×${cols} (${labelCount} label(s) total).`;
    } else {
      planSummaryEl.textContent = `Output: ${pageCount} page(s).`;
    }
  };

  const refreshPreview = () => {
    previewArea.innerHTML = "";
    const plan = loadPlan();
    if (plan.length === 0) {
      previewArea.append(el("p", { class: "muted" }, "Plan is empty."));
      return;
    }
    const mode = getOutputMode(activeModeId);
    if (!mode) return;
    if (mode.id === "dk-1201-diecut") {
      previewArea.append(buildDiecutPreview(plan, activeModeOpts));
    } else {
      // Continuous: show a sample of label SVGs (existing behavior).
      const sample = plan.slice(0, 8);
      for (const item of sample) {
        const layout = getLayout(item.layoutId);
        if (!layout) continue;
        const wrap = el("div", { class: "label-preview__item" });
        wrap.innerHTML = layout.renderSvg(item.id, {
          size: item.size,
          extra: { ...item.extras },
        });
        wrap.append(
          el(
            "div",
            { class: "muted small" },
            `${fmtId(item.id)} · ${item.layoutId} · ${item.size}mm × ${item.copies}`,
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
    }
  };

  modeSel.addEventListener("change", () => {
    activeModeId = modeSel.value;
    saveModeId(activeModeId);
    const mode = getOutputMode(activeModeId);
    if (!mode) return;
    activeModeOpts = { ...defaultsFor(mode), ...loadModeOpts(activeModeId) };
    renderModeOpts();
    refreshPreview();
    refreshPlanSummary();
  });

  const renderPlan = () => {
    const plan = loadPlan();
    summary.textContent = planSummary(plan);
    tableWrap.innerHTML = "";
    tableWrap.append(renderTable(ctx, plan, () => {
      renderPlan();
      refreshPlanSummary();
    }));
    refreshPlanSummary();
    refreshLivePreview();
  };
  renderPlan();
  renderModeOpts();

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
    const bulkUnitSel = select([
      { value: "mm", label: "mm" },
      { value: "px", label: "px" },
    ]);
    bulkUnitSel.value = "mm";
    const bulkSizeHint = el("span", { class: "muted small size-hint" });
    const updateBulkSizeHint = () => {
      if (bulkUnitSel.value === "px") {
        const px = parseFloat(sizeIn.value) || 0;
        bulkSizeHint.textContent = `= ${(px * PX_TO_MM).toFixed(2)} mm`;
      } else {
        bulkSizeHint.textContent = "";
      }
    };
    bulkUnitSel.addEventListener("change", updateBulkSizeHint);
    sizeIn.addEventListener("input", updateBulkSizeHint);
    tapeSel.addEventListener("change", () => {
      if (tapeSel.value) {
        sizeIn.value = String(TAPE_SIZES[tapeSel.value]);
        bulkUnitSel.value = "mm";
        updateBulkSizeHint();
      }
    });
    const copiesIn = numberInput({ value: 1, min: 1, max: 100, step: 1 });
    const bulkExtrasArea = el("div");
    const bulkExtraInputs: Record<string, HTMLInputElement> = {};
    const updateBulkExtras = () => {
      const layout = getLayout(layoutSel.value);
      const fields = layout?.optionFields?.() ?? [];
      bulkExtrasArea.innerHTML = "";
      for (const f of fields) {
        if (f.type === "checkbox") {
          const cb = document.createElement("input");
          cb.type = "checkbox";
          cb.checked = false;
          const lbl = el("label", { class: "muted small" });
          lbl.append(cb, " " + f.label);
          bulkExtrasArea.append(formRow([lbl]));
          bulkExtraInputs[f.key] = cb;
        } else {
          const inp = numberInput({
            value: f.default as number,
            min: f.min,
            max: f.max,
            step: f.step,
          });
          const lbl = el("label", { class: "muted small" }, f.label);
          bulkExtrasArea.append(formRow([lbl, inp]));
          bulkExtraInputs[f.key] = inp;
        }
      }
    };
    layoutSel.addEventListener("change", updateBulkExtras);
    updateBulkExtras();

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
      if (!window.confirm(`Add ${rows.length} labels from batch '${batchSel.value}' to the plan?`)) {
        return;
      }
      const layout = getLayout(layoutSel.value);
      const fields = layout?.optionFields?.() ?? [];
      const extras: Record<string, number> = {};
      for (const f of fields) {
        const inp = bulkExtraInputs[f.key];
        if (!inp) continue;
        if (f.type === "checkbox") {
          extras[f.key] = inp.checked ? 1 : 0;
        } else {
          extras[f.key] = parseFloat(inp.value) || (f.default as number);
        }
      }
      const rawSize = parseFloat(sizeIn.value);
      const sizeMm = bulkUnitSel.value === "px" ? rawSize * PX_TO_MM : rawSize;
      const plan = loadPlan();
      for (const r of rows) {
        plan.push({
          id: r.id,
          layoutId: layoutSel.value,
          size: sizeMm,
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
      formRow([el("label", {}, "Tape"), tapeSel, el("label", {}, "Size"), sizeIn, bulkUnitSel, bulkSizeHint]),
      bulkExtrasArea,
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

  previewBtn.addEventListener("click", refreshPreview);

  printBtn.addEventListener("click", () => {
    const plan = loadPlan();
    if (plan.length === 0) {
      alert("Plan is empty.");
      return;
    }
    const mode = getOutputMode(activeModeId);
    if (!mode) {
      alert("No output mode selected.");
      return;
    }
    const pages = mode.plan(plan.map(jobItemToPlanItem), activeModeOpts);
    if (pages.length === 0) {
      alert("Nothing to print.");
      return;
    }
    openPrintWindow(mode.renderPrintHtml(pages));
  });

  root.append(
    formRow([bulkBtn, clearBtn]),
    summary,
    tableWrap,
    livePreviewArea,
    el("h3", {}, "Paper format"),
    formRow([el("label", {}, "Output"), modeSel]),
    modeOptsArea,
    planSummaryEl,
    formRow([previewBtn, printBtn]),
    previewArea,
  );
  return root;
}

function buildModeOptField(
  field: OutputModeField,
  opts: ModeOpts,
  onChange: () => void,
): HTMLElement {
  const wrap = el("label", { class: "form-row__inline" });
  wrap.append(el("span", { class: "muted small" }, field.label));
  if (field.type === "select") {
    const sel = select(field.options ?? []);
    sel.value = String(opts[field.key] ?? field.default);
    sel.addEventListener("change", () => {
      opts[field.key] = sel.value;
      onChange();
    });
    wrap.append(sel);
  } else {
    const inp = numberInput({
      value: Number(opts[field.key] ?? field.default),
      min: field.min,
      max: field.max,
      step: field.step,
    });
    inp.addEventListener("change", () => {
      const n = parseFloat(inp.value);
      opts[field.key] = Number.isFinite(n) ? n : Number(field.default);
      onChange();
    });
    wrap.append(inp);
  }
  if (field.hint) {
    wrap.append(el("span", { class: "muted small" }, " " + field.hint));
  }
  return wrap;
}

function toNum(v: number | string | undefined, fallback: number): number {
  if (typeof v === "number") return v;
  if (typeof v === "string") {
    const n = parseFloat(v);
    if (!Number.isNaN(n)) return n;
  }
  return fallback;
}

// DK-1201 preview: render the 25×80 printable area at a fixed pixel
// scale, with cells outlined and labels placed per the active opts.
// Shows the *first* die-cut sheet only — the plan summary tells the
// user how many sheets total.
function buildDiecutPreview(
  plan: JobItem[],
  opts: ModeOpts,
): HTMLElement {
  const wrap = el("div", { class: "diecut-preview" });
  const mode = getOutputMode("dk-1201-diecut");
  if (!mode) return wrap;
  const planItems = plan.map(jobItemToPlanItem);
  const pages = mode.plan(planItems, opts);
  if (pages.length === 0) {
    wrap.append(el("p", { class: "muted" }, "Plan is empty."));
    return wrap;
  }

  // Render all sheets (for multi-sheet preview), capped to 4 to keep
  // the UI light. Each sheet is a 25 × 80 mm box scaled to ~3 px / mm.
  const PX_PER_MM = 3;
  const PRINTABLE_W = 25;
  const PRINTABLE_H = 80;
  const cap = 4;
  const shown = pages.slice(0, cap);

  const sheetsRow = el("div", { class: "diecut-preview__sheets" });
  for (let i = 0; i < shown.length; i++) {
    const p = shown[i];
    const sheet = el("div", {
      class: "diecut-preview__sheet",
      style:
        `position:relative;width:${PRINTABLE_W * PX_PER_MM}px;` +
        `height:${PRINTABLE_H * PX_PER_MM}px;` +
        `border:1px solid #888;background:#fff;` +
        `margin-right:12px;` +
        // Scale the mm-positioned children: the bodyHtml uses absolute
        // mm coordinates; we set CSS so 1mm = PX_PER_MM px in this box.
        `font-size:${PX_PER_MM}px;`,
    });
    // Rebuild the inner positioned content but with px instead of mm
    // by wrapping in a transform-scaled div (simplest: use a child div
    // that is 25mm × 80mm in CSS-mm and apply a transform).
    const inner = el("div", {
      style:
        `position:absolute;left:0;top:0;` +
        `width:${PRINTABLE_W}mm;height:${PRINTABLE_H}mm;` +
        // Browsers compute mm relative to assumed 96 dpi → 1 mm ≈ 3.78 px.
        // We want exactly PX_PER_MM. Use transform: scale.
        `transform-origin:top left;transform:scale(${(PX_PER_MM / 3.7795275591).toFixed(6)});`,
    });
    inner.innerHTML = p.bodyHtml;
    sheet.append(inner);
    const label = el(
      "div",
      { class: "muted small", style: "text-align:center;margin-top:2px;" },
      `Sheet ${i + 1}/${pages.length}`,
    );
    const col = el("div", {
      style: "display:inline-block;vertical-align:top;margin-right:12px;",
    });
    col.append(sheet, label);
    sheetsRow.append(col);
  }
  wrap.append(sheetsRow);
  if (pages.length > cap) {
    wrap.append(
      el(
        "div",
        { class: "muted small" },
        `… ${pages.length - cap} more sheet(s) (printed in full).`,
      ),
    );
  }
  return wrap;
}

function planSummary(plan: JobItem[]): string {
  const totalLabels = plan.reduce((acc, it) => acc + it.copies, 0);
  if (plan.length === 0) return "Plan is empty.";
  return `${plan.length} item(s) · ${totalLabels} label(s) total.`;
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

  const idCell = el("td", { class: "id-cell", title: item.id }, fmtId(item.id));
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

  // Extras cell: layout-specific option fields (cableOd, noMarkers, etc.).
  const extrasCell = el("td");
  const extraInputs: Record<string, HTMLInputElement> = {};
  const updateExtras = () => {
    const layout = getLayout(layoutSel.value);
    const fields = layout?.optionFields?.() ?? [];
    extrasCell.innerHTML = "";
    for (const f of fields) {
      if (f.type === "checkbox") {
        const cb = document.createElement("input");
        cb.type = "checkbox";
        cb.checked = Boolean(item.extras[f.key]);
        cb.title = f.label;
        const lbl = el("label", { class: "muted small", style: "display:inline-flex;align-items:center;gap:2px;margin-right:6px;" });
        lbl.append(cb, f.label);
        extrasCell.append(lbl);
        extraInputs[f.key] = cb;
        cb.addEventListener("change", persist);
      } else {
        const inp = numberInput({
          value: (item.extras[f.key] as number) ?? f.default,
          min: f.min,
          max: f.max,
          step: f.step,
        });
        inp.title = f.label;
        extrasCell.append(inp);
        extraInputs[f.key] = inp;
        inp.addEventListener("change", persist);
      }
    }
  };
  updateExtras();
  tr.append(extrasCell);

  const copiesIn = numberInput({ value: item.copies, min: 1, max: 100, step: 1 });
  tr.append(el("td", {}, copiesIn));

  // Matrix-add (#11): duplicate this row for the same ID with the next
  // layout in the registry. Lets the operator stack a `horz` housing
  // label + `flag` cable label for one part with a single click.
  const matrixBtn = button(
    {
      class: "icon-only matrix-add",
      title: "Add another layout for this ID",
    },
    icon("plus"),
  );
  matrixBtn.addEventListener("click", () => {
    const plan = loadPlan();
    const current = plan[index];
    if (!current) return;
    const layouts = allLayouts();
    const idx = layouts.findIndex((l) => l.id === current.layoutId);
    const next = layouts[(idx + 1) % layouts.length];
    if (!next) return;
    const nextLayout = getLayout(next.id);
    const fields = nextLayout?.optionFields?.() ?? [];
    const extras: Record<string, number> = {};
    for (const f of fields) {
      // Carry forward matching extras from current row, else default.
      extras[f.key] = current.extras[f.key] ?? (f.default as number);
    }
    plan.splice(index + 1, 0, {
      id: current.id,
      layoutId: next.id,
      size: current.size,
      copies: current.copies,
      extras,
    });
    savePlan(plan);
    onChange();
  });

  const trashBtn = button({ class: "icon-only", title: "Remove" }, icon("trash"));
  trashBtn.addEventListener("click", () => {
    const plan = loadPlan();
    plan.splice(index, 1);
    savePlan(plan);
    onChange();
  });
  tr.append(el("td", { class: "row-actions" }, matrixBtn, trashBtn));

  // Persist any field change.
  const persist = () => {
    const plan = loadPlan();
    const target = plan[index];
    if (!target) return;
    target.layoutId = layoutSel.value;
    target.size = parseFloat(sizeIn.value) || target.size;
    target.copies = Math.max(1, parseInt(copiesIn.value, 10) || target.copies);
    const layout = getLayout(target.layoutId);
    const fields = layout?.optionFields?.() ?? [];
    const extras: Record<string, number> = {};
    for (const f of fields) {
      const inp = extraInputs[f.key];
      if (!inp) continue;
      if (f.type === "checkbox") {
        extras[f.key] = inp.checked ? 1 : 0;
      } else {
        extras[f.key] = parseFloat(inp.value) || (f.default as number);
      }
    }
    target.extras = extras;
    savePlan(plan);
    onChange();
  };
  layoutSel.addEventListener("change", () => {
    persist();
    updateExtras();
  });
  for (const inp of [sizeIn, copiesIn]) {
    inp.addEventListener("change", persist);
  }

  return tr;
}

function renderEntryRow(ctx: AppContext, onAdd: () => void): HTMLElement {
  const tr = el("tr", { class: "entry-row" });
  const idIn = input({
    type: "text",
    placeholder: "ID (14-char)",
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
  const idCell = el("td", { class: "id-cell" });
  idCell.append(idWrap);
  tr.append(idCell);

  // Update title on the ID cell whenever the input changes, so
  // hovering shows the raw canonical ID for copy.
  idIn.addEventListener("input", () => {
    const raw = idIn.value.trim().toUpperCase().replace(/-/g, "");
    idCell.title = raw.length === 14 ? raw : "";
  });

  const layoutSel = select(
    allLayouts().map((l) => ({ value: l.id, label: l.label })),
  );
  layoutSel.value = "horz";
  tr.append(el("td", {}, layoutSel));

  const sizeIn = numberInput({ value: DEFAULT_SIZE_MM, min: 4, max: 100, step: 0.5 });
  const unitSel = select([
    { value: "mm", label: "mm" },
    { value: "px", label: "px" },
  ]);
  unitSel.value = "mm";
  const sizeHint = el("div", { class: "muted small size-hint" });
  const updateSizeHint = () => {
    if (unitSel.value === "px") {
      const px = parseFloat(sizeIn.value) || 0;
      sizeHint.textContent = `= ${(px * PX_TO_MM).toFixed(2)} mm`;
    } else {
      sizeHint.textContent = "";
    }
  };
  unitSel.addEventListener("change", updateSizeHint);
  sizeIn.addEventListener("input", updateSizeHint);
  const sizeWrap = el("div");
  const sizeRow = el("div", { style: "display:flex;gap:4px;align-items:center;" });
  sizeRow.append(sizeIn, unitSel);
  sizeWrap.append(sizeRow, sizeHint);
  tr.append(el("td", {}, sizeWrap));

  const extrasCell = el("td");
  const entryExtraInputs: Record<string, HTMLInputElement> = {};
  const updateExtras = () => {
    const layout = getLayout(layoutSel.value);
    const fields = layout?.optionFields?.() ?? [];
    extrasCell.innerHTML = "";
    for (const f of fields) {
      if (f.type === "checkbox") {
        const cb = document.createElement("input");
        cb.type = "checkbox";
        cb.checked = false;
        cb.title = f.label;
        const lbl = el("label", { class: "muted small", style: "display:inline-flex;align-items:center;gap:2px;margin-right:6px;" });
        lbl.append(cb, f.label);
        extrasCell.append(lbl);
        entryExtraInputs[f.key] = cb;
      } else {
        const inp = numberInput({
          value: f.default as number,
          min: f.min,
          max: f.max,
          step: f.step,
        });
        inp.title = f.label;
        extrasCell.append(inp);
        entryExtraInputs[f.key] = inp;
      }
    }
  };
  layoutSel.addEventListener("change", updateExtras);
  updateExtras();
  tr.append(extrasCell);

  const copiesIn = numberInput({ value: 1, min: 1, max: 100, step: 1 });
  tr.append(el("td", {}, copiesIn));

  const addBtn = button({ class: "icon-only primary", title: "Add to plan" }, icon("plus"));
  addBtn.addEventListener("click", () => {
    const id = idIn.value.trim().toUpperCase().replace(/-/g, "");
    if (id.length !== 14) {
      alert("ID must be 14 characters.");
      return;
    }
    const layout = getLayout(layoutSel.value);
    const fields = layout?.optionFields?.() ?? [];
    const extras: Record<string, number> = {};
    for (const f of fields) {
      const inp = entryExtraInputs[f.key];
      if (!inp) continue;
      if (f.type === "checkbox") {
        extras[f.key] = inp.checked ? 1 : 0;
      } else {
        extras[f.key] = parseFloat(inp.value) || (f.default as number);
      }
    }
    const rawSize = parseFloat(sizeIn.value);
    const sizeMm = unitSel.value === "px" ? rawSize * PX_TO_MM : rawSize;
    const plan = loadPlan();
    plan.push({
      id,
      layoutId: layoutSel.value,
      size: sizeMm,
      copies: parseInt(copiesIn.value, 10),
      extras,
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
    if (id.length !== 14) return;
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

export function fmtId(id: string): string {
  if (id.length < 8) return id;
  // Full 14-char ID in 4-4-4-2 grouping: ABCD-EFGH-JKMN-PQ
  return `${id.slice(0, 4)}-${id.slice(4, 8)}-${id.slice(8, 12)}-${id.slice(12, 14)}`;
}

// Open a print-only window with the HTML produced by the active output
// mode. The mode owns the @page rules and body content; we only host
// the popup and trigger window.print().
function openPrintWindow(html: string): void {
  const w = window.open("", "_blank", "width=400,height=600");
  if (!w) {
    alert("Pop-up blocked — allow pop-ups for this site to print.");
    return;
  }
  w.document.open();
  w.document.write(html);
  w.document.close();
}
