# Dual-engine scan bench — rxing vs zxing

The honest A/B for `decode-image-surfaced` + `zxing-wasm-dropped` (ADR-032 /
ADR-017 step 8). One camera, every frame fanned to **both** decoders on the
**same pixels at the same moment**:

- **zxing-wasm** — the production scanner's decoder (`barcode-detector`
  ponyfill), `detect(canvas)`.
- **rxing (Rust)** — the `decode_image` wasm façade (`crates/wasm`, built
  with `--features decoder`), fed the frame as JPEG bytes.

Static photos can't settle this — scanning is a *live, close-up* regime. The
bench records hit-rate, p50/p95 latency, and the one number that gates a
zxing drop: **rxing's parity on the frames zxing actually reads**. It must
reach ~100% before zxing can go.

## Run

```sh
cd web
npm run bench:wasm   # build the rxing decoder bundle into bench/wasm/ (~6 min, ~4.5 MB)
npm run bench        # vite dev server on :5174  (camera needs localhost or https)
```

Open the printed URL on a phone (same network, via https) or a webcam-equipped
laptop. **Start camera**, point at a `qx` label up close.

## What you get

- Live green overlay quad + decoded id (whichever engine hits).
- Live stats table: frames, hits, hit-rate, p50/p95 ms per engine.
- **rxing parity on zxing's hits** — the gate number.
- A divergence log (one engine read, the other missed) — the interesting cases.
- **Capture frame → corpus**: saves the current JPEG so you can drop real
  scan frames into `crates/codec/tests/corpus/` for a reproducible CI A/B.
- **Auto-capture divergences**: every disagreement frame downloads itself.

## Why it's a separate Vite app

The `--features decoder` rxing wasm is ~4.5 MB — it must never touch the lean
production GH Pages bundle (`web/src/`). Rooting the bench here keeps it out.
The decoder wasm (`bench/wasm/qx_decoder*`) is git-ignored; rebuild with
`npm run bench:wasm`.
