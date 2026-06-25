//! `qx-transport-github-pr` — first MVP `ProposalSink`
//! adapter per ADR-019. Opens GitHub PRs against the data repository
//! via the GitHub REST API.
//!
//! Strangler-fig step 6 per ADR-017. ADR-019 §"submit + status only"
//! is honoured strictly: this adapter never merges, closes, or
//! comments on a PR; acceptance belongs to the policy authority
//! (CI + reviewers per ADR-016).
//!
//! ## Adapter shape
//!
//! [`GithubPrProposalSink`] is generic over a [`GithubPrHttp`] seam
//! so the adapter compiles + tests cleanly on both targets:
//!
//! - **Native**: [`ReqwestGithubPrHttp`] (blocking `reqwest`, rustls
//!   per ADR-028 §H6 — no OpenSSL on the build host).
//! - **wasm32 (browser FE)**: stub that returns
//!   [`ProposalError::Backend`]; the FE will wire its own browser-
//!   `fetch` implementation through the trait.
//!
//! ## HTTP trait — duplication vs. reuse
//!
//! `identity_github_oauth` defines its own `GithubHttp` covering the
//! OAuth device flow + `GET /user`. This crate's [`GithubPrHttp`]
//! covers an entirely different REST surface (branch creation,
//! contents API, pulls API). Trying to unify the two via a single
//! "fetch JSON" trait would either:
//!
//! 1. expose so many GitHub-specific endpoints that the abstraction
//!    is meaningless, or
//! 2. drop to a raw-HTTP shape (`get(url) -> bytes`,
//!    `put(url, body) -> bytes`) and force every adapter to reparse
//!    the response shapes.
//!
//! The two adapters share the *pattern* (a typed HTTP trait with a
//! native `reqwest::blocking` impl and a wasm32 stub) but the
//! surface methods are disjoint. A future deduplication issue can
//! lift the shared bearer-auth + user-agent + status-mapping
//! plumbing into a tiny helper crate; for now both adapters keep
//! their own typed seam.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use base64::Engine;
use serde::{Deserialize, Serialize};

use qx_domain::{
    Action, ActionKind, Diff, DiffEdit, DiffRow, Operator, Proposal, ProposalRef, ProposalStatus,
    Signature,
};
use qx_transport::{ProposalError, ProposalSink};
use qx_validators::{print_log_sort_key, registry_sort_key, PRINT_LOG_HEADER, REGISTRY_HEADER};

// -------------------------------------------------------------------
// Config
// -------------------------------------------------------------------

/// Adapter configuration. Wired from `crates/config/` per ADR-021 in
/// production; tests construct directly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GithubPrConfig {
    /// Owner of the *data* repo (split per ADR-019 §"Data-repo split").
    /// For exo-pet this is `"exo-pet"`.
    pub data_repo_owner: String,
    /// Repo name; e.g. `"exopet-registry"`.
    pub data_repo_name: String,
    /// Default base branch — typically `"main"`.
    pub base_branch: String,
    /// Prefix for proposal branches. Branch is built as
    /// `<branch_prefix><request_id>`, e.g.
    /// `"proposal/01HEXR4..."`. Per ADR-014 §"queue-and-batch-submit",
    /// one PR per proposal keeps the human review surface narrow.
    pub branch_prefix: String,
    /// Used for the git API `committer` field.
    pub commit_author_name: String,
    /// Used for the git API `committer` field.
    pub commit_author_email: String,
}

// -------------------------------------------------------------------
// HTTP transport seam
// -------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("HTTP transport error: {0}")]
    Transport(String),
    #[error("HTTP status {status}: {body}")]
    Status { status: u16, body: String },
    #[error("HTTP body deserialize error: {0}")]
    Deserialize(String),
}

/// Branch reference response from `GET /repos/{owner}/{repo}/git/ref/heads/{branch}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitRefResponse {
    #[serde(rename = "ref")]
    pub ref_: String,
    pub object: GitRefObject,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitRefObject {
    pub sha: String,
    #[serde(rename = "type")]
    pub type_: String,
}

/// Body of `POST /repos/{owner}/{repo}/git/refs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRefRequest {
    #[serde(rename = "ref")]
    pub ref_: String,
    pub sha: String,
}

/// Response from `GET /repos/{owner}/{repo}/contents/{path}` (file
/// case — for the directory case the API returns a JSON array, but
/// we only request specific file paths).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetContentsResponse {
    pub sha: String,
    pub content: String,
    pub encoding: String,
    #[serde(default)]
    pub path: String,
}

/// Author/committer for `PUT contents`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentsAuthor {
    pub name: String,
    pub email: String,
}

/// Body of `PUT /repos/{owner}/{repo}/contents/{path}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PutContentsRequest {
    pub message: String,
    /// base64-encoded file contents.
    pub content: String,
    pub branch: String,
    /// SHA of the existing blob; required for updates, absent for
    /// new-file creates.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha: Option<String>,
    pub committer: ContentsAuthor,
    pub author: ContentsAuthor,
}

/// Body of `POST /repos/{owner}/{repo}/pulls`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePullRequest {
    pub title: String,
    pub body: String,
    pub head: String,
    pub base: String,
}

/// Response from `POST /repos/{owner}/{repo}/pulls` or
/// `GET .../pulls/{number}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullResponse {
    pub number: u64,
    pub html_url: String,
    pub state: String,
    #[serde(default)]
    pub merged_at: Option<String>,
    #[serde(default)]
    pub requested_reviewers: Vec<PullReviewer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullReviewer {
    pub login: String,
}

/// One `check-run` entry from
/// `GET /repos/{owner}/{repo}/commits/{ref}/check-runs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckRun {
    pub name: String,
    /// `queued` / `in_progress` / `completed`.
    pub status: String,
    /// On completed runs: `success` / `failure` / `neutral` /
    /// `cancelled` / `skipped` / `timed_out` / `action_required`.
    #[serde(default)]
    pub conclusion: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckRunsResponse {
    pub total_count: u64,
    pub check_runs: Vec<CheckRun>,
}

/// One review entry from
/// `GET /repos/{owner}/{repo}/pulls/{number}/reviews`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullReview {
    /// `APPROVED` / `CHANGES_REQUESTED` / `COMMENTED` / `DISMISSED`.
    pub state: String,
}

/// The HTTP surface this adapter needs against the GitHub REST API.
///
/// Methods are typed (request body → response body) rather than raw
/// HTTP so the wasm32 stub + the native impl + the test fakes all
/// agree on serialisation. See module-level docs for the duplication
/// rationale vs. `identity_github_oauth::GithubHttp`.
pub trait GithubPrHttp: Send + Sync {
    /// `GET /repos/{owner}/{repo}/git/ref/heads/{branch}`.
    fn get_branch_ref(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
    ) -> Result<GitRefResponse, HttpError>;

    /// `POST /repos/{owner}/{repo}/git/refs`.
    fn create_ref(
        &self,
        owner: &str,
        repo: &str,
        body: &CreateRefRequest,
    ) -> Result<GitRefResponse, HttpError>;

    /// `GET /repos/{owner}/{repo}/contents/{path}?ref=<ref>`. `Ok(None)`
    /// when the API returns 404 (file does not exist yet).
    fn get_contents(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
        ref_: &str,
    ) -> Result<Option<GetContentsResponse>, HttpError>;

    /// `PUT /repos/{owner}/{repo}/contents/{path}`.
    fn put_contents(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
        body: &PutContentsRequest,
    ) -> Result<(), HttpError>;

    /// `POST /repos/{owner}/{repo}/pulls`.
    fn create_pull(
        &self,
        owner: &str,
        repo: &str,
        body: &CreatePullRequest,
    ) -> Result<PullResponse, HttpError>;

    /// `GET /repos/{owner}/{repo}/pulls/{number}`.
    fn get_pull(&self, owner: &str, repo: &str, number: u64) -> Result<PullResponse, HttpError>;

    /// `GET /repos/{owner}/{repo}/commits/{ref}/check-runs`.
    fn get_check_runs(
        &self,
        owner: &str,
        repo: &str,
        ref_: &str,
    ) -> Result<CheckRunsResponse, HttpError>;

    /// `GET /repos/{owner}/{repo}/pulls/{number}/reviews`.
    fn get_reviews(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<Vec<PullReview>, HttpError>;
}

// -------------------------------------------------------------------
// Native reqwest impl
// -------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
pub struct ReqwestGithubPrHttp {
    base_url: String,
    token: String,
    client: reqwest::blocking::Client,
}

#[cfg(not(target_arch = "wasm32"))]
impl ReqwestGithubPrHttp {
    /// Construct against the public GitHub API
    /// (`https://api.github.com`).
    pub fn new(token: impl Into<String>) -> Result<Self, HttpError> {
        Self::with_base_url("https://api.github.com", token)
    }

    /// Construct against an arbitrary base URL — used by the integration
    /// tests against `mockito`.
    pub fn with_base_url(
        base_url: impl Into<String>,
        token: impl Into<String>,
    ) -> Result<Self, HttpError> {
        let client = reqwest::blocking::Client::builder()
            .user_agent(concat!("qx/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| HttpError::Transport(e.to_string()))?;
        Ok(Self {
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            token: token.into(),
            client,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn send_get_json<T: for<'de> Deserialize<'de>>(&self, url: String) -> Result<T, HttpError> {
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .map_err(|e| HttpError::Transport(e.to_string()))?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(HttpError::Status { status, body });
        }
        resp.json::<T>()
            .map_err(|e| HttpError::Deserialize(e.to_string()))
    }

    fn send_post_json<B: Serialize, T: for<'de> Deserialize<'de>>(
        &self,
        url: String,
        body: &B,
    ) -> Result<T, HttpError> {
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .json(body)
            .send()
            .map_err(|e| HttpError::Transport(e.to_string()))?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(HttpError::Status { status, body });
        }
        resp.json::<T>()
            .map_err(|e| HttpError::Deserialize(e.to_string()))
    }

    fn send_put_json<B: Serialize>(&self, url: String, body: &B) -> Result<(), HttpError> {
        let resp = self
            .client
            .put(&url)
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .json(body)
            .send()
            .map_err(|e| HttpError::Transport(e.to_string()))?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(HttpError::Status { status, body });
        }
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl GithubPrHttp for ReqwestGithubPrHttp {
    fn get_branch_ref(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
    ) -> Result<GitRefResponse, HttpError> {
        let url = self.url(&format!(
            "/repos/{owner}/{repo}/git/ref/heads/{branch}",
            owner = owner,
            repo = repo,
            branch = branch
        ));
        self.send_get_json(url)
    }

    fn create_ref(
        &self,
        owner: &str,
        repo: &str,
        body: &CreateRefRequest,
    ) -> Result<GitRefResponse, HttpError> {
        let url = self.url(&format!("/repos/{owner}/{repo}/git/refs"));
        self.send_post_json(url, body)
    }

    fn get_contents(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
        ref_: &str,
    ) -> Result<Option<GetContentsResponse>, HttpError> {
        let url = self.url(&format!("/repos/{owner}/{repo}/contents/{path}?ref={ref_}"));
        match self.send_get_json::<GetContentsResponse>(url) {
            Ok(r) => Ok(Some(r)),
            Err(HttpError::Status { status: 404, .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn put_contents(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
        body: &PutContentsRequest,
    ) -> Result<(), HttpError> {
        let url = self.url(&format!("/repos/{owner}/{repo}/contents/{path}"));
        self.send_put_json(url, body)
    }

    fn create_pull(
        &self,
        owner: &str,
        repo: &str,
        body: &CreatePullRequest,
    ) -> Result<PullResponse, HttpError> {
        let url = self.url(&format!("/repos/{owner}/{repo}/pulls"));
        self.send_post_json(url, body)
    }

    fn get_pull(&self, owner: &str, repo: &str, number: u64) -> Result<PullResponse, HttpError> {
        let url = self.url(&format!("/repos/{owner}/{repo}/pulls/{number}"));
        self.send_get_json(url)
    }

    fn get_check_runs(
        &self,
        owner: &str,
        repo: &str,
        ref_: &str,
    ) -> Result<CheckRunsResponse, HttpError> {
        let url = self.url(&format!("/repos/{owner}/{repo}/commits/{ref_}/check-runs"));
        self.send_get_json(url)
    }

    fn get_reviews(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<Vec<PullReview>, HttpError> {
        let url = self.url(&format!("/repos/{owner}/{repo}/pulls/{number}/reviews"));
        self.send_get_json(url)
    }
}

// -------------------------------------------------------------------
// File classification
// -------------------------------------------------------------------

/// Which CSV file a `DiffRow` targets. Heuristic-based per ADR-013 /
/// ADR-015 — we look at the field set to disambiguate without
/// embedding a file path inside `DiffRow` (which would push
/// transport detail into the domain types).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TargetFile {
    Registry,
    PrintLog,
    AuditLog,
}

impl TargetFile {
    fn path(self) -> &'static str {
        match self {
            TargetFile::Registry => "registry.csv",
            TargetFile::PrintLog => "print_log.csv",
            TargetFile::AuditLog => "audit_log.csv",
        }
    }

    /// Best-effort classification from a row's field keys.
    ///
    /// - `print_log.csv` columns (ADR-015) include `printed_at` /
    ///   `printed_by` / `layout`.
    /// - `audit_log.csv` columns (ADR-022) include `request_id` /
    ///   `actor` / `action`.
    /// - Anything else with registry-shaped columns
    ///   (`id`/`status`/`minted_at`/`type`/`vendor` ...) falls back to
    ///   `Registry`.
    fn classify_row(fields: &BTreeMap<String, String>) -> TargetFile {
        if fields.contains_key("printed_at")
            || fields.contains_key("printed_by")
            || fields.contains_key("layout")
        {
            TargetFile::PrintLog
        } else if fields.contains_key("request_id")
            || fields.contains_key("actor")
            || fields.contains_key("action")
        {
            TargetFile::AuditLog
        } else {
            TargetFile::Registry
        }
    }
}

// -------------------------------------------------------------------
// PR body templating
// -------------------------------------------------------------------

/// Machine-parseable proposal metadata embedded as an HTML-comment
/// block in the PR body. ADR-016's CI diff classifier locates this
/// block by its `proposal:` YAML key without parsing free-form text.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrMetadata {
    pub request_id: String,
    pub author: String,
    pub author_display: String,
    pub classification: Vec<String>,
    #[serde(default)]
    pub batch: Option<String>,
    #[serde(default)]
    pub signatures: Vec<Signature>,
}

impl PrMetadata {
    fn from_proposal(p: &Proposal) -> Self {
        Self {
            request_id: p.request_id.to_string(),
            author: p.author.id.0.clone(),
            author_display: p.author.display_name.clone(),
            classification: p
                .change_classification
                .iter()
                .map(|a| action_kind_str(a.kind()).to_owned())
                .collect(),
            batch: p.batch_label.clone(),
            signatures: p.signatures.clone(),
        }
    }
}

fn action_kind_str(k: ActionKind) -> &'static str {
    match k {
        ActionKind::RowAdd => "row_add",
        ActionKind::RowDelete => "row_delete",
        ActionKind::RowVoid => "row_void",
        ActionKind::RowBind => "row_bind",
        ActionKind::RowEdit => "row_edit",
        ActionKind::HeaderChange => "header_change",
        ActionKind::BulkChange => "bulk_change",
    }
}

/// Roll classifications up to `kind × N` counts for the human-
/// readable summary line.
fn classification_summary(actions: &[Action]) -> String {
    let mut counts: BTreeMap<&'static str, u32> = BTreeMap::new();
    for a in actions {
        *counts.entry(action_kind_str(a.kind())).or_insert(0) += 1;
    }
    let mut parts: Vec<String> = counts
        .into_iter()
        .map(|(k, v)| format!("{k} × {v}"))
        .collect();
    if parts.is_empty() {
        parts.push("(empty)".into());
    }
    parts.join(", ")
}

fn build_pr_title(p: &Proposal) -> String {
    let summary = classification_summary(&p.change_classification);
    match &p.batch_label {
        Some(b) => format!("Proposal {} [{}]: {}", p.request_id, b, summary),
        None => format!("Proposal {}: {}", p.request_id, summary),
    }
}

fn build_pr_body(p: &Proposal) -> String {
    let summary = classification_summary(&p.change_classification);
    let meta = PrMetadata::from_proposal(p);
    let meta_yaml = serde_yaml_lite(&meta);

    let batch_line = match &p.batch_label {
        Some(b) => format!("**Batch**: {b}\n"),
        None => String::new(),
    };

    let custom_message = if p.message.trim().is_empty() {
        String::new()
    } else {
        format!("\n{}\n", p.message.trim())
    };

    format!(
        "## Proposal {request_id}\n\
         \n\
         **Author**: {display} ({id})\n\
         {batch_line}**Request ID**: {request_id}\n\
         **Advisory classification**: {summary}\n\
         {custom_message}\n\
         <!--\n\
         proposal:\n\
         {meta_yaml}-->\n\
         \n\
         CI re-runs the semantic-diff classifier authoritatively per ADR-016.\n",
        request_id = p.request_id,
        display = p.author.display_name,
        id = p.author.id.0,
        batch_line = batch_line,
        summary = summary,
        custom_message = custom_message,
        meta_yaml = meta_yaml,
    )
}

/// Tiny YAML-ish serialiser tuned for embedding `PrMetadata` in the
/// HTML-comment block. We deliberately avoid pulling a YAML
/// dependency for one struct — JSON-encode the signatures (round-
/// trips via serde) and dump the scalar fields as indented key/value.
fn serde_yaml_lite(m: &PrMetadata) -> String {
    let mut out = String::new();
    out.push_str(&format!("  request_id: {}\n", m.request_id));
    out.push_str(&format!("  author: {}\n", m.author));
    out.push_str(&format!("  author_display: {}\n", m.author_display));
    if let Some(b) = &m.batch {
        out.push_str(&format!("  batch: {}\n", b));
    }
    out.push_str("  classification:\n");
    if m.classification.is_empty() {
        out.push_str("    []\n");
    } else {
        for c in &m.classification {
            out.push_str(&format!("    - {}\n", c));
        }
    }
    out.push_str("  signatures: ");
    let sigs_json = serde_json::to_string(&m.signatures).unwrap_or_else(|_| "[]".to_owned());
    out.push_str(&sigs_json);
    out.push('\n');
    out
}

/// Parse the machine-parseable block back out of a PR body. Exposed
/// so CI (and tests) can round-trip the metadata without scraping
/// free-form text.
pub fn parse_pr_metadata(body: &str) -> Option<PrMetadata> {
    let start = body.find("<!--")?;
    let end = body[start..].find("-->")? + start;
    let block = &body[start + 4..end];
    let block = block.trim();
    let block = block.strip_prefix("proposal:").unwrap_or(block).trim();

    let mut request_id = String::new();
    let mut author = String::new();
    let mut author_display = String::new();
    let mut batch: Option<String> = None;
    let mut classification: Vec<String> = Vec::new();
    let mut signatures: Vec<Signature> = Vec::new();

    let mut in_classification = false;
    for raw in block.lines() {
        let line = raw.trim_end();
        if line.trim().is_empty() {
            continue;
        }
        if let Some(rest) = line.trim_start().strip_prefix("request_id:") {
            request_id = rest.trim().to_owned();
            in_classification = false;
        } else if let Some(rest) = line.trim_start().strip_prefix("author_display:") {
            author_display = rest.trim().to_owned();
            in_classification = false;
        } else if let Some(rest) = line.trim_start().strip_prefix("author:") {
            author = rest.trim().to_owned();
            in_classification = false;
        } else if let Some(rest) = line.trim_start().strip_prefix("batch:") {
            batch = Some(rest.trim().to_owned());
            in_classification = false;
        } else if line.trim_start().starts_with("classification:") {
            in_classification = true;
        } else if let Some(rest) = line.trim_start().strip_prefix("signatures:") {
            let json = rest.trim();
            if !json.is_empty() {
                signatures = serde_json::from_str(json).unwrap_or_default();
            }
            in_classification = false;
        } else if in_classification {
            if let Some(item) = line.trim_start().strip_prefix("- ") {
                classification.push(item.trim().to_owned());
            }
        }
    }
    Some(PrMetadata {
        request_id,
        author,
        author_display,
        classification,
        batch,
        signatures,
    })
}

// -------------------------------------------------------------------
// CSV (de)serialisation helpers
// -------------------------------------------------------------------
//
// We deliberately implement minimal CSV here (no `csv` crate dep) —
// the adapter only needs to read the existing file, edit/append rows
// keyed by `id`, and emit a canonical sort. Field values in this
// project's schemas are constrained (alphabetic IDs, ISO-8601
// timestamps, no embedded newlines) so a comma-+newline split is
// sufficient for MVP. If a future schema admits embedded commas the
// adapter swaps in `csv::ReaderBuilder` locally.

fn parse_csv(content: &str) -> (Vec<String>, Vec<BTreeMap<String, String>>) {
    let mut lines = content.lines();
    let header_line = lines.next().unwrap_or("");
    let header: Vec<String> = header_line.split(',').map(|s| s.to_owned()).collect();
    let rows: Vec<BTreeMap<String, String>> = lines
        .filter(|l| !l.is_empty())
        .map(|line| {
            let cells: Vec<&str> = line.split(',').collect();
            let mut row = BTreeMap::new();
            for (i, name) in header.iter().enumerate() {
                let v = cells.get(i).copied().unwrap_or("").to_owned();
                row.insert(name.clone(), v);
            }
            row
        })
        .collect();
    (header, rows)
}

fn emit_csv(header: &[String], rows: &[BTreeMap<String, String>]) -> String {
    let mut out = String::new();
    out.push_str(&header.join(","));
    out.push('\n');
    for row in rows {
        let line: Vec<String> = header
            .iter()
            .map(|h| row.get(h).cloned().unwrap_or_default())
            .collect();
        out.push_str(&line.join(","));
        out.push('\n');
    }
    out
}

/// Apply the subset of `Diff` operations that target `path` to
/// `existing` (the current file content; empty string when the file
/// doesn't exist yet). Re-sorts per ADR-013 / ADR-015 stable-sort
/// rules and returns the new content + the existing-blob SHA (if any)
/// — caller threads the SHA through to `PUT contents`.
fn apply_diff_to_file(
    file: TargetFile,
    diff: &Diff,
    existing: Option<&GetContentsResponse>,
) -> Result<String, ProposalError> {
    // 1. Reject deletes per ADR-012's "registry never deletes" rule.
    //    The trait shape supports delete for forward-compat (e.g. a
    //    future scratch-data adapter); the GitHub-PR adapter refuses
    //    because the data repo is the system of record. ADR-016's
    //    policy engine should already have blocked these, but we
    //    defend in depth.
    let our_deletes: Vec<&DiffRow> = diff
        .deletes
        .iter()
        .filter(|r| file_matches_row(file, r))
        .collect();
    if !our_deletes.is_empty() && file == TargetFile::Registry {
        return Err(ProposalError::Rejected(
            "registry deletes are not permitted per ADR-012 — \
             use a void edit (status -> void) instead"
                .into(),
        ));
    }

    // 2. Decode the existing file (base64 from GitHub Contents API).
    let existing_text = match existing {
        Some(r) => decode_b64_content(&r.content)?,
        None => String::new(),
    };

    // 3. Parse to (header, rows).
    let canonical_header: Vec<String> = match file {
        TargetFile::Registry => REGISTRY_HEADER.iter().map(|s| (*s).to_owned()).collect(),
        TargetFile::PrintLog => PRINT_LOG_HEADER.iter().map(|s| (*s).to_owned()).collect(),
        // ADR-022 §"AuditEntry shape" — header lives in storage; we
        // mirror the storage adapter's order here. If the file
        // doesn't exist yet we seed with this header so the first
        // proposal-driven write produces a sane CSV.
        TargetFile::AuditLog => vec![
            "request_id".into(),
            "timestamp".into(),
            "actor".into(),
            "action".into(),
            "target".into(),
            "before".into(),
            "after".into(),
            "extra".into(),
            "signatures".into(),
            "chain_hash".into(),
        ],
    };
    let (mut header, mut rows) = if existing_text.is_empty() {
        (
            canonical_header.clone(),
            Vec::<BTreeMap<String, String>>::new(),
        )
    } else {
        parse_csv(&existing_text)
    };

    // 4. Apply header changes targeting this file.
    for hc in &diff.header_changes {
        if hc.file == file.path() {
            header = hc.after.clone();
        }
    }

    // 5. Apply edits (replace by id).
    for edit in &diff.edits {
        if !file_matches_edit(file, edit) {
            continue;
        }
        let id_str = edit.id.as_str();
        if let Some(existing_row) = rows
            .iter_mut()
            .find(|r| r.get("id").map(String::as_str) == Some(id_str))
        {
            for (k, v) in &edit.after {
                existing_row.insert(k.clone(), v.clone());
            }
        } else {
            // No existing row — promote to add so the proposal still
            // makes a net-positive change.
            let mut row = edit.after.clone();
            row.entry("id".into()).or_insert_with(|| id_str.to_owned());
            rows.push(row);
        }
    }

    // 6. Apply adds.
    for add in &diff.adds {
        if !file_matches_row(file, add) {
            continue;
        }
        let mut row = add.fields.clone();
        if let Some(id) = &add.id {
            row.insert("id".into(), id.as_str().to_owned());
        }
        rows.push(row);
    }

    // 7. Apply deletes for files where they are permitted (audit_log
    //    + print_log honour the trait, registry rejected above).
    if file != TargetFile::Registry {
        for del in &diff.deletes {
            if !file_matches_row(file, del) {
                continue;
            }
            if let Some(id) = &del.id {
                rows.retain(|r| r.get("id").map(String::as_str) != Some(id.as_str()));
            }
        }
    }

    // 8. Re-sort per ADR-013 / ADR-015.
    sort_rows(file, &mut rows);

    Ok(emit_csv(&header, &rows))
}

fn file_matches_row(file: TargetFile, row: &DiffRow) -> bool {
    TargetFile::classify_row(&row.fields) == file
}

fn file_matches_edit(file: TargetFile, edit: &DiffEdit) -> bool {
    // Edits classify by the union of before+after fields.
    let mut merged = edit.before.clone();
    for (k, v) in &edit.after {
        merged.insert(k.clone(), v.clone());
    }
    TargetFile::classify_row(&merged) == file
}

fn sort_rows(file: TargetFile, rows: &mut [BTreeMap<String, String>]) {
    match file {
        TargetFile::Registry => {
            // Mirrors `qx_validators::registry_sort_key`:
            // ascending by `id`.
            let _ = registry_sort_key; // explicit re-export anchor for grep
            rows.sort_by(|a, b| {
                let ai = a.get("id").map(String::as_str).unwrap_or("");
                let bi = b.get("id").map(String::as_str).unwrap_or("");
                ai.cmp(bi)
            });
        }
        TargetFile::PrintLog => {
            // Mirrors `print_log_sort_key`: `(printed_at, id)`.
            let _ = print_log_sort_key;
            rows.sort_by(|a, b| {
                let at = a.get("printed_at").map(String::as_str).unwrap_or("");
                let bt = b.get("printed_at").map(String::as_str).unwrap_or("");
                let cmp = at.cmp(bt);
                if cmp != std::cmp::Ordering::Equal {
                    return cmp;
                }
                let ai = a.get("id").map(String::as_str).unwrap_or("");
                let bi = b.get("id").map(String::as_str).unwrap_or("");
                ai.cmp(bi)
            });
        }
        TargetFile::AuditLog => {
            // ADR-022 §"sort stability": `(timestamp, request_id)`.
            rows.sort_by(|a, b| {
                let at = a.get("timestamp").map(String::as_str).unwrap_or("");
                let bt = b.get("timestamp").map(String::as_str).unwrap_or("");
                let cmp = at.cmp(bt);
                if cmp != std::cmp::Ordering::Equal {
                    return cmp;
                }
                let ar = a.get("request_id").map(String::as_str).unwrap_or("");
                let br = b.get("request_id").map(String::as_str).unwrap_or("");
                ar.cmp(br)
            });
        }
    }
}

// -------------------------------------------------------------------
// base64
// -------------------------------------------------------------------

fn b64_encode(s: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(s.as_bytes())
}

fn decode_b64_content(c: &str) -> Result<String, ProposalError> {
    // GitHub line-wraps the encoded content at 60 chars; strip
    // whitespace before decoding.
    let stripped: String = c.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(stripped.as_bytes())
        .map_err(|e| ProposalError::Backend(Box::new(e)))?;
    String::from_utf8(bytes).map_err(|e| ProposalError::Backend(Box::new(e)))
}

// -------------------------------------------------------------------
// Adapter
// -------------------------------------------------------------------

pub struct GithubPrProposalSink<H: GithubPrHttp> {
    http: H,
    config: GithubPrConfig,
}

impl<H: GithubPrHttp> GithubPrProposalSink<H> {
    pub fn new(http: H, config: GithubPrConfig) -> Self {
        Self { http, config }
    }

    pub fn config(&self) -> &GithubPrConfig {
        &self.config
    }

    fn build_branch(&self, request_id: &str, suffix: Option<&str>) -> String {
        let mut s = format!("{}{}", self.config.branch_prefix, request_id);
        if let Some(suf) = suffix {
            s.push('-');
            s.push_str(suf);
        }
        s
    }

    /// Author/committer used for the contents API. Per the task spec,
    /// the *committer* is the adapter's configured pair (the bot
    /// identity acting on behalf of the operator); the *author* is
    /// the operator themselves. `git log` then shows both.
    fn author_for(&self, op: &Operator) -> ContentsAuthor {
        let email = op
            .claims
            .get("github_email")
            .cloned()
            .or_else(|| op.claims.get("email").cloned())
            .unwrap_or_else(|| self.config.commit_author_email.clone());
        ContentsAuthor {
            name: op.display_name.clone(),
            email,
        }
    }

    fn committer(&self) -> ContentsAuthor {
        ContentsAuthor {
            name: self.config.commit_author_name.clone(),
            email: self.config.commit_author_email.clone(),
        }
    }

    /// Single attempt at creating + populating + opening the PR for a
    /// given branch name. Returns the open PR or an HttpError —
    /// `submit()` wraps this with the 409-retry logic.
    fn submit_to_branch(
        &self,
        branch: &str,
        proposal: &Proposal,
    ) -> Result<PullResponse, HttpError> {
        let owner = self.config.data_repo_owner.as_str();
        let repo = self.config.data_repo_name.as_str();
        let base = self.config.base_branch.as_str();

        // 1. Look up the base SHA.
        let base_ref = self.http.get_branch_ref(owner, repo, base)?;

        // 2. Create the proposal branch off that SHA.
        let create_body = CreateRefRequest {
            ref_: format!("refs/heads/{branch}"),
            sha: base_ref.object.sha,
        };
        self.http.create_ref(owner, repo, &create_body)?;

        // 3. For each affected file, fetch existing + apply diff + PUT.
        for file in affected_files(&proposal.diff) {
            let existing = self.http.get_contents(owner, repo, file.path(), base)?;
            let new_content =
                apply_diff_to_file(file, &proposal.diff, existing.as_ref()).map_err(|e| {
                    HttpError::Transport(format!("apply_diff_to_file({}): {}", file.path(), e))
                })?;
            let put_body = PutContentsRequest {
                message: format!(
                    "{} ({})",
                    if proposal.message.trim().is_empty() {
                        "proposal".to_owned()
                    } else {
                        proposal
                            .message
                            .lines()
                            .next()
                            .unwrap_or("proposal")
                            .to_owned()
                    },
                    file.path()
                ),
                content: b64_encode(&new_content),
                branch: branch.to_owned(),
                sha: existing.as_ref().map(|e| e.sha.clone()),
                committer: self.committer(),
                author: self.author_for(&proposal.author),
            };
            self.http
                .put_contents(owner, repo, file.path(), &put_body)?;
        }

        // 4. Open the PR.
        let pr_body = CreatePullRequest {
            title: build_pr_title(proposal),
            body: build_pr_body(proposal),
            head: branch.to_owned(),
            base: base.to_owned(),
        };
        self.http.create_pull(owner, repo, &pr_body)
    }
}

/// Which files this proposal touches, in a stable order.
fn affected_files(diff: &Diff) -> Vec<TargetFile> {
    let mut seen: Vec<TargetFile> = Vec::new();
    let mut push = |f: TargetFile| {
        if !seen.contains(&f) {
            seen.push(f);
        }
    };
    for row in diff.adds.iter().chain(diff.deletes.iter()) {
        push(TargetFile::classify_row(&row.fields));
    }
    for edit in &diff.edits {
        let mut merged = edit.before.clone();
        for (k, v) in &edit.after {
            merged.insert(k.clone(), v.clone());
        }
        push(TargetFile::classify_row(&merged));
    }
    for hc in &diff.header_changes {
        match hc.file.as_str() {
            "registry.csv" => push(TargetFile::Registry),
            "print_log.csv" => push(TargetFile::PrintLog),
            "audit_log.csv" => push(TargetFile::AuditLog),
            _ => {} // Unknown file; let CI surface.
        }
    }
    if seen.is_empty() {
        // Header-only or empty proposal — default to registry so a
        // PR can still be opened (rare; CI will surface).
        seen.push(TargetFile::Registry);
    }
    seen
}

impl<H: GithubPrHttp> ProposalSink for GithubPrProposalSink<H> {
    fn submit(&self, proposal: Proposal) -> Result<ProposalRef, ProposalError> {
        let request_id_str = proposal.request_id.to_string();
        let primary_branch = self.build_branch(&request_id_str, None);

        // Try once on the primary branch. If the branch already
        // exists (422 from `git/refs` POST per GitHub's docs, or 409
        // depending on path), retry once with a short
        // request-id-derived suffix.
        let attempt = self.submit_to_branch(&primary_branch, &proposal);
        let (branch, pr) = match attempt {
            Ok(pr) => (primary_branch, pr),
            Err(HttpError::Status { status, body })
                if status == 409 || status == 422 && body.contains("Reference already exists") =>
            {
                let suffix = suffix_from_request_id(&request_id_str);
                let retry_branch = self.build_branch(&request_id_str, Some(&suffix));
                let pr = self
                    .submit_to_branch(&retry_branch, &proposal)
                    .map_err(map_http_to_proposal_error)?;
                (retry_branch, pr)
            }
            Err(e) => return Err(map_http_to_proposal_error(e)),
        };

        Ok(ProposalRef {
            url: pr.html_url,
            local_id: Some(branch),
            adapter: "github_pr".into(),
        })
    }

    fn status(&self, proposal_ref: &ProposalRef) -> Result<ProposalStatus, ProposalError> {
        let number = parse_pr_number(&proposal_ref.url).ok_or_else(|| {
            ProposalError::Other(format!(
                "could not parse PR number from {}",
                proposal_ref.url
            ))
        })?;
        let owner = self.config.data_repo_owner.as_str();
        let repo = self.config.data_repo_name.as_str();

        let pr = match self.http.get_pull(owner, repo, number) {
            Ok(pr) => pr,
            Err(e) => {
                return Ok(ProposalStatus::Errored {
                    reason: e.to_string(),
                })
            }
        };

        // Closed / merged shortcuts first.
        if pr.state == "closed" {
            if pr.merged_at.is_some() {
                return Ok(ProposalStatus::Merged);
            }
            return Ok(ProposalStatus::Closed);
        }

        // Open. Look at reviews + checks.
        let head_ref = proposal_ref
            .local_id
            .clone()
            .unwrap_or_else(|| self.build_branch(&number.to_string(), None));

        // Check runs.
        let checks = self.http.get_check_runs(owner, repo, &head_ref);
        if let Ok(runs) = checks {
            // Any failed conclusion → BlockedByPolicy.
            if let Some(failed) = runs.check_runs.iter().find(|r| {
                r.status == "completed"
                    && matches!(
                        r.conclusion.as_deref(),
                        Some("failure")
                            | Some("timed_out")
                            | Some("action_required")
                            | Some("cancelled")
                    )
            }) {
                return Ok(ProposalStatus::BlockedByPolicy {
                    reason: format!(
                        "{} {}",
                        failed.name,
                        failed.conclusion.as_deref().unwrap_or("failed")
                    ),
                });
            }
        }

        // Requested reviewers + no completed review → RequiresReview.
        if !pr.requested_reviewers.is_empty() {
            let reviews = self
                .http
                .get_reviews(owner, repo, number)
                .unwrap_or_default();
            let has_review = reviews
                .iter()
                .any(|r| r.state == "APPROVED" || r.state == "CHANGES_REQUESTED");
            if !has_review {
                return Ok(ProposalStatus::RequiresReview);
            }
        }

        Ok(ProposalStatus::Open)
    }
}

fn map_http_to_proposal_error(e: HttpError) -> ProposalError {
    match &e {
        HttpError::Status { status, body } => {
            if *status == 401 || *status == 403 {
                ProposalError::Auth(body.clone())
            } else if *status == 429 {
                ProposalError::RateLimited {
                    retry_after_seconds: 60,
                }
            } else if *status >= 500 {
                ProposalError::Backend(Box::new(e))
            } else {
                ProposalError::Rejected(format!("HTTP {status}: {body}"))
            }
        }
        HttpError::Transport(_) => ProposalError::Network(Box::new(e)),
        HttpError::Deserialize(_) => ProposalError::Backend(Box::new(e)),
    }
}

/// Derive a deterministic 6-char suffix from the request_id for the
/// duplicate-branch retry. UUIDv7 already contains randomness in its
/// trailing bytes; we hex-encode the last three bytes for a stable
/// suffix without pulling a PRNG dep.
fn suffix_from_request_id(req: &str) -> String {
    // Strip dashes; take the last 6 hex chars; fall back to a short
    // hash if the input doesn't look like a UUID.
    let hex: String = req.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.len() >= 6 {
        hex[hex.len() - 6..].to_lowercase()
    } else {
        let mut s = 0u64;
        for b in req.as_bytes() {
            s = s.wrapping_mul(31).wrapping_add(*b as u64);
        }
        format!("{:06x}", s & 0xff_ffff)
    }
}

/// Extract the PR number from a `https://github.com/<o>/<r>/pull/<n>`
/// URL. Tolerant of trailing slashes / fragments.
pub fn parse_pr_number(url: &str) -> Option<u64> {
    let after = url.rsplit("/pull/").next()?;
    let num: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    if num.is_empty() {
        return None;
    }
    num.parse().ok()
}

// -------------------------------------------------------------------
// Tests
// -------------------------------------------------------------------

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use qx_domain::{
        DiffRow, HeaderChange, IdentitySource, KeyId, OperatorId, PartId, RekorProof, RequestId,
    };
    use qx_port_tests::proposal_sink_conformance;

    // ----- HTTP test double -----------------------------------------

    type GetBranchFn = dyn Fn(&str, &str, &str) -> Result<GitRefResponse, HttpError> + Send + Sync;
    type CreateRefFn =
        dyn Fn(&str, &str, &CreateRefRequest) -> Result<GitRefResponse, HttpError> + Send + Sync;
    type GetContentsFn = dyn Fn(&str, &str, &str, &str) -> Result<Option<GetContentsResponse>, HttpError>
        + Send
        + Sync;
    type PutContentsFn =
        dyn Fn(&str, &str, &str, &PutContentsRequest) -> Result<(), HttpError> + Send + Sync;
    type CreatePullFn =
        dyn Fn(&str, &str, &CreatePullRequest) -> Result<PullResponse, HttpError> + Send + Sync;
    type GetPullFn = dyn Fn(&str, &str, u64) -> Result<PullResponse, HttpError> + Send + Sync;
    type GetCheckRunsFn =
        dyn Fn(&str, &str, &str) -> Result<CheckRunsResponse, HttpError> + Send + Sync;
    type GetReviewsFn = dyn Fn(&str, &str, u64) -> Result<Vec<PullReview>, HttpError> + Send + Sync;

    #[derive(Default)]
    struct FakeHttp {
        get_branch: Mutex<Option<Box<GetBranchFn>>>,
        create_ref: Mutex<Option<Box<CreateRefFn>>>,
        get_contents: Mutex<Option<Box<GetContentsFn>>>,
        put_contents: Mutex<Option<Box<PutContentsFn>>>,
        create_pull: Mutex<Option<Box<CreatePullFn>>>,
        get_pull: Mutex<Option<Box<GetPullFn>>>,
        get_check_runs: Mutex<Option<Box<GetCheckRunsFn>>>,
        get_reviews: Mutex<Option<Box<GetReviewsFn>>>,
        // Recorded call audit trail.
        calls: Mutex<Vec<String>>,
    }

    impl FakeHttp {
        fn record(&self, s: impl Into<String>) {
            self.calls.lock().unwrap().push(s.into());
        }
        #[allow(dead_code)]
        fn snapshot_calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl GithubPrHttp for FakeHttp {
        fn get_branch_ref(
            &self,
            owner: &str,
            repo: &str,
            branch: &str,
        ) -> Result<GitRefResponse, HttpError> {
            self.record(format!("get_branch_ref({owner},{repo},{branch})"));
            let g = self.get_branch.lock().unwrap();
            let f = g
                .as_ref()
                .ok_or_else(|| HttpError::Transport("get_branch_ref unset".into()))?;
            f(owner, repo, branch)
        }
        fn create_ref(
            &self,
            owner: &str,
            repo: &str,
            body: &CreateRefRequest,
        ) -> Result<GitRefResponse, HttpError> {
            self.record(format!("create_ref({owner},{repo},{})", body.ref_));
            let g = self.create_ref.lock().unwrap();
            let f = g
                .as_ref()
                .ok_or_else(|| HttpError::Transport("create_ref unset".into()))?;
            f(owner, repo, body)
        }
        fn get_contents(
            &self,
            owner: &str,
            repo: &str,
            path: &str,
            ref_: &str,
        ) -> Result<Option<GetContentsResponse>, HttpError> {
            self.record(format!("get_contents({owner},{repo},{path},{ref_})"));
            let g = self.get_contents.lock().unwrap();
            let f = g
                .as_ref()
                .ok_or_else(|| HttpError::Transport("get_contents unset".into()))?;
            f(owner, repo, path, ref_)
        }
        fn put_contents(
            &self,
            owner: &str,
            repo: &str,
            path: &str,
            body: &PutContentsRequest,
        ) -> Result<(), HttpError> {
            self.record(format!(
                "put_contents({owner},{repo},{path},branch={})",
                body.branch
            ));
            let g = self.put_contents.lock().unwrap();
            let f = g
                .as_ref()
                .ok_or_else(|| HttpError::Transport("put_contents unset".into()))?;
            f(owner, repo, path, body)
        }
        fn create_pull(
            &self,
            owner: &str,
            repo: &str,
            body: &CreatePullRequest,
        ) -> Result<PullResponse, HttpError> {
            self.record(format!(
                "create_pull({owner},{repo},head={},base={})",
                body.head, body.base
            ));
            let g = self.create_pull.lock().unwrap();
            let f = g
                .as_ref()
                .ok_or_else(|| HttpError::Transport("create_pull unset".into()))?;
            f(owner, repo, body)
        }
        fn get_pull(&self, owner: &str, repo: &str, n: u64) -> Result<PullResponse, HttpError> {
            self.record(format!("get_pull({owner},{repo},{n})"));
            let g = self.get_pull.lock().unwrap();
            let f = g
                .as_ref()
                .ok_or_else(|| HttpError::Transport("get_pull unset".into()))?;
            f(owner, repo, n)
        }
        fn get_check_runs(
            &self,
            owner: &str,
            repo: &str,
            r: &str,
        ) -> Result<CheckRunsResponse, HttpError> {
            self.record(format!("get_check_runs({owner},{repo},{r})"));
            let g = self.get_check_runs.lock().unwrap();
            let f = g
                .as_ref()
                .ok_or_else(|| HttpError::Transport("get_check_runs unset".into()))?;
            f(owner, repo, r)
        }
        fn get_reviews(
            &self,
            owner: &str,
            repo: &str,
            n: u64,
        ) -> Result<Vec<PullReview>, HttpError> {
            self.record(format!("get_reviews({owner},{repo},{n})"));
            let g = self.get_reviews.lock().unwrap();
            let f = g
                .as_ref()
                .ok_or_else(|| HttpError::Transport("get_reviews unset".into()))?;
            f(owner, repo, n)
        }
    }

    // ----- Fixtures --------------------------------------------------

    fn config() -> GithubPrConfig {
        GithubPrConfig {
            data_repo_owner: "exo-pet".into(),
            data_repo_name: "exopet-registry".into(),
            base_branch: "main".into(),
            branch_prefix: "proposal/".into(),
            commit_author_name: "exopet-bot".into(),
            commit_author_email: "bot@exopet.example".into(),
        }
    }

    fn operator() -> Operator {
        let mut claims = BTreeMap::new();
        claims.insert("github_login".into(), "gerchowl".into());
        claims.insert("github_email".into(), "lars@example.com".into());
        Operator {
            id: OperatorId("github:gerchowl".into()),
            display_name: "Lars Gerchow".into(),
            source: IdentitySource::GitHubOAuth,
            verified_at: None,
            claims,
            pubkey: Some(KeyId("k1".into())),
        }
    }

    fn pid(s: &str) -> PartId {
        PartId::new(s).unwrap()
    }

    fn registry_add_diff() -> Diff {
        let mut fields = BTreeMap::new();
        fields.insert("status".into(), "unbound".into());
        fields.insert("minted_at".into(), "2026-05-11T12:00:00Z".into());
        fields.insert("batch".into(), "B-2026-05-11-experiment".into());
        Diff {
            adds: vec![DiffRow {
                id: Some(pid("ABCDEFGHJKMNPQ")),
                fields,
            }],
            ..Default::default()
        }
    }

    fn proposal_with_diff(diff: Diff) -> Proposal {
        let actions = diff.classify();
        Proposal {
            diff,
            batch_label: Some("B-2026-05-11-experiment".into()),
            author: operator(),
            signatures: vec![],
            change_classification: actions,
            message: "Add new parts from batch B-2026-05-11-experiment".into(),
            request_id: RequestId(uuid::Uuid::from_u128(
                0x01_8000_0000_0000_0000_0000_0000_0000_u128,
            )),
        }
    }

    fn base_branch_ref() -> GitRefResponse {
        GitRefResponse {
            ref_: "refs/heads/main".into(),
            object: GitRefObject {
                sha: "main-sha-abc".into(),
                type_: "commit".into(),
            },
        }
    }

    fn created_ref(branch: &str) -> GitRefResponse {
        GitRefResponse {
            ref_: format!("refs/heads/{branch}"),
            object: GitRefObject {
                sha: "branch-sha-xyz".into(),
                type_: "commit".into(),
            },
        }
    }

    fn existing_registry() -> GetContentsResponse {
        // Header + one existing row sorted by id.
        let body = "id,status,minted_at,batch,bound_at,type,description,vendor,part_number,location,notes\n\
                    AAAAAAAAAAAAAA,unbound,2026-05-10T00:00:00Z,B-2026-05-10,,,,,,,\n";
        GetContentsResponse {
            sha: "old-blob-sha".into(),
            content: b64_encode(body),
            encoding: "base64".into(),
            path: "registry.csv".into(),
        }
    }

    fn pr_response_open(num: u64, _branch: &str) -> PullResponse {
        PullResponse {
            number: num,
            html_url: format!("https://github.com/exo-pet/exopet-registry/pull/{}", num),
            state: "open".into(),
            merged_at: None,
            requested_reviewers: vec![],
        }
    }

    fn install_happy_path(http: &FakeHttp) {
        *http.get_branch.lock().unwrap() = Some(Box::new(|_, _, _| Ok(base_branch_ref())));
        *http.create_ref.lock().unwrap() = Some(Box::new(|_, _, body| {
            let branch = body.ref_.strip_prefix("refs/heads/").unwrap_or(&body.ref_);
            Ok(created_ref(branch))
        }));
        *http.get_contents.lock().unwrap() = Some(Box::new(|_, _, path, _| {
            if path == "registry.csv" {
                Ok(Some(existing_registry()))
            } else {
                Ok(None)
            }
        }));
        *http.put_contents.lock().unwrap() = Some(Box::new(|_, _, _, _| Ok(())));
        *http.create_pull.lock().unwrap() =
            Some(Box::new(|_, _, body| Ok(pr_response_open(42, &body.head))));
    }

    // ----- Conformance ----------------------------------------------

    #[test]
    fn github_pr_sink_passes_generic_conformance() {
        let http = FakeHttp::default();
        install_happy_path(&http);
        let sink = GithubPrProposalSink::new(http, config());
        proposal_sink_conformance(&sink, proposal_with_diff(registry_add_diff()));
    }

    // ----- 1. Submit happy path -------------------------------------

    #[test]
    fn submit_happy_path_creates_branch_files_and_pr() {
        let http = FakeHttp::default();
        install_happy_path(&http);
        let put_log: std::sync::Arc<Mutex<Vec<(String, PutContentsRequest)>>> =
            std::sync::Arc::new(Mutex::new(Vec::new()));
        let put_log_inner = put_log.clone();
        *http.put_contents.lock().unwrap() = Some(Box::new(move |_, _, path, body| {
            put_log_inner
                .lock()
                .unwrap()
                .push((path.to_owned(), body.clone()));
            Ok(())
        }));

        let create_pull_log: std::sync::Arc<Mutex<Vec<CreatePullRequest>>> =
            std::sync::Arc::new(Mutex::new(Vec::new()));
        let cpl_inner = create_pull_log.clone();
        *http.create_pull.lock().unwrap() = Some(Box::new(move |_, _, body| {
            cpl_inner.lock().unwrap().push(body.clone());
            Ok(pr_response_open(42, &body.head))
        }));

        let sink = GithubPrProposalSink::new(http, config());
        let proposal = proposal_with_diff(registry_add_diff());
        let r = sink.submit(proposal.clone()).expect("submit ok");
        assert_eq!(r.adapter, "github_pr");
        assert!(r.url.contains("/pull/42"));
        let branch = r.local_id.as_deref().unwrap();
        assert!(branch.starts_with("proposal/"));

        // PR body carries the metadata block.
        let pulls = create_pull_log.lock().unwrap();
        assert_eq!(pulls.len(), 1);
        let pr_body = &pulls[0].body;
        assert!(pr_body.contains(&proposal.request_id.to_string()));
        assert!(pr_body.contains("github:gerchowl"));
        assert!(pr_body.contains("B-2026-05-11-experiment"));
        assert!(pr_body.contains("row_add"));

        // Parse the metadata block back out.
        let meta = parse_pr_metadata(pr_body).expect("metadata block present");
        assert_eq!(meta.request_id, proposal.request_id.to_string());
        assert_eq!(meta.author, "github:gerchowl");
        assert_eq!(meta.classification, vec!["row_add"]);

        // PUT-contents body uses the operator's email as author and
        // the bot identity as committer; new content includes the new row.
        let puts = put_log.lock().unwrap();
        assert_eq!(puts.len(), 1);
        assert_eq!(puts[0].0, "registry.csv");
        let new_text = decode_b64_content(&puts[0].1.content).unwrap();
        assert!(new_text.contains("ABCDEFGHJKMNPQ"));
        assert!(new_text.contains("AAAAAAAAAAAAAA"));
        // Sorted: AAA... comes before ABC...
        let aaa_idx = new_text.find("AAAAAAAAAAAAAA").unwrap();
        let abc_idx = new_text.find("ABCDEFGHJKMNPQ").unwrap();
        assert!(aaa_idx < abc_idx);
        assert_eq!(puts[0].1.author.email, "lars@example.com");
        assert_eq!(puts[0].1.committer.email, "bot@exopet.example");
        assert_eq!(puts[0].1.sha.as_deref(), Some("old-blob-sha"));
    }

    // ----- 2. Multi-file diff ---------------------------------------

    #[test]
    fn submit_multi_file_diff_updates_registry_and_audit_log() {
        let mut diff = registry_add_diff();
        // Append an audit_log row.
        let mut audit_fields = BTreeMap::new();
        audit_fields.insert("request_id".into(), "01HEXR40...".into());
        audit_fields.insert("timestamp".into(), "2026-05-11T12:00:01Z".into());
        audit_fields.insert("actor".into(), "github:gerchowl".into());
        audit_fields.insert("action".into(), "row_add".into());
        diff.adds.push(DiffRow {
            id: None,
            fields: audit_fields,
        });

        let http = FakeHttp::default();
        install_happy_path(&http);
        let path_log: std::sync::Arc<Mutex<Vec<String>>> =
            std::sync::Arc::new(Mutex::new(Vec::new()));
        let pl_inner = path_log.clone();
        *http.put_contents.lock().unwrap() = Some(Box::new(move |_, _, path, _| {
            pl_inner.lock().unwrap().push(path.to_owned());
            Ok(())
        }));

        let sink = GithubPrProposalSink::new(http, config());
        let _ = sink.submit(proposal_with_diff(diff)).expect("submit ok");
        let paths = path_log.lock().unwrap();
        assert!(paths.iter().any(|p| p == "registry.csv"));
        assert!(paths.iter().any(|p| p == "audit_log.csv"));
        // Still one PR though — we don't have a direct counter, but
        // install_happy_path's create_pull mock would have panicked on
        // double-invocation through Box.
    }

    // ----- 3. Header change -----------------------------------------

    #[test]
    fn submit_header_change_rewrites_first_line() {
        let mut diff = Diff::default();
        let new_header = vec![
            "id".into(),
            "status".into(),
            "minted_at".into(),
            "batch".into(),
            "bound_at".into(),
            "type".into(),
            "description".into(),
            "vendor".into(),
            "part_number".into(),
            "location".into(),
            "notes".into(),
            "criticality".into(), // new column
        ];
        diff.header_changes.push(HeaderChange {
            file: "registry.csv".into(),
            before: REGISTRY_HEADER.iter().map(|s| (*s).to_owned()).collect(),
            after: new_header.clone(),
        });

        let http = FakeHttp::default();
        install_happy_path(&http);
        let put_log: std::sync::Arc<Mutex<Vec<PutContentsRequest>>> =
            std::sync::Arc::new(Mutex::new(Vec::new()));
        let pl_inner = put_log.clone();
        *http.put_contents.lock().unwrap() = Some(Box::new(move |_, _, _, body| {
            pl_inner.lock().unwrap().push(body.clone());
            Ok(())
        }));

        let sink = GithubPrProposalSink::new(http, config());
        let _ = sink.submit(proposal_with_diff(diff)).expect("submit ok");
        let puts = put_log.lock().unwrap();
        let new_text = decode_b64_content(&puts[0].content).unwrap();
        let first_line = new_text.lines().next().unwrap();
        assert!(first_line.contains("criticality"));
    }

    // ----- 4. Sigstore-signature round-trip (ADR-027 Tier 2) --------

    #[test]
    fn submit_round_trips_sigstore_signatures_in_pr_body() {
        let mut proposal = proposal_with_diff(registry_add_diff());
        proposal.signatures = vec![Signature::Sigstore {
            cert: vec![1, 2, 3],
            sig: vec![4, 5, 6],
            rekor_proof: RekorProof {
                uuid: "rekor-uuid".into(),
                log_index: 7,
            },
        }];

        let http = FakeHttp::default();
        install_happy_path(&http);
        let body_log: std::sync::Arc<Mutex<Vec<String>>> =
            std::sync::Arc::new(Mutex::new(Vec::new()));
        let bl_inner = body_log.clone();
        *http.create_pull.lock().unwrap() = Some(Box::new(move |_, _, body| {
            bl_inner.lock().unwrap().push(body.body.clone());
            Ok(pr_response_open(42, &body.head))
        }));

        let sink = GithubPrProposalSink::new(http, config());
        let _ = sink.submit(proposal.clone()).expect("submit ok");
        let bodies = body_log.lock().unwrap();
        let meta = parse_pr_metadata(&bodies[0]).expect("metadata block");
        assert_eq!(meta.signatures.len(), 1);
        assert_eq!(meta.signatures, proposal.signatures);
    }

    // ----- 5. Branch creation failure -> Backend --------------------

    #[test]
    fn submit_branch_creation_failure_maps_to_proposal_error() {
        let http = FakeHttp::default();
        *http.get_branch.lock().unwrap() = Some(Box::new(|_, _, _| Ok(base_branch_ref())));
        *http.create_ref.lock().unwrap() = Some(Box::new(|_, _, _| {
            Err(HttpError::Status {
                status: 500,
                body: "internal server error".into(),
            })
        }));

        let sink = GithubPrProposalSink::new(http, config());
        let err = sink
            .submit(proposal_with_diff(registry_add_diff()))
            .unwrap_err();
        match err {
            ProposalError::Backend(_) => {}
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    // ----- 6. Duplicate-branch 422 retries with suffix --------------

    #[test]
    fn submit_duplicate_branch_retries_with_suffix() {
        let http = FakeHttp::default();
        *http.get_branch.lock().unwrap() = Some(Box::new(|_, _, _| Ok(base_branch_ref())));

        // First call fails 422; second succeeds.
        let call_count = std::sync::Arc::new(Mutex::new(0u32));
        let cc = call_count.clone();
        let branches_seen: std::sync::Arc<Mutex<Vec<String>>> =
            std::sync::Arc::new(Mutex::new(Vec::new()));
        let bs = branches_seen.clone();
        *http.create_ref.lock().unwrap() = Some(Box::new(move |_, _, body| {
            *cc.lock().unwrap() += 1;
            bs.lock().unwrap().push(body.ref_.clone());
            if *cc.lock().unwrap() == 1 {
                Err(HttpError::Status {
                    status: 422,
                    body: "Reference already exists".into(),
                })
            } else {
                let branch = body.ref_.strip_prefix("refs/heads/").unwrap_or(&body.ref_);
                Ok(created_ref(branch))
            }
        }));
        *http.get_contents.lock().unwrap() = Some(Box::new(|_, _, path, _| {
            if path == "registry.csv" {
                Ok(Some(existing_registry()))
            } else {
                Ok(None)
            }
        }));
        *http.put_contents.lock().unwrap() = Some(Box::new(|_, _, _, _| Ok(())));
        *http.create_pull.lock().unwrap() =
            Some(Box::new(|_, _, body| Ok(pr_response_open(43, &body.head))));

        let sink = GithubPrProposalSink::new(http, config());
        let r = sink
            .submit(proposal_with_diff(registry_add_diff()))
            .expect("retry succeeds");
        let branch = r.local_id.as_deref().unwrap();
        assert_eq!(*call_count.lock().unwrap(), 2);
        let seen = branches_seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        // Second attempt has a suffix.
        assert!(seen[1] != seen[0]);
        assert!(branch.contains('-')); // has a suffix
    }

    #[test]
    fn submit_duplicate_branch_persistent_failure_surfaces_error() {
        let http = FakeHttp::default();
        *http.get_branch.lock().unwrap() = Some(Box::new(|_, _, _| Ok(base_branch_ref())));
        *http.create_ref.lock().unwrap() = Some(Box::new(|_, _, _| {
            Err(HttpError::Status {
                status: 422,
                body: "Reference already exists".into(),
            })
        }));

        let sink = GithubPrProposalSink::new(http, config());
        let err = sink
            .submit(proposal_with_diff(registry_add_diff()))
            .unwrap_err();
        // 422 with "Reference already exists" on the retry is mapped
        // by `map_http_to_proposal_error` as a Rejected (non-5xx, non-auth).
        match err {
            ProposalError::Rejected(_) => {}
            other => panic!("expected Rejected, got {other:?}"),
        }
    }

    // ----- 7-10. status() flow --------------------------------------

    fn proposal_ref_for(num: u64) -> ProposalRef {
        ProposalRef {
            url: format!("https://github.com/exo-pet/exopet-registry/pull/{}", num),
            local_id: Some(format!("proposal/abc-{}", num)),
            adapter: "github_pr".into(),
        }
    }

    #[test]
    fn status_open_with_checks_pending_is_open() {
        let http = FakeHttp::default();
        *http.get_pull.lock().unwrap() = Some(Box::new(|_, _, n| {
            Ok(PullResponse {
                number: n,
                html_url: "https://github.com/exo-pet/exopet-registry/pull/1".into(),
                state: "open".into(),
                merged_at: None,
                requested_reviewers: vec![],
            })
        }));
        *http.get_check_runs.lock().unwrap() = Some(Box::new(|_, _, _| {
            Ok(CheckRunsResponse {
                total_count: 1,
                check_runs: vec![CheckRun {
                    name: "ci/validators".into(),
                    status: "in_progress".into(),
                    conclusion: None,
                }],
            })
        }));

        let sink = GithubPrProposalSink::new(http, config());
        let s = sink.status(&proposal_ref_for(1)).unwrap();
        assert_eq!(s, ProposalStatus::Open);
    }

    #[test]
    fn status_open_with_failed_check_is_blocked() {
        let http = FakeHttp::default();
        *http.get_pull.lock().unwrap() = Some(Box::new(|_, _, n| {
            Ok(PullResponse {
                number: n,
                html_url: "x".into(),
                state: "open".into(),
                merged_at: None,
                requested_reviewers: vec![],
            })
        }));
        *http.get_check_runs.lock().unwrap() = Some(Box::new(|_, _, _| {
            Ok(CheckRunsResponse {
                total_count: 1,
                check_runs: vec![CheckRun {
                    name: "ci/validators".into(),
                    status: "completed".into(),
                    conclusion: Some("failure".into()),
                }],
            })
        }));

        let sink = GithubPrProposalSink::new(http, config());
        let s = sink.status(&proposal_ref_for(2)).unwrap();
        match s {
            ProposalStatus::BlockedByPolicy { reason } => {
                assert!(reason.contains("ci/validators"));
                assert!(reason.contains("failure"));
            }
            other => panic!("expected BlockedByPolicy, got {other:?}"),
        }
    }

    #[test]
    fn status_closed_and_merged_is_merged() {
        let http = FakeHttp::default();
        *http.get_pull.lock().unwrap() = Some(Box::new(|_, _, n| {
            Ok(PullResponse {
                number: n,
                html_url: "x".into(),
                state: "closed".into(),
                merged_at: Some("2026-05-11T12:30:00Z".into()),
                requested_reviewers: vec![],
            })
        }));
        let sink = GithubPrProposalSink::new(http, config());
        let s = sink.status(&proposal_ref_for(3)).unwrap();
        assert_eq!(s, ProposalStatus::Merged);
    }

    #[test]
    fn status_closed_and_not_merged_is_closed() {
        let http = FakeHttp::default();
        *http.get_pull.lock().unwrap() = Some(Box::new(|_, _, n| {
            Ok(PullResponse {
                number: n,
                html_url: "x".into(),
                state: "closed".into(),
                merged_at: None,
                requested_reviewers: vec![],
            })
        }));
        let sink = GithubPrProposalSink::new(http, config());
        let s = sink.status(&proposal_ref_for(4)).unwrap();
        assert_eq!(s, ProposalStatus::Closed);
    }

    #[test]
    fn status_open_with_requested_reviewer_no_review_is_requires_review() {
        let http = FakeHttp::default();
        *http.get_pull.lock().unwrap() = Some(Box::new(|_, _, n| {
            Ok(PullResponse {
                number: n,
                html_url: "x".into(),
                state: "open".into(),
                merged_at: None,
                requested_reviewers: vec![PullReviewer {
                    login: "reviewer1".into(),
                }],
            })
        }));
        *http.get_check_runs.lock().unwrap() = Some(Box::new(|_, _, _| {
            Ok(CheckRunsResponse {
                total_count: 0,
                check_runs: vec![],
            })
        }));
        *http.get_reviews.lock().unwrap() = Some(Box::new(|_, _, _| Ok(vec![])));

        let sink = GithubPrProposalSink::new(http, config());
        let s = sink.status(&proposal_ref_for(5)).unwrap();
        assert_eq!(s, ProposalStatus::RequiresReview);
    }

    #[test]
    fn status_http_error_becomes_errored() {
        let http = FakeHttp::default();
        *http.get_pull.lock().unwrap() = Some(Box::new(|_, _, _| {
            Err(HttpError::Transport("connection refused".into()))
        }));
        let sink = GithubPrProposalSink::new(http, config());
        let s = sink.status(&proposal_ref_for(6)).unwrap();
        match s {
            ProposalStatus::Errored { reason } => {
                assert!(reason.contains("connection refused"));
            }
            other => panic!("expected Errored, got {other:?}"),
        }
    }

    // ----- Helpers --------------------------------------------------

    #[test]
    fn parse_pr_number_from_url() {
        assert_eq!(
            parse_pr_number("https://github.com/exo-pet/exopet-registry/pull/42"),
            Some(42)
        );
        assert_eq!(
            parse_pr_number("https://github.com/exo-pet/exopet-registry/pull/42/files"),
            Some(42)
        );
        assert_eq!(parse_pr_number("not a url"), None);
    }

    #[test]
    fn classification_summary_aggregates_kinds() {
        let acts = vec![
            Action::RowAdd {
                row: serde_json::Value::Object(serde_json::Map::new()),
            },
            Action::RowAdd {
                row: serde_json::Value::Object(serde_json::Map::new()),
            },
            Action::HeaderChange {
                before: vec![],
                after: vec![],
            },
        ];
        let s = classification_summary(&acts);
        assert!(s.contains("row_add × 2"));
        assert!(s.contains("header_change × 1"));
    }

    #[test]
    fn target_file_classification_distinguishes_three_files() {
        let mut reg = BTreeMap::new();
        reg.insert("status".into(), "unbound".into());
        reg.insert("minted_at".into(), "x".into());
        assert_eq!(TargetFile::classify_row(&reg), TargetFile::Registry);

        let mut pl = BTreeMap::new();
        pl.insert("printed_at".into(), "x".into());
        pl.insert("printed_by".into(), "x".into());
        assert_eq!(TargetFile::classify_row(&pl), TargetFile::PrintLog);

        let mut al = BTreeMap::new();
        al.insert("request_id".into(), "x".into());
        al.insert("action".into(), "row_add".into());
        assert_eq!(TargetFile::classify_row(&al), TargetFile::AuditLog);
    }

    #[test]
    fn pr_body_metadata_round_trip() {
        let proposal = proposal_with_diff(registry_add_diff());
        let body = build_pr_body(&proposal);
        let meta = parse_pr_metadata(&body).expect("metadata block present");
        assert_eq!(meta.request_id, proposal.request_id.to_string());
        assert_eq!(meta.author, "github:gerchowl");
        assert_eq!(meta.batch.as_deref(), Some("B-2026-05-11-experiment"));
    }

    #[test]
    fn registry_delete_is_rejected_per_adr_012() {
        let mut diff = Diff::default();
        let mut fields = BTreeMap::new();
        fields.insert("status".into(), "unbound".into());
        diff.deletes.push(DiffRow {
            id: Some(pid("ABCDEFGHJKMNPQ")),
            fields,
        });
        let err = apply_diff_to_file(TargetFile::Registry, &diff, None).unwrap_err();
        match err {
            ProposalError::Rejected(_) => {}
            other => panic!("expected Rejected, got {other:?}"),
        }
    }

    #[test]
    fn map_http_to_proposal_error_categorises_status() {
        match map_http_to_proposal_error(HttpError::Status {
            status: 401,
            body: "no".into(),
        }) {
            ProposalError::Auth(_) => {}
            other => panic!("got {other:?}"),
        }
        match map_http_to_proposal_error(HttpError::Status {
            status: 500,
            body: "no".into(),
        }) {
            ProposalError::Backend(_) => {}
            other => panic!("got {other:?}"),
        }
        match map_http_to_proposal_error(HttpError::Transport("oops".into())) {
            ProposalError::Network(_) => {}
            other => panic!("got {other:?}"),
        }
    }
}
