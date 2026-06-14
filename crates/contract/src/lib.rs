//! `part-registry-contract` — the pure parser + types for the canonical
//! `contract.json` form (ADR-039).
//!
//! ## What this crate is
//!
//! The **single source of truth** for "what is a well-formed contract".
//! The Rust types here ARE the schema; `schema/contract.schema.json`
//! mirrors them for editor tooling and the CI meta-schema gate. Two
//! responsibilities, nothing else:
//!
//! 1. [`Contract::from_bytes`] — parse raw bytes into a typed [`Contract`]
//!    and run the **structural** checks a JSON Schema cannot express
//!    (internal foreign keys, lifecycle integrity, facet presence,
//!    uniqueness). On success the value is guaranteed internally
//!    consistent; downstream crates never re-check shape.
//! 2. [`is_compatible`] — does THIS tool understand the contract's
//!    `format_version`? The supported range ([`TOOL_SUPPORTED_FORMAT`])
//!    is a const the tool holds; the contract carries only its own
//!    `format_version` (ADR-039 §6 — the only in-file version; identity
//!    is the content hash; governance is host-projected).
//!
//! ## Cross-surface parity (ADR-039 §4)
//!
//! No I/O, no `std::fs`, no clock. `serde` + `serde_json` + `thiserror`
//! only, so the crate compiles bit-identically to native (the `pr check`
//! gate, the authority) and `wasm32-unknown-unknown` (FE form-gen +
//! preflight, advisory). Parity is by construction: there is nothing to
//! drift.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};
use thiserror::Error;

// -------------------------------------------------------------------
// Format-version compatibility (ADR-039 §6)
// -------------------------------------------------------------------

/// The `format_version` range THIS build of the engine can parse. The
/// contract↔engine parse-capability axis: a format outside this range
/// is a generation the tool does not understand and must refuse rather
/// than mis-parse. Bumped only when the canonical form changes shape —
/// a format generation, not a content edit (those are the content hash).
pub const TOOL_SUPPORTED_FORMAT: RangeInclusive<u32> = 1..=1;

/// Does this tool understand the contract's format generation? Separate
/// from parsing on purpose: a caller may parse a contract to *inspect*
/// it (e.g. report "needs a newer tool") even when it cannot safely act
/// on it. The `pr check` gate calls this before trusting a contract.
pub fn is_compatible(contract: &Contract) -> bool {
    TOOL_SUPPORTED_FORMAT.contains(&contract.format_version)
}

// -------------------------------------------------------------------
// Canonical contract types — mirror schema/contract.schema.json $defs.
// `deny_unknown_fields` everywhere the meta-schema says
// `additionalProperties: false`, so a typo'd facet is a parse error,
// not silently-ignored chrome.
// -------------------------------------------------------------------

/// A registry's contract: a format generation + the collection roster.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contract {
    /// Engine↔contract parse capability — the ONLY in-file version.
    pub format_version: u32,
    /// The collection roster (≥1). The `parts` preset is the
    /// code-owned floor (ADR-035 guardrail #1) — extendable, never
    /// weakenable; that guard lives in the preset layer, not here.
    pub collections: Vec<Collection>,
}

/// One collection descriptor — a generic entity kind over the shared
/// fabric (ADR-035 §0). `parts`, `companies`, `documents` are all just
/// this with different fields.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Collection {
    pub name: String,
    pub id: IdScheme,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<Lifecycle>,
    #[serde(default)]
    pub fields: Vec<Field>,
    /// Typed relations + graph rules (ADR-035 §1a). Opaque to the
    /// parser until the vocab collections land — kept as raw JSON so a
    /// forward-declared relations block round-trips losslessly.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<serde_json::Value>,
    /// Tier-3 escape bag on/off (ADR-035 §1). Shape-checked only;
    /// regulated core fields are forbidden here (the §5 demotion guard
    /// lives in the validators crate, which has the record in hand).
    #[serde(default)]
    pub open_properties: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub render: Option<RenderCollection>,
}

/// Identity scheme for a collection (ADR-012 / ADR-035 typed ids).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdScheme {
    /// e.g. `nano14` (ADR-012), `sha256`, `udi`, `gs1`.
    pub scheme: String,
    /// One default scheme per registry; its bare value is a valid short
    /// form (no colon). Enforced across the roster in [`validate`].
    pub default: bool,
    /// Minted here (`nano`) vs asserted/imported (`udi:`, `gs1:`).
    pub mintable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix_length: Option<u32>,
}

/// Status machine for a collection. `initial` and every transition
/// endpoint must be a declared status (checked in [`validate`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lifecycle {
    pub statuses: Vec<String>,
    /// from-status → allowed to-statuses.
    pub transitions: BTreeMap<String, Vec<String>>,
    pub initial: String,
}

/// The ADR-039 §2 scalar set. The TYPE (data shape) — the widget is
/// declared separately in [`RenderField`] (type ≠ widget). There is no
/// `json` type: genuinely open data lives in `open_properties`, and an
/// `object` must carry a `schema` (no freeform-json backdoor).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    String,
    Enum,
    Integer,
    Number,
    Decimal,
    Date,
    Timestamp,
    Bool,
    Reference,
    Attachment,
    Object,
}

/// Policy for a value not in the source set (enum / reference).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OnUnknown {
    Create,
    Warn,
    Reject,
}

/// enum facet `closed`: reject / warn / accept an unknown value. JSON
/// shape is `true | "warn" | false` (untagged).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Closed {
    /// `true` → reject unknowns; `false` → accept unknowns.
    Flag(bool),
    /// `"warn"` → accept but surface a warning.
    Mode(String),
}

impl Closed {
    /// Does this facet reject values outside [`Field::values`]?
    pub fn rejects_unknown(&self) -> bool {
        matches!(self, Closed::Flag(true))
    }
}

/// object facet `schema`: a nested field list, or a `$ref` to a shared
/// shape. An object WITHOUT a schema would be a freeform-json backdoor
/// (ADR-039 §2), so the facet is required and checked in [`validate`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ObjectSchema {
    /// Inline nested fields (recursive — an object field may nest).
    Fields(Vec<Field>),
    /// `{ "$ref": "..." }` to a shared shape.
    Ref(SchemaRef),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaRef {
    #[serde(rename = "$ref")]
    pub ref_: String,
}

/// One field descriptor. Flat (mirrors the meta-schema's flat
/// `properties` + `allOf` if/then) so the facet keys sit beside `type`
/// exactly as they do in JSON. Which facets are REQUIRED for a given
/// `type` is enforced in [`validate`], not by the type system, so the
/// parser and the meta-schema agree by construction.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub key: String,
    #[serde(rename = "type")]
    pub type_: FieldType,
    pub label: String,
    #[serde(default = "default_true")]
    pub editable: bool,

    // --- lifecycle-coupled flags ---
    /// Required to EXIST (independent of lifecycle).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    /// Hard transition gate (ADR-039 §6): the entity cannot advance to
    /// <status> unless this field is present + valid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_to_enter: Option<String>,
    /// Downstream readers may TRUST this field once <status> is reached.
    /// Documentation, not enforcement — use `required_to_enter` to gate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meaningful_from: Option<String>,

    /// Policy for a value outside the source set (enum / reference).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_unknown: Option<OnUnknown>,

    /// Chrome — the widget a shell draws. Never a verdict.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub render: Option<RenderField>,

    // --- string facets ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(default, rename = "maxLength", skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u64>,

    // --- enum facet ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed: Option<Closed>,

    // --- numeric facets (integer / number / decimal) ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,

    // --- decimal facets ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub precision: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<u32>,

    // --- reference facets ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,

    // --- attachment facet ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraint: Option<String>,

    // --- object facet ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<ObjectSchema>,
}

/// The widget a shell draws for a field. Chrome only — never a verdict
/// (ADR-039 §1: type drives validation, render drives the control).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderField {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widget: Option<Widget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<i64>,
    /// combobox: a collection to suggest values from (free-text field).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggest_from: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Widget {
    Text,
    Textarea,
    Dropdown,
    Combobox,
    Toggle,
    Picker,
    Date,
    Number,
    File,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderCollection {
    /// Which fields compose an entity's short label rendering.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub label_fields: Vec<String>,
}

fn default_true() -> bool {
    true
}

// -------------------------------------------------------------------
// Errors
// -------------------------------------------------------------------

/// Why a byte slice is not a usable contract.
#[derive(Debug, Error)]
pub enum ContractError {
    /// The bytes are not the canonical shape (serde rejected them —
    /// unknown field, wrong type, missing required key, bad enum).
    #[error("contract parse error: {0}")]
    Parse(#[from] serde_json::Error),

    /// The bytes parsed, but the contract is internally inconsistent.
    /// Carries EVERY structural problem found in one pass so an author
    /// fixes them all at once rather than one-per-round.
    #[error("contract is structurally invalid:\n{}", .0.join("\n"))]
    Invalid(Vec<String>),
}

// -------------------------------------------------------------------
// Parse + structural validation
// -------------------------------------------------------------------

impl Contract {
    /// Parse + structurally validate raw bytes. On `Ok`, the contract is
    /// internally consistent: every reference targets a declared
    /// collection, every lifecycle endpoint is a declared status, every
    /// type's required facets are present, and names/keys are unique.
    ///
    /// Does NOT check [`is_compatible`] — parsing an incompatible
    /// format to *inspect* it is allowed; acting on it is the caller's
    /// gate.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ContractError> {
        let contract: Contract = serde_json::from_slice(bytes)?;
        contract.validate()?;
        Ok(contract)
    }

    /// Run the structural checks a JSON Schema cannot express. Collects
    /// all failures. Pure; called by [`from_bytes`] but exposed so a
    /// caller holding an already-built `Contract` can re-assert.
    pub fn validate(&self) -> Result<(), ContractError> {
        let mut errs: Vec<String> = Vec::new();

        if self.collections.is_empty() {
            errs.push("collections: must declare at least one collection".into());
        }

        // The set of declared collection names — the FK target universe.
        let declared: BTreeSet<&str> = self.collections.iter().map(|c| c.name.as_str()).collect();

        // Exactly-one default id scheme across the roster.
        let default_count = self.collections.iter().filter(|c| c.id.default).count();
        if default_count != 1 {
            errs.push(format!(
                "id.default: exactly one collection must be the default id scheme, found {default_count}"
            ));
        }

        // Unique collection names.
        let mut seen_names: BTreeSet<&str> = BTreeSet::new();
        for c in &self.collections {
            if !seen_names.insert(c.name.as_str()) {
                errs.push(format!(
                    "collections: duplicate collection name `{}`",
                    c.name
                ));
            }
        }

        for c in &self.collections {
            c.validate_into(&declared, &mut errs);
        }

        if errs.is_empty() {
            Ok(())
        } else {
            Err(ContractError::Invalid(errs))
        }
    }

    /// Look up a collection by name.
    pub fn collection(&self, name: &str) -> Option<&Collection> {
        self.collections.iter().find(|c| c.name == name)
    }

    /// The default id scheme's collection (the one whose bare ids need
    /// no `scheme:` prefix). `None` only on an invalid contract.
    pub fn default_collection(&self) -> Option<&Collection> {
        self.collections.iter().find(|c| c.id.default)
    }
}

impl Collection {
    fn validate_into(&self, declared: &BTreeSet<&str>, errs: &mut Vec<String>) {
        let where_ = &self.name;

        if self.name.is_empty() {
            errs.push("collections[]: empty collection name".into());
        }

        // Lifecycle integrity: initial + every transition endpoint is a
        // declared status.
        if let Some(lc) = &self.lifecycle {
            let statuses: BTreeSet<&str> = lc.statuses.iter().map(String::as_str).collect();
            if statuses.is_empty() {
                errs.push(format!("{where_}.lifecycle.statuses: must be non-empty"));
            }
            if !statuses.contains(lc.initial.as_str()) {
                errs.push(format!(
                    "{where_}.lifecycle.initial `{}` is not a declared status",
                    lc.initial
                ));
            }
            for (from, tos) in &lc.transitions {
                if !statuses.contains(from.as_str()) {
                    errs.push(format!(
                        "{where_}.lifecycle.transitions: from-status `{from}` is not declared"
                    ));
                }
                for to in tos {
                    if !statuses.contains(to.as_str()) {
                        errs.push(format!(
                            "{where_}.lifecycle.transitions: `{from}` -> `{to}` targets undeclared status"
                        ));
                    }
                }
            }
        }

        // Field-key uniqueness within the collection.
        let mut seen_keys: BTreeSet<&str> = BTreeSet::new();
        for f in &self.fields {
            if !seen_keys.insert(f.key.as_str()) {
                errs.push(format!("{where_}.fields: duplicate field key `{}`", f.key));
            }
        }

        // The status universe a field's lifecycle-coupled flags may name.
        let statuses: BTreeSet<&str> = self
            .lifecycle
            .as_ref()
            .map(|lc| lc.statuses.iter().map(String::as_str).collect())
            .unwrap_or_default();

        for f in &self.fields {
            f.validate_into(where_, declared, &statuses, self.lifecycle.is_some(), errs);
        }
    }
}

impl Field {
    fn validate_into(
        &self,
        coll: &str,
        declared: &BTreeSet<&str>,
        statuses: &BTreeSet<&str>,
        has_lifecycle: bool,
        errs: &mut Vec<String>,
    ) {
        let at = format!("{coll}.{}", self.key);

        if self.key.is_empty() {
            errs.push(format!("{coll}.fields[]: empty field key"));
        }
        if self.label.is_empty() {
            errs.push(format!(
                "{at}.label: must be non-empty (descriptor owns display)"
            ));
        }

        // Facet-presence rules — the meta-schema's allOf if/then,
        // re-enforced so from_bytes is the SSOT even if ajv never ran.
        match self.type_ {
            FieldType::Reference => match &self.collection {
                None => errs.push(format!("{at}: reference field requires `collection`")),
                Some(target) if !declared.contains(target.as_str()) => errs.push(format!(
                    "{at}: reference targets undeclared collection `{target}`"
                )),
                Some(_) => {}
            },
            FieldType::Object => match &self.schema {
                None => errs.push(format!(
                    "{at}: object field requires `schema` (no freeform-json backdoor)"
                )),
                Some(ObjectSchema::Fields(nested)) => {
                    let mut nested_keys: BTreeSet<&str> = BTreeSet::new();
                    for nf in nested {
                        if !nested_keys.insert(nf.key.as_str()) {
                            errs.push(format!("{at}.schema: duplicate nested key `{}`", nf.key));
                        }
                        // Nested fields validate too, but cannot carry
                        // lifecycle flags (no inner lifecycle).
                        nf.validate_into(&at, declared, &BTreeSet::new(), false, errs);
                    }
                }
                Some(ObjectSchema::Ref(_)) => {}
            },
            FieldType::Enum => {
                if self.values.as_ref().map(|v| v.is_empty()).unwrap_or(true) {
                    errs.push(format!(
                        "{at}: enum field requires a non-empty `values` set"
                    ));
                }
            }
            FieldType::Decimal => match (self.precision, self.scale) {
                (Some(p), Some(s)) if s > p => errs.push(format!(
                    "{at}: decimal scale ({s}) cannot exceed precision ({p})"
                )),
                (Some(_), Some(_)) => {}
                _ => errs.push(format!(
                    "{at}: decimal field requires `precision` and `scale`"
                )),
            },
            _ => {}
        }

        // Lifecycle-coupled flags must name a real status of THIS
        // collection (and the collection must have a lifecycle at all).
        for (flag, val) in [
            ("required_to_enter", &self.required_to_enter),
            ("meaningful_from", &self.meaningful_from),
        ] {
            if let Some(status) = val {
                if !has_lifecycle {
                    errs.push(format!(
                        "{at}.{flag} names `{status}` but `{coll}` has no lifecycle"
                    ));
                } else if !statuses.contains(status.as_str()) {
                    errs.push(format!("{at}.{flag} `{status}` is not a declared status"));
                }
            }
        }
    }
}

// -------------------------------------------------------------------
// Tests
// -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// The real shipped example — parsing it here is the parity gate
    /// between the Rust SSOT and `schema/contract.example.json`.
    const EXAMPLE: &str = include_str!("../../../schema/contract.example.json");

    fn parse(json: &str) -> Result<Contract, ContractError> {
        Contract::from_bytes(json.as_bytes())
    }

    #[test]
    fn shipped_example_parses_and_validates() {
        let c = Contract::from_bytes(EXAMPLE.as_bytes()).expect("shipped example must be valid");
        assert_eq!(c.format_version, 1);
        assert!(c.collection("parts").is_some());
        assert!(c.collection("companies").is_some());
        assert!(c.collection("contacts").is_some());
        // parts.manufacturer references companies.
        let parts = c.collection("parts").unwrap();
        let manu = parts
            .fields
            .iter()
            .find(|f| f.key == "manufacturer")
            .unwrap();
        assert_eq!(manu.type_, FieldType::Reference);
        assert_eq!(manu.collection.as_deref(), Some("companies"));
    }

    #[test]
    fn shipped_example_is_compatible() {
        let c = Contract::from_bytes(EXAMPLE.as_bytes()).unwrap();
        assert!(is_compatible(&c));
    }

    #[test]
    fn default_collection_is_parts() {
        let c = Contract::from_bytes(EXAMPLE.as_bytes()).unwrap();
        assert_eq!(
            c.default_collection().map(|c| c.name.as_str()),
            Some("parts")
        );
    }

    #[test]
    fn round_trips_through_serde() {
        let c = Contract::from_bytes(EXAMPLE.as_bytes()).unwrap();
        let bytes = serde_json::to_vec(&c).unwrap();
        let c2 = Contract::from_bytes(&bytes).unwrap();
        assert_eq!(c, c2);
    }

    // --- format compatibility ---

    #[test]
    fn future_format_version_parses_but_is_incompatible() {
        let json = r#"{ "format_version": 99,
            "collections": [ { "name": "parts",
              "id": { "scheme": "nano14", "default": true, "mintable": true },
              "fields": [] } ] }"#;
        let c = parse(json).expect("a future format still parses for inspection");
        assert!(
            !is_compatible(&c),
            "format 99 is beyond TOOL_SUPPORTED_FORMAT"
        );
    }

    #[test]
    fn missing_format_version_is_a_parse_error() {
        let json = r#"{ "collections": [] }"#;
        assert!(matches!(parse(json), Err(ContractError::Parse(_))));
    }

    // --- deny_unknown_fields ---

    #[test]
    fn unknown_field_key_is_rejected() {
        // `widge` typo for `widget` — additionalProperties:false bites.
        let json = r#"{ "format_version": 1, "collections": [ { "name": "parts",
            "id": { "scheme": "nano14", "default": true, "mintable": true },
            "fields": [ { "key": "x", "type": "string", "label": "X",
              "render": { "widge": "text" } } ] } ] }"#;
        assert!(matches!(parse(json), Err(ContractError::Parse(_))));
    }

    #[test]
    fn stray_change_control_block_is_rejected() {
        // §6: governance is host-projected, never stored in-file.
        let json = r#"{ "format_version": 1, "change_control": { "approved_by": "x" },
            "collections": [ { "name": "parts",
              "id": { "scheme": "nano14", "default": true, "mintable": true },
              "fields": [] } ] }"#;
        assert!(matches!(parse(json), Err(ContractError::Parse(_))));
    }

    // --- structural: facet presence ---

    fn one_collection(field: &str) -> String {
        format!(
            r#"{{ "format_version": 1, "collections": [ {{ "name": "parts",
              "id": {{ "scheme": "nano14", "default": true, "mintable": true }},
              "fields": [ {field} ] }} ] }}"#
        )
    }

    #[test]
    fn reference_without_collection_is_invalid() {
        let json = one_collection(r#"{ "key": "v", "type": "reference", "label": "V" }"#);
        let err = parse(&json).unwrap_err();
        assert!(
            matches!(err, ContractError::Invalid(ref v) if v.iter().any(|m| m.contains("requires `collection`")))
        );
    }

    #[test]
    fn reference_to_undeclared_collection_is_invalid() {
        let json = one_collection(
            r#"{ "key": "v", "type": "reference", "label": "V", "collection": "ghosts" }"#,
        );
        let err = parse(&json).unwrap_err();
        assert!(
            matches!(err, ContractError::Invalid(ref v) if v.iter().any(|m| m.contains("undeclared collection `ghosts`")))
        );
    }

    #[test]
    fn object_without_schema_is_invalid() {
        let json = one_collection(r#"{ "key": "o", "type": "object", "label": "O" }"#);
        let err = parse(&json).unwrap_err();
        assert!(
            matches!(err, ContractError::Invalid(ref v) if v.iter().any(|m| m.contains("no freeform-json backdoor")))
        );
    }

    #[test]
    fn enum_without_values_is_invalid() {
        let json = one_collection(r#"{ "key": "e", "type": "enum", "label": "E" }"#);
        let err = parse(&json).unwrap_err();
        assert!(
            matches!(err, ContractError::Invalid(ref v) if v.iter().any(|m| m.contains("non-empty `values`")))
        );
    }

    #[test]
    fn decimal_scale_exceeding_precision_is_invalid() {
        let json = one_collection(
            r#"{ "key": "d", "type": "decimal", "label": "D", "precision": 2, "scale": 4 }"#,
        );
        let err = parse(&json).unwrap_err();
        assert!(
            matches!(err, ContractError::Invalid(ref v) if v.iter().any(|m| m.contains("cannot exceed precision")))
        );
    }

    #[test]
    fn decimal_without_precision_is_invalid() {
        let json = one_collection(r#"{ "key": "d", "type": "decimal", "label": "D" }"#);
        let err = parse(&json).unwrap_err();
        assert!(
            matches!(err, ContractError::Invalid(ref v) if v.iter().any(|m| m.contains("requires `precision` and `scale`")))
        );
    }

    // --- structural: lifecycle + flags ---

    #[test]
    fn lifecycle_initial_not_a_status_is_invalid() {
        let json = r#"{ "format_version": 1, "collections": [ { "name": "parts",
            "id": { "scheme": "nano14", "default": true, "mintable": true },
            "lifecycle": { "statuses": ["a","b"], "transitions": {}, "initial": "z" },
            "fields": [] } ] }"#;
        let err = parse(json).unwrap_err();
        assert!(
            matches!(err, ContractError::Invalid(ref v) if v.iter().any(|m| m.contains("initial `z` is not a declared status")))
        );
    }

    #[test]
    fn transition_to_undeclared_status_is_invalid() {
        let json = r#"{ "format_version": 1, "collections": [ { "name": "parts",
            "id": { "scheme": "nano14", "default": true, "mintable": true },
            "lifecycle": { "statuses": ["a","b"], "transitions": { "a": ["q"] }, "initial": "a" },
            "fields": [] } ] }"#;
        let err = parse(json).unwrap_err();
        assert!(
            matches!(err, ContractError::Invalid(ref v) if v.iter().any(|m| m.contains("targets undeclared status")))
        );
    }

    #[test]
    fn required_to_enter_unknown_status_is_invalid() {
        let json = r#"{ "format_version": 1, "collections": [ { "name": "parts",
            "id": { "scheme": "nano14", "default": true, "mintable": true },
            "lifecycle": { "statuses": ["a","b"], "transitions": {}, "initial": "a" },
            "fields": [ { "key": "x", "type": "string", "label": "X", "required_to_enter": "z" } ] } ] }"#;
        let err = parse(json).unwrap_err();
        assert!(
            matches!(err, ContractError::Invalid(ref v) if v.iter().any(|m| m.contains("required_to_enter `z` is not a declared status")))
        );
    }

    #[test]
    fn required_to_enter_without_lifecycle_is_invalid() {
        let json = one_collection(
            r#"{ "key": "x", "type": "string", "label": "X", "required_to_enter": "bound" }"#,
        );
        let err = parse(&json).unwrap_err();
        assert!(
            matches!(err, ContractError::Invalid(ref v) if v.iter().any(|m| m.contains("has no lifecycle")))
        );
    }

    // --- structural: uniqueness + default scheme ---

    #[test]
    fn duplicate_collection_names_invalid() {
        let json = r#"{ "format_version": 1, "collections": [
            { "name": "parts", "id": { "scheme": "nano14", "default": true, "mintable": true }, "fields": [] },
            { "name": "parts", "id": { "scheme": "nano14", "default": false, "mintable": true }, "fields": [] } ] }"#;
        let err = parse(json).unwrap_err();
        assert!(
            matches!(err, ContractError::Invalid(ref v) if v.iter().any(|m| m.contains("duplicate collection name")))
        );
    }

    #[test]
    fn duplicate_field_keys_invalid() {
        let json = one_collection(
            r#"{ "key": "x", "type": "string", "label": "X" }, { "key": "x", "type": "string", "label": "X2" }"#,
        );
        let err = parse(&json).unwrap_err();
        assert!(
            matches!(err, ContractError::Invalid(ref v) if v.iter().any(|m| m.contains("duplicate field key `x`")))
        );
    }

    #[test]
    fn zero_default_schemes_invalid() {
        let json = r#"{ "format_version": 1, "collections": [
            { "name": "parts", "id": { "scheme": "nano14", "default": false, "mintable": true }, "fields": [] } ] }"#;
        let err = parse(json).unwrap_err();
        assert!(
            matches!(err, ContractError::Invalid(ref v) if v.iter().any(|m| m.contains("exactly one collection must be the default")))
        );
    }

    #[test]
    fn two_default_schemes_invalid() {
        let json = r#"{ "format_version": 1, "collections": [
            { "name": "parts", "id": { "scheme": "nano14", "default": true, "mintable": true }, "fields": [] },
            { "name": "vendors", "id": { "scheme": "nano14", "default": true, "mintable": true }, "fields": [] } ] }"#;
        let err = parse(json).unwrap_err();
        assert!(
            matches!(err, ContractError::Invalid(ref v) if v.iter().any(|m| m.contains("exactly one collection")))
        );
    }

    #[test]
    fn closed_facet_rejects_unknown_semantics() {
        assert!(Closed::Flag(true).rejects_unknown());
        assert!(!Closed::Flag(false).rejects_unknown());
        assert!(!Closed::Mode("warn".into()).rejects_unknown());
    }

    #[test]
    fn all_structural_errors_collected_in_one_pass() {
        // Two independent problems → both reported, not just the first.
        let json = r#"{ "format_version": 1, "collections": [ { "name": "parts",
            "id": { "scheme": "nano14", "default": false, "mintable": true },
            "fields": [ { "key": "v", "type": "reference", "label": "V" } ] } ] }"#;
        let err = parse(json).unwrap_err();
        match err {
            ContractError::Invalid(v) => assert!(v.len() >= 2, "expected ≥2 errors, got {v:?}"),
            other => panic!("expected Invalid, got {other:?}"),
        }
    }
}
