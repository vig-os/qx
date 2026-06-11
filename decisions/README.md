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
- [ADR-028 — SOUP validation per IEC 62304 §5.3 + §8.1.2 (Class B; H1–H8 harnesses; surveillance plan)](ADR-028-soup-validation.md)
- [ADR-029 — Architectural coverage validator (`coverage.toml` + prek/CI tool)](ADR-029-architectural-coverage-validator.md)

Multi-tier app design set (2026-06-10/11 — accepted 2026-06-11 after a
two-agent generalization review; see [LOG.md](LOG.md)):

- [ADR-030 — Multi-tier shells over one application layer + command protocol (CLI/TUI/serve/MCP/web/Tauri; supersedes ADR-014)](ADR-030-multi-tier-shells-and-application-layer.md)
- [ADR-031 — Label rendering + structured print-request model (px-true QR, padding-fill, optimistic mint+print fast-path)](ADR-031-label-render-print-request-model.md)
- [ADR-032 — Scan pipeline + `decode-image` (one processor over still/video/live; drop zxing-wasm)](ADR-032-scan-pipeline-decode-image.md)
- [ADR-033 — Registry anatomy: self-describing data repo (own versioned contract; core+custom schema) + operator workspace](ADR-033-registry-anatomy-self-describing.md)
- [ADR-034 — Registry manifest + capabilities (host-enforced authz via branch protection + CODEOWNERS; tool classifies/advises)](ADR-034-registry-manifest-capabilities.md)
- [ADR-035 — Registry data model: collections metamodel (parts/types/vendors/… as declared collections; `batch` retired) + tiered schema + JSONL](ADR-035-part-data-model-tiered-schema-jsonl.md)

Earlier accepted:

- [ADR-016 — PR-diff-based policy enforcement for registry changes](ADR-016-pr-diff-policy-enforcement.md) _(change classes generalized to `{collection, op-kind}` by ADR-035 §0)_

### Proposed

Audit-evidence set (2026-06-11 session — identity, trail integrity,
gate lifecycle; see [LOG.md](LOG.md)):

- [ADR-036 — Audit identity: personas collection + host-resolved accountability](ADR-036-audit-identity-personas.md)
- [ADR-037 — Audit-trail integrity: checkpoints, merge-sync, tool provenance, anchor ledger](ADR-037-audit-trail-integrity-anchoring.md)
- [ADR-038 — Gate artifact lifecycle: vendoring, federated upgrades, host-independent CI](ADR-038-gate-artifact-upgrades-host-independent-ci.md)

Earlier proposed:

- [ADR-012 — Part identification: nano-id + QR labels with mint-then-bind workflow](ADR-012-part-identification.md) _(refined by ADR-035: `kind` + `components` added; `batch` retired; ids typed `(scheme,value)`)_
- [ADR-013 — Parts registry web app: GH Pages + WASM DuckDB + PR-driven binds](ADR-013-parts-registry-web-app.md)
- [ADR-015 — Print event log: non-destructive audit trail of every label print](ADR-015-print-event-log.md) _(folded into the ADR-022 audit stream; invariants restate as audit-spine validators per ADR-035)_

### Superseded

- [ADR-014 — Web app architecture: extension interfaces, SSOT, plugin model](ADR-014-web-app-architecture.md) _(superseded by ADR-030 — the TS SPA is retired; §"Pyodide migration trigger" was already closed by ADR-017)_
