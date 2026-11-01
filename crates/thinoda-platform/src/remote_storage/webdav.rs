use super::*;
use quick_xml::{NsReader, events::Event, name::ResolveResult};

/// A DAV collection URL (including account/path) and explicit in-memory credentials.
pub struct WebDavConfig {
    pub collection_url: String,
    pub authentication: Authentication,
}

pub struct WebDavProvider {
    base: Url,
    http: Http,
}

impl WebDavProvider {
    pub fn new(config: WebDavConfig) -> Result<Self> {
        Ok(Self {
            base: endpoint(&config.collection_url, &[])?,
            http: Http::new(config.authentication)?,
        })
    }

    /// Create one named child atomically with `If-None-Match: *`.
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
            let parent = self.target(parent)?;
            let id = StorageId(route(&parent, &[name])?.to_string());
            let revision = self
                .write(&id, bytes, ExpectedRevision::Missing, cancel)
                .await?;
            Ok(Entry {
                id,
                name: name.into(),
                size: bytes.len() as u64,
                revision: Some(revision),
                availability: Availability::OnlineOnly,
                is_dir: false,
            })
        })
    }

    /// Build an opaque DAV ID beneath the configured collection, without network I/O.
    pub fn id(&self, relative_path: &str) -> Result<StorageId> {
        if relative_path.is_empty() {
            return Ok(StorageId(self.base.to_string()));
        }
        let segments: Vec<_> = relative_path.split('/').collect();
        Ok(StorageId(route(&self.base, &segments)?.to_string()))
    }

    fn target(&self, id: &StorageId) -> Result<Url> {
        let url = Url::parse(&id.0).map_err(|_| remote("invalid DAV identifier"))?;
        validate_url(&url)?;
        if url.origin() != self.base.origin()
            || !url.path().starts_with(self.base.path())
            || url.query().is_some()
            || url.path().to_ascii_lowercase().contains("%2f")
            || url.path().to_ascii_lowercase().contains("%5c")
        {
            return Err(remote("DAV identifier is outside configured collection"));
        }
        Ok(url)
    }
}

impl StorageProvider for WebDavProvider {
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
            let url = self.target(location)?;
            let mut headers = authenticated();
            headers.insert("depth", HeaderValue::from_static("1"));
            headers.insert(
                CONTENT_TYPE,
                HeaderValue::from_static("application/xml; charset=utf-8"),
            );
            let xml = br#"<?xml version="1.0" encoding="utf-8"?><d:propfind xmlns:d="DAV:"><d:prop><d:displayname/><d:getcontentlength/><d:getetag/><d:resourcetype/></d:prop></d:propfind>"#;
            let response = self
                .http
                .send(
                    Method::from_bytes(b"PROPFIND").expect("static method"),
                    url.clone(),
                    headers,
                    xml.to_vec(),
                    &cancel,
                    METADATA_LIMIT,
                )
                .await?
                .success()?;
            if response.status != 207 {
                return Err(remote("DAV listing requires a multistatus response"));
            }
            parse_list(&response.bytes, &url, self, &cancel)
        })
    }

    fn read<'a>(&'a self, id: &'a StorageId, cancel: CancelToken) -> StorageFuture<'a, ReadResult> {
        Box::pin(async move {
            cancel.check()?;
            let response = self
                .http
                .send(
                    Method::GET,
                    self.target(id)?,
                    authenticated(),
                    Vec::new(),
                    &cancel,
                    CONTENT_LIMIT,
                )
                .await?
                .success()?;
            if response.status != 200 {
                return Err(remote("DAV download did not return complete content"));
            }
            let revision = response
                .revision()
                .ok_or_else(|| remote("DAV download has no byte-bound ETag"))?;
            Ok(ReadResult {
                revision,
                bytes: response.bytes,
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
            if bytes.len() > CONTENT_LIMIT {
                return Err(StorageError::Unsupported);
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
                    self.target(id)?,
                    headers,
                    bytes.to_vec(),
                    &cancel,
                    METADATA_LIMIT,
                )
                .await?
                .success()?;
            if !matches!(response.status, 200 | 201 | 204) {
                return Err(remote("DAV write did not complete"));
            }
            // Never fetch a post-write ETag: another writer could already have replaced it.
            response.revision().ok_or_else(|| {
                remote("write succeeded without an ETag; reread before saving again")
            })
        })
    }
}

#[derive(Default)]
struct Properties {
    name: String,
    size: String,
    etag: Option<String>,
    is_dir: bool,
}

fn parse_list(
    xml: &[u8],
    location: &Url,
    provider: &WebDavProvider,
    cancel: &CancelToken,
) -> Result<Vec<Entry>> {
    let mut reader = NsReader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut stack: Vec<String> = Vec::new();
    let mut href = String::new();
    let mut props = Properties::default();
    let mut accepted = Properties::default();
    let mut status = String::new();
    let mut output = Vec::new();
    let mut rooted = false;
    let mut has_success = false;
    loop {
        cancel.check()?;
        let (namespace, event) = reader
            .read_resolved_event()
            .map_err(|_| remote("invalid DAV XML"))?;
        let dav = matches!(namespace, ResolveResult::Bound(ns) if ns.as_ref() == "DAV:");
        match event {
            Event::Start(tag) => {
                let name = if dav {
                    tag.local_name().as_ref().to_owned()
                } else {
                    String::new()
                };
                if stack.is_empty() {
                    if rooted || name != "multistatus" {
                        return Err(remote("DAV response has no multistatus root"));
                    }
                    rooted = true;
                }
                if name == "response" {
                    href.clear();
                    accepted = Properties::default();
                    has_success = false;
                } else if name == "propstat" {
                    props = Properties::default();
                    status.clear();
                } else if name == "collection" {
                    props.is_dir = true;
                }
                stack.push(name);
                if stack.len() > 128 {
                    return Err(remote("DAV XML exceeds nesting limit"));
                }
            }
            Event::Empty(tag)
                if dav
                    && tag.local_name().as_ref() == "multistatus"
                    && stack.is_empty()
                    && !rooted =>
            {
                rooted = true
            }
            Event::Empty(tag) if dav && tag.local_name().as_ref() == "collection" => {
                props.is_dir = true
            }
            Event::Text(text) => {
                let decoded = text.xml10_content();
                append_text(&stack, &decoded, &mut href, &mut props, &mut status);
            }
            Event::CData(text) => {
                let text = text.xml10_content();
                append_text(&stack, &text, &mut href, &mut props, &mut status);
            }
            Event::GeneralRef(reference) => {
                let name = reference.xml10_content();
                let value = quick_xml::escape::unescape(&format!("&{name};"))
                    .map_err(|_| remote("invalid DAV reference"))?
                    .into_owned();
                append_text(&stack, &value, &mut href, &mut props, &mut status);
            }
            Event::End(_) => match stack.pop().as_deref() {
                Some("propstat") if status.split_whitespace().nth(1) == Some("200") => {
                    has_success = true;
                    if !props.name.is_empty() {
                        accepted.name = std::mem::take(&mut props.name);
                    }
                    if !props.size.is_empty() {
                        accepted.size = std::mem::take(&mut props.size);
                    }
                    if props.etag.is_some() {
                        accepted.etag = props.etag.take();
                    }
                    accepted.is_dir |= props.is_dir;
                }
                Some("response") => {
                    if href.trim().is_empty() {
                        return Err(remote("DAV response has no href"));
                    }
                    let url = location
                        .join(href.trim())
                        .map_err(|_| remote("invalid DAV href"))?;
                    provider.target(&StorageId(url.to_string()))?;
                    if url.path().trim_end_matches('/') == location.path().trim_end_matches('/') {
                        continue;
                    }
                    if !has_success {
                        continue;
                    }
                    let prefix = format!("{}/", location.path().trim_end_matches('/'));
                    let Some(relative) = url.path().strip_prefix(&prefix) else {
                        return Err(remote("DAV listing escaped collection"));
                    };
                    if relative.trim_end_matches('/').contains('/') {
                        return Err(remote("DAV listing returned non-child"));
                    }
                    if accepted.name.is_empty() {
                        accepted.name = relative.trim_end_matches('/').into();
                    }
                    output.push(Entry {
                        id: StorageId(url.to_string()),
                        name: std::mem::take(&mut accepted.name),
                        size: if accepted.size.is_empty() {
                            0
                        } else {
                            accepted
                                .size
                                .trim()
                                .parse()
                                .map_err(|_| remote("invalid DAV size"))?
                        },
                        revision: accepted.etag.take().map(Revision),
                        availability: Availability::OnlineOnly,
                        is_dir: accepted.is_dir,
                    });
                }
                _ => {}
            },
            Event::DocType(_) => return Err(remote("DAV document types are not supported")),
            Event::Eof => break,
            _ => {}
        }
    }
    if !stack.is_empty() || !rooted {
        return Err(remote("incomplete DAV XML"));
    }
    output.sort_by(|a, b| a.id.0.cmp(&b.id.0));
    output.dedup_by(|a, b| a.id == b.id);
    Ok(output)
}

fn append_text(
    stack: &[String],
    text: &str,
    href: &mut String,
    props: &mut Properties,
    status: &mut String,
) {
    match stack.last().map(String::as_str) {
        Some("href") => href.push_str(text),
        Some("displayname") => props.name.push_str(text),
        Some("getcontentlength") => props.size.push_str(text),
        Some("getetag") => props.etag.get_or_insert_with(String::new).push_str(text),
        Some("status") => status.push_str(text),
        _ => {}
    }
}
