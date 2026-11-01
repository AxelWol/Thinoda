use super::*;
use serde_json::json;

/// Explicit API roots: normally `https://api.dropboxapi.com/2/` and
/// `https://content.dropboxapi.com/2/`. Literal loopback roots support tests.
pub struct DropboxConfig {
    pub api_url: String,
    pub content_url: String,
    pub access_token: String,
}

pub struct DropboxProvider {
    api: Url,
    content: Url,
    http: Http,
}

impl DropboxProvider {
    pub fn new(config: DropboxConfig) -> Result<Self> {
        Ok(Self {
            api: endpoint(&config.api_url, &["api.dropboxapi.com"])?,
            content: endpoint(&config.content_url, &["content.dropboxapi.com"])?,
            http: Http::new(Authentication::Bearer(config.access_token))?,
        })
    }

    /// Parent is a Dropbox folder path (empty for root) or `id:...` folder ID.
    /// `add` + strict_conflict + no autorename is enforced by Dropbox at commit.
    pub fn create_file<'a>(
        &'a self,
        parent: &'a StorageId,
        name: &'a str,
        bytes: &'a [u8],
        cancel: CancelToken,
    ) -> StorageFuture<'a, Entry> {
        Box::pin(async move {
            cancel.check()?;
            new_file_name(name)?;
            if bytes.len() > 150_000_000 {
                return Err(StorageError::Unsupported);
            }
            if !parent.0.is_empty() && !parent.0.starts_with('/') && !parent.0.starts_with("id:") {
                return Err(remote("invalid Dropbox parent"));
            }
            let path = format!("{}/{name}", parent.0.trim_end_matches('/'));
            let mut headers = Self::argument(json!({
                "path": path, "mode": "add", "autorename": false, "mute": false,
                "strict_conflict": true,
            }))?;
            headers.insert(
                CONTENT_TYPE,
                HeaderValue::from_static("application/octet-stream"),
            );
            let response = self
                .http
                .send(
                    Method::POST,
                    route(&self.content, &["files", "upload"])?,
                    headers,
                    bytes.to_vec(),
                    &cancel,
                    METADATA_LIMIT,
                )
                .await?
                .success()?;
            created_entry(&response.json()?, name, bytes.len(), "rev")
        })
    }

    fn argument(value: Value) -> Result<HeaderMap> {
        let mut headers = authenticated();
        // Dropbox requires ASCII HTTP headers; JSON escaping also handles Unicode paths.
        let json = serde_json::to_string(&value).map_err(|_| remote("invalid Dropbox argument"))?;
        let ascii: String = json
            .chars()
            .map(|c| {
                if c.is_ascii() {
                    c.to_string()
                } else {
                    let mut units = [0; 2];
                    c.encode_utf16(&mut units)
                        .iter()
                        .map(|u| format!("\\u{u:04x}"))
                        .collect()
                }
            })
            .collect();
        headers.insert(
            "dropbox-api-arg",
            HeaderValue::from_str(&ascii).map_err(|_| remote("invalid Dropbox argument"))?,
        );
        Ok(headers)
    }
}

impl StorageProvider for DropboxProvider {
    fn capabilities(&self) -> Capabilities {
        capabilities(false, false)
    }

    fn list<'a>(
        &'a self,
        location: &'a StorageId,
        cancel: CancelToken,
    ) -> StorageFuture<'a, Vec<Entry>> {
        Box::pin(async move {
            cancel.check()?;
            let mut url = route(&self.api, &["files", "list_folder"])?;
            let mut argument =
                json!({"path": location.0, "recursive": false, "include_deleted": false});
            let mut output = Vec::new();
            for _ in 0..PAGE_LIMIT {
                let data = self
                    .http
                    .json(
                        Method::POST,
                        url.clone(),
                        argument,
                        authenticated(),
                        &cancel,
                    )
                    .await?
                    .json()?;
                for item in entries(&data, "entries")? {
                    let is_dir = string(item, ".tag")? == "folder";
                    output.push(Entry {
                        id: StorageId(string(item, "id")?.into()),
                        name: string(item, "name")?.into(),
                        size: item.get("size").and_then(Value::as_u64).unwrap_or(0),
                        revision: if is_dir {
                            None
                        } else {
                            Some(revision(item, "rev")?)
                        },
                        availability: Availability::OnlineOnly,
                        is_dir,
                    });
                }
                if data.get("has_more").and_then(Value::as_bool) == Some(false) {
                    return Ok(output);
                }
                argument = json!({"cursor": string(&data, "cursor")?});
                url = route(&self.api, &["files", "list_folder", "continue"])?;
            }
            Err(remote("Dropbox listing exceeded page limit"))
        })
    }

    fn read<'a>(&'a self, id: &'a StorageId, cancel: CancelToken) -> StorageFuture<'a, ReadResult> {
        Box::pin(async move {
            cancel.check()?;
            let response = self
                .http
                .send(
                    Method::POST,
                    route(&self.content, &["files", "download"])?,
                    Self::argument(json!({"path": id.0}))?,
                    Vec::new(),
                    &cancel,
                    CONTENT_LIMIT,
                )
                .await?
                .success()?;
            if response.status != 200 {
                return Err(remote("Dropbox download did not return complete content"));
            }
            let metadata: Value = serde_json::from_str(
                response
                    .headers
                    .get("dropbox-api-result")
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| remote("Dropbox download has no metadata"))?,
            )
            .map_err(|_| remote("invalid Dropbox download metadata"))?;
            Ok(ReadResult {
                bytes: response.bytes,
                revision: revision(&metadata, "rev")?,
            })
        })
    }

    fn write<'a>(
        &'a self,
        id: &'a StorageId,
        bytes: &'a [u8],
        expected: ExpectedRevision,
        cancel: CancelToken,
    ) -> StorageFuture<'a, Revision> {
        Box::pin(async move {
            cancel.check()?;
            // The simple upload endpoint accepts at most 150 MB. No silent chunk fallback.
            if bytes.len() > 150_000_000 {
                return Err(StorageError::Unsupported);
            }
            let mode = match expected {
                ExpectedRevision::Missing => json!("add"),
                ExpectedRevision::Match(rev) if !rev.0.is_empty() => {
                    json!({".tag": "update", "update": rev.0})
                }
                ExpectedRevision::Match(_) => return Err(remote("empty Dropbox revision")),
                ExpectedRevision::Any => json!("overwrite"),
            };
            let mut headers = Self::argument(json!({
                "path": id.0, "mode": mode, "autorename": false,
                "mute": false, "strict_conflict": true
            }))?;
            headers.insert(
                CONTENT_TYPE,
                HeaderValue::from_static("application/octet-stream"),
            );
            let response = self
                .http
                .send(
                    Method::POST,
                    route(&self.content, &["files", "upload"])?,
                    headers,
                    bytes.to_vec(),
                    &cancel,
                    METADATA_LIMIT,
                )
                .await?
                .success()?;
            revision(&response.json()?, "rev")
        })
    }
}
