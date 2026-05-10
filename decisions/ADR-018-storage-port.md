# ADR-018 — Storage as a port

- Status: Accepted
- Date: 2026-05-10
- Component / area: cross-cutting — defines the trait shape every
  persistence backend in the project must implement; first adapter is
  CSV+git, future adapters cited in §Forward-compatibility
- Reviewers: Lars Gerchow
- Related: ADR-013 (Parts registry web app — names CSV+git as the
  substrate), ADR-014 (Web app architecture), ADR-015 (Print event
  log), ADR-016 (PR-diff policy enforcement), ADR-017 (Rust core +
  ports/adapters), ADR-019 (Proposal sink port — owns mutations),
  ADR-022 (Observability — audit-log shape), ADR-023 (Threat model +
  crypto-MVP scope — schema forward-compat), ADR-024 (Cryptographic
  baseline), ADR-027 (Port conformance + parity tests)

## Context

ADR-013 fixed the registry's substrate as a sorted CSV (`registry.csv`)
versioned in a git repository, with an in-browser DuckDB-WASM instance
loading that CSV for read queries and a PR-driven workflow for writes.
ADR-015 added `print_log.csv` as a non-destructive audit trail with the
same substrate. The Python tooling (`mint.py`, `bind.py`, `label.py`,
`tools/sheet.py`) and the TypeScript FE (`web/src/`) currently both
read those files through ad-hoc paths: `csv.DictReader(...)` in Python,
`papaparse` over a `fetch()` in TS. There is no abstraction; the file
layout is hardcoded across at least four call sites today.

ADR-017 commits the project to a Rust workspace with a
ports-and-adapters shape. Storage is one of the named ports
(`crates/storage/`). Concretely, three pressures force storage into a
trait rather than a free function:

1. **ADR-013's "data = git history" property is load-bearing.** The
   regulatory case in `METHODOLOGY.md` rests on `git log` answering
   "who changed what when, forever." Any storage implementation must
   either preserve that property directly (CSV+git, Dolt) or carry an
   equivalent audit-of-record story explicitly designed in. Without a
   trait that names "what storage owes the rest of the system," a
   future adapter could quietly drop the property and the breakage
   would only surface during an audit.
2. **ADR-023 §"Schema forward-compatibility"** requires the data
   structures (`AuditEntry`, `Part`, `PrintEvent`) to reserve
   `signatures: Vec<Signature>` and `chain_hash: Option<Hash>` columns
   even though MVP code paths populate `signatures` with one
   `GitCommit` variant and leave `chain_hash` as `None`. Storage
   adapters must round-trip those fields blindly — including adapters
   that don't yet exist — or activating Sigstore later (re-open
   triggers T2/T4 in ADR-023) requires a schema migration we
   explicitly committed to avoiding.
3. **ADR-027 cross-adapter parity tests** require that any second
   storage adapter pass a fixed-corpus parity suite vs. the CSV+git
   reference adapter. That contract has nowhere to live unless storage
   is a trait — it must be a single interface the conformance harness
   can call against any adapter implementation.

The fourth pressure is operational: the web app (per ADR-013) loads
the CSV directly into DuckDB-WASM. That access pattern is *also*
storage, just from a different process and different host. A
`Repository` trait in the Rust core does not eliminate the FE's direct
DuckDB-WASM read path on day one (the FE still parses the CSV), but it
*does* mean the day the FE migrates its read path through the
WASM-compiled core (per ADR-017 strangler-fig step 8), the abstraction
already exists.

## Alternatives considered

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| **Status quo: code reads CSV directly via Python's `csv` module + `papaparse` in TS** | Works today; zero up-front cost | Hardcoded paths everywhere (≥4 call sites); no abstraction means no parity testing, no schema-evolution story, no place to enforce ADR-023's forward-compat columns; cannot swap to SQLite/DuckDB/Dolt without a per-call-site refactor | Rejected — the cost is paid every time a storage concern needs to change |
| **Single `Repository` trait in `crates/storage/`, CSV+git as the first and only MVP adapter, designed-for SQLite / DuckDB / Dolt / file-per-entry future adapters** | Swappable; parity-testable per ADR-027; 12-factor-friendly per ADR-021 (adapter selected by env var); preserves ADR-013's git-history substrate via the first adapter; gives ADR-023's forward-compat columns a single owner | Up-front design cost (one trait + one adapter); requires discipline to keep the trait minimal | **Chosen** |
| **ORM (Diesel, SeaORM)** | Mature ecosystems; type-safe SQL; migrations built in | Drags SQL assumptions into the abstraction (CSV+git has no schema-migration story in the SQL sense; Dolt has its own migration tooling; file-per-entry has no schema at all); overkill for the current schema (≤5 tables, sub-million rows per ADR-013 §Rationale); pulls in ~50 crates of transitive deps for what is currently a 1-file CSV | Rejected — wrong abstraction for the storage *substrate* the project actually has |
| **Direct DuckDB-WASM as the only adapter** (resolving ADR-013's promise to its endpoint immediately) | Matches ADR-013's "WASM DuckDB" mention; one engine on FE and CLI; SQL ergonomics on day one | Locks the model to one engine; breaks the strangler-fig migration from CSV (ADR-013 explicitly defers Parquet/columnar storage as premature for sub-million rows); DuckDB-WASM is a query engine over the CSV today, not the storage substrate — collapsing them prematurely loses the git-history audit property unless paired with Dolt or similar | Rejected for MVP — ADR-013 already drew the boundary at "DuckDB queries the CSV; CSV is the substrate." This trait preserves that boundary while leaving room to merge them later via a `storage_dolt` or `storage_duckdb` adapter |
| **Skip the trait, write a thin wrapper module instead** | Less ceremony than a trait | A wrapper has one implementation by definition; the moment a second backend is wanted, the wrapper either bifurcates internally (inheritance-by-`if`) or gets refactored into a trait anyway. Pay the design cost now or pay it under deadline pressure later | Rejected — the trait IS the cheap option once you accept that a second adapter is plausible inside 24 months |

## Decision

Storage is a port. The Rust workspace declares a `Repository` trait in
`crates/storage/src/lib.rs` and ships exactly one adapter for the MVP:
`crates/storage_csv_git/`, which implements `Repository` over the
ADR-013 substrate (sorted `registry.csv` + ADR-015's `print_log.csv`,
both versioned in a git repository).

The trait surface is **read + audit-append only**. State-changing
mutations to `Part` records do not flow through `Repository`; they
flow through the `ProposalSink` trait (per ADR-019). This separation
is not stylistic — it is the mechanism that prevents future adapters
from quietly bypassing ADR-013's audit invariant ("every change is a
PR"). A trait that exposed `update_part(...)` could be implemented by
a SQLite adapter that writes the row directly with no PR, and the
violation would not surface until an auditor asked. The split makes
the violation a compile error: there is no method to call.

### Trait shape

```rust
// crates/storage/src/lib.rs

use crate::types::{
    Part, PartId, PartFilter,
    AuditEntry, AuditFilter,
    PrintEvent, PrintEventFilter,
    Hash,
};

pub trait Repository: Send + Sync {
    // Read: parts
    fn get_part(&self, id: &PartId) -> Result<Option<Part>, RepoError>;
    fn list_parts(&self, filter: &PartFilter) -> Result<Vec<Part>, RepoError>;

    // Read: audit log (per ADR-022)
    fn list_audit_events(
        &self,
        filter: &AuditFilter,
    ) -> Result<Vec<AuditEntry>, RepoError>;

    // Read: print events (per ADR-015)
    fn list_print_events(
        &self,
        filter: &PrintEventFilter,
    ) -> Result<Vec<PrintEvent>, RepoError>;

    // Append-only: audit + print events
    //
    // These are *not* mutations to Part state — they are
    // append-only side effects of actions that already happened
    // (a print run, an identity verification, a CI policy check).
    // Mutations to Part state go through ProposalSink (ADR-019).
    fn append_audit_event(&self, ev: AuditEntry) -> Result<(), RepoError>;
    fn append_print_event(&self, ev: PrintEvent) -> Result<(), RepoError>;

    // Reproducibility (per ADR-024): a deterministic content hash
    // over the current state, suitable for citing in a release
    // signature or comparing across two clones.
    fn snapshot_hash(&self) -> Result<Hash, RepoError>;

    // Optional capability: not every adapter can answer this in
    // O(1). CSV+git can (it's the HEAD commit); SQLite can (a
    // hash over the WAL position + table digests); file-per-entry
    // must hash the directory tree.
    //
    // NOTE: NO `update_part`, `delete_part`, `upsert_part`. State
    // changes flow through ProposalSink (ADR-019). The trait
    // deliberately offers no method that would let an adapter
    // bypass the PR pipeline.
}

#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("schema mismatch: {0}")]
    SchemaMismatch(String),
    #[error("backend error: {0}")]
    Backend(#[source] Box<dyn std::error::Error + Send + Sync>),
}
```

The data types (`Part`, `AuditEntry`, `PrintEvent`, `Signature`,
`Hash`, `PartId`) live in `crates/domain/` per ADR-017's workspace
shape. Each carries the ADR-023 forward-compat columns at the type
level — adapters cannot implement `Repository` without round-tripping
them.

```rust
// crates/domain/src/lib.rs (excerpt)

pub struct Part {
    pub id: PartId,
    pub status: Status,
    // ... existing ADR-013 columns ...

    // ADR-023 §"Schema forward-compatibility": present in MVP,
    // populated trivially today, semantically activated by a
    // future ADR-024 successor.
    pub signatures: Vec<Signature>,
    pub chain_hash: Option<Hash>,
}

pub struct AuditEntry {
    pub id: AuditId,
    pub timestamp: OffsetDateTime,
    pub actor: Operator,        // type defined by ADR-020; recorded per ADR-022
    pub action: Action,
    pub subject: SubjectRef,
    pub source: AuditSource,    // per ADR-022 — IdP attestation provenance

    // ADR-023 forward-compat. MVP populates `signatures` with one
    // `Signature::GitCommit { sha }`; `chain_hash` is `None`.
    pub signatures: Vec<Signature>,
    pub chain_hash: Option<Hash>,
}
```

The CSV+git adapter serializes `signatures` as a JSON-encoded column
and `chain_hash` as a hex string column (or empty if `None`). Both
columns exist in the CSV header from day one even though MVP rows
populate them with a single git-commit signature and no chain hash.
Adapters that lose those columns on round-trip fail the ADR-027
parity suite.

### Adapter selection

Per ADR-021 (12-factor configuration), the active adapter is selected
by an environment variable read at process start:

```
PARTREG_STORAGE_BACKEND=csv_git              # MVP default
PARTREG_STORAGE_BACKEND=sqlite               # future
PARTREG_STORAGE_BACKEND=duckdb               # future
PARTREG_STORAGE_BACKEND=dolt                 # future
PARTREG_STORAGE_BACKEND=file_per_entry       # future
```

The `cli/` crate's wiring code (per ADR-017's workspace shape)
matches on this value and constructs the appropriate `Box<dyn
Repository>`. Adding a future adapter is one new crate plus one match
arm; no caller needs to change.

### Repo split (per ADR-019)

Storage adapters operate against the **data repo** (the cloned working
tree of `part-registry-data` or wherever ADR-019 lands the data
repository). The **code repo** is not "storage" in this sense; the
code repo holds Rust crates, the data repo holds CSVs. The CSV+git
adapter takes a path to the data-repo working tree as its
constructor argument; future adapters take whatever connection string
their backend wants (`sqlite:///path/to/file.db`,
`duckdb:///path/to/file.duckdb`, `dolt://host:port/db`,
`file:///path/to/parts/`).

## Forward-compatibility

The trait must accommodate each of the following adapters without
redesign. For each: what triggers building it, what the integration
cost is, what trait surface it relies on.

### `storage_sqlite` — single-file local-first scenarios

- **Trigger**: a deployment context where the data repo is not
  cloneable (offline lab terminal, kiosk with no git installed) but
  the same registry data must be browsable. Or: read query latency
  on the CSV at >100k rows becomes a felt UX problem (today's CSV is
  ~hundreds of rows; ADR-013 sized DuckDB-WASM at ≤100k rows
  comfortably).
- **Integration cost**: ~3–5 days. SQLite schema mirrors the CSV
  columns 1:1 (each CSV becomes one table). Read methods become
  prepared statements. Append methods become `INSERT INTO audit_log`
  / `INSERT INTO print_log`. `snapshot_hash` is a hash over the
  schema digest + a stable order-by query digest.
- **Trait surface used**: all read methods, both append methods,
  `snapshot_hash`. Forward-compat columns (`signatures`,
  `chain_hash`) become `TEXT` columns with JSON-encoded contents.
- **Critical invariant**: a SQLite adapter does NOT replace ADR-013's
  git history; it shadows the data repo for read performance. The
  data repo remains the audit-of-record. If a deployment uses
  `storage_sqlite` exclusively (no git substrate behind it), that
  deployment has explicitly opted out of ADR-013's "data = git
  history" property and a successor ADR documenting the trade is
  required.

### `storage_duckdb` — analytics queries (ADR-013's WASM mention)

- **Trigger**: ADR-017 strangler-fig step 8 lands and the FE wants
  the WASM core (not the inline TS) to own the CSV-to-DuckDB load
  path. Or: a CLI-side analytical query ("every PT100 in `sdmd_v2`
  not recalibrated in 12 months", per ADR-013 §Rationale) becomes a
  felt requirement and parsing the CSV per-query in plain Rust is
  slower than wanted.
- **Integration cost**: ~5–7 days. The DuckDB adapter loads the
  CSVs into in-memory DuckDB tables on construction (or attaches them
  via `read_csv`). Read methods translate `PartFilter` /
  `AuditFilter` into parameterized DuckDB SQL. Append methods write
  to the underlying CSV via the CSV+git adapter held as a delegate
  (DuckDB is the query engine; CSV+git remains the substrate).
- **Trait surface used**: all read methods (DuckDB-accelerated),
  both append methods (delegated), `snapshot_hash` (delegated to the
  underlying CSV+git layer or computed over the DuckDB digest).
- **Critical invariant**: this adapter is a *query accelerator over*
  CSV+git, not a replacement for it. The constructor takes a CSV+git
  `Repository` instance for writes and reads-of-record; DuckDB is the
  query path only.

### `storage_dolt` — versioned SQL

- **Trigger**: the project outgrows CSV diff legibility (PR review of
  a 50-row change becomes painful) but is unwilling to sacrifice
  per-row history. Or: a regulatory ask for SQL-shaped audit reports
  ("show me the chain of custody for every part in batch X as a
  joined query") becomes recurring and ad-hoc DuckDB queries over
  CSVs are too slow.
- **Integration cost**: ~7–10 days. Dolt provides MySQL-protocol
  access to a versioned table store with git-style branches, merges,
  and commit history. The adapter is mostly schema mapping +
  connection management. Mutations (still routed via `ProposalSink`,
  not via `Repository`) become Dolt PRs in the same shape as today's
  GitHub PRs.
- **Trait surface used**: all read methods, both append methods,
  `snapshot_hash` (Dolt commit SHA). Forward-compat columns become
  proper SQL columns of type `JSON` and `BINARY(32)`.
- **Critical invariant**: Dolt preserves ADR-013's audit invariant
  ("every change is a versioned commit") without git as the
  substrate. A migration from CSV+git to Dolt does not require a
  successor to ADR-013; the substrate changes, the invariant
  doesn't. (This is the strongest case for why the trait must be
  read + audit-append only — the alternative-substrate story
  collapses if `Repository` allows direct mutations.)

### `storage_file_per_entry` — one TOML per part

- **Trigger**: the registry grows past ~50k–100k parts and the
  single sorted CSV becomes painful to diff (PRs touching scattered
  rows show large unrelated context windows; merge conflicts
  proliferate when two operators bind in parallel). One file per
  part flips the diff math: each PR touches only the files for the
  parts it changes.
- **Integration cost**: ~5–7 days. Each part lives at
  `parts/<id-prefix>/<id>.toml`; the audit log and print log remain
  CSV (append-only files don't have the per-row diff problem). Read
  methods walk the directory tree (or maintain an in-memory index
  built at construction time). Append methods for audit/print events
  remain identical to CSV+git.
- **Trait surface used**: all read methods (with an internal index
  for `list_parts` performance), both append methods,
  `snapshot_hash` (hash over the sorted directory listing of file
  hashes).
- **Critical invariant**: this adapter preserves ADR-013's substrate
  property (still git, still file-based, still PR-driven via
  `ProposalSink`); it changes only the file granularity.
  Migration from CSV+git to file-per-entry is reversible; a
  one-way conversion script rebuilds the CSV from the TOML tree
  and vice versa. Parity testing per ADR-027 confirms equivalence.

## Rationale

**Why a trait at all, not a thin wrapper.** The wrapper-versus-trait
choice is decided by whether a second backend is plausible within the
relevant horizon. Four future adapters are named above with concrete
triggers; at least one (`storage_duckdb`) activates as soon as
ADR-017's strangler-fig migration reaches step 8. The wrapper
collapses on the first second adapter; the trait costs the same up
front and absorbs all four without re-litigation.

**Why read + audit-append only, not a full CRUD trait.** The single
most important property the trait must enforce is ADR-013's
"every change is a PR." A trait method named `update_part(...)` is an
attractive nuisance: a future adapter could implement it directly,
the test suite would pass, and the audit invariant would silently
break. By moving mutations to `ProposalSink` (per ADR-019),
`Repository`'s surface stops *being able to express* a direct write.
The PR-pipeline property becomes a structural property of the code,
not a discipline maintained by reviewers reading every adapter.

The audit-append and print-append methods are exceptions because they
are append-only by definition — there is no past state to mutate; an
audit entry or print event is recording an action that already
happened. Append-only also has no in-flight review semantics: an audit
entry recording "operator X verified at time Y" doesn't go through a
PR; it lands as a side effect of the action it records (per ADR-022's
audit-log semantics).

**Why ADR-023's forward-compat columns live in the domain types, not
the adapters.** If `signatures` and `chain_hash` were optional fields
each adapter chose whether to round-trip, ADR-023 §"Schema
forward-compatibility" would be a documentation-only constraint:
correct adapters would honour it, incorrect ones would silently drop
the columns and the loss would only surface when Sigstore is
activated and historical entries are missing signatures. By baking
the columns into the domain types in `crates/domain/`, no adapter
*can* implement `Repository` without round-tripping them — the type
system enforces what the documentation merely asks for.

**Why CSV+git first and only for MVP.** ADR-013 already analysed and
chose CSV+git as the substrate; that decision is in force. This ADR
inherits it. Building a SQLite or DuckDB adapter alongside the CSV+git
one in the same MVP would (a) double the adapter surface for no MVP
benefit, (b) require choosing which is the audit-of-record (a choice
ADR-013 already made for CSV+git), and (c) front-load ADR-027's parity
testing infrastructure for adapters that aren't needed yet. Designing
the trait to accommodate the future adapters costs the same whether
zero or four are built; building the adapters costs work per adapter.

**Why operational against the data repo, not the code repo.** Per
ADR-019, the project splits into a code repo (Rust crates, Python
fallback during strangler-fig, web app source) and a data repo
(CSVs). Storage adapters operate against the data repo; the code
repo is not "storage" in any useful sense (its content is
versioned source code, not registry rows). The CSV+git adapter
takes a path-to-data-repo as its constructor argument, making the
boundary explicit at the type level.

## Consequences

This ADR commits the project to:

- **Trait minimalism**: the `Repository` surface is intentionally
  small and read + audit-append only. Adding a method requires
  showing the alternative (route through `ProposalSink`,
  `IdentityProvider`, etc.) is structurally wrong, not just
  inconvenient. Mutations belong in `ProposalSink` (ADR-019).
- **Domain type ownership of forward-compat columns**: `Part`,
  `AuditEntry`, and `PrintEvent` carry `signatures` and `chain_hash`
  in `crates/domain/`. Adapters cannot opt out. Removing or
  renaming either field is an ADR-level change, not a refactor.
- **CSV header stability**: the CSV+git adapter's column set is
  frozen on the day this ADR is accepted. New columns require a
  schema-migration step (per a successor ADR or per ADR-027's
  schema-evolution test suite). The two new columns introduced by
  ADR-023 (`signatures`, `chain_hash`) land at acceptance and never
  leave.
- **Adapter selection at the boundary, not in domain code**: domain
  crates (`domain/`, `validators/`, `codec/`) never name a concrete
  adapter. Wiring lives in `cli/` (per ADR-017) and reads
  `PARTREG_STORAGE_BACKEND` (per ADR-021).
- **Parity-test discipline (ADR-027)**: any second adapter must
  pass the `port_tests` parity suite vs. CSV+git on a fixed test
  corpus before it is merged. CI rejects PRs that introduce a new
  adapter without a parity-test invocation. The corpus lives in
  the code repo at `port_tests/corpus/storage/`.
- **No direct write methods, ever**: the trait's missing
  `update_part` is load-bearing. Future adapter authors will be
  tempted to add it ("just for `storage_sqlite`'s test fixtures");
  the answer is to use `ProposalSink` for the mutation and let the
  test fixture exercise both in tandem. Adding a write method
  requires superseding this ADR.
- **`storage_csv_git` interoperability with the existing TS FE**:
  ADR-013's FE reads `registry.csv` directly via `fetch()` +
  `papaparse`. That path remains unchanged at MVP; the FE eventually
  swaps to the WASM core's `Repository` (per ADR-017 step 8). Until
  then, the FE and the CSV+git adapter MUST agree on the on-disk
  format byte-for-byte (sort order, line endings, quoting). The
  ADR-027 parity suite includes a "FE-format byte-equivalence"
  case asserted on every PR.
- **`snapshot_hash` becomes citable in releases**: per ADR-024, a
  signed release tag includes the `snapshot_hash` of the data repo
  state at the time of release. The CSV+git adapter implements
  this as the HEAD commit SHA of the data repo's `main`; future
  adapters compute their own deterministic equivalent.

This ADR does **not** commit the project to:

- Building any of the future adapters (`storage_sqlite`,
  `storage_duckdb`, `storage_dolt`, `storage_file_per_entry`). Each
  activates on its own trigger; this ADR only guarantees the trait
  shape will accommodate them.
- An ORM. SQL adapters (sqlite, duckdb, dolt) implement the trait
  directly with `rusqlite` / `duckdb` / `mysql_async` — no Diesel,
  no SeaORM.
- Migrating the FE off its direct CSV fetch path before ADR-017
  step 8 lands. The CSV+git adapter and the FE coexist by sharing
  the on-disk format; the migration happens when the WASM core
  replaces `papaparse`, not before.
- A schema-migration framework. CSV+git has no schema migrations
  in the SQL sense (the column list is the header row); SQL-backed
  future adapters carry their own migration tooling
  (`refinery`, `sqlx::migrate!`, Dolt's native migrations) at the
  time they're built.

## Open questions / supersession triggers

- **Whether `snapshot_hash` should be required or optional on the
  trait.** Today it is required. If a future adapter genuinely
  cannot compute one efficiently, the trait might split into
  `Repository` and `RepositoryWithSnapshot`. No such adapter is
  planned; the trait stays single-tier until one materializes.
- **Whether `list_audit_events` and `list_print_events` should
  return iterators / streams instead of `Vec`.** At sub-million-row
  scale (per ADR-013's sizing), `Vec` is fine. At >10M rows the
  allocator becomes the bottleneck. Re-opens if a future deployment
  pushes through that scale.
- **Whether `storage_duckdb` should be promoted to a co-MVP adapter
  once ADR-017 step 8 lands.** The argument for: ADR-013 already
  named DuckDB-WASM as the FE's query engine; making the WASM core
  use the same engine on the CLI side is symmetric. The argument
  against: it doubles the parity-test surface for an adapter whose
  trigger hasn't fired. Decision deferred to the step-8 PR.
- **Whether the trait should expose a `transaction(...)` method for
  multi-event atomic appends.** CSV+git's "transaction" is a single
  commit; SQL backends have native transactions; file-per-entry has
  no transaction primitive at all. A naive trait method would
  paper over real semantic differences. Re-opens if a workflow
  needs cross-event atomicity that ADR-019's `ProposalSink` (which
  already batches multi-row mutations into a single PR) does not
  provide.
- **Whether `Repository` should be `async`.** Today it is
  synchronous (CSV+git is local-disk I/O, fast enough to block on).
  Future adapters with network I/O (Dolt over MySQL protocol,
  remote SQLite via libSQL) would benefit from `async`. The cost
  of going `async` later is a trait version bump and a wrapper for
  the CSV+git adapter; the cost of going `async` now is dragging
  Tokio into every CLI command. Deferred until the first
  network-I/O adapter is on the roadmap.

## References

- [ADR-013 — Parts registry web app](ADR-013-parts-registry-web-app.md)
  §"Decision" (CSV+git substrate), §"Rationale" (DuckDB-WASM at
  sub-million-row scale, Parquet deferred)
- [ADR-014 — Web app architecture](ADR-014-web-app-architecture.md)
- [ADR-015 — Print event log](ADR-015-print-event-log.md)
  (`print_log.csv` audit trail)
- [ADR-016 — PR-diff policy enforcement](ADR-016-pr-diff-policy-enforcement.md)
- [ADR-017 — Rust core + ports/adapters](ADR-017-rust-core-ports-adapters.md)
  §"Workspace shape" (`crates/storage/`, `crates/storage_csv_git/`),
  §"Strangler-fig migration sequence" step 4
- [ADR-019 — Proposal sink as a port](ADR-019-proposal-sink-port.md)
  (owns mutations; `ProposalSink` trait the missing `update_part`
  routes through)
- [ADR-022 — Observability: tracing + audit trail](ADR-022-observability-tracing-audit.md)
  (`AuditEntry` shape, `Operator` provenance)
- [ADR-023 — Threat model + crypto-MVP scope](ADR-023-threat-model-and-crypto-mvp-scope.md)
  §"MVP crypto scope — fixed" item 6 (forward-compat columns
  `signatures`, `chain_hash`)
- [ADR-024 — Cryptographic baseline (MVP)](ADR-024-crypto-baseline-mvp.md)
  (`snapshot_hash` semantics, signed release tags)
- [ADR-027 — Port conformance + forward-compatibility tests](ADR-027-port-conformance-tests.md)
  (parity-test corpus and discipline)
- ISO 13485:2016 §7.3 — design controls (audit-of-record requirement)
- IEC 62304:2006/AMD1:2015 — software lifecycle (storage as
  configuration item)
- Hexagonal / ports & adapters — Alistair Cockburn, 2005
- Dolt — <https://www.dolthub.com/>
- DuckDB — <https://duckdb.org/>
- libSQL — <https://github.com/tursodatabase/libsql>
