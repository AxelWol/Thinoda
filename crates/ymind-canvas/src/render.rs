//! Painting of a laid-out sheet.

use crate::measure::BOLD_WIDTH;
use crate::view::ObjectSelection;
use egui::{
    Align, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, pos2,
    text::{LayoutJob, TextFormat},
    vec2,
};
use ymind_core::{
    Layout, Sheet, TextAlign, TopicId, TopicShape,
    geometry::{Color, Vec2},
    layout::{ConnectorKind, Dir, Icon, TopicLayout, cubic},
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

    pub fn rect(&self, r: ymind_core::Rect) -> Rect {
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
}

/// Background color of the sheet.
pub fn sheet_background(sheet: &Sheet, dark: bool) -> Color32 {
    color32(Theme::builtin(&sheet.theme.theme_id).background(&sheet.theme, dark))
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
        paint_topic(
            painter,
            t,
            topic,
            xf,
            bg,
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

fn paint_topic(
    painter: &Painter,
    t: &TopicLayout,
    topic: Option<&ymind_core::Topic>,
    xf: Xf,
    bg: Color32,
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

    let text_color = a(color32(st.text_color));
    // Image placeholder.
    if let Some(ir) = t.image_rect {
        let ir = xf.rect(ir);
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

    // To-Do checkbox.
    if let (Some(tr), Some(done)) = (t.todo_rect, topic.and_then(|x| x.todo)) {
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
        }
    }

    // Markers.
    for (m, mr) in &t.markers {
        paint_marker(painter, m, xf.rect(*mr), dim);
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
pub fn paint_marker(painter: &Painter, id: &str, r: Rect, dim: bool) {
    let a = |c: Color32| if dim { c.gamma_multiply(0.3) } else { c };
    let c = r.center();
    let rad = r.width().min(r.height()) / 2.0;
    let named = |name: &str| match name {
        "red" => Color32::from_rgb(0xE5, 0x48, 0x4D),
        "orange" => Color32::from_rgb(0xF5, 0x8A, 0x1F),
        "yellow" => Color32::from_rgb(0xF2, 0xC0, 0x1E),
        "green" => Color32::from_rgb(0x3C, 0xB3, 0x71),
        "blue" => Color32::from_rgb(0x2E, 0x7C, 0xF6),
        "purple" => Color32::from_rgb(0x8E, 0x5C, 0xE6),
        _ => Color32::from_rgb(0x8A, 0x8F, 0x98),
    };
    let (group, rest) = id.split_once('-').unwrap_or((id, ""));
    match group {
        "priority" => {
            let colors = [
                Color32::from_rgb(0xE5, 0x48, 0x4D),
                Color32::from_rgb(0xF5, 0x8A, 0x1F),
                Color32::from_rgb(0x2E, 0x7C, 0xF6),
                Color32::from_rgb(0x3C, 0xB3, 0x71),
                Color32::from_rgb(0x8E, 0x5C, 0xE6),
            ];
            let n: usize = rest.parse().unwrap_or(1);
            painter.circle_filled(c, rad, a(colors[(n.saturating_sub(1)) % colors.len()]));
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                rest,
                FontId::proportional(rad * 1.3),
                a(Color32::WHITE),
            );
        }
        "task" => {
            let frac = match rest {
                "start" => 0.0,
                "oct" => 0.125,
                "quarter" => 0.25,
                "3oct" => 0.375,
                "half" => 0.5,
                "5oct" => 0.625,
                "3quar" => 0.75,
                "7oct" => 0.875,
                _ => 1.0,
            };
            let green = a(Color32::from_rgb(0x3C, 0xB3, 0x71));
            painter.circle(c, rad - 0.5, a(Color32::WHITE), Stroke::new(1.2, green));
            if frac >= 1.0 {
                painter.circle_filled(c, rad - 0.5, green);
                painter.add(Shape::line(
                    vec![
                        c + vec2(-rad * 0.45, 0.0),
                        c + vec2(-rad * 0.1, rad * 0.35),
                        c + vec2(rad * 0.5, -rad * 0.35),
                    ],
                    Stroke::new(1.5, Color32::WHITE),
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
            let col = a(named(rest));
            let x0 = r.left() + r.width() * 0.25;
            painter.line_segment(
                [pos2(x0, r.top() + 1.0), pos2(x0, r.bottom() - 1.0)],
                Stroke::new(1.4, a(Color32::from_gray(90))),
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
            let col = a(named(rest));
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
            let glyph = ymind_core::markers::glyph(id);
            let has = painter
                .ctx()
                .fonts_mut(|f| f.has_glyphs(&FontId::proportional(rad * 1.6), glyph));
            if has && !glyph.is_empty() {
                painter.text(
                    c,
                    egui::Align2::CENTER_CENTER,
                    glyph,
                    FontId::proportional(rad * 1.6),
                    a(Color32::from_rgb(0x44, 0x4C, 0x58)),
                );
            } else {
                painter.circle_filled(c, rad, a(Color32::from_rgb(0x8A, 0x8F, 0x98)));
                let initial: String = id.chars().take(1).collect::<String>().to_uppercase();
                painter.text(
                    c,
                    egui::Align2::CENTER_CENTER,
                    initial,
                    FontId::proportional(rad * 1.2),
                    a(Color32::WHITE),
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
    fn selected_summary_paints_descendant_range_frame_and_topic_connector() {
        for (structure, dir) in [
            (ymind_core::Structure::LogicRight, Dir::Right),
            (ymind_core::Structure::LogicLeft, Dir::Left),
            (ymind_core::Structure::OrgDown, Dir::Down),
            (ymind_core::Structure::OrgUp, Dir::Up),
        ] {
            let mut sheet = Sheet::new("S", "Root");
            sheet.structure = structure;
            let a = sheet.add_child(sheet.root, None, "A");
            let b = sheet.add_child(sheet.root, None, "B");
            let child = sheet.add_child(a, None, "Descendant");
            let summary = sheet.add_summary(&[a, b], "Conclusion").unwrap();
            let layout = ymind_core::layout_sheet(&sheet, &ymind_core::ApproxMeasure, false);
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
}
