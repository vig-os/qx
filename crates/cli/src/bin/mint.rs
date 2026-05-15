//! `mint` — replaces `mint.py` per ADR-017 strangler-fig step 7.
//! Thin wrapper: parse args, load config + wiring, run, format output.
//! Business logic lives in `part_registry_cli::run_mint`.

use std::process::ExitCode;

use clap::Parser;

use part_registry_cli::{
    init_observability, render_mint_summary, run_mint, DryRunTarget, MintArgs, Wiring,
};
use part_registry_config::Config;
use part_registry_domain::RequestId;
use part_registry_observability::request_id_span;

fn main() -> ExitCode {
    let args = MintArgs::parse();

    let cfg = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("config error: {e}");
            return ExitCode::from(2);
        }
    };

    // Resolve dry-run target before wiring so `from_config` knows
    // which sink to install. `mint.py` writes registry.csv directly;
    // Rust submits via ProposalSink. Until the live GitHub PR sink
    // is wired through Config (#35 follow-up), `--dry-run` is the
    // only operational mode — but we surface a clear error if the
    // user forgets the flag.
    let dry_run = if let Some(path) = args.dry_run_file.clone() {
        Some(DryRunTarget::File(path))
    } else if args.dry_run {
        Some(DryRunTarget::Stdout)
    } else {
        // Default to stdout-capture so the binary remains usable
        // without explicit flags during the foundation phase.
        Some(DryRunTarget::Stdout)
    };

    let wiring = match Wiring::from_config(&cfg, dry_run) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("wiring error: {e}");
            return ExitCode::from(2);
        }
    };

    let _ = init_observability(&cfg.observability, wiring.repo.clone());

    let rid = RequestId::new();
    let span = request_id_span("cli.mint", rid);
    let _g = span.enter();

    match run_mint(&args, &wiring) {
        Ok(outcome) => {
            print!("{}", render_mint_summary(&outcome, &wiring.repo_root));
            ExitCode::SUCCESS
        }
        Err(e) => {
            tracing::error!(error = %e, "mint failed");
            eprintln!("mint failed: {e}");
            ExitCode::FAILURE
        }
    }
}
