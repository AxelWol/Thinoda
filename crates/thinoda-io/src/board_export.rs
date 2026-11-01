//! Freeform board support shared by the native writer, outline exporters and the export
//! preflight: native version selection, the single-rooted outline projection and the
//! itemised loss report.

use crate::{ExportOptions, Format};
use std::borrow::Cow;
use thinoda_core::{
    Sheet, Topic, Workbook,
    board::{CanvasObjectKind, SheetRootMode},
    model::PresentationSource,
};

/// Native `.thinoda` format version written by this build.
pub const NATIVE_FORMAT_VERSION: u64 = 6;

/// Lowest native format version able to represent `book`.
///
/// 2 is the base format, 3 adds embedded resources, 4 formatted notes, 5 FIM documents
/// and 6 Freeform board semantics. Workbooks without newer content keep the version
/// (and bytes) they had before the newer feature existed.
pub fn required_native_version(book: &Workbook) -> u64 {
    if book.sheets.iter().any(uses_board_semantics) {
        6
    } else if book.sheets.iter().any(|sheet| sheet.fim.is_some()) {
        5
    } else if thinoda_core::notes::has_formatted_notes(book) {
        4
    } else if book.sheets.iter().any(|sheet| !sheet.resources.is_empty()) {
        3
    } else {
        2
    }
}

/// Whether `sheet` uses content only native format version 6 can represent.
pub fn uses_board_semantics(sheet: &Sheet) -> bool {
    sheet.root_mode == SheetRootMode::Hidden
        || !sheet.canvas.is_empty()
        || !sheet.board.is_default()
        || sheet.topics.values().any(|topic| topic.card.is_some())
        || sheet
            .zones
            .iter()
            .any(|zone| zone.color.is_some() || zone.clip)
        || sheet
            .presentation
            .slides
            .iter()
            .any(|slide| matches!(slide.source, PresentationSource::Viewport))
}

/// Sheet used by single-rooted outline exporters.
///
/// Central sheets are borrowed unchanged, so their output stays byte-identical.
/// Hidden-root sheets become a Central copy whose root is synthesized from the sheet
/// title (see [`thinoda_core::outline::project_to_outline`]) and whose children are the
/// semantic roots in reading order. The anchor's own fields never leak, cards become
/// ordinary topics, and board-only content (canvas, zones, slides) is dropped.
pub fn outline_sheet(sheet: &Sheet) -> Cow<'_, Sheet> {
    if sheet.root_mode == SheetRootMode::Central {
        return Cow::Borrowed(sheet);
    }
    let roots = sheet.semantic_roots();
    let mut out = sheet.clone();
    let title = synthesized_root_title(sheet);
    let mut root = Topic::new(out.root, title);
    root.children = roots.clone();
    out.topics.insert(out.root, root);
    for id in &roots {
        if let Some(topic) = out.topics.get_mut(id) {
            topic.parent = Some(out.root);
            topic.position = None;
            topic.card = None;
        }
    }
    out.floating.retain(|id| !roots.contains(id));
    out.root_mode = SheetRootMode::Central;
    out.canvas = Default::default();
    out.board = Default::default();
    out.zones.clear();
    out.presentation = Default::default();
    Cow::Owned(out)
}

/// Title of the root synthesized for a Hidden-root sheet.
pub fn synthesized_root_title(sheet: &Sheet) -> String {
    let title = sheet.title.trim();
    if title.is_empty() {
        "Untitled".into()
    } else {
        title.into()
    }
}

/// Workbook with the selected sheet replaced by its [`outline_sheet`] projection.
pub(crate) fn outline_workbook<'a>(book: &'a Workbook, opts: &ExportOptions) -> Cow<'a, Workbook> {
    let Some(index) = selected_sheet(book, opts) else {
        return Cow::Borrowed(book);
    };
    match outline_sheet(&book.sheets[index]) {
        Cow::Borrowed(_) => Cow::Borrowed(book),
        Cow::Owned(sheet) => {
            let mut out = book.clone();
            out.sheets[index] = sheet;
            Cow::Owned(out)
        }
    }
}

fn selected_sheet(book: &Workbook, opts: &ExportOptions) -> Option<usize> {
    (!book.sheets.is_empty()).then(|| {
        opts.sheet
            .unwrap_or(book.active_sheet)
            .min(book.sheets.len() - 1)
    })
}

/// One itemised loss of an export, for an export preflight.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExportLoss {
    /// Drawn shapes are not written.
    ShapesDropped(usize),
    /// Ink strokes are not written.
    InkDropped(usize),
    /// Free connectors are not written.
    ConnectorsDropped(usize),
    /// Cards are written as ordinary topics (detached topics or children of the root).
    CardsConvertedToTopics(usize),
    /// The hidden central topic of sheet `sheet` is replaced by a root named `title`.
    SynthesizedRoot { sheet: usize, title: String },
    /// Presentation slides showing a free viewport are not written.
    ViewportSlidesDropped(usize),
    /// Card rotation is not written.
    RotationDropped(usize),
    /// Zones (frames, colors, clipping) are not written.
    ZonesDropped(usize),
    /// Zone content clipping is not applied.
    ZoneClipDropped(usize),
    /// Floating topics of Central sheets are outside the single outline and not written.
    FloatingTopicsDropped(usize),
    /// Board grid and snapping settings are not written.
    BoardSettingsDropped,
}

/// Content lost when exporting `book` to `format` with `opts`, itemised with counts.
///
/// Plain data for an export preflight; empty when nothing is lost. Thinoda keeps
/// everything. Visual formats (SVG, PNG, JPEG, PDF, PowerPoint) paint the whole scene
/// and only lose zone clipping and the editing grid. XMind keeps every root as detached
/// topics; the other editable formats write one outline (see [`outline_sheet`]).
pub fn export_losses(book: &Workbook, format: Format, opts: &ExportOptions) -> Vec<ExportLoss> {
    if matches!(format, Format::Thinoda) {
        return Vec::new();
    }
    let sheets: Vec<(usize, &Sheet)> = match format {
        Format::XMind => book.sheets.iter().enumerate().collect(),
        _ => selected_sheet(book, opts)
            .map(|index| vec![(index, &book.sheets[index])])
            .unwrap_or_default(),
    };
    let mut c = Counts::default();
    for (_, sheet) in &sheets {
        c.add(sheet);
    }
    let mut out = Vec::new();
    let mut push = |loss: fn(usize) -> ExportLoss, count: usize| {
        if count > 0 {
            out.push(loss(count));
        }
    };
    if matches!(
        format,
        Format::Svg | Format::Png | Format::Jpeg | Format::Pdf | Format::PowerPoint
    ) {
        push(ExportLoss::ZoneClipDropped, c.zone_clip);
    } else {
        push(ExportLoss::ShapesDropped, c.shapes);
        push(ExportLoss::InkDropped, c.ink);
        push(ExportLoss::ConnectorsDropped, c.connectors);
        push(ExportLoss::CardsConvertedToTopics, c.cards);
        push(ExportLoss::RotationDropped, c.rotated_cards);
        push(ExportLoss::ViewportSlidesDropped, c.viewport_slides);
        push(ExportLoss::ZonesDropped, c.zones);
        if format != Format::XMind {
            push(ExportLoss::FloatingTopicsDropped, c.central_floating);
        }
        for (index, sheet) in &sheets {
            if sheet.root_mode == SheetRootMode::Hidden {
                out.push(ExportLoss::SynthesizedRoot {
                    sheet: *index,
                    title: synthesized_root_title(sheet),
                });
            }
        }
    }
    if c.settings {
        out.push(ExportLoss::BoardSettingsDropped);
    }
    out
}

#[derive(Default)]
struct Counts {
    shapes: usize,
    ink: usize,
    connectors: usize,
    cards: usize,
    rotated_cards: usize,
    viewport_slides: usize,
    zones: usize,
    zone_clip: usize,
    /// Floating topics of Central sheets (Hidden sheets fold them into the outline).
    central_floating: usize,
    settings: bool,
}

impl Counts {
    fn add(&mut self, sheet: &Sheet) {
        for object in &sheet.canvas.objects {
            match object.kind {
                CanvasObjectKind::Shape { .. } => self.shapes += 1,
                CanvasObjectKind::Ink(_) => self.ink += 1,
                CanvasObjectKind::Connector(_) => self.connectors += 1,
            }
        }
        for card in sheet
            .topics
            .values()
            .filter_map(|topic| topic.card.as_ref())
        {
            self.cards += 1;
            self.rotated_cards += usize::from(card.rotation != 0.0);
        }
        self.viewport_slides += sheet
            .presentation
            .slides
            .iter()
            .filter(|slide| matches!(slide.source, PresentationSource::Viewport))
            .count();
        self.zones += sheet.zones.len();
        self.zone_clip += sheet.zones.iter().filter(|zone| zone.clip).count();
        if sheet.root_mode == SheetRootMode::Central {
            self.central_floating += sheet
                .floating
                .iter()
                .filter(|id| sheet.topics.contains_key(id))
                .count();
        }
        self.settings |= !sheet.board.is_default();
    }
}
