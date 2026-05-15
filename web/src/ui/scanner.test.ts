// Unit tests for the pure ID helpers extracted from scanner.ts.
//
// The camera/video/WASM-decoder code in scanner.ts is inherently
// browser-dependent (getUserMedia, BarcodeDetector, DOM overlays) and
// cannot be unit-tested outside a real browser environment. These
// tests cover the extracted pure functions: canonicalizeId and
// formatIdDashed.

import { describe, it, expect } from "vitest";
import { canonicalizeId, formatIdDashed } from "./scanner";
import type { ScanStatus, ScanOptions } from "./scanner";

// -- canonicalizeId -------------------------------------------------------

describe("canonicalizeId", () => {
  it("uppercases lowercase input", () => {
    expect(canonicalizeId("k7m3pq9rt5vaxy")).toBe("K7M3PQ9RT5VAXY");
  });

  it("strips dashes", () => {
    expect(canonicalizeId("K7M3-PQ9R-T5VA-XY")).toBe("K7M3PQ9RT5VAXY");
  });

  it("handles mixed case with dashes", () => {
    expect(canonicalizeId("k7m3-pq9r")).toBe("K7M3PQ9R");
  });

  it("is idempotent on already-canonical input", () => {
    const canonical = "K7M3PQ9RT5VAXY";
    expect(canonicalizeId(canonical)).toBe(canonical);
  });

  it("returns empty string for empty input", () => {
    expect(canonicalizeId("")).toBe("");
  });
});

// -- formatIdDashed -------------------------------------------------------

describe("formatIdDashed", () => {
  it("formats a 14-char ID as 4-4-4-2", () => {
    expect(formatIdDashed("K7M3PQ9RT5VAXY")).toBe("K7M3-PQ9R-T5VA-XY");
  });

  it("formats a 12-char ID as 4-4-4", () => {
    expect(formatIdDashed("K7M3PQ9RT5VA")).toBe("K7M3-PQ9R-T5VA");
  });

  it("returns short IDs (< 12 chars) unchanged", () => {
    expect(formatIdDashed("K7M3PQ9R")).toBe("K7M3PQ9R");
    expect(formatIdDashed("ABCD")).toBe("ABCD");
    expect(formatIdDashed("")).toBe("");
  });

  it("handles a 16-char ID with 4-char tail", () => {
    expect(formatIdDashed("K7M3PQ9RT5VAXYAB")).toBe("K7M3-PQ9R-T5VA-XYAB");
  });
});

// -- Exported type signatures compile correctly ---------------------------
//
// These assertions verify the exported types are structurally sound.
// They do not exercise runtime behavior but ensure the public API
// surface is importable and type-correct.

describe("exported types", () => {
  it("ScanStatus accepts the expected string literals", () => {
    const statuses: ScanStatus[] = ["bound", "unbound", "queued", "unknown"];
    expect(statuses).toHaveLength(4);
  });

  it("ScanOptions has the expected shape", () => {
    const opts: ScanOptions = {
      multi: true,
      resolveStatus: (_id: string) => "unbound" as ScanStatus,
    };
    expect(opts.multi).toBe(true);
    expect(opts.resolveStatus!("X")).toBe("unbound");
  });
});
