//! Durable same-directory replacement and external-change detection.

#[cfg(not(target_arch = "wasm32"))]
use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Write,
};
use std::{
    io,
    path::{Path, PathBuf},
};
use web_time::SystemTime;

#[cfg(target_arch = "wasm32")]
use crate::vfs;

/// Append `.bak` to the complete filename (for example `map.xmind.bak`).
pub fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".bak");
    PathBuf::from(name)
}

#[cfg(not(target_arch = "wasm32"))]
struct Temporary(PathBuf);
#[cfg(not(target_arch = "wasm32"))]
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Durably write bytes before atomically replacing the destination.
/// The old destination survives any failed replacement. A requested backup is
/// durably replaced before the destination; callers must serialize concurrent saves.
/// In the browser build, files live in browser storage (see [`crate::vfs`]) and
/// each replacement is a single storage operation.
pub fn atomic_write(path: impl AsRef<Path>, bytes: &[u8], keep_backup: bool) -> io::Result<()> {
    let path = path.as_ref();
    #[cfg(target_arch = "wasm32")]
    {
        let vfs = vfs::global();
        if keep_backup && let Ok(previous) = vfs.read(path) {
            vfs.write(&backup_path(path), &previous, vfs::now_ms())?;
        }
        vfs.write(path, bytes, vfs::now_ms())
    }
    #[cfg(not(target_arch = "wasm32"))]
    native_atomic_write(path, bytes, keep_backup)
}

#[cfg(not(target_arch = "wasm32"))]
fn native_atomic_write(path: &Path, bytes: &[u8], keep_backup: bool) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let name = path.file_name().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "destination must name a file")
    })?;
    let mut temp_name = OsString::from(".");
    temp_name.push(name);
    temp_name.push(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let temporary = Temporary(parent.join(temp_name));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary.0)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    if keep_backup {
        match fs::read(path) {
            Ok(previous) => atomic_write(backup_path(path), &previous, false)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    fs::rename(&temporary.0, path).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!(
                "could not replace {} (the file may be read-only or in use): {e}",
                path.display()
            ),
        )
    })?;
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

/// Cheap file identity used to detect a possible external edit (not a content hash).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileFingerprint {
    pub len: u64,
    pub modified: SystemTime,
    path: PathBuf,
}

/// Record a file's length and last-modified timestamp.
pub fn fingerprint(path: impl AsRef<Path>) -> io::Result<FileFingerprint> {
    let path = path.as_ref();
    #[cfg(target_arch = "wasm32")]
    {
        let meta = vfs::global().metadata(path)?;
        Ok(FileFingerprint {
            len: meta.len,
            modified: web_time::UNIX_EPOCH + std::time::Duration::from_millis(meta.modified_ms),
            path: path.to_path_buf(),
        })
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let metadata = fs::metadata(path)?;
        Ok(FileFingerprint {
            len: metadata.len(),
            modified: metadata.modified()?,
            path: path.to_path_buf(),
        })
    }
}

impl FileFingerprint {
    /// Check the recorded file; deletion counts as a change.
    pub fn changed_since(&self) -> io::Result<bool> {
        changed_since(self)
    }
}

/// Check the file recorded in a fingerprint, including deletion.
pub fn changed_since(previous: &FileFingerprint) -> io::Result<bool> {
    match fingerprint(&previous.path) {
        Ok(now) => Ok(now != *previous),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(true),
        Err(e) => Err(e),
    }
}

/// Find likely cloud-sync conflict siblings with the same extension.
/// This is a heuristic; numbered iCloud copies may also be intentional duplicates.
pub fn conflict_copies(path: impl AsRef<Path>) -> io::Result<Vec<PathBuf>> {
    let path = path.as_ref();
    if cfg!(target_arch = "wasm32") {
        // Browser storage is never touched by sync clients.
        return Ok(Vec::new());
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    let extension = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    let computer = std::env::var("COMPUTERNAME").ok().map(|s| s.to_lowercase());
    let mut copies = Vec::new();
    for (candidate, kind) in list_dir(parent)? {
        if candidate == path
            || kind != EntryKind::File
            || candidate
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase()
                != extension
        {
            continue;
        }
        let candidate_stem = candidate
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        let Some(suffix) = candidate_stem.strip_prefix(&stem) else {
            continue;
        };
        let conflict =
            suffix.starts_with(" (") && suffix.ends_with(')') && suffix.contains("conflicted copy");
        let numbered = suffix
            .strip_prefix(' ')
            .is_some_and(|number| number.parse::<u32>().is_ok_and(|n| n >= 2));
        let machine = suffix.strip_prefix('-').is_some_and(|name| {
            (!name.is_empty() && (name.starts_with("desktop-") || name.starts_with("laptop-")))
                || computer
                    .as_deref()
                    .is_some_and(|c| !c.is_empty() && name == c)
        });
        if conflict || numbered || machine {
            copies.push(candidate);
        }
    }
    copies.sort();
    Ok(copies)
}

/// Read a whole file (browser storage on the web).
pub fn read(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
    #[cfg(target_arch = "wasm32")]
    return vfs::global().read(path.as_ref());
    #[cfg(not(target_arch = "wasm32"))]
    fs::read(path)
}

/// Whether a file or directory exists; errors count as missing.
pub fn exists(path: impl AsRef<Path>) -> bool {
    #[cfg(target_arch = "wasm32")]
    return vfs::global().exists(path.as_ref());
    #[cfg(not(target_arch = "wasm32"))]
    path.as_ref().exists()
}

/// Whether a regular file exists.
pub fn is_file(path: impl AsRef<Path>) -> bool {
    #[cfg(target_arch = "wasm32")]
    return vfs::global().is_file(path.as_ref());
    #[cfg(not(target_arch = "wasm32"))]
    path.as_ref().is_file()
}

/// Remove a file.
pub fn remove_file(path: impl AsRef<Path>) -> io::Result<()> {
    #[cfg(target_arch = "wasm32")]
    return vfs::global().remove_file(path.as_ref());
    #[cfg(not(target_arch = "wasm32"))]
    fs::remove_file(path)
}

/// Rename a file (not atomic across directories on every platform).
pub fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    #[cfg(target_arch = "wasm32")]
    return vfs::global().rename(from.as_ref(), to.as_ref(), vfs::now_ms());
    #[cfg(not(target_arch = "wasm32"))]
    fs::rename(from, to)
}

/// Resolve an existing path. The browser filesystem has no links, so this is a
/// lexical normalization that still reports missing files.
pub fn canonicalize(path: impl AsRef<Path>) -> io::Result<PathBuf> {
    #[cfg(target_arch = "wasm32")]
    {
        let path = path.as_ref();
        if !vfs::global().exists(path) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("{} not found", path.display()),
            ));
        }
        Ok(PathBuf::from(vfs::normalize(path)))
    }
    #[cfg(not(target_arch = "wasm32"))]
    fs::canonicalize(path)
}

/// Make a path absolute without touching the filesystem.
pub fn absolute(path: impl AsRef<Path>) -> io::Result<PathBuf> {
    #[cfg(target_arch = "wasm32")]
    return Ok(PathBuf::from(vfs::normalize(path.as_ref())));
    #[cfg(not(target_arch = "wasm32"))]
    std::path::absolute(path)
}

pub(crate) fn create_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = path;
        Ok(())
    }
    #[cfg(not(target_arch = "wasm32"))]
    fs::create_dir_all(path)
}

pub(crate) fn remove_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    #[cfg(target_arch = "wasm32")]
    return vfs::global().remove_dir_all(path.as_ref());
    #[cfg(not(target_arch = "wasm32"))]
    fs::remove_dir_all(path)
}

/// Kind of a directory entry; symbolic links and special files are `Other`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EntryKind {
    File,
    Dir,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    Other,
}

/// Immediate children of a directory. Unreadable entries are reported as `Other`.
pub(crate) fn list_dir(path: impl AsRef<Path>) -> io::Result<Vec<(PathBuf, EntryKind)>> {
    #[cfg(target_arch = "wasm32")]
    {
        Ok(vfs::global()
            .read_dir(path.as_ref())?
            .into_iter()
            .map(|(path, kind)| {
                let kind = match kind {
                    vfs::Kind::File => EntryKind::File,
                    vfs::Kind::Dir => EntryKind::Dir,
                };
                (path, kind)
            })
            .collect())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut entries = Vec::new();
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let kind = match entry.file_type() {
                Ok(t) if t.is_symlink() => EntryKind::Other,
                Ok(t) if t.is_file() => EntryKind::File,
                Ok(t) if t.is_dir() => EntryKind::Dir,
                _ => EntryKind::Other,
            };
            entries.push((entry.path(), kind));
        }
        Ok(entries)
    }
}

/// Exclusive advisory lock released on drop: an OS file lock natively, and a
/// heartbeat record shared between browser tabs on the web.
pub(crate) struct Lock {
    #[cfg(not(target_arch = "wasm32"))]
    _file: fs::File,
    #[cfg(target_arch = "wasm32")]
    path: PathBuf,
}

/// Identity of this browser tab for heartbeat locks.
#[cfg(target_arch = "wasm32")]
fn tab_id() -> &'static str {
    static ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ID.get_or_init(|| uuid::Uuid::new_v4().to_string())
}

impl Lock {
    /// Try to take the lock without waiting; `WouldBlock` means another holder.
    pub(crate) fn try_acquire(path: &Path) -> io::Result<Self> {
        #[cfg(target_arch = "wasm32")]
        {
            vfs::global().try_lock(path, tab_id(), vfs::now_ms())?;
            Ok(Self {
                path: path.to_path_buf(),
            })
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(path)?;
            file.try_lock().map_err(io::Error::from)?;
            Ok(Self { _file: file })
        }
    }

    /// Keep a browser lock alive; a no-op for OS locks.
    pub(crate) fn refresh(&self) {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = vfs::global().try_lock(&self.path, tab_id(), vfs::now_ms());
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for Lock {
    fn drop(&mut self) {
        vfs::global().unlock(&self.path, tab_id());
    }
}
