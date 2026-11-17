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
/// Composite markers (ER, SysML, double block) add up to two extra parts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConnectorMarkerGeometry {
    shape: ConnectorMarkerShape,
    pub closed: bool,
    pub filled: bool,
    pub line_inset: f32,
    extra: [Option<ConnectorMarkerPart>; 2],
}

/// One independently stroked (and optionally filled) piece of a marker.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConnectorMarkerPart {
    shape: ConnectorMarkerShape,
    pub closed: bool,
    pub filled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ConnectorMarkerShape {
    Polygon { points: [Vec2; 4], point_count: u8 },
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

impl ConnectorMarkerShape {
    fn points(&self) -> &[Vec2] {
        match self {
            Self::Polygon {
                points,
                point_count,
            } => &points[..usize::from(*point_count)],
            _ => &[],
        }
    }

    fn circle(&self) -> Option<(Vec2, f32)> {
        match *self {
            Self::Circle(center, radius) => Some((center, radius)),
            _ => None,
        }
    }

    fn curve(&self) -> Option<[Vec2; 5]> {
        match *self {
            Self::Curve(points) => Some(points),
            _ => None,
        }
    }

    fn flattened_curve(&self, width: f32) -> Option<(Vec<Vec2>, f32)> {
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

macro_rules! marker_shape_accessors {
    () => {
        /// The polygonal outline, including the notch of a classic arrow.
        pub fn points(&self) -> &[Vec2] {
            self.shape.points()
        }

        /// The center and radius of a circular part, when present.
        pub fn circle(&self) -> Option<(Vec2, f32)> {
            self.shape.circle()
        }

        /// Two connected quadratic spans: start, control, join, control, end.
        pub fn curve(&self) -> Option<[Vec2; 5]> {
            self.shape.curve()
        }

        /// Flatten a marker curve for conservative stroke hit testing.
        pub fn flattened_curve(&self, width: f32) -> Option<(Vec<Vec2>, f32)> {
            self.shape.flattened_curve(width)
        }
    };
}

impl ConnectorMarkerPart {
    marker_shape_accessors!();

    fn segment(a: Vec2, b: Vec2) -> Self {
        Self::polygon(&[a, b], false, false)
    }

    fn polygon(points: &[Vec2], closed: bool, filled: bool) -> Self {
        let mut stored = [points[points.len() - 1]; 4];
        stored[..points.len()].copy_from_slice(points);
        Self {
            shape: ConnectorMarkerShape::Polygon {
                points: stored,
                point_count: points.len() as u8,
            },
            closed,
            filled: closed && filled,
        }
    }

    fn circle_part(center: Vec2, radius: f32, filled: bool) -> Self {
        Self {
            shape: ConnectorMarkerShape::Circle(center, radius),
            closed: true,
            filled,
        }
    }
}

impl ConnectorMarkerGeometry {
    marker_shape_accessors!();

    /// The primary part followed by any extra parts, in paint order.
    pub fn parts(&self) -> impl Iterator<Item = ConnectorMarkerPart> + '_ {
        std::iter::once(ConnectorMarkerPart {
            shape: self.shape,
            closed: self.closed,
            filled: self.filled,
        })
        .chain(self.extra.iter().flatten().copied())
    }

    fn single(shape: ConnectorMarkerShape, closed: bool, filled: bool, line_inset: f32) -> Self {
        Self {
            shape,
            closed,
            filled,
            line_inset,
            extra: [None, None],
        }
    }

    fn compose(line_inset: f32, parts: &[ConnectorMarkerPart]) -> Self {
        let primary = parts[0];
        Self {
            shape: primary.shape,
            closed: primary.closed,
            filled: primary.filled,
            line_inset,
            extra: [parts.get(1).copied(), parts.get(2).copied()],
        }
    }
}

/// draw.io's library markers (mxMarker.js, mxER.js, mxSysML.js, Shapes.js).
/// `n` is draw.io's `size + sw + 1` unit; `d` points back from the tip.
/// Tip position plus the unit travel direction and its normal at a connector end.
#[derive(Clone, Copy)]
struct MarkerFrame {
    tip: Vec2,
    direction: Vec2,
    normal: Vec2,
}

fn library_marker_geometry(
    frame: MarkerFrame,
    at_start: bool,
    kind: ConnectorMarkerKind,
    width: f32,
    marker_size: f32,
    fill: bool,
) -> Option<ConnectorMarkerGeometry> {
    use ConnectorMarkerKind as K;
    use ConnectorMarkerPart as P;
    let MarkerFrame {
        tip,
        direction,
        normal,
    } = frame;
    let n = marker_size + width + 1.0;
    let back = |distance: f32| tip - direction * distance;
    let bar = |distance: f32| {
        P::segment(
            back(distance) + normal * (n * 0.5),
            back(distance) - normal * (n * 0.5),
        )
    };
    let crow = P::polygon(
        &[tip - normal * (n * 0.5), back(n), tip + normal * (n * 0.5)],
        false,
        false,
    );
    let ring = |filled: bool| P::circle_part(back(1.5 * n), marker_size * 0.5, filled);
    let er_inset = 2.0 * n - width * 0.5;
    // Async barbs sit on the left of the travel direction at the source, right at the target.
    let side = |sign: f32, u: f32| normal * (sign * u * 0.5);
    let geometry = match kind {
        K::ErOne => ConnectorMarkerGeometry::compose(0.0, &[bar(n * 0.5)]),
        K::ErMandOne => ConnectorMarkerGeometry::compose(0.0, &[bar(n * 0.5), bar(n)]),
        K::ErMany => ConnectorMarkerGeometry::compose(0.0, &[crow]),
        K::ErOneToMany => ConnectorMarkerGeometry::compose(0.0, &[bar(n), crow]),
        // draw.io fills the "filled" ER ring with white and leaves the line untrimmed;
        // an unfilled ring with a trimmed line looks identical on the default canvas.
        K::ErZeroToMany => ConnectorMarkerGeometry::compose(
            er_inset,
            &[ring(false), crow, P::segment(back(n), tip)],
        ),
        K::ErZeroToOne => ConnectorMarkerGeometry::compose(
            er_inset,
            &[
                ring(false),
                bar(n * 0.5),
                P::segment(back(n + width * 0.5), tip),
            ],
        ),
        K::ManyOptional => ConnectorMarkerGeometry::compose(
            er_inset,
            &[ring(fill), P::segment(tip, back(n)), crow],
        ),
        K::Box => {
            let p = tip + direction * (n * 0.5);
            let half = normal * (n * 0.5);
            let near = p - direction * (n * 0.5);
            let far = p - direction * (n * 1.5);
            ConnectorMarkerGeometry::compose(
                n,
                &[P::polygon(
                    &[near + half, near - half, far - half, far + half],
                    true,
                    fill,
                )],
            )
        }
        K::Cross => {
            let half = normal * (n * 0.5);
            ConnectorMarkerGeometry::compose(
                0.0,
                &[
                    P::segment(back(n * 0.5) + half, back(n * 1.5) - half),
                    P::segment(back(n * 0.5) - half, back(n * 1.5) + half),
                ],
            )
        }
        K::Circle | K::CirclePlus => {
            let s = marker_size + width;
            let center = back(s + width);
            let circle = P::circle_part(center, s, fill);
            if kind == K::Circle {
                ConnectorMarkerGeometry::compose(2.0 * s + width, &[circle])
            } else {
                let m = marker_size + 2.0 * width;
                let cross_center = back(m);
                ConnectorMarkerGeometry::compose(
                    2.0 * s + width,
                    &[
                        circle,
                        P::segment(back(width), back(2.0 * m - width)),
                        P::segment(
                            cross_center + normal * (m - width),
                            cross_center - normal * (m - width),
                        ),
                    ],
                )
            }
        }
        K::Async | K::OpenAsync | K::DoubleBlock => {
            let u = marker_size + width;
            let offset = if kind == K::OpenAsync {
                0.0
            } else {
                width * 1.118
            };
            let pt = tip - direction * offset;
            let barb = |at: Vec2, sign: f32| at - direction * u + side(sign, u);
            let sign = if at_start { 1.0 } else { -1.0 };
            match kind {
                K::OpenAsync => {
                    ConnectorMarkerGeometry::compose(0.0, &[P::segment(pt, barb(pt, sign))])
                }
                K::Async => ConnectorMarkerGeometry::compose(
                    u + offset,
                    &[P::polygon(
                        &[pt, barb(pt, sign), pt - direction * u],
                        true,
                        fill,
                    )],
                ),
                _ => {
                    let second = pt - direction * u;
                    let triangle = |at: Vec2| {
                        P::polygon(
                            &[
                                at,
                                at - direction * u + side(1.0, u),
                                at - direction * u + side(-1.0, u),
                            ],
                            true,
                            fill,
                        )
                    };
                    ConnectorMarkerGeometry::compose(
                        2.0 * u + offset,
                        &[triangle(pt), triangle(second)],
                    )
                }
            }
        }
        K::SysmlX => {
            let along = direction * (n * 0.5);
            let across = normal * (n * 0.5);
            ConnectorMarkerGeometry::compose(
                0.0,
                &[
                    P::segment(tip - along + across, tip + along - across),
                    P::segment(tip + along + across, tip - along - across),
                ],
            )
        }
        K::SysmlLost => ConnectorMarkerGeometry::compose(
            0.0,
            &[
                P::polygon(
                    &[
                        back(1.5 * n) + normal * (n * 0.5),
                        back(n * 0.5),
                        back(1.5 * n) - normal * (n * 0.5),
                    ],
                    false,
                    false,
                ),
                P::circle_part(back(n * 0.5), marker_size * 0.5, true),
            ],
        ),
        K::SysmlFound => ConnectorMarkerGeometry::compose(
            0.0,
            &[P::circle_part(back(n * 0.5), marker_size * 0.5, true)],
        ),
        K::SysmlPackCont => ConnectorMarkerGeometry::compose(
            0.0,
            &[
                bar(n * 0.5),
                P::circle_part(back(n * 0.5), marker_size * 0.5, false),
            ],
        ),
        _ => return None,
    };
    Some(geometry)
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
    if kind.requires_marker_library_format() {
        return library_marker_geometry(
            MarkerFrame {
                tip,
                direction,
                normal,
            },
            at_start,
            kind,
            width,
            marker_size,
            fill,
        );
    }
    if kind == ConnectorMarkerKind::Oval {
        let radius = marker_size * 0.5;
        return Some(ConnectorMarkerGeometry::single(
            ConnectorMarkerShape::Circle(tip, radius),
            true,
            fill,
            radius,
        ));
    }
    if kind == ConnectorMarkerKind::HalfCircle {
        let radius = marker_size + width + 1.0;
        let apex = tip - direction * radius;
        let side = normal * radius;
        return Some(ConnectorMarkerGeometry::single(
            ConnectorMarkerShape::Curve([tip + side, apex + side, apex, apex - side, tip - side]),
            false,
            false,
            radius,
        ));
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
        return Some(ConnectorMarkerGeometry::single(
            ConnectorMarkerShape::Polygon {
                points: [first, last, last, last],
                point_count: 2,
            },
            false,
            false,
            0.0,
        ));
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
        return Some(ConnectorMarkerGeometry::single(
            ConnectorMarkerShape::Polygon {
                points: [
                    midpoint + normal * wing,
                    tip,
                    midpoint - normal * wing,
                    base,
                ],
                point_count: 4,
            },
            true,
            fill,
            offset + size,
        ));
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
        extra: [None, None],
    })
}

/// Trim a connector stroke to the inside of its non-default markers.
/// If the markers overlap on a short path, the remaining stroke collapses to a point.
pub fn connector_stroke_path(
    points: &[Vec2],
    markers: &[Option<ConnectorMarkerGeometry>; 2],
) -> Vec<Vec2> {
    if markers.iter().all(Option::is_none) {
        return points.to_vec();
    }
    trim_path(
        points,
        markers[0].map_or(0.0, |marker| marker.line_inset),
        markers[1].map_or(0.0, |marker| marker.line_inset),
    )
}

/// Shorten a path by `start` and `end` along its length; overlapping trims on
/// a short path collapse it to a point.
fn trim_path(points: &[Vec2], mut start: f32, mut end: f32) -> Vec<Vec2> {
    if points.len() < 2 || (start <= 0.0 && end <= 0.0) {
        return points.to_vec();
    }
    let total: f32 = points
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).length())
        .sum();
    if total <= f32::EPSILON {
        return points.to_vec();
    }
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

/// Jumped stroke of a connector crossing earlier connectors, already flattened.
#[derive(Debug, Clone, PartialEq)]
pub struct LineJumpGeometry {
    pub style: crate::board::LineJumpStyle,
    /// Open subpaths replacing the marker-trimmed stroke.
    pub paths: Vec<Vec<Vec2>>,
}

/// Total segment-pair tests one scene may spend on line jumps.
const LINE_JUMP_TEST_BUDGET: usize = 4_000_000;
const LINE_JUMP_ARC_STEPS: usize = 8;

/// Open subpaths a connector stroke is painted with, including line jumps.
pub fn connector_stroke_paths(
    points: &[Vec2],
    markers: &[Option<ConnectorMarkerGeometry>; 2],
    jumps: Option<&LineJumpGeometry>,
) -> Vec<Vec<Vec2>> {
    match jumps {
        Some(jumps) => jumps.paths.clone(),
        None => vec![connector_stroke_path(points, markers)],
    }
}

/// Remove waypoints lying on the segment between their neighbours, like draw.io.
fn merge_collinear(points: &[Vec2]) -> Vec<Vec2> {
    let mut result: Vec<Vec2> = Vec::with_capacity(points.len());
    for (index, &point) in points.iter().enumerate() {
        if let (Some(&previous), Some(&next)) = (result.last(), points.get(index + 1))
            && index > 0
            && point_segment_distance_squared(point, previous, next) < 1.0
        {
            continue;
        }
        result.push(point);
    }
    result
}

fn dot(a: Vec2, b: Vec2) -> f32 {
    a.x * b.x + a.y * b.y
}

fn point_segment_distance_squared(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let length = dot(ab, ab);
    let t = if length <= f32::EPSILON {
        0.0
    } else {
        (dot(p - a, ab) / length).clamp(0.0, 1.0)
    };
    let d = p - (a + ab * t);
    dot(d, d)
}

fn segment_crossing(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> Option<Vec2> {
    let r = b - a;
    let s = d - c;
    let denominator = r.x * s.y - r.y * s.x;
    if denominator.abs() <= f32::EPSILON {
        return None;
    }
    let ac = c - a;
    let t = (ac.x * s.y - ac.y * s.x) / denominator;
    let u = (ac.x * r.y - ac.y * r.x) / denominator;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then(|| a + r * t)
}

fn near(a: Vec2, b: Vec2) -> bool {
    (a.x - b.x).abs() <= 0.5 && (a.y - b.y).abs() <= 0.5
}

/// draw.io `jumpStyle` geometry: `stroke` jumps wherever it crosses `others`.
/// Returns `None` when nothing is crossed or the jump would be invisible.
pub fn line_jump_geometry(
    stroke: &[Vec2],
    others: &[&[Vec2]],
    jumps: crate::board::LineJumps,
    width: f32,
    budget: &mut usize,
) -> Option<LineJumpGeometry> {
    use crate::board::LineJumpStyle;
    let half = (jumps.size - 2.0) / 2.0 + width;
    if half.is_nan() || half <= 0.0 || stroke.len() < 2 || others.is_empty() {
        return None;
    }
    let stroke = merge_collinear(stroke);
    let others: Vec<(Rect, Vec<Vec2>)> = others
        .iter()
        .filter_map(|points| {
            let points = merge_collinear(points);
            Some((points_bounds(&points)?.expand(0.5), points))
        })
        .collect();
    let mut paths = Vec::new();
    let mut current = vec![stroke[0]];
    let mut jumped = false;
    let mut last = stroke[0];
    let half_squared = half * half;
    for pair in stroke.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let segment = Rect::from_min_size(
            Vec2::new(a.x.min(b.x), a.y.min(b.y)),
            Vec2::new((a.x - b.x).abs(), (a.y - b.y).abs()),
        )
        .expand(0.5);
        let mut crossings: Vec<(f32, Vec2)> = Vec::new();
        for (bounds, points) in &others {
            if !bounds.intersects(&segment) {
                continue;
            }
            for other in points.windows(2) {
                if *budget == 0 {
                    break;
                }
                *budget -= 1;
                if let Some(pt) = segment_crossing(a, b, other[0], other[1])
                    && !near(pt, a)
                    && !near(pt, b)
                {
                    crossings.push(((pt - a).length(), pt));
                }
            }
        }
        crossings.sort_by(|x, y| x.0.total_cmp(&y.0));
        crossings.dedup_by(|later, earlier| near(later.1, earlier.1));
        let mut direction = None;
        for (_, pt) in crossings {
            let distance_next = dot(b - pt, b - pt);
            let distance_last = dot(last - pt, last - pt);
            if distance_next <= half_squared || distance_last <= half_squared {
                last = pt;
                continue;
            }
            let n = *direction.get_or_insert_with(|| {
                let delta = pt - last;
                delta * (half / delta.length())
            });
            let p0 = pt - n;
            let p1 = pt + n;
            let flip = if n.x.round() < 0.0 || (n.x.round() == 0.0 && n.y.round() <= 0.0) {
                1.0
            } else {
                -1.0
            };
            current.push(p0);
            jumped = true;
            match jumps.style {
                LineJumpStyle::Sharp => {
                    let offset = Vec2::new(-n.y * flip, n.x * flip);
                    current.extend([p0 + offset, p1 + offset, p1]);
                }
                LineJumpStyle::Arc => {
                    let flip = flip * 1.3;
                    let offset = Vec2::new(-n.y * flip, n.x * flip);
                    let (c0, c1) = (p0 + offset, p1 + offset);
                    for step in 1..=LINE_JUMP_ARC_STEPS {
                        let t = step as f32 / LINE_JUMP_ARC_STEPS as f32;
                        let u = 1.0 - t;
                        current.push(
                            p0 * (u * u * u)
                                + c0 * (3.0 * u * u * t)
                                + c1 * (3.0 * u * t * t)
                                + p1 * (t * t * t),
                        );
                    }
                }
                LineJumpStyle::Line => {
                    let tick = Vec2::new(n.y * flip, -n.x * flip);
                    paths.push(std::mem::take(&mut current));
                    paths.push(vec![p0 + tick, p0 - tick]);
                    paths.push(vec![p1 + tick, p1 - tick]);
                    current.push(p1);
                }
                LineJumpStyle::Gap => {
                    paths.push(std::mem::take(&mut current));
                    current.push(p1);
                }
            }
        }
        current.push(b);
        last = b;
    }
    if !jumped {
        return None;
    }
    paths.push(current);
    paths.retain(|path| path.len() >= 2);
    Some(LineJumpGeometry {
        style: jumps.style,
        paths,
    })
}

/// Apply draw.io line jumps in paint order: each jumping connector jumps over
/// straight and elbow connectors painted before it; curves neither jump nor
/// are jumped over.
fn apply_line_jumps(
    items: &mut [SceneItem],
    jumpers: &[(usize, Option<crate::board::LineJumps>, f32)],
) {
    if jumpers.iter().all(|(_, jumps, _)| jumps.is_none()) {
        return;
    }
    let mut budget = LINE_JUMP_TEST_BUDGET;
    for (position, &(index, jumps, width)) in jumpers.iter().enumerate() {
        let Some(jumps) = jumps else {
            continue;
        };
        let geometry = {
            let others: Vec<&[Vec2]> = jumpers[..position]
                .iter()
                .filter_map(|&(other, _, _)| match &items[other].geometry {
                    SceneGeometry::Polyline { points, .. } => Some(points.as_slice()),
                    _ => None,
                })
                .collect();
            let SceneGeometry::Polyline {
                points, markers, ..
            } = &items[index].geometry
            else {
                continue;
            };
            let stroke = connector_stroke_path(points, markers);
            line_jump_geometry(&stroke, &others, jumps, width, &mut budget)
        };
        if let Some(geometry) = geometry {
            let item = &mut items[index];
            if let SceneGeometry::Polyline { jumps, .. } = &mut item.geometry {
                *jumps = Some(Box::new(geometry));
            }
            if let Some(bounds) = geometry_bounds(&item.geometry) {
                item.bounds = item.bounds.union(&bounds);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SceneGeometry {
    /// Derived semantic flow, shared by canvas, export, bounds and hit testing.
    Ribbon {
        outline: Vec<Vec2>,
        mass: f64,
        color: Option<crate::Color>,
        label_bounds: Rect,
    },
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
        /// Shared color-scale override for an unassigned semantic placeholder.
        color: Option<crate::Color>,
        /// Boxed so composite marker parts do not bloat every scene item.
        markers: Box<[Option<ConnectorMarkerGeometry>; 2]>,
        label_bounds: Option<Rect>,
        /// Estimated frames of positioned labels, which may sit away from the path.
        text_labels: Vec<Rect>,
        /// draw.io line jumps over connectors painted earlier.
        jumps: Option<Box<LineJumpGeometry>>,
        /// draw.io body shape replacing or overpainting the plain stroke.
        body: Option<Box<EdgeBody>>,
    },
}

impl SceneGeometry {
    fn hit(&self, p: Vec2, tolerance: f32) -> bool {
        match self {
            SceneGeometry::Rect { rect, rotation } => {
                rotated_rect_contains(rect.expand(tolerance), *rotation, p)
            }
            SceneGeometry::Ribbon {
                outline,
                label_bounds,
                ..
            } => {
                label_bounds.expand(tolerance).contains(p)
                    || point_in_polygon(p, outline)
                    || distance_to_outline(p, outline) <= tolerance
            }
            SceneGeometry::Polygon { outline, details } => {
                point_in_polygon(p, outline)
                    || distance_to_outline(p, outline) <= tolerance
                    || details
                        .iter()
                        .any(|detail| distance_to_polyline(p, detail) <= tolerance)
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
                label_bounds,
                text_labels,
                body,
                ..
            } => {
                let radius = body
                    .as_ref()
                    .map_or(*width, |body| body.stroke_width(*width))
                    / 2.0
                    + tolerance;
                let label_hit = label_bounds.is_some_and(|r| r.expand(tolerance).contains(p))
                    || text_labels.iter().any(|r| r.expand(tolerance).contains(p));
                match body.as_deref() {
                    Some(EdgeBody::Rails(rails)) => {
                        return label_hit
                            || rails
                                .iter()
                                .any(|rail| distance_to_polyline(p, rail) <= radius);
                    }
                    Some(EdgeBody::Band { outline, .. })
                        if point_in_polygon(p, outline)
                            || distance_to_outline(p, outline) <= radius =>
                    {
                        return true;
                    }
                    _ => {}
                }
                let stroke_hit = if body.as_ref().is_some_and(|body| body.replaces_stroke()) {
                    false
                } else if markers.iter().any(Option::is_some) {
                    distance_to_polyline(p, &connector_stroke_path(points, markers)) <= radius
                } else {
                    distance_to_polyline(p, points) <= radius
                };
                label_hit
                    || stroke_hit
                    || markers
                        .iter()
                        .flatten()
                        .flat_map(ConnectorMarkerGeometry::parts)
                        .any(|marker| {
                            if let Some((center, marker_radius)) = marker.circle() {
                                let distance = (p - center).length();
                                marker_radius > 0.0
                                    && ((marker.filled && distance <= marker_radius + radius)
                                        || (distance - marker_radius).abs() <= radius)
                            } else if let Some((path, error)) = marker.flattened_curve(*width) {
                                point_near_curve_path(
                                    p,
                                    &path,
                                    f64::from(*width) / 2.0
                                        + f64::from(tolerance)
                                        + f64::from(error),
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
    /// Resolved stencil data shared by paint, labels, hit geometry and export.
    pub stencil_render: Option<crate::stencil::StencilRender>,
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

/// Transient collapse geometry over an immutable source sheet. This is not a
/// savable document: stored bounds, endpoints and ownership remain in the source.
#[derive(Debug)]
pub struct CollapsePreview<'a> {
    sheet: &'a Sheet,
    objects: HashMap<CanvasObjectId, &'a CanvasObject>,
    compact: HashMap<CanvasObjectId, CanvasObject>,
    hidden_by: HashMap<CanvasObjectId, CanvasObjectId>,
}

impl<'a> CollapsePreview<'a> {
    pub fn new(sheet: &'a Sheet, collapsed: &[(CanvasObjectId, Rect)]) -> Result<Self, String> {
        if collapsed.len() > crate::board::MAX_CANVAS_OBJECTS
            || collapsed.len() > sheet.canvas.objects.len()
        {
            return Err("collapse preview exceeds canvas limits".into());
        }
        crate::board::validate_board(sheet)?;
        if !collapsed.is_empty() && sheet.has_sankey() {
            return Err("collapse preview does not support Sankey sheets".into());
        }
        let objects: HashMap<_, _> = sheet.canvas.objects.iter().map(|o| (o.id, o)).collect();
        let mut compact: HashMap<_, _> = sheet
            .canvas
            .objects
            .iter()
            .filter(|object| sheet.canvas.scene.collapsed.contains_key(&object.id))
            .map(|object| (object.id, object.clone()))
            .collect();
        let mut requested = HashSet::with_capacity(collapsed.len());
        for &(id, rect) in collapsed {
            if !sheet.canvas.scene.containers.contains(&id) {
                return Err(format!("collapse target {id} is not a scene container"));
            }
            if !requested.insert(id) {
                return Err(format!("duplicate collapse target {id}"));
            }
            let mut object = (**objects.get(&id).ok_or("collapse target not found")?).clone();
            object.rect = rect;
            crate::board::validate_object(&object)?;
            if rect.size.x <= 0.0 || rect.size.y <= 0.0 {
                return Err(format!(
                    "collapse target {id} requires a positive compact frame"
                ));
            }
            compact.insert(id, object);
        }
        Ok(Self::project(sheet, objects, compact))
    }

    fn stored(sheet: &'a Sheet) -> Self {
        let objects = sheet.canvas.objects.iter().map(|o| (o.id, o)).collect();
        let compact = sheet
            .canvas
            .objects
            .iter()
            .filter(|object| sheet.canvas.scene.collapsed.contains_key(&object.id))
            .map(|object| (object.id, object.clone()))
            .collect();
        Self::project(sheet, objects, compact)
    }

    // Both entry points operate on validated sheets. The bounded parent walk is
    // order-independent; dependency order is shared with load-boundary validation.
    fn project(
        sheet: &'a Sheet,
        objects: HashMap<CanvasObjectId, &'a CanvasObject>,
        compact: HashMap<CanvasObjectId, CanvasObject>,
    ) -> Self {
        let mut hidden_by = HashMap::new();
        for &id in objects.keys() {
            let mut cursor = id;
            for _ in 0..crate::scene_hierarchy::MAX_SCENE_DEPTH {
                let Some(&parent) = sheet.canvas.scene.parents.get(&cursor) else {
                    break;
                };
                if compact.contains_key(&parent) {
                    hidden_by.insert(id, parent);
                }
                cursor = parent;
            }
        }
        for id in connector_dependency_order(sheet).0 {
            let Some(CanvasObjectKind::Connector(connector)) =
                objects.get(&id).map(|object| &object.kind)
            else {
                continue;
            };
            let terminal = |end: Endpoint| match end {
                Endpoint::Object(target) => Some((
                    hidden_by.get(&target).copied().unwrap_or(target),
                    hidden_by.contains_key(&target),
                )),
                _ => None,
            };
            let from = terminal(connector.from);
            let to = terminal(connector.to);
            let visible_self_loop = from
                .zip(to)
                .is_some_and(|((a, folded_a), (b, folded_b))| a == b && !folded_a && !folded_b);
            let structural_owner = hidden_by.get(&id).copied();
            let folded_owner = from.zip(to).and_then(|((a, folded_a), (b, folded_b))| {
                (a == b && (folded_a || folded_b)).then_some(a)
            });
            let hidden_owner = structural_owner
                .filter(|_| !visible_self_loop)
                .filter(|owner| {
                    [connector.from, connector.to]
                        .into_iter()
                        .all(|end| match end {
                            Endpoint::Point(_) => true,
                            Endpoint::Topic(_) => false,
                            Endpoint::Object(_) => {
                                terminal(end).is_some_and(|(target, _)| target == *owner)
                            }
                        })
                })
                .or(folded_owner);
            if let Some(owner) = hidden_owner {
                hidden_by.insert(id, owner);
            } else {
                // Imported crossing edges can be owned by hidden containers.
                hidden_by.remove(&id);
            }
        }
        Self {
            sheet,
            objects,
            compact,
            hidden_by,
        }
    }

    pub fn source_sheet(&self) -> &Sheet {
        self.sheet
    }

    /// The nearest visible target, or the object itself when it is visible.
    pub fn visible_representative(&self, id: CanvasObjectId) -> Option<CanvasObjectId> {
        self.objects.get(&id)?;
        Some(self.hidden_by.get(&id).copied().unwrap_or(id))
    }

    /// Visible object geometry/styles for consumers of the derived scene.
    pub fn object(&self, id: CanvasObjectId) -> Option<&CanvasObject> {
        if self.hidden_by.contains_key(&id) {
            return None;
        }
        self.compact
            .get(&id)
            .or_else(|| self.objects.get(&id).copied())
    }

    pub fn build_scene(&self, layout: &Layout) -> Scene {
        build_scene_with_collapse(self.sheet, layout, Some(self))
    }
}

/// Builds the paint-order scene of a validated `sheet` from its layout.
/// Load/import/recovery/paste boundaries must validate first: unsupported Sankey
/// endpoints are rejected there, never repaired or normalized by this renderer.
pub fn build_scene(sheet: &Sheet, layout: &Layout) -> Scene {
    let collapse = sheet
        .has_scene_collapse()
        .then(|| CollapsePreview::stored(sheet));
    build_scene_with_collapse(sheet, layout, collapse.as_ref())
}

fn build_scene_with_collapse<'a>(
    sheet: &'a Sheet,
    layout: &'a Layout,
    collapse: Option<&'a CollapsePreview<'a>>,
) -> Scene {
    let mut items =
        Vec::with_capacity(sheet.zones.len() + layout.order.len() + sheet.canvas.objects.len());
    let mut connectors = ConnectorResolver::with_collapse(sheet, layout, collapse);
    let flow_values = crate::sankey::evaluate(sheet).unwrap_or_default();
    let ribbons = crate::sankey::ribbons_with_values(sheet, layout, &flow_values);
    let mut jumpers = Vec::new();
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
            stencil_render: None,
        });
    }
    let mut objects = |band: Band, items: &mut Vec<SceneItem>| {
        let jumpers = &mut jumpers;
        for object in sheet
            .canvas
            .objects
            .iter()
            .filter(|o| o.band == band && sheet.canvas.is_layer_visible(o.layer))
        {
            let object = if let Some(collapse) = collapse {
                let Some(object) = collapse.object(object.id) else {
                    continue;
                };
                object
            } else {
                object
            };
            if let Some(item) = object_item(sheet, object, &mut connectors) {
                if let Some(outline) = ribbons.get(&object.id) {
                    if let Some(bounds) = crate::board::points_bounds(outline) {
                        let c = match &object.kind {
                            CanvasObjectKind::Connector(c) => c,
                            _ => continue,
                        };
                        let mass = flow_values.mass.get(&object.id).copied().unwrap_or(0.0);
                        let anchor =
                            crate::sankey::ribbon_label_anchor(outline).unwrap_or(bounds.center());
                        let label_bounds = crate::sankey::label_bounds(
                            anchor,
                            &crate::sankey::flow_label(c, mass),
                        );
                        items.push(SceneItem {
                            bounds: bounds.union(&label_bounds),
                            geometry: SceneGeometry::Ribbon {
                                outline: outline.clone(),
                                mass,
                                label_bounds,
                                color: sheet.sankey.as_ref().filter(|s| s.color_scale).map(|_| {
                                    crate::sankey::weight_color(
                                        flow_values.mass.get(&object.id).copied().unwrap_or(0.0),
                                    )
                                }),
                            },
                            ..item
                        });
                    }
                } else {
                    if let (CanvasObjectKind::Connector(c), SceneGeometry::Polyline { .. }) =
                        (&object.kind, &item.geometry)
                        && c.route != ConnectorRoute::Curve
                    {
                        let jumps = c.line_jumps.filter(|_| {
                            c.edge_shape
                                .is_none_or(|shape| shape.kind.supports_line_jumps())
                        });
                        jumpers.push((items.len(), jumps, c.width));
                    }
                    items.push(item);
                }
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
            stencil_render: None,
        });
    }
    objects(Band::Above, &mut items);
    apply_line_jumps(&mut items, &jumpers);
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
    let header_policy = sheet
        .canvas
        .scene
        .layout_policies
        .get(&object.id)
        .map(|policy| policy.header);
    let stencil_render = matches!(object.kind, CanvasObjectKind::Shape { .. })
        .then(|| crate::stencil_object::render_with_header_policy(object, header_policy))
        .flatten();
    let geometry = match &object.kind {
        CanvasObjectKind::Shape { .. } => {
            let g = stencil_render
                .as_ref()
                .map(|render| crate::stencil_object::geometry(object, render))
                .or_else(|| crate::board::object_shape_geometry(object))?;
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
            let mut markers = Box::new([
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
            ]);
            let body = edge_shape::edge_body(c, &points, &mut markers);
            SceneGeometry::Polyline {
                color: (c.sankey.is_some() && sheet.sankey.as_ref().is_some_and(|s| s.color_scale))
                    .then(|| crate::sankey::weight_color(0.0)),
                label_bounds: c.sankey.filter(|f| f.weight.is_none()).map(|_| {
                    crate::sankey::label_bounds(
                        crate::board::polyline_midpoint(&points).unwrap_or_default(),
                        &crate::sankey::flow_label(c, 0.0),
                    )
                }),
                text_labels: positioned_label_bounds(c, &points),
                markers,
                jumps: None,
                body,
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
        stencil_render,
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

/// Estimated frames of labels placed away from the default midpoint; the
/// renderers measure text exactly, so these only widen hit testing and bounds.
fn positioned_label_bounds(c: &CanvasConnector, points: &[Vec2]) -> Vec<Rect> {
    if !c.has_positioned_labels() {
        return Vec::new();
    }
    let skip_main = c.label_placement.is_none() && !c.label.is_empty();
    c.label_anchors(points)
        .into_iter()
        .skip(usize::from(skip_main))
        .map(|(text, anchor)| crate::board::approx_connector_label_bounds(anchor, text))
        .collect()
}

fn geometry_bounds(geometry: &SceneGeometry) -> Option<Rect> {
    match geometry {
        SceneGeometry::Rect { rect, rotation } => Some(rotated_bounds(*rect, *rotation)),
        SceneGeometry::Ribbon {
            outline,
            label_bounds,
            ..
        } => crate::board::points_bounds(outline).map(|r| r.union(label_bounds)),
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
            label_bounds,
            text_labels,
            jumps,
            body,
            ..
        } => {
            let width = &body
                .as_ref()
                .map_or(*width, |body| body.stroke_width(*width));
            let mut bounds = points_bounds(points)?.expand(width / 2.0);
            match body.as_deref() {
                Some(EdgeBody::Rails(rails)) => {
                    for rail in rails {
                        if let Some(rail_bounds) = points_bounds(rail) {
                            bounds = bounds.union(&rail_bounds.expand(width / 2.0));
                        }
                    }
                }
                Some(EdgeBody::Band { outline, .. }) => {
                    if let Some(outline_bounds) = points_bounds(outline) {
                        bounds = bounds.union(&outline_bounds.expand(width / 2.0));
                    }
                }
                _ => {}
            }
            if let Some(label_bounds) = label_bounds {
                bounds = bounds.union(label_bounds);
            }
            for label in text_labels {
                bounds = bounds.union(label);
            }
            for path in jumps.iter().flat_map(|jumps| &jumps.paths) {
                if let Some(path_bounds) = points_bounds(path) {
                    bounds = bounds.union(&path_bounds.expand(width / 2.0));
                }
            }
            for marker in markers
                .iter()
                .flatten()
                .flat_map(ConnectorMarkerGeometry::parts)
            {
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
    if matches!(&object.kind,CanvasObjectKind::Connector(c) if c.sankey.is_some()) {
        return build_scene(sheet, layout)
            .item(SceneId::Object(object.id))
            .and_then(SceneItem::visible_bounds);
    }
    let collapse = sheet
        .has_scene_collapse()
        .then(|| CollapsePreview::stored(sheet));
    if collapse
        .as_ref()
        .is_some_and(|collapse| collapse.hidden_by.contains_key(&object.id))
    {
        return None;
    }
    object_item(
        sheet,
        object,
        &mut ConnectorResolver::with_collapse(sheet, layout, collapse.as_ref()),
    )
    .and_then(|item| item.visible_bounds())
}

/// Visible bounds of the whole canvas layer, resolving shared connector dependencies once.
pub fn canvas_bounds(sheet: &Sheet, layout: &Layout) -> Option<Rect> {
    if sheet.has_sankey() || sheet.has_scene_collapse() {
        return build_scene(sheet, layout)
            .items
            .iter()
            .filter(|item| matches!(item.id, SceneId::Object(_)))
            .filter_map(SceneItem::visible_bounds)
            .reduce(|a, b| a.union(&b));
    }
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
#[derive(Clone)]
struct EndTarget {
    rect: Rect,
    rotation: f32,
    shape: Option<(ShapeKind, Option<ShapeRounding>)>,
    custom_outline: Option<Vec<Vec2>>,
}

impl EndTarget {
    fn boxed(rect: Rect, rotation: f32) -> Self {
        Self {
            rect,
            rotation,
            shape: None,
            custom_outline: None,
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
    fn clip(&self, toward: Vec2) -> Vec2 {
        if let Some(outline) = self.custom_outline.as_deref()
            && outline.len() >= 3
        {
            let center = self.rect.center();
            let origin = if point_in_polygon(center, outline)
                && distance_to_outline(center, outline) > 0.5
            {
                center
            } else {
                polygon_centroid(outline)
            };
            if let Some(point) = clip_to_polygon(outline, origin, toward) {
                return point;
            }
        }
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

    /// draw.io's orthogonal perimeter projection for routed edge styles: when
    /// `next` lies within the target's horizontal or vertical span, the end
    /// leaves the outline straight towards it instead of from the centre.
    fn clip_orthogonal(&self, next: Vec2) -> Vec2 {
        let rect = self.rect;
        if self.rotation != 0.0 || rect.size.x <= 0.0 || rect.size.y <= 0.0 {
            return self.clip(next);
        }
        let max = rect.max();
        let center = rect.center();
        let within_x = (rect.min.x..=max.x).contains(&next.x);
        let within_y = (rect.min.y..=max.y).contains(&next.y);
        // Rectangular shapes use draw.io's rectangle perimeter unless a
        // rounded perimeter is requested.
        let rectangle_perimeter = self.shape.is_none_or(|(kind, rounding)| {
            matches!(
                kind,
                ShapeKind::Rectangle
                    | ShapeKind::RoundedRectangle
                    | ShapeKind::Process
                    | ShapeKind::PredefinedProcess
            ) && !rounding.is_some_and(|rounding| rounding.rounded_perimeter)
        });
        if self.custom_outline.is_none() && rectangle_perimeter {
            // Exactly mxPerimeter.RectanglePerimeter with `orthogonal` set.
            let mut point = self.clip(next);
            if within_x {
                point.x = next.x;
            } else if within_y {
                point.y = next.y;
            }
            point.x = point.x.clamp(rect.min.x, max.x);
            point.y = point.y.clamp(rect.min.y, max.y);
            if next.x < rect.min.x {
                point.x = rect.min.x;
            } else if next.x > max.x {
                point.x = max.x;
            }
            if next.y < rect.min.y {
                point.y = rect.min.y;
            } else if next.y > max.y {
                point.y = max.y;
            }
            return point;
        }
        let origin = match (within_x, within_y) {
            (true, false) => Vec2::new(next.x, center.y),
            (false, true) => Vec2::new(center.x, next.y),
            _ => return self.clip(next),
        };
        if origin == center {
            return self.clip(next);
        }
        let outline = match (&self.custom_outline, self.shape) {
            (Some(outline), _) if outline.len() >= 3 => outline.clone(),
            (_, Some((kind, rounding))) => {
                shape_geometry_with_rounding(kind, rect, 0.0, rounding).outline
            }
            _ => vec![
                rect.min,
                Vec2::new(max.x, rect.min.y),
                max,
                Vec2::new(rect.min.x, max.y),
            ],
        };
        if point_in_polygon(origin, &outline)
            && let Some(point) = clip_to_polygon(&outline, origin, next)
        {
            return point;
        }
        self.clip(next)
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
        custom_outline: None,
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
        Some(ShapeKind::Ellipse | ShapeKind::DoubleEllipse | ShapeKind::FinalState) => {
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
        Some(shape) => shape_outline_perimeter(rect, shape, point),
    }
}

fn shape_outline_perimeter(rect: Rect, shape: ShapeKind, point: Vec2) -> Option<Vec2> {
    let center = rect.center();
    let direction = point - center;
    if !direction.x.is_finite()
        || !direction.y.is_finite()
        || direction == Vec2::ZERO
        || rect.size.x <= 0.0
        || rect.size.y <= 0.0
    {
        return None;
    }

    let outline = shape.unit_outline();
    if outline.len() < 3 {
        return None;
    }

    let mut nearest = None;
    let mut nearest_t = f64::INFINITY;
    for (index, start) in outline.iter().copied().enumerate() {
        let end = outline[(index + 1) % outline.len()];
        let a = rect.min + Vec2::new(start.x * rect.size.x, start.y * rect.size.y);
        let b = rect.min + Vec2::new(end.x * rect.size.x, end.y * rect.size.y);
        let edge = b - a;
        let offset = a - center;
        let cross =
            f64::from(direction.x) * f64::from(edge.y) - f64::from(direction.y) * f64::from(edge.x);
        if cross.abs() <= f64::EPSILON {
            continue;
        }
        let t = (f64::from(offset.x) * f64::from(edge.y) - f64::from(offset.y) * f64::from(edge.x))
            / cross;
        let u = (f64::from(offset.x) * f64::from(direction.y)
            - f64::from(offset.y) * f64::from(direction.x))
            / cross;
        if t >= 0.0 && (0.0..=1.0).contains(&u) && t < nearest_t {
            nearest_t = t;
            nearest = Some(center + direction * t as f32);
        }
    }
    nearest
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
    if is_drawio_loop(connector) {
        return loop_path(connector, a, b);
    }
    if connector.route.is_drawio_edge_style() {
        return drawio_edge_style_path(connector, a, b);
    }
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
        ConnectorRoute::ElbowHorizontal
        | ConnectorRoute::ElbowVertical
        | ConnectorRoute::SideToSide
        | ConnectorRoute::TopToBottom
        | ConnectorRoute::EntityRelation
        | ConnectorRoute::Segment
        | ConnectorRoute::IsometricHorizontal
        | ConnectorRoute::IsometricVertical => drawio_edge_style_path(connector, a, b),
    }
}

/// draw.io's default grid size, the `segment` of its loop router.
const LOOP_SEGMENT: f32 = 10.0;

/// Whether draw.io's `Loop` router replaces the edge style, as it does for
/// floating self-loops with fewer than two waypoints (mxGraphView's
/// `isLoopStyleEnabled`).
fn is_drawio_loop(connector: &CanvasConnector) -> bool {
    connector.from == connector.to
        && !matches!(connector.from, Endpoint::Point(_))
        && connector.waypoints.len() < 2
        && connector.from_port.is_none()
        && connector.to_port.is_none()
}

/// The two bends of draw.io's `Loop` router with its default west direction:
/// a loop beside the right edge, or through the first waypoint as a hint.
fn loop_points(source: Rect, hint: Option<Vec2>) -> [Vec2; 2] {
    let center = source.center();
    let max = source.max();
    let hint = hint.filter(|&hint| !rect_contains(source, hint));
    let (x, dx, y, dy) = match hint {
        Some(hint) if (source.min.x..=max.x).contains(&hint.x) => (
            center.x,
            (center.x - hint.x).abs().max(LOOP_SEGMENT),
            hint.y,
            0.0,
        ),
        Some(hint) => (
            hint.x,
            0.0,
            center.y,
            (center.y - hint.y).abs().max(LOOP_SEGMENT),
        ),
        None => (max.x + 2.0 * LOOP_SEGMENT, 0.0, center.y, LOOP_SEGMENT),
    };
    [Vec2::new(x - dx, y - dy), Vec2::new(x + dx, y + dy)]
}

/// A self-loop routed like draw.io: through the loop bends, smoothed for curves.
fn loop_path(connector: &CanvasConnector, a: EndTarget, b: EndTarget) -> Vec<Vec2> {
    let mut looped = connector.clone();
    looped.waypoints = loop_points(a.rect, connector.waypoints.first().copied())
        .to_vec()
        .into();
    if looped.route != ConnectorRoute::Curve {
        looped.route = ConnectorRoute::Straight;
    }
    routed_waypoints(&looped, a, b)
}

/// draw.io's default `segment` for entity-relation edges.
const ENTITY_RELATION_SEGMENT: f32 = 30.0;

fn rect_contains(rect: Rect, point: Vec2) -> bool {
    let max = rect.max();
    point.x >= rect.min.x && point.x <= max.x && point.y >= rect.min.y && point.y <= max.y
}

/// JavaScript's `Math.round`, which rounds halves towards positive infinity.
fn js_round(value: f32) -> f32 {
    (value + 0.5).floor()
}

/// Interior points of draw.io's `SideToSide` router (mxEdgeStyle.js).
fn side_to_side_points(source: Rect, target: Rect, hint: Option<Vec2>) -> Vec<Vec2> {
    let (source_max, target_max) = (source.max(), target.max());
    let left = source.min.x.max(target.min.x);
    let right = source_max.x.min(target_max.x);
    let x = hint.map_or_else(|| js_round(right + (left - right) / 2.0), |hint| hint.x);
    let mut y1 = source.center().y;
    let mut y2 = target.center().y;
    if let Some(hint) = hint {
        if (source.min.y..=source_max.y).contains(&hint.y) {
            y1 = hint.y;
        }
        if (target.min.y..=target_max.y).contains(&hint.y) {
            y2 = hint.y;
        }
    }
    let outside = |point| !rect_contains(source, point) && !rect_contains(target, point);
    let mut points: Vec<Vec2> = [y1, y2]
        .into_iter()
        .map(|y| Vec2::new(x, y))
        .filter(|&point| outside(point))
        .collect();
    if points.len() == 1 {
        match hint {
            Some(hint) => {
                let point = Vec2::new(x, hint.y);
                if outside(point) {
                    points.push(point);
                }
            }
            None => {
                let top = source.min.y.max(target.min.y);
                let bottom = source_max.y.min(target_max.y);
                points.push(Vec2::new(x, top + (bottom - top) / 2.0));
            }
        }
    }
    points
}

/// Interior points of draw.io's `TopToBottom` router (mxEdgeStyle.js).
fn top_to_bottom_points(source: Rect, target: Rect, hint: Option<Vec2>) -> Vec<Vec2> {
    let (source_max, target_max) = (source.max(), target.max());
    let top = source.min.y.max(target.min.y);
    let bottom = source_max.y.min(target_max.y);
    let x1 = hint
        .filter(|hint| (source.min.x..=source_max.x).contains(&hint.x))
        .map_or(source.center().x, |hint| hint.x);
    let y = hint.map_or_else(|| js_round(bottom + (top - bottom) / 2.0), |hint| hint.y);
    let x2 = hint
        .filter(|hint| (target.min.x..=target_max.x).contains(&hint.x))
        .map_or(target.center().x, |hint| hint.x);
    let outside = |point| !rect_contains(source, point) && !rect_contains(target, point);
    let mut points: Vec<Vec2> = [x1, x2]
        .into_iter()
        .map(|x| Vec2::new(x, y))
        .filter(|&point| outside(point))
        .collect();
    if points.len() == 1 {
        match hint {
            Some(hint) => {
                let point = Vec2::new(hint.x, y);
                if outside(point) {
                    points.push(point);
                }
            }
            None => {
                let left = source.min.x.max(target.min.x);
                let right = source_max.x.min(target_max.x);
                points.push(Vec2::new(left + (right - left) / 2.0, y));
            }
        }
    }
    points
}

/// draw.io's `ElbowConnector`: side-to-side unless the hint or the terminal
/// overlap demands top-to-bottom, or `elbow=vertical` prefers it.
fn elbow_points(source: Rect, target: Rect, hint: Option<Vec2>, vertical: bool) -> Vec<Vec2> {
    let (source_max, target_max) = (source.max(), target.max());
    let (force_vertical, force_horizontal) = match hint {
        Some(hint) => {
            let left = source.min.x.min(target.min.x);
            let right = source_max.x.max(target_max.x);
            let top = source.min.y.min(target.min.y);
            let bottom = source_max.y.max(target_max.y);
            (
                hint.y < top || hint.y > bottom,
                hint.x < left || hint.x > right,
            )
        }
        None => {
            let left = source.min.x.max(target.min.x);
            let right = source_max.x.min(target_max.x);
            let force_vertical = left == right;
            let top = source.min.y.max(target.min.y);
            let bottom = source_max.y.min(target_max.y);
            (force_vertical, !force_vertical && top == bottom)
        }
    };
    if !force_horizontal && (force_vertical || vertical) {
        top_to_bottom_points(source, target, hint)
    } else {
        side_to_side_points(source, target, hint)
    }
}

/// Interior points of draw.io's `EntityRelation` router, which leaves both
/// terminals horizontally through a fixed-length segment and ignores hints.
fn entity_relation_points(source: Rect, target: Rect) -> Vec<Vec2> {
    let segment = ENTITY_RELATION_SEGMENT;
    let (source_max, target_max) = (source.max(), target.max());
    let source_left = target_max.x < source.min.x;
    let target_left = source_max.x < target.min.x;
    let x0 = if source_left {
        source.min.x
    } else {
        source_max.x
    };
    let xe = if target_left {
        target.min.x
    } else {
        target_max.x
    };
    let (y0, ye) = (source.center().y, target.center().y);
    let departure = Vec2::new(x0 + if source_left { -segment } else { segment }, y0);
    let arrival = Vec2::new(xe + if target_left { -segment } else { segment }, ye);
    if source_left == target_left {
        let x = if source_left {
            x0.min(xe) - segment
        } else {
            x0.max(xe) + segment
        };
        vec![Vec2::new(x, y0), Vec2::new(x, ye)]
    } else if (departure.x < arrival.x) == source_left {
        let middle = y0 + (ye - y0) / 2.0;
        vec![
            departure,
            Vec2::new(departure.x, middle),
            Vec2::new(arrival.x, middle),
            arrival,
        ]
    } else {
        vec![departure, arrival]
    }
}

/// Interior points of draw.io's `SegmentConnector`, where each waypoint fixes
/// the position of the next segment, alternating between horizontal and
/// vertical segments. Zero-sized terminals are fixed points, like draw.io's
/// absolute terminal points.
fn segment_points(source: Rect, target: Rect, waypoints: &[Vec2]) -> Vec<Vec2> {
    const TOLERANCE: f32 = 1.0;
    let fixed = |rect: Rect| (rect.size == Vec2::ZERO).then(|| rect.center());
    let (p0, pe) = (fixed(source), fixed(target));
    let (source_max, target_max) = (source.max(), target.max());
    let outside_y = |rect: Rect, max: Vec2, y: f32| y < rect.min.y || y > max.y;
    let outside_x = |rect: Rect, max: Vec2, x: f32| x < rect.min.x || x > max.x;
    let mut temp = Vec::new();
    let mut horizontal = true;
    let mut pt = source.center();
    let mut hint = pt;
    if !waypoints.is_empty() {
        let mut hints = waypoints.to_vec();
        if (hints[0].x - pt.x).abs() < TOLERANCE {
            hints[0].x = pt.x;
        }
        if (hints[0].y - pt.y).abs() < TOLERANCE {
            hints[0].y = pt.y;
        }
        if let Some(pe) = pe {
            let last = hints.last_mut().expect("hints are not empty");
            if (last.x - pe.x).abs() < TOLERANCE {
                last.x = pe.x;
            }
            if (last.y - pe.y).abs() < TOLERANCE {
                last.y = pe.y;
            }
        }
        hint = hints[0];
        // Orientation of the first segment from alignment with fixed points or
        // the terminal channels, trying the source end and then the target end.
        let (mut terminal, mut channel, mut current, mut current_hint) =
            (p0.is_none(), source, p0, hint);
        for end in 0..2 {
            let fixed_vertical = current.is_some_and(|point| point.x == current_hint.x);
            let fixed_horizontal = current.is_some_and(|point| point.y == current_hint.y);
            let channel_max = channel.max();
            let in_horizontal_channel =
                terminal && (channel.min.y..=channel_max.y).contains(&current_hint.y);
            let in_vertical_channel =
                terminal && (channel.min.x..=channel_max.x).contains(&current_hint.x);
            let horizontal_channel =
                fixed_horizontal || (current.is_none() && in_horizontal_channel);
            let vertical_channel = fixed_vertical || (current.is_none() && in_vertical_channel);
            let ambiguous =
                (horizontal_channel && vertical_channel) || (fixed_vertical && fixed_horizontal);
            if !(end == 0 && ambiguous) && (vertical_channel || horizontal_channel) {
                horizontal = if end == 1 && hints.len() % 2 == 1 {
                    vertical_channel
                } else {
                    horizontal_channel
                };
                break;
            }
            (terminal, channel, current) = (pe.is_none(), target, pe);
            current_hint = *hints.last().expect("hints are not empty");
            if fixed_vertical && fixed_horizontal {
                hints.remove(0);
            }
        }
        if horizontal && p0.map_or(outside_y(source, source_max, hint.y), |p0| p0.y != hint.y) {
            temp.push(Vec2::new(pt.x, hint.y));
        } else if !horizontal
            && p0.map_or(outside_x(source, source_max, hint.x), |p0| p0.x != hint.x)
        {
            temp.push(Vec2::new(hint.x, pt.y));
        }
        if horizontal {
            pt.y = hint.y;
        } else {
            pt.x = hint.x;
        }
        for &next in &hints {
            horizontal = !horizontal;
            hint = next;
            if horizontal {
                pt.y = hint.y;
            } else {
                pt.x = hint.x;
            }
            temp.push(pt);
        }
    }
    let last = pe.unwrap_or(target.center());
    if horizontal && pe.map_or(outside_y(target, target_max, hint.y), |pe| pe.y != hint.y) {
        temp.push(Vec2::new(last.x, hint.y));
    } else if !horizontal && pe.map_or(outside_x(target, target_max, hint.x), |pe| pe.x != hint.x) {
        temp.push(Vec2::new(hint.x, last.y));
    }
    // Bends inside a floating terminal are dropped.
    if p0.is_none() {
        let inside = temp
            .iter()
            .take_while(|&&point| rect_contains(source, point))
            .count();
        temp.drain(..inside);
    }
    if pe.is_none() {
        while temp
            .last()
            .is_some_and(|&point| rect_contains(target, point))
        {
            temp.pop();
        }
    }
    let mut result: Vec<Vec2> = Vec::with_capacity(temp.len());
    for point in temp {
        let point = Vec2::new(
            js_round(point.x * 10.0) / 10.0,
            js_round(point.y * 10.0) / 10.0,
        );
        if result.last().is_none_or(|last| {
            (last.x - point.x).abs() >= TOLERANCE || (last.y - point.y).abs() >= TOLERANCE
        }) {
            result.push(point);
        }
    }
    if let Some(pe) = pe
        && result.last().is_some_and(|last| {
            (last.x - pe.x).abs() <= TOLERANCE && (last.y - pe.y).abs() <= TOLERANCE
        })
    {
        result.pop();
        if let Some(last) = result.last_mut() {
            if (last.x - pe.x).abs() < TOLERANCE {
                last.x = pe.x;
            }
            if (last.y - pe.y).abs() < TOLERANCE {
                last.y = pe.y;
            }
        }
    }
    result
}

/// Interior points of draw.io's `IsometricConnector`: from the source centre
/// along one isometric axis and then the other to the hint (or the midpoint),
/// then on along the second axis, leaving the last span to the target on the
/// first axis. Bends that coincide with a terminal centre are dropped so
/// that the ends still leave along an axis.
fn isometric_points(source: Rect, target: Rect, hint: Option<Vec2>, vertical: bool) -> Vec<Vec2> {
    let (p0, pe) = (source.center(), target.center());
    let angle = |degrees: f32| {
        let radians = degrees.to_radians();
        Vec2::new(radians.cos(), radians.sin())
    };
    let (a, b) = (angle(-30.0), angle(-150.0));
    let mut result = Vec::with_capacity(3);
    let mut last = p0;
    let mut line_to = |to: Vec2, both: bool| {
        let c = to - last;
        let h = (b.y * c.x - b.x * c.y) / (a.x * b.y - a.y * b.x);
        let v = (a.y * c.x - a.x * c.y) / (a.y * b.x - a.x * b.y);
        let (first, second) = if vertical {
            (b * v, a * h)
        } else {
            (a * h, b * v)
        };
        if both {
            last = last + first;
            result.push(last);
        }
        last = last + second;
        result.push(last);
    };
    let hint = hint.unwrap_or(p0 + (pe - p0) * 0.5);
    line_to(hint, true);
    line_to(pe, false);
    let near = |p: Vec2, q: Vec2| (p - q).length() < 1e-3;
    let mut points: Vec<Vec2> = Vec::with_capacity(result.len());
    for point in result {
        if !near(point, p0) && !near(point, pe) && points.last().is_none_or(|&l| !near(l, point)) {
            points.push(point);
        }
    }
    points
}

/// Path of a connector using one of draw.io's routed edge styles. Like draw.io,
/// only the first waypoint is a routing hint for elbow and isometric styles,
/// segment routes use every waypoint, and entity relations ignore them.
fn drawio_edge_style_path(connector: &CanvasConnector, a: EndTarget, b: EndTarget) -> Vec<Vec2> {
    let hint = connector
        .route
        .uses_waypoint_hint()
        .then(|| connector.waypoints.first().copied())
        .flatten();
    let (source, target) = (a.rect, b.rect);
    let interior = match connector.route {
        ConnectorRoute::ElbowHorizontal => elbow_points(source, target, hint, false),
        ConnectorRoute::ElbowVertical => elbow_points(source, target, hint, true),
        ConnectorRoute::SideToSide => side_to_side_points(source, target, hint),
        ConnectorRoute::TopToBottom => top_to_bottom_points(source, target, hint),
        ConnectorRoute::EntityRelation => entity_relation_points(source, target),
        ConnectorRoute::Segment => segment_points(source, target, &connector.waypoints),
        ConnectorRoute::IsometricHorizontal => isometric_points(source, target, hint, false),
        ConnectorRoute::IsometricVertical => isometric_points(source, target, hint, true),
        ConnectorRoute::Straight | ConnectorRoute::Elbow | ConnectorRoute::Curve => Vec::new(),
    };
    // Like mxGraphView, the source end aims at the first route point (or the
    // target centre) and the target end at the point preceding it. Isometric
    // routes are not orthogonal, so their ends clip from the centre.
    let orthogonal = !matches!(
        connector.route,
        ConnectorRoute::IsometricHorizontal | ConnectorRoute::IsometricVertical
    );
    let clip = |end: &EndTarget, next: Vec2| {
        if orthogonal {
            end.clip_orthogonal(next)
        } else {
            end.clip(next)
        }
    };
    let start = clip(&a, interior.first().copied().unwrap_or(target.center()));
    let end = clip(&b, interior.last().copied().unwrap_or(start));
    let mut points = Vec::with_capacity(interior.len() + 2);
    push_distinct(&mut points, start);
    for point in interior {
        push_distinct(&mut points, point);
    }
    push_distinct(&mut points, end);
    if points.len() == 1 {
        points.push(points[0]);
    }
    points
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
    collapse: Option<&'a CollapsePreview<'a>>,
}

impl<'a> ConnectorResolver<'a> {
    fn new(sheet: &'a Sheet, layout: &'a Layout) -> Self {
        Self::with_collapse(sheet, layout, None)
    }

    fn with_collapse(
        sheet: &'a Sheet,
        layout: &'a Layout,
        collapse: Option<&'a CollapsePreview<'a>>,
    ) -> Self {
        let mut objects: HashMap<_, _> = sheet.canvas.objects.iter().map(|o| (o.id, o)).collect();
        if let Some(collapse) = collapse {
            objects.extend(collapse.compact.iter().map(|(id, object)| (*id, object)));
        }
        Self {
            sheet,
            layout,
            objects,
            paths: HashMap::new(),
            collapse,
        }
    }

    fn visible_target(&self, id: CanvasObjectId) -> Option<CanvasObjectId> {
        self.collapse
            .map_or(Some(id), |collapse| collapse.visible_representative(id))
    }

    fn endpoint(&self, end: Endpoint, port: Option<ConnectorPort>) -> Option<EndTarget> {
        let (end, port) = if let Endpoint::Object(id) = end {
            let visible = self.visible_target(id)?;
            (
                Endpoint::Object(visible),
                (visible == id).then_some(port).flatten(),
            )
        } else {
            (end, port)
        };
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
                        custom_outline: crate::stencil_object::render(object)
                            .map(|render| crate::stencil_object::geometry(object, &render).outline),
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
                    && let Some(target) = self.visible_target(target)
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
                    && let Some(target) = self.visible_target(target)
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
    let (order, count) = connector_dependency_order(sheet);
    if order.len() == count {
        Ok(())
    } else {
        Err("connector dependencies contain a cycle".into())
    }
}

fn connector_dependency_order(sheet: &Sheet) -> (Vec<CanvasObjectId>, usize) {
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
    let mut order = Vec::with_capacity(connectors.len());
    while let Some(id) = ready.pop_front() {
        order.push(id);
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
    (order, connectors.len())
}

fn connector_targets(
    sheet: &Sheet,
    layout: &Layout,
    connector: &CanvasConnector,
) -> Option<(EndTarget, EndTarget)> {
    let collapse = sheet
        .has_scene_collapse()
        .then(|| CollapsePreview::stored(sheet));
    let mut resolver = ConnectorResolver::with_collapse(sheet, layout, collapse.as_ref());
    for end in [connector.from, connector.to] {
        if let Endpoint::Object(id) = end
            && let Some(id) = resolver.visible_target(id)
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
    let looped = is_drawio_loop(connector);
    let hinted = connector.route.uses_waypoint_hint() || connector.route == ConnectorRoute::Segment;
    if (connector.route == ConnectorRoute::EntityRelation && !looped)
        || ((hinted || looped) && !connector.waypoints.is_empty())
    {
        return Vec::new();
    }
    if connector.waypoints.is_empty() {
        let path = routed_path(connector, from, to);
        if connector.route == ConnectorRoute::Curve || hinted || looped {
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

mod edge_shape;
pub use edge_shape::EdgeBody;
