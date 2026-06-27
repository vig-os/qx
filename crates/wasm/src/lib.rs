//! `qx-wasm` — `wasm-bindgen` façade over `codec`,
//! `validators`, and the policy engine per ADR-017 strangler-fig
//! step 8. Consumed by `web/src/` after the inline TS encoder
//! (`web/src/layouts/qrcode-generator.ts` + `svg.ts`) is retired in
//! foundation issue #33.
//!
//! ## Surface
//!
//! The façade exposes a small set of pure functions plus the
//! observability entry points:
//!
//! - [`render_label`] — render an SVG label for a canonical 14-char
//!   ID. Mirrors `label.py` and delegates to
//!   `qx_codec::render`. Layout is one of `"vert"`,
//!   `"horz"`, or `"flag"`; format is one of `"4/4"`, `"4/4/4"`,
//!   `"5/5/4"` (matches the Python CLI flag values verbatim).
//! - decoding is intentionally absent from this façade. The FE
//!   continues to scan with `zxing-wasm` per `web/README.md`; the
//!   A/B parity harness round-trips Rust-encoded SVGs through
//!   `zxing-wasm` directly. Pulling rxing into the FE bundle would
//!   bust the 1.5 MB ceiling and duplicate the existing decoder.
//! - [`validate_diff`] — run every structural validator against a
//!   JSON-encoded `Diff` + accompanying registry + print-log state.
//!   Returns a `{ ok, violations }` JSON object. Advisory per
//!   ADR-016.
//! - [`classify_diff`] — classify a `Diff` into the list of `Action`s
//!   per ADR-016 §"Semantic change classes". Returns the JSON-encoded
//!   `Vec<Action>` array.
//! - [`policy_decision`] — combined classifier + policy engine: takes
//!   a `Diff` + an `Operator` and returns the `AuthDecision`. Mirrors
//!   `validators::policy_decision` exactly.
//! - [`recommend_format`] / [`check_format_warning`] — surface the
//!   Python label.py format-recommendation helpers so the FE can warn
//!   the operator before a print job goes wrong.
//! - [`wasm_request_id_new`] / [`wasm_init`] — observability entry
//!   points (per ADR-022 §"request_id propagation"). `wasm_init` is a
//!   no-op today; `tracing-web` shim deferred (see Observability note
//!   below).
//!
//! Pure functions, no I/O, no globals. Compiles to
//! `wasm32-unknown-unknown` via `wasm-pack build --target web`.
//!
//! ## Observability (ADR-022)
//!
//! `wasm_init()` is the WASM-side entry point that mints a
//! [`RequestId`] for one FE-initiated user action and opens a tracing
//! span so any subsequent emits inside the WASM module inherit it via
//! `tracing` span context. The actual subscriber install on wasm32 is
//! a no-op today: the `tracing-subscriber` `fmt` layer assumes
//! `std::io::stderr`/`std::io::stdout` which behave differently in
//! browsers, and a proper `tracing-web` shim is out of scope for the
//! foundation. The FE conducts its own browser-side logging today and
//! propagates the `request_id` returned here through the ProposalSink
//! payload (ADR-019) per ADR-022 §"request_id propagation".

#![forbid(unsafe_code)]

/// Browser shell over `app::dispatch` (ADR-030 §3) — `registry_open`
/// + `registry_dispatch` + `registry_set_operator`. The pure-function
/// surface below predates it and stays for the legacy `web/` consumer
/// until the feature port retires it.
pub mod dispatch;

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use qx_codec::{
    check_format_warning as codec_check_format_warning, recommend_format as codec_recommend_format,
    render as codec_render, Layout, TextFormat,
};
use qx_domain::{Diff, Operator, Part, PrintEvent, RequestId};
use qx_validators::{
    classify as validators_classify, policy_decision as validators_policy_decision,
    print_log_sort_key, registry_sort_key, validate_print_log_fk, validate_print_log_header,
    validate_print_log_schema, validate_registry_header, validate_registry_schema,
    validate_sort_stable, validate_unique_ids, Policy, ValidationError,
};

// -------------------------------------------------------------------
// Observability entry points (ADR-022)
// -------------------------------------------------------------------

/// Mint a fresh UUIDv7 request id for an FE-initiated action.
///
/// Per ADR-022 §"request_id propagation" the FE generates one ID per
/// user-action root (click, scan, open-proposal) and attaches it to
/// the proposal payload + telemetry. The string is returned in
/// hyphenated lowercase form ready to embed in PR body / fetch
/// headers.
#[wasm_bindgen]
pub fn wasm_request_id_new() -> String {
    RequestId::new().to_string()
}

/// Decode a QR/Micro-QR/DataMatrix from an encoded image (PNG/JPEG bytes)
/// via the Rust `qx_codec` (rxing) — the `decode-image` façade (ADR-032 §1).
/// Gated behind the `decoder` feature so it ships only in the dual-engine
/// A/B test bench, not the lean production FE. Returns the decoded payload,
/// or `null` when nothing is found (a blurred/empty frame).
#[cfg(feature = "decoder")]
#[wasm_bindgen]
pub fn decode_image(bytes: &[u8]) -> Option<String> {
    qx_codec::decode_qr(bytes).ok()
}

/// One-shot init for the WASM façade. Today a no-op: see module docs
/// for why a `tracing-web` shim is deferred. Idempotent; safe to call
/// from any JS-side entry point.
#[wasm_bindgen]
pub fn wasm_init() {
    // intentionally empty — production-side wiring deferred to a
    // future `tracing-web` shim (no foundation issue blocked on it).
}

// -------------------------------------------------------------------
// Layout / format string parsing
// -------------------------------------------------------------------

/// Parse a layout identifier (`"vert"`, `"horz"`, or `"flag"`) plus
/// optional flag-specific parameters into a `codec::Layout`.
///
/// `cable_od_mm` ≤ 0 falls back to the Python default of 6 mm so the
/// FE can call without bothering to set it for non-flag layouts.
fn parse_layout(
    layout: &str,
    cable_od_mm: f64,
    no_markers: bool,
    alignment_line: bool,
) -> Result<Layout, String> {
    match layout {
        "vert" => Ok(Layout::Vert),
        "horz" => Ok(Layout::Horz),
        "flag" => {
            let od = if cable_od_mm > 0.0 { cable_od_mm } else { 6.0 };
            Ok(Layout::Flag {
                cable_od_mm: od,
                no_markers,
                alignment_line,
            })
        }
        other => Err(format!(
            "unknown layout {other:?}: expected one of vert/horz/flag"
        )),
    }
}

/// Parse a text-format identifier (`"4/4"`, `"4/4/4"`, `"5/5/4"`).
/// Mirrors `label.py`'s `--format` flag values verbatim.
fn parse_format(fmt: &str) -> Result<TextFormat, String> {
    match fmt {
        "4/4" => Ok(TextFormat::FourFour),
        "4/4/4" => Ok(TextFormat::FourFourFour),
        "5/5/4" => Ok(TextFormat::FiveFiveFour),
        other => Err(format!(
            "unknown format {other:?}: expected one of 4/4, 4/4/4, 5/5/4"
        )),
    }
}

// -------------------------------------------------------------------
// render_label
// -------------------------------------------------------------------

/// Render an SVG label. Returns the SVG string on success; throws a
/// JS exception on bad arguments (unknown layout/format).
///
/// `micro` selects Micro QR M4 over Standard QR V1.
/// `cable_od_mm` is consumed only by the flag layout; pass `0.0` for
/// vert/horz.
/// `no_markers` suppresses the flag wrap-zone outline + caption.
/// `alignment_line` adds a center registration line to the flag gutter.
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn render_label(
    canonical: &str,
    layout: &str,
    size_mm: f64,
    format: &str,
    micro: bool,
    cable_od_mm: f64,
    no_markers: bool,
    alignment_line: bool,
) -> Result<String, JsError> {
    let layout = parse_layout(layout, cable_od_mm, no_markers, alignment_line)
        .map_err(|e| JsError::new(&e))?;
    let fmt = parse_format(format).map_err(|e| JsError::new(&e))?;
    Ok(codec_render(canonical, layout, size_mm, fmt, micro))
}

// -------------------------------------------------------------------
// recommend_format / check_format_warning
// -------------------------------------------------------------------

/// JS-friendly tuple: `(format_string, warning_or_null)`.
#[derive(Serialize, Deserialize)]
pub struct FormatRecommendation {
    pub format: String,
    pub warning: Option<String>,
}

#[wasm_bindgen]
pub fn recommend_format(size_mm: f64) -> Result<JsValue, JsError> {
    let (fmt, warn) = codec_recommend_format(size_mm);
    let out = FormatRecommendation {
        format: fmt.as_str().to_owned(),
        warning: warn,
    };
    serde_wasm_bindgen::to_value(&out).map_err(|e| JsError::new(&e.to_string()))
}

#[wasm_bindgen]
pub fn check_format_warning(size_mm: f64, format: &str) -> Result<Option<String>, JsError> {
    let fmt = parse_format(format).map_err(|e| JsError::new(&e))?;
    Ok(codec_check_format_warning(size_mm, fmt))
}

// -------------------------------------------------------------------
// validate_diff
// -------------------------------------------------------------------

/// Validation-input bundle. The FE preflight uses this to drive every
/// structural validator in one call: schema, sort, uniqueness, FK.
///
/// All three lists are optional so the FE can pass only what it has;
/// the corresponding checks are skipped if the list is `None`.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct ValidateInput {
    /// Optional registry header row (column names in order). When
    /// `Some`, the canonical-header check runs.
    registry_header: Option<Vec<String>>,
    /// Optional registry rows.
    registry: Option<Vec<Part>>,
    /// Optional print-log header row.
    print_log_header: Option<Vec<String>>,
    /// Optional print-log rows.
    print_log: Option<Vec<PrintEvent>>,
}

/// Single advisory finding. `kind` matches the `ValidationError`
/// discriminator (lower-snake) so the FE can render per-class UI.
#[derive(Clone, Debug, Serialize)]
struct Violation {
    kind: String,
    message: String,
}

impl From<&ValidationError> for Violation {
    fn from(e: &ValidationError) -> Self {
        let kind = match e {
            ValidationError::HeaderMismatch { .. } => "header_mismatch",
            ValidationError::UnsortedAt { .. } => "unsorted_at",
            ValidationError::DuplicateId { .. } => "duplicate_id",
            ValidationError::OrphanPrintEvents { .. } => "orphan_print_events",
            ValidationError::IllegalTransition { .. } => "illegal_transition",
            ValidationError::Policy { .. } => "policy",
        };
        Self {
            kind: kind.to_owned(),
            message: e.to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct ValidateOutput {
    ok: bool,
    violations: Vec<Violation>,
}

/// Run every structural validator against the JSON-encoded input
/// bundle. Returns `{ ok, violations: [{ kind, message }] }`.
///
/// This is the FE-preflight half of the validators-on-the-browser
/// contract (ADR-019). Advisory only; CI re-runs the same logic
/// authoritatively per ADR-016.
#[wasm_bindgen]
pub fn validate_diff(input_json: &str) -> Result<JsValue, JsError> {
    let input: ValidateInput = serde_json::from_str(input_json)
        .map_err(|e| JsError::new(&format!("validate_diff: bad input JSON: {e}")))?;

    let mut violations: Vec<Violation> = Vec::new();

    if let Some(header) = &input.registry_header {
        if let Err(e) = validate_registry_header(header) {
            violations.push((&e).into());
        }
    }
    if let Some(rows) = &input.registry {
        if let Err(e) = validate_registry_schema(rows) {
            violations.push((&e).into());
        }
        if let Err(e) = validate_unique_ids(rows) {
            violations.push((&e).into());
        }
        if let Err(e) = validate_sort_stable(rows, registry_sort_key) {
            violations.push((&e).into());
        }
    }
    if let Some(header) = &input.print_log_header {
        if let Err(e) = validate_print_log_header(header) {
            violations.push((&e).into());
        }
    }
    if let Some(prints) = &input.print_log {
        if let Err(e) = validate_print_log_schema(prints) {
            violations.push((&e).into());
        }
        if let Err(e) = validate_sort_stable(prints, print_log_sort_key) {
            violations.push((&e).into());
        }
        if let Some(registry) = &input.registry {
            if let Err(e) = validate_print_log_fk(prints, registry) {
                violations.push((&e).into());
            }
        }
    }

    let out = ValidateOutput {
        ok: violations.is_empty(),
        violations,
    };
    serde_wasm_bindgen::to_value(&out).map_err(|e| JsError::new(&e.to_string()))
}

// -------------------------------------------------------------------
// validate_record (ADR-039) — the SSOT record validator, wasm arm
// -------------------------------------------------------------------

/// Validate one record against its collection descriptor through the
/// SAME engine the native `qx check` gate runs (ADR-039 §3-4). The wasm
/// arm of the conformance triplet: the FE calls it for form preflight and
/// gets byte-identical verdicts to CI.
///
/// JSON in / JSON out (UTF-8 bytes). `contract` is a canonical
/// `contract.json`; `record` is the entity object; `known_ids` is
/// `{ "<collection>": ["id", ...] }` for reference FK checks (empty = no
/// universe). Returns `[{ "path", "severity": "error"|"warn", "message" },
/// ...]` — an empty array means valid.
#[wasm_bindgen]
pub fn validate_record_json(
    contract: &[u8],
    record: &[u8],
    collection: &str,
    status: Option<String>,
    known_ids: &[u8],
) -> Result<String, JsError> {
    validate_record_json_impl(contract, record, collection, status.as_deref(), known_ids)
        .map_err(|e| JsError::new(&e))
}

/// Pure core of [`validate_record_json`] — native-testable; the wasm
/// wrapper is a thin marshalling shim. `Err` is a wiring failure (bad
/// JSON, unknown collection), distinct from validation issues (which are
/// the JSON payload).
fn validate_record_json_impl(
    contract: &[u8],
    record: &[u8],
    collection: &str,
    status: Option<&str>,
    known_ids: &[u8],
) -> Result<String, String> {
    use qx_contract::Contract;
    use qx_validators::record::{validate_record, RecordContext, Severity};
    use std::collections::{BTreeMap, BTreeSet};

    let contract = Contract::from_bytes(contract).map_err(|e| format!("contract: {e}"))?;
    let coll = contract
        .collection(collection)
        .ok_or_else(|| format!("unknown collection `{collection}`"))?;
    let record: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(record).map_err(|e| format!("record: {e}"))?;

    let universe: BTreeMap<String, BTreeSet<String>> = if known_ids.is_empty() {
        BTreeMap::new()
    } else {
        let raw: BTreeMap<String, Vec<String>> =
            serde_json::from_slice(known_ids).map_err(|e| format!("known_ids: {e}"))?;
        raw.into_iter()
            .map(|(k, v)| (k, v.into_iter().collect()))
            .collect()
    };
    let ctx = RecordContext::new(universe);

    let issues: Vec<serde_json::Value> = validate_record(coll, &record, status, &ctx)
        .iter()
        .map(|i| {
            serde_json::json!({
                "path": i.path,
                "severity": match i.severity { Severity::Error => "error", Severity::Warn => "warn" },
                "message": i.message,
            })
        })
        .collect();
    serde_json::to_string(&issues).map_err(|e| e.to_string())
}

// -------------------------------------------------------------------
// classify_diff
// -------------------------------------------------------------------

/// Classify a JSON-encoded `Diff` into a list of `Action`s per
/// ADR-016 §"Semantic change classes". Pure function; identical to
/// the CI classifier.
#[wasm_bindgen]
pub fn classify_diff(diff_json: &str) -> Result<JsValue, JsError> {
    let diff: Diff = serde_json::from_str(diff_json)
        .map_err(|e| JsError::new(&format!("classify_diff: bad Diff JSON: {e}")))?;
    let actions = validators_classify(&diff);
    serde_wasm_bindgen::to_value(&actions).map_err(|e| JsError::new(&e.to_string()))
}

// -------------------------------------------------------------------
// policy_decision (combined classifier + policy engine)
// -------------------------------------------------------------------

/// DTO for `validators::Policy`. We define a separate JSON-facing
/// shape rather than depending on a `Deserialize` impl in the
/// validators crate — that crate is intentionally serde-shy on its
/// policy struct because the canonical inputs come from the CI
/// `policy.toml`, not arbitrary JSON. Field defaults mirror
/// `Policy::default()` exactly.
#[derive(Deserialize)]
#[serde(default)]
struct PolicyDto {
    allow_header_changes: bool,
    destructive_requires_elevation: bool,
    bulk_threshold: u32,
    elevation_role_claim: String,
}

impl Default for PolicyDto {
    fn default() -> Self {
        let p = Policy::default();
        Self {
            allow_header_changes: p.allow_header_changes,
            destructive_requires_elevation: p.destructive_requires_elevation,
            bulk_threshold: p.bulk_threshold,
            elevation_role_claim: p.elevation_role_claim,
        }
    }
}

impl From<PolicyDto> for Policy {
    fn from(d: PolicyDto) -> Self {
        Self {
            allow_header_changes: d.allow_header_changes,
            destructive_requires_elevation: d.destructive_requires_elevation,
            bulk_threshold: d.bulk_threshold,
            elevation_role_claim: d.elevation_role_claim,
        }
    }
}

/// Inputs to [`policy_decision`]. The FE passes a `Diff` + `Operator`;
/// the policy is read from the optional `policy` field or falls back
/// to the canonical defaults (header changes blocked, destructive
/// requires elevation, bulk threshold 100, `qms-approver` claim).
#[derive(Deserialize)]
struct PolicyInput {
    diff: Diff,
    operator: Operator,
    #[serde(default)]
    policy: Option<PolicyDto>,
}

#[wasm_bindgen]
pub fn policy_decision(input_json: &str) -> Result<JsValue, JsError> {
    let input: PolicyInput = serde_json::from_str(input_json)
        .map_err(|e| JsError::new(&format!("policy_decision: bad input JSON: {e}")))?;
    let policy: Policy = input.policy.unwrap_or_default().into();
    let decision = validators_policy_decision(&input.diff, &input.operator, &policy);
    serde_wasm_bindgen::to_value(&decision).map_err(|e| JsError::new(&e.to_string()))
}

// -------------------------------------------------------------------
// Tests
// -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    //! Native-side smoke tests. The wasm32 build is gated by `cargo
    //! build --target wasm32-unknown-unknown -p qx-wasm`
    //! in CI; these tests only exercise the parser + JSON shape on
    //! native so `cargo test --workspace` covers the surface without
    //! a wasm runtime.

    use super::validate_record_json_impl;

    const RJ_CONTRACT: &[u8] = br#"{"format_version":1,"collections":[
        {"name":"parts","id":{"scheme":"nano14","default":true,"mintable":true},
         "fields":[{"key":"grade","type":"enum","label":"Grade","values":["a","b"],"closed":true},
                   {"key":"maker","type":"reference","label":"Maker","collection":"companies","on_unknown":"reject"}]},
        {"name":"companies","id":{"scheme":"nano14","default":false,"mintable":true},
         "fields":[{"key":"label","type":"string","label":"Name"}]}]}"#;

    #[test]
    fn validate_record_json_valid_is_empty() {
        let out = validate_record_json_impl(
            RJ_CONTRACT,
            br#"{"id":"PART2223AAAAAA","grade":"a"}"#,
            "parts",
            None,
            b"",
        )
        .unwrap();
        assert_eq!(out, "[]");
    }

    #[test]
    fn validate_record_json_flags_invalid_enum() {
        let out = validate_record_json_impl(
            RJ_CONTRACT,
            br#"{"id":"PART2224AAAAAA","grade":"z"}"#,
            "parts",
            None,
            b"",
        )
        .unwrap();
        assert!(out.contains(r#""severity":"error""#), "got {out}");
        assert!(out.contains(r#""path":"grade""#), "got {out}");
    }

    #[test]
    fn validate_record_json_marshals_known_ids() {
        // `maker` references companies with on_unknown:reject; with the
        // referenced id in the universe the record is clean.
        let out = validate_record_json_impl(
            RJ_CONTRACT,
            br#"{"id":"PART2225AAAAAA","grade":"a","maker":"CMPY2223AAAAAA"}"#,
            "parts",
            None,
            br#"{"companies":["CMPY2223AAAAAA"]}"#,
        )
        .unwrap();
        assert_eq!(out, "[]", "valid FK should produce no issues; got {out}");
    }

    #[test]
    fn validate_record_json_unknown_collection_is_wiring_error() {
        let err = validate_record_json_impl(RJ_CONTRACT, b"{}", "ghosts", None, b"").unwrap_err();
        assert!(err.contains("unknown collection"), "got {err}");
    }

    use super::*;

    const FIXED_ID: &str = "K7M3PQ9RT5VAXY";

    #[test]
    fn parse_layout_known_variants() {
        assert!(parse_layout("vert", 0.0, false, false).is_ok());
        assert!(parse_layout("horz", 0.0, false, false).is_ok());
        match parse_layout("flag", 8.0, false, false).unwrap() {
            Layout::Flag {
                cable_od_mm,
                no_markers,
                alignment_line,
            } => {
                assert!((cable_od_mm - 8.0).abs() < 1e-9);
                assert!(!no_markers);
                assert!(!alignment_line);
            }
            other => panic!("expected Flag, got {other:?}"),
        }
        // cable_od ≤ 0 → 6 mm fallback.
        match parse_layout("flag", 0.0, false, false).unwrap() {
            Layout::Flag { cable_od_mm, .. } => assert!((cable_od_mm - 6.0).abs() < 1e-9),
            other => panic!("expected Flag, got {other:?}"),
        }
    }

    #[test]
    fn parse_layout_flag_options() {
        match parse_layout("flag", 6.0, true, true).unwrap() {
            Layout::Flag {
                no_markers,
                alignment_line,
                ..
            } => {
                assert!(no_markers);
                assert!(alignment_line);
            }
            other => panic!("expected Flag, got {other:?}"),
        }
        // Options are ignored for non-flag layouts.
        assert_eq!(parse_layout("vert", 0.0, true, true).unwrap(), Layout::Vert);
    }

    #[test]
    fn parse_layout_rejects_unknown() {
        assert!(parse_layout("circle", 0.0, false, false).is_err());
    }

    #[test]
    fn parse_format_known_variants() {
        assert_eq!(parse_format("4/4").unwrap(), TextFormat::FourFour);
        assert_eq!(parse_format("4/4/4").unwrap(), TextFormat::FourFourFour);
        assert_eq!(parse_format("5/5/4").unwrap(), TextFormat::FiveFiveFour);
        assert!(parse_format("3/3").is_err());
    }

    #[test]
    fn render_label_emits_well_formed_svg_for_every_layout_format() {
        // Pure native call — bypasses #[wasm_bindgen] glue.
        for &layout in &["vert", "horz", "flag"] {
            for &fmt in &["4/4", "4/4/4", "5/5/4"] {
                let layout_enum = parse_layout(layout, 6.0, false, false).unwrap();
                let fmt_enum = parse_format(fmt).unwrap();
                let svg = codec_render(FIXED_ID, layout_enum, 11.0, fmt_enum, false);
                assert!(
                    svg.starts_with("<svg"),
                    "{layout}/{fmt} not an SVG: {svg:.80}..."
                );
                assert!(svg.contains("</svg>"));
            }
        }
    }

    /// Inline equivalent of the `validate_diff` body so the native
    /// tests can exercise the same flow without going through
    /// wasm-bindgen's JsValue layer (which requires a JS host).
    fn validate_native(input: &ValidateInput) -> ValidateOutput {
        let mut violations: Vec<Violation> = Vec::new();
        if let Some(h) = &input.registry_header {
            if let Err(e) = validate_registry_header(h) {
                violations.push((&e).into());
            }
        }
        if let Some(rows) = &input.registry {
            if let Err(e) = validate_registry_schema(rows) {
                violations.push((&e).into());
            }
            if let Err(e) = validate_unique_ids(rows) {
                violations.push((&e).into());
            }
            if let Err(e) = validate_sort_stable(rows, registry_sort_key) {
                violations.push((&e).into());
            }
        }
        if let Some(h) = &input.print_log_header {
            if let Err(e) = validate_print_log_header(h) {
                violations.push((&e).into());
            }
        }
        if let Some(prints) = &input.print_log {
            if let Err(e) = validate_print_log_schema(prints) {
                violations.push((&e).into());
            }
            if let Err(e) = validate_sort_stable(prints, print_log_sort_key) {
                violations.push((&e).into());
            }
            if let Some(registry) = &input.registry {
                if let Err(e) = validate_print_log_fk(prints, registry) {
                    violations.push((&e).into());
                }
            }
        }
        ValidateOutput {
            ok: violations.is_empty(),
            violations,
        }
    }

    #[test]
    fn validate_diff_passes_on_canonical_headers() {
        let input: ValidateInput = serde_json::from_value(serde_json::json!({
            "registry_header": [
                "id", "status", "minted_at", "bound_at", "type",
                "description", "vendor", "part_number", "location", "notes",
                "minted_by", "bound_by", "last_edited_at", "last_edited_by",
                "components", "manufacturer_id", "metadata"
            ],
            "print_log_header": [
                "id", "printed_at", "printed_by", "layout", "size_mm",
                "extra", "copies", "output_mode", "batch_label"
            ],
        }))
        .unwrap();
        let result = validate_native(&input);
        assert!(result.ok, "{:?}", result.violations);
    }

    #[test]
    fn validate_diff_flags_header_mismatch() {
        let input: ValidateInput =
            serde_json::from_str(r#"{"registry_header": ["id", "wrong"]}"#).unwrap();
        let result = validate_native(&input);
        assert_eq!(result.violations.len(), 1);
        assert_eq!(result.violations[0].kind, "header_mismatch");
        assert!(!result.ok);
    }

    #[test]
    fn classify_diff_routes_header_change() {
        let diff_json = serde_json::json!({
            "adds": [],
            "deletes": [],
            "edits": [],
            "header_changes": [{
                "file": "registry.csv",
                "before": ["id"],
                "after": ["id", "status"],
            }]
        })
        .to_string();
        let diff: Diff = serde_json::from_str(&diff_json).unwrap();
        let actions = validators_classify(&diff);
        assert_eq!(actions.len(), 1);
    }
}
