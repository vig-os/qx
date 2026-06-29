import basicSsl from "@vitejs/plugin-basic-ssl";
import { defineConfig } from "vite";

// Standalone Vite app for the dual-engine scan bench. Rooted here so the
// heavy `--features decoder` rxing wasm (web/bench/wasm/) never touches the
// lean production GH Pages bundle. Shares web/node_modules (zxing-wasm,
// barcode-detector) via normal upward resolution.
//
//   npm run bench        # https dev server on :5174 — open the LAN URL on a
//                        #   phone (accept the self-signed cert) to scan real
//                        #   labels; the camera needs a secure context.
//   npm run bench:wasm   # rebuild the rxing decoder bundle
export default defineConfig({
  root: __dirname,
  // basic-ssl gives a self-signed https origin so getUserMedia works over
  // the LAN URL (phone scanning), not just localhost.
  plugins: [basicSsl()],
  server: { port: 5174, host: true },
  // The decoder wasm is a committed build artifact under ./wasm — let Vite
  // serve it as an asset.
  assetsInclude: ["**/*.wasm"],
});
