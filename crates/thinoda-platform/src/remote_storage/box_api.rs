use super::*;
use serde_json::json;

pub struct BoxConfig {
    /// Normally `https://api.box.com/2.0/`.
    pub api_url: String,
    /// Normally `https://upload.box.com/api/2.0/`.
    pub upload_url: String,
    pub access_token: String,
}

pub struct BoxProvider {
    api: Url,
    upload: Url,
    http: Http,
}

/// Sharing is never implicit. Collaborators is the least expansive option.
pub enum BoxShareAccess {
    Collaborators,
    Company,
    Open,
}

impl BoxProvider {
    /// Authenticated account root; folder IDs need not be entered by users.
    pub fn default_root(&self) -> StorageId {
        StorageId("0".into())
    }

    pub fn new(config: BoxConfig) -> Result<Self> {
        Ok(Self {
            api: endpoint(&config.api_url, &["api.box.com"])?,
            upload: endpoint(&config.upload_url, &["upload.box.com"])?,
            http: Http::new(Authentication::Bearer(config.access_token))?,
        })
    }

    /// New file upload, not a replacement or preflight-then-overwrite operation.
    /// Box rejects an occupied sibling name at commit with HTTP 409.
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
                return Err(remote("Box parent ID is required"));
            }
            if bytes.len() > 50_000_000 {
                return Err(StorageError::Unsupported);
            }
            let (content_type, body) =
                multipart_file(&json!({"name": name, "parent": {"id": parent.0}}), bytes)?;
            let mut headers = conditional(&ExpectedRevision::Missing)?;
            headers.insert(CONTENT_TYPE, content_type);
            let response = self
                .http
                .send(
                    Method::POST,
                    route(&self.upload, &["files", "content"])?,
                    headers,
                    body,
                    &cancel,
                    METADATA_LIMIT,
                )
                .await?
                .success()?;
            if response.status != 201 {
                return Err(remote("Box creation did not return Created"));
            }
            let data = response.json()?;
            let items = entries(&data, "entries")?;
            if items.len() != 1 {
                return Err(remote("Box creation returned unexpected entry count"));
            }
            created_entry(&items[0], name, bytes.len(), "etag")
        })
    }

    /// Browser-safe direct download when the account/server exposes Box's
    /// top-level preauthorized `download_url`. Never creates or uses a shared link.
    pub fn read_via_download_url<'a>(
        &'a self,
        id: &'a StorageId,
        cancel: CancelToken,
    ) -> StorageFuture<'a, ReadResult> {
        Box::pin(async move {
            let mut url = route(&self.api, &["files", &id.0])?;
            url.query_pairs_mut()
                .append_pair("fields", "id,download_url");
            metadata_content(
                &self.http,
                url,
                id,
                &self.api,
                "download_url",
                &["boxcloud.com"],
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
            let mut url = route(&self.api, &["files", &id.0, "versions"])?;
            url.query_pairs_mut().append_pair("limit", "1000");
            let response = self
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
            if response.get("next_marker").is_some_and(|v| !v.is_null())
                || response
                    .get("total_count")
                    .and_then(Value::as_u64)
                    .is_some_and(|n| {
                        n > entries(&response, "entries").map_or(0, |e| e.len() as u64)
                    })
            {
                return Err(remote("Box version listing exceeds single-page limit"));
            }
            versions(&response, "entries")
        })
    }

    pub fn create_share_link<'a>(
        &'a self,
        id: &'a StorageId,
        access: BoxShareAccess,
        cancel: CancelToken,
    ) -> StorageFuture<'a, ShareLink> {
        Box::pin(async move {
            let access = match access {
                BoxShareAccess::Collaborators => "collaborators",
                BoxShareAccess::Company => "company",
                BoxShareAccess::Open => "open",
            };
            let data = self.http.json(Method::PUT, route(&self.api, &["files", &id.0])?, json!({"shared_link": {"access": access, "permissions": {"can_download": true, "can_preview": true}}}), authenticated(), &cancel).await?.json()?;
            let link = data
                .get("shared_link")
                .ok_or_else(|| remote("Box did not create a share link"))?;
            let raw = string(link, "url")?;
            trusted_link(raw, &self.api, &["box.com"])?;
            Ok(ShareLink(raw.into()))
        })
    }
}

impl StorageProvider for BoxProvider {
    fn capabilities(&self) -> Capabilities {
        capabilities(true, true)
    }

    fn list<'a>(
        &'a self,
        location: &'a StorageId,
        cancel: CancelToken,
    ) -> StorageFuture<'a, Vec<Entry>> {
        Box::pin(async move {
            let mut output = Vec::new();
            let mut marker: Option<String> = None;
            for _ in 0..PAGE_LIMIT {
                let mut url = route(&self.api, &["folders", &location.0, "items"])?;
                url.query_pairs_mut()
                    .append_pair("usemarker", "true")
                    .append_pair("limit", "1000")
                    .append_pair("fields", "id,name,size,etag,type");
                if let Some(ref marker) = marker {
                    url.query_pairs_mut().append_pair("marker", marker);
                }
                let response = self
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
                for item in entries(&response, "entries")? {
                    let is_dir = string(item, "type")? == "folder";
                    if !is_dir && string(item, "type")? != "file" {
                        continue;
                    }
                    output.push(Entry {
                        id: StorageId(string(item, "id")?.into()),
                        name: string(item, "name")?.into(),
                        size: item.get("size").and_then(Value::as_u64).unwrap_or(0),
                        revision: item
                            .get("etag")
                            .and_then(Value::as_str)
                            .map(|s| Revision(s.into())),
                        availability: Availability::OnlineOnly,
                        is_dir,
                    });
                }
                marker = response
                    .get("next_marker")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned);
                if marker.is_none() {
                    return Ok(output);
                }
            }
            Err(remote("Box listing exceeded page limit"))
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
                route(&self.api, &["files", &id.0, "content"])?,
                &self.api,
                &["boxcloud.com"],
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
            // This method replaces an existing ID. Creation needs a parent/name API
            // and cannot be represented by this stable-file-ID operation.
            if matches!(expected, ExpectedRevision::Missing) || bytes.len() > 50_000_000 {
                return Err(StorageError::Unsupported);
            }
            let mut headers = conditional(&expected)?;
            let (content_type, body) = multipart_file(&json!({}), bytes)?;
            headers.insert(CONTENT_TYPE, content_type);
            let response = self
                .http
                .send(
                    Method::POST,
                    route(&self.upload, &["files", &id.0, "content"])?,
                    headers,
                    body,
                    &cancel,
                    METADATA_LIMIT,
                )
                .await?
                .success()?
                .json()?;
            let entry = entries(&response, "entries")?
                .first()
                .ok_or_else(|| remote("Box upload returned no entry"))?;
            revision(entry, "etag")
        })
    }
}
