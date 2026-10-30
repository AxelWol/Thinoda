//! Opt-in transcripts, deliberately independent of settings, credentials and maps.
use crate::{Paths, fs};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, io, path::Path};
#[cfg(target_arch = "wasm32")]
mod browser {
    use super::*;
    use wasm_bindgen::prelude::*;

    // Keep an origin-wide Web Lock until tab shutdown. Unlike VFS heartbeat
    // leases, this excludes paused/background tabs without a read/write race.
    #[wasm_bindgen(inline_js = "
let state = 0;
export function chat_lock() {
  if (state === 0) {
    state = 1;
    if (!globalThis.navigator?.locks) { state = -1; return state; }
    navigator.locks.request('thinoda-chat-history-v1', {ifAvailable:true}, lock => {
      if (!lock) { state = 0; return; }
      state = 2;
      return new Promise(() => {});
    }).catch(() => { state = -1; });
  }
  return state;
}
export function chat_read(key, limit) {
  const value = globalThis.localStorage.getItem(key);
  if (value !== null && value.length > limit) throw new Error('History storage exceeds limits');
  return value;
}
")]
    extern "C" {
        fn chat_lock() -> i32;
        #[wasm_bindgen(catch)]
        fn chat_read(key: &str, limit: usize) -> Result<Option<String>, JsValue>;
    }

    fn storage() -> io::Result<web_sys::Storage> {
        if chat_lock() != 2 {
            return Err(invalid(
                "Browser history lock unavailable or preparing; retry after closing other tabs",
            ));
        }
        web_sys::window()
            .ok_or_else(|| invalid("Browser storage unavailable"))?
            .local_storage()
            .map_err(|_| invalid("Browser storage denied"))?
            .ok_or_else(|| invalid("Browser storage unavailable"))
    }
    fn key(paths: &Paths) -> String {
        format!("thinoda.chat:{}", crate::vfs::normalize(&paths.data_dir))
    }
    pub fn read(paths: &Paths) -> io::Result<Vec<u8>> {
        // Do not use the VFS's volatile fallback or swallow storage access errors.
        let _storage = storage()?;
        // Check UTF-16 length before copying into WASM (at most 3 * limit
        // UTF-8 bytes), then enforce the exact serialized byte bound below.
        let text = chat_read(&key(paths), MAX_BYTES)
            .map_err(|_| invalid("Browser storage read failed"))?
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
        if text.len() > MAX_BYTES {
            return Err(invalid("History storage exceeds limits"));
        }
        Ok(text.into_bytes())
    }
    pub fn write(paths: &Paths, bytes: &[u8]) -> io::Result<()> {
        storage()?
            .set_item(
                &key(paths),
                std::str::from_utf8(bytes).map_err(|_| invalid("Invalid history"))?,
            )
            .map_err(|_| invalid("Browser history write failed (quota or access)"))
    }
}

pub const MAX_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_DOCUMENTS: usize = 32;
pub const MAX_THREADS: usize = 8;
pub const MAX_MESSAGES: usize = 32;
pub const MAX_THREAD_BYTES: usize = 262_144;
const MAX_FILE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub path: String,
    pub digest: String,
}

impl Binding {
    pub fn from_bytes(path: &Path, bytes: &[u8]) -> io::Result<Self> {
        let canonical = fs::canonicalize(path)?;
        let path = canonical
            .to_str()
            .ok_or_else(|| invalid("Unsupported path"))?;
        if path.len() > 4096 || bytes.len() > MAX_FILE_BYTES {
            return Err(invalid("Document identity exceeds limits"));
        }
        Ok(Self {
            path: path.into(),
            digest: format!("{:x}", Sha256::digest(bytes)),
        })
    }

    pub fn read(path: &Path) -> io::Result<Self> {
        Self::from_bytes(path, &read_bounded(path, MAX_FILE_BYTES)?)
    }

    /// Return the exact bounded bytes used for identity so callers can verify
    /// an asynchronous import report still describes this file version.
    pub fn read_document(path: &Path) -> io::Result<(Self, Vec<u8>)> {
        let bytes = read_bounded(path, MAX_FILE_BYTES)?;
        Ok((Self::from_bytes(path, &bytes)?, bytes))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub assistant: bool,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Thread {
    pub id: String,
    pub name: String,
    pub updated: u64,
    pub messages: Vec<Entry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct History {
    pub binding: Binding,
    pub threads: Vec<Thread>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Store {
    version: u32,
    revision: String,
    pub retention_days: u32,
    pub documents: Vec<History>,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            version: 1,
            revision: String::new(),
            retention_days: 30,
            documents: Vec::new(),
        }
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// Never allocate a native file's unbounded contents before checking its size.
fn read_bounded(path: &Path, limit: usize) -> io::Result<Vec<u8>> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > limit {
            return Err(invalid("History storage exceeds limits"));
        }
        Ok(bytes)
    }
    #[cfg(target_arch = "wasm32")]
    {
        crate::vfs::global().read_bounded(path, limit)
    }
}

pub fn validate_name(name: &str) -> bool {
    !name.trim().is_empty()
        && name == name.trim()
        && name.len() <= 80
        && !name.chars().any(char::is_control)
}

impl Store {
    pub fn load(paths: &Paths) -> io::Result<Self> {
        #[cfg(not(target_arch = "wasm32"))]
        let read = read_bounded(&paths.data_dir.join("ai-conversations.json"), MAX_BYTES);
        #[cfg(target_arch = "wasm32")]
        let read = browser::read(paths);
        let bytes = match read {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(error),
        };
        let store: Self =
            serde_json::from_slice(&bytes).map_err(|_| invalid("Invalid history storage"))?;
        store.validate()?;
        Ok(store)
    }

    pub fn validate(&self) -> io::Result<()> {
        if self.version != 1
            || self.revision.len() > 64
            || self.documents.len() > MAX_DOCUMENTS
            || ![7, 30, 90].contains(&self.retention_days)
        {
            return Err(invalid("Unsupported history storage or limits"));
        }
        let mut bindings = BTreeSet::new();
        for doc in &self.documents {
            if doc.binding.path.is_empty()
                || doc.binding.path.len() > 4096
                || doc.binding.digest.len() != 64
                || !doc.binding.digest.bytes().all(|b| b.is_ascii_hexdigit())
                || !bindings.insert((&doc.binding.path, &doc.binding.digest))
                || doc.threads.len() > MAX_THREADS
            {
                return Err(invalid("Invalid history binding"));
            }
            let mut ids = BTreeSet::new();
            let mut names = BTreeSet::new();
            for thread in &doc.threads {
                if uuid::Uuid::parse_str(&thread.id).is_err()
                    || !ids.insert(&thread.id)
                    || !validate_name(&thread.name)
                    || !names.insert(thread.name.to_lowercase())
                    || thread.messages.len() > MAX_MESSAGES
                    || thread.messages.len() % 2 != 0
                    || thread.messages.iter().map(|m| m.text.len()).sum::<usize>()
                        > MAX_THREAD_BYTES
                {
                    return Err(invalid("Invalid history thread"));
                }
                for (i, message) in thread.messages.iter().enumerate() {
                    if message.assistant != (i % 2 == 1)
                        || message.text.trim().is_empty()
                        || message.text.len() > if message.assistant { 65_536 } else { 8192 }
                    {
                        return Err(invalid("Invalid history message"));
                    }
                }
            }
        }
        Ok(())
    }

    /// Transactional clone-then-commit: callers replace memory only on success.
    /// The OS lock serializes cooperating clients; exact revisions refuse stale writers.
    pub fn commit(&self, paths: &Paths, mut next: Self) -> io::Result<Self> {
        next.validate()?;
        #[cfg(not(target_arch = "wasm32"))]
        let _lock = {
            fs::create_dir_all(&paths.data_dir)?;
            fs::Lock::try_acquire(&paths.data_dir.join("ai-conversations.lock"))?
        };
        if Self::load(paths)? != *self {
            return Err(invalid(
                "History changed elsewhere; reopen the app before saving history",
            ));
        }
        next.revision = uuid::Uuid::new_v4().to_string();
        let bytes = serde_json::to_vec(&next).map_err(|_| invalid("Invalid history storage"))?;
        if bytes.len() > MAX_BYTES {
            return Err(invalid("History storage is full"));
        }
        #[cfg(not(target_arch = "wasm32"))]
        fs::atomic_write(paths.data_dir.join("ai-conversations.json"), &bytes, false)?;
        #[cfg(target_arch = "wasm32")]
        browser::write(paths, &bytes)?;
        Ok(next)
    }

    /// Explicit retention, never automatic while a stream may own a thread.
    pub fn expire(&mut self, now: u64) {
        let cutoff = now.saturating_sub(u64::from(self.retention_days) * 86_400);
        for doc in &mut self.documents {
            doc.threads.retain(|thread| thread.updated >= cutoff);
        }
    }
}
