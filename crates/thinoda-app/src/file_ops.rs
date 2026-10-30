//! Document open and save work that keeps the UI painting.
//!
//! Importing or writing a large document (for example a FIM model) can take a while. The
//! interactive app runs that work on a worker thread and paints a busy indicator with the
//! file name until the result is applied on the UI thread, like the startup loader does.
//! Without worker threads (browser build) the work runs on the UI thread, but only after
//! the indicator was presented, so it stays visible while the page waits.
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
        mpsc::{self, Receiver, TryRecvError},
    },
};

use egui::{Context, ViewportId};
use thinoda_io::Format;
use thinoda_platform::fs::FileFingerprint;
use thinoda_ui::tokens::Palette;

use crate::{ThinodaApp, doc::OpenDoc, i18n::t, startup::display_name};

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
}

type Job<R> = Box<dyn FnOnce() -> Result<R, String> + Send>;

enum Work<R> {
    /// Requested but not started yet (starting needs the frame context).
    Pending(Option<Job<R>>),
    Thread(Receiver<Result<R, String>>),
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
        if mode != FileWorkMode::Background || cfg!(target_arch = "wasm32") {
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
                    let _ = tx.send(job());
                    ctx.request_repaint_of(ViewportId::ROOT);
                }
            });
        *self = match spawned {
            Ok(_) => Self::Thread(rx),
            Err(error) => {
                log::warn!("could not start the file worker thread, using the UI thread: {error}");
                Self::Deferred(slot.lock().unwrap_or_else(PoisonError::into_inner).take())
            }
        };
    }

    fn deferred(&self) -> bool {
        matches!(self, Self::Deferred(_))
    }

    /// The finished result. Deferred work only runs when blocking the UI is acceptable.
    fn poll(&mut self, may_block: bool, stopped: &str) -> Option<Result<R, String>> {
        match self {
            Self::Pending(_) => None,
            Self::Thread(rx) => match rx.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err(stopped.to_owned())),
            },
            Self::Deferred(job) => may_block.then(|| {
                job.take()
                    .map_or_else(|| Err(stopped.to_owned()), |job| job())
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
    work: Work<thinoda_io::ImportReport>,
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
    work: Work<Option<FileFingerprint>>,
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
            busy.push(FileBusy::Opening {
                index: self.opened + 1,
                total: self.opened + usize::from(self.opening.is_some()) + self.queued.len(),
                name: display_name(path),
            });
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

    /// Opens `path` for a UI request: queued behind the busy indicator unless file work
    /// runs inline. Already open or queued files are not opened twice.
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
        debug_assert!(self.file_ops.save.is_none());
        let doc = &mut self.docs[index];
        let format = doc.prepare_save(target)?;
        let workbook = doc.doc.workbook().clone();
        let path = target.to_path_buf();
        let job: Job<Option<FileFingerprint>> =
            Box::new(move || crate::doc::write_workbook(&workbook, format, &path, backup));
        self.file_ops.save = Some(Saving {
            key: doc.key.clone(),
            target: target.to_path_buf(),
            format,
            state: doc.doc.state_id(),
            deliver,
            autosave,
            work: Work::new(job),
        });
        Ok(())
    }

    /// Starts requested work and applies finished results; called once per frame.
    pub(crate) fn poll_file_ops(&mut self, ctx: &Context) {
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
                self.add_opened(
                    &path,
                    result.and_then(|report| OpenDoc::from_import_report(&path, report)),
                );
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
            let job: Job<thinoda_io::ImportReport> =
                Box::new(move || crate::doc::import_report(&source));
            self.file_ops.opening = Some(Opening {
                path,
                viewport,
                work: Work::new(job),
            });
            return;
        }
    }

    fn finish_file_save(
        &mut self,
        ctx: &Context,
        save: Saving,
        result: Result<Option<FileFingerprint>, String>,
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
                        let _ = recovery.remove(&doc.key);
                    }
                }
                if !save.autosave {
                    self.settings.add_recent(&save.target);
                    self.save_settings();
                    self.toasts
                        .success(format!("{} {}", t("Saved to"), save.target.display()));
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
                self.toasts
                    .error(format!("{}: {error}", t("Could not save")));
                false
            }
        };
        if !save.autosave && !self.resolve_save_all(ctx, &save.key, saved) {
            self.resolve_save_close_tab(&save.key, saved);
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
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .fill(palette.surface)
                    .stroke(egui::Stroke::new(1.0, palette.border))
                    .show(ui, |ui| {
                        ui.set_max_width(360.0);
                        for item in &busy {
                            match item {
                                FileBusy::Saving { name } => {
                                    ui.horizontal(|ui| {
                                        ui.add(egui::Spinner::new());
                                        ui.add(
                                            egui::Label::new(format!(
                                                "{} {name}",
                                                t("Saving file:")
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
                    });
            });
        ctx.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::FileWorkMode;
    use crate::ThinodaApp;

    fn failures(app: &ThinodaApp) -> usize {
        app.toasts
            .texts()
            .filter(|text| text.starts_with("Could not save"))
            .count()
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
