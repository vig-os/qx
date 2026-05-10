//! `part-registry-transport-github-pr` — first MVP `ProposalSink`
//! adapter per ADR-019. Opens GitHub PRs against the data repository
//! via the GitHub REST API (specific client crate is an implementation
//! detail; `octocrab` is the working assumption per ADR-019 §Decision).
//!
//! Foundation scaffold — body filled in during ADR-017 step 6.

#![forbid(unsafe_code)]

use part_registry_domain::{Proposal, ProposalRef};
use part_registry_transport::{ProposalError, ProposalSink, ProposalStatus};

pub struct GithubPrSink {
    _data_repo_url: String,
    _token: String,
}

impl GithubPrSink {
    pub fn new(data_repo_url: impl Into<String>, token: impl Into<String>) -> Self {
        Self {
            _data_repo_url: data_repo_url.into(),
            _token: token.into(),
        }
    }
}

impl ProposalSink for GithubPrSink {
    fn submit(&self, _proposal: Proposal) -> Result<ProposalRef, ProposalError> {
        Err(ProposalError::Other(
            "GithubPrSink::submit not implemented (foundation scaffold)".into(),
        ))
    }

    fn status(&self, _proposal_ref: &ProposalRef) -> Result<ProposalStatus, ProposalError> {
        Err(ProposalError::Other(
            "GithubPrSink::status not implemented (foundation scaffold)".into(),
        ))
    }
}
