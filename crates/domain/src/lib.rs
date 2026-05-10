//! `part-registry-domain` — pure data types shared across the workspace.
//!
//! Foundation scaffold per ADR-017. No I/O, no side effects, no
//! dependencies on other workspace crates. Every other crate in the
//! workspace can depend on this one; this crate depends on nothing
//! workspace-internal.
//!
//! Types here are referenced by:
//! - `Repository` (ADR-018) — `Part`, `AuditEntry`, `PrintEvent`
//! - `ProposalSink` (ADR-019) — `Proposal`, `ProposalRef`, `Diff`
//! - `IdentityProvider` / `Authorizer` (ADR-020) — `Operator`, `Action`
//! - `SigningProvider` / `VerificationProvider` (ADR-024) — `Signature`
//! - audit-CSV layer (ADR-022) — `AuditEntry` schema

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use time::OffsetDateTime;

// -------------------------------------------------------------------
// Identifiers and primitives
// -------------------------------------------------------------------

/// Canonical part identifier (e.g. `EX-PT100-2026-0001`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PartId(pub String);

/// UUIDv7 per ADR-022 — time-ordered, sortable in CSV cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RequestId(pub uuid::Uuid);

/// UTC timestamp; ADR-022 mandates ISO-8601 with `Z` suffix in CSV.
pub type Timestamp = OffsetDateTime;

/// Content / commit hash. Hex-encoded in CSV.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Hash(pub String);

/// Public-key identifier (GPG long key id, SSH fingerprint, etc.).
/// ADR-020 plumbs this through `Operator.pubkey` for ADR-024 forward-compat.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyId(pub String);

// -------------------------------------------------------------------
// Identity (ADR-020)
// -------------------------------------------------------------------

/// Canonical operator identifier (e.g. `github:lars-gerchow`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OperatorId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IdentitySource {
    GitConfig,
    GitHubOAuth,
    OidcGeneric { issuer: String },
    MtlsCert { fingerprint: String },
    EnvUser,
    OfflineClaim,
    SigstoreKeyless { fulcio: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Operator {
    pub id: OperatorId,
    pub display_name: String,
    pub source: IdentitySource,
    pub verified_at: Option<Timestamp>,
    pub claims: BTreeMap<String, String>,
    pub pubkey: Option<KeyId>,
}

/// Lightweight operator reference (id only) for embedding in audit
/// rows where the full claims map is not needed.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OperatorRef(pub OperatorId);

// -------------------------------------------------------------------
// Diff + change classification (ADR-016)
// -------------------------------------------------------------------

/// Structured diff over registry rows. Concrete row representation
/// is intentionally `Json` at scaffold time so the semantic-diff
/// classifier (ADR-016) can be fleshed out without renaming this
/// type. Future iteration may swap `Json` for a typed `Row` struct.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Diff {
    pub adds: Vec<Json>,
    pub deletes: Vec<Json>,
    pub edits: Vec<DiffEdit>,
    pub header_changes: Vec<HeaderChange>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiffEdit {
    pub before: Json,
    pub after: Json,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HeaderChange {
    pub before: Vec<String>,
    pub after: Vec<String>,
}

/// ADR-016 §"Classification classes": the seven row-shape categories
/// the semantic-diff classifier emits. CI is the policy authority;
/// FE preflight attaches an advisory `Vec<ChangeClass>` per ADR-019.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeClass {
    RowAdd,
    RowDelete,
    RowVoid,
    RowBind,
    RowEdit,
    HeaderChange,
    BulkChange,
}

// -------------------------------------------------------------------
// Action (ADR-016 / ADR-022)
// -------------------------------------------------------------------

/// `ActionKind` — discriminant for an `Action`. Mirrors the audit-log
/// `action` column per ADR-022.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    PolicyDecision,
    IdentityVerify,
}

/// One in-flight change the policy engine reasons about. Pairs the
/// kind with the change classes the semantic-diff classifier produced.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Action {
    pub kind: ActionKind,
    pub classes: Vec<ChangeClass>,
}

// -------------------------------------------------------------------
// Authorization (ADR-020)
// -------------------------------------------------------------------

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

/// `Signature` is `#[non_exhaustive]` per ADR-024 so adding the
/// `Sigstore` population path is not a breaking change for storage
/// adapters that round-trip the value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Signature {
    GitCommit {
        commit_sha: String,
        signer_key_id: KeyId,
    },
    /// Forward-compat. ADR-027 §Tier 2 round-trips this variant
    /// through every storage adapter even at MVP.
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
// AuditEntry (ADR-022) and TargetRef
// -------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AuditId(pub uuid::Uuid);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TargetRef {
    PartId(PartId),
    BatchLabel(String),
    Diff { sha: String },
    ProposalRef(String),
    None,
}

/// `AuditEntry` per ADR-022 §"AuditEntry shape". `signatures` and
/// `chain_hash` are ADR-023 forward-compat columns: present at MVP,
/// populated trivially today, semantically activated by future ADRs.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuditEntry {
    pub request_id: RequestId,
    pub timestamp: Timestamp,
    pub actor: Operator,
    pub action: ActionKind,
    pub target: TargetRef,
    pub before: Option<Json>,
    pub after: Option<Json>,
    pub extra: Json,
    pub signatures: Vec<Signature>,
    pub chain_hash: Option<Hash>,
}

// -------------------------------------------------------------------
// Proposal + ProposalRef (ADR-019)
// -------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Proposal {
    pub diff: Diff,
    pub batch_label: Option<String>,
    pub author: Operator,
    pub signatures: Vec<Signature>,
    pub change_classification: Vec<ChangeClass>,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProposalRef {
    pub url: String,
    pub local_id: Option<String>,
}
