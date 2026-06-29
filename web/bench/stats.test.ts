import { describe, expect, it } from "vitest";
import type { FrameResult } from "./dual-decode";
import { emptyTallies, hitRate, percentile, record, rxingParityOnZxingHits } from "./stats";

function fr(zVal: string | null, rVal: string | null, zMs = 5, rMs = 30): FrameResult {
  const zn = zVal?.toUpperCase().replace(/[-\s]/g, "") ?? null;
  const rn = rVal?.toUpperCase().replace(/[-\s]/g, "") ?? null;
  return {
    zxing: { hit: zn !== null, value: zVal, ms: zMs },
    rxing: { hit: rn !== null, value: rVal, ms: rMs },
    agree: zn !== null && zn === rn,
    diverge: (zn === null) !== (rn === null),
  };
}

describe("bench stats", () => {
  it("percentile is nearest-rank, empty → 0", () => {
    expect(percentile([], 50)).toBe(0);
    expect(percentile([10, 20, 30, 40], 50)).toBe(20);
    expect(percentile([10, 20, 30, 40], 95)).toBe(40);
  });

  it("scores rxing parity only over the frames zxing actually read", () => {
    const t = emptyTallies();
    // zxing reads 3 frames; rxing matches 2 of them, misses 1, plus a frame
    // where neither reads.
    record(t, 1, fr("8V46KM8B", "8V46KM8B"));
    record(t, 2, fr("8V46KM8B", "8V46KM8B"));
    record(t, 3, fr("8V46KM8B", null)); // zxing hit, rxing missed → diverge
    record(t, 4, fr(null, null)); // both miss
    expect(t.zxing.hits).toBe(3);
    expect(t.rxing.hits).toBe(2);
    expect(t.diverge).toBe(1);
    const p = rxingParityOnZxingHits(t);
    expect(p.zxingHitFrames).toBe(3);
    expect(p.agree).toBe(2);
    expect(Math.round(p.parityPct)).toBe(67);
    expect(Math.round(hitRate(t.zxing))).toBe(75); // 3/4 frames
  });
});
