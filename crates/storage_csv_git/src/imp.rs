//! Host-target implementation of the CSV+git `Repository` adapter.
//!
//! See the crate-level docs in `lib.rs` for the architectural notes
//! (read + audit-append only per ADR-018, shell-vs-git2 rationale,
//! forward-compat column handling).

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use part_registry_domain::{
    Action, AuditEntry, Hash, Operator, OperatorId, OperatorRef, Part, PartId, PartSortKey,
    PartStatus, RequestId, Signature, TargetRef, Timestamp,
};
use part_registry_storage::{
    AuditFilter, PartFilter, PrintEvent, PrintEventFilter, RepoError, Repository,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use sha2::{Digest, Sha256};
use time::format_description::well_known::Rfc3339;

// -------------------------------------------------------------------
// Config
// -------------------------------------------------------------------

/// Adapter configuration.
///
/// `repo_path` is the local clone path of the data repo (per ADR-018
/// §"Repo split"). `commit_on_write` controls whether audit-log
/// appends produce a git commit; `signing_key_id` selects the GPG /
/// SSH key for `git commit -S` (falls back to `git config
/// user.signingkey` when `None`).
#[derive(Clone, Debug)]
pub struct CsvGitConfig {
    pub repo_path: PathBuf,
    pub commit_on_write: bool,
    pub signing_key_id: Option<String>,
}

impl CsvGitConfig {
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

pub struct CsvGitRepository {
    cfg: CsvGitConfig,
}

impl CsvGitRepository {
    /// Open the adapter against an existing local clone of the data
    /// repo. Verifies `repo_path` exists and contains the
    /// `registry.csv` file (the file may be header-only). Does NOT
    /// clone from a remote — that is #35's responsibility.
    pub fn open(repo_path: PathBuf, cfg: CsvGitConfig) -> Result<Self, RepoError> {
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
        let registry = repo_path.join("registry.csv");
        if !registry.exists() {
            return Err(RepoError::Backend(
                format!(
                    "expected registry.csv inside data repo: {}",
                    registry.display()
                )
                .into(),
            ));
        }
        let cfg = CsvGitConfig {
            repo_path,
            commit_on_write: cfg.commit_on_write,
            signing_key_id: cfg.signing_key_id,
        };
        Ok(Self { cfg })
    }

    fn registry_path(&self) -> PathBuf {
        self.cfg.repo_path.join("registry.csv")
    }

    fn print_log_path(&self) -> PathBuf {
        self.cfg.repo_path.join("print_log.csv")
    }

    fn audit_log_path(&self) -> PathBuf {
        self.cfg.repo_path.join("audit_log.csv")
    }

    fn read_parts(&self) -> Result<Vec<Part>, RepoError> {
        let path = self.registry_path();
        let mut rdr = csv::Reader::from_path(&path)
            .map_err(|e| RepoError::Backend(format!("read {}: {e}", path.display()).into()))?;
        let mut out = Vec::new();
        for rec in rdr.deserialize::<PartRow>() {
            let row = rec
                .map_err(|e| RepoError::SchemaMismatch(format!("registry.csv row decode: {e}")))?;
            out.push(row.into_domain()?);
        }
        Ok(out)
    }

    fn read_print_events(&self) -> Result<Vec<PrintEvent>, RepoError> {
        let path = self.print_log_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let mut rdr = csv::Reader::from_path(&path)
            .map_err(|e| RepoError::Backend(format!("read {}: {e}", path.display()).into()))?;
        let mut out = Vec::new();
        for rec in rdr.deserialize::<PrintEventRow>() {
            let row = rec
                .map_err(|e| RepoError::SchemaMismatch(format!("print_log.csv row decode: {e}")))?;
            out.push(row.into_domain()?);
        }
        Ok(out)
    }

    fn read_audit_events(&self) -> Result<Vec<AuditEntry>, RepoError> {
        let path = self.audit_log_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let mut rdr = csv::Reader::from_path(&path)
            .map_err(|e| RepoError::Backend(format!("read {}: {e}", path.display()).into()))?;
        let mut out = Vec::new();
        for rec in rdr.deserialize::<AuditEntryRow>() {
            let row = rec
                .map_err(|e| RepoError::SchemaMismatch(format!("audit_log.csv row decode: {e}")))?;
            out.push(row.into_domain()?);
        }
        Ok(out)
    }

    fn write_audit_events(&self, events: &[AuditEntry]) -> Result<(), RepoError> {
        let path = self.audit_log_path();
        let mut wtr = csv::Writer::from_path(&path)
            .map_err(|e| RepoError::Backend(format!("write {}: {e}", path.display()).into()))?;
        for ev in events {
            let row = AuditEntryRow::from_domain(ev)?;
            wtr.serialize(&row)
                .map_err(|e| RepoError::Backend(format!("serialize audit row: {e}").into()))?;
        }
        wtr.flush()
            .map_err(|e| RepoError::Backend(format!("flush audit_log.csv: {e}").into()))?;
        Ok(())
    }

    /// Run `git` in the data repo with the given args. Returns the
    /// process output on success; on non-zero exit, returns a
    /// [`RepoError::Backend`] carrying stderr.
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
        self.git(&["add", "audit_log.csv"])?;
        // Build the commit. `-S` signs; `--gpg-sign=<keyid>` overrides
        // git config if set; otherwise we rely on `user.signingkey`.
        let message = format!("audit: append entry (request_id={request_id})");
        let mut args: Vec<String> = vec!["commit".into()];
        if let Some(keyid) = &self.cfg.signing_key_id {
            args.push(format!("--gpg-sign={keyid}"));
        }
        args.push("-m".into());
        args.push(message);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.git(&arg_refs)
    }
}

// -------------------------------------------------------------------
// Repository impl
// -------------------------------------------------------------------

impl Repository for CsvGitRepository {
    fn get_part(&self, id: &PartId) -> Result<Option<Part>, RepoError> {
        let all = self.read_parts()?;
        Ok(all.into_iter().find(|p| &p.id == id))
    }

    fn list_parts(&self, filter: &PartFilter) -> Result<Vec<Part>, RepoError> {
        let mut all = self.read_parts()?;

        // status filter
        if let Some(allowed) = &filter.status {
            all.retain(|p| allowed.contains(&p.status));
        }
        // batch filter — exact match against Part.batch (Option<String>).
        if let Some(needle) = &filter.batch {
            all.retain(|p| p.batch.as_deref() == Some(needle.as_str()));
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
        let mut iter: Vec<Part> = all.into_iter().skip(offset).collect();
        if let Some(lim) = filter.limit {
            iter.truncate(lim as usize);
        }
        Ok(iter)
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
        // Sort ascending by (timestamp, request_id) per ADR-022.
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

    fn list_print_events(&self, filter: &PrintEventFilter) -> Result<Vec<PrintEvent>, RepoError> {
        let mut all = self.read_print_events()?;
        if let Some(id) = &filter.id {
            all.retain(|e| &e.id == id);
        }
        if let Some(by) = &filter.printed_by {
            all.retain(|e| &e.printed_by.0 == by);
        }
        if let Some(since) = filter.since {
            all.retain(|e| e.printed_at >= since);
        }
        if let Some(until) = filter.until {
            all.retain(|e| e.printed_at <= until);
        }
        if let Some(batch) = &filter.batch {
            all.retain(|e| e.batch_label.as_deref() == Some(batch.as_str()));
        }
        all.sort_by(|a, b| {
            a.printed_at
                .cmp(&b.printed_at)
                .then_with(|| a.id.as_str().cmp(b.id.as_str()))
        });
        if let Some(lim) = filter.limit {
            all.truncate(lim as usize);
        }
        Ok(all)
    }

    fn append_audit_event(&self, ev: AuditEntry) -> Result<(), RepoError> {
        // Read current, append, sort by (timestamp, request_id), write.
        let mut events = self.read_audit_events()?;
        let request_id = ev.request_id;
        events.push(ev);
        events.sort_by(|a, b| {
            a.timestamp
                .cmp(&b.timestamp)
                .then_with(|| a.request_id.0.cmp(&b.request_id.0))
        });
        self.write_audit_events(&events)?;
        if self.cfg.commit_on_write {
            self.commit_audit_append(&request_id)?;
        }
        Ok(())
    }

    fn append_print_event(&self, ev: PrintEvent) -> Result<(), RepoError> {
        // Read existing print events, append, sort by (printed_at,
        // id), write back. The original on-disk `print_log.csv`
        // header (ADR-015) is preserved by the `PrintEventRow` shape.
        let mut events = self.read_print_events()?;
        events.push(ev);
        events.sort_by(|a, b| {
            a.printed_at
                .cmp(&b.printed_at)
                .then_with(|| a.id.as_str().cmp(b.id.as_str()))
        });
        let path = self.print_log_path();
        let mut wtr = csv::Writer::from_path(&path)
            .map_err(|e| RepoError::Backend(format!("write {}: {e}", path.display()).into()))?;
        for e in &events {
            let row = PrintEventRow::from_domain(e)?;
            wtr.serialize(&row)
                .map_err(|e| RepoError::Backend(format!("serialize print row: {e}").into()))?;
        }
        wtr.flush()
            .map_err(|e| RepoError::Backend(format!("flush print_log.csv: {e}").into()))?;
        Ok(())
    }

    fn snapshot_hash(&self) -> Result<Hash, RepoError> {
        // Deterministic SHA-256 over the concatenation of the three
        // CSV files in canonical name order. Files that do not exist
        // contribute the empty byte sequence (their header is
        // implicit). The hash is therefore reproducible per ADR-024
        // for any two clones with byte-equivalent CSVs.
        let mut hasher = Sha256::new();
        for name in ["registry.csv", "audit_log.csv", "print_log.csv"] {
            hasher.update(name.as_bytes());
            hasher.update([0u8]); // separator so name + content cannot collide with content
            let path = self.cfg.repo_path.join(name);
            if path.exists() {
                let bytes = read_bytes(&path)?;
                hasher.update(&bytes);
            }
            hasher.update([0u8]);
        }
        let digest = hasher.finalize();
        Ok(Hash(hex_lower(&digest)))
    }
}

fn read_bytes(path: &Path) -> Result<Vec<u8>, RepoError> {
    let mut f = fs::File::open(path)
        .map_err(|e| RepoError::Backend(format!("open {}: {e}", path.display()).into()))?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)
        .map_err(|e| RepoError::Backend(format!("read {}: {e}", path.display()).into()))?;
    Ok(buf)
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

// -------------------------------------------------------------------
// CSV row schemas
// -------------------------------------------------------------------
//
// Each `Row` type is a 1:1 mirror of one CSV's columns, in order, with
// every column a `String`. Optional columns serialise as empty
// strings; JSON columns serialise as compact JSON strings. The
// conversion to/from domain types is explicit so encode/decode is
// reviewable per ADR-018.

// --- registry.csv ---
// header: id,status,minted_at,batch,bound_at,type,description,vendor,part_number,location,notes,minted_by,bound_by,last_edited_at,last_edited_by,components,manufacturer_id,metadata,signatures,chain_hash

#[derive(Debug, Serialize, Deserialize)]
struct PartRow {
    id: String,
    status: String,
    minted_at: String,
    batch: String,
    bound_at: String,
    #[serde(rename = "type")]
    type_: String,
    description: String,
    vendor: String,
    part_number: String,
    location: String,
    notes: String,
    #[serde(default)]
    minted_by: String,
    #[serde(default)]
    bound_by: String,
    #[serde(default)]
    last_edited_at: String,
    #[serde(default)]
    last_edited_by: String,
    // #168: semicolon-separated child part IDs. Empty = not an assembly.
    #[serde(default)]
    components: String,
    // #171: manufacturer's own tracking number. Plain string.
    #[serde(default)]
    manufacturer_id: String,
    // #171: type-specific metadata as a JSON object string (single-line).
    #[serde(default)]
    metadata: String,
    // ADR-023 forward-compat. Optional in the CSV header so the
    // existing on-disk `registry.csv` (which predates this ADR)
    // continues to deserialise without these columns.
    #[serde(default)]
    signatures: String,
    #[serde(default)]
    chain_hash: String,
}

impl PartRow {
    fn into_domain(self) -> Result<Part, RepoError> {
        let id = PartId::new(self.id.clone()).map_err(|e| {
            RepoError::SchemaMismatch(format!("invalid part id {:?}: {e}", self.id))
        })?;
        let status = self.status.parse::<PartStatus>().map_err(|e| {
            RepoError::SchemaMismatch(format!("invalid status {:?}: {e}", self.status))
        })?;
        let minted_at = parse_ts(&self.minted_at).map_err(|e| {
            RepoError::SchemaMismatch(format!("invalid minted_at {:?}: {e}", self.minted_at))
        })?;
        let bound_at = if self.bound_at.is_empty() {
            None
        } else {
            Some(parse_ts(&self.bound_at).map_err(|e| {
                RepoError::SchemaMismatch(format!("invalid bound_at {:?}: {e}", self.bound_at))
            })?)
        };
        let signatures: Vec<Signature> = if self.signatures.is_empty() {
            Vec::new()
        } else {
            serde_json::from_str(&self.signatures)
                .map_err(|e| RepoError::SchemaMismatch(format!("invalid signatures JSON: {e}")))?
        };
        let chain_hash = if self.chain_hash.is_empty() {
            None
        } else {
            Some(Hash(self.chain_hash))
        };
        Ok(Part {
            id,
            status,
            minted_at,
            batch: opt(self.batch),
            bound_at,
            type_: opt(self.type_),
            description: opt(self.description),
            vendor: opt(self.vendor),
            part_number: opt(self.part_number),
            location: opt(self.location),
            notes: opt(self.notes),
            minted_by: opt(self.minted_by),
            bound_by: opt(self.bound_by),
            last_edited_at: opt(self.last_edited_at),
            last_edited_by: opt(self.last_edited_by),
            components: if self.components.is_empty() {
                Vec::new()
            } else {
                let mut ids: Vec<PartId> = self
                    .components
                    .split(';')
                    .map(|s| PartId::new(s.trim().to_string()))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| RepoError::SchemaMismatch(format!("invalid component id: {e}")))?;
                ids.sort();
                ids
            },
            manufacturer_id: opt(self.manufacturer_id),
            metadata: if self.metadata.trim().is_empty() {
                BTreeMap::new()
            } else {
                serde_json::from_str(&self.metadata)
                    .map_err(|e| RepoError::SchemaMismatch(format!("invalid metadata JSON: {e}")))?
            },
            signatures,
            chain_hash,
        })
    }

    #[allow(dead_code)] // write path lands with #19 / ProposalSink; required for round-trip test
    fn from_domain(p: &Part) -> Result<Self, RepoError> {
        Ok(Self {
            id: p.id.as_str().into(),
            status: p.status.to_string(),
            minted_at: fmt_ts(&p.minted_at)?,
            batch: p.batch.clone().unwrap_or_default(),
            bound_at: match &p.bound_at {
                Some(t) => fmt_ts(t)?,
                None => String::new(),
            },
            type_: p.type_.clone().unwrap_or_default(),
            description: p.description.clone().unwrap_or_default(),
            vendor: p.vendor.clone().unwrap_or_default(),
            part_number: p.part_number.clone().unwrap_or_default(),
            location: p.location.clone().unwrap_or_default(),
            notes: p.notes.clone().unwrap_or_default(),
            minted_by: p.minted_by.clone().unwrap_or_default(),
            bound_by: p.bound_by.clone().unwrap_or_default(),
            last_edited_at: p.last_edited_at.clone().unwrap_or_default(),
            last_edited_by: p.last_edited_by.clone().unwrap_or_default(),
            components: {
                let mut ids = p.components.clone();
                ids.sort();
                ids.iter()
                    .map(|id| id.as_str())
                    .collect::<Vec<_>>()
                    .join(";")
            },
            manufacturer_id: p.manufacturer_id.clone().unwrap_or_default(),
            metadata: if p.metadata.is_empty() {
                String::new()
            } else {
                // Single-line JSON for stable CSV diffs (BTreeMap → sorted keys).
                serde_json::to_string(&p.metadata)
                    .map_err(|e| RepoError::Backend(format!("encode metadata: {e}").into()))?
            },
            signatures: if p.signatures.is_empty() {
                String::new()
            } else {
                serde_json::to_string(&p.signatures)
                    .map_err(|e| RepoError::Backend(format!("encode signatures: {e}").into()))?
            },
            chain_hash: p
                .chain_hash
                .as_ref()
                .map(|h| h.0.clone())
                .unwrap_or_default(),
        })
    }
}

// --- print_log.csv ---
// header: id,printed_at,printed_by,layout,size_mm,extra,copies,output_mode,batch_label

#[derive(Debug, Serialize, Deserialize)]
struct PrintEventRow {
    id: String,
    printed_at: String,
    printed_by: String,
    layout: String,
    size_mm: f64,
    extra: String,
    copies: u32,
    output_mode: String,
    batch_label: String,
}

impl PrintEventRow {
    fn into_domain(self) -> Result<PrintEvent, RepoError> {
        let id = PartId::new(self.id.clone()).map_err(|e| {
            RepoError::SchemaMismatch(format!("invalid part id {:?}: {e}", self.id))
        })?;
        let printed_at = parse_ts(&self.printed_at).map_err(|e| {
            RepoError::SchemaMismatch(format!("invalid printed_at {:?}: {e}", self.printed_at))
        })?;
        let extra: Json = if self.extra.is_empty() {
            Json::Object(serde_json::Map::new())
        } else {
            serde_json::from_str(&self.extra)
                .map_err(|e| RepoError::SchemaMismatch(format!("invalid extra JSON: {e}")))?
        };
        Ok(PrintEvent {
            id,
            printed_at,
            printed_by: OperatorRef(OperatorId(self.printed_by)),
            layout: self.layout,
            size_mm: self.size_mm,
            extra,
            copies: self.copies,
            output_mode: self.output_mode,
            batch_label: opt(self.batch_label),
        })
    }

    fn from_domain(e: &PrintEvent) -> Result<Self, RepoError> {
        Ok(Self {
            id: e.id.as_str().into(),
            printed_at: fmt_ts(&e.printed_at)?,
            printed_by: e.printed_by.0 .0.clone(),
            layout: e.layout.clone(),
            size_mm: e.size_mm,
            extra: if matches!(&e.extra, Json::Object(o) if o.is_empty()) {
                "{}".into()
            } else {
                serde_json::to_string(&e.extra)
                    .map_err(|err| RepoError::Backend(format!("encode extra: {err}").into()))?
            },
            copies: e.copies,
            output_mode: e.output_mode.clone(),
            batch_label: e.batch_label.clone().unwrap_or_default(),
        })
    }
}

// --- audit_log.csv ---
// header per ADR-022:
// request_id,timestamp,actor,action,target,before,after,extra,signatures,chain_hash

#[derive(Debug, Serialize, Deserialize)]
struct AuditEntryRow {
    request_id: String,
    timestamp: String,
    actor: String,
    action: String,
    target: String,
    before: String,
    after: String,
    extra: String,
    signatures: String,
    chain_hash: String,
}

impl AuditEntryRow {
    fn into_domain(self) -> Result<AuditEntry, RepoError> {
        let request_id = self
            .request_id
            .parse::<uuid::Uuid>()
            .map(RequestId)
            .map_err(|e| {
                RepoError::SchemaMismatch(format!("invalid request_id {:?}: {e}", self.request_id))
            })?;
        let timestamp = parse_ts(&self.timestamp).map_err(|e| {
            RepoError::SchemaMismatch(format!("invalid timestamp {:?}: {e}", self.timestamp))
        })?;
        let actor: Operator = serde_json::from_str(&self.actor)
            .map_err(|e| RepoError::SchemaMismatch(format!("invalid actor JSON: {e}")))?;
        let action: Action = serde_json::from_str(&self.action)
            .map_err(|e| RepoError::SchemaMismatch(format!("invalid action JSON: {e}")))?;
        let target: TargetRef = serde_json::from_str(&self.target)
            .map_err(|e| RepoError::SchemaMismatch(format!("invalid target JSON: {e}")))?;
        let before = parse_opt_json(&self.before, "before")?;
        let after = parse_opt_json(&self.after, "after")?;
        let extra: Json = if self.extra.is_empty() {
            Json::Object(serde_json::Map::new())
        } else {
            serde_json::from_str(&self.extra)
                .map_err(|e| RepoError::SchemaMismatch(format!("invalid extra JSON: {e}")))?
        };
        let signatures: Vec<Signature> = if self.signatures.is_empty() {
            Vec::new()
        } else {
            serde_json::from_str(&self.signatures)
                .map_err(|e| RepoError::SchemaMismatch(format!("invalid signatures JSON: {e}")))?
        };
        let chain_hash = if self.chain_hash.is_empty() {
            None
        } else {
            Some(Hash(self.chain_hash))
        };
        Ok(AuditEntry {
            request_id,
            timestamp,
            actor,
            action,
            target,
            before,
            after,
            extra,
            signatures,
            chain_hash,
        })
    }

    fn from_domain(e: &AuditEntry) -> Result<Self, RepoError> {
        Ok(Self {
            request_id: e.request_id.0.to_string(),
            timestamp: fmt_ts(&e.timestamp)?,
            actor: serde_json::to_string(&e.actor)
                .map_err(|err| RepoError::Backend(format!("encode actor: {err}").into()))?,
            action: serde_json::to_string(&e.action)
                .map_err(|err| RepoError::Backend(format!("encode action: {err}").into()))?,
            target: serde_json::to_string(&e.target)
                .map_err(|err| RepoError::Backend(format!("encode target: {err}").into()))?,
            before: match &e.before {
                Some(v) => serde_json::to_string(v)
                    .map_err(|err| RepoError::Backend(format!("encode before: {err}").into()))?,
                None => String::new(),
            },
            after: match &e.after {
                Some(v) => serde_json::to_string(v)
                    .map_err(|err| RepoError::Backend(format!("encode after: {err}").into()))?,
                None => String::new(),
            },
            extra: serde_json::to_string(&e.extra)
                .map_err(|err| RepoError::Backend(format!("encode extra: {err}").into()))?,
            signatures: if e.signatures.is_empty() {
                String::new()
            } else {
                serde_json::to_string(&e.signatures)
                    .map_err(|err| RepoError::Backend(format!("encode signatures: {err}").into()))?
            },
            chain_hash: e
                .chain_hash
                .as_ref()
                .map(|h| h.0.clone())
                .unwrap_or_default(),
        })
    }
}

fn parse_opt_json(s: &str, label: &str) -> Result<Option<Json>, RepoError> {
    if s.is_empty() {
        Ok(None)
    } else {
        Ok(Some(serde_json::from_str(s).map_err(|e| {
            RepoError::SchemaMismatch(format!("invalid {label} JSON: {e}"))
        })?))
    }
}

fn parse_ts(s: &str) -> Result<Timestamp, time::error::Parse> {
    Timestamp::parse(s, &Rfc3339)
}

fn fmt_ts(t: &Timestamp) -> Result<String, RepoError> {
    t.format(&Rfc3339)
        .map_err(|e| RepoError::Backend(format!("format timestamp: {e}").into()))
}

fn opt(s: String) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}
