//! `qx-storage` — `Repository` trait per ADR-018.
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
//! everything from `qx_storage::*` without a separate
//! `qx_domain` import.

#![forbid(unsafe_code)]

use thiserror::Error;

// Re-export the storage-shaped domain types so adapters can do
// `use qx_storage::{Repository, Part, AuditEntry, ...};`
// without a separate `qx_domain` import. Per ADR-018 these
// types live in `crates/domain/`; the trait surface lives here.
pub use qx_domain::{
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

    /// Read: a generic collection's records as JSON objects (ADR-035
    /// entity store). The default serves nothing — only stores that hold
    /// collections beyond `parts` (the JSONL adapter) override it; the
    /// parts-only CSV adapter leaves non-parts collections empty.
    fn list_collection(
        &self,
        collection: &str,
    ) -> Result<Vec<serde_json::Map<String, serde_json::Value>>, RepoError> {
        let _ = collection;
        Ok(Vec::new())
    }

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
