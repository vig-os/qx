//! ADR-027 §Tier 1 — IdentityProvider conformance for
//! `GithubOauthIdentity`. The shared `port_tests::identity_provider_conformance`
//! body is still a stub (foundation scaffold); this file invokes it so
//! the wiring exists, plus pins the documented `verified_at` contract
//! for the OAuth surface.

use std::collections::BTreeMap;
use std::sync::Mutex;

use qx_domain::IdentitySource;
use qx_identity::IdentityProvider;
use qx_identity_github_oauth::{
    CachedToken, GithubHttp, GithubOauthIdentity, GithubUserResponse, HttpError, MemoryTokenStore,
};
use qx_port_tests::identity_provider_conformance;

#[derive(Default)]
struct FakeHttp {
    user: Mutex<Option<Result<GithubUserResponse, HttpError>>>,
}

impl GithubHttp for FakeHttp {
    fn get_user(&self, _t: &str) -> Result<GithubUserResponse, HttpError> {
        self.user
            .lock()
            .unwrap()
            .take()
            .unwrap_or_else(|| Err(HttpError::Transport("unset".into())))
    }
    fn post_device_code(&self) -> Result<qx_identity_github_oauth::DeviceCodeResponse, HttpError> {
        Err(HttpError::Transport("not used in conformance".into()))
    }
    fn poll_access_token(
        &self,
        _d: &str,
    ) -> Result<qx_identity_github_oauth::AccessTokenResponse, HttpError> {
        Err(HttpError::Transport("not used in conformance".into()))
    }
}

fn now() -> time::OffsetDateTime {
    time::OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap()
}

fn good_token() -> CachedToken {
    CachedToken {
        access_token: "gho_x".into(),
        token_type: "bearer".into(),
        scope: "read:user".into(),
        issued_at: now(),
    }
}

fn good_user() -> GithubUserResponse {
    GithubUserResponse {
        login: "conformance".into(),
        id: 1,
        name: Some("Conformance".into()),
        email: None,
    }
}

#[test]
fn github_oauth_identity_passes_generic_conformance() {
    let http = FakeHttp::default();
    *http.user.lock().unwrap() = Some(Ok(good_user()));
    let id = GithubOauthIdentity::new(
        Box::new(http),
        Box::new(MemoryTokenStore::with_token(good_token())),
    );
    identity_provider_conformance(id);
}

#[test]
fn github_oauth_identity_roundtrip_basic() {
    let http = FakeHttp::default();
    *http.user.lock().unwrap() = Some(Ok(good_user()));
    let id = GithubOauthIdentity::new(
        Box::new(http),
        Box::new(MemoryTokenStore::with_token(good_token())),
    );
    let op = id.current().expect("identity should resolve");
    assert_eq!(op.source, IdentitySource::GitHubOAuth);
    assert_eq!(op.verified_at, Some(now()));
    // Claims must be populated for a verified GitHub identity.
    assert!(op.claims.contains_key("github_login"));
    // Documented capability shape.
    let caps = id.capabilities(&op);
    assert_eq!(
        caps,
        qx_identity::Capabilities::default(),
        "MVP returns Capabilities::default()"
    );
    // Silence unused warning for the BTreeMap import.
    let _: BTreeMap<String, String> = BTreeMap::new();
}
