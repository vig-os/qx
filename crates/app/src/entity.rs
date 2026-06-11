//! The protocol `Entity` — the ADR-035 micro-core render every shell
//! consumes (`{id, collection, label, created_at, status, kind,
//! transitioned_at, fields, properties}`).
//!
//! Today's storage still carries the pre-ADR-035 `Part` shape
//! (`minted_at` / `bound_at` / legacy `batch` column); this module is
//! the seam that renders it into the metamodel shape — `created_at` =
//! `minted_at`, `transitioned_at["bound"]` = `bound_at`, legacy
//! `batch` surfaces under `properties` until the data migration lands
//! (obligations `lifecycle-timestamps`, `batch-deprecated`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use time::format_description::well_known::Rfc3339;

use part_registry_domain::Part;

/// Wire entity per ADR-035 §0 (micro-core + declared fields + open
/// properties). Timestamps are RFC-3339 strings on the wire.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    pub id: String,
    pub collection: String,
    pub label: Option<String>,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub transitioned_at: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub properties: serde_json::Map<String, Json>,
}

fn rfc3339(ts: &time::OffsetDateTime) -> String {
    ts.format(&Rfc3339)
        .unwrap_or_else(|_| ts.unix_timestamp().to_string())
}

/// Render a stored [`Part`] into the protocol [`Entity`].
pub fn part_to_entity(p: &Part) -> Entity {
    let mut fields = BTreeMap::new();
    let mut put = |k: &str, v: &Option<String>| {
        if let Some(v) = v {
            fields.insert(k.to_string(), v.clone());
        }
    };
    put("type", &p.type_);
    put("description", &p.description);
    put("vendor", &p.vendor);
    put("part_number", &p.part_number);
    put("location", &p.location);
    put("notes", &p.notes);

    let mut transitioned_at = BTreeMap::new();
    if let Some(b) = &p.bound_at {
        transitioned_at.insert("bound".to_string(), rfc3339(b));
    }

    let mut properties = serde_json::Map::new();
    if let Some(batch) = &p.batch {
        // Legacy column (ADR-035 retires `batch`); surfaced as an open
        // property until the data migration drops it.
        properties.insert("batch".to_string(), Json::String(batch.clone()));
    }

    Entity {
        id: p.id.as_str().to_string(),
        collection: "parts".to_string(),
        label: None,
        created_at: rfc3339(&p.minted_at),
        status: Some(p.status.to_string()),
        kind: None,
        transitioned_at,
        fields,
        properties,
    }
}

/// One field value as seen by filters/sort: core fields by name, then
/// declared fields, then properties (stringified).
pub fn field_value(e: &Entity, key: &str) -> Option<String> {
    match key {
        "id" => Some(e.id.clone()),
        "status" => e.status.clone(),
        "kind" => e.kind.clone(),
        "created_at" => Some(e.created_at.clone()),
        _ => e
            .fields
            .get(key)
            .cloned()
            .or_else(|| e.transitioned_at.get(key).map(ToOwned::to_owned))
            .or_else(|| e.properties.get(key).map(json_to_string)),
    }
}

fn json_to_string(v: &Json) -> String {
    match v {
        Json::String(s) => s.clone(),
        other => other.to_string(),
    }
}
