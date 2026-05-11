//! ADR-027 §Tier 1 — SigningProvider conformance for `GitCommitSigner`.
//! Shared `port_tests::signing_provider_conformance` body is still a
//! stub; this file invokes it for wiring + asserts the pending-SHA
//! contract documented in `signing_git_commit/src/lib.rs`.

use std::collections::BTreeMap;

use part_registry_domain::{
    ActionKind, IdentitySource, KeyId, Operator, OperatorId, SigAlgorithm, Signature,
};
use part_registry_port_tests::signing_provider_conformance;
use part_registry_signing::{SigningContext, SigningProvider};
use part_registry_signing_git_commit::GitCommitSigner;

fn sample_operator() -> Operator {
    Operator {
        id: OperatorId("git-config:tester@example.com".into()),
        display_name: "Tester".into(),
        source: IdentitySource::GitConfig,
        verified_at: None,
        claims: BTreeMap::new(),
        pubkey: Some(KeyId("KEY".into())),
    }
}

#[test]
fn git_commit_signer_passes_generic_conformance() {
    let signer = GitCommitSigner::with_ssh(KeyId("KEY".into()));
    signing_provider_conformance(signer);
}

#[test]
fn git_commit_signer_sign_returns_placeholder_signature() {
    let signer = GitCommitSigner::with_ssh(KeyId("KEY".into()));
    assert_eq!(signer.algorithm(), SigAlgorithm::GitCommitSsh);
    let op = sample_operator();
    let ctx = SigningContext {
        operator: &op,
        payload: b"audit payload",
        action: ActionKind::RowAdd,
        timestamp: time::OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap(),
    };
    match signer.sign(&ctx).unwrap() {
        Signature::GitCommit {
            commit_sha,
            signer_key_id,
        } => {
            // ADR-024 pending-SHA pattern: at sign() time the commit
            // SHA isn't known; downstream patches it via
            // `PendingSignature::resolve` once the commit lands.
            assert_eq!(commit_sha, "");
            assert_eq!(signer_key_id, KeyId("KEY".into()));
        }
        other => panic!("expected GitCommit, got {other:?}"),
    }
}
