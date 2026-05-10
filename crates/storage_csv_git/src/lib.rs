//! `part-registry-storage-csv-git` — first `Repository` adapter per
//! ADR-018. CSV+git substrate as fixed by ADR-013 / ADR-015 / ADR-022.
//!
//! Foundation scaffold per ADR-017 §"Strangler-fig migration sequence"
//! step 4. Production logic intentionally absent.

#![forbid(unsafe_code)]

use std::path::PathBuf;

use part_registry_domain::{AuditEntry, Hash, PartId};
use part_registry_storage::{
    AuditFilter, Part, PartFilter, PrintEvent, PrintEventFilter, RepoError, Repository,
};

/// CSV-on-git adapter. Constructor takes the path to the data-repo
/// working tree per ADR-018 §"Repo split".
pub struct CsvGitRepository {
    _data_repo_path: PathBuf,
}

impl CsvGitRepository {
    pub fn new(data_repo_path: PathBuf) -> Self {
        Self {
            _data_repo_path: data_repo_path,
        }
    }
}

impl Repository for CsvGitRepository {
    fn get_part(&self, _id: &PartId) -> Result<Option<Part>, RepoError> {
        Err(RepoError::Other(
            "CsvGitRepository::get_part not implemented (foundation scaffold)".into(),
        ))
    }

    fn list_parts(&self, _filter: &PartFilter) -> Result<Vec<Part>, RepoError> {
        Err(RepoError::Other(
            "CsvGitRepository::list_parts not implemented (foundation scaffold)".into(),
        ))
    }

    fn list_audit_events(&self, _filter: &AuditFilter) -> Result<Vec<AuditEntry>, RepoError> {
        Err(RepoError::Other(
            "CsvGitRepository::list_audit_events not implemented (foundation scaffold)".into(),
        ))
    }

    fn list_print_events(&self, _filter: &PrintEventFilter) -> Result<Vec<PrintEvent>, RepoError> {
        Err(RepoError::Other(
            "CsvGitRepository::list_print_events not implemented (foundation scaffold)".into(),
        ))
    }

    fn append_audit_event(&self, _ev: AuditEntry) -> Result<(), RepoError> {
        Err(RepoError::Other(
            "CsvGitRepository::append_audit_event not implemented (foundation scaffold)".into(),
        ))
    }

    fn append_print_event(&self, _ev: PrintEvent) -> Result<(), RepoError> {
        Err(RepoError::Other(
            "CsvGitRepository::append_print_event not implemented (foundation scaffold)".into(),
        ))
    }

    fn snapshot_hash(&self) -> Result<Hash, RepoError> {
        Err(RepoError::Other(
            "CsvGitRepository::snapshot_hash not implemented (foundation scaffold)".into(),
        ))
    }
}
