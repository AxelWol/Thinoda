//! Open documents: file I/O, dirty tracking, conflict detection and recovery snapshots.

use std::{
    path::{Path, PathBuf},
    time::Instant,
};

use ymind_canvas::MapView;
use ymind_core::{Document, Workbook};
use ymind_io::{ExportOptions, Format};
use ymind_platform::{
    fs::{FileFingerprint, atomic_write, fingerprint},
    recovery::{RecoveryManager, Snapshot, SnapshotMeta},
};

use crate::i18n::t;

/// Formats that can be written back without losing yMind features.
pub fn is_native(format: Format) -> bool {
    matches!(format, Format::YMind | Format::XMind)
}

/// One open document (a tab).
pub struct OpenDoc {
    /// Stable key used for recovery snapshots.
    pub key: String,
    pub doc: Document,
    pub view: MapView,
    pub path: Option<PathBuf>,
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
            format: Format::YMind,
            fingerprint: None,
            snapshot_state: state,
            last_change: Instant::now(),
            last_seen_state: state,
            ignore_conflict: false,
        }
    }

    /// Opens a file in any importable format.
    pub fn open(path: &Path) -> Result<Self, String> {
        let workbook = ymind_io::import_file(path).map_err(|e| e.to_string())?;
        let format = Format::from_path(path).unwrap_or(Format::YMind);
        let mut d = Self::new(workbook);
        if is_native(format) {
            d.path = Some(path.to_path_buf());
            d.format = format;
            d.fingerprint = fingerprint(path).ok();
        } else {
            // Imported from a foreign format: saving asks for a new native file name.
            d.doc.mark_dirty();
        }
        Ok(d)
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
        let format = Format::from_path(path)
            .filter(|f| is_native(*f))
            .unwrap_or(Format::YMind);
        let bytes = ymind_io::export_bytes(self.doc.workbook(), format, &ExportOptions::default())
            .map_err(|e| e.to_string())?;
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
        let format = Format::from_path(path)
            .filter(|format| is_native(*format))
            .ok_or("choose a .ymind or .xmind file for a sheet copy")?;
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
        let bytes = ymind_io::export_bytes(&workbook, format, &ExportOptions::default())
            .map_err(|e| e.to_string())?;
        atomic_write(path, &bytes, backup).map_err(|e| e.to_string())
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
    let workbook = ymind_io::import_file(input).map_err(|e| format!("{}: {e}", input.display()))?;
    let format = Format::from_path(output)
        .ok_or_else(|| format!("unknown output format: {}", output.display()))?;
    if !format.can_export() {
        return Err(format!("{} cannot be exported", format.label()));
    }
    let bytes = ymind_io::export_bytes(&workbook, format, opts).map_err(|e| e.to_string())?;
    atomic_write(output, &bytes, false).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_and_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.ymind");
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
