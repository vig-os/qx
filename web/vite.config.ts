import { resolve } from "node:path";

import { defineConfig } from "vite";
import { VitePWA } from "vite-plugin-pwa";

// Base path for GH Pages: site is served from <user>.github.io/<repo>/
// Override with VITE_BASE env var for local dev or custom-domain hosting.
// Data-repo deployments inject their own VITE_BASE per docs/release.md.
const BASE = process.env.VITE_BASE ?? "/part-registry/";

// Per ADR-013 §Consequences "PWA installability is mandatory for the
// lab-floor UX": vite-plugin-pwa generates the manifest + a Workbox
// service worker that caches the SPA shell + WASM artifacts so the
// site keeps working offline (registry.csv stays NetworkFirst so
// fresh writes are still picked up when online).
export default defineConfig({
  base: BASE,
  resolve: {
    alias: {
      "@registry-contract": resolve(
        __dirname,
        "../schema/registry-contract.json",
      ),
    },
  },
  build: {
    outDir: "dist",
    sourcemap: true,
  },
  plugins: [
    VitePWA({
      registerType: "autoUpdate",
      strategies: "generateSW",
      includeAssets: ["icon.svg", "icon-maskable.svg"],
      manifest: {
        name: "part-registry",
        short_name: "parts",
        description:
          "Scan QRs, look up parts, print labels, queue binds for batched PR submission.",
        // Use BASE so the manifest works for both code-repo and data-
        // repo Pages deployments without manual tweaking.
        start_url: BASE,
        scope: BASE,
        display: "standalone",
        theme_color: "#1f6feb",
        background_color: "#ffffff",
        orientation: "any",
        icons: [
          {
            src: "icon.svg",
            sizes: "any",
            type: "image/svg+xml",
            purpose: "any",
          },
          {
            src: "icon-maskable.svg",
            sizes: "any",
            type: "image/svg+xml",
            purpose: "maskable",
          },
        ],
      },
      workbox: {
        // Globs collected from `dist/` at build time. Excludes the
        // .map files (sourcemaps) and the registry CSV (which is
        // fetched from the data-repo, not bundled).
        globPatterns: ["**/*.{js,css,html,svg,wasm,woff2,png,ico}"],
        // Bumped from the 2 MB default because zxing's reader.wasm
        // alone is ~1 MB raw. Capping at 8 MB keeps room for the
        // full SPA + both WASM artifacts.
        maximumFileSizeToCacheInBytes: 8 * 1024 * 1024,
        runtimeCaching: [
          {
            // Registry / audit / print log live in the data repo on
            // raw.githubusercontent.com — fetch fresh when online,
            // fall through to cache when offline. Keeps the audit-
            // of-record property: a cached read never overrides a
            // fresh write.
            urlPattern:
              /^https:\/\/raw\.githubusercontent\.com\/.+\.csv$/,
            handler: "NetworkFirst",
            options: {
              cacheName: "registry-data",
              networkTimeoutSeconds: 5,
              expiration: {
                maxEntries: 32,
                maxAgeSeconds: 60 * 60 * 24,
              },
              cacheableResponse: { statuses: [0, 200] },
            },
          },
        ],
        navigateFallback: `${BASE}index.html`,
        cleanupOutdatedCaches: true,
      },
      devOptions: {
        enabled: false,
      },
    }),
  ],
});
