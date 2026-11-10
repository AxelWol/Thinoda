//! File pickers, save targets and deliveries.
//!
//! Desktop builds use the native dialogs of `rfd` and write straight to disk. In the
//! browser build (`crates/thinoda-web`) uploaded documents are stored in durable
//! origin-private storage (`/Documents`, OPFS or IndexedDB through
//! [`thinoda_platform::browser_storage::BrowserStorageProvider`]). Attachments,
//! merge/schema inputs and export scratch files stay in the synchronous virtual
//! file system (VFS, `localStorage`); image picker and dropped image uploads stay
//! in memory. Exports and explicit saves are delivered as browser downloads; files
//! picked with the File System Access API can additionally be saved back with
//! [`save_back_picked`].

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
    StencilLibrary,
}

/// A bounded image picker upload, owned only until the app consumes the result.
#[derive(Debug)]
pub(crate) struct ImageUpload {
    pub(crate) name: String,
    pub(crate) bytes: Vec<u8>,
}

impl ImageUpload {
    #[cfg(any(target_arch = "wasm32", test))]
    fn new(name: &str, bytes: Vec<u8>) -> Result<Self, String> {
        if bytes.len() > thinoda_core::resources::MAX_RESOURCE_BYTES {
            return Err("local resource exceeds 8 MiB".into());
        }
        let name = thinoda_platform::web::sanitize_file_name(name, "")
            .ok_or_else(|| "invalid file name".to_owned())?;
        Ok(Self { name, bytes })
    }
}

/// Browser drops keep image bytes and document paths in their original order.
// Native `take_picked` is always empty; the variants are built by web staging.
#[cfg_attr(not(any(target_arch = "wasm32", test)), expect(dead_code))]
#[derive(Debug)]
pub(crate) enum DroppedUpload {
    Image(ImageUpload),
    Document(PathBuf),
}

/// Files or image bytes that finished uploading in the browser.
#[derive(Debug)]
pub struct Picked {
    pub purpose: Purpose,
    pub paths: Vec<PathBuf>,
    pub errors: Vec<String>,
    pub(crate) image_owner: Option<crate::images::ImageRequest>,
    pub(crate) image_upload: Option<ImageUpload>,
    pub(crate) dropped_uploads: Vec<DroppedUpload>,
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

#[cfg(any(target_arch = "wasm32", test))]
#[derive(Debug)]
struct PickedRevision {
    saved: thinoda_platform::storage::Revision,
    conflict: Option<thinoda_platform::storage::Revision>,
}

#[cfg(any(target_arch = "wasm32", test))]
impl PickedRevision {
    fn expected(&self, overwrite_approved: bool) -> thinoda_platform::storage::ExpectedRevision {
        let revision = if overwrite_approved {
            self.conflict.as_ref().unwrap_or(&self.saved)
        } else {
            &self.saved
        };
        thinoda_platform::storage::ExpectedRevision::Match(revision.clone())
    }

    fn record(
        &mut self,
        result: &Result<
            thinoda_platform::storage::Revision,
            thinoda_platform::storage::StorageError,
        >,
    ) {
        match result {
            Ok(revision) => {
                self.saved = revision.clone();
                self.conflict = None;
            }
            Err(thinoda_platform::storage::StorageError::Conflict { actual }) => {
                self.conflict = actual.clone();
            }
            Err(_) => {}
        }
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
    save_back_picked_with_overwrite(path, bytes, false, cancel).await
}

pub(crate) async fn save_back_picked_with_overwrite(
    path: &Path,
    bytes: &[u8],
    overwrite_approved: bool,
    cancel: thinoda_platform::storage::CancelToken,
) -> Result<thinoda_platform::storage::Revision, thinoda_platform::storage::StorageError> {
    #[cfg(target_arch = "wasm32")]
    {
        web::save_back_picked(path, bytes, overwrite_approved, cancel).await
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (path, bytes, overwrite_approved, cancel);
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

fn is_drawio_png_name(path: &Path) -> bool {
    path.file_name().is_some_and(|name| {
        name.to_string_lossy()
            .to_ascii_lowercase()
            .ends_with(".drawio.png")
    })
}

/// Explicit draw.io PNG exports open as documents; other supported images keep
/// the existing image-drop behavior.
pub(crate) fn is_dropped_image_path(path: &Path) -> bool {
    crate::images::is_supported_image_path(path) && !is_drawio_png_name(path)
}

/// Files dropped onto the window. Desktop: their paths. Web: browsers only expose
/// contents, which are returned later by [`take_picked`].
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
        if let Some(limit) = crate::doc::import_read_limit(path) {
            let bytes = pfs::read_bounded(path, limit).map_err(|error| {
                if pfs::is_read_limit_exceeded(&error)
                    && let Err(error) =
                        thinoda_io::validate_import_file_size(path, limit as u64 + 1)
                {
                    return error;
                }
                error.into()
            })?;
            return thinoda_io::import_named_bytes(path, &bytes);
        }
        thinoda_io::import_file(path)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let bytes = match crate::doc::import_read_limit(path) {
            Some(limit) => thinoda_platform::vfs::global()
                .read_bounded(path, limit)
                .map_err(|error| crate::doc::vfs_import_error(path, error))?,
            None => pfs::read(path)?,
        };
        thinoda_io::import_named_bytes(path, &bytes)
    }
}

/// Normalizes a file name typed into the browser save prompt. Keeps an allowed
/// extension the user typed, otherwise appends the first allowed one.
pub fn web_save_name(input: &str, allowed: &[&str]) -> Option<String> {
    let name = thinoda_platform::web::sanitize_file_name(input, "")?;
    let lower = name.to_ascii_lowercase();
    if allowed
        .iter()
        .any(|ext| lower.ends_with(&format!(".{}", ext.to_ascii_lowercase())))
    {
        return Some(name);
    }
    let default = allowed.first().copied().unwrap_or_default();
    if default.is_empty() {
        return Some(name);
    }
    if let Some((_, last)) = default.rsplit_once('.') {
        let suffix = format!(".{}", last.to_ascii_lowercase());
        if lower.ends_with(&suffix) && name.len() > suffix.len() {
            return Some(format!("{}.{default}", &name[..name.len() - suffix.len()]));
        }
    }
    Some(format!("{name}.{default}"))
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

#[cfg(any(target_arch = "wasm32", test))]
fn is_dropped_image(name: &str) -> bool {
    !is_drawio_png_name(Path::new(name))
        && thinoda_platform::web::sanitize_file_name(name, "")
            .is_some_and(|name| is_dropped_image_path(Path::new(&name)))
}

#[cfg(any(target_arch = "wasm32", test))]
fn normalized_upload_name(name: &str) -> String {
    thinoda_platform::web::sanitize_file_name(name, "").unwrap_or_else(|| name.to_owned())
}

#[cfg(any(target_arch = "wasm32", test))]
fn document_upload_size_error(purpose: Purpose, name: &str, size: u64) -> Option<String> {
    if purpose == Purpose::StencilLibrary {
        return (size > thinoda_io::drawio_library::MAX_LIBRARY_BYTES as u64)
            .then(|| crate::i18n::t(crate::file_ops::DOCUMENT_TOO_LARGE).to_owned());
    }
    let document = matches!(purpose, Purpose::Open | Purpose::Import | Purpose::Merge)
        || matches!(purpose, Purpose::Dropped) && !is_dropped_image(name);
    if !document {
        return None;
    }
    let name = normalized_upload_name(name);
    let path = Path::new(&name);
    thinoda_io::validate_import_size(path, size)
        .is_err()
        .then(|| crate::doc::import_size_error(path))
}

#[cfg(any(target_arch = "wasm32", test))]
fn dropped_image_size_error(picked: &Picked, size: u64) -> Option<&'static str> {
    if size > thinoda_core::resources::MAX_RESOURCE_BYTES as u64 {
        return Some("local resource exceeds 8 MiB");
    }
    let staged = picked
        .dropped_uploads
        .iter()
        .filter_map(|upload| match upload {
            DroppedUpload::Image(image) => Some(image.bytes.len()),
            DroppedUpload::Document(_) => None,
        })
        .sum::<usize>();
    let remaining = thinoda_core::resources::MAX_SHEET_RESOURCE_BYTES.saturating_sub(staged);
    if size > remaining as u64 {
        return Some("dropped images exceed the 32 MiB staging limit");
    }
    None
}

#[cfg(test)]
fn store_uploaded_file(picked: &mut Picked, name: &str, bytes: Result<Vec<u8>, String>) {
    store_uploaded_file_with(picked, name, bytes, store_upload_in);
}

#[cfg(any(target_arch = "wasm32", test))]
fn store_uploaded_file_with(
    picked: &mut Picked,
    name: &str,
    bytes: Result<Vec<u8>, String>,
    store: impl FnOnce(&Path, &str, &[u8]) -> io::Result<PathBuf>,
) {
    match bytes.and_then(|bytes| {
        let bounded_image_drop =
            matches!(picked.purpose, Purpose::Dropped) && is_dropped_image(name);
        if matches!(picked.purpose, Purpose::Image(_) | Purpose::Attachment(_))
            && bytes.len() > thinoda_core::resources::MAX_RESOURCE_BYTES
        {
            return Err("local resource exceeds 8 MiB".into());
        }
        if matches!(picked.purpose, Purpose::Image(_)) || bounded_image_drop {
            if bounded_image_drop
                && let Some(error) = dropped_image_size_error(picked, bytes.len() as u64)
            {
                return Err(error.into());
            }
            let upload = ImageUpload::new(name, bytes)?;
            if bounded_image_drop {
                picked.dropped_uploads.push(DroppedUpload::Image(upload));
            } else if picked.image_upload.is_none() {
                picked.image_upload = Some(upload);
            }
            return Ok(None);
        }
        let directory = match picked.purpose {
            Purpose::Attachment(request) => format!("/AttachmentUploads/{request}"),
            _ => WEB_DOCUMENTS.into(),
        };
        let name = if matches!(picked.purpose, Purpose::Attachment(_)) {
            thinoda_core::attachments::safe_name(name)
        } else {
            name.to_owned()
        };
        store(Path::new(&directory), &name, &bytes)
            .map(Some)
            .map_err(|e| e.to_string())
    }) {
        Ok(Some(path)) if matches!(picked.purpose, Purpose::Dropped) => {
            picked.dropped_uploads.push(DroppedUpload::Document(path));
        }
        Ok(Some(path)) => picked.paths.push(path),
        Ok(None) => {}
        Err(error) => picked.errors.push(format!("{name}: {error}")),
    }
}

#[cfg(any(target_arch = "wasm32", test))]
fn upload_read_error(purpose: Purpose) -> &'static str {
    match purpose {
        Purpose::Image(_) => "could not read local image",
        Purpose::Attachment(_) => "could not read local attachment",
        _ => "could not read local document",
    }
}

#[cfg(any(target_arch = "wasm32", test))]
mod browser_upload {
    use super::{
        Picked, Purpose, WEB_DOCUMENTS, WEB_UPLOAD_INPUTS, document_id, document_upload_size_error,
        is_dropped_image, store_uploaded_file_with, upload_candidates,
    };
    use std::{
        io,
        path::{Path, PathBuf},
    };
    use thinoda_platform::storage::{CancelToken, ExpectedRevision, StorageError, StorageProvider};

    /// Create a document with conditional writes, optionally reusing identical uploads.
    pub(super) async fn store_document(
        provider: &impl StorageProvider,
        name: &str,
        bytes: &[u8],
        reuse_identical: bool,
    ) -> Result<PathBuf, String> {
        if let Some(error) = document_upload_size_error(Purpose::Open, name, bytes.len() as u64) {
            return Err(error);
        }
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
                Err(StorageError::Conflict { .. }) if reuse_identical => {
                    let existing = match crate::doc::import_read_limit(&path) {
                        Some(limit) => {
                            provider
                                .read_bounded(&id, limit, CancelToken::default())
                                .await
                        }
                        None => provider.read(&id, CancelToken::default()).await,
                    };
                    match existing {
                        Ok(existing) if existing.bytes == bytes => return Ok(path),
                        Ok(_) | Err(StorageError::TooLarge) => {}
                        Err(error) => return Err(error.to_string()),
                    }
                }
                Err(StorageError::Conflict { .. }) => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        Err("too many files with this name".into())
    }

    /// Route a browser upload to durable documents, synchronous inputs, or memory.
    pub(super) async fn store_with(
        picked: &mut Picked,
        name: &str,
        bytes: Result<Vec<u8>, String>,
        provider: &impl StorageProvider,
        store_vfs: impl FnOnce(&Path, &str, &[u8]) -> io::Result<PathBuf>,
    ) {
        if let Ok(bytes) = &bytes
            && let Some(error) =
                document_upload_size_error(picked.purpose, name, bytes.len() as u64)
        {
            picked.errors.push(format!("{name}: {error}"));
            return;
        }
        let durable_document = matches!(picked.purpose, Purpose::Open | Purpose::Import)
            || matches!(picked.purpose, Purpose::Dropped) && !is_dropped_image(name);
        if durable_document {
            match bytes {
                Ok(bytes) => match store_document(provider, name, &bytes, true).await {
                    Ok(path) if matches!(picked.purpose, Purpose::Dropped) => {
                        picked
                            .dropped_uploads
                            .push(super::DroppedUpload::Document(path));
                    }
                    Ok(path) => picked.paths.push(path),
                    Err(error) => picked.errors.push(format!("{name}: {error}")),
                },
                Err(error) => picked.errors.push(format!("{name}: {error}")),
            }
        } else if matches!(
            picked.purpose,
            Purpose::Merge | Purpose::FimSchemas | Purpose::StencilLibrary
        ) {
            match bytes.and_then(|bytes| {
                store_vfs(Path::new(WEB_UPLOAD_INPUTS), name, &bytes)
                    .map_err(|error| error.to_string())
            }) {
                Ok(path) => picked.paths.push(path),
                Err(error) => picked.errors.push(format!("{name}: {error}")),
            }
        } else {
            store_uploaded_file_with(picked, name, bytes, store_vfs);
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use super::{
        Filter, Picked, PickedRevision, Purpose, document_id, document_upload_size_error,
        dropped_image_size_error, is_dropped_image, normalized_upload_name, store_upload_in,
        upload_read_error,
    };
    use std::{
        cell::RefCell,
        collections::HashMap,
        path::{Path, PathBuf},
    };
    use thinoda_platform::{
        browser_storage::{self as bs, BrowserStorageProvider},
        storage::{CancelToken, Revision, StorageError, StorageProvider},
    };

    /// A file explicitly picked with the File System Access API for this tab.
    struct PickedHandle {
        session: String,
        revision: PickedRevision,
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
            && picked.image_upload.is_none()
            && picked.dropped_uploads.is_empty()
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

    async fn store(picked: &mut Picked, name: &str, bytes: Result<Vec<u8>, String>) {
        let provider = BrowserStorageProvider::default();
        super::browser_upload::store_with(picked, name, bytes, &provider, store_upload_in).await;
    }

    async fn store_document(name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
        let provider = BrowserStorageProvider::default();
        // A byte-identical file may be a different OS file. Never attach a new
        // writable handle to an existing open document's durable path.
        super::browser_upload::store_document(&provider, name, bytes, false).await
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
            wasm_bindgen_futures::spawn_local(open_with_handles(multiple));
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
                image_upload: None,
                dropped_uploads: Vec::new(),
            };
            for handle in handles {
                let name = handle.file_name();
                if matches!(purpose, Purpose::Image(_) | Purpose::Attachment(_))
                    && handle.inner().size() > thinoda_core::resources::MAX_RESOURCE_BYTES as f64
                {
                    picked.errors.push("local resource exceeds 8 MiB".into());
                    continue;
                }
                if let Some(error) =
                    document_upload_size_error(purpose, &name, handle.inner().size() as u64)
                {
                    picked.errors.push(format!("{name}: {error}"));
                    continue;
                }
                // Unlike rfd's FileReader wrapper, Blob::arrayBuffer rejects on
                // failure for every purpose, including attachments and inputs.
                let bytes = handle
                    .inner()
                    .array_buffer()
                    .await
                    .map_err(|_| upload_read_error(purpose).to_owned())
                    .and_then(|buffer| {
                        let bytes = js_sys::Uint8Array::new(&buffer);
                        if let Some(error) =
                            document_upload_size_error(purpose, &name, bytes.length() as u64)
                        {
                            Err(error)
                        } else {
                            Ok(bytes.to_vec())
                        }
                    });
                store(&mut picked, &name, bytes).await;
            }
            deliver(picked);
        });
    }

    async fn open_with_handles(multiple: bool) {
        let mut picked = Picked {
            purpose: Purpose::Open,
            paths: Vec::new(),
            errors: Vec::new(),
            image_owner: None,
            image_upload: None,
            dropped_uploads: Vec::new(),
        };
        match bs::pick_file_handles(multiple).await {
            Err(StorageError::Cancelled) => {}
            Err(error) => picked.errors.push(error.to_string()),
            Ok(files) => {
                for file in files {
                    let file = match file {
                        Ok(file) => file,
                        Err(error) => {
                            picked.errors.push(error.to_string());
                            continue;
                        }
                    };
                    if let Err(error) = super::upload_candidates(&file.name) {
                        bs::forget_picked_file(&file.session);
                        picked.errors.push(format!("{}: {error}", file.name));
                        continue;
                    }
                    let name = normalized_upload_name(&file.name);
                    let limit = crate::doc::import_read_limit(Path::new(&name));
                    let content =
                        match bs::read_picked_file(&file.session, limit, CancelToken::default())
                            .await
                        {
                            Ok(content) => content,
                            Err(error) => {
                                bs::forget_picked_file(&file.session);
                                let message = match error {
                                    StorageError::TooLarge => {
                                        crate::doc::import_size_error(Path::new(&name))
                                    }
                                    error => crate::file_ops::storage_error(&error),
                                };
                                picked.errors.push(format!("{}: {message}", file.name));
                                continue;
                            }
                        };
                    match store_document(&file.name, &content.bytes).await {
                        Ok(path) => {
                            let previous = HANDLES.with(|handles| {
                                handles.borrow_mut().insert(
                                    path.clone(),
                                    PickedHandle {
                                        session: file.session,
                                        revision: PickedRevision {
                                            saved: content.revision,
                                            conflict: None,
                                        },
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
                    }
                }
            }
        }
        deliver(picked);
    }

    pub(super) fn has_picked_file(path: &Path) -> bool {
        HANDLES.with(|handles| handles.borrow().contains_key(path))
    }

    pub(super) async fn save_back_picked(
        path: &Path,
        bytes: &[u8],
        overwrite_approved: bool,
        cancel: CancelToken,
    ) -> Result<Revision, StorageError> {
        let (session, expected) = HANDLES
            .with(|handles| {
                handles
                    .borrow()
                    .get(path)
                    .map(|h| (h.session.clone(), h.revision.expected(overwrite_approved)))
            })
            .ok_or(StorageError::Unsupported)?;
        let result = bs::write_picked_file(&session, bytes, expected, cancel).await;
        HANDLES.with(|handles| {
            if let Some(handle) = handles.borrow_mut().get_mut(path)
                && handle.session == session
            {
                handle.revision.record(&result);
            }
        });
        result
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
                image_upload: None,
                dropped_uploads: Vec::new(),
            };
            for file in files {
                let name = file.path().to_string_lossy().into_owned();
                // Check the browser File before `bytes_async` allocates.
                if let Some(web_file) = file.web_file() {
                    let error = if is_dropped_image(&name) {
                        dropped_image_size_error(&picked, web_file.size() as u64).map(str::to_owned)
                    } else {
                        document_upload_size_error(Purpose::Dropped, &name, web_file.size() as u64)
                    };
                    if let Some(error) = error {
                        picked.errors.push(format!("{name}: {error}"));
                        continue;
                    }
                }
                let bytes = file.bytes_async().await;
                store(&mut picked, &name, bytes).await;
            }
            deliver(picked);
        });
    }
}

#[cfg(test)]
mod tests {
    #[cfg(not(target_arch = "wasm32"))]
    use super::browser_upload::{store_document, store_with as store_browser_upload_with};
    use super::*;
    use thinoda_platform::vfs::KvStore;

    struct QuotaStore {
        inner: thinoda_platform::vfs::MemoryStore,
        max_value: usize,
    }

    impl KvStore for QuotaStore {
        fn get(&self, key: &str) -> Option<String> {
            self.inner.get(key)
        }

        fn set(&self, key: &str, value: &str) -> io::Result<()> {
            if value.len() > self.max_value {
                return Err(io::Error::other("QuotaExceededError"));
            }
            self.inner.set(key, value)
        }

        fn remove(&self, key: &str) {
            self.inner.remove(key);
        }

        fn keys(&self) -> Vec<String> {
            self.inner.keys()
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn run_upload_test(future: impl std::future::Future<Output = ()>) {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(future);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn empty_picked(purpose: Purpose) -> Picked {
        Picked {
            purpose,
            paths: Vec::new(),
            errors: Vec::new(),
            image_owner: None,
            image_upload: None,
            dropped_uploads: Vec::new(),
        }
    }

    #[test]
    fn document_quota_failure_keeps_the_existing_local_copy_intact() {
        let vfs = thinoda_platform::vfs::Vfs::new(Box::new(QuotaStore {
            inner: Default::default(),
            max_value: 256,
        }));
        let original = Path::new("/Documents/existing.thinoda");
        let upload = Path::new("/Documents/large.thinoda");
        vfs.write(original, b"existing document", 1).unwrap();
        let error = vfs.write(upload, &[0; 1024], 2).unwrap_err();
        assert!(error.to_string().contains("storage may be full"));
        assert!(!vfs.exists(upload));
        assert_eq!(vfs.read(original).unwrap(), b"existing document");
        assert!(vfs.write(original, &[0; 1024], 3).is_err());
        assert_eq!(vfs.read(original).unwrap(), b"existing document");
    }

    #[test]
    fn explicitly_named_drawio_png_drops_are_documents() {
        for name in [
            "diagram.drawio.png",
            "diagram.DRAWIO.PNG",
            ".drawio.png",
            "diagram.drawio?.p?ng",
        ] {
            assert!(!is_dropped_image(name), "{name}");
        }
        for name in [
            "diagram.png",
            "drawio.png",
            "diagram.drawio.jpg",
            "diagram.jpg",
        ] {
            assert!(is_dropped_image(name), "{name}");
        }
        assert!(crate::images::is_supported_image_path(Path::new(
            "diagram.drawio.png"
        )));
    }

    #[test]
    fn document_upload_limits_use_the_staged_name_and_only_document_purposes() {
        let archive_limit = 64 * 1024 * 1024;
        for purpose in [
            Purpose::Open,
            Purpose::Import,
            Purpose::Merge,
            Purpose::Dropped,
        ] {
            assert_eq!(
                document_upload_size_error(purpose, "map.mmap", archive_limit),
                None
            );
            assert_eq!(
                document_upload_size_error(purpose, "map.mmap", archive_limit + 1),
                Some(
                    crate::i18n::t(
                        "Could not read this MindManager file. It may be invalid, encrypted, too large or unsafe."
                    )
                    .to_owned()
                )
            );
            assert_eq!(
                document_upload_size_error(
                    purpose,
                    "map.xdf?2",
                    thinoda_io::fim::MAX_XML_BYTES as u64
                ),
                None
            );
            assert_eq!(
                document_upload_size_error(
                    purpose,
                    "map.xdf?2",
                    thinoda_io::fim::MAX_XML_BYTES as u64 + 1
                ),
                Some(crate::i18n::t(crate::file_ops::DOCUMENT_TOO_LARGE).into())
            );
            assert_eq!(
                document_upload_size_error(purpose, "map.xml", archive_limit + 1),
                Some(crate::i18n::t(crate::file_ops::DOCUMENT_TOO_LARGE).into())
            );
            assert_eq!(
                document_upload_size_error(purpose, "map.xmind", u64::MAX),
                None
            );
        }
        assert_eq!(
            document_upload_size_error(Purpose::FimSchemas, "schemas.xdf2", u64::MAX),
            None
        );
        assert_eq!(
            document_upload_size_error(Purpose::Dropped, "image.png", u64::MAX),
            None
        );
        assert_eq!(
            document_upload_size_error(Purpose::Dropped, "diagram.DRAWIO.PNG", archive_limit),
            None
        );
        assert_eq!(
            document_upload_size_error(Purpose::Dropped, "diagram.DRAWIO.PNG", archive_limit + 1),
            Some(crate::i18n::t(crate::file_ops::DOCUMENT_TOO_LARGE).into())
        );
        assert_eq!(
            document_upload_size_error(Purpose::Image(1), "image.xdf2", u64::MAX),
            None
        );
        let library_limit = thinoda_io::drawio_library::MAX_LIBRARY_BYTES as u64;
        assert_eq!(
            document_upload_size_error(Purpose::StencilLibrary, "shapes.xml", library_limit),
            None
        );
        assert_eq!(
            document_upload_size_error(Purpose::StencilLibrary, "shapes.xml", library_limit + 1),
            Some(crate::i18n::t(crate::file_ops::DOCUMENT_TOO_LARGE).into())
        );
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn oversized_uploaded_document_is_not_staged() {
        let vfs = std::sync::Arc::new(thinoda_platform::vfs::Vfs::memory());
        let provider = thinoda_platform::storage::VfsStorageProvider::new(vfs.clone(), false);
        let mut picked = empty_picked(Purpose::Import);
        run_upload_test(async {
            store_browser_upload_with(
                &mut picked,
                "map.xdf3",
                Ok(vec![0; thinoda_io::fim::MAX_XML_BYTES + 1]),
                &provider,
                |_, _, _| panic!("document must not reach VFS staging"),
            )
            .await;
        });
        assert!(picked.paths.is_empty());
        assert_eq!(
            picked.errors,
            [format!(
                "map.xdf3: {}",
                crate::i18n::t(crate::file_ops::DOCUMENT_TOO_LARGE)
            )]
        );
        assert!(!vfs.exists(Path::new("/Documents/map.xdf3")));
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn merged_fim_input_is_bounded_before_import() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("large.xdf2");
        let size = thinoda_io::fim::MAX_XML_BYTES as u64 + 1;
        std::fs::File::create(&path).unwrap().set_len(size).unwrap();
        let error = import_workbook(&path).unwrap_err();
        assert!(matches!(
            error,
            thinoda_io::IoError::UnsupportedFormat(name) if name == path.display().to_string()
        ));
        assert_eq!(std::fs::metadata(path).unwrap().len(), size);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn oversized_merged_mindmanager_keeps_the_file_import_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("large.mmap");
        let size = thinoda_io::import_read_limit(&path).unwrap() as u64 + 1;
        std::fs::File::create(&path).unwrap().set_len(size).unwrap();
        assert!(matches!(
            import_workbook(&path),
            Err(thinoda_io::IoError::Invalid(message))
                if message == "MindManager file exceeds size limit"
        ));
        assert_eq!(std::fs::metadata(path).unwrap().len(), size);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn open_and_import_only_dispatch_a_successfully_stored_document() {
        for purpose in [Purpose::Open, Purpose::Import] {
            let vfs = std::sync::Arc::new(thinoda_platform::vfs::Vfs::new(Box::new(QuotaStore {
                inner: Default::default(),
                max_value: 256,
            })));
            let provider = thinoda_platform::storage::VfsStorageProvider::new(vfs.clone(), false);
            let original = Path::new("/Documents/existing.thinoda");
            vfs.write(original, b"existing document", 1).unwrap();
            let mut picked = empty_picked(purpose);
            run_upload_test(async {
                store_browser_upload_with(
                    &mut picked,
                    "small.thinoda",
                    Ok(vec![1, 2, 3]),
                    &provider,
                    |_, _, _| panic!("documents must use the durable provider"),
                )
                .await;
                assert_eq!(picked.paths, [PathBuf::from("/Documents/small.thinoda")]);
                assert!(picked.errors.is_empty());
                assert!(picked.image_upload.is_none());
                assert!(picked.dropped_uploads.is_empty());

                store_browser_upload_with(
                    &mut picked,
                    "large.thinoda",
                    Ok(vec![0; 1024]),
                    &provider,
                    |_, _, _| panic!("documents must use the durable provider"),
                )
                .await;
                assert_eq!(picked.paths, [PathBuf::from("/Documents/small.thinoda")]);
                assert_eq!(picked.errors.len(), 1);
                assert!(picked.errors[0].contains("storage may be full"));
                assert!(!vfs.exists(Path::new("/Documents/large.thinoda")));
                assert_eq!(vfs.read(original).unwrap(), b"existing document");

                store_browser_upload_with(
                    &mut picked,
                    "unreadable.thinoda",
                    Err("could not read local document".into()),
                    &provider,
                    |_, _, _| panic!("unreadable documents must not be stored"),
                )
                .await;
                assert_eq!(picked.paths, [PathBuf::from("/Documents/small.thinoda")]);
                assert_eq!(
                    picked.errors[1],
                    "unreadable.thinoda: could not read local document"
                );
                assert_eq!(vfs.read(original).unwrap(), b"existing document");
            });
        }
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn durable_uploads_reuse_identical_data_without_overwriting_conflicts() {
        let vfs = std::sync::Arc::new(thinoda_platform::vfs::Vfs::new(Box::new(
            thinoda_platform::vfs::MemoryStore::default(),
        )));
        let provider = thinoda_platform::storage::VfsStorageProvider::new(vfs.clone(), false);
        run_upload_test(async {
            let first = store_document(&provider, "map.thinoda", b"first", true)
                .await
                .unwrap();
            let same = store_document(&provider, "map.thinoda", b"first", true)
                .await
                .unwrap();
            let different = store_document(&provider, "map.thinoda", b"second", true)
                .await
                .unwrap();
            assert_eq!(first, Path::new(WEB_DOCUMENTS).join("map.thinoda"));
            assert_eq!(same, first);
            assert_eq!(different, Path::new(WEB_DOCUMENTS).join("map (2).thinoda"));
            assert_eq!(vfs.read(&first).unwrap(), b"first");
            assert_eq!(vfs.read(&different).unwrap(), b"second");

            let bounded = store_document(&provider, "bounded.mmap", b"same", true)
                .await
                .unwrap();
            let reused = store_document(&provider, "bounded.mmap", b"same", true)
                .await
                .unwrap();
            assert_eq!(bounded, reused);
        });
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn oversized_existing_upload_is_preserved_and_not_read_unbounded() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use thinoda_platform::storage::{
            CancelToken, Capabilities, Entry, ExpectedRevision, ReadResult, Revision, StorageError,
            StorageFuture, StorageId, StorageProvider, VfsStorageProvider,
        };

        struct OversizedExisting {
            inner: VfsStorageProvider,
            bounded_reads: AtomicUsize,
        }
        impl StorageProvider for OversizedExisting {
            fn capabilities(&self) -> Capabilities {
                self.inner.capabilities()
            }
            fn list<'a>(
                &'a self,
                id: &'a StorageId,
                cancel: CancelToken,
            ) -> StorageFuture<'a, Vec<Entry>> {
                self.inner.list(id, cancel)
            }
            fn read<'a>(
                &'a self,
                _id: &'a StorageId,
                _cancel: CancelToken,
            ) -> StorageFuture<'a, ReadResult> {
                panic!("bounded document collisions must not use an unbounded read")
            }
            fn read_bounded<'a>(
                &'a self,
                _id: &'a StorageId,
                limit: usize,
                _cancel: CancelToken,
            ) -> StorageFuture<'a, ReadResult> {
                assert_eq!(limit, 64 * 1024 * 1024);
                self.bounded_reads.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Err(StorageError::TooLarge) })
            }
            fn write<'a>(
                &'a self,
                id: &'a StorageId,
                bytes: &'a [u8],
                expected: ExpectedRevision,
                cancel: CancelToken,
            ) -> StorageFuture<'a, Revision> {
                self.inner.write(id, bytes, expected, cancel)
            }
        }

        let vfs = std::sync::Arc::new(thinoda_platform::vfs::Vfs::memory());
        let original = Path::new("/Documents/map.mmap");
        vfs.write(original, b"preserved", 1).unwrap();
        let provider = OversizedExisting {
            inner: VfsStorageProvider::new(vfs.clone(), false),
            bounded_reads: AtomicUsize::new(0),
        };
        run_upload_test(async {
            let copy = store_document(&provider, "map.mmap", b"new", true)
                .await
                .unwrap();
            assert_eq!(copy, Path::new("/Documents/map (2).mmap"));
            assert_eq!(vfs.read(original).unwrap(), b"preserved");
            assert_eq!(vfs.read(&copy).unwrap(), b"new");
        });
        assert_eq!(provider.bounded_reads.load(Ordering::SeqCst), 1);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn picked_file_handles_get_distinct_paths_even_for_identical_files() {
        let vfs = std::sync::Arc::new(thinoda_platform::vfs::Vfs::new(Box::new(
            thinoda_platform::vfs::MemoryStore::default(),
        )));
        let provider = thinoda_platform::storage::VfsStorageProvider::new(vfs.clone(), false);
        run_upload_test(async {
            let existing = store_document(&provider, "map.thinoda", b"same", true)
                .await
                .unwrap();
            let first_handle = store_document(&provider, "map.thinoda", b"same", false)
                .await
                .unwrap();
            let second_handle = store_document(&provider, "map.thinoda", b"same", false)
                .await
                .unwrap();
            assert_ne!(existing, first_handle);
            assert_ne!(first_handle, second_handle);
            for path in [existing, first_handle, second_handle] {
                assert_eq!(vfs.read(&path).unwrap(), b"same");
            }
        });
    }

    #[test]
    fn picked_file_overwrite_matches_only_the_observed_conflict_revision() {
        use thinoda_platform::storage::{ExpectedRevision, Revision, StorageError};

        let revision = |value: &str| Revision(value.into());
        let mut picked = PickedRevision {
            saved: revision("saved"),
            conflict: None,
        };
        assert_eq!(
            picked.expected(true),
            ExpectedRevision::Match(revision("saved"))
        );
        picked.record(&Err(StorageError::Conflict {
            actual: Some(revision("external")),
        }));
        assert_eq!(
            picked.expected(false),
            ExpectedRevision::Match(revision("saved"))
        );
        assert_eq!(
            picked.expected(true),
            ExpectedRevision::Match(revision("external"))
        );
        picked.record(&Err(StorageError::Conflict {
            actual: Some(revision("newer external")),
        }));
        assert_eq!(
            picked.expected(true),
            ExpectedRevision::Match(revision("newer external"))
        );
        picked.record(&Ok(revision("written")));
        assert_eq!(
            picked.expected(true),
            ExpectedRevision::Match(revision("written"))
        );
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn browser_upload_routes_preserve_drop_order_and_synchronous_inputs() {
        let vfs = std::sync::Arc::new(thinoda_platform::vfs::Vfs::new(Box::new(
            thinoda_platform::vfs::MemoryStore::default(),
        )));
        let provider = thinoda_platform::storage::VfsStorageProvider::new(vfs.clone(), false);
        run_upload_test(async {
            let mut dropped = empty_picked(Purpose::Dropped);
            for (name, bytes) in [
                ("before.j?pg", vec![1]),
                ("map.thinoda", vec![2]),
                ("after.PNG", vec![3]),
                ("diagram.DRAWIO.PNG", vec![4]),
                ("diagram.DRAWIO.PNG", vec![5]),
                (".drawio.png", vec![6]),
            ] {
                store_browser_upload_with(&mut dropped, name, Ok(bytes), &provider, |_, _, _| {
                    panic!("dropped images stay in memory and documents use durable storage")
                })
                .await;
            }
            assert!(dropped.paths.is_empty());
            assert!(dropped.errors.is_empty());
            assert_eq!(dropped.dropped_uploads.len(), 6);
            assert!(matches!(
                &dropped.dropped_uploads[0],
                DroppedUpload::Image(image) if image.name == "before.jpg" && image.bytes == [1]
            ));
            assert!(matches!(
                &dropped.dropped_uploads[1],
                DroppedUpload::Document(path) if path == &Path::new(WEB_DOCUMENTS).join("map.thinoda")
            ));
            assert!(matches!(
                &dropped.dropped_uploads[2],
                DroppedUpload::Image(image) if image.name == "after.PNG" && image.bytes == [3]
            ));
            assert!(matches!(
                &dropped.dropped_uploads[3],
                DroppedUpload::Document(path) if path == &Path::new(WEB_DOCUMENTS).join("diagram.DRAWIO.PNG")
            ));
            assert!(matches!(
                &dropped.dropped_uploads[4],
                DroppedUpload::Document(path) if path == &Path::new(WEB_DOCUMENTS).join("diagram.DRAWIO (2).PNG")
            ));
            assert!(matches!(
                &dropped.dropped_uploads[5],
                DroppedUpload::Document(path) if path == &Path::new(WEB_DOCUMENTS).join("drawio.png")
            ));
            assert_eq!(
                vfs.read(&Path::new(WEB_DOCUMENTS).join("map.thinoda"))
                    .unwrap(),
                [2]
            );
            assert_eq!(
                vfs.read(&Path::new(WEB_DOCUMENTS).join("diagram.DRAWIO.PNG"))
                    .unwrap(),
                [4]
            );
            assert_eq!(
                vfs.read(&Path::new(WEB_DOCUMENTS).join("diagram.DRAWIO (2).PNG"))
                    .unwrap(),
                [5]
            );
            assert_eq!(
                vfs.read(&Path::new(WEB_DOCUMENTS).join("drawio.png"))
                    .unwrap(),
                [6]
            );
            store_browser_upload_with(
                &mut dropped,
                "unreadable.thinoda",
                Err("could not read local document".into()),
                &provider,
                |_, _, _| panic!("unreadable drops must not enter the VFS"),
            )
            .await;
            assert_eq!(dropped.dropped_uploads.len(), 6);
            assert_eq!(
                dropped.errors,
                ["unreadable.thinoda: could not read local document"]
            );

            for purpose in [Purpose::Merge, Purpose::FimSchemas] {
                let mut picked = empty_picked(purpose);
                store_browser_upload_with(
                    &mut picked,
                    "input.bin",
                    Ok(vec![4]),
                    &provider,
                    |directory, name, bytes| {
                        assert_eq!(directory, Path::new(WEB_UPLOAD_INPUTS));
                        assert_eq!(bytes, [4]);
                        Ok(directory.join(name))
                    },
                )
                .await;
                assert_eq!(
                    picked.paths,
                    [Path::new(WEB_UPLOAD_INPUTS).join("input.bin")]
                );
                assert!(picked.errors.is_empty());
            }

            let mut image = empty_picked(Purpose::Image(1));
            store_browser_upload_with(
                &mut image,
                "oversized.jpg",
                Ok(vec![0; thinoda_core::resources::MAX_RESOURCE_BYTES + 1]),
                &provider,
                |_, _, _| panic!("images must not enter the VFS"),
            )
            .await;
            assert!(image.image_upload.is_none());
            assert!(image.errors[0].contains("local resource exceeds 8 MiB"));

            let mut attachment = empty_picked(Purpose::Attachment(47));
            store_browser_upload_with(
                &mut attachment,
                "file.txt",
                Ok(vec![5]),
                &provider,
                |directory, name, bytes| {
                    assert_eq!(directory, Path::new("/AttachmentUploads/47"));
                    assert_eq!(bytes, [5]);
                    Ok(directory.join(name))
                },
            )
            .await;
            assert_eq!(
                attachment.paths,
                [Path::new("/AttachmentUploads/47").join("file.txt")]
            );
            assert!(attachment.errors.is_empty());
        });
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn picker_read_failures_are_reported_without_staging_any_upload() {
        let vfs = std::sync::Arc::new(thinoda_platform::vfs::Vfs::new(Box::new(
            thinoda_platform::vfs::MemoryStore::default(),
        )));
        let provider = thinoda_platform::storage::VfsStorageProvider::new(vfs, false);
        run_upload_test(async {
            for (purpose, message) in [
                (Purpose::Open, "could not read local document"),
                (Purpose::Import, "could not read local document"),
                (Purpose::Merge, "could not read local document"),
                (Purpose::FimSchemas, "could not read local document"),
                (Purpose::Image(1), "could not read local image"),
                (Purpose::Attachment(2), "could not read local attachment"),
            ] {
                assert_eq!(upload_read_error(purpose), message);
                let mut picked = empty_picked(purpose);
                store_browser_upload_with(
                    &mut picked,
                    "unreadable.dat",
                    Err(message.into()),
                    &provider,
                    |_, _, _| panic!("failed reads must not enter the VFS"),
                )
                .await;
                assert_eq!(picked.errors, [format!("unreadable.dat: {message}")]);
                assert!(picked.paths.is_empty());
                assert!(picked.image_upload.is_none());
                assert!(picked.dropped_uploads.is_empty());
            }
        });
    }

    #[test]
    fn attachments_keep_request_scoped_storage_and_safe_names() {
        let mut picked = Picked {
            purpose: Purpose::Attachment(47),
            paths: Vec::new(),
            errors: Vec::new(),
            image_owner: None,
            image_upload: None,
            dropped_uploads: Vec::new(),
        };
        store_uploaded_file_with(
            &mut picked,
            r"C:\docs\bad?.txt",
            Ok(vec![1, 2, 3]),
            |directory, name, bytes| {
                assert_eq!(directory, Path::new("/AttachmentUploads/47"));
                assert_eq!(name, "bad_.txt");
                assert_eq!(bytes, [1, 2, 3]);
                Ok(directory.join(name))
            },
        );
        assert_eq!(
            picked.paths,
            [PathBuf::from("/AttachmentUploads/47/bad_.txt")]
        );
        assert!(picked.image_upload.is_none());
        assert!(picked.errors.is_empty());
        store_uploaded_file_with(
            &mut picked,
            "oversized.txt",
            Ok(vec![0; thinoda_core::resources::MAX_RESOURCE_BYTES + 1]),
            |_, _, _| panic!("oversized attachments must not be stored"),
        );
        assert_eq!(picked.paths.len(), 1);
        assert_eq!(picked.errors.len(), 1);
        assert!(picked.errors[0].contains("local resource exceeds 8 MiB"));
    }

    #[test]
    fn image_upload_is_bounded_sanitized_and_kept_in_memory() {
        let bytes = vec![42; 4_368_119];
        let original_allocation = bytes.as_ptr();
        let upload = ImageUpload::new(r"C:\photos\large.jpg", bytes).unwrap();
        assert_eq!(upload.name, "large.jpg");
        assert_eq!(upload.bytes.len(), 4_368_119);
        assert_eq!(upload.bytes.as_ptr(), original_allocation);
        assert!(ImageUpload::new("..", vec![1]).is_err());
        assert!(
            ImageUpload::new(
                "maximum.jpg",
                vec![0; thinoda_core::resources::MAX_RESOURCE_BYTES]
            )
            .is_ok()
        );
        assert!(
            ImageUpload::new(
                "oversized.jpg",
                vec![0; thinoda_core::resources::MAX_RESOURCE_BYTES + 1]
            )
            .is_err()
        );
    }

    #[test]
    fn image_picker_and_drop_stage_bytes_without_vfs_paths() {
        let mut picker = Picked {
            purpose: Purpose::Image(7),
            paths: Vec::new(),
            errors: Vec::new(),
            image_owner: None,
            image_upload: None,
            dropped_uploads: Vec::new(),
        };
        store_uploaded_file_with(
            &mut picker,
            r"C:\photos\large.jpg",
            Ok(vec![7; 4_368_119]),
            |_, _, _| panic!("image bytes must not enter the persistent store"),
        );
        assert!(picker.paths.is_empty());
        assert!(picker.errors.is_empty());
        let upload = picker.image_upload.as_ref().unwrap();
        assert_eq!(upload.name, "large.jpg");
        assert_eq!(upload.bytes.len(), 4_368_119);
        store_uploaded_file(
            &mut picker,
            "oversized.jpg",
            Ok(vec![0; thinoda_core::resources::MAX_RESOURCE_BYTES + 1]),
        );
        assert_eq!(picker.errors.len(), 1);
        assert_eq!(picker.image_upload.unwrap().name, "large.jpg");

        let mut dropped = Picked {
            purpose: Purpose::Dropped,
            paths: Vec::new(),
            errors: Vec::new(),
            image_owner: None,
            image_upload: None,
            dropped_uploads: vec![DroppedUpload::Document(PathBuf::from("first.thinoda"))],
        };
        // The sanitized name, not just the raw browser name, determines how
        // housekeeping would classify the dropped file.
        store_uploaded_file(&mut dropped, "a.j?pg", Ok(vec![1, 2, 3]));
        assert!(dropped.paths.is_empty());
        assert!(dropped.errors.is_empty());
        assert!(matches!(
            &dropped.dropped_uploads[0],
            DroppedUpload::Document(path) if path == Path::new("first.thinoda")
        ));
        assert!(matches!(
            &dropped.dropped_uploads[1],
            DroppedUpload::Image(upload) if upload.name == "a.jpg" && upload.bytes == [1, 2, 3]
        ));
    }

    #[test]
    fn image_picker_read_failure_is_reported_without_staging() {
        let mut picker = Picked {
            purpose: Purpose::Image(8),
            paths: Vec::new(),
            errors: Vec::new(),
            image_owner: None,
            image_upload: None,
            dropped_uploads: Vec::new(),
        };
        store_uploaded_file(
            &mut picker,
            "unreadable.jpg",
            Err("could not read local image".into()),
        );
        assert!(picker.paths.is_empty());
        assert!(picker.image_upload.is_none());
        assert_eq!(
            picker.errors,
            ["unreadable.jpg: could not read local image"]
        );
    }

    #[test]
    fn image_drop_staging_has_a_bounded_batch_budget() {
        let mut dropped = Picked {
            purpose: Purpose::Dropped,
            paths: Vec::new(),
            errors: Vec::new(),
            image_owner: None,
            image_upload: None,
            dropped_uploads: Vec::new(),
        };
        assert_eq!(
            dropped_image_size_error(&dropped, thinoda_core::resources::MAX_RESOURCE_BYTES as u64),
            None
        );
        assert_eq!(
            dropped_image_size_error(
                &dropped,
                thinoda_core::resources::MAX_RESOURCE_BYTES as u64 + 1
            ),
            Some("local resource exceeds 8 MiB")
        );
        for _ in 0..4 {
            store_uploaded_file(
                &mut dropped,
                "maximum.jpg",
                Ok(vec![0; thinoda_core::resources::MAX_RESOURCE_BYTES]),
            );
        }
        assert_eq!(dropped.dropped_uploads.len(), 4);
        assert!(dropped.errors.is_empty());
        assert_eq!(
            dropped_image_size_error(&dropped, 1),
            Some("dropped images exceed the 32 MiB staging limit")
        );
        store_uploaded_file(&mut dropped, "extra.jpg", Ok(vec![1]));
        assert_eq!(dropped.dropped_uploads.len(), 4);
        assert!(dropped.errors[0].contains("dropped images exceed the 32 MiB staging limit"));
    }

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
        for (input, expected) in [
            ("Plan", "Plan.drawio.png"),
            ("Plan.drawio.png", "Plan.drawio.png"),
            ("Plan.DRAWIO.PNG", "Plan.DRAWIO.PNG"),
            ("Plan.png", "Plan.drawio.png"),
            ("Plan.PNG", "Plan.drawio.png"),
            ("Plan.jpeg", "Plan.jpeg.drawio.png"),
        ] {
            assert_eq!(
                web_save_name(input, &["drawio.png"]).as_deref(),
                Some(expected)
            );
        }
        assert_eq!(
            web_save_name("Plan.svg", &["drawio.svg"]).as_deref(),
            Some("Plan.drawio.svg")
        );
        assert_eq!(
            web_save_name("Plan.DRAWIO.SVG", &["drawio.svg"]).as_deref(),
            Some("Plan.DRAWIO.SVG")
        );
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
