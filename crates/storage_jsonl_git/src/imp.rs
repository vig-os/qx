//! Host-target implementation of the JSONL+git `Repository` adapter.
//!
//! See the crate-level docs in `lib.rs` for the architectural notes
//! (JSONL layout per ADR-035 §4, atomic writes, append-only logs,
//! shell-vs-git2 rationale, snapshot-hash order).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use qx_domain::{AuditEntry, Hash, Part, PartId, PartSortKey, PartStatus, RequestId};
use qx_storage::{AuditFilter, PartFilter, RepoError, Repository};
use sha2::{Digest, Sha256};

/// Relative path of the parts collection (ADR-035 §0/§4: a registry
/// is a set of collections under `collections/`).
const PARTS_FILE: &str = "collections/parts.jsonl";

/// Relative path of the append-only audit stream (ADR-022).
const AUDIT_FILE: &str = "audit_log.jsonl";

/// Fixed, documented file order for [`Repository::snapshot_hash`].
/// A missing file hashes as the empty byte sequence. Print events fold
/// into the audit spine (ADR-022 print-fold) — there is no print_log.
const SNAPSHOT_FILES: [&str; 2] = [PARTS_FILE, AUDIT_FILE];

// -------------------------------------------------------------------
// Config
// -------------------------------------------------------------------

/// Adapter configuration.
///
/// `repo_path` is the local clone path of the data repo (per ADR-018
/// §"Repo split"). `commit_on_write` controls whether audit-log
/// appends produce a git commit; `signing_key_id` selects the GPG /
/// SSH key for `git commit -S` (falls back to `git config
/// user.signingkey` when `None`). Same shape as `CsvGitConfig` so the
/// adapter swap (ADR-035 §4: jsonl becomes primary) is config-only.
#[derive(Clone, Debug)]
pub struct JsonlGitConfig {
    pub repo_path: PathBuf,
    pub commit_on_write: bool,
    pub signing_key_id: Option<String>,
}

impl JsonlGitConfig {
    pub fn new(repo_path: PathBuf) -> Self {
        Self {
            repo_path,
            commit_on_write: true,
            signing_key_id: None,
        }
    }
}

// -------------------------------------------------------------------
// Repository
// -------------------------------------------------------------------

pub struct JsonlGitRepository {
    cfg: JsonlGitConfig,
}

impl JsonlGitRepository {
    /// Open the adapter against an existing local clone of the data
    /// repo. Verifies `repo_path` exists and contains
    /// `collections/parts.jsonl` (the file may be empty). Does NOT
    /// clone from a remote — that is #35's responsibility.
    pub fn open(repo_path: PathBuf, cfg: JsonlGitConfig) -> Result<Self, RepoError> {
        if !repo_path.exists() {
            return Err(RepoError::Backend(
                format!("data repo path does not exist: {}", repo_path.display()).into(),
            ));
        }
        if !repo_path.is_dir() {
            return Err(RepoError::Backend(
                format!("data repo path is not a directory: {}", repo_path.display()).into(),
            ));
        }
        let parts = repo_path.join(PARTS_FILE);
        if !parts.exists() {
            return Err(RepoError::Backend(
                format!(
                    "expected {PARTS_FILE} inside data repo: {}",
                    parts.display()
                )
                .into(),
            ));
        }
        tracing::debug!(path = %repo_path.display(), "opened jsonl-git repository");
        let cfg = JsonlGitConfig {
            repo_path,
            commit_on_write: cfg.commit_on_write,
            signing_key_id: cfg.signing_key_id,
        };
        Ok(Self { cfg })
    }

    fn parts_path(&self) -> PathBuf {
        self.cfg.repo_path.join(PARTS_FILE)
    }

    fn audit_log_path(&self) -> PathBuf {
        self.cfg.repo_path.join(AUDIT_FILE)
    }

    fn read_parts(&self) -> Result<Vec<Part>, RepoError> {
        read_jsonl(&self.parts_path(), PARTS_FILE)
    }

    fn read_audit_events(&self) -> Result<Vec<AuditEntry>, RepoError> {
        read_jsonl(&self.audit_log_path(), AUDIT_FILE)
    }

    /// Rewrite `collections/parts.jsonl` from the given parts, sorted
    /// by id, via tempfile + atomic rename.
    ///
    /// This is **not** a `Repository` trait method — the port is
    /// read-and-audit-append only per ADR-018, and `Part` mutations
    /// flow through `ProposalSink` (ADR-019). The helper is the write
    /// seam those proposal-side paths (and test fixtures) use, and is
    /// the single place that enforces the ADR-035 §4 sort-by-id
    /// invariant (sort stability keeps PR diffs line-stable per
    /// ADR-016).
    pub fn write_parts(&self, parts: &[Part]) -> Result<(), RepoError> {
        let mut sorted: Vec<&Part> = parts.iter().collect();
        sorted.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        write_jsonl(&self.parts_path(), &sorted, PARTS_FILE)
    }

    /// Run `git` in the data repo with the given args. On non-zero
    /// exit, returns a [`RepoError::Backend`] carrying stderr.
    fn git(&self, args: &[&str]) -> Result<(), RepoError> {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.cfg.repo_path)
            .args(args)
            .output()
            .map_err(|e| RepoError::Backend(format!("git {args:?}: spawn: {e}").into()))?;
        if !out.status.success() {
            return Err(RepoError::Backend(
                format!(
                    "git {args:?} exited {}: {}",
                    out.status,
                    String::from_utf8_lossy(&out.stderr)
                )
                .into(),
            ));
        }
        Ok(())
    }

    fn commit_audit_append(&self, request_id: &RequestId) -> Result<(), RepoError> {
        // Stage the file.
        self.git(&["add", AUDIT_FILE])?;
        // Build the commit. `--gpg-sign=<keyid>` overrides git config
        // when set; otherwise we rely on `user.signingkey`.
        let message = format!("audit: append entry (request_id={request_id})");
        let mut args: Vec<String> = vec!["commit".into()];
        if let Some(keyid) = &self.cfg.signing_key_id {
            args.push(format!("--gpg-sign={keyid}"));
        }
        args.push("-m".into());
        args.push(message);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.git(&arg_refs)?;
        tracing::debug!(%request_id, "committed audit append");
        Ok(())
    }
}

// -------------------------------------------------------------------
// Repository impl
// -------------------------------------------------------------------

impl Repository for JsonlGitRepository {
    fn get_part(&self, id: &PartId) -> Result<Option<Part>, RepoError> {
        let all = self.read_parts()?;
        Ok(all.into_iter().find(|p| &p.id == id))
    }

    fn is_jsonl_native(&self) -> bool {
        true
    }

    fn list_collection(
        &self,
        collection: &str,
    ) -> Result<Vec<serde_json::Map<String, serde_json::Value>>, RepoError> {
        // Generic read of `collections/<name>.jsonl` (ADR-035 entity
        // store). A missing file is an empty collection, not an error.
        let path = self
            .cfg
            .repo_path
            .join(format!("collections/{collection}.jsonl"));
        read_jsonl::<serde_json::Map<String, serde_json::Value>>(&path, collection)
    }

    fn list_parts(&self, filter: &PartFilter) -> Result<Vec<Part>, RepoError> {
        let mut all = self.read_parts()?;

        // status filter
        if let Some(allowed) = &filter.status {
            all.retain(|p| allowed.contains(&p.status));
        }
        // bound filter — true iff status == Bound.
        if let Some(bound) = filter.bound {
            all.retain(|p| (p.status == PartStatus::Bound) == bound);
        }
        // vendor_contains — case-sensitive substring on Part.vendor.
        if let Some(needle) = &filter.vendor_contains {
            all.retain(|p| {
                p.vendor
                    .as_deref()
                    .map(|v| v.contains(needle.as_str()))
                    .unwrap_or(false)
            });
        }

        // Sort.
        match filter.sort_by {
            PartSortKey::Id => all.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str())),
            PartSortKey::MintedAtAsc => all.sort_by(|a, b| a.minted_at.cmp(&b.minted_at)),
            PartSortKey::MintedAtDesc => all.sort_by(|a, b| b.minted_at.cmp(&a.minted_at)),
            PartSortKey::Status => {
                all.sort_by(|a, b| status_order(a.status).cmp(&status_order(b.status)))
            }
        }

        // Offset + limit.
        let offset = filter.offset.unwrap_or(0) as usize;
        if offset >= all.len() {
            return Ok(Vec::new());
        }
        let mut page: Vec<Part> = all.into_iter().skip(offset).collect();
        if let Some(lim) = filter.limit {
            page.truncate(lim as usize);
        }
        Ok(page)
    }

    fn list_audit_events(&self, filter: &AuditFilter) -> Result<Vec<AuditEntry>, RepoError> {
        let mut all = self.read_audit_events()?;
        if let Some(actor) = &filter.actor {
            all.retain(|e| &e.actor.id == actor);
        }
        if let Some(kinds) = &filter.action_kinds {
            all.retain(|e| kinds.contains(&e.action.kind()));
        }
        if let Some(since) = filter.since {
            all.retain(|e| e.timestamp >= since);
        }
        if let Some(until) = filter.until {
            all.retain(|e| e.timestamp <= until);
        }
        if let Some(target) = &filter.target {
            all.retain(|e| &e.target == target);
        }
        if let Some(rid) = &filter.request_id {
            all.retain(|e| &e.request_id == rid);
        }
        // Sort ascending by (timestamp, request_id) per ADR-022. The
        // file itself stays in arrival order (append-only); ordering
        // is a read-side concern.
        all.sort_by(|a, b| {
            a.timestamp
                .cmp(&b.timestamp)
                .then_with(|| a.request_id.0.cmp(&b.request_id.0))
        });
        if let Some(lim) = filter.limit {
            all.truncate(lim as usize);
        }
        Ok(all)
    }

    fn append_audit_event(&self, ev: AuditEntry) -> Result<(), RepoError> {
        // Append-only: existing lines are never rewritten, the new
        // entry lands as one new line at the end (atomic rewrite via
        // tempfile + rename so a crash never leaves a torn file).
        let request_id = ev.request_id;
        let line = serde_json::to_string(&ev)
            .map_err(|e| RepoError::Backend(format!("encode audit entry: {e}").into()))?;
        append_jsonl_line(&self.audit_log_path(), &line, AUDIT_FILE)?;
        if self.cfg.commit_on_write {
            self.commit_audit_append(&request_id)?;
        }
        Ok(())
    }

    fn snapshot_hash(&self) -> Result<Hash, RepoError> {
        // Deterministic SHA-256 over the three JSONL files in the
        // fixed documented order (collections/parts.jsonl,
        // audit_log.jsonl, print_log.jsonl). Each file contributes
        // `name NUL content NUL` so name and content bytes cannot
        // collide across files; a missing file contributes the empty
        // byte sequence as its content. Reproducible per ADR-024 for
        // any two clones with byte-equivalent JSONL files.
        let mut hasher = Sha256::new();
        for name in SNAPSHOT_FILES {
            hasher.update(name.as_bytes());
            hasher.update([0u8]);
            let path = self.cfg.repo_path.join(name);
            if path.exists() {
                let bytes = fs::read(&path).map_err(|e| {
                    RepoError::Backend(format!("read {}: {e}", path.display()).into())
                })?;
                hasher.update(&bytes);
            }
            hasher.update([0u8]);
        }
        let digest = hasher.finalize();
        Ok(Hash(hex_lower(&digest)))
    }
}

// -------------------------------------------------------------------
// JSONL + atomic-write helpers
// -------------------------------------------------------------------

/// Read a JSONL file into a vector of `T`, one JSON document per
/// line. A missing file reads as empty (the trait treats absent logs
/// as "no events yet"); blank lines are skipped; any malformed line
/// is a [`RepoError::SchemaMismatch`] carrying the 1-based line
/// number.
fn read_jsonl<T: serde::de::DeserializeOwned>(
    path: &Path,
    label: &str,
) -> Result<Vec<T>, RepoError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(path)
        .map_err(|e| RepoError::Backend(format!("read {}: {e}", path.display()).into()))?;
    let mut out = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let item: T = serde_json::from_str(line)
            .map_err(|e| RepoError::SchemaMismatch(format!("{label} line {}: {e}", idx + 1)))?;
        out.push(item);
    }
    Ok(out)
}

/// Serialize `items` one-JSON-document-per-line and atomically
/// replace `path` with the result.
fn write_jsonl<T: serde::Serialize>(
    path: &Path,
    items: &[T],
    label: &str,
) -> Result<(), RepoError> {
    let mut buf = String::new();
    for item in items {
        let line = serde_json::to_string(item)
            .map_err(|e| RepoError::Backend(format!("encode {label} line: {e}").into()))?;
        buf.push_str(&line);
        buf.push('\n');
    }
    atomic_write(path, buf.as_bytes())
}

/// Append one already-serialized JSON line to a JSONL file without
/// rewriting any existing line: existing bytes are preserved verbatim
/// (a missing trailing newline is repaired first) and the whole file
/// is atomically replaced via tempfile + rename.
fn append_jsonl_line(path: &Path, line: &str, label: &str) -> Result<(), RepoError> {
    let mut buf = if path.exists() {
        fs::read_to_string(path)
            .map_err(|e| RepoError::Backend(format!("read {label}: {e}").into()))?
    } else {
        String::new()
    };
    if !buf.is_empty() && !buf.ends_with('\n') {
        buf.push('\n');
    }
    buf.push_str(line);
    buf.push('\n');
    atomic_write(path, buf.as_bytes())
}

/// Write `bytes` to `path` atomically: tempfile in the same directory
/// (so the rename never crosses a filesystem boundary), then rename
/// over the target. Creates the parent directory if needed.
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), RepoError> {
    let dir = path.parent().ok_or_else(|| {
        RepoError::Backend(format!("no parent directory for {}", path.display()).into())
    })?;
    fs::create_dir_all(dir)
        .map_err(|e| RepoError::Backend(format!("mkdir {}: {e}", dir.display()).into()))?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir)
        .map_err(|e| RepoError::Backend(format!("tempfile in {}: {e}", dir.display()).into()))?;
    tmp.write_all(bytes)
        .map_err(|e| RepoError::Backend(format!("write tempfile: {e}").into()))?;
    tmp.persist(path)
        .map_err(|e| RepoError::Backend(format!("rename over {}: {e}", path.display()).into()))?;
    Ok(())
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}

fn status_order(s: PartStatus) -> u8 {
    match s {
        PartStatus::Unbound => 0,
        PartStatus::Bound => 1,
        PartStatus::Void => 2,
    }
}
