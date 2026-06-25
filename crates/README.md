# `crates/` — Rust workspace

Foundation scaffold for the Rust core called for by ADR-017
(`decisions/ADR-017-rust-core-ports-adapters.md`).

This is the **scaffold**, not the implementation. Trait surfaces and
domain types are pinned per the cited ADRs; method bodies return
`unimplemented!()` or `Err(_::Other("not implemented (foundation
scaffold)"))`. Production logic lands in subsequent PRs as the
strangler-fig sequence (ADR-017 §"Strangler-fig migration sequence")
progresses.

## Workspace shape

```
crates/
  domain/                    pure data types (PartId, Operator, AuditEntry, …)
  codec/                     QR encode + decode + SVG label rendering
  validators/                pure-function validators + ADR-016 classifier
  storage/                   Repository trait (ADR-018)
  storage_csv_git/           CSV+git adapter (ADR-018, MVP)
  identity/                  IdentityProvider + Authorizer traits (ADR-020)
  identity_git_config/       CLI identity adapter (ADR-020)
  identity_github_oauth/     FE identity adapter (ADR-020)
  transport/                 ProposalSink trait (ADR-019)
  transport_github_pr/       GitHub-PR adapter (ADR-019, MVP)
  signing/                   SigningProvider + VerificationProvider (ADR-024)
  signing_git_commit/        Git-commit signing adapter (ADR-024, MVP)
  observability/             tracing + audit-log subscriber init (ADR-022)
  config/                    12-factor configuration loader (ADR-021)
  cli/                       mint / label / bind binaries
  devtools/                  repo gate tooling (obligations-check, ADR-029)
  wasm/                      wasm-bindgen façade for the FE
  port_tests/                conformance + parity + drift framework (ADR-027)
```

## Strangler-fig migration sequence

Per ADR-017 §"Strangler-fig migration sequence", the legacy Python
CLIs remained the canonical implementation **until the corresponding
Rust crate reached parity**. Order:

1. `codec` — QR encoder / decoder, SVG layout primitives.
2. `validators` — port the Python validators; add ADR-016 classifier.
3. `domain` + `config` — types and configuration loader.
4. `storage_csv_git` — Repository trait + first adapter.
5. `identity_git_config` + `signing_git_commit` — MVP identity / signing.
6. `transport_github_pr` — ProposalSink for bind/edit pipeline.
7. `cli/{mint,label,bind}` — replacement binaries; Python deletable.
8. `wasm` — wasm-bindgen façade; FE swaps inline TS encoder.
9. Delete Python.

Each step lands as its own PR. Each step has its own `port_tests`
conformance suite that must pass before the step is considered
complete.

Step 9 executed 2026-06-12: the operational Python (mint/label/bind,
validators, tools/sheet.py, tools/obligations_check.py and friends) is
gone. Parity evidence stays executable as the golden suite in
`cli/tests/label_parity_golden.rs` + `cli/tests/golden/`. The two
design-time font tools (`tools/bake_glyph_font.py`,
`tools/font_editor_gen.py`) are the only remaining Python, pending
their own Rust port.

## Running

```sh
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -Dwarnings
cargo fmt --all
cargo build --target wasm32-unknown-unknown -p qx-wasm
```

The pinned toolchain is in `rust-toolchain.toml` (channel `1.85.0`,
components `rustfmt` + `clippy`, target `wasm32-unknown-unknown`).
`rustup` honours this on first invocation.

## Conformance-test discipline

Per ADR-027, every adapter crate ships a `tests/conformance.rs` that
calls the matching `port_tests::*_conformance` generic. CI rejects
adapter PRs that omit it (drift-test enforcement). Today the
generic functions are placeholders; real assertion bodies land
alongside each strangler-fig PR.

## What is intentionally NOT here

- Production trait method bodies (return `unimplemented!()`).
- Real subscriber layers in `observability` (empty registry).
- Real adapter selection wiring in `cli/` (binaries print and exit).
- Drift-test source scanners (regex bodies are TODOs).
- The two-host reproducible-build CI matrix per ADR-024 §Reproducible
  builds — the workflow runs on one host today; the second host wires
  in once a release tag exists.
