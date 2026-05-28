//! Shared test helpers — build a `Wiring` against a tempdir-backed
//! `CsvGitRepository` + a `DryRunSink::in_memory`. No network.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tempfile::TempDir;

use part_registry_cli::{DryRunSink, DryRunTarget, Wiring};
use part_registry_domain::{Capabilities, IdentitySource, Operator, OperatorId, Proposal};
use part_registry_identity::{IdentityError, IdentityProvider};
use part_registry_storage::Repository;
use part_registry_storage_csv_git::{CsvGitConfig, CsvGitRepository};

/// Build a tempdir-backed `Repository` with empty `registry.csv` +
/// `print_log.csv` + `audit_log.csv`.
pub fn fresh_repo() -> (TempDir, Arc<dyn Repository>, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    // registry.csv with header
    std::fs::write(
        root.join("registry.csv"),
        "id,status,minted_at,batch,bound_at,type,description,vendor,part_number,location,notes,minted_by,bound_by,last_edited_at,last_edited_by,components,signatures,chain_hash\n",
    )
    .unwrap();
    std::fs::write(
        root.join("print_log.csv"),
        "id,printed_at,printed_by,layout,size_mm,extra,copies,output_mode,batch_label\n",
    )
    .unwrap();
    std::fs::write(
        root.join("audit_log.csv"),
        "request_id,timestamp,actor,action,target,before,after,extra,signatures,chain_hash\n",
    )
    .unwrap();

    let mut cfg = CsvGitConfig::new(root.clone());
    cfg.commit_on_write = false;
    let repo: Arc<dyn Repository> = Arc::new(CsvGitRepository::open(root.clone(), cfg).unwrap());
    (tmp, repo, root)
}

/// Stub `IdentityProvider` for tests.
pub struct FakeIdentity {
    op: Operator,
}

impl FakeIdentity {
    pub fn new() -> Self {
        Self {
            op: Operator {
                id: OperatorId("test:tester".into()),
                display_name: "Tester".into(),
                source: IdentitySource::GitConfig,
                verified_at: None,
                claims: BTreeMap::new(),
                pubkey: None,
            },
        }
    }
}

impl IdentityProvider for FakeIdentity {
    fn current(&self) -> Result<Operator, IdentityError> {
        Ok(self.op.clone())
    }
    fn refresh(&self) -> Result<Operator, IdentityError> {
        Ok(self.op.clone())
    }
    fn capabilities(&self, _op: &Operator) -> Capabilities {
        Capabilities::default()
    }
}

/// Build a `Wiring` with an in-memory dry-run sink. Returns the
/// proposal store so tests can assert on what got submitted.
pub fn fresh_wiring() -> (TempDir, Wiring, Arc<Mutex<Vec<Proposal>>>) {
    let (tmp, repo, root) = fresh_repo();
    let store = Arc::new(Mutex::new(Vec::new()));
    let sink = DryRunSink::new(DryRunTarget::Memory(store.clone()));
    let wiring = Wiring {
        repo,
        identity: Box::new(FakeIdentity::new()),
        sink: Box::new(sink),
        repo_root: root,
    };
    (tmp, wiring, store)
}

/// Same as `fresh_wiring`, but seeds `registry.csv` with one row per
/// (id, batch) pair.
pub fn seeded_wiring(rows: &[(&str, &str, &str)]) -> (TempDir, Wiring, Arc<Mutex<Vec<Proposal>>>) {
    let (tmp, wiring, store) = fresh_wiring();
    let path = wiring.repo_root.join("registry.csv");
    let mut s = std::fs::read_to_string(&path).unwrap();
    for (id, status, batch) in rows {
        // 18 columns: id..last_edited_by + components + signatures + chain_hash
        s.push_str(&format!(
            "{id},{status},2026-05-01T00:00:00Z,{batch},,,,,,,,,,,,,,\n"
        ));
    }
    std::fs::write(&path, s).unwrap();
    (tmp, wiring, store)
}

/// Same as `seeded_wiring`, but also lets the caller set notes per row.
/// Each tuple: (id, status, batch, notes).
#[allow(dead_code)]
pub fn seeded_wiring_with_notes(
    rows: &[(&str, &str, &str, &str)],
) -> (TempDir, Wiring, Arc<Mutex<Vec<Proposal>>>) {
    let (tmp, wiring, store) = fresh_wiring();
    let path = wiring.repo_root.join("registry.csv");
    let mut s = std::fs::read_to_string(&path).unwrap();
    for (id, status, batch, notes) in rows {
        // 18 columns: id(0)..notes(10),minted_by(11)..last_edited_by(14),
        // components(15),signatures(16),chain_hash(17)
        s.push_str(&format!(
            "{id},{status},2026-05-01T00:00:00Z,{batch},,,,,,,{notes},,,,,,,\n"
        ));
    }
    std::fs::write(&path, s).unwrap();
    (tmp, wiring, store)
}
