//! `part-registry-identity` — `IdentityProvider` + `Authorizer` traits
//! per ADR-020. Authentication and authorization co-located because
//! they share `Operator` at every call site.
//!
//! Adapters live in sibling crates (`identity_git_config`,
//! `identity_github_oauth`, future `identity_oidc_generic`, etc.).

#![forbid(unsafe_code)]

use thiserror::Error;

use part_registry_domain::{Action, AuthDecision, Operator};

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("no identity available: {0}")]
    NoIdentity(String),
    #[error("verification failed: {0}")]
    VerificationFailed(String),
    #[error("backend error: {0}")]
    Backend(#[source] Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Clone, Debug, Default)]
pub struct Capabilities {
    pub can_propose: bool,
    pub can_approve_destructive: bool,
    pub roles: Vec<String>,
}

pub trait IdentityProvider: Send + Sync {
    fn current(&self) -> Result<Operator, IdentityError>;
    fn refresh(&self) -> Result<Operator, IdentityError>;
    fn capabilities(&self, op: &Operator) -> Capabilities;
}

pub trait Authorizer: Send + Sync {
    fn authorize(&self, op: &Operator, action: &Action) -> AuthDecision;
}
