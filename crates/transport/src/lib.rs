//! `part-registry-transport` — `ProposalSink` trait per ADR-019.
//!
//! `submit + status` only. There is no `merge`, `close`, or `comment`
//! — acceptance belongs to the policy authority (CI + reviewers per
//! ADR-016), not the binary that authored the proposal. See ADR-019
//! §"Why submit + status only" for the load-bearing rationale.
//!
//! Per foundation issue #28: `Proposal`, `ProposalRef`,
//! `ProposalStatus`, `Diff`, `Action`/`ChangeClass` all live in
//! `crates/domain/`. Re-exported here for adapter convenience.

#![forbid(unsafe_code)]

use thiserror::Error;

// Re-export the proposal payload / status types so adapters can do
// `use part_registry_transport::{ProposalSink, Proposal, ProposalStatus};`
// without a separate `part_registry_domain` import.
pub use part_registry_domain::{Proposal, ProposalRef, ProposalStatus};

#[derive(Debug, Error)]
pub enum ProposalError {
    #[error("network: {0}")]
    Network(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("auth: {0}")]
    Auth(String),
    #[error("rate limited; retry after {retry_after_seconds}s")]
    RateLimited { retry_after_seconds: u64 },
    #[error("backend rejected proposal: {0}")]
    Rejected(String),
    #[error("backend error: {0}")]
    Backend(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("not implemented (foundation scaffold): {0}")]
    Other(String),
}

pub trait ProposalSink: Send + Sync {
    /// Submit a proposal. Does not block on CI / merge.
    fn submit(&self, proposal: Proposal) -> Result<ProposalRef, ProposalError>;

    /// Poll status. Stateless from the caller's perspective.
    fn status(&self, proposal_ref: &ProposalRef) -> Result<ProposalStatus, ProposalError>;
}
