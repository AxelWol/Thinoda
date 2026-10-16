//! Import and export adapters for yMind and common mind-map formats.
//!
//! Markdown export uses the selected sheet, or the active sheet when no sheet is selected.
//! Raster and PDF output are rendered from the same vector layout; JPEG always uses an opaque background.
mod formats;
mod office;
mod render;
mod xmind;

use std::{
    fs,
    io::{Read, Write},
    path::Path,
};
use thiserror::Error;
use ymind_core::Workbook;

const MAX_ENTRY_SIZE: u64 = 200 * 1024 * 1024;

/// Supported document formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    YMind,
    XMind,
    FreeMind,
    Opml,
    Markdown,
    PlainText,
    MindManager,
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
        Self::YMind,
        Self::XMind,
        Self::FreeMind,
        Self::Opml,
        Self::Markdown,
        Self::PlainText,
        Self::MindManager,
    ];
    pub const EXPORTABLE: &'static [Format] = &[
        Self::YMind,
        Self::XMind,
        Self::FreeMind,
        Self::Opml,
        Self::Markdown,
        Self::PlainText,
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
            "ymind" => Self::YMind,
            "xmind" => Self::XMind,
            "mm" => Self::FreeMind,
            "opml" => Self::Opml,
            "md" | "markdown" => Self::Markdown,
            "txt" => Self::PlainText,
            "mmap" => Self::MindManager,
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
            Self::YMind => &["ymind"],
            Self::XMind => &["xmind"],
            Self::FreeMind => &["mm"],
            Self::Opml => &["opml"],
            Self::Markdown => &["md", "markdown"],
            Self::PlainText => &["txt"],
            Self::MindManager => &["mmap"],
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
            Self::YMind => "yMind",
            Self::XMind => "XMind",
            Self::FreeMind => "FreeMind / Freeplane",
            Self::Opml => "OPML",
            Self::Markdown => "Markdown",
            Self::PlainText => "Plain text",
            Self::MindManager => "MindManager",
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
    pub sheet: Option<usize>,
    pub dark: bool,
    pub scale: f32,
    pub transparent: bool,
    pub padding: f32,
}
impl Default for ExportOptions {
    fn default() -> Self {
        Self {
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
        return Some(Format::YMind);
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
    if !format.can_import() {
        return Err(IoError::UnsupportedFormat(format.label().into()));
    }
    let result = match format {
        Format::YMind => formats::native_import(bytes)?,
        Format::XMind => xmind::import(bytes)?,
        Format::FreeMind => formats::freemind_import(bytes)?,
        Format::Opml => formats::opml_import(bytes)?,
        Format::Markdown => formats::markdown_import(bytes)?,
        Format::PlainText => formats::text_import(bytes)?,
        Format::MindManager => formats::mindmanager_import(bytes)?,
        _ => unreachable!(),
    };
    result.validate().map_err(IoError::Invalid)?;
    Ok(result)
}
/// Import a document from disk.
pub fn import_file(path: &Path) -> Result<Workbook, IoError> {
    let b = fs::read(path)?;
    let f = detect_format(path, &b)
        .ok_or_else(|| IoError::UnsupportedFormat(path.display().to_string()))?;
    import_bytes(f, &b)
}
/// Export a workbook to bytes in the requested format.
pub fn export_bytes(
    book: &Workbook,
    format: Format,
    opts: &ExportOptions,
) -> Result<Vec<u8>, IoError> {
    book.validate().map_err(IoError::Invalid)?;
    if !format.can_export() {
        return Err(IoError::UnsupportedFormat(format.label().into()));
    }
    Ok(match format {
        Format::YMind => formats::native_export(book)?,
        Format::XMind => xmind::export(book)?,
        Format::FreeMind => formats::freemind_export(book, opts)?,
        Format::Opml => formats::opml_export(book, opts)?,
        Format::Markdown => formats::markdown_export(book, opts),
        Format::PlainText => formats::text_export(book, opts),
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
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(&data)?;
        f.sync_all()?;
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
    let f = z.by_name(name)?;
    if f.size() > MAX_ENTRY_SIZE {
        return Err(IoError::Invalid(format!(
            "archive entry {name} exceeds size limit"
        )));
    }
    let mut out = Vec::with_capacity(f.size() as usize);
    f.take(MAX_ENTRY_SIZE + 1).read_to_end(&mut out)?;
    if out.len() as u64 > MAX_ENTRY_SIZE {
        return Err(IoError::Invalid(format!(
            "archive entry {name} exceeds size limit"
        )));
    }
    Ok(out)
}
