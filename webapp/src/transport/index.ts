import type { Transport } from "./types";
import { httpTransport } from "./http";
import { mockTransport } from "./mock";
import { wasmTransport } from "./wasm";

export type { Transport } from "./types";
export { httpTransport } from "./http";
export { mockTransport } from "./mock";
export { wasmTransport, type WasmTransportEnv } from "./wasm";
export { partsFixtures, partsDescribe, partsEntities, type Fixtures } from "./fixtures";

export interface TransportEnv {
  VITE_TRANSPORT?: string;
  VITE_API_BASE?: string;
  VITE_DATA_URL?: string;
  VITE_DATA_FORMAT?: string;
  VITE_REGISTRY_NAME?: string;
}

/**
 * Select the transport from the build/runtime environment:
 *   VITE_TRANSPORT     = mock (default) | http | wasm
 *   VITE_API_BASE      = base URL for http (default: same origin)
 *   VITE_DATA_URL      = snapshot URL for wasm (required)
 *   VITE_DATA_FORMAT   = snapshot format for wasm (default: csv)
 *   VITE_REGISTRY_NAME = display name for wasm (default: the data URL)
 */
export function transportFromEnv(env: TransportEnv = import.meta.env): Transport {
  const kind = env.VITE_TRANSPORT ?? "mock";
  switch (kind) {
    case "mock":
      return mockTransport();
    case "http":
      return httpTransport(env.VITE_API_BASE ?? "");
    case "wasm": {
      // wasmTransport is async — it imports the pkg, fetches the
      // snapshot, and opens the registry — so wrap it lazily to keep
      // this selector synchronous. A failed init rejects every
      // request with the same wiring error.
      const ready = wasmTransport(env);
      void ready.catch(() => undefined);
      return async (req) => (await ready)(req);
    }
    default:
      throw new Error(`unknown VITE_TRANSPORT: ${kind} (expected mock | http | wasm)`);
  }
}
