//! Browser recovery restores wait for IndexedDB confirmation and retain each
//! original until the user separately confirms its discard. Native recovery
//! remains synchronous.

use std::{
    sync::{Arc, mpsc},
    time::Duration,
};

use egui::Context;
use thinoda_platform::recovery::{
    BrowserStartupBudget, OrphanSession, RecoveryManager, RecoverySourceState,
};

use crate::{ThinodaApp, app::new_topic_titles, doc::OpenDoc, i18n::t};

pub(crate) enum BrowserRecoveryStartup {
    Unstarted,
    Pending {
        receiver: mpsc::Receiver<BrowserStartupEvent>,
        budget: BrowserStartupBudget,
    },
    AwaitingLaunch {
        orphans: Vec<OrphanSession>,
        warning: String,
        budget: BrowserStartupBudget,
    },
    Ready,
    Deferred(String),
    Disabled,
}

impl Default for BrowserRecoveryStartup {
    fn default() -> Self {
        Self::Unstarted
    }
}

pub(crate) enum BrowserStartupEvent {
    WriterReady {
        manager: Arc<RecoveryManager>,
        budget: BrowserStartupBudget,
    },
    Finished {
        orphans: Vec<OrphanSession>,
        warning: String,
        budget: BrowserStartupBudget,
    },
}

pub(crate) struct BrowserRecoveryJob {
    result: mpsc::Receiver<RecoveryResult>,
    /// If this task disappears before reporting its result, a discard may
    /// already have committed. Keep its source cached but require an ordered
    /// verification before allowing another destructive action.
    pending_discard: Option<(String, String)>,
}

struct RecoveryResult {
    restored: Vec<(OpenDoc, String)>,
    copies: Vec<(String, Vec<String>)>,
    keep: Vec<OrphanSession>,
    errors: Vec<String>,
    discarded: Vec<(String, String)>,
    uncertain: Vec<(String, String)>,
    verified: Vec<(String, String)>,
    notice: Option<String>,
}

enum ReviewAction {
    Restore(Option<usize>),
    Discard(usize, usize),
    Verify(usize, usize),
    KeepDiscard(usize, usize),
    Download(usize, usize),
    DownloadRaw(usize, usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BrowserDiscardAction {
    Request,
    Complete,
    Keep,
}

struct BrowserUiState {
    phase: &'static str,
    message: &'static str,
    detail: Option<String>,
    protection: Option<String>,
    can_retry: bool,
    can_review: bool,
}

impl ThinodaApp {
    /// Begin after the first frame. Editing remains gated until discovery has
    /// completed because a separate writer alone does not prove document
    /// identity is isolated from an unresolved snapshot.
    pub(crate) fn poll_browser_recovery_startup(&mut self, ctx: &Context) {
        if matches!(
            &self.browser_recovery_startup,
            BrowserRecoveryStartup::Unstarted
        ) {
            self.start_browser_recovery(ctx);
        }
        if self.launch.is_none()
            && matches!(
                &self.browser_recovery_startup,
                BrowserRecoveryStartup::AwaitingLaunch { .. }
            )
        {
            let finished = std::mem::replace(
                &mut self.browser_recovery_startup,
                BrowserRecoveryStartup::Disabled,
            );
            if let BrowserRecoveryStartup::AwaitingLaunch {
                orphans,
                warning,
                budget,
            } = finished
            {
                if RecoveryManager::browser_startup_is_current(budget) {
                    self.install_browser_recovery_results(orphans, warning);
                } else {
                    self.defer_stale_browser_recovery();
                }
                ctx.request_repaint();
            }
        }
        let event = match &self.browser_recovery_startup {
            BrowserRecoveryStartup::Pending { receiver, budget } => match receiver.try_recv() {
                Ok(event) => Some(event),
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(100));
                    None
                }
                Err(mpsc::TryRecvError::Disconnected) => Some(BrowserStartupEvent::Finished {
                    orphans: Vec::new(),
                    warning: "Browser recovery discovery stopped unexpectedly. Retry recovery."
                        .into(),
                    budget: *budget,
                }),
            },
            _ => None,
        };
        let Some(event) = event else {
            return;
        };
        let budget = match &event {
            BrowserStartupEvent::WriterReady { budget, .. }
            | BrowserStartupEvent::Finished { budget, .. } => *budget,
        };
        if !RecoveryManager::browser_startup_is_current(budget) {
            // Dropping the stale event releases only its in-memory claims;
            // source snapshots and committed writer records are not deleted.
            self.defer_stale_browser_recovery();
            ctx.request_repaint();
            return;
        }
        match event {
            BrowserStartupEvent::WriterReady { manager, .. } => {
                if self.services.recovery.is_none() {
                    self.services.recovery = Some(manager);
                    if let Some(paths) = self.services.paths.clone() {
                        thinoda_platform::crash::install_panic_hook(
                            paths,
                            self.services.recovery.clone(),
                        );
                    }
                    if let Some(manager) = &self.services.recovery {
                        // Include changes made while startup was pending
                        // without waiting for another edit.
                        for doc in &mut self.docs {
                            doc.snapshot(manager);
                        }
                    }
                }
                // Discovery is still pending even though this independent
                // writer can now preserve current edits.
                ctx.request_repaint();
            }
            BrowserStartupEvent::Finished {
                orphans,
                warning,
                budget,
            } => {
                // The existing one-time launch handoff assigns its own
                // `Startup.orphans`. Do not let it overwrite late results.
                if self.launch.is_some() {
                    self.browser_recovery_startup = BrowserRecoveryStartup::AwaitingLaunch {
                        orphans,
                        warning,
                        budget,
                    };
                } else {
                    self.install_browser_recovery_results(orphans, warning);
                }
                ctx.request_repaint();
            }
        }
    }

    fn defer_stale_browser_recovery(&mut self) {
        self.browser_recovery_startup = BrowserRecoveryStartup::Deferred(
            "Recovery discovery was interrupted when the page was hidden. Retry after returning; source snapshots were retained."
                .into(),
        );
    }

    fn install_browser_recovery_results(
        &mut self,
        orphans: Vec<OrphanSession>,
        mut warning: String,
    ) {
        if merge_browser_recovery(&mut self.dialogs.recovery, orphans) {
            append_warning(
                &mut warning,
                Some(t(
                    "Some cached recovery copies could not be rechecked. They remain available to download; their originals may have changed or disappeared.",
                )
                .to_owned()),
            );
        }
        // A discard request is durable before it can delete the source. After
        // a reload, recover its pending decision from the retained document.
        let pending: Vec<_> = self
            .dialogs
            .recovery
            .iter()
            .flat_map(|session| {
                session.docs.iter().filter_map(move |doc| {
                    (doc.discard_pending() && !doc.requires_recheck())
                        .then(|| (session.review_key(), doc.doc_key.clone()))
                })
            })
            .collect();
        self.browser_recovery_uncertain.extend(pending);
        self.browser_recovery_startup = if warning.is_empty() {
            BrowserRecoveryStartup::Ready
        } else {
            self.toasts.error(warning.clone());
            BrowserRecoveryStartup::Deferred(warning)
        };
    }

    fn start_browser_recovery(&mut self, ctx: &Context) {
        let Some(paths) = self.services.paths.clone() else {
            self.browser_recovery_startup = BrowserRecoveryStartup::Deferred(
                "Browser recovery paths are unavailable. Retry or download your work; existing snapshots were not changed."
                    .into(),
            );
            return;
        };
        let existing = self.services.recovery.clone();
        let (sender, receiver) = mpsc::channel();
        let budget = RecoveryManager::browser_startup_budget();
        self.browser_recovery_startup = BrowserRecoveryStartup::Pending { receiver, budget };
        let ctx = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            discover_browser_recovery(paths, existing, budget, sender, ctx).await;
        });
    }

    pub(crate) fn retry_browser_recovery(&mut self, ctx: &Context) {
        if self.browser_recovery_job.is_some() {
            return;
        }
        if let Some(manager) = &self.services.recovery
            && manager.current_error().is_some()
        {
            self.browser_recovery_notice = None;
            if let Err(error) = manager.retry_browser_writer() {
                self.browser_recovery_notice = Some(error.to_string());
            }
            ctx.request_repaint_after(Duration::from_millis(100));
            return;
        }
        if !matches!(
            &self.browser_recovery_startup,
            BrowserRecoveryStartup::Deferred(_)
        ) {
            return;
        }
        // Keep cached bytes and their claim while an unverified discard exists.
        // The user can verify or download them before any rescan replaces them.
        if !self.browser_recovery_uncertain.is_empty() {
            return;
        }
        // Keep the cached originals and the open review visible. Release only
        // their old claims; no cached copy may be restored or discarded until
        // a fresh scan finds an identical source under a new claim.
        for session in &mut self.dialogs.recovery {
            if let Err(error) = session.prepare_browser_retry() {
                self.browser_recovery_notice = Some(error.to_string());
                ctx.request_repaint();
                return;
            }
        }
        self.browser_recovery_notice = None;
        self.start_browser_recovery(ctx);
    }

    pub(crate) fn browser_recovery_pending(&self) -> bool {
        matches!(
            &self.browser_recovery_startup,
            BrowserRecoveryStartup::Unstarted
                | BrowserRecoveryStartup::Pending { .. }
                | BrowserRecoveryStartup::AwaitingLaunch { .. }
        )
    }

    pub(crate) fn browser_recovery_warning(&self) -> Option<&str> {
        match &self.browser_recovery_startup {
            BrowserRecoveryStartup::Deferred(warning) => Some(warning),
            _ => None,
        }
    }

    pub(crate) fn browser_recovery_can_retry(&self) -> bool {
        self.browser_recovery_job.is_none()
            && ((self.browser_recovery_warning().is_some()
                && self.browser_recovery_uncertain.is_empty())
                || self
                    .services
                    .recovery
                    .as_ref()
                    .is_some_and(|manager| manager.current_error().is_some()))
    }

    /// Do not assume the live document is distinct merely because its writer
    /// session is fenced. An incomplete/failed scan keeps editing gated.
    pub(crate) fn browser_recovery_can_edit(&self) -> bool {
        matches!(
            &self.browser_recovery_startup,
            BrowserRecoveryStartup::Ready
        ) && self.browser_recovery_uncertain.is_empty()
            && self
                .dialogs
                .recovery
                .iter()
                .flat_map(|session| &session.docs)
                .all(|doc| !doc.requires_recheck())
            && self
                .services
                .recovery
                .as_ref()
                .is_some_and(|manager| manager.current_error().is_none())
    }

    /// Keep the browser's unload warning active until queued saves/deletions
    /// have been confirmed, even if the corresponding document became clean.
    pub(crate) fn browser_recovery_needs_unload_warning(&self) -> bool {
        RecoveryManager::browser_has_unconfirmed_work()
            || self.browser_recovery_job.is_some()
            || !self.browser_recovery_uncertain.is_empty()
            || self
                .dialogs
                .recovery
                .iter()
                .flat_map(|session| &session.docs)
                .any(|doc| doc.requires_recheck())
            || self.docs.iter().any(|doc| {
                doc.browser_recovery_has_unqueued_changes(self.services.recovery.as_deref())
            })
            || self
                .services
                .recovery
                .as_ref()
                .is_some_and(|manager| manager.has_pending() || manager.current_error().is_some())
    }

    fn browser_recovery_copy_is_open(&self, session: &OrphanSession) -> bool {
        self.browser_recovery_copies
            .get(&session.review_key())
            .is_some_and(|keys| {
                keys.iter()
                    .any(|key| self.docs.iter().any(|doc| &doc.key == key))
            })
    }

    fn browser_recovery_source_uncertain(&self, session: &OrphanSession, key: &str) -> bool {
        self.browser_recovery_uncertain
            .contains(&(session.review_key(), key.to_owned()))
    }

    fn browser_recovery_can_rescue(&self) -> bool {
        !self.browser_recovery_can_edit() || !self.dialogs.recovery.is_empty()
    }

    fn browser_rescue_config(&self) -> serde_json::Value {
        let legacy_prefix = self
            .services
            .paths
            .as_ref()
            .map(RecoveryManager::browser_legacy_rescue_prefix)
            .unwrap_or_default();
        serde_json::json!({
            "legacyPrefix": legacy_prefix,
            "labels": {
                "title": t("Rescue original recovery data"),
                "description": t("Read-only rescue. Downloads do not resolve recovery or change browser storage."),
                "privacy": t("Raw recovery copies may contain private content."),
                "legacy": t("Legacy recovery entries"),
                "documents": t("Browser recovery documents"),
                "sessions": t("Browser recovery session records"),
                "refresh": t("Refresh list"),
                "next": t("Next entries"),
                "choose": t("Select a record to prepare a download."),
                "prepare": t("Prepare download"),
                "preparing": t("Preparing a rescue copy…"),
                "download": t("Download prepared rescue copy"),
                "raw": t("Download raw recovery data"),
                "empty": t("No entries on this page."),
                "ready": t("Rescue copy prepared. Use the download link."),
                "archive": t("This archive is a rescue copy, not a workbook; keep the original browser data."),
                "changed": t("Other tabs may change these entries. Refresh the list to check again."),
                "failure": t("Recovery data could not be read. The original was not changed."),
                "close": t("Close"),
            }
        })
    }

    fn open_browser_rescue(&mut self) {
        if let Err(error) =
            RecoveryManager::open_browser_rescue(&self.browser_rescue_config().to_string())
        {
            self.browser_recovery_notice = Some(error.to_string());
        }
    }

    fn browser_ui_state(&self) -> BrowserUiState {
        let manager = self.services.recovery.as_ref();
        let writer_error = manager.and_then(|manager| manager.current_error());
        let gap = self.docs.iter().find_map(|doc| doc.browser_recovery_gap());
        let unqueued = self
            .docs
            .iter()
            .any(|doc| doc.browser_recovery_has_unqueued_changes(manager.map(Arc::as_ref)));
        let pending = manager.is_some_and(|manager| manager.has_pending());
        let unacknowledged = manager.is_some_and(|manager| {
            self.docs
                .iter()
                .any(|doc| doc.is_dirty() && !doc.browser_recovery_acknowledged(manager))
        });
        let eviction_risk = manager.is_some_and(|manager| !manager.eviction_protected());
        let durability_risk = manager.is_some_and(|manager| !manager.strict_durability_requested());
        let protection = manager.map(|_| {
            let mut text = if writer_error.is_some() {
                t("Crash recovery is unavailable. Save or download your work.").to_owned()
            } else if let Some(detail) = gap {
                format!(
                    "{}: {detail}",
                    t("Some changes could not be included in crash recovery.")
                )
            } else if unqueued {
                t("Recent edits are not yet queued for crash recovery.").to_owned()
            } else if pending || unacknowledged {
                t("Saving crash recovery. Recent changes are not confirmed yet.").to_owned()
            } else if self.docs.iter().any(|doc| doc.is_dirty()) {
                t("Current unsaved changes have confirmed recovery snapshots.").to_owned()
            } else {
                t("No unsaved changes need crash recovery.").to_owned()
            };
            if eviction_risk {
                text.push(' ');
                text.push_str(t(
                    "Browser storage is not protected from eviction. Keep a downloaded backup.",
                ));
            }
            if durability_risk {
                text.push(' ');
                text.push_str(t(
                    "This browser cannot confirm strict recovery durability. Keep a downloaded backup.",
                ));
            }
            text
        });
        let can_retry = self.browser_recovery_can_retry();
        let can_review = !self.dialogs.recovery.is_empty() && self.browser_recovery_job.is_none();
        let (phase, message, detail) = if let Some(error) = writer_error {
            (
                "unavailable",
                "Crash recovery is unavailable. Save or download your work.",
                Some(error),
            )
        } else if manager.is_none() {
            if self.browser_recovery_pending() {
                ("waiting-writer", "Preparing a safe recovery session…", None)
            } else {
                (
                    "unavailable",
                    "Crash recovery is unavailable. Save or download your work.",
                    self.browser_recovery_warning().map(str::to_owned),
                )
            }
        } else if self.browser_recovery_job.is_some() {
            (
                "reconciling",
                "Finishing the recovery decision. Keep this page open.",
                None,
            )
        } else if !self.browser_recovery_uncertain.is_empty() {
            (
                "unverified-discard",
                "Discard is pending or its result is unknown. Verify and complete it, keep the original, or download the cached copy.",
                None,
            )
        } else if self.browser_recovery_pending() {
            (
                "discovery-pending",
                "Previous work is still being checked. Editing is paused.",
                None,
            )
        } else if let Some(warning) = self.browser_recovery_warning() {
            (
                "deferred",
                "Recovery needs attention. Review saved work or retry.",
                Some(warning.to_owned()),
            )
        } else if can_review
            && self
                .dialogs
                .recovery
                .iter()
                .all(|session| self.browser_recovery_copy_is_open(session))
        {
            (
                "originals-retained",
                "Original recovery snapshots are retained and use browser storage.",
                None,
            )
        } else if can_review {
            ("review", "Previous work is available to review.", None)
        } else if let Some(detail) = gap {
            (
                "snapshot-gap",
                "Some changes could not be included in crash recovery.",
                Some(detail.to_owned()),
            )
        } else if unqueued {
            (
                "unqueued",
                "Recent edits are not yet queued for crash recovery.",
                None,
            )
        } else if pending || unacknowledged {
            (
                "writing",
                "Saving crash recovery. Recent changes are not confirmed yet.",
                None,
            )
        } else if eviction_risk {
            (
                "eviction-risk",
                "Browser storage is not protected from eviction. Keep a downloaded backup.",
                None,
            )
        } else if durability_risk {
            (
                "durability-risk",
                "This browser cannot confirm strict recovery durability. Keep a downloaded backup.",
                None,
            )
        } else {
            ("ready", "Crash recovery is ready.", None)
        };
        BrowserUiState {
            phase,
            message,
            detail,
            protection,
            can_retry,
            can_review,
        }
    }

    /// Mirror the visible state to an actual DOM live region. Retry and review
    /// buttons there are available to keyboard and browser screen readers.
    pub(crate) fn poll_browser_recovery_accessibility(&mut self, ctx: &Context) {
        let state = self.browser_ui_state();
        let mut message = t(state.message).to_owned();
        if let Some(detail) = &state.detail {
            message.push(' ');
            message.push_str(detail);
        }
        if let Some(protection) = &state.protection {
            if protection != t(state.message) {
                message.push(' ');
                message.push_str(protection);
            }
        }
        if let Some(notice) = &self.browser_recovery_notice {
            message.push(' ');
            message.push_str(notice);
        }
        let model = self.browser_accessibility_model();
        RecoveryManager::publish_browser_status(
            state.phase,
            &message,
            t("Retry recovery"),
            t("Review recovery"),
            state.can_retry,
            state.can_review,
            &model,
        );
        let action = RecoveryManager::take_browser_accessibility_action();
        match action.as_str() {
            "retry" if state.can_retry => self.retry_browser_recovery(ctx),
            "review" if state.can_review => self.browser_recovery_prompt_open = true,
            _ => self.handle_browser_accessibility_action(&action, ctx),
        }
        if state.phase != "ready" || state.can_review {
            ctx.request_repaint_after(Duration::from_millis(500));
        } else {
            // An idle page must eventually recheck origin persistence and
            // unexpected storage closure; it may otherwise receive no frames.
            ctx.request_repaint_after(Duration::from_secs(30));
        }
    }

    fn browser_accessibility_model(&self) -> String {
        let sessions: Vec<_> = self
            .dialogs
            .recovery
            .iter()
            .map(|session| {
                let docs: Vec<_> = session
                    .docs
                    .iter()
                    .map(|doc| serde_json::json!({
                        "key": doc.doc_key,
                        "title": doc.meta.title,
                        "uncertain": self.browser_recovery_source_uncertain(session, &doc.doc_key),
                        "canDiscard": doc.can_discard(),
                        "requiresRecheck": doc.requires_recheck(),
                    }))
                    .collect();
                serde_json::json!({
                    "id": session.review_key(),
                    "docs": docs,
                    "copied": self.browser_recovery_copy_is_open(session),
                    "uncertain": session.docs.iter().any(|doc| self.browser_recovery_source_uncertain(session, &doc.doc_key)),
                    "requiresRecheck": session.docs.iter().any(|doc| doc.requires_recheck()),
                })
            })
            .collect();
        let documents: Vec<_> = if self.browser_recovery_can_edit() {
            Vec::new()
        } else {
            self.docs
                .iter()
                .map(|doc| {
                    let drafts = doc.has_unresolved_ink()
                        || doc.view.has_title_changes()
                        || doc.view.has_board_label_changes()
                        || doc.inspector_edit.is_some()
                        || doc.fim_edit.is_some();
                    serde_json::json!({
                        "key": doc.key,
                        "title": doc.title(),
                        "drafts": drafts,
                        "ink": doc.has_unresolved_ink(),
                        "canRetryInk": doc.view.has_released_ink_gesture()
                            && !doc.view.pending_ink_has_rejected_input(),
                    })
                })
                .collect()
        };
        serde_json::json!({
            "sessions": sessions,
            "documents": documents,
            "canRestore": self.browser_recovery_can_edit() && self.browser_recovery_job.is_none(),
            "canDecide": !self.browser_recovery_pending() && self.browser_recovery_job.is_none(),
            "canRescue": self.browser_recovery_can_rescue(),
            "rescue": self.browser_rescue_config(),
            "labels": {
                "restore": t("Restore"),
                "restoreAll": t("Restore All"),
                "original": t("Original recovery snapshot"),
                "copied": t("A recovered copy is already open; the original is retained."),
                "retained": t("Original recovery snapshots use browser storage until discarded."),
                "discard": t("Discard this original"),
                "confirmDiscard": t("Confirm discard"),
                "cancel": t("Cancel"),
                "discardWarning": t("This requests discard. The original remains until you verify and complete it."),
                "legacyRetained": t("Legacy snapshots are kept because older tabs cannot be excluded safely."),
                "recheck": t("Cached recovery copy needs rechecking. Download it or retry."),
                "uncertain": t("Discard is pending or its result is unknown. Verify and complete it, keep the original, or download the cached copy."),
                "verify": t("Verify and complete discard"),
                "keep": t("Keep original"),
                "cached": t("Download cached copy"),
                "raw": t("Download raw recovery data"),
                "download": t("Download a copy"),
                "draftDetails": t("Download unfinished draft details"),
                "committed": t("Download workbook without unfinished drafts"),
                "retryInk": t("Retry ink"),
                "discardInk": t("Discard ink"),
            }
        })
        .to_string()
    }

    fn handle_browser_accessibility_action(&mut self, action: &str, ctx: &Context) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(action) else {
            return;
        };
        let kind = value
            .get("action")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let key = value
            .get("key")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let session_id = value
            .get("session")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let session = self
            .dialogs
            .recovery
            .iter()
            .position(|session| session.review_key() == session_id);
        let doc = self.docs.iter().position(|doc| doc.key == key);
        let can_review = !self.dialogs.recovery.is_empty() && self.browser_recovery_job.is_none();
        let can_edit = self.browser_recovery_can_edit();
        let result = match kind {
            "restore-all" if can_review && can_edit => {
                self.restore_orphans(None);
                None
            }
            "restore" if can_review && can_edit && session.is_some() => {
                self.restore_orphans(session);
                None
            }
            "discard-doc"
                if can_review && !self.browser_recovery_pending() && session.is_some() =>
            {
                let session = session.expect("session checked above");
                if let Some(index) = self.dialogs.recovery[session]
                    .docs
                    .iter()
                    .position(|doc| doc.doc_key == key)
                {
                    self.discard_browser_doc(session, index);
                }
                None
            }
            "verify-discard"
                if can_review && !self.browser_recovery_pending() && session.is_some() =>
            {
                let session = session.expect("session checked above");
                if let Some(index) = self.dialogs.recovery[session]
                    .docs
                    .iter()
                    .position(|doc| doc.doc_key == key)
                {
                    self.verify_browser_discard(session, index);
                }
                None
            }
            "keep-discard"
                if can_review && !self.browser_recovery_pending() && session.is_some() =>
            {
                let session = session.expect("session checked above");
                if let Some(index) = self.dialogs.recovery[session]
                    .docs
                    .iter()
                    .position(|doc| doc.doc_key == key)
                {
                    self.keep_browser_discard(session, index);
                }
                None
            }
            "download-orphan" if can_review && session.is_some() => {
                let session = session.expect("session checked above");
                self.dialogs.recovery[session]
                    .docs
                    .iter()
                    .position(|doc| doc.doc_key == key)
                    .map(|index| self.browser_download_orphan(session, index))
            }
            "download-orphan-raw" if can_review && session.is_some() => {
                let session = session.expect("session checked above");
                self.dialogs.recovery[session]
                    .docs
                    .iter()
                    .position(|doc| doc.doc_key == key)
                    .map(|index| self.browser_download_orphan_raw(session, index))
            }
            "download-doc" if !can_edit => doc.map(|index| self.browser_download_open_doc(index)),
            "download-committed" if !can_edit => {
                doc.map(|index| self.browser_download_committed_doc(index))
            }
            "download-drafts" if !can_edit => {
                doc.map(|index| self.browser_download_draft_details(index))
            }
            "retry-ink" if !can_edit => doc.and_then(|index| {
                let doc = &mut self.docs[index];
                if !doc.view.has_released_ink_gesture() || doc.view.pending_ink_has_rejected_input()
                {
                    return None;
                }
                Some(doc.retry_pending_ink().map_err(|error| error.to_string()))
            }),
            "discard-ink" if !can_edit => doc.map(|index| {
                self.docs[index].discard_pending_ink();
                Ok(())
            }),
            _ => None,
        };
        if let Some(result) = result {
            let success = if kind.starts_with("download") {
                "Download requested. Check your browser for the file."
            } else {
                "Ink action completed."
            };
            self.report_browser_rescue_result(result, success);
            ctx.request_repaint();
        }
    }

    fn report_browser_rescue_result(&mut self, result: Result<(), String>, success: &'static str) {
        match result {
            Ok(()) => {
                let notice = t(success).to_owned();
                self.browser_recovery_notice = Some(notice.clone());
                self.toasts.info(notice);
            }
            Err(error) => {
                let notice = format!("{}: {error}", t("Could not save"));
                self.browser_recovery_notice = Some(notice.clone());
                self.toasts.error(notice);
            }
        }
    }

    /// Retain the old-version heartbeat while unresolved legacy work is shown.
    pub(crate) fn refresh_browser_orphans(&mut self, ctx: &Context) {
        if self.dialogs.recovery.is_empty() {
            return;
        }
        ctx.request_repaint_after(Duration::from_secs(30));
        if self.browser_recovery_last_refresh.elapsed() < Duration::from_secs(30) {
            return;
        }
        self.browser_recovery_last_refresh = web_time::Instant::now();
        if let Some(error) = self
            .dialogs
            .recovery
            .iter()
            .find_map(|session| session.refresh().err())
            && !self.browser_recovery_pending()
        {
            self.browser_recovery_startup = BrowserRecoveryStartup::Deferred(format!(
                "Recovery ownership could not be refreshed: {error}. Retry; source snapshots were retained."
            ));
        }
    }

    pub(crate) fn browser_recovery_status_bar(&mut self, ui: &mut egui::Ui) {
        let state = self.browser_ui_state();
        if state.phase == "ready" && self.browser_recovery_notice.is_none() {
            return;
        }
        egui::Panel::top("thinoda_browser_recovery_status").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong(t(state.message));
                if state.can_retry && ui.button(t("Retry recovery")).clicked() {
                    self.retry_browser_recovery(ui.ctx());
                }
                if state.can_review && ui.button(t("Review recovery")).clicked() {
                    self.browser_recovery_prompt_open = true;
                }
                if self.browser_recovery_can_rescue()
                    && ui.button(t("Rescue original recovery data")).clicked()
                {
                    self.open_browser_rescue();
                }
            });
            if let Some(detail) = &state.detail {
                ui.label(detail);
            }
            if let Some(protection) = &state.protection
                && protection != t(state.message)
            {
                ui.label(protection);
            }
            if let Some(notice) = self.browser_recovery_notice.clone() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(notice);
                    if ui.button(t("Dismiss")).clicked() {
                        self.browser_recovery_notice = None;
                    }
                });
            }
        });
    }

    /// Do not draw the ordinary app while discovery is incomplete or a writer
    /// is absent/unhealthy: shortcuts and floating windows can mutate data.
    /// This screen permits only retry, review and explicit rescue downloads.
    pub(crate) fn browser_recovery_blocked_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.apply_appearance(&ctx);
        if self.launch.is_some() && !self.launch_frame(ui) {
            egui::Area::new(egui::Id::new("thinoda_recovery_startup_status"))
                .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -16.0])
                .show(&ctx, |ui| {
                    ui.label(t("Preparing a safe recovery session…"));
                });
            return;
        }
        for doc in &mut self.docs {
            // A gesture that was active when storage failed still needs its
            // pointer release and the existing retry/copy/discard controls.
            doc.suspend_ink(&ctx);
        }
        self.browser_recovery_status_bar(ui);
        let mut download_doc = None;
        let mut download_committed = None;
        let mut download_drafts = None;
        let mut download_orphan = None;
        let mut download_orphan_raw = None;
        let mut resolve_ink = None;
        egui::CentralPanel::default().show(ui, |ui| {
            let heading = if self.browser_recovery_pending() {
                "Previous work is still being checked. Editing is paused."
            } else if self.browser_recovery_warning().is_some() {
                "Recovery needs attention. Review saved work or retry."
            } else {
                "Crash recovery is unavailable. Save or download your work."
            };
            ui.heading(t(heading));
            ui.label(t(
                "Editing is paused until a safe recovery session is available.",
            ));
            for (index, doc) in self.docs.iter().enumerate() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(doc.title());
                    if ui.button(t("Download a copy")).clicked() {
                        download_doc = Some(index);
                    }
                    if doc.has_unresolved_ink() && ui.button(t("Resolve pending ink")).clicked() {
                        resolve_ink = Some(index);
                    }
                });
                if doc.has_unresolved_ink()
                    || doc.view.has_title_changes()
                    || doc.view.has_board_label_changes()
                    || doc.inspector_edit.is_some()
                    || doc.fim_edit.is_some()
                {
                    ui.label(t("Unfinished drafts may need a separate download."));
                    ui.horizontal_wrapped(|ui| {
                        if ui.button(t("Download unfinished draft details")).clicked() {
                            download_drafts = Some(index);
                        }
                        if ui
                            .button(t("Download workbook without unfinished drafts"))
                            .clicked()
                        {
                            download_committed = Some(index);
                        }
                    });
                }
            }
            if !self.dialogs.recovery.is_empty() {
                ui.separator();
                ui.label(t("Original recovery snapshots were not deleted."));
                for (session_index, session) in self.dialogs.recovery.iter().enumerate() {
                    for (doc_index, doc) in session.docs.iter().enumerate() {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(&doc.meta.title);
                            let download_label = if doc.requires_recheck()
                                || self.browser_recovery_source_uncertain(session, &doc.doc_key)
                            {
                                "Download cached copy"
                            } else {
                                "Download a copy"
                            };
                            if ui.button(t(download_label)).clicked() {
                                download_orphan = Some((session_index, doc_index));
                            }
                            if ui.button(t("Download raw recovery data")).clicked() {
                                download_orphan_raw = Some((session_index, doc_index));
                            }
                        });
                    }
                }
            }
        });
        if let Some(index) = resolve_ink {
            self.active = index;
        }
        self.ink_recovery_ui(&ctx);
        self.browser_recovery_review(&ctx);
        let result = if let Some(index) = download_doc {
            Some(self.browser_download_open_doc(index))
        } else if let Some(index) = download_committed {
            Some(self.browser_download_committed_doc(index))
        } else if let Some(index) = download_drafts {
            Some(self.browser_download_draft_details(index))
        } else if let Some((session, doc)) = download_orphan {
            Some(self.browser_download_orphan(session, doc))
        } else if let Some((session, doc)) = download_orphan_raw {
            Some(self.browser_download_orphan_raw(session, doc))
        } else {
            None
        };
        if let Some(result) = result {
            self.report_browser_rescue_result(
                result,
                "Download requested. Check your browser for the file.",
            );
        }
        self.toasts.tick(0.5);
        self.toasts.show(&ctx);
    }

    fn browser_download_open_doc(&self, index: usize) -> Result<(), String> {
        let doc = self
            .docs
            .get(index)
            .ok_or_else(|| t("Could not open").to_owned())?;
        let workbook = doc.document_with_drafts()?;
        browser_download_workbook(&doc.title(), workbook.workbook())
    }

    fn browser_download_committed_doc(&self, index: usize) -> Result<(), String> {
        let doc = self
            .docs
            .get(index)
            .ok_or_else(|| t("Could not open").to_owned())?;
        browser_download_workbook(&doc.title(), doc.doc.workbook())
    }

    fn browser_download_draft_details(&self, index: usize) -> Result<(), String> {
        let doc = self
            .docs
            .get(index)
            .ok_or_else(|| t("Could not open").to_owned())?;
        let details = format!(
            "Thinoda unfinished draft details (not a restorable workbook)\n\
             Download the workbook separately. This file may contain private content.\n\n\
             Title draft: {:#?}\nBoard-label draft: {:#?}\nInspector draft: {:#?}\n\
             FIM draft: {:#?}\nInk settings: {:#?}\nAccepted ink samples: {:#?}\n\
             Ink feedback: {:#?}\n",
            doc.view.title_draft(),
            doc.view.board_label_draft(),
            doc.inspector_edit,
            doc.fim_edit,
            doc.view.pending_ink_settings(),
            doc.view.pending_ink_samples(),
            doc.ink_feedback,
        );
        let name = thinoda_platform::web::sanitize_file_name(
            &format!("{}-draft-details", doc.title()),
            "txt",
        )
        .unwrap_or_else(|| "draft-details.txt".into());
        thinoda_platform::web::download(&name, details.as_bytes())
    }

    fn browser_download_orphan(&self, session: usize, index: usize) -> Result<(), String> {
        let session = self
            .dialogs
            .recovery
            .get(session)
            .ok_or_else(|| t("Could not open").to_owned())?;
        let source = session
            .docs
            .get(index)
            .ok_or_else(|| t("Could not open").to_owned())?;
        let workbook = if source.requires_recheck()
            || self.browser_recovery_source_uncertain(session, &source.doc_key)
        {
            source.load_cached()
        } else {
            source.load()
        }
        .map_err(|error| error.to_string())?;
        browser_download_workbook(&source.meta.title, &workbook)
    }

    fn browser_download_orphan_raw(&self, session: usize, index: usize) -> Result<(), String> {
        let source = self
            .dialogs
            .recovery
            .get(session)
            .and_then(|session| session.docs.get(index))
            .ok_or_else(|| t("Could not open").to_owned())?;
        let name = thinoda_platform::web::sanitize_file_name(
            &format!("{}-recovery-data", source.meta.title),
            "json",
        )
        .unwrap_or_else(|| "recovery-data.json".into());
        thinoda_platform::web::download_opaque(&name, &source.cached_recovery_bytes())
    }

    /// No recovery window is opened merely because a late result arrived.
    pub(crate) fn browser_recovery_review(&mut self, ctx: &Context) {
        if !self.browser_recovery_prompt_open || self.dialogs.recovery.is_empty() {
            return;
        }
        let can_restore = self.browser_recovery_can_edit() && self.browser_recovery_job.is_none();
        let can_decide = !self.browser_recovery_pending() && self.browser_recovery_job.is_none();
        let touch_allowed = thinoda_ui::widgets::touch_activation_allowed(ctx);
        let labels = crate::i18n::dialog_labels();
        let mut open = self.browser_recovery_prompt_open;
        let mut action = None;
        let confirm_id = egui::Id::new("thinoda_browser_recovery_discard_confirmation");
        let mut confirm_discard = ctx.data(|data| data.get_temp::<(String, String)>(confirm_id));
        thinoda_ui::Dialog::new(
            egui::Id::new("thinoda_browser_recovery_review"),
            t("Recover Unsaved Work"),
        )
        .labels(&labels)
        .open(&mut open)
        .modal(false)
        .default_width(460.0)
        .default_height(500.0)
        .min_size(egui::vec2(340.0, 220.0))
        .show(ctx, |ui| {
            ui.label(t(
                "Thinoda was not closed properly. The following documents can be restored:",
            ));
            ui.label(t("Restored work opens as a separate unsaved copy."));
            ui.label(t(
                "Original recovery snapshots use browser storage until discarded.",
            ));
            for (index, session) in self.dialogs.recovery.iter().enumerate() {
                ui.group(|ui| {
                    for (doc_index, doc) in session.docs.iter().enumerate() {
                        ui.strong(format!(
                            "{}: {}",
                            t("Original recovery snapshot"),
                            doc.meta.title
                        ));
                        if let Some(path) = &doc.meta.original_path {
                            thinoda_ui::widgets::caption(ui, path.display().to_string());
                        }
                        if self.browser_recovery_source_uncertain(session, &doc.doc_key) {
                            ui.label(t(
                                "Discard is pending or its result is unknown. Verify and complete it, keep the original, or download the cached copy.",
                            ));
                            if ui
                                .add_enabled(can_decide, egui::Button::new(t("Verify and complete discard")))
                                .clicked()
                                && touch_allowed
                            {
                                action = Some(ReviewAction::Verify(index, doc_index));
                            }
                            if ui
                                .add_enabled(can_decide, egui::Button::new(t("Keep original")))
                                .clicked()
                                && touch_allowed
                            {
                                action = Some(ReviewAction::KeepDiscard(index, doc_index));
                            }
                            if ui.button(t("Download cached copy")).clicked() && touch_allowed {
                                action = Some(ReviewAction::Download(index, doc_index));
                            }
                        } else if doc.requires_recheck() {
                            ui.label(t("Cached recovery copy needs rechecking. Download it or retry."));
                        } else if !doc.can_discard() {
                            ui.label(t(
                                "Legacy snapshots are kept because older tabs cannot be excluded safely.",
                            ));
                        } else if ui
                            .add_enabled(can_decide, egui::Button::new(t("Discard this original")))
                            .clicked()
                            && touch_allowed
                        {
                            confirm_discard = Some((session.review_key(), doc.doc_key.clone()));
                        }
                        if ui.button(t("Download raw recovery data")).clicked() && touch_allowed {
                            action = Some(ReviewAction::DownloadRaw(index, doc_index));
                        }
                    }
                    let copied = self.browser_recovery_copy_is_open(session);
                    let uncertain = session
                        .docs
                        .iter()
                        .any(|doc| self.browser_recovery_source_uncertain(session, &doc.doc_key));
                    let needs_recheck = session.docs.iter().any(|doc| doc.requires_recheck());
                    if copied {
                        ui.label(t(
                            "A recovered copy is already open; the original is retained.",
                        ));
                    }
                    if ui
                        .add_enabled(
                            can_restore && !copied && !uncertain && !needs_recheck,
                            egui::Button::new(t("Restore this session")),
                        )
                        .clicked()
                        && touch_allowed
                    {
                        action = Some(ReviewAction::Restore(Some(index)));
                    }
                });
            }
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        can_restore
                            && self.dialogs.recovery.iter().any(|session| {
                                !self.browser_recovery_copy_is_open(session)
                                    && !session.docs.iter().any(|doc| {
                                        self.browser_recovery_source_uncertain(
                                            session,
                                            &doc.doc_key,
                                        ) || doc.requires_recheck()
                                    })
                            }),
                        egui::Button::new(t("Restore All")),
                    )
                    .clicked()
                    && touch_allowed
                {
                    action = Some(ReviewAction::Restore(None));
                }
            });
            if let Some((session_key, doc_key)) = confirm_discard.clone() {
                ui.label(t(
                    "This requests discard. The original remains until you verify and complete it.",
                ));
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(can_decide, egui::Button::new(t("Confirm discard")))
                        .clicked()
                        && touch_allowed
                    {
                        if let Some((session_index, session)) = self
                            .dialogs
                            .recovery
                            .iter()
                            .enumerate()
                            .find(|(_, session)| session.review_key() == session_key)
                            && let Some(doc_index) =
                                session.docs.iter().position(|doc| doc.doc_key == doc_key)
                        {
                            action = Some(ReviewAction::Discard(session_index, doc_index));
                        }
                        confirm_discard = None;
                    }
                    if ui.button(t("Cancel")).clicked() && touch_allowed {
                        confirm_discard = None;
                    }
                });
            }
        });
        self.browser_recovery_prompt_open = open;
        if !open || action.is_some() {
            confirm_discard = None;
        }
        ctx.data_mut(|data| {
            if let Some(target) = confirm_discard {
                data.insert_temp(confirm_id, target);
            } else {
                data.remove::<(String, String)>(confirm_id);
            }
        });
        match action {
            Some(ReviewAction::Restore(index)) => self.restore_orphans(index),
            Some(ReviewAction::Discard(session, doc)) => self.discard_browser_doc(session, doc),
            Some(ReviewAction::Verify(session, doc)) => self.verify_browser_discard(session, doc),
            Some(ReviewAction::KeepDiscard(session, doc)) => {
                self.keep_browser_discard(session, doc)
            }
            Some(ReviewAction::Download(session, doc)) => {
                let result = self.browser_download_orphan(session, doc);
                self.report_browser_rescue_result(
                    result,
                    "Download requested. Check your browser for the file.",
                );
            }
            Some(ReviewAction::DownloadRaw(session, doc)) => {
                let result = self.browser_download_orphan_raw(session, doc);
                self.report_browser_rescue_result(
                    result,
                    "Download requested. Check your browser for the file.",
                );
            }
            None => {}
        }
    }

    pub fn restore_orphans(&mut self, index: Option<usize>) {
        if !self.browser_recovery_can_edit() || self.browser_recovery_job.is_some() {
            return;
        }
        let mut selected = Vec::new();
        for (i, session) in std::mem::take(&mut self.dialogs.recovery)
            .into_iter()
            .enumerate()
        {
            if index.is_none_or(|selected| selected == i)
                && !self.browser_recovery_copy_is_open(&session)
                && !session.docs.iter().any(|doc| {
                    doc.requires_recheck()
                        || self.browser_recovery_source_uncertain(&session, &doc.doc_key)
                })
            {
                selected.push(session);
            } else {
                self.dialogs.recovery.push(session);
            }
        }
        if selected.is_empty() {
            return;
        }
        self.browser_recovery_prompt_open = false;
        let manager = self.services.recovery.clone();
        let (sender, receiver) = mpsc::channel();
        self.browser_recovery_job = Some(BrowserRecoveryJob {
            result: receiver,
            pending_discard: None,
        });
        wasm_bindgen_futures::spawn_local(async move {
            let result = restore(selected, manager).await;
            let _ = sender.send(result);
        });
    }

    fn discard_browser_doc(&mut self, session: usize, doc: usize) {
        self.start_browser_discard_action(session, doc, BrowserDiscardAction::Request);
    }

    fn verify_browser_discard(&mut self, session: usize, doc: usize) {
        self.start_browser_discard_action(session, doc, BrowserDiscardAction::Complete);
    }

    fn keep_browser_discard(&mut self, session: usize, doc: usize) {
        self.start_browser_discard_action(session, doc, BrowserDiscardAction::Keep);
    }

    fn start_browser_discard_action(
        &mut self,
        session: usize,
        doc: usize,
        action: BrowserDiscardAction,
    ) {
        if self.browser_recovery_job.is_some() || self.browser_recovery_pending() {
            return;
        }
        let Some(session) = self.dialogs.recovery.get(session) else {
            return;
        };
        let Some(source) = session.docs.get(doc) else {
            return;
        };
        if source.requires_recheck() {
            self.browser_recovery_notice =
                Some(t("Cached recovery copy needs rechecking. Download it or retry.").into());
            return;
        }
        if action == BrowserDiscardAction::Request && !source.can_discard() {
            self.browser_recovery_notice = Some(
                t("Legacy snapshots are kept because older tabs cannot be excluded safely.").into(),
            );
            return;
        }
        let identity = (session.review_key(), source.doc_key.clone());
        if self.browser_recovery_uncertain.contains(&identity)
            != (action != BrowserDiscardAction::Request)
        {
            return;
        }
        let source = source.clone();
        self.browser_recovery_prompt_open = false;
        let (sender, receiver) = mpsc::channel();
        self.browser_recovery_job = Some(BrowserRecoveryJob {
            result: receiver,
            pending_discard: Some(identity.clone()),
        });
        wasm_bindgen_futures::spawn_local(async move {
            let mut result = RecoveryResult {
                restored: Vec::new(),
                copies: Vec::new(),
                keep: Vec::new(),
                errors: Vec::new(),
                discarded: Vec::new(),
                uncertain: Vec::new(),
                verified: Vec::new(),
                notice: None,
            };
            if action != BrowserDiscardAction::Request {
                match source
                    .resolve_browser_discard(action == BrowserDiscardAction::Keep)
                    .await
                {
                    Ok(RecoverySourceState::Present) => {
                        result.verified.push(identity);
                        result.notice =
                            Some(t("The original is present. You can retry discard.").into());
                    }
                    Ok(RecoverySourceState::Absent) => {
                        result.discarded.push(identity);
                        result.notice =
                            Some(t("The original is no longer present in browser storage.").into());
                    }
                    Err(error) => result.errors.push(error.to_string()),
                }
            } else {
                match source.discard_async().await {
                    Ok(()) => {
                        result.uncertain.push(identity);
                        result.notice = Some(
                            t("Discard is pending. The original is retained until you verify.")
                                .into(),
                        );
                    }
                    Err(thinoda_platform::Error::RecoveryOutcomeUnknown(error)) => {
                        result.uncertain.push(identity);
                        result.errors.push(error);
                    }
                    Err(error) => result.errors.push(error.to_string()),
                }
            }
            let _ = sender.send(result);
        });
    }

    pub(crate) fn poll_browser_recovery(&mut self, ctx: &Context) {
        let Some(job) = &self.browser_recovery_job else {
            return;
        };
        let result = match job.result.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(100));
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                if let Some(identity) = job.pending_discard.clone() {
                    self.browser_recovery_uncertain.insert(identity);
                }
                self.browser_recovery_job = None;
                self.browser_recovery_startup = BrowserRecoveryStartup::Deferred(
                    "Recovery handoff was interrupted. Retry or reload; retained snapshots may still be available."
                        .into(),
                );
                self.toasts.error(format!(
                    "{}: {}",
                    t("Recover Unsaved Work"),
                    t("Could not open")
                ));
                return;
            }
        };
        self.browser_recovery_job = None;
        let opened_copies = !result.restored.is_empty();
        self.browser_recovery_copies.extend(result.copies);
        for (session_key, doc_key) in result.verified {
            self.browser_recovery_uncertain
                .remove(&(session_key.clone(), doc_key.clone()));
            if let Some(doc) = self
                .dialogs
                .recovery
                .iter_mut()
                .find(|session| session.review_key() == session_key)
                .and_then(|session| session.docs.iter_mut().find(|doc| doc.doc_key == doc_key))
            {
                doc.clear_discard_pending();
            }
        }
        for (session_key, doc_key) in result.discarded {
            self.browser_recovery_uncertain
                .remove(&(session_key.clone(), doc_key.clone()));
            if let Some(session) = self
                .dialogs
                .recovery
                .iter_mut()
                .find(|session| session.review_key() == session_key)
            {
                session.docs.retain(|doc| doc.doc_key != doc_key);
            }
        }
        self.dialogs
            .recovery
            .retain(|session| !session.docs.is_empty());
        if !result.uncertain.is_empty() {
            self.browser_recovery_uncertain.extend(result.uncertain);
        }
        if !result.errors.is_empty() {
            self.browser_recovery_notice = Some(format!(
                "{}: {}",
                t("Recover Unsaved Work"),
                result.errors.join(" ")
            ));
        } else if let Some(notice) = result.notice {
            self.browser_recovery_notice = Some(notice);
        } else if opened_copies {
            self.browser_recovery_notice = Some(t(
                "Recovered copies are open. Original snapshots were kept; discard them only after checking the copies.",
            ).to_owned());
        } else {
            self.browser_recovery_notice = Some(t("Recovery decision completed.").to_owned());
        }
        self.dialogs.recovery.extend(result.keep);
        let keep_active_document = self.active < self.docs.len();
        for (doc, title) in result.restored {
            self.docs.push(doc);
            if !keep_active_document {
                self.active = self.docs.len() - 1;
            }
            self.toasts.success(format!("{}: {title}", t("Restored")));
        }
        for error in result.errors {
            self.toasts
                .error(format!("{}: {error}", t("Recover Unsaved Work")));
        }
    }
}

/// Reacquire matching originals without replacing cached bytes that the user
/// has already been asked to review. A changed or missing source stays visible
/// as a cached copy and keeps editing gated; the read-only rescue panel can
/// separately export the current browser record.
fn merge_browser_recovery(existing: &mut Vec<OrphanSession>, incoming: Vec<OrphanSession>) -> bool {
    let mut changed = false;
    for mut found in incoming {
        let key = found.review_key();
        if let Some(previous) = existing
            .iter_mut()
            .find(|session| session.review_key() == key)
        {
            for source in found.docs.drain(..) {
                if let Some(cached) = previous
                    .docs
                    .iter_mut()
                    .find(|cached| cached.doc_key == source.doc_key)
                {
                    if cached.same_recovery_source(&source) {
                        *cached = source;
                    } else {
                        changed = true;
                    }
                } else {
                    previous.docs.push(source);
                }
            }
            previous.docs.sort_by(|a, b| a.doc_key.cmp(&b.doc_key));
        } else {
            existing.push(found);
        }
    }
    changed
        || existing
            .iter()
            .flat_map(|session| &session.docs)
            .any(|doc| doc.requires_recheck())
}

fn browser_download_workbook(title: &str, workbook: &thinoda_core::Workbook) -> Result<(), String> {
    let bytes =
        thinoda_io::export_bytes(workbook, thinoda_io::Format::Thinoda, &Default::default())
            .map_err(|error| error.to_string())?;
    let name = thinoda_platform::web::sanitize_file_name(title, "thinoda")
        .unwrap_or_else(|| "recovered.thinoda".into());
    // A requested download is not an acknowledged disk save; do not mark the
    // document clean or remove any recovery snapshot.
    thinoda_platform::web::download(&name, &bytes)
}

async fn discover_browser_recovery(
    paths: thinoda_platform::Paths,
    existing: Option<Arc<RecoveryManager>>,
    budget: BrowserStartupBudget,
    sender: mpsc::Sender<BrowserStartupEvent>,
    ctx: Context,
) {
    let mut warning = String::new();
    let mut orphans = Vec::new();
    if let Err(error) = RecoveryManager::after_first_browser_paint().await {
        let _ = sender.send(BrowserStartupEvent::Finished {
            orphans,
            warning: format!("Browser recovery discovery could not start: {error}"),
            budget,
        });
        ctx.request_repaint();
        return;
    }
    let mut writer_started = false;
    if existing.is_none() {
        match RecoveryManager::start_browser_writer(&paths, budget).await {
            Ok((manager, writer_warning)) => {
                writer_started = true;
                if sender
                    .send(BrowserStartupEvent::WriterReady {
                        manager: Arc::new(manager),
                        budget,
                    })
                    .is_err()
                {
                    return;
                }
                ctx.request_repaint();
                append_warning(&mut warning, writer_warning);
            }
            Err(error) => append_warning(
                &mut warning,
                Some(format!("Browser crash recovery is unavailable: {error}")),
            ),
        }
    }
    let discovery = if writer_started {
        RecoveryManager::discover_after_browser_writer(&paths, budget).await
    } else {
        RecoveryManager::retry_browser_discovery(&paths, budget).await
    };
    match discovery {
        Ok((found, discovery_warning)) => {
            orphans.extend(found);
            append_warning(&mut warning, discovery_warning);
        }
        Err(error) => append_warning(
            &mut warning,
            Some(format!("Browser recovery discovery was deferred: {error}")),
        ),
    }
    let (legacy, legacy_warning) =
        RecoveryManager::find_legacy_orphans_with_warning(&paths, budget).await;
    orphans.extend(legacy);
    append_warning(&mut warning, legacy_warning);
    let _ = sender.send(BrowserStartupEvent::Finished {
        orphans,
        warning,
        budget,
    });
    ctx.request_repaint();
}

fn append_warning(target: &mut String, warning: Option<String>) {
    if let Some(warning) = warning.filter(|warning| !warning.is_empty()) {
        if !target.is_empty() {
            target.push(' ');
        }
        target.push_str(&warning);
    }
}

async fn restore(
    sessions: Vec<OrphanSession>,
    manager: Option<Arc<RecoveryManager>>,
) -> RecoveryResult {
    let mut result = RecoveryResult {
        restored: Vec::new(),
        copies: Vec::new(),
        keep: Vec::new(),
        errors: Vec::new(),
        discarded: Vec::new(),
        uncertain: Vec::new(),
        verified: Vec::new(),
        notice: None,
    };
    for session in sessions {
        let Some(manager) = &manager else {
            result.errors.push(format!(
                "{} {}",
                t("Crash recovery is unavailable. Save or download your work."),
                t("Original recovery snapshots were not deleted.")
            ));
            result.keep.push(session);
            continue;
        };
        if let Some(error) = manager.current_error() {
            result.errors.push(format!(
                "{error} {}",
                t("Original recovery snapshots were not deleted.")
            ));
            result.keep.push(session);
            continue;
        }
        let workbooks = session
            .docs
            .iter()
            .map(|doc| {
                doc.load()
                    .map_err(|error| format!("{} {}: {error}", t("Could not open"), doc.meta.title))
            })
            .collect::<Result<Vec<_>, _>>();
        let workbooks = match workbooks {
            Ok(workbooks) => workbooks,
            Err(error) => {
                result.errors.push(error);
                result.keep.push(session);
                continue;
            }
        };
        let mut restored = Vec::new();
        for (source, workbook) in session.docs.iter().zip(workbooks) {
            let mut doc = OpenDoc::new(workbook);
            doc.view.titles = new_topic_titles();
            // Browser recovery is always a separate unsaved copy. An old
            // snapshot may name a file the user has since opened or edited;
            // never bind this tab to that file or save over it implicitly.
            doc.path = None;
            doc.import_source = source.meta.import_source.clone();
            doc.doc.mark_dirty();
            restored.push(doc);
        }

        let mut committed = Vec::new();
        let mut failure = None;
        for (doc, source) in restored.iter().zip(&session.docs) {
            if let Err(error) = session.refresh() {
                failure = Some(error.to_string());
                break;
            }
            let mut meta = source.meta.clone();
            meta.original_path = doc.path.clone();
            meta.dirty = true;
            match manager
                .persist_snapshot_async(&doc.key, meta, doc.doc.workbook())
                .await
            {
                Ok(()) => committed.push(doc.key.clone()),
                Err(error) => {
                    failure = Some(error.to_string());
                    break;
                }
            }
        }
        if let Some(error) = failure {
            // Roll back only copies created by this attempted handoff.
            // Failure to remove a copy can produce an extra orphan, never
            // remove the still-claimed original.
            for key in committed {
                match manager.remove(&key) {
                    Ok(()) | Err(thinoda_platform::Error::RecoveryPending) => {}
                    Err(rollback) => {
                        result
                            .errors
                            .push(format!("Recovery rollback failed: {rollback}"));
                    }
                }
            }
            // Never open an editable copy until its fenced replacement has
            // committed. Keep the sources available for an explicit retry.
            result.errors.push(format!(
                "{error} {}",
                t("Original recovery snapshots were not deleted.")
            ));
            result.keep.push(session);
            continue;
        }
        let keys = restored.iter().map(|doc| doc.key.clone()).collect();
        result.copies.push((session.review_key(), keys));
        for (doc, source) in restored.into_iter().zip(&session.docs) {
            result.restored.push((doc, source.meta.title.clone()));
        }
        // Retain every original even after a successful copy. IndexedDB and
        // legacy localStorage cannot share one atomic transaction, and the UI
        // may close before it observes this result. A separate, confirmed user
        // action can discard sources after the copies have been checked.
        result.keep.push(session);
    }
    result
}
