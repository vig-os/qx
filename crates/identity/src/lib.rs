//! `part-registry-identity` — `IdentityProvider` + `Authorizer` traits
//! per ADR-020. Authentication and authorization co-located because
//! they share `Operator` at every call site.
//!
//! Adapters live in sibling crates (`identity_git_config`,
//! `identity_github_oauth`, future `identity_oidc_generic`, etc.).
//!
//! Per foundation issue #28: `Capabilities`, `Action`, `AuthDecision`
//! now live in `crates/domain/`. Re-exported here for adapter
//! convenience.

#![forbid(unsafe_code)]

use thiserror::Error;

use part_registry_domain::Operator;

// Re-export the policy / authorization types so adapters can do
// `use part_registry_identity::{IdentityProvider, Capabilities};`
// without a separate `part_registry_domain` import.
pub use part_registry_domain::{Action, AuthDecision, Capabilities};

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("no identity available: {0}")]
    NoIdentity(String),
    #[error("verification failed: {0}")]
    VerificationFailed(String),
    #[error("backend error: {0}")]
    Backend(#[source] Box<dyn std::error::Error + Send + Sync>),
}

pub trait IdentityProvider: Send + Sync {
    fn current(&self) -> Result<Operator, IdentityError>;
    fn refresh(&self) -> Result<Operator, IdentityError>;
    /// Per ADR-020 §"Capabilities": MVP adapters return
    /// `Capabilities::default()` — the MVP `Authorizer` reads
    /// `Operator::claims` directly. Future adapters populate this
    /// struct from richer claim sources (RBAC roles, ABAC attributes).
    fn capabilities(&self, op: &Operator) -> Capabilities;
}

pub trait Authorizer: Send + Sync {
    fn authorize(&self, op: &Operator, action: &Action) -> AuthDecision;
}
