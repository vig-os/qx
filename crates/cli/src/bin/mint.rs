//! `mint` — replaces `mint.py` per ADR-017 strangler-fig step 7.
//! Foundation scaffold; argument parsing is sketched to lock down the
//! shape, but no business logic runs. Observability is wired up per
//! ADR-022 — a `request_id` root span opens at process start so any
//! future business logic emits inherit it automatically.

use clap::Parser;

use part_registry_domain::RequestId;
use part_registry_observability::{
    cli_scaffold_operator, init, set_current_operator, AuditSinkHandle, ObservabilityConfig,
};

#[derive(Parser, Debug)]
#[command(name = "mint", about = "Mint new part IDs into the registry")]
struct Args {
    /// Number of part IDs to mint.
    #[arg(short = 'n', long, default_value_t = 1)]
    count: u32,

    /// Subtype prefix (e.g. PT100, RH-1). Future ADR-012 enforced.
    #[arg(long)]
    subtype: Option<String>,
}

fn main() {
    let _args = Args::parse();

    // Observability init (ADR-022). Audit-CSV is disabled in the
    // scaffold until #29's `Repository` adapter is wired through
    // config (ADR-021); business logic per #32 lands the wiring.
    let cfg = ObservabilityConfig {
        audit_csv: false,
        ..ObservabilityConfig::cli_defaults()
    };
    let _ = init(&cfg, AuditSinkHandle::disabled());

    set_current_operator(cli_scaffold_operator());

    let rid = RequestId::new();
    let span = tracing::info_span!("cli.mint", request_id = %rid);
    let _g = span.enter();

    tracing::info!(request_id = %rid, "foundation scaffold; not yet implemented");
    eprintln!("foundation scaffold; not yet implemented");
    std::process::exit(2);
}
