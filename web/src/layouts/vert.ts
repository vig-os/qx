// Vertical layout: QR on top, text below. size × 2*size.
//
// Per foundation issue #33 (ADR-017 step 8): renders via the Rust
// WASM façade (`crates/wasm/`). The inline TS encoder
// (`qrcode-generator.ts` + `svg.ts`) has been deleted; there is now
// one canonical encoder/renderer across CLI + FE.

import type { Layout, LayoutDimensions, LayoutOptions } from "../core/types";
import { renderLabelSync, type WasmFormatId } from "../wasm/loader";
import { resolveFormat, resolveMicro, stripText } from "./label-settings";

export const vertLayout: Layout = {
  id: "vert",
  label: "Vertical",
  description: "QR on top of text. Aspect 1:2.",
  measure(opts: LayoutOptions): LayoutDimensions {
    return { widthMm: opts.size, heightMm: 2 * opts.size };
  },
  renderSvg(canonical: string, opts: LayoutOptions): string {
    const fmt: WasmFormatId = resolveFormat(opts);
    const svg = renderLabelSync(canonical, "vert", opts.size, fmt, {
      micro: resolveMicro(opts),
    });
    return opts.extra?.showText === false ? stripText(svg) : svg;
  },
};
