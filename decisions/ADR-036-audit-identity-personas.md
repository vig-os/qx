# ADR-036 — Audit identity: personas collection + host-resolved accountability

- Status: Proposed
- Date: 2026-06-11
- Component / area: who an audit record is attributed to, and how that
  attribution is established. Resolves ADR-035's deferred "operators
  directory collection"; discharges the e-signature question raised by
  the platform/HQ exploration; revises the `file://` stance of
  ADR-030/033/034 (see their Corrections).
- Reviewers: Lars Gerchow (required for Accepted)
- Related: ADR-019 (proposal sink), ADR-020 (identity port), ADR-022
  (audit trail), ADR-023 (threat model), ADR-024 (signed commits),
  ADR-034 (host-enforced authz), ADR-035 (collections metamodel),
  ADR-037 (audit-trail integrity — sibling)

## Context

The audit stream's `operator` field needs a regulatory-grade answer to
"who is accountable for this record". Investigation of e-signature
options (Sigstore keyless, SSH/GPG commit signing, FIDO2/WebAuthn,
PAdES) against the project's accepted write-path produced three
load-bearing observations:

1. **GitHub repo allowance ≠ audit identity.** Host permission answers
   "may this account act"; it does not identify the accountable legal
   person. A collaborator without a registered identity can have access
   yet must not be able to produce an audit-valid record.
2. **A signed commit proves key possession, not personhood or intent.**
   An unlocked laptop, exfiltrated key file, or script satisfies it.
   Possession ≠ presence; and authorship of a *proposal* is not the
   accountable act in a PR-gated registry anyway — acceptance is.
3. **Truth is established host-side.** Per ADR-018/019/016/034, every
   mutation becomes registry truth only via a reviewed merge under
   branch protection. The live, 2FA-enrolled, host-witnessed approval
   at merge is a *higher-assurance* act than any off-platform key
   operation at a forgeable local timestamp.

## Alternatives considered

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| **Local cryptographic signature as the identity of record** (SSH/GPG key, persona→key map) | self-contained; no host dependency | possession ≠ presence; key custody burden (§11.300-class obligations); forgeable commit dates; authorship still isn't the accountable act | Rejected as identity; **retained as authorship + integrity evidence** |
| **Sigstore keyless per-act signing now** | no key custody; Rekor time; portable bundle | public log leaks identity/metadata (GDPR for EU operators — immutable, no erasure); needs Fulcio/Rekor at signing; adds infra for a property the host already witnesses | **Deferred** to an escalation rung (triggers below) |
| **Per-act WebAuthn/FIDO2 with user verification** | only mechanism proving *human presence per act* (biometric/PIN gates each assertion; key non-exfiltratable) | new ceremony + enrollment process; not needed while the accountable act is the host merge | **Deferred** rung (triggers below) |
| **Host-resolved accountability + a first-class persona registry** (chosen) | reuses enforced machinery (2FA accounts, branch protection, CODEOWNERS); identity becomes FK-checked data in the existing engine; no new key infra (ADR-017/023 goal: "no bespoke key infrastructure for operators") | evidence is host-witnessed, not a portable per-person cryptographic artifact — the explicit trade, with escalation rungs kept open | **Chosen** |

## Decision

### 1. `personas` is a collection (the ADR-035 metamodel, applied to identity)

A registry declares a `personas` collection: typed ids
(`persona:<slug>`), fields `{legal_name, roles[], github_login,
ssh_key_fprs[], oidc_subjects[], status}`, lifecycle
`active → suspended → revoked` (revocation is an audited `Transition`),
PR-gated like every collection. **The audit `operator` field is a typed
FK into it** — so "unknown persona ⇒ rejected" is the engine's existing
referential-integrity check, not a bespoke gate. Onboarding is a PR to
the collection approved by a persona-admin (CODEOWNERS on that path);
the genesis persona is seeded at bootstrap. ADR-034's `roles.toml`
dissolves into this collection (the `roles[]` field is the same data,
now FK-checked and lifecycle-managed; see ADR-034 Corrections).

### 2. Accountability resolves host-side at merge (accountable-approver default)

The identity of record for a merged change is the **approver/merger**,
authenticated live by the host (login + GitHub's mandatory-2FA
enrollment), resolved to a persona via `github_login`, and synced into
the audit stream (mechanics in ADR-037 §merge-sync) with **explicit
meaning derived from the transition the merge effected** — never left
implicit in the diff. A manifest knob selects the stricter mode:

- `accountable-approver` (default): unknown authors may propose;
  merging requires a registered approver, who is the accountable
  identity.
- `author-strict`: the gate additionally requires the commit
  author/signer to resolve to an active persona.

CI cross-checks: every CODEOWNERS principal and every approver on a
merged PR must resolve to an **active persona holding the required
role**; failure blocks, it does not silently grant.

### 3. Signed commits = authorship + integrity, not identity

Commit signing stays (it feeds tamper-evidence and authorship
non-repudiation per ADR-024) but is demoted from identity: it answers
"who drafted", not "who is accountable".

### 4. Elevation = the existing auth ladder, stated honestly

`RequiresElevation{approver_role}` (ADR-034) is satisfied by a
deliberate act under a 2FA-*enrolled*, host-authenticated identity with
the required role. **Precision the record must carry:** GitHub OAuth
cannot force (or prove) a *fresh per-act* MFA challenge from a
third-party app; the claim is "enrolled + authenticated + witnessed",
not "fresh 2FA at signing". Where fresh-per-act proof is demanded, that
is rung E2 below — not something to squeeze out of OAuth.

### 5. Write path: PR-is-truth; `file://` demoted

Every shell is a client of canonical remote state: reads serve a synced
snapshot of `origin/main` (staleness shown); **writes require
reconciling with `origin/main` and emit a proposal — error otherwise.**
A direct-write local mode (`file://` as a *write* target) becomes a
**flagged future feature** behind an explicit opt-in; this deletes the
ADR-034 §5 local-trust asymmetry from the default product (Corrections
filed on ADR-030/033/034). Known cost, accepted deliberately: offline
operation (including the print-event append) is blocked until either a
queued-proposal mode or the flagged local mode ships.

### 6. Escalation rungs (deferred, with triggers)

| Rung | Mechanism | Trigger |
|---|---|---|
| E1 — portable in-record signature | Sigstore/`sigstore-rs` bundle over the **entry content-hash** (not the commit — survives squash), embedded in the entry, verified by the gate, resolved to a persona | an auditor rejects host-custodial evidence; or local mode ships (there it is the *only* identity) |
| E2 — per-act human presence | app-driven WebAuthn/FIDO2 **with user verification** (per-act biometric/PIN; attestation-gated, device-bound credentials) over the same content-hash | "operator's unlocked machine misused" enters the threat model for accountable acts; or fresh-per-act MFA is demanded |
| E3 — document e-signature | PAdES (pyHanko) on rendered controlled documents, manifestation per §11.50 | controlled-document approval (QMS preset) ships |
| E4 — QES | qualified cert + TSP per eIDAS | a legal-equivalence requirement is filed |

GDPR note banked for E1: operator-identity in a *public* immutable log
(public Rekor) is personal data with no erasure path — EU deployments
use self-hosted/private anchoring for any operator-level signing.

## Rationale

The accountable act in this architecture is **acceptance into truth**,
which only happens host-side under enforced review — so identity
evidence belongs where the act happens. A persona registry as a plain
collection reuses every existing mechanism (descriptors, FK validation,
lifecycle, PR gating, audit) instead of inventing an identity subsystem;
"unknown persona is rejected" costs zero new code. Local cryptographic
ceremonies add real burdens (custody, enrollment, GDPR) and, against
the actual threats (warm unlocked session; account compromise), buy
little that device lock + host 2FA don't — until the specific triggers
above fire, at which point the rungs slot into the same `signature`
field and gate without redesign.

## Consequences

- New code-owned preset: `personas` collection descriptor; bootstrap
  seeds the genesis persona + the persona-admin CODEOWNERS path.
- Audit entry `operator` becomes `persona:<id>` (FK); validators gain
  the active-at-time-of-act check (answerable from collection history).
- ADR-034 `roles.toml` retired as a separate artifact (Corrections
  there); the CODEOWNERS seed generates from the personas collection.
- The gate gains: CODEOWNERS ⊆ active personas; approver-resolves-to-
  persona; mode knob `accountable-approver | author-strict`.
- ADR-030/033/034 carry Corrections demoting `file://` writes.
- The offline-printing wrinkle is now an explicit, tracked limitation.

## Open questions / supersession triggers

- Queued-proposal mode (offline capture, submit-on-reconnect) vs the
  full flagged local mode — which ships first, and under what flag.
- Enrollment assurance for `github_login` ↔ legal person binding
  (SSO/SAML enforcement on the org as procedural baseline).
- Whether `suspended` personas' historical records need re-attestation
  (current position: no — validity is judged active-at-time-of-act).

## References

- ADR-020 — Identity & authorization port (the table this externalizes)
- ADR-034 — Host-enforced authz (the enforcement this rides on)
- ADR-035 — Collections metamodel (the machinery personas reuse)
- ADR-037 — Audit-trail integrity & anchoring (sibling: the *what*)
- 21 CFR Part 11 §11.50/§11.70/§11.300; FIDO2/WebAuthn UV; Sigstore
