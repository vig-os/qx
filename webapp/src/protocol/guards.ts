import type { ErrorKind, ProtocolError, Response } from "./types";

const ERROR_KINDS: readonly string[] = [
  "NotFound",
  "Ambiguous",
  "Validation",
  "Unsupported",
  "Auth",
  "Backend",
  "BadRequest",
];

function isProtocolError(v: unknown): v is ProtocolError {
  if (typeof v !== "object" || v === null) return false;
  const e = v as Record<string, unknown>;
  return (
    typeof e["kind"] === "string" &&
    ERROR_KINDS.includes(e["kind"]) &&
    typeof e["message"] === "string"
  );
}

/** Structural check that an untrusted value is a protocol Response envelope. */
export function isResponse(v: unknown): v is Response {
  if (typeof v !== "object" || v === null) return false;
  const r = v as Record<string, unknown>;
  if (r["ok"] === true) return "data" in r;
  if (r["ok"] === false) return isProtocolError(r["error"]);
  return false;
}

export function ok<T>(data: T): Response<T> {
  return { ok: true, data };
}

export function err(kind: ErrorKind, message: string): Response<never> {
  return { ok: false, error: { kind, message } };
}
