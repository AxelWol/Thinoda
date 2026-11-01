use super::*;

pub struct GoogleDriveConfig {
    /// Normally `https://www.googleapis.com/drive/v3/`.
    pub api_url: String,
    /// Normally `https://www.googleapis.com/upload/drive/v3/`.
    pub upload_url: String,
    pub access_token: String,
}

pub struct GoogleDriveProvider {
    api: Url,
    upload: Url,
    http: Http,
}

impl GoogleDriveProvider {
    pub fn new(config: GoogleDriveConfig) -> Result<Self> {
        Ok(Self {
            api: endpoint(&config.api_url, &["www.googleapis.com"])?,
            upload: endpoint(&config.upload_url, &["www.googleapis.com"])?,
            http: Http::new(Authentication::Bearer(config.access_token))?,
        })
    }

    /// Explicitly create a DISTINCT file ID. Drive allows duplicate sibling names:
    /// callers must warn/obtain consent for that policy, not promise Missing-name.
    /// No lookup, update target, overwrite fallback, or automatic retry is used.
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
                return Err(remote("Drive parent ID is required"));
            }
            if bytes.len() > 5_000_000 {
                return Err(StorageError::Unsupported);
            }
            let metadata = serde_json::json!({
                "name": name, "parents": [parent.0], "mimeType": "application/octet-stream"
            });
            let (content_type, body) = multipart_related(&metadata, bytes)?;
            let mut url = route(&self.upload, &["files"])?;
            url.query_pairs_mut()
                .append_pair("uploadType", "multipart")
                .append_pair("fields", "id,name");
            let mut headers = authenticated();
            // If-None-Match on the collection cannot impose sibling-name uniqueness.
            // This action creates a new resource, with no existing file ID in its URL.
            headers.insert(CONTENT_TYPE, content_type);
            let response = self
                .http
                .send(Method::POST, url, headers, body, &cancel, METADATA_LIMIT)
                .await?
                .success()?;
            if !matches!(response.status, 200 | 201) {
                return Err(remote("Drive creation did not complete"));
            }
            let revision = response
                .revision()
                .filter(|tag| !tag.0.is_empty())
                .ok_or_else(|| {
                    remote("Drive creation succeeded without an ETag; reread before retrying")
                })?;
            let data = response.json()?;
            if string(&data, "id")?.is_empty() || string(&data, "name")? != name {
                return Err(remote(
                    "creation succeeded with unexpected identity; reread before retrying",
                ));
            }
            Ok(Entry {
                id: StorageId(string(&data, "id")?.into()),
                name: name.into(),
                size: bytes.len() as u64,
                revision: Some(revision),
                availability: Availability::OnlineOnly,
                is_dir: false,
            })
        })
    }
}

impl StorageProvider for GoogleDriveProvider {
    fn capabilities(&self) -> Capabilities {
        capabilities(false, false)
    }

    fn list<'a>(
        &'a self,
        location: &'a StorageId,
        cancel: CancelToken,
    ) -> StorageFuture<'a, Vec<Entry>> {
        Box::pin(async move {
            // Escape the Drive query language independently of URL encoding.
            let folder = location.0.replace('\\', "\\\\").replace('\'', "\\'");
            let query = format!("'{folder}' in parents and trashed = false");
            let mut token: Option<String> = None;
            let mut output = Vec::new();
            for _ in 0..PAGE_LIMIT {
                let mut url = route(&self.api, &["files"])?;
                url.query_pairs_mut()
                    .append_pair("q", &query)
                    .append_pair("pageSize", "1000")
                    .append_pair("fields", "nextPageToken,files(id,name,size,mimeType)");
                if let Some(ref token) = token {
                    url.query_pairs_mut().append_pair("pageToken", token);
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
                for item in entries(&response, "files")? {
                    let is_dir = string(item, "mimeType")? == "application/vnd.google-apps.folder";
                    let size = match item.get("size") {
                        Some(Value::String(value)) => {
                            value.parse().map_err(|_| remote("invalid Drive size"))?
                        }
                        None => 0,
                        _ => return Err(remote("invalid Drive size")),
                    };
                    output.push(Entry {
                        id: StorageId(string(item, "id")?.into()),
                        name: string(item, "name")?.into(),
                        size,
                        revision: None, // v3 list resources do not contain per-file HTTP ETags.
                        availability: Availability::OnlineOnly,
                        is_dir,
                    });
                }
                token = response
                    .get("nextPageToken")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned);
                if token.is_none() {
                    return Ok(output);
                }
            }
            Err(remote("Drive listing exceeded page limit"))
        })
    }

    fn read<'a>(&'a self, id: &'a StorageId, cancel: CancelToken) -> StorageFuture<'a, ReadResult> {
        Box::pin(async move {
            let mut url = route(&self.api, &["files", &id.0])?;
            url.query_pairs_mut().append_pair("alt", "media");
            let response = self
                .http
                .send(
                    Method::GET,
                    url,
                    authenticated(),
                    Vec::new(),
                    &cancel,
                    CONTENT_LIMIT,
                )
                .await?
                .success()?;
            if response.status != 200 {
                return Err(remote("Drive download did not return complete content"));
            }
            let revision = response
                .revision()
                .ok_or_else(|| remote("Drive media response has no byte-bound ETag"))?;
            Ok(ReadResult {
                bytes: response.bytes,
                revision,
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
            // Stable-ID updates only. Explicit distinct-file creation is a separate
            // POST API; Missing must not turn this PATCH into creation or overwrite.
            if matches!(expected, ExpectedRevision::Missing) || bytes.len() > 5_000_000 {
                return Err(StorageError::Unsupported);
            }
            let mut url = route(&self.upload, &["files", &id.0])?;
            url.query_pairs_mut().append_pair("uploadType", "media");
            let mut headers = conditional(&expected)?;
            headers.insert(
                CONTENT_TYPE,
                HeaderValue::from_static("application/octet-stream"),
            );
            let response = self
                .http
                .send(
                    Method::PATCH,
                    url,
                    headers,
                    bytes.to_vec(),
                    &cancel,
                    METADATA_LIMIT,
                )
                .await?
                .success()?;
            response.revision().ok_or_else(|| {
                remote("Drive write succeeded without an ETag; reread before saving again")
            })
        })
    }
}
