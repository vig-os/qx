# ADR-039 — Contract engine: canonical form, SSOT validation, effective-dated versioning

- Status: Proposed
- Date: 2026-06-12
- Component / area: the one canonical `contract.json` form (refines
  ADR-033 §3's scalar set; ADR-035 §0's collection descriptor), the
  validation architecture that makes one engine drive every consumer
  (FE form-gen + preflight, core record-validation, the `pr check`
  gate), and the **effective-dated versioning** model that makes a
  registry self-describing in a way an auditor can defend. Realizes the
  pending obligations `registry-self-describing` (ADR-033 §2) and
  `core-plus-custom-schema` (ADR-033 §3).
- Reviewers: Lars Gerchow (required for Accepted)
- Related: ADR-012 (id scheme floor), ADR-016 (PR-diff as the review
  surface; CI authoritative / FE advisory), ADR-018 (storage port —
  goes collection-generic), ADR-020 (authz: `on_unknown`/elevation),
  ADR-022/024 (audit + signature shapes the change-control header
  reuses), ADR-033 (self-describing anatomy + scalar set — **this ADR
  resolves the scalar set and adds `reference`**), ADR-034 (manifest /
  federated enforcement), ADR-035 (collections metamodel + three-tier
  records + typed ids)
- Spike / reviews: issue #204 (4 fresh-context expert reviews:
  schema/type-system, eQMS-compliance, Rust/wasm-SSOT, frontend-DX);
  parent eQMS spine spike #189

## Context

A registry is meant to be a **self-describing** NDJSON entity store: it
carries its own pinned, versioned contract so it neither silently drifts
nor breaks when the tool moves (ADR-033 §2). Today it is none of that —
four partially-overlapping "contract" shapes exist with **three
different scalar-type vocabularies**, the meta-schema is stale against
its own instance, and **nothing in the core loads the contract** (`pr
describe` returns a hand-built descriptor from `crates/app/src/preset.rs`;
the storage port is `Part`-typed; the lifecycle is a hardcoded match).
The contract file is vestigial.

The four shapes (issue #204 §"divergence surface"):

1. `schema/registry-contract.json` (FE-facing, live) — types
   `string|dropdown|yes-no|date|number|json`, a `typeFields` map.
2. `schema/contract.schema.json` (meta-schema) — enum
   `[string,dropdown,yes-no,date,number]`; **missing `json` and
   `typeFields`** → it does not validate the contract we ship.
3. `crates/app/src/preset.rs` (Rust core, hardcoded) — scalar set
   `string|enum|integer|number|date|bool|attachment`, a `lifecycle`,
   an `id` block; no validation/options.
4. ADR-033 §3 + ADR-035 §0 target (accepted, unbuilt) — collection-
   generic descriptor, the §3 scalar set, three-tier records.

This ADR collapses the four into one, fixes where validation runs, and —
the part that needs ratification — fixes **how a record is validated
against the contract version in force when it was written**, not today's.

## Decision

### 1. One canonical contract form — type ≠ widget

The contract declares **types** (the data shape a validator reasons
about); a separate per-field `render` block declares the **widget** (the
control a shell draws). The FE's `dropdown`/`yes-no`/`json` were widget
names masquerading as types — a Rust validator and a TS form generator
can never agree on what "dropdown" *means*. They collapse:

- `dropdown` → an `enum` (or `reference`) **type** + `render: dropdown`
- `yes-no` → a `bool` **type** + `render: toggle`
- `json` → **not a field type** (see §2 — structured → `object(schema)`;
  additional/unstructured → the tier-3 `properties` bag)

A field descriptor is therefore:

```
field = {
  key, type,                       # the data shape (§2)
  label,                           # display name (descriptor-owned, ADR-035 §1a)
  required?, meaningful_from?,     # presence flags
  required_to_enter?,              # transition gate (§6) — distinct from meaningful_from
  render?,                         # { widget, group?, order?, suggest_from? } — chrome only
  on_unknown?,                     # enum/reference policy: create | warn | reject (§2/§5)
  …type-specific facets            # options, pattern, min/max, precision/scale, unit, ref-target
}
```

### 2. The canonical scalar set

Resolves ADR-033 §3 (which fixed `string|enum|integer|number|date|bool|
attachment`) by **adding `reference`, `timestamp`, `decimal`, and
`object`**, and lifting `unit` onto numerics:

| Type | Facets | Notes |
|---|---|---|
| `string` | `pattern?`, `maxLength?` | |
| `enum` | `values[]`, `closed: true\|warn\|false` | `closed` is the old `on_unknown` reframed as a *validation* policy that survives the type/widget move |
| `integer` | `min?`, `max?`, `unit?` | |
| `number` | `min?`, `max?`, `unit?` | IEEE double; for regulated quantities prefer `decimal` |
| `decimal` | `precision`, `scale`, `min?`, `max?`, `unit?` | exact — for torque/tolerance/measurement specs (lossy `f64` is an audit smell) |
| `date` | | calendar day (ISO-8601 date) |
| `timestamp` | | instant, RFC3339, tz-aware — **distinct from `date`** (audit-trail correctness: "manufactured on" ≠ "record written at") |
| `bool` | | |
| `reference` | `collection`, `key?`, `display?`, `on_unknown: create\|warn\|reject` | **first-class FK to another collection** — typed-id `scheme:value` (ADR-035 §0); the gate enforces referential integrity at PR time. Without it cross-collection links degrade to `string` and dangling refs go uncaught (the entire point of the gate). |
| `attachment` | `constraint?` (e.g. `pdf\|png`, `md`) | value `{ref: sha256:…, name, desc?}` (ADR-035 §4) |
| `object` | `schema` (a nested field list or `$ref`) | a **declared, structured** value with a known key. `schema` is **required** — an `object` without a schema would be a freeform-`json` backdoor that smuggles untyped data past validation. |

**The `json` resolution (issue #204, user-ratified):** there is **no
freeform `json` field type**. The two things it conflated are kept
separate and each has a typed home:

- **Structured additional data with a known key** → `object(schema)` —
  declared and validated.
- **Genuinely open / not-yet-schematized additional data** → the
  **tier-3 `properties` bag** (ADR-035 §1 tier-3): shape-checked only,
  from which regulated core fields are *forbidden* (§5), with a
  promotion path to tier-2. This is the escape hatch so capture never
  blocks on schema — *not* a field type.

**Forward-compat policy** at the contract root: `unknown_type_policy:
reject` (always — a reader that silently treats an unknown scalar as
`string` passes garbage through the gate) and `unknown_field_policy:
ignore|warn|reject` for unknown *props on a known type*. Minor versions
are **additive-only** (§6).

### 3. SSOT validation — one Rust engine, compiled to wasm

The validator is written **once in Rust** and the FE calls *that*
(compiled to wasm), rather than reimplementing rules in TypeScript — two
validators drift, and the failure mode ("the PR check disagrees with the
form") is unbounded. The contract stays **declarative**; Rust interprets
it; the FE reads it directly for *form generation* (structural, not a
verdict) but routes every *"is this record valid?"* question through
wasm.

Crate split, with the purity boundary drawn so the wasm build *cannot*
compile anything impure:

- **`crates/contract`** (new) — descriptor types + parser +
  `[min,max]`/effective-date compat check. Pure: `serde`/`serde_json`
  only, no `std::fs`, no `tokio`, no `git2`. Builds for
  `wasm32-unknown-unknown` with zero features. The **only** parse path
  is `Contract::from_bytes(&[u8])` + `is_compatible(engine_version,
  contract)` — CLI does `fs::read`, FE does `fetch().arrayBuffer()`,
  both hand bytes to the same function (one parser, one compat rule).
- **`crates/validators`** — depends on `contract` + `serde` only; takes
  `&Contract` + a record, returns a `Verdict`. No I/O, no time, no
  randomness. Same wasm-clean rules. Kind-dispatching
  (`core ∪ kind ∪ shape`).
- **`crates/storage*`, git, `crates/cli`, `crates/app`** — host-only;
  may depend on `validators`, **never the reverse**.
- **`crates/wasm`** — thin façade: `validate_record(contract_bytes,
  record_bytes) -> Verdict`, marshalling only.

**Enforced mechanically, not by policy:** a CI job runs `cargo build -p
part-registry-contract -p part-registry-validators --target
wasm32-unknown-unknown` every PR. Pull `git2` into `validators` and the
wasm build breaks *before* review.

**Latency split (FE):** cheap declarative checks (`required`, `pattern`,
`min`/`max`, `enum` membership) run **native in JS** from the contract
(sub-ms, no async boundary); semantic checks (uniqueness, cross-field,
reference-existence, type-schema conformance) run in **wasm on
blur/submit** — never per-keystroke. **Error-message strings come from
one place**: the Rust validator exports a message catalog the JS cheap
checks also read, so wording never drifts.

**Marshalling discipline:** records cross the wasm boundary as **UTF-8
bytes**, parsed by the same `serde_json` on both sides — never as JS
objects (which lose `u64` precision above 2⁵³ and may reorder keys,
breaking hash-stable checks).

### 4. Parity by conformance — a tested property, not a claim

"Same code via wasm ⇒ same verdict" is a claim until a test enforces it.
A **conformance corpus** of `(contract, record, expected_verdict)`
triples is checked into the repo and consumed by **three runners**:

1. native Rust (`cargo test -p part-registry-validators`),
2. wasm (`wasm-bindgen-test`, headless in CI),
3. FE Vitest loading the **actual shipped wasm artifact**.

A PR that changes validation logic must update the corpus; CI fails if
any runner disagrees. Separately, CI validates the shipped contract
**against the meta-schema** *and* a fixture instance **against the
contract** on every PR — the stale-meta-schema failure mode (#2 above)
can never recur.

### 5. Floor vs extend — the non-weakenable core

Per ADR-035 guardrail #1: the tool ships **code-owned presets**
(`parts` = the ADR-012 regulated floor: id scheme, lifecycle,
`components`, audit hooks, open bag). A registry's contract may
**extend** a preset (add typed fields, vocab collections); it may
**not weaken or redefine** core fields, lifecycle, or id scheme. Exactly
one meta level (guardrail #2): contracts declare collections; Rust types
govern contracts; no meta-meta.

The compliance review surfaced that the floor can be **de-facto
weakened** without touching core fields:

- **Forbidden-in-tier-3 list.** Core/regulated fields may not appear in
  the tier-3 `properties` bag (prevents moving a CAPA disposition into
  the ungated escape hatch).
- **Relaxation is change-controlled.** Flipping an `enum`'s `closed:
  true → warn`, or demoting a tier-2 field to tier-3, requires a
  **validation-relaxation change-control record** (§6 header) with
  rationale + dual approval — it is not a silent edit.
- **Drift report.** The gate emits a periodic report of fields living in
  tier-3 with regulated-sounding names, so de-facto demotion is visible.

### 6. Versioning + effective-dated validation — THE RATIFICATION GATE

This is the contested decision the spike exists to settle (issue #204
"the real decision point"). A `[min,max]` integer range answers *"can
today's binary read this repo?"* — a **tool-compatibility** question. It
does **not** answer the only question an auditor asks: *"under which
contract version, approved by whom, on what date, was this record
validated when it was created?"* Re-validating a 2024 record against
today's `HEAD` contract is **retroactive re-qualification of historical
evidence** — an ALCOA+ "Original/Accurate" failure that will not survive
inspection (21 CFR Part 11 §11.10(e); ISO 13485 §4.2.5).

The model (the record-shape **one-way door** — cheap now, a migration
later):

1. **Every record carries an immutable `contract_version` stamp** at
   write time (git already records *when*; the row must record *against
   what*).
2. **The gate validates each record against the version named on the
   record** — fetched from git history — **not** `HEAD`'s contract. New
   writes use `HEAD`'s (current effective) version.
3. **The contract carries a change-control header:**
   ```
   version, effective_from (UTC), supersedes,
   change_rationale, approved_by [author + approver — two DISTINCT
   identities, Part 11 §11.200], approval_commit_sha
   ```
   Bumping `version` without these fields is a gate failure.
4. **Migrations are forward-only transformations producing a new
   version** — never silent rewrites of historical rows. ("Migrate
   outside the range," as earlier framings put it, is rejected: it
   rewrites evidence.)
5. **`[min,max]` stays — as the *tool* guard only** (refuse to operate
   on a contract this binary cannot parse), explicitly **not** the
   governance mechanism. Minor versions are additive-only (§2).

And the presence-flag split the compliance review demanded:

- **`required_to_enter: <status>`** — a hard **transition gate**: the
  entity cannot advance to `<status>` unless the field is present and
  valid. (Closes the backdating gap: required quality data is captured
  *to advance*, not after.)
- **`meaningful_from: <status>`** — downstream readers may trust the
  field once the entity reaches `<status>` (the existing semantic;
  documentation, not enforcement).

A field may declare either, both, or neither.

### 7. Anatomy, bootstrap, and the typeFields seam

- The contract moves to **`.part-registry/contract.json`** in the data
  repo (ADR-033 §4 anatomy); `bootstrap` seeds it from the tool baseline
  with the change-control header's first version.
- **`typeFields` stays embedded in the contract for v1** (synchronous,
  cacheable, no "schema loading…" spinner blocking the bind form), but
  every consumer reaches per-type schemas through an async
  `loadTypeSchema(typeId)` seam — so the ADR-035 move to a `types`
  collection (schema-as-data) is a resolver swap, not a rewrite. Policy
  fixed now (survives both regimes): an **unknown type renders base
  fields + a warning banner and allows save**, never a hard error.
- The FE's silent `parseContract` plain-text fallback is **removed**: a
  malformed contract fails loudly (toast + read-only mode), not a silent
  degrade that lets an operator enter data the gate will reject in PR
  review.

## Rationale

Same governance shape as the rest of the project: **hard-gate the
deterministic** (the regulated core, the non-weakenable floor),
**federate the rest** (per-type schemas, declared fields), **explicit
auditable escape hatch** (tier-3) with a **promotion** path. One Rust
validator → wasm is the only way "same contract ⇒ same verdict" is true
by construction rather than by hope, and the conformance corpus is what
keeps it true. Effective-dated validation is the difference between a
clever git-backed store and a records system an auditor will accept: it
makes every historical record reconstructable *as it was validated when
written*. The `[min,max]` guard is kept but demoted to its honest role
(tool readability), so the governance question is answered by governance
machinery, not by a version-range heuristic.

## Consequences

- **New `crates/contract`** (pure descriptor + parser + compat) and a
  **rewritten `crates/validators`** (pure, kind-dispatching, wasm-clean);
  `crates/app` `describe` serves the *loaded* contract, `preset.rs`
  becomes the seed baseline, not the runtime source.
- **The storage port goes collection-generic** (ADR-018 / ADR-033
  consequence) and records gain a `contract_version` stamp.
- **`schema/contract.schema.json` rewritten** to the canonical scalar
  set and self-validated in CI; `schema/registry-contract.json`
  regenerated to the new form and relocated to `.part-registry/`.
- **`pr check` becomes contract-driven over JSONL** (today CSV/parts-
  only) and fetches historical contracts for effective-dated validation.
- **The FE** deletes its TS validation rules, consumes the wasm
  validator (cheap-native + semantic-wasm split), drops the silent
  fallback, and gains the message catalog.
- **A conformance corpus + 3-runner harness** joins CI; a wasm-clean
  build job guards the purity boundary; a size-budget assertion guards
  bundle bloat (`panic=abort`, `opt-level=z`, trimmed `serde_json`).
- **Bootstrap** seeds `.part-registry/contract.json` with the change-
  control header v1.

## Open questions / supersession triggers

- **CSV-import batch reconciliation** for `reference` + `on_unknown`
  (grouped unknowns; bulk create / map-to-existing / reject-row) — must
  be designed before `reference` ships in the import path, or operators
  pre-clean in Excel and the type buys nothing. *Filed as a follow-up.*
- **Absent QMS contract primitives** (compliance review): per-collection
  `retention`/immutability declaration, `signature_meaning` on contract
  changes (Part 11 §11.50), `vocabulary_owner` (+ review cadence +
  deprecation path) for controlled vocabularies. *Filed; not in P0.*
- **The full `typeFields` → `types` collection migration** (schema-as-
  data, kind tree with inheritance — ADR-035 §0/§2) — v2; the v1 async
  seam is the forward-compat hedge.
- **`relations[]` + graph integrity** beyond `components` (ADR-035 §1a) —
  lands with the controlled-vocabulary collections.
- **Decimal representation on the wire** — string vs JSON number with
  declared scale; settle when `decimal` is implemented (string is
  lossless and the safer default).
- **Effective-dated validation performance** — fetching a historical
  contract per record could be O(versions); the gate should resolve the
  in-force contract once per version-epoch, not per row.

## References

- Issue #204 — the spike + 4 fresh-context reviews + this resolution
- Issue #189 — parent eQMS spine spike
- ADR-033 §2-4 (self-describing anatomy + scalar set + canonical layout)
- ADR-035 §0-5 (collections metamodel, three-tier records, typed ids)
- Obligations `registry-self-describing`, `core-plus-custom-schema`
- 21 CFR Part 11 §11.10(e), §11.50, §11.200; ISO 13485 §4.2.5; ALCOA+
