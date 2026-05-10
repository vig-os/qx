//! `part-registry-observability` — `tracing` setup + audit-CSV
//! subscriber + `request_id` propagation per ADR-022.
//!
//! Single `init(...)` call from each CLI binary and the WASM façade.
//! Foundation scaffold — the audit-CSV layer wires through
//! `Repository::append_audit_event` once `storage_csv_git` is fleshed
//! out (ADR-017 step 4 → step 5 audit layer).

#![forbid(unsafe_code)]

use thiserror::Error;
use tracing_subscriber::util::SubscriberInitExt;

#[derive(Debug, Error)]
pub enum InitError {
    #[error("subscriber already initialised: {0}")]
    AlreadyInit(String),
}

#[derive(Debug, Clone)]
pub struct ObservabilityConfig {
    pub log_level: String, // ADR-022 keeps this as a string per ADR-021's config schema
    pub stdout_json: bool,
    pub stderr_human: bool,
    pub audit_csv: bool,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            log_level: "info".into(),
            stdout_json: true,
            stderr_human: true,
            audit_csv: false, // mutating processes flip this to true
        }
    }
}

/// Initialise the global tracing subscriber. Foundation scaffold:
/// today this only attaches an empty registry so callers can wire
/// `init` into their startup paths without crashing. Real layers
/// (stdout JSON, stderr human, audit-CSV) land in the corresponding
/// strangler-fig PRs.
pub fn init(_cfg: &ObservabilityConfig) -> Result<(), InitError> {
    let registry = tracing_subscriber::registry();
    registry
        .try_init()
        .map_err(|e| InitError::AlreadyInit(e.to_string()))
}
