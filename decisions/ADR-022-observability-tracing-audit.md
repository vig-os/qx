# ADR-022 — Observability: tracing + audit trail

- Status: Proposed
- Date: 2026-05-10
- Component / area: cross-cutting — single structured event stream
  for the workspace (CLI, FE-via-WASM, CI), layered subscriber set,
  `AuditEntry` schema written to the data repo, and `request_id`
  propagation tying one user action to its CLI logs, FE telemetry,
  audit-log row, PR, and CI run
- Reviewers: _(pending)_
- Related: ADR-013, ADR-014, ADR-015 (Print event log — generalised
  by this ADR), ADR-016 (emits policy-decision audit events), ADR-017
  (names `crates/observability/`), ADR-018 (`Repository::append_audit_event`),
  ADR-020 (`Operator` stamps every event), ADR-023 (`signatures` and
  `chain_hash` forward-compat), ADR-024 (populates `signatures`),
  ADR-027 (parity tests)

## Context

Today the project's observability is split across two unrelated
mechanisms with no shared shape:

- **`print_log.csv`** (per ADR-015) — a structured, append-only audit
  artifact for one specific action class (label printing). Schema is
  fixed; CI validates header equality, FK to `registry.csv`, sort
  stability. This is the only first-class audit record the project
  has.
- **Ad-hoc `print()` / `print(file=stderr)`** in `label.py`, `mint.py`,
  `bind.py`, `validators/`, and `tools/sheet.py` — unstructured
  diagnostics. No `request_id`, no consistent shape, no machine-
  parseable form, no propagation across process boundaries (CLI →
  FE → CI). Useful for one developer at one terminal; useless for an
  auditor reconstructing what happened.

Five pressures collide on this gap:

1. **ADR-017's strangler-fig centralizes everything in Rust.** Python
   `print()` calls and the `print_log.csv` writer are both being
   replaced by Rust crates. Picking the shape *now* avoids re-
   litigating it inside each migration PR.
2. **ADR-016 expands the audit surface beyond printing.** Bind, edit,
   void, delete, propose, merge are each state-changing actions
   that need the same provenance fidelity `print_log.csv` gives
   printing today. One-CSV-per-action does not scale.
3. **ADR-020 puts a typed `Operator` on every mutation.** That value
   has nowhere to go in the current `print()`-based world.
4. **ADR-023's forward-compat columns must land on day one.**
   `signatures` and `chain_hash` reserved at acceptance; migrating
   later is a breaking change to historical rows we committed to
   avoiding.
5. **End-to-end traceability requires `request_id` propagation.** One
   user action produces FE telemetry, an audit-log append, a PR, CI
   runs, and policy decisions. An auditor asking "show me everything
   for batch X" must grep one ID and get every artifact.

ADR-017 names `crates/observability/` as the home for tracing setup,
audit-log subscriber, and request-ID propagation. This ADR fills in
*what those subscribers are* and *what schema the audit-log
subscriber writes*.

The audit subset lands in the data repo via `Repository` (ADR-018), so
it inherits ADR-013's "data = git history" property automatically.
Diagnostic logs (stdout JSON, stderr human) are separate ephemeral
streams; only the audit subset is durable.

## Alternatives considered

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| **Status quo: ad-hoc `print(file=stderr)` + per-feature CSVs (`print_log.csv` only)** | Works for the one feature that exists today (printing); zero up-front infrastructure cost | Unstructured strings everywhere else; no `request_id`; no consistent shape across CLI/FE/CI; one-CSV-per-action does not scale to bind/edit/void/delete/propose/merge; drift between Python `print()` and Rust CLI inevitable during migration | Rejected — already missing every property the broader audit surface (ADR-016, ADR-020, ADR-023) requires |
| **`log` + `env_logger`** (Rust stdlib-style logging) | Familiar; zero crate-graph cost beyond `log` itself; no learning curve | Unstructured strings; no spans (cannot attach `request_id` once and have it appear on every emit inside a span); no per-event metadata beyond a level + a string; no fan-out to multiple subscribers (one global logger); no path to OTLP export | Rejected — solves nothing the status quo doesn't already half-solve |
| **`tracing` ecosystem with layered subscribers (stdout JSON, stderr human, audit-CSV, future OTLP)** | Structured spans and fields; span context propagates `request_id` automatically to every emit inside the span; layered subscribers fan out one emit to many destinations with different filtering; ecosystem-standard (used by the entire Tokio / axum / sqlx world); the `tracing-opentelemetry` layer slots in later without changing call sites | Learning curve for the `tracing` macro family (`#[instrument]`, `info_span!`, `event!`); subscriber composition needs a single setup point | **Chosen** — only option that simultaneously gives structured emits, span propagation, multi-subscriber fan-out, and an OTLP forward path |
| **OpenTelemetry-only (drop `tracing`, use `opentelemetry-rust` directly)** | Full distributed tracing on day one; OTLP export native; vendor-neutral telemetry pipeline | Heavier than MVP needs (the project does not yet have multi-service traces to export); collapses the audit-CSV layer concept into a "span exporter that happens to write CSV" which is awkward; ergonomics worse than `tracing` for in-process spans; `tracing-opentelemetry` already adapts `tracing` to OTLP, so this option is "skip the layer that gives us local logging too" | Rejected as MVP — preserved as a future *layer* on top of `tracing`, not a replacement for it |

## Decision

The Rust workspace (per ADR-017) emits one structured event stream
from a single tracing infrastructure in `crates/observability/`. The
infrastructure is built on the `tracing` crate ecosystem
(`tracing`, `tracing-subscriber`) and exposes one initialization
function the `cli/` binaries and the `wasm` façade both call at
startup.

The same emit point feeds multiple subscribers, layered:

- **stdout JSON layer** — every `tracing` event serialized as one
  JSON line on stdout. Consumed by ops/logging infrastructure
  (`jq`, log aggregators) when one exists. Filterable by level via
  config.
- **stderr human layer** — the same events rendered as colourised
  human-readable lines on stderr, for dev/CLI ergonomics. Same
  filter level as stdout JSON by default; overridable.
- **audit-CSV layer** — a *durable subset* of events: those tagged
  with the `audit = true` field. The layer routes those events to
  `Repository::append_audit_event(...)` (per ADR-018), which writes
  to `audit_log.csv` in the data repo. Append-only, sort-stable for
  git diff readability, byte-equivalent on re-sort (same invariant
  as `registry.csv` per ADR-013 and `print_log.csv` per ADR-015).
- **OTLP layer** — *future, not MVP*. Slots in via
  `tracing-opentelemetry` when distributed deployment with
  multi-service traces becomes load-bearing. The layer composition
  shape is what makes this addition cost a few lines of setup code,
  not a refactor.

### `AuditEntry` shape

The type lives in `crates/domain/` (per ADR-017) because it is
consumed by `Repository::append_audit_event` (ADR-018), the policy
engine (ADR-016), and the identity port's audit-stamping path
(ADR-020). The shape is forward-compatible per ADR-023 from day one.

```rust
// crates/domain/src/audit.rs

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use time::OffsetDateTime;

use crate::identity::Operator;
use crate::signing::{Signature, Hash};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuditEntry {
    pub request_id: RequestId,
    pub timestamp: OffsetDateTime,
    pub actor: Operator,                    // ADR-020
    pub action: ActionKind,
    pub target: TargetRef,
    pub before: Option<Json>,
    pub after: Option<Json>,
    pub extra: Json,                        // action-specific
    pub signatures: Vec<Signature>,         // ADR-023 forward-compat; ADR-024 populates
    pub chain_hash: Option<Hash>,           // ADR-023 forward-compat; deferred trigger T2
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct RequestId(pub uuid::Uuid);       // UUIDv7 for time-ordered sortability

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    Mint,
    Bind,
    Edit,
    Void,
    Delete,
    Print,
    Propose,
    Merge,
    PolicyDecision,                          // ADR-016 emits these
    IdentityVerify,                          // ADR-020 emits these
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TargetRef {
    PartId(crate::PartId),
    BatchLabel(String),
    Diff { sha: String },
    ProposalRef(String),
    None,
}
```

### CSV serialization

The audit-CSV layer writes one row per `AuditEntry`. Column order is
normative; CI checks the header. JSON-typed columns (`actor`,
`target`, `before`, `after`, `extra`, `signatures`) are serialized as
compact JSON strings with sorted keys (`serde_json` with the
`preserve_order` feature off and a stable serializer wrapper), so
re-serialization is byte-equivalent.

```
request_id,timestamp,actor,action,target,before,after,extra,signatures,chain_hash
```

| Column | Type / domain | Notes |
|---|---|---|
| `request_id` | UUIDv7 string | Propagation key; see §"request_id propagation" below |
| `timestamp` | ISO-8601 UTC, second precision (`Z` suffix) | Sort key (ascending); ties broken by `request_id` |
| `actor` | JSON object (sorted keys) | Full `Operator` per ADR-020: `{id, display_name, source, verified_at, claims, pubkey}` |
| `action` | snake_case enum string | `mint`, `bind`, `edit`, `void`, `delete`, `print`, `propose`, `merge`, `policy_decision`, `identity_verify` |
| `target` | JSON object (sorted keys) | Tagged union: `{"kind":"part_id","value":"..."}`, etc. |
| `before` | JSON or empty | Pre-state for state-changing actions; empty for create-only |
| `after` | JSON or empty | Post-state; empty for delete-only |
| `extra` | JSON object | Action-specific payload (e.g. for `print`: `{layout, size_mm, copies, output_mode, batch_label}`) |
| `signatures` | JSON array | ADR-023 forward-compat; MVP populates with one `{"kind":"git_commit","sha":"..."}` per ADR-024 |
| `chain_hash` | hex string or empty | ADR-023 forward-compat; MVP empty until ADR-023 trigger T2 fires |

**Sort rule:** `audit_log.csv` is sorted by `timestamp` ascending,
then by `request_id` for ties. Re-sorting equals the file
byte-for-byte (same invariant as `registry.csv` per ADR-013 and
`print_log.csv` per ADR-015). Validators (per ADR-016) reject PRs
whose audit-log diff is not sort-stable.

### `request_id` propagation

Every emit carries a `request_id` via `tracing` span context. The ID
is generated at the *outermost* boundary of one logical user action
and propagates through every nested call without manual threading.

| Surface | When generated | How it propagates |
|---|---|---|
| **CLI** | Process start, before business logic | Root `info_span!("cli", request_id = ...)`; all inner emits inherit |
| **FE** | Each user-action root (click, scan, open-proposal) | Attached to the proposal payload (ADR-019) and FE telemetry; travels through to PR and CI |
| **PR** | Re-used from proposal payload | Embedded in PR body as `Request-Id: <uuid>` |
| **CI** | Re-used from PR body | Root `info_span!("ci", request_id = ...)`; policy/validator/conformance emits inherit |

**Identifier choice:** UUIDv7. Time-ordered, 128-bit, no central
allocator, fits cleanly in CSV cells. ULID was the runner-up;
rejected on Rust ecosystem inertia.

An auditor reconstructing one logical action runs:

```
grep "request_id=<uuid>" audit_log.csv cli-stdout.jsonl ci-logs/*.jsonl
```

### Tracing setup

```rust
// crates/observability/src/lib.rs
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub struct ObservabilityConfig {
    pub log_level: tracing::Level,
    pub stdout_json: bool,
    pub stderr_human: bool,
    pub audit_csv: bool,
}

pub fn init(cfg: &ObservabilityConfig, repo: RepositoryHandle)
    -> Result<(), InitError>
{
    let registry = tracing_subscriber::registry()
        .with(cfg.stdout_json.then(|| json_stdout_layer(cfg.log_level)))
        .with(cfg.stderr_human.then(|| human_stderr_layer(cfg.log_level)))
        .with(cfg.audit_csv.then(|| audit_csv_layer(repo)));
    // future: .with(otlp_layer(cfg))
    registry.try_init()?;
    Ok(())
}
```

`audit_csv_layer` is the bridge: it filters events tagged
`audit = true`, builds an `AuditEntry` from the event's fields plus
the active span's `request_id` and the active `Operator` (resolved
from a thread-local set by the identity port), and calls
`Repository::append_audit_event(entry)`. A failure to append is
logged to stderr and counted but does not fail the originating
operation — the operation already happened in physical reality;
refusing to record it makes the audit log *less* accurate, not more.

Emitting an audit event from business code is one macro:

```rust
tracing::info!(
    audit = true,
    action = "bind",
    target_kind = "part_id",
    target_value = %part_id,
    before = %serde_json::to_string(&before)?,
    after = %serde_json::to_string(&after)?,
    "bound part {part_id} to {batch_label}"
);
```

The same call produces:
- a JSON line on stdout (full event, including `request_id` from the
  span)
- a colourised human line on stderr
- an `AuditEntry` row appended to `audit_log.csv`

## Rationale

**Why one stream feeding many subscribers.** A bind is one event with
one timestamp and one `Operator`. Three independent APIs (stderr
formatter, JSON serializer, audit-CSV writer) means three places to
add a field and three places for a bug. The `tracing` layered-
subscriber pattern emits once and fans out; representations agree
because they derive from one event.

**Why `tracing` over `log`.** Spans propagate context (`request_id`,
`operator_id`) to every nested emit without manual threading. With
`log`, every callee would have to take a `request_id` parameter or
the project would invent a thread-local context that re-implements
`tracing`'s span machinery badly.

**Why audit goes through `Repository`, not a separate file handle.**
ADR-018 makes `Repository` the only writer to the data repo. A
separate file handle for `audit_log.csv` would create a second writer
that has to coordinate with git operations (commit boundaries, lock
files, sort-stability rewrites). Routing through
`Repository::append_audit_event` keeps the one-writer property and
lets future storage adapters (SQLite, DuckDB, Dolt) automatically own
the audit log too without extra work in `crates/observability/`.

**Why `request_id` at the user-action boundary.** The unit of
correlation is the user action, not the process or function call. A
CLI that runs `mint && bind && label` in one shell line is one
`request_id` (one user intention); three separate commands are three.
The boundary is the outermost API call from the user — the click in
the FE, the process start in the CLI, the PR body in CI.

**Why UUIDv7 over ULID.** Both are time-ordered 128-bit IDs with no
central allocator. UUIDv7 has broader Rust ecosystem support (`uuid`
crate already in most dependency trees), native column types in DuckDB
and SQLite, and a published RFC (9562). ULID's text encoding is a few
chars shorter; not load-bearing.

**Why the audit-CSV layer is opt-in via `audit = true`.** Most
`tracing` events are diagnostics that should not enter the durable
audit log. Opt-in forces the call site to *decide* "is this one of
the things that should appear in the audit log forever?" Opt-out lets
every diagnostic accidentally pollute the audit log.

**Why ADR-023's forward-compat columns are mandatory at MVP.**
Adding columns later means either a one-time migration of historical
rows (explicitly committed to avoiding in ADR-018 §Consequences) or a
heterogeneous file that breaks every parser. Reserving them now costs
nothing; activating Sigstore (ADR-023 trigger T2) becomes a
population change, not a schema change.

## Consequences

This ADR commits the project to:

- **One init point per process.** `cli/` binaries and the `wasm`
  façade call `observability::init(...)` exactly once at startup.
  Multiple inits are a runtime error.
- **The audit-CSV layer requires a `Repository` handle.** Read-only
  processes set `audit_csv: false`; mutating processes (`bind`,
  `mint`, `label`, FE proposal-sink) MUST enable it. Wiring in
  `cli/` enforces this per-binary.
- **`tracing` is a transitive dependency of every emitting crate.**
  Macros are zero-cost when no subscriber is attached.
- **The audit log's column set is frozen on day one.** Adding a
  column is an ADR-level change, consistent with ADR-018 §"CSV
  header stability."
- **`request_id` propagates to the PR body, CI logs, and every audit
  row.** PR-pipeline tooling (per ADR-016) parses `Request-Id:` from
  the PR body. PRs without one are accepted but flagged as "audit
  chain incomplete" — a nudge, not a block.
- **Conformance-test discipline (ADR-027).** Any second storage
  adapter must round-trip `signatures` + `chain_hash` and reproduce
  sort-stable output on the parity corpus at
  `port_tests/corpus/observability/`.
- **Stdout JSON is the machine-parseable contract.** Stderr human
  stream is *not* a stable contract.

This ADR does **not** commit the project to:

- An OTLP exporter at MVP. The layer composition preserves the
  option; pulling the trigger is a separate ADR.
- Log aggregation infrastructure (Loki, ELK, Datadog).
- Per-event signing — the `signatures` column is reserved;
  population is ADR-024's job.
- A typed query API over the audit log. Auditors use `grep` or
  load it into the FE's DuckDB-WASM session.
- An in-memory event bus beyond the `tracing` subscriber chain.

### Migration of `print_log.csv`

ADR-015's `print_log.csv` is the only structured audit artifact
today. This ADR generalises the substrate to a single
`audit_log.csv` keyed by `action`. Two paths considered:

| Option | Pros | Cons |
|---|---|---|
| **(a) Keep `print_log.csv` as a specialized slice the audit-CSV layer ALSO writes** | Easier short-term; existing consumers keep working | Double-write; two files to keep in sync; drift surface auditors cannot tolerate |
| **(b) Deprecate `print_log.csv` in favour of filtering `audit_log.csv` where `action=print`** | One source of truth; one writer per ADR-018; consistent audit story | One-time back-fill of historical rows; FE row-detail must repoint |

**Decision: option (b), deprecate `print_log.csv`.** The double-write
of (a) violates ADR-018's one-writer property.

**Migration path:**

1. The strangler-fig step landing `crates/observability/`'s audit-CSV
   layer (after `storage_csv_git`) creates `audit_log.csv`.
2. A one-time script back-fills existing `print_log.csv` rows as
   `AuditEntry` with `action = print`, `request_id` synthesised as a
   deterministic UUIDv5 over `(printed_at, id)` (idempotent), `actor`
   from `printed_by` with `source: GitConfig`, `extra` carrying
   `{layout, size_mm, copies, output_mode, batch_label}`.
3. New `cli/label` writes only to `audit_log.csv`.
4. `print_log.csv` retained read-only for one release cycle.
5. `print_log.csv` deleted in a dedicated PR. History lives in
   `audit_log.csv` and git.

ADR-015 invariants (FK to `registry.csv`, sort stability, header
equality) carry over for `action=print` rows, enforced by the same
validator pipeline (per ADR-016) — filter-by-action over
`audit_log.csv` instead of a dedicated reader.

## Open questions / supersession triggers

- **Whether `audit_log.csv` should be partitioned by year** once it
  grows past diff-friendly size. Estimate ~10³–10⁴ events/year,
  ~1 MB/year. Re-opens at ~10 MB total or when PR diffs become
  painful.
- **Whether the OTLP layer should be added before deployment becomes
  multi-service.** Deferred until a concrete service-to-service
  trace requirement exists.
- **Whether `chain_hash` activation (ADR-023 trigger T2) belongs in
  this ADR's successor or ADR-024's successor.** Decision deferred
  to whichever ADR triggers first.
- **Whether the audit-CSV layer should fail-closed on append
  errors.** Today it fails open (logs to stderr, lets the operation
  proceed) — the operation already happened in physical reality.
  Re-opens if fail-closed becomes a regulatory requirement.
- **Whether `request_id` should appear in user-facing error
  messages.** Quote-back argument vs. privacy. Re-opens at FE
  polish time.
- **Whether JSON columns should be JSON-typed natively in future SQL
  adapters.** Implementation detail of each adapter; trait shape
  unchanged.

## References

- [ADR-013 — Parts registry web app](ADR-013-parts-registry-web-app.md) §"Decision"
- [ADR-014 — Web app architecture](ADR-014-web-app-architecture.md)
- [ADR-015 — Print event log](ADR-015-print-event-log.md) (schema this ADR generalises)
- [ADR-016 — PR-diff policy enforcement](ADR-016-pr-diff-policy-enforcement.md) (Request-Id in PR body)
- [ADR-017 — Rust core + ports/adapters](ADR-017-rust-core-ports-adapters.md) §"Workspace shape"
- [ADR-018 — Storage as a port](ADR-018-storage-port.md) (`Repository::append_audit_event`)
- [ADR-020 — Identity & authorization as a port](ADR-020-identity-authorization-port.md) (`Operator`)
- [ADR-023 — Threat model + crypto-MVP scope](ADR-023-threat-model-and-crypto-mvp-scope.md) §"Schema forward-compatibility"
- [ADR-024 — Cryptographic baseline (MVP)](ADR-024-crypto-baseline-mvp.md) (`signatures` population)
- [ADR-027 — Port conformance tests](ADR-027-port-conformance-tests.md)
- ISO 13485:2016 §7.5.9 — control of records (audit trail
  permanence)
- IEC 62304:2006/AMD1:2015 §5.8 — software release records
- `tracing` — <https://docs.rs/tracing>
- `tracing-subscriber` — <https://docs.rs/tracing-subscriber>
- `tracing-opentelemetry` — <https://docs.rs/tracing-opentelemetry>
- RFC 9562 — UUID Version 7 — <https://www.rfc-editor.org/rfc/rfc9562>
- OpenTelemetry — <https://opentelemetry.io/>
