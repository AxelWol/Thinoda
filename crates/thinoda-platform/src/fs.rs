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
    atomic_write_checked(path.as_ref(), bytes, keep_backup, || Ok(()))
}

/// Cooperative cancellation for worker-driven saves. `check` is called before
/// each chunk and immediately before replacement, never after the save commits.
/// A backup may have committed if a later cancellation stops the destination.
pub(crate) fn atomic_write_checked(
    path: &Path,
    bytes: &[u8],
    keep_backup: bool,
    mut check: impl FnMut() -> io::Result<()>,
) -> io::Result<()> {
    atomic_write_snapshot_checked(path, bytes, keep_backup, &mut check).map(|_| ())
}

/// Return a fingerprint of the actual saved handle/bytes, not a later path read.
pub(crate) fn atomic_write_snapshot_checked(
    path: &Path,
    bytes: &[u8],
    keep_backup: bool,
    mut check: impl FnMut() -> io::Result<()>,
) -> io::Result<FileFingerprint> {
    let backup = keep_backup.then(|| backup_path(path));
    atomic_write_snapshot_with_backup_checked(path, bytes, backup.as_deref(), &mut check)
}

/// Durable atomic replacement with an optional separately located backup.
/// Callers validate that the backup destination is outside any sync root.
pub(crate) fn atomic_write_snapshot_with_backup_checked(
    path: &Path,
    bytes: &[u8],
    backup: Option<&Path>,
    mut check: impl FnMut() -> io::Result<()>,
) -> io::Result<FileFingerprint> {
    check()?;
    #[cfg(target_arch = "wasm32")]
    {
        let vfs = vfs::global();
        if let Some(backup) = backup {
            match vfs.read(path) {
                Ok(previous) => {
                    check()?;
                    vfs.write(backup, &previous, vfs::now_ms())?;
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        check()?;
        let modified_ms = vfs::now_ms();
        vfs.write(path, bytes, modified_ms)?;
        Ok(FileFingerprint {
            len: bytes.len() as u64,
            modified: web_time::UNIX_EPOCH + std::time::Duration::from_millis(modified_ms),
            identity: vfs::normalize(path),
            content_hash: Some(sample_hash(bytes)),
            online_only: false,
            stamp: modified_ms.to_string(),
            path: path.to_path_buf(),
        })
    }
    #[cfg(not(target_arch = "wasm32"))]
    native_atomic_write(path, bytes, backup, &mut check)
}

#[cfg(not(target_arch = "wasm32"))]
fn native_atomic_write(
    path: &Path,
    bytes: &[u8],
    backup: Option<&Path>,
    mut check: &mut dyn FnMut() -> io::Result<()>,
) -> io::Result<FileFingerprint> {
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
    for chunk in bytes.chunks(64 * 1024) {
        check()?;
        file.write_all(chunk)?;
    }
    file.sync_all()?;
    if let Some(backup) = backup {
        // Backup reads must not silently hydrate an online-only destination.
        match online_only(path) {
            Ok(true) => {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "cannot back up an online-only file without an explicit read",
                ));
            }
            Ok(false) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        match read_checked(path, &mut check) {
            Ok((previous, _)) => {
                atomic_write_checked(backup, &previous, false, &mut check)?;
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    check()?;
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
    file_fingerprint(path, &file, Some(sample_hash(bytes)))
}

/// File identity, metadata and bounded SHA-256 content sample.
///
/// At most 192 KiB is hashed (first/middle/last 64 KiB for larger files).
/// Metadata changes anywhere in the file are detected, but a deliberate edit
/// outside those samples with restored metadata is not a filesystem CAS.
/// Online-only files are metadata-only and are never opened for data here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileFingerprint {
    pub len: u64,
    pub modified: SystemTime,
    pub identity: String,
    pub content_hash: Option<[u8; 32]>,
    pub online_only: bool,
    stamp: String,
    path: PathBuf,
}

/// Record identity and a bounded hash without hydrating placeholders.
pub fn fingerprint(path: impl AsRef<Path>) -> io::Result<FileFingerprint> {
    fingerprint_checked(path.as_ref(), || Ok(()))
}

pub(crate) fn fingerprint_checked(
    path: &Path,
    mut check: impl FnMut() -> io::Result<()>,
) -> io::Result<FileFingerprint> {
    check()?;
    #[cfg(target_arch = "wasm32")]
    {
        let meta = vfs::global().metadata(path)?;
        Ok(FileFingerprint {
            len: meta.len,
            modified: web_time::UNIX_EPOCH + std::time::Duration::from_millis(meta.modified_ms),
            identity: vfs::normalize(path),
            content_hash: Some(sample_hash(&vfs::global().read(path)?)),
            online_only: false,
            stamp: meta.modified_ms.to_string(),
            path: path.to_path_buf(),
        })
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let metadata = fs::metadata(path)?;
        if metadata_online_only(&metadata) {
            return path_metadata_fingerprint(path);
        }
        let mut file = fs::File::open(path)?;
        let before = file_fingerprint(path, &file, None)?;
        // A provider can change state between stat and open.
        if before.online_only {
            return Ok(before);
        }
        let hash = hash_file(&mut file, before.len, &mut check)?;
        let mut after = file_fingerprint(path, &file, None)?;
        let named = path_metadata_fingerprint(path)?;
        if before != after || after != named {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "file changed while hashing",
            ));
        }
        after.content_hash = Some(hash);
        Ok(after)
    }
}

/// Attribute-only placeholder detection; no data open or hydration.
pub fn online_only(path: impl AsRef<Path>) -> io::Result<bool> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = path;
        Ok(false)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Ok(metadata_online_only(&fs::metadata(path)?))
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn metadata_online_only(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // OFFLINE, RECALL_ON_OPEN and RECALL_ON_DATA_ACCESS.
        metadata.file_attributes() & (0x1000 | 0x40000 | 0x400000) != 0
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::macos::fs::MetadataExt;
        const SF_DATALESS: u32 = 0x40000000;
        metadata.st_flags() & SF_DATALESS != 0
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = metadata;
        false
    }
}

#[cfg(windows)]
fn metadata_file(path: &Path) -> io::Result<fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    // FILE_READ_ATTRIBUTES and FILE_FLAG_OPEN_REPARSE_POINT. No data access,
    // including for recall-on-open placeholders.
    OpenOptions::new()
        .access_mode(0x80)
        .custom_flags(0x00200000)
        .open(path)
}

#[cfg(not(target_arch = "wasm32"))]
fn path_metadata_fingerprint(path: &Path) -> io::Result<FileFingerprint> {
    #[cfg(windows)]
    {
        file_fingerprint(path, &metadata_file(path)?, None)
    }
    #[cfg(not(windows))]
    metadata_fingerprint(path, &fs::metadata(path)?, None)
}

#[cfg(not(target_arch = "wasm32"))]
fn file_fingerprint(
    path: &Path,
    file: &fs::File,
    content_hash: Option<[u8; 32]>,
) -> io::Result<FileFingerprint> {
    let metadata = file.metadata()?;
    #[cfg(windows)]
    {
        let mut fingerprint = metadata_fingerprint(path, &metadata, content_hash)?;
        fingerprint.identity = windows_file_identity(file)?;
        Ok(fingerprint)
    }
    #[cfg(not(windows))]
    metadata_fingerprint(path, &metadata, content_hash)
}

#[cfg(not(target_arch = "wasm32"))]
fn metadata_fingerprint(
    path: &Path,
    metadata: &fs::Metadata,
    content_hash: Option<[u8; 32]>,
) -> io::Result<FileFingerprint> {
    #[cfg(unix)]
    let identity = {
        use std::os::unix::fs::MetadataExt;
        format!("{}:{}", metadata.dev(), metadata.ino())
    };
    #[cfg(windows)]
    let identity = String::new(); // Filled from the live handle by file_fingerprint.
    #[cfg(not(any(unix, windows)))]
    let identity = format!("{:?}", metadata.created()?);
    #[cfg(unix)]
    let stamp = {
        use std::os::unix::fs::MetadataExt;
        format!("{}:{}", metadata.ctime(), metadata.ctime_nsec())
    };
    #[cfg(windows)]
    let stamp = {
        use std::os::windows::fs::MetadataExt;
        format!(
            "{}:{}",
            metadata.creation_time(),
            metadata.file_attributes()
        )
    };
    #[cfg(not(any(unix, windows)))]
    let stamp = String::new();
    Ok(FileFingerprint {
        len: metadata.len(),
        modified: metadata.modified()?,
        identity,
        content_hash,
        online_only: metadata_online_only(metadata),
        stamp,
        path: path.to_path_buf(),
    })
}

#[cfg(windows)]
fn windows_file_identity(file: &fs::File) -> io::Result<String> {
    use std::{ffi::c_void, os::windows::io::AsRawHandle};
    // Stable std MetadataExt does not expose file IDs. This small kernel32
    // binding avoids adding a dependency/feature for one metadata-only call.
    #[repr(C)]
    #[derive(Default)]
    struct FileInfo {
        attributes: u32,
        created: [u32; 2],
        accessed: [u32; 2],
        written: [u32; 2],
        volume: u32,
        size_high: u32,
        size_low: u32,
        links: u32,
        index_high: u32,
        index_low: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandle(handle: *mut c_void, info: *mut FileInfo) -> i32;
    }
    let mut info = FileInfo::default();
    // SAFETY: file owns a live handle; info is a correctly sized/aligned,
    // writable BY_HANDLE_FILE_INFORMATION and is not retained by Windows.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(format!(
        "{}:{}:{}",
        info.volume, info.index_high, info.index_low
    ))
}

const HASH_CHUNK: usize = 64 * 1024;

fn sample_ranges(len: usize) -> Vec<(usize, usize)> {
    if len <= 3 * HASH_CHUNK {
        vec![(0, len)]
    } else {
        vec![
            (0, HASH_CHUNK),
            (len / 2 - HASH_CHUNK / 2, HASH_CHUNK),
            (len - HASH_CHUNK, HASH_CHUNK),
        ]
    }
}

pub(crate) fn sample_hash(bytes: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update((bytes.len() as u64).to_le_bytes());
    for (offset, len) in sample_ranges(bytes.len()) {
        hash.update((offset as u64).to_le_bytes());
        hash.update(&bytes[offset..offset + len]);
    }
    hash.finalize().into()
}

#[cfg(not(target_arch = "wasm32"))]
fn hash_file(
    file: &mut fs::File,
    len: u64,
    mut check: impl FnMut() -> io::Result<()>,
) -> io::Result<[u8; 32]> {
    use sha2::{Digest, Sha256};
    use std::io::{Read, Seek, SeekFrom};
    let len = usize::try_from(len)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "file too large"))?;
    let mut hash = Sha256::new();
    hash.update((len as u64).to_le_bytes());
    let mut buffer = vec![0; HASH_CHUNK];
    for (offset, len) in sample_ranges(len) {
        hash.update((offset as u64).to_le_bytes());
        file.seek(SeekFrom::Start(offset as u64))?;
        let mut remaining = len;
        while remaining != 0 {
            check()?;
            let chunk = remaining.min(buffer.len());
            file.read_exact(&mut buffer[..chunk])?;
            hash.update(&buffer[..chunk]);
            remaining -= chunk;
        }
    }
    check()?;
    Ok(hash.finalize().into())
}

/// Explicit cancellable read. Only this primitive may read placeholder data.
/// OS hydration/open and a single read syscall cannot be interrupted by this
/// cooperative token; cancellation is checked before and between calls.
pub(crate) fn read_checked(
    path: &Path,
    mut check: impl FnMut() -> io::Result<()>,
) -> io::Result<(Vec<u8>, FileFingerprint)> {
    check()?;
    #[cfg(target_arch = "wasm32")]
    {
        let before = vfs::global().metadata(path)?;
        let bytes = vfs::global().read(path)?;
        check()?;
        let after = vfs::global().metadata(path)?;
        if before != after || bytes != vfs::global().read(path)? {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "VFS changed while reading",
            ));
        }
        let fingerprint = FileFingerprint {
            len: after.len,
            modified: web_time::UNIX_EPOCH + std::time::Duration::from_millis(after.modified_ms),
            identity: vfs::normalize(path),
            content_hash: Some(sample_hash(&bytes)),
            online_only: false,
            stamp: after.modified_ms.to_string(),
            path: path.to_path_buf(),
        };
        Ok((bytes, fingerprint))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::io::Read;
        let mut file = fs::File::open(path)?;
        check()?;
        let before = file_fingerprint(path, &file, None)?;
        let mut bytes = Vec::new();
        let mut chunk = vec![0; HASH_CHUNK];
        loop {
            check()?;
            let n = file.read(&mut chunk)?;
            if n == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..n]);
        }
        check()?;
        let mut after = file_fingerprint(path, &file, None)?;
        // Hydration may clear placeholder flags. Compare identity, length and
        // modification time, and ensure the path still names the read handle.
        let named = path_metadata_fingerprint(path)?;
        if before.identity != after.identity
            || before.len != after.len
            || before.modified != after.modified
            || (!before.online_only && before.stamp != after.stamp)
            || after != named
            || bytes.len() as u64 != after.len
        {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "file changed while reading",
            ));
        }
        after.content_hash = Some(sample_hash(&bytes));
        Ok((bytes, after))
    }
}

impl FileFingerprint {
    pub(crate) fn revision_key(&self) -> String {
        format!(
            "{:?}:{:?}:{}:{}:{:?}:{}",
            self.identity, self.modified, self.len, self.stamp, self.content_hash, self.online_only
        )
    }

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
        let conflict = suffix.starts_with(" (")
            && suffix.ends_with(')')
            && [
                "conflicted copy",
                "conflict copy",
                "conflict ",
                "konfliktkopie",
                "conflicting copy",
            ]
            .iter()
            .any(|marker| suffix.contains(marker));
        // Nextcloud/ownCloud also use a `_conflict-YYYYMMDD-HHMMSS` suffix.
        let timestamped = suffix.strip_prefix("_conflict-").is_some_and(|stamp| {
            stamp.len() >= 8
                && stamp.chars().all(|c| c.is_ascii_digit() || c == '-')
                && stamp.chars().filter(|c| c.is_ascii_digit()).count() >= 8
        });
        let numbered = suffix
            .strip_prefix(' ')
            .is_some_and(|number| number.parse::<u32>().is_ok_and(|n| n >= 2));
        let machine = suffix.strip_prefix('-').is_some_and(|name| {
            (!name.is_empty() && (name.starts_with("desktop-") || name.starts_with("laptop-")))
                || computer
                    .as_deref()
                    .is_some_and(|c| !c.is_empty() && name == c)
        });
        if conflict || timestamped || numbered || machine {
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
    {
        let path = path.as_ref();
        if online_only(path)? {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "online-only file requires an explicit cancellable storage read",
            ));
        }
        // Attribute check again on the opened handle narrows a provider-state
        // race. An external provider may still change state between OS calls.
        let mut file = fs::File::open(path)?;
        if metadata_online_only(&file.metadata()?) {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "online-only file requires an explicit cancellable storage read",
            ));
        }
        use std::io::Read;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok(bytes)
    }
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

/// Exclusive advisory lock explicitly released on drop: an OS file lock natively,
/// and a heartbeat record shared between browser tabs on the web.
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

#[cfg(not(target_arch = "wasm32"))]
impl Drop for Lock {
    fn drop(&mut self) {
        // A forked child can retain the open file description until exec/exit.
        // Closing our descriptor alone would keep that session locked meanwhile.
        if let Err(error) = self._file.unlock() {
            log::warn!("Recovery lock release failed: {error}");
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod storage_tests {
    use super::*;
    use std::io::{Seek, SeekFrom};

    #[test]
    fn cancellable_read_checks_between_chunks() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("large");
        fs::write(&path, vec![0; 4 * HASH_CHUNK]).unwrap();
        let mut checks = 0;
        let error = read_checked(&path, || {
            checks += 1;
            if checks == 4 {
                Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "test cancellation",
                ))
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        assert_eq!(checks, 4);
    }

    #[test]
    fn read_rejects_mutation_between_chunks_instead_of_pairing_bytes_with_new_revision() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("large");
        fs::write(&path, vec![0; 4 * HASH_CHUNK]).unwrap();
        let mut checks = 0;
        let error = read_checked(&path, || {
            checks += 1;
            if checks == 4 {
                let mut file = OpenOptions::new().write(true).open(&path)?;
                file.seek(SeekFrom::Start(0))?;
                file.write_all(b"modified")?;
                // Ensure detection even on coarse timestamp filesystems.
                file.set_times(fs::FileTimes::new().set_modified(
                    std::time::SystemTime::now() + std::time::Duration::from_secs(5),
                ))?;
            }
            Ok(())
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
    }

    #[test]
    fn cancelled_save_before_replacement_preserves_original_and_leaves_no_backup_or_temp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("map");
        fs::write(&path, b"old").unwrap();
        let mut checks = 0;
        let error = atomic_write_checked(&path, &vec![1; 4 * HASH_CHUNK], true, || {
            checks += 1;
            if checks == 3 {
                Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "test cancellation",
                ))
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        assert_eq!(fs::read(&path).unwrap(), b"old");
        assert!(!backup_path(&path).exists());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn bounded_hash_matches_saved_bytes_and_metadata_detects_unsampled_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("large");
        let bytes = vec![7; 16 * HASH_CHUNK];
        fs::write(&path, &bytes).unwrap();
        let before = fingerprint(&path).unwrap();
        assert_eq!(before.content_hash, Some(sample_hash(&bytes)));
        let mut file = OpenOptions::new().write(true).open(&path).unwrap();
        file.seek(SeekFrom::Start((2 * HASH_CHUNK) as u64)).unwrap();
        file.write_all(b"unsampled edit").unwrap();
        file.set_times(
            fs::FileTimes::new()
                .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(5)),
        )
        .unwrap();
        let after = fingerprint(&path).unwrap();
        assert_eq!(before.content_hash, after.content_hash);
        assert_ne!(before, after);
    }

    #[test]
    fn final_precommit_check_can_cancel_and_checks_stop_after_commit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("map");
        fs::write(&path, b"old").unwrap();
        let mut checks = 0;
        let error = atomic_write_checked(&path, b"new", false, || {
            checks += 1;
            if checks == 3 {
                Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "test cancellation",
                ))
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        assert_eq!(fs::read(&path).unwrap(), b"old");
        checks = 0;
        atomic_write_checked(&path, b"new", false, || {
            checks += 1;
            assert!(checks <= 3, "must not check cancellation after commit");
            Ok(())
        })
        .unwrap();
        assert_eq!(checks, 3);
        assert_eq!(fs::read(&path).unwrap(), b"new");
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod lock_tests {
    use super::*;

    #[test]
    fn dropping_lock_releases_it_even_with_a_duplicated_descriptor() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lock");
        let lock = Lock::try_acquire(&path).unwrap();
        let duplicate = lock._file.try_clone().unwrap();
        assert!(Lock::try_acquire(&path).is_err());
        drop(lock);
        let next = Lock::try_acquire(&path)
            .expect("a duplicated descriptor must not retain a released session lock");
        drop(duplicate);
        assert!(
            Lock::try_acquire(&path).is_err(),
            "closing the old descriptor must not release the new owner's lock"
        );
        drop(next);
        assert!(Lock::try_acquire(&path).is_ok());
    }
}
