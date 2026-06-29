import { defineConfig } from "vite";

// Standalone Vite app for the dual-engine scan bench. Rooted here so the
// heavy `--features decoder` rxing wasm (web/bench/wasm/) never touches the
// lean production GH Pages bundle. Shares web/node_modules (zxing-wasm,
// barcode-detector) via normal upward resolution.
//
//   npm run bench        # dev server (camera needs https or localhost)
//   npm run bench:wasm   # rebuild the rxing decoder bundle
export default defineConfig({
  root: __dirname,
  server: { port: 5174, host: true },
  // The decoder wasm is a committed build artifact under ./wasm — let Vite
  // serve it as an asset.
  assetsInclude: ["**/*.wasm"],
});
