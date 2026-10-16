//! Advisory-lock primary-instance election and atomic file-based open requests.

use crate::{Paths, Result, fs::atomic_write};
use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

/// A process's primary/secondary role. Drop releases the primary lock; the lock
/// file itself is never deleted to prevent races between inode generations.
pub struct SingleInstance {
    lock: Option<File>,
    ipc_dir: PathBuf,
}
impl SingleInstance {
    /// Elect the primary for this data directory; a busy lock means secondary.
    pub fn acquire(paths: &Paths) -> Result<Self> {
        let ipc_dir = paths.data_dir.join("ipc");
        fs::create_dir_all(&ipc_dir)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(ipc_dir.join("primary.lock"))?;
        let lock = match file.try_lock() {
            Ok(()) => Some(file),
            Err(std::fs::TryLockError::WouldBlock) => None,
            Err(e) => return Err(io::Error::from(e).into()),
        };
        Ok(Self { lock, ipc_dir })
    }

    /// Whether this handle owns the primary-instance lock.
    pub fn is_primary(&self) -> bool {
        self.lock.is_some()
    }

    /// Atomically queue a file-open request. Relative paths become absolute before
    /// forwarding so different process working directories cannot change meaning.
    pub fn forward_open(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        let file = self
            .ipc_dir
            .join(format!("request-{}.json", uuid::Uuid::new_v4()));
        atomic_write(file, &serde_json::to_vec(&path)?, false)?;
        Ok(())
    }

    /// Read pending requests (primary only) in filename order and remove them.
    /// Malformed requests are removed and logged rather than wedging the queue.
    pub fn poll(&self) -> Result<Vec<PathBuf>> {
        if !self.is_primary() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "only the primary instance can poll requests",
            )
            .into());
        }
        let mut files: Vec<_> = fs::read_dir(&self.ipc_dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.starts_with("request-") && s.ends_with(".json"))
            })
            .collect();
        files.sort();
        let mut requests = Vec::new();
        for file in files {
            let bytes = fs::read(&file)?;
            match serde_json::from_slice::<PathBuf>(&bytes) {
                Ok(path) => requests.push(path),
                Err(error) => log::warn!(
                    "Discarding malformed IPC request {}: {error}",
                    file.display()
                ),
            }
            fs::remove_file(file)?;
        }
        Ok(requests)
    }
}
