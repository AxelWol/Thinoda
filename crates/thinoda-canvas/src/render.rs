//! Painting of a laid-out sheet.

use crate::measure::BOLD_WIDTH;
use crate::view::ObjectSelection;
use egui::{
    Align, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, pos2,
    text::{LayoutJob, TextFormat},
    vec2,
};
use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};
use thinoda_core::{
    CanvasObject, CanvasObjectId, CanvasObjectKind, CardKind, GridKind, Layout, SceneBand, SceneId,
    ShapeKind, Sheet, TextAlign, TopicId, TopicShape, ZoneId,
    board::{polyline_midpoint, rotate_around, rotated_bounds, rotated_corners, shape_label_rect},
    geometry::{Color, Vec2},
    layout::{ConnectorKind, Dir, Icon, TopicLayout, cubic},
    scene::{Scene, SceneGeometry, build_scene, connector_stroke_path, topic_clip_zone},
    theme::Theme,
};

pub(crate) const RELATIONSHIP_SEGMENTS: usize = 32;

pub(crate) fn shape_label_font_size(zoom: f32) -> f32 {
    (12.0 * zoom).max(7.0)
}

/// Converts a core color into an egui color.
pub fn color32(c: Color) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

/// Sheet → screen transform.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Xf {
    pub origin: Pos2,
    pub zoom: f32,
}

impl Xf {
    pub fn pos(&self, v: Vec2) -> Pos2 {
        pos2(
            self.origin.x + v.x * self.zoom,
            self.origin.y + v.y * self.zoom,
        )
    }

    pub fn rect(&self, r: thinoda_core::Rect) -> Rect {
        Rect::from_min_max(self.pos(r.min), self.pos(r.max()))
    }

    pub fn sheet(&self, p: Pos2) -> Vec2 {
        Vec2::new(
            (p.x - self.origin.x) / self.zoom,
            (p.y - self.origin.y) / self.zoom,
        )
    }
}

/// Per-frame paint options.
#[derive(Debug, Clone, Default)]
pub struct PaintOptions<'a> {
    pub selection: &'a [TopicId],
    pub hovered: Option<TopicId>,
    pub editing: Option<TopicId>,
    pub accent: Color32,
    pub dark: bool,
    /// Topics dimmed by an active filter (`None` = no filter).
    pub filter: Option<&'a [TopicId]>,
    /// Additional source topics dimmed while a view-only drag preview is active.
    pub dimmed: Option<&'a std::collections::BTreeSet<TopicId>>,
    pub selected_object: Option<ObjectSelection>,
    pub omit_zones: bool,
    /// Show the interactive board grid. Thumbnails omit it by default.
    pub show_grid: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct GhostClip {
    pub delta: Vec2,
    pub moving_zone: Option<ZoneId>,
}

/// Background color of the sheet.
pub fn sheet_background(sheet: &Sheet, dark: bool) -> Color32 {
    color32(Theme::for_settings(&sheet.theme).background(&sheet.theme, dark))
}

fn core_color32(color: Color32) -> Color {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    Color::rgba(r, g, b, a)
}

fn zone_title_color(
    sheet: &Sheet,
    background: Color32,
    tint: Color32,
    fill_opacity: f32,
) -> Color32 {
    if !sheet.uses_board_semantics() {
        return tint;
    }

    let background = core_color32(background);
    let tint = core_color32(tint.gamma_multiply(fill_opacity.clamp(0.0, 1.0)));
    color32(Color::highest_contrast_on(tint.over(background)))
}

/// Fits a noninteractive sample into a thumbnail, without changing any canvas camera.
pub fn paint_thumbnail(painter: &Painter, sheet: &Sheet, layout: &Layout, rect: Rect, dark: bool) {
    painter.rect_filled(rect, 4, sheet_background(sheet, dark));
    let rect = rect.shrink(5.0);
    let scene = build_scene(sheet, layout);
    let scene_bounds = scene.bounds();
    let bounds = match scene_bounds {
        Some(bounds) if layout.order.is_empty() => bounds,
        Some(bounds) => bounds.union(&layout.bounds),
        None => layout.bounds,
    };
    let zoom = (rect.width() / bounds.size.x.max(1.0)).min(rect.height() / bounds.size.y.max(1.0));
    let center = bounds.center();
    let xf = Xf {
        origin: rect.center() - vec2(center.x, center.y) * zoom,
        zoom,
    };
    paint_map_scene(
        &painter.with_clip_rect(rect),
        sheet,
        layout,
        xf,
        &PaintOptions {
            dark,
            ..Default::default()
        },
        &scene,
        None,
    );
}

/// Paints the whole map.
pub fn paint_map(
    painter: &Painter,
    sheet: &Sheet,
    layout: &Layout,
    xf: Xf,
    opts: &PaintOptions<'_>,
) {
    let scene = build_scene(sheet, layout);
    paint_map_scene(painter, sheet, layout, xf, opts, &scene, None);
}

pub(crate) fn paint_map_scene(
    painter: &Painter,
    sheet: &Sheet,
    layout: &Layout,
    xf: Xf,
    opts: &PaintOptions<'_>,
    scene: &Scene,
    ghost_clip: Option<GhostClip>,
) {
    let z = xf.zoom;
    let bg = sheet_background(sheet, opts.dark);
    let dim = |id: TopicId| {
        opts.filter.is_some_and(|f| !f.contains(&id))
            || opts.dimmed.is_some_and(|ids| ids.contains(&id))
    };
    // Ghost geometry already uses a translated transform; stationary clips cancel it.
    let topic_clip = |id| {
        topic_clip_zone(sheet, id).map(|(zone, frame)| match ghost_clip {
            Some(ghost) if ghost.moving_zone != Some(zone) => frame.translate(ghost.delta * -1.0),
            _ => frame,
        })
    };
    let objects: HashMap<_, _> = sheet
        .canvas
        .objects
        .iter()
        .map(|object| (object.id, object))
        .collect();

    if opts.show_grid {
        paint_grid(painter, sheet, xf, opts.dark);
    }

    if !opts.omit_zones {
        for zone in &sheet.zones {
            let rect = xf.rect(zone.frame);
            let selected = opts.selected_object == Some(ObjectSelection::Zone(zone.id));
            let color = if selected {
                opts.accent
            } else if let Some(color) = zone.color {
                color32(color)
            } else if opts.dark {
                Color32::from_gray(170)
            } else {
                Color32::from_gray(85)
            };
            let fill_tint = zone.background_color.or(zone.color);
            let fill_color = fill_tint.map_or(color, color32);
            let fill_opacity = if fill_tint.is_some() { 0.16 } else { 0.04 };
            painter.rect(
                rect,
                4.0,
                fill_color.gamma_multiply(fill_opacity),
                Stroke::new(if selected { 2.0 } else { 1.0 }, color),
                StrokeKind::Inside,
            );
            let title_color = zone_title_color(sheet, bg, fill_color, fill_opacity);
            painter.with_clip_rect(rect.shrink(4.0)).text(
                rect.left_top() + vec2(8.0, 6.0),
                egui::Align2::LEFT_TOP,
                &zone.name,
                FontId::proportional(12.0),
                title_color,
            );
            if selected {
                painter.rect_filled(
                    Rect::from_center_size(rect.right_bottom(), vec2(10.0, 10.0)),
                    1.0,
                    color,
                );
            }
        }
    }

    let ctx = painter.ctx();
    let cache_id = ink_cache_id();
    let mut ink_cache = ctx
        .data_mut(|data| data.remove_temp::<InkGeometryCache>(cache_id))
        .unwrap_or_default();
    // Protect both bands before admitting misses, so a lower-band miss cannot
    // evict a visible mesh that will be painted later in the upper band.
    ink_cache.begin_pass();
    let viewport = sheet_viewport(painter.clip_rect(), xf);
    for item in &scene.items {
        let SceneId::Object(id) = item.id else {
            continue;
        };
        if !matches!(&item.geometry, SceneGeometry::Ink { points, .. } if points.len() > 1) {
            continue;
        }
        ink_cache.protect_if(id, || {
            objects
                .get(&id)
                .is_some_and(|object| object_in_viewport(object, item, xf.zoom, viewport))
        });
    }
    let ink_tessellation = ink_tessellation_config(ctx);
    paint_objects_in_band(
        painter,
        sheet,
        &objects,
        scene,
        SceneBand::BelowMap,
        xf,
        opts,
        &mut ink_cache,
        ink_tessellation,
    );

    // Boundaries.
    for b in &layout.boundaries {
        let bd = sheet.boundaries.iter().find(|x| x.id == b.id);
        let clipped = bd
            .and_then(|boundary| topic_clip(boundary.parent))
            .map(|clip| painter.with_clip_rect(xf.rect(clip)));
        let painter = clipped.as_ref().unwrap_or(painter);
        let selected = opts.selected_object == Some(ObjectSelection::Boundary(b.id));
        let mut color = if selected {
            opts.accent
        } else {
            bd.and_then(|x| x.color)
                .map(color32)
                .unwrap_or(Color32::from_rgb(0x8A, 0x9B, 0xB0))
        };
        if bd.is_some_and(|b| dim(b.parent)) {
            color = color.gamma_multiply(0.3);
        }
        let r = xf.rect(b.rect);
        painter.rect_filled(
            r,
            CornerRadius::same((10.0 * z).min(255.0) as u8),
            color.gamma_multiply(0.12),
        );
        dashed_rect(
            painter,
            r,
            Stroke::new(if selected { 2.5 } else { 1.5 } * z.max(0.5), color),
            6.0 * z,
            4.0 * z,
        );
        if let Some(t) = bd.and_then(|x| x.title.as_ref()) {
            let font = FontId::proportional(12.0 * z);
            let g = painter.layout_no_wrap(t.clone(), font, Color32::WHITE);
            let tr = Rect::from_min_size(
                r.left_top() + vec2(8.0 * z, -g.size().y - 2.0 * z),
                g.size() + vec2(10.0 * z, 4.0 * z),
            );
            painter.rect_filled(tr, CornerRadius::same((4.0 * z) as u8), color);
            painter.galley(tr.min + vec2(5.0 * z, 2.0 * z), g, Color32::WHITE);
        }
    }

    // Branch connectors.
    for c in &layout.connectors {
        let clipped = topic_clip(c.child).map(|clip| painter.with_clip_rect(xf.rect(clip)));
        let painter = clipped.as_ref().unwrap_or(painter);
        let mut color = color32(c.color);
        if dim(c.child) {
            color = color.gamma_multiply(0.25);
        }
        let pts = connector_path(c.kind, c.vertical, c.start, c.end);
        let pts: Vec<Pos2> = pts.into_iter().map(|p| xf.pos(p)).collect();
        let tapered = sheet.theme.tapered_lines && c.parent == sheet.root;
        if tapered {
            tapered_line(
                painter,
                &pts,
                (c.width * 3.0 * z).max(1.0),
                (c.width * z).max(0.5),
                color,
            );
        } else {
            painter.add(Shape::line(pts, Stroke::new((c.width * z).max(0.5), color)));
        }
    }

    // Summaries (brace + connector to summary topic).
    for s in &layout.summaries {
        let clipped = topic_clip(s.topic).map(|clip| painter.with_clip_rect(xf.rect(clip)));
        let painter = clipped.as_ref().unwrap_or(painter);
        let selected = opts.selection.contains(&s.topic)
            || opts.selected_object == Some(ObjectSelection::Summary(s.id));
        let mut color = layout
            .topics
            .get(&s.topic)
            .map(|t| color32(t.style.line_color))
            .unwrap_or(Color32::GRAY);
        if selected {
            color = opts.accent;
        } else if dim(s.topic) {
            color = color.gamma_multiply(0.25);
        }
        let stroke = Stroke::new(1.5 * z.max(0.5), color);
        let pts = brace_path(s.start, s.end, s.tip, s.dir);
        painter.add(Shape::line(
            pts.into_iter().map(|p| xf.pos(p)).collect(),
            stroke,
        ));
        if let Some(topic) = layout.rect(s.topic) {
            let end = match s.dir {
                Dir::Right => topic.left_center(),
                Dir::Left => topic.right_center(),
                Dir::Down => topic.top_center(),
                Dir::Up => topic.bottom_center(),
            };
            painter.line_segment([xf.pos(s.tip), xf.pos(end)], stroke);
        }
        if selected {
            // Selection feedback only: resizing a sibling range has no defined
            // interaction contract yet, so do not imply draggable handles.
            dashed_rect(
                painter,
                xf.rect(s.range_rect.expand(4.0)),
                Stroke::new(1.0, opts.accent),
                5.0 * z,
                3.0 * z,
            );
        }
    }

    // Topics.
    for id in &layout.order {
        let Some(t) = layout.topics.get(id) else {
            continue;
        };
        let topic = sheet.topic(*id);
        let clipped = topic_clip(*id).map(|r| painter.with_clip_rect(xf.rect(r)));
        paint_topic(
            clipped.as_ref().unwrap_or(painter),
            sheet,
            t,
            topic,
            xf,
            bg,
            opts.dark,
            dim(*id),
            opts.editing == Some(*id),
        );
    }

    // Relationships (on top of topics, like XMind).
    for r in &layout.relationships {
        let rel = sheet.relationships.iter().find(|x| x.id == r.id);
        let clipped = rel
            .and_then(|relationship| {
                relationship_clip_rect(sheet, relationship.from, relationship.to)?;
                topic_clip(relationship.from)
            })
            .map(|clip| painter.with_clip_rect(xf.rect(clip)));
        let painter = clipped.as_ref().unwrap_or(painter);
        let mut color = rel
            .and_then(|x| x.color)
            .map(color32)
            .unwrap_or(if opts.dark {
                Color32::from_rgb(0xB0, 0xB8, 0xC4)
            } else {
                Color32::from_rgb(0x5A, 0x6A, 0x80)
            });
        if rel.is_some_and(|r| dim(r.from) && dim(r.to)) {
            color = color.gamma_multiply(0.3);
        }
        let pts: Vec<Pos2> = (0..=RELATIONSHIP_SEGMENTS)
            .map(|i| {
                xf.pos(cubic(
                    r.start,
                    r.c1,
                    r.c2,
                    r.end,
                    i as f32 / RELATIONSHIP_SEGMENTS as f32,
                ))
            })
            .collect();
        let selected = opts.selected_object == Some(ObjectSelection::Relationship(r.id));
        if selected {
            painter.add(Shape::line(
                pts.clone(),
                Stroke::new(5.0, opts.accent.gamma_multiply(0.25)),
            ));
            for (anchor, control) in [(r.start, r.c1), (r.end, r.c2)] {
                painter.line_segment(
                    [xf.pos(anchor), xf.pos(control)],
                    Stroke::new(1.0, opts.accent.gamma_multiply(0.6)),
                );
            }
        }
        let stroke = Stroke::new(1.5 * z.max(0.5), color);
        for s in Shape::dashed_line(&pts, stroke, 6.0 * z, 4.0 * z) {
            painter.add(s);
        }
        arrow_head(
            painter,
            xf.pos(cubic(r.start, r.c1, r.c2, r.end, 0.97)),
            xf.pos(r.end),
            9.0 * z,
            stroke.color,
        );
        if selected {
            for point in [r.c1, r.c2] {
                painter.rect(
                    Rect::from_center_size(xf.pos(point), vec2(9.0, 9.0)),
                    1.0,
                    bg,
                    Stroke::new(1.5, opts.accent),
                    StrokeKind::Inside,
                );
            }
            for point in [r.start, r.end] {
                painter.circle(xf.pos(point), 5.0, bg, Stroke::new(1.5, opts.accent));
            }
        }
        if let Some(title) = rel.map(|x| x.title.as_str()).filter(|t| !t.is_empty()) {
            let font = FontId::proportional(12.0 * z);
            let g = painter.layout_no_wrap(title.to_owned(), font, color);
            let m = xf.pos(r.mid);
            let rr = Rect::from_center_size(m, g.size() + vec2(8.0, 4.0) * z);
            painter.rect_filled(rr, CornerRadius::same((4.0 * z) as u8), bg);
            painter.galley(rr.min + vec2(4.0, 2.0) * z, g, color);
        }
    }

    paint_objects_in_band(
        painter,
        sheet,
        &objects,
        scene,
        SceneBand::AboveMap,
        xf,
        opts,
        &mut ink_cache,
        ink_tessellation,
    );
    ctx.data_mut(|data| data.insert_temp(cache_id, ink_cache));

    // Selection and badges.
    for id in opts.selection {
        if let Some(t) = layout.topics.get(id) {
            let item = scene.item(SceneId::Topic(*id));
            let r = xf
                .rect(item.map_or(t.rect, |item| item.bounds))
                .expand(3.0 * z.max(0.6));
            let clipped = item
                .and_then(|item| item.clip)
                .map(|r| painter.with_clip_rect(xf.rect(r)));
            clipped.as_ref().unwrap_or(painter).rect_stroke(
                r,
                CornerRadius::same((8.0 * z).min(255.0) as u8),
                Stroke::new(2.0, opts.accent),
                StrokeKind::Outside,
            );
        }
    }
    if let Some(h) = opts.hovered.filter(|h| !opts.selection.contains(h))
        && let Some(t) = layout.topics.get(&h)
    {
        let item = scene.item(SceneId::Topic(h));
        let r = xf
            .rect(item.map_or(t.rect, |item| item.bounds))
            .expand(3.0 * z.max(0.6));
        let clipped = item
            .and_then(|item| item.clip)
            .map(|r| painter.with_clip_rect(xf.rect(r)));
        clipped.as_ref().unwrap_or(painter).rect_stroke(
            r,
            CornerRadius::same((8.0 * z).min(255.0) as u8),
            Stroke::new(1.5, opts.accent.gamma_multiply(0.5)),
            StrokeKind::Outside,
        );
    }
    for id in &layout.order {
        let Some(t) = layout.topics.get(id) else {
            continue;
        };
        let has_children = sheet.topic(*id).is_some_and(|x| !x.children.is_empty());
        if !has_children {
            continue;
        }
        let show =
            t.hidden_children > 0 || opts.hovered == Some(*id) || opts.selection.contains(id);
        if show {
            let clipped = topic_clip(*id).map(|clip| painter.with_clip_rect(xf.rect(clip)));
            paint_badge(
                clipped.as_ref().unwrap_or(painter),
                sheet,
                t,
                xf,
                opts.accent,
                bg,
            );
        }
    }
}

pub(crate) fn relationship_clip_rect(
    sheet: &Sheet,
    from: TopicId,
    to: TopicId,
) -> Option<thinoda_core::Rect> {
    let (from_zone, from_rect) = topic_clip_zone(sheet, from)?;
    let (to_zone, _) = topic_clip_zone(sheet, to)?;
    (from_zone == to_zone).then_some(from_rect)
}

fn paint_grid(painter: &Painter, sheet: &Sheet, xf: Xf, dark: bool) {
    let (kind, grid_size) = (sheet.board.grid, sheet.board.grid_size);
    if kind == GridKind::None || !grid_size.is_finite() || grid_size <= 0.0 || xf.zoom <= 0.0 {
        return;
    }

    let clip = painter.clip_rect();
    let a = xf.sheet(clip.left_top());
    let b = xf.sheet(clip.right_bottom());
    let min = Vec2::new(a.x.min(b.x), a.y.min(b.y));
    let max = Vec2::new(a.x.max(b.x), a.y.max(b.y));
    let mut step = grid_size.max(4.0);
    step *= (8.0 / (step * xf.zoom)).ceil().max(1.0);
    let count = |lo: f32, hi: f32, step: f32| {
        ((hi / step).floor() - (lo / step).ceil() + 1.0).max(0.0) as usize
    };
    let largest_axis = count(min.x, max.x, step).max(count(min.y, max.y, step));
    if largest_axis > 1_000 {
        step *= (largest_axis as f32 / 1_000.0).ceil();
    }
    let nx = count(min.x, max.x, step);
    let ny = count(min.y, max.y, step);
    if kind == GridKind::Dots && nx.saturating_mul(ny) > 40_000 {
        step *= ((nx.saturating_mul(ny) as f32 / 40_000.0).sqrt()).ceil();
    }

    let color = if dark {
        Color32::from_rgba_unmultiplied(255, 255, 255, 32)
    } else {
        Color32::from_rgba_unmultiplied(45, 54, 68, 34)
    };
    let x0 = (min.x / step).ceil() as i32;
    let x1 = (max.x / step).floor() as i32;
    let y0 = (min.y / step).ceil() as i32;
    let y1 = (max.y / step).floor() as i32;
    match kind {
        GridKind::None => {}
        GridKind::Lines => {
            for x in x0..=x1 {
                let p = xf.pos(Vec2::new(x as f32 * step, min.y));
                let q = xf.pos(Vec2::new(x as f32 * step, max.y));
                painter.line_segment([p, q], Stroke::new(1.0, color));
            }
            for y in y0..=y1 {
                let p = xf.pos(Vec2::new(min.x, y as f32 * step));
                let q = xf.pos(Vec2::new(max.x, y as f32 * step));
                painter.line_segment([p, q], Stroke::new(1.0, color));
            }
        }
        GridKind::Dots => {
            for x in x0..=x1 {
                for y in y0..=y1 {
                    painter.circle_filled(
                        xf.pos(Vec2::new(x as f32 * step, y as f32 * step)),
                        1.0,
                        color,
                    );
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_objects_in_band(
    painter: &Painter,
    sheet: &Sheet,
    objects: &HashMap<CanvasObjectId, &CanvasObject>,
    scene: &Scene,
    band: SceneBand,
    xf: Xf,
    opts: &PaintOptions<'_>,
    ink_cache: &mut InkGeometryCache,
    ink_tessellation: InkTessellationConfig,
) {
    let viewport = sheet_viewport(painter.clip_rect(), xf);
    for item in scene.items.iter().filter(|item| item.band == band) {
        let SceneId::Object(id) = item.id else {
            continue;
        };
        let Some(object) = objects.get(&id) else {
            continue;
        };
        if !object_in_viewport(object, item, xf.zoom, viewport) {
            continue;
        }
        let clipped = item.clip.map(|r| painter.with_clip_rect(xf.rect(r)));
        paint_canvas_object(
            clipped.as_ref().unwrap_or(painter),
            sheet,
            object,
            item,
            xf,
            opts,
            ink_cache,
            ink_tessellation,
        );
    }
}

fn object_in_viewport(
    object: &CanvasObject,
    item: &thinoda_core::SceneItem,
    zoom: f32,
    viewport: Option<thinoda_core::Rect>,
) -> bool {
    viewport.is_none_or(|viewport| {
        object_visible_bounds(object, item, zoom)
            .is_some_and(|bounds| rects_intersect_inclusive(bounds, viewport))
            || shape_label_visible_bounds(object, item, zoom)
                .is_some_and(|bounds| rects_intersect_inclusive(bounds, viewport))
    })
}

fn sheet_viewport(screen: Rect, xf: Xf) -> Option<thinoda_core::Rect> {
    if !xf.zoom.is_finite() || xf.zoom <= 0.0 {
        return None;
    }
    let min = xf.sheet(screen.min);
    let max = xf.sheet(screen.max);
    Some(thinoda_core::Rect::from_min_size(min, max - min))
}

fn object_visible_bounds(
    object: &CanvasObject,
    item: &thinoda_core::SceneItem,
    zoom: f32,
) -> Option<thinoda_core::Rect> {
    let geometry_margin = match &object.kind {
        CanvasObjectKind::Shape { .. } => object
            .style
            .stroke
            .map(|_| object.style.stroke_width.max(0.0) / 2.0)
            .unwrap_or_else(|| {
                if object.style.fill.is_none() {
                    1.0
                } else {
                    0.0
                }
            }),
        CanvasObjectKind::Ink(_) => 0.0,
        CanvasObjectKind::Connector(connector) => {
            let zoom_scale = (1.0 / zoom.max(f32::EPSILON)).max(1.0);
            let arrow_margin = if connector.arrow_start || connector.arrow_end {
                12.0 * zoom_scale
            } else {
                0.0
            };
            let label_margin = if connector.label.is_empty() {
                0.0
            } else {
                connector.label.chars().count() as f32 * 12.0 * zoom_scale + 8.0 * zoom_scale
            };
            arrow_margin.max(label_margin)
        }
    };
    let mut bounds = item
        .bounds
        .expand(geometry_margin + 1.0 / zoom.max(f32::EPSILON));
    if let Some(clip) = item.clip {
        bounds = intersect_rect(bounds, clip)?;
    }
    Some(bounds)
}

fn shape_label_visible_bounds(
    object: &CanvasObject,
    item: &thinoda_core::SceneItem,
    zoom: f32,
) -> Option<thinoda_core::Rect> {
    let CanvasObjectKind::Shape { shape, label } = &object.kind else {
        return None;
    };
    if label.is_empty() {
        return None;
    }
    // Labels can extend beyond their outline; keep a generous text bound for culling.
    let (longest, line_count) = label.split('\n').fold((0, 0), |(longest, lines), line| {
        (longest.max(line.len()), lines + 1)
    });
    let font_size = shape_label_font_size(zoom) / zoom.max(f32::EPSILON);
    let size = Vec2::new(longest as f32, line_count as f32) * (font_size * 2.0);
    let center = rotate_around(
        shape_label_rect(*shape, object.rect).center(),
        object.rect.center(),
        object.rotation,
    );
    let mut bounds = rotated_bounds(
        thinoda_core::Rect::from_center_size(center, size),
        object.rotation,
    );
    if let Some(clip) = item.clip {
        bounds = intersect_rect(bounds, clip)?;
    }
    Some(bounds)
}

fn intersect_rect(a: thinoda_core::Rect, b: thinoda_core::Rect) -> Option<thinoda_core::Rect> {
    let a_max = a.max();
    let b_max = b.max();
    let min = Vec2::new(a.min.x.max(b.min.x), a.min.y.max(b.min.y));
    let max = Vec2::new(a_max.x.min(b_max.x), a_max.y.min(b_max.y));
    (max.x >= min.x && max.y >= min.y).then(|| thinoda_core::Rect::from_min_size(min, max - min))
}

fn rects_intersect_inclusive(a: thinoda_core::Rect, b: thinoda_core::Rect) -> bool {
    let a_max = a.max();
    let b_max = b.max();
    a.min.x <= b_max.x && a_max.x >= b.min.x && a.min.y <= b_max.y && a_max.y >= b.min.y
}

const MAX_INK_CACHE_ENTRIES: usize = 8_192;
const MAX_INK_CACHE_BYTES: usize = 64 * 1024 * 1024;
const MAX_INK_CACHE_CONTAINER_RESERVE_BYTES: usize = 4 * 1024 * 1024;
const INK_CACHE_ENTRY_OVERHEAD_BYTES: usize = 256;
const MAX_CACHED_STROKE_POINTS: usize = 8_192;

fn ink_cache_id() -> egui::Id {
    static ID: OnceLock<egui::Id> = OnceLock::new();
    *ID.get_or_init(|| egui::Id::unique("canvas-ink-geometry-cache"))
}

#[derive(Clone, Copy)]
struct InkTessellationConfig {
    pixels_per_point: f32,
    options: egui::epaint::TessellationOptions,
}

fn ink_tessellation_config(ctx: &egui::Context) -> InkTessellationConfig {
    InkTessellationConfig {
        pixels_per_point: ctx.pixels_per_point(),
        options: ctx.tessellation_options(|options| *options),
    }
}

#[derive(Clone)]
struct InkGeometryEntry {
    object: CanvasObjectId,
    token: u64,
    protected: bool,
    source_points: Arc<[Vec2]>,
    mesh: Arc<egui::Mesh>,
    center: Vec2,
    rotation_bits: u32,
    zoom_bits: u32,
    origin_bits: [u32; 2],
    clip_rect: Rect,
    stroke_width_bits: u32,
    color: Color32,
    pixels_per_point_bits: u32,
    tessellation_options: egui::epaint::TessellationOptions,
}

// The cache stores final screen-space stroke meshes so paint and final tessellation only
// submit existing triangles. Screen origin is part of the key because line snapping can
// depend on the pan offset.
#[derive(Clone, Default)]
struct InkGeometryCache {
    entries: HashMap<CanvasObjectId, InkGeometryEntry>,
    insertion_order: std::collections::VecDeque<(CanvasObjectId, u64)>,
    next_token: u64,
    allocated_bytes: usize,
    evictable: usize,
    protected_bytes: usize,
}

impl InkGeometryEntry {
    fn allocation_cost(&self) -> usize {
        INK_CACHE_ENTRY_OVERHEAD_BYTES
            .saturating_add(
                self.source_points
                    .len()
                    .saturating_mul(core::mem::size_of::<Vec2>()),
            )
            .saturating_add(
                self.mesh
                    .vertices
                    .capacity()
                    .saturating_mul(core::mem::size_of::<egui::epaint::Vertex>()),
            )
            .saturating_add(
                self.mesh
                    .indices
                    .capacity()
                    .saturating_mul(core::mem::size_of::<u32>()),
            )
    }

    #[allow(clippy::too_many_arguments)]
    fn matches(
        &self,
        object: CanvasObjectId,
        source_points: &Arc<[Vec2]>,
        center: Vec2,
        rotation: f32,
        xf: Xf,
        clip_rect: Rect,
        stroke_width: f32,
        color: Color32,
        tessellation: InkTessellationConfig,
    ) -> bool {
        self.object == object
            && (Arc::ptr_eq(&self.source_points, source_points)
                || self.source_points.as_ref() == source_points.as_ref())
            && self.center == center
            && self.rotation_bits == rotation.to_bits()
            && self.zoom_bits == xf.zoom.to_bits()
            && self.origin_bits == [xf.origin.x.to_bits(), xf.origin.y.to_bits()]
            && self.clip_rect == clip_rect
            && self.stroke_width_bits == stroke_width.to_bits()
            && self.color == color
            && self.pixels_per_point_bits == tessellation.pixels_per_point.to_bits()
            && self.tessellation_options == tessellation.options
    }
}

impl InkGeometryCache {
    fn begin_pass(&mut self) {
        self.evictable = self.entries.len();
        self.protected_bytes = 0;
        for entry in self.entries.values_mut() {
            entry.protected = false;
        }
    }

    fn protect_if(&mut self, object: CanvasObjectId, visible: impl FnOnce() -> bool) {
        if let Some(entry) = self.entries.get_mut(&object)
            && !entry.protected
            && visible()
        {
            entry.protected = true;
            self.evictable -= 1;
            self.protected_bytes += entry.allocation_cost();
        }
    }

    fn remove(&mut self, object: CanvasObjectId) {
        if let Some(entry) = self.entries.remove(&object) {
            let allocation_cost = entry.allocation_cost();
            self.allocated_bytes = self.allocated_bytes.saturating_sub(allocation_cost);
            if entry.protected {
                self.protected_bytes -= allocation_cost;
            } else {
                self.evictable -= 1;
            }
        }
        if self.insertion_order.len() >= MAX_INK_CACHE_ENTRIES * 2 {
            let entries = &self.entries;
            self.insertion_order.retain(|(queued_object, token)| {
                entries
                    .get(queued_object)
                    .is_some_and(|entry| entry.token == *token)
            });
        }
    }

    fn insert(&mut self, entry: InkGeometryEntry) -> bool {
        self.insert_with_limits(
            entry,
            MAX_INK_CACHE_ENTRIES,
            MAX_INK_CACHE_BYTES.saturating_sub(MAX_INK_CACHE_CONTAINER_RESERVE_BYTES),
        )
    }

    fn insert_with_limits(
        &mut self,
        entry: InkGeometryEntry,
        max_entries: usize,
        max_bytes: usize,
    ) -> bool {
        let object = entry.object;
        self.remove(object);
        if entry.source_points.is_empty()
            || entry.source_points.len() > MAX_CACHED_STROKE_POINTS
            || max_entries == 0
        {
            return false;
        }
        let allocation_cost = entry.allocation_cost();
        if self.protected_bytes.saturating_add(allocation_cost) > max_bytes
            || self.entries.len() - self.evictable >= max_entries
        {
            return false;
        }
        while self.entries.len() >= max_entries
            || self.allocated_bytes.saturating_add(allocation_cost) > max_bytes
        {
            if self.evictable == 0 {
                return false;
            }
            let Some((evicted_object, token)) = self.insertion_order.pop_front() else {
                return false;
            };
            if let Some(entry) = self.entries.get(&evicted_object)
                && entry.token == token
            {
                if entry.protected {
                    self.insertion_order.push_back((evicted_object, token));
                } else {
                    self.remove(evicted_object);
                }
            }
        }
        let token = self.next_token;
        self.next_token = self.next_token.wrapping_add(1);
        self.allocated_bytes += allocation_cost;
        self.protected_bytes += allocation_cost;
        self.entries.insert(
            object,
            InkGeometryEntry {
                token,
                protected: true,
                ..entry
            },
        );
        self.insertion_order.push_back((object, token));
        true
    }
}

#[allow(clippy::too_many_arguments)]
fn ink_tessellated_mesh(
    painter: &Painter,
    cache: &mut InkGeometryCache,
    object: CanvasObjectId,
    source_points: &Arc<[Vec2]>,
    center: Vec2,
    rotation: f32,
    xf: Xf,
    stroke_width: f32,
    color: Color32,
    tessellation: InkTessellationConfig,
) -> Option<Arc<egui::Mesh>> {
    let clip_rect = painter.clip_rect();

    if let Some(entry) = cache.entries.get(&object)
        && entry.matches(
            object,
            source_points,
            center,
            rotation,
            xf,
            clip_rect,
            stroke_width,
            color,
            tessellation,
        )
    {
        return Some(Arc::clone(&entry.mesh));
    }

    let points: Vec<Pos2> = source_points
        .iter()
        .map(|point| xf.pos(rotate_around(*point, center, rotation)))
        .collect();
    let mut tessellator = egui::epaint::Tessellator::new(
        tessellation.pixels_per_point,
        tessellation.options,
        [0, 0],
        Vec::new(),
    );
    tessellator.set_clip_rect(clip_rect);
    let mut mesh = egui::Mesh::default();
    tessellator.tessellate_shape(
        Shape::line(points, Stroke::new(stroke_width, color)),
        &mut mesh,
    );
    if mesh.is_empty() {
        cache.remove(object);
        return None;
    }

    let mesh = Arc::new(mesh);
    let entry = InkGeometryEntry {
        object,
        token: 0,
        protected: true,
        source_points: Arc::clone(source_points),
        mesh: Arc::clone(&mesh),
        center,
        rotation_bits: rotation.to_bits(),
        zoom_bits: xf.zoom.to_bits(),
        origin_bits: [xf.origin.x.to_bits(), xf.origin.y.to_bits()],
        clip_rect,
        stroke_width_bits: stroke_width.to_bits(),
        color,
        pixels_per_point_bits: tessellation.pixels_per_point.to_bits(),
        tessellation_options: tessellation.options,
    };
    cache.insert(entry);
    Some(mesh)
}

/// Resolve a board label's text colour and the opaque backdrop it is judged against.
pub(crate) fn board_label_colors(
    sheet: &Sheet,
    style: &thinoda_core::board::ObjectStyle,
    dark: bool,
) -> (Color32, Color32) {
    let page = core_color32(sheet_background(sheet, dark));
    let background = style.fill.map_or(page, |fill| fill.over(page));
    let text = style
        .text_color
        .map(color32)
        .unwrap_or_else(|| color32(Color::highest_contrast_on(background)));
    (text, color32(background))
}

fn opacity_color(color: Color32, opacity: f32) -> Color32 {
    color.gamma_multiply(opacity.clamp(0.0, 1.0))
}

/// Resolve the automatic stroke used by unstyled board shapes and connectors.
pub fn automatic_board_stroke(accent: Color32, dark: bool) -> Color32 {
    if accent.a() > 0 {
        accent
    } else if dark {
        Color32::from_gray(225)
    } else {
        Color32::from_gray(65)
    }
}

fn default_object_stroke(opts: &PaintOptions<'_>) -> Color32 {
    automatic_board_stroke(opts.accent, opts.dark)
}

struct ShapeFill {
    convex: bool,
    triangles: Vec<u32>,
}

fn shape_fill(kind: ShapeKind) -> &'static ShapeFill {
    static FILLS: OnceLock<HashMap<ShapeKind, ShapeFill>> = OnceLock::new();
    FILLS
        .get_or_init(|| {
            ShapeKind::ALL
                .into_iter()
                .map(|kind| {
                    let outline = kind.unit_outline();
                    let mut winding = 0.0_f32;
                    let mut convex = true;
                    for i in 0..outline.len() {
                        let a = outline[i];
                        let b = outline[(i + 1) % outline.len()];
                        let c = outline[(i + 2) % outline.len()];
                        let cross = (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x);
                        if cross.abs() > 1e-6 {
                            if winding == 0.0 {
                                winding = cross.signum();
                            } else if winding * cross < 0.0 {
                                convex = false;
                                break;
                            }
                        }
                    }
                    let coordinates: Vec<f64> = outline
                        .iter()
                        .flat_map(|point| [f64::from(point.x), f64::from(point.y)])
                        .collect();
                    let triangles = earcutr::earcut(&coordinates, &[], 2)
                        .expect("predefined shape outline must be triangulable")
                        .into_iter()
                        .map(|index| u32::try_from(index).expect("shape outline fits in u32"))
                        .collect();
                    (kind, ShapeFill { convex, triangles })
                })
                .collect()
        })
        .get(&kind)
        .expect("every shape has a fill")
}

#[allow(clippy::too_many_arguments)]
fn paint_canvas_object(
    painter: &Painter,
    sheet: &Sheet,
    object: &CanvasObject,
    item: &thinoda_core::SceneItem,
    xf: Xf,
    opts: &PaintOptions<'_>,
    ink_cache: &mut InkGeometryCache,
    ink_tessellation: InkTessellationConfig,
) {
    let z = xf.zoom;
    match (&object.kind, &item.geometry) {
        (CanvasObjectKind::Shape { shape, label }, SceneGeometry::Polygon { outline, details }) => {
            let fill = object
                .style
                .fill
                .map(color32)
                .map(|color| opacity_color(color, object.style.opacity))
                .unwrap_or(Color32::TRANSPARENT);
            let stroke_color = object
                .style
                .stroke
                .map(color32)
                .unwrap_or_else(|| default_object_stroke(opts));
            let stroke = if let Some(color) = object.style.stroke {
                Stroke::new(
                    (object.style.stroke_width * z).max(0.5),
                    opacity_color(color32(color), object.style.opacity),
                )
            } else if object.style.fill.is_none() {
                Stroke::new(
                    (2.0 * z).max(0.5),
                    opacity_color(stroke_color, object.style.opacity),
                )
            } else {
                Stroke::NONE
            };
            let points: Vec<_> = outline.iter().copied().map(|p| xf.pos(p)).collect();
            let triangulation = shape_fill(*shape);
            if triangulation.convex {
                painter.add(Shape::convex_polygon(points, fill, stroke));
            } else {
                if fill != Color32::TRANSPARENT {
                    let mut mesh = egui::Mesh::default();
                    for point in &points {
                        mesh.colored_vertex(*point, fill);
                    }
                    for triangle in triangulation.triangles.as_chunks::<3>().0 {
                        mesh.add_triangle(triangle[0], triangle[1], triangle[2]);
                    }
                    painter.add(Shape::mesh(mesh));
                }
                if stroke != Stroke::NONE {
                    painter.add(Shape::closed_line(points, stroke));
                }
            }
            for detail in details {
                painter.add(Shape::line(
                    detail.iter().copied().map(|p| xf.pos(p)).collect(),
                    stroke,
                ));
            }
            if !label.is_empty() {
                let text_rect = shape_label_rect(*shape, object.rect);
                let font = FontId::proportional(shape_label_font_size(z));
                let (text_color, _) = board_label_colors(sheet, &object.style, opts.dark);
                let text_color = opacity_color(text_color, object.style.opacity);
                let galley = painter.layout_no_wrap(label.clone(), font, text_color);
                let center = xf.pos(rotate_around(
                    text_rect.center(),
                    object.rect.center(),
                    object.rotation,
                ));
                let angle = object.rotation.to_radians();
                let origin = center - egui::emath::Rot2::from_angle(angle) * (galley.size() / 2.0);
                painter.add(
                    egui::epaint::TextShape::new(origin, galley, text_color).with_angle(angle),
                );
            }
        }
        (
            CanvasObjectKind::Ink(ink),
            SceneGeometry::Ink {
                points,
                center,
                rotation,
                ..
            },
        ) => {
            let opacity = object.style.opacity
                * if ink.tool == thinoda_core::InkTool::Highlighter {
                    0.28
                } else {
                    1.0
                };
            let color = opacity_color(color32(ink.color), opacity);
            let stroke_width = (ink.width * z).max(0.5);
            if points.len() == 1 {
                let point = xf.pos(rotate_around(points[0], *center, *rotation));
                painter.circle_filled(point, (ink.width * z / 2.0).max(0.5), color);
            } else if points.len() > 1 {
                if let Some(mesh) = ink_tessellated_mesh(
                    painter,
                    ink_cache,
                    object.id,
                    points,
                    *center,
                    *rotation,
                    xf,
                    stroke_width,
                    color,
                    ink_tessellation,
                ) {
                    painter.add(Shape::from(mesh));
                } else {
                    let points = points
                        .iter()
                        .map(|point| xf.pos(rotate_around(*point, *center, *rotation)))
                        .collect();
                    painter.add(Shape::line(points, Stroke::new(stroke_width, color)));
                }
            }
        }
        (
            CanvasObjectKind::Connector(connector),
            SceneGeometry::Polyline {
                points, markers, ..
            },
        ) => {
            let color = connector
                .color
                .map(color32)
                .or(object.style.stroke.map(color32))
                .unwrap_or_else(|| default_object_stroke(opts));
            let color = opacity_color(color, object.style.opacity);
            let label_center = polyline_midpoint(points);
            let stroke = Stroke::new((connector.width * z).max(0.5), color);
            let line = connector_stroke_path(points, markers)
                .into_iter()
                .map(|point| xf.pos(point))
                .collect();
            painter.add(Shape::line(line, stroke));
            if points.len() >= 2 {
                if connector.arrow_start {
                    if let Some(marker) = markers[0] {
                        let points = marker.points.map(|point| xf.pos(point)).to_vec();
                        painter.add(if marker.filled {
                            Shape::convex_polygon(points, color, stroke)
                        } else {
                            Shape::line(points, stroke)
                        });
                    } else if connector.start_marker.is_none() {
                        arrow_head(
                            painter,
                            xf.pos(points[1]),
                            xf.pos(points[0]),
                            (9.0 * z).max(4.0),
                            color,
                        );
                    }
                }
                if connector.arrow_end {
                    let n = points.len();
                    if let Some(marker) = markers[1] {
                        let points = marker.points.map(|point| xf.pos(point)).to_vec();
                        painter.add(if marker.filled {
                            Shape::convex_polygon(points, color, stroke)
                        } else {
                            Shape::line(points, stroke)
                        });
                    } else if connector.end_marker.is_none() {
                        arrow_head(
                            painter,
                            xf.pos(points[n - 2]),
                            xf.pos(points[n - 1]),
                            (9.0 * z).max(4.0),
                            color,
                        );
                    }
                }
                if !connector.label.is_empty() {
                    let mid = xf.pos(label_center.expect("a connector path has points"));
                    let font = FontId::proportional((12.0 * z).max(7.0));
                    let galley = painter.layout_no_wrap(connector.label.clone(), font, color);
                    let rect = Rect::from_center_size(mid, galley.size() + vec2(8.0, 4.0) * z);
                    painter.rect_filled(
                        rect,
                        CornerRadius::same((4.0 * z) as u8),
                        sheet_background(sheet, opts.dark),
                    );
                    painter.galley(rect.min + vec2(4.0, 2.0) * z, galley, color);
                }
            }
        }
        _ => {}
    }
}

pub(crate) fn topic_badge_center(sheet: &Sheet, t: &TopicLayout) -> Vec2 {
    let rotation = sheet
        .topic(t.id)
        .and_then(|topic| topic.card)
        .map_or(0.0, |card| card.rotation);
    if rotation == 0.0 {
        t.badge_center()
    } else {
        rotate_around(t.badge_center(), t.rect.center(), rotation)
    }
}

fn paint_badge(
    painter: &Painter,
    sheet: &Sheet,
    t: &TopicLayout,
    xf: Xf,
    accent: Color32,
    bg: Color32,
) {
    let z = xf.zoom.max(0.5);
    let c = xf.pos(topic_badge_center(sheet, t));
    let line = color32(t.style.line_color);
    if t.hidden_children > 0 {
        let text = if t.hidden_children > 99 {
            "99+".to_owned()
        } else {
            t.hidden_children.to_string()
        };
        let g = painter.layout_no_wrap(text, FontId::proportional(10.0 * z), Color32::WHITE);
        let w = (g.size().x + 8.0 * z).max(16.0 * z);
        let r = Rect::from_center_size(c, vec2(w, 16.0 * z));
        painter.rect_filled(r, CornerRadius::same((8.0 * z) as u8), line);
        painter.galley(r.center() - g.size() / 2.0, g, Color32::WHITE);
    } else {
        painter.circle(c, 7.0 * z, bg, Stroke::new(1.2, accent));
        painter.line_segment(
            [c - vec2(3.5 * z, 0.0), c + vec2(3.5 * z, 0.0)],
            Stroke::new(1.5, accent),
        );
    }
}

/// Context-owned LRU: at most 128 previews (512² RGBA, <=128 MiB),
/// including cached failures. Stable URIs avoid per-frame hashing/decoding.
#[derive(Clone, Default)]
struct ImageCache(std::collections::VecDeque<(String, Option<egui::TextureHandle>)>);

/// Returns a cached, bounded preview texture for a valid embedded PNG/JPEG.
///
/// ResourceStore rejects legacy paths and URLs; declared media type must also
/// match the decoded content so opaque attachments are never treated as images.
pub fn resource_preview_texture(
    ctx: &egui::Context,
    sheet: &Sheet,
    reference: &str,
) -> Option<egui::TextureHandle> {
    let id = egui::Id::unique("local-image-cache");
    // Never call load_texture while holding the context data lock.
    let mut cache = ctx
        .data_mut(|data| data.remove_temp::<ImageCache>(id))
        .unwrap_or_default();
    let entry = if let Some(index) = cache.0.iter().position(|(id, _)| id == reference) {
        cache.0.remove(index).expect("existing cache entry")
    } else {
        let texture = sheet
            .resources
            .get(reference)
            .ok()
            .and_then(|resource| {
                let decoded = thinoda_core::images::decode(resource.bytes()).ok()?;
                (resource.media_type == decoded.media_type).then(|| decoded.preview())
            })
            .map(|preview| {
                ctx.load_texture(
                    reference,
                    egui::ColorImage::from_rgba_unmultiplied(
                        [preview.width as usize, preview.height as usize],
                        &preview.rgba,
                    ),
                    egui::TextureOptions::LINEAR,
                )
            });
        (reference.to_owned(), texture)
    };
    let texture = entry.1.clone();
    cache.0.push_back(entry);
    while cache.0.len() > 128 {
        cache.0.pop_front();
    }
    ctx.data_mut(|data| data.insert_temp(id, cache));
    texture
}

fn paint_image(painter: &Painter, sheet: &Sheet, reference: &str, rect: Rect, dim: bool) -> bool {
    let Some(texture) = resource_preview_texture(painter.ctx(), sheet, reference) else {
        return false;
    };
    {
        let source = texture.size_vec2();
        let scale = (rect.width() / source.x).min(rect.height() / source.y);
        let target = Rect::from_center_size(rect.center(), source * scale);
        painter.image(
            texture.id(),
            target,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE.gamma_multiply(if dim { 0.3 } else { 1.0 }),
        );
    }
    true
}

fn rotate_topic_shape(shape: &mut Shape, pivot: Pos2, angle: f32) {
    let rotation = egui::emath::Rot2::from_angle(angle);
    let rotate = |point: &mut Pos2| *point = pivot + rotation * (*point - pivot);
    match shape {
        Shape::Noop => {}
        Shape::Vec(shapes) => {
            for shape in shapes {
                rotate_topic_shape(shape, pivot, angle);
            }
        }
        Shape::Circle(circle) => rotate(&mut circle.center),
        Shape::Ellipse(ellipse) => {
            rotate(&mut ellipse.center);
            ellipse.angle += angle;
        }
        Shape::LineSegment { points, .. } => points.iter_mut().for_each(rotate),
        Shape::Path(path) => path.points.iter_mut().for_each(rotate),
        Shape::Band(band) => {
            *band = band.clone().with_angle_and_pivot(band.angle + angle, pivot);
        }
        Shape::Rect(rect) => {
            let center = rect.rect.center();
            let rotated = pivot + rotation * (center - pivot);
            rect.rect = rect.rect.translate(rotated - center);
            rect.angle += angle;
        }
        Shape::Text(text) => {
            rotate(&mut text.pos);
            text.angle += angle;
        }
        Shape::Mesh(mesh) => Arc::make_mut(mesh).rotate(rotation, pivot),
        Shape::QuadraticBezier(curve) => curve.points.iter_mut().for_each(rotate),
        Shape::CubicBezier(curve) => curve.points.iter_mut().for_each(rotate),
        Shape::Callback(_) => unreachable!("topic painters cannot emit GPU callbacks"),
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_topic(
    painter: &Painter,
    sheet: &Sheet,
    t: &TopicLayout,
    topic: Option<&thinoda_core::Topic>,
    xf: Xf,
    bg: Color32,
    dark: bool,
    dim: bool,
    editing: bool,
) {
    let z = xf.zoom;
    let a = |c: Color32| if dim { c.gamma_multiply(0.3) } else { c };
    let st = &t.style;
    let r = xf.rect(t.rect);
    let fill = st.fill.map(color32).map(a);
    let border = st.border_color.map(color32).map(a);
    let bw = (st.border_width * z).max(if border.is_some() { 0.5 } else { 0.0 });
    let stroke = border.map(|c| Stroke::new(bw, c)).unwrap_or(Stroke::NONE);
    let card = topic.and_then(|topic| topic.card.as_ref());
    let card_fill = card.map(|card| {
        let fallback = match card.kind {
            CardKind::Sticky => {
                if dark {
                    Color32::from_rgb(0x9B, 0x7B, 0x24)
                } else {
                    Color32::from_rgb(0xFF, 0xF2, 0x9E)
                }
            }
            CardKind::Text => {
                if dark {
                    Color32::from_rgb(0x3E, 0x48, 0x58)
                } else {
                    Color32::from_rgb(0xF5, 0xF7, 0xFA)
                }
            }
            CardKind::Image => {
                if dark {
                    Color32::from_rgb(0x35, 0x4C, 0x65)
                } else {
                    Color32::from_rgb(0xD9, 0xEA, 0xF7)
                }
            }
        };
        card.color.map(color32).unwrap_or(fallback)
    });
    if let (Some(card), Some(card_fill)) = (card, card_fill) {
        let corners = rotated_corners(t.rect, card.rotation)
            .into_iter()
            .map(|point| xf.pos(point))
            .collect();
        painter.add(Shape::convex_polygon(
            corners,
            card_fill,
            Stroke::new(1.0, card_fill.gamma_multiply(0.72)),
        ));
    } else {
        match st.shape {
            TopicShape::RoundedRect => {
                let rad = (6.0 * z).min(r.height() / 2.0).min(255.0) as u8;
                painter.rect(
                    r,
                    CornerRadius::same(rad),
                    fill.unwrap_or(Color32::TRANSPARENT),
                    stroke,
                    StrokeKind::Inside,
                );
            }
            TopicShape::Rect => {
                painter.rect(
                    r,
                    CornerRadius::ZERO,
                    fill.unwrap_or(Color32::TRANSPARENT),
                    stroke,
                    StrokeKind::Inside,
                );
            }
            TopicShape::Capsule => {
                painter.rect(
                    r,
                    CornerRadius::same((r.height() / 2.0).min(255.0) as u8),
                    fill.unwrap_or(Color32::TRANSPARENT),
                    stroke,
                    StrokeKind::Inside,
                );
            }
            TopicShape::Ellipse => {
                if let Some(f) = fill {
                    painter.add(Shape::ellipse_filled(r.center(), r.size() / 2.0, f));
                }
                if stroke.width > 0.0 {
                    painter.add(Shape::ellipse_stroke(r.center(), r.size() / 2.0, stroke));
                }
            }
            TopicShape::Diamond => {
                let pts = vec![
                    r.center_top(),
                    r.right_center(),
                    r.center_bottom(),
                    r.left_center(),
                ];
                painter.add(Shape::convex_polygon(
                    pts,
                    fill.unwrap_or(Color32::TRANSPARENT),
                    stroke,
                ));
            }
            TopicShape::Underline => {
                if let Some(f) = fill {
                    painter.rect_filled(r, CornerRadius::same((4.0 * z) as u8), f);
                }
                let w = (st.line_width * z).max(0.75);
                painter.line_segment(
                    [r.left_bottom(), r.right_bottom()],
                    Stroke::new(w, a(color32(st.line_color))),
                );
            }
            TopicShape::None => {
                if let Some(f) = fill {
                    painter.rect_filled(r, CornerRadius::same((4.0 * z) as u8), f);
                }
            }
        }
    }

    let rotation = card.map_or(0.0, |card| card.rotation);
    let content_start = (rotation != 0.0).then(|| {
        painter
            .ctx()
            .graphics_mut(|graphics| graphics.entry(painter.layer_id()).next_idx())
    });
    let text_color = if let Some(card_fill) = card_fill {
        let explicit = topic.and_then(|topic| topic.style.text_color);
        let color = explicit.map(color32).unwrap_or_else(|| {
            let under = core_color32(card_fill).over(core_color32(bg));
            color32(Color::highest_contrast_on(under))
        });
        a(color)
    } else {
        a(color32(st.text_color))
    };
    // Visible fallback for legacy/malformed images. Successful textures replace it.
    if let Some(ir) = t.image_rect {
        let visible = if rotation == 0.0 {
            ir
        } else {
            rotated_bounds(
                thinoda_core::Rect::from_center_size(
                    rotate_around(ir.center(), t.rect.center(), rotation),
                    ir.size,
                ),
                rotation,
            )
        };
        let ir = xf.rect(ir);
        let rendered = topic
            .and_then(|t| t.image.as_deref())
            .is_some_and(|reference| {
                !painter.clip_rect().intersects(xf.rect(visible))
                    || paint_image(painter, sheet, reference, ir, dim)
            });
        if !rendered {
            painter.rect(
                ir,
                CornerRadius::same((4.0 * z) as u8),
                text_color.gamma_multiply(0.08),
                Stroke::new(1.0, text_color.gamma_multiply(0.3)),
                StrokeKind::Inside,
            );
            painter.text(
                ir.center(),
                egui::Align2::CENTER_CENTER,
                "🖼",
                FontId::proportional(24.0 * z),
                text_color.gamma_multiply(0.6),
            );
        }
    }

    // To-Do checkbox.
    if let (Some(tr), Some(done)) = (t.todo_rect, topic.and_then(|x| x.todo)) {
        let done = t.todo_progress.map_or(done, |p| p.complete());
        let tr = xf.rect(tr).shrink(1.0 * z);
        let rad = CornerRadius::same((3.0 * z) as u8);
        if done {
            painter.rect_filled(tr, rad, a(Color32::from_rgb(0x2D, 0xA4, 0x4E)));
            let p = |x: f32, y: f32| pos2(tr.left() + tr.width() * x, tr.top() + tr.height() * y);
            painter.add(Shape::line(
                vec![p(0.22, 0.52), p(0.42, 0.72), p(0.78, 0.3)],
                Stroke::new(1.8 * z, Color32::WHITE),
            ));
        } else {
            painter.rect_stroke(
                tr,
                rad,
                Stroke::new(1.5 * z, text_color),
                StrokeKind::Inside,
            );
            if t.todo_progress.is_some_and(|p| p.completed > 0) {
                painter.line_segment(
                    [
                        pos2(tr.left() + tr.width() * 0.25, tr.center().y),
                        pos2(tr.right() - tr.width() * 0.25, tr.center().y),
                    ],
                    Stroke::new(1.8 * z, text_color),
                );
            }
        }
    }

    // Structural number.
    if let Some((label, nr)) = &t.number {
        let nr = xf.rect(*nr);
        let font = FontId::new(
            crate::measure::render_font_size(st.font_size * z),
            crate::measure::font_family(painter.ctx(), st.font_family.as_deref()),
        );
        let pos = pos2(nr.left(), nr.center().y);
        if st.bold {
            painter.text(
                pos + vec2(0.45 * z, 0.0),
                egui::Align2::LEFT_CENTER,
                label,
                font.clone(),
                text_color,
            );
        }
        painter.text(pos, egui::Align2::LEFT_CENTER, label, font, text_color);
    }

    // Markers.
    for (index, (m, mr)) in t.markers.iter().enumerate() {
        let color = topic.and_then(|t| t.marker_colors.get(&index)).copied();
        paint_marker_colored(painter, m, xf.rect(*mr), dim, color);
    }

    // Title.
    if !editing && let Some(topic) = topic {
        let tr = xf.rect(t.title_rect);
        let font = FontId::new(
            crate::measure::render_font_size(st.font_size * z),
            crate::measure::font_family(painter.ctx(), st.font_family.as_deref()),
        );
        let mut job = LayoutJob::default();
        job.wrap.max_width = tr.width() / if st.bold { BOLD_WIDTH } else { 1.0 } + 1.0;
        job.halign = match st.align {
            TextAlign::Left => Align::LEFT,
            TextAlign::Center => Align::Center,
            TextAlign::Right => Align::RIGHT,
        };
        let line = Stroke::new((st.font_size * z / 14.0).max(0.6), text_color);
        job.append(
            &topic.title,
            0.0,
            TextFormat {
                font_id: font,
                color: text_color,
                italics: st.italic,
                underline: if st.underline { line } else { Stroke::NONE },
                strikethrough: if st.strikethrough { line } else { Stroke::NONE },
                ..Default::default()
            },
        );
        let g = painter.layout_job(job);
        let x = match st.align {
            TextAlign::Left => tr.left(),
            TextAlign::Center => tr.center().x,
            TextAlign::Right => tr.right(),
        };
        let pos = pos2(x, tr.center().y - g.size().y / 2.0);
        if st.bold {
            painter.galley(pos + vec2(0.45 * z, 0.0), g.clone(), text_color);
        }
        painter.galley(pos, g, text_color);
    }

    // Icons.
    for (icon, ir) in &t.icons {
        let glyph = match icon {
            Icon::Link => "🔗",
            Icon::Notes => "📝",
        };
        let ir = xf.rect(*ir);
        painter.text(
            ir.center(),
            egui::Align2::CENTER_CENTER,
            glyph,
            FontId::proportional(crate::measure::render_font_size(ir.height() * 0.9)),
            text_color.gamma_multiply(0.8),
        );
    }

    // Labels.
    for (l, lr) in &t.labels {
        let lr = xf.rect(*lr);
        let chip = if bg.r() < 128 {
            Color32::from_gray(70)
        } else {
            Color32::from_rgb(0xE6, 0xEA, 0xF0)
        };
        let fg = if bg.r() < 128 {
            Color32::from_gray(225)
        } else {
            Color32::from_rgb(0x44, 0x4C, 0x58)
        };
        painter.rect_filled(
            lr,
            CornerRadius::same((lr.height() / 2.0).min(255.0) as u8),
            a(chip),
        );
        painter.text(
            lr.center(),
            egui::Align2::CENTER_CENTER,
            l,
            FontId::proportional((st.font_size * 0.72).max(9.0) * z),
            a(fg),
        );
    }

    if let Some(start) = content_start {
        let pivot = xf.pos(t.rect.center());
        let angle = rotation.to_radians();
        painter.ctx().graphics_mut(|graphics| {
            let list = graphics.entry(painter.layer_id());
            for index in start.0..list.next_idx().0 {
                list.mutate_shape(egui::layers::ShapeIdx(index), |clipped| {
                    rotate_topic_shape(&mut clipped.shape, pivot, angle);
                });
            }
        });
    }
}

/// Vector-rendered marker icons (robust regardless of installed emoji fonts).
pub fn paint_marker_colored(painter: &Painter, id: &str, r: Rect, dim: bool, color: Option<Color>) {
    let a = |c: Color32| if dim { c.gamma_multiply(0.3) } else { c };
    let ink = color.unwrap_or_else(|| thinoda_core::markers::default_color(id));
    let foreground = color32(ink);
    let detail = color
        .map(|c| color32(Color::highest_contrast_on(c.over(Color::WHITE))))
        .unwrap_or(Color32::WHITE);
    let c = r.center();
    let rad = r.width().min(r.height()) / 2.0;
    let (group, rest) = id.split_once('-').unwrap_or((id, ""));
    match group {
        "priority" => {
            painter.circle_filled(c, rad, a(foreground));
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                rest,
                FontId::proportional(crate::measure::render_font_size(rad * 1.3)),
                a(detail),
            );
        }
        "task" => {
            let frac = thinoda_core::markers::task_fraction(id);
            let green = a(foreground);
            painter.circle(c, rad - 0.5, a(detail), Stroke::new(1.2, green));
            if frac >= 1.0 {
                painter.circle_filled(c, rad - 0.5, green);
                painter.add(Shape::line(
                    vec![
                        c + vec2(-rad * 0.45, 0.0),
                        c + vec2(-rad * 0.1, rad * 0.35),
                        c + vec2(rad * 0.5, -rad * 0.35),
                    ],
                    Stroke::new(1.5, a(detail)),
                ));
            } else if frac > 0.0 {
                let mut pts = vec![c];
                let steps = (24.0_f32 * frac).ceil() as usize + 1;
                for i in 0..=steps {
                    let ang = -std::f32::consts::FRAC_PI_2
                        + std::f32::consts::TAU * frac * i as f32 / steps as f32;
                    pts.push(c + vec2(ang.cos(), ang.sin()) * (rad - 1.5));
                }
                painter.add(Shape::Path(egui::epaint::PathShape {
                    points: pts,
                    closed: true,
                    fill: green,
                    stroke: Stroke::NONE.into(),
                }));
            }
        }
        "flag" => {
            let col = a(foreground);
            let x0 = r.left() + r.width() * 0.25;
            painter.line_segment(
                [pos2(x0, r.top() + 1.0), pos2(x0, r.bottom() - 1.0)],
                Stroke::new(1.4, a(color.map(color32).unwrap_or(Color32::from_gray(90)))),
            );
            painter.add(Shape::convex_polygon(
                vec![
                    pos2(x0, r.top() + 1.5),
                    pos2(r.right() - 1.0, r.top() + r.height() * 0.3),
                    pos2(x0, r.top() + r.height() * 0.6),
                ],
                col,
                Stroke::NONE,
            ));
        }
        "star" => {
            let col = a(foreground);
            let mut pts = Vec::with_capacity(10);
            for i in 0..10 {
                let ang = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
                let rr = if i % 2 == 0 { rad } else { rad * 0.45 };
                pts.push(c + vec2(ang.cos(), ang.sin()) * rr);
            }
            painter.add(Shape::Path(egui::epaint::PathShape {
                points: pts,
                closed: true,
                fill: col,
                stroke: Stroke::NONE.into(),
            }));
        }
        _ => {
            let glyph = thinoda_core::markers::glyph(id);
            let font = FontId::proportional(crate::measure::render_font_size(rad * 1.6));
            let has = painter.ctx().fonts_mut(|f| f.has_glyphs(&font, glyph));
            if has && !glyph.is_empty() {
                painter.text(c, egui::Align2::CENTER_CENTER, glyph, font, a(foreground));
            } else {
                painter.circle_filled(
                    c,
                    rad,
                    a(color
                        .map(color32)
                        .unwrap_or(Color32::from_rgb(0x8A, 0x8F, 0x98))),
                );
                let initial: String = id.chars().take(1).collect::<String>().to_uppercase();
                painter.text(
                    c,
                    egui::Align2::CENTER_CENTER,
                    initial,
                    FontId::proportional(crate::measure::render_font_size(rad * 1.2)),
                    a(detail),
                );
            }
        }
    }
}

/// Polyline (sheet coords) for a branch connector.
pub fn connector_path(kind: ConnectorKind, vertical: bool, s: Vec2, e: Vec2) -> Vec<Vec2> {
    let dx = e.x - s.x;
    let dy = e.y - s.y;
    let sample = |c1: Vec2, c2: Vec2| {
        (0..=20)
            .map(|i| cubic(s, c1, c2, e, i as f32 / 20.0))
            .collect::<Vec<_>>()
    };
    match kind {
        ConnectorKind::Straight => vec![s, e],
        ConnectorKind::Curve => {
            if vertical {
                sample(
                    Vec2::new(s.x, s.y + dy * 0.5),
                    Vec2::new(e.x, s.y + dy * 0.5),
                )
            } else {
                sample(
                    Vec2::new(s.x + dx * 0.5, s.y),
                    Vec2::new(s.x + dx * 0.5, e.y),
                )
            }
        }
        ConnectorKind::Brace => sample(
            Vec2::new(s.x + dx * 0.9, s.y),
            Vec2::new(s.x + dx * 0.1, e.y),
        ),
        ConnectorKind::Elbow | ConnectorKind::RoundedElbow => {
            let (a, b) = if vertical {
                let my = s.y + dy * 0.5;
                (Vec2::new(s.x, my), Vec2::new(e.x, my))
            } else {
                let mx = s.x + dx * 0.5;
                (Vec2::new(mx, s.y), Vec2::new(mx, e.y))
            };
            if kind == ConnectorKind::RoundedElbow {
                rounded(&[s, a, b, e], 8.0)
            } else {
                vec![s, a, b, e]
            }
        }
        ConnectorKind::Tree => rounded(&[s, Vec2::new(s.x, e.y), e], 6.0),
    }
}

/// Rounds polyline corners with quadratic arcs of radius `r`.
fn rounded(pts: &[Vec2], r: f32) -> Vec<Vec2> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut out = vec![pts[0]];
    for w in pts.windows(3) {
        let (p0, p1, p2) = (w[0], w[1], w[2]);
        let d1 = (p1 - p0).length();
        let d2 = (p2 - p1).length();
        let rr = r.min(d1 / 2.0).min(d2 / 2.0);
        if rr < 0.5 {
            out.push(p1);
            continue;
        }
        let a = p1 + (p0 - p1) * (rr / d1);
        let b = p1 + (p2 - p1) * (rr / d2);
        for i in 0..=6 {
            let t = i as f32 / 6.0;
            let q = a * ((1.0 - t) * (1.0 - t)) + p1 * (2.0 * (1.0 - t) * t) + b * (t * t);
            out.push(q);
        }
    }
    out.push(*pts.last().unwrap());
    out
}

/// Curly-brace polyline from `s` to `e` pointing to `tip`.
pub fn brace_path(s: Vec2, e: Vec2, tip: Vec2, dir: Dir) -> Vec<Vec2> {
    let horizontal = dir.is_horizontal();
    let mid = Vec2::new((s.x + e.x) / 2.0, (s.y + e.y) / 2.0);
    let (out_s, out_mid) = if horizontal {
        (Vec2::new(tip.x, s.y), Vec2::new(tip.x, mid.y))
    } else {
        (Vec2::new(s.x, tip.y), Vec2::new(mid.x, tip.y))
    };
    let mut v = Vec::new();
    let q = |a: Vec2, c: Vec2, b: Vec2, v: &mut Vec<Vec2>| {
        for i in 0..=10 {
            let t = i as f32 / 10.0;
            v.push(a * ((1.0 - t) * (1.0 - t)) + c * (2.0 * (1.0 - t) * t) + b * (t * t));
        }
    };
    let s_mid = Vec2::new((s.x + out_mid.x) / 2.0, (s.y + out_mid.y) / 2.0);
    let e_mid = Vec2::new((e.x + out_mid.x) / 2.0, (e.y + out_mid.y) / 2.0);
    let (c1, c2, c3, c4) = if horizontal {
        (
            Vec2::new(s_mid.x, s.y),
            Vec2::new(s_mid.x, out_mid.y),
            Vec2::new(e_mid.x, out_mid.y),
            Vec2::new(e_mid.x, e.y),
        )
    } else {
        (
            Vec2::new(s.x, s_mid.y),
            Vec2::new(out_mid.x, s_mid.y),
            Vec2::new(out_mid.x, e_mid.y),
            Vec2::new(e.x, e_mid.y),
        )
    };
    let _ = out_s;
    q(s, c1, s_mid, &mut v);
    q(s_mid, c2, out_mid, &mut v);
    q(out_mid, c3, e_mid, &mut v);
    q(e_mid, c4, e, &mut v);
    v
}

fn tapered_line(painter: &Painter, pts: &[Pos2], w0: f32, w1: f32, color: Color32) {
    let n = pts.len();
    for (i, w) in pts.windows(2).enumerate() {
        let t = i as f32 / (n.max(2) - 1) as f32;
        let width = w0 + (w1 - w0) * t;
        painter.line_segment([w[0], w[1]], Stroke::new(width, color));
        painter.circle_filled(w[1], width / 2.0, color);
    }
}

fn arrow_head(painter: &Painter, from: Pos2, to: Pos2, size: f32, color: Color32) {
    let d = (to - from).normalized();
    if !d.x.is_finite() {
        return;
    }
    let n = vec2(-d.y, d.x);
    let base = to - d * size;
    painter.add(Shape::convex_polygon(
        vec![to, base + n * size * 0.45, base - n * size * 0.45],
        color,
        Stroke::NONE,
    ));
}

fn dashed_rect(painter: &Painter, r: Rect, stroke: Stroke, dash: f32, gap: f32) {
    let pts = [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
        r.left_top(),
    ];
    for s in Shape::dashed_line(&pts, stroke, dash, gap) {
        painter.add(s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_opacity_preserves_premultiplied_color() {
        let translucent = Color32::from_rgba_unmultiplied(200, 100, 50, 128);
        assert_eq!(opacity_color(translucent, 1.0), translucent);
        assert_eq!(
            opacity_color(translucent, 0.5),
            translucent.gamma_multiply(0.5)
        );
    }

    #[test]
    fn kanban_zone_titles_have_contrast_in_both_themes_without_changing_tint_or_clip() {
        use thinoda_core::{ApproxMeasure, templates};

        let mut workbook = templates::instantiate("kanban-board");
        workbook
            .active_mut()
            .zones
            .iter_mut()
            .find(|zone| zone.name == "Done")
            .expect("Kanban template has a Done frame")
            .background_color = Some(Color::rgb(245, 220, 180));
        let sheet = workbook.active();
        let layout = thinoda_core::layout_sheet(sheet, &ApproxMeasure, false);
        let zone = sheet
            .zones
            .iter()
            .find(|zone| zone.name == "Done")
            .expect("Kanban template has a Done frame");
        let zone_color = color32(zone.color.expect("template frame has a custom tint"));
        let fill_color = color32(
            zone.background_color
                .expect("frame has a separate background tint"),
        );
        let zone_rect = Xf {
            origin: Pos2::ZERO,
            zoom: 1.0,
        }
        .rect(zone.frame);
        let ctx = egui::Context::default();
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 700.0));

        for dark in [false, true] {
            let background = sheet_background(sheet, dark);
            let fill_opacity = 0.16;
            let label_color = zone_title_color(sheet, background, fill_color, fill_opacity);
            let mut output = ctx.run_pass(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ctx| {
                    paint_map(
                        &ctx.layer_painter(egui::LayerId::background()),
                        sheet,
                        &layout,
                        Xf {
                            origin: Pos2::ZERO,
                            zoom: 1.0,
                        },
                        &PaintOptions {
                            dark,
                            ..Default::default()
                        },
                    );
                },
            );
            output.textures_delta.clear();

            let title = output
                .shapes
                .iter()
                .find_map(|clipped| match &clipped.shape {
                    Shape::Text(text) if text.galley.job.text == "Done" => Some((clipped, text)),
                    _ => None,
                })
                .expect("Done frame title is painted");
            assert_eq!(title.1.fallback_color, label_color);
            assert_eq!(title.0.clip_rect, zone_rect.shrink(4.0));
            let composited_fill = core_color32(fill_color.gamma_multiply(fill_opacity))
                .over(core_color32(background));
            assert!(
                core_color32(label_color).contrast_on(composited_fill) >= 4.5,
                "frame title should meet WCAG AA text contrast"
            );

            let frame = output
                .shapes
                .iter()
                .find_map(|clipped| match &clipped.shape {
                    Shape::Rect(rect) if rect.rect == zone_rect => Some(rect),
                    _ => None,
                })
                .expect("Done frame background is painted");
            assert_eq!(frame.fill, fill_color.gamma_multiply(fill_opacity));
            assert_eq!(frame.stroke.color, zone_color);
            let tint = zone.color.expect("template frame has a custom tint");
            let previous_tint = Color::rgba(tint.r, tint.g, tint.b, 0x30);
            assert!(
                tint.contrast_on(core_color32(background))
                    > previous_tint.contrast_on(core_color32(background)),
                "default frame outline should be more visible in {dark} theme"
            );
        }
    }

    #[test]
    fn legacy_zone_titles_keep_the_original_tint_color() {
        let sheet = Sheet::new("Map", "Root");
        let tint = Color32::from_rgb(90, 110, 125);
        assert!(!sheet.uses_board_semantics());
        assert_eq!(zone_title_color(&sheet, Color32::WHITE, tint, 0.16), tint);
    }

    #[test]
    fn shape_labels_honor_explicit_text_color_and_use_export_contrast_fallback() {
        let mut sheet = Sheet::new("S", "Root");
        let mut shape = CanvasObject::shape(
            ShapeKind::Rectangle,
            thinoda_core::Rect::from_center_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
        );
        if let CanvasObjectKind::Shape { label, .. } = &mut shape.kind {
            *label = "label".to_owned();
        }
        let fill = Color::rgb(20, 30, 40);
        let explicit = Color::rgba(30, 90, 200, 192);
        shape.style.fill = Some(fill);
        shape.style.text_color = Some(explicit);
        shape.style.opacity = 0.5;
        sheet.canvas.objects.push(shape);
        let layout = thinoda_core::layout_sheet(&sheet, &thinoda_core::ApproxMeasure, false);
        let ctx = egui::Context::default();
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0));
        let paint = |sheet: &Sheet| {
            ctx.run_pass(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ctx| {
                    paint_map(
                        &ctx.layer_painter(egui::LayerId::background()),
                        sheet,
                        &layout,
                        Xf {
                            origin: pos2(300.0, 250.0),
                            zoom: 1.0,
                        },
                        &PaintOptions::default(),
                    );
                },
            )
        };
        let label_color = |output: &egui::FullOutput| {
            output
                .shapes
                .iter()
                .find_map(|clipped| match &clipped.shape {
                    Shape::Text(text) if text.galley.job.text == "label" => {
                        Some(text.fallback_color)
                    }
                    _ => None,
                })
                .expect("shape label should be painted")
        };

        let mut explicit_output = paint(&sheet);
        assert_eq!(
            label_color(&explicit_output),
            opacity_color(color32(explicit), 0.5)
        );
        explicit_output.textures_delta.clear();

        sheet.canvas.objects[0].style.text_color = None;
        let mut fallback_output = paint(&sheet);
        let under = fill.over(core_color32(sheet_background(&sheet, false)));
        let expected = opacity_color(color32(Color::highest_contrast_on(under)), 0.5);
        assert_eq!(label_color(&fallback_output), expected);
        fallback_output.textures_delta.clear();
    }

    #[test]
    fn shape_labels_rotate_about_the_shape_center_at_any_zoom() {
        let mut sheet = Sheet::new("S", "Root");
        let mut shape = CanvasObject::shape(
            ShapeKind::Triangle,
            thinoda_core::Rect::from_center_size(Vec2::ZERO, Vec2::new(200.0, 120.0)),
        );
        if let CanvasObjectKind::Shape { label, .. } = &mut shape.kind {
            *label = "Rotating label".to_owned();
        }
        sheet.canvas.objects.push(shape);
        let layout = thinoda_core::layout_sheet(&sheet, &thinoda_core::ApproxMeasure, false);
        let ctx = egui::Context::default();
        for zoom in [0.2, 1.5, 3.0] {
            let xf = Xf {
                origin: pos2(300.0, 250.0),
                zoom,
            };
            for angle in [0.0_f32, 45.0, 90.0, -90.0] {
                let shape = &mut sheet.canvas.objects[0];
                shape.rotation = angle;
                let label_center = shape_label_rect(ShapeKind::Triangle, shape.rect).center();
                let expected = xf.pos(rotate_around(label_center, shape.rect.center(), angle));
                let mut output = ctx.run_pass(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
                        ..Default::default()
                    },
                    |ctx| {
                        paint_map(
                            &ctx.layer_painter(egui::LayerId::background()),
                            &sheet,
                            &layout,
                            xf,
                            &PaintOptions::default(),
                        );
                    },
                );
                let text = output
                    .shapes
                    .iter()
                    .find_map(|clipped| match &clipped.shape {
                        Shape::Text(text) if text.galley.job.text == "Rotating label" => {
                            Some((text.angle, text.pos, text.galley.size()))
                        }
                        _ => None,
                    });
                output.textures_delta.clear();
                let (actual_angle, position, size) = text.expect("shape label should be painted");
                assert!((actual_angle - angle.to_radians()).abs() < 0.0001);
                let center = position + egui::emath::Rot2::from_angle(actual_angle) * (size / 2.0);
                assert!(
                    center.distance(expected) < 0.1,
                    "label center {center:?} differs from {expected:?} at {angle} degrees and {zoom} zoom"
                );
            }
        }
    }

    #[test]
    fn a_rotated_label_remains_visible_when_its_shape_is_outside_the_viewport() {
        let mut sheet = Sheet::new("S", "Root");
        let mut shape = CanvasObject::shape(
            ShapeKind::Triangle,
            thinoda_core::Rect::from_center_size(Vec2::new(140.0, 0.0), Vec2::new(20.0, 20.0)),
        );
        shape.rotation = 45.0;
        if let CanvasObjectKind::Shape { label, .. } = &mut shape.kind {
            *label = "Rotating label extends into viewport".to_owned();
        }
        sheet.canvas.objects.push(shape);
        let layout = thinoda_core::layout_sheet(&sheet, &thinoda_core::ApproxMeasure, false);
        let ctx = egui::Context::default();
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(200.0, 200.0));
        let label_bounds = |sheet: &Sheet| {
            let mut output = ctx.run_pass(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ctx| {
                    paint_map(
                        &ctx.layer_painter(egui::LayerId::background()),
                        sheet,
                        &layout,
                        Xf {
                            origin: pos2(100.0, 100.0),
                            zoom: 1.0,
                        },
                        &PaintOptions::default(),
                    );
                },
            );
            let bounds = output
                .shapes
                .iter()
                .find_map(|clipped| match &clipped.shape {
                    Shape::Text(text)
                        if text.galley.job.text == "Rotating label extends into viewport" =>
                    {
                        Some(text.visual_bounding_rect())
                    }
                    _ => None,
                });
            output.textures_delta.clear();
            bounds
        };
        assert!(
            label_bounds(&sheet)
                .expect("visible label should not be culled with its offscreen shape")
                .intersects(screen)
        );
        sheet.zones.push(thinoda_core::Zone {
            id: thinoda_core::ZoneId(1),
            name: "Offscreen clip".into(),
            frame: thinoda_core::Rect::from_center_size(
                Vec2::new(140.0, 0.0),
                Vec2::new(40.0, 40.0),
            ),
            roots: vec![],
            objects: vec![],
            keep_empty: true,
            color: None,
            background_color: None,
            clip: true,
        });
        assert!(label_bounds(&sheet).is_none());
        sheet.zones.clear();
        sheet.canvas.objects[0].rect.min.x += 2_000.0;
        assert!(label_bounds(&sheet).is_none());
    }

    #[test]
    fn card_title_auto_color_uses_card_fill_and_explicit_color_wins() {
        let mut sheet = Sheet::new("S", "Card title");
        let card_color = Color::rgb(18, 30, 52);
        let card = sheet.root;
        {
            let topic = sheet.topic_mut(card).unwrap();
            topic.style.fill = Some(Color::rgb(250, 250, 250));
            topic.style.text_color = None;
            topic.card = Some(thinoda_core::Card {
                kind: CardKind::Sticky,
                size: Vec2::new(150.0, 90.0),
                rotation: 0.0,
                color: Some(card_color),
            });
        }

        let ctx = egui::Context::default();
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0));
        let label_color = |sheet: &Sheet| {
            let layout = thinoda_core::layout_sheet(sheet, &thinoda_core::ApproxMeasure, false);
            let mut output = ctx.run_pass(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ctx| {
                    paint_map(
                        &ctx.layer_painter(egui::LayerId::background()),
                        sheet,
                        &layout,
                        Xf {
                            origin: pos2(300.0, 250.0),
                            zoom: 1.0,
                        },
                        &PaintOptions::default(),
                    );
                },
            );
            let color = output
                .shapes
                .iter()
                .find_map(|clipped| match &clipped.shape {
                    Shape::Text(text) if text.galley.job.text == "Card title" => {
                        Some(text.fallback_color)
                    }
                    _ => None,
                })
                .expect("card title should be painted");
            output.textures_delta.clear();
            color
        };

        let under = card_color.over(core_color32(sheet_background(&sheet, false)));
        assert_eq!(
            label_color(&sheet),
            color32(Color::highest_contrast_on(under))
        );

        let explicit = Color::rgb(170, 35, 95);
        sheet.topic_mut(card).unwrap().style.text_color = Some(explicit);
        assert_eq!(label_color(&sheet), color32(explicit));
    }

    #[test]
    fn thumbnails_fit_rotated_cards_and_object_only_boards() {
        let thumbnail = Rect::from_min_size(Pos2::ZERO, vec2(240.0, 80.0));
        let fill = Color::rgb(190, 80, 120);
        for objects_only in [false, true] {
            let mut sheet = Sheet::new("Board", "");
            sheet.root_mode = thinoda_core::SheetRootMode::Hidden;
            if objects_only {
                let mut shape = CanvasObject::shape(
                    ShapeKind::Rectangle,
                    thinoda_core::Rect::from_center_size(
                        Vec2::new(2_000.0, 3_000.0),
                        Vec2::new(80.0, 40.0),
                    ),
                );
                shape.style.fill = Some(fill);
                sheet.add_canvas_object(shape).unwrap();
            } else {
                let mut card = thinoda_core::Card::new(CardKind::Sticky, Vec2::new(300.0, 100.0));
                card.rotation = 90.0;
                card.color = Some(fill);
                sheet.add_card("Card", Vec2::ZERO, card).unwrap();
            }
            let layout = thinoda_core::layout_sheet(&sheet, &thinoda_core::ApproxMeasure, false);
            let ctx = egui::Context::default();
            let mut output = ctx.run_pass(
                egui::RawInput {
                    screen_rect: Some(thumbnail),
                    ..Default::default()
                },
                |ctx| {
                    paint_thumbnail(
                        &ctx.layer_painter(egui::LayerId::background()),
                        &sheet,
                        &layout,
                        thumbnail,
                        false,
                    );
                },
            );
            let path = output
                .shapes
                .iter()
                .find_map(|clipped| match &clipped.shape {
                    Shape::Path(path) if path.fill == color32(fill) => Some(path),
                    _ => None,
                })
                .expect("thumbnail should paint its board content");
            let available = thumbnail.shrink(5.0).expand(0.1);
            assert!(path.points.iter().all(|point| available.contains(*point)));
            let bounds = path
                .points
                .iter()
                .copied()
                .fold(Rect::NOTHING, |bounds, point| {
                    bounds.union(Rect::from_min_max(point, point))
                });
            assert!(
                bounds.height() > 25.0,
                "board content should fill the preview"
            );
            output.textures_delta.clear();
        }
    }

    fn decorated_image_card() -> (Sheet, TopicId) {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(160, 80)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = thinoda_core::SheetRootMode::Hidden;
        let id = sheet
            .add_card(
                "Rotated title",
                Vec2::ZERO,
                thinoda_core::Card::new(CardKind::Image, Vec2::new(340.0, 260.0)),
            )
            .unwrap();
        thinoda_core::images::set_image(
            &mut sheet,
            id,
            Some((
                thinoda_core::resources::Resource::new("test.png", "image/png", bytes.into_inner())
                    .unwrap(),
                Vec2::new(160.0, 80.0),
            )),
        )
        .unwrap();
        let topic = sheet.topic_mut(id).unwrap();
        topic.todo = Some(false);
        topic.hyperlink = Some("https://example.com".into());
        topic.markers.push("priority-1".into());
        topic.labels.push("Tag".into());
        (sheet, id)
    }

    #[test]
    fn rotated_card_text_images_and_annotations_share_the_card_pivot() {
        fn text<'a>(
            shapes: &'a [egui::epaint::ClippedShape],
            value: &str,
        ) -> &'a egui::epaint::TextShape {
            shapes
                .iter()
                .find_map(|clipped| match &clipped.shape {
                    Shape::Text(text) if text.galley.job.text == value => Some(text),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing card text {value}"))
        }

        fn image_mesh(shapes: &[egui::epaint::ClippedShape]) -> &egui::epaint::Mesh {
            shapes
                .iter()
                .find_map(|clipped| match &clipped.shape {
                    Shape::Mesh(mesh) => Some(mesh.as_ref()),
                    _ => None,
                })
                .expect("missing card image mesh")
        }

        let (mut sheet, id) = decorated_image_card();
        let layout = thinoda_core::layout_sheet(&sheet, &thinoda_core::ApproxMeasure, false);
        let topic = &layout.topics[&id];
        let ctx = egui::Context::default();
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 1200.0));
        for zoom in [0.5, 1.0, 2.0] {
            let xf = Xf {
                origin: pos2(700.0, 500.0),
                zoom,
            };
            let mut paint = |angle| {
                sheet.topic_mut(id).unwrap().card.as_mut().unwrap().rotation = angle;
                let mut output = ctx.run_pass(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        ..Default::default()
                    },
                    |ctx| {
                        let painter = ctx.layer_painter(egui::LayerId::background());
                        paint_topic(
                            &painter.with_clip_rect(screen),
                            &sheet,
                            topic,
                            sheet.topic(id),
                            xf,
                            sheet_background(&sheet, false),
                            false,
                            false,
                            false,
                        );
                    },
                );
                output.textures_delta.clear();
                output.shapes
            };
            let unrotated = paint(0.0);
            let pivot = xf.pos(topic.rect.center());
            for angle in [45.0_f32, 90.0, -90.0] {
                let rotated = paint(angle);
                assert_eq!(unrotated.len(), rotated.len());
                assert!(
                    unrotated
                        .iter()
                        .zip(&rotated)
                        .all(|(before, after)| before.clip_rect == after.clip_rect)
                );
                let rotation = egui::emath::Rot2::from_angle(angle.to_radians());
                for value in ["Rotated title", "Tag", "1"] {
                    let before = text(&unrotated, value);
                    let after = text(&rotated, value);
                    let expected = pivot + rotation * (before.pos - pivot);
                    assert!(
                        after.pos.distance(expected) < 0.1,
                        "{value} at {angle} degrees and {zoom} zoom"
                    );
                    assert!((after.angle - angle.to_radians()).abs() < 0.0001);
                }
                let before = image_mesh(&unrotated);
                let after = image_mesh(&rotated);
                assert_eq!(before.texture_id, after.texture_id);
                assert_eq!(before.indices, after.indices);
                for (before, after) in before.vertices.iter().zip(&after.vertices) {
                    let expected = pivot + rotation * (before.pos - pivot);
                    assert!(after.pos.distance(expected) < 0.1);
                    assert_eq!(before.uv, after.uv);
                }

                let check = xf.rect(topic.todo_rect.unwrap()).shrink(zoom);
                let index = unrotated
                    .iter()
                    .position(
                        |shape| matches!(&shape.shape, Shape::Rect(rect) if rect.rect == check),
                    )
                    .expect("missing to-do checkbox");
                let Shape::Rect(after) = &rotated[index].shape else {
                    panic!("checkbox shape changed kind");
                };
                let expected = pivot + rotation * (check.center() - pivot);
                assert!(after.rect.center().distance(expected) < 0.1);
                assert!((after.angle - angle.to_radians()).abs() < 0.0001);
            }
        }
    }

    #[test]
    fn rotated_card_image_culling_uses_the_rotated_image_bounds() {
        let (mut sheet, id) = decorated_image_card();
        let mut layout = thinoda_core::layout_sheet(&sheet, &thinoda_core::ApproxMeasure, false);
        let topic = layout.topics.get_mut(&id).unwrap();
        let center = topic.rect.center();
        topic.image_rect = Some(thinoda_core::Rect::from_center_size(
            center + Vec2::new(-100.0, 0.0),
            Vec2::new(20.0, 20.0),
        ));
        let xf = Xf {
            origin: pos2(500.0, 500.0),
            zoom: 1.0,
        };
        let rotated_center = rotate_around(topic.image_rect.unwrap().center(), center, 90.0);
        let clip = Rect::from_center_size(xf.pos(rotated_center), vec2(30.0, 30.0));
        assert!(!clip.intersects(xf.rect(topic.image_rect.unwrap())));
        let ctx = egui::Context::default();
        let mut paint = |angle| {
            sheet.topic_mut(id).unwrap().card.as_mut().unwrap().rotation = angle;
            let mut output = ctx.run_pass(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 1000.0))),
                    ..Default::default()
                },
                |ctx| {
                    paint_topic(
                        &ctx.layer_painter(egui::LayerId::background())
                            .with_clip_rect(clip),
                        &sheet,
                        topic,
                        sheet.topic(id),
                        xf,
                        sheet_background(&sheet, false),
                        false,
                        false,
                        false,
                    );
                },
            );
            let painted = output
                .shapes
                .iter()
                .any(|shape| matches!(&shape.shape, Shape::Mesh(_)));
            output.textures_delta.clear();
            painted
        };
        assert!(!paint(0.0));
        assert!(paint(90.0));
    }

    #[test]
    fn rotated_card_text_is_painted_when_only_its_rotated_position_is_visible() {
        let (mut sheet, id) = decorated_image_card();
        sheet.topic_mut(id).unwrap().card.as_mut().unwrap().rotation = 90.0;
        let mut layout = thinoda_core::layout_sheet(&sheet, &thinoda_core::ApproxMeasure, false);
        let topic = layout.topics.get_mut(&id).unwrap();
        topic.title_rect = thinoda_core::Rect::from_center_size(
            topic.rect.center() + Vec2::new(-100.0, 0.0),
            Vec2::new(100.0, 20.0),
        );
        let xf = Xf {
            origin: pos2(500.0, 500.0),
            zoom: 1.0,
        };
        let rotated_center = rotate_around(topic.title_rect.center(), topic.rect.center(), 90.0);
        let clip = Rect::from_center_size(xf.pos(rotated_center), vec2(80.0, 40.0));
        assert!(!clip.intersects(xf.rect(topic.title_rect)));
        let ctx = egui::Context::default();
        let mut output = ctx.run_pass(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 1000.0))),
                ..Default::default()
            },
            |ctx| {
                paint_topic(
                    &ctx.layer_painter(egui::LayerId::background())
                        .with_clip_rect(clip),
                    &sheet,
                    topic,
                    sheet.topic(id),
                    xf,
                    sheet_background(&sheet, false),
                    false,
                    false,
                    false,
                );
            },
        );
        assert!(output.shapes.iter().any(|shape| {
            matches!(&shape.shape, Shape::Text(text)
                if text.galley.job.text == "Rotated title"
                    && text.visual_bounding_rect().intersects(clip))
        }));
        output.textures_delta.clear();
    }

    #[test]
    fn rotated_card_contents_keep_the_screen_space_frame_clip() {
        let (mut sheet, id) = decorated_image_card();
        sheet.topic_mut(id).unwrap().card.as_mut().unwrap().rotation = 45.0;
        let frame = thinoda_core::Rect::from_center_size(Vec2::ZERO, Vec2::new(100.0, 100.0));
        sheet.create_zone("Clip", vec![id], frame).unwrap();
        sheet.zones[0].clip = true;
        let layout = thinoda_core::layout_sheet(&sheet, &thinoda_core::ApproxMeasure, false);
        let xf = Xf {
            origin: pos2(300.0, 250.0),
            zoom: 1.5,
        };
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0));
        let ctx = egui::Context::default();
        let mut output = ctx.run_pass(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ctx| {
                paint_map(
                    &ctx.layer_painter(egui::LayerId::background())
                        .with_clip_rect(screen),
                    &sheet,
                    &layout,
                    xf,
                    &PaintOptions::default(),
                );
            },
        );
        let title = output
            .shapes
            .iter()
            .find(|shape| {
                matches!(&shape.shape, Shape::Text(text) if text.galley.job.text == "Rotated title")
            })
            .expect("missing rotated card title");
        assert_eq!(title.clip_rect, xf.rect(frame).intersect(screen));
        output.textures_delta.clear();
    }

    #[test]
    fn ink_mesh_cache_reuses_and_invalidates_projected_meshes() {
        let ctx = egui::Context::default();
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(200.0, 100.0));
        let mut cache = InkGeometryCache::default();
        let object = CanvasObjectId(501);
        let source: Arc<[Vec2]> = [Vec2::ZERO, Vec2::new(10.0, 0.0)].into();
        let center = Vec2::ZERO;
        let color = Color32::from_rgba_unmultiplied(190, 45, 80, 72);
        let xf = Xf {
            origin: pos2(20.0, 30.0),
            zoom: 1.0,
        };
        let make_mesh = |cache: &mut InkGeometryCache,
                         source: &Arc<[Vec2]>,
                         rotation: f32,
                         xf: Xf,
                         stroke_width: f32,
                         color: Color32,
                         clip_rect: Rect| {
            let mut mesh = None;
            let mut output = ctx.run_pass(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ctx| {
                    let painter = ctx
                        .layer_painter(egui::LayerId::background())
                        .with_clip_rect(clip_rect);
                    mesh = ink_tessellated_mesh(
                        &painter,
                        cache,
                        object,
                        source,
                        center,
                        rotation,
                        xf,
                        stroke_width,
                        color,
                        ink_tessellation_config(ctx),
                    );
                },
            );
            output.textures_delta.clear();
            mesh.expect("line should tessellate")
        };
        let first = make_mesh(&mut cache, &source, 0.0, xf, 3.0, color, screen);
        let reused = make_mesh(&mut cache, &source, 0.0, xf, 3.0, color, screen);
        assert!(Arc::ptr_eq(&first, &reused));
        assert!(Arc::ptr_eq(
            &cache.entries.get(&object).unwrap().source_points,
            &source
        ));
        let equivalent: Arc<[Vec2]> = source.as_ref().to_vec().into();
        assert!(!Arc::ptr_eq(&source, &equivalent));
        let equivalent_mesh = make_mesh(&mut cache, &equivalent, 0.0, xf, 3.0, color, screen);
        assert!(Arc::ptr_eq(&first, &equivalent_mesh));
        assert!(!first.indices.is_empty());
        assert!(first.is_valid());
        let expected_points: Vec<_> = source
            .iter()
            .map(|point| xf.pos(rotate_around(*point, center, 0.0)))
            .collect();
        let tessellation = ink_tessellation_config(&ctx);
        let mut reference_tessellator = egui::epaint::Tessellator::new(
            tessellation.pixels_per_point,
            tessellation.options,
            [0, 0],
            Vec::new(),
        );
        reference_tessellator.set_clip_rect(screen);
        let mut reference = egui::Mesh::default();
        reference_tessellator.tessellate_shape(
            Shape::line(expected_points, Stroke::new(3.0, color)),
            &mut reference,
        );
        assert_eq!(&*first, &reference);

        let clip_rect = Rect::from_min_max(pos2(20.0, 25.0), pos2(26.0, 35.0));
        let clipped = make_mesh(&mut cache, &source, 0.0, xf, 3.0, color, clip_rect);
        assert!(!Arc::ptr_eq(&first, &clipped));

        let zoomed_xf = Xf { zoom: 2.0, ..xf };
        let zoomed = make_mesh(&mut cache, &source, 0.0, zoomed_xf, 6.0, color, screen);
        assert!(!Arc::ptr_eq(&first, &zoomed));

        let rotated = make_mesh(&mut cache, &source, 90.0, zoomed_xf, 6.0, color, screen);
        assert!(!Arc::ptr_eq(&zoomed, &rotated));

        let mut changed = Arc::clone(&source);
        Arc::make_mut(&mut changed)[1].x = 12.0;
        assert!(!Arc::ptr_eq(&source, &changed));
        let updated = make_mesh(&mut cache, &changed, 90.0, zoomed_xf, 6.0, color, screen);
        assert!(!Arc::ptr_eq(&rotated, &updated));
        let recolored = make_mesh(
            &mut cache,
            &changed,
            90.0,
            zoomed_xf,
            6.0,
            Color32::RED,
            screen,
        );
        assert!(!Arc::ptr_eq(&updated, &recolored));
        let resized = make_mesh(
            &mut cache,
            &changed,
            90.0,
            zoomed_xf,
            7.0,
            Color32::RED,
            screen,
        );
        assert!(!Arc::ptr_eq(&recolored, &resized));
        assert_eq!(cache.entries.len(), 1);
        assert_eq!(
            cache.allocated_bytes,
            cache
                .entries
                .get(&object)
                .expect("latest mesh should be cached")
                .allocation_cost()
        );
    }

    #[test]
    fn ink_mesh_cache_preserves_pan_and_highlighter_alpha() {
        use thinoda_core::{Band, InkStroke, InkTool};

        let mut sheet = Sheet::new("S", "Root");
        let ink_color = Color::rgb(190, 45, 80);
        let mut ink = CanvasObject::ink(InkStroke::new(
            InkTool::Highlighter,
            ink_color,
            7.0,
            vec![
                Vec2::new(40.0, 30.0),
                Vec2::new(55.0, 42.0),
                Vec2::new(72.0, 36.0),
            ],
        ));
        ink.id = CanvasObjectId(502);
        ink.band = Band::Above;
        ink.rotation = 27.0;
        sheet.canvas.objects.push(ink);
        let layout = thinoda_core::layout_sheet(&sheet, &thinoda_core::ApproxMeasure, false);
        let ctx = egui::Context::default();
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0));

        let render = |xf| {
            ctx.run_pass(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ctx| {
                    paint_map(
                        &ctx.layer_painter(egui::LayerId::background()),
                        &sheet,
                        &layout,
                        xf,
                        &PaintOptions::default(),
                    );
                },
            )
        };
        let output_mesh = |output: &egui::FullOutput| {
            output
                .shapes
                .iter()
                .find_map(|clipped| match &clipped.shape {
                    Shape::Mesh(mesh) => Some(Arc::clone(mesh)),
                    _ => None,
                })
                .expect("ink mesh should be painted")
        };
        let cached_mesh = |ctx: &egui::Context| {
            ctx.data(|data| {
                data.get_temp::<InkGeometryCache>(ink_cache_id())
                    .expect("ink geometry cache should be stored")
                    .entries
                    .get(&CanvasObjectId(502))
                    .expect("ink entry should be cached")
                    .mesh
                    .clone()
            })
        };

        let first_xf = Xf {
            origin: pos2(200.0, 150.0),
            zoom: 1.0,
        };
        let mut first_output = render(first_xf);
        let first_mesh = output_mesh(&first_output);
        first_output.textures_delta.clear();
        let first_cached = cached_mesh(&ctx);
        assert!(Arc::ptr_eq(&first_mesh, &first_cached));
        let highlighter_color = opacity_color(color32(ink_color), 0.28);
        assert!(
            first_mesh
                .vertices
                .iter()
                .any(|vertex| vertex.color == highlighter_color),
            "highlighter opacity should be baked into the cached mesh"
        );

        let mut reused_output = render(first_xf);
        let reused_mesh = output_mesh(&reused_output);
        reused_output.textures_delta.clear();
        assert!(Arc::ptr_eq(&first_mesh, &reused_mesh));

        let pan = vec2(13.0, -9.0);
        let panned_xf = Xf {
            origin: first_xf.origin + pan,
            zoom: first_xf.zoom,
        };
        let mut panned_output = render(panned_xf);
        let panned_mesh = output_mesh(&panned_output);
        panned_output.textures_delta.clear();
        assert!(!Arc::ptr_eq(&first_cached, &cached_mesh(&ctx)));
        assert_eq!(first_mesh.vertices.len(), panned_mesh.vertices.len());
        for (first, panned) in first_mesh.vertices.iter().zip(&panned_mesh.vertices) {
            assert!((panned.pos - first.pos - pan).length() < 1e-4);
        }

        let zoomed_xf = Xf {
            origin: panned_xf.origin,
            zoom: 2.0,
        };
        let mut zoomed_output = render(zoomed_xf);
        let zoomed_mesh = output_mesh(&zoomed_output);
        zoomed_output.textures_delta.clear();
        assert!(!Arc::ptr_eq(&panned_mesh, &zoomed_mesh));
        assert!(zoomed_mesh.is_valid());

        ctx.set_pixels_per_point(2.0);
        let mut dpi_output = render(zoomed_xf);
        let dpi_mesh = output_mesh(&dpi_output);
        dpi_output.textures_delta.clear();
        assert!(!Arc::ptr_eq(&zoomed_mesh, &dpi_mesh));

        ctx.tessellation_options_mut(|options| options.feathering = !options.feathering);
        let mut options_output = render(zoomed_xf);
        let options_mesh = output_mesh(&options_output);
        options_output.textures_delta.clear();
        assert!(!Arc::ptr_eq(&dpi_mesh, &options_mesh));
    }

    #[test]
    fn ink_mesh_cache_evicts_only_unprotected_entries_within_budgets() {
        fn entry(object: u64) -> InkGeometryEntry {
            let mesh = egui::Mesh {
                vertices: vec![
                    egui::epaint::Vertex::untextured(Pos2::ZERO, Color32::WHITE),
                    egui::epaint::Vertex::untextured(pos2(1.0, 0.0), Color32::WHITE),
                    egui::epaint::Vertex::untextured(pos2(1.0, 1.0), Color32::WHITE),
                ],
                indices: vec![0, 1, 2],
                ..Default::default()
            };
            InkGeometryEntry {
                object: CanvasObjectId(object),
                token: 0,
                protected: true,
                source_points: Arc::from(vec![Vec2::ZERO, Vec2::new(1.0, 0.0)]),
                mesh: Arc::new(mesh),
                center: Vec2::ZERO,
                rotation_bits: 0.0_f32.to_bits(),
                zoom_bits: 1.0_f32.to_bits(),
                origin_bits: [0.0_f32.to_bits(); 2],
                clip_rect: Rect::EVERYTHING,
                stroke_width_bits: 1.0_f32.to_bits(),
                color: Color32::WHITE,
                pixels_per_point_bits: 1.0_f32.to_bits(),
                tessellation_options: egui::epaint::TessellationOptions::default(),
            }
        }

        let mut cache = InkGeometryCache::default();
        let first = entry(1);
        let second = entry(2);
        let third = entry(3);
        let byte_limit = first.allocation_cost() + second.allocation_cost();
        assert!(cache.insert_with_limits(first.clone(), 3, byte_limit));
        assert!(cache.insert_with_limits(second.clone(), 3, byte_limit));
        assert!(!cache.insert_with_limits(third.clone(), 3, byte_limit));
        for _ in 0..3 {
            cache.begin_pass();
            for id in [CanvasObjectId(1), CanvasObjectId(2)] {
                cache.protect_if(id, || true);
            }
            assert!(!cache.insert_with_limits(third.clone(), 3, byte_limit));
            assert!(cache.entries.contains_key(&CanvasObjectId(1)));
            assert!(cache.entries.contains_key(&CanvasObjectId(2)));
            assert_eq!(cache.allocated_bytes, byte_limit);
        }
        cache.begin_pass();
        cache.begin_pass();
        cache.protect_if(CanvasObjectId(2), || true);
        assert!(cache.insert_with_limits(third, 3, byte_limit));
        assert!(!cache.entries.contains_key(&CanvasObjectId(1)));
        assert!(cache.entries.contains_key(&CanvasObjectId(2)));
        assert!(cache.entries.contains_key(&CanvasObjectId(3)));
        assert_eq!(cache.entries.len(), 2);
        assert!(cache.allocated_bytes <= byte_limit);
        assert_eq!(cache.evictable, 0);

        let mut entry_limited = InkGeometryCache::default();
        let entry_limit =
            first.allocation_cost() + second.allocation_cost() + entry(6).allocation_cost();
        assert!(entry_limited.insert_with_limits(first, 2, entry_limit));
        assert!(entry_limited.insert_with_limits(second, 2, entry_limit));
        assert!(!entry_limited.insert_with_limits(entry(6), 2, entry_limit));
        entry_limited.begin_pass();
        entry_limited.protect_if(CanvasObjectId(2), || true);
        assert!(entry_limited.insert_with_limits(entry(6), 2, entry_limit));
        assert!(!entry_limited.entries.contains_key(&CanvasObjectId(1)));
        assert!(entry_limited.entries.contains_key(&CanvasObjectId(2)));
        assert_eq!(entry_limited.entries.len(), 2);

        for _ in 0..4 {
            assert!(entry_limited.insert_with_limits(entry(2), 2, entry_limit));
        }
        entry_limited.begin_pass();
        entry_limited.protect_if(CanvasObjectId(6), || true);
        assert!(entry_limited.insert_with_limits(entry(7), 2, entry_limit));
        assert!(!entry_limited.entries.contains_key(&CanvasObjectId(2)));
        assert!(entry_limited.entries.contains_key(&CanvasObjectId(6)));
        assert!(entry_limited.entries.contains_key(&CanvasObjectId(7)));
        assert_eq!(entry_limited.evictable, 0);

        let mut oversized = entry(3);
        oversized.source_points = Arc::from(vec![Vec2::ZERO; MAX_CACHED_STROKE_POINTS + 1]);
        assert!(!cache.insert_with_limits(oversized, 3, byte_limit));
        assert!(!cache.entries.contains_key(&CanvasObjectId(3)));
        assert_eq!(cache.allocated_bytes, entry(2).allocation_cost());

        let too_small = entry(5);
        let too_small_limit = too_small.allocation_cost() - 1;
        assert!(!cache.insert_with_limits(too_small, 3, too_small_limit));
        assert!(!cache.entries.contains_key(&CanvasObjectId(5)));
        assert!(cache.allocated_bytes <= byte_limit);

        let protected = entry(10);
        let offscreen = entry(11);
        let mut too_large_for_protected = entry(12);
        too_large_for_protected.mesh = Arc::new(egui::Mesh {
            vertices: vec![egui::epaint::Vertex::default(); 10],
            indices: vec![0, 1, 2],
            ..Default::default()
        });
        let admission_limit = protected.allocation_cost() + offscreen.allocation_cost();
        assert!(too_large_for_protected.allocation_cost() <= admission_limit);
        assert!(
            protected.allocation_cost() + too_large_for_protected.allocation_cost()
                > admission_limit
        );
        let mut partial = InkGeometryCache::default();
        assert!(partial.insert_with_limits(protected, 3, admission_limit));
        assert!(partial.insert_with_limits(offscreen, 3, admission_limit));
        partial.begin_pass();
        partial.protect_if(CanvasObjectId(10), || true);
        partial.protect_if(CanvasObjectId(10), || panic!("already protected"));
        partial.protect_if(CanvasObjectId(11), || false);
        assert_eq!(partial.protected_bytes, entry(10).allocation_cost());
        assert_eq!(partial.evictable, 1);
        assert!(!partial.insert_with_limits(too_large_for_protected, 3, admission_limit));
        assert!(partial.entries.contains_key(&CanvasObjectId(10)));
        assert!(partial.entries.contains_key(&CanvasObjectId(11)));
        assert_eq!(partial.allocated_bytes, admission_limit);
    }

    #[test]
    fn ink_mesh_cache_protects_visible_meshes_across_bands_at_capacity() {
        use thinoda_core::{Band, InkStroke, InkTool};

        let mut sheet = Sheet::new("S", "Root");
        for index in 0..MAX_INK_CACHE_ENTRIES + 4 {
            let mut ink = CanvasObject::ink(InkStroke::new(
                InkTool::Pen,
                Color::rgb(20, 40, 200),
                2.0,
                vec![Vec2::new(10.0, 10.0), Vec2::new(30.0, 20.0)],
            ));
            ink.id = CanvasObjectId(index as u64 + 100);
            ink.band = if index % 2 == 0 {
                Band::Below
            } else {
                Band::Above
            };
            sheet.canvas.objects.push(ink);
        }
        let layout = thinoda_core::layout_sheet(&sheet, &thinoda_core::ApproxMeasure, false);
        let ctx = egui::Context::default();
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0));
        let render = |sheet: &Sheet, layout: &Layout| {
            let mut output = ctx.run_pass(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ctx| {
                    paint_map(
                        &ctx.layer_painter(egui::LayerId::background()),
                        sheet,
                        layout,
                        Xf {
                            origin: pos2(100.0, 100.0),
                            zoom: 1.0,
                        },
                        &PaintOptions::default(),
                    );
                },
            );
            output.textures_delta.clear();
            ctx.data(|data| {
                data.get_temp::<InkGeometryCache>(ink_cache_id())
                    .unwrap()
                    .entries
                    .into_iter()
                    .map(|(id, entry)| (id, entry.mesh))
                    .collect::<HashMap<_, _>>()
            })
        };
        let first = render(&sheet, &layout);
        assert_eq!(first.len(), MAX_INK_CACHE_ENTRIES);
        assert!(
            sheet
                .canvas
                .objects
                .iter()
                .any(|object| { object.band == Band::Below && first.contains_key(&object.id) })
        );
        assert!(
            sheet
                .canvas
                .objects
                .iter()
                .any(|object| { object.band == Band::Above && first.contains_key(&object.id) })
        );
        let second = render(&sheet, &layout);
        assert_eq!(first.len(), second.len());
        for (id, mesh) in &first {
            assert!(
                Arc::ptr_eq(mesh, &second[id]),
                "visible mesh {id:?} was evicted"
            );
        }

        let empty = Sheet::new("Other", "Root");
        let empty_layout = thinoda_core::layout_sheet(&empty, &thinoda_core::ApproxMeasure, false);
        let after_empty = render(&empty, &empty_layout);
        for (id, mesh) in &first {
            assert!(Arc::ptr_eq(mesh, &after_empty[id]));
        }
        let restored = render(&sheet, &layout);
        for (id, mesh) in &first {
            assert!(Arc::ptr_eq(mesh, &restored[id]));
        }

        let mut other = Sheet::new("Other", "Root");
        for index in 0..4 {
            let mut ink = CanvasObject::ink(InkStroke::new(
                InkTool::Highlighter,
                Color::rgb(200, 30, 50),
                3.0,
                vec![Vec2::new(20.0, 30.0), Vec2::new(40.0, 40.0)],
            ));
            ink.id = CanvasObjectId(MAX_INK_CACHE_ENTRIES as u64 + 200 + index);
            other.canvas.objects.push(ink);
        }
        let other_layout = thinoda_core::layout_sheet(&other, &thinoda_core::ApproxMeasure, false);
        let switched = render(&other, &other_layout);
        assert_eq!(switched.len(), MAX_INK_CACHE_ENTRIES);
        for object in &other.canvas.objects {
            assert!(switched.contains_key(&object.id));
        }
        let returned = render(&sheet, &layout);
        for (id, mesh) in &first {
            if let Some(retained) = switched.get(id) {
                assert!(Arc::ptr_eq(mesh, retained));
                assert!(Arc::ptr_eq(mesh, &returned[id]));
            }
        }
        for object in &other.canvas.objects {
            assert!(!returned.contains_key(&object.id));
        }

        let reused_id = *returned.keys().next().unwrap();
        let mut reused = Sheet::new("Reused ID", "Root");
        let mut ink = CanvasObject::ink(InkStroke::new(
            InkTool::Pen,
            Color::rgb(200, 30, 50),
            4.0,
            vec![Vec2::new(40.0, 40.0), Vec2::new(60.0, 50.0)],
        ));
        ink.id = reused_id;
        reused.canvas.objects.push(ink);
        let reused_layout =
            thinoda_core::layout_sheet(&reused, &thinoda_core::ApproxMeasure, false);
        let replaced = render(&reused, &reused_layout);
        assert!(!Arc::ptr_eq(&returned[&reused_id], &replaced[&reused_id]));
        assert!(
            replaced[&reused_id]
                .vertices
                .iter()
                .any(|vertex| vertex.color == color32(Color::rgb(200, 30, 50)))
        );
    }

    #[test]
    fn board_object_culling_keeps_visible_and_edge_geometry_only() {
        use thinoda_core::{Band, CanvasConnector, Endpoint, InkStroke, InkTool, Zone, ZoneId};

        let mut sheet = Sheet::new("S", "Root");
        let mut visible = CanvasObject::shape(
            ShapeKind::Rectangle,
            thinoda_core::Rect::from_center_size(Vec2::ZERO, Vec2::new(20.0, 20.0)),
        );
        visible.id = CanvasObjectId(601);
        visible.band = Band::Above;
        visible.style.fill = Some(Color::rgb(10, 200, 30));

        let mut edge = CanvasObject::shape(
            ShapeKind::Rectangle,
            thinoda_core::Rect::from_center_size(Vec2::new(105.0, 0.0), Vec2::new(20.0, 20.0)),
        );
        edge.id = CanvasObjectId(602);
        edge.band = Band::Above;
        edge.style.fill = Some(Color::rgb(20, 70, 220));

        let mut offscreen = CanvasObject::shape(
            ShapeKind::Rectangle,
            thinoda_core::Rect::from_center_size(Vec2::new(200.0, 0.0), Vec2::new(20.0, 20.0)),
        );
        offscreen.id = CanvasObjectId(603);
        offscreen.band = Band::Above;
        offscreen.style.fill = Some(Color::rgb(230, 20, 200));

        let mut frame_clipped = CanvasObject::shape(
            ShapeKind::Rectangle,
            thinoda_core::Rect::from_center_size(Vec2::new(145.0, 0.0), Vec2::new(100.0, 40.0)),
        );
        frame_clipped.id = CanvasObjectId(604);
        frame_clipped.band = Band::Above;
        frame_clipped.style.fill = Some(Color::rgb(220, 130, 10));

        let mut visible_ink = CanvasObject::ink(InkStroke::new(
            InkTool::Pen,
            Color::rgb(200, 10, 20),
            4.0,
            vec![Vec2::new(-20.0, 30.0), Vec2::new(20.0, 30.0)],
        ));
        visible_ink.id = CanvasObjectId(605);
        visible_ink.band = Band::Above;

        let mut edge_ink = CanvasObject::ink(InkStroke::new(
            InkTool::Pen,
            Color::rgb(10, 180, 200),
            4.0,
            vec![Vec2::new(95.0, 50.0), Vec2::new(115.0, 50.0)],
        ));
        edge_ink.id = CanvasObjectId(606);
        edge_ink.band = Band::Above;

        let mut offscreen_ink = CanvasObject::ink(InkStroke::new(
            InkTool::Pen,
            Color::rgb(160, 30, 40),
            4.0,
            vec![Vec2::new(200.0, 60.0), Vec2::new(220.0, 60.0)],
        ));
        offscreen_ink.id = CanvasObjectId(607);
        offscreen_ink.band = Band::Above;

        let mut clipped_ink = CanvasObject::ink(InkStroke::new(
            InkTool::Pen,
            Color::rgb(90, 40, 210),
            4.0,
            vec![Vec2::new(90.0, 80.0), Vec2::new(200.0, 80.0)],
        ));
        clipped_ink.id = CanvasObjectId(608);
        clipped_ink.band = Band::Above;

        let mut visible_connector = CanvasObject::connector(CanvasConnector::new(
            Endpoint::Point(Vec2::new(-30.0, 100.0)),
            Endpoint::Point(Vec2::new(30.0, 100.0)),
        ));
        visible_connector.id = CanvasObjectId(609);
        visible_connector.band = Band::Above;
        if let CanvasObjectKind::Connector(connector) = &mut visible_connector.kind {
            connector.color = Some(Color::rgb(40, 140, 180));
        }

        let mut edge_connector = CanvasObject::connector(CanvasConnector::new(
            Endpoint::Point(Vec2::new(-110.0, 20.0)),
            Endpoint::Point(Vec2::new(-80.0, 20.0)),
        ));
        edge_connector.id = CanvasObjectId(610);
        edge_connector.band = Band::Above;
        if let CanvasObjectKind::Connector(connector) = &mut edge_connector.kind {
            connector.color = Some(Color::rgb(120, 80, 190));
        }

        let mut offscreen_connector = CanvasObject::connector(CanvasConnector::new(
            Endpoint::Point(Vec2::new(200.0, 140.0)),
            Endpoint::Point(Vec2::new(220.0, 140.0)),
        ));
        offscreen_connector.id = CanvasObjectId(611);
        offscreen_connector.band = Band::Above;
        if let CanvasObjectKind::Connector(connector) = &mut offscreen_connector.kind {
            connector.color = Some(Color::rgb(140, 110, 30));
        }

        let mut clipped_connector = CanvasObject::connector(CanvasConnector::new(
            Endpoint::Point(Vec2::new(90.0, 90.0)),
            Endpoint::Point(Vec2::new(200.0, 90.0)),
        ));
        clipped_connector.id = CanvasObjectId(612);
        clipped_connector.band = Band::Above;
        if let CanvasObjectKind::Connector(connector) = &mut clipped_connector.kind {
            connector.color = Some(Color::rgb(110, 170, 45));
        }

        sheet.canvas.objects.extend([
            visible,
            edge,
            offscreen,
            frame_clipped,
            visible_ink,
            edge_ink,
            offscreen_ink,
            clipped_ink,
            visible_connector,
            edge_connector,
            offscreen_connector,
            clipped_connector,
        ]);
        sheet.zones.push(Zone {
            id: ZoneId(1),
            name: "Offscreen clip".into(),
            frame: thinoda_core::Rect::from_center_size(
                Vec2::new(145.0, 40.0),
                Vec2::new(30.0, 200.0),
            ),
            roots: vec![],
            objects: vec![],
            keep_empty: true,
            color: None,
            background_color: None,
            clip: true,
        });
        let layout = thinoda_core::layout_sheet(&sheet, &thinoda_core::ApproxMeasure, false);
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(200.0, 200.0));
        let ctx = egui::Context::default();
        let mut output = ctx.run_pass(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ctx| {
                paint_map(
                    &ctx.layer_painter(egui::LayerId::background()),
                    &sheet,
                    &layout,
                    Xf {
                        origin: pos2(100.0, 100.0),
                        zoom: 1.0,
                    },
                    &PaintOptions::default(),
                );
            },
        );
        output.textures_delta.clear();
        let has_fill = |color| {
            output.shapes.iter().any(|shape| match &shape.shape {
                Shape::Path(path) => path.fill == color32(color),
                _ => false,
            })
        };
        let has_stroke = |color| {
            output.shapes.iter().any(|shape| match &shape.shape {
                Shape::LineSegment { stroke, .. } => stroke.color == color32(color),
                Shape::Path(path) => matches!(
                    path.stroke.color,
                    egui::epaint::ColorMode::Solid(actual) if actual == color32(color)
                ),
                Shape::Mesh(mesh) => mesh
                    .vertices
                    .iter()
                    .any(|vertex| vertex.color == color32(color)),
                _ => false,
            })
        };
        assert!(has_fill(Color::rgb(10, 200, 30)));
        assert!(
            has_fill(Color::rgb(20, 70, 220)),
            "partially visible edge shape"
        );
        assert!(!has_fill(Color::rgb(230, 20, 200)), "offscreen shape");
        assert!(!has_fill(Color::rgb(220, 130, 10)), "frame-clipped shape");
        assert!(has_stroke(Color::rgb(200, 10, 20)));
        assert!(
            has_stroke(Color::rgb(10, 180, 200)),
            "partially visible edge ink"
        );
        assert!(!has_stroke(Color::rgb(160, 30, 40)), "offscreen ink");
        assert!(!has_stroke(Color::rgb(90, 40, 210)), "frame-clipped ink");
        assert!(has_stroke(Color::rgb(40, 140, 180)));
        assert!(
            has_stroke(Color::rgb(120, 80, 190)),
            "partially visible edge connector"
        );
        assert!(!has_stroke(Color::rgb(140, 110, 30)), "offscreen connector");
        assert!(
            !has_stroke(Color::rgb(110, 170, 45)),
            "frame-clipped connector"
        );
    }

    #[test]
    #[ignore = "manual native paint_map CPU measurement"]
    fn measure_ink_render_cpu_for_1000_and_10000_strokes() {
        use std::time::Instant;
        use thinoda_core::{Band, InkStroke, InkTool};

        for stroke_count in [1_000, 10_000] {
            let mut sheet = Sheet::new("Benchmark", "Root");
            for index in 0..stroke_count {
                let column = (index % 100) as f32;
                let row = (index / 100) as f32;
                let x = -350.0 + column * 7.0;
                let y = -250.0 + row * 5.0;
                let points = (0..16)
                    .map(|point| {
                        Vec2::new(x + point as f32, y + if point % 2 == 0 { 0.0 } else { 2.0 })
                    })
                    .collect();
                let mut stroke = CanvasObject::ink(InkStroke::new(
                    InkTool::Pen,
                    Color::rgb(40, 100, 220),
                    2.0,
                    points,
                ));
                stroke.id = CanvasObjectId(index as u64 + 1);
                stroke.band = Band::Above;
                sheet.canvas.objects.push(stroke);
            }
            let layout = thinoda_core::layout_sheet(&sheet, &thinoda_core::ApproxMeasure, false);
            let ctx = egui::Context::default();
            let screen = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0));
            let mut paint_samples = Vec::new();
            let mut tessellation_samples = Vec::new();
            for _ in 0..4 {
                let start = Instant::now();
                let mut output = ctx.run_pass(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        ..Default::default()
                    },
                    |ctx| {
                        paint_map(
                            &ctx.layer_painter(egui::LayerId::background()),
                            &sheet,
                            &layout,
                            Xf {
                                origin: pos2(400.0, 300.0),
                                zoom: 1.0,
                            },
                            &PaintOptions::default(),
                        );
                    },
                );
                paint_samples.push(start.elapsed());
                let tessellation_start = Instant::now();
                let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
                tessellation_samples.push(tessellation_start.elapsed());
                std::hint::black_box(primitives);
                output.textures_delta.clear();
            }
            eprintln!(
                "native strokes={stroke_count}: paint cold={:?}, paint warm={:?}; tessellation cold={:?}, tessellation warm={:?}",
                paint_samples[0],
                &paint_samples[1..],
                tessellation_samples[0],
                &tessellation_samples[1..]
            );
        }
    }

    #[test]
    fn shape_triangulations_cover_their_outlines_without_filling_notches() {
        fn cross(a: Vec2, b: Vec2, c: Vec2) -> f64 {
            f64::from(b.x - a.x) * f64::from(c.y - a.y)
                - f64::from(b.y - a.y) * f64::from(c.x - a.x)
        }
        fn in_triangle(p: Vec2, a: Vec2, b: Vec2, c: Vec2) -> bool {
            let sides = [cross(a, b, p), cross(b, c, p), cross(c, a, p)];
            sides.iter().all(|side| *side >= -1e-7) || sides.iter().all(|side| *side <= 1e-7)
        }

        assert!(!shape_fill(ShapeKind::Star).convex);
        assert!(!shape_fill(ShapeKind::Chevron).convex);
        for kind in ShapeKind::ALL {
            let outline = kind.unit_outline();
            let triangles = &shape_fill(kind).triangles;
            assert!(!triangles.is_empty(), "{kind:?}");
            assert_eq!(triangles.len() % 3, 0, "{kind:?}");
            let polygon_area = (0..outline.len())
                .map(|i| {
                    let a = outline[i];
                    let b = outline[(i + 1) % outline.len()];
                    f64::from(a.x) * f64::from(b.y) - f64::from(a.y) * f64::from(b.x)
                })
                .sum::<f64>()
                .abs()
                / 2.0;
            let triangle_area = triangles
                .as_chunks::<3>()
                .0
                .iter()
                .map(|triangle| {
                    let [a, b, c] = [
                        outline[triangle[0] as usize],
                        outline[triangle[1] as usize],
                        outline[triangle[2] as usize],
                    ];
                    cross(a, b, c).abs() / 2.0
                })
                .sum::<f64>();
            assert!(
                (triangle_area - polygon_area).abs() < 1e-4,
                "{kind:?}: triangles {triangle_area}, polygon {polygon_area}"
            );
            for y in 1..20 {
                for x in 1..20 {
                    let point = Vec2::new(x as f32 / 20.0, y as f32 / 20.0);
                    if thinoda_core::board::distance_to_outline(point, &outline) < 1e-4 {
                        continue;
                    }
                    let expected = thinoda_core::board::point_in_polygon(point, &outline);
                    let covered = triangles.as_chunks::<3>().0.iter().any(|triangle| {
                        in_triangle(
                            point,
                            outline[triangle[0] as usize],
                            outline[triangle[1] as usize],
                            outline[triangle[2] as usize],
                        )
                    });
                    assert_eq!(covered, expected, "{kind:?} at {point:?}");
                }
            }
        }
    }

    #[test]
    fn image_texture_is_painted_once_cached_and_alpha_is_preserved() {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            40,
            20,
            image::Rgba([255, 0, 0, 128]),
        ))
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
        let resource =
            thinoda_core::resources::Resource::new("evidence.png", "image/png", bytes.into_inner())
                .unwrap();
        let mut sheet = Sheet::new("Image", "Topic");
        let root = sheet.root;
        thinoda_core::images::set_image(&mut sheet, root, Some((resource, Vec2::new(160.0, 80.0))))
            .unwrap();
        for dark in [false, true] {
            let ctx = egui::Context::default();
            ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            let layout = thinoda_core::layout_sheet(&sheet, &thinoda_core::ApproxMeasure, dark);
            let xf = Xf {
                origin: pos2(300.0, 300.0),
                zoom: 1.0,
            };
            for frame in 0..3 {
                let mut output = ctx.run_pass(egui::RawInput::default(), |ctx| {
                    paint_map(
                        &ctx.layer_painter(egui::LayerId::background()),
                        &sheet,
                        &layout,
                        xf,
                        &PaintOptions {
                            dark,
                            ..Default::default()
                        },
                    );
                });
                let cache = ctx
                    .data(|d| d.get_temp::<ImageCache>(egui::Id::unique("local-image-cache")))
                    .unwrap();
                assert_eq!(cache.0.len(), 1);
                let texture = cache.0[0].1.as_ref().unwrap();
                assert!(output.shapes.iter().any(
                    |s| matches!(&s.shape, Shape::Mesh(mesh) if mesh.texture_id == texture.id())
                ));
                if frame == 0 {
                    assert!(output.textures_delta.set.iter().any(|(id, delta)| *id
                        == texture.id()
                        && delta.iter().any(|delta| match &delta.image {
                            egui::ImageData::Color(image) => image.pixels[0].a() == 128,
                        })));
                } else {
                    assert!(
                        !output
                            .textures_delta
                            .set
                            .iter()
                            .any(|(id, _)| *id == texture.id())
                    );
                }
                output.textures_delta.clear();
            }
        }
    }

    #[test]
    fn failed_image_decode_is_cached_and_entry_count_is_bounded() {
        let ctx = egui::Context::default();
        let sheet = Sheet::new("Bad image", "Topic");
        let mut output = ctx.run_pass(egui::RawInput::default(), |ctx| {
            let painter = ctx.layer_painter(egui::LayerId::background());
            for index in 0..140 {
                assert!(!paint_image(
                    &painter,
                    &sheet,
                    &format!("legacy-{index}"),
                    Rect::from_min_size(Pos2::ZERO, vec2(100.0, 100.0)),
                    false
                ));
            }
        });
        let cache = ctx
            .data(|d| d.get_temp::<ImageCache>(egui::Id::unique("local-image-cache")))
            .unwrap();
        assert_eq!(cache.0.len(), 128);
        assert!(cache.0.iter().all(|(_, texture)| texture.is_none()));
        output.textures_delta.clear();
    }

    #[test]
    fn selected_summary_paints_descendant_range_frame_and_topic_connector() {
        for (structure, dir) in [
            (thinoda_core::Structure::LogicRight, Dir::Right),
            (thinoda_core::Structure::LogicLeft, Dir::Left),
            (thinoda_core::Structure::OrgDown, Dir::Down),
            (thinoda_core::Structure::OrgUp, Dir::Up),
        ] {
            let mut sheet = Sheet::new("S", "Root");
            sheet.structure = structure;
            let a = sheet.add_child(sheet.root, None, "A");
            let b = sheet.add_child(sheet.root, None, "B");
            let child = sheet.add_child(a, None, "Descendant");
            let summary = sheet.add_summary(&[a, b], "Conclusion").unwrap();
            let layout = thinoda_core::layout_sheet(&sheet, &thinoda_core::ApproxMeasure, false);
            let sl = &layout.summaries[0];
            assert!(sl.range_rect.contains(layout.rect(child).unwrap().center()));
            let topic = layout.rect(summary).unwrap();
            let end = match dir {
                Dir::Right => topic.left_center(),
                Dir::Left => topic.right_center(),
                Dir::Down => topic.top_center(),
                Dir::Up => topic.bottom_center(),
            };
            for zoom in [0.25, 1.0, 2.0] {
                let xf = Xf {
                    origin: pos2(400.0, 300.0),
                    zoom,
                };
                let accent = Color32::from_rgb(255, 0, 200);
                let paint = |selection: &[TopicId]| {
                    let ctx = egui::Context::default();
                    let mut output = ctx.run_pass(egui::RawInput::default(), |ctx| {
                        let painter = ctx.layer_painter(egui::LayerId::background());
                        paint_map(
                            &painter,
                            &sheet,
                            &layout,
                            xf,
                            &PaintOptions {
                                selection,
                                accent,
                                ..Default::default()
                            },
                        );
                    });
                    // This test inspects paint commands without a GPU backend.
                    output.textures_delta.clear();
                    output.shapes
                };
                let selected = paint(&[summary]);
                assert!(selected.iter().any(|s| matches!(&s.shape,
                    Shape::LineSegment { points, stroke }
                        if *points == [xf.pos(sl.tip), xf.pos(end)] && stroke.color == accent
                )));
                let frame = xf.rect(sl.range_rect.expand(4.0));
                let is_frame = |s: &egui::epaint::ClippedShape| match &s.shape {
                    Shape::LineSegment { points, stroke } => {
                        stroke.color == accent
                            && stroke.width == 1.0
                            && points.iter().all(|p| {
                                (p.x == frame.left()
                                    || p.x == frame.right()
                                    || p.y == frame.top()
                                    || p.y == frame.bottom())
                                    && frame.expand(0.01).contains(*p)
                            })
                    }
                    _ => false,
                };
                assert!(selected.iter().any(is_frame), "{dir:?} zoom {zoom}");
                assert!(!paint(&[]).iter().any(is_frame));
            }
        }
    }

    #[test]
    fn map_decorations_use_their_owning_topic_frame_clips() {
        use thinoda_core::{Zone, ZoneId};

        let mut sheet = Sheet::new("S", "Root");
        let root = sheet.root;
        let a = sheet.add_child(root, None, "A");
        let aa = sheet.add_child(a, None, "AA");
        let b = sheet.add_child(root, None, "B");
        let boundary = sheet.add_boundary(&[root]).unwrap();
        let summary_topic = sheet.add_summary(&[a, b], "Conclusion").unwrap();
        let summary_id = sheet.summaries[0].id;
        let same_frame = sheet.add_relationship(a, aa).unwrap();
        let cross_frame = sheet.add_relationship(a, b).unwrap();
        let mut layout = thinoda_core::layout_sheet(&sheet, &thinoda_core::ApproxMeasure, false);

        let root_clip = layout.bounds.expand(20.0);
        let a_clip = layout.rect(a).unwrap().expand(20.0);
        let b_clip = layout.rect(b).unwrap().expand(20.0);
        let summary_clip = layout.rect(summary_topic).unwrap().expand(20.0);
        sheet.zones = vec![
            Zone {
                id: ZoneId(1),
                name: "Root clip".into(),
                frame: root_clip,
                roots: vec![root],
                objects: vec![],
                keep_empty: false,
                color: None,
                background_color: None,
                clip: true,
            },
            Zone {
                id: ZoneId(2),
                name: "A clip".into(),
                frame: a_clip,
                roots: vec![a],
                objects: vec![],
                keep_empty: false,
                color: None,
                background_color: None,
                clip: true,
            },
            Zone {
                id: ZoneId(3),
                name: "B clip".into(),
                frame: b_clip,
                roots: vec![b],
                objects: vec![],
                keep_empty: false,
                color: None,
                background_color: None,
                clip: true,
            },
            Zone {
                id: ZoneId(4),
                name: "Summary clip".into(),
                frame: summary_clip,
                roots: vec![summary_topic],
                objects: vec![],
                keep_empty: false,
                color: None,
                background_color: None,
                clip: true,
            },
        ];

        let boundary_color = Color::rgb(235, 25, 30);
        sheet
            .boundaries
            .iter_mut()
            .find(|item| item.id == boundary)
            .unwrap()
            .color = Some(boundary_color);
        let branch_color = Color::rgb(15, 195, 55);
        layout
            .connectors
            .iter_mut()
            .find(|connector| connector.child == aa)
            .unwrap()
            .color = branch_color;
        let same_color = Color::rgb(25, 60, 235);
        let cross_color = Color::rgb(235, 130, 20);
        sheet
            .relationships
            .iter_mut()
            .find(|item| item.id == same_frame)
            .unwrap()
            .color = Some(same_color);
        sheet
            .relationships
            .iter_mut()
            .find(|item| item.id == cross_frame)
            .unwrap()
            .color = Some(cross_color);
        let accent = Color32::from_rgb(180, 30, 200);
        let xf = Xf {
            origin: pos2(
                500.0 - layout.bounds.center().x,
                500.0 - layout.bounds.center().y,
            ),
            zoom: 1.0,
        };
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 1000.0));
        let ctx = egui::Context::default();
        let mut output = ctx.run_pass(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ctx| {
                paint_map(
                    &ctx.layer_painter(egui::LayerId::background()),
                    &sheet,
                    &layout,
                    xf,
                    &PaintOptions {
                        selected_object: Some(ObjectSelection::Summary(summary_id)),
                        accent,
                        ..Default::default()
                    },
                );
            },
        );

        fn stroke_color(shape: &egui::epaint::ClippedShape) -> Option<Color32> {
            match &shape.shape {
                Shape::LineSegment { stroke, .. } => Some(stroke.color),
                Shape::Path(path) => match path.stroke.color {
                    egui::epaint::ColorMode::Solid(color) => Some(color),
                    egui::epaint::ColorMode::UV(_) => None,
                },
                Shape::Mesh(mesh) => mesh
                    .vertices
                    .iter()
                    .map(|vertex| vertex.color)
                    .max_by_key(Color32::a),
                _ => None,
            }
        }
        let assert_clip = |color: Color32, clip: Rect| {
            let matching = output
                .shapes
                .iter()
                .filter(|shape| stroke_color(shape) == Some(color))
                .collect::<Vec<_>>();
            assert!(!matching.is_empty(), "no stroke painted with {color:?}");
            assert!(
                matching.iter().all(|shape| shape.clip_rect == clip),
                "stroke {color:?} did not use expected clip {clip:?}"
            );
        };
        assert_clip(color32(boundary_color), xf.rect(root_clip));
        assert_clip(color32(branch_color), xf.rect(a_clip));
        assert_clip(accent, xf.rect(summary_clip));
        assert_clip(color32(same_color), xf.rect(a_clip));
        assert_clip(color32(cross_color), screen);
        output.textures_delta.clear();
    }

    #[test]
    fn relationships_between_identically_framed_zones_are_not_clipped_together() {
        let mut sheet = Sheet::new("S", "Root");
        let from = sheet.add_floating("From", Vec2::new(0.0, 0.0));
        let to = sheet.add_floating("To", Vec2::new(100.0, 0.0));
        let frame =
            thinoda_core::Rect::from_min_size(Vec2::new(-100.0, -100.0), Vec2::new(300.0, 200.0));
        let from_zone = sheet.create_zone("From frame", vec![from], frame).unwrap();
        let to_zone = sheet.create_zone("To frame", vec![to], frame).unwrap();
        for zone in &mut sheet.zones {
            zone.clip = true;
        }

        assert_ne!(from_zone, to_zone);
        assert_eq!(topic_clip_zone(&sheet, from), Some((from_zone, frame)));
        assert_eq!(topic_clip_zone(&sheet, to), Some((to_zone, frame)));
        assert_eq!(
            thinoda_core::scene::topic_clip_rect(&sheet, from),
            Some(frame)
        );
        assert_eq!(relationship_clip_rect(&sheet, from, to), None);
    }

    #[test]
    fn relationships_within_a_shared_zone_keep_the_clip() {
        let mut sheet = Sheet::new("S", "Root");
        let from = sheet.add_floating("From", Vec2::new(0.0, 0.0));
        let to = sheet.add_floating("To", Vec2::new(100.0, 0.0));
        let frame =
            thinoda_core::Rect::from_min_size(Vec2::new(-100.0, -100.0), Vec2::new(300.0, 200.0));
        let shared_zone = sheet
            .create_zone("Shared frame", vec![from, to], frame)
            .unwrap();
        sheet.zones[0].clip = true;

        assert_eq!(topic_clip_zone(&sheet, from), Some((shared_zone, frame)));
        assert_eq!(topic_clip_zone(&sheet, to), Some((shared_zone, frame)));
        assert_eq!(relationship_clip_rect(&sheet, from, to), Some(frame));
    }

    #[test]
    fn xf_roundtrip() {
        let xf = Xf {
            origin: pos2(100.0, 50.0),
            zoom: 2.0,
        };
        let v = Vec2::new(10.0, -5.0);
        let p = xf.pos(v);
        assert_eq!(p, pos2(120.0, 40.0));
        assert_eq!(xf.sheet(p), v);
    }

    #[test]
    fn connector_paths_hit_endpoints() {
        let s = Vec2::new(0.0, 0.0);
        let e = Vec2::new(100.0, 40.0);
        for k in [
            ConnectorKind::Curve,
            ConnectorKind::Straight,
            ConnectorKind::Elbow,
            ConnectorKind::RoundedElbow,
            ConnectorKind::Brace,
            ConnectorKind::Tree,
        ] {
            for v in [false, true] {
                let p = connector_path(k, v, s, e);
                assert_eq!(p.first().copied(), Some(s), "{k:?}");
                let last = *p.last().unwrap();
                assert!((last - e).length() < 0.01, "{k:?} {last:?}");
            }
        }
    }

    #[test]
    fn brace_ends() {
        let p = brace_path(
            Vec2::new(0.0, 0.0),
            Vec2::new(0.0, 100.0),
            Vec2::new(20.0, 50.0),
            Dir::Right,
        );
        assert_eq!(p[0], Vec2::new(0.0, 0.0));
        assert!((*p.last().unwrap() - Vec2::new(0.0, 100.0)).length() < 0.01);
        assert!(p.iter().any(|q| (q.x - 20.0).abs() < 0.01));
    }

    #[test]
    fn blank_central_and_hidden_sheets_have_expected_scene_topics() {
        let central = Sheet::new("Central", "Anchor");
        let layout = thinoda_core::layout_sheet(&central, &thinoda_core::ApproxMeasure, false);
        let scene = build_scene(&central, &layout);
        assert_eq!(
            scene
                .items
                .iter()
                .filter(|item| matches!(item.id, SceneId::Topic(_)))
                .count(),
            1
        );

        let mut hidden = Sheet::new("Board", "");
        hidden.root_mode = thinoda_core::SheetRootMode::Hidden;
        let card = hidden.add_floating("Card", Vec2::new(120.0, 80.0));
        hidden.topic_mut(card).unwrap().card = Some(thinoda_core::Card {
            kind: CardKind::Sticky,
            size: Vec2::new(140.0, 100.0),
            rotation: 17.0,
            color: Some(Color::rgb(240, 200, 80)),
        });
        let layout = thinoda_core::layout_sheet(&hidden, &thinoda_core::ApproxMeasure, false);
        let scene = build_scene(&hidden, &layout);
        assert!(!layout.topics.contains_key(&hidden.root));
        assert!(scene.hit_test(Vec2::ZERO) != Some(thinoda_core::SceneHit::Topic(hidden.root)));
        assert_eq!(
            scene
                .items
                .iter()
                .filter(|item| matches!(item.id, SceneId::Topic(_)))
                .count(),
            1
        );

        let ctx = egui::Context::default();
        let mut output = ctx.run_pass(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
                ..Default::default()
            },
            |ctx| {
                paint_map(
                    &ctx.layer_painter(egui::LayerId::background()),
                    &hidden,
                    &layout,
                    Xf {
                        origin: pos2(300.0, 250.0),
                        zoom: 1.0,
                    },
                    &PaintOptions::default(),
                );
            },
        );
        output.textures_delta.clear();
        assert!(output.shapes.iter().any(|shape| matches!(
            &shape.shape,
            Shape::Path(path) if path.fill == color32(Color::rgb(240, 200, 80))
        )));
    }

    #[test]
    fn connector_markers_paint_the_shared_scene_geometry() {
        use thinoda_core::{
            CanvasConnector, ConnectorMarker, ConnectorMarkerKind, Endpoint, SheetRootMode,
        };

        let mut sheet = Sheet::new("Markers", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.arrow_start = true;
        connector.start_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Block,
        });
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Open,
        });
        connector.color = Some(Color::rgb(20, 40, 60));
        let id = sheet
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let layout = thinoda_core::layout_sheet(&sheet, &thinoda_core::ApproxMeasure, false);
        let scene = build_scene(&sheet, &layout);
        let SceneGeometry::Polyline {
            points, markers, ..
        } = &scene.item(SceneId::Object(id)).unwrap().geometry
        else {
            panic!("expected connector");
        };
        let xf = Xf {
            origin: pos2(100.0, 100.0),
            zoom: 2.0,
        };
        let ctx = egui::Context::default();
        let mut output = ctx.run_pass(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(500.0, 400.0))),
                ..Default::default()
            },
            |ctx| {
                paint_map(
                    &ctx.layer_painter(egui::LayerId::background()),
                    &sheet,
                    &layout,
                    xf,
                    &PaintOptions::default(),
                );
            },
        );
        output.textures_delta.clear();
        for marker in markers.iter().flatten() {
            let expected = marker.points.map(|point| xf.pos(point));
            let path = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    Shape::Path(path) if path.points.as_slice() == expected => Some(path),
                    _ => None,
                })
                .expect("marker must use the scene's geometry");
            assert_eq!(path.closed, marker.filled);
            assert_eq!(
                path.fill,
                if marker.filled {
                    color32(Color::rgb(20, 40, 60))
                } else {
                    Color32::TRANSPARENT
                }
            );
        }
        let expected: Vec<_> = connector_stroke_path(points, markers)
            .into_iter()
            .map(|point| xf.pos(point))
            .collect();
        assert!(output.shapes.iter().any(|shape| matches!(
            &shape.shape,
            Shape::Path(path) if path.points == expected && !path.closed
        )));
    }

    #[test]
    fn board_items_render_in_scene_order_with_zone_fill_clipping_and_grid() {
        use thinoda_core::{
            Band, CanvasConnector, CanvasObject, CanvasObjectId, Endpoint, GridKind, InkStroke,
            InkTool, ShapeKind, Zone, ZoneId,
        };

        let mut sheet = Sheet::new("Board", "Central topic");
        let layout = thinoda_core::layout_sheet(&sheet, &thinoda_core::ApproxMeasure, false);
        let topic_rect = layout.rect(sheet.root).unwrap();
        let center = topic_rect.center();
        let mut below = CanvasObject::shape(ShapeKind::Rectangle, topic_rect);
        below.id = CanvasObjectId(101);
        below.band = Band::Below;
        below.style.fill = Some(Color::rgb(230, 40, 40));

        let mut ink = CanvasObject::ink(InkStroke::new(
            InkTool::Highlighter,
            Color::rgb(20, 180, 90),
            8.0,
            vec![
                center + Vec2::new(150.0, 80.0),
                center + Vec2::new(190.0, 100.0),
                center + Vec2::new(220.0, 75.0),
            ],
        ));
        ink.id = CanvasObjectId(102);
        ink.band = Band::Below;

        let mut connector = CanvasObject::connector(CanvasConnector::new(
            Endpoint::Point(center + Vec2::new(-200.0, -140.0)),
            Endpoint::Point(center + Vec2::new(-90.0, -140.0)),
        ));
        connector.id = CanvasObjectId(103);
        connector.band = Band::Below;
        if let CanvasObjectKind::Connector(connector) = &mut connector.kind {
            connector.label = "Connect".into();
            connector.color = Some(Color::rgb(110, 30, 210));
        }

        let mut above = CanvasObject::shape(
            ShapeKind::Diamond,
            thinoda_core::Rect::from_center_size(center, topic_rect.size),
        );
        above.id = CanvasObjectId(104);
        above.band = Band::Above;
        above.style.fill = Some(Color::rgb(35, 75, 235));

        let zone = Zone {
            id: ZoneId(90),
            name: "Colored zone".into(),
            frame: thinoda_core::Rect::from_center_size(center, Vec2::new(70.0, 55.0)),
            roots: vec![sheet.root],
            objects: vec![],
            keep_empty: false,
            color: Some(Color::rgb(50, 210, 130)),
            background_color: None,
            clip: true,
        };
        sheet.zones.push(zone);
        sheet.canvas.objects.extend([below, ink, connector, above]);
        sheet.board.grid = GridKind::Lines;

        let card = sheet.add_floating("Sticky card", center + Vec2::new(300.0, 120.0));
        sheet.topic_mut(card).unwrap().card = Some(thinoda_core::Card {
            kind: CardKind::Sticky,
            size: Vec2::new(150.0, 95.0),
            rotation: 12.0,
            color: Some(Color::rgb(255, 215, 50)),
        });
        let layout = thinoda_core::layout_sheet(&sheet, &thinoda_core::ApproxMeasure, false);
        let scene = build_scene(&sheet, &layout);
        assert_eq!(
            scene.items.iter().map(|item| item.band).collect::<Vec<_>>(),
            [
                SceneBand::Frames,
                SceneBand::BelowMap,
                SceneBand::BelowMap,
                SceneBand::BelowMap,
                SceneBand::Map,
                SceneBand::Map,
                SceneBand::AboveMap,
            ]
        );

        let xf = Xf {
            origin: pos2(380.0, 290.0),
            zoom: 1.0,
        };
        let ctx = egui::Context::default();
        let mut output = ctx.run_pass(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
                ..Default::default()
            },
            |ctx| {
                paint_map(
                    &ctx.layer_painter(egui::LayerId::background()),
                    &sheet,
                    &layout,
                    xf,
                    &PaintOptions {
                        show_grid: true,
                        ..Default::default()
                    },
                );
            },
        );
        output.textures_delta.clear();

        let fill_index = |color| {
            output
                .shapes
                .iter()
                .position(|shape| matches!(&shape.shape, Shape::Path(path) if path.fill == color))
        };
        let below_index = fill_index(color32(Color::rgb(230, 40, 40))).unwrap();
        let above_index = fill_index(color32(Color::rgb(35, 75, 235))).unwrap();
        let card_index = fill_index(color32(Color::rgb(255, 215, 50))).unwrap();
        assert!(below_index < above_index);
        assert!(card_index < above_index);
        assert!(output.shapes.iter().any(|shape| {
            shape.clip_rect == xf.rect(sheet.zones[0].frame)
                && matches!(&shape.shape, Shape::Path(path) if path.fill == color32(Color::rgb(230, 40, 40)))
        }));
        assert!(output.shapes.iter().any(|shape| matches!(
            &shape.shape,
            Shape::Rect(rect) if rect.fill.g() > rect.fill.r() && rect.fill.a() < 255
        )));
        assert!(
            output.shapes.iter().any(|shape| match &shape.shape {
                Shape::Path(path) => matches!(
                    path.stroke.color,
                    egui::epaint::ColorMode::Solid(color)
                        if color == opacity_color(color32(Color::rgb(20, 180, 90)), 0.28)
                ),
                Shape::Mesh(mesh) => mesh.vertices.iter().any(|vertex| {
                    vertex.color == opacity_color(color32(Color::rgb(20, 180, 90)), 0.28)
                }),
                Shape::LineSegment { stroke, .. } => {
                    stroke.color == opacity_color(color32(Color::rgb(20, 180, 90)), 0.28)
                }
                _ => false,
            }),
            "expected a translucent highlighter stroke"
        );
        assert!(output.shapes.iter().any(|shape| matches!(
            &shape.shape,
            Shape::Path(path) if matches!(
                path.stroke.color,
                egui::epaint::ColorMode::Solid(color) if color == color32(Color::rgb(110, 30, 210))
            ) || matches!(
                &shape.shape,
                Shape::LineSegment { stroke, .. } if stroke.color == color32(Color::rgb(110, 30, 210))
            )
        )));
        assert!(output.shapes.iter().any(|shape| matches!(
            &shape.shape,
            Shape::LineSegment { stroke, .. } if stroke.color.a() == 34
        )));
    }
}
