import { resolve } from "node:path";

import { defineConfig } from "vite";

// Base path for GH Pages: site is served from <user>.github.io/part-registry/
// Override with VITE_BASE env var for local dev or custom-domain hosting.
export default defineConfig({
  base: process.env.VITE_BASE ?? "/part-registry/",
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
});
