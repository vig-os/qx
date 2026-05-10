//! `part-registry-identity-github-oauth` — first FE `IdentityProvider`
//! adapter per ADR-020. GitHub OAuth device flow yields a verified
//! GitHub user identity with `verified_at: <token issued_at>`.
//!
//! Foundation scaffold — body filled in during ADR-017 step 5.

#![forbid(unsafe_code)]

use part_registry_domain::Operator;
use part_registry_identity::{Capabilities, IdentityError, IdentityProvider};

#[derive(Default)]
pub struct GithubOauthIdentity {
    _client_id: String,
}

impl GithubOauthIdentity {
    pub fn new(client_id: impl Into<String>) -> Self {
        Self {
            _client_id: client_id.into(),
        }
    }
}

impl IdentityProvider for GithubOauthIdentity {
    fn current(&self) -> Result<Operator, IdentityError> {
        Err(IdentityError::NoIdentity(
            "GithubOauthIdentity::current not implemented (foundation scaffold)".into(),
        ))
    }

    fn refresh(&self) -> Result<Operator, IdentityError> {
        Err(IdentityError::NoIdentity(
            "GithubOauthIdentity::refresh not implemented (foundation scaffold)".into(),
        ))
    }

    fn capabilities(&self, _op: &Operator) -> Capabilities {
        Capabilities::default()
    }
}
