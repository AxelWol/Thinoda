//! Paint-order scene of a sheet: zones, canvas objects and laid out topics in bands,
//! with rotated hit testing shared by the canvas, exports and tests. See
//! `docs/FREEFORM.md`.

#[cfg(test)]
use crate::board::shape_geometry;
use crate::board::{
    Band, CanvasConnector, CanvasObject, CanvasObjectId, CanvasObjectKind, ConnectorMarkerKind,
    ConnectorPort, ConnectorRoute, DEFAULT_CONNECTOR_MARKER_SIZE, Endpoint, GridKind, ShapeKind,
    ShapeRounding, distance_to_outline, distance_to_polyline, point_in_polygon, points_bounds,
    rotate_around, rotated_bounds, rotated_rect_contains, shape_geometry_with_rounding,
};
use crate::geometry::{Rect, Vec2};
use crate::layout::{Layout, clip_to_rotated_rect, cubic};
use crate::model::{Sheet, TopicId, ZoneId};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
};

/// Default hit tolerance in sheet units.
pub const HIT_TOLERANCE: f32 = 3.0;
/// Segments used to flatten curved connectors.
const CURVE_SEGMENTS: usize = 16;
/// Segments per quadratic span of an explicitly controlled curve.
const WAYPOINT_CURVE_SEGMENTS: usize = 8;

/// Paint bands, bottom to top. UI overlays are drawn above `AboveMap` and are not part
/// of the scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SceneBand {
    /// Grid; carried by [`Scene::grid`], never an item.
    Background,
    /// Zone frames.
    Frames,
    /// Canvas objects in [`Band::Below`].
    BelowMap,
    /// Laid out topics, including cards.
    Map,
    /// Canvas objects in [`Band::Above`].
    AboveMap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SceneId {
    Zone(ZoneId),
    Topic(TopicId),
    Object(CanvasObjectId),
}

/// Result of [`Scene::hit_test`]. Zones are not hit targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SceneHit {
    Topic(TopicId),
    Object(CanvasObjectId),
}

/// Paint geometry for a non-default connector marker, in sheet coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConnectorMarkerGeometry {
    shape: ConnectorMarkerShape,
    pub closed: bool,
    pub filled: bool,
    pub line_inset: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ConnectorMarkerShape {
    Polygon {
        points: [Vec2; 4],
        point_count: usize,
    },
    Circle(Vec2, f32),
    Curve([Vec2; 5]),
}

fn quadratic_point(points: [Vec2; 3], t: f64) -> [f64; 2] {
    let coordinate = |a: f32, b: f32, c: f32| {
        let a = f64::from(a);
        let b = f64::from(b);
        let c = f64::from(c);
        let first = a + (b - a) * t;
        let second = b + (c - b) * t;
        first + (second - first) * t
    };
    [
        coordinate(points[0].x, points[1].x, points[2].x),
        coordinate(points[0].y, points[1].y, points[2].y),
    ]
}

fn quadratic_bounds(points: [Vec2; 3]) -> Rect {
    let mut min = [
        f64::from(points[0].x.min(points[2].x)),
        f64::from(points[0].y.min(points[2].y)),
    ];
    let mut max = [
        f64::from(points[0].x.max(points[2].x)),
        f64::from(points[0].y.max(points[2].y)),
    ];
    for (a, b, c) in [
        (points[0].x, points[1].x, points[2].x),
        (points[0].y, points[1].y, points[2].y),
    ] {
        let (a, b, c) = (f64::from(a), f64::from(b), f64::from(c));
        let denominator = a - 2.0 * b + c;
        if denominator != 0.0 {
            let t = (a - b) / denominator;
            if t > 0.0 && t < 1.0 {
                let point = quadratic_point(points, t);
                for axis in 0..2 {
                    min[axis] = min[axis].min(point[axis]);
                    max[axis] = max[axis].max(point[axis]);
                }
            }
        }
    }
    // Keep the f32 rectangle conservative when extrema are rounded at large coordinates.
    let magnitude = min
        .into_iter()
        .chain(max)
        .map(f64::abs)
        .fold(1.0_f64, f64::max) as f32;
    let padding = magnitude * f32::EPSILON * 2.0;
    let min = Vec2::new(min[0] as f32 - padding, min[1] as f32 - padding);
    let max = Vec2::new(max[0] as f32 + padding, max[1] as f32 + padding);
    Rect::from_min_size(min, max - min)
}

fn point_near_curve_path(point: Vec2, path: &[Vec2], radius: f64) -> bool {
    if radius < 0.0 {
        return false;
    }
    let point = [f64::from(point.x), f64::from(point.y)];
    path.windows(2).any(|segment| {
        let a = [f64::from(segment[0].x), f64::from(segment[0].y)];
        let delta = [
            f64::from(segment[1].x) - a[0],
            f64::from(segment[1].y) - a[1],
        ];
        let length_squared = delta[0] * delta[0] + delta[1] * delta[1];
        let t = if length_squared == 0.0 {
            0.0
        } else {
            (((point[0] - a[0]) * delta[0] + (point[1] - a[1]) * delta[1]) / length_squared)
                .clamp(0.0, 1.0)
        };
        let dx = point[0] - a[0] - delta[0] * t;
        let dy = point[1] - a[1] - delta[1] * t;
        dx * dx + dy * dy <= radius * radius
    })
}

impl ConnectorMarkerGeometry {
    /// The polygonal marker outline, including the notch of a classic arrow.
    pub fn points(&self) -> &[Vec2] {
        match &self.shape {
            ConnectorMarkerShape::Polygon {
                points,
                point_count,
            } => &points[..*point_count],
            _ => &[],
        }
    }

    /// The center and radius of a circular marker, when present.
    pub fn circle(&self) -> Option<(Vec2, f32)> {
        match self.shape {
            ConnectorMarkerShape::Circle(center, radius) => Some((center, radius)),
            _ => None,
        }
    }

    /// Two connected quadratic spans: start, control, join, control, end.
    pub fn curve(&self) -> Option<[Vec2; 5]> {
        match self.shape {
            ConnectorMarkerShape::Curve(points) => Some(points),
            _ => None,
        }
    }

    /// Flatten a marker curve for conservative stroke hit testing.
    pub fn flattened_curve(&self, width: f32) -> Option<(Vec<Vec2>, f32)> {
        let points = self.curve()?;
        let spans = [
            [points[0], points[1], points[2]],
            [points[2], points[3], points[4]],
        ];
        let curvature = spans
            .iter()
            .map(|span| {
                let second_difference =
                    |a: f32, b: f32, c: f32| f64::from(a) - 2.0 * f64::from(b) + f64::from(c);
                second_difference(span[0].x, span[1].x, span[2].x)
                    .hypot(second_difference(span[0].y, span[1].y, span[2].y))
            })
            .fold(0.0_f64, f64::max);
        let target = (f64::from(width) * 0.01).clamp(0.001, 0.05);
        let segments = ((curvature / (4.0 * target)).sqrt().ceil() as usize).clamp(1, 512);
        // Include vertex quantization in the hit-test error, not just curve subdivision.
        let mut rounding_error_squared = 0.0_f64;
        let mut path = Vec::with_capacity(2 * segments + 1);
        path.push(points[0]);
        for span in spans {
            for index in 1..=segments {
                let point = quadratic_point(span, index as f64 / segments as f64);
                let rounded = Vec2::new(point[0] as f32, point[1] as f32);
                let dx = point[0] - f64::from(rounded.x);
                let dy = point[1] - f64::from(rounded.y);
                rounding_error_squared = rounding_error_squared.max(dx * dx + dy * dy);
                path.push(rounded);
            }
        }
        let error = (curvature / (4.0 * (segments * segments) as f64)
            + rounding_error_squared.sqrt()) as f32;
        let error = error.next_up();
        Some((path, error))
    }
}

fn connector_marker_geometry(
    points: &[Vec2],
    at_start: bool,
    kind: ConnectorMarkerKind,
    width: f32,
    marker_size: f32,
    fill: bool,
) -> Option<ConnectorMarkerGeometry> {
    let tip = if at_start {
        *points.first()?
    } else {
        *points.last()?
    };
    let toward = if at_start {
        points
            .iter()
            .find(|point| (**point - tip).length() > f32::EPSILON)
    } else {
        points
            .iter()
            .rev()
            .find(|point| (**point - tip).length() > f32::EPSILON)
    }?;
    let delta = tip - *toward;
    let direction = delta * (1.0 / delta.length());
    let normal = Vec2::new(-direction.y, direction.x);
    if kind == ConnectorMarkerKind::Oval {
        let radius = marker_size * 0.5;
        return Some(ConnectorMarkerGeometry {
            shape: ConnectorMarkerShape::Circle(tip, radius),
            closed: true,
            filled: fill,
            line_inset: radius,
        });
    }
    if kind == ConnectorMarkerKind::HalfCircle {
        let radius = marker_size + width + 1.0;
        let apex = tip - direction * radius;
        let side = normal * radius;
        return Some(ConnectorMarkerGeometry {
            shape: ConnectorMarkerShape::Curve([
                tip + side,
                apex + side,
                apex,
                apex - side,
                tip - side,
            ]),
            closed: false,
            filled: false,
            line_inset: radius,
        });
    }
    if kind.is_bar() {
        let length = marker_size + width + 1.0;
        let side = normal * (length * 0.5);
        let (first, last) = if kind == ConnectorMarkerKind::BaseDash {
            (tip + side, tip - side)
        } else {
            (
                tip - direction * (length * 0.5) + side,
                tip - direction * (length * 1.5) - side,
            )
        };
        return Some(ConnectorMarkerGeometry {
            shape: ConnectorMarkerShape::Polygon {
                points: [first, last, last, last],
                point_count: 2,
            },
            closed: false,
            filled: false,
            line_inset: 0.0,
        });
    }
    // Keep draw.io's rounded coefficients for stroke-width-dependent tip offsets.
    let offset = width
        * match kind {
            ConnectorMarkerKind::Diamond => 7071.0 / 10_000.0,
            ConnectorMarkerKind::DiamondThin => 0.9862,
            _ => 1.118,
        };
    let size = marker_size + width;
    let tip = tip - direction * offset;
    let base = tip - direction * size;
    if kind.is_diamond() {
        let midpoint = tip - direction * (size * 0.5);
        let wing = size
            / if kind == ConnectorMarkerKind::Diamond {
                2.0
            } else {
                3.4
            };
        return Some(ConnectorMarkerGeometry {
            shape: ConnectorMarkerShape::Polygon {
                points: [
                    midpoint + normal * wing,
                    tip,
                    midpoint - normal * wing,
                    base,
                ],
                point_count: 4,
            },
            closed: true,
            filled: fill,
            line_inset: offset + size,
        });
    }
    let classic = matches!(
        kind,
        ConnectorMarkerKind::Classic | ConnectorMarkerKind::ClassicThin
    );
    let thin = matches!(
        kind,
        ConnectorMarkerKind::ClassicThin
            | ConnectorMarkerKind::OpenThin
            | ConnectorMarkerKind::BlockThin
    );
    let closed = !matches!(
        kind,
        ConnectorMarkerKind::Open | ConnectorMarkerKind::OpenThin
    );
    let wing = size / if thin { 3.0 } else { 2.0 };
    Some(ConnectorMarkerGeometry {
        shape: ConnectorMarkerShape::Polygon {
            points: [
                base + normal * wing,
                tip,
                base - normal * wing,
                tip - direction * (size * 0.75),
            ],
            point_count: if classic { 4 } else { 3 },
        },
        closed,
        filled: closed && fill,
        line_inset: if classic {
            size * 0.75 + offset
        } else if closed {
            size + offset
        } else {
            offset * 2.0
        },
    })
}

/// Trim a connector stroke to the inside of its non-default markers.
/// If the markers overlap on a short path, the remaining stroke collapses to a point.
pub fn connector_stroke_path(
    points: &[Vec2],
    markers: &[Option<ConnectorMarkerGeometry>; 2],
) -> Vec<Vec2> {
    if markers.iter().all(Option::is_none) || points.len() < 2 {
        return points.to_vec();
    }
    let total: f32 = points
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).length())
        .sum();
    if total <= f32::EPSILON {
        return points.to_vec();
    }
    let mut start = markers[0].map_or(0.0, |marker| marker.line_inset);
    let mut end = markers[1].map_or(0.0, |marker| marker.line_inset);
    if start + end > total {
        let scale = total / (start + end);
        start *= scale;
        end *= scale;
    }
    let stop = (total - end).max(start).min(total);
    let mut result = Vec::new();
    let mut offset = 0.0;
    for pair in points.windows(2) {
        let delta = pair[1] - pair[0];
        let length = delta.length();
        if length <= f32::EPSILON {
            continue;
        }
        let segment_end = offset + length;
        if segment_end >= start && offset <= stop {
            let first = ((start - offset) / length).clamp(0.0, 1.0);
            let last = ((stop - offset) / length).clamp(0.0, 1.0);
            let a = pair[0] + delta * first;
            let b = pair[0] + delta * last;
            if result.last() != Some(&a) {
                result.push(a);
            }
            if result.last() != Some(&b) {
                result.push(b);
            }
        }
        offset = segment_end;
        if offset > stop {
            break;
        }
    }
    if result.len() == 1 {
        result.push(result[0]);
    }
    result
}

#[derive(Debug, Clone, PartialEq)]
pub enum SceneGeometry {
    /// Unrotated rect turned by `rotation` degrees around its centre.
    Rect { rect: Rect, rotation: f32 },
    /// Absolute (already rotated) shape outline and detail strokes.
    Polygon {
        outline: Vec<Vec2>,
        details: Vec<Vec<Vec2>>,
    },
    /// Ink stroke; `points` are unrotated and turned by `rotation` around `center`.
    Ink {
        points: Arc<[Vec2]>,
        width: f32,
        center: Vec2,
        rotation: f32,
    },
    /// Free connector path.
    Polyline {
        points: Vec<Vec2>,
        width: f32,
        markers: [Option<ConnectorMarkerGeometry>; 2],
    },
}

impl SceneGeometry {
    fn hit(&self, p: Vec2, tolerance: f32) -> bool {
        match self {
            SceneGeometry::Rect { rect, rotation } => {
                rotated_rect_contains(rect.expand(tolerance), *rotation, p)
            }
            SceneGeometry::Polygon { outline, .. } => {
                point_in_polygon(p, outline) || distance_to_outline(p, outline) <= tolerance
            }
            SceneGeometry::Ink {
                points,
                width,
                center,
                rotation,
            } => {
                let local = rotate_around(p, *center, -*rotation);
                distance_to_polyline(local, points) <= width / 2.0 + tolerance
            }
            SceneGeometry::Polyline {
                points,
                width,
                markers,
            } => {
                let radius = width / 2.0 + tolerance;
                let stroke_hit = if markers.iter().any(Option::is_some) {
                    distance_to_polyline(p, &connector_stroke_path(points, markers)) <= radius
                } else {
                    distance_to_polyline(p, points) <= radius
                };
                stroke_hit
                    || markers.iter().flatten().any(|marker| {
                        if let Some((center, marker_radius)) = marker.circle() {
                            let distance = (p - center).length();
                            marker_radius > 0.0
                                && ((marker.filled && distance <= marker_radius + radius)
                                    || (distance - marker_radius).abs() <= radius)
                        } else if let Some((path, error)) = marker.flattened_curve(*width) {
                            point_near_curve_path(
                                p,
                                &path,
                                f64::from(*width) / 2.0 + f64::from(tolerance) + f64::from(error),
                            )
                        } else {
                            (marker.filled && point_in_polygon(p, marker.points()))
                                || if marker.closed {
                                    distance_to_outline(p, marker.points()) <= radius
                                } else {
                                    distance_to_polyline(p, marker.points()) <= radius
                                }
                        }
                    })
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneItem {
    pub id: SceneId,
    pub band: SceneBand,
    /// Axis-aligned bounds of the painted geometry.
    pub bounds: Rect,
    /// Optional frame clipping in sheet coordinates.
    pub clip: Option<Rect>,
    pub geometry: SceneGeometry,
}

impl SceneItem {
    /// Bounds remaining after frame clipping.
    pub fn visible_bounds(&self) -> Option<Rect> {
        let Some(clip) = self.clip else {
            return Some(self.bounds);
        };
        if !self.bounds.intersects(&clip) {
            return None;
        }
        let max = self.bounds.max();
        let clip_max = clip.max();
        let min = Vec2::new(
            self.bounds.min.x.max(clip.min.x),
            self.bounds.min.y.max(clip.min.y),
        );
        let max = Vec2::new(max.x.min(clip_max.x), max.y.min(clip_max.y));
        Some(Rect::from_min_size(min, max - min))
    }
}

/// Items in paint order (bottom first).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scene {
    /// Background grid, if shown.
    pub grid: Option<(GridKind, f32)>,
    pub items: Vec<SceneItem>,
}

/// Builds the paint-order scene of `sheet` from its layout.
pub fn build_scene(sheet: &Sheet, layout: &Layout) -> Scene {
    let mut items =
        Vec::with_capacity(sheet.zones.len() + layout.order.len() + sheet.canvas.objects.len());
    let mut connectors = ConnectorResolver::new(sheet, layout);
    for zone in &sheet.zones {
        items.push(SceneItem {
            id: SceneId::Zone(zone.id),
            band: SceneBand::Frames,
            bounds: zone.frame,
            clip: None,
            geometry: SceneGeometry::Rect {
                rect: zone.frame,
                rotation: 0.0,
            },
        });
    }
    let mut objects = |band: Band, items: &mut Vec<SceneItem>| {
        for object in sheet.canvas.objects.iter().filter(|o| o.band == band) {
            if let Some(item) = object_item(sheet, object, &mut connectors) {
                items.push(item);
            }
        }
    };
    objects(Band::Below, &mut items);
    for id in &layout.order {
        let Some(t) = layout.topics.get(id) else {
            continue;
        };
        let rotation = sheet
            .topic(*id)
            .and_then(|t| t.card.as_ref())
            .map_or(0.0, |c| c.rotation);
        items.push(SceneItem {
            id: SceneId::Topic(*id),
            band: SceneBand::Map,
            bounds: rotated_bounds(t.rect, rotation),
            clip: topic_clip_rect(sheet, *id),
            geometry: SceneGeometry::Rect {
                rect: t.rect,
                rotation,
            },
        });
    }
    objects(Band::Above, &mut items);
    let grid =
        (sheet.board.grid != GridKind::None).then_some((sheet.board.grid, sheet.board.grid_size));
    Scene { grid, items }
}

fn object_item(
    sheet: &Sheet,
    object: &CanvasObject,
    connectors: &mut ConnectorResolver<'_>,
) -> Option<SceneItem> {
    let band = match object.band {
        Band::Below => SceneBand::BelowMap,
        Band::Above => SceneBand::AboveMap,
    };
    let geometry = match &object.kind {
        CanvasObjectKind::Shape { shape, .. } => {
            let g = crate::board::shape_geometry_with_style(
                *shape,
                object.rect,
                object.rotation,
                object.rounding,
                object.style.effective_shape_stroke_width(),
                object.double_ellipse_margin,
            );
            SceneGeometry::Polygon {
                outline: g.outline,
                details: g.details,
            }
        }
        CanvasObjectKind::Ink(stroke) => SceneGeometry::Ink {
            points: stroke.points.clone(),
            width: stroke.width,
            center: object.rect.center(),
            rotation: object.rotation,
        },
        CanvasObjectKind::Connector(c) => {
            let points = connectors.path(object.id, c)?;
            if points.len() < 2 {
                return None;
            }
            SceneGeometry::Polyline {
                markers: [
                    c.start_marker.filter(|_| c.arrow_start).and_then(|marker| {
                        connector_marker_geometry(
                            &points,
                            true,
                            marker.kind,
                            c.width,
                            c.start_marker_size.unwrap_or(DEFAULT_CONNECTOR_MARKER_SIZE),
                            !c.start_marker_unfilled,
                        )
                    }),
                    c.end_marker.filter(|_| c.arrow_end).and_then(|marker| {
                        connector_marker_geometry(
                            &points,
                            false,
                            marker.kind,
                            c.width,
                            c.end_marker_size.unwrap_or(DEFAULT_CONNECTOR_MARKER_SIZE),
                            !c.end_marker_unfilled,
                        )
                    }),
                ],
                points,
                width: c.width,
            }
        }
    };
    let mut bounds = geometry_bounds(&geometry)?;
    if let Some(sides) = object.rectangle_sides
        && (sides.top || sides.right || sides.bottom || sides.left)
        && object.style.opacity > 0.0
    {
        let stroke_width = match object.style.stroke {
            Some(color) if color.a > 0 => Some(object.style.stroke_width),
            None if object.style.fill.is_none() => Some(2.0),
            _ => None,
        };
        if let Some(width) = stroke_width {
            bounds = bounds.expand(width * std::f32::consts::FRAC_1_SQRT_2);
        }
    }
    let clip = match &object.kind {
        CanvasObjectKind::Connector(_) => object_clip_at(sheet, object.id, bounds.center()),
        _ => object_clip_rect(sheet, object),
    };
    Some(SceneItem {
        id: SceneId::Object(object.id),
        band,
        bounds,
        clip,
        geometry,
    })
}

/// Clipping owner inherited from the frame containing a floating topic tree.
pub fn topic_clip_zone(sheet: &Sheet, topic: TopicId) -> Option<(ZoneId, Rect)> {
    if !sheet.zones.iter().any(|zone| zone.clip) {
        return None;
    }
    let mut current = Some(topic);
    let mut seen = HashSet::new();
    while let Some(id) = current {
        if !seen.insert(id) {
            return None;
        }
        if let Some(zone) = sheet
            .zones
            .iter()
            .find(|zone| zone.clip && zone.roots.contains(&id))
        {
            return Some((zone.id, zone.frame));
        }
        current = sheet
            .topic(id)
            .and_then(|topic| topic.parent)
            .or_else(|| sheet.summary_for_topic(id).map(|summary| summary.parent));
    }
    None
}

/// Rectangular clipping inherited from the frame containing a floating topic tree.
pub fn topic_clip_rect(sheet: &Sheet, topic: TopicId) -> Option<Rect> {
    topic_clip_zone(sheet, topic).map(|(_, rect)| rect)
}

/// Explicit ownership controls clipping, falling back to spatial containment for
/// unassigned objects.
/// Connector paths are resolved by [`build_scene`], which clips using their actual
/// rendered bounds instead of their potentially stale stored rectangle.
pub fn object_clip_rect(sheet: &Sheet, object: &CanvasObject) -> Option<Rect> {
    object_clip_at(sheet, object.id, object.rect.center())
}

pub fn object_clip_at(sheet: &Sheet, object: CanvasObjectId, center: Vec2) -> Option<Rect> {
    if let Some(zone) = sheet
        .zones
        .iter()
        .find(|zone| zone.objects.contains(&object))
    {
        return zone.clip.then_some(zone.frame);
    }
    sheet
        .zones
        .iter()
        .find(|zone| zone.clip && zone.frame.contains(center))
        .map(|zone| zone.frame)
}

fn geometry_bounds(geometry: &SceneGeometry) -> Option<Rect> {
    match geometry {
        SceneGeometry::Rect { rect, rotation } => Some(rotated_bounds(*rect, *rotation)),
        SceneGeometry::Polygon { outline, details } => {
            let mut r = points_bounds(outline)?;
            for d in details {
                if let Some(b) = points_bounds(d) {
                    r = r.union(&b);
                }
            }
            Some(r)
        }
        SceneGeometry::Ink {
            points,
            width,
            center,
            rotation,
        } => {
            let r = points_bounds(points)?.expand(width / 2.0);
            let corners =
                crate::board::rotated_corners(r, 0.0).map(|c| rotate_around(c, *center, *rotation));
            points_bounds(&corners)
        }
        SceneGeometry::Polyline {
            points,
            width,
            markers,
        } => {
            let mut bounds = points_bounds(points)?.expand(width / 2.0);
            for marker in markers.iter().flatten() {
                let marker_bounds = if let Some((center, radius)) = marker.circle() {
                    (radius > 0.0).then(|| {
                        Rect::from_center_size(center, Vec2::new(2.0 * radius, 2.0 * radius))
                    })
                } else if let Some(curve) = marker.curve() {
                    Some(
                        quadratic_bounds([curve[0], curve[1], curve[2]])
                            .union(&quadratic_bounds([curve[2], curve[3], curve[4]])),
                    )
                } else {
                    points_bounds(marker.points())
                };
                if let Some(marker_bounds) = marker_bounds {
                    bounds = bounds.union(&marker_bounds.expand(width / 2.0));
                }
            }
            Some(bounds)
        }
    }
}

/// Painted bounds of a canvas object (`None` for an unresolvable connector).
pub fn object_bounds(sheet: &Sheet, layout: &Layout, object: &CanvasObject) -> Option<Rect> {
    object_item(sheet, object, &mut ConnectorResolver::new(sheet, layout))
        .and_then(|item| item.visible_bounds())
}

/// Visible bounds of the whole canvas layer, resolving shared connector dependencies once.
pub fn canvas_bounds(sheet: &Sheet, layout: &Layout) -> Option<Rect> {
    let mut connectors = ConnectorResolver::new(sheet, layout);
    sheet
        .canvas
        .objects
        .iter()
        .filter_map(|object| object_item(sheet, object, &mut connectors)?.visible_bounds())
        .reduce(|a, b| a.union(&b))
}

pub(crate) fn resolved_object_centers(
    sheet: &Sheet,
    layout: &Layout,
    ids: &HashSet<CanvasObjectId>,
) -> HashMap<CanvasObjectId, Vec2> {
    let mut connectors = ConnectorResolver::new(sheet, layout);
    sheet
        .canvas
        .objects
        .iter()
        .filter(|object| ids.contains(&object.id))
        .filter_map(|object| {
            let center = match &object.kind {
                CanvasObjectKind::Connector(connector) => {
                    crate::board::polyline_midpoint(&connectors.path(object.id, connector)?)?
                }
                _ => object.rect.center(),
            };
            Some((object.id, center))
        })
        .collect()
}

pub(crate) fn resolved_connector_endpoints(
    sheet: &Sheet,
    layout: &Layout,
    ids: &HashSet<CanvasObjectId>,
) -> HashMap<CanvasObjectId, (Vec2, Vec2)> {
    let mut connectors = ConnectorResolver::new(sheet, layout);
    sheet
        .canvas
        .objects
        .iter()
        .filter(|object| ids.contains(&object.id))
        .filter_map(|object| {
            let CanvasObjectKind::Connector(connector) = &object.kind else {
                return None;
            };
            let path = connectors.path(object.id, connector)?;
            Some((object.id, (*path.first()?, *path.last()?)))
        })
        .collect()
}

impl Scene {
    /// Top-most topic or object under `p`.
    pub fn hit_test(&self, p: Vec2) -> Option<SceneHit> {
        self.hit_test_tolerance(p, HIT_TOLERANCE)
    }

    /// Top-most topic or object within `tolerance` of `p`.
    pub fn hit_test_tolerance(&self, p: Vec2, tolerance: f32) -> Option<SceneHit> {
        self.hit_test_tolerance_excluding(p, tolerance, None)
    }

    /// Top-most hit within `tolerance`, excluding an item being manipulated.
    pub fn hit_test_tolerance_excluding(
        &self,
        p: Vec2,
        tolerance: f32,
        excluded: Option<SceneId>,
    ) -> Option<SceneHit> {
        self.items.iter().rev().find_map(|item| {
            if excluded == Some(item.id) {
                return None;
            }
            let hit = match item.id {
                SceneId::Zone(_) => return None,
                SceneId::Topic(id) => SceneHit::Topic(id),
                SceneId::Object(id) => SceneHit::Object(id),
            };
            (item.clip.is_none_or(|clip| clip.contains(p))
                && item.bounds.expand(tolerance).contains(p)
                && item.geometry.hit(p, tolerance))
            .then_some(hit)
        })
    }

    /// Topics and objects whose bounds intersect `area`, in paint order.
    pub fn objects_in(&self, area: Rect) -> Vec<SceneHit> {
        self.items
            .iter()
            .filter(|item| {
                item.visible_bounds()
                    .is_some_and(|bounds| bounds.intersects(&area))
            })
            .filter_map(|item| match item.id {
                SceneId::Zone(_) => None,
                SceneId::Topic(id) => Some(SceneHit::Topic(id)),
                SceneId::Object(id) => Some(SceneHit::Object(id)),
            })
            .collect()
    }

    /// Union of all item bounds.
    pub fn bounds(&self) -> Option<Rect> {
        self.items
            .iter()
            .filter_map(SceneItem::visible_bounds)
            .reduce(|a, b| a.union(&b))
    }

    pub fn item(&self, id: SceneId) -> Option<&SceneItem> {
        self.items.iter().find(|item| item.id == id)
    }
}

/// Rect of `id` or of its nearest laid out ancestor (inside a collapsed branch).
/// `None` for the hidden anchor and unknown topics.
pub fn topic_anchor_rect(sheet: &Sheet, layout: &Layout, id: TopicId) -> Option<Rect> {
    topic_anchor(sheet, layout, id).map(|(_, rect)| rect)
}

fn topic_anchor(sheet: &Sheet, layout: &Layout, id: TopicId) -> Option<(TopicId, Rect)> {
    let mut current = Some(id);
    while let Some(t) = current {
        if sheet.is_anchor(t) {
            return None;
        }
        if let Some(r) = layout.rect(t) {
            return Some((t, r));
        }
        current = sheet.topic(t)?.parent;
    }
    None
}

/// What a connector end attaches to: a (rotated) box and, for shapes, the outline
/// the end lands on.
#[derive(Clone, Copy)]
struct EndTarget {
    rect: Rect,
    rotation: f32,
    shape: Option<(ShapeKind, Option<ShapeRounding>)>,
}

impl EndTarget {
    fn boxed(rect: Rect, rotation: f32) -> Self {
        Self {
            rect,
            rotation,
            shape: None,
        }
    }

    fn with_port(self, port: Option<ConnectorPort>) -> Self {
        let Some(port) = port else {
            return self;
        };
        let point = Vec2::new(
            self.rect.min.x + self.rect.size.x * port.position.x + port.offset.x,
            self.rect.min.y + self.rect.size.y * port.position.y + port.offset.y,
        );
        let point = if port.perimeter {
            if let Some(mut projected) =
                fixed_port_perimeter(self.rect, self.shape.map(|(kind, _)| kind), point)
            {
                if let Some((_, Some(rounding))) = self.shape
                    && rounding.is_standard()
                    && rounding.rounded_perimeter
                    && let Some((corner, radius)) =
                        rounded_perimeter_corner(self.rect, rounding, projected)
                {
                    let delta = projected - corner;
                    let length = delta.x.hypot(delta.y);
                    if length > 0.0 {
                        projected = corner + delta * (radius / length);
                    }
                }
                rotate_around(projected, self.rect.center(), self.rotation)
            } else {
                self.clip(rotate_around(point, self.rect.center(), self.rotation))
            }
        } else {
            rotate_around(point, self.rect.center(), self.rotation)
        };
        Self::boxed(Rect::from_center_size(point, Vec2::ZERO), 0.0)
    }

    /// Point on the visible perimeter in the direction of `toward`, like draw.io's
    /// shape perimeters; boxes and rectangular shapes clip to their rotated rect.
    fn clip(self, toward: Vec2) -> Vec2 {
        if let Some((_, Some(rounding))) = self.shape
            && rounding.is_standard()
        {
            return clip_to_drawio_rounded_rect(self.rect, self.rotation, rounding, toward);
        }
        let rectangular = |kind| {
            matches!(
                kind,
                ShapeKind::Rectangle | ShapeKind::Process | ShapeKind::PredefinedProcess
            )
        };
        self.shape
            .filter(|(kind, rounding)| {
                !rectangular(*kind) || rounding.is_some_and(|rounding| rounding.radius > 0.0)
            })
            .and_then(|(kind, rounding)| {
                let outline =
                    shape_geometry_with_rounding(kind, self.rect, self.rotation, rounding).outline;
                // Shapes like the right triangle have the box centre on their edge;
                // cast from the area centroid there so the ray still leaves the inside.
                let center = self.rect.center();
                let origin = if point_in_polygon(center, &outline)
                    && distance_to_outline(center, &outline) > 0.5
                {
                    center
                } else {
                    polygon_centroid(&outline)
                };
                clip_to_polygon(&outline, origin, toward)
            })
            .unwrap_or_else(|| clip_to_rotated_rect(self.rect, self.rotation, toward))
    }
}

fn rounded_perimeter_corner(
    rect: Rect,
    rounding: ShapeRounding,
    point: Vec2,
) -> Option<(Vec2, f32)> {
    let radius = rounding
        .radius_for_rect(rect)
        .min(rect.size.x / 2.0)
        .min(rect.size.y / 2.0);
    if radius <= 0.0 {
        return None;
    }
    let min = rect.min + Vec2::new(radius, radius);
    let max = rect.max() - Vec2::new(radius, radius);
    if (point.x < min.x || point.x > max.x) && (point.y < min.y || point.y > max.y) {
        Some((
            Vec2::new(
                if point.x < min.x { min.x } else { max.x },
                if point.y < min.y { min.y } else { max.y },
            ),
            radius,
        ))
    } else {
        None
    }
}

fn clip_to_drawio_rounded_rect(
    rect: Rect,
    rotation: f32,
    rounding: ShapeRounding,
    toward: Vec2,
) -> Vec2 {
    let center = rect.center();
    let local_toward = rotate_around(toward, center, -rotation);
    let box_point = fixed_port_perimeter(rect, None, local_toward)
        .unwrap_or_else(|| clip_to_rotated_rect(rect, 0.0, local_toward));
    if !rounding.rounded_perimeter {
        return rotate_around(box_point, center, rotation);
    }
    let Some((corner, radius)) = rounded_perimeter_corner(rect, rounding, box_point) else {
        return rotate_around(box_point, center, rotation);
    };
    let dx = f64::from(box_point.x) - f64::from(center.x);
    let dy = f64::from(box_point.y) - f64::from(center.y);
    let length = dx.hypot(dy);
    if length == 0.0 {
        return rotate_around(box_point, center, rotation);
    }
    let (dx, dy) = (dx / length, dy / length);
    let ox = f64::from(center.x) - f64::from(corner.x);
    let oy = f64::from(center.y) - f64::from(corner.y);
    // Perpendicular distance avoids cancellation in the quadratic discriminant.
    let perpendicular = ox * dy - oy * dx;
    let distance = -(ox * dx + oy * dy)
        + (f64::from(radius).powi(2) - perpendicular * perpendicular)
            .max(0.0)
            .sqrt();
    let intersection = Vec2::new(
        (f64::from(center.x) + dx * distance) as f32,
        (f64::from(center.y) + dy * distance) as f32,
    );
    rotate_around(intersection, center, rotation)
}

/// Resolves a fixed connector port with the same geometry used by scene routing.
pub fn fixed_connector_port_position(
    rect: Rect,
    rotation: f32,
    shape: Option<(ShapeKind, Option<ShapeRounding>)>,
    port: ConnectorPort,
) -> Vec2 {
    EndTarget {
        rect,
        rotation,
        shape,
    }
    .with_port(Some(port))
    .rect
    .center()
}

/// draw.io applies a constraint to the unrotated perimeter before rotating it.
fn fixed_port_perimeter(rect: Rect, shape: Option<ShapeKind>, point: Vec2) -> Option<Vec2> {
    let half = rect.size * 0.5;
    if half.x <= 0.0 || half.y <= 0.0 {
        return None;
    }
    let center = rect.center();
    let delta = point - center;
    match shape {
        None
        | Some(
            ShapeKind::Rectangle
            | ShapeKind::RoundedRectangle
            | ShapeKind::Process
            | ShapeKind::PredefinedProcess,
        ) => {
            if delta == Vec2::ZERO {
                return Some(Vec2::new(rect.max().x, center.y));
            }
            let x = f64::from(delta.x);
            let y = f64::from(delta.y);
            let scale = (f64::from(half.x) / x.abs()).min(f64::from(half.y) / y.abs());
            Some(Vec2::new(
                center.x + (x * scale) as f32,
                center.y + (y * scale) as f32,
            ))
        }
        Some(ShapeKind::Ellipse | ShapeKind::DoubleEllipse) => {
            // The published ellipse perimeter truncates center-relative distances.
            let x = delta.x.trunc();
            let y = delta.y.trunc();
            if x == 0.0 && y == 0.0 {
                return Some(point);
            }
            let scale = half.x * half.y / (half.y * x).hypot(half.x * y);
            Some(center + Vec2::new(x, y) * scale)
        }
        Some(ShapeKind::Diamond) => {
            if delta.x == 0.0 {
                return Some(Vec2::new(
                    center.x,
                    if delta.y < 0.0 {
                        rect.min.y
                    } else {
                        rect.max().y
                    },
                ));
            }
            if delta.y == 0.0 {
                return Some(Vec2::new(
                    if delta.x < 0.0 {
                        rect.min.x
                    } else {
                        rect.max().x
                    },
                    center.y,
                ));
            }
            let distance = delta.x.abs() / half.x + delta.y.abs() / half.y;
            Some(if distance < 1.0 {
                center
            } else {
                center + delta * (1.0 / distance)
            })
        }
        _ => None,
    }
}

/// Area centroid of a closed polygon; the vertex mean for degenerate outlines.
fn polygon_centroid(outline: &[Vec2]) -> Vec2 {
    let (mut area, mut sum) = (0.0, Vec2::ZERO);
    for (i, &a) in outline.iter().enumerate() {
        let b = outline[(i + 1) % outline.len()];
        let cross = a.x * b.y - b.x * a.y;
        area += cross;
        sum = sum + (a + b) * cross;
    }
    if area.abs() > 1e-6 {
        sum * (1.0 / (3.0 * area))
    } else {
        outline.iter().fold(Vec2::ZERO, |acc, &p| acc + p) * (1.0 / outline.len().max(1) as f32)
    }
}

/// Outermost crossing of the ray from `center` towards `toward` with the closed
/// `outline`, so concave outlines (stars, clouds) end on their outer edge.
fn clip_to_polygon(outline: &[Vec2], center: Vec2, toward: Vec2) -> Option<Vec2> {
    let cross = |a: Vec2, b: Vec2| a.x * b.y - a.y * b.x;
    let direction = toward - center;
    if direction.length() < 1e-3 {
        return None;
    }
    let mut best: Option<f32> = None;
    for (i, &a) in outline.iter().enumerate() {
        let edge = outline[(i + 1) % outline.len()] - a;
        let denominator = cross(direction, edge);
        if denominator.abs() < 1e-9 {
            continue;
        }
        let offset = a - center;
        let t = cross(offset, edge) / denominator;
        let u = cross(offset, direction) / denominator;
        if t > 1e-6 && (-1e-6..=1.0 + 1e-6).contains(&u) {
            best = Some(best.map_or(t, |best: f32| best.max(t)));
        }
    }
    best.map(|t| center + direction * t)
}

fn routed_path(connector: &CanvasConnector, a: EndTarget, b: EndTarget) -> Vec<Vec2> {
    if !connector.waypoints.is_empty() {
        return routed_waypoints(connector, a, b);
    }
    let (ca, cb) = (a.rect.center(), b.rect.center());
    match connector.route {
        ConnectorRoute::Straight => vec![a.clip(cb), b.clip(ca)],
        ConnectorRoute::Elbow => {
            let a = a.clip(Vec2::new(cb.x, ca.y));
            let b = b.clip(Vec2::new(ca.x, cb.y));
            let mx = (a.x + b.x) / 2.0;
            vec![a, Vec2::new(mx, a.y), Vec2::new(mx, b.y), b]
        }
        ConnectorRoute::Curve => {
            let a = a.clip(cb);
            let b = b.clip(ca);
            let dx = (b.x - a.x) / 2.0;
            let (c1, c2) = (a + Vec2::new(dx, 0.0), b - Vec2::new(dx, 0.0));
            (0..=CURVE_SEGMENTS)
                .map(|i| cubic(a, c1, c2, b, i as f32 / CURVE_SEGMENTS as f32))
                .collect()
        }
    }
}

fn push_distinct(points: &mut Vec<Vec2>, point: Vec2) {
    if points.last() != Some(&point) {
        points.push(point);
    }
}

fn quadratic(start: Vec2, control: Vec2, end: Vec2, t: f32) -> Vec2 {
    start * ((1.0 - t) * (1.0 - t)) + control * (2.0 * (1.0 - t) * t) + end * (t * t)
}

#[derive(Clone, Copy)]
struct WaypointVertex {
    point: Vec2,
    passed: usize,
}

fn push_waypoint_vertex(points: &mut Vec<WaypointVertex>, point: Vec2, passed: usize) {
    if let Some(last) = points.last_mut()
        && last.point == point
    {
        last.passed = passed;
    } else {
        points.push(WaypointVertex { point, passed });
    }
}

fn routed_waypoint_vertices(
    connector: &CanvasConnector,
    a: EndTarget,
    b: EndTarget,
) -> Vec<WaypointVertex> {
    let mut points = Vec::with_capacity(connector.waypoints.len() + 2);
    points.push(WaypointVertex {
        point: a.rect.center(),
        passed: 0,
    });
    for (index, &waypoint) in connector.waypoints.iter().enumerate() {
        if connector.route == ConnectorRoute::Curve {
            points.push(WaypointVertex {
                point: waypoint,
                passed: index + 1,
            });
        } else {
            push_waypoint_vertex(&mut points, waypoint, index + 1);
        }
    }
    if connector.route == ConnectorRoute::Curve {
        points.push(WaypointVertex {
            point: b.rect.center(),
            passed: connector.waypoints.len(),
        });
    } else {
        push_waypoint_vertex(&mut points, b.rect.center(), connector.waypoints.len());
    }
    if points.len() == 1 {
        points.push(points[0]);
    }

    if connector.route == ConnectorRoute::Elbow {
        let mut orthogonal = Vec::with_capacity(points.len() * 2);
        orthogonal.push(points[0]);
        for &next in &points[1..] {
            let previous = *orthogonal.last().expect("path starts with a point");
            if previous.point.x != next.point.x && previous.point.y != next.point.y {
                push_waypoint_vertex(
                    &mut orthogonal,
                    Vec2::new(next.point.x, previous.point.y),
                    previous.passed,
                );
            }
            push_waypoint_vertex(&mut orthogonal, next.point, next.passed);
        }
        if orthogonal.len() == 1 {
            orthogonal.push(orthogonal[0]);
        }
        points = orthogonal;
    }

    let last = points.len() - 1;
    let first_toward = points[1..]
        .iter()
        .map(|vertex| vertex.point)
        .find(|point| *point != points[0].point)
        .unwrap_or(points[last].point);
    let last_toward = points[..last]
        .iter()
        .rev()
        .map(|vertex| vertex.point)
        .find(|point| *point != points[last].point)
        .unwrap_or(points[0].point);
    points[0].point = a.clip(first_toward);
    points[last].point = b.clip(last_toward);

    if connector.route != ConnectorRoute::Curve || points.len() == 2 {
        let mut distinct = Vec::with_capacity(points.len());
        for vertex in points {
            push_waypoint_vertex(&mut distinct, vertex.point, vertex.passed);
        }
        if distinct.len() == 1 {
            distinct.push(distinct[0]);
        }
        return distinct;
    }
    points
}

fn routed_waypoints(connector: &CanvasConnector, a: EndTarget, b: EndTarget) -> Vec<Vec2> {
    let points = routed_waypoint_vertices(connector, a, b);
    if connector.route != ConnectorRoute::Curve || points.len() == 2 {
        return points.into_iter().map(|vertex| vertex.point).collect();
    }
    let last = points.len() - 1;

    // draw.io's curved polylines use each control point as the handle of a
    // quadratic span, with intermediate spans meeting halfway between handles.
    let mut curved = vec![points[0].point];
    let mut start = points[0].point;
    for index in 1..last {
        let control = points[index].point;
        let end = if index + 1 == last {
            points[last].point
        } else {
            (control + points[index + 1].point) * 0.5
        };
        for step in 1..=WAYPOINT_CURVE_SEGMENTS {
            let t = step as f32 / WAYPOINT_CURVE_SEGMENTS as f32;
            push_distinct(&mut curved, quadratic(start, control, end, t));
        }
        start = end;
    }
    if curved.len() == 1 {
        curved.push(curved[0]);
    }
    curved
}

struct ConnectorResolver<'a> {
    sheet: &'a Sheet,
    layout: &'a Layout,
    objects: HashMap<CanvasObjectId, &'a CanvasObject>,
    paths: HashMap<CanvasObjectId, Option<Vec<Vec2>>>,
}

impl<'a> ConnectorResolver<'a> {
    fn new(sheet: &'a Sheet, layout: &'a Layout) -> Self {
        Self {
            sheet,
            layout,
            objects: sheet.canvas.objects.iter().map(|o| (o.id, o)).collect(),
            paths: HashMap::new(),
        }
    }

    fn endpoint(&self, end: Endpoint, port: Option<ConnectorPort>) -> Option<EndTarget> {
        let target = match end {
            Endpoint::Point(p) => {
                Some(EndTarget::boxed(Rect::from_center_size(p, Vec2::ZERO), 0.0))
            }
            Endpoint::Topic(t) => {
                let (anchor, rect) = topic_anchor(self.sheet, self.layout, t)?;
                let rotation = self
                    .sheet
                    .topic(anchor)
                    .and_then(|topic| topic.card.as_ref())
                    .map_or(0.0, |card| card.rotation);
                Some(EndTarget::boxed(rect, rotation))
            }
            Endpoint::Object(id) => {
                let object = self.objects.get(&id)?;
                if object.is_connector() {
                    // Edge-to-edge ends meet the target path halfway along it,
                    // where its label sits, even when the route bends.
                    let points = self.paths.get(&id)?.as_ref()?;
                    let center = crate::board::polyline_midpoint(points)?;
                    Some(EndTarget::boxed(
                        Rect::from_center_size(center, Vec2::ZERO),
                        0.0,
                    ))
                } else {
                    Some(EndTarget {
                        rect: object.rect,
                        rotation: object.rotation,
                        shape: match &object.kind {
                            CanvasObjectKind::Shape { shape, .. } => {
                                Some((*shape, object.rounding))
                            }
                            _ => None,
                        },
                    })
                }
            }
        }?;
        Some(target.with_port(port))
    }

    fn resolve(&mut self, id: CanvasObjectId) -> bool {
        let mut stack = vec![(id, false)];
        let mut active = HashSet::new();
        while let Some((id, exit)) = stack.pop() {
            if self.paths.contains_key(&id) {
                continue;
            }
            let Some(object) = self.objects.get(&id) else {
                return false;
            };
            let CanvasObjectKind::Connector(connector) = &object.kind else {
                continue;
            };
            if exit {
                active.remove(&id);
                let path = self
                    .endpoint(connector.from, connector.from_port)
                    .zip(self.endpoint(connector.to, connector.to_port))
                    .map(|(from, to)| routed_path(connector, from, to));
                self.paths.insert(id, path);
                continue;
            }
            if !active.insert(id) {
                return false;
            }
            stack.push((id, true));
            for end in [connector.from, connector.to] {
                if let Endpoint::Object(target) = end
                    && self.objects.get(&target).is_some_and(|o| o.is_connector())
                    && !self.paths.contains_key(&target)
                {
                    if active.contains(&target) {
                        return false;
                    }
                    stack.push((target, false));
                }
            }
        }
        true
    }

    fn path(&mut self, id: CanvasObjectId, connector: &CanvasConnector) -> Option<Vec<Vec2>> {
        if self.objects.get(&id).is_some_and(|o| o.is_connector()) {
            self.resolve(id)
                .then(|| self.paths.get(&id).cloned().flatten())?
        } else {
            for end in [connector.from, connector.to] {
                if let Endpoint::Object(target) = end
                    && !self.resolve(target)
                {
                    return None;
                }
            }
            self.endpoint(connector.from, connector.from_port)
                .zip(self.endpoint(connector.to, connector.to_port))
                .map(|(from, to)| routed_path(connector, from, to))
        }
    }
}

/// Reject cyclic connector dependencies before a board is edited or loaded.
pub fn validate_connector_cycles(sheet: &Sheet) -> Result<(), String> {
    let connectors: HashMap<_, _> = sheet
        .canvas
        .objects
        .iter()
        .filter_map(|object| match &object.kind {
            CanvasObjectKind::Connector(connector) => Some((object.id, connector)),
            _ => None,
        })
        .collect();
    let mut pending = HashMap::new();
    let mut dependents: HashMap<CanvasObjectId, Vec<CanvasObjectId>> = HashMap::new();
    for (&id, connector) in &connectors {
        let mut count = 0;
        for end in [connector.from, connector.to] {
            if let Endpoint::Object(target) = end
                && connectors.contains_key(&target)
            {
                count += 1;
                dependents.entry(target).or_default().push(id);
            }
        }
        pending.insert(id, count);
    }
    let mut ready: VecDeque<_> = pending
        .iter()
        .filter_map(|(&id, &count)| (count == 0).then_some(id))
        .collect();
    let mut visited = 0;
    while let Some(id) = ready.pop_front() {
        visited += 1;
        if let Some(owners) = dependents.get(&id) {
            for owner in owners {
                let count = pending.get_mut(owner).expect("connector dependency exists");
                *count -= 1;
                if *count == 0 {
                    ready.push_back(*owner);
                }
            }
        }
    }
    if visited == connectors.len() {
        Ok(())
    } else {
        Err("connector dependencies contain a cycle".into())
    }
}

fn connector_targets(
    sheet: &Sheet,
    layout: &Layout,
    connector: &CanvasConnector,
) -> Option<(EndTarget, EndTarget)> {
    let mut resolver = ConnectorResolver::new(sheet, layout);
    for end in [connector.from, connector.to] {
        if let Endpoint::Object(id) = end
            && !resolver.resolve(id)
        {
            return None;
        }
    }
    resolver
        .endpoint(connector.from, connector.from_port)
        .zip(resolver.endpoint(connector.to, connector.to_port))
}

/// Polyline of a free connector, clipped to its endpoint targets. Empty if an endpoint
/// cannot be resolved (dangling reference, hidden anchor or invalid connector cycle).
pub fn connector_path(sheet: &Sheet, layout: &Layout, connector: &CanvasConnector) -> Vec<Vec2> {
    connector_targets(sheet, layout, connector)
        .map_or_else(Vec::new, |(from, to)| routed_path(connector, from, to))
}

/// A point on a rendered connector where another waypoint can be inserted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConnectorInsertHandle {
    pub point: Vec2,
    /// Index before which the new waypoint is inserted.
    pub insert_at: usize,
}

/// Insertion handles for the supplied rendered path, in path order.
///
/// Elbow bends introduced by the router retain the insertion index of their
/// logical span. Curve controls are not generally on the path, so their
/// handles use sampled positions on either side of each control.
pub fn connector_insert_handles(
    sheet: &Sheet,
    layout: &Layout,
    connector: &CanvasConnector,
) -> Vec<ConnectorInsertHandle> {
    let Some((from, to)) = connector_targets(sheet, layout, connector) else {
        return Vec::new();
    };
    if connector.waypoints.is_empty() {
        let path = routed_path(connector, from, to);
        if connector.route == ConnectorRoute::Curve {
            return crate::board::polyline_midpoint(&path)
                .map(|point| {
                    vec![ConnectorInsertHandle {
                        point,
                        insert_at: 0,
                    }]
                })
                .unwrap_or_default();
        }
        return path
            .windows(2)
            .filter(|segment| segment[0] != segment[1])
            .map(|segment| ConnectorInsertHandle {
                point: (segment[0] + segment[1]) * 0.5,
                insert_at: 0,
            })
            .collect();
    }
    let vertices = routed_waypoint_vertices(connector, from, to);
    if connector.route == ConnectorRoute::Curve {
        let mut handles = Vec::with_capacity(connector.waypoints.len() * 2);
        let mut start = vertices[0].point;
        let last = vertices[vertices.len() - 1].point;
        for (index, &control) in connector.waypoints.iter().enumerate() {
            let end = connector
                .waypoints
                .get(index + 1)
                .map_or(last, |next| (control + *next) * 0.5);
            for (t, insert_at) in [(0.25, index), (0.75, index + 1)] {
                handles.push(ConnectorInsertHandle {
                    point: quadratic(start, control, end, t),
                    insert_at,
                });
            }
            start = end;
        }
        return handles;
    }
    vertices
        .windows(2)
        .filter(|segment| segment[0].point != segment[1].point)
        .map(|segment| ConnectorInsertHandle {
            point: (segment[0].point + segment[1].point) * 0.5,
            insert_at: segment[0].passed,
        })
        .collect()
}

#[cfg(test)]
mod tests;
