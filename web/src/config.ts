// App constants — derived from deploy-config.json (SSoT for deployment
// settings) and registry-contract.json (SSoT for data schema).
//
// This file is the bridge: it reads both configs and exports the
// flat constants that the rest of the FE imports. No other file
// should import from @deploy-config or @code-types directly — go
// through this module or config/deploy-config.ts.

import { REGISTRY_CONTRACT } from "./registry/contract";
import { getConfig } from "./config/deploy-config";

const cfg = getConfig();

// ---- Repository ----
// Env vars override deploy-config for repo settings (12-factor: env > file).
export const CODE_REPO_SLUG: string =
  import.meta.env.VITE_CODE_REPO ?? cfg.repo.codeRepo;
export const DATA_REPO_SLUG: string =
  import.meta.env.VITE_DATA_REPO ?? cfg.repo.dataRepo;
export const DEFAULT_BRANCH: string =
  import.meta.env.VITE_DATA_BRANCH ?? cfg.repo.defaultBranch;

// Back-compat alias
export const REPO_SLUG = DATA_REPO_SLUG;

export const REGISTRY_URL = `https://raw.githubusercontent.com/${DATA_REPO_SLUG}/${DEFAULT_BRANCH}/registry.csv`;
export const ISSUE_NEW_URL = `https://github.com/${CODE_REPO_SLUG}/issues/new`;

// ---- ID rules (from contract) ----
export const ID_ALPHABET = REGISTRY_CONTRACT.id.alphabet;
export const ID_LENGTH = REGISTRY_CONTRACT.id.canonicalLength;
export const ID_REGEX = new RegExp(`^[${ID_ALPHABET}]{${ID_LENGTH}}$`);

// ---- Label defaults (from deploy config) ----
export const DEFAULT_SIZE_MM = cfg.labels.defaultSizeMm;
export const PRINTER_DPI = cfg.labels.printerDpi;

// ---- Deprecated: TAPE_SIZES and QR_BORDER_MODULES ----
// Tape presets replaced by <value> <mm|pt|px> unit selector.
// QR border modules are now per-code-type in code-types.json.
// Kept for backward compat until all consumers migrate.
export const TAPE_SIZES: Record<string, number> = {
  "pt-9": 6.5,
  "pt-12": 9.0,
  "pt-18": 12.0,
  "pt-24": 18.0,
  "pt-36": 28.0,
  "dk-12": 10.0,
  "dk-29": 25.0,
  "dk-38": 33.0,
  "dk-62": 56.0,
};
export const QR_BORDER_MODULES = 4;
