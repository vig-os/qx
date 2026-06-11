# ADR-037 — Audit-trail integrity: checkpoints, merge-sync, tool provenance, anchor ledger

- Status: Proposed
- Date: 2026-06-11
- Component / area: the integrity mechanics of the ONE audit stream
  (ADR-022/035) and the external evidence that the repo's history is
  whole. Refines ADR-022 (chain mechanics), ADR-024/025 (the anchor is
  the repro/integrity discipline applied to the data repo), ADR-033
  (anatomy gains the anchor workflows).
- Reviewers: Lars Gerchow (required for Accepted)
- Related: ADR-016 (gate), ADR-022 (audit trail), ADR-024 (crypto
  baseline), ADR-034 (host-enforced authz + drift audit), ADR-035 (one
  stream), ADR-036 (audit identity — sibling)

## Context

A review of "what git/GitHub give natively vs what we were about to
build" found one redundancy, one conflict, and one gap:

- **Redundant:** git is a Merkle DAG — the head hash transitively
  covers every prior state, so within the repo, a per-entry hash chain
  proves nothing the DAG + an append-only rule don't already prove.
  Against the one adversary git can't handle (a privileged admin
  rewriting history), an unsigned chain *also* fails — the rewriter
  recomputes it. Both stand or fall on an external anchor.
- **Conflict:** per-entry `prev_chain_hash` **serializes all writes**.
  Two concurrent proposal PRs append entries pointing at the same
  predecessor; whichever merges second carries a broken link. A linear
  chain is the wrong structure for a concurrent-PR write path (and if
  entries are ever operator-signed, links can't be recomputed without
  re-signing).
- **Gap:** nothing yet records *when* a state existed, in a place the
  repo's own admin cannot rewrite — commit dates are forgeable and the
  in-repo workflow can be disabled by the same admin it would catch.

Plus two evidence-durability facts: GitHub's org audit-log API has
~90-day retention (review records persist with PRs but are
host-custodial), and GitHub **immutable releases** lock
{tag → commit SHA, assets} against later modification — even by
admins — *outside* the git object store, inheriting the repo's privacy.

## Alternatives considered

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Per-entry hash chain (status quo header field) | stream self-verifies standalone | redundant on-host; defeated by the same adversary as git; serializes concurrent PRs | Rejected — replaced by checkpoints |
| Public Rekor as the anchor | append-only, public proofs, free | leaks repo identity/cadence/workflow names; permanent unretractable evidence; unqualifiable supplier (no SLA/DPA) for a GxP control; GDPR if operator-level entries ever land | Escalation rung only |
| RFC-3161 TSA only | contractable, private, independent time | tokens committed in-repo can be rewritten away with history — still needs external storage; no ledger semantics | Add-on rung (independent time), not the ledger |
| **Immutable-release anchor ledger + stream checkpoints + append-only gate rule** (chosen) | native, zero infra, privacy-inheriting; survives history rewrite; ledger semantics via the ancestry rule; checkpoints restore standalone stream verifiability without serializing writers | host-custodial (mitigated by pulled bundles); repo deletion destroys (loud, answered by offline copies) | **Chosen** |

## Decision

### 1. Stream integrity: append-only rule + checkpoints (chain links retired)

- Entries carry a **`content_hash`** of their own body; they do **not**
  link to predecessors. (Existing `chain_hash` data remains valid
  history; new entries omit it — refinement of ADR-022's chain
  mechanics, same goal, different carrier.)
- The gate enforces **append-only as a diff rule**: any PR whose diff
  to `audit_log.jsonl` is not pure trailing addition is rejected.
- **Checkpoint entries** restore standalone verifiability without
  serializing proposers: written only by the serialized anchor job
  (§3), each records `{line_count, stream_digest (sha256 of the stream
  up to here), head_sha, anchor_ref}`. Continuity *between* checkpoints
  is git's; continuity *of* checkpoints is the ledger's.

### 2. Merge-sync: host witness becomes durable stream data

For every merged proposal, a sync step appends the host's witness
evidence to the stream: approver/merger (→ persona FK per ADR-036),
review identities, timestamps, PR ref, and the **derived meaning** (the
`{collection, op-kind, edge}` the merge effected). This converts
ephemeral/host-custodial evidence into repo-resident records — the
load-bearing reason the trail survives the host (ADR-038's "GH gone"
matrix). The same event carries **gate provenance**:

- `producer: {tool_version, tool_commit}` on every entry — a **claim**
  (self-reported; forensic value only, never a security argument).
- `gate: {version, artifact_sha256, env_digest, run_ref,
  attestation_ref?}` on the merge event — **evidence**: the CI step
  already verifies the artifact hash against the pin before executing
  (ADR-034 §2); it records the value it verified. The pin itself is
  CODEOWNERS-gated policy (downgrade-to-buggy-gate is the bypass).

### 3. The anchor ledger: immutable releases, push + heartbeat + bundle

The data repo's releases page is its anchor ledger:

| Trigger | Condition | Publishes |
|---|---|---|
| push to `main` | always | small immutable release: manifest `{head_sha, prev_anchor_sha, stream_digest, line_count, ts, reason: push}` |
| nightly cron | only if no push anchored that day | same manifest, `reason: heartbeat` (re-attests the head — **silence stays unambiguous**: a gap means a problem, never a quiet day) |
| monthly cron | only if head moved since last bundle | full evidence package: `git bundle` + `pr verify` report + manifest |
| on demand | pre-audit | same as monthly |

The anchor publishes **only after `pr verify` passes** — each anchor
attests a *verified* state; on failure it fails loudly and opens an
issue (a ledger gap + an issue is the alarm; a green anchor over a
broken repo would be the worst outcome).

**What the ledger proves (state plainly, for auditors):** each anchor
freezes everything behind it — once anchor *n+1* publishes, the whole
lineage through it (including the n→n+1 segment) is tamper-evident
*from that moment on*; any later rewrite breaks the **ancestry rule**
(every anchored SHA must be an ancestor of the next; the current head
must descend from the latest). It does **not** prove the interior of
the open window (prevention there = branch protection), nor capture
transiently-pushed-then-removed states, nor content legitimacy (that is
the gate + review layer). Per-push anchoring shrinks the window to
~seconds.

### 4. Custody + escalation knob

Release assets are host-custodial until pulled: an **external watcher**
(separate admin domain — other org or corporate IT) downloads each
bundle release to offline storage; that single scheduled fetch makes
the scheme host-independent and answers repo-deletion (loud
destruction, not quiet tampering). Per-registry manifest knob:

```
anchor = releases (default) | releases+tsa | +witness-org | +rekor-public
```

— escalations for independent time, separate-domain ledger, and public
verifiability respectively, chosen by confidentiality class and threat
model (Rekor-public never for operator-identifying payloads in EU
deployments — ADR-036 GDPR note).

### 5. Verification: `pr verify [--anchors]`

Offline, from a clone: contract validity; FK/graph integrity; stream
append-only + checkpoint digests; every operator → active-at-act
persona; CODEOWNERS ⊆ personas. With `--anchors` (API access): release
**immutability actually enabled** (a release on a repo without the
setting is mutable — check, don't assume), ancestry rule across the
ledger, head descends from latest anchor, monotonic timestamps,
expected publisher identity. Asset attestations on private repos are
plan-dependent (Enterprise-tier) — verified per deployment, not
designed in as guaranteed.

## Rationale

Each layer keeps only the job it does best: **git DAG** = integrity and
order; **gate diff rule** = append-only; **merge-sync** = durable
witness; **checkpoints** = standalone stream verifiability without
serializing writers; **ledger + ancestry rule** = rewrite detection
with a bounded, per-push-small window; **pulled bundles** = host
independence and destruction insurance. The previous chain design
duplicated git inside the window where both are equally defenseless,
while breaking concurrent proposals — the review caught a real bug
before it shipped.

## Consequences

- `chain_hash` retired from new entries (historical values stay);
  audit-spine validators updated (append-only, checkpoint digests, FK
  to personas).
- New data-repo workflows: `anchor.yml`, `bundle.yml` (templates seeded
  by bootstrap — ADR-038); releases page becomes the ledger (repo
  setting "immutable releases" is part of a correctly-deployed
  registry, joining ADR-034 §6's teeth).
- `pr verify` and `pr verify --anchors` join the CLI surface (ADR-030
  op catalog; CI-deep per ADR-016).
- The external watcher is a deployment responsibility documented in the
  qualification checklist (validation package).
- Anchor manifests + checkpoints give the auditor a freeze-line
  timeline: "trust shrinks to the window since the last anchor."

## Open questions / supersession triggers

- Checkpoint cadence (per anchor vs per merge) — start per-anchor;
  revisit if stream-export consumers need finer standalone granularity.
- Whether the monthly bundle also carries the exported build closure
  (ADR-038 archival) or that stays cold-storage-only.
- GitLab/Gitea ledger equivalents when a non-GitHub T1 host lands
  (ADR-034 tiers) — immutable releases are GitHub-specific; the ledger
  abstraction (append-only tag→SHA witness + ancestry rule) is not.

## References

- ADR-022 — Observability/audit (the stream this hardens)
- ADR-024/025 — Crypto baseline / distribution integrity
- ADR-034 — Host-enforced authz; pinned-artifact gate; drift audit
- ADR-036 — Audit identity (who; this ADR is the what/when)
- GitHub immutable releases; RFC-3161; Sigstore/Rekor
