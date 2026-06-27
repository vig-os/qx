//! `qx-cli` — wiring crate for the `mint`, `label`, `bind`
//! binaries per ADR-017. Adapter selection per ADR-021's
//! `PART_REGISTRY_*` env vars happens here so domain crates never
//! match on adapter strings (ADR-027 §Tier 4 drift discipline).
//!
//! ## Shape
//!
//! Each binary's `main()` is a ~30-line wrapper that:
//!
//! 1. Parses its `Args` clap struct (defined in this crate).
//! 2. Loads [`Config`] via `qx_config`.
//! 3. Calls `init_observability(...)` to set up tracing + audit-CSV.
//! 4. Opens a `request_id` root span per ADR-022.
//! 5. Calls `run_mint` / `run_label` / `run_bind` with parsed args +
//!    `Wiring` (the constructed adapter set) and returns an `ExitCode`.
//!
//! Test code can build a `Wiring` from doubles directly without
//! touching `Config` / env vars / file system.
//!
//! ## Parity with the Python CLIs
//!
//! `mint.py`, `label.py`, `bind.py` at the repo root are the parity
//! targets per ADR-017 strangler-fig step 7. Flags + stdout output +
//! print-event semantics match byte-for-byte except for two
//! deliberate diffs documented inline:
//!
//! - **Mutation flow**: Python writes `registry.csv` directly; Rust
//!   submits a `Proposal` via `ProposalSink` (ADR-019). A
//!   `--dry-run` flag captures the diff locally for parity-test
//!   purposes without opening a GitHub PR.
//! - **QR matrix**: encoder mask selection differs by one bit per
//!   ADR-017 §Consequences — the SVG is structurally identical
//!   (same viewBox, same text rows, same module count) but the
//!   `<rect>` pattern inside the QR differs. Decoding round-trips.

#![forbid(unsafe_code)]

/// `qx serve` — HTTP shell over the command protocol (ADR-030 §2).
#[cfg(feature = "serve")]
pub mod serve;

/// `qx mcp` — stdio MCP shell over the command protocol (ADR-030 §2).
#[cfg(feature = "mcp")]
pub mod mcp;

/// `qx tui` — terminal shell over the command protocol (ADR-030 §2).
#[cfg(feature = "tui")]
pub mod tui;

/// In-core SVG -> png/jpeg/pdf raster for `qx print --emit` (ADR-031
/// §8). The `Emit` enum + svg pass-through compile unconditionally;
/// the actual rasterisers need the `raster` feature.
pub mod raster;

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use clap::{Parser, ValueEnum};
use serde_json::json;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use qx_codec::{encode_pinned, render_label, Ec, Family, Layout, TextFormat};
use qx_config::{Config, IdentityAdapterChoice, StorageAdapterChoice};
use qx_domain::{
    Action, AuditEntry, Diff, DiffEdit, DiffRow, Operator, Part, PartId, PartStatus, Proposal,
    ProposalRef, ProposalStatus, RequestId, Signature, TargetRef, PART_ID_ALPHABET, PART_ID_LEN,
};
use qx_identity::IdentityProvider;
use qx_identity_git_config::GitConfigIdentity;
use qx_observability::{
    bind_audit_entry, emit_audit, init, mint_audit_entry, request_id_span, void_audit_entry,
    AuditSinkHandle, ObservabilityConfig, OperatorGuard,
};
use qx_storage::{PartFilter, Repository};
use qx_storage_csv_git::{CsvGitConfig, CsvGitRepository};
use qx_transport::{ProposalError, ProposalSink};
use qx_transport_github_pr::{GithubPrConfig, GithubPrProposalSink, ReqwestGithubPrHttp};

// -------------------------------------------------------------------
// ADR-012 identifier helpers
// -------------------------------------------------------------------

/// Mint one fresh [`PartId`] disjoint from `existing`.
///
/// Mirrors `mint.py:mint_id`: try up to 16 times, fail loudly if the
/// RNG keeps colliding. The 14-char alphanumeric draw from the
/// ADR-012 alphabet has a ~1e-22 collision probability per attempt at
/// realistic registry sizes (< 10^6 entries), so 16 retries gives
/// ~1e-352 failure probability — effectively impossible.
///
/// Uses [`getrandom::getrandom`] (CSPRNG, OS-seeded) for the random
/// bytes. For 14 chars from a 31-symbol alphabet we need
/// 14·log2(31) ≈ 70 bits of entropy; we draw 32 bytes per attempt
/// which is plenty after rejection-sampling.
pub fn mint_part_id(existing: &HashSet<String>) -> Result<PartId, CliError> {
    for _ in 0..64 {
        let candidate = generate_one();
        if existing.contains(&candidate) {
            continue;
        }
        // M3-L fence (interim, issue #211): every minted id must encode
        // in the registry's canonical compact label symbology — Micro QR
        // **M3-L** — so every part renders as a uniform 60px micro label.
        // ~3% of random nano14 ids exceed M3-L's budget (scattered digit
        // runs inflate the Micro-QR segment count) and are re-rolled.
        // Remove this label-coupling once #211 (ISO terminator
        // truncation in the encoder) lands and M3-L holds all 14 chars.
        if encode_pinned(&candidate, Family::Micro, 3, Ec::L).is_err() {
            continue;
        }
        return PartId::new(candidate.clone()).map_err(|e| {
            CliError::Other(format!(
                "minted candidate {candidate:?} failed validation: {e}"
            ))
        });
    }
    Err(CliError::Other(
        "minting kept failing collision + M3-L feasibility — registry corrupt or RNG broken".into(),
    ))
}

fn generate_one() -> String {
    // Draw 14 indices uniformly from PART_ID_ALPHABET (31 symbols).
    // Use rejection sampling to avoid modulo bias.
    let alphabet: Vec<char> = PART_ID_ALPHABET.chars().collect();
    let n = alphabet.len() as u8;
    debug_assert_eq!(n, 31);
    // Largest multiple of n that fits in u8 — bytes >= this are rejected.
    let limit = (u8::MAX / n) * n;

    let mut out = String::with_capacity(PART_ID_LEN);
    while out.chars().count() < PART_ID_LEN {
        // Draw 32 random bytes per round; reject those that would
        // produce modulo bias and keep the rest. Two rounds gives
        // ~64 usable indices on average — well above the 14 needed.
        let mut buf = [0u8; 32];
        getrandom::getrandom(&mut buf)
            .expect("OS CSPRNG should always be available for mint_part_id");
        for &b in &buf {
            if b < limit {
                out.push(alphabet[(b % n) as usize]);
                if out.chars().count() == PART_ID_LEN {
                    break;
                }
            }
        }
    }
    out
}

// -------------------------------------------------------------------
// Errors
// -------------------------------------------------------------------

/// Errors surfaced by `run_*`.
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("config: {0}")]
    Config(#[from] qx_config::ConfigError),
    #[error("storage: {0}")]
    Storage(#[from] qx_storage::RepoError),
    #[error("identity: {0}")]
    Identity(#[from] qx_identity::IdentityError),
    #[error("transport: {0}")]
    Transport(#[from] ProposalError),
    #[error("codec: {0}")]
    Codec(#[from] qx_codec::CodecError),
    #[error("invalid argument: {0}")]
    BadArg(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("ambiguous: {0}")]
    Ambiguous(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("data-repo bootstrap: {0}")]
    Bootstrap(String),
    #[error("{0}")]
    Other(String),
}

// -------------------------------------------------------------------
// Tape presets (parity with label.py:TAPE_SIZES)
// -------------------------------------------------------------------

/// Brother P-touch / DK tape presets. Parity with `label.py:52-62`.
/// Returns `Some(mm)` for known tape codes; `None` otherwise.
pub fn tape_size_mm(tape: &str) -> Option<f64> {
    Some(match tape {
        "pt-9" => 6.5,
        "pt-12" => 9.0,
        "pt-18" => 12.0,
        "pt-24" => 18.0,
        "pt-36" => 28.0,
        "dk-12" => 10.0,
        "dk-29" => 25.0,
        "dk-38" => 33.0,
        "dk-62" => 56.0,
        _ => return None,
    })
}

/// List of valid tape preset names. Used for `--help` text + parity
/// with label.py's `choices=list(TAPE_SIZES)`.
pub const TAPE_NAMES: &[&str] = &[
    "pt-9", "pt-12", "pt-18", "pt-24", "pt-36", "dk-12", "dk-29", "dk-38", "dk-62",
];

/// `label.py:DEFAULT_SIZE_MM` — 11.0 mm.
pub const DEFAULT_SIZE_MM: f64 = 11.0;

// -------------------------------------------------------------------
// CLI args — shared layout enum
// -------------------------------------------------------------------

/// Layout selector mirroring `label.py --layout {vert,horz,flag}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum LayoutArg {
    /// QR on top of text. Aspect 1:2.
    Vert,
    /// QR left of text. Aspect 2:1.
    Horz,
    /// `horz` mirrored around a cable-wrap zone (requires --cable-od).
    Flag,
}

/// Text-format selector mirroring `label.py --format {4/4,4/4/4,5/5/4,auto}`.
///
/// `auto` defers to [`qx_codec::recommend_format`] at runtime.
/// Clap parses the slash-separated form for parity with the Python CLI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum FormatArg {
    /// 8 chars, 2 rows (4 + 4).
    #[value(name = "4/4")]
    FourFour,
    /// 12 chars, 3 rows (4 + 4 + 4).
    #[value(name = "4/4/4")]
    FourFourFour,
    /// 14 chars, 3 rows (5 + 5 + 4) — full canonical.
    #[value(name = "5/5/4")]
    FiveFiveFour,
    /// Auto-select by size tier (default).
    #[value(name = "auto")]
    Auto,
}

/// Status filter for `label --status`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum StatusArg {
    Unbound,
    Bound,
    Void,
}

impl StatusArg {
    fn to_domain(self) -> PartStatus {
        match self {
            StatusArg::Unbound => PartStatus::Unbound,
            StatusArg::Bound => PartStatus::Bound,
            StatusArg::Void => PartStatus::Void,
        }
    }
}

// -------------------------------------------------------------------
// Mint CLI
// -------------------------------------------------------------------

/// `mint` — produce N fresh ADR-012 part IDs and propose them via the
/// configured `ProposalSink` (GitHub PR in production; local diff
/// capture with `--dry-run`).
///
/// Parity with `mint.py`: `--count` is required, `--batch` defaults
/// to `B-YYYY-MM-DD-HHMM`, stdout lists the minted IDs one per line
/// after a one-line summary.
#[derive(Parser, Debug, Clone)]
#[command(
    name = "mint",
    about = "Mint nano-id part IDs and propose them to the registry",
    long_about = "Mint N fresh ADR-012 part IDs (14-char alphanumeric, no \
                  look-alikes) and propose them via the configured \
                  ProposalSink. With --dry-run the proposal is captured \
                  locally as JSON; without, a GitHub PR is opened.\n\n\
                  AuditEntry rows for each mint flow to audit_log.csv \
                  per ADR-022."
)]
pub struct MintArgs {
    /// Number of part IDs to mint (>= 1). Parity with `mint.py --count`.
    #[arg(long, required = true)]
    pub count: u32,

    /// Suppress ProposalSink submission; write the proposal JSON to
    /// stdout or `--dry-run-file` instead. The minted-IDs summary is
    /// still printed to stdout (after the proposal JSON when stdout
    /// is the sink target, otherwise immediately).
    #[arg(long)]
    pub dry_run: bool,

    /// Write the dry-run proposal JSON to this file instead of stdout.
    /// Implies `--dry-run`.
    #[arg(long)]
    pub dry_run_file: Option<PathBuf>,

    /// **Dev-only** — the `--local` flag is exposed only in builds
    /// compiled with the `dev-local` cargo feature, so it can never be
    /// invoked from the default/release `pr` (clap skips it otherwise,
    /// pinning it to `false`). When set, the mint is applied straight
    /// to the local `registry.csv` via [`LocalRegistrySink`] instead
    /// of opening a GitHub PR, restoring the old `mint.py` mint ->
    /// immediately-renderable loop for development. No token required.
    #[cfg_attr(feature = "dev-local", arg(long))]
    #[cfg_attr(not(feature = "dev-local"), arg(skip))]
    pub local: bool,
}

// -------------------------------------------------------------------
// Label CLI
// -------------------------------------------------------------------

/// `label` — render SVG labels for IDs already in the registry.
///
/// Parity with `label.py`. Selection is by any combination of
/// `--id`/`--batch`/`--status`; geometry by `--size`/`--tape`; text
/// by `--format`. Cable-flag layouts require `--cable-od`. Print
/// events are appended to `print_log.csv` via the storage adapter
/// (ADR-015) unless `--no-log` is passed.
#[derive(Parser, Debug, Clone)]
#[command(
    name = "label",
    about = "Render SVG labels for part IDs already in the registry",
    long_about = "Render SVG labels for one or more part IDs. A label is \
                  two equal-size square blocks (QR + text), assembled as \
                  vert (1:2), horz (2:1), or flag (horz mirrored around \
                  a cable-wrap zone, requires --cable-od).\n\n\
                  Selection is by any combination of --id/--status. \
                  Geometry: --size or --tape (presets pt-9..dk-62). \
                  Text format: --format (auto by size tier by default).\n\n\
                  Per ADR-015, a row per ID is appended to print_log.csv \
                  unless --no-log is passed."
)]
pub struct LabelArgs {
    /// Explicit ID. Repeat for multiple. Parity with `label.py --id`.
    #[arg(long = "id", value_name = "ID")]
    pub ids: Vec<String>,

    /// Render every ID with this status. Parity with `label.py --status`.
    #[arg(long, value_enum)]
    pub status: Option<StatusArg>,

    /// Label layout. Parity with `label.py --layout` (default: horz).
    #[arg(long, value_enum, default_value_t = LayoutArg::Horz)]
    pub layout: LayoutArg,

    /// Short-side size in mm (default 11). Parity with `label.py --size`.
    #[arg(long)]
    pub size: Option<f64>,

    /// Tape preset (shorthand for --size). Parity with `label.py --tape`.
    /// See `TAPE_NAMES` for valid values.
    #[arg(long)]
    pub tape: Option<String>,

    /// Text format. `auto` picks by size tier (default). Parity with
    /// `label.py --format`.
    #[arg(long, value_enum, default_value_t = FormatArg::Auto)]
    pub format: FormatArg,

    /// Cable outer diameter in mm (required for `--layout flag`).
    #[arg(long = "cable-od")]
    pub cable_od: Option<f64>,

    /// Output directory (default: `labels/<descriptor>-<layout>-s<size>`).
    /// Parity with `label.py --out-dir`.
    #[arg(long = "out-dir")]
    pub out_dir: Option<PathBuf>,

    /// Copies per ID (recorded in print_log; default 1). Does not
    /// duplicate rendered SVGs. Parity with `label.py --copies`.
    #[arg(long, default_value_t = 1)]
    pub copies: u32,

    /// Do not append rows to `print_log.csv`. Default is to log.
    /// Parity with `label.py --no-log`.
    #[arg(long = "no-log", action = clap::ArgAction::SetTrue)]
    pub no_log: bool,

    /// Operator name recorded in `print_log.printed_by` (default: $USER).
    /// Parity with `label.py --operator`.
    #[arg(long)]
    pub operator: Option<String>,

    /// Print-pipeline descriptor recorded in `print_log.output_mode`.
    /// Parity with `label.py --output-mode`.
    #[arg(long = "output-mode", default_value = "dk-continuous-auto-cut")]
    pub output_mode: String,

    /// Encode as Micro QR M4 instead of Standard QR V1. Parity with
    /// `label.py --micro`.
    #[arg(long)]
    pub micro: bool,
}

// -------------------------------------------------------------------
// Bind CLI
// -------------------------------------------------------------------

/// `bind` — bind an unbound part ID to physical-part metadata.
///
/// Parity with `bind.py`: the positional argument is either the full
/// 14-char canonical ID or the 8-char human prefix (dashes / spaces
/// stripped; uppercased). Prefix collisions print all matches and
/// exit non-zero without binding.
#[derive(Parser, Debug, Clone)]
#[command(
    name = "bind",
    about = "Bind an unbound part ID to physical-part metadata",
    long_about = "Bind a part ID — full 14-char canonical or 8-char human \
                  prefix — to a row of metadata: type, vendor, part-number, \
                  location, etc. Submits a Diff via the configured \
                  ProposalSink (GitHub PR in production; --dry-run \
                  captures locally).\n\n\
                  --rebind allows rewriting metadata on an already-bound \
                  ID. --void marks the ID as void (sticker spoiled / lost) \
                  instead of binding."
)]
pub struct BindArgs {
    /// Full 14-char canonical or 8-char human prefix. Parity with
    /// `bind.py` positional `id`.
    pub id: String,

    /// Part type, e.g. "PT100 1/3 DIN class B, 4-wire".
    #[arg(long = "type")]
    pub type_: Option<String>,

    /// Free-text description.
    #[arg(long)]
    pub description: Option<String>,

    /// Vendor name.
    #[arg(long)]
    pub vendor: Option<String>,

    /// Vendor part number.
    #[arg(long = "part-number")]
    pub part_number: Option<String>,

    /// Where the part lives, e.g. "sdmd_v2 / cooling-loop".
    #[arg(long)]
    pub location: Option<String>,

    /// Free-text notes.
    #[arg(long)]
    pub notes: Option<String>,

    /// Allow rewriting metadata on an already-bound ID.
    #[arg(long)]
    pub rebind: bool,

    /// Mark this ID as void (sticker spoiled / lost) instead of binding.
    #[arg(long)]
    pub void: bool,

    /// Suppress ProposalSink submission; write the proposal JSON to
    /// stdout (or `--dry-run-file`) instead.
    #[arg(long)]
    pub dry_run: bool,

    /// Write the dry-run proposal JSON to this file instead of stdout.
    /// Implies `--dry-run`.
    #[arg(long)]
    pub dry_run_file: Option<PathBuf>,
}

// -------------------------------------------------------------------
// Wiring — adapters injected into run_*
// -------------------------------------------------------------------

/// Adapter bundle for `run_*`. Tests build this from doubles;
/// production builds via [`Wiring::from_config`].
pub struct Wiring {
    pub repo: Arc<dyn Repository>,
    pub identity: Box<dyn IdentityProvider>,
    pub sink: Box<dyn ProposalSink>,
    /// Local clone path for filesystem-rooted operations (label
    /// `--out-dir` default base, etc.). Set to the same path as the
    /// `Repository` backend in production.
    pub repo_root: PathBuf,
}

impl Wiring {
    /// Build the production wiring from a loaded [`Config`].
    ///
    /// Picks adapters per ADR-021's `PART_REGISTRY_*` env vars. At
    /// MVP the only supported storage adapter is `csv_git`; the
    /// only identity adapter for the CLI surface is `git_config`;
    /// the only proposal sink is `github_pr` — or [`DryRunSink`] when
    /// `dry_run` is requested.
    pub fn from_config(cfg: &Config, dry_run: Option<DryRunTarget>) -> Result<Self, CliError> {
        // Repository ---------------------------------------------------
        if cfg.storage.adapter != StorageAdapterChoice::CsvGit {
            return Err(CliError::BadArg(format!(
                "unsupported storage adapter {:?}; only `csv_git` is wired today",
                cfg.storage.adapter
            )));
        }
        // Resolve the on-disk clone path (XDG-derived by default per
        // #35) and bootstrap the data repo into it. Idempotent: clone
        // if missing, fetch+reset if present. Honours
        // `PARTREG_OFFLINE=true` for hermetic test/dev runs.
        let repo_path = cfg.resolve_data_path()?;
        bootstrap_data_repo(&cfg.repo.data_repo_url, &cfg.repo.branch, &repo_path)?;
        // Detect the storage shape (ADR-035 cutover): a legacy
        // `registry.csv` selects the CSV adapter; a qx-init'd repo (no
        // registry.csv, `collections/*.jsonl` instead) is JSONL-native.
        let repo_arc: Arc<dyn Repository> = if repo_path.join("registry.csv").exists() {
            let mut csv_cfg = CsvGitConfig::new(repo_path.clone());
            // The CLI does not commit on audit-append — left to the
            // data-repo automation (signed commits via transport_github_pr).
            csv_cfg.commit_on_write = false;
            Arc::new(CsvGitRepository::open(repo_path.clone(), csv_cfg)?)
        } else {
            let jsonl_cfg = qx_storage_jsonl_git::JsonlGitConfig::new(repo_path.clone());
            Arc::new(qx_storage_jsonl_git::JsonlGitRepository::open(
                repo_path.clone(),
                jsonl_cfg,
            )?)
        };

        // Identity -----------------------------------------------------
        let identity: Box<dyn IdentityProvider> = match cfg.identity.adapter {
            IdentityAdapterChoice::GitConfig => Box::new(GitConfigIdentity::new()),
            other => {
                return Err(CliError::BadArg(format!(
                    "unsupported identity adapter {other:?}; CLI supports `git_config`"
                )));
            }
        };

        // Proposal sink ------------------------------------------------
        // Live path per ADR-030 build-order step 2: the fully-built
        // `GithubPrProposalSink` wires through Config. Token resolves
        // per ADR-020 §"Credential resolution order": explicit config
        // (PART_REGISTRY__TRANSPORT__GITHUB_TOKEN) → conventional
        // GITHUB_TOKEN / GH_TOKEN. (Cached-token + device-flow live in
        // the identity adapter; system-auth borrow is tracked.)
        let sink: Box<dyn ProposalSink> = if let Some(target) = dry_run {
            Box::new(DryRunSink::new(target))
        } else {
            if cfg.transport.adapter != qx_config::TransportAdapterChoice::GithubPr {
                return Err(CliError::BadArg(format!(
                    "unsupported transport adapter {:?}; live submission supports \
                     `github_pr` (or pass --dry-run)",
                    cfg.transport.adapter
                )));
            }
            let token = resolve_github_token(cfg).ok_or_else(|| {
                CliError::BadArg(
                    "live GitHub PR submission needs a token: set \
                     PART_REGISTRY__TRANSPORT__GITHUB_TOKEN (or GITHUB_TOKEN / \
                     GH_TOKEN), or pass --dry-run to capture the proposal locally."
                        .into(),
                )
            })?;
            let (owner, name) = qx_config::parse_owner_repo(&cfg.repo.data_repo_url)?;
            let http = ReqwestGithubPrHttp::new(token)
                .map_err(|e| CliError::Other(format!("github http client: {e}")))?;
            let gh_cfg = GithubPrConfig {
                data_repo_owner: owner,
                data_repo_name: name,
                base_branch: cfg.repo.branch.clone(),
                branch_prefix: "proposal/".into(),
                // Contents-API committer pair (the PR author is the
                // token's user; the *operator* travels in the proposal
                // body per ADR-019/020).
                commit_author_name: "qx".into(),
                commit_author_email: "qx@users.noreply.github.com".into(),
            };
            Box::new(GithubPrProposalSink::new(http, gh_cfg))
        };

        Ok(Self {
            repo: repo_arc,
            identity,
            sink,
            repo_root: repo_path,
        })
    }
}

/// Resolve the GitHub token per ADR-020 §"Credential resolution order"
/// (first hit wins): explicit config → `GITHUB_TOKEN` → `GH_TOKEN`.
fn resolve_github_token(cfg: &Config) -> Option<String> {
    resolve_github_token_from(cfg, |k| std::env::var(k).ok())
}

/// Testable form: the env lookup is injected so tests don't race on
/// process-global env (same pattern as `bootstrap_data_repo_with_options`).
fn resolve_github_token_from(cfg: &Config, env: impl Fn(&str) -> Option<String>) -> Option<String> {
    // Blank values are "unset" per source, so a blank earlier source
    // falls through to the next instead of short-circuiting the chain.
    let non_blank = |t: String| {
        if t.trim().is_empty() {
            None
        } else {
            Some(t)
        }
    };
    cfg.transport
        .github_token
        .clone()
        .and_then(non_blank)
        .or_else(|| env("GITHUB_TOKEN").and_then(non_blank))
        .or_else(|| env("GH_TOKEN").and_then(non_blank))
}

// -------------------------------------------------------------------
// Data-repo bootstrap (per #35)
// -------------------------------------------------------------------

/// Ensure the data repo is cloned at `target`, and refresh it to
/// `branch`.
///
/// - If `target` does not exist (or is empty), runs `git clone --branch
///   <branch> --depth 1 <url> <target>`.
/// - If `target` is already a git working tree, runs `git fetch
///   origin <branch>` then `git reset --hard origin/<branch>`. The
///   reset is load-bearing: the CLI is read-only locally (mutations
///   route through `ProposalSink`), so any local divergence is
///   transient state we want to drop.
///
/// Honours the `PARTREG_OFFLINE=true` env var: if set, the function
/// only verifies that `target` exists and looks like a git working
/// tree, performing no network I/O. Used in CI and in tests that
/// pre-seed a tempdir via test helpers.
///
/// Shells out to the `git` CLI rather than depending on `git2` /
/// `gitoxide` — the foundation crate set is already heavy, and `git`
/// is universally available in CI + dev environments.
pub fn bootstrap_data_repo(url: &str, branch: &str, target: &Path) -> Result<(), CliError> {
    let offline = std::env::var("PARTREG_OFFLINE")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);
    bootstrap_data_repo_with_options(url, branch, target, offline)
}

/// Same as [`bootstrap_data_repo`] but with the offline flag passed
/// in explicitly. Lets tests exercise the offline path without
/// mutating the process environment (which races under `cargo test`'s
/// default parallel scheduler).
pub fn bootstrap_data_repo_with_options(
    url: &str,
    branch: &str,
    target: &Path,
    offline: bool,
) -> Result<(), CliError> {
    if offline {
        if !target.exists() {
            return Err(CliError::Bootstrap(format!(
                "PARTREG_OFFLINE=true but no clone at {target:?} — \
                 pre-seed the directory or unset PARTREG_OFFLINE"
            )));
        }
        return Ok(());
    }

    // Treat "no dir" / "empty dir" / "non-git dir" as "needs clone".
    let needs_clone = match fs::read_dir(target) {
        Err(_) => true,
        Ok(mut entries) => entries.next().is_none(),
    } || !target.join(".git").exists();

    if needs_clone {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                CliError::Bootstrap(format!("creating parent dir {parent:?} for clone: {e}"))
            })?;
        }
        // If the directory exists but is empty / non-git, remove it
        // first so `git clone` doesn't refuse with "destination path
        // already exists".
        if target.exists() {
            // Best-effort: only delete if we created it as a stub.
            // `remove_dir_all` is safe for empty/junk dirs but we
            // guard against eating an unrelated populated dir.
            if let Ok(mut entries) = fs::read_dir(target) {
                if entries.next().is_none() {
                    let _ = fs::remove_dir(target);
                }
            }
        }
        run_git(
            &["clone", "--branch", branch, "--depth", "1", url],
            None,
            Some(target),
        )?;
        return Ok(());
    }

    // Refresh existing clone. `reset --hard` only touches tracked
    // files; `clean -fdx` drops untracked + ignored noise so the
    // working tree is byte-identical to `origin/<branch>` afterwards.
    // We want this strict equivalence because audit-defensibility
    // hinges on the CLI seeing the same bytes the upstream policy CI
    // saw when it accepted the last merged PR.
    run_git(&["fetch", "origin", branch], Some(target), None)?;
    run_git(
        &["reset", "--hard", &format!("origin/{branch}")],
        Some(target),
        None,
    )?;
    run_git(&["clean", "-fdx"], Some(target), None)?;
    Ok(())
}

/// Run `git <args...>` with optional `cwd` and an optional final
/// positional argument (used for `git clone <url> <dest>` where the
/// dest is the cwd's *parent*-relative path).
fn run_git(args: &[&str], cwd: Option<&Path>, clone_dest: Option<&Path>) -> Result<(), CliError> {
    let mut cmd = std::process::Command::new("git");
    for a in args {
        cmd.arg(a);
    }
    if let Some(dest) = clone_dest {
        cmd.arg(dest);
    }
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    let output = cmd
        .output()
        .map_err(|e| CliError::Bootstrap(format!("spawn `git {}`: {e}", args.join(" "))))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CliError::Bootstrap(format!(
            "`git {}` failed ({}): {}",
            args.join(" "),
            output.status,
            stderr.trim()
        )));
    }
    Ok(())
}

// -------------------------------------------------------------------
// DryRunSink — local capture of submitted Proposals
// -------------------------------------------------------------------

/// Where a [`DryRunSink`] writes its captured proposals.
#[derive(Clone, Debug)]
pub enum DryRunTarget {
    /// Write to stdout. One JSON line per submission.
    Stdout,
    /// Append one JSON line per submission to this file. Created if
    /// it does not exist.
    File(PathBuf),
    /// Capture in-memory for tests. The `Arc<Mutex<...>>` is the
    /// test's hand-off point.
    Memory(Arc<Mutex<Vec<Proposal>>>),
}

/// Local-capture `ProposalSink` for `--dry-run` and tests. Records
/// every submitted [`Proposal`] without touching the network.
pub struct DryRunSink {
    target: DryRunTarget,
    next_id: Mutex<u64>,
}

impl DryRunSink {
    pub fn new(target: DryRunTarget) -> Self {
        Self {
            target,
            next_id: Mutex::new(0),
        }
    }

    /// Test convenience — capture every proposal in memory.
    pub fn in_memory() -> (Self, Arc<Mutex<Vec<Proposal>>>) {
        let store = Arc::new(Mutex::new(Vec::new()));
        let sink = Self::new(DryRunTarget::Memory(store.clone()));
        (sink, store)
    }
}

impl ProposalSink for DryRunSink {
    fn submit(&self, proposal: Proposal) -> Result<ProposalRef, ProposalError> {
        let id_num = {
            let mut g = self.next_id.lock().map_err(|_| {
                ProposalError::Backend("dry-run sink mutex poisoned".to_owned().into())
            })?;
            *g += 1;
            *g
        };
        let local_id = format!("dry-run-{id_num}");
        let request_id = proposal.request_id;
        let payload = serde_json::to_string(&proposal)
            .map_err(|e| ProposalError::Backend(format!("encode dry-run proposal: {e}").into()))?;
        match &self.target {
            DryRunTarget::Stdout => {
                // Stdout IS the dry-run output channel (consumers
                // capture it); the rest of the CLI uses tracing.
                println!("{payload}"); // guardrails-ok
            }
            DryRunTarget::File(path) => {
                let mut f = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .map_err(|e| {
                        ProposalError::Backend(
                            format!("open dry-run file {}: {e}", path.display()).into(),
                        )
                    })?;
                writeln!(f, "{payload}").map_err(|e| {
                    ProposalError::Backend(format!("write dry-run file: {e}").into())
                })?;
            }
            DryRunTarget::Memory(store) => {
                let mut g = store.lock().map_err(|_| {
                    ProposalError::Backend("dry-run memory store mutex poisoned".to_owned().into())
                })?;
                g.push(proposal);
            }
        }
        Ok(ProposalRef {
            url: format!("file://dry-run/{local_id}"),
            local_id: Some(local_id),
            adapter: "dry_run".into(),
            // request_id is not on ProposalRef; we encode it into the
            // URL fragment so test assertions can recover it.
        })
        .map(|mut r| {
            r.url = format!("{}#request_id={request_id}", r.url);
            r
        })
    }

    fn status(&self, _proposal_ref: &ProposalRef) -> Result<ProposalStatus, ProposalError> {
        // Dry-run proposals are always "open" — they never close.
        Ok(ProposalStatus::Open)
    }
}

// -------------------------------------------------------------------
// LocalRegistrySink — DEV-ONLY direct write to the local registry.csv
// -------------------------------------------------------------------

/// **Dev-only** `ProposalSink` that applies a proposal's diff straight
/// to the local working-copy `registry.csv`, bypassing the GitHub PR
/// flow (ADR-019). It restores the pre-Rust `mint.py` loop: minted IDs
/// are immediately renderable by `qx print` / `qx label` in the same
/// checkout, with no token, network, or PR round-trip.
///
/// Gated behind the `dev-local` cargo feature so the entire type — and
/// the `qx mint --local` flag that reaches it — is **compiled out of
/// the default/release build** and can never ship in the production
/// tool (the policy authority stays CI + reviewers per ADR-016/019).
///
/// Scope: applies `diff.adds` only (what `mint` produces). Rows are
/// re-sorted ascending by `id` to preserve the registry's canonical
/// CSV order (the same order `run_mint` assumes).
#[cfg(feature = "dev-local")]
pub struct LocalRegistrySink {
    registry_csv: PathBuf,
}

#[cfg(feature = "dev-local")]
impl LocalRegistrySink {
    /// `repo_root` is the local data-repo clone path
    /// ([`Wiring::repo_root`]); `registry.csv` sits at its root.
    pub fn new(repo_root: PathBuf) -> Self {
        Self {
            registry_csv: repo_root.join("registry.csv"),
        }
    }
}

#[cfg(feature = "dev-local")]
impl ProposalSink for LocalRegistrySink {
    fn submit(&self, proposal: Proposal) -> Result<ProposalRef, ProposalError> {
        let path = &self.registry_csv;
        let backend = |e: String| ProposalError::Backend(e.into());

        // Read existing header + rows.
        let mut rdr = csv::Reader::from_path(path)
            .map_err(|e| backend(format!("open {}: {e}", path.display())))?;
        let header: Vec<String> = rdr
            .headers()
            .map_err(|e| backend(format!("read header {}: {e}", path.display())))?
            .iter()
            .map(|s| s.to_owned())
            .collect();
        let id_col = header
            .iter()
            .position(|c| c == "id")
            .ok_or_else(|| backend(format!("{}: no `id` column", path.display())))?;
        let mut rows: Vec<Vec<String>> = Vec::new();
        for rec in rdr.records() {
            let rec = rec.map_err(|e| backend(format!("read row: {e}")))?;
            rows.push(rec.iter().map(|s| s.to_owned()).collect());
        }

        // Apply adds — project each DiffRow onto the header columns.
        let added = proposal.diff.adds.len();
        for add in &proposal.diff.adds {
            let id = add
                .id
                .as_ref()
                .map(|p| p.as_str().to_owned())
                .unwrap_or_default();
            let row: Vec<String> = header
                .iter()
                .map(|col| {
                    if col == "id" {
                        id.clone()
                    } else {
                        add.fields.get(col).cloned().unwrap_or_default()
                    }
                })
                .collect();
            rows.push(row);
        }

        // Keep canonical ascending-by-id CSV order.
        rows.sort_by(|a, b| a.get(id_col).cmp(&b.get(id_col)));

        // Write back (truncate + rewrite).
        let mut wtr = csv::Writer::from_path(path)
            .map_err(|e| backend(format!("open for write {}: {e}", path.display())))?;
        wtr.write_record(&header)
            .map_err(|e| backend(format!("write header: {e}")))?;
        for r in &rows {
            wtr.write_record(r)
                .map_err(|e| backend(format!("write row: {e}")))?;
        }
        wtr.flush()
            .map_err(|e| backend(format!("flush {}: {e}", path.display())))?;

        Ok(ProposalRef {
            url: format!("local://{}#adds={added}", path.display()),
            local_id: Some("local".into()),
            adapter: "local".into(),
        })
    }

    fn status(&self, _proposal_ref: &ProposalRef) -> Result<ProposalStatus, ProposalError> {
        // A local apply is effectively merged the instant it returns.
        Ok(ProposalStatus::Merged)
    }
}

// -------------------------------------------------------------------
// init_observability — shared init pattern
// -------------------------------------------------------------------

/// Initialise the global tracing subscriber + audit-CSV layer.
///
/// `repo` is wrapped in an [`AuditSinkHandle`] so audit emits land in
/// the same `Repository` the binary already opened. `audit_csv` is
/// always enabled for mutating CLIs per ADR-022.
///
/// Returns a [`InitGuard`] that should be dropped at the end of
/// `main` to ensure log buffers flush. Idempotent: re-initialisation
/// after the first call is silently ignored (tests use this).
pub fn init_observability(
    cfg: &ObservabilityConfig,
    repo: Arc<dyn Repository>,
) -> Result<(), qx_observability::InitError> {
    // The AuditSinkHandle wants `Box<dyn Repository>`, not `Arc`.
    // Wrap the Arc in a thin shim so multiple holders are fine.
    let shim: Box<dyn Repository> = Box::new(ArcRepository(repo));
    let sink = if cfg.audit_csv {
        AuditSinkHandle::new(shim)
    } else {
        AuditSinkHandle::disabled()
    };
    match init(cfg, sink) {
        Ok(()) => Ok(()),
        // Re-init in the same process (CLI tests) is fine — the
        // global subscriber stays installed from the first call.
        Err(qx_observability::InitError::AlreadyInit(_)) => Ok(()),
        Err(e) => Err(e),
    }
}

/// Thin `Repository` wrapper that delegates through an `Arc`. Lets
/// `init_observability` share one repo handle with the rest of the
/// binary.
struct ArcRepository(Arc<dyn Repository>);

impl Repository for ArcRepository {
    fn get_part(&self, id: &PartId) -> Result<Option<Part>, qx_storage::RepoError> {
        self.0.get_part(id)
    }

    fn list_parts(&self, filter: &PartFilter) -> Result<Vec<Part>, qx_storage::RepoError> {
        self.0.list_parts(filter)
    }

    fn list_audit_events(
        &self,
        filter: &qx_storage::AuditFilter,
    ) -> Result<Vec<AuditEntry>, qx_storage::RepoError> {
        self.0.list_audit_events(filter)
    }

    fn append_audit_event(&self, ev: AuditEntry) -> Result<(), qx_storage::RepoError> {
        self.0.append_audit_event(ev)
    }

    fn snapshot_hash(&self) -> Result<qx_domain::Hash, qx_storage::RepoError> {
        self.0.snapshot_hash()
    }
}

// -------------------------------------------------------------------
// run_mint
// -------------------------------------------------------------------

/// Outcome of a successful `mint` run. Returned for test inspection
/// and stdout formatting by `main()`.
#[derive(Debug, Clone)]
pub struct MintOutcome {
    pub minted: Vec<PartId>,
    pub minted_at: OffsetDateTime,
    pub proposal_ref: ProposalRef,
}

/// Execute a `mint` invocation against the provided wiring.
///
/// Per `mint.py`:
/// 1. Generate N fresh IDs disjoint from `Repository::list_parts`.
/// 2. Construct a [`Diff`] with N `RowAdd` entries.
/// 3. Submit via [`Wiring::sink`].
/// 4. Emit one `AuditEntry::RowAdd` per minted ID via the
///    observability layer (which routes to `audit_log.csv`).
pub fn run_mint(args: &MintArgs, wiring: &Wiring) -> Result<MintOutcome, CliError> {
    if args.count < 1 {
        return Err(CliError::BadArg("count must be >= 1".into()));
    }

    let operator = wiring.identity.current()?;
    let _op_guard = OperatorGuard::new(operator.clone());

    let minted_at = OffsetDateTime::now_utc();

    // Existing IDs (sorted ascending by id, which is the natural CSV
    // order). PartFilter::default() returns every status.
    let existing_parts = wiring.repo.list_parts(&PartFilter::default())?;
    let mut existing: HashSet<String> = existing_parts
        .iter()
        .map(|p| p.id.as_str().to_owned())
        .collect();

    // Mint N IDs.
    let mut new_ids: Vec<PartId> = Vec::with_capacity(args.count as usize);
    for _ in 0..args.count {
        let id = mint_part_id(&existing)?;
        existing.insert(id.as_str().to_owned());
        new_ids.push(id);
    }

    // Build the Diff (N adds).
    let diff = build_mint_diff(&new_ids, minted_at, &operator)?;
    let request_id = RequestId::new();
    let proposal = Proposal {
        diff: diff.clone(),
        batch_label: None,
        author: operator.clone(),
        signatures: Vec::new(),
        change_classification: diff.classify(),
        message: format!("mint: {n} new IDs", n = new_ids.len()),
        request_id,
    };

    let proposal_ref = wiring.sink.submit(proposal)?;

    // Emit one AuditEntry per minted ID. The audit-CSV layer (when
    // enabled) routes each into `audit_log.csv`. We also write each
    // entry directly via `Repository::append_audit_event` so the
    // round-trip is testable without a global subscriber.
    for id in &new_ids {
        let extra = json!({ "proposal": proposal_ref.url });
        let entry = mint_audit_entry(request_id, operator.clone(), id.clone(), extra);
        emit_audit(&entry);
        // Direct append for tests + audit independence from tracing
        // global state (ADR-022 §"audit_csv_layer fails open").
        if let Err(e) = wiring.repo.append_audit_event(entry) {
            tracing::warn!(error = %e, "append_audit_event failed; tracing layer is the fallback");
        }
    }

    Ok(MintOutcome {
        minted: new_ids,
        minted_at,
        proposal_ref,
    })
}

/// Stdout summary mirroring `mint.py`'s output. Test callers compare
/// against this to lock the parity contract.
pub fn render_mint_summary(outcome: &MintOutcome, repo_root: &Path) -> String {
    let registry_path = repo_root.join("registry.csv");
    let mut s = String::new();
    s.push_str(&format!("minted {n} ids\n", n = outcome.minted.len()));
    s.push_str(&format!("  registry: {}\n", registry_path.display()));
    for id in &outcome.minted {
        s.push_str(&format!("    {id}\n"));
    }
    s.push_str("\nrender labels:  label <id> --layout horz\n");
    s
}
fn build_mint_diff(
    new_ids: &[PartId],
    minted_at: OffsetDateTime,
    operator: &Operator,
) -> Result<Diff, CliError> {
    let ts = minted_at
        .format(&Rfc3339)
        .map_err(|e| CliError::Other(format!("format minted_at: {e}")))?;
    let mut adds = Vec::with_capacity(new_ids.len());
    for id in new_ids {
        let mut fields = BTreeMap::new();
        fields.insert("status".into(), "unbound".into());
        fields.insert("minted_at".into(), ts.clone());
        fields.insert("minted_by".into(), operator.id.0.clone());
        adds.push(DiffRow {
            id: Some(id.clone()),
            fields,
        });
    }
    Ok(Diff {
        adds,
        ..Diff::default()
    })
}

// -------------------------------------------------------------------
// run_label
// -------------------------------------------------------------------

/// Outcome of a successful `label` run. Returned for test inspection.
#[derive(Debug, Clone)]
pub struct LabelOutcome {
    pub rendered: Vec<RenderedLabel>,
    pub out_dir: PathBuf,
    pub layout: LayoutArg,
    pub size_mm: f64,
    pub format: TextFormat,
    pub logged: bool,
    /// Stderr-bound warning, if format auto-select / check produced one.
    pub warning: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RenderedLabel {
    pub id: PartId,
    pub path: PathBuf,
    pub svg: String,
}

/// Execute a `label` invocation against the provided wiring.
///
/// Per `label.py`: select IDs by `--id`/`--batch`/`--status`, render
/// one SVG per ID via `crates/codec::render_label`, optionally append
/// a print event per ID via `Repository::append_print_event`.
pub fn run_label(args: &LabelArgs, wiring: &Wiring) -> Result<LabelOutcome, CliError> {
    if args.copies < 1 {
        return Err(CliError::BadArg("--copies must be >= 1".into()));
    }
    if args.tape.is_some() && args.size.is_some() {
        return Err(CliError::BadArg(
            "use either --size or --tape, not both".into(),
        ));
    }
    let size_mm = if let Some(t) = &args.tape {
        tape_size_mm(t).ok_or_else(|| {
            CliError::BadArg(format!(
                "unknown tape preset {t:?}; valid: {}",
                TAPE_NAMES.join(", ")
            ))
        })?
    } else {
        args.size.unwrap_or(DEFAULT_SIZE_MM)
    };

    // Resolve format: auto-select by size, or use explicit choice +
    // optional warning.
    let (text_format, warning) = match args.format {
        FormatArg::Auto => qx_codec::recommend_format(size_mm),
        explicit => {
            let f = match explicit {
                FormatArg::FourFour => TextFormat::FourFour,
                FormatArg::FourFourFour => TextFormat::FourFourFour,
                FormatArg::FiveFiveFour => TextFormat::FiveFiveFour,
                FormatArg::Auto => unreachable!(),
            };
            (f, qx_codec::check_format_warning(size_mm, f))
        }
    };

    if args.layout == LayoutArg::Flag && args.cable_od.is_none() {
        return Err(CliError::BadArg(
            "--layout flag requires --cable-od <mm>".into(),
        ));
    }

    let identity = wiring.identity.current().ok();
    let _op_guard = identity.as_ref().map(|op| OperatorGuard::new(op.clone()));

    // Selection.
    let all_parts = wiring.repo.list_parts(&PartFilter::default())?;
    let selected = select_parts(&all_parts, &args.ids, args.status)?;
    if selected.is_empty() {
        return Err(CliError::NotFound("no IDs matched the selection".into()));
    }

    // Out-dir.
    let descriptor = args
        .status
        .map(|s| s.to_domain().to_string())
        .unwrap_or_else(|| "ad-hoc".into());
    let layout_name = match args.layout {
        LayoutArg::Vert => "vert",
        LayoutArg::Horz => "horz",
        LayoutArg::Flag => "flag",
    };
    let out_dir = args.out_dir.clone().unwrap_or_else(|| {
        wiring
            .repo_root
            .join("labels")
            .join(format!("{descriptor}-{layout_name}-s{}", format_g(size_mm)))
    });
    fs::create_dir_all(&out_dir)
        .map_err(|e| CliError::Other(format!("create out-dir {}: {e}", out_dir.display())))?;

    // Render.
    let layout = match args.layout {
        LayoutArg::Vert => Layout::Vert,
        LayoutArg::Horz => Layout::Horz,
        LayoutArg::Flag => Layout::Flag {
            cable_od_mm: args.cable_od.expect("cable_od checked above"),
            no_markers: false,
            alignment_line: false,
        },
    };

    let mut rendered = Vec::with_capacity(selected.len());
    for part in &selected {
        let svg = render_label(part.id.as_str(), layout, size_mm, text_format, args.micro)?;
        let path = out_dir.join(format!("{}.svg", part.id));
        fs::write(&path, &svg)
            .map_err(|e| CliError::Other(format!("write {}: {e}", path.display())))?;
        rendered.push(RenderedLabel {
            id: part.id.clone(),
            path,
            svg,
        });
    }

    // Optionally append print events.
    let logged = !args.no_log;
    if logged {
        let operator = identity.clone().ok_or_else(|| {
            CliError::Identity(qx_identity::IdentityError::NoIdentity(
                "label --no-log not set but identity lookup failed; pass --no-log \
                 to skip print_log.csv writes"
                    .into(),
            ))
        })?;
        let operator_name = args.operator.clone().unwrap_or_else(|| {
            std::env::var("USER").unwrap_or_else(|_| operator.display_name.clone())
        });

        // Layout-specific extra.
        let extra = match args.layout {
            LayoutArg::Flag => json!({"cableOd": args.cable_od.unwrap_or(0.0)}),
            _ => json!({}),
        };

        let now = OffsetDateTime::now_utc();
        // Print-fold (ADR-022): the label print is a typed audit event on
        // the one spine, not a print_log row.
        let op = qx_domain::Operator {
            id: qx_domain::OperatorId(operator_name.clone()),
            display_name: operator_name.clone(),
            source: qx_domain::IdentitySource::GitConfig,
            verified_at: None,
            claims: std::collections::BTreeMap::new(),
            pubkey: None,
        };
        let request_id = qx_domain::RequestId::new();
        for part in &selected {
            let print_extra = json!({
                "layout": layout_name, "size_mm": size_mm,
                "output_mode": args.output_mode,
                "extra": extra,
            });
            let entry = qx_observability::print_audit_entry(
                request_id,
                op.clone(),
                part.id.clone(),
                args.copies,
                print_extra,
                now,
            );
            wiring.repo.append_audit_event(entry)?;
        }
    }

    Ok(LabelOutcome {
        rendered,
        out_dir,
        layout: args.layout,
        size_mm,
        format: text_format,
        logged,
        warning,
    })
}

/// Mirrors `label.py`'s final summary line for stdout. `dim_str` is
/// "W × H mm (wrap W2)" for flag, "W × H mm" otherwise.
pub fn render_label_summary(out: &LabelOutcome, cable_od: Option<f64>) -> String {
    let dim_str = match out.layout {
        LayoutArg::Vert => format!("{:.1} × {:.1} mm", out.size_mm, 2.0 * out.size_mm),
        LayoutArg::Horz => format!("{:.1} × {:.1} mm", 2.0 * out.size_mm, out.size_mm),
        LayoutArg::Flag => {
            let od = cable_od.unwrap_or(0.0);
            let wrap_w = std::f64::consts::PI * od * 1.1;
            format!(
                "{:.1} × {:.1} mm (wrap {:.1})",
                4.0 * out.size_mm + wrap_w,
                out.size_mm,
                wrap_w
            )
        }
    };
    let format_name = match out.format {
        TextFormat::FourFour => "4/4",
        TextFormat::FourFourFour => "4/4/4",
        TextFormat::FiveFiveFour => "5/5/4",
    };
    let layout_name = match out.layout {
        LayoutArg::Vert => "vert",
        LayoutArg::Horz => "horz",
        LayoutArg::Flag => "flag",
    };
    let mut s = String::new();
    s.push_str(&format!(
        "rendered {n} labels  layout={layout_name} format={format_name}  ({dim_str})\n",
        n = out.rendered.len(),
    ));
    s.push_str(&format!("  out: {}/\n", out.out_dir.display()));
    if out.logged {
        s.push_str(&format!(
            "  logged {n} print event(s) to print_log.csv\n",
            n = out.rendered.len()
        ));
    }
    s
}

fn select_parts(
    rows: &[Part],
    explicit_ids: &[String],
    status: Option<StatusArg>,
) -> Result<Vec<Part>, CliError> {
    if explicit_ids.is_empty() && status.is_none() {
        return Err(CliError::BadArg(
            "specify at least one of --id, --status".into(),
        ));
    }
    let mut selected: Vec<Part> = rows.to_vec();

    if !explicit_ids.is_empty() {
        let wanted: HashSet<String> = explicit_ids.iter().map(|s| normalize_id(s)).collect();
        selected.retain(|p| wanted.contains(p.id.as_str()));
        let have: HashSet<String> = selected.iter().map(|p| p.id.as_str().into()).collect();
        let missing: Vec<String> = wanted.difference(&have).cloned().collect();
        if !missing.is_empty() {
            let mut m = missing;
            m.sort();
            return Err(CliError::NotFound(format!(
                "unknown ID(s): {}",
                m.join(", ")
            )));
        }
    }
    if let Some(s) = status {
        let want = s.to_domain();
        selected.retain(|p| p.status == want);
    }
    Ok(selected)
}

/// Strip dashes / whitespace, uppercase. Mirrors `bind.py:normalize`.
pub fn normalize_id(query: &str) -> String {
    query
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .flat_map(|c| c.to_uppercase())
        .collect()
}

/// Mirror Python's `f"{x:g}"` for sizes — drops trailing `.0`.
fn format_g(v: f64) -> String {
    if v.fract() == 0.0 && v.is_finite() {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

// -------------------------------------------------------------------
// run_bind
// -------------------------------------------------------------------

/// ADR-012 prefix length (8 chars). Mirrors `registry_contract.py:HUMAN_LENGTH`.
pub const HUMAN_LENGTH: usize = 8;

/// Outcome of a successful `bind` run.
#[derive(Debug, Clone)]
pub struct BindOutcome {
    pub id: PartId,
    pub voided: bool,
    pub fields: BTreeMap<String, String>,
    pub proposal_ref: ProposalRef,
}

/// Execute a `bind` invocation against the provided wiring.
///
/// Per `bind.py`:
/// 1. Resolve the positional ID (full 14-char or 8-char prefix);
///    error on collision.
/// 2. Status transitions: `unbound -> bound`, or `* -> void` for
///    `--void`. Bound rows require `--rebind` to overwrite.
/// 3. Submit a `Diff` with one `RowEdit` (or `RowVoid`) via
///    `ProposalSink`.
/// 4. Emit an `AuditEntry` via the observability layer.
pub fn run_bind(args: &BindArgs, wiring: &Wiring) -> Result<BindOutcome, CliError> {
    if args.void && (args.type_.is_some() || args.vendor.is_some() || args.notes.is_some()) {
        // Parity with bind.py — silently ignore metadata on --void; guardrails-ok
        // the Python CLI tolerates this. We surface a warning via
        // tracing but proceed. Per issue #56, notes is also ignored
        // (Python appends the void marker to existing notes, ignoring
        // --notes).
        tracing::warn!("--void ignores metadata flags (type, vendor, notes)");
    }

    let operator = wiring.identity.current()?;
    let _op_guard = OperatorGuard::new(operator.clone());

    let all_parts = wiring.repo.list_parts(&PartFilter::default())?;
    let target = resolve_part(&all_parts, &args.id)?;

    let now = OffsetDateTime::now_utc();
    let now_iso = now
        .format(&Rfc3339)
        .map_err(|e| CliError::Other(format!("format now: {e}")))?;

    if args.void {
        // Parity with bind.py:98 — append `[voided <ts>]` to existing
        // notes; ignore `args.notes` (see issue #56).
        let existing_notes = target.notes.clone().unwrap_or_default();
        let reason = format!("{existing_notes} [voided {now_iso}]");
        let (proposal_ref, request_id) =
            submit_void(wiring, &operator, &target, &reason, now_iso.clone())?;
        let extra = json!({
            "proposal": proposal_ref.url,
            "voided_at": now_iso,
        });
        let entry = void_audit_entry(
            request_id,
            operator.clone(),
            target.id.clone(),
            reason,
            extra,
        );
        emit_audit(&entry);
        wiring.repo.append_audit_event(entry).ok();
        return Ok(BindOutcome {
            id: target.id,
            voided: true,
            fields: BTreeMap::new(),
            proposal_ref,
        });
    }

    // Standard bind path.
    if target.status == PartStatus::Bound && !args.rebind {
        return Err(CliError::BadArg(format!(
            "{} is already bound to {:?}. Pass --rebind to overwrite.",
            target.id,
            target
                .type_
                .clone()
                .or_else(|| target.description.clone())
                .unwrap_or_default()
        )));
    }
    if target.status == PartStatus::Void {
        return Err(CliError::BadArg(format!(
            "{} is voided; cannot bind. Mint a new ID.",
            target.id
        )));
    }

    let (before, after) = build_bind_fields(&target, args, &now_iso, &operator);
    let (proposal_ref, request_id) = submit_bind(wiring, &operator, &target, &before, &after)?;

    let extra = json!({
        "proposal": proposal_ref.url,
        "bound_at": now_iso,
    });
    let entry = bind_audit_entry(
        request_id,
        operator.clone(),
        target.id.clone(),
        after.clone(),
        extra,
    );
    emit_audit(&entry);
    wiring.repo.append_audit_event(entry).ok();

    Ok(BindOutcome {
        id: target.id,
        voided: false,
        fields: after,
        proposal_ref,
    })
}

fn resolve_part(rows: &[Part], query: &str) -> Result<Part, CliError> {
    let q = normalize_id(query);
    if q.len() == PART_ID_LEN {
        rows.iter()
            .find(|p| p.id.as_str() == q)
            .cloned()
            .ok_or_else(|| CliError::NotFound(format!("no match for {query:?}")))
    } else if q.len() >= HUMAN_LENGTH {
        let matches: Vec<&Part> = rows
            .iter()
            .filter(|p| p.id.as_str().starts_with(&q))
            .collect();
        match matches.len() {
            0 => Err(CliError::NotFound(format!("no match for {query:?}"))),
            1 => Ok(matches[0].clone()),
            n => {
                let mut detail = String::new();
                for m in &matches {
                    let label = m
                        .type_
                        .clone()
                        .or_else(|| m.description.clone())
                        .unwrap_or_else(|| "(unbound)".into());
                    let loc = m.location.clone().unwrap_or_else(|| "-".into());
                    detail.push_str(&format!(
                        "  {}  status={}  {}  @ {}\n",
                        m.id, m.status, label, loc
                    ));
                }
                Err(CliError::Ambiguous(format!(
                    "ambiguous prefix {query:?} — {n} matches:\n{detail}"
                )))
            }
        }
    } else {
        Err(CliError::BadArg(format!(
            "query too short ({} chars); need >= {HUMAN_LENGTH}",
            q.len()
        )))
    }
}

fn build_bind_fields(
    target: &Part,
    args: &BindArgs,
    now_iso: &str,
    operator: &Operator,
) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
    let mut before = BTreeMap::new();
    before.insert("status".into(), target.status.to_string());
    if let Some(t) = &target.type_ {
        before.insert("type".into(), t.clone());
    }
    if let Some(t) = &target.description {
        before.insert("description".into(), t.clone());
    }
    if let Some(t) = &target.vendor {
        before.insert("vendor".into(), t.clone());
    }
    if let Some(t) = &target.part_number {
        before.insert("part_number".into(), t.clone());
    }
    if let Some(t) = &target.location {
        before.insert("location".into(), t.clone());
    }
    if let Some(t) = &target.notes {
        before.insert("notes".into(), t.clone());
    }

    let mut after = BTreeMap::new();
    after.insert("status".into(), "bound".into());
    after.insert("bound_at".into(), now_iso.into());
    after.insert("bound_by".into(), operator.id.0.clone());
    let pick = |new: &Option<String>, old: &Option<String>| -> Option<String> {
        new.clone().or_else(|| old.clone())
    };
    if let Some(v) = pick(&args.type_, &target.type_) {
        after.insert("type".into(), v);
    }
    if let Some(v) = pick(&args.description, &target.description) {
        after.insert("description".into(), v);
    }
    if let Some(v) = pick(&args.vendor, &target.vendor) {
        after.insert("vendor".into(), v);
    }
    if let Some(v) = pick(&args.part_number, &target.part_number) {
        after.insert("part_number".into(), v);
    }
    if let Some(v) = pick(&args.location, &target.location) {
        after.insert("location".into(), v);
    }
    if let Some(v) = pick(&args.notes, &target.notes) {
        after.insert("notes".into(), v);
    }
    (before, after)
}

fn submit_bind(
    wiring: &Wiring,
    operator: &Operator,
    target: &Part,
    before: &BTreeMap<String, String>,
    after: &BTreeMap<String, String>,
) -> Result<(ProposalRef, RequestId), CliError> {
    let changed_keys: Vec<String> = after
        .iter()
        .filter(|(k, v)| before.get(k.as_str()) != Some(*v))
        .map(|(k, _)| k.clone())
        .collect();
    let diff = Diff {
        edits: vec![DiffEdit {
            id: target.id.clone(),
            before: before.clone(),
            after: after.clone(),
            changed_keys,
        }],
        ..Diff::default()
    };
    submit_diff(
        wiring,
        operator,
        diff,
        Some(&format!("bind: {}", target.id)),
    )
}

fn submit_void(
    wiring: &Wiring,
    operator: &Operator,
    target: &Part,
    reason: &str,
    now_iso: String,
) -> Result<(ProposalRef, RequestId), CliError> {
    let mut before = BTreeMap::new();
    before.insert("status".into(), target.status.to_string());
    if let Some(n) = &target.notes {
        before.insert("notes".into(), n.clone());
    }
    let mut after = BTreeMap::new();
    after.insert("status".into(), "void".into());
    after.insert("notes".into(), reason.into());
    let _ = now_iso; // currently encoded into `reason`
    let diff = Diff {
        edits: vec![DiffEdit {
            id: target.id.clone(),
            before: before.clone(),
            after,
            changed_keys: vec!["status".into(), "notes".into()],
        }],
        ..Diff::default()
    };
    submit_diff(
        wiring,
        operator,
        diff,
        Some(&format!("void: {}", target.id)),
    )
}

fn submit_diff(
    wiring: &Wiring,
    operator: &Operator,
    diff: Diff,
    message: Option<&str>,
) -> Result<(ProposalRef, RequestId), CliError> {
    let request_id = RequestId::new();
    let actions = diff.classify();
    let proposal = Proposal {
        diff,
        batch_label: None,
        author: operator.clone(),
        signatures: Vec::new(),
        change_classification: actions,
        message: message.unwrap_or("proposal").into(),
        request_id,
    };
    let proposal_ref = wiring.sink.submit(proposal)?;
    Ok((proposal_ref, request_id))
}

/// Mirrors `bind.py`'s stdout summary.
pub fn render_bind_summary(outcome: &BindOutcome) -> String {
    if outcome.voided {
        return format!("voided {}\n", outcome.id);
    }
    let mut s = format!("bound {}\n", outcome.id);
    for k in [
        "type",
        "description",
        "vendor",
        "part_number",
        "location",
        "notes",
    ] {
        if let Some(v) = outcome.fields.get(k) {
            if !v.is_empty() {
                s.push_str(&format!("  {k:14} {v}\n"));
            }
        }
    }
    s
}

// -------------------------------------------------------------------
// Suppress unused-import warning when Signature variants are not
// reached from this module.
// -------------------------------------------------------------------

#[allow(dead_code)]
fn _signature_type_anchor() -> Option<Signature> {
    None
}

#[allow(dead_code)]
fn _request_id_span_anchor() -> tracing::Span {
    request_id_span("anchor", RequestId::new())
}

#[allow(dead_code)]
fn _target_ref_anchor(_t: TargetRef) -> Option<Action> {
    None
}

// -------------------------------------------------------------------
// Tests
// -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_token_resolution_order_is_config_then_conventional_env() {
        let mut cfg = Config::from_layers(None, std::iter::empty::<(String, String)>())
            .expect("defaults parse");

        // 1. Explicit config wins over everything.
        cfg.transport.github_token = Some("from-config".into());
        let tok = resolve_github_token_from(&cfg, |k| Some(format!("env-{k}")));
        assert_eq!(tok.as_deref(), Some("from-config"));

        // 2. GITHUB_TOKEN before GH_TOKEN.
        cfg.transport.github_token = None;
        let tok = resolve_github_token_from(&cfg, |k| match k {
            "GITHUB_TOKEN" => Some("from-github-token".into()),
            "GH_TOKEN" => Some("from-gh-token".into()),
            _ => None,
        });
        assert_eq!(tok.as_deref(), Some("from-github-token"));

        // 3. GH_TOKEN as the last fallback; blank values don't count.
        let tok = resolve_github_token_from(&cfg, |k| match k {
            "GITHUB_TOKEN" => Some("   ".into()),
            "GH_TOKEN" => Some("from-gh-token".into()),
            _ => None,
        });
        assert_eq!(tok.as_deref(), Some("from-gh-token"));

        // 4. Nothing anywhere → None (live submission will error
        //    with the set-a-token guidance).
        let tok = resolve_github_token_from(&cfg, |_| None);
        assert!(tok.is_none());
    }

    #[test]
    fn mint_part_id_produces_valid_canonical() {
        let existing = HashSet::new();
        let id = mint_part_id(&existing).unwrap();
        assert_eq!(id.as_str().chars().count(), PART_ID_LEN);
        for c in id.as_str().chars() {
            assert!(PART_ID_ALPHABET.contains(c), "bad char {c} in {id}");
        }
    }

    #[test]
    fn mint_part_id_avoids_existing() {
        let mut existing = HashSet::new();
        for _ in 0..10 {
            let id = mint_part_id(&existing).unwrap();
            assert!(!existing.contains(id.as_str()));
            existing.insert(id.as_str().to_owned());
        }
        assert_eq!(existing.len(), 10);
    }

    #[test]
    fn normalize_id_strips_dashes_and_uppercases() {
        assert_eq!(normalize_id("k7m3-pq9r"), "K7M3PQ9R");
        assert_eq!(normalize_id("K7M3 PQ9R t5va xy"), "K7M3PQ9RT5VAXY");
    }

    #[test]
    fn tape_size_mm_resolves_known_presets() {
        assert_eq!(tape_size_mm("pt-12"), Some(9.0));
        assert_eq!(tape_size_mm("dk-62"), Some(56.0));
        assert_eq!(tape_size_mm("nope"), None);
    }
    #[test]
    fn minted_ids_always_fit_micro_m3_l() {
        // The mint M3-L fence (issue #211 interim): every minted id
        // must encode in Micro QR M3-L so labels are uniform 60px. ~3%
        // of raw nano14 ids don't fit and are re-rolled; this asserts
        // the fence holds across a batch.
        use std::collections::HashSet;
        let existing = HashSet::new();
        for _ in 0..200 {
            let id = mint_part_id(&existing).expect("mint succeeds");
            encode_pinned(id.as_str(), Family::Micro, 3, Ec::L)
                .expect("every minted id must encode in micro-m3-l (the fence)");
        }
    }

    #[test]
    fn dry_run_memory_sink_captures_proposals() {
        let (sink, store) = DryRunSink::in_memory();
        let proposal = Proposal {
            diff: Diff::default(),
            batch_label: None,
            author: dummy_operator(),
            signatures: Vec::new(),
            change_classification: Vec::new(),
            message: "test".into(),
            request_id: RequestId::new(),
        };
        let r = sink.submit(proposal).unwrap();
        assert_eq!(r.adapter, "dry_run");
        assert_eq!(store.lock().unwrap().len(), 1);
    }

    fn dummy_operator() -> Operator {
        Operator {
            id: qx_domain::OperatorId("test:user".into()),
            display_name: "Test".into(),
            source: qx_domain::IdentitySource::GitConfig,
            verified_at: None,
            claims: BTreeMap::new(),
            pubkey: None,
        }
    }
}
