// Single source of truth for all build-time / runtime constants.
//
// The registry schema and ID rules live in the shared contract under
// `schema/registry-contract.json`; app-only constants stay here.

import { REGISTRY_CONTRACT } from "./registry/contract";

// Per #35: code repo (this) is open source; data lives in private
// per-registry data repos (`exo-pet/exopet-registry[-sandbox]`). The
// FE reads its data repo from `VITE_DATA_REPO` at build time. Default
// is the sandbox so a vanilla `npm run build` never accidentally
// targets the audit-of-record registry.
export const CODE_REPO_SLUG = "MorePET/part-registry";
export const DATA_REPO_SLUG: string =
  import.meta.env.VITE_DATA_REPO ?? "exo-pet/exopet-registry-sandbox";
export const DEFAULT_BRANCH: string =
  import.meta.env.VITE_DATA_BRANCH ?? "main";

// Back-compat alias — existing call sites that meant "the data repo"
// (e.g. raw.githubusercontent.com URLs) read this. New code should
// use `DATA_REPO_SLUG` explicitly.
export const REPO_SLUG = DATA_REPO_SLUG;

// raw.githubusercontent.com is unauthenticated and CORS-open for public
// repos. Direct fetch of registry.csv from the data repo. Once the
// data repo is private, this URL needs an authenticated fetch (handled
// through the GitHub OAuth flow per ADR-020) — for now the sandbox is
// public so this path Just Works.
export const REGISTRY_URL = `https://raw.githubusercontent.com/${DATA_REPO_SLUG}/${DEFAULT_BRANCH}/registry.csv`;

// GitHub web URL for opening prefilled issue forms (no API token needed).
// Issues go to the code repo (bugs/features), proposals go to the data
// repo (PR via ProposalSink per ADR-019).
export const ISSUE_NEW_URL = `https://github.com/${CODE_REPO_SLUG}/issues/new`;

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
