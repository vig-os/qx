// OCR text scan (#171 P2) — read a manufacturer label or a plain-
// printed canonical ID from a photo, match it against the registry,
// and surface the matched parts with the same chip UI as QR scans.
//
// tesseract.js is lazy-loaded on first use (its worker/core/lang load
// from CDN, then the service worker caches them for offline reuse).

import { el, button } from "./dom";
import { icon } from "./icons";
import { formatIdDashed, type ScanStatus } from "./scanner";
import { fileToCanvas, recognize } from "./ocr-engine";
import { matchOcrText, type OcrMatch } from "../registry/ocr-match";
import type { RegistryRow } from "../registry/schema";

export interface OcrScanOptions {
  /** Registry rows to match OCR text against (id / manufacturer_id /
   *  part_number). */
  rows: ReadonlyArray<RegistryRow>;
  /** Per-match status for chip colour-coding. */
  resolveStatus?: (canonical: string) => ScanStatus;
}

const VIA_LABEL: Record<OcrMatch["via"], string> = {
  id: "ID",
  manufacturer_id: "mfr",
  part_number: "part#",
};

/**
 * Open the OCR scan overlay. Resolves with the canonical IDs the
 * operator selected, or rejects if cancelled.
 */
export function openOcrScan(opts: OcrScanOptions): Promise<string[]> {
  return new Promise<string[]>((resolve, reject) => {
    let resolved = false;
    const selected = new Map<string, OcrMatch>();

    const overlay = el("div", { class: "scan-overlay scan-overlay--image scan-overlay--ocr" });

    // Drop zone.
    const dropZone = el("div", { class: "image-scan__dropzone" });
    dropZone.append(
      el("p", { class: "image-scan__drop-label" }, "Photograph a label"),
      el("p", { class: "muted small" }, "Reads manufacturer IDs or plain-printed part IDs"),
    );
    const fileInput = document.createElement("input");
    fileInput.type = "file";
    fileInput.accept = "image/*";
    fileInput.style.display = "none";
    const chooseBtn = button({ class: "primary" }, icon("upload"), " Choose image");
    chooseBtn.addEventListener("click", () => fileInput.click());
    dropZone.append(chooseBtn, fileInput);

    // Preview + status.
    const imageWrap = el("div", { class: "image-scan__preview" });
    imageWrap.style.display = "none";
    const stillWrap = el("div", { class: "scan-overlay__still" });
    const statusLine = el("div", { class: "scan-overlay__hint" });
    statusLine.style.display = "none";
    const textPreview = el("pre", { class: "ocr-text-preview" });
    textPreview.style.display = "none";
    imageWrap.append(stillWrap, statusLine, textPreview);

    // Chip tray.
    const chipTray = el("div", { class: "scan-overlay__chips" });

    // Actions.
    const actions = el("div", { class: "scan-overlay__actions" });
    const doneBtn = button({ class: "scan-overlay__done primary" }, icon("plus"), " Add all to queue");
    doneBtn.disabled = true;
    const retakeBtn = button({}, icon("upload"), " Choose another");
    retakeBtn.style.display = "none";
    const cancelBtn = button({ class: "scan-overlay__cancel" }, icon("x"), " Cancel");
    actions.append(doneBtn, retakeBtn, cancelBtn);

    const badge = el("div", { class: "scan-overlay__badge" }, "OCR text scan");

    overlay.append(
      dropZone,
      imageWrap,
      el("div", { class: "scan-overlay__chips-slot" }, chipTray),
      badge,
      actions,
    );
    document.body.append(overlay);

    const finish = (err: Error | null, value?: string[]) => {
      if (resolved) return;
      resolved = true;
      overlay.remove();
      if (err) reject(err);
      else resolve(value ?? []);
    };

    cancelBtn.addEventListener("click", () => finish(new Error("ocr scan cancelled")));
    doneBtn.addEventListener("click", () => finish(null, [...selected.keys()]));

    const renderChips = () => {
      chipTray.innerHTML = "";
      for (const [id, m] of selected) {
        const status: ScanStatus = opts.resolveStatus ? opts.resolveStatus(id) : "unbound";
        const chip = el("span", { class: `scan-chip scan-chip--${status}` });
        chip.append(
          document.createTextNode(formatIdDashed(id)),
          el("span", { class: "scan-chip__via" }, ` ${VIA_LABEL[m.via]}`),
        );
        const removeBtn = button(
          { class: "scan-chip__remove", title: "Remove" },
          icon("x", { size: 12 }),
        );
        removeBtn.addEventListener("click", (e) => {
          e.stopPropagation();
          selected.delete(id);
          renderChips();
        });
        chip.append(removeBtn);
        chipTray.append(chip);
      }
      doneBtn.disabled = selected.size === 0;
      doneBtn.textContent = "";
      doneBtn.append(icon("plus"), ` Add ${selected.size || "all"} to queue`);
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

      statusLine.textContent = "Reading text…";
      statusLine.style.display = "";

      let text = "";
      try {
        text = (await recognize(canvas)).text;
      } catch (e) {
        statusLine.textContent = `OCR failed: ${(e as Error).message}`;
        return;
      }

      const matches = matchOcrText(text, opts.rows);
      for (const m of matches) selected.set(m.id, m);
      renderChips();

      statusLine.textContent =
        matches.length === 0
          ? "No registry parts matched the text."
          : `Matched ${matches.length} part(s).`;

      // Show the raw OCR text for transparency / debugging.
      textPreview.textContent = text.trim();
      textPreview.style.display = text.trim() ? "" : "none";
    };

    fileInput.addEventListener("change", () => {
      const f = fileInput.files?.[0];
      if (f) void processFile(f);
    });
    retakeBtn.addEventListener("click", () => fileInput.click());

    renderChips();
  });
}
