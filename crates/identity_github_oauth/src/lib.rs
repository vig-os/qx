//! `qx-identity-github-oauth` — FE/CLI `IdentityProvider`
//! adapter per ADR-020.
//!
//! GitHub OAuth device flow yields a verified GitHub identity:
//! `source: GitHubOAuth`, `verified_at: <now>` (the token introspection
//! against `GET /user` succeeded, so we know the token is still good).
//!
//! ## Two surfaces, one trait
//!
//! - **Native (CLI / future server)**: blocking HTTP via `reqwest`,
//!   token cache at `~/.config/qx/github-token.json`.
//! - **wasm32 (browser FE)**: HTTP via the browser fetch API, token
//!   cache in `localStorage`. The FE layer is wired in `crates/wasm/`;
//!   this crate exposes only the type seams (the `TokenStore` trait
//!   and the cached-token shape) on wasm32 so the trait still
//!   compiles for the target.
//!
//! ## Device flow vs. introspection
//!
//! Per the task spec:
//!
//! - `current()` does **not** trigger interactive auth. It reads the
//!   cached token and confirms it with `GET /user`. If there is no
//!   cached token, returns `IdentityError::NoIdentity("no session")`.
//!   If the token is rejected by GitHub (401), returns
//!   `IdentityError::VerificationFailed("session expired")`.
//! - `refresh()` re-introspects the cached token (same shape as
//!   `current()` — GitHub OAuth tokens don't auto-refresh; the only
//!   "refresh" available is to re-run the device flow).
//! - `start_device_flow()` is the interactive entry point. Binaries
//!   call it explicitly when `current()` returns `NoIdentity`.
//!
//! ## Capabilities
//!
//! Per ADR-020 §"Capabilities — reserved for future adapters", MVP
//! returns `Capabilities::default()`. Claim-to-capability mapping
//! (org membership, team membership) is per-adapter policy and is
//! not defined for MVP.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};

use qx_domain::{IdentitySource, Operator, OperatorId, Timestamp};
use qx_identity::{Capabilities, IdentityError, IdentityProvider};

// -------------------------------------------------------------------
// Token store
// -------------------------------------------------------------------

/// Persisted shape of a GitHub OAuth token. `issued_at` becomes
/// `Operator::verified_at` so the audit trail records when GitHub
/// last attested the identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedToken {
    pub access_token: String,
    pub token_type: String,
    pub scope: String,
    /// RFC-3339 timestamp at which the device flow completed (or the
    /// token was first observed).
    #[serde(with = "time::serde::rfc3339")]
    pub issued_at: Timestamp,
}

/// Pluggable storage for the cached OAuth token. Native default is
/// [`FileTokenStore`]; tests use [`MemoryTokenStore`]; the browser FE
/// will wire its own `localStorage`-backed implementation.
pub trait TokenStore: Send + Sync {
    fn load(&self) -> Result<Option<CachedToken>, TokenStoreError>;
    fn save(&self, token: &CachedToken) -> Result<(), TokenStoreError>;
    fn clear(&self) -> Result<(), TokenStoreError>;
}

#[derive(Debug, thiserror::Error)]
pub enum TokenStoreError {
    #[error("token store I/O error: {0}")]
    Io(String),
    #[error("token store malformed payload: {0}")]
    Malformed(String),
}

/// In-memory token cache. Test-only — production code uses
/// [`FileTokenStore`] (native) or a browser-side implementation
/// (wasm32).
#[derive(Default)]
pub struct MemoryTokenStore {
    inner: Mutex<Option<CachedToken>>,
}

impl MemoryTokenStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_token(token: CachedToken) -> Self {
        Self {
            inner: Mutex::new(Some(token)),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Option<CachedToken>> {
        self.inner.lock().expect("MemoryTokenStore mutex poisoned")
    }
}

impl TokenStore for MemoryTokenStore {
    fn load(&self) -> Result<Option<CachedToken>, TokenStoreError> {
        Ok(self.lock().clone())
    }

    fn save(&self, token: &CachedToken) -> Result<(), TokenStoreError> {
        *self.lock() = Some(token.clone());
        Ok(())
    }

    fn clear(&self) -> Result<(), TokenStoreError> {
        *self.lock() = None;
        Ok(())
    }
}

/// File-backed token cache at
/// `$XDG_CONFIG_HOME/qx/github-token.json` (or `~/.config/`
/// on macOS/Linux defaults). Native only; gated off on wasm32.
#[cfg(not(target_arch = "wasm32"))]
pub struct FileTokenStore {
    path: std::path::PathBuf,
}

#[cfg(not(target_arch = "wasm32"))]
impl FileTokenStore {
    /// Use the platform default location.
    pub fn default_path() -> Option<std::path::PathBuf> {
        dirs::config_dir().map(|d| d.join("qx").join("github-token.json"))
    }

    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn at_default() -> Result<Self, TokenStoreError> {
        let path = Self::default_path()
            .ok_or_else(|| TokenStoreError::Io("no platform config directory available".into()))?;
        Ok(Self { path })
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl TokenStore for FileTokenStore {
    fn load(&self) -> Result<Option<CachedToken>, TokenStoreError> {
        match std::fs::read(&self.path) {
            Ok(bytes) => {
                let token: CachedToken = serde_json::from_slice(&bytes)
                    .map_err(|e| TokenStoreError::Malformed(e.to_string()))?;
                Ok(Some(token))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(TokenStoreError::Io(e.to_string())),
        }
    }

    fn save(&self, token: &CachedToken) -> Result<(), TokenStoreError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| TokenStoreError::Io(e.to_string()))?;
        }
        let bytes = serde_json::to_vec_pretty(token)
            .map_err(|e| TokenStoreError::Malformed(e.to_string()))?;
        std::fs::write(&self.path, bytes).map_err(|e| TokenStoreError::Io(e.to_string()))
    }

    fn clear(&self) -> Result<(), TokenStoreError> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(TokenStoreError::Io(e.to_string())),
        }
    }
}

// -------------------------------------------------------------------
// HTTP transport abstraction
// -------------------------------------------------------------------
//
// Direct `reqwest` calls would couple the adapter to a specific HTTP
// implementation and make wasm32 builds painful. Lift the surface this
// adapter needs (GET /user with a bearer token, POST device endpoints)
// into a trait. Native default uses `reqwest::blocking`; wasm32 will
// wire a browser-fetch implementation later.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubUserResponse {
    pub login: String,
    pub id: u64,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AccessTokenResponse {
    Success {
        access_token: String,
        token_type: String,
        #[serde(default)]
        scope: String,
    },
    Pending {
        error: String,
        #[serde(default)]
        error_description: String,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("HTTP transport error: {0}")]
    Transport(String),
    #[error("HTTP status {status}: {body}")]
    Status { status: u16, body: String },
    #[error("HTTP body deserialize error: {0}")]
    Deserialize(String),
}

pub trait GithubHttp: Send + Sync {
    /// `GET <user_endpoint>` with `Authorization: Bearer <token>`.
    /// On 200 returns the parsed user; on 401 returns
    /// `HttpError::Status { status: 401, .. }`.
    fn get_user(&self, token: &str) -> Result<GithubUserResponse, HttpError>;

    /// `POST <device_code_endpoint>` for the device flow.
    fn post_device_code(&self) -> Result<DeviceCodeResponse, HttpError>;

    /// `POST <access_token_endpoint>` polling for the user's
    /// confirmation. Caller polls until success or timeout.
    fn poll_access_token(&self, device_code: &str) -> Result<AccessTokenResponse, HttpError>;
}

// -- Native reqwest implementation --------------------------------------

#[cfg(not(target_arch = "wasm32"))]
pub struct ReqwestGithubHttp {
    pub client_id: String,
    pub user_endpoint: String,
    pub device_code_endpoint: String,
    pub access_token_endpoint: String,
    client: reqwest::blocking::Client,
}

#[cfg(not(target_arch = "wasm32"))]
impl ReqwestGithubHttp {
    pub fn new(client_id: impl Into<String>) -> Result<Self, HttpError> {
        Self::with_endpoints(
            client_id,
            "https://api.github.com/user",
            "https://github.com/login/device/code",
            "https://github.com/login/oauth/access_token",
        )
    }

    pub fn with_endpoints(
        client_id: impl Into<String>,
        user: impl Into<String>,
        device_code: impl Into<String>,
        access_token: impl Into<String>,
    ) -> Result<Self, HttpError> {
        let client = reqwest::blocking::Client::builder()
            .user_agent(concat!("qx/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| HttpError::Transport(e.to_string()))?;
        Ok(Self {
            client_id: client_id.into(),
            user_endpoint: user.into(),
            device_code_endpoint: device_code.into(),
            access_token_endpoint: access_token.into(),
            client,
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl GithubHttp for ReqwestGithubHttp {
    fn get_user(&self, token: &str) -> Result<GithubUserResponse, HttpError> {
        let resp = self
            .client
            .get(&self.user_endpoint)
            .bearer_auth(token)
            .header("Accept", "application/vnd.github+json")
            .send()
            .map_err(|e| HttpError::Transport(e.to_string()))?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(HttpError::Status { status, body });
        }
        resp.json::<GithubUserResponse>()
            .map_err(|e| HttpError::Deserialize(e.to_string()))
    }

    fn post_device_code(&self) -> Result<DeviceCodeResponse, HttpError> {
        let resp = self
            .client
            .post(&self.device_code_endpoint)
            .header("Accept", "application/json")
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("scope", "read:user"),
            ])
            .send()
            .map_err(|e| HttpError::Transport(e.to_string()))?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(HttpError::Status { status, body });
        }
        resp.json::<DeviceCodeResponse>()
            .map_err(|e| HttpError::Deserialize(e.to_string()))
    }

    fn poll_access_token(&self, device_code: &str) -> Result<AccessTokenResponse, HttpError> {
        let resp = self
            .client
            .post(&self.access_token_endpoint)
            .header("Accept", "application/json")
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("device_code", device_code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .map_err(|e| HttpError::Transport(e.to_string()))?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(HttpError::Status { status, body });
        }
        resp.json::<AccessTokenResponse>()
            .map_err(|e| HttpError::Deserialize(e.to_string()))
    }
}

// -------------------------------------------------------------------
// Adapter
// -------------------------------------------------------------------

pub struct GithubOauthIdentity {
    http: Box<dyn GithubHttp>,
    store: Box<dyn TokenStore>,
}

impl GithubOauthIdentity {
    /// Construct with a pre-wired HTTP client + token store. Production
    /// code on native passes `ReqwestGithubHttp` + `FileTokenStore`; the
    /// FE wires its own.
    pub fn new(http: Box<dyn GithubHttp>, store: Box<dyn TokenStore>) -> Self {
        Self { http, store }
    }

    /// Inspect the cached token; `None` if no session is open.
    pub fn cached_token(&self) -> Result<Option<CachedToken>, IdentityError> {
        self.store
            .load()
            .map_err(|e| IdentityError::Backend(Box::new(e)))
    }

    /// Persist a token (used by the device-flow entry point).
    pub fn save_token(&self, token: &CachedToken) -> Result<(), IdentityError> {
        self.store
            .save(token)
            .map_err(|e| IdentityError::Backend(Box::new(e)))
    }

    /// Clear any cached token (logout).
    pub fn logout(&self) -> Result<(), IdentityError> {
        self.store
            .clear()
            .map_err(|e| IdentityError::Backend(Box::new(e)))
    }

    fn read_operator(&self) -> Result<Operator, IdentityError> {
        let token = self
            .store
            .load()
            .map_err(|e| IdentityError::Backend(Box::new(e)))?
            .ok_or_else(|| {
                IdentityError::NoIdentity("no GitHub session; run `start_device_flow` first".into())
            })?;

        let user = match self.http.get_user(&token.access_token) {
            Ok(u) => u,
            Err(HttpError::Status { status: 401, .. })
            | Err(HttpError::Status { status: 403, .. }) => {
                return Err(IdentityError::VerificationFailed(
                    "GitHub rejected the cached token (session expired); \
                     re-run the device flow"
                        .into(),
                ));
            }
            Err(e) => {
                return Err(IdentityError::Backend(Box::new(e)));
            }
        };

        let mut claims = BTreeMap::new();
        claims.insert("github_login".into(), user.login.clone());
        claims.insert("github_id".into(), user.id.to_string());
        if let Some(email) = &user.email {
            claims.insert("github_email".into(), email.clone());
        }
        claims.insert("github_scope".into(), token.scope.clone());

        Ok(Operator {
            id: OperatorId(format!("github:{}", user.login)),
            display_name: user.name.unwrap_or_else(|| user.login.clone()),
            // ADR-020 §"MVP adapters": GitHub OAuth is IdP-verified;
            // `verified_at` is the moment GitHub confirmed the token.
            source: IdentitySource::GitHubOAuth,
            verified_at: Some(token.issued_at),
            claims,
            pubkey: None,
        })
    }

    /// Start the GitHub device flow. Returns the device-code payload
    /// the caller shows the user (URL + user code) plus a handle the
    /// caller polls with [`Self::poll_device_flow`].
    ///
    /// Per the task spec: this is a separate explicit entry point so
    /// `current()` never blocks on interactive auth.
    pub fn start_device_flow(&self) -> Result<DeviceCodeResponse, IdentityError> {
        self.http
            .post_device_code()
            .map_err(|e| IdentityError::Backend(Box::new(e)))
    }

    /// One poll iteration. Caller loops with the `interval` from the
    /// device-code response. Returns `Ok(Some(token))` when the user
    /// has confirmed, `Ok(None)` while still pending.
    pub fn poll_device_flow(
        &self,
        device_code: &str,
        now: Timestamp,
    ) -> Result<Option<CachedToken>, IdentityError> {
        let resp = self
            .http
            .poll_access_token(device_code)
            .map_err(|e| IdentityError::Backend(Box::new(e)))?;
        match resp {
            AccessTokenResponse::Success {
                access_token,
                token_type,
                scope,
            } => {
                let token = CachedToken {
                    access_token,
                    token_type,
                    scope,
                    issued_at: now,
                };
                self.save_token(&token)?;
                Ok(Some(token))
            }
            AccessTokenResponse::Pending {
                error,
                error_description,
            } => {
                // `authorization_pending` and `slow_down` are the only
                // "still polling" states per the OAuth device-flow spec.
                if error == "authorization_pending" || error == "slow_down" {
                    Ok(None)
                } else {
                    Err(IdentityError::VerificationFailed(format!(
                        "device flow failed: {error}: {error_description}"
                    )))
                }
            }
        }
    }
}

impl IdentityProvider for GithubOauthIdentity {
    fn current(&self) -> Result<Operator, IdentityError> {
        self.read_operator()
    }

    fn refresh(&self) -> Result<Operator, IdentityError> {
        // GitHub OAuth tokens don't refresh; "refresh" is just a fresh
        // introspection. If the token is bad the caller learns now.
        self.read_operator()
    }

    fn capabilities(&self, _op: &Operator) -> Capabilities {
        // ADR-020 §"Capabilities": MVP returns the default; the MVP
        // Authorizer reads `Operator::claims` directly. Future
        // adapters may consult `claims["github_team"]` etc. to
        // populate this struct.
        Capabilities::default()
    }
}

// -------------------------------------------------------------------
// Tests (native only — wasm32 doesn't run `cargo test`)
// -------------------------------------------------------------------

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // ----- HTTP test double ------------------------------------------------

    #[derive(Default)]
    struct FakeHttp {
        user_response: Mutex<Option<Result<GithubUserResponse, HttpError>>>,
        device_response: Mutex<Option<Result<DeviceCodeResponse, HttpError>>>,
        access_response: Mutex<Option<Result<AccessTokenResponse, HttpError>>>,
    }

    impl FakeHttp {
        fn set_user(&self, r: Result<GithubUserResponse, HttpError>) {
            *self.user_response.lock().unwrap() = Some(r);
        }
        fn set_device(&self, r: Result<DeviceCodeResponse, HttpError>) {
            *self.device_response.lock().unwrap() = Some(r);
        }
        fn set_access(&self, r: Result<AccessTokenResponse, HttpError>) {
            *self.access_response.lock().unwrap() = Some(r);
        }
    }

    impl GithubHttp for FakeHttp {
        fn get_user(&self, _token: &str) -> Result<GithubUserResponse, HttpError> {
            match self.user_response.lock().unwrap().take() {
                Some(Ok(u)) => Ok(u),
                Some(Err(e)) => Err(e),
                None => Err(HttpError::Transport("FakeHttp::get_user unset".into())),
            }
        }
        fn post_device_code(&self) -> Result<DeviceCodeResponse, HttpError> {
            match self.device_response.lock().unwrap().take() {
                Some(Ok(d)) => Ok(d),
                Some(Err(e)) => Err(e),
                None => Err(HttpError::Transport(
                    "FakeHttp::post_device_code unset".into(),
                )),
            }
        }
        fn poll_access_token(&self, _device_code: &str) -> Result<AccessTokenResponse, HttpError> {
            match self.access_response.lock().unwrap().take() {
                Some(Ok(a)) => Ok(a),
                Some(Err(e)) => Err(e),
                None => Err(HttpError::Transport(
                    "FakeHttp::poll_access_token unset".into(),
                )),
            }
        }
    }

    fn now() -> Timestamp {
        time::OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap()
    }

    fn good_token() -> CachedToken {
        CachedToken {
            access_token: "gho_abc123".into(),
            token_type: "bearer".into(),
            scope: "read:user".into(),
            issued_at: now(),
        }
    }

    fn good_user() -> GithubUserResponse {
        GithubUserResponse {
            login: "ada".into(),
            id: 42,
            name: Some("Ada Lovelace".into()),
            email: Some("ada@example.com".into()),
        }
    }

    // ----- TokenStore (memory + file) -------------------------------------

    #[test]
    fn memory_token_store_roundtrips() {
        let store = MemoryTokenStore::new();
        assert!(store.load().unwrap().is_none());
        store.save(&good_token()).unwrap();
        assert_eq!(store.load().unwrap().as_ref(), Some(&good_token()));
        store.clear().unwrap();
        assert!(store.load().unwrap().is_none());
    }

    #[test]
    fn file_token_store_roundtrips() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nested").join("token.json");
        let store = FileTokenStore::new(&path);
        assert!(store.load().unwrap().is_none());
        store.save(&good_token()).unwrap();
        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded, good_token());
        store.clear().unwrap();
        assert!(store.load().unwrap().is_none());
        // Idempotent clear.
        store.clear().unwrap();
    }

    #[test]
    fn file_token_store_handles_malformed_payload() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("token.json");
        std::fs::write(&path, b"not valid json {").unwrap();
        let store = FileTokenStore::new(&path);
        match store.load() {
            Err(TokenStoreError::Malformed(_)) => {}
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    // ----- IdentityProvider -----------------------------------------------

    #[test]
    fn current_with_no_session_returns_no_identity() {
        let id = GithubOauthIdentity::new(
            Box::new(FakeHttp::default()),
            Box::new(MemoryTokenStore::new()),
        );
        match id.current() {
            Err(IdentityError::NoIdentity(msg)) => {
                assert!(msg.contains("no GitHub session"), "got: {msg}");
            }
            other => panic!("expected NoIdentity, got {other:?}"),
        }
    }

    #[test]
    fn current_with_valid_token_returns_verified_operator() {
        let http = FakeHttp::default();
        http.set_user(Ok(good_user()));
        let id = GithubOauthIdentity::new(
            Box::new(http),
            Box::new(MemoryTokenStore::with_token(good_token())),
        );
        let op = id.current().expect("identity should resolve");
        assert_eq!(op.id, OperatorId("github:ada".into()));
        assert_eq!(op.display_name, "Ada Lovelace");
        assert_eq!(op.source, IdentitySource::GitHubOAuth);
        assert_eq!(op.verified_at, Some(now()));
        assert_eq!(op.claims.get("github_login"), Some(&"ada".to_string()));
        assert_eq!(op.claims.get("github_id"), Some(&"42".to_string()));
        assert_eq!(
            op.claims.get("github_email"),
            Some(&"ada@example.com".to_string())
        );
    }

    #[test]
    fn current_with_401_returns_session_expired() {
        let http = FakeHttp::default();
        http.set_user(Err(HttpError::Status {
            status: 401,
            body: "Bad credentials".into(),
        }));
        let id = GithubOauthIdentity::new(
            Box::new(http),
            Box::new(MemoryTokenStore::with_token(good_token())),
        );
        match id.current() {
            Err(IdentityError::VerificationFailed(msg)) => {
                assert!(msg.contains("session expired"), "got: {msg}");
            }
            other => panic!("expected VerificationFailed, got {other:?}"),
        }
    }

    #[test]
    fn current_with_transport_error_returns_backend() {
        let http = FakeHttp::default();
        http.set_user(Err(HttpError::Transport("connection refused".into())));
        let id = GithubOauthIdentity::new(
            Box::new(http),
            Box::new(MemoryTokenStore::with_token(good_token())),
        );
        match id.current() {
            Err(IdentityError::Backend(_)) => {}
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    #[test]
    fn refresh_re_introspects_against_cached_token() {
        let http = FakeHttp::default();
        http.set_user(Ok(GithubUserResponse {
            login: "ada".into(),
            id: 42,
            name: Some("Ada".into()),
            email: None,
        }));
        let id = GithubOauthIdentity::new(
            Box::new(http),
            Box::new(MemoryTokenStore::with_token(good_token())),
        );
        let op = id.refresh().unwrap();
        assert_eq!(op.id, OperatorId("github:ada".into()));
        assert!(!op.claims.contains_key("github_email"));
    }

    #[test]
    fn capabilities_is_default_for_verified_identity() {
        let id = GithubOauthIdentity::new(
            Box::new(FakeHttp::default()),
            Box::new(MemoryTokenStore::new()),
        );
        let op = Operator {
            id: OperatorId("github:ada".into()),
            display_name: "Ada".into(),
            source: IdentitySource::GitHubOAuth,
            verified_at: Some(now()),
            claims: BTreeMap::new(),
            pubkey: None,
        };
        assert_eq!(id.capabilities(&op), Capabilities::default());
    }

    // ----- Device flow -----------------------------------------------------

    #[test]
    fn start_device_flow_returns_user_code() {
        let http = FakeHttp::default();
        http.set_device(Ok(DeviceCodeResponse {
            device_code: "dev123".into(),
            user_code: "ABCD-1234".into(),
            verification_uri: "https://github.com/login/device".into(),
            expires_in: 900,
            interval: 5,
        }));
        let id = GithubOauthIdentity::new(Box::new(http), Box::new(MemoryTokenStore::new()));
        let resp = id.start_device_flow().unwrap();
        assert_eq!(resp.user_code, "ABCD-1234");
    }

    #[test]
    fn poll_device_flow_pending_returns_none() {
        let http = FakeHttp::default();
        http.set_access(Ok(AccessTokenResponse::Pending {
            error: "authorization_pending".into(),
            error_description: "user has not yet entered the code".into(),
        }));
        let id = GithubOauthIdentity::new(Box::new(http), Box::new(MemoryTokenStore::new()));
        assert!(id.poll_device_flow("dev123", now()).unwrap().is_none());
    }

    #[test]
    fn poll_device_flow_success_persists_token() {
        let http = FakeHttp::default();
        http.set_access(Ok(AccessTokenResponse::Success {
            access_token: "gho_new".into(),
            token_type: "bearer".into(),
            scope: "read:user".into(),
        }));
        let store = Box::new(MemoryTokenStore::new());
        // Sneak a peek at the store via a second handle… can't, because
        // Box<dyn>. Instead drive `current()` afterwards.
        let id = GithubOauthIdentity::new(Box::new(http), store);
        let token = id.poll_device_flow("dev123", now()).unwrap().unwrap();
        assert_eq!(token.access_token, "gho_new");
        // Cached token now lives in the store; verify via cached_token().
        let cached = id.cached_token().unwrap().unwrap();
        assert_eq!(cached.access_token, "gho_new");
    }

    #[test]
    fn poll_device_flow_terminal_error_surfaces() {
        let http = FakeHttp::default();
        http.set_access(Ok(AccessTokenResponse::Pending {
            error: "access_denied".into(),
            error_description: "user cancelled".into(),
        }));
        let id = GithubOauthIdentity::new(Box::new(http), Box::new(MemoryTokenStore::new()));
        match id.poll_device_flow("dev123", now()) {
            Err(IdentityError::VerificationFailed(msg)) => {
                assert!(msg.contains("access_denied"), "got: {msg}");
            }
            other => panic!("expected VerificationFailed, got {other:?}"),
        }
    }

    #[test]
    fn logout_clears_token() {
        let store = MemoryTokenStore::with_token(good_token());
        let id = GithubOauthIdentity::new(Box::new(FakeHttp::default()), Box::new(store));
        assert!(id.cached_token().unwrap().is_some());
        id.logout().unwrap();
        assert!(id.cached_token().unwrap().is_none());
    }

    // ----- Reqwest impl smoke (mockito-driven, native-only) ---------------

    #[test]
    fn reqwest_get_user_against_mock_server() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/user")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"login":"ada","id":42,"name":"Ada Lovelace","email":"ada@example.com"}"#)
            .create();
        let http = ReqwestGithubHttp::with_endpoints(
            "client-id",
            format!("{}/user", server.url()),
            format!("{}/device", server.url()),
            format!("{}/token", server.url()),
        )
        .unwrap();
        let user = http.get_user("gho_abc").unwrap();
        assert_eq!(user.login, "ada");
        mock.assert();
    }

    #[test]
    fn reqwest_get_user_401_surfaces_status_error() {
        let mut server = mockito::Server::new();
        let _mock = server
            .mock("GET", "/user")
            .with_status(401)
            .with_body("Bad credentials")
            .create();
        let http = ReqwestGithubHttp::with_endpoints(
            "client-id",
            format!("{}/user", server.url()),
            format!("{}/device", server.url()),
            format!("{}/token", server.url()),
        )
        .unwrap();
        match http.get_user("bad") {
            Err(HttpError::Status { status: 401, .. }) => {}
            other => panic!("expected 401 Status, got {other:?}"),
        }
    }
}
