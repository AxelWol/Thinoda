//! Per-process crash recovery with advisory locks and coalesced snapshot writes.

use crate::{
    Error, Paths, Result,
    fs::{self, EntryKind, Lock, atomic_write},
    unix_seconds,
};
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{Error as _, IgnoredAny, MapAccess, SeqAccess, Visitor, value::MapAccessDeserializer},
    ser::SerializeTuple,
};
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
    #[serde(
        serialize_with = "serialize_workbook",
        deserialize_with = "deserialize_workbook"
    )]
    pub workbook: Workbook,
}

const INLINE_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INLINE-1";
const WAYPOINT_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-WAYPOINTS-1";

fn serialize_workbook<S>(workbook: &Workbook, serializer: S) -> std::result::Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let version = if workbook.has_connector_waypoints() {
        WAYPOINT_WORKBOOK_VERSION
    } else if workbook.has_inline_text() {
        INLINE_WORKBOOK_VERSION
    } else {
        return workbook.serialize(serializer);
    };
    let mut tuple = serializer.serialize_tuple(2)?;
    tuple.serialize_element(version)?;
    tuple.serialize_element(workbook)?;
    tuple.end()
}

fn deserialize_workbook<'de, D>(deserializer: D) -> std::result::Result<Workbook, D::Error>
where
    D: Deserializer<'de>,
{
    struct WorkbookVisitor;
    impl<'de> Visitor<'de> for WorkbookVisitor {
        type Value = Workbook;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a legacy workbook or a versioned recovery workbook")
        }

        fn visit_map<A>(self, map: A) -> std::result::Result<Workbook, A::Error>
        where
            A: MapAccess<'de>,
        {
            let workbook = Workbook::deserialize(MapAccessDeserializer::new(map))?;
            if workbook.has_connector_waypoints() {
                return Err(A::Error::custom(
                    "connector waypoints require a versioned recovery workbook",
                ));
            }
            Ok(workbook)
        }

        fn visit_seq<A>(self, mut seq: A) -> std::result::Result<Workbook, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let version: String = seq
                .next_element()?
                .ok_or_else(|| A::Error::custom("missing recovery workbook version"))?;
            if version != INLINE_WORKBOOK_VERSION && version != WAYPOINT_WORKBOOK_VERSION {
                return Err(A::Error::custom(format!(
                    "unsupported recovery workbook version {version}"
                )));
            }
            let workbook: Workbook = seq
                .next_element()?
                .ok_or_else(|| A::Error::custom("missing recovery workbook"))?;
            if version == INLINE_WORKBOOK_VERSION && workbook.has_connector_waypoints() {
                return Err(A::Error::custom(
                    "connector waypoints require a waypoint-aware recovery workbook",
                ));
            }
            if seq.next_element::<IgnoredAny>()?.is_some() {
                return Err(A::Error::custom("unexpected recovery workbook data"));
            }
            Ok(workbook)
        }
    }

    deserializer.deserialize_any(WorkbookVisitor)
}

impl Snapshot {
    /// Serialize a recovery envelope, without writing or taking a recovery lock.
    pub fn to_bytes(meta: SnapshotMeta, workbook: &Workbook) -> Result<Vec<u8>> {
        #[derive(Serialize)]
        struct Borrowed<'a> {
            meta: SnapshotMeta,
            #[serde(serialize_with = "serialize_workbook")]
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
    unflushed: AtomicBool,
    flush_failed: AtomicBool,
    preserve: AtomicBool,
    closed: AtomicBool,
}

/// Claimable sessions and whether some recovery data could not be inspected.
#[derive(Default)]
pub struct OrphanDiscovery {
    pub sessions: Vec<OrphanSession>,
    pub incomplete: bool,
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

fn retire_session(dir: &Path) -> Result<()> {
    match fs::remove_file(dir.join("session.json")) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn cleanup_retired_session(dir: &Path) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Only remove empty directories; never delete a newly resumed owner's data.
        for (path, file) in [
            (dir.join("docs"), false),
            (dir.join("lock"), true),
            (dir.to_path_buf(), false),
        ] {
            let result = if file {
                std::fs::remove_file(&path)
            } else {
                std::fs::remove_dir(&path)
            };
            if let Err(error) = result
                && error.kind() != io::ErrorKind::NotFound
            {
                log::warn!("Recovery session cleanup failed: {error}");
                break;
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = dir;
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
            unflushed: AtomicBool::new(false),
            flush_failed: AtomicBool::new(false),
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

    /// Whether a queued snapshot has not yet been persisted or canceled.
    pub fn has_unflushed_snapshots(&self) -> bool {
        self.unflushed.load(Ordering::Acquire)
    }

    /// Whether the last flush or suspension write attempt failed.
    pub fn last_flush_failed(&self) -> bool {
        self.flush_failed.load(Ordering::Acquire)
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
        lock.refresh()?;
        let mut pending = self.pending.lock().map_err(|_| Error::Poisoned)?;
        atomic_write(
            self.session_dir
                .join("docs")
                .join(format!("{doc_key}.json")),
            &bytes,
            false,
        )?;
        pending.remove(doc_key);
        self.unflushed.store(!pending.is_empty(), Ordering::Release);
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
        self.unflushed.store(true, Ordering::Release);
        Ok(())
    }

    fn write_pending(
        &self,
        pending: &mut BTreeMap<String, Vec<u8>>,
        mut refresh: impl FnMut() -> io::Result<()>,
    ) -> Result<()> {
        // Remove only successful writes, retaining failed and unattempted snapshots.
        while let Some((key, bytes)) = pending.first_key_value() {
            refresh()?;
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
        let result = self.flush_locked(&state);
        self.flush_failed.store(result.is_err(), Ordering::Release);
        result?;
        state.lock.take();
        Ok(())
    }

    /// Re-acquire the session lock after [`Self::suspend`]. Fails if another
    /// process or tab claimed the session in the meantime.
    pub fn resume(&self) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| Error::Poisoned)?;
        if state.finished {
            return Err(Error::Finished);
        }
        if let Some(lock) = &state.lock {
            return Ok(lock.refresh()?);
        }
        let lock = Lock::try_resume(&self.session_dir.join("lock"))?;
        fs::read_bounded(self.session_dir.join("session.json"), 4096)?;
        state.lock = Some(lock);
        Ok(())
    }

    /// Persist pending snapshots synchronously. Failed writes remain queued for retry.
    /// A suspended session must resume ownership before writing again.
    pub fn flush(&self) -> Result<()> {
        match self.state.lock() {
            Ok(state) => {
                let result = self.flush_locked(&state);
                self.flush_failed.store(result.is_err(), Ordering::Release);
                result
            }
            Err(_) => {
                self.flush_failed.store(true, Ordering::Release);
                Err(Error::Poisoned)
            }
        }
    }

    fn flush_locked(&self, state: &State) -> Result<()> {
        if state.finished {
            return Err(Error::Finished);
        }
        let lock = state.lock.as_ref().ok_or(Error::Suspended)?;
        lock.refresh()?;
        self.flush_pending(|| lock.refresh())
    }

    fn flush_pending(&self, refresh: impl FnMut() -> io::Result<()>) -> Result<()> {
        let mut pending = {
            let mut shared = self.pending.lock().map_err(|_| Error::Poisoned)?;
            std::mem::take(&mut *shared)
        };
        let result = self.write_pending(&mut pending, refresh);
        let mut shared = self.pending.lock().map_err(|_| Error::Poisoned)?;
        for (key, bytes) in pending {
            shared.entry(key).or_insert(bytes);
        }
        self.unflushed.store(!shared.is_empty(), Ordering::Release);
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
        let Some(lock) = state.lock.as_ref() else {
            return false;
        };
        if lock.refresh().is_err() {
            return false;
        }
        let Ok(mut pending) = self.pending.try_lock() else {
            return false;
        };
        let result = self.write_pending(&mut pending, || lock.refresh());
        self.unflushed.store(!pending.is_empty(), Ordering::Release);
        result.is_ok()
    }

    /// Remove a saved/closed document and cancel its pending snapshot.
    pub fn remove(&self, doc_key: &str) -> Result<()> {
        check_key(doc_key)?;
        let state = self.state.lock().map_err(|_| Error::Poisoned)?;
        if state.finished {
            return Err(Error::Finished);
        }
        let lock = state.lock.as_ref().ok_or(Error::Suspended)?;
        lock.refresh()?;
        let mut pending = self.pending.lock().map_err(|_| Error::Poisoned)?;
        match fs::remove_file(
            self.session_dir
                .join("docs")
                .join(format!("{doc_key}.json")),
        ) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        pending.remove(doc_key);
        self.unflushed.store(!pending.is_empty(), Ordering::Release);
        Ok(())
    }

    /// Clean shutdown: discard pending snapshots, release the lock and remove the session.
    /// Call only after documents are saved or the user explicitly discards them.
    /// A suspended session must be resumed explicitly before it can be removed.
    pub fn finish(&self) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| Error::Poisoned)?;
        if state.finished {
            return Ok(());
        }
        let lock = state.lock.as_ref().ok_or(Error::Suspended)?;
        lock.refresh()?;
        let mut pending = self.pending.lock().map_err(|_| Error::Poisoned)?;
        // Remove docs while the lock is held; no scanner can claim them in between.
        match fs::remove_dir_all(self.session_dir.join("docs")) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        retire_session(&self.session_dir)?;
        cleanup_retired_session(&self.session_dir);
        state.finished = true;
        self.closed.store(true, Ordering::Release);
        pending.clear();
        self.unflushed.store(false, Ordering::Release);
        state.lock.take();
        Ok(())
    }

    /// Discover unlocked sessions containing snapshots. Active sessions are skipped.
    /// Returned handles retain the orphan lock so concurrent processes cannot claim it.
    pub fn find_orphans(paths: &Paths) -> Vec<OrphanSession> {
        Self::discover_orphans(paths).sessions
    }

    /// Discover claimable sessions and report unreadable or corrupt recovery data.
    pub fn discover_orphans(paths: &Paths) -> OrphanDiscovery {
        let root = paths.data_dir.join("recovery");
        let entries = match fs::list_dir(root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return OrphanDiscovery::default();
            }
            Err(error) => {
                log::warn!("Recovery discovery failed: {error}");
                return OrphanDiscovery {
                    sessions: Vec::new(),
                    incomplete: true,
                };
            }
        };
        let mut sessions = Vec::new();
        let mut incomplete = false;
        for (dir, kind) in entries {
            if kind != EntryKind::Dir {
                continue;
            }
            let Some(session_id) = dir.file_name().and_then(|s| s.to_str()).map(str::to_owned)
            else {
                continue;
            };
            let lock = match Lock::try_acquire(&dir.join("lock")) {
                Ok(lock) => lock,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => continue,
                Err(error) => {
                    log::warn!("Could not inspect a recovery session: {error}");
                    incomplete = true;
                    continue;
                }
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
                            incomplete = true;
                            continue;
                        }
                        let Some(key) = path.file_stem().and_then(|s| s.to_str()) else {
                            can_cleanup = false;
                            incomplete = true;
                            continue;
                        };
                        let meta = match fs::read(&path).and_then(|bytes| {
                            serde_json::from_slice::<Snapshot>(&bytes).map_err(io::Error::other)
                        }) {
                            Ok(snapshot) => snapshot.meta,
                            Err(error) => {
                                log::warn!("Could not inspect a recovery snapshot: {error}");
                                incomplete = true;
                                SnapshotMeta {
                                    title: key.into(),
                                    ..SnapshotMeta::default()
                                }
                            }
                        };
                        documents.push((key.to_owned(), meta, path));
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(error) => {
                    log::warn!("Could not list recovery snapshots: {error}");
                    can_cleanup = false;
                    incomplete = true;
                }
            }
            if documents.is_empty() {
                // A suspended live page can still resume an empty session.
                // Keep its marker; only clean abandoned directories without one.
                let marker_missing = match fs::read_bounded(dir.join("session.json"), 4096) {
                    Ok(_) => false,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => true,
                    Err(error) => {
                        log::warn!("Could not inspect a recovery session marker: {error}");
                        incomplete = true;
                        false
                    }
                };
                if can_cleanup && marker_missing {
                    if let Err(error) = lock.refresh() {
                        log::warn!("Recovery session cleanup failed: {error}");
                        incomplete = true;
                        continue;
                    }
                    if let Err(error) = retire_session(&dir) {
                        log::warn!("Recovery session cleanup failed: {error}");
                        incomplete = true;
                        continue;
                    }
                    cleanup_retired_session(&dir);
                }
                drop(lock);
                continue;
            }
            if let Err(error) = lock.refresh() {
                log::warn!("Recovery session claim was lost: {error}");
                incomplete = true;
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
        OrphanDiscovery {
            sessions,
            incomplete,
        }
    }

    /// Delete all currently claimable orphan snapshots, never a live session.
    pub fn discard_all_orphans(paths: &Paths) -> Result<()> {
        let discovery = Self::discover_orphans(paths);
        if discovery.incomplete {
            return Err(io::Error::other("some recovery sessions could not be inspected").into());
        }
        for session in discovery.sessions {
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
        let Some(held) = lock.as_ref() else {
            return Ok(());
        };
        held.refresh()?;
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
            retire_session(&self.dir)?;
            cleanup_retired_session(&self.dir);
            lock.take();
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
        let guard = self.lease.lock.lock().map_err(|_| Error::Poisoned)?;
        guard.as_ref().ok_or(Error::Finished)?.refresh()?;
        let snapshot: Snapshot = serde_json::from_slice(&fs::read(&self.path)?)?;
        snapshot
            .workbook
            .validate()
            .map_err(Error::InvalidWorkbook)?;
        Ok(snapshot.workbook)
    }

    /// Copy the raw browser storage record while this session is still claimed.
    /// The UTF-16LE blob is not a validated snapshot or a `.thinoda` document.
    #[cfg(target_arch = "wasm32")]
    pub fn raw_browser_record(&self) -> Result<web_sys::Blob> {
        // Discovery represents storage keys as UTF-8 paths. Reject noncanonical
        // IDs so lossy conversion or path normalization cannot select another
        // record; recovery-generated session and document IDs are ASCII.
        check_key(&self.session_id)?;
        check_key(&self.doc_key)?;
        let guard = self.lease.lock.lock().map_err(|_| Error::Poisoned)?;
        guard.as_ref().ok_or(Error::Finished)?.refresh()?;
        Ok(crate::vfs::raw_browser_record(&self.path)?)
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

    #[derive(Serialize, Deserialize)]
    struct LegacySnapshot {
        meta: SnapshotMeta,
        workbook: Workbook,
    }

    fn formatted_workbook(kind: thinoda_core::CardKind) -> Workbook {
        let mut book = Workbook::default();
        let sheet = book.active_mut();
        let title = "A🦀é\n第二";
        let card = sheet
            .add_card(
                title,
                thinoda_core::Vec2::new(100.0, 100.0),
                thinoda_core::Card::new(kind, thinoda_core::Vec2::new(160.0, 100.0)),
            )
            .unwrap();
        let topic = sheet.topic_mut(card).unwrap();
        topic
            .inline_text
            .update_range(title, 0..3, |style| {
                style.bold = Some(true);
                style.font_family = Some("Sans".into());
            })
            .unwrap();
        topic
            .inline_text
            .paragraphs
            .push(thinoda_core::InlineParagraph {
                index: 1,
                list: thinoda_core::ListKind::Numbered,
            });
        book.validate().unwrap();
        book
    }

    #[test]
    fn recovery_workbook_versions_protect_rich_data_and_keep_legacy_snapshots() {
        let meta = SnapshotMeta {
            title: "Recovery".into(),
            dirty: true,
            ..SnapshotMeta::default()
        };
        let plain = Workbook::default();
        let legacy_plain = LegacySnapshot {
            meta: meta.clone(),
            workbook: plain.clone(),
        };
        let plain_bytes = Snapshot::to_bytes(meta.clone(), &plain).unwrap();
        assert_eq!(plain_bytes, serde_json::to_vec(&legacy_plain).unwrap());
        assert_eq!(
            serde_json::from_slice::<LegacySnapshot>(&plain_bytes)
                .unwrap()
                .workbook,
            plain
        );

        for kind in [thinoda_core::CardKind::Sticky, thinoda_core::CardKind::Text] {
            let book = formatted_workbook(kind);
            let bytes = Snapshot::to_bytes(meta.clone(), &book).unwrap();
            let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(snapshot.meta, meta);
            assert_eq!(snapshot.workbook, book);
            assert_eq!(serde_json::to_vec(&snapshot).unwrap(), bytes);
            assert!(serde_json::from_slice::<LegacySnapshot>(&bytes).is_err());

            let historical = serde_json::to_vec(&LegacySnapshot {
                meta: meta.clone(),
                workbook: book.clone(),
            })
            .unwrap();
            assert_eq!(
                serde_json::from_slice::<Snapshot>(&historical)
                    .unwrap()
                    .workbook,
                book
            );

            let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            value["workbook"][0] = serde_json::json!("THINODA-RECOVERY-INLINE-999");
            assert!(serde_json::from_value::<Snapshot>(value).is_err());
            let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            value["workbook"].as_array_mut().unwrap().pop();
            assert!(serde_json::from_value::<Snapshot>(value).is_err());
            let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            value["workbook"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!(null));
            assert!(serde_json::from_value::<Snapshot>(value).is_err());
        }
    }

    #[test]
    fn recovery_workbook_versions_protect_waypoints_on_inactive_sheets() {
        use thinoda_core::{CanvasConnector, CanvasObject, Endpoint, Sheet, Vec2};

        let mut sheet = Sheet::new("Routes", "Root");
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.waypoints = vec![Vec2::new(50.0, 50.0)].into();
        sheet
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let meta = SnapshotMeta::default();
        for mut book in [
            Workbook::default(),
            formatted_workbook(thinoda_core::CardKind::Text),
        ] {
            book.sheets.push(sheet.clone());
            book.validate().unwrap();
            let bytes = Snapshot::to_bytes(meta.clone(), &book).unwrap();
            let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(snapshot.workbook, book);
            assert_eq!(serde_json::to_vec(&snapshot).unwrap(), bytes);
            assert!(serde_json::from_slice::<LegacySnapshot>(&bytes).is_err());

            let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(value["workbook"][0], WAYPOINT_WORKBOOK_VERSION);
            let mut inline_only = value.clone();
            inline_only["workbook"][0] = serde_json::json!(INLINE_WORKBOOK_VERSION);
            assert!(
                serde_json::from_value::<Snapshot>(inline_only)
                    .unwrap_err()
                    .to_string()
                    .contains("connector waypoints require a waypoint-aware recovery workbook")
            );
            let mut unversioned = value.clone();
            unversioned["workbook"] = value["workbook"][1].clone();
            assert!(
                serde_json::from_value::<Snapshot>(unversioned)
                    .unwrap_err()
                    .to_string()
                    .contains("connector waypoints require a versioned recovery workbook")
            );
        }
    }

    #[test]
    fn recovery_load_restores_formatted_cards_and_rejects_malformed_runs() {
        let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let paths = Paths::from_root(dir.path());
        let manager = RecoveryManager::start(&paths).unwrap();
        let mut book = formatted_workbook(thinoda_core::CardKind::Sticky);
        manager
            .snapshot("rich", SnapshotMeta::default(), &book)
            .unwrap();
        manager.suspend().unwrap();
        let orphans = RecoveryManager::find_orphans(&paths);
        assert_eq!(orphans[0].docs[0].load().unwrap(), book);
        drop(orphans);
        manager.resume().unwrap();
        let sheet = book.active_mut();
        let card = sheet.floating[0];
        sheet.topic_mut(card).unwrap().inline_text.runs[0].end = 100;
        manager
            .snapshot("rich", SnapshotMeta::default(), &book)
            .unwrap();
        manager.suspend().unwrap();
        let orphans = RecoveryManager::find_orphans(&paths);
        assert!(orphans[0].docs[0].load().is_err());
        drop(orphans);
        manager.resume().unwrap();
        manager.finish().unwrap();
    }

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
        thinoda_core::attachments::insert(
            sheet,
            sheet.root,
            "opaque.docm",
            b"\0PK\x03\x04macro bytes\xff".to_vec(),
        )
        .unwrap();
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

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_snapshot_queued_during_a_flush_remains_unflushed() {
        use std::{sync::mpsc, time::Duration};

        for fail in [false, true] {
            let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
            let manager = RecoveryManager::start(&Paths::from_root(dir.path())).unwrap();
            manager.queue_snapshot("doc", b"earlier".to_vec()).unwrap();
            let (started_tx, started_rx) = mpsc::channel();
            let (resume_tx, resume_rx) = mpsc::channel();
            std::thread::scope(|scope| {
                let manager = &manager;
                let writer = scope.spawn(move || {
                    manager.flush_pending(|| {
                        started_tx.send(()).unwrap();
                        resume_rx
                            .recv_timeout(Duration::from_secs(5))
                            .map_err(io::Error::other)?;
                        if fail {
                            Err(io::Error::other("injected write failure"))
                        } else {
                            Ok(())
                        }
                    })
                });
                started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                assert!(manager.has_unflushed_snapshots());
                manager.queue_snapshot("doc", b"newer".to_vec()).unwrap();
                resume_tx.send(()).unwrap();
                assert_eq!(writer.join().unwrap().is_err(), fail);
            });
            assert!(manager.has_unflushed_snapshots());
            let path = manager.session_dir.join("docs").join("doc.json");
            if fail {
                assert!(!fs::exists(&path));
            } else {
                assert_eq!(fs::read(&path).unwrap(), b"earlier");
            }
            manager.flush().unwrap();
            assert!(!manager.has_unflushed_snapshots());
            assert_eq!(fs::read(path).unwrap(), b"newer");
            manager.finish().unwrap();
        }
    }

    #[test]
    fn pending_flush_rechecks_ownership_before_each_document() {
        let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let manager = RecoveryManager::start(&Paths::from_root(dir.path())).unwrap();
        let first = b"first snapshot".to_vec();
        let second = b"second snapshot".to_vec();
        let mut pending = BTreeMap::from([
            ("a".to_owned(), first.clone()),
            ("b".to_owned(), second.clone()),
        ]);
        let mut checks = 0;
        let result = manager.write_pending(&mut pending, || {
            checks += 1;
            if checks == 2 {
                Err(io::Error::new(io::ErrorKind::WouldBlock, "claim lost"))
            } else {
                Ok(())
            }
        });
        assert!(result.is_err());
        assert_eq!(checks, 2);
        assert_eq!(
            fs::read(manager.session_dir.join("docs").join("a.json")).unwrap(),
            first
        );
        assert!(!fs::exists(manager.session_dir.join("docs").join("b.json")));
        assert_eq!(pending, BTreeMap::from([("b".to_owned(), second.clone())]));
        manager.write_pending(&mut pending, || Ok(())).unwrap();
        assert_eq!(
            fs::read(manager.session_dir.join("docs").join("b.json")).unwrap(),
            second
        );
        assert!(pending.is_empty());
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
