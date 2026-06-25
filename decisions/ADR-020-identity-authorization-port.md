# ADR-020 — Identity & authorization as a port

- Status: Accepted
- Date: 2026-05-10
- Component / area: cross-cutting — defines the trait shape for
  operator identity and the authorization decision surface that every
  mutation in the system flows through; first adapters are
  `identity_git_config` (CLI), `identity_github_oauth` (FE), and
  `identity_env_user` (test/dev only)
- Reviewers: Lars Gerchow
- Related: ADR-013 (Parts registry web app — names "auth = GitHub
  identity" aspirationally), ADR-014 (Web app architecture), ADR-016
  (PR-diff policy enforcement — consumes `&Operator + &Diff`), ADR-017
  (Rust core + ports/adapters — names `crates/identity/` and the MVP
  adapter set), ADR-018 (Storage port — `Operator` stamps audit entries),
  ADR-019 (Proposal sink port — mutations carry `&Operator`), ADR-022
  (Observability — `AuditSource` provenance), ADR-023 (Threat model +
  crypto-MVP scope — UX constraint "no bespoke key infrastructure for
  operators" and `verified_at` semantics), ADR-024 (Cryptographic
  baseline — future Sigstore cert binding), ADR-027 (Port conformance
  + parity tests)

## Context

The Python tooling today ascribes operator identity by reading a
shell environment variable: `--operator $USER` (or a CLI flag with the
same default). The TypeScript FE has no operator concept at all; any
PR opened from the browser carries the GitHub-Pages anonymous reader's
identity (i.e. nothing). ADR-013 §"Decision" gestured at "auth =
GitHub identity" but did not specify what trait surface that resolves
to or how non-GitHub identity sources would integrate.

Four pressures collide on this gap:

1. **ADR-023 requires identity provenance on every mutation.** Each
   `AuditEntry` (per ADR-018 / ADR-022) carries an `Operator` with a
   `source` (which IdP attested this identity) and a `verified_at`
   timestamp (when, if ever, that attestation was checked). A bare
   `$USER` string offers neither. The audit story collapses on the
   first auditor question of the form "how do you know operator X
   actually bound part Y at time Z?"
2. **ADR-016 defines policy as a function of `&Operator + &Diff`.**
   The policy engine cannot decide whether to allow, warn, or block a
   candidate change without a typed `Operator` value richer than a
   string. It needs to know whether the actor is verified, whether
   the IdP provided role claims (e.g. `qms-approver`), and whether
   the action is destructive enough to require elevation.
3. **ADR-017 already names `crates/identity/`** with `IdentityProvider`
   as the trait and `identity_git_config` + `identity_github_oauth`
   as the MVP adapters. This ADR fills in what those types actually
   are so adapter implementations have a contract to build against.
4. **ADR-023's UX constraint is the load-bearing constraint.**
   ADR-023 explicitly forbids minting bespoke GPG/PGP keys for
   non-developer operators — an FDA-style audit trail must not
   require a lab technician to learn `gpg --gen-key`. Identity must
   piggyback on whatever IdP login the operator already has.

Authentication and authorization are not separable in this project. A
change-proposal workflow does not have "logged-in but no permissions"
as a useful state; it has "verified actor with capabilities" or
"unverified claim with proposal-only fallback." Both concerns live in
`crates/identity/` and both consume the same `Operator` value.

## Alternatives considered

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| **Status quo: `--operator $USER` strings, no verification, no provenance** | Zero implementation cost; works today | Trivially spoofable (`--operator anyone`); ADR-023 demands `source` + `verified_at` on every audit entry and a string carries neither; no policy hook (ADR-016 needs typed values to decide elevation); no FE story (browser has no `$USER`) | Rejected — the string lacks every property downstream ADRs require |
| **GitHub-only hardcoded auth** (use `octocrab` directly in `bind`/`mint`/`label`, no abstraction) | Simple; matches ADR-013's aspirational "auth = GitHub identity" literally | Locks the model to one IdP forever; no path to generic OIDC for non-GitHub orgs (Microsoft/Okta/Google customers exist in the project's plausible roadmap); no path to mTLS kiosk auth (shop-floor terminals can't run a browser OAuth flow); refactoring later costs the same as designing the trait now | Rejected — IdP diversity is a near-certain requirement and the lock-in cost is gratuitous |
| **Single `IdentityProvider` trait + sibling `Authorizer` trait, MVP adapters `identity_git_config` (CLI) + `identity_github_oauth` (FE) + `identity_env_user` (test/dev), generic OIDC + mTLS + Sigstore-keyless as future adapters** | Extensible (one new file per future IdP); parity-testable per ADR-027; honors ADR-023 UX constraint (operators reuse existing IdP login, no bespoke key minting); typed `Operator` carries the provenance ADR-023 / ADR-022 / ADR-016 all need; auth + authz land in one crate so the policy hook is local | Up-front trait design cost; team must hold the line on `&Operator` as a parameter to every mutation (no string fallback) | **Chosen** — the only option that satisfies all four constraints simultaneously |
| **Sigstore-keyless from day one** (every identity is a Fulcio-issued short-lived cert) | No long-lived key management; keyless model is forward-looking; matches ADR-024's eventual destination | Full Sigstore stack (Fulcio + Rekor + cosign) is explicitly deferred per ADR-023 to keep MVP scope manageable; OIDC dependency for cert issuance still requires the IdP integration this ADR is designing; doubles MVP integration surface for a property the MVP threat model does not yet require | Rejected for MVP — preserved as future adapter `identity_sigstore_keyless`; activates on ADR-023 trigger T2 (auditor request) or T3 (operator-key friction) |
| **Separate `Authentication` and `Authorization` crates** | Textbook separation of concerns | The two concerns share a value (`Operator`) and are consumed at the same call sites; splitting invents a coordination surface (which crate owns the `Operator → Capabilities` mapping) for no observed benefit; one crate with two traits captures the conceptual split without coordination cost | Rejected — co-location matches actual call-site needs |

## Decision

Identity and authorization are a single port in `crates/identity/`.
The crate exposes two traits: `IdentityProvider` (produces `Operator`
values) and `Authorizer` (turns `Operator + Action` into
`AuthDecision`).

MVP adapters:

- `crates/identity_git_config/` — CLI surface. Operator identity is
  the git commit author (`user.name` + `user.email` from active git
  config). `source: GitConfig`, `verified_at: None` because the
  values are a self-asserted claim, not a verified attestation.
- `crates/identity_github_oauth/` — FE surface. Browser OAuth device
  flow against GitHub yields a verified GitHub user identity.
  `source: GitHubOAuth`, `verified_at: <timestamp from token issued_at>`.
- `crates/identity_env_user/` — test and development only. Reads
  `$USER` and returns `source: EnvUser, verified_at: None`.
  Production builds **reject** this adapter at construction time
  (constructor checks `cfg!(debug_assertions)` or an explicit
  `PARTREG_ALLOW_DEV_IDENTITY=1` env var per ADR-021).

### Type and trait shapes

The types live in `crates/domain/` (per ADR-017) because `Operator`
is referenced by `AuditEntry` (per ADR-018), the proposal sink (per
ADR-019), and the policy engine (per ADR-016). The traits live in
`crates/identity/`.

```rust
// crates/domain/src/identity.rs

use std::collections::BTreeMap;
use time::OffsetDateTime;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct OperatorId(pub String);  // canonical, e.g. "github:lars-gerchow"

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdentitySource {
    GitConfig,
    GitHubOAuth,
    OidcGeneric { issuer: String },     // future
    MtlsCert { fingerprint: String },   // future
    EnvUser,                             // test/dev only
    OfflineClaim,                        // future — see ADR-023 offline mode
    SigstoreKeyless { fulcio: String },  // future — see ADR-024
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyId(pub String);  // forward-compat per ADR-024

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Operator {
    pub id: OperatorId,
    pub display_name: String,
    pub source: IdentitySource,
    pub verified_at: Option<OffsetDateTime>,  // None for unverified claims
    pub claims: BTreeMap<String, String>,     // arbitrary IdP-provided claims
    pub pubkey: Option<KeyId>,                // ADR-024 forward-compat
}
```

```rust
// crates/identity/src/lib.rs

pub trait IdentityProvider: Send + Sync {
    fn current(&self) -> Result<Operator, IdentityError>;
    fn refresh(&self) -> Result<Operator, IdentityError>;
    fn capabilities(&self, op: &Operator) -> Capabilities;  // for ADR-016 policy
}

pub trait Authorizer: Send + Sync {
    fn authorize(&self, op: &Operator, action: &Action) -> AuthDecision;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthDecision {
    Allow,
    Warn { reason: String },
    Block { reason: String },
    RequiresElevation { approver_role: String },
}

#[derive(Clone, Debug, Default)]
pub struct Capabilities {
    pub can_propose: bool,
    pub can_approve_destructive: bool,
    pub roles: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    #[error("no identity available: {0}")]
    NoIdentity(String),
    #[error("verification failed: {0}")]
    VerificationFailed(String),
    #[error("backend error: {0}")]
    Backend(#[source] Box<dyn std::error::Error + Send + Sync>),
}
```

`Action` is the value the semantic-diff classifier from ADR-016 emits
(`row_add`, `row_delete`, `row_void`, `row_bind`, `row_edit`,
`header_change`, `bulk_change`). The classifier produces `Action`s
from a `Diff`; the `Authorizer` maps `Operator + Action` to
`AuthDecision`. This composition is the entire policy engine surface
— ADR-016's CI gate and the FE preflight both consume it.

### MVP authorization policy

The default `Authorizer` shipped with MVP is a fixed table:

| Operator state | Action class | Decision |
|---|---|---|
| `verified_at: None` (any source) | any non-read | `Block { reason: "unverified actor; sign in with the IdP first" }` |
| verified, no `qms-approver` claim | `row_add`, `row_bind`, `row_edit` | `Allow` |
| verified, no `qms-approver` claim | `row_delete`, `row_void`, `header_change`, `bulk_change` | `RequiresElevation { approver_role: "qms-approver" }` |
| verified, claims contain `qms-approver` | any | `Allow` |
| `source: EnvUser` in a release build | any non-read | `Block { reason: "dev identity not permitted in release" }` |

Hardcoded at MVP. Externalising to config is a future ADR.

### Adapter selection

Per ADR-021, the active adapter is selected by env var:

```
PARTREG_IDENTITY_PROVIDER=git_config        # MVP CLI default
PARTREG_IDENTITY_PROVIDER=github_oauth      # MVP FE default (build-time)
PARTREG_IDENTITY_PROVIDER=env_user          # test/dev only
PARTREG_IDENTITY_PROVIDER=oidc_generic      # future
PARTREG_IDENTITY_PROVIDER=mtls_cert         # future
PARTREG_IDENTITY_PROVIDER=sigstore_keyless  # future
```

The `cli/` crate's wiring code matches on this value and constructs
the appropriate `Box<dyn IdentityProvider>`.

### Credential resolution order (refinement — 2026-06-10)

> Additive refinement, driven by ADR-030's multi-shell surface. It does
> not change any decision above (trait shapes, `Operator`, the authz
> table); it specifies *where the GitHub token comes from* before the
> interactive device flow — a gap the original ADR left implicit. The
> `identity_github_oauth` adapter (and the CI path) resolve credentials
> first-hit-wins, every step reusing existing IdP state per the ADR-023
> "no bespoke keys" constraint:

1. **Explicit token** — `config.github_token`
   (`PART_REGISTRY__TRANSPORT__GITHUB_TOKEN`); also accept the
   conventional `GITHUB_TOKEN` / `GH_TOKEN` names. This is the CI path.
2. **Cached token** — `FileTokenStore`
   (`~/.config/qx/github-token.json`) from a prior login;
   `verified_at` = token issue time.
3. **Borrow system auth** *(opt-in adapter, no hard dependency)* —
   `gh auth token`, a git credential helper, or the OS keychain, **if
   present**. Used only when available, so the toolbelt never *requires*
   `gh`; the borrowed token still attests the user via `GET /user`
   (`source: GitHubOAuth`).
4. **Interactive device flow** — `github.com/login/device` (the MVP
   path). Native shells only.

**Per-shell consequence (ADR-030).** A serverless web shell (GH Pages)
**cannot** run the device flow: GitHub's device/token endpoints are not
CORS-enabled for browser JS. Its write-path credential must therefore
come from a backend/proxy — the `pr serve` / GitHub App named in
ADR-030. Read of a public registry stays anonymous. This is the one
shell that cannot self-serve write auth, and the concrete reason the
GitHub App exists.

### `&Operator` is a structural parameter

Every state-changing call in the workspace takes `&Operator` as a
typed parameter: `ProposalSink::submit(...)` (per ADR-019),
`AuditEntry` (per ADR-018), the policy engine (per ADR-016). There is
no API in the workspace that mutates state without an `Operator` in
scope. The default `Authorizer` is invoked at the proposal-sink
boundary; a `Block` decision short-circuits the submission with a
typed error.

## Rationale

**Why a port, not a hardcoded GitHub integration.** ADR-013's "auth =
GitHub identity" was a placeholder for "the IdP we happen to use
today," not a permanent commitment. The portability matrix in ADR-017
lists deployments (kiosk stations, embedded handhelds, Microsoft/Okta
customers) where GitHub is structurally not the IdP. A hardcoded
integration would require a refactor on the first non-GitHub
deployment; a trait absorbs every plausible IdP for the same up-front
cost.

**Why ADR-023's UX constraint is load-bearing.** The single largest
risk to operator adoption is making a lab technician learn key
management to scan a part. ADR-023 §"MVP scope — fixed" is explicit:
no bespoke key infrastructure for operators beyond what their existing
IdP login provides. Every MVP adapter piggybacks on existing IdP state
— git config (already present on every dev machine), GitHub OAuth
device flow (no key minting, browser handles it). Future adapters
preserve the property: OIDC piggybacks on the customer's IdP, mTLS
piggybacks on station-issued certs the IT department already manages,
Sigstore-keyless removes long-lived keys altogether.

**Why authentication and authorization in one crate.** The two
concerns share `Operator` and are consumed at the same call sites.
Splitting them would invent a coordination surface (which crate owns
`Operator → Capabilities`) that solves no observed problem. One crate
with two traits captures the conceptual split without paying
coordination cost.

**Why `verified_at: Option<Timestamp>`, not a `Verified` /
`Unverified` enum.** The timestamp is itself audit-relevant — knowing
*when* an attestation was last checked matters for stale-token policy.
The boolean case is recoverable from `Option::is_some()` whenever a
caller wants it; the timestamp form loses no information and gains the
audit hook.

**Why `claims: BTreeMap<String, String>` instead of typed roles.**
Different IdPs surface different claim shapes (GitHub OAuth has
`scopes`, OIDC has issuer-defined `id_token` claims, mTLS has cert SAN
entries). Forcing a typed schema at MVP would either under-fit (drop
adapter-specific data the policy might want) or over-fit (force
adapters to populate fields they don't have). `Capabilities` is the
typed projection the policy engine consumes; adapters own the
claim-to-capability mapping.

**Why `EnvUser` is a real adapter, not a debug shim.** Test fixtures,
conformance suites (per ADR-027), and local development all need
*some* identity. A shim that bypasses the trait during tests means
the test path and the production path diverge — the trait isn't
actually exercised by tests. A real adapter that the trait sees as
just another provider keeps test and production paths structurally
identical; the production-build rejection is enforced at construction
time.

**Why MVP authorization is a hardcoded table, not config.** Every
deployment of the MVP is the same project's own use. No deployment
exists yet with custom policy needs; configuring a non-existent need
would be premature abstraction.

## Consequences

This ADR commits the project to:

- **`Operator` is a typed parameter on every mutation.** No call site
  in the workspace mutates state without an `Operator` value in
  scope. Adding a new mutation surface without an `Operator`
  parameter requires superseding this ADR.
- **`AuthDecision` is checked at the proposal-sink boundary.** The
  default `Authorizer` runs before `ProposalSink::submit(...)`
  proceeds; CI re-runs the same `Authorizer` against the actual PR
  diff (per ADR-016) so FE preflight and CI use the same logic.
- **Forward-compat fields on `Operator` are mandatory.**
  `pubkey: Option<KeyId>` exists at MVP even though no adapter
  populates it; future Sigstore integration (per ADR-024) fills it
  in. Adapters that strip it on round-trip fail conformance tests.
- **`identity_env_user` is rejected by release builds.** Constructor
  checks `cfg!(debug_assertions)` or `PARTREG_ALLOW_DEV_IDENTITY=1`
  (per ADR-021); a release binary that finds neither returns
  `IdentityError::NoIdentity` and refuses to construct.
- **GitHub OAuth device flow is an external dependency.** Outage
  there blocks the FE login flow. Mitigation: the CLI path (using
  `identity_git_config`) remains independent.
- **Conformance-test discipline (ADR-027).** Every adapter must pass
  the `port_tests` `IdentityProvider` and `Authorizer` suites:
  `current()` returns a well-formed `Operator`, `refresh()` is
  idempotent for unchanged state, `verified_at` semantics match the
  adapter's documented contract, default `Authorizer` decisions are
  stable.
- **Audit trail enrichment (ADR-022).** Every `AuditEntry` written
  by the storage adapter (per ADR-018) embeds the `Operator` value
  used at the time of the action.

This ADR does **not** commit the project to:

- Building any future adapter (`identity_oidc_generic`,
  `identity_mtls_cert`, `identity_offline_claim`,
  `identity_sigstore_keyless`). Each activates on its own trigger.
- A user-management database. The project does not own user records;
  it consumes whatever the IdP returns.
- A session model. `IdentityProvider::current()` is called per
  operation; there is no logged-in/logged-out lifecycle the runtime
  tracks.
- Custom role definitions at MVP. The default `Authorizer`
  recognises `qms-approver` for elevation; everything else is `Allow`
  or `Block`.
- A signing infrastructure. Per ADR-024, signing is a separate port
  (`SigningProvider` / `VerificationProvider`). `Operator.pubkey` is
  forward-compat for the day signing lands.

## Forward-compatibility

For each future adapter: trigger, integration cost, trait surface used.

### `identity_oidc_generic` — generic OIDC for non-GitHub orgs

- **Trigger**: a customer or partner deployment requires login via
  Microsoft / Okta / Google Workspace / Keycloak — any IdP that
  speaks OIDC but is not GitHub. Plausible inside 12 months given
  the project's intended QMS-customer audience.
- **Integration cost**: ~2 days. OIDC discovery + device-code flow
  via `openidconnect-rs`; adapter is mostly configuration plumbing
  (issuer URL, client ID, scopes) plus claim-extraction.
  `IdentitySource::OidcGeneric { issuer }` carries the issuer URL.
- **Trait surface used**: full `IdentityProvider`. The default
  `Authorizer` already handles the resulting `Operator` via the
  `verified_at` + `claims` projection.

### `identity_mtls_cert` — kiosk auth via station-issued cert

- **Trigger**: shop-floor terminal deployment where operators do not
  have personal browser sessions and OIDC device flow is impractical.
  The station holds a long-lived mTLS cert issued by the org's PKI.
- **Integration cost**: ~1 week. Cert validation against the org's
  CA chain, claim extraction from cert SAN / extensions, optional
  card-reader integration. `IdentitySource::MtlsCert { fingerprint }`
  carries the cert fingerprint; `verified_at` is the start of the
  validated cert window.
- **Trait surface used**: full `IdentityProvider`. `Capabilities`
  may include a `station-operator` role distinct from `qms-approver`.

### `identity_offline_claim` — unverified claim during offline operation

- **Trigger**: ADR-023's offline-mode story becomes load-bearing (a
  deployment must function without network for hours-to-days).
  Operators continue working; their actions are recorded as
  `IdentitySource::OfflineClaim` with `verified_at: None`. At sync
  time the claims are bound to a real verified identity.
- **Integration cost**: ~1–2 weeks including the sync-time binding
  protocol. Trait surface unchanged; binding is a separate flow that
  produces post-hoc `AuditEntry` updates.
- **Trait surface used**: full `IdentityProvider`. The default
  `Authorizer` blocks unverified claims; an offline deployment
  overrides with a deployment-specific `Authorizer` that allows
  offline claims with reduced capabilities.

### `identity_sigstore_keyless` — full Sigstore-keyless identity binding

- **Trigger**: ADR-023 trigger T2 (auditor request for cryptographic
  non-repudiation beyond git-commit signatures) or T3 (operator
  feedback that long-lived OAuth tokens are operationally painful).
- **Integration cost**: ~2 weeks. Fulcio cert issuance, Rekor log
  inclusion, cosign verification path. The OIDC piece reuses
  `identity_oidc_generic`; net new work is the cert binding and the
  Rekor entry. `Operator.pubkey` populates with the Fulcio cert
  fingerprint. `IdentitySource::SigstoreKeyless { fulcio }` carries
  the Fulcio instance URL.
- **Trait surface used**: full `IdentityProvider` plus the `pubkey`
  forward-compat field.

## Open questions / supersession triggers

- **Whether the default `Authorizer` table belongs in
  `crates/identity/` or a separate `crates/policy/`.** Today it
  lives in `identity/` because no other policy logic exists.
  Re-opens when the table reaches ~20 rules or when externalising it
  is requested.
- **Whether `Operator` should be `Arc`-shared or owned.** Today the
  type is `Clone`-cheap. At higher claim cardinality (large OIDC
  tokens, x509 cert chains) `Arc<Operator>` becomes attractive.
  Re-opens if profiling shows clone cost on the hot path.
- **Whether `IdentityProvider::refresh()` should be `async`.** Today
  the trait is synchronous. Future adapters with non-trivial network
  I/O on refresh (OIDC token rotation, mTLS OCSP check) may want
  `async`. Deferred until the first network-heavy refresh adapter is
  on the roadmap.
- **Whether the `EnvUser` adapter should be behind a `dev-identity`
  feature flag.** Today it ships in the workspace and the constructor
  enforces the dev-only invariant at runtime. A feature flag would
  catch the invariant at compile time. Re-opens if a release build
  ever ships a working `EnvUser` adapter by accident.
- **Whether GitHub OAuth scopes should be requested minimally or
  liberally.** MVP requests `read:user` only. Future workflows
  (auto-PR creation per ADR-019) may need `repo` scope. Decision
  deferred to the proposal-sink integration ADR.

## References

- [ADR-013 — Parts registry web app](ADR-013-parts-registry-web-app.md)
  §"Decision" (mentions "auth = GitHub identity" aspirationally)
- [ADR-014 — Web app architecture](ADR-014-web-app-architecture.md)
- [ADR-016 — PR-diff policy enforcement](ADR-016-pr-diff-policy-enforcement.md)
  §"Decision" (`Operator + Diff` policy input)
- [ADR-017 — Rust core + ports/adapters](ADR-017-rust-core-ports-adapters.md)
  §"Workspace shape" (`crates/identity/`, `identity_git_config`,
  `identity_github_oauth` as MVP adapters)
- [ADR-018 — Storage as a port](ADR-018-storage-port.md)
  §"Trait shape" (`AuditEntry.actor: Operator`)
- [ADR-019 — Proposal sink as a port](ADR-019-proposal-sink-port.md)
  (`ProposalSink::submit(...)` takes `&Operator`)
- [ADR-022 — Observability: tracing + audit trail](ADR-022-observability-tracing-audit.md)
  (`AuditSource` provenance, `Operator` propagation through tracing)
- [ADR-023 — Threat model + crypto-MVP scope](ADR-023-threat-model-and-crypto-mvp-scope.md)
  §"MVP scope — fixed" (no bespoke key infrastructure for operators;
  `Operator` carries `source` + `verified_at`)
- [ADR-024 — Cryptographic baseline (MVP)](ADR-024-crypto-baseline-mvp.md)
  (`Operator.pubkey` forward-compat for future Sigstore cert binding)
- [ADR-027 — Port conformance + forward-compatibility tests](ADR-027-port-conformance-tests.md)
  (`IdentityProvider` and `Authorizer` conformance suites)
- ISO 13485:2016 §7.3 — design controls (operator identification on
  every design output)
- IEC 62304:2006/AMD1:2015 §5.1.6 — software configuration item
  identification
- Hexagonal / ports & adapters — Alistair Cockburn, 2005
- GitHub OAuth device flow —
  <https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps#device-flow>
- OpenID Connect Core 1.0 —
  <https://openid.net/specs/openid-connect-core-1_0.html>
- Sigstore architecture — <https://docs.sigstore.dev/>
- `openidconnect-rs` — <https://github.com/ramosbugs/openidconnect-rs>
