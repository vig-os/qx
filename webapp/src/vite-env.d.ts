/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** mock (default) | http | wasm — see src/transport/index.ts */
  readonly VITE_TRANSPORT?: string;
  /** Base URL for the http transport; defaults to same origin. */
  readonly VITE_API_BASE?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
