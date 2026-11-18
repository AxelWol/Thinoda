//! draw.io connector body shapes (`shape=flexArrow`, `link`, `pipe`,
//! `filledEdge`, `taperedArrow` and `wire`), ported from mxArrowConnector and
//! draw.io's Shapes.js so canvas, export, bounds and hit testing share them.

use super::{ConnectorMarkerGeometry, connector_marker_geometry, trim_path};
use crate::board::{
    CanvasConnector, ConnectorMarkerKind, DEFAULT_CONNECTOR_MARKER_SIZE, EdgeShape, EdgeShapeKind,
};
use crate::{Color, Vec2};

/// Resolved paint geometry of a connector body shape, in sheet coordinates.
#[derive(Debug, Clone, PartialEq)]
pub enum EdgeBody {
    /// The normal stroke painted `casing` wide, then again `inner.0` wide in
    /// the fill colour (pipe, filledEdge, wire).
    Lines {
        casing: f32,
        inner: Option<(f32, Color)>,
    },
    /// Open rails stroked like the connector, replacing stroke and markers
    /// (link).
    Rails(Vec<Vec<Vec2>>),
    /// Closed outline stroked like the connector and optionally filled,
    /// replacing stroke and merged heads (flexArrow, taperedArrow).
    Band {
        outline: Vec<Vec2>,
        fill: Option<Color>,
    },
}

impl EdgeBody {
    /// Width the connector line and its markers are stroked with.
    pub fn stroke_width(&self, width: f32) -> f32 {
        match self {
            Self::Lines { casing, .. } => *casing,
            _ => width,
        }
    }

    /// Whether the body replaces the connector line and legacy arrow heads.
    pub fn replaces_stroke(&self) -> bool {
        !matches!(self, Self::Lines { .. })
    }
}

/// Resolve the body of `connector` along `points`. Band shapes replace the
/// marker geometry: flexArrow and link paint no separate markers, and
/// taperedArrow keeps only the markers it cannot merge into its outline.
pub(super) fn edge_body(
    connector: &CanvasConnector,
    points: &[Vec2],
    markers: &mut [Option<ConnectorMarkerGeometry>; 2],
) -> Option<Box<EdgeBody>> {
    let shape = connector.edge_shape?;
    if connector.sankey.is_some() {
        return None;
    }
    let width = connector.width;
    let body = match shape.kind {
        EdgeShapeKind::Wire => EdgeBody::Lines {
            casing: width,
            inner: shape.fill.map(|fill| (width, fill)),
        },
        EdgeShapeKind::FilledEdge => EdgeBody::Lines {
            casing: width,
            inner: shape
                .fill
                .filter(|_| width >= 3.0)
                .map(|fill| (width - 2.0, fill)),
        },
        EdgeShapeKind::Pipe => {
            let body_width = shape.resolved_width();
            EdgeBody::Lines {
                casing: body_width + 2.0 * width,
                inner: shape.fill.map(|fill| (body_width, fill)),
            }
        }
        EdgeShapeKind::Link => {
            *markers = [None, None];
            let edge_width = shape.resolved_width() + (width - 1.0).max(0.0);
            let sides = arrow_sides(points, edge_width, 0.0, [None, None])?;
            EdgeBody::Rails(vec![sides.0, sides.1.into_iter().rev().collect()])
        }
        EdgeShapeKind::FlexArrow => {
            *markers = [None, None];
            let edge_width = shape.resolved_width() + (width - 1.0).max(0.0);
            let head = |start: bool| {
                let (on, size) = if start {
                    (connector.arrow_start, connector.start_marker_size)
                } else {
                    (connector.arrow_end, connector.end_marker_size)
                };
                on.then(|| ArrowHead {
                    size: size.unwrap_or(DEFAULT_CONNECTOR_MARKER_SIZE) * 3.0 + width,
                    width: edge_width + shape.resolved_end_width(start) + width,
                })
            };
            let (out, inner) =
                arrow_sides(points, edge_width, width / 2.0, [head(true), head(false)])?;
            let mut outline = out;
            outline.extend(inner.into_iter().rev());
            let mut outline = dedup(&outline);
            if outline.len() > 1 && outline.first() == outline.last() {
                outline.pop();
            }
            EdgeBody::Band {
                outline,
                fill: shape.fill,
            }
        }
        EdgeShapeKind::TaperedArrow => tapered_body(connector, shape, points, markers)?,
    };
    if matches!(body, EdgeBody::Lines { casing, inner: None } if casing == width) {
        return None;
    }
    Some(Box::new(body))
}

#[derive(Debug, Clone, Copy)]
struct ArrowHead {
    size: f32,
    width: f32,
}

fn dedup(points: &[Vec2]) -> Vec<Vec2> {
    let mut result: Vec<Vec2> = Vec::with_capacity(points.len());
    for &point in points {
        if result.last() != Some(&point) {
            result.push(point);
        }
    }
    result
}

fn normalized(v: Vec2) -> Option<Vec2> {
    let length = v.length();
    (length > 0.0 && length.is_finite()).then(|| v * (1.0 / length))
}

/// Port of mxArrowConnector's outline without rounded joins: the "out" side
/// from start to end including both caps or heads, and the "in" side from
/// start to end. With `spacing == 0.0` and no heads the sides are the open
/// rails of draw.io's link shape.
fn arrow_sides(
    points: &[Vec2],
    edge_width: f32,
    spacing: f32,
    heads: [Option<ArrowHead>; 2],
) -> Option<(Vec<Vec2>, Vec<Vec2>)> {
    let points = dedup(points);
    if points.len() < 2 {
        return None;
    }
    let mut n = normalized(points[1] - points[0])?;
    let mut n1 = n;
    let orth = Vec2::new(n.y, -n.x) * edge_width;
    let start = points[0];
    let mut out = Vec::new();
    let mut inner = Vec::new();
    match heads[0] {
        Some(head) => {
            let marker = arrow_marker(start, n, head, edge_width, spacing);
            out.extend_from_slice(&marker[2..]);
            inner.extend(marker[..3].iter().rev());
        }
        None => {
            out.push(start + orth * 0.5 + n * spacing);
            inner.push(start - orth * 0.5 + n * spacing);
        }
    }
    for window in points.windows(3) {
        let Some(next) = normalized(window[2] - window[1]) else {
            continue;
        };
        n1 = next;
        let factor = ((n.x * n1.x + n.y * n1.y + 1.0) / 2.0).sqrt().max(0.06);
        if let Some(n2) = normalized(n + n1) {
            let side = Vec2::new(n2.y, -n2.x) * (edge_width / 2.0 / factor);
            out.push(window[1] + side);
            inner.push(window[1] - side);
            n = n1;
        }
    }
    let end = *points.last()?;
    match heads[1] {
        Some(head) => {
            let marker = arrow_marker(end, n * -1.0, head, edge_width, spacing);
            out.extend_from_slice(&marker[..3]);
            inner.extend(marker[2..].iter().rev());
        }
        None => {
            let orth = Vec2::new(n1.y, -n1.x) * edge_width;
            out.push(end - n1 * spacing + orth * 0.5);
            inner.push(end - n1 * spacing - orth * 0.5);
        }
    }
    Some((out, inner))
}

/// mxArrowConnector.paintMarker: five points from the "in" side through the
/// tip to the "out" side, with `n` pointing from the end into the edge.
fn arrow_marker(point: Vec2, n: Vec2, head: ArrowHead, edge_width: f32, spacing: f32) -> [Vec2; 5] {
    let orth = Vec2::new(n.y, -n.x) * (edge_width / 2.0);
    let wide = Vec2::new(n.y, -n.x) * (head.width / 2.0);
    let space = n * (spacing + head.size);
    [
        point - orth + space,
        point - wide + space,
        point + n * spacing,
        point + wide + space,
        point + orth + space,
    ]
}

#[derive(Debug, Clone, Copy)]
struct TaperedHead {
    left: Vec2,
    tip: Vec2,
    right: Vec2,
}

fn tapered_body(
    connector: &CanvasConnector,
    shape: EdgeShape,
    points: &[Vec2],
    markers: &mut [Option<ConnectorMarkerGeometry>; 2],
) -> Option<EdgeBody> {
    let points = dedup(points);
    if points.len() < 2 {
        return None;
    }
    let stroke = connector.width;
    let widths = [
        shape.resolved_end_width(true),
        shape.resolved_end_width(false),
    ];
    let mut heads = [None, None];
    let mut insets = [0.0f32; 2];
    for (index, start) in [true, false].into_iter().enumerate() {
        markers[index] = None;
        let (on, marker, size, unfilled) = if start {
            (
                connector.arrow_start,
                connector.start_marker,
                connector.start_marker_size,
                connector.start_marker_unfilled,
            )
        } else {
            (
                connector.arrow_end,
                connector.end_marker,
                connector.end_marker_size,
                connector.end_marker_unfilled,
            )
        };
        if !on {
            continue;
        }
        let kind = marker.map_or(ConnectorMarkerKind::Classic, |marker| marker.kind);
        let size = size.unwrap_or(DEFAULT_CONNECTOR_MARKER_SIZE);
        let width = widths[index];
        let merged = (!unfilled)
            .then_some(match kind {
                ConnectorMarkerKind::Classic | ConnectorMarkerKind::Block => Some(2.0),
                ConnectorMarkerKind::ClassicThin | ConnectorMarkerKind::BlockThin => Some(3.0),
                _ => None,
            })
            .flatten();
        let (end, toward) = if start {
            (points[0], points[1])
        } else {
            (points[points.len() - 1], points[points.len() - 2])
        };
        let Some(u) = normalized(end - toward) else {
            continue;
        };
        if let Some(factor) = merged {
            let size = size + width * factor / 2.0;
            let offset = u * (stroke * 1.118);
            let unit = u * (size + stroke);
            let tip = end - offset;
            let back = if matches!(
                kind,
                ConnectorMarkerKind::Classic | ConnectorMarkerKind::ClassicThin
            ) {
                0.75
            } else {
                1.0
            };
            insets[index] = (size + stroke) * back + stroke * 1.118;
            let first = Vec2::new(
                tip.x - unit.x - unit.y / factor,
                tip.y - unit.y + unit.x / factor,
            );
            let second = Vec2::new(
                tip.x + unit.y / factor - unit.x,
                tip.y - unit.y - unit.x / factor,
            );
            heads[index] = Some(if start {
                TaperedHead {
                    left: second,
                    tip,
                    right: first,
                }
            } else {
                TaperedHead {
                    left: first,
                    tip,
                    right: second,
                }
            });
        } else {
            let geometry =
                connector_marker_geometry(&points, start, kind, stroke, size + width, !unfilled);
            insets[index] = geometry.map_or(0.0, |marker| marker.line_inset);
            markers[index] = geometry;
        }
    }
    let center = dedup(&trim_path(&points, insets[0], insets[1]));
    if center.len() < 2 {
        return None;
    }
    let mut normals = Vec::with_capacity(center.len() - 1);
    let mut lengths = vec![0.0f32];
    for pair in center.windows(2) {
        let delta = pair[1] - pair[0];
        let length = delta.length();
        normals.push(Vec2::new(-delta.y / length, delta.x / length));
        lengths.push(lengths.last().copied().unwrap_or_default() + length);
    }
    let total = lengths.last().copied().unwrap_or_default();
    if total <= 0.0 {
        return None;
    }
    let mut left = Vec::with_capacity(center.len());
    let mut right = Vec::with_capacity(center.len());
    for (index, &point) in center.iter().enumerate() {
        let half = (widths[0] + (widths[1] - widths[0]) * lengths[index] / total) / 2.0;
        let n0 = normals[index.saturating_sub(1)];
        let n1 = normals[index.min(normals.len() - 1)];
        let sum = n0 + n1;
        let length = sum.length();
        let factor = half / (length / 2.0).max(0.5);
        let direction = if length == 0.0 {
            n1
        } else {
            sum * (1.0 / length)
        };
        left.push(point + direction * factor);
        right.push(point - direction * factor);
    }
    let span = 2.0 * widths[0].max(widths[1]);
    let left = remove_loops(&left, &lengths, span);
    let right = remove_loops(&right, &lengths, span);
    let mut outline = left;
    if let Some(head) = heads[1] {
        outline.extend([head.left, head.tip, head.right]);
    }
    outline.extend(right.into_iter().rev());
    if let Some(head) = heads[0] {
        outline.extend([head.right, head.tip, head.left]);
    }
    Some(EdgeBody::Band {
        outline,
        fill: shape.fill,
    })
}

const MAX_LOOP_POINTS: usize = 1000;

/// TaperedArrowShape.removeLoops: cut local loops of an offset polyline at
/// self-intersections at most `span` apart along the center line.
fn remove_loops(points: &[Vec2], lengths: &[f32], span: f32) -> Vec<Vec2> {
    let n = points.len();
    let mut current = points[0];
    let mut result = vec![current];
    let mut i = 0;
    while i + 1 < n {
        let mut j = i + 2;
        while j + 1 < n && j < i + 2 + MAX_LOOP_POINTS && lengths[j] - lengths[i + 1] <= span {
            j += 1;
        }
        let mut hit = None;
        while j > i + 2 && hit.is_none() {
            j -= 1;
            hit = segment_intersection(current, points[i + 1], points[j], points[j + 1]);
        }
        if let Some(point) = hit {
            current = point;
            i = j + 1;
        } else {
            current = points[i + 1];
            i += 1;
        }
        result.push(current);
    }
    result
}

/// mxUtils.intersection: the crossing of two segments, if any.
fn segment_intersection(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> Option<Vec2> {
    let denominator = (d.y - c.y) * (b.x - a.x) - (d.x - c.x) * (b.y - a.y);
    let ua = ((d.x - c.x) * (a.y - c.y) - (d.y - c.y) * (a.x - c.x)) / denominator;
    let ub = ((b.x - a.x) * (a.y - c.y) - (b.y - a.y) * (a.x - c.x)) / denominator;
    ((0.0..=1.0).contains(&ua) && (0.0..=1.0).contains(&ub)).then(|| a + (b - a) * ua)
}
