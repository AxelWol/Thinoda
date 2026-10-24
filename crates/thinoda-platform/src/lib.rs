//! Persistence, recovery, cloud discovery, and native operating-system helpers.
//!
//! # Application lifecycle
//!
//! ```no_run
//! use std::sync::Arc;
//! use thinoda_platform::{Paths, Settings, crash, recovery::{RecoveryManager, Snapshot, SnapshotMeta}};
//!
//! # fn main() -> thinoda_platform::Result<()> {
//! let paths = Paths::new()?; // THINODA_HOME selects an isolated portable root.
//! let settings = Settings::load_with_warning(&paths)?;
//! // Display settings.warning, if present, then offer recovery of these sessions.
//! let orphans = RecoveryManager::find_orphans(&paths);
//! for session in &orphans {
//!     for doc in &session.docs {
//!         let workbook = doc.load()?; // Checks JSON and workbook invariants.
//!         // Open as a dirty document. Discard only after the user resolves it.
//!         # let _ = workbook;
//!     }
//! }
//! let recovery = Arc::new(RecoveryManager::start(&paths)?);
//! crash::install_panic_hook(paths.clone(), Some(Arc::clone(&recovery)));
//! let workbook = thinoda_core::Workbook::default();
//! let metadata = SnapshotMeta { title: "Untitled".into(), dirty: true, ..Default::default() };
//! recovery.queue_snapshot("document-uuid", Snapshot::to_bytes(metadata, &workbook)?)?;
//! // Debounce flushes using settings.settings.recovery_interval_secs (normally 1 s).
//! recovery.flush()?;
//! // After a successful explicit save/clean document close:
//! recovery.remove("document-uuid")?;
//! settings.settings.save(&paths)?;
//! // Only after every document was saved or explicitly discarded:
//! recovery.finish()?;
//! # Ok(())
//! # }
//! ```
//!
//! Autosave is distinct from recovery: only autosave workbooks with an existing
//! original path, and compare a [`fs::FileFingerprint`] before replacing that file.
//! Run disk flushes off the UI thread; queueing holds only a short in-memory lock.
//! The hook retains its recovery `Arc`, so explicit `finish` is important.
//!
//! # Limits
//!
//! Recovery is best-effort after the latest queued/serialized state; it cannot
//! protect edits never queued, hard termination before a flush, failed storage,
//! or a lock held at panic time. Orphan handles keep an exclusive claim until
//! dropped. Fingerprints are metadata, not hashes, and conflict/cloud discovery
//! is heuristic. Platform launchers and actual macOS/Linux integration should
//! additionally be smoke-tested on those operating systems. Local IPC is an
//! at-most-once file-open queue, without acknowledgements or ordering guarantees.

pub mod cloud;
pub mod crash;
pub mod credentials;
pub mod fs;
pub mod native_menu;
pub mod paths;
#[cfg(not(target_arch = "wasm32"))]
pub mod printing;
pub mod recovery;
pub mod settings;
pub mod single_instance;
pub mod system;
pub mod vfs;
pub mod web;

pub use paths::Paths;
pub use settings::Settings;

/// Platform persistence failure.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid custom theme library: {0}")]
    InvalidCustomThemes(String),
    #[error("invalid quick style library: {0}")]
    InvalidQuickStyles(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Toml(#[from] toml::ser::Error),
    #[error("invalid workbook: {0}")]
    InvalidWorkbook(String),
    #[error("invalid document key: {0:?}; use ASCII letters, digits, hyphens or underscores")]
    InvalidDocKey(String),
    #[error("recovery mutex is poisoned")]
    Poisoned,
    #[error("recovery session has already finished")]
    Finished,
    #[error("recovery session is suspended; resume it before modifying snapshots")]
    Suspended,
    #[error("secure credential storage failed: {0}")]
    Credential(String),
}

/// Result returned by platform persistence operations.
pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn unix_seconds() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests;
