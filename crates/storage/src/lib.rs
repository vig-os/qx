//! `part-registry-storage` — `Repository` trait per ADR-018.
//!
//! Read + audit-append only. State-changing mutations to `Part`
//! records flow through `ProposalSink` (ADR-019); the trait deliberately
//! offers no direct write method — see ADR-018 §"Why read +
//! audit-append only" for the load-bearing rationale.
//!
//! Adapters live in sibling crates (`storage_csv_git`, future
//! `storage_sqlite`, etc.). This crate names no concrete adapter.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use thiserror::Error;

use part_registry_domain::{AuditEntry, Hash, PartId};

#[derive(Debug, Error)]
pub enum RepoError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("schema mismatch: {0}")]
    SchemaMismatch(String),
    #[error("backend error: {0}")]
    Backend(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("not implemented (foundation scaffold): {0}")]
    Other(String),
}

// -------------------------------------------------------------------
// Filter types — placeholder shapes for the foundation scaffold.
// Concrete fields land with the strangler-fig step 4 PR.
// -------------------------------------------------------------------

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PartFilter {
    pub id: Option<PartId>,
    pub limit: Option<usize>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct AuditFilter {
    pub limit: Option<usize>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PrintEventFilter {
    pub limit: Option<usize>,
}

// -------------------------------------------------------------------
// Domain placeholders that strictly belong here (storage-shaped) but
// have not yet earned their own module in `domain`. `Part` and
// `PrintEvent` are placeholders; ADR-013 §Decision and ADR-015 fix
// the column sets that fill them in during step 4.
// -------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Part {
    pub id: PartId,
    // ADR-023 forward-compat columns are owned by `domain` once the
    // full `Part` lands; placeholder body keeps the trait compilable.
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintEvent {
    pub part_id: PartId,
}

// -------------------------------------------------------------------
// `Repository` trait (ADR-018 §"Trait shape")
// -------------------------------------------------------------------

pub trait Repository: Send + Sync {
    // Read: parts
    fn get_part(&self, id: &PartId) -> Result<Option<Part>, RepoError>;
    fn list_parts(&self, filter: &PartFilter) -> Result<Vec<Part>, RepoError>;

    // Read: audit log (per ADR-022)
    fn list_audit_events(&self, filter: &AuditFilter) -> Result<Vec<AuditEntry>, RepoError>;

    // Read: print events (per ADR-015)
    fn list_print_events(&self, filter: &PrintEventFilter) -> Result<Vec<PrintEvent>, RepoError>;

    // Append-only side effects.
    fn append_audit_event(&self, ev: AuditEntry) -> Result<(), RepoError>;
    fn append_print_event(&self, ev: PrintEvent) -> Result<(), RepoError>;

    /// Reproducibility hash per ADR-024 §Reproducible builds.
    fn snapshot_hash(&self) -> Result<Hash, RepoError>;
}
