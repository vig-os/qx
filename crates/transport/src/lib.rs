//! `part-registry-transport` — `ProposalSink` trait per ADR-019.
//!
//! `submit + status` only. There is no `merge`, `close`, or `comment`
//! — acceptance belongs to the policy authority (CI + reviewers per
//! ADR-016), not the binary that authored the proposal. See ADR-019
//! §"Why submit + status only" for the load-bearing rationale.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use thiserror::Error;

use part_registry_domain::{Proposal, ProposalRef};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProposalStatus {
    Open,
    PolicyPending,
    Merged { sha: String },
    MergedAfterReview { sha: String, reviewer: String },
    Closed { reason: String },
    RequiresReview,
    BlockedByPolicy { reason: String },
}

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
