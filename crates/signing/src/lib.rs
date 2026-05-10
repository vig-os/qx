//! `part-registry-signing` — `SigningProvider` + `VerificationProvider`
//! traits per ADR-024.
//!
//! `Signature` lives in `crates/domain/` and is `#[non_exhaustive]`
//! so adding a `Sigstore` adapter later does not require recompiling
//! every storage adapter.

#![forbid(unsafe_code)]

use thiserror::Error;

use part_registry_domain::{
    ActionKind, Operator, SigAlgorithm, Signature, Timestamp, Verification,
};

#[derive(Debug, Error)]
pub enum SignError {
    #[error("signing failed: {0}")]
    Failed(String),
    #[error("backend error: {0}")]
    Backend(#[source] Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Debug, Error)]
pub enum VerifyError {
    #[error("verification failed: {0}")]
    Failed(String),
    #[error("backend error: {0}")]
    Backend(#[source] Box<dyn std::error::Error + Send + Sync>),
}

pub struct SigningContext<'a> {
    pub operator: &'a Operator,
    pub payload: &'a [u8],
    pub action: ActionKind,
    pub timestamp: Timestamp,
}

pub trait SigningProvider: Send + Sync {
    fn algorithm(&self) -> SigAlgorithm;
    fn sign(&self, ctx: &SigningContext<'_>) -> Result<Signature, SignError>;
}

pub trait VerificationProvider: Send + Sync {
    fn algorithms(&self) -> &[SigAlgorithm];
    fn verify(
        &self,
        payload: &[u8],
        sig: &Signature,
        op: &Operator,
    ) -> Result<Verification, VerifyError>;
}
