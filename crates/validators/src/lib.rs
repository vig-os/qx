//! `part-registry-validators` — pure-function validators over registry
//! state. Repository-trait-agnostic per ADR-017 §"Strangler-fig
//! migration sequence" step 2.
//!
//! ADR-016 §"Classification classes" is implemented here via
//! `classify_diff`. CI is the policy authority; FE preflight calls
//! the same function to attach an advisory `Vec<ChangeClass>` to a
//! `Proposal` (ADR-019).

#![forbid(unsafe_code)]

use thiserror::Error;

use part_registry_domain::{ChangeClass, Diff};

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("schema mismatch: {0}")]
    Schema(String),
    #[error("sort instability: {0}")]
    SortInstability(String),
    #[error("foreign-key violation: {0}")]
    ForeignKey(String),
}

/// Opaque repository-state handle the pure validators consume.
/// Concrete shape lives in `crates/storage_csv_git/` once the CSV
/// adapter is fleshed out (ADR-017 step 4).
#[derive(Debug, Default)]
pub struct RepoState;

/// Validate the on-disk schema matches the ADR-013 / ADR-022 column set.
pub fn validate_schema(_repo_state: &RepoState) -> Result<(), ValidationError> {
    unimplemented!("foundation scaffold; ADR-017 step 2")
}

/// Validate sort stability per ADR-013 / ADR-015 / ADR-022 — re-sorting
/// the file equals the file byte-for-byte.
pub fn validate_sort_stable(_repo_state: &RepoState) -> Result<(), ValidationError> {
    unimplemented!("foundation scaffold; ADR-017 step 2")
}

/// Classify a diff per ADR-016. CI re-runs this authoritatively;
/// FE preflight ships its result advisory in `Proposal`.
pub fn classify_diff(_diff: &Diff) -> Vec<ChangeClass> {
    unimplemented!("foundation scaffold; ADR-016 + ADR-017 step 2")
}
