# ADR-029 — Architectural-coverage validator

- Status: Accepted
- Date: 2026-05-11
- Component / area: cross-cutting — architectural coverage validation
  framework. Defends the design surface (crates, port traits, SOUP
  dependencies, ADR commitments, re-open triggers, foundation issues)
  against silent omission, complementing the per-source-line drift
  discipline in ADR-027 and the SOUP discipline in ADR-028.
- Reviewers: Lars Gerchow
- Related: ADR-016 (PR-diff policy enforcement), ADR-017 (Rust core +
  ports/adapters), ADR-023 (threat model + re-open triggers T1–T6),
  ADR-024 (crypto baseline + branch protection), ADR-027 (port
  conformance tests, four-tier discipline), ADR-028 (SOUP validation
  per IEC 62304 §5.3)

## Context

ADR-017 commits the project to a Rust workspace organised around a
small domain core with concerns (storage, identity, transport,
signing, observability, configuration) lifted into traits with
multiple adapter crates. ADR-027 commits each adapter to a four-tier
conformance discipline. ADR-023 carries six explicit re-open triggers
(T1–T6) on which deferred crypto controls activate. ADR-028 (in
flight, written in parallel; see
[ADR-028](ADR-028-soup-validation.md)) commits the project to a SOUP
inventory satisfying IEC 62304 §5.3.3 (identification of software of
unknown provenance) and §8.1.2 (problem resolution / change control
on SOUP).

Each of those ADRs defends a *category* of failure. None of them
defends against the failure mode that any one of those categories is
*silently incomplete*. The category-level failure modes already
observed in this repository:

- **Forgotten crate.** `crates/port_tests/src/lib.rs` ships with
  intentionally empty function bodies (`pub fn
  repository_conformance<R: Repository>(_repo: R) { /* TODO */ }`).
  ADR-027 requires every adapter crate to call into these
  conformance functions; ADR-027 does not provide a mechanism that
  fails CI when an adapter crate is added without wiring the call,
  nor when the conformance body itself stays empty past a deadline.
  The discipline survives only as a code-review norm.
- **Forgotten conformance call.** An adapter PR can land a green
  `cargo test` while shipping no `tests/conformance.rs` at all.
  ADR-027 Tier 4 includes a drift test for this obligation, but the
  drift test itself is a manual addition; if `port_tests` is not yet
  populated for a trait, the absence is undetectable.
- **Forgotten SOUP entry.** Adding a transitive dependency to any
  crate's `Cargo.toml` does not mechanically force a SOUP inventory
  update. IEC 62304 §5.3.3 requires every SOUP item to be identified
  by title, manufacturer, and unique designator; ISO 13485 §7.3.5
  (design and development verification) requires evidence that
  design outputs meet inputs. A SOUP item present in `Cargo.lock`
  but absent from the inventory file fails both clauses silently.
- **Silently resolved re-open trigger.** ADR-023 §"Re-open triggers"
  lists T1–T6 with activation conditions. If a trigger fires, the
  methodology requires a `LOG.md` entry plus a Corrections section
  on ADR-023 (per [`METHODOLOGY.md`](METHODOLOGY.md) §Correction
  protocol). Today there is no mechanism that asserts the inverse:
  that no trigger has been quietly resolved without the audit
  artefact being written.
- **Orphan obligation.** ADR-019 originally referenced
  `ADR-020-operator-identity.md` (the file was renamed to
  `ADR-020-identity-authorization-port.md`). The broken cross-link
  was caught only by the end-to-end PR reviewer pass, well after
  ADR-019 was merged Accepted. ADR-027 Tier 4 scans source files for
  invariants; it does not scan ADR markdown for cross-reference
  integrity.
- **Orphan inventory row.** Symmetric failure: a `coverage.toml`
  entry referencing a crate that no longer exists, a port trait that
  has been deleted, or a deferred control that has shipped.
  Obligations files accrete dead rows without periodic reconciliation.

Each of these is a category of work the project has already committed
to. None of them is currently mechanically enforced at the
architectural-element level. ADR-027 Tier 4 enforces invariants at
the per-source-line level (no `println!`, no hardcoded paths,
mutations take `&Operator`); it does not answer "is every crate
covered", "is every port trait wired", "is every SOUP item logged",
"is every Consequences bullet traceable to code or a tracked issue".

The audit framing: IEC 62304 §5.3.3 (SOUP identification) and §5.6
(software integration testing) require evidence of *coverage*, not
just evidence of *behaviour*. ISO 13485 §7.3.5 (design verification)
requires that design outputs are verified against design inputs;
ADRs are design inputs in this project's methodology
([`METHODOLOGY.md`](METHODOLOGY.md) §Purpose), and verification of
"every Consequence is realised" is exactly what is currently
unmechanised.

The Rust workspace shape from ADR-017 already includes a
`crates/port_tests/` slot for cross-cutting test discipline. The
coverage validator is the natural fifth tier of that crate: where
Tiers 1–4 verify behaviour per adapter and per source line, this tier
verifies *that every architectural element has its obligations
satisfied*.

## Alternatives considered

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| **Status quo — manual reviewer judgement only** | Zero engineering; today's process | Gaps caught only by post-hoc end-to-end review (ADR-019 broken link is the empirical example); fails the audit posture in `METHODOLOGY.md` (a Notified Body asking "how do you know nothing is forgotten" has no artefact to point at); does not scale past ~20 ADRs | Rejected — the failure mode is already observed |
| **Per-crate ad-hoc tests** — each crate writes its own coverage check against its own obligations | Distributes ownership | Fragments the audit surface across N crates; an auditor must visit every crate to assemble the picture; no single artefact answers "is anything forgotten"; obligations duplicated and drift between crates | Rejected — defeats the audit-artefact purpose |
| **Compose existing tools only** — `cargo-deny` (license + version pinning), `cargo-vet` (audit trail), `cargo-machete` (unused deps), wrapper script glues them | Less custom code; benefits from upstream maintenance | Each upstream tool is partial: none know about ADR cross-refs, none know about conformance-function call obligations, none know about ADR-023 trigger watchers, none read `decisions/*.md` frontmatter; still needs custom logic for ~60% of the matrix | Rejected as the *whole* solution; adopted as *feeders* into the matrix (see Decision) |
| **Standalone Python script outside the workspace** | Quick to iterate; doesn't pollute `Cargo.toml` | Separate runtime (Python + venv) on every contributor's machine, conflicting with the Rust-only toolchain commitment in ADR-017; cannot use `cargo metadata` natively without re-parsing TOML; drift between script and workspace as crates are added | Rejected — runtime dependency conflict with ADR-017 |
| **Rust binary `part-registry-coverage` inside `crates/port_tests/`** reading a declarative `coverage.toml` at the repo root plus workspace state from `cargo metadata` plus `decisions/*.md` plus (when present) the SOUP inventory file | Uses `cargo metadata` as the workspace SSOT; co-located with ADR-027 conformance framework (one crate, one audit pointer); deterministic; single artefact (`target/coverage-matrix.{json,md}`); composable with `cargo-deny` and `cargo-vet` as feeders | One more binary in `port_tests`; needs `toml`, `pulldown-cmark`, `cargo_metadata`, `serde_json` as deps; obligations file must be kept current | **Chosen** — only option that produces a single auditor-readable artefact while using `cargo metadata` as the workspace truth |

## Decision

A new binary `part-registry-coverage` is added to
`crates/port_tests/` (alongside the existing library target that
hosts the Tier 1–4 conformance suites per ADR-027). The binary reads:

- `Cargo.toml` workspace metadata via `cargo_metadata` (the single
  source of truth for which crates exist),
- a declarative `coverage.toml` at the repository root (the
  obligations specification; one row per architectural element),
- `decisions/*.md` (ADR frontmatter, headings, cross-references),
- the SOUP inventory file owned by ADR-028 (path TBD by ADR-028; the
  validator degrades to `WARN` if the file is absent so this ADR
  lands before ADR-028 without blocking),
- (optionally) outputs of `cargo-deny` and `cargo-machete` as
  feeders.

It emits two artefacts per run:

- `target/coverage-matrix.json` — machine-readable
  obligation × satisfaction table,
- `target/coverage-matrix.md` — auditor-readable human summary,
  sectioned by dimension, listing every row with status and citation
  to the satisfying file (or the open foundation issue if pending).

Exit codes:

- `0` — every required cell is filled.
- `1` — at least one required cell is missing (a forgotten
  obligation).
- `2` — orphan rows detected (`coverage.toml` references workspace
  elements that no longer exist).
- `3` — expired exemption: an obligation marked exempt with an
  expiry date has passed that date without being re-evaluated.

The locally-invoked hook (via [`prek`](https://prek.dev) — the
Rust-native pre-commit reimplementation — falling back to canonical
`pre-commit` if the contributor prefers Python tooling) runs the
binary at `pre-push` time and surfaces failures as `WARN`. The CI
job (`.github/workflows/coverage.yml`) runs the same binary and
treats non-zero exit as a hard `ERROR` blocking merge. The CI job is
registered as a `required_status_check` on `main` per
[ADR-024](ADR-024-crypto-baseline-mvp.md) §"Branch protection
configuration", so the gate cannot be bypassed by a tired reviewer
clicking through merge.

### Coverage matrix specification

The validator enforces six dimensions. Each dimension has a fixed
set of obligations and a role taxonomy that determines which
obligations apply to which element.

**Role taxonomy** (assigned per crate in `coverage.toml`):

| Role | Examples | Min tests | Conformance call required | Doc header required |
|---|---|---|---|---|
| `pure-data` | `domain` | ≥ 1 | no | yes |
| `port-trait` | `storage`, `identity`, `transport`, `signing` | 0 (trait-only) | no (trait host) | yes |
| `adapter` | `storage_csv_git`, `identity_git_config` | ≥ 5 | yes (Tier 1) | yes |
| `binary` | `cli`, `wasm` | ≥ 1 (smoke) | no | yes |
| `test-fixture` | `port_tests` itself | self-referential | n/a | yes |

**The six dimensions:**

1. **Crates** — every workspace member listed in `cargo metadata
   workspace_members` has a row in `[crates]`. Role assigned.
   Obligations per role enforced: header doc-comment referencing the
   ADR section it implements, `Cargo.toml` `description` field
   non-empty, test floor per role, conformance call wired for
   `adapter`-role crates.
2. **Ports** — every port trait listed in `[[ports]]` must have at
   least one declared adapter, at least one conformance-function
   stub in `crates/port_tests/`, and at least one ADR cross-reference
   (typically the ADR that introduced the port).
3. **SOUP** — every dependency surfacing in `cargo metadata` (after
   filtering workspace-internal crates) must appear in the SOUP
   inventory file owned by ADR-028. Class 3 SOUP additionally
   requires a `validation_harness_path` pointing at a file under
   `crates/port_tests/src/soup/`, a `maintenance_url`, and a pinned
   version. When the SOUP file is absent (pre-ADR-028 state), this
   dimension degrades to `WARN` and does not block CI.
4. **ADR commitments** — every Accepted ADR must have a populated
   `Reviewers` field, a populated `Related` field where any
   cross-reference exists, every `[ADR-NNN](file.md)` link must
   resolve, every Consequences bullet must be traceable to either
   implemented code (path) or an open foundation issue (URL). When
   the trace target cannot be machine-inferred, the row is emitted
   to the `WARN` list with text "manual check needed".
5. **Re-open triggers** — every Deferred ADR and every named
   re-open trigger (the canonical set today is ADR-023 T1–T6) must
   have an activation condition documented and a "watch" annotation
   recording who or what surveys the trigger. A trigger marked
   resolved without a matching `LOG.md` entry or Corrections section
   is a coverage failure (exit code 1).
6. **Foundation issues** — every issue listed in `[foundation]` must
   reference at least one ADR, list its hard/soft dependencies
   explicitly, and carry a strangler-fig step annotation per
   ADR-017 §"Strangler-fig migration sequence". When an issue
   closes, the validator verifies the closing PR touched the
   referenced ADR's `Consequences` trace target (this verification
   may be `WARN`-only if GitHub API access is unavailable from the
   hook).

The full TOML schema (field names, optional vs required keys,
example rows for all six dimensions) is captured in the design
analysis at
`/private/tmp/claude-501/-Users-larsgerchow-Projects-eXoma-part-registry/86cb9a40-4ebd-4eb3-bcad-341510bcd9c9/tasks/ae92ea58592b1a940.output`
§2 ("The obligations specification") and will be reproduced in the
implementation issue. The ADR carries the high-level shape only:

```toml
schema_version = 1

[crates.domain]
role = "pure-data"
adr_ref = "ADR-017 §workspace shape"
min_tests = 1

[crates.storage_csv_git]
role = "adapter"
adr_ref = "ADR-018 §MVP adapter"
implements_port = "storage::Repository"
min_tests = 5
conformance_fn = "port_tests::repository_conformance"

[[ports]]
name = "Repository"
crate = "storage"
adr_introduced = "ADR-018"
adapters = ["storage_csv_git"]
conformance_fn = "port_tests::repository_conformance"

[[triggers]]
id = "T2"
adr = "ADR-023"
activates = ["sigstore_keyless", "rekor_anchor"]
watch = "manual quarterly review until automation lands"

[exemptions.example]
path = "crates/wasm"
obligation = "min_tests"
reason = "wasm-bindgen target lacks std test harness; deferred to step 8"
expires = "2026-09-01"
```

Failure-mode contract: when the validator emits exit code 1, the
markdown summary lists every missing cell with the violated ADR
clause and the suggested remediation file path. Auditor reading the
markdown cold can answer "what is forgotten" without opening any
other file.

### CI workflow shape

```bash
# .github/workflows/coverage.yml (illustrative)
name: coverage-matrix
on: [pull_request, push]
jobs:
  coverage:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo run -p part-registry-port-tests --bin part-registry-coverage
      - uses: marocchino/sticky-pull-request-comment@v2
        with:
          path: target/coverage-matrix.md
```

The sticky-comment action keeps the matrix visible on the PR thread
across pushes; non-zero exit fails the job, the job is required by
branch protection.

## Rationale

**Why a Rust binary in the workspace rather than a standalone
script.** `cargo metadata` is the single source of truth for which
crates exist and what they depend on; a standalone script (Python,
shell) re-parses TOML manually and drifts every time a workspace
member moves. ADR-017 commits the project to a Rust-only toolchain
on contributor machines; a Python script imposes a second runtime
that conflicts with that commitment. Co-locating the binary inside
`crates/port_tests/` means one audit pointer covers ADR-027's
four-tier discipline *and* the architectural-coverage gate — an
auditor asking "where is the cross-cutting verification machinery"
gets one answer, not two. The cost (one new binary target, ~five
crates of dependencies via `cargo_metadata`, `toml`,
`pulldown-cmark`, `serde_json`, `regex`) is bounded.

**Why `coverage.toml` at the repo root rather than embedded in
crate manifests.** The obligations cross-cut crates (a port trait
spans the trait-host crate and N adapter crates; a re-open trigger
is owned by an ADR, not a crate). One top-level file is the only
location consistent with "single audit artefact". Putting fragments
in per-crate `Cargo.toml` `[package.metadata.coverage]` tables would
re-fragment the audit surface — exactly the failure mode the
per-crate ad-hoc alternative was rejected for.

**Why `prek` for the local hook with `pre-commit` fallback.** `prek`
is the actively-maintained Rust-native reimplementation of
`pre-commit`, with drop-in `.pre-commit-config.yaml` compatibility,
faster invocation (no Python interpreter bootstrap), and no Python
runtime requirement on contributor machines — consistent with the
Rust-only toolchain commitment in ADR-017. Contributors who already
have Python tooling configured can fall back to canonical
`pre-commit` without changing the config file. The fallback is
documented in `CONTRIBUTING.md`; both paths invoke the same binary.

**Why `WARN` locally and `ERROR` in CI.** The user's framing
("validates/warns about such coverage so we don't forget") is a
two-tier discipline: local development should *surface* gaps without
blocking work-in-progress commits; CI should *block* gaps from
landing on `main`. A `WARN`-only hook honours the contributor's
right to commit a known-incomplete intermediate state (an adapter
PR in progress legitimately has zero conformance calls until the
final commit lands them); a CI `ERROR` gate enforces that the
incomplete state cannot merge. Crucially, this design does **not**
rely on `--no-verify` bypass: the CI job is the load-bearing gate,
local `WARN` is informational. An attacker (or a tired contributor)
running `git commit --no-verify` cannot bypass CI.

**Why CI runs after `cargo test`.** A coverage failure on top of a
broken build is noise; the matrix is most useful when the workspace
is otherwise green. The workflow orders `cargo test` first, then
the coverage job, so the matrix output reflects the same workspace
state the tests passed against.

**Why expiry-dated exemptions rather than open-ended opt-outs.** An
opt-out without an expiry becomes permanent dead weight in the
obligations file. Forcing every exemption to carry an `expires =
"YYYY-MM-DD"` field and surfacing expired exemptions as a separate
exit code (3) ensures exemptions are revisited rather than
forgotten. The audit posture: an auditor reading `coverage.toml`
sees both what is exempt *and* when the exemption will be
re-evaluated.

**Why symmetric orphan detection.** A `coverage.toml` row pointing
at a crate that has been deleted is the inverse failure of a crate
that has no row. Both are signs the obligations file has drifted
from reality. Exit code 2 distinguishes orphan failures from
missing-coverage failures so the remediation is unambiguous (delete
the row vs add the artefact).

## Consequences

This ADR commits the project to the following concrete obligations
on contributors and on the project itself:

- **Workspace-member discipline.** Adding a workspace member to
  `Cargo.toml` `[workspace.members]` in any PR requires updating
  `coverage.toml`'s `[crates]` table in the same PR. Role
  assignment is mandatory; the validator rejects rows without a
  `role` field. Crates without a row trip exit code 1.
- **Port-trait discipline.** Adding a port trait to any crate
  requires adding a `[[ports]]` row plus at least one
  conformance-function stub in `port_tests`. The stub may have a
  `TODO` body during foundation-phase work (matching the current
  state of `crates/port_tests/src/lib.rs`), but the function symbol
  must exist and must be referenced from the matching `[crates.*]`
  row's `conformance_fn` field.
- **SOUP discipline (cross-reference to ADR-028).** Adding any new
  dependency to any `Cargo.toml` requires adding a SOUP inventory
  entry in the file owned by ADR-028. While ADR-028 is still in
  flight, this dimension is `WARN`-only and CI does not block on
  SOUP gaps. When ADR-028 lands, the dimension activates and the
  validator begins enforcing it as `ERROR` in CI.
- **Adapter-conformance discipline.** Adapter PRs must wire
  `tests/conformance.rs` calling `part_registry_port_tests::*` for
  their port. This was already an ADR-027 §Tier 4 obligation
  enforced by a drift test; this ADR enforces it symmetrically from
  the `coverage.toml` side so a deleted drift test cannot silently
  retire the obligation.
- **Accepted-ADR discipline.** ADR PRs landing with `Status:
  Accepted` must include named reviewers, a populated `Related`
  field where any cross-reference exists, and resolved cross-link
  targets. Broken `[ADR-NNN](file.md)` links trip exit code 1
  (mechanically reproducing the ADR-019 broken-link case that
  motivated this ADR).
- **Hook installation.** `pre-commit` (Rust-native `prek`, with
  canonical Python `pre-commit` as documented fallback) becomes
  part of the standard contributor setup; the bootstrap step `prek
  install` is added to `CONTRIBUTING.md`. The hook is not auto-
  installed by `cargo build` — installing developer tooling silently
  on `cargo build` would violate the principle of least surprise —
  but the bootstrap instruction is explicit and the hook is
  expected on contributor machines.
- **CI required status check.** `.github/workflows/coverage.yml` is
  added as a `required_status_check` on the `main` branch
  protection rules captured in
  `.github/branch-protection.yaml` per
  [ADR-024](ADR-024-crypto-baseline-mvp.md) §"Branch protection
  configuration". The check cannot be bypassed by a non-admin
  merger.
- **Exemption discipline.** An obligation can be exempted by adding
  an entry to `[exemptions]` in `coverage.toml`. Each exemption
  requires a `reason` and an `expires` date. Expired exemptions trip
  exit code 3 surfaced as a separate WARN section in the markdown
  output so they do not silently roll forward.
- **Orphan-row discipline.** `coverage.toml` rows referencing
  workspace members, port traits, ADRs, triggers, or issues that no
  longer exist trip exit code 2. The obligations file cannot accrete
  dead weight.

This ADR does **not** commit the project to:

- Replacing ADR-027's per-source-line drift tests. ADR-027 Tier 4 and
  this ADR's dimension matrix are complementary, run from the same
  crate, and produce distinct findings.
- Owning the SOUP inventory schema. That is ADR-028's responsibility;
  this ADR consumes whatever ADR-028 commits to.
- A particular markdown rendering library or PR-comment GitHub
  Action; the references list is the recommended choice but the
  implementation issue may substitute equivalents.

## Forward-compatibility

- **SOUP dimension activation.** Today (with ADR-028 in flight) the
  SOUP dimension is `WARN`-only. When ADR-028 lands and the inventory
  file path is fixed, the validator's `[soup]` section in
  `coverage.toml` is updated with the path, and the dimension
  promotes to `ERROR`. No code change to the validator binary
  required — the path is configurable.
- **`cargo-audit` integration.** When CVE surveillance is wired
  (today: not committed; recommended for a successor ADR), a
  seventh dimension `[advisories]` activates via the same mechanism.
  `cargo-audit` becomes a feeder whose JSON output is consumed
  alongside the SOUP inventory.
- **Schema versioning.** `coverage.toml` carries `schema_version =
  1` as its first key. Schema migrations are explicit: the validator
  refuses to run against an unsupported `schema_version` and the
  migration is captured in a successor ADR (or a Corrections section
  on this one if minor).
- **AST-level extraction.** Today the validator uses `walkdir` +
  `regex` for source scanning (mirroring ADR-027's choice). If
  invariants become structural enough to require AST (`syn`-based)
  extraction, that is a successor optimisation and does not
  invalidate the dimension matrix.

## Open questions / supersession triggers

- **When does the binary split out of `crates/port_tests/` into its
  own crate?** Today: kept inside `port_tests` so one crate hosts
  the entire cross-cutting test discipline. Soft threshold: ~1500
  LoC of validator source, or when the validator's dependency
  closure begins to materially slow `cargo test -p
  part-registry-port-tests`. Re-opens at that threshold.
- **When does the obligations file migrate from TOML to a
  JSON-Schema'd format?** TOML is human-editable and matches the
  rest of the Rust toolchain. If a generative consumer (e.g. a
  separate audit-dashboard tool) wants to ingest the obligations
  spec programmatically, JSON-Schema becomes warranted. Re-opens
  when such a consumer is proposed.
- **When does ADR-027 Tier 4 source-line drift merge into the
  matrix?** Today: separate runs, separate test invocations. Soft
  trigger: when the contributor-visible UX would benefit from a
  single markdown artefact covering both per-line and per-element
  findings. Decision deferred to a successor ADR.
- **When does a seventh dimension activate for `cargo-audit` /
  `cargo-vet`?** When CVE surveillance is committed by a successor
  ADR. The mechanism is reusable: a feeder tool produces JSON,
  `coverage.toml` declares the obligation, the validator joins.
- **Should the validator emit a SARIF artefact for GitHub
  code-scanning?** Out of scope for the foundation phase. Re-opens
  if the project adopts GitHub Advanced Security or an equivalent
  SARIF-consuming tool.

## References

- IEC 62304:2006/AMD1:2015 §5.3.3 — Software unit verification
  (SOUP identification)
- IEC 62304:2006/AMD1:2015 §5.6 — Software integration and
  integration testing
- IEC 62304:2006/AMD1:2015 §8.1.2 — Problem resolution on SOUP
- ISO 13485:2016 §7.3.5 — Design and development verification
- [ADR-016 — PR-diff-based policy enforcement](ADR-016-pr-diff-policy-enforcement.md)
- [ADR-017 — Rust core, ports/adapters, multi-target deploy](ADR-017-rust-core-ports-adapters.md)
- [ADR-023 — Threat model + crypto-MVP scope](ADR-023-threat-model-and-crypto-mvp-scope.md) — re-open triggers T1–T6
- [ADR-024 — Cryptographic baseline (MVP)](ADR-024-crypto-baseline-mvp.md) — branch protection, required status checks
- [ADR-027 — Port conformance + forward-compatibility tests](ADR-027-port-conformance-tests.md) — per-source-line drift discipline; this ADR extends to per-architectural-element
- [ADR-028 — SOUP validation per IEC 62304 §5.3](ADR-028-soup-validation.md) — SOUP inventory schema; this ADR enforces inventory completeness. Both ADRs depend on each other but neither blocks the other: ADR-029 degrades to WARN on the SOUP dimension when the inventory file is absent.
- [`METHODOLOGY.md`](METHODOLOGY.md) — audit principles, Correction protocol
- Design analysis report: `/private/tmp/claude-501/-Users-larsgerchow-Projects-eXoma-part-registry/86cb9a40-4ebd-4eb3-bcad-341510bcd9c9/tasks/ae92ea58592b1a940.output` — full `coverage.toml` schema, tool architecture, rollout sequence
- `prek` — <https://prek.dev> (Rust-native pre-commit reimplementation)
- `pre-commit` — <https://pre-commit.com> (canonical Python implementation, fallback)
- `cargo-metadata` — <https://crates.io/crates/cargo_metadata>
- `cargo-deny` — <https://github.com/EmbarkStudios/cargo-deny> (license + version pinning feeder)
- `cargo-vet` — <https://mozilla.github.io/cargo-vet/> (audit-trail feeder)
- `cargo-machete` — <https://github.com/bnjbvr/cargo-machete> (unused-dep feeder)
- `marocchino/sticky-pull-request-comment` — <https://github.com/marocchino/sticky-pull-request-comment> (PR comment GitHub Action)
- Hexagonal / ports & adapters — Alistair Cockburn, 2005
