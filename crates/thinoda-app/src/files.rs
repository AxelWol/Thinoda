//! File pickers, save targets and deliveries.
//!
//! Desktop builds use the native dialogs of `rfd` and write straight to disk. The
//! browser build (`crates/thinoda-web`) has no file system access: uploads are copied
//! into the virtual file system (VFS, backed by `localStorage`), documents are saved
//! in `/Documents` and exports or explicit saves are delivered as browser downloads.

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

#[cfg(target_arch = "wasm32")]
mod web {
    use super::{Filter, Picked, Purpose, WEB_DOCUMENTS, store_upload_in};
    use std::{cell::RefCell, path::Path};

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

    fn store(picked: &mut Picked, name: &str, bytes: Result<Vec<u8>, String>) {
        match bytes.and_then(|bytes| {
            let directory = match picked.purpose {
                Purpose::Image(request) => format!("/ImageUploads/{request}"),
                Purpose::Attachment(request) => format!("/AttachmentUploads/{request}"),
                _ => WEB_DOCUMENTS.into(),
            };
            let bounded_image_drop = matches!(picked.purpose, Purpose::Dropped)
                && crate::images::is_supported_image_path(Path::new(name));
            if (matches!(picked.purpose, Purpose::Image(_) | Purpose::Attachment(_))
                || bounded_image_drop)
                && bytes.len() > thinoda_core::resources::MAX_RESOURCE_BYTES
            {
                return Err("local resource exceeds 8 MiB".into());
            }
            let name = if matches!(picked.purpose, Purpose::Attachment(_)) {
                thinoda_core::attachments::safe_name(name)
            } else {
                name.to_owned()
            };
            store_upload_in(Path::new(&directory), &name, &bytes).map_err(|e| e.to_string())
        }) {
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
                store(&mut picked, &handle.file_name(), Ok(bytes));
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
            };
            for file in files {
                let name = file.path().to_string_lossy().into_owned();
                let bytes = file.bytes_async().await;
                store(&mut picked, &name, bytes);
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
    fn desktop_builds_have_no_web_side_effects() {
        assert!(!is_web());
        assert!(take_picked().is_empty());
        assert!(deliver(Path::new("/nonexistent")).is_ok());
        assert!(is_scratch(Path::new("/Downloads/x.png")));
        assert!(!is_scratch(Path::new("/Documents/x.thinoda")));
    }
}
