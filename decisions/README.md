# Decision records — part-registry

Markdown ADRs (Architecture Decision Records) for the parts registry.
Each ADR captures one decision — its context, the alternatives that
were considered, the chosen path, and the consequences — so that an
auditor or a new engineer can reconstruct *why* a choice was made and
*what* would invalidate it.

Methodology, status taxonomy, and template are mirrored from the
`MorePET/exopet` decisions framework; see [`METHODOLOGY.md`](METHODOLOGY.md).

## How this folder is organized

- [`METHODOLOGY.md`](METHODOLOGY.md) — playbook for how decisions are
  made and recorded.
- [`LOG.md`](LOG.md) — append-only chronological process trail.
- [`ADR-template.md`](ADR-template.md) — copy this when adding a new ADR.

## Status taxonomy

Six allowed values, defined in full in [`METHODOLOGY.md`](METHODOLOGY.md):

- **Accepted** — in force, governs current work.
- **Proposed** — recommended, awaiting team review.
- **Research** — architectural exploration; not a commitment.
- **Deferred** — postponed with explicit trigger conditions.
- **Rejected** — considered and decided against.
- **Superseded by ADR-NNN** — replaced by a newer ADR.

## Naming convention

`ADR-NNN-short-slug.md`. Numbers continue from `MorePET/exopet`'s
sequence — ADR-001 through ADR-011 are exopet hardware decisions and
stay there; ADR-012 and ADR-013 are the parts-identification scheme
and the registry web app, respectively, and live here. New ADRs in
this repo continue from ADR-014.

## Index

### Accepted

_(none yet — all ADRs are Proposed pending team review.)_

### Proposed

Foundation set (architectural reset, 2026-05-10 — see [LOG.md](LOG.md)):

- [ADR-017 — Rust core, ports/adapters, multi-target deploy](ADR-017-rust-core-ports-adapters.md)
- [ADR-018 — Storage as a port (CSV+git first; SQLite/DuckDB/Dolt/file-per-entry future)](ADR-018-storage-port.md)
- [ADR-019 — Proposal sink as a port (GitHub PR first; local-branch/webhook/filesystem future)](ADR-019-proposal-sink-port.md)
- [ADR-020 — Identity & authorization as a port (git-config + GitHub OAuth first; OIDC/mTLS/Sigstore future)](ADR-020-identity-authorization-port.md)
- [ADR-021 — Configuration model (12-factor: env-first, typed parse-at-boundary)](ADR-021-configuration-12-factor.md)
- [ADR-022 — Observability: structured tracing + audit log + request_id propagation](ADR-022-observability-tracing-audit.md)
- [ADR-023 — Threat model + crypto-MVP scope (the keystone for ADRs 020/022/024)](ADR-023-threat-model-and-crypto-mvp-scope.md)
- [ADR-024 — Cryptographic baseline MVP + bolt-on path to full crypto](ADR-024-crypto-baseline-mvp.md)
- [ADR-025 — Distribution integrity (signed releases, SRI, repro builds, future Cosign)](ADR-025-distribution-integrity.md)
- [ADR-027 — Port conformance + forward-compatibility test framework](ADR-027-port-conformance-tests.md)

Earlier ADRs (still Proposed; ADR-016 updated 2026-05-10 to align with foundation set):

- [ADR-012 — Part identification: nano-id + QR labels with mint-then-bind workflow](ADR-012-part-identification.md)
- [ADR-013 — Parts registry web app: GH Pages + WASM DuckDB + PR-driven binds](ADR-013-parts-registry-web-app.md)
- [ADR-014 — Web app architecture: extension interfaces, SSOT, plugin model](ADR-014-web-app-architecture.md) _(§"Pyodide migration trigger" superseded by ADR-017)_
- [ADR-015 — Print event log: non-destructive audit trail of every label print](ADR-015-print-event-log.md) _(generalised by ADR-022 audit-log layer; print_log.csv migration tracked in #34)_
- [ADR-016 — PR-diff-based policy enforcement for registry changes](ADR-016-pr-diff-policy-enforcement.md) _(open question on Pyodide vs TS-port resolved by ADR-017: validators are Rust)_
