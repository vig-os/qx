//! `bind` — replaces `bind.py` per ADR-017 strangler-fig step 7.
//! Thin wrapper: parse args, load config + wiring, run, format output.
//! Business logic lives in `part_registry_cli::run_bind`.

use std::process::ExitCode;

use clap::Parser;

use part_registry_cli::{
    init_observability, render_bind_summary, run_bind, BindArgs, DryRunTarget, Wiring,
};
use part_registry_config::Config;
use part_registry_domain::RequestId;
use part_registry_observability::request_id_span;

fn main() -> ExitCode {
    let args = BindArgs::parse();

    let cfg = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("config error: {e}");
            return ExitCode::from(2);
        }
    };

    let dry_run = if let Some(path) = args.dry_run_file.clone() {
        Some(DryRunTarget::File(path))
    } else {
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
    let span = request_id_span("cli.bind", rid);
    let _g = span.enter();

    match run_bind(&args, &wiring) {
        Ok(outcome) => {
            print!("{}", render_bind_summary(&outcome));
            // Per spec: print proposal URL for downstream tooling.
            println!("{}", outcome.proposal_ref.url);
            ExitCode::SUCCESS
        }
        Err(e) => {
            tracing::error!(error = %e, "bind failed");
            eprintln!("bind failed: {e}");
            ExitCode::FAILURE
        }
    }
}
