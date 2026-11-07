//! Freeform board support shared by the native writer, outline exporters and the export
//! preflight: native version selection, the single-rooted outline projection and the
//! itemised loss report.

use crate::{ExportOptions, Format};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use thinoda_core::{
    CanvasObjectId, Endpoint, Sheet, Topic, Workbook,
    board::{CanvasObjectKind, SheetRootMode},
    model::PresentationSource,
};

/// Native `.thinoda` format version written by this build.
pub const NATIVE_FORMAT_VERSION: u64 = 10;

/// Lowest native format version able to represent `book`.
///
/// 2 is the base format, 3 adds embedded resources, 4 formatted notes, 5 FIM documents,
/// 6 Freeform board semantics, 7 independent Zone background colors, and 8 inline card
/// text formatting, 9 explicit Zone object membership, and 10 connector waypoints.
/// Workbooks without newer content keep their previous format version.
pub fn required_native_version(book: &Workbook) -> u64 {
    if book.has_connector_waypoints() {
        10
    } else if book
        .sheets
        .iter()
        .any(|sheet| sheet.zones.iter().any(|zone| !zone.objects.is_empty()))
    {
        9
    } else if book.has_inline_text() {
        8
    } else if book.sheets.iter().any(|sheet| {
        sheet
            .zones
            .iter()
            .any(|zone| zone.background_color.is_some())
    }) {
        7
    } else if book.sheets.iter().any(uses_board_semantics) {
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

pub(crate) fn connector_attachment_counts(sheet: &Sheet) -> HashMap<CanvasObjectId, usize> {
    let connector_ids: HashSet<_> = sheet
        .canvas
        .objects
        .iter()
        .filter(|object| object.is_connector())
        .map(|object| object.id)
        .collect();
    let mut counts = HashMap::new();
    for object in &sheet.canvas.objects {
        if let CanvasObjectKind::Connector(connector) = &object.kind {
            for end in [connector.from, connector.to] {
                if let Endpoint::Object(id) = end
                    && connector_ids.contains(&id)
                {
                    *counts.entry(id).or_insert(0) += 1;
                }
            }
        }
    }
    counts
}

/// Whether `sheet` uses content only native format version 6 can represent.
pub fn uses_board_semantics(sheet: &Sheet) -> bool {
    sheet.uses_board_semantics()
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
    let root_ids: HashSet<_> = roots.iter().copied().collect();
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
    out.floating.retain(|id| !root_ids.contains(id));
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
    /// Cards on Central sheets are omitted with their floating root trees. This
    /// count is a subset of [`ExportLoss::FloatingTopicsDropped`].
    CardsDropped(usize),
    /// The hidden central topic of sheet `sheet` is replaced by a root named `title`.
    SynthesizedRoot { sheet: usize, title: String },
    /// Presentation slides showing a free viewport are not written.
    ViewportSlidesDropped(usize),
    /// Card rotation is not written.
    RotationDropped(usize),
    /// Zones (frames, colors, clipping) are not written.
    ZonesDropped(usize),
    /// Zone content clipping may be omitted by exported editable slides.
    ZoneClipDropped(usize),
    /// Floating topics of Central sheets are outside the single outline and not written.
    FloatingTopicsDropped(usize),
    /// Detached summary topic trees, including their descendants, are not written.
    SummaryTreesDropped(usize),
    /// Relationship annotations between topics are not written.
    RelationshipsDropped(usize),
    /// Boundary annotations around topics are not written.
    BoundariesDropped(usize),
    /// Camera-image metadata or color profiles may be omitted during normalization.
    OrientedImageMetadataDropped(usize),
    /// Board grid and snapping settings are not written.
    BoardSettingsDropped,
    /// Inline card titles with formatting or list semantics are not written.
    InlineTextDropped(usize),
    /// Mind-map topics, branches and annotations are written as unlinked drawn
    /// shapes and lines: they no longer reflow or form a hierarchy.
    MapElementsFlattened(usize),
    /// Connector ends attached to other connectors are written as free points.
    ConnectorAttachmentsDetached(usize),
    /// Topic notes, labels, markers and task state are not written.
    TopicDetailsDropped(usize),
    /// Presentation slides are not written.
    SlidesDropped(usize),
}

/// Content lost when exporting `book` to `format` with `opts`, itemised with counts.
///
/// Plain data for an export preflight; empty when nothing is lost. Thinoda keeps
/// everything. SVG, PNG, JPEG and PDF apply frame clipping visually; this is not
/// secure redaction of embedded resources or vector content. Flattened PowerPoint
/// viewport slides also preserve visual clipping, while editable branch/zone slides
/// may omit it and retain a clipping warning. Visual formats also omit the editing
/// grid. XMind keeps every root as detached topics; the other editable formats write
/// one outline (see [`outline_sheet`]).
pub fn export_losses(book: &Workbook, format: Format, opts: &ExportOptions) -> Vec<ExportLoss> {
    if matches!(format, Format::Thinoda) {
        return Vec::new();
    }
    if format == Format::DrawIo {
        return drawio_losses(book);
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
        let oriented_images = sheets
            .iter()
            .map(|(_, sheet)| {
                let references: HashSet<_> = sheet
                    .topics
                    .values()
                    .filter_map(|topic| topic.image.as_deref())
                    .collect();
                references
                    .into_iter()
                    .filter(|reference| {
                        match sheet.resources.get(reference).and_then(|resource| {
                            thinoda_core::images::requires_visual_normalization(resource.bytes())
                        }) {
                            Ok(required) => required,
                            Err(error) => {
                                log::warn!("Could not inspect image for visual export: {error}");
                                // The export itself will report the invalid resource.
                                true
                            }
                        }
                    })
                    .count()
            })
            .sum();
        push(ExportLoss::OrientedImageMetadataDropped, oriented_images);
        if format == Format::PowerPoint
            && sheets.iter().any(|(_, sheet)| {
                sheet.presentation.slides.is_empty()
                    || sheet
                        .presentation
                        .slides
                        .iter()
                        .any(|slide| slide.source != PresentationSource::Viewport)
            })
        {
            push(ExportLoss::ZoneClipDropped, c.zone_clip);
        }
        push(ExportLoss::InlineTextDropped, c.inline_text);
    } else {
        push(ExportLoss::ShapesDropped, c.shapes);
        push(ExportLoss::InkDropped, c.ink);
        push(ExportLoss::ConnectorsDropped, c.connectors);
        if format == Format::XMind {
            push(ExportLoss::CardsConvertedToTopics, c.cards);
        } else {
            push(
                ExportLoss::CardsConvertedToTopics,
                c.cards - c.central_floating_cards,
            );
            push(ExportLoss::CardsDropped, c.central_floating_cards);
        }
        push(ExportLoss::RotationDropped, c.rotated_cards);
        push(ExportLoss::ViewportSlidesDropped, c.viewport_slides);
        push(ExportLoss::ZonesDropped, c.zones);
        if format != Format::XMind {
            push(ExportLoss::FloatingTopicsDropped, c.central_floating);
        }
        // These formats write only the ordinary child hierarchy (or, for FIM,
        // the authoritative FIM document). XMind instead writes these
        // annotations and detached summary topics in its public content.
        if matches!(
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
                | Format::FimXdf2
                | Format::FimXdf3
        ) {
            push(
                ExportLoss::SummaryTreesDropped,
                sheets
                    .iter()
                    .map(|(_, sheet)| omitted_summary_trees(sheet))
                    .sum(),
            );
            push(
                ExportLoss::RelationshipsDropped,
                sheets
                    .iter()
                    .map(|(_, sheet)| sheet.relationships.len())
                    .sum(),
            );
            push(
                ExportLoss::BoundariesDropped,
                sheets.iter().map(|(_, sheet)| sheet.boundaries.len()).sum(),
            );
        }
        push(ExportLoss::InlineTextDropped, c.inline_text);
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

/// draw.io writes every sheet as a page. Board content round-trips; the mind-map
/// structure is drawn but not editable as a map.
fn drawio_losses(book: &Workbook) -> Vec<ExportLoss> {
    let mut flattened = 0;
    let mut details = 0;
    let mut slides = 0;
    let mut inline_text = 0;
    let mut connector_attachments = 0;
    let mut settings = false;
    for sheet in &book.sheets {
        for (id, topic) in &sheet.topics {
            if topic.card.is_none() && !sheet.is_anchor(*id) {
                flattened += 1;
            }
            details += usize::from(
                topic.notes.is_some()
                    || !topic.labels.is_empty()
                    || !topic.markers.is_empty()
                    || topic.todo.is_some(),
            );
            inline_text += usize::from(!topic.inline_text.is_empty());
        }
        flattened += sheet.relationships.len() + sheet.boundaries.len() + sheet.summaries.len();
        slides += sheet.presentation.slides.len();
        connector_attachments += connector_attachment_counts(sheet).values().sum::<usize>();
        settings |= sheet.board.grid == thinoda_core::GridKind::Dots || sheet.board.snap;
    }
    let mut out = Vec::new();
    for (loss, count) in [
        (
            ExportLoss::MapElementsFlattened as fn(usize) -> ExportLoss,
            flattened,
        ),
        (
            ExportLoss::ConnectorAttachmentsDetached,
            connector_attachments,
        ),
        (ExportLoss::TopicDetailsDropped, details),
        (ExportLoss::InlineTextDropped, inline_text),
        (ExportLoss::SlidesDropped, slides),
    ] {
        if count > 0 {
            out.push(loss(count));
        }
    }
    if settings {
        out.push(ExportLoss::BoardSettingsDropped);
    }
    out
}

/// Count distinct summary roots outside the exported ordinary child hierarchy.
/// Summary descendants are not counted separately, and nested summaries count as
/// their own detached trees. Hidden-root outlines start at the semantic roots.
fn omitted_summary_trees(sheet: &Sheet) -> usize {
    if sheet.summaries.is_empty() {
        return 0;
    }
    let mut reachable = HashSet::new();
    let mut stack = sheet.semantic_roots();
    while let Some(id) = stack.pop() {
        if reachable.insert(id)
            && let Some(topic) = sheet.topic(id)
        {
            stack.extend(topic.children.iter().copied());
        }
    }
    sheet
        .summaries
        .iter()
        .map(|summary| summary.topic)
        .filter(|id| sheet.contains(*id) && !reachable.contains(id))
        .collect::<HashSet<_>>()
        .len()
}

#[derive(Default)]
struct Counts {
    shapes: usize,
    ink: usize,
    connectors: usize,
    cards: usize,
    /// Cards among the floating trees omitted from a Central-sheet outline.
    central_floating_cards: usize,
    rotated_cards: usize,
    viewport_slides: usize,
    zones: usize,
    zone_clip: usize,
    inline_text: usize,
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
            if sheet.root_mode == SheetRootMode::Central {
                self.central_floating_cards += 1;
            }
            self.rotated_cards += usize::from(card.rotation != 0.0);
        }
        self.inline_text += sheet
            .topics
            .values()
            .filter(|topic| !topic.inline_text.is_empty())
            .count();
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
