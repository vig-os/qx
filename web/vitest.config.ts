import { resolve } from "node:path";

import { defineConfig } from "vitest/config";

export default defineConfig({
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
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts", "bench/**/*.test.ts"],
    // Per foundation issue #33: load `crates/wasm/` once before any
    // test module, so the synchronous `renderLabelSync` surface used
    // by layouts works the same way it does in the browser (where
    // `main.ts` awaits `loadWasm()` at boot).
    setupFiles: ["src/wasm/test-setup.ts"],
  },
});
