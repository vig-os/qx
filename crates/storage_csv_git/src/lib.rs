//! `qx-storage-csv-git` — first `Repository` adapter per
//! ADR-018. CSV+git substrate as fixed by ADR-013 / ADR-015 / ADR-022.
//!
//! ## Layout
//!
//! The adapter takes a path to a *local clone* of the data repo. It
//! does **not** clone from a remote — that responsibility belongs to
//! issue #35 (data-repo split). Three files are read/written under
//! that path:
//!
//! - `registry.csv` — every `Part`, sorted by id (ADR-013).
//! - `print_log.csv` — every `PrintEvent`, sorted by `(printed_at,
//!   id)` (ADR-015).
//! - `audit_log.csv` — every `AuditEntry`, sorted by `(timestamp,
//!   request_id)` (ADR-022).
//!
//! The adapter is **read + audit-append only** per ADR-018 — there is
//! no `update_part` / `delete_part`. State changes to `Part` records
//! flow through `ProposalSink` (ADR-019, a separate port).
//!
//! ## Git integration: shell, not `git2`
//!
//! Audit-log appends optionally produce a signed git commit so the
//! ADR-013 "data = git history" property holds for the audit subset.
//! The commit is produced by **shelling out to `git`** via
//! `std::process::Command`, not via the `git2` crate. The shelling
//! choice is deliberate:
//!
//! - `git2` is a ~3 MB libgit2 C dependency that doubles the adapter's
//!   build surface and pins the project to a specific libgit2 minor
//!   release.
//! - The adapter needs exactly two operations (`git add` +
//!   `git commit -S`); the surface area of going through `git2` is
//!   wider than the surface we use.
//! - Existing CI runners already have `git` on `PATH` (per ADR-016),
//!   so the PATH dependency is paid anyway.
//!
//! If a future ADR needs programmatic access to commit SHAs without
//! parsing stdout, or wants to verify signatures from inside the
//! process, the switch to `git2` is local to this crate.
//!
//! ## Forward-compat columns (ADR-023)
//!
//! `signatures: Vec<Signature>` and `chain_hash: Option<Hash>` are
//! serialised as JSON-encoded columns on every row. The columns are
//! present from day one even though MVP code populates them
//! trivially. Round-tripping them blindly is the property the
//! ADR-027 §Tier 2 forward-shape tests assert.
//!
//! ## Wasm32 gating
//!
//! Filesystem and git-shell I/O are unavailable on
//! `wasm32-unknown-unknown`. The crate compiles cleanly on that
//! target but the full implementation is gated behind
//! `#[cfg(not(target_arch = "wasm32"))]`. The wasm32 build offers a
//! stub `CsvGitRepository` whose methods return
//! [`RepoError::Backend`] with an "unsupported target" message; this
//! keeps `cargo build --target wasm32-unknown-unknown -p
//! qx-wasm` green even though the wasm leaf does not
//! transitively depend on this crate today.

#![forbid(unsafe_code)]

#[cfg(not(target_arch = "wasm32"))]
mod imp;

#[cfg(not(target_arch = "wasm32"))]
pub use imp::{CsvGitConfig, CsvGitRepository};

// -----------------------------------------------------------------
// wasm32 stub — `cargo build --target wasm32-unknown-unknown` compiles
// without pulling fs or process deps. Methods are unreachable in
// practice because the wasm façade (crates/wasm) does not depend on
// this crate; the stub exists only to make the workspace shape
// uniform.
// -----------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
mod wasm_stub {
    use std::path::PathBuf;

    use qx_domain::{AuditEntry, Hash, PartId};
    use qx_storage::{AuditFilter, Part, PartFilter, RepoError, Repository};

    /// Config struct kept in-shape for wasm32 so downstream code
    /// referencing it under `cfg(target_arch = "wasm32")` does not
    /// break.
    #[derive(Clone, Debug)]
    pub struct CsvGitConfig {
        pub repo_path: PathBuf,
        pub commit_on_write: bool,
        pub signing_key_id: Option<String>,
    }

    /// Wasm32 stub. Construction succeeds (no I/O); every trait
    /// method returns [`RepoError::Backend`] with an "unsupported
    /// target" message.
    pub struct CsvGitRepository {
        _cfg: CsvGitConfig,
    }

    impl CsvGitRepository {
        pub fn open(repo_path: PathBuf, cfg: CsvGitConfig) -> Result<Self, RepoError> {
            let _ = repo_path;
            Ok(Self { _cfg: cfg })
        }
    }

    fn unsupported() -> RepoError {
        RepoError::Backend(Box::<dyn std::error::Error + Send + Sync>::from(
            "CsvGitRepository is unsupported on wasm32-unknown-unknown (fs + git unavailable)",
        ))
    }

    impl Repository for CsvGitRepository {
        fn get_part(&self, _id: &PartId) -> Result<Option<Part>, RepoError> {
            Err(unsupported())
        }
        fn list_parts(&self, _filter: &PartFilter) -> Result<Vec<Part>, RepoError> {
            Err(unsupported())
        }
        fn list_audit_events(&self, _filter: &AuditFilter) -> Result<Vec<AuditEntry>, RepoError> {
            Err(unsupported())
        }
        fn list_print_events(
            &self,
            _filter: &PrintEventFilter,
        ) -> Result<Vec<PrintEvent>, RepoError> {
            Err(unsupported())
        }
        fn append_audit_event(&self, _ev: AuditEntry) -> Result<(), RepoError> {
            Err(unsupported())
        }
        fn append_print_event(&self, _ev: PrintEvent) -> Result<(), RepoError> {
            Err(unsupported())
        }
        fn snapshot_hash(&self) -> Result<Hash, RepoError> {
            Err(unsupported())
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm_stub::{CsvGitConfig, CsvGitRepository};
