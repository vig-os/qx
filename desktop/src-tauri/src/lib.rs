//! `part-registry-desktop` — the Tauri v2 desktop shell per ADR-030 §3.
//!
//! The webview loads the webapp bundle (`webapp/dist`; the Vite dev
//! server in dev) and its transport calls the one Tauri command
//! [`dispatch`], which runs `part_registry_app::dispatch` **in
//! process** — no HTTP hop. Wiring mirrors `pr serve` in
//! `crates/cli/src/bin/pr.rs`: live GitHub PR sink when a token
//! resolves, dry-run capture otherwise so read-only use stays
//! token-free.

#![forbid(unsafe_code)]

use std::sync::Arc;

use part_registry_app::{AppContext, ErrorKind, Request, Response};
use part_registry_cli::{init_observability, DryRunTarget, Wiring};
use part_registry_config::Config;
use part_registry_observability::ObservabilityConfig;

/// The one command behind the webview transport's
/// `invoke("dispatch", …)`.
///
/// Always resolves with the protocol envelope: a malformed request
/// maps to the `BadRequest` envelope — the same taxonomy the serve
/// shell uses — never a rejection (the `Result` is the Tauri macro's
/// required shape for async commands borrowing `State`; the error arm
/// is unreachable). Async so the blocking port I/O inside the app
/// layer stays off the webview's event loop, mirroring the serve
/// shell's `spawn_blocking`.
#[tauri::command]
async fn dispatch(
    state: tauri::State<'_, Arc<AppContext>>,
    request: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let resp = match serde_json::from_value::<Request>(request) {
        Ok(req) => {
            let ctx = Arc::clone(state.inner());
            tauri::async_runtime::spawn_blocking(move || part_registry_app::dispatch(&ctx, req))
                .await
                .unwrap_or_else(|e| {
                    Response::error(ErrorKind::Backend, format!("dispatch task failed: {e}"))
                })
        }
        Err(e) => Response::error(ErrorKind::BadRequest, format!("malformed request: {e}")),
    };
    Ok(envelope(resp))
}

/// Encode the protocol envelope. Serializing a [`Response`] cannot
/// realistically fail; if it ever does, a hand-built `Backend`
/// envelope is still better than rejecting the invoke.
fn envelope(resp: Response) -> serde_json::Value {
    serde_json::to_value(&resp).unwrap_or_else(|e| {
        serde_json::json!({
            "ok": false,
            "error": { "kind": "Backend", "message": format!("encode response: {e}") }
        })
    })
}

/// Build the production wiring once, exactly like `serve_cmd` in
/// `pr.rs`: prefer the live sink — the desktop is a write-capable
/// host — and fall back to dry-run capture with a loud notice when no
/// token resolves, so read-only use still works.
fn build_context() -> Result<AppContext, String> {
    let cfg = Config::from_env().map_err(|e| format!("config: {e}"))?;
    let wiring = match Wiring::from_config(&cfg, None) {
        Ok(w) => w,
        Err(_) => {
            // Pre-tracing startup notice — stderr is the only channel
            // before init_observability runs.
            let notice = "part-registry-desktop: no GitHub token resolved — mutations \
                 will be captured as dry-run JSON on stdout, not submitted. Set \
                 PART_REGISTRY__TRANSPORT__GITHUB_TOKEN (or GITHUB_TOKEN) for live \
                 proposals.";
            eprintln!("{notice}"); // guardrails-ok
            Wiring::from_config(&cfg, Some(DryRunTarget::Stdout))
                .map_err(|e| format!("wiring: {e}"))?
        }
    };
    let obs_cfg = ObservabilityConfig {
        log_level: cfg.observability.log_level.clone(),
        stdout_json: cfg.observability.stdout_json,
        stderr_human: cfg.observability.stderr_human,
        audit_csv: cfg.observability.audit_csv,
    };
    let _ = init_observability(&obs_cfg, wiring.repo.clone());
    let registry_name = part_registry_config::parse_owner_repo(&cfg.repo.data_repo_url)
        .map(|(o, r)| format!("{o}/{r}"))
        .unwrap_or_else(|_| cfg.repo.data_repo_url.clone());
    Ok(AppContext {
        repo: wiring.repo,
        identity: wiring.identity,
        sink: wiring.sink,
        registry_name,
    })
}

/// Build the shared [`AppContext`] and run the Tauri app.
pub fn run() {
    let ctx = match build_context() {
        Ok(ctx) => Arc::new(ctx),
        Err(e) => {
            // Startup failure precedes tracing init — stderr by design.
            eprintln!("part-registry-desktop: startup failed: {e}"); // guardrails-ok
            std::process::exit(2);
        }
    };
    tauri::Builder::default()
        .manage(ctx)
        .invoke_handler(tauri::generate_handler![dispatch])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            // Runtime teardown failure — the webview (and its console)
            // is gone; stderr is what remains.
            eprintln!("part-registry-desktop: tauri runtime failed: {e}"); // guardrails-ok
            std::process::exit(1);
        });
}
