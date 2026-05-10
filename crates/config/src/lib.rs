//! `part-registry-config` — 12-factor configuration loader per ADR-021.
//!
//! Single read site for every deploy-varying value. Domain crates
//! never call `std::env::var`; ADR-027 §Tier 4 drift-detection
//! enforces this with a workspace grep.
//!
//! Foundation scaffold — `Config::from_env()` is wired through
//! `figment` but the deploy-file path resolution and the WASM-
//! specific `Config::from_runtime(json)` constructor land in
//! ADR-017 step 3.

#![forbid(unsafe_code)]

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config parse error: {0}")]
    Parse(String),
}

impl From<figment::Error> for ConfigError {
    fn from(value: figment::Error) -> Self {
        ConfigError::Parse(value.to_string())
    }
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoConfig {
    pub data_repo_url: String,
    pub code_repo_url: String,
    pub local_clone_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// `csv_git` | `sqlite` | `duckdb` | `dolt` | `file_per_entry`
    pub adapter: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityConfig {
    /// `git_config` | `github_oauth` | `env_user` | `oidc_generic`
    /// | `mtls_cert` | `sigstore_keyless`
    pub adapter: String,
    pub verified_at_window_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    /// `github_pr` | `local_branch` | `webhook` | `filesystem` | `dolt`
    pub adapter: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigningConfig {
    /// `git_commit` | `sigstore` | `none`
    pub adapter: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelDefaults {
    pub default_size_mm: f64,
    pub font_family: String,
    pub labels_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    pub log_level: String,
    pub audit_log_path: PathBuf,
}

const DEFAULTS_TOML: &str = include_str!("../defaults.toml");

impl Config {
    /// Parse defaults + env per ADR-021 §"Config crate shape". Layered
    /// precedence: built-in defaults < env vars (deploy-file layer
    /// will be inserted between them once the path resolution lands).
    pub fn from_env() -> Result<Self, ConfigError> {
        use figment::{
            providers::{Env, Format, Toml},
            Figment,
        };

        let figment = Figment::new()
            .merge(Toml::string(DEFAULTS_TOML))
            .merge(Env::prefixed("PART_REGISTRY_").split("_"));
        Ok(figment.extract()?)
    }
}
