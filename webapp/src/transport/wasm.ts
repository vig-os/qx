// WASM in-process transport (ADR-030 §3): the serverless deploy runs
// the Rust core in the browser. crates/wasm exposes a wasm-bindgen
// façade over the app layer's dispatch; this factory fetches the
// registry snapshot, opens it, and returns a Transport that round-trips
// each Request/Response as JSON through registry_dispatch.
//
// The wasm-pack output (src/wasm-pkg, gitignored) is environment-built
// via `npm run build:wasm`. The package is loaded through
// import.meta.glob, which resolves to an empty map when the pkg has
// not been built — so typecheck and `vite build` succeed without it,
// and the absence only surfaces as a clear runtime error here.

import { err, isResponse } from "../protocol";
import type { Transport } from "./types";

/** The wasm-pack (target web) module surface of crates/wasm. */
interface WasmPkg {
  /** wasm-bindgen init: instantiates the .wasm next to the JS glue. */
  default: (input?: unknown) => Promise<unknown>;
  /** Open a snapshot; returns the part count, throws on malformed input. */
  registry_open: (format: string, text: string, registryName: string) => number;
  /** Dispatch a Request JSON; returns Response JSON, never throws. */
  registry_dispatch: (requestJson: string) => string;
  /** Assert the operator identity (post-OAuth hand-off). */
  registry_set_operator: (id: string, displayName: string) => void;
}

const PKG_PATH = "../wasm-pkg/qx_wasm.js";

async function loadPkg(): Promise<WasmPkg> {
  const candidates = import.meta.glob("../wasm-pkg/qx_wasm.js");
  const load = candidates[PKG_PATH];
  if (!load) {
    throw new Error(
      "wasm transport: src/wasm-pkg is missing — run `npm run build:wasm` " +
        "(wasm-pack over crates/wasm) before building/serving with VITE_TRANSPORT=wasm",
    );
  }
  const pkg = (await load()) as WasmPkg;
  await pkg.default();
  return pkg;
}

/** The env keys the wasm transport consumes (see src/vite-env.d.ts). */
export interface WasmTransportEnv {
  VITE_DATA_URL?: string;
  VITE_DATA_FORMAT?: string;
  VITE_REGISTRY_NAME?: string;
}

export async function wasmTransport(
  env: WasmTransportEnv = import.meta.env,
): Promise<Transport> {
  const pkg = await loadPkg();
  const url = env.VITE_DATA_URL;
  if (!url) {
    throw new Error("wasm transport: VITE_DATA_URL is not set (URL of the registry snapshot)");
  }
  const res = await fetch(url);
  if (!res.ok) {
    throw new Error(`wasm transport: snapshot fetch ${url} failed (HTTP ${res.status})`);
  }
  const text = await res.text();
  const format = env.VITE_DATA_FORMAT ?? "csv";
  // Throws on a malformed snapshot — a wiring failure, not a protocol
  // error, so it propagates as a thrown error per the Transport contract.
  pkg.registry_open(format, text, env.VITE_REGISTRY_NAME ?? url);
  return (req) => {
    const raw = pkg.registry_dispatch(JSON.stringify(req));
    let parsed: unknown;
    try {
      parsed = JSON.parse(raw);
    } catch {
      return Promise.resolve(
        err("Backend", `wasm dispatch returned non-JSON: ${raw.slice(0, 200)}`),
      );
    }
    if (isResponse(parsed)) return Promise.resolve(parsed);
    return Promise.resolve(err("Backend", "wasm dispatch returned a non-protocol shape"));
  };
}
