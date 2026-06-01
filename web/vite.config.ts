import { resolve } from "node:path";
import { readFileSync, existsSync } from "node:fs";
import { execSync } from "node:child_process";

import { defineConfig } from "vite";
import { VitePWA } from "vite-plugin-pwa";
import tailwindcss from "@tailwindcss/vite";

// Base path for GH Pages: site is served from <user>.github.io/<repo>/
// Override with VITE_BASE env var for local dev or custom-domain hosting.
// Data-repo deployments inject their own VITE_BASE per docs/release.md.
const BASE = process.env.VITE_BASE ?? "/part-registry/";

// Inject version + git commit hash at build time so the deployed site
// can display exactly which build is running — catches stale caches.
//
// Resolution order:
//   1. GITHUB_REF_NAME env var (code-repo CI release build)
//   2. BUNDLE_METADATA.json (data-repo CI — bundle extracted from tarball)
//   3. git describe (local dev)
//   4. "dev" fallback
// BUNDLE_TAG = tag passed to data-repo sandbox deploy workflow (preferred).
// GITHUB_REF_NAME = tag in code-repo release CI (only if it looks like a version tag).
const refName = process.env.GITHUB_REF_NAME ?? "";
const isTag = refName.startsWith("v") && /^v\d/.test(refName);
const bundleTag = process.env.BUNDLE_TAG ?? "";
let appVersion = (bundleTag && bundleTag !== "latest" ? bundleTag : "") || (isTag ? refName : "") || "";
let gitHash = "";

// Try BUNDLE_METADATA.json (present when building from a release bundle).
// Search both __dirname/.. (normal) and CWD/.. (fallback for working-directory builds).
if (!appVersion) {
  const candidates = [
    resolve(__dirname, "../BUNDLE_METADATA.json"),
    resolve(process.cwd(), "../BUNDLE_METADATA.json"),
    resolve(process.cwd(), "BUNDLE_METADATA.json"),
  ];
  for (const metaPath of candidates) {
    try {
      if (existsSync(metaPath)) {
        const meta = JSON.parse(readFileSync(metaPath, "utf8")) as {
          tag?: string;
          commit?: string;
        };
        if (meta.tag) appVersion = meta.tag;
        if (meta.commit) gitHash = meta.commit.slice(0, 7);
        break;
      }
    } catch { /* parse error — try next */ }
  }
}

if (!appVersion) {
  try {
    appVersion = execSync("git describe --tags --always", { encoding: "utf8" }).trim();
  } catch {
    appVersion = "dev";
  }
}
if (!gitHash) {
  try {
    gitHash = execSync("git rev-parse --short HEAD", { encoding: "utf8" }).trim();
  } catch {
    gitHash = "dev";
  }
}
const buildTime = new Date().toISOString();

// Per ADR-013 §Consequences "PWA installability is mandatory for the
// lab-floor UX": vite-plugin-pwa generates the manifest + a Workbox
// service worker that caches the SPA shell + WASM artifacts so the
// site keeps working offline (registry.csv stays NetworkFirst so
// fresh writes are still picked up when online).
export default defineConfig({
  base: BASE,
  define: {
    __APP_VERSION__: JSON.stringify(appVersion),
    __GIT_HASH__: JSON.stringify(gitHash),
    __BUILD_TIME__: JSON.stringify(buildTime),
  },
  resolve: {
    alias: {
      "@registry-contract": resolve(
        __dirname,
        "../schema/registry-contract.json",
      ),
      "@deploy-config": resolve(
        __dirname,
        "../schema/deploy-config.json",
      ),
      "@code-types": resolve(
        __dirname,
        "../schema/code-types.json",
      ),
    },
  },
  build: {
    outDir: "dist",
    sourcemap: true,
  },
  plugins: [
    tailwindcss(),
    VitePWA({
      registerType: "autoUpdate",
      // injectManifest: custom SW with token enclave for GitHub API
      // auth (#133). The precache manifest is injected via
      // self.__WB_MANIFEST at build time; runtime caching for
      // registry CSV is configured in the SW source.
      strategies: "injectManifest",
      srcDir: "src",
      filename: "sw.ts",
      includeAssets: ["icon.svg", "icon-maskable.svg"],
      manifest: {
        name: "part-registry",
        short_name: "parts",
        description:
          "Scan QRs, look up parts, print labels, queue binds for batched PR submission.",
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
      injectManifest: {
        globPatterns: ["**/*.{js,css,html,svg,wasm,woff2,png,ico}"],
        maximumFileSizeToCacheInBytes: 8 * 1024 * 1024,
      },
      devOptions: {
        enabled: false,
      },
    }),
  ],
});
