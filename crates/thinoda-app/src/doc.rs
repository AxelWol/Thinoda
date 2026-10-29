//! Open documents: file I/O, dirty tracking, conflict detection and recovery snapshots.

use std::path::{Path, PathBuf};
use thinoda_platform::fs as pfs;
use web_time::Instant;

use thinoda_canvas::MapView;
use thinoda_core::{Document, TopicId, Workbook};
use thinoda_io::{ExportOptions, Format};
use thinoda_platform::{
    fs::{FileFingerprint, atomic_write, fingerprint},
    recovery::{RecoveryManager, Snapshot, SnapshotMeta},
};

use crate::i18n::t;

/// Formats that can be written back without losing Thinoda features.
pub fn is_native(format: Format) -> bool {
    matches!(format, Format::Thinoda | Format::XMind)
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

/// One open document (a tab).
pub struct OpenDoc {
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
    snapshot_state: u64,
    last_change: Instant,
    last_seen_state: u64,
    /// Set when the user accepted that the on-disk file may be overwritten.
    pub ignore_conflict: bool,
    pub(crate) inspector_edit: Option<InspectorEdit>,
    snapshot_inspector_edit: Option<InspectorEdit>,
    last_seen_inspector_edit: Option<InspectorEdit>,
}

impl OpenDoc {
    pub fn new(workbook: Workbook) -> Self {
        let doc = Document::new(workbook);
        let mut view = MapView::default();
        view.select_only(doc.sheet().root);
        let state = doc.state_id();
        Self {
            key: uuid::Uuid::new_v4().simple().to_string(),
            doc,
            view,
            path: None,
            import_source: None,
            format: Format::Thinoda,
            fingerprint: None,
            snapshot_state: state,
            last_change: Instant::now(),
            last_seen_state: state,
            ignore_conflict: false,
            inspector_edit: None,
            snapshot_inspector_edit: None,
            last_seen_inspector_edit: None,
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
        let report = import_report(path)?;
        Self::from_import_report(path, report)
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
            d.fingerprint = fingerprint(path).ok();
        } else {
            // Imported from a foreign format: saving asks for a new native file name.
            d.import_source = Some(pfs::canonicalize(path).map_err(|e| e.to_string())?);
            d.doc.mark_dirty();
        }
        Ok((d, report.warnings))
    }

    pub fn title(&self) -> String {
        match &self.path {
            Some(p) => p
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            None => {
                let root = self.doc.workbook().sheets[0].root;
                self.doc.workbook().sheets[0]
                    .topic(root)
                    .map(|t| t.title.lines().next().unwrap_or_default().to_string())
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| t("Untitled").to_string())
            }
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.doc.is_dirty() || self.has_inspector_changes()
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

    pub(crate) fn flush_inspector_edit(&mut self, ctx: &egui::Context) -> Result<(), String> {
        let buffer_id = self.inspector_edit.as_ref().and_then(|edit| edit.buffer_id);
        self.commit_inspector_edit()?;
        if let Some(id) = buffer_id {
            ctx.data_mut(|data| data.remove::<String>(id));
        }
        Ok(())
    }

    /// Call once per frame to track when the document last changed.
    pub fn tick(&mut self) {
        let s = self.doc.state_id();
        if s != self.last_seen_state || self.inspector_edit != self.last_seen_inspector_edit {
            self.last_seen_state = s;
            self.last_seen_inspector_edit = self.inspector_edit.clone();
            self.last_change = Instant::now();
        }
    }

    pub fn idle_secs(&self) -> f32 {
        self.last_change.elapsed().as_secs_f32()
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
        self.commit_inspector_edit()?;
        self.ensure_export_destination(path)?;
        let format = Format::from_path(path)
            .filter(|f| is_native(*f) || *f == Format::MindManager)
            .ok_or_else(|| t("Choose a .thinoda, .xmind or .mmap destination.").to_string())?;
        if format == Format::MindManager && self.doc.workbook().sheets.len() != 1 {
            return Err(t("MindManager saves one sheet. Export the current sheet or save the complete workbook as .thinoda.").into());
        }
        let bytes =
            thinoda_io::export_bytes(self.doc.workbook(), format, &ExportOptions::default())
                .map_err(export_error)?;
        atomic_write(path, &bytes, backup).map_err(|e| e.to_string())?;
        self.path = Some(path.to_path_buf());
        self.format = format;
        self.fingerprint = fingerprint(path).ok();
        self.ignore_conflict = false;
        self.doc.mark_saved();
        Ok(())
    }

    /// Writes just the active sheet without marking or retargeting the source document.
    pub fn save_sheet_to(&self, path: &Path, backup: bool) -> Result<(), String> {
        self.ensure_export_destination(path)?;
        let format = Format::from_path(path)
            .filter(|format| is_native(*format) || *format == Format::MindManager)
            .ok_or_else(|| t("Choose a .thinoda, .xmind or .mmap destination.").to_string())?;
        if self.targets_current_file(path)? {
            return Err("choose a different path to preserve the source workbook".into());
        }
        let source = if let Some(edit) = &self.inspector_edit {
            let mut source = self.doc.workbook().clone();
            edit.apply(&mut source).map_err(|error| error.to_string())?;
            std::borrow::Cow::Owned(source)
        } else {
            std::borrow::Cow::Borrowed(self.doc.workbook())
        };
        let workbook = Workbook {
            metadata: source.metadata.clone(),
            ..Workbook::new(source.active().clone())
        };
        let bytes = thinoda_io::export_bytes(&workbook, format, &ExportOptions::default())
            .map_err(export_error)?;
        atomic_write(path, &bytes, backup).map_err(|e| e.to_string())
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

    /// Queues a crash-recovery snapshot if the document changed since the last one.
    pub fn snapshot(&mut self, recovery: &RecoveryManager) {
        let s = self.doc.state_id();
        if s == self.snapshot_state && self.inspector_edit == self.snapshot_inspector_edit {
            return;
        }
        if !self.is_dirty() {
            match recovery.remove(&self.key) {
                Ok(()) => {
                    self.snapshot_state = s;
                    self.snapshot_inspector_edit = self.inspector_edit.clone();
                }
                Err(error) => log::warn!("recovery snapshot cleanup failed: {error}"),
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
        let workbook = if let Some(edit) = &self.inspector_edit {
            let mut workbook = self.doc.workbook().clone();
            if let Err(error) = edit.apply(&mut workbook) {
                log::warn!("inspector recovery snapshot failed; draft retained: {error}");
                return;
            }
            std::borrow::Cow::Owned(workbook)
        } else {
            std::borrow::Cow::Borrowed(self.doc.workbook())
        };
        match Snapshot::to_bytes(meta, &workbook) {
            Ok(bytes) => match recovery.queue_snapshot(&self.key, bytes) {
                Ok(()) => {
                    self.snapshot_state = s;
                    self.snapshot_inspector_edit = self.inspector_edit.clone();
                }
                Err(error) => log::warn!("recovery snapshot queue failed: {error}"),
            },
            Err(e) => log::warn!("snapshot failed: {e}"),
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
            return Err(t("Choose a .thinoda, .xmind or .mmap destination.").into());
        }
        return Err(format!("{} cannot be exported", format.label()));
    }
    if thinoda_io::symbol_color_loss(&report.workbook, format) && !opts.allow_symbol_color_loss {
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
    if format == Format::MindManager {
        log::warn!(
            "{}",
            t(
                "Only the current sheet is exported. Colors, assets, relationships, advanced layout and unsupported data may be omitted. The source workbook is unchanged."
            )
        );
    }
    let bytes = thinoda_io::export_bytes_with_consent(&report.workbook, format, opts, allow_lossy)
        .map_err(|error| if matches!(error, thinoda_io::IoError::MindManagerExportNeedsConfirmation) {
            t("MindManager export cannot preserve all content. Use --allow-lossy only if you accept omitted content.").into()
        } else { export_error(error) })?;
    atomic_write(output, &bytes, false).map_err(|e| e.to_string())
}

pub(crate) fn import_report(path: &Path) -> Result<thinoda_io::ImportReport, String> {
    // Natively the reader bounds MindManager input before allocating; the browser
    // stores files in its virtual file system, which import reads from memory.
    #[cfg(not(target_arch = "wasm32"))]
    let report = thinoda_io::import_file_with_report(path);
    #[cfg(target_arch = "wasm32")]
    let report = pfs::read(path)
        .map_err(thinoda_io::IoError::from)
        .and_then(|bytes| thinoda_io::import_named_bytes_with_report(path, &bytes));
    report.map_err(|error| {
        if matches!(error, thinoda_io::IoError::MindManager(_)) || matches!(Format::from_path(path), Some(Format::MindManager | Format::MindManagerXml)) {
            log::warn!("MindManager import failed: {error}");
            t("Could not read this MindManager file. It may be invalid, encrypted, too large or unsafe.").into()
        } else { error.to_string() }
    })
}

pub(crate) fn export_error(error: thinoda_io::IoError) -> String {
    match error {
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
