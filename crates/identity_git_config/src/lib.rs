//! `part-registry-identity-git-config` — first MVP `IdentityProvider`
//! adapter (CLI surface) per ADR-020.
//!
//! `source: GitConfig`, `verified_at: None` because the values are a
//! self-asserted claim, not a verified attestation.
//!
//! Foundation scaffold — body filled in during ADR-017 step 5.

#![forbid(unsafe_code)]

use part_registry_domain::Operator;
use part_registry_identity::{Capabilities, IdentityError, IdentityProvider};

#[derive(Default)]
pub struct GitConfigIdentity;

impl GitConfigIdentity {
    pub fn new() -> Self {
        Self
    }
}

impl IdentityProvider for GitConfigIdentity {
    fn current(&self) -> Result<Operator, IdentityError> {
        Err(IdentityError::NoIdentity(
            "GitConfigIdentity::current not implemented (foundation scaffold)".into(),
        ))
    }

    fn refresh(&self) -> Result<Operator, IdentityError> {
        Err(IdentityError::NoIdentity(
            "GitConfigIdentity::refresh not implemented (foundation scaffold)".into(),
        ))
    }

    fn capabilities(&self, _op: &Operator) -> Capabilities {
        Capabilities::default()
    }
}
