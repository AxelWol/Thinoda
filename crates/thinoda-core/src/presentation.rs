//! Live presentation resolution without changing the map's layout geometry.

use std::collections::{HashMap, HashSet};

use crate::{
    ChildRange, Layout, PresentationScope, PresentationSlide, PresentationSource, Rect, Sheet,
    TopicId, Vec2, Zone,
};

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedPresentationSlide {
    pub layout: Layout,
    pub frame: Rect,
    /// Zones are not stored in `Layout`; consumers must paint this list, not `sheet.zones`.
    pub zones: Vec<Zone>,
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
    let roots = match slide.source {
        PresentationSource::Branch(root)
            if sheet.contains(root) && sheet.main_branch_of(root).is_some() =>
        {
            vec![root]
        }
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
    };
    let hidden: HashSet<_> = slide
        .hidden_topics
        .iter()
        .filter(|id| sheet.contains(**id))
        .flat_map(|id| sheet.owned_subtree_ids(*id))
        .collect();
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
            !hidden.contains(id)
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
            !zone.roots.is_empty()
                && zone
                    .roots
                    .iter()
                    .all(|id| sheet.owned_subtree_ids(*id).into_iter().all(&allowed))
        })
        .cloned()
        .collect();
    let mut bounds = filtered
        .topics
        .values()
        .map(|t| t.rect)
        .reduce(|a, b| a.union(&b));
    let mut include_rect = |rect: Rect| {
        bounds = Some(bounds.map_or(rect, |bounds| bounds.union(&rect)));
    };
    for boundary in &filtered.boundaries {
        include_rect(boundary.rect);
    }
    for summary in &filtered.summaries {
        include_rect(summary.range_rect);
        for point in [summary.start, summary.end, summary.tip] {
            include_rect(Rect::from_min_size(point, Vec2::ZERO));
        }
    }
    for relationship in &filtered.relationships {
        // Cubic control hull conservatively bounds the whole curve.
        for point in [
            relationship.start,
            relationship.c1,
            relationship.c2,
            relationship.end,
        ] {
            include_rect(Rect::from_min_size(point, Vec2::ZERO));
        }
    }
    for zone in &zones {
        include_rect(zone.frame);
    }
    // Empty content deliberately stays empty, with a stable saved viewport.
    // Never use the full map's bounds as an empty-slide fallback.
    let frame = if slide.auto_frame {
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
}
