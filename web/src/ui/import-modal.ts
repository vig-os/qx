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
import { addMint, addBind } from "../registry/session";
import {
  parseDelimited,
  autoDetectMapping,
  buildImportedRows,
  targetOptions,
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
    commitBtn.addEventListener("click", async () => {
      if (!table) return;
      commitBtn.disabled = true;
      const rows = buildImportedRows(table, mapping);
      const toMint = rows.filter((r) => r.mint);
      const freshIds = mintUniqueIds(toMint.length, opts.existingIds);

      let mintI = 0;
      let minted = 0;
      let bound = 0;
      for (const r of rows) {
        let id = r.id;
        if (r.mint) {
          id = freshIds[mintI++];
          if (!id) continue; // ran out of unique IDs (impossible in practice)
          await addMint(id, r.batch, "");
          minted++;
        }
        await addBind(id, r.fields);
        bound++;
      }

      document.removeEventListener("keydown", onEsc);
      overlay.remove();
      resolve({ minted, bound });
    });
  });
}
