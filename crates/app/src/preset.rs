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

/// Enforce the non-weakenable `parts` **lifecycle** floor (ADR-035 §1): a
/// registry's contract may *extend* the regulated preset but never weaken
/// the binding state machine. If the contract declares a `parts`
/// collection it must keep the regulated lifecycle statuses
/// (`unbound`/`bound`/`void`) and the `unbound` initial. Returns the list
/// of violations (empty == satisfied).
///
/// Scope (honest): the regulated floor enforced here is the lifecycle —
/// the part of the preset ADR-035 §1 settles as load-bearing. Which
/// *fields* count as floor, and the rename-around / non-instantiation
/// bypass (dodging by not naming the collection `parts`), are the open
/// design questions of ADR-040 spike #216 and are deliberately NOT decided
/// here — a parts-less contract passes.
pub fn assert_parts_floor(contract: &qx_contract::Contract) -> Result<(), Vec<String>> {
    let Some(parts) = contract.collection("parts") else {
        return Ok(()); // no regulated `parts` declared → nothing to floor
    };
    let floor = parts_descriptor();
    let floor_lc = floor
        .lifecycle
        .as_ref()
        .expect("parts floor always declares a lifecycle");
    let mut errs = Vec::new();

    match &parts.lifecycle {
        None => errs.push("drops the regulated unbound→bound→void lifecycle".to_string()),
        Some(lc) => {
            for s in &floor_lc.statuses {
                if !lc.statuses.iter().any(|x| x == s) {
                    errs.push(format!("lifecycle drops the floor status `{s}`"));
                }
            }
            if lc.initial != floor_lc.initial {
                errs.push(format!(
                    "lifecycle initial must remain `{}` (floor)",
                    floor_lc.initial
                ));
            }
        }
    }

    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}

/// Map a parsed `contract::Collection` to the `Describe` payload
/// (ADR-039: the contract types ARE the schema, so this is a mechanical
/// 1:1 projection — the descriptor is the contract's display view).
pub fn descriptor_from_contract(c: &qx_contract::Collection) -> CollectionDescriptor {
    CollectionDescriptor {
        name: c.name.clone(),
        id: IdScheme {
            scheme: c.id.scheme.clone(),
            default: c.id.default,
            mintable: c.id.mintable,
            prefix_length: c.id.prefix_length,
        },
        lifecycle: c.lifecycle.as_ref().map(|lc| Lifecycle {
            statuses: lc.statuses.clone(),
            transitions: lc.transitions.clone(),
            initial: lc.initial.clone(),
        }),
        fields: c
            .fields
            .iter()
            .map(|f| FieldDescriptor {
                key: f.key.clone(),
                type_: field_type_str(&f.type_).to_string(),
                label: f.label.clone(),
                editable: f.editable,
                meaningful_from: f.meaningful_from.clone(),
            })
            .collect(),
        render: RenderBlock {
            label_fields: c
                .render
                .as_ref()
                .map(|r| r.label_fields.clone())
                .unwrap_or_else(|| vec!["id".into()]),
        },
    }
}

/// The contract's `FieldType` enum as its canonical scalar-set string.
fn field_type_str(t: &qx_contract::FieldType) -> &'static str {
    use qx_contract::FieldType::*;
    match t {
        String => "string",
        Enum => "enum",
        Integer => "integer",
        Number => "number",
        Decimal => "decimal",
        Date => "date",
        Timestamp => "timestamp",
        Bool => "bool",
        Reference => "reference",
        Attachment => "attachment",
        Object => "object",
    }
}

#[cfg(test)]
mod floor_tests {
    use super::{assert_parts_floor, descriptor_from_contract, parts_descriptor};
    use qx_contract::Contract;

    #[test]
    fn contract_maps_to_descriptor_matching_the_parts_preset() {
        // A contract whose `parts` mirrors the preset maps 1:1 to it.
        let c = Contract::from_bytes(FLOOR_OK).unwrap();
        let d = descriptor_from_contract(c.collection("parts").unwrap());
        let preset = parts_descriptor();
        assert_eq!(d.name, preset.name);
        assert_eq!(d.lifecycle, preset.lifecycle);
        assert_eq!(
            d.fields.iter().map(|f| &f.key).collect::<Vec<_>>(),
            preset.fields.iter().map(|f| &f.key).collect::<Vec<_>>()
        );
    }

    const FLOOR_OK: &[u8] = br#"{"format_version":1,"collections":[
        {"name":"parts","id":{"scheme":"nano14","default":true,"mintable":true},
         "lifecycle":{"statuses":["unbound","bound","void"],"initial":"unbound",
           "transitions":{"unbound":["bound","void"],"bound":["void"],"void":[]}},
         "fields":[
           {"key":"type","type":"string","label":"Type","required_to_enter":"bound"},
           {"key":"description","type":"string","label":"Description","required_to_enter":"bound"},
           {"key":"vendor","type":"string","label":"Vendor","required_to_enter":"bound"},
           {"key":"part_number","type":"string","label":"Part number","required_to_enter":"bound"},
           {"key":"location","type":"string","label":"Location","required_to_enter":"bound"},
           {"key":"notes","type":"string","label":"Notes"}]}]}"#;

    #[test]
    fn floor_is_satisfied_by_the_preset_shape() {
        let c = Contract::from_bytes(FLOOR_OK).unwrap();
        assert!(
            assert_parts_floor(&c).is_ok(),
            "{:?}",
            assert_parts_floor(&c)
        );
    }

    #[test]
    fn dropping_the_void_status_is_rejected() {
        // A parts lifecycle without the regulated `void` terminal status.
        let c = Contract::from_bytes(
            br#"{"format_version":1,"collections":[
            {"name":"parts","id":{"scheme":"nano14","default":true,"mintable":true},
             "lifecycle":{"statuses":["unbound","bound"],"initial":"unbound",
               "transitions":{"unbound":["bound"],"bound":[]}},
             "fields":[{"key":"type","type":"string","label":"Type"}]}]}"#,
        )
        .unwrap();
        let errs = assert_parts_floor(&c).unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.contains("drops the floor status `void`")),
            "{errs:?}"
        );
    }

    #[test]
    fn changing_the_initial_status_is_rejected() {
        let c = Contract::from_bytes(
            br#"{"format_version":1,"collections":[
            {"name":"parts","id":{"scheme":"nano14","default":true,"mintable":true},
             "lifecycle":{"statuses":["unbound","bound","void"],"initial":"bound",
               "transitions":{"unbound":["bound"],"bound":["void"],"void":[]}},
             "fields":[{"key":"type","type":"string","label":"Type"}]}]}"#,
        )
        .unwrap();
        let errs = assert_parts_floor(&c).unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.contains("initial must remain `unbound`")),
            "{errs:?}"
        );
    }

    #[test]
    fn parts_less_registry_passes() {
        // No `parts` collection → nothing to floor (rename-around bypass is
        // spike #216's concern, documented on the function).
        let c = Contract::from_bytes(
            br#"{"format_version":1,"collections":[
            {"name":"companies","id":{"scheme":"nano14","default":true,"mintable":true},
             "fields":[{"key":"label","type":"string","label":"Label"}]}]}"#,
        )
        .unwrap();
        assert!(assert_parts_floor(&c).is_ok());
    }
}
