//! View-only subtree previews; the document changes only on a valid drop.

use std::collections::BTreeSet;

use egui::{Color32, Painter, Stroke};
use thinoda_core::{
    BranchSide, EditError, Layout, Sheet, Structure, TopicId, Vec2, layout::ConnectorKind,
    theme::level_of,
};

use crate::{MapView, PaintOptions, Xf, paint_map, render::connector_path};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DropTarget {
    pub parent: TopicId,
    pub index: usize,
    pub slot: Option<[Vec2; 2]>,
    pub side: Option<BranchSide>,
}

fn main_side(sheet: &Sheet, layout: &Layout, parent: TopicId, at: Vec2) -> Option<BranchSide> {
    (parent == sheet.root
        && matches!(
            sheet.structure,
            Structure::MapClockwise | Structure::MapAnticlockwise | Structure::MapUnbalanced
        ))
    .then(|| {
        if at.x < layout.topics[&parent].rect.center().x {
            BranchSide::Left
        } else {
            BranchSide::Right
        }
    })
}

pub(crate) fn roots(sheet: &Sheet, ids: &[TopicId]) -> Vec<TopicId> {
    ids.iter()
        .copied()
        .filter(|id| {
            *id != sheet.root
                && sheet.summary_for_topic(*id).is_none()
                && !ids.iter().any(|other| {
                    other != id
                        && *other != sheet.root
                        && sheet.owned_subtree_ids(*other).contains(id)
                })
        })
        .collect()
}

pub(crate) fn members(sheet: &Sheet, ids: &[TopicId]) -> BTreeSet<TopicId> {
    ids.iter()
        .flat_map(|id| sheet.owned_subtree_ids(*id))
        .collect()
}

pub(crate) fn target(
    sheet: &Sheet,
    layout: &Layout,
    members: &BTreeSet<TopicId>,
    at: Vec2,
) -> Result<Option<DropTarget>, EditError> {
    if sheet.theme.flexible_floating_topics {
        let roots: Vec<_> = members
            .iter()
            .copied()
            .filter(|id| {
                sheet
                    .parent(*id)
                    .is_none_or(|parent| !members.contains(&parent))
                    && sheet.summary_for_topic(*id).is_none()
            })
            .collect();
        let floating = roots.iter().filter(|id| sheet.is_floating(**id)).count();
        if floating > 0 && floating < roots.len() {
            return Err(EditError::Invalid(
                "move floating and attached topics separately while Flexible Floating Topics is enabled".into(),
            ));
        }
        if floating > 0 {
            return Ok(None);
        }
    }
    let Some(topic) = layout.hit_test(at) else {
        if sheet.theme.flexible_floating_topics {
            return Err(EditError::Invalid(
                "disable Flexible Floating Topics to detach a topic into blank space".into(),
            ));
        }
        return Ok(None);
    };
    if members.contains(&topic) {
        return Err(EditError::Cycle);
    }
    let rect = layout.topics[&topic].rect;
    if let Some(parent) = sheet.parent(topic) {
        let index = sheet.index_in_parent(topic).ok_or(EditError::NotFound)?;
        let horizontal = layout
            .connectors
            .iter()
            .find(|c| c.child == topic && c.parent == parent)
            .is_some_and(|c| c.vertical);
        let coordinate = |p: Vec2| if horizontal { p.x } else { p.y };
        let low = coordinate(rect.min);
        let high = coordinate(rect.max());
        let pointer = coordinate(at);
        let edge = (high - low) * 0.25;
        if pointer < low + edge || pointer > high - edge {
            // Some split-map sides run in reverse document order.
            let parent_center = layout.topics[&parent].rect.center();
            let side = |id| {
                let center = layout.topics[&id].rect.center();
                if horizontal {
                    center.y >= parent_center.y
                } else {
                    center.x >= parent_center.x
                }
            };
            let neighbor = sheet
                .children(parent)
                .iter()
                .enumerate()
                .filter(|(_, id)| {
                    **id != topic && layout.topics.contains_key(id) && side(**id) == side(topic)
                })
                .min_by_key(|(i, _)| i.abs_diff(index));
            let fallback_reversed = parent == sheet.root
                && !horizontal
                && matches!(
                    sheet.structure,
                    Structure::MapClockwise
                        | Structure::MapAnticlockwise
                        | Structure::MapUnbalanced
                )
                && if sheet.structure == Structure::MapAnticlockwise && !sheet.theme.balanced {
                    side(topic)
                } else {
                    !side(topic)
                };
            let reversed = neighbor.map_or(fallback_reversed, |(i, id)| {
                let ascending =
                    coordinate(layout.topics[id].rect.center()) > coordinate(rect.center());
                ascending != (i > index)
            });
            let high_edge = pointer > high - edge;
            let after = high_edge != reversed;
            let line = if horizontal {
                let x = if high_edge { rect.max().x } else { rect.min.x };
                [
                    Vec2::new(x, rect.min.y - 4.0),
                    Vec2::new(x, rect.max().y + 4.0),
                ]
            } else {
                let y = if high_edge { rect.max().y } else { rect.min.y };
                [
                    Vec2::new(rect.min.x - 4.0, y),
                    Vec2::new(rect.max().x + 4.0, y),
                ]
            };
            return Ok(Some(DropTarget {
                parent,
                index: index + usize::from(after),
                slot: Some(line),
                side: main_side(sheet, layout, parent, rect.center()),
            }));
        }
    }
    Ok(Some(DropTarget {
        parent: topic,
        index: sheet.children(topic).len(),
        slot: None,
        side: main_side(sheet, layout, topic, at),
    }))
}

pub(crate) fn apply(
    sheet: &mut Sheet,
    ids: &[TopicId],
    target: &DropTarget,
    layout: &Layout,
) -> Result<(), EditError> {
    let mut index = target.index;
    for id in ids {
        sheet.move_topic(*id, target.parent, index)?;
        preserve_size_after_move(sheet, *id, layout)?;
        if let Some(side) = target.side {
            sheet.set_branch_side(*id, Some(side))?;
        }
        index = sheet.index_in_parent(*id).ok_or(EditError::NotFound)? + 1;
    }
    sheet.set_collapsed(target.parent, false)
}

pub(crate) fn preserve_size_after_move(
    sheet: &mut Sheet,
    id: TopicId,
    layout: &Layout,
) -> Result<(), EditError> {
    let Some(previous) = layout.topics.get(&id) else {
        return Ok(());
    };
    preserve_previous_size(sheet, id, previous)
}

fn preserve_previous_size(
    sheet: &mut Sheet,
    id: TopicId,
    previous: &thinoda_core::TopicLayout,
) -> Result<(), EditError> {
    if level_of(sheet, id) != previous.level {
        let topic = sheet.topic_mut(id).ok_or(EditError::NotFound)?;
        topic.style.font_size = Some(previous.style.font_size);
        topic.style.padding = Some(previous.padding);
    }
    Ok(())
}

pub(crate) fn edge_pan(ctx: &egui::Context, view: &mut MapView, pointer: egui::Pos2) {
    let rect = view.canvas_rect();
    if !rect.contains(pointer) {
        return;
    }
    let strength = |low: f32, high: f32| {
        ((32.0 - low) / 32.0).clamp(0.0, 1.0) - ((32.0 - high) / 32.0).clamp(0.0, 1.0)
    };
    let direction = egui::vec2(
        strength(pointer.x - rect.left(), rect.right() - pointer.x),
        strength(pointer.y - rect.top(), rect.bottom() - pointer.y),
    );
    if direction != egui::Vec2::ZERO {
        view.offset += direction * 480.0 * ctx.input(|i| i.stable_dt.min(0.05));
        ctx.request_repaint();
    }
}

pub(crate) fn ghost_layout(sheet: &Sheet, layout: &Layout, members: &BTreeSet<TopicId>) -> Layout {
    Layout {
        topics: layout
            .topics
            .iter()
            .filter(|(id, _)| members.contains(id))
            .map(|(id, topic)| (*id, topic.clone()))
            .collect(),
        order: layout
            .order
            .iter()
            .copied()
            .filter(|id| members.contains(id))
            .collect(),
        connectors: layout
            .connectors
            .iter()
            .filter(|c| members.contains(&c.parent) && members.contains(&c.child))
            .cloned()
            .collect(),
        boundaries: layout
            .boundaries
            .iter()
            .filter(|b| {
                sheet
                    .boundaries
                    .iter()
                    .any(|source| source.id == b.id && members.contains(&source.parent))
            })
            .cloned()
            .collect(),
        summaries: layout
            .summaries
            .iter()
            .filter(|s| members.contains(&s.topic))
            .cloned()
            .collect(),
        relationships: layout
            .relationships
            .iter()
            .filter(|r| {
                sheet.relationships.iter().any(|source| {
                    source.id == r.id
                        && members.contains(&source.from)
                        && members.contains(&source.to)
                })
            })
            .cloned()
            .collect(),
        bounds: layout.bounds,
    }
}

fn alignment_guides(
    layout: &Layout,
    members: &BTreeSet<TopicId>,
    delta: Vec2,
    tolerance: f32,
) -> [Option<[Vec2; 2]>; 2] {
    let moving = layout
        .order
        .iter()
        .filter(|id| members.contains(id))
        .filter_map(|id| layout.rect(*id))
        .reduce(|a, b| a.union(&b));
    let Some(moving) = moving.map(|rect| rect.translate(delta)) else {
        return [None, None];
    };
    let coordinates = |rect: thinoda_core::Rect| [rect.min, rect.center(), rect.max()];
    let mut nearest = [tolerance; 2];
    let mut guides = [None, None];
    for rect in layout
        .order
        .iter()
        .filter(|id| !members.contains(id))
        .filter_map(|id| layout.rect(*id))
    {
        for (source, reference) in coordinates(moving).into_iter().zip(coordinates(rect)) {
            let distances = [
                (source.x - reference.x).abs(),
                (source.y - reference.y).abs(),
            ];
            for axis in 0..2 {
                if distances[axis] <= tolerance
                    && (guides[axis].is_none() || distances[axis] < nearest[axis])
                {
                    nearest[axis] = distances[axis];
                    guides[axis] = Some(if axis == 0 {
                        [
                            Vec2::new(reference.x, moving.min.y.min(rect.min.y)),
                            Vec2::new(reference.x, moving.max().y.max(rect.max().y)),
                        ]
                    } else {
                        [
                            Vec2::new(moving.min.x.min(rect.min.x), reference.y),
                            Vec2::new(moving.max().x.max(rect.max().x), reference.y),
                        ]
                    });
                }
            }
        }
    }
    guides
}

pub(crate) fn paint(
    painter: &Painter,
    sheet: &Sheet,
    layout: &Layout,
    view: &MapView,
    members: &BTreeSet<TopicId>,
    delta: Vec2,
    current: Vec2,
) {
    let xf = view.xf();
    let ghost = ghost_layout(sheet, layout, members);
    let mut translucent = painter.clone();
    translucent.set_opacity(0.55);
    paint_map(
        &translucent,
        sheet,
        &ghost,
        Xf {
            origin: xf.origin + egui::vec2(delta.x, delta.y) * xf.zoom,
            ..xf
        },
        &PaintOptions {
            dark: view.dark,
            omit_zones: true,
            ..Default::default()
        },
    );
    let target = target(sheet, layout, members, current);
    if matches!(target, Ok(None)) {
        if view.show_alignment_guides {
            for guide in alignment_guides(layout, members, delta, 4.0 / xf.zoom)
                .into_iter()
                .flatten()
            {
                painter.line_segment(
                    guide.map(|point| xf.pos(point)),
                    Stroke::new(1.0, view.accent),
                );
            }
        }
        return;
    }
    let Some(topic) = layout.hit_test(current) else {
        return;
    };
    let rect = &layout.topics[&topic].rect;
    let color = if target.is_ok() {
        view.accent
    } else {
        Color32::from_rgb(220, 60, 60)
    };
    painter.rect_stroke(
        xf.rect(*rect).expand(4.0),
        8.0,
        Stroke::new(2.5, color),
        egui::StrokeKind::Outside,
    );
    if let Ok(Some(target)) = target {
        if let Some(slot) = target.slot {
            painter.line_segment(slot.map(|p| xf.pos(p)), Stroke::new(3.0, view.accent));
        }
        let parent = layout.topics[&target.parent].rect;
        for id in members.iter().filter(|id| {
            sheet
                .parent(**id)
                .is_none_or(|parent| !members.contains(&parent))
                && sheet.summary_for_topic(**id).is_none()
        }) {
            if let Some(source) = layout.rect(*id) {
                let end = source.center() + delta;
                let vertical = !layout.topics[&target.parent].grow.is_horizontal();
                let points = connector_path(ConnectorKind::Curve, vertical, parent.center(), end);
                painter.add(egui::Shape::line(
                    points.into_iter().map(|p| xf.pos(p)).collect(),
                    Stroke::new(2.0, view.accent),
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use thinoda_core::{ApproxMeasure, layout_sheet};

    #[test]
    fn alignment_uses_group_bounds_and_stable_nearest_reference_per_axis() {
        let mut sheet = Sheet::new("Guides", "Root");
        let a = sheet.add_floating("Topic", Vec2::new(-400.0, -200.0));
        let b = sheet.add_floating("Topic", Vec2::new(-200.0, -200.0));
        let first = sheet.add_floating("Topic", Vec2::new(400.0, 200.0));
        let second = sheet.add_floating("Topic", Vec2::new(404.0, 300.0));
        let mut layout = layout_sheet(&sheet, &ApproxMeasure, false);
        layout.order = vec![a, b, first, second];
        let group = layout.rect(a).unwrap().union(&layout.rect(b).unwrap());
        let first_rect = layout.rect(first).unwrap();
        let second_rect = layout.rect(second).unwrap();
        let moving = [a, b].into_iter().collect();
        let delta = Vec2::new(first_rect.center().x - group.center().x + 2.0, 0.0);
        let guide = alignment_guides(&layout, &moving, delta, 4.0)[0].unwrap();
        assert_eq!(guide[0].x, first_rect.center().x, "Ties follow paint order");
        let delta = Vec2::new(second_rect.center().x - group.center().x - 0.5, 0.0);
        let guide = alignment_guides(&layout, &moving, delta, 4.0)[0].unwrap();
        assert_eq!(guide[0].x, second_rect.center().x, "Nearest reference wins");
        assert_eq!(guide[0].y, group.min.y);
        assert_eq!(guide[1].y, second_rect.max().y);
    }

    #[test]
    fn alignment_matches_visible_group_edges_and_centers_without_self_matches() {
        let mut sheet = Sheet::new("Guides", "Center");
        let moving = sheet.add_floating("Topic", Vec2::new(-400.0, -200.0));
        let reference = sheet.add_floating("Topic", Vec2::new(400.0, 200.0));
        let hidden = sheet.insert_child(reference, "Hidden").unwrap();
        sheet.set_collapsed(reference, true).unwrap();
        let layout = layout_sheet(&sheet, &ApproxMeasure, false);
        assert!(!layout.order.contains(&hidden));
        let owned = members(&sheet, &[moving]);
        let a = layout.rect(moving).unwrap();
        let b = layout.rect(reference).unwrap();
        for zoom in [0.5, 1.0, 2.0] {
            let tolerance = 4.0 / zoom;
            let delta = Vec2::new(b.center().x - a.center().x + 3.9 / zoom, 0.0);
            let guides = alignment_guides(&layout, &owned, delta, tolerance);
            assert!(guides[0].is_some());
            assert!(guides[1].is_none());
            let delta = Vec2::new(b.center().x - a.center().x + 4.1 / zoom, 0.0);
            assert!(alignment_guides(&layout, &owned, delta, tolerance)[0].is_none());
            let delta = Vec2::new(0.0, b.min.y - a.min.y);
            let horizontal = alignment_guides(&layout, &owned, delta, tolerance)[1].unwrap();
            assert_eq!(horizontal[0].y, b.min.y);
            assert_eq!(horizontal[1].y, b.min.y);
        }
        let all = layout.order.iter().copied().collect();
        assert_eq!(
            alignment_guides(&layout, &all, Vec2::ZERO, 4.0),
            [None, None]
        );
        assert_eq!(
            alignment_guides(&layout, &BTreeSet::new(), Vec2::ZERO, 4.0),
            [None, None]
        );
        // Hidden topics cannot contribute guides even if a layout map retains their rectangles.
        let mut hidden_only = layout.clone();
        hidden_only.order.retain(|id| *id == moving);
        assert_eq!(
            alignment_guides(&hidden_only, &owned, Vec2::ZERO, 4.0),
            [None, None]
        );
    }

    #[test]
    fn ghost_contains_owned_annotations_and_internal_links_only() {
        let mut sheet = Sheet::new("Preview", "Center");
        let branch = sheet.insert_child(sheet.root, "Branch").unwrap();
        let child = sheet.insert_child(branch, "Child").unwrap();
        let summary = sheet.add_summary(&[child], "Summary").unwrap();
        sheet.insert_child(summary, "Summary child").unwrap();
        sheet.add_boundary(&[child]).unwrap();
        let internal = sheet.add_relationship(branch, child).unwrap();
        sheet.add_relationship(child, sheet.root).unwrap();
        let owned = members(&sheet, &[branch]);
        let before = sheet.clone();
        let layout = layout_sheet(&sheet, &ApproxMeasure, false);
        let ghost = ghost_layout(&sheet, &layout, &owned);
        assert_eq!(ghost.topics.keys().copied().collect::<BTreeSet<_>>(), owned);
        assert_eq!(ghost.boundaries.len(), 1);
        assert_eq!(ghost.summaries.len(), 1);
        assert_eq!(ghost.relationships.len(), 1);
        assert_eq!(ghost.relationships[0].id, internal);
        assert!(
            ghost
                .connectors
                .iter()
                .all(|c| owned.contains(&c.parent) && owned.contains(&c.child))
        );
        assert_eq!(sheet, before);
        sheet.set_collapsed(branch, true).unwrap();
        let layout = layout_sheet(&sheet, &ApproxMeasure, false);
        let ghost = ghost_layout(&sheet, &layout, &owned);
        assert_eq!(ghost.order, vec![branch]);
        assert!(
            ghost.summaries.is_empty()
                && ghost.boundaries.is_empty()
                && ghost.relationships.is_empty()
        );
    }
}
