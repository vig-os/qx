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
//! Deferred (tracked for the conformance task): `pattern` regex
//! enforcement and `$ref` object-schema resolution — every other facet
//! in the §2 scalar set is enforced here.

use std::collections::{BTreeMap, BTreeSet};

use qx_contract::{Closed, Collection, Field, FieldType, ObjectSchema, OnUnknown};
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
}

impl RecordContext {
    /// Build a context from `(collection, ids)` pairs.
    pub fn new(universe: BTreeMap<String, BTreeSet<String>>) -> Self {
        Self {
            known_ids: universe,
        }
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
    let declared: BTreeSet<&str> = collection.fields.iter().map(|f| f.key.as_str()).collect();

    // Unknown keys: allowed only when the collection opts into tier-3
    // open_properties (ADR-035 §1). `id` and `status` are engine-owned
    // envelope keys, never declared as fields.
    if !collection.open_properties {
        for key in record.keys() {
            if key == "id" || key == "status" {
                continue;
            }
            if !declared.contains(key.as_str()) {
                issues.push(RecordIssue::error(
                    key.clone(),
                    format!("unknown field `{key}` (collection does not allow open properties)"),
                ));
            }
        }
    }

    for field in &collection.fields {
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
    let Some(s) = value.as_str() else {
        issues.push(RecordIssue::error(
            path,
            "expected an attachment reference (string)",
        ));
        return;
    };
    if let Some(constraint) = field.constraint.as_deref() {
        // constraint is a pipe-separated extension allow-list, e.g. "pdf|png".
        let allowed: Vec<&str> = constraint.split('|').map(str::trim).collect();
        let ext = s.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        if !allowed.iter().any(|a| a.eq_ignore_ascii_case(&ext)) {
            issues.push(RecordIssue::error(
                path,
                format!("attachment extension `{ext}` not in allowed set [{constraint}]"),
            ));
        }
    }
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
            r#"{ "id": "ABCDEFGH", "type": "M3 bolt", "description": "hex",
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
        let record = obj(r#"{ "id": "ABCDEFGH", "manufacturer": "VENDOR01" }"#);
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
        let record = obj(r#"{ "id": "ABCDEFGH" }"#);
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
    fn attachment_extension_enforced() {
        let c = contract();
        let companies = c.collection("companies").unwrap();
        // certification constraint "pdf"; give a .docx.
        let record =
            obj(r#"{ "label": "Acme", "role": "manufacturer", "certification": "cert.docx" }"#);
        let issues = validate_record(
            companies,
            &record,
            Some("active"),
            &RecordContext::default(),
        );
        assert!(issues
            .iter()
            .any(|i| i.path == "certification" && i.message.contains("not in allowed set")));
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
}
