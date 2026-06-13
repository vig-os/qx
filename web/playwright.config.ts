// Playwright config for part-registry FE e2e (#35 Phase 3).
//
// Headless by default. Runs against a Vite preview server bound to a
// local port so tests are deterministic + offline.
//
// CI runs this via `.github/workflows/playwright.yml`. Locally,
// `nix develop -c npm run e2e` uses the playwright-driver browsers
// pinned by the flake.

import { defineConfig, devices } from "@playwright/test";

const PORT = 4173;

export default defineConfig({
  testDir: "./tests/e2e",
  // Each spec runs in its own browser context. Sequential by default —
  // bump to parallel once we have > 1 spec that's safe to parallelise.
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: 1,
  reporter: process.env.CI ? "github" : "list",
  // The FE is a static SPA; no auth context to persist between tests.
  use: {
    baseURL: `http://localhost:${PORT}`,
    trace: "on-first-retry",
    screenshot: "only-on-failure",
  },
  // Build once + serve via `vite preview` so we exercise the same
  // bundle CI deploys, not the dev server. E2E_WEB_SERVER_CMD lets a
  // caller that has ALREADY built the bundle (the Nix web-e2e check —
  // no cargo/wasm-bindgen in its sandbox) serve it directly.
  webServer: {
    command:
      process.env.E2E_WEB_SERVER_CMD ??
      `npm run build && npm exec -- vite preview --port ${PORT} --strictPort --base /`,
    url: `http://localhost:${PORT}/`,
    reuseExistingServer: !process.env.CI,
    timeout: 180_000,
    env: {
      // Match what a sandbox deploy would bake in.
      VITE_DATA_REPO: "exo-pet/exopet-registry-sandbox",
      VITE_BASE: "/",
    },
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
});
