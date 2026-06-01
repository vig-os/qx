// Popup row editor (PR3). The Bind queue's inline cells are great for quick
// tweaks, but editing a full row across a horizontally-scrolling 11-column
// table is fiddly. This opens the same fields as a roomy vertical form in a
// modal — "edit as a popup, like Lookup". Reuses the shared modal + the same
// combobox / tags-input controls as the inline cells, so behaviour matches.
//
// Operates on a flat values map keyed by field; the caller persists on save.
// metadata/typeFields are intentionally out of scope (edited via the inline
// Properties sub-row) — `fields` should be the json-excluded editable set.

import { openModal } from "./modal";
import { makeCombobox } from "./combobox";
import { makeTagsInput } from "./tags-input";
import { el, button, input } from "../dom";
import { fieldVocabOptions, componentCandidates, stageVocabValue } from "../../registry/vocab";
import { parseComponents } from "../../registry/assembly-graph";
import type { FieldDef } from "../../registry/schema";
import type { AppContext } from "../../core/types";

export interface RowEditorOptions {
  title: string;
  /** Editable field defs (json/metadata excluded). */
  fields: FieldDef[];
  /** Current values keyed by field key. */
  values: Record<string, string>;
  ctx: AppContext;
  /** Pretty-printer for component-ID chips (value stays canonical). */
  fmtId?: (id: string) => string;
  /** Called with the full updated values map when Save is pressed. */
  onSave: (values: Record<string, string>) => void;
}

export function openRowEditor(opts: RowEditorOptions): void {
  const draft: Record<string, string> = { ...opts.values };

  openModal({
    overlayClass: "row-editor-overlay",
    cardClass: "row-editor-card",
    ariaLabel: opts.title,
    body: (close) => {
      const form = el("div", { class: "row-editor" });
      form.append(el("h3", { class: "row-editor__title" }, opts.title));

      for (const f of opts.fields) {
        const key = f.key;
        const value = draft[key] ?? "";
        const field = el("div", { class: "row-editor__field" });
        field.append(el("label", { class: "row-editor__label" }, f.label));

        if (key === "vendor" || key === "location") {
          const cb = makeCombobox({
            value,
            getOptions: () => fieldVocabOptions(opts.ctx, key),
            ariaLabel: f.label,
            onChange: (v, isNew) => {
              draft[key] = v;
              if (isNew) stageVocabValue(key, v);
            },
          });
          field.append(cb.el);
        } else if (key === "components") {
          const tags = makeTagsInput({
            value: parseComponents(value),
            getOptions: () => componentCandidates(opts.ctx),
            formatTag: opts.fmtId,
            ariaLabel: f.label,
            onChange: (vals) => { draft[key] = vals.join(";"); },
          });
          field.append(tags.el);
        } else {
          const inp = input({ type: f.type === "number" ? "number" : "text", value });
          if (f.validation?.maxLength != null) inp.maxLength = f.validation.maxLength;
          inp.addEventListener("input", () => { draft[key] = inp.value; });
          field.append(inp);
        }
        form.append(field);
      }

      const actions = el("div", { class: "row-editor__actions" });
      const cancelBtn = button({ class: "secondary" }, "Cancel");
      cancelBtn.addEventListener("click", close);
      const saveBtn = button({ class: "primary" }, "Save");
      saveBtn.addEventListener("click", () => {
        opts.onSave({ ...draft });
        close();
      });
      actions.append(cancelBtn, saveBtn);
      form.append(actions);
      return form;
    },
  });
}
