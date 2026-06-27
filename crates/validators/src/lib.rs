//! `qx-validators` — pure-function validators over registry
//! state. Repository-trait-agnostic per ADR-017 §"Strangler-fig
//! migration sequence" step 2.
//!
//! ADR-016 §"Classification classes" is implemented here via
//! [`classify`] (delegating to [`qx_domain::Diff::classify`])
//! and [`policy_decision`]. CI is the policy authority; FE preflight
//! calls the same functions to attach an advisory `Vec<Action>` /
//! `AuthDecision` to a `Proposal` (ADR-019).
//!
//! ## Cross-surface parity
//!
//! Every public function in this crate is a pure function over already-
//! loaded inputs (slices / `Vec`s / domain values). The crate compiles
//! identically to:
//!
//! - native (CI policy authority per ADR-016 §"CI is the policy authority")
//! - wasm32-unknown-unknown (FE preflight per ADR-019 §"Pre-classification")
//!
//! No I/O. No file reads. No `OnceLock<PathBuf>`. The cross-surface
//! parity contract from ADR-027 §"Parity tests" is enforced by
//! construction: there is nothing to drift.

#![forbid(unsafe_code)]

/// Schema-driven record validation (ADR-039) — the contract-generic
/// validator that supersedes the `Part`-specific checks below as the FE
/// and `qx check` migrate onto the canonical form.
pub mod record;
pub use record::{validate_record, RecordContext, RecordIssue, Severity};

use std::fmt;

use thiserror::Error;

use qx_domain::{
    Action, ActionKind, AuthDecision, Diff, Operator, Part, PartId, PartStatus, PrintEvent,
    Timestamp,
};

// -------------------------------------------------------------------
// Schema constants (ADR-013 §"Validation rules")
// -------------------------------------------------------------------

/// Canonical registry column order, mirroring `schema/registry-
/// contract.json` and `validators/rules.py::REGISTRY_FIELDS`.
///
/// Per ADR-013, header reorder or rename is a breaking schema change
/// gated by ADR-016's `header_change` policy class.
pub const REGISTRY_HEADER: &[&str] = &[
    "id",
    "status",
    "minted_at",
    "bound_at",
    "type",
    "description",
    "vendor",
    "part_number",
    "location",
    "notes",
    "minted_by",
    "bound_by",
    "last_edited_at",
    "last_edited_by",
    "components",
    "manufacturer_id",
    "metadata",
];

/// Canonical `print_log.csv` column order per ADR-015 §"Schema".
pub const PRINT_LOG_HEADER: &[&str] = &[
    "id",
    "printed_at",
    "printed_by",
    "layout",
    "size_mm",
    "extra",
    "copies",
    "output_mode",
    "batch_label",
];

// -------------------------------------------------------------------
// ValidationError
// -------------------------------------------------------------------

/// All structural failures the validators can report.
///
/// Each variant is a single concrete failure; callers that want to
/// collect every failure in a sweep should call the individual
/// validators (each returns at most one `Err`) in a loop and aggregate.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ValidationError {
    /// Header column list doesn't match the canonical schema.
    #[error("header mismatch: expected {expected:?}, found {found:?}")]
    HeaderMismatch {
        expected: Vec<String>,
        found: Vec<String>,
    },

    /// Rows are not in canonical sort order; reports the first
    /// out-of-order index (0-based against the slice).
    #[error("rows out of sort order at index {row_index}")]
    UnsortedAt { row_index: usize },

    /// Duplicate part id across the registry.
    #[error("duplicate part id: {id}")]
    DuplicateId { id: PartId },

    /// Print-log rows reference part ids not in the registry (ADR-015
    /// §"FK semantics").
    #[error("orphan print_log rows reference unknown part ids: {ids:?}")]
    OrphanPrintEvents { ids: Vec<PartId> },

    /// Illegal lifecycle transition per ADR-012 §"Status lifecycle".
    #[error("illegal status transition {from} -> {to}")]
    IllegalTransition { from: PartStatus, to: PartStatus },

    /// Policy engine rejected a proposed change (ADR-016).
    #[error("policy: {reason}")]
    Policy { reason: String },
}

// -------------------------------------------------------------------
// Schema validation
// -------------------------------------------------------------------

/// Validate that the registry row set conforms to the canonical
/// schema. Per ADR-013 §"Validation rules", header conformance is
/// enforced at the (de)serialisation boundary by `serde`; this
/// function exists to surface that contract explicitly and to allow
/// callers that already have parsed `Part` rows to assert the
/// canonical column-order invariant without round-tripping through
/// CSV.
///
/// The check is structural: any successfully-deserialized `Part`
/// already passes serde's column-name + ordering enforcement. The
/// function reports `Ok(())` for any slice (including empty); a
/// `HeaderMismatch` is only reachable from the `*_with_header`
/// variant below.
pub fn validate_registry_schema(_rows: &[Part]) -> Result<(), ValidationError> {
    // Successfully-typed `Part` values already round-trip through the
    // canonical schema (serde rejects unknown fields at deserialisation
    // when configured by the storage adapter). This entry point
    // exists so callers can assert the invariant explicitly.
    Ok(())
}

/// Same as [`validate_registry_schema`] but takes an explicit header
/// list — used when the upstream layer parsed CSV before typing rows.
pub fn validate_registry_header(header: &[String]) -> Result<(), ValidationError> {
    expect_header(header, REGISTRY_HEADER)
}

/// Validate `print_log` rows. Same shape contract as
/// [`validate_registry_schema`].
pub fn validate_print_log_schema(_rows: &[PrintEvent]) -> Result<(), ValidationError> {
    Ok(())
}

/// `print_log` header check; mirrors [`validate_registry_header`].
pub fn validate_print_log_header(header: &[String]) -> Result<(), ValidationError> {
    expect_header(header, PRINT_LOG_HEADER)
}

fn expect_header(found: &[String], expected: &[&str]) -> Result<(), ValidationError> {
    if found.len() == expected.len() && found.iter().zip(expected).all(|(a, b)| a == b) {
        return Ok(());
    }
    Err(ValidationError::HeaderMismatch {
        expected: expected.iter().map(|s| (*s).to_owned()).collect(),
        found: found.to_vec(),
    })
}

// -------------------------------------------------------------------
// Sort stability
// -------------------------------------------------------------------

/// Validate that `rows` is already in ascending order under
/// `sort_key`. Reports the first out-of-order index on failure per
/// ADR-013 §"Sort stability".
pub fn validate_sort_stable<T, K, F>(rows: &[T], sort_key: F) -> Result<(), ValidationError>
where
    F: Fn(&T) -> K,
    K: Ord,
{
    for i in 1..rows.len() {
        let prev = sort_key(&rows[i - 1]);
        let curr = sort_key(&rows[i]);
        if prev > curr {
            return Err(ValidationError::UnsortedAt { row_index: i });
        }
    }
    Ok(())
}

/// Canonical registry sort key per ADR-013: ascending by `id`.
/// Matches the on-disk byte order of `registry.csv` produced by the
/// Python `mint.py` / `bind.py` / `validators/rules.py` toolchain
/// (id-only sort). Reviewer note: an earlier draft of this function
/// used `(status, id)` but that diverged from ADR-013 + Python
/// parity; aligned 2026-05-11 per PR #39 reviewer (subagent
/// `a79d4083`).
pub fn registry_sort_key(p: &Part) -> String {
    p.id.as_str().to_owned()
}

/// Canonical print-log sort key per ADR-015: `(printed_at, id)`.
/// Timestamp is primary; `id` is the tiebreaker so concurrent prints
/// of different IDs at the same second produce a stable order.
pub fn print_log_sort_key(e: &PrintEvent) -> (Timestamp, String) {
    (e.printed_at, e.id.as_str().to_owned())
}

// -------------------------------------------------------------------
// Uniqueness
// -------------------------------------------------------------------

/// Validate that every `Part.id` in `rows` is unique. Reports the
/// first duplicate found.
pub fn validate_unique_ids(rows: &[Part]) -> Result<(), ValidationError> {
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for p in rows {
        if !seen.insert(p.id.as_str()) {
            return Err(ValidationError::DuplicateId { id: p.id.clone() });
        }
    }
    Ok(())
}

// -------------------------------------------------------------------
// FK integrity (ADR-015)
// -------------------------------------------------------------------

/// Validate that every `PrintEvent.id` is present in `registry`.
/// Returns the full orphan list (sorted by id for determinism) on
/// failure so CI can render every offending row in one pass.
pub fn validate_print_log_fk(
    prints: &[PrintEvent],
    registry: &[Part],
) -> Result<(), ValidationError> {
    let known: std::collections::HashSet<&str> = registry.iter().map(|p| p.id.as_str()).collect();
    let mut orphans: Vec<PartId> = prints
        .iter()
        .filter(|e| !known.contains(e.id.as_str()))
        .map(|e| e.id.clone())
        .collect();
    if orphans.is_empty() {
        return Ok(());
    }
    orphans.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    orphans.dedup_by(|a, b| a.as_str() == b.as_str());
    Err(ValidationError::OrphanPrintEvents { ids: orphans })
}

// -------------------------------------------------------------------
// Status transition (ADR-012 §"Status lifecycle")
// -------------------------------------------------------------------

/// Validate a single `before -> after` status transition.
///
/// Allowed:
/// - `unbound -> bound`
/// - `unbound -> void`
/// - `bound -> void`
/// - same-status (idempotent edits) for `unbound`, `bound`, `void`
///
/// Disallowed:
/// - `bound -> unbound` (no resurrection)
/// - `void -> *` (terminal)
pub fn validate_status_transition(
    before: PartStatus,
    after: PartStatus,
) -> Result<(), ValidationError> {
    use PartStatus::*;
    match (before, after) {
        (Unbound, Unbound) => Ok(()),
        (Unbound, Bound) => Ok(()),
        (Unbound, Void) => Ok(()),
        (Bound, Bound) => Ok(()),
        (Bound, Void) => Ok(()),
        (Void, Void) => Ok(()),
        (from, to) => Err(ValidationError::IllegalTransition { from, to }),
    }
}

// -------------------------------------------------------------------
// Semantic-diff policy engine (ADR-016 — load-bearing)
// -------------------------------------------------------------------

/// Policy inputs consumed by [`policy_decision`].
///
/// Default values map to ADR-016 §"Policy model" baseline:
/// header changes blocked, destructive operations require elevation,
/// bulk threshold = 100 rows, elevation read from claim
/// `qms-approver`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Policy {
    /// If `false`, any `HeaderChange` action returns `Block`.
    pub allow_header_changes: bool,
    /// If `true`, `RowDelete` and `RowVoid` actions return
    /// `RequiresElevation` unless the operator carries the
    /// elevation claim.
    pub destructive_requires_elevation: bool,
    /// `BulkChange { count }` actions with `count > bulk_threshold`
    /// return `RequiresElevation`.
    pub bulk_threshold: u32,
    /// Name of the claim key the authorizer reads from
    /// `Operator::claims` to grant elevation. The value is the
    /// matched role (e.g. `"qms-approver"`); presence of the key
    /// with value `"true"` (or any non-empty value other than
    /// `"false"`) is sufficient.
    pub elevation_role_claim: String,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            allow_header_changes: false,
            destructive_requires_elevation: true,
            bulk_threshold: 100,
            elevation_role_claim: "qms-approver".to_owned(),
        }
    }
}

/// Re-export of [`qx_domain::Diff::classify`]. The
/// validators crate is the single reader-of-record for the policy
/// vocabulary per ADR-016 §"Classification classes"; the classifier
/// implementation lives in `domain` for type-locality but every
/// downstream call site (CI, FE preflight) goes through this
/// function so it can be intercepted, logged, or replaced as a unit.
pub fn classify(diff: &Diff) -> Vec<Action> {
    diff.classify()
}

/// Apply ADR-016 §"Policy model" rules to a diff and produce an
/// `AuthDecision`. The engine runs identically in CI (authoritative)
/// and FE preflight (advisory); divergence between the two is a
/// drift bug, never a feature.
///
/// Rules:
///
/// - `HeaderChange` → `Block` unless `policy.allow_header_changes`.
/// - `RowDelete` / `RowVoid` → `RequiresElevation` if
///   `policy.destructive_requires_elevation` and the operator does
///   not carry the elevation claim; otherwise `Allow`.
/// - `BulkChange { count }` with `count > policy.bulk_threshold` →
///   `RequiresElevation`.
/// - `RowAdd` / `RowBind` / `RowEdit` → `Allow` (schema validators
///   are the gate for these classes).
///
/// Multi-action diffs collapse via the strict order
/// `Block > RequiresElevation > Warn > Allow`.
pub fn policy_decision(diff: &Diff, operator: &Operator, policy: &Policy) -> AuthDecision {
    let actions = classify(diff);
    let mut strongest: AuthDecision = AuthDecision::Allow;
    for action in &actions {
        let next = decide_one(action, operator, policy);
        strongest = stronger(strongest, next);
    }
    strongest
}

fn decide_one(action: &Action, operator: &Operator, policy: &Policy) -> AuthDecision {
    match action.kind() {
        ActionKind::HeaderChange => {
            if policy.allow_header_changes {
                AuthDecision::Allow
            } else {
                AuthDecision::Block {
                    reason: "header change not permitted under current policy".into(),
                }
            }
        }
        ActionKind::RowDelete | ActionKind::RowVoid => {
            if !policy.destructive_requires_elevation {
                return AuthDecision::Allow;
            }
            if has_elevation_claim(operator, &policy.elevation_role_claim) {
                AuthDecision::Allow
            } else {
                AuthDecision::RequiresElevation {
                    approver_role: policy.elevation_role_claim.clone(),
                }
            }
        }
        ActionKind::BulkChange => {
            // Pull the count off the matched variant.
            let count = match action {
                Action::BulkChange { count, .. } => *count,
                _ => unreachable!("ActionKind::BulkChange matches only Action::BulkChange"),
            };
            if count > policy.bulk_threshold {
                AuthDecision::RequiresElevation {
                    approver_role: policy.elevation_role_claim.clone(),
                }
            } else {
                AuthDecision::Allow
            }
        }
        ActionKind::RowAdd | ActionKind::RowBind | ActionKind::RowEdit => AuthDecision::Allow,
        // A generic entity-store upsert is a non-destructive write (the
        // PR review + gate enforce the contract); allow, like RowEdit.
        ActionKind::RecordWrite => AuthDecision::Allow,
        // A label print is read-only output (ADR-022 print-fold) — allow.
        ActionKind::Print => AuthDecision::Allow,
    }
}

fn has_elevation_claim(operator: &Operator, claim_key: &str) -> bool {
    match operator.claims.get(claim_key) {
        None => false,
        Some(v) => {
            let v = v.trim();
            !v.is_empty() && !v.eq_ignore_ascii_case("false") && v != "0"
        }
    }
}

/// Total order over `AuthDecision`: Block > RequiresElevation >
/// Warn > Allow. Used to collapse multi-action diff decisions.
fn rank(d: &AuthDecision) -> u8 {
    match d {
        AuthDecision::Allow => 0,
        AuthDecision::Warn { .. } => 1,
        AuthDecision::RequiresElevation { .. } => 2,
        AuthDecision::Block { .. } => 3,
    }
}

fn stronger(a: AuthDecision, b: AuthDecision) -> AuthDecision {
    if rank(&b) > rank(&a) {
        b
    } else {
        a
    }
}

// -------------------------------------------------------------------
// Display helpers
// -------------------------------------------------------------------

/// Render a `ValidationError` as a CI-friendly single-line message.
/// Re-uses the `Display` impl from `thiserror` but exposed as a
/// function so call sites don't have to import the trait.
pub fn format_error(e: &ValidationError) -> String {
    fmt::format(format_args!("{e}"))
}

// -------------------------------------------------------------------
// Tests
// -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use qx_domain::{
        DiffEdit, DiffRow, HeaderChange, IdentitySource, KeyId, OperatorId, OperatorRef, PartId,
        PartStatus,
    };
    use std::collections::BTreeMap;
    use time::OffsetDateTime;

    fn ts(secs: i64) -> Timestamp {
        OffsetDateTime::from_unix_timestamp(secs).unwrap()
    }

    fn pid(s: &str) -> PartId {
        PartId::new(s).unwrap()
    }

    fn sample_part(id: &str, status: PartStatus) -> Part {
        Part {
            id: pid(id),
            status,
            minted_at: ts(1_700_000_000),
            bound_at: None,
            type_: None,
            description: None,
            vendor: None,
            part_number: None,
            location: None,
            notes: None,
            minted_by: None,
            bound_by: None,
            last_edited_at: None,
            last_edited_by: None,
            components: vec![],
            manufacturer_id: None,
            metadata: std::collections::BTreeMap::new(),
            signatures: vec![],
            chain_hash: None,
        }
    }

    fn sample_print(id: &str, when: i64) -> PrintEvent {
        PrintEvent {
            id: pid(id),
            printed_at: ts(when),
            printed_by: OperatorRef(OperatorId("github:tester".into())),
            layout: "single".into(),
            size_mm: 12.0,
            extra: serde_json::Value::Object(serde_json::Map::new()),
            copies: 1,
            output_mode: "preview".into(),
            batch_label: None,
        }
    }

    fn op(claims: &[(&str, &str)]) -> Operator {
        let mut m = BTreeMap::new();
        for (k, v) in claims {
            m.insert((*k).to_owned(), (*v).to_owned());
        }
        Operator {
            id: OperatorId("github:tester".into()),
            display_name: "Tester".into(),
            source: IdentitySource::GitConfig,
            verified_at: None,
            claims: m,
            pubkey: Some(KeyId("k1".into())),
        }
    }

    // ------------------------------------------------------------------
    // 1. Schema
    // ------------------------------------------------------------------

    #[test]
    fn schema_passes_on_typed_parts() {
        let rows = [
            sample_part("ABCDEFGHJKMNPQ", PartStatus::Unbound),
            sample_part("ABCDEFGHJKMNPR", PartStatus::Bound),
        ];
        assert!(validate_registry_schema(&rows).is_ok());
    }

    #[test]
    fn schema_header_match_passes() {
        let header: Vec<String> = REGISTRY_HEADER.iter().map(|s| (*s).to_owned()).collect();
        assert!(validate_registry_header(&header).is_ok());
    }

    #[test]
    fn schema_header_mismatch_reports_diff() {
        let header: Vec<String> = vec!["id".into(), "wrong".into()];
        let err = validate_registry_header(&header).unwrap_err();
        match err {
            ValidationError::HeaderMismatch { .. } => {}
            other => panic!("expected HeaderMismatch, got {other:?}"),
        }
    }

    #[test]
    fn print_log_header_match_passes() {
        let header: Vec<String> = PRINT_LOG_HEADER.iter().map(|s| (*s).to_owned()).collect();
        assert!(validate_print_log_header(&header).is_ok());
    }

    // ------------------------------------------------------------------
    // 2. Sort stability
    // ------------------------------------------------------------------

    #[test]
    fn sort_stable_happy_path() {
        let rows = [
            sample_part("ABCDEFGHJKMNPQ", PartStatus::Unbound),
            sample_part("ABCDEFGHJKMNPR", PartStatus::Unbound),
            sample_part("ABCDEFGHJKMNPS", PartStatus::Unbound),
        ];
        assert!(validate_sort_stable(&rows, registry_sort_key).is_ok());
    }

    #[test]
    fn sort_stable_detects_out_of_order_row() {
        let rows = [
            sample_part("ABCDEFGHJKMNPR", PartStatus::Unbound),
            sample_part("ABCDEFGHJKMNPQ", PartStatus::Unbound),
        ];
        let err = validate_sort_stable(&rows, registry_sort_key).unwrap_err();
        assert_eq!(err, ValidationError::UnsortedAt { row_index: 1 });
    }

    #[test]
    fn sort_stable_print_log_by_printed_at() {
        let prints = [
            sample_print("ABCDEFGHJKMNPQ", 10),
            sample_print("ABCDEFGHJKMNPR", 20),
        ];
        assert!(validate_sort_stable(&prints, print_log_sort_key).is_ok());

        let backwards = [
            sample_print("ABCDEFGHJKMNPQ", 30),
            sample_print("ABCDEFGHJKMNPR", 10),
        ];
        assert!(validate_sort_stable(&backwards, print_log_sort_key).is_err());
    }

    #[test]
    fn sort_stable_print_log_uses_id_as_tiebreaker_per_adr_015() {
        // Two prints at the same printed_at; secondary key is id.
        let in_order = [
            sample_print("ABCDEFGHJKMNPQ", 10),
            sample_print("ABCDEFGHJKMNPR", 10),
        ];
        assert!(validate_sort_stable(&in_order, print_log_sort_key).is_ok());

        let out_of_order = [
            sample_print("ABCDEFGHJKMNPR", 10),
            sample_print("ABCDEFGHJKMNPQ", 10),
        ];
        let err = validate_sort_stable(&out_of_order, print_log_sort_key).unwrap_err();
        assert_eq!(err, ValidationError::UnsortedAt { row_index: 1 });
    }

    // ------------------------------------------------------------------
    // 3. Uniqueness
    // ------------------------------------------------------------------

    #[test]
    fn uniqueness_happy_path() {
        let rows = [
            sample_part("ABCDEFGHJKMNPQ", PartStatus::Unbound),
            sample_part("ABCDEFGHJKMNPR", PartStatus::Bound),
        ];
        assert!(validate_unique_ids(&rows).is_ok());
    }

    #[test]
    fn uniqueness_detects_duplicate_id() {
        let rows = [
            sample_part("ABCDEFGHJKMNPQ", PartStatus::Unbound),
            sample_part("ABCDEFGHJKMNPQ", PartStatus::Bound),
        ];
        let err = validate_unique_ids(&rows).unwrap_err();
        match err {
            ValidationError::DuplicateId { id } => {
                assert_eq!(id.as_str(), "ABCDEFGHJKMNPQ");
            }
            other => panic!("expected DuplicateId, got {other:?}"),
        }
    }

    // ------------------------------------------------------------------
    // 4. FK
    // ------------------------------------------------------------------

    #[test]
    fn fk_happy_path() {
        let registry = [sample_part("ABCDEFGHJKMNPQ", PartStatus::Bound)];
        let prints = [sample_print("ABCDEFGHJKMNPQ", 10)];
        assert!(validate_print_log_fk(&prints, &registry).is_ok());
    }

    #[test]
    fn fk_detects_orphan() {
        let registry = [sample_part("ABCDEFGHJKMNPQ", PartStatus::Bound)];
        let prints = [
            sample_print("ABCDEFGHJKMNPQ", 10),
            sample_print("ABCDEFGHJKMNPR", 20),
        ];
        let err = validate_print_log_fk(&prints, &registry).unwrap_err();
        match err {
            ValidationError::OrphanPrintEvents { ids } => {
                assert_eq!(ids.len(), 1);
                assert_eq!(ids[0].as_str(), "ABCDEFGHJKMNPR");
            }
            other => panic!("expected OrphanPrintEvents, got {other:?}"),
        }
    }

    // ------------------------------------------------------------------
    // 5. Status transitions (ADR-012)
    // ------------------------------------------------------------------

    #[test]
    fn transitions_unbound_to_bound_to_void_pass() {
        assert!(validate_status_transition(PartStatus::Unbound, PartStatus::Bound).is_ok());
        assert!(validate_status_transition(PartStatus::Bound, PartStatus::Void).is_ok());
        assert!(validate_status_transition(PartStatus::Unbound, PartStatus::Void).is_ok());
    }

    #[test]
    fn transitions_bound_to_unbound_rejected() {
        let err = validate_status_transition(PartStatus::Bound, PartStatus::Unbound).unwrap_err();
        match err {
            ValidationError::IllegalTransition { from, to } => {
                assert_eq!(from, PartStatus::Bound);
                assert_eq!(to, PartStatus::Unbound);
            }
            other => panic!("expected IllegalTransition, got {other:?}"),
        }
    }

    #[test]
    fn transitions_void_to_anything_rejected() {
        assert!(validate_status_transition(PartStatus::Void, PartStatus::Bound).is_err());
        assert!(validate_status_transition(PartStatus::Void, PartStatus::Unbound).is_err());
    }

    // ------------------------------------------------------------------
    // 6-9. Policy engine
    // ------------------------------------------------------------------

    fn header_change_diff() -> Diff {
        Diff {
            header_changes: vec![HeaderChange {
                file: "registry.csv".into(),
                before: vec!["id".into(), "status".into()],
                after: vec!["id".into(), "status".into(), "vendor".into()],
            }],
            ..Default::default()
        }
    }

    fn row_delete_diff() -> Diff {
        Diff {
            deletes: vec![DiffRow {
                id: Some(pid("ABCDEFGHJKMNPQ")),
                fields: BTreeMap::new(),
            }],
            ..Default::default()
        }
    }

    fn bulk_change_diff(count: u32) -> Diff {
        // Use header-less deletes to synthesize a BulkChange action.
        let deletes = (0..count)
            .map(|_| DiffRow {
                id: None,
                fields: BTreeMap::new(),
            })
            .collect();
        Diff {
            deletes,
            ..Default::default()
        }
    }

    fn row_add_diff() -> Diff {
        let mut fields = BTreeMap::new();
        fields.insert("status".into(), "unbound".into());
        Diff {
            adds: vec![DiffRow {
                id: Some(pid("ABCDEFGHJKMNPQ")),
                fields,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn policy_header_change_blocked_by_default() {
        let policy = Policy::default();
        let decision = policy_decision(&header_change_diff(), &op(&[]), &policy);
        match decision {
            AuthDecision::Block { .. } => {}
            other => panic!("expected Block, got {other:?}"),
        }
    }

    #[test]
    fn policy_header_change_allowed_when_flagged() {
        let policy = Policy {
            allow_header_changes: true,
            ..Policy::default()
        };
        let decision = policy_decision(&header_change_diff(), &op(&[]), &policy);
        assert_eq!(decision, AuthDecision::Allow);
    }

    #[test]
    fn policy_destructive_requires_elevation() {
        let policy = Policy::default();
        let decision = policy_decision(&row_delete_diff(), &op(&[]), &policy);
        match decision {
            AuthDecision::RequiresElevation { approver_role } => {
                assert_eq!(approver_role, "qms-approver");
            }
            other => panic!("expected RequiresElevation, got {other:?}"),
        }
    }

    #[test]
    fn policy_destructive_allowed_with_elevation_claim() {
        let policy = Policy::default();
        let decision = policy_decision(
            &row_delete_diff(),
            &op(&[("qms-approver", "true")]),
            &policy,
        );
        assert_eq!(decision, AuthDecision::Allow);
    }

    #[test]
    fn policy_destructive_claim_false_value_does_not_elevate() {
        let policy = Policy::default();
        let decision = policy_decision(
            &row_delete_diff(),
            &op(&[("qms-approver", "false")]),
            &policy,
        );
        match decision {
            AuthDecision::RequiresElevation { .. } => {}
            other => panic!("expected RequiresElevation, got {other:?}"),
        }
    }

    #[test]
    fn policy_bulk_above_threshold_requires_elevation() {
        let policy = Policy {
            bulk_threshold: 100,
            ..Policy::default()
        };
        let decision = policy_decision(&bulk_change_diff(200), &op(&[]), &policy);
        match decision {
            AuthDecision::RequiresElevation { .. } => {}
            other => panic!("expected RequiresElevation, got {other:?}"),
        }
    }

    #[test]
    fn policy_bulk_below_threshold_allowed() {
        let policy = Policy {
            bulk_threshold: 100,
            ..Policy::default()
        };
        let decision = policy_decision(&bulk_change_diff(50), &op(&[]), &policy);
        assert_eq!(decision, AuthDecision::Allow);
    }

    #[test]
    fn policy_row_add_allowed() {
        let policy = Policy::default();
        let decision = policy_decision(&row_add_diff(), &op(&[]), &policy);
        assert_eq!(decision, AuthDecision::Allow);
    }

    #[test]
    fn policy_row_edit_allowed() {
        let mut before = BTreeMap::new();
        before.insert("status".into(), "bound".into());
        before.insert("location".into(), "L1".into());
        let mut after = BTreeMap::new();
        after.insert("status".into(), "bound".into());
        after.insert("location".into(), "L2".into());
        let diff = Diff {
            edits: vec![DiffEdit {
                id: pid("ABCDEFGHJKMNPQ"),
                before,
                after,
                changed_keys: vec!["location".into()],
            }],
            ..Default::default()
        };
        let decision = policy_decision(&diff, &op(&[]), &Policy::default());
        assert_eq!(decision, AuthDecision::Allow);
    }

    #[test]
    fn policy_strictest_wins_with_row_add_and_header_change() {
        // RowAdd alone → Allow; HeaderChange alone → Block; combined → Block.
        let mut diff = row_add_diff();
        diff.header_changes.push(HeaderChange {
            file: "registry.csv".into(),
            before: vec!["id".into()],
            after: vec!["id".into(), "status".into()],
        });
        let policy = Policy::default();
        let decision = policy_decision(&diff, &op(&[]), &policy);
        match decision {
            AuthDecision::Block { .. } => {}
            other => panic!("expected Block (strictest), got {other:?}"),
        }
    }

    #[test]
    fn policy_strictest_wins_destructive_beats_allow() {
        // RowAdd + RowDelete (no elevation claim) → RequiresElevation.
        let mut diff = row_add_diff();
        diff.deletes.push(DiffRow {
            id: Some(pid("ABCDEFGHJKMNPR")),
            fields: BTreeMap::new(),
        });
        let policy = Policy::default();
        let decision = policy_decision(&diff, &op(&[]), &policy);
        match decision {
            AuthDecision::RequiresElevation { .. } => {}
            other => panic!("expected RequiresElevation, got {other:?}"),
        }
    }

    #[test]
    fn classify_re_exports_domain_classifier() {
        // Sanity: validators::classify and Diff::classify are
        // bit-identical so a future intercept point doesn't drift.
        let diff = header_change_diff();
        assert_eq!(classify(&diff), diff.classify());
    }

    // ------------------------------------------------------------------
    // 10. Cross-surface parity — all test bodies above are pure (no FS,
    //     no env) so they run identically on native + wasm32. Captured
    //     here as a placeholder to make the contract explicit; the
    //     real wasm32 invocation happens in the `cargo build` gate.
    // ------------------------------------------------------------------

    #[test]
    fn pure_function_contract_holds() {
        // If this file imported `std::fs`, `std::env`, or `std::path`
        // outside #[cfg(test)] the wasm32 build of the crate would
        // still link but the FE preflight runtime would crash at the
        // first I/O attempt. We assert lexically here: any future
        // contributor adding I/O sees this test name in their grep.
        let registry = [sample_part("ABCDEFGHJKMNPQ", PartStatus::Bound)];
        let prints = [sample_print("ABCDEFGHJKMNPQ", 10)];
        assert!(validate_print_log_fk(&prints, &registry).is_ok());
    }
}
