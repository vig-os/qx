//! Integration test for the full audit-CSV pipeline.
//!
//! Wires the [`AuditCsvLayer`] through a real `CsvGitRepository` and
//! asserts that an emitted [`AuditEntry`] — including ADR-023 forward-
//! compat `signatures` (Sigstore variant) and `chain_hash` — round-
//! trips through `audit_log.csv` on disk byte-equivalently.
//!
//! This is the ADR-027 Tier 2 forward-shape evidence for the
//! observability adapter: the audit-CSV writer survives a synthetic
//! Sigstore signature without populating it.

use std::fs;
use std::process::Command;

use part_registry_domain::{
    Action, AuditEntry, AuditFilter, Hash, IdentitySource, Operator, OperatorId, PartId,
    RekorProof, RequestId, Signature, TargetRef,
};
use part_registry_observability::{emit_audit, request_id_span, AuditSinkHandle};
use part_registry_storage::Repository;
use part_registry_storage_csv_git::{CsvGitConfig, CsvGitRepository};
use tracing_subscriber::layer::SubscriberExt;

fn sample_operator() -> Operator {
    Operator {
        id: OperatorId("github:tester".into()),
        display_name: "Tester".into(),
        source: IdentitySource::GitConfig,
        verified_at: None,
        claims: Default::default(),
        pubkey: None,
    }
}

fn init_data_repo(path: &std::path::Path) {
    // Initialise a bare-bones data repo: registry.csv with just a header
    // (the CsvGitRepository open() checks for the file's existence).
    // Initialise a real git repo so commit_on_write works.
    fs::write(
        path.join("registry.csv"),
        "id,status,minted_at,batch,bound_at,type,description,vendor,part_number,location,notes,signatures,chain_hash\n",
    )
    .unwrap();
    let _ = Command::new("git").arg("init").current_dir(path).output();
    let _ = Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(path)
        .output();
    let _ = Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(path)
        .output();
    let _ = Command::new("git")
        .args(["config", "commit.gpgsign", "false"])
        .current_dir(path)
        .output();
    let _ = Command::new("git")
        .args(["add", "registry.csv"])
        .current_dir(path)
        .output();
    let _ = Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(path)
        .output();
}

#[test]
fn full_pipeline_writes_csv_with_sigstore_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let repo_path = tmp.path().to_path_buf();
    init_data_repo(&repo_path);

    let cfg = CsvGitConfig {
        repo_path: repo_path.clone(),
        commit_on_write: false, // tests pin git off to avoid signing-cfg dependencies
        signing_key_id: None,
    };
    let repo = CsvGitRepository::open(repo_path.clone(), cfg).expect("open repo");
    let handle = AuditSinkHandle::new(Box::new(repo));

    let rid = RequestId::new();
    let sig = Signature::Sigstore {
        cert: vec![1, 2, 3, 4],
        sig: vec![5, 6, 7, 8],
        rekor_proof: RekorProof {
            uuid: "rekor-uuid-99".into(),
            log_index: 99,
        },
    };
    let entry = AuditEntry {
        request_id: rid,
        timestamp: time::OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap(),
        actor: sample_operator(),
        action: Action::RowBind {
            id: PartId::new("ABCDEFGHJKMNPQ").unwrap(),
            fields: Default::default(),
        },
        target: TargetRef::Part {
            id: PartId::new("ABCDEFGHJKMNPQ").unwrap(),
        },
        before: None,
        after: None,
        extra: serde_json::Value::Object(Default::default()),
        signatures: vec![sig.clone()],
        chain_hash: Some(Hash("abc123".into())),
    };

    let layer = part_registry_observability::__test_audit_csv_layer(handle);
    let subscriber = tracing_subscriber::registry().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        let span = request_id_span("integration_test", rid);
        let _g = span.enter();
        emit_audit(&entry);
    });

    // Re-open and verify the row landed.
    let cfg2 = CsvGitConfig {
        repo_path: repo_path.clone(),
        commit_on_write: false,
        signing_key_id: None,
    };
    let repo2 = CsvGitRepository::open(repo_path.clone(), cfg2).expect("re-open");
    let read_back = repo2
        .list_audit_events(&AuditFilter::default())
        .expect("list");
    assert_eq!(read_back.len(), 1);
    assert_eq!(read_back[0].signatures, vec![sig]);
    assert_eq!(read_back[0].chain_hash, Some(Hash("abc123".into())));
    assert_eq!(read_back[0].request_id, rid);

    // Confirm `audit_log.csv` exists on disk with the ADR-022 header.
    let csv = fs::read_to_string(repo_path.join("audit_log.csv")).expect("audit_log.csv");
    let header = csv.lines().next().unwrap();
    assert_eq!(
        header,
        "request_id,timestamp,actor,action,target,before,after,extra,signatures,chain_hash"
    );
}
