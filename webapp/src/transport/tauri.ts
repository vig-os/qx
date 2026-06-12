// Tauri invoke transport (ADR-030 §3): the desktop/mobile webview
// calls the Rust app layer in-process — invoke("dispatch") lands on
// the desktop shell's one Tauri command, which runs app::dispatch and
// always resolves with the protocol envelope (no HTTP hop).
//
// @tauri-apps/api is imported lazily (same shape as the wasm
// transport) so the module only loads when the build actually selects
// the tauri transport; outside a Tauri webview the invoke itself
// rejects and maps to a Backend-error envelope.

import { err, isResponse } from "../protocol";
import type { Transport } from "./types";

export async function tauriTransport(): Promise<Transport> {
  const { invoke } = await import("@tauri-apps/api/core");
  return async (req) => {
    let raw: unknown;
    try {
      raw = await invoke("dispatch", { request: req });
    } catch (e) {
      return err("Backend", `tauri invoke dispatch failed: ${String(e)}`);
    }
    // The Rust command never rejects for domain failures — those come
    // back inside the envelope; anything non-envelope is a wiring bug.
    if (isResponse(raw)) return raw;
    return err("Backend", "tauri dispatch returned a non-protocol shape");
  };
}
