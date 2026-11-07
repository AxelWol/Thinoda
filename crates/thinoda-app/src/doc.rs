//! Open documents: file I/O, dirty tracking, conflict detection and recovery snapshots.

use std::path::{Path, PathBuf};
use thinoda_platform::fs as pfs;
use web_time::Instant;

use thinoda_canvas::MapView;
use thinoda_core::{CanvasObjectId, CanvasObjectKind, Document, TopicId, Workbook};
pub(crate) use thinoda_io::import_read_limit;
use thinoda_io::{ExportOptions, Format};
use thinoda_platform::{
    fs::{FileFingerprint, atomic_write, fingerprint},
    recovery::{RecoveryManager, Snapshot, SnapshotMeta},
    storage::{CancelToken, ExpectedRevision, Revision, StorageError, StorageId, StorageProvider},
};

use crate::i18n::t;

/// Formats that can be written back without losing Thinoda features.
pub fn is_native(format: Format) -> bool {
    matches!(format, Format::Thinoda | Format::XMind)
}

/// Formats a document can be saved to; non-native targets go through a loss preflight.
pub fn is_save_target(format: Format) -> bool {
    is_native(format) || matches!(format, Format::MindManager | Format::DrawIo)
}

#[cfg(any(test, target_arch = "wasm32"))]
pub(crate) fn oversized_import_error(path: &Path) -> Option<thinoda_io::IoError> {
    let limit = import_read_limit(path)?;
    thinoda_io::validate_import_size(path, limit as u64 + 1).err()
}

pub(crate) fn import_size_error(path: &Path) -> String {
    if matches!(
        Format::from_path(path),
        Some(Format::MindManager | Format::MindManagerXml)
    ) {
        t("Could not read this MindManager file. It may be invalid, encrypted, too large or unsafe.")
            .into()
    } else {
        t(crate::file_ops::DOCUMENT_TOO_LARGE).into()
    }
}

#[cfg(any(test, target_arch = "wasm32"))]
pub(crate) fn vfs_import_error(path: &Path, error: std::io::Error) -> thinoda_io::IoError {
    if thinoda_platform::vfs::is_read_limit_exceeded(&error)
        && let Some(error) = oversized_import_error(path)
    {
        return error;
    }
    error.into()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InspectorField {
    Hyperlink,
    Notes,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InspectorEdit {
    pub buffer_id: Option<egui::Id>,
    pub sheet_index: usize,
    pub sheet_root: TopicId,
    pub topic: TopicId,
    pub field: InspectorField,
    pub original: Option<thinoda_core::notes::NoteContent>,
    pub format_notes: bool,
    pub text: String,
}

#[derive(Clone, Debug)]
pub(crate) struct FimEdit {
    pub sheet: usize,
    pub root: TopicId,
    pub state: u64,
    pub change: thinoda_core::fim::FimChange,
}

impl FimEdit {
    fn apply(&self, document: &mut Document) -> Result<(), String> {
        if document.state_id() != self.state {
            return Err("FIM source changed while editing; draft retained".into());
        }
        document
            .edit_book("FIM data", |book| {
                let sheet = book
                    .sheets
                    .get_mut(self.sheet)
                    .filter(|sheet| sheet.root == self.root)
                    .ok_or_else(|| {
                        thinoda_core::EditError::Invalid(
                            "FIM source sheet changed; draft retained".into(),
                        )
                    })?;
                sheet
                    .apply_fim_change(self.change.clone())
                    .map_err(|error| thinoda_core::EditError::Invalid(error.to_string()))
            })
            .map_err(|error| error.to_string())
    }
}

impl InspectorEdit {
    fn value(&self) -> Option<&str> {
        match self.field {
            InspectorField::Hyperlink => (!self.text.trim().is_empty()).then(|| self.text.trim()),
            InspectorField::Notes => (!self.text.trim().is_empty()).then_some(self.text.as_str()),
        }
    }

    fn apply(&self, book: &mut Workbook) -> thinoda_core::EditResult<()> {
        let sheet = book
            .sheets
            .get_mut(self.sheet_index)
            .filter(|sheet| sheet.root == self.sheet_root)
            .ok_or_else(|| {
                thinoda_core::EditError::Invalid("Inspector source sheet no longer exists".into())
            })?;
        self.apply_to_sheet(sheet)
    }

    fn apply_to_sheet(&self, sheet: &mut thinoda_core::Sheet) -> thinoda_core::EditResult<()> {
        if sheet.root != self.sheet_root {
            return Err(thinoda_core::EditError::Invalid(
                "Inspector source sheet changed".into(),
            ));
        }
        let topic = sheet.topic(self.topic).ok_or_else(|| {
            thinoda_core::EditError::Invalid("Inspector source topic no longer exists".into())
        })?;
        let current = match self.field {
            InspectorField::Hyperlink => topic.hyperlink.clone().map(Into::into),
            InspectorField::Notes => topic.notes.clone(),
        };
        let value = self.value();
        let format_changed = self.format_notes
            && value.is_some()
            && current.as_ref().is_none_or(|note| !note.is_markdown());
        if current.as_deref() == value && !format_changed {
            return Ok(());
        }
        if current != self.original {
            return Err(thinoda_core::EditError::Invalid(
                "Inspector source changed while editing; draft retained".into(),
            ));
        }
        match self.field {
            InspectorField::Hyperlink => sheet.set_hyperlink(self.topic, value.map(str::to_owned)),
            InspectorField::Notes if self.format_notes => {
                sheet.set_markdown_notes(self.topic, value.map(str::to_owned))
            }
            InspectorField::Notes => sheet.set_notes(self.topic, value.map(str::to_owned)),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct InkSnapshotVersion {
    gesture: u64,
    feedback: u64,
    source_current: bool,
    retry_tolerance_bits: u32,
    retry_smooth: bool,
}

/// One open document (a tab).
pub struct OpenDoc {
    /// App-local chat binding, never serialized into the workbook or recovery.
    pub(crate) chat_identity: Option<thinoda_platform::chat_history::Binding>,
    /// Stable key used for recovery snapshots.
    pub key: String,
    pub doc: Document,
    pub view: MapView,
    pub path: Option<PathBuf>,
    /// Conversion-only source: never a writable document destination.
    pub import_source: Option<PathBuf>,
    /// Format the document was loaded from / is saved as.
    pub format: Format,
    pub fingerprint: Option<FileFingerprint>,
    pub(crate) storage_revision: Option<Revision>,
    pub(crate) externally_changed: bool,
    snapshot_state: u64,
    snapshot_dirty: bool,
    pub(crate) recovery_snapshot_failed: bool,
    pub(crate) recovery_cleanup_failed: bool,
    last_change: Instant,
    last_seen_state: u64,
    /// Set when the user accepted that the on-disk file may be overwritten.
    pub ignore_conflict: bool,
    pub(crate) inspector_edit: Option<InspectorEdit>,
    pub(crate) fim_edit: Option<FimEdit>,
    pub(crate) ink_feedback: crate::ink_ui::InkFeedback,
    snapshot_inspector_edit: Option<InspectorEdit>,
    last_seen_inspector_edit: Option<InspectorEdit>,
    /// After a failed autosave, the next automatic attempt waits until then.
    pub(crate) autosave_retry_at: Option<Instant>,
    snapshot_title: Option<(TopicId, String, String)>,
    snapshot_board_label: Option<(CanvasObjectId, String, String)>,
    last_seen_board_label: Option<(CanvasObjectId, String, String)>,
    snapshot_saw_ink: bool,
    snapshot_ink_version: Option<InkSnapshotVersion>,
    last_ink_snapshot_attempt: Option<Instant>,
    fim_draft_revision: u64,
    snapshot_fim_revision: u64,
}

impl OpenDoc {
    pub fn new(workbook: Workbook) -> Self {
        let doc = Document::new(workbook);
        let mut view = MapView::default();
        if let Some(root) = doc.sheet().primary_content_root() {
            view.select_only(root);
        }
        let state = doc.state_id();
        Self {
            chat_identity: None,
            key: uuid::Uuid::new_v4().simple().to_string(),
            doc,
            view,
            path: None,
            import_source: None,
            format: Format::Thinoda,
            fingerprint: None,
            storage_revision: None,
            externally_changed: false,
            snapshot_state: state,
            snapshot_dirty: false,
            recovery_snapshot_failed: false,
            recovery_cleanup_failed: false,
            last_change: Instant::now(),
            last_seen_state: state,
            ignore_conflict: false,
            inspector_edit: None,
            fim_edit: None,
            ink_feedback: crate::ink_ui::InkFeedback::default(),
            snapshot_inspector_edit: None,
            last_seen_inspector_edit: None,
            autosave_retry_at: None,
            snapshot_title: None,
            snapshot_board_label: None,
            last_seen_board_label: None,
            snapshot_saw_ink: false,
            snapshot_ink_version: None,
            last_ink_snapshot_attempt: None,
            fim_draft_revision: 0,
            snapshot_fim_revision: 0,
        }
    }

    /// Opens a file in any importable format.
    pub fn open(path: &Path) -> Result<Self, String> {
        let (doc, warnings) = Self::open_with_report(path)?;
        if !warnings.is_empty() {
            return Err(t("Import requires confirmation").into());
        }
        Ok(doc)
    }

    pub fn open_with_report(path: &Path) -> Result<(Self, Vec<thinoda_io::ImportWarning>), String> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let loaded = run_storage(load_workbook_async(path, CancelToken::default()))
                .map_err(|error| crate::file_ops::storage_error(&error))?;
            Self::from_storage_report(path, loaded)
        }
        #[cfg(target_arch = "wasm32")]
        {
            if crate::files::is_durable_document(path) {
                return Err(t("Use the asynchronous file workflow for browser documents.").into());
            }
            let report = import_report(path)?;
            Self::from_import_report(path, report)
        }
    }

    /// Wraps a report imported on the UI or startup loader thread.
    pub fn from_import_report(
        path: &Path,
        report: thinoda_io::ImportReport,
    ) -> Result<(Self, Vec<thinoda_io::ImportWarning>), String> {
        let format = report.format;
        let mut d = Self::new(report.workbook);
        if is_native(format) {
            d.path = Some(path.to_path_buf());
            d.format = format;
            let observed = (|| {
                let before = fingerprint(path).ok()?;
                let bytes = pfs::read(path).ok()?;
                let current = thinoda_io::import_named_bytes_with_report(path, &bytes).ok()?;
                let after = fingerprint(path).ok()?;
                if before != after || current.workbook != *d.doc.workbook() {
                    return None;
                }
                Some((
                    after,
                    thinoda_platform::chat_history::Binding::from_bytes(path, &bytes).ok(),
                ))
            })();
            if let Some((fingerprint, binding)) = observed {
                d.storage_revision = Some(thinoda_platform::storage::revision_from_fingerprint(
                    &fingerprint,
                ));
                d.fingerprint = Some(fingerprint);
                d.chat_identity = binding;
            }
        } else {
            // Imported from a foreign format: saving asks for a new native file name.
            d.import_source = Some(pfs::canonicalize(path).map_err(|e| e.to_string())?);
            d.doc.mark_dirty();
        }
        Ok((d, report.warnings))
    }

    pub(crate) fn from_storage_report(
        path: &Path,
        loaded: LoadedWorkbook,
    ) -> Result<(Self, Vec<thinoda_io::ImportWarning>), String> {
        let format = loaded.report.format;
        let mut doc = Self::new(loaded.report.workbook);
        if is_native(format) {
            doc.path = Some(path.to_path_buf());
            doc.format = format;
            doc.fingerprint = loaded.fingerprint;
            doc.storage_revision = Some(loaded.revision);
            doc.chat_identity = loaded.chat_identity;
        } else {
            #[cfg(not(target_arch = "wasm32"))]
            let source = pfs::canonicalize(path).map_err(|error| error.to_string())?;
            #[cfg(target_arch = "wasm32")]
            let source = pfs::absolute(path).map_err(|error| error.to_string())?;
            doc.import_source = Some(source);
            doc.doc.mark_dirty();
        }
        Ok((doc, loaded.report.warnings))
    }

    pub fn title(&self) -> String {
        match &self.path {
            Some(p) => p
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            None => {
                let sheet = &self.doc.workbook().sheets[0];
                let title = if sheet.is_anchor(sheet.root) {
                    Some(sheet.title.as_str())
                } else {
                    sheet.topic(sheet.root).map(|topic| topic.title.as_str())
                };
                title
                    .map(|title| title.lines().next().unwrap_or_default().to_string())
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| t("Untitled").to_string())
            }
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.doc.is_dirty()
            || self.has_inspector_changes()
            || self.view.has_title_changes()
            || self.view.has_board_label_changes()
            || self.fim_edit.is_some()
            || self.has_unresolved_ink()
    }

    pub(crate) fn has_unresolved_ink(&self) -> bool {
        self.view.has_pending_ink_gesture() || !self.ink_feedback.is_empty()
    }

    pub(crate) fn ensure_no_pending_ink(&self) -> Result<(), String> {
        if self.has_unresolved_ink() {
            Err(t("Resolve pending ink before continuing. Retry, copy, or discard it.").into())
        } else {
            Ok(())
        }
    }

    pub(crate) fn has_inspector_changes(&self) -> bool {
        self.inspector_edit.as_ref().is_some_and(|edit| {
            edit.value() != edit.original.as_deref()
                || (edit.format_notes
                    && edit.value().is_some()
                    && edit
                        .original
                        .as_ref()
                        .is_none_or(|note| !note.is_markdown()))
        })
    }

    pub(crate) fn commit_inspector_edit(&mut self) -> Result<(), String> {
        if self.inspector_edit.is_some() || self.fim_edit.is_some() {
            self.ensure_no_pending_ink()?;
        }
        if let Some(edit) = self.fim_edit.clone() {
            edit.apply(&mut self.doc)?;
            self.clear_fim_edit();
        }
        if let Some(edit) = self.inspector_edit.clone() {
            let label = match edit.field {
                InspectorField::Hyperlink => "Hyperlink",
                InspectorField::Notes => "Notes",
            };
            if edit.sheet_index == self.doc.active_index() {
                self.doc
                    .edit_sheet(label, |sheet| edit.apply_to_sheet(sheet))
            } else {
                self.doc.edit_book(label, |book| edit.apply(book))
            }
            .map_err(|error| error.to_string())?;
            self.inspector_edit = None;
        }
        Ok(())
    }

    pub(crate) fn set_fim_edit(&mut self, change: thinoda_core::fim::FimChange) {
        if let Some(edit) = &mut self.fim_edit {
            edit.change = change;
        } else {
            self.fim_edit = Some(FimEdit {
                sheet: self.doc.active_index(),
                root: self.doc.sheet().root,
                state: self.doc.state_id(),
                change,
            });
        }
        self.fim_draft_revision = self.fim_draft_revision.wrapping_add(1);
        self.last_change = Instant::now();
    }

    pub(crate) fn clear_fim_edit(&mut self) {
        if self.fim_edit.take().is_some() {
            self.fim_draft_revision = self.fim_draft_revision.wrapping_add(1);
            self.last_change = Instant::now();
        }
    }

    pub(crate) fn flush_inspector_edit(&mut self, ctx: &egui::Context) -> Result<(), String> {
        let buffer_id = self.inspector_edit.as_ref().and_then(|edit| edit.buffer_id);
        self.commit_inspector_edit()?;
        if let Some(id) = buffer_id {
            ctx.data_mut(|data| data.remove::<String>(id));
        }
        Ok(())
    }

    pub(crate) fn commit_canvas_drafts(&mut self) -> Result<(), String> {
        self.ensure_no_pending_ink()?;
        self.view
            .try_commit_edit(&mut self.doc)
            .and_then(|_| self.view.try_commit_board_label(&mut self.doc))
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    pub(crate) fn switch_sheet(&mut self, index: usize) -> Result<bool, String> {
        if index >= self.doc.workbook().sheets.len() {
            return Err(t("Not possible here").into());
        }
        if index == self.doc.active_index() {
            return Ok(false);
        }
        self.commit_canvas_drafts()?;
        self.doc.set_active(index);
        self.view.reset();
        Ok(true)
    }

    /// Call once per frame to track when the document last changed.
    pub fn tick(&mut self) {
        self.snapshot_saw_ink |= self.has_unresolved_ink();
        let s = self.doc.state_id();
        let board_label = self
            .view
            .board_label_draft()
            .map(|(id, original, text)| (id, original.to_owned(), text.to_owned()));
        if s != self.last_seen_state
            || self.inspector_edit != self.last_seen_inspector_edit
            || board_label != self.last_seen_board_label
        {
            self.last_seen_state = s;
            self.last_seen_inspector_edit = self.inspector_edit.clone();
            self.last_seen_board_label = board_label;
            self.last_change = Instant::now();
        }
    }

    /// Whether autosave may try again; a failed attempt waits one more interval.
    pub(crate) fn autosave_due(&self) -> bool {
        self.autosave_retry_at
            .is_none_or(|retry| Instant::now() >= retry)
    }

    /// Defers automatic saving after a failed attempt instead of retrying each frame.
    pub(crate) fn autosave_failed(&mut self, interval: f32) {
        self.autosave_retry_at =
            Some(Instant::now() + std::time::Duration::from_secs_f32(interval));
    }

    #[cfg(test)]
    pub(crate) fn backdate_change(&mut self, secs: u64) {
        let earlier = self
            .last_change
            .checked_sub(std::time::Duration::from_secs(secs));
        self.last_change = earlier.unwrap_or(self.last_change);
    }

    #[cfg(test)]
    pub(crate) fn backdate_ink_recovery_attempt(&mut self, secs: u64) {
        if let Some(attempt) = &mut self.last_ink_snapshot_attempt {
            *attempt = attempt
                .checked_sub(std::time::Duration::from_secs(secs))
                .unwrap_or(*attempt);
        }
    }

    pub fn idle_secs(&self) -> f32 {
        self.last_change.elapsed().as_secs_f32()
    }

    pub(crate) fn recovery_due(&self, interval_secs: f32) -> bool {
        let interval = interval_secs.min(2.0);
        if self.has_unresolved_ink() || self.snapshot_saw_ink {
            self.last_ink_snapshot_attempt
                .is_none_or(|attempt| attempt.elapsed().as_secs_f32() >= interval)
        } else {
            self.idle_secs() >= interval
        }
    }

    /// Whether the file changed or its identity cannot be verified.
    /// Uncertainty requires conflict resolution and must never permit autosave.
    pub fn changed_on_disk(&self) -> bool {
        match (&self.path, &self.fingerprint) {
            (Some(path), Some(fp)) => match fp.changed_since() {
                Ok(changed) => changed,
                Err(error) => {
                    log::warn!(
                        "Could not verify file identity for {}: {error}",
                        path.display()
                    );
                    true
                }
            },
            (Some(path), None) => {
                log::warn!("No saved file identity available for {}", path.display());
                true
            }
            (None, _) => false,
        }
    }

    pub(crate) fn targets_current_file(&self, path: &Path) -> Result<bool, String> {
        self.path.as_ref().map_or(Ok(false), |source| {
            same_destination(source, path).map_err(|error| error.to_string())
        })
    }

    /// Writes the document to `path` atomically.
    pub fn save_to(&mut self, path: &Path, backup: bool) -> Result<(), String> {
        self.save_to_with_backup_directory_and_consent(
            path,
            backup,
            None,
            crate::export_preflight::SaveConsent::default(),
        )
    }

    pub(crate) fn save_to_with_backup_directory_and_consent(
        &mut self,
        path: &Path,
        backup: bool,
        backup_directory: Option<PathBuf>,
        consent: crate::export_preflight::SaveConsent,
    ) -> Result<(), String> {
        #[cfg(target_arch = "wasm32")]
        if crate::files::is_durable_document(path) {
            return Err(t("Use the asynchronous file workflow for browser documents.").into());
        }
        let format = self.prepare_save(path)?;
        let expected = self.expected_revision(path)?;
        let state = self.doc.state_id();
        #[cfg(not(target_arch = "wasm32"))]
        let identity = run_storage(write_workbook_async(
            self.doc.workbook(),
            format,
            path,
            backup,
            expected,
            CancelToken::default(),
            backup_directory,
            consent,
        ))
        .map_err(|error| error.to_string())?;
        #[cfg(target_arch = "wasm32")]
        let identity = {
            let _ = (expected, backup_directory);
            write_workbook(
                self.doc.workbook(),
                format,
                path,
                backup,
                consent.board,
                consent.mindmanager,
            )?
        };
        self.finish_save(path, format, state, identity);
        Ok(())
    }

    pub(crate) fn expected_revision(&self, path: &Path) -> Result<ExpectedRevision, String> {
        if self.ignore_conflict {
            return Ok(ExpectedRevision::Any);
        }
        if self.targets_current_file(path)? {
            return self
                .storage_revision
                .clone()
                .or_else(|| {
                    self.fingerprint
                        .as_ref()
                        .map(thinoda_platform::storage::revision_from_fingerprint)
                })
                .map(ExpectedRevision::Match)
                .ok_or_else(|| t("File changed on disk").to_owned());
        }
        #[cfg(target_arch = "wasm32")]
        if crate::files::is_durable_document(path) {
            // A browser filename prompt does not grant overwrite consent.
            return Ok(ExpectedRevision::Missing);
        }
        match fingerprint(path) {
            Ok(value) => Ok(ExpectedRevision::Match(
                thinoda_platform::storage::revision_from_fingerprint(&value),
            )),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(ExpectedRevision::Missing)
            }
            Err(error) => Err(error.to_string()),
        }
    }

    /// Commits pending drafts and validates `path` as a save destination.
    ///
    /// The committed workbook is what a following [`write_workbook`] must write; its
    /// `state_id` identifies that content for [`Self::finish_save`].
    pub(crate) fn prepare_save(&mut self, path: &Path) -> Result<Format, String> {
        self.commit_canvas_drafts()?;
        self.commit_inspector_edit()?;
        self.ensure_export_destination(path)?;
        let format = Format::from_path(path)
            .filter(|f| is_save_target(*f))
            .ok_or_else(|| {
                t("Choose a .thinoda, .xmind, .mmap or .drawio destination.").to_string()
            })?;
        if format == Format::MindManager && self.doc.workbook().sheets.len() != 1 {
            return Err(t("MindManager saves one sheet. Export the current sheet or save the complete workbook as .thinoda.").into());
        }
        Ok(format)
    }

    /// Records a completed write of the content identified by `state`.
    ///
    /// The document only becomes clean when it still holds exactly that content and
    /// no newer draft exists; later edits stay unsaved.
    pub(crate) fn finish_save(
        &mut self,
        path: &Path,
        format: Format,
        state: u64,
        identity: WrittenWorkbook,
    ) {
        self.chat_identity = identity.chat_identity;
        self.path = Some(path.to_path_buf());
        self.format = format;
        self.fingerprint = identity.fingerprint;
        self.storage_revision = identity.revision;
        self.externally_changed = false;
        self.ignore_conflict = false;
        self.autosave_retry_at = None;
        if self.doc.state_id() == state {
            self.doc.mark_saved();
        }
    }

    /// Writes just the active sheet without marking or retargeting the source document.
    pub fn save_sheet_to(&self, path: &Path, backup: bool) -> Result<(), String> {
        self.save_sheet_to_with_consent(path, backup, false, false)
    }

    pub(crate) fn save_sheet_to_with_consent(
        &self,
        path: &Path,
        backup: bool,
        allow_board_loss: bool,
        allow_mindmanager_loss: bool,
    ) -> Result<(), String> {
        self.ensure_export_destination(path)?;
        let format = Format::from_path(path)
            .filter(|format| is_save_target(*format))
            .ok_or_else(|| {
                t("Choose a .thinoda, .xmind, .mmap or .drawio destination.").to_string()
            })?;
        if self.targets_current_file(path)? {
            return Err("choose a different path to preserve the source workbook".into());
        }
        let draft = self.document_with_drafts()?;
        let source = draft.workbook();
        let workbook = Workbook {
            metadata: source.metadata.clone(),
            ..Workbook::new(source.active().clone())
        };
        crate::export_preflight::require_board_consent(
            &workbook,
            format,
            &ExportOptions::default(),
            allow_board_loss,
        )?;
        let bytes = thinoda_io::export_bytes_with_consent(
            &workbook,
            format,
            &ExportOptions::default(),
            crate::export_preflight::SaveConsent {
                board: allow_board_loss,
                mindmanager: allow_mindmanager_loss,
            }
            .for_io(format),
        )
        .map_err(export_error)?;
        atomic_write(path, &bytes, backup).map_err(|e| e.to_string())
    }

    pub(crate) fn validate_pending_drafts(&self) -> Result<(), String> {
        if self.view.has_title_changes() || self.view.has_board_label_changes() {
            self.document_with_drafts().map(|_| ())
        } else {
            Ok(())
        }
    }

    pub(crate) fn document_with_drafts(&self) -> Result<Document, String> {
        self.ensure_no_pending_ink()?;
        self.document_with_drafts_for_recovery()
    }

    fn document_with_drafts_for_recovery(&self) -> Result<Document, String> {
        let board_label = if self.view.has_board_label_changes() {
            self.view
                .validated_board_label_draft(&self.doc)
                .map_err(|error| error.to_string())?
        } else {
            None
        };
        let mut document = self.doc.clone();
        if let Some(edit) = &self.fim_edit {
            edit.apply(&mut document)?;
        }
        if let Some((id, original, text)) = self.view.title_draft()
            && original != text
        {
            document
                .edit_sheet("Pending title", |sheet| {
                    if sheet.topic(id).is_none_or(|topic| topic.title != original) {
                        return Err(thinoda_core::EditError::Invalid(
                            "Title source changed; draft retained".into(),
                        ));
                    }
                    sheet.set_title(id, text)
                })
                .map_err(|error| error.to_string())?;
        }
        if let Some((id, _, text)) = board_label {
            document
                .update_canvas_object("Pending board label", id, |object| match &mut object.kind {
                    CanvasObjectKind::Shape { label, .. } => *label = text.to_owned(),
                    CanvasObjectKind::Connector(connector) => connector.label = text.to_owned(),
                    CanvasObjectKind::Ink(_) => {}
                })
                .map_err(|error| error.to_string())?;
        }
        if let Some(edit) = &self.inspector_edit {
            document
                .edit_book("Pending inspector", |book| edit.apply(book))
                .map_err(|error| error.to_string())?;
        }
        Ok(document)
    }

    pub(crate) fn has_pending_drafts(&self) -> bool {
        self.view.has_title_changes()
            || self.view.has_board_label_changes()
            || self.inspector_edit.is_some()
            || self.fim_edit.is_some()
            || self.has_unresolved_ink()
    }

    pub(crate) fn reload_storage(&mut self, loaded: LoadedWorkbook) -> Result<(), String> {
        if self.has_pending_drafts() {
            return Err(t("Not possible here").into());
        }
        if !loaded.report.warnings.is_empty() {
            return Err(t("Import requires confirmation").into());
        }
        self.doc
            .reload(loaded.report.workbook)
            .map_err(|error| error.to_string())?;
        self.format = loaded.report.format;
        self.storage_revision = Some(loaded.revision);
        self.fingerprint = loaded.fingerprint;
        self.chat_identity = loaded.chat_identity;
        self.externally_changed = false;
        self.ignore_conflict = false;
        Ok(())
    }

    pub fn ensure_export_destination(&self, path: &Path) -> Result<(), String> {
        if let Some(source) = &self.import_source
            && same_destination(source, path).map_err(|e| e.to_string())?
        {
            return Err(
                t("Choose a different destination to preserve the imported source.").into(),
            );
        }
        Ok(())
    }

    fn record_recovery_snapshot_failure(&mut self, error: impl std::fmt::Display) {
        if !self.recovery_snapshot_failed {
            log::warn!("recovery snapshot failed; draft retained: {error}");
        }
        self.recovery_snapshot_failed = true;
    }

    pub(crate) fn cleanup_recovery(&mut self, recovery: &RecoveryManager) -> bool {
        debug_assert!(!self.is_dirty());
        match recovery.remove(&self.key) {
            Ok(()) => {
                self.recovery_cleanup_failed = false;
                true
            }
            Err(error) => {
                if !self.recovery_cleanup_failed {
                    log::warn!("recovery snapshot cleanup failed: {error}");
                }
                self.recovery_cleanup_failed = true;
                false
            }
        }
    }

    /// Queues a crash-recovery snapshot if the document changed since the last one.
    pub fn snapshot(&mut self, recovery: &RecoveryManager) {
        let ink_version = self.has_unresolved_ink().then(|| InkSnapshotVersion {
            gesture: self.view.pending_ink_recovery_revision(),
            feedback: self.ink_feedback.revision(),
            source_current: self.view.pending_ink_source_is_current(&self.doc),
            retry_tolerance_bits: self.view.ink_settings.finish.tolerance.to_bits(),
            retry_smooth: self.view.ink_settings.finish.smooth,
        });
        if ink_version.is_some() || self.snapshot_saw_ink {
            self.last_ink_snapshot_attempt = Some(Instant::now());
        } else {
            self.last_ink_snapshot_attempt = None;
        }
        self.snapshot_saw_ink |= ink_version.is_some();
        let s = self.doc.state_id();
        let title_draft = self
            .view
            .title_draft()
            .map(|(id, original, text)| (id, original.to_owned(), text.to_owned()));
        let board_label = self
            .view
            .board_label_draft()
            .map(|(id, original, text)| (id, original.to_owned(), text.to_owned()));
        let dirty = self.is_dirty();
        if s == self.snapshot_state
            && dirty == self.snapshot_dirty
            && self.inspector_edit == self.snapshot_inspector_edit
            && title_draft == self.snapshot_title
            && board_label == self.snapshot_board_label
            && self.fim_draft_revision == self.snapshot_fim_revision
            && ink_version == self.snapshot_ink_version
            && (ink_version.is_some() || !self.snapshot_saw_ink)
            && !self.recovery_cleanup_failed
        {
            self.recovery_snapshot_failed = false;
            return;
        }
        if !dirty {
            self.recovery_snapshot_failed = false;
            if self.cleanup_recovery(recovery) {
                self.snapshot_state = s;
                self.snapshot_dirty = false;
                self.snapshot_inspector_edit = self.inspector_edit.clone();
                self.snapshot_title = title_draft;
                self.snapshot_board_label = board_label;
                self.snapshot_fim_revision = self.fim_draft_revision;
                self.snapshot_saw_ink = false;
                self.snapshot_ink_version = None;
                self.last_ink_snapshot_attempt = None;
            }
            return;
        }
        let meta = SnapshotMeta {
            title: self.title(),
            original_path: self.path.clone(),
            import_source: self.import_source.clone(),
            dirty: true,
            ..Default::default()
        };
        let snapshot = match self.document_with_drafts_for_recovery() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.record_recovery_snapshot_failure(error);
                return;
            }
        };
        let pending_ink = if ink_version.is_some() {
            let has_other_drafts = self.view.has_title_changes()
                || self.view.has_board_label_changes()
                || self.has_inspector_changes()
                || self.fim_edit.is_some();
            match self.pending_ink_for_recovery(has_other_drafts) {
                Ok(pending_ink) => Some(pending_ink),
                Err(error) => {
                    self.record_recovery_snapshot_failure(error);
                    return;
                }
            }
        } else {
            None
        };
        let bytes = if let Some(pending_ink) = &pending_ink {
            Snapshot::to_bytes_with_pending_ink(meta, snapshot.workbook(), pending_ink)
        } else {
            Snapshot::to_bytes(meta, snapshot.workbook())
        };
        match bytes {
            Ok(bytes) => match recovery.queue_snapshot(&self.key, bytes) {
                Ok(()) => {
                    self.snapshot_state = s;
                    self.snapshot_dirty = true;
                    self.snapshot_inspector_edit = self.inspector_edit.clone();
                    self.snapshot_title = title_draft;
                    self.snapshot_board_label = board_label;
                    self.snapshot_fim_revision = self.fim_draft_revision;
                    self.snapshot_saw_ink = ink_version.is_some();
                    self.snapshot_ink_version = ink_version;
                    if ink_version.is_none() {
                        self.last_ink_snapshot_attempt = None;
                    }
                    self.recovery_snapshot_failed = false;
                    self.recovery_cleanup_failed = false;
                }
                Err(error) => self.record_recovery_snapshot_failure(error),
            },
            Err(error) => self.record_recovery_snapshot_failure(error),
        }
    }
}

/// Converts a file between formats (used by the `--convert` CLI).
pub fn convert(input: &Path, output: &Path, opts: &ExportOptions) -> Result<(), String> {
    convert_with_consent(input, output, opts, false)
}

pub fn convert_with_consent(
    input: &Path,
    output: &Path,
    opts: &ExportOptions,
    allow_lossy: bool,
) -> Result<(), String> {
    if same_destination(input, output).map_err(|e| e.to_string())? {
        return Err(t("Choose a different destination to preserve the imported source.").into());
    }
    let report = import_report(input).map_err(|error| format!("{}: {error}", input.display()))?;
    if !report.warnings.is_empty() {
        if !allow_lossy {
            return Err(t("Import requires confirmation. Use --allow-lossy only if you accept omitted content.").into());
        }
        log::warn!(
            "{}",
            t("This import may omit unsupported content. The source file is preserved.")
        );
    }
    let format = Format::from_path(output)
        .ok_or_else(|| format!("unknown output format: {}", output.display()))?;
    if !format.can_export() {
        if format == Format::MindManagerXml {
            return Err(t("Choose a .thinoda, .xmind, .mmap or .drawio destination.").into());
        }
        return Err(format!("{} cannot be exported", format.label()));
    }
    if thinoda_io::symbol_color_loss_with_options(&report.workbook, format, opts)
        && !opts.allow_symbol_color_loss
    {
        return Err(format!(
            "{} (--allow-symbol-color-loss)",
            t(crate::symbols::EXPORT_WARNING)
        ));
    }
    if thinoda_io::note_format_loss(&report.workbook, format, opts) {
        if !opts.allow_note_format_loss {
            return Err(format!(
                "{} (--allow-note-format-loss)",
                t(crate::notes::EXPORT_SOURCE_REQUIRED)
            ));
        }
        log::warn!("{}", t(crate::notes::EXPORT_SOURCE_WARNING));
    }
    let board_losses = crate::export_preflight::export_loss_report(&report.workbook, format, opts);
    if !board_losses.is_empty() {
        let message = crate::export_preflight::loss_message(&board_losses, format);
        if !allow_lossy {
            return Err(format!(
                "{message}\n{}",
                t("Use --allow-lossy only if you accept these changes.")
            ));
        }
        // A synthesized root can contain a user-provided sheet title. Keep it
        // in the interactive refusal, but do not put document content in logs.
        let mut logged_losses = board_losses.clone();
        for loss in &mut logged_losses.losses {
            if let thinoda_io::ExportLoss::SynthesizedRoot { title, .. } = loss {
                *title = "…".into();
            }
        }
        log::warn!(
            "{}",
            crate::export_preflight::loss_message(&logged_losses, format)
        );
    }
    if format == Format::MindManager {
        log::warn!(
            "{}",
            t(
                "Only the current sheet is exported. Colors, assets, relationships, advanced layout and unsupported data may be omitted. The source workbook is unchanged."
            )
        );
    }
    if format.is_drawio_image() {
        log::warn!(
            "{} {}",
            t(crate::export_preflight::EDITABLE_IMAGE_SCOPE),
            t(crate::export_preflight::EDITABLE_IMAGE_NOTICE)
        );
    }
    let bytes = thinoda_io::export_bytes_with_consent(&report.workbook, format, opts, allow_lossy)
        .map_err(|error| if matches!(error, thinoda_io::IoError::MindManagerExportNeedsConfirmation) {
            t("MindManager export cannot preserve all content. Use --allow-lossy only if you accept omitted content.").into()
        } else { export_error(error) })?;
    atomic_write(output, &bytes, false).map_err(|e| e.to_string())
}

/// Identities of the bytes actually written, independent of newer editor changes.
pub(crate) struct WrittenWorkbook {
    fingerprint: Option<FileFingerprint>,
    chat_identity: Option<thinoda_platform::chat_history::Binding>,
    revision: Option<Revision>,
}

/// Serializes and atomically writes a workbook prepared by [`OpenDoc::prepare_save`].
/// Runs on the file worker thread; returns the written file's identity.
#[cfg(target_arch = "wasm32")]
pub(crate) fn write_workbook(
    workbook: &Workbook,
    format: Format,
    path: &Path,
    backup: bool,
    allow_board_loss: bool,
    allow_mindmanager_loss: bool,
) -> Result<WrittenWorkbook, String> {
    crate::export_preflight::require_board_consent(
        workbook,
        format,
        &ExportOptions::default(),
        allow_board_loss,
    )?;
    let bytes = thinoda_io::export_bytes_with_consent(
        workbook,
        format,
        &ExportOptions::default(),
        crate::export_preflight::SaveConsent {
            board: allow_board_loss,
            mindmanager: allow_mindmanager_loss,
        }
        .for_io(format),
    )
    .map_err(export_error)?;
    atomic_write(path, &bytes, backup).map_err(|e| e.to_string())?;
    Ok(WrittenWorkbook {
        fingerprint: fingerprint(path).ok(),
        revision: fingerprint(path)
            .ok()
            .as_ref()
            .map(thinoda_platform::storage::revision_from_fingerprint),
        chat_identity: if is_native(format) {
            thinoda_platform::chat_history::Binding::from_bytes(path, &bytes).ok()
        } else {
            None
        },
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn run_storage<T>(
    future: impl std::future::Future<Output = Result<T, StorageError>>,
) -> Result<T, StorageError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(future)
}

// Keep revision, cancellation, backup, and export consent explicit at the save boundary.
#[expect(clippy::too_many_arguments)]
pub(crate) async fn write_workbook_async(
    workbook: &Workbook,
    format: Format,
    path: &Path,
    backup: bool,
    expected: ExpectedRevision,
    cancel: CancelToken,
    backup_directory: Option<PathBuf>,
    consent: crate::export_preflight::SaveConsent,
) -> Result<WrittenWorkbook, StorageError> {
    cancel.check()?;
    crate::export_preflight::require_board_consent(
        workbook,
        format,
        &ExportOptions::default(),
        consent.board,
    )
    .map_err(StorageError::Remote)?;
    let bytes = thinoda_io::export_bytes_with_consent(
        workbook,
        format,
        &ExportOptions::default(),
        consent.for_io(format),
    )
    .map_err(|error| StorageError::Remote(export_error(error)))?;
    let provider = document_provider(path, backup, backup_directory.as_deref());
    #[cfg(target_arch = "wasm32")]
    let overwrite_picked_conflict = matches!(&expected, ExpectedRevision::Any);
    let revision = provider
        .write(&document_id(path), &bytes, expected, cancel.clone())
        .await?;
    #[cfg(target_arch = "wasm32")]
    if crate::files::has_picked_file(path) {
        // A durable copy is not a successful save to the original picked file.
        crate::files::save_back_picked_with_overwrite(
            path,
            &bytes,
            overwrite_picked_conflict,
            cancel,
        )
        .await?;
    }
    let fingerprint = fingerprint(path)
        .ok()
        .filter(|value| thinoda_platform::storage::revision_from_fingerprint(value) == revision);
    Ok(WrittenWorkbook {
        fingerprint,
        revision: Some(revision),
        chat_identity: if is_native(format) {
            thinoda_platform::chat_history::Binding::from_bytes(path, &bytes).ok()
        } else {
            None
        },
    })
}

pub(crate) struct LoadedWorkbook {
    pub(crate) report: thinoda_io::ImportReport,
    fingerprint: Option<FileFingerprint>,
    revision: Revision,
    chat_identity: Option<thinoda_platform::chat_history::Binding>,
}

pub(crate) async fn load_workbook_async(
    path: &Path,
    cancel: CancelToken,
) -> Result<LoadedWorkbook, StorageError> {
    let provider = document_provider(path, false, None);
    let id = document_id(path);
    let read = match import_read_limit(path) {
        Some(limit) => provider
            .read_bounded(&id, limit, cancel.clone())
            .await
            .map_err(|error| match error {
                StorageError::TooLarge
                    if matches!(
                        Format::from_path(path),
                        Some(Format::MindManager | Format::MindManagerXml)
                    ) =>
                {
                    StorageError::Remote(import_size_error(path))
                }
                error => error,
            })?,
        None => provider.read(&id, cancel.clone()).await?,
    };
    cancel.check()?;
    let report = thinoda_io::import_named_bytes_with_report(path, &read.bytes)
        .map_err(|error| StorageError::Remote(import_error(path, error)))?;
    let fingerprint = fingerprint(path).ok().filter(|value| {
        thinoda_platform::storage::revision_from_fingerprint(value) == read.revision
    });
    let chat_identity = if is_native(report.format) {
        thinoda_platform::chat_history::Binding::from_bytes(path, &read.bytes).ok()
    } else {
        None
    };
    Ok(LoadedWorkbook {
        report,
        fingerprint,
        revision: read.revision,
        chat_identity,
    })
}

fn document_id(path: &Path) -> StorageId {
    #[cfg(target_arch = "wasm32")]
    {
        crate::files::document_id(path)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        StorageId::from_path(path)
    }
}

fn document_provider(
    path: &Path,
    backup: bool,
    directory: Option<&Path>,
) -> Box<dyn StorageProvider> {
    #[cfg(target_arch = "wasm32")]
    if crate::files::is_durable_document(path) {
        return Box::new(thinoda_platform::browser_storage::BrowserStorageProvider::new(backup));
    }
    let _ = path;
    let mut provider = thinoda_platform::storage::LocalStorageProvider::new(backup);
    if let Some(directory) = directory {
        provider = provider.with_backup_directory(directory.to_path_buf());
    }
    Box::new(provider)
}

pub(crate) fn import_report(path: &Path) -> Result<thinoda_io::ImportReport, String> {
    #[cfg(not(target_arch = "wasm32"))]
    let report = run_storage(load_workbook_async(path, CancelToken::default()))
        .map(|loaded| loaded.report)
        .map_err(|error| crate::file_ops::storage_error(&error));
    #[cfg(target_arch = "wasm32")]
    let report = match import_read_limit(path) {
        Some(limit) => thinoda_platform::vfs::global().read_bounded(path, limit),
        None => pfs::read(path),
    }
    .map_err(|error| {
        if thinoda_platform::vfs::is_read_limit_exceeded(&error) {
            import_size_error(path)
        } else {
            import_error(path, error.into())
        }
    })
    .and_then(|bytes| {
        thinoda_io::import_named_bytes_with_report(path, &bytes)
            .map_err(|error| import_error(path, error))
    });
    #[cfg(not(target_arch = "wasm32"))]
    return report;
    #[cfg(target_arch = "wasm32")]
    report
}

pub(crate) fn import_error(path: &Path, error: thinoda_io::IoError) -> String {
    match error {
        thinoda_io::IoError::DrawIoHtml(thinoda_io::DrawIoHtmlError::MultipleViewers) => {
            return t(
                "This HTML contains multiple draw.io diagrams. Export each diagram separately.",
            )
            .into();
        }
        thinoda_io::IoError::DrawIoHtml(thinoda_io::DrawIoHtmlError::NoEmbeddedDiagram) => {
            return t(
                "This HTML viewer has no embedded diagram. Remote references are not loaded.",
            )
            .into();
        }
        thinoda_io::IoError::DrawIoHtml(_) => {
            return t("Could not read an embedded draw.io diagram from this HTML. It may be invalid, too large or unsafe.").into();
        }
        _ => {}
    }
    if matches!(error, thinoda_io::IoError::MindManager(_))
        || matches!(
            Format::from_path(path),
            Some(Format::MindManager | Format::MindManagerXml)
        )
    {
        log::warn!("MindManager import failed: {error}");
        t("Could not read this MindManager file. It may be invalid, encrypted, too large or unsafe.").into()
    } else {
        error.to_string()
    }
}

pub(crate) fn export_error(error: thinoda_io::IoError) -> String {
    match error {
        thinoda_io::IoError::EquationNotesUnsupported => t(thinoda_core::equations::EXPORT_ERROR).into(),
        thinoda_io::IoError::NoteFormatLossNeedsConfirmation => t(crate::notes::EXPORT_SOURCE_REQUIRED).into(),
        thinoda_io::IoError::ExcelNote { topic, reason } => {
            let message = match reason {
                thinoda_core::notes::NoteExportError::Limit => crate::notes::EXCEL_LIMIT,
                thinoda_core::notes::NoteExportError::Unsupported => crate::notes::EXCEL_UNSUPPORTED,
            };
            format!("{}: {topic}", t(message))
        }
        thinoda_io::IoError::FreeMindNote { topic, reason } => {
            let message = match reason {
                thinoda_core::notes::NoteExportError::Limit => crate::notes::FREEMIND_LIMIT,
                thinoda_core::notes::NoteExportError::Unsupported => crate::notes::FREEMIND_UNSUPPORTED,
            };
            format!("{}: {topic}", t(message))
        }
        thinoda_io::IoError::WordNote { topic, reason } => {
            let message = match reason {
                thinoda_core::notes::NoteExportError::Limit => crate::notes::WORD_LIMIT,
                thinoda_core::notes::NoteExportError::Unsupported => crate::notes::WORD_UNSUPPORTED,
            };
            format!("{}: {topic}", t(message))
        }
        thinoda_io::IoError::MindManagerExportNeedsConfirmation => t("MindManager export cannot preserve all content. Use Export with confirmation, or save as .thinoda.").into(),
        thinoda_io::IoError::DrawioConnectorAttachmentsNeedConfirmation(count) => {
            let report = crate::export_preflight::LossReport {
                losses: vec![thinoda_io::ExportLoss::ConnectorAttachmentsDetached(count)],
                ..Default::default()
            };
            crate::export_preflight::loss_message(&report, Format::DrawIo)
        }
        thinoda_io::IoError::MindManagerExport(detail) => {
            log::warn!("MindManager export failed: {detail}");
            t("Could not write this MindManager file. The map may be too large or contain invalid data.").into()
        }
        error => error.to_string(),
    }
}

fn same_destination(source: &Path, destination: &Path) -> std::io::Result<bool> {
    if pfs::absolute(source)? == pfs::absolute(destination)? {
        return Ok(true);
    }
    let canonical = |path: &Path| match pfs::canonicalize(path) {
        Ok(path) => Ok(Some(path)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    };
    match (canonical(source)?, canonical(destination)?) {
        (Some(source), Some(destination)) => Ok(source == destination),
        _ => Ok(false),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn document_read_limits_match_import_format_policy() {
        for name in [
            "map.mmap",
            "map.XMMAP",
            "map.textbundle",
            "map.textpack",
            "map.docx",
            "map.xml",
            "map.unknown",
        ] {
            let path = Path::new(name);
            assert_eq!(import_read_limit(path), Some(64 * 1024 * 1024));
            assert!(thinoda_io::validate_import_size(path, 64 * 1024 * 1024).is_ok());
            assert!(thinoda_io::validate_import_size(path, 64 * 1024 * 1024 + 1).is_err());
        }
        for name in ["map.xdf2", "map.XDF3"] {
            let path = Path::new(name);
            assert_eq!(
                import_read_limit(path),
                Some(thinoda_io::fim::MAX_XML_BYTES)
            );
            assert!(
                thinoda_io::validate_import_size(path, thinoda_io::fim::MAX_XML_BYTES as u64)
                    .is_ok()
            );
            assert!(
                thinoda_io::validate_import_size(path, thinoda_io::fim::MAX_XML_BYTES as u64 + 1)
                    .is_err()
            );
        }
        for name in ["map.thinoda", "map.xmind", "map.mm", "map.opml", "map.md"] {
            assert_eq!(import_read_limit(Path::new(name)), None);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn loading_rejects_oversized_documents_before_import_and_honors_cancellation() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["large.mmap", "large.xdf3", "large.xml", "large.unknown"] {
            let path = dir.path().join(name);
            let size = import_read_limit(&path).unwrap() as u64 + 1;
            std::fs::File::create(&path).unwrap().set_len(size).unwrap();
            let cancelled = CancelToken::default();
            cancelled.cancel();
            assert!(matches!(
                run_storage(load_workbook_async(&path, cancelled)),
                Err(StorageError::Cancelled)
            ));
            let error = run_storage(load_workbook_async(&path, CancelToken::default()))
                .err()
                .expect("oversized document must be rejected");
            if name == "large.mmap" {
                assert!(
                    matches!(error, StorageError::Remote(message) if message == import_size_error(&path))
                );
            } else {
                assert!(matches!(error, StorageError::TooLarge));
                assert_eq!(
                    crate::file_ops::storage_error(&error),
                    t(crate::file_ops::DOCUMENT_TOO_LARGE)
                );
            }
            assert_eq!(std::fs::metadata(path).unwrap().len(), size);
        }
    }

    #[test]
    fn bounded_vfs_errors_preserve_import_messages_without_masking_corruption() {
        let vfs = thinoda_platform::vfs::Vfs::memory();
        let path = Path::new("/Documents/map.xml");
        vfs.write(path, b"12345", 1).unwrap();
        let error = vfs.read_bounded(path, 4).unwrap_err();
        assert_eq!(
            vfs_import_error(path, error).to_string(),
            "invalid document: import archive exceeds size limit"
        );
        let corrupt = std::io::Error::new(std::io::ErrorKind::InvalidData, "corrupt storage");
        assert!(matches!(
            vfs_import_error(path, corrupt),
            thinoda_io::IoError::Io(error) if error.to_string() == "corrupt storage"
        ));
    }

    fn incoming_workbook() -> LoadedWorkbook {
        let bytes = thinoda_io::export_bytes(
            &Workbook::default(),
            Format::Thinoda,
            &ExportOptions::default(),
        )
        .unwrap();
        LoadedWorkbook {
            report: thinoda_io::import_bytes_with_report(Format::Thinoda, &bytes).unwrap(),
            fingerprint: None,
            revision: Revision("incoming".into()),
            chat_identity: None,
        }
    }

    fn pending_board_label() -> (Harness<'static, OpenDoc>, CanvasObjectId) {
        use thinoda_core::{CanvasObject, Rect, ShapeKind, Sheet, SheetRootMode, Vec2};

        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let mut object = CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_center_size(Vec2::ZERO, Vec2::new(120.0, 80.0)),
        );
        if let CanvasObjectKind::Shape { label, .. } = &mut object.kind {
            *label = "Original".into();
        }
        let id = sheet.add_canvas_object(object).unwrap();
        let mut workbook = Workbook::new(sheet);
        workbook.sheets.push(Sheet::new("Other", "Other"));
        let mut doc = OpenDoc::new(workbook);
        doc.doc.mark_saved();
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1000.0, 800.0))
            .build_ui_state(
                |ui, doc: &mut OpenDoc| {
                    thinoda_canvas::MapCanvas::show(ui, &mut doc.doc, &mut doc.view);
                },
                doc,
            );
        harness.run_steps(3);
        harness.get_by_label("Rectangle: Original").focus();
        harness.run_steps(2);
        harness.key_press(egui::Key::F2);
        harness.run_steps(2);
        harness
            .get_by_label("Board object label")
            .type_text("Pending label");
        harness.run_steps(2);
        assert!(harness.state().view.has_board_label_changes());
        (harness, id)
    }

    #[test]
    fn board_label_drafts_survive_snapshot_sheet_export_and_switch() {
        let (mut harness, id) = pending_board_label();
        let doc = harness.state_mut();
        assert!(doc.is_dirty());
        let draft = doc.document_with_drafts().unwrap();
        assert_eq!(
            draft.sheet().canvas.object(id).unwrap().label(),
            "Pending label"
        );
        assert_eq!(
            doc.doc.sheet().canvas.object(id).unwrap().label(),
            "Original"
        );

        let dir = tempfile::tempdir().unwrap();
        let sheet_path = dir.path().join("sheet.thinoda");
        doc.save_sheet_to(&sheet_path, false).unwrap();
        assert_eq!(
            OpenDoc::open(&sheet_path)
                .unwrap()
                .doc
                .sheet()
                .canvas
                .object(id)
                .unwrap()
                .label(),
            "Pending label"
        );
        assert!(doc.view.has_board_label_changes());

        let paths = thinoda_platform::Paths::from_root(dir.path().join("profile"));
        let recovery = RecoveryManager::start(&paths).unwrap();
        doc.snapshot(&recovery);
        recovery.suspend().unwrap();
        let orphans = RecoveryManager::find_orphans(&paths);
        assert_eq!(orphans.len(), 1);
        assert_eq!(
            orphans[0].docs[0]
                .load()
                .unwrap()
                .active()
                .canvas
                .object(id)
                .unwrap()
                .label(),
            "Pending label"
        );
        drop(orphans);
        recovery.resume().unwrap();
        recovery.finish().unwrap();

        assert!(doc.switch_sheet(1).unwrap());
        assert_eq!(
            doc.doc.workbook().sheets[0]
                .canvas
                .object(id)
                .unwrap()
                .label(),
            "Pending label"
        );
        assert!(doc.is_dirty());
        assert!(doc.doc.undo());
        assert_eq!(
            doc.doc.workbook().sheets[0]
                .canvas
                .object(id)
                .unwrap()
                .label(),
            "Original"
        );
    }

    #[test]
    fn stale_board_label_draft_is_retained_and_blocks_sheet_switch() {
        let (mut harness, id) = pending_board_label();
        let doc = harness.state_mut();
        let replacement = Document::new(doc.doc.workbook().clone());
        doc.doc = replacement;
        assert!(doc.document_with_drafts().is_err());
        assert!(doc.switch_sheet(1).is_err());
        assert_eq!(doc.doc.active_index(), 0);
        assert_eq!(
            doc.doc.sheet().canvas.object(id).unwrap().label(),
            "Original"
        );
        assert!(doc.view.has_board_label_changes());
    }

    #[test]
    fn pending_board_label_blocks_external_reload() {
        let (mut harness, _) = pending_board_label();
        let doc = harness.state_mut();
        let before = doc.doc.workbook().clone();
        assert!(doc.has_pending_drafts());
        assert!(!doc.doc.is_dirty(), "the draft is not yet committed");
        assert!(doc.reload_storage(incoming_workbook()).is_err());
        assert_eq!(doc.doc.workbook(), &before);
        assert!(doc.view.has_board_label_changes());
    }

    #[test]
    fn unresolved_ink_feedback_blocks_external_reload() {
        use thinoda_canvas::ink_controller::InkControllerError;

        let mut doc = OpenDoc::new(Workbook::default());
        assert!(!doc.has_pending_drafts());
        let state = doc.doc.state_id();
        doc.ink_feedback.record(InkControllerError::GesturePending);
        assert!(doc.has_pending_drafts());
        assert_eq!(doc.doc.state_id(), state, "the ink is not yet committed");
        assert!(doc.reload_storage(incoming_workbook()).is_err());
        assert!(doc.has_unresolved_ink());
        doc.discard_pending_ink();
        assert!(!doc.has_pending_drafts());
    }

    #[test]
    fn hidden_boards_select_content_not_the_anchor() {
        let mut sheet = thinoda_core::Sheet::new("Board title", "");
        sheet.root_mode = thinoda_core::SheetRootMode::Hidden;
        let empty = OpenDoc::new(Workbook::new(sheet.clone()));
        assert!(empty.view.primary().is_none());
        assert_eq!(empty.title(), "Board title");

        sheet.add_floating("Later", thinoda_core::Vec2::new(0.0, 200.0));
        let first = sheet.add_floating("First", thinoda_core::Vec2::new(0.0, 0.0));
        let doc = OpenDoc::new(Workbook::new(sheet));
        assert_eq!(doc.view.primary(), Some(first));
        assert!(!doc.view.selection.contains(&doc.doc.sheet().root));
    }

    #[test]
    fn drawio_html_import_preserves_its_source_and_requires_warning_consent() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("diagram.html");
        let html = include_str!("../../thinoda-io/tests/fixtures/published-rectangle.drawio.html")
            .replace("rounded=0;", "rounded=0;dashed=1;");
        std::fs::write(&path, &html).unwrap();

        assert_eq!(
            OpenDoc::open(&path).err().unwrap(),
            t("Import requires confirmation")
        );
        let (mut document, warnings) = OpenDoc::open_with_report(&path).unwrap();
        assert!(warnings.contains(&thinoda_io::ImportWarning::Formatting));
        assert!(document.path.is_none());
        assert!(document.is_dirty());
        assert!(same_destination(document.import_source.as_deref().unwrap(), &path).unwrap());
        assert_eq!(
            document.save_to(&path, false).unwrap_err(),
            t("Choose a different destination to preserve the imported source.")
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), html);

        let native = directory.path().join("copy.thinoda");
        document.save_to(&native, false).unwrap();
        assert!(native.exists());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), html);
    }

    #[test]
    fn drawio_html_errors_are_actionable_and_localized() {
        let cases = [
            (
                thinoda_io::DrawIoHtmlError::MultipleViewers,
                "This HTML contains multiple draw.io diagrams. Export each diagram separately.",
            ),
            (
                thinoda_io::DrawIoHtmlError::NoEmbeddedDiagram,
                "This HTML viewer has no embedded diagram. Remote references are not loaded.",
            ),
            (
                thinoda_io::DrawIoHtmlError::InvalidViewer,
                "Could not read an embedded draw.io diagram from this HTML. It may be invalid, too large or unsafe.",
            ),
        ];
        for (error, message) in cases {
            assert_eq!(
                import_error(Path::new("diagram.html"), error.into()),
                t(message)
            );
            for language in crate::i18n::Lang::ALL {
                let translation = crate::i18n::t_in(language, message);
                assert!(!translation.trim().is_empty(), "{language:?}: {message}");
                if language != crate::i18n::Lang::En {
                    assert_ne!(translation, message, "{language:?}: {message}");
                }
            }
        }
    }

    // macOS filesystems reject this deliberately invalid UTF-8 filename.
    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn legacy_import_without_chat_binding_retains_revision_and_can_save() {
        use std::os::unix::ffi::OsStringExt;
        let directory = tempfile::tempdir().unwrap();
        let path = directory
            .path()
            .join(std::ffi::OsString::from_vec(b"map-\xff.thinoda".to_vec()));
        let bytes = thinoda_io::export_bytes(
            &Workbook::default(),
            Format::Thinoda,
            &ExportOptions::default(),
        )
        .unwrap();
        std::fs::write(&path, &bytes).unwrap();
        let report = thinoda_io::import_named_bytes_with_report(&path, &bytes).unwrap();
        let (mut document, _) = OpenDoc::from_import_report(&path, report).unwrap();
        assert!(document.chat_identity.is_none());
        assert!(document.storage_revision.is_some());
        assert!(document.fingerprint.is_some());
        assert!(!document.changed_on_disk());
        document.save_to(&path, false).unwrap();
    }

    #[test]
    fn stale_legacy_report_cannot_adopt_the_new_files_revision() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("stale.thinoda");
        let before = thinoda_io::export_bytes(
            &Workbook::default(),
            Format::Thinoda,
            &ExportOptions::default(),
        )
        .unwrap();
        std::fs::write(&path, &before).unwrap();
        let report = thinoda_io::import_named_bytes_with_report(&path, &before).unwrap();
        let mut changed = Workbook::default();
        let root = changed.sheets[0].root;
        changed.sheets[0].set_title(root, "External edit").unwrap();
        let after =
            thinoda_io::export_bytes(&changed, Format::Thinoda, &ExportOptions::default()).unwrap();
        std::fs::write(&path, &after).unwrap();
        let (mut document, _) = OpenDoc::from_import_report(&path, report).unwrap();
        assert!(document.storage_revision.is_none());
        assert!(document.changed_on_disk());
        assert!(document.save_to(&path, false).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), after);
    }

    #[test]
    fn excel_note_export_errors_identify_the_topic() {
        for (reason, message) in [
            (
                thinoda_core::notes::NoteExportError::Limit,
                crate::notes::EXCEL_LIMIT,
            ),
            (
                thinoda_core::notes::NoteExportError::Unsupported,
                crate::notes::EXCEL_UNSUPPORTED,
            ),
        ] {
            assert_eq!(
                export_error(thinoda_io::IoError::ExcelNote {
                    topic: "café 日本語".into(),
                    reason,
                }),
                format!("{}: café 日本語", t(message)),
            );
        }
    }

    #[test]
    fn freemind_note_export_errors_identify_the_topic() {
        for (reason, message) in [
            (
                thinoda_core::notes::NoteExportError::Limit,
                crate::notes::FREEMIND_LIMIT,
            ),
            (
                thinoda_core::notes::NoteExportError::Unsupported,
                crate::notes::FREEMIND_UNSUPPORTED,
            ),
        ] {
            assert_eq!(
                export_error(thinoda_io::IoError::FreeMindNote {
                    topic: "café 日本語".into(),
                    reason
                }),
                format!("{}: café 日本語", t(message)),
            );
        }
    }

    #[test]
    fn word_note_export_errors_identify_the_topic_and_preserve_localized_guidance() {
        for (reason, message) in [
            (
                thinoda_core::notes::NoteExportError::Limit,
                crate::notes::WORD_LIMIT,
            ),
            (
                thinoda_core::notes::NoteExportError::Unsupported,
                crate::notes::WORD_UNSUPPORTED,
            ),
        ] {
            let error = export_error(thinoda_io::IoError::WordNote {
                topic: "café 日本語".into(),
                reason,
            });
            assert_eq!(error, format!("{}: café 日本語", t(message)));
        }
    }

    #[test]
    fn destination_identity_errors_do_not_allow_source_preservation_bypass() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.thinoda");
        let mut doc = OpenDoc::new(Workbook::default());
        doc.save_to(&source, false).unwrap();
        let original = std::fs::read(&source).unwrap();
        let alias = dir.path().join("invalid\0.thinoda");
        assert!(doc.targets_current_file(&alias).is_err());
        assert!(doc.save_sheet_to(&alias, false).is_err());
        doc.import_source = Some(source.clone());
        assert!(doc.ensure_export_destination(&alias).is_err());
        assert_eq!(std::fs::read(&source).unwrap(), original);
    }

    #[test]
    fn sheet_save_rejects_parent_alias_without_altering_source_or_history() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.thinoda");
        let nested = dir.path().join("nested");
        std::fs::create_dir(&nested).unwrap();
        let alias = nested.join("..").join("source.thinoda");
        let mut doc = OpenDoc::new(Workbook::default());
        doc.doc
            .edit_book("Second sheet", |book| {
                book.sheets.push(thinoda_core::Sheet::new("Other", "Other"));
                Ok(())
            })
            .unwrap();
        doc.save_to(&source, false).unwrap();
        let original = std::fs::read(&source).unwrap();
        let revision = doc.doc.state_id();
        assert!(doc.save_sheet_to(&alias, false).is_err());
        assert_eq!(std::fs::read(&source).unwrap(), original);
        assert_eq!(doc.doc.state_id(), revision);
        assert!(!doc.is_dirty());
    }

    #[test]
    fn save_and_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.thinoda");
        let mut d = OpenDoc::new(Workbook::default());
        let root = d.doc.sheet().root;
        d.doc
            .edit_sheet("t", |s| s.set_title(root, "Hello"))
            .unwrap();
        assert!(d.is_dirty());
        d.save_to(&path, false).unwrap();
        assert!(!d.is_dirty());
        assert_eq!(d.title(), "a");
        let e = OpenDoc::open(&path).unwrap();
        assert_eq!(
            e.doc.sheet().topic(e.doc.sheet().root).unwrap().title,
            "Hello"
        );
        assert!(!e.is_dirty());
        assert!(!e.changed_on_disk());
    }

    #[test]
    fn unknown_or_unreadable_file_identity_requires_conflict_resolution() {
        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path().join("maps");
        std::fs::create_dir(&parent).unwrap();
        let path = parent.join("map.thinoda");
        let mut doc = OpenDoc::new(Workbook::default());
        doc.save_to(&path, false).unwrap();
        let fingerprint = doc.fingerprint.take().unwrap();
        assert!(
            doc.changed_on_disk(),
            "an existing document without a baseline cannot be assumed unchanged"
        );
        doc.fingerprint = Some(fingerprint);
        let moved = dir.path().join("moved");
        std::fs::rename(&parent, &moved).unwrap();
        std::fs::write(&parent, b"parent is no longer a directory").unwrap();
        assert!(
            doc.changed_on_disk(),
            "an unreadable identity must not silently permit overwrite/autosave"
        );
    }

    #[test]
    fn formatted_draft_recovery_and_failed_save_preserve_format_and_source() {
        use thinoda_core::notes::NoteContent;
        use thinoda_platform::Paths;

        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::from_root(dir.path().join("profile"));
        let recovery = RecoveryManager::start(&paths).unwrap();
        let mut doc = OpenDoc::new(Workbook::default());
        let root = doc.doc.sheet().root;
        let source = "**café**\r\n\n- 日本語";
        doc.inspector_edit = Some(InspectorEdit {
            buffer_id: None,
            sheet_index: 0,
            sheet_root: root,
            topic: root,
            field: InspectorField::Notes,
            original: None,
            format_notes: true,
            text: source.into(),
        });
        let before = doc.doc.workbook().clone();
        doc.snapshot(&recovery);
        recovery.suspend().unwrap();
        let orphans = RecoveryManager::find_orphans(&paths);
        assert_eq!(orphans.len(), 1);
        let recovered = orphans[0].docs[0].load().unwrap();
        assert_eq!(
            recovered.active().root_topic().notes,
            Some(NoteContent::markdown(source.into()))
        );
        assert_eq!(doc.doc.workbook(), &before);
        assert!(doc.inspector_edit.is_some());
        drop(orphans);
        recovery.resume().unwrap();
        let invalid = dir.path().join("directory.thinoda");
        std::fs::create_dir(&invalid).unwrap();
        assert!(doc.save_to(&invalid, false).is_err());
        assert!(doc.is_dirty());
        assert_eq!(
            doc.doc.sheet().root_topic().notes,
            recovered.active().root_topic().notes
        );
        let path = dir.path().join("saved.thinoda");
        doc.save_to(&path, false).unwrap();
        assert_eq!(
            OpenDoc::open(&path).unwrap().doc.sheet().root_topic().notes,
            recovered.active().root_topic().notes
        );
        recovery.finish().unwrap();
    }

    #[test]
    fn note_format_only_changes_do_not_bypass_draft_conflict_checks() {
        let mut doc = OpenDoc::new(Workbook::default());
        let root = doc.doc.sheet().root;
        doc.doc
            .edit_sheet("Fixture", |sheet| {
                sheet.set_notes(root, Some("**literal**".into()))
            })
            .unwrap();
        doc.inspector_edit = Some(InspectorEdit {
            buffer_id: None,
            sheet_index: 0,
            sheet_root: root,
            topic: root,
            field: InspectorField::Notes,
            original: Some("**literal**".into()),
            format_notes: false,
            text: "My pending edit".into(),
        });
        doc.doc
            .edit_sheet("Other edit", |sheet| {
                sheet.set_markdown_notes(root, Some("**literal**".into()))
            })
            .unwrap();
        let before = doc.doc.workbook().clone();
        assert!(doc.commit_inspector_edit().is_err());
        assert_eq!(doc.doc.workbook(), &before);
        assert_eq!(doc.inspector_edit.as_ref().unwrap().text, "My pending edit");
    }

    #[test]
    fn inspector_drafts_reject_changed_or_missing_sources_without_overwriting_them() {
        for missing in [false, true] {
            let mut doc = OpenDoc::new(Workbook::default());
            let root = doc.doc.sheet().root;
            let child = doc
                .doc
                .edit_sheet("Fixture", |sheet| Ok(sheet.add_child(root, None, "Child")))
                .unwrap();
            doc.inspector_edit = Some(InspectorEdit {
                buffer_id: None,
                sheet_index: 0,
                sheet_root: root,
                topic: child,
                field: InspectorField::Notes,
                original: None,
                format_notes: false,
                text: "Draft to preserve".into(),
            });
            doc.doc
                .edit_sheet("Other change", |sheet| {
                    if missing {
                        sheet.remove_topics(&[child])?;
                        Ok(())
                    } else {
                        sheet.set_notes(child, Some("Newer model value".into()))
                    }
                })
                .unwrap();
            let before = doc.doc.workbook().clone();
            assert!(doc.commit_inspector_edit().is_err());
            assert_eq!(doc.doc.workbook(), &before);
            assert_eq!(
                doc.inspector_edit.as_ref().unwrap().text,
                "Draft to preserve"
            );
        }
    }

    #[test]
    fn inspector_drafts_distinguish_duplicated_sheets_with_shared_topic_ids() {
        let mut doc = OpenDoc::new(Workbook::default());
        doc.doc.duplicate_sheet(0).unwrap();
        let root = doc.doc.sheet().root;
        assert_eq!(doc.doc.workbook().sheets[0].root, root);
        doc.inspector_edit = Some(InspectorEdit {
            buffer_id: None,
            sheet_index: 1,
            sheet_root: root,
            topic: root,
            field: InspectorField::Notes,
            original: None,
            format_notes: false,
            text: "Second sheet only".into(),
        });
        doc.doc.set_active(0);
        doc.commit_inspector_edit().unwrap();
        assert_eq!(doc.doc.workbook().sheets[0].root_topic().notes, None);
        assert_eq!(
            doc.doc.workbook().sheets[1].root_topic().notes.as_deref(),
            Some("Second sheet only")
        );
        assert_eq!(doc.doc.active_index(), 0);
    }

    #[test]
    fn inspector_drafts_survive_failed_save_and_are_included_in_separate_sheet_output() {
        let dir = tempfile::tempdir().unwrap();
        let invalid = dir.path().join("directory.thinoda");
        std::fs::create_dir(&invalid).unwrap();
        let mut doc = OpenDoc::new(Workbook::default());
        let root = doc.doc.sheet().root;
        doc.inspector_edit = Some(InspectorEdit {
            buffer_id: None,
            sheet_index: 0,
            sheet_root: root,
            topic: root,
            field: InspectorField::Hyperlink,
            original: None,
            format_notes: false,
            text: "  https://example.org/  ".into(),
        });
        let output = dir.path().join("separate.thinoda");
        let before = doc.doc.workbook().clone();
        doc.save_sheet_to(&output, false).unwrap();
        assert_eq!(
            OpenDoc::open(&output)
                .unwrap()
                .doc
                .sheet()
                .root_topic()
                .hyperlink
                .as_deref(),
            Some("https://example.org/")
        );
        assert_eq!(doc.doc.workbook(), &before);
        assert!(doc.inspector_edit.is_some());
        assert!(doc.path.is_none());
        assert!(doc.save_to(&invalid, false).is_err());
        assert!(doc.is_dirty());
        assert_eq!(
            doc.doc.sheet().root_topic().hyperlink.as_deref(),
            Some("https://example.org/")
        );
        assert!(doc.doc.undo());
        assert_eq!(doc.doc.sheet().root_topic().hyperlink, None);
    }

    #[test]
    fn untitled_uses_central_topic() {
        let mut d = OpenDoc::new(Workbook::default());
        let root = d.doc.sheet().root;
        d.doc
            .edit_sheet("t", |s| s.set_title(root, "Project X\nsecond line"))
            .unwrap();
        assert_eq!(d.title(), "Project X");
    }

    #[test]
    fn convert_markdown_to_opml() {
        let dir = tempfile::tempdir().unwrap();
        let md = dir.path().join("in.md");
        std::fs::write(&md, "# Root\n\n- A\n  - A1\n- B\n").unwrap();
        let out = dir.path().join("out.opml");
        convert(&md, &out, &ExportOptions::default()).unwrap();
        let text = std::fs::read_to_string(&out).unwrap();
        assert!(text.contains("A1"));
    }
}
