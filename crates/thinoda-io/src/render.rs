use crate::{ExportOptions, IoError, VisualExportContext};
use std::{borrow::Cow, collections::HashMap, io::Cursor};
use thinoda_core::{
    Rect, Sheet, TextAlign, TopicId, Workbook,
    equations::{self as math, Layout as EquationLayout, Primitive as EquationPrimitive},
    geometry::{Color, Vec2},
    layout::{ConnectorKind, Dir, layout_sheet},
    model::TopicShape,
    theme::Theme,
};

mod board;
mod pptx;
mod text;
pub(crate) use pptx::editable_slide;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn col(c: Color) -> String {
    c.to_hex()
}

/// Choose a board frame title against the existing export fill and theme.
/// Transparent exports cannot guarantee contrast on an unknown final backdrop.
fn frame_title_color(tint: Color, fill: Color, background: Color, board: bool) -> Color {
    if board {
        Color::highest_contrast_on(fill.over(background))
    } else {
        tint
    }
}

pub(super) fn zone_export_fill(zone: &thinoda_core::Zone) -> Color {
    if let Some(fill) = zone.background_color {
        Color::rgba(
            fill.r,
            fill.g,
            fill.b,
            (f32::from(fill.a) * 0.16).round() as u8,
        )
    } else {
        let tint = zone.color.unwrap_or(Color::rgb(0x5D, 0x7A, 0x91));
        Color::rgba(tint.r, tint.g, tint.b, 20)
    }
}

fn xy(p: Vec2) -> String {
    format!("{:.2},{:.2}", p.x, p.y)
}
fn sheet<'a>(w: &'a Workbook, o: &ExportOptions) -> &'a thinoda_core::Sheet {
    &w.sheets[o.sheet.unwrap_or(w.active_sheet).min(w.sheets.len() - 1)]
}

struct VisualImage<'a> {
    media_type: &'a str,
    bytes: Cow<'a, [u8]>,
}

fn visual_image<'a>(
    resource: &'a thinoda_core::resources::Resource,
    remaining: &mut usize,
) -> Result<VisualImage<'a>, IoError> {
    let normalized = thinoda_core::images::normalized_visual_image(resource.bytes(), *remaining)
        .map_err(|error| IoError::Invalid(format!("could not prepare visual image: {error}")))?;
    let (media_type, bytes) = match normalized {
        Some(bytes) => ("image/png", Cow::Owned(bytes)),
        None => (
            resource.media_type.as_str(),
            Cow::Borrowed(resource.bytes()),
        ),
    };
    *remaining = remaining
        .checked_sub(bytes.len())
        .ok_or_else(|| IoError::Invalid("visual images exceed the export size limit".into()))?;
    Ok(VisualImage { media_type, bytes })
}

pub(crate) fn svg(
    w: &Workbook,
    o: &ExportOptions,
    visual: VisualExportContext,
) -> Result<String, IoError> {
    svg_with_fonts(w, o, crate::fonts::database(), visual)
}

fn svg_with_fonts(
    w: &Workbook,
    o: &ExportOptions,
    fontdb: std::sync::Arc<resvg::usvg::fontdb::Database>,
    visual: VisualExportContext,
) -> Result<String, IoError> {
    let s = sheet(w, o);
    svg_sheet_with_fonts(s, o, fontdb, None, visual)
}

pub(crate) fn presentation_layout(s: &Sheet, dark: bool) -> Result<thinoda_core::Layout, IoError> {
    let text = text::ExportText::new(s, dark, crate::fonts::database())?;
    Ok(layout_sheet(s, &text, dark))
}

pub(crate) struct EquationRow {
    pub topic: TopicId,
    title: text::Block,
    pub layout: EquationLayout,
    pub equation_y: f32,
    pub height: f32,
    pub color: Color,
    pub font_family: String,
}

pub(crate) struct EquationAppendix {
    pub rows: Vec<EquationRow>,
    pub width: f32,
    pub height: f32,
}

/// Validate every authored block, including equations outside a presentation's
/// visible scope. Imported malformed blocks must never be silently discarded.
pub(crate) fn validate_equation_notes(sheet: &Sheet) -> Result<(), IoError> {
    for topic in sheet.topics.values() {
        let Some(note) = topic.notes.as_ref().filter(|note| math::has_equation(note)) else {
            continue;
        };
        let Some((_, source)) =
            math::split_note(note).map_err(|_| IoError::EquationNotesUnsupported)?
        else {
            continue;
        };
        math::parse(source).map_err(|_| IoError::EquationNotesUnsupported)?;
    }
    Ok(())
}

pub(crate) fn equation_appendix(
    sheet: &Sheet,
    layout: &thinoda_core::Layout,
) -> Result<Option<EquationAppendix>, IoError> {
    validate_equation_notes(sheet)?;
    let mut rows = Vec::new();
    let mut width = 280.0_f32;
    let mut height = 48.0_f32;
    for id in &layout.order {
        let Some(topic) = sheet.topics.get(id) else {
            continue;
        };
        let Some(note) = topic.notes.as_ref().filter(|note| math::has_equation(note)) else {
            continue;
        };
        let (_, source) = math::split_note(note)
            .map_err(|_| IoError::EquationNotesUnsupported)?
            .ok_or(IoError::EquationNotesUnsupported)?;
        let formula = math::layout(source, 18.0, |_, size| (size * 0.62, size))
            .map_err(|_| IoError::EquationNotesUnsupported)?;
        let Some(topic_layout) = layout.topics.get(id) else {
            continue;
        };
        let color = topic_layout.style.text_color;
        let font_family = topic_layout
            .style
            .font_family
            .as_deref()
            .unwrap_or("Segoe UI, Helvetica, Arial, sans-serif")
            .to_owned();
        let title = text::equation_title(&topic.title, &font_family)?;
        let equation_y = title.size.y + 8.0;
        let row_height = equation_y + formula.height.max(18.0) + 12.0;
        width = width.max(title.size.x.max(formula.width) + 40.0);
        height += row_height;
        if !width.is_finite()
            || !height.is_finite()
            || width > math::MAX_WIDTH
            || height > math::MAX_HEIGHT
        {
            return Err(IoError::EquationNotesUnsupported);
        }
        rows.push(EquationRow {
            topic: *id,
            title,
            layout: formula,
            equation_y,
            height: row_height,
            color,
            font_family,
        });
    }
    if rows.is_empty() {
        return Ok(None);
    }
    Ok(Some(EquationAppendix {
        rows,
        width,
        height,
    }))
}

pub(crate) fn extend_equation_frame(
    sheet: &Sheet,
    layout: &thinoda_core::Layout,
    frame: Rect,
) -> Result<Rect, IoError> {
    let Some(appendix) = equation_appendix(sheet, layout)? else {
        return Ok(frame);
    };
    let width = frame.size.x.max(appendix.width);
    let height = frame.size.y + appendix.height;
    if !width.is_finite()
        || !height.is_finite()
        || width > math::MAX_WIDTH
        || height > math::MAX_HEIGHT
    {
        return Err(IoError::EquationNotesUnsupported);
    }
    Ok(Rect::from_min_size(
        Vec2::new(frame.center().x - width / 2.0, frame.min.y),
        Vec2::new(width, height),
    ))
}

/// A filtered paint sheet must retain the original sheet's export semantics.
pub(crate) fn svg_resolved(
    source: &Sheet,
    paint_sheet: &Sheet,
    o: &ExportOptions,
    resolved: &thinoda_core::ResolvedPresentationSlide,
    clip_scene: bool,
    visual: VisualExportContext,
) -> Result<String, IoError> {
    svg_layout_with_fonts(
        paint_sheet,
        o,
        crate::fonts::database(),
        Some(resolved.frame),
        &resolved.layout,
        &resolved.zones,
        SvgSceneOptions::new(source, clip_scene, visual),
    )
}

#[derive(Clone, Copy)]
struct SvgSceneOptions {
    clip_scene: bool,
    has_board_semantics: bool,
    visual: VisualExportContext,
}

impl SvgSceneOptions {
    fn new(source: &Sheet, clip_scene: bool, visual: VisualExportContext) -> Self {
        Self {
            clip_scene,
            has_board_semantics: source.uses_board_semantics(),
            visual,
        }
    }
}

pub(crate) fn branch_frames(
    sheet: &Sheet,
    roots: &[TopicId],
    dark: bool,
) -> Result<Vec<Rect>, IoError> {
    for root in roots {
        if !sheet.is_presentation_branch(*root) {
            return Err(IoError::Invalid(format!(
                "topic {root} is not a presentation branch"
            )));
        }
    }
    let text = text::ExportText::new(sheet, dark, crate::fonts::database())?;
    let layout = layout_sheet(sheet, &text, dark);
    let scene = thinoda_core::build_scene(sheet, &layout);
    let topic_bounds: HashMap<_, _> = scene
        .items
        .iter()
        .filter_map(|item| match item.id {
            thinoda_core::SceneId::Topic(id) => item.visible_bounds().map(|bounds| (id, bounds)),
            _ => None,
        })
        .collect();
    let relationship_sources: HashMap<_, _> = if sheet.zones.iter().any(|zone| zone.clip) {
        sheet
            .relationships
            .iter()
            .map(|relationship| (relationship.id, relationship))
            .collect()
    } else {
        HashMap::new()
    };
    roots
        .iter()
        .map(|root| {
            let owned: std::collections::HashSet<_> =
                sheet.owned_subtree_ids(*root).into_iter().collect();
            let root_clip = thinoda_core::scene::topic_clip_rect(sheet, *root);
            let mut bounds = owned
                .iter()
                .filter_map(|id| topic_bounds.get(id).copied())
                .reduce(|bounds, rect| bounds.union(&rect))
                // A completely clipped tree uses its frame rather than the
                // potentially distant, invisible topic geometry as a fallback.
                .or(root_clip)
                .or_else(|| {
                    owned
                        .iter()
                        .filter_map(|id| layout.topics.get(id).map(|topic| topic.rect))
                        .reduce(|bounds, rect| bounds.union(&rect))
                })
                .ok_or_else(|| IoError::Invalid(format!("branch {root} has no visible topics")))?;
            for summary in &layout.summaries {
                if owned.contains(&summary.topic)
                    || sheet
                        .summaries
                        .iter()
                        .any(|source| source.id == summary.id && owned.contains(&source.parent))
                {
                    let clip = thinoda_core::scene::topic_clip_rect(sheet, summary.topic);
                    if let Some(rect) = thinoda_core::presentation::clip_presentation_bounds(
                        summary.range_rect,
                        clip,
                    ) {
                        bounds = bounds.union(&rect);
                    }
                }
            }
            for boundary in &layout.boundaries {
                if let Some(source) = sheet
                    .boundaries
                    .iter()
                    .find(|source| source.id == boundary.id && owned.contains(&source.parent))
                {
                    let clip = thinoda_core::scene::topic_clip_rect(sheet, source.parent);
                    let rect = if clip.is_some() {
                        boundary.rect.expand(1.0)
                    } else {
                        boundary.rect
                    };
                    if let Some(rect) =
                        thinoda_core::presentation::clip_presentation_bounds(rect, clip)
                    {
                        bounds = bounds.union(&rect);
                    }
                    if let Some(clip) = clip
                        && source
                            .title
                            .as_deref()
                            .is_some_and(|title| !title.is_empty())
                    {
                        bounds = bounds.union(&clip);
                    }
                }
            }
            // Valid frame membership is attached to floating roots, so every
            // topic and summary owned by this branch inherits the same clip.
            // Additional connector geometry is considered only for clipped
            // branches, preserving the historical no-clip generated frame.
            if let Some(clip) = root_clip {
                let mut include_points = |points: &[Vec2], margin: f32| {
                    if let Some(rect) =
                        thinoda_core::board::points_bounds(points).and_then(|rect| {
                            thinoda_core::presentation::clip_presentation_bounds(
                                rect.expand(margin),
                                Some(clip),
                            )
                        })
                    {
                        bounds = bounds.union(&rect);
                    }
                };
                for connector in &layout.connectors {
                    if owned.contains(&connector.parent) && owned.contains(&connector.child) {
                        include_points(
                            &[connector.start, connector.end],
                            connector.width.max(0.0) / 2.0,
                        );
                    }
                }
                for summary in &layout.summaries {
                    if !owned.contains(&summary.topic) {
                        continue;
                    }
                    include_points(&[summary.start, summary.tip], 1.0);
                    include_points(&[summary.end, summary.tip], 1.0);
                    if let Some(topic) = layout.rect(summary.topic) {
                        let end = match summary.dir {
                            Dir::Right => topic.left_center(),
                            Dir::Left => topic.right_center(),
                            Dir::Down => topic.top_center(),
                            Dir::Up => topic.bottom_center(),
                        };
                        include_points(&[summary.tip, end], 1.0);
                    }
                }
                for relationship in &layout.relationships {
                    let Some(source) = relationship_sources.get(&relationship.id) else {
                        continue;
                    };
                    if owned.contains(&source.from) && owned.contains(&source.to) {
                        include_points(
                            &[
                                relationship.start,
                                relationship.c1,
                                relationship.c2,
                                relationship.end,
                            ],
                            10.0, // Conservative room for canvas and SVG arrowheads.
                        );
                        // Relationship label extents are not measured by Layout.
                        if !source.title.is_empty() {
                            include_points(&[clip.min, clip.max()], 0.0);
                        }
                    }
                }
            }
            Ok(bounds.expand(80.0))
        })
        .collect()
}

fn svg_sheet_with_fonts(
    s: &thinoda_core::Sheet,
    o: &ExportOptions,
    fontdb: std::sync::Arc<resvg::usvg::fontdb::Database>,
    frame: Option<thinoda_core::Rect>,
    visual: VisualExportContext,
) -> Result<String, IoError> {
    let text = text::ExportText::new(s, o.dark, fontdb.clone())?;
    let layout = layout_sheet(s, &text, o.dark);
    svg_layout_with_fonts(
        s,
        o,
        fontdb,
        frame,
        &layout,
        &s.zones,
        SvgSceneOptions::new(s, false, visual),
    )
}

fn svg_layout_with_fonts(
    s: &Sheet,
    o: &ExportOptions,
    fontdb: std::sync::Arc<resvg::usvg::fontdb::Database>,
    frame: Option<Rect>,
    layout: &thinoda_core::Layout,
    zones: &[thinoda_core::Zone],
    scene_options: SvgSceneOptions,
) -> Result<String, IoError> {
    validate_equation_notes(s)?;
    let mut image_dimensions = thinoda_core::images::validate_visual(s).map_err(|error| {
        IoError::Invalid(format!("embedded resources cannot be rendered: {error}"))
    })?;
    image_dimensions.retain(|reference, _| {
        layout.order.iter().any(|id| {
            s.topic(*id).and_then(|topic| topic.image.as_deref()) == Some(reference.as_str())
        })
    });
    let text = text::ExportText::new(s, o.dark, fontdb.clone())?;
    let pad = o.padding.max(0.0);
    let scene = thinoda_core::build_scene(s, layout);
    // Viewports already opt in, but ordinary exports and legacy presentation
    // sources must also honor any clipping frames on the sheet.
    let has_clipped_frames = s.zones.iter().any(|zone| zone.clip);
    // Keep the historical SVG byte-for-byte for source sheets without board
    // features, even when a presentation has filtered its paint sheet.
    let has_board_semantics = scene_options.has_board_semantics;
    let clip_scene = scene_options.clip_scene || has_clipped_frames;
    let topic_clips: HashMap<_, _> = if clip_scene {
        scene
            .items
            .iter()
            .filter_map(|item| match item.id {
                thinoda_core::SceneId::Topic(id) => Some((id, item.clip)),
                _ => None,
            })
            .collect()
    } else {
        HashMap::new()
    };
    // Decorations are not SceneItems. Map edges inherit the child's clip,
    // boundaries the parent's, and summaries the summary topic's. Relationships
    // inherit a clip only when both endpoints belong to the same clipped zone.
    // Look up owners omitted from a filtered presentation layout in the full
    // sheet as well.
    let topic_clip = |id| {
        clip_scene
            .then(|| {
                topic_clips
                    .get(&id)
                    .copied()
                    .unwrap_or_else(|| thinoda_core::scene::topic_clip_rect(s, id))
            })
            .flatten()
    };
    // Canvas objects and rotated cards extend the export; the grid never does.
    let content = match board::extra_bounds(s, &scene) {
        Some(extra) if layout.order.is_empty() && !has_clipped_frames => extra,
        Some(extra) => layout.bounds.union(&extra),
        None => layout.bounds,
    };
    let base_frame = frame.unwrap_or_else(|| content.expand(pad));
    let appendix = equation_appendix(s, layout)?;
    let mut b = if frame.is_none() {
        appendix.as_ref().map_or(base_frame, |appendix| {
            let width = base_frame.size.x.max(appendix.width);
            Rect::from_min_size(
                Vec2::new(base_frame.center().x - width / 2.0, base_frame.min.y),
                Vec2::new(width, base_frame.size.y + appendix.height),
            )
        })
    } else {
        base_frame
    };
    let bg = Theme::for_settings(&s.theme).background(&s.theme, o.dark);
    let opening_tag = |bounds: Rect| {
        let max = bounds.max();
        let width = max.x - bounds.min.x;
        let height = max.y - bounds.min.y;
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{width:.2}\" height=\"{height:.2}\" viewBox=\"{:.2} {:.2} {width:.2} {height:.2}\">",
            bounds.min.x, bounds.min.y
        )
    };
    let measure_visible_bounds = has_clipped_frames && frame.is_none();
    let background = |bounds: Rect| {
        let max = bounds.max();
        let width = max.x - bounds.min.x;
        let height = max.y - bounds.min.y;
        let id = if measure_visible_bounds {
            " id=\"scene-export-background\""
        } else {
            ""
        };
        format!(
            "<rect{id} x=\"{:.2}\" y=\"{:.2}\" width=\"{width:.2}\" height=\"{height:.2}\" fill=\"{}\"/>",
            bounds.min.x,
            bounds.min.y,
            col(bg)
        )
    };
    let opening = opening_tag(b);
    let mut out = opening.clone();
    out.push_str("<defs><marker id=\"arrow\" markerWidth=\"8\" markerHeight=\"8\" refX=\"6\" refY=\"3\" orient=\"auto\"><path d=\"M0,0 L0,6 L7,3 z\" fill=\"#777777\"/></marker></defs>");
    // Embed each referenced blob once, even when thousands of topics share it.
    out.push_str("<defs>");
    let mut image_budget = thinoda_core::resources::MAX_WORKBOOK_RESOURCE_BYTES;
    for (reference, size) in &image_dimensions {
        use base64::Engine;
        let resource = s.resources.get(reference).map_err(IoError::Invalid)?;
        let image = visual_image(resource, &mut image_budget)?;
        let id = reference
            .strip_prefix(thinoda_core::resources::RESOURCE_PREFIX)
            .ok_or_else(|| IoError::Invalid("not an embedded image".into()))?;
        let data = base64::engine::general_purpose::STANDARD.encode(&image.bytes);
        out.push_str(&format!(
            "<image id=\"image-{}\" width=\"{}\" height=\"{}\" xlink:href=\"data:{};base64,{}\"/>",
            esc(id),
            size.x,
            size.y,
            image.media_type,
            data
        ));
    }
    out.push_str("</defs>");
    if !o.transparent {
        out.push_str(&background(b));
    }
    for zone in zones {
        let r = zone.frame;
        let color = zone.color.unwrap_or(Color::rgb(0x5D, 0x7A, 0x91));
        let fill = zone_export_fill(zone);
        let title_color = frame_title_color(color, fill, bg, scene_options.has_board_semantics);
        out.push_str(&format!(
            "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"4\" fill=\"{}\" stroke=\"{}\"/>",
            r.min.x,
            r.min.y,
            r.size.x,
            r.size.y,
            col(fill),
            col(color)
        ));
        out.push_str(&format!(
            "<text x=\"{:.2}\" y=\"{:.2}\" fill=\"{}\" font-size=\"12\">{}</text>",
            r.min.x + 8.0,
            r.min.y + 17.0,
            col(title_color),
            esc(&zone.name)
        ));
    }
    for (item, object) in board::band_items(s, &scene, thinoda_core::SceneBand::BelowMap) {
        let clipped = clip_scene && open_scene_clip(&mut out, item.clip, "object", object.id.0);
        out.push_str(&board_object_svg(
            item,
            object,
            bg,
            o.dark,
            scene_options.visual,
        ));
        if clipped {
            out.push_str("</g>");
        }
    }
    for boundary in &layout.boundaries {
        let r = boundary.rect;
        let source = s.boundaries.iter().find(|source| source.id == boundary.id);
        let clipped = open_scene_clip(
            &mut out,
            source.and_then(|source| topic_clip(source.parent)),
            "boundary",
            boundary.id.0,
        );
        let title = source
            .and_then(|source| source.title.as_deref())
            .unwrap_or("");
        let color = source
            .and_then(|source| source.color)
            .unwrap_or(Color::rgb(0x8A, 0x9B, 0xB0));
        let fill = Color::rgba(
            color.r,
            color.g,
            color.b,
            (color.a as f32 * 0.12).round() as u8,
        );
        out.push_str(&format!("<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"10\" fill=\"{}\" stroke=\"{}\" stroke-dasharray=\"5 4\"/>",r.min.x,r.min.y,r.size.x,r.size.y,col(fill),col(color)));
        if !title.is_empty() {
            out.push_str(&format!(
                "<text x=\"{:.2}\" y=\"{:.2}\" fill=\"#777777\" font-size=\"12\">{}</text>",
                r.min.x + 6.0,
                r.min.y + 14.0,
                esc(title)
            ));
        }
        if clipped {
            out.push_str("</g>");
        }
    }
    for c in &layout.connectors {
        let clipped = open_scene_clip(&mut out, topic_clip(c.child), "map-edge", c.child.0);
        let d = match c.kind {
            ConnectorKind::Straight => format!("M {} L {}", xy(c.start), xy(c.end)),
            ConnectorKind::Elbow
            | ConnectorKind::RoundedElbow
            | ConnectorKind::Tree
            | ConnectorKind::Brace => {
                if c.vertical {
                    let mid = (c.start.y + c.end.y) / 2.0;
                    format!(
                        "M {} L {:.2},{mid:.2} L {:.2},{mid:.2} L {}",
                        xy(c.start),
                        c.start.x,
                        c.end.x,
                        xy(c.end)
                    )
                } else {
                    let mid = (c.start.x + c.end.x) / 2.0;
                    format!(
                        "M {} L {mid:.2},{:.2} L {mid:.2},{:.2} L {}",
                        xy(c.start),
                        c.start.y,
                        c.end.y,
                        xy(c.end)
                    )
                }
            }
            ConnectorKind::Curve => {
                let dx = c.end.x - c.start.x;
                let dy = c.end.y - c.start.y;
                let k = if c.vertical {
                    dy.abs() * 0.5
                } else {
                    dx.abs() * 0.5
                };
                let p1 = if c.vertical {
                    Vec2::new(c.start.x, c.start.y + dy.signum() * k)
                } else {
                    Vec2::new(c.start.x + dx.signum() * k, c.start.y)
                };
                let p2 = if c.vertical {
                    Vec2::new(c.end.x, c.end.y - dy.signum() * k)
                } else {
                    Vec2::new(c.end.x - dx.signum() * k, c.end.y)
                };
                format!("M {} C {} {} {}", xy(c.start), xy(p1), xy(p2), xy(c.end))
            }
        };
        out.push_str(&format!("<path d=\"{d}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{:.2}\" stroke-linecap=\"round\"/>",col(c.color),c.width));
        if clipped {
            out.push_str("</g>");
        }
    }
    let mut relationship_clip_zones = HashMap::new();
    for r in &layout.relationships {
        let source = s.relationships.iter().find(|source| source.id == r.id);
        // Distinct frames can have identical rectangles; compare their stable
        // identities rather than clipping an unrelated cross-frame link.
        let clip = if has_clipped_frames {
            source.and_then(|source| {
                let from = *relationship_clip_zones
                    .entry(source.from)
                    .or_insert_with(|| thinoda_core::scene::topic_clip_zone(s, source.from));
                let to = *relationship_clip_zones
                    .entry(source.to)
                    .or_insert_with(|| thinoda_core::scene::topic_clip_zone(s, source.to));
                match (from, to) {
                    (Some((from_zone, rect)), Some((to_zone, _))) if from_zone == to_zone => {
                        Some(rect)
                    }
                    _ => None,
                }
            })
        } else {
            None
        };
        let clipped = open_scene_clip(&mut out, clip, "relationship", r.id.0);
        let title = source.map(|source| source.title.as_str()).unwrap_or("");
        out.push_str(&format!("<path d=\"M {} C {} {} {}\" fill=\"none\" stroke=\"#777777\" stroke-dasharray=\"5 4\" marker-end=\"url(#arrow)\"/>",xy(r.start),xy(r.c1),xy(r.c2),xy(r.end)));
        if !title.is_empty() {
            out.push_str(&format!(
                "<text x=\"{:.2}\" y=\"{:.2}\" fill=\"#666666\" font-size=\"12\">{}</text>",
                r.mid.x,
                r.mid.y,
                esc(title)
            ));
        }
        if clipped {
            out.push_str("</g>");
        }
    }
    for summary in &layout.summaries {
        let clipped = open_scene_clip(&mut out, topic_clip(summary.topic), "summary", summary.id.0);
        out.push_str(&format!(
            "<path d=\"M {} L {} L {} L {}\" fill=\"none\" stroke=\"#777777\" stroke-width=\"2\"/>",
            xy(summary.start),
            xy(summary.tip),
            xy(summary.end),
            xy(summary.tip)
        ));
        if has_board_semantics && let Some(topic) = layout.rect(summary.topic) {
            let end = match summary.dir {
                Dir::Right => topic.left_center(),
                Dir::Left => topic.right_center(),
                Dir::Down => topic.top_center(),
                Dir::Up => topic.bottom_center(),
            };
            out.push_str(&format!(
                "<path class=\"summary-connector\" d=\"M {} L {}\" fill=\"none\" stroke=\"#777777\" stroke-width=\"2\"/>",
                xy(summary.tip),
                xy(end)
            ));
        }
        if clipped {
            out.push_str("</g>");
        }
    }
    for id in &layout.order {
        let Some(tl) = layout.topics.get(id) else {
            continue;
        };
        let clipped = open_scene_clip(&mut out, topic_clip(*id), "topic", id.0);
        let r = tl.rect;
        let style = &tl.style;
        let card = s
            .topic(*id)
            .filter(|topic| topic.parent.is_none())
            .and_then(|topic| topic.card.as_ref());
        let card_fill = card.and_then(|card| board::card_fill(card, o.dark));
        let text_color = card_fill.map_or(style.text_color, |fill| {
            s.topic(*id)
                .and_then(|topic| topic.style.text_color)
                .unwrap_or_else(|| Color::highest_contrast_on(fill.over(bg)))
        });
        if let Some(card) = card.filter(|card| card.rotation != 0.0) {
            out.push_str(&format!(
                "<g transform=\"rotate({:.2} {:.2} {:.2})\">",
                card.rotation,
                r.center().x,
                r.center().y
            ));
        }
        let fill = style.fill.map(col).unwrap_or_else(|| "none".into());
        let stroke = style
            .border_color
            .map(col)
            .unwrap_or_else(|| col(style.line_color));
        let sw = if style.border_width > 0.0 {
            style.border_width
        } else {
            style.line_width
        };
        let shape = match style.shape {
            TopicShape::Ellipse => format!(
                "<ellipse cx=\"{:.2}\" cy=\"{:.2}\" rx=\"{:.2}\" ry=\"{:.2}\"",
                r.center().x,
                r.center().y,
                r.size.x / 2.0,
                r.size.y / 2.0
            ),
            TopicShape::Diamond => format!(
                "<polygon points=\"{:.2},{:.2} {:.2},{:.2} {:.2},{:.2} {:.2},{:.2}\"",
                r.center().x,
                r.min.y,
                r.max().x,
                r.center().y,
                r.center().x,
                r.max().y,
                r.min.x,
                r.center().y
            ),
            TopicShape::Underline => format!(
                "<path d=\"M {:.2},{:.2} L {:.2},{:.2}\"",
                r.min.x,
                r.max().y,
                r.max().x,
                r.max().y
            ),
            TopicShape::None => format!(
                "<path d=\"M {:.2},{:.2} L {:.2},{:.2}\"",
                r.min.x,
                r.max().y,
                r.max().x,
                r.max().y
            ),
            TopicShape::Rect => format!(
                "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\"",
                r.min.x, r.min.y, r.size.x, r.size.y
            ),
            _ => format!(
                "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"{}\"",
                r.min.x,
                r.min.y,
                r.size.x,
                r.size.y,
                if style.shape == TopicShape::Capsule {
                    r.size.y / 2.0
                } else {
                    6.0
                }
            ),
        };
        if let Some(card) = card.filter(|card| card.kind != thinoda_core::board::CardKind::Image) {
            let (fill, stroke) = match (card.kind, card_fill) {
                (thinoda_core::board::CardKind::Sticky, Some(fill)) => {
                    (col(fill), col(board::card_edge(fill)))
                }
                (_, Some(fill)) => (col(fill), "none".into()),
                (_, None) => ("none".into(), "none".into()),
            };
            out.push_str(&format!(
                "<rect class=\"card\" x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"3\" fill=\"{fill}\" stroke=\"{stroke}\" stroke-width=\"1\"/>",
                r.min.x, r.min.y, r.size.x, r.size.y
            ));
        } else if style.shape == TopicShape::None || style.shape == TopicShape::Underline {
            if style.fill.is_some() {
                out.push_str(&format!("<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"4\" fill=\"{fill}\"/>",
                    r.min.x, r.min.y, r.size.x, r.size.y));
            }
            out.push_str(&format!(
                "{shape} fill=\"none\" stroke=\"{stroke}\" stroke-width=\"{sw:.2}\"/>",
            ));
        } else {
            out.push_str(&format!(
                "{shape} fill=\"{fill}\" stroke=\"{stroke}\" stroke-width=\"{sw:.2}\"/>"
            ));
        }
        if let Some(rect) = tl.image_rect
            && let Some(reference) = s.topic(*id).and_then(|topic| topic.image.as_deref())
        {
            let size = image_dimensions
                .get(reference)
                .ok_or_else(|| IoError::Invalid("missing image dimensions".into()))?;
            let scale = (rect.size.x / size.x).min(rect.size.y / size.y);
            let min = rect.center() - *size * (scale / 2.0);
            let id = reference
                .strip_prefix(thinoda_core::resources::RESOURCE_PREFIX)
                .ok_or_else(|| IoError::Invalid("not an embedded image".into()))?;
            out.push_str(&format!(
                "<use xlink:href=\"#image-{}\" transform=\"translate({:.4} {:.4}) scale({scale:.8})\"/>",
                esc(id), min.x, min.y
            ));
        }
        let block = text.topic(*id);
        let mut y = tl.title_rect.min.y;
        let weight = if style.bold { "bold" } else { "normal" };
        let font = style
            .font_family
            .as_deref()
            .unwrap_or("Segoe UI, Helvetica, Arial, sans-serif");
        let italic = if style.italic {
            " font-style=\"italic\""
        } else {
            ""
        };
        let decoration = match (style.underline, style.strikethrough) {
            (true, true) => " text-decoration=\"underline line-through\"",
            (true, false) => " text-decoration=\"underline\"",
            (false, true) => " text-decoration=\"line-through\"",
            (false, false) => "",
        };
        for (index, line) in block.lines.iter().enumerate() {
            let x = match style.align {
                TextAlign::Left => tl.title_rect.min.x - line.bounds.min.x,
                TextAlign::Center => tl.title_rect.center().x - line.bounds.center().x,
                TextAlign::Right => tl.title_rect.max().x - line.bounds.max().x,
            };
            let baseline = y + (block.line_height - line.bounds.size.y) / 2.0 - line.bounds.min.y;
            // Standalone SVG viewers may substitute fonts; retain the measured width.
            let length = if line.text.is_empty() {
                String::new()
            } else {
                format!(
                    " textLength=\"{:.2}\" lengthAdjust=\"spacingAndGlyphs\"",
                    line.bounds.size.x
                )
            };
            out.push_str(&format!("<text id=\"topic-{id}-line-{index}\" x=\"{x:.2}\" y=\"{baseline:.2}\" font-family=\"{}\" font-size=\"{:.2}\" font-weight=\"{weight}\"{italic}{decoration} fill=\"{}\"{length}>{}</text>",esc(font),style.font_size,col(text_color),esc(&line.text)));
            y += block.line_height;
        }
        if let Some((label, nr)) = &tl.number {
            out.push_str(&format!("<text class=\"topic-number\" x=\"{:.2}\" y=\"{:.2}\" dominant-baseline=\"central\" font-family=\"{}\" font-size=\"{:.2}\" font-weight=\"{weight}\" fill=\"{}\">{}</text>",nr.min.x,nr.center().y,esc(font),style.font_size,col(style.text_color),esc(label)));
        }
        for (index, (mark, mr)) in tl.markers.iter().enumerate() {
            if let Some(color) = s.topics[id].marker_colors.get(&index) {
                out.push_str(&colored_marker_svg(mark, *mr, *color));
                continue;
            }
            let g = thinoda_core::markers::glyph(mark);
            out.push_str(&format!(
                "<text x=\"{:.2}\" y=\"{:.2}\" font-size=\"14\">{}</text>",
                mr.center().x,
                mr.center().y,
                esc(g)
            ));
        }
        for (label, lr) in &tl.labels {
            out.push_str(&format!("<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"5\" fill=\"#EEEEEE\"/><text x=\"{:.2}\" y=\"{:.2}\" font-size=\"10\" text-anchor=\"middle\" fill=\"#333333\">{}</text>",lr.min.x,lr.min.y,lr.size.x,lr.size.y,lr.center().x,lr.center().y,esc(label)));
        }
        if card.is_some_and(|card| card.rotation != 0.0) {
            out.push_str("</g>");
        }
        if clipped {
            out.push_str("</g>");
        }
    }
    for (item, object) in board::band_items(s, &scene, thinoda_core::SceneBand::AboveMap) {
        let clipped = clip_scene && open_scene_clip(&mut out, item.clip, "object", object.id.0);
        out.push_str(&board_object_svg(
            item,
            object,
            bg,
            o.dark,
            scene_options.visual,
        ));
        if clipped {
            out.push_str("</g>");
        }
    }
    if measure_visible_bounds && out.contains("<clipPath id=\"scene-clip-") {
        // The layout's raw topic/decorations bounds can be arbitrarily far outside
        // their owning clips. Besides whitespace, that can make raster exports
        // truncate genuinely visible content at their size limit. Measure the
        // actual un-clipped SVG nodes (including text and rotated shapes); each
        // skipped clip group is bounded by the corresponding zone frame.
        let source = format!("{out}</svg>");
        let visible = zones.iter().fold(
            visible_svg_bounds(&source, b.min, fontdb)?,
            |bounds, zone| Some(bounds.map_or(zone.frame, |bounds| bounds.union(&zone.frame))),
        );
        if let Some(visible) = visible {
            let base_frame = visible.expand(pad);
            let corrected = appendix.as_ref().map_or(base_frame, |appendix| {
                let width = base_frame.size.x.max(appendix.width);
                Rect::from_min_size(
                    Vec2::new(base_frame.center().x - width / 2.0, base_frame.min.y),
                    Vec2::new(width, base_frame.size.y + appendix.height),
                )
            });
            let old_background = background(b);
            let new_background = background(corrected);
            out.replace_range(..opening.len(), &opening_tag(corrected));
            if !o.transparent {
                let start = out
                    .find(&old_background)
                    .ok_or_else(|| IoError::Invalid("SVG bounds: background is missing".into()))?;
                out.replace_range(start..start + old_background.len(), &new_background);
            }
            b = corrected;
        }
    }
    if let Some(appendix) = &appendix {
        let start_y = b.max().y - appendix.height;
        out.push_str("<g class=\"notes-equations\">");
        out.push_str(&format!(
            "<text x=\"{:.2}\" y=\"{:.2}\" font-family=\"sans-serif\" font-size=\"16\" font-weight=\"bold\" fill=\"#555555\">Notes equations</text>",
            b.min.x + 16.0,
            start_y + 22.0
        ));
        let mut y = start_y + 34.0;
        for row in &appendix.rows {
            out.push_str(&format!("<g id=\"equation-{}-title\">", row.topic));
            for (index, line) in row.title.lines.iter().enumerate() {
                let baseline = y
                    + index as f32 * row.title.line_height
                    + (row.title.line_height - line.bounds.size.y) / 2.0
                    - line.bounds.min.y;
                out.push_str(&format!(
                    "<text x=\"{:.2}\" y=\"{baseline:.2}\" font-family=\"{}\" font-size=\"14\" textLength=\"{:.2}\" lengthAdjust=\"spacingAndGlyphs\" fill=\"{}\">{}</text>",
                    b.min.x + 16.0 - line.bounds.min.x,
                    esc(&row.font_family),
                    line.bounds.size.x,
                    col(row.color),
                    esc(&line.text)
                ));
            }
            out.push_str(&format!("</g><g id=\"equation-{}-formula\">", row.topic));
            let origin_x = b.min.x + 16.0;
            let origin_y = y + row.equation_y;
            for primitive in &row.layout.primitives {
                match primitive {
                    EquationPrimitive::Glyph {
                        character,
                        x,
                        y,
                        size,
                    } => out.push_str(&format!(
                        "<text x=\"{:.2}\" y=\"{:.2}\" font-family=\"{}\" font-size=\"{size:.2}\" textLength=\"{:.2}\" lengthAdjust=\"spacingAndGlyphs\" fill=\"{}\">{}</text>",
                        origin_x + x,
                        origin_y + y + size * 0.8,
                        esc(&row.font_family),
                        size * 0.62,
                        col(row.color),
                        esc(&character.to_string())
                    )),
                    EquationPrimitive::Rule { x, y, width } => out.push_str(&format!(
                        "<path d=\"M {:.2},{:.2} L {:.2},{:.2}\" fill=\"none\" stroke=\"{}\" stroke-width=\"1.2\"/>",
                        origin_x + x,
                        origin_y + y,
                        origin_x + x + width,
                        origin_y + y,
                        col(row.color)
                    )),
                }
            }
            out.push_str("</g>");
            y += row.height;
        }
        out.push_str("</g>");
    }
    out.push_str("</svg>");
    Ok(out)
}

/// Bounds actual SVG paint not already constrained by a frame. The root SVG uses
/// equal viewport/viewBox dimensions, so absolute SVG coordinates are translated
/// back to sheet coordinates by its rounded viewBox origin.
fn visible_svg_bounds(
    source: &str,
    origin: Vec2,
    fontdb: std::sync::Arc<resvg::usvg::fontdb::Database>,
) -> Result<Option<Rect>, IoError> {
    use resvg::usvg::{Node, Options, Tree};

    fn include(node: &Node, bounds: &mut Option<Rect>) {
        if node.id() == "scene-export-background" {
            return;
        }
        if let Node::Group(group) = node {
            if group
                .clip_path()
                .is_some_and(|clip| clip.id().starts_with("scene-clip-"))
            {
                return;
            }
            for child in group.children() {
                include(child, bounds);
            }
            return;
        }
        let rect = node.abs_stroke_bounding_box();
        let rect = Rect::from_min_size(
            Vec2::new(rect.x(), rect.y()),
            Vec2::new(rect.width(), rect.height()),
        );
        *bounds = Some(bounds.map_or(rect, |existing| existing.union(&rect)));
    }

    let tree = Tree::from_str(
        source,
        &Options {
            fontdb,
            ..Default::default()
        },
    )
    .map_err(|error| IoError::Invalid(format!("SVG bounds: {error}")))?;
    let mut bounds = None;
    for node in tree.root().children() {
        include(node, &mut bounds);
    }
    let rounded = |n: f32| format!("{n:.2}").parse::<f32>().unwrap_or(n);
    let offset = Vec2::new(rounded(origin.x), rounded(origin.y));
    Ok(bounds.map(|bounds| bounds.translate(offset)))
}

/// Opens a scene-item clip in sheet coordinates. This is a visual clip, not
/// redaction: the SVG can still contain the original geometry and resources.
fn open_scene_clip(out: &mut String, clip: Option<Rect>, kind: &str, id: u64) -> bool {
    let Some(rect) = clip else {
        return false;
    };
    out.push_str(&format!(
        "<defs><clipPath id=\"scene-clip-{kind}-{id}\" clipPathUnits=\"userSpaceOnUse\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/></clipPath></defs><g clip-path=\"url(#scene-clip-{kind}-{id})\">",
        rect.min.x, rect.min.y, rect.size.x, rect.size.y
    ));
    true
}

fn board_object_svg(
    item: &thinoda_core::SceneItem,
    object: &thinoda_core::board::CanvasObject,
    background: Color,
    dark: bool,
    visual: VisualExportContext,
) -> String {
    let mut out = format!("<g class=\"canvas-object\" id=\"object-{}\">", object.id.0);
    for prim in board::object_prims(item, object, background, dark, visual) {
        match prim {
            board::Prim::Path {
                points,
                closed,
                fill,
                stroke,
                round,
            } => {
                let Some(first) = points.first() else {
                    continue;
                };
                let mut d = format!("M {}", xy(*first));
                for point in &points[1..] {
                    d.push_str(&format!(" L {}", xy(*point)));
                }
                if closed {
                    d.push_str(" Z");
                }
                let fill = fill.map_or_else(|| "none".into(), col);
                let stroke = stroke.map_or_else(
                    || "stroke=\"none\"".to_owned(),
                    |(color, width)| {
                        format!("stroke=\"{}\" stroke-width=\"{width:.2}\"", col(color))
                    },
                );
                let caps = if round {
                    " stroke-linecap=\"round\" stroke-linejoin=\"round\""
                } else {
                    " stroke-linejoin=\"round\""
                };
                out.push_str(&format!("<path d=\"{d}\" fill=\"{fill}\" {stroke}{caps}/>"));
            }
            board::Prim::RectangleSides {
                paths,
                stroke,
                width,
            } => {
                let mut d = String::new();
                for path in paths {
                    let Some(first) = path.points.first() else {
                        continue;
                    };
                    d.push_str(&format!(" M {}", xy(*first)));
                    for point in &path.points[1..] {
                        d.push_str(&format!(" L {}", xy(*point)));
                    }
                }
                out.push_str(&format!(
                    "<path d=\"{d}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{width:.2}\" stroke-linecap=\"square\" stroke-linejoin=\"miter\"/>",
                    col(stroke)
                ));
            }
            board::Prim::Circle {
                center,
                radius,
                fill,
                stroke,
                width,
            } => {
                if radius > 0.0 {
                    let fill = fill.map_or_else(|| "none".into(), col);
                    out.push_str(&format!(
                        "<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{radius}\" fill=\"{fill}\" stroke=\"{}\" stroke-width=\"{width:.2}\"/>",
                        center.x, center.y, col(stroke)
                    ));
                }
            }
            board::Prim::Quadratic {
                points,
                stroke,
                width,
            } => {
                out.push_str(&format!(
                    "<path d=\"M {:.2},{:.2} Q {:.2},{:.2} {:.2},{:.2} Q {:.2},{:.2} {:.2},{:.2}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{width:.2}\"/>",
                    points[0].x,
                    points[0].y,
                    points[1].x,
                    points[1].y,
                    points[2].x,
                    points[2].y,
                    points[3].x,
                    points[3].y,
                    points[4].x,
                    points[4].y,
                    col(stroke)
                ));
            }
            board::Prim::Text {
                rect,
                pivot,
                rotation,
                lines,
                size,
                color,
                backdrop,
                group_backdrop,
            } => {
                let c = rect.center();
                if rotation != 0.0 {
                    out.push_str(&format!(
                        "<g transform=\"rotate({rotation:.2} {:.2} {:.2})\">",
                        pivot.x, pivot.y
                    ));
                }
                let grouped = group_backdrop.filter(|_| color.a < 255);
                let (color, backdrop) = if let Some(background) = grouped {
                    out.push_str(&format!(
                        "<g opacity=\"{:.6}\">",
                        f32::from(color.a) / 255.0
                    ));
                    (
                        Color::rgba(color.r, color.g, color.b, 255),
                        Some(background),
                    )
                } else {
                    (color, backdrop)
                };
                if let Some(backdrop) = backdrop {
                    out.push_str(&format!(
                        "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"3\" fill=\"{}\"/>",
                        rect.min.x,
                        rect.min.y,
                        rect.size.x,
                        rect.size.y,
                        col(backdrop)
                    ));
                }
                let line_height = size * 1.25;
                let top = c.y - line_height * lines.len() as f32 / 2.0;
                for (index, line) in lines.iter().enumerate() {
                    out.push_str(&format!(
                        "<text x=\"{:.2}\" y=\"{:.2}\" text-anchor=\"middle\" dominant-baseline=\"central\" font-family=\"Segoe UI, Helvetica, Arial, sans-serif\" font-size=\"{size:.2}\" fill=\"{}\">{}</text>",
                        c.x,
                        top + line_height * (index as f32 + 0.5),
                        col(color),
                        esc(line)
                    ));
                }
                if grouped.is_some() {
                    out.push_str("</g>");
                }
                if rotation != 0.0 {
                    out.push_str("</g>");
                }
            }
        }
    }
    out.push_str("</g>");
    out
}

fn colored_marker_svg(id: &str, r: thinoda_core::Rect, color: Color) -> String {
    use thinoda_core::markers;
    let c = r.center();
    let rad = r.size.x.min(r.size.y) / 2.0;
    let ink = col(color);
    let detail = col(Color::highest_contrast_on(color.over(Color::WHITE)));
    let text = |value: &str, size: f32, fill: &str| {
        format!(
            "<text x=\"{}\" y=\"{}\" text-anchor=\"middle\" dominant-baseline=\"central\" font-family=\"Segoe UI, sans-serif\" font-size=\"{}\" fill=\"{}\">{}</text>",
            c.x,
            c.y,
            size,
            fill,
            esc(value)
        )
    };
    match markers::group_of(id) {
        "priority" => format!(
            "<circle cx=\"{}\" cy=\"{}\" r=\"{rad}\" fill=\"{ink}\"/>{}",
            c.x,
            c.y,
            text(
                id.strip_prefix("priority-").unwrap_or("1"),
                rad * 1.3,
                &detail
            )
        ),
        "task" => {
            let fraction = markers::task_fraction(id);
            let mut svg = format!(
                "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\" stroke=\"{ink}\" stroke-width=\"1.2\"/>",
                c.x,
                c.y,
                rad - 0.5,
                if fraction >= 1.0 { &ink } else { &detail }
            );
            if fraction >= 1.0 {
                svg.push_str(&format!("<polyline points=\"{} {} {}\" fill=\"none\" stroke=\"{detail}\" stroke-width=\"1.5\"/>",
                    xy(c + Vec2::new(-rad * 0.45, 0.0)), xy(c + Vec2::new(-rad * 0.1, rad * 0.35)),
                    xy(c + Vec2::new(rad * 0.5, -rad * 0.35))));
            } else if fraction > 0.0 {
                let steps = (24.0 * fraction).ceil() as usize + 1;
                let mut points = vec![xy(c)];
                for i in 0..=steps {
                    let angle = -std::f32::consts::FRAC_PI_2
                        + std::f32::consts::TAU * fraction * i as f32 / steps as f32;
                    points.push(xy(c + Vec2::new(angle.cos(), angle.sin()) * (rad - 1.5)));
                }
                svg.push_str(&format!(
                    "<polygon points=\"{}\" fill=\"{ink}\"/>",
                    points.join(" ")
                ));
            }
            svg
        }
        "flag" => {
            let x = r.min.x + r.size.x * 0.25;
            format!(
                "<path d=\"M {x},{} L {x},{}\" stroke=\"{ink}\" stroke-width=\"1.4\"/><polygon points=\"{x},{} {},{} {x},{}\" fill=\"{ink}\"/>",
                r.min.y + 1.0,
                r.max().y - 1.0,
                r.min.y + 1.5,
                r.max().x - 1.0,
                r.min.y + r.size.y * 0.3,
                r.min.y + r.size.y * 0.6
            )
        }
        "star" => {
            let points = (0..10)
                .map(|i| {
                    let angle =
                        -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
                    let radius = if i % 2 == 0 { rad } else { rad * 0.45 };
                    xy(c + Vec2::new(angle.cos(), angle.sin()) * radius)
                })
                .collect::<Vec<_>>()
                .join(" ");
            format!("<polygon points=\"{points}\" fill=\"{ink}\"/>")
        }
        _ => text(markers::glyph(id), rad * 1.6, &ink),
    }
}
pub(crate) fn raster(
    w: &Workbook,
    o: &ExportOptions,
    fmt: image::ImageFormat,
    visual: VisualExportContext,
) -> Result<Vec<u8>, IoError> {
    let source = svg(w, o, visual)?;
    raster_svg(&source, o.scale, fmt)
}

pub(crate) fn raster_svg(
    source: &str,
    scale: f32,
    fmt: image::ImageFormat,
) -> Result<Vec<u8>, IoError> {
    let opts = resvg::usvg::Options {
        fontdb: crate::fonts::database(),
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_str(source, &opts)
        .map_err(|e| IoError::Invalid(format!("SVG parse: {e}")))?;
    let scale = if scale.is_finite() {
        scale.clamp(0.1, 8.0)
    } else {
        1.0
    };
    let size = tree.size();
    let width = (size.width() * scale).ceil().clamp(1.0, 16_384.0) as u32;
    let height = (size.height() * scale).ceil().clamp(1.0, 16_384.0) as u32;
    let mut pix = resvg::tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| IoError::Invalid("image dimensions exceed limits".into()))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pix.as_mut(),
    );
    let mut rgba = pix.data().to_vec();
    // JPEG has no alpha; its existing path composites over black.
    if fmt != image::ImageFormat::Jpeg {
        unpremultiply_rgba(&mut rgba);
    }
    let mut img = image::RgbaImage::from_raw(width, height, rgba)
        .ok_or_else(|| IoError::Invalid("invalid rendered image".into()))?;
    let mut out = Cursor::new(Vec::new());
    if fmt == image::ImageFormat::Jpeg {
        let rgb = image::DynamicImage::ImageRgba8(img).to_rgb8();
        image::DynamicImage::ImageRgb8(rgb)
            .write_to(&mut out, fmt)
            .map_err(|e| IoError::Invalid(e.to_string()))?;
    } else {
        image::DynamicImage::ImageRgba8(std::mem::take(&mut img))
            .write_to(&mut out, fmt)
            .map_err(|e| IoError::Invalid(e.to_string()))?;
    }
    Ok(out.into_inner())
}

pub(crate) fn unpremultiply_rgba(rgba: &mut [u8]) {
    for pixel in rgba.as_chunks_mut::<4>().0 {
        let alpha = u16::from(pixel[3]);
        if alpha > 0 && alpha < 255 {
            for channel in &mut pixel[..3] {
                *channel = ((u16::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
            }
        }
    }
}

pub(crate) fn pdf(
    w: &Workbook,
    o: &ExportOptions,
    visual: VisualExportContext,
) -> Result<Vec<u8>, IoError> {
    let mut opts = o.clone();
    opts.transparent = false;
    let source = svg(w, &opts, visual)?;
    pdf_svg(&source)
}

pub(crate) fn pdf_svg(source: &str) -> Result<Vec<u8>, IoError> {
    let tree = pdf_svg_tree(source)?;
    svg2pdf::to_pdf(&tree, Default::default(), Default::default())
        .map_err(|e| IoError::Invalid(format!("PDF conversion: {e}")))
}

fn pdf_svg_tree(source: &str) -> Result<svg2pdf::usvg::Tree, IoError> {
    let tree = resvg::usvg::Tree::from_str(
        source,
        &resvg::usvg::Options {
            fontdb: crate::fonts::database(),
            ..Default::default()
        },
    )
    .map_err(|e| IoError::Invalid(format!("SVG parse: {e}")))?;
    // Outline with the same font resolver as raster export. The PDF converter's
    // usvg version differs, so pass normalized SVG rather than incompatible trees.
    let outlined = tree.to_string(&resvg::usvg::WriteOptions::default());
    svg2pdf::usvg::Tree::from_str(&outlined, &Default::default())
        .map_err(|e| IoError::Invalid(format!("PDF SVG parse: {e}")))
}

/// Compose independent vector SVG form XObjects into a multipage PDF. Text is
/// outlined with the same fonts as SVG exports; only photo content is rasterized.
pub(crate) fn pdf_svg_pages(
    pages: &[(String, f32, f32)],
    limit: usize,
) -> Result<Vec<u8>, IoError> {
    use pdf_writer::{Content, Finish, Name, Pdf, Ref};
    use std::collections::HashMap;

    let mut alloc = Ref::new(1);
    let catalog = alloc.bump();
    let page_tree = alloc.bump();
    let page_ids: Vec<_> = pages.iter().map(|_| alloc.bump()).collect();
    let mut pdf = Pdf::new();
    pdf.catalog(catalog).pages(page_tree);
    pdf.pages(page_tree)
        .kids(page_ids.iter().copied())
        .count(pages.len() as i32);
    for ((source, width, height), page_id) in pages.iter().zip(page_ids) {
        let tree = pdf_svg_tree(source)?;
        let (chunk, object) = svg2pdf::to_chunk(&tree, Default::default())
            .map_err(|e| IoError::Invalid(format!("PDF conversion: {e}")))?;
        let mut references = HashMap::new();
        let chunk = chunk.renumber(|old| *references.entry(old).or_insert_with(|| alloc.bump()));
        let object = references[&object];
        let content_id = alloc.bump();
        let name = Name(b"Slide");
        let mut page = pdf.page(page_id);
        page.parent(page_tree);
        page.media_box(pdf_writer::Rect::new(0.0, 0.0, *width, *height));
        page.contents(content_id);
        page.resources().x_objects().pair(name, object);
        page.finish();
        let mut content = Content::new();
        content
            .transform([*width, 0.0, 0.0, *height, 0.0, 0.0])
            .x_object(name);
        pdf.stream(content_id, &content.finish());
        pdf.extend(&chunk);
        if pdf.as_bytes().len() > limit {
            return Err(IoError::PresentationLimit("PDF size"));
        }
    }
    let bytes = pdf.finish();
    if bytes.len() > limit {
        return Err(IoError::PresentationLimit("PDF size"));
    }
    Ok(bytes)
}

/// Wraps an opaque JPEG in a single PDF page. Page dimensions are PDF points,
/// independent of raster resolution (printing uses physical paper dimensions).
pub(crate) fn image_pdf(
    jpg: &[u8],
    width: u32,
    height: u32,
    page_width: f32,
    page_height: f32,
) -> Vec<u8> {
    image_pdf_pages(&[PdfImagePage {
        jpg,
        width,
        height,
        page_width,
        page_height,
    }])
}

pub(crate) struct PdfImagePage<'a> {
    pub jpg: &'a [u8],
    pub width: u32,
    pub height: u32,
    pub page_width: f32,
    pub page_height: f32,
}

pub(crate) fn image_pdf_pages(pages: &[PdfImagePage<'_>]) -> Vec<u8> {
    let mut objects = Vec::with_capacity(2 + pages.len() * 3);
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    let page_refs = (0..pages.len())
        .map(|index| format!("{} 0 R", 3 + index * 3))
        .collect::<Vec<_>>()
        .join(" ");
    objects.push(
        format!(
            "<< /Type /Pages /Kids [{page_refs}] /Count {} >>",
            pages.len()
        )
        .into_bytes(),
    );
    for (index, page) in pages.iter().enumerate() {
        let page_id = 3 + index * 3;
        let image_id = page_id + 1;
        let content_id = page_id + 2;
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.3} {:.3}] /Resources << /XObject << /Im0 {image_id} 0 R >> >> /Contents {content_id} 0 R >>",
            page.page_width, page.page_height
        ).into_bytes());
        let mut image_obj = format!(
            "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
            page.width,
            page.height,
            page.jpg.len()
        )
        .into_bytes();
        image_obj.extend_from_slice(page.jpg);
        image_obj.extend_from_slice(b"\nendstream");
        objects.push(image_obj);
        let stream = format!(
            "q {:.3} 0 0 {:.3} 0 0 cm /Im0 Do Q",
            page.page_width, page.page_height
        );
        objects.push(
            format!(
                "<< /Length {} >>\nstream\n{}\nendstream",
                stream.len(),
                stream
            )
            .into_bytes(),
        );
    }
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = vec![0usize];
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        pdf.extend_from_slice(obj);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref = pdf.len();
    pdf.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for off in offsets.iter().skip(1) {
        pdf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF",
            objects.len() + 1
        )
        .as_bytes(),
    );
    pdf
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use thinoda_core::{Rect, Sheet};

    #[test]
    fn partial_rectangle_svg_pixels_match_the_published_open_path() {
        use thinoda_core::{
            ApproxMeasure, CanvasObject, RectangleSides, SceneId, ShapeKind, SheetRootMode,
            board::rotated_corners, scene::build_scene,
        };

        for (width, height) in [
            (40.0, 30.0),
            (2.0, 3.0),
            (8.0, 8.0),
            (8.0, 2.0),
            (2.0, 8.0),
            (8.0, 30.0),
            (40.0, 8.0),
            (0.0, 40.0),
            (40.0, 0.0),
            (0.0, 0.0),
        ] {
            for rotation in [0.0, 37.0, 90.0] {
                for mask in 0..16 {
                    for color in [Color::rgb(18, 100, 210), Color::rgba(18, 100, 210, 128)] {
                        let sides = RectangleSides {
                            top: mask & 1 != 0,
                            right: mask & 2 != 0,
                            bottom: mask & 4 != 0,
                            left: mask & 8 != 0,
                        };
                        let frame =
                            Rect::from_min_size(Vec2::new(40.0, 40.0), Vec2::new(width, height));
                        let mut object = CanvasObject::shape(ShapeKind::Rectangle, frame);
                        object.rectangle_sides = Some(sides);
                        object.style.stroke = Some(color);
                        object.style.stroke_width = 8.0;
                        object.rotation = rotation;
                        let mut sheet = Sheet::new("Board", "");
                        sheet.root_mode = SheetRootMode::Hidden;
                        let id = sheet.add_canvas_object(object).unwrap();
                        let layout = layout_sheet(&sheet, &ApproxMeasure, false);
                        let scene = build_scene(&sheet, &layout);
                        let item = scene
                            .items
                            .iter()
                            .find(|item| item.id == SceneId::Object(id))
                            .unwrap();
                        let actual = board_object_svg(
                            item,
                            sheet.canvas.object(id).unwrap(),
                            Color::WHITE,
                            false,
                            VisualExportContext::default(),
                        );

                        let corners = rotated_corners(frame, rotation);
                        let visible = [sides.top, sides.right, sides.bottom, sides.left];
                        let mut path = format!("M {}", xy(corners[0]));
                        for (index, side) in visible.into_iter().enumerate() {
                            if index < 3 || side {
                                path.push_str(&format!(
                                    " {} {}",
                                    if side { "L" } else { "M" },
                                    xy(corners[(index + 1) % 4])
                                ));
                            }
                        }
                        let expected = format!(
                            "<path d=\"{path}\" fill=\"none\" stroke=\"{}\" stroke-width=\"8\" stroke-linecap=\"square\" stroke-linejoin=\"miter\"/>",
                            col(color)
                        );
                        let wrap = |body: &str| {
                            format!(
                                "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"128\" height=\"128\">{body}</svg>"
                            )
                        };
                        let pixels = |svg: &str| {
                            image::load_from_memory(
                                &raster_svg(svg, 1.0, image::ImageFormat::Png).unwrap(),
                            )
                            .unwrap()
                            .to_rgba8()
                        };
                        let actual_pixels = pixels(&wrap(&actual));
                        let expected_pixels = pixels(&wrap(&expected));
                        let premultiply = |pixel: image::Rgba<u8>| {
                            let alpha = u16::from(pixel[3]);
                            [
                                (u16::from(pixel[0]) * alpha + 127) / 255,
                                (u16::from(pixel[1]) * alpha + 127) / 255,
                                (u16::from(pixel[2]) * alpha + 127) / 255,
                                alpha,
                            ]
                        };
                        let mut changed = 0usize;
                        let mut max_delta = 0;
                        let mut examples = Vec::new();
                        for (x, y, actual) in actual_pixels.enumerate_pixels() {
                            let expected = *expected_pixels.get_pixel(x, y);
                            let delta = premultiply(*actual)
                                .into_iter()
                                .zip(premultiply(expected))
                                .map(|(a, b)| a.abs_diff(b))
                                .max()
                                .unwrap();
                            if delta > 0 {
                                changed += 1;
                                max_delta = max_delta.max(delta);
                                if examples.len() < 8 {
                                    examples.push((x, y, *actual, expected));
                                }
                            }
                        }
                        // resvg can vary by up to 16/255 in one antialiased
                        // pixel when a move-only subpath is omitted.
                        assert!(
                            changed <= 1 && max_delta <= 16,
                            "{width}x{height}, {rotation}, mask {mask:04b}, color {color:?}: {changed} pixels, max delta {max_delta}\nactual: {actual}\nexpected: {expected}\nfirst differences: {examples:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn transparent_png_uses_straight_alpha_for_color_channels() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="3" height="1">
            <rect x="0" width="1" height="1" fill="#FF8000" fill-opacity="0.5"/>
            <rect x="1" width="1" height="1" fill="#14283C"/>
        </svg>"##;
        let png = raster_svg(svg, 1.0, image::ImageFormat::Png).unwrap();
        let pixels = image::load_from_memory(&png).unwrap().to_rgba8();
        let translucent = pixels.get_pixel(0, 0).0;
        assert_eq!(translucent[0], 255);
        assert!((127..=129).contains(&translucent[1]));
        assert_eq!(translucent[2], 0);
        assert!((127..=128).contains(&translucent[3]));
        assert_eq!(pixels.get_pixel(1, 0).0, [20, 40, 60, 255]);
        assert_eq!(pixels.get_pixel(2, 0).0, [0, 0, 0, 0]);
    }

    #[test]
    fn visual_image_budget_is_cumulative_and_does_not_change_on_failure() {
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(8, 4)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let bytes = bytes.into_inner();
        let resource =
            thinoda_core::resources::Resource::new("identity.png", "image/png", bytes.clone())
                .unwrap();
        let mut remaining = bytes.len() * 2 - 1;
        let image = visual_image(&resource, &mut remaining).unwrap();
        assert_eq!(image.bytes.as_ref(), bytes);
        assert_eq!(remaining, bytes.len() - 1);
        let before = remaining;
        assert!(visual_image(&resource, &mut remaining).is_err());
        assert_eq!(remaining, before);
    }

    #[test]
    fn visual_image_budget_limits_normalization_before_allocation_grows() {
        use image::ImageEncoder;

        let exif = [
            b'I', b'I', 42, 0, 8, 0, 0, 0, 1, 0, 0x12, 0x01, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0,
            0,
        ];
        let mut bytes = Vec::new();
        let mut encoder = image::codecs::png::PngEncoder::new(&mut bytes);
        encoder.set_exif_metadata(exif.to_vec()).unwrap();
        encoder
            .write_image(&[127; 8 * 4 * 4], 8, 4, image::ExtendedColorType::Rgba8)
            .unwrap();
        let resource =
            thinoda_core::resources::Resource::new("camera.png", "image/png", bytes).unwrap();
        let normalized = thinoda_core::images::normalized_visual_image(resource.bytes(), 1024)
            .unwrap()
            .unwrap();
        let mut too_small = normalized.len() - 1;
        assert!(visual_image(&resource, &mut too_small).is_err());
        assert_eq!(too_small, normalized.len() - 1);
        let mut exact = normalized.len();
        let image = visual_image(&resource, &mut exact).unwrap();
        assert_eq!(image.bytes.as_ref(), normalized);
        assert_eq!(exact, 0);
    }

    #[test]
    fn svg_escapes_topic_text() {
        let w = Workbook::new(thinoda_core::Sheet::new("S", "<root>"));
        let xml = svg(
            &w,
            &ExportOptions::default(),
            VisualExportContext::default(),
        )
        .unwrap();
        assert!(xml.contains("&lt;root&gt;"));
    }

    #[test]
    fn plain_map_with_summary_matches_prior_svg_bytes() {
        let mut sheet = Sheet::new("Legacy", "Root");
        let first = sheet.add_child(sheet.root, None, "First");
        let second = sheet.add_child(sheet.root, None, "Second");
        sheet.add_summary(&[first, second], "Summary").unwrap();
        assert!(!crate::board_export::uses_board_semantics(&sheet));

        // Captured from the pre-change renderer at 65eacfcb with only embedded
        // fallback fonts; this is a complete SVG, not a comparison of two
        // paths through the current renderer.
        let expected = concat!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="561.00" height="122.00" viewBox="-228.00 -61.00 561.00 122.00">"##,
            r##"<defs><marker id="arrow" markerWidth="8" markerHeight="8" refX="6" refY="3" orient="auto"><path d="M0,0 L0,6 L7,3 z" fill="#777777"/></marker></defs><defs></defs>"##,
            r##"<rect x="-228.00" y="-61.00" width="561.00" height="122.00" fill="#FFFFFF"/>"##,
            r##"<path d="M 57.00,0.00 C 85.00,0.00 85.00,0.00 113.00,0.00" fill="none" stroke="#FF6B6B" stroke-width="3.00" stroke-linecap="round"/>"##,
            r##"<path d="M -57.00,0.00 C -85.00,0.00 -85.00,0.00 -113.00,0.00" fill="none" stroke="#FFA26B" stroke-width="3.00" stroke-linecap="round"/>"##,
            r##"<path d="M 189.00,-22.50 L 203.00,0.00 L 189.00,22.50 L 203.00,0.00" fill="none" stroke="#777777" stroke-width="2"/>"##,
            r##"<rect x="-57.00" y="-37.00" width="114.00" height="74.00" rx="6" fill="#2E3440" stroke="#2E3440" stroke-width="2.00"/>"##,
            r##"<text id="topic-1-line-0" x="-33.00" y="10.71" font-family="Segoe UI, Helvetica, Arial, sans-serif" font-size="30.00" font-weight="bold" fill="#FFFFFF" textLength="65.07" lengthAdjust="spacingAndGlyphs">Root</text>"##,
            r##"<rect x="113.00" y="-22.50" width="68.00" height="45.00" rx="6" fill="#FF6B6B" stroke="#FF6B6B" stroke-width="2.00"/>"##,
            r##"<text id="topic-2-line-0" x="129.00" y="6.53" font-family="Segoe UI, Helvetica, Arial, sans-serif" font-size="18.00" font-weight="normal" fill="#FFFFFF" textLength="35.03" lengthAdjust="spacingAndGlyphs">First</text>"##,
            r##"<rect x="-204.00" y="-22.50" width="91.00" height="45.00" rx="6" fill="#FFA26B" stroke="#FFA26B" stroke-width="2.00"/>"##,
            r##"<text id="topic-3-line-0" x="-188.00" y="6.53" font-family="Segoe UI, Helvetica, Arial, sans-serif" font-size="18.00" font-weight="normal" fill="#1F2328" textLength="58.66" lengthAdjust="spacingAndGlyphs">Second</text>"##,
            r##"<rect x="213.00" y="-20.00" width="96.00" height="40.00" rx="6" fill="#FFD3D3" stroke="#FF6B6B" stroke-width="2.00"/>"##,
            r##"<text id="topic-4-line-0" x="227.00" y="5.91" font-family="Segoe UI, Helvetica, Arial, sans-serif" font-size="16.00" font-weight="normal" fill="#1F2328" textLength="67.04" lengthAdjust="spacingAndGlyphs">Summary</text></svg>"##,
        );
        let fontdb = Arc::new(crate::fonts::build(false));
        let actual = svg_sheet_with_fonts(
            &sheet,
            &ExportOptions::default(),
            fontdb.clone(),
            None,
            VisualExportContext::default(),
        )
        .unwrap();
        assert_eq!(actual, expected);

        // A board setting changes no other paint, but enables the additional
        // summary connector even when there are no clipping frames.
        sheet.board.snap = !sheet.board.snap;
        assert!(crate::board_export::uses_board_semantics(&sheet));
        let board = svg_sheet_with_fonts(
            &sheet,
            &ExportOptions::default(),
            fontdb,
            None,
            VisualExportContext::default(),
        )
        .unwrap();
        let connector = r##"<path class="summary-connector" d="M 203.00,0.00 L 213.00,0.00" fill="none" stroke="#777777" stroke-width="2"/>"##;
        assert!(board.contains(connector));
        assert_eq!(board.replacen(connector, "", 1), expected);
    }

    #[test]
    fn plain_map_with_legacy_zone_matches_prior_svg_bytes() {
        let mut sheet = Sheet::new("Legacy zones", "Root");
        let floating = sheet.add_floating("Floating", Vec2::new(220.0, 80.0));
        sheet
            .create_zone(
                "Legacy frame",
                vec![floating],
                Rect::from_min_size(Vec2::new(190.0, 40.0), Vec2::new(180.0, 120.0)),
            )
            .unwrap();
        // Legacy files carry no zone colour; new zones get an automatic accent.
        sheet.zones[0].color = None;
        assert!(!sheet.uses_board_semantics());
        let fontdb = Arc::new(crate::fonts::build(false));
        let actual = svg_sheet_with_fonts(
            &sheet,
            &ExportOptions::default(),
            fontdb,
            None,
            VisualExportContext::default(),
        )
        .unwrap();

        // Captured before the board-title contrast change with only embedded
        // fallback fonts. Check the complete legacy output, including its tint.
        let expected = concat!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="475.00" height="245.00" viewBox="-81.00 -61.00 475.00 245.00">"##,
            r##"<defs><marker id="arrow" markerWidth="8" markerHeight="8" refX="6" refY="3" orient="auto"><path d="M0,0 L0,6 L7,3 z" fill="#777777"/></marker></defs><defs></defs>"##,
            r##"<rect x="-81.00" y="-61.00" width="475.00" height="245.00" fill="#FFFFFF"/>"##,
            r##"<rect x="190.00" y="40.00" width="180.00" height="120.00" rx="4" fill="#5D7A9114" stroke="#5D7A91"/>"##,
            r##"<text x="198.00" y="57.00" fill="#5D7A91" font-size="12">Legacy frame</text>"##,
            r##"<rect x="-57.00" y="-37.00" width="114.00" height="74.00" rx="6" fill="#2E3440" stroke="#2E3440" stroke-width="2.00"/>"##,
            r##"<text id="topic-1-line-0" x="-33.00" y="10.71" font-family="Segoe UI, Helvetica, Arial, sans-serif" font-size="30.00" font-weight="bold" fill="#FFFFFF" textLength="65.07" lengthAdjust="spacingAndGlyphs">Root</text>"##,
            r##"<rect x="176.50" y="60.00" width="87.00" height="40.00" rx="6" fill="#FFD3D3" stroke="#FF6B6B" stroke-width="2.00"/>"##,
            r##"<text id="topic-2-line-0" x="190.50" y="85.91" font-family="Segoe UI, Helvetica, Arial, sans-serif" font-size="16.00" font-weight="normal" fill="#1F2328" textLength="58.11" lengthAdjust="spacingAndGlyphs">Floating</text></svg>"##,
        );
        assert_eq!(actual, expected);
    }

    #[test]
    fn clipping_opt_in_preserves_bytes_without_clipped_frames() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, Card, CardKind, Endpoint, ShapeKind, board::Band,
        };

        let mut sheet = Sheet::new("Legacy", "Root");
        let branch = sheet.add_child(sheet.root, None, "Branch");
        let first = sheet.add_child(branch, None, "First");
        let second = sheet.add_child(branch, None, "Second");
        sheet.add_boundary(&[first, second]).unwrap();
        sheet.add_summary(&[first, second], "Summary").unwrap();
        sheet.add_relationship(first, second).unwrap();
        let mut card = Card::new(CardKind::Sticky, Vec2::new(120.0, 80.0));
        card.rotation = 15.0;
        let floating = sheet
            .add_card("Card", Vec2::new(500.0, 100.0), card)
            .unwrap();
        sheet
            .create_zone(
                "Unclipped frame",
                vec![floating],
                Rect::from_min_size(Vec2::new(450.0, 30.0), Vec2::new(100.0, 140.0)),
            )
            .unwrap();
        let mut object = CanvasObject::shape(
            ShapeKind::Diamond,
            Rect::from_min_size(Vec2::new(520.0, 60.0), Vec2::new(60.0, 40.0)),
        );
        object.band = Band::Below;
        let object = sheet.add_canvas_object(object).unwrap();
        sheet
            .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
                Endpoint::Topic(floating),
                Endpoint::Object(object),
            )))
            .unwrap();
        sheet.validate().unwrap();

        let opts = ExportOptions::default();
        let fontdb = Arc::new(crate::fonts::build(false));
        let text = text::ExportText::new(&sheet, opts.dark, fontdb.clone()).unwrap();
        let layout = layout_sheet(&sheet, &text, opts.dark);
        for frame in [
            None,
            Some(Rect::from_min_size(
                Vec2::new(-200.0, -200.0),
                Vec2::new(800.0, 600.0),
            )),
        ] {
            let without_clipping = svg_layout_with_fonts(
                &sheet,
                &opts,
                fontdb.clone(),
                frame,
                &layout,
                &sheet.zones,
                SvgSceneOptions::new(&sheet, false, VisualExportContext::default()),
            )
            .unwrap();
            let with_clipping = svg_layout_with_fonts(
                &sheet,
                &opts,
                fontdb.clone(),
                frame,
                &layout,
                &sheet.zones,
                SvgSceneOptions::new(&sheet, true, VisualExportContext::default()),
            )
            .unwrap();
            // Compare the current paths, not a historical renderer fixture.
            assert_eq!(without_clipping, with_clipping);
            if frame.is_none() {
                assert_eq!(
                    without_clipping,
                    svg_sheet_with_fonts(
                        &sheet,
                        &opts,
                        fontdb.clone(),
                        None,
                        VisualExportContext::default(),
                    )
                    .unwrap()
                );
                for format in [image::ImageFormat::Png, image::ImageFormat::Jpeg] {
                    assert_eq!(
                        raster_svg(&without_clipping, opts.scale, format).unwrap(),
                        raster_svg(&with_clipping, opts.scale, format).unwrap()
                    );
                }
                // Identical SVG is the stable PDF input: svg2pdf may serialize
                // resource dictionaries in different orders on repeated calls.
                assert!(pdf_svg(&without_clipping).unwrap().starts_with(b"%PDF-"));
                let settings = crate::print::PrintSettings::default();
                assert_eq!(
                    crate::print::render_print_svg(&without_clipping, settings)
                        .unwrap()
                        .image,
                    crate::print::render_print_svg(&with_clipping, settings)
                        .unwrap()
                        .image
                );
            }
        }
    }

    #[test]
    fn summary_topic_connectors_follow_direction_and_frame_clipping() {
        use thinoda_core::{
            SheetRootMode,
            layout::{Dir, Layout, SummaryLayout},
        };

        let rect = |x, y, w, h| Rect::from_min_size(Vec2::new(x, y), Vec2::new(w, h));
        let mut sheet = Sheet::new("Summary", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let root = sheet.add_floating("Root", Vec2::new(40.0, 40.0));
        let first = sheet.add_child(root, None, "First");
        let second = sheet.add_child(root, None, "Second");
        let topic = sheet.add_summary(&[first, second], "Summary").unwrap();
        let summary_id = sheet.summaries[0].id;
        let zone = sheet
            .create_zone("Frame", vec![root], rect(0.0, 0.0, 100.0, 100.0))
            .unwrap();
        sheet.zones.iter_mut().find(|z| z.id == zone).unwrap().clip = true;
        let mut unclipped_sheet = sheet.clone();
        unclipped_sheet.zones[0].clip = false;

        let options = ExportOptions {
            transparent: true,
            ..Default::default()
        };
        let frame = rect(-60.0, -60.0, 260.0, 260.0);
        let fontdb = Arc::new(crate::fonts::build(false));
        let text = text::ExportText::new(&sheet, options.dark, fontdb.clone()).unwrap();
        let topic_layout = layout_sheet(&sheet, &text, options.dark).topics[&topic].clone();
        let render = |source: &Sheet, layout: &Layout| {
            let svg = svg_layout_with_fonts(
                source,
                &options,
                fontdb.clone(),
                Some(frame),
                layout,
                &source.zones,
                SvgSceneOptions::new(source, true, VisualExportContext::default()),
            )
            .unwrap();
            let image =
                image::load_from_memory(&raster_svg(&svg, 1.0, image::ImageFormat::Png).unwrap())
                    .unwrap()
                    .to_rgba8();
            (svg, image)
        };
        let (_, baseline) = render(&sheet, &Layout::default());
        let pixel = |image: &image::RgbaImage, point: Vec2| {
            *image.get_pixel(
                (point.x - frame.min.x) as u32,
                (point.y - frame.min.y) as u32,
            )
        };
        let cases = [
            (
                Dir::Right,
                rect(120.0, 50.0, 20.0, 20.0),
                Vec2::new(70.0, 60.0),
                Vec2::new(120.0, 60.0),
                Vec2::new(80.0, 60.0),
                Vec2::new(110.0, 60.0),
            ),
            (
                Dir::Left,
                rect(-40.0, 50.0, 20.0, 20.0),
                Vec2::new(30.0, 60.0),
                Vec2::new(-20.0, 60.0),
                Vec2::new(10.0, 60.0),
                Vec2::new(-10.0, 60.0),
            ),
            (
                Dir::Down,
                rect(50.0, 120.0, 20.0, 20.0),
                Vec2::new(60.0, 70.0),
                Vec2::new(60.0, 120.0),
                Vec2::new(60.0, 80.0),
                Vec2::new(60.0, 110.0),
            ),
            (
                Dir::Up,
                rect(50.0, -40.0, 20.0, 20.0),
                Vec2::new(60.0, 30.0),
                Vec2::new(60.0, -20.0),
                Vec2::new(60.0, 10.0),
                Vec2::new(60.0, -10.0),
            ),
        ];
        let mut layout = Layout::default();
        layout.topics.insert(topic, topic_layout);
        for (dir, topic_rect, tip, end, inside, outside) in cases {
            layout.topics.get_mut(&topic).unwrap().rect = topic_rect;
            layout.summaries = vec![SummaryLayout {
                id: summary_id,
                topic,
                dir,
                range_rect: rect(0.0, 0.0, 1.0, 1.0),
                start: tip,
                end: tip,
                tip,
            }];
            let (svg, clipped) = render(&sheet, &layout);
            let expected = format!(
                "<path class=\"summary-connector\" d=\"M {} L {}\"",
                xy(tip),
                xy(end)
            );
            let group = format!(
                "<g clip-path=\"url(#scene-clip-summary-{})\">",
                summary_id.0
            );
            let body = svg
                .split_once(&group)
                .unwrap()
                .1
                .split_once("</g>")
                .unwrap()
                .0;
            assert!(body.contains(&expected), "{dir:?}: missing connector");
            assert_ne!(pixel(&clipped, inside), pixel(&baseline, inside), "{dir:?}");
            assert_eq!(
                pixel(&clipped, outside),
                pixel(&baseline, outside),
                "{dir:?}"
            );

            let (svg, unclipped) = render(&unclipped_sheet, &layout);
            assert!(svg.contains(&expected));
            assert_ne!(
                pixel(&unclipped, outside),
                pixel(&baseline, outside),
                "{dir:?}: un-clipped connector disappeared"
            );
        }

        layout.topics.get_mut(&topic).unwrap().rect = rect(170.0, 50.0, 20.0, 20.0);
        layout.summaries[0].dir = Dir::Right;
        layout.summaries[0].tip = Vec2::new(120.0, 60.0);
        layout.summaries[0].start = layout.summaries[0].tip;
        layout.summaries[0].end = layout.summaries[0].tip;
        let outside = Vec2::new(150.0, 60.0);
        let (_, clipped) = render(&sheet, &layout);
        let (_, unclipped) = render(&unclipped_sheet, &layout);
        assert_eq!(pixel(&clipped, outside), pixel(&baseline, outside));
        assert_ne!(pixel(&unclipped, outside), pixel(&baseline, outside));

        layout.topics.clear();
        let (svg, _) = render(&sheet, &layout);
        assert!(!svg.contains("class=\"summary-connector\""));
        let summary = &layout.summaries[0];
        assert!(svg.contains(&format!(
            "<path d=\"M {} L {} L {} L {}\"",
            xy(summary.start),
            xy(summary.tip),
            xy(summary.end),
            xy(summary.tip)
        )));
    }

    #[test]
    fn decoration_clips_follow_topic_ownership_and_preserve_cross_frame_links() {
        use thinoda_core::{
            SheetRootMode,
            layout::{BoundaryLayout, Connector, Dir, Layout, RelationshipLayout, SummaryLayout},
        };

        let rect = |x, y, w, h| Rect::from_min_size(Vec2::new(x, y), Vec2::new(w, h));
        let mut sheet = Sheet::new("Decorations", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let root = sheet.add_floating("Root", Vec2::new(40.0, 40.0));
        let first = sheet.add_child(root, None, "First");
        let second = sheet.add_child(root, None, "Second");
        let summary = sheet.add_summary(&[first, second], "Summary").unwrap();
        let hidden_summary = sheet.add_summary(&[first, second], "Hidden").unwrap();
        let summary_id = sheet.summaries[0].id;
        let hidden_summary_id = sheet.summaries[1].id;
        let boundary = sheet.add_boundary(&[first, second]).unwrap();
        let hidden_boundary = sheet.add_boundary(&[root]).unwrap();
        sheet.boundaries[0].color = Some(Color::rgb(235, 30, 20));
        sheet.boundaries[1].color = Some(Color::rgb(235, 30, 20));

        let other = sheet.add_floating("Other frame", Vec2::new(260.0, 40.0));
        let same_rect = sheet.add_floating("Same rectangle", Vec2::new(50.0, 40.0));
        let free = sheet.add_floating("Free", Vec2::new(150.0, 40.0));
        let free_other = sheet.add_floating("Free other", Vec2::new(180.0, 40.0));
        for (root, frame) in [
            (root, rect(0.0, 0.0, 100.0, 250.0)),
            (other, rect(220.0, 0.0, 100.0, 250.0)),
            (same_rect, rect(0.0, 0.0, 100.0, 250.0)),
        ] {
            let zone = sheet.create_zone("Frame", vec![root], frame).unwrap();
            sheet.zones.iter_mut().find(|z| z.id == zone).unwrap().clip = true;
        }
        let same = sheet.add_relationship(first, second).unwrap();
        let same_rect_link = sheet.add_relationship(first, same_rect).unwrap();
        let fully_clipped = sheet.add_relationship(first, second).unwrap();
        let cross = sheet.add_relationship(first, other).unwrap();
        let mixed = sheet.add_relationship(first, free).unwrap();
        let unowned = sheet.add_relationship(free, free_other).unwrap();
        sheet.set_relationship_title(same, "Clipped label").unwrap();
        sheet.validate().unwrap();

        // Isolated, known geometry exercises actual SVG-to-pixel behavior rather
        // than relying on SVG markup or on font-dependent automatic positions.
        let mut layout = Layout::default();
        for (child, x0, x1, y) in [(first, 70.0, 140.0, 30.0), (second, 140.0, 180.0, 40.0)] {
            layout.connectors.push(Connector {
                parent: root,
                child,
                kind: ConnectorKind::Straight,
                vertical: false,
                start: Vec2::new(x0, y),
                end: Vec2::new(x1, y),
                color: Color::rgb(20, 200, 30),
                width: 6.0,
            });
        }
        layout.boundaries.extend([
            BoundaryLayout {
                id: boundary,
                rect: rect(80.0, 50.0, 60.0, 20.0),
            },
            BoundaryLayout {
                id: hidden_boundary,
                rect: rect(140.0, 75.0, 40.0, 15.0),
            },
        ]);
        for (id, topic, x0, x1, y) in [
            (summary_id, summary, 70.0, 140.0, 100.0),
            (hidden_summary_id, hidden_summary, 140.0, 180.0, 110.0),
        ] {
            layout.summaries.push(SummaryLayout {
                id,
                topic,
                dir: Dir::Right,
                range_rect: rect(x0, y - 1.0, x1 - x0, 2.0),
                start: Vec2::new(x0, y),
                end: Vec2::new(x0, y),
                tip: Vec2::new(x1, y),
            });
        }
        for (id, x0, x1, y) in [
            (same, 70.0, 140.0, 130.0),
            (same_rect_link, 70.0, 140.0, 150.0),
            (fully_clipped, 140.0, 180.0, 165.0),
            (cross, 70.0, 260.0, 185.0),
            (mixed, 70.0, 200.0, 215.0),
            (unowned, 120.0, 200.0, 275.0),
        ] {
            let dx = x1 - x0;
            layout.relationships.push(RelationshipLayout {
                id,
                start: Vec2::new(x0, y),
                c1: Vec2::new(x0 + dx / 3.0, y),
                c2: Vec2::new(x0 + dx * 2.0 / 3.0, y),
                end: Vec2::new(x1, y),
                mid: Vec2::new((x0 + x1) / 2.0, y),
            });
        }
        let options = ExportOptions {
            transparent: true,
            ..Default::default()
        };
        let frame = Some(rect(0.0, 0.0, 400.0, 300.0));
        let fontdb = Arc::new(crate::fonts::build(false));
        let render = |source: &Sheet| {
            svg_layout_with_fonts(
                source,
                &options,
                fontdb.clone(),
                frame,
                &layout,
                &source.zones,
                SvgSceneOptions::new(source, true, VisualExportContext::default()),
            )
            .unwrap()
        };
        let clipped_svg = render(&sheet);
        for (kind, id) in [
            ("map-edge", first.0),
            ("map-edge", second.0),
            ("boundary", boundary.0),
            ("boundary", hidden_boundary.0),
            ("summary", summary_id.0),
            ("summary", hidden_summary_id.0),
            ("relationship", same.0),
            ("relationship", fully_clipped.0),
        ] {
            assert!(clipped_svg.contains(&format!("scene-clip-{kind}-{id}")));
        }
        for id in [same_rect_link, cross, mixed, unowned] {
            assert!(!clipped_svg.contains(&format!("scene-clip-relationship-{}", id.0)));
        }
        let clipped = image::load_from_memory(
            &raster_svg(&clipped_svg, 1.0, image::ImageFormat::Png).unwrap(),
        )
        .unwrap()
        .to_rgba8();
        let mut unclipped_sheet = sheet.clone();
        for zone in &mut unclipped_sheet.zones {
            zone.clip = false;
        }
        let unclipped = image::load_from_memory(
            &raster_svg(&render(&unclipped_sheet), 1.0, image::ImageFormat::Png).unwrap(),
        )
        .unwrap()
        .to_rgba8();
        let empty_svg = svg_layout_with_fonts(
            &sheet,
            &options,
            fontdb,
            frame,
            &Layout::default(),
            &sheet.zones,
            SvgSceneOptions::new(&sheet, true, VisualExportContext::default()),
        )
        .unwrap();
        let empty =
            image::load_from_memory(&raster_svg(&empty_svg, 1.0, image::ImageFormat::Png).unwrap())
                .unwrap()
                .to_rgba8();
        let differences = |x0: u32, x1: u32, y0: u32, y1: u32| {
            (y0..y1)
                .flat_map(|y| (x0..x1).map(move |x| (x, y)))
                .filter(|(x, y)| clipped.get_pixel(*x, *y) != unclipped.get_pixel(*x, *y))
                .count()
        };
        let visible_differences = |x0: u32, x1: u32, y0: u32, y1: u32| {
            (y0..y1)
                .flat_map(|y| (x0..x1).map(move |x| (x, y)))
                .filter(|(x, y)| {
                    // Compare visible premultiplied colors; straight RGB can
                    // differ substantially when alpha is close to zero.
                    let premultiply = |pixel: [u8; 4]| {
                        let alpha = u16::from(pixel[3]);
                        [
                            (u16::from(pixel[0]) * alpha + 127) / 255,
                            (u16::from(pixel[1]) * alpha + 127) / 255,
                            (u16::from(pixel[2]) * alpha + 127) / 255,
                            alpha,
                        ]
                    };
                    premultiply(clipped.get_pixel(*x, *y).0)
                        .iter()
                        .zip(premultiply(unclipped.get_pixel(*x, *y).0))
                        .any(|(a, b)| a.abs_diff(b) > 2)
                })
                .count()
        };
        let painted = |x0: u32, x1: u32, y0: u32, y1: u32| {
            (y0..y1)
                .flat_map(|y| (x0..x1).map(move |x| (x, y)))
                .filter(|(x, y)| clipped.get_pixel(*x, *y) != empty.get_pixel(*x, *y))
                .count()
        };
        for (name, y0, y1) in [
            ("map edge", 26, 34),
            ("boundary", 52, 68),
            ("summary", 97, 103),
            ("relationship", 127, 133),
        ] {
            // resvg may round translucent clip-group compositing by 1–2 units.
            assert_eq!(
                visible_differences(75, 95, y0, y1),
                0,
                "{name} inside frame"
            );
            assert!(painted(75, 95, y0, y1) > 0, "{name} disappeared");
            assert!(differences(105, 135, y0, y1) > 0, "{name} outside frame");
        }
        for (name, y0, y1) in [
            ("fully clipped edge", 36, 44),
            ("fully clipped boundary", 76, 89),
            ("fully clipped summary", 107, 113),
            ("fully clipped relationship", 162, 168),
        ] {
            assert!(differences(145, 178, y0, y1) > 0, "{name}");
            assert_eq!(painted(145, 178, y0, y1), 0, "{name} leaked pixels");
        }
        for (name, y0, y1) in [
            ("distinct coincident frames", 147, 153),
            ("cross-frame relationship", 182, 188),
            ("clipped-to-unclipped relationship", 212, 218),
            ("unowned relationship", 272, 278),
        ] {
            assert_eq!(differences(120, 180, y0, y1), 0, "{name} was cropped");
            assert!(painted(120, 180, y0, y1) > 0, "{name} disappeared");
        }
    }

    fn assert_title_fits(
        source: &str,
        fontdb: Arc<resvg::usvg::fontdb::Database>,
        layout: &thinoda_core::layout::Layout,
        text: &text::ExportText,
    ) {
        let tree = resvg::usvg::Tree::from_str(
            source,
            &resvg::usvg::Options {
                fontdb,
                ..Default::default()
            },
        )
        .unwrap();
        for (&id, topic) in &layout.topics {
            let inset = topic.style.border_width.max(topic.style.line_width) / 2.0 + 2.0;
            let inner = topic.rect.expand(-inset);
            for (index, line) in text.topic(id).lines.iter().enumerate() {
                if line.text.is_empty() {
                    continue;
                }
                let node = tree
                    .node_by_id(&format!("topic-{id}-line-{index}"))
                    .unwrap();
                let bbox = node.bounding_box();
                let bounds = Rect::from_min_size(
                    Vec2::new(bbox.x(), bbox.y()),
                    Vec2::new(bbox.width(), bbox.height()),
                );
                for corner in [
                    bounds.min,
                    bounds.max(),
                    Vec2::new(bounds.min.x, bounds.max().y),
                    Vec2::new(bounds.max().x, bounds.min.y),
                ] {
                    assert!(
                        inner.contains(corner),
                        "{id} {:?}: {bounds:?} outside {inner:?}",
                        line.text
                    );
                    let delta = corner - inner.center();
                    let x = delta.x / (inner.size.x / 2.0);
                    let y = delta.y / (inner.size.y / 2.0);
                    match topic.style.shape {
                        TopicShape::Ellipse => assert!(x * x + y * y <= 1.0, "ellipse {id}"),
                        TopicShape::Diamond => assert!(x.abs() + y.abs() <= 1.0, "diamond {id}"),
                        TopicShape::Capsule => {
                            let radius = inner.size.y / 2.0;
                            let dx = (delta.x.abs() - (inner.size.x / 2.0 - radius)).max(0.0);
                            assert!(
                                dx * dx + delta.y * delta.y <= radius * radius,
                                "capsule {id}"
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn regression_map() -> Workbook {
        let mut s = Sheet::new("Export regression", "KI Coden");
        let branch = s.add_child(s.root, None, "6. Von der Idee zum Code");
        for title in [
            "Formen von KI-Coding",
            "Chat in VsCode",
            "GitHub Copilot CLI",
            "KERN Katalog",
            "R\u{fc}ckblick / Entwicklung von LLMs",
            "Refactoring und Produkt ab 9/25 (V1.x)",
            "Integration in die kern-ux.de Dom\u{e4}ne ab 1/26",
            "One line\n\nAn explicit third line",
            "WWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWW",
            "",
        ] {
            s.add_child(branch, None, title);
        }
        for shape in [
            TopicShape::RoundedRect,
            TopicShape::Rect,
            TopicShape::Capsule,
            TopicShape::Ellipse,
            TopicShape::Diamond,
            TopicShape::Underline,
        ] {
            let id = s.add_child(branch, None, "Wide WWW accents \u{c4}\u{e9}gyp\nmore text");
            let style = &mut s.topics.get_mut(&id).unwrap().style;
            style.shape = Some(shape);
            style.width = Some(95.0);
            style.font_size = Some(28.0);
            style.bold = Some(true);
            style.font_family = Some("monospace".into());
            style.border_width = Some(12.0);
        }
        let narrow = s.add_child(branch, None, "W");
        s.topics.get_mut(&narrow).unwrap().style.width = Some(1.0);
        Workbook::new(s)
    }

    #[test]
    fn rendered_titles_keep_whitespace_inside_nodes_with_native_and_web_fonts() {
        let w = regression_map();
        for fontdb in [
            crate::fonts::database(),
            Arc::new(crate::fonts::build(false)),
        ] {
            for dark in [false, true] {
                let opts = ExportOptions {
                    dark,
                    ..Default::default()
                };
                let text = text::ExportText::new(w.active(), dark, fontdb.clone()).unwrap();
                let layout = layout_sheet(w.active(), &text, dark);
                let source =
                    svg_with_fonts(&w, &opts, fontdb.clone(), VisualExportContext::default())
                        .unwrap();
                assert_title_fits(&source, fontdb.clone(), &layout, &text);
                let substituted = source.replace(
                    "font-family=\"Segoe UI, Helvetica, Arial, sans-serif\"",
                    "font-family=\"monospace\"",
                );
                assert_title_fits(&substituted, fontdb.clone(), &layout, &text);
                let id = w
                    .active()
                    .topics
                    .values()
                    .find(|t| t.title == "KERN Katalog")
                    .unwrap()
                    .id;
                assert_eq!(text.topic(id).lines.len(), 1);
            }
        }
    }

    #[test]
    fn drawing_reuses_measured_lines_even_when_the_last_space_was_trimmed() {
        let mut sheet = Sheet::new("S", "Root");
        let parent = sheet.add_child(sheet.root, None, "Branch");
        let id = sheet.add_child(parent, None, "Formen von KI-Coding");
        let w = Workbook::new(sheet);
        let text = text::ExportText::new(w.active(), false, crate::fonts::database()).unwrap();
        assert_eq!(text.topic(id).lines.len(), 1);
        let source = svg(&w, &Default::default(), VisualExportContext::default()).unwrap();
        assert!(source.contains("Formen von KI-Coding</text>"));
        assert!(!source.contains(&format!("topic-{id}-line-1")));
    }

    #[test]
    fn pdf_contains_vectors_not_a_page_sized_bitmap_and_ignores_raster_scale() {
        let w = regression_map();
        let a = pdf(&w, &Default::default(), VisualExportContext::default()).unwrap();
        let b = pdf(
            &w,
            &ExportOptions {
                scale: 8.0,
                ..Default::default()
            },
            VisualExportContext::default(),
        )
        .unwrap();
        assert!(a.starts_with(b"%PDF-"));
        let source = String::from_utf8_lossy(&a);
        assert!(!source.contains("/Subtype /Image"));
        assert!(!source.contains("/DCTDecode"));
        assert!(source.contains("/MediaBox"));
        assert_eq!(a, b);
    }
}
