//! ADR-027 §Tier 1 — IdentityProvider conformance for
//! `GitConfigIdentity`. The shared `port_tests::identity_provider_conformance`
//! body is still a stub (foundation scaffold per the port_tests crate
//! header); this file invokes it so the wiring exists, plus adds an
//! adapter-specific roundtrip assertion to defend the contract today.

use std::fs;
use std::sync::Mutex;

use qx_domain::IdentitySource;
use qx_identity::IdentityProvider;
use qx_identity_git_config::GitConfigIdentity;
use qx_port_tests::identity_provider_conformance;

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvGuard {
    previous: Vec<(String, Option<String>)>,
}

impl EnvGuard {
    fn set(vars: &[(&str, &str)]) -> Self {
        let mut previous = Vec::new();
        for (k, v) in vars {
            previous.push(((*k).into(), std::env::var(k).ok()));
            std::env::set_var(k, v);
        }
        Self { previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (k, v) in &self.previous {
            match v {
                Some(val) => std::env::set_var(k, val),
                None => std::env::remove_var(k),
            }
        }
    }
}

fn with_test_git_config<R>(name: &str, email: &str, f: impl FnOnce() -> R) -> R {
    let _lock = ENV_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let cfg = tmp.path().join("gitconfig");
    fs::write(
        &cfg,
        format!("[user]\n    name = {name}\n    email = {email}\n"),
    )
    .unwrap();
    let _guard = EnvGuard::set(&[
        ("GIT_CONFIG_GLOBAL", cfg.to_str().unwrap()),
        ("GIT_CONFIG_NOSYSTEM", "1"),
        ("HOME", tmp.path().to_str().unwrap()),
        ("XDG_CONFIG_HOME", tmp.path().to_str().unwrap()),
    ]);
    f()
}

#[test]
fn git_config_identity_passes_generic_conformance() {
    with_test_git_config("Conformance", "conformance@example.com", || {
        identity_provider_conformance(GitConfigIdentity::new());
    });
}

#[test]
fn git_config_identity_roundtrip_basic() {
    with_test_git_config("Conformance", "conformance@example.com", || {
        let id = GitConfigIdentity::new();
        let op = id.current().expect("identity should resolve");
        assert_eq!(op.source, IdentitySource::GitConfig);
        assert!(op.verified_at.is_none());
        // Refresh is idempotent when state is unchanged.
        let again = id.refresh().expect("refresh should succeed");
        assert_eq!(op, again);
    });
}
