//! ADR-027 conformance tests for the JSONL+git storage adapter.
//!
//! Mirrors `storage_csv_git/tests/conformance.rs`: Tier 1 hook into
//! the generic `port_tests` framework, plus concrete adapter
//! behaviours per ADR-018 §"Trait shape", ADR-035 §4 (sorted-by-id
//! parts file, append-only logs) and ADR-027 §Tier 2 (forward-shape
//! round-trip).
//!
//! Fixtures are seeded programmatically (serde-JSON lines built from
//! the same domain values the adapter reads back) instead of static
//! files: JSONL lines embed `time`'s default serde timestamp format,
//! which is owned by the library, not hand-maintained fixtures. The
//! seeded data mirrors the csv_git fixture rows.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use qx_domain::{
    Action, ActionKind, AuditEntry, Hash, IdentitySource, KeyId, Operator, OperatorId, Part,
    PartFilter, PartId, PartStatus, RekorProof, RequestId, Signature, TargetRef, Timestamp,
};
use qx_storage::{AuditFilter, Repository};
use qx_storage_jsonl_git::{JsonlGitConfig, JsonlGitRepository};
use serde_json::json;
use tempfile::TempDir;
use time::macros::datetime;

// -------------------------------------------------------------------
// Seeded fixture data — mirrors storage_csv_git/tests/fixtures.
// -------------------------------------------------------------------

fn pid(s: &str) -> PartId {
    PartId::new(s).unwrap()
}

fn part(id: &str, status: PartStatus, bound_at: Option<Timestamp>, vendor: Option<&str>) -> Part {
    Part {
        id: pid(id),
        status,
        minted_at: datetime!(2026-05-08 13:00 UTC),
        bound_at,
        type_: None,
        description: None,
        vendor: vendor.map(Into::into),
        part_number: None,
        location: None,
        notes: None,
        minted_by: None,
        bound_by: None,
        last_edited_at: None,
        last_edited_by: None,
        components: Vec::new(),
        manufacturer_id: None,
        metadata: std::collections::BTreeMap::new(),
        signatures: vec![],
        chain_hash: None,
    }
}

/// Five parts mirroring the csv_git fixture rows (already sorted by id).
fn fixture_parts() -> Vec<Part> {
    vec![
        part(
            "26N4T5BU5FCGAB",
            PartStatus::Bound,
            Some(datetime!(2026-05-08 14:00 UTC)),
            Some("Acme"),
        ),
        part(
            "2Y5PZVD7PBK9CD",
            PartStatus::Bound,
            Some(datetime!(2026-05-08 14:30 UTC)),
            Some("Acme"),
        ),
        part("3NYKQ7D2GRX3EF", PartStatus::Unbound, None, None),
        part("4M4DWPCHD9PTGH", PartStatus::Unbound, None, Some("Beta")),
        part(
            "5RHCG9G7CHKMJK",
            PartStatus::Void,
            Some(datetime!(2026-05-08 15:00 UTC)),
            Some("Gamma"),
        ),
    ]
}
fn write_lines<T: serde::Serialize>(path: &Path, items: &[T]) {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).unwrap();
    }
    let mut buf = String::new();
    for item in items {
        buf.push_str(&serde_json::to_string(item).unwrap());
        buf.push('\n');
    }
    fs::write(path, buf).unwrap();
}

/// Create a tempdir seeded with `collections/parts.jsonl` and open a
/// `JsonlGitRepository` with
/// `commit_on_write = false` (tests must never commit or push).
fn fresh_repo() -> (TempDir, JsonlGitRepository) {
    let tmp = TempDir::new().expect("tempdir");
    write_lines(
        &tmp.path().join("collections").join("parts.jsonl"),
        &fixture_parts(),
    );
    let repo = open_at(tmp.path());
    (tmp, repo)
}

fn open_at(path: &Path) -> JsonlGitRepository {
    let cfg = JsonlGitConfig {
        repo_path: path.to_path_buf(),
        commit_on_write: false,
        signing_key_id: None,
    };
    JsonlGitRepository::open(path.to_path_buf(), cfg).expect("open repo")
}

/// Raw lines of a repo-relative JSONL file.
fn raw_lines(root: &Path, rel: &str) -> Vec<String> {
    fs::read_to_string(root.join(rel))
        .unwrap()
        .lines()
        .map(ToOwned::to_owned)
        .collect()
}

// -------------------------------------------------------------------
// Tier 1 hook
// -------------------------------------------------------------------

/// ADR-027 §Tier 1: wire the adapter into the generic framework.
/// Body is a foundation stub today; the call confirms the shape
/// compiles + matches.
#[test]
fn jsonl_git_conforms_to_repository() {
    let (_tmp, repo) = fresh_repo();
    qx_port_tests::repository_conformance(repo);
}

// -------------------------------------------------------------------
// Read methods
// -------------------------------------------------------------------

#[test]
fn get_part_returns_some_for_known_id() {
    let (_tmp, repo) = fresh_repo();
    let id = pid("26N4T5BU5FCGAB");
    let part = repo.get_part(&id).unwrap().expect("known part");
    assert_eq!(part.id, id);
    assert_eq!(part.status, PartStatus::Bound);
}

#[test]
fn get_part_returns_none_for_missing_id() {
    let (_tmp, repo) = fresh_repo();
    let missing = pid("ZZZZZZZZZZZZZZ");
    assert!(repo.get_part(&missing).unwrap().is_none());
}

#[test]
fn list_parts_default_returns_all_in_stable_id_order() {
    let (_tmp, repo) = fresh_repo();
    let parts = repo.list_parts(&PartFilter::default()).unwrap();
    assert_eq!(parts.len(), 5);
    let ids: Vec<&str> = parts.iter().map(|p| p.id.as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted, "default sort must be by id ascending");
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
        vendor_contains: Some("Beta".into()),
        ..Default::default()
    };
    let parts = repo.list_parts(&filter).unwrap();
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].id.as_str(), "4M4DWPCHD9PTGH");
}
// -------------------------------------------------------------------
// Parts write path — write_parts roundtrip + sorted-on-write
// (ADR-035 §4 sort-stability; ADR-027 §T2 forward shape on Part)
// -------------------------------------------------------------------

#[test]
fn write_parts_round_trips_including_forward_compat_fields() {
    let (_tmp, repo) = fresh_repo();
    let mut parts = fixture_parts();
    // Forward-compat per ADR-023 / ADR-027 §Tier 2: a Sigstore-shaped
    // signature + chain_hash must round-trip byte-for-byte.
    parts[0].signatures = vec![Signature::Sigstore {
        cert: vec![1, 2, 3],
        sig: vec![4, 5, 6],
        rekor_proof: RekorProof {
            uuid: "rekor-uuid".into(),
            log_index: 42,
        },
    }];
    parts[0].chain_hash = Some(Hash("0123abcd".into()));
    parts[1].signatures = vec![Signature::GitCommit {
        commit_sha: "deadbeef".into(),
        signer_key_id: KeyId("k1".into()),
    }];
    repo.write_parts(&parts).unwrap();

    let read = repo.list_parts(&PartFilter::default()).unwrap();
    assert_eq!(read, parts);
}

#[test]
fn write_parts_sorts_file_by_id() {
    let (tmp, repo) = fresh_repo();
    let mut shuffled = fixture_parts();
    shuffled.reverse();
    repo.write_parts(&shuffled).unwrap();

    let on_disk_ids: Vec<String> = raw_lines(tmp.path(), "collections/parts.jsonl")
        .iter()
        .map(|line| {
            let v: serde_json::Value = serde_json::from_str(line).unwrap();
            v["id"].as_str().unwrap().to_owned()
        })
        .collect();
    let mut sorted = on_disk_ids.clone();
    sorted.sort();
    assert_eq!(on_disk_ids, sorted, "parts.jsonl must be sorted by id");
    assert_eq!(on_disk_ids.len(), 5);
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
        timestamp: datetime!(2026-05-08 16:00 UTC),
        time_source: qx_domain::TimeSource::System,
        actor: sample_actor(),
        action: Action::RowAdd {
            row: json!({"id": "26N4T5BU5FCGAB", "status": "unbound"}),
        },
        target: TargetRef::Part {
            id: pid("26N4T5BU5FCGAB"),
        },
        before: None,
        after: Some(json!({"status": "unbound"})),
        extra: json!({"note": "ingest"}),
        signatures,
        chain_hash: None,
        content_hash: None,
    }
}

#[test]
fn append_audit_event_no_signatures_round_trips() {
    let (_tmp, repo) = fresh_repo();
    let entry = sample_entry(RequestId(uuid::Uuid::nil()), vec![]);
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
fn audit_file_is_append_only_and_list_sorts_by_timestamp() {
    let (tmp, repo) = fresh_repo();
    let mut later = sample_entry(RequestId(uuid::Uuid::from_u128(1)), vec![]);
    let mut earlier = sample_entry(RequestId(uuid::Uuid::from_u128(2)), vec![]);
    later.timestamp = datetime!(2026-05-08 17:00 UTC);
    earlier.timestamp = datetime!(2026-05-08 15:00 UTC);
    repo.append_audit_event(later.clone()).unwrap();
    let lines_after_first = raw_lines(tmp.path(), "audit_log.jsonl");
    repo.append_audit_event(earlier.clone()).unwrap();

    // Append-only at the byte level: the first line is untouched, the
    // second append landed strictly after it (arrival order on disk).
    let lines = raw_lines(tmp.path(), "audit_log.jsonl");
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], lines_after_first[0]);

    // Read-side ordering is (timestamp, request_id) per ADR-022.
    let read = repo.list_audit_events(&AuditFilter::default()).unwrap();
    assert_eq!(read.len(), 2);
    assert_eq!(read[0].timestamp, earlier.timestamp);
    assert_eq!(read[1].timestamp, later.timestamp);
}

#[test]
fn list_audit_events_applies_filters() {
    let (_tmp, repo) = fresh_repo();
    let keep = sample_entry(RequestId(uuid::Uuid::from_u128(7)), vec![]);
    let mut other = sample_entry(RequestId(uuid::Uuid::from_u128(8)), vec![]);
    other.action = Action::RowVoid {
        id: pid("5RHCG9G7CHKMJK"),
        reason: "decommissioned".into(),
    };
    repo.append_audit_event(keep.clone()).unwrap();
    repo.append_audit_event(other).unwrap();

    let by_rid = repo
        .list_audit_events(&AuditFilter {
            request_id: Some(keep.request_id),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(by_rid, vec![keep.clone()]);

    let by_kind = repo
        .list_audit_events(&AuditFilter {
            action_kinds: Some(vec![ActionKind::RowVoid]),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(by_kind.len(), 1);
    assert_eq!(by_kind[0].action.kind(), ActionKind::RowVoid);
}
// -------------------------------------------------------------------
// Missing-file behaviour
// -------------------------------------------------------------------

#[test]
fn missing_log_files_read_as_empty() {
    let tmp = TempDir::new().unwrap();
    write_lines(
        &tmp.path().join("collections").join("parts.jsonl"),
        &fixture_parts(),
    );
    let repo = open_at(tmp.path());
    assert!(repo
        .list_audit_events(&AuditFilter::default())
        .unwrap()
        .is_empty());
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
    assert_eq!(h_a, h_b, "two clones with identical JSONL files hash equal");
}

#[test]
fn snapshot_hash_is_stable_across_reopen() {
    let (tmp, repo) = fresh_repo();
    repo.append_audit_event(sample_entry(RequestId(uuid::Uuid::nil()), vec![]))
        .unwrap();
    let before = repo.snapshot_hash().unwrap();
    drop(repo);
    let reopened = open_at(tmp.path());
    let after = reopened.snapshot_hash().unwrap();
    assert_eq!(before, after, "hash is a function of file bytes only");
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

#[test]
fn snapshot_hash_treats_missing_file_as_empty() {
    // A repo without audit/print logs hashes equal to one where those
    // files exist but are empty — documented "missing hashes as empty"
    // semantics.
    let tmp_missing = TempDir::new().unwrap();
    write_lines(
        &tmp_missing.path().join("collections").join("parts.jsonl"),
        &fixture_parts(),
    );
    let repo_missing = open_at(tmp_missing.path());

    let tmp_empty = TempDir::new().unwrap();
    write_lines(
        &tmp_empty.path().join("collections").join("parts.jsonl"),
        &fixture_parts(),
    );
    fs::write(tmp_empty.path().join("audit_log.jsonl"), "").unwrap();
    let repo_empty = open_at(tmp_empty.path());

    assert_eq!(
        repo_missing.snapshot_hash().unwrap(),
        repo_empty.snapshot_hash().unwrap()
    );
}

// -------------------------------------------------------------------
// `open()` rejects nonsense paths
// -------------------------------------------------------------------

#[test]
fn open_rejects_missing_path() {
    let cfg = JsonlGitConfig {
        repo_path: PathBuf::from("/does/not/exist/qx-test-xyz"),
        commit_on_write: false,
        signing_key_id: None,
    };
    assert!(JsonlGitRepository::open(cfg.repo_path.clone(), cfg).is_err());
}

#[test]
fn open_rejects_directory_without_parts_jsonl() {
    let tmp = TempDir::new().unwrap();
    let cfg = JsonlGitConfig {
        repo_path: tmp.path().to_path_buf(),
        commit_on_write: false,
        signing_key_id: None,
    };
    assert!(JsonlGitRepository::open(tmp.path().to_path_buf(), cfg).is_err());
}

// -------------------------------------------------------------------
// Malformed lines surface as SchemaMismatch
// -------------------------------------------------------------------

#[test]
fn malformed_jsonl_line_is_schema_mismatch() {
    let (tmp, repo) = fresh_repo();
    let path = tmp.path().join("collections").join("parts.jsonl");
    let mut text = fs::read_to_string(&path).unwrap();
    text.push_str("{not json\n");
    fs::write(&path, text).unwrap();
    let err = repo.list_parts(&PartFilter::default()).unwrap_err();
    assert!(
        matches!(err, qx_storage::RepoError::SchemaMismatch(_)),
        "expected SchemaMismatch, got {err:?}"
    );
}
