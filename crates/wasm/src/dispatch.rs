//! Browser dispatch — the serverless shell over `app::dispatch` per
//! ADR-030 §3.
//!
//! The FE fetches the registry snapshot itself (CSV or JSONL text),
//! hands it to [`registry_open`], and from then on speaks the one
//! command protocol via [`registry_dispatch`] — the same
//! `Request`/`Response` JSON every other shell uses.
//!
//! ## Browser port wiring (honest capabilities)
//!
//! - **Repository**: an in-memory snapshot of the fetched registry —
//!   reads (`Resolve`/`List`/`Count`/`Describe`/`Export`) are fully
//!   served; audit/print appends accumulate in memory for the session
//!   (the FE can read them back; durable submission is the proposal
//!   path).
//! - **Identity**: anonymous until the FE calls
//!   [`registry_set_operator`] (post-OAuth); until then mutating ops
//!   return the protocol `Auth` error. The operator is recorded as an
//!   unverified claim (`IdentitySource::OfflineClaim`) — browser OAuth
//!   verification rides the ADR-020 device-flow/serve work.
//! - **ProposalSink**: returns a protocol `Backend` error directing to
//!   the OAuth + PR wiring (ADR-019/020) — mutations *classify and
//!   validate* in-browser but cannot yet open PRs from the serverless
//!   deploy. `qx serve` (ADR-030 §2) is the write-capable host.
//!
//! No panics on caller input: every failure maps into the protocol
//! error envelope so the FE handles one shape.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use wasm_bindgen::prelude::*;

use qx_app::{dispatch as app_dispatch, AppContext, ErrorKind, Request, Response};
use qx_domain::{
    AuditEntry, Hash, IdentitySource, Operator, OperatorId, Part, PartId, PartStatus, PrintEvent,
    Proposal, ProposalRef, ProposalStatus,
};
use qx_identity::{Capabilities, IdentityError, IdentityProvider};
use qx_storage::{AuditFilter, PartFilter, PrintEventFilter, RepoError, Repository};
use qx_transport::{ProposalError, ProposalSink};

// -------------------------------------------------------------------
// Browser ports
// -------------------------------------------------------------------

/// In-memory registry snapshot (the FE fetched the bytes; this is the
/// browser's `Repository`).
struct SnapshotRepo {
    parts: Vec<Part>,
    audit: Mutex<Vec<AuditEntry>>,
    prints: Mutex<Vec<PrintEvent>>,
}

impl Repository for SnapshotRepo {
    fn get_part(&self, id: &PartId) -> Result<Option<Part>, RepoError> {
        Ok(self.parts.iter().find(|p| &p.id == id).cloned())
    }
    fn list_parts(&self, _filter: &PartFilter) -> Result<Vec<Part>, RepoError> {
        Ok(self.parts.clone())
    }
    fn list_audit_events(&self, _filter: &AuditFilter) -> Result<Vec<AuditEntry>, RepoError> {
        Ok(self.audit.lock().expect("audit lock").clone())
    }
    fn list_print_events(&self, _filter: &PrintEventFilter) -> Result<Vec<PrintEvent>, RepoError> {
        Ok(self.prints.lock().expect("prints lock").clone())
    }
    fn append_audit_event(&self, ev: AuditEntry) -> Result<(), RepoError> {
        self.audit.lock().expect("audit lock").push(ev);
        Ok(())
    }
    fn append_print_event(&self, ev: PrintEvent) -> Result<(), RepoError> {
        self.prints.lock().expect("prints lock").push(ev);
        Ok(())
    }
    fn snapshot_hash(&self) -> Result<Hash, RepoError> {
        // The browser holds a point-in-time snapshot; the durable hash
        // is the data repo's (ADR-024). Identify the session snapshot
        // by its row count + first/last id so the FE can display
        // staleness hints without pretending to be the repo hash.
        let first = self.parts.first().map(|p| p.id.as_str()).unwrap_or("");
        let last = self.parts.last().map(|p| p.id.as_str()).unwrap_or("");
        Ok(Hash(format!(
            "snapshot:{}:{first}:{last}",
            self.parts.len()
        )))
    }
}

/// Identity = whatever the FE asserted via [`registry_set_operator`].
struct BrowserIdentity {
    operator: Mutex<Option<Operator>>,
}

impl IdentityProvider for BrowserIdentity {
    fn current(&self) -> Result<Operator, IdentityError> {
        self.operator
            .lock()
            .expect("operator lock")
            .clone()
            .ok_or_else(|| {
                IdentityError::NoIdentity(
                    "no operator set — sign in (registry_set_operator) before mutating".into(),
                )
            })
    }
    fn refresh(&self) -> Result<Operator, IdentityError> {
        self.current()
    }
    fn capabilities(&self, _op: &Operator) -> Capabilities {
        Capabilities::default()
    }
}

/// Browser proposal sink — honest about what the serverless deploy
/// cannot do yet.
struct BrowserSink;

impl ProposalSink for BrowserSink {
    fn submit(&self, _proposal: Proposal) -> Result<ProposalRef, ProposalError> {
        Err(ProposalError::Backend(
            "browser submission lands with the OAuth + PR wiring (ADR-019/020); \
             use `qx serve` or the CLI for writes today"
                .to_owned()
                .into(),
        ))
    }
    fn status(&self, _r: &ProposalRef) -> Result<ProposalStatus, ProposalError> {
        Err(ProposalError::Backend(
            "proposal polling from the browser lands with the OAuth wiring"
                .to_owned()
                .into(),
        ))
    }
}

// -------------------------------------------------------------------
// Snapshot parsing (CSV per the registry contract; JSONL per ADR-035)
// -------------------------------------------------------------------

fn parse_timestamp(s: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(s, &Rfc3339).ok()
}

/// Parse `registry.csv` text (header-addressed, column order free).
fn parse_csv(text: &str) -> Result<Vec<Part>, String> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(text.as_bytes());
    let header: Vec<String> = rdr
        .headers()
        .map_err(|e| format!("csv header: {e}"))?
        .iter()
        .map(ToOwned::to_owned)
        .collect();
    let mut parts = Vec::new();
    for (i, rec) in rdr.records().enumerate() {
        let line = i + 2;
        let rec = rec.map_err(|e| format!("csv line {line}: {e}"))?;
        let mut row: BTreeMap<&str, &str> = BTreeMap::new();
        for (k, v) in header.iter().zip(rec.iter()) {
            if !v.is_empty() {
                row.insert(k.as_str(), v);
            }
        }
        let id = row
            .get("id")
            .ok_or_else(|| format!("csv line {line}: missing id"))
            .and_then(|s| PartId::new(*s).map_err(|e| format!("csv line {line}: {e}")))?;
        let status = row
            .get("status")
            .ok_or_else(|| format!("csv line {line}: missing status"))
            .and_then(|s| {
                s.parse::<PartStatus>()
                    .map_err(|e| format!("csv line {line}: {e}"))
            })?;
        let minted_at = row
            .get("minted_at")
            .and_then(|s| parse_timestamp(s))
            .ok_or_else(|| format!("csv line {line}: missing/invalid minted_at"))?;
        let opt = |k: &str| row.get(k).map(|s| (*s).to_owned());
        parts.push(Part {
            id,
            status,
            minted_at,
            batch: opt("batch"),
            bound_at: row.get("bound_at").and_then(|s| parse_timestamp(s)),
            type_: opt("type"),
            description: opt("description"),
            vendor: opt("vendor"),
            part_number: opt("part_number"),
            location: opt("location"),
            notes: opt("notes"),
            minted_by: opt("minted_by"),
            bound_by: opt("bound_by"),
            last_edited_at: opt("last_edited_at"),
            last_edited_by: opt("last_edited_by"),
            components: Vec::new(),
            manufacturer_id: opt("manufacturer_id"),
            metadata: std::collections::BTreeMap::new(),
            signatures: Vec::new(),
            chain_hash: None,
        });
    }
    Ok(parts)
}

/// Parse `collections/parts.jsonl` text (one serde `Part` per line).
fn parse_jsonl(text: &str) -> Result<Vec<Part>, String> {
    let mut parts = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let part: Part =
            serde_json::from_str(line).map_err(|e| format!("jsonl line {}: {e}", i + 1))?;
        parts.push(part);
    }
    Ok(parts)
}

// -------------------------------------------------------------------
// The browser shell state + entry points
// -------------------------------------------------------------------

thread_local! {
    static CTX: RefCell<Option<AppContext>> = const { RefCell::new(None) };
    static IDENTITY: RefCell<Option<Arc<BrowserIdentity>>> = const { RefCell::new(None) };
}

/// Identity handle shared between the context and
/// `registry_set_operator` (the context owns a `Box<dyn>`; this shim
/// lets both point at one mutable slot).
struct SharedIdentity(Arc<BrowserIdentity>);

impl IdentityProvider for SharedIdentity {
    fn current(&self) -> Result<Operator, IdentityError> {
        self.0.current()
    }
    fn refresh(&self) -> Result<Operator, IdentityError> {
        self.0.refresh()
    }
    fn capabilities(&self, op: &Operator) -> Capabilities {
        self.0.capabilities(op)
    }
}

fn open_impl(format: &str, text: &str, registry_name: &str) -> Result<usize, String> {
    let parts = match format {
        "csv" => parse_csv(text)?,
        "jsonl" => parse_jsonl(text)?,
        other => return Err(format!("unknown snapshot format {other:?} (csv | jsonl)")),
    };
    let n = parts.len();
    let identity = Arc::new(BrowserIdentity {
        operator: Mutex::new(None),
    });
    let ctx = AppContext {
        repo: Arc::new(SnapshotRepo {
            parts,
            audit: Mutex::new(Vec::new()),
            prints: Mutex::new(Vec::new()),
        }),
        identity: Box::new(SharedIdentity(identity.clone())),
        sink: Box::new(BrowserSink),
        registry_name: registry_name.to_owned(),
    };
    CTX.with(|c| *c.borrow_mut() = Some(ctx));
    IDENTITY.with(|i| *i.borrow_mut() = Some(identity));
    Ok(n)
}

fn dispatch_impl(request_json: &str) -> String {
    let response = match serde_json::from_str::<Request>(request_json) {
        Ok(req) => CTX.with(|c| match &*c.borrow() {
            Some(ctx) => app_dispatch(ctx, req),
            None => Response::error(
                ErrorKind::Backend,
                "no registry open — call registry_open(format, text, name) first",
            ),
        }),
        Err(e) => Response::error(ErrorKind::BadRequest, format!("request parse: {e}")),
    };
    serde_json::to_string(&response).unwrap_or_else(|e| {
        format!(
            "{{\"ok\":false,\"error\":{{\"kind\":\"Backend\",\"message\":\"encode response: {e}\"}}}}"
        )
    })
}

fn set_operator_impl(id: &str, display_name: &str) {
    let op = Operator {
        id: OperatorId(id.to_owned()),
        display_name: display_name.to_owned(),
        source: IdentitySource::OfflineClaim,
        verified_at: None,
        claims: BTreeMap::new(),
        pubkey: None,
    };
    IDENTITY.with(|i| {
        if let Some(identity) = &*i.borrow() {
            *identity.operator.lock().expect("operator lock") = Some(op);
        }
    });
}

/// Open a registry snapshot. `format` is `"csv"` (today's
/// `registry.csv`) or `"jsonl"` (`collections/parts.jsonl`, ADR-035).
/// Returns the number of parts loaded; throws on a malformed snapshot.
#[wasm_bindgen]
pub fn registry_open(format: &str, text: &str, registry_name: &str) -> Result<u32, JsError> {
    open_impl(format, text, registry_name)
        .map(|n| n as u32)
        .map_err(|e| JsError::new(&e))
}

/// Dispatch one protocol `Request` (JSON string) and return the
/// protocol `Response` (JSON string). Never throws — errors come back
/// in the protocol envelope.
#[wasm_bindgen]
pub fn registry_dispatch(request_json: &str) -> String {
    dispatch_impl(request_json)
}

/// Assert the current operator (post-OAuth identity hand-off from the
/// FE). Recorded as an unverified claim until ADR-020's browser
/// verification lands.
#[wasm_bindgen]
pub fn registry_set_operator(id: &str, display_name: &str) {
    set_operator_impl(id, display_name);
}

// -------------------------------------------------------------------
// Native tests — same code paths the browser runs
// -------------------------------------------------------------------

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    const CSV: &str = "\
id,status,minted_at,batch,bound_at,type,description,vendor,part_number,location,notes
23456789ABCDEF,bound,2026-05-10T12:00:00Z,B-2026-05-10-1200,2026-05-11T09:30:00Z,PT100 sensor,,Acme,,lab-1,
23456789GHJKMN,unbound,2026-05-10T12:00:00Z,B-2026-05-10-1200,,,,,,,
";

    fn dispatch_value(req: Value) -> Value {
        let out = dispatch_impl(&req.to_string());
        serde_json::from_str(&out).expect("response is JSON")
    }

    #[test]
    fn open_csv_then_resolve_and_list() {
        let n = open_impl("csv", CSV, "test/registry").expect("opens");
        assert_eq!(n, 2);

        let r = dispatch_value(json!({"op":"Resolve","id":"23456789ABCDEF"}));
        assert_eq!(r["ok"], json!(true));
        assert_eq!(r["data"]["status"], json!("bound"));
        assert_eq!(r["data"]["fields"]["type"], json!("PT100 sensor"));

        let r =
            dispatch_value(json!({"op":"List","collection":"parts","filter":{"status":"unbound"}}));
        assert_eq!(r["data"]["total"], json!(1));

        let r = dispatch_value(json!({"op":"Describe"}));
        assert_eq!(r["data"]["name"], json!("test/registry"));
    }

    #[test]
    fn open_jsonl_roundtrip() {
        let parts = parse_csv(CSV).expect("csv parses");
        let jsonl: String = parts
            .iter()
            .map(|p| serde_json::to_string(p).expect("part encodes"))
            .collect::<Vec<_>>()
            .join("\n");
        let n = open_impl("jsonl", &jsonl, "test/registry").expect("opens");
        assert_eq!(n, 2);
        let r = dispatch_value(json!({"op":"Count","collection":"parts","by":"status"}));
        assert_eq!(r["data"]["counts"]["bound"], json!(1));
    }

    #[test]
    fn print_renders_in_snapshot_mode() {
        open_impl("csv", CSV, "test/registry").expect("opens");
        set_operator_impl("web:tester", "Web Tester");
        let r = dispatch_value(json!({
            "op":"Print","collection":"parts",
            "selection":{"ids":["23456789ABCDEF"]},
            "options":{"layout":"horz","chars":"44"}
        }));
        assert_eq!(r["ok"], json!(true));
        assert!(r["data"]["labels"][0]["svg"]
            .as_str()
            .expect("svg")
            .contains("<svg"));
    }

    #[test]
    fn mutations_without_operator_return_auth_error() {
        open_impl("csv", CSV, "test/registry").expect("opens");
        let r = dispatch_value(json!({"op":"Create","collection":"parts","n":1}));
        assert_eq!(r["ok"], json!(false));
        assert_eq!(r["error"]["kind"], json!("Auth"));
    }

    #[test]
    fn mutations_with_operator_hit_the_honest_sink_error() {
        open_impl("csv", CSV, "test/registry").expect("opens");
        set_operator_impl("web:tester", "Web Tester");
        let r = dispatch_value(json!({"op":"Create","collection":"parts","n":1}));
        assert_eq!(r["ok"], json!(false));
        assert_eq!(r["error"]["kind"], json!("Backend"));
        assert!(r["error"]["message"]
            .as_str()
            .expect("msg")
            .contains("OAuth"));
    }

    #[test]
    fn dispatch_without_open_is_protocol_error() {
        CTX.with(|c| *c.borrow_mut() = None);
        let r = dispatch_value(json!({"op":"Whoami"}));
        assert_eq!(r["ok"], json!(false));
        assert!(r["error"]["message"]
            .as_str()
            .expect("msg")
            .contains("registry_open"));
    }

    #[test]
    fn malformed_request_is_bad_request_not_panic() {
        open_impl("csv", CSV, "test/registry").expect("opens");
        let out = dispatch_impl("{\"op\":\"Nonsense\"}");
        let r: Value = serde_json::from_str(&out).expect("json");
        assert_eq!(r["error"]["kind"], json!("BadRequest"));
    }
}
