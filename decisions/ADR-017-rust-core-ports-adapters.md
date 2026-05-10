# ADR-017 — Rust core, ports/adapters, multi-target deploy

- Status: Accepted
- Date: 2026-05-10
- Component / area: cross-cutting — replaces the Python+Pyodide
  trajectory captured in ADR-013/014 with a Rust workspace; defines
  the architectural shape every other ADR builds against
- Reviewers: Lars Gerchow
- Supersedes (in part): ADR-014 §"Pyodide migration trigger" — the
  trigger is no longer "when do we move to Pyodide"; the answer is
  "we don't, we move to Rust"
- Related: ADR-016 (PR-diff policy), ADR-018 (Storage port), ADR-019
  (Proposal sink port), ADR-020 (Identity port), ADR-021 (Config),
  ADR-022 (Observability), ADR-023 (Threat model), ADR-024 (Crypto
  baseline), ADR-025 (Distribution), ADR-027 (Conformance tests)

## Context

The project currently has two parallel implementations of overlapping
logic:

- **Python** (`label.py`, `mint.py`, `bind.py`, `validators/`,
  `tools/sheet.py`) — the canonical CLI tooling. Uses `segno` for
  QR generation, ad-hoc CSV I/O, ad-hoc operator identity
  (`--operator $USER`).
- **TypeScript** (`web/src/`) — the GitHub Pages SPA. Uses an inline
  pure-JS QR encoder (`web/src/layouts/qrcode-generator.ts`),
  hand-ported layout primitives in `web/src/layouts/svg.ts`,
  `papaparse` for CSV.

Issue #13 (Micro QR phases 2-3) confirmed empirically that the two QR
encoders produce **different** Standard QR matrices for the same
payload — same payload, both decodable, but the bit patterns differ
because mask-pattern selection is not bit-identical between
`segno` and the inline TS encoder. This is the exact "TS-port drift
risk" ADR-014 acknowledged when it accepted the spike's dual
implementation.

Issue #3 originally proposed resolving the drift by moving FE QR
generation into Pyodide-loaded `label.py`. That proposal carries
~6 MB cold-load weight, ties the project's encoder to Python forever,
and does nothing for the other surfaces the project will need
(standalone desktop, mobile scanner, embedded handheld).

The threat-model conversation captured in ADR-023 surfaced additional
constraints that bear on this choice:

- **Audit-grade integrity** for `Operator`, `AuditEntry`, `Signature`
  records, with forward-compatibility for Sigstore-keyless signing
  (deferred per ADR-024).
- **No bespoke key infrastructure for operators** beyond what
  existing IdP login provides.
- **12-factor configuration** (per ADR-021) with no hardcoded paths
  or repo names.
- **Structured logging + audit trail** (per ADR-022) emitted from a
  single tracing infrastructure, not scattered `print()` calls.
- **Storage as a port** (per ADR-018) so CSV+git, SQLite, DuckDB,
  Dolt, file-per-entry are interchangeable adapters.
- **Identity as a port** (per ADR-020) so GitHub OIDC, generic OIDC,
  mTLS, and offline modes share one trait.

Each of these is a cross-cutting requirement that benefits from a
language with first-class traits, a strong type system, deterministic
WASM output, and a single binary deploy story. Python (with or
without Pyodide) is workable but has weaker type discipline, heavier
WASM, and no native-binary CLI story without PyInstaller-grade
packaging.

The portability matrix the project will need over the next 12–24
months:

| Target | Need | Python+Pyodide | Rust |
|---|---|---|---|
| Browser FE (WASM) | Existing GH Pages SPA | ~6 MB cold | ~1–1.5 MB gzipped |
| CLI on dev machines | `mint`, `label`, `bind` | Works today | Single static binary |
| CLI on lab machines (no admin rights) | `label` for printing | Requires Python install | Single static binary, no install |
| Standalone desktop (macOS/Win/Linux) | Future packaging for non-CLI users | PyInstaller (~50 MB, fragile signing) | Tauri (~5 MB, signed natively) |
| iOS / Android scanner | Future bench mobile scanner | Kivy/BeeWare second-class, App Store friction | Tauri 2 mobile or `uniffi` bindings |
| Embedded (handheld scanner, RPi kiosk) | Possible future deployment | Heavy runtime | `cargo build --target arm-musleabihf` static binary |

## Alternatives considered

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| **Status quo: Python CLI + TS FE port** | Already working; no rewrite | Confirmed encoder drift; two implementations to maintain; no path to mobile / desktop / embedded; FE format support lags CLI; no shared validators | Rejected — drift is already a real bug, not a hypothetical |
| **Pyodide-loaded `label.py` in FE (issue #3 original plan)** | Eliminates encoder drift; reuses existing Python; smallest near-term diff | ~6 MB cold load + slow interpreter bootstrap; commits the project to Python forever; no mobile/desktop/embedded path; Pyodide WASM startup is multi-second on cold cache | Rejected — solves one problem (encoder drift) at the cost of foreclosing future surfaces |
| **TypeScript-everywhere** (port `label.py` and `validators/` to TS, drop Python entirely) | Single language across CLI (via Node/Deno/Bun) and FE; smallest WASM bundle (no WASM at all on FE) | TS for QR codec is hand-written cryptography territory; weaker type guarantees than Rust; Node-as-CLI is a runtime dependency on operator machines; no realistic mobile/embedded story | Rejected — wrong tool for codec work, no portability win over Rust |
| **Rust workspace, ports/adapters, single core compiles to native CLI binary + WASM for FE + Tauri/uniffi for desktop/mobile later** | One source of truth for codec, validators, policy; deterministic across native + WASM builds; first-class traits make ports/adapters real not nominal; smallest WASM bundle of the viable options; clear path to every future surface; matches the conformance-testing discipline (ADR-027) | Up-front porting cost (label.py, mint.py, bind.py, validators/ → Rust); team must include a Rust contributor; toolchain pinning required for reproducible builds | **Chosen** — the only option that addresses both the immediate drift bug and the multi-surface portability requirement |
| **Go workspace** | Strong cross-compilation story; simpler than Rust | WASM story is heavier (Go's runtime is ~2 MB minimum even for hello world); no production-quality QR + Micro QR encoder/decoder ecosystem; weaker compile-time guarantees | Rejected — WASM bundle penalty negates the FE win |

## Decision

The canonical implementation of all domain logic — QR codec (Standard
QR + Micro QR), label rendering, validators, policy engine, audit
log, identity provider abstraction, configuration parsing — moves to
a **Rust Cargo workspace** at the repo root.

The workspace compiles to:

- **Native CLI binaries** (`mint`, `label`, `bind`, replacing today's
  Python scripts).
- **WebAssembly module** consumed by the existing TypeScript FE
  (`web/src/`) via `wasm-pack --target web` + `vite-plugin-wasm`.
  The TS layer becomes a thin UI shell; all rendering, validation,
  and policy decisions are made in Rust.
- **(Future)** Tauri desktop app, Tauri 2 mobile, `uniffi` bindings
  for native iOS/Android UI, static binaries for embedded targets.
  None of these are committed by this ADR; the architecture
  preserves the option without forcing the spend.

The Python CLI (`label.py`, `mint.py`, `bind.py`, `validators/`,
`tools/sheet.py`) is retained as a **strangler-fig fallback** during
the migration, then deleted once the Rust binaries reach feature
parity (target: foundation phase, ~6 weeks per the conversation that
produced this ADR set).

## Workspace shape

```
crates/
  domain/              ports-internal types: PartId, Operator, Diff,
                       AuditEntry, ProposalRef, Signature, ChainHash.
                       Pure data, no I/O, no side effects.
  codec/               QR encode (Standard + Micro QR M4) + decode,
                       SVG label rendering. Wraps `qrcode-rust2` for
                       encode and `rxing` for decode (per the
                       research subagent report — see explorations/
                       once added).
  validators/          Pure functions over domain types. Schema,
                       sort-stability, FK integrity, semantic-diff
                       classifier per ADR-016. No I/O.
  storage/             Repository trait (per ADR-018). First adapter
                       crate `storage_csv_git/`. Adapter crates
                       `storage_sqlite/`, etc., land per future ADRs.
  identity/            IdentityProvider trait (per ADR-020). First
                       adapters `identity_git_config/` (CLI) and
                       `identity_github_oauth/` (FE).
  transport/           ProposalSink trait (per ADR-019). First adapter
                       `transport_github_pr/`. Future:
                       `transport_local_branch/`, `transport_webhook/`.
  signing/             SigningProvider + VerificationProvider traits
                       (per ADR-024). First adapter
                       `signing_git_commit/`. Future:
                       `signing_sigstore/`.
  observability/       `tracing` setup, audit-log subscriber, request
                       ID propagation (per ADR-022).
  config/              12-factor env-driven configuration loader (per
                       ADR-021). Typed parse-at-boundary; defaults
                       file shipped with binary.
  cli/                 Binaries: `mint`, `label`, `bind`. Wires the
                       MVP adapters together via `config`.
  wasm/                wasm-bindgen façade over codec + validators +
                       policy. Consumed by `web/src/`.
  port_tests/          Generic conformance + parity + drift-detection
                       test framework (per ADR-027). Each adapter
                       crate calls into it from its `tests/` dir.
```

Workspace `Cargo.toml` pins the toolchain and the wasm-bindgen CLI
version. CI runs `cargo check`, `cargo test`, `cargo clippy --
-Dwarnings`, `cargo build --target wasm32-unknown-unknown` on every
PR (per ADR-021 / ADR-027).

## Strangler-fig migration sequence

The Python implementation is not deleted up front; it stays in place
as a working fallback while the Rust workspace fills in. Order of
migration is from purest functions outward:

1. **`codec`** — Rust QR encoder (`qrcode-rust2`) + decoder (`rxing`),
   SVG layout primitives. Validate against current Python output via
   `port_tests` parity suite (golden files at known canonical IDs +
   sizes + formats). Accept the one-time SVG diff vs. current segno
   output (mask-selection differs); regenerate `examples/` from the
   Rust output and lock as the new baseline.
2. **`validators`** — port `validators/` package; add the semantic-diff
   classifier required by ADR-016.
3. **`domain` + `config`** — types and configuration loader.
4. **`storage_csv_git`** — Repository trait + first adapter, replicating
   today's `registry.csv` + `print_log.csv` read/write paths.
5. **`identity_git_config`** + **`signing_git_commit`** — MVP identity
   and signing per ADR-020 / ADR-024.
6. **`transport_github_pr`** — Proposal sink for the bind/edit
   pipeline currently stubbed in `web/src/`.
7. **`cli/mint`, `cli/label`, `cli/bind`** — replacement binaries. At
   this point the Python CLIs are deletable.
8. **`wasm`** — wasm-bindgen façade. FE swaps
   `web/src/layouts/qrcode-generator.ts` and `web/src/layouts/svg.ts`
   for calls into the WASM module. `web/src/layouts/svg.ts:48-68`
   gains real format support (4/4, 4/4/4, 5/5/4) for free since the
   Rust codec already has it.
9. **Delete** Python files. Keep this ADR as the migration record.

Each step lands behind its own PR. Each step has its own
`port_tests` conformance suite that must pass before the step is
considered complete (per ADR-027).

## Rationale

**Why Rust over Python+Pyodide.** The drift between segno and the
inline TS encoder is a *real bug today*, not a hypothetical risk.
Pyodide eliminates that bug at the cost of ~6 MB cold load and
foreclosing every future deploy surface beyond browser. Rust
eliminates the bug at the cost of an up-front port and gives the
project a viable path to every surface it will plausibly need within
24 months. The portability matrix above makes this concrete.

**Why ports/adapters as the architectural shape.** The ADR-023
threat model and the surrounding ADRs (018, 019, 020, 021, 022, 024)
all point at the same pattern: a small domain core with concerns
(storage, identity, transport, signing, observability, config) lifted
into traits with multiple implementations. Without ports/adapters,
each concern hardcodes one choice and dragging a different choice in
later is a refactor. With ports/adapters, each choice is an adapter
crate; adding SQLite alongside CSV+git is one new file in `storage/`,
not a refactor. Rust's traits + cargo's workspace model express this
pattern more cleanly than any other mainstream language at this size.

**Why a strangler-fig migration rather than a big-bang rewrite.** The
Python CLIs are in active use (the bootstrap batch
`B-2026-05-08-sheet-1` was minted and printed via `label.py --micro`
just two days ago). A big-bang rewrite would block all label
production for the duration of the migration. The strangler-fig
sequence keeps the Python path working until the corresponding Rust
binary is at parity; deletion happens only after parity is verified.

**Why pin the toolchain.** Reproducible builds are a requirement of
ADR-024 (signed releases must be re-buildable from the same source
commit). Rust's `rust-toolchain.toml` + `Cargo.lock` + pinned
wasm-bindgen CLI achieve this with no extra ceremony.

**Why `qrcode-rust2` for encode and `rxing` for decode.** The two
load-bearing requirements are Micro QR M4 generation and Micro QR
decoding in WASM. `qrcode-rust2` (active fork of `kennytm/qrcode`)
supports Micro QR including M4 and is actively maintained. `rxing`
(Rust port of zxing) supports Micro QR + DataMatrix decoding and
ships a `rxing-wasm` wrapper at ~1 MB gzipped. The empirical bundle
size (~1.0–1.5 MB gzipped for encode + decode combined) is 4–5×
smaller than Pyodide+segno.

## Consequences

- **Toolchain commitment**: Rust stable (current MSRV captured in
  `rust-toolchain.toml`), `wasm-pack`, `wasm-bindgen` CLI version
  pinned. Contributors install via `rustup`. CI matrix covers
  native + `wasm32-unknown-unknown`.
- **Reproducible-build discipline**: every PR's CI verifies the
  release build is byte-identical across two independent build hosts.
  Required by ADR-024 §4.
- **Conformance-test discipline**: every adapter crate must
  implement and pass the `port_tests` generic suites (per ADR-027).
  CI rejects PRs that introduce a new adapter without conformance
  tests.
- **Strangler-fig hygiene**: the Python CLIs stay green while they
  exist. Their tests still pass. Their entry points still work.
  Removing a Python file requires evidence (in the PR description)
  that the corresponding Rust binary has reached parity and is in
  use.
- **One-time SVG output diff**: `qrcode-rust2`'s mask-selection
  produces a different (still ISO-conformant) Standard QR matrix
  than `segno`. This means: regenerated `examples/` will not be
  byte-identical to the current ones. Visual / decode behaviour is
  unchanged. Deliberate baseline reset, captured in `LOG.md` when
  the migration step lands.
- **Web bundle change**: FE eventually loads a ~1.0–1.5 MB gzipped
  WASM module instead of a ~10 KB inline TS encoder. Acceptable for
  an installed PWA used daily; mitigated by long-cache headers and
  `vite-plugin-wasm`'s code-splitting.
- **No more inline JS QR encoder**: `web/src/layouts/qrcode-generator.ts`
  is deletable at strangler-fig step 8. Its license attribution
  notice migrates to the Rust crate's NOTICE if `qrcode-rust2`
  carries equivalent BSD-2-Clause terms.
- **Dependency on `rxing` upstream**: `rxing`'s Micro QR decode path
  is less battle-tested than `zxing-wasm`'s. Pre-cutover A/B test
  required against the existing `zxing-wasm` decoder on a corpus of
  real-world Micro QR scans. Captured as a step-8 acceptance
  criterion.
- **ADR-014 §"Pyodide migration trigger" is closed**, not by
  pulling the trigger, but by changing the destination. ADR-014's
  body is otherwise unaffected (Tab/Layout/Plugin extension model,
  three SSOTs, Error Report plugin demo all stand).
- **Issue #3 (Pyodide PRIORITY)** is superseded by this ADR and
  closed; new issues will track the strangler-fig steps.
- **Issue #13 phases 2–3 (Micro QR FE)** unblock as soon as
  strangler-fig step 8 lands.

## Open questions / supersession triggers

- Whether to ship Tauri / Tauri-mobile / `uniffi` adapters in the
  foundation phase or wait for an explicit deployment trigger. This
  ADR preserves the option but does not commit. Re-opens if a
  desktop-packaging or mobile-scanner requirement is filed.
- Whether `validators` should also be exposed via FFI for use by
  external integrators (e.g. a customer's QMS hooking into our
  policy engine). Out of scope for the foundation phase; re-opens
  if a customer asks.
- Whether the Python CLI deletion should be batched (one PR at end
  of migration) or step-wise (each Python file deleted in the same
  PR that retires it). Methodology accepts either; preference is
  step-wise for cleaner audit trail. Decision deferred to migration
  execution time.
- Whether `qrcode-rust2` is the right pin or whether we should
  vendor it (its dormant-but-functional upstream `kennytm/qrcode`
  is a maintenance risk; the active fork is one maintainer). If the
  fork goes dormant we may need to vendor or fork-of-the-fork.
  Re-opens if `qrcode-rust2` shows >9 months without commits.

## References

- [ADR-013 — Parts registry web app](ADR-013-parts-registry-web-app.md)
- [ADR-014 — Web app architecture](ADR-014-web-app-architecture.md)
- [ADR-016 — PR-diff-based policy enforcement](ADR-016-pr-diff-policy-enforcement.md)
- [ADR-018 — Storage as a port](ADR-018-storage-port.md)
- [ADR-019 — Proposal sink as a port](ADR-019-proposal-sink-port.md)
- [ADR-020 — Identity & authorization as a port](ADR-020-identity-authorization-port.md)
- [ADR-021 — Configuration model (12-factor)](ADR-021-configuration-12-factor.md)
- [ADR-022 — Observability: tracing + audit trail](ADR-022-observability-tracing-audit.md)
- [ADR-023 — Threat model + crypto-MVP scope](ADR-023-threat-model-and-crypto-mvp-scope.md)
- [ADR-024 — Cryptographic baseline (MVP)](ADR-024-crypto-baseline-mvp.md)
- [ADR-027 — Port conformance + forward-compatibility tests](ADR-027-port-conformance-tests.md)
- Issue #3 — Pyodide PRIORITY (superseded by this ADR)
- Issue #13 — Micro QR phases 2–3 (unblocks at strangler-fig step 8)
- `qrcode-rust2` — <https://github.com/sorairolake/qrcode-rust2>
- `rxing` — <https://github.com/rxing-core/rxing>
- `wasm-pack` — <https://rustwasm.github.io/wasm-pack/>
- `vite-plugin-wasm` — <https://www.npmjs.com/package/vite-plugin-wasm>
- Hexagonal / ports & adapters — Alistair Cockburn, 2005
