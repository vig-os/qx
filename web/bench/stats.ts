// Running A/B tallies for the dual-engine bench. Pure + unit-testable.

import type { FrameResult } from "./dual-decode";

export interface EngineTally {
  frames: number;
  hits: number;
  latencies: number[];
}

export interface Tallies {
  zxing: EngineTally;
  rxing: EngineTally;
  /** Frames where both decoded the same payload. */
  agree: number;
  /** Frames where exactly one engine decoded. */
  diverge: number;
  /** Per-id divergence detail: which engine read what the other missed. */
  divergences: Array<{ frame: number; zxing: string | null; rxing: string | null }>;
}

export function emptyTallies(): Tallies {
  return {
    zxing: { frames: 0, hits: 0, latencies: [] },
    rxing: { frames: 0, hits: 0, latencies: [] },
    agree: 0,
    diverge: 0,
    divergences: [],
  };
}

export function record(t: Tallies, frame: number, r: FrameResult): void {
  t.zxing.frames++;
  t.rxing.frames++;
  t.zxing.latencies.push(r.zxing.ms);
  t.rxing.latencies.push(r.rxing.ms);
  if (r.zxing.hit) t.zxing.hits++;
  if (r.rxing.hit) t.rxing.hits++;
  if (r.agree) t.agree++;
  if (r.diverge) {
    t.diverge++;
    t.divergences.push({ frame, zxing: r.zxing.value, rxing: r.rxing.value });
  }
}

export function pct(n: number, d: number): number {
  return d === 0 ? 0 : (100 * n) / d;
}

/** Nearest-rank percentile (p in [0,100]). Empty → 0. */
export function percentile(xs: number[], p: number): number {
  if (xs.length === 0) return 0;
  const sorted = [...xs].sort((a, b) => a - b);
  const rank = Math.ceil((p / 100) * sorted.length);
  return sorted[Math.min(rank, sorted.length) - 1];
}

export function hitRate(t: EngineTally): number {
  return pct(t.hits, t.frames);
}

/** Of the frames where ZXING decoded (the proven baseline), how often did
 *  rxing agree? This is THE number that gates a zxing drop. */
export function rxingParityOnZxingHits(t: Tallies): {
  zxingHitFrames: number;
  agree: number;
  parityPct: number;
} {
  // agree already requires both hit + same value; zxing hits is the
  // denominator (the frames a working scanner reads today).
  return {
    zxingHitFrames: t.zxing.hits,
    agree: t.agree,
    parityPct: pct(t.agree, t.zxing.hits),
  };
}
