//! `qx-storage-jsonl-git` — `Repository` adapter over JSONL
//! files per ADR-035 §4 (JSONL primary; one entity per line; CSV
//! demotes to the export path).
//!
//! ## Layout
//!
//! The adapter takes a path to a *local clone* of the data repo. It
//! does **not** clone from a remote — that responsibility belongs to
//! issue #35 (data-repo split). Three files are read/written under
//! that path:
//!
//! - `collections/parts.jsonl` — one serde-JSON [`Part`] per line,
//!   kept **sorted by id** so PR diffs stay line-stable (ADR-035 §4 /
//!   ADR-016 PR-diff-as-review).
//! - `audit_log.jsonl` — one serde-JSON [`AuditEntry`] per line,
//!   **append-only** (ADR-022): existing lines are never rewritten.
//! - `print_log.jsonl` — one serde-JSON `PrintEvent` per line,
//!   **append-only**. Per ADR-035 §0 guardrail 3 ("logs are streams,
//!   not collections — and there is exactly ONE stream") print events
//!   fold into the audit spine as a typed event kind in a later step;
//!   the file and the `list_print_events` / `append_print_event`
//!   methods exist today for ADR-018 trait parity only.
//!
//! The adapter is **read + audit-append only** per ADR-018 — there is
//! no `update_part` / `delete_part` on the trait. State changes to
//! `Part` records flow through `ProposalSink` (ADR-019, a separate
//! port). The inherent [`JsonlGitRepository::write_parts`] helper is
//! the seam those write paths (and test fixtures) use; it is not part
//! of the `Repository` surface.
//!
//! ## Atomic writes
//!
//! Every file write goes through tempfile-in-same-directory + atomic
//! rename, so readers never observe a half-written JSONL file even if
//! the process dies mid-write. Appends rewrite the whole file through
//! the same path but never alter existing lines (append-only is a
//! byte-level property, asserted in tests).
//!
//! ## Git integration: shell, not `git2`
//!
//! Audit-log appends optionally produce a signed git commit so the
//! ADR-013 "data = git history" property holds for the audit subset.
//! The commit is produced by **shelling out to `git`** via
//! `std::process::Command`, not via the `git2` crate — same rationale
//! as `storage_csv_git`: two operations (`git add` + `git commit -S`)
//! do not justify a ~3 MB libgit2 dependency, and `git` is already on
//! `PATH` per ADR-016.
//!
//! ## Snapshot hash
//!
//! [`Repository::snapshot_hash`] is SHA-256 over the three files in
//! the fixed, documented order `collections/parts.jsonl`,
//! `audit_log.jsonl`, `print_log.jsonl`. A missing file hashes as the
//! empty byte sequence, so a clone that has never printed hashes equal
//! to one with an empty `print_log.jsonl`.
//!
//! ## Wasm32 gating
//!
//! Filesystem and git-shell I/O are unavailable on
//! `wasm32-unknown-unknown`. The crate compiles cleanly on that
//! target but the full implementation is gated behind
//! `#[cfg(not(target_arch = "wasm32"))]`. The wasm32 build offers a
//! stub `JsonlGitRepository` whose methods return
//! [`qx_storage::RepoError::Backend`] with an "unsupported
//! target" message — same pattern as `storage_csv_git`.
//!
//! [`Part`]: qx_domain::Part
//! [`AuditEntry`]: qx_domain::AuditEntry
//! [`Repository::snapshot_hash`]: qx_storage::Repository::snapshot_hash

#![forbid(unsafe_code)]

#[cfg(not(target_arch = "wasm32"))]
mod imp;

#[cfg(not(target_arch = "wasm32"))]
pub use imp::{JsonlGitConfig, JsonlGitRepository};

// -----------------------------------------------------------------
// wasm32 stub — `cargo build --target wasm32-unknown-unknown` compiles
// without pulling fs or process deps. Methods are unreachable in
// practice because the wasm façade (crates/wasm) does not depend on
// this crate; the stub exists only to make the workspace shape
// uniform (mirrors storage_csv_git).
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
    pub struct JsonlGitConfig {
        pub repo_path: PathBuf,
        pub commit_on_write: bool,
        pub signing_key_id: Option<String>,
    }

    /// Wasm32 stub. Construction succeeds (no I/O); every trait
    /// method returns [`RepoError::Backend`] with an "unsupported
    /// target" message.
    pub struct JsonlGitRepository {
        _cfg: JsonlGitConfig,
    }

    impl JsonlGitRepository {
        pub fn open(repo_path: PathBuf, cfg: JsonlGitConfig) -> Result<Self, RepoError> {
            let _ = repo_path;
            Ok(Self { _cfg: cfg })
        }
    }

    fn unsupported() -> RepoError {
        RepoError::Backend(Box::<dyn std::error::Error + Send + Sync>::from(
            "JsonlGitRepository is unsupported on wasm32-unknown-unknown (fs + git unavailable)",
        ))
    }

    impl Repository for JsonlGitRepository {
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
pub use wasm_stub::{JsonlGitConfig, JsonlGitRepository};
