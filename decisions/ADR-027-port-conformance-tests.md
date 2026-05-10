# ADR-027 — Port conformance + forward-compatibility tests

- Status: Accepted
- Date: 2026-05-10
- Component / area: cross-cutting — defines the test discipline that
  enforces the ports/adapters architecture committed in ADR-017
- Reviewers: Lars Gerchow
- Related: ADR-017 (Rust core + ports/adapters), ADR-018 (Storage
  port), ADR-019 (Proposal sink port), ADR-020 (Identity port),
  ADR-022 (Observability), ADR-023 (Threat model + crypto MVP),
  ADR-024 (Crypto baseline)

## Context

ADR-017 commits the project to a Rust workspace structured as a small
domain core with concerns (storage, identity, transport, signing,
observability, configuration) lifted into traits with multiple
adapter crates. That commitment is only meaningful if the trait
contract is *enforced* — i.e. if every adapter is held to the same
behavioural specification, and if the architectural shape itself is
defended against PR-by-PR drift.

The failure modes ports/adapters discipline is supposed to prevent
are well-known and have already been observed in this project's
pre-Rust state:

- **Trait drift.** Two adapters claim to implement the same trait but
  behave differently for the same input. The QR-encoder split between
  Python `segno` and the inline TS encoder (ADR-017 §Context) is the
  canonical example: same payload, both decodable, different bit
  matrices. Without a generic conformance suite, the trait is a
  suggestion, not a contract.
- **Schema-shape drift.** The MVP data model is designed
  forward-compatible (per ADR-023 §Decision): `AuditEntry` reserves
  `signatures: Vec<Signature>` and `chain_hash: Option<Hash>` so
  Sigstore activation (deferred per ADR-024) is an adapter swap, not
  a schema migration. That guarantee only holds if every storage
  adapter is actually exercised against a forward-shaped record. A
  storage adapter that silently drops `signatures` because today's
  code paths never populate them breaks the deferred-control
  activation path the threat model depends on.
- **Cross-adapter divergence.** When two adapters back the same trait
  (e.g. CSV-on-git storage and SQLite storage), an integration that
  works against one must work against the other. Without a parity
  suite, the registry's behaviour silently depends on which adapter
  is wired in via `config` (per ADR-021).
- **Architectural decay.** ADR-021 forbids hardcoded paths; ADR-022
  forbids unstructured logging; ADR-020 requires every mutation to
  carry an `&Operator`. None of these are enforceable by `cargo
  clippy` alone. A PR that introduces `println!("writing to
  /tmp/foo")` in a mutation function violates three accepted ADRs at
  once and will pass every unit test the adapter author writes.

Conventional unit tests and integration tests are necessary but not
sufficient for any of the above. Unit tests verify *one adapter's
behaviour against itself*. Integration tests verify *one wired
configuration end-to-end*. Neither defends the trait contract, the
forward-shape commitment, the cross-adapter parity, or the
architectural invariants.

The Rust workspace shape from ADR-017 already includes a
`crates/port_tests/` slot for this discipline. This ADR fixes the
shape of what lives in that crate and the obligations every adapter
crate inherits from it.

## Alternatives considered

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| **No conformance discipline** — adapters tested ad-hoc per crate by their author | Lowest friction; no shared framework to maintain | Trait drift inevitable (already empirically observed pre-Rust per ADR-017 §Context); no defence against schema-shape drift; cross-adapter divergence undetectable until production; auditor has no evidence the trait contract is enforced | Rejected — this is the pre-Rust state the migration is meant to fix |
| **Adapter-specific tests only, no shared trait suite** — each adapter author writes their own tests, but no generic `fn test_<port>_provider<P: Port>(p: P)` exists | Adapter authors retain full control; no abstraction to learn | Trait contract is not enforced — two authors can interpret the same trait method differently and both pass their own tests; auditor cannot answer "what does this trait promise" by pointing at one file | Rejected — defeats the purpose of having a trait |
| **Conformance + parity, but no forward-shape or drift tests** | Covers trait contract and cross-adapter equivalence; cheaper to maintain than four tiers | Forward-shape is the load-bearing guarantee that lets ADR-024 defer Sigstore without a future schema migration (per ADR-023 §Decision item 6); drift tests are what prevent architectural decay PR-by-PR; both are exactly the controls that fail silently when omitted | Rejected — the two omitted tiers are the ones that defend deferred decisions and accepted invariants |
| **Four-tier discipline** — conformance + forward-shape + cross-adapter parity + drift-detection (lint-as-test), all expressed in `crates/port_tests/`, all required by CI for every adapter | Each tier defends a distinct failure mode named in Context; framework is one crate; obligations are mechanically checkable; auditor can point at one test crate to evidence trait enforcement | Up-front cost of writing the generic test functions; adapter authors must wire their adapter into the suite; CI configuration to reject non-conforming PRs | **Chosen** — the only option that defends every failure mode the ADR-017 architecture is exposed to |
| **Property-based testing only** (`proptest` / `quickcheck` over trait methods, no concrete tier separation) | Generative coverage; finds edge cases humans miss | Property tests express what's true *for arbitrary input*; they do not express what's true *across adapters* (parity), do not test forward-shaped records (the schema-evolution case is not generatable from today's domain types alone), and do not detect architectural-invariant violations in source | Rejected as a substitute — adopted as a *complement* within the conformance tier where generative coverage adds value |

## Decision

A four-tier port test discipline lives in **`crates/port_tests/`** and
is mandatory for every adapter crate in the workspace. The four tiers,
the failure mode each defends, and the obligation on adapter crates
are fixed below.

### Tier 1 — Trait conformance suite

Generic conformance functions live in `crates/port_tests/src/<port>.rs`
with the shape:

```rust
// crates/port_tests/src/storage.rs
pub fn test_storage_provider<S: Storage>(mut s: S) {
    test_roundtrip_part(&mut s);
    test_roundtrip_audit_entry(&mut s);
    test_error_on_missing_part(&mut s);
    test_error_on_duplicate_id(&mut s);
    test_query_stable_ordering(&mut s);
    test_concurrent_write_rejected(&mut s);
    // … one assertion per contract clause the trait promises
}
```

Every adapter crate calls the matching `test_<port>_provider` from its
own `tests/` directory:

```rust
// crates/storage_csv_git/tests/conformance.rs
#[test]
fn csv_git_conforms() {
    let s = storage_csv_git::CsvGitStorage::new_in_tempdir();
    port_tests::storage::test_storage_provider(s);
}
```

Conformance covers:
- **Roundtrip**: every `write` followed by the matching `read` returns
  an equal value.
- **Error cases**: every error variant the trait declares is reachable
  via at least one input.
- **Edge inputs**: empty collections, unicode in identifiers, the
  largest payload size the trait permits, the smallest non-empty
  payload.
- **Contract invariants**: ordering guarantees, idempotency
  guarantees, concurrency guarantees the trait's docstring promises.

The generic function is the **single source of truth** for what the
trait promises. A trait method whose contract is not exercised by
`test_<port>_provider` is, for audit purposes, undefined.

### Tier 2 — Forward-shape tests

Today's data model accepts tomorrow's data. Storage adapters must
round-trip records whose shape MVP code paths do not produce, so that
when a deferred control activates (per ADR-023 re-open triggers) the
schema does not need to change.

The load-bearing case fixed by ADR-023 §Decision item 6:

```rust
// crates/port_tests/src/storage.rs
pub fn test_storage_roundtrips_sigstore_shaped_audit_entry<S: Storage>(s: &mut S) {
    let entry = AuditEntry {
        // … MVP fields populated normally …
        signatures: vec![
            Signature::Sigstore {
                cert_chain: synth_cert_chain(),
                rekor_uuid: synth_rekor_uuid(),
                bundle: synth_sigstore_bundle(),
            },
        ],
        chain_hash: Some(synth_hash()),
    };
    s.write_audit_entry(&entry).unwrap();
    let read = s.read_audit_entry(&entry.id).unwrap();
    assert_eq!(entry, read);   // every byte round-trips, including
                                // the Sigstore-shaped signature MVP
                                // does not produce
}
```

This is the "YAGNI but design for the Y" discipline: the project is
not implementing Sigstore today (deferred per ADR-024), but it *is*
designing the data model so adding Sigstore later is an adapter swap,
not a redesign. The forward-shape test is the mechanical guarantee
that the swap remains an adapter swap.

Forward-shape tests are required for every port whose trait or data
type reserves space for a deferred control. The current set, derived
from ADR-023 deferred items:

- **Storage**: round-trip an `AuditEntry` with a `Sigstore`-variant
  `Signature` and a populated `chain_hash` (defends T2, T4, T5).
- **Storage**: round-trip an `AuditEntry` with multiple `Signature`
  entries (defends T4 — per-row vs. per-commit attribution).
- **Identity**: an `Operator` with `source: "oidc:<issuer>"` and a
  populated `verified_at` round-trips through every identity adapter,
  even adapters whose MVP code paths only emit
  `source: "git_commit_author"` (defends T1, T3).
- **Signing**: a `VerificationRequest` carrying a Rekor anchor
  reference parses and validates through every verifier adapter
  (defends T2, T5).

When ADR-023 grows a re-open trigger, the corresponding forward-shape
test is added in the same PR that activates the deferred control —
not after.

### Tier 3 — Cross-adapter parity

When two adapters back the same port, they must produce the same
result for the same input. Parity tests live in
`crates/port_tests/src/parity/` and take a pair of adapters:

```rust
// crates/port_tests/src/parity/storage.rs
pub fn test_storage_parity<A: Storage, B: Storage>(mut a: A, mut b: B) {
    let parts = synth_part_corpus();
    for p in &parts {
        a.write_part(p).unwrap();
        b.write_part(p).unwrap();
    }
    for query in synth_query_corpus() {
        let from_a = a.query_parts(&query).unwrap();
        let from_b = b.query_parts(&query).unwrap();
        assert_eq!(from_a, from_b,
            "storage adapters disagree on query {query:?}");
    }
}
```

Parity obligations the workspace inherits:

- **Storage parity**: `storage_csv_git` and any future
  `storage_sqlite` (or `storage_duckdb`, `storage_dolt`,
  `storage_file_per_entry`) return identical `Vec<Part>` for identical
  queries against identical writes. Required by ADR-018.
- **Codec parity**: the QR encoder and decoder roundtrip across
  adapter pairs. For every payload in a fixed corpus, `decode(encode(p))
  == p` regardless of which encoder/decoder pair is wired in.
  Required by ADR-017 (the original drift bug).
- **Identity parity**: `identity_git_config` (CLI) and
  `identity_github_oauth` (FE) produce `Operator` records that
  compare equal on the subset of fields the trait promises are
  identity-stable (typically `subject`, `source`), even though
  side-channel fields (`verified_at`, transport metadata) legitimately
  differ.

Parity tests are the empirical answer to "does this trait actually
abstract over its implementations". A parity test failure is either a
trait-contract bug (the trait does not specify enough) or an adapter
bug (the adapter violates the trait's specification) — both are
load-bearing findings.

### Tier 4 — Drift-detection (lint-as-test)

Architectural invariants survive PRs only if a test fails when they
are violated. `cargo clippy` covers code-quality lints; this tier
covers *project-specific architectural invariants* that no off-the-
shelf linter knows about. These tests live in
`crates/port_tests/src/drift/` and run against the workspace source
tree:

```rust
// crates/port_tests/src/drift/source_invariants.rs
#[test]
fn no_hardcoded_paths_in_source() {
    let violations = scan_workspace_source(|file, line| {
        contains_hardcoded_path(line)
            && !is_in_allowlisted_test_file(file)
    });
    assert!(violations.is_empty(),
        "hardcoded paths found (ADR-021 violation):\n{violations:#?}");
}

#[test]
fn no_println_outside_cli_main() {
    let violations = scan_workspace_source(|file, line| {
        (line.contains("println!") || line.contains("eprintln!"))
            && !is_cli_main(file)
    });
    assert!(violations.is_empty(),
        "unstructured logging found (ADR-022 violation):\n{violations:#?}");
}

#[test]
fn mutation_functions_take_operator() {
    let violations = scan_trait_methods(|trait_name, method| {
        is_mutation(method) && !takes_operator_param(method)
    });
    assert!(violations.is_empty(),
        "mutation method missing &Operator (ADR-020 violation):\n{violations:#?}");
}
```

The current set of architectural invariants drift-tested:

- **No hardcoded paths or repository names** in source (per ADR-021,
  12-factor configuration). Allowlisted: test fixtures and the
  configuration defaults file shipped with the binary.
- **No `print!` / `println!` / `eprint!` / `eprintln!`** outside
  CLI binary `main.rs` files (per ADR-022, structured `tracing`-based
  logging). The CLI binaries themselves emit human output via
  `tracing`'s pretty subscriber; library crates emit only structured
  events.
- **Every mutation method on every port trait takes `&Operator`**
  (per ADR-020, identity-on-mutation). A mutation that does not
  carry the operator cannot produce a complete audit-log entry.
- **No use of `unwrap()` or `expect()` in non-test library code**
  outside an allowlisted set with a comment justification (per the
  Rust-core panic discipline implied by ADR-017).
- **Every adapter crate has a `tests/conformance.rs` calling the
  matching `test_<port>_provider`** (Tier 1 obligation, mechanically
  checked).

Drift tests run as part of `cargo test` in the `port_tests` crate and
therefore as part of CI on every PR.

## Rationale

**Why four tiers, not one.** Each tier defends a distinct failure
mode named in Context. Conformance defends the trait contract.
Forward-shape defends the deferred-control activation path that
ADR-023 depends on. Parity defends the trait abstraction itself
(traits that two adapters interpret differently are not abstractions).
Drift-detection defends architectural invariants that no individual
adapter author owns. Collapsing the tiers loses the failure mode the
collapsed tier was defending — the rejected
"Conformance + parity, no forward-shape or drift" alternative
demonstrates exactly which failure modes go undefended when the tiers
are pruned.

**Why a generic `fn test_<port>_provider<P: Port>(p: P)` and not a
trait-with-default-tests pattern.** Rust traits cannot carry test
functions in a way that makes them runnable from an arbitrary
adapter's `tests/` directory. The generic function pattern is the
idiomatic way to express "every implementor of this trait must pass
this fixed suite" while keeping each adapter's test-binary independent
(adapter-specific failure modes show up against the adapter, not in a
shared test runner). This pattern is widely used in the Rust
ecosystem (e.g. `embedded-hal` conformance fixtures, `tokio`'s
runtime parity tests).

**Why forward-shape tests are mandatory, not best-effort.** ADR-023
defers Sigstore-keyless signing, hash-chained audit, per-row
attribution, and several other controls on the explicit guarantee
that they activate as adapter swaps without a schema migration. That
guarantee is the entire reason ADR-023 is acceptable as an MVP scope:
without forward-compatibility, deferring these controls would mean
re-engineering the data model when a re-open trigger fires, which
turns "deferred" into "indefinitely postponed". The forward-shape
test is the only mechanical evidence that the guarantee holds. An
auditor reading ADR-023's deferred list and asking "what makes you
confident you can activate these later" is answered by pointing at
the forward-shape suite.

**Why parity tests across adapters and not just within.** ADR-018
(storage as a port) and ADR-019 (proposal sink as a port) explicitly
contemplate multiple adapters per port — the value of the
ports/adapters architecture is the option to swap. That option is
worthless if a swap silently changes behaviour. Parity tests are the
discipline that turns "we *could* swap" into "we *can* swap and the
auditor can read the proof".

**Why drift-detection at the test layer rather than in CI scripts.**
Lint-as-test runs in the same `cargo test` invocation as every other
test, lives in the same crate as the conformance suite, and is
visible to every developer running tests locally. Putting these
checks in a CI-only YAML script hides them from local development and
makes them feel external to the codebase. Treating architectural
invariants as tests (with clear failure messages naming the violated
ADR) makes the invariants part of the codebase's voice. The cost is
slightly slower `cargo test`; the benefit is that violations surface
at the same moment as regular test failures.

**Why CI rejects PRs that introduce a new adapter without
conformance tests.** The discipline only works if it's mechanical.
A "please remember to add conformance tests" code-review norm fails
the first time a tired reviewer waves a PR through. CI enforcement
makes the obligation un-skippable: the new adapter crate without
`tests/conformance.rs` calling `test_<port>_provider` fails the
drift-detection test that checks the obligation, and the PR cannot
merge.

## Consequences

This ADR commits the project to:

- **One framework crate**: `crates/port_tests/` exists and depends
  only on `domain` and the trait-defining crates (`storage`,
  `identity`, `transport`, `signing`). It does not depend on any
  adapter crate (which would create a dependency cycle); adapters
  depend on `port_tests` as a dev-dependency only.
- **Per-adapter conformance obligation**: every adapter crate in
  `crates/` has a `tests/conformance.rs` that wires its adapter into
  the matching generic `test_<port>_provider`. CI rejects an adapter
  PR without this file via the drift test in Tier 4.
- **Forward-shape obligation per port with deferred controls**:
  storage, identity, and signing carry forward-shape tests today
  (per ADR-023's deferred list). Adding a new deferred control to
  any other port adds a forward-shape test to `port_tests` in the
  same PR that defers the control.
- **Parity obligation when a second adapter lands**: the second
  adapter for any port lands together with a parity test pairing it
  with the first adapter. The first adapter does not need to add
  parity tests retroactively (there is nothing to pair against); the
  second adapter does.
- **Drift-test maintenance**: when ADR-021, ADR-022, or any other
  invariant-bearing ADR lands or changes, the corresponding
  drift-detection test is added or updated in the same PR. The drift
  test's failure message names the ADR being defended, so an engineer
  hitting the failure can find the rationale.
- **CI matrix**: `cargo test -p port_tests` runs on every PR
  alongside `cargo test --workspace`. A failure in `port_tests` is a
  blocking failure regardless of which crate the diff touched (a
  drift test can fail because of a hardcoded path introduced
  anywhere in the workspace).
- **`port_tests` is itself testable**: the framework's own helpers
  (corpus generators, source scanners, `synth_*` builders) have
  unit tests. A bug in a drift scanner that fails to flag a real
  violation is a higher-priority bug than the violation it failed
  to flag.
- **Documentation surface**: each generic `test_<port>_provider`
  carries a docstring listing the contract clauses it asserts, with
  `// per ADR-NNN §X` references. The generic functions become the
  human-readable trait specification.
- **Property-based testing as a complement**: `proptest` is permitted
  inside conformance functions where generative coverage adds value
  (e.g. `proptest!{ |(p in arb_part())| roundtrip(p) }`) but does
  not replace the fixed-corpus tests that defend named edge cases.

This ADR does **not** commit the project to:

- Mutation testing or fuzz harnesses (out of scope for the
  foundation phase; re-openable as a successor ADR).
- Cross-language conformance (e.g. testing the Python strangler-fig
  CLI against the Rust trait suite). The Python path is being
  deleted per ADR-017's strangler-fig sequence; testing it against
  the Rust contract would be effort against code with a known
  end-of-life date.
- A specific source-scanning library for the drift tier. Hand-rolled
  walkers using `walkdir` + `regex` are sufficient for the current
  invariant set; adopting `syn`-based AST scanning is a future
  optimisation if invariants grow more structural.

## Open questions / supersession triggers

- Whether parity tests should run as a fixed pair (CSV vs. SQLite)
  or as an N-by-N matrix (every adapter against every other adapter
  for the same port). N-by-N grows quadratically; fixed-pair scales
  linearly. Decision deferred to when the second storage adapter
  actually lands. Re-opens if a third storage adapter is proposed.
- Whether the drift-detection scanners should be written against
  source text (`walkdir` + `regex`) or against AST (`syn`). Source
  text is faster to write and adequate for the current invariants;
  AST is more precise but adds compile-time cost to `port_tests`.
  Re-opens if a drift test produces a false positive that source-
  text scanning cannot fix without an unreasonable allowlist.
- Whether adapter crates should be allowed to *extend* the
  conformance suite with adapter-specific extra tests in the same
  `conformance.rs` file, or whether those must live in a separate
  test file to keep the conformance call site uncluttered. Style
  preference; defer to the first adapter that wants to add extra
  tests.
- Whether forward-shape tests should be marked `#[ignore]` until the
  corresponding deferred control activates, or always run. Currently
  always-run, because a forward-shape regression in MVP code (a
  storage adapter that drops `signatures: Vec<Signature>` because
  the field is empty) is the exact failure the tier defends against.
  Re-opens if always-running becomes a CI cost burden.
- Whether `port_tests` should expose its corpora (the fixed sets of
  parts, queries, payloads) as published fixtures for downstream
  integrators (a customer's QMS hooking into our policy engine, per
  ADR-017's open question). Out of scope today; re-opens if a
  customer asks.

## References

- [ADR-017 — Rust core, ports/adapters, multi-target deploy](ADR-017-rust-core-ports-adapters.md)
- [ADR-018 — Storage as a port](ADR-018-storage-port.md)
- [ADR-019 — Proposal sink as a port](ADR-019-proposal-sink-port.md)
- [ADR-020 — Identity & authorization as a port](ADR-020-identity-authorization-port.md)
- [ADR-021 — Configuration model (12-factor)](ADR-021-configuration-12-factor.md)
- [ADR-022 — Observability: tracing + audit trail](ADR-022-observability-tracing-audit.md)
- [ADR-023 — Threat model + crypto-MVP scope](ADR-023-threat-model-and-crypto-mvp-scope.md)
- [ADR-024 — Cryptographic baseline (MVP)](ADR-024-crypto-baseline-mvp.md)
- [`METHODOLOGY.md`](METHODOLOGY.md) — audit principles, citation
  discipline
- ISO 13485:2016 §7.3.7 (Design and development verification)
- IEC 62304:2006/AMD1:2015 §5.6 (Software integration and integration
  testing)
- `embedded-hal` conformance fixtures —
  <https://github.com/rust-embedded/embedded-hal>
- Hexagonal / ports & adapters — Alistair Cockburn, 2005
