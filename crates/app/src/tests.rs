//! Engine tests over in-memory port fakes: dispatch is exercised
//! end-to-end (protocol JSON in → protocol JSON out) without touching
//! the filesystem, network, or git.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde_json::json;
use time::macros::datetime;

use part_registry_domain::{
    AuditEntry, Hash, IdentitySource, Operator, OperatorId, Part, PartId, PartStatus, PrintEvent,
    Proposal, ProposalRef, ProposalStatus,
};
use part_registry_identity::{Capabilities, IdentityError, IdentityProvider};
use part_registry_storage::{AuditFilter, PartFilter, PrintEventFilter, RepoError, Repository};
use part_registry_transport::{ProposalError, ProposalSink};

use crate::engine::{dispatch, AppContext};
use crate::protocol::{ErrorKind, Request, Response};

// -------------------------------------------------------------------
// Fakes
// -------------------------------------------------------------------

struct MemRepo {
    parts: Mutex<Vec<Part>>,
    audit: Mutex<Vec<AuditEntry>>,
    prints: Mutex<Vec<PrintEvent>>,
}

impl MemRepo {
    fn new(parts: Vec<Part>) -> Self {
        Self {
            parts: Mutex::new(parts),
            audit: Mutex::new(Vec::new()),
            prints: Mutex::new(Vec::new()),
        }
    }
}

impl Repository for MemRepo {
    fn get_part(&self, id: &PartId) -> Result<Option<Part>, RepoError> {
        Ok(self
            .parts
            .lock()
            .expect("lock")
            .iter()
            .find(|p| &p.id == id)
            .cloned())
    }
    fn list_parts(&self, _filter: &PartFilter) -> Result<Vec<Part>, RepoError> {
        Ok(self.parts.lock().expect("lock").clone())
    }
    fn list_audit_events(&self, _filter: &AuditFilter) -> Result<Vec<AuditEntry>, RepoError> {
        Ok(self.audit.lock().expect("lock").clone())
    }
    fn list_print_events(&self, _filter: &PrintEventFilter) -> Result<Vec<PrintEvent>, RepoError> {
        Ok(self.prints.lock().expect("lock").clone())
    }
    fn append_audit_event(&self, ev: AuditEntry) -> Result<(), RepoError> {
        self.audit.lock().expect("lock").push(ev);
        Ok(())
    }
    fn append_print_event(&self, ev: PrintEvent) -> Result<(), RepoError> {
        self.prints.lock().expect("lock").push(ev);
        Ok(())
    }
    fn snapshot_hash(&self) -> Result<Hash, RepoError> {
        Ok(Hash("mem".into()))
    }
}

struct MemSink {
    submitted: Arc<Mutex<Vec<Proposal>>>,
}

impl ProposalSink for MemSink {
    fn submit(&self, proposal: Proposal) -> Result<ProposalRef, ProposalError> {
        let n = {
            let mut g = self.submitted.lock().expect("lock");
            g.push(proposal);
            g.len()
        };
        Ok(ProposalRef {
            url: format!("mem://proposal/{n}"),
            local_id: Some(format!("{n}")),
            adapter: "mem".into(),
        })
    }
    fn status(&self, _r: &ProposalRef) -> Result<ProposalStatus, ProposalError> {
        Ok(ProposalStatus::Open)
    }
}

struct FixedIdentity;

impl IdentityProvider for FixedIdentity {
    fn current(&self) -> Result<Operator, IdentityError> {
        Ok(Operator {
            id: OperatorId("git:test-operator".into()),
            display_name: "Test Operator".into(),
            source: IdentitySource::GitConfig,
            verified_at: None,
            claims: BTreeMap::new(),
            pubkey: None,
        })
    }
    fn refresh(&self) -> Result<Operator, IdentityError> {
        self.current()
    }
    fn capabilities(&self, _op: &Operator) -> Capabilities {
        Capabilities::default()
    }
}

fn part(id: &str, status: PartStatus, type_: Option<&str>, vendor: Option<&str>) -> Part {
    Part {
        id: PartId::new(id).expect("valid test id"),
        status,
        minted_at: datetime!(2026-05-10 12:00 UTC),
        batch: Some("B-2026-05-10-1200".into()),
        bound_at: if status == PartStatus::Bound {
            Some(datetime!(2026-05-11 09:30 UTC))
        } else {
            None
        },
        type_: type_.map(Into::into),
        description: None,
        vendor: vendor.map(Into::into),
        part_number: None,
        location: None,
        notes: None,
        signatures: Vec::new(),
        chain_hash: None,
    }
}

fn ctx_with(parts: Vec<Part>) -> (AppContext, Arc<Mutex<Vec<Proposal>>>) {
    let submitted = Arc::new(Mutex::new(Vec::new()));
    let ctx = AppContext {
        repo: Arc::new(MemRepo::new(parts)),
        identity: Box::new(FixedIdentity),
        sink: Box::new(MemSink {
            submitted: submitted.clone(),
        }),
        registry_name: "test-registry".into(),
    };
    (ctx, submitted)
}

fn fixture_parts() -> Vec<Part> {
    vec![
        part(
            "23456789ABCDEF",
            PartStatus::Bound,
            Some("PT100 sensor"),
            Some("Acme"),
        ),
        part("23456789GHJKMN", PartStatus::Unbound, None, None),
        part(
            "ZZZZZZZZ234567",
            PartStatus::Void,
            Some("cable"),
            Some("Acme"),
        ),
    ]
}

/// Dispatch a JSON request (the wire form) and return the response.
fn dispatch_json(ctx: &AppContext, req: serde_json::Value) -> Response {
    let req: Request = serde_json::from_value(req).expect("request parses");
    dispatch(ctx, req)
}

// -------------------------------------------------------------------
// Protocol shape
// -------------------------------------------------------------------

#[test]
fn request_json_is_internally_tagged() {
    let req: Request =
        serde_json::from_value(json!({"op": "Resolve", "id": "23456789ABCDEF"})).expect("parses");
    assert_eq!(
        req,
        Request::Resolve {
            id: "23456789ABCDEF".into()
        }
    );
}

#[test]
fn ok_response_serializes_with_ok_true() {
    let r = Response::ok(json!({"x": 1}));
    let v = serde_json::to_value(&r).expect("encodes");
    assert_eq!(v["ok"], json!(true));
    assert_eq!(v["data"]["x"], json!(1));
}

#[test]
fn err_response_serializes_with_ok_false() {
    let r = Response::error(ErrorKind::NotFound, "nope");
    let v = serde_json::to_value(&r).expect("encodes");
    assert_eq!(v["ok"], json!(false));
    assert_eq!(v["error"]["kind"], json!("NotFound"));
}

// -------------------------------------------------------------------
// Resolve
// -------------------------------------------------------------------

#[test]
fn resolve_full_id_returns_entity() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(&ctx, json!({"op":"Resolve","id":"23456789ABCDEF"}));
    let d = r.data().expect("ok");
    assert_eq!(d["id"], json!("23456789ABCDEF"));
    assert_eq!(d["status"], json!("bound"));
    assert_eq!(d["collection"], json!("parts"));
    assert_eq!(d["fields"]["type"], json!("PT100 sensor"));
    assert!(d["transitioned_at"]["bound"].is_string());
}

#[test]
fn resolve_prefix_unique_and_ambiguous() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(&ctx, json!({"op":"Resolve","id":"ZZZZZZZZ"}));
    assert_eq!(r.data().expect("ok")["id"], json!("ZZZZZZZZ234567"));

    let r = dispatch_json(&ctx, json!({"op":"Resolve","id":"23456789"}));
    assert_eq!(r.err().expect("err").kind, ErrorKind::Ambiguous);
}

#[test]
fn resolve_typed_id_default_scheme_and_unknown_scheme() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(&ctx, json!({"op":"Resolve","id":"nano14:23456789ABCDEF"}));
    assert!(r.is_ok());
    let r = dispatch_json(&ctx, json!({"op":"Resolve","id":"udi:0001"}));
    assert_eq!(r.err().expect("err").kind, ErrorKind::Validation);
}

// -------------------------------------------------------------------
// List / Count / Describe
// -------------------------------------------------------------------

#[test]
fn list_filters_sorts_and_pages() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"List","collection":"parts","filter":{"fields":{"vendor":"acme"}}}),
    );
    let d = r.data().expect("ok");
    assert_eq!(d["total"], json!(2));

    let r = dispatch_json(
        &ctx,
        json!({"op":"List","collection":"parts",
               "sort":[{"field":"id","dir":"desc"}],
               "page":{"offset":0,"limit":1}}),
    );
    let d = r.data().expect("ok");
    assert_eq!(d["total"], json!(3));
    assert_eq!(d["items"][0]["id"], json!("ZZZZZZZZ234567"));
    assert_eq!(d["items"].as_array().expect("array").len(), 1);
}

#[test]
fn list_free_text_matches_fields() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"List","collection":"parts","filter":{"text":"pt100"}}),
    );
    assert_eq!(r.data().expect("ok")["total"], json!(1));
}

#[test]
fn unknown_collection_is_unsupported() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(&ctx, json!({"op":"List","collection":"vendors"}));
    assert_eq!(r.err().expect("err").kind, ErrorKind::Unsupported);
}

#[test]
fn count_groups_by_field() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Count","collection":"parts","by":"status"}),
    );
    let d = r.data().expect("ok");
    assert_eq!(d["counts"]["bound"], json!(1));
    assert_eq!(d["counts"]["unbound"], json!(1));
    assert_eq!(d["counts"]["void"], json!(1));
}

#[test]
fn describe_serves_descriptors_with_labels() {
    let (ctx, _) = ctx_with(Vec::new());
    let r = dispatch_json(&ctx, json!({"op":"Describe"}));
    let d = r.data().expect("ok");
    assert_eq!(d["name"], json!("test-registry"));
    assert_eq!(d["collections"][0]["name"], json!("parts"));
    let fields = d["collections"][0]["fields"].as_array().expect("fields");
    let type_field = fields
        .iter()
        .find(|f| f["key"] == json!("type"))
        .expect("type field");
    // Display label is descriptor-owned (ADR-035 §1a).
    assert_eq!(type_field["label"], json!("Type"));
    assert_eq!(
        d["collections"][0]["lifecycle"]["statuses"],
        json!(["unbound", "bound", "void"])
    );
}

// -------------------------------------------------------------------
// Create (mint) / Transition / Edit
// -------------------------------------------------------------------

#[test]
fn create_mints_n_ids_submits_proposal_and_audits() {
    let (ctx, submitted) = ctx_with(fixture_parts());
    let r = dispatch_json(&ctx, json!({"op":"Create","collection":"parts","n":3}));
    let d = r.data().expect("ok").clone();
    let minted = d["minted"].as_array().expect("minted").clone();
    assert_eq!(minted.len(), 3);
    // One proposal with 3 RowAdds, classified.
    let proposals = submitted.lock().expect("lock");
    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0].diff.adds.len(), 3);
    assert_eq!(proposals[0].change_classification.len(), 3);
    drop(proposals);
    // One audit entry per minted id.
    let audit = ctx
        .repo
        .list_audit_events(&AuditFilter::default())
        .expect("audit");
    assert_eq!(audit.len(), 3);
    // All three share one created_at (one stamp per mint event, ADR-035 §1b).
    assert!(d["created_at"].is_string());
}

#[test]
fn transition_bind_with_fields() {
    let (ctx, submitted) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Transition","collection":"parts","id":"23456789GHJKMN",
               "to":"bound","fields":{"type":"valve","vendor":"Bosch"}}),
    );
    let d = r.data().expect("ok");
    assert_eq!(d["to"], json!("bound"));
    let proposals = submitted.lock().expect("lock");
    let edit = &proposals[0].diff.edits[0];
    assert_eq!(edit.after["status"], "bound");
    assert_eq!(edit.after["type"], "valve");
    assert!(edit.after.contains_key("bound_at"));
}

#[test]
fn transition_bind_already_bound_directs_to_edit() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Transition","collection":"parts","id":"23456789ABCDEF","to":"bound"}),
    );
    let e = r.err().expect("err");
    assert_eq!(e.kind, ErrorKind::Validation);
    assert!(e.message.contains("Edit"));
}

#[test]
fn transition_void_records_reason() {
    let (ctx, submitted) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Transition","collection":"parts","id":"23456789GHJKMN",
               "to":"void","fields":{"notes":"sticker damaged"}}),
    );
    assert!(r.is_ok());
    let proposals = submitted.lock().expect("lock");
    let edit = &proposals[0].diff.edits[0];
    assert_eq!(edit.after["status"], "void");
    assert!(edit.after["notes"].contains("sticker damaged"));
}

#[test]
fn bind_voided_part_is_rejected() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Transition","collection":"parts","id":"ZZZZZZZZ234567","to":"bound"}),
    );
    assert_eq!(r.err().expect("err").kind, ErrorKind::Validation);
}

#[test]
fn edit_is_status_preserving_and_bound_only() {
    let (ctx, submitted) = ctx_with(fixture_parts());
    // Edit a bound part: ok.
    let r = dispatch_json(
        &ctx,
        json!({"op":"Edit","collection":"parts","id":"23456789ABCDEF",
               "fields":{"location":"lab-3"}}),
    );
    assert!(r.is_ok());
    let proposals = submitted.lock().expect("lock");
    let edit = &proposals[0].diff.edits[0];
    assert_eq!(edit.after["status"], "bound"); // unchanged
    assert_eq!(edit.after["location"], "lab-3");
    drop(proposals);
    // Edit an unbound part: directed to Transition.
    let r = dispatch_json(
        &ctx,
        json!({"op":"Edit","collection":"parts","id":"23456789GHJKMN",
               "fields":{"location":"lab-3"}}),
    );
    assert_eq!(r.err().expect("err").kind, ErrorKind::Validation);
}

#[test]
fn unknown_field_key_is_validation_error() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Transition","collection":"parts","id":"23456789GHJKMN",
               "to":"bound","fields":{"favourite_colour":"blue"}}),
    );
    assert_eq!(r.err().expect("err").kind, ErrorKind::Validation);
}

// -------------------------------------------------------------------
// Print / Export / Whoami / PollProposal
// -------------------------------------------------------------------

#[test]
fn print_renders_svgs_and_logs_events() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"layout":"horz","size_mm":8.0,"chars":"44"}}),
    );
    let d = r.data().expect("ok");
    let svg = d["labels"][0]["svg"].as_str().expect("svg");
    assert!(svg.contains("<svg"));
    assert_eq!(d["chars"], json!("44"));
    let events = ctx
        .repo
        .list_print_events(&PrintEventFilter::default())
        .expect("events");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].layout, "horz");
}

#[test]
fn print_by_filter_selection() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"filter":{"status":"bound"}},
               "options":{"log":false}}),
    );
    let d = r.data().expect("ok");
    assert_eq!(d["labels"].as_array().expect("labels").len(), 1);
}

#[test]
fn print_flag_requires_cable_od() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"layout":"flag"}}),
    );
    assert_eq!(r.err().expect("err").kind, ErrorKind::Validation);
}

#[test]
fn export_csv_has_header_and_rows() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Export","collection":"parts","format":"csv"}),
    );
    let d = r.data().expect("ok");
    let content = d["content"].as_str().expect("content");
    assert!(content.starts_with("id,status,created_at"));
    assert_eq!(d["rows"], json!(3));
    assert_eq!(content.lines().count(), 4); // header + 3 rows
}

#[test]
fn whoami_renders_operator() {
    let (ctx, _) = ctx_with(Vec::new());
    let r = dispatch_json(&ctx, json!({"op":"Whoami"}));
    let d = r.data().expect("ok");
    assert_eq!(d["id"], json!("git:test-operator"));
    assert_eq!(d["display_name"], json!("Test Operator"));
}

#[test]
fn poll_proposal_returns_status() {
    let (ctx, _) = ctx_with(Vec::new());
    let r = dispatch_json(
        &ctx,
        json!({"op":"PollProposal","proposal":{"url":"mem://proposal/1","local_id":"1","adapter":"mem"}}),
    );
    let d = r.data().expect("ok");
    assert_eq!(d["status"]["kind"], json!("open"));
}
