//! `part-registry-domain` — pure data types shared across the workspace.
//!
//! Per ADR-017 §"Workspace shape": this crate is the type keystone.
//! No I/O, no side effects, no dependencies on other workspace crates.
//! Every other crate in the workspace can depend on this one; this
//! crate depends on nothing workspace-internal.
//!
//! Types here are referenced by:
//! - `Repository` (ADR-018) — `Part`, `AuditEntry`, `PrintEvent`,
//!   `PartFilter`, `AuditFilter`, `PrintEventFilter`, `Hash`
//! - `ProposalSink` (ADR-019) — `Proposal`, `ProposalRef`, `Diff`,
//!   `Action`, `ChangeClass`, `ProposalStatus`
//! - `IdentityProvider` / `Authorizer` (ADR-020) — `Operator`,
//!   `IdentitySource`, `KeyId`, `Capabilities`, `AuthDecision`
//! - `SigningProvider` / `VerificationProvider` (ADR-024) —
//!   `Signature`, `Verification`, `SigAlgorithm`
//! - audit-CSV layer (ADR-022) — `AuditEntry`, `AuditSource`,
//!   `RequestId`, `TargetRef`
//!
//! ## Foundation issue #28 — interface-sharpness gap closure
//!
//! This module locks the five gaps surfaced by the foundation
//! parallelism audit (2026-05-10):
//!
//! 1. (HARD) `Diff` — concrete `{adds, deletes, edits, header_changes}`
//!    shape with typed `DiffEdit` per-row before/after; `Diff::classify`
//!    is the pure-function FE-preflight + CI-authoritative classifier.
//! 2. (HARD) `Action` lives here (not in `identity`, not in
//!    `validators`). Variants exactly match ADR-016 §"Semantic change
//!    classes". `ActionKind` is the discriminator-only enum.
//! 3. (HARD) `PartFilter`, `AuditFilter`, `PrintEventFilter` — concrete
//!    fields fixed; sort key + limit/offset for paging.
//! 4. (SOFT) `Capabilities` populated as documented "reserved for
//!    future adapters" per ADR-020 §"MVP authorization policy". MVP
//!    `Authorizer` continues to read claims directly.
//! 5. (SOFT) `init_with_pending_audit_sink` lives in
//!    `crates/observability/`; the type seam (`AuditEntry`,
//!    `Repository`) is here so #34 can attach late-bound.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use thiserror::Error;
use time::OffsetDateTime;

// -------------------------------------------------------------------
// Identifiers and primitives
// -------------------------------------------------------------------

/// Canonical part identifier per ADR-012.
///
/// Fourteen characters drawn from the no-lookalike alphabet
/// `23456789ABCDEFGHJKMNPQRSTUVWXYZ` (Crockford-style: no `0`/`O`,
/// no `1`/`I`/`L`). Constructors validate; field is private to make
/// the invariant unforgeable from outside the crate.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PartId(String);

/// ADR-012's canonical alphabet.
pub const PART_ID_ALPHABET: &str = "23456789ABCDEFGHJKMNPQRSTUVWXYZ";

/// ADR-012's canonical length.
pub const PART_ID_LEN: usize = 14;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PartIdError {
    #[error("part id must be {expected} characters (got {got})")]
    BadLength { expected: usize, got: usize },
    #[error("part id contains character {0:?} not in canonical alphabet")]
    BadAlphabet(char),
}

impl PartId {
    /// Construct a `PartId` from any string-like value, validating the
    /// length and the canonical alphabet.
    pub fn new(s: impl Into<String>) -> Result<Self, PartIdError> {
        let s = s.into();
        if s.chars().count() != PART_ID_LEN {
            return Err(PartIdError::BadLength {
                expected: PART_ID_LEN,
                got: s.chars().count(),
            });
        }
        if let Some(bad) = s.chars().find(|c| !PART_ID_ALPHABET.contains(*c)) {
            return Err(PartIdError::BadAlphabet(bad));
        }
        Ok(Self(s))
    }

    /// Borrow the canonical 14-char string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PartId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for PartId {
    type Err = PartIdError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s.to_owned())
    }
}

impl TryFrom<String> for PartId {
    type Error = PartIdError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::new(s)
    }
}

impl From<PartId> for String {
    fn from(id: PartId) -> Self {
        id.0
    }
}

/// UUIDv7 per ADR-022 §"request_id propagation".
///
/// Time-ordered, 128-bit, sortable in CSV cells. One generated at
/// the outermost user-action boundary; propagated via `tracing` span
/// context to every inner emit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RequestId(pub uuid::Uuid);

impl RequestId {
    /// Generate a fresh UUIDv7 (time-ordered).
    pub fn new() -> Self {
        Self(uuid::Uuid::now_v7())
    }
}

impl Default for RequestId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for RequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// UTC timestamp alias; ADR-022 mandates ISO-8601 with `Z` suffix in
/// CSV serialisation. Adapters serialise via the `time` crate's
/// `format!` with `Format::Rfc3339`.
///
/// Type alias rather than a newtype so consumers can freely use the
/// rich `time::OffsetDateTime` API; the wire format is enforced at
/// adapter boundaries (CSV column / JSON value), not at the type
/// level.
pub type Timestamp = OffsetDateTime;

/// Content / commit hash. ADR-022 §"AuditEntry shape" mandates
/// hex-encoded representation in CSV. ADR-018 §"Snapshot hash"
/// uses this for `Repository::snapshot_hash`.
///
/// The value is opaque to domain code — we don't constrain whether
/// it's BLAKE3 (32 bytes), SHA-256 (32 bytes), or a git commit SHA
/// (20 bytes hex). Adapters validate format at construction and
/// round-trip the string blindly.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Hash(pub String);

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Public-key identifier (GPG long key id, SSH fingerprint, Sigstore
/// cert serial). ADR-020 plumbs this through `Operator.pubkey` for
/// ADR-024 forward-compat.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyId(pub String);

impl fmt::Display for KeyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// -------------------------------------------------------------------
// Identity (ADR-020)
// -------------------------------------------------------------------

/// Canonical operator identifier (e.g. `github:lars-gerchow`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OperatorId(pub String);

impl fmt::Display for OperatorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Where this operator's identity came from.
///
/// `EnvUser` is **test/dev only**. Production deploys reject it at
/// adapter construction time per ADR-020 §"MVP adapters".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IdentitySource {
    GitConfig,
    GitHubOAuth,
    OidcGeneric {
        issuer: String,
    },
    MtlsCert {
        fingerprint: String,
    },
    /// **Deprecated for production** per ADR-020. Construction in
    /// release builds is rejected at the adapter layer.
    EnvUser,
    OfflineClaim,
    SigstoreKeyless {
        fulcio: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Operator {
    pub id: OperatorId,
    pub display_name: String,
    pub source: IdentitySource,
    /// `None` for unverified self-asserted claims (e.g. git config);
    /// timestamp of the IdP attestation when verified.
    pub verified_at: Option<Timestamp>,
    /// Arbitrary IdP-provided claims. Adapters own the
    /// claim-to-capability mapping per ADR-020 §"Why `claims`".
    pub claims: BTreeMap<String, String>,
    /// ADR-024 forward-compat: bound public key when known. Reserved
    /// at MVP; populated when a `Sigstore` adapter activates.
    pub pubkey: Option<KeyId>,
}

/// Lightweight operator reference (id only) for embedding in
/// rows where the full claims map would be redundant (e.g.
/// `print_log.csv` — operator already audited at session open).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OperatorRef(pub OperatorId);

impl From<&Operator> for OperatorRef {
    fn from(op: &Operator) -> Self {
        Self(op.id.clone())
    }
}

/// Per-action capability projection per ADR-020 §"Capabilities".
///
/// **Reserved for future adapters.** The MVP `Authorizer` table reads
/// `Operator::claims` directly without going through this struct (per
/// ADR-020 §"MVP authorization policy"). Future adapters that want
/// richer policy (RBAC over typed roles, ABAC over structured
/// attributes) will populate this struct from their richer claim
/// sources, and a future `Authorizer` implementation will dispatch on
/// it.
///
/// Closes interface-sharpness gap #4 (SOFT) from the foundation
/// parallelism audit: populated and documented, not deferred.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub propose_routine: bool,
    pub propose_destructive: bool,
    pub approve_routine: bool,
    pub approve_destructive: bool,
    pub admin: bool,
}

// -------------------------------------------------------------------
// Action / change classification (ADR-016 + ADR-020)
// -------------------------------------------------------------------
//
// Closes interface-sharpness gap #2 (HARD): `Action` lives here in
// `crates/domain/`, NOT in `identity`, NOT in `validators`. Both
// crates import; neither re-defines.

/// Discriminator for `Action`. Equals `ChangeClass` per ADR-016 §"the
/// classes above are the policy vocabulary." Use this when matching
/// on the kind of action without needing the payload (e.g. policy
/// table lookup, audit-log column projection).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    RowAdd,
    RowDelete,
    RowVoid,
    RowBind,
    RowEdit,
    HeaderChange,
    BulkChange,
}

/// `ChangeClass` is an alias for `ActionKind` per ADR-016 §"Semantic
/// change classes." The classifier emits these; `Authorizer` consumes
/// them via `Action::kind()`. The two names exist to mirror the ADR
/// vocabulary at each call site (a `ChangeClass` is what the
/// classifier produces; an `ActionKind` is what the authorizer
/// matches on); the type is the same.
pub type ChangeClass = ActionKind;

/// One concrete action the policy engine reasons about.
///
/// Variants exactly match ADR-016 §"Semantic change classes." Each
/// variant carries the payload necessary to evaluate policy without
/// re-reading the diff.
///
/// Use [`Action::kind`] to extract the discriminator-only
/// `ActionKind` for matching.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    RowAdd {
        row: Json,
    },
    RowDelete {
        id: PartId,
    },
    RowVoid {
        id: PartId,
        reason: String,
    },
    RowBind {
        id: PartId,
        fields: BTreeMap<String, String>,
    },
    RowEdit {
        id: PartId,
        before: BTreeMap<String, String>,
        after: BTreeMap<String, String>,
    },
    HeaderChange {
        before: Vec<String>,
        after: Vec<String>,
    },
    BulkChange {
        description: String,
        count: u32,
    },
}

impl Action {
    /// Project this action onto its discriminator. Used by the
    /// `Authorizer` policy table and the audit-log `action` column.
    pub fn kind(&self) -> ActionKind {
        match self {
            Action::RowAdd { .. } => ActionKind::RowAdd,
            Action::RowDelete { .. } => ActionKind::RowDelete,
            Action::RowVoid { .. } => ActionKind::RowVoid,
            Action::RowBind { .. } => ActionKind::RowBind,
            Action::RowEdit { .. } => ActionKind::RowEdit,
            Action::HeaderChange { .. } => ActionKind::HeaderChange,
            Action::BulkChange { .. } => ActionKind::BulkChange,
        }
    }
}

// -------------------------------------------------------------------
// Diff (ADR-019 — interface-sharpness gap #1, the keystone)
// -------------------------------------------------------------------
//
// Closes the most important gap from the foundation parallelism audit.
// Concrete `{adds, deletes, edits, header_changes}` shape with typed
// `DiffEdit` per-row before/after. The `Diff::classify` pure function
// is the FE preflight + CI authoritative classifier per ADR-016.

/// Structured diff over registry rows.
///
/// Concrete shape per ADR-019 §"Trait shape": adds, deletes, edits,
/// and header changes are separate vectors so the policy classifier
/// can match on shape without re-deriving it from a unified-diff
/// string. CI re-runs `Diff::classify` authoritatively per ADR-016.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diff {
    pub adds: Vec<DiffRow>,
    pub deletes: Vec<DiffRow>,
    pub edits: Vec<DiffEdit>,
    pub header_changes: Vec<HeaderChange>,
}

/// Row added to or deleted from the registry. `id` is optional only
/// for the unusual case of a header-only diff that touches no rows;
/// in normal use it is `Some`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffRow {
    pub id: Option<PartId>,
    pub fields: BTreeMap<String, String>,
}

/// Edit to an existing row. `changed_keys` is the precomputed set of
/// columns that actually changed value; classifiers can use it
/// without diffing `before` vs `after` themselves.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffEdit {
    pub id: PartId,
    pub before: BTreeMap<String, String>,
    pub after: BTreeMap<String, String>,
    pub changed_keys: Vec<String>,
}

/// Header (column-set) change for one CSV file in the data repo.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeaderChange {
    pub file: String,
    pub before: Vec<String>,
    pub after: Vec<String>,
}

/// Where a `Diff` came from. Tells consumers (audit log, policy
/// engine, FE preflight) which authority produced the row-set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DiffSource {
    /// Diff computed from `git diff <base>..<head>` (CI authority).
    FromGitDiff { base_sha: String, head_sha: String },
    /// Diff computed from a local FE/CLI batch queue (preflight).
    FromQueue { batch_label: String },
}

impl Diff {
    /// Classify this diff into a list of `Action`s per ADR-016.
    ///
    /// Pure function, no I/O. Identical implementation runs in:
    ///
    /// - the FE WASM module (preflight per ADR-019; advisory)
    /// - the CI policy step (authoritative per ADR-016 §"CI is the
    ///   policy authority")
    ///
    /// Closes interface-sharpness gap #1 (HARD): single classifier,
    /// canonical location.
    ///
    /// ### Classification rules
    ///
    /// - One `HeaderChange` Action per `header_changes` entry.
    /// - One `RowAdd` Action per `adds` entry. The row payload is
    ///   the row's fields encoded as `serde_json::Value::Object`.
    /// - One `RowDelete` Action per `deletes` entry that has an
    ///   `id`. Header-only deletes (no `id`) collapse into
    ///   `BulkChange` so the policy table doesn't lose them.
    /// - For each `edits` entry:
    ///   - if the edit binds a previously-unbound row (status
    ///     transition `Unbound -> Bound`) → `RowBind`
    ///   - if the edit voids a row (status transition
    ///     `* -> Void`) → `RowVoid`
    ///   - otherwise → `RowEdit`
    /// - `BulkChange` is reserved for the catch-all case (e.g.
    ///   schema-migration commits where the diff is too large to
    ///   classify per row).
    pub fn classify(&self) -> Vec<Action> {
        let mut out: Vec<Action> = Vec::new();

        // Headers first — they're typically the most policy-relevant.
        for hc in &self.header_changes {
            out.push(Action::HeaderChange {
                before: hc.before.clone(),
                after: hc.after.clone(),
            });
        }

        // Adds.
        for row in &self.adds {
            // Construct a JSON object from the row fields. This is
            // the payload the policy engine sees; alphabetic key
            // order is preserved by `BTreeMap::iter` and serde.
            let mut obj = serde_json::Map::new();
            if let Some(id) = &row.id {
                obj.insert("id".into(), Json::String(id.as_str().into()));
            }
            for (k, v) in &row.fields {
                obj.insert(k.clone(), Json::String(v.clone()));
            }
            out.push(Action::RowAdd {
                row: Json::Object(obj),
            });
        }

        // Deletes — without an `id`, fall through to BulkChange.
        let mut bulk_deletes: u32 = 0;
        for row in &self.deletes {
            if let Some(id) = &row.id {
                out.push(Action::RowDelete { id: id.clone() });
            } else {
                bulk_deletes += 1;
            }
        }
        if bulk_deletes > 0 {
            out.push(Action::BulkChange {
                description: "header-only deletes".into(),
                count: bulk_deletes,
            });
        }

        // Edits — distinguish bind / void / edit.
        for edit in &self.edits {
            let before_status = edit.before.get("status").map(String::as_str);
            let after_status = edit.after.get("status").map(String::as_str);
            match (before_status, after_status) {
                (Some("unbound"), Some("bound")) => {
                    out.push(Action::RowBind {
                        id: edit.id.clone(),
                        fields: edit.after.clone(),
                    });
                }
                (_, Some("void")) if before_status != Some("void") => {
                    let reason = edit
                        .after
                        .get("notes")
                        .cloned()
                        .unwrap_or_else(|| "void via edit".into());
                    out.push(Action::RowVoid {
                        id: edit.id.clone(),
                        reason,
                    });
                }
                _ => {
                    out.push(Action::RowEdit {
                        id: edit.id.clone(),
                        before: edit.before.clone(),
                        after: edit.after.clone(),
                    });
                }
            }
        }

        out
    }
}

// -------------------------------------------------------------------
// Authorization (ADR-020)
// -------------------------------------------------------------------

/// The output of `Authorizer::authorize(operator, action)` per
/// ADR-020.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthDecision {
    Allow,
    Warn { reason: String },
    Block { reason: String },
    RequiresElevation { approver_role: String },
}

// -------------------------------------------------------------------
// Signing (ADR-024)
// -------------------------------------------------------------------
//
// Note on type ownership: `Signature` lives here in `crates/domain/`,
// not in `crates/signing/`. `crates/signing/` defines the `SigningProvider`
// and `VerificationProvider` traits which operate on `Signature` values;
// those traits already depend on domain types (`Operator`, `KeyId`,
// `Timestamp`), so signing depends on domain — not the other way round.
// Storage and transport adapters consume `Signature` via the audit-log
// and proposal-payload columns (ADR-023 forward-compat) without
// pulling signing into their dependency closure.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum SigAlgorithm {
    GitCommitGpg,
    GitCommitSsh,
    /// Reserved per ADR-024 forward-compat; not produced by MVP code.
    SigstoreKeyless,
    /// Reserved per ADR-024 forward-compat.
    Cosign,
}

/// Signature record carried alongside every audit entry / proposal
/// per ADR-023 §"Schema forward-compatibility."
///
/// `#[non_exhaustive]` per ADR-024 so adding a `Sigstore` variant
/// later is not a breaking change for storage adapters that
/// round-trip the value byte-for-byte.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Signature {
    GitCommit {
        commit_sha: String,
        signer_key_id: KeyId,
    },
    /// Forward-compat per ADR-027 §Tier 2: round-tripped through
    /// every storage adapter even at MVP, never produced by MVP code.
    Sigstore {
        cert: Vec<u8>,
        sig: Vec<u8>,
        rekor_proof: RekorProof,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RekorProof {
    pub uuid: String,
    pub log_index: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum VerificationSource {
    GitVerifyCommit,
    GitHubVerifiedApi,
    SigstoreRekor,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Verification {
    Verified {
        at: Timestamp,
        source: VerificationSource,
    },
    Unverified {
        reason: String,
    },
    Invalid {
        reason: String,
    },
}

// -------------------------------------------------------------------
// Storage data types (ADR-018)
// -------------------------------------------------------------------

/// Status of a `Part` in the registry, per ADR-013.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartStatus {
    Unbound,
    Bound,
    Void,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("invalid PartStatus: {0:?}")]
pub struct PartStatusParseError(pub String);

impl fmt::Display for PartStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            PartStatus::Unbound => "unbound",
            PartStatus::Bound => "bound",
            PartStatus::Void => "void",
        })
    }
}

impl FromStr for PartStatus {
    type Err = PartStatusParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "unbound" => Ok(PartStatus::Unbound),
            "bound" => Ok(PartStatus::Bound),
            "void" => Ok(PartStatus::Void),
            other => Err(PartStatusParseError(other.into())),
        }
    }
}

/// One row of `registry.csv` per ADR-013 / ADR-018.
///
/// `signatures` and `chain_hash` are ADR-023 §"Schema forward-
/// compatibility" columns: present at MVP, populated trivially today
/// (one `Signature::GitCommit`, no chain hash), semantically
/// activated by future ADRs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Part {
    pub id: PartId,
    pub status: PartStatus,
    pub minted_at: Timestamp,
    pub batch: Option<String>,
    pub bound_at: Option<Timestamp>,
    /// `type` is a Rust keyword — serialised as `type` in CSV/JSON.
    #[serde(rename = "type")]
    pub type_: Option<String>,
    pub description: Option<String>,
    pub vendor: Option<String>,
    pub part_number: Option<String>,
    pub location: Option<String>,
    pub notes: Option<String>,
    /// ADR-023 forward-compat. Default `vec![]` round-trips correctly.
    #[serde(default)]
    pub signatures: Vec<Signature>,
    /// ADR-023 forward-compat. `None` at MVP.
    #[serde(default)]
    pub chain_hash: Option<Hash>,
}

/// One row of `print_log.csv` per ADR-015.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PrintEvent {
    pub id: PartId,
    pub printed_at: Timestamp,
    pub printed_by: OperatorRef,
    pub layout: String,
    pub size_mm: f64,
    pub extra: Json,
    pub copies: u32,
    pub output_mode: String,
    pub batch_label: Option<String>,
}

/// What an `AuditEntry` points at.
///
/// All variants use struct form (named fields) because `#[serde(tag)]`
/// internally-tagged enums cannot represent newtype variants over
/// non-struct types — the tag has no field to attach to. Struct
/// variants serialise as `{"kind": "part", "id": "..."}` etc.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TargetRef {
    Part { id: PartId },
    Batch { label: String },
    Diff { hash: Hash },
    File { repo: String, path: String },
}

/// Where an audit emit originated, per ADR-022 §"AuditSource".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuditSource {
    Cli { invocation_id: String },
    Web { session_id: String },
    Ci { workflow_run_id: String },
}

/// One row of `audit_log.csv` per ADR-022 §"AuditEntry shape."
///
/// `signatures` and `chain_hash` are ADR-023 §"Schema forward-
/// compatibility" columns: present at MVP, populated trivially today,
/// activated by future ADRs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub request_id: RequestId,
    pub timestamp: Timestamp,
    pub actor: Operator,
    /// Carries the full payload-bearing action, not just a kind. Use
    /// `action.kind()` for the audit-log discriminator column.
    pub action: Action,
    pub target: TargetRef,
    pub before: Option<Json>,
    pub after: Option<Json>,
    pub extra: Json,
    /// ADR-023 forward-compat. Default `vec![]` round-trips correctly.
    #[serde(default)]
    pub signatures: Vec<Signature>,
    /// ADR-023 forward-compat. `None` at MVP.
    #[serde(default)]
    pub chain_hash: Option<Hash>,
}

// -------------------------------------------------------------------
// Filter shapes (ADR-018 — interface-sharpness gap #3)
// -------------------------------------------------------------------
//
// Closes interface-sharpness gap #3 (HARD): concrete fields fixed
// here so #29 (storage adapter) can build queries against them.

/// Sort key for `PartFilter::sort_by`. Stable ordering per ADR-013
/// §"Sort stability."
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartSortKey {
    #[default]
    Id,
    MintedAtAsc,
    MintedAtDesc,
    Status,
}

/// Filter for `Repository::list_parts`. Default = list all, sorted by
/// id ascending.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartFilter {
    pub status: Option<Vec<PartStatus>>,
    pub batch: Option<String>,
    pub bound: Option<bool>,
    pub vendor_contains: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    #[serde(default)]
    pub sort_by: PartSortKey,
}

/// Filter for `Repository::list_audit_events`. ADR-018 +
/// ADR-022 fields.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditFilter {
    pub actor: Option<OperatorId>,
    pub action_kinds: Option<Vec<ActionKind>>,
    pub since: Option<Timestamp>,
    pub until: Option<Timestamp>,
    pub target: Option<TargetRef>,
    pub request_id: Option<RequestId>,
    pub limit: Option<u32>,
}

/// Filter for `Repository::list_print_events`. ADR-015 fields.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrintEventFilter {
    pub id: Option<PartId>,
    pub printed_by: Option<OperatorId>,
    pub since: Option<Timestamp>,
    pub until: Option<Timestamp>,
    pub batch: Option<String>,
    pub limit: Option<u32>,
}

// -------------------------------------------------------------------
// Proposal types (ADR-019)
// -------------------------------------------------------------------

/// Proposal payload submitted via `ProposalSink::submit`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposal {
    pub diff: Diff,
    pub batch_label: Option<String>,
    pub author: Operator,
    /// ADR-023 forward-compat: today populated with one
    /// `Signature::GitCommit`. Sigstore variants slot in without
    /// changing the type.
    #[serde(default)]
    pub signatures: Vec<Signature>,
    /// Pre-classification per ADR-016. **Advisory only.** CI re-runs
    /// `Diff::classify` authoritatively. Divergence is logged.
    pub change_classification: Vec<Action>,
    pub message: String,
    pub request_id: RequestId,
}

/// Reference to a submitted proposal returned by `ProposalSink::submit`.
///
/// `adapter` disambiguates which adapter produced this ref so a
/// multi-adapter audit log can route a `status(...)` call back to the
/// right sink.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposalRef {
    pub url: String,
    pub local_id: Option<String>,
    pub adapter: String,
}

/// Status of a submitted proposal per ADR-019.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProposalStatus {
    Open,
    Merged,
    Closed,
    RequiresReview,
    BlockedByPolicy { reason: String },
    Errored { reason: String },
}

// -------------------------------------------------------------------
// Tests
// -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_part_id() -> PartId {
        PartId::new("ABCDEFGHJKMNPQ").unwrap()
    }

    fn sample_operator() -> Operator {
        Operator {
            id: OperatorId("github:tester".into()),
            display_name: "Tester".into(),
            source: IdentitySource::GitConfig,
            verified_at: None,
            claims: BTreeMap::new(),
            pubkey: None,
        }
    }

    fn now() -> Timestamp {
        OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap()
    }

    // -----------------------------------------------------------
    // PartId — ADR-012 alphabet validation
    // -----------------------------------------------------------

    #[test]
    fn part_id_accepts_canonical_form() {
        let id = PartId::new("ABCDEFGHJKMNPQ").unwrap();
        assert_eq!(id.as_str(), "ABCDEFGHJKMNPQ");
        assert_eq!(id.to_string(), "ABCDEFGHJKMNPQ");
    }

    #[test]
    fn part_id_rejects_wrong_length() {
        let err = PartId::new("ABC").unwrap_err();
        assert_eq!(
            err,
            PartIdError::BadLength {
                expected: 14,
                got: 3
            }
        );
    }

    #[test]
    fn part_id_rejects_lookalikes() {
        // '0' is not in the canonical alphabet (Crockford-style).
        let err = PartId::new("0BCDEFGHJKMNPQ").unwrap_err();
        assert_eq!(err, PartIdError::BadAlphabet('0'));
    }

    #[test]
    fn part_id_round_trips_via_serde_json() {
        let id = sample_part_id();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"ABCDEFGHJKMNPQ\"");
        let back: PartId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }

    #[test]
    fn part_id_from_str() {
        let id: PartId = "ABCDEFGHJKMNPQ".parse().unwrap();
        assert_eq!(id, sample_part_id());
    }

    // -----------------------------------------------------------
    // PartStatus — Display + FromStr round-trip
    // -----------------------------------------------------------

    #[test]
    fn part_status_display_fromstr_roundtrip() {
        for s in [PartStatus::Unbound, PartStatus::Bound, PartStatus::Void] {
            let printed = s.to_string();
            let parsed: PartStatus = printed.parse().unwrap();
            assert_eq!(parsed, s);
        }
    }

    #[test]
    fn part_status_rejects_unknown() {
        assert!("retired".parse::<PartStatus>().is_err());
    }

    // -----------------------------------------------------------
    // Action — kind() returns the right ActionKind for every variant
    // -----------------------------------------------------------

    #[test]
    fn action_kind_for_every_variant() {
        let cases = [
            (
                Action::RowAdd {
                    row: Json::Object(serde_json::Map::new()),
                },
                ActionKind::RowAdd,
            ),
            (
                Action::RowDelete {
                    id: sample_part_id(),
                },
                ActionKind::RowDelete,
            ),
            (
                Action::RowVoid {
                    id: sample_part_id(),
                    reason: "x".into(),
                },
                ActionKind::RowVoid,
            ),
            (
                Action::RowBind {
                    id: sample_part_id(),
                    fields: BTreeMap::new(),
                },
                ActionKind::RowBind,
            ),
            (
                Action::RowEdit {
                    id: sample_part_id(),
                    before: BTreeMap::new(),
                    after: BTreeMap::new(),
                },
                ActionKind::RowEdit,
            ),
            (
                Action::HeaderChange {
                    before: vec![],
                    after: vec![],
                },
                ActionKind::HeaderChange,
            ),
            (
                Action::BulkChange {
                    description: "x".into(),
                    count: 0,
                },
                ActionKind::BulkChange,
            ),
        ];
        for (action, expected) in cases {
            assert_eq!(action.kind(), expected, "kind() for {action:?}");
        }
    }

    // -----------------------------------------------------------
    // Diff::classify — produces the right Action for each shape
    // -----------------------------------------------------------

    #[test]
    fn classify_empty_diff_yields_no_actions() {
        let diff = Diff::default();
        assert!(diff.classify().is_empty());
    }

    #[test]
    fn classify_header_change_emits_header_action() {
        let diff = Diff {
            header_changes: vec![HeaderChange {
                file: "registry.csv".into(),
                before: vec!["id".into(), "status".into()],
                after: vec!["id".into(), "status".into(), "vendor".into()],
            }],
            ..Default::default()
        };
        let actions = diff.classify();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].kind(), ActionKind::HeaderChange);
    }

    #[test]
    fn classify_row_add_emits_row_add_action() {
        let mut fields = BTreeMap::new();
        fields.insert("status".into(), "unbound".into());
        let diff = Diff {
            adds: vec![DiffRow {
                id: Some(sample_part_id()),
                fields,
            }],
            ..Default::default()
        };
        let actions = diff.classify();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].kind(), ActionKind::RowAdd);
    }

    #[test]
    fn classify_row_delete_emits_row_delete_action() {
        let diff = Diff {
            deletes: vec![DiffRow {
                id: Some(sample_part_id()),
                fields: BTreeMap::new(),
            }],
            ..Default::default()
        };
        let actions = diff.classify();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].kind(), ActionKind::RowDelete);
    }

    #[test]
    fn classify_row_bind_emits_row_bind_action() {
        let mut before = BTreeMap::new();
        before.insert("status".into(), "unbound".into());
        let mut after = BTreeMap::new();
        after.insert("status".into(), "bound".into());
        after.insert("vendor".into(), "Acme".into());
        let diff = Diff {
            edits: vec![DiffEdit {
                id: sample_part_id(),
                before,
                after,
                changed_keys: vec!["status".into(), "vendor".into()],
            }],
            ..Default::default()
        };
        let actions = diff.classify();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].kind(), ActionKind::RowBind);
    }

    #[test]
    fn classify_row_void_emits_row_void_action() {
        let mut before = BTreeMap::new();
        before.insert("status".into(), "bound".into());
        let mut after = BTreeMap::new();
        after.insert("status".into(), "void".into());
        after.insert("notes".into(), "decommissioned".into());
        let diff = Diff {
            edits: vec![DiffEdit {
                id: sample_part_id(),
                before,
                after,
                changed_keys: vec!["status".into(), "notes".into()],
            }],
            ..Default::default()
        };
        let actions = diff.classify();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::RowVoid { reason, .. } => assert_eq!(reason, "decommissioned"),
            other => panic!("expected RowVoid, got {other:?}"),
        }
    }

    #[test]
    fn classify_row_edit_emits_row_edit_action() {
        let mut before = BTreeMap::new();
        before.insert("status".into(), "bound".into());
        before.insert("location".into(), "L1".into());
        let mut after = BTreeMap::new();
        after.insert("status".into(), "bound".into());
        after.insert("location".into(), "L2".into());
        let diff = Diff {
            edits: vec![DiffEdit {
                id: sample_part_id(),
                before,
                after,
                changed_keys: vec!["location".into()],
            }],
            ..Default::default()
        };
        let actions = diff.classify();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].kind(), ActionKind::RowEdit);
    }

    #[test]
    fn classify_bulk_emits_bulk_when_deletes_lack_id() {
        let diff = Diff {
            deletes: vec![
                DiffRow {
                    id: None,
                    fields: BTreeMap::new(),
                },
                DiffRow {
                    id: None,
                    fields: BTreeMap::new(),
                },
            ],
            ..Default::default()
        };
        let actions = diff.classify();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::BulkChange { count, .. } => assert_eq!(*count, 2),
            other => panic!("expected BulkChange, got {other:?}"),
        }
    }

    // -----------------------------------------------------------
    // ADR-027 Tier 2 — round-trip with synthetic Sigstore-shaped
    // signature; also covers the no-signature MVP shape.
    // -----------------------------------------------------------

    fn sample_part(signatures: Vec<Signature>) -> Part {
        Part {
            id: sample_part_id(),
            status: PartStatus::Unbound,
            minted_at: now(),
            batch: Some("B-2026-05-08-sheet-1".into()),
            bound_at: None,
            type_: Some("PT100".into()),
            description: None,
            vendor: None,
            part_number: None,
            location: None,
            notes: None,
            signatures,
            chain_hash: None,
        }
    }

    fn sample_audit_entry(signatures: Vec<Signature>) -> AuditEntry {
        AuditEntry {
            request_id: RequestId(uuid::Uuid::nil()),
            timestamp: now(),
            actor: sample_operator(),
            action: Action::RowAdd {
                row: Json::Object(serde_json::Map::new()),
            },
            target: TargetRef::Part {
                id: sample_part_id(),
            },
            before: None,
            after: None,
            extra: Json::Object(serde_json::Map::new()),
            signatures,
            chain_hash: None,
        }
    }

    #[test]
    fn part_roundtrips_with_no_signatures() {
        let p = sample_part(vec![]);
        let json = serde_json::to_string(&p).unwrap();
        let back: Part = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn part_roundtrips_with_git_commit_signature() {
        let sig = Signature::GitCommit {
            commit_sha: "abc123".into(),
            signer_key_id: KeyId("k1".into()),
        };
        let p = sample_part(vec![sig]);
        let json = serde_json::to_string(&p).unwrap();
        let back: Part = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn part_roundtrips_with_sigstore_signature() {
        // ADR-027 §Tier 2: future Sigstore variant must round-trip
        // even at MVP, so activating it later is an adapter swap.
        let sig = Signature::Sigstore {
            cert: vec![1, 2, 3],
            sig: vec![4, 5, 6],
            rekor_proof: RekorProof {
                uuid: "rekor-uuid".into(),
                log_index: 42,
            },
        };
        let p = sample_part(vec![sig]);
        let json = serde_json::to_string(&p).unwrap();
        let back: Part = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn audit_entry_roundtrips_with_no_signatures() {
        let e = sample_audit_entry(vec![]);
        let json = serde_json::to_string(&e).unwrap();
        let back: AuditEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(e, back);
    }

    #[test]
    fn audit_entry_roundtrips_with_git_commit_signature() {
        let sig = Signature::GitCommit {
            commit_sha: "abc123".into(),
            signer_key_id: KeyId("k1".into()),
        };
        let e = sample_audit_entry(vec![sig]);
        let json = serde_json::to_string(&e).unwrap();
        let back: AuditEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(e, back);
    }

    #[test]
    fn audit_entry_roundtrips_with_sigstore_signature() {
        // ADR-027 §Tier 2 forward-shape on AuditEntry: the audit log must
        // round-trip a synthetic Sigstore variant today so activating it
        // later (ADR-023 trigger T2) is an adapter swap, not a schema
        // migration. Mirrors part_roundtrips_with_sigstore_signature.
        let sig = Signature::Sigstore {
            cert: vec![1, 2, 3],
            sig: vec![4, 5, 6],
            rekor_proof: RekorProof {
                uuid: "rekor-uuid".into(),
                log_index: 42,
            },
        };
        let e = sample_audit_entry(vec![sig]);
        let json = serde_json::to_string(&e).unwrap();
        let back: AuditEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(e, back);
    }
}
