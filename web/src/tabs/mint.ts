// Mint tab — generate + register IDs from the browser.
//
// Two flows:
//   A. "Mint + Print" — generates IDs, then adds them to the Print tab's
//      plan and switches to Print.
//   B. "Mint for export" — generates IDs, then downloads CSV or copies
//      to clipboard.
//
// Both flows can optionally submit IDs to the data repo registry via
// the same PR pipeline used by the Bind tab.

import { ID_ALPHABET, ID_LENGTH, DATA_REPO_SLUG } from "../config";
import type { AppContext, Tab } from "../core/types";
import { appendBind, loadQueue } from "../registry/queue";
import {
  submitBatch,
  promptForToken,
  getStoredToken,
  SubmitError,
} from "../registry/submit";
import { el, button, input, formRow, number as numberInput } from "../ui/dom";

// ---- ID generation ----

/** Generate a single random ID from the given alphabet. */
export function generateId(alphabet: string, length: number): string {
  const bytes = new Uint8Array(length);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (b) => alphabet[b % alphabet.length]).join("");
}

/** Generate `count` unique random IDs. */
export function generateIds(
  count: number,
  alphabet: string,
  length: number,
): string[] {
  const ids = new Set<string>();
  // Guard against infinite loops if the alphabet/length space is too
  // small (shouldn't happen with the real 14-char base-25 alphabet, but
  // be safe in tests).
  let attempts = 0;
  const maxAttempts = count * 10;
  while (ids.size < count && attempts < maxAttempts) {
    ids.add(generateId(alphabet, length));
    attempts++;
  }
  return [...ids];
}

// ---- Print plan integration ----

const PRINT_PLAN_KEY = "part-registry.print-plan";

interface PrintPlanItem {
  id: string;
  layoutId: string;
  size: number;
  copies: number;
  extras: Record<string, number>;
}

function loadPrintPlan(): PrintPlanItem[] {
  try {
    const raw = localStorage.getItem(PRINT_PLAN_KEY);
    if (!raw) return [];
    return JSON.parse(raw) as PrintPlanItem[];
  } catch {
    return [];
  }
}

function savePrintPlan(plan: PrintPlanItem[]): void {
  localStorage.setItem(PRINT_PLAN_KEY, JSON.stringify(plan));
}

// ---- CSV export ----

function buildCsv(
  ids: string[],
  batch: string,
  notes: string,
): string {
  const now = new Date().toISOString();
  const header = "id,status,minted_at,batch,notes";
  const rows = ids.map(
    (id) =>
      `${id},unbound,${now},${escapeCsvField(batch)},${escapeCsvField(notes)}`,
  );
  return [header, ...rows].join("\n") + "\n";
}

function escapeCsvField(val: string): string {
  if (val.includes(",") || val.includes('"') || val.includes("\n")) {
    return `"${val.replace(/"/g, '""')}"`;
  }
  return val;
}

function downloadCsv(csv: string, filename: string): void {
  const blob = new Blob([csv], { type: "text/csv;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.style.display = "none";
  document.body.append(a);
  a.click();
  // Clean up after a tick.
  setTimeout(() => {
    URL.revokeObjectURL(url);
    a.remove();
  }, 100);
}

// ---- Clipboard ----

async function copyToClipboard(ids: string[]): Promise<void> {
  await navigator.clipboard.writeText(ids.join("\n"));
}

// ---- 4-4-4 display formatting ----

function fmtId(id: string): string {
  if (id.length < 12) return id;
  return `${id.slice(0, 4)}-${id.slice(4, 8)}-${id.slice(8, 12)}${
    id.length > 12 ? "-" + id.slice(12) : ""
  }`;
}

// ---- Default batch label ----

function defaultBatchLabel(): string {
  const d = new Date();
  const yyyy = d.getFullYear();
  const mm = String(d.getMonth() + 1).padStart(2, "0");
  const dd = String(d.getDate()).padStart(2, "0");
  return `B-${yyyy}-${mm}-${dd}`;
}

// ---- Tab ----

export const mintTab: Tab = {
  id: "mint",
  label: "Mint",
  mount(container, ctx) {
    container.innerHTML = "";
    container.append(buildUI(ctx));
  },
};

function buildUI(ctx: AppContext): HTMLElement {
  const root = el("div", { class: "tab tab--mint" });
  root.append(el("h2", {}, "Mint"));
  root.append(
    el(
      "p",
      { class: "muted" },
      "Generate new part IDs. Minted IDs start as \"unbound\" — bind them later to associate metadata.",
    ),
  );

  // ---- Inputs ----
  const countInput = numberInput({ value: 1, min: 1, max: 100 });
  countInput.style.width = "80px";

  const batchInput = input({ type: "text", value: defaultBatchLabel(), placeholder: "B-YYYY-MM-DD" });
  const notesInput = input({ type: "text", value: "", placeholder: "Optional notes" });

  root.append(
    formRow([el("label", {}, "Count"), countInput]),
    formRow([el("label", {}, "Batch"), batchInput]),
    formRow([el("label", {}, "Notes"), notesInput]),
  );

  // ---- Mint button ----
  const mintBtn = button({ class: "primary" }, "Mint");
  root.append(formRow([mintBtn]));

  // ---- Results area ----
  const resultsArea = el("div", { class: "mint__results" });
  root.append(resultsArea);

  let mintedIds: string[] = [];

  mintBtn.addEventListener("click", () => {
    const count = Math.min(Math.max(1, Number(countInput.value) || 1), 100);
    mintedIds = generateIds(count, ID_ALPHABET, ID_LENGTH);
    renderResults();
  });

  function renderResults(): void {
    resultsArea.innerHTML = "";
    if (mintedIds.length === 0) return;

    resultsArea.append(
      el("h3", {}, `${mintedIds.length} ID${mintedIds.length > 1 ? "s" : ""} generated`),
    );

    const list = el("ul", { class: "mint__id-list" });
    for (const id of mintedIds) {
      list.append(el("li", {}, fmtId(id)));
    }
    resultsArea.append(list);

    // ---- Action buttons ----
    const actions = el("div", { class: "mint__actions" });
    const feedback = el("div", { class: "mint__feedback muted" });

    const addToPrintBtn = button({}, "Add to print plan");
    const downloadBtn = button({}, "Download CSV");
    const copyBtn = button({}, "Copy to clipboard");
    const submitBtn = button({ class: "primary" }, "Submit to registry");

    actions.append(addToPrintBtn, downloadBtn, copyBtn, submitBtn);
    resultsArea.append(actions, feedback);

    // -- Add to print plan --
    addToPrintBtn.addEventListener("click", () => {
      const plan = loadPrintPlan();
      for (const id of mintedIds) {
        plan.push({
          id,
          layoutId: "horz",
          size: 11,
          copies: 1,
          extras: {},
        });
      }
      savePrintPlan(plan);
      feedback.textContent = `${mintedIds.length} ID(s) added to print plan.`;
      feedback.className = "mint__feedback muted";
      ctx.showTab("print");
    });

    // -- Download CSV --
    downloadBtn.addEventListener("click", () => {
      const csv = buildCsv(mintedIds, batchInput.value, notesInput.value);
      const ts = Date.now();
      downloadCsv(csv, `minted-ids-${ts}.csv`);
      feedback.textContent = "CSV downloaded.";
      feedback.className = "mint__feedback muted";
    });

    // -- Copy to clipboard --
    copyBtn.addEventListener("click", () => {
      void copyToClipboard(mintedIds).then(() => {
        feedback.textContent = `${mintedIds.length} ID(s) copied to clipboard.`;
        feedback.className = "mint__feedback muted";
      }).catch(() => {
        feedback.textContent = "Clipboard copy failed (not allowed in this browser context).";
        feedback.className = "mint__feedback error";
      });
    });

    // -- Submit to registry --
    submitBtn.addEventListener("click", () => {
      void submitToRegistry(mintedIds, batchInput.value, notesInput.value, feedback, submitBtn);
    });
  }

  return root;
}

async function submitToRegistry(
  ids: string[],
  batch: string,
  notes: string,
  feedback: HTMLElement,
  submitBtn: HTMLButtonElement,
): Promise<void> {
  let token = getStoredToken();
  if (!token) {
    token = promptForToken();
    if (!token) {
      feedback.textContent = "Submit cancelled — no token provided.";
      feedback.className = "mint__feedback warn";
      return;
    }
  }

  // Queue all IDs as unbound binds — the submit pipeline expects
  // QueueItem[]. We add them to the shared queue, submit, then remove
  // the ones we added.
  for (const id of ids) {
    appendBind({
      id,
      type: "",
      description: "",
      vendor: "",
      part_number: "",
      location: "",
      notes,
    });
  }

  const queue = loadQueue();
  // Filter to just the items we added (by id set).
  const idSet = new Set(ids);
  const toSubmit = queue.filter((q) => idSet.has(q.id));

  submitBtn.disabled = true;
  feedback.textContent = "Submitting...";
  feedback.className = "mint__feedback muted";

  try {
    const result = await submitBatch(toSubmit, token, DATA_REPO_SLUG);
    feedback.innerHTML = "";
    feedback.className = "mint__feedback";
    const link = el("a", { href: result.prUrl, target: "_blank", rel: "noopener" }, `PR #${result.prNumber}`);
    feedback.append("Submitted! ", link);
  } catch (e) {
    const msg =
      e instanceof SubmitError
        ? `Submit failed at step "${e.step}": ${e.message}`
        : `Submit failed: ${(e as Error).message}`;
    feedback.textContent = msg;
    feedback.className = "mint__feedback error";
  } finally {
    submitBtn.disabled = false;
  }
}
