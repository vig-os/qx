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
// `BENCH_NO_SSL=1` runs plain http — used when a reverse proxy terminates
// TLS with a real cert (e.g. `tailscale serve`), which is nicer than the
// self-signed LAN path (no cert warning on the phone).
const behindProxy = !!process.env.BENCH_NO_SSL;

export default defineConfig({
  root: __dirname,
  // basic-ssl gives a self-signed https origin so getUserMedia works over
  // the LAN URL (phone scanning), not just localhost. Skipped behind a proxy.
  plugins: behindProxy ? [] : [basicSsl()],
  server: {
    port: 5174,
    host: true,
    // Allow the tailnet hostname when fronted by `tailscale serve`.
    allowedHosts: [".ts.net"],
  },
  // The decoder wasm is a committed build artifact under ./wasm — let Vite
  // serve it as an asset.
  assetsInclude: ["**/*.wasm"],
});
