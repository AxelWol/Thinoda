//! Canvas painting of shape style effects: gradients, fill patterns, dashed
//! outlines, shadows and glass. Geometry comes from `thinoda_core::shape_style`
//! so the exporters draw the same marks.

use crate::render::{Xf, color32};
use egui::{Color32, Painter, Shape, Stroke};
use thinoda_core::{
    Rect,
    board::ObjectStyle,
    geometry::{Color, Vec2},
    shape_style::{self, FillPaint, ShapePaint},
};

const RADIAL_SUBDIVISIONS: u32 = 3;

fn triangulate(points: &[Vec2]) -> Vec<[usize; 3]> {
    let coordinates: Vec<f64> = points
        .iter()
        .flat_map(|p| [f64::from(p.x), f64::from(p.y)])
        .collect();
    earcutr::earcut(&coordinates, &[], 2)
        .map(|indices| indices.as_chunks::<3>().0.to_vec())
        .unwrap_or_default()
}

fn push_triangle(
    mesh: &mut egui::Mesh,
    xf: Xf,
    offset: Vec2,
    triangle: [Vec2; 3],
    color: &dyn Fn(Vec2) -> Color32,
    depth: u32,
) {
    if depth > 0 {
        let [a, b, c] = triangle;
        let (ab, bc, ca) = ((a + b) * 0.5, (b + c) * 0.5, (c + a) * 0.5);
        for t in [[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]] {
            push_triangle(mesh, xf, offset, t, color, depth - 1);
        }
        return;
    }
    let base = mesh.vertices.len() as u32;
    for point in triangle {
        mesh.colored_vertex(xf.pos(point + offset), color(point));
    }
    mesh.add_triangle(base, base + 1, base + 2);
}

/// Fills each closed polygon independently, coloured per point.
pub(crate) fn fill_polygons(
    painter: &Painter,
    polygons: &[Vec<Vec2>],
    xf: Xf,
    offset: Vec2,
    color: &dyn Fn(Vec2) -> Color32,
    subdivide: bool,
) {
    let depth = if subdivide { RADIAL_SUBDIVISIONS } else { 0 };
    let mut mesh = egui::Mesh::default();
    for polygon in polygons.iter().filter(|p| p.len() >= 3) {
        for [a, b, c] in triangulate(polygon) {
            push_triangle(
                &mut mesh,
                xf,
                offset,
                [polygon[a], polygon[b], polygon[c]],
                color,
                depth,
            );
        }
    }
    if !mesh.is_empty() {
        painter.add(Shape::mesh(mesh));
    }
}

fn paint_color(paint: &ShapePaint) -> impl Fn(Vec2) -> Color32 + '_ {
    move |point| color32(paint.color_at(point))
}

/// Paints a styled fill that is not plain solid: a gradient mesh or pattern
/// marks clipped to `polygons`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_fill(
    painter: &Painter,
    polygons: &[Vec<Vec2>],
    paint: &FillPaint,
    rect: Rect,
    rotation: f32,
    stroke_width: f32,
    xf: Xf,
) {
    match paint {
        FillPaint::Solid(color) => {
            let color = color32(*color);
            fill_polygons(painter, polygons, xf, Vec2::ZERO, &|_| color, false);
        }
        FillPaint::Gradient(gradient) => fill_polygons(
            painter,
            polygons,
            xf,
            Vec2::ZERO,
            &paint_color(gradient),
            matches!(gradient, ShapePaint::Radial { .. }),
        ),
        FillPaint::Pattern(color, pattern) => {
            let marks =
                shape_style::pattern_marks(polygons, rect, rotation, *pattern, stroke_width);
            let color = color32(*color);
            let stroke = Stroke::new((marks.width * xf.zoom).max(0.5), color);
            for line in &marks.lines {
                painter.add(Shape::line(
                    line.iter().map(|p| xf.pos(*p)).collect(),
                    stroke,
                ));
            }
            let radius = (marks.dot_radius * xf.zoom).max(0.5);
            for dot in &marks.dots {
                painter.circle_filled(xf.pos(*dot), radius, color);
            }
        }
    }
}

/// Strokes `points` with the style's dash lengths, or solid when `lengths` is `None`.
pub(crate) fn stroke_path(
    painter: &Painter,
    points: &[Vec2],
    closed: bool,
    lengths: Option<&[f32]>,
    stroke: Stroke,
    xf: Xf,
    offset: Vec2,
) {
    if stroke == Stroke::NONE || points.len() < 2 {
        return;
    }
    let screen = |points: &[Vec2]| -> Vec<egui::Pos2> {
        points.iter().map(|p| xf.pos(*p + offset)).collect()
    };
    match lengths {
        Some(lengths) => {
            for dash in shape_style::dash_polyline(points, closed, lengths) {
                painter.add(Shape::line(screen(&dash), stroke));
            }
        }
        None if closed => {
            painter.add(Shape::closed_line(screen(points), stroke));
        }
        None => {
            painter.add(Shape::line(screen(points), stroke));
        }
    }
}

/// Paints the drop shadow: filled polygons and stroked paths offset and
/// recoloured, beneath the shape.
pub(crate) fn paint_shadow(
    painter: &Painter,
    fills: &[Vec<Vec2>],
    strokes: &[(Vec<Vec2>, bool)],
    stroke_width: f32,
    opacity: f32,
    xf: Xf,
) {
    let color = color32(shape_style::shadow_color(opacity));
    fill_polygons(
        painter,
        fills,
        xf,
        shape_style::SHADOW_OFFSET,
        &|_| color,
        false,
    );
    if stroke_width > 0.0 {
        let stroke = Stroke::new((stroke_width * xf.zoom).max(0.5), color);
        for (points, closed) in strokes {
            stroke_path(
                painter,
                points,
                *closed,
                None,
                stroke,
                xf,
                shape_style::SHADOW_OFFSET,
            );
        }
    }
}

/// Paints the glass highlight over `outline`.
pub(crate) fn paint_glass(
    painter: &Painter,
    style: &ObjectStyle,
    rect: Rect,
    rotation: f32,
    outline: &[Vec2],
    stroke_width: Option<f32>,
    xf: Xf,
) {
    if let Some((region, paint)) = shape_style::glass(style, rect, rotation, outline, stroke_width)
    {
        fill_polygons(
            painter,
            &[region],
            xf,
            Vec2::ZERO,
            &paint_color(&paint),
            false,
        );
    }
}

/// The core colour of an egui colour (unpremultiplied).
pub(crate) fn core_color(color: Color32) -> Color {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    Color { r, g, b, a }
}
