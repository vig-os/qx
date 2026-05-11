//! `label` — replaces `label.py` per ADR-017 strangler-fig step 7.
//! Foundation scaffold. Observability wired per ADR-022 — root span
//! carries a `request_id`. `label` is the mutator that historically
//! wrote `print_log.csv`; per ADR-022 §"Migration of print_log.csv"
//! it will write to `audit_log.csv` via the audit-CSV layer once #32
//! wires the full label pipeline through.

use clap::Parser;

use part_registry_domain::RequestId;
use part_registry_observability::{
    cli_scaffold_operator, init, set_current_operator, AuditSinkHandle, ObservabilityConfig,
};

#[derive(Parser, Debug)]
#[command(name = "label", about = "Render label SVGs for part IDs")]
struct Args {
    /// Canonical part IDs to render.
    #[arg(required = true)]
    ids: Vec<String>,

    /// Tape height in mm (per ADR-021 default override).
    #[arg(long)]
    size: Option<f64>,

    /// Use Micro QR (M4) instead of Standard QR.
    #[arg(long, default_value_t = false)]
    micro: bool,
}

fn main() {
    let _args = Args::parse();

    let cfg = ObservabilityConfig {
        audit_csv: false,
        ..ObservabilityConfig::cli_defaults()
    };
    let _ = init(&cfg, AuditSinkHandle::disabled());

    set_current_operator(cli_scaffold_operator());

    let rid = RequestId::new();
    let span = tracing::info_span!("cli.label", request_id = %rid);
    let _g = span.enter();

    tracing::info!(request_id = %rid, "foundation scaffold; not yet implemented");
    eprintln!("foundation scaffold; not yet implemented");
    std::process::exit(2);
}
