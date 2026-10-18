//! Open documents: file I/O, dirty tracking, conflict detection and recovery snapshots.

use std::{
    path::{Path, PathBuf},
    time::Instant,
};

use thinoda_canvas::MapView;
use thinoda_core::{Document, Workbook};
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
            d.import_source = Some(path.canonicalize().map_err(|e| e.to_string())?);
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
        self.doc.is_dirty()
    }

    /// Call once per frame to track when the document last changed.
    pub fn tick(&mut self) {
        let s = self.doc.state_id();
        if s != self.last_seen_state {
            self.last_seen_state = s;
            self.last_change = Instant::now();
        }
    }

    pub fn idle_secs(&self) -> f32 {
        self.last_change.elapsed().as_secs_f32()
    }

    /// Whether the file on disk changed since it was loaded or last saved.
    pub fn changed_on_disk(&self) -> bool {
        match (&self.path, &self.fingerprint) {
            (Some(_), Some(fp)) => fp.changed_since().unwrap_or(false),
            _ => false,
        }
    }

    /// Writes the document to `path` atomically.
    pub fn save_to(&mut self, path: &Path, backup: bool) -> Result<(), String> {
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
        if let Some(source) = &self.path {
            let same_path = std::path::absolute(source).map_err(|e| e.to_string())?
                == std::path::absolute(path).map_err(|e| e.to_string())?;
            let same_file = if source.try_exists().map_err(|e| e.to_string())?
                && path.try_exists().map_err(|e| e.to_string())?
            {
                source.canonicalize().map_err(|e| e.to_string())?
                    == path.canonicalize().map_err(|e| e.to_string())?
            } else {
                false
            };
            if same_path || same_file {
                return Err("choose a different path to preserve the source workbook".into());
            }
        }
        let workbook = Workbook {
            metadata: self.doc.workbook().metadata.clone(),
            ..Workbook::new(self.doc.sheet().clone())
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
        if s == self.snapshot_state {
            return;
        }
        if !self.is_dirty() {
            let _ = recovery.remove(&self.key);
            self.snapshot_state = s;
            return;
        }
        let meta = SnapshotMeta {
            title: self.title(),
            original_path: self.path.clone(),
            import_source: self.import_source.clone(),
            dirty: true,
            ..Default::default()
        };
        match Snapshot::to_bytes(meta, self.doc.workbook()) {
            Ok(bytes) => {
                if recovery.queue_snapshot(&self.key, bytes).is_ok() {
                    self.snapshot_state = s;
                }
            }
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
            t("MindManager conversion may omit unsupported content. The source file is preserved.")
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
    thinoda_io::import_file_with_report(path).map_err(|error| {
        if matches!(error, thinoda_io::IoError::MindManager(_)) || matches!(Format::from_path(path), Some(Format::MindManager | Format::MindManagerXml)) {
            log::warn!("MindManager import failed: {error}");
            t("Could not read this MindManager file. It may be invalid, encrypted, too large or unsafe.").into()
        } else { error.to_string() }
    })
}

pub(crate) fn export_error(error: thinoda_io::IoError) -> String {
    match error {
        thinoda_io::IoError::MindManagerExportNeedsConfirmation => t("MindManager export cannot preserve all content. Use Export with confirmation, or save as .thinoda.").into(),
        thinoda_io::IoError::MindManagerExport(detail) => {
            log::warn!("MindManager export failed: {detail}");
            t("Could not write this MindManager file. The map may be too large or contain invalid data.").into()
        }
        error => error.to_string(),
    }
}

fn same_destination(source: &Path, destination: &Path) -> std::io::Result<bool> {
    if std::path::absolute(source)? == std::path::absolute(destination)? {
        return Ok(true);
    }
    Ok(source.try_exists()?
        && destination.try_exists()?
        && source.canonicalize()? == destination.canonicalize()?)
}
#[cfg(test)]
mod tests {
    use super::*;

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
