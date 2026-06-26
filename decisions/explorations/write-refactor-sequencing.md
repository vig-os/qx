# Generic write refactor — sequencing (the CSV→JSONL cutover)

Status: execution map (exploration, not an ADR). The entity-store READ
engine is complete (Describe/List/Count/Resolve serve any declared
collection). This scopes the WRITE half — the plan's largest, highest-
breadth change — into mergeable PRs, each with verification built in.

## Why this is a coordinated refactor (verified at the type level)
The write path is parts-CSV-coupled in the TYPES, not just by convention:
- `crates/domain/src/lib.rs`: `Diff { adds/deletes/edits/header_changes }`;
  `DiffRow.id: Option<PartId>` and `DiffEdit.id: PartId` — the diff can
  only name parts ids, and carries no `collection`.
- `crates/transport_github_pr/src/lib.rs`: the sink renders a `Diff` to
  CSV files — `TargetFile::Registry → "registry.csv"` (+ print_log.csv,
  audit_log.csv); `apply_diff_to_file` (line ~764) writes CSV rows.
- `crates/app/src/engine.rs`: `submit_edit_diff` builds `Diff{edits:[
  DiffEdit{id: target.id (PartId), before, after}]}` → `Proposal{diff,…}`;
  `resolve_part`/`part_field_map` are Part-typed. Mutations are parts-only
  (guarded by `known_collection`, NOT the generic `served_collection`).

So a generic write must migrate the `Diff` off `PartId`, make it
collection-aware, teach the sink to render `collections/<name>.jsonl`, and
genericize the mutation ops. There is NO additive sub-slice that delivers
generic write alone — the proposal machinery IS parts-CSV.

## Sequenced PRs (each behind `cargo test` + `qx check`)
1. **W1 — thread `collection` through the diff** (additive). Add
   `collection: String` (serde default `"parts"`) to `Diff` (or per-row).
   Every existing construction site defaults to `"parts"`; behavior-
   neutral; suite verifies. Foundation for routing.
2. **W2 — `PartId` → generic `String` id in `Diff`/`DiffRow`/`DiffEdit`**.
   Mechanical type migration (like the nano14 fixture migration: verify
   behavior-neutrality with the full suite BEFORE relying on it). Adjust
   callers that use `PartId` methods on diff ids. Unblocks non-parts ids.
3. **W3 — sink JSONL rendering**. Add a `TargetFile::Collection(name)` →
   `collections/<name>.jsonl`; `apply_diff_to_file` renders a JSONL line
   (one-doc-per-line, reuse the jsonl adapter's `write_jsonl`) when the
   diff's `collection != "parts"`. Parts stays CSV until W6. Test: a
   generic-collection diff produces the right JSONL file change.
4. **W4 — genericize the mutations**. `edit`/`create`/`transition` for a
   non-parts declared collection: use the generic `resolve` (done) + a
   generic field map (from the record) + build a `Diff{collection}`; gate
   on `served_collection` for these (writes for declared collections).
   Parts keeps the Part-typed path. Test via the MemSink: a generic edit
   submits a proposal with the right collection + before/after.
5. **W5 — PR-template parser genericization** (`transport_github_pr`
   line ~530-686): round-trip a JSONL-collection change in the PR body,
   not just CSV rows. The breadth risk — do it last, with golden tests.
6. **W6 — parts onto JSONL primary** (the cutover): flip parts writes from
   `registry.csv` to `collections/parts.jsonl`; drop `registry.csv`
   rendering + `REGISTRY_HEADER`. THEN flip `collections-metamodel` (ops
   parameterized by collection) + `typed-ids` (scheme-parameterized
   minting) + `properties-promotion` to satisfied.

## Open design decisions
- **Generic id type**: keep `String` in the `Diff` (W2) — do NOT introduce
  a domain `Id` enum; the validator already enforces format per scheme.
- **Minting per scheme** (W4 create): `nano14` mints via `mint_part_id`'s
  generator; `sha256`/`udi`/`gs1` are imported (create supplies the id, no
  mint). Dispatch on `IdScheme.mintable`.
- **Cutover timing**: parts-onto-JSONL (W6) LAST, so the vocabulary/sink
  refactor lands while both formats still work (no double-bisect).
- **Audit/print logs**: `audit_log.csv`/`print_log.csv` → JSONL rides this
  too (ties to `print-fold-audit-spine`, `unified-change-vocabulary`).

## Biggest risk
W2+W5 breadth: `PartId` and the CSV row shape are referenced across
`domain`, `app`, `transport_github_pr`, `transport_table`, `storage_csv_git`,
`storage_jsonl_git`, `port_tests`. Migrate one crate per PR; the PR-template
parser (W5) is the single most likely ballooning point — it round-trips the
registry-CSV columns today and silently dropping one breaks in-flight PRs.
Keep `registry.csv` working until W6 so the gate stays green throughout.
