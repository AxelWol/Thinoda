//! Worker-driven asynchronous storage contract, without an executor or UI.
//!
//! These futures deliberately perform synchronous filesystem/VFS work when
//! polled. Run native futures on a dedicated worker, and web futures with
//! `spawn_local`. Cancellation is cooperative, not an OS hydration interrupt.
//! Local conditional writes serialize all saves through this adapter in this
//! process, but cannot provide CAS against external programs or other tabs:
//! they can write between our revision check and rename/store replacement.
//! A bounded content sample is change detection, not a full-file integrity hash.

use crate::{fs, vfs};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    io,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StorageId(pub String);

impl StorageId {
    /// Lossless native path identity, including non-Unicode names. Does not
    /// stat or open the path. Provider IDs other than local IDs remain opaque.
    pub fn from_path(path: &Path) -> Self {
        let path = fs::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            Self(format!(
                "path:windows:{}",
                path.as_os_str()
                    .encode_wide()
                    .map(|unit| format!("{unit:04x}"))
                    .collect::<String>()
            ))
        }
        #[cfg(all(unix, not(target_arch = "wasm32")))]
        {
            use std::os::unix::ffi::OsStrExt;
            Self(format!("path:unix:{}", hex(path.as_os_str().as_bytes())))
        }
        #[cfg(target_arch = "wasm32")]
        {
            Self(vfs::normalize(&path))
        }
        #[cfg(not(any(windows, unix, target_arch = "wasm32")))]
        {
            Self(path.to_string_lossy().into_owned())
        }
    }

    /// Decode a local path ID. Plain Unicode paths are accepted for VFS and
    /// compatibility; cloud adapters must not interpret their IDs as paths.
    pub fn to_path(&self) -> Result<PathBuf, StorageError> {
        #[cfg(windows)]
        if let Some(text) = self.0.strip_prefix("path:windows:") {
            use std::os::windows::ffi::OsStringExt;
            if text.len() % 4 != 0 || !text.is_ascii() {
                return Err(invalid_id());
            }
            let units = text
                .as_bytes()
                .chunks(4)
                .map(|chunk| {
                    u16::from_str_radix(std::str::from_utf8(chunk).unwrap_or(""), 16)
                        .map_err(|_| invalid_id())
                })
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(std::ffi::OsString::from_wide(&units).into());
        }
        #[cfg(all(unix, not(target_arch = "wasm32")))]
        if let Some(text) = self.0.strip_prefix("path:unix:") {
            use std::os::unix::ffi::OsStringExt;
            if text.len() % 2 != 0 || !text.is_ascii() {
                return Err(invalid_id());
            }
            let bytes = text
                .as_bytes()
                .chunks(2)
                .map(|chunk| {
                    u8::from_str_radix(std::str::from_utf8(chunk).unwrap_or(""), 16)
                        .map_err(|_| invalid_id())
                })
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(std::ffi::OsString::from_vec(bytes).into());
        }
        if self.0.starts_with("path:") {
            return Err(invalid_id());
        }
        Ok(PathBuf::from(&self.0))
    }
}

fn invalid_id() -> StorageError {
    io::Error::new(io::ErrorKind::InvalidInput, "invalid local storage ID").into()
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Revision(pub String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExpectedRevision {
    Missing,
    Match(Revision),
    /// Only for an explicit user-approved overwrite, never an ordinary save.
    Any,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Availability {
    Local,
    OnlineOnly,
    Downloading,
    Error(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: StorageId,
    pub name: String,
    pub size: u64,
    pub revision: Option<Revision>,
    pub availability: Availability,
    pub is_dir: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadResult {
    pub bytes: Vec<u8>,
    pub revision: Revision,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub placeholders: bool,
    pub revisions: bool,
    pub versions: bool,
    pub share_links: bool,
    /// Caller-driven polling, not an automatically running OS subscription.
    pub watch: bool,
}

#[derive(Clone, Debug, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    pub fn check(&self) -> Result<(), StorageError> {
        if self.is_cancelled() {
            Err(StorageError::Cancelled)
        } else {
            Ok(())
        }
    }

    fn check_io(&self) -> io::Result<()> {
        if self.is_cancelled() {
            Err(io::Error::new(io::ErrorKind::Interrupted, CancelledIo))
        } else {
            Ok(())
        }
    }
}

#[derive(Debug)]
struct CancelledIo;

impl std::fmt::Display for CancelledIo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("storage cancelled")
    }
}

impl std::error::Error for CancelledIo {}

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("storage operation cancelled")]
    Cancelled,
    #[error("storage revision conflict")]
    Conflict { actual: Option<Revision> },
    #[error("storage operation unsupported")]
    Unsupported,
    #[error("remote storage failed: {0}")]
    Remote(String),
}

#[cfg(not(target_arch = "wasm32"))]
pub type StorageFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, StorageError>> + Send + 'a>>;
#[cfg(target_arch = "wasm32")]
pub type StorageFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, StorageError>> + 'a>>;

pub trait StorageProvider: Send + Sync {
    fn capabilities(&self) -> Capabilities;
    fn list<'a>(
        &'a self,
        location: &'a StorageId,
        cancel: CancelToken,
    ) -> StorageFuture<'a, Vec<Entry>>;
    fn read<'a>(&'a self, id: &'a StorageId, cancel: CancelToken) -> StorageFuture<'a, ReadResult>;
    fn write<'a>(
        &'a self,
        id: &'a StorageId,
        bytes: &'a [u8],
        expected: ExpectedRevision,
        cancel: CancelToken,
    ) -> StorageFuture<'a, Revision>;
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Convert the exact fingerprint acquired by the read worker. Never fingerprint
/// a path again to derive the revision of bytes already loaded by the app.
pub fn revision_from_fingerprint(fingerprint: &fs::FileFingerprint) -> Revision {
    Revision(format!(
        "local-v1:{}",
        hex(&Sha256::digest(fingerprint.revision_key()))
    ))
}

fn checked<T>(result: io::Result<T>, _cancel: &CancelToken) -> Result<T, StorageError> {
    result.map_err(|error| {
        if error
            .get_ref()
            .is_some_and(|source| source.is::<CancelledIo>())
        {
            StorageError::Cancelled
        } else if error.kind() == io::ErrorKind::Unsupported {
            StorageError::Unsupported
        } else {
            StorageError::Io(error)
        }
    })
}

fn expect_revision(
    expected: &ExpectedRevision,
    actual: Option<Revision>,
) -> Result<(), StorageError> {
    match expected {
        ExpectedRevision::Any => Ok(()),
        ExpectedRevision::Missing if actual.is_none() => Ok(()),
        ExpectedRevision::Match(revision) if actual.as_ref() == Some(revision) => Ok(()),
        _ => Err(StorageError::Conflict { actual }),
    }
}

// Serialize even independently constructed adapters and path aliases. This
// gate is process-local; it is neither a filesystem nor cross-tab CAS.
static SAVES: Mutex<()> = Mutex::new(());

fn save_gate(cancel: &CancelToken) -> Result<MutexGuard<'static, ()>, StorageError> {
    #[cfg(target_arch = "wasm32")]
    {
        cancel.check()?;
        SAVES.try_lock().map_err(|error| {
            io::Error::other(format!("storage save gate unavailable: {error}")).into()
        })
    }
    #[cfg(not(target_arch = "wasm32"))]
    loop {
        cancel.check()?;
        match SAVES.try_lock() {
            Ok(guard) => return Ok(guard),
            Err(std::sync::TryLockError::Poisoned(_)) => {
                return Err(io::Error::other("storage save gate poisoned").into());
            }
            Err(std::sync::TryLockError::WouldBlock) => {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
}

type OwnSaves = Arc<Mutex<BTreeMap<StorageId, Revision>>>;

fn record_save(saves: &OwnSaves, id: &StorageId, revision: &Revision) {
    if let Ok(mut saves) = saves.lock() {
        saves.insert(id.clone(), revision.clone());
    }
}

#[derive(Clone, Default)]
pub struct LocalStorageProvider {
    keep_backup: bool,
    backup_directory: Option<PathBuf>,
    own_saves: OwnSaves,
}

impl LocalStorageProvider {
    pub fn new(keep_backup: bool) -> Self {
        Self {
            keep_backup,
            ..Self::default()
        }
    }

    /// Keep durable backups outside a synchronized tree. Requires `new(true)`;
    /// `new(false)` still disables backups. The app must validate this directory
    /// against all detected cloud roots (including path aliases). At write time
    /// we additionally reject the source directory and its descendants.
    ///
    /// The backup filename is SHA-256 of the absolute, lossless source path ID
    /// plus `.bak`, so identically named documents do not overwrite each other.
    /// Atomic same-directory destination replacement remains the save strategy;
    /// preservation of provider history/sharing still needs the live matrix.
    pub fn with_backup_directory(mut self, directory: PathBuf) -> Self {
        self.backup_directory = Some(directory);
        self
    }

    fn backup_path(&self, source: &Path) -> Result<Option<PathBuf>, StorageError> {
        if !self.keep_backup {
            return Ok(None);
        }
        let Some(directory) = &self.backup_directory else {
            return Ok(Some(fs::backup_path(source)));
        };
        let source = fs::absolute(source)?;
        let parent = source.parent().ok_or_else(invalid_id)?;
        let directory = fs::absolute(directory)?;
        // Resolve existing directories to reject simple symlink/junction aliases
        // too. The app's sync-root validation remains the authoritative policy.
        let resolved_parent = fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
        let resolved_directory = fs::canonicalize(&directory).unwrap_or_else(|_| directory.clone());
        if directory_within(&directory, parent)
            || directory_within(&resolved_directory, &resolved_parent)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "backup directory must be outside the source directory tree",
            )
            .into());
        }
        let key = hex(&Sha256::digest(StorageId::from_path(&source).0.as_bytes()));
        Ok(Some(directory.join(format!("{key}.bak"))))
    }

    pub fn watch(&self, location: StorageId, debounce: Duration) -> PollWatcher {
        PollWatcher::with_saves(location, debounce, self.own_saves.clone())
    }
}

fn directory_within(directory: &Path, parent: &Path) -> bool {
    #[cfg(windows)]
    {
        // Windows path components are normally case-insensitive. Existing
        // aliases were resolved above; no lossy conversion is used for IDs.
        PathBuf::from(directory.to_string_lossy().to_lowercase())
            .starts_with(PathBuf::from(parent.to_string_lossy().to_lowercase()))
    }
    #[cfg(not(windows))]
    directory.starts_with(parent)
}

impl StorageProvider for LocalStorageProvider {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            placeholders: cfg!(any(windows, target_os = "macos")),
            revisions: true,
            watch: true,
            ..Capabilities::default()
        }
    }

    fn list<'a>(
        &'a self,
        location: &'a StorageId,
        cancel: CancelToken,
    ) -> StorageFuture<'a, Vec<Entry>> {
        Box::pin(async move {
            cancel.check()?;
            let mut entries = Vec::new();
            for (path, kind) in fs::list_dir(location.to_path()?)? {
                cancel.check()?;
                if kind == fs::EntryKind::Other {
                    continue;
                }
                let mut entry = Entry {
                    id: StorageId::from_path(&path),
                    name: path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    size: 0,
                    revision: None,
                    availability: Availability::Local,
                    is_dir: kind == fs::EntryKind::Dir,
                };
                if !entry.is_dir {
                    match checked(
                        fs::fingerprint_checked(&path, || cancel.check_io()),
                        &cancel,
                    ) {
                        Ok(fingerprint) => {
                            entry.size = fingerprint.len;
                            if fingerprint.online_only {
                                entry.availability = Availability::OnlineOnly;
                            } else {
                                entry.revision = Some(revision_from_fingerprint(&fingerprint));
                            }
                        }
                        Err(StorageError::Cancelled) => return Err(StorageError::Cancelled),
                        Err(error) => entry.availability = Availability::Error(error.to_string()),
                    }
                }
                entries.push(entry);
            }
            cancel.check()?;
            entries.sort_by(|a, b| a.id.cmp(&b.id));
            Ok(entries)
        })
    }

    fn read<'a>(&'a self, id: &'a StorageId, cancel: CancelToken) -> StorageFuture<'a, ReadResult> {
        Box::pin(async move {
            let (bytes, fingerprint) = checked(
                fs::read_checked(&id.to_path()?, || cancel.check_io()),
                &cancel,
            )?;
            cancel.check()?;
            Ok(ReadResult {
                bytes,
                revision: revision_from_fingerprint(&fingerprint),
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
            let _gate = save_gate(&cancel)?;
            let path = id.to_path()?;
            let mut online_only = false;
            let actual = match checked(
                fs::fingerprint_checked(&path, || cancel.check_io()),
                &cancel,
            ) {
                Ok(fingerprint) => {
                    online_only = fingerprint.online_only;
                    Some(revision_from_fingerprint(&fingerprint))
                }
                Err(StorageError::Io(error)) if error.kind() == io::ErrorKind::NotFound => None,
                Err(error) => return Err(error),
            };
            expect_revision(&expected, actual)?;
            // Only explicit overwrite without a backup may replace a
            // placeholder without first explicitly reading its data. Missing
            // still reports a conflict for an existing placeholder.
            if online_only && (self.keep_backup || expected != ExpectedRevision::Any) {
                return Err(StorageError::Unsupported);
            }
            let backup = self.backup_path(&path)?;
            let fingerprint = checked(
                fs::atomic_write_snapshot_with_backup_checked(
                    &path,
                    bytes,
                    backup.as_deref(),
                    || cancel.check_io(),
                ),
                &cancel,
            )?;
            // No cancellation checks or path reads after commit. The revision
            // belongs to the saved handle, even if an external writer replaces
            // the path immediately afterwards.
            let revision = revision_from_fingerprint(&fingerprint);
            record_save(&self.own_saves, id, &revision);
            Ok(revision)
        })
    }
}

/// Target-independent VFS adapter for tests and the synchronous browser fallback.
/// Concurrent direct Vfs users and other tabs are external writers, not CAS.
#[derive(Clone)]
pub struct VfsStorageProvider {
    vfs: Arc<vfs::Vfs>,
    keep_backup: bool,
    own_saves: OwnSaves,
}

impl VfsStorageProvider {
    pub fn new(vfs: Arc<vfs::Vfs>, keep_backup: bool) -> Self {
        Self {
            vfs,
            keep_backup,
            own_saves: OwnSaves::default(),
        }
    }

    pub fn watch(&self, location: StorageId, debounce: Duration) -> PollWatcher {
        PollWatcher::with_saves(location, debounce, self.own_saves.clone())
    }

    fn revision(&self, path: &Path, bytes: &[u8]) -> Result<Revision, StorageError> {
        let meta = self.vfs.metadata(path)?;
        Ok(Revision(format!(
            "vfs-v1:{}:{}:{}:{}",
            vfs::normalize(path),
            meta.modified_ms,
            meta.len,
            hex(&fs::sample_hash(bytes))
        )))
    }

    fn snapshot(&self, path: &Path, cancel: &CancelToken) -> Result<ReadResult, StorageError> {
        cancel.check()?;
        let before = self.vfs.metadata(path)?;
        let bytes = self.vfs.read(path)?;
        // The legacy VFS has no transaction/generation API. Check metadata and
        // bytes twice; direct concurrent users still leave a residual race.
        cancel.check()?;
        let revision = self.revision(path, &bytes)?;
        if before != self.vfs.metadata(path)? || bytes != self.vfs.read(path)? {
            return Err(
                io::Error::new(io::ErrorKind::WouldBlock, "VFS changed while reading").into(),
            );
        }
        Ok(ReadResult { bytes, revision })
    }
}

impl StorageProvider for VfsStorageProvider {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            revisions: true,
            watch: true,
            ..Capabilities::default()
        }
    }

    fn list<'a>(
        &'a self,
        location: &'a StorageId,
        cancel: CancelToken,
    ) -> StorageFuture<'a, Vec<Entry>> {
        Box::pin(async move {
            cancel.check()?;
            let mut entries = Vec::new();
            for (path, kind) in self.vfs.read_dir(&location.to_path()?)? {
                cancel.check()?;
                let is_dir = kind == vfs::Kind::Dir;
                let read = if is_dir {
                    None
                } else {
                    Some(self.snapshot(&path, &cancel)?)
                };
                entries.push(Entry {
                    // VFS IDs are normalized virtual paths on every target.
                    id: StorageId(vfs::normalize(&path)),
                    name: path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    size: read.as_ref().map_or(0, |read| read.bytes.len() as u64),
                    revision: read.map(|read| read.revision),
                    availability: Availability::Local,
                    is_dir,
                });
            }
            cancel.check()?;
            Ok(entries)
        })
    }

    fn read<'a>(&'a self, id: &'a StorageId, cancel: CancelToken) -> StorageFuture<'a, ReadResult> {
        Box::pin(async move { self.snapshot(&id.to_path()?, &cancel) })
    }

    fn write<'a>(
        &'a self,
        id: &'a StorageId,
        bytes: &'a [u8],
        expected: ExpectedRevision,
        cancel: CancelToken,
    ) -> StorageFuture<'a, Revision> {
        Box::pin(async move {
            let _gate = save_gate(&cancel)?;
            let path = id.to_path()?;
            let actual = match self.snapshot(&path, &cancel) {
                Ok(read) => Some(read.revision),
                Err(StorageError::Io(error)) if error.kind() == io::ErrorKind::NotFound => None,
                Err(error) => return Err(error),
            };
            expect_revision(&expected, actual)?;
            if self.keep_backup && self.vfs.is_file(&path) {
                let previous = self.snapshot(&path, &cancel)?.bytes;
                cancel.check()?;
                self.vfs
                    .write(&fs::backup_path(&path), &previous, vfs::now_ms())?;
            }
            cancel.check()?;
            self.vfs.write(&path, bytes, vfs::now_ms())?;
            let revision = self.revision(&path, bytes)?;
            record_save(
                &self.own_saves,
                &StorageId(vfs::normalize(&path)),
                &revision,
            );
            Ok(revision)
        })
    }
}

/// A final directory state change; `entry == None` means removal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageChange {
    pub id: StorageId,
    pub entry: Option<Entry>,
}

/// Debounced, caller-driven polling. First poll establishes a baseline; invoke
/// later polls on the worker with monotonic elapsed time. No thread is spawned.
/// Transient steps are coalesced per ID, deletions included. Reverting to the
/// baseline emits nothing. Own-save suppression consumes only an exact revision.
pub struct PollWatcher {
    location: StorageId,
    debounce: Duration,
    baseline: Option<BTreeMap<StorageId, Entry>>,
    pending: BTreeMap<StorageId, (Option<Entry>, Duration)>,
    own_saves: OwnSaves,
    consumed_saves: BTreeMap<StorageId, Revision>,
}

impl PollWatcher {
    pub fn new(location: StorageId, debounce: Duration) -> Self {
        Self::with_saves(location, debounce, OwnSaves::default())
    }

    fn with_saves(location: StorageId, debounce: Duration, own_saves: OwnSaves) -> Self {
        Self {
            location,
            debounce,
            baseline: None,
            pending: BTreeMap::new(),
            own_saves,
            consumed_saves: BTreeMap::new(),
        }
    }

    /// For adapters not constructed through Local/VFS `watch`.
    pub fn suppress_own_save(&mut self, id: StorageId, revision: Revision) {
        self.consumed_saves.remove(&id);
        record_save(&self.own_saves, &id, &revision);
    }

    pub fn poll<'a>(
        &'a mut self,
        provider: &'a dyn StorageProvider,
        now: Duration,
        cancel: CancelToken,
    ) -> StorageFuture<'a, Vec<StorageChange>> {
        Box::pin(async move {
            let entries = match provider.list(&self.location, cancel.clone()).await {
                Ok(entries) => entries,
                // VFS directories are implicit; deleting their last file
                // removes the directory too. Native folder deletion is also
                // represented as removal of the previously observed children.
                Err(StorageError::Io(error))
                    if error.kind() == io::ErrorKind::NotFound && self.baseline.is_some() =>
                {
                    Vec::new()
                }
                Err(error) => return Err(error),
            };
            cancel.check()?;
            let current: BTreeMap<_, _> = entries
                .into_iter()
                .map(|entry| (entry.id.clone(), entry))
                .collect();
            let Some(baseline) = &mut self.baseline else {
                if let Ok(saves) = self.own_saves.lock() {
                    for (id, revision) in saves.iter() {
                        if current.get(id).and_then(|entry| entry.revision.as_ref())
                            == Some(revision)
                        {
                            self.consumed_saves.insert(id.clone(), revision.clone());
                        }
                    }
                }
                self.baseline = Some(current);
                return Ok(Vec::new());
            };
            let ids: BTreeSet<_> = baseline
                .keys()
                .chain(current.keys())
                .chain(self.pending.keys())
                .cloned()
                .collect();
            let saves = self
                .own_saves
                .lock()
                .map_err(|_| io::Error::other("storage watcher poisoned"))?;
            let mut changes = Vec::new();
            for id in ids {
                let entry = current.get(&id).cloned();
                let own = entry
                    .as_ref()
                    .and_then(|e| e.revision.as_ref())
                    .is_some_and(|revision| {
                        saves.get(&id) == Some(revision)
                            && self.consumed_saves.get(&id) != Some(revision)
                    });
                if own {
                    if let Some(revision) = saves.get(&id) {
                        self.consumed_saves.insert(id.clone(), revision.clone());
                    }
                    if let Some(entry) = entry {
                        baseline.insert(id.clone(), entry);
                    }
                    self.pending.remove(&id);
                    continue;
                }
                if baseline.get(&id) == entry.as_ref() {
                    self.pending.remove(&id);
                    continue;
                }
                let pending = self
                    .pending
                    .entry(id.clone())
                    .or_insert((entry.clone(), now));
                if pending.0 != entry {
                    *pending = (entry.clone(), now);
                }
                if now.saturating_sub(pending.1) >= self.debounce {
                    match &entry {
                        Some(entry) => {
                            baseline.insert(id.clone(), entry.clone());
                        }
                        None => {
                            baseline.remove(&id);
                        }
                    }
                    self.pending.remove(&id);
                    changes.push(StorageChange { id, entry });
                }
            }
            Ok(changes)
        })
    }
}
