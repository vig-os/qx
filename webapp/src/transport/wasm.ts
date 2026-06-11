// WASM in-process transport — DOCUMENTED INTEGRATION POINT, not built yet.
//
// Per ADR-030 §3, the serverless deploy (GitHub Pages) runs the Rust
// core in-process: crates/wasm exposes a wasm-bindgen façade over the
// app layer's dispatch. When that façade exists, this factory awaits
// the wasm module's init, then returns a Transport that serializes
// each Request to JSON, calls the exported dispatch, and parses the
// returned JSON Response. Until then it throws loudly at wiring time
// (not per-request), so a misconfigured build fails fast.

import type { Transport } from "./types";

export function wasmTransport(): Transport {
  throw new Error("wasm transport: crates/wasm dispatch not built yet — see ADR-030 §3");
}
