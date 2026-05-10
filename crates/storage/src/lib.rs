//! `part-registry-storage` — `Repository` trait per ADR-018.
//!
//! Read + audit-append only. State-changing mutations to `Part`
//! records flow through `ProposalSink` (ADR-019); the trait deliberately
//! offers no direct write method — see ADR-018 §"Why read +
//! audit-append only" for the load-bearing rationale.
//!
//! Adapters live in sibling crates (`storage_csv_git`, future
//! `storage_sqlite`, etc.). This crate names no concrete adapter.
//!
//! Per foundation issue #28: `Part`, `PrintEvent`, `PartFilter`,
//! `AuditFilter`, `PrintEventFilter` now live in `crates/domain/`.
//! Re-exported here for adapter convenience so adapters import
//! everything from `part_registry_storage::*` without a separate
//! `part_registry_domain` import.

#![forbid(unsafe_code)]

use thiserror::Error;

// Re-export the storage-shaped domain types so adapters can do
// `use part_registry_storage::{Repository, Part, AuditEntry, ...};`
// without a separate `part_registry_domain` import. Per ADR-018 these
// types live in `crates/domain/`; the trait surface lives here.
pub use part_registry_domain::{
    AuditEntry, AuditFilter, Hash, Part, PartFilter, PartId, PrintEvent, PrintEventFilter,
};

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
