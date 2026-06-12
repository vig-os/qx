/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** mock (default) | http | wasm — see src/transport/index.ts */
  readonly VITE_TRANSPORT?: string;
  /** Base URL for the http transport; defaults to same origin. */
  readonly VITE_API_BASE?: string;
  /** Snapshot URL the wasm transport fetches (required for wasm). */
  readonly VITE_DATA_URL?: string;
  /** Snapshot format for the wasm transport: csv (default) | jsonl. */
  readonly VITE_DATA_FORMAT?: string;
  /** Registry display name for the wasm transport; defaults to the data URL. */
  readonly VITE_REGISTRY_NAME?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
