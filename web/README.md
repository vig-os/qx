# web/ — part-registry SPA

Static site that scans QRs, looks up parts, prints labels via the OS
print dialog, and queues binds for batched PR submission.

**Deployment** (per #35):
- The code repo's [`pages.yml`](../.github/workflows/pages.yml) publishes
  a preview build to `https://morepet.github.io/part-registry/`
  bundled with the **sandbox** data repo
  (`exo-pet/exopet-registry-sandbox`) on every push touching `web/**`.
- Per-registry production deployments live on each data repo's own
  Pages site (Phase 3 in #35). Each one consumes the released bundle
  from this code repo and bakes its own `VITE_DATA_REPO` at build time.

The selected data repo is controlled by the `VITE_DATA_REPO` build-time
env var (defaults to the sandbox so a vanilla `npm run build` never
accidentally targets the audit-of-record registry).

## Architecture

Three extension points, each with a small interface in
[`src/core/types.ts`](src/core/types.ts):

| Interface | Add a new… by | Examples |
|---|---|---|
| `Tab` | dropping a file in `src/tabs/` and registering | Lookup, Print, Bind |
| `Layout` | dropping a file in `src/layouts/` and registering | vert, horz, flag |
| `OutputMode` | dropping a file in `src/output/` and registering | dk-continuous, dk-1201-diecut |
| `Plugin` | dropping a file in `src/plugins/` and registering | error report (more later) |

**Layout vs OutputMode.** `Layout` decides what *one label* looks like
(QR + 4/4/4 text arrangement at a given size). `OutputMode` decides
how *N labels* lay out on paper — page-per-label on continuous DK
tape, packed grid on a DK-1201 die-cut, strip-with-crop-marks (#7),
A4 sticker sheet, etc. The Print tab builds `JobItem[]` and delegates
both planning (item list → physical pages) and print-HTML emission to
the active mode. Adding a new paper format = new file in
`src/output/`, register, done — the Print tab UI auto-renders the
mode's option fields.

Single sources of truth:

- [`src/config.ts`](src/config.ts) — repo slug, registry URL, ID
  alphabet/length/regex, QR border, tape sizes, default size.
- [`src/registry/schema.ts`](src/registry/schema.ts) — registry row
  shape + field metadata. Imported by lookup detail view, bind form,
  validators (when added).
- [`src/registry/registry.ts`](src/registry/registry.ts) — sole entry
  point for reading registry data. Tabs depend on the `Registry`
  interface, never on `fetch` or CSV parsing details (Dependency
  Inversion).

## Scripts

```bash
npm install
npm run dev          # local dev with HMR
npm run build        # type-check + production bundle to dist/
npm run preview      # serve the built bundle
```

## QR / Micro QR scanning

The scanner ([`src/ui/scanner.ts`](src/ui/scanner.ts)) decodes QR and
Micro QR (M1–M4) using [`barcode-detector`](https://github.com/Sec-ant/barcode-detector)
(MIT) — a `BarcodeDetector`-shaped polyfill backed by
[`zxing-wasm`](https://github.com/Sec-ant/zxing-wasm) (Apache-2.0,
ZXing-C++ compiled to WebAssembly). We use it *unconditionally*, not
as a fallback to the native `BarcodeDetector` API: native availability
and Micro QR coverage are both inconsistent across browsers (Firefox
and desktop Safari don't expose it; Chrome/Android often advertises
only `qr_code` and doesn't actually decode Micro QR; iOS Safari
decodes Micro QR transparently but doesn't advertise it). One decoder
everywhere removes that platform matrix.

**Bundle cost** (lazy-loaded on first scan):

| Asset | Raw | Gzipped |
|---|---|---|
| `zxing_reader.wasm` | ~1.0 MB | ~419 KB |
| `barcode-detector` ponyfill JS | ~43 KB | ~15 KB |

The wasm binary is bundled via Vite (`?url` import) and served from
the same origin as the rest of the SPA — no third-party CDN
dependency at runtime. The cold page load is unaffected; the WASM
chunk only loads when the user opens the scanner.

The overlay badge names the active decoder + version + supported
formats so a misbehaving scan can be diagnosed quickly:
`QR + Micro QR (zxing-wasm 3.0.3)`.

## Label rendering: Rust WASM façade

The SVG layout renderers in [`src/layouts/`](src/layouts/) call into
[`crates/wasm/`](../crates/wasm/) — a `wasm-bindgen` façade over the
Rust `codec` + `validators` crates per
[ADR-017](../decisions/ADR-017-rust-core-ports-adapters.md)
§"strangler-fig step 8" (foundation issue #33, landed 2026-05-11).

Drift is now structurally impossible: CLI, CI, and FE all link the
same encoder. The previous TS port (`qrcode-generator.ts` + `svg.ts`)
has been deleted.

### Bundle cost

| Asset | Raw | Gzipped |
|---|---|---|
| `part_registry_wasm_bg.wasm` (encoder + validators + classifier) | 334 KB | 128 KB |
| wasm-bindgen JS shim | 16 KB | 6 KB |

Comfortably under the 1.5 MB gzipped budget set in foundation issue
#33. The decoder (`rxing`) is feature-gated *off* in this build —
the FE keeps `zxing-wasm` for scanning (see §QR/Micro QR scanning
below); the Rust decoder is used by the CLI + native tests + the
A/B parity harness only.

### Dev workflow

```bash
# wasm-pack (or wasm-bindgen + cargo) must be on PATH.
cargo install wasm-bindgen-cli --version 0.2.121  # rustc ≥ 1.86
npm run build:wasm                                  # invoked by build/test
npm run build                                       # full prod build
```

The `build:wasm` script compiles `crates/wasm/` to wasm32 and runs
`wasm-bindgen --target web` into `src/wasm/`. Output is gitignored
(generated artefact); CI regenerates on every build.

### Parity gate

`src/wasm/ab-parity.test.ts` round-trips ≥ 6 canonical IDs through
the Rust encoder → `zxing-wasm` decoder. The synthetic round-trip
covers both Standard and Micro QR; the printed-scan corpus is a
follow-up (see test file for the open TODO).

## Deployment

The repo's GitHub Pages settings need to be set to **Source: GitHub
Actions** (not "deploy from a branch") for the `pages.yml` workflow to
publish. After the first push to `main`, set this once at:
`https://github.com/MorePET/part-registry/settings/pages`.
