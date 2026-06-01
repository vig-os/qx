import { describe, it, expect } from "vitest";
import { parseVocab, mergeVocab, serializeVocab } from "./vocab-files";

describe("parseVocab", () => {
  it("parses an array of {name} entries", () => {
    expect(parseVocab('[{"name":"Digi-Key"},{"name":"Mouser"}]')).toEqual([
      { name: "Digi-Key" },
      { name: "Mouser" },
    ]);
  });

  it("keeps extra fields (expandable entries)", () => {
    expect(parseVocab('[{"name":"Acme","url":"x"}]')).toEqual([{ name: "Acme", url: "x" }]);
  });

  it("tolerates malformed / non-array / entries without a name", () => {
    expect(parseVocab("not json")).toEqual([]);
    expect(parseVocab('{"name":"x"}')).toEqual([]);
    expect(parseVocab('[{"nope":1},{"name":"ok"}]')).toEqual([{ name: "ok" }]);
  });
});

describe("mergeVocab", () => {
  it("appends genuinely new names, sorted", () => {
    const merged = mergeVocab([{ name: "Mouser" }], ["Acme", "Zeta"]);
    expect(merged).toEqual([{ name: "Acme" }, { name: "Mouser" }, { name: "Zeta" }]);
  });

  it("returns null when nothing is new (skip the no-op commit)", () => {
    expect(mergeVocab([{ name: "Mouser" }], ["Mouser"])).toBeNull();
    expect(mergeVocab([{ name: "Mouser" }], [])).toBeNull();
  });

  it("ignores blanks and de-dupes against existing", () => {
    expect(mergeVocab([{ name: "A" }], ["  ", "A", "B"])).toEqual([{ name: "A" }, { name: "B" }]);
  });

  it("preserves existing extra fields", () => {
    const merged = mergeVocab([{ name: "A", url: "u" }], ["B"]);
    expect(merged).toEqual([{ name: "A", url: "u" }, { name: "B" }]);
  });
});

describe("serializeVocab", () => {
  it("pretty-prints with a trailing newline for clean diffs", () => {
    expect(serializeVocab([{ name: "A" }])).toBe('[\n  {\n    "name": "A"\n  }\n]\n');
  });
});
