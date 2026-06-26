# Roadmap to 71/71 — the remaining obligations, categorized

Status: execution map (exploration, not an ADR). As of 2026-06-27,
**31/71 obligations satisfied**. The generic contract-driven entity engine
(read+write) is complete and the **parts→JSONL cutover (A1 below) is
DONE** — `jsonl-storage` + `export-never-committed` flipped (JSONL primary
via `is_jsonl_native`/adapter-detection; the generic Export op + a
committed-`*.csv` gate). `lifecycle-timestamps` (engine-materialized
`created_at`/`transitioned_at`) and `unified-change-vocabulary`
(`Action::RecordWrite` classification — generic writes no longer bypass
policy) are ADVANCED; their flips ride the audit-spine refactor below.
This maps every remaining obligation to what it actually needs, and flags
which **cannot** reach `satisfied` from this repo alone.

Sibling sequencing docs: `m-b-data-model-sequencing.md` (the M-B data
model) and `write-refactor-sequencing.md` (the CSV→JSONL cutover).

## A. Coordinated code refactors (multi-PR, single-session-infeasible)
1. **Parts→JSONL cutover** — ✅ **DONE** (2026-06-27). Delivered via the
   additive `record_writes` + `is_jsonl_native` path (NOT the 34-site
   `PartId` migration): qx init seeds `collections/*.jsonl`; the CLI
   detects+selects the JSONL adapter (legacy `registry.csv` → CSV); parts
   route through the generic path on a JSONL-native store; the generic
   Export op emits CSV on demand + `qx check` rejects committed
   `collections/*.csv`. FLIPPED `jsonl-storage` + `export-never-committed`.
   STILL OPEN (ride the audit-spine, A3/§unified-vocab): the parts `Row*`
   actions → `{collection,op-kind}` generalization + the PR-template
   round-trip (W5/W6) flips `unified-change-vocabulary` +
   `lifecycle-timestamps` (the cross-check) + `print-fold-audit-spine`.
2. **Kind tree + `$ref`/`$defs` resolver** — add a `defs` map to
   `Contract`, resolve `ObjectSchema::Ref` in `check_object`
   (`record.rs:576` is a stub today); per-kind field schemas with
   inheritance, validation dispatching on `kind` (kinds live in a `types`
   collection per ADR-035 §5). UNBLOCKS: `tiered-data-model`,
   `entity-store` (kind-tree term), `core-plus-custom-schema`.
3. **Audit hash-chain** — `AuditEntry.prev_hash`; the append path
   (`crates/observability`) computes it; `qx check` verifies the chain.
   The append-only DIFF rule already landed this session
   (`audit_append_only_violation`). UNBLOCKS the chain half of
   `audit-append-only-gate-rule`.
4. **Scan processor** (`scan-processor-dry`) + **decode surfacing**
   (`decode-image-surfaced`, `zxing-wasm-dropped`) — ADR-032 §2; the
   FrameSource→decode→resolve→accumulator→Sink module + replay fixtures.

## B. CI infrastructure (workflows, not just library code)
- `canary-pipeline` (needs the provisioner App — see D), the audit
  **checkpoint anchor job** (periodic head-hash, signed), `protection-
  drift-selfaudit` seeding, `merge-sync-witness`, `reproducible-signed-
  releases`, `distribution-integrity`.

## C. Feature-builds (code, each its own gated PR)
- `properties-promotion` (a contract-edit `Promote` op + shape-check),
  `batch-deprecated` (remove `batch` from mint/label/web + synth-mint
  migration), `optimistic-mint-preflight` (preflight read + optimistic
  mint), `structured-print-request` (id-scheme groupings — the rest
  landed), `timestamp-trust` (`MintClock` + skew + `time_source`),
  `print-fold-audit-spine`, `print-contracts`, `crypto-reopen-triggers-
  watched`, `px-true-qr-render` extras (`--size-mode snap`, `--align`),
  `pr-verify-offline` (`pr verify` subcommand), `gate-vendored` (after the
  musl static build), `upgrade-succession`, `per-op-floors`,
  `spoke-feature-parity`, `coverage-joiner`, `port-conformance-wired`,
  `declared-relations` remainder (backlink derivation), `properties-
  promotion`, `attachments` prose-render (FE).

## D. Structurally blocked OUTSIDE this repo (cannot flip by code here)
These need resources to be provisioned first — list them so they can be:
- **A first release tag** → `pr-diff-policy-gate`, `gate-vendored`,
  `reproducible-signed-releases`, `distribution-integrity`,
  `upgrade-succession`.
- **The qx-provisioner GitHub App** (Administration rw, per-run tokens) →
  `canary-pipeline`, `host-enforced-authz`, `protection-drift-selfaudit`.
- **A deployed/seeded data repo** → `registry-manifest`,
  `capability-grain`, `operator-workspace`, `personas-collection`
  (personas preset + CODEOWNERS cross-check), `registry-self-describing`
  consumers.

## E. The FE epic (M-D)
The web port: `contract-ssot-validation`'s FE-Vitest arm (corpus through
the shipped wasm), declared-relations display labels, the whole
contract-driven form/list UI. A self-contained epic.

## Recommended order
Cutover (A1) → kind tree (A2) → audit chain+checkpoint (A3+B) → the
feature-builds (C) → M-D FE → then the external-gated items (D) as the
release tag / App / data repo are provisioned. The generic engine this
session built is the substrate all of A–C now stand on.
