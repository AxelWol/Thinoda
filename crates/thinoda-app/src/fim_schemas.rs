//! Consented installation of pinned FIM schemas. No document bytes enter this module.
//!
//! `ensure_cached` is strictly local. Only `request_download`, called after the
//! UI has obtained explicit consent, can schedule HTTP. A successful installation
//! verifies and compiles the complete bundle before atomically persisting it.

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard,
        mpsc::{self, Receiver},
    },
};
use thinoda_io::fim::{
    Dialect, ExchangeValidator, FimError, VALIDATION_POLICY_VERSION, official_downloads,
};
use thinoda_platform::{Paths, fs};

// Keep in step with the IO archive limit when re-pinning official artifacts.
const MAX_ARCHIVE_BYTES: usize = 1024 * 1024;
const HEADER: &[u8; 8] = b"FIMZIP01";
const MAX_BUNDLE_BYTES: usize = 16 + 2 * MAX_ARCHIVE_BYTES;

/// Availability of an exchange validator, NOT a document validation result.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum SchemaStatus {
    #[default]
    Unchecked,
    Checking,
    Downloading,
    Installing,
    Removing,
    Ready,
    Unavailable(String),
}

#[derive(Default)]
struct Entry {
    status: SchemaStatus,
    validator: Option<Arc<Mutex<ExchangeValidator>>>,
}

enum Work {
    Cached,
    Download,
    Bytes(Vec<u8>, Option<Vec<u8>>),
    Paths(PathBuf, Option<PathBuf>),
    Remove,
}

struct Pending {
    dialect: Dialect,
    work: Work,
}

type Outcome = Result<Option<Arc<Mutex<ExchangeValidator>>>, String>;

/// Two dialect-specific compiled caches with one bounded worker at a time.
///
/// Requests return false while another request is outstanding; callers may retry
/// local checks on a later frame. Failed operations never retain a Ready validator.
pub(crate) struct FimSchemas {
    root: Option<PathBuf>,
    entries: [Entry; 2],
    pending: Option<Pending>,
    receiver: Option<(Dialect, Receiver<Outcome>)>,
    context: Option<egui::Context>,
}

impl Default for FimSchemas {
    fn default() -> Self {
        Self::new(None)
    }
}

impl FimSchemas {
    /// `None` intentionally disables persistence (for example headless tests).
    /// Browser callers can pass `Paths::new()` just as native callers do.
    pub(crate) fn new(paths: Option<&Paths>) -> Self {
        Self {
            // Verified archives are persistent data, not an evictable HTTP cache.
            root: paths.map(|p| p.data_dir.join("fim-schemas")),
            entries: Default::default(),
            pending: None,
            receiver: None,
            context: None,
        }
    }

    pub(crate) fn status(&self, dialect: Dialect) -> &SchemaStatus {
        &self.entries[index(dialect)].status
    }

    pub(crate) fn busy(&self) -> bool {
        self.pending.is_some() || self.receiver.is_some()
    }

    pub(crate) fn validator(&self, dialect: Dialect) -> Option<MutexGuard<'_, ExchangeValidator>> {
        self.entries[index(dialect)]
            .validator
            .as_ref()?
            .try_lock()
            .ok()
    }

    /// Clone this handle when document validation also needs a native worker.
    pub(crate) fn validator_handle(
        &self,
        dialect: Dialect,
    ) -> Option<Arc<Mutex<ExchangeValidator>>> {
        self.entries[index(dialect)].validator.clone()
    }

    /// Check once, without network access or automatic retries after failure.
    pub(crate) fn ensure_cached(&mut self, dialect: Dialect) -> bool {
        if !matches!(self.status(dialect), SchemaStatus::Unchecked) {
            return false;
        }
        self.check_cached(dialect)
    }

    /// Explicit local retry, including after a manual out-of-process installation.
    pub(crate) fn check_cached(&mut self, dialect: Dialect) -> bool {
        self.schedule(dialect, Work::Cached, SchemaStatus::Checking)
    }

    /// The caller MUST obtain explicit consent before invoking this method.
    /// Calling `ensure_cached`, opening a document, or polling never grants consent.
    pub(crate) fn request_download(&mut self, dialect: Dialect) -> bool {
        self.schedule(dialect, Work::Download, SchemaStatus::Downloading)
    }

    /// Install user-selected local archives; XDF3 needs both exchange and examples.
    #[allow(dead_code)] // Alternate adapter API; file-picker UI may use install_paths.
    pub(crate) fn install_bytes(
        &mut self,
        dialect: Dialect,
        exchange: Vec<u8>,
        supplement: Option<Vec<u8>>,
    ) -> bool {
        self.schedule(
            dialect,
            Work::Bytes(exchange, supplement),
            SchemaStatus::Installing,
        )
    }

    /// Paths are native files or paths previously uploaded to the browser VFS.
    pub(crate) fn install_paths(
        &mut self,
        dialect: Dialect,
        exchange: PathBuf,
        supplement: Option<PathBuf>,
    ) -> bool {
        self.schedule(
            dialect,
            Work::Paths(exchange, supplement),
            SchemaStatus::Installing,
        )
    }

    /// Explicit removal of this exact bundle/policy identity only.
    pub(crate) fn remove_cached(&mut self, dialect: Dialect) -> bool {
        self.schedule(dialect, Work::Remove, SchemaStatus::Removing)
    }

    /// Cancel an action queued during this frame, before `poll` starts it.
    /// A started worker is bounded by the HTTP timeout and must finish before
    /// another action can begin; this never claims to abort an in-flight fetch.
    #[allow(dead_code)] // Optional consent-dialog adapter; cancellation can precede scheduling.
    pub(crate) fn cancel_pending(&mut self, dialect: Dialect) -> bool {
        if self.pending.as_ref().is_some_and(|p| p.dialect == dialect) {
            self.pending = None;
            self.accept(dialect, Err("FIM schema installation cancelled".into()));
            return true;
        }
        false
    }

    fn schedule(&mut self, dialect: Dialect, work: Work, status: SchemaStatus) -> bool {
        if self.busy() {
            return false;
        }
        let entry = &mut self.entries[index(dialect)];
        entry.validator = None;
        entry.status = status;
        self.pending = Some(Pending { dialect, work });
        if let Some(ctx) = &self.context {
            // A consent-dialog click can enqueue after this frame's poll.
            ctx.request_repaint();
        }
        true
    }

    fn accept(&mut self, dialect: Dialect, result: Outcome) {
        let entry = &mut self.entries[index(dialect)];
        entry.validator = None;
        entry.status = match result {
            Ok(Some(validator)) => {
                entry.validator = Some(validator);
                SchemaStatus::Ready
            }
            Ok(None) => SchemaStatus::Unchecked,
            Err(error) => SchemaStatus::Unavailable(error),
        };
    }

    /// Start/receive background work. On wasm, HTTP is asynchronous; compilation
    /// uses the browser task queue (no native threads are available there).
    pub(crate) fn poll(&mut self, ctx: &egui::Context) {
        self.context = Some(ctx.clone());
        if let Some((dialect, receiver)) = &self.receiver {
            let result = match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err("FIM schema worker stopped before completion".into()))
                }
                Err(mpsc::TryRecvError::Empty) => None,
            };
            if let Some(result) = result {
                let dialect = *dialect;
                self.receiver = None;
                self.accept(dialect, result);
            }
        }
        let Some(pending) = self.pending.take() else {
            return;
        };
        let Some(root) = self.root.clone() else {
            self.accept(
                pending.dialect,
                Err("FIM schema storage is unavailable".into()),
            );
            return;
        };
        let (sender, receiver) = mpsc::channel();
        let dialect = pending.dialect;
        let context = ctx.clone();
        #[cfg(not(target_arch = "wasm32"))]
        {
            match std::thread::Builder::new()
                .name("fim-schemas".into())
                .spawn(move || {
                    let result = run_native(&root, pending);
                    let _ = sender.send(result);
                    context.request_repaint();
                }) {
                Ok(_) => self.receiver = Some((dialect, receiver)),
                Err(_) => self.accept(dialect, Err("Could not start FIM schema worker".into())),
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                let result = run_web(&root, pending).await;
                let _ = sender.send(result);
                context.request_repaint();
            });
            self.receiver = Some((dialect, receiver));
        }
    }
}

fn index(dialect: Dialect) -> usize {
    match dialect {
        Dialect::Xdf2 => 0,
        Dialect::Xdf3 => 1,
    }
}

fn cache_path(root: &Path, dialect: Dialect) -> PathBuf {
    let pins = official_downloads(dialect);
    // Both artifact hashes and validation policy participate in cache identity.
    // Never use a document's schemaLocation, supplied path, or server filename.
    root.join(format!(
        "{}-p{}-{}-{}.bundle",
        match dialect {
            Dialect::Xdf2 => "xdf2",
            Dialect::Xdf3 => "xdf3",
        },
        VALIDATION_POLICY_VERSION,
        pins[0].sha256,
        pins.get(1).map_or("none", |pin| pin.sha256),
    ))
}

fn size_check(exchange: &[u8], supplement: Option<&[u8]>) -> Result<(), String> {
    if exchange.len() > MAX_ARCHIVE_BYTES
        || supplement.is_some_and(|bytes| bytes.len() > MAX_ARCHIVE_BYTES)
    {
        return Err("FIM schema archive exceeds the 1 MiB safety limit".into());
    }
    Ok(())
}

fn compile(
    dialect: Dialect,
    exchange: &[u8],
    supplement: Option<&[u8]>,
) -> Result<Arc<Mutex<ExchangeValidator>>, String> {
    size_check(exchange, supplement)?;
    if dialect == Dialect::Xdf2 && supplement.is_some() {
        return Err("XDF2 installation accepts only its exchange schema archive".into());
    }
    ExchangeValidator::from_official_archives(dialect, exchange, supplement)
        .map(|validator| Arc::new(Mutex::new(validator)))
        .map_err(|error| match error {
            FimError::SchemaHash { .. } => format!(
                "FIM schema SHA-256 mismatch: {error}. The archive is corrupt or the publisher \
                 changed the pinned artifact. Nothing was installed; do not bypass this check."
            ),
            other => format!("FIM schema compilation unavailable: {other}"),
        })
}

fn encode_bundle(exchange: &[u8], supplement: Option<&[u8]>) -> Result<Vec<u8>, String> {
    size_check(exchange, supplement)?;
    let supplement = supplement.unwrap_or_default();
    let mut bytes = Vec::with_capacity(16 + exchange.len() + supplement.len());
    bytes.extend_from_slice(HEADER);
    bytes.extend_from_slice(&(exchange.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(supplement.len() as u32).to_le_bytes());
    bytes.extend_from_slice(exchange);
    bytes.extend_from_slice(supplement);
    Ok(bytes)
}

type Archives<'a> = (&'a [u8], Option<&'a [u8]>);

fn decode_bundle(bytes: &[u8]) -> Result<Archives<'_>, String> {
    if bytes.len() < 16 || bytes.len() > MAX_BUNDLE_BYTES || &bytes[..8] != HEADER {
        return Err("FIM schema cache has an invalid bundle header".into());
    }
    let first = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let second = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    if first > MAX_ARCHIVE_BYTES || second > MAX_ARCHIVE_BYTES || bytes.len() != 16 + first + second
    {
        return Err("FIM schema cache has invalid archive lengths".into());
    }
    Ok((
        &bytes[16..16 + first],
        (second != 0).then_some(&bytes[16 + first..]),
    ))
}

fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    #[cfg(not(target_arch = "wasm32"))]
    let bytes = {
        use std::io::Read;
        let file = std::fs::File::open(path)
            .map_err(|error| format!("FIM local archive/cache could not be opened: {error}"))?;
        let mut bytes = Vec::new();
        file.take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("FIM local archive/cache could not be read: {error}"))?;
        bytes
    };
    #[cfg(target_arch = "wasm32")]
    let bytes = {
        let metadata = thinoda_platform::vfs::global()
            .metadata(path)
            .map_err(|error| format!("FIM local archive/cache unavailable: {error}"))?;
        if metadata.len > limit as u64 {
            return Err("FIM local archive/cache exceeds the safety limit".into());
        }
        fs::read(path).map_err(|error| format!("FIM local archive/cache unreadable: {error}"))?
    };
    if bytes.len() > limit {
        return Err("FIM local archive/cache exceeds the safety limit".into());
    }
    Ok(bytes)
}

fn install(root: &Path, dialect: Dialect, exchange: &[u8], supplement: Option<&[u8]>) -> Outcome {
    // IO checks the exact hashes of BOTH archives before parsing or compilation.
    // No write is attempted until the entire trusted dependency closure compiles.
    let validator = compile(dialect, exchange, supplement)?;
    let bundle = encode_bundle(exchange, supplement)?;
    let path = cache_path(root, dialect);
    #[cfg(not(target_arch = "wasm32"))]
    fs::atomic_write(&path, &bundle, false)
        .map_err(|error| format!("FIM schema cache could not be saved: {error}"))?;
    #[cfg(target_arch = "wasm32")]
    persistent_vfs()
        .write(&path, &bundle, thinoda_platform::vfs::now_ms())
        .map_err(|error| format!("FIM schema cache could not be saved: {error}"))?;
    Ok(Some(validator))
}

#[cfg(target_arch = "wasm32")]
fn persistent_vfs() -> thinoda_platform::vfs::Vfs {
    // Unlike the application's document VFS, schema installation must never
    // silently use the in-memory fallback when browser persistence is blocked.
    thinoda_platform::vfs::Vfs::new(Box::new(thinoda_platform::vfs::LocalStorage))
}

fn read_cache(path: &Path) -> Result<Vec<u8>, String> {
    #[cfg(not(target_arch = "wasm32"))]
    return read_bounded(path, MAX_BUNDLE_BYTES);
    #[cfg(target_arch = "wasm32")]
    {
        let vfs = persistent_vfs();
        let meta = vfs
            .metadata(path)
            .map_err(|error| format!("FIM schema cache unavailable: {error}"))?;
        if meta.len > MAX_BUNDLE_BYTES as u64 {
            return Err("FIM schema cache exceeds the safety limit".into());
        }
        let bytes = vfs
            .read(path)
            .map_err(|error| format!("FIM schema cache unreadable: {error}"))?;
        if bytes.len() > MAX_BUNDLE_BYTES {
            return Err("FIM schema cache exceeds the safety limit".into());
        }
        Ok(bytes)
    }
}

fn remove_cache(path: &Path) -> std::io::Result<()> {
    #[cfg(not(target_arch = "wasm32"))]
    return fs::remove_file(path);
    #[cfg(target_arch = "wasm32")]
    {
        let vfs = persistent_vfs();
        vfs.remove_file(path)?;
        // KvStore::remove cannot report a denied localStorage operation.
        if vfs.exists(path) {
            return Err(std::io::Error::other(
                "browser schema cache removal was denied",
            ));
        }
        Ok(())
    }
}

fn run_local(root: &Path, dialect: Dialect, work: Work) -> Outcome {
    match work {
        Work::Cached => {
            let bytes = read_cache(&cache_path(root, dialect))?;
            let (exchange, supplement) = decode_bundle(&bytes)?;
            compile(dialect, exchange, supplement).map(Some)
        }
        Work::Bytes(exchange, supplement) => {
            install(root, dialect, &exchange, supplement.as_deref())
        }
        Work::Paths(exchange, supplement) => {
            let exchange = read_bounded(&exchange, MAX_ARCHIVE_BYTES)?;
            let supplement = supplement
                .as_deref()
                .map(|path| read_bounded(path, MAX_ARCHIVE_BYTES))
                .transpose()?;
            install(root, dialect, &exchange, supplement.as_deref())
        }
        Work::Remove => {
            match remove_cache(&cache_path(root, dialect)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(format!("FIM schema cache removal failed: {error}")),
            }
            Ok(None)
        }
        Work::Download => Err("Internal error: download requires the platform HTTP adapter".into()),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn run_native(root: &Path, pending: Pending) -> Outcome {
    if !matches!(pending.work, Work::Download) {
        return run_local(root, pending.dialect, pending.work);
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(60)))
        .max_redirects(0)
        .build()
        .into();
    let mut archives = Vec::new();
    for pin in official_downloads(pending.dialect) {
        let mut response = agent
            .get(pin.url)
            .header("Accept", "application/zip, application/octet-stream")
            .call()
            .map_err(|_| {
                format!(
                    "FIM schema download failed for {} (offline, proxy, timeout or HTTP error); \
                     use manual local installation if necessary",
                    pin.artifact
                )
            })?;
        if response.status() != ureq::http::StatusCode::OK {
            return Err(format!("Unexpected HTTP status for {}", pin.artifact));
        }
        let bytes = response
            .body_mut()
            .with_config()
            .limit(MAX_ARCHIVE_BYTES as u64)
            .read_to_vec()
            .map_err(|_| format!("FIM archive {} unreadable or exceeds 1 MiB", pin.artifact))?;
        archives.push(bytes);
    }
    install(
        root,
        pending.dialect,
        &archives[0],
        archives.get(1).map(Vec::as_slice),
    )
}

#[cfg(target_arch = "wasm32")]
async fn run_web(root: &Path, pending: Pending) -> Outcome {
    // Do not mistake the VFS's memory fallback for an offline installation.
    if !thinoda_platform::vfs::LocalStorage::available() {
        return Err("FIM schema storage is blocked by the browser".into());
    }
    // Use a dedicated persistent VFS rather than its possibly volatile global
    // fallback, selected earlier during application startup.
    if !matches!(pending.work, Work::Download) {
        return run_local(root, pending.dialect, pending.work);
    }
    let mut archives = Vec::new();
    for pin in official_downloads(pending.dialect) {
        archives.push(fetch_web_archive(pin.url, pin.artifact).await?);
    }
    install(
        root,
        pending.dialect,
        &archives[0],
        archives.get(1).map(Vec::as_slice),
    )
}

/// Keep the timeout alive through streaming, not just receipt of response headers.
#[cfg(target_arch = "wasm32")]
struct BrowserFetchGuard {
    window: web_sys::Window,
    controller: web_sys::AbortController,
    timer: i32,
    _callback: wasm_bindgen::closure::Closure<dyn FnMut()>,
}

#[cfg(target_arch = "wasm32")]
impl Drop for BrowserFetchGuard {
    fn drop(&mut self) {
        self.window.clear_timeout_with_handle(self.timer);
        // Also cancel a body stream on oversize, malformed response or cancellation.
        self.controller.abort();
    }
}

#[cfg(target_arch = "wasm32")]
fn schema_request_init(signal: &web_sys::AbortSignal) -> web_sys::RequestInit {
    let init = web_sys::RequestInit::new();
    init.set_method("GET");
    // Must reject BEFORE following Location; checking Response.url is too late.
    init.set_redirect(web_sys::RequestRedirect::Error);
    init.set_credentials(web_sys::RequestCredentials::Omit);
    init.set_mode(web_sys::RequestMode::Cors);
    init.set_signal(Some(signal));
    init
}

#[cfg(target_arch = "wasm32")]
async fn fetch_web_archive(url: &str, artifact: &str) -> Result<Vec<u8>, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    let failure = || {
        format!(
            "FIM schema download failed for {artifact} (offline, proxy, CORS, redirect or timeout); \
         use manual local installation if necessary"
        )
    };
    let window = web_sys::window().ok_or_else(failure)?;
    let controller = web_sys::AbortController::new().map_err(|_| failure())?;
    let abort = controller.clone();
    let callback = wasm_bindgen::closure::Closure::<dyn FnMut()>::new(move || abort.abort());
    let timer = window
        .set_timeout_with_callback_and_timeout_and_arguments_0(
            callback.as_ref().unchecked_ref(),
            60_000,
        )
        .map_err(|_| failure())?;
    let guard = BrowserFetchGuard {
        window,
        controller,
        timer,
        _callback: callback,
    };
    let init = schema_request_init(&guard.controller.signal());
    let request = web_sys::Request::new_with_str_and_init(url, &init).map_err(|_| failure())?;
    let response: web_sys::Response = JsFuture::from(guard.window.fetch_with_request(&request))
        .await
        .map_err(|_| failure())?
        .dyn_into()
        .map_err(|_| failure())?;
    if response.status() != 200 || response.redirected() || response.url() != url {
        return Err(format!("Unexpected HTTP status or URL for {artifact}"));
    }
    if response
        .headers()
        .get("Content-Length")
        .ok()
        .flatten()
        .and_then(|n| n.parse::<u64>().ok())
        .is_some_and(|n| n > MAX_ARCHIVE_BYTES as u64)
    {
        return Err(format!("FIM archive {artifact} exceeds 1 MiB"));
    }
    let body = response.body().ok_or_else(failure)?;
    let reader: web_sys::ReadableStreamDefaultReader =
        body.get_reader().dyn_into().map_err(|_| failure())?;
    let mut bytes = Vec::new();
    loop {
        let chunk = JsFuture::from(reader.read()).await.map_err(|_| failure())?;
        let done = js_sys::Reflect::get(&chunk, &"done".into()).map_err(|_| failure())?;
        if done.as_bool() == Some(true) {
            break;
        }
        let value = js_sys::Reflect::get(&chunk, &"value".into()).map_err(|_| failure())?;
        let chunk: js_sys::Uint8Array = value.dyn_into().map_err(|_| failure())?;
        if bytes.len().saturating_add(chunk.length() as usize) > MAX_ARCHIVE_BYTES {
            return Err(format!("FIM archive {artifact} exceeds 1 MiB"));
        }
        bytes.extend_from_slice(&chunk.to_vec());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_request_rejects_redirects_before_fetch_and_omits_credentials() {
        // Source-level regression: native tests cannot construct browser JS
        // objects. Check the actual request factory and its use by Fetch rather
        // than mistaking a post-response URL comparison for redirect prevention.
        let source = include_str!("fim_schemas.rs");
        let factory = source
            .split("fn schema_request_init(")
            .nth(1)
            .unwrap()
            .split("async fn fetch_web_archive(")
            .next()
            .unwrap();
        assert!(factory.contains("init.set_redirect(web_sys::RequestRedirect::Error)"));
        assert!(factory.contains("init.set_credentials(web_sys::RequestCredentials::Omit)"));
        assert!(factory.contains("init.set_signal(Some(signal))"));
        let fetch = source
            .split("async fn fetch_web_archive(")
            .nth(1)
            .unwrap()
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(fetch.contains("schema_request_init(&guard.controller.signal())"));
        assert!(fetch.contains("Request::new_with_str_and_init(url, &init)"));
        assert!(fetch.contains("fetch_with_request(&request)"));
        assert!(fetch.contains("60_000"));
        assert!(fetch.contains("MAX_ARCHIVE_BYTES"));
    }

    #[test]
    fn cache_identity_covers_both_dialects_all_pins_and_policy() {
        let root = Path::new("schemas");
        let two = cache_path(root, Dialect::Xdf2);
        let three = cache_path(root, Dialect::Xdf3);
        assert_ne!(two, three);
        for dialect in [Dialect::Xdf2, Dialect::Xdf3] {
            let path = cache_path(root, dialect).to_string_lossy().into_owned();
            assert!(path.contains(&format!("-p{}-", VALIDATION_POLICY_VERSION)));
            for pin in official_downloads(dialect) {
                assert!(path.contains(pin.sha256));
            }
        }
    }

    #[test]
    fn bundle_lengths_are_exact_and_bounded() {
        for supplement in [None, Some(b"examples".as_slice())] {
            let bytes = encode_bundle(b"exchange", supplement).unwrap();
            assert_eq!(
                decode_bundle(&bytes).unwrap(),
                (b"exchange".as_slice(), supplement)
            );
            let mut trailing = bytes.clone();
            trailing.push(0);
            assert!(decode_bundle(&trailing).is_err());
            assert!(decode_bundle(&bytes[..bytes.len() - 1]).is_err());
        }
        assert!(decode_bundle(b"FIMZIP01").is_err());
        let mut bad = encode_bundle(b"", None).unwrap();
        bad[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_bundle(&bad).is_err());
    }

    #[test]
    fn bad_hash_never_creates_or_overwrites_a_cache() {
        let temp = tempfile::tempdir().unwrap();
        let path = cache_path(temp.path(), Dialect::Xdf3);
        let error = install(temp.path(), Dialect::Xdf3, b"changed", Some(b"changed"))
            .err()
            .unwrap();
        assert!(error.contains("SHA-256 mismatch"));
        assert!(!path.exists());
        fs::atomic_write(&path, b"previous", false).unwrap();
        assert!(install(temp.path(), Dialect::Xdf3, b"changed", Some(b"changed")).is_err());
        assert_eq!(fs::read(path).unwrap(), b"previous");
    }

    #[test]
    fn local_check_and_manual_install_never_become_downloads() {
        let paths = Paths::from_root("unused");
        let mut state = FimSchemas::new(Some(&paths));
        assert!(state.ensure_cached(Dialect::Xdf2));
        assert!(matches!(state.pending.as_ref().unwrap().work, Work::Cached));
        assert_eq!(state.status(Dialect::Xdf2), &SchemaStatus::Checking);
        assert!(!state.request_download(Dialect::Xdf3));
        state.pending = None;
        state.accept(Dialect::Xdf2, Err("missing cache".into()));
        assert!(state.validator(Dialect::Xdf2).is_none());
        assert!(!state.ensure_cached(Dialect::Xdf2));
        assert!(state.install_bytes(Dialect::Xdf2, b"local".to_vec(), None));
        assert!(matches!(
            state.pending.as_ref().unwrap().work,
            Work::Bytes(_, _)
        ));
    }

    #[test]
    fn disabled_storage_stays_unavailable_without_starting_http() {
        let mut state = FimSchemas::default();
        assert!(state.request_download(Dialect::Xdf2));
        state.poll(&egui::Context::default());
        assert!(!state.busy());
        assert!(matches!(
            state.status(Dialect::Xdf2),
            SchemaStatus::Unavailable(_)
        ));
        assert!(state.validator_handle(Dialect::Xdf2).is_none());
    }

    #[test]
    fn cancellation_before_poll_cannot_start_a_download() {
        let paths = Paths::from_root("unused");
        let mut state = FimSchemas::new(Some(&paths));
        assert!(state.request_download(Dialect::Xdf3));
        assert!(!state.cancel_pending(Dialect::Xdf2));
        assert!(state.cancel_pending(Dialect::Xdf3));
        state.poll(&egui::Context::default());
        assert!(!state.busy());
        assert!(matches!(
            state.status(Dialect::Xdf3),
            SchemaStatus::Unavailable(error) if error.contains("cancelled")
        ));
    }

    #[test]
    fn missing_corrupt_and_oversized_local_cache_are_unavailable() {
        let temp = tempfile::tempdir().unwrap();
        assert!(run_local(temp.path(), Dialect::Xdf2, Work::Cached).is_err());
        let path = cache_path(temp.path(), Dialect::Xdf2);
        fs::atomic_write(&path, b"not a bundle", false).unwrap();
        assert!(run_local(temp.path(), Dialect::Xdf2, Work::Cached).is_err());
        fs::atomic_write(&path, &vec![0; MAX_BUNDLE_BYTES + 1], false).unwrap();
        assert!(run_local(temp.path(), Dialect::Xdf2, Work::Cached).is_err());
        assert!(
            run_local(temp.path(), Dialect::Xdf2, Work::Remove)
                .unwrap()
                .is_none()
        );
        assert!(!path.exists());
    }

    #[test]
    fn archive_limits_are_enforced_before_compilation_or_persistence() {
        let temp = tempfile::tempdir().unwrap();
        let huge = vec![0; MAX_ARCHIVE_BYTES + 1];
        let error = install(temp.path(), Dialect::Xdf2, &huge, None)
            .err()
            .unwrap();
        assert!(error.contains("safety limit"));
        assert!(!cache_path(temp.path(), Dialect::Xdf2).exists());
        assert!(compile(Dialect::Xdf2, b"zip", Some(b"unexpected")).is_err());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn manual_path_worker_reports_failure_then_explicit_removal_resets_status() {
        let temp = tempfile::tempdir().unwrap();
        let paths = Paths::from_root(temp.path());
        let archive = temp.path().join("selected.zip");
        fs::atomic_write(&archive, b"not official", false).unwrap();
        let mut state = FimSchemas::new(Some(&paths));
        assert!(state.install_paths(Dialect::Xdf2, archive, None));
        assert_eq!(state.status(Dialect::Xdf2), &SchemaStatus::Installing);
        let ctx = egui::Context::default();
        let finish = |state: &mut FimSchemas| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while state.busy() && std::time::Instant::now() < deadline {
                state.poll(&ctx);
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            assert!(!state.busy());
        };
        finish(&mut state);
        assert!(matches!(
            state.status(Dialect::Xdf2),
            SchemaStatus::Unavailable(error) if error.contains("SHA-256 mismatch")
        ));
        assert!(state.validator(Dialect::Xdf2).is_none());
        assert_eq!(state.status(Dialect::Xdf3), &SchemaStatus::Unchecked);
        assert!(state.remove_cached(Dialect::Xdf2));
        finish(&mut state);
        assert_eq!(state.status(Dialect::Xdf2), &SchemaStatus::Unchecked);
    }
}
