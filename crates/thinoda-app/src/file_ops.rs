//! Document open and save work that keeps the UI painting.
//!
//! Importing or writing a large document (for example a FIM model) can take a while. The
//! interactive app runs that work on a worker thread and paints a busy indicator with the
//! file name until the result is applied on the UI thread, like the startup loader does.
//! Browser storage is awaited asynchronously; native I/O runs on a worker.
//!
//! Progress inside one file cannot be measured, so the indicator is indeterminate; only the
//! position within several queued files is shown. A save writes a snapshot of the committed
//! content: the document only becomes clean if it still holds exactly that content when
//! the write completes, so edits made meanwhile stay unsaved.

use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
};

use egui::{Context, ViewportId};
use thinoda_io::Format;
use thinoda_platform::storage::{CancelToken, StorageError, StorageFuture};
use thinoda_ui::tokens::Palette;

use crate::{ThinodaApp, doc::OpenDoc, i18n::t, startup::display_name};

pub(crate) const DOCUMENT_TOO_LARGE: &str = "This file exceeds the supported size limit.";

pub(crate) fn storage_error(error: &StorageError) -> String {
    match error {
        StorageError::TooLarge => t(DOCUMENT_TOO_LARGE).into(),
        _ => error.to_string(),
    }
}

/// How document file operations started from the UI are executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FileWorkMode {
    /// Work completes inside the requesting call (operation API, headless use).
    #[default]
    Inline,
    /// Work runs on a worker thread; deferred work is used where threads are unavailable.
    Background,
    /// Work runs on the UI thread once the busy indicator has been presented (browser).
    Deferred,
}

/// A document file operation in progress.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileBusy {
    /// Requested file `index` (1-based) of `total` queued files is being opened.
    Opening {
        index: usize,
        total: usize,
        name: String,
    },
    /// The document is being written to the named file.
    Saving { name: String },
    /// A user-requested read of an online-only sync placeholder.
    Downloading { name: String },
}

type Job<R> = Box<dyn FnOnce() -> StorageFuture<'static, R> + Send>;

enum Work<R> {
    /// Requested but not started yet (starting needs the frame context).
    Pending(Option<Job<R>>),
    Thread(Receiver<Result<R, StorageError>>),
    Deferred(Option<Job<R>>),
}

impl<R: Send + 'static> Work<R> {
    fn new(job: Job<R>) -> Self {
        Self::Pending(Some(job))
    }

    fn start(&mut self, mode: FileWorkMode, gate: &Arc<Gate>, ctx: &Context) {
        let Self::Pending(job) = self else {
            return;
        };
        let Some(job) = job.take() else {
            *self = Self::Deferred(None);
            return;
        };
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (mode, gate);
            let (tx, rx) = mpsc::channel();
            let ctx = ctx.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let _ = tx.send(job().await);
                ctx.request_repaint_of(ViewportId::ROOT);
            });
            *self = Self::Thread(rx);
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            if mode != FileWorkMode::Background {
                *self = Self::Deferred(Some(job));
                return;
            }
            let slot = Arc::new(Mutex::new(Some(job)));
            let (tx, rx) = mpsc::channel();
            let (worker_slot, gate, ctx) = (slot.clone(), gate.clone(), ctx.clone());
            let spawned = std::thread::Builder::new()
                .name("thinoda-file".into())
                .spawn(move || {
                    let job = worker_slot
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .take();
                    if let Some(job) = job {
                        gate.wait();
                        let _ = tx.send(crate::doc::run_storage(job()));
                        ctx.request_repaint_of(ViewportId::ROOT);
                    }
                });
            *self = match spawned {
                Ok(_) => Self::Thread(rx),
                Err(error) => {
                    let (tx, rx) = mpsc::channel();
                    let _ = tx.send(Err(StorageError::Io(error)));
                    Self::Thread(rx)
                }
            };
        }
    }

    fn deferred(&self) -> bool {
        matches!(self, Self::Deferred(_))
    }

    /// The finished result. Deferred work only runs when blocking the UI is acceptable.
    fn poll(&mut self, may_block: bool, stopped: &str) -> Option<Result<R, StorageError>> {
        match self {
            Self::Pending(_) => None,
            Self::Thread(rx) => match rx.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => {
                    Some(Err(StorageError::Remote(stopped.to_owned())))
                }
            },
            Self::Deferred(job) => may_block.then(|| {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    job.take().map_or_else(
                        || Err(StorageError::Remote(stopped.to_owned())),
                        |job| crate::doc::run_storage(job()),
                    )
                }
                #[cfg(target_arch = "wasm32")]
                {
                    let _ = job;
                    Err(StorageError::Remote(stopped.to_owned()))
                }
            }),
        }
    }
}

/// Lets tests hold workers before they start, to observe the indicator deterministically.
#[derive(Default)]
struct Gate {
    paused: Mutex<bool>,
    resume: Condvar,
}

impl Gate {
    fn set(&self, paused: bool) {
        *self.paused.lock().unwrap_or_else(PoisonError::into_inner) = paused;
        self.resume.notify_all();
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn wait(&self) {
        let mut paused = self.paused.lock().unwrap_or_else(PoisonError::into_inner);
        while *paused {
            paused = self
                .resume
                .wait(paused)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

struct Opening {
    path: PathBuf,
    viewport: ViewportId,
    work: Work<crate::doc::LoadedWorkbook>,
    cancel: CancelToken,
    downloading: Arc<AtomicBool>,
}

struct Saving {
    key: String,
    target: PathBuf,
    format: Format,
    /// Content state that is being written.
    state: u64,
    /// Browser: download the written file once it is complete.
    deliver: bool,
    autosave: bool,
    consent: crate::export_preflight::SaveConsent,
    work: Work<crate::doc::WrittenWorkbook>,
    cancel: CancelToken,
}

/// Consent applies only to the same content and destination on an explicit
/// overwrite retry after a queued write encountered a conflict.
struct ConflictRetry {
    key: String,
    target: PathBuf,
    state: u64,
    deliver: bool,
    consent: crate::export_preflight::SaveConsent,
}

#[derive(Default)]
pub(crate) struct FileOps {
    mode: FileWorkMode,
    gate: Arc<Gate>,
    queued: VecDeque<(PathBuf, ViewportId)>,
    opening: Option<Opening>,
    /// Files of the current open batch that already finished.
    opened: usize,
    save: Option<Saving>,
    conflict_retry: Option<ConflictRetry>,
    /// Root frame in which the indicator was first drawn for pending deferred work.
    presented: Option<u64>,
}

impl FileOps {
    pub(crate) fn inline(&self) -> bool {
        self.mode == FileWorkMode::Inline
    }

    pub(crate) fn saving(&self) -> bool {
        self.save.is_some()
    }

    pub(crate) fn saving_key(&self, key: &str) -> bool {
        self.save.as_ref().is_some_and(|save| save.key == key)
    }

    fn opening_path(&self, path: &Path) -> bool {
        self.opening.as_ref().is_some_and(|o| o.path == path)
            || self.queued.iter().any(|(queued, _)| queued == path)
    }

    fn activity(&self) -> Vec<FileBusy> {
        let mut busy = Vec::new();
        if let Some(save) = &self.save {
            busy.push(FileBusy::Saving {
                name: display_name(&save.target),
            });
        }
        let current = self
            .opening
            .as_ref()
            .map(|o| &o.path)
            .or_else(|| self.queued.front().map(|(path, _)| path));
        if let Some(path) = current {
            if self
                .opening
                .as_ref()
                .is_some_and(|opening| opening.downloading.load(Ordering::Relaxed))
            {
                busy.push(FileBusy::Downloading {
                    name: display_name(path),
                });
            } else {
                busy.push(FileBusy::Opening {
                    index: self.opened + 1,
                    total: self.opened + usize::from(self.opening.is_some()) + self.queued.len(),
                    name: display_name(path),
                });
            }
        }
        busy
    }

    /// Deferred work may block the UI thread once a frame showing the indicator was
    /// presented: the indicator is drawn in frame `n`, and frame `n + 1` repeats it after
    /// any first-frame sizing pass of the overlay area.
    fn may_block(&self, ctx: &Context) -> bool {
        self.presented
            .is_some_and(|frame| ctx.cumulative_frame_nr_for(ViewportId::ROOT) >= frame + 2)
    }
}

impl ThinodaApp {
    pub(crate) fn saving_document(&self) -> bool {
        self.file_ops.saving() || self.cloud_program.saving()
    }

    pub(crate) fn document_save_pending(&self, key: &str) -> bool {
        self.file_ops.saving_key(key) || self.cloud_program.saving_key(key)
    }

    pub(crate) fn configured_backup_directory(
        &self,
        backup: bool,
    ) -> Result<Option<PathBuf>, String> {
        if !backup {
            return Ok(None);
        }
        if let Some(path) = &self.settings.cloud.backup_folder
            && !crate::cloud_program::backup_folder_allowed(path, &self.services.cloud)
        {
            return Err(t("Choose a backup folder outside every detected cloud folder.").into());
        }
        Ok(self.settings.cloud.backup_folder.clone())
    }

    /// Selects how UI-requested opens and saves run. The interactive app uses
    /// [`FileWorkMode::Background`]; the default completes work inside each call.
    pub fn set_file_work_mode(&mut self, mode: FileWorkMode) {
        self.file_ops.mode = mode;
    }

    /// Document file operations in progress, saving first.
    pub fn file_activity(&self) -> Vec<FileBusy> {
        self.file_ops.activity()
    }

    /// Holds (or releases) background workers before they start their file work.
    #[doc(hidden)]
    pub fn pause_file_workers(&self, paused: bool) {
        self.file_ops.gate.set(paused);
    }

    /// Opens `path`, queued behind the busy indicator unless file work runs inline.
    /// Already open or queued files are not opened twice. UI event producers should
    /// dispatch `Cmd::OpenPath` through `exec` to order pending card input first.
    pub fn request_open(&mut self, path: &Path) {
        if self.file_ops.inline() {
            self.open_path(path);
            return;
        }
        if self.activate_open_path(path)
            || self.dialogs.imports.iter().any(|(p, ..)| p == path)
            || self.file_ops.opening_path(path)
        {
            return;
        }
        self.file_ops
            .queued
            .push_back((path.to_path_buf(), self.current_viewport));
    }

    /// Commits drafts of document `index` and queues writing that content to `target`.
    pub(crate) fn queue_save(
        &mut self,
        index: usize,
        target: &Path,
        backup: bool,
        deliver: bool,
        autosave: bool,
    ) -> Result<(), String> {
        self.queue_save_with_consent(
            index,
            target,
            backup,
            deliver,
            autosave,
            crate::export_preflight::SaveConsent::default(),
        )
    }

    pub(crate) fn queue_save_with_consent(
        &mut self,
        index: usize,
        target: &Path,
        backup: bool,
        deliver: bool,
        autosave: bool,
        mut consent: crate::export_preflight::SaveConsent,
    ) -> Result<(), String> {
        debug_assert!(self.file_ops.save.is_none());
        let backup_directory = self.configured_backup_directory(backup)?;
        let doc = &mut self.docs[index];
        let expected = doc.expected_revision(target)?;
        let format = doc.prepare_save(target)?;
        let mut deliver = deliver;
        if let Some(retry) = self.file_ops.conflict_retry.take()
            && doc.ignore_conflict
            && retry.key == doc.key
            && retry.target.as_path() == target
        {
            deliver |= retry.deliver;
            if retry.state == doc.doc.state_id() {
                consent.board |= retry.consent.board;
                consent.mindmanager |= retry.consent.mindmanager;
            } else if !autosave
                && format == Format::MindManager
                && retry.consent.mindmanager
                && !consent.mindmanager
            {
                // A retry may arrive from the board preflight after the
                // document changed. The earlier MindManager consent no longer
                // applies; resume through the normal confirmation dialog.
                self.dialogs.mindmanager_save =
                    Some((doc.key.clone(), target.to_path_buf(), false));
                self.dialogs.mindmanager_save_deliver = deliver;
                return Ok(());
            }
        }
        let workbook = doc.doc.workbook().clone();
        let path = target.to_path_buf();
        let cancel = CancelToken::default();
        let worker_cancel = cancel.clone();
        let job: Job<crate::doc::WrittenWorkbook> = Box::new(move || {
            Box::pin(async move {
                crate::doc::write_workbook_async(
                    &workbook,
                    format,
                    &path,
                    backup,
                    expected,
                    worker_cancel,
                    backup_directory,
                    consent,
                )
                .await
            })
        });
        self.file_ops.save = Some(Saving {
            key: doc.key.clone(),
            target: target.to_path_buf(),
            format,
            state: doc.doc.state_id(),
            deliver,
            autosave,
            consent,
            work: Work::new(job),
            cancel,
        });
        Ok(())
    }

    /// Starts requested work and applies finished results; called once per frame.
    pub(crate) fn poll_file_ops(&mut self, ctx: &Context) {
        if crate::ui::card_text_input_has_pending_events(ctx, self.current_viewport) {
            ctx.request_repaint_of(self.current_viewport);
            return;
        }
        let mode = self.file_ops.mode;
        let gate = self.file_ops.gate.clone();
        let mut may_block = self.file_ops.may_block(ctx);
        if let Some(save) = &mut self.file_ops.save {
            save.work.start(mode, &gate, ctx);
            let deferred = save.work.deferred();
            if let Some(result) = save
                .work
                .poll(may_block, t("The file writer stopped unexpectedly"))
            {
                if deferred {
                    may_block = false;
                    self.file_ops.presented = None;
                }
                let save = self.file_ops.save.take().expect("save state checked above");
                self.finish_file_save(ctx, save, result);
            }
        }
        self.next_open();
        let ready = self.file_ops.opening.as_ref().is_some_and(|opening| {
            opening.viewport == self.current_viewport
                || (opening.viewport != ViewportId::ROOT
                    && !self
                        .document_windows
                        .values()
                        .any(|owner| *owner == opening.viewport))
        });
        if ready && let Some(opening) = &mut self.file_ops.opening {
            opening.work.start(mode, &gate, ctx);
            let deferred = opening.work.deferred();
            if let Some(result) = opening
                .work
                .poll(may_block, t("The file loader stopped unexpectedly"))
            {
                if deferred {
                    self.file_ops.presented = None;
                }
                let path = self
                    .file_ops
                    .opening
                    .take()
                    .expect("open state checked above")
                    .path;
                self.file_ops.opened += 1;
                if !matches!(result, Err(StorageError::Cancelled)) {
                    self.add_opened(
                        &path,
                        result
                            .map_err(|error| storage_error(&error))
                            .and_then(|report| OpenDoc::from_storage_report(&path, report)),
                    );
                }
                self.next_open();
                if let Some(opening) = &mut self.file_ops.opening
                    && opening.viewport == self.current_viewport
                {
                    opening.work.start(mode, &gate, ctx);
                }
            }
        }
        if self.file_ops.opening.is_none() && self.file_ops.queued.is_empty() {
            self.file_ops.opened = 0;
        }
        if self.file_ops.activity().is_empty() {
            self.file_ops.presented = None;
        }
        if self.file_ops.save.is_some() || self.file_ops.opening.is_some() {
            ctx.request_repaint();
        }
    }

    fn next_open(&mut self) {
        if self.file_ops.opening.is_some() {
            return;
        }
        while let Some((path, viewport)) = self.file_ops.queued.pop_front() {
            // Opened meanwhile (for example by a launch file or another request).
            if self
                .docs
                .iter()
                .any(|d| d.path.as_deref() == Some(path.as_path()))
            {
                self.file_ops.opened += 1;
                continue;
            }
            let source = path.clone();
            let cancel = CancelToken::default();
            let worker_cancel = cancel.clone();
            let downloading = Arc::new(AtomicBool::new(false));
            let worker_downloading = downloading.clone();
            let job: Job<crate::doc::LoadedWorkbook> = Box::new(move || {
                Box::pin(async move {
                    let online_only = thinoda_platform::fs::online_only(&source)?;
                    worker_downloading.store(online_only, Ordering::Relaxed);
                    crate::doc::load_workbook_async(&source, worker_cancel).await
                })
            });
            self.file_ops.opening = Some(Opening {
                path,
                viewport,
                work: Work::new(job),
                cancel,
                downloading,
            });
            return;
        }
    }

    fn finish_file_save(
        &mut self,
        ctx: &Context,
        save: Saving,
        result: Result<crate::doc::WrittenWorkbook, StorageError>,
    ) {
        let index = self.docs.iter().position(|doc| doc.key == save.key);
        let saved = match result {
            Ok(identity) => {
                if let Some(index) = index {
                    let doc = &mut self.docs[index];
                    doc.finish_save(&save.target, save.format, save.state, identity);
                    // Edits made while writing keep their crash-recovery snapshot.
                    if !doc.is_dirty()
                        && let Some(recovery) = &self.services.recovery
                    {
                        doc.cleanup_recovery(recovery);
                    }
                }
                if !save.autosave {
                    self.cloud_program.forget_document(&save.key);
                    self.settings.add_recent(&save.target);
                    self.save_settings();
                    self.toasts
                        .success(format!("{} {}", t("Saved to"), save.target.display()));
                    if index.is_some_and(|index| self.docs[index].has_unresolved_ink()) {
                        self.toasts.warning(t(
                            "Pending ink is not in the saved file. Crash recovery may miss recent samples.",
                        ));
                    }
                }
                if save.deliver {
                    self.deliver(&save.target);
                }
                true
            }
            Err(error) => {
                if let Some(index) = index {
                    let doc = &mut self.docs[index];
                    doc.ignore_conflict = false;
                    if save.autosave {
                        doc.autosave_failed(self.settings.autosave_interval_secs.max(5) as f32);
                    }
                }
                match error {
                    StorageError::Conflict { .. } => {
                        self.file_ops.conflict_retry = Some(ConflictRetry {
                            key: save.key.clone(),
                            target: save.target.clone(),
                            state: save.state,
                            deliver: save.deliver,
                            consent: save.consent,
                        });
                        self.dialogs.conflict = Some(save.key.clone());
                        self.dialogs.conflict_save_target =
                            Some((save.key.clone(), save.target.clone()));
                    }
                    StorageError::Cancelled => {}
                    error => {
                        self.toasts.error(format!(
                            "{}: {}",
                            t("Could not save"),
                            storage_error(&error)
                        ));
                    }
                }
                false
            }
        };
        if !save.autosave && !self.resolve_save_all(ctx, &save.key, saved) {
            self.resolve_save_close_tab(&save.key, saved);
        }
    }

    /// Cancels pending I/O without closing documents or discarding edits.
    pub fn cancel_file_work(&mut self) {
        self.file_ops.queued.clear();
        if let Some(opening) = &self.file_ops.opening {
            opening.cancel.cancel();
        }
        if let Some(save) = &self.file_ops.save {
            save.cancel.cancel();
        }
    }

    /// Nonmodal busy card for document file operations; editing stays possible.
    pub(crate) fn file_busy_overlay(&mut self, ctx: &Context) {
        let busy = self.file_ops.activity();
        if busy.is_empty() {
            return;
        }
        if ctx.viewport_id() == ViewportId::ROOT && self.file_ops.presented.is_none() {
            self.file_ops.presented = Some(ctx.cumulative_frame_nr());
        }
        let palette = Palette::of(ctx);
        egui::Area::new(egui::Id::unique("thinoda_file_busy"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -48.0))
            .interactable(true)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .fill(palette.surface)
                    .stroke(egui::Stroke::new(1.0, palette.border))
                    .show(ui, |ui| {
                        ui.set_max_width(360.0);
                        for item in &busy {
                            match item {
                                FileBusy::Saving { name } | FileBusy::Downloading { name } => {
                                    ui.horizontal(|ui| {
                                        ui.add(egui::Spinner::new());
                                        ui.add(
                                            egui::Label::new(format!(
                                                "{} {name}",
                                                if matches!(item, FileBusy::Downloading { .. }) {
                                                    t("Downloading file:")
                                                } else {
                                                    t("Saving file:")
                                                }
                                            ))
                                            .truncate(),
                                        );
                                    });
                                }
                                FileBusy::Opening { index, total, name } => {
                                    ui.horizontal(|ui| {
                                        ui.add(egui::Spinner::new());
                                        ui.add(
                                            egui::Label::new(format!(
                                                "{} {name}",
                                                t("Opening file:")
                                            ))
                                            .truncate(),
                                        );
                                    });
                                    // Like the startup splash: only finished files count.
                                    if *total > 1 {
                                        ui.add(
                                            egui::ProgressBar::new(
                                                (*index - 1) as f32 / *total as f32,
                                            )
                                            .desired_width(320.0)
                                            .text(format!("{index} / {total}")),
                                        );
                                    }
                                }
                            }
                        }
                        if ui.button(t("Cancel")).clicked() {
                            self.cancel_file_work();
                        }
                    });
            });
        ctx.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{DOCUMENT_TOO_LARGE, FileWorkMode, storage_error};
    use crate::{ThinodaApp, export_preflight::SaveConsent};
    use thinoda_platform::storage::StorageError;

    #[test]
    fn size_limit_errors_are_localized_without_hiding_other_errors() {
        assert_eq!(
            storage_error(&StorageError::TooLarge),
            crate::i18n::t(DOCUMENT_TOO_LARGE)
        );
        assert_eq!(
            storage_error(&StorageError::Unsupported),
            StorageError::Unsupported.to_string()
        );
    }

    fn failures(app: &ThinodaApp) -> usize {
        app.toasts
            .texts()
            .filter(|text| text.starts_with("Could not save"))
            .count()
    }

    fn finish(app: &mut ThinodaApp, ctx: &egui::Context) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !app.file_activity().is_empty() {
            assert!(Instant::now() < deadline, "file work never completed");
            app.poll_file_ops(ctx);
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn add_board_shape(app: &mut ThinodaApp) {
        use thinoda_core::{CanvasObject, Rect, ShapeKind, Vec2};

        app.docs[0]
            .doc
            .edit_sheet("Add shape", |sheet| {
                sheet
                    .add_canvas_object(CanvasObject::shape(
                        ShapeKind::Rectangle,
                        Rect::from_min_size(Vec2::ZERO, Vec2::new(40.0, 40.0)),
                    ))
                    .unwrap();
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn queued_mindmanager_save_requires_both_loss_consents() {
        for (board, mindmanager, should_save) in [
            (false, false, false),
            (true, false, false),
            (false, true, false),
            (true, true, true),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("board.mmap");
            let original = b"existing destination";
            std::fs::write(&path, original).unwrap();
            let mut app = ThinodaApp::headless();
            add_board_shape(&mut app);
            app.set_file_work_mode(FileWorkMode::Background);
            app.queue_save_with_consent(
                0,
                &path,
                true,
                true,
                false,
                SaveConsent { board, mindmanager },
            )
            .unwrap();
            finish(&mut app, &egui::Context::default());

            let backup = thinoda_platform::fs::backup_path(&path);
            if should_save {
                assert_ne!(std::fs::read(&path).unwrap(), original);
                assert_eq!(std::fs::read(backup).unwrap(), original);
                assert!(!app.docs[0].is_dirty());
            } else {
                assert_eq!(std::fs::read(&path).unwrap(), original);
                assert!(!backup.exists());
                assert!(app.docs[0].is_dirty());
                assert_eq!(failures(&app), 1);
            }
        }
    }

    #[test]
    fn conflict_retry_reuses_consent_only_for_the_same_saved_content() {
        for changed in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("save-as.mmap");
            std::fs::write(&path, b"original destination").unwrap();
            let mut app = ThinodaApp::headless();
            add_board_shape(&mut app);
            app.set_file_work_mode(FileWorkMode::Background);
            app.pause_file_workers(true);
            app.queue_save_with_consent(
                0,
                &path,
                true,
                true,
                false,
                SaveConsent {
                    board: true,
                    mindmanager: true,
                },
            )
            .unwrap();
            let ctx = egui::Context::default();
            app.poll_file_ops(&ctx);
            let external = b"external modification after the save was queued";
            std::fs::write(&path, external).unwrap();
            app.pause_file_workers(false);
            finish(&mut app, &ctx);
            assert_eq!(std::fs::read(&path).unwrap(), external);
            assert_eq!(
                app.dialogs.conflict.as_deref(),
                Some(app.docs[0].key.as_str())
            );

            if changed {
                let doc = &mut app.docs[0].doc;
                let root = doc.sheet().root;
                doc.edit_sheet("New edit", |sheet| sheet.set_title(root, "New content"))
                    .unwrap();
            }
            // Model the conflict dialog's explicit Overwrite followed by a fresh
            // board confirmation. MindManager consent came from the first attempt.
            app.dialogs.conflict = None;
            app.dialogs.conflict_save_target = None;
            app.docs[0].ignore_conflict = true;
            app.queue_save_with_consent(
                0,
                &path,
                true,
                false,
                false,
                SaveConsent {
                    board: true,
                    mindmanager: false,
                },
            )
            .unwrap();
            if changed {
                assert!(app.file_ops.save.is_none());
                assert_eq!(
                    app.dialogs.mindmanager_save.as_ref(),
                    Some(&(app.docs[0].key.clone(), path.clone(), false))
                );
                assert_eq!(std::fs::read(&path).unwrap(), external);
                assert!(app.docs[0].is_dirty());
                app.dialogs.mindmanager_save = None;
                let deliver = std::mem::take(&mut app.dialogs.mindmanager_save_deliver);
                assert!(deliver);
                app.queue_save_with_consent(
                    0,
                    &path,
                    true,
                    deliver,
                    false,
                    SaveConsent {
                        board: true,
                        mindmanager: true,
                    },
                )
                .unwrap();
            }
            assert!(app.file_ops.save.as_ref().unwrap().deliver);
            finish(&mut app, &ctx);

            let backup = thinoda_platform::fs::backup_path(&path);
            assert_ne!(std::fs::read(&path).unwrap(), external);
            assert_eq!(std::fs::read(backup).unwrap(), external);
            assert!(!app.docs[0].is_dirty());
        }
    }

    #[test]
    fn cancelled_open_does_not_install_a_document() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cancel.thinoda");
        let mut app = ThinodaApp::headless();
        app.docs[0].save_to(&path, false).unwrap();
        app.docs.clear();
        app.set_file_work_mode(FileWorkMode::Background);
        app.pause_file_workers(true);
        app.request_open(&path);
        let ctx = egui::Context::default();
        app.poll_file_ops(&ctx);
        app.cancel_file_work();
        app.pause_file_workers(false);
        finish(&mut app, &ctx);
        assert!(app.docs.is_empty());
        assert_eq!(app.toasts.texts().count(), 0);
    }

    #[test]
    fn cancelled_save_keeps_original_bytes_and_unsaved_edits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cancel.thinoda");
        let mut app = ThinodaApp::headless();
        app.docs[0].save_to(&path, false).unwrap();
        let original = std::fs::read(&path).unwrap();
        let doc = &mut app.docs[0];
        let root = doc.doc.sheet().root;
        doc.doc
            .edit_sheet("Edit", |sheet| sheet.set_title(root, "Unsaved"))
            .unwrap();
        app.set_file_work_mode(FileWorkMode::Background);
        app.pause_file_workers(true);
        app.queue_save(0, &path, true, false, false).unwrap();
        let ctx = egui::Context::default();
        app.poll_file_ops(&ctx);
        app.cancel_file_work();
        app.pause_file_workers(false);
        finish(&mut app, &ctx);
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert!(app.docs[0].is_dirty());
        assert!(!thinoda_platform::fs::backup_path(&path).exists());
        assert_eq!(failures(&app), 0);
    }

    #[test]
    fn queued_save_rechecks_revision_after_an_external_edit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stale.thinoda");
        let mut app = ThinodaApp::headless();
        app.docs[0].save_to(&path, false).unwrap();
        app.set_file_work_mode(FileWorkMode::Background);
        app.pause_file_workers(true);
        app.queue_save(0, &path, true, false, false).unwrap();
        let ctx = egui::Context::default();
        app.poll_file_ops(&ctx);
        std::fs::write(&path, b"external edit").unwrap();
        app.pause_file_workers(false);
        finish(&mut app, &ctx);
        assert_eq!(std::fs::read(&path).unwrap(), b"external edit");
        assert_eq!(
            app.dialogs.conflict.as_deref(),
            Some(app.docs[0].key.as_str())
        );
        assert!(!thinoda_platform::fs::backup_path(&path).exists());
    }

    #[test]
    fn queued_save_uses_the_configured_backup_directory() {
        let dir = tempfile::tempdir().unwrap();
        let sources = dir.path().join("sources");
        let backups = dir.path().join("backups");
        std::fs::create_dir(&sources).unwrap();
        std::fs::create_dir(&backups).unwrap();
        let path = sources.join("map.thinoda");
        let mut app = ThinodaApp::headless();
        app.docs[0].save_to(&path, false).unwrap();
        let original = std::fs::read(&path).unwrap();
        let doc = &mut app.docs[0].doc;
        let root = doc.sheet().root;
        doc.edit_sheet("Edit", |sheet| sheet.set_title(root, "Updated"))
            .unwrap();
        app.settings.cloud.backup_folder = Some(backups.clone());
        app.set_file_work_mode(FileWorkMode::Background);
        app.queue_save(0, &path, true, false, false).unwrap();
        finish(&mut app, &egui::Context::default());

        assert_ne!(std::fs::read(&path).unwrap(), original);
        assert!(!thinoda_platform::fs::backup_path(&path).exists());
        let entries = std::fs::read_dir(backups)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(std::fs::read(entries[0].path()).unwrap(), original);
        assert!(!app.docs[0].is_dirty());
    }

    #[test]
    fn background_save_binds_chat_to_written_bytes_not_newer_edits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chat.thinoda");
        let mut app = ThinodaApp::headless();
        app.set_file_work_mode(FileWorkMode::Background);
        app.pause_file_workers(true);
        app.queue_save(0, &path, false, false, false).unwrap();
        let ctx = egui::Context::default();
        app.poll_file_ops(&ctx);
        let doc = app.doc_mut().unwrap();
        let root = doc.doc.sheet().root;
        doc.doc
            .edit_sheet("Newer edit", |sheet| sheet.set_title(root, "Still unsaved"))
            .unwrap();
        app.pause_file_workers(false);
        let deadline = Instant::now() + Duration::from_secs(20);
        while app.file_ops.saving() {
            assert!(Instant::now() < deadline, "save never completed");
            app.poll_file_ops(&ctx);
            std::thread::sleep(Duration::from_millis(2));
        }
        let bytes = std::fs::read(&path).unwrap();
        let expected = thinoda_platform::chat_history::Binding::from_bytes(&path, &bytes).unwrap();
        let doc = app.doc().unwrap();
        assert_eq!(doc.chat_identity.as_ref(), Some(&expected));
        assert!(doc.is_dirty());
        assert_ne!(
            doc.chat_identity,
            Some(
                thinoda_platform::chat_history::Binding::from_bytes(
                    &path,
                    &thinoda_io::export_bytes(
                        doc.doc.workbook(),
                        thinoda_io::Format::Thinoda,
                        &thinoda_io::ExportOptions::default(),
                    )
                    .unwrap(),
                )
                .unwrap()
            )
        );
    }

    #[test]
    fn background_save_cleanup_failure_does_not_remove_newer_edits() {
        use std::sync::Arc;
        use thinoda_platform::{Paths, recovery::RecoveryManager};

        for (autosave, newer_edit) in [(false, false), (true, false), (false, true), (true, true)] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("background.thinoda");
            let recovery = Arc::new(RecoveryManager::start(&Paths::from_root(dir.path())).unwrap());
            let mut app = ThinodaApp::headless();
            app.services.recovery = Some(Arc::clone(&recovery));
            let doc = &mut app.docs[0];
            doc.doc
                .edit_sheet("Before", |sheet| sheet.set_title(sheet.root, "Before"))
                .unwrap();
            doc.snapshot(&recovery);
            recovery.suspend().unwrap();
            let snapshot_path = recovery
                .session_dir()
                .join("docs")
                .join(format!("{}.json", doc.key));
            let original = std::fs::read(&snapshot_path).unwrap();
            app.set_file_work_mode(FileWorkMode::Background);
            app.queue_save(0, &path, false, false, autosave).unwrap();
            if newer_edit {
                app.docs[0]
                    .doc
                    .edit_sheet("After", |sheet| sheet.set_title(sheet.root, "After"))
                    .unwrap();
                app.docs[0].snapshot(&recovery);
            }
            finish(&mut app, &egui::Context::default());
            assert_eq!(std::fs::read(&snapshot_path).unwrap(), original);
            assert_eq!(app.docs[0].is_dirty(), newer_edit);
            assert_eq!(app.docs[0].recovery_cleanup_failed, !newer_edit);
            recovery.resume().unwrap();
            if newer_edit {
                recovery.flush().unwrap();
                let snapshot: thinoda_platform::recovery::Snapshot =
                    serde_json::from_slice(&std::fs::read(&snapshot_path).unwrap()).unwrap();
                assert_eq!(snapshot.workbook.active().root_topic().title, "After");
            } else {
                app.docs[0].snapshot(&recovery);
                assert!(!snapshot_path.exists());
            }
            recovery.finish().unwrap();
        }
    }

    #[test]
    fn failed_background_autosave_waits_an_interval_instead_of_rewriting_each_frame() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auto.thinoda");
        let mut app = ThinodaApp::headless();
        app.settings.autosave = true;
        app.settings.autosave_interval_secs = 5;
        app.settings.backup_on_save = true;
        app.set_file_work_mode(FileWorkMode::Background);
        let doc = app.doc_mut().unwrap();
        doc.save_to(&path, false).unwrap();
        // A directory where the backup must go makes every write fail.
        let backup = thinoda_platform::fs::backup_path(&path);
        std::fs::create_dir(&backup).unwrap();
        std::fs::write(backup.join("keep"), b"blocker").unwrap();
        let root = doc.doc.sheet().root;
        doc.doc
            .edit_sheet("Title", |sheet| sheet.set_title(root, "Unsaved"))
            .unwrap();
        let ctx = egui::Context::default();
        let frame = |app: &mut ThinodaApp| {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.housekeeping(ui.ctx()));
            output.textures_delta.set.clear();
            output.textures_delta.free.clear();
        };
        frame(&mut app);
        app.docs[0].backdate_change(60);
        let deadline = Instant::now() + Duration::from_secs(20);
        while failures(&app) == 0 {
            assert!(Instant::now() < deadline, "autosave never completed");
            frame(&mut app);
            std::thread::sleep(Duration::from_millis(2));
        }
        for _ in 0..50 {
            frame(&mut app);
            assert!(
                app.file_activity().is_empty(),
                "no new write before the interval"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(failures(&app), 1);
        assert!(app.docs[0].is_dirty());
        assert!(!app.docs[0].autosave_due());
    }
}
