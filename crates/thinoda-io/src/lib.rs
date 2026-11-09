//! Import and export adapters for Thinoda and common mind-map formats.
//!
//! Markdown export uses the selected sheet, or the active sheet when no sheet is selected.
//! Graphical output shares font-measured title layout. PDF keeps vector paths
//! (including outlined text); JPEG always uses an opaque background.
mod board_export;
mod drawio;
pub mod drawio_clipboard;
pub mod fim;
pub mod fonts;
mod formats;
mod freemind_notes;
pub mod gantt;
mod mindmanager;
mod native_resources;
mod office;
mod office_notes;
pub mod presentation;
pub mod print;
mod render;
mod xmind;

use std::{
    collections::HashSet,
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
    FimXdf2,
    FimXdf3,
    Thinoda,
    XMind,
    FreeMind,
    Opml,
    Markdown,
    PlainText,
    MindManager,
    MindManagerXml,
    TextBundle,
    DrawIo,
    DrawIoSvg,
    DrawIoPng,
    DrawIoHtml,
    Svg,
    Png,
    Jpeg,
    Pdf,
    Word,
    PowerPoint,
    Excel,
    Csv,
}
impl Format {
    pub const IMPORTABLE: &'static [Format] = &[
        Self::FimXdf2,
        Self::FimXdf3,
        Self::Thinoda,
        Self::XMind,
        Self::FreeMind,
        Self::Opml,
        Self::Markdown,
        Self::PlainText,
        Self::MindManager,
        Self::MindManagerXml,
        Self::TextBundle,
        Self::DrawIo,
        Self::DrawIoSvg,
        Self::DrawIoPng,
        Self::DrawIoHtml,
        Self::Word,
    ];
    pub const EXPORTABLE: &'static [Format] = &[
        Self::FimXdf2,
        Self::FimXdf3,
        Self::Thinoda,
        Self::XMind,
        Self::FreeMind,
        Self::Opml,
        Self::Markdown,
        Self::PlainText,
        Self::MindManager,
        Self::TextBundle,
        Self::DrawIo,
        Self::DrawIoSvg,
        Self::DrawIoPng,
        Self::Svg,
        Self::Png,
        Self::Jpeg,
        Self::Pdf,
        Self::Word,
        Self::PowerPoint,
        Self::Excel,
        Self::Csv,
    ];
    pub fn from_path(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
        if name.ends_with(".drawio.svg") {
            return Some(Self::DrawIoSvg);
        }
        if name.ends_with(".drawio.png") {
            return Some(Self::DrawIoPng);
        }
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        Some(match ext.as_str() {
            "xdf2" => Self::FimXdf2,
            "xdf3" => Self::FimXdf3,
            "thinoda" => Self::Thinoda,
            "xmind" => Self::XMind,
            "mm" => Self::FreeMind,
            "opml" => Self::Opml,
            "md" | "markdown" => Self::Markdown,
            "txt" => Self::PlainText,
            "mmap" => Self::MindManager,
            "xmmap" => Self::MindManagerXml,
            "textpack" | "textbundle" => Self::TextBundle,
            "drawio" | "dio" => Self::DrawIo,
            "html" | "htm" => Self::DrawIoHtml,
            "svg" => Self::Svg,
            "png" => Self::Png,
            "jpg" | "jpeg" => Self::Jpeg,
            "pdf" => Self::Pdf,
            "docx" => Self::Word,
            "pptx" => Self::PowerPoint,
            "xlsx" => Self::Excel,
            "csv" => Self::Csv,
            _ => return None,
        })
    }
    pub fn extensions(&self) -> &'static [&'static str] {
        match self {
            Self::FimXdf2 => &["xdf2"],
            Self::FimXdf3 => &["xdf3"],
            Self::Thinoda => &["thinoda"],
            Self::XMind => &["xmind"],
            Self::FreeMind => &["mm"],
            Self::Opml => &["opml"],
            Self::Markdown => &["md", "markdown"],
            Self::PlainText => &["txt"],
            Self::MindManager => &["mmap"],
            Self::MindManagerXml => &["xmmap"],
            Self::TextBundle => &["textpack", "textbundle"],
            Self::DrawIo => &["drawio", "dio"],
            Self::DrawIoSvg => &["drawio.svg"],
            Self::DrawIoPng => &["drawio.png"],
            Self::DrawIoHtml => &["html", "htm"],
            Self::Svg => &["svg"],
            Self::Png => &["png"],
            Self::Jpeg => &["jpg", "jpeg"],
            Self::Pdf => &["pdf"],
            Self::Word => &["docx"],
            Self::PowerPoint => &["pptx"],
            Self::Excel => &["xlsx"],
            Self::Csv => &["csv"],
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::FimXdf2 => "FIM XDatenfelder 2",
            Self::FimXdf3 => "FIM XDatenfelder 3",
            Self::Thinoda => "Thinoda",
            Self::XMind => "XMind",
            Self::FreeMind => "FreeMind / Freeplane",
            Self::Opml => "OPML",
            Self::Markdown => "Markdown",
            Self::PlainText => "Plain text",
            Self::MindManager => "MindManager",
            Self::MindManagerXml => "MindManager XML",
            Self::TextBundle => "TextBundle",
            Self::DrawIo => "draw.io",
            Self::DrawIoSvg => "draw.io SVG (editable)",
            Self::DrawIoPng => "draw.io PNG (editable)",
            Self::DrawIoHtml => "draw.io HTML (editable)",
            Self::Svg => "SVG",
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::Pdf => "PDF",
            Self::Word => "Word",
            Self::PowerPoint => "PowerPoint",
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

    pub fn is_drawio(self) -> bool {
        matches!(
            self,
            Self::DrawIo | Self::DrawIoSvg | Self::DrawIoPng | Self::DrawIoHtml
        )
    }

    pub fn is_drawio_image(self) -> bool {
        matches!(self, Self::DrawIoSvg | Self::DrawIoPng)
    }
}

/// Rendering settings. `sheet: None` selects the active sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportOptions {
    /// Explicit caller acknowledgement for editable formats without symbol-color fields.
    pub allow_symbol_color_loss: bool,
    /// Explicit acknowledgement that CSV/OPML retain Markdown source, not rich formatting.
    pub allow_note_format_loss: bool,
    pub sheet: Option<usize>,
    pub dark: bool,
    pub scale: f32,
    pub transparent: bool,
    pub padding: f32,
    pub presentation_aspect_ratio: PresentationAspectRatio,
}
impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            allow_symbol_color_loss: false,
            allow_note_format_loss: false,
            sheet: None,
            dark: false,
            scale: 1.0,
            transparent: false,
            padding: 24.0,
            presentation_aspect_ratio: PresentationAspectRatio::default(),
        }
    }
}

/// Optional view appearance for visual exports. The default retains the
/// historical export colors, independently of any application UI settings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VisualExportContext {
    automatic_board_stroke: Option<thinoda_core::Color>,
}

impl VisualExportContext {
    /// Use the caller's resolved automatic board stroke for connectors and
    /// unstyled shape outlines. Explicit and inherited colors still take
    /// precedence, and the source document is not modified. The supplied
    /// color must already include any fallback for a transparent UI accent.
    /// This also matches the canvas's two-unit default outline width and
    /// suppresses interior strokes of filled shapes with no explicit stroke.
    pub fn with_automatic_board_stroke(mut self, color: thinoda_core::Color) -> Self {
        self.automatic_board_stroke = Some(color);
        self
    }
}

/// Import/export error.
#[derive(Debug, Error)]
pub enum IoError {
    #[error(transparent)]
    Fim(#[from] fim::FimError),
    #[error(
        "FIM output requires exact-byte exchange-XSD and semantic validation; use fim::export_validated or the application's validated FIM export command"
    )]
    FimValidationRequired,
    #[error("{}", thinoda_core::equations::EXPORT_ERROR)]
    EquationNotesUnsupported,
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
    #[error("draw.io HTML: {0}")]
    DrawIoHtml(#[from] DrawIoHtmlError),
    #[error("Gantt export contains no tasks")]
    GanttNoTasks,
    #[error("Gantt export exceeds supported {0} limit")]
    GanttLimit(&'static str),
    #[error("presentation export exceeds supported {0} limit")]
    PresentationLimit(&'static str),
    #[error("invalid Gantt schedule: {0}")]
    GanttSchedule(#[from] thinoda_core::ScheduleError),
    #[error("import requires confirmation because unsupported content may be omitted: {0:?}")]
    ImportNeedsConfirmation(Vec<ImportWarning>),
    #[error(
        "draw.io clipboard conversion would lose content: {0:?}; handle the conversion report explicitly"
    )]
    DrawioClipboardLoss(Vec<ExportLoss>),
    #[error("The first draw.io page has no pasteable items; additional pages were not pasted.")]
    DrawioClipboardEmptyFirstPage,
    #[error("The draw.io page has no pasteable items.")]
    DrawioClipboardEmptyPage,
    #[error(
        "draw.io export would detach {0} connector-to-connector attachment(s); explicit consent is required"
    )]
    DrawioConnectorAttachmentsNeedConfirmation(usize),
    #[error(
        "MindManager export requires confirmation: only the selected sheet and supported features are written"
    )]
    MindManagerExportNeedsConfirmation,
    #[error(
        "this format exports formatted notes as Markdown source; explicitly acknowledge note formatting loss or save as .thinoda"
    )]
    NoteFormatLossNeedsConfirmation,
    #[error("MindManager import failed: {0}")]
    MindManager(String),
    #[error("MindManager export failed: {0}")]
    MindManagerExport(String),
    #[error("Word export cannot preserve the note on topic {topic}: {reason:?}")]
    WordNote {
        topic: String,
        reason: thinoda_core::notes::NoteExportError,
    },
    #[error("Excel export cannot preserve the note on topic {topic}: {reason:?}")]
    ExcelNote {
        topic: String,
        reason: thinoda_core::notes::NoteExportError,
    },
    #[error("FreeMind export cannot preserve the note on topic {topic}: {reason:?}")]
    FreeMindNote {
        topic: String,
        reason: thinoda_core::notes::NoteExportError,
    },
}

/// Failures while extracting an embedded draw.io HTML viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum DrawIoHtmlError {
    #[error("the HTML exceeds the supported size limit")]
    SizeLimit,
    #[error("the HTML exceeds the supported structure limit")]
    StructureLimit,
    #[error("the HTML is not UTF-8")]
    InvalidEncoding,
    #[error("the HTML cannot be parsed safely")]
    InvalidMarkup,
    #[error("the HTML has no embedded draw.io viewer configuration")]
    NoViewer,
    #[error("the draw.io viewer configuration is invalid")]
    InvalidViewer,
    #[error("the HTML viewer has no embedded diagram; remote references are not loaded")]
    NoEmbeddedDiagram,
    #[error("the HTML contains multiple draw.io viewers; export each diagram separately")]
    MultipleViewers,
}

pub use board_export::{
    ExportLoss, NATIVE_FORMAT_VERSION, export_losses, outline_sheet, required_native_version,
    synthesized_root_title, uses_board_semantics,
};
pub use gantt::{
    GanttBounds, GanttDocument, GanttRow, MAX_GANTT_CSV_BYTES, MAX_GANTT_IMAGE_PIXELS,
    MAX_GANTT_IMAGE_SIDE, MAX_GANTT_LABEL_BYTES, MAX_GANTT_ROWS, MAX_GANTT_SVG_BYTES,
    MAX_GANTT_TIMELINE_DAYS, export_gantt_csv, render_gantt, render_gantt_pdf, render_gantt_png,
    render_gantt_print,
};
pub use presentation::{
    MAX_PRESENTATION_ARCHIVE_BYTES, MAX_PRESENTATION_IMAGE_PIXELS, MAX_PRESENTATION_IMAGE_SIDE,
    MAX_PRESENTATION_TOTAL_PIXELS, PresentationAspectRatio, export_presentation_pdf,
    export_presentation_pdf_with_visual_context, export_presentation_png_zip,
    export_presentation_png_zip_with_visual_context, export_presentation_pptx,
    export_presentation_pptx_with_visual_context, fit_presentation_frame, generate_presentation,
    render_presentation_slide_png, render_presentation_slide_png_with_visual_context,
    render_presentation_slide_svg, render_presentation_slide_svg_with_visual_context,
};
pub use print::PrintDocument;

/// Categories of source content not fully represented by the converted workbook.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ImportWarning {
    Formatting,
    Notes,
    Resources,
    Markers,
    Links,
    /// A connector's referenced terminal became an unanchored point.
    DetachedConnectors,
    /// An unrecognized draw.io shape or named style was represented by a rectangle.
    UnsupportedDrawioShapes,
    /// A draw.io grid size or Thinoda grid extension was adjusted or ignored.
    DrawioGridAdjusted,
    /// A draw.io page background was approximated or omitted.
    DrawioBackgroundAdjusted,
    /// A draw.io folded note's appearance was approximated.
    DrawioFoldedNotesApproximated,
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
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ["xml", "xdf2", "xdf3"]
        .iter()
        .any(|e| extension.eq_ignore_ascii_case(e))
    {
        let detected = fim::inspect(bytes).ok().map(|p| match p.dialect {
            fim::Dialect::Xdf2 => Format::FimXdf2,
            fim::Dialect::Xdf3 => Format::FimXdf3,
        });
        if detected.is_some() || !extension.eq_ignore_ascii_case("xml") {
            return detected;
        }
    }
    // draw.io stores editable diagrams in images, HTML and plain .xml files.
    let named = Format::from_path(path);
    if matches!(
        named,
        None | Some(
            Format::Svg | Format::Png | Format::DrawIo | Format::DrawIoSvg | Format::DrawIoPng
        )
    ) && drawio::sniff(bytes)
    {
        return Some(Format::DrawIo);
    }
    if named.is_none() && drawio::sniff_html(bytes) {
        return Some(Format::DrawIoHtml);
    }
    if let Some(f) = named {
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
            .any(|entry| entry.is_ok_and(|value| value == "word/document.xml"))
        {
            return Some(Format::Word);
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
    let report = match format {
        Format::MindManager | Format::MindManagerXml => mindmanager::import(bytes, format)
            .map_err(|error| IoError::MindManager(error.to_string()))?,
        Format::TextBundle => formats::textbundle_import(bytes)?,
        Format::Word => office::docx_import(bytes)?,
        Format::FreeMind => formats::freemind_import_with_report(bytes)?,
        Format::DrawIo | Format::DrawIoSvg | Format::DrawIoPng => drawio::import(bytes)?,
        Format::DrawIoHtml => drawio::import_html(bytes)?,
        _ => {
            let workbook = match format {
                Format::Thinoda => formats::native_import(bytes)?,
                Format::XMind => xmind::import(bytes)?,
                Format::Opml => formats::opml_import(bytes)?,
                Format::Markdown => formats::markdown_import(bytes)?,
                Format::PlainText => formats::text_import(bytes)?,
                Format::FimXdf2 | Format::FimXdf3 => {
                    let profile = fim::inspect(bytes)?;
                    if (format == Format::FimXdf2) != (profile.dialect == fim::Dialect::Xdf2) {
                        return Err(fim::FimError::WrongDialect.into());
                    }
                    fim::import_document(bytes)?
                }
                _ => unreachable!(),
            };
            ImportReport {
                format,
                workbook,
                warnings: Vec::new(),
            }
        }
    };
    report.workbook.validate().map_err(IoError::Invalid)?;
    Ok(report)
}
/// Import a document from disk.
pub fn import_file(path: &Path) -> Result<Workbook, IoError> {
    let report = import_file_with_report(path)?;
    if !report.warnings.is_empty() {
        return Err(IoError::ImportNeedsConfirmation(report.warnings));
    }
    Ok(report.workbook)
}

/// Largest accepted archive/unknown-format input, bounded before parsing.
const IMPORT_ARCHIVE_LIMIT: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy)]
enum ImportReadPolicy {
    Fim,
    MindManager,
    Archive,
}

impl ImportReadPolicy {
    fn limit(self) -> usize {
        match self {
            Self::Fim => fim::MAX_XML_BYTES,
            Self::MindManager | Self::Archive => IMPORT_ARCHIVE_LIMIT,
        }
    }

    fn named_limit_error(self, name: &Path) -> IoError {
        match self {
            // FIM detection has always treated an over-limit XDF as unsupported.
            Self::Fim => IoError::UnsupportedFormat(name.display().to_string()),
            Self::MindManager | Self::Archive => {
                IoError::Invalid("import archive exceeds size limit".into())
            }
        }
    }

    fn file_limit_error(self, path: &Path) -> IoError {
        match self {
            Self::Fim => IoError::UnsupportedFormat(path.display().to_string()),
            Self::MindManager => IoError::Invalid("MindManager file exceeds size limit".into()),
            Self::Archive => IoError::Invalid("import archive exceeds size limit".into()),
        }
    }
}

fn import_read_policy(name: &Path) -> Option<ImportReadPolicy> {
    match Format::from_path(name) {
        Some(Format::FimXdf2 | Format::FimXdf3) => Some(ImportReadPolicy::Fim),
        Some(Format::MindManager | Format::MindManagerXml) => Some(ImportReadPolicy::MindManager),
        None
        | Some(
            Format::TextBundle
            | Format::Word
            | Format::DrawIo
            | Format::DrawIoSvg
            | Format::DrawIoPng
            | Format::DrawIoHtml
            | Format::Svg
            | Format::Png,
        ) => Some(ImportReadPolicy::Archive),
        _ => None,
    }
}

/// Input-byte limit determined by the file name, before format detection.
///
/// Explicit XDF names use the FIM XML limit. Unknown extensions, including
/// `.xml`, use the archive limit until their content can be identified.
/// `None` means the named format has no limit at this layer.
pub fn import_read_limit(name: &Path) -> Option<usize> {
    import_read_policy(name).map(ImportReadPolicy::limit)
}

/// Validate a named input's size using the same error as named-byte import.
///
/// This can be used before allocating the input, such as with browser file
/// metadata. Unknown and XML names retain the archive limit until detection.
pub fn validate_import_size(name: &Path, size: u64) -> Result<(), IoError> {
    if let Some(policy) = import_read_policy(name)
        && size > policy.limit() as u64
    {
        return Err(policy.named_limit_error(name));
    }
    Ok(())
}

/// Validate a file's size using the same error as file import.
///
/// Unlike named-byte import, oversized MindManager files retain their
/// historical file-specific error.
pub fn validate_import_file_size(path: &Path, size: u64) -> Result<(), IoError> {
    if let Some(policy) = import_read_policy(path)
        && size > policy.limit() as u64
    {
        return Err(policy.file_limit_error(path));
    }
    Ok(())
}

/// Import bytes whose format is detected from `name` (a file name or path) and
/// content, as for [`import_file_with_report`]. Used where files arrive as
/// bytes, for example from a browser file picker or drag and drop.
pub fn import_named_bytes_with_report(name: &Path, bytes: &[u8]) -> Result<ImportReport, IoError> {
    validate_import_size(name, bytes.len() as u64)?;
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

/// Largest self-contained draw.io published link accepted for local import.
pub const MAX_DRAWIO_LINK_BYTES: usize = drawio::MAX_LINK_BYTES;

/// Imports the diagram embedded in a draw.io `#R` link or fragment.
///
/// The link is never fetched. Callers must inspect the returned conversion
/// warnings before presenting the imported workbook as an editable copy.
pub fn import_drawio_link(link: &str) -> Result<ImportReport, IoError> {
    drawio::import_link(link)
}

/// File counterpart of [`import_bytes_with_report`].
pub fn import_file_with_report(path: &Path) -> Result<ImportReport, IoError> {
    let b = if let Some(policy) = import_read_policy(path) {
        let limit = policy.limit();
        validate_import_file_size(path, fs::metadata(path)?.len())?;
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)?;
        validate_import_file_size(path, bytes.len() as u64)?;
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

/// Export using an optional caller-provided view appearance.
pub fn export_bytes_with_visual_context(
    book: &Workbook,
    format: Format,
    opts: &ExportOptions,
    visual: VisualExportContext,
) -> Result<Vec<u8>, IoError> {
    export_bytes_with_consent_and_visual_context(book, format, opts, false, visual)
}

/// Export with explicit consent to omit unsupported MindManager content or
/// detach connector-to-connector attachments in draw.io. Callers should also
/// inspect [`export_losses`] for other changes.
pub fn export_bytes_with_consent(
    book: &Workbook,
    format: Format,
    opts: &ExportOptions,
    allow_lossy: bool,
) -> Result<Vec<u8>, IoError> {
    export_bytes_with_consent_and_visual_context(
        book,
        format,
        opts,
        allow_lossy,
        VisualExportContext::default(),
    )
}

/// Export with both explicit consent and an optional caller-provided view
/// appearance. Non-visual formats retain their existing behavior.
pub fn export_bytes_with_consent_and_visual_context(
    book: &Workbook,
    format: Format,
    opts: &ExportOptions,
    allow_lossy: bool,
    visual: VisualExportContext,
) -> Result<Vec<u8>, IoError> {
    if exported_sheets(book, format, opts).any(|s| s.fim.is_some())
        && !matches!(
            format,
            Format::Thinoda
                | Format::FimXdf2
                | Format::FimXdf3
                | Format::Svg
                | Format::Png
                | Format::Jpeg
                | Format::Pdf
                | Format::PowerPoint
        )
    {
        return Err(IoError::Invalid("this editable format cannot preserve FIM source/provenance; save as .thinoda or explicitly create a non-FIM projection first".into()));
    }
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
    if format.is_drawio()
        && !allow_lossy
        && let Some(count) = export_losses(book, format, opts)
            .into_iter()
            .find_map(|loss| match loss {
                ExportLoss::ConnectorAttachmentsDetached(count) => Some(count),
                _ => None,
            })
    {
        return Err(IoError::DrawioConnectorAttachmentsNeedConfirmation(count));
    }
    if format == Format::MindManager
        && !allow_lossy
        && book
            .sheets
            .get(
                opts.sheet
                    .unwrap_or(book.active_sheet)
                    .min(book.sheets.len().saturating_sub(1)),
            )
            .is_some_and(uses_board_semantics)
    {
        // Freeform board content is flattened to one outline (see `export_losses`).
        return Err(IoError::MindManagerExportNeedsConfirmation);
    }
    // Single-outline formats see Hidden-root sheets through the synthesized-root projection.
    let projected = if matches!(
        format,
        Format::FreeMind
            | Format::Opml
            | Format::Markdown
            | Format::PlainText
            | Format::MindManager
            | Format::TextBundle
            | Format::Word
            | Format::Excel
            | Format::Csv
    ) {
        board_export::outline_workbook(book, opts)
    } else {
        std::borrow::Cow::Borrowed(book)
    };
    let book: &Workbook = &projected;
    if format != Format::Thinoda {
        let equation_support = matches!(
            format,
            Format::XMind
                | Format::Opml
                | Format::Csv
                | Format::Svg
                | Format::Png
                | Format::Jpeg
                | Format::Pdf
                | Format::PowerPoint
        );
        if equation_support {
            let exported_sheets: Box<dyn Iterator<Item = &thinoda_core::Sheet>> =
                if format == Format::XMind {
                    Box::new(book.sheets.iter())
                } else {
                    let index = opts
                        .sheet
                        .unwrap_or(book.active_sheet)
                        .min(book.sheets.len() - 1);
                    Box::new(std::iter::once(&book.sheets[index]))
                };
            for sheet in exported_sheets {
                render::validate_equation_notes(sheet)?;
                if matches!(format, Format::Opml | Format::Csv)
                    && equation_notes_outside_outline(sheet)
                {
                    return Err(IoError::EquationNotesUnsupported);
                }
            }
        } else if exported_sheets(book, format, opts)
            .any(thinoda_core::equations::sheet_has_equations)
        {
            return Err(IoError::EquationNotesUnsupported);
        }
    }
    let supports_embedded_resources = matches!(
        format,
        Format::Thinoda
            | Format::XMind
            | Format::Svg
            | Format::Png
            | Format::Jpeg
            | Format::Pdf
            | Format::PowerPoint
    );
    if exported_sheets(book, format, opts).any(|sheet| {
        !sheet.resources.is_empty()
            && !(format.is_drawio() && drawio_keeps_resources(sheet))
            && (!supports_embedded_resources
                || (format == Format::PowerPoint
                    && (sheet
                        .topics
                        .values()
                        .any(|topic| topic.attachment.is_some())
                        || sheet.resources.entries.values().any(|resource| {
                            !matches!(resource.media_type.as_str(), "image/png" | "image/jpeg")
                        }))))
    }) {
        return Err(IoError::Invalid(
            "this format cannot yet preserve embedded resources; save as .thinoda to retain all bytes".into(),
        ));
    }
    if symbol_color_loss_with_options(book, format, opts) && !opts.allow_symbol_color_loss {
        if format == Format::MindManager {
            return Err(IoError::MindManagerExportNeedsConfirmation);
        }
        return Err(IoError::Invalid(
            "this editable format cannot preserve symbol colors; use Thinoda, XMind, or a visual export, or explicitly acknowledge color loss with allow_symbol_color_loss".into(),
        ));
    }
    if note_format_loss(book, format, opts) && !opts.allow_note_format_loss {
        return Err(IoError::NoteFormatLossNeedsConfirmation);
    }
    if matches!(format, Format::FimXdf2 | Format::FimXdf3) {
        return Err(IoError::FimValidationRequired);
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
        Format::DrawIo => drawio::export(book)?,
        Format::DrawIoSvg => drawio::embed_svg(
            render::svg(book, opts, visual)?,
            &book.sheets[export_sheet_index(book, opts)],
        )?,
        Format::DrawIoPng => drawio::embed_png(
            render::raster(book, opts, image::ImageFormat::Png, visual)?,
            &book.sheets[export_sheet_index(book, opts)],
        )?,
        Format::Svg => render::svg(book, opts, visual)?.into_bytes(),
        Format::Png => render::raster(book, opts, image::ImageFormat::Png, visual)?,
        Format::Jpeg => render::raster(book, opts, image::ImageFormat::Jpeg, visual)?,
        Format::Pdf => render::pdf(book, opts, visual)?,
        Format::Word => office::docx(book, opts)?,
        Format::PowerPoint => {
            let sheet = &book.sheets[opts
                .sheet
                .unwrap_or(book.active_sheet)
                .min(book.sheets.len() - 1)];
            export_presentation_pptx_with_visual_context(sheet, opts, visual)?
        }
        Format::Excel => office::xlsx(book, opts)?,
        Format::Csv => office::csv(book, opts),
        _ => unreachable!(),
    })
}

fn export_sheet_index(book: &Workbook, opts: &ExportOptions) -> usize {
    opts.sheet
        .unwrap_or(book.active_sheet)
        .min(book.sheets.len().saturating_sub(1))
}

fn exported_sheets<'a>(
    book: &'a Workbook,
    format: Format,
    opts: &ExportOptions,
) -> impl Iterator<Item = &'a thinoda_core::Sheet> {
    let selected = format
        .is_drawio_image()
        .then(|| export_sheet_index(book, opts));
    book.sheets
        .iter()
        .enumerate()
        .filter(move |(index, _)| selected.is_none_or(|selected| selected == *index))
        .map(|(_, sheet)| sheet)
}

/// Formats which do not preserve or render notes equations still refuse them.
/// Check all topics, including detached/hidden ones, before creating output.
pub(crate) fn reject_equation_notes(sheet: &thinoda_core::Sheet) -> Result<(), IoError> {
    if thinoda_core::equations::sheet_has_equations(sheet) {
        Err(IoError::EquationNotesUnsupported)
    } else {
        Ok(())
    }
}

/// Whether selected, reachable notes lose their declared rich-text interpretation.
/// CSV/OPML retain complete Markdown source; acknowledgement never changes the map.
pub fn note_format_loss(book: &Workbook, format: Format, opts: &ExportOptions) -> bool {
    if !matches!(format, Format::Csv | Format::Opml) || book.sheets.is_empty() {
        return false;
    }
    let sheet = board_export::outline_sheet(
        &book.sheets[opts
            .sheet
            .unwrap_or(book.active_sheet)
            .min(book.sheets.len() - 1)],
    );
    let mut stack = vec![(sheet.root, 0usize)];
    let mut visited = HashSet::new();
    while let Some((id, depth)) = stack.pop() {
        if depth > 2000 || !visited.insert(id) {
            continue;
        }
        if let Some(topic) = sheet.topics.get(&id) {
            if topic.notes.as_ref().is_some_and(|note| note.is_markdown()) {
                return true;
            }
            stack.extend(topic.children.iter().map(|child| (*child, depth + 1)));
        }
    }
    false
}

/// CSV/OPML emit only the central hierarchy. Refuse equation notes on detached
/// topics (or beyond the exporters' depth bound) rather than silently dropping
/// their source while claiming an equation-capable interchange path.
fn equation_notes_outside_outline(sheet: &thinoda_core::Sheet) -> bool {
    let mut included = HashSet::new();
    let mut stack = vec![(sheet.root, 0usize)];
    while let Some((id, depth)) = stack.pop() {
        if depth > 2000 || !included.insert(id) {
            continue;
        }
        if let Some(topic) = sheet.topics.get(&id) {
            stack.extend(topic.children.iter().map(|child| (*child, depth + 1)));
        }
    }
    sheet.topics.iter().any(|(id, topic)| {
        !included.contains(id)
            && topic
                .notes
                .as_ref()
                .is_some_and(thinoda_core::equations::has_equation)
    })
}

/// Formats without per-symbol color support. XMind retains the Thinoda metadata payload,
/// but its standard icons do not display custom colors in other applications.
/// draw.io inlines the PNG/JPEG pictures of image cards; any other resource use would be lost.
fn drawio_keeps_resources(sheet: &thinoda_core::Sheet) -> bool {
    let card_images: HashSet<&str> = sheet
        .topics
        .values()
        .filter(|topic| {
            topic.parent.is_none()
                && topic
                    .card
                    .is_some_and(|card| card.kind == thinoda_core::board::CardKind::Image)
        })
        .filter_map(|topic| topic.image.as_deref())
        .collect();
    sheet.topics.values().all(|topic| {
        topic.attachment.is_none()
            && topic
                .image
                .as_deref()
                .is_none_or(|image| card_images.contains(image))
    }) && sheet.resources.entries.iter().all(|(id, resource)| {
        matches!(resource.media_type.as_str(), "image/png" | "image/jpeg")
            && card_images
                .contains(format!("{}{id}", thinoda_core::resources::RESOURCE_PREFIX).as_str())
    })
}

pub fn symbol_color_loss(book: &Workbook, format: Format) -> bool {
    symbol_color_loss_with_options(book, format, &ExportOptions::default())
}

/// Whether the sheets selected by the export options lose custom symbol colors.
pub fn symbol_color_loss_with_options(
    book: &Workbook,
    format: Format,
    opts: &ExportOptions,
) -> bool {
    !matches!(
        format,
        Format::Thinoda
            | Format::XMind
            | Format::Svg
            | Format::Png
            | Format::Jpeg
            | Format::Pdf
            | Format::PowerPoint
    ) && exported_sheets(book, format, opts)
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

pub(crate) fn validate_document_archive<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
) -> Result<HashSet<String>, IoError> {
    const MAX_ENTRIES: usize = 4096;
    const MAX_ENTRY_SIZE: u64 = 32 * 1024 * 1024;
    const MAX_EXPANDED_SIZE: u64 = 128 * 1024 * 1024;

    if archive.len() > MAX_ENTRIES {
        return Err(IoError::Invalid(
            "document archive contains too many entries".into(),
        ));
    }

    let mut names = HashSet::with_capacity(archive.len());
    let mut expanded_size = 0u64;
    for index in 0..archive.len() {
        let file = archive.by_index(index)?;
        let name = file.name()?.to_string();
        let path = name.strip_suffix('/').unwrap_or(&name);
        if path.is_empty()
            || path.starts_with('/')
            || path
                .bytes()
                .any(|byte| byte == b'\\' || byte == b':' || byte.is_ascii_control())
            || path
                .split('/')
                .any(|component| component.is_empty() || component == "." || component == "..")
        {
            return Err(IoError::Invalid(format!(
                "document archive contains unsafe path {name}"
            )));
        }
        if !names.insert(name.clone()) {
            return Err(IoError::Invalid(format!(
                "document archive contains duplicate entry {name}"
            )));
        }
        if file.size() > MAX_ENTRY_SIZE {
            return Err(IoError::Invalid(format!(
                "document archive entry {name} exceeds size limit"
            )));
        }
        expanded_size = expanded_size.saturating_add(file.size());
        if expanded_size > MAX_EXPANDED_SIZE {
            return Err(IoError::Invalid(
                "document archive exceeds expanded size limit".into(),
            ));
        }
    }
    Ok(names)
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
