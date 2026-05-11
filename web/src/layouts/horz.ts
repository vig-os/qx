// Horizontal layout: QR left, 4/4 right. 2*size × size.
//
// Per foundation issue #33 (ADR-017 step 8): renders via the Rust
// WASM façade (`crates/wasm/`). The inline TS encoder
// (`qrcode-generator.ts` + `svg.ts`) has been deleted.

import type { Layout, LayoutDimensions, LayoutOptions } from "../core/types";
import { renderLabelSync, type WasmFormatId } from "../wasm/loader";

const FORMAT: WasmFormatId = "4/4";

export const horzLayout: Layout = {
  id: "horz",
  label: "Horizontal",
  description: "QR left of 4/4 text. Aspect 2:1. Default.",
  measure(opts: LayoutOptions): LayoutDimensions {
    return { widthMm: 2 * opts.size, heightMm: opts.size };
  },
  renderSvg(canonical: string, opts: LayoutOptions): string {
    return renderLabelSync(canonical, "horz", opts.size, FORMAT);
  },
};
