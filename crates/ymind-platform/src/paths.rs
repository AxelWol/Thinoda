//! User-specific application directories (or an isolated portable root).

use std::{
    io,
    path::{Path, PathBuf},
};

/// Locations of user configuration, persistent data, and disposable caches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
}

impl Paths {
    /// Resolve platform defaults; `YMIND_HOME` overrides every directory.
    /// Directories are not created until an operation needs them.
    pub fn new() -> io::Result<Self> {
        if let Some(root) = std::env::var_os("YMIND_HOME").filter(|s| !s.is_empty()) {
            return Ok(Self::from_root(root));
        }
        let dirs = directories::ProjectDirs::from("com", "yMind", "yMind").ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "cannot determine yMind user directories",
            )
        })?;
        Ok(Self {
            config_dir: dirs.config_dir().into(),
            data_dir: dirs.data_dir().into(),
            cache_dir: dirs.cache_dir().into(),
        })
    }

    /// Use `config`, `data`, and `cache` children of an explicit root.
    /// This explicit constructor intentionally ignores the process environment.
    pub fn from_root(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref();
        Self {
            config_dir: root.join("config"),
            data_dir: root.join("data"),
            cache_dir: root.join("cache"),
        }
    }

    /// Create all application directories, including missing parents.
    pub fn ensure(&self) -> io::Result<()> {
        for path in [&self.config_dir, &self.data_dir, &self.cache_dir] {
            std::fs::create_dir_all(path)?;
        }
        Ok(())
    }
}
