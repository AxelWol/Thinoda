// Expanded inside cloud_auth so private mock endpoint overrides are available
// only to unit tests. Cargo also discovers this file as an empty integration
// target; it intentionally has no dependency on a not-yet-exported module.
#[allow(unused_macros)]
macro_rules! cloud_auth_tests {
() => {
use super::*;

fn config() -> OAuthConfig {
    OAuthConfig::new(CloudAccountType::Dropbox, "test-public-client").unwrap()
}
fn callback(flow: &PkceFlow, tail: &str) -> String {
    format!("{}?state={}&{tail}", flow.redirect, flow.state)
}
#[test]
fn rfc7636_vector_and_secure_randomness() {
    assert_eq!(
        s256_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
    let a = PkceFlow::new(&config(), "http://127.0.0.1:8765/callback").unwrap();
    let b = PkceFlow::new(&config(), "http://127.0.0.1:8765/callback").unwrap();
    assert_eq!(a.verifier.len(), 43);
    assert_eq!(a.state.len(), 43);
    assert_ne!(a.state, a.verifier);
    assert_ne!(a.verifier, b.verifier);
    assert!(constant_time_state(&a.state, &a.state));
    assert!(!constant_time_state(&a.state, ""));
}
#[test]
fn callback_rejects_duplicates_state_errors_and_wrong_path() {
    for tail in [
        "code=ok&state=duplicate", "code=a&code=b", "error=denied",
        "code=", "code=ok&error=denied",
    ] {
        let flow = PkceFlow::new(&config(), "http://127.0.0.1:8765/callback").unwrap();
        let url = callback(&flow, tail);
        assert!(flow.consume(&url, &config()).is_err());
    }
    for url in [
        "http://127.0.0.1:8765/callback?state=wrong&code=ok",
        "http://127.0.0.1:8765/callback/extra?state=wrong&code=ok",
        "http://127.0.0.1:8766/callback?state=wrong&code=ok",
    ] {
        let flow = PkceFlow::new(&config(), "http://127.0.0.1:8765/callback").unwrap();
        assert!(matches!(flow.consume(url, &config()), Err(AuthError::Callback)));
    }
    let flow = PkceFlow::new(&config(), "http://127.0.0.1:8765/callback").unwrap();
    let url = callback(&flow, "code=accepted");
    assert!(flow.consume(&url, &config()).is_ok());
}
#[test]
fn endpoint_and_binding_validation() {
    assert!(secure_url("http://example.invalid").is_err());
    assert!(secure_url("https://user:password@example.invalid").is_err());
    assert!(secure_url("https://example.invalid/#fragment").is_err());
    let server = secure_url("https://example.invalid/cloud/").unwrap();
    assert!(same_server(&server, "https://other.invalid/cloud").is_err());
    let flow = PkceFlow::new(&config(), "https://example.invalid/callback").unwrap();
    assert!(flow.authorization_url(&OAuthConfig::new(CloudAccountType::Box, "test-public-client").unwrap()).is_err());
    let mut expired = PkceFlow::new(&config(), "https://example.invalid/callback").unwrap();
    expired.created_at = now().saturating_sub(601);
    assert!(expired.authorization_url(&config()).is_err());
}
#[test]
fn authorization_urls_have_pkce_and_minimal_provider_permissions() {
    for provider in [CloudAccountType::OneDrive, CloudAccountType::Dropbox, CloudAccountType::Box, CloudAccountType::GoogleDrive] {
        let config = OAuthConfig::new(provider, "mock-public-client").unwrap();
        let flow = PkceFlow::new(&config, "https://example.invalid/callback").unwrap();
        let url = flow.authorization_url(&config).unwrap();
        assert_eq!(url.scheme(), "https");
        let query: std::collections::HashMap<_, _> = url.query_pairs().collect();
        assert_eq!(query.get("code_challenge_method").unwrap(), "S256");
        assert_eq!(query.get("code_challenge").unwrap(), &s256_challenge(&flow.verifier));
        assert_eq!(query.get("response_type").unwrap(), "code");
        assert!(!query.contains_key("client_secret"));
        match provider {
            CloudAccountType::OneDrive => assert_eq!(query.get("scope").unwrap(), "Files.ReadWrite.AppFolder offline_access"),
            CloudAccountType::Dropbox => assert_eq!(query.get("scope").unwrap(), "files.metadata.read files.content.read files.content.write"),
            CloudAccountType::Box => assert!(!query.contains_key("scope")),
            CloudAccountType::GoogleDrive => assert_eq!(query.get("scope").unwrap(), "https://www.googleapis.com/auth/drive.file"),
            _ => unreachable!(),
        }
    }
}
#[derive(Default)]
struct MemoryStore(std::sync::Mutex<std::collections::HashMap<&'static str, String>>);
impl CloudCredentialStore for MemoryStore {
    fn read(&self, provider: CloudAccountType) -> Result<Option<CloudCredential>> {
        self.0.lock().unwrap().get(provider.credential_key())
            .map(|s| decode_credential(s, provider)).transpose()
    }
    fn write(&self, credential: &CloudCredential) -> Result<()> {
        self.0.lock().unwrap().insert(credential.provider().credential_key(),
            serde_json::to_string(credential).unwrap());
        Ok(())
    }
    fn delete(&self, provider: CloudAccountType) -> Result<()> {
        self.0.lock().unwrap().remove(provider.credential_key());
        Ok(())
    }
}
#[test]
fn credential_stores_are_separate() {
    let store = MemoryStore::default();
    for provider in [CloudAccountType::OneDrive, CloudAccountType::Dropbox, CloudAccountType::Box, CloudAccountType::GoogleDrive] {
        store.write(&CloudCredential::OAuth(TokenSet {
            provider, client_id: "mock".into(), access_token: provider.credential_key().into(),
            refresh_token: None, expires_at: None, scope: None,
        })).unwrap();
    }
    store.write(&CloudCredential::WebDav(AppPassword {
        server: "https://example.invalid/".into(), username: "mock".into(), password: "fixture".into(),
    })).unwrap();
    store.delete(CloudAccountType::Dropbox).unwrap();
    assert!(store.read(CloudAccountType::Dropbox).unwrap().is_none());
    assert!(store.read(CloudAccountType::OneDrive).unwrap().is_some());
    assert!(store.read(CloudAccountType::WebDav).unwrap().is_some());
    let wrong = serde_json::to_string(&store.read(CloudAccountType::OneDrive).unwrap().unwrap()).unwrap();
    assert!(decode_credential(&wrong, CloudAccountType::Box).is_err());
}

#[cfg(not(target_arch = "wasm32"))]
fn run(work: impl Future<Output = ()>) {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(work);
}
#[cfg(not(target_arch = "wasm32"))]
async fn mock(response: String) -> (Url, tokio::task::JoinHandle<String>) {
    let (url, task) = mock_sequence(|_| vec![response]).await;
    (url, tokio::spawn(async move { task.await.unwrap().remove(0) }))
}
#[cfg(not(target_arch = "wasm32"))]
async fn mock_sequence(
    responses: impl FnOnce(&Url) -> Vec<String>,
) -> (Url, tokio::task::JoinHandle<Vec<String>>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let url = Url::parse(&format!("http://{}/token", listener.local_addr().unwrap())).unwrap();
    let responses = responses(&url);
    let task = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(3), async move {
            let mut requests = Vec::new();
            for response in responses {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            loop {
                let mut chunk = [0; 1024];
                let n = stream.read(&mut chunk).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&chunk[..n]);
                if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&bytes[..end]);
                    let length = header.lines().find_map(|line| {
                        line.to_ascii_lowercase().strip_prefix("content-length:").map(|s| s.trim().parse::<usize>().unwrap())
                    }).unwrap_or(0);
                    if bytes.len() >= end + 4 + length { break; }
                }
                assert!(bytes.len() < MAX_BODY);
            }
            stream.write_all(response.as_bytes()).await.unwrap();
            requests.push(String::from_utf8(bytes).unwrap());
            }
            requests
        }).await.unwrap()
    });
    (url, task)
}
#[cfg(not(target_arch = "wasm32"))]
fn json_response(body: &str) -> String {
    format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
}
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn token_exchange_refresh_and_revoke_are_explicit() {
    run(async {
        let (url, server) = mock(json_response(r#"{"access_token":"fixture-access","token_type":"Bearer","refresh_token":"fixture-refresh","expires_in":3600}"#)).await;
        let mut config = config();
        config.token = url;
        let flow = PkceFlow::new(&config, "http://127.0.0.1:8765/callback").unwrap();
        let verifier = flow.verifier.clone();
        let url = callback(&flow, "code=fixture-code");
        let grant = flow.consume(&url, &config);
        let tokens = AuthClient::new().unwrap().exchange(&config, grant.unwrap(), &Cancellation::default()).await.unwrap();
        assert_eq!(tokens.provider, CloudAccountType::Dropbox);
        assert!(tokens.expires_at.unwrap() >= now() + 3590);
        let request = server.await.unwrap();
        assert!(request.contains("code_verifier="));
        assert!(request.contains(&verifier));
        assert!(!request.contains("client_secret"));
        let store = MemoryStore::default();
        store.write(&CloudCredential::OAuth(tokens.clone())).unwrap();
        let (url, server) = mock(json_response(r#"{"access_token":"rotated","token_type":"bearer","expires_in":120}"#)).await;
        config.token = url;
        let refreshed = AuthClient::new().unwrap().refresh(&config, &tokens, &Cancellation::default()).await.unwrap();
        assert_eq!(refreshed.refresh_token.as_deref(), Some("fixture-refresh"));
        assert!(server.await.unwrap().contains("grant_type=refresh_token"));
        let (url, server) = mock(json_response("{}")).await;
        config.revoke = Some(url);
        AuthClient::new().unwrap().revoke(&config, &refreshed, &Cancellation::default()).await.unwrap();
        assert!(server.await.unwrap().contains("Bearer rotated"));
    });
}
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn native_listener_timeout_cancel_and_success() {
    run(async {
        let config = config();
        let login = NativeLogin::bind(&config).await.unwrap();
        assert!(matches!(login.receive(&config, Duration::from_millis(50), &Cancellation::default()).await, Err(AuthError::Timeout)));
        let cancel = Cancellation::default();
        cancel.cancel();
        let login = NativeLogin::bind(&config).await.unwrap();
        assert!(matches!(login.receive(&config, Duration::from_secs(1), &cancel).await, Err(AuthError::Cancelled)));
        let login = NativeLogin::bind(&config).await.unwrap();
        let addr = login.listener.local_addr().unwrap();
        let request = format!("GET /callback?state={}&code=fixture HTTP/1.1\r\nHost: {addr}\r\n\r\n", login.flow.state);
        let sender = tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            stream.write_all(request.as_bytes()).await.unwrap();
        });
        assert!(login.receive(&config, Duration::from_secs(1), &Cancellation::default()).await.is_ok());
        sender.await.unwrap();
    });
}
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn nextcloud_initiate_pending_and_credentials_stay_on_configured_server() {
    run(async {
        let (url, server) = mock_sequence(|url| {
            let origin = url.origin().ascii_serialization();
            vec![
                json_response(&serde_json::json!({
                    "login": format!("{origin}/login?flow=fixture"),
                    "poll": {"token": "fixture-poll", "endpoint": format!("{origin}/poll")}
                }).to_string()),
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
                json_response(&serde_json::json!({
                    "server": origin, "loginName": "fixture-user", "appPassword": "fixture-password"
                }).to_string()),
            ]
        }).await;
        let configured = url.origin().ascii_serialization();
        let client = AuthClient::new().unwrap();
        let cancel = Cancellation::default();
        let flow = client.nextcloud_start(&configured, &cancel).await.unwrap();
        assert_eq!(flow.login_url().origin(), url.origin());
        let credentials = client.nextcloud_poll(flow, Duration::from_secs(1), Duration::from_secs(3), &cancel).await.unwrap();
        assert_eq!(credentials.server.trim_end_matches('/'), configured);
        assert_eq!(credentials.password, "fixture-password");
        let requests = server.await.unwrap();
        assert!(requests[0].starts_with("POST /index.php/login/v2 "));
        assert!(requests[1].contains("token=fixture-poll"));
        assert!(requests[2].contains("token=fixture-poll"));
    });
}
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn nextcloud_rejects_foreign_login_poll_and_credential_endpoints() {
    run(async {
        for foreign_login in [true, false] {
            let (url, server) = mock_sequence(|url| {
                let origin = url.origin().ascii_serialization();
                vec![json_response(&serde_json::json!({
                    "login": if foreign_login { "https://other.invalid/login".into() } else { format!("{origin}/login") },
                    "poll": {"token": "fixture", "endpoint": if foreign_login { format!("{origin}/poll") } else { "https://other.invalid/poll".into() }}
                }).to_string())]
            }).await;
            let result = AuthClient::new().unwrap().nextcloud_start(&url.origin().ascii_serialization(), &Cancellation::default()).await;
            assert!(matches!(result, Err(AuthError::Configuration)));
            server.await.unwrap();
        }
        let (url, server) = mock(json_response(r#"{"server":"https://other.invalid","loginName":"fixture","appPassword":"fixture"}"#)).await;
        let flow = NextcloudLogin {
            server: url.join("/").unwrap(), login: url.clone(), endpoint: url, token: "fixture".into(), created_at: now(),
        };
        assert!(matches!(AuthClient::new().unwrap().nextcloud_poll(flow, Duration::from_secs(1), Duration::from_secs(2), &Cancellation::default()).await, Err(AuthError::Configuration)));
        server.await.unwrap();
    });
}
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn http_redirects_and_oversize_tokens_are_rejected() {
    run(async {
        for response in [
            "HTTP/1.1 307 Temporary Redirect\r\nLocation: https://other.invalid/steal\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
            "HTTP/1.1 200 OK\r\nContent-Length: 999999\r\nConnection: close\r\n\r\n".to_owned(),
            json_response(r#"{"access_token":"fixture","token_type":"mac"}"#),
        ] {
            let (url, server) = mock(response).await;
            let mut config = config();
            config.token = url;
            let flow = PkceFlow::new(&config, "http://127.0.0.1:8765/callback").unwrap();
            let callback = callback(&flow, "code=fixture");
            let grant = flow.consume(&callback, &config).unwrap();
            assert!(matches!(AuthClient::new().unwrap().exchange(&config, grant, &Cancellation::default()).await, Err(AuthError::Provider)));
            server.await.unwrap();
        }
    });
}
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn native_receiver_rejects_invalid_request_and_bounds_slow_clients() {
    run(async {
        for target in ["/callback/other", "/callback?state=invalid&code=fixture"] {
            let config = config();
            let login = NativeLogin::bind(&config).await.unwrap();
            let addr = login.listener.local_addr().unwrap();
            let request = format!("GET {target} HTTP/1.1\r\nHost: {addr}\r\n\r\n");
            let sender = tokio::spawn(async move {
                use tokio::io::AsyncWriteExt;
                let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
                stream.write_all(request.as_bytes()).await.unwrap();
            });
            assert!(matches!(login.receive(&config, Duration::from_secs(1), &Cancellation::default()).await, Err(AuthError::Callback)));
            sender.await.unwrap();
        }
        let config = config();
        let login = NativeLogin::bind(&config).await.unwrap();
        let _slow = tokio::net::TcpStream::connect(login.listener.local_addr().unwrap()).await.unwrap();
        assert!(matches!(login.receive(&config, Duration::from_millis(50), &Cancellation::default()).await, Err(AuthError::Timeout)));
    });
}
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn cancelled_exchange_and_foreign_refresh_do_not_send_requests() {
    run(async {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let mut config = config();
        config.token = Url::parse(&format!("http://{}/token", listener.local_addr().unwrap())).unwrap();
        let flow = PkceFlow::new(&config, "http://127.0.0.1:8765/callback").unwrap();
        let callback = callback(&flow, "code=fixture");
        let grant = flow.consume(&callback, &config).unwrap();
        let cancel = Cancellation::default();
        cancel.cancel();
        let client = AuthClient::new().unwrap();
        assert!(matches!(client.exchange(&config, grant, &cancel).await, Err(AuthError::Cancelled)));
        let tokens = TokenSet {
            provider: CloudAccountType::Box, client_id: config.client_id.clone(),
            access_token: "fixture".into(), refresh_token: Some("fixture".into()), expires_at: None, scope: None,
        };
        assert!(matches!(client.refresh(&config, &tokens, &Cancellation::default()).await, Err(AuthError::Configuration)));
        assert!(tokio::time::timeout(Duration::from_millis(50), listener.accept()).await.is_err());
    });
}
};
}
