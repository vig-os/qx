//! `qx-identity-git-config` — first MVP `IdentityProvider`
//! adapter (CLI surface) per ADR-020.
//!
//! Reads operator identity from git config (`user.name`, `user.email`,
//! `user.signingkey`) by shelling out to `git config --get <key>`. The
//! resulting [`Operator`] carries `source: IdentitySource::GitConfig`
//! and `verified_at: None` — git config is an operator-asserted claim,
//! not an IdP-verified attestation (ADR-020 §"MVP adapters").
//!
//! `capabilities()` returns `Capabilities::default()` because the MVP
//! `Authorizer` reads `Operator::claims` directly (ADR-020 §"MVP
//! authorization policy"); unverified identities are blocked from
//! mutating actions by the policy table, not by this struct.
//!
//! ## Native only
//!
//! This adapter shells out via `std::process::Command` and is gated
//! behind `#[cfg(not(target_arch = "wasm32"))]`. The browser FE uses
//! [`crate::identity_github_oauth`](../identity_github_oauth) instead.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use qx_domain::{IdentitySource, KeyId, Operator, OperatorId};
use qx_identity::{Capabilities, IdentityError, IdentityProvider};

/// CLI identity adapter. Lazy — every `current()` / `refresh()` call
/// re-reads git config so a mid-session `git config user.email ...`
/// is picked up without restarting the binary.
#[derive(Default, Debug, Clone)]
pub struct GitConfigIdentity {
    /// Optional override for the git binary (test hook); defaults to
    /// `"git"` resolved against `$PATH`.
    #[allow(dead_code)] // unused on wasm32 where read_key is a stub
    git_binary: Option<String>,
}

impl GitConfigIdentity {
    pub fn new() -> Self {
        Self::default()
    }

    /// Use a non-default git binary. Primarily for tests; production
    /// code uses [`GitConfigIdentity::new`].
    pub fn with_git_binary(git_binary: impl Into<String>) -> Self {
        Self {
            git_binary: Some(git_binary.into()),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn git_bin(&self) -> &str {
        self.git_binary.as_deref().unwrap_or("git")
    }

    /// Read one `git config --get <key>` value. Returns `Ok(None)` when
    /// the key is absent (`git config` exits with status 1 and no
    /// output); returns `Err` for any other failure mode.
    #[cfg(not(target_arch = "wasm32"))]
    fn read_key(&self, key: &str) -> Result<Option<String>, IdentityError> {
        use std::process::Command;

        let output = Command::new(self.git_bin())
            .args(["config", "--get", key])
            .output()
            .map_err(|e| {
                IdentityError::Backend(Box::new(GitConfigError::Spawn {
                    binary: self.git_bin().into(),
                    source: e,
                }))
            })?;

        if output.status.success() {
            let val = String::from_utf8(output.stdout)
                .map_err(|e| IdentityError::Backend(Box::new(GitConfigError::Utf8(e))))?
                .trim()
                .to_owned();
            if val.is_empty() {
                Ok(None)
            } else {
                Ok(Some(val))
            }
        } else if output.status.code() == Some(1) {
            // `git config --get` exits 1 when the key is unset.
            Ok(None)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            Err(IdentityError::Backend(Box::new(
                GitConfigError::ExitStatus {
                    code: output.status.code(),
                    stderr,
                },
            )))
        }
    }

    #[cfg(target_arch = "wasm32")]
    #[allow(clippy::unused_self)]
    fn read_key(&self, _key: &str) -> Result<Option<String>, IdentityError> {
        Err(IdentityError::NoIdentity(
            "GitConfigIdentity is not available on wasm32 \
             (use identity_github_oauth in the browser FE)"
                .into(),
        ))
    }

    fn read_operator(&self) -> Result<Operator, IdentityError> {
        let name = self.read_key("user.name")?.ok_or_else(|| {
            IdentityError::NoIdentity(
                "git config user.name is unset; run \
                 `git config --global user.name 'Your Name'`"
                    .into(),
            )
        })?;
        let email = self.read_key("user.email")?.ok_or_else(|| {
            IdentityError::NoIdentity(
                "git config user.email is unset; run \
                 `git config --global user.email 'you@example.com'`"
                    .into(),
            )
        })?;
        let signing_key = self.read_key("user.signingkey")?;

        Ok(Operator {
            id: OperatorId(format!("git-config:{email}")),
            display_name: name,
            // ADR-020 §"MVP adapters": git config is operator-asserted,
            // not IdP-verified; `verified_at` stays `None` and the MVP
            // Authorizer policy blocks unverified mutations.
            source: IdentitySource::GitConfig,
            verified_at: None,
            claims: BTreeMap::new(),
            pubkey: signing_key.map(KeyId),
        })
    }
}

impl IdentityProvider for GitConfigIdentity {
    fn current(&self) -> Result<Operator, IdentityError> {
        self.read_operator()
    }

    fn refresh(&self) -> Result<Operator, IdentityError> {
        // Stateless — git config may have changed since the last call.
        self.read_operator()
    }

    fn capabilities(&self, _op: &Operator) -> Capabilities {
        // ADR-020 §"Capabilities — reserved for future adapters". The
        // MVP Authorizer reads `Operator::claims` directly; unverified
        // git-config identities are blocked from mutations there.
        Capabilities::default()
    }
}

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)] // gated off on wasm32; the variants are emitted by run_key on native only
enum GitConfigError {
    #[error("failed to spawn git binary {binary:?}")]
    Spawn {
        binary: String,
        #[source]
        source: std::io::Error,
    },
    #[error("git output was not UTF-8")]
    Utf8(#[source] std::string::FromUtf8Error),
    #[error("git config exited with status {code:?}: {stderr}")]
    ExitStatus { code: Option<i32>, stderr: String },
}

// -------------------------------------------------------------------
// Tests
// -------------------------------------------------------------------
//
// Strategy: rather than mocking the git binary, set up a real git
// config file in a tempdir and point `GIT_CONFIG_GLOBAL` at it. This
// exercises the actual `git config --get` code path end-to-end.

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use std::env;
    use std::fs;
    use std::sync::Mutex;

    // git config reads env vars at process scope; serialise the tests
    // that mutate `GIT_CONFIG_GLOBAL` etc. so they don't trample each
    // other on a `cargo test` parallel runner.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        previous: Vec<(String, Option<String>)>,
    }

    impl EnvGuard {
        fn set(vars: &[(&str, &str)]) -> Self {
            let mut previous = Vec::new();
            for (k, v) in vars {
                previous.push(((*k).into(), env::var(k).ok()));
                // SAFETY: tests serialise via ENV_LOCK; set_var is
                // unsafe in edition 2024 but allowed here because we
                // run single-threaded under the guard.
                env::set_var(k, v);
            }
            Self { previous }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (k, v) in &self.previous {
                match v {
                    Some(val) => env::set_var(k, val),
                    None => env::remove_var(k),
                }
            }
        }
    }

    fn write_gitconfig(dir: &std::path::Path, body: &str) -> std::path::PathBuf {
        let path = dir.join("gitconfig");
        fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn current_returns_operator_with_canonical_id_and_no_verification() {
        let _lock = ENV_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let cfg = write_gitconfig(
            tmp.path(),
            "[user]\n    name = Ada Lovelace\n    email = ada@example.com\n",
        );
        let _guard = EnvGuard::set(&[
            ("GIT_CONFIG_GLOBAL", cfg.to_str().unwrap()),
            ("GIT_CONFIG_NOSYSTEM", "1"),
            // Keep HOME/XDG away from the user's real config.
            ("HOME", tmp.path().to_str().unwrap()),
            ("XDG_CONFIG_HOME", tmp.path().to_str().unwrap()),
        ]);

        let id = GitConfigIdentity::new();
        let op = id.current().expect("identity should resolve");

        assert_eq!(op.id, OperatorId("git-config:ada@example.com".into()));
        assert_eq!(op.display_name, "Ada Lovelace");
        assert_eq!(op.source, IdentitySource::GitConfig);
        assert!(
            op.verified_at.is_none(),
            "git config is operator-asserted; verified_at must be None"
        );
        assert!(op.pubkey.is_none());
        assert!(op.claims.is_empty());
    }

    #[test]
    fn current_picks_up_signing_key_into_pubkey() {
        let _lock = ENV_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let cfg = write_gitconfig(
            tmp.path(),
            "[user]\n    name = Grace\n    email = g@example.com\n    \
             signingkey = ABCD1234EF567890\n",
        );
        let _guard = EnvGuard::set(&[
            ("GIT_CONFIG_GLOBAL", cfg.to_str().unwrap()),
            ("GIT_CONFIG_NOSYSTEM", "1"),
            ("HOME", tmp.path().to_str().unwrap()),
            ("XDG_CONFIG_HOME", tmp.path().to_str().unwrap()),
        ]);

        let op = GitConfigIdentity::new().current().unwrap();
        assert_eq!(op.pubkey, Some(KeyId("ABCD1234EF567890".into())));
    }

    #[test]
    fn refresh_reflects_mid_session_changes() {
        let _lock = ENV_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let cfg = write_gitconfig(
            tmp.path(),
            "[user]\n    name = First\n    email = first@example.com\n",
        );
        let _guard = EnvGuard::set(&[
            ("GIT_CONFIG_GLOBAL", cfg.to_str().unwrap()),
            ("GIT_CONFIG_NOSYSTEM", "1"),
            ("HOME", tmp.path().to_str().unwrap()),
            ("XDG_CONFIG_HOME", tmp.path().to_str().unwrap()),
        ]);
        let id = GitConfigIdentity::new();
        let before = id.current().unwrap();
        assert_eq!(before.display_name, "First");

        // Rewrite the config file in-place.
        fs::write(
            &cfg,
            "[user]\n    name = Second\n    email = second@example.com\n",
        )
        .unwrap();
        let after = id.refresh().unwrap();
        assert_eq!(after.display_name, "Second");
        assert_eq!(after.id, OperatorId("git-config:second@example.com".into()));
    }

    #[test]
    fn missing_user_name_yields_no_identity_error() {
        let _lock = ENV_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let cfg = write_gitconfig(tmp.path(), "[user]\n    email = only-email@example.com\n");
        let _guard = EnvGuard::set(&[
            ("GIT_CONFIG_GLOBAL", cfg.to_str().unwrap()),
            ("GIT_CONFIG_NOSYSTEM", "1"),
            ("HOME", tmp.path().to_str().unwrap()),
            ("XDG_CONFIG_HOME", tmp.path().to_str().unwrap()),
        ]);

        let err = GitConfigIdentity::new().current().unwrap_err();
        match err {
            IdentityError::NoIdentity(msg) => {
                assert!(msg.contains("user.name"), "got: {msg}");
            }
            other => panic!("expected NoIdentity, got {other:?}"),
        }
    }

    #[test]
    fn missing_user_email_yields_no_identity_error() {
        let _lock = ENV_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let cfg = write_gitconfig(tmp.path(), "[user]\n    name = NameOnly\n");
        let _guard = EnvGuard::set(&[
            ("GIT_CONFIG_GLOBAL", cfg.to_str().unwrap()),
            ("GIT_CONFIG_NOSYSTEM", "1"),
            ("HOME", tmp.path().to_str().unwrap()),
            ("XDG_CONFIG_HOME", tmp.path().to_str().unwrap()),
        ]);

        let err = GitConfigIdentity::new().current().unwrap_err();
        match err {
            IdentityError::NoIdentity(msg) => {
                assert!(msg.contains("user.email"), "got: {msg}");
            }
            other => panic!("expected NoIdentity, got {other:?}"),
        }
    }

    #[test]
    fn capabilities_is_default_for_unverified_identity() {
        // ADR-020 §"Capabilities" — MVP adapters return the default;
        // the MVP Authorizer reads claims directly.
        let op = Operator {
            id: OperatorId("git-config:x@y".into()),
            display_name: "X".into(),
            source: IdentitySource::GitConfig,
            verified_at: None,
            claims: BTreeMap::new(),
            pubkey: None,
        };
        let caps = GitConfigIdentity::new().capabilities(&op);
        assert_eq!(caps, Capabilities::default());
    }

    #[test]
    fn missing_git_binary_surfaces_backend_error() {
        let id = GitConfigIdentity::with_git_binary("definitely-not-a-real-git-binary-xyz");
        let err = id.current().unwrap_err();
        match err {
            IdentityError::Backend(_) => {}
            other => panic!("expected Backend, got {other:?}"),
        }
    }
}
