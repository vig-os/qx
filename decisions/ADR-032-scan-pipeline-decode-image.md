# ADR-032 — Scan pipeline + `decode-image` (one processor, many sources)

- Status: Accepted
- Date: 2026-06-10
- Component / area: `crates/codec` (decode surfaced to all targets) +
  `crates/app` (the scan processor) + the web FE (camera shrinks to a
  frame source). Realizes the "surface `decode-image`, drop `zxing-wasm`"
  decision from the operations catalog.
- Reviewers: Lars Gerchow (accepted 2026-06-11)
- Related: ADR-013/014 (current web scanner), ADR-017 (Rust codec as the
  one decoder; rxing decode + the step-8 A/B cutover criterion), ADR-027
  (port conformance — replay fixtures), ADR-028 (SOUP — dropping
  `zxing-wasm`), ADR-030 (shells + command protocol; op = core,
  I/O = shell capability)
- Feeds: `decisions/explorations/operations-catalog.md` (§D)

## Context

Scanning today lives entirely in `web/src/ui/scanner.ts`: it runs its
**own** QR/Micro-QR decode via `zxing-wasm` (a `barcode-detector`
ponyfill), resolves each hit's status (bound/unbound/queued/unknown),
and offers two modes (live polling, multi-snapshot). Meanwhile the Rust
codec **already** decodes (`codec::qr::decode_qr`, rxing) — but it is
deliberately excluded from the wasm façade, so the FE keeps a parallel
JavaScript decoder.

That parallel decoder is exactly the drift ADR-017 exists to kill (two
QR codecs, one in Rust, one in JS) **and** a SOUP item (`zxing-wasm`,
ADR-028). Separately, three "different" scanning surfaces — a single
still image, a replayed video, and a live camera — are the same logical
pipeline reimplemented per surface. Both problems have one fix.

## Alternatives considered

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| **Keep `zxing-wasm` in the FE; Rust decode stays CLI/server-only** | Works today; no A/B needed | Two QR codecs (the ADR-017 drift); a SOUP item kept; `capture+decode` can't be a uniform op; scan logic reimplemented per surface | Rejected |
| **Surface `decode-image` from the codec + a shared scan processor behind a `FrameSource` seam; drop `zxing-wasm` after an A/B gate** | One decoder everywhere; one resolver + rolling-state machine; live path testable by replaying fixtures; removes a SOUP item | Must A/B rxing vs zxing on real Micro QR scans before removal; rxing decode enters the wasm bundle | **Chosen** |
| **Per-shell scan implementations sharing only the decoder** | Simple per shell | Re-implements resolve + rolling/dedupe N times; the exact reinvention ADR-030 §8 parity fights | Rejected |

## Decision

### 1. Surface `decode-image` everywhere

Expose the Rust codec's decode through `crates/app` (a `Request` op) and
the wasm façade. One decoder for CLI, TUI, server, MCP, **and** the web —
no JavaScript QR codec.

### 2. The scan pipeline (DRY / SOLID)

A still image, a replayed video, and a live camera are the **same
pipeline**; only the ends differ:

```
FrameSource → decode-image → Resolve{id} → RollingAccumulator → Sink
 (shell cap)      (core)        (core)          (core)          (shell)
```

- **`FrameSource`** — the dependency-inversion seam. Still image =
  length-1 stream · video file = frame replay · live camera = frame
  stream · image directory = batch. All yield `Frame`; the processor is
  source-agnostic. "Replay a video / an image / live" is *effectively the
  same*.
- **Core processor** (`crates/app`, a `scan` module) — per frame:
  `decode-image` → **`Resolve{id}`** (2026-06-11 pass: the universal
  resolver per ADR-035 §0 — decoded text parses bare-value → default
  scheme, resolving to an entity in *whatever* collection, so scanning a
  location or container label needs zero pipeline changes) → a
  **`RollingAccumulator`** that dedupes across frames, tracks
  first/last-seen, and debounces. Per-hit state is
  `{resolved(collection, entity, status per the descriptor's lifecycle),
  pending(proposal ref), unknown}` — "queued" is the generic
  in-flight-proposal state, and displayed statuses come from the
  descriptor, not code. Pure + deterministic; no I/O.
- **`Sink`** — per shell: GUI multi-snapshot overlay · TUI list · CLI
  batch JSON · MCP response.

`capture-frames` is the shell capability; `decode + resolve + roll` is
the core op — the same operations-universal / IO-is-shell-capability
split the operations catalog uses throughout.

### 3. Drop `zxing-wasm` after an A/B cutover gate

Before removing `zxing-wasm`, A/B `rxing` against it on a corpus of
real-world Micro QR scans (the ADR-017 step-8 acceptance criterion);
`rxing` must match or beat decode rate. The two may coexist behind a
flag until the gate passes. On pass: remove `zxing-wasm` from the web
dependencies and retire it from the SOUP inventory (ADR-028); `rxing`
becomes the sole decode path.

### 4. Replay is the test harness

Because the processor consumes a `FrameSource`, a recorded video / image
fixture *is* a deterministic test of the live path — no camera. These
fixtures become a `port_tests` conformance suite (ADR-027) for the scan
processor.

## Rationale

One fix retires two problems: the last parallel codec (drift, ADR-017)
and a SOUP dependency (ADR-028). Past the decoder, **resolve** and the
**rolling-state machine** are identical across every surface — writing
them once in the core and varying only `FrameSource`/`Sink` is the DRY
win, and the `FrameSource` seam is what makes the otherwise-untestable
live camera path testable by replay. This is the operations-catalog
principle (operation universal; capture is a shell capability) applied to
input, mirroring ADR-031's treatment of output (render universal;
delivery is a shell capability).

## Consequences

- **codec decode enters the wasm façade**; `decode-image` becomes a
  `crates/app` `Request` op → an Op in the ADR-030 §8 parity matrix.
- **`web/src/ui/scanner.ts` shrinks** to `FrameSource(camera)` +
  `Sink(overlay)`; its decode + resolve move into the shared core.
- **`zxing-wasm` is removed** from web deps and the SOUP inventory once
  the A/B gate passes (tracked, not immediate).
- **`RollingAccumulator` is new core code** with replay-fixture tests.
- **Bundle**: rxing decode (~1 MB gz per ADR-017) replaces `zxing-wasm`
  in the web bundle — net wash or better, per ADR-017's estimate.

## Open questions / supersession triggers

- **Image formats in core** — camera frames are raw buffers; do
  non-camera sources (PNG/JPEG files) decode inside the core or get
  decoded to raw frames by the shell? Likely: shell decodes container →
  raw frame → core. Confirm at build.
- **Video demux** is a `FrameSource`/shell concern (the core only sees
  frames), not core scope.
- **Symbology scope** — rxing also decodes DataMatrix; in scope or QR +
  Micro QR only for now?
- **Live-path perf** — per-frame decode cost; the processor may need
  downscale / region-of-interest. Revisit if the live path drops frames.

## References

- ADR-013 / ADR-014 — current web scanner (`web/src/ui/scanner.ts`)
- ADR-017 — Rust codec as the one decoder; rxing; step-8 A/B criterion
- ADR-027 — port conformance (scan replay fixtures)
- ADR-028 — SOUP inventory (`zxing-wasm` removal)
- ADR-030 — shells + command protocol; op×spoke parity (§8)
- ADR-031 — output counterpart (render universal; delivery is shell I/O)
- `crates/codec/src/qr.rs` (`decode_qr`), `rxing` — <https://github.com/rxing-core/rxing>
- `decisions/explorations/operations-catalog.md` §D — scan pipeline
