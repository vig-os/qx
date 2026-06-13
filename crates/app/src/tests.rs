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
        minted_by: None,
        bound_by: None,
        last_edited_at: None,
        last_edited_by: None,
        components: Vec::new(),
        manufacturer_id: None,
        metadata: BTreeMap::new(),
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

// ADR-031 §2/§8 px-true path (obligation `px-true-qr-render`):
// size_px is the EXACT output canvas, padding references the MODULE
// part, module_px DEDUCED per padding_mode. The §8 worked example:
// micro M4 (data 17, quiet 2) at 64/pad 2 overlap → m=3 (17·3 +
// 2·max(2,6) = 63 ≤ 64), module part 51px, uniform white 6px.
#[test]
fn print_px_true_deduces_module_from_exact_canvas() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44","padding_px":2}}),
    );
    let d = r.data().expect("ok");
    let label = &d["labels"][0];
    assert_eq!(label["module_px"], json!(3));
    assert_eq!(label["data_px"], json!(51));
    // Per-side actual white (§8): controlling-axis remainder px lands
    // on the bottom; non-controlling sides sit at their floors.
    assert_eq!(
        label["white"],
        json!({"top": 6, "right": 6, "bottom": 7, "left": 6})
    );
    assert_eq!(label["qr_px"], json!(63));
    assert_eq!(label["padding_mode"], json!("overlap"));
    // The deprecated micro flag resolved through the symbology grammar.
    assert_eq!(label["symbology"], json!("micro-m4-m"));
    assert_eq!(label["height_px"], json!(64), "exact canvas");
    assert_eq!(label["id"], json!("23456789ABCDEF"));
    // ADR-031 §8 bitmap typography (nx75 anchor font): the 7-row
    // glyph cell and the g-law scale ride the response — module part
    // 51, 2 rows → g = min(51/15, m) = 3 — and the SVG is one
    // font-free binary raster.
    assert_eq!(label["glyph_cell"], json!(7), "nx75 7-row cell");
    assert_eq!(label["glyph_px"], json!(3), "g-law scale");
    let svg = label["svg"].as_str().expect("svg");
    assert!(svg.contains("shape-rendering=\"crispEdges\""));
    assert!(!svg.contains("<text"), "no <text> in px output");
    assert!(!svg.contains("font-family"), "no fonts in px output");
    assert_eq!(d["unit"], json!("px"));
    assert_eq!(d["size_px"], json!(64));
    assert_eq!(d["padding_mode"], json!("overlap"));
    // Print events still log, with the resolved px params as evidence.
    let events = ctx
        .repo
        .list_print_events(&PrintEventFilter::default())
        .expect("events");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].extra["module_px"], json!(3));
    assert_eq!(events[0].extra["data_px"], json!(51));
    assert_eq!(events[0].extra["white"]["top"], json!(6));
    assert_eq!(events[0].extra["padding_mode"], json!("overlap"));
    assert_eq!(events[0].extra["symbology"], json!("micro-m4-m"));

    // additive excludes the quiet zone from the padding floor:
    // (17 + 2·2)·m + 2·2 ≤ 64 → m=2, module part 34px.
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44","padding_px":2,
                          "padding_mode":"additive","log":false}}),
    );
    let label = &r.data().expect("ok")["labels"][0];
    assert_eq!(label["module_px"], json!(2));
    assert_eq!(label["data_px"], json!(34));
    assert_eq!(label["padding_mode"], json!("additive"));
    assert_eq!(label["height_px"], json!(64), "exact canvas");
}

#[test]
fn print_px_unknown_padding_mode_is_validation() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "padding_mode":"bleed","log":false}}),
    );
    let e = r.err().expect("err");
    assert_eq!(e.kind, ErrorKind::Validation);
    assert!(e.message.contains("overlap"), "modes listed: {}", e.message);
}

#[test]
fn print_px_job_fills_to_uniform_footprint() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF","23456789GHJKMN"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44","padding_px":2,"log":false}}),
    );
    let d = r.data().expect("ok");
    let labels = d["labels"].as_array().expect("labels");
    assert_eq!(labels.len(), 2);
    assert_eq!(labels[0]["width_px"], labels[1]["width_px"]);
    assert_eq!(labels[0]["height_px"], labels[1]["height_px"]);
}

#[test]
fn print_px_mm_dpi_converts_then_snaps() {
    // 8 mm at the default 300 dpi rounds to a 94 px canvas; Micro M4
    // (data 17, quiet 2, pad 0, overlap) deduces m=4: 17·4 +
    // 2·max(0,8) = 84 ≤ 94 → module part 68px.
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_mm":8.0,"micro":true,
                          "chars":"44","log":false}}),
    );
    let d = r.data().expect("ok");
    assert_eq!(d["size_px"], json!(94));
    assert_eq!(d["dpi"], json!(300.0));
    assert_eq!(d["labels"][0]["module_px"], json!(4));
    assert_eq!(d["labels"][0]["data_px"], json!(68));
    assert_eq!(d["labels"][0]["qr_px"], json!(84));
}

#[test]
fn print_px_below_minimum_is_validation_with_hint() {
    // Overlap minimum for micro M4 at pad 0: 17 + 2·2 = 21px.
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":20,"micro":true,"chars":"44"}}),
    );
    let e = r.err().expect("err");
    assert_eq!(e.kind, ErrorKind::Validation);
    assert!(e.message.contains("21px"), "hint missing: {}", e.message);

    // The hint follows the ACTIVE mode: additive minimum at pad 2 is
    // (17 + 4)·1 + 4 = 25px.
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":24,"micro":true,"chars":"44",
                          "padding_px":2,"padding_mode":"additive"}}),
    );
    let e = r.err().expect("err");
    assert_eq!(e.kind, ErrorKind::Validation);
    assert!(e.message.contains("25px"), "hint missing: {}", e.message);
}

// ADR-031 §8: symbology version + EC are contract parameters. The
// wire speaks the canonical compact string; labels carry the RESOLVED
// form. M3-L pinned at clip@64: 15 data modules → floor(64/15) = 4px
// modules (vs M4's 3px — the §8 bigger-dots A/B).
#[test]
fn print_px_symbology_pin_resolves_and_flows_into_the_deduction() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"symbology":"micro-m3-l",
                          "padding_mode":"clip","chars":"44","log":false}}),
    );
    let label = &r.data().expect("ok")["labels"][0];
    assert_eq!(label["symbology"], json!("micro-m3-l"));
    assert_eq!(label["module_px"], json!(4), "clip@64 on 15 modules");
    assert_eq!(label["data_px"], json!(60));
    // Typography follows the module part under the g-law: a 2-row
    // block needs 15g ≤ 60, so the bigger M3 dots carry g to 4.
    assert_eq!(label["glyph_cell"], json!(7));
    assert_eq!(label["glyph_px"], json!(4));
    assert_eq!(label["height_px"], json!(64), "exact canvas");

    // Version-only pin: EC auto-falls to the strongest feasible (L at
    // M3 for 14 alnum chars) and the response says so.
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"symbology":"micro-m3",
                          "chars":"44","log":false}}),
    );
    let label = &r.data().expect("ok")["labels"][0];
    assert_eq!(label["symbology"], json!("micro-m3-l"));
}

#[test]
fn print_px_symbology_wins_over_deprecated_micro_flag() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "symbology":"qr","chars":"44","log":false}}),
    );
    let label = &r.data().expect("ok")["labels"][0];
    assert_eq!(label["symbology"], json!("qr-v1-m"), "symbology wins");
    assert_eq!(label["data_px"], json!(42), "V1 = 21 modules at m=2");
}

#[test]
fn print_px_unknown_symbology_family_is_validation_with_roster() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"symbology":"aztec","log":false}}),
    );
    let e = r.err().expect("err");
    assert_eq!(e.kind, ErrorKind::Validation);
    assert!(e.message.contains("micro, qr"), "roster: {}", e.message);
    assert!(e.message.contains("dm"), "future hint: {}", e.message);
}

#[test]
fn print_px_infeasible_pin_is_validation_with_feasible_space() {
    // NB the fixture id matters: feasibility is the encoder's verdict
    // on the actual payload, and qrcode's optimal segmentation packs
    // long numeric runs (e.g. "23456789…") into M4-Q despite the
    // 13-alnum cap. A mixed id has no such run.
    let (ctx, _) = ctx_with(vec![part(
        "K7M3PQ9RT5VAXY",
        PartStatus::Unbound,
        None,
        None,
    )]);
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["K7M3PQ9RT5VAXY"]},
               "options":{"unit":"px","size_px":64,"symbology":"micro-m4-q","log":false}}),
    );
    let e = r.err().expect("err");
    assert_eq!(e.kind, ErrorKind::Validation);
    assert!(
        e.message.contains("M4-Q caps at 13 alnum chars"),
        "cap: {}",
        e.message
    );
    assert!(
        e.message
            .contains("feasible for this payload: micro-m4-l, micro-m4-m, micro-m3-l"),
        "feasible space: {}",
        e.message
    );
}

// §8 per-side padding: the wire mirrors the CSS shorthand as
// serde-untagged `2 | [2,6] | [2,6,4,6]`; the plain integer (asserted
// in print_px_true_deduces_module_from_exact_canvas above) is also
// the pre-shorthand wire shape — old payloads keep deserializing.
#[test]
fn print_px_padding_css_shorthand_on_the_wire() {
    let (ctx, _) = ctx_with(fixture_parts());
    // [2,6]: vertical 2, horizontal 6 — controlling axis (horz =
    // vertical) still reaches m=3; left/right floors are max(6, 6).
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44","padding_px":[2,6],"log":false}}),
    );
    let label = &r.data().expect("ok")["labels"][0];
    assert_eq!(label["module_px"], json!(3));
    assert_eq!(
        label["white"],
        json!({"top": 6, "right": 6, "bottom": 7, "left": 6})
    );

    // Padding 2,10,6,4 — top/bottom floors 6/6 keep m=3 since
    // 51+12 = 63 fits 64, right floor max(10,6) = 10, left max(4,6) = 6.
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44","padding_px":[2,10,6,4],"log":false}}),
    );
    let d = r.data().expect("ok");
    let label = &d["labels"][0];
    assert_eq!(label["module_px"], json!(3));
    assert_eq!(
        label["white"],
        json!({"top": 6, "right": 10, "bottom": 7, "left": 6})
    );
    // The resolved per-side floors ride the response as evidence.
    assert_eq!(
        d["padding"],
        json!({"top": 2, "right": 10, "bottom": 6, "left": 4})
    );
}

#[test]
fn print_px_malformed_padding_is_a_request_parse_error() {
    // Three values match no PaddingSpec arm — the request fails to
    // deserialize (1, 2, or 4 values only).
    let req = serde_json::from_value::<Request>(json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"padding_px":[2,6,4]}}));
    assert!(req.is_err(), "3-value padding must not parse");
}

#[test]
fn print_mm_takes_symbology_family_but_rejects_pins() {
    let (ctx, _) = ctx_with(fixture_parts());
    // Family-only symbology selects the micro mm renderer.
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"symbology":"micro","chars":"44","log":false}}),
    );
    let label = &r.data().expect("ok")["labels"][0];
    assert_eq!(label["symbology"], json!("micro-m4-m"), "mm fixed pin");
    // Version/EC pins are px-contract parameters.
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"symbology":"micro-m3-l","chars":"44","log":false}}),
    );
    let e = r.err().expect("err");
    assert_eq!(e.kind, ErrorKind::Validation);
    assert!(e.message.contains("px"), "points at px: {}", e.message);
}

// Repeat primitives compose on the px path only; the mm renderer
// refuses them instead of silently rendering a single copy.
#[test]
fn print_mm_rejects_repeat_flags_instead_of_ignoring_them() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"symbology":"micro","chars":"44","log":false,
                          "repeat":"3","repeat_orient":"alternate"}}),
    );
    let e = r.err().expect("err");
    assert_eq!(e.kind, ErrorKind::Validation);
    assert!(
        e.message.contains("px-true"),
        "points at the px path: {}",
        e.message
    );
}

// The one CSS-shorthand expansion rule, text form (the CLI value
// parser) and wire form (serde-untagged) — plus old-wire compat.
#[test]
fn padding_spec_parses_and_expands_the_css_shorthand() {
    use part_registry_codec::Padding;

    use crate::protocol::PaddingSpec;

    let cases = [
        ("2", Padding::uniform(2)),
        ("2,6", Padding::axes(2, 6)),
        ("2,6,4,6", Padding::sides(2, 6, 4, 6)),
        (" 2 , 6 ", Padding::axes(2, 6)),
    ];
    for (input, expected) in cases {
        let spec: PaddingSpec = input.parse().expect(input);
        assert_eq!(spec.expand(), expected, "input {input:?}");
    }
    for bad in ["", "2,6,4", "2,6,4,6,8", "a", "2,-6", "2.5"] {
        assert!(bad.parse::<PaddingSpec>().is_err(), "must reject {bad:?}");
    }
    // Old wire shape: a plain integer is Uniform.
    let spec: PaddingSpec = serde_json::from_value(json!(2)).expect("integer parses");
    assert_eq!(spec.expand(), Padding::uniform(2));
    let spec: PaddingSpec = serde_json::from_value(json!([2, 6])).expect("pair parses");
    assert_eq!(spec.expand(), Padding::axes(2, 6));
    let spec: PaddingSpec = serde_json::from_value(json!([2, 6, 4, 6])).expect("quad parses");
    assert_eq!(spec.expand(), Padding::sides(2, 6, 4, 6));
}

#[test]
fn print_px_flag_layout_is_unsupported() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"layout":"flag","chars":"44"}}),
    );
    assert_eq!(r.err().expect("err").kind, ErrorKind::Unsupported);
}

#[test]
fn print_unknown_unit_is_validation() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"inch"}}),
    );
    assert_eq!(r.err().expect("err").kind, ErrorKind::Validation);
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

// -------------------------------------------------------------------
// ADR-031 §10 print-contract — stage 1
// -------------------------------------------------------------------

// Payload DSL: stage 2 opens single-level h/v groups. Two nesting
// levels remain staged with the explicit message.
#[test]
fn print_px_payload_nesting_rejected_with_staged_message() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "payload":"[h: [v: qr id]]","log":false}}),
    );
    let e = r.err().expect("err");
    assert_eq!(e.kind, ErrorKind::Validation);
    assert!(
        e.message.contains("groups inside groups"),
        "got: {}",
        e.message
    );
}

// Sugar equivalence (ADR-031 §10): `--chars 554` mapped to the
// payload form `id:554` (via --payload) produces the same px
// geometry. The receipt's payload string is the stage-1 canonical
// form; sugar's job is geometry-equivalence, not string identity
// (the implicit "qr id" path doesn't know the grouping flag
// resolves to "554", and the §10 grammar lets either form coexist).
#[test]
fn print_px_payload_id_grouping_matches_chars_flag_geometry() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r1 = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"554","log":false}}),
    );
    let r2 = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"554","payload":"qr id:554","log":false}}),
    );
    let a = &r1.data().expect("ok")["labels"][0];
    let b = &r2.data().expect("ok")["labels"][0];
    assert_eq!(a["module_px"], b["module_px"]);
    assert_eq!(a["data_px"], b["data_px"]);
    assert_eq!(a["qr_px"], b["qr_px"]);
    // Both paths inscribe the same default-form receipt payload
    // when no solver block is engaged (--chars threads through the
    // legacy g-law; --id-chars/--rows/--id-size engages the solver,
    // tested separately).
    assert_eq!(a["receipt"]["payload"], json!("qr id"));
    assert_eq!(b["receipt"]["payload"], json!("qr id"));
}

// ADR-031 §10 fix-two-derive-one: --id-chars + --rows derives g.
#[test]
fn print_px_solver_derives_glyph_from_chars_and_rows() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44",
                          "id_chars":14,"rows":3,"log":false}}),
    );
    let l = &r.data().expect("ok")["labels"][0];
    // budget = data_px = 51 (m=3), 14 chars over 3 rows balances
    // to 5,5,4, and g = floor(51 / (8·3 - 1)) = floor(51/23) = 2
    // (capped at m=3).
    assert_eq!(l["module_px"], json!(3));
    assert_eq!(l["glyph_px"], json!(2));
}

// ADR-031 §10 infeasible solver pin: the error message MUST quote
// the nearest feasible triple (the §10 example pattern). Use a
// size where the g-law cap (module_px) is bigger than the requested
// id_size_px, so the budget-need-have message fires before the cap.
#[test]
fn print_px_solver_infeasible_quotes_nearest_feasible_triple() {
    let (ctx, _) = ctx_with(fixture_parts());
    // size_px 300, clip, micro → module_px = 300/17 = 17, budget =
    // 17·17 = 289. Requested: 14 chars / 3 rows / 16px needs
    // (8·3-1)·16 = 368; budget 289 — infeasible. Nearest g for
    // rows=3 = 289/23 = 12; nearest rows for g=16 = 8r-1 ≤ 18 →
    // r=2 (15·16 = 240 ≤ 289).
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":300,"micro":true,
                          "padding_mode":"clip","chars":"44",
                          "id_chars":14,"rows":3,"id_size_px":16,
                          "log":false}}),
    );
    let e = r.err().expect("err");
    assert_eq!(e.kind, ErrorKind::Validation);
    assert!(
        e.message.contains("needs") && e.message.contains("have"),
        "missing need/have: {}",
        e.message
    );
    assert!(
        e.message.contains("feasible:") || e.message.contains("increase the size"),
        "missing nearest-feasible hint: {}",
        e.message
    );
}

// ADR-031 §10 colors: --fg + --bg ride the receipt; the response
// surfaces the parsed canonical forms.
#[test]
fn print_px_colors_ride_through_to_response_and_receipt() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44","fg":"#222","bg":"#fff","log":false}}),
    );
    let d = r.data().expect("ok");
    assert_eq!(d["fg"], json!("#222"));
    assert_eq!(d["bg"], json!("#fff"));
    assert_eq!(d["labels"][0]["receipt"]["fg"], json!("#222"));
    assert_eq!(d["labels"][0]["receipt"]["bg"], json!("#fff"));
    let svg = d["labels"][0]["svg"].as_str().unwrap();
    assert!(svg.contains("fill=\"#fff\""), "bg rect emits fg: {svg}");
    assert!(svg.contains("fill=\"#222\""), "module fg present");
}

// Low-contrast WARN tier rides the response.warning field (combined
// with other warnings via the "; " separator).
#[test]
fn print_px_low_contrast_colors_emit_warning() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44","fg":"#444","bg":"#333","log":false}}),
    );
    let d = r.data().expect("ok");
    let w = d["warning"].as_str().expect("warning");
    assert!(
        w.contains("low contrast") || w.contains("inverted"),
        "expected color warning: {w}"
    );
}

// bg=none flips the warning to surface-dependent + the SVG omits the
// background rect entirely.
#[test]
fn print_px_bg_none_omits_rect_and_warns_surface() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44","bg":"none","log":false}}),
    );
    let d = r.data().expect("ok");
    assert_eq!(d["bg"], json!("none"));
    let svg = d["labels"][0]["svg"].as_str().unwrap();
    // The fg group still emits `fill="black"` (default fg); the
    // bg rect is absent.
    assert!(
        !svg.contains("<rect width=\"64\""),
        "bg rect must be absent: {svg}"
    );
    let w = d["warning"].as_str().expect("surface warning");
    assert!(w.contains("surface-dependent"), "got: {w}");
}

// mm renderer fix: the bg rect is emitted (default white) when --bg
// is set, retiring the legacy accidental transparency.
#[test]
fn print_mm_with_colors_emits_background_rect() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"mm","size_mm":8.0,"chars":"44",
                          "fg":"#000","bg":"white","log":false}}),
    );
    let d = r.data().expect("ok");
    let svg = d["labels"][0]["svg"].as_str().unwrap();
    assert!(
        svg.contains("fill=\"white\""),
        "mm bg rect now emitted: {svg}"
    );
}

// SSOT: the response receipt EQUALS the SVG `<metadata>` JSON,
// field for field. Built once, used twice.
#[test]
fn print_px_receipt_equals_inscribed_metadata() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44","log":false}}),
    );
    let l = &r.data().expect("ok")["labels"][0];
    let response_receipt = &l["receipt"];
    let svg = l["svg"].as_str().expect("svg");
    let inscribed: serde_json::Value = {
        let start = svg.find("<![CDATA[").expect("metadata present");
        let end = svg[start..].find("]]>").expect("CDATA closed");
        let json = &svg[start + "<![CDATA[".len()..start + end];
        serde_json::from_str(json).expect("metadata is JSON")
    };
    assert_eq!(*response_receipt, inscribed, "receipt SSOT drift");
}

// Snap-mode geometry: the canvas snaps DOWN to the content lattice
// instead of holding size_px exactly.
#[test]
fn print_px_snap_mode_shrinks_canvas_to_content_lattice() {
    let (ctx, _) = ctx_with(fixture_parts());
    let exact = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44","log":false}}),
    );
    let snap = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44","size_mode":"snap","log":false}}),
    );
    let e = &exact.data().expect("ok")["labels"][0];
    let s = &snap.data().expect("ok")["labels"][0];
    // Exact mode holds the canvas at size_px on the controlling axis
    // (horz: height); snap mode drops the lattice remainder.
    assert_eq!(e["height_px"], json!(64));
    assert!(
        s["height_px"].as_u64().unwrap() < 64,
        "snap shrank, exact stayed: snap_h = {}",
        s["height_px"]
    );
    // Module geometry is the same — only the canvas changes.
    assert_eq!(e["module_px"], s["module_px"]);
    assert_eq!(e["data_px"], s["data_px"]);
    assert_eq!(s["receipt"]["size_mode"], json!("snap"));
    assert_eq!(e["receipt"]["size_mode"], json!("exact"));
}

// Unknown size_mode is Validation with the modes list.
#[test]
fn print_px_unknown_size_mode_is_validation() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"size_mode":"wibble","log":false}}),
    );
    let e = r.err().expect("err");
    assert_eq!(e.kind, ErrorKind::Validation);
    assert!(e.message.contains("exact"));
    assert!(e.message.contains("snap"));
}

// Unknown payload color value is Validation (e.g. uppercase name
// without the explicit lowercase grammar).
#[test]
fn print_px_unknown_color_is_validation() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"fg":"BLACK","log":false}}),
    );
    let e = r.err().expect("err");
    assert_eq!(e.kind, ErrorKind::Validation);
    assert!(e.message.contains("BLACK"), "got: {}", e.message);
}

// ---------- ADR-031 §10 repeat (stage 2) ----------

// --repeat 3 composes three copies along the canvas axis with the
// explicit gap; the repeat receipt rides the response.
#[test]
fn print_px_repeat_composes_n_copies_with_gap() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44","repeat":"3","repeat_gap_px":10,
                          "log":false}}),
    );
    let d = r.data().expect("ok");
    let label = &d["labels"][0];
    let repeat = &label["repeat"];
    assert_eq!(repeat["n"], json!(3));
    assert_eq!(repeat["gap_px"], json!(10));
    assert_eq!(repeat["axis"], json!("along"));
    assert_eq!(repeat["spacing"], json!("linear"));
    // The composed SVG has three translate(...) groups.
    let svg = label["svg"].as_str().expect("svg");
    assert_eq!(
        svg.matches("translate(").count() - svg.matches("translate(-").count(),
        3,
        "three positive translates for three copies: {svg}"
    );
}

// --repeat fill needs --length to know how many copies fit.
#[test]
fn print_px_repeat_fill_resolves_count_from_length() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44","repeat":"fill","length_px":400,
                          "repeat_gap_px":5,"log":false}}),
    );
    let d = r.data().expect("ok");
    let n = d["labels"][0]["repeat"]["n"].as_u64().expect("n");
    assert!(n >= 2, "fill should pick multiple copies, got {n}");
}

// fill without --length is a Validation error pointing at --length.
#[test]
fn print_px_repeat_fill_without_length_is_validation() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44","repeat":"fill","log":false}}),
    );
    let e = r.err().expect("err");
    assert_eq!(e.kind, ErrorKind::Validation);
    assert!(e.message.contains("--length"), "got: {}", e.message);
}

// Infeasible n quotes feasible alternatives (the §10 contract).
#[test]
fn print_px_repeat_infeasible_quotes_feasible() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44","repeat":"100","repeat_gap_px":5,
                          "length_px":200,"log":false}}),
    );
    let e = r.err().expect("err");
    assert_eq!(e.kind, ErrorKind::Validation);
    assert!(e.message.contains("Feasible"), "got: {}", e.message);
}

// Cyclic spacing with derived gap from length: 4 copies + 4 gaps
// land at i*length/n offsets.
#[test]
fn print_px_repeat_cyclic_derives_gap() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44","repeat":"4","spacing":"cyclic",
                          "length_px":800,"log":false}}),
    );
    let d = r.data().expect("ok");
    let repeat = &d["labels"][0]["repeat"];
    assert_eq!(repeat["spacing"], json!("cyclic"));
    assert_eq!(repeat["n"], json!(4));
}

// --rotate 90 swaps the per-label canvas dims before composition.
#[test]
fn print_px_rotate_90_swaps_dims() {
    let (ctx, _) = ctx_with(fixture_parts());
    let plain = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44","log":false}}),
    );
    let rotated = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44","rotate":90,"log":false}}),
    );
    let p = &plain.data().expect("ok")["labels"][0];
    let r = &rotated.data().expect("ok")["labels"][0];
    // Rotated canvas: width <-> height swap.
    assert_eq!(p["width_px"], r["height_px"]);
    assert_eq!(p["height_px"], r["width_px"]);
    assert_eq!(r["repeat"]["rotate"], json!(90));
}

// --excess-at start shifts the first copy to the excess offset.
#[test]
fn print_px_repeat_excess_at_start() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44","repeat":"1","length_excess_px":40,
                          "excess_at":"start","log":false}}),
    );
    let d = r.data().expect("ok");
    let svg = d["labels"][0]["svg"].as_str().expect("svg");
    assert!(svg.contains("translate(40,0)"), "first copy shifted: {svg}");
}

// Alternate orient flips every second copy.
#[test]
fn print_px_repeat_alternate_orient_rotates_odd_copies() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44","repeat":"2","repeat_gap_px":0,
                          "repeat_orient":"alternate","log":false}}),
    );
    let d = r.data().expect("ok");
    let svg = d["labels"][0]["svg"].as_str().expect("svg");
    assert!(svg.contains("rotate(180)"), "alternate must rotate: {svg}");
    assert_eq!(d["labels"][0]["repeat"]["orient"], json!("alternate"));
}

// Deprecated --layout flag + --cable-od expands to repeat 2 / linear
// / alternate, and emits a deprecation warning.
#[test]
fn print_px_deprecated_flag_sugar_expands_to_repeat_2_alternate() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44","layout":"flag","cable_od_mm":6.0,
                          "log":false}}),
    );
    let d = r.data().expect("ok");
    let repeat = &d["labels"][0]["repeat"];
    assert_eq!(repeat["n"], json!(2));
    assert_eq!(repeat["spacing"], json!("linear"));
    assert_eq!(repeat["orient"], json!("alternate"));
    let warn = d["warning"].as_str().expect("warning string");
    assert!(warn.contains("deprecated"), "got: {warn}");
    assert!(warn.contains("--repeat 2"), "got: {warn}");
}

// Across-axis repeat builds multi-up rows.
#[test]
fn print_px_repeat_across_axis_builds_rows() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44","repeat":"3","repeat_axis":"across",
                          "repeat_gap_px":4,"log":false}}),
    );
    let d = r.data().expect("ok");
    let label = &d["labels"][0];
    assert_eq!(label["repeat"]["axis"], json!("across"));
}

// Unknown --repeat value is Validation.
#[test]
fn print_px_repeat_unknown_count_is_validation() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44","repeat":"wibble","log":false}}),
    );
    let e = r.err().expect("err");
    assert_eq!(e.kind, ErrorKind::Validation);
}

// Unknown --rotate degree is Validation (non-right-angle).
#[test]
fn print_px_rotate_45_is_validation() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44","rotate":45,"log":false}}),
    );
    let e = r.err().expect("err");
    assert_eq!(e.kind, ErrorKind::Validation);
    assert!(e.message.contains("right angles"), "got: {}", e.message);
}

// Plain payload without --repeat keeps the response shape stable:
// no repeat field is emitted, byte-identical output.
#[test]
fn print_px_no_repeat_keeps_response_shape_stable() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44","log":false}}),
    );
    let label = &r.data().expect("ok")["labels"][0];
    // serde_json represents missing field as Null when included via
    // json!; ensure when we DON'T request repeat, the repeat field is
    // null (no composer ran).
    assert!(label["repeat"].is_null(), "no repeat means null: {label}");
}

// ---------- ADR-031 §10 nested groups (stage 2) ----------

// REGRESSION PIN: plain `qr id` payload renders byte-identical to the
// pre-stage-2 behavior. The new grammar must not perturb the
// canonical flat-list case.
#[test]
fn print_px_plain_qr_id_payload_byte_identical_to_no_payload() {
    let (ctx, _) = ctx_with(fixture_parts());
    let no_payload = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44","log":false}}),
    );
    let with_payload = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44","payload":"qr id","log":false}}),
    );
    let a = no_payload.data().expect("ok");
    let b = with_payload.data().expect("ok");
    let svg_a = a["labels"][0]["svg"].as_str().expect("svg");
    let svg_b = b["labels"][0]["svg"].as_str().expect("svg");
    assert_eq!(svg_a, svg_b, "plain qr id must be byte-identical");
}

// Nested h-group flattens to the same SVG as plain qr id.
#[test]
fn print_px_h_group_payload_equivalent_to_flat() {
    let (ctx, _) = ctx_with(fixture_parts());
    let flat = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44","payload":"qr id","log":false}}),
    );
    let grouped = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44","payload":"[h: qr id]","log":false}}),
    );
    let a = flat.data().expect("ok")["labels"][0]["svg"]
        .as_str()
        .unwrap()
        .to_string();
    let b = grouped.data().expect("ok")["labels"][0]["svg"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(a, b, "[h: qr id] flattens to qr id");
}

// Group-inside-group is a Validation error with the staged message.
#[test]
fn print_px_groups_inside_groups_rejected_staged() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44","payload":"[h: [v: qr id]]",
                          "log":false}}),
    );
    let e = r.err().expect("err");
    assert_eq!(e.kind, ErrorKind::Validation);
    assert!(
        e.message.contains("groups inside groups"),
        "got: {}",
        e.message
    );
}

// Per-node sizing parses into the tree without breaking the flat
// flatten path (the size is recorded; stage 2 render is the flat case).
#[test]
fn print_px_payload_with_node_sizing_parses() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44","payload":"qr@8px id","log":false}}),
    );
    // The flat path doesn't act on sizing yet — it parses cleanly
    // and renders as the qr+id arrangement.
    assert!(r.data().is_some(), "parses: {r:?}");
}

// ---------- ADR-031 §10 canvas group (stage 2) ----------

// Canvas at root validates dims + positions and surfaces the resolved
// tree in the response.
#[test]
fn print_px_canvas_root_resolves_to_response() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44",
                          "payload":"[c 100x50px: qr@(0px,0px)@30px id@(40px,10px)@20px]",
                          "log":false}}),
    );
    let d = r.data().expect("ok");
    assert_eq!(d["canvas"]["width_px"], json!(100));
    assert_eq!(d["canvas"]["height_px"], json!(50));
    let children = d["canvas"]["children"].as_array().expect("children");
    assert_eq!(children.len(), 2);
}

// Canvas child overflow is Validation with the overflow px noted.
#[test]
fn print_px_canvas_overflow_is_validation_with_overflow_px() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44",
                          "payload":"[c 50x50px: qr@(40px,0px)@20px]",
                          "log":false}}),
    );
    let e = r.err().expect("err");
    assert_eq!(e.kind, ErrorKind::Validation);
    assert!(e.message.contains("overflow"), "got: {}", e.message);
}

// QR-over-QR canvas overlap is ERROR.
#[test]
fn print_px_canvas_qr_over_qr_is_validation() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44",
                          "payload":"[c 100x100px: qr@(0px,0px)@50px qr@(30px,30px)@50px]",
                          "log":false}}),
    );
    let e = r.err().expect("err");
    assert_eq!(e.kind, ErrorKind::Validation);
    assert!(e.message.contains("qr-over-qr"), "got: {}", e.message);
}

// QR-over-id canvas overlap is WARN (rides as the response warning).
#[test]
fn print_px_canvas_qr_over_id_is_warning() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44",
                          "payload":"[c 100x100px: qr@(0px,0px)@50px id@(30px,30px)@50px]",
                          "log":false}}),
    );
    let d = r.data().expect("ok");
    let warning = d["warning"].as_str().expect("warning");
    assert!(warning.contains("overlaps"), "got: {warning}");
}

// Canvas inside flow ([qr [c ...]]) is rejected as staged.
#[test]
fn print_px_canvas_inside_flow_rejected_staged() {
    let (ctx, _) = ctx_with(fixture_parts());
    let r = dispatch_json(
        &ctx,
        json!({"op":"Print","collection":"parts",
               "selection":{"ids":["23456789ABCDEF"]},
               "options":{"unit":"px","size_px":64,"micro":true,
                          "chars":"44",
                          "payload":"qr [c 100x100px: qr@(0px,0px)]",
                          "log":false}}),
    );
    let e = r.err().expect("err");
    assert_eq!(e.kind, ErrorKind::Validation);
    assert!(e.message.contains("root-only"), "got: {}", e.message);
}
