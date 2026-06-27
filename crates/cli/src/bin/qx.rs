//! `qx` — the single qx command per ADR-030 §2: one native artifact for
//! the whole shell family.
//!
//! - `qx mint` / `qx label` / `qx bind` — parity delegates onto the
//!   same engine the legacy single-purpose binaries used (now folded in;
//!   the standalone bins are retired). Omitting `--dry-run` here uses
//!   the **live** GitHub PR sink (ADR-030 build-order step 2).
//! - `qx list|resolve|describe|count|export|print|whoami` — thin shells
//!   over `qx_app::dispatch` (the command protocol); output
//!   is the protocol's JSON `data` payload, pretty-printed.
//! - `qx check` — the ADR-016 gate: structural validation of a data
//!   repo plus, with `--base <git-ref>`, semantic-diff classification +
//!   policy per ADR-034 (tool classifies/advises; the host's branch
//!   protection + CODEOWNERS enforce).
//!
//! `serve` / `mcp` / `tui` land behind cargo features per ADR-030 §2.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::{Map, Value};

use clap::{Parser, Subcommand};

use qx_app::{dispatch, AppContext, Request, Response};
use qx_cli::{
    init_observability, render_bind_summary, render_mint_summary, run_bind, run_label, run_mint,
    BindArgs, DryRunTarget, LabelArgs, MintArgs, Wiring,
};
use qx_config::Config;
use qx_contract::{is_compatible, reshaped_collections, Contract};
use qx_domain::{
    Diff, DiffEdit, DiffRow, HeaderChange, IdentitySource, Operator, OperatorId, PartId,
    PartStatus, RequestId,
};
use qx_observability::{request_id_span, ObservabilityConfig};
use qx_validators::record::{
    validate_collection_graph, validate_record, validate_void_policy, RecordContext, Severity,
};
use qx_validators::{
    policy_decision, registry_sort_key, validate_sort_stable, validate_status_transition,
    validate_unique_ids, Policy,
};

#[derive(Parser)]
#[command(name = "qx", about = "qx — one binary, every shell (ADR-030)", version)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[allow(clippy::large_enum_variant)]
#[derive(Subcommand)]
enum Cmd {
    /// Mint N fresh part ids (live PR unless --dry-run).
    Mint(MintArgs),
    /// Render label SVGs for a selection.
    Label(LabelArgs),
    /// Bind / void a part (live PR unless --dry-run).
    Bind(BindArgs),
    /// List entities of a collection (protocol List).
    List {
        #[arg(long, default_value = "parts")]
        collection: String,
        #[arg(long)]
        status: Option<String>,
        /// Free-text filter over id + fields.
        #[arg(long)]
        text: Option<String>,
        /// Per-field substring filters, `key=value` (repeatable).
        #[arg(long = "field", value_parser = parse_key_val)]
        fields: Vec<(String, String)>,
        #[arg(long, default_value_t = 50)]
        limit: u32,
        #[arg(long, default_value_t = 0)]
        offset: u32,
        /// Write the pretty JSON data payload to a file instead of
        /// stdout (ADR-031 §10).
        #[arg(long, short = 'o')]
        output: Option<PathBuf>,
    },
    /// Resolve one id (full, prefix, or scheme:value).
    Resolve {
        id: String,
        /// Write the pretty JSON data payload to a file instead of
        /// stdout (ADR-031 §10).
        #[arg(long, short = 'o')]
        output: Option<PathBuf>,
    },
    /// Render the registry descriptors (what exists + how it's minted).
    Describe {
        #[arg(long)]
        collection: Option<String>,
        /// Write the pretty JSON data payload to a file instead of
        /// stdout (ADR-031 §10).
        #[arg(long, short = 'o')]
        output: Option<PathBuf>,
    },
    /// Group-by count over a collection field.
    Count {
        #[arg(long, default_value = "parts")]
        collection: String,
        #[arg(long)]
        by: String,
        #[arg(long)]
        status: Option<String>,
        /// Write the pretty JSON data payload to a file instead of
        /// stdout (ADR-031 §10).
        #[arg(long, short = 'o')]
        output: Option<PathBuf>,
    },
    /// Flat export (generated artifact — never commit it).
    Export {
        #[arg(long, default_value = "parts")]
        collection: String,
        #[arg(long, default_value = "csv")]
        format: String,
        /// Write to file instead of stdout.
        #[arg(long, short = 'o')]
        output: Option<PathBuf>,
    },
    /// Render labels via the protocol Print op; SVGs written to --out-dir.
    Print {
        /// Ids to print (repeatable).
        #[arg(long = "id", required = true)]
        ids: Vec<String>,
        #[arg(long, default_value = "horz")]
        layout: String,
        /// Label size, unit riding the value (ADR-031 §8): 64px
        /// (integer device px, selects the px-true renderer), 8mm, or
        /// bare 8 / 8.5 (= mm). Wins over the hidden --unit/--size-mm/
        /// --size-px aliases.
        #[arg(long, value_parser = parse_size_spec)]
        size: Option<SizeSpec>,
        /// Hidden alias retired by --size (`--size 8mm`).
        #[arg(long, default_value_t = 8.0, hide = true)]
        size_mm: f64,
        /// Human-id grouping: 44 | 444 | 554 | auto.
        #[arg(long, default_value = "auto")]
        chars: String,
        /// Deprecated alias for `--type micro`; when both are given,
        /// --type wins.
        #[arg(long)]
        micro: bool,
        /// Symbology type, canonical compact form (ADR-031 §8):
        /// <family>[-<version>][-<ec>], e.g. micro, micro-m3-l,
        /// qr-v1-m. Families: micro, qr. Version/EC auto-fit against
        /// the payload when unpinned.
        #[arg(long = "type", value_name = "FAMILY[-VERSION][-EC]")]
        symbology: Option<String>,
        #[arg(long)]
        cable_od: Option<f64>,
        #[arg(long, default_value_t = 1)]
        copies: u32,
        /// Skip the print-event audit append.
        #[arg(long)]
        no_log: bool,
        #[arg(long, default_value = "labels")]
        out_dir: PathBuf,
        /// Output format (ADR-031 §8): svg (default) | png | jpeg |
        /// pdf. png/jpeg/pdf rasterise in-core (no external
        /// rsvg-convert) and need the `raster` build feature
        /// (default-on). Files are written as `<id>.<ext>`.
        #[arg(long, default_value = "svg")]
        emit: String,
        /// Round-trip fence: rasterise each rendered label and decode
        /// its QR (rxing), refusing to finish if any QR does not scan
        /// back to its id. Confirms every printed code is machine-
        /// readable. Needs the `raster` + codec `decoder` features.
        #[arg(long)]
        verify: bool,
        /// Hidden alias retired by --size (ADR-031 §8: the unit rides
        /// the value): mm (default, the mm-native renderer) or px
        /// (the px-true device-pixel renderer).
        #[arg(long, default_value = "mm", hide = true)]
        unit: String,
        /// Hidden alias retired by --size (`--size 64px`): EXACT
        /// output canvas in device px.
        #[arg(long, hide = true)]
        size_px: Option<u32>,
        /// Minimum padding in device px, canvas edge -> module part
        /// (ADR-031 §4 floor consumed by the deduction; the
        /// controlling axis absorbs the remainder on top of the
        /// floors). CSS shorthand: 2 (all) | 2,6 (vertical,horizontal)
        /// | 2,6,4,6 (top,right,bottom,left).
        #[arg(long, value_parser = parse_padding_spec)]
        padding: Option<qx_app::PaddingSpec>,
        /// Quiet-zone accounting for the deduction (ADR-031 §8):
        /// overlap (quiet zone counts toward outside padding),
        /// additive (excluded; full-bleed/die-cut), or clip (no
        /// embedded quiet zone — the printer's intrinsic margins
        /// supply the safe space; maximal modules).
        #[arg(long, default_value = "overlap")]
        padding_mode: String,
        /// Dots per inch for the mm -> px conversion (default 300
        /// = Brother QL class).
        #[arg(long)]
        dpi: Option<f64>,
        /// ADR-031 §10 — flat-list payload DSL (stage 1):
        /// whitespace-separated `qr[:TYPE] | id[:GROUPING|chars-N]
        /// | space[:SIZE]` along the layout axis. Overrides
        /// --content; element params win over --chars/--type.
        #[arg(long)]
        payload: Option<String>,
        /// ADR-031 §10 sugar over --payload: qr+id (default),
        /// id+qr, qr, id.
        #[arg(long)]
        content: Option<String>,
        /// Foreground color (ADR-031 §10). Accepts #RGB / #RRGGBB
        /// / #RRGGBBAA, rgb(r,g,b), lowercase ascii names.
        #[arg(long)]
        fg: Option<String>,
        /// Background color (ADR-031 §10). Same forms as --fg,
        /// plus "none" (omits the background rect).
        #[arg(long)]
        bg: Option<String>,
        /// ADR-031 §8 size-mode: exact (default, the §2/§8 law)
        /// or snap (size_px is an UPPER BOUND; canvas snaps DOWN
        /// to the content lattice).
        #[arg(long, default_value = "exact")]
        size_mode: String,
        /// ADR-031 §10 id-text solver: how many id characters
        /// render (e.g. 8 or 14 for nano14). Combine with --rows
        /// or --id-size to derive the third.
        #[arg(long)]
        id_chars: Option<u32>,
        /// ADR-031 §10 id-text solver: how many rows the chars
        /// split across (balanced — 14/3 → 5,5,4).
        #[arg(long)]
        rows: Option<u32>,
        /// ADR-031 §10 id-text solver: glyph height in device px.
        /// Suffix grammar like --size: 28px / 8mm / bare 28 = px.
        #[arg(long, value_parser = parse_id_size_spec)]
        id_size_px: Option<u32>,
        /// ADR-031 §10 repeat: compose the rendered label into
        /// N copies. Accepts a whole number or "fill".
        #[arg(long)]
        repeat: Option<String>,
        /// `--repeat-axis along|across` (along = canvas flow,
        /// default; across = multi-up rows).
        #[arg(long)]
        repeat_axis: Option<String>,
        /// `--repeat-gap <N>[px|mm]` — explicit inter-copy gap.
        #[arg(long, value_parser = parse_id_size_spec)]
        repeat_gap: Option<u32>,
        /// `--repeat-orient same|alternate` (alternate rotates
        /// every second copy 180°).
        #[arg(long)]
        repeat_orient: Option<String>,
        /// `--length <N>[px|mm]` — required for `fill` and for
        /// derived gaps.
        #[arg(long, value_parser = parse_id_size_spec)]
        length: Option<u32>,
        /// `--spacing linear|cyclic` (linear = n-1 gaps,
        /// cyclic = n gaps).
        #[arg(long)]
        spacing: Option<String>,
        /// `--rotate 0|90|180|270` — whole-label rotation
        /// applied BEFORE repeating.
        #[arg(long)]
        rotate: Option<u32>,
        /// `--length-excess <N>[px|mm]` — BLANK leader/tail.
        #[arg(long, value_parser = parse_id_size_spec)]
        length_excess: Option<u32>,
        /// `--excess-at start|end` — which end carries the excess.
        #[arg(long)]
        excess_at: Option<String>,
    },
    /// Current operator identity.
    Whoami {
        /// Write the pretty JSON data payload to a file instead of
        /// stdout (ADR-031 §10).
        #[arg(long, short = 'o')]
        output: Option<PathBuf>,
    },
    /// Serve the command protocol over HTTP (+ the webapp bundle).
    #[cfg(feature = "serve")]
    Serve {
        /// Listen address.
        #[arg(long, default_value = "127.0.0.1:8470")]
        addr: std::net::SocketAddr,
        /// Serve a static webapp bundle (SPA fallback to index.html).
        #[arg(long)]
        static_dir: Option<PathBuf>,
    },
    /// List the operator-workspace registries (ADR-033 §5). Reads
    /// `registries.toml` from the XDG config dir (or --path) and prints
    /// each registry's name + locator, marking the default.
    Registries {
        /// Read the workspace from this file instead of the XDG default.
        #[arg(long)]
        path: Option<PathBuf>,
    },
    /// Append a stream checkpoint (ADR-037 §1) pinning the current
    /// `audit_log.jsonl` to `audit_checkpoints.jsonl`, restoring standalone
    /// stream verifiability without a base diff.
    Checkpoint {
        /// Path to the data-repo working tree.
        #[arg(long, default_value = ".")]
        path: PathBuf,
    },
    /// Verify a data-repo clone OFFLINE (ADR-037 §5): contract + record/FK
    /// validation, audit content-hash + checkpoint integrity, and persona
    /// accountability — all base-free (no PR diff, no network). `--anchors`
    /// adds anchor-ledger ancestry + immutability checks (reserved until
    /// the release ledger lands).
    Verify {
        /// Path to the cloned data repo to verify.
        #[arg(long, default_value = ".")]
        path: PathBuf,
        /// Also run the anchor-ledger ancestry + immutability checks.
        #[arg(long)]
        anchors: bool,
    },
    /// Stdio MCP server speaking the command protocol (for agents).
    #[cfg(feature = "mcp")]
    Mcp,
    /// Terminal UI — entity table + detail over the command protocol.
    #[cfg(feature = "tui")]
    Tui,
    /// ADR-016 gate over a data repo: structural validation (+ diff
    /// classification and policy with --base).
    Check {
        /// Path to the data-repo working tree.
        #[arg(long, default_value = ".")]
        path: PathBuf,
        /// Git ref to diff against (e.g. origin/main). Enables the
        /// semantic-diff classification + policy advisory.
        #[arg(long)]
        base: Option<String>,
        /// Merge-approver github logins (comma-separated), supplied by CI
        /// from the host's PR review data (ADR-036 §2). Each must resolve
        /// to an active persona when a `personas` collection is declared.
        #[arg(long, value_delimiter = ',')]
        approver: Vec<String>,
    },
    /// Scaffold a fresh company data repo: the canonical contract
    /// (parts + companies + contacts), empty collections, and the CI
    /// gate workflow (ADR-039). The starting point for a deployment.
    Init {
        /// Directory to scaffold into (created if missing).
        #[arg(long, default_value = ".")]
        path: PathBuf,
        /// Overwrite an existing contract/collections if present.
        #[arg(long)]
        force: bool,
    },
    /// Promote a hot open-properties key into a declared, typed tier-2
    /// field (ADR-035 §3): edits `.qx/contract.json` so the key is no
    /// longer tier-3 open data but a validated field. Commit the change
    /// to land it (the gate validates the result).
    Promote {
        /// Collection whose contract to edit.
        collection: String,
        /// The open-property key to promote.
        key: String,
        /// Field type: string | integer | number | decimal | date |
        /// timestamp | bool (reference/enum/attachment/object need their
        /// target/values declared by hand).
        #[arg(long, default_value = "string")]
        field_type: String,
        /// Display label (defaults to the key).
        #[arg(long)]
        label: Option<String>,
        /// Path to the data-repo working tree.
        #[arg(long, default_value = ".")]
        path: PathBuf,
    },
}

fn parse_key_val(s: &str) -> Result<(String, String), String> {
    s.split_once('=')
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .ok_or_else(|| format!("expected key=value, got {s:?}"))
}

/// `--size <N>[px|mm]` — the unit rides the value (ADR-031 §8). CLI
/// sugar only: it expands into the protocol's explicit
/// `{unit, size_px|size_mm}` fields, so the wire stays explicit.
#[derive(Clone, Copy, Debug, PartialEq)]
enum SizeSpec {
    /// `64px` — exact output canvas in integer device px.
    Px(u32),
    /// `8mm` or bare `8` / `8.5` — physical mm (bare preserves the
    /// pre-suffix default unit).
    Mm(f64),
}

fn parse_size_spec(s: &str) -> Result<SizeSpec, String> {
    let t = s.trim();
    if let Some(px) = t.strip_suffix("px") {
        return px
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|n| *n > 0)
            .map(SizeSpec::Px)
            .ok_or_else(|| {
                format!("size {t:?}: px sizes are whole positive device pixels (e.g. 64px)")
            });
    }
    let mm = t.strip_suffix("mm").unwrap_or(t);
    mm.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v > 0.0)
        .map(SizeSpec::Mm)
        .ok_or_else(|| format!("size {t:?}: expected <N>[px|mm], e.g. 64px, 8mm, 8.5 (bare = mm)"))
}

/// `--padding 2 | 2,6 | 2,6,4,6` — the protocol's one CSS-shorthand
/// expansion rule, exposed as a clap value parser.
fn parse_padding_spec(s: &str) -> Result<qx_app::PaddingSpec, String> {
    s.parse()
}

/// `--id-size <N>[px|mm]` — same suffix grammar as `--size`. The
/// protocol field is integer device px; mm rides as the bare-value
/// alternative for ergonomics (stage 1 keeps mm rounded to whole px).
/// ADR-031 §10 sugar mapping: `--content qr+id` → `--payload "qr id"`,
/// `id+qr` → `"id qr"`, `qr` → `"qr"`, `id` → `"id"`. Anything else
/// is left to the engine, which validates the payload form.
fn content_to_payload(content: Option<&str>) -> Option<String> {
    content.map(|c| match c {
        "qr+id" => "qr id".into(),
        "id+qr" => "id qr".into(),
        "qr" => "qr".into(),
        "id" => "id".into(),
        // Pass-through so the engine surfaces the actual grammar
        // error (the staged `[` rejection rides through).
        other => other.into(),
    })
}

fn parse_id_size_spec(s: &str) -> Result<u32, String> {
    let t = s.trim();
    if let Some(px) = t.strip_suffix("px") {
        return px
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(|| format!("--id-size {t:?}: px sizes are whole positive device pixels"));
    }
    // mm is accepted but resolved here; the dpi/mm round-trip is
    // documented stage 2 (printer profiles); stage 1 reads bare and
    // `mm` as the same px count via the default 300dpi assumption.
    let stripped = t.strip_suffix("mm").unwrap_or(t);
    stripped
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v > 0.0)
        .map(|mm| (mm / 25.4 * 300.0).round() as u32)
        .ok_or_else(|| format!("--id-size {t:?}: expected <N>[px|mm]"))
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Mint(args) => parity_mint(args),
        Cmd::Label(args) => parity_label(args),
        Cmd::Bind(args) => parity_bind(args),
        Cmd::Check {
            path,
            base,
            approver,
        } => check(&path, base.as_deref(), &approver),
        Cmd::Init { path, force } => init_repo(&path, force),
        Cmd::Promote {
            collection,
            key,
            field_type,
            label,
            path,
        } => promote_cmd(&path, &collection, &key, &field_type, label.as_deref()),
        #[cfg(feature = "serve")]
        Cmd::Serve { addr, static_dir } => serve_cmd(addr, static_dir),
        Cmd::Registries { path } => registries_cmd(path),
        Cmd::Checkpoint { path } => checkpoint_cmd(&path),
        Cmd::Verify { path, anchors } => verify_cmd(&path, anchors),
        #[cfg(feature = "mcp")]
        Cmd::Mcp => mcp_cmd(),
        #[cfg(feature = "tui")]
        Cmd::Tui => tui_cmd(),
        protocol => protocol_cmd(protocol),
    }
}

#[cfg(feature = "tui")]
fn tui_cmd() -> ExitCode {
    let cfg = match load_config() {
        Ok(c) => c,
        Err(e) => return e,
    };
    // The TUI is read-only today (table + detail); a dry-run sink keeps
    // it token-free.
    let wiring = match build_wiring(&cfg, Some(DryRunTarget::Stdout)) {
        Ok(w) => w,
        Err(e) => return e,
    };
    let ctx = app_context(&cfg, wiring);
    match qx_cli::tui::run(ctx) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("tui failed: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(feature = "mcp")]
fn mcp_cmd() -> ExitCode {
    let cfg = match load_config() {
        Ok(c) => c,
        Err(e) => return e,
    };
    // Same sink policy as serve: live when a token resolves, dry-run
    // capture otherwise (stderr keeps stdout clean for the MCP wire).
    let wiring = match build_wiring(&cfg, None) {
        Ok(w) => w,
        Err(_) => {
            eprintln!(
                "qx mcp: no GitHub token resolved — mutations will be captured as \
                 dry-run JSON, not submitted (set PART_REGISTRY__TRANSPORT__GITHUB_TOKEN)."
            );
            // Stdout carries the MCP wire; dry-run capture must go to a
            // file, never stdout.
            let capture = std::env::temp_dir().join("qx-mcp-dry-run.jsonl");
            eprintln!("qx mcp: dry-run proposals -> {}", capture.display());
            match build_wiring(&cfg, Some(DryRunTarget::File(capture))) {
                Ok(w) => w,
                Err(e) => return e,
            }
        }
    };
    init_obs(&cfg, &wiring);
    let ctx = app_context(&cfg, wiring);
    match qx_cli::mcp::run(ctx) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mcp failed: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(feature = "serve")]
fn serve_cmd(addr: std::net::SocketAddr, static_dir: Option<PathBuf>) -> ExitCode {
    let cfg = match load_config() {
        Ok(c) => c,
        Err(e) => return e,
    };
    // Prefer the live sink (the server is a write-capable host); fall
    // back to dry-run capture with a loud notice when no token is
    // resolvable, so read-only serving still works.
    let wiring = match build_wiring(&cfg, None) {
        Ok(w) => w,
        Err(_) => {
            eprintln!(
                "qx serve: no GitHub token resolved — mutations will be captured \
                 as dry-run JSON on the server's stdout, not submitted. Set \
                 PART_REGISTRY__TRANSPORT__GITHUB_TOKEN (or GITHUB_TOKEN) for \
                 live proposals."
            );
            match build_wiring(&cfg, Some(DryRunTarget::Stdout)) {
                Ok(w) => w,
                Err(e) => return e,
            }
        }
    };
    init_obs(&cfg, &wiring);
    let span = request_id_span("qx.serve", RequestId::new());
    let _g = span.enter();
    let ctx = app_context(&cfg, wiring);
    match qx_cli::serve::run(ctx, addr, static_dir) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("serve failed: {e}");
            ExitCode::FAILURE
        }
    }
}

// -------------------------------------------------------------------
// Shared wiring
// -------------------------------------------------------------------

fn load_config() -> Result<Config, ExitCode> {
    Config::from_env().map_err(|e| {
        eprintln!("config error: {e}");
        ExitCode::from(2)
    })
}

fn build_wiring(cfg: &Config, dry_run: Option<DryRunTarget>) -> Result<Wiring, ExitCode> {
    Wiring::from_config(cfg, dry_run).map_err(|e| {
        eprintln!("wiring error: {e}");
        ExitCode::from(2)
    })
}

fn init_obs(cfg: &Config, wiring: &Wiring) {
    let obs_cfg = ObservabilityConfig {
        log_level: cfg.observability.log_level.clone(),
        stdout_json: cfg.observability.stdout_json,
        stderr_human: cfg.observability.stderr_human,
        audit_csv: cfg.observability.audit_csv,
        audit_log_path: cfg.observability.audit_log_path.clone(),
    };
    let _ = init_observability(&obs_cfg, wiring.repo.clone());
}

fn dry_run_target(dry_run: bool, dry_run_file: &Option<PathBuf>) -> Option<DryRunTarget> {
    if let Some(path) = dry_run_file {
        Some(DryRunTarget::File(path.clone()))
    } else if dry_run {
        Some(DryRunTarget::Stdout)
    } else {
        None // live sink (step 2)
    }
}

fn app_context(cfg: &Config, wiring: Wiring) -> AppContext {
    let registry_name = qx_config::parse_owner_repo(&cfg.repo.data_repo_url)
        .map(|(o, r)| format!("{o}/{r}"))
        .unwrap_or_else(|_| cfg.repo.data_repo_url.clone());
    // Load the registry's contract (`.qx/contract.json`) so the engine can
    // self-describe from its declared collections (ADR-035). Absent or
    // unparseable → None, and the engine falls back to the code presets.
    let contract = cfg
        .resolve_data_path()
        .ok()
        .map(|p| p.join(".qx/contract.json"))
        .filter(|p| p.exists())
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| qx_contract::Contract::from_bytes(&b).ok())
        .map(std::sync::Arc::new);
    AppContext {
        repo: wiring.repo,
        identity: wiring.identity,
        sink: wiring.sink,
        registry_name,
        contract,
    }
}

// -------------------------------------------------------------------
// Parity delegates
// -------------------------------------------------------------------

fn parity_mint(args: MintArgs) -> ExitCode {
    let cfg = match load_config() {
        Ok(c) => c,
        Err(e) => return e,
    };
    // Dev-only `--local` sink: skip the token-requiring live path by
    // wiring a throwaway in-memory dry-run target, then swap in the
    // LocalRegistrySink below. `args.local` is forced false unless the
    // `dev-local` feature exposes the flag (clap `arg(skip)` otherwise).
    let local = args.local;
    let dry = if local {
        Some(DryRunTarget::Memory(std::sync::Arc::new(
            std::sync::Mutex::new(Vec::new()),
        )))
    } else {
        dry_run_target(args.dry_run, &args.dry_run_file)
    };
    #[allow(unused_mut)]
    let mut wiring = match build_wiring(&cfg, dry) {
        Ok(w) => w,
        Err(e) => return e,
    };
    #[cfg(feature = "dev-local")]
    if local {
        eprintln!(
            "qx mint --local: DEV BUILD — applying straight to local registry.csv, \
             NOT opening a PR"
        );
        wiring.sink = Box::new(qx_cli::LocalRegistrySink::new(wiring.repo_root.clone()));
    }
    init_obs(&cfg, &wiring);
    let span = request_id_span("qx.mint", RequestId::new());
    let _g = span.enter();
    match run_mint(&args, &wiring) {
        Ok(outcome) => {
            print!("{}", render_mint_summary(&outcome, &wiring.repo_root));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("mint failed: {e}");
            ExitCode::FAILURE
        }
    }
}

fn parity_label(args: LabelArgs) -> ExitCode {
    let cfg = match load_config() {
        Ok(c) => c,
        Err(e) => return e,
    };
    // Label renders + appends print events; the proposal sink is
    // unused, so a stdout dry-run target avoids requiring a token.
    let wiring = match build_wiring(&cfg, Some(DryRunTarget::Stdout)) {
        Ok(w) => w,
        Err(e) => return e,
    };
    init_obs(&cfg, &wiring);
    let span = request_id_span("qx.label", RequestId::new());
    let _g = span.enter();
    match run_label(&args, &wiring) {
        Ok(outcome) => {
            println!(
                "rendered {} label(s) -> {}",
                outcome.rendered.len(),
                outcome.out_dir.display()
            );
            if let Some(w) = outcome.warning {
                eprintln!("warning: {w}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("label failed: {e}");
            ExitCode::FAILURE
        }
    }
}

fn parity_bind(args: BindArgs) -> ExitCode {
    let cfg = match load_config() {
        Ok(c) => c,
        Err(e) => return e,
    };
    let dry = dry_run_target(args.dry_run, &args.dry_run_file);
    let wiring = match build_wiring(&cfg, dry) {
        Ok(w) => w,
        Err(e) => return e,
    };
    init_obs(&cfg, &wiring);
    let span = request_id_span("qx.bind", RequestId::new());
    let _g = span.enter();
    match run_bind(&args, &wiring) {
        Ok(outcome) => {
            print!("{}", render_bind_summary(&outcome));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("bind failed: {e}");
            ExitCode::FAILURE
        }
    }
}

// -------------------------------------------------------------------
// Protocol subcommands — thin shells over app::dispatch
// -------------------------------------------------------------------

fn protocol_cmd(cmd: Cmd) -> ExitCode {
    let cfg = match load_config() {
        Ok(c) => c,
        Err(e) => return e,
    };
    // Read-side + print ops never submit proposals; dry-run sink keeps
    // them token-free.
    let wiring = match build_wiring(&cfg, Some(DryRunTarget::Stdout)) {
        Ok(w) => w,
        Err(e) => return e,
    };
    init_obs(&cfg, &wiring);
    let span = request_id_span("qx.dispatch", RequestId::new());
    let _g = span.enter();
    let ctx = app_context(&cfg, wiring);

    // Print needs post-processing (write SVGs to disk); everything
    // else prints the protocol payload.
    if let Cmd::Print {
        ids,
        layout,
        size,
        size_mm,
        chars,
        micro,
        symbology,
        cable_od,
        copies,
        no_log,
        out_dir,
        emit,
        verify,
        unit,
        size_px,
        padding,
        padding_mode,
        dpi,
        payload,
        content,
        fg,
        bg,
        size_mode,
        id_chars,
        rows,
        id_size_px,
        repeat,
        repeat_axis,
        repeat_gap,
        repeat_orient,
        length,
        spacing,
        rotate,
        length_excess,
        excess_at,
    } = cmd
    {
        // ADR-031 §10 sugar: --content qr+id|id+qr|qr|id expands
        // into --payload "qr id" etc. --payload wins if both set
        // (the explicit form).
        let payload = payload.or_else(|| content_to_payload(content.as_deref()));
        // --size wins over the hidden --unit/--size-mm/--size-px
        // aliases; the suffix expands into the explicit wire fields.
        let (unit, size_mm, size_px) = match size {
            Some(SizeSpec::Px(n)) => ("px".into(), size_mm, Some(n)),
            Some(SizeSpec::Mm(v)) => (unit, v, None),
            None => (unit, size_mm, size_px),
        };
        let options = qx_app::PrintOptions {
            layout,
            size_mm,
            chars,
            micro,
            symbology,
            cable_od_mm: cable_od,
            copies,
            log: !no_log,
            unit,
            size_px,
            padding_px: padding,
            padding_mode: Some(padding_mode),
            dpi,
            payload,
            fg,
            bg,
            size_mode: Some(size_mode),
            id_chars,
            rows,
            id_size_px,
            repeat,
            repeat_axis,
            repeat_gap_px: repeat_gap,
            repeat_orient,
            length_px: length,
            spacing,
            rotate,
            length_excess_px: length_excess,
            excess_at,
        };
        let emit = match qx_cli::raster::Emit::parse(&emit) {
            Ok(e) => e,
            Err(msg) => {
                eprintln!("{msg}");
                return ExitCode::from(2);
            }
        };
        return protocol_print(&ctx, ids, options, &out_dir, emit, verify);
    }

    let (req, output) = match cmd {
        Cmd::List {
            collection,
            status,
            text,
            fields,
            limit,
            offset,
            output,
        } => (
            Request::List {
                collection,
                filter: qx_app::Filter {
                    status,
                    kind: None,
                    text,
                    fields: fields.into_iter().collect(),
                },
                sort: Vec::new(),
                page: qx_app::Page { offset, limit },
            },
            output,
        ),
        Cmd::Resolve { id, output } => (Request::Resolve { id }, output),
        Cmd::Describe { collection, output } => (Request::Describe { collection }, output),
        Cmd::Count {
            collection,
            by,
            status,
            output,
        } => (
            Request::Count {
                collection,
                filter: qx_app::Filter {
                    status,
                    ..Default::default()
                },
                by,
            },
            output,
        ),
        Cmd::Export {
            collection,
            format,
            output,
        } => {
            return protocol_export(&ctx, collection, format, output);
        }
        Cmd::Whoami { output } => (Request::Whoami, output),
        // Parity + Check arms are handled in main; Print above.
        _ => {
            eprintln!("internal: non-protocol command reached protocol_cmd");
            return ExitCode::from(2);
        }
    };
    emit(dispatch(&ctx, req), output.as_deref())
}

fn emit(resp: Response, output: Option<&Path>) -> ExitCode {
    match resp {
        Response::Ok { data, .. } => {
            let s = serde_json::to_string_pretty(&data).unwrap_or_else(|_| data.to_string());
            match output {
                Some(path) => {
                    if let Err(e) = std::fs::write(path, format!("{s}\n")) {
                        eprintln!("write {}: {e}", path.display());
                        return ExitCode::FAILURE;
                    }
                }
                None => {
                    println!("{s}");
                }
            }
            ExitCode::SUCCESS
        }
        Response::Err { error, .. } => {
            eprintln!("{:?}: {}", error.kind, error.message);
            ExitCode::FAILURE
        }
    }
}

fn protocol_export(
    ctx: &AppContext,
    collection: String,
    format: String,
    output: Option<PathBuf>,
) -> ExitCode {
    let resp = dispatch(ctx, Request::Export { collection, format });
    match resp {
        Response::Ok { data, .. } => {
            let content = data["content"].as_str().unwrap_or_default();
            match output {
                Some(path) => {
                    if let Err(e) = std::fs::write(&path, content) {
                        eprintln!("write {}: {e}", path.display());
                        return ExitCode::FAILURE;
                    }
                    println!("exported {} rows -> {}", data["rows"], path.display());
                }
                None => print!("{content}"),
            }
            ExitCode::SUCCESS
        }
        Response::Err { error, .. } => {
            eprintln!("{:?}: {}", error.kind, error.message);
            ExitCode::FAILURE
        }
    }
}

fn protocol_print(
    ctx: &AppContext,
    ids: Vec<String>,
    options: qx_app::PrintOptions,
    out_dir: &Path,
    emit: qx_cli::raster::Emit,
    verify: bool,
) -> ExitCode {
    let resp = dispatch(
        ctx,
        Request::Print {
            collection: "parts".into(),
            selection: qx_app::Selection::Ids(ids),
            options,
        },
    );
    match resp {
        Response::Ok { data, .. } => {
            if let Err(e) = std::fs::create_dir_all(out_dir) {
                eprintln!("create {}: {e}", out_dir.display());
                return ExitCode::FAILURE;
            }
            let labels = data["labels"].as_array().cloned().unwrap_or_default();
            let ext = emit.ext();
            for l in &labels {
                let id = l["id"].as_str().unwrap_or("label");
                let svg = l["svg"].as_str().unwrap_or_default();
                // Round-trip fence (--verify): rasterise to PNG and
                // decode the QR, refusing if it does not scan back to
                // the id. Independent of --emit (always uses a PNG for
                // the decoder).
                if verify {
                    let png = match qx_cli::raster::render(svg, qx_cli::raster::Emit::Png) {
                        Ok(b) => b,
                        Err(msg) => {
                            eprintln!("verify {id}: rasterise failed: {msg}");
                            return ExitCode::FAILURE;
                        }
                    };
                    match qx_codec::decode_qr(&png) {
                        Ok(decoded) if decoded == id => {}
                        Ok(decoded) => {
                            eprintln!(
                                "verify {id}: QR decoded to {decoded:?}, not the id — refusing"
                            );
                            return ExitCode::FAILURE;
                        }
                        Err(e) => {
                            eprintln!("verify {id}: QR did not decode ({e}) — refusing");
                            return ExitCode::FAILURE;
                        }
                    }
                }
                // The engine always renders SVG; --emit rasterises it
                // in-core (ADR-031 §8) for png/jpeg/pdf, pass-through
                // for svg.
                let bytes = match qx_cli::raster::render(svg, emit) {
                    Ok(b) => b,
                    Err(msg) => {
                        eprintln!("emit {id}.{ext}: {msg}");
                        return ExitCode::FAILURE;
                    }
                };
                let path = out_dir.join(format!("{id}.{ext}"));
                if let Err(e) = std::fs::write(&path, bytes) {
                    eprintln!("write {}: {e}", path.display());
                    return ExitCode::FAILURE;
                }
            }
            println!(
                "rendered {} {ext} label(s) -> {}",
                labels.len(),
                out_dir.display()
            );
            if let Some(w) = data["warning"].as_str() {
                eprintln!("warning: {w}");
            }
            ExitCode::SUCCESS
        }
        Response::Err { error, .. } => {
            eprintln!("{:?}: {}", error.kind, error.message);
            ExitCode::FAILURE
        }
    }
}

// -------------------------------------------------------------------
// qx check — the ADR-016 gate (ADR-034: classify + advise; the host
// enforces)
// -------------------------------------------------------------------

fn check(path: &Path, base: Option<&str>, approvers: &[String]) -> ExitCode {
    let mut failures: Vec<String> = Vec::new();
    let mut notices: Vec<String> = Vec::new();
    let mut ran_something = false;

    // --- Legacy CSV path (ADR-013/016) — runs only when registry.csv is
    //     present, so a canonical JSONL-only repo no longer hard-fails.
    let registry_path = path.join("registry.csv");
    let mut csv_rows = 0usize;
    if registry_path.exists() {
        ran_something = true;
        match read_csv_rows(&registry_path) {
            Ok(head) => {
                csv_rows = head.rows.len();
                let head_parts = rows_to_parts(&head.rows, &mut failures);
                if let Err(e) = validate_unique_ids(&head_parts) {
                    failures.push(format!("unique-ids: {e}"));
                }
                if let Err(e) = validate_sort_stable(&head_parts, registry_sort_key) {
                    failures.push(format!("sort-stability: {e}"));
                }
                if let Some(base_ref) = base {
                    match git_show(path, base_ref, "registry.csv") {
                        Ok(base_text) => match parse_csv_text(&base_text) {
                            Ok(base_csv) => {
                                let diff = build_diff(&base_csv, &head);
                                check_transitions(&diff, &mut failures);
                                advise_policy(&diff, &mut failures, &mut notices);
                            }
                            Err(e) => failures.push(format!("parse base registry.csv: {e}")),
                        },
                        Err(e) => failures.push(format!("git show {base_ref}:registry.csv: {e}")),
                    }
                }
            }
            Err(e) => failures.push(format!("read {}: {e}", registry_path.display())),
        }
    }

    // --- Contract-driven path (ADR-039) — runs when a contract is present.
    let contract_records = if path.join(".qx/contract.json").exists() {
        ran_something = true;
        check_contract(path, base, &mut failures, &mut notices)
    } else {
        0
    };

    // --- Audit log append-only (ADR-037 §1): with a base, the head's
    //     audit_log.jsonl must be the base content plus trailing lines.
    if let Some(base_ref) = base {
        let head = std::fs::read_to_string(path.join("audit_log.jsonl")).unwrap_or_default();
        // The file may not exist at the base yet (first audit) — empty base.
        let base_text = git_show(path, base_ref, "audit_log.jsonl").unwrap_or_default();
        if let Some(violation) = audit_append_only_violation(&base_text, &head) {
            failures.push(format!("audit_log.jsonl: {violation}"));
        }
    }

    // --- Audit integrity (ADR-037 §1): per-entry content_hash + stream
    //     checkpoints. Each entry that carries a content_hash must still
    //     verify (in-line tamper evidence), and every checkpoint must pin a
    //     prefix that still digests correctly. Runs always (not just with a
    //     base) — these are standalone, base-free integrity checks.
    audit_integrity_check(path, &mut failures);

    // --- Personas accountability (ADR-036 §1/§2): when a registry declares
    //     a `personas` collection, the audit operator FK + CODEOWNERS
    //     principals must resolve to (active) personas. Skipped silently
    //     when no personas collection is present.
    personas_cross_check(path, approvers, &mut failures);

    // --- Manifest↔contract FK (ADR-034 §3 / capability-grain): when a
    //     `.qx/manifest.toml` is present, every collection it names (in
    //     [ops] or a role map) must be a contract-declared collection.
    manifest_cross_check(path, &mut failures);

    // --- Exports never committed (ADR-035): a `*.csv` beside the JSONL
    //     collections is a committed export. CSV is an export VIEW —
    //     generated on demand (Export op / Pages build), never stored.
    if let Ok(entries) = std::fs::read_dir(path.join("collections")) {
        for e in entries.flatten() {
            if e.path().extension().and_then(|x| x.to_str()) == Some("csv") {
                failures.push(format!(
                    "collections/{}: committed CSV export — generate via the Export op, never commit it beside the JSONL source",
                    e.file_name().to_string_lossy()
                ));
            }
        }
    }

    if !ran_something {
        eprintln!(
            "qx check: nothing to check — neither registry.csv nor \
             .qx/contract.json found in {}",
            path.display()
        );
        return ExitCode::FAILURE;
    }

    for n in &notices {
        println!("notice: {n}");
    }
    if failures.is_empty() {
        println!(
            "qx check: OK ({csv_rows} csv row(s), {contract_records} contract record(s){})",
            if base.is_some() {
                ", diff classified"
            } else {
                ", structural only"
            }
        );
        ExitCode::SUCCESS
    } else {
        for f in &failures {
            eprintln!("FAIL: {f}");
        }
        eprintln!("qx check: {} failure(s)", failures.len());
        ExitCode::FAILURE
    }
}

/// Contract-driven validation (ADR-039). Loads the working-tree contract,
/// then validates each collection's `collections/<name>.jsonl` records
/// against it through the SSOT record validator. With `base`, only ADDED
/// or CHANGED records are validated — commit-resolved effective-dating:
/// untouched merged records were qualified under their contemporaneous
/// contract and are not re-litigated (ADR-039 §6). Returns the count of
/// records validated; errors go to `failures`, warnings to `notices`.
fn check_contract(
    path: &Path,
    base: Option<&str>,
    failures: &mut Vec<String>,
    notices: &mut Vec<String>,
) -> usize {
    // 1. Load + parse + structurally validate the HEAD contract.
    let contract_path = path.join(".qx/contract.json");
    let bytes = match std::fs::read(&contract_path) {
        Ok(b) => b,
        Err(e) => {
            failures.push(format!("read {}: {e}", contract_path.display()));
            return 0;
        }
    };
    let contract = match Contract::from_bytes(&bytes) {
        Ok(c) => c,
        Err(e) => {
            failures.push(format!("contract invalid: {e}"));
            return 0;
        }
    };
    if !is_compatible(&contract) {
        failures.push(format!(
            "contract format_version {} is outside this tool's supported range",
            contract.format_version
        ));
        return 0;
    }

    // 1b. The regulated `parts` floor must not be weakened (ADR-035 §0 /
    //     ADR-040): a registry contract may extend the preset, never drop a
    //     floor field, loosen its presence gate, or shrink the lifecycle.
    if let Err(violations) = qx_app::preset::assert_parts_floor(&contract) {
        for v in violations {
            failures.push(format!("parts floor violated: {v}"));
        }
    }

    // 1c. Contract identity is its content hash (ADR-039 §6) — DERIVED,
    //     never stored in-file (the only in-file version is format_version).
    //     Surface it so the gate run records which contract governed this
    //     validation (effective-dating: records are governed by the
    //     contract content at their commit).
    notices.push(format!("contract identity: {}", contract_identity(&bytes)));

    // 2. Read every collection's HEAD records + build the cross-collection
    //    id universe for reference FK checks.
    let mut head_records: BTreeMap<String, Vec<Map<String, Value>>> = BTreeMap::new();
    let mut universe: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for coll in &contract.collections {
        let file = format!("collections/{}.jsonl", coll.name);
        let recs = match read_jsonl_records(&path.join(&file)) {
            Ok(r) => r,
            Err(e) => {
                failures.push(format!("read {file}: {e}"));
                continue;
            }
        };
        let ids: BTreeSet<String> = recs
            .iter()
            .filter_map(|r| r.get("id").and_then(Value::as_str).map(str::to_owned))
            .collect();
        universe.insert(coll.name.clone(), ids);
        head_records.insert(coll.name.clone(), recs);
    }
    // The kind tree (ADR-035 §5): kinds live as records in the `types`
    // collection; resolve their inherited field schemas for per-kind
    // validation dispatch.
    let kind_schemas = resolve_kind_schemas(
        &read_jsonl_records(&path.join("collections/types.jsonl")).unwrap_or_default(),
    );
    let ctx = RecordContext::new(universe).with_kind_schemas(kind_schemas);

    // 3. Effective-dating filter: with a base, the set of new/changed ids
    //    per collection (untouched records are skipped).
    let changed: Option<BTreeMap<String, BTreeSet<String>>> =
        base.map(|b| changed_record_ids(path, b, &contract));

    // 3b. Contract-shape change (ADR-039 §6): a contract tightening can
    //     invalidate an UNTOUCHED record without that record appearing in
    //     the diff. So when the contract itself changed, re-validate every
    //     record of any collection whose descriptor changed — the
    //     effective-dating skip must not hide it. Blunt M-A.1 rule: any
    //     descriptor change re-validates that collection (never misses a
    //     tightening; on a pure widening, records simply still pass).
    let reshaped: BTreeSet<String> = match base {
        Some(b) => git_show(path, b, ".qx/contract.json")
            .ok()
            .and_then(|t| Contract::from_bytes(t.as_bytes()).ok())
            .map(|base_contract| reshaped_collections(&base_contract, &contract))
            .unwrap_or_default(),
        None => BTreeSet::new(),
    };

    // 4. Validate the in-scope records against their collection descriptor.
    // The audit spine indexed once for the lifecycle-timestamps cross-check.
    let audit_ts = audit_timestamp_index(path);
    let mut validated = 0usize;
    for coll in &contract.collections {
        let Some(recs) = head_records.get(&coll.name) else {
            continue;
        };
        let changed_in = changed.as_ref().map(|m| m.get(&coll.name));
        let coll_reshaped = reshaped.contains(&coll.name);
        for rec in recs {
            let id = rec.get("id").and_then(Value::as_str).unwrap_or("<no-id>");
            if let Some(set_opt) = changed_in {
                let touched = set_opt.map(|s| s.contains(id)).unwrap_or(false);
                if !touched && !coll_reshaped {
                    continue; // unchanged record AND unchanged descriptor — already qualified
                }
            }
            let status = rec.get("status").and_then(Value::as_str);
            for issue in validate_record(coll, rec, status, &ctx) {
                let line = format!("{}[{id}].{}: {}", coll.name, issue.path, issue.message);
                match issue.severity {
                    Severity::Error => failures.push(line),
                    Severity::Warn => notices.push(line),
                }
            }
            // Timestamp skew sanity-check (ADR-035 §1b timestamp-trust):
            // a stamp implausibly far in the future is clock skew or
            // tampering — the CI plausibility backstop.
            for issue in timestamp_skew_issues(rec, time::OffsetDateTime::now_utc()) {
                failures.push(format!("{}[{id}]: {issue}", coll.name));
            }
            // Lifecycle-timestamps cross-check: each stamp must be a cache
            // of an audit-spine entry's timestamp for this id (ADR-035 §1b).
            for issue in stamp_provenance_issues(rec, &audit_ts) {
                failures.push(format!("{}[{id}]: {issue}", coll.name));
            }
            validated += 1;
        }
        // Cross-record graph integrity for `acyclic` relations (ADR-035
        // §1a): a cycle is invalid regardless of which records the PR
        // touched, so it runs over all current records of the collection.
        for issue in validate_collection_graph(coll, recs) {
            failures.push(format!("{}.{}: {}", coll.name, issue.path, issue.message));
        }
    }

    // Cross-collection void-policy (ADR-035 §1a): a voided record still
    // referenced through a `block`/`warn` relation is an error/notice.
    for issue in validate_void_policy(&contract, &head_records) {
        let line = format!("{}: {}", issue.path, issue.message);
        match issue.severity {
            Severity::Error => failures.push(line),
            Severity::Warn => notices.push(line),
        }
    }

    // Content-addressed attachment integrity (ADR-035 §4): every
    // attachment blob exists and hashes to its declared ref.
    for line in check_attachment_blobs(&contract, &head_records, path) {
        failures.push(line);
    }
    validated
}

/// Verify content-addressed attachment blobs (ADR-035 §4): every
/// attachment value's blob must exist at `attachments/<hex>.<ext>` and
/// sha256 to its declared `ref` (tamper-evident). The object SHAPE is
/// already enforced by `validate_record`; this checks the bytes on disk.
fn check_attachment_blobs(
    contract: &Contract,
    head_records: &BTreeMap<String, Vec<Map<String, Value>>>,
    path: &Path,
) -> Vec<String> {
    use sha2::{Digest, Sha256};
    let mut errs = Vec::new();
    for coll in &contract.collections {
        let attach_keys: Vec<&str> = coll
            .fields
            .iter()
            .filter(|f| matches!(f.type_, qx_contract::FieldType::Attachment))
            .map(|f| f.key.as_str())
            .collect();
        if attach_keys.is_empty() {
            continue;
        }
        let Some(recs) = head_records.get(&coll.name) else {
            continue;
        };
        for rec in recs {
            let id = rec.get("id").and_then(Value::as_str).unwrap_or("?");
            for key in &attach_keys {
                let Some(obj) = rec.get(*key).and_then(Value::as_object) else {
                    continue;
                };
                let (Some(r), Some(name)) = (
                    obj.get("ref").and_then(Value::as_str),
                    obj.get("name").and_then(Value::as_str),
                ) else {
                    continue;
                };
                let Some(hex) = r.strip_prefix("sha256:") else {
                    continue;
                };
                let ext = name.rsplit('.').next().unwrap_or("");
                let blob = path.join("attachments").join(format!("{hex}.{ext}"));
                match std::fs::read(&blob) {
                    Err(_) => errs.push(format!(
                        "{}[{id}].{key}: attachment blob missing at attachments/{hex}.{ext}",
                        coll.name
                    )),
                    Ok(bytes) => {
                        let digest = format!("{:x}", Sha256::digest(&bytes));
                        if digest != hex {
                            errs.push(format!(
                                "{}[{id}].{key}: attachment blob hashes to {digest}, not ref {hex} (tampered)",
                                coll.name
                            ));
                        }
                    }
                }
            }
        }
    }
    errs
}

/// Resolve the kind tree (ADR-035 §5) from the `types` collection records
/// into `{kind -> fields}` with inheritance applied: a kind's own fields
/// plus its `extends` ancestors' (own wins on key collisions; cycles are
/// broken). Drives per-kind validation dispatch.
fn resolve_kind_schemas(
    types: &[Map<String, Value>],
) -> std::collections::BTreeMap<String, Vec<qx_contract::Field>> {
    use std::collections::{BTreeMap, BTreeSet};
    let mut own: BTreeMap<String, (Vec<qx_contract::Field>, Option<String>)> = BTreeMap::new();
    for rec in types {
        let Some(id) = rec.get("id").and_then(Value::as_str) else {
            continue;
        };
        let fields: Vec<qx_contract::Field> = rec
            .get("fields")
            .and_then(|f| serde_json::from_value(f.clone()).ok())
            .unwrap_or_default();
        let extends = rec
            .get("extends")
            .and_then(Value::as_str)
            .map(str::to_owned);
        own.insert(id.to_owned(), (fields, extends));
    }
    let mut resolved: BTreeMap<String, Vec<qx_contract::Field>> = BTreeMap::new();
    for kind in own.keys() {
        let mut acc: Vec<qx_contract::Field> = Vec::new();
        let mut keys: BTreeSet<String> = BTreeSet::new();
        let mut cur = Some(kind.clone());
        let mut seen: BTreeSet<String> = BTreeSet::new();
        while let Some(k) = cur {
            if !seen.insert(k.clone()) {
                break; // inheritance cycle — stop
            }
            let Some((fields, extends)) = own.get(&k) else {
                break;
            };
            for f in fields {
                if keys.insert(f.key.clone()) {
                    acc.push(f.clone());
                }
            }
            cur = extends.clone();
        }
        resolved.insert(kind.clone(), acc);
    }
    resolved
}

/// Audit-integrity check (ADR-037 §1): per-entry `content_hash` + stream
/// `checkpoints`. Every entry that carries a content_hash must still verify
/// (in-line tamper evidence — independent of the predecessor chain, which
/// is retired), and every checkpoint in `audit_checkpoints.jsonl` must pin
/// a stream prefix that still digests correctly. Base-free: standalone.
fn audit_integrity_check(path: &Path, failures: &mut Vec<String>) {
    let log = std::fs::read_to_string(path.join("audit_log.jsonl")).unwrap_or_default();
    for (i, line) in log.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<qx_domain::AuditEntry>(line) {
            if !qx_observability::content_hash_valid(&entry) {
                failures.push(format!(
                    "audit_log.jsonl line {}: content_hash does not match the entry body (tampered)",
                    i + 1
                ));
            }
        }
    }

    let cps = std::fs::read_to_string(path.join("audit_checkpoints.jsonl")).unwrap_or_default();
    for line in cps.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<qx_observability::Checkpoint>(line) {
            Ok(cp) => {
                if let Err(e) = qx_observability::verify_checkpoint(&cp, &log) {
                    failures.push(format!("audit_checkpoints.jsonl: {e}"));
                }
            }
            Err(e) => failures.push(format!(
                "audit_checkpoints.jsonl: malformed checkpoint: {e}"
            )),
        }
    }
}

/// Manifest↔contract FK cross-check (ADR-034 §3 / `capability-grain`).
/// When a `.qx/manifest.toml` is present, parse it and require every
/// collection it references (via `[ops]` keys or role-capability keys) to
/// be declared in the contract. A repo without a manifest is unaffected.
fn manifest_cross_check(path: &Path, failures: &mut Vec<String>) {
    let manifest_path = path.join(".qx/manifest.toml");
    let Ok(text) = std::fs::read_to_string(&manifest_path) else {
        return;
    };
    let manifest = match qx_cli::manifest::Manifest::parse(&text) {
        Ok(m) => m,
        Err(e) => {
            failures.push(format!(".qx/manifest.toml: {e}"));
            return;
        }
    };
    // The contract's declared collection roster is the FK target universe.
    let contract_path = path.join(".qx/contract.json");
    let declared: Vec<String> = match std::fs::read(&contract_path) {
        Ok(bytes) => match qx_contract::Contract::from_bytes(&bytes) {
            Ok(c) => c.collections.iter().map(|c| c.name.clone()).collect(),
            Err(_) => Vec::new(), // contract invalidity is reported by check_contract
        },
        Err(_) => Vec::new(),
    };
    let declared_refs: Vec<&str> = declared.iter().map(String::as_str).collect();
    for issue in manifest.contract_fk_issues(&declared_refs) {
        failures.push(issue);
    }
}

/// Personas accountability cross-check (ADR-036 §1/§2). When the registry
/// declares a `personas` collection (`collections/personas.jsonl`), every
/// audit `operator` must resolve to a declared persona (the typed FK), and
/// every individual CODEOWNERS principal must resolve to an *active*
/// persona. A registry without personas is unaffected (the check returns
/// early). The merge-approver half uses the same resolver
/// ([`qx_validators::approver_resolution_issues`]) fed the host's approver
/// list at CI time.
fn personas_cross_check(path: &Path, approvers: &[String], failures: &mut Vec<String>) {
    let personas = match read_jsonl_records(&path.join("collections/personas.jsonl")) {
        Ok(recs) if !recs.is_empty() => recs,
        _ => return,
    };
    let idx = qx_validators::PersonaIndex::from_records(&personas);

    // Merge approvers (ADR-036 §2): the host's approver logins (supplied by
    // CI via --approver) must each resolve to an active persona.
    let approver_refs: Vec<&str> = approvers.iter().map(String::as_str).collect();
    for issue in qx_validators::approver_resolution_issues(&idx, &approver_refs) {
        failures.push(format!("personas: {}", issue.message));
    }

    // Audit-operator FK: each distinct operator on the spine must resolve.
    let text = std::fs::read_to_string(path.join("audit_log.jsonl")).unwrap_or_default();
    let mut operators: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<qx_domain::AuditEntry>(line) {
            operators.insert(entry.actor.id.0);
        }
    }
    let op_refs: Vec<&str> = operators.iter().map(String::as_str).collect();
    for issue in qx_validators::audit_operator_fk_issues(&idx, &op_refs) {
        failures.push(format!("personas: {}", issue.message));
    }

    // CODEOWNERS principals must resolve to active personas.
    for candidate in ["CODEOWNERS", ".github/CODEOWNERS", "docs/CODEOWNERS"] {
        if let Ok(codeowners) = std::fs::read_to_string(path.join(candidate)) {
            for issue in qx_validators::codeowners_principal_issues(&idx, &codeowners) {
                failures.push(format!("personas ({candidate}): {}", issue.message));
            }
            break;
        }
    }
}

/// Index the audit spine `audit_log.jsonl` as `{id -> set of entry
/// timestamps (unix nanos)}`. Used by the lifecycle-timestamps cross-
/// check. Unparseable lines are skipped (the append-only gate guards
/// shape separately).
fn audit_timestamp_index(
    path: &Path,
) -> std::collections::BTreeMap<String, std::collections::BTreeSet<i128>> {
    use std::collections::{BTreeMap, BTreeSet};
    let mut idx: BTreeMap<String, BTreeSet<i128>> = BTreeMap::new();
    let text = std::fs::read_to_string(path.join("audit_log.jsonl")).unwrap_or_default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<qx_domain::AuditEntry>(line) else {
            continue;
        };
        let id = match &entry.target {
            qx_domain::TargetRef::Record { id, .. } => Some(id.clone()),
            qx_domain::TargetRef::Part { id } => Some(id.as_str().to_string()),
            _ => None,
        };
        if let Some(id) = id {
            idx.entry(id)
                .or_default()
                .insert(entry.timestamp.unix_timestamp_nanos());
        }
    }
    idx
}

/// Lifecycle-timestamps cross-check (ADR-035 §1b): a record's stamps
/// (`created_at`, each `transitioned_at[status]`) are validator-checked
/// CACHES of the audit spine — each must equal an audit entry's timestamp
/// for that id. A stamp with no backing spine entry is fabricated. An id
/// absent from the spine is skipped (pre-spine / legacy data, like the FK
/// universe-absent case). Returns one message per unbacked stamp.
fn stamp_provenance_issues(
    rec: &Map<String, Value>,
    audit_ts: &std::collections::BTreeMap<String, std::collections::BTreeSet<i128>>,
) -> Vec<String> {
    let Some(id) = rec.get("id").and_then(Value::as_str) else {
        return Vec::new();
    };
    let Some(ts_set) = audit_ts.get(id) else {
        return Vec::new();
    };
    let fmt = &time::format_description::well_known::Rfc3339;
    let mut stamps: Vec<(String, &str)> = Vec::new();
    if let Some(s) = rec.get("created_at").and_then(Value::as_str) {
        stamps.push(("created_at".to_string(), s));
    }
    if let Some(ta) = rec.get("transitioned_at").and_then(Value::as_object) {
        for (status, v) in ta {
            if let Some(s) = v.as_str() {
                stamps.push((format!("transitioned_at.{status}"), s));
            }
        }
    }
    let mut out = Vec::new();
    for (label, s) in stamps {
        if let Ok(t) = time::OffsetDateTime::parse(s, fmt) {
            if !ts_set.contains(&t.unix_timestamp_nanos()) {
                out.push(format!(
                    "{label} `{s}` has no matching audit-spine entry for {id} \
                     (a lifecycle stamp must be a cache of the audit spine)"
                ));
            }
        }
    }
    out
}

/// The timestamps a record carries (`created_at`, each
/// `transitioned_at[status]`) that fail the skew sanity-check (ADR-035
/// §1b timestamp-trust): a stamp more than `SKEW_TOLERANCE_HOURS` ahead of
/// `now` is clock skew or tampering; one that is not valid RFC3339 is
/// malformed. Returns one message per offending stamp.
fn timestamp_skew_issues(rec: &Map<String, Value>, now: time::OffsetDateTime) -> Vec<String> {
    const SKEW_TOLERANCE_HOURS: i64 = 48;
    let limit = now
        .checked_add(time::Duration::hours(SKEW_TOLERANCE_HOURS))
        .unwrap_or(now);
    let mut stamps: Vec<(String, &str)> = Vec::new();
    if let Some(s) = rec.get("created_at").and_then(Value::as_str) {
        stamps.push(("created_at".to_string(), s));
    }
    if let Some(ta) = rec.get("transitioned_at").and_then(Value::as_object) {
        for (status, v) in ta {
            if let Some(s) = v.as_str() {
                stamps.push((format!("transitioned_at.{status}"), s));
            }
        }
    }
    let mut out = Vec::new();
    for (label, s) in stamps {
        match time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339) {
            Ok(t) if t > limit => out.push(format!(
                "{label} `{s}` is implausibly future-dated \
                 (> now + {SKEW_TOLERANCE_HOURS}h — clock skew or tampering)"
            )),
            Ok(_) => {}
            Err(_) => out.push(format!("{label} `{s}` is not a valid RFC3339 timestamp")),
        }
    }
    out
}

/// Read a `collections/*.jsonl` file into generic JSON objects. A missing
/// file is an empty collection (no records yet), not an error.
fn read_jsonl_records(path: &Path) -> Result<Vec<Map<String, Value>>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(line) {
            Ok(Value::Object(m)) => out.push(m),
            Ok(_) => return Err(format!("line {}: not a JSON object", i + 1)),
            Err(e) => return Err(format!("line {}: {e}", i + 1)),
        }
    }
    Ok(out)
}

/// Per-collection set of record ids that are new or whose JSONL line
/// differs from the base ref — the records this PR actually touches.
/// A collection file absent at base means every HEAD record is new.
fn changed_record_ids(
    path: &Path,
    base_ref: &str,
    contract: &Contract,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut changed = BTreeMap::new();
    for coll in &contract.collections {
        let file = format!("collections/{}.jsonl", coll.name);
        let base_by_id = git_show(path, base_ref, &file)
            .map(|t| jsonl_lines_by_id(&t))
            .unwrap_or_default();
        let head_text = std::fs::read_to_string(path.join(&file)).unwrap_or_default();
        let head_by_id = jsonl_lines_by_id(&head_text);

        let mut set = BTreeSet::new();
        for (id, head_line) in &head_by_id {
            if base_by_id.get(id) != Some(head_line) {
                set.insert(id.clone());
            }
        }
        changed.insert(coll.name.clone(), set);
    }
    changed
}

/// Map record id → its raw (trimmed) JSONL line. Raw-line compare is
/// conservative: a formatting-only change reads as "changed" and the
/// record is re-validated — the safe direction (never skips a real edit).
fn jsonl_lines_by_id(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(Value::Object(m)) = serde_json::from_str::<Value>(line) {
            if let Some(id) = m.get("id").and_then(Value::as_str) {
                out.insert(id.to_string(), line.to_string());
            }
        }
    }
    out
}

// -------------------------------------------------------------------
// qx init — scaffold a deployable company data repo (ADR-039)
// -------------------------------------------------------------------

/// The code-owned canonical starter contract: parts + companies +
/// contacts. A fresh deployment begins here and EXTENDS it (add
/// collections/fields), never weakens the floor (ADR-035 guardrail #1).
const COMPANY_CONTRACT: &str = include_str!("../../../../schema/presets/company.contract.json");

/// CI gate workflow dropped into a scaffolded repo: every PR is validated
/// against the contract at its own diff (commit-resolved, ADR-039 §6).
const CHECK_WORKFLOW: &str = r#"# Generated by `qx init` — the ADR-016/039 gate. Every PR is validated
# against the contract effective at its commit; untouched records are not
# re-litigated (commit-resolved effective-dating).
name: check
on:
  pull_request:
  push:
    branches: [main]
permissions:
  contents: read
jobs:
  pr-check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - name: Install the qx engine
        # Swap for a pinned released binary in production (faster + the
        # SSoT hash you attest to — see ADR-038 release engineering).
        run: cargo install --git https://github.com/MorePET/part-registry qx-cli --bin qx --locked
      - name: Validate against the contract
        run: qx check --path . --base "origin/${{ github.base_ref || 'main' }}"
"#;

/// The scalar field types a `promote` may introduce without further
/// configuration. Reference/enum/attachment/object fields carry extra
/// required keys (target/values/constraint/schema), so they must be
/// authored by hand.
const PROMOTABLE_TYPES: [&str; 7] = [
    "string",
    "integer",
    "number",
    "decimal",
    "date",
    "timestamp",
    "bool",
];

/// Pure contract edit: add `{key, type, label}` to `collection`'s
/// `fields`, moving the key from the tier-3 open bag into a declared
/// tier-2 field (ADR-035 §3). Errors instead of producing a contract the
/// parser would reject.
fn promote_field(
    contract: &mut serde_json::Value,
    collection: &str,
    key: &str,
    field_type: &str,
    label: Option<&str>,
) -> Result<(), String> {
    if !PROMOTABLE_TYPES.contains(&field_type) {
        return Err(format!(
            "unsupported field type `{field_type}` for promote (scalar types: {}); \
             reference/enum/attachment/object fields must be authored by hand",
            PROMOTABLE_TYPES.join(", ")
        ));
    }
    if key == "id" || key == "status" {
        return Err(format!("`{key}` is an engine envelope key, not promotable"));
    }
    let colls = contract
        .get_mut("collections")
        .and_then(|c| c.as_array_mut())
        .ok_or("contract has no `collections` array")?;
    let coll = colls
        .iter_mut()
        .find(|c| c.get("name").and_then(|n| n.as_str()) == Some(collection))
        .ok_or_else(|| format!("no collection `{collection}` in the contract"))?;
    let fields = coll
        .get_mut("fields")
        .and_then(|f| f.as_array_mut())
        .ok_or_else(|| format!("collection `{collection}` has no `fields` array"))?;
    if fields
        .iter()
        .any(|f| f.get("key").and_then(|k| k.as_str()) == Some(key))
    {
        return Err(format!(
            "`{key}` is already a declared field on `{collection}`"
        ));
    }
    let mut field = serde_json::Map::new();
    field.insert(
        "key".to_string(),
        serde_json::Value::String(key.to_string()),
    );
    field.insert(
        "type".to_string(),
        serde_json::Value::String(field_type.to_string()),
    );
    field.insert(
        "label".to_string(),
        serde_json::Value::String(label.unwrap_or(key).to_string()),
    );
    fields.push(serde_json::Value::Object(field));
    Ok(())
}

/// `qx verify` — offline clone verification (ADR-037 §5). Runs the
/// base-free integrity suite: contract + record/FK validation, audit
/// content-hash + checkpoint integrity, persona accountability, and the
/// manifest↔contract FK. No PR diff, no network. `--anchors` would add the
/// anchor-ledger ancestry + immutability-enabled checks — reserved until
/// the immutable-release ledger lands (reported, not silently skipped).
fn verify_cmd(path: &Path, anchors: bool) -> ExitCode {
    let mut failures: Vec<String> = Vec::new();
    let mut notices: Vec<String> = Vec::new();

    // Contract + per-record + cross-collection FK (base=None → full scan).
    if path.join(".qx/contract.json").exists() {
        check_contract(path, None, &mut failures, &mut notices);
    } else {
        notices.push("no .qx/contract.json — contract/FK checks skipped".into());
    }
    // Audit integrity: per-entry content_hash + stream checkpoints (the
    // base-free form of the append-only guarantee).
    audit_integrity_check(path, &mut failures);
    // Persona accountability (audit-operator FK + CODEOWNERS principals).
    personas_cross_check(path, &[], &mut failures);
    // Manifest↔contract FK.
    manifest_cross_check(path, &mut failures);

    if anchors {
        // Anchor-ledger ancestry + immutability checks (ADR-037 §5). The
        // release anchor ledger does not exist yet; surface that rather
        // than silently passing.
        notices.push(
            "--anchors: anchor-ledger ancestry + immutability checks are reserved \
             (the immutable-release ledger is not yet provisioned)"
                .into(),
        );
    }

    for n in &notices {
        eprintln!("note: {n}");
    }
    if failures.is_empty() {
        println!(
            "verify: OK — clone is internally consistent ({})",
            path.display()
        );
        ExitCode::SUCCESS
    } else {
        for f in &failures {
            eprintln!("FAIL: {f}");
        }
        eprintln!("qx verify: {} failure(s)", failures.len());
        ExitCode::FAILURE
    }
}

/// `qx checkpoint` — append a stream checkpoint (ADR-037 §1). Reads the
/// current `audit_log.jsonl`, computes the cumulative digest + line count,
/// stamps the repo HEAD, and appends one checkpoint line to
/// `audit_checkpoints.jsonl` (seq = number of existing checkpoints).
fn checkpoint_cmd(path: &Path) -> ExitCode {
    let log = std::fs::read_to_string(path.join("audit_log.jsonl")).unwrap_or_default();
    let cp_path = path.join("audit_checkpoints.jsonl");
    let existing = std::fs::read_to_string(&cp_path).unwrap_or_default();
    let seq = existing.lines().filter(|l| !l.trim().is_empty()).count() as u64;
    let head_sha = git_show(path, "HEAD", "")
        .ok()
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let head_sha = if head_sha.is_empty() {
        // `git rev-parse HEAD` rather than `git show` for the bare sha.
        std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(path)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    } else {
        head_sha
    };
    let cp = qx_observability::make_checkpoint(&log, seq, head_sha);
    let line = match serde_json::to_string(&cp) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("checkpoint: serialize: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut body = existing;
    if !body.is_empty() && !body.ends_with('\n') {
        body.push('\n');
    }
    body.push_str(&line);
    body.push('\n');
    if let Err(e) = std::fs::write(&cp_path, body) {
        eprintln!("checkpoint: write {}: {e}", cp_path.display());
        return ExitCode::FAILURE;
    }
    println!(
        "checkpoint {} written: {} lines pinned, digest {}",
        cp.seq, cp.line_count, cp.stream_digest
    );
    ExitCode::SUCCESS
}

/// `qx registries` — list the operator-workspace registries (ADR-033 §5).
fn registries_cmd(path: Option<PathBuf>) -> ExitCode {
    let ws_path = match path.or_else(qx_cli::workspace::Workspace::default_path) {
        Some(p) => p,
        None => {
            eprintln!("registries: no workspace path (set --path or $XDG_CONFIG_HOME)");
            return ExitCode::FAILURE;
        }
    };
    let text = match std::fs::read_to_string(&ws_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("registries: read {}: {e}", ws_path.display());
            return ExitCode::FAILURE;
        }
    };
    let ws = match qx_cli::workspace::Workspace::parse(&text) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("registries: {e}");
            return ExitCode::FAILURE;
        }
    };
    if ws.registries.is_empty() {
        println!("(no registries listed in {})", ws_path.display());
        return ExitCode::SUCCESS;
    }
    for (name, entry) in &ws.registries {
        let marker = if ws.default.as_deref() == Some(name.as_str()) {
            " (default)"
        } else {
            ""
        };
        let identity = entry
            .identity
            .as_deref()
            .map(|i| format!("  [{i}]"))
            .unwrap_or_default();
        println!("{name}{marker}\t{}{identity}", entry.locator);
    }
    ExitCode::SUCCESS
}

fn promote_cmd(
    path: &Path,
    collection: &str,
    key: &str,
    field_type: &str,
    label: Option<&str>,
) -> ExitCode {
    let contract_path = path.join(".qx/contract.json");
    let text = match std::fs::read_to_string(&contract_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("qx promote: cannot read {}: {e}", contract_path.display());
            return ExitCode::FAILURE;
        }
    };
    let mut json: serde_json::Value = match serde_json::from_str(&text) {
        Ok(j) => j,
        Err(e) => {
            eprintln!(
                "qx promote: {} is not valid JSON: {e}",
                contract_path.display()
            );
            return ExitCode::FAILURE;
        }
    };
    if let Err(e) = promote_field(&mut json, collection, key, field_type, label) {
        eprintln!("qx promote: {e}");
        return ExitCode::FAILURE;
    }
    let out = match serde_json::to_string_pretty(&json) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("qx promote: cannot serialize the edited contract: {e}");
            return ExitCode::FAILURE;
        }
    };
    // The edit must still parse as a valid contract — never write one the
    // gate would reject.
    if let Err(e) = Contract::from_bytes(out.as_bytes()) {
        eprintln!("qx promote: the edit would produce an invalid contract: {e}");
        return ExitCode::FAILURE;
    }
    if let Err(e) = std::fs::write(&contract_path, format!("{out}\n")) {
        eprintln!("qx promote: cannot write {}: {e}", contract_path.display());
        return ExitCode::FAILURE;
    }
    println!(
        "promoted `{key}` to a `{field_type}` field on `{collection}` — \
         commit {} to land it",
        contract_path.display()
    );
    ExitCode::SUCCESS
}

fn init_repo(path: &Path, force: bool) -> ExitCode {
    // The contract is the SSOT — validate the embedded preset BEFORE
    // writing it, so `qx init` can never lay down an invalid repo.
    let contract = match Contract::from_bytes(COMPANY_CONTRACT.as_bytes()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("qx init: embedded preset is invalid (build bug): {e}");
            return ExitCode::FAILURE;
        }
    };

    let contract_path = path.join(".qx/contract.json");
    if contract_path.exists() && !force {
        eprintln!(
            "qx init: {} already exists (use --force to overwrite)",
            contract_path.display()
        );
        return ExitCode::FAILURE;
    }

    // Lay down the scaffold. Each write is reported so the user sees the
    // shape of what a deployment is.
    let mut wrote: Vec<String> = Vec::new();
    macro_rules! write_file {
        ($rel:expr, $content:expr) => {{
            let p = path.join($rel);
            if let Some(parent) = p.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    eprintln!("qx init: mkdir {}: {e}", parent.display());
                    return ExitCode::FAILURE;
                }
            }
            if let Err(e) = std::fs::write(&p, $content) {
                eprintln!("qx init: write {}: {e}", p.display());
                return ExitCode::FAILURE;
            }
            wrote.push($rel.to_string());
        }};
    }

    write_file!(".qx/contract.json", COMPANY_CONTRACT);
    // One empty append-only stream per collection the contract declares.
    for coll in &contract.collections {
        let rel = format!("collections/{}.jsonl", coll.name);
        write_file!(&rel, "");
    }
    write_file!(".github/workflows/check.yml", CHECK_WORKFLOW);
    write_file!("README.md", &readme_for(&contract));

    println!(
        "qx init: scaffolded a company data repo in {}",
        path.display()
    );
    for f in &wrote {
        println!("  + {f}");
    }
    println!(
        "\ncollections: {}",
        contract
            .collections
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!("next: `git init && git add -A && git commit`, push, and the gate runs on every PR.");
    ExitCode::SUCCESS
}

/// A README that explains the scaffolded repo and how to grow it.
fn readme_for(contract: &Contract) -> String {
    let collections = contract
        .collections
        .iter()
        .map(|c| format!("- `{}` ({} fields)", c.name, c.fields.len()))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"# A qx data repo

A git-native registry scaffolded by `qx init`. The **contract**
(`.qx/contract.json`) is the source of truth for what this
repo holds and how each record is validated; records live as append-only
NDJSON under `collections/`.

## Collections

{collections}

## How it works

- **Add / edit records** by committing lines to `collections/<name>.jsonl`
  (or through the app). Each record is a JSON object with an `id`, an
  optional lifecycle `status`, and the fields the contract declares.
- **Every PR is gated**: `qx check --base origin/main` validates the
  records you touched against the contract — types, required fields,
  enum/reference policy, foreign-key integrity, lifecycle transitions.
  Records you did not touch are not re-validated (commit-resolved
  effective-dating).
- **The contract is versioned with the data**: changing it is a PR like
  any other; its identity is its content hash, and who approved it is the
  merge itself.

## Growing this repo

The contract is a **floor you extend, never weaken**. To add a domain
(e.g. controlled documents / SOPs, suppliers, training records), add a
collection to `.qx/contract.json` — no engine change. The same
gate then governs it. See the project's QMS preset family for the
SOP/QA/QC vertical.
"#
    )
}

struct CsvTable {
    header: Vec<String>,
    rows: Vec<BTreeMap<String, String>>,
}

fn read_csv_rows(path: &Path) -> Result<CsvTable, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    parse_csv_text(&text)
}

fn parse_csv_text(text: &str) -> Result<CsvTable, String> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(text.as_bytes());
    let header: Vec<String> = rdr
        .headers()
        .map_err(|e| e.to_string())?
        .iter()
        .map(ToOwned::to_owned)
        .collect();
    let mut rows = Vec::new();
    for rec in rdr.records() {
        let rec = rec.map_err(|e| e.to_string())?;
        let mut row = BTreeMap::new();
        for (k, v) in header.iter().zip(rec.iter()) {
            if !v.is_empty() {
                row.insert(k.clone(), v.to_string());
            }
        }
        rows.push(row);
    }
    Ok(CsvTable { header, rows })
}

/// Parse the minimum `Part` surface the structural validators need
/// (id + status); rows that don't parse become failures rather than
/// silently skipped.
fn rows_to_parts(
    rows: &[BTreeMap<String, String>],
    failures: &mut Vec<String>,
) -> Vec<qx_domain::Part> {
    let mut parts = Vec::with_capacity(rows.len());
    for (i, row) in rows.iter().enumerate() {
        let line = i + 2; // header is line 1
        let id = match row.get("id").map(|s| PartId::new(s.clone())) {
            Some(Ok(id)) => id,
            Some(Err(e)) => {
                failures.push(format!("registry.csv:{line}: id: {e}"));
                continue;
            }
            None => {
                failures.push(format!("registry.csv:{line}: missing id"));
                continue;
            }
        };
        let status = match row.get("status").map(|s| s.parse::<PartStatus>()) {
            Some(Ok(s)) => s,
            Some(Err(e)) => {
                failures.push(format!("registry.csv:{line}: {e}"));
                continue;
            }
            None => {
                failures.push(format!("registry.csv:{line}: missing status"));
                continue;
            }
        };
        let minted_at = row
            .get("minted_at")
            .and_then(|s| {
                time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok()
            })
            .unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
        parts.push(qx_domain::Part {
            id,
            status,
            minted_at,
            bound_at: None,
            type_: row.get("type").cloned(),
            description: row.get("description").cloned(),
            vendor: row.get("vendor").cloned(),
            part_number: row.get("part_number").cloned(),
            location: row.get("location").cloned(),
            notes: row.get("notes").cloned(),
            minted_by: row.get("minted_by").cloned(),
            bound_by: row.get("bound_by").cloned(),
            last_edited_at: row.get("last_edited_at").cloned(),
            last_edited_by: row.get("last_edited_by").cloned(),
            components: Vec::new(),
            manufacturer_id: row.get("manufacturer_id").cloned(),
            metadata: std::collections::BTreeMap::new(),
            signatures: Vec::new(),
            chain_hash: None,
        });
    }
    parts
}

/// The contract's derived identity: `sha256:<hex>` of its on-disk bytes
/// (ADR-039 §6 — content-addressed, never stored in-file).
fn contract_identity(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("sha256:{:x}", Sha256::digest(bytes))
}

/// Returns a violation message if `head` is not `base` plus trailing
/// additions (ADR-037 §1 append-only): every non-empty base line must
/// appear unchanged, in order, as a prefix of head's non-empty lines.
fn audit_append_only_violation(base: &str, head: &str) -> Option<String> {
    let base_lines: Vec<&str> = base.lines().filter(|l| !l.trim().is_empty()).collect();
    let head_lines: Vec<&str> = head.lines().filter(|l| !l.trim().is_empty()).collect();
    if head_lines.len() < base_lines.len() {
        return Some(format!(
            "{} entry(ies) removed (had {}, now {}) — the audit log is append-only",
            base_lines.len() - head_lines.len(),
            base_lines.len(),
            head_lines.len()
        ));
    }
    for (i, (b, h)) in base_lines.iter().zip(head_lines.iter()).enumerate() {
        if b != h {
            return Some(format!(
                "entry {} changed — existing audit entries are immutable (append-only)",
                i + 1
            ));
        }
    }
    None
}

fn git_show(repo: &Path, git_ref: &str, file: &str) -> Result<String, String> {
    let out = std::process::Command::new("git")
        .args(["show", &format!("{git_ref}:{file}")])
        .current_dir(repo)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    String::from_utf8(out.stdout).map_err(|e| e.to_string())
}

fn build_diff(base: &CsvTable, head: &CsvTable) -> Diff {
    let mut diff = Diff::default();
    if base.header != head.header {
        diff.header_changes.push(HeaderChange {
            file: "registry.csv".into(),
            before: base.header.clone(),
            after: head.header.clone(),
        });
    }
    let key = |row: &BTreeMap<String, String>| row.get("id").cloned().unwrap_or_default();
    let base_by_id: BTreeMap<String, &BTreeMap<String, String>> =
        base.rows.iter().map(|r| (key(r), r)).collect();
    let head_by_id: BTreeMap<String, &BTreeMap<String, String>> =
        head.rows.iter().map(|r| (key(r), r)).collect();

    for (id, row) in &head_by_id {
        match base_by_id.get(id) {
            None => diff.adds.push(DiffRow {
                id: PartId::new(id.clone()).ok(),
                fields: (*row).clone(),
            }),
            Some(before) if before != row => {
                let changed_keys: Vec<String> = row
                    .iter()
                    .filter(|(k, v)| before.get(k.as_str()) != Some(*v))
                    .map(|(k, _)| k.clone())
                    .chain(before.keys().filter(|k| !row.contains_key(*k)).cloned())
                    .collect();
                if let Ok(pid) = PartId::new(id.clone()) {
                    diff.edits.push(DiffEdit {
                        id: pid,
                        before: (*before).clone(),
                        after: (*row).clone(),
                        changed_keys,
                    });
                }
            }
            Some(_) => {}
        }
    }
    for (id, row) in &base_by_id {
        if !head_by_id.contains_key(id) {
            diff.deletes.push(DiffRow {
                id: PartId::new(id.clone()).ok(),
                fields: (*row).clone(),
            });
        }
    }
    diff
}

fn check_transitions(diff: &Diff, failures: &mut Vec<String>) {
    for e in &diff.edits {
        let parse = |m: &BTreeMap<String, String>| {
            m.get("status").and_then(|s| s.parse::<PartStatus>().ok())
        };
        if let (Some(from), Some(to)) = (parse(&e.before), parse(&e.after)) {
            if let Err(err) = validate_status_transition(from, to) {
                failures.push(format!("{}: {err}", e.id));
            }
        }
    }
}

/// Run the ADR-016 policy table over the classified diff. The identity
/// dimension belongs to the host (ADR-034 — the PR author's identity
/// and required reviews are GitHub's to enforce); the check evaluates
/// the *action* dimension with a synthetic verified CI context, so the
/// outcome reflects what the change *is*, not who proposed it.
fn advise_policy(diff: &Diff, failures: &mut Vec<String>, notices: &mut Vec<String>) {
    let ci_operator = Operator {
        id: OperatorId("ci:pr-check".into()),
        display_name: "qx check (advisory)".into(),
        source: IdentitySource::OfflineClaim,
        verified_at: Some(time::OffsetDateTime::now_utc()),
        claims: BTreeMap::new(),
        pubkey: None,
    };
    let actions = diff.classify();
    let decision = policy_decision(diff, &ci_operator, &Policy::default());
    notices.push(format!(
        "classification: {} action(s): {}",
        actions.len(),
        actions
            .iter()
            .map(|a| format!("{:?}", a.kind()))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    match decision {
        qx_domain::AuthDecision::Allow => {
            notices.push("policy: allow".into());
        }
        qx_domain::AuthDecision::Warn { reason } => {
            notices.push(format!("policy: warn — {reason}"));
        }
        qx_domain::AuthDecision::RequiresElevation { approver_role } => {
            notices.push(format!(
                "policy: requires elevation — CODEOWNERS review by `{approver_role}` \
                 enforces this (ADR-034)"
            ));
        }
        qx_domain::AuthDecision::Block { reason } => {
            failures.push(format!("policy: block — {reason}"));
        }
    }
}

// ---------- tests ----------

#[cfg(test)]
mod tests {
    use super::*;

    const PROMOTE_CONTRACT: &str = r#"{"format_version":1,"collections":[
        {"name":"parts","id":{"scheme":"nano14","default":true,"mintable":true},
         "open_properties":true,
         "lifecycle":{"statuses":["unbound","bound","void"],"initial":"unbound",
           "transitions":{"unbound":["bound","void"],"bound":["void"],"void":[]}},
         "fields":[{"key":"type","type":"string","label":"Type"}]}]}"#;

    #[test]
    fn promote_field_adds_a_declared_field_and_stays_valid() {
        let mut c: serde_json::Value = serde_json::from_str(PROMOTE_CONTRACT).unwrap();
        promote_field(&mut c, "parts", "manufacturer", "string", None).expect("promote ok");
        let fields = c["collections"][0]["fields"].as_array().unwrap();
        assert!(
            fields.iter().any(|f| f["key"] == "manufacturer"),
            "the promoted key is now a declared field"
        );
        // The edited contract must still parse as a valid contract.
        let bytes = serde_json::to_vec(&c).unwrap();
        qx_contract::Contract::from_bytes(&bytes).expect("the edited contract parses");
    }

    #[test]
    fn promote_field_rejects_invalid_promotions() {
        let mut c: serde_json::Value = serde_json::from_str(PROMOTE_CONTRACT).unwrap();
        // already a declared field
        assert!(promote_field(&mut c, "parts", "type", "string", None).is_err());
        // unknown collection
        assert!(promote_field(&mut c, "widgets", "x", "string", None).is_err());
        // a non-promotable (structured) type
        assert!(promote_field(&mut c, "parts", "x", "reference", None).is_err());
        // an engine envelope key
        assert!(promote_field(&mut c, "parts", "status", "string", None).is_err());
    }

    #[test]
    fn timestamp_skew_flags_future_and_malformed_stamps() {
        let fmt = &time::format_description::well_known::Rfc3339;
        let now = time::OffsetDateTime::parse("2026-06-27T00:00:00Z", fmt).unwrap();
        let obj = |s: &str| serde_json::from_str::<Map<String, Value>>(s).unwrap();

        // A past created_at is fine.
        assert!(
            timestamp_skew_issues(&obj(r#"{"created_at":"2026-05-01T00:00:00Z"}"#), now).is_empty()
        );
        // A far-future created_at is clock skew / tampering.
        assert!(
            timestamp_skew_issues(&obj(r#"{"created_at":"2027-01-01T00:00:00Z"}"#), now)
                .iter()
                .any(|m| m.contains("future-dated"))
        );
        // A malformed stamp is flagged.
        assert!(
            timestamp_skew_issues(&obj(r#"{"created_at":"not-a-date"}"#), now)
                .iter()
                .any(|m| m.contains("not a valid"))
        );
        // transitioned_at[status] stamps are checked too.
        assert!(timestamp_skew_issues(
            &obj(r#"{"transitioned_at":{"bound":"2030-01-01T00:00:00Z"}}"#),
            now
        )
        .iter()
        .any(|m| m.contains("transitioned_at.bound")));
    }

    #[test]
    fn stamp_provenance_cross_check_matches_the_audit_spine() {
        let fmt = &time::format_description::well_known::Rfc3339;
        let t = time::OffsetDateTime::parse("2026-06-27T12:00:00Z", fmt).unwrap();
        let op = qx_domain::Operator {
            id: qx_domain::OperatorId("test".into()),
            display_name: "T".into(),
            source: qx_domain::IdentitySource::GitConfig,
            verified_at: None,
            claims: std::collections::BTreeMap::new(),
            pubkey: None,
        };
        // A real audit entry (ts = t) serialized to audit_log.jsonl, then
        // indexed — verifies the timestamp round-trips through serde.
        let entry = qx_observability::record_write_audit_entry(
            qx_domain::RequestId::new(),
            op,
            "companies".into(),
            "CMPY2223AAAAAA".into(),
            serde_json::json!({}),
            t,
        );
        let line = serde_json::to_string(&entry).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("audit_log.jsonl"), format!("{line}\n")).unwrap();
        let idx = audit_timestamp_index(tmp.path());

        // created_at equal to the spine entry's ts passes (exact match).
        let ok: Map<String, Value> =
            serde_json::from_str(r#"{"id":"CMPY2223AAAAAA","created_at":"2026-06-27T12:00:00Z"}"#)
                .unwrap();
        assert!(
            stamp_provenance_issues(&ok, &idx).is_empty(),
            "a stamp backed by the spine passes: {:?}",
            stamp_provenance_issues(&ok, &idx)
        );
        // A non-matching created_at is flagged — fabricated stamp.
        let bad: Map<String, Value> =
            serde_json::from_str(r#"{"id":"CMPY2223AAAAAA","created_at":"2026-06-27T13:00:00Z"}"#)
                .unwrap();
        assert!(stamp_provenance_issues(&bad, &idx)
            .iter()
            .any(|m| m.contains("no matching audit-spine")));
        // An id absent from the spine is skipped (cannot verify).
        let absent: Map<String, Value> =
            serde_json::from_str(r#"{"id":"ZZZZZZZZZZZZZZ","created_at":"2030-01-01T00:00:00Z"}"#)
                .unwrap();
        assert!(stamp_provenance_issues(&absent, &idx).is_empty());
    }

    #[test]
    fn resolve_kind_schemas_applies_extends_inheritance() {
        let obj = |s: &str| serde_json::from_str::<Map<String, Value>>(s).unwrap();
        let types = vec![
            obj(r#"{"id":"component","fields":[{"key":"footprint","type":"string","label":"F"}]}"#),
            obj(
                r#"{"id":"resistor","extends":"component","fields":[{"key":"resistance","type":"string","label":"R"}]}"#,
            ),
        ];
        let ks = resolve_kind_schemas(&types);
        let resistor: Vec<&str> = ks["resistor"].iter().map(|f| f.key.as_str()).collect();
        // resistor carries its own field AND the inherited parent field.
        assert!(resistor.contains(&"resistance"));
        assert!(
            resistor.contains(&"footprint"),
            "inherits the parent's field: {resistor:?}"
        );
        // component has only its own.
        let component: Vec<&str> = ks["component"].iter().map(|f| f.key.as_str()).collect();
        assert_eq!(component, vec!["footprint"]);
    }

    #[test]
    fn contract_identity_is_content_addressed() {
        let a = contract_identity(b"{\"format_version\":1}");
        assert_eq!(a, contract_identity(b"{\"format_version\":1}"));
        assert_ne!(a, contract_identity(b"{\"format_version\":2}"));
        assert!(a.starts_with("sha256:") && a.len() == 7 + 64);
    }

    #[test]
    fn audit_append_only_detects_tampering() {
        let base = "{\"a\":1}\n{\"b\":2}\n";
        // Pure trailing append → ok.
        assert!(audit_append_only_violation(base, "{\"a\":1}\n{\"b\":2}\n{\"c\":3}\n").is_none());
        // A removed entry → violation.
        assert!(audit_append_only_violation(base, "{\"a\":1}\n")
            .unwrap()
            .contains("removed"));
        // A changed existing entry → violation.
        assert!(audit_append_only_violation(base, "{\"a\":99}\n{\"b\":2}\n")
            .unwrap()
            .contains("changed"));
        // Empty base (first audit log) → any head is append-only.
        assert!(audit_append_only_violation("", "{\"a\":1}\n").is_none());
    }

    // ---------- --size: the unit rides the value (ADR-031 §8) ----------

    #[test]
    fn size_spec_parses_px_mm_and_bare_mm() {
        let cases = [
            ("64px", SizeSpec::Px(64)),
            (" 64px ", SizeSpec::Px(64)),
            ("8mm", SizeSpec::Mm(8.0)),
            ("8", SizeSpec::Mm(8.0)),
            ("8.5", SizeSpec::Mm(8.5)),
            ("8.5mm", SizeSpec::Mm(8.5)),
        ];
        for (input, expected) in cases {
            assert_eq!(parse_size_spec(input), Ok(expected), "input {input:?}");
        }
    }

    #[test]
    fn size_spec_rejects_fractional_px_and_nonsense() {
        for bad in [
            "64.5px", "-3px", "0px", "px", "mm", "", "0", "-8", "NaNmm", "8cm",
        ] {
            assert!(parse_size_spec(bad).is_err(), "must reject {bad:?}");
        }
    }

    // ---------- --padding: the CSS shorthand value parser ----------

    #[test]
    fn padding_value_parser_accepts_the_three_arities() {
        for input in ["2", "2,6", "2,6,4,6"] {
            assert!(parse_padding_spec(input).is_ok(), "input {input:?}");
        }
        for bad in ["2,6,4", "", "two"] {
            assert!(parse_padding_spec(bad).is_err(), "must reject {bad:?}");
        }
    }

    // ---------- clap wiring stays valid ----------

    #[test]
    fn cli_definition_is_internally_consistent() {
        use clap::CommandFactory as _;
        Cli::command().debug_assert();
    }

    // ---------- ADR-031 §10 content sugar + id-size parser ----------

    #[test]
    fn content_sugar_expands_into_payload_strings() {
        assert_eq!(content_to_payload(Some("qr+id")).as_deref(), Some("qr id"));
        assert_eq!(content_to_payload(Some("id+qr")).as_deref(), Some("id qr"));
        assert_eq!(content_to_payload(Some("qr")).as_deref(), Some("qr"));
        assert_eq!(content_to_payload(Some("id")).as_deref(), Some("id"));
        assert_eq!(content_to_payload(None), None);
        // Unknown content passes through so the engine surfaces the
        // payload grammar error.
        assert_eq!(content_to_payload(Some("xx")).as_deref(), Some("xx"));
    }

    #[test]
    fn id_size_spec_parses_px_and_mm() {
        // Suffix grammar like --size: bare = mm, explicit px is px.
        assert_eq!(parse_id_size_spec("28px"), Ok(28));
        // 8mm at 300dpi rounds to 94.
        assert_eq!(parse_id_size_spec("8mm"), Ok(94));
        // Bare reads as mm (consistent with --size).
        assert_eq!(parse_id_size_spec("8"), Ok(94));
    }

    #[test]
    fn id_size_spec_rejects_nonsense() {
        for bad in ["", "0px", "-3", "abc", "NaN"] {
            assert!(parse_id_size_spec(bad).is_err(), "must reject {bad:?}");
        }
    }
}
