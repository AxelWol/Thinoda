//! Paint-order scene of a sheet: zones, canvas objects and laid out topics in bands,
//! with rotated hit testing shared by the canvas, exports and tests. See
//! `docs/FREEFORM.md`.

use crate::board::{
    Band, CanvasConnector, CanvasObject, CanvasObjectId, CanvasObjectKind, ConnectorMarkerKind,
    ConnectorRoute, DEFAULT_CONNECTOR_MARKER_SIZE, Endpoint, GridKind, ShapeKind,
    distance_to_outline, distance_to_polyline, point_in_polygon, points_bounds, rotate_around,
    rotated_bounds, rotated_rect_contains, shape_geometry,
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
    points: [Vec2; 4],
    point_count: usize,
    pub closed: bool,
    pub filled: bool,
    pub line_inset: f32,
}

impl ConnectorMarkerGeometry {
    /// The marker outline, including the notch of a classic arrow.
    pub fn points(&self) -> &[Vec2] {
        &self.points[..self.point_count]
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
    let offset = width * 1.118;
    let size = marker_size + width;
    let tip = tip - direction * offset;
    let base = tip - direction * size;
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
        points: [
            base + normal * wing,
            tip,
            base - normal * wing,
            tip - direction * (size * 0.75),
        ],
        point_count: if classic { 4 } else { 3 },
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
                        (marker.filled && point_in_polygon(p, marker.points()))
                            || if marker.closed {
                                distance_to_outline(p, marker.points()) <= radius
                            } else {
                                distance_to_polyline(p, marker.points()) <= radius
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
            let g = shape_geometry(*shape, object.rect, object.rotation);
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
    let bounds = geometry_bounds(&geometry)?;
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
                if let Some(marker_bounds) = points_bounds(marker.points()) {
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

impl Scene {
    /// Top-most topic or object under `p`.
    pub fn hit_test(&self, p: Vec2) -> Option<SceneHit> {
        self.hit_test_tolerance(p, HIT_TOLERANCE)
    }

    /// Top-most topic or object within `tolerance` of `p`.
    pub fn hit_test_tolerance(&self, p: Vec2, tolerance: f32) -> Option<SceneHit> {
        self.items.iter().rev().find_map(|item| {
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
    shape: Option<ShapeKind>,
}

impl EndTarget {
    fn boxed(rect: Rect, rotation: f32) -> Self {
        Self {
            rect,
            rotation,
            shape: None,
        }
    }

    /// Point on the visible perimeter in the direction of `toward`, like draw.io's
    /// shape perimeters; boxes and rectangular shapes clip to their rotated rect.
    fn clip(self, toward: Vec2) -> Vec2 {
        let rectangular = |kind| {
            matches!(
                kind,
                ShapeKind::Rectangle | ShapeKind::Process | ShapeKind::PredefinedProcess
            )
        };
        self.shape
            .filter(|kind| !rectangular(*kind))
            .and_then(|kind| {
                let outline = shape_geometry(kind, self.rect, self.rotation).outline;
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

    fn endpoint(&self, end: Endpoint) -> Option<EndTarget> {
        match end {
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
                            CanvasObjectKind::Shape { shape, .. } => Some(*shape),
                            _ => None,
                        },
                    })
                }
            }
        }
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
                    .endpoint(connector.from)
                    .zip(self.endpoint(connector.to))
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
            self.endpoint(connector.from)
                .zip(self.endpoint(connector.to))
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
        .endpoint(connector.from)
        .zip(resolver.endpoint(connector.to))
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
