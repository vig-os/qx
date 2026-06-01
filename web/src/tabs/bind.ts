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

import { FIELDS, type RegistryRow, type FieldDef } from "../registry/schema";
import { REGISTRY_CONTRACT, type TypeField } from "../registry/contract";
import { parseMetadata, serializeMetadata } from "../registry/metadata";
import { runPreflight, type QueueItem } from "../registry/preflight";
import {
  appendBind,
  loadQueue,
  removeAt,
  saveQueue,
  type EditableKey,
  type QueuedBind,
  type QueuedEdit,
} from "../registry/queue";
import { validateField, type ValidationError } from "../registry/validate";
import type { AppContext, Tab } from "../core/types";
import { el, button, input, formRow } from "../ui/dom";
import { icon } from "../ui/icons";
import { renderErrorCard, renderValidationErrors } from "../ui/error-card";
import { tableScroll, makeFilterDropdown } from "../ui/components/data-table";
import { makeCombobox } from "../ui/components/combobox";
import { makeTagsInput } from "../ui/components/tags-input";
import { openRowEditor } from "../ui/components/row-editor";
import { fieldVocabOptions, componentCandidates, stageVocabValue } from "../registry/vocab";
import { parseComponents } from "../registry/assembly-graph";
import {
  openScannerMulti,
  openImageScan,
  openScannerRolling,
  type ScanStatus,
} from "../ui/scanner";
import type { Action, AuthDecision } from "../wasm/loader";
import {
  submitSession,
  promptForToken,
  getStoredToken,
  clearToken,
  SubmitError,
} from "../registry/submit";
import { loadSession, clearSession, summarizeSession, addBind as sessionAddBind, removeItemAt as sessionRemoveAt, getSessionSync, type SessionMint } from "../registry/session";
import { DATA_REPO_SLUG } from "../config";

// Editable fields shown as columns in the bind queue table. Excludes
// `json`-type fields (metadata) — those need typeFields-driven rendering
// (#171 P1), not a raw-JSON text column.
const BIND_FIELDS = FIELDS.filter((f) => f.editable && f.type !== "json");

function emptyBindFields(): Pick<
  QueuedBind,
  "type" | "description" | "vendor" | "part_number" | "location" | "notes" | "components" | "manufacturer_id" | "metadata"
> {
  return {
    type: "",
    description: "",
    vendor: "",
    part_number: "",
    location: "",
    notes: "",
    components: "",
    manufacturer_id: "",
    metadata: "",
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
  const root = el("div", { class: "tabview tab--bind" });

  // Datalist for ID autocomplete — shows unbound IDs from the registry
  const datalist = document.createElement("datalist");
  datalist.id = "unbound-ids";
  const unboundIds = ctx.registry.all()
    .filter((r) => r.status === "unbound")
    .map((r) => r.id);
  for (const id of unboundIds) {
    const opt = document.createElement("option");
    opt.value = fmtId(id);
    datalist.append(opt);
  }
  root.append(datalist);

  root.append(el("h2", {}, "Session queue"));
  root.append(
    el(
      "p",
      { class: "muted small" },
      "All pending operations (mints, binds, edits, voids) are shown here. " +
        "Add binds via \"+\u00a0Add row\" or scan QR codes. Edits and voids arrive from the Lookup tab. " +
        "Mints arrive from the Mint tab. Submit creates a single PR with all changes.",
    ),
  );

  const submitBtn = button({ class: "primary" }, icon("plus"), " Submit session");
  const clearBtn = button({ class: "destructive" }, icon("trash"), " Clear session");
  const summaryEl = el("span", { class: "queue-summary muted small" });

  // ADR-016 §"FE preflight" + issue #23: every queue mutation re-runs
  // the same classify + policy engine the CI gate runs, advisory only.
  const preflightContainer = el("div", { class: "preflight" });

  // Persistent error card container (replaces alert() for submit errors).
  const submitErrorContainer = el("div", { class: "submit-error" });

  const tableContainer = el("div", {});

  // Queue filter bar (PR2) — bulk imports can stack dozens of rows, so the
  // Bind queue gets the same affordances as Lookup: a free-text filter plus
  // a Kind multi-select, both reusing the shared makeFilterDropdown. Filter
  // state lives here so it survives re-renders; applyQueueFilter hides
  // non-matching rows (never the bottom "+ new bind" entry row).
  let queueSearch = "";
  const kindFilter = new Set<string>();

  // A bind "row" can be a fragment (main tr + a Properties sub-row), so we
  // read kind from the .chip--kind badge on the main row; sub-rows have no
  // badge and inherit their parent row's visibility.
  const rowKind = (tr: HTMLElement): string | null => {
    const chip = tr.querySelector(".chip--kind");
    if (!chip) return null;
    for (const k of ["mint", "bind", "edit", "void"]) {
      if (chip.classList.contains(`chip--${k}`)) return k;
    }
    return "";
  };

  const applyQueueFilter = () => {
    const q = queueSearch.trim().toLowerCase();
    const rows = tableContainer.querySelectorAll<HTMLTableRowElement>("tbody tr");
    let shown = 0;
    let parentVisible = true;
    rows.forEach((tr) => {
      if (tr.classList.contains("entry-row")) return; // always visible
      const kind = rowKind(tr);
      if (kind === null) {
        // Sub-row (e.g. Properties) — follow the row it belongs to.
        tr.style.display = parentVisible ? "" : "none";
        return;
      }
      const kindOk = kindFilter.size === 0 || kindFilter.has(kind);
      const textOk = q === "" || (tr.textContent ?? "").toLowerCase().includes(q);
      parentVisible = kindOk && textOk;
      tr.style.display = parentVisible ? "" : "none";
      if (parentVisible) shown += 1;
    });
    queueFilterBar.style.display = rows.length > 1 ? "" : "none"; // hide when only the entry row
    queueFilterCount.textContent =
      (queueSearch || kindFilter.size > 0) ? `${shown} shown` : "";
  };

  const queueSearchInput = input({
    type: "search",
    placeholder: "Filter queue…",
    class: "queue-filter-search",
    "aria-label": "Filter session queue",
  });
  queueSearchInput.addEventListener("input", () => {
    queueSearch = queueSearchInput.value;
    applyQueueFilter();
  });
  const kindDd = makeFilterDropdown(
    "Kind",
    () => ["mint", "bind", "edit", "void"],
    kindFilter,
    applyQueueFilter,
  );
  const queueFilterCount = el("span", { class: "muted small" });
  const queueFilterBar = el("div", { class: "filter-bar" }, queueSearchInput, kindDd.wrap, queueFilterCount);
  queueFilterBar.style.display = "none";

  const refreshPreflight = (queue: ReadonlyArray<QueuedBind | QueuedEdit>) => {
    preflightContainer.innerHTML = "";
    if (queue.length === 0) return;
    try {
      const registry = buildRegistryMap(ctx);
      // Same-session mints aren't in the loaded registry yet, but binding
      // them in the same batch is legitimate (mint-from-label, CSV import
      // with mint). Treat pending mints as known unbound parts so the
      // preflight doesn't flag them unknown_id and block submit (#176).
      addSessionMints(registry);
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
            components: q.components,
          },
        }));
      const result = runPreflight(items, registry);
      preflightContainer.append(renderPreflight(result));
      // Block submit on policy block OR an FE-local definite error:
      // unknown id, or an assembly component that's unknown/void/self
      // (#176 — merging into an assembly via mint+bind). These are
      // factual errors, not policy judgments, so blocking is safe.
      const BLOCKING_ISSUES = new Set([
        "unknown_id",
        "unknown_component",
        "void_component",
        "self_component",
      ]);
      const blocked =
        result.decision.kind === "block" ||
        result.localIssues.some((i) => BLOCKING_ISSUES.has(i.kind));
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

    // Show full session summary (including mints)
    void loadSession().then((sess) => {
      const stats = summarizeSession(sess);
      summaryEl.textContent =
        sess.items.length === 0
          ? ""
          : `${stats.total} item(s): ${stats.label}`;
    });

    const table = el("table", { class: "data bind-queue" });
    const thead = el("thead");
    const tr = el("tr");
    tr.append(el("th", {}, "Kind"));
    tr.append(el("th", {}, "ID"));
    for (const f of BIND_FIELDS) {
      tr.append(el("th", {}, f.label));
    }
    tr.append(el("th", {}, "Queued"));
    tr.append(el("th", {}, ""));
    thead.append(tr);
    table.append(thead);

    const tbody = el("tbody");

    // Render mint items from the session (read-only, shown at the top)
    void loadSession().then((sess) => {
      const mintItems = sess.items.filter((i) => i.kind === "mint");
      for (let i = 0; i < mintItems.length; i++) {
        const mint = mintItems[i];
        // Find the actual index in session.items for removal
        const sessionIdx = sess.items.indexOf(mint);
        tbody.insertBefore(renderMintRow(mint as SessionMint, sessionIdx, renderTable), tbody.firstChild);
      }
      applyQueueFilter(); // mints arrive async — re-apply once they're in
    });

    for (let i = 0; i < queue.length; i++) {
      const item = queue[i];
      if (item.kind === "bind") {
        tbody.append(renderBindRow(item, i, renderTable, ctx));
      } else {
        tbody.append(renderEditRow(item, i, renderTable));
      }
    }

    // Bottom "new bind" row.
    tbody.append(renderEntryRow(ctx, renderTable));

    table.append(tbody);
    tableContainer.append(tableScroll(table, { maxHeight: false }));
    applyQueueFilter();

    void loadSession().then((sess) => {
      if (sess.items.length === 0) {
        tableContainer.append(
          el("p", { class: "muted small" }, "Session is empty. Mint IDs, add binds below, or queue edits from the Lookup tab."),
        );
      }
    });
  };

  renderTable();

  submitBtn.addEventListener("click", async () => {
    const session = await loadSession();
    if (session.items.length === 0) {
      submitErrorContainer.innerHTML = "";
      submitErrorContainer.append(
        renderErrorCard({
          title: "Nothing to submit",
          message: "Session is empty. Mint IDs, add binds, or queue edits first.",
          kind: "warning",
          actions: [{ label: "Dismiss", onClick: () => { submitErrorContainer.innerHTML = ""; } }],
        }),
      );
      return;
    }

    // Check for field-level validation errors before submitting.
    const fieldErrors = document.querySelectorAll(".field--error");
    if (fieldErrors.length > 0) {
      const errorFields = Array.from(fieldErrors).map((el) => ({
        field: el.getAttribute("title") ?? el.getAttribute("placeholder") ?? "field",
        message: el.parentElement?.querySelector(".field-error")?.textContent ?? "invalid",
      }));
      submitErrorContainer.innerHTML = "";
      submitErrorContainer.append(
        renderValidationErrors(errorFields, () => { submitErrorContainer.innerHTML = ""; }),
      );
      return;
    }

    // Ensure we have a PAT.
    let token = getStoredToken();
    if (!token) {
      token = await promptForToken();
      if (!token) return; // cancelled
    }

    const stats = summarizeSession(session);
    if (
      !confirm(
        `Submit session (${stats.label}) as a PR to ${DATA_REPO_SLUG}?`,
      )
    ) {
      return;
    }

    submitBtn.disabled = true;
    submitBtn.textContent = "Submitting\u2026";

    try {
      submitErrorContainer.innerHTML = "";
      const result = await submitSession(session, token, DATA_REPO_SLUG);
      await clearSession();
      renderTable();
      // Success card instead of alert
      submitErrorContainer.innerHTML = "";
      submitErrorContainer.append(
        renderErrorCard({
          title: "PR created",
          message: `PR #${result.prNumber} submitted successfully.`,
          kind: "warning", // reuse warning style for green-ish success
          details: [{ label: "URL", value: result.prUrl }],
          actions: [
            {
              label: "Open PR",
              style: "primary",
              onClick: () => window.open(result.prUrl, "_blank", "noopener"),
            },
            {
              label: "Dismiss",
              onClick: () => { submitErrorContainer.innerHTML = ""; },
            },
          ],
        }),
      );
    } catch (e) {
      const msg = e instanceof SubmitError
        ? `Failed at step "${e.step}": ${e.message}`
        : `${(e as Error).message}`;
      console.error("Submit error:", e);

      const details: Array<{ label: string; value: string }> = [];
      if (e instanceof SubmitError) {
        details.push({ label: "Step", value: e.step });
        if (e.status) details.push({ label: "HTTP status", value: String(e.status) });
      }

      const isAuthError = e instanceof SubmitError && (e.status === 401 || e.status === 403);

      submitErrorContainer.innerHTML = "";
      submitErrorContainer.append(
        renderErrorCard({
          title: "Submit failed",
          message: msg,
          details,
          actions: [
            {
              label: "Retry",
              style: "primary",
              onClick: () => {
                submitErrorContainer.innerHTML = "";
                submitBtn.click();
              },
            },
            ...(isAuthError
              ? [{
                  label: "Re-enter token",
                  style: "outline" as const,
                  onClick: () => {
                    clearToken();
                    submitErrorContainer.innerHTML = "";
                    submitBtn.click();
                  },
                }]
              : []),
            {
              label: "Dismiss",
              onClick: () => { submitErrorContainer.innerHTML = ""; },
            },
          ],
        }),
      );
    } finally {
      submitBtn.disabled = false;
      submitBtn.textContent = "";
      submitBtn.append(icon("plus"), " Submit session");
    }
  });

  clearBtn.addEventListener("click", () => {
    void loadSession().then((sess) => {
      if (sess.items.length === 0) return;
      if (!confirm("Clear the entire session without submitting? This will discard all pending mints, binds, edits, and voids.")) return;
      void clearSession().then(() => renderTable());
    });
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

  const addScannedIds = async (ids: string[]) => {
    if (ids.length === 0) return;
    const queuedIds = new Set(loadQueue().map((q) => q.id));
    for (const id of ids) {
      if (queuedIds.has(id)) continue;
      await sessionAddBind(id, {});
    }
    renderTable();
  };

  const uploadBtn = button({ class: "secondary" }, icon("upload"), " Upload image");
  uploadBtn.addEventListener("click", async () => {
    try {
      const ids = await openImageScan({
        resolveStatus: makeResolveStatus(),
      });
      await addScannedIds(ids);
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

  // #176 reorg: recognition-by-label ("Scan text") moved to the Lookup
  // tab — that's where "which part is this?" belongs. The Bind tab keeps
  // scans that queue parts for binding + "Mint from label" (creates).

  // #176 P0: bulk import — paste/upload a CSV/TSV, map columns, commit
  // mint+bind rows into the queue.
  const importBtn = button({ class: "secondary" }, icon("upload"), " Import list");
  importBtn.addEventListener("click", async () => {
    const { openImportModal } = await import("../ui/import-modal");
    const existingIds = new Set(ctx.registry.all().map((r) => r.id));
    const result = await openImportModal({ existingIds });
    if (result) {
      renderTable();
    }
  });

  // #176 P1: mint-from-label — photograph a manufacturer label, extract
  // fields (operator-assisted + regex pre-fill), mint+bind one part.
  const mintLabelBtn = button({ class: "secondary" }, icon("scan-text"), " Mint from label");
  mintLabelBtn.addEventListener("click", async () => {
    const { openOcrExtract } = await import("../ui/ocr-extract-scan");
    const existingIds = new Set(ctx.registry.all().map((r) => r.id));
    const result = await openOcrExtract({ existingIds });
    if (result) {
      renderTable();
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
    submitErrorContainer,
    formRow([uploadBtn, rollingBtn, importBtn, mintLabelBtn]),
    formRow([repeatLabel]),
    preflightContainer,
    queueFilterBar,
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

/** Add session-pending mints to the registry map as synthetic unbound
 *  rows, so a bind queued in the same session as its mint is treated as
 *  a known part by the preflight (#176). Read-only synchronous peek at
 *  the session cache. */
function addSessionMints(map: Map<string, RegistryRow>): void {
  const sess = getSessionSync();
  if (!sess) return;
  for (const item of sess.items) {
    if (item.kind === "mint" && !map.has(item.id)) {
      map.set(item.id, {
        id: item.id,
        status: "unbound",
        batch: (item as SessionMint).batch ?? "",
      } as unknown as RegistryRow);
    }
  }
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

function renderMintRow(
  item: SessionMint,
  sessionIndex: number,
  onChange: () => void,
): HTMLElement {
  const tr = el("tr", { class: "queue-row queue-row--mint", "data-kind": "mint", "data-id": item.id });
  tr.append(el("td", {}, el("span", { class: "chip chip--kind chip--mint" }, "mint")));

  // ID cell (read-only)
  tr.append(el("td", { class: "id-cell" }, fmtId(item.id)));

  // Editable field cells — mints don't have bind metadata, show dashes
  for (const f of BIND_FIELDS) {
    const cell = el("td");
    if (f.key === "notes" && item.notes) {
      cell.append(item.notes);
    } else {
      cell.append(el("span", { class: "muted" }, "\u2014"));
    }
    tr.append(cell);
  }

  // Queued-at timestamp
  tr.append(
    el(
      "td",
      { class: "muted small" },
      new Date(item.createdAt).toLocaleString(),
    ),
  );

  // Remove button
  const trashBtn = button({ class: "icon-only", title: "Remove from session" }, icon("trash"));
  trashBtn.addEventListener("click", () => {
    void sessionRemoveAt(sessionIndex).then(() => onChange());
  });
  tr.append(el("td", { class: "row-actions" }, trashBtn));

  return tr;
}

function renderBindRow(
  item: QueuedBind,
  index: number,
  onChange: () => void,
  ctx: AppContext,
): DocumentFragment {
  const frag = document.createDocumentFragment();
  const tr = el("tr", { class: "queue-row queue-row--bind", "data-kind": "bind", "data-id": item.id });
  tr.append(el("td", {}, el("span", { class: "chip chip--kind chip--bind" }, "bind")));

  // Editable ID cell — supports blank rows added via "+" button
  const idCell = el("td", { class: "id-cell" });
  const idInp = input({
    type: "text",
    value: item.id ? fmtId(item.id) : "",
    placeholder: "XXXX-XXXX-XXXX-XX",
    autocapitalize: "characters",
    maxlength: "19", // 14 chars + 4 dashes + 1 buffer
    list: "unbound-ids",
  });

  // Auto-format on input: uppercase, insert dashes in 4-4-4-2 pattern
  idInp.addEventListener("input", () => {
    const pos = idInp.selectionStart ?? 0;
    const raw = idInp.value.toUpperCase().replace(/[^A-Z0-9]/g, "").slice(0, 14);
    const formatted = fmtId(raw);
    if (idInp.value !== formatted) {
      idInp.value = formatted;
      // Restore cursor position, adjusting for inserted dashes
      const dashesBeforePos = formatted.slice(0, pos).split("-").length - 1;
      const rawBeforePos = idInp.value.slice(0, pos).replace(/-/g, "").length;
      const newPos = rawBeforePos + dashesBeforePos;
      idInp.setSelectionRange(newPos, newPos);
    }
  });

  idInp.addEventListener("change", () => {
    const raw = idInp.value.trim().toUpperCase().replace(/[^A-Z0-9]/g, "").slice(0, 14);
    idInp.value = raw.length > 0 ? fmtId(raw) : "";
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

  for (const f of BIND_FIELDS) {
    const cell = el("td");
    const key = f.key as EditableKey;
    const currentValue = (item as unknown as Record<string, string>)[key] ?? "";
    const errEl = el("div", { class: "field-error small" });
    errEl.style.display = "none";

    const persistValue = (val: string) => {
      const queue = loadQueue();
      const current = queue[index];
      if (current && current.kind === "bind") {
        (current as unknown as Record<string, string>)[key] = val;
        saveQueue(queue);
        // #92: track last-edited fields for repeat mode
        if (repeatMode) {
          if (!lastBindFields) lastBindFields = {} as Record<EditableKey, string>;
          lastBindFields[key] = val;
        }
      }
    };

    // Controlled-vocabulary combobox for vendor / location (PR3): fuzzy
    // pick-or-create instead of free text, so spellings converge and a new
    // value is staged for write-back to the data-repo vocabulary.
    if (key === "vendor" || key === "location") {
      const combo = makeCombobox({
        value: currentValue,
        getOptions: () => fieldVocabOptions(ctx, key),
        ariaLabel: f.label,
        inputClass: "bind-field-input",
        onChange: (val, isNew) => {
          persistValue(val);
          if (isNew) stageVocabValue(key, val);
          showFieldErrors(errEl, combo.input, validateField(key, val, f));
        },
      });
      cell.append(combo.el, errEl);
      tr.append(cell);
      continue;
    }

    // Components multiselect (PR3): pick existing / staged-for-mint IDs as
    // chips; stored ";"-joined per the contract.
    if (key === "components") {
      const tags = makeTagsInput({
        value: parseComponents(currentValue),
        getOptions: () => componentCandidates(ctx),
        formatTag: fmtId,
        ariaLabel: f.label,
        onChange: (vals) => {
          persistValue(vals.join(";"));
          showFieldErrors(errEl, tags.el, validateField(key, vals.join(";"), f));
        },
      });
      cell.append(tags.el, errEl);
      tr.append(cell);
      continue;
    }

    const fieldInput = createFieldInput(f, currentValue);

    const runValidation = (val: string) => {
      const errs = validateField(key, val, f);
      showFieldErrors(errEl, fieldInput, errs);
    };

    fieldInput.addEventListener("change", () => {
      const val = fieldInput instanceof HTMLSelectElement ? fieldInput.value : (fieldInput as HTMLInputElement).value;
      // #171: changing `type` can change which type-specific properties
      // apply. Warn before discarding existing metadata, then re-render
      // so the Properties sub-row reflects the new type.
      if (key === "type" && val !== item.type) {
        const hadMeta = Object.keys(parseMetadata(item.metadata)).length > 0;
        if (hadMeta && !confirm(`Changing type from "${item.type}" to "${val}" will discard the current properties. Continue?`)) {
          (fieldInput as HTMLInputElement).value = item.type;
          return;
        }
        persistValue(val);
        if (hadMeta) {
          const queue = loadQueue();
          const current = queue[index];
          if (current && current.kind === "bind") {
            current.metadata = "";
            saveQueue(queue);
          }
        }
        onChange(); // rebuild row → Properties sub-row matches new type
        return;
      }
      persistValue(val);
      runValidation(val);
    });
    fieldInput.addEventListener("blur", () => {
      const val = fieldInput instanceof HTMLSelectElement ? fieldInput.value : (fieldInput as HTMLInputElement).value;
      runValidation(val);
    });

    cell.append(fieldInput, errEl);
    tr.append(cell);
  }

  tr.append(
    el(
      "td",
      { class: "muted small" },
      new Date(item.queued_at).toLocaleString(),
    ),
  );

  // Edit-in-popup (PR3): the inline cells are great for quick tweaks, but a
  // full row across the scrolling 11-column table is easier to edit in a
  // roomy modal form (the same combobox/tags controls).
  const editBtn = button({ class: "icon-only", title: "Edit in popup" }, icon("edit"));
  editBtn.addEventListener("click", () => {
    const queue = loadQueue();
    const current = queue[index];
    if (!current || current.kind !== "bind") return;
    const values: Record<string, string> = {};
    for (const f of BIND_FIELDS) {
      values[f.key] = (current as unknown as Record<string, string>)[f.key] ?? "";
    }
    openRowEditor({
      title: current.id ? `Edit ${fmtId(current.id)}` : "Edit bind row",
      fields: BIND_FIELDS,
      values,
      ctx,
      fmtId,
      onSave: (updated) => {
        const q = loadQueue();
        const cur = q[index];
        if (cur && cur.kind === "bind") {
          for (const f of BIND_FIELDS) {
            (cur as unknown as Record<string, string>)[f.key] = updated[f.key] ?? "";
          }
          saveQueue(q);
        }
        onChange();
      },
    });
  });

  const trashBtn = button({ class: "icon-only", title: "Remove from queue" }, icon("trash"));
  trashBtn.addEventListener("click", async () => {
    await removeAt(index);
    onChange();
  });
  tr.append(el("td", { class: "row-actions" }, editBtn, trashBtn));

  frag.append(tr);

  // #171: type-specific Properties sub-row, shown when the row's type
  // has typeFields defined in the contract.
  const typeFields = REGISTRY_CONTRACT.typeFields?.[item.type] ?? [];
  if (typeFields.length > 0) {
    frag.append(renderPropsRow(item, index, typeFields));
  }

  return frag;
}

// #171: full-width sub-row holding type-specific metadata inputs.
function renderPropsRow(
  item: QueuedBind,
  index: number,
  typeFields: TypeField[],
): HTMLElement {
  // Span: Kind + ID + BIND_FIELDS + Queued + actions.
  const totalCols = BIND_FIELDS.length + 4;
  const tr = el("tr", { class: "queue-row--props", "data-props-for": item.id });
  const td = el("td", { colspan: String(totalCols) });
  const wrap = el("div", { class: "props-editor" });
  wrap.append(el("span", { class: "props-editor__label muted small" }, `${item.type} properties:`));

  const meta = parseMetadata(item.metadata);

  for (const tf of typeFields) {
    const field = el("label", { class: "props-editor__field" });
    field.append(el("span", { class: "props-editor__field-label" },
      `${tf.label}${tf.unit ? ` (${tf.unit})` : ""}`));
    const current = meta[tf.key] != null ? String(meta[tf.key]) : "";
    const inp = createTypeFieldInput(tf, current);

    const persist = (raw: string) => {
      const queue = loadQueue();
      const current2 = queue[index];
      if (current2 && current2.kind === "bind") {
        const m = parseMetadata(current2.metadata);
        if (raw === "") {
          delete m[tf.key];
        } else {
          m[tf.key] = tf.type === "number" ? Number(raw) : raw;
        }
        current2.metadata = serializeMetadata(m);
        saveQueue(queue);
      }
    };

    inp.addEventListener("change", () => {
      const v = inp instanceof HTMLSelectElement ? inp.value
        : inp.type === "checkbox" ? (inp.checked ? "yes" : "")
        : inp.value;
      persist(v);
    });

    field.append(inp);
    wrap.append(field);
  }

  td.append(wrap);
  tr.append(td);
  return tr;
}

/** Build an input for a type-specific field (#171). */
function createTypeFieldInput(tf: TypeField, value: string): HTMLInputElement | HTMLSelectElement {
  switch (tf.type) {
    case "dropdown": {
      const sel = document.createElement("select");
      const empty = document.createElement("option");
      empty.value = "";
      empty.textContent = `-- ${tf.label} --`;
      sel.append(empty);
      for (const opt of tf.options ?? []) {
        const o = document.createElement("option");
        o.value = opt;
        o.textContent = opt;
        if (opt === value) o.selected = true;
        sel.append(o);
      }
      return sel;
    }
    case "yes-no": {
      const inp = document.createElement("input");
      inp.type = "checkbox";
      inp.checked = value === "true" || value === "yes" || value === "1";
      return inp;
    }
    case "number":
      return input({ type: "number", value });
    case "date":
      return input({ type: "date", value: value.slice(0, 10) });
    default:
      return input({ type: "text", value });
  }
}

function renderEditRow(
  item: QueuedEdit,
  index: number,
  onChange: () => void,
): HTMLElement {
  const tr = el("tr", { class: "queue-row queue-row--edit", "data-kind": "edit", "data-id": item.id });
  tr.append(el("td", {}, el("span", { class: "chip chip--kind chip--edit" }, "edit")));
  tr.append(el("td", { class: "id-cell" }, fmtId(item.id)));

  for (const f of BIND_FIELDS) {
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
  trashBtn.addEventListener("click", async () => {
    await removeAt(index);
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
  const editableCount = BIND_FIELDS.length;
  // +1 Kind, +1 ID, +editableCount fields, +1 Queued, +1 actions = editableCount + 4
  const totalCols = editableCount + 4;
  const addCell = el("td", { colspan: String(totalCols), style: "text-align: center;" });

  const addBlankBtn = button({ class: "secondary small", title: "Add blank row to queue" }, icon("plus"), " Add row");
  addBlankBtn.addEventListener("click", async () => {
    const fields = repeatMode && lastBindFields
      ? { ...lastBindFields }
      : emptyBindFields();
    await appendBind({ id: "", ...fields });
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
        await sessionAddBind(id, fields as Record<string, string>);
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

// ---------------------------------------------------------------------
// Dynamic field input rendering (#112)
// ---------------------------------------------------------------------

/** Create an appropriate input element based on the field's contract type. */
function createFieldInput(
  fieldDef: FieldDef,
  value: string,
): HTMLInputElement | HTMLSelectElement {
  switch (fieldDef.type) {
    case "dropdown": {
      if (fieldDef.on_unknown === "warn") {
        // Allow free text with a datalist for suggestions.
        const inp = input({ type: "text", value });
        const listId = `dl-${fieldDef.key}-${Math.random().toString(36).slice(2, 8)}`;
        const datalist = document.createElement("datalist");
        datalist.id = listId;
        for (const opt of fieldDef.options ?? []) {
          const o = document.createElement("option");
          o.value = opt;
          datalist.append(o);
        }
        inp.setAttribute("list", listId);
        inp.after(datalist);
        // Append datalist to DOM via a wrapper trick -- caller appends inp to cell,
        // so we attach the datalist as a sibling after inp is in the DOM.
        requestAnimationFrame(() => {
          if (inp.parentElement && !inp.parentElement.querySelector(`#${listId}`)) {
            inp.parentElement.append(datalist);
          }
        });
        return inp;
      }
      // Strict dropdown (on_unknown: "block" or no on_unknown).
      const sel = document.createElement("select");
      const emptyOpt = document.createElement("option");
      emptyOpt.value = "";
      emptyOpt.textContent = `-- ${fieldDef.label} --`;
      sel.append(emptyOpt);
      for (const opt of fieldDef.options ?? []) {
        const o = document.createElement("option");
        o.value = opt;
        o.textContent = opt;
        if (opt === value) o.selected = true;
        sel.append(o);
      }
      if (value && !(fieldDef.options ?? []).includes(value)) {
        // Current value not in options -- show it anyway.
        const o = document.createElement("option");
        o.value = value;
        o.textContent = value;
        o.selected = true;
        sel.append(o);
      }
      return sel;
    }
    case "yes-no": {
      const inp = document.createElement("input");
      inp.type = "checkbox";
      inp.checked = value === "true" || value === "yes" || value === "1";
      // Sync the .value property so callers reading inp.value get a string.
      inp.value = inp.checked ? "yes" : "no";
      inp.addEventListener("change", () => {
        inp.value = inp.checked ? "yes" : "no";
      });
      return inp;
    }
    case "date": {
      return input({ type: "date", value: value.slice(0, 10) });
    }
    case "number": {
      const inp = input({ type: "number", value });
      if (fieldDef.validation?.min != null) inp.min = String(fieldDef.validation.min);
      if (fieldDef.validation?.max != null) inp.max = String(fieldDef.validation.max);
      return inp;
    }
    default: {
      // "string" or fallback
      const inp = input({ type: "text", value });
      if (fieldDef.validation?.maxLength != null) {
        inp.maxLength = fieldDef.validation.maxLength;
      }
      return inp;
    }
  }
}

/** Show/hide validation error messages below a field input. */
function showFieldErrors(
  errEl: HTMLElement,
  fieldInput: HTMLElement,
  errors: ValidationError[],
): void {
  if (errors.length === 0) {
    errEl.style.display = "none";
    errEl.textContent = "";
    fieldInput.style.borderColor = "";
    fieldInput.classList.remove("field--error", "field--warning");
    return;
  }
  const hasError = errors.some((e) => e.severity === "error");
  const color = hasError ? "var(--error, #d32f2f)" : "var(--warn, #ed6c02)";
  fieldInput.style.borderColor = color;
  fieldInput.classList.toggle("field--error", hasError);
  fieldInput.classList.toggle("field--warning", !hasError);
  errEl.style.display = "block";
  errEl.style.color = color;
  errEl.textContent = errors.map((e) => e.message).join("; ");
}
