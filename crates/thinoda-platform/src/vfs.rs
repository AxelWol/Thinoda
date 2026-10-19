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

    fn entry(&self, path: &str) -> Option<Entry> {
        serde_json::from_str(&self.store.get(&Self::key(path))?).ok()
    }

    /// Normalized paths of every stored file.
    fn files(&self) -> Vec<String> {
        self.store
            .keys()
            .into_iter()
            .filter_map(|k| k.strip_prefix(KEY_PREFIX).map(str::to_owned))
            .collect()
    }

    pub fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        let path = normalize(path);
        let entry = self.entry(&path).ok_or_else(|| not_found(&path))?;
        base64::engine::general_purpose::STANDARD
            .decode(entry.d)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Replace a file in one store operation (atomic with respect to this tab).
    pub fn write(&self, path: &Path, bytes: &[u8], now_ms: u64) -> io::Result<()> {
        let path = normalize(path);
        if path == "/" || self.is_dir_norm(&path) {
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
        if self.store.get(&Self::key(&path)).is_none() {
            return Err(not_found(&path));
        }
        self.store.remove(&Self::key(&path));
        Ok(())
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
        let entry = self.entry(&path).ok_or_else(|| not_found(&path))?;
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
        for file in self.files() {
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
        if children.is_empty() && !self.is_dir_norm(&path) {
            return Err(not_found(&path));
        }
        Ok(children
            .into_iter()
            .map(|(name, kind)| (PathBuf::from(format!("{prefix}{name}")), kind))
            .collect())
    }

    pub fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        let path = normalize(path);
        if !self.is_dir_norm(&path) {
            return Err(not_found(&path));
        }
        let prefix = format!("{path}/");
        for file in self.files() {
            if file.starts_with(&prefix) {
                self.store.remove(&Self::key(&file));
            }
        }
        Ok(())
    }

    /// Acquire or refresh a cooperative cross-tab lock. Fails with `WouldBlock`
    /// while another owner has refreshed it within [`LOCK_STALE_MS`].
    pub fn try_lock(&self, path: &Path, owner: &str, now_ms: u64) -> io::Result<()> {
        if let Some(record) = self
            .read(path)
            .ok()
            .and_then(|b| serde_json::from_slice::<LockRecord>(&b).ok())
            && record.owner != owner
            && now_ms.saturating_sub(record.heartbeat_ms) < LOCK_STALE_MS
        {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "locked by another browser tab",
            ));
        }
        let record = LockRecord {
            owner: owner.into(),
            heartbeat_ms: now_ms,
        };
        self.write(
            path,
            &serde_json::to_vec(&record).map_err(io::Error::other)?,
            now_ms,
        )
    }

    /// Release a lock if it is still held by `owner`.
    pub fn unlock(&self, path: &Path, owner: &str) {
        let held = self
            .read(path)
            .ok()
            .and_then(|b| serde_json::from_slice::<LockRecord>(&b).ok())
            .is_some_and(|record| record.owner == owner);
        if held {
            let _ = self.remove_file(path);
        }
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
        let Some(storage) = Self::storage() else {
            return Vec::new();
        };
        let len = storage.length().unwrap_or(0);
        (0..len)
            .filter_map(|i| storage.key(i).ok().flatten())
            .filter(|k| k.starts_with(KEY_PREFIX))
            .collect()
    }
}

/// The process-wide browser filesystem (falls back to memory if storage is blocked).
#[cfg(target_arch = "wasm32")]
pub fn global() -> &'static Vfs {
    static VFS: std::sync::OnceLock<Vfs> = std::sync::OnceLock::new();
    VFS.get_or_init(|| {
        if LocalStorage::available() {
            Vfs::new(Box::new(LocalStorage))
        } else {
            log::warn!("localStorage is unavailable; documents are kept in memory only");
            Vfs::new(Box::<MemoryStore>::default())
        }
    })
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
        vfs.unlock(lock, "tab-b");
        assert!(vfs.is_file(lock));
        vfs.try_lock(lock, "tab-b", 2_000 + LOCK_STALE_MS).unwrap();
        vfs.unlock(lock, "tab-a");
        assert!(vfs.is_file(lock));
        vfs.unlock(lock, "tab-b");
        assert!(!vfs.is_file(lock));
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
