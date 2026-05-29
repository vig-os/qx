// Mint-from-label overlay (#176 P1) — photograph a manufacturer label,
// and create ONE new part (mint + bind) from it.
//
// Design (from #176 review): operator-assisted assignment is the spine,
// regex extraction is a reversible pre-fill on top.
//   - extractFields() pre-fills the bind inputs with a visibly "guessed"
//     style; every value stays editable (a guess must look like a guess).
//   - OCR word tokens render as chips, dimmed by low confidence; tapping
//     a chip appends its text to the focused field — turning transcription
//     into tapping.
//   - Confirm mints a fresh canonical ID and binds the assembled fields.

import { el, button, input } from "./dom";
import { icon } from "./icons";
import { fileToCanvas, recognize, type OcrWord } from "./ocr-engine";
import { extractFields, type ExtractField } from "../registry/ocr-extract";
import { ID_ALPHABET, ID_LENGTH } from "../config";
import { generateId } from "../tabs/mint";
import { addItems, type SessionItem } from "../registry/session";

export interface OcrExtractOptions {
  /** Existing registry IDs — so the freshly-minted ID can't collide. */
  existingIds: ReadonlySet<string>;
}

export interface OcrExtractResult {
  minted: number;
  bound: number;
}

// Bind fields offered on the mint-from-label form (order = display order).
const FORM_FIELDS: { key: ExtractField | "location" | "notes"; label: string }[] = [
  { key: "type", label: "Type" },
  { key: "description", label: "Description" },
  { key: "vendor", label: "Vendor" },
  { key: "part_number", label: "Part number" },
  { key: "manufacturer_id", label: "Manufacturer ID" },
  { key: "location", label: "Location" },
  { key: "notes", label: "Notes" },
];

const LOW_CONFIDENCE = 60; // tesseract confidence below this → dim the chip

function freshId(existing: ReadonlySet<string>): string {
  let guard = 0;
  while (guard < 1000) {
    const id = generateId(ID_ALPHABET, ID_LENGTH);
    guard++;
    if (!existing.has(id)) return id;
  }
  return generateId(ID_ALPHABET, ID_LENGTH); // give up guarding (impossible)
}

export function openOcrExtract(opts: OcrExtractOptions): Promise<OcrExtractResult | null> {
  return new Promise((resolve) => {
    const overlay = el("div", { class: "scan-overlay scan-overlay--image scan-overlay--ocr scan-overlay--mint" });

    // Drop zone.
    const dropZone = el("div", { class: "image-scan__dropzone" });
    dropZone.append(
      el("p", { class: "image-scan__drop-label" }, "Photograph a label to mint a part"),
      el("p", { class: "muted small" }, "Reads the label, pre-fills the fields, you confirm"),
    );
    const fileInput = document.createElement("input");
    fileInput.type = "file";
    fileInput.accept = "image/*";
    fileInput.style.display = "none";
    const chooseBtn = button({ class: "primary" }, icon("upload"), " Choose image");
    chooseBtn.addEventListener("click", () => fileInput.click());
    dropZone.append(fileInput, chooseBtn);

    // Preview + status.
    const imageWrap = el("div", { class: "image-scan__preview" });
    imageWrap.style.display = "none";
    const stillWrap = el("div", { class: "scan-overlay__still" });
    const statusLine = el("div", { class: "scan-overlay__hint" });
    statusLine.style.display = "none";
    imageWrap.append(stillWrap, statusLine);

    // Field form.
    const form = el("div", { class: "ocr-mint__form" });
    form.style.display = "none";
    const inputs = new Map<string, HTMLInputElement>();
    let lastFocused: HTMLInputElement | null = null;
    for (const f of FORM_FIELDS) {
      const row = el("label", { class: "ocr-mint__field" });
      row.append(el("span", { class: "ocr-mint__field-label" }, f.label));
      const inp = input({ type: "text" });
      inp.addEventListener("focus", () => { lastFocused = inp; });
      // Typing in a guessed field clears the "guessed" styling.
      inp.addEventListener("input", () => inp.classList.remove("ocr-mint__guessed"));
      inputs.set(f.key, inp);
      row.append(inp);
      form.append(row);
    }

    // OCR token chips.
    const tokenTray = el("div", { class: "ocr-mint__tokens" });
    tokenTray.style.display = "none";

    // Actions.
    const actions = el("div", { class: "scan-overlay__actions" });
    const confirmBtn = button({ class: "scan-overlay__done primary", disabled: "true" }, icon("plus"), " Mint + bind");
    const retakeBtn = button({}, icon("upload"), " Choose another");
    retakeBtn.style.display = "none";
    const cancelBtn = button({ class: "scan-overlay__cancel" }, icon("x"), " Cancel");
    actions.append(confirmBtn, retakeBtn, cancelBtn);

    const badge = el("div", { class: "scan-overlay__badge" }, "Mint from label");

    overlay.append(dropZone, imageWrap, form, tokenTray, badge, actions);
    document.body.append(overlay);

    let resolved = false;
    const finish = (value: OcrExtractResult | null) => {
      if (resolved) return;
      resolved = true;
      overlay.remove();
      resolve(value);
    };
    cancelBtn.addEventListener("click", () => finish(null));
    overlay.addEventListener("click", (e) => { if (e.target === overlay) finish(null); });

    const renderTokens = (words: OcrWord[]) => {
      tokenTray.innerHTML = "";
      // De-dup tokens, keep best confidence.
      const seen = new Map<string, OcrWord>();
      for (const w of words) {
        const prev = seen.get(w.text);
        if (!prev || w.confidence > prev.confidence) seen.set(w.text, w);
      }
      for (const w of seen.values()) {
        const chip = button(
          { class: `ocr-mint__token${w.confidence < LOW_CONFIDENCE ? " ocr-mint__token--low" : ""}`,
            title: `confidence ${Math.round(w.confidence)}%` },
          w.text,
        );
        chip.addEventListener("click", () => {
          const target = lastFocused ?? firstEmptyInput();
          if (!target) return;
          target.value = target.value ? `${target.value} ${w.text}` : w.text;
          target.classList.remove("ocr-mint__guessed");
          target.focus();
        });
        tokenTray.append(chip);
      }
      tokenTray.style.display = seen.size > 0 ? "" : "none";
    };

    const firstEmptyInput = (): HTMLInputElement | null => {
      for (const inp of inputs.values()) if (!inp.value) return inp;
      return null;
    };

    const updateConfirm = () => {
      // Need at least one non-empty field to bind something meaningful.
      const any = [...inputs.values()].some((i) => i.value.trim());
      confirmBtn.disabled = !any;
    };

    const processFile = async (file: File) => {
      let canvas: HTMLCanvasElement;
      try {
        canvas = await fileToCanvas(file);
      } catch (e) {
        statusLine.textContent = (e as Error).message;
        statusLine.style.display = "";
        return;
      }
      dropZone.style.display = "none";
      imageWrap.style.display = "";
      retakeBtn.style.display = "";
      stillWrap.innerHTML = "";
      canvas.classList.add("scan-overlay__still-canvas");
      stillWrap.append(canvas);
      statusLine.textContent = "Reading label…";
      statusLine.style.display = "";

      let text = "";
      let words: OcrWord[] = [];
      try {
        const res = await recognize(canvas);
        text = res.text;
        words = res.words;
      } catch (e) {
        statusLine.textContent = `OCR failed: ${(e as Error).message}`;
        return;
      }

      // Regex pre-fill (reversible, visibly "guessed").
      for (const s of extractFields(text)) {
        const inp = inputs.get(s.field);
        if (inp && !inp.value) {
          inp.value = s.value;
          inp.classList.add("ocr-mint__guessed");
        }
      }
      for (const inp of inputs.values()) {
        inp.addEventListener("input", updateConfirm);
        inp.addEventListener("change", updateConfirm);
      }

      renderTokens(words);
      form.style.display = "";
      statusLine.textContent = words.length
        ? "Review the pre-filled fields, tap tokens to fill the rest, then confirm."
        : "No text read — fill the fields manually, or choose another image.";
      updateConfirm();
    };

    fileInput.addEventListener("change", () => {
      const f = fileInput.files?.[0];
      if (f) void processFile(f);
    });
    retakeBtn.addEventListener("click", () => fileInput.click());

    confirmBtn.addEventListener("click", async () => {
      confirmBtn.disabled = true;
      const fields: Record<string, string> = {};
      for (const [key, inp] of inputs) {
        const v = inp.value.trim();
        if (v) fields[key] = v;
      }
      const id = freshId(opts.existingIds);
      const now = new Date().toISOString();
      const items: SessionItem[] = [
        { kind: "mint", id, batch: "", notes: "", createdAt: now },
        { kind: "bind", id, fields, createdAt: now },
      ];
      try {
        await addItems(items);
      } catch (e) {
        statusLine.textContent = `Could not save: ${(e as Error).message}`;
        statusLine.style.display = "";
        confirmBtn.disabled = false;
        return;
      }
      finish({ minted: 1, bound: 1 });
    });
  });
}
