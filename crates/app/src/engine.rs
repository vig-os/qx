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

use part_registry_codec::{
    check_format_warning, fill_to_max, recommend_format, render_label, render_label_px, CodecError,
    Family, Layout, Padding, PaddingMode, PxLabel, Symbology, TextFormat,
};
use part_registry_domain::{
    Diff, DiffEdit, DiffRow, Operator, OperatorRef, Part, PartId, PartStatus, PrintEvent, Proposal,
    ProposalRef, RequestId, PART_ID_ALPHABET, PART_ID_LEN,
};
use part_registry_identity::IdentityProvider;
use part_registry_observability::{
    bind_audit_entry, emit_audit, mint_audit_entry, void_audit_entry,
};
use part_registry_storage::{PartFilter, Repository};
use part_registry_transport::ProposalSink;

use crate::entity::{field_value, part_to_entity, Entity};
use crate::preset::{parts_descriptor, registry_descriptor};
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
    match resolve_part(ctx, query) {
        Ok(p) => Response::ok(part_to_entity(&p)),
        Err(r) => r,
    }
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

fn load_entities(ctx: &AppContext) -> Result<Vec<Entity>, Response> {
    let parts = ctx
        .repo
        .list_parts(&PartFilter::default())
        .map_err(|e| Response::error(ErrorKind::Backend, e.to_string()))?;
    Ok(parts.iter().map(part_to_entity).collect())
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
    if let Err(r) = known_collection(ctx, collection) {
        return r;
    }
    let entities = match load_entities(ctx) {
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
    if let Err(r) = known_collection(ctx, collection) {
        return r;
    }
    let entities = match load_entities(ctx) {
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

fn create(
    ctx: &AppContext,
    collection: &str,
    n: Option<u32>,
    fields: &BTreeMap<String, String>,
) -> Response {
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
    let entry = part_registry_domain::AuditEntry {
        request_id,
        timestamp: OffsetDateTime::now_utc(),
        actor: op,
        action: part_registry_domain::Action::RowEdit {
            id: target.id.clone(),
            before,
            after,
        },
        target: part_registry_domain::TargetRef::Part {
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
            let entities = load_entities(ctx)?;
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

    let mut labels = Vec::with_capacity(targets.len());
    for p in &targets {
        match render_label(p.id.as_str(), layout, options.size_mm, text_format, micro) {
            Ok(svg) => labels.push(json!({
                "id": p.id.as_str(),
                "svg": svg,
                "symbology": resolved,
            })),
            Err(e) => return Response::error(ErrorKind::Backend, e.to_string()),
        }
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

    Response::ok(json!({
        "labels": labels,
        "size_mm": options.size_mm,
        "chars": chars_name(text_format),
        "warning": warning,
    }))
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
    // authoritative message.
    let layout = match options.layout.as_str() {
        "vert" => Layout::Vert,
        "horz" => Layout::Horz,
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

    let mut rendered: Vec<PxLabel> = Vec::with_capacity(targets.len());
    for p in &targets {
        match render_label_px(
            p.id.as_str(),
            layout,
            size_px,
            text_format,
            &symbology,
            padding,
            padding_mode,
        ) {
            Ok(l) => rendered.push(l),
            Err(e) => return px_codec_error(e),
        }
    }
    // §4: padding is a floor; the job fills to the largest footprint
    // so a mixed batch comes out physically uniform.
    fill_to_max(&mut rendered, padding);

    let labels: Vec<serde_json::Value> = targets
        .iter()
        .zip(&rendered)
        .map(|(p, l)| {
            json!({
                "id": p.id.as_str(),
                "svg": l.svg,
                "width_px": l.width_px,
                "height_px": l.height_px,
                "qr_px": l.qr_px,
                "module_px": l.module_px,
                "data_px": l.data_px,
                "glyph_px": l.glyph_px,
                "glyph_cell": l.glyph_cell,
                "white": l.white,
                "padding_mode": l.padding_mode,
                "symbology": l.symbology,
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

    Response::ok(json!({
        "labels": labels,
        "unit": "px",
        "size_px": size_px,
        "padding": padding,
        "padding_mode": padding_mode,
        "dpi": dpi,
        "chars": chars_name(text_format),
        "warning": warning,
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
    let entities = match load_entities(ctx) {
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
