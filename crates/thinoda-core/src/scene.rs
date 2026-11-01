//! Paint-order scene of a sheet: zones, canvas objects and laid out topics in bands,
//! with rotated hit testing shared by the canvas, exports and tests. See
//! `docs/FREEFORM.md`.

use crate::board::{
    Band, CanvasConnector, CanvasObject, CanvasObjectId, CanvasObjectKind, ConnectorRoute,
    Endpoint, GridKind, distance_to_outline, distance_to_polyline, point_in_polygon, points_bounds,
    rotate_around, rotated_bounds, rotated_rect_contains, shape_geometry,
};
use crate::geometry::{Rect, Vec2};
use crate::layout::{Layout, clip_to_rect, cubic};
use crate::model::{Sheet, TopicId, ZoneId};
use std::{collections::HashSet, sync::Arc};

/// Default hit tolerance in sheet units.
pub const HIT_TOLERANCE: f32 = 3.0;
/// Segments used to flatten curved connectors.
const CURVE_SEGMENTS: usize = 16;

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
    Polyline { points: Vec<Vec2>, width: f32 },
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
            SceneGeometry::Polyline { points, width } => {
                distance_to_polyline(p, points) <= width / 2.0 + tolerance
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
    let objects = |band: Band, items: &mut Vec<SceneItem>| {
        for object in sheet.canvas.objects.iter().filter(|o| o.band == band) {
            if let Some(item) = object_item(sheet, layout, object) {
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

fn object_item(sheet: &Sheet, layout: &Layout, object: &CanvasObject) -> Option<SceneItem> {
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
            let points = connector_path(sheet, layout, c);
            if points.len() < 2 {
                return None;
            }
            SceneGeometry::Polyline {
                points,
                width: c.width,
            }
        }
    };
    let bounds = geometry_bounds(&geometry)?;
    Some(SceneItem {
        id: SceneId::Object(object.id),
        band,
        bounds,
        clip: object_clip_rect(sheet, object),
        geometry,
    })
}

/// Clipping inherited from the frame containing a floating topic tree.
pub fn topic_clip_rect(sheet: &Sheet, topic: TopicId) -> Option<Rect> {
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
            return Some(zone.frame);
        }
        current = sheet
            .topic(id)
            .and_then(|topic| topic.parent)
            .or_else(|| sheet.summary_for_topic(id).map(|summary| summary.parent));
    }
    None
}

/// Clipping from the first clipping frame containing the object's center.
pub fn object_clip_rect(sheet: &Sheet, object: &CanvasObject) -> Option<Rect> {
    sheet
        .zones
        .iter()
        .find(|zone| zone.clip && zone.frame.contains(object.rect.center()))
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
        SceneGeometry::Polyline { points, width } => {
            Some(points_bounds(points)?.expand(width / 2.0))
        }
    }
}

/// Painted bounds of a canvas object (`None` for an unresolvable connector).
pub fn object_bounds(sheet: &Sheet, layout: &Layout, object: &CanvasObject) -> Option<Rect> {
    object_item(sheet, layout, object).and_then(|item| item.visible_bounds())
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
    let mut current = Some(id);
    while let Some(t) = current {
        if sheet.is_anchor(t) {
            return None;
        }
        if let Some(r) = layout.rect(t) {
            return Some(r);
        }
        current = sheet.topic(t)?.parent;
    }
    None
}

/// Resolved endpoint: rect (zero-size for points) and rotation.
fn resolve_endpoint(sheet: &Sheet, layout: &Layout, end: Endpoint) -> Option<(Rect, f32)> {
    match end {
        Endpoint::Point(p) => Some((Rect::from_center_size(p, Vec2::ZERO), 0.0)),
        Endpoint::Topic(t) => {
            let rect = topic_anchor_rect(sheet, layout, t)?;
            let rotation = if layout.rect(t).is_some() {
                sheet
                    .topic(t)
                    .and_then(|t| t.card.as_ref())
                    .map_or(0.0, |c| c.rotation)
            } else {
                0.0
            };
            Some((rect, rotation))
        }
        Endpoint::Object(x) => {
            let o = sheet.canvas.object(x)?;
            if o.is_connector() {
                Some((Rect::from_center_size(o.rect.center(), Vec2::ZERO), 0.0))
            } else {
                Some((o.rect, o.rotation))
            }
        }
    }
}

/// Border point of a (rotated) rect toward `toward`.
fn clip_rotated(rect: Rect, rotation: f32, toward: Vec2) -> Vec2 {
    if rect.size.x <= 0.0 && rect.size.y <= 0.0 {
        return rect.center();
    }
    let c = rect.center();
    let local = rotate_around(toward, c, -rotation);
    rotate_around(clip_to_rect(rect, local), c, rotation)
}

/// Polyline of a free connector, clipped to its endpoint targets. Empty if an endpoint
/// cannot be resolved (dangling reference or the hidden anchor).
pub fn connector_path(sheet: &Sheet, layout: &Layout, connector: &CanvasConnector) -> Vec<Vec2> {
    let (Some((ra, rot_a)), Some((rb, rot_b))) = (
        resolve_endpoint(sheet, layout, connector.from),
        resolve_endpoint(sheet, layout, connector.to),
    ) else {
        return Vec::new();
    };
    let (ca, cb) = (ra.center(), rb.center());
    match connector.route {
        ConnectorRoute::Straight => {
            vec![clip_rotated(ra, rot_a, cb), clip_rotated(rb, rot_b, ca)]
        }
        ConnectorRoute::Elbow => {
            let a = clip_rotated(ra, rot_a, Vec2::new(cb.x, ca.y));
            let b = clip_rotated(rb, rot_b, Vec2::new(ca.x, cb.y));
            let mx = (a.x + b.x) / 2.0;
            vec![a, Vec2::new(mx, a.y), Vec2::new(mx, b.y), b]
        }
        ConnectorRoute::Curve => {
            let a = clip_rotated(ra, rot_a, cb);
            let b = clip_rotated(rb, rot_b, ca);
            let dx = (b.x - a.x) / 2.0;
            let (c1, c2) = (a + Vec2::new(dx, 0.0), b - Vec2::new(dx, 0.0));
            (0..=CURVE_SEGMENTS)
                .map(|i| cubic(a, c1, c2, b, i as f32 / CURVE_SEGMENTS as f32))
                .collect()
        }
    }
}

#[cfg(test)]
mod tests;
