//! Code-owned collection presets per ADR-035 §0: the regulated floor
//! ships in code (non-weakenable); a registry's `contract.json`
//! instantiates and may *extend* these, never weaken them. Until the
//! contract engine lands (obligation `registry-self-describing`),
//! `Describe` serves exactly the presets.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Descriptor of one collection — the `Describe` payload. Field labels
/// here are the SSOT for every shell's rendered strings (ADR-035 §1a:
/// no hardcoded display strings in shells).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CollectionDescriptor {
    pub name: String,
    pub id: IdScheme,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<Lifecycle>,
    pub fields: Vec<FieldDescriptor>,
    pub render: RenderBlock,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IdScheme {
    /// e.g. "nano14" (ADR-012 / ADR-035 typed ids).
    pub scheme: String,
    /// Bare values parse as this scheme (ADR-035: one default per
    /// registry; QR payloads stay bare).
    pub default: bool,
    /// Whether ids are minted here or imported/asserted.
    pub mintable: bool,
    /// Human prefix length accepted for resolution (nano14: 8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix_length: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Lifecycle {
    pub statuses: Vec<String>,
    /// Allowed transitions: from-status → list of to-statuses.
    pub transitions: BTreeMap<String, Vec<String>>,
    pub initial: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FieldDescriptor {
    pub key: String,
    /// "string" | "enum" | "integer" | "number" | "date" | "bool" |
    /// "attachment" (ADR-033 §3 scalar set).
    #[serde(rename = "type")]
    pub type_: String,
    /// Display label — descriptor-owned (ADR-035 §1a).
    pub label: String,
    pub editable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meaningful_from: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RenderBlock {
    /// Which fields compose an entity's short label rendering.
    pub label_fields: Vec<String>,
}

/// `Describe` payload for the whole registry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RegistryDescriptor {
    pub name: String,
    pub collections: Vec<CollectionDescriptor>,
}

fn field(
    key: &str,
    type_: &str,
    label: &str,
    editable: bool,
    meaningful_from: Option<&str>,
) -> FieldDescriptor {
    FieldDescriptor {
        key: key.into(),
        type_: type_.into(),
        label: label.into(),
        editable,
        meaningful_from: meaningful_from.map(Into::into),
    }
}

/// The `parts` preset — the ADR-012/035 regulated floor.
pub fn parts_descriptor() -> CollectionDescriptor {
    let mut transitions = BTreeMap::new();
    transitions.insert("unbound".to_string(), vec!["bound".into(), "void".into()]);
    transitions.insert("bound".to_string(), vec!["void".into()]);
    transitions.insert("void".to_string(), Vec::new());

    CollectionDescriptor {
        name: "parts".into(),
        id: IdScheme {
            scheme: "nano14".into(),
            default: true,
            mintable: true,
            prefix_length: Some(8),
        },
        lifecycle: Some(Lifecycle {
            statuses: vec!["unbound".into(), "bound".into(), "void".into()],
            transitions,
            initial: "unbound".into(),
        }),
        fields: vec![
            field("type", "string", "Type", true, Some("bound")),
            field("description", "string", "Description", true, Some("bound")),
            field("vendor", "string", "Vendor", true, Some("bound")),
            field("part_number", "string", "Part number", true, Some("bound")),
            field("location", "string", "Location", true, Some("bound")),
            field("notes", "string", "Notes", true, None),
        ],
        render: RenderBlock {
            label_fields: vec!["id".into(), "type".into()],
        },
    }
}

/// The registry descriptor served by `Describe` (presets only until
/// the per-registry contract engine lands).
pub fn registry_descriptor(name: &str) -> RegistryDescriptor {
    RegistryDescriptor {
        name: name.to_string(),
        collections: vec![parts_descriptor()],
    }
}
