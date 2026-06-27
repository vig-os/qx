//! `.qx/manifest.toml` — the per-registry policy manifest (ADR-034 §3).
//!
//! The manifest declares **policy**, not an authz engine: the registry
//! identity, which operations are enabled at the `(op-family ×
//! collection [× edge])` grain, and an *advisory* role→capability map
//! keyed on the `{collection, op-kind}` unified change vocabulary. It
//! declares **no render structure** (single-home rule — layouts and
//! groupings live only in the contract descriptor).
//!
//! CI cross-checks the manifest↔contract FK: every collection named by an
//! `[ops]` key or a role-capability key must be a contract-declared
//! collection (`capability-grain`).

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

/// A parsed `.qx/manifest.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub registry: RegistryMeta,
    /// Enabled operations at the `(op-family × collection [× edge])`
    /// grain. Keys are `"<family>:<collection>"` or
    /// `"<family>:<collection>:<edge>"` (e.g. `"create:parts"`,
    /// `"transition:parts:void"`); values are `"on"`/`"off"`.
    #[serde(default)]
    pub ops: BTreeMap<String, OpState>,
    /// Advisory role → capability map. `roles.<role>` maps a
    /// `"<collection>:<op-kind>"` change class to the elevation it needs
    /// (e.g. `"approve"`). The CODEOWNERS seed is generated from this.
    #[serde(default)]
    pub roles: BTreeMap<String, BTreeMap<String, String>>,
}

/// Registry identity + metadata.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryMeta {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub owner: String,
    /// Optional `min_producer_version` POSTURE KNOB (ADR-038 §3): the
    /// minimum producer (tool) version a write op should self-report. A
    /// *claim*, never the security argument — the enforced boundary is
    /// always content-validated-against-current-contract at the gate.
    /// `None` = no floor advertised.
    #[serde(default)]
    pub min_producer_version: Option<String>,
}

/// Whether an `(op, collection)` is enabled in this registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OpState {
    On,
    Off,
}

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("manifest parse error: {0}")]
    Parse(String),
}

impl Manifest {
    /// Parse a manifest from its TOML text.
    pub fn parse(text: &str) -> Result<Self, ManifestError> {
        toml::from_str(text).map_err(|e| ManifestError::Parse(e.to_string()))
    }

    /// The collection each `[ops]` / role-capability key targets — the FK
    /// target set the contract must declare. The collection is the second
    /// `:`-segment of an ops key and the first of a role-capability key.
    fn referenced_collections(&self) -> BTreeMap<String, &'static str> {
        let mut refs: BTreeMap<String, &'static str> = BTreeMap::new();
        for key in self.ops.keys() {
            if let Some(coll) = key.split(':').nth(1) {
                refs.entry(coll.to_string()).or_insert("ops");
            }
        }
        for caps in self.roles.values() {
            for key in caps.keys() {
                if let Some(coll) = key.split(':').next() {
                    refs.entry(coll.to_string()).or_insert("roles");
                }
            }
        }
        refs
    }

    /// Generate the `.github/CODEOWNERS` body from the advisory role →
    /// capability map (ADR-034 §4: the role map is "the source the
    /// CODEOWNERS seed is generated from"). Each collection any role holds
    /// a capability over becomes a CODEOWNERS rule mapping its data path to
    /// the owning role team(s) — GitHub then enforces the human review. The
    /// tool *generates + advises*; it does not gate. The owner token is the
    /// role as a team handle (`@<role>`); the deployment binds the team's
    /// membership (ADR-036 personas / the org).
    pub fn codeowners_body(&self) -> String {
        let mut by_collection: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (role, caps) in &self.roles {
            for key in caps.keys() {
                if let Some(coll) = key.split(':').next() {
                    if !coll.is_empty() {
                        by_collection
                            .entry(coll.to_string())
                            .or_default()
                            .insert(format!("@{role}"));
                    }
                }
            }
        }
        let mut out = String::new();
        out.push_str("# Generated from .qx/manifest.toml role→capability map (ADR-034 §4).\n");
        out.push_str("# The tool advises; GitHub branch protection + CODEOWNERS enforce.\n");
        for (coll, owners) in &by_collection {
            let owners: Vec<&str> = owners.iter().map(String::as_str).collect();
            out.push_str(&format!("/collections/{coll}.jsonl {}\n", owners.join(" ")));
        }
        out
    }

    /// Whether a feature (op-family, e.g. `print`) is enabled for a
    /// collection (ADR-031 §8 / ADR-034 §3 — the manifest gates the
    /// feature). A `<family>:<collection>` ops key set to `off` disables
    /// it; absence means enabled (default = all reads + writes-via-
    /// proposal). A disabled `(op, collection)` disappears from the shells.
    pub fn feature_enabled(&self, family: &str, collection: &str) -> bool {
        let key = format!("{family}:{collection}");
        !matches!(self.ops.get(&key), Some(OpState::Off))
    }

    /// Manifest↔contract FK (`capability-grain`): every collection named
    /// by an `[ops]` key or a role-capability key must be declared in the
    /// contract. Returns one message per dangling reference.
    pub fn contract_fk_issues(&self, declared_collections: &[&str]) -> Vec<String> {
        let mut out = Vec::new();
        for (coll, origin) in self.referenced_collections() {
            if coll.is_empty() {
                out.push(format!(
                    "manifest [{origin}]: a key names an empty collection (expected `<op>:<collection>`)"
                ));
            } else if !declared_collections.contains(&coll.as_str()) {
                out.push(format!(
                    "manifest [{origin}]: references collection `{coll}`, which is not declared in the contract"
                ));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
        [registry]
        id = "acme-parts"
        name = "Acme Parts"
        owner = "persona:alice"

        [ops]
        "create:parts" = "on"
        "create:companies" = "off"
        "transition:parts:void" = "on"

        [roles.quality-lead]
        "companies:bulk" = "approve"
    "#;

    #[test]
    fn parses_identity_ops_and_roles() {
        let m = Manifest::parse(SAMPLE).expect("parses");
        assert_eq!(m.registry.id, "acme-parts");
        assert_eq!(m.ops["create:companies"], OpState::Off);
        assert_eq!(m.ops["transition:parts:void"], OpState::On);
        assert_eq!(m.roles["quality-lead"]["companies:bulk"], "approve");
    }

    #[test]
    fn fk_passes_when_every_collection_is_declared() {
        let m = Manifest::parse(SAMPLE).unwrap();
        assert!(m.contract_fk_issues(&["parts", "companies"]).is_empty());
    }

    #[test]
    fn fk_flags_collection_absent_from_contract() {
        let m = Manifest::parse(SAMPLE).unwrap();
        // `companies` is referenced by both ops and roles but not declared.
        let issues = m.contract_fk_issues(&["parts"]);
        assert_eq!(
            issues.len(),
            1,
            "one dangling collection, deduped: {issues:?}"
        );
        assert!(issues[0].contains("companies"));
        assert!(issues[0].contains("not declared"));
    }

    #[test]
    fn codeowners_body_maps_collections_to_role_teams() {
        // ADR-034 §4: the role→capability map is the CODEOWNERS seed source.
        let body = Manifest::parse(SAMPLE).unwrap().codeowners_body();
        // SAMPLE gives quality-lead a capability on `companies`.
        assert!(
            body.contains("/collections/companies.jsonl @quality-lead"),
            "expected a companies rule owned by @quality-lead:\n{body}"
        );
        // The tool advises; it does not gate (documented in the header).
        assert!(body.contains("advises") && body.contains("enforce"));
    }

    #[test]
    fn manifest_gates_a_feature_per_collection() {
        // ADR-031 §8 / ADR-034 §3: the manifest gates a feature (op-family)
        // per collection. Absence = enabled; an explicit `off` disables.
        let m = Manifest::parse(
            "[registry]\nid=\"x\"\nname=\"X\"\n[ops]\n\"print:parts\" = \"off\"\n\"print:labels\" = \"on\"\n",
        )
        .unwrap();
        assert!(
            !m.feature_enabled("print", "parts"),
            "explicit off disables"
        );
        assert!(m.feature_enabled("print", "labels"), "explicit on enables");
        assert!(
            m.feature_enabled("print", "widgets"),
            "absence = enabled (default)"
        );
    }

    #[test]
    fn unknown_top_level_key_is_rejected() {
        let bad = "[registry]\nid=\"x\"\nname=\"X\"\n[bogus]\nk=1\n";
        assert!(Manifest::parse(bad).is_err());
    }
}
