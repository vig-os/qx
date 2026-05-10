# ADR-016 — PR-diff-based policy enforcement for registry changes

- Status: Proposed
- Date: 2026-05-08
- Component / area: registry governance (`registry.csv`, future
  `print_log.csv` / related files), CI validators, PR review policy
- Reviewers: _(pending)_
- Related: ADR-017 (Rust core), ADR-018 (Storage port), ADR-020 (Identity port), ADR-023 (Threat model)

## Context

The registry already uses PRs plus CI validation as its write path
(ADR-013). Today that validation is mostly **schema** and **state
transition** oriented: header equality, sort stability, uniqueness,
and allowed `status` transitions.

That is necessary but not sufficient once the workflow includes more
operators and more action types. Some changes are materially riskier
than others:

- binding an `unbound` row
- editing metadata like `location`
- voiding an ID
- deleting rows
- changing headers / schema
- bulk changes touching many rows

The key requirement is that policy must be **frontend-independent**.
The FE may help the user compose a change, but it is not the trusted
source of what happened. The trusted artifact is the **git diff in the
PR** (`base` vs `head`).

## Alternatives considered

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| FE-declared action type (`bind`, `delete`, `void`, …) | Easy to implement in the app | Untrusted; CLI edits and hand-edited CSV bypass it; drift between clients | Rejected |
| `CODEOWNERS` only | Native GitHub feature, simple | Path-based only; cannot distinguish safe binds from destructive deletes within the same file | Rejected |
| Manual reviewer discipline only | Zero implementation work | Inconsistent, not machine-enforced, brittle at scale | Rejected |
| CI derives semantic change classes from the PR diff and enforces policy | FE-independent, works for CLI/manual/FE edits alike, reviewable and auditable | More validator logic; policy has to be codified explicitly | **Chosen** |

## Decision

Repository policy for registry mutations is enforced from the **PR
diff**, not from client-side intent.

CI will:

1. Load the base and head versions of the governed file(s).
2. Compute semantic change classes from the diff.
3. Enforce policy based on those classes.

The FE, CLI, and manual file edits are all treated as equivalent
producers of candidate diffs. None of them are policy authorities.

### Immediate feedback vs final authority

The same policy engine should also run in the FE as a **preflight**
check so operators get immediate feedback before a branch/commit/PR is
opened.

Authority levels are intentionally split:

- **FE preflight** — advisory, immediate, user-facing
- **GitHub CI** — final, merge-blocking, authoritative

The FE must not invent its own rule set. It runs the same semantic
diff classifier and policy evaluation logic against the locally
proposed change. CI then re-runs that same logic against the actual
PR diff.

This avoids the failure mode of “submit first, wait for CI to reject
10 minutes later” while still keeping final authority in GitHub.

### Semantic change classes

At minimum, the classifier distinguishes:

- `row_add`
- `row_delete`
- `row_void`
- `row_bind`
- `row_edit`
- `header_change`
- `bulk_change`

The exact internal representation is an implementation detail; the
classes above are the policy vocabulary.

### Policy model

Baseline rules:

- schema/header changes are blocked unless explicitly allowed
- row deletions are treated as destructive
- `* -> void` transitions are treated as destructive
- normal bind/edit flows remain allowed if validator checks pass

The specific branch protection / required-review settings stay in
GitHub repo configuration, but CI becomes the **semantic gate** that
classifies a PR and decides whether it is eligible for merge under the
current policy.

### Repository boundary

This ADR does **not** require a frontend, GitHub App, or browser-side
save flow to be valid as a governance rule. It applies equally to:

- FE-originated PRs
- CLI-generated commits
- manual CSV edits in a branch

## Rationale

This keeps enforcement aligned with the only stable cross-client
artifact: the git diff itself.

`CODEOWNERS` answers “who reviews this file?” but not “what kind of
change happened inside this file?” Diff-aware CI fills that gap.

The approach also composes with the permanence and auditability goals
of ADR-012/013:

- the diff is reviewable
- the policy is reproducible
- the enforcement is centralized
- clients stay thin and non-authoritative

## Consequences

- validators grow a **semantic diff classifier** instead of stopping at
  structural validity
- destructive operations like row deletion or voiding can be surfaced
  explicitly in CI output
- the FE can provide immediate “allowed / warning / blocked” feedback
  by running the same engine locally, without becoming the authority
- PR review policy becomes more precise without coupling it to any one
  client implementation
- future FE “save to PR” flows can stay simple because they do not need
  to be the enforcement point

## Open questions / supersession triggers

- whether destructive classes should hard-fail outright or require a
  second explicit approval signal
- whether approval signals should be labels, PR body markers, review
  states, or a combination
- whether bulk thresholds should be absolute row counts or percentage
  based
- whether `print_log.csv` and future related files use the same policy
  engine or file-specific variants
- ~~whether FE preflight should run via Pyodide-loaded Python validators
  (SSOT) or a TypeScript port (smaller runtime, higher drift risk)~~
  **Resolved 2026-05-10 by ADR-017**: validators are Rust, compiled
  natively for CI and to WebAssembly for the FE. One source of truth,
  no port, no Pyodide cold-load cost. The semantic-diff classifier
  required by this ADR lives in the Rust `validators` crate and is
  exposed to both surfaces through the same trait.

## References

- [ADR-012 — Part identification](ADR-012-part-identification.md)
- [ADR-013 — Parts registry web app](ADR-013-parts-registry-web-app.md)
- [ADR-014 — Web app architecture](ADR-014-web-app-architecture.md)
- [ADR-015 — Print event log](ADR-015-print-event-log.md)
