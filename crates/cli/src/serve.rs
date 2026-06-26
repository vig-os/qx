//! `qx serve` — the HTTP shell per ADR-030 §2 (feature `serve`).
//!
//! One POST endpoint speaks the command protocol — the same
//! `Request`/`Response` JSON every other shell uses — plus a health
//! probe and (optionally) the static webapp bundle:
//!
//! - `POST /api/dispatch` — body: protocol `Request`; reply: protocol
//!   `Response` (HTTP 200 either way — the protocol envelope carries
//!   ok/error; transport-level failures are the only non-200s).
//! - `GET  /healthz` — liveness.
//! - `GET  /*` — the webapp bundle when `--static-dir` is given
//!   (SPA fallback to `index.html`).
//!
//! Dispatch is synchronous (port I/O may block); each request runs on
//! the blocking pool so the async accept loop stays responsive.
//!
//! MCP-over-HTTP (ADR-030 §2) mounts alongside this router when the
//! `mcp` feature lands.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};

use qx_app::{dispatch, AppContext, ErrorKind, Request, Response};

/// Build the protocol router over a shared [`AppContext`].
pub fn router(ctx: Arc<AppContext>) -> Router {
    Router::new()
        .route("/api/dispatch", post(dispatch_handler))
        .route("/healthz", get(|| async { "ok" }))
        .with_state(ctx)
}

async fn dispatch_handler(
    State(ctx): State<Arc<AppContext>>,
    Json(req): Json<Request>,
) -> Json<Response> {
    let resp = tokio::task::spawn_blocking(move || dispatch(&ctx, req))
        .await
        .unwrap_or_else(|e| {
            Response::error(ErrorKind::Backend, format!("dispatch task failed: {e}"))
        });
    Json(resp)
}

/// Run the server until ctrl-c. `static_dir`, when given, serves the
/// webapp bundle with SPA fallback.
pub fn run(
    ctx: AppContext,
    addr: SocketAddr,
    static_dir: Option<PathBuf>,
) -> Result<(), crate::CliError> {
    let ctx = Arc::new(ctx);
    let mut app = router(ctx);
    if let Some(dir) = static_dir {
        let index = dir.join("index.html");
        app = app.fallback_service(
            tower_http::services::ServeDir::new(&dir)
                .fallback(tower_http::services::ServeFile::new(index)),
        );
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| crate::CliError::Other(format!("tokio runtime: {e}")))?;
    rt.block_on(async move {
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| crate::CliError::Other(format!("bind {addr}: {e}")))?;
        tracing::info!(%addr, "qx serve listening");
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = tokio::signal::ctrl_c().await;
            })
            .await
            .map_err(|e| crate::CliError::Other(format!("serve: {e}")))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use axum::body::Body;
    use axum::http::{Request as HttpRequest, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use qx_domain::{
        AuditEntry, Hash, IdentitySource, Operator, OperatorId, Part, PartId, PartStatus,
        PrintEvent, Proposal, ProposalRef, ProposalStatus,
    };
    use qx_identity::{Capabilities, IdentityError, IdentityProvider};
    use qx_storage::{AuditFilter, PartFilter, PrintEventFilter, RepoError, Repository};
    use qx_transport::{ProposalError, ProposalSink};

    struct MemRepo(Mutex<Vec<Part>>);
    impl Repository for MemRepo {
        fn get_part(&self, id: &PartId) -> Result<Option<Part>, RepoError> {
            Ok(self
                .0
                .lock()
                .expect("lock")
                .iter()
                .find(|p| &p.id == id)
                .cloned())
        }
        fn list_parts(&self, _f: &PartFilter) -> Result<Vec<Part>, RepoError> {
            Ok(self.0.lock().expect("lock").clone())
        }
        fn list_audit_events(&self, _f: &AuditFilter) -> Result<Vec<AuditEntry>, RepoError> {
            Ok(Vec::new())
        }
        fn list_print_events(&self, _f: &PrintEventFilter) -> Result<Vec<PrintEvent>, RepoError> {
            Ok(Vec::new())
        }
        fn append_audit_event(&self, _e: AuditEntry) -> Result<(), RepoError> {
            Ok(())
        }
        fn append_print_event(&self, _e: PrintEvent) -> Result<(), RepoError> {
            Ok(())
        }
        fn snapshot_hash(&self) -> Result<Hash, RepoError> {
            Ok(Hash("mem".into()))
        }
    }

    struct NullSink;
    impl ProposalSink for NullSink {
        fn submit(&self, _p: Proposal) -> Result<ProposalRef, ProposalError> {
            Ok(ProposalRef {
                url: "mem://1".into(),
                local_id: None,
                adapter: "mem".into(),
            })
        }
        fn status(&self, _r: &ProposalRef) -> Result<ProposalStatus, ProposalError> {
            Ok(ProposalStatus::Open)
        }
    }

    struct TestIdentity;
    impl IdentityProvider for TestIdentity {
        fn current(&self) -> Result<Operator, IdentityError> {
            Ok(Operator {
                id: OperatorId("test:op".into()),
                display_name: "Test".into(),
                source: IdentitySource::GitConfig,
                verified_at: None,
                claims: BTreeMap::new(),
                pubkey: None,
            })
        }
        fn refresh(&self) -> Result<Operator, IdentityError> {
            self.current()
        }
        fn capabilities(&self, _o: &Operator) -> Capabilities {
            Capabilities::default()
        }
    }

    fn test_ctx() -> Arc<AppContext> {
        let part = Part {
            id: PartId::new("23456789ABCDEF").expect("id"),
            status: PartStatus::Unbound,
            minted_at: time::OffsetDateTime::UNIX_EPOCH,
            batch: None,
            bound_at: None,
            type_: None,
            description: None,
            vendor: None,
            part_number: None,
            location: None,
            notes: None,
            minted_by: None,
            bound_by: None,
            last_edited_at: None,
            last_edited_by: None,
            components: Vec::new(),
            manufacturer_id: None,
            metadata: std::collections::BTreeMap::new(),
            signatures: Vec::new(),
            chain_hash: None,
        };
        Arc::new(AppContext {
            repo: Arc::new(MemRepo(Mutex::new(vec![part]))),
            identity: Box::new(TestIdentity),
            sink: Box::new(NullSink),
            registry_name: "test/serve".into(),
            contract: None,
        })
    }

    async fn post_dispatch(body: &str) -> (StatusCode, serde_json::Value) {
        let app = router(test_ctx());
        let resp = app
            .oneshot(
                HttpRequest::post("/api/dispatch")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_owned()))
                    .expect("request"),
            )
            .await
            .expect("response");
        let status = resp.status();
        let bytes = resp.into_body().collect().await.expect("body").to_bytes();
        let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, json)
    }

    #[tokio::test]
    async fn healthz_is_ok() {
        let app = router(test_ctx());
        let resp = app
            .oneshot(
                HttpRequest::get("/healthz")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn dispatch_list_round_trips_the_protocol() {
        let (status, json) = post_dispatch(r#"{"op":"List","collection":"parts"}"#).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["ok"], serde_json::json!(true));
        assert_eq!(json["data"]["total"], serde_json::json!(1));
    }

    #[tokio::test]
    async fn dispatch_protocol_errors_stay_http_200() {
        let (status, json) = post_dispatch(r#"{"op":"List","collection":"vendors"}"#).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["ok"], serde_json::json!(false));
        assert_eq!(json["error"]["kind"], serde_json::json!("Unsupported"));
    }

    #[tokio::test]
    async fn malformed_body_is_a_4xx() {
        let (status, _) = post_dispatch("{not json").await;
        assert!(status.is_client_error());
    }
}
