# ADR-019 — Proposal sink as a port

- Status: Accepted
- Date: 2026-05-10
- Component / area: cross-cutting — defines the trait shape every
  mutation pathway in the project must implement; first adapter is
  GitHub PR via REST API, future adapters cited in
  §Forward-compatibility
- Reviewers: Lars Gerchow
- Related: ADR-013 (Parts registry web app — PR-driven mutation
  model), ADR-014 (Web app architecture — queue-and-batch-submit
  pattern), ADR-016 (PR-diff policy enforcement — CI is the policy
  authority), ADR-017 (Rust core + ports/adapters — workspace shape),
  ADR-018 (Storage port — read + audit-append only; mutations route
  here), ADR-020 (Identity & authorization as a port), ADR-022 (Observability — audit
  trail), ADR-023 (Threat model + crypto-MVP scope — `signatures`
  forward-compat), ADR-024 (Cryptographic baseline), ADR-027 (Port
  conformance + parity tests)

## Context

ADR-013 fixed the registry's mutation model: **the only legal way to
change a `Part` is to open a pull request against the data
repository.** ADR-016 made the property enforceable — a CI workflow
runs a semantic-diff classifier on the PR's `registry.csv` patch and
either auto-merges, requires review, or blocks. ADR-014 added the
batched-submit shape on the FE: bind/edit operations queue locally
(per-tab `localStorage`) and submit as one PR per session, not one
PR per row. ADR-018 then split storage's responsibilities — the
`Repository` trait is read + audit-append only, with mutations
explicitly routed elsewhere. ADR-018 §"Why read + audit-append only"
names this ADR as the place mutations live.

That leaves the question: **where do mutations actually go.** Four
pressures force a trait rather than a hardcoded GitHub-PR call:

1. **The repo split is real.** The project ships as a code repo
   (`part-registry`, OSS — Rust workspace, ADRs, FE source) and a
   data repo (e.g. `exopet-registry`, closed — `registry.csv`,
   `print_log.csv`, `audit_log.csv`). Mutations target the data
   repo; the code repo is not a mutation target.
2. **Multiple submission channels are credibly on the roadmap.**
   GitHub PR (today) is one of at least four shapes needed over 24
   months: offline / local-branch (ADR-023's defer-signing mode),
   GitHub App broker (code-repo issue #5), air-gapped
   deposit-to-folder. Each is a *delivery mechanism*; the
   *contents* of the proposal — diff, batch label, author,
   signatures, message, advisory classification — are identical
   across them.
3. **ADR-027 cross-adapter parity tests** require any second
   adapter to round-trip a fixed-corpus suite vs. the reference
   adapter. That contract has nowhere to live unless submission is
   a trait.
4. **ADR-023 §"Schema forward-compatibility" requires `signatures:
   Vec<Signature>` to round-trip the entire pipeline.** If
   `submit()` took only `(diff, message)`, activating Sigstore
   later (re-open trigger T2/T4) would force widening the trait
   and revisiting every adapter.

## Alternatives considered

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| **Direct write to `registry.csv` from the binary (no PR)** | Simplest possible code path; no network dependency at write time | Violates ADR-013's PR-driven mutation model directly; ADR-016's CI policy engine has nothing to evaluate (the change is already on `main`); no reviewer trail; no batched-review property from ADR-014; loses the audit-of-record argument | **Rejected** — the PR-driven property is the entire point of ADR-013/016 |
| **GitHub PR via `gh` CLI subprocess from the binary** | Quick to prototype (`gh pr create` is one shell-out); reuses the operator's existing `gh` auth | Shells out to a Node-based binary the project does not control; depends on `gh` being installed and on `PATH`; failure modes (rate limit, network, auth expiry) come back as exit codes and stderr scraping; no programmatic access to the resulting PR object's fields (URL, number, status); cannot be cross-compiled to WASM (subprocess invocation has no WASM equivalent) | **Rejected** — the WASM constraint alone disqualifies it; ADR-017's strangler-fig step 8 explicitly targets a WASM-compiled core |
| **GitHub PR via REST API (`crates/transport_github_pr/`), wrapped behind a `ProposalSink` trait, future adapters slot in** | Clean abstraction; supports the multi-target deploy story above; parity-testable per ADR-027; no subprocess dependency; WASM-compatible (REST over `wasm-bindgen`'s `fetch`); one place to add forward-compat fields (signatures, classifications); structural prevention of bypass paths matches ADR-018's read-only `Repository` | Up-front trait design cost; one more crate in the workspace; requires picking a Rust GitHub client library | **Chosen** |
| **Single hard-coded GitHub App (issue #5's broker is the architecture, not an adapter)** | Possible with the broker pattern; centralizes auth (no personal OAuth tokens); audit log on the broker side | Locks the project to one mechanism — there is no offline workflow, no air-gapped workflow, no "submit to a different host" workflow; couples the project's mutation pipeline to a specific deployed service the team must operate; ADR-023's defer-signing offline mode has no submission path at all | **Rejected** — the App becomes one adapter (`transport_webhook`) alongside others, not the architecture itself |
| **No abstraction; inline the GitHub REST calls at each call site** | Zero ceremony for the MVP; every call site sees the full request shape | Same problem ADR-018 solved for storage: hardcoded paths everywhere, no parity testing, no place to enforce ADR-023's forward-compat fields, every future adapter is a per-call-site refactor; ADR-016's classification metadata has no canonical location to live in | **Rejected** — the wrapper-vs-trait calculus from ADR-018 §Rationale applies identically here |

## Decision

Mutations are a port. The Rust workspace declares a `ProposalSink`
trait in `crates/transport/src/lib.rs` and ships exactly one adapter
for the MVP: `crates/transport_github_pr/`, which implements
`ProposalSink` by opening pull requests against the data repository
via the GitHub REST API (using `octocrab` or equivalent — the
specific client crate is an implementation detail of the adapter,
not a workspace-wide commitment).

The trait surface is **submit + status only**. There is no
`merge()`, no `close()`, no `comment()`. The proposal is *submitted*
by the binary; *evaluated* by CI per ADR-016; *merged or rejected*
by the GitHub UI (or future adapters' equivalents) under reviewer
authority. The trait deliberately offers no method that would let
the binary self-merge — that capability belongs to the policy
authority (CI + reviewers), not to the mutation pipeline.

### Trait shape

```rust
// crates/transport/src/lib.rs

use crate::types::{
    Diff, ChangeClass, ProposalStatus, ProposalRef,
    Operator, Signature,
};

pub struct Proposal {
    /// Structured diff (not raw unified-diff) so the
    /// change_classification can be derived deterministically by
    /// FE preflight and re-derived authoritatively by CI.
    pub diff: Diff,

    /// Per ADR-014, batched FE submissions get a stable label
    /// (e.g. "B-2026-05-08-sdmd") cross-referencing the FE queue.
    pub batch_label: Option<String>,

    /// Per ADR-020, the Operator from IdentityProvider at session
    /// open. Survives into the PR body and the audit log.
    pub author: Operator,

    /// ADR-023 §"Schema forward-compatibility" — today populated
    /// with one Signature::GitCommit variant. Sigstore / DSSE /
    /// hardware-key variants slot in without changing this type
    /// (re-open triggers T2/T4 in ADR-023).
    pub signatures: Vec<Signature>,

    /// Pre-classification per ADR-016's semantic-diff classifier.
    /// **Advisory only.** CI re-runs authoritatively (ADR-016 §"CI
    /// is the policy authority"). Divergence is logged.
    pub change_classification: Vec<ChangeClass>,

    /// Human-readable PR body. Trait does not constrain content.
    pub message: String,
}

pub struct ProposalRef {
    /// Canonical reference. For transport_github_pr the PR URL.
    /// Stable; ADR-022 audit log cites this.
    pub url: String,
    /// Adapter-internal handle (PR number, local branch name,
    /// deposit file path). Opaque to callers.
    pub local_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProposalStatus {
    Open,
    PolicyPending,
    Merged { sha: String },
    MergedAfterReview { sha: String, reviewer: String },
    Closed { reason: String },
    RequiresReview,
    BlockedByPolicy { reason: String },
}

pub trait ProposalSink: Send + Sync {
    /// Submit a proposal. **Does not block** on CI / merge —
    /// submission is decoupled from acceptance per ADR-016.
    fn submit(&self, proposal: Proposal) -> Result<ProposalRef, ProposalError>;

    /// Poll status. Used by the FE for "your batch merged"
    /// notifications and by the CLI to wait on a long batch.
    fn status(&self, proposal_ref: &ProposalRef) -> Result<ProposalStatus, ProposalError>;

    // NO `merge`, `close`, `comment`, `force_push`. Structurally
    // incapable of bypassing the policy authority. Operations are
    // "propose" and "observe"; acceptance belongs to CI + reviewers.
}

#[derive(Debug, thiserror::Error)]
pub enum ProposalError {
    #[error("network: {0}")]
    Network(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("auth: {0}")]
    Auth(String),
    #[error("rate limited; retry after {retry_after_seconds}s")]
    RateLimited { retry_after_seconds: u64 },
    #[error("backend rejected proposal: {0}")]
    Rejected(String),
    #[error("backend error: {0}")]
    Backend(#[source] Box<dyn std::error::Error + Send + Sync>),
}
```

Data types (`Diff`, `Operator`, `Signature`, `ChangeClass`) live
in `crates/domain/` per ADR-017's workspace shape, alongside types
ADR-018 placed there. `Operator` and `Signature` are shared with
`Repository`'s `AuditEntry` so a successful merge records to audit
without re-deriving identity.

### Repo split

The proposal sink targets the **data repo**, never the code repo.
The CSV+git storage adapter (ADR-018) and the GitHub-PR proposal
adapter both operate against the same data repo, but the storage
adapter's connection is a local working-tree path while the
proposal adapter's is a remote URL
(`https://github.com/<org>/<data-repo>`). The two are
deliberately not bundled into one "data-repo client" — the split
is what makes `Repository::list_parts` work against a read-only
clone (no GitHub auth needed) while `ProposalSink::submit` holds
the credential for write.

### Adapter selection

Per ADR-021 (12-factor configuration), the active adapter is
selected by an environment variable read at process start:

```
PARTREG_PROPOSAL_SINK=github_pr             # MVP default
PARTREG_PROPOSAL_SINK=local_branch          # future, ADR-023 offline
PARTREG_PROPOSAL_SINK=webhook               # future, issue #5
PARTREG_PROPOSAL_SINK=filesystem            # future, air-gap
PARTREG_PROPOSAL_SINK=dolt                  # future, paired with storage_dolt
```

The `cli/` crate's wiring code matches on this value and
constructs the appropriate `Box<dyn ProposalSink>`. Adding a future
adapter is one new crate plus one match arm; no caller changes.

### Interaction with ADR-014's batched-submit pattern

The FE queue (per ADR-014, per-tab `localStorage`) serializes on
flush into a single `Diff`, attaches the IdP-session `Operator`
(per ADR-020), runs the ADR-016 classifier as preflight, and
calls `ProposalSink::submit` exactly once with a generated
`batch_label`. The trait is deliberately *not* "submit-many";
batching is the caller's concern, expressed by a multi-row `Diff`.

## Forward-compatibility

The trait must accommodate each adapter below without redesign.
For each: trigger, integration cost, trait surface, invariant.

### `transport_local_branch` — offline workflow

- **Trigger**: ADR-023's defer-signing offline mode becomes
  load-bearing (field clinic with no internet, lab terminal during
  outage). Operator must bind parts and queue proposals offline;
  sync happens when connectivity returns.
- **Integration cost**: ~3–5 days. Adapter writes the diff to a
  local branch in the data-repo working tree
  (`offline-batch-<label>`), commits with the operator's git
  identity, records the branch name as `local_id`. A separate
  `sync` command (CLI-only, not part of the trait) pushes pending
  branches and opens PRs via the GitHub-PR adapter.
- **Trait surface**: `submit` (local branch write); `status`
  (returns `Open` for pending, delegates after sync).
- **Critical invariant**: the offline branch IS the durable form
  until sync. A power loss between `submit` and `sync` must not
  lose work — the adapter commits eagerly.

### `transport_webhook` — GitHub App / proposal broker (issue #5)

- **Trigger**: scaling beyond a few operators where personal OAuth
  tokens become friction (rotation, scope creep, attribution). A
  centralized broker holds one App credential and attributes each
  proposal to the originating operator via ADR-020's IdP claims.
- **Integration cost**: ~5–7 days for the adapter; the broker
  service itself is a separate scope (issue #5). Adapter POSTs the
  `Proposal` to `POST /v1/proposals`; broker validates identity,
  opens the PR via its App, returns the PR URL. `status` polls the
  broker.
- **Trait surface**: `submit` (POST); `status` (GET). Trait shape
  unchanged; only the wire differs.
- **Critical invariant**: the broker is one adapter, not the
  architecture. A privileged operator can bypass it by setting
  `PARTREG_PROPOSAL_SINK=github_pr`. The broker doesn't get to be
  a chokepoint by default.

### `transport_filesystem` — air-gapped deposit-to-folder

- **Trigger**: deployment with no outbound network (GMP-grade
  isolated lab, egress-filtered network). Proposals are written
  as files to a watched directory; an out-of-band process
  (sneaker-net, data diode) carries them across the air gap.
- **Integration cost**: ~2–3 days. Adapter serializes the
  `Proposal` as `<batch_label>-<timestamp>.proposal.json` into a
  deposit directory; `status` reads a sibling `.status.json`
  written by the out-of-band processor.
- **Trait surface**: `submit` (file write); `status` (file read).
  No network, no auth.
- **Critical invariant**: the JSON file format IS the wire
  format. Forward-compat fields (`signatures`,
  `change_classification`) round-trip through the JSON.

### `transport_dolt` — proposals as Dolt PRs

- **Trigger**: storage migrates to Dolt per ADR-018
  §`storage_dolt`. Dolt has a native PR mechanism; proposals
  become Dolt PRs in the same shape as today's GitHub PRs.
- **Integration cost**: ~3–5 days (paired with the Dolt storage
  adapter). Adapter opens a Dolt branch, applies the diff via
  Dolt SQL, opens a Dolt PR via the Dolt server API.
- **Trait surface**: `submit`, `status`. Substrate moves from git
  to Dolt; trait shape unchanged.
- **Critical invariant**: storage and proposal adapters move
  together. `PARTREG_STORAGE_BACKEND=dolt` paired with
  `PARTREG_PROPOSAL_SINK=github_pr` is incoherent; the CLI wiring
  rejects incompatible pairs at startup.

## Rationale

**Why a trait at all.** The wrapper-versus-trait calculus from
ADR-018 §Rationale applies identically: four future adapters are
named with concrete triggers; at least one
(`transport_local_branch`) activates the moment ADR-023's
defer-signing offline mode lands. The wrapper collapses on the
first second adapter; the trait costs the same up front and
absorbs all four without re-litigation.

**Why submit + status only.** The trait deliberately excludes
`merge`, `close`, `comment`. ADR-016 §"CI is the policy
authority" places acceptance with CI + reviewers, not with the
binary that authored the proposal. A `merge()` method would be
an attractive nuisance for the same reason ADR-018's missing
`update_part` is: a future adapter could implement it
("test-only" auto-merge for fixtures), tests would pass, and the
policy invariant would silently break. Structural incapacity
beats reviewer discipline.

**Why the payload carries advisory `change_classification`.** Per
ADR-016, CI is the authoritative classifier. But the FE needs to
show the operator a "this will auto-merge" / "this needs review"
signal at submission time, before CI runs. The proposal carries
the FE's advisory result so CI can detect divergence (FE bug,
stale classifier, evasion attempt) and record it in the audit
log. Missing the field would make divergence invisible.

**Why `signatures` on the payload, not just on the commit.**
ADR-023 §"Schema forward-compatibility" requires signatures to
round-trip the entire pipeline. If `Signature` only existed on
the post-merge commit, offline proposals (`transport_local_branch`)
signed with a deferred scheme couldn't carry signatures across
the air gap into the eventual PR. Placing signatures on the
payload makes every adapter round-trip them by construction.

**Why the data repo, not the code repo.** Code repo is versioned
source; data repo is versioned registry rows. Mutations target
rows; source-code commits go through the developer's ordinary git
workflow, outside the ADR-013/016/019 pipeline. Conflating them
would force the registry classifier over Rust source it has
nothing to say about.

**Why REST API and not `gh` subprocess.** The WASM constraint is
decisive: ADR-017 step 8 targets a WASM-compiled core in the
browser. Subprocess invocation has no WASM equivalent; a REST
client over `reqwest` (native) and `wasm-bindgen`'s `fetch`
(WASM) compiles both ways. The specific client crate
(`octocrab`, `reqwest`-direct) is an adapter implementation
detail.

## Consequences

This ADR commits the project to:

- **Trait minimalism**: `submit` + `status`, full stop. Adding a
  method requires showing the alternative (route through CI,
  through `IdentityProvider`, through `Repository`'s
  audit-append) is structurally wrong.
- **Domain type ownership of forward-compat fields**: `Operator`,
  `Signature`, `ChangeClass` live in `crates/domain/` (shared
  with `Repository`'s `AuditEntry` per ADR-018). Adapters cannot
  opt out; renaming is an ADR-level change.
- **Repo split is operational**: CLI wiring resolves
  `PARTREG_PROPOSAL_SINK` against a data-repo URL distinct from
  the code repo. Mis-pointing at the code repo is an immediate
  startup failure (data-repo schema check fails on a code repo).
- **Adapter selection at the boundary**: domain crates never name
  a concrete adapter. Wiring lives in `cli/` per ADR-017 and
  reads `PARTREG_PROPOSAL_SINK` per ADR-021.
- **Parity-test discipline (ADR-027)**: any second adapter must
  pass the `port_tests` parity suite vs. `transport_github_pr`
  on a fixed corpus at `port_tests/corpus/proposal/` before
  merge. Parity means: given the same `Proposal`, the resulting
  acceptance trace is equivalent — same diff applied, same
  `Operator` recorded, same `Signature` round-tripped.
- **No merge / close / comment methods, ever**: load-bearing.
  Test fixtures simulate merge by writing post-merge state
  directly to the storage adapter under test, not by extending
  `ProposalSink`. Adding such a method requires superseding this
  ADR.
- **FE preflight classifier ships in the WASM core**: per
  ADR-016 CI is authoritative; per this ADR the FE attaches an
  advisory classification. That requires the classifier callable
  from the FE — compiled into the WASM core (ADR-017 step 8) or
  TS-mirrored as a temporary strangler-fig measure (ADR-014
  permits the latter).
- **`ProposalRef::url` is the audit-log citation form**: ADR-022
  cites a proposal by `url`. Adapters whose canonical reference
  isn't URL-shaped (`transport_filesystem` uses `file://...`,
  `transport_local_branch` uses `git+local://...`) must produce
  a parseable URL even if the scheme is adapter-specific.
- **Cross-adapter incompatibility is a startup error**: CLI
  wiring rejects incoherent pairings (e.g. `storage=dolt` +
  `proposal=github_pr`) at process start.

This ADR does **not** commit the project to:

- Building any future adapters. Each activates on its own
  trigger; this ADR only guarantees the trait shape accommodates
  them.
- A specific GitHub-client crate. `transport_github_pr/` chooses
  internally (`octocrab` is the working assumption, swappable).
- Building the broker service (issue #5). The
  `transport_webhook` adapter is the client side; the broker is
  a separate scope.
- An async trait. Synchronous today (CSV+git over REST is
  network-bound but blocking is acceptable at MVP scale).
  Deferred until a high-fanout adapter makes blocking painful.
- A retry / circuit-breaker policy. Adapters surface
  `Network` and `RateLimited` faithfully; the caller decides
  retry. Pushing retry into the trait conflates policy with
  mechanism.

## Open questions / supersession triggers

- **Idempotency key on `Proposal`**. Today double-submit opens
  two PRs. If duplicate-submission rates exceed ~1% in
  production, add an idempotency key from
  `(author, batch_label, diff_hash)`.
- **Streaming `status`**. Today it's a poll; 5-second polls fit
  the expected scale. Higher scales might want SSE / webhook
  push — that becomes a `transport_webhook` capability, not a
  trait change.
- **`dry_run(...)` method**. Redundant with the FE building the
  `Diff` before `submit`. Re-opens if a CLI-only workflow needs
  "show me the PR body that would be opened" without the FE.
- **Folding `Merged` and `MergedAfterReview` into one variant**.
  Separate today because ADR-022's audit log differentiates
  them. If audit collapses the distinction, the variants merge.
- **Multi-target submission (one proposal, two repos)**.
  Out-of-scope today; would require either caller-side
  duplication or a `compose(...)` combinator. Deferred until a
  real workflow surfaces.

## References

- [ADR-013 — Parts registry web app](ADR-013-parts-registry-web-app.md)
  §"Decision" (PR-driven mutation model)
- [ADR-014 — Web app architecture](ADR-014-web-app-architecture.md)
  (queue-and-batch-submit pattern, FE-side `localStorage` queue)
- [ADR-016 — PR-diff policy enforcement](ADR-016-pr-diff-policy-enforcement.md)
  §"CI is the policy authority", §"Auto-merge / requires-review /
  blocked classes"
- [ADR-017 — Rust core + ports/adapters](ADR-017-rust-core-ports-adapters.md)
  §"Workspace shape" (`crates/transport/`,
  `crates/transport_github_pr/`), §"Strangler-fig migration
  sequence" step 8
- [ADR-018 — Storage as a port](ADR-018-storage-port.md)
  §"Why read + audit-append only" (mutations route through
  `ProposalSink`)
- [ADR-020 — Identity & authorization as a port](ADR-020-identity-authorization-port.md)
  (`Operator` provenance, IdP claims at session open)
- [ADR-022 — Observability: tracing + audit trail](ADR-022-observability-tracing-audit.md)
  (audit log cites `ProposalRef::url`)
- [ADR-023 — Threat model + crypto-MVP scope](ADR-023-threat-model-and-crypto-mvp-scope.md)
  §"Schema forward-compatibility" (`signatures` round-trip),
  §"Offline behaviour (defer-signing, Option A)"
  (`transport_local_branch` trigger)
- [ADR-024 — Cryptographic baseline (MVP)](ADR-024-crypto-baseline-mvp.md)
  (`Signature` variants, `GitCommit` MVP shape)
- [ADR-027 — Port conformance + forward-compatibility tests](ADR-027-port-conformance-tests.md)
  (parity-test corpus and discipline)
- code-repo issue #5 — GitHub App / proposal broker
- ISO 13485:2016 §7.3 — design controls (change-control evidence)
- IEC 62304:2006/AMD1:2015 §5.7 — software change-control process
- `octocrab` — <https://github.com/XAMPPRocky/octocrab>
- Hexagonal / ports & adapters — Alistair Cockburn, 2005

## Corrections

- **2026-05-12** — #35 names the concrete data repos:
  `exo-pet/exopet-registry` (production audit-of-record) and
  `exo-pet/exopet-registry-sandbox` (throwaway sandbox). The
  `ProposalSink` adapter is built once and targeted per-deployment via
  `PART_REGISTRY__REPO__DATA_REPO_URL`. The code repo
  (`MorePET/part-registry`, this) stays open-source. Data repos start
  public (Pages-on-private requires GitHub Pro; the org upgrade is
  tracked at `exo-pet/exopet-registry#1`) and migrate to private once
  branch protection is available. `transport_github_pr`'s
  `GithubPrProposalSink` does not change — only the target field it's
  constructed with does. Phase 1 of #35 introduces the
  `RepoConfig.data_repo_url` field and XDG-based clone-path
  resolution (`Config::resolve_data_path`); Phases 2 + 3 wire the
  release artifact and the data-repo Pages workflows.
