import { describe, expect, it } from "vitest";

import { parseAppPath } from "./route";

describe("parseAppPath", () => {
  it("treats the GH Pages base path root as home", () => {
    expect(parseAppPath("/part-registry/")).toEqual({ kind: "home" });
  });

  it("normalizes hyphenated mixed-case IDs into the canonical part route", () => {
    expect(parseAppPath("/part-registry/abCd-efGh-jkMn")).toEqual({
      kind: "part",
      id: "ABCDEFGHJKMN",
    });
  });

  it("reports invalid normalized IDs explicitly", () => {
    expect(parseAppPath("/part-registry/ABCD-0FGH-IJKL")).toEqual({
      kind: "invalid-part-id",
      rawSegment: "ABCD-0FGH-IJKL",
      normalized: "ABCD0FGHIJKL",
    });
  });

  it("ignores extra path depth for now", () => {
    expect(parseAppPath("/part-registry/ABCD-EFGH-IJKL/history")).toEqual({
      kind: "home",
    });
  });
});
