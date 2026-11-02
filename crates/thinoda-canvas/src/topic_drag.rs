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

const MAX_GUIDE_REFERENCES: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct DragGuides {
    /// Vertical and horizontal edge/center alignment guides.
    pub alignment: [Option<[Vec2; 2]>; 2],
    /// Moving/reference bounds with matching width and height.
    pub matching_size: [Option<[thinoda_core::Rect; 2]>; 2],
    /// Three bounds in reading order whose adjacent gaps match.
    pub equal_spacing: [Option<[thinoda_core::Rect; 3]>; 2],
}

fn drag_guides(
    layout: &Layout,
    members: &BTreeSet<TopicId>,
    delta: Vec2,
    tolerance: f32,
) -> DragGuides {
    let moving = layout
        .order
        .iter()
        .filter(|id| members.contains(id))
        .filter_map(|id| layout.rect(*id))
        .reduce(|a, b| a.union(&b));
    let Some(moving) = moving.map(|rect| rect.translate(delta)) else {
        return DragGuides::default();
    };
    drag_guides_for_bounds(
        moving,
        layout
            .order
            .iter()
            .filter(|id| !members.contains(id))
            .filter_map(|id| layout.rect(*id)),
        tolerance,
    )
}

pub(crate) fn drag_guides_for_bounds(
    moving: thinoda_core::Rect,
    references: impl IntoIterator<Item = thinoda_core::Rect>,
    tolerance: f32,
) -> DragGuides {
    let tolerance = if tolerance.is_finite() {
        tolerance.max(0.0)
    } else {
        0.0
    };
    let references: Vec<_> = references.into_iter().take(MAX_GUIDE_REFERENCES).collect();
    let coordinates = |rect: thinoda_core::Rect| [rect.min, rect.center(), rect.max()];
    let mut guides = DragGuides::default();
    let mut nearest = [tolerance; 2];
    for rect in &references {
        for (source, reference) in coordinates(moving).into_iter().zip(coordinates(*rect)) {
            let distances = [
                (source.x - reference.x).abs(),
                (source.y - reference.y).abs(),
            ];
            for axis in 0..2 {
                if distances[axis] <= tolerance
                    && (guides.alignment[axis].is_none() || distances[axis] < nearest[axis])
                {
                    nearest[axis] = distances[axis];
                    guides.alignment[axis] = Some(if axis == 0 {
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

    for axis in 0..2 {
        let moving_size = axis_size(moving, axis);
        let mut best_size: Option<((f32, f32, usize), [thinoda_core::Rect; 2])> = None;
        for (index, rect) in references.iter().enumerate() {
            let difference = (moving_size - axis_size(*rect, axis)).abs();
            if difference > tolerance {
                continue;
            }
            let center_delta = moving.center() - rect.center();
            let distance_squared =
                center_delta.x * center_delta.x + center_delta.y * center_delta.y;
            let score = (difference, distance_squared, index);
            if best_size.is_none_or(|(best, _)| {
                score.0 < best.0
                    || (score.0 == best.0
                        && (score.1 < best.1 || (score.1 == best.1 && score.2 < best.2)))
            }) {
                best_size = Some((score, [moving, *rect]));
            }
        }
        guides.matching_size[axis] = best_size.map(|(_, bounds)| bounds);
        guides.equal_spacing[axis] = equal_spacing_for_axis(moving, &references, axis, tolerance);
    }
    guides
}

fn axis_size(rect: thinoda_core::Rect, axis: usize) -> f32 {
    if axis == 0 { rect.size.x } else { rect.size.y }
}

fn axis_start(rect: thinoda_core::Rect, axis: usize) -> f32 {
    if axis == 0 { rect.min.x } else { rect.min.y }
}

fn axis_end(rect: thinoda_core::Rect, axis: usize) -> f32 {
    if axis == 0 {
        rect.max().x
    } else {
        rect.max().y
    }
}

fn axis_cross_start(rect: thinoda_core::Rect, axis: usize) -> f32 {
    axis_start(rect, 1 - axis)
}

fn axis_cross_end(rect: thinoda_core::Rect, axis: usize) -> f32 {
    axis_end(rect, 1 - axis)
}

fn equal_spacing_for_axis(
    moving: thinoda_core::Rect,
    references: &[thinoda_core::Rect],
    axis: usize,
    tolerance: f32,
) -> Option<[thinoda_core::Rect; 3]> {
    let moving_start = axis_start(moving, axis);
    let moving_end = axis_end(moving, axis);
    let moving_cross_start = axis_cross_start(moving, axis);
    let moving_cross_end = axis_cross_end(moving, axis);
    let cross_tolerance = tolerance;
    let mut before: Vec<(f32, usize, thinoda_core::Rect)> = Vec::with_capacity(2);
    let mut after: Vec<(f32, usize, thinoda_core::Rect)> = Vec::with_capacity(2);
    for (index, rect) in references.iter().copied().enumerate() {
        if axis_cross_start(rect, axis) > moving_cross_end + cross_tolerance
            || axis_cross_end(rect, axis) < moving_cross_start - cross_tolerance
        {
            continue;
        }
        let start = axis_start(rect, axis);
        let end = axis_end(rect, axis);
        if end <= moving_start {
            before.push((end, index, rect));
            before.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
            before.truncate(2);
        } else if start >= moving_end {
            after.push((start, index, rect));
            after.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
            after.truncate(2);
        }
    }

    let mut best = None;
    if let Some((_, _, nearest)) = before.first() {
        consider_spacing(
            [
                before.get(1).map(|(_, _, rect)| *rect),
                Some(*nearest),
                Some(moving),
            ],
            axis,
            tolerance,
            &mut best,
        );
    }
    if let Some((_, _, nearest)) = after.first() {
        consider_spacing(
            [
                Some(moving),
                Some(*nearest),
                after.get(1).map(|(_, _, rect)| *rect),
            ],
            axis,
            tolerance,
            &mut best,
        );
    }
    if let (Some((_, _, nearest_before)), Some((_, _, nearest_after))) =
        (before.first(), after.first())
    {
        consider_spacing(
            [Some(*nearest_before), Some(moving), Some(*nearest_after)],
            axis,
            tolerance,
            &mut best,
        );
    }
    best.map(|(_, bounds)| bounds)
}

fn consider_spacing(
    candidate: [Option<thinoda_core::Rect>; 3],
    axis: usize,
    tolerance: f32,
    best: &mut Option<(f32, [thinoda_core::Rect; 3])>,
) {
    let [Some(first), Some(middle), Some(last)] = candidate else {
        return;
    };
    let first_gap = axis_start(middle, axis) - axis_end(first, axis);
    let second_gap = axis_start(last, axis) - axis_end(middle, axis);
    let difference = (first_gap - second_gap).abs();
    if first_gap <= 0.0 || second_gap <= 0.0 || difference > tolerance {
        return;
    }
    if best.is_none_or(|(best_difference, _)| difference < best_difference) {
        *best = Some((difference, [first, middle, last]));
    }
}

pub(crate) fn paint_drag_guides(painter: &Painter, xf: Xf, color: Color32, guides: DragGuides) {
    let stroke = Stroke::new(1.0, color);
    for guide in guides.alignment.into_iter().flatten() {
        painter.line_segment(guide.map(|point| xf.pos(point)), stroke);
    }

    let zoom = xf.zoom.max(f32::EPSILON);
    let offset = 6.0 / zoom;
    let tick = 2.5 / zoom;
    for axis in 0..2 {
        if let Some([moving, reference]) = guides.matching_size[axis] {
            for rect in [moving, reference] {
                if axis == 0 {
                    let y = rect.min.y - offset;
                    painter.line_segment(
                        [
                            xf.pos(Vec2::new(rect.min.x, y)),
                            xf.pos(Vec2::new(rect.max().x, y)),
                        ],
                        stroke,
                    );
                    for x in [rect.min.x, rect.max().x] {
                        painter.line_segment(
                            [
                                xf.pos(Vec2::new(x, y - tick)),
                                xf.pos(Vec2::new(x, y + tick)),
                            ],
                            stroke,
                        );
                    }
                } else {
                    let x = rect.min.x - offset;
                    painter.line_segment(
                        [
                            xf.pos(Vec2::new(x, rect.min.y)),
                            xf.pos(Vec2::new(x, rect.max().y)),
                        ],
                        stroke,
                    );
                    for y in [rect.min.y, rect.max().y] {
                        painter.line_segment(
                            [
                                xf.pos(Vec2::new(x - tick, y)),
                                xf.pos(Vec2::new(x + tick, y)),
                            ],
                            stroke,
                        );
                    }
                }
            }
        }

        if let Some([first, middle, last]) = guides.equal_spacing[axis] {
            let cross = axis_cross_start(middle, axis)
                + (axis_cross_end(middle, axis) - axis_cross_start(middle, axis)) / 2.0;
            let gaps = [
                (axis_end(first, axis), axis_start(middle, axis)),
                (axis_end(middle, axis), axis_start(last, axis)),
            ];
            for (start, end) in gaps {
                if axis == 0 {
                    painter.line_segment(
                        [
                            xf.pos(Vec2::new(start, cross)),
                            xf.pos(Vec2::new(end, cross)),
                        ],
                        stroke,
                    );
                    for x in [start, end] {
                        painter.line_segment(
                            [
                                xf.pos(Vec2::new(x, cross - tick)),
                                xf.pos(Vec2::new(x, cross + tick)),
                            ],
                            stroke,
                        );
                    }
                } else {
                    painter.line_segment(
                        [
                            xf.pos(Vec2::new(cross, start)),
                            xf.pos(Vec2::new(cross, end)),
                        ],
                        stroke,
                    );
                    for y in [start, end] {
                        painter.line_segment(
                            [
                                xf.pos(Vec2::new(cross - tick, y)),
                                xf.pos(Vec2::new(cross + tick, y)),
                            ],
                            stroke,
                        );
                    }
                }
            }
        }
    }
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
            let guides = drag_guides(layout, members, delta, 4.0 / xf.zoom);
            paint_drag_guides(painter, xf, view.accent, guides);
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
        let guide = drag_guides(&layout, &moving, delta, 4.0).alignment[0].unwrap();
        assert_eq!(guide[0].x, first_rect.center().x, "Ties follow paint order");
        let delta = Vec2::new(second_rect.center().x - group.center().x - 0.5, 0.0);
        let guide = drag_guides(&layout, &moving, delta, 4.0).alignment[0].unwrap();
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
            let guides = drag_guides(&layout, &owned, delta, tolerance).alignment;
            assert!(guides[0].is_some());
            assert!(guides[1].is_none());
            let delta = Vec2::new(b.center().x - a.center().x + 4.1 / zoom, 0.0);
            assert!(drag_guides(&layout, &owned, delta, tolerance).alignment[0].is_none());
            let delta = Vec2::new(0.0, b.min.y - a.min.y);
            let horizontal = drag_guides(&layout, &owned, delta, tolerance).alignment[1].unwrap();
            assert_eq!(horizontal[0].y, b.min.y);
            assert_eq!(horizontal[1].y, b.min.y);
        }
        let all = layout.order.iter().copied().collect();
        assert_eq!(
            drag_guides(&layout, &all, Vec2::ZERO, 4.0).alignment,
            [None, None]
        );
        assert_eq!(
            drag_guides(&layout, &BTreeSet::new(), Vec2::ZERO, 4.0).alignment,
            [None, None]
        );
        // Hidden topics cannot contribute guides even if a layout map retains their rectangles.
        let mut hidden_only = layout.clone();
        hidden_only.order.retain(|id| *id == moving);
        assert_eq!(
            drag_guides(&hidden_only, &owned, Vec2::ZERO, 4.0).alignment,
            [None, None]
        );
        assert_eq!(
            drag_guides(&hidden_only, &owned, Vec2::ZERO, 4.0),
            DragGuides::default(),
            "a moving group cannot match itself for size or spacing"
        );
    }

    #[test]
    fn matching_size_and_equal_spacing_use_zoom_scaled_tolerance() {
        let moving = thinoda_core::Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 50.0));
        for zoom in [0.5, 1.0, 2.0] {
            let tolerance = 4.0 / zoom;
            let difference = 3.9 / zoom;
            let reference = thinoda_core::Rect::from_min_size(
                Vec2::new(300.0, 100.0),
                Vec2::new(100.0 + difference, 50.0 + difference),
            );
            let guides = drag_guides_for_bounds(moving, [reference], tolerance);
            assert_eq!(guides.matching_size, [Some([moving, reference]); 2]);
            let too_different = thinoda_core::Rect::from_min_size(
                Vec2::new(300.0, 100.0),
                Vec2::new(100.0 + 4.1 / zoom, 50.0 + 4.1 / zoom),
            );
            assert_eq!(
                drag_guides_for_bounds(moving, [too_different], tolerance).matching_size,
                [None, None]
            );

            let moving =
                thinoda_core::Rect::from_min_size(Vec2::new(30.0, 0.0), Vec2::new(20.0, 20.0));
            let left = thinoda_core::Rect::from_min_size(Vec2::ZERO, Vec2::new(20.0, 20.0));
            let right = thinoda_core::Rect::from_min_size(
                Vec2::new(60.0 + difference, 0.0),
                Vec2::new(20.0, 20.0),
            );
            let guides = drag_guides_for_bounds(moving, [left, right], tolerance);
            assert_eq!(guides.equal_spacing[0], Some([left, moving, right]));
            let right = right.translate(Vec2::new(4.1 / zoom, 0.0));
            assert_eq!(
                drag_guides_for_bounds(moving, [left, right], tolerance).equal_spacing[0],
                None
            );
        }
    }

    #[test]
    fn size_and_spacing_do_not_emit_false_guides_without_references() {
        let moving = thinoda_core::Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 50.0));
        assert_eq!(
            drag_guides_for_bounds(moving, [], 4.0),
            DragGuides::default()
        );
        let nearby_but_unequal =
            thinoda_core::Rect::from_min_size(Vec2::new(105.0, 0.0), Vec2::new(80.0, 20.0));
        let guides = drag_guides_for_bounds(moving, [nearby_but_unequal], 4.0);
        assert_eq!(guides.matching_size, [None, None]);
        assert_eq!(guides.equal_spacing, [None, None]);
    }

    #[test]
    fn large_guide_reference_sets_have_a_deterministic_work_limit() {
        use std::cell::Cell;

        let moving = thinoda_core::Rect::from_min_size(Vec2::ZERO, Vec2::new(20.0, 20.0));
        let consumed = Cell::new(0);
        let references = (0..10_000).map(|index| {
            consumed.set(consumed.get() + 1);
            thinoda_core::Rect::from_min_size(
                Vec2::new(500.0 + index as f32 * 25.0, 0.0),
                Vec2::new(20.0, 20.0),
            )
        });
        let first = drag_guides_for_bounds(moving, references, 4.0);
        assert_eq!(consumed.get(), MAX_GUIDE_REFERENCES);

        let second = drag_guides_for_bounds(
            moving,
            (0..10_000).map(|index| {
                thinoda_core::Rect::from_min_size(
                    Vec2::new(500.0 + index as f32 * 25.0, 0.0),
                    Vec2::new(20.0, 20.0),
                )
            }),
            4.0,
        );
        assert_eq!(first, second);
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
