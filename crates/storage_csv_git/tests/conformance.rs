//! ADR-027 conformance tests for the CSV+git storage adapter.
//!
//! Tier 1 (trait conformance) hook: invokes the generic
//! `port_tests::repository_conformance` framework function. The
//! framework is currently a foundation stub (see
//! `crates/port_tests/src/lib.rs`); this file additionally exercises
//! the concrete adapter behaviours required by ADR-018 §"Trait shape"
//! and ADR-027 §Tier 2 (forward-shape round-trip) so the adapter is
//! actually verified end-to-end without waiting for the framework
//! body.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use part_registry_domain::{
    Action, AuditEntry, Hash, IdentitySource, KeyId, Operator, OperatorId, PartFilter, PartId,
    PartStatus, RekorProof, RequestId, Signature, TargetRef,
};
use part_registry_storage::{AuditFilter, PrintEventFilter, Repository};
use part_registry_storage_csv_git::{CsvGitConfig, CsvGitRepository};
use serde_json::json;
use tempfile::TempDir;
use time::OffsetDateTime;

// -------------------------------------------------------------------
// Fixture handling: tests copy the project-local fixtures into a
// TempDir-backed clone so they never touch the on-disk real CSVs.
// -------------------------------------------------------------------

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

/// Create a tempdir, copy `registry.csv` + `print_log.csv` fixtures
/// into it, and open a `CsvGitRepository` with `commit_on_write =
/// false` (no git commit produced — tests must never push or commit).
fn fresh_repo() -> (TempDir, CsvGitRepository) {
    let tmp = TempDir::new().expect("tempdir");
    copy_fixture(
        &fixtures_dir().join("registry.csv"),
        &tmp.path().join("registry.csv"),
    );
    copy_fixture(
        &fixtures_dir().join("print_log.csv"),
        &tmp.path().join("print_log.csv"),
    );
    let cfg = CsvGitConfig {
        repo_path: tmp.path().to_path_buf(),
        commit_on_write: false,
        signing_key_id: None,
    };
    let repo = CsvGitRepository::open(tmp.path().to_path_buf(), cfg).expect("open repo");
    (tmp, repo)
}

fn copy_fixture(src: &Path, dst: &Path) {
    let bytes = fs::read(src).unwrap_or_else(|e| panic!("read fixture {}: {e}", src.display()));
    fs::write(dst, bytes).unwrap_or_else(|e| panic!("write fixture {}: {e}", dst.display()));
}

// -------------------------------------------------------------------
// Tier 1 hook
// -------------------------------------------------------------------

/// ADR-027 §Tier 1: wire the adapter into the generic framework.
/// Body is a foundation stub today; the call confirms the shape
/// compiles + matches.
#[test]
fn csv_git_conforms_to_repository() {
    let (_tmp, repo) = fresh_repo();
    part_registry_port_tests::repository_conformance(repo);
}

// -------------------------------------------------------------------
// Read methods
// -------------------------------------------------------------------

#[test]
fn get_part_returns_some_for_known_id() {
    let (_tmp, repo) = fresh_repo();
    let id = PartId::new("26N4T5BU5FCGAB").unwrap();
    let part = repo.get_part(&id).unwrap();
    let part = part.expect("known part should exist");
    assert_eq!(part.id, id);
    assert_eq!(part.status, PartStatus::Bound);
}

#[test]
fn get_part_returns_none_for_missing_id() {
    let (_tmp, repo) = fresh_repo();
    let missing = PartId::new("ZZZZZZZZZZZZZZ").unwrap();
    assert!(repo.get_part(&missing).unwrap().is_none());
}

#[test]
fn list_parts_default_returns_all_in_stable_id_order() {
    let (_tmp, repo) = fresh_repo();
    let parts = repo.list_parts(&PartFilter::default()).unwrap();
    assert_eq!(parts.len(), 5);
    // PartFilter::default() sorts by id ascending.
    let mut ids: Vec<&str> = parts.iter().map(|p| p.id.as_str()).collect();
    let sorted = {
        let mut copy = ids.clone();
        copy.sort();
        copy
    };
    assert_eq!(ids, sorted, "default sort must be by id ascending");
    // Spot-check the actual order.
    ids.sort();
    assert_eq!(ids[0], "26N4T5BU5FCGAB");
    assert_eq!(ids[4], "5RHCG9G7CHKMJK");
}

#[test]
fn list_parts_filters_by_status() {
    let (_tmp, repo) = fresh_repo();
    let filter = PartFilter {
        status: Some(vec![PartStatus::Bound]),
        ..Default::default()
    };
    let parts = repo.list_parts(&filter).unwrap();
    assert_eq!(parts.len(), 2);
    assert!(parts.iter().all(|p| p.status == PartStatus::Bound));
}

#[test]
fn list_parts_filters_by_batch_and_vendor() {
    let (_tmp, repo) = fresh_repo();
    let filter = PartFilter {
        batch: Some("B-2026-05-08-sheet-2".into()),
        vendor_contains: Some("Beta".into()),
        ..Default::default()
    };
    let parts = repo.list_parts(&filter).unwrap();
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].id.as_str(), "4M4DWPCHD9PTGH");
}

#[test]
fn list_print_events_reads_fixture() {
    let (_tmp, repo) = fresh_repo();
    let events = repo
        .list_print_events(&PrintEventFilter::default())
        .unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].layout, "horz");
    assert_eq!(events[0].copies, 1);
}

// -------------------------------------------------------------------
// Audit-append round-trip (forward-compat per ADR-023 / ADR-027 §T2)
// -------------------------------------------------------------------

fn sample_actor() -> Operator {
    Operator {
        id: OperatorId("github:test-user".into()),
        display_name: "Test User".into(),
        source: IdentitySource::GitConfig,
        verified_at: None,
        claims: BTreeMap::new(),
        pubkey: None,
    }
}

fn sample_entry(request_id: RequestId, signatures: Vec<Signature>) -> AuditEntry {
    AuditEntry {
        request_id,
        timestamp: OffsetDateTime::from_unix_timestamp(1_715_000_000).unwrap(),
        actor: sample_actor(),
        action: Action::RowAdd {
            row: json!({"id": "26N4T5BU5FCGAB", "status": "unbound"}),
        },
        target: TargetRef::Part {
            id: PartId::new("26N4T5BU5FCGAB").unwrap(),
        },
        before: None,
        after: Some(json!({"status": "unbound"})),
        extra: json!({"note": "ingest"}),
        signatures,
        chain_hash: None,
    }
}

#[test]
fn append_audit_event_no_signatures_round_trips() {
    let (_tmp, repo) = fresh_repo();
    let rid = RequestId(uuid::Uuid::nil());
    let entry = sample_entry(rid, vec![]);
    repo.append_audit_event(entry.clone()).unwrap();

    let read = repo.list_audit_events(&AuditFilter::default()).unwrap();
    assert_eq!(read.len(), 1);
    assert_eq!(read[0], entry);
}

#[test]
fn append_audit_event_with_git_commit_signature_round_trips() {
    let (_tmp, repo) = fresh_repo();
    let sigs = vec![Signature::GitCommit {
        commit_sha: "deadbeef".into(),
        signer_key_id: KeyId("k1".into()),
    }];
    let entry = sample_entry(RequestId(uuid::Uuid::nil()), sigs);
    repo.append_audit_event(entry.clone()).unwrap();
    let read = repo.list_audit_events(&AuditFilter::default()).unwrap();
    assert_eq!(read.len(), 1);
    assert_eq!(read[0], entry);
}

#[test]
fn append_audit_event_with_sigstore_signature_round_trips() {
    // ADR-027 §Tier 2 forward-shape: MVP code never produces a
    // Sigstore variant, but storage must round-trip one byte-for-byte
    // so activating Sigstore later (ADR-024 successor) is an adapter
    // swap not a schema migration.
    let (_tmp, repo) = fresh_repo();
    let sigs = vec![Signature::Sigstore {
        cert: vec![1, 2, 3],
        sig: vec![4, 5, 6],
        rekor_proof: RekorProof {
            uuid: "rekor-uuid".into(),
            log_index: 42,
        },
    }];
    let mut entry = sample_entry(RequestId(uuid::Uuid::nil()), sigs);
    entry.chain_hash = Some(Hash("0123abcd".into()));
    repo.append_audit_event(entry.clone()).unwrap();
    let read = repo.list_audit_events(&AuditFilter::default()).unwrap();
    assert_eq!(read.len(), 1);
    assert_eq!(read[0], entry);
}

#[test]
fn append_multiple_audit_events_sorts_by_timestamp() {
    let (_tmp, repo) = fresh_repo();
    let mut later = sample_entry(RequestId(uuid::Uuid::from_u128(1)), vec![]);
    let mut earlier = sample_entry(RequestId(uuid::Uuid::from_u128(2)), vec![]);
    later.timestamp = OffsetDateTime::from_unix_timestamp(1_715_001_000).unwrap();
    earlier.timestamp = OffsetDateTime::from_unix_timestamp(1_714_999_000).unwrap();
    repo.append_audit_event(later.clone()).unwrap();
    repo.append_audit_event(earlier.clone()).unwrap();

    let read = repo.list_audit_events(&AuditFilter::default()).unwrap();
    assert_eq!(read.len(), 2);
    assert_eq!(read[0].timestamp, earlier.timestamp);
    assert_eq!(read[1].timestamp, later.timestamp);
}

// -------------------------------------------------------------------
// snapshot_hash determinism
// -------------------------------------------------------------------

#[test]
fn snapshot_hash_is_deterministic_for_same_state() {
    let (_tmp_a, repo_a) = fresh_repo();
    let (_tmp_b, repo_b) = fresh_repo();
    let h_a = repo_a.snapshot_hash().unwrap();
    let h_b = repo_b.snapshot_hash().unwrap();
    assert_eq!(h_a, h_b, "two clones with identical CSVs hash equal");
}

#[test]
fn snapshot_hash_changes_when_content_changes() {
    let (_tmp, repo) = fresh_repo();
    let before = repo.snapshot_hash().unwrap();
    repo.append_audit_event(sample_entry(RequestId(uuid::Uuid::nil()), vec![]))
        .unwrap();
    let after = repo.snapshot_hash().unwrap();
    assert_ne!(before, after);
}

// -------------------------------------------------------------------
// `open()` rejects nonsense paths
// -------------------------------------------------------------------

#[test]
fn open_rejects_missing_path() {
    let cfg = CsvGitConfig {
        repo_path: PathBuf::from("/does/not/exist/part-registry-test-xyz"),
        commit_on_write: false,
        signing_key_id: None,
    };
    assert!(CsvGitRepository::open(cfg.repo_path.clone(), cfg).is_err());
}

#[test]
fn open_rejects_directory_without_registry_csv() {
    let tmp = TempDir::new().unwrap();
    let cfg = CsvGitConfig {
        repo_path: tmp.path().to_path_buf(),
        commit_on_write: false,
        signing_key_id: None,
    };
    assert!(CsvGitRepository::open(tmp.path().to_path_buf(), cfg).is_err());
}
