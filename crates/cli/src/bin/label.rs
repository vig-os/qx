//! `label` — replaces `label.py` per ADR-017 strangler-fig step 7.
//! Thin wrapper: parse args, load config + wiring, run, format output.
//! Business logic lives in `part_registry_cli::run_label`.

use std::process::ExitCode;

use clap::Parser;

use part_registry_cli::{init_observability, render_label_summary, run_label, LabelArgs, Wiring};
use part_registry_config::Config;
use part_registry_domain::RequestId;
use part_registry_observability::{request_id_span, ObservabilityConfig};

fn main() -> ExitCode {
    let args = LabelArgs::parse();

    let cfg = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("config error: {e}");
            return ExitCode::from(2);
        }
    };

    // `label` is a read+print-log writer only — no ProposalSink path.
    // The Wiring still wants one for the bundle; we install the
    // dry-run sink as a no-op since `run_label` never submits.
    let wiring = match Wiring::from_config(&cfg, Some(part_registry_cli::DryRunTarget::Stdout)) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("wiring error: {e}");
            return ExitCode::from(2);
        }
    };

    let obs_cfg = ObservabilityConfig {
        log_level: cfg.observability.log_level.clone(),
        stdout_json: cfg.observability.stdout_json,
        stderr_human: cfg.observability.stderr_human,
        audit_csv: cfg.observability.audit_csv,
    };
    let _ = init_observability(&obs_cfg, wiring.repo.clone());

    let rid = RequestId::new();
    let span = request_id_span("cli.label", rid);
    let _g = span.enter();

    match run_label(&args, &wiring) {
        Ok(outcome) => {
            if let Some(w) = &outcome.warning {
                eprintln!("info: {w}");
            }
            print!("{}", render_label_summary(&outcome, args.cable_od));
            ExitCode::SUCCESS
        }
        Err(e) => {
            tracing::error!(error = %e, "label failed");
            eprintln!("label failed: {e}");
            ExitCode::FAILURE
        }
    }
}
