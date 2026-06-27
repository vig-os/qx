//! `qx-observability` — `tracing` setup + audit-CSV
//! subscriber + `request_id` propagation per ADR-022.
//!
//! ## Shape (ADR-022 §"Tracing setup")
//!
//! One `init(...)` call from each CLI binary and the WASM façade.
//! The function builds a layered [`tracing_subscriber::Registry`] with
//! up to three layers:
//!
//! - **stdout JSON layer** (gated by [`ObservabilityConfig::stdout_json`])
//!   — every event serialised as one JSON line on stdout. Consumed by
//!   ops tooling (`jq`, log aggregators). Machine-parseable contract.
//! - **stderr human layer** (gated by [`ObservabilityConfig::stderr_human`])
//!   — same events rendered colourised for dev/CLI ergonomics. Not a
//!   stable contract.
//! - **audit-CSV layer** (gated by [`ObservabilityConfig::audit_csv`])
//!   — durable subset of events tagged `audit = true`. Routes the
//!   reconstructed [`AuditEntry`] to a [`Repository::append_audit_event`]
//!   handle injected at init time.
//!
//! ## `request_id` propagation (ADR-022 §"request_id propagation")
//!
//! UUIDv7 per RFC 9562 — time-ordered 128-bit identifier generated at
//! the outermost user-action boundary (CLI process start, FE click,
//! CI run open). Propagated through every nested `tracing` span via
//! span context so any emit inside the user-action root span inherits
//! it without manual threading.
//!
//! Use [`RequestId::new`] to mint one and [`request_id_span`] to open
//! the root span at the binary's entry point:
//!
//! ```no_run
//! # use qx_observability::{init, ObservabilityConfig, AuditSinkHandle, request_id_span};
//! # use qx_domain::RequestId;
//! let cfg = ObservabilityConfig::cli_defaults();
//! let _ = init(&cfg, AuditSinkHandle::disabled());
//! let rid = RequestId::new();
//! let _root = request_id_span("cli.mint", rid);
//! let _guard = _root.enter();
//! // … business code …
//! ```
//!
//! ## Audit-CSV layer & the `audit = true` convention
//!
//! Per ADR-022 §"audit-CSV layer is opt-in via `audit = true`": only
//! events that carry the field `audit = true` reach the audit log.
//! Use [`emit_audit`] for the structured form (recommended); the layer
//! falls back to reconstructing the [`AuditEntry`] from discrete event
//! fields when the structured payload is absent, so the macro form
//! from ADR-022 §"Emitting an audit event from business code" remains
//! supported.
//!
//! ## Forward-compat (ADR-023 / ADR-027 Tier 2)
//!
//! The audit-CSV writer round-trips `signatures` and `chain_hash`
//! columns even though MVP code does not populate them. Sigstore
//! activation (ADR-023 trigger T2) is therefore a population change,
//! not a schema change.

#![forbid(unsafe_code)]

use std::sync::{Arc, Mutex, OnceLock};

use thiserror::Error;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id};
use tracing::{Event, Span, Subscriber};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use qx_domain::{Action, AuditEntry, Hash, Operator, OperatorId, RequestId, TargetRef, Timestamp};
use qx_storage::{RepoError, Repository};

// -------------------------------------------------------------------
// Errors
// -------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum InitError {
    #[error("tracing subscriber already initialised: {0}")]
    AlreadyInit(String),
    #[error("invalid log level filter {value:?}: {source}")]
    BadLogLevel {
        value: String,
        #[source]
        source: tracing_subscriber::filter::ParseError,
    },
    #[error("audit-CSV layer requested but no Repository handle provided")]
    MissingAuditSink,
}

// -------------------------------------------------------------------
// Config — re-exported from the config crate (single source of truth)
// -------------------------------------------------------------------

/// Re-exported from [`qx_config::ObservabilityConfig`] so
/// consumers that already depend on this crate don't need a separate
/// `qx_config` import. The config crate owns the shape;
/// this crate consumes it. See ADR-021 §Corrections and issue #38.
pub use qx_config::ObservabilityConfig;

// -------------------------------------------------------------------
// AuditSinkHandle
// -------------------------------------------------------------------

/// Thread-safe handle to a [`Repository`] used by the audit-CSV layer.
///
/// Wrapped in `Arc<Mutex<...>>` so the layer can be safely registered
/// on a `Send + Sync` subscriber while sharing one writer across span
/// emits from multiple threads. Per ADR-018 there is exactly one
/// writer to the data repo; this handle is that writer.
///
/// Construct via [`AuditSinkHandle::new`] passing any `Repository`
/// (the `Box<dyn Repository>` lets adapters be swapped per
/// ADR-017 / ADR-021). For read-only processes that should never emit
/// audit rows, use [`AuditSinkHandle::disabled`].
#[derive(Clone)]
pub struct AuditSinkHandle {
    inner: Option<Arc<Mutex<Box<dyn Repository>>>>,
}

impl std::fmt::Debug for AuditSinkHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuditSinkHandle")
            .field("enabled", &self.inner.is_some())
            .finish()
    }
}

impl AuditSinkHandle {
    pub fn new(repo: Box<dyn Repository>) -> Self {
        Self {
            inner: Some(Arc::new(Mutex::new(repo))),
        }
    }

    /// Sentinel used by processes that do not write to the audit log.
    /// Passing this with `audit_csv: true` in config returns
    /// [`InitError::MissingAuditSink`] from [`init`].
    pub fn disabled() -> Self {
        Self { inner: None }
    }

    pub fn is_enabled(&self) -> bool {
        self.inner.is_some()
    }

    fn append(&self, entry: AuditEntry) -> Result<(), RepoError> {
        let inner = self
            .inner
            .as_ref()
            .ok_or_else(|| RepoError::Other("audit sink disabled".into()))?;
        let guard = inner
            .lock()
            .map_err(|e| RepoError::Backend(format!("audit sink mutex poisoned: {e}").into()))?;
        guard.append_audit_event(entry)
    }
}

// -------------------------------------------------------------------
// Operator thread-local (ADR-022 §"audit_csv_layer ... active Operator
// resolved from a thread-local set by the identity port")
// -------------------------------------------------------------------

thread_local! {
    static CURRENT_OPERATOR: std::cell::RefCell<Option<Operator>> = const { std::cell::RefCell::new(None) };
}

/// Bind the active [`Operator`] for the current thread.
///
/// Per ADR-022 §"audit_csv_layer is the bridge": the layer reads the
/// active `Operator` from a thread-local. The identity port (ADR-020)
/// sets it before any audit-emitting code runs.
///
/// **Prefer [`OperatorGuard`]** for scoped usage — it clears the slot
/// automatically on drop, even if the guarded code panics.
pub fn set_current_operator(op: Operator) {
    CURRENT_OPERATOR.with(|slot| *slot.borrow_mut() = Some(op));
}

/// Clear the active operator (test convenience + identity port teardown).
///
/// **Prefer [`OperatorGuard`]** for scoped usage — it clears the slot
/// automatically on drop, even if the guarded code panics.
pub fn clear_current_operator() {
    CURRENT_OPERATOR.with(|slot| *slot.borrow_mut() = None);
}

/// RAII guard that sets the thread-local [`Operator`] on construction
/// and clears it on drop — including during stack unwinding (panics).
///
/// ```no_run
/// # use qx_observability::OperatorGuard;
/// # use qx_domain::{Operator, OperatorId, IdentitySource};
/// let op = Operator {
///     id: OperatorId("git:user".into()),
///     display_name: "user".into(),
///     source: IdentitySource::GitConfig,
///     verified_at: None,
///     claims: Default::default(),
///     pubkey: None,
/// };
/// let _guard = OperatorGuard::new(op);
/// // … business code — operator is active here …
/// // guard dropped here (or on panic) → slot cleared
/// ```
pub struct OperatorGuard(());

impl OperatorGuard {
    /// Set the thread-local operator and return a guard that will clear
    /// it when dropped.
    pub fn new(op: Operator) -> Self {
        set_current_operator(op);
        Self(())
    }
}

impl Drop for OperatorGuard {
    fn drop(&mut self) {
        clear_current_operator();
    }
}

/// Snapshot of the active operator, if any.
pub fn current_operator() -> Option<Operator> {
    CURRENT_OPERATOR.with(|slot| slot.borrow().clone())
}

// -------------------------------------------------------------------
// init
// -------------------------------------------------------------------

/// One-shot init for the global tracing subscriber.
///
/// Builds the layered registry described in the module docs and
/// installs it with [`SubscriberInitExt::try_init`]. Calling more than
/// once returns [`InitError::AlreadyInit`].
pub fn init(cfg: &ObservabilityConfig, audit_sink: AuditSinkHandle) -> Result<(), InitError> {
    if cfg.audit_csv && !audit_sink.is_enabled() {
        return Err(InitError::MissingAuditSink);
    }

    let env_filter = build_env_filter(&cfg.log_level)?;

    let json_layer = cfg.stdout_json.then(|| {
        tracing_subscriber::fmt::layer()
            .json()
            .with_writer(std::io::stdout)
            .with_target(true)
            .with_current_span(true)
            .with_span_list(true)
            .boxed()
    });

    let human_layer = cfg.stderr_human.then(|| {
        tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .with_target(false)
            .with_ansi(false) // colour off by default for CI/log capture cleanliness
            .boxed()
    });

    let audit_layer = if cfg.audit_csv {
        Some(AuditCsvLayer::new(audit_sink).boxed())
    } else {
        None
    };

    let registry = tracing_subscriber::registry()
        .with(env_filter)
        .with(json_layer)
        .with(human_layer)
        .with(audit_layer);

    registry
        .try_init()
        .map_err(|e| InitError::AlreadyInit(e.to_string()))
}

fn build_env_filter(level: &str) -> Result<EnvFilter, InitError> {
    EnvFilter::try_new(level).map_err(|e| InitError::BadLogLevel {
        value: level.into(),
        source: e,
    })
}

// -------------------------------------------------------------------
// Root span helpers
// -------------------------------------------------------------------

/// Open the user-action root span carrying a [`RequestId`].
///
/// Per ADR-022: every audit-relevant emit happens inside this span so
/// the `request_id` propagates automatically via span context. The
/// audit-CSV layer reads the id back out of the active span.
///
/// `name` is the static span name (e.g. `"cli.mint"`, `"cli.bind"`,
/// `"ci.policy_check"`). The id appears as a span field
/// `request_id = "<uuid>"`.
#[macro_export]
macro_rules! request_id_span {
    ($name:expr, $rid:expr) => {{
        ::tracing::info_span!($name, request_id = %$rid)
    }};
}

/// Builder form of [`request_id_span!`] for runtime-chosen span names.
///
/// Returns the span; the caller is responsible for entering it.
pub fn request_id_span(name: &'static str, rid: RequestId) -> Span {
    tracing::info_span!(target: "qx::observability", "request", name = name, request_id = %rid)
}

// -------------------------------------------------------------------
// Audit event emission
// -------------------------------------------------------------------

/// Emit a structured audit event from business code.
///
/// Per ADR-022 §"Emitting an audit event from business code": one
/// macro call produces a JSON line on stdout, a human line on stderr,
/// and an `AuditEntry` row in `audit_log.csv`. The `request_id` is
/// inherited from the active span; the `actor` from the thread-local
/// operator set by the identity port.
///
/// This function consumes a pre-built [`AuditEntry`] so call sites that
/// want maximal type-safety pass one. Lower-friction callers can use
/// `tracing::info!` directly with `audit = true` and discrete fields;
/// the audit-CSV layer reconstructs the [`AuditEntry`] from those
/// fields when no full payload is provided.
pub fn emit_audit(entry: &AuditEntry) {
    // Pre-serialise the full payload so the audit-CSV layer can ingest
    // it without rebuilding from discrete fields (the structured path).
    // We still emit discrete fields for the JSON/human layer readers.
    let payload = match serde_json::to_string(entry) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "emit_audit: failed to serialise AuditEntry");
            return;
        }
    };
    tracing::info!(
        target: "qx::audit",
        audit = true,
        audit_entry = %payload,
        action = %action_kind_str(&entry.action),
        target_kind = %target_kind_str(&entry.target),
        actor_id = %entry.actor.id,
        request_id = %entry.request_id,
        "audit"
    );
}

fn action_kind_str(action: &Action) -> &'static str {
    match action.kind() {
        qx_domain::ActionKind::RowAdd => "row_add",
        qx_domain::ActionKind::RowDelete => "row_delete",
        qx_domain::ActionKind::RowVoid => "row_void",
        qx_domain::ActionKind::RowBind => "row_bind",
        qx_domain::ActionKind::RowEdit => "row_edit",
        qx_domain::ActionKind::HeaderChange => "header_change",
        qx_domain::ActionKind::BulkChange => "bulk_change",
        qx_domain::ActionKind::RecordWrite => "record_write",
    }
}

fn target_kind_str(t: &TargetRef) -> &'static str {
    match t {
        TargetRef::Part { .. } => "part",
        TargetRef::Batch { .. } => "batch",
        TargetRef::Diff { .. } => "diff",
        TargetRef::File { .. } => "file",
        TargetRef::Record { .. } => "record",
    }
}

// -------------------------------------------------------------------
// AuditCsvLayer
// -------------------------------------------------------------------
//
// Filters events tagged `audit = true`, reconstructs an `AuditEntry`
// (either from a pre-serialised `audit_entry` field or from discrete
// `action`/`target`/... fields plus the active span's `request_id` and
// the thread-local `Operator`), and calls
// `Repository::append_audit_event`.
//
// A failure to append is logged via the global tracing error sink but
// does not panic — ADR-022 §"audit_csv_layer ... fails open": the
// operation already happened in physical reality; refusing to record
// it makes the audit log less accurate, not more.

struct AuditCsvLayer {
    sink: AuditSinkHandle,
    /// Counter of failed appends — exposed for tests and ops.
    failed_appends: Arc<Mutex<u64>>,
}

impl AuditCsvLayer {
    fn new(sink: AuditSinkHandle) -> Self {
        Self {
            sink,
            failed_appends: Arc::new(Mutex::new(0)),
        }
    }
}

/// Stores the request_id on every span as part of its extensions so
/// child events can read it without walking the parent chain field
/// list manually. Set in `on_new_span`.
#[derive(Clone, Debug)]
struct SpanRequestId(RequestId);

impl<S> Layer<S> for AuditCsvLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        // Extract request_id from the span's fields, if present.
        let mut v = RequestIdFieldVisitor::default();
        attrs.record(&mut v);
        if let Some(rid) = v.request_id {
            if let Some(span) = ctx.span(id) {
                span.extensions_mut().insert(SpanRequestId(rid));
            }
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        // Cheap pre-check: is this event tagged `audit = true`?
        let mut tag = AuditTagVisitor::default();
        event.record(&mut tag);
        if !tag.audit {
            return;
        }

        // Two paths: full pre-built payload, or discrete fields.
        let entry = if let Some(payload) = tag.audit_entry {
            match serde_json::from_str::<AuditEntry>(&payload) {
                Ok(e) => e,
                Err(err) => {
                    self.bump_failed();
                    // Stderr by design: this layer sits INSIDE the
                    // tracing pipeline — reporting via tracing here
                    // would recurse (ADR-022 fail-open).
                    eprintln!("audit-csv layer: failed to deserialise audit_entry payload: {err}"); // guardrails-ok
                    return;
                }
            }
        } else {
            match reconstruct_from_discrete(event, &ctx) {
                Some(e) => e,
                None => {
                    self.bump_failed();
                    // Stderr by design — see above (in-pipeline, no tracing).
                    eprintln!( // guardrails-ok
                        "audit-csv layer: event tagged audit=true but lacks audit_entry payload and discrete fields"
                    );
                    return;
                }
            }
        };

        if let Err(err) = self.sink.append(entry) {
            self.bump_failed();
            // Stderr by design — see above (in-pipeline, no tracing).
            eprintln!("audit-csv layer: append_audit_event failed: {err}"); // guardrails-ok
        }
    }
}

impl AuditCsvLayer {
    fn bump_failed(&self) {
        if let Ok(mut g) = self.failed_appends.lock() {
            *g += 1;
        }
    }
}

// -------------------------------------------------------------------
// Visitors
// -------------------------------------------------------------------

#[derive(Default)]
struct RequestIdFieldVisitor {
    request_id: Option<RequestId>,
}

impl Visit for RequestIdFieldVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "request_id" {
            if let Ok(uuid) = value.parse::<uuid::Uuid>() {
                self.request_id = Some(RequestId(uuid));
            }
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        // `info_span!(... request_id = %rid)` arrives here via the
        // Display-vs-Debug routing of `tracing`. Parse defensively.
        if field.name() == "request_id" {
            let s = format!("{value:?}");
            // strip optional surrounding quotes
            let trimmed = s.trim_matches('"');
            if let Ok(uuid) = trimmed.parse::<uuid::Uuid>() {
                self.request_id = Some(RequestId(uuid));
            }
        }
    }
}

#[derive(Default)]
struct AuditTagVisitor {
    audit: bool,
    audit_entry: Option<String>,
    // Discrete-fallback fields (ADR-022 macro form).
    action: Option<String>,
    target_kind: Option<String>,
    target_value: Option<String>,
    before: Option<String>,
    after: Option<String>,
    extra: Option<String>,
    request_id: Option<RequestId>,
    actor_json: Option<String>,
}

impl Visit for AuditTagVisitor {
    fn record_bool(&mut self, field: &Field, value: bool) {
        if field.name() == "audit" {
            self.audit = value;
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.set_string_field(field.name(), value);
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        // `tracing::info!(... audit_entry = %s)` arrives via record_debug too
        // depending on the implementation; capture defensively.
        let s = format!("{value:?}");
        let trimmed = s.trim_matches('"').to_string();
        self.set_string_field(field.name(), &trimmed);
    }
}

impl AuditTagVisitor {
    fn set_string_field(&mut self, name: &str, value: &str) {
        match name {
            "audit_entry" => self.audit_entry = Some(value.to_owned()),
            "action" => self.action = Some(value.to_owned()),
            "target_kind" => self.target_kind = Some(value.to_owned()),
            "target_value" => self.target_value = Some(value.to_owned()),
            "before" => self.before = Some(value.to_owned()),
            "after" => self.after = Some(value.to_owned()),
            "extra" => self.extra = Some(value.to_owned()),
            "actor" => self.actor_json = Some(value.to_owned()),
            "request_id" => {
                if let Ok(u) = value.parse::<uuid::Uuid>() {
                    self.request_id = Some(RequestId(u));
                }
            }
            _ => {}
        }
    }
}

// -------------------------------------------------------------------
// Discrete-fields reconstruction (ADR-022 macro form)
// -------------------------------------------------------------------

fn reconstruct_from_discrete<S>(event: &Event<'_>, ctx: &Context<'_, S>) -> Option<AuditEntry>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    let mut v = AuditTagVisitor::default();
    event.record(&mut v);

    // request_id: from event field, else from active span chain.
    let request_id = v.request_id.or_else(|| find_request_id_in_span(ctx))?;

    // actor: from thread-local, else from event-field actor JSON.
    let actor = current_operator().or_else(|| {
        v.actor_json
            .as_deref()
            .and_then(|s| serde_json::from_str::<Operator>(s).ok())
    })?;

    // action: ADR-022 macro form sends `action = "bind"` (a snake_case
    // string of `ActionKind`). For the discrete-fallback path we map it
    // back to a payload-less `Action::Row*` shape. This is best-effort
    // (the discrete form does not carry the full payload); call sites
    // that want lossless fidelity use [`emit_audit`] with a pre-built
    // payload instead.
    let action_str = v.action.as_deref()?;
    let action = action_from_kind_str(action_str, v.target_value.as_deref())?;

    // target: from `target_kind` + `target_value` per the ADR macro.
    let target = match v.target_kind.as_deref() {
        Some("part_id") | Some("part") => v
            .target_value
            .as_deref()
            .and_then(|s| qx_domain::PartId::new(s.to_string()).ok())
            .map(|id| TargetRef::Part { id }),
        Some("batch") => v
            .target_value
            .clone()
            .map(|label| TargetRef::Batch { label }),
        Some("diff") => v
            .target_value
            .clone()
            .map(|hash| TargetRef::Diff { hash: Hash(hash) }),
        _ => None,
    }?;

    let before = v.before.as_deref().and_then(|s| {
        (!s.is_empty())
            .then(|| serde_json::from_str(s).ok())
            .flatten()
    });
    let after = v.after.as_deref().and_then(|s| {
        (!s.is_empty())
            .then(|| serde_json::from_str(s).ok())
            .flatten()
    });
    let extra = v
        .extra
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));

    Some(AuditEntry {
        request_id,
        timestamp: now_utc(),
        actor,
        action,
        target,
        before,
        after,
        extra,
        signatures: Vec::new(),
        chain_hash: None,
    })
}

fn action_from_kind_str(s: &str, target_value: Option<&str>) -> Option<Action> {
    use qx_domain::PartId;
    // Try to use the real target value as the PartId; fall back to a
    // placeholder when the value is missing or fails validation.
    let part_id = target_value
        .and_then(|v| PartId::new(v.to_string()).ok())
        .or_else(|| PartId::new("23456789ABCDEF").ok())?;
    Some(match s {
        "row_add" | "mint" | "add" => Action::RowAdd {
            row: serde_json::Value::Object(serde_json::Map::new()),
        },
        "row_delete" | "delete" => Action::RowDelete { id: part_id },
        "row_void" | "void" => Action::RowVoid {
            id: part_id,
            reason: "discrete-form void".into(),
        },
        "row_bind" | "bind" => Action::RowBind {
            id: part_id,
            fields: std::collections::BTreeMap::new(),
        },
        "row_edit" | "edit" => Action::RowEdit {
            id: part_id,
            before: std::collections::BTreeMap::new(),
            after: std::collections::BTreeMap::new(),
        },
        "header_change" => Action::HeaderChange {
            before: Vec::new(),
            after: Vec::new(),
        },
        "bulk_change" => Action::BulkChange {
            description: "discrete-form bulk".into(),
            count: 0,
        },
        _ => return None,
    })
}

fn find_request_id_in_span<S>(ctx: &Context<'_, S>) -> Option<RequestId>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    let event_span = ctx.lookup_current()?;
    for span in event_span.scope() {
        if let Some(rid) = span.extensions().get::<SpanRequestId>() {
            return Some(rid.0);
        }
    }
    None
}

fn now_utc() -> Timestamp {
    // Use `time::OffsetDateTime::now_utc()` for non-test code paths.
    // Test injection point is `TEST_CLOCK` so deterministic timestamps
    // can be asserted on.
    if let Some(t) = TEST_CLOCK
        .get()
        .and_then(|c| c.lock().ok())
        .and_then(|g| *g)
    {
        return t;
    }
    time::OffsetDateTime::now_utc()
}

// A `OnceLock` test clock. Production builds never set this; tests
// override it with [`set_test_clock`].
static TEST_CLOCK: OnceLock<Mutex<Option<Timestamp>>> = OnceLock::new();

/// Test helper: pin the clock used for discrete-form reconstruction.
/// Production callers must not use this.
#[doc(hidden)]
pub fn set_test_clock(t: Option<Timestamp>) {
    let cell = TEST_CLOCK.get_or_init(|| Mutex::new(None));
    if let Ok(mut g) = cell.lock() {
        *g = t;
    }
}

// -------------------------------------------------------------------
// Convenience: build a synthetic AuditEntry for one of the common
// mutations. Used by the CLI binaries' scaffolds and by tests.
// -------------------------------------------------------------------

/// Build an [`AuditEntry`] for a `mint` (RowAdd) action.
pub fn mint_audit_entry(
    request_id: RequestId,
    actor: Operator,
    minted_id: qx_domain::PartId,
    extra: serde_json::Value,
) -> AuditEntry {
    let row = serde_json::json!({ "id": minted_id.as_str(), "status": "unbound" });
    AuditEntry {
        request_id,
        timestamp: time::OffsetDateTime::now_utc(),
        actor,
        action: Action::RowAdd { row },
        target: TargetRef::Part { id: minted_id },
        before: None,
        after: None,
        extra,
        signatures: Vec::new(),
        chain_hash: None,
    }
}

/// Build an [`AuditEntry`] for a generic entity-store write (ADR-035):
/// a `RecordWrite` action keyed on {collection, id}, so non-parts
/// collections are audited on the same spine as parts.
pub fn record_write_audit_entry(
    request_id: RequestId,
    actor: Operator,
    collection: String,
    id: String,
    extra: serde_json::Value,
    timestamp: time::OffsetDateTime,
) -> AuditEntry {
    AuditEntry {
        request_id,
        timestamp,
        actor,
        action: Action::RecordWrite {
            collection: collection.clone(),
            id: id.clone(),
        },
        target: TargetRef::Record { collection, id },
        before: None,
        after: None,
        extra,
        signatures: Vec::new(),
        chain_hash: None,
    }
}

/// Build an [`AuditEntry`] for a `bind` (RowBind) action.
pub fn bind_audit_entry(
    request_id: RequestId,
    actor: Operator,
    id: qx_domain::PartId,
    fields: std::collections::BTreeMap<String, String>,
    extra: serde_json::Value,
) -> AuditEntry {
    AuditEntry {
        request_id,
        timestamp: time::OffsetDateTime::now_utc(),
        actor,
        action: Action::RowBind {
            id: id.clone(),
            fields,
        },
        target: TargetRef::Part { id },
        before: None,
        after: None,
        extra,
        signatures: Vec::new(),
        chain_hash: None,
    }
}

/// Build an [`AuditEntry`] for an `edit` (RowEdit) action.
pub fn edit_audit_entry(
    request_id: RequestId,
    actor: Operator,
    id: qx_domain::PartId,
    before: std::collections::BTreeMap<String, String>,
    after: std::collections::BTreeMap<String, String>,
    extra: serde_json::Value,
) -> AuditEntry {
    AuditEntry {
        request_id,
        timestamp: time::OffsetDateTime::now_utc(),
        actor,
        action: Action::RowEdit {
            id: id.clone(),
            before,
            after,
        },
        target: TargetRef::Part { id },
        before: None,
        after: None,
        extra,
        signatures: Vec::new(),
        chain_hash: None,
    }
}

/// Build an [`AuditEntry`] for a `void` (RowVoid) action.
pub fn void_audit_entry(
    request_id: RequestId,
    actor: Operator,
    id: qx_domain::PartId,
    reason: String,
    extra: serde_json::Value,
) -> AuditEntry {
    AuditEntry {
        request_id,
        timestamp: time::OffsetDateTime::now_utc(),
        actor,
        action: Action::RowVoid {
            id: id.clone(),
            reason,
        },
        target: TargetRef::Part { id },
        before: None,
        after: None,
        extra,
        signatures: Vec::new(),
        chain_hash: None,
    }
}

// -------------------------------------------------------------------
// Operator identity helper for binaries that have no IdentityProvider
// wired in yet (foundation scaffold).
// -------------------------------------------------------------------

/// Construct a synthetic CLI operator from the `USER` env var and a
/// `GitConfig` source. Used by the foundation-scaffold CLI binaries
/// before the identity port (#30) is wired through `main()`.
pub fn cli_scaffold_operator() -> Operator {
    let user = std::env::var("USER").unwrap_or_else(|_| "cli-scaffold".into());
    Operator {
        id: OperatorId(format!("git:{user}")),
        display_name: user.clone(),
        source: qx_domain::IdentitySource::GitConfig,
        verified_at: None,
        claims: std::collections::BTreeMap::new(),
        pubkey: None,
    }
}

// -------------------------------------------------------------------
// Test-only public helper — exposes the AuditCsvLayer for the
// `tests/csv_git_pipeline.rs` integration test. Hidden from docs.
// Production callers MUST use [`init`] instead.
// -------------------------------------------------------------------

#[doc(hidden)]
pub fn __test_audit_csv_layer(
    sink: AuditSinkHandle,
) -> Box<dyn tracing_subscriber::Layer<tracing_subscriber::Registry> + Send + Sync> {
    AuditCsvLayer::new(sink).boxed()
}

// -------------------------------------------------------------------
// Tests
// -------------------------------------------------------------------

#[cfg(test)]
mod tests;
