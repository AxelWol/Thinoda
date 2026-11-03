//! Asynchronous, transaction-confirmed browser recovery.
//!
//! The browser VFS remains the reader for older localStorage recovery files.
//! New snapshots use IndexedDB and are never reported as durable merely because
//! they have been queued in memory.

pub use crate::recovery_types::{Snapshot, SnapshotMeta};
use crate::{Error, Paths, Result, recovery_types::check_key, vfs};
use base64::Engine as _;
use js_sys::{Array, Promise, Reflect, Uint8Array};
use serde::Deserialize;
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
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

#[wasm_bindgen(module = "/src/recovery_db.js")]
extern "C" {
    #[wasm_bindgen(js_name = recoveryStartupDeadline)]
    fn startup_deadline() -> f64;
    #[wasm_bindgen(js_name = recoveryStartupGeneration)]
    fn startup_generation() -> f64;
    #[wasm_bindgen(js_name = recoveryStartupHasTime)]
    fn startup_has_time(deadline: f64, generation: f64) -> bool;
    #[wasm_bindgen(js_name = recoveryStartupIsCurrent)]
    fn startup_is_current(generation: f64) -> bool;
    #[wasm_bindgen(catch, js_name = recoveryYield)]
    fn yield_browser() -> std::result::Result<Promise, JsValue>;
    #[wasm_bindgen(catch, js_name = recoveryAfterFirstPaint)]
    fn after_first_paint() -> std::result::Result<Promise, JsValue>;
    #[wasm_bindgen(catch, js_name = recoveryAccessibilityStatus)]
    fn accessibility_status(
        phase: &str,
        message: &str,
        retry_label: &str,
        review_label: &str,
        can_retry: bool,
        can_review: bool,
        model_json: &str,
    ) -> std::result::Result<(), JsValue>;
    #[wasm_bindgen(js_name = recoveryTakeAccessibilityAction)]
    fn take_accessibility_action() -> String;
    #[wasm_bindgen(catch, js_name = recoveryOpenRescue)]
    fn open_rescue(config_json: &str) -> std::result::Result<(), JsValue>;
    #[wasm_bindgen(catch, js_name = recoveryReadLegacyValue)]
    fn read_legacy_value(key: &str) -> std::result::Result<Option<String>, JsValue>;
    #[wasm_bindgen(catch, js_name = recoveryInitialize)]
    fn initialize(deadline: f64, generation: f64) -> std::result::Result<Promise, JsValue>;
    #[wasm_bindgen(catch, js_name = recoveryCreate)]
    fn create(id: &str, deadline: f64, generation: f64) -> std::result::Result<Promise, JsValue>;
    #[wasm_bindgen(catch, js_name = recoveryClaimLegacy)]
    fn claim_legacy(deadline: f64, generation: f64) -> std::result::Result<Promise, JsValue>;
    #[wasm_bindgen(catch, js_name = recoveryDiscover)]
    fn discover(deadline: f64, generation: f64) -> std::result::Result<Promise, JsValue>;
    #[wasm_bindgen(catch, js_name = recoveryQueue)]
    fn queue(id: &str, key: &str, bytes: &[u8]) -> std::result::Result<f64, JsValue>;
    #[wasm_bindgen(catch, js_name = recoveryPersist)]
    fn persist(id: &str, key: &str, bytes: &[u8]) -> std::result::Result<Promise, JsValue>;
    #[wasm_bindgen(catch, js_name = recoveryRemove)]
    fn remove(id: &str, key: &str) -> std::result::Result<String, JsValue>;
    #[wasm_bindgen(catch, js_name = recoveryStatus)]
    fn status(id: &str) -> std::result::Result<String, JsValue>;
    #[wasm_bindgen(js_name = recoveryHasUnconfirmedWork)]
    fn has_unconfirmed_work() -> bool;
    #[wasm_bindgen(js_name = recoveryAcknowledged)]
    fn acknowledged(id: &str, key: &str, revision: f64) -> bool;
    #[wasm_bindgen(js_name = recoveryCurrent)]
    fn current(id: &str, key: &str, revision: f64) -> bool;
    #[wasm_bindgen(catch, js_name = recoveryTakeError)]
    fn take_error(id: &str) -> std::result::Result<String, JsValue>;
    #[wasm_bindgen(catch, js_name = recoveryFinish)]
    fn finish(id: &str, lease: &str) -> std::result::Result<String, JsValue>;
    #[wasm_bindgen(catch, js_name = recoveryHeld)]
    fn held(id: &str, lease: &str) -> std::result::Result<bool, JsValue>;
    #[wasm_bindgen(catch, js_name = recoveryRelease)]
    fn release(id: &str, lease: &str) -> std::result::Result<(), JsValue>;
    #[wasm_bindgen(catch, js_name = recoverySuspend)]
    fn suspend_if_idle(id: &str) -> std::result::Result<String, JsValue>;
    #[wasm_bindgen(catch, js_name = recoveryPauseStartup)]
    fn pause_startup(keep_live_writer: bool) -> std::result::Result<String, JsValue>;
    #[wasm_bindgen(catch, js_name = recoveryResumeAll)]
    fn resume_all() -> std::result::Result<Promise, JsValue>;
    #[wasm_bindgen(catch, js_name = recoveryRetryWriter)]
    fn retry_writer(id: &str) -> std::result::Result<Promise, JsValue>;
    #[wasm_bindgen(catch, js_name = recoveryDiscard)]
    fn discard(
        id: &str,
        key: &str,
        revision: f64,
        lease: &str,
        expected_bytes: &[u8],
    ) -> std::result::Result<Promise, JsValue>;
    #[wasm_bindgen(catch, js_name = recoveryCompleteDiscard)]
    fn complete_discard(
        id: &str,
        key: &str,
        revision: f64,
        lease: &str,
        expected_bytes: &[u8],
    ) -> std::result::Result<Promise, JsValue>;
    #[wasm_bindgen(catch, js_name = recoveryCancelDiscard)]
    fn cancel_discard(
        id: &str,
        key: &str,
        revision: f64,
        lease: &str,
        expected_bytes: &[u8],
    ) -> std::result::Result<Promise, JsValue>;
}

fn browser_error(value: JsValue) -> Error {
    let property = |name| {
        Reflect::get(&value, &JsValue::from_str(name))
            .ok()
            .and_then(|v| v.as_string())
    };
    let name = property("name").unwrap_or_default();
    let message = property("message")
        .or_else(|| value.as_string())
        .unwrap_or_else(|| "Browser recovery storage failed.".into());
    if name == "RecoveryOutcomeUnknown" {
        Error::RecoveryOutcomeUnknown(message)
    } else if name == "QuotaExceededError" {
        Error::Io(io::Error::new(
            io::ErrorKind::StorageFull,
            "Browser recovery storage is full. Save or download your work and free site storage.",
        ))
    } else {
        Error::BrowserRecovery(message)
    }
}

async fn wait(promise: std::result::Result<Promise, JsValue>) -> Result<JsValue> {
    JsFuture::from(promise.map_err(browser_error)?)
        .await
        .map_err(browser_error)
}

fn field(value: &JsValue, key: &str) -> Result<JsValue> {
    Reflect::get(value, &JsValue::from_str(key)).map_err(browser_error)
}

#[derive(Deserialize)]
struct BrowserStatus {
    active: bool,
    pending: bool,
    error: String,
    persistent: bool,
    strict_durability: bool,
}

#[derive(Deserialize)]
struct BrowserInitialization {
    persistent: bool,
    warning: String,
}

#[derive(Clone, Copy)]
pub struct BrowserStartupBudget {
    deadline: f64,
    generation: f64,
}

/// A browser recovery session. IndexedDB owns its bytes and Web Locks own its
/// session lease; neither JS handle is stored inside the `Send + Sync` manager.
pub struct RecoveryManager {
    session_id: String,
    lease: String,
    session_dir: PathBuf,
    persistent: bool,
    closed: AtomicBool,
    preserve: AtomicBool,
}

const LEGACY_LEASE_ID: &str = "legacy-localstorage";
const MAX_SNAPSHOT_PREVIEW_BYTES: usize = 32 * 1024 * 1024;
const MAX_METADATA_PREFIX_BYTES: usize = 64 * 1024;
const MAX_LEGACY_STORAGE_KEYS: u32 = 4096;
const MAX_LEGACY_KEY_BYTES: usize = 4096;
const MAX_LEGACY_SESSIONS: usize = 256;
const MAX_LEGACY_DOCUMENTS: usize = 512;
const MAX_LEGACY_ENVELOPE_BYTES: usize = 32 * 1024 * 1024;
const MAX_LEGACY_TOTAL_BYTES: usize = 64 * 1024 * 1024;

const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<RecoveryManager>();
};

fn add_warning(target: &mut String, message: &str) {
    if message.is_empty() || target.contains(message) {
        return;
    }
    if !target.is_empty() {
        target.push(' ');
    }
    target.push_str(message);
}

/// Releases raw JS claims unless ownership was transferred to an OrphanDoc.
struct DiscoveryClaims(Vec<(String, String)>);

impl DiscoveryClaims {
    fn transfer(&mut self, id: &str, lease: &str) {
        self.0
            .retain(|candidate| candidate.0 != id || candidate.1 != lease);
    }
}

impl Drop for DiscoveryClaims {
    fn drop(&mut self) {
        for (id, lease) in &self.0 {
            let _ = release(id, lease);
        }
    }
}

async fn browser_discovery(
    budget: BrowserStartupBudget,
    initialize_store: bool,
) -> Result<(Array, DiscoveryClaims, String)> {
    let mut warning = String::new();
    if initialize_store {
        let initialized = wait(initialize(budget.deadline, budget.generation))
            .await?
            .as_string()
            .ok_or_else(|| Error::BrowserRecovery("Invalid browser recovery status.".into()))?;
        let initialized: BrowserInitialization = serde_json::from_str(&initialized)?;
        warning = initialized.warning;
    }
    let found = wait(discover(budget.deadline, budget.generation)).await?;
    add_warning(
        &mut warning,
        &field(&found, "warning")?.as_string().unwrap_or_default(),
    );
    let sessions = Array::from(&field(&found, "sessions")?);
    let mut claims = DiscoveryClaims(Vec::new());
    for session in sessions.iter() {
        let id = field(&session, "id")?
            .as_string()
            .ok_or_else(|| Error::BrowserRecovery("Invalid recovery session id.".into()))?;
        let lease = field(&session, "lease")?
            .as_string()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::BrowserRecovery("Invalid recovery session claim.".into()))?;
        claims.0.push((id, lease));
    }
    Ok((sessions, claims, warning))
}

async fn parse_browser_sessions(
    paths: &Paths,
    sessions: Array,
    mut claims: DiscoveryClaims,
    budget: BrowserStartupBudget,
) -> Result<(Vec<OrphanSession>, bool, bool)> {
    let mut orphans = Vec::new();
    let mut deferred = false;
    let mut unverified_meta = false;
    for (session_index, session) in sessions.iter().enumerate() {
        if !startup_has_time(budget.deadline, budget.generation) {
            deferred = true;
            break;
        }
        let id = field(&session, "id")?
            .as_string()
            .ok_or_else(|| Error::BrowserRecovery("Invalid recovery session id.".into()))?;
        let lease_token = field(&session, "lease")?
            .as_string()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::BrowserRecovery("Invalid recovery session claim.".into()))?;
        if !held(&id, &lease_token).map_err(browser_error)? {
            return Err(Error::BrowserRecovery(
                "Recovery discovery lost its tab ownership; retry after returning.".into(),
            ));
        }
        let lease = Arc::new(OrphanLease {
            kind: LeaseKind::Browser {
                id: id.clone(),
                token: lease_token.clone(),
            },
        });
        let entries = Array::from(&field(&session, "docs")?);
        let mut docs = Vec::new();
        for (index, entry) in entries.iter().enumerate() {
            if !startup_has_time(budget.deadline, budget.generation) {
                deferred = true;
                break;
            }
            let key = field(&entry, "key")?
                .as_string()
                .ok_or_else(|| Error::BrowserRecovery("Invalid recovery document key.".into()))?;
            check_key(&key)?;
            let revision = field(&entry, "revision")?
                .as_f64()
                .filter(|revision| {
                    revision.is_finite()
                        && *revision > 0.0
                        && *revision <= 9_007_199_254_740_991.0
                        && revision.fract() == 0.0
                })
                .ok_or_else(|| Error::BrowserRecovery("Invalid recovery revision.".into()))?;
            let bytes = Uint8Array::new(&field(&entry, "bytes")?);
            if bytes.length() as usize > MAX_SNAPSHOT_PREVIEW_BYTES {
                return Err(Error::BrowserRecovery(
                    "A recovery snapshot exceeds the safe preview limit; its original was retained."
                        .into(),
                ));
            }
            let bytes = bytes.to_vec();
            let discard_pending = field(&entry, "pendingDiscard")?.as_bool().ok_or_else(|| {
                Error::BrowserRecovery("Invalid pending discard status; retry recovery.".into())
            })?;
            let (meta, verified) = snapshot_meta(&bytes, &key);
            unverified_meta |= !verified;
            docs.push(OrphanDoc {
                session_id: id.clone(),
                doc_key: key.clone(),
                meta,
                path: paths
                    .data_dir
                    .join("recovery-indexeddb")
                    .join(&id)
                    .join(format!("{key}.json")),
                source: SnapshotSource::Browser {
                    bytes: Arc::new(bytes),
                    revision,
                },
                lease: Arc::clone(&lease),
                requires_recheck: false,
                discard_pending,
            });
            let more = index + 1 < entries.length() as usize
                || session_index + 1 < sessions.length() as usize;
            if more {
                // Individual JSON parses cannot be preempted; yielding here
                // prevents a long list of snapshots from monopolizing frames.
                wait(yield_browser()).await?;
                if !held(&id, &lease_token).map_err(browser_error)? {
                    return Err(Error::BrowserRecovery(
                        "Recovery discovery lost its tab ownership; retry after returning.".into(),
                    ));
                }
            }
        }
        if !docs.is_empty() {
            docs.sort_by(|a, b| a.doc_key.cmp(&b.doc_key));
            claims.transfer(&id, &lease_token);
            orphans.push(OrphanSession {
                session_id: id,
                docs,
            });
        }
        if deferred {
            break;
        }
    }
    orphans.sort_by(|a, b| a.session_id.cmp(&b.session_id));
    Ok((orphans, deferred, unverified_meta))
}

impl RecoveryManager {
    /// Open an explicitly requested, read-only rescue panel. It is independent
    /// of recovery discovery and cannot change the editing gate.
    pub fn open_browser_rescue(config_json: &str) -> Result<()> {
        open_rescue(config_json).map_err(browser_error)
    }

    /// Exact localStorage prefix used by this installation's legacy VFS.
    pub fn browser_legacy_rescue_prefix(paths: &Paths) -> String {
        let root = paths.data_dir.join("recovery");
        format!("{}{}/", vfs::KEY_PREFIX, vfs::normalize(&root))
    }

    /// Publish the same visible recovery state to browser assistive technology.
    pub fn publish_browser_status(
        phase: &str,
        message: &str,
        retry_label: &str,
        review_label: &str,
        can_retry: bool,
        can_review: bool,
        model_json: &str,
    ) {
        if let Err(error) = accessibility_status(
            phase,
            message,
            retry_label,
            review_label,
            can_retry,
            can_review,
            model_json,
        ) {
            log::warn!(
                "Browser recovery accessibility status: {}",
                browser_error(error)
            );
        }
    }

    /// Take an explicitly activated DOM retry/review action, if any.
    pub fn take_browser_accessibility_action() -> String {
        take_accessibility_action()
    }

    /// Delay discovery until the shell has had an opportunity to paint.
    pub async fn after_first_browser_paint() -> Result<()> {
        wait(after_first_paint()).await?;
        Ok(())
    }

    /// One absolute budget shared by legacy discovery, IndexedDB discovery,
    /// persistence permission, and starting the replacement session.
    pub fn browser_startup_budget() -> BrowserStartupBudget {
        BrowserStartupBudget {
            deadline: startup_deadline(),
            generation: startup_generation(),
        }
    }

    /// A result from before pagehide must not be installed after BFCache
    /// resumes, even when its asynchronous work completed successfully.
    pub fn browser_startup_is_current(budget: BrowserStartupBudget) -> bool {
        startup_is_current(budget.generation)
    }

    /// Create a fresh writer independently of discovery. Its random session
    /// cannot overwrite unknown orphan sessions, but the application must
    /// still gate edits until discovery proves document-level safety.
    pub async fn start_browser_writer(
        paths: &Paths,
        budget: BrowserStartupBudget,
    ) -> Result<(Self, Option<String>)> {
        let initialized = wait(initialize(budget.deadline, budget.generation))
            .await?
            .as_string()
            .ok_or_else(|| Error::BrowserRecovery("Invalid browser recovery status.".into()))?;
        let initialized: BrowserInitialization = serde_json::from_str(&initialized)?;
        let session_id = uuid::Uuid::new_v4().to_string();
        let lease = wait(create(&session_id, budget.deadline, budget.generation))
            .await?
            .as_string()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::BrowserRecovery("Invalid recovery writer claim.".into()))?;
        Ok((
            Self {
                session_dir: paths.data_dir.join("recovery-indexeddb").join(&session_id),
                session_id,
                lease,
                persistent: initialized.persistent,
                closed: AtomicBool::new(false),
                preserve: AtomicBool::new(false),
            },
            (!initialized.warning.is_empty()).then_some(initialized.warning),
        ))
    }

    /// Discover or retry without replacing the live writer or its snapshots.
    pub async fn retry_browser_discovery(
        paths: &Paths,
        budget: BrowserStartupBudget,
    ) -> Result<(Vec<OrphanSession>, Option<String>)> {
        Self::discover_browser(paths, budget, true).await
    }

    /// Discover immediately after successfully starting the writer, without
    /// requesting persistence a second time.
    pub async fn discover_after_browser_writer(
        paths: &Paths,
        budget: BrowserStartupBudget,
    ) -> Result<(Vec<OrphanSession>, Option<String>)> {
        Self::discover_browser(paths, budget, false).await
    }

    async fn discover_browser(
        paths: &Paths,
        budget: BrowserStartupBudget,
        initialize_store: bool,
    ) -> Result<(Vec<OrphanSession>, Option<String>)> {
        let (sessions, claims, mut warning) = browser_discovery(budget, initialize_store).await?;
        let (orphans, deferred, unverified_meta) =
            parse_browser_sessions(paths, sessions, claims, budget).await?;
        if deferred {
            add_warning(
                &mut warning,
                "Some recovery discovery was deferred. Resolve the shown items, then retry; remaining snapshots were not deleted.",
            );
        }
        if unverified_meta {
            add_warning(
                &mut warning,
                "Some recovery details could not be verified within the preview limit. Originals were retained; download the raw data or retry.",
            );
        }
        Ok((orphans, (!warning.is_empty()).then_some(warning)))
    }

    /// Browser initialization is necessarily asynchronous.
    pub fn start(_paths: &Paths) -> Result<Self> {
        Err(Error::BrowserRecovery(
            "Browser recovery requires asynchronous initialization.".into(),
        ))
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn session_dir(&self) -> &Path {
        &self.session_dir
    }

    pub fn eviction_protected(&self) -> bool {
        self.browser_status()
            .map(|status| status.persistent)
            .unwrap_or(self.persistent)
    }

    /// Strict is a browser durability hint, not a guarantee against a device
    /// or browser failure. Missing support must remain visible to the user.
    pub fn strict_durability_requested(&self) -> bool {
        self.browser_status()
            .map(|status| status.strict_durability)
            .unwrap_or(false)
    }

    pub fn snapshot(&self, key: &str, meta: SnapshotMeta, workbook: &Workbook) -> Result<()> {
        self.queue_snapshot(key, Snapshot::to_bytes(meta, workbook)?)?;
        self.flush()
    }

    /// A synchronous handoff must not pretend an IndexedDB write has committed.
    pub fn persist_snapshot(
        &self,
        _key: &str,
        _meta: SnapshotMeta,
        _workbook: &Workbook,
    ) -> Result<()> {
        Err(Error::BrowserRecovery(
            "Browser handoff requires an asynchronous commit.".into(),
        ))
    }

    /// Confirm the replacement snapshot's transaction before the caller drops
    /// the original orphan. Failed handoffs are not silently retried.
    pub async fn persist_snapshot_async(
        &self,
        key: &str,
        meta: SnapshotMeta,
        workbook: &Workbook,
    ) -> Result<()> {
        check_key(key)?;
        if self.closed.load(Ordering::Acquire) {
            return Err(Error::Finished);
        }
        let bytes = Snapshot::to_bytes(meta, workbook)?;
        wait(persist(&self.session_id, key, &bytes)).await?;
        Ok(())
    }

    pub fn queue_snapshot(&self, key: &str, bytes: Vec<u8>) -> Result<()> {
        self.queue_snapshot_revision(key, bytes).map(|_| ())
    }

    /// Accept a snapshot and return the writer-local revision to verify later.
    pub fn queue_snapshot_revision(&self, key: &str, bytes: Vec<u8>) -> Result<f64> {
        check_key(key)?;
        if self.closed.load(Ordering::Acquire) {
            return Err(Error::Finished);
        }
        queue(&self.session_id, key, &bytes).map_err(browser_error)
    }

    /// Positive transaction acknowledgement for this exact document revision.
    pub fn snapshot_acknowledged(&self, key: &str, revision: f64) -> bool {
        !self.closed.load(Ordering::Acquire) && acknowledged(&self.session_id, key, revision)
    }

    /// True if this put is still the desired revision, even while pending.
    /// A later same-document tombstone or put makes it false.
    pub fn snapshot_is_current(&self, key: &str, revision: f64) -> bool {
        !self.closed.load(Ordering::Acquire) && current(&self.session_id, key, revision)
    }

    fn browser_status(&self) -> Result<BrowserStatus> {
        let value = status(&self.session_id).map_err(browser_error)?;
        Ok(serde_json::from_str(&value)?)
    }

    /// Return success only after every queued transaction has committed.
    pub fn flush(&self) -> Result<()> {
        if self.closed.load(Ordering::Acquire) {
            return Err(Error::Finished);
        }
        let status = self.browser_status()?;
        if !status.error.is_empty() {
            return Err(Error::BrowserRecovery(status.error));
        }
        if !status.active {
            return Err(Error::Suspended);
        }
        if status.pending {
            return Err(Error::RecoveryPending);
        }
        Ok(())
    }

    /// A new storage failure, suitable for a user-facing notification.
    pub fn take_error(&self) -> Option<String> {
        match take_error(&self.session_id) {
            Ok(message) if !message.is_empty() => Some(message),
            Ok(_) => None,
            Err(error) => Some(browser_error(error).to_string()),
        }
    }

    /// Last observed failure, kept visible until a later transaction succeeds.
    pub fn current_error(&self) -> Option<String> {
        match self.browser_status() {
            Ok(status) if !status.error.is_empty() => Some(status.error),
            Ok(status) if !status.active => {
                Some("Browser recovery is suspended or waiting to resume.".into())
            }
            Ok(_) => None,
            Err(error) => Some(error.to_string()),
        }
    }

    /// True while accepted writes or cleanup are still awaiting confirmation.
    pub fn has_pending(&self) -> bool {
        self.browser_status()
            .map(|status| status.pending)
            .unwrap_or(true)
    }

    /// Also used directly by beforeunload between app frames.
    pub fn browser_has_unconfirmed_work() -> bool {
        has_unconfirmed_work()
    }

    /// Pagehide cannot wait for an asynchronous commit. Retain the lease when
    /// writes are pending or failed (notably for BFCache); an actual page
    /// termination releases Web Locks automatically.
    pub fn suspend(&self) -> Result<()> {
        match suspend_if_idle(&self.session_id)
            .map_err(browser_error)?
            .as_str()
        {
            "suspended" => Ok(()),
            _ => Err(Error::RecoveryPending),
        }
    }

    /// Cancel an unfinished browser discovery and suspend idle leases on
    /// pagehide. A running/failed writer keeps its lock until termination.
    pub fn pause_browser_startup(keep_live_writer: bool) -> Result<()> {
        match pause_startup(keep_live_writer)
            .map_err(browser_error)?
            .as_str()
        {
            "suspended" => Ok(()),
            _ => Err(Error::RecoveryPending),
        }
    }

    /// Reclaim a BFCache session asynchronously, only if no other tab claimed it.
    pub fn resume(&self) -> Result<()> {
        Self::resume_browser_sessions()
    }

    /// Also used when the page was hidden before a writer could be created.
    pub fn resume_browser_sessions() -> Result<()> {
        let promise = resume_all().map_err(browser_error)?;
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(error) = JsFuture::from(promise).await {
                log::warn!("Browser recovery resume failed: {}", browser_error(error));
            }
        });
        Err(Error::RecoveryPending)
    }

    /// Retry this writer without changing its identity or its committed data.
    pub fn retry_browser_writer(&self) -> Result<()> {
        let promise = retry_writer(&self.session_id).map_err(browser_error)?;
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(error) = JsFuture::from(promise).await {
                log::warn!("Browser recovery retry failed: {}", browser_error(error));
            }
        });
        Err(Error::RecoveryPending)
    }

    pub fn try_flush_for_crash(&self) -> bool {
        self.preserve.store(true, Ordering::Release);
        self.flush().is_ok()
    }

    /// Queue a revisioned tombstone; return success only once it has committed.
    pub fn remove(&self, key: &str) -> Result<()> {
        check_key(key)?;
        if self.closed.load(Ordering::Acquire) {
            return Err(Error::Finished);
        }
        match remove(&self.session_id, key)
            .map_err(browser_error)?
            .as_str()
        {
            "committed" => Ok(()),
            _ => Err(Error::RecoveryPending),
        }
    }

    pub fn finish(&self) -> Result<()> {
        if self.closed.load(Ordering::Acquire) {
            return Ok(());
        }
        match finish(&self.session_id, &self.lease) {
            Ok(status) if status == "finished" => {
                self.closed.store(true, Ordering::Release);
                Ok(())
            }
            Ok(_) => Err(Error::RecoveryPending),
            Err(error) => Err(browser_error(error)),
        }
    }

    /// Discover old localStorage/VFS snapshots. New IndexedDB orphans are
    /// discovered asynchronously by [`Self::discover_after_browser_writer`].
    /// Also report when old localStorage snapshots could not be inspected or
    /// locked; a full or denied localStorage must not look like an empty scan.
    /// A Web Lock serializes current browser tabs; old heartbeats are checked
    /// read-only and cannot authorize deletion against older versions.
    pub async fn find_legacy_orphans_with_warning(
        paths: &Paths,
        budget: BrowserStartupBudget,
    ) -> (Vec<OrphanSession>, Option<String>) {
        match wait(claim_legacy(budget.deadline, budget.generation)).await {
            Ok(value) => match value.as_string().filter(|value| !value.is_empty()) {
                Some(token) => {
                    legacy_orphans(paths, Arc::new(LegacyClaim { token }), budget).await
                }
                None => (
                    Vec::new(),
                    Some(
                        "Previous recovery snapshots may be in use by another tab and cannot be inspected here yet."
                            .into(),
                    ),
                ),
            },
            Err(error) => (
                Vec::new(),
                Some(format!("Previous recovery snapshots cannot be claimed safely: {error}")),
            ),
        }
    }
}

impl Drop for RecoveryManager {
    fn drop(&mut self) {
        if !self.preserve.load(Ordering::Acquire) && !self.closed.load(Ordering::Acquire) {
            let _ = self.finish();
        }
        if !self.closed.load(Ordering::Acquire) {
            // A dropped manager cannot own a lock forever. The release keeps
            // committed records; in-flight transactions remain token-fenced.
            let _ = release(&self.session_id, &self.lease);
        }
    }
}

enum LeaseKind {
    Legacy {
        lock: Mutex<Option<LegacyHeartbeat>>,
        claim: Arc<LegacyClaim>,
    },
    Browser {
        id: String,
        token: String,
    },
}

struct LegacyClaim {
    token: String,
}

impl Drop for LegacyClaim {
    fn drop(&mut self) {
        let _ = release(LEGACY_LEASE_ID, &self.token);
    }
}

#[derive(Deserialize)]
struct LegacyLockRecord {
    owner: String,
    heartbeat_ms: u64,
}

/// Inspect the old heartbeat without modifying it. Legacy tabs do not use Web
/// Locks and their heartbeat protocol is not an atomic lock, so it cannot
/// safely authorize a read/compare/delete sequence against another tab.
struct LegacyHeartbeat {
    path: PathBuf,
}

impl LegacyHeartbeat {
    fn acquire(path: PathBuf) -> Result<Self> {
        let heartbeat = Self { path };
        heartbeat.verify()?;
        Ok(heartbeat)
    }

    fn verify(&self) -> Result<()> {
        if let Some(record) = legacy_lock_record(&self.path)? {
            if record.owner.is_empty() {
                return Err(Error::BrowserRecovery(
                    "A previous recovery heartbeat is invalid; its data was retained.".into(),
                ));
            }
            if vfs::now_ms().saturating_sub(record.heartbeat_ms) < vfs::LOCK_STALE_MS {
                return Err(
                    io::Error::new(io::ErrorKind::WouldBlock, "legacy recovery is in use").into(),
                );
            }
        }
        Ok(())
    }

    fn refresh_checked(&self) -> Result<()> {
        self.verify()
    }
}

struct OrphanLease {
    kind: LeaseKind,
}

impl Drop for OrphanLease {
    fn drop(&mut self) {
        if let LeaseKind::Browser { id, token } = &self.kind {
            let _ = release(id, token);
        }
    }
}

impl OrphanLease {
    fn refresh(&self) -> Result<()> {
        match &self.kind {
            LeaseKind::Legacy { lock, claim } => {
                if !held(LEGACY_LEASE_ID, &claim.token).map_err(browser_error)? {
                    return Err(Error::BrowserRecovery(
                        "This legacy recovery session is no longer owned by this tab.".into(),
                    ));
                }
                let guard = lock.lock().map_err(|_| Error::Poisoned)?;
                guard.as_ref().ok_or(Error::Suspended)?.refresh_checked()?;
            }
            LeaseKind::Browser { id, token } => {
                if !held(id, token).map_err(browser_error)? {
                    return Err(Error::BrowserRecovery(
                        "This recovery session is no longer owned by this tab.".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct OrphanSession {
    pub session_id: String,
    pub docs: Vec<OrphanDoc>,
}

impl OrphanSession {
    /// Stable review identity, including the backing store. Legacy and new
    /// sessions can have the same textual session id.
    pub fn review_key(&self) -> String {
        let store = match self.docs.first().map(|doc| &doc.source) {
            Some(SnapshotSource::Legacy { .. }) => "legacy",
            Some(SnapshotSource::Browser { .. }) => "indexeddb",
            None => "empty",
        };
        format!("{store}:{}", self.session_id)
    }

    /// Recheck ownership and any active legacy heartbeat while the prompt is open.
    pub fn refresh(&self) -> Result<()> {
        if self.docs.iter().any(OrphanDoc::requires_recheck) {
            return Err(Error::BrowserRecovery(
                "A cached recovery copy still needs to be rechecked; its bytes were retained."
                    .into(),
            ));
        }
        if let Some(doc) = self.docs.first() {
            doc.lease.refresh()?;
        }
        Ok(())
    }

    /// Keep every cached source visible while releasing its old claim for a
    /// fresh discovery. A cached source cannot be restored or discarded until
    /// an identical source has been claimed again.
    pub fn prepare_browser_retry(&mut self) -> Result<()> {
        for doc in &mut self.docs {
            doc.requires_recheck = true;
        }
        for doc in &self.docs {
            match &doc.lease.kind {
                LeaseKind::Browser { id, token } => {
                    release(id, token).map_err(browser_error)?;
                }
                LeaseKind::Legacy { claim, .. } => {
                    release(LEGACY_LEASE_ID, &claim.token).map_err(browser_error)?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
enum SnapshotSource {
    Legacy { value: Arc<String> },
    Browser { bytes: Arc<Vec<u8>>, revision: f64 },
}

/// What a post-discard source re-read observed.
pub enum RecoverySourceState {
    Present,
    Absent,
}

#[derive(Clone)]
pub struct OrphanDoc {
    pub session_id: String,
    pub doc_key: String,
    pub meta: SnapshotMeta,
    pub path: PathBuf,
    source: SnapshotSource,
    lease: Arc<OrphanLease>,
    requires_recheck: bool,
    discard_pending: bool,
}

impl OrphanDoc {
    /// A retry never substitutes a changed or missing source for the cached
    /// copy already shown to the user.
    pub fn same_recovery_source(&self, other: &Self) -> bool {
        if self.session_id != other.session_id
            || self.doc_key != other.doc_key
            || self.path != other.path
        {
            return false;
        }
        match (&self.source, &other.source) {
            (SnapshotSource::Legacy { value: left }, SnapshotSource::Legacy { value: right }) => {
                left.as_str() == right.as_str()
            }
            (
                SnapshotSource::Browser {
                    bytes: left,
                    revision: left_revision,
                },
                SnapshotSource::Browser {
                    bytes: right,
                    revision: right_revision,
                },
            ) => left_revision == right_revision && left.as_slice() == right.as_slice(),
            _ => false,
        }
    }

    pub fn requires_recheck(&self) -> bool {
        self.requires_recheck
    }

    pub fn discard_pending(&self) -> bool {
        self.discard_pending
    }

    pub fn clear_discard_pending(&mut self) {
        self.discard_pending = false;
    }

    fn browser_lease(&self) -> Result<&str> {
        match &self.lease.kind {
            LeaseKind::Browser { id, token } if id == &self.session_id => Ok(token),
            _ => Err(Error::BrowserRecovery("Invalid recovery lease.".into())),
        }
    }

    /// Old tabs do not participate in Web Locks; their localStorage snapshots
    /// cannot safely be discarded by a non-atomic compare/remove operation.
    pub fn can_discard(&self) -> bool {
        !self.requires_recheck
            && !self.discard_pending
            && matches!(&self.source, SnapshotSource::Browser { .. })
    }

    /// Opaque cached source for a user-requested rescue download. Legacy
    /// entries include their original localStorage envelope.
    pub fn cached_recovery_bytes(&self) -> Vec<u8> {
        match &self.source {
            SnapshotSource::Legacy { value } => value.as_bytes().to_vec(),
            SnapshotSource::Browser { bytes, .. } => bytes.as_ref().clone(),
        }
    }

    /// Validated bytes already held by this tab. This remains available for an
    /// explicit rescue download if a discard's outcome cannot be confirmed.
    pub fn load_cached(&self) -> Result<Workbook> {
        let bytes = match &self.source {
            SnapshotSource::Legacy { value } => decode_legacy_value(value)?,
            SnapshotSource::Browser { bytes, .. } => bytes.as_ref().clone(),
        };
        let snapshot: Snapshot = serde_json::from_slice(&bytes)?;
        snapshot
            .workbook
            .validate()
            .map_err(Error::InvalidWorkbook)?;
        Ok(snapshot.workbook)
    }

    pub fn load(&self) -> Result<Workbook> {
        if self.requires_recheck {
            return Err(Error::BrowserRecovery(
                "This cached recovery copy needs to be rechecked before it can be restored.".into(),
            ));
        }
        let snapshot: Snapshot = match &self.source {
            SnapshotSource::Legacy { value } => {
                let LeaseKind::Legacy { lock, claim } = &self.lease.kind else {
                    return Err(Error::BrowserRecovery("Invalid recovery lease.".into()));
                };
                if !held(LEGACY_LEASE_ID, &claim.token).map_err(browser_error)? {
                    return Err(Error::BrowserRecovery(
                        "This legacy recovery session is no longer owned by this tab.".into(),
                    ));
                }
                let guard = lock.lock().map_err(|_| Error::Poisoned)?;
                guard.as_ref().ok_or(Error::Suspended)?.refresh_checked()?;
                if legacy_value(&self.path)?.as_str() != value.as_str() {
                    return Err(Error::BrowserRecovery(
                        "A previous recovery snapshot changed. Reload before trying again.".into(),
                    ));
                }
                let bytes = decode_legacy_value(value)?;
                if legacy_value(&self.path)?.as_str() != value.as_str() {
                    return Err(Error::BrowserRecovery(
                        "A previous recovery snapshot changed. Reload before trying again.".into(),
                    ));
                }
                serde_json::from_slice(&bytes)?
            }
            SnapshotSource::Browser { bytes, .. } => {
                if !held(&self.session_id, self.browser_lease()?).map_err(browser_error)? {
                    return Err(Error::BrowserRecovery(
                        "This recovery session is no longer owned by this tab.".into(),
                    ));
                }
                serde_json::from_slice(bytes.as_slice())?
            }
        };
        snapshot
            .workbook
            .validate()
            .map_err(Error::InvalidWorkbook)?;
        Ok(snapshot.workbook)
    }

    pub fn discard(&self) -> Result<()> {
        match &self.source {
            SnapshotSource::Legacy { .. } => Err(Error::BrowserRecovery(
                "Legacy snapshots are kept because older tabs cannot be excluded safely.".into(),
            )),
            SnapshotSource::Browser { .. } => Err(Error::RecoveryPending),
        }
    }

    pub async fn discard_async(&self) -> Result<()> {
        if self.requires_recheck {
            return Err(Error::BrowserRecovery(
                "This cached recovery copy needs to be rechecked; nothing was discarded.".into(),
            ));
        }
        match &self.source {
            SnapshotSource::Legacy { .. } => Err(Error::BrowserRecovery(
                "Legacy snapshots are kept because older tabs cannot be excluded safely.".into(),
            )),
            SnapshotSource::Browser { bytes, revision } => {
                wait(discard(
                    &self.session_id,
                    &self.doc_key,
                    *revision,
                    self.browser_lease()?,
                    bytes.as_slice(),
                ))
                .await?;
                Ok(())
            }
        }
    }

    /// Verify and complete a pending discard, or explicitly keep the original.
    /// Neither path treats a missing record as evidence of a committed discard.
    pub async fn resolve_browser_discard(&self, keep: bool) -> Result<RecoverySourceState> {
        match &self.source {
            SnapshotSource::Legacy { .. } => Err(Error::BrowserRecovery(
                "Legacy snapshots are kept because older tabs cannot be excluded safely.".into(),
            )),
            SnapshotSource::Browser { bytes, revision } => {
                let lease = self.browser_lease()?;
                let operation = if keep {
                    cancel_discard(&self.session_id, &self.doc_key, *revision, lease, bytes)
                } else {
                    complete_discard(&self.session_id, &self.doc_key, *revision, lease, bytes)
                };
                let result = wait(operation).await?.as_string();
                match result.as_deref() {
                    Some("present") => Ok(RecoverySourceState::Present),
                    Some("absent") => Ok(RecoverySourceState::Absent),
                    _ => Err(Error::RecoveryOutcomeUnknown(
                        "The recovery snapshot could not be verified.".into(),
                    )),
                }
            }
        }
    }
}

fn legacy_storage() -> Result<web_sys::Storage> {
    let window = web_sys::window()
        .ok_or_else(|| Error::BrowserRecovery("Browser storage is unavailable.".into()))?;
    window
        .local_storage()
        .map_err(browser_error)?
        .ok_or_else(|| Error::BrowserRecovery("Browser storage is unavailable.".into()))
}

fn legacy_storage_key(path: &Path) -> String {
    format!("{}{}", crate::vfs::KEY_PREFIX, crate::vfs::normalize(path))
}

fn legacy_value(path: &Path) -> Result<String> {
    read_legacy_value(&legacy_storage_key(path))
        .map_err(browser_error)?
        .ok_or_else(|| {
            Error::BrowserRecovery(
                "A previous recovery snapshot disappeared. Reload before trying again.".into(),
            )
        })
}

fn decode_legacy_value(raw: &str) -> Result<Vec<u8>> {
    #[derive(Deserialize)]
    struct Envelope {
        l: u64,
        d: String,
    }
    let envelope: Envelope = serde_json::from_str(raw).map_err(|_| {
        Error::BrowserRecovery(
            "A previous recovery entry is invalid; its bytes were retained.".into(),
        )
    })?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(envelope.d)
        .map_err(|_| {
            Error::BrowserRecovery(
                "A previous recovery entry is invalid; its bytes were retained.".into(),
            )
        })?;
    if bytes.len() as u64 != envelope.l {
        return Err(Error::BrowserRecovery(
            "A previous recovery entry has an invalid length; its bytes were retained.".into(),
        ));
    }
    Ok(bytes)
}

fn legacy_lock_record(path: &Path) -> Result<Option<LegacyLockRecord>> {
    let Some(raw) = read_legacy_value(&legacy_storage_key(path)).map_err(browser_error)? else {
        return Ok(None);
    };
    if raw.len() > MAX_METADATA_PREFIX_BYTES {
        return Err(Error::BrowserRecovery(
            "A previous recovery lock exceeds the safe preview limit; its data was retained."
                .into(),
        ));
    }
    let bytes = decode_legacy_value(&raw)?;
    serde_json::from_slice(&bytes).map(Some).map_err(|_| {
        Error::BrowserRecovery("A previous recovery lock is invalid; its data was retained.".into())
    })
}

/// Enumerate without the VFS's best-effort `keys()` behavior. A partial scan
/// must not be mistaken for an empty recovery directory.
fn checked_storage_keys(storage: &web_sys::Storage, prefix: &str) -> Result<Vec<String>> {
    let count = storage.length().map_err(browser_error)?;
    if count > MAX_LEGACY_STORAGE_KEYS {
        return Err(Error::BrowserRecovery(
            "Browser storage has too many entries for a safe recovery scan; originals were retained."
                .into(),
        ));
    }
    let mut keys = Vec::new();
    for index in 0..count {
        let key = storage.key(index).map_err(browser_error)?.ok_or_else(|| {
            Error::BrowserRecovery(
                "Previous browser recovery changed during discovery; retry.".into(),
            )
        })?;
        if key.starts_with(prefix) {
            if key.len() > MAX_LEGACY_KEY_BYTES {
                return Err(Error::BrowserRecovery(
                    "A previous recovery path exceeds the safe scan limit; its data was retained."
                        .into(),
                ));
            }
            keys.push(key);
        }
    }
    if storage.length().map_err(browser_error)? != count {
        return Err(Error::BrowserRecovery(
            "Previous browser recovery changed during discovery; retry.".into(),
        ));
    }
    keys.sort();
    Ok(keys)
}

fn legacy_documents(paths: &Paths) -> Result<BTreeMap<String, Vec<PathBuf>>> {
    let storage = legacy_storage()?;
    let root = paths.data_dir.join("recovery");
    let prefix = format!("{}{}/", vfs::KEY_PREFIX, vfs::normalize(&root));
    let keys = checked_storage_keys(&storage, &prefix)?;
    if checked_storage_keys(&storage, &prefix)? != keys {
        return Err(Error::BrowserRecovery(
            "Previous browser recovery changed during discovery; retry.".into(),
        ));
    }
    let mut sessions = BTreeMap::<String, Vec<PathBuf>>::new();
    let mut document_count = 0;
    for key in keys {
        let Some(rest) = key.strip_prefix(&prefix) else {
            continue;
        };
        let Some((id, file)) = rest.split_once("/docs/") else {
            // Directories are implicit in the legacy VFS. Only the two known
            // session files are safe to ignore; any other entry might be an
            // unreadable recovery source and remains available to rescue.
            if let Some((id, name)) = rest.split_once('/')
                && check_key(id).is_ok()
                && matches!(name, "lock" | "session.json")
            {
                continue;
            }
            return Err(Error::BrowserRecovery(
                "A previous recovery entry has an unexpected path; its data was retained for rescue."
                    .into(),
            ));
        };
        let Some(doc_key) = file.strip_suffix(".json") else {
            return Err(Error::BrowserRecovery(
                "A previous recovery path is invalid; its data was retained.".into(),
            ));
        };
        // VFS normalization treats backslashes and ".." as separators. Only
        // accept canonical generated keys, so a malformed legacy key can
        // never cause a different localStorage entry to be read or removed.
        if check_key(id).is_err() || check_key(doc_key).is_err() {
            return Err(Error::BrowserRecovery(
                "A previous recovery path is invalid; its data was retained.".into(),
            ));
        }
        document_count += 1;
        if document_count > MAX_LEGACY_DOCUMENTS
            || (!sessions.contains_key(id) && sessions.len() >= MAX_LEGACY_SESSIONS)
        {
            return Err(Error::BrowserRecovery(
                "Previous recovery discovery reached its safe preview limit; originals were retained."
                    .into(),
            ));
        }
        sessions
            .entry(id.to_owned())
            .or_default()
            .push(root.join(id).join("docs").join(file));
    }
    Ok(sessions)
}

/// Read only the leading metadata object, never parse the workbook during
/// startup. Native and browser writers serialize `meta` first. Unknown
/// ordering, oversized metadata, or malformed JSON is explicitly unverified.
fn snapshot_meta(bytes: &[u8], key: &str) -> (SnapshotMeta, bool) {
    fn preview(bytes: &[u8]) -> Option<SnapshotMeta> {
        fn skip_space(bytes: &[u8], position: &mut usize) {
            while bytes
                .get(*position)
                .is_some_and(|&byte| matches!(byte, b' ' | b'\n' | b'\r' | b'\t'))
            {
                *position += 1;
            }
        }

        let bytes = &bytes[..bytes.len().min(MAX_METADATA_PREFIX_BYTES)];
        let mut position = 0;
        skip_space(bytes, &mut position);
        if bytes.get(position) != Some(&b'{') {
            return None;
        }
        position += 1;
        skip_space(bytes, &mut position);
        let key_start = position;
        if bytes.get(position) != Some(&b'"') {
            return None;
        }
        position += 1;
        let mut escaped = false;
        while let Some(&byte) = bytes.get(position) {
            position += 1;
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                break;
            }
        }
        let name: String = serde_json::from_slice(bytes.get(key_start..position)?).ok()?;
        if name != "meta" {
            return None;
        }
        skip_space(bytes, &mut position);
        if bytes.get(position) != Some(&b':') {
            return None;
        }
        position += 1;
        skip_space(bytes, &mut position);
        let start = position;
        if bytes.get(start) != Some(&b'{') {
            return None;
        }
        let mut depth = 0usize;
        let mut quoted = false;
        let mut escaped = false;
        for (offset, &byte) in bytes[start..].iter().enumerate() {
            if quoted {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    quoted = false;
                }
            } else if byte == b'"' {
                quoted = true;
            } else if byte == b'{' {
                depth += 1;
            } else if byte == b'}' {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return serde_json::from_slice(&bytes[start..=start + offset]).ok();
                }
            }
        }
        None
    }

    match preview(bytes) {
        Some(meta) => (meta, true),
        None => (
            SnapshotMeta {
                title: format!("{key} (unverified)"),
                dirty: true,
                ..SnapshotMeta::default()
            },
            false,
        ),
    }
}

async fn legacy_orphans(
    paths: &Paths,
    claim: Arc<LegacyClaim>,
    budget: BrowserStartupBudget,
) -> (Vec<OrphanSession>, Option<String>) {
    let entries = match legacy_documents(paths) {
        Ok(entries) => entries,
        Err(error) => {
            return (
                Vec::new(),
                Some(format!(
                    "Previous localStorage recovery snapshots could not be inspected: {error}"
                )),
            );
        }
    };
    let mut sessions = Vec::new();
    let mut warning = String::new();
    let mut deferred = false;
    let mut total_bytes = 0usize;
    for (id, entries) in entries {
        if !startup_has_time(budget.deadline, budget.generation) {
            deferred = true;
            break;
        }
        if !held(LEGACY_LEASE_ID, &claim.token).unwrap_or(false) {
            return (
                Vec::new(),
                Some(
                    "Legacy recovery discovery lost its tab ownership; retry after returning."
                        .into(),
                ),
            );
        }
        let dir = paths.data_dir.join("recovery").join(&id);
        let lock = match LegacyHeartbeat::acquire(dir.join("lock")) {
            Ok(lock) => lock,
            Err(Error::Io(error)) if error.kind() == io::ErrorKind::WouldBlock => {
                add_warning(
                    &mut warning,
                    "Some previous recovery snapshots may still be in use. Close older Thinoda tabs and retry after their heartbeat expires.",
                );
                continue;
            }
            Err(_) => {
                add_warning(
                    &mut warning,
                    "Some previous localStorage recovery snapshots could not be checked safely. Restore storage access and retry.",
                );
                continue;
            }
        };
        let mut documents = Vec::new();
        for path in entries {
            if !startup_has_time(budget.deadline, budget.generation) {
                deferred = true;
                break;
            }
            let Some(key) = path
                .file_stem()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
            else {
                add_warning(
                    &mut warning,
                    "A previous recovery snapshot has an invalid name.",
                );
                continue;
            };
            let value = match legacy_value(&path) {
                Ok(value) => value,
                Err(_) => {
                    add_warning(
                        &mut warning,
                        "Some previous localStorage recovery snapshots could not be read.",
                    );
                    continue;
                }
            };
            if value.len() > MAX_LEGACY_ENVELOPE_BYTES
                || value.len() > MAX_LEGACY_TOTAL_BYTES.saturating_sub(total_bytes)
            {
                add_warning(
                    &mut warning,
                    "Some previous recovery snapshots exceed the safe preview limit; their originals were retained.",
                );
                continue;
            }
            total_bytes += value.len();
            let meta = match decode_legacy_value(&value) {
                Ok(bytes) => {
                    let (meta, verified) = snapshot_meta(&bytes, &key);
                    if !verified {
                        add_warning(
                            &mut warning,
                            "Some recovery details could not be verified within the preview limit. Originals were retained; download the raw data or retry.",
                        );
                    }
                    meta
                }
                Err(_) => {
                    add_warning(
                        &mut warning,
                        "Some previous localStorage recovery snapshots are invalid and were retained.",
                    );
                    SnapshotMeta {
                        title: format!("{key} (unverified)"),
                        dirty: true,
                        ..SnapshotMeta::default()
                    }
                }
            };
            documents.push((key, meta, path, value));
            if let Err(error) = wait(yield_browser()).await {
                return (
                    Vec::new(),
                    Some(format!("Legacy recovery discovery failed: {error}")),
                );
            }
            if !held(LEGACY_LEASE_ID, &claim.token).unwrap_or(false) {
                return (
                    Vec::new(),
                    Some(
                        "Legacy recovery discovery lost its tab ownership; retry after returning."
                            .into(),
                    ),
                );
            }
        }
        if documents.is_empty() {
            drop(lock);
            if deferred {
                break;
            }
            continue;
        }
        let lease = Arc::new(OrphanLease {
            kind: LeaseKind::Legacy {
                lock: Mutex::new(Some(lock)),
                claim: Arc::clone(&claim),
            },
        });
        let mut docs: Vec<_> = documents
            .into_iter()
            .map(|(doc_key, meta, path, value)| OrphanDoc {
                session_id: id.clone(),
                doc_key,
                meta,
                path,
                source: SnapshotSource::Legacy {
                    value: Arc::new(value),
                },
                lease: Arc::clone(&lease),
                requires_recheck: false,
                discard_pending: false,
            })
            .collect();
        docs.sort_by(|a, b| a.doc_key.cmp(&b.doc_key));
        sessions.push(OrphanSession {
            session_id: id,
            docs,
        });
        if deferred {
            break;
        }
    }
    sessions.sort_by(|a, b| a.session_id.cmp(&b.session_id));
    if deferred {
        add_warning(
            &mut warning,
            "Some previous recovery discovery was deferred. Resolve the shown items, then retry; remaining snapshots were not deleted.",
        );
    }
    (sessions, (!warning.is_empty()).then_some(warning))
}
