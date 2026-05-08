// Single source of truth for all build-time / runtime constants.
//
// The registry schema and ID rules live in the shared contract under
// `schema/registry-contract.json`; app-only constants stay here.

import { REGISTRY_CONTRACT } from "./registry/contract";

export const REPO_SLUG = "MorePET/part-registry";
export const DEFAULT_BRANCH = "main";

// raw.githubusercontent.com is unauthenticated and CORS-open for public
// repos. Direct fetch of registry.csv from main branch.
export const REGISTRY_URL =
  `https://raw.githubusercontent.com/${REPO_SLUG}/${DEFAULT_BRANCH}/registry.csv`;

// GitHub web URL for opening prefilled issue forms (no API token needed).
export const ISSUE_NEW_URL = `https://github.com/${REPO_SLUG}/issues/new`;

export const ID_ALPHABET = REGISTRY_CONTRACT.id.alphabet;
export const ID_LENGTH = REGISTRY_CONTRACT.id.canonicalLength;
export const ID_REGEX = new RegExp(`^[${ID_ALPHABET}]{${ID_LENGTH}}$`);

// QR encoding parameters — must match label.py.
export const QR_BORDER_MODULES = 4;

// Brother label tape printable-height presets (mm).
//   pt-N : Brother P-touch (TZe tapes)
//   dk-N : Brother QL (DK rolls), e.g. QL-820NWBc
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

export const DEFAULT_SIZE_MM = 11.0;
