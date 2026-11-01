//! File pickers, save targets and deliveries.
//!
//! Desktop builds use the native dialogs of `rfd` and write straight to disk. In the
//! browser build (`crates/thinoda-web`) uploaded documents are stored in durable
//! origin-private storage (`/Documents`, OPFS or IndexedDB through
//! [`thinoda_platform::browser_storage::BrowserStorageProvider`]). Images,
//! attachments, merge/schema inputs and export scratch files stay in the synchronous
//! virtual file system (VFS, `localStorage`). Exports and explicit saves are
//! delivered as browser downloads; files picked with the File System Access API can
//! additionally be saved back with [`save_back_picked`].

use std::{
    io,
    path::{Path, PathBuf},
};

use thinoda_core::Workbook;
use thinoda_platform::fs as pfs;

/// VFS folder holding browser documents (survives reloads).
pub const WEB_DOCUMENTS: &str = "/Documents";
/// VFS scratch folder for downloads; files are removed after delivery.
pub const WEB_DOWNLOADS: &str = "/Downloads";
/// VFS folder for uploads read synchronously (merge sources, FIM schema archives).
pub const WEB_UPLOAD_INPUTS: &str = "/UploadInputs";
/// VFS folder for dropped images; the app removes them after insertion.
pub const WEB_DROPPED_IMAGES: &str = "/ImageUploads/dropped";

pub use thinoda_platform::browser_storage::{MigrationConflict, MigrationReport};

thread_local! {
    static BROWSER_MIGRATION: std::cell::RefCell<Option<MigrationReport>> =
        const { std::cell::RefCell::new(None) };
}

/// Web entry hook: stores the verified startup migration result before the app
/// starts. Startup does not run at all when migration failed.
pub fn record_browser_migration(report: MigrationReport) {
    BROWSER_MIGRATION.with(|slot| *slot.borrow_mut() = Some(report));
}

/// The startup migration report, once. Conflicts list legacy documents whose
/// different bytes were preserved as copies next to a newer durable original;
/// the app must tell the user about them.
pub fn take_browser_migration() -> Option<MigrationReport> {
    BROWSER_MIGRATION.with(|slot| slot.borrow_mut().take())
}

/// Whether `path` is a durable browser document (not VFS).
pub fn is_durable_document(path: &Path) -> bool {
    is_web() && path.starts_with(WEB_DOCUMENTS) && path != Path::new(WEB_DOCUMENTS)
}

/// Whether a document path currently exists, without I/O for durable browser
/// documents: those use the metadata index filled at startup and by commits,
/// other paths the synchronous VFS. Opening still reads the real storage.
pub fn document_exists(path: &Path) -> bool {
    thinoda_platform::browser_storage::known_document(path).unwrap_or_else(|| pfs::exists(path))
}

/// Durable browser document identifier for a `/Documents/...` path.
pub fn document_id(path: &Path) -> thinoda_platform::storage::StorageId {
    thinoda_platform::storage::StorageId(path.to_string_lossy().replace('\\', "/"))
}

/// What the user picked files for. On the web, pickers finish asynchronously and
/// the app dispatches the result by purpose in its housekeeping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    Open,
    Import,
    Merge,
    Dropped,
    Image(u64),
    Attachment(u64),
    FimSchemas,
}

/// Files that finished uploading into the browser VFS.
#[derive(Debug)]
pub struct Picked {
    pub purpose: Purpose,
    pub paths: Vec<PathBuf>,
    pub errors: Vec<String>,
    pub(crate) image_owner: Option<crate::images::ImageRequest>,
}

/// Where a save dialog result is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// A map document that stays openable (recent files, autosave).
    Document,
    /// A one-off export that is downloaded and then removed from browser storage.
    Download,
}

pub type Filter<'a> = (&'a str, &'a [&'a str]);

/// Whether this build runs in a browser.
pub const fn is_web() -> bool {
    cfg!(target_arch = "wasm32")
}

/// Shows an open dialog. Returns the picked files on desktop; on the web the
/// upload completes later and is returned by [`take_picked`].
pub fn pick_files(
    purpose: Purpose,
    title: Option<&str>,
    filters: &[Filter<'_>],
    directory: Option<&Path>,
    multiple: bool,
) -> Option<Vec<PathBuf>> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = purpose;
        let mut dialog = rfd::FileDialog::new();
        if let Some(title) = title {
            dialog = dialog.set_title(title);
        }
        for (name, extensions) in filters {
            dialog = dialog.add_filter(*name, extensions);
        }
        if let Some(directory) = directory {
            dialog = dialog.set_directory(directory);
        }
        if multiple {
            dialog.pick_files()
        } else {
            dialog.pick_file().map(|path| vec![path])
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = directory;
        web::pick_files(purpose, title, filters, multiple);
        None
    }
}

/// Asks for a save destination. Desktop: native save dialog. Web: a file name
/// prompt; the path points into the browser VFS.
pub fn save_target(
    title: Option<&str>,
    filters: &[Filter<'_>],
    file_name: &str,
    directory: Option<&Path>,
    target: Target,
) -> Option<PathBuf> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = target;
        let mut dialog = rfd::FileDialog::new();
        if let Some(title) = title {
            dialog = dialog.set_title(title);
        }
        for (name, extensions) in filters {
            dialog = dialog.add_filter(*name, extensions);
        }
        dialog = dialog.set_file_name(file_name);
        if let Some(directory) = directory {
            dialog = dialog.set_directory(directory);
        }
        dialog.save_file()
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = directory;
        let allowed: Vec<&str> = filters
            .iter()
            .flat_map(|(_, extensions)| extensions.iter().copied())
            .collect();
        let message = title.unwrap_or("File name");
        let input = thinoda_platform::web::prompt_file_name(message, file_name)?;
        let name = web_save_name(&input, &allowed)?;
        let folder = match target {
            Target::Document => WEB_DOCUMENTS,
            Target::Download => WEB_DOWNLOADS,
        };
        Some(Path::new(folder).join(name))
    }
}

/// Hands a written file to the user. Desktop: no-op (it is already on disk).
/// Web: downloads it; scratch exports are removed from browser storage afterwards.
pub fn deliver(path: &Path) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        if is_durable_document(path) {
            web::deliver_durable(path.to_path_buf());
            return Ok(());
        }
        let bytes = pfs::read(path).map_err(|e| e.to_string())?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "download".into());
        let result = thinoda_platform::web::download(&name, &bytes);
        if is_scratch(path) {
            let _ = pfs::remove_file(path);
        }

        result
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = path;
        Ok(())
    }
}

/// Same VFS delivery/cleanup as exports, but never infer an active MIME type
/// (HTML/SVG, for example) from an untrusted attachment filename.
pub fn deliver_attachment(path: &Path) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        let bytes = pfs::read(path).map_err(|e| e.to_string())?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("attachment");
        let name = thinoda_core::attachments::safe_name(name);
        let result = thinoda_platform::web::download_opaque(&name, &bytes);
        if is_scratch(path) {
            let _ = pfs::remove_file(path);
        }
        result
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        deliver(path)
    }
}

/// Whether `path` is a temporary web download that is not kept.
pub fn is_scratch(path: &Path) -> bool {
    path.starts_with(WEB_DOWNLOADS)
}

/// Failures of asynchronous durable-document downloads started by [`deliver`]
/// (always empty on desktop). Callers must surface them; [`deliver`] only
/// reports that the download was started.
pub fn take_delivery_errors() -> Vec<String> {
    #[cfg(target_arch = "wasm32")]
    {
        web::take_delivery_errors()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Vec::new()
    }
}

/// Whether `path` was opened from a file explicitly picked with the File System
/// Access API in this tab, so [`save_back_picked`] can write it back.
pub fn has_picked_file(path: &Path) -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        web::has_picked_file(path)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = path;
        false
    }
}

/// Writes `bytes` (normally the just-saved durable document) back to the file the
/// user picked for `path`. Fails with `Conflict` when that file changed since it
/// was read or last saved, and with `Unsupported` when no picked handle exists.
/// Never falls back to a download.
pub async fn save_back_picked(
    path: &Path,
    bytes: &[u8],
    cancel: thinoda_platform::storage::CancelToken,
) -> Result<thinoda_platform::storage::Revision, thinoda_platform::storage::StorageError> {
    #[cfg(target_arch = "wasm32")]
    {
        web::save_back_picked(path, bytes, cancel).await
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (path, bytes, cancel);
        Err(thinoda_platform::storage::StorageError::Unsupported)
    }
}

/// Releases a picked-file handle, for example when its document closes.
pub fn forget_picked(path: &Path) {
    #[cfg(target_arch = "wasm32")]
    web::forget_picked(path);
    #[cfg(not(target_arch = "wasm32"))]
    let _ = path;
}

/// Uploads that finished since the last call (always empty on desktop).
pub fn take_picked() -> Vec<Picked> {
    #[cfg(target_arch = "wasm32")]
    {
        web::take_picked()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Vec::new()
    }
}

/// Lets asynchronous web uploads wake the UI when they finish.
pub fn set_repaint_context(ctx: &egui::Context) {
    #[cfg(target_arch = "wasm32")]
    web::set_context(ctx);
    #[cfg(not(target_arch = "wasm32"))]
    let _ = ctx;
}

/// Files dropped onto the window. Desktop: their paths. Web: browsers only expose
/// contents, which are copied into the VFS and returned later by [`take_picked`].
pub(crate) fn dropped_files(
    ctx: &egui::Context,
    image_owner: Option<crate::images::ImageRequest>,
) -> Vec<(PathBuf, Option<crate::images::ImageRequest>)> {
    let files = ctx.input(|i| i.raw.dropped_files.clone());
    #[cfg(target_arch = "wasm32")]
    {
        if !files.is_empty() {
            web::store_dropped(files, image_owner);
        }
        Vec::new()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        files
            .iter()
            .map(|file| (file.path().to_path_buf(), image_owner.clone()))
            .collect()
    }
}

/// Imports a workbook from a desktop path or a browser VFS path.
pub fn import_workbook(path: &Path) -> Result<Workbook, thinoda_io::IoError> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        thinoda_io::import_file(path)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let bytes = pfs::read(path)?;
        thinoda_io::import_named_bytes(path, &bytes)
    }
}

/// Normalizes a file name typed into the browser save prompt. Keeps an allowed
/// extension the user typed, otherwise appends the first allowed one.
pub fn web_save_name(input: &str, allowed: &[&str]) -> Option<String> {
    let default = allowed.first().copied().unwrap_or_default();
    let keeps = input
        .trim()
        .rsplit_once('.')
        .is_some_and(|(_, ext)| allowed.iter().any(|a| a.eq_ignore_ascii_case(ext)));
    thinoda_platform::web::sanitize_file_name(input, if keeps { "" } else { default })
}

/// Safe upload names in deduplication order: `name`, then `stem (2).ext` … `(999)`.
pub fn upload_candidates(name: &str) -> io::Result<impl Iterator<Item = String>> {
    let file = thinoda_platform::web::sanitize_file_name(name, "")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid file name"))?;
    let (stem, extension) = match file.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem.to_owned(), format!(".{ext}")),
        _ => (file.clone(), String::new()),
    };
    Ok((1..1000).map(move |n| {
        if n == 1 {
            file.clone()
        } else {
            format!("{stem} ({n}){extension}")
        }
    }))
}

/// Copies uploaded `bytes` into `dir` under a safe, unused name. An identical
/// existing file is reused so re-opening the same upload does not pile up copies.
pub fn store_upload_in(dir: &Path, name: &str, bytes: &[u8]) -> io::Result<PathBuf> {
    for candidate in upload_candidates(name)? {
        let path = dir.join(candidate);
        if !pfs::exists(&path) {
            pfs::atomic_write(&path, bytes, false)?;
            return Ok(path);
        }
        if pfs::read(&path).is_ok_and(|existing| existing == bytes) {
            return Ok(path);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "too many files with this name",
    ))
}

#[cfg(target_arch = "wasm32")]
mod web {
    use super::{
        Filter, Picked, Purpose, WEB_DOCUMENTS, WEB_DROPPED_IMAGES, WEB_UPLOAD_INPUTS, document_id,
        store_upload_in, upload_candidates,
    };
    use std::{
        cell::RefCell,
        collections::HashMap,
        path::{Path, PathBuf},
    };
    use thinoda_platform::{
        browser_storage::{self as bs, BrowserStorageProvider},
        storage::{CancelToken, ExpectedRevision, Revision, StorageError, StorageProvider},
    };

    /// A file explicitly picked with the File System Access API for this tab.
    struct PickedHandle {
        session: String,
        revision: Revision,
    }

    thread_local! {
        static INBOX: RefCell<Vec<Picked>> = const { RefCell::new(Vec::new()) };
        static CONTEXT: RefCell<Option<egui::Context>> = const { RefCell::new(None) };
        static DELIVERY_ERRORS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
        static HANDLES: RefCell<HashMap<PathBuf, PickedHandle>> = RefCell::new(HashMap::new());
    }

    pub(super) fn set_context(ctx: &egui::Context) {
        CONTEXT.with(|c| {
            if c.borrow().is_none() {
                *c.borrow_mut() = Some(ctx.clone());
            }
        });
    }

    fn repaint() {
        CONTEXT.with(|c| {
            if let Some(ctx) = c.borrow().as_ref() {
                ctx.request_repaint();
            }
        });
    }

    pub(super) fn take_picked() -> Vec<Picked> {
        INBOX.with(|inbox| std::mem::take(&mut *inbox.borrow_mut()))
    }

    pub(super) fn take_delivery_errors() -> Vec<String> {
        DELIVERY_ERRORS.with(|errors| std::mem::take(&mut *errors.borrow_mut()))
    }

    fn deliver(picked: Picked) {
        if picked.paths.is_empty()
            && picked.errors.is_empty()
            && !matches!(picked.purpose, Purpose::Image(_) | Purpose::Attachment(_))
        {
            return;
        }
        INBOX.with(|inbox| inbox.borrow_mut().push(picked));
        repaint();
    }

    /// Reads a durable document and downloads it; failures are queued for
    /// [`super::take_delivery_errors`] instead of being reported as success.
    pub(super) fn deliver_durable(path: PathBuf) {
        wasm_bindgen_futures::spawn_local(async move {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "download".into());
            let result = BrowserStorageProvider::default()
                .read(&document_id(&path), CancelToken::default())
                .await
                .map_err(|e| e.to_string())
                .and_then(|read| thinoda_platform::web::download(&name, &read.bytes));
            if let Err(error) = result {
                log::warn!("durable document download failed: {error}");
                DELIVERY_ERRORS.with(|errors| errors.borrow_mut().push(format!("{name}: {error}")));
                repaint();
            }
        });
    }

    /// Stores an uploaded document in durable browser storage under a unique
    /// name. An identical existing document is reused; a different one is never
    /// overwritten (conditional `Missing` write, then the next ` (n)` name).
    async fn store_document(name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
        let provider = BrowserStorageProvider::default();
        for candidate in upload_candidates(name).map_err(|e| e.to_string())? {
            let path = Path::new(WEB_DOCUMENTS).join(candidate);
            let id = document_id(&path);
            match provider
                .write(
                    &id,
                    bytes,
                    ExpectedRevision::Missing,
                    CancelToken::default(),
                )
                .await
            {
                Ok(_) => return Ok(path),
                Err(StorageError::Conflict { .. }) => {
                    let existing = provider
                        .read(&id, CancelToken::default())
                        .await
                        .map_err(|e| e.to_string())?;
                    if existing.bytes == bytes {
                        return Ok(path);
                    }
                }
                Err(error) => return Err(error.to_string()),
            }
        }
        Err("too many files with this name".into())
    }

    /// Where a non-document upload is kept in the synchronous VFS, or `None`
    /// for documents that go to durable storage.
    fn vfs_directory(purpose: Purpose, name: &str) -> Option<String> {
        match purpose {
            Purpose::Image(request) => Some(format!("/ImageUploads/{request}")),
            Purpose::Attachment(request) => Some(format!("/AttachmentUploads/{request}")),
            // Read synchronously by merge and schema installation.
            Purpose::Merge | Purpose::FimSchemas => Some(WEB_UPLOAD_INPUTS.into()),
            // The app inserts and then removes dropped images through the VFS.
            Purpose::Dropped if crate::images::is_supported_image_path(Path::new(name)) => {
                Some(WEB_DROPPED_IMAGES.into())
            }
            Purpose::Open | Purpose::Import | Purpose::Dropped => None,
        }
    }

    async fn store(picked: &mut Picked, name: &str, bytes: Result<Vec<u8>, String>) {
        let result = match bytes {
            Err(error) => Err(error),
            Ok(bytes) => {
                let bounded_image_drop = matches!(picked.purpose, Purpose::Dropped)
                    && crate::images::is_supported_image_path(Path::new(name));
                if (matches!(picked.purpose, Purpose::Image(_) | Purpose::Attachment(_))
                    || bounded_image_drop)
                    && bytes.len() > thinoda_core::resources::MAX_RESOURCE_BYTES
                {
                    Err("local resource exceeds 8 MiB".into())
                } else {
                    let name = if matches!(picked.purpose, Purpose::Attachment(_)) {
                        thinoda_core::attachments::safe_name(name)
                    } else {
                        name.to_owned()
                    };
                    match vfs_directory(picked.purpose, &name) {
                        Some(directory) => store_upload_in(Path::new(&directory), &name, &bytes)
                            .map_err(|e| e.to_string()),
                        None => store_document(&name, &bytes).await,
                    }
                }
            }
        };
        match result {
            Ok(path) => picked.paths.push(path),
            Err(error) => picked.errors.push(format!("{name}: {error}")),
        }
    }

    pub(super) fn pick_files(
        purpose: Purpose,
        title: Option<&str>,
        filters: &[Filter<'_>],
        multiple: bool,
    ) {
        // Open with File System Access keeps a writable handle for save-back.
        // The picker is started first, inside the user gesture.
        if purpose == Purpose::Open && bs::file_system_access_supported() {
            wasm_bindgen_futures::spawn_local(open_with_handle());
            return;
        }
        let mut dialog = rfd::AsyncFileDialog::new();
        if let Some(title) = title {
            dialog = dialog.set_title(title);
        }
        for (name, extensions) in filters {
            dialog = dialog.add_filter(*name, extensions);
        }
        wasm_bindgen_futures::spawn_local(async move {
            let handles = if multiple {
                dialog.pick_files().await.unwrap_or_default()
            } else {
                dialog.pick_file().await.into_iter().collect()
            };
            let mut picked = Picked {
                purpose,
                paths: Vec::new(),
                errors: Vec::new(),
                image_owner: None,
            };
            for handle in handles {
                if matches!(purpose, Purpose::Image(_) | Purpose::Attachment(_))
                    && handle.inner().size() > thinoda_core::resources::MAX_RESOURCE_BYTES as f64
                {
                    picked.errors.push("local resource exceeds 8 MiB".into());
                    continue;
                }
                let bytes = handle.read().await;
                store(&mut picked, &handle.file_name(), Ok(bytes)).await;
            }
            deliver(picked);
        });
    }

    async fn open_with_handle() {
        let mut picked = Picked {
            purpose: Purpose::Open,
            paths: Vec::new(),
            errors: Vec::new(),
            image_owner: None,
        };
        match bs::open_picked_file().await {
            Err(StorageError::Cancelled) => {}
            Err(error) => picked.errors.push(error.to_string()),
            Ok(file) => match store_document(&file.name, &file.content.bytes).await {
                Ok(path) => {
                    let previous = HANDLES.with(|handles| {
                        handles.borrow_mut().insert(
                            path.clone(),
                            PickedHandle {
                                session: file.session,
                                revision: file.content.revision,
                            },
                        )
                    });
                    if let Some(previous) = previous {
                        bs::forget_picked_file(&previous.session);
                    }
                    picked.paths.push(path);
                }
                Err(error) => {
                    bs::forget_picked_file(&file.session);
                    picked.errors.push(format!("{}: {error}", file.name));
                }
            },
        }
        deliver(picked);
    }

    pub(super) fn has_picked_file(path: &Path) -> bool {
        HANDLES.with(|handles| handles.borrow().contains_key(path))
    }

    pub(super) async fn save_back_picked(
        path: &Path,
        bytes: &[u8],
        cancel: CancelToken,
    ) -> Result<Revision, StorageError> {
        let (session, expected) = HANDLES
            .with(|handles| {
                handles
                    .borrow()
                    .get(path)
                    .map(|h| (h.session.clone(), h.revision.clone()))
            })
            .ok_or(StorageError::Unsupported)?;
        let revision =
            bs::write_picked_file(&session, bytes, ExpectedRevision::Match(expected), cancel)
                .await?;
        HANDLES.with(|handles| {
            if let Some(handle) = handles.borrow_mut().get_mut(path)
                && handle.session == session
            {
                handle.revision = revision.clone();
            }
        });
        Ok(revision)
    }

    pub(super) fn forget_picked(path: &Path) {
        if let Some(handle) = HANDLES.with(|handles| handles.borrow_mut().remove(path)) {
            bs::forget_picked_file(&handle.session);
        }
    }

    pub(super) fn store_dropped(
        files: Vec<egui::DroppedFileHandle>,
        image_owner: Option<crate::images::ImageRequest>,
    ) {
        wasm_bindgen_futures::spawn_local(async move {
            let mut picked = Picked {
                purpose: Purpose::Dropped,
                paths: Vec::new(),
                errors: Vec::new(),
                image_owner,
            };
            for file in files {
                let name = file.path().to_string_lossy().into_owned();
                let bytes = file.bytes_async().await;
                store(&mut picked, &name, bytes).await;
            }
            deliver(picked);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_save_names_keep_allowed_extensions_and_strip_paths() {
        let allowed = ["thinoda", "xmind", "mmap"];
        assert_eq!(
            web_save_name("Plan", &allowed).as_deref(),
            Some("Plan.thinoda")
        );
        assert_eq!(
            web_save_name("Plan.XMIND", &allowed).as_deref(),
            Some("Plan.XMIND")
        );
        assert_eq!(
            web_save_name("../a/b:c.txt", &allowed).as_deref(),
            Some("bc.txt.thinoda")
        );
        assert_eq!(web_save_name("  ..  ", &allowed), None);
        assert_eq!(web_save_name("shot", &["png"]).as_deref(), Some("shot.png"));
    }

    #[test]
    fn uploads_get_unique_names_but_identical_reuploads_are_reused() {
        let dir = tempfile::tempdir().unwrap();
        let a = store_upload_in(dir.path(), "map.thinoda", b"one").unwrap();
        assert_eq!(a, dir.path().join("map.thinoda"));
        assert_eq!(
            store_upload_in(dir.path(), "map.thinoda", b"one").unwrap(),
            a
        );
        let b = store_upload_in(dir.path(), "map.thinoda", b"two").unwrap();
        assert_eq!(b, dir.path().join("map (2).thinoda"));
        assert_eq!(std::fs::read(&b).unwrap(), b"two");
        let c = store_upload_in(dir.path(), "C:\\x\\README", b"r").unwrap();
        assert_eq!(c, dir.path().join("README"));
        assert!(store_upload_in(dir.path(), "...", b"").is_err());
    }

    #[test]
    fn upload_candidates_deduplicate_in_order() {
        let names: Vec<_> = upload_candidates("dir/map.thinoda")
            .unwrap()
            .take(3)
            .collect();
        assert_eq!(names, ["map.thinoda", "map (2).thinoda", "map (3).thinoda"]);
        assert_eq!(
            upload_candidates("README").unwrap().nth(1).unwrap(),
            "README (2)"
        );
        assert_eq!(upload_candidates("a").unwrap().count(), 999);
        assert!(upload_candidates("..").is_err());
    }

    #[test]
    fn migration_report_hook_is_taken_once() {
        record_browser_migration(MigrationReport {
            backend: "opfs".into(),
            verified: 1,
            conflicts: vec![MigrationConflict {
                original: "/Documents/a".into(),
                preserved: "/Documents/.thinoda-legacy/x/y/a".into(),
            }],
        });
        assert_eq!(take_browser_migration().unwrap().conflicts.len(), 1);
        assert!(take_browser_migration().is_none());
    }

    #[test]
    fn desktop_builds_have_no_web_side_effects() {
        assert!(take_delivery_errors().is_empty());
        assert!(!has_picked_file(Path::new("/Documents/x.thinoda")));
        assert!(!is_durable_document(Path::new("/Documents/x.thinoda")));
        assert_eq!(document_id(Path::new("/Documents/x")).0, "/Documents/x");
        assert!(!is_web());
        assert!(take_picked().is_empty());
        assert!(deliver(Path::new("/nonexistent")).is_ok());
        assert!(is_scratch(Path::new("/Downloads/x.png")));
        assert!(!is_scratch(Path::new("/Documents/x.thinoda")));
    }
}
