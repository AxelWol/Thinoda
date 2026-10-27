//! Per-process crash recovery with advisory locks and coalesced snapshot writes.

use crate::{
    Error, Paths, Result,
    fs::{self, EntryKind, Lock, atomic_write},
    unix_seconds,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use thinoda_core::Workbook;

/// Document identity and recovery timestamp (Unix seconds).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SnapshotMeta {
    pub title: String,
    pub original_path: Option<PathBuf>,
    pub import_source: Option<PathBuf>,
    pub dirty: bool,
    pub saved_at: u64,
}

/// On-disk envelope; callers can serialize off the UI thread and queue the bytes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub meta: SnapshotMeta,
    pub workbook: Workbook,
}

impl Snapshot {
    /// Serialize a recovery envelope, without writing or taking a recovery lock.
    pub fn to_bytes(meta: SnapshotMeta, workbook: &Workbook) -> Result<Vec<u8>> {
        #[derive(Serialize)]
        struct Borrowed<'a> {
            meta: SnapshotMeta,
            workbook: &'a Workbook,
        }
        Ok(serde_json::to_vec(&Borrowed { meta, workbook })?)
    }
}

#[derive(Serialize)]
struct SessionInfo {
    pid: u32,
    started_at: u64,
    app_version: &'static str,
}

struct State {
    lock: Option<Lock>,
    finished: bool,
}

/// A live recovery session. Share an `Arc` with the panic hook and explicitly
/// call `finish` after all documents have closed cleanly. Dropping normally also
/// cleans up; a panic hook marks the session for preservation during unwinding.
pub struct RecoveryManager {
    session_id: String,
    session_dir: PathBuf,
    pending: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
    state: Mutex<State>,
    preserve: AtomicBool,
    closed: AtomicBool,
}

fn check_key(key: &str) -> Result<()> {
    if key.is_empty()
        || key.len() > 180
        || !key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err(Error::InvalidDocKey(key.into()));
    }
    Ok(())
}

impl RecoveryManager {
    /// Create and exclusively lock a fresh UUID-named recovery session.
    pub fn start(paths: &Paths) -> Result<Self> {
        let session_id = uuid::Uuid::new_v4().to_string();
        let session_dir = paths.data_dir.join("recovery").join(&session_id);
        fs::create_dir_all(session_dir.join("docs"))?;
        let lock = Lock::try_acquire(&session_dir.join("lock"))?;
        let manager = Self {
            session_id,
            session_dir,
            pending: Arc::default(),
            state: Mutex::new(State {
                lock: Some(lock),
                finished: false,
            }),
            preserve: AtomicBool::new(false),
            closed: AtomicBool::new(false),
        };
        let info = SessionInfo {
            pid: if cfg!(target_arch = "wasm32") {
                0
            } else {
                std::process::id()
            },
            started_at: unix_seconds(),
            app_version: crate::system::app_version(),
        };
        atomic_write(
            manager.session_dir.join("session.json"),
            &serde_json::to_vec(&info)?,
            false,
        )?;
        Ok(manager)
    }

    /// Session UUID used to group recovered documents.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// On-disk session directory, useful for diagnostics.
    pub fn session_dir(&self) -> &Path {
        &self.session_dir
    }

    /// Serialize and synchronously persist a document; replaces an older queued snapshot.
    pub fn snapshot(&self, doc_key: &str, meta: SnapshotMeta, workbook: &Workbook) -> Result<()> {
        self.queue_snapshot(doc_key, Snapshot::to_bytes(meta, workbook)?)?;
        self.flush()
    }

    /// Persist a handoff without queuing failed work that the caller may abandon.
    pub fn persist_snapshot(
        &self,
        doc_key: &str,
        meta: SnapshotMeta,
        workbook: &Workbook,
    ) -> Result<()> {
        check_key(doc_key)?;
        let bytes = Snapshot::to_bytes(meta, workbook)?;
        let state = self.state.lock().map_err(|_| Error::Poisoned)?;
        if state.finished {
            return Err(Error::Finished);
        }
        let lock = state.lock.as_ref().ok_or(Error::Suspended)?;
        lock.refresh();
        let mut pending = self.pending.lock().map_err(|_| Error::Poisoned)?;
        atomic_write(
            self.session_dir
                .join("docs")
                .join(format!("{doc_key}.json")),
            &bytes,
            false,
        )?;
        pending.remove(doc_key);
        Ok(())
    }

    /// Coalesce a serialized `Snapshot` by document key without doing filesystem I/O.
    /// Serialize with `Snapshot::to_bytes`; the latest queued value wins.
    pub fn queue_snapshot(&self, doc_key: &str, bytes: Vec<u8>) -> Result<()> {
        check_key(doc_key)?;
        let mut pending = self.pending.lock().map_err(|_| Error::Poisoned)?;
        if self.closed.load(Ordering::Acquire) {
            return Err(Error::Finished);
        }
        pending.insert(doc_key.into(), bytes);
        Ok(())
    }

    fn write_pending(&self, pending: &mut BTreeMap<String, Vec<u8>>) -> Result<()> {
        // Remove only successful writes, retaining failed and unattempted snapshots.
        while let Some((key, bytes)) = pending.first_key_value() {
            atomic_write(
                self.session_dir.join("docs").join(format!("{key}.json")),
                bytes,
                false,
            )?;
            let key = key.clone();
            pending.remove(&key);
        }
        Ok(())
    }

    /// Persist pending snapshots and release the session lock, e.g. when a browser
    /// page is hidden or unloaded. Another tab (or the reloaded page) can then offer
    /// the snapshots for recovery. [`Self::resume`] takes the lock again.
    /// Failed persistence retains the live lock and pending snapshots for retry.
    pub fn suspend(&self) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| Error::Poisoned)?;
        if state.finished {
            return Err(Error::Finished);
        }
        if state.lock.is_none() {
            return Ok(());
        }
        self.flush_locked(&state)?;
        state.lock.take();
        Ok(())
    }

    /// Re-acquire the session lock after [`Self::suspend`]. Fails if another
    /// process or tab claimed the session in the meantime.
    pub fn resume(&self) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| Error::Poisoned)?;
        if state.finished || state.lock.is_some() {
            return Ok(());
        }
        state.lock = Some(Lock::try_acquire(&self.session_dir.join("lock"))?);
        Ok(())
    }

    /// Persist pending snapshots synchronously. Failed writes remain queued for retry.
    /// A suspended session must resume ownership before writing again.
    pub fn flush(&self) -> Result<()> {
        let state = self.state.lock().map_err(|_| Error::Poisoned)?;
        self.flush_locked(&state)
    }

    fn flush_locked(&self, state: &State) -> Result<()> {
        if state.finished {
            return Err(Error::Finished);
        }
        let lock = state.lock.as_ref().ok_or(Error::Suspended)?;
        lock.refresh();
        let mut pending = {
            let mut shared = self.pending.lock().map_err(|_| Error::Poisoned)?;
            std::mem::take(&mut *shared)
        };
        let result = self.write_pending(&mut pending);
        if !pending.is_empty() {
            let mut shared = self.pending.lock().map_err(|_| Error::Poisoned)?;
            for (key, bytes) in pending {
                shared.entry(key).or_insert(bytes);
            }
        }
        result
    }

    /// Mark this session for crash preservation and flush without waiting on locks.
    /// Returns `false` when a lock was busy/poisoned or a disk write failed.
    pub fn try_flush_for_crash(&self) -> bool {
        self.preserve.store(true, Ordering::Release);
        let Ok(state) = self.state.try_lock() else {
            return false;
        };
        if state.finished {
            return true;
        }
        if state.lock.is_none() {
            return false;
        }
        let Ok(mut pending) = self.pending.try_lock() else {
            return false;
        };
        self.write_pending(&mut pending).is_ok()
    }

    /// Remove a saved/closed document and cancel its pending snapshot.
    pub fn remove(&self, doc_key: &str) -> Result<()> {
        check_key(doc_key)?;
        let state = self.state.lock().map_err(|_| Error::Poisoned)?;
        if state.finished {
            return Err(Error::Finished);
        }
        if state.lock.is_none() {
            return Err(Error::Suspended);
        }
        self.pending
            .lock()
            .map_err(|_| Error::Poisoned)?
            .remove(doc_key);
        match fs::remove_file(
            self.session_dir
                .join("docs")
                .join(format!("{doc_key}.json")),
        ) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// Clean shutdown: discard pending snapshots, release the lock and remove the session.
    /// Call only after documents are saved or the user explicitly discards them.
    pub fn finish(&self) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| Error::Poisoned)?;
        if state.finished {
            return Ok(());
        }
        if state.lock.is_none() {
            state.lock = Some(Lock::try_acquire(&self.session_dir.join("lock"))?);
        }
        let mut pending = self.pending.lock().map_err(|_| Error::Poisoned)?;
        // Remove docs while the lock is held; no scanner can claim them in between.
        match fs::remove_dir_all(self.session_dir.join("docs")) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        state.finished = true;
        self.closed.store(true, Ordering::Release);
        pending.clear();
        state.lock.take();
        match fs::remove_dir_all(&self.session_dir) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// Discover unlocked sessions containing snapshots. Active sessions are skipped.
    /// Returned handles retain the orphan lock so concurrent processes cannot claim it.
    pub fn find_orphans(paths: &Paths) -> Vec<OrphanSession> {
        let root = paths.data_dir.join("recovery");
        let Ok(entries) = fs::list_dir(root) else {
            return Vec::new();
        };
        let mut sessions = Vec::new();
        for (dir, kind) in entries {
            if kind != EntryKind::Dir {
                continue;
            }
            let Some(session_id) = dir.file_name().and_then(|s| s.to_str()).map(str::to_owned)
            else {
                continue;
            };
            let Ok(lock) = Lock::try_acquire(&dir.join("lock")) else {
                continue;
            };
            let mut documents = Vec::new();
            let mut can_cleanup = true;
            match fs::list_dir(dir.join("docs")) {
                Ok(entries) => {
                    for (path, kind) in entries {
                        if path.extension().is_none_or(|e| e != "json") {
                            continue;
                        }
                        if kind != EntryKind::File {
                            can_cleanup = false;
                            continue;
                        }
                        let Some(key) = path.file_stem().and_then(|s| s.to_str()) else {
                            can_cleanup = false;
                            continue;
                        };
                        let meta = fs::read(&path)
                            .ok()
                            .and_then(|bytes| serde_json::from_slice::<Snapshot>(&bytes).ok())
                            .map(|s| s.meta)
                            .unwrap_or_else(|| SnapshotMeta {
                                title: key.into(),
                                ..SnapshotMeta::default()
                            });
                        documents.push((key.to_owned(), meta, path));
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(_) => can_cleanup = false,
            }
            if documents.is_empty() {
                drop(lock);
                if can_cleanup {
                    let _ = fs::remove_dir_all(&dir);
                }
                continue;
            }
            let lease = Arc::new(OrphanLease {
                lock: Mutex::new(Some(lock)),
                dir,
            });
            let mut docs: Vec<_> = documents
                .into_iter()
                .map(|(doc_key, meta, path)| OrphanDoc {
                    session_id: session_id.clone(),
                    doc_key,
                    meta,
                    path,
                    lease: Arc::clone(&lease),
                })
                .collect();
            docs.sort_by(|a, b| a.doc_key.cmp(&b.doc_key));
            sessions.push(OrphanSession { session_id, docs });
        }
        sessions.sort_by(|a, b| a.session_id.cmp(&b.session_id));
        sessions
    }

    /// Delete all currently claimable orphan snapshots, never a live session.
    pub fn discard_all_orphans(paths: &Paths) -> Result<()> {
        for session in Self::find_orphans(paths) {
            session.discard()?;
        }
        Ok(())
    }
}

impl Drop for RecoveryManager {
    fn drop(&mut self) {
        if !self.preserve.load(Ordering::Acquire)
            && !std::thread::panicking()
            && let Err(error) = self.finish()
        {
            log::warn!("Recovery cleanup failed: {error}");
        }
    }
}

struct OrphanLease {
    lock: Mutex<Option<Lock>>,
    dir: PathBuf,
}
impl OrphanLease {
    fn discard(&self, path: &Path) -> Result<()> {
        let mut lock = self.lock.lock().map_err(|_| Error::Poisoned)?;
        if lock.is_none() {
            return Ok(());
        }
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        let has_docs = match fs::list_dir(self.dir.join("docs")) {
            Ok(entries) => entries
                .iter()
                .any(|(path, _)| path.extension().is_some_and(|s| s == "json")),
            // The browser filesystem has no empty directories.
            Err(e) if cfg!(target_arch = "wasm32") && e.kind() == io::ErrorKind::NotFound => false,
            Err(e) => return Err(e.into()),
        };
        if !has_docs {
            // Empty docs are no longer recoverable, so unlocking before deletion is safe.
            lock.take();
            match fs::remove_dir_all(&self.dir) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
}

/// A dead process's recoverable documents; holding these handles claims the session.
#[derive(Clone)]
pub struct OrphanSession {
    pub session_id: String,
    pub docs: Vec<OrphanDoc>,
}
impl OrphanSession {
    /// Discard every document in this orphan session.
    pub fn discard(&self) -> Result<()> {
        for doc in &self.docs {
            doc.discard()?;
        }
        Ok(())
    }
}

/// A recoverable document. Corrupt snapshots remain visible and can be discarded.
#[derive(Clone)]
pub struct OrphanDoc {
    pub session_id: String,
    pub doc_key: String,
    pub meta: SnapshotMeta,
    pub path: PathBuf,
    lease: Arc<OrphanLease>,
}
impl OrphanDoc {
    /// Deserialize and validate before exposing a workbook to application code.
    pub fn load(&self) -> Result<Workbook> {
        let _guard = self.lease.lock.lock().map_err(|_| Error::Poisoned)?;
        let snapshot: Snapshot = serde_json::from_slice(&fs::read(&self.path)?)?;
        snapshot
            .workbook
            .validate()
            .map_err(Error::InvalidWorkbook)?;
        Ok(snapshot.workbook)
    }

    /// Permanently discard this snapshot, cleaning the session when it becomes empty.
    pub fn discard(&self) -> Result<()> {
        self.lease.discard(&self.path)
    }
}

// Make accidental changes to the panic hook's sharing contract a compile error.
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<RecoveryManager>();
};

#[cfg(test)]
mod lock_tests {
    use super::*;

    #[test]
    fn recovery_restores_embedded_bytes_and_rejects_missing_resources() {
        let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let paths = Paths::from_root(dir.path());
        let manager = RecoveryManager::start(&paths).unwrap();
        let mut book = Workbook::default();
        let sheet = book.active_mut();
        let reference = sheet
            .resources
            .insert(
                thinoda_core::resources::Resource::new(
                    "evidence.txt",
                    "text/plain",
                    b"evidence".to_vec(),
                )
                .unwrap(),
            )
            .unwrap();
        sheet.topic_mut(sheet.root).unwrap().image = Some(reference);
        sheet.topic_mut(sheet.root).unwrap().image_size =
            Some(thinoda_core::Vec2::new(240.0, 120.0));
        sheet.topic_mut(sheet.root).unwrap().image_below = true;
        manager
            .snapshot("resource", SnapshotMeta::default(), &book)
            .unwrap();
        manager.suspend().unwrap();
        let orphans = RecoveryManager::find_orphans(&paths);
        let recovered = orphans[0].docs[0].load().unwrap();
        assert_eq!(recovered, book);
        drop(orphans);
        manager.resume().unwrap();
        book.active_mut().resources.entries.clear();
        manager
            .snapshot("resource", SnapshotMeta::default(), &book)
            .unwrap();
        manager.suspend().unwrap();
        let orphans = RecoveryManager::find_orphans(&paths);
        assert!(orphans[0].docs[0].load().is_err());
        drop(orphans);
        manager.resume().unwrap();
        manager.finish().unwrap();
    }

    #[test]
    fn crash_flush_never_waits_for_busy_mutexes() {
        let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let manager = RecoveryManager::start(&Paths::from_root(dir.path())).unwrap();
        let pending = manager.pending.lock().unwrap();
        assert!(!manager.try_flush_for_crash());
        drop(pending);
        let state = manager.state.lock().unwrap();
        assert!(!manager.try_flush_for_crash());
        drop(state);
        assert!(manager.try_flush_for_crash());
        manager.finish().unwrap();
    }

    #[test]
    fn suspended_sessions_are_recoverable_until_resumed() {
        let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let paths = Paths::from_root(dir.path());
        let manager = RecoveryManager::start(&paths).unwrap();
        manager
            .queue_snapshot(
                "doc",
                Snapshot::to_bytes(SnapshotMeta::default(), &Workbook::default()).unwrap(),
            )
            .unwrap();
        assert!(RecoveryManager::find_orphans(&paths).is_empty());
        manager.suspend().unwrap();
        let orphans = RecoveryManager::find_orphans(&paths);
        assert_eq!(orphans.len(), 1);
        assert!(
            manager.resume().is_err(),
            "a claimed session must not be re-locked"
        );
        drop(orphans);
        manager.resume().unwrap();
        assert!(RecoveryManager::find_orphans(&paths).is_empty());
        manager.finish().unwrap();
    }
}
