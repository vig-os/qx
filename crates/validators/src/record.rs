//! Schema-driven record validation (ADR-039 §3-4) — the SSOT that makes
//! ONE engine validate every collection. Given a [`Collection`]
//! descriptor from the parsed contract and a record (a decoded NDJSON
//! line as a JSON object), [`validate_record`] checks the record against
//! the descriptor: type conformance, facets, the enum/reference policy,
//! foreign-key integrity, nested objects, and the lifecycle presence
//! gate (`required` / `required_to_enter`).
//!
//! Two surfaces, one function (ADR-039 §4):
//!
//! - **CI / `qx check`** — authoritative, full id universe → FK enforced.
//! - **FE preflight** — advisory; the universe may be partial. A target
//!   collection absent from [`RecordContext::known_ids`] means "universe
//!   unknown here" and the FK check is skipped, never falsely failed.
//!
//! Pure: no I/O, no clock. Compiles native + wasm32 like the rest of the
//! validator surface.
//!
//! Every §2 scalar facet is enforced here, including `pattern` regex.
//! The one remaining gap is `$ref` object-schema resolution (M-A.2,
//! paired with the `$defs` block — see ADR-039 open questions).

use std::collections::{BTreeMap, BTreeSet};

use qx_contract::{
    Closed, Collection, Contract, Field, FieldType, IdScheme, ObjectSchema, OnUnknown, VoidPolicy,
};
use serde_json::{Map, Value};

/// Severity of a single record issue. `Warn` never blocks a merge; it is
/// surfaced for the author (e.g. `closed: "warn"`, `on_unknown: "warn"`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warn,
}

/// One problem found in a record, addressed by a dotted field path
/// (`"address.country"`) so a shell can attach it to the right control.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordIssue {
    pub path: String,
    pub message: String,
    pub severity: Severity,
}

impl RecordIssue {
    fn error(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
            severity: Severity::Error,
        }
    }
    fn warn(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
            severity: Severity::Warn,
        }
    }
}

/// External facts the validator needs but cannot derive from the record:
/// the id universe per collection, for reference foreign-key checks.
///
/// A collection MISSING from `known_ids` means "the universe is not
/// loaded here" — FK checks against it are skipped (the FE-preflight
/// case). A collection PRESENT with an empty set means "loaded, and
/// empty" — every reference to it is an orphan.
#[derive(Clone, Debug, Default)]
pub struct RecordContext {
    pub known_ids: BTreeMap<String, BTreeSet<String>>,
    /// Per-kind tier-2 field schemas (ADR-035 §5), inheritance already
    /// resolved (each entry = the kind's own fields PLUS its ancestors').
    /// A record whose `kind` is present here is validated against these
    /// fields in addition to its collection's tier-1 core. Empty = no
    /// kind tree loaded (validation dispatches on `kind` only when known).
    pub kind_schemas: BTreeMap<String, Vec<Field>>,
}

impl RecordContext {
    /// Build a context from `(collection, ids)` pairs.
    pub fn new(universe: BTreeMap<String, BTreeSet<String>>) -> Self {
        Self {
            known_ids: universe,
            kind_schemas: BTreeMap::new(),
        }
    }

    /// Attach the resolved per-kind field schemas (the kind tree).
    pub fn with_kind_schemas(mut self, kind_schemas: BTreeMap<String, Vec<Field>>) -> Self {
        self.kind_schemas = kind_schemas;
        self
    }
}

/// Validate `record` against `collection`. `status` is the record's
/// current lifecycle status (if the collection has a lifecycle), used to
/// evaluate `required_to_enter` presence gates. Returns EVERY issue in
/// one pass (errors and warnings interleaved), empty == clean.
pub fn validate_record(
    collection: &Collection,
    record: &Map<String, Value>,
    status: Option<&str>,
    ctx: &RecordContext,
) -> Vec<RecordIssue> {
    let mut issues = Vec::new();
    // Tier-2: the record's kind (ADR-035 §5) contributes its resolved
    // per-kind fields on top of the collection's tier-1 core. Validation
    // dispatches on `kind`: an unknown/absent kind contributes nothing.
    let kind_fields: &[Field] = record
        .get("kind")
        .and_then(Value::as_str)
        .and_then(|k| ctx.kind_schemas.get(k))
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let declared: BTreeSet<&str> = collection
        .fields
        .iter()
        .chain(kind_fields)
        .map(|f| f.key.as_str())
        .collect();

    // Unknown keys: allowed only when the collection opts into tier-3
    // open_properties (ADR-035 §1). `id` and `status` are engine-owned
    // envelope keys, never declared as fields.
    // Engine-owned envelope keys — never declared as fields, never part
    // of the tier-3 open-properties bag (`transitioned_at` is a status→ts
    // object, the rest are scalars).
    const ENVELOPE: [&str; 6] = [
        "id",
        "status",
        "created_at",
        "transitioned_at",
        "label",
        "kind",
    ];
    if !collection.open_properties {
        for key in record.keys() {
            if ENVELOPE.contains(&key.as_str()) {
                continue;
            }
            if !declared.contains(key.as_str()) {
                issues.push(RecordIssue::error(
                    key.clone(),
                    format!("unknown field `{key}` (collection does not allow open properties)"),
                ));
            }
        }
    } else {
        // The tier-3 open-properties bag is shape-checked (ADR-035 §3):
        // it holds flat scalars. Structured data (objects/arrays) must be
        // a declared tier-2 field, or promoted into one — it cannot hide
        // in the open bag.
        for (key, value) in record {
            if declared.contains(key.as_str()) || ENVELOPE.contains(&key.as_str()) {
                continue;
            }
            if value.is_object() || value.is_array() {
                issues.push(RecordIssue::error(
                    key.clone(),
                    format!(
                        "open property `{key}` must be a scalar (string/number/bool); \
                         structured data needs a declared field"
                    ),
                ));
            }
        }
    }

    // Envelope `id` must conform to the collection's declared id scheme
    // (ADR-035 §0 typed ids). Absent here = a pre-mint draft (FE), skipped.
    if let Some(id) = record.get("id").and_then(Value::as_str) {
        check_id_format(&collection.id, id, &mut issues);
    }

    // Validate the collection's tier-1 core fields AND the record's kind's
    // tier-2 fields (ADR-035 §5 — validation dispatches on kind).
    for field in collection.fields.iter().chain(kind_fields) {
        validate_field_value(
            field,
            record.get(&field.key),
            &field.key,
            status,
            ctx,
            &mut issues,
        );
    }

    issues
}

/// Check the envelope `id` against the collection's declared id scheme
/// (ADR-035 §0). Minted schemes (`nano14`) and content-addresses
/// (`sha256`) are format-checked; imported/asserted schemes (`udi`,
/// `gs1`, …) are owned by an external authority, so any non-empty value
/// is accepted.
fn check_id_format(scheme: &IdScheme, id: &str, issues: &mut Vec<RecordIssue>) {
    match scheme.scheme.as_str() {
        "nano14" => {
            if id.chars().count() != qx_domain::PART_ID_LEN {
                issues.push(RecordIssue::error(
                    "id",
                    format!(
                        "id `{id}` is not nano14 ({} chars, expected {})",
                        id.chars().count(),
                        qx_domain::PART_ID_LEN
                    ),
                ));
            } else if let Some(bad) = id
                .chars()
                .find(|c| !qx_domain::PART_ID_ALPHABET.contains(*c))
            {
                issues.push(RecordIssue::error(
                    "id",
                    format!("id `{id}` has char `{bad}` outside the nano14 alphabet"),
                ));
            }
        }
        "sha256" => {
            if !is_sha256_ref(id) {
                issues.push(RecordIssue::error(
                    "id",
                    format!("id `{id}` is not a `sha256:<64 hex>` content-address"),
                ));
            }
        }
        _ => {
            if id.is_empty() {
                issues.push(RecordIssue::error("id", "id must not be empty"));
            }
        }
    }
}

/// Validate one field's value (which may be absent). `path` is the dotted
/// address for messages (top-level == the key; nested == `parent.key`).
fn validate_field_value(
    field: &Field,
    value: Option<&Value>,
    path: &str,
    status: Option<&str>,
    ctx: &RecordContext,
    issues: &mut Vec<RecordIssue>,
) {
    // Presence gates first.
    let present = !matches!(value, None | Some(Value::Null));
    if !present {
        if field.required == Some(true) {
            issues.push(RecordIssue::error(path, "required field is missing"));
        }
        if let (Some(gate), Some(cur)) = (field.required_to_enter.as_deref(), status) {
            if gate == cur {
                issues.push(RecordIssue::error(
                    path,
                    format!("must be present to enter status `{gate}`"),
                ));
            }
        }
        return; // nothing more to check on an absent value
    }
    let value = value.unwrap();

    match field.type_ {
        FieldType::String => check_string(field, value, path, issues),
        FieldType::Integer => check_integer(field, value, path, issues),
        FieldType::Number => check_number(field, value, path, issues),
        FieldType::Decimal => check_decimal(field, value, path, issues),
        FieldType::Bool => {
            if !value.is_boolean() {
                issues.push(RecordIssue::error(path, "expected a boolean"));
            }
        }
        FieldType::Date => check_date(value, path, issues),
        FieldType::Timestamp => check_timestamp(value, path, issues),
        FieldType::Enum => check_enum(field, value, path, issues),
        FieldType::Reference => check_reference(field, value, path, ctx, issues),
        FieldType::Attachment => check_attachment(field, value, path, issues),
        FieldType::Object => check_object(field, value, path, status, ctx, issues),
    }
}

fn check_string(field: &Field, value: &Value, path: &str, issues: &mut Vec<RecordIssue>) {
    let Some(s) = value.as_str() else {
        issues.push(RecordIssue::error(path, "expected a string"));
        return;
    };
    if let Some(max) = field.max_length {
        if s.chars().count() as u64 > max {
            issues.push(RecordIssue::error(
                path,
                format!("exceeds maxLength {max} ({} chars)", s.chars().count()),
            ));
        }
    }
    if let Some(pat) = field.pattern.as_deref() {
        match regex::Regex::new(pat) {
            Ok(re) => {
                if !re.is_match(s) {
                    issues.push(RecordIssue::error(
                        path,
                        format!("`{s}` does not match pattern /{pat}/"),
                    ));
                }
            }
            // A bad pattern is a contract authoring error surfaced here
            // (the gate has the record in hand, not the contract loader).
            Err(e) => issues.push(RecordIssue::error(
                path,
                format!("contract pattern /{pat}/ is not a valid regex: {e}"),
            )),
        }
    }
}

fn check_integer(field: &Field, value: &Value, path: &str, issues: &mut Vec<RecordIssue>) {
    let n = match value {
        Value::Number(n) if n.is_i64() || n.is_u64() => n.as_f64().unwrap_or(f64::NAN),
        Value::Number(_) => {
            issues.push(RecordIssue::error(
                path,
                "expected an integer, found a fractional number",
            ));
            return;
        }
        _ => {
            issues.push(RecordIssue::error(path, "expected an integer"));
            return;
        }
    };
    check_min_max(field, n, path, issues);
}

fn check_number(field: &Field, value: &Value, path: &str, issues: &mut Vec<RecordIssue>) {
    let Some(n) = value.as_f64() else {
        issues.push(RecordIssue::error(path, "expected a number"));
        return;
    };
    check_min_max(field, n, path, issues);
}

/// Decimal travels as a STRING (to preserve precision over JSON floats)
/// or a JSON number; precision/scale are checked on the digit string.
fn check_decimal(field: &Field, value: &Value, path: &str, issues: &mut Vec<RecordIssue>) {
    let s = match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => {
            issues.push(RecordIssue::error(
                path,
                "expected a decimal (string or number)",
            ));
            return;
        }
    };
    let body = s.strip_prefix('-').unwrap_or(&s);
    let (int_part, frac_part) = match body.split_once('.') {
        Some((i, f)) => (i, f),
        None => (body, ""),
    };
    if int_part.is_empty()
        || !int_part.chars().all(|c| c.is_ascii_digit())
        || !frac_part.chars().all(|c| c.is_ascii_digit())
    {
        issues.push(RecordIssue::error(
            path,
            format!("not a valid decimal: `{s}`"),
        ));
        return;
    }
    if let Some(scale) = field.scale {
        if frac_part.len() as u32 > scale {
            issues.push(RecordIssue::error(
                path,
                format!("scale {} exceeds declared {scale}", frac_part.len()),
            ));
        }
    }
    if let Some(precision) = field.precision {
        let sig = int_part.trim_start_matches('0').len() + frac_part.len();
        if sig as u32 > precision {
            issues.push(RecordIssue::error(
                path,
                format!("precision {sig} exceeds declared {precision}"),
            ));
        }
    }
    if let Ok(n) = s.parse::<f64>() {
        check_min_max(field, n, path, issues);
    }
}

fn check_min_max(field: &Field, n: f64, path: &str, issues: &mut Vec<RecordIssue>) {
    if let Some(min) = field.min {
        if n < min {
            issues.push(RecordIssue::error(path, format!("{n} is below min {min}")));
        }
    }
    if let Some(max) = field.max {
        if n > max {
            issues.push(RecordIssue::error(path, format!("{n} is above max {max}")));
        }
    }
}

/// `YYYY-MM-DD`, structurally (no calendar validation without a date
/// crate — month/day ranges are checked, not month length).
fn check_date(value: &Value, path: &str, issues: &mut Vec<RecordIssue>) {
    let Some(s) = value.as_str() else {
        issues.push(RecordIssue::error(
            path,
            "expected a date string (YYYY-MM-DD)",
        ));
        return;
    };
    let ok = s.len() == 10
        && s.as_bytes()[4] == b'-'
        && s.as_bytes()[7] == b'-'
        && s[0..4].chars().all(|c| c.is_ascii_digit())
        && s[5..7].chars().all(|c| c.is_ascii_digit())
        && s[8..10].chars().all(|c| c.is_ascii_digit())
        && (1..=12).contains(&s[5..7].parse::<u8>().unwrap_or(0))
        && (1..=31).contains(&s[8..10].parse::<u8>().unwrap_or(0));
    if !ok {
        issues.push(RecordIssue::error(
            path,
            format!("malformed date `{s}` (want YYYY-MM-DD)"),
        ));
    }
}

/// ISO-8601-ish timestamp: a non-empty string containing a date and a
/// `T` separator. Full RFC-3339 parsing is the storage layer's job.
fn check_timestamp(value: &Value, path: &str, issues: &mut Vec<RecordIssue>) {
    let Some(s) = value.as_str() else {
        issues.push(RecordIssue::error(path, "expected a timestamp string"));
        return;
    };
    if s.len() < 10 || !s[0..4].chars().all(|c| c.is_ascii_digit()) {
        issues.push(RecordIssue::error(
            path,
            format!("malformed timestamp `{s}`"),
        ));
    }
}

fn check_enum(field: &Field, value: &Value, path: &str, issues: &mut Vec<RecordIssue>) {
    let Some(s) = value.as_str() else {
        issues.push(RecordIssue::error(path, "expected an enum string value"));
        return;
    };
    let known = field
        .values
        .as_ref()
        .map(|v| v.iter().any(|x| x == s))
        .unwrap_or(false);
    if known {
        return;
    }
    // Unknown value — policy from `closed` (enum's own facet), then
    // `on_unknown` as the override, default reject for a closed set.
    let policy = enum_policy(field);
    push_unknown(
        policy,
        path,
        format!("`{s}` is not in the declared value set"),
        issues,
    );
}

/// Resolve the effective unknown-value policy for an enum field.
fn enum_policy(field: &Field) -> OnUnknown {
    if let Some(p) = field.on_unknown {
        return p;
    }
    match &field.closed {
        Some(Closed::Flag(true)) => OnUnknown::Reject,
        Some(Closed::Mode(_)) => OnUnknown::Warn, // "warn"
        Some(Closed::Flag(false)) => OnUnknown::Create, // open set, accept
        None => OnUnknown::Reject,                // a value set with no policy is closed
    }
}

fn check_reference(
    field: &Field,
    value: &Value,
    path: &str,
    ctx: &RecordContext,
    issues: &mut Vec<RecordIssue>,
) {
    let Some(s) = value.as_str() else {
        issues.push(RecordIssue::error(path, "expected a reference id (string)"));
        return;
    };
    let Some(target) = field.collection.as_deref() else {
        // A structurally-valid contract always names the target
        // collection on a reference field; this guards a hand-built one.
        issues.push(RecordIssue::error(
            path,
            "reference field has no target collection",
        ));
        return;
    };
    // Universe not loaded for this target → skip (FE-preflight case).
    let Some(ids) = ctx.known_ids.get(target) else {
        return;
    };
    if ids.contains(s) {
        return;
    }
    let policy = field.on_unknown.unwrap_or(OnUnknown::Reject);
    push_unknown(
        policy,
        path,
        format!("references `{s}` not found in collection `{target}`"),
        issues,
    );
}

/// Apply an unknown-value policy: reject → error, warn → warning, create
/// → accepted silently (the host will mint the new value).
fn push_unknown(policy: OnUnknown, path: &str, msg: String, issues: &mut Vec<RecordIssue>) {
    match policy {
        OnUnknown::Reject => issues.push(RecordIssue::error(path, msg)),
        OnUnknown::Warn => issues.push(RecordIssue::warn(path, msg)),
        OnUnknown::Create => {}
    }
}

fn check_attachment(field: &Field, value: &Value, path: &str, issues: &mut Vec<RecordIssue>) {
    // ADR-035 §4: an attachment value is a content-addressed, out-of-line
    // object with `ref` (the `sha256:<64 hex>` integrity anchor), `name`
    // (the filename — carries the extension and the human download name),
    // and an optional `desc` caption. Never a bare string or stored path.
    let Some(obj) = value.as_object() else {
        issues.push(RecordIssue::error(
            path,
            "expected an attachment object { ref, name, desc? }",
        ));
        return;
    };
    match obj.get("ref").and_then(Value::as_str) {
        None => issues.push(RecordIssue::error(
            format!("{path}.ref"),
            "attachment requires a string `ref`",
        )),
        Some(r) if !is_sha256_ref(r) => issues.push(RecordIssue::error(
            format!("{path}.ref"),
            format!("attachment `ref` `{r}` is not a `sha256:<64 hex>` id"),
        )),
        Some(_) => {}
    }
    let name = obj.get("name").and_then(Value::as_str);
    if name.is_none() {
        issues.push(RecordIssue::error(
            format!("{path}.name"),
            "attachment requires a string `name`",
        ));
    }
    if let Some(d) = obj.get("desc") {
        if !d.is_string() {
            issues.push(RecordIssue::error(
                format!("{path}.desc"),
                "attachment `desc` must be a string",
            ));
        }
    }
    // The value shape is exactly ref/name/desc — no stored path/who/when.
    for k in obj.keys() {
        if k != "ref" && k != "name" && k != "desc" {
            issues.push(RecordIssue::error(
                format!("{path}.{k}"),
                format!("unknown attachment key `{k}`"),
            ));
        }
    }
    // The per-field extension constraint (`attachment(pdf|png)`) narrows
    // the media type via the `name`'s extension.
    if let (Some(constraint), Some(n)) = (field.constraint.as_deref(), name) {
        let allowed: Vec<&str> = constraint.split('|').map(str::trim).collect();
        let ext = n.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        if !allowed.iter().any(|a| a.eq_ignore_ascii_case(&ext)) {
            issues.push(RecordIssue::error(
                format!("{path}.name"),
                format!("attachment extension `{ext}` not in allowed set [{constraint}]"),
            ));
        }
    }
}

/// A `sha256:` content-address: the literal prefix + 64 lowercase hex.
fn is_sha256_ref(s: &str) -> bool {
    s.strip_prefix("sha256:")
        .map(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()))
        .unwrap_or(false)
}

fn check_object(
    field: &Field,
    value: &Value,
    path: &str,
    status: Option<&str>,
    ctx: &RecordContext,
    issues: &mut Vec<RecordIssue>,
) {
    let Some(obj) = value.as_object() else {
        issues.push(RecordIssue::error(path, "expected an object"));
        return;
    };
    let Some(schema) = &field.schema else {
        // A structurally-valid contract guarantees objects carry a schema.
        issues.push(RecordIssue::error(path, "object field has no schema"));
        return;
    };
    match schema {
        ObjectSchema::Fields(nested) => {
            let declared: BTreeSet<&str> = nested.iter().map(|f| f.key.as_str()).collect();
            for key in obj.keys() {
                if !declared.contains(key.as_str()) {
                    issues.push(RecordIssue::error(
                        format!("{path}.{key}"),
                        format!("unknown nested field `{key}`"),
                    ));
                }
            }
            for nf in nested {
                let child_path = format!("{path}.{}", nf.key);
                // Nested fields carry no independent lifecycle; `status`
                // still flows so a nested required_to_enter could gate,
                // but nested lifecycle flags are rejected at parse time.
                validate_field_value(nf, obj.get(&nf.key), &child_path, status, ctx, issues);
            }
        }
        ObjectSchema::Ref(_) => {
            // $ref resolution deferred (conformance task); accept shape.
        }
    }
}

/// Graph-integrity check for a collection's `acyclic` relations (ADR-035
/// §1a / `component-graph-integrity`). For each relation marked
/// `acyclic`, the field keyed by the relation name on each record holds
/// the outgoing edge target ids; a back-edge into the active path is an
/// Error. Cross-record (unlike [`validate_record`]). FK validity —
/// targets exist — is `validate_record`'s job; this only catches cycles.
pub fn validate_collection_graph(
    collection: &Collection,
    records: &[Map<String, Value>],
) -> Vec<RecordIssue> {
    let mut issues = Vec::new();
    for rel in &collection.relations {
        if !rel.acyclic {
            continue;
        }
        let mut adj: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for rec in records {
            let Some(id) = rec.get("id").and_then(Value::as_str) else {
                continue;
            };
            let edges = rec
                .get(&rel.name)
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            adj.insert(id.to_string(), edges);
        }
        if let Some(via) = first_cycle(&adj) {
            issues.push(RecordIssue::error(
                rel.name.clone(),
                format!(
                    "relation `{}` is declared acyclic but a cycle reaches `{via}`",
                    rel.name
                ),
            ));
        }
    }
    issues
}

/// DFS three-colour cycle detection. Returns the node a back-edge points
/// at (the witness) if the graph has a cycle, else `None`. Edges to
/// non-nodes (dangling FKs) are ignored — that is the FK check's concern.
fn first_cycle(adj: &BTreeMap<String, Vec<String>>) -> Option<String> {
    // 0 = white (unseen), 1 = gray (on the active path), 2 = black (done).
    fn dfs(
        node: &str,
        adj: &BTreeMap<String, Vec<String>>,
        color: &mut BTreeMap<String, u8>,
    ) -> Option<String> {
        color.insert(node.to_string(), 1);
        if let Some(nbrs) = adj.get(node) {
            for n in nbrs {
                match color.get(n).copied().unwrap_or(0) {
                    1 => return Some(n.clone()),
                    0 if adj.contains_key(n) => {
                        if let Some(c) = dfs(n, adj, color) {
                            return Some(c);
                        }
                    }
                    _ => {}
                }
            }
        }
        color.insert(node.to_string(), 2);
        None
    }
    let mut color: BTreeMap<String, u8> = BTreeMap::new();
    for node in adj.keys().cloned().collect::<Vec<_>>() {
        if color.get(&node).copied().unwrap_or(0) == 0 {
            if let Some(c) = dfs(&node, adj, &mut color) {
                return Some(c);
            }
        }
    }
    None
}

/// Cross-collection void-policy enforcement (ADR-035 §1a /
/// `component-graph-integrity`). When a record is voided
/// (`status == "void"`) but is still referenced through a relation whose
/// `void_policy` is `block` (Error) or `warn` (Warn), surface an issue
/// against the referencing record's relation. `cascade` is allowed — the
/// dependent edges are expected to be voided too. Takes every
/// collection's records keyed by name (the cross-collection universe
/// `qx check` already builds).
pub fn validate_void_policy(
    contract: &Contract,
    records: &BTreeMap<String, Vec<Map<String, Value>>>,
) -> Vec<RecordIssue> {
    let mut issues = Vec::new();
    for source in &contract.collections {
        let Some(source_recs) = records.get(&source.name) else {
            continue;
        };
        for rel in &source.relations {
            let severity = match rel.void_policy {
                VoidPolicy::Block => Severity::Error,
                VoidPolicy::Warn => Severity::Warn,
                VoidPolicy::Cascade => continue,
            };
            let Some(target_recs) = records.get(&rel.target) else {
                continue;
            };
            let voided: BTreeSet<&str> = target_recs
                .iter()
                .filter(|r| r.get("status").and_then(Value::as_str) == Some("void"))
                .filter_map(|r| r.get("id").and_then(Value::as_str))
                .collect();
            if voided.is_empty() {
                continue;
            }
            for rec in source_recs {
                let src_id = rec.get("id").and_then(Value::as_str).unwrap_or("?");
                for target_id in relation_edges(rec, &rel.name) {
                    if voided.contains(target_id) {
                        let msg = format!(
                            "relation `{}`: `{src_id}` references voided `{target_id}` (void_policy)",
                            rel.name
                        );
                        issues.push(match severity {
                            Severity::Error => RecordIssue::error(rel.name.clone(), msg),
                            Severity::Warn => RecordIssue::warn(rel.name.clone(), msg),
                        });
                    }
                }
            }
        }
    }
    issues
}

/// The outgoing edge ids a record holds under a relation field, accepting
/// either a single id string (many-one) or an array of ids (many-many).
fn relation_edges<'a>(rec: &'a Map<String, Value>, field: &str) -> Vec<&'a str> {
    match rec.get(field) {
        Some(Value::String(s)) => vec![s.as_str()],
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qx_contract::Contract;

    const EXAMPLE: &str = include_str!("../../../schema/contract.example.json");

    fn contract() -> Contract {
        Contract::from_bytes(EXAMPLE.as_bytes()).unwrap()
    }

    fn obj(json: &str) -> Map<String, Value> {
        match serde_json::from_str(json).unwrap() {
            Value::Object(m) => m,
            _ => panic!("not an object"),
        }
    }

    fn ctx_with(target: &str, ids: &[&str]) -> RecordContext {
        let mut u = BTreeMap::new();
        u.insert(
            target.to_string(),
            ids.iter().map(|s| s.to_string()).collect(),
        );
        RecordContext::new(u)
    }

    fn errors(issues: &[RecordIssue]) -> Vec<&RecordIssue> {
        issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .collect()
    }

    #[test]
    fn clean_part_at_bound_validates() {
        let c = contract();
        let parts = c.collection("parts").unwrap();
        let record = obj(
            r#"{ "id": "ABCDEFGHJKMNPQ", "type": "M3 bolt", "description": "hex",
                 "manufacturer": "VENDOR01", "part_number": "PN-1",
                 "torque_spec": "1.50", "calibration_due": "2026-01-01T00:00:00Z" }"#,
        );
        let ctx = ctx_with("companies", &["VENDOR01"]);
        let issues = validate_record(parts, &record, Some("bound"), &ctx);
        assert!(errors(&issues).is_empty(), "unexpected: {issues:?}");
    }

    #[test]
    fn missing_required_to_enter_field_blocks_at_target_status() {
        let c = contract();
        let parts = c.collection("parts").unwrap();
        // `type` is required_to_enter "bound"; omit it at status bound.
        let record = obj(r#"{ "id": "ABCDEFGHJKMNPQ", "manufacturer": "VENDOR01" }"#);
        let ctx = ctx_with("companies", &["VENDOR01"]);
        let issues = validate_record(parts, &record, Some("bound"), &ctx);
        assert!(issues.iter().any(|i| i.path == "type"
            && i.severity == Severity::Error
            && i.message.contains("enter status `bound`")));
    }

    #[test]
    fn missing_required_to_enter_field_ok_before_target_status() {
        let c = contract();
        let parts = c.collection("parts").unwrap();
        let record = obj(r#"{ "id": "ABCDEFGHJKMNPQ" }"#);
        let ctx = RecordContext::default();
        // At "unbound", `type` (required_to_enter bound) need not exist.
        let issues = validate_record(parts, &record, Some("unbound"), &ctx);
        assert!(errors(&issues).is_empty(), "unexpected: {issues:?}");
    }

    #[test]
    fn unknown_reference_rejected_by_policy() {
        let c = contract();
        let parts = c.collection("parts").unwrap();
        // manufacturer on_unknown is "reject" in the example.
        let record = obj(r#"{ "type": "x", "manufacturer": "GHOST" }"#);
        let ctx = ctx_with("companies", &["VENDOR01"]);
        let issues = validate_record(parts, &record, Some("bound"), &ctx);
        assert!(issues.iter().any(|i| i.path == "manufacturer"
            && i.severity == Severity::Error
            && i.message.contains("not found")));
    }

    #[test]
    fn unknown_reference_skipped_when_universe_absent() {
        let c = contract();
        let parts = c.collection("parts").unwrap();
        let record = obj(r#"{ "type": "x", "manufacturer": "GHOST" }"#);
        // Universe does NOT include "companies" → FE-preflight, skip FK.
        let ctx = RecordContext::default();
        let issues = validate_record(parts, &record, Some("bound"), &ctx);
        assert!(!issues.iter().any(|i| i.path == "manufacturer"));
    }

    #[test]
    fn enum_unknown_value_rejected_for_closed_set() {
        let c = contract();
        let companies = c.collection("companies").unwrap();
        // role is closed:true → unknown rejected.
        let record = obj(r#"{ "label": "Acme", "role": "wholesaler" }"#);
        let issues = validate_record(
            companies,
            &record,
            Some("active"),
            &RecordContext::default(),
        );
        assert!(issues
            .iter()
            .any(|i| i.path == "role" && i.severity == Severity::Error));
    }

    #[test]
    fn decimal_scale_violation_is_error() {
        let c = contract();
        let parts = c.collection("parts").unwrap();
        // torque_spec scale=2; "1.234" has scale 3.
        let record = obj(r#"{ "type": "x", "manufacturer": "VENDOR01", "torque_spec": "1.234" }"#);
        let ctx = ctx_with("companies", &["VENDOR01"]);
        let issues = validate_record(parts, &record, Some("bound"), &ctx);
        assert!(issues
            .iter()
            .any(|i| i.path == "torque_spec" && i.message.contains("scale")));
    }

    #[test]
    fn decimal_below_min_is_error() {
        let c = contract();
        let parts = c.collection("parts").unwrap();
        // torque_spec min=0; negative rejected.
        let record = obj(r#"{ "type": "x", "manufacturer": "VENDOR01", "torque_spec": "-1.00" }"#);
        let ctx = ctx_with("companies", &["VENDOR01"]);
        let issues = validate_record(parts, &record, Some("bound"), &ctx);
        assert!(issues
            .iter()
            .any(|i| i.path == "torque_spec" && i.message.contains("below min")));
    }

    #[test]
    fn nested_object_validated_against_schema() {
        let c = contract();
        let parts = c.collection("parts").unwrap();
        // measurement.passed must be bool; give it a string.
        let record = obj(r#"{ "type": "x", "manufacturer": "VENDOR01",
                 "measurement": { "value": "1.0000", "unit": "mm", "passed": "yes" } }"#);
        let ctx = ctx_with("companies", &["VENDOR01"]);
        let issues = validate_record(parts, &record, Some("bound"), &ctx);
        assert!(issues
            .iter()
            .any(|i| i.path == "measurement.passed" && i.severity == Severity::Error));
    }

    #[test]
    fn nested_unknown_key_rejected() {
        let c = contract();
        let parts = c.collection("parts").unwrap();
        let record = obj(r#"{ "type": "x", "manufacturer": "VENDOR01",
                 "measurement": { "value": "1.0000", "rogue": 1 } }"#);
        let ctx = ctx_with("companies", &["VENDOR01"]);
        let issues = validate_record(parts, &record, Some("bound"), &ctx);
        assert!(issues.iter().any(|i| i.path == "measurement.rogue"));
    }

    #[test]
    fn unknown_top_level_key_rejected_without_open_properties() {
        let c = contract();
        // companies has open_properties:true in the example → allowed.
        // contacts does NOT → unknown key rejected.
        let contacts = c.collection("contacts").unwrap();
        let record = obj(r#"{ "name": "Jo", "company": "VENDOR01", "rogue": 1 }"#);
        let ctx = ctx_with("companies", &["VENDOR01"]);
        let issues = validate_record(contacts, &record, None, &ctx);
        assert!(issues
            .iter()
            .any(|i| i.path == "rogue" && i.severity == Severity::Error));
    }

    #[test]
    fn open_properties_collection_allows_extra_keys() {
        let c = contract();
        // parts has open_properties:true.
        let parts = c.collection("parts").unwrap();
        let record =
            obj(r#"{ "type": "x", "manufacturer": "VENDOR01", "scratchpad": "anything" }"#);
        let ctx = ctx_with("companies", &["VENDOR01"]);
        let issues = validate_record(parts, &record, Some("bound"), &ctx);
        assert!(!issues.iter().any(|i| i.path == "scratchpad"));
    }

    #[test]
    fn open_property_with_structured_value_is_rejected() {
        let c = contract();
        let parts = c.collection("parts").unwrap();
        // The tier-3 open bag is flat scalars: an object (or array) open
        // property is rejected — structured data needs a declared field.
        let record = obj(r#"{ "type": "x", "manufacturer": "VENDOR01", "nested": { "a": 1 } }"#);
        let ctx = ctx_with("companies", &["VENDOR01"]);
        let issues = validate_record(parts, &record, Some("bound"), &ctx);
        assert!(
            issues.iter().any(|i| i.path == "nested"),
            "an object-valued open property must be rejected: {issues:?}"
        );
    }

    #[test]
    fn validation_dispatches_on_kind() {
        // A no-open-properties collection with no `resistance` field.
        let contract = Contract::from_bytes(
            br#"{"format_version":1,"collections":[
            {"name":"parts","id":{"scheme":"nano14","default":true,"mintable":true},
             "fields":[{"key":"type","type":"string","label":"Type"}]}]}"#,
        )
        .unwrap();
        let parts = contract.collection("parts").unwrap();
        let resistance: Field =
            serde_json::from_str(r#"{"key":"resistance","type":"string","label":"R"}"#).unwrap();
        let mut ks = BTreeMap::new();
        ks.insert("resistor".to_string(), vec![resistance]);
        let ctx = RecordContext::default().with_kind_schemas(ks);

        let rec = obj(r#"{"id":"PART2223AAAAAA","kind":"resistor","resistance":"10k","type":"r"}"#);
        // With the kind's tier-2 schema, `resistance` is an accepted field.
        let issues = validate_record(parts, &rec, None, &ctx);
        assert!(
            !issues.iter().any(|i| i.path == "resistance"),
            "the kind's field is accepted: {issues:?}"
        );
        // Without the kind tree loaded, `resistance` is an unknown field.
        let issues2 = validate_record(parts, &rec, None, &RecordContext::default());
        assert!(
            issues2.iter().any(|i| i.path == "resistance"),
            "unknown without the kind dispatch: {issues2:?}"
        );
    }

    #[test]
    fn attachment_extension_enforced() {
        let c = contract();
        let companies = c.collection("companies").unwrap();
        // certification constraint "pdf"; give a .docx (ADR-035 §4 object).
        let record = obj(r#"{ "label": "Acme", "role": "manufacturer",
                 "certification": { "ref": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "name": "cert.docx" } }"#);
        let issues = validate_record(
            companies,
            &record,
            Some("active"),
            &RecordContext::default(),
        );
        assert!(issues
            .iter()
            .any(|i| i.path == "certification.name" && i.message.contains("not in allowed set")));
    }

    #[test]
    fn attachment_requires_content_addressed_object() {
        // ADR-035 §4: an attachment value is { ref: sha256:…, name, desc? }.
        let c = contract();
        let companies = c.collection("companies").unwrap();
        // A bare string is no longer a valid attachment.
        let as_string =
            obj(r#"{ "label": "Acme", "role": "manufacturer", "certification": "cert.pdf" }"#);
        let i1 = validate_record(
            companies,
            &as_string,
            Some("active"),
            &RecordContext::default(),
        );
        assert!(i1
            .iter()
            .any(|i| i.path == "certification" && i.message.contains("attachment object")));
        // A non-sha256 ref is rejected.
        let bad_ref = obj(
            r#"{ "label": "Acme", "role": "manufacturer", "certification": { "ref": "nope", "name": "cert.pdf" } }"#,
        );
        let i2 = validate_record(
            companies,
            &bad_ref,
            Some("active"),
            &RecordContext::default(),
        );
        assert!(i2
            .iter()
            .any(|i| i.path == "certification.ref" && i.message.contains("sha256")));
        // A well-formed object with the allowed extension is clean.
        let ok = obj(
            r#"{ "label": "Acme", "role": "manufacturer", "certification": { "ref": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "name": "cert.pdf", "desc": "ISO cert" } }"#,
        );
        let i3 = validate_record(companies, &ok, Some("active"), &RecordContext::default());
        assert!(
            i3.iter()
                .all(|i| i.path != "certification" && !i.path.starts_with("certification.")),
            "got {i3:?}"
        );
    }

    #[test]
    fn missing_required_field_is_error() {
        let c = contract();
        let companies = c.collection("companies").unwrap();
        // label + role are required:true; omit both.
        let record = obj(r#"{ }"#);
        let issues = validate_record(
            companies,
            &record,
            Some("active"),
            &RecordContext::default(),
        );
        assert!(issues
            .iter()
            .any(|i| i.path == "label" && i.message.contains("required")));
        assert!(issues
            .iter()
            .any(|i| i.path == "role" && i.message.contains("required")));
    }

    #[test]
    fn warn_policy_does_not_error() {
        let c = contract();
        // companies.primary_contact on_unknown is "warn".
        let companies = c.collection("companies").unwrap();
        let record =
            obj(r#"{ "label": "Acme", "role": "manufacturer", "primary_contact": "NOBODY" }"#);
        let ctx = ctx_with("contacts", &["KNOWN"]);
        let issues = validate_record(companies, &record, Some("active"), &ctx);
        let pc: Vec<_> = issues
            .iter()
            .filter(|i| i.path == "primary_contact")
            .collect();
        assert_eq!(pc.len(), 1);
        assert_eq!(pc[0].severity, Severity::Warn);
    }

    #[test]
    fn graph_acyclicity_detects_cycles() {
        let c = Contract::from_bytes(
            br#"{"format_version":1,"collections":[
            {"name":"parts","id":{"scheme":"nano14","default":true,"mintable":true},
             "open_properties":true,
             "relations":[{"name":"components","target":"parts","acyclic":true,"kind":"many-many"}]}]}"#,
        )
        .unwrap();
        let coll = c.collection("parts").unwrap();

        // Acyclic chain P1 -> P2 -> P3 → no issue.
        let acyclic = [
            obj(r#"{"id":"P1","components":["P2"]}"#),
            obj(r#"{"id":"P2","components":["P3"]}"#),
            obj(r#"{"id":"P3","components":[]}"#),
        ];
        assert!(validate_collection_graph(coll, &acyclic).is_empty());

        // Cycle P1 -> P2 -> P1 → exactly one error.
        let cyclic = [
            obj(r#"{"id":"P1","components":["P2"]}"#),
            obj(r#"{"id":"P2","components":["P1"]}"#),
        ];
        let issues = validate_collection_graph(coll, &cyclic);
        assert_eq!(issues.len(), 1, "expected one cycle issue, got {issues:?}");
        assert!(issues[0].message.contains("cycle"));
    }

    #[test]
    fn id_format_dispatches_on_declared_scheme() {
        // sha256-scheme collection: the id must be a content-address.
        let blobs_c = Contract::from_bytes(
            br#"{"format_version":1,"collections":[
            {"name":"blobs","id":{"scheme":"sha256","default":true,"mintable":false},
             "open_properties":true}]}"#,
        )
        .unwrap();
        let blobs = blobs_c.collection("blobs").unwrap();
        let good = obj(
            r#"{"id":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#,
        );
        assert!(validate_record(blobs, &good, None, &RecordContext::default()).is_empty());
        let bad = obj(r#"{"id":"not-a-hash"}"#);
        assert!(
            validate_record(blobs, &bad, None, &RecordContext::default())
                .iter()
                .any(|i| i.path == "id" && i.message.contains("sha256"))
        );

        // Imported/asserted scheme (udi): any non-empty value accepted.
        let dev_c = Contract::from_bytes(
            br#"{"format_version":1,"collections":[
            {"name":"devices","id":{"scheme":"udi","default":true,"mintable":false},
             "open_properties":true}]}"#,
        )
        .unwrap();
        let dev = dev_c.collection("devices").unwrap();
        let imported = obj(r#"{"id":"(01)00844588003288"}"#);
        assert!(validate_record(dev, &imported, None, &RecordContext::default()).is_empty());
    }

    #[test]
    fn void_policy_flags_references_to_voided_records() {
        let block = Contract::from_bytes(
            br#"{"format_version":1,"collections":[
            {"name":"parts","id":{"scheme":"nano14","default":true,"mintable":true},
             "open_properties":true,
             "relations":[{"name":"components","target":"parts","kind":"many-many","void_policy":"block"}]}]}"#,
        )
        .unwrap();
        let mut records = BTreeMap::new();
        records.insert(
            "parts".to_string(),
            vec![
                obj(r#"{"id":"P2","status":"void"}"#),
                obj(r#"{"id":"P1","components":["P2"]}"#),
            ],
        );
        let issues = validate_void_policy(&block, &records);
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Error);
        assert!(issues[0].message.contains("voided `P2`"), "{issues:?}");

        // `warn` policy downgrades to a Warn, never blocking.
        let warn = Contract::from_bytes(
            br#"{"format_version":1,"collections":[
            {"name":"parts","id":{"scheme":"nano14","default":true,"mintable":true},
             "open_properties":true,
             "relations":[{"name":"components","target":"parts","kind":"many-many","void_policy":"warn"}]}]}"#,
        )
        .unwrap();
        assert_eq!(
            validate_void_policy(&warn, &records)[0].severity,
            Severity::Warn
        );

        // No voided target → no issue.
        let mut clean = BTreeMap::new();
        clean.insert(
            "parts".to_string(),
            vec![
                obj(r#"{"id":"P2","status":"bound"}"#),
                obj(r#"{"id":"P1","components":["P2"]}"#),
            ],
        );
        assert!(validate_void_policy(&block, &clean).is_empty());
    }
}
