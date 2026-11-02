use super::*;
use serde_json::json;

const SIMPLE_LIMIT: usize = 4 * 1024 * 1024;
// Below 60 MiB, and a multiple of 320 KiB. Only the final fragment may be shorter.
const FRAGMENT_SIZE: usize = 10 * 327_680;

pub struct GraphConfig {
    /// Global Graph API root, normally `https://graph.microsoft.com/v1.0/`.
    pub api_url: String,
    pub drive_id: String,
    pub access_token: String,
}

/// Explicit delegated account discovery; never infer/broaden OAuth permissions.
pub struct GraphDiscoveryConfig {
    pub api_url: String,
    pub access_token: String,
    pub location: GraphStartLocation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphStartLocation {
    /// Ordinary least-privilege connection: Files.ReadWrite.AppFolder.
    AppFolder,
    /// Only after explicit broader Files.ReadWrite consent.
    DriveRoot,
}

/// IDs are obtained from authenticated metadata, not entered by the user.
/// No Debug: avoid logging account-identifying drive/folder IDs accidentally.
pub struct GraphDiscovery {
    pub provider: GraphProvider,
    pub drive_id: String,
    pub root: StorageId,
}

pub struct GraphProvider {
    api: Url,
    drive_id: String,
    http: Http,
}

pub enum GraphShareScope {
    Organization,
    Anonymous,
}

impl GraphProvider {
    pub fn new(config: GraphConfig) -> Result<Self> {
        if config.drive_id.is_empty() {
            return Err(remote("Graph drive ID is required"));
        }
        Ok(Self {
            api: endpoint(&config.api_url, &["graph.microsoft.com"])?,
            drive_id: config.drive_id,
            http: Http::new(Authentication::Bearer(config.access_token))?,
        })
    }

    /// Resolve an authenticated user's drive/app folder on explicit Connect.
    /// Only metadata GETs; no children enumeration or document transfer.
    /// App-folder resolution can create the registered app's folder server-side.
    pub fn discover(
        config: GraphDiscoveryConfig,
        cancel: CancelToken,
    ) -> StorageFuture<'static, GraphDiscovery> {
        Box::pin(async move {
            cancel.check()?;
            let api = endpoint(&config.api_url, &["graph.microsoft.com"])?;
            let http = Http::new(Authentication::Bearer(config.access_token))?;
            let (drive_id, folder) = match config.location {
                GraphStartLocation::AppFolder => {
                    let mut url = route(&api, &["me", "drive", "special", "approot"])?;
                    url.query_pairs_mut()
                        .append_pair("$select", "id,folder,specialFolder,parentReference");
                    let folder = discovery_json(&http, url, &cancel).await?;
                    if folder
                        .get("specialFolder")
                        .and_then(|v| v.get("name"))
                        .and_then(Value::as_str)
                        != Some("approot")
                    {
                        return Err(remote("discovery did not return the app folder"));
                    }
                    let drive_id = discovery_drive_id(&folder)?.to_owned();
                    (drive_id, folder)
                }
                GraphStartLocation::DriveRoot => {
                    let mut url = route(&api, &["me", "drive"])?;
                    url.query_pairs_mut().append_pair("$select", "id");
                    let drive = discovery_json(&http, url, &cancel).await?;
                    let drive_id = string(&drive, "id")?.to_owned();
                    if drive_id.is_empty() {
                        return Err(remote("discovery returned no drive ID"));
                    }
                    let mut url = route(&api, &["drives", &drive_id, "root"])?;
                    url.query_pairs_mut()
                        .append_pair("$select", "id,folder,parentReference");
                    let folder = discovery_json(&http, url, &cancel).await?;
                    if discovery_drive_id(&folder)? != drive_id {
                        return Err(remote("discovery returned a folder in a different drive"));
                    }
                    (drive_id, folder)
                }
            };
            if !folder.get("folder").is_some_and(Value::is_object) {
                return Err(remote("discovery did not return a folder"));
            }
            let id = string(&folder, "id")?;
            if id.is_empty() || matches!(id, "." | "..") || id.chars().any(char::is_control) {
                return Err(remote("discovery returned an invalid folder ID"));
            }
            cancel.check()?;
            Ok(GraphDiscovery {
                root: StorageId(id.into()),
                provider: Self {
                    api,
                    http,
                    drive_id: drive_id.clone(),
                },
                drive_id,
            })
        })
    }

    /// Create a named child, never replace or silently rename an occupied name.
    /// Returns the server-assigned stable item ID and committed response ETag.
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
            if parent.0.is_empty() {
                return Err(remote("Graph parent ID is required"));
            }
            if bytes.len() > CONTENT_LIMIT {
                return Err(StorageError::Unsupported);
            }
            // Graph's path-addressed child syntax. Encode each user segment
            // independently; no string concatenation of user-controlled URL paths.
            let parent_path = format!("{}:", parent.0);
            let child_path = format!("{name}:");
            let mut url = route(
                &self.api,
                &[
                    "drives",
                    &self.drive_id,
                    "items",
                    &parent_path,
                    &child_path,
                    if bytes.len() > SIMPLE_LIMIT {
                        "createUploadSession"
                    } else {
                        "content"
                    },
                ],
            )?;
            if bytes.len() > SIMPLE_LIMIT {
                let session = self.http.json(
                    Method::POST, url,
                    json!({"item": {"name": name, "@microsoft.graph.conflictBehavior": "fail"}}),
                    conditional(&ExpectedRevision::Missing)?, &cancel,
                ).await?.json()?;
                let upload_url = trusted_link(
                    string(&session, "uploadUrl")?,
                    &self.api,
                    &["1drv.com", "sharepoint.com"],
                )?;
                let data = self
                    .upload_session(upload_url, bytes, &cancel, true)
                    .await?;
                created_entry(&data, name, bytes.len(), "eTag")
            } else {
                url.query_pairs_mut()
                    .append_pair("@microsoft.graph.conflictBehavior", "fail");
                let mut headers = conditional(&ExpectedRevision::Missing)?;
                headers.insert(
                    CONTENT_TYPE,
                    HeaderValue::from_static("application/octet-stream"),
                );
                let response = self
                    .http
                    .send(
                        Method::PUT,
                        url,
                        headers,
                        bytes.to_vec(),
                        &cancel,
                        METADATA_LIMIT,
                    )
                    .await?
                    .success()?;
                if response.status != 201 {
                    return Err(remote("Graph creation did not return Created"));
                }
                created_entry(&response.json()?, name, bytes.len(), "eTag")
            }
        })
    }

    fn item(&self, id: &StorageId, suffix: Option<&str>) -> Result<Url> {
        let mut parts = vec!["drives", &self.drive_id, "items", &id.0];
        if let Some(suffix) = suffix {
            parts.push(suffix);
        }
        route(&self.api, &parts)
    }

    fn next_page(&self, raw: &str) -> Result<Url> {
        let url = Url::parse(raw).map_err(|_| remote("invalid Graph next page"))?;
        validate_url(&url)?;
        if url.origin() != self.api.origin() || !url.path().starts_with(self.api.path()) {
            return Err(remote("untrusted Graph next page"));
        }
        Ok(url)
    }

    /// Browser-safe download route, also callable natively for protocol tests.
    /// Does not create a share link; the short-lived capability is never exposed.
    pub fn read_via_download_url<'a>(
        &'a self,
        id: &'a StorageId,
        cancel: CancelToken,
    ) -> StorageFuture<'a, ReadResult> {
        Box::pin(async move {
            let mut url = self.item(id, None)?;
            url.query_pairs_mut()
                .append_pair("$select", "id,@microsoft.graph.downloadUrl");
            metadata_content(
                &self.http,
                url,
                id,
                &self.api,
                "@microsoft.graph.downloadUrl",
                &["1drv.com", "sharepoint.com", "storage.live.com"],
                &cancel,
            )
            .await
        })
    }

    pub fn list_versions<'a>(
        &'a self,
        id: &'a StorageId,
        cancel: CancelToken,
    ) -> StorageFuture<'a, Vec<RemoteVersion>> {
        Box::pin(async move {
            let mut url = self.item(id, Some("versions"))?;
            let mut output = Vec::new();
            for _ in 0..PAGE_LIMIT {
                let data = self
                    .http
                    .send(
                        Method::GET,
                        url,
                        authenticated(),
                        Vec::new(),
                        &cancel,
                        METADATA_LIMIT,
                    )
                    .await?
                    .success()?
                    .json()?;
                output.extend(versions(&data, "value")?);
                match data.get("@odata.nextLink").and_then(Value::as_str) {
                    None => return Ok(output),
                    Some(next) => url = self.next_page(next)?,
                }
            }
            Err(remote("Graph version listing exceeded page limit"))
        })
    }

    pub fn create_share_link<'a>(
        &'a self,
        id: &'a StorageId,
        scope: GraphShareScope,
        cancel: CancelToken,
    ) -> StorageFuture<'a, ShareLink> {
        Box::pin(async move {
            let scope = match scope {
                GraphShareScope::Organization => "organization",
                GraphShareScope::Anonymous => "anonymous",
            };
            let data = self
                .http
                .json(
                    Method::POST,
                    self.item(id, Some("createLink"))?,
                    json!({"type": "view", "scope": scope}),
                    authenticated(),
                    &cancel,
                )
                .await?
                .json()?;
            let raw = data
                .get("link")
                .and_then(|link| link.get("webUrl"))
                .and_then(Value::as_str)
                .ok_or_else(|| remote("Graph did not create a share link"))?;
            trusted_link(
                raw,
                &self.api,
                &["1drv.ms", "onedrive.live.com", "sharepoint.com"],
            )?;
            Ok(ShareLink(raw.into()))
        })
    }

    /// Sequential, non-resuming upload of an existing file. Preconditions are
    /// supplied at session creation; final conflicts are never retried/renamed.
    pub fn write_large<'a>(
        &'a self,
        id: &'a StorageId,
        bytes: &'a [u8],
        expected: ExpectedRevision,
        cancel: CancelToken,
    ) -> StorageFuture<'a, Revision> {
        Box::pin(async move {
            cancel.check()?;
            if matches!(expected, ExpectedRevision::Missing)
                || bytes.is_empty()
                || bytes.len() > CONTENT_LIMIT
            {
                return Err(StorageError::Unsupported);
            }
            let session = self
                .http
                .json(
                    Method::POST,
                    self.item(id, Some("createUploadSession"))?,
                    json!({"item": {"@microsoft.graph.conflictBehavior": "fail"}}),
                    conditional(&expected)?,
                    &cancel,
                )
                .await?
                .json()?;
            let upload_url = trusted_link(
                string(&session, "uploadUrl")?,
                &self.api,
                &["1drv.com", "sharepoint.com"],
            )?;
            let data = self
                .upload_session(upload_url, bytes, &cancel, false)
                .await?;
            revision(&data, "eTag")
        })
    }

    async fn upload_session(
        &self,
        upload_url: Url,
        bytes: &[u8],
        cancel: &CancelToken,
        creating: bool,
    ) -> Result<Value> {
        let result = self
            .upload_fragments(upload_url.clone(), bytes, cancel, creating)
            .await;
        if result.is_err() {
            // No bearer on the preauthenticated cleanup endpoint, either.
            // Bounded best effort; expired sessions are also cleaned by Graph.
            #[cfg(not(target_arch = "wasm32"))]
            {
                let cleanup = CancelToken::default();
                let _ = tokio::time::timeout(
                    std::time::Duration::from_secs(2),
                    self.http.send(
                        Method::DELETE,
                        upload_url,
                        HeaderMap::new(),
                        Vec::new(),
                        &cleanup,
                        METADATA_LIMIT,
                    ),
                )
                .await;
            }
            // Browser cancellation aborts Fetch; abandoned sessions expire.
        }
        result
    }

    async fn upload_fragments(
        &self,
        url: Url,
        bytes: &[u8],
        cancel: &CancelToken,
        creating: bool,
    ) -> Result<Value> {
        let total = bytes.len();
        for start in (0..total).step_by(FRAGMENT_SIZE) {
            cancel.check()?;
            let end = (start + FRAGMENT_SIZE).min(total);
            let mut headers = HeaderMap::new();
            headers.insert(
                CONTENT_TYPE,
                HeaderValue::from_static("application/octet-stream"),
            );
            headers.insert(
                "content-length",
                HeaderValue::from_str(&(end - start).to_string())
                    .map_err(|_| remote("invalid fragment size"))?,
            );
            headers.insert(
                "content-range",
                HeaderValue::from_str(&format!("bytes {start}-{}/{total}", end - 1))
                    .map_err(|_| remote("invalid fragment range"))?,
            );
            let response = self
                .http
                .send(
                    Method::PUT,
                    url.clone(),
                    headers,
                    bytes[start..end].to_vec(),
                    cancel,
                    METADATA_LIMIT,
                )
                .await?
                .success()?;
            let data = response.json()?;
            if end == total {
                if !matches!(response.status, 200 | 201) {
                    return Err(remote("Graph did not commit final fragment"));
                }
                if creating && response.status != 201 {
                    return Err(remote("Graph creation did not return Created"));
                }
                // The caller checks the committed response revision and, for
                // creation, the new stable ID/name. Never fetch newer metadata.
                return Ok(data);
            }
            if response.status != 202 {
                return Err(remote("Graph committed before all fragments were sent"));
            }
            let ranges = entries(&data, "nextExpectedRanges")?;
            // A fresh sequential session should acknowledge exactly the next offset.
            // Do not blindly replay, skip bytes, or guess resumption after a mismatch.
            if ranges.len() != 1 || ranges[0].as_str() != Some(format!("{end}-").as_str()) {
                return Err(remote("Graph upload returned unexpected next range"));
            }
        }
        Err(remote("Graph upload had no fragments"))
    }
}

async fn discovery_json(http: &Http, url: Url, cancel: &CancelToken) -> Result<Value> {
    let response = http
        .send(
            Method::GET,
            url,
            authenticated(),
            Vec::new(),
            cancel,
            METADATA_LIMIT,
        )
        .await?
        .success()?;
    if response.status != 200 {
        return Err(remote("discovery did not return complete metadata"));
    }
    response.json()
}

fn discovery_drive_id(folder: &Value) -> Result<&str> {
    let id = folder
        .get("parentReference")
        .and_then(|value| value.get("driveId"))
        .and_then(Value::as_str)
        .ok_or_else(|| remote("discovery returned no parent drive ID"))?;
    if id.is_empty() || matches!(id, "." | "..") || id.chars().any(char::is_control) {
        return Err(remote("discovery returned an invalid drive ID"));
    }
    Ok(id)
}

impl StorageProvider for GraphProvider {
    fn capabilities(&self) -> Capabilities {
        capabilities(true, true)
    }

    fn list<'a>(
        &'a self,
        location: &'a StorageId,
        cancel: CancelToken,
    ) -> StorageFuture<'a, Vec<Entry>> {
        Box::pin(async move {
            let mut url = self.item(location, Some("children"))?;
            url.query_pairs_mut()
                .append_pair("$select", "id,name,size,eTag,folder,file");
            let mut output = Vec::new();
            for _ in 0..PAGE_LIMIT {
                let data = self
                    .http
                    .send(
                        Method::GET,
                        url,
                        authenticated(),
                        Vec::new(),
                        &cancel,
                        METADATA_LIMIT,
                    )
                    .await?
                    .success()?
                    .json()?;
                for item in entries(&data, "value")? {
                    output.push(Entry {
                        id: StorageId(string(item, "id")?.into()),
                        name: string(item, "name")?.into(),
                        size: item.get("size").and_then(Value::as_u64).unwrap_or(0),
                        revision: item
                            .get("eTag")
                            .and_then(Value::as_str)
                            .map(|s| Revision(s.into())),
                        availability: Availability::OnlineOnly,
                        is_dir: item.get("folder").is_some(),
                    });
                }
                match data.get("@odata.nextLink").and_then(Value::as_str) {
                    None => return Ok(output),
                    Some(next) => url = self.next_page(next)?,
                }
            }
            Err(remote("Graph listing exceeded page limit"))
        })
    }

    fn read<'a>(&'a self, id: &'a StorageId, cancel: CancelToken) -> StorageFuture<'a, ReadResult> {
        #[cfg(target_arch = "wasm32")]
        {
            self.read_via_download_url(id, cancel)
        }
        #[cfg(not(target_arch = "wasm32"))]
        Box::pin(async move {
            content(
                &self.http,
                self.item(id, Some("content"))?,
                &self.api,
                &["1drv.com", "sharepoint.com", "storage.live.com"],
                &cancel,
            )
            .await
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
            if matches!(expected, ExpectedRevision::Missing) {
                return Err(StorageError::Unsupported);
            }
            if bytes.len() > SIMPLE_LIMIT {
                return self.write_large(id, bytes, expected, cancel).await;
            }
            let mut headers = conditional(&expected)?;
            headers.insert(
                CONTENT_TYPE,
                HeaderValue::from_static("application/octet-stream"),
            );
            let response = self
                .http
                .send(
                    Method::PUT,
                    self.item(id, Some("content"))?,
                    headers,
                    bytes.to_vec(),
                    &cancel,
                    METADATA_LIMIT,
                )
                .await?
                .success()?;
            if !matches!(response.status, 200 | 201) {
                return Err(remote("Graph write did not complete"));
            }
            revision(&response.json()?, "eTag")
        })
    }
}
