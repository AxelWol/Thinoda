//! Explicit cloud authentication primitives. Construction and credential reads
//! never connect. Run native async operations on a Tokio runtime off the UI thread.
//!
//! Integration: export this module; add workspace reqwest and native Tokio
//! (`rt`, `time`, `net`, `io-util`) dependencies. Web needs
//! wasm-bindgen-futures and web-sys `History`, `AbortController`, `AbortSignal`.
//! No client secret is accepted.
//! Normal sign-in uses [`effective_oauth_config`], not user-entered client IDs.
//! Register supported public/native or SPA clients at build/deployment time;
//! see `docs/CLOUD-AUTH-SETUP.md`. Box and Google SPA code exchange are gated
//! unsupported because this implementation has no confidential backend/GIS flow.
//! OneDrive currently targets personal accounts (`consumers`), not SharePoint.
//! Dropbox app-folder access and Box permissions are registration settings.
//! Web deployment requires HTTPS, endpoint CORS and CSP allowances for Fetch
//! and the wasm-bindgen JS module. Browser session storage is not XSS-proof.
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
#[cfg(not(target_arch = "wasm32"))]
use reqwest::Client;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

const MAX_BODY: usize = 64 * 1024;
const MAX_CALLBACK: usize = 8192;
const MAX_FLOW: Duration = Duration::from_secs(600);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Errors deliberately omit response bodies, codes, tokens and request URLs.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AuthError {
    #[error("cloud sign-in is unavailable in this build; publisher configuration is required")]
    BuildConfiguration,
    #[error("invalid cloud authentication configuration")]
    Configuration,
    #[error("invalid authentication callback")]
    Callback,
    #[error("provider rejected authentication")]
    Provider,
    #[error("cloud authentication transport failed")]
    Transport,
    #[error("cloud authentication timed out")]
    Timeout,
    #[error("cloud authentication cancelled")]
    Cancelled,
    #[error("secure credential storage unavailable")]
    Storage,
    #[error("operation unsupported by this provider")]
    Unsupported,
}
pub type Result<T> = std::result::Result<T, AuthError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CloudAccountType {
    OneDrive,
    Dropbox,
    Box,
    GoogleDrive,
    WebDav,
}
impl CloudAccountType {
    /// Stable identifiers, intentionally distinct from the existing AI key.
    pub const fn credential_key(self) -> &'static str {
        match self {
            Self::OneDrive => "cloud-onedrive-v1",
            Self::Dropbox => "cloud-dropbox-v1",
            Self::Box => "cloud-box-v1",
            Self::GoogleDrive => "cloud-google-drive-v1",
            Self::WebDav => "cloud-webdav-v1",
        }
    }
}

/// Execution target, supplied by the app, never an ordinary user form field.
#[derive(Clone, Copy, Debug)]
pub enum OAuthTarget<'a> {
    Native,
    Web { deployment_url: &'a str },
}

/// Public build inputs. Empty by default; no credentials or placeholder IDs.
/// Separate clients avoid mixing native and browser platform registrations.
#[derive(Clone, Copy, Debug, Default)]
pub struct PublicOAuthRegistration<'a> {
    pub native_client_id: Option<&'a str>,
    pub spa_client_id: Option<&'a str>,
    /// Exact publisher-registered `http://127.0.0.1:PORT/callback`.
    /// Required for OneDrive/Dropbox; Google Desktop uses an ephemeral port.
    pub native_redirect_uri: Option<&'a str>,
}

/// Injectable public registrations for deterministic tests and publisher builds.
/// Self-hosting overrides are a different build, not saved ordinary-user IDs.
#[derive(Clone, Copy, Debug, Default)]
pub struct PublisherOAuthRegistrations<'a> {
    pub onedrive: PublicOAuthRegistration<'a>,
    pub dropbox: PublicOAuthRegistration<'a>,
    pub box_: PublicOAuthRegistration<'a>,
    pub google_drive: PublicOAuthRegistration<'a>,
}
impl PublisherOAuthRegistrations<'static> {
    /// Read *compile-time* public variables only. Runtime environment/settings
    /// cannot silently replace the publisher registration or enable a protocol.
    pub const fn from_build() -> Self {
        Self {
            onedrive: PublicOAuthRegistration {
                native_client_id: option_env!("THINODA_OAUTH_ONEDRIVE_NATIVE_CLIENT_ID"),
                spa_client_id: option_env!("THINODA_OAUTH_ONEDRIVE_SPA_CLIENT_ID"),
                native_redirect_uri: option_env!("THINODA_OAUTH_ONEDRIVE_NATIVE_REDIRECT_URI"),
            },
            dropbox: PublicOAuthRegistration {
                native_client_id: option_env!("THINODA_OAUTH_DROPBOX_NATIVE_CLIENT_ID"),
                spa_client_id: option_env!("THINODA_OAUTH_DROPBOX_SPA_CLIENT_ID"),
                native_redirect_uri: option_env!("THINODA_OAUTH_DROPBOX_NATIVE_REDIRECT_URI"),
            },
            box_: PublicOAuthRegistration {
                native_client_id: option_env!("THINODA_OAUTH_BOX_NATIVE_CLIENT_ID"),
                spa_client_id: option_env!("THINODA_OAUTH_BOX_SPA_CLIENT_ID"),
                native_redirect_uri: None,
            },
            google_drive: PublicOAuthRegistration {
                native_client_id: option_env!("THINODA_OAUTH_GOOGLE_DRIVE_NATIVE_CLIENT_ID"),
                spa_client_id: option_env!("THINODA_OAUTH_GOOGLE_DRIVE_SPA_CLIENT_ID"),
                native_redirect_uri: None,
            },
        }
    }
}

/// Ready for the existing explicit PKCE flows; not a live registration check.
pub struct EffectiveOAuthConfig {
    pub oauth: OAuthConfig,
    pub web_redirect_uri: Option<String>,
}
impl PublisherOAuthRegistrations<'_> {
    /// Pure and deterministic: no browser launch, socket, storage or network.
    /// Missing/malformed build inputs and unsupported provider/target protocols
    /// return BuildConfiguration. IDs alone do not bypass capability gates.
    pub fn effective(
        &self,
        provider: CloudAccountType,
        target: OAuthTarget<'_>,
    ) -> Result<EffectiveOAuthConfig> {
        if provider == CloudAccountType::Box
            || (provider == CloudAccountType::GoogleDrive
                && matches!(target, OAuthTarget::Web { .. }))
            || provider == CloudAccountType::WebDav
        {
            return Err(AuthError::BuildConfiguration);
        }
        let registration = match provider {
            CloudAccountType::OneDrive => self.onedrive,
            CloudAccountType::Dropbox => self.dropbox,
            CloudAccountType::Box => self.box_,
            CloudAccountType::GoogleDrive => self.google_drive,
            CloudAccountType::WebDav => return Err(AuthError::BuildConfiguration),
        };
        let client_id = match target {
            OAuthTarget::Native => registration.native_client_id,
            OAuthTarget::Web { .. } => registration.spa_client_id,
        }
        .filter(|id| valid_public_client_id(id))
        .ok_or(AuthError::BuildConfiguration)?;
        let mut oauth =
            OAuthConfig::new(provider, client_id).map_err(|_| AuthError::BuildConfiguration)?;
        let web_redirect_uri = match target {
            OAuthTarget::Native => {
                if matches!(
                    provider,
                    CloudAccountType::OneDrive | CloudAccountType::Dropbox
                ) {
                    let raw = registration
                        .native_redirect_uri
                        .ok_or(AuthError::BuildConfiguration)?;
                    oauth.native_redirect = Some(publisher_loopback(raw)?);
                }
                None
            }
            OAuthTarget::Web { deployment_url } => {
                oauth.browser = true;
                let redirect = web_callback_from_deployment(deployment_url)?;
                oauth.web_redirect =
                    Some(Url::parse(&redirect).map_err(|_| AuthError::BuildConfiguration)?);
                Some(redirect)
            }
        };
        Ok(EffectiveOAuthConfig {
            oauth,
            web_redirect_uri,
        })
    }
}

/// Normal UI entry point. Builds without registration remain explicitly disabled.
pub fn effective_oauth_config(
    provider: CloudAccountType,
    target: OAuthTarget<'_>,
) -> Result<EffectiveOAuthConfig> {
    PublisherOAuthRegistrations::from_build().effective(provider, target)
}

fn valid_public_client_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 512
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._".contains(&b))
}
fn publisher_loopback(raw: &str) -> Result<Url> {
    let url = redirect_url(raw).map_err(|_| AuthError::BuildConfiguration)?;
    if url.scheme() != "http"
        || url.host_str() != Some("127.0.0.1")
        || url.path() != "/callback"
        || url.port().is_none_or(|p| p < 1024)
    {
        return Err(AuthError::BuildConfiguration);
    }
    Ok(url)
}

/// Canonical static entrypoint callback: `https://host/subpath/`.
/// `index.html`, query and fragment are removed; origin, port, case and
/// deployment directory are retained. Deep routes/file entrypoints fail closed.
/// The maintainer registers this exact URI once per deployment, not each user.
pub fn web_callback_from_deployment(deployment_url: &str) -> Result<String> {
    if deployment_url.len() > MAX_CALLBACK || deployment_url.chars().any(char::is_control) {
        return Err(AuthError::BuildConfiguration);
    }
    let mut url = Url::parse(deployment_url).map_err(|_| AuthError::BuildConfiguration)?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(AuthError::BuildConfiguration);
    }
    let path = url.path().to_owned();
    if let Some(directory) = path.strip_suffix("index.html") {
        if !directory.ends_with('/') {
            return Err(AuthError::BuildConfiguration);
        }
        url.set_path(directory);
    } else if !path.ends_with('/') {
        return Err(AuthError::BuildConfiguration);
    }
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.to_string())
}

/// Never derives Debug: serialization is only for the secure store.
#[derive(Clone, Serialize, Deserialize)]
pub struct TokenSet {
    pub provider: CloudAccountType,
    pub client_id: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<u64>,
    pub scope: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct AppPassword {
    pub server: String,
    pub username: String,
    pub password: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub enum CloudCredential {
    OAuth(TokenSet),
    WebDav(AppPassword),
}
impl CloudCredential {
    pub fn provider(&self) -> CloudAccountType {
        match self {
            Self::OAuth(tokens) => tokens.provider,
            Self::WebDav(_) => CloudAccountType::WebDav,
        }
    }
}

/// Injectable store; production implementations must not log values.
pub trait CloudCredentialStore {
    fn read(&self, provider: CloudAccountType) -> Result<Option<CloudCredential>>;
    fn write(&self, credential: &CloudCredential) -> Result<()>;
    fn delete(&self, provider: CloudAccountType) -> Result<()>;
}
#[derive(Default)]
pub struct ProviderCredentialStore;

fn decode_credential(raw: &str, provider: CloudAccountType) -> Result<CloudCredential> {
    if raw.len() > MAX_BODY {
        return Err(AuthError::Storage);
    }
    let value: CloudCredential = serde_json::from_str(raw).map_err(|_| AuthError::Storage)?;
    if value.provider() != provider {
        return Err(AuthError::Storage);
    }
    Ok(value)
}

#[cfg(all(
    not(target_arch = "wasm32"),
    any(target_os = "windows", target_os = "macos", target_os = "linux")
))]
impl CloudCredentialStore for ProviderCredentialStore {
    fn read(&self, provider: CloudAccountType) -> Result<Option<CloudCredential>> {
        let entry = keyring::Entry::new("Thinoda", provider.credential_key())
            .map_err(|_| AuthError::Storage)?;
        match entry.get_password() {
            Ok(raw) => decode_credential(&raw, provider).map(Some),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(AuthError::Storage),
        }
    }
    fn write(&self, credential: &CloudCredential) -> Result<()> {
        let raw = serde_json::to_string(credential).map_err(|_| AuthError::Storage)?;
        if raw.len() > MAX_BODY {
            return Err(AuthError::Storage);
        }
        keyring::Entry::new("Thinoda", credential.provider().credential_key())
            .map_err(|_| AuthError::Storage)?
            .set_password(&raw)
            .map_err(|_| AuthError::Storage)
    }
    fn delete(&self, provider: CloudAccountType) -> Result<()> {
        match keyring::Entry::new("Thinoda", provider.credential_key())
            .map_err(|_| AuthError::Storage)?
            .delete_credential()
        {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(AuthError::Storage),
        }
    }
}
#[cfg(all(
    not(target_arch = "wasm32"),
    not(any(target_os = "windows", target_os = "macos", target_os = "linux"))
))]
impl CloudCredentialStore for ProviderCredentialStore {
    fn read(&self, _: CloudAccountType) -> Result<Option<CloudCredential>> {
        Err(AuthError::Storage)
    }
    fn write(&self, _: &CloudCredential) -> Result<()> {
        Err(AuthError::Storage)
    }
    fn delete(&self, _: CloudAccountType) -> Result<()> {
        Err(AuthError::Storage)
    }
}

/// Clone to cancel an in-flight operation. Dropping the future also cancels it.
#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
#[cfg(not(target_arch = "wasm32"))]
async fn sleep(duration: Duration) -> Result<()> {
    tokio::time::sleep(duration).await;
    Ok(())
}
#[cfg(target_arch = "wasm32")]
async fn sleep(duration: Duration) -> Result<()> {
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let scheduled = web_sys::window().is_some_and(|window| {
            window
                .set_timeout_with_callback_and_timeout_and_arguments_0(
                    &resolve,
                    duration.as_millis().min(i32::MAX as u128) as i32,
                )
                .is_ok()
        });
        if !scheduled {
            let _ = reject.call0(&wasm_bindgen::JsValue::NULL);
        }
    });
    wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .map(|_| ())
        .map_err(|_| AuthError::Transport)
}
async fn bounded<T>(
    future: impl Future<Output = Result<T>>,
    timeout: Duration,
    cancel: &Cancellation,
) -> Result<T> {
    if timeout.is_zero() || timeout > MAX_FLOW {
        return Err(AuthError::Configuration);
    }
    let start = web_time::Instant::now();
    let mut work = std::pin::pin!(future);
    loop {
        if cancel.is_cancelled() {
            return Err(AuthError::Cancelled);
        }
        if start.elapsed() >= timeout {
            return Err(AuthError::Timeout);
        }
        let mut tick = std::pin::pin!(sleep(Duration::from_millis(25)));
        let result = std::future::poll_fn(|cx| {
            if let std::task::Poll::Ready(value) = work.as_mut().poll(cx) {
                return std::task::Poll::Ready(Some(value));
            }
            tick.as_mut().poll(cx).map(|result| match result {
                Ok(()) => None,
                Err(error) => Some(Err(error)),
            })
        })
        .await;
        if let Some(value) = result {
            return value;
        }
    }
}
fn now() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn random_secret() -> String {
    // UUID v4 uses the OS CSPRNG (Web Crypto with uuid's `js` feature).
    // Two UUIDs yield 244 random bits and a 43-character RFC 7636 verifier.
    let mut bytes = [0; 32];
    bytes[..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    bytes[16..].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    URL_SAFE_NO_PAD.encode(bytes)
}
pub fn s256_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}
fn constant_time_state(expected: &str, actual: &str) -> bool {
    // Fixed-size digest comparison avoids a length-dependent early return.
    let a = Sha256::digest(expected.as_bytes());
    let b = Sha256::digest(actual.as_bytes());
    let mut difference = 0u8;
    for i in 0..a.len() {
        difference |= std::hint::black_box(a[i] ^ b[i]);
    }
    difference == 0
}
fn secure_url(raw: &str) -> Result<Url> {
    let url = Url::parse(raw).map_err(|_| AuthError::Configuration)?;
    let https = url.scheme() == "https";
    // HTTP exceptions exist only in unit-test builds, never production.
    let mock = cfg!(test) && url.scheme() == "http" && url.host_str() == Some("127.0.0.1");
    if !(https || mock)
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || raw.chars().any(char::is_control)
    {
        return Err(AuthError::Configuration);
    }
    Ok(url)
}
fn redirect_url(raw: &str) -> Result<Url> {
    if raw.chars().any(char::is_control) {
        return Err(AuthError::Configuration);
    }
    let url = Url::parse(raw).map_err(|_| AuthError::Configuration)?;
    if url.scheme() == "http"
        && url.host_str() == Some("127.0.0.1")
        && url.port().is_some()
        && url.path() == "/callback"
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
    {
        return Ok(url);
    }
    let url = secure_url(raw)?;
    if url.query().is_some() {
        return Err(AuthError::Configuration);
    }
    Ok(url)
}

/// Public client configuration; endpoints and scopes cannot be overridden.
#[derive(Clone)]
pub struct OAuthConfig {
    provider: CloudAccountType,
    client_id: String,
    authorization: Url,
    token: Url,
    revoke: Option<Url>,
    native_redirect: Option<Url>,
    web_redirect: Option<Url>,
    browser: bool,
}
impl OAuthConfig {
    /// Low-level explicit configuration for protocol tests/integrations.
    /// Ordinary sign-in must use `effective_oauth_config` instead. Unsupported
    /// public protocols cannot be enabled by passing a public client ID.
    pub fn new(provider: CloudAccountType, client_id: &str) -> Result<Self> {
        if !cfg!(test)
            && (provider == CloudAccountType::Box
                || (cfg!(target_arch = "wasm32") && provider == CloudAccountType::GoogleDrive))
        {
            return Err(AuthError::Unsupported);
        }
        if !valid_public_client_id(client_id) {
            return Err(AuthError::Configuration);
        }
        let (authorization, token, revoke) = match provider {
            CloudAccountType::OneDrive => (
                "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize",
                "https://login.microsoftonline.com/consumers/oauth2/v2.0/token",
                None,
            ),
            CloudAccountType::Dropbox => (
                "https://www.dropbox.com/oauth2/authorize",
                "https://api.dropboxapi.com/oauth2/token",
                Some("https://api.dropboxapi.com/2/auth/token/revoke"),
            ),
            CloudAccountType::Box => (
                "https://account.box.com/api/oauth2/authorize",
                "https://api.box.com/oauth2/token",
                Some("https://api.box.com/oauth2/revoke"),
            ),
            CloudAccountType::GoogleDrive => (
                "https://accounts.google.com/o/oauth2/v2/auth",
                "https://oauth2.googleapis.com/token",
                Some("https://oauth2.googleapis.com/revoke"),
            ),
            CloudAccountType::WebDav => return Err(AuthError::Unsupported),
        };
        Ok(Self {
            provider,
            client_id: client_id.to_owned(),
            authorization: secure_url(authorization)?,
            token: secure_url(token)?,
            revoke: revoke.map(secure_url).transpose()?,
            native_redirect: None,
            web_redirect: None,
            browser: false,
        })
    }
    pub fn provider(&self) -> CloudAccountType {
        self.provider
    }
    pub fn client_id(&self) -> &str {
        &self.client_id
    }
    fn scope(&self) -> &'static str {
        match self.provider {
            CloudAccountType::OneDrive => "Files.ReadWrite.AppFolder offline_access",
            CloudAccountType::Dropbox => {
                "files.metadata.read files.content.read files.content.write"
            }
            CloudAccountType::GoogleDrive => "https://www.googleapis.com/auth/drive.file",
            // Box's permissions are configured on the app registration.
            CloudAccountType::Box | CloudAccountType::WebDav => "",
        }
    }
}

/// One-use PKCE transaction. No Debug implementation; web serialization belongs
/// exclusively in per-tab session storage, never in settings or localStorage.
#[derive(Serialize, Deserialize)]
pub struct PkceFlow {
    provider: CloudAccountType,
    client_id: String,
    redirect: String,
    verifier: String,
    state: String,
    created_at: u64,
}
impl PkceFlow {
    pub fn new(config: &OAuthConfig, redirect: &str) -> Result<Self> {
        let redirect = redirect_url(redirect)?;
        if config
            .native_redirect
            .as_ref()
            .or(config.web_redirect.as_ref())
            .is_some_and(|registered| registered != &redirect)
        {
            return Err(AuthError::Configuration);
        }
        Ok(Self {
            provider: config.provider,
            client_id: config.client_id.clone(),
            redirect: redirect.to_string(),
            verifier: random_secret(),
            state: random_secret(),
            created_at: now(),
        })
    }
    pub fn authorization_url(&self, config: &OAuthConfig) -> Result<Url> {
        self.validate(config)?;
        let mut url = config.authorization.clone();
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("response_type", "code");
            query.append_pair("client_id", &self.client_id);
            query.append_pair("redirect_uri", &self.redirect);
            query.append_pair("state", &self.state);
            query.append_pair("code_challenge", &s256_challenge(&self.verifier));
            query.append_pair("code_challenge_method", "S256");
            if !config.scope().is_empty() {
                query.append_pair("scope", config.scope());
            }
            if config.provider == CloudAccountType::Dropbox {
                query.append_pair(
                    "token_access_type",
                    if config.browser { "online" } else { "offline" },
                );
            }
            if config.provider == CloudAccountType::GoogleDrive {
                query.append_pair("access_type", "offline");
                query.append_pair("prompt", "consent");
            }
        }
        Ok(url)
    }
    fn validate(&self, config: &OAuthConfig) -> Result<()> {
        if self.provider != config.provider
            || self.client_id != config.client_id
            || now() < self.created_at
            || now().saturating_sub(self.created_at) >= MAX_FLOW.as_secs()
            || self.verifier.len() < 43
            || self.verifier.len() > 128
            || !self
                .verifier
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
            || self.state.len() != 43
        {
            return Err(AuthError::Configuration);
        }
        let redirect = redirect_url(&self.redirect)?;
        if config
            .native_redirect
            .as_ref()
            .or(config.web_redirect.as_ref())
            .is_some_and(|registered| registered != &redirect)
        {
            return Err(AuthError::Configuration);
        }
        Ok(())
    }
    /// Consumes the flow even on an invalid/error callback. Retries start anew.
    pub fn consume(self, callback: &str, config: &OAuthConfig) -> Result<AuthorizationGrant> {
        self.validate(config)?;
        if callback.len() > MAX_CALLBACK {
            return Err(AuthError::Callback);
        }
        if callback.chars().any(char::is_control) {
            return Err(AuthError::Callback);
        }
        let url = Url::parse(callback).map_err(|_| AuthError::Callback)?;
        let expected = redirect_url(&self.redirect)?;
        if url.origin() != expected.origin()
            || url.path() != expected.path()
            || url.fragment().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(AuthError::Callback);
        }
        let mut seen = std::collections::HashSet::new();
        let mut state = None;
        let mut code = None;
        let mut error = false;
        for (key, value) in url.query_pairs() {
            if !seen.insert(key.to_string()) {
                return Err(AuthError::Callback);
            }
            match key.as_ref() {
                "state" => state = Some(value.into_owned()),
                "code" => code = Some(value.into_owned()),
                "error" => error = true,
                _ => {}
            }
        }
        if !state
            .as_deref()
            .is_some_and(|s| constant_time_state(&self.state, s))
        {
            return Err(AuthError::Callback);
        }
        if error {
            return Err(AuthError::Provider);
        }
        let code = code.filter(|s| !s.is_empty()).ok_or(AuthError::Callback)?;
        Ok(AuthorizationGrant { flow: self, code })
    }
}
pub struct AuthorizationGrant {
    flow: PkceFlow,
    code: String,
}

/// Bounded HTTP with redirect following disabled, including for Nextcloud.
pub struct AuthClient {
    #[cfg(not(target_arch = "wasm32"))]
    client: Client,
}
impl AuthClient {
    pub fn new() -> Result<Self> {
        #[cfg(not(target_arch = "wasm32"))]
        let builder = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(REQUEST_TIMEOUT);
        Ok(Self {
            #[cfg(not(target_arch = "wasm32"))]
            client: builder.build().map_err(|_| AuthError::Transport)?,
        })
    }
    #[cfg(not(target_arch = "wasm32"))]
    async fn post(
        &self,
        url: &Url,
        fields: &[(&str, &str)],
        bearer: Option<&str>,
    ) -> Result<(u16, Vec<u8>)> {
        let mut request = self.client.post(url.clone()).form(fields);
        if let Some(bearer) = bearer {
            request = request.bearer_auth(bearer);
        }
        let mut response = request.send().await.map_err(|_| AuthError::Transport)?;
        let status = response.status().as_u16();
        if response
            .content_length()
            .is_some_and(|n| n > MAX_BODY as u64)
        {
            return Err(AuthError::Provider);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| AuthError::Transport)? {
            if bytes.len() + chunk.len() > MAX_BODY {
                return Err(AuthError::Provider);
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok((status, bytes))
    }
    #[cfg(target_arch = "wasm32")]
    async fn post(
        &self,
        url: &Url,
        fields: &[(&str, &str)],
        bearer: Option<&str>,
    ) -> Result<(u16, Vec<u8>)> {
        web::post(url, fields, bearer).await
    }
    async fn response_bytes(
        &self,
        url: &Url,
        fields: &[(&str, &str)],
        bearer: Option<&str>,
    ) -> Result<Vec<u8>> {
        let (status, bytes) = self.post(url, fields, bearer).await?;
        if !(200..300).contains(&status) {
            return Err(AuthError::Provider);
        }
        Ok(bytes)
    }
    async fn token_request(
        &self,
        config: &OAuthConfig,
        fields: &[(&str, &str)],
        previous_refresh: Option<String>,
        cancel: &Cancellation,
    ) -> Result<TokenSet> {
        bounded(
            async {
                let bytes = self.response_bytes(&config.token, fields, None).await?;
                let response: TokenResponse =
                    serde_json::from_slice(&bytes).map_err(|_| AuthError::Provider)?;
                if response.access_token.is_empty()
                    || !response.token_type.eq_ignore_ascii_case("bearer")
                    || response.refresh_token.as_deref() == Some("")
                {
                    return Err(AuthError::Provider);
                }
                Ok(TokenSet {
                    provider: config.provider,
                    client_id: config.client_id.clone(),
                    access_token: response.access_token,
                    refresh_token: response.refresh_token.or(previous_refresh),
                    expires_at: response.expires_in.map(|s| now().saturating_add(s)),
                    scope: response.scope,
                })
            },
            REQUEST_TIMEOUT,
            cancel,
        )
        .await
    }
    pub async fn exchange(
        &self,
        config: &OAuthConfig,
        grant: AuthorizationGrant,
        cancel: &Cancellation,
    ) -> Result<TokenSet> {
        grant.flow.validate(config)?;
        self.token_request(
            config,
            &[
                ("grant_type", "authorization_code"),
                ("client_id", &config.client_id),
                ("code", &grant.code),
                ("redirect_uri", &grant.flow.redirect),
                ("code_verifier", &grant.flow.verifier),
            ],
            None,
            cancel,
        )
        .await
    }
    pub async fn refresh(
        &self,
        config: &OAuthConfig,
        tokens: &TokenSet,
        cancel: &Cancellation,
    ) -> Result<TokenSet> {
        check_binding(config, tokens)?;
        let refresh = tokens
            .refresh_token
            .as_deref()
            .ok_or(AuthError::Unsupported)?;
        self.token_request(
            config,
            &[
                ("grant_type", "refresh_token"),
                ("client_id", &config.client_id),
                ("refresh_token", refresh),
            ],
            Some(refresh.to_owned()),
            cancel,
        )
        .await
    }
    /// OneDrive has no public token revocation endpoint: return Unsupported.
    /// Local deletion is separate and must also happen when remote revoke fails.
    pub async fn revoke(
        &self,
        config: &OAuthConfig,
        tokens: &TokenSet,
        cancel: &Cancellation,
    ) -> Result<()> {
        check_binding(config, tokens)?;
        let url = config.revoke.clone().ok_or(AuthError::Unsupported)?;
        let token = tokens
            .refresh_token
            .as_deref()
            .unwrap_or(&tokens.access_token);
        let fields = match config.provider {
            CloudAccountType::Dropbox => vec![],
            CloudAccountType::Box => vec![("token", token), ("client_id", &config.client_id)],
            _ => vec![("token", token)],
        };
        let bearer =
            (config.provider == CloudAccountType::Dropbox).then_some(tokens.access_token.as_str());
        bounded(
            async { self.response_bytes(&url, &fields, bearer).await.map(|_| ()) },
            REQUEST_TIMEOUT,
            cancel,
        )
        .await
    }
}
fn check_binding(config: &OAuthConfig, tokens: &TokenSet) -> Result<()> {
    if tokens.provider != config.provider || tokens.client_id != config.client_id {
        Err(AuthError::Configuration)
    } else {
        Ok(())
    }
}
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    scope: Option<String>,
}

/// Loopback receiver, bound before launching the browser; no public interface.
#[cfg(not(target_arch = "wasm32"))]
pub struct NativeLogin {
    listener: tokio::net::TcpListener,
    flow: PkceFlow,
}
#[cfg(not(target_arch = "wasm32"))]
impl NativeLogin {
    pub async fn bind(config: &OAuthConfig) -> Result<Self> {
        if config.browser {
            return Err(AuthError::Configuration);
        }
        let port = config
            .native_redirect
            .as_ref()
            .and_then(Url::port)
            .unwrap_or(0);
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
            .await
            .map_err(|_| AuthError::Transport)?;
        let port = listener
            .local_addr()
            .map_err(|_| AuthError::Transport)?
            .port();
        let flow = PkceFlow::new(config, &format!("http://127.0.0.1:{port}/callback"))?;
        Ok(Self { listener, flow })
    }
    pub fn authorization_url(&self, config: &OAuthConfig) -> Result<Url> {
        self.flow.authorization_url(config)
    }
    /// Explicit user action only. Existing launcher accepts `&str`.
    pub fn launch(&self, config: &OAuthConfig) -> Result<()> {
        crate::system::open_url(self.authorization_url(config)?.as_str())
            .map_err(|_| AuthError::Transport)
    }
    pub async fn receive(
        self,
        config: &OAuthConfig,
        timeout: Duration,
        cancel: &Cancellation,
    ) -> Result<AuthorizationGrant> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        bounded(async {
            let (mut stream, peer) = self.listener.accept().await.map_err(|_| AuthError::Transport)?;
            if !peer.ip().is_loopback() {
                return Err(AuthError::Callback);
            }
            let mut bytes = Vec::new();
            loop {
                let mut buffer = [0; 512];
                let count = stream.read(&mut buffer).await.map_err(|_| AuthError::Transport)?;
                if count == 0 || bytes.len() + count > MAX_CALLBACK {
                    return Err(AuthError::Callback);
                }
                bytes.extend_from_slice(&buffer[..count]);
                if bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            if !bytes.ends_with(b"\r\n\r\n") {
                return Err(AuthError::Callback);
            }
            let request = std::str::from_utf8(&bytes).map_err(|_| AuthError::Callback)?;
            let first = request.split("\r\n").next().ok_or(AuthError::Callback)?;
            let mut parts = first.split(' ');
            if parts.next() != Some("GET") {
                return Err(AuthError::Callback);
            }
            let target = parts.next().ok_or(AuthError::Callback)?;
            if !target.starts_with("/callback?")
                || parts.next() != Some("HTTP/1.1")
                || parts.next().is_some()
            {
                return Err(AuthError::Callback);
            }
            let redirect = Url::parse(&self.flow.redirect).map_err(|_| AuthError::Callback)?;
            let expected_host = format!("127.0.0.1:{}", redirect.port().ok_or(AuthError::Callback)?);
            let mut host = None;
            let mut seen = std::collections::HashSet::new();
            for line in request.split("\r\n").skip(1).filter(|line| !line.is_empty()) {
                let (name, value) = line.split_once(':').ok_or(AuthError::Callback)?;
                if name.is_empty()
                    || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                    || value.chars().any(|c| c.is_control() && c != '\t')
                    || !seen.insert(name.to_ascii_lowercase())
                {
                    return Err(AuthError::Callback);
                }
                if name.eq_ignore_ascii_case("host") {
                    host = Some(value.trim());
                }
                if name.eq_ignore_ascii_case("transfer-encoding")
                    || (name.eq_ignore_ascii_case("content-length") && value.trim() != "0")
                {
                    return Err(AuthError::Callback);
                }
            }
            if host != Some(expected_host.as_str()) {
                return Err(AuthError::Callback);
            }
            let callback = format!("{}{}", redirect.origin().ascii_serialization(), target);
            let result = self.flow.consume(&callback, config);
            let response = if result.is_ok() {
                b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n".as_slice()
            } else {
                b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n".as_slice()
            };
            // Browser acknowledgement is best-effort; a closed tab must not
            // discard an otherwise valid one-use authorization grant.
            let _ = stream.write_all(response).await;
            result
        }, timeout, cancel).await
    }
}

/// Returned poll token and app password never implement Debug.
pub struct NextcloudLogin {
    server: Url,
    login: Url,
    endpoint: Url,
    token: String,
    created_at: u64,
}
#[derive(Deserialize)]
struct NextcloudStart {
    login: String,
    poll: NextcloudPoll,
}
#[derive(Deserialize)]
struct NextcloudPoll {
    token: String,
    endpoint: String,
}
#[derive(Deserialize)]
struct NextcloudCredentials {
    server: String,
    #[serde(rename = "loginName")]
    login_name: String,
    #[serde(rename = "appPassword")]
    app_password: String,
}
fn same_server(server: &Url, candidate: &str) -> Result<Url> {
    let url = secure_url(candidate)?;
    if url.origin() != server.origin() || url.query().is_some() {
        return Err(AuthError::Configuration);
    }
    Ok(url)
}
impl AuthClient {
    pub async fn nextcloud_start(
        &self,
        server: &str,
        cancel: &Cancellation,
    ) -> Result<NextcloudLogin> {
        let mut server = secure_url(server)?;
        if server.query().is_some() {
            return Err(AuthError::Configuration);
        }
        if !server.path().ends_with('/') {
            server.set_path(&format!("{}/", server.path()));
        }
        let start = server
            .join("index.php/login/v2")
            .map_err(|_| AuthError::Configuration)?;
        bounded(
            async {
                let bytes = self.response_bytes(&start, &[], None).await?;
                let response: NextcloudStart =
                    serde_json::from_slice(&bytes).map_err(|_| AuthError::Provider)?;
                let login = secure_url(&response.login)?;
                if login.origin() != server.origin() || response.poll.token.is_empty() {
                    return Err(AuthError::Configuration);
                }
                let endpoint = same_server(&server, &response.poll.endpoint)?;
                Ok(NextcloudLogin {
                    server,
                    login,
                    endpoint,
                    token: response.poll.token,
                    created_at: now(),
                })
            },
            REQUEST_TIMEOUT,
            cancel,
        )
        .await
    }
    pub async fn nextcloud_poll(
        &self,
        flow: NextcloudLogin,
        interval: Duration,
        timeout: Duration,
        cancel: &Cancellation,
    ) -> Result<AppPassword> {
        if !(Duration::from_secs(1)..=Duration::from_secs(30)).contains(&interval) {
            return Err(AuthError::Configuration);
        }
        bounded(
            async {
                loop {
                    if now().saturating_sub(flow.created_at) >= MAX_FLOW.as_secs() {
                        return Err(AuthError::Timeout);
                    }
                    let (status, bytes) = bounded(
                        self.post(&flow.endpoint, &[("token", &flow.token)], None),
                        REQUEST_TIMEOUT,
                        cancel,
                    )
                    .await?;
                    if status == 404 {
                        sleep(interval).await?;
                        continue;
                    }
                    if !(200..300).contains(&status) {
                        return Err(AuthError::Provider);
                    }
                    let credentials: NextcloudCredentials =
                        serde_json::from_slice(&bytes).map_err(|_| AuthError::Provider)?;
                    let server = same_server(&flow.server, &credentials.server)?;
                    // Preserve the configured deployment path, not just its host.
                    if server.path().trim_end_matches('/')
                        != flow.server.path().trim_end_matches('/')
                        || credentials.login_name.is_empty()
                        || credentials.app_password.is_empty()
                    {
                        return Err(AuthError::Configuration);
                    }
                    return Ok(AppPassword {
                        server: flow.server.to_string(),
                        username: credentials.login_name,
                        password: credentials.app_password,
                    });
                }
            },
            timeout,
            cancel,
        )
        .await
    }
}
impl NextcloudLogin {
    pub fn login_url(&self) -> &Url {
        &self.login
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn launch(&self) -> Result<()> {
        crate::system::open_url(self.login.as_str()).map_err(|_| AuthError::Transport)
    }
}

#[cfg(target_arch = "wasm32")]
pub mod web {
    use super::*;
    use wasm_bindgen::prelude::*;
    // Fetch's default redirects are unsafe for credential-bearing POST bodies.
    // Always abort on redirect, bound streamed bytes, and omit cookies/referrer.
    #[wasm_bindgen(inline_js = r#"
export async function thinodaAuthPost(url, body, bearer, signal) {
    const headers = { "Content-Type": "application/x-www-form-urlencoded" };
    if (bearer) headers.Authorization = "Bearer " + bearer;
    const response = await fetch(url, {
        method: "POST", body, headers, signal, redirect: "error",
        credentials: "omit", mode: "cors", cache: "no-store",
        referrerPolicy: "no-referrer"
    });
    if (Number(response.headers.get("Content-Length")) > 65536)
        throw new Error("response too large");
    if (!response.body) return [response.status, new Uint8Array()];
    const reader = response.body.getReader();
    const chunks = [];
    let size = 0;
    try {
        for (;;) {
            const { done, value } = await reader.read();
            if (done) break;
            size += value.length;
            if (size > 65536) throw new Error("response too large");
            chunks.push(value);
        }
    } finally {
        await reader.cancel();
    }
    const bytes = new Uint8Array(size);
    let offset = 0;
    for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length; }
    return [response.status, bytes];
}
"#)]
    extern "C" {
        #[wasm_bindgen(js_name = thinodaAuthPost)]
        fn auth_post(
            url: &str,
            body: &str,
            bearer: &str,
            signal: &web_sys::AbortSignal,
        ) -> js_sys::Promise;
    }
    struct AbortOnDrop(web_sys::AbortController);
    impl Drop for AbortOnDrop {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    pub(super) async fn post(
        url: &Url,
        fields: &[(&str, &str)],
        bearer: Option<&str>,
    ) -> Result<(u16, Vec<u8>)> {
        let mut form = url.clone();
        form.set_query(None);
        for (key, value) in fields {
            form.query_pairs_mut().append_pair(key, value);
        }
        let controller =
            AbortOnDrop(web_sys::AbortController::new().map_err(|_| AuthError::Transport)?);
        let result = wasm_bindgen_futures::JsFuture::from(auth_post(
            url.as_str(),
            form.query().unwrap_or(""),
            bearer.unwrap_or(""),
            &controller.0.signal(),
        ))
        .await
        .map_err(|_| AuthError::Transport)?;
        let array = js_sys::Array::from(&result);
        let status = array.get(0).as_f64().ok_or(AuthError::Transport)? as u16;
        let bytes = js_sys::Uint8Array::new(&array.get(1)).to_vec();
        if bytes.len() > MAX_BODY {
            return Err(AuthError::Provider);
        }
        Ok((status, bytes))
    }
    fn storage() -> Result<web_sys::Storage> {
        web_sys::window()
            .and_then(|w| w.session_storage().ok().flatten())
            .ok_or(AuthError::Storage)
    }
    fn key(provider: CloudAccountType) -> String {
        format!("Thinoda.{}", provider.credential_key())
    }
    impl CloudCredentialStore for ProviderCredentialStore {
        fn read(&self, provider: CloudAccountType) -> Result<Option<CloudCredential>> {
            storage()?
                .get_item(&key(provider))
                .map_err(|_| AuthError::Storage)?
                .map(|raw| decode_credential(&raw, provider))
                .transpose()
        }
        fn write(&self, credential: &CloudCredential) -> Result<()> {
            let raw = serde_json::to_string(credential).map_err(|_| AuthError::Storage)?;
            if raw.len() > MAX_BODY {
                return Err(AuthError::Storage);
            }
            storage()?
                .set_item(&key(credential.provider()), &raw)
                .map_err(|_| AuthError::Storage)
        }
        fn delete(&self, provider: CloudAccountType) -> Result<()> {
            storage()?
                .remove_item(&key(provider))
                .map_err(|_| AuthError::Storage)
        }
    }
    /// Call from Connect, then navigate the current tab to the returned URL.
    /// Do not use the system launcher's new-tab behaviour for a web PKCE flow.
    pub fn begin(config: &OAuthConfig, redirect: &str) -> Result<Url> {
        let window = web_sys::window().ok_or(AuthError::Configuration)?;
        let current = Url::parse(
            &window
                .location()
                .href()
                .map_err(|_| AuthError::Configuration)?,
        )
        .map_err(|_| AuthError::Configuration)?;
        let target = secure_url(redirect)?;
        if current.origin() != target.origin() {
            return Err(AuthError::Configuration);
        }
        let flow = PkceFlow::new(config, redirect)?;
        let authorization = flow.authorization_url(config)?;
        let raw = serde_json::to_string(&flow).map_err(|_| AuthError::Storage)?;
        storage()?
            .set_item(&format!("{}.pkce", key(config.provider)), &raw)
            .map_err(|_| AuthError::Storage)?;
        Ok(authorization)
    }
    /// Read/remove the flow once, scrub the URL even for failed authentication.
    /// Call before initializing routing/analytics, then explicitly exchange.
    pub fn consume_redirect(config: &OAuthConfig) -> Result<AuthorizationGrant> {
        let window = web_sys::window().ok_or(AuthError::Configuration)?;
        let callback = window.location().href().map_err(|_| AuthError::Callback)?;
        // Consume before validation/cleanup so failed callbacks cannot replay.
        // Still clean the URL when storage is unavailable or has no transaction.
        let raw = (|| {
            let storage = storage()?;
            let key = format!("{}.pkce", key(config.provider));
            let raw = storage.get_item(&key).map_err(|_| AuthError::Storage);
            storage.remove_item(&key).map_err(|_| AuthError::Storage)?;
            raw
        })();
        let mut clean = Url::parse(&callback).map_err(|_| AuthError::Callback)?;
        clean.set_query(None);
        clean.set_fragment(None);
        window
            .history()
            .map_err(|_| AuthError::Callback)?
            .replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(clean.as_str()))
            .map_err(|_| AuthError::Callback)?;
        let raw = raw?.ok_or(AuthError::Callback)?;
        if raw.len() > MAX_CALLBACK {
            return Err(AuthError::Callback);
        }
        let flow: PkceFlow = serde_json::from_str(&raw).map_err(|_| AuthError::Callback)?;
        flow.consume(&callback, config)
    }
    pub fn cancel_pending(provider: CloudAccountType) -> Result<()> {
        storage()?
            .remove_item(&format!("{}.pkce", key(provider)))
            .map_err(|_| AuthError::Storage)
    }
}

#[cfg(test)]
#[macro_use]
#[path = "../tests/cloud_auth.rs"]
mod test_definitions;
#[cfg(test)]
mod tests {
    cloud_auth_tests!();
}
