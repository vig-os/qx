# M-B (ADR-035 data model) — implementation sequencing

Status: execution map (exploration, not an ADR). Produced after the
contract/validator-layer M-B pieces landed, to sequence the larger,
interconnected remainder into small mergeable increments — the same
scoping that made the M-A contract engine drivable.

## Already done & merged (contract/validator layer)
- `declared-relations` — typed `Relation{name,target,kind,acyclic,void_policy,backlink}` in `crates/contract` (#235).
- `component-graph-integrity` (acyclicity) — `validate_collection_graph` in `crates/validators`, wired into `qx check` (#236).
- `attachments-content-addressed` (shape) — `check_attachment` enforces `{ref: sha256, name, desc?}` (#237).

Each kept its obligation **pending** (honest): the remaining piece of
each (runtime backlinks · void-policy enforcement · blob hash-match)
rides a later increment below.

## The true foundation
**`load-contract-into-app` (collections-metamodel), not `typed-ids`/`entity-store`.**
`crates/app/src/preset.rs::parts_descriptor()` + `engine.rs::known_collection()`
(hard-allows only `"parts"`) are the bottleneck: every app/CLI obligation
eventually has to read its declared shape from the contract instead of
the preset. `typed-ids` and `entity-store` are downstream of it.

## Increments that can land NOW behind the existing gate (no storage cutover)
- **typed-ids scheme-format validator** — pure addition to `validate_record`.
  ⚠️ CAVEAT (found on contact): the conformance corpus + many test
  fixtures use synthetic ids (`PART0001`, `COMP0001`) that are NOT valid
  nano14 (alphabet excludes `0`/`1`). Adding the check breaks them — so
  this increment MUST also migrate every fixture id to a valid nano14, OR
  gate the check to mintable schemes only. Not the "pure addition" it
  first appears.
- **properties-promotion** — `Request::Promote{collection,from_key,to_field}` op + handler (additive; multi-record proposal).
- **component-graph void_policy** — enforce `Block`/`Warn`/`Cascade` in `engine.rs::transition()` (validator already exists).
- **timestamp-trust** — additive `time_source` field on `AuditEntry` + a `MintClock` with a skew check.

## Sequenced increments (each ≈ one PR behind `cargo test` + conformance + `qx check`)
1. **S1 typed-ids format validator** — dispatch on `IdScheme.scheme` (nano14 = `qx_domain::PART_ID_ALPHABET`+len14; sha256 = 64 hex; udi/gs1 = accept). *Must* handle the fixture-id churn (above).
2. **S2 typed-ids resolve** — contract-driven `resolve` (bare = default scheme; `scheme:value` = matching collection). Needs S5's contract-in-context.
3. **S3 properties-promotion** — `Request::Promote` op + handler.
4. **S4 void_policy enforcement** — in `transition()`, walk relations targeting the collection; Block/Warn/Cascade.
5. **S5 load contract into AppContext** (THE foundation) — drive `known_collection`/`Describe`/EDITABLE_KEYS from the contract; keep `parts_descriptor()` as the floor asserted by a compatibility check (rides ADR-040 spike #216).
6. **S6 unified-change-vocabulary, additive** — add `Action::Op{collection,op_kind,target,payload}` + `OpKind` alongside the parts-shaped variants (no wire break).
7. **S7 unified-change-vocabulary, emit sites** — migrate `engine.rs` mint/bind/void to `Action::Op` via a generic `op_audit_entry`.
8. **S8 print-fold-audit-spine** — print emits `OpKind::Print` audit entries; one-shot importer for legacy `print_log.jsonl`.
9. **S9 batch-deprecated** — `qx migrate retire-batch` synthesises a mint audit entry per distinct legacy `batch` (idempotent `request_id = sha256("batch:"+label)`), drops the column; remove from `REGISTRY_HEADER`, mint, PR templates.
10. **S10 lifecycle-timestamps** — cross-record validator: `transitioned_at[s]` == the `Transition` audit ts.
11. **S11 jsonl-storage primary** — flip default adapter; rename `Part` fields (`minted_at`→`created_at`, drop `bound_at`/`batch`); drop CSV-era `REGISTRY_HEADER`/`registry_sort_key`. Largest non-additive change.
12. **S12 attachments blob verify** — `attachments/<hex>` store + cross-record `validate_attachment_blobs` (ref-exists + hash-matches).
13. **S13 timestamp-trust** — `MintClock` + additive `time_source`; pr-check plausibility step.
14. **S14 export-never-committed** — implement `Export{csv}` + `qx export`; gate rejects committed `*.csv` beside `*.jsonl`.
15. **S15 declared-relations remainder** — backlink as a render-time `List` view; shells read display labels from `Describe`.

Independent of the chain (any time): S3, S4, S12, S13.

## Open design decisions (defaults recommended)
- **Typed-id representation** — keep `String` canonical on the wire and in records; NO domain `Id{scheme,value}` enum yet (a `(scheme,value)` parse-on-read view only). Blocking for S1/S2; 5-min call. The enum drags every storage adapter — defer.
- **`batch`→mint migration** — one synth audit entry per distinct `batch`, back-dated to `min(minted_at)`, idempotent synthetic `request_id`. Mint-events view key = `request_id`. Blocking for S9.
- **JSONL-primary cutover timing** — AFTER S10, not during S5–S7 (keep both adapters working through the vocab refactor so regressions don't bisect across two changes). Blocking for S11.
- **Unified vocab shape** — `Action::Op{collection,op_kind,target:TypedId,payload}`, additive first, old variants `#[deprecated]` shims. `TypedId` serialized as the canonical `String`, never `{scheme,value}`. Blocking for S6.
- **Kind tree (schema-bearing kinds)** — DEFER to M-B.2; `enum`+`reference` already covers non-schema-bearing vocab; schema-bearing needs the `$ref` resolver (M-A.2).

## Biggest risk
**S6+S7+S11 — the unified-change-vocabulary migration.** Breadth, not
depth: `Action`/`ActionKind`/`TargetRef` are imported in ~9 crates, and
the `transport_github_pr` PR-template parser round-trips `batch:` as a
first-class field — dropping it without a back-compat shim breaks
in-flight PRs. Mitigate by landing `Action::Op` with NO consumers first,
then migrating one crate per PR. Secondary risk: `qx check` is accreting
work (base-ref contract diff + audit cross-check + attachment blobs) —
watch the per-contributor CI latency.
