# Exploration — Platform vs registry: the "HQ" question

- Date: 2026-06-10 … 2026-06-11 (interactive sessions)
- Status: exploration — NOT a commitment (see METHODOLOGY). The decided
  subset graduated to ADR-036/037/038; what remains here is the
  platform expansion itself, which stays trigger-gated.
- Feeds: a future "QMS preset family" ADR if/when a trigger fires.

## The question

ADR-035 made the registry a git-native NDJSON entity store with
declared collections, lifecycles, relations, attachments, one audit
stream, host-enforced authz, and a self-describing contract. Should
that substrate expand into an "HQ" — folding in QMS artifacts (SOPs,
CAPAs, training records, DHF) — and what would actually be different
beyond the opinionated render and storage?

## Finding 1 — the platform already exists; `parts` is preset #1

Nothing about the engine is parts-specific (ADR-035 §0 says so
explicitly: one generic engine, descriptors, no special-cased file).
Controlled documents map onto existing primitives almost 1:1:

| QMS concept | Existing primitive | Gap? |
|---|---|---|
| SOPs / WIs / policies | `documents` collection; draft→review→approved→effective→retired = lifecycle; versions = git | none (descriptor) |
| Records (CAPA, training, complaints, DHF) | collections + the one audit stream | none |
| The controlled PDF itself | content-addressed `attachment` | none |
| Change control | PR-diff classifier + proposal sink + CODEOWNERS | none |
| Approver routing | manifest capabilities + CODEOWNERS + personas (ADR-036) | none |
| Audit trail | the ONE stream + anchor ledger (ADR-037) | none — stronger than typical eQMS |
| Controlled-doc rendering | descriptor render block + a **Typst→PDF** target | new render target (extension point exists) |
| **Approval e-signature manifestation** | — | rungs E1–E4 in ADR-036 (PAdES/pyHanko = E3) |
| **Time-triggered workflow** (periodic review, effective dates, training expiry) | cron Actions exist (ADR-034 drift audit) but no domain scheduler | genuinely new |
| **Validation burden on the tool itself** | — | a consequence, not a feature: a tool holding the DHF is a higher-criticality validated system (CSV/CSA, Part 11) |

So "expand into an HQ" ≈ recognize the platform + instantiate a second
**preset family** — not a rebuild. The real costs are the three gaps
plus focus.

## Finding 2 — deployment/business model (the part that makes it viable)

**Open-source engine + validation package + sovereign private data
repos.** The tool develops publicly; companies deploy against their own
repos; the vendor never custodies records. "Certified" decomposes into
*each adopter validates for intended use* (ISO 13485 §4.1.6, 21 CFR
820.70(i), Annex 11, GAMP 5) — and the open repo's existing discipline
**is** the validation package in waiting: ADRs + obligations.toml =
design I/O traceability; conformance/parity suites = test evidence;
SOUP inventory = dependency dossier; signed repro releases = known
states; guardrails+gates = tool change control. The dossier becomes an
*export* of the repo's structure. Open source strengthens (not
weakens) the audit story: the validator's logic is inspectable, and the
SSoT gate means the adopter attests *by hash* which engine ran.

Demands in return: release engineering is the product surface
(pin → upgrade = revalidation delta, ADR-038); engine/preset boundary
stays code-owned (ADR-035 guardrail); bootstrap grows into the
adopter's IQ/OQ checklist (ADR-038 §5); per-instance GDPR posture
(operator identity stays in-repo; public-log rungs opt-in).

## Finding 3 — identity & signing (decided → ADR-036; summary of *why*)

The accountable act in this architecture is **acceptance into truth**
(the host-witnessed merge), not local authorship. Hence: personas as a
collection, host-resolved accountability, signed commits demoted to
authorship+integrity, no operator key infrastructure by default, and an
explicit escalation ladder (portable in-record signature → per-act
WebAuthn presence → PAdES manifestation → QES) with concrete triggers.
Key insights worth preserving verbatim:

- *Repo allowance ≠ audit identity* — host permission authorizes
  accounts; the persona registry identifies accountable persons; the
  gate joins them (FK + CODEOWNERS ⊆ personas).
- *Possession ≠ presence* — a resting key signs without a human;
  only per-act UV (biometric/PIN) proves presence; Sigstore's actual
  win is "no reusable key + every use logged", a *different* threat.
- *The anchor is continuous, never a "certified genesis"* — trust
  shrinks to the window since the last anchor (ADR-037).

## Finding 4 — the remaining genuinely-new work for a QMS preset

1. **Typst→PDF render target** — deterministic (pin fonts, strip
   timestamps; repro-discipline applies), templates versioned in-repo,
   controlled headers/footers/"uncontrolled when printed". Wanted for
   *parts* anyway (CoC, signed audit exports), so it can land before
   any QMS commitment.
2. **Scheduler** — periodic review/effective-date/expiry events; likely
   a cron workflow emitting proposals (so even time-triggered changes
   flow through the PR gate). Needs its own design pass.
3. **E3 signature manifestation** (pyHanko/PAdES on rendered PDFs) —
   trigger-gated per ADR-036.

## Recommendation (unchanged from the session)

1. Name the platform/instance split explicitly (done implicitly in
   ADR-035; make it loud in docs when the second preset family lands).
2. Keep `parts` as the proving instance in production first.
3. Build Typst render + (when triggered) E-sig rungs as **generic
   capabilities** justified by parts use-cases.
4. **QMS preset family is trigger-gated**: build when SOPs actually
   need managing in-system, or a customer/auditor asks — not
   speculatively. Same presets-and-triggers discipline as everywhere
   else.

## Triggers that would graduate this exploration to an ADR

- A real need to manage this project's own SOPs/design docs in-engine
  (note: `decisions/` + obligations.toml already behave like a
  proto-QMS for design controls — promoting them = dogfooding).
- A customer/partner asks for the QMS preset.
- The Typst target lands and a controlled-document workflow follows it
  naturally.

## References

- ADR-033/034/035 (the substrate) · ADR-036/037/038 (the decided slice)
- GAMP 5; ISO 13485 §4.1.6; 21 CFR Part 11 / 820.70(i); Annex 11
- Typst; pyHanko (PAdES); EU DSS; OpenRegulatory (kindred, templates-side)
