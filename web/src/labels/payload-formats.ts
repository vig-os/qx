// Payload format registry — fixed enum of barcode payload shapes.
//
// Each format defines how to render a registry row into a barcode
// payload string, and how to extract the ID back from a scanned
// payload. Adding a new format = one entry here + done.
//
// Design decision (spike #155): fixed enum over user-defined templates.
// Three agent reviews converged: templates are over-engineering for
// 1-3 labs with 5 useful payload shapes. Fixed formats give trivial
// roundtrip decoding, config-time capacity validation, and zero
// injection surface.

import type { RegistryRow } from "../registry/schema";
import { getConfig } from "../config/deploy-config";

export interface PayloadFormat {
  /** Unique key used in config and localStorage. */
  id: string;
  /** Human-readable label for the dropdown. */
  label: string;
  /** QR encoding mode needed — affects capacity calculation. */
  encoding: "alphanumeric" | "byte";
  /**
   * Fixed character overhead beyond the 14-char ID.
   * Used for config-time capacity validation.
   */
  fixedOverhead: number;
  /** If true, payload length varies with registry data (type, vendor). */
  variable: boolean;
  /** Render a barcode payload string from a registry row (or just an ID). */
  render: (id: string, row?: Partial<RegistryRow>) => string;
  /** Extract the canonical ID from a scanned payload. Returns null if no match. */
  extractId: (payload: string) => string | null;
  /** Example payload for the config/preview. */
  example: string;
}

// ---- Format definitions ----

const ID_RE = /^[23456789ABCDEFGHJKMNPQRSTUVWXYZ]{14}$/;

const idOnly: PayloadFormat = {
  id: "id_only",
  label: "Raw ID",
  encoding: "alphanumeric",
  fixedOverhead: 0,
  variable: false,
  render: (id) => id,
  extractId: (payload) => {
    const norm = payload.toUpperCase().replace(/[-\s]/g, "");
    return ID_RE.test(norm) ? norm : null;
  },
  example: "K7M3PQ9RT5VAXY",
};

const prefixedId: PayloadFormat = {
  id: "prefixed_id",
  label: "Prefixed ID (PR:)",
  encoding: "alphanumeric",
  fixedOverhead: 3, // "PR:" = 3 chars
  variable: false,
  render: (id) => `PR:${id}`,
  extractId: (payload) => {
    const upper = payload.toUpperCase();
    // Accept with or without prefix
    if (upper.startsWith("PR:")) {
      const rest = upper.slice(3).replace(/[-\s]/g, "");
      return ID_RE.test(rest) ? rest : null;
    }
    // Fall through to raw ID check
    const norm = upper.replace(/[-\s]/g, "");
    return ID_RE.test(norm) ? norm : null;
  },
  example: "PR:K7M3PQ9RT5VAXY",
};

const idType: PayloadFormat = {
  id: "id_type",
  label: "ID + Type",
  encoding: "byte", // type field may contain lowercase
  fixedOverhead: 1, // ":" separator
  variable: true,
  render: (id, row) => {
    const type = row?.type ?? "";
    return type ? `${id}:${type}` : id;
  },
  extractId: (payload) => {
    const parts = payload.split(":");
    const norm = parts[0].toUpperCase().replace(/[-\s]/g, "");
    return ID_RE.test(norm) ? norm : null;
  },
  example: "K7M3PQ9RT5VAXY:PT100",
};

const urlFormat: PayloadFormat = {
  id: "url",
  label: "Lookup URL",
  encoding: "byte",
  fixedOverhead: 0, // computed dynamically from base URL
  variable: false,
  render: (id) => {
    const cfg = getConfig();
    const base = `https://${cfg.repo.dataRepo.split("/")[0]}.github.io/${cfg.repo.dataRepo.split("/")[1]}`;
    return `${base}/${id}`;
  },
  extractId: (payload) => {
    // Extract trailing 14-char ID from a URL path
    const match = payload.match(/\/([23456789ABCDEFGHJKMNPQRSTUVWXYZ]{14})\/?$/i);
    if (match) return match[1].toUpperCase();
    // Fallback: try raw ID
    const norm = payload.toUpperCase().replace(/[-\s]/g, "");
    return ID_RE.test(norm) ? norm : null;
  },
  example: "https://example.github.io/registry/K7M3PQ9RT5VAXY",
};

// ---- Registry ----

const ALL_FORMATS: PayloadFormat[] = [idOnly, prefixedId, idType, urlFormat];

export function getAllPayloadFormats(): readonly PayloadFormat[] {
  return ALL_FORMATS;
}

export function getPayloadFormat(id: string): PayloadFormat | undefined {
  return ALL_FORMATS.find((f) => f.id === id);
}

/**
 * Try to extract a canonical ID from a scanned payload by trying
 * each registered format's extractor in order. The first match wins.
 * Falls back to raw ID normalization if no format matches.
 */
export function tryExtractId(payload: string): string | null {
  for (const fmt of ALL_FORMATS) {
    const id = fmt.extractId(payload);
    if (id) return id;
  }
  return null;
}

/**
 * Compute the maximum payload length for a format + ID length.
 * For variable formats, adds a buffer for the variable field.
 */
export function maxPayloadLength(
  format: PayloadFormat,
  idLength: number,
  maxVariableFieldLength = 100,
): number {
  if (format.variable) {
    return idLength + format.fixedOverhead + maxVariableFieldLength;
  }
  if (format.id === "url") {
    // URL overhead is dynamic — use a conservative estimate
    return idLength + 50; // https://x.github.io/repo/ ≈ 35-50 chars
  }
  return idLength + format.fixedOverhead;
}
