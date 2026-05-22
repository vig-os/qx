// Global label settings shared across all layouts.
//
// The Print tab stores user-chosen code-type, text-format, and
// show-text preferences in localStorage. Layouts read them from
// `opts.extra` (injected by the print tab) or fall back to defaults.
//
// This module also provides `stripText()` for layouts that need to
// render QR-only labels (showText = false).

import type { LayoutOptions } from "../core/types";
import type { WasmFormatId } from "../wasm/loader";

// ---- localStorage keys ----

const KEY_CODE_TYPE = "part-registry.label.codeType";
const KEY_FORMAT = "part-registry.label.format";
const KEY_SHOW_TEXT = "part-registry.label.showText";

export type CodeType = "standard" | "micro" | "datamatrix";
export type FormatSetting = "auto" | WasmFormatId;

// ---- Persistence ----

export function loadLabelSettings(): {
  codeType: CodeType;
  format: FormatSetting;
  showText: boolean;
} {
  return {
    codeType: (localStorage.getItem(KEY_CODE_TYPE) as CodeType) || "standard",
    format: (localStorage.getItem(KEY_FORMAT) as FormatSetting) || "auto",
    showText: localStorage.getItem(KEY_SHOW_TEXT) !== "false",
  };
}

export function saveLabelSettings(settings: {
  codeType: CodeType;
  format: FormatSetting;
  showText: boolean;
}): void {
  localStorage.setItem(KEY_CODE_TYPE, settings.codeType);
  localStorage.setItem(KEY_FORMAT, settings.format);
  localStorage.setItem(KEY_SHOW_TEXT, String(settings.showText));
}

// ---- Helpers called from layout renderSvg ----

/** Resolve the WasmFormatId from opts.extra or the auto-recommendation. */
export function resolveFormat(opts: LayoutOptions): WasmFormatId {
  const fmt = opts.extra?.format as string | undefined;
  if (fmt === "4/4" || fmt === "4/4/4" || fmt === "5/5/4") return fmt;
  // Auto: recommend based on size
  if (opts.size >= 10) return "4/4/4";
  return "4/4";
}

/** Resolve the micro flag from opts.extra. */
export function resolveMicro(opts: LayoutOptions): boolean {
  return opts.extra?.micro === true;
}

/** Check if the code type is DataMatrix. */
export function isDataMatrix(opts: LayoutOptions): boolean {
  return opts.extra?.micro === "datamatrix" ||
    (typeof opts.extra?.codeType === "string" && opts.extra.codeType === "datamatrix");
}

/**
 * Strip `<text>` elements from an SVG string for QR-only output.
 * Simple regex-based removal — the codec's SVG output is machine-
 * generated and predictable: `<text ...>...</text>` on a single line.
 */
export function stripText(svg: string): string {
  return svg.replace(/<text[^>]*>.*?<\/text>/g, "");
}
