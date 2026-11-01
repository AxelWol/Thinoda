//! Painting of a laid-out sheet.

use crate::measure::BOLD_WIDTH;
use crate::view::ObjectSelection;
use egui::{
    Align, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, pos2,
    text::{LayoutJob, TextFormat},
    vec2,
};
use std::{collections::HashMap, sync::OnceLock};
use thinoda_core::{
    CanvasObject, CanvasObjectId, CanvasObjectKind, CardKind, GridKind, Layout, SceneBand, SceneId,
    ShapeKind, Sheet, TextAlign, TopicId, TopicShape,
    board::{rotate_around, rotated_corners, shape_label_rect},
    geometry::{Color, Vec2},
    layout::{ConnectorKind, Dir, Icon, TopicLayout, cubic},
    scene::{Scene, SceneGeometry, build_scene},
    theme::Theme,
};

pub(crate) const RELATIONSHIP_SEGMENTS: usize = 32;

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

/// Background color of the sheet.
pub fn sheet_background(sheet: &Sheet, dark: bool) -> Color32 {
    color32(Theme::for_settings(&sheet.theme).background(&sheet.theme, dark))
}

/// Fits a noninteractive sample into a thumbnail, without changing any canvas camera.
pub fn paint_thumbnail(painter: &Painter, sheet: &Sheet, layout: &Layout, rect: Rect, dark: bool) {
    painter.rect_filled(rect, 4, sheet_background(sheet, dark));
    let rect = rect.shrink(5.0);
    let zoom = (rect.width() / layout.bounds.size.x.max(1.0))
        .min(rect.height() / layout.bounds.size.y.max(1.0));
    let center = layout.bounds.center();
    let xf = Xf {
        origin: rect.center() - vec2(center.x, center.y) * zoom,
        zoom,
    };
    paint_map(
        &painter.with_clip_rect(rect),
        sheet,
        layout,
        xf,
        &PaintOptions {
            dark,
            ..Default::default()
        },
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
    let z = xf.zoom;
    let bg = sheet_background(sheet, opts.dark);
    let dim = |id: TopicId| {
        opts.filter.is_some_and(|f| !f.contains(&id))
            || opts.dimmed.is_some_and(|ids| ids.contains(&id))
    };
    let scene = build_scene(sheet, layout);
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
            let fill_opacity = if zone.color.is_some() { 0.16 } else { 0.04 };
            painter.rect(
                rect,
                4.0,
                color.gamma_multiply(fill_opacity),
                Stroke::new(if selected { 2.0 } else { 1.0 }, color),
                StrokeKind::Inside,
            );
            painter.with_clip_rect(rect.shrink(4.0)).text(
                rect.left_top() + vec2(8.0, 6.0),
                egui::Align2::LEFT_TOP,
                &zone.name,
                FontId::proportional(12.0),
                color,
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

    paint_objects_in_band(
        painter,
        sheet,
        &objects,
        &scene,
        SceneBand::BelowMap,
        xf,
        opts,
    );

    // Boundaries.
    for b in &layout.boundaries {
        let bd = sheet.boundaries.iter().find(|x| x.id == b.id);
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
        let clipped = topic_clip_rect(sheet, *id).map(|r| painter.with_clip_rect(xf.rect(r)));
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
        &scene,
        SceneBand::AboveMap,
        xf,
        opts,
    );

    // Selection and badges.
    for id in opts.selection {
        if let Some(t) = layout.topics.get(id) {
            let r = xf.rect(t.rect).expand(3.0 * z.max(0.6));
            painter.rect_stroke(
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
        let r = xf.rect(t.rect).expand(3.0 * z.max(0.6));
        painter.rect_stroke(
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
            paint_badge(painter, t, xf, opts.accent, bg);
        }
    }
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

fn paint_objects_in_band(
    painter: &Painter,
    sheet: &Sheet,
    objects: &HashMap<CanvasObjectId, &CanvasObject>,
    scene: &Scene,
    band: SceneBand,
    xf: Xf,
    opts: &PaintOptions<'_>,
) {
    for item in scene.items.iter().filter(|item| item.band == band) {
        let SceneId::Object(id) = item.id else {
            continue;
        };
        let Some(object) = objects.get(&id) else {
            continue;
        };
        let clipped = object_clip_rect(sheet, object).map(|r| painter.with_clip_rect(xf.rect(r)));
        paint_canvas_object(
            clipped.as_ref().unwrap_or(painter),
            sheet,
            object,
            item,
            xf,
            opts,
        );
    }
}

fn object_clip_rect(sheet: &Sheet, object: &CanvasObject) -> Option<thinoda_core::Rect> {
    sheet
        .zones
        .iter()
        .find(|zone| zone.clip && zone.frame.contains(object.rect.center()))
        .map(|zone| zone.frame)
}

fn topic_clip_rect(sheet: &Sheet, topic: TopicId) -> Option<thinoda_core::Rect> {
    let mut current = Some(topic);
    while let Some(id) = current {
        if let Some(zone) = sheet
            .zones
            .iter()
            .find(|zone| zone.clip && zone.roots.contains(&id))
        {
            return Some(zone.frame);
        }
        current = sheet.topic(id).and_then(|topic| topic.parent);
    }
    None
}

fn opacity_color(color: Color32, opacity: f32) -> Color32 {
    color.gamma_multiply(opacity.clamp(0.0, 1.0))
}

fn default_object_stroke(opts: &PaintOptions<'_>) -> Color32 {
    if opts.accent.a() > 0 {
        opts.accent
    } else if opts.dark {
        Color32::from_gray(225)
    } else {
        Color32::from_gray(65)
    }
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

fn paint_canvas_object(
    painter: &Painter,
    sheet: &Sheet,
    object: &CanvasObject,
    item: &thinoda_core::SceneItem,
    xf: Xf,
    opts: &PaintOptions<'_>,
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
                let font = FontId::proportional((12.0 * z).max(7.0));
                painter.text(
                    xf.pos(text_rect.center()),
                    egui::Align2::CENTER_CENTER,
                    label,
                    font,
                    opacity_color(stroke_color, object.style.opacity),
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
            let points: Vec<Pos2> = points
                .iter()
                .map(|point| xf.pos(rotate_around(*point, *center, *rotation)))
                .collect();
            if points.len() == 1 {
                painter.circle_filled(points[0], (ink.width * z / 2.0).max(0.5), color);
            } else if points.len() > 1 {
                painter.add(Shape::line(
                    points,
                    Stroke::new((ink.width * z).max(0.5), color),
                ));
            }
        }
        (CanvasObjectKind::Connector(connector), SceneGeometry::Polyline { points, .. }) => {
            let color = connector
                .color
                .map(color32)
                .or(object.style.stroke.map(color32))
                .unwrap_or_else(|| default_object_stroke(opts));
            let color = opacity_color(color, object.style.opacity);
            let points: Vec<Pos2> = points.iter().copied().map(|point| xf.pos(point)).collect();
            let stroke = Stroke::new((connector.width * z).max(0.5), color);
            painter.add(Shape::line(points.clone(), stroke));
            if points.len() >= 2 {
                if connector.arrow_start {
                    arrow_head(painter, points[1], points[0], (9.0 * z).max(4.0), color);
                }
                if connector.arrow_end {
                    let n = points.len();
                    arrow_head(
                        painter,
                        points[n - 2],
                        points[n - 1],
                        (9.0 * z).max(4.0),
                        color,
                    );
                }
                if !connector.label.is_empty() {
                    let mid = points[points.len() / 2];
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

fn paint_badge(painter: &Painter, t: &TopicLayout, xf: Xf, accent: Color32, bg: Color32) {
    let z = xf.zoom.max(0.5);
    let c = xf.pos(t.badge_center());
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
    if let Some(card) = topic.and_then(|topic| topic.card.as_ref()) {
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
        let card_fill = card.color.map(color32).unwrap_or(fallback);
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

    let text_color = a(color32(st.text_color));
    // Visible fallback for legacy/malformed images. Successful textures replace it.
    if let Some(ir) = t.image_rect {
        let ir = xf.rect(ir);
        let rendered = topic
            .and_then(|t| t.image.as_deref())
            .is_some_and(|reference| {
                !painter.clip_rect().intersects(ir)
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
            st.font_size * z,
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
            st.font_size * z,
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
            FontId::proportional(ir.height() * 0.9),
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
                FontId::proportional(rad * 1.3),
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
            let has = painter
                .ctx()
                .fonts_mut(|f| f.has_glyphs(&FontId::proportional(rad * 1.6), glyph));
            if has && !glyph.is_empty() {
                painter.text(
                    c,
                    egui::Align2::CENTER_CENTER,
                    glyph,
                    FontId::proportional(rad * 1.6),
                    a(foreground),
                );
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
                    FontId::proportional(rad * 1.2),
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
            color: Some(Color::rgb(50, 210, 130)),
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
