//! Explicitly configured remote storage. Constructors never perform network I/O.
//!
//! See `docs/remote-storage-protocols.md` for wire contracts and integration hooks.
//! Credentials and preauthenticated URLs are deliberately not `Debug`.

#[path = "remote_storage/box_api.rs"]
mod box_api;
#[cfg(target_arch = "wasm32")]
#[path = "remote_storage/browser.rs"]
mod browser;
#[path = "remote_storage/drive.rs"]
mod drive;
#[path = "remote_storage/dropbox.rs"]
mod dropbox;
#[path = "remote_storage/graph.rs"]
mod graph;
#[path = "remote_storage/webdav.rs"]
mod webdav;

pub use box_api::{BoxConfig, BoxProvider, BoxShareAccess};
pub use drive::{GoogleDriveConfig, GoogleDriveProvider};
pub use dropbox::{DropboxConfig, DropboxProvider};
pub use graph::{GraphConfig, GraphProvider, GraphShareScope};
pub use webdav::{WebDavConfig, WebDavProvider};

use crate::storage::{
    Availability, CancelToken, Capabilities, Entry, ExpectedRevision, ReadResult, Revision,
    StorageError, StorageFuture, StorageId, StorageProvider,
};
use reqwest::{
    Method, Url,
    header::{AUTHORIZATION, CONTENT_TYPE, ETAG, HeaderMap, HeaderValue, IF_MATCH, IF_NONE_MATCH},
};
use serde_json::Value;
#[cfg(not(target_arch = "wasm32"))]
use std::future::Future;

type Result<T> = std::result::Result<T, StorageError>;
const METADATA_LIMIT: usize = 8 * 1024 * 1024;
const CONTENT_LIMIT: usize = 256 * 1024 * 1024;
const PAGE_LIMIT: usize = 100;

/// In-memory credential supplied by the account layer; never serialized or logged.
#[derive(Clone)]
pub enum Authentication {
    Bearer(String),
    Basic { username: String, password: String },
}

impl Authentication {
    fn header(&self) -> Result<HeaderValue> {
        use base64::Engine;
        let value = match self {
            Self::Bearer(token) if !token.is_empty() => format!("Bearer {token}"),
            Self::Basic { username, password } if !username.contains(':') => format!(
                "Basic {}",
                base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"))
            ),
            _ => return Err(remote("invalid authentication")),
        };
        let mut header =
            HeaderValue::from_str(&value).map_err(|_| remote("invalid authentication"))?;
        header.set_sensitive(true);
        Ok(header)
    }
}

struct Http {
    #[cfg(not(target_arch = "wasm32"))]
    client: reqwest::Client,
    authorization: HeaderValue,
}

struct Response {
    status: u16,
    headers: HeaderMap,
    bytes: Vec<u8>,
}

impl Response {
    fn revision(&self) -> Option<Revision> {
        self.headers
            .get(ETAG)
            .and_then(|v| v.to_str().ok())
            .map(|v| Revision(v.into()))
    }

    fn json(&self) -> Result<Value> {
        serde_json::from_slice(&self.bytes).map_err(|_| remote("invalid JSON response"))
    }

    fn success(self) -> Result<Self> {
        if self.status == 409 || self.status == 412 {
            return Err(StorageError::Conflict {
                actual: self.revision(),
            });
        }
        if !(200..300).contains(&self.status) {
            // Never expose provider bodies, request URLs, or reqwest error text:
            // those can contain documents, account identifiers, tokens or signed URLs.
            return Err(remote(&format!("HTTP {}", self.status)));
        }
        Ok(self)
    }
}

impl Http {
    fn new(auth: Authentication) -> Result<Self> {
        Ok(Self {
            authorization: auth.header()?,
            #[cfg(not(target_arch = "wasm32"))]
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(std::time::Duration::from_secs(15))
                .timeout(std::time::Duration::from_secs(120))
                // Do not inherit ambient proxy settings for explicitly configured accounts.
                .no_proxy()
                .build()
                .map_err(|_| remote("HTTP client initialization failed"))?,
        })
    }

    async fn send(
        &self,
        method: Method,
        url: Url,
        mut headers: HeaderMap,
        body: Vec<u8>,
        cancel: &CancelToken,
        limit: usize,
    ) -> Result<Response> {
        cancel.check()?;
        validate_url(&url)?;
        // Callers must validate the destination before opting into account authentication.
        if headers.remove("x-thinoda-auth").is_some() {
            headers.insert(AUTHORIZATION, self.authorization.clone());
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let request = self.client.request(method, url).headers(headers).body(body);
            let mut response = cancellable(cancel, async {
                request
                    .send()
                    .await
                    .map_err(|_| remote("network request failed"))
            })
            .await?;
            let status = response.status().as_u16();
            let headers = response.headers().clone();
            let mut bytes = Vec::new();
            // Error bodies are intentionally bounded to ZERO bytes. Status + revision
            // suffice for diagnosis without accidentally surfacing remote content.
            if (200..300).contains(&status) {
                while let Some(chunk) = cancellable(cancel, async {
                    response
                        .chunk()
                        .await
                        .map_err(|_| remote("response body failed"))
                })
                .await?
                {
                    if chunk.len() > limit.saturating_sub(bytes.len()) {
                        return Err(remote("response exceeds configured limit"));
                    }
                    bytes.extend_from_slice(&chunk);
                }
            }
            cancel.check()?;
            Ok(Response {
                status,
                headers,
                bytes,
            })
        }
        #[cfg(target_arch = "wasm32")]
        {
            browser::send(method, url, headers, body, cancel, limit).await
        }
    }

    async fn json(
        &self,
        method: Method,
        url: Url,
        value: Value,
        mut headers: HeaderMap,
        cancel: &CancelToken,
    ) -> Result<Response> {
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        self.send(
            method,
            url,
            headers,
            serde_json::to_vec(&value).map_err(|_| remote("JSON encoding failed"))?,
            cancel,
            METADATA_LIMIT,
        )
        .await?
        .success()
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn cancellable<T>(
    cancel: &CancelToken,
    future: impl Future<Output = Result<T>>,
) -> Result<T> {
    cancel.check()?;
    let mut future = std::pin::pin!(future);
    let interval = std::time::Duration::from_millis(25);
    let mut heartbeat = std::pin::pin!(tokio::time::sleep(interval));
    // Select the network future against a cancellation heartbeat, without adding
    // an executor/macros dependency to the shared token's atomic-only contract.
    std::future::poll_fn(|cx| {
        use std::task::Poll;
        if let Err(error) = cancel.check() {
            return Poll::Ready(Err(error));
        }
        if let Poll::Ready(result) = future.as_mut().poll(cx) {
            return Poll::Ready(cancel.check().and(result));
        }
        if heartbeat.as_mut().poll(cx).is_ready() {
            heartbeat
                .as_mut()
                .reset(tokio::time::Instant::now() + interval);
            let _ = heartbeat.as_mut().poll(cx);
        }
        Poll::Pending
    })
    .await
}

fn remote(message: &str) -> StorageError {
    StorageError::Remote(message.into())
}

fn validate_url(url: &Url) -> Result<()> {
    let loopback = is_loopback(url);
    if (url.scheme() != "https" && !(url.scheme() == "http" && loopback))
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(remote("invalid or insecure endpoint"));
    }
    Ok(())
}

fn endpoint(raw: &str, hosts: &[&str]) -> Result<Url> {
    let mut url = Url::parse(raw).map_err(|_| remote("invalid endpoint"))?;
    validate_url(&url)?;
    let mock = is_loopback(&url);
    if !mock && !hosts.is_empty() && !hosts.contains(&url.host_str().unwrap_or("")) {
        return Err(remote("endpoint is not a supported provider host"));
    }
    if url.query().is_some() {
        return Err(remote("base endpoint must not contain a query"));
    }
    if !url.path().ends_with('/') {
        url.set_path(&format!("{}/", url.path()));
    }
    Ok(url)
}

fn is_loopback(url: &Url) -> bool {
    // No DNS-based "localhost" exception; only literal loopback addresses.
    url.host_str()
        .unwrap_or("")
        .trim_matches(['[', ']'])
        .parse::<std::net::IpAddr>()
        .is_ok_and(|ip| ip.is_loopback())
}

fn route(base: &Url, segments: &[&str]) -> Result<Url> {
    let mut url = base.clone();
    {
        let mut path = url
            .path_segments_mut()
            .map_err(|_| remote("invalid base path"))?;
        path.pop_if_empty();
        for segment in segments {
            if segment.is_empty() || matches!(*segment, "." | "..") {
                return Err(remote("invalid storage identifier"));
            }
            path.push(segment);
        }
    }
    Ok(url)
}

fn trusted_link(raw: &str, base: &Url, suffixes: &[&str]) -> Result<Url> {
    let url = Url::parse(raw).map_err(|_| remote("invalid provider URL"))?;
    validate_url(&url)?;
    // Loopback responses cannot redirect to another server or out onto the Internet.
    let allowed = if is_loopback(base) {
        url.origin() == base.origin()
    } else {
        url.scheme() == "https"
            && url.port_or_known_default() == Some(443)
            && suffixes.iter().any(|suffix| {
                let host = url.host_str().unwrap_or("");
                host == *suffix || host.ends_with(&format!(".{suffix}"))
            })
    };
    if !allowed {
        return Err(remote("untrusted provider URL"));
    }
    Ok(url)
}

fn authenticated() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("x-thinoda-auth", HeaderValue::from_static("1"));
    headers
}

fn conditional(expected: &ExpectedRevision) -> Result<HeaderMap> {
    let mut headers = authenticated();
    match expected {
        ExpectedRevision::Missing => {
            headers.insert(IF_NONE_MATCH, HeaderValue::from_static("*"));
        }
        ExpectedRevision::Match(revision) => {
            if revision.0.is_empty()
                || revision.0 == "*"
                || revision.0.starts_with("W/")
                || revision.0.contains(',')
            {
                return Err(remote("conditional writes require a strong revision"));
            }
            headers.insert(
                IF_MATCH,
                HeaderValue::from_str(&revision.0)
                    .map_err(|_| remote("invalid revision header"))?,
            );
        }
        ExpectedRevision::Any => {}
    }
    Ok(headers)
}

fn string<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| remote("missing response field"))
}

fn revision(value: &Value, field: &str) -> Result<Revision> {
    Ok(Revision(string(value, field)?.into()))
}

fn capabilities(versions: bool, share_links: bool) -> Capabilities {
    Capabilities {
        placeholders: false,
        revisions: true,
        versions,
        share_links,
        watch: false,
    }
}

fn new_file_name(name: &str) -> Result<()> {
    if name.trim().is_empty()
        || matches!(name, "." | "..")
        || name
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\'))
    {
        return Err(remote("invalid new file name"));
    }
    Ok(())
}

fn created_entry(data: &Value, name: &str, size: usize, tag: &str) -> Result<Entry> {
    if string(data, "id")?.is_empty() || string(data, "name")? != name {
        return Err(remote(
            "creation succeeded with unexpected identity; reread before retrying",
        ));
    }
    let revision = revision(data, tag)?;
    if revision.0.is_empty() {
        return Err(remote(
            "creation succeeded without a revision; reread before retrying",
        ));
    }
    Ok(Entry {
        id: StorageId(string(data, "id")?.into()),
        name: name.into(),
        size: size as u64,
        revision: Some(revision),
        availability: Availability::OnlineOnly,
        is_dir: false,
    })
}

/// Original multipart framing, without an additional optional reqwest feature.
/// Attributes precede content; boundaries never occur in either part.
fn multipart_file(attributes: &Value, bytes: &[u8]) -> Result<(HeaderValue, Vec<u8>)> {
    multipart(attributes, bytes, false)
}

fn multipart_related(attributes: &Value, bytes: &[u8]) -> Result<(HeaderValue, Vec<u8>)> {
    multipart(attributes, bytes, true)
}

fn multipart(attributes: &Value, bytes: &[u8], related: bool) -> Result<(HeaderValue, Vec<u8>)> {
    let metadata =
        serde_json::to_vec(attributes).map_err(|_| remote("multipart metadata failed"))?;
    let boundary = loop {
        let candidate = format!("thinoda-{}", uuid::Uuid::new_v4().simple());
        if !bytes
            .windows(candidate.len())
            .any(|w| w == candidate.as_bytes())
            && !metadata
                .windows(candidate.len())
                .any(|w| w == candidate.as_bytes())
        {
            break candidate;
        }
    };
    let kind = if related { "related" } else { "form-data" };
    let content_type = HeaderValue::from_str(&format!("multipart/{kind}; boundary={boundary}"))
        .map_err(|_| remote("multipart encoding failed"))?;
    let disposition = if related {
        ""
    } else {
        "Content-Disposition: form-data; name=\"attributes\"\r\n"
    };
    let mut body =
        format!("--{boundary}\r\n{disposition}Content-Type: application/json\r\n\r\n").into_bytes();
    body.extend_from_slice(&metadata);
    let disposition = if related {
        ""
    } else {
        "Content-Disposition: form-data; name=\"file\"; filename=\"document.thinoda\"\r\n"
    };
    body.extend_from_slice(
        format!("\r\n--{boundary}\r\n{disposition}Content-Type: application/octet-stream\r\n\r\n")
            .as_bytes(),
    );
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    Ok((content_type, body))
}

fn entries<'a>(value: &'a Value, field: &str) -> Result<&'a [Value]> {
    value
        .get(field)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| remote("missing response entries"))
}

/// Provider version metadata. It does not download any version content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteVersion {
    pub id: String,
    pub size: Option<u64>,
    pub modified: Option<String>,
}

fn versions(value: &Value, field: &str) -> Result<Vec<RemoteVersion>> {
    entries(value, field)?
        .iter()
        .map(|item| {
            Ok(RemoteVersion {
                id: string(item, "id")?.into(),
                size: item.get("size").and_then(Value::as_u64),
                modified: item
                    .get("lastModifiedDateTime")
                    .or(item.get("modified_at"))
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            })
        })
        .collect()
}

/// An explicitly created sharing URL; do not log it (it may grant access).
pub struct ShareLink(pub String);

#[cfg(not(target_arch = "wasm32"))]
async fn content(
    http: &Http,
    url: Url,
    base: &Url,
    download_hosts: &[&str],
    cancel: &CancelToken,
) -> Result<ReadResult> {
    let response = http
        .send(
            Method::GET,
            url,
            authenticated(),
            Vec::new(),
            cancel,
            CONTENT_LIMIT,
        )
        .await?;
    let response = if matches!(response.status, 301 | 302 | 303 | 307 | 308) {
        let location = response
            .headers
            .get("location")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| remote("download redirect has no location"))?;
        let target = trusted_link(location, base, download_hosts)?;
        // Exactly one validated, preauthenticated hop, WITHOUT account credentials.
        http.send(
            Method::GET,
            target,
            HeaderMap::new(),
            Vec::new(),
            cancel,
            CONTENT_LIMIT,
        )
        .await?
        .success()?
    } else {
        response.success()?
    };
    if response.status != 200 {
        return Err(remote("download did not return complete content"));
    }
    let revision = response
        .revision()
        .ok_or_else(|| remote("download has no byte-bound ETag"))?;
    Ok(ReadResult {
        revision,
        bytes: response.bytes,
    })
}

/// Obtain a documented preauthenticated URL without asking Fetch to follow an
/// Authorization-bearing redirect. Metadata ETags are deliberately ignored:
/// only the download response can identify the bytes returned to the caller.
async fn metadata_content(
    http: &Http,
    metadata_url: Url,
    requested_id: &StorageId,
    base: &Url,
    download_field: &str,
    download_hosts: &[&str],
    cancel: &CancelToken,
) -> Result<ReadResult> {
    let metadata = http
        .send(
            Method::GET,
            metadata_url,
            authenticated(),
            Vec::new(),
            cancel,
            METADATA_LIMIT,
        )
        .await?
        .success()?
        .json()?;
    if string(&metadata, "id")? != requested_id.0 {
        return Err(remote("download metadata returned a different file"));
    }
    let raw = metadata
        .get(download_field)
        .and_then(Value::as_str)
        .filter(|url| !url.is_empty())
        .ok_or(StorageError::Unsupported)?;
    let target = trusted_link(raw, base, download_hosts)?;
    let response = http
        .send(
            Method::GET,
            target,
            HeaderMap::new(),
            Vec::new(),
            cancel,
            CONTENT_LIMIT,
        )
        .await?
        .success()?;
    // Reject redirects even to another trusted host: the metadata URL already
    // authorizes one specific download. No token-bearing or signed-url chain.
    if response.status != 200 {
        return Err(remote("metadata download did not return complete content"));
    }
    let revision = response
        .revision()
        .ok_or_else(|| remote("metadata download has no byte-bound ETag"))?;
    Ok(ReadResult {
        bytes: response.bytes,
        revision,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_urls_require_https_and_exact_provider_domain_boundaries() {
        let base = endpoint(
            "https://graph.microsoft.com/v1.0/",
            &["graph.microsoft.com"],
        )
        .unwrap();
        for raw in [
            "http://sn3302.up.1drv.com/session",
            "https://1drv.com.untrusted.invalid/session",
            "https://evil1drv.com/session",
            "https://user:secret@sn3302.up.1drv.com/session",
            "https://sn3302.up.1drv.com:8443/session",
            "https://127.0.0.1/session",
        ] {
            assert!(trusted_link(raw, &base, &["1drv.com", "sharepoint.com"]).is_err());
        }
        assert!(
            trusted_link(
                "https://sn3302.up.1drv.com/session?opaque=mock",
                &base,
                &["1drv.com"]
            )
            .is_ok()
        );
        assert!(
            trusted_link(
                "https://tenant.sharepoint.com/session?opaque=mock",
                &base,
                &["sharepoint.com"]
            )
            .is_ok()
        );
        assert!(conditional(&ExpectedRevision::Match(Revision("*".into()))).is_err());
        assert!(conditional(&ExpectedRevision::Match(Revision("W/\"weak\"".into()))).is_err());
        assert!(conditional(&ExpectedRevision::Match(Revision("\"a\",\"b\"".into()))).is_err());
    }
}
