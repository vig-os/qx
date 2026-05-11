//! `bind` — replaces `bind.py` per ADR-017 strangler-fig step 7.
//! Foundation scaffold. Observability wired per ADR-022 — root span
//! carries a `request_id` so future emits inherit it.

use clap::Parser;

use part_registry_domain::RequestId;
use part_registry_observability::{
    cli_scaffold_operator, init, set_current_operator, AuditSinkHandle, ObservabilityConfig,
};

#[derive(Parser, Debug)]
#[command(name = "bind", about = "Bind a part ID to a serial / batch / location")]
struct Args {
    /// Canonical part ID to bind.
    id: String,

    /// Free-form binding payload (placeholder until ADR-bind lands).
    #[arg(long)]
    to: Option<String>,
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
    let span = tracing::info_span!("cli.bind", request_id = %rid);
    let _g = span.enter();

    tracing::info!(request_id = %rid, "foundation scaffold; not yet implemented");
    eprintln!("foundation scaffold; not yet implemented");
    std::process::exit(2);
}
