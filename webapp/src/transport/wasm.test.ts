import { describe, expect, it } from "vitest";
import { wasmTransport } from "./wasm";

// The wasm-pack output is environment-built (npm run build:wasm) and
// gitignored, so in the test environment the pkg is absent. The
// transport must fail at wiring time with an actionable message, not
// crash typecheck/build — that absence path IS the contract under test.
describe("wasmTransport without the built pkg", () => {
  it("rejects with a build:wasm hint", async () => {
    await expect(wasmTransport({ VITE_DATA_URL: "http://example.test/registry.csv" })).rejects.toThrow(
      /build:wasm/,
    );
  });
});
