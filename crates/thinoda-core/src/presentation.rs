//! Live presentation resolution without changing the map's layout geometry.

use std::collections::{HashMap, HashSet};

use crate::{
    CanvasObjectId, CanvasObjectKind, ChildRange, Endpoint, Layout, PresentationScope,
    PresentationSlide, PresentationSource, Rect, SceneId, Sheet, TopicId, Vec2, Zone, build_scene,
};

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedPresentationSlide {
    pub layout: Layout,
    pub frame: Rect,
    /// Zones are not stored in `Layout`; consumers must paint this list, not `sheet.zones`.
    pub zones: Vec<Zone>,
    /// Topics permitted by source, visibility and depth before collapsed layout filtering.
    pub permitted_topics: HashSet<TopicId>,
    /// Explicit Zone objects for source-only slides, or all objects in contextual views.
    pub permitted_objects: HashSet<crate::CanvasObjectId>,
}

impl ResolvedPresentationSlide {
    /// Whether a scene connector may expose this endpoint. Collapsed descendants
    /// can use their visible collapsed ancestor, but hidden or unrevealed topics cannot.
    pub fn scene_topic_visible(&self, sheet: &Sheet, id: TopicId) -> bool {
        scene_topic_visible(sheet, &self.permitted_topics, &self.layout, id)
    }

    /// Canvas objects visible in this slide, including eligible connector endpoints.
    /// Objects not assigned to a Zone remain visible in source-only slides.
    pub fn scene_canvas_object_ids(&self, sheet: &Sheet) -> HashSet<CanvasObjectId> {
        scene_canvas_object_ids(
            sheet,
            &self.permitted_objects,
            &self.permitted_topics,
            &self.layout,
        )
    }
}

fn scene_topic_visible(
    sheet: &Sheet,
    permitted_topics: &HashSet<TopicId>,
    layout: &Layout,
    id: TopicId,
) -> bool {
    if !permitted_topics.contains(&id) {
        return false;
    }
    let mut current = Some(id);
    let mut visible = id;
    let mut seen = HashSet::new();
    while let Some(topic_id) = current {
        if !seen.insert(topic_id) || sheet.is_anchor(topic_id) {
            return false;
        }
        let Some(topic) = sheet.topic(topic_id) else {
            return false;
        };
        if topic.collapsed {
            visible = topic_id;
        }
        current = topic.parent.or_else(|| {
            sheet
                .summary_for_topic(topic_id)
                .map(|summary| summary.parent)
        });
    }
    layout.topics.contains_key(&visible)
}

fn scene_canvas_object_ids(
    sheet: &Sheet,
    permitted_objects: &HashSet<CanvasObjectId>,
    permitted_topics: &HashSet<TopicId>,
    layout: &Layout,
) -> HashSet<CanvasObjectId> {
    let mut excluded: HashSet<_> = sheet
        .zones
        .iter()
        .flat_map(|zone| zone.objects.iter().copied())
        .filter(|id| !permitted_objects.contains(id))
        .collect();
    let mut dependents: HashMap<CanvasObjectId, Vec<CanvasObjectId>> = HashMap::new();
    let mut topic_visibility = HashMap::new();
    for object in &sheet.canvas.objects {
        let CanvasObjectKind::Connector(connector) = &object.kind else {
            continue;
        };
        for endpoint in [connector.from, connector.to] {
            match endpoint {
                Endpoint::Topic(id) => {
                    if !*topic_visibility
                        .entry(id)
                        .or_insert_with(|| scene_topic_visible(sheet, permitted_topics, layout, id))
                    {
                        excluded.insert(object.id);
                    }
                }
                Endpoint::Object(id) => dependents.entry(id).or_default().push(object.id),
                Endpoint::Point(_) => {}
            }
        }
    }
    let mut pending: Vec<_> = excluded.iter().copied().collect();
    while let Some(id) = pending.pop() {
        if let Some(connectors) = dependents.get(&id) {
            for &connector in connectors {
                if excluded.insert(connector) {
                    pending.push(connector);
                }
            }
        }
    }
    sheet
        .canvas
        .objects
        .iter()
        .filter(|object| !excluded.contains(&object.id))
        .map(|object| object.id)
        .collect()
}

/// Intersects conservative presentation bounds with an optional visual clip.
/// Degenerate intersections are retained because a stroked line can lie on a
/// clip edge even when its geometric bounds have no width or height.
pub fn clip_presentation_bounds(bounds: Rect, clip: Option<Rect>) -> Option<Rect> {
    let Some(clip) = clip else {
        return Some(bounds);
    };
    let bounds_max = bounds.max();
    let clip_max = clip.max();
    let min = Vec2::new(bounds.min.x.max(clip.min.x), bounds.min.y.max(clip.min.y));
    let max = Vec2::new(bounds_max.x.min(clip_max.x), bounds_max.y.min(clip_max.y));
    if min.x > max.x || min.y > max.y {
        None
    } else {
        Some(Rect::from_min_size(min, max - min))
    }
}

/// Resolves current slide content against a full-map layout. Never relayouts a filtered sheet.
///
/// A missing/invalid source is an error, even for legacy contextual slides. Normal map
/// deletion operations drop slides owned by the deleted source, with undo restoring both.
/// Hidden topics suppress their owned subtree; stale hidden IDs are harmless.
pub fn resolve_presentation_slide(
    sheet: &Sheet,
    slide: &PresentationSlide,
    layout: &Layout,
) -> Result<ResolvedPresentationSlide, String> {
    let saved_max = slide.frame.max();
    let limit = crate::MAX_PRESENTATION_COORDINATE;
    if !slide.frame.min.x.is_finite()
        || !slide.frame.min.y.is_finite()
        || !saved_max.x.is_finite()
        || !saved_max.y.is_finite()
        || !slide.frame.size.x.is_finite()
        || !slide.frame.size.y.is_finite()
        || slide.frame.size.x < 1.0
        || slide.frame.size.y < 1.0
        || slide.frame.size.x > limit
        || slide.frame.size.y > limit
        || slide.frame.min.x.abs() > limit
        || slide.frame.min.y.abs() > limit
        || saved_max.x.abs() > limit
        || saved_max.y.abs() > limit
    {
        return Err("presentation frame has invalid dimensions".into());
    }
    let roots = match slide.source {
        PresentationSource::Branch(root) if sheet.is_presentation_branch(root) => vec![root],
        PresentationSource::Branch(root) => {
            return Err(format!(
                "presentation source branch {root} is missing or invalid"
            ));
        }
        PresentationSource::Zone(id) => sheet
            .zones
            .iter()
            .find(|zone| zone.id == id)
            .ok_or_else(|| format!("presentation source zone {id} is missing"))?
            .roots
            .clone(),
        // Everything inside the frame; the frame crops the content.
        PresentationSource::Viewport => sheet.all_content_roots(),
    };
    let hidden: HashSet<_> = slide
        .hidden_topics
        .iter()
        .filter(|id| sheet.contains(**id))
        .flat_map(|id| sheet.owned_subtree_ids(*id))
        .collect();
    let permitted_objects: HashSet<_> = if slide.source == PresentationSource::Viewport
        || slide.scope == PresentationScope::Context
    {
        sheet
            .canvas
            .objects
            .iter()
            .map(|object| object.id)
            .collect()
    } else if let PresentationSource::Zone(id) = slide.source {
        sheet
            .zones
            .iter()
            .find(|zone| zone.id == id)
            .into_iter()
            .flat_map(|zone| zone.objects.iter().copied())
            .collect()
    } else {
        HashSet::new()
    };
    // Include detached summary trees through ownership, counting their root as
    // one level below the owning parent. Iterative traversal avoids stack limits.
    let mut depths = HashMap::new();
    let mut pending: Vec<_> = roots.iter().map(|id| (*id, 0usize)).collect();
    while let Some((id, depth)) = pending.pop() {
        if depths.get(&id).is_some_and(|previous| *previous <= depth) {
            continue;
        }
        depths.insert(id, depth);
        pending.extend(sheet.children(id).iter().map(|child| (*child, depth + 1)));
        pending.extend(
            sheet
                .summaries
                .iter()
                .filter(|s| s.parent == id)
                .map(|s| (s.topic, depth + 1)),
        );
    }
    let mut permitted: HashSet<_> = sheet
        .topics
        .keys()
        .copied()
        .filter(|id| {
            !sheet.is_anchor(*id)
                && !hidden.contains(id)
                && (slide.scope == PresentationScope::Context || depths.contains_key(id))
                && slide
                    .max_depth
                    .is_none_or(|max| depths.get(id).is_some_and(|depth| *depth <= max))
        })
        .collect();
    // Summary trees belong to their annotation, not to the canvas spatially.
    // Prune ownership transitively before filtering any primitive, so an
    // annotation attached to an excluded summary cannot leak its title either.
    loop {
        let rejected: HashSet<_> = sheet
            .summaries
            .iter()
            .filter(|s| !range_permitted(sheet, &permitted, s.parent, Some(s.range)))
            .flat_map(|s| sheet.owned_subtree_ids(s.topic))
            .collect();
        let before = permitted.len();
        permitted.retain(|id| !rejected.contains(id));
        if permitted.len() == before {
            break;
        }
    }
    let included: HashSet<_> = layout
        .topics
        .keys()
        .copied()
        .filter(|id| permitted.contains(id))
        .collect();
    // An annotation may cover collapsed descendants, but must never refer to
    // explicitly excluded content (hidden/depth/scope), even if not laid out.
    let allowed = |id: TopicId| permitted.contains(&id);
    let range_allowed = |parent: TopicId, range: Option<ChildRange>| {
        range_permitted(sheet, &permitted, parent, range)
    };
    let mut filtered = layout.clone();
    filtered.topics.retain(|id, _| included.contains(id));
    filtered.order.retain(|id| included.contains(id));
    filtered
        .connectors
        .retain(|c| included.contains(&c.parent) && included.contains(&c.child));
    filtered.relationships.retain(|r| {
        sheet.relationships.iter().any(|source| {
            source.id == r.id && included.contains(&source.from) && included.contains(&source.to)
        })
    });
    filtered.boundaries.retain(|b| {
        sheet
            .boundaries
            .iter()
            .any(|source| source.id == b.id && range_allowed(source.parent, source.range))
    });
    filtered.summaries.retain(|s| {
        sheet.summaries.iter().any(|source| {
            source.id == s.id
                && included.contains(&source.topic)
                && range_allowed(source.parent, Some(source.range))
        })
    });
    let zones: Vec<_> = sheet
        .zones
        .iter()
        .filter(|zone| {
            if zone
                .objects
                .iter()
                .any(|id| !permitted_objects.contains(id))
            {
                return false;
            }
            if zone.roots.is_empty() {
                zone.keep_empty
                    && (slide.scope == PresentationScope::Context
                        || slide.source == PresentationSource::Viewport
                        || slide.source == PresentationSource::Zone(zone.id))
            } else {
                zone.roots
                    .iter()
                    .all(|id| sheet.owned_subtree_ids(*id).into_iter().all(&allowed))
            }
        })
        .cloned()
        .collect();
    let zone_ids: HashSet<_> = zones.iter().map(|zone| zone.id).collect();
    let visible_objects = scene_canvas_object_ids(sheet, &permitted_objects, &permitted, &filtered);
    let scene = build_scene(sheet, &filtered);
    let mut bounds = scene
        .items
        .iter()
        .filter(|item| match item.id {
            SceneId::Topic(id) => included.contains(&id),
            SceneId::Zone(id) => zone_ids.contains(&id),
            SceneId::Object(id) => permitted_objects.contains(&id) && visible_objects.contains(&id),
        })
        .filter_map(|item| item.visible_bounds())
        .reduce(|a, b| a.union(&b));
    let has_clipped_frames = sheet.zones.iter().any(|zone| zone.clip);
    let topic_clips: HashMap<_, _> = if has_clipped_frames {
        scene
            .items
            .iter()
            .filter_map(|item| match item.id {
                SceneId::Topic(id) => Some((id, item.clip)),
                _ => None,
            })
            .collect()
    } else {
        HashMap::new()
    };
    let topic_clip = |id| {
        has_clipped_frames
            .then(|| {
                topic_clips
                    .get(&id)
                    .copied()
                    .unwrap_or_else(|| crate::scene::topic_clip_rect(sheet, id))
            })
            .flatten()
    };
    let mut include_rect = |rect: Rect, clip: Option<Rect>| {
        if let Some(rect) = clip_presentation_bounds(rect, clip) {
            bounds = Some(bounds.map_or(rect, |bounds| bounds.union(&rect)));
        }
    };
    // The legacy frame did not include map-edge geometry. Only add it when it
    // is clipped: both endpoint topics can be outside while the edge crosses
    // the frame and is still painted.
    if has_clipped_frames {
        for connector in &filtered.connectors {
            if let Some(clip) = topic_clip(connector.child)
                && let Some(rect) = crate::board::points_bounds(&[connector.start, connector.end])
            {
                include_rect(rect.expand(connector.width.max(0.0) / 2.0), Some(clip));
            }
        }
    }
    for boundary in &filtered.boundaries {
        let source = has_clipped_frames
            .then(|| {
                sheet
                    .boundaries
                    .iter()
                    .find(|source| source.id == boundary.id)
            })
            .flatten();
        let clip = source.and_then(|source| topic_clip(source.parent));
        let rect = if clip.is_some() {
            // Both the canvas and SVG paint a stroke beyond the rectangle.
            boundary.rect.expand(1.0)
        } else {
            boundary.rect
        };
        include_rect(rect, clip);
        // The layout does not measure boundary titles. A clipped title may
        // extend beyond the boundary rectangle, so retain its clip conservatively.
        if let Some(clip) = clip
            && source.is_some_and(|source| source.title.as_deref().is_some_and(|s| !s.is_empty()))
        {
            include_rect(clip, None);
        }
    }
    for summary in &filtered.summaries {
        let clip = topic_clip(summary.topic);
        include_rect(summary.range_rect, clip);
        if let Some(clip) = clip {
            // Clip segment hulls, not individual vertices: a visible segment
            // can cross the frame with both endpoints outside it.
            for points in [[summary.start, summary.tip], [summary.end, summary.tip]] {
                if let Some(rect) = crate::board::points_bounds(&points) {
                    include_rect(rect.expand(1.0), Some(clip));
                }
            }
            if let Some(topic) = filtered.rect(summary.topic) {
                let end = match summary.dir {
                    crate::layout::Dir::Right => topic.left_center(),
                    crate::layout::Dir::Left => topic.right_center(),
                    crate::layout::Dir::Down => topic.top_center(),
                    crate::layout::Dir::Up => topic.bottom_center(),
                };
                if let Some(rect) = crate::board::points_bounds(&[summary.tip, end]) {
                    include_rect(rect.expand(1.0), Some(clip));
                }
            }
        } else {
            for point in [summary.start, summary.end, summary.tip] {
                include_rect(Rect::from_min_size(point, Vec2::ZERO), None);
            }
        }
    }
    let mut relationship_clip_zones = HashMap::new();
    for relationship in &filtered.relationships {
        let source = has_clipped_frames
            .then(|| {
                sheet
                    .relationships
                    .iter()
                    .find(|source| source.id == relationship.id)
            })
            .flatten();
        let clip = source.and_then(|source| {
            let from = *relationship_clip_zones
                .entry(source.from)
                .or_insert_with(|| crate::scene::topic_clip_zone(sheet, source.from));
            let to = *relationship_clip_zones
                .entry(source.to)
                .or_insert_with(|| crate::scene::topic_clip_zone(sheet, source.to));
            match (from, to) {
                (Some((from_zone, clip)), Some((to_zone, _))) if from_zone == to_zone => Some(clip),
                _ => None,
            }
        });
        // Cubic control hull conservatively bounds the whole curve.
        let points = [
            relationship.start,
            relationship.c1,
            relationship.c2,
            relationship.end,
        ];
        if let Some(clip) = clip {
            if let Some(rect) = crate::board::points_bounds(&points) {
                // The canvas arrowhead can extend almost ten units from the
                // endpoint; this also covers the SVG marker and line strokes.
                include_rect(rect.expand(10.0), Some(clip));
            }
            // Relationship label extents are not measured by Layout.
            if source.is_some_and(|source| !source.title.is_empty()) {
                include_rect(clip, None);
            }
        } else {
            for point in points {
                include_rect(Rect::from_min_size(point, Vec2::ZERO), None);
            }
        }
    }
    for zone in &zones {
        include_rect(zone.frame, None);
    }
    // Empty content deliberately stays empty, with a stable saved viewport.
    // Never use the full map's bounds as an empty-slide fallback.
    let frame = if slide.auto_frame && slide.source != PresentationSource::Viewport {
        bounds.map_or(slide.frame, |b| b.expand(40.0))
    } else {
        slide.frame
    };
    filtered.bounds = bounds.unwrap_or(frame);
    if !frame.min.x.is_finite()
        || !frame.min.y.is_finite()
        || !frame.size.x.is_finite()
        || !frame.size.y.is_finite()
        || frame.size.x <= 0.0
        || frame.size.y <= 0.0
    {
        return Err("presentation frame has invalid dimensions".into());
    }
    Ok(ResolvedPresentationSlide {
        layout: filtered,
        frame,
        zones,
        permitted_topics: permitted,
        permitted_objects,
    })
}

fn range_permitted(
    sheet: &Sheet,
    permitted: &HashSet<TopicId>,
    parent: TopicId,
    range: Option<ChildRange>,
) -> bool {
    permitted.contains(&parent)
        && sheet
            .children(parent)
            .iter()
            .enumerate()
            .all(|(index, id)| {
                !range.is_none_or(|range| range.contains(index))
                    || sheet
                        .owned_subtree_ids(*id)
                        .iter()
                        .all(|id| permitted.contains(id))
            })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ApproxMeasure, SlideId, layout_sheet};

    fn slide(source: PresentationSource) -> PresentationSlide {
        PresentationSlide {
            id: SlideId(1),
            title: "Slide".into(),
            source,
            frame: Rect::from_min_size(Vec2::ZERO, Vec2::new(300.0, 200.0)),
            scope: PresentationScope::Source,
            auto_frame: true,
            hidden_topics: Vec::new(),
            max_depth: None,
        }
    }

    fn decoration_fixture() -> (Sheet, TopicId, crate::ZoneId) {
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = crate::SheetRootMode::Hidden;
        let root = sheet.add_floating("Root", Vec2::new(100.0, 100.0));
        let first = sheet.add_child(root, None, "First");
        let second = sheet.add_child(root, None, "Second");
        sheet.add_boundary(&[first, second]).unwrap();
        sheet.add_summary(&[first, second], "Summary").unwrap();
        sheet.add_relationship(first, second).unwrap();
        let zone = sheet
            .create_zone(
                "Frame",
                vec![root],
                Rect::from_min_size(Vec2::ZERO, Vec2::new(320.0, 240.0)),
            )
            .unwrap();
        sheet.zones[0].clip = true;
        sheet.validate().unwrap();
        (sheet, root, zone)
    }

    #[test]
    fn auto_frame_ignores_zone_connectors_to_foreign_objects() {
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = crate::SheetRootMode::Hidden;
        let root = sheet.add_floating("Root", Vec2::new(80.0, 80.0));
        let frame = Rect::from_min_size(Vec2::ZERO, Vec2::new(300.0, 200.0));
        let zone = sheet.create_zone("Source", vec![root], frame).unwrap();
        let other = sheet.create_board_frame("Other", frame).unwrap();
        let foreign = sheet
            .add_canvas_object(crate::CanvasObject::shape(
                crate::ShapeKind::Rectangle,
                Rect::from_min_size(Vec2::new(10_000.0, 100.0), Vec2::new(80.0, 50.0)),
            ))
            .unwrap();
        let unassigned = sheet
            .add_canvas_object(crate::CanvasObject::shape(
                crate::ShapeKind::Ellipse,
                Rect::from_min_size(Vec2::new(8_000.0, 100.0), Vec2::new(80.0, 50.0)),
            ))
            .unwrap();
        let connector = sheet
            .add_canvas_object(crate::CanvasObject::connector(crate::CanvasConnector::new(
                Endpoint::Point(Vec2::new(100.0, 100.0)),
                Endpoint::Object(foreign),
            )))
            .unwrap();
        sheet
            .set_zone_members(zone, vec![root], vec![connector])
            .unwrap();
        sheet
            .set_zone_members(other, vec![], vec![foreign])
            .unwrap();
        sheet.validate().unwrap();
        let layout = layout_sheet(&sheet, &ApproxMeasure, false);
        let resolved =
            resolve_presentation_slide(&sheet, &slide(PresentationSource::Zone(zone)), &layout)
                .unwrap();
        assert!(resolved.frame.max().x < 1_000.0, "{:?}", resolved.frame);
        let visible = resolved.scene_canvas_object_ids(&sheet);
        assert!(!visible.contains(&foreign));
        assert!(!visible.contains(&connector));
        assert!(visible.contains(&unassigned));
    }

    #[test]
    fn scene_object_filter_handles_long_reversed_connector_chains() {
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = crate::SheetRootMode::Hidden;
        let root = sheet.add_floating("Root", Vec2::new(80.0, 80.0));
        let frame = Rect::from_min_size(Vec2::ZERO, Vec2::new(300.0, 200.0));
        let other = sheet.create_board_frame("Other", frame).unwrap();
        let foreign = sheet
            .add_canvas_object(crate::CanvasObject::shape(
                crate::ShapeKind::Rectangle,
                frame,
            ))
            .unwrap();
        let free = sheet
            .add_canvas_object(crate::CanvasObject::shape(crate::ShapeKind::Ellipse, frame))
            .unwrap();
        sheet
            .set_zone_members(other, vec![], vec![foreign])
            .unwrap();
        let mut target = foreign;
        for _ in 0..2_000 {
            target = sheet
                .add_canvas_object(crate::CanvasObject::connector(crate::CanvasConnector::new(
                    Endpoint::Object(target),
                    Endpoint::Point(Vec2::ZERO),
                )))
                .unwrap();
        }
        sheet.canvas.objects.reverse();
        sheet.validate().unwrap();
        let mut resolved = ResolvedPresentationSlide {
            layout: layout_sheet(&sheet, &ApproxMeasure, false),
            frame,
            zones: Vec::new(),
            permitted_topics: HashSet::from([root]),
            permitted_objects: HashSet::new(),
        };
        assert_eq!(
            resolved.scene_canvas_object_ids(&sheet),
            HashSet::from([free])
        );
        resolved.permitted_objects = sheet
            .canvas
            .objects
            .iter()
            .map(|object| object.id)
            .collect();
        assert_eq!(resolved.scene_canvas_object_ids(&sheet).len(), 2_002);
    }

    #[test]
    fn clip_bounds_keep_lines_and_points_on_the_frame_edge() {
        let rect =
            |x, y, width, height| Rect::from_min_size(Vec2::new(x, y), Vec2::new(width, height));
        let clip = rect(0.0, 0.0, 100.0, 100.0);
        for (bounds, expected) in [
            (
                rect(100.0, 20.0, 0.0, 60.0),
                Some(rect(100.0, 20.0, 0.0, 60.0)),
            ),
            (
                rect(-20.0, 0.0, 140.0, 0.0),
                Some(rect(0.0, 0.0, 100.0, 0.0)),
            ),
            (
                rect(-10.0, -10.0, 10.0, 10.0),
                Some(rect(0.0, 0.0, 0.0, 0.0)),
            ),
            (rect(100.01, 20.0, 0.0, 60.0), None),
        ] {
            assert_eq!(clip_presentation_bounds(bounds, Some(clip)), expected);
            assert_eq!(clip_presentation_bounds(bounds, None), Some(bounds));
        }
    }

    #[test]
    fn clipped_labels_keep_the_owning_frame_when_its_zone_is_filtered_out() {
        for kind in ["boundary", "relationship"] {
            let (mut sheet, root, _) = decoration_fixture();
            let other = sheet.add_floating("Other", Vec2::new(800.0, 800.0));
            sheet.zones[0].roots.push(other);
            let clip = sheet.zones[0].frame;
            let boundary = sheet.boundaries[0].id;
            let relationship = sheet.relationships[0].id;
            sheet
                .set_boundary_title(boundary, Some("Unmeasured boundary label".into()))
                .unwrap();
            sheet
                .set_relationship_title(relationship, "Unmeasured relationship label")
                .unwrap();
            sheet.validate().unwrap();

            let mut layout = layout_sheet(&sheet, &ApproxMeasure, false);
            for topic in layout.topics.values_mut() {
                topic.rect =
                    Rect::from_min_size(Vec2::new(1_000.0, 1_000.0), Vec2::new(20.0, 20.0));
            }
            layout.connectors.clear();
            layout.summaries.clear();
            let interior = Rect::from_min_size(Vec2::new(100.0, 100.0), Vec2::new(10.0, 10.0));
            match kind {
                "boundary" => {
                    layout.relationships.clear();
                    layout.boundaries[0].rect = interior;
                }
                "relationship" => {
                    layout.boundaries.clear();
                    let line = &mut layout.relationships[0];
                    line.start = interior.min;
                    line.c1 = Vec2::new(110.0, 100.0);
                    line.c2 = interior.max();
                    line.end = Vec2::new(100.0, 110.0);
                }
                _ => unreachable!(),
            }
            let source = slide(PresentationSource::Branch(root));
            let resolved = resolve_presentation_slide(&sheet, &source, &layout).unwrap();
            assert!(resolved.zones.is_empty(), "{kind}");
            assert_eq!(resolved.layout.bounds, clip, "{kind}");
            assert_eq!(resolved.frame, clip.expand(40.0), "{kind}");

            sheet.set_boundary_title(boundary, None).unwrap();
            sheet.set_relationship_title(relationship, "").unwrap();
            let unlabelled = resolve_presentation_slide(&sheet, &source, &layout).unwrap();
            let painted = interior.expand(if kind == "boundary" { 1.0 } else { 10.0 });
            assert_eq!(unlabelled.layout.bounds, painted, "{kind}");
            assert_eq!(unlabelled.frame, painted.expand(40.0), "{kind}");
        }
    }

    #[test]
    fn clipped_decorations_do_not_inflate_branch_or_zone_auto_frames() {
        let (sheet, root, zone) = decoration_fixture();
        let clip = sheet.zones[0].frame;
        for kind in ["boundary", "summary", "relationship"] {
            let mut layout = layout_sheet(&sheet, &ApproxMeasure, false);
            match kind {
                "boundary" => {
                    layout.boundaries[0].rect =
                        Rect::from_min_size(Vec2::new(30_000.0, 30_000.0), Vec2::new(50.0, 50.0));
                }
                "summary" => {
                    let summary = &mut layout.summaries[0];
                    summary.range_rect =
                        Rect::from_min_size(Vec2::new(30_000.0, 30_000.0), Vec2::new(50.0, 50.0));
                    summary.start = Vec2::new(30_000.0, 30_000.0);
                    summary.end = Vec2::new(30_050.0, 30_050.0);
                    summary.tip = Vec2::new(30_025.0, 30_025.0);
                }
                "relationship" => {
                    layout.relationships[0].c1 = Vec2::new(30_000.0, 30_000.0);
                    layout.relationships[0].c2 = Vec2::new(30_050.0, 30_050.0);
                }
                _ => unreachable!(),
            }
            for source in [
                PresentationSource::Branch(root),
                PresentationSource::Zone(zone),
            ] {
                let resolved = resolve_presentation_slide(&sheet, &slide(source), &layout).unwrap();
                assert_eq!(resolved.frame, clip.expand(40.0), "{kind}, {source:?}");

                // Without a clip, retain the previous raw-geometry framing.
                let mut legacy = sheet.clone();
                legacy.zones[0].clip = false;
                let resolved =
                    resolve_presentation_slide(&legacy, &slide(source), &layout).unwrap();
                assert_eq!(resolved.frame.max().x, 30_090.0, "{kind}, {source:?}");
            }
            let viewport = slide(PresentationSource::Viewport);
            assert_eq!(
                resolve_presentation_slide(&sheet, &viewport, &layout)
                    .unwrap()
                    .frame,
                viewport.frame,
                "{kind}"
            );
        }
    }

    #[test]
    fn clipped_decoration_segments_crossing_a_hidden_frame_remain_framed() {
        let (mut sheet, root, _) = decoration_fixture();
        let other = sheet.add_floating("Other", Vec2::new(800.0, 800.0));
        sheet.zones[0].roots.push(other);
        sheet.validate().unwrap();
        let original = layout_sheet(&sheet, &ApproxMeasure, false);
        let summary_topic = sheet.summaries[0].topic;

        for kind in ["boundary", "map edge", "summary connector", "relationship"] {
            let mut layout = original.clone();
            for topic in layout.topics.values_mut() {
                topic.rect =
                    Rect::from_min_size(Vec2::new(1_000.0, 1_000.0), Vec2::new(20.0, 20.0));
            }
            let mut boundary = layout.boundaries[0].clone();
            let mut connector = layout
                .connectors
                .iter()
                .find(|connector| connector.parent == root)
                .unwrap()
                .clone();
            let mut summary = layout.summaries[0].clone();
            let mut relationship = layout.relationships[0].clone();
            layout.boundaries.clear();
            layout.connectors.clear();
            layout.summaries.clear();
            layout.relationships.clear();

            let visible = match kind {
                "boundary" => {
                    boundary.rect =
                        Rect::from_min_size(Vec2::new(-50.0, 30.0), Vec2::new(420.0, 40.0));
                    layout.boundaries.push(boundary);
                    Rect::from_min_size(Vec2::new(0.0, 29.0), Vec2::new(320.0, 42.0))
                }
                "map edge" => {
                    connector.start = Vec2::new(-50.0, 50.0);
                    connector.end = Vec2::new(370.0, 50.0);
                    let padding = connector.width.max(0.0) / 2.0;
                    layout.connectors.push(connector);
                    Rect::from_min_size(
                        Vec2::new(0.0, 50.0 - padding),
                        Vec2::new(320.0, padding * 2.0),
                    )
                }
                "summary connector" => {
                    summary.dir = crate::layout::Dir::Right;
                    summary.range_rect =
                        Rect::from_min_size(Vec2::new(-150.0, 40.0), Vec2::new(20.0, 20.0));
                    summary.start = Vec2::new(-130.0, 40.0);
                    summary.end = Vec2::new(-130.0, 60.0);
                    summary.tip = Vec2::new(-50.0, 50.0);
                    layout.topics.get_mut(&summary_topic).unwrap().rect =
                        Rect::from_min_size(Vec2::new(370.0, 40.0), Vec2::new(20.0, 20.0));
                    layout.summaries.push(summary);
                    Rect::from_min_size(Vec2::new(0.0, 49.0), Vec2::new(320.0, 2.0))
                }
                "relationship" => {
                    relationship.start = Vec2::new(-50.0, 50.0);
                    relationship.c1 = Vec2::new(-50.0, 50.0);
                    relationship.c2 = Vec2::new(370.0, 50.0);
                    relationship.end = Vec2::new(370.0, 50.0);
                    layout.relationships.push(relationship);
                    Rect::from_min_size(Vec2::new(0.0, 40.0), Vec2::new(320.0, 20.0))
                }
                _ => unreachable!(),
            };
            let resolved = resolve_presentation_slide(
                &sheet,
                &slide(PresentationSource::Branch(root)),
                &layout,
            )
            .unwrap();
            assert!(resolved.zones.is_empty(), "{kind}");
            assert_eq!(resolved.layout.bounds, visible, "{kind}");
            assert_eq!(resolved.frame, visible.expand(40.0), "{kind}");
        }
    }

    #[test]
    fn clipped_strokes_outside_a_hidden_frame_remain_framed() {
        let (mut sheet, root, _) = decoration_fixture();
        let other = sheet.add_floating("Other", Vec2::new(800.0, 800.0));
        sheet.zones[0].roots.push(other);
        sheet.validate().unwrap();
        let original = layout_sheet(&sheet, &ApproxMeasure, false);

        for kind in [
            "boundary",
            "map edge",
            "summary",
            "relationship",
            "relationship arrow",
        ] {
            let mut layout = original.clone();
            for topic in layout.topics.values_mut() {
                topic.rect =
                    Rect::from_min_size(Vec2::new(1_000.0, 1_000.0), Vec2::new(20.0, 20.0));
            }
            let mut boundary = layout.boundaries[0].clone();
            let mut connector = layout
                .connectors
                .iter()
                .find(|connector| connector.parent == root)
                .unwrap()
                .clone();
            let mut summary = layout.summaries[0].clone();
            let mut relationship = layout.relationships[0].clone();
            layout.boundaries.clear();
            layout.connectors.clear();
            layout.summaries.clear();
            layout.relationships.clear();

            match kind {
                "boundary" => {
                    boundary.rect =
                        Rect::from_min_size(Vec2::new(-50.0, -40.0), Vec2::new(420.0, 39.75));
                    layout.boundaries.push(boundary);
                }
                "map edge" => {
                    connector.start = Vec2::new(-50.0, -0.25);
                    connector.end = Vec2::new(370.0, -0.25);
                    connector.width = 2.0;
                    layout.connectors.push(connector);
                }
                "summary" => {
                    summary.dir = crate::layout::Dir::Right;
                    summary.range_rect =
                        Rect::from_min_size(Vec2::new(-150.0, -90.0), Vec2::new(20.0, 20.0));
                    summary.start = Vec2::new(-50.0, -0.25);
                    summary.end = Vec2::new(370.0, -0.25);
                    summary.tip = Vec2::new(160.0, -0.25);
                    layout.topics.get_mut(&summary.topic).unwrap().rect =
                        Rect::from_min_size(Vec2::new(160.0, -50.0), Vec2::new(20.0, 10.0));
                    layout.summaries.push(summary);
                }
                "relationship" | "relationship arrow" => {
                    let y = if kind == "relationship" { -0.25 } else { -3.0 };
                    relationship.start = Vec2::new(100.0, y);
                    relationship.c1 = Vec2::new(150.0, y);
                    relationship.c2 = Vec2::new(180.0, y);
                    relationship.end = Vec2::new(200.0, y);
                    layout.relationships.push(relationship);
                }
                _ => unreachable!(),
            }
            let mut source = slide(PresentationSource::Branch(root));
            source.frame =
                Rect::from_min_size(Vec2::new(1_000.0, 1_000.0), Vec2::new(200.0, 200.0));
            let resolved = resolve_presentation_slide(&sheet, &source, &layout).unwrap();
            assert!(resolved.zones.is_empty(), "{kind}");
            assert!(resolved.layout.bounds.min.x < 320.0, "{kind}");
            assert!(resolved.layout.bounds.max().x > 0.0, "{kind}");
            assert!(resolved.layout.bounds.max().y > 0.0, "{kind}");
        }
    }

    #[test]
    fn relationships_with_different_endpoint_clips_keep_their_full_bounds() {
        let (mut sheet, root, _) = decoration_fixture();
        let other = sheet.add_floating("Other", Vec2::new(500.0, 100.0));
        sheet
            .create_zone(
                "Other frame",
                vec![other],
                Rect::from_min_size(Vec2::new(400.0, 0.0), Vec2::new(240.0, 240.0)),
            )
            .unwrap();
        let relationship = sheet.add_relationship(root, other).unwrap();
        let mut layout = layout_sheet(&sheet, &ApproxMeasure, false);
        let curve = layout
            .relationships
            .iter_mut()
            .find(|curve| curve.id == relationship)
            .unwrap();
        curve.c1 = Vec2::new(30_000.0, 50.0);
        curve.c2 = Vec2::new(30_050.0, 50.0);
        let mut source = slide(PresentationSource::Branch(root));
        source.scope = PresentationScope::Context;

        for clips in [(true, true), (true, false), (false, false)] {
            sheet.zones[0].clip = clips.0;
            sheet.zones[1].clip = clips.1;
            let resolved = resolve_presentation_slide(&sheet, &source, &layout).unwrap();
            assert_eq!(resolved.frame.max().x, 30_090.0, "{clips:?}");
        }
    }

    #[test]
    fn coincident_distinct_frame_clips_do_not_bound_context_relationships() {
        let (mut sheet, root, zone) = decoration_fixture();
        let other = sheet.add_floating("Other", Vec2::new(200.0, 100.0));
        let frame = sheet.zones[0].frame;
        sheet
            .create_zone("Other frame", vec![other], frame)
            .unwrap();
        sheet.zones[1].clip = true;
        let relationship = sheet.add_relationship(root, other).unwrap();
        sheet.validate().unwrap();
        let mut layout = layout_sheet(&sheet, &ApproxMeasure, false);
        let curve = layout
            .relationships
            .iter_mut()
            .find(|curve| curve.id == relationship)
            .unwrap();
        curve.c1 = Vec2::new(30_000.0, 50.0);
        curve.c2 = Vec2::new(30_050.0, 50.0);

        for source in [
            PresentationSource::Branch(root),
            PresentationSource::Zone(zone),
        ] {
            let mut context = slide(source);
            context.scope = PresentationScope::Context;
            let resolved = resolve_presentation_slide(&sheet, &context, &layout).unwrap();
            assert!(
                resolved
                    .layout
                    .relationships
                    .iter()
                    .any(|curve| curve.id == relationship),
                "{source:?}"
            );
            assert_eq!(resolved.frame.max().x, 30_090.0, "{source:?}");
        }
    }

    #[test]
    fn isolates_owned_content_without_moving_geometry_and_removes_crossing_annotations() {
        let mut sheet = Sheet::new("Map", "Central");
        let branch = sheet.add_child(sheet.root, None, "Branch");
        let child = sheet.add_child(branch, None, "Child");
        let other = sheet.add_child(branch, None, "Other");
        let sibling = sheet.add_child(sheet.root, None, "Sibling");
        let summary = sheet.add_summary(&[child, other], "Summary").unwrap();
        sheet.add_boundary(&[child, other]).unwrap();
        sheet.add_boundary(&[summary]).unwrap();
        sheet.add_relationship(child, sibling).unwrap();
        sheet.add_relationship(child, other).unwrap();
        let full = layout_sheet(&sheet, &ApproxMeasure, false);
        let mut slide = slide(PresentationSource::Branch(branch));
        let resolved = resolve_presentation_slide(&sheet, &slide, &full).unwrap();
        assert_eq!(resolved.layout.topics.len(), 4);
        for (id, topic) in &resolved.layout.topics {
            assert_eq!(topic, &full.topics[id]);
        }
        assert_eq!(resolved.layout.relationships.len(), 1);
        assert_eq!(resolved.layout.boundaries.len(), 2);
        assert_eq!(resolved.layout.summaries.len(), 1);
        assert!(resolved.layout.topics.contains_key(&summary));
        assert!(!resolved.layout.topics.contains_key(&sibling));
        assert!(resolved.zones.is_empty());
        slide.hidden_topics = vec![other, TopicId(u64::MAX)];
        let hidden = resolve_presentation_slide(&sheet, &slide, &full).unwrap();
        assert_eq!(hidden.layout.order.len(), 2);
        assert!(!hidden.layout.topics.contains_key(&summary));
        assert!(hidden.layout.relationships.is_empty());
        assert!(hidden.layout.boundaries.is_empty());
        assert!(hidden.layout.summaries.is_empty());
        slide.max_depth = Some(0);
        let shallow = resolve_presentation_slide(&sheet, &slide, &full).unwrap();
        assert_eq!(shallow.layout.order, vec![branch]);
        assert!(shallow.layout.connectors.is_empty());
    }

    #[test]
    fn live_bounds_manual_frames_empty_slides_and_missing_sources_are_safe() {
        let mut sheet = Sheet::new("Map", "Central");
        let branch = sheet.add_child(sheet.root, None, "Branch");
        let mut slide = slide(PresentationSource::Branch(branch));
        let before = resolve_presentation_slide(
            &sheet,
            &slide,
            &layout_sheet(&sheet, &ApproxMeasure, false),
        )
        .unwrap();
        let child = sheet.add_child(branch, None, "A new descendant");
        let full = layout_sheet(&sheet, &ApproxMeasure, false);
        let after = resolve_presentation_slide(&sheet, &slide, &full).unwrap();
        assert!(after.layout.topics.contains_key(&child));
        assert_ne!(before.frame, after.frame);
        slide.auto_frame = false;
        assert_eq!(
            resolve_presentation_slide(&sheet, &slide, &full)
                .unwrap()
                .frame,
            slide.frame
        );
        slide.hidden_topics.push(branch);
        let empty = resolve_presentation_slide(&sheet, &slide, &full).unwrap();
        assert!(empty.layout.topics.is_empty());
        assert_eq!(empty.frame, slide.frame);
        sheet.detach_subtree(branch).unwrap();
        assert!(resolve_presentation_slide(&sheet, &slide, &full).is_err());
    }

    #[test]
    fn zone_roots_and_descendants_only_and_partial_ownership_hides_zone_frame() {
        let mut sheet = Sheet::new("Map", "Central");
        let first = sheet.add_floating("First", Vec2::new(300.0, 300.0));
        let second = sheet.add_floating("Second", Vec2::new(600.0, 300.0));
        let child = sheet.add_child(first, None, "Child");
        let zone = sheet
            .create_zone(
                "Zone",
                vec![first, second],
                Rect::from_min_size(Vec2::new(200.0, 200.0), Vec2::new(600.0, 400.0)),
            )
            .unwrap();
        let full = layout_sheet(&sheet, &ApproxMeasure, false);
        let mut slide = slide(PresentationSource::Zone(zone));
        let resolved = resolve_presentation_slide(&sheet, &slide, &full).unwrap();
        assert_eq!(resolved.layout.topics.len(), 3);
        assert_eq!(resolved.zones.len(), 1);
        slide.hidden_topics.push(child);
        let resolved = resolve_presentation_slide(&sheet, &slide, &full).unwrap();
        assert_eq!(resolved.layout.topics.len(), 2);
        assert!(resolved.zones.is_empty());
        sheet.remove_zone(zone).unwrap();
        assert!(resolve_presentation_slide(&sheet, &slide, &full).is_err());
    }

    #[test]
    fn zone_depth_limit_retains_members_and_drops_partially_owned_enclosure() {
        let mut sheet = Sheet::new("Map", "Central");
        let first = sheet.add_floating("First", Vec2::new(300.0, 300.0));
        let second = sheet.add_floating("Second", Vec2::new(600.0, 300.0));
        let child = sheet.add_child(first, None, "Child");
        let grandchild = sheet.add_child(child, None, "Grandchild");
        let sibling = sheet.add_child(second, None, "Second child");
        let unrelated = sheet.add_floating("Unrelated", Vec2::new(900.0, 300.0));
        let zone = sheet
            .create_zone(
                "Zone",
                vec![first, second],
                Rect::from_min_size(Vec2::new(200.0, 200.0), Vec2::new(600.0, 400.0)),
            )
            .unwrap();
        let full = layout_sheet(&sheet, &ApproxMeasure, false);
        let mut slide = slide(PresentationSource::Zone(zone));
        for (depth, expected) in [
            (0, vec![first, second]),
            (1, vec![first, second, child, sibling]),
            (2, vec![first, second, child, sibling, grandchild]),
        ] {
            slide.max_depth = Some(depth);
            let resolved = resolve_presentation_slide(&sheet, &slide, &full).unwrap();
            let expected: HashSet<_> = expected.into_iter().collect();
            assert_eq!(
                resolved
                    .layout
                    .topics
                    .keys()
                    .copied()
                    .collect::<HashSet<_>>(),
                expected
            );
            assert!(!resolved.layout.topics.contains_key(&unrelated));
            for id in &expected {
                assert_eq!(resolved.layout.topics[id], full.topics[id]);
            }
            assert_eq!(resolved.layout.connectors.len(), expected.len() - 2);
            assert_eq!(resolved.zones.len(), usize::from(depth == 2));
            if depth < 2 {
                assert!(
                    resolved.zones.is_empty(),
                    "partial ownership hides the enclosure"
                );
            } else {
                assert_eq!(resolved.zones[0], sheet.zones[0]);
            }
        }
    }

    #[test]
    fn old_json_defaults_to_context_and_manual_and_stale_hidden_ids_validate() {
        let mut sheet = Sheet::new("Map", "Central");
        let branch = sheet.add_child(sheet.root, None, "Branch");
        let mut json = serde_json::to_value(slide(PresentationSource::Branch(branch))).unwrap();
        let object = json.as_object_mut().unwrap();
        for key in ["scope", "auto_frame", "hidden_topics", "max_depth"] {
            object.remove(key);
        }
        let old: PresentationSlide = serde_json::from_value(json).unwrap();
        assert_eq!(old.scope, PresentationScope::Context);
        assert!(!old.auto_frame);
        assert_eq!(old.max_depth, None);
        let full = layout_sheet(&sheet, &ApproxMeasure, false);
        assert_eq!(
            resolve_presentation_slide(&sheet, &old, &full)
                .unwrap()
                .layout,
            full
        );
        let id = sheet
            .add_presentation_slide("New", PresentationSource::Branch(branch), old.frame)
            .unwrap();
        assert!(sheet.presentation.slides[0].auto_frame);
        sheet.presentation.slides[0].hidden_topics = vec![TopicId(u64::MAX)];
        sheet.validate().unwrap();
        sheet.set_presentation_slide_frame(id, old.frame).unwrap();
        assert!(!sheet.presentation.slides[0].auto_frame);
        sheet.rename_presentation_slide(id, "Renamed").unwrap();
        assert_eq!(sheet.presentation.slides[0].id, id);
    }

    #[test]
    fn hidden_board_viewport_preserves_frame_and_excludes_anchor() {
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = crate::SheetRootMode::Hidden;
        let first = sheet.add_floating("First", Vec2::new(120.0, 100.0));
        let second = sheet.add_floating("Second", Vec2::new(500.0, 100.0));
        let child = sheet.add_child(first, None, "Child");
        let frame = Rect::from_min_size(Vec2::new(10.0, 20.0), Vec2::new(300.0, 200.0));
        let zone = sheet.create_board_frame("Empty frame", frame).unwrap();
        let mut viewport = slide(PresentationSource::Viewport);
        viewport.frame = frame;
        // Even old callers that leave auto-frame enabled must not replace the
        // authored viewport with the bounds of content elsewhere on the board.
        assert!(viewport.auto_frame);
        let layout = layout_sheet(&sheet, &ApproxMeasure, false);
        let resolved = resolve_presentation_slide(&sheet, &viewport, &layout).unwrap();
        assert_eq!(resolved.frame, frame);
        assert!(resolved.layout.topics.contains_key(&first));
        assert!(resolved.layout.topics.contains_key(&second));
        assert!(resolved.layout.topics.contains_key(&child));
        assert!(!resolved.layout.topics.contains_key(&sheet.root));
        assert!(resolved.zones.iter().any(|item| item.id == zone));

        let mut source = slide(PresentationSource::Branch(first));
        let branch = resolve_presentation_slide(&sheet, &source, &layout).unwrap();
        assert!(branch.layout.topics.contains_key(&first));
        assert!(branch.layout.topics.contains_key(&child));
        assert!(!branch.layout.topics.contains_key(&second));
        assert!(branch.zones.is_empty());
        source.source = PresentationSource::Branch(sheet.root);
        assert!(resolve_presentation_slide(&sheet, &source, &layout).is_err());
    }

    #[test]
    fn empty_frame_sources_and_clipped_scene_bounds_remain_usable() {
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = crate::SheetRootMode::Hidden;
        let frame = Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 100.0));
        let zone = sheet.create_board_frame("Frame", frame).unwrap();
        sheet.zones[0].clip = true;
        let object = crate::CanvasObject::shape(
            crate::ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::new(60.0, 20.0), Vec2::new(60.0, 40.0)),
        );
        sheet.add_canvas_object(object).unwrap();
        let layout = layout_sheet(&sheet, &ApproxMeasure, false);

        let viewport = slide(PresentationSource::Viewport);
        let resolved = resolve_presentation_slide(&sheet, &viewport, &layout).unwrap();
        assert_eq!(resolved.frame, viewport.frame);
        assert_eq!(resolved.layout.bounds, frame);
        assert!(resolved.layout.topics.is_empty());
        assert_eq!(resolved.zones[0].id, zone);

        let source = slide(PresentationSource::Zone(zone));
        let resolved = resolve_presentation_slide(&sheet, &source, &layout).unwrap();
        assert_eq!(resolved.zones[0].id, zone);
        assert_eq!(resolved.frame, frame.expand(40.0));
    }

    #[test]
    fn branch_source_survives_hiding_and_showing_the_central_topic() {
        let mut sheet = Sheet::new("Map", "Central");
        let branch = sheet.add_child(sheet.root, None, "Branch");
        let child = sheet.add_child(branch, None, "Child");
        let other = sheet.add_child(sheet.root, None, "Other");
        let source = slide(PresentationSource::Branch(branch));
        sheet
            .add_presentation_slide("Branch", source.source, source.frame)
            .unwrap();
        let moved = sheet.hide_central_topic().unwrap().unwrap();
        sheet.validate().unwrap();
        assert_eq!(sheet.presentation.slides[0].source, source.source);
        for _ in 0..2 {
            let layout = layout_sheet(&sheet, &ApproxMeasure, false);
            let resolved = resolve_presentation_slide(&sheet, &source, &layout).unwrap();
            assert!(resolved.layout.topics.contains_key(&branch));
            assert!(resolved.layout.topics.contains_key(&child));
            assert!(!resolved.layout.topics.contains_key(&other));
            assert!(!resolved.layout.topics.contains_key(&moved));
            sheet.show_central_topic();
            sheet.validate().unwrap();
        }
    }

    #[test]
    fn invalid_saved_viewport_frames_are_rejected_even_with_auto_frame() {
        let sheet = Sheet::new("Map", "Central");
        let layout = layout_sheet(&sheet, &ApproxMeasure, false);
        for frame in [
            Rect::from_min_size(Vec2::ZERO, Vec2::new(0.0, 100.0)),
            Rect::from_min_size(Vec2::new(f32::NAN, 0.0), Vec2::new(100.0, 100.0)),
            Rect::from_min_size(Vec2::ZERO, Vec2::new(f32::INFINITY, 100.0)),
            Rect::from_min_size(
                Vec2::new(crate::MAX_PRESENTATION_COORDINATE, 0.0),
                Vec2::new(100.0, 100.0),
            ),
        ] {
            let mut viewport = slide(PresentationSource::Viewport);
            viewport.frame = frame;
            assert!(resolve_presentation_slide(&sheet, &viewport, &layout).is_err());
        }
    }
}
