# ADR-023 — Threat model + crypto-MVP scope

- Status: Proposed
- Date: 2026-05-10
- Component / area: cross-cutting — defines the security posture all
  other ADRs are built against
- Reviewers: _(pending)_
- Related: ADR-016 (PR-diff policy), ADR-017 (Rust core), ADR-020
  (Identity port), ADR-022 (Observability), ADR-024 (Crypto baseline),
  ADR-025 (Distribution integrity)

## Context

Several ADRs in this folder commit the project to security-relevant
behaviour: ADR-016 makes CI the policy authority; ADR-014 names
"audit-grade" as a property of the registry's history; ADR-015
positions `print_log.csv` as a non-destructive audit trail; the
medical-device QMS framing in [`METHODOLOGY.md`](METHODOLOGY.md) lists
ISO 13485 §7.3, ISO 14971, IEC 62304, and Notified Body / FDA review
as the audiences of record.

Without an explicit threat model, every downstream cryptographic
choice — what to sign, with what key material, with what verifier,
for how long — is being made in a vacuum. Different threat models
produce wildly different costs (a CI-only signing key is hours of
work; a per-operator hardware-backed key with rotation is months).
This ADR fixes the model so the rest of the architecture has a
referent.

It also fixes the **MVP crypto scope**. The full posture the threat
model implies (per-row signing with Sigstore-keyless, Rekor anchoring,
hash-chained audit log, reproducible signed releases) is multi-week
engineering. The MVP scope is narrower, with explicit re-open triggers
for each deferred control — captured here so an auditor reading the
ADR knows what is in force today and what is queued for activation.

## Alternatives considered

### Option set A — what threat model to fix

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| **No formal threat model**, decide crypto controls case-by-case as features land | Lowest friction to ship features | Inconsistency across ports; auditor cannot reconstruct *why* a control was chosen; controls drift between ports | Rejected — incompatible with the QMS audit posture in METHODOLOGY |
| **STRIDE-only** (categorise threats: spoofing, tampering, repudiation, disclosure, DoS, elevation) | Mainstream, tool-supported | Per-category enumeration without an asset/adversary model produces noise; not what the QMS audiences need | Rejected — wrong abstraction layer for this project |
| **Adversary × Asset × Consequence × UX-ceiling matrix**, fixed in this ADR and cited by every downstream control ADR | Each axis is a load-bearing engineering constraint; matrix is small enough to read in one page; downstream ADRs cite specific cells | Requires explicit answers on each axis (the threat-model interview) | **Chosen** — gives the rest of the architecture a finite, citable referent |

### Option set B — MVP crypto scope

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| **Sigstore-everywhere from day one** — per-row Fulcio signing, Rekor anchoring, self-hosted Sigstore stack, Cosign for releases | Maximum audit-grade rigour from the first commit; no future migration | 2–3 weeks engineering before any feature work resumes; ops burden of self-hosted Fulcio + Rekor; over-builds for the regulatory tier this project is at today | Rejected — disproportionate cost vs. the immediate threat reduction |
| **Hybrid (git signed commits + Sigstore for per-row & long-term anchor)** | Honours "no operator key management" UX constraint via Sigstore's keyless flow; rigorous | ~1 week to stand up self-hosted Sigstore; ongoing ops; still sizeable up-front cost | Rejected for MVP — kept as the documented next step |
| **Git-only (signed commits + branch protection + signed tags + reproducible builds)** as MVP, with the data model and trait shapes designed forward-compatible so Sigstore drops in as adapter swaps | ~2 hours to enable; familiar developer workflow; defends against the most likely adversaries today; bolt-on path to full crypto preserved | Long-term verifiability tied to GitHub remaining authoritative; per-row attribution at commit granularity, not per-row; operators must hold a signing key (GPG or SSH); CI bot key compromise = forged signature | **Chosen** — pragmatic for the current regulatory tier; explicit re-open triggers documented below |
| **Defer all crypto** — username strings in CSVs, no signed commits | Zero engineering | Indefensible against insider tampering; not auditor-defensible at any tier above "internal lab tool" | Rejected — incompatible with the QMS audit posture |

## Decision

The threat model and the MVP crypto scope are **fixed in this ADR**
and cited by every downstream control ADR. Both are summarised in two
matrices below.

### Threat model — fixed

**Adversary tier** (multi-select; adversaries the system must defend
against):

| Adversary | In scope | Notes |
|---|---|---|
| External attacker (network/web) | ✅ | Defended via TLS, signed releases, branch protection, SRI on FE WASM |
| Insider with repo write access | ✅ | Defended via signed commits + branch protection; per-row attribution requires successor ADRs once activated |
| Compromised CI runner | ✅ | Forces signing to live on operator side; CI is verifier-only. CI bot tokens limited in blast radius (no merge to main, no tag push) |
| Compromised operator device | ❌ | Out of scope for MVP. Defending requires hardware-backed keys (YubiKey, Secure Enclave) — re-opens this ADR if compromised-device becomes in-scope (re-open trigger T1 below) |

**Asset tier** (multi-select; assets that must be protected):

| Asset | In scope | Notes |
|---|---|---|
| Registry contents (file integrity) | ✅ | Git's content-addressing + signed commits + branch protection cover this in MVP |
| Per-part chain of custody (every row's history) | ✅ | MVP: per-commit attribution via signed commits. Per-row attribution deferred to ADR-024 successor when re-opened |
| Long-term non-repudiation (years/decades) | ✅ | MVP: tied to GitHub remaining authoritative. Sigstore Rekor anchoring deferred (re-open trigger T2 below) |

**Consequence tier**: **Regulatory finding / contractual breach.**

This rules out "embarrassing-only" MVP shortcuts and rules out the
formal-verification + HSM tier required by life-critical regimes. The
project sits in the audit-grade tier: ISO 13485 / IEC 62304 /
Notified Body review must be able to follow the chain.

**UX ceiling** (multi-select; signing must accommodate all):

| Surface | Signing model |
|---|---|
| Phone scanner | Identity = OIDC login (browser passkey acceptable). MVP defers per-action signing; mutations queued client-side and signed on commit by whoever submits the PR |
| CLI / dev machine | Identity = git config (commit author); signing key = the developer's existing GPG or SSH key registered to GitHub |
| Kiosk / shared workstation | Identity = OIDC login at session start; mutations attributed to the logged-in operator via the audit log (ADR-022); signing aggregated at PR-submit time |

The hard UX constraint stated in the threat-model interview is: **no
new bespoke key infrastructure for operators.** No GPG keys minted
for non-developers, no per-operator SSH keys, no rotating tokens to
manage. Signing piggybacks on identity infrastructure operators
already use (GitHub login, existing SSH/GPG keys for developers).

### MVP crypto scope — fixed

In force from ADR-024 acceptance:

1. **Signed commits** required on `main` of every governed repository
   (code repo + data repo, see ADR-019 split). GitHub branch
   protection enforces.
2. **Branch protection** on `main`: no force-push, no delete, signed
   commits required, ≥1 review required for human-authored PRs.
3. **Signed git tags** for every release, signed by the release
   manager's key.
4. **Reproducible Rust builds** (per ADR-017): `--locked`, pinned
   toolchain, controlled environment. Anyone can rebuild from the
   tagged commit and verify the artifact hash matches.
5. **`Operator` struct in the audit log** (per ADR-022) carries
   `source` (`git_commit_author`, `github_oauth`, `oidc:<issuer>`)
   and `verified_at` so a future audit can distinguish unverified
   claims from IdP-attested ones.
6. **Forward-compatible data model** (per ADR-027): `AuditEntry`
   reserves `signatures: Vec<Signature>` and `chain_hash:
   Option<Hash>` columns. MVP populates `signatures` with one
   `GitCommit` variant per entry (recording the commit SHA the entry
   landed in); `chain_hash` is `None`. Storage adapters round-trip
   both fields blindly so the schema does not change when Sigstore
   activates.

Explicitly **deferred** (with re-open triggers below):

- Per-row Sigstore-keyless signing (Fulcio + Rekor)
- Self-hosted Sigstore infrastructure
- Hash-chained audit log beyond what git provides
- Per-action operator signatures separate from commit author
- WebAuthn / passkey-bound action signing
- Cosign-signed binary artifacts beyond signed git tags

## Rationale

The matrix in the Decision section directly maps each cell of the
threat model to a defensive posture. Three observations make this
specific scope coherent rather than arbitrary:

**(1) The adversaries in scope are defended by what git already
provides.** External attackers cannot bypass branch protection.
Insiders with repo write must hold a signing key registered to their
GitHub account, and any commit they make is traceable to that key.
Compromised-CI is constrained because CI tokens are scoped to
non-merge operations. Adding Sigstore on top of these does not
materially change the *probability of compromise*; it changes the
*provability of attribution after the fact*. That property is
load-bearing eventually but not load-bearing today.

**(2) The deferred controls all hinge on the same trigger: an
external auditor or regulator asking for proof that the current
controls do not provide.** Until that happens, the deferred controls
are insurance against a future request, not a defence against a
present threat. The ADR-027 forward-compatibility discipline ensures
the insurance premium (data model design, trait shapes) is paid up
front so the controls can be activated quickly when the trigger fires
— without a schema migration or a refactor.

**(3) The UX constraint ("no new bespoke key infrastructure") is the
single biggest reason the MVP is git-only.** Sigstore's keyless flow
honours this constraint elegantly, but standing up self-hosted
Sigstore is itself bespoke infrastructure. The MVP path leans on the
key infrastructure operators *already have* (GitHub-registered SSH or
GPG keys for developers; OIDC login for everyone else) without
introducing new cryptographic material that needs lifecycle
management.

The risk this accepts: the MVP cannot defend against a sophisticated
insider who wants to commit a row change on someone else's behalf
*and* has access to that someone else's signing key. It also cannot
defend against GitHub being compromised (the IdP for all attribution).
Both are accepted residual risks for the current regulatory tier; both
are explicitly addressed by deferred controls listed above and
re-openable via the triggers below.

## Consequences

This ADR commits the project to:

- **Citation discipline**: every downstream ADR (017, 018, 019, 020,
  022, 024, 025) cites the specific row(s) of the threat-model matrix
  it defends. An ADR introducing a new control without citing what
  threat-model cell it addresses is incomplete.
- **Schema forward-compatibility**: the audit log schema (ADR-022),
  the storage trait (ADR-018), and the signing trait (ADR-024) all
  reserve space for the deferred controls. Adding Sigstore later must
  not require a schema migration.
- **Operator key minimum**: developers committing to either repo must
  have a GPG or SSH key registered to their GitHub account for signed
  commits. This is documented in the contributing guide; CI rejects
  unsigned commits on PR.
- **Branch protection enforced**: both the code repo and the data
  repo (per ADR-019) have `main` branch protection with signed
  commits required, no force-push, no delete, ≥1 review for
  human-authored PRs. Configuration is captured in the repo settings
  and exported as a checked-in file (`.github/branch-protection.json`
  or equivalent — exact form per ADR-019).
- **Reproducible builds**: ADR-017's Rust workspace must produce
  byte-identical artifacts from the same source commit on supported
  build hosts. CI verifies on every release.
- **Operator struct in audit**: the `Operator` data type (per ADR-020
  and ADR-022) carries provenance fields (`source`, `verified_at`)
  even when the value is unverified. An unverified operator claim is
  recorded as such, not as if it were verified.
- **Audit log forward-compat**: per ADR-027, the audit-log schema
  test suite includes a "round-trip a Sigstore-shaped signature
  entry" case that must pass even though MVP code paths do not
  produce one.

This ADR does **not** commit the project to:

- Standing up Sigstore infrastructure (deferred until trigger T2).
- Per-row signatures (deferred until trigger T2 or T3).
- Hardware-backed keys (deferred until trigger T1).
- WebAuthn/passkey signing (deferred until trigger T2 or T4).

## Re-open triggers

This ADR is reviewed and reconsidered when any of the following
occurs. Each trigger names the specific deferred control(s) that
should activate when the trigger fires.

- **T1 — Compromised operator device enters scope.** Activated when
  the project ships to a deployment context where operator device
  compromise is a credible threat (e.g. shop-floor terminals shared
  by many transient operators, mobile devices outside corporate
  control). Activates: hardware-backed key requirement, WebAuthn /
  passkey-bound signing.
- **T2 — External auditor or regulator request.** Activated by a
  Notified Body, FDA reviewer, customer auditor, or QMS internal
  audit asking for cryptographic proof that the MVP controls do not
  provide (typical asks: "prove this row was signed by this
  individual at this time, independently of GitHub's continued
  existence"). Activates: per-row Sigstore-keyless signing, Rekor
  anchoring, self-hosted Sigstore infrastructure.
- **T3 — Operator-key friction becomes load-bearing.** Activated when
  the GPG/SSH key requirement for signed commits is observed to
  block non-developer operator workflows that the project decides to
  support directly (e.g. a lab tech with no command-line workflow
  needs to commit a bind directly). Activates: Sigstore-keyless flow
  via OIDC for non-developer operators.
- **T4 — Per-row attribution is required by a workflow.** Activated
  when a workflow requires distinguishing "operator A originated
  this row in a multi-row commit submitted by operator B" — for
  example, the queue-and-batch-submit pipeline (ADR-014) where the
  PR author is not the row author. Activates: per-row signatures in
  the audit log (orthogonal to whether they go through Sigstore or
  another mechanism).
- **T5 — Long-term verifiability gap is contested.** Activated if a
  party (auditor, customer, court) challenges the integrity of an
  audit-log entry whose verification chain depends on GitHub. The
  challenge does not need to succeed; the existence of the challenge
  is the trigger. Activates: Rekor anchoring of historical audit
  entries (retroactive where possible).
- **T6 — Consequence tier changes to "loss of life / safety-critical".**
  Activated if the project's deployment context expands to include
  life-critical applications. Activates: full Sigstore-everywhere
  posture, formal third-party crypto audit, HSM evaluation.

When a trigger fires, the activating event is recorded in `LOG.md`
with the date and the deferred control being activated. The
successor ADR (typically a new revision of ADR-024) supersedes the
MVP scope.

## Open questions / supersession triggers

- Whether the data repository (per ADR-019 split: code repo + data
  repo) needs a stricter MVP than the code repo. Argument for: the
  data repo is the audit-of-record; a successful tamper there is
  more consequential than a code-repo tamper. Argument against:
  same threat model applies; differential controls add complexity.
  Resolved with ADR-018 / ADR-019 if a difference materialises.
- Whether the deferred controls should be packaged as a single
  successor ADR ("ADR-024 v2") or as a sequence (ADR-024.1 Sigstore,
  ADR-024.2 hash-chained audit, etc.). Methodology supports either;
  preference is sequence so that partial activation is recordable.
- Whether public sigstore.dev usage is acceptable for CODE-side
  artefacts (binary releases, WASM bundles) even while the
  data-side stays on self-hosted Sigstore once activated. Code-side
  signatures publishing to a public Rekor log is not sensitive (the
  binaries themselves are public if the repo is OSS). Decision
  deferred to ADR-025.

## References

- [`METHODOLOGY.md`](METHODOLOGY.md) — QMS audience definitions
- [ADR-013 — Parts registry web app](ADR-013-parts-registry-web-app.md)
- [ADR-014 — Web app architecture](ADR-014-web-app-architecture.md)
- [ADR-015 — Print event log](ADR-015-print-event-log.md)
- [ADR-016 — PR-diff-based policy enforcement](ADR-016-pr-diff-policy-enforcement.md)
- [ADR-017 — Rust core + ports/adapters](ADR-017-rust-core-ports-adapters.md)
- [ADR-024 — Cryptographic baseline (MVP)](ADR-024-crypto-baseline-mvp.md)
- ISO 13485:2016 §7.3 (Design and development controls)
- ISO 14971:2019 (Application of risk management to medical devices)
- IEC 62304:2006/AMD1:2015 (Medical device software lifecycle)
- Sigstore project — <https://www.sigstore.dev/>
- The 12-factor app — <https://12factor.net/>
