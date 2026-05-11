/// <reference types="vite/client" />

declare module "*.wasm?url" {
  const url: string;
  export default url;
}

// Per #35: build-time data-repo selection. Set at deploy time by the
// data-repo's Pages workflow (Phase 2 release.yml in the code repo +
// Phase 3 pages.yml in each data repo).
interface ImportMetaEnv {
  /** Data-repo owner/slug, e.g. `exo-pet/exopet-registry`. */
  readonly VITE_DATA_REPO?: string;
  /** Branch of the data repo to read. Defaults to `main`. */
  readonly VITE_DATA_BRANCH?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
