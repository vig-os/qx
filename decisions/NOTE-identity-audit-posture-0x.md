# NOTE — Identity & audit posture for 0.x (what personas + signed commits already buy)

- Status: posture note (not an ADR). Clarifies ADR-036 for the 0.x
  trajectory; input to the future 1.0 e-signature/authorization ADR.
- Date: 2026-06-26
- Related: ADR-036 (audit identity / personas), ADR-037 (audit-trail
  integrity / anchor ledger), NOTE-189 (Part 11 / ALCOA+ gaps),
  ADR-016 (gate = policy authority), ADR-020 (identity+authz),
  ADR-023/024 (signing port).

## Purpose

Pin what the **persona list + git-commit-signature** model *already*
provides for **retrospective attribution** ("post identification"), so
0.x can ship an honest audit posture and 1.0 owns the e-signature
*ceremony*. This prevents both under-selling (0.x already gives strong
attribution) and over-selling (0.x is **not** a Part 11 e-signature
system — see NOTE-189 Gap 1).

## The split

The thing Part 11 / regulated records actually want is **two distinct
controls** that this project has historically blurred:

1. **Attribution + integrity (retrospective).** "Who did/approved this,
   and has the record been tampered with since?" — a *forensic,
   after-the-fact* property.
2. **Signing ceremony (prospective).** "At the moment of approval, the
   approver re-authenticated and asserted a *meaning* (authorship /
   review / approval)." — §11.50 / §11.70 / §11.200.

The persona+signed-commit model delivers (1) today. (2) is a separate,
*prospective* control = the e-signature rungs E1–E4, reserved for 1.0.

## What 0.x already buys (control 1 — strong)

- **Attributable (ALCOA / §11.10).** Every record change → a git commit
  → commit author/committer → a `personas` entry (`github_login` →
  named person + role). "Who did this" resolves to a named, role-bearing
  human.
- **Approval chain.** The **merge** is the accountable act, performed by
  a CODEOWNER who is ⊆ `personas`. "Who authorized this" is answerable.
- **Integrity / tamper-evidence.** A GitHub-verified signing key proves
  the commit content was not altered and came from that key-holder;
  append-only audit + the anchor ledger (ADR-037) make the whole chain
  tamper-evident and recoverable.
- **Weak non-repudiation of authorship.** If the signing key is
  sole-controlled, the author cannot plausibly deny authoring — ADR-036's
  "authorship, not identity."

→ This covers most of **§11.10** plus ALCOA **Attributable / Original /
Accurate-traceable**, and is genuinely **sufficient for post
identification** (forensic attribution).

## What 0.x does NOT buy (control 2 — reserved for 1.0)

- **Meaning of signature** (§11.50) — a signed commit does not capture
  "*I, as QA, approve this*" as a deliberate, meaning-tagged act.
- **Re-authentication at signing** (§11.200) — an enrolled, session-
  authenticated GitHub identity ≠ fresh per-act MFA.
- **Two-component, positively-binding per signing** (§11.200).
- **Identity proofing** — `github_login` → persona is an *assertion*,
  not proofed to the human.

→ These are the e-signature **ceremony** (prospective), implemented as
rung **E1** (in-record signature manifest `{signer, printed_name,
signed_at, meaning_of_signature, auth_event_ref}` over the merge content
hash) and above (E2 WebAuthn UV → E3 PAdES → E4 QES).

## Posture statement (on the record)

- **0.x claim:** "attribution + integrity + tamper-evident audit trail;
  **Part 11 §11.10-ready**." It is **not** an e-signature system, and no
  0.x approval may be called a Part 11 e-signature (NOTE-189 Gap 1).
- **1.0 gate:** ship rung **E1** (in-record signing, meaning-tagged,
  bound to the merge hash, gate-verified) + the validation dossier →
  then a defensible **§11.50 / §11.70 / §11.200** claim holds.
- The `AuditEntry.signatures: Vec<Signature>` schema is already
  forward-compatible for E1 (ADR-023/024), so 0.x records do not need
  rewriting when the ceremony lands.

## Why this matters for the roadmap

This is the load-bearing reason 0.x is honest without e-signatures: the
*attribution* an auditor needs for traceability is already there; the
*ceremony* is a clearly-scoped 1.0 addition, not a foundational gap. It
lets the engine + parts + governance mature through 0.x while the
formal e-signature/validation claim is deferred to a single, well-fenced
1.0 milestone.
