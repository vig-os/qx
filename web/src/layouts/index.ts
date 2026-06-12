// Layout registry. Open/Closed: add a new layout file and register here.
//
// !!! DRIFT WARNING !!!
//
// These layouts are a TypeScript port of the label renderer. The SSOT
// is the Rust codec (`crates/codec`, per ADR-017; the legacy Python
// label.py was deleted in step 9). The long-term solution is the WASM
// façade (`crates/wasm`) so FE and CLI run literally the same code.
// Until that swap lands, drift between this file and `crates/codec`
// is a real risk; `crates/cli/tests/label_parity_golden.rs` is the
// canonical correctness gate, and any rule change here must be
// mirrored to the Rust codec + retested.

import type { Layout } from "../core/types";
import { vertLayout } from "./vert";
import { horzLayout } from "./horz";
import { flagLayout } from "./flag";

const LAYOUTS: Record<string, Layout> = {};

export function registerLayout(layout: Layout): void {
  LAYOUTS[layout.id] = layout;
}

export function getLayout(id: string): Layout | undefined {
  return LAYOUTS[id];
}

export function allLayouts(): Layout[] {
  return Object.values(LAYOUTS);
}

// Bootstrap: register the built-in layouts.
registerLayout(vertLayout);
registerLayout(horzLayout);
registerLayout(flagLayout);
