//! `qx mcp` — stdio MCP server over the command protocol (ADR-030 §2,
//! feature `mcp`).
//!
//! One MCP tool per protocol op family. The mapping is structural, not
//! hand-mirrored: `call_tool` injects the op tag into the tool's
//! arguments object and serde-parses the protocol [`Request`] — so the
//! tool surface cannot drift from the protocol for *parsing* (the
//! advertised JSON schemas are documentation; serde is the validator).
//!
//! Results carry the protocol envelope verbatim as JSON text: an Ok
//! envelope returns as a success result, an error envelope as an MCP
//! error result whose content is still the envelope — agents handle
//! one shape everywhere, exactly like every other shell.
//!
//! Agent access is gated by the same identity/policy as any shell
//! (ADR-030/034): no special path.

use std::sync::Arc;

use rmcp::model::*;
use rmcp::service::RequestContext;
use rmcp::{RoleServer, ServerHandler, ServiceExt};

use qx_app::{dispatch, AppContext, Request, Response};

/// `(tool name, protocol op tag, description)` — one row per op family.
const OPS: &[(&str, &str, &str)] = &[
    (
        "resolve",
        "Resolve",
        "Resolve one entity by id (full 14-char, 8-char human prefix, or scheme:value). Args: {id}",
    ),
    (
        "list",
        "List",
        "Query a collection. Args: {collection, filter?: {status?, kind?, text?, fields?}, sort?: [{field, dir}], page?: {offset, limit}}",
    ),
    (
        "count",
        "Count",
        "Single-field group-by count. Args: {collection, by, filter?}",
    ),
    (
        "describe",
        "Describe",
        "Registry descriptors — what collections/fields/lifecycles exist and how ids are minted. Args: {collection?}",
    ),
    (
        "create",
        "Create",
        "Mint n fresh part ids (submits a proposal). Args: {collection, n?}",
    ),
    (
        "edit",
        "Edit",
        "Status-preserving field edit on a bound entity (submits a proposal). Args: {collection, id, fields}",
    ),
    (
        "transition",
        "Transition",
        "Lifecycle transition; bind = {to: \"bound\", fields}. Args: {collection, id, to, fields?}",
    ),
    (
        "print",
        "Print",
        "Render label SVGs for a selection. Args: {collection, selection: {ids: [..]} | {filter: {..}}, options?: {layout, size_mm, chars, micro, copies, log}}",
    ),
    (
        "export",
        "Export",
        "Flat export of a collection (generated artifact). Args: {collection, format: \"csv\"}",
    ),
    (
        "poll_proposal",
        "PollProposal",
        "Status of a submitted proposal. Args: {proposal: {url, local_id, adapter}}",
    ),
    ("whoami", "Whoami", "Current operator identity. Args: {}"),
];

fn op_for_tool(tool: &str) -> Option<&'static str> {
    OPS.iter()
        .find(|(t, _, _)| *t == tool)
        .map(|(_, op, _)| *op)
}

/// Build the protocol `Request` from a tool call: inject the op tag
/// into the args object and let serde validate. Pure — unit-tested
/// without an MCP transport.
fn tool_call_to_request(
    tool: &str,
    mut args: serde_json::Map<String, serde_json::Value>,
) -> Result<Request, String> {
    let op = op_for_tool(tool).ok_or_else(|| {
        format!(
            "unknown tool {tool:?}; available: {}",
            OPS.iter()
                .map(|(t, _, _)| *t)
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;
    args.insert("op".into(), serde_json::Value::String(op.into()));
    serde_json::from_value(serde_json::Value::Object(args)).map_err(|e| format!("args: {e}"))
}

/// Render the protocol envelope into an MCP result. Pure.
fn response_to_result(resp: &Response) -> CallToolResult {
    let text = serde_json::to_string(resp).unwrap_or_else(|e| {
        format!("{{\"ok\":false,\"error\":{{\"kind\":\"Backend\",\"message\":\"encode: {e}\"}}}}")
    });
    if resp.is_ok() {
        CallToolResult::success(vec![Content::text(text)])
    } else {
        CallToolResult::error(vec![Content::text(text)])
    }
}

/// Advertised input schema per op — documentation-grade (serde is the
/// validator); permissive so schema lag never blocks a valid request.
fn tool_schema(desc: &str) -> Arc<serde_json::Map<String, serde_json::Value>> {
    let schema = serde_json::json!({
        "type": "object",
        "description": desc,
        "additionalProperties": true,
    });
    Arc::new(schema.as_object().cloned().unwrap_or_default())
}

pub struct RegistryMcp {
    ctx: Arc<AppContext>,
}

impl RegistryMcp {
    pub fn new(ctx: Arc<AppContext>) -> Self {
        Self { ctx }
    }
}

impl ServerHandler for RegistryMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("qx", env!("CARGO_PKG_VERSION")))
            .with_instructions(format!(
                "qx command protocol over MCP (registry: {}). Reads \
                 (resolve/list/count/describe/export) are safe; mutations \
                 (create/edit/transition) submit PR proposals through the same \
                 policy gates as every other shell — they do not change the \
                 registry directly. Results are the protocol envelope: \
                 {{ok:true,data}} or {{ok:false,error:{{kind,message}}}}.",
                self.ctx.registry_name
            ))
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, rmcp::ErrorData> {
        let tools = OPS
            .iter()
            .map(|(name, _, desc)| Tool::new(*name, *desc, tool_schema(desc)))
            .collect();
        Ok(ListToolsResult {
            tools,
            meta: None,
            next_cursor: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let args = request.arguments.unwrap_or_default();
        let req = match tool_call_to_request(&request.name, args) {
            Ok(r) => r,
            Err(msg) => {
                let envelope = serde_json::json!({
                    "ok": false,
                    "error": { "kind": "BadRequest", "message": msg }
                });
                return Ok(CallToolResult::error(vec![Content::text(
                    envelope.to_string(),
                )]));
            }
        };
        let ctx = self.ctx.clone();
        let resp = tokio::task::spawn_blocking(move || dispatch(&ctx, req))
            .await
            .unwrap_or_else(|e| {
                Response::error(
                    qx_app::ErrorKind::Backend,
                    format!("dispatch task failed: {e}"),
                )
            });
        Ok(response_to_result(&resp))
    }
}

/// Run the stdio MCP server until the transport closes.
pub fn run(ctx: AppContext) -> Result<(), crate::CliError> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| crate::CliError::Other(format!("tokio runtime: {e}")))?;
    rt.block_on(async move {
        let service = RegistryMcp::new(Arc::new(ctx))
            .serve(rmcp::transport::stdio())
            .await
            .map_err(|e| crate::CliError::Other(format!("mcp serve: {e}")))?;
        service
            .waiting()
            .await
            .map_err(|e| crate::CliError::Other(format!("mcp wait: {e}")))?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn args(v: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
        v.as_object().cloned().expect("object")
    }

    #[test]
    fn every_op_family_has_a_tool() {
        // The protocol families exposed by every shell (ADR-030 §8
        // parity at the op-family grain).
        for op in [
            "Resolve",
            "List",
            "Count",
            "Describe",
            "Create",
            "Edit",
            "Transition",
            "Print",
            "Export",
            "PollProposal",
            "Whoami",
        ] {
            assert!(
                OPS.iter().any(|(_, o, _)| *o == op),
                "op family {op} has no MCP tool"
            );
        }
    }

    #[test]
    fn tool_args_parse_into_protocol_requests() {
        let req =
            tool_call_to_request("resolve", args(json!({"id": "23456789ABCDEF"}))).expect("parses");
        assert_eq!(
            req,
            Request::Resolve {
                id: "23456789ABCDEF".into()
            }
        );

        let req = tool_call_to_request(
            "list",
            args(json!({"collection":"parts","filter":{"status":"bound"}})),
        )
        .expect("parses");
        match req {
            Request::List {
                collection, filter, ..
            } => {
                assert_eq!(collection, "parts");
                assert_eq!(filter.status.as_deref(), Some("bound"));
            }
            other => panic!("wrong request: {other:?}"),
        }

        let req = tool_call_to_request(
            "transition",
            args(json!({"collection":"parts","id":"X","to":"bound","fields":{"type":"valve"}})),
        )
        .expect("parses");
        match req {
            Request::Transition { to, fields, .. } => {
                assert_eq!(to, "bound");
                assert_eq!(fields["type"], "valve");
            }
            other => panic!("wrong request: {other:?}"),
        }
    }

    #[test]
    fn unknown_tool_and_bad_args_are_clean_errors() {
        let e = tool_call_to_request("nuke", args(json!({}))).expect_err("unknown tool");
        assert!(e.contains("available"));

        let e = tool_call_to_request("resolve", args(json!({"wrong": 1}))).expect_err("bad args");
        assert!(e.contains("args:"));
    }

    #[test]
    fn envelope_maps_to_mcp_success_and_error() {
        let ok = Response::ok(json!({"x": 1}));
        let r = response_to_result(&ok);
        assert_ne!(r.is_error, Some(true));

        let err = Response::error(qx_app::ErrorKind::NotFound, "nope");
        let r = response_to_result(&err);
        assert_eq!(r.is_error, Some(true));
    }
}
