// HTTP transport: POST {base}/api/dispatch against `pr serve`
// (ADR-030 §2/§3), JSON in / protocol Response envelope out.

import { err, isResponse } from "../protocol";
import type { Transport } from "./types";

export function httpTransport(baseUrl: string): Transport {
  const endpoint = `${baseUrl.replace(/\/+$/, "")}/api/dispatch`;
  return async (req) => {
    let body: unknown;
    let status: number;
    try {
      const res = await fetch(endpoint, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(req),
      });
      status = res.status;
      body = await res.json().catch(() => undefined);
    } catch (e) {
      return err("Backend", `dispatch to ${endpoint} failed: ${String(e)}`);
    }
    // The server speaks the envelope even for errors; trust any
    // well-formed envelope regardless of HTTP status.
    if (isResponse(body)) return body;
    return err("Backend", `non-protocol response from ${endpoint} (HTTP ${status})`);
  };
}
