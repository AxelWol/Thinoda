//! Freeform board support shared by the native writer, outline exporters and the export
//! preflight: native version selection, the single-rooted outline projection and the
//! itemised loss report.

use crate::{ExportOptions, Format};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use thinoda_core::{
    CanvasObjectId, Endpoint, Sheet, Topic, Workbook,
    board::{CanvasObject, CanvasObjectKind, CardKind, ShapeKind, SheetRootMode},
    model::PresentationSource,
};

/// Native `.thinoda` format version written by this build.
pub const NATIVE_FORMAT_VERSION: u64 = 23;

/// Lowest native format version able to represent `book`.
///
/// 2 is the base format, 3 adds embedded resources, 4 formatted notes, 5 FIM documents,
/// 6 Freeform board semantics, 7 independent Zone background colors, and 8 inline card
/// text formatting, 9 explicit Zone object membership, 10 connector waypoints,
/// 11 open/block connector markers, 12 classic and thin connector markers,
/// 13 custom connector marker sizes, 14 connector marker fill overrides,
/// 15 diamond connector markers, 16 oval connector markers, 17 half-circle markers,
/// 18 bar connector markers, 19 adjustable rectangle corners, and
/// 20 partial rectangle outline sides, 21 double ellipses, 22 fixed connector ports,
/// and 23 standard draw.io rectangle rounding.
/// Workbooks without newer content keep their previous format version.
pub fn required_native_version(book: &Workbook) -> u64 {
    if book.has_drawio_rectangle_rounding() {
        23
    } else if book.has_fixed_connector_ports() {
        22
    } else if book.has_double_ellipses() {
        21
    } else if book.has_rectangle_sides() {
        20
    } else if book.has_shape_rounding() {
        19
    } else if book.has_bar_connector_markers() {
        18
    } else if book.has_half_circle_connector_markers() {
        17
    } else if book.has_oval_connector_markers() {
        16
    } else if book.has_diamond_connector_markers() {
        15
    } else if book.has_connector_marker_fills() {
        14
    } else if book.has_connector_marker_sizes() {
        13
    } else if book.has_extended_connector_markers() {
        12
    } else if book.has_connector_markers() {
        11
    } else if book.has_connector_waypoints() {
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
    /// Grouped translucent labels can differ from their canvas compositing.
    ConnectorLabelCompositingApproximated(usize),
    /// Board grid and snapping settings are not written.
    BoardSettingsDropped,
    /// Inline card titles with formatting or list semantics are not written.
    InlineTextDropped(usize),
    /// Mind-map topics, branches and annotations are written as unlinked drawn
    /// shapes and lines: they no longer reflow or form a hierarchy.
    MapElementsFlattened(usize),
    /// Connector ends attached to other connectors are written as free points.
    ConnectorAttachmentsDetached(usize),
    /// Fixed connector ends on topics not written to draw.io become free points.
    FixedConnectorAttachmentsDetached(usize),
    /// Fixed connector ends can land at different positions in draw.io.
    DrawioFixedConnectorPositionApproximated(usize),
    /// Legacy triangle arrow ends are approximated by draw.io classic arrows.
    LegacyConnectorMarkersApproximated(usize),
    /// Label color or opacity can differ in draw.io.
    DrawioLabelAppearanceApproximated(usize),
    /// Partial-rectangle colors and automatic outlines can differ in draw.io.
    DrawioRectangleAppearanceApproximated(usize),
    /// Double-ellipse colors, automatic outlines or labels can differ in draw.io.
    DrawioDoubleEllipseAppearanceApproximated(usize),
    /// A zero-width or zero-height process outline can be omitted by draw.io.
    DrawioCollapsedProcessApproximated(usize),
    /// draw.io couples grid visibility to snapping and cannot display a dot grid.
    DrawioGridSettingsApproximated(usize),
    /// Translucent backgrounds can blend against different editor backgrounds.
    DrawioBackgroundAppearanceApproximated(usize),
    /// Flat cards are exported as draw.io notes with folded corners.
    DrawioCardAppearanceApproximated(usize),
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
    if format.is_drawio_image() {
        return drawio_losses(selected_sheet(book, opts).map(|index| &book.sheets[index]));
    }
    if format == Format::DrawIo {
        return drawio_losses(&book.sheets);
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
        let grouped_labels = sheets
            .iter()
            .filter(|(_, sheet)| {
                format != Format::PowerPoint
                    || sheet
                        .presentation
                        .slides
                        .iter()
                        .any(|slide| slide.source == PresentationSource::Viewport)
            })
            .flat_map(|(_, sheet)| &sheet.canvas.objects)
            .filter(|object| connector_label_compositing_differs(object))
            .count();
        push(
            ExportLoss::ConnectorLabelCompositingApproximated,
            grouped_labels,
        );
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

fn connector_label_compositing_differs(object: &CanvasObject) -> bool {
    let CanvasObjectKind::Connector(connector) = &object.kind else {
        return false;
    };
    if connector.label.is_empty() {
        return false;
    }
    object.style.text_color.is_some_and(|color| {
        let opacity = if object.style.opacity.is_finite() {
            object.style.opacity.clamp(0.0, 1.0)
        } else {
            1.0
        };
        let alpha = (f32::from(color.a) * opacity).round() as u8;
        alpha > 0 && alpha < 255
    })
}

/// draw.io writes every sheet as a page. Board content round-trips; the mind-map
/// structure is drawn but not editable as a map.
fn drawio_losses<'a>(sheets: impl IntoIterator<Item = &'a Sheet>) -> Vec<ExportLoss> {
    let mut flattened = 0;
    let mut details = 0;
    let mut slides = 0;
    let mut inline_text = 0;
    let mut connector_attachments = 0;
    let mut fixed_connector_attachments = 0;
    let mut fixed_connector_positions = 0;
    let mut legacy_markers = 0;
    let mut label_appearance = 0;
    let mut rectangle_appearance = 0;
    let mut double_ellipse_appearance = 0;
    let mut collapsed_processes = 0;
    let mut grid_settings = 0;
    let mut background_appearance = 0;
    let mut card_appearance = 0;
    for sheet in sheets {
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
            card_appearance += usize::from(topic.card.is_some_and(|card| match card.kind {
                CardKind::Sticky => true,
                CardKind::Image => crate::drawio::card_image(sheet, topic).is_none(),
                CardKind::Text => false,
            }));
        }
        flattened += sheet.relationships.len() + sheet.boundaries.len() + sheet.summaries.len();
        slides += sheet.presentation.slides.len();
        connector_attachments += connector_attachment_counts(sheet).values().sum::<usize>();
        if sheet.canvas.has_fixed_connector_ports() {
            let objects: HashMap<_, _> = sheet
                .canvas
                .objects
                .iter()
                .map(|object| (object.id, object))
                .collect();
            let has_fixed_topic = sheet.canvas.objects.iter().any(|object| {
                matches!(&object.kind, CanvasObjectKind::Connector(connector)
                    if matches!(connector.from, Endpoint::Topic(_)) && connector.from_port.is_some()
                        || matches!(connector.to, Endpoint::Topic(_)) && connector.to_port.is_some())
            });
            let layout = has_fixed_topic.then(|| {
                thinoda_core::layout::layout_sheet(
                    sheet,
                    &thinoda_core::layout::ApproxMeasure,
                    false,
                )
            });
            let topics = layout
                .as_ref()
                .map(|layout| crate::drawio::exported_topics(sheet, layout));
            for (endpoint, port) in sheet
                .canvas
                .objects
                .iter()
                .filter_map(|object| match &object.kind {
                    CanvasObjectKind::Connector(connector) => Some([
                        (connector.from, connector.from_port),
                        (connector.to, connector.to_port),
                    ]),
                    _ => None,
                })
                .flatten()
                .filter_map(|(endpoint, port)| port.map(|port| (endpoint, port)))
            {
                let target = match endpoint {
                    Endpoint::Object(id) => objects.get(&id).copied(),
                    _ => None,
                };
                if matches!(endpoint, Endpoint::Topic(id) if topics.as_ref().is_some_and(|ids| !ids.contains(&id)))
                {
                    fixed_connector_attachments += 1;
                } else if crate::drawio::fixed_port_position_approximate(
                    sheet,
                    target,
                    layout.as_ref(),
                    endpoint,
                    port,
                ) {
                    fixed_connector_positions += 1;
                }
            }
        }
        for object in &sheet.canvas.objects {
            if matches!(
                object.kind,
                CanvasObjectKind::Shape {
                    shape: ShapeKind::Process,
                    ..
                }
            ) && (object.rect.size.x == 0.0 || object.rect.size.y == 0.0)
                && object.style.opacity > 0.0
                && match object.style.stroke {
                    Some(color) => color.a > 0,
                    None => object.style.fill.is_none(),
                }
            {
                collapsed_processes += 1;
            }
            if let Some(sides) = object.rectangle_sides
                && object.style.opacity > 0.0
            {
                let visible_outline = (sides.top || sides.right || sides.bottom || sides.left)
                    && match object.style.stroke {
                        Some(color) => color.a > 0,
                        None => object.style.fill.is_none(),
                    };
                let visible_fill = object.style.fill.is_some_and(|color| color.a > 0);
                let visible_label = !object.label().is_empty()
                    && object.style.text_color.is_none_or(|color| color.a > 0);
                rectangle_appearance +=
                    usize::from(visible_outline || visible_fill || visible_label);
            }
            if matches!(
                object.kind,
                CanvasObjectKind::Shape {
                    shape: ShapeKind::DoubleEllipse,
                    ..
                }
            ) && object.style.opacity > 0.0
            {
                let visible_outline = match object.style.stroke {
                    Some(color) => color.a > 0,
                    None => object.style.fill.is_none(),
                };
                let visible_fill = object.style.fill.is_some_and(|color| color.a > 0);
                let visible_label = !object.label().is_empty()
                    && object.style.text_color.is_none_or(|color| color.a > 0);
                double_ellipse_appearance +=
                    usize::from(visible_outline || visible_fill || visible_label);
            }
            let opacity_differs = !object.label().is_empty()
                && object.style.opacity < 1.0
                && object.style.text_color.is_none_or(|color| color.a > 0);
            let connector_label_differs = matches!(
                &object.kind,
                CanvasObjectKind::Connector(connector)
                    if !connector.label.is_empty()
                        && (object.style.text_color.is_none()
                            || connector_label_compositing_differs(object))
            );
            label_appearance += usize::from(opacity_differs || connector_label_differs);
            if let CanvasObjectKind::Connector(connector) = &object.kind {
                legacy_markers +=
                    usize::from(connector.arrow_start && connector.start_marker.is_none());
                legacy_markers +=
                    usize::from(connector.arrow_end && connector.end_marker.is_none());
            }
        }
        grid_settings += usize::from(
            sheet.board.grid == thinoda_core::GridKind::Dots
                || sheet.board.snap != (sheet.board.grid != thinoda_core::GridKind::None),
        );
        background_appearance +=
            usize::from(sheet.theme.background.is_some_and(|color| color.a < 255));
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
        (
            ExportLoss::FixedConnectorAttachmentsDetached,
            fixed_connector_attachments,
        ),
        (
            ExportLoss::DrawioFixedConnectorPositionApproximated,
            fixed_connector_positions,
        ),
        (
            ExportLoss::LegacyConnectorMarkersApproximated,
            legacy_markers,
        ),
        (
            ExportLoss::DrawioLabelAppearanceApproximated,
            label_appearance,
        ),
        (
            ExportLoss::DrawioRectangleAppearanceApproximated,
            rectangle_appearance,
        ),
        (
            ExportLoss::DrawioDoubleEllipseAppearanceApproximated,
            double_ellipse_appearance,
        ),
        (
            ExportLoss::DrawioCollapsedProcessApproximated,
            collapsed_processes,
        ),
        (ExportLoss::DrawioGridSettingsApproximated, grid_settings),
        (
            ExportLoss::DrawioBackgroundAppearanceApproximated,
            background_appearance,
        ),
        (
            ExportLoss::DrawioCardAppearanceApproximated,
            card_appearance,
        ),
        (ExportLoss::TopicDetailsDropped, details),
        (ExportLoss::InlineTextDropped, inline_text),
        (ExportLoss::SlidesDropped, slides),
    ] {
        if count > 0 {
            out.push(loss(count));
        }
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
