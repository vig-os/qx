// Horizontal layout: QR left, text right. 2*size × size.
//
// Per foundation issue #33 (ADR-017 step 8): renders via the Rust
// WASM façade (`crates/wasm/`). The inline TS encoder
// (`qrcode-generator.ts` + `svg.ts`) has been deleted.

import type { Layout, LayoutDimensions, LayoutOptions } from "../core/types";
import { renderLabelSync, type WasmFormatId } from "../wasm/loader";
import { resolveFormat, resolveMicro, isDataMatrix, stripText } from "./label-settings";
import { renderDataMatrixSync } from "../wasm/datamatrix-writer";

export const horzLayout: Layout = {
  id: "horz",
  label: "Horizontal",
  description: "QR left of text. Aspect 2:1. Default.",
  measure(opts: LayoutOptions): LayoutDimensions {
    return { widthMm: 2 * opts.size, heightMm: opts.size };
  },
  renderSvg(canonical: string, opts: LayoutOptions): string {
    if (isDataMatrix(opts)) {
      return renderDataMatrixSync(canonical, opts.size, opts.extra?.showText !== false);
    }
    const fmt: WasmFormatId = resolveFormat(opts);
    const svg = renderLabelSync(canonical, "horz", opts.size, fmt, {
      micro: resolveMicro(opts),
    });
    return opts.extra?.showText === false ? stripText(svg) : svg;
  },
};
