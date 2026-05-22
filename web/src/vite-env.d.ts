/// <reference types="vite/client" />
/// <reference types="vite-plugin-pwa/client" />

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

// Deploy config + code types (build-time JSON import via Vite alias)
declare module "@deploy-config" {
  const config: Record<string, unknown>;
  export default config;
}
declare module "@code-types" {
  const codeTypes: Array<Record<string, unknown>>;
  export default codeTypes;
}

// Build-time constants injected by vite.config.ts `define`.
declare const __APP_VERSION__: string;
declare const __GIT_HASH__: string;
declare const __BUILD_TIME__: string;
