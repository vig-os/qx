// Flag layout: horz mirrored across a cable wrap zone. Folded around
// a cable to make a double-sided readable flag.
//
// Per foundation issue #33 (ADR-017 step 8): renders via the Rust
// WASM façade (`crates/wasm/`). The inline TS encoder
// (`qrcode-generator.ts` + `svg.ts`) has been deleted.

import type {
  Layout,
  LayoutDimensions,
  LayoutOptions,
  LayoutOptionField,
} from "../core/types";
import { renderLabelSync, type WasmFormatId } from "../wasm/loader";
import { resolveFormat, resolveMicro, isDataMatrix, stripText } from "./label-settings";
import { renderDataMatrixSync } from "../wasm/datamatrix-writer";

const DEFAULT_CABLE_OD_MM = 6;

function cableOd(opts: LayoutOptions): number {
  const v = opts.extra?.cableOd;
  if (typeof v === "number" && v > 0) return v;
  return DEFAULT_CABLE_OD_MM;
}

export const flagLayout: Layout = {
  id: "flag",
  label: "Flag (cable wrap)",
  description:
    "Two horz halves mirrored across a wrap zone. Wraps around a cable so the flag is readable from both sides.",
  measure(opts: LayoutOptions): LayoutDimensions {
    const s = opts.size;
    const wrap = Math.PI * cableOd(opts) * 1.1;
    return { widthMm: 4 * s + wrap, heightMm: s };
  },
  renderSvg(canonical: string, opts: LayoutOptions): string {
    if (isDataMatrix(opts)) {
      // Flag layout with DataMatrix falls back to horizontal — the
      // cable-wrap mirroring is QR-specific geometry.
      return renderDataMatrixSync(canonical, opts.size, opts.extra?.showText !== false);
    }
    const fmt: WasmFormatId = resolveFormat(opts);
    const svg = renderLabelSync(canonical, "flag", opts.size, fmt, {
      micro: resolveMicro(opts),
      cableOdMm: cableOd(opts),
      noMarkers: Boolean(opts.extra?.noMarkers),
      alignmentLine: Boolean(opts.extra?.alignmentLine),
    });
    return opts.extra?.showText === false ? stripText(svg) : svg;
  },
  optionFields(): LayoutOptionField[] {
    return [
      {
        key: "cableOd",
        label: "Cable OD (mm)",
        type: "number",
        default: DEFAULT_CABLE_OD_MM,
        min: 1,
        max: 50,
        step: 0.5,
      },
      {
        key: "noMarkers",
        label: "No markers",
        type: "checkbox",
        default: 0,
      },
      {
        key: "alignmentLine",
        label: "Alignment line",
        type: "checkbox",
        default: 0,
      },
    ];
  },
};
