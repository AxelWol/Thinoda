//! Paints styled shape labels from the shared core layout so the canvas uses
//! the same line breaks and placement as every exporter.

use egui::{
    Color32, FontId, Painter, Pos2, Stroke,
    emath::Rot2,
    epaint::{PathShape, TextShape},
    text::{LayoutJob, TextFormat},
    vec2,
};
use thinoda_core::{
    geometry::Vec2,
    label_style::{LabelLayout, LabelStyle},
    model::TextAlign,
};

use crate::render::{Xf, color32, opacity_color};

/// Screen-space corners of a text-space rectangle.
fn corners(layout: &LabelLayout, rect: thinoda_core::Rect, xf: Xf) -> Vec<Pos2> {
    let max = rect.max();
    [
        rect.min,
        Vec2::new(max.x, rect.min.y),
        max,
        Vec2::new(rect.min.x, max.y),
    ]
    .into_iter()
    .map(|point| xf.pos(layout.to_sheet(point)))
    .collect()
}

pub(crate) fn paint(
    painter: &Painter,
    layout: &LabelLayout,
    style: &LabelStyle,
    color: Color32,
    opacity: f32,
    xf: Xf,
) {
    if layout.lines.is_empty() {
        return;
    }
    let z = xf.zoom;
    if style.background.is_some() || style.border.is_some() {
        let fill = style.background.map_or(Color32::TRANSPARENT, |fill| {
            opacity_color(color32(fill), opacity)
        });
        let stroke = style.border.map_or(Stroke::NONE, |border| {
            Stroke::new(z.max(0.5), opacity_color(color32(border), opacity))
        });
        painter.add(PathShape::convex_polygon(
            corners(layout, layout.box_rect(), xf),
            fill,
            stroke,
        ));
    }
    let angle = layout.rotation.to_radians();
    let rotation = Rot2::from_angle(angle);
    let pivot = xf.pos(layout.pivot);
    let font = FontId::new(
        crate::measure::render_font_size(layout.size * z),
        crate::measure::font_family(painter.ctx(), style.font_family.as_deref()),
    );
    let decoration = Stroke::new((layout.size * z / 14.0).max(0.6), color);
    for line in &layout.lines {
        if line.text.is_empty() {
            continue;
        }
        let mut job = LayoutJob::default();
        job.append(
            &line.text,
            0.0,
            TextFormat {
                font_id: font.clone(),
                color,
                italics: style.italic,
                underline: if style.underline {
                    decoration
                } else {
                    Stroke::NONE
                },
                strikethrough: if style.strikethrough {
                    decoration
                } else {
                    Stroke::NONE
                },
                ..Default::default()
            },
        );
        let galley = painter.layout_job(job);
        let size = galley.size();
        let rect = xf.rect(line.rect);
        let x = match layout.align {
            TextAlign::Left => rect.left(),
            TextAlign::Center => rect.center().x - size.x / 2.0,
            TextAlign::Right => rect.right() - size.x,
        };
        let unrotated = Pos2::new(x, rect.center().y - size.y / 2.0);
        let origin = pivot + rotation * (unrotated - pivot);
        if style.bold {
            painter.add(
                TextShape::new(
                    origin + rotation * vec2(0.45 * z, 0.0),
                    galley.clone(),
                    color,
                )
                .with_angle(angle),
            );
        }
        painter.add(TextShape::new(origin, galley, color).with_angle(angle));
    }
}
