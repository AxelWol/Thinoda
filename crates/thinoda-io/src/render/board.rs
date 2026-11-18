//! Freeform board painting shared by the SVG (and so PNG, JPEG, PDF, print) and the
//! editable PowerPoint renderers. Objects are reduced to absolute, already rotated
//! primitives so both back ends draw identical geometry from the shared core scene.

use std::{borrow::Cow, collections::HashMap, sync::Arc};
use thinoda_core::{
    Color, Rect, Sheet, Vec2,
    board::{
        CanvasObject, CanvasObjectKind, Card, CardKind, InkTool, RectangleSidePath, ShapeKind,
        rectangle_side_paths, rotate_around,
    },
    label_style::{self, LabelLayout, LabelStyle},
    scene::{
        EdgeBody, Scene, SceneBand, SceneGeometry, SceneId, SceneItem, connector_stroke_paths,
    },
    shape_style::{self, FillPaint, ShapePaint},
};

use crate::VisualExportContext;

/// Multiplier applied to highlighter ink.
pub(crate) const HIGHLIGHTER_ALPHA: f32 = 0.35;
pub(crate) const LABEL_SIZE: f32 = 14.0;

pub(crate) enum Prim {
    /// Polyline or closed polygon in sheet coordinates.
    Path {
        points: Vec<Vec2>,
        closed: bool,
        fill: Option<Color>,
        stroke: Option<(Color, f32)>,
        /// Round caps and joins (ink).
        round: bool,
    },
    /// All visible rectangle sides are stroked as one compound path.
    RectangleSides {
        paths: Vec<RectangleSidePath>,
        stroke: Color,
        width: f32,
    },
    /// Closed polygon filled with a gradient (shape fills and glass).
    Gradient {
        points: Vec<Vec2>,
        paint: ShapePaint,
    },
    /// Circular connector marker in sheet coordinates.
    Circle {
        center: Vec2,
        radius: f32,
        fill: Option<Color>,
        stroke: Color,
        width: f32,
    },
    /// Two connected quadratic spans of an open connector marker.
    Quadratic {
        points: [Vec2; 5],
        stroke: Color,
        width: f32,
    },
    /// Centered text lines in an unrotated `rect`, turned around `pivot`.
    Text {
        rect: Rect,
        pivot: Vec2,
        rotation: f32,
        lines: Vec<String>,
        size: f32,
        color: Color,
        /// Backdrop behind the text (connector labels).
        backdrop: Option<Color>,
        /// Unfaded backdrop when text and backdrop share one opacity group.
        group_backdrop: Option<Color>,
    },
    /// Shape label with a text style, laid out by `thinoda_core::label_style`.
    StyledLabel {
        layout: LabelLayout,
        style: Arc<LabelStyle>,
        /// Faded text colour.
        color: Color,
        /// Object opacity applied to the label background and border.
        opacity: f32,
    },
}

/// The styled label of a shape whose default text area is `base`.
fn styled_label(object: &CanvasObject, base: Rect, label: &str, color: Color) -> Option<Prim> {
    let style = object
        .label_style
        .clone()
        .unwrap_or_else(|| Arc::new(LabelStyle::default()));
    Some(Prim::StyledLabel {
        layout: label_style::layout(object, base, label)?,
        style,
        color,
        opacity: object.style.opacity,
    })
}

fn styled_connector_label(
    object: &CanvasObject,
    anchor: Vec2,
    label: &str,
    wrap: bool,
    color: Color,
    backdrop: Color,
    group_backdrop: Option<Color>,
) -> Option<[Prim; 2]> {
    let rich = object.label_rich_text.as_deref()?;
    let layout = label_style::layout_connector(anchor, label, rich, wrap);
    let rect = Rect::from_center_size(
        layout.block.center(),
        layout.block.size + Vec2::new(8.0, 4.0),
    );
    Some([
        Prim::Text {
            rect,
            pivot: anchor,
            rotation: 0.0,
            lines: Vec::new(),
            size: layout.size,
            color,
            backdrop: Some(backdrop),
            group_backdrop,
        },
        Prim::StyledLabel {
            layout,
            style: Arc::new(LabelStyle::default()),
            color,
            opacity: object.style.opacity,
        },
    ])
}

fn faded(color: Color, opacity: f32) -> Color {
    let opacity = if opacity.is_finite() {
        opacity.clamp(0.0, 1.0)
    } else {
        1.0
    };
    Color::rgba(
        color.r,
        color.g,
        color.b,
        (f32::from(color.a) * opacity).round() as u8,
    )
}

/// Greedy word wrap with an approximate glyph width; explicit newlines are kept.
pub(crate) fn wrap(text: &str, width: f32, size: f32) -> Vec<String> {
    label_style::wrap_lines(text, width, size)
}

/// Canvas objects of `scene` in `band`, in paint order, with their source object.
pub(crate) fn band_items<'a>(
    sheet: &'a Sheet,
    scene: &'a Scene,
    band: SceneBand,
) -> impl Iterator<Item = (&'a SceneItem, Cow<'a, CanvasObject>)> + 'a {
    let objects: HashMap<_, _> = sheet
        .canvas
        .objects
        .iter()
        .map(|object| (object.id, object))
        .collect();
    scene
        .items
        .iter()
        .filter(move |item| item.band == band)
        .filter_map(move |item| {
            let SceneId::Object(id) = item.id else {
                return None;
            };
            objects.get(&id).copied().map(|object| {
                let object = thinoda_core::scene_table::with_table_stripes(
                    &sheet.canvas,
                    thinoda_core::object_data::with_resolved_placeholders(
                        &sheet.canvas,
                        object,
                        Some(&sheet.title),
                    ),
                );
                (item, object)
            })
        })
}

/// Bounds of painted board content that the topic layout does not cover: canvas
/// objects and rotated cards.
pub(crate) fn extra_bounds(sheet: &Sheet, scene: &Scene) -> Option<Rect> {
    scene
        .items
        .iter()
        .filter(|item| match item.id {
            SceneId::Object(_) => true,
            SceneId::Topic(id) => sheet.topic(id).is_some_and(|t| t.card.is_some()),
            SceneId::Zone(_) => false,
        })
        .filter_map(SceneItem::visible_bounds)
        .reduce(|a, b| a.union(&b))
}

/// Primitives of one canvas object. `background` is the page color for contrast.
pub(crate) fn object_prims(
    item: &SceneItem,
    object: &CanvasObject,
    background: Color,
    dark: bool,
    visual: VisualExportContext,
) -> Vec<Prim> {
    let opacity = object.style.opacity;
    let ink = if dark {
        Color::rgb(0xC8, 0xD0, 0xDA)
    } else {
        Color::rgb(0x4A, 0x55, 0x68)
    };
    let automatic_stroke = visual.automatic_board_stroke.unwrap_or(ink);
    let mut out = Vec::new();
    if let Some(render) = item.stencil_render.as_ref() {
        let under = object.style.fill.map_or(background, |f| f.over(background));
        let font = object
            .style
            .text_color
            .unwrap_or_else(|| Color::highest_contrast_on(under));
        let slots = thinoda_core::stencil_object::slots(object, automatic_stroke, font);
        let (paths, texts) = thinoda_core::stencil_object::primitives(object, render, &slots);
        let style = &object.style;
        let closed_subpaths = |path: &thinoda_core::stencil_object::StencilPrimitive| {
            path.subpaths
                .iter()
                .filter(|(points, closed)| *closed && points.len() >= 3)
                .map(|(points, _)| points.clone())
                .collect::<Vec<_>>()
        };
        if style.shadow {
            for path in &paths {
                let fills = if path.fill.is_some() {
                    closed_subpaths(path)
                } else {
                    Vec::new()
                };
                shadow_prims(
                    &mut out,
                    &fills,
                    &path.subpaths,
                    path.stroke.as_ref().map_or(0.0, |stroke| stroke.width),
                    opacity,
                );
            }
        }
        let effect_fill = style.gradient.is_some() || !style.fill_pattern.is_solid();
        for path in paths {
            let mut fill = path.fill;
            if path.styled_fill
                && effect_fill
                && let Some(color) = fill.take()
            {
                let paint = shape_style::fill_paint(style, object.rect, object.rotation, color);
                fill_prims(
                    &mut out,
                    &closed_subpaths(&path),
                    &paint,
                    object,
                    style.effective_shape_stroke_width(),
                );
            }
            let dash = path
                .stroke
                .as_ref()
                .and_then(|s| s.dash.clone())
                .filter(|dash| dash.len() >= 2);
            let stroke = path.stroke.map(|s| (s.color, s.width));
            for (points, closed) in path.subpaths {
                if let (Some(dash), Some(stroke)) = (&dash, stroke) {
                    if closed && fill.is_some() {
                        out.push(Prim::Path {
                            points: points.clone(),
                            closed,
                            fill,
                            stroke: None,
                            round: false,
                        });
                    }
                    dashed_prims(&mut out, &points, closed, dash, stroke);
                    continue;
                }
                out.push(Prim::Path {
                    points,
                    closed,
                    fill: if closed { fill } else { None },
                    stroke,
                    round: false,
                });
            }
        }
        if style.glass {
            let outline = thinoda_core::stencil_object::geometry(object, render).outline;
            glass_prims(
                &mut out,
                object,
                &outline,
                style
                    .stroke
                    .is_some()
                    .then(|| style.effective_shape_stroke_width()),
            );
        }
        for text in texts {
            let lines: Vec<String> = text.text.split('\n').map(str::to_owned).collect();
            let longest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
            let size = Vec2::new(
                longest as f32 * text.size * 0.6,
                lines.len() as f32 * text.size * 1.2,
            );
            let min = Vec2::new(
                text.at.x
                    - match text.align {
                        thinoda_core::stencil::TextAlign::Left => 0.0,
                        thinoda_core::stencil::TextAlign::Center => size.x / 2.0,
                        thinoda_core::stencil::TextAlign::Right => size.x,
                    },
                text.at.y
                    - match text.valign {
                        thinoda_core::stencil::TextVAlign::Top => 0.0,
                        thinoda_core::stencil::TextVAlign::Middle => size.y / 2.0,
                        thinoda_core::stencil::TextVAlign::Bottom => size.y,
                    },
            );
            out.push(Prim::Text {
                rect: Rect::from_min_size(min, size),
                pivot: text.at,
                rotation: text.rotation,
                lines,
                size: text.size,
                color: text.color,
                backdrop: None,
                group_backdrop: None,
            });
        }
        if let CanvasObjectKind::Shape { label, .. } = &object.kind
            && (!label.trim().is_empty() || object.has_label_list())
        {
            let rect = thinoda_core::stencil_object::label_rect(object, render);
            out.push(
                styled_label(object, rect, label, faded(font, opacity)).unwrap_or_else(|| {
                    Prim::Text {
                        rect,
                        pivot: object.rect.center(),
                        rotation: object.rotation,
                        lines: wrap(label, rect.size.x, LABEL_SIZE),
                        size: LABEL_SIZE,
                        color: faded(font, opacity),
                        backdrop: None,
                        group_backdrop: None,
                    }
                }),
            );
        }
        return out;
    }
    match (&object.kind, &item.geometry) {
        (CanvasObjectKind::Shape { shape, label }, SceneGeometry::Polygon { outline, details }) => {
            let fill = object.style.fill.map(|c| faded(c, opacity));
            let stroke_color = object
                .style
                .stroke
                .or(object.style.fill.is_none().then_some(automatic_stroke))
                .map(|c| faded(c, opacity));
            let width = if object.style.stroke.is_none()
                && (visual.automatic_board_stroke.is_some()
                    || (object.rectangle_sides.is_some()
                        || matches!(*shape, ShapeKind::DoubleEllipse | ShapeKind::FinalState))
                        && object.style.fill.is_none())
            {
                2.0
            } else {
                object.style.stroke_width.max(0.0)
            };
            let style = &object.style;
            let stroke = stroke_color.map(|c| (c, width));
            let dash = stroke.and_then(|(_, width)| shape_style::dash_lengths(style, width));
            let fill_polygons = shape_style::builtin_fill_polygons(object, outline, details);
            if style.shadow {
                let fills = if fill.is_some_and(|fill| fill.a > 0) {
                    fill_polygons.clone()
                } else {
                    Vec::new()
                };
                let strokes = if stroke.is_some() {
                    shape_style::builtin_stroke_paths(object, outline, details)
                } else {
                    Vec::new()
                };
                shadow_prims(&mut out, &fills, &strokes, width, opacity);
            }
            let fill = match fill {
                Some(color)
                    if color.a > 0
                        && (style.gradient.is_some() || !style.fill_pattern.is_solid()) =>
                {
                    let paint = shape_style::fill_paint(style, object.rect, object.rotation, color);
                    fill_prims(&mut out, &fill_polygons, &paint, object, width);
                    None
                }
                fill => fill,
            };
            let dashed_stroke = stroke;
            let stroke = if dash.is_some() { None } else { stroke };
            let sides = object
                .rectangle_sides
                .map(|sides| sides.flipped(style.flip_h, style.flip_v));
            if *shape == ShapeKind::FinalState {
                for detail in details {
                    out.push(Prim::Path {
                        points: detail.clone(),
                        closed: true,
                        fill,
                        stroke,
                        round: false,
                    });
                }
                out.push(Prim::Path {
                    points: outline.clone(),
                    closed: true,
                    fill: None,
                    stroke,
                    round: false,
                });
            } else if let Some(sides) = sides {
                out.push(Prim::Path {
                    points: outline.clone(),
                    closed: true,
                    fill,
                    stroke: None,
                    round: false,
                });
                if let Some((stroke, width)) = stroke {
                    let paths = rectangle_side_paths(object.rect, object.rotation, sides);
                    if !paths.is_empty() {
                        out.push(Prim::RectangleSides {
                            paths,
                            stroke,
                            width,
                        });
                    }
                }
            } else {
                out.push(Prim::Path {
                    points: outline.clone(),
                    closed: true,
                    fill,
                    stroke,
                    round: false,
                });
            }
            if *shape != ShapeKind::FinalState {
                for detail in details {
                    out.push(Prim::Path {
                        points: detail.clone(),
                        closed: false,
                        fill: None,
                        stroke: if *shape == ShapeKind::DoubleEllipse
                            || visual.automatic_board_stroke.is_some()
                            || dash.is_some()
                        {
                            stroke
                        } else {
                            Some(stroke.unwrap_or((ink, 1.0)))
                        },
                        round: false,
                    });
                }
            }
            if let (Some(dash), Some(stroke)) = (&dash, dashed_stroke) {
                for (points, closed) in shape_style::builtin_stroke_paths(object, outline, details)
                {
                    dashed_prims(&mut out, &points, closed, dash, stroke);
                }
            }
            glass_prims(
                &mut out,
                object,
                outline,
                dashed_stroke.map(|(_, width)| width),
            );
            if !label.trim().is_empty() || object.has_label_list() {
                let rect = thinoda_core::board::object_label_rect(object).unwrap_or(object.rect);
                let under = object.style.fill.map_or(background, |f| f.over(background));
                let color = faded(
                    object
                        .style
                        .text_color
                        .unwrap_or_else(|| Color::highest_contrast_on(under)),
                    opacity,
                );
                out.push(
                    styled_label(object, rect, label, color).unwrap_or_else(|| Prim::Text {
                        rect,
                        pivot: object.rect.center(),
                        rotation: object.rotation,
                        lines: wrap(label, rect.size.x, LABEL_SIZE),
                        size: LABEL_SIZE,
                        color,
                        backdrop: None,
                        group_backdrop: None,
                    }),
                );
            }
        }
        (
            CanvasObjectKind::Ink(stroke),
            SceneGeometry::Ink {
                points,
                width,
                center,
                rotation,
            },
        ) => {
            let alpha = match stroke.tool {
                InkTool::Pen => opacity,
                InkTool::Highlighter => opacity * HIGHLIGHTER_ALPHA,
            };
            let mut points: Vec<Vec2> = points
                .iter()
                .map(|p| rotate_around(*p, *center, *rotation))
                .collect();
            if points.len() == 1 {
                points.push(points[0]);
            }
            out.push(Prim::Path {
                points,
                closed: false,
                fill: None,
                stroke: Some((faded(stroke.color, alpha), width.max(0.1))),
                round: true,
            });
        }
        (
            CanvasObjectKind::Connector(connector),
            SceneGeometry::Ribbon {
                outline,
                mass,
                color,
                ..
            },
        ) => {
            let color = color
                .or(connector.color)
                .or(object.style.stroke)
                .unwrap_or(automatic_stroke);
            out.push(Prim::Path {
                points: outline.clone(),
                closed: true,
                fill: Some(faded(color, opacity * 0.75)),
                stroke: None,
                round: false,
            });
            if let Some(anchor) = thinoda_core::sankey::ribbon_label_anchor(outline) {
                let label = thinoda_core::sankey::flow_label(connector, *mass);
                let text_color = faded(
                    object
                        .style
                        .text_color
                        .unwrap_or_else(|| Color::highest_contrast_on(background)),
                    opacity,
                );
                if let Some(primitives) = styled_connector_label(
                    object,
                    anchor,
                    &label,
                    true,
                    text_color,
                    faded(background, opacity),
                    None,
                ) {
                    out.extend(primitives);
                    return out;
                }
                let lines = wrap(&label, 200.0, 12.0);
                let size = Vec2::new(
                    lines.iter().map(|l| l.chars().count()).max().unwrap_or(1) as f32 * 12.0 * 0.6
                        + 8.0,
                    lines.len() as f32 * 12.0 * 1.25 + 4.0,
                );
                let rect = thinoda_core::Rect {
                    min: anchor - size * 0.5,
                    size,
                };
                out.push(Prim::Text {
                    rect,
                    pivot: rect.center(),
                    rotation: 0.0,
                    lines,
                    size: 12.0,
                    color: text_color,
                    backdrop: Some(background),
                    group_backdrop: None,
                });
            }
        }
        (
            CanvasObjectKind::Connector(connector),
            SceneGeometry::Polyline {
                points,
                width,
                markers,
                color,
                jumps,
                body,
                ..
            },
        ) => {
            let color = faded(
                color
                    .or(connector.color)
                    .or(object.style.stroke)
                    .unwrap_or(automatic_stroke),
                opacity,
            );
            let body = body.as_deref();
            let width = body
                .map_or(*width, |body| body.stroke_width(*width))
                .max(0.1);
            let replaced = body.is_some_and(EdgeBody::replaces_stroke);
            let unset = connector.sankey.is_some_and(|f| f.weight.is_none());
            let paths = if unset {
                thinoda_core::sankey::dash_segments(points)
            } else {
                match body {
                    Some(EdgeBody::Rails(rails)) => rails.clone(),
                    Some(EdgeBody::Band { .. }) => Vec::new(),
                    _ => connector_stroke_paths(points, markers, jumps.as_deref()),
                }
            };
            if let Some(EdgeBody::Band { outline, fill }) = body {
                out.push(Prim::Path {
                    points: outline.clone(),
                    closed: true,
                    fill: fill.map(|fill| faded(fill, opacity)),
                    stroke: Some((color, width)),
                    round: false,
                });
            }
            let inner = match body {
                Some(EdgeBody::Lines {
                    inner: Some((inner, fill)),
                    ..
                }) => Some((faded(*fill, opacity), inner.max(0.1))),
                _ => None,
            };
            for path in paths {
                if let Some(inner) = inner {
                    out.push(Prim::Path {
                        points: path.clone(),
                        closed: false,
                        fill: None,
                        stroke: Some((color, width)),
                        round: true,
                    });
                    out.push(Prim::Path {
                        points: path,
                        closed: false,
                        fill: None,
                        stroke: Some(inner),
                        round: true,
                    });
                } else {
                    out.push(Prim::Path {
                        points: path,
                        closed: false,
                        fill: None,
                        stroke: Some((color, width)),
                        round: true,
                    });
                }
            }
            let n = points.len();
            if connector.arrow_end && n >= 2 {
                if let Some(marker) = markers[1] {
                    out.extend(
                        marker
                            .parts()
                            .map(|part| marker_primitive(part, width, color)),
                    );
                } else if connector.end_marker.is_none() && !replaced {
                    out.push(arrowhead(points[n - 2], points[n - 1], width, color));
                }
            }
            if connector.arrow_start && n >= 2 {
                if let Some(marker) = markers[0] {
                    out.extend(
                        marker
                            .parts()
                            .map(|part| marker_primitive(part, width, color)),
                    );
                } else if connector.start_marker.is_none() && !replaced {
                    out.push(arrowhead(points[1], points[0], width, color));
                }
            }
            let mut labels: Vec<(String, Vec2, bool, bool)> = Vec::new();
            if unset {
                labels.push((
                    thinoda_core::sankey::flow_label(connector, 0.0),
                    midpoint(points),
                    true,
                    true,
                ));
            } else if (!connector.label.is_empty() || object.has_label_list())
                && let Some(anchor) = connector.primary_label_anchor(points)
            {
                labels.push((connector.label.clone(), anchor, false, true));
            }
            labels.extend(
                connector
                    .label_anchors(points)
                    .into_iter()
                    .skip(usize::from(!connector.label.is_empty()))
                    .filter(|(text, _)| !text.trim().is_empty())
                    .map(|(text, anchor)| (text.to_owned(), anchor, false, false)),
            );
            for (label, mid, flow, primary) in labels {
                if !flow && label.trim().is_empty() && !object.has_label_list() {
                    continue;
                }
                let lines = wrap(&label, 200.0, 12.0);
                let longest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(1);
                let size = Vec2::new(
                    longest as f32 * 12.0 * 0.6 + 8.0,
                    lines.len() as f32 * 12.0 * 1.25 + 4.0,
                );
                let text_color = object
                    .style
                    .text_color
                    .map(|text| faded(text, opacity))
                    .unwrap_or(color);
                let backdrop = if object.style.text_color.is_some() {
                    faded(background, f32::from(text_color.a) / 255.0)
                } else {
                    background
                };
                let group_backdrop = object.style.text_color.map(|_| background);
                if primary
                    && let Some(primitives) = styled_connector_label(
                        object,
                        mid,
                        &label,
                        flow,
                        text_color,
                        backdrop,
                        group_backdrop,
                    )
                {
                    out.extend(primitives);
                    continue;
                }
                out.push(Prim::Text {
                    rect: Rect::from_center_size(mid, size),
                    pivot: mid,
                    rotation: 0.0,
                    lines,
                    size: 12.0,
                    color: text_color,
                    backdrop: Some(backdrop),
                    group_backdrop,
                });
            }
        }
        _ => {}
    }
    out
}

/// Offset, recoloured copies of filled polygons and stroked paths, beneath the shape.
fn shadow_prims(
    out: &mut Vec<Prim>,
    fills: &[Vec<Vec2>],
    strokes: &[(Vec<Vec2>, bool)],
    width: f32,
    opacity: f32,
) {
    let color = shape_style::shadow_color(opacity);
    let shift = |points: &[Vec2]| -> Vec<Vec2> {
        points
            .iter()
            .map(|p| *p + shape_style::SHADOW_OFFSET)
            .collect()
    };
    for polygon in fills.iter().filter(|p| p.len() >= 3) {
        out.push(Prim::Path {
            points: shift(polygon),
            closed: true,
            fill: Some(color),
            stroke: None,
            round: false,
        });
    }
    if width > 0.0 {
        for (points, closed) in strokes.iter().filter(|(p, _)| p.len() >= 2) {
            out.push(Prim::Path {
                points: shift(points),
                closed: *closed,
                fill: None,
                stroke: Some((color, width)),
                round: false,
            });
        }
    }
}

/// Gradient polygons or pattern marks for a non-solid styled fill.
fn fill_prims(
    out: &mut Vec<Prim>,
    polygons: &[Vec<Vec2>],
    paint: &FillPaint,
    object: &CanvasObject,
    stroke_width: f32,
) {
    match paint {
        FillPaint::Solid(color) => {
            for polygon in polygons {
                out.push(Prim::Path {
                    points: polygon.clone(),
                    closed: true,
                    fill: Some(*color),
                    stroke: None,
                    round: false,
                });
            }
        }
        FillPaint::Gradient(paint) => {
            for polygon in polygons.iter().filter(|p| p.len() >= 3) {
                out.push(Prim::Gradient {
                    points: polygon.clone(),
                    paint: *paint,
                });
            }
        }
        FillPaint::Pattern(color, pattern) => {
            let marks = shape_style::pattern_marks(
                polygons,
                object.rect,
                object.rotation,
                *pattern,
                stroke_width,
            );
            for line in marks.lines {
                out.push(Prim::Path {
                    points: line,
                    closed: false,
                    fill: None,
                    stroke: Some((*color, marks.width)),
                    round: false,
                });
            }
            for center in marks.dots {
                out.push(Prim::Circle {
                    center,
                    radius: marks.dot_radius,
                    fill: Some(*color),
                    stroke: Color::TRANSPARENT,
                    width: 0.0,
                });
            }
        }
    }
}

/// The visible dashes of a stroked path.
fn dashed_prims(
    out: &mut Vec<Prim>,
    points: &[Vec2],
    closed: bool,
    dash: &[f32],
    stroke: (Color, f32),
) {
    for points in shape_style::dash_polyline(points, closed, dash) {
        out.push(Prim::Path {
            points,
            closed: false,
            fill: None,
            stroke: Some(stroke),
            round: false,
        });
    }
}

fn glass_prims(
    out: &mut Vec<Prim>,
    object: &CanvasObject,
    outline: &[Vec2],
    stroke_width: Option<f32>,
) {
    if let Some((points, paint)) = shape_style::glass(
        &object.style,
        object.rect,
        object.rotation,
        outline,
        stroke_width,
    ) && points.len() >= 3
    {
        out.push(Prim::Gradient { points, paint });
    }
}

fn marker_primitive(
    marker: thinoda_core::scene::ConnectorMarkerPart,
    width: f32,
    color: Color,
) -> Prim {
    if let Some((center, radius)) = marker.circle() {
        Prim::Circle {
            center,
            radius,
            fill: marker.filled.then_some(color),
            stroke: color,
            width,
        }
    } else if let Some(points) = marker.curve() {
        Prim::Quadratic {
            points,
            stroke: color,
            width,
        }
    } else {
        Prim::Path {
            points: marker.points().to_vec(),
            closed: marker.closed,
            fill: marker.filled.then_some(color),
            stroke: Some((color, width)),
            round: false,
        }
    }
}

fn arrowhead(from: Vec2, tip: Vec2, width: f32, color: Color) -> Prim {
    let d = tip - from;
    let len = d.length();
    let dir = if len > f32::EPSILON {
        Vec2::new(d.x / len, d.y / len)
    } else {
        Vec2::new(1.0, 0.0)
    };
    let normal = Vec2::new(-dir.y, dir.x);
    let size = thinoda_core::board::legacy_arrow_size(width);
    let base = Vec2::new(tip.x - dir.x * size, tip.y - dir.y * size);
    Prim::Path {
        points: vec![
            tip,
            Vec2::new(
                base.x + normal.x * size * 0.5,
                base.y + normal.y * size * 0.5,
            ),
            Vec2::new(
                base.x - normal.x * size * 0.5,
                base.y - normal.y * size * 0.5,
            ),
        ],
        closed: true,
        fill: Some(color),
        stroke: None,
        round: false,
    }
}

/// Point halfway along a polyline.
pub(crate) fn midpoint(points: &[Vec2]) -> Vec2 {
    thinoda_core::board::polyline_midpoint(points).unwrap_or(Vec2::ZERO)
}

/// Preserve existing image-card exports, except when their background is fully transparent.
pub(crate) fn export_card_background(card: &Card) -> bool {
    card.kind != CardKind::Image || card.color.is_some_and(|color| color.a == 0)
}

/// Match the canvas defaults for sticky and text cards in each color mode.
pub(crate) fn card_fill(card: &Card, dark: bool) -> Option<Color> {
    match card.kind {
        CardKind::Sticky => Some(card.color.unwrap_or(if dark {
            Color::rgb(0x9B, 0x7B, 0x24)
        } else {
            Color::rgb(0xFF, 0xF2, 0x9E)
        })),
        CardKind::Text => Some(card.color.unwrap_or(if dark {
            Color::rgb(0x3E, 0x48, 0x58)
        } else {
            Color::rgb(0xF5, 0xF7, 0xFA)
        })),
        CardKind::Image => card.color,
    }
}

/// Darker edge of a sticky note.
pub(crate) fn card_edge(fill: Color) -> Color {
    fill.lerp(Color::BLACK, 0.18)
}

#[cfg(test)]
mod tests {
    use super::*;
    use thinoda_core::{
        scene::{SceneId, build_scene},
        scene_layout::{ContainerLayout, ContainerLayoutPolicy, HeaderPolicy, HeaderSide},
        stencil_object::StencilRef,
    };

    #[test]
    fn export_keeps_the_marker_of_an_empty_list_label() {
        let mut sheet = Sheet::new("Board", "");
        let mut object = CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(120.0, 80.0)),
        );
        object.label_rich_text = Some(Arc::new(thinoda_core::InlineText {
            paragraphs: vec![thinoda_core::InlineParagraph {
                index: 0,
                list: thinoda_core::ListKind::Bulleted,
            }],
            ..Default::default()
        }));
        let id = sheet.add_canvas_object(object).unwrap();
        let scene = build_scene(&sheet, &thinoda_core::layout::Layout::default());
        let primitives = object_prims(
            scene.item(SceneId::Object(id)).unwrap(),
            sheet.canvas.object(id).unwrap(),
            Color::WHITE,
            false,
            VisualExportContext::default(),
        );
        assert!(primitives.iter().any(|primitive| matches!(
            primitive,
            Prim::StyledLabel { layout, .. } if layout.lines[0].text == "• "
        )));
    }

    #[test]
    fn export_primitives_use_the_stored_header_side_and_extent() {
        let mut sheet = Sheet::new("Header export", "Root");
        let mut object = CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(120.0, 100.0)),
        );
        if let CanvasObjectKind::Shape { label, .. } = &mut object.kind {
            *label = "External service".into();
        }
        object.stencil = Some(StencilRef::new("thinoda.general", "header-frame-v2"));
        let id = sheet.add_canvas_object(object).unwrap();
        sheet.set_canvas_container(id, true).unwrap();
        sheet
            .set_canvas_container_layout_policy(
                id,
                Some(ContainerLayoutPolicy {
                    header: HeaderPolicy {
                        side: HeaderSide::Right,
                        extent: 20.0,
                    },
                    layout: ContainerLayout::Free,
                }),
            )
            .unwrap();

        let scene = build_scene(&sheet, &thinoda_core::layout::Layout::default());
        let item = scene.item(SceneId::Object(id)).unwrap();
        let object = sheet.canvas.object(id).unwrap();
        let primitives = object_prims(
            item,
            object,
            Color::rgb(255, 255, 255),
            false,
            VisualExportContext::default(),
        );
        assert!(primitives.iter().any(|primitive| match primitive {
            Prim::Path { points, closed, .. } if !closed => points.windows(2).any(|segment| {
                segment[0] == Vec2::new(100.0, 0.0) && segment[1] == Vec2::new(100.0, 100.0)
            }),
            _ => false,
        }));
        assert!(primitives.iter().any(|primitive| matches!(
            primitive,
            Prim::Text {
                rect,
                lines,
                ..
            } if *rect == Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 100.0))
                && lines.join(" ") == "External service"
        )));
    }
}
