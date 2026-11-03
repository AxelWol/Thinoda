//! Small synchronous virtual filesystem for the browser build.
//!
//! Static web apps have no filesystem, so the web target stores settings,
//! recovery snapshots, crash logs and saved workbooks in a key-value store
//! (`window.localStorage`). Files are values keyed by a normalized absolute
//! `/`-separated path; directories are implicit path prefixes. The module is
//! target-independent so its semantics are covered by native unit tests.

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
    sync::Mutex,
};

/// Key prefix separating Thinoda's entries from other data of the same origin.
pub const KEY_PREFIX: &str = "thinoda.vfs:";

/// A heartbeat lock older than this is considered abandoned by a closed tab.
pub const LOCK_STALE_MS: u64 = 120_000;

/// Minimal string key-value storage (implemented by `localStorage` on the web).
pub trait KvStore: Send + Sync {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&self, key: &str, value: &str) -> io::Result<()>;
    fn remove(&self, key: &str);
    fn keys(&self) -> Vec<String>;

    fn try_get(&self, key: &str) -> io::Result<Option<String>> {
        Ok(self.get(key))
    }

    fn try_remove(&self, key: &str) -> io::Result<()> {
        self.remove(key);
        if self.try_get(key)?.is_some() {
            return Err(io::Error::other("browser storage could not remove a value"));
        }
        Ok(())
    }

    fn try_keys(&self) -> io::Result<Vec<String>> {
        Ok(self.keys())
    }
}

/// In-memory store for tests and as a fallback when browser storage is blocked.
#[derive(Default)]
pub struct MemoryStore(Mutex<BTreeMap<String, String>>);

impl KvStore for MemoryStore {
    fn get(&self, key: &str) -> Option<String> {
        self.0.lock().ok()?.get(key).cloned()
    }
    fn set(&self, key: &str, value: &str) -> io::Result<()> {
        self.0
            .lock()
            .map_err(|_| io::Error::other("memory store poisoned"))?
            .insert(key.into(), value.into());
        Ok(())
    }
    fn remove(&self, key: &str) {
        if let Ok(mut map) = self.0.lock() {
            map.remove(key);
        }
    }
    fn keys(&self) -> Vec<String> {
        self.0
            .lock()
            .map(|map| map.keys().cloned().collect())
            .unwrap_or_default()
    }

    fn try_get(&self, key: &str) -> io::Result<Option<String>> {
        Ok(self
            .0
            .lock()
            .map_err(|_| io::Error::other("memory store poisoned"))?
            .get(key)
            .cloned())
    }

    fn try_remove(&self, key: &str) -> io::Result<()> {
        self.0
            .lock()
            .map_err(|_| io::Error::other("memory store poisoned"))?
            .remove(key);
        Ok(())
    }

    fn try_keys(&self) -> io::Result<Vec<String>> {
        Ok(self
            .0
            .lock()
            .map_err(|_| io::Error::other("memory store poisoned"))?
            .keys()
            .cloned()
            .collect())
    }
}

#[derive(Serialize, Deserialize)]
struct Entry {
    /// Modification time, milliseconds since the Unix epoch.
    m: u64,
    /// Decoded length in bytes.
    l: u64,
    /// Base64 contents.
    d: String,
}

#[derive(Serialize, Deserialize)]
struct LockRecord {
    owner: String,
    heartbeat_ms: u64,
}

/// Kind of a directory listing entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    File,
    Dir,
}

/// Length and modification time of a stored file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Meta {
    pub len: u64,
    pub modified_ms: u64,
}

/// A virtual filesystem over any [`KvStore`].
pub struct Vfs {
    store: Box<dyn KvStore>,
}

fn not_found(path: &str) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, format!("{path} not found"))
}

#[derive(Debug)]
pub(crate) struct ReadLimitExceeded;

impl std::fmt::Display for ReadLimitExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("storage exceeds limit")
    }
}

impl std::error::Error for ReadLimitExceeded {}

/// Whether a VFS read failed specifically because its configured limit was exceeded.
pub fn is_read_limit_exceeded(error: &io::Error) -> bool {
    error
        .get_ref()
        .is_some_and(|source| source.is::<ReadLimitExceeded>())
}

fn read_limit_error() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, ReadLimitExceeded)
}

/// Lexically normalize to an absolute `/`-separated path (`\` is treated as `/`).
pub fn normalize(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut parts: Vec<&str> = Vec::new();
    for part in text.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    format!("/{}", parts.join("/"))
}

impl Vfs {
    pub fn new(store: Box<dyn KvStore>) -> Self {
        Self { store }
    }

    /// A fresh volatile filesystem.
    pub fn memory() -> Self {
        Self::new(Box::<MemoryStore>::default())
    }

    fn key(path: &str) -> String {
        format!("{KEY_PREFIX}{path}")
    }

    fn entry(&self, path: &str) -> io::Result<Entry> {
        let value = self
            .store
            .try_get(&Self::key(path))?
            .ok_or_else(|| not_found(path))?;
        serde_json::from_str(&value)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    /// Normalized paths of every stored file.
    fn files(&self) -> Vec<String> {
        self.try_files().unwrap_or_default()
    }

    fn try_files(&self) -> io::Result<Vec<String>> {
        Ok(self
            .store
            .try_keys()?
            .into_iter()
            .filter_map(|k| k.strip_prefix(KEY_PREFIX).map(str::to_owned))
            .collect())
    }

    pub fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        let path = normalize(path);
        let entry = self.entry(&path)?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(entry.d)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        if bytes.len() as u64 != entry.l {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid storage length",
            ));
        }
        Ok(bytes)
    }

    /// Bounded decoding for app-local data. Reject oversized envelopes before
    /// parsing/base64 allocation; malformed entries are not treated as missing.
    pub fn read_bounded(&self, path: &Path, limit: usize) -> io::Result<Vec<u8>> {
        let entry = self.bounded_entry(path, limit)?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(entry.d)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid storage"))?;
        if bytes.len() > limit {
            return Err(read_limit_error());
        }
        if bytes.len() as u64 != entry.l {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid storage length",
            ));
        }
        Ok(bytes)
    }

    fn bounded_entry(&self, path: &Path, limit: usize) -> io::Result<Entry> {
        let path = normalize(path);
        let raw = self
            .store
            .try_get(&Self::key(&path))?
            .ok_or_else(|| not_found(&path))?;
        if raw.len() > limit.saturating_mul(2).saturating_add(1024) {
            return Err(read_limit_error());
        }
        let entry: Entry = serde_json::from_str(&raw)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid storage"))?;
        let max_encoded = (limit.saturating_add(2) / 3).saturating_mul(4);
        if entry.l > limit as u64 || entry.d.len() > max_encoded {
            return Err(read_limit_error());
        }
        Ok(entry)
    }

    /// Like `metadata`, but reject oversized VFS envelopes before parsing.
    pub(crate) fn metadata_bounded(&self, path: &Path, limit: usize) -> io::Result<Meta> {
        let entry = self.bounded_entry(path, limit)?;
        Ok(Meta {
            len: entry.l,
            modified_ms: entry.m,
        })
    }

    /// Replace a file in one store operation (atomic with respect to this tab).
    pub fn write(&self, path: &Path, bytes: &[u8], now_ms: u64) -> io::Result<()> {
        let path = normalize(path);
        let prefix = format!("{path}/");
        if path == "/"
            || self
                .try_files()?
                .iter()
                .any(|file| file.starts_with(&prefix))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{path} is a directory"),
            ));
        }
        let entry = Entry {
            m: now_ms,
            l: bytes.len() as u64,
            d: base64::engine::general_purpose::STANDARD.encode(bytes),
        };
        let value = serde_json::to_string(&entry).map_err(io::Error::other)?;
        self.store.set(&Self::key(&path), &value).map_err(|e| {
            io::Error::new(
                e.kind(),
                format!("browser storage could not save {path} (storage may be full): {e}"),
            )
        })
    }

    pub fn remove_file(&self, path: &Path) -> io::Result<()> {
        let path = normalize(path);
        if self.store.try_get(&Self::key(&path))?.is_none() {
            return Err(not_found(&path));
        }
        self.store.try_remove(&Self::key(&path))
    }

    pub fn is_file(&self, path: &Path) -> bool {
        self.store.get(&Self::key(&normalize(path))).is_some()
    }

    fn is_dir_norm(&self, path: &str) -> bool {
        if path == "/" {
            return true;
        }
        let prefix = format!("{path}/");
        self.files().iter().any(|f| f.starts_with(&prefix))
    }

    pub fn is_dir(&self, path: &Path) -> bool {
        self.is_dir_norm(&normalize(path))
    }

    pub fn exists(&self, path: &Path) -> bool {
        self.is_file(path) || self.is_dir(path)
    }

    pub fn metadata(&self, path: &Path) -> io::Result<Meta> {
        let path = normalize(path);
        let entry = self.entry(&path)?;
        Ok(Meta {
            len: entry.l,
            modified_ms: entry.m,
        })
    }

    pub fn rename(&self, from: &Path, to: &Path, now_ms: u64) -> io::Result<()> {
        let bytes = self.read(from)?;
        self.write(to, &bytes, now_ms)?;
        if normalize(from) != normalize(to) {
            self.remove_file(from)?;
        }
        Ok(())
    }

    /// Immediate children of a directory, sorted by name.
    pub fn read_dir(&self, path: &Path) -> io::Result<Vec<(PathBuf, Kind)>> {
        let path = normalize(path);
        let prefix = if path == "/" {
            "/".to_owned()
        } else {
            format!("{path}/")
        };
        let mut children = BTreeMap::new();
        for file in self.try_files()? {
            let Some(rest) = file.strip_prefix(&prefix) else {
                continue;
            };
            match rest.split_once('/') {
                Some((dir, _)) => {
                    children.insert(dir.to_owned(), Kind::Dir);
                }
                None if !rest.is_empty() => {
                    children.entry(rest.to_owned()).or_insert(Kind::File);
                }
                None => {}
            }
        }
        if children.is_empty() && path != "/" {
            return Err(not_found(&path));
        }
        Ok(children
            .into_iter()
            .map(|(name, kind)| (PathBuf::from(format!("{prefix}{name}")), kind))
            .collect())
    }

    pub fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        let path = normalize(path);
        let files = self.try_files()?;
        let prefix = format!("{path}/");
        if path != "/" && !files.iter().any(|file| file.starts_with(&prefix)) {
            return Err(not_found(&path));
        }
        for file in files {
            if file.starts_with(&prefix) {
                self.store.try_remove(&Self::key(&file))?;
            }
        }
        Ok(())
    }

    fn lock_record(&self, path: &Path) -> io::Result<Option<LockRecord>> {
        let key = Self::key(&normalize(path));
        let Some(value) = self.store.try_get(&key)? else {
            return Ok(None);
        };
        let invalid = || io::Error::new(io::ErrorKind::InvalidData, "invalid browser lock");
        if value.len() > 4096 {
            return Err(invalid());
        }
        let entry: Entry = serde_json::from_str(&value).map_err(|_| invalid())?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(entry.d)
            .map_err(|_| invalid())?;
        if bytes.len() as u64 != entry.l {
            return Err(invalid());
        }
        let record: LockRecord = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        if record.owner.is_empty() {
            return Err(invalid());
        }
        Ok(Some(record))
    }

    fn write_lock(&self, path: &Path, owner: &str, now_ms: u64) -> io::Result<()> {
        let record = LockRecord {
            owner: owner.into(),
            heartbeat_ms: now_ms,
        };
        self.write(
            path,
            &serde_json::to_vec(&record).map_err(io::Error::other)?,
            now_ms,
        )?;
        match self.lock_record(path)? {
            Some(record) if record.owner == owner && record.heartbeat_ms == now_ms => Ok(()),
            _ => Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "browser lock changed during acquisition",
            )),
        }
    }

    /// Acquire or refresh a cooperative cross-tab lock. Fails with `WouldBlock`
    /// while another owner has refreshed it within [`LOCK_STALE_MS`].
    pub fn try_lock(&self, path: &Path, owner: &str, now_ms: u64) -> io::Result<()> {
        if let Some(record) = self.lock_record(path)?
            && record.owner != owner
            && now_ms.saturating_sub(record.heartbeat_ms) < LOCK_STALE_MS
        {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "locked by another browser tab",
            ));
        }
        self.write_lock(path, owner, now_ms)
    }

    /// Resume only an unclaimed lock; an expired claim still belongs to its
    /// current owner until a separate orphan-discovery decision takes it over.
    #[cfg(any(test, target_arch = "wasm32"))]
    pub(crate) fn try_resume_lock(&self, path: &Path, owner: &str, now_ms: u64) -> io::Result<()> {
        if let Some(record) = self.lock_record(path)?
            && record.owner != owner
        {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "locked by another browser tab",
            ));
        }
        self.write_lock(path, owner, now_ms)
    }

    /// Refresh an existing claim without silently reacquiring a missing,
    /// corrupt, or different owner's lock.
    #[cfg(any(test, target_arch = "wasm32"))]
    pub(crate) fn refresh_lock(&self, path: &Path, owner: &str, now_ms: u64) -> io::Result<()> {
        let record = self
            .lock_record(path)?
            .ok_or_else(|| not_found(&normalize(path)))?;
        if record.owner != owner {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "locked by another browser tab",
            ));
        }
        self.write_lock(path, owner, now_ms)
    }

    /// Release a lock if it is still held by `owner`.
    pub fn unlock(&self, path: &Path, owner: &str) -> io::Result<()> {
        if self
            .lock_record(path)?
            .is_some_and(|record| record.owner == owner)
        {
            self.remove_file(path)?;
        }
        Ok(())
    }
}

/// `window.localStorage`, looked up per call because browser handles are not `Send`.
#[cfg(target_arch = "wasm32")]
pub struct LocalStorage;

#[cfg(target_arch = "wasm32")]
impl LocalStorage {
    fn storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok()?
    }
    /// Whether the browser grants persistent local storage to this origin.
    pub fn available() -> bool {
        Self::storage().is_some()
    }
}

#[cfg(target_arch = "wasm32")]
impl KvStore for LocalStorage {
    fn get(&self, key: &str) -> Option<String> {
        Self::storage()?.get_item(key).ok()?
    }
    fn set(&self, key: &str, value: &str) -> io::Result<()> {
        Self::storage()
            .ok_or_else(|| io::Error::other("browser storage is unavailable"))?
            .set_item(key, value)
            .map_err(|e| io::Error::new(io::ErrorKind::StorageFull, format!("{e:?}")))
    }
    fn remove(&self, key: &str) {
        if let Some(storage) = Self::storage() {
            let _ = storage.remove_item(key);
        }
    }
    fn keys(&self) -> Vec<String> {
        self.try_keys().unwrap_or_default()
    }

    fn try_get(&self, key: &str) -> io::Result<Option<String>> {
        Self::storage()
            .ok_or_else(|| io::Error::other("browser storage is unavailable"))?
            .get_item(key)
            .map_err(|e| io::Error::other(format!("browser storage read failed: {e:?}")))
    }

    fn try_remove(&self, key: &str) -> io::Result<()> {
        Self::storage()
            .ok_or_else(|| io::Error::other("browser storage is unavailable"))?
            .remove_item(key)
            .map_err(|e| io::Error::other(format!("browser storage removal failed: {e:?}")))
    }

    fn try_keys(&self) -> io::Result<Vec<String>> {
        let storage =
            Self::storage().ok_or_else(|| io::Error::other("browser storage is unavailable"))?;
        let len = storage
            .length()
            .map_err(|e| io::Error::other(format!("browser storage listing failed: {e:?}")))?;
        let mut keys = Vec::new();
        for index in 0..len {
            let key = storage
                .key(index)
                .map_err(|e| io::Error::other(format!("browser storage listing failed: {e:?}")))?
                .ok_or_else(|| io::Error::other("browser storage changed during listing"))?;
            if key.starts_with(KEY_PREFIX) {
                keys.push(key);
            }
        }
        let final_len = storage
            .length()
            .map_err(|e| io::Error::other(format!("browser storage listing failed: {e:?}")))?;
        if final_len != len {
            return Err(io::Error::other("browser storage changed during listing"));
        }
        Ok(keys)
    }
}

/// Whether the process-wide browser filesystem selected localStorage.
#[cfg(target_arch = "wasm32")]
pub fn global_uses_local_storage() -> bool {
    static AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *AVAILABLE.get_or_init(LocalStorage::available)
}

/// The process-wide browser filesystem (falls back to memory if storage is blocked).
#[cfg(target_arch = "wasm32")]
pub fn global() -> &'static Vfs {
    static VFS: std::sync::OnceLock<Vfs> = std::sync::OnceLock::new();
    VFS.get_or_init(|| {
        if global_uses_local_storage() {
            Vfs::new(Box::new(LocalStorage))
        } else {
            log::warn!("localStorage is unavailable; documents are kept in memory only");
            Vfs::new(Box::<MemoryStore>::default())
        }
    })
}

/// Copy the exact localStorage value, including malformed VFS envelopes.
/// The blob contains UTF-16LE code units without a BOM, not a decoded file.
#[cfg(target_arch = "wasm32")]
pub(crate) fn raw_browser_record(path: &Path) -> io::Result<web_sys::Blob> {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(inline_js = r#"
        export function thinoda_raw_storage_record(storage, key) {
            const value = storage.getItem(key);
            if (typeof value !== "string") throw new Error("Recovery record is missing");
            // Serialize each original UTF-16 code unit, without a BOM. Passing
            // the string to Blob, TextEncoder, or Rust can replace lone surrogates.
            const bytes = new Uint8Array(value.length * 2);
            for (let i = 0; i < value.length; i++) {
                const unit = value.charCodeAt(i);
                bytes[i * 2] = unit & 0xff;
                bytes[i * 2 + 1] = unit >>> 8;
            }
            return new Blob([bytes], { type: "application/octet-stream" });
        }
    "#)]
    extern "C" {
        #[wasm_bindgen(catch)]
        fn thinoda_raw_storage_record(
            storage: &web_sys::Storage,
            key: &str,
        ) -> std::result::Result<web_sys::Blob, JsValue>;
    }

    // Never consult localStorage when the VFS chose the volatile fallback.
    if !global_uses_local_storage() {
        return Err(io::Error::other("browser recovery storage is unavailable"));
    }
    let storage = LocalStorage::storage()
        .ok_or_else(|| io::Error::other("browser storage is unavailable"))?;
    thinoda_raw_storage_record(&storage, &Vfs::key(&normalize(path)))
        .map_err(|_| io::Error::other("browser recovery record could not be read"))
}

/// Milliseconds since the Unix epoch (browser-safe).
pub fn now_ms() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> &Path {
        Path::new(s)
    }

    #[test]
    fn normalizes_separators_dots_and_root() {
        assert_eq!(normalize(p("a/b/../c/./d.txt")), "/a/c/d.txt");
        assert_eq!(normalize(p("\\x\\y")), "/x/y");
        assert_eq!(normalize(p("/")), "/");
        assert_eq!(normalize(p("/../..")), "/");
    }

    #[test]
    fn files_roundtrip_with_metadata_and_binary_content() {
        let vfs = Vfs::memory();
        let bytes = [0u8, 255, 10, 13, 128];
        vfs.write(p("/t/data/x.bin"), &bytes, 42).unwrap();
        assert_eq!(vfs.read(p("/t/data/x.bin")).unwrap(), bytes);
        assert_eq!(
            vfs.metadata(p("/t/data/x.bin")).unwrap(),
            Meta {
                len: 5,
                modified_ms: 42
            }
        );
        assert!(vfs.is_file(p("/t/data/x.bin")));
        assert!(vfs.is_dir(p("/t/data")) && vfs.is_dir(p("/t")) && vfs.is_dir(p("/")));
        assert!(!vfs.is_file(p("/t/data")));
        assert_eq!(
            vfs.read(p("/missing")).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }

    #[test]
    fn directories_are_implicit_and_listings_are_shallow() {
        let vfs = Vfs::memory();
        vfs.write(p("/r/a.json"), b"1", 1).unwrap();
        vfs.write(p("/r/s1/docs/x.json"), b"2", 1).unwrap();
        vfs.write(p("/r/s2/lock"), b"3", 1).unwrap();
        vfs.write(p("/rx/other"), b"4", 1).unwrap();
        let list = vfs.read_dir(p("/r")).unwrap();
        assert_eq!(
            list,
            vec![
                (PathBuf::from("/r/a.json"), Kind::File),
                (PathBuf::from("/r/s1"), Kind::Dir),
                (PathBuf::from("/r/s2"), Kind::Dir),
            ]
        );
        assert!(vfs.read_dir(p("/nope")).is_err());
        assert!(vfs.write(p("/r/s1"), b"x", 1).is_err());
        vfs.remove_dir_all(p("/r/s1")).unwrap();
        assert!(!vfs.exists(p("/r/s1/docs/x.json")));
        assert!(vfs.exists(p("/rx/other")));
        vfs.rename(p("/r/a.json"), p("/r/b.json"), 2).unwrap();
        assert!(!vfs.exists(p("/r/a.json")) && vfs.read(p("/r/b.json")).unwrap() == b"1");
        vfs.remove_file(p("/r/b.json")).unwrap();
        assert!(vfs.remove_file(p("/r/b.json")).is_err());
    }

    #[test]
    fn heartbeat_locks_exclude_live_owners_and_expire() {
        let vfs = Vfs::memory();
        let lock = p("/s/lock");
        vfs.try_lock(lock, "tab-a", 1_000).unwrap();
        vfs.try_lock(lock, "tab-a", 2_000).unwrap();
        assert_eq!(
            vfs.try_lock(lock, "tab-b", 3_000).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        vfs.unlock(lock, "tab-b").unwrap();
        assert!(vfs.is_file(lock));
        vfs.try_lock(lock, "tab-b", 2_000 + LOCK_STALE_MS).unwrap();
        vfs.unlock(lock, "tab-a").unwrap();
        assert!(vfs.is_file(lock));
        vfs.unlock(lock, "tab-b").unwrap();
        assert!(!vfs.is_file(lock));
    }

    #[test]
    fn refresh_and_resume_never_take_a_different_or_missing_claim() {
        let vfs = Vfs::memory();
        let lock = p("/s/lock");
        vfs.try_lock(lock, "first", 1_000).unwrap();
        vfs.refresh_lock(lock, "first", 2_000).unwrap();
        vfs.try_lock(lock, "second", 2_000 + LOCK_STALE_MS).unwrap();
        let later = 2_000 + LOCK_STALE_MS * 3;
        assert_eq!(
            vfs.refresh_lock(lock, "first", later).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(
            vfs.try_resume_lock(lock, "first", later)
                .unwrap_err()
                .kind(),
            io::ErrorKind::WouldBlock
        );
        vfs.unlock(lock, "first").unwrap();
        assert!(vfs.is_file(lock));
        vfs.unlock(lock, "second").unwrap();
        assert_eq!(
            vfs.refresh_lock(lock, "first", later).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert!(!vfs.is_file(lock));
        vfs.try_resume_lock(lock, "first", later).unwrap();
        vfs.refresh_lock(lock, "first", later + 1).unwrap();
    }

    #[test]
    fn malformed_lock_is_not_treated_as_an_unclaimed_session() {
        let vfs = Vfs::memory();
        let lock = p("/s/lock");
        let key = Vfs::key(&normalize(lock));
        for value in [
            "not json".to_owned(),
            r#"{"m":1,"l":1,"d":"e30="}"#.to_owned(),
            "x".repeat(4097),
        ] {
            vfs.store.set(&key, &value).unwrap();
            for result in [
                vfs.try_lock(lock, "first", 1_000),
                vfs.try_resume_lock(lock, "first", 1_000),
                vfs.refresh_lock(lock, "first", 1_000),
                vfs.unlock(lock, "first"),
            ] {
                assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);
                assert_eq!(vfs.store.get(&key).as_deref(), Some(value.as_str()));
            }
        }
    }

    #[test]
    fn denied_removal_is_reported_without_erasing_the_file() {
        struct Denied(MemoryStore);
        impl KvStore for Denied {
            fn get(&self, key: &str) -> Option<String> {
                self.0.get(key)
            }
            fn set(&self, key: &str, value: &str) -> io::Result<()> {
                self.0.set(key, value)
            }
            fn remove(&self, _: &str) {}
            fn keys(&self) -> Vec<String> {
                self.0.keys()
            }
        }
        let vfs = Vfs::new(Box::new(Denied(MemoryStore::default())));
        vfs.write(p("/s/docs/one"), b"retained", 1).unwrap();
        assert!(vfs.remove_file(p("/s/docs/one")).is_err());
        assert!(vfs.remove_dir_all(p("/s")).is_err());
        assert_eq!(vfs.read(p("/s/docs/one")).unwrap(), b"retained");
        vfs.try_lock(p("/s/lock"), "first", 1).unwrap();
        assert!(vfs.unlock(p("/s/lock"), "first").is_err());
        assert!(vfs.is_file(p("/s/lock")));
    }

    #[test]
    fn denied_lock_reads_and_listings_are_not_treated_as_missing() {
        struct Denied(MemoryStore);
        impl KvStore for Denied {
            fn get(&self, key: &str) -> Option<String> {
                self.0.get(key)
            }
            fn set(&self, key: &str, value: &str) -> io::Result<()> {
                self.0.set(key, value)
            }
            fn remove(&self, key: &str) {
                self.0.remove(key);
            }
            fn keys(&self) -> Vec<String> {
                self.0.keys()
            }
            fn try_get(&self, _: &str) -> io::Result<Option<String>> {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "read denied",
                ))
            }
            fn try_keys(&self) -> io::Result<Vec<String>> {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "list denied",
                ))
            }
        }
        let vfs = Vfs::new(Box::new(Denied(MemoryStore::default())));
        let key = Vfs::key("/s/docs/one");
        vfs.store.set(&key, "retained").unwrap();
        for result in [
            vfs.try_lock(p("/s/lock"), "first", 1),
            vfs.try_resume_lock(p("/s/lock"), "first", 1),
            vfs.refresh_lock(p("/s/lock"), "first", 1),
            vfs.write(p("/s/docs/two"), b"new", 1),
            vfs.remove_dir_all(p("/s")),
        ] {
            assert_eq!(result.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        }
        assert_eq!(
            vfs.read_dir(p("/s")).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        for result in [
            vfs.read(p("/s/docs/one")).map(|_| ()),
            vfs.read_bounded(p("/s/docs/one"), 64).map(|_| ()),
            vfs.metadata(p("/s/docs/one")).map(|_| ()),
            vfs.metadata_bounded(p("/s/docs/one"), 64).map(|_| ()),
        ] {
            assert_eq!(result.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        }
        assert_eq!(vfs.store.get(&key).as_deref(), Some("retained"));
    }

    #[test]
    fn malformed_files_are_reported_as_corrupt_not_missing() {
        let vfs = Vfs::memory();
        let path = p("/s/docs/one");
        let key = Vfs::key(&normalize(path));
        vfs.store.set(&key, "not json").unwrap();
        for result in [
            vfs.read(path).map(|_| ()),
            vfs.read_bounded(path, 64).map(|_| ()),
            vfs.metadata(path).map(|_| ()),
        ] {
            assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);
        }
        vfs.store.set(&key, r#"{"m":1,"l":1,"d":"e30="}"#).unwrap();
        assert_eq!(
            vfs.read(path).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(
            vfs.read_bounded(path, 64).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert!(vfs.store.get(&key).is_some());
    }

    #[test]
    fn store_failures_are_reported() {
        struct Full;
        impl KvStore for Full {
            fn get(&self, _: &str) -> Option<String> {
                None
            }
            fn set(&self, _: &str, _: &str) -> io::Result<()> {
                Err(io::Error::new(io::ErrorKind::StorageFull, "quota"))
            }
            fn remove(&self, _: &str) {}
            fn keys(&self) -> Vec<String> {
                Vec::new()
            }
        }
        let error = Vfs::new(Box::new(Full))
            .write(p("/a"), b"x", 0)
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::StorageFull);
        assert!(error.to_string().contains("storage may be full"));
    }
}
