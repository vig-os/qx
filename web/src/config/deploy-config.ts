// Deploy config loader — typed, validated, with runtime override support.
//
// The deploy config is the SSoT for all deployment-specific settings
// that an admin configures per-deployment. It is NOT the data schema
// (that's registry-contract.json) and NOT user preferences (that's
// localStorage).
//
// Loaded at build time via Vite alias. Runtime override via
// window.__PART_REGISTRY_CONFIG__ for standalone app deployments.

import DEPLOY_CONFIG_DEFAULT from "@deploy-config";
import CODE_TYPES from "@code-types";

// ---- Types ----

export interface CodeTypeEntry {
  id: string;
  displayLabel: string;
  scannerFormat: string;
  encoderFamily: string;
  ecLevels: string[];
  defaultEcLevel: string;
  minModuleSizeMm: number;
  minPayloadAlphanumeric: number;
  quietZoneModules: number;
  description: string;
}

export interface DeployConfig {
  repo: {
    dataRepo: string;
    codeRepo: string;
    defaultBranch: string;
  };
  labels: {
    allowedCodeTypes: string[];
    defaultCodeType: string;
    defaultEcLevel: string;
    defaultSizeMm: number;
    allowedLayouts: string[];
    defaultLayout: string;
    allowedTextFormats: string[];
    defaultTextFormat: string;
    allowedPayloadFormats: string[];
    defaultPayloadFormat: string;
    printerDpi: number;
  };
  scanner: {
    allowedFormats: string[];
  };
  print: {
    defaultOutputMode: string;
    defaultCopies: number;
  };
  features: {
    enableScanner: boolean;
    enableMintTab: boolean;
    enablePrintTab: boolean;
    enableBindTab: boolean;
    enablePrSubmission: boolean;
  };
  presentation: {
    appTitle: string;
    defaultTab: string;
    defaultTheme: string;
  };
  auth: {
    mode: string;
  };
  validation: {
    strictMode: boolean;
  };
}

// ---- Runtime override support ----

declare global {
  interface Window {
    __PART_REGISTRY_CONFIG__?: Partial<DeployConfig>;
  }
}

// ---- Singleton ----

let _config: DeployConfig | null = null;
let _codeTypes: CodeTypeEntry[] | null = null;

function deepMerge<T extends Record<string, unknown>>(base: T, override: Partial<T>): T {
  const result = { ...base };
  for (const key of Object.keys(override) as (keyof T)[]) {
    const val = override[key];
    if (val !== undefined && val !== null && typeof val === "object" && !Array.isArray(val)) {
      result[key] = deepMerge(
        (result[key] ?? {}) as Record<string, unknown>,
        val as Record<string, unknown>,
      ) as T[keyof T];
    } else if (val !== undefined) {
      result[key] = val as T[keyof T];
    }
  }
  return result;
}

/** Load the deploy config (cached). Merges runtime override if present. */
export function getConfig(): DeployConfig {
  if (_config) return _config;
  const base = DEPLOY_CONFIG_DEFAULT as unknown as DeployConfig;
  const override = typeof window !== "undefined"
    ? window.__PART_REGISTRY_CONFIG__
    : undefined;
  _config = override
    ? deepMerge(base as unknown as Record<string, unknown>, override as unknown as Record<string, unknown>) as unknown as DeployConfig
    : base;
  return _config;
}

/** All known code types from the SSoT array. */
export function getAllCodeTypes(): readonly CodeTypeEntry[] {
  if (_codeTypes) return _codeTypes;
  _codeTypes = CODE_TYPES as unknown as CodeTypeEntry[];
  return _codeTypes;
}

/** Code types allowed for printing in this deployment. */
export function getAllowedPrintCodeTypes(): CodeTypeEntry[] {
  const config = getConfig();
  const allowed = new Set(config.labels.allowedCodeTypes);
  return getAllCodeTypes().filter((ct) => allowed.has(ct.id));
}

/** Code types allowed for scanning in this deployment. */
export function getAllowedScanFormats(): string[] {
  return getConfig().scanner.allowedFormats;
}

/** Validate that a code type can hold the given payload length. */
export function validateCapacity(codeTypeId: string, payloadLength: number): string | null {
  const ct = getAllCodeTypes().find((c) => c.id === codeTypeId);
  if (!ct) return `Unknown code type: ${codeTypeId}`;
  if (payloadLength > ct.minPayloadAlphanumeric) {
    return `${ct.displayLabel} can hold max ${ct.minPayloadAlphanumeric} alphanumeric characters; ${payloadLength} required`;
  }
  return null;
}

// ---- Unit conversion (replaces hardcoded PX_TO_MM / TAPE_SIZES) ----

/** Convert a size value from the given unit to mm using the configured DPI. */
export function toMm(value: number, unit: "mm" | "pt" | "px"): number {
  const dpi = getConfig().labels.printerDpi;
  switch (unit) {
    case "mm": return value;
    case "pt": return value * 25.4 / 72;     // 1 pt = 1/72 inch
    case "px": return value * 25.4 / dpi;     // 1 px = 1/dpi inch
  }
}

/** Convert mm to the given unit. */
export function fromMm(mm: number, unit: "mm" | "pt" | "px"): number {
  const dpi = getConfig().labels.printerDpi;
  switch (unit) {
    case "mm": return mm;
    case "pt": return mm * 72 / 25.4;
    case "px": return mm * dpi / 25.4;
  }
}
