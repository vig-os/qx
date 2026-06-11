# ADR-035 — Registry data model: collections metamodel + tiered schema + JSONL

- Status: Accepted
- Date: 2026-06-11
- Component / area: the record model for a part (`crates/domain`), the
  validators (`crates/validators`), the on-disk format (`crates/storage*`
  behind ADR-018), and what `contract.json` declares (ADR-033). Refines
  ADR-012 (adds the `kind` discriminator + components relation;
  **retires `batch`**), ADR-018 (adds a JSONL adapter), ADR-033 (the
  contract now declares collection descriptors).
- Reviewers: Lars Gerchow (accepted 2026-06-11)
- Related: ADR-012 (part identification), ADR-016 (PR-diff-as-review),
  ADR-018 (storage port), ADR-033 (self-describing contract + scalar
  custom-field types), ADR-034 (tiered/federated enforcement, SSoT-core
  gate)
- Feeds: `decisions/explorations/operations-catalog.md`

## Context

ADR-033 fixed the schema as "core fields + flat custom scalars." But
parts are heterogeneous: a `sensor` / `t-sensor` carries sensor fields
(`sensor_type, range, unit, calib_due`), a `cable` carries
`length, gauge`, and operators need to capture ad-hoc data *before* it is
formally schematized. A flat column list models none of this well —
type-specific fields become sparse columns, and ad-hoc data has nowhere
to live but a JSON string stuffed in a CSV cell (the string-parsing pain
this ADR removes). The need is a record model that is **ragged + nested
by design**, with enforcement that hard-gates the regulated minimum,
composes per type, and leaves an explicit escape hatch.

## Decision

### 0. One metamodel: everything is a collection (added 2026-06-11)

A registry is a set of **collections** declared in its `contract.json`.
One descriptor shape is the single higher config level:

```
collection = {
  name,              # "parts" | "types" | "vendors" | "products" | "locations"
  id,                # scheme(s), mintable vs imported (parts: nano14 per ADR-012)
  kind_tree?,        # opt into a kind hierarchy (entities in `types`; parts: on)
  lifecycle?,        # statuses + allowed transitions (parts: unbound→bound→void)
  fields[],          # typed scalars + attachments (ADR-033 §3) + required / meaningful_from
  relations[]?,      # refs to other collections + graph rules + backlinks
                     #   (parts.components → parts, acyclic, void-policy)
  open_properties?,  # tier-3 escape bag on/off
  render,            # display labels / label block / groupings (§1a — no label_field:
                     #   `label` is intrinsic micro-core; display naming is render metadata)
}
```

(2026-06-11 generalization pass: `kind_tree?` is a descriptor capability any
collection may opt into — the engine dispatches on the flag, never on the
collection name; parts pins it on. A vendor "kind" stays a plain `enum`
field until it actually needs its own field schema — kind buys
inheritance + dispatch, an enum buys a value.)

**One generic engine** implements every collection: id-minting
(parameterized by the declared scheme), validation (`core ∪ fields ∪
shape`), FK + graph integrity, lifecycle transitions, rename-as-label-
edit, audit-append. The operations catalog **collapses** into one
parameterized family — `Create / Get / List / Edit / Transition /
Promote / Resolve / Describe / Export / Count { collection, … }` — with
`mint` = `Create{parts}` sugar, `create-vendor` = `Create{vendors}`,
`void` = `Transition{parts, void}`. Adding a collection or a field is a
**contract edit (PR-reviewed), not code**; `Describe` renders the
descriptors ("what exists in my registry and how it's minted" is
introspectable data). Family rules (2026-06-11 pass):

- **`Transition` takes an optional fields payload** —
  `Transition{collection, id, edge, fields?}`, validated as "fields
  whose `meaningful_from` is satisfied by the target status". So
  `bind` = `Transition{parts, →bound, fields}` and `rebind` =
  `Edit{parts, id, fields}` on a bound part. The crisp boundary:
  **status-changing ⇒ Transition; status-preserving ⇒ Edit.**
- **One `Selection` type everywhere** — `Selection = Ids([Id]) |
  Filter(Filter)` where `Filter` is *the* `List` filter; every
  selection-taking op (Print, Export, …) and the stream-read ops reuse
  it. One filter grammar, no drift.
- **`Count{collection, filter, by}`** is the only aggregation:
  group-by-count, single field, single collection — inside the
  filter/sort/page surface, never a join. `registry-stats` is its
  render.
- **`Export{collection, format}`** is an operation (and a build
  artifact); a derived export is **never committed** to the registry —
  a committed CSV beside the JSONL is a rival truth (the `batch`
  failure at artifact level).
- **One change vocabulary** — diff classification, audit `ActionKind`,
  and authz `Action` (ADR-016/020/022) all key on
  `{collection, op-kind}` (op-kind ∈ create / edit /
  transition(to-status) / delete / descriptor_change / bulk(n)) —
  derived from which `collections/*.jsonl` changed plus the
  per-line delta against that collection's descriptor. The parts-era
  enums (`row_bind`, `row_void`, `ActionKind::Mint`,
  `TargetRef::BatchLabel`) dissolve into it; `header_change`
  generalizes to `descriptor_change` (a contract edit,
  CODEOWNERS-gated); targets are bare typed ids. Classification now
  covers *every* collection, not just parts.

Three guardrails keep this from becoming an inner platform:

1. **Presets, not config-all-the-way-down.** The tool ships
   **code-owned presets** — `parts` (the regulated tier-1 floor:
   ADR-012 id scheme, lifecycle, `components` relation, audit/print
   hooks, open bag) and minimal vocab presets. A registry's contract
   instantiates and may *extend* a preset; it **cannot weaken or
   redefine** preset fields, lifecycle, or relations. The Class B floor
   stays non-configurable.
2. **Exactly one meta level.** Contracts declare collections; the
   tool's meta-schema (Rust types) governs contracts; there is no
   meta-meta — the regress stops in code.
3. **Logs are streams, not collections — and there is exactly ONE
   stream.** Print events fold into the audit spine as a typed event
   kind (`{parts, print}` in the unified vocabulary, with
   layout/size/copies/output_mode in the payload) — completing the fold
   ADR-022 §Migration already decided; `print_log` leaves the anatomy
   and `PrintEvent` leaves the storage port (ADR-018 refinement).
   ADR-015's FK/ordering invariants restate as audit-spine validators.
   With one hardwired stream, **stream-descriptor meta-machinery is
   rejected** (speculative generality; brushes the one-meta-level
   guardrail) — revisit only if a second genuine stream ever exists.
   Two corollaries (2026-06-11):
   - **The stream is JSONL** like everything else — each line a
     self-describing typed event (`{ts, time_source, operator,
     action: {collection, op-kind, …}, target: typed-id, payload,
     chain_hash, signature}`), so external ingest (jq / DuckDB / SIEM)
     tails it directly. **Per-action "logs" (mint log, print log, …)
     are derived views** — `List{audit, filter: {collection, op-kind}}`
     — never separate files (the print_log mistake × N; cross-action
     ordering must live in one place).
   - **No registered-actions list in the contract.** The effective
     action set is *derived*: op-kinds (code-owned with the op family,
     payload schemas versioned with the tool) × declared collections ×
     declared lifecycle edges, gated by the manifest's op×collection
     grain. A second `actions[]` registry would denormalize that
     product (`batch`-shaped); `Describe` renders the effective set as
     introspectable data.

**`batch` is deprecated** (2026-06-11, retires it from the ADR-012
minimum). The mint event is already a first-class record: every mint
writes an `AuditEntry` (and a proposal/PR) listing the ids it created,
and all parts of one mint share **one** `created_at` stamp — so `batch`
was a denormalized, hand-named duplicate of the audit spine. Grouping =
stamp-equality (human handle) or the mint audit-entry/proposal ref
(exact key); "batch pickers" become a derived **mint-events** view
(timestamp + count + operator). Manufacturing/vendor *lot* was never
this field — if lot tracking is needed it's a deliberate tier-2 field
on the relevant `kind`. Durable human-named groupings, if ever wanted,
are a future *tag* concept (tier-3 or a collection), not a mandatory
core column.

Storage is uniform: **`collections/<name>.jsonl`**, one entity per
line — including `collections/parts.jsonl` (no special-cased registry
file).

#### The entity store (added 2026-06-11)

Pushed to its conclusion, the metamodel makes a registry a **git-native
NDJSON entity store** — and naming that honestly clarifies every rule:

- **Global id space, typed ids.** An id is a `(scheme, value)` pair,
  canonical form **`scheme:value`** (CURIE/multihash-style; nano-id's
  alphabet has no `:`). Ids are unique store-wide on the canonical form,
  so `Resolve{id}` is universal and references (`components`, `vendor`,
  `kind`, …) never name their collection. Rules:
  - **Schemes are declared per contract/collection**, each with its own
    format validator (`nano14` = ADR-012 alphabet+length; `sha256` =
    hex64; `udi`/`gs1` = their standards). Unknown scheme = validation
    error; admitting a scheme is a deliberate, CODEOWNERS-gated
    contract change.
  - **One default scheme per registry; its bare value is a valid short
    form** (no colon → default). QR payloads, labels, and human entry
    stay the bare 14-char value — a scheme prefix would nearly double a
    Micro QR payload (ADR-031 px-true cost) — and bare-as-default is
    the zero-cost migration for existing data.
  - **Mintable vs imported** per the descriptor's id block: nano ids
    are *minted*; external schemes (`udi:`, `gs1:` — plausible for a
    Class B platform meeting FDA UDI) are *asserted/imported*, never
    minted here. Content-addressed refs (`sha256:`) become admissible
    the same way.
  - The **parts preset pins `nano14`** (the ADR-012 floor, unchanged).
- **Uniform entity micro-core.** Beneath any preset, *every* entity
  shares `{id, label?, created_at (+time_source), audit-append on every
  mutation}`. The parts "core" is the parts preset extending this;
  vendors/types extend it with less. Four layers, uniform:
  micro-core → preset core → declared fields → open properties.
- **Collection vs kind — the rule.** A **collection** is a partition
  with its own *lifecycle, relations, and ACL surface* (own file →
  path-scoped CODEOWNERS + scoped PR diffs). A **kind** is a schema
  specialization *within* a partition: kinds form a declared **tree
  with field inheritance** (`part/sensor/t-sensor` inherits `sensor`'s
  fields), living as entities in the `types` collection — schema-as-data
  where vocabulary changes often, while `contract.json` stays the
  structural floor that changes rarely. A vendor is therefore *not* a
  part-kind (no mint→bind lifecycle, no components, no labels): it is
  its own simpler collection on identical machinery.
- **One generic query.** `List{collection, filter}` (filter/sort/page
  over `core ∪ declared ∪ properties`) serves every collection; the v1
  data-grid, labels, TUI views, and MCP responses are all **opinionated
  renders over the one store**.
- **Adapters are views, not rival truths.** SQLite/DuckDB (ADR-018)
  become indexes / materialized views *of* the entity store for query
  scale; JSONL + git stays the source of truth (transaction = PR,
  constraint = validator, history/WAL = git, ACL = CODEOWNERS).
- **Honest limits (the fourth guardrail).** This is a regulated
  document store, **not a general DBMS**: concurrency = PR
  serialization + git merge; no indexes until an adapter provides them;
  the query surface stays filter/sort/page — **no join DSL** (joins are
  code or materialized views). It must stay that way.

The tiers below (§1–§3) are therefore *per-collection* properties; the
`parts` preset is simply the collection that uses all of them.

### 1. Three-tier record model

| Tier | What | Enforcement | Declared by |
|---|---|---|---|
| **1 — common core** | `id, status, created_at, transitioned_at{…}, kind, components[]` — `created_at` is the micro-core stamp (rendered "Minted" for parts; replaces `minted_at`); `transitioned_at` = engine-materialized lifecycle stamps (`transitioned_at[bound]` renders "Bound", replaces `bound_at`); `batch` retired per §0; `components` = the as-built relation | **hard-gated, always, everywhere** (incl. referential + acyclicity integrity on `components`) | tool-owned |
| **2 — per-type schema** | a `kind` discriminator; per-`kind` field set (ADR-033 scalar types + `required`/`meaningful_from` flags) | **enforced per type** — a `kind=K` part must satisfy K's schema; the gate *composes* core + K's rules | the registry's `contract.json` |
| **3 — open `properties`** | a freeform `properties` object — arbitrary keys + JSON values | **shape-checked only** (must be a well-formed object; nothing required/typed smuggled past validation) | nobody — the escape hatch |

Validation dispatches on `kind`: `core gate ∪ kind-schema gate ∪
properties shape-check`, all run by the same SSoT core CLI (ADR-034 §2),
loading the registry's declared types.

### 1a. Components — composition is core

`components` is a first-class **core** field: a part's **as-built
genealogy** — references to the *specific part instances* it is built
from (each entry: a referenced part `id`, optional `qty`, optional
`designator`/role). It is instance-level (actual serialized parts), not
a type template — the type-BOM is a separate, deferred enhancement (Open
questions). Composition is fundamental to traceability (a part's
genealogy = its audit trail **plus** its component graph), so it lives in
tier 1 and is hard-gated — **not** a deferred custom `ref` (ADR-033).
This pulls **graph integrity into the core validators**: every
referenced id must exist (referential integrity), no part may
transitively contain itself (acyclicity), and voiding/deleting a part
that is a component of another is policy-gated (ADR-016/034 — block or
warn, never silent). Cross-registry components (a component sourced
from another registry) are deferred to the federation story.

**Relations generalize (2026-06-11).** `components` is the parts
preset's instance of the descriptor's general `relations[]` mechanism
(§0): any collection may declare typed relations to other collections,
each with its own graph rules — this subsumes the previously deferred
`ref` custom fields (declared relations are the typed-ref mechanism;
ad-hoc untyped ref fields stay out). Two SSOT rules govern every
relation:

- **Store one direction, derive the reverse.** A part stores
  `vendor: <id>`; "the vendor's parts" is `List{parts, vendor=id}` —
  never a second stored array on the vendor (the same denormalized-
  duplicate failure `batch` was). Likewise "where is this part
  installed?" (`used_in`) derives from `components`.
- **Backlinks are render hints, not data.** A descriptor may declare a
  named backlink (`vendor.parts = backlink(parts.vendor)`) so
  `Describe`, grids, and detail views render the reverse listing
  consistently — pure metadata, zero storage.
- **Naming lives in the descriptor.** Fields, relations, and backlinks
  all carry display metadata (`label`, order, grouping — extending the
  existing `label`/`editable`/`meaningful_from` field pattern):
  `components` renders as "Bill of materials", `vendor.parts` as "Parts
  supplied", `used_in` as "Installed in". Declared once in the
  contract; every shell's opinionated view is *generated* from the
  descriptor — no hardcoded display strings in any shell.

### 1b. Time: the mint-event invariant + clock trust

With `batch` retired, the creation stamp carries grouping weight, so
these rules become explicit:

- **`minted_at` ≡ `created_at`** (2026-06-11 pass): the micro-core
  already gives every entity `created_at` — a separate `minted_at` was
  the same duplicate shape that killed `batch`, in schema instead of
  data. One stored field, `created_at`; "Minted" is parts *render*
  metadata. (For *imported* ids — `udi:`, `gs1:` — nothing is "minted";
  `created_at` = "entered this store" is the only honest semantic, and
  manufacture date is a deliberate tier-2 field.)
- **One stamp per mint event** — all parts created by one mint request
  share a single `created_at`. Stamp-equality is the human group key;
  the mint `AuditEntry` / proposal ref is the exact key.
- **The materialization rule** (what `batch` failed and `bound_at`
  passes): *a derivable fact may be materialized iff the no-join query
  surface needs it; a materialized stamp is a validator-checked cache
  of the audit spine* — engine-written only, never hand-set, and CI
  cross-checks stamp == the transition `AuditEntry`'s ts (lenient for
  the documented fail-open audit gap). Collections declaring a
  `lifecycle` get engine-materialized `transitioned_at[<status>]`
  stamps; `bound_at` is the parts render name for
  `transitioned_at[bound]`. `batch` failed this rule because
  stamp-equality already served grouping without materialization.
- **Don't trust the system clock; don't require an external one** (which
  would break ADR-023 offline mode). Layered policy:
  1. **Online: prefer transport time** — the ADR-031 pre-flight already
     talks to the host; use the server `Date` over the local clock.
  2. **Always sanity-check**: a new stamp must be ≥ the registry's
     newest known timestamp (monotonic vs `main`) and within tolerance
     of server time; skew beyond threshold blocks with a fix-your-clock
     error.
  3. **Record provenance**: stamps carry `time_source: server | system`
     (the `Operator.source`/`verified_at` pattern applied to time);
     offline falls back to system + provenance instead of blocking.
  4. **CI is the trusted backstop**: `pr check` verifies proposed stamps
     are plausible against the PR's host-attested `created_at` — an
     operator can skew their clock, not GitHub's.
  5. **Cryptographic time later**: the deferred Sigstore/Rekor path
     (ADR-024) carries inclusion timestamps — the upgrade is already on
     the crypto roadmap, no NTP infra now.

This applies to all audit-spine timestamps (`created_at`,
`transitioned_at[…]`, print-event `ts`, audit `ts`), not just minting.

### 2. Types are per-registry, with seeded defaults

Type schemas are declared **per registry** in its `contract.json`. There
is **no shared cross-registry type vocabulary** (deferred). Bootstrap
**seeds commented-out starter type definitions** (`sensor`, `cable`, …)
so a new registry starts from a good template rather than a blank slate;
the registry then owns and evolves its own types. "Federated enforcement"
means per-type schemas compose *within* a registry — not that types are
shared across registries.

### 3. Promotion: tier 3 → tier 2

When a `properties.foo` proves load-bearing, **promote** it: add `foo` to
a `kind`'s schema (a scalar type + flags), migrate existing
`properties.foo` values into the typed field, drop the key from the bag.
Auditable + PR-reviewed like any change; a tool-assisted `promote`
operation performs the mechanical migration. **Capture never blocks on
schema** — that is the point of the open bag.

### 4. Serialization = JSONL (CSV becomes export)

The primary on-disk format is **JSONL / NDJSON** — one JSON object per
part, one per line — via a `storage_jsonl_git` adapter behind the ADR-018
port. It carries typed values + nested per-type fields + `properties`
natively, while staying **line-oriented** so the PR-diff-as-review model
(ADR-016) and append/stream paths still work. **CSV remains a flat
export / interop format** (spreadsheets, bind-templates), not the source
of truth. SQLite/DuckDB stay the query-scale future adapters (a JSON
column holds tier 3). The domain `Part` is format-agnostic, so this is an
**adapter choice, not a rewrite**.

**Attachments & prose live out-of-line (2026-06-11).** Large content —
how-tos, long comments, datasheets, images — would wreck JSONL's
line-diff reviewability, so it never lives in a JSONL line. Instead: an
**`attachment` field type** (in the ADR-033 scalar set) whose value is a
**`sha256:` typed id** (§0), with the blob stored at
`attachments/<sha256>.<ext>` — the extension carries the media type
("file ending decides") and keeps files humanly openable; per-field
constraints narrow it (`attachment(md)`, `attachment(pdf|png)`); prose =
a markdown attachment, rendered inline by the opinionated views.
Content-addressing makes attachments **tamper-evident**: editing one
yields a new hash, so the referencing entity visibly changes in the PR
diff — evidence cannot be silently rewritten. Validators enforce
ref-exists + hash-matches-content. Git-lfs is the deferred escape hatch
for genuinely large binaries (Open questions).

### 5. Controlled vocabularies — id'd reference entities

Beyond parts, a registry holds a set of **controlled-vocabulary
collections** — **types/kinds, products, vendors, locations** — each
entity carrying a **stable id assigned once (immutable) + a mutable
`label`** (+ optional attributes; a *type* entity also carries its
tier-2 field schema). Any part field that names one of these references
it **by id, never by name**:

- **Rename-safe by construction** — renaming a vendor / type / location
  is a one-record `label` edit; every part referencing its id stays
  valid. This is ADR-012's "stable id, mutable label" discipline applied
  to the whole vocabulary (parts already had it; `batch` / `OperatorId`
  partly).
- **Referential integrity is a core validator** — a part's `kind`,
  `vendor`, `location`, `product` ids must resolve to a live vocab
  entity (same FK discipline as `components`); a dangling reference is a
  hard error.
- **Promotion still applies** — a value can start as tier-3 free text and
  be promoted into a controlled vocabulary (mint an entity id + back-fill
  references), the same pattern as field promotion.
- `kind` is the **schema-bearing** vocabulary (`{id, label, fields[]}`);
  product / vendor / location are `{id, label, …attrs}`.

Vocabularies are ordinary **collections** per §0 (the `types` collection
is the schema-bearing one); storage follows the uniform
`collections/<name>.jsonl` layout — ADR-033 anatomy.

## Rationale

This is the project's own governance pattern applied to data: **hard-gate
the deterministic** (the regulated common core), **federate the rest**
(per-type schemas as composable units), **explicit auditable escape
hatch** (the open bag) with a **promotion** path — the same
gate/nudge/escape shape as guardrails and the obligations registry. JSONL
fits ragged/nested data while *preserving* the git-line-diff review model
that made CSV attractive; going straight to SQLite would forfeit that
(binary, not diff-reviewable). Per-registry types + seeded defaults avoid
both reinvention-from-scratch (bootstrap hands you a starting vocabulary)
and the infra/governance cost of a shared type registry (which can be
added later if real reuse demand appears).

## Consequences

- **`crates/domain` `Part`** gains `kind`, `components` (the composition
  relation), a per-kind typed field set, and an open `properties` map;
  **ADR-012** gains the `kind` discriminator + the components relation.
- **`contract.json`** declares core version, per-`kind` schemas (scalar
  fields + flags, ADR-033 §3), and that `properties` is open/shape-checked.
- **`crates/validators`** become kind-dispatching + schema-driven
  (`core ∪ kind ∪ shape`) and gain **graph integrity** for `components`
  (ref-existence, acyclicity, void/cascade semantics).
- **`storage_jsonl_git`** becomes the primary adapter; `storage_csv_git`
  demotes to an export/import path. ADR-018's port is unchanged.
- **One generic collection engine** replaces per-entity CRUD: the
  `Request` enum carries `Create/Get/List/Edit/Transition/Promote
  { collection, … }` instead of a bespoke variant family per entity
  (shrinks ADR-030's protocol and the §8 parity matrix).
- **Controlled-vocabulary collections** (types / products / vendors /
  locations) are id'd entity tables (`collections/*.jsonl`); part fields
  reference them **by id**, and **FK integrity across all of them is a
  core validator**. Renames are label-only.
- **`batch` is removed from the core** (validators, mint/label CLI
  selection, print-event schema, web batch pickers): selection moves to
  `minted_at` / mint-event; legacy `B-*` strings migrate (one mint-event
  per distinct batch value). UI "batches" become a derived mint-events
  view.
- **Timestamps gain provenance** (`time_source`) + the pre-flight skew
  check and CI plausibility check (§1b).
- **`contract.json` declares the collection roster** (refines ADR-033
  §2–3: the contract's unit of declaration is the collection
  descriptor, with the `parts` preset mandatory).
- **Bootstrap** seeds the preset collections + commented-out starter
  types.
- **`promote`** joins the operations catalog (ADR-030 §8 parity).
- **SOUP**: `serde_json` (already a dependency) carries JSONL; the CSV
  crate moves to the export path — net neutral.

## Open questions / supersession triggers

- **Shared cross-registry type vocabulary** — deferred; revisit if the
  same type is hand-redefined across enough registries to justify the
  governance infra.
- **Is `kind` a fixed/enumerated set** (declared in the contract) or open?
  Lean: declared set, so an unknown `kind` is a validation error.
- **Schema/core/type version migration** mechanics (ties to ADR-033's
  contract-migration open question).
- **Relational `ref` *custom* fields** — still deferred (ADR-033);
  `components` (§1a) is the one blessed core relation, already in tier 1.
- **Component void/cascade semantics** — voiding a part that is a
  component of a bound assembly: block vs warn vs cascade (ADR-016/034
  policy). Settle when composition lands.
- **Cross-registry components** — a component sourced from another
  registry; deferred to the federation story.
- **Type BOM + as-built validation** *(filed to review)* — a per-`kind`
  design BOM (type + quantity) that the as-built `components` is checked
  against ("as-built satisfies BOM"). Deferred enhancement on top of the
  instance-level core relation, tracked as an issue.
- **Tags / named groupings** — if a durable human-named grouping is ever
  wanted post-`batch` (e.g. "sheet-1"), it's an opt-in tag concept
  (tier-3 property or a collection), not a core column. Re-opens on a
  real operator request.
- **Trusted timestamping** — if an auditor requires cryptographic time
  attestation before the Sigstore/Rekor path (ADR-024) lands, evaluate
  RFC 3161 TSA / Roughtime then. Until that trigger, §1b's layered
  policy stands.
- **Operators as an imported-id directory collection** — every audit
  entry / roles row references an operator id, yet operators are the one
  referenced thing that isn't an entity (`Resolve{operator-id}` has no
  target; display names have no rename-safe home). An `operators`
  collection with *imported* ids (`github:lars`) would be a directory
  record, not user management (ADR-020's line) — but IdP-sync and
  lenient-FK semantics need design. Filed for review, not adopted.
- **Attachment storage scaling** — plain files in `attachments/` until
  size hurts; git-lfs (or content-store adapter) on a real trigger.

## References

- ADR-012 — Part identification (gains `kind`)
- ADR-016 — PR-diff as the review surface (why line-oriented matters)
- ADR-018 — Storage as a port (the JSONL adapter slots in)
- ADR-033 — Self-describing contract + scalar custom-field types
- ADR-034 — Tiered/federated enforcement, SSoT-core gate
- `decisions/explorations/operations-catalog.md` — adds `promote`
- JSON Lines — <https://jsonlines.org/>
