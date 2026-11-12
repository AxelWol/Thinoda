//! Freeform board painting shared by the SVG (and so PNG, JPEG, PDF, print) and the
//! editable PowerPoint renderers. Objects are reduced to absolute, already rotated
//! primitives so both back ends draw identical geometry from the shared core scene.

use std::collections::HashMap;
use thinoda_core::{
    Color, Rect, Sheet, Vec2,
    board::{
        CanvasObject, CanvasObjectKind, Card, CardKind, InkTool, RectangleSidePath, ShapeKind,
        rectangle_side_paths, rotate_around,
    },
    scene::{Scene, SceneBand, SceneGeometry, SceneId, SceneItem, connector_stroke_path},
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
    let max_chars = ((width / (size * 0.55)).floor() as usize).max(1);
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let mut word = word.to_owned();
            while word.chars().count() > max_chars {
                if !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                }
                let split = word
                    .char_indices()
                    .nth(max_chars)
                    .map_or(word.len(), |(index, _)| index);
                lines.push(word[..split].to_owned());
                word = word[split..].to_owned();
            }
            if line.is_empty() {
                line = word;
            } else if line.chars().count() + 1 + word.chars().count() <= max_chars {
                line.push(' ');
                line.push_str(&word);
            } else {
                lines.push(std::mem::replace(&mut line, word));
            }
        }
        lines.push(line);
    }
    while lines.last().is_some_and(String::is_empty) && lines.len() > 1 {
        lines.pop();
    }
    lines
}

/// Canvas objects of `scene` in `band`, in paint order, with their source object.
pub(crate) fn band_items<'a>(
    sheet: &'a Sheet,
    scene: &'a Scene,
    band: SceneBand,
) -> impl Iterator<Item = (&'a SceneItem, &'a CanvasObject)> + 'a {
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
            objects.get(&id).copied().map(|object| (item, object))
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
    if let Some(render) = thinoda_core::stencil_object::render(object) {
        let under = object.style.fill.map_or(background, |f| f.over(background));
        let font = object
            .style
            .text_color
            .unwrap_or_else(|| Color::highest_contrast_on(under));
        let slots = thinoda_core::stencil_object::slots(object, automatic_stroke, font);
        let (paths, texts) = thinoda_core::stencil_object::primitives(object, &render, &slots);
        for path in paths {
            let stroke = path.stroke.map(|s| (s.color, s.width));
            for (points, closed) in path.subpaths {
                out.push(Prim::Path {
                    points,
                    closed,
                    fill: if closed { path.fill } else { None },
                    stroke,
                    round: false,
                });
            }
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
            && !label.trim().is_empty()
        {
            let rect = thinoda_core::stencil_object::label_rect(object, &render);
            out.push(Prim::Text {
                rect,
                pivot: object.rect.center(),
                rotation: object.rotation,
                lines: wrap(label, rect.size.x, LABEL_SIZE),
                size: LABEL_SIZE,
                color: faded(font, opacity),
                backdrop: None,
                group_backdrop: None,
            });
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
            let stroke = stroke_color.map(|c| (c, width));
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
            } else if let Some(sides) = object.rectangle_sides {
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
                        {
                            stroke
                        } else {
                            Some(stroke.unwrap_or((ink, 1.0)))
                        },
                        round: false,
                    });
                }
            }
            if !label.trim().is_empty() {
                let rect = thinoda_core::board::object_label_rect(object).unwrap_or(object.rect);
                let under = object.style.fill.map_or(background, |f| f.over(background));
                out.push(Prim::Text {
                    rect,
                    pivot: object.rect.center(),
                    rotation: object.rotation,
                    lines: wrap(label, rect.size.x, LABEL_SIZE),
                    size: LABEL_SIZE,
                    color: faded(
                        object
                            .style
                            .text_color
                            .unwrap_or_else(|| Color::highest_contrast_on(under)),
                        opacity,
                    ),
                    backdrop: None,
                    group_backdrop: None,
                });
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
                    color: faded(
                        object
                            .style
                            .text_color
                            .unwrap_or_else(|| Color::highest_contrast_on(background)),
                        opacity,
                    ),
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
            let width = width.max(0.1);
            let unset = connector.sankey.is_some_and(|f| f.weight.is_none());
            let paths = if unset {
                thinoda_core::sankey::dash_segments(points)
            } else {
                vec![connector_stroke_path(points, markers)]
            };
            for path in paths {
                out.push(Prim::Path {
                    points: path,
                    closed: false,
                    fill: None,
                    stroke: Some((color, width)),
                    round: true,
                });
            }
            let n = points.len();
            if connector.arrow_end && n >= 2 {
                if let Some(marker) = markers[1] {
                    out.push(marker_primitive(marker, width, color));
                } else if connector.end_marker.is_none() {
                    out.push(arrowhead(points[n - 2], points[n - 1], width, color));
                }
            }
            if connector.arrow_start && n >= 2 {
                if let Some(marker) = markers[0] {
                    out.push(marker_primitive(marker, width, color));
                } else if connector.start_marker.is_none() {
                    out.push(arrowhead(points[1], points[0], width, color));
                }
            }
            if !connector.label.trim().is_empty() || unset {
                let mid = midpoint(points);
                let label = if unset {
                    thinoda_core::sankey::flow_label(connector, 0.0)
                } else {
                    connector.label.clone()
                };
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
                out.push(Prim::Text {
                    rect: Rect::from_center_size(mid, size),
                    pivot: mid,
                    rotation: 0.0,
                    lines,
                    size: 12.0,
                    color: text_color,
                    backdrop: Some(if object.style.text_color.is_some() {
                        faded(background, f32::from(text_color.a) / 255.0)
                    } else {
                        background
                    }),
                    group_backdrop: object.style.text_color.map(|_| background),
                });
            }
        }
        _ => {}
    }
    out
}

fn marker_primitive(
    marker: thinoda_core::scene::ConnectorMarkerGeometry,
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
    let size = 6.0 + width * 2.0;
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
