//! `part-registry-signing-git-commit` — MVP `SigningProvider` adapter
//! per ADR-024. Does no cryptography itself; git already does that
//! correctly when commits are created. The adapter records the binding
//! between `AuditEntry` and the commit SHA it lands in.

#![forbid(unsafe_code)]

use part_registry_domain::{KeyId, SigAlgorithm, Signature};
use part_registry_signing::{SignError, SigningContext, SigningProvider};

pub struct GitCommitSigner {
    _signer_key_id: KeyId,
}

impl GitCommitSigner {
    pub fn new(signer_key_id: KeyId) -> Self {
        Self {
            _signer_key_id: signer_key_id,
        }
    }
}

impl SigningProvider for GitCommitSigner {
    fn algorithm(&self) -> SigAlgorithm {
        // MVP: caller can override at construction time once the real
        // git-config inspection lands. Default to SSH which matches
        // GitHub's current default for new accounts.
        SigAlgorithm::GitCommitSsh
    }

    fn sign(&self, _ctx: &SigningContext<'_>) -> Result<Signature, SignError> {
        // ADR-024 §Trait shape — record the binding from AuditEntry
        // to the commit SHA that contains it. The concrete pending-
        // commit lookup is wired in at strangler-fig step 5.
        Ok(Signature::GitCommit {
            commit_sha: "<unimplemented (foundation scaffold)>".into(),
            signer_key_id: self._signer_key_id.clone(),
        })
    }
}
