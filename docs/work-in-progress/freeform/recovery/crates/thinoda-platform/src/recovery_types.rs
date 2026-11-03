//! Recovery snapshot format shared by native and browser storage.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
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

/// On-disk envelope; callers can serialize before queuing the bytes.
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

pub(crate) fn check_key(key: &str) -> Result<()> {
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
