import { describe, expect, it } from "vitest";

import { generateId, generateIds } from "./mint";

// The canonical alphabet from the registry contract. We replicate it
// here so the tests are self-contained and don't depend on contract
// loading (which needs the WASM setup file).
const ALPHABET = "23456789ACDEFGHJKMNPQRTVWXY";
const LENGTH = 14;

describe("generateId", () => {
  it("returns a string of the requested length", () => {
    const id = generateId(ALPHABET, LENGTH);
    expect(id).toHaveLength(LENGTH);
  });

  it("uses only characters from the alphabet", () => {
    const allowed = new Set(ALPHABET.split(""));
    for (let i = 0; i < 50; i++) {
      const id = generateId(ALPHABET, LENGTH);
      for (const ch of id) {
        expect(allowed.has(ch)).toBe(true);
      }
    }
  });

  it("respects custom alphabet and length", () => {
    const id = generateId("AB", 5);
    expect(id).toHaveLength(5);
    expect(id).toMatch(/^[AB]{5}$/);
  });
});

describe("generateIds", () => {
  it("returns the requested count of IDs", () => {
    const ids = generateIds(10, ALPHABET, LENGTH);
    expect(ids).toHaveLength(10);
  });

  it("returns unique IDs", () => {
    const ids = generateIds(50, ALPHABET, LENGTH);
    const unique = new Set(ids);
    expect(unique.size).toBe(50);
  });

  it("generates IDs with correct length and alphabet", () => {
    const ids = generateIds(20, ALPHABET, LENGTH);
    const pattern = new RegExp(`^[${ALPHABET}]{${LENGTH}}$`);
    for (const id of ids) {
      expect(id).toMatch(pattern);
    }
  });

  it("caps at max count even with small alphabet space", () => {
    // 2-char alphabet, length 2 → only 4 possible IDs
    const ids = generateIds(10, "AB", 2);
    // Should get at most 4 unique IDs (capped by the maxAttempts guard)
    expect(ids.length).toBeLessThanOrEqual(4);
    expect(ids.length).toBeGreaterThan(0);
    // All unique
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("handles count of 1", () => {
    const ids = generateIds(1, ALPHABET, LENGTH);
    expect(ids).toHaveLength(1);
  });

  it("returns empty array for count 0", () => {
    const ids = generateIds(0, ALPHABET, LENGTH);
    expect(ids).toHaveLength(0);
  });
});
