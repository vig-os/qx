// Bulk import modal (#176 P0) — paste a TSV/CSV or upload a file, map
// columns to registry fields, review the mint-vs-bind classification,
// then commit each row as a mint+bind (or bind-only) into the session
// queue. The queue itself is the editable preview / validation surface
// (per #176 review consolidation) — this modal is the pre-parse mapping
// step, nothing more.

import { el, button, select } from "./dom";
import { icon } from "./icons";
import { formatIdDashed } from "./scanner";
import { ID_ALPHABET, ID_LENGTH } from "../config";
import { generateId } from "../tabs/mint";
import { addItems, type SessionItem } from "../registry/session";
import {
  parseDelimited,
  autoDetectMapping,
  buildImportedRows,
  targetOptions,
  parseTargetValue,
  type ParsedTable,
} from "../registry/csv-import";

export interface ImportModalOptions {
  /** Existing registry IDs — to guarantee freshly-minted IDs don't collide. */
  existingIds: ReadonlySet<string>;
}

export interface ImportResult {
  minted: number;
  bound: number;
}

/** Generate `count` canonical IDs guaranteed unique vs. `existing` and
 *  each other (collision-safe even though the 14-char space makes it
 *  near-impossible). */
function mintUniqueIds(count: number, existing: ReadonlySet<string>): string[] {
  const used = new Set(existing);
  const out: string[] = [];
  let guard = 0;
  while (out.length < count && guard < count * 50 + 100) {
    const id = generateId(ID_ALPHABET, ID_LENGTH);
    guard++;
    if (used.has(id)) continue;
    used.add(id);
    out.push(id);
  }
  return out;
}

export function openImportModal(opts: ImportModalOptions): Promise<ImportResult | null> {
  return new Promise((resolve) => {
    const overlay = el("div", { class: "import-modal-overlay" });
    const modal = el("div", {
      class: "import-modal",
      role: "dialog",
      "aria-modal": "true",
      "aria-labelledby": "import-modal-title",
    });

    const onEsc = (e: KeyboardEvent) => { if (e.key === "Escape") cancel(); };
    const cancel = () => {
      document.removeEventListener("keydown", onEsc);
      overlay.remove();
      resolve(null);
    };

    const closeBtn = button({ class: "import-modal__close icon-only", title: "Cancel" }, icon("x"));
    closeBtn.addEventListener("click", cancel);

    const header = el("div", { class: "import-modal__header" },
      icon("upload", { size: 22 }),
      el("h3", { id: "import-modal-title" }, "Import parts from a list"));

    // ---- Step 1: paste / upload ----
    const step1 = el("div", { class: "import-modal__step" });
    const textarea = document.createElement("textarea");
    textarea.className = "import-modal__textarea";
    textarea.placeholder =
      "Paste a CSV or TSV here (with a header row), e.g.\n" +
      "vendor\tpart_number\tlocation\nOmega\t402-141\tLab-1";
    textarea.rows = 6;
    const fileInput = document.createElement("input");
    fileInput.type = "file";
    fileInput.accept = ".csv,.tsv,.txt,text/csv,text/tab-separated-values,text/plain";
    fileInput.style.display = "none";
    const fileBtn = button({ class: "secondary" }, icon("upload"), " Choose file");
    fileBtn.addEventListener("click", () => fileInput.click());
    const parseBtn = button({ class: "primary" }, " Parse");
    step1.append(
      el("p", { class: "muted small" }, "Paste tabular data or choose a file. The first row must be column headers."),
      textarea,
      // fileInput is hidden but must be IN the DOM — fileBtn triggers it
      // via .click(), and a detached input can't be driven reliably.
      fileInput,
      el("div", { class: "form-row" }, fileBtn, parseBtn),
    );

    // ---- Step 2: mapping + preview (built on parse) ----
    const step2 = el("div", { class: "import-modal__step" });
    step2.style.display = "none";

    const actions = el("div", { class: "import-modal__actions" });
    const backBtn = button({}, "Back");
    const commitBtn = button({ class: "primary" }, icon("plus"), " Add to queue");
    actions.append(commitBtn, backBtn);
    actions.style.display = "none";

    modal.append(closeBtn, header, step1, step2, actions);
    overlay.append(modal);
    overlay.addEventListener("click", (e) => { if (e.target === overlay) cancel(); });
    document.addEventListener("keydown", onEsc);
    document.body.append(overlay);
    textarea.focus();

    // ---- Parse handler ----
    let table: ParsedTable | null = null;
    let mapping: string[] = [];

    const doParse = (text: string) => {
      const parsed = parseDelimited(text);
      if (parsed.headers.length === 0 || parsed.rows.length === 0) {
        step1.querySelector(".import-modal__parse-error")?.remove();
        const err = el("p", { class: "import-modal__parse-error" },
          "Couldn't find a header row plus at least one data row.");
        step1.append(err);
        return;
      }
      table = parsed;
      mapping = autoDetectMapping(parsed.headers);
      renderStep2();
      step1.style.display = "none";
      step2.style.display = "";
      actions.style.display = "";
    };

    parseBtn.addEventListener("click", () => doParse(textarea.value));
    fileInput.addEventListener("change", () => {
      const f = fileInput.files?.[0];
      if (!f) return;
      void f.text().then((t) => { textarea.value = t; doParse(t); });
    });

    backBtn.addEventListener("click", () => {
      step2.style.display = "none";
      actions.style.display = "none";
      step1.style.display = "";
    });

    const opts2 = targetOptions();

    const renderStep2 = () => {
      if (!table) return;
      step2.innerHTML = "";

      // Mapping table: one row per source column.
      step2.append(el("h4", {}, "Map columns"));
      const mapTable = el("table", { class: "import-map" });
      const thead = el("thead");
      thead.append(el("tr", {},
        el("th", {}, "Source column"),
        el("th", {}, "Sample"),
        el("th", {}, "→ Registry field")));
      mapTable.append(thead);
      const tbody = el("tbody");
      for (let c = 0; c < table.headers.length; c++) {
        const tr = el("tr");
        tr.append(el("td", { class: "import-map__src" }, table.headers[c] || `(column ${c + 1})`));
        const sample = table.rows[0]?.[c] ?? "";
        tr.append(el("td", { class: "import-map__sample muted" }, sample || "—"));
        const sel = select(opts2.map((o) => ({ value: o.value, label: o.label })));
        sel.value = mapping[c];
        sel.addEventListener("change", () => { mapping[c] = sel.value; renderSummary(); });
        tr.append(el("td", {}, sel));
        tbody.append(tr);
      }
      mapTable.append(tbody);
      step2.append(mapTable);

      const summary = el("div", { class: "import-modal__summary" });
      step2.append(summary);
      renderSummary();

      function renderSummary() {
        if (!table) return;
        const rows = buildImportedRows(table, mapping);
        const mints = rows.filter((r) => r.mint).length;
        const binds = rows.length - mints;
        const mappedCols = mapping.filter((m) => m !== "ignore").length;
        summary.innerHTML = "";
        summary.append(
          el("p", {},
            `${rows.length} row(s): `,
            el("strong", {}, `${mints} new (mint+bind)`),
            mints && binds ? ", " : "",
            binds ? el("strong", {}, `${binds} existing (bind-only)`) : "",
          ),
        );
        if (mappedCols === 0) {
          summary.append(el("p", { class: "import-modal__warn" },
            "No columns mapped — nothing will be imported. Map at least one field."));
          commitBtn.disabled = true;
        } else {
          commitBtn.disabled = false;
        }

        // Ragged-row warning (silent column drop/gain).
        if (table.raggedRows > 0) {
          summary.append(el("p", { class: "import-modal__warn" },
            `${table.raggedRows} row(s) had a different column count than the header — ` +
            "extra columns were dropped and missing ones left blank. Check the source data."));
        }

        // Duplicate mapping-target warning (last column wins).
        const dupTargets = duplicateTargets(mapping);
        if (dupTargets.length > 0) {
          summary.append(el("p", { class: "import-modal__warn" },
            `Multiple columns map to: ${dupTargets.join(", ")}. The right-most column wins; ` +
            "set duplicates to “Ignore” if that's not intended."));
        }

        // Orphan bind-only warning: a valid-looking ID not in the registry.
        const orphans = rows.filter(
          (r) => !r.mint && r.id && !opts.existingIds.has(r.id),
        ).length;
        if (orphans > 0) {
          summary.append(el("p", { class: "import-modal__warn" },
            `${orphans} bind-only row(s) reference an ID not in the loaded registry — ` +
            "they'll be flagged in the queue and block submit until resolved."));
        }
        // First-rows preview of the resulting IDs / bind-vs-mint.
        const preview = el("div", { class: "import-preview" });
        for (const r of rows.slice(0, 8)) {
          const chip = el("span", { class: `import-preview__chip import-preview__chip--${r.mint ? "mint" : "bind"}` });
          chip.append(r.mint ? "mint → " : `${formatIdDashed(r.id)} `,
            el("span", { class: "muted" }, Object.keys(r.fields).filter((k) => k !== "metadata").join(", ") || (r.fields.metadata ? "properties" : "—")));
          preview.append(chip);
        }
        if (rows.length > 8) preview.append(el("span", { class: "muted small" }, `+${rows.length - 8} more`));
        summary.append(preview);
      }
    };

    // ---- Commit ----
    const commitError = el("p", { class: "import-modal__parse-error" });
    commitError.style.display = "none";
    actions.before(commitError);

    commitBtn.addEventListener("click", async () => {
      if (!table) return;
      commitBtn.disabled = true;
      commitError.style.display = "none";

      const rows = buildImportedRows(table, mapping);
      const toMint = rows.filter((r) => r.mint);
      const freshIds = mintUniqueIds(toMint.length, opts.existingIds);
      if (freshIds.length < toMint.length) {
        commitError.textContent = "Could not generate enough unique IDs — please retry.";
        commitError.style.display = "";
        commitBtn.disabled = false;
        return;
      }

      // Build all session items up front, then commit in ONE batched
      // write (addItems) — avoids the O(n²) per-row read-modify-write and
      // gives all-or-nothing semantics (no half-populated queue).
      const now = new Date().toISOString();
      const items: SessionItem[] = [];
      let mintI = 0;
      let minted = 0;
      let bound = 0;
      for (const r of rows) {
        let id = r.id;
        if (r.mint) {
          id = freshIds[mintI++];
          items.push({ kind: "mint", id, batch: r.batch, notes: "", createdAt: now });
          minted++;
        }
        items.push({ kind: "bind", id, fields: r.fields, createdAt: now });
        bound++;
      }

      // Single batched write (one read-modify-write). saveSession writes
      // localStorage first and swallows IndexedDB errors, so this throws
      // only on an unexpected failure (e.g. storage quota) — surface it
      // and let the operator retry rather than silently losing the import.
      try {
        await addItems(items);
      } catch (e) {
        commitError.textContent = `Import failed: ${(e as Error).message}. Please retry.`;
        commitError.style.display = "";
        commitBtn.disabled = false;
        return;
      }

      document.removeEventListener("keydown", onEsc);
      overlay.remove();
      resolve({ minted, bound });
    });
  });
}

/** Mapping-target values (excluding "ignore") that appear more than
 *  once, as human labels. */
function duplicateTargets(mapping: string[]): string[] {
  const counts = new Map<string, number>();
  for (const v of mapping) {
    if (v === "ignore") continue;
    counts.set(v, (counts.get(v) ?? 0) + 1);
  }
  const dups: string[] = [];
  for (const [v, n] of counts) {
    if (n > 1) {
      const t = parseTargetValue(v);
      dups.push(t.kind === "ignore" ? v : t.key);
    }
  }
  return dups;
}
