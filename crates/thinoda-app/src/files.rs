//! File pickers, save targets and deliveries.
//!
//! Desktop builds use the native dialogs of `rfd` and write straight to disk. The
//! browser build (`crates/thinoda-web`) has no file system access: document uploads
//! are copied into the virtual file system (VFS, backed by `localStorage`), documents
//! are saved in `/Documents` and exports or explicit saves are delivered as browser
//! downloads. Image picker and dropped-image uploads are passed directly in memory.

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

/// Copies uploaded `bytes` into `dir` under a safe, unused name. An identical
/// existing file is reused so re-opening the same upload does not pile up copies.
pub fn store_upload_in(dir: &Path, name: &str, bytes: &[u8]) -> io::Result<PathBuf> {
    let file = thinoda_platform::web::sanitize_file_name(name, "")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid file name"))?;
    let (stem, extension) = match file.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem.to_owned(), format!(".{ext}")),
        _ => (file.clone(), String::new()),
    };
    for n in 1..1000 {
        let candidate = if n == 1 {
            file.clone()
        } else {
            format!("{stem} ({n}){extension}")
        };
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
    thinoda_platform::web::sanitize_file_name(name, "")
        .is_some_and(|name| crate::images::is_supported_image_path(Path::new(&name)))
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

#[cfg(any(target_arch = "wasm32", test))]
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

#[cfg(target_arch = "wasm32")]
mod web {
    use super::{
        Filter, Picked, Purpose, dropped_image_size_error, is_dropped_image, store_uploaded_file,
    };
    use std::cell::RefCell;

    thread_local! {
        static INBOX: RefCell<Vec<Picked>> = const { RefCell::new(Vec::new()) };
        static CONTEXT: RefCell<Option<egui::Context>> = const { RefCell::new(None) };
    }

    pub(super) fn set_context(ctx: &egui::Context) {
        CONTEXT.with(|c| {
            if c.borrow().is_none() {
                *c.borrow_mut() = Some(ctx.clone());
            }
        });
    }

    pub(super) fn take_picked() -> Vec<Picked> {
        INBOX.with(|inbox| std::mem::take(&mut *inbox.borrow_mut()))
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
        CONTEXT.with(|c| {
            if let Some(ctx) = c.borrow().as_ref() {
                ctx.request_repaint();
            }
        });
    }

    pub(super) fn pick_files(
        purpose: Purpose,
        title: Option<&str>,
        filters: &[Filter<'_>],
        multiple: bool,
    ) {
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
                if matches!(purpose, Purpose::Image(_) | Purpose::Attachment(_))
                    && handle.inner().size() > thinoda_core::resources::MAX_RESOURCE_BYTES as f64
                {
                    picked.errors.push("local resource exceeds 8 MiB".into());
                    continue;
                }
                let bytes =
                    if matches!(purpose, Purpose::Image(_) | Purpose::Open | Purpose::Import) {
                        // Unlike rfd's FileReader wrapper, Blob::arrayBuffer rejects
                        // on read failure, allowing us to clear the pending request.
                        let error = if matches!(purpose, Purpose::Image(_)) {
                            "could not read local image"
                        } else {
                            "could not read local document"
                        };
                        handle
                            .inner()
                            .array_buffer()
                            .await
                            .map(|buffer| js_sys::Uint8Array::new(&buffer).to_vec())
                            .map_err(|_| error.to_owned())
                    } else {
                        Ok(handle.read().await)
                    };
                store_uploaded_file(&mut picked, &handle.file_name(), bytes);
            }
            deliver(picked);
        });
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
                // egui exposes the browser File, so reject oversized images
                // before `bytes_async` allocates an ArrayBuffer and Rust copy.
                if is_dropped_image(&name)
                    && let Some(file) = file.web_file()
                    && let Some(error) = dropped_image_size_error(&picked, file.size() as u64)
                {
                    picked.errors.push(format!("{name}: {error}"));
                    continue;
                }
                let bytes = file.bytes_async().await;
                store_uploaded_file(&mut picked, &name, bytes);
            }
            deliver(picked);
        });
    }
}

#[cfg(test)]
mod tests {
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
    fn open_and_import_only_dispatch_a_successfully_stored_document() {
        for purpose in [Purpose::Open, Purpose::Import] {
            let vfs = thinoda_platform::vfs::Vfs::new(Box::new(QuotaStore {
                inner: Default::default(),
                max_value: 256,
            }));
            let original = Path::new("/Documents/existing.thinoda");
            vfs.write(original, b"existing document", 1).unwrap();
            let mut picked = Picked {
                purpose,
                paths: Vec::new(),
                errors: Vec::new(),
                image_owner: None,
                image_upload: None,
                dropped_uploads: Vec::new(),
            };
            let store = |directory: &Path, name: &str, bytes: &[u8]| {
                assert_eq!(directory, Path::new(WEB_DOCUMENTS));
                let path = directory.join(name);
                vfs.write(&path, bytes, 2)?;
                Ok(path)
            };
            store_uploaded_file_with(&mut picked, "small.thinoda", Ok(vec![1, 2, 3]), store);
            assert_eq!(picked.paths, [PathBuf::from("/Documents/small.thinoda")]);
            assert!(picked.errors.is_empty());
            assert!(picked.image_upload.is_none());
            assert!(picked.dropped_uploads.is_empty());

            let store = |directory: &Path, name: &str, bytes: &[u8]| {
                assert_eq!(directory, Path::new(WEB_DOCUMENTS));
                let path = directory.join(name);
                vfs.write(&path, bytes, 3)?;
                Ok(path)
            };
            store_uploaded_file_with(&mut picked, "large.thinoda", Ok(vec![0; 1024]), store);
            assert_eq!(picked.paths, [PathBuf::from("/Documents/small.thinoda")]);
            assert_eq!(picked.errors.len(), 1);
            assert!(picked.errors[0].contains("storage may be full"));
            assert!(!vfs.exists(Path::new("/Documents/large.thinoda")));
            assert_eq!(vfs.read(original).unwrap(), b"existing document");

            store_uploaded_file_with(
                &mut picked,
                "unreadable.thinoda",
                Err("could not read local document".into()),
                |_, _, _| panic!("unreadable documents must not be stored"),
            );
            assert_eq!(picked.paths, [PathBuf::from("/Documents/small.thinoda")]);
            assert_eq!(
                picked.errors[1],
                "unreadable.thinoda: could not read local document"
            );
            assert_eq!(vfs.read(original).unwrap(), b"existing document");
        }
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
    fn desktop_builds_have_no_web_side_effects() {
        assert!(!is_web());
        assert!(take_picked().is_empty());
        assert!(deliver(Path::new("/nonexistent")).is_ok());
        assert!(is_scratch(Path::new("/Downloads/x.png")));
        assert!(!is_scratch(Path::new("/Documents/x.thinoda")));
    }
}
