//! `qx-config` — 12-factor configuration loader per ADR-021.
//!
//! Single read site for every deploy-varying value. Domain crates
//! never call `std::env::var`; ADR-027 §Tier 4 drift-detection
//! enforces this with a workspace grep.
//!
//! ## Layered precedence
//!
//! Per ADR-021 §"Decision":
//!
//! ```text
//! built-in defaults (defaults.toml, embedded via include_str!)
//!     <  per-deploy override TOML (test/config-file layer)
//!     <  environment variables (PART_REGISTRY__* with double-underscore
//!                               nested-key separator)
//! ```
//!
//! ## Env var convention
//!
//! Per the foundation parallelism audit (cross-cutting reviewer
//! question on figment prefix splitting): the env-var nested-key
//! separator is **double-underscore** (`__`). Single-underscore would
//! collide with `default_size_mm` field names that themselves contain
//! underscores. Examples:
//!
//! ```text
//! PART_REGISTRY__STORAGE__ADAPTER=sqlite
//! PART_REGISTRY__LABEL__DEFAULT_SIZE_MM=8.0
//! PART_REGISTRY__OBSERVABILITY__LOG_LEVEL=debug
//! ```
//!
//! Closes interface-sharpness concern raised by the cross-cutting
//! reviewer; the convention is documented here so adapter-side
//! `env::var` lookups never disagree on the separator.

#![forbid(unsafe_code)]

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config parse error: {0}")]
    Parse(String),
    #[error("invalid data_repo_url {url:?}: {reason}")]
    InvalidRepoUrl { url: String, reason: String },
    #[error("cannot resolve data path: no XDG data dir and no explicit local_clone_path")]
    NoDataDir,
}

impl From<figment::Error> for ConfigError {
    fn from(value: figment::Error) -> Self {
        ConfigError::Parse(value.to_string())
    }
}

// -------------------------------------------------------------------
// Top-level Config
// -------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub repo: RepoConfig,
    pub storage: StorageConfig,
    pub identity: IdentityConfig,
    pub transport: TransportConfig,
    pub signing: SigningConfig,
    pub label: LabelDefaults,
    pub observability: ObservabilityConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RepoConfig {
    /// env: `PART_REGISTRY__REPO__DATA_REPO_URL`
    ///
    /// Accepts `owner/repo` short form, `https://github.com/owner/repo[.git]`,
    /// or `git@github.com:owner/repo.git` per [`Config::data_repo_owner_repo`].
    pub data_repo_url: String,
    /// env: `PART_REGISTRY__REPO__CODE_REPO_URL`
    pub code_repo_url: String,
    /// env: `PART_REGISTRY__REPO__LOCAL_CLONE_PATH`
    ///
    /// When unset, [`Config::resolve_data_path`] derives the path as
    /// `$XDG_DATA_HOME/qx/<owner>-<repo>/` (Linux) or the
    /// platform-equivalent under `dirs::data_dir()`. Operators only
    /// set this for hermetic test environments or shared-storage
    /// hosts where XDG resolution is wrong.
    #[serde(default)]
    pub local_clone_path: Option<PathBuf>,
    /// env: `PART_REGISTRY__REPO__BRANCH` — default `main`
    pub branch: String,
}

/// Adapter selection for the storage port (ADR-018).
///
/// Flat enum rather than `#[serde(tag)]` tagged enum — see ADR-021
/// §Corrections for rationale. Associated config (e.g. `sqlite_path`)
/// lives as sibling `Option<T>` fields on [`StorageConfig`]; adapters
/// validate required fields at construction time.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StorageAdapterChoice {
    CsvGit,
    Sqlite,
    DuckDb,
    Dolt,
    FilePerEntry,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StorageConfig {
    /// env: `PART_REGISTRY__STORAGE__ADAPTER`
    pub adapter: StorageAdapterChoice,
    /// env: `PART_REGISTRY__STORAGE__SQLITE_PATH` — when adapter=sqlite
    pub sqlite_path: Option<PathBuf>,
    /// env: `PART_REGISTRY__STORAGE__DUCKDB_PATH` — when adapter=duckdb
    pub duckdb_path: Option<PathBuf>,
}

/// Adapter selection for the identity port (ADR-020).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IdentityAdapterChoice {
    GitConfig,
    GithubOauth,
    EnvUser,
    OidcGeneric,
    MtlsCert,
    SigstoreKeyless,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IdentityConfig {
    /// env: `PART_REGISTRY__IDENTITY__ADAPTER`
    pub adapter: IdentityAdapterChoice,
    /// env: `PART_REGISTRY__IDENTITY__VERIFIED_AT_WINDOW_SECS`
    pub verified_at_window_secs: u64,
    /// env: `PART_REGISTRY__IDENTITY__GITHUB_CLIENT_ID`
    pub github_client_id: Option<String>,
    /// env: `PART_REGISTRY__IDENTITY__OIDC_ISSUER`
    pub oidc_issuer: Option<String>,
    /// env: `PART_REGISTRY__IDENTITY__OIDC_CLIENT_ID`
    pub oidc_client_id: Option<String>,
    /// env: `PART_REGISTRY__IDENTITY__ALLOW_DEV_IDENTITY`
    /// — defaults to `false`. Production builds reject `env_user`
    /// adapter unless this is explicitly `true`.
    pub allow_dev_identity: bool,
}

/// Adapter selection for the transport / proposal-sink port (ADR-019).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransportAdapterChoice {
    GithubPr,
    LocalBranch,
    Webhook,
    Filesystem,
    Dolt,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransportConfig {
    /// env: `PART_REGISTRY__TRANSPORT__ADAPTER`
    pub adapter: TransportAdapterChoice,
    /// env: `PART_REGISTRY__TRANSPORT__GITHUB_TOKEN`
    pub github_token: Option<String>,
    /// env: `PART_REGISTRY__TRANSPORT__DEPOSIT_PATH`
    pub deposit_path: Option<PathBuf>,
}

/// Adapter selection for the signing port (ADR-024).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SigningAdapterChoice {
    GitCommit,
    Sigstore,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SigningConfig {
    /// env: `PART_REGISTRY__SIGNING__ADAPTER`
    pub adapter: SigningAdapterChoice,
    /// env: `PART_REGISTRY__SIGNING__FULCIO_URL` — when adapter=sigstore
    pub fulcio_url: Option<String>,
    /// env: `PART_REGISTRY__SIGNING__REKOR_URL` — when adapter=sigstore
    pub rekor_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelDefaults {
    /// env: `PART_REGISTRY__LABEL__DEFAULT_SIZE_MM` (default 11.0)
    pub default_size_mm: f64,
    /// env: `PART_REGISTRY__LABEL__FONT_FAMILY`
    pub font_family: String,
    /// env: `PART_REGISTRY__LABEL__LABELS_DIR`
    pub labels_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObservabilityConfig {
    /// env: `PART_REGISTRY__OBSERVABILITY__LOG_LEVEL` (default `info`)
    pub log_level: String,
    /// env: `PART_REGISTRY__OBSERVABILITY__AUDIT_LOG_PATH`
    pub audit_log_path: PathBuf,
    /// env: `PART_REGISTRY__OBSERVABILITY__STDOUT_JSON`
    pub stdout_json: bool,
    /// env: `PART_REGISTRY__OBSERVABILITY__STDERR_HUMAN`
    pub stderr_human: bool,
    /// env: `PART_REGISTRY__OBSERVABILITY__AUDIT_CSV`
    pub audit_csv: bool,
}

impl Default for ObservabilityConfig {
    /// Read-only / library defaults: stderr human on, stdout JSON off,
    /// audit-CSV off. Suitable for tests and the WASM facade.
    fn default() -> Self {
        Self {
            log_level: "info".into(),
            audit_log_path: PathBuf::from("./audit_log.csv"),
            stdout_json: false,
            stderr_human: true,
            audit_csv: false,
        }
    }
}

impl ObservabilityConfig {
    /// Defaults for a mutating CLI binary (`mint`, `bind`, `label`):
    /// stderr human + audit-CSV on, stdout JSON off by default.
    /// Per ADR-022: mutating processes MUST enable the audit-CSV layer.
    pub fn cli_defaults() -> Self {
        Self {
            log_level: "info".into(),
            audit_log_path: PathBuf::from("./audit_log.csv"),
            stdout_json: false,
            stderr_human: true,
            audit_csv: true,
        }
    }

    /// Defaults for CI runs: stdout JSON on (machine-parseable
    /// contract), stderr human off, audit-CSV gated on the workflow's
    /// `Repository` wiring.
    pub fn ci_defaults() -> Self {
        Self {
            log_level: "info".into(),
            audit_log_path: PathBuf::from("./audit_log.csv"),
            stdout_json: true,
            stderr_human: false,
            audit_csv: true,
        }
    }
}

const DEFAULTS_TOML: &str = include_str!("../defaults.toml");

/// Env-var prefix per ADR-021 §"Why `PART_REGISTRY_*` as the env prefix."
/// Combined with the double-underscore separator, full keys look like
/// `PART_REGISTRY__STORAGE__ADAPTER`.
pub const ENV_PREFIX: &str = "PART_REGISTRY_";

/// Nested-key separator inside an env var. **Double underscore** —
/// see crate docs §"Env var convention" for rationale.
pub const ENV_SEPARATOR: &str = "__";

impl Config {
    /// Parse defaults + env per ADR-021 §"Config crate shape."
    ///
    /// Layered precedence: built-in defaults < environment variables.
    /// For the deploy-file layer, use [`Config::from_env_with_overrides`].
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_layers(None, std::env::vars())
    }

    /// Parse defaults plus an additional intermediate TOML override
    /// layer plus env per ADR-021. Used by tests and per-deploy
    /// override files (e.g. `~/.config/qx/config.toml`).
    ///
    /// Precedence: defaults < `toml` argument < env vars.
    pub fn from_env_with_overrides(toml: &str) -> Result<Self, ConfigError> {
        Self::from_layers(Some(toml), std::env::vars())
    }

    /// Test-friendly entry point: parse defaults + optional override
    /// TOML + an explicit `(key, value)` iterator that stands in for
    /// the process environment. Production callers use
    /// [`Config::from_env`] / [`Config::from_env_with_overrides`]
    /// which feed `std::env::vars()` here.
    ///
    /// Decoupling the env source from process-global state lets tests
    /// avoid `std::env::set_var` (which Rust 1.85+ correctly marks
    /// `unsafe` because POSIX `setenv` is not thread-safe and cargo
    /// runs tests in parallel by default).
    pub fn from_layers<I, K, V>(overrides: Option<&str>, env: I) -> Result<Self, ConfigError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        use figment::{
            providers::{Format, Toml},
            Figment,
        };

        let mut figment = Figment::new().merge(Toml::string(DEFAULTS_TOML));
        if let Some(toml) = overrides {
            figment = figment.merge(Toml::string(toml));
        }

        // Build an Env provider seeded from our iterator instead of
        // `std::env::vars()`. We do this by collecting the relevant
        // pairs into a HashMap and using figment's `Env::raw()` with
        // an iterator override — but figment 0.10's `Env::prefixed`
        // always pulls from `std::env`, so we transcribe the pairs
        // into a TOML layer that mirrors the env precedence.
        //
        // Mapping rule (matches `Env::prefixed(PREFIX).split("__")`):
        //   PART_REGISTRY__STORAGE__ADAPTER=sqlite
        //     -> [storage] adapter = "sqlite"
        let env_toml = env_pairs_to_toml(env);
        if !env_toml.is_empty() {
            figment = figment.merge(Toml::string(&env_toml));
        }

        Ok(figment.extract()?)
    }

    /// Parse `repo.data_repo_url` into `(owner, repo)`.
    ///
    /// Accepted forms (matched in order):
    ///   - `owner/repo`
    ///   - `https://github.com/owner/repo` (optional `.git` suffix)
    ///   - `http://github.com/owner/repo` (treated identically)
    ///   - `git@github.com:owner/repo` (optional `.git` suffix)
    ///
    /// Any other shape returns [`ConfigError::InvalidRepoUrl`]. The
    /// parser is host-agnostic past the `github.com` literal: future
    /// self-hosted forges go through this same helper once the URL
    /// pattern matches.
    pub fn data_repo_owner_repo(&self) -> Result<(String, String), ConfigError> {
        parse_owner_repo(&self.repo.data_repo_url)
    }

    /// Resolve the on-disk path the data-repo clone should live at.
    ///
    /// Priority:
    ///   1. `repo.local_clone_path` if the operator set it explicitly.
    ///   2. `<data_dir>/qx/<owner>-<repo>/` where
    ///      `<data_dir>` is `dirs::data_dir()` — `$XDG_DATA_HOME` on
    ///      Linux, `~/Library/Application Support` on macOS,
    ///      `%APPDATA%` on Windows.
    ///
    /// Returns [`ConfigError::NoDataDir`] only if there is no explicit
    /// override AND the host has no resolvable data dir — extremely
    /// rare in practice (would need `$HOME` unset on every platform).
    pub fn resolve_data_path(&self) -> Result<PathBuf, ConfigError> {
        if let Some(p) = &self.repo.local_clone_path {
            return Ok(p.clone());
        }
        let base = dirs::data_dir().ok_or(ConfigError::NoDataDir)?;
        let (owner, repo) = self.data_repo_owner_repo()?;
        Ok(base.join("qx").join(format!("{owner}-{repo}")))
    }
}

/// Free-function form of [`Config::data_repo_owner_repo`] — exposed so
/// callers that already have a URL string in hand (e.g. test helpers
/// or bootstrap scripts) don't need to build a full `Config`.
pub fn parse_owner_repo(url: &str) -> Result<(String, String), ConfigError> {
    let bad = |reason: &str| ConfigError::InvalidRepoUrl {
        url: url.to_owned(),
        reason: reason.to_owned(),
    };
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(bad("empty"));
    }

    // HTTPS / HTTP form first — must be checked before the bare
    // `owner/repo` heuristic so the URL scheme isn't misread as
    // an owner literal.
    let rest = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .unwrap_or("");
    if !rest.is_empty() {
        // Expect `<host>/<owner>/<repo>[.git][/...]`.
        let mut parts = rest.splitn(2, '/');
        let _host = parts.next().ok_or_else(|| bad("missing host in URL"))?;
        let path = parts.next().ok_or_else(|| bad("missing path in URL"))?;
        return parse_owner_repo_path(path, url);
    }

    // SSH form: `git@host:owner/repo[.git]`.
    if let Some(after_at) = trimmed.strip_prefix("git@") {
        let mut parts = after_at.splitn(2, ':');
        let _host = parts.next().ok_or_else(|| bad("missing host in SSH URL"))?;
        let path = parts.next().ok_or_else(|| bad("missing path in SSH URL"))?;
        return parse_owner_repo_path(path, url);
    }

    // Bare `owner/repo` form.
    parse_owner_repo_path(trimmed, url)
}

/// Helper: split `owner/repo[.git][/...]` after the host has been
/// stripped. Trims a trailing `.git` and rejects empty components.
fn parse_owner_repo_path(path: &str, original: &str) -> Result<(String, String), ConfigError> {
    let bad = |reason: &str| ConfigError::InvalidRepoUrl {
        url: original.to_owned(),
        reason: reason.to_owned(),
    };
    let path = path.trim_start_matches('/');
    let mut parts = path.splitn(3, '/');
    let owner = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad("missing owner"))?;
    let mut repo = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad("missing repo"))?
        .to_owned();
    if let Some(stripped) = repo.strip_suffix(".git") {
        repo = stripped.to_owned();
    }
    if repo.is_empty() {
        return Err(bad("repo name is empty after stripping .git suffix"));
    }
    Ok((owner.to_owned(), repo))
}

/// Transcribe an iterator of `(KEY, VALUE)` env-style pairs into a
/// TOML document. Keys are filtered to those starting with
/// [`ENV_PREFIX`]; remaining suffixes are split on [`ENV_SEPARATOR`]
/// and lower-cased to obtain the dotted TOML path. Values are emitted
/// as TOML literals where the parser can infer the type (booleans,
/// integers, floats), otherwise as quoted strings.
///
/// This is the test-deterministic equivalent of
/// `figment::providers::Env::prefixed(PREFIX).split("__")` — same
/// mapping, different source.
fn env_pairs_to_toml<I, K, V>(env: I) -> String
where
    I: IntoIterator<Item = (K, V)>,
    K: Into<String>,
    V: Into<String>,
{
    use std::collections::BTreeMap;

    // section -> field -> value
    let mut sections: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut top: BTreeMap<String, String> = BTreeMap::new();

    for (k, v) in env {
        let k = k.into();
        let v = v.into();
        let Some(rest) = k.strip_prefix(ENV_PREFIX) else {
            continue;
        };
        // After stripping `PART_REGISTRY_`, we expect at minimum one
        // more underscore (the leading half of `__`) before the
        // section. e.g. `PART_REGISTRY_` + `_STORAGE__ADAPTER`.
        let rest = rest.strip_prefix('_').unwrap_or(rest);
        let parts: Vec<&str> = rest.split(ENV_SEPARATOR).collect();
        match parts.as_slice() {
            [section, field] if !section.is_empty() && !field.is_empty() => {
                sections
                    .entry(section.to_lowercase())
                    .or_default()
                    .insert(field.to_lowercase(), v);
            }
            [field] if !field.is_empty() => {
                top.insert(field.to_lowercase(), v);
            }
            _ => {
                // Unsupported nesting depth; ignore (figment's split
                // would also error on this). Future work: support
                // 3-level nesting when a config field needs it.
            }
        }
    }

    let mut out = String::new();
    for (k, v) in &top {
        out.push_str(&format!("{} = {}\n", k, toml_value_literal(v)));
    }
    for (section, fields) in &sections {
        out.push_str(&format!("\n[{section}]\n"));
        for (k, v) in fields {
            out.push_str(&format!("{} = {}\n", k, toml_value_literal(v)));
        }
    }
    out
}

/// Best-effort conversion of an env-var string to a TOML literal.
/// Numbers and booleans are emitted as bare values; everything else
/// becomes a TOML string with embedded quotes/backslashes escaped.
fn toml_value_literal(v: &str) -> String {
    if v == "true" || v == "false" {
        return v.to_owned();
    }
    if v.parse::<i64>().is_ok() || v.parse::<f64>().is_ok() {
        return v.to_owned();
    }
    let escaped = v.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

// -------------------------------------------------------------------
// Tests
// -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `Config` with an explicit env map — no process-global
    /// env mutation, no unsafe, no thread-races.
    fn cfg(envs: &[(&str, &str)]) -> Config {
        Config::from_layers(None, envs.iter().map(|(k, v)| (*k, *v))).expect("config parses")
    }

    #[test]
    fn from_env_with_no_overrides_returns_defaults() {
        let cfg = cfg(&[]);
        // Spot-check defaults to prove the bundled file loads.
        assert_eq!(cfg.storage.adapter, StorageAdapterChoice::CsvGit);
        assert_eq!(cfg.identity.adapter, IdentityAdapterChoice::GitConfig);
        assert_eq!(cfg.transport.adapter, TransportAdapterChoice::GithubPr);
        assert_eq!(cfg.signing.adapter, SigningAdapterChoice::GitCommit);
        assert_eq!(cfg.label.default_size_mm, 11.0);
        assert_eq!(cfg.observability.log_level, "info");
        assert_eq!(cfg.repo.branch, "main");
        // Per #35: bare `cargo run` targets the sandbox, not the
        // audit-of-record registry. Operators opt into prod via env.
        assert_eq!(
            cfg.repo.data_repo_url,
            "https://github.com/exo-pet/exopet-registry-sandbox"
        );
        // local_clone_path is now unset in defaults; resolution falls
        // through to XDG.
        assert!(cfg.repo.local_clone_path.is_none());
        // Optional fields have no default in the TOML and become None.
        assert!(cfg.storage.sqlite_path.is_none());
        assert!(cfg.identity.github_client_id.is_none());
    }

    // -----------------------------------------------------------
    // parse_owner_repo — accepted URL shapes (#35)
    // -----------------------------------------------------------

    #[test]
    fn parse_owner_repo_accepts_short_form() {
        let (owner, repo) = parse_owner_repo("exo-pet/exopet-registry").unwrap();
        assert_eq!(owner, "exo-pet");
        assert_eq!(repo, "exopet-registry");
    }

    #[test]
    fn parse_owner_repo_accepts_https_url() {
        let (owner, repo) = parse_owner_repo("https://github.com/exo-pet/exopet-registry").unwrap();
        assert_eq!(owner, "exo-pet");
        assert_eq!(repo, "exopet-registry");
    }

    #[test]
    fn parse_owner_repo_accepts_https_url_with_git_suffix() {
        let (owner, repo) =
            parse_owner_repo("https://github.com/exo-pet/exopet-registry.git").unwrap();
        assert_eq!(owner, "exo-pet");
        assert_eq!(repo, "exopet-registry");
    }

    #[test]
    fn parse_owner_repo_accepts_ssh_url() {
        let (owner, repo) = parse_owner_repo("git@github.com:exo-pet/exopet-registry.git").unwrap();
        assert_eq!(owner, "exo-pet");
        assert_eq!(repo, "exopet-registry");
    }

    #[test]
    fn parse_owner_repo_rejects_empty() {
        assert!(matches!(
            parse_owner_repo(""),
            Err(ConfigError::InvalidRepoUrl { .. })
        ));
    }

    #[test]
    fn parse_owner_repo_rejects_missing_repo() {
        assert!(matches!(
            parse_owner_repo("exo-pet"),
            Err(ConfigError::InvalidRepoUrl { .. })
        ));
        assert!(matches!(
            parse_owner_repo("exo-pet/"),
            Err(ConfigError::InvalidRepoUrl { .. })
        ));
    }

    #[test]
    fn parse_owner_repo_rejects_dotgit_only_repo() {
        assert!(matches!(
            parse_owner_repo("https://github.com/exo-pet/.git"),
            Err(ConfigError::InvalidRepoUrl { .. })
        ));
    }

    #[test]
    fn data_repo_owner_repo_reads_through_config() {
        let c = cfg(&[(
            "PART_REGISTRY__REPO__DATA_REPO_URL",
            "https://github.com/exo-pet/exopet-registry",
        )]);
        let (owner, repo) = c.data_repo_owner_repo().unwrap();
        assert_eq!(owner, "exo-pet");
        assert_eq!(repo, "exopet-registry");
    }

    #[test]
    fn resolve_data_path_uses_explicit_override_when_set() {
        let c = cfg(&[("PART_REGISTRY__REPO__LOCAL_CLONE_PATH", "/tmp/foo")]);
        assert_eq!(c.resolve_data_path().unwrap(), PathBuf::from("/tmp/foo"));
    }

    #[test]
    fn resolve_data_path_falls_back_to_xdg_owner_repo_subdir() {
        let c = cfg(&[]);
        let p = c.resolve_data_path().unwrap();
        // We don't pin the absolute prefix (host-dependent), only the
        // tail — the XDG layer is `dirs::data_dir()`'s job to test.
        assert!(
            p.ends_with("qx/exo-pet-exopet-registry-sandbox"),
            "unexpected resolved path: {p:?}"
        );
    }

    #[test]
    fn env_var_overrides_storage_adapter() {
        let c = cfg(&[("PART_REGISTRY__STORAGE__ADAPTER", "sqlite")]);
        assert_eq!(c.storage.adapter, StorageAdapterChoice::Sqlite);
    }

    #[test]
    fn double_underscore_separator_resolves_underscored_field() {
        // The discriminator: a field name with an internal underscore
        // (`default_size_mm`) must not be confused with a nested-key
        // boundary. Double-underscore separator makes this unambiguous.
        let c = cfg(&[("PART_REGISTRY__LABEL__DEFAULT_SIZE_MM", "8.0")]);
        assert_eq!(c.label.default_size_mm, 8.0);
    }

    #[test]
    fn env_var_overrides_observability_log_level() {
        let c = cfg(&[("PART_REGISTRY__OBSERVABILITY__LOG_LEVEL", "debug")]);
        assert_eq!(c.observability.log_level, "debug");
    }

    #[test]
    fn boolean_envs_are_parsed_as_booleans_not_strings() {
        // Round-trips through TOML: "true" becomes a TOML bool, not a
        // string, so the deserialiser into `bool` succeeds.
        let c = cfg(&[("PART_REGISTRY__OBSERVABILITY__AUDIT_CSV", "true")]);
        assert!(c.observability.audit_csv);
    }

    #[test]
    fn invalid_overrides_toml_yields_typed_parse_error() {
        let result = Config::from_layers(
            Some("not = valid = toml"),
            std::iter::empty::<(String, String)>(),
        );
        match result {
            Err(ConfigError::Parse(_)) => (),
            other => panic!("expected Parse error, got {other:?}"),
        }
    }

    #[test]
    fn from_env_with_overrides_layers_correctly() {
        // Override TOML sets sqlite; env var changes it again to duckdb.
        let c = Config::from_layers(
            Some("[storage]\nadapter = \"sqlite\"\n"),
            [("PART_REGISTRY__STORAGE__ADAPTER", "duck_db")]
                .iter()
                .map(|(k, v)| (*k, *v)),
        )
        .expect("layered parse");
        // Env wins per ADR-021 §"Layered precedence".
        assert_eq!(c.storage.adapter, StorageAdapterChoice::DuckDb);
    }

    #[test]
    fn defaults_file_parses_via_from_env_with_empty_override() {
        // Sanity check: the bundled defaults.toml is itself a complete
        // valid Config with no env help.
        let c = Config::from_layers(Some(""), std::iter::empty::<(String, String)>())
            .expect("empty override layer is fine");
        assert_eq!(c.storage.adapter, StorageAdapterChoice::CsvGit);
    }

    // -----------------------------------------------------------
    // env_pairs_to_toml direct tests — locks the underscore-splitting
    // contract so future refactors don't silently regress it.
    // -----------------------------------------------------------

    #[test]
    fn env_to_toml_handles_section_field() {
        let toml = env_pairs_to_toml([("PART_REGISTRY__STORAGE__ADAPTER", "sqlite")]);
        assert!(toml.contains("[storage]"));
        assert!(toml.contains("adapter = \"sqlite\""));
    }

    #[test]
    fn env_to_toml_emits_numeric_literal() {
        let toml = env_pairs_to_toml([("PART_REGISTRY__LABEL__DEFAULT_SIZE_MM", "8.0")]);
        assert!(toml.contains("default_size_mm = 8.0"), "{toml}");
    }

    #[test]
    fn env_to_toml_skips_keys_without_prefix() {
        let toml = env_pairs_to_toml([
            ("PATH", "/usr/bin"),
            ("HOME", "/home/x"),
            ("PART_REGISTRY__STORAGE__ADAPTER", "csv_git"),
        ]);
        assert!(!toml.contains("/usr/bin"));
        assert!(toml.contains("csv_git"));
    }
}
