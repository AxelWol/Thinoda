//! Import and export adapters for Thinoda and common mind-map formats.
//!
//! Markdown export uses the selected sheet, or the active sheet when no sheet is selected.
//! Graphical output shares font-measured title layout. PDF keeps vector paths
//! (including outlined text); JPEG always uses an opaque background.
pub mod fonts;
mod formats;
mod mindmanager;
mod office;
pub mod print;
mod render;
mod xmind;

use std::{
    fs,
    io::{Read, Write},
    path::Path,
};
use thinoda_core::Workbook;
use thiserror::Error;

const MAX_ENTRY_SIZE: u64 = 200 * 1024 * 1024;

/// Supported document formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    Thinoda,
    XMind,
    FreeMind,
    Opml,
    Markdown,
    PlainText,
    MindManager,
    MindManagerXml,
    TextBundle,
    Svg,
    Png,
    Jpeg,
    Pdf,
    Word,
    Excel,
    Csv,
}
impl Format {
    pub const IMPORTABLE: &'static [Format] = &[
        Self::Thinoda,
        Self::XMind,
        Self::FreeMind,
        Self::Opml,
        Self::Markdown,
        Self::PlainText,
        Self::MindManager,
        Self::MindManagerXml,
    ];
    pub const EXPORTABLE: &'static [Format] = &[
        Self::Thinoda,
        Self::XMind,
        Self::FreeMind,
        Self::Opml,
        Self::Markdown,
        Self::PlainText,
        Self::MindManager,
        Self::TextBundle,
        Self::Svg,
        Self::Png,
        Self::Jpeg,
        Self::Pdf,
        Self::Word,
        Self::Excel,
        Self::Csv,
    ];
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        Some(match ext.as_str() {
            "thinoda" => Self::Thinoda,
            "xmind" => Self::XMind,
            "mm" => Self::FreeMind,
            "opml" => Self::Opml,
            "md" | "markdown" => Self::Markdown,
            "txt" => Self::PlainText,
            "mmap" => Self::MindManager,
            "xmmap" => Self::MindManagerXml,
            "textpack" | "textbundle" => Self::TextBundle,
            "svg" => Self::Svg,
            "png" => Self::Png,
            "jpg" | "jpeg" => Self::Jpeg,
            "pdf" => Self::Pdf,
            "docx" => Self::Word,
            "xlsx" => Self::Excel,
            "csv" => Self::Csv,
            _ => return None,
        })
    }
    pub fn extensions(&self) -> &'static [&'static str] {
        match self {
            Self::Thinoda => &["thinoda"],
            Self::XMind => &["xmind"],
            Self::FreeMind => &["mm"],
            Self::Opml => &["opml"],
            Self::Markdown => &["md", "markdown"],
            Self::PlainText => &["txt"],
            Self::MindManager => &["mmap"],
            Self::MindManagerXml => &["xmmap"],
            Self::TextBundle => &["textpack", "textbundle"],
            Self::Svg => &["svg"],
            Self::Png => &["png"],
            Self::Jpeg => &["jpg", "jpeg"],
            Self::Pdf => &["pdf"],
            Self::Word => &["docx"],
            Self::Excel => &["xlsx"],
            Self::Csv => &["csv"],
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::Thinoda => "Thinoda",
            Self::XMind => "XMind",
            Self::FreeMind => "FreeMind / Freeplane",
            Self::Opml => "OPML",
            Self::Markdown => "Markdown",
            Self::PlainText => "Plain text",
            Self::MindManager => "MindManager",
            Self::MindManagerXml => "MindManager XML",
            Self::TextBundle => "TextBundle",
            Self::Svg => "SVG",
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::Pdf => "PDF",
            Self::Word => "Word",
            Self::Excel => "Excel",
            Self::Csv => "CSV",
        }
    }
    pub fn can_import(&self) -> bool {
        Self::IMPORTABLE.contains(self)
    }
    pub fn can_export(&self) -> bool {
        Self::EXPORTABLE.contains(self)
    }
}

/// Rendering settings. `sheet: None` selects the active sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportOptions {
    /// Explicit caller acknowledgement for editable formats without symbol-color fields.
    pub allow_symbol_color_loss: bool,
    pub sheet: Option<usize>,
    pub dark: bool,
    pub scale: f32,
    pub transparent: bool,
    pub padding: f32,
}
impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            allow_symbol_color_loss: false,
            sheet: None,
            dark: false,
            scale: 1.0,
            transparent: false,
            padding: 24.0,
        }
    }
}

/// Import/export error.
#[derive(Debug, Error)]
pub enum IoError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("ZIP error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("XML error: {0}")]
    Xml(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),
    #[error("document version {0} is newer than supported")]
    NewerVersion(String),
    #[error("invalid document: {0}")]
    Invalid(String),
    #[error("import requires confirmation because unsupported content may be omitted: {0:?}")]
    ImportNeedsConfirmation(Vec<ImportWarning>),
    #[error(
        "MindManager export requires confirmation: only the selected sheet and supported features are written"
    )]
    MindManagerExportNeedsConfirmation,
    #[error("MindManager import failed: {0}")]
    MindManager(String),
    #[error("MindManager export failed: {0}")]
    MindManagerExport(String),
}

/// Categories of source content not fully represented by the converted workbook.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ImportWarning {
    Formatting,
    Notes,
    Resources,
    Markers,
    Links,
    Other,
}

/// A best-effort conversion that must not be used destructively without consent.
#[derive(Debug)]
pub struct ImportReport {
    pub format: Format,
    pub workbook: Workbook,
    pub warnings: Vec<ImportWarning>,
}

/// Detect a format using extension first, then common file signatures/content.
pub fn detect_format(path: &Path, bytes: &[u8]) -> Option<Format> {
    if let Some(f) = Format::from_path(path) {
        return Some(f);
    }
    if bytes.starts_with(b"PK\x03\x04")
        && let Ok(z) = zip::ZipArchive::new(std::io::Cursor::new(bytes))
    {
        for name in ["content.json", "content.xml", "Document.xml"] {
            if z.file_names()
                .any(|entry| entry.is_ok_and(|value| value == name))
            {
                return Some(match name {
                    "content.json" | "content.xml" => Format::XMind,
                    _ => Format::MindManager,
                });
            }
        }
        if z.file_names()
            .any(|entry| entry.is_ok_and(|value| value.ends_with(".textbundle/info.json")))
        {
            return Some(Format::TextBundle);
        }
        return Some(Format::Thinoda);
    }
    // Inspect XML's actual root, not a substring in a comment or topic title.
    let mut reader = quick_xml::NsReader::from_reader(bytes);
    loop {
        match reader.read_resolved_event() {
            Ok((
                quick_xml::name::ResolveResult::Bound(ns),
                quick_xml::events::Event::Start(e) | quick_xml::events::Event::Empty(e),
            )) if e.local_name().as_ref() == "Map"
                && ns.as_ref() == "http://schemas.mindjet.com/MindManager/Application/2003" =>
            {
                return Some(Format::MindManagerXml);
            }
            Ok((
                _,
                quick_xml::events::Event::Decl(_)
                | quick_xml::events::Event::Comment(_)
                | quick_xml::events::Event::Text(_),
            )) => {}
            _ => break,
        }
    }
    let start = String::from_utf8_lossy(bytes)
        .trim_start()
        .to_ascii_lowercase();
    if start.contains("<map") {
        Some(Format::FreeMind)
    } else if start.contains("<opml") {
        Some(Format::Opml)
    } else if start.contains("<xmap-content") || start.contains("<xmind") {
        Some(Format::XMind)
    } else if start.starts_with("<svg") || start.contains("<svg ") {
        Some(Format::Svg)
    } else {
        None
    }
}

/// Import document bytes into a validated workbook.
pub fn import_bytes(format: Format, bytes: &[u8]) -> Result<Workbook, IoError> {
    let report = import_bytes_with_report(format, bytes)?;
    if !report.warnings.is_empty() {
        return Err(IoError::ImportNeedsConfirmation(report.warnings));
    }
    Ok(report.workbook)
}

/// Parse without discarding the conversion warnings. Callers must obtain explicit
/// consent before using a report with warnings, and must preserve the source.
pub fn import_bytes_with_report(format: Format, bytes: &[u8]) -> Result<ImportReport, IoError> {
    if !format.can_import() {
        return Err(IoError::UnsupportedFormat(format.label().into()));
    }
    if matches!(format, Format::MindManager | Format::MindManagerXml) {
        let report = mindmanager::import(bytes, format)
            .map_err(|error| IoError::MindManager(error.to_string()))?;
        report.workbook.validate().map_err(IoError::Invalid)?;
        return Ok(report);
    }
    let result = match format {
        Format::Thinoda => formats::native_import(bytes)?,
        Format::XMind => xmind::import(bytes)?,
        Format::FreeMind => formats::freemind_import(bytes)?,
        Format::Opml => formats::opml_import(bytes)?,
        Format::Markdown => formats::markdown_import(bytes)?,
        Format::PlainText => formats::text_import(bytes)?,
        _ => unreachable!(),
    };
    result.validate().map_err(IoError::Invalid)?;
    Ok(ImportReport {
        format,
        workbook: result,
        warnings: Vec::new(),
    })
}
/// Import a document from disk.
pub fn import_file(path: &Path) -> Result<Workbook, IoError> {
    let report = import_file_with_report(path)?;
    if !report.warnings.is_empty() {
        return Err(IoError::ImportNeedsConfirmation(report.warnings));
    }
    Ok(report.workbook)
}

/// Largest accepted MindManager input, bounded before parsing.
const MINDMANAGER_LIMIT: usize = 64 * 1024 * 1024;

/// Import bytes whose format is detected from `name` (a file name or path) and
/// content, as for [`import_file_with_report`]. Used where files arrive as
/// bytes, for example from a browser file picker or drag and drop.
pub fn import_named_bytes_with_report(name: &Path, bytes: &[u8]) -> Result<ImportReport, IoError> {
    let bounded = matches!(
        Format::from_path(name),
        None | Some(Format::MindManager | Format::MindManagerXml)
    );
    if bounded && bytes.len() > MINDMANAGER_LIMIT {
        return Err(IoError::Invalid(
            "MindManager file exceeds size limit".into(),
        ));
    }
    let f = detect_format(name, bytes)
        .ok_or_else(|| IoError::UnsupportedFormat(name.display().to_string()))?;
    import_bytes_with_report(f, bytes)
}

/// [`import_named_bytes_with_report`] that refuses imports needing confirmation.
pub fn import_named_bytes(name: &Path, bytes: &[u8]) -> Result<Workbook, IoError> {
    let report = import_named_bytes_with_report(name, bytes)?;
    if !report.warnings.is_empty() {
        return Err(IoError::ImportNeedsConfirmation(report.warnings));
    }
    Ok(report.workbook)
}

/// File counterpart of [`import_bytes_with_report`].
pub fn import_file_with_report(path: &Path) -> Result<ImportReport, IoError> {
    // Bound MindManager file reads before allocating the input buffer.
    let bounded = matches!(
        Format::from_path(path),
        None | Some(Format::MindManager | Format::MindManagerXml)
    );
    if bounded && fs::metadata(path)?.len() > 64 * 1024 * 1024 {
        return Err(IoError::Invalid(
            "MindManager file exceeds size limit".into(),
        ));
    }
    let b = if bounded {
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take(64 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 64 * 1024 * 1024 {
            return Err(IoError::Invalid(
                "MindManager file exceeds size limit".into(),
            ));
        }
        bytes
    } else {
        fs::read(path)?
    };
    import_named_bytes_with_report(path, &b)
}
/// Export a workbook to bytes in the requested format.
pub fn export_bytes(
    book: &Workbook,
    format: Format,
    opts: &ExportOptions,
) -> Result<Vec<u8>, IoError> {
    export_bytes_with_consent(book, format, opts, false)
}

/// Export with explicit consent to omit unsupported MindManager content.
/// Other format policies are unchanged.
pub fn export_bytes_with_consent(
    book: &Workbook,
    format: Format,
    opts: &ExportOptions,
    allow_lossy: bool,
) -> Result<Vec<u8>, IoError> {
    book.validate().map_err(|error| {
        if format == Format::MindManager {
            IoError::MindManagerExport(error)
        } else {
            IoError::Invalid(error)
        }
    })?;
    if !format.can_export() {
        return Err(IoError::UnsupportedFormat(format.label().into()));
    }
    if symbol_color_loss(book, format) && !opts.allow_symbol_color_loss {
        if format == Format::MindManager {
            return Err(IoError::MindManagerExportNeedsConfirmation);
        }
        return Err(IoError::Invalid(
            "this editable format cannot preserve symbol colors; use Thinoda, XMind, or a visual export, or explicitly acknowledge color loss with allow_symbol_color_loss".into(),
        ));
    }
    Ok(match format {
        Format::Thinoda => formats::native_export(book)?,
        Format::XMind => xmind::export(book)?,
        Format::FreeMind => formats::freemind_export(book, opts)?,
        Format::Opml => formats::opml_export(book, opts)?,
        Format::Markdown => formats::markdown_export(book, opts),
        Format::PlainText => formats::text_export(book, opts),
        Format::MindManager => {
            mindmanager::export(book, opts, allow_lossy).map_err(|error| match error {
                IoError::MindManagerExportNeedsConfirmation => error,
                error => IoError::MindManagerExport(error.to_string()),
            })?
        }
        Format::TextBundle => formats::textbundle_export(book, opts)?,
        Format::Svg => render::svg(book, opts)?.into_bytes(),
        Format::Png => render::raster(book, opts, image::ImageFormat::Png)?,
        Format::Jpeg => render::raster(book, opts, image::ImageFormat::Jpeg)?,
        Format::Pdf => render::pdf(book, opts)?,
        Format::Word => office::docx(book, opts)?,
        Format::Excel => office::xlsx(book, opts)?,
        Format::Csv => office::csv(book, opts),
        _ => unreachable!(),
    })
}

/// Formats without per-symbol color support. XMind retains the Thinoda metadata payload,
/// but its standard icons do not display custom colors in other applications.
pub fn symbol_color_loss(book: &Workbook, format: Format) -> bool {
    !matches!(
        format,
        Format::Thinoda | Format::XMind | Format::Svg | Format::Png | Format::Jpeg | Format::Pdf
    ) && book
        .sheets
        .iter()
        .any(|s| s.topics.values().any(|t| !t.marker_colors.is_empty()))
}
/// Atomically write an exported document next to its destination.
pub fn export_file(
    book: &Workbook,
    path: &Path,
    format: Format,
    opts: &ExportOptions,
) -> Result<(), IoError> {
    let data = export_bytes(book, format, opts)?;
    let parent = path.parent().unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("export");
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp = parent.join(format!(".{name}.{}.{nonce}.tmp", std::process::id()));
    let written = {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        f.write_all(&data).and_then(|_| f.sync_all())
    };
    if let Err(e) = written {
        let _ = fs::remove_file(&tmp);
        return Err(IoError::Io(e));
    }
    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(IoError::Io(e));
    }
    Ok(())
}

/// Read a named archive entry with a decompressed-size guard.
pub(crate) fn zip_entry<R: Read + std::io::Seek>(
    z: &mut zip::ZipArchive<R>,
    name: &str,
) -> Result<Vec<u8>, IoError> {
    zip_entry_with_limit(z, name, MAX_ENTRY_SIZE)
}

pub(crate) fn zip_entry_with_limit<R: Read + std::io::Seek>(
    z: &mut zip::ZipArchive<R>,
    name: &str,
    limit: u64,
) -> Result<Vec<u8>, IoError> {
    let f = z.by_name(name)?;
    if f.size() > limit {
        return Err(IoError::Invalid(format!(
            "archive entry {name} exceeds size limit"
        )));
    }
    let mut out = Vec::with_capacity(f.size() as usize);
    f.take(limit + 1).read_to_end(&mut out)?;
    if out.len() as u64 > limit {
        return Err(IoError::Invalid(format!(
            "archive entry {name} exceeds size limit"
        )));
    }
    Ok(out)
}
