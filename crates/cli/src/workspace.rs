//! `registries.toml` — the operator workspace (ADR-033 §5).
//!
//! Lists the registries an operator uses (`name → locator + default
//! identity/profile`) for quick switching and as the home for any
//! cross-registry UX. Single-registry operations still take a locator
//! directly; the workspace is the convenience layer that resolves a short
//! `name` to its locator.
//!
//! Lives at `$XDG_CONFIG_HOME/qx/registries.toml` (─ `~/.config/qx/` on
//! Linux, Application Support on macOS), resolved via `dirs`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;

/// A parsed `registries.toml` workspace.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workspace {
    /// Name of the registry selected by default (must be a key of
    /// `registries`). Optional — single-registry operators may omit it.
    #[serde(default)]
    pub default: Option<String>,
    /// `name → entry`. The name is the short handle an operator switches
    /// by; the entry carries the locator + optional identity/profile.
    #[serde(default)]
    pub registries: BTreeMap<String, RegistryEntry>,
}

/// One workspace registry: where it lives + the default identity to act
/// as there.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryEntry {
    /// The registry locator — a `github:owner/repo` ref or a local path.
    pub locator: String,
    /// Default operator identity/profile to use against this registry
    /// (e.g. a persona slug or a git identity profile). Optional.
    #[serde(default)]
    pub identity: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("workspace parse error: {0}")]
    Parse(String),
    #[error("workspace integrity: {0}")]
    Integrity(String),
}

impl Workspace {
    /// The conventional workspace path: `<config-dir>/qx/registries.toml`.
    pub fn default_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("qx").join("registries.toml"))
    }

    /// Parse a workspace from its TOML text, validating that `default`
    /// (when set) names a listed registry.
    pub fn parse(text: &str) -> Result<Self, WorkspaceError> {
        let ws: Workspace =
            toml::from_str(text).map_err(|e| WorkspaceError::Parse(e.to_string()))?;
        if let Some(name) = &ws.default {
            if !ws.registries.contains_key(name) {
                return Err(WorkspaceError::Integrity(format!(
                    "default = `{name}` is not a listed registry"
                )));
            }
        }
        Ok(ws)
    }

    /// The registries an operator can switch between, sorted by name.
    pub fn names(&self) -> Vec<&str> {
        self.registries.keys().map(String::as_str).collect()
    }

    /// Resolve a registry `name` (or, when `name` is `None`, the `default`)
    /// to its entry — the switching primitive.
    pub fn resolve(&self, name: Option<&str>) -> Option<&RegistryEntry> {
        let key = name.or(self.default.as_deref())?;
        self.registries.get(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
        default = "acme"

        [registries.acme]
        locator = "github:acme/parts"
        identity = "persona:alice"

        [registries.local-dev]
        locator = "/home/op/dev-registry"
    "#;

    #[test]
    fn parses_and_lists_registries() {
        let ws = Workspace::parse(SAMPLE).expect("parses");
        assert_eq!(ws.names(), vec!["acme", "local-dev"]);
        assert_eq!(ws.default.as_deref(), Some("acme"));
    }

    #[test]
    fn resolves_by_name_and_default() {
        let ws = Workspace::parse(SAMPLE).unwrap();
        assert_eq!(
            ws.resolve(Some("local-dev")).unwrap().locator,
            "/home/op/dev-registry"
        );
        // None → the default registry (the switching convenience).
        assert_eq!(ws.resolve(None).unwrap().locator, "github:acme/parts");
        assert_eq!(
            ws.resolve(Some("acme")).unwrap().identity.as_deref(),
            Some("persona:alice")
        );
        assert!(ws.resolve(Some("ghost")).is_none());
    }

    #[test]
    fn default_must_name_a_listed_registry() {
        let bad = "default = \"missing\"\n[registries.acme]\nlocator = \"x\"\n";
        assert!(matches!(
            Workspace::parse(bad),
            Err(WorkspaceError::Integrity(_))
        ));
    }

    #[test]
    fn unknown_key_is_rejected() {
        let bad = "[registries.acme]\nlocator = \"x\"\nbogus = 1\n";
        assert!(Workspace::parse(bad).is_err());
    }
}
