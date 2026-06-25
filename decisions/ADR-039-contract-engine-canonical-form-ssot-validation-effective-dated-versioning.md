# ADR-039 — Contract engine: canonical form, SSOT validation, effective-dated versioning

- Status: Accepted
- Date: 2026-06-12 (Accepted 2026-06-14)
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

**Forward-compat is engine behavior keyed on `format_version` (§6), not
in-contract config:** an unknown *type* is always rejected (a reader
that silently treats an unknown scalar as `string` passes garbage
through the gate); an unknown *prop on a known type* warns by default.
What "known" means is fixed by the `format_version` the engine
implements, so there is no per-contract `unknown_*_policy` toggle to
drift. Format generations are additive-only.

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
  `format_version` compat check (§6). Pure: `serde`/`serde_json`
  only, no `std::fs`, no `tokio`, no `git2`. Builds for
  `wasm32-unknown-unknown` with zero features. The **only** parse path
  is `Contract::from_bytes(&[u8])` + `is_compatible(engine_version,
  contract)` (= `TOOL_SUPPORTED.contains(contract.format_version)`) —
  CLI does `fs::read`, FE does `fetch().arrayBuffer()`, both hand bytes
  to the same function (one parser, one compat rule).
- **`crates/validators`** — depends on `contract` + `serde` only; takes
  `&Contract` + a record, returns a `Verdict`. No I/O, no time, no
  randomness. Same wasm-clean rules. Kind-dispatching
  (`core ∪ kind ∪ shape`).
- **`crates/storage*`, git, `crates/cli`, `crates/app`** — host-only;
  may depend on `validators`, **never the reverse**.
- **`crates/wasm`** — thin façade: `validate_record(contract_bytes,
  record_bytes) -> Verdict`, marshalling only.

**Enforced mechanically, not by policy:** a CI job runs `cargo build -p
qx-contract -p qx-validators --target
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

1. native Rust (`cargo test -p qx-validators`),
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
  true → warn`, or demoting a tier-2 field to tier-3, requires
  **host-projected approval** (§6): a PR carrying the relaxation,
  CODEOWNERS-routed, with ≥ 2 distinct approvers + rationale — it is not
  a silent edit.
- **Drift report.** The gate emits a periodic report of fields living in
  tier-3 with regulated-sounding names, so de-facto demotion is visible.

### 6. Versioning + effective-dated validation — git-native (revised after review, #204)

The auditor's question is *"under which contract, approved by whom, on
what date, was this record validated when written?"* The first draft
answered it with an in-file change-control header (`version,
effective_from, approved_by, approval_commit_sha, change_rationale`) +
an integer `contract_version` stamped on each record. Review (#204)
showed that **re-stores in-band what git + GitHub already record
out-of-band, authoritatively and tamper-evidently** — the same
denormalization failure that retired `batch` (ADR-035 §0) — and the
in-file copy is *weaker* than the source (an operator can type any name
into `approved_by`; a host review is authenticated). It also conflated
two version axes. The revised model leans on the substrate:
**transaction = PR, constraint = validator, history/WAL = git, ACL =
host review** (ADR-035 §0 entity-store principle).

**Three axes, three homes — only the first is in-file:**

1. **`format_version` (integer, in-file) — engine↔contract parse
   capability.** "Can this *binary* read a contract in this format?" git
   cannot answer this (it is a capability fact, not history). The
   **tool** holds the supported range as a const; `is_compatible(engine,
   contract)` = `TOOL_SUPPORTED.contains(contract.format_version)`;
   outside → refuse or offer migration. This is the **only** in-file
   version, and it moves rarely (a format generation, not a content
   edit).
2. **Contract identity = its content hash (derived).** The contract *is*
   `sha256(contract.json)`; no hand-bumped integer to forget, no
   two-branches-both-claim-v2 collision, and self-certifying offline
   (verify content == hash with no git). Tamper-evident like attachments
   (ADR-035 §4): edit the contract, its hash changes, every reference
   visibly moves in the diff. This hash feeds the **ADR-037 anchor
   ledger** directly.
3. **Governance (who/when/why/approved) = projected from the host,
   never stored.** `effective_from` = the merge commit's host-attested
   committer time (operator-unsettable — the trusted-clock property
   ADR-035 §1b wanted, for free); `approved_by` = the PR's authenticated
   reviews; `change_rationale` = the PR/commit message;
   `approval_commit_sha` = the merge SHA; "which contract governed this
   record" = `git show <record-commit>:.qx/contract.json`.

**Effective-dating is commit-resolved — and that gives the ALCOA
property for free.** A record is governed by the contract content in the
tree at its commit. The gate validates **changed** records in a PR
against HEAD-of-PR's contract; already-merged records were validated
against their commit's contract at merge time (a required status check
guarantees it) and are **never re-checked**. So a *tightening* contract
change **cannot retroactively invalidate history** — precisely the
"don't re-qualify historical evidence" guarantee (21 CFR Part 11
§11.10(e); ISO 13485 §4.2.5), achieved by git's structure rather than an
in-file stamp. A **migration** is a forward PR that rewrites old records
to satisfy the new contract, validated at that PR — forward-only,
auditable, with zero versioning machinery.

**Records carry no `contract_version` stamp** in the git-resident case
(the commit resolves it). A **content-hash stamp `contract: sha256:…` is
added only when data leaves git** — a CSV export, a printed label's
metadata, a record shipped to an external system — so it self-describes
which contract governed it; even then it is *derived*, never hand-set.

**Governance is policy-checked, host-projected, host-neutral.** The gate
enforces the *policy* (≥ 2 distinct approvers, rationale present, gate
green, CODEOWNERS on `.qx/contract.json` satisfied) against
the host's review record, and may materialize a **read-only receipt**
(like the print receipt) for offline auditors — but the source of truth
is the host. Because authority lives with the host (ADR-019/034), the
contract file stays **host-neutral**; the gate *projects* governance
from whatever host it runs on: PR reviews + merge commit on GitHub;
signed commits + commit trailers on `file://`; commit metadata on a Dolt
backend. Same policy, different projection.

**Why this is better, not just smaller:** authenticated host facts beat
operator-typed in-file fields; a derived hash beats a hand-bumped
integer (can't forget, collision-free, verifiable offline); and the
contract stops carrying a worse copy of what git already holds. The one
thing genuinely *not* derivable — whether this binary can parse this
format — stays in-file as `format_version`.

And the presence-flag split the compliance review demanded (this is
validation semantics, unaffected by the versioning rework):

- **`required_to_enter: <status>`** — a hard **transition gate**: the
  entity cannot advance to `<status>` unless the field is present and
  valid. (Closes the backdating gap: required quality data is captured
  *to advance*, not after.)
- **`meaningful_from: <status>`** — downstream readers may trust the
  field once the entity reaches `<status>` (the existing semantic;
  documentation, not enforcement).

A field may declare either, both, or neither.

### 7. Anatomy, bootstrap, and the typeFields seam

- The contract moves to **`.qx/contract.json`** in the data
  repo (ADR-033 §4 anatomy); `bootstrap` seeds it from the tool baseline
  at `format_version` 1 (governance for that seed = the bootstrap PR's
  own host review).
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
keeps it true. Effective-dated validation — commit-resolved, so every
historical record is reconstructable *as it was validated when written*
— is the difference between a clever git-backed store and a records
system an auditor will accept; deriving it from git + the host review
(rather than an in-file header) makes the evidence authenticated and
tamper-evident instead of operator-asserted. The one fact git cannot
derive — whether this binary can parse this format — stays in-file as
`format_version`.

## Consequences

- **New `crates/contract`** (pure descriptor + parser + compat) and a
  **rewritten `crates/validators`** (pure, kind-dispatching, wasm-clean);
  `crates/app` `describe` serves the *loaded* contract, `preset.rs`
  becomes the seed baseline, not the runtime source.
- **The storage port goes collection-generic** (ADR-018 / ADR-033
  consequence); records carry **no** `contract_version` stamp
  (commit-resolved), only an optional derived `contract: sha256:…` when
  data leaves git (§6).
- **`schema/contract.schema.json` rewritten** to the canonical scalar
  set and self-validated in CI; `schema/registry-contract.json`
  regenerated to the new form and relocated to `.qx/`.
- **`pr check` becomes contract-driven over JSONL** (today CSV/parts-
  only): it validates changed records against HEAD-of-PR's contract and
  **never re-checks merged records** (commit-resolved effective-dating);
  governance is projected from the host review, not read from the file.
- **The FE** deletes its TS validation rules, consumes the wasm
  validator (cheap-native + semantic-wasm split), drops the silent
  fallback, and gains the message catalog.
- **A conformance corpus + 3-runner harness** joins CI; a wasm-clean
  build job guards the purity boundary; a size-budget assertion guards
  bundle bloat (`panic=abort`, `opt-level=z`, trimmed `serde_json`).
- **Bootstrap** seeds `.qx/contract.json` with the change-
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
