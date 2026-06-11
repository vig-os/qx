import type { Transport } from "./types";
import { httpTransport } from "./http";
import { mockTransport } from "./mock";
import { wasmTransport } from "./wasm";

export type { Transport } from "./types";
export { httpTransport } from "./http";
export { mockTransport } from "./mock";
export { wasmTransport } from "./wasm";
export { partsFixtures, partsDescribe, partsEntities, type Fixtures } from "./fixtures";

export interface TransportEnv {
  VITE_TRANSPORT?: string;
  VITE_API_BASE?: string;
}

/**
 * Select the transport from the build/runtime environment:
 *   VITE_TRANSPORT = mock (default) | http | wasm
 *   VITE_API_BASE  = base URL for http (default: same origin)
 */
export function transportFromEnv(env: TransportEnv = import.meta.env): Transport {
  const kind = env.VITE_TRANSPORT ?? "mock";
  switch (kind) {
    case "mock":
      return mockTransport();
    case "http":
      return httpTransport(env.VITE_API_BASE ?? "");
    case "wasm":
      return wasmTransport();
    default:
      throw new Error(`unknown VITE_TRANSPORT: ${kind} (expected mock | http | wasm)`);
  }
}
