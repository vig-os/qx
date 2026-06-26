//! `dispatch` — the one entry point every shell calls (ADR-030 §1).
//!
//! Handlers operate over the ports only (`Repository`, `ProposalSink`,
//! `IdentityProvider`); shells must depend on this crate and never on
//! adapter crates (ADR-030 architectural invariant, enforced via the
//! ADR-029 coverage discipline).

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use serde_json::json;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use qx_codec::{
    check_format_warning, color, compose_repeat, deprecated_flag_sugar, fill_to_max,
    payload as payload_dsl, recommend_format, render_label, render_label_px_with_opts,
    solver as solver_mod, svg as svg_mod, CodecError, Color, ExcessAt, Family, IdBlock, Layout,
    Orient, Padding, PaddingMode, PayloadShape, PxLabel, RenderOpts, RepeatAxis, RepeatCount,
    RepeatOpts, Rotate, SizeMode, SolverInputs, Spacing, Symbology, TextFormat,
};
use qx_domain::{
    Diff, DiffEdit, DiffRow, Operator, OperatorRef, Part, PartId, PartStatus, PrintEvent, Proposal,
    ProposalRef, RecordWrite, RequestId, PART_ID_ALPHABET, PART_ID_LEN,
};
use qx_identity::IdentityProvider;
use qx_observability::{bind_audit_entry, emit_audit, mint_audit_entry, void_audit_entry};
use qx_storage::{PartFilter, Repository};
use qx_transport::ProposalSink;

use crate::entity::{entity_from_record, field_value, part_to_entity, Entity};
use crate::preset::{
    descriptor_from_contract, parts_descriptor, registry_descriptor, RegistryDescriptor,
};
use crate::protocol::{
    ErrorKind, Filter, PaddingSpec, Page, PrintOptions, Request, Response, Selection, Sort, SortDir,
};

/// Human-prefix length accepted by `Resolve` (ADR-012; mirrors the
/// `parts` preset's `prefix_length`).
pub const HUMAN_PREFIX_LEN: usize = 8;

/// The wired ports for one open registry connection (ADR-030 §4).
pub struct AppContext {
    pub repo: Arc<dyn Repository>,
    pub identity: Box<dyn IdentityProvider>,
    pub sink: Box<dyn ProposalSink>,
    /// Display name served by `Describe` (registry identity until the
    /// manifest lands, ADR-034).
    pub registry_name: String,
    /// The registry's parsed contract (`.qx/contract.json`), when loaded.
    /// `None` = the FE snapshot path / tests with no contract → the engine
    /// falls back to the code-owned presets (ADR-035 §0 / ADR-040).
    pub contract: Option<Arc<qx_contract::Contract>>,
}

/// Dispatch one protocol request. Never panics on caller input; every
/// failure maps into `Response::Err` with the protocol error taxonomy.
pub fn dispatch(ctx: &AppContext, req: Request) -> Response {
    match req {
        Request::Resolve { id } => resolve(ctx, &id),
        Request::List {
            collection,
            filter,
            sort,
            page,
        } => list(ctx, &collection, &filter, &sort, &page),
        Request::Count {
            collection,
            filter,
            by,
        } => count(ctx, &collection, &filter, &by),
        Request::Describe { collection } => describe(ctx, collection.as_deref()),
        Request::Create {
            collection,
            n,
            fields,
        } => create(ctx, &collection, n, &fields),
        Request::Edit {
            collection,
            id,
            fields,
        } => edit(ctx, &collection, &id, &fields),
        Request::Transition {
            collection,
            id,
            to,
            fields,
        } => transition(ctx, &collection, &id, &to, &fields),
        Request::Print {
            collection,
            selection,
            options,
        } => print(ctx, &collection, &selection, &options),
        Request::Export { collection, format } => export(ctx, &collection, &format),
        Request::PollProposal { proposal } => poll_proposal(ctx, &proposal),
        Request::Whoami => whoami(ctx),
    }
}

// -------------------------------------------------------------------
// Collection guard — until the per-registry contract engine lands the
// declared roster is exactly the code-owned presets (ADR-035 §0).
// -------------------------------------------------------------------

fn known_collection(ctx: &AppContext, collection: &str) -> Result<(), Response> {
    let _ = ctx;
    if collection == "parts" {
        Ok(())
    } else {
        Err(Response::error(
            ErrorKind::Unsupported,
            format!(
                "collection {collection:?} is not declared in this registry \
                 (preset roster: parts; vocab collections land with the \
                 contract engine — obligation `collections-metamodel`)"
            ),
        ))
    }
}

fn rfc3339(ts: &OffsetDateTime) -> String {
    ts.format(&Rfc3339)
        .unwrap_or_else(|_| ts.unix_timestamp().to_string())
}

fn normalize_id(q: &str) -> String {
    q.trim()
        .chars()
        .filter(|c| !matches!(c, '-' | ' '))
        .collect::<String>()
        .to_ascii_uppercase()
}

// -------------------------------------------------------------------
// Resolve — universal over the global id space (ADR-035 §0)
// -------------------------------------------------------------------

fn resolve(ctx: &AppContext, query: &str) -> Response {
    // Default `parts` path first: rich Part projection + human-prefix
    // resolution.
    match resolve_part(ctx, query) {
        Ok(p) => return Response::ok(part_to_entity(&p)),
        // No contract → preserve the parts-specific error (unchanged).
        Err(e) if ctx.contract.is_none() => return e,
        Err(_) => {}
    }
    // Global id space (ADR-035 §0): search the declared non-parts
    // collections for an exact id match.
    if let Some(contract) = &ctx.contract {
        for coll in &contract.collections {
            if coll.name == "parts" {
                continue;
            }
            if let Ok(records) = ctx.repo.list_collection(&coll.name) {
                if let Some(rec) = records
                    .iter()
                    .find(|r| r.get("id").and_then(|v| v.as_str()) == Some(query))
                {
                    return Response::ok(entity_from_record(&coll.name, rec));
                }
            }
        }
    }
    Response::error(
        ErrorKind::NotFound,
        format!("id {query:?} not found in any declared collection"),
    )
}

fn resolve_part(ctx: &AppContext, query: &str) -> Result<Part, Response> {
    // Typed-id form: `scheme:value` (ADR-035 §0). Bare = the default
    // scheme (nano14).
    let bare = match query.split_once(':') {
        Some(("nano14", v)) => v.to_string(),
        Some((scheme, _)) => {
            return Err(Response::error(
                ErrorKind::Validation,
                format!("id scheme {scheme:?} is not declared (default: nano14)"),
            ));
        }
        None => query.to_string(),
    };
    let q = normalize_id(&bare);
    let parts = match ctx.repo.list_parts(&PartFilter::default()) {
        Ok(p) => p,
        Err(e) => return Err(Response::error(ErrorKind::Backend, e.to_string())),
    };
    if q.len() == PART_ID_LEN {
        return parts
            .into_iter()
            .find(|p| p.id.as_str() == q)
            .ok_or_else(|| {
                Response::error(ErrorKind::NotFound, format!("no match for {query:?}"))
            });
    }
    if q.len() >= HUMAN_PREFIX_LEN {
        let matches: Vec<Part> = parts
            .into_iter()
            .filter(|p| p.id.as_str().starts_with(&q))
            .collect();
        return match matches.len() {
            0 => Err(Response::error(
                ErrorKind::NotFound,
                format!("no match for {query:?}"),
            )),
            1 => Ok(matches.into_iter().next().expect("len checked")),
            n => Err(Response::error(
                ErrorKind::Ambiguous,
                format!(
                    "ambiguous prefix {query:?} — {n} matches: {}",
                    matches
                        .iter()
                        .map(|p| p.id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )),
        };
    }
    Err(Response::error(
        ErrorKind::BadRequest,
        format!(
            "query too short ({} chars); need >= {HUMAN_PREFIX_LEN}",
            q.len()
        ),
    ))
}

// -------------------------------------------------------------------
// List / Count — the one generic query (ADR-035 §0)
// -------------------------------------------------------------------

fn load_entities(ctx: &AppContext, collection: &str) -> Result<Vec<Entity>, Response> {
    // The default `parts` collection keeps its rich Part projection
    // (minted/bound timestamps). Other declared collections are served
    // generically from their JSONL records (ADR-035 entity store).
    if collection == "parts" {
        let parts = ctx
            .repo
            .list_parts(&PartFilter::default())
            .map_err(|e| Response::error(ErrorKind::Backend, e.to_string()))?;
        Ok(parts.iter().map(part_to_entity).collect())
    } else {
        let records = ctx
            .repo
            .list_collection(collection)
            .map_err(|e| Response::error(ErrorKind::Backend, e.to_string()))?;
        Ok(records
            .iter()
            .map(|r| entity_from_record(collection, r))
            .collect())
    }
}

/// Collections the read path (`list`/`count`) can serve: any the contract
/// declares (ADR-035 collections-metamodel), or just `parts` when no
/// contract is loaded. Distinct from [`known_collection`] — the mutation
/// guard stays parts-only until generic write lands.
fn served_collection(ctx: &AppContext, collection: &str) -> Result<(), Response> {
    if let Some(contract) = &ctx.contract {
        return if contract.collection(collection).is_some() {
            Ok(())
        } else {
            Err(Response::error(
                ErrorKind::Unsupported,
                format!("collection {collection:?} is not declared in this registry's contract"),
            ))
        };
    }
    if collection == "parts" {
        Ok(())
    } else {
        Err(Response::error(
            ErrorKind::Unsupported,
            format!("collection {collection:?} is not declared (preset roster: parts)"),
        ))
    }
}

fn apply_filter(entities: Vec<Entity>, filter: &Filter) -> Vec<Entity> {
    entities
        .into_iter()
        .filter(|e| {
            if let Some(s) = &filter.status {
                if e.status.as_deref() != Some(s.as_str()) {
                    return false;
                }
            }
            if let Some(k) = &filter.kind {
                if e.kind.as_deref() != Some(k.as_str()) {
                    return false;
                }
            }
            for (key, want) in &filter.fields {
                let have = field_value(e, key).unwrap_or_default().to_lowercase();
                if !have.contains(&want.to_lowercase()) {
                    return false;
                }
            }
            if let Some(text) = &filter.text {
                let needle = text.to_lowercase();
                let mut haystack = e.id.to_lowercase();
                for v in e.fields.values() {
                    haystack.push('\n');
                    haystack.push_str(&v.to_lowercase());
                }
                if !haystack.contains(&needle) {
                    return false;
                }
            }
            true
        })
        .collect()
}

fn apply_sort(entities: &mut [Entity], sort: &[Sort]) {
    // Stable sort, last key applied first so the first key dominates.
    for s in sort.iter().rev() {
        entities.sort_by(|a, b| {
            let va = field_value(a, &s.field).unwrap_or_default();
            let vb = field_value(b, &s.field).unwrap_or_default();
            match s.dir {
                SortDir::Asc => va.cmp(&vb),
                SortDir::Desc => vb.cmp(&va),
            }
        });
    }
}

fn list(
    ctx: &AppContext,
    collection: &str,
    filter: &Filter,
    sort: &[Sort],
    page: &Page,
) -> Response {
    if let Err(r) = served_collection(ctx, collection) {
        return r;
    }
    let entities = match load_entities(ctx, collection) {
        Ok(e) => e,
        Err(r) => return r,
    };
    let mut filtered = apply_filter(entities, filter);
    if sort.is_empty() {
        apply_sort(
            &mut filtered,
            &[Sort {
                field: "id".into(),
                dir: SortDir::Asc,
            }],
        );
    } else {
        apply_sort(&mut filtered, sort);
    }
    let total = filtered.len();
    let items: Vec<Entity> = filtered
        .into_iter()
        .skip(page.offset as usize)
        .take(page.limit as usize)
        .collect();
    Response::ok(json!({ "items": items, "total": total }))
}

fn count(ctx: &AppContext, collection: &str, filter: &Filter, by: &str) -> Response {
    if let Err(r) = served_collection(ctx, collection) {
        return r;
    }
    let entities = match load_entities(ctx, collection) {
        Ok(e) => e,
        Err(r) => return r,
    };
    let filtered = apply_filter(entities, filter);
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    for e in &filtered {
        let key = field_value(e, by).unwrap_or_else(|| "(none)".into());
        *counts.entry(key).or_insert(0) += 1;
    }
    Response::ok(json!({ "by": by, "counts": counts }))
}

// -------------------------------------------------------------------
// Describe — descriptors as introspectable data (ADR-035 §0)
// -------------------------------------------------------------------

fn describe(ctx: &AppContext, collection: Option<&str>) -> Response {
    // Always the same `{name, collections}` envelope; `collection`
    // narrows the roster (uniform client handling — one shape).

    // Contract-driven roster when a contract is loaded (ADR-035
    // collections-metamodel): the registry self-describes from its
    // declared collections, not the hard-coded preset.
    if let Some(contract) = &ctx.contract {
        let all: Vec<_> = contract
            .collections
            .iter()
            .map(descriptor_from_contract)
            .collect();
        return match collection {
            None => Response::ok(RegistryDescriptor {
                name: ctx.registry_name.clone(),
                collections: all,
            }),
            Some(name) => match all.into_iter().find(|d| d.name == name) {
                Some(d) => Response::ok(RegistryDescriptor {
                    name: ctx.registry_name.clone(),
                    collections: vec![d],
                }),
                None => Response::error(
                    ErrorKind::NotFound,
                    format!("collection {name:?} is not declared in this registry's contract"),
                ),
            },
        };
    }

    // No contract loaded (FE snapshot / tests): the code-owned preset roster.
    match collection {
        None => Response::ok(registry_descriptor(&ctx.registry_name)),
        Some("parts") => {
            let mut d = registry_descriptor(&ctx.registry_name);
            d.collections = vec![parts_descriptor()];
            Response::ok(d)
        }
        Some(other) => Response::error(
            ErrorKind::NotFound,
            format!("collection {other:?} is not declared (preset roster: parts)"),
        ),
    }
}

// -------------------------------------------------------------------
// Create — mint (parts)
// -------------------------------------------------------------------

/// Mint one fresh part id disjoint from `existing` (ADR-012; canonical
/// home of the generator per ADR-030 — shells reuse this through the
/// app layer).
pub fn mint_part_id(existing: &HashSet<String>) -> Result<PartId, String> {
    for _ in 0..16 {
        let candidate = generate_one();
        if !existing.contains(&candidate) {
            return PartId::new(candidate.clone())
                .map_err(|e| format!("minted candidate {candidate:?} failed validation: {e}"));
        }
    }
    Err("nanoid keeps colliding — registry corrupt or RNG broken".into())
}

fn generate_one() -> String {
    let alphabet: Vec<char> = PART_ID_ALPHABET.chars().collect();
    let n = alphabet.len() as u8;
    let limit = (u8::MAX / n) * n;
    let mut out = String::with_capacity(PART_ID_LEN);
    while out.chars().count() < PART_ID_LEN {
        let mut buf = [0u8; 32];
        getrandom::getrandom(&mut buf).expect("OS CSPRNG available");
        for &b in &buf {
            if b < limit {
                out.push(alphabet[(b % n) as usize]);
                if out.chars().count() == PART_ID_LEN {
                    break;
                }
            }
        }
    }
    out
}

fn operator(ctx: &AppContext) -> Result<Operator, Response> {
    ctx.identity
        .current()
        .map_err(|e| Response::error(ErrorKind::Auth, e.to_string()))
}

/// Generic entity-store create (ADR-035): mint a nano14 id disjoint from
/// the collection's existing records, build a record from `fields`, and
/// submit a `record_writes` proposal targeting
/// `collections/<collection>.jsonl`. Only nano14-scheme collections mint
/// here; imported schemes (udi/gs1) would supply their own id.
fn generic_create(
    ctx: &AppContext,
    collection: &str,
    fields: &BTreeMap<String, String>,
) -> Response {
    if let Err(r) = served_collection(ctx, collection) {
        return r;
    }
    let scheme = ctx
        .contract
        .as_ref()
        .and_then(|c| c.collection(collection))
        .map(|c| c.id.scheme.clone())
        .unwrap_or_else(|| "nano14".to_string());
    if scheme != "nano14" {
        return Response::error(
            ErrorKind::Unsupported,
            format!("create for id scheme {scheme:?} not yet supported (mint covers nano14)"),
        );
    }
    let op = match operator(ctx) {
        Ok(o) => o,
        Err(r) => return r,
    };
    let existing: HashSet<String> = match ctx.repo.list_collection(collection) {
        Ok(recs) => recs
            .iter()
            .filter_map(|r| r.get("id").and_then(|v| v.as_str()).map(String::from))
            .collect(),
        Err(e) => return Response::error(ErrorKind::Backend, e.to_string()),
    };
    let id = match mint_part_id(&existing) {
        Ok(p) => p.as_str().to_string(),
        Err(e) => return Response::error(ErrorKind::Backend, e),
    };
    let mut record = serde_json::Map::new();
    record.insert("id".to_string(), serde_json::Value::String(id.clone()));
    // A lifecycle collection's new record starts at the declared initial
    // status (ADR-035 — e.g. a JSONL-native part mints at `unbound`).
    if let Some(initial) = ctx
        .contract
        .as_ref()
        .and_then(|c| c.collection(collection))
        .and_then(|c| c.lifecycle.as_ref())
        .map(|lc| lc.initial.clone())
    {
        record.insert("status".to_string(), serde_json::Value::String(initial));
    }
    for (k, v) in fields {
        record.insert(k.clone(), serde_json::Value::String(v.clone()));
    }
    let diff = Diff {
        record_writes: vec![RecordWrite {
            collection: collection.to_string(),
            id: id.clone(),
            record,
        }],
        ..Diff::default()
    };
    let request_id = RequestId::new();
    let proposal = Proposal {
        diff: diff.clone(),
        batch_label: None,
        author: op,
        signatures: Vec::new(),
        change_classification: diff.classify(),
        message: format!("create {collection}: {id}"),
        request_id,
    };
    match ctx.sink.submit(proposal) {
        Ok(proposal_ref) => Response::ok(json!({
            "id": id,
            "collection": collection,
            "proposal": proposal_ref.url,
        })),
        Err(e) => Response::error(ErrorKind::Backend, e.to_string()),
    }
}

/// Find a record by id in a declared collection's JSONL store, or a
/// NotFound response.
fn fetch_collection_record(
    ctx: &AppContext,
    collection: &str,
    id: &str,
) -> Result<serde_json::Map<String, serde_json::Value>, Response> {
    let records = ctx
        .repo
        .list_collection(collection)
        .map_err(|e| Response::error(ErrorKind::Backend, e.to_string()))?;
    records
        .into_iter()
        .find(|r| r.get("id").and_then(|v| v.as_str()) == Some(id))
        .ok_or_else(|| {
            Response::error(
                ErrorKind::NotFound,
                format!("{collection} record {id:?} not found"),
            )
        })
}

/// Submit a single generic record upsert (ADR-035 `record_writes` channel).
fn submit_record_write(
    ctx: &AppContext,
    op: Operator,
    collection: &str,
    id: &str,
    record: serde_json::Map<String, serde_json::Value>,
    message: String,
) -> Response {
    let diff = Diff {
        record_writes: vec![RecordWrite {
            collection: collection.to_string(),
            id: id.to_string(),
            record,
        }],
        ..Diff::default()
    };
    let request_id = RequestId::new();
    let proposal = Proposal {
        diff: diff.clone(),
        batch_label: None,
        author: op,
        signatures: Vec::new(),
        change_classification: diff.classify(),
        message,
        request_id,
    };
    match ctx.sink.submit(proposal) {
        Ok(proposal_ref) => Response::ok(json!({
            "id": id,
            "collection": collection,
            "proposal": proposal_ref.url,
        })),
        Err(e) => Response::error(ErrorKind::Backend, e.to_string()),
    }
}

/// Generic entity-store edit (ADR-035): merge `fields` into an existing
/// record and re-write its JSONL line.
fn generic_edit(
    ctx: &AppContext,
    collection: &str,
    id: &str,
    fields: &BTreeMap<String, String>,
) -> Response {
    if let Err(r) = served_collection(ctx, collection) {
        return r;
    }
    if fields.is_empty() {
        return Response::error(ErrorKind::BadRequest, "Edit requires at least one field");
    }
    let op = match operator(ctx) {
        Ok(o) => o,
        Err(r) => return r,
    };
    let mut record = match fetch_collection_record(ctx, collection, id) {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    for (k, v) in fields {
        record.insert(k.clone(), serde_json::Value::String(v.clone()));
    }
    submit_record_write(
        ctx,
        op,
        collection,
        id,
        record,
        format!("edit {collection}: {id}"),
    )
}

/// Generic entity-store transition (ADR-035): move a record to status
/// `to` if the collection's lifecycle allows it from the current status.
fn generic_transition(
    ctx: &AppContext,
    collection: &str,
    id: &str,
    to: &str,
    fields: &BTreeMap<String, String>,
) -> Response {
    if let Err(r) = served_collection(ctx, collection) {
        return r;
    }
    let op = match operator(ctx) {
        Ok(o) => o,
        Err(r) => return r,
    };
    let Some(lc) = ctx
        .contract
        .as_ref()
        .and_then(|c| c.collection(collection))
        .and_then(|c| c.lifecycle.clone())
    else {
        return Response::error(
            ErrorKind::BadRequest,
            format!("collection {collection:?} has no lifecycle to transition"),
        );
    };
    let mut record = match fetch_collection_record(ctx, collection, id) {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    let current = record
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or(&lc.initial)
        .to_string();
    let allowed = lc
        .transitions
        .get(&current)
        .map(|tos| tos.iter().any(|t| t == to))
        .unwrap_or(false);
    if !allowed {
        return Response::error(
            ErrorKind::Validation,
            format!("transition {current:?} -> {to:?} not allowed for {collection}"),
        );
    }
    record.insert(
        "status".to_string(),
        serde_json::Value::String(to.to_string()),
    );
    for (k, v) in fields {
        record.insert(k.clone(), serde_json::Value::String(v.clone()));
    }
    submit_record_write(
        ctx,
        op,
        collection,
        id,
        record,
        format!("transition {collection}: {id} -> {to}"),
    )
}

fn create(
    ctx: &AppContext,
    collection: &str,
    n: Option<u32>,
    fields: &BTreeMap<String, String>,
) -> Response {
    // Generic entity-store create for declared non-parts collections
    // (ADR-035): mint an id and write a record with the given fields,
    // via the JSONL `record_writes` channel. Parts keep their blank-mint
    // semantics below (mint-then-bind, ADR-012).
    if collection != "parts" {
        return generic_create(ctx, collection, fields);
    }
    if let Err(r) = known_collection(ctx, collection) {
        return r;
    }
    if !fields.is_empty() {
        return Response::error(
            ErrorKind::Validation,
            "Create{parts} mints blank unbound ids; metadata binds via Transition (mint-then-bind, ADR-012)",
        );
    }
    let count = n.unwrap_or(1);
    if count < 1 {
        return Response::error(ErrorKind::BadRequest, "n must be >= 1");
    }
    let op = match operator(ctx) {
        Ok(o) => o,
        Err(r) => return r,
    };

    let now = OffsetDateTime::now_utc();
    let now_iso = rfc3339(&now);
    let existing_parts = match ctx.repo.list_parts(&PartFilter::default()) {
        Ok(p) => p,
        Err(e) => return Response::error(ErrorKind::Backend, e.to_string()),
    };
    let mut existing: HashSet<String> = existing_parts
        .iter()
        .map(|p| p.id.as_str().to_owned())
        .collect();

    let mut new_ids: Vec<PartId> = Vec::with_capacity(count as usize);
    for _ in 0..count {
        match mint_part_id(&existing) {
            Ok(id) => {
                existing.insert(id.as_str().to_owned());
                new_ids.push(id);
            }
            Err(e) => return Response::error(ErrorKind::Backend, e),
        }
    }

    // Diff: N RowAdds. The legacy `batch` column is still part of the
    // stored schema pre-migration (ADR-035 retires it; obligation
    // `batch-deprecated`) — populate it with the timestamp-derived
    // label so existing validators stay green.
    let legacy_batch = format!(
        "B-{:04}-{:02}-{:02}-{:02}{:02}",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute()
    );
    let mut adds = Vec::with_capacity(new_ids.len());
    for id in &new_ids {
        let mut f = BTreeMap::new();
        f.insert("status".into(), "unbound".into());
        f.insert("minted_at".into(), now_iso.clone());
        f.insert("batch".into(), legacy_batch.clone());
        adds.push(DiffRow {
            id: Some(id.clone()),
            fields: f,
        });
    }
    let diff = Diff {
        adds,
        ..Diff::default()
    };
    let request_id = RequestId::new();
    let proposal = Proposal {
        diff: diff.clone(),
        batch_label: None,
        author: op.clone(),
        signatures: Vec::new(),
        change_classification: diff.classify(),
        message: format!("mint: {} new IDs", new_ids.len()),
        request_id,
    };
    let proposal_ref = match ctx.sink.submit(proposal) {
        Ok(r) => r,
        Err(e) => return Response::error(ErrorKind::Backend, e.to_string()),
    };

    for id in &new_ids {
        let extra = json!({ "proposal": proposal_ref.url, "created_at": now_iso });
        let entry = mint_audit_entry(request_id, op.clone(), id.clone(), extra);
        emit_audit(&entry);
        if let Err(e) = ctx.repo.append_audit_event(entry) {
            tracing::warn!(error = %e, "append_audit_event failed; tracing layer is the fallback");
        }
    }

    Response::ok(json!({
        "minted": new_ids.iter().map(|i| i.as_str()).collect::<Vec<_>>(),
        "created_at": now_iso,
        "proposal": proposal_ref,
    }))
}

// -------------------------------------------------------------------
// Transition / Edit — the crisp boundary (ADR-035 §0):
// status-changing ⇒ Transition; status-preserving ⇒ Edit.
// -------------------------------------------------------------------

const EDITABLE_KEYS: [&str; 6] = [
    "type",
    "description",
    "vendor",
    "part_number",
    "location",
    "notes",
];

fn validate_field_keys(fields: &BTreeMap<String, String>) -> Result<(), Response> {
    for k in fields.keys() {
        if !EDITABLE_KEYS.contains(&k.as_str()) {
            return Err(Response::error(
                ErrorKind::Validation,
                format!(
                    "unknown field {k:?}; editable fields: {}",
                    EDITABLE_KEYS.join(", ")
                ),
            ));
        }
    }
    Ok(())
}

fn part_field_map(p: &Part) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    m.insert("status".into(), p.status.to_string());
    let mut put = |k: &str, v: &Option<String>| {
        if let Some(v) = v {
            m.insert(k.to_string(), v.clone());
        }
    };
    put("type", &p.type_);
    put("description", &p.description);
    put("vendor", &p.vendor);
    put("part_number", &p.part_number);
    put("location", &p.location);
    put("notes", &p.notes);
    m
}

fn submit_edit_diff(
    ctx: &AppContext,
    op: &Operator,
    target: &Part,
    before: BTreeMap<String, String>,
    after: BTreeMap<String, String>,
    message: String,
) -> Result<(ProposalRef, RequestId), Response> {
    let changed_keys: Vec<String> = after
        .iter()
        .filter(|(k, v)| before.get(k.as_str()) != Some(*v))
        .map(|(k, _)| k.clone())
        .collect();
    let diff = Diff {
        edits: vec![DiffEdit {
            id: target.id.clone(),
            before,
            after,
            changed_keys,
        }],
        ..Diff::default()
    };
    let request_id = RequestId::new();
    let proposal = Proposal {
        diff: diff.clone(),
        batch_label: None,
        author: op.clone(),
        signatures: Vec::new(),
        change_classification: diff.classify(),
        message,
        request_id,
    };
    let proposal_ref = ctx
        .sink
        .submit(proposal)
        .map_err(|e| Response::error(ErrorKind::Backend, e.to_string()))?;
    Ok((proposal_ref, request_id))
}

fn transition(
    ctx: &AppContext,
    collection: &str,
    id: &str,
    to: &str,
    fields: &BTreeMap<String, String>,
) -> Response {
    if collection != "parts" {
        return generic_transition(ctx, collection, id, to, fields);
    }
    if let Err(r) = known_collection(ctx, collection) {
        return r;
    }
    if let Err(r) = validate_field_keys(fields) {
        return r;
    }
    let op = match operator(ctx) {
        Ok(o) => o,
        Err(r) => return r,
    };
    let target = match resolve_part(ctx, id) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let now = OffsetDateTime::now_utc();
    let now_iso = rfc3339(&now);

    match to {
        "bound" => {
            if target.status == PartStatus::Bound {
                return Response::error(
                    ErrorKind::Validation,
                    format!(
                        "{} is already bound — status-preserving changes go through Edit",
                        target.id
                    ),
                );
            }
            if target.status == PartStatus::Void {
                return Response::error(
                    ErrorKind::Validation,
                    format!("{} is voided; cannot bind. Mint a new ID.", target.id),
                );
            }
            let before = part_field_map(&target);
            let mut after = BTreeMap::new();
            after.insert("status".into(), "bound".into());
            after.insert("bound_at".into(), now_iso.clone());
            for k in EDITABLE_KEYS {
                let new = fields.get(k).cloned();
                let old = before.get(k).cloned();
                if let Some(v) = new.or(old) {
                    after.insert(k.into(), v);
                }
            }
            let (proposal_ref, request_id) = match submit_edit_diff(
                ctx,
                &op,
                &target,
                before,
                after.clone(),
                format!("bind: {}", target.id),
            ) {
                Ok(x) => x,
                Err(r) => return r,
            };
            let extra = json!({ "proposal": proposal_ref.url, "bound_at": now_iso });
            let entry = bind_audit_entry(request_id, op, target.id.clone(), after, extra);
            emit_audit(&entry);
            ctx.repo.append_audit_event(entry).ok();
            Response::ok(json!({
                "id": target.id.as_str(),
                "to": "bound",
                "proposal": proposal_ref,
            }))
        }
        "void" => {
            let reason = match fields.get("notes") {
                Some(n) => format!("{n} [voided {now_iso}]"),
                None => format!("[voided {now_iso}]"),
            };
            let mut before = BTreeMap::new();
            before.insert("status".into(), target.status.to_string());
            if let Some(n) = &target.notes {
                before.insert("notes".into(), n.clone());
            }
            let mut after = BTreeMap::new();
            after.insert("status".into(), "void".into());
            after.insert("notes".into(), reason.clone());
            let (proposal_ref, request_id) = match submit_edit_diff(
                ctx,
                &op,
                &target,
                before,
                after,
                format!("void: {}", target.id),
            ) {
                Ok(x) => x,
                Err(r) => return r,
            };
            let extra = json!({ "proposal": proposal_ref.url, "voided_at": now_iso });
            let entry = void_audit_entry(request_id, op, target.id.clone(), reason, extra);
            emit_audit(&entry);
            ctx.repo.append_audit_event(entry).ok();
            Response::ok(json!({
                "id": target.id.as_str(),
                "to": "void",
                "proposal": proposal_ref,
            }))
        }
        other => Response::error(
            ErrorKind::Validation,
            format!(
                "unknown transition target {other:?}; parts lifecycle: unbound -> bound -> void"
            ),
        ),
    }
}

fn edit(
    ctx: &AppContext,
    collection: &str,
    id: &str,
    fields: &BTreeMap<String, String>,
) -> Response {
    if collection != "parts" {
        return generic_edit(ctx, collection, id, fields);
    }
    if let Err(r) = known_collection(ctx, collection) {
        return r;
    }
    if fields.is_empty() {
        return Response::error(ErrorKind::BadRequest, "Edit requires at least one field");
    }
    if let Err(r) = validate_field_keys(fields) {
        return r;
    }
    let op = match operator(ctx) {
        Ok(o) => o,
        Err(r) => return r,
    };
    let target = match resolve_part(ctx, id) {
        Ok(p) => p,
        Err(r) => return r,
    };
    if target.status != PartStatus::Bound {
        return Response::error(
            ErrorKind::Validation,
            format!(
                "{} is {} — Edit is status-preserving and applies to bound parts; \
                 use Transition to bind or void",
                target.id, target.status
            ),
        );
    }
    let before = part_field_map(&target);
    let mut after = before.clone();
    for (k, v) in fields {
        after.insert(k.clone(), v.clone());
    }
    let (proposal_ref, request_id) = match submit_edit_diff(
        ctx,
        &op,
        &target,
        before.clone(),
        after.clone(),
        format!("edit: {}", target.id),
    ) {
        Ok(x) => x,
        Err(r) => return r,
    };
    // RowEdit audit (the bind/mint/void constructors don't cover plain
    // edits; build the entry directly).
    let entry = qx_domain::AuditEntry {
        request_id,
        timestamp: OffsetDateTime::now_utc(),
        actor: op,
        action: qx_domain::Action::RowEdit {
            id: target.id.clone(),
            before,
            after,
        },
        target: qx_domain::TargetRef::Part {
            id: target.id.clone(),
        },
        before: None,
        after: None,
        extra: json!({ "proposal": proposal_ref.url }),
        signatures: Vec::new(),
        chain_hash: None,
    };
    emit_audit(&entry);
    ctx.repo.append_audit_event(entry).ok();
    Response::ok(json!({
        "id": target.id.as_str(),
        "proposal": proposal_ref,
    }))
}

// -------------------------------------------------------------------
// Print — render is universal; delivery is the shell's (ADR-031)
// -------------------------------------------------------------------

/// Default dots-per-inch for the ADR-031 §3 mm → px conversion when
/// the request carries none. 300 dpi ≈ the Brother QL class of
/// thermal heads; the per-printer profile default is an ADR-031 open
/// question, so until that lands this is the documented fallback.
const DEFAULT_DPI: f64 = 300.0;

const MM_PER_INCH: f64 = 25.4;

fn print(
    ctx: &AppContext,
    collection: &str,
    selection: &Selection,
    options: &PrintOptions,
) -> Response {
    if let Err(r) = known_collection(ctx, collection) {
        return r;
    }
    if options.copies < 1 {
        return Response::error(ErrorKind::BadRequest, "copies must be >= 1");
    }
    match options.unit.as_str() {
        "mm" => print_mm(ctx, selection, options),
        "px" => print_px(ctx, selection, options),
        other => Response::error(
            ErrorKind::Validation,
            format!("unknown unit {other:?}; units: mm, px (ADR-031 §3)"),
        ),
    }
}

/// Human-ID grouping → `TextFormat` (+ legibility warning).
/// `legibility_mm` is the *physical* label size the warning tiers are
/// defined over — for px-mode requests it is the dpi-converted size.
fn parse_chars(chars: &str, legibility_mm: f64) -> Result<(TextFormat, Option<String>), Response> {
    match chars {
        "auto" => Ok(recommend_format(legibility_mm)),
        "44" => Ok((
            TextFormat::FourFour,
            check_format_warning(legibility_mm, TextFormat::FourFour),
        )),
        "444" => Ok((
            TextFormat::FourFourFour,
            check_format_warning(legibility_mm, TextFormat::FourFourFour),
        )),
        "554" => Ok((
            TextFormat::FiveFiveFour,
            check_format_warning(legibility_mm, TextFormat::FiveFiveFour),
        )),
        other => Err(Response::error(
            ErrorKind::Validation,
            format!("unknown chars grouping {other:?}; nano14 declares: 44, 444, 554, auto"),
        )),
    }
}

fn chars_name(fmt: TextFormat) -> &'static str {
    match fmt {
        TextFormat::FourFour => "44",
        TextFormat::FourFourFour => "444",
        TextFormat::FiveFiveFour => "554",
    }
}

/// The requested symbology (ADR-031 §8): the canonical compact string
/// when given, else the deprecated `micro` flag mapped to its family
/// (`micro: true` == symbology "micro") — `symbology` wins when both
/// are present.
fn requested_symbology(options: &PrintOptions) -> Result<Symbology, Response> {
    match options.symbology.as_deref() {
        Some(s) => s
            .parse()
            .map_err(|e: String| Response::error(ErrorKind::Validation, e)),
        None => Ok(Symbology::family(if options.micro {
            Family::Micro
        } else {
            Family::Qr
        })),
    }
}

/// Selection → parts (shared by the mm and px render paths).
fn select_targets(ctx: &AppContext, selection: &Selection) -> Result<Vec<Part>, Response> {
    let targets = match selection {
        Selection::Ids(ids) => {
            let mut out = Vec::with_capacity(ids.len());
            for id in ids {
                out.push(resolve_part(ctx, id)?);
            }
            out
        }
        Selection::Filter(filter) => {
            let entities = load_entities(ctx, "parts")?;
            let selected = apply_filter(entities, filter);
            let mut out = Vec::with_capacity(selected.len());
            for e in &selected {
                out.push(resolve_part(ctx, &e.id)?);
            }
            out
        }
    };
    if targets.is_empty() {
        return Err(Response::error(
            ErrorKind::NotFound,
            "selection matched no entities",
        ));
    }
    Ok(targets)
}

/// The original mm-native render path — behavior unchanged from
/// pre-ADR-031-§2 (`unit: "mm"`, the default). Version/EC pins are
/// print-contract parameters the px renderer consumes; here only the
/// symbology *family* selects between the fixed mm pins (micro →
/// M4/EC-M, qr → V1/EC-M).
fn print_mm(ctx: &AppContext, selection: &Selection, options: &PrintOptions) -> Response {
    let symbology = match requested_symbology(options) {
        Ok(s) => s,
        Err(r) => return r,
    };
    if symbology.version.is_some() || symbology.ec.is_some() {
        return Response::error(
            ErrorKind::Validation,
            format!(
                "symbology {symbology:?} pins version/EC — print-contract parameters \
                 the px-true renderer consumes (ADR-031 §8); unit \"mm\" renders the \
                 family default only. Drop the pin or switch to unit \"px\"",
                symbology = symbology.to_string(),
            ),
        );
    }
    // §10 repeat primitives compose on the px-true path only — the
    // mm renderer would silently drop them, which is worse than
    // refusing (same staging rule as the symbology pin above).
    if options.repeat.is_some()
        || options.repeat_axis.is_some()
        || options.repeat_gap_px.is_some()
        || options.repeat_orient.is_some()
        || options.length_px.is_some()
        || options.spacing.is_some()
        || options.rotate.is_some()
        || options.length_excess_px.is_some()
        || options.excess_at.is_some()
    {
        return Response::error(
            ErrorKind::Validation,
            "repeat/rotate/length compose on the px-true renderer only (ADR-031 §10) \
             — switch to a px size (8mm @300dpi ≈ 94px) or drop the repeat flags",
        );
    }
    let micro = symbology.family == Family::Micro;
    // The mm renderer's fixed pins, reported as resolved evidence.
    let resolved = if micro { "micro-m4-m" } else { "qr-v1-m" };
    let layout = match options.layout.as_str() {
        "vert" => Layout::Vert,
        "horz" => Layout::Horz,
        "flag" => match options.cable_od_mm {
            Some(od) => Layout::Flag {
                cable_od_mm: od,
                no_markers: false,
                alignment_line: false,
            },
            None => {
                return Response::error(
                    ErrorKind::Validation,
                    "layout \"flag\" requires cable_od_mm",
                );
            }
        },
        other => {
            return Response::error(
                ErrorKind::Validation,
                format!("unknown layout {other:?}; presets: vert, horz, flag"),
            );
        }
    };
    let (text_format, warning) = match parse_chars(&options.chars, options.size_mm) {
        Ok(x) => x,
        Err(r) => return r,
    };
    let targets = match select_targets(ctx, selection) {
        Ok(t) => t,
        Err(r) => return r,
    };

    // ADR-031 §10 colors land on the mm path too — when fg/bg are
    // explicitly set the new `render_with` entry emits the receipt
    // metadata + bg rect (default white = the §10 fix to the
    // accidental transparency bug). Absent colors keep the legacy
    // byte-identical output for golden parity.
    let (fg_mm, bg_mm, color_warning) = match resolve_colors(options) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let use_color_entry = options.fg.is_some() || options.bg.is_some();
    let mut labels = Vec::with_capacity(targets.len());
    for p in &targets {
        let svg = if use_color_entry {
            let receipt = mm_receipt(
                p.id.as_str(),
                resolved,
                &options.layout,
                options.size_mm,
                &fg_mm,
                &bg_mm,
            );
            let metadata_body = qx_codec::receipt::metadata_element(&receipt);
            let mm_opts = svg_mod::MmRenderOpts {
                fg: &fg_mm.svg,
                bg: &bg_mm.svg,
                metadata: Some(&metadata_body),
            };
            svg_mod::render_with(
                p.id.as_str(),
                layout,
                options.size_mm,
                text_format,
                micro,
                &mm_opts,
            )
        } else {
            match render_label(p.id.as_str(), layout, options.size_mm, text_format, micro) {
                Ok(s) => s,
                Err(e) => return Response::error(ErrorKind::Backend, e.to_string()),
            }
        };
        labels.push(json!({
            "id": p.id.as_str(),
            "svg": svg,
            "symbology": resolved,
        }));
    }

    if options.log {
        let printed_by = match operator(ctx) {
            Ok(o) => OperatorRef(o.id),
            Err(r) => return r,
        };
        let now = OffsetDateTime::now_utc();
        for p in &targets {
            let ev = PrintEvent {
                id: p.id.clone(),
                printed_at: now,
                printed_by: printed_by.clone(),
                layout: options.layout.clone(),
                size_mm: options.size_mm,
                extra: json!({ "chars": options.chars, "micro": micro, "symbology": resolved }),
                copies: options.copies,
                output_mode: "app-dispatch-svg".into(),
                batch_label: None,
            };
            if let Err(e) = ctx.repo.append_print_event(ev) {
                tracing::warn!(error = %e, "append_print_event failed");
            }
        }
    }

    let combined = combine_warnings(warning.as_deref(), color_warning.as_deref());
    Response::ok(json!({
        "labels": labels,
        "size_mm": options.size_mm,
        "chars": chars_name(text_format),
        "fg": fg_mm.svg,
        "bg": bg_mm.svg,
        "warning": combined,
    }))
}

/// Build an mm-path receipt. The mm renderer has no module_px /
/// glyph_px (Consolas via SVG `<text>`); receipt values that the px
/// renderer carries get zeroed out as "n/a in this renderer". This
/// keeps the single-Receipt-type SSOT (used by both renderers) but
/// honestly marks the mm path's reduced detail.
fn mm_receipt(
    canonical: &str,
    symbology: &str,
    layout: &str,
    size_mm: f64,
    fg: &Color,
    bg: &Color,
) -> qx_codec::Receipt {
    qx_codec::Receipt {
        id: canonical.into(),
        // Stage 1: mm always renders the legacy qr+id arrangement,
        // so shape/payload is documented as "qr id" for the receipt.
        payload: format!("qr id (layout {layout})"),
        symbology: symbology.into(),
        // The mm path is mm-native, so size_px is reported as the
        // nearest dpi-equivalent at 300dpi for receipt comparability.
        size_px: (size_mm / MM_PER_INCH * DEFAULT_DPI).round() as u32,
        padding: [0, 0, 0, 0],
        padding_mode: "n/a".into(),
        size_mode: "exact".into(),
        qr_px: 0,
        module_px: 0,
        glyph_px: 0,
        fg: fg.svg.clone(),
        bg: bg.svg.clone(),
        font: "Consolas".into(),
        generator: qx_codec::receipt::generator(),
        // The mm path refuses repeat flags outright (px-only).
        repeat: None,
    }
}

/// The ADR-031 §2/§8 px-true render path (`unit: "px"`). Native unit
/// is device px; an mm-expressed size converts at `dpi` and the codec
/// deduces the module size per `padding_mode` (quiet-zone accounting,
/// §8). The whole job then fills to the batch's largest footprint
/// (§4).
fn print_px(ctx: &AppContext, selection: &Selection, options: &PrintOptions) -> Response {
    let dpi = options.dpi.unwrap_or(DEFAULT_DPI);
    if !dpi.is_finite() || dpi <= 0.0 {
        return Response::error(
            ErrorKind::Validation,
            format!("dpi must be a positive number, got {dpi}"),
        );
    }
    // ADR-031 §10 colors: parse + collect a contrast/polarity warning.
    let (fg_color, bg_color, color_warning) = match resolve_colors(options) {
        Ok(t) => t,
        Err(r) => return r,
    };
    // ADR-031 §8 size-mode: exact (default) or snap.
    let size_mode = match options.size_mode.as_deref() {
        None | Some("exact") => SizeMode::Exact,
        Some("snap") => SizeMode::Snap,
        Some(other) => {
            return Response::error(
                ErrorKind::Validation,
                format!("unknown size_mode {other:?}; modes: exact, snap (ADR-031 §8)"),
            );
        }
    };
    // ADR-031 §10 payload DSL (stage 2: nested groups + canvas at
    // root). Parse the structured tree once and dispatch.
    let payload_tree = match options
        .payload
        .as_deref()
        .map(payload_dsl::parse_tree)
        .transpose()
    {
        Ok(p) => p,
        Err(e) => return Response::error(ErrorKind::Validation, e),
    };
    // Stage 2 canvas: validate + surface as Unsupported for the
    // composed render (the resolved tree is the receipt). Full
    // canvas-aware rendering is the future ROI step.
    if let Some(qx_codec::PayloadNode::Canvas {
        width,
        height,
        children,
    }) = &payload_tree
    {
        let resolved = match qx_codec::resolve_canvas(*width, *height, children, dpi) {
            Ok(r) => r,
            Err(e) => return Response::error(ErrorKind::Validation, e),
        };
        return Response::ok(json!({
            "labels": [],
            "unit": "px",
            "canvas": resolved,
            "warning": if resolved.overlaps.is_empty() {
                None
            } else {
                Some(resolved.overlaps.join("; "))
            },
            "note": "canvas geometry validated; full render is a future step (ADR-031 §10 stage 2 minimum-viable surface)",
        }));
    }
    // For non-canvas trees, flatten into the existing flat-list path —
    // the regression-pin "qr id" still parses to the same byte-
    // identical render.
    let payload_elements = match payload_tree.as_ref() {
        Some(tree) => match payload_dsl::flatten(tree) {
            Ok(v) => Some(v),
            Err(e) => return Response::error(ErrorKind::Validation, e),
        },
        None => None,
    };
    // §3: size_px (the EXACT output canvas) is direct; otherwise
    // mm → px at `dpi` defines the canvas. The codec deduces the
    // module size inside it (§2/§8, 2026-06-11).
    let size_px = match options.size_px {
        Some(px) => px,
        None => (options.size_mm / MM_PER_INCH * dpi).round() as u32,
    };
    // §8: per-side padding floors (CSS shorthand on the wire; the
    // pre-shorthand plain integer still deserializes as uniform).
    let padding: Padding = options
        .padding_px
        .map(PaddingSpec::expand)
        .unwrap_or_default();
    // §8: symbology — family[-version][-ec], auto-fit where unpinned.
    // the deprecated `micro` flag maps to its family when absent.
    let symbology = match requested_symbology(options) {
        Ok(s) => s,
        Err(r) => return r,
    };
    // §8: how the quiet zone counts toward the padding floor.
    let padding_mode = match options.padding_mode.as_deref() {
        None | Some("overlap") => PaddingMode::Overlap,
        Some("additive") => PaddingMode::Additive,
        Some("clip") => PaddingMode::Clip,
        Some(other) => {
            return Response::error(
                ErrorKind::Validation,
                format!(
                    "unknown padding_mode {other:?}; modes: overlap, additive, clip (ADR-031 §8)"
                ),
            );
        }
    };
    // px mode does not require cable_od_mm up front: the codec rejects
    // the flag layout itself (Unsupported, ADR-031 §5) with the
    // authoritative message — UNLESS the deprecated --layout flag +
    // --cable-od sugar is firing (ADR-031 §10), in which case the
    // per-label render is `horz` and the repeat composer materializes
    // the flag geometry from repeat 2 / linear / alternate.
    let layout = match options.layout.as_str() {
        "vert" => Layout::Vert,
        "horz" => Layout::Horz,
        "flag" if options.cable_od_mm.is_some() => Layout::Horz,
        "flag" => Layout::Flag {
            cable_od_mm: options.cable_od_mm.unwrap_or(0.0),
            no_markers: false,
            alignment_line: false,
        },
        other => {
            return Response::error(
                ErrorKind::Validation,
                format!("unknown layout {other:?}; presets: vert, horz, flag"),
            );
        }
    };
    // Legibility tiers are physical-mm rules; evaluate them at the
    // physical size this canvas maps to.
    let legibility_mm = f64::from(size_px) * MM_PER_INCH / dpi;
    let (text_format, warning) = match parse_chars(&options.chars, legibility_mm) {
        Ok(x) => x,
        Err(r) => return r,
    };
    let targets = match select_targets(ctx, selection) {
        Ok(t) => t,
        Err(r) => return r,
    };

    // §10 payload shape: from --payload if present, else from
    // --content/legacy flags (effective "qr id" today).
    let shape = match resolve_payload_shape(payload_elements.as_deref()) {
        Ok(s) => s,
        Err(r) => return r,
    };

    let mut rendered: Vec<PxLabel> = Vec::with_capacity(targets.len());
    for p in &targets {
        // §10 solver: with --id-chars/--rows/--id-size set, override
        // the text_format-derived id-block. Otherwise the g-law runs.
        let solver_block = match resolve_solver_block(
            options,
            layout,
            shape,
            size_px,
            padding,
            padding_mode,
            &symbology,
            p.id.as_str(),
        ) {
            Ok(b) => b,
            Err(r) => return r,
        };
        let render_opts = RenderOpts {
            fg: fg_color.clone(),
            bg: bg_color.clone(),
            size_mode,
            shape,
            id_block: solver_block,
            embed_metadata: true,
        };
        match render_label_px_with_opts(
            p.id.as_str(),
            layout,
            size_px,
            text_format,
            &symbology,
            padding,
            padding_mode,
            &render_opts,
        ) {
            Ok(l) => rendered.push(l),
            Err(e) => return px_codec_error(e),
        }
    }
    // §4: padding is a floor; the job fills to the largest footprint
    // so a mixed batch comes out physically uniform.
    fill_to_max(&mut rendered, padding);

    // §10 repeat primitives: compose copies (orthogonal to single-label
    // sizing). Also handles deprecated `--layout flag` + `--cable-od`
    // sugar, which expands to repeat 2 / linear / alternate.
    let (repeat_opts, deprecation_warning) = match resolve_repeat_opts(options, dpi) {
        Ok(x) => x,
        Err(r) => return r,
    };
    let mut composed: Vec<Option<qx_codec::RepeatComposed>> = Vec::with_capacity(rendered.len());
    if let Some(opts) = &repeat_opts {
        for l in &rendered {
            match compose_repeat(l, opts) {
                Ok(c) => composed.push(Some(c)),
                Err(e) => return px_codec_error(e),
            }
        }
    }

    let labels: Vec<serde_json::Value> = targets
        .iter()
        .zip(&rendered)
        .enumerate()
        .map(|(i, (p, l))| {
            // When repeat is active, swap in the composed SVG + dims
            // — the per-label receipt records the resolved repeat object.
            let (svg, width_px, height_px, repeat_field) =
                match composed.get(i).and_then(|c| c.as_ref()) {
                    Some(c) => (
                        c.svg.clone(),
                        c.width_px,
                        c.height_px,
                        Some(c.resolved.clone()),
                    ),
                    None => (l.svg.clone(), l.width_px, l.height_px, None),
                };
            json!({
                "id": p.id.as_str(),
                "svg": svg,
                "width_px": width_px,
                "height_px": height_px,
                "qr_px": l.qr_px,
                "module_px": l.module_px,
                "data_px": l.data_px,
                "glyph_px": l.glyph_px,
                "glyph_cell": l.glyph_cell,
                "white": l.white,
                "padding_mode": l.padding_mode,
                "symbology": l.symbology,
                "receipt": l.receipt,
                "repeat": repeat_field,
            })
        })
        .collect();

    if options.log {
        let printed_by = match operator(ctx) {
            Ok(o) => OperatorRef(o.id),
            Err(r) => return r,
        };
        let now = OffsetDateTime::now_utc();
        for (p, l) in targets.iter().zip(&rendered) {
            let ev = PrintEvent {
                id: p.id.clone(),
                printed_at: now,
                printed_by: printed_by.clone(),
                layout: options.layout.clone(),
                // The physical size the snapped symbol maps to at `dpi`
                // — resolved params are the audit evidence (ADR-031 §7).
                size_mm: f64::from(l.qr_px) * MM_PER_INCH / dpi,
                extra: json!({
                    "chars": options.chars,
                    "symbology": l.symbology,
                    "unit": "px",
                    "qr_px": l.qr_px,
                    "module_px": l.module_px,
                    "data_px": l.data_px,
                    "white": l.white,
                    "padding": padding,
                    "padding_mode": l.padding_mode,
                    "dpi": dpi,
                }),
                copies: options.copies,
                output_mode: "app-dispatch-svg".into(),
                batch_label: None,
            };
            if let Err(e) = ctx.repo.append_print_event(ev) {
                tracing::warn!(error = %e, "append_print_event failed");
            }
        }
    }

    let mut combined_warning = combine_warnings(warning.as_deref(), color_warning.as_deref());
    if let Some(w) = deprecation_warning {
        combined_warning = combine_warnings(combined_warning.as_deref(), Some(w.as_str()));
    }
    Response::ok(json!({
        "labels": labels,
        "unit": "px",
        "size_px": size_px,
        "size_mode": size_mode_name(size_mode),
        "padding": padding,
        "padding_mode": padding_mode,
        "fg": fg_color.svg,
        "bg": bg_color.svg,
        "dpi": dpi,
        "chars": chars_name(text_format),
        "warning": combined_warning,
    }))
}

/// Map a px-render codec failure into the protocol taxonomy: an
/// undersized target is caller-fixable (`Validation`, carries the
/// minimum-size hint), as is an infeasible symbology/payload pairing
/// (`Encode`, carries the §8 feasibility hint); a not-yet-implemented
/// mode is `Unsupported`.
fn px_codec_error(e: CodecError) -> Response {
    match e {
        CodecError::Unsupported(m) => Response::error(ErrorKind::Unsupported, m),
        CodecError::Render(m) => Response::error(ErrorKind::Validation, m),
        CodecError::Encode(m) => Response::error(ErrorKind::Validation, m),
        other => Response::error(ErrorKind::Backend, other.to_string()),
    }
}

/// Parse `--fg`/`--bg` and assemble the contrast/polarity WARN
/// (ADR-031 §10).
fn resolve_colors(options: &PrintOptions) -> Result<(Color, Color, Option<String>), Response> {
    let fg = match options.fg.as_deref() {
        Some(s) => color::parse(s, false).map_err(|e| Response::error(ErrorKind::Validation, e))?,
        None => color::default_fg(),
    };
    let bg = match options.bg.as_deref() {
        Some(s) => color::parse(s, true).map_err(|e| Response::error(ErrorKind::Validation, e))?,
        None => color::default_bg(),
    };
    let warn = color::warning(&fg, &bg);
    Ok((fg, bg, warn))
}

/// Resolve the payload shape from the explicit DSL list (stage 1:
/// flat list — `qr`, `id`, `qr id`, `id qr`).
fn resolve_payload_shape(
    elements: Option<&[qx_codec::PayloadElement]>,
) -> Result<PayloadShape, Response> {
    let Some(els) = elements else {
        return Ok(PayloadShape::QrId);
    };
    // Stage 1 supports the four exhaustive arrangements; anything
    // else (e.g. a `space` element between qr and id) is fine
    // structurally — space is treated as a zero-flex gap stage 1 —
    // but the shape stays one of the four.
    let mut has_qr = false;
    let mut has_id = false;
    let mut qr_first = false;
    let mut id_first = false;
    for (i, e) in els.iter().enumerate() {
        match e {
            qx_codec::PayloadElement::Qr { .. } => {
                if !has_qr {
                    qr_first = i == 0 || (!has_id);
                    has_qr = true;
                }
            }
            qx_codec::PayloadElement::Id { .. } => {
                if !has_id {
                    id_first = i == 0 || (!has_qr);
                    has_id = true;
                }
            }
            qx_codec::PayloadElement::Space { .. } => {}
        }
    }
    match (has_qr, has_id) {
        (true, true) if qr_first && !id_first => Ok(PayloadShape::QrId),
        (true, true) if id_first && !qr_first => Ok(PayloadShape::IdQr),
        (true, true) => Ok(PayloadShape::QrId),
        (true, false) => Ok(PayloadShape::QrOnly),
        (false, true) => Ok(PayloadShape::IdOnly),
        (false, false) => Err(Response::error(
            ErrorKind::Validation,
            "payload: stage 1 requires at least one of qr or id",
        )),
    }
}

/// Convert PrintOptions's solver knobs (--id-chars / --rows /
/// --id-size) into the codec solver's [`SolverInputs`] and solve.
/// Returns `None` when no knob is set (the codec falls back to the
/// g-law). Errors are §10 Validation with the nearest-feasible hint.
#[allow(clippy::too_many_arguments)]
fn resolve_solver_block(
    options: &PrintOptions,
    layout: Layout,
    shape: PayloadShape,
    size_px: u32,
    padding: Padding,
    padding_mode: PaddingMode,
    symbology: &Symbology,
    canonical: &str,
) -> Result<Option<IdBlock>, Response> {
    if options.id_chars.is_none() && options.rows.is_none() && options.id_size_px.is_none() {
        return Ok(None);
    }
    if matches!(shape, PayloadShape::QrOnly) {
        return Err(Response::error(
            ErrorKind::Validation,
            "id solver inputs (--id-chars/--rows/--id-size) require an id element \
             in the payload — the qr-only shape has no id text",
        ));
    }
    // Compute the per-layout text budget. Id-only shape: the budget is
    // the whole canvas along the layout axis (minus pad floors).
    let (budget, glyph_cap) = if matches!(shape, PayloadShape::IdOnly) {
        let pad_a = match layout {
            Layout::Horz => padding.top + padding.bottom,
            Layout::Vert => padding.left + padding.right,
            Layout::Flag { .. } => 0,
        };
        (size_px.saturating_sub(pad_a), None)
    } else {
        // Need the module_px to know the budget + g cap.
        let (resolved, matrix) = symbology
            .resolve(canonical)
            .map_err(|e| Response::error(ErrorKind::Validation, e.to_string()))?;
        let data = matrix.size as u32;
        let quiet = resolved.quiet_modules();
        let (pad_a, pad_b) = match layout {
            Layout::Horz => (padding.top, padding.bottom),
            Layout::Vert => (padding.left, padding.right),
            Layout::Flag { .. } => (0, 0),
        };
        let module_px = deduce_module_px(size_px, pad_a, pad_b, data, quiet, padding_mode);
        if module_px < 1 {
            return Err(Response::error(
                ErrorKind::Validation,
                format!(
                    "id solver: cannot run before module_px is deducible \
                     (size {size_px}px does not fit one px/module under \
                     padding_mode {})",
                    padding_mode_name(padding_mode),
                ),
            ));
        }
        (data * module_px, Some(module_px))
    };
    let inputs = SolverInputs {
        chars: options.id_chars,
        rows: options.rows,
        glyph_px: options.id_size_px,
        glyph_px_cap: glyph_cap,
        chars_max: canonical.chars().count() as u32,
    };
    match solver_mod::solve(inputs, budget) {
        Ok(b) => Ok(Some(b)),
        Err(e) => Err(Response::error(ErrorKind::Validation, e.message)),
    }
}

/// Resolve the ADR-031 §10 repeat primitives from the request
/// options. Returns `(Some(opts), Some(warning))` when the deprecated
/// `--layout flag` + `--cable-od` sugar fires; `(None, None)` when no
/// repeat is requested.
fn resolve_repeat_opts(
    options: &PrintOptions,
    dpi: f64,
) -> Result<(Option<RepeatOpts>, Option<String>), Response> {
    // Deprecated sugar: layout=flag + cable_od_mm → repeat 2 alternate.
    // Only fires on the px path (where this fn runs); when the user
    // ALSO sets --repeat explicitly, the explicit form wins (the
    // deprecation note still rides as a warning).
    let flag_sugar = if options.layout == "flag" {
        options.cable_od_mm.map(|od| deprecated_flag_sugar(od, dpi))
    } else {
        None
    };
    if options.repeat.is_none()
        && options.repeat_gap_px.is_none()
        && options.length_px.is_none()
        && options.rotate.is_none()
        && options.length_excess_px.is_none()
        && flag_sugar.is_none()
    {
        return Ok((None, None));
    }
    let (mut opts, deprecation) = match flag_sugar {
        Some((o, w)) => (o, Some(w)),
        None => (RepeatOpts::default(), None),
    };
    if let Some(s) = options.repeat.as_deref() {
        opts.count =
            parse_repeat_count(s).map_err(|e| Response::error(ErrorKind::Validation, e))?;
    }
    if let Some(s) = options.repeat_axis.as_deref() {
        opts.axis = match s {
            "along" => RepeatAxis::Along,
            "across" => RepeatAxis::Across,
            other => {
                return Err(Response::error(
                    ErrorKind::Validation,
                    format!("unknown --repeat-axis {other:?}; values: along, across"),
                ));
            }
        };
    }
    if let Some(g) = options.repeat_gap_px {
        opts.gap_px = Some(g);
    }
    if let Some(s) = options.repeat_orient.as_deref() {
        opts.orient = match s {
            "same" => Orient::Same,
            "alternate" => Orient::Alternate,
            other => {
                return Err(Response::error(
                    ErrorKind::Validation,
                    format!("unknown --repeat-orient {other:?}; values: same, alternate"),
                ));
            }
        };
    }
    if let Some(l) = options.length_px {
        opts.length_px = Some(l);
    }
    if let Some(s) = options.spacing.as_deref() {
        opts.spacing = match s {
            "linear" => Spacing::Linear,
            "cyclic" => Spacing::Cyclic,
            other => {
                return Err(Response::error(
                    ErrorKind::Validation,
                    format!("unknown --spacing {other:?}; values: linear, cyclic"),
                ));
            }
        };
    }
    if let Some(r) = options.rotate {
        opts.rotate = Rotate::from_deg(r).map_err(|e| Response::error(ErrorKind::Validation, e))?;
    }
    if let Some(ex) = options.length_excess_px {
        opts.excess_px = ex;
    }
    if let Some(s) = options.excess_at.as_deref() {
        opts.excess_at = match s {
            "start" => ExcessAt::Start,
            "end" => ExcessAt::End,
            other => {
                return Err(Response::error(
                    ErrorKind::Validation,
                    format!("unknown --excess-at {other:?}; values: start, end"),
                ));
            }
        };
    }
    Ok((Some(opts), deprecation))
}

/// `--repeat <n|fill>` parser. `n` must be ≥ 1.
fn parse_repeat_count(s: &str) -> Result<RepeatCount, String> {
    let t = s.trim();
    if t == "fill" {
        return Ok(RepeatCount::Fill);
    }
    let n: u32 = t
        .parse()
        .map_err(|_| format!("--repeat {t:?}: expected a positive integer or \"fill\""))?;
    if n < 1 {
        return Err(format!("--repeat {n}: must be >= 1"));
    }
    Ok(RepeatCount::N(n))
}

fn padding_mode_name(m: PaddingMode) -> &'static str {
    match m {
        PaddingMode::Overlap => "overlap",
        PaddingMode::Additive => "additive",
        PaddingMode::Clip => "clip",
    }
}

fn size_mode_name(m: SizeMode) -> &'static str {
    match m {
        SizeMode::Exact => "exact",
        SizeMode::Snap => "snap",
    }
}

/// Re-implementation of the codec's private `deduce_module_px` for the
/// engine's solver-budget calculation — kept in sync structurally by
/// the px-render tests (overlap/additive/clip boundary tables).
fn deduce_module_px(
    size_px: u32,
    pad_a: u32,
    pad_b: u32,
    data: u32,
    quiet: u32,
    mode: PaddingMode,
) -> u32 {
    let mut m = 1u32;
    let mut best = 0u32;
    loop {
        let floor_a = match mode {
            PaddingMode::Overlap => pad_a.max(quiet * m),
            PaddingMode::Additive => quiet * m + pad_a,
            PaddingMode::Clip => pad_a,
        };
        let floor_b = match mode {
            PaddingMode::Overlap => pad_b.max(quiet * m),
            PaddingMode::Additive => quiet * m + pad_b,
            PaddingMode::Clip => pad_b,
        };
        let need = data * m + floor_a + floor_b;
        if need > size_px {
            break;
        }
        best = m;
        m += 1;
    }
    best
}

/// Compose multiple per-axis warning strings into one — the response
/// `warning` field carries `null`, a single string, or a "; "-joined
/// concatenation of every active warning.
fn combine_warnings(a: Option<&str>, b: Option<&str>) -> Option<String> {
    match (a, b) {
        (None, None) => None,
        (Some(a), None) => Some(a.into()),
        (None, Some(b)) => Some(b.into()),
        (Some(a), Some(b)) => Some(format!("{a}; {b}")),
    }
}

// -------------------------------------------------------------------
// Export — generated artifact, never committed (ADR-035 §0)
// -------------------------------------------------------------------

fn export(ctx: &AppContext, collection: &str, format: &str) -> Response {
    if let Err(r) = known_collection(ctx, collection) {
        return r;
    }
    if format != "csv" {
        return Response::error(
            ErrorKind::Unsupported,
            format!("export format {format:?} not supported (formats: csv)"),
        );
    }
    let entities = match load_entities(ctx, "parts") {
        Ok(e) => e,
        Err(r) => return r,
    };
    let columns = [
        "id",
        "status",
        "created_at",
        "type",
        "description",
        "vendor",
        "part_number",
        "location",
        "notes",
    ];
    let mut csv = columns.join(",");
    csv.push('\n');
    for e in &entities {
        let row: Vec<String> = columns
            .iter()
            .map(|c| csv_escape(&field_value(e, c).unwrap_or_default()))
            .collect();
        csv.push_str(&row.join(","));
        csv.push('\n');
    }
    Response::ok(json!({ "format": "csv", "content": csv, "rows": entities.len() }))
}

fn csv_escape(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

// -------------------------------------------------------------------
// PollProposal / Whoami
// -------------------------------------------------------------------

fn poll_proposal(ctx: &AppContext, proposal: &ProposalRef) -> Response {
    match ctx.sink.status(proposal) {
        Ok(status) => Response::ok(json!({ "status": status })),
        Err(e) => Response::error(ErrorKind::Backend, e.to_string()),
    }
}

fn whoami(ctx: &AppContext) -> Response {
    match ctx.identity.current() {
        Ok(op) => {
            // `source` rides as the IdentitySource serde shape (not a
            // Rust Debug string) so the wire form is a stable protocol
            // value.
            let source = serde_json::to_value(&op.source)
                .unwrap_or_else(|_| serde_json::Value::String("unknown".into()));
            Response::ok(json!({
                "id": op.id.0,
                "display_name": op.display_name,
                "source": source,
                "verified_at": op.verified_at.map(|t| rfc3339(&t)),
            }))
        }
        Err(e) => Response::error(ErrorKind::Auth, e.to_string()),
    }
}
