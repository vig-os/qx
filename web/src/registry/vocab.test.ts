import { describe, it, expect, beforeEach } from "vitest";
import {
  fieldVocabOptions,
  componentCandidates,
  stageVocabValue,
  getStagedVocab,
  clearStagedVocab,
  setFetchedVocab,
} from "./vocab";
import type { AppContext } from "../core/types";

function ctxWith(rows: Record<string, string>[]): AppContext {
  return { registry: { all: () => rows } } as unknown as AppContext;
}

beforeEach(() => {
  clearStagedVocab();
  setFetchedVocab("vendor", []);
  setFetchedVocab("location", []);
});

describe("fieldVocabOptions", () => {
  it("merges contract seeds with registry values, sorted + de-duplicated", () => {
    const ctx = ctxWith([{ vendor: "Acme" }, { vendor: "Acme" }, { vendor: "Zeta" }]);
    const opts = fieldVocabOptions(ctx, "vendor");
    // Contract seeds (Adafruit, Digi-Key, …) + Acme + Zeta, sorted, unique.
    expect(opts).toContain("Acme");
    expect(opts).toContain("Zeta");
    expect(opts).toContain("Digi-Key"); // a contract seed
    expect(opts.filter((v) => v === "Acme").length).toBe(1);
    expect([...opts]).toEqual([...opts].sort((a, b) => a.localeCompare(b)));
  });

  it("includes staged + fetched values", () => {
    const ctx = ctxWith([]);
    setFetchedVocab("location", ["Lab A"]);
    stageVocabValue("location", "Lab B");
    const opts = fieldVocabOptions(ctx, "location");
    expect(opts).toContain("Lab A");
    expect(opts).toContain("Lab B");
  });
});

describe("staging", () => {
  it("stages, reads back, and clears created values", () => {
    stageVocabValue("vendor", "Keysight");
    stageVocabValue("vendor", "Keysight"); // de-duped by the Set
    stageVocabValue("vendor", "  "); // blank ignored
    expect(getStagedVocab("vendor")).toEqual(["Keysight"]);
    clearStagedVocab();
    expect(getStagedVocab("vendor")).toEqual([]);
  });
});

describe("componentCandidates", () => {
  it("returns the registry's part IDs", () => {
    const ctx = ctxWith([{ id: "AAAA" }, { id: "BBBB" }]);
    const ids = componentCandidates(ctx);
    expect(ids).toContain("AAAA");
    expect(ids).toContain("BBBB");
  });
});
