import type { Request, Response } from "../protocol";

/**
 * The one seam between the UI and the app layer (ADR-030 §3).
 *
 * Every backend — in-process WASM, HTTP `pr serve`, Tauri invoke, the
 * in-memory mock — is a function of this shape. Transports never throw
 * for domain failures; those come back as `{ok: false, error}`. A
 * thrown error means the transport itself is broken/misconfigured.
 */
export type Transport = (req: Request) => Promise<Response>;
