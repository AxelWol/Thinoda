//! Explicit external-change resolution. Background checks never read placeholders.

use crate::{ThinodaApp, doc::LoadedWorkbook, i18n::t};
#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;
use std::{path::PathBuf, sync::mpsc};
use thinoda_core::Workbook;
use thinoda_platform::storage::{CancelToken, Revision, StorageError};

#[derive(Debug, Default, PartialEq, Eq)]
struct Difference {
    sheets_added: usize,
    sheets_removed: usize,
    sheets_changed: usize,
    topics_added: usize,
    topics_removed: usize,
    topics_changed: usize,
}

fn difference(local: &Workbook, remote: &Workbook) -> Difference {
    let mut result = Difference {
        sheets_added: remote.sheets.len().saturating_sub(local.sheets.len()),
        sheets_removed: local.sheets.len().saturating_sub(remote.sheets.len()),
        ..Default::default()
    };
    // Sheets have no persistent sheet ID; compare their positions, not ambiguous titles.
    for (before, after) in local.sheets.iter().zip(&remote.sheets) {
        result.sheets_changed += usize::from(before != after);
        result.topics_added += after
            .topics
            .keys()
            .filter(|id| !before.topics.contains_key(id))
            .count();
        result.topics_removed += before
            .topics
            .keys()
            .filter(|id| !after.topics.contains_key(id))
            .count();
        result.topics_changed += before
            .topics
            .iter()
            .filter(|(id, topic)| after.topics.get(id).is_some_and(|other| *topic != other))
            .count();
    }
    result.topics_added += remote
        .sheets
        .iter()
        .skip(local.sheets.len())
        .map(|s| s.topics.len())
        .sum::<usize>();
    result.topics_removed += local
        .sheets
        .iter()
        .skip(remote.sheets.len())
        .map(|s| s.topics.len())
        .sum::<usize>();
    result
}

enum Intent {
    Compare(Workbook),
    Reload { state: u64 },
}

struct Loading {
    key: String,
    path: PathBuf,
    intent: Intent,
    cancel: CancelToken,
    receiver: mpsc::Receiver<Result<LoadedWorkbook, StorageError>>,
}

struct RevisionRefresh {
    key: String,
    path: PathBuf,
    receiver: mpsc::Receiver<Result<Option<Revision>, StorageError>>,
}

#[cfg(not(target_arch = "wasm32"))]
struct Observation {
    key: String,
    baseline: Option<Revision>,
    revision: Option<Revision>,
    copies: Vec<PathBuf>,
    error: Option<String>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
struct Settling {
    revision: Option<Revision>,
    since: Option<web_time::Instant>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Settling {
    fn observe(&mut self, revision: Option<Revision>, now: web_time::Instant) -> bool {
        if self.since.is_none() || self.revision != revision {
            self.revision = revision;
            self.since = Some(now);
            false
        } else {
            self.since
                .is_some_and(|since| now.duration_since(since).as_millis() >= 500)
        }
    }
}

#[derive(Default)]
pub(crate) struct CloudChanges {
    loading: Option<Loading>,
    revision_refreshes: Vec<RevisionRefresh>,
    comparison: Option<Difference>,
    external: Option<String>,
    copies: Option<(String, Vec<PathBuf>)>,
    #[cfg(not(target_arch = "wasm32"))]
    watcher: Option<mpsc::Receiver<Vec<Observation>>>,
    #[cfg(not(target_arch = "wasm32"))]
    last_watch: Option<web_time::Instant>,
    #[cfg(not(target_arch = "wasm32"))]
    observed: HashMap<String, Option<Revision>>,
    #[cfg(not(target_arch = "wasm32"))]
    seen_copies: HashMap<String, Vec<PathBuf>>,
    #[cfg(not(target_arch = "wasm32"))]
    settling: HashMap<String, Settling>,
}

impl CloudChanges {
    pub(crate) fn comparing(&self) -> bool {
        self.loading.is_some() || self.comparison.is_some()
    }

    pub(crate) fn modal_open(&self) -> bool {
        self.comparing() || self.external.is_some() || self.copies.is_some()
    }
}

impl ThinodaApp {
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn refresh_recovered_revision(&mut self, key: &str, path: &std::path::Path) {
        let (tx, receiver) = mpsc::channel();
        let id = crate::files::document_id(path);
        wasm_bindgen_futures::spawn_local(async move {
            let result =
                thinoda_platform::browser_storage::document_revision(&id, CancelToken::default())
                    .await;
            let _ = tx.send(result);
        });
        self.cloud_changes.revision_refreshes.push(RevisionRefresh {
            key: key.into(),
            path: path.into(),
            receiver,
        });
    }

    pub(crate) fn compare_file(&mut self, key: &str, path: &std::path::Path) {
        let Some(doc) = self.docs.iter().find(|doc| doc.key == key) else {
            return;
        };
        let workbook = match doc.document_with_drafts() {
            Ok(document) => document.workbook().clone(),
            Err(error) => {
                self.toasts.error(error);
                return;
            }
        };
        self.start_change_load(key, path.to_path_buf(), Intent::Compare(workbook));
    }

    fn start_change_load(&mut self, key: &str, path: PathBuf, intent: Intent) {
        if self.cloud_changes.loading.is_some() {
            return;
        }
        let cancel = CancelToken::default();
        let (tx, receiver) = mpsc::channel();
        let source = path.clone();
        let token = cancel.clone();
        #[cfg(not(target_arch = "wasm32"))]
        {
            let spawned = std::thread::Builder::new()
                .name("thinoda-compare".into())
                .spawn(move || {
                    let _ = tx.send(crate::doc::run_storage(crate::doc::load_workbook_async(
                        &source, token,
                    )));
                });
            if let Err(error) = spawned {
                self.toasts
                    .error(format!("{}: {error}", t("Could not open")));
                return;
            }
        }
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            let _ = tx.send(crate::doc::load_workbook_async(&source, token).await);
        });
        self.cloud_changes.loading = Some(Loading {
            key: key.into(),
            path,
            intent,
            cancel,
            receiver,
        });
    }

    pub(crate) fn poll_cloud_changes(&mut self, ctx: &egui::Context) {
        for refresh in std::mem::take(&mut self.cloud_changes.revision_refreshes) {
            let result = match refresh.receiver.try_recv() {
                Ok(result) => result,
                Err(mpsc::TryRecvError::Empty) => {
                    self.cloud_changes.revision_refreshes.push(refresh);
                    ctx.request_repaint();
                    continue;
                }
                Err(mpsc::TryRecvError::Disconnected) => Err(StorageError::Remote(
                    t("The file loader stopped unexpectedly").into(),
                )),
            };
            let Some(doc) = self.docs.iter_mut().find(|doc| {
                doc.key == refresh.key
                    && doc.path.as_ref() == Some(&refresh.path)
                    && doc.storage_revision.is_none()
            }) else {
                continue;
            };
            match result {
                Ok(Some(revision)) => doc.storage_revision = Some(revision),
                Ok(None) => {
                    doc.externally_changed = true;
                    self.toasts.warning(t("File changed on disk"));
                }
                Err(error) => {
                    doc.externally_changed = true;
                    self.toasts
                        .error(format!("{}: {error}", t("File changed on disk")));
                }
            }
        }
        if let Some(loading) = &self.cloud_changes.loading {
            let result = match loading.receiver.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(Err(StorageError::Remote(
                    t("The file loader stopped unexpectedly").into(),
                ))),
            };
            if let Some(result) = result {
                let loading = self.cloud_changes.loading.take().expect("loading checked");
                match result {
                    Ok(loaded) => match loading.intent {
                        Intent::Compare(local) => {
                            self.cloud_changes.comparison =
                                Some(difference(&local, &loaded.report.workbook))
                        }
                        Intent::Reload { state } => {
                            if let Some(doc) =
                                self.docs.iter_mut().find(|doc| doc.key == loading.key)
                            {
                                if doc.doc.state_id() != state || doc.has_pending_drafts() {
                                    self.toasts.warning(t("Not possible here"));
                                } else if let Err(error) = doc.reload_storage(loaded) {
                                    self.toasts.error(error);
                                }
                            }
                        }
                    },
                    Err(StorageError::Cancelled) => {}
                    Err(error) => {
                        self.toasts.error(format!(
                            "{}: {}",
                            t("Could not open"),
                            crate::file_ops::storage_error(&error)
                        ));
                    }
                }
            }
            ctx.request_repaint();
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.poll_native_changes();
            if !self.file_ops.inline() && self.docs.iter().any(|doc| doc.path.is_some()) {
                ctx.request_repaint_after(std::time::Duration::from_millis(
                    if self.cloud_changes.watcher.is_some() {
                        50
                    } else {
                        1000
                    },
                ));
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn poll_native_changes(&mut self) {
        if let Some(receiver) = &self.cloud_changes.watcher {
            match receiver.try_recv() {
                Ok(observations) => {
                    self.cloud_changes.watcher = None;
                    for observation in observations {
                        let Some(doc) = self.docs.iter_mut().find(|doc| doc.key == observation.key)
                        else {
                            continue;
                        };
                        // A completed own save invalidates checks started before that save.
                        if doc.storage_revision != observation.baseline {
                            continue;
                        }
                        let changed = observation.revision != doc.storage_revision
                            || observation.error.is_some();
                        doc.externally_changed = changed;
                        let settled = if changed {
                            self.cloud_changes
                                .settling
                                .entry(doc.key.clone())
                                .or_default()
                                .observe(observation.revision.clone(), web_time::Instant::now())
                        } else {
                            self.cloud_changes.settling.remove(&doc.key);
                            self.cloud_changes
                                .observed
                                .insert(doc.key.clone(), observation.revision.clone());
                            false
                        };
                        let previous = self.cloud_changes.observed.get(&doc.key);
                        if settled
                            && previous != Some(&observation.revision)
                            && self.cloud_changes.external.is_none()
                        {
                            self.cloud_changes
                                .observed
                                .insert(doc.key.clone(), observation.revision.clone());
                            self.cloud_changes.external = Some(doc.key.clone());
                            if let Some(error) = observation.error {
                                self.toasts
                                    .error(format!("{}: {error}", t("File changed on disk")));
                            }
                        }
                        let seen = self
                            .cloud_changes
                            .seen_copies
                            .insert(doc.key.clone(), observation.copies.clone());
                        if !observation.copies.is_empty()
                            && seen.as_ref() != Some(&observation.copies)
                            && self.cloud_changes.copies.is_none()
                        {
                            self.cloud_changes.copies = Some((doc.key.clone(), observation.copies));
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.cloud_changes.watcher = None;
                    self.toasts.error(t("The file loader stopped unexpectedly"));
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.file_ops.inline()
            || self.cloud_changes.watcher.is_some()
            || self.file_ops.saving()
            || self
                .cloud_changes
                .last_watch
                .is_some_and(|last| last.elapsed().as_millis() < 1000)
        {
            return;
        }
        self.cloud_changes.last_watch = Some(web_time::Instant::now());
        let paths: Vec<_> = self
            .docs
            .iter()
            .filter_map(|doc| {
                doc.path
                    .as_ref()
                    .map(|path| (doc.key.clone(), path.clone(), doc.storage_revision.clone()))
            })
            .collect();
        if paths.is_empty() {
            return;
        }
        let (tx, receiver) = mpsc::channel();
        match std::thread::Builder::new()
            .name("thinoda-watch".into())
            .spawn(move || {
                let result = paths
                    .into_iter()
                    .map(|(key, path, baseline)| {
                        let checked = thinoda_platform::fs::fingerprint(&path);
                        let (revision, error) = match checked {
                            Ok(fp) => (
                                Some(thinoda_platform::storage::revision_from_fingerprint(&fp)),
                                None,
                            ),
                            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                                (None, None)
                            }
                            Err(error) => (None, Some(error.to_string())),
                        };
                        let (copies, copy_error) =
                            match thinoda_platform::fs::conflict_copies(&path) {
                                Ok(copies) => (copies, None),
                                Err(error) => (Vec::new(), Some(error.to_string())),
                            };
                        Observation {
                            key,
                            baseline,
                            revision,
                            copies,
                            error: error.or(copy_error),
                        }
                    })
                    .collect();
                let _ = tx.send(result);
            }) {
            Ok(_) => self.cloud_changes.watcher = Some(receiver),
            Err(error) => {
                self.toasts.error(format!(
                    "{}: {error}",
                    t("The file loader stopped unexpectedly")
                ));
            }
        }
    }

    pub(crate) fn cloud_change_dialogs(&mut self, ctx: &egui::Context) {
        use thinoda_ui::{DialogButton, confirm_dialog_with_labels as confirm_dialog};
        let labels = crate::i18n::dialog_labels();
        if let Some(loading) = &self.cloud_changes.loading {
            let response = thinoda_ui::Dialog::new(
                egui::Id::unique("thinoda_change_loading"),
                t("Opening file:"),
            )
            .labels(&labels)
            .show(ctx, |ui| {
                ui.label(loading.path.display().to_string());
                ui.spinner();
                if ui.button(t("Cancel")).clicked() {
                    loading.cancel.cancel();
                }
            });
            if response.close_requested {
                loading.cancel.cancel();
            }
        }
        if !self.cloud_changes.comparing()
            && let Some(key) = self.cloud_changes.external.clone()
        {
            let buttons = [
                (t("Reload file"), DialogButton::Primary),
                (t("Keep my edits"), DialogButton::Secondary),
                (t("Compare"), DialogButton::Secondary),
            ];
            if let Some(action) = confirm_dialog(
                ctx,
                "thinoda_external",
                t("File changed on disk"),
                t("Reload the changed file? Your current content remains available through Undo."),
                &buttons,
                &crate::i18n::dialog_labels(),
            ) {
                self.cloud_changes.external = None;
                if let Some(doc) = self.docs.iter().find(|doc| doc.key == key)
                    && let Some(path) = doc.path.clone()
                {
                    match action {
                        0 => self.start_change_load(
                            &key,
                            path,
                            Intent::Reload {
                                state: doc.doc.state_id(),
                            },
                        ),
                        2 => self.compare_file(&key, &path),
                        _ => {}
                    }
                }
            }
        }
        if let Some((key, copies)) = self.cloud_changes.copies.clone() {
            let mut action = None;
            let response = thinoda_ui::Dialog::new(
                egui::Id::unique("thinoda_conflict_copies"),
                t("Sync conflict copies"),
            )
            .labels(&labels)
            .show(ctx, |ui| {
                ui.label(t(
                    "These files are kept by your sync client. Thinoda never deletes them.",
                ));
                for path in copies {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(path.display().to_string());
                        if ui.button(t("Open")).clicked() {
                            action = Some((path.clone(), false));
                        }
                        if ui.button(t("Compare")).clicked() {
                            action = Some((path, true));
                        }
                    });
                }
            });
            if let Some((path, compare)) = action {
                self.cloud_changes.copies = None;
                if compare {
                    self.compare_file(&key, &path);
                } else {
                    self.request_open(&path);
                }
            } else if response.close_requested {
                self.cloud_changes.copies = None;
            }
        }
        if let Some(diff) = &self.cloud_changes.comparison {
            let response = thinoda_ui::Dialog::new(egui::Id::unique("thinoda_compare"), t("Compare")).labels(&labels).show(ctx, |ui| {
                for (key, count) in [
                    ("Sheets added", diff.sheets_added), ("Sheets removed", diff.sheets_removed),
                    ("Sheets changed", diff.sheets_changed), ("Topics added", diff.topics_added),
                    ("Topics removed", diff.topics_removed), ("Topics changed", diff.topics_changed),
                ] { ui.label(format!("{}: {count}", t(key))); }
                ui.label(t("Sheet positions and topic IDs are compared; this summary does not merge content."));
            });
            if response.close_requested {
                self.cloud_changes.comparison = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn transient_sync_steps_do_not_prompt_until_a_revision_settles() {
        let now = web_time::Instant::now();
        let mut settling = Settling::default();
        assert!(!settling.observe(None, now));
        assert!(!settling.observe(
            Some(Revision("new".into())),
            now + std::time::Duration::from_millis(100)
        ));
        assert!(!settling.observe(
            Some(Revision("new".into())),
            now + std::time::Duration::from_millis(300)
        ));
        assert!(settling.observe(
            Some(Revision("new".into())),
            now + std::time::Duration::from_millis(600)
        ));
    }

    #[test]
    fn summary_counts_topic_edits_and_complete_added_sheets() {
        let before = Workbook::default();
        let mut after = before.clone();
        let root = after.sheets[0].root;
        after.sheets[0].set_title(root, "Changed").unwrap();
        after.sheets.push(thinoda_core::Sheet::new("Added", "New"));
        assert_eq!(
            difference(&before, &after),
            Difference {
                sheets_added: 1,
                sheets_changed: 1,
                topics_added: 1,
                topics_changed: 1,
                ..Default::default()
            }
        );
        assert_eq!(difference(&before, &before), Difference::default());
    }

    fn recovered_app() -> ThinodaApp {
        let mut app = ThinodaApp::new(Default::default(), crate::app::Services::default(), &[]);
        let mut doc = crate::doc::OpenDoc::new(Workbook::default());
        doc.path = Some(PathBuf::from("recovered.thinoda"));
        doc.doc.mark_dirty();
        app.docs.push(doc);
        app.active = 0;
        app
    }

    fn pending_revision(
        app: &mut ThinodaApp,
    ) -> mpsc::Sender<Result<Option<Revision>, StorageError>> {
        let (tx, receiver) = mpsc::channel();
        app.cloud_changes.revision_refreshes.push(RevisionRefresh {
            key: app.docs[0].key.clone(),
            path: app.docs[0].path.clone().unwrap(),
            receiver,
        });
        tx
    }

    #[test]
    fn recovered_revision_preserves_unsaved_content_and_waits_for_metadata() {
        let mut app = recovered_app();
        let before = app.docs[0].doc.workbook().clone();
        let tx = pending_revision(&mut app);
        app.poll_cloud_changes(&egui::Context::default());
        assert!(app.docs[0].storage_revision.is_none());
        assert_eq!(app.cloud_changes.revision_refreshes.len(), 1);
        tx.send(Ok(Some(Revision("browser-revision".into()))))
            .unwrap();
        app.poll_cloud_changes(&egui::Context::default());
        assert_eq!(
            app.docs[0].storage_revision,
            Some(Revision("browser-revision".into()))
        );
        assert!(app.docs[0].fingerprint.is_none());
        assert_eq!(app.docs[0].doc.workbook(), &before);
        assert!(app.docs[0].is_dirty());
    }

    #[test]
    fn recovered_revision_cannot_rebind_a_retargeted_or_saved_document() {
        for retarget in [true, false] {
            let mut app = recovered_app();
            let tx = pending_revision(&mut app);
            if retarget {
                app.docs[0].path = Some(PathBuf::from("other.thinoda"));
            } else {
                app.docs[0].storage_revision = Some(Revision("saved".into()));
            }
            let before = app.docs[0].storage_revision.clone();
            tx.send(Ok(Some(Revision("old".into())))).unwrap();
            app.poll_cloud_changes(&egui::Context::default());
            assert_eq!(app.docs[0].storage_revision, before);
        }
    }

    #[test]
    fn missing_or_failed_recovered_identity_requires_conflict_resolution() {
        for result in [Ok(None), Err(StorageError::Remote("unavailable".into()))] {
            let mut app = recovered_app();
            let tx = pending_revision(&mut app);
            tx.send(result).unwrap();
            app.poll_cloud_changes(&egui::Context::default());
            assert!(app.docs[0].storage_revision.is_none());
            assert!(app.docs[0].externally_changed);
            assert!(app.docs[0].is_dirty());
        }
    }
}
