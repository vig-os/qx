//! `pr` — the multicall binary per ADR-030 §2: one native artifact for
//! the whole shell family.
//!
//! - `pr mint` / `pr label` / `pr bind` — parity delegates onto the
//!   same engine the legacy single-purpose binaries use (strangler-fig:
//!   those stay until parity retirement). Unlike the legacy bins,
//!   omitting `--dry-run` here uses the **live** GitHub PR sink
//!   (ADR-030 build-order step 2).
//! - `pr list|resolve|describe|count|export|print|whoami` — thin shells
//!   over `part_registry_app::dispatch` (the command protocol); output
//!   is the protocol's JSON `data` payload, pretty-printed.
//! - `pr check` — the ADR-016 gate: structural validation of a data
//!   repo plus, with `--base <git-ref>`, semantic-diff classification +
//!   policy per ADR-034 (tool classifies/advises; the host's branch
//!   protection + CODEOWNERS enforce).
//!
//! `serve` / `mcp` / `tui` land behind cargo features per ADR-030 §2.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use part_registry_app::{dispatch, AppContext, Request, Response};
use part_registry_cli::{
    init_observability, render_bind_summary, render_mint_summary, run_bind, run_label, run_mint,
    BindArgs, DryRunTarget, LabelArgs, MintArgs, Wiring,
};
use part_registry_config::Config;
use part_registry_domain::{
    Diff, DiffEdit, DiffRow, HeaderChange, IdentitySource, Operator, OperatorId, PartId,
    PartStatus, RequestId,
};
use part_registry_observability::{request_id_span, ObservabilityConfig};
use part_registry_validators::{
    policy_decision, registry_sort_key, validate_sort_stable, validate_status_transition,
    validate_unique_ids, Policy,
};

#[derive(Parser)]
#[command(
    name = "pr",
    about = "part-registry — one binary, every shell (ADR-030)",
    version
)]
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
        padding: Option<part_registry_app::PaddingSpec>,
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
fn parse_padding_spec(s: &str) -> Result<part_registry_app::PaddingSpec, String> {
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
        Cmd::Check { path, base } => check(&path, base.as_deref()),
        #[cfg(feature = "serve")]
        Cmd::Serve { addr, static_dir } => serve_cmd(addr, static_dir),
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
    match part_registry_cli::tui::run(ctx) {
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
                "pr mcp: no GitHub token resolved — mutations will be captured as \
                 dry-run JSON, not submitted (set PART_REGISTRY__TRANSPORT__GITHUB_TOKEN)."
            );
            // Stdout carries the MCP wire; dry-run capture must go to a
            // file, never stdout.
            let capture = std::env::temp_dir().join("pr-mcp-dry-run.jsonl");
            eprintln!("pr mcp: dry-run proposals -> {}", capture.display());
            match build_wiring(&cfg, Some(DryRunTarget::File(capture))) {
                Ok(w) => w,
                Err(e) => return e,
            }
        }
    };
    init_obs(&cfg, &wiring);
    let ctx = app_context(&cfg, wiring);
    match part_registry_cli::mcp::run(ctx) {
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
                "pr serve: no GitHub token resolved — mutations will be captured \
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
    let span = request_id_span("pr.serve", RequestId::new());
    let _g = span.enter();
    let ctx = app_context(&cfg, wiring);
    match part_registry_cli::serve::run(ctx, addr, static_dir) {
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
    let registry_name = part_registry_config::parse_owner_repo(&cfg.repo.data_repo_url)
        .map(|(o, r)| format!("{o}/{r}"))
        .unwrap_or_else(|_| cfg.repo.data_repo_url.clone());
    AppContext {
        repo: wiring.repo,
        identity: wiring.identity,
        sink: wiring.sink,
        registry_name,
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
    let dry = dry_run_target(args.dry_run, &args.dry_run_file);
    let wiring = match build_wiring(&cfg, dry) {
        Ok(w) => w,
        Err(e) => return e,
    };
    init_obs(&cfg, &wiring);
    let span = request_id_span("pr.mint", RequestId::new());
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
    let span = request_id_span("pr.label", RequestId::new());
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
    let span = request_id_span("pr.bind", RequestId::new());
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
    let span = request_id_span("pr.dispatch", RequestId::new());
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
        let options = part_registry_app::PrintOptions {
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
        };
        return protocol_print(&ctx, ids, options, &out_dir);
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
                filter: part_registry_app::Filter {
                    status,
                    kind: None,
                    text,
                    fields: fields.into_iter().collect(),
                },
                sort: Vec::new(),
                page: part_registry_app::Page { offset, limit },
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
                filter: part_registry_app::Filter {
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
    options: part_registry_app::PrintOptions,
    out_dir: &Path,
) -> ExitCode {
    let resp = dispatch(
        ctx,
        Request::Print {
            collection: "parts".into(),
            selection: part_registry_app::Selection::Ids(ids),
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
            for l in &labels {
                let id = l["id"].as_str().unwrap_or("label");
                let svg = l["svg"].as_str().unwrap_or_default();
                let path = out_dir.join(format!("{id}.svg"));
                if let Err(e) = std::fs::write(&path, svg) {
                    eprintln!("write {}: {e}", path.display());
                    return ExitCode::FAILURE;
                }
            }
            println!(
                "rendered {} label(s) -> {}",
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
// pr check — the ADR-016 gate (ADR-034: classify + advise; the host
// enforces)
// -------------------------------------------------------------------

fn check(path: &Path, base: Option<&str>) -> ExitCode {
    let registry_path = path.join("registry.csv");
    let head = match read_csv_rows(&registry_path) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("pr check: read {}: {e}", registry_path.display());
            return ExitCode::FAILURE;
        }
    };

    let mut failures: Vec<String> = Vec::new();
    let mut notices: Vec<String> = Vec::new();

    // Structural validation over the head state.
    let head_parts = rows_to_parts(&head.rows, &mut failures);
    if let Err(e) = validate_unique_ids(&head_parts) {
        failures.push(format!("unique-ids: {e}"));
    }
    if let Err(e) = validate_sort_stable(&head_parts, registry_sort_key) {
        failures.push(format!("sort-stability: {e}"));
    }

    // Semantic diff vs base (ADR-016).
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

    for n in &notices {
        println!("notice: {n}");
    }
    if failures.is_empty() {
        println!(
            "pr check: OK ({} rows{})",
            head.rows.len(),
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
        eprintln!("pr check: {} failure(s)", failures.len());
        ExitCode::FAILURE
    }
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
) -> Vec<part_registry_domain::Part> {
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
        parts.push(part_registry_domain::Part {
            id,
            status,
            minted_at,
            batch: row.get("batch").cloned(),
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
        display_name: "pr check (advisory)".into(),
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
        part_registry_domain::AuthDecision::Allow => {
            notices.push("policy: allow".into());
        }
        part_registry_domain::AuthDecision::Warn { reason } => {
            notices.push(format!("policy: warn — {reason}"));
        }
        part_registry_domain::AuthDecision::RequiresElevation { approver_role } => {
            notices.push(format!(
                "policy: requires elevation — CODEOWNERS review by `{approver_role}` \
                 enforces this (ADR-034)"
            ));
        }
        part_registry_domain::AuthDecision::Block { reason } => {
            failures.push(format!("policy: block — {reason}"));
        }
    }
}

// ---------- tests ----------

#[cfg(test)]
mod tests {
    use super::*;

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
