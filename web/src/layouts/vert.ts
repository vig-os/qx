// Vertical layout: QR on top, 4/4 below. size × 2*size.
//
// Per foundation issue #33 (ADR-017 step 8): renders via the Rust
// WASM façade (`crates/wasm/`). The inline TS encoder
// (`qrcode-generator.ts` + `svg.ts`) has been deleted; there is now
// one canonical encoder/renderer across CLI + FE.

import type { Layout, LayoutDimensions, LayoutOptions } from "../core/types";
import { renderLabelSync, type WasmFormatId } from "../wasm/loader";

const FORMAT: WasmFormatId = "4/4";

export const vertLayout: Layout = {
  id: "vert",
  label: "Vertical",
  description: "QR on top of 4/4 text. Aspect 1:2.",
  measure(opts: LayoutOptions): LayoutDimensions {
    return { widthMm: opts.size, heightMm: 2 * opts.size };
  },
  renderSvg(canonical: string, opts: LayoutOptions): string {
    return renderLabelSync(canonical, "vert", opts.size, FORMAT);
  },
};
