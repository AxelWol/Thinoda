//! Asynchronous, origin-private browser documents (OPFS with IndexedDB fallback).
//!
//! This deliberately does not replace the synchronous settings/recovery VFS.
//! App document operations must opt into `BrowserStorageProvider` on wasm.
//! Initialization verifies and retains legacy `/Documents` entries; different
//! durable contents are never overwritten by migration.

use base64::Engine as _;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io;

/// Strictly decoded legacy VFS document. Timestamps are not revision identities.
#[derive(Debug, PartialEq, Eq)]
pub struct LegacyDocument {
    pub id: String,
    pub bytes: Vec<u8>,
    pub modified_ms: u64,
}

fn valid_document_id(id: &str) -> bool {
    id.starts_with("/Documents/")
        && !id.contains(['\\', '\0'])
        && id
            .split('/')
            .skip(1)
            .all(|part| !matches!(part, "" | "." | ".."))
}

/// Decode one legacy key without silently discarding malformed entries.
/// Only `/Documents` belongs to this migration; settings/recovery stay in VFS.
pub fn decode_legacy_document(key: &str, raw: &str) -> io::Result<LegacyDocument> {
    #[derive(Deserialize)]
    struct Envelope {
        m: u64,
        l: u64,
        d: String,
    }
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "invalid legacy document");
    let id = key.strip_prefix("thinoda.vfs:").ok_or_else(invalid)?;
    if !valid_document_id(id) {
        return Err(invalid());
    }
    let entry: Envelope = serde_json::from_str(raw).map_err(|_| invalid())?;
    // JS cannot represent larger integers losslessly. Keep the native validation
    // in step with browser migration rather than round timestamps/lengths.
    const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
    if entry.m > MAX_SAFE_INTEGER || entry.l > MAX_SAFE_INTEGER {
        return Err(invalid());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(entry.d)
        .map_err(|_| invalid())?;
    if bytes.len() as u64 != entry.l {
        return Err(invalid());
    }
    Ok(LegacyDocument {
        id: id.into(),
        bytes,
        modified_ms: entry.m,
    })
}

/// Conservative, repeatable migration decision based on verified *bytes*.
#[derive(Debug, PartialEq, Eq)]
pub enum MigrationDecision {
    /// Write with `ExpectedRevision::Missing`, then reread and verify.
    CreateOriginal,
    /// Equal durable bytes have already preserved this legacy content.
    AlreadyVerified,
    /// Keep the durable original and write the legacy bytes to this stable ID
    /// with `Missing`. If the copy exists, verify it; never overwrite it.
    PreserveConflict { copy_id: String },
}

/// Pure migration planner. `durable` must be actual verified stored bytes, not
/// length/mtime or metadata supplied by a cache.
pub fn plan_migration(legacy: &LegacyDocument, durable: Option<&[u8]>) -> MigrationDecision {
    match durable {
        None => MigrationDecision::CreateOriginal,
        Some(bytes) if bytes == legacy.bytes => MigrationDecision::AlreadyVerified,
        Some(_) => {
            let path_hash = format!("{:x}", Sha256::digest(legacy.id.as_bytes()));
            let content_hash = format!("{:x}", Sha256::digest(&legacy.bytes));
            let name = legacy.id.rsplit('/').next().unwrap_or("document");
            MigrationDecision::PreserveConflict {
                copy_id: format!("/Documents/.thinoda-legacy/{path_hash}/{content_hash}/{name}"),
            }
        }
    }
}

/// A conflict preserved without replacing the durable original.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct MigrationConflict {
    pub original: String,
    pub preserved: String,
}

/// Initialization report; retained legacy data is not deleted by this API.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct MigrationReport {
    pub backend: String,
    pub verified: usize,
    pub conflicts: Vec<MigrationConflict>,
}

/// Origin-wide estimates are optional and browser-defined, not available space
/// guarantees. Persistent storage can still be removed explicitly by the user.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct StorageEstimate {
    pub usage: Option<u64>,
    pub quota: Option<u64>,
    pub persisted: Option<bool>,
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::{MigrationReport, StorageEstimate};
    use crate::storage::{
        Availability, CancelToken, Capabilities, Entry, ExpectedRevision, ReadResult, Revision,
        StorageError, StorageFuture, StorageId, StorageProvider,
    };
    use js_sys::{Function, Promise, Reflect, Uint8Array};
    use serde::Deserialize;
    use std::io;
    use wasm_bindgen::{JsCast, JsValue, closure::Closure, prelude::wasm_bindgen};
    use wasm_bindgen_futures::JsFuture;

    #[wasm_bindgen(module = "/src/browser_storage.js")]
    extern "C" {
        #[wasm_bindgen(catch, js_name = initializeDocuments)]
        fn js_initialize() -> Result<Promise, JsValue>;
        #[wasm_bindgen(catch, js_name = listDocuments)]
        fn js_list(id: &str, cancel: &Function) -> Result<Promise, JsValue>;
        #[wasm_bindgen(catch, js_name = readDocument)]
        fn js_read(id: &str, cancel: &Function) -> Result<Promise, JsValue>;
        #[wasm_bindgen(catch, js_name = writeDocument)]
        fn js_write(
            id: &str,
            bytes: &Uint8Array,
            expected: &str,
            cancel: &Function,
            backup: bool,
        ) -> Result<Promise, JsValue>;
        #[wasm_bindgen(catch, js_name = documentRevision)]
        fn js_revision(id: &str, cancel: &Function) -> Result<Promise, JsValue>;
        #[wasm_bindgen(catch, js_name = collectGarbage)]
        fn js_collect_garbage() -> Result<Promise, JsValue>;
        #[wasm_bindgen(catch, js_name = storageEstimate)]
        fn js_estimate() -> Result<Promise, JsValue>;
        #[wasm_bindgen(catch, js_name = requestPersistence)]
        fn js_persist() -> Result<Promise, JsValue>;
        #[wasm_bindgen(js_name = fileSystemAccessSupported)]
        fn js_fsa_supported() -> bool;
        #[wasm_bindgen(catch, js_name = openPickedFile)]
        fn js_open_picked() -> Result<Promise, JsValue>;
        #[wasm_bindgen(catch, js_name = readPickedFile)]
        fn js_read_picked(session: &str, cancel: &Function) -> Result<Promise, JsValue>;
        #[wasm_bindgen(catch, js_name = writePickedFile)]
        fn js_write_picked(
            session: &str,
            bytes: &Uint8Array,
            expected: &str,
            cancel: &Function,
        ) -> Result<Promise, JsValue>;
        #[wasm_bindgen(js_name = forgetPickedFile)]
        fn js_forget_picked(session: &str);
    }

    fn field(value: &JsValue, name: &str) -> Result<JsValue, StorageError> {
        Reflect::get(value, &JsValue::from_str(name)).map_err(error)
    }

    fn text(value: &JsValue) -> Result<String, StorageError> {
        value.as_string().ok_or_else(|| {
            StorageError::Io(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid browser storage response",
            ))
        })
    }

    fn error(value: JsValue) -> StorageError {
        let get = |name: &str| Reflect::get(&value, &JsValue::from_str(name)).ok();
        match get("code").and_then(|v| v.as_string()).as_deref() {
            Some("cancelled") => StorageError::Cancelled,
            Some("unsupported") => StorageError::Unsupported,
            Some("conflict") => StorageError::Conflict {
                actual: get("actual").and_then(|v| v.as_string()).map(Revision),
            },
            _ => {
                // Do not echo browser errors containing user filenames/content.
                let kind = if get("name").and_then(|v| v.as_string()).as_deref()
                    == Some("QuotaExceededError")
                {
                    io::ErrorKind::StorageFull
                } else {
                    io::ErrorKind::Other
                };
                StorageError::Io(io::Error::new(
                    kind,
                    "browser storage failed; original data was retained where possible",
                ))
            }
        }
    }

    async fn resolve(promise: Result<Promise, JsValue>) -> Result<JsValue, StorageError> {
        JsFuture::from(promise.map_err(error)?).await.map_err(error)
    }

    fn json<T: for<'de> Deserialize<'de>>(value: &JsValue) -> Result<T, StorageError> {
        serde_json::from_str(&text(value)?).map_err(|_| {
            StorageError::Io(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid browser storage report",
            ))
        })
    }

    fn read_result(value: &JsValue) -> Result<ReadResult, StorageError> {
        let bytes = field(value, "bytes")?
            .dyn_into::<Uint8Array>()
            .map_err(error)?
            .to_vec();
        Ok(ReadResult {
            bytes,
            revision: Revision(text(&field(value, "revision")?)?),
        })
    }

    fn expected_text(expected: ExpectedRevision) -> String {
        match expected {
            ExpectedRevision::Missing => "missing".into(),
            ExpectedRevision::Match(revision) => format!("match:{}", revision.0),
            ExpectedRevision::Any => "any".into(),
        }
    }

    fn cancellation(cancel: CancelToken) -> Function {
        // Let JS own the closure while a promise is in flight, including when
        // the caller drops the Rust future. A dropped stack-owned Closure would
        // otherwise invalidate JS's callback before its next checkpoint.
        Closure::<dyn Fn() -> bool>::new(move || cancel.is_cancelled())
            .into_js_value()
            .unchecked_into()
    }

    /// Durable storage is not a cloud connection. Listing never reads blobs.
    /// Revisions are SHA-256 of the bytes, checked atomically in IndexedDB.
    #[derive(Clone, Copy, Debug, Default)]
    pub struct BrowserStorageProvider {
        backup: bool,
    }

    impl BrowserStorageProvider {
        /// `backup` keeps the replaced record as `<id>.bak` (same naming as the
        /// VFS `backup_path`), committed in the same IndexedDB transaction.
        pub const fn new(backup: bool) -> Self {
            Self { backup }
        }
    }

    /// Current committed revision of a durable document (`None` if missing),
    /// for `ExpectedRevision` on save-as/replace. Does not read blob bytes.
    pub async fn document_revision(
        id: &StorageId,
        cancel: CancelToken,
    ) -> Result<Option<Revision>, StorageError> {
        cancel.check()?;
        let callback = cancellation(cancel);
        Ok(resolve(js_revision(&id.0, &callback))
            .await?
            .as_string()
            .map(Revision))
    }

    impl StorageProvider for BrowserStorageProvider {
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                revisions: true,
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
                let callback = cancellation(cancel);
                let value = resolve(js_list(&location.0, &callback)).await?;
                #[derive(Deserialize)]
                struct WireEntry {
                    id: String,
                    name: String,
                    size: u64,
                    revision: Option<String>,
                    is_dir: bool,
                }
                Ok(json::<Vec<WireEntry>>(&value)?
                    .into_iter()
                    .map(|entry| Entry {
                        id: StorageId(entry.id),
                        name: entry.name,
                        size: entry.size,
                        revision: entry.revision.map(Revision),
                        availability: Availability::Local,
                        is_dir: entry.is_dir,
                    })
                    .collect())
            })
        }

        fn read<'a>(
            &'a self,
            id: &'a StorageId,
            cancel: CancelToken,
        ) -> StorageFuture<'a, ReadResult> {
            Box::pin(async move {
                cancel.check()?;
                let callback = cancellation(cancel);
                let value = resolve(js_read(&id.0, &callback)).await?;
                read_result(&value)
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
                cancel.check()?;
                let callback = cancellation(cancel);
                let value = resolve(js_write(
                    &id.0,
                    &Uint8Array::from(bytes),
                    &expected_text(expected),
                    &callback,
                    self.backup,
                ))
                .await?;
                // No post-commit cancellation: a completed write is successful.
                Ok(Revision(text(&value)?))
            })
        }
    }

    /// Await before services/recovery/UI startup. Never requests persistence or
    /// picker permissions. Errors must block startup or offer explicit recovery.
    pub async fn initialize_documents() -> Result<MigrationReport, StorageError> {
        json(&resolve(js_initialize()).await?)
    }

    /// Removes OPFS blobs no record references (interrupted writes or failed
    /// post-save reclamation). Runs during initialization; requires Web Locks to
    /// avoid in-flight writes from other tabs, otherwise it removes nothing.
    /// Returns the number of blobs removed.
    pub async fn collect_garbage() -> Result<u32, StorageError> {
        resolve(js_collect_garbage())
            .await?
            .as_f64()
            .map(|count| count as u32)
            .ok_or_else(|| {
                StorageError::Io(io::Error::other("invalid garbage collection response"))
            })
    }

    pub async fn storage_estimate() -> Result<StorageEstimate, StorageError> {
        json(&resolve(js_estimate()).await?)
    }

    /// Explicit user action only. `false` is a real denial, not success.
    pub async fn request_persistence() -> Result<bool, StorageError> {
        resolve(js_persist())
            .await?
            .as_bool()
            .ok_or_else(|| StorageError::Io(io::Error::other("invalid persistence response")))
    }

    pub fn file_system_access_supported() -> bool {
        js_fsa_supported()
    }

    /// A tab-lifetime session referring only to an explicitly picked file.
    /// Do not persist `session` as a recent-file ID; it expires on reload.
    #[derive(Debug)]
    pub struct PickedFile {
        pub session: String,
        pub name: String,
        pub content: ReadResult,
    }

    /// Poll immediately from a user gesture, not after other awaited work.
    pub async fn open_picked_file() -> Result<PickedFile, StorageError> {
        let value = resolve(js_open_picked()).await?;
        Ok(PickedFile {
            session: text(&field(&value, "session")?)?,
            name: text(&field(&value, "name")?)?,
            content: read_result(&value)?,
        })
    }

    pub async fn read_picked_file(
        session: &str,
        cancel: CancelToken,
    ) -> Result<ReadResult, StorageError> {
        cancel.check()?;
        let callback = cancellation(cancel);
        read_result(&resolve(js_read_picked(session, &callback)).await?)
    }

    /// Best-effort external-change detection, not atomic OS-level CAS. Save is
    /// serialized with other picked-file saves on this origin using Web Locks.
    /// Never fall back to a download after a denied/failed picked-file save.
    pub async fn write_picked_file(
        session: &str,
        bytes: &[u8],
        expected: ExpectedRevision,
        cancel: CancelToken,
    ) -> Result<Revision, StorageError> {
        cancel.check()?;
        let callback = cancellation(cancel);
        let value = resolve(js_write_picked(
            session,
            &Uint8Array::from(bytes),
            &expected_text(expected),
            &callback,
        ))
        .await?;
        Ok(Revision(text(&value)?))
    }

    /// Release a selected handle when its document closes.
    pub fn forget_picked_file(session: &str) {
        js_forget_picked(session);
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy(bytes: &[u8]) -> LegacyDocument {
        LegacyDocument {
            id: "/Documents/folder/map.thinoda".into(),
            bytes: bytes.to_vec(),
            modified_ms: 42,
        }
    }

    #[test]
    fn strict_binary_decoding() {
        let document = decode_legacy_document(
            "thinoda.vfs:/Documents/a",
            r#"{"m":42,"l":4,"d":"AP8KgA=="}"#,
        )
        .unwrap();
        assert_eq!(document.bytes, [0, 255, 10, 128]);
        assert_eq!(document.modified_ms, 42);
    }

    #[test]
    fn malformed_entries_are_errors_not_missing_documents() {
        for raw in [
            "null",
            "{}",
            r#"{"m":0,"l":1,"d":""}"#,
            r#"{"m":0,"l":1,"d":"@@=="}"#,
            r#"{"m":0,"l":1,"d":"AB=="}"#,
            r#"{"m":0,"l":1,"d":" AA=="}"#,
            r#"{"m":-1,"l":0,"d":""}"#,
            r#"{"m":1.0,"l":0,"d":""}"#,
            r#"{"m":1e0,"l":0,"d":""}"#,
            r#"{"m":0,"m":1,"l":0,"d":""}"#,
            r#"{"m":0,"\u006d":1,"l":0,"d":""}"#,
            r#"{"m":9007199254740992,"l":0,"d":""}"#,
        ] {
            assert!(decode_legacy_document("thinoda.vfs:/Documents/a", raw).is_err());
        }
    }

    #[test]
    fn migration_cannot_take_settings_recovery_or_noncanonical_paths() {
        for key in [
            "thinoda.vfs:/Documents",
            "thinoda.vfs:/Documents/",
            "thinoda.vfs:/Documents/../settings.toml",
            "thinoda.vfs:/Documents/a//b",
            "thinoda.vfs:/Documents/a\\b",
            "thinoda.vfs:/Documents-old/a",
            "thinoda.vfs:/recovery/a",
            "thinoda.vfs:/settings.toml",
        ] {
            assert!(decode_legacy_document(key, r#"{"m":0,"l":0,"d":""}"#).is_err());
        }
    }

    #[test]
    fn missing_equal_and_different_content_have_safe_decisions() {
        let old = legacy(b"legacy");
        assert_eq!(
            plan_migration(&old, None),
            MigrationDecision::CreateOriginal
        );
        assert_eq!(
            plan_migration(&old, Some(b"legacy")),
            MigrationDecision::AlreadyVerified
        );
        assert!(matches!(
            plan_migration(&old, Some(b"newer!")),
            MigrationDecision::PreserveConflict { .. }
        ));
    }

    #[test]
    fn conflicts_are_stable_and_independent_of_timestamps_and_durable_content() {
        let mut old = legacy(b"legacy");
        let decision = plan_migration(&old, Some(b"newer"));
        old.modified_ms = u64::MAX;
        assert_eq!(plan_migration(&old, Some(b"even newer")), decision);
        let MigrationDecision::PreserveConflict { copy_id } = decision else {
            panic!("expected preserved conflict");
        };
        assert!(valid_document_id(&copy_id));
        assert!(copy_id.ends_with("/map.thinoda"));
        assert_ne!(
            plan_migration(&legacy(b"other legacy"), Some(b"newer")),
            plan_migration(&old, Some(b"newer"))
        );
        old.id = "/Documents/different/map.thinoda".into();
        assert_ne!(
            plan_migration(&old, Some(b"newer")),
            plan_migration(&legacy(b"legacy"), Some(b"newer"))
        );
    }
}
