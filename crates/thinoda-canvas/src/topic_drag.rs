//! View-only subtree previews; the document changes only on a valid drop.

use std::collections::{BTreeSet, HashMap, HashSet};

use egui::{Color32, Painter, Stroke};
use thinoda_core::{
    BranchSide, CanvasObjectKind, EditError, Endpoint, Layout, Scene, SceneHit, SceneId, Sheet,
    Structure, TopicId, Vec2, ZoneId,
    board::{BoardMember, rotated_corners},
    layout::ConnectorKind,
    theme::level_of,
};

use crate::{
    MapView, PaintOptions, Xf,
    render::{GhostClip, PaintSceneExtras, connector_path, paint_map_scene},
};

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
    scene: &Scene,
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
    let Some(SceneHit::Topic(topic)) = scene.hit_test_tolerance(at, 0.0) else {
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

fn moving_object_clip(
    sheet: &Sheet,
    object: thinoda_core::CanvasObjectId,
    center: Vec2,
    delta: Vec2,
    moving_zone: Option<ZoneId>,
) -> Option<thinoda_core::Rect> {
    let frame = |zone: &thinoda_core::Zone| {
        if moving_zone == Some(zone.id) {
            zone.frame.translate(delta)
        } else {
            zone.frame
        }
    };
    if let Some(owner) = sheet
        .zones
        .iter()
        .find(|zone| zone.objects.contains(&object))
    {
        return owner.clip.then(|| frame(owner));
    }
    sheet.zones.iter().find_map(|zone| {
        let frame = frame(zone);
        (zone.clip && frame.contains(center)).then_some(frame)
    })
}

fn ghost_scene(
    sheet: &Sheet,
    scene: &Scene,
    members: &BTreeSet<TopicId>,
    delta: Vec2,
    moving_zone: Option<ZoneId>,
) -> Scene {
    let connectors: HashSet<_> = sheet
        .canvas
        .objects
        .iter()
        .filter_map(|object| match &object.kind {
            CanvasObjectKind::Connector(connector)
                if matches!(connector.from, Endpoint::Topic(id) if members.contains(&id))
                    && matches!(connector.to, Endpoint::Topic(id) if members.contains(&id)) =>
            {
                Some(object.id)
            }
            _ => None,
        })
        .collect();
    Scene {
        grid: None,
        items: scene
            .items
            .iter()
            .filter(|item| matches!(item.id, SceneId::Object(id) if connectors.contains(&id)))
            .map(|item| {
                let mut item = item.clone();
                let center = item.bounds.center() + delta;
                if let SceneId::Object(id) = item.id {
                    item.clip = moving_object_clip(sheet, id, center, delta, moving_zone)
                        .map(|frame| frame.translate(delta * -1.0));
                }
                item
            })
            .collect(),
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_ghost(
    painter: &Painter,
    sheet: &Sheet,
    layout: &Layout,
    scene: &Scene,
    view: &MapView,
    members: &BTreeSet<TopicId>,
    delta: Vec2,
    moving_zone: Option<ZoneId>,
) {
    let xf = view.xf();
    let ghost = ghost_layout(sheet, layout, members);
    let scene = ghost_scene(sheet, scene, members, delta, moving_zone);
    paint_map_scene(
        painter,
        sheet,
        &ghost,
        Xf {
            origin: xf.origin + egui::vec2(delta.x, delta.y) * xf.zoom,
            ..xf
        },
        &PaintOptions {
            dark: view.dark,
            accent: view.accent,
            omit_zones: true,
            ..Default::default()
        },
        &scene,
        PaintSceneExtras {
            ghost_clip: Some(GhostClip { delta, moving_zone }),
            ..Default::default()
        },
    );
}

pub(crate) fn zone_board_members(
    sheet: &Sheet,
    roots: &[TopicId],
    objects: &[thinoda_core::CanvasObjectId],
) -> Result<Vec<BoardMember>, EditError> {
    let cards: Vec<_> = roots
        .iter()
        .copied()
        .filter(|root| sheet.topic(*root).is_some_and(|topic| topic.card.is_some()))
        .map(BoardMember::Card)
        .chain(objects.iter().copied().map(BoardMember::Object))
        .collect();
    if cards.is_empty() {
        return Ok(Vec::new());
    }
    sheet
        .expand_board_selection(&cards)
        .map_err(EditError::Invalid)
}

pub(crate) fn zone_moving_topics(
    sheet: &Sheet,
    topics: &BTreeSet<TopicId>,
    board_members: &[BoardMember],
) -> BTreeSet<TopicId> {
    topics
        .iter()
        .copied()
        .chain(
            board_members
                .iter()
                .filter_map(|member| match member {
                    BoardMember::Card(id) if !topics.contains(id) => Some(*id),
                    _ => None,
                })
                .flat_map(|id| sheet.owned_subtree_ids(id)),
        )
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_zone_members(
    painter: &Painter,
    sheet: &Sheet,
    scene: &Scene,
    view: &MapView,
    zone: ZoneId,
    topics: &BTreeSet<TopicId>,
    board_members: &[BoardMember],
    moving_topics: &BTreeSet<TopicId>,
    delta: Vec2,
) {
    if board_members
        .iter()
        .all(|member| matches!(member, BoardMember::Card(id) if topics.contains(id)))
    {
        return;
    }
    let selected: HashSet<_> = board_members.iter().copied().collect();
    let objects: HashMap<_, _> = sheet
        .canvas
        .objects
        .iter()
        .filter(|object| selected.contains(&BoardMember::Object(object.id)))
        .map(|object| (object.id, object))
        .collect();
    let follows = |endpoint| match endpoint {
        Endpoint::Point(_) => true,
        Endpoint::Topic(id) => moving_topics.contains(&id),
        Endpoint::Object(id) => {
            selected.contains(&BoardMember::Object(id))
                && objects
                    .get(&id)
                    .is_some_and(|object| !object.is_connector())
        }
    };
    let xf = view.xf();
    for item in &scene.items {
        let clip = match item.id {
            SceneId::Topic(id) if moving_topics.contains(&id) && !topics.contains(&id) => {
                thinoda_core::scene::topic_clip_zone(sheet, id).map(|(owner, frame)| {
                    if owner == zone {
                        frame.translate(delta)
                    } else {
                        frame
                    }
                })
            }
            SceneId::Object(id) if selected.contains(&BoardMember::Object(id)) => {
                let Some(object) = objects.get(&id) else {
                    continue;
                };
                if let CanvasObjectKind::Connector(connector) = &object.kind
                    && !(follows(connector.from) && follows(connector.to))
                {
                    continue;
                }
                let center = match object.kind {
                    CanvasObjectKind::Connector(_) => item.bounds.center(),
                    _ => object.rect.center(),
                } + delta;
                moving_object_clip(sheet, id, center, delta, Some(zone))
            }
            _ => continue,
        };
        let clipped = clip.map(|clip| painter.with_clip_rect(xf.rect(clip)));
        let painter = clipped.as_ref().unwrap_or(painter);
        painter.rect_stroke(
            xf.rect(item.bounds.translate(delta)),
            2.0,
            Stroke::new(2.0, view.accent),
            egui::StrokeKind::Outside,
        );
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

#[allow(clippy::too_many_arguments)]
pub(crate) fn paint(
    painter: &Painter,
    sheet: &Sheet,
    layout: &Layout,
    scene: &Scene,
    view: &MapView,
    members: &BTreeSet<TopicId>,
    delta: Vec2,
    current: Vec2,
) {
    let mut translucent = painter.clone();
    translucent.set_opacity(0.55);
    paint_ghost(
        &translucent,
        sheet,
        layout,
        scene,
        view,
        members,
        delta,
        None,
    );
    let xf = view.xf();
    let target = target(sheet, layout, scene, members, current);
    if matches!(target, Ok(None)) {
        if view.show_alignment_guides {
            let guides = drag_guides(layout, members, delta, 4.0 / xf.zoom);
            paint_drag_guides(painter, xf, view.accent, guides);
        }
        return;
    }
    let Some(SceneHit::Topic(topic)) = scene.hit_test_tolerance(current, 0.0) else {
        return;
    };
    let rect = &layout.topics[&topic].rect;
    let color = if target.is_ok() {
        view.accent
    } else {
        Color32::from_rgb(220, 60, 60)
    };
    let rotation = sheet
        .topic(topic)
        .and_then(|topic| topic.card)
        .map_or(0.0, |card| card.rotation);
    {
        let clipped = thinoda_core::scene::topic_clip_rect(sheet, topic)
            .map(|clip| painter.with_clip_rect(xf.rect(clip)));
        let painter = clipped.as_ref().unwrap_or(painter);
        if rotation == 0.0 {
            painter.rect_stroke(
                xf.rect(*rect).expand(4.0),
                8.0,
                Stroke::new(2.5, color),
                egui::StrokeKind::Outside,
            );
        } else {
            painter.add(egui::Shape::closed_line(
                rotated_corners(rect.expand(4.0 / xf.zoom), rotation)
                    .into_iter()
                    .map(|point| xf.pos(point))
                    .collect(),
                Stroke::new(2.5, color),
            ));
        }
    }
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
    use thinoda_core::{ApproxMeasure, build_scene, layout_sheet};

    #[test]
    fn topic_drag_ghost_uses_the_view_automatic_stroke() {
        use thinoda_core::{CanvasConnector, CanvasObject, Endpoint, SheetRootMode};

        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let first = sheet.add_floating("First", Vec2::new(-100.0, 0.0));
        let second = sheet.add_floating("Second", Vec2::new(100.0, 0.0));
        let mut connector = CanvasConnector::new(Endpoint::Topic(first), Endpoint::Topic(second));
        connector.arrow_end = false;
        sheet
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let members = members(&sheet, &[first, second]);
        for dark in [false, true] {
            let layout = layout_sheet(&sheet, &ApproxMeasure, dark);
            let scene = build_scene(&sheet, &layout);
            for accent in [Color32::from_rgb(221, 12, 179), Color32::TRANSPARENT] {
                let mut view = MapView::default();
                view.accent = accent;
                view.dark = dark;
                let ctx = egui::Context::default();
                let mut output = ctx.run_ui(Default::default(), |ui| {
                    paint(
                        ui.painter(),
                        &sheet,
                        &layout,
                        &scene,
                        &view,
                        &members,
                        Vec2::ZERO,
                        Vec2::new(10_000.0, 10_000.0),
                    );
                });
                output.textures_delta.clear();
                let stroke = crate::automatic_board_stroke(accent, dark).gamma_multiply(0.55);
                assert!(output.shapes.iter().any(|shape| matches!(
                    &shape.shape,
                    egui::Shape::Path(path)
                        if !path.closed
                            && path.stroke.color == egui::epaint::ColorMode::Solid(stroke)
                )));
            }
        }
    }

    #[test]
    fn topic_ghost_contains_only_connectors_between_moving_topics() {
        use thinoda_core::{
            Band, CanvasConnector, CanvasObject, Color, InkStroke, InkTool, Rect, ShapeKind,
        };

        let mut sheet = Sheet::new("Map", "Root");
        let first = sheet.add_floating("First", Vec2::new(-100.0, 0.0));
        let second = sheet.add_floating("Second", Vec2::new(100.0, 0.0));
        let stationary = sheet.add_floating("Stationary", Vec2::new(300.0, 0.0));
        let mut shape = CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_center_size(Vec2::new(0.0, 100.0), Vec2::new(80.0, 40.0)),
        );
        shape.band = Band::Below;
        let shape = sheet.add_canvas_object(shape).unwrap();
        let ink = sheet
            .add_canvas_object(CanvasObject::ink(InkStroke::new(
                InkTool::Pen,
                Color::rgb(20, 180, 90),
                3.0,
                vec![Vec2::new(-50.0, 120.0), Vec2::new(50.0, 120.0)],
            )))
            .unwrap();
        let free = sheet
            .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
                Endpoint::Point(Vec2::new(-80.0, 150.0)),
                Endpoint::Point(Vec2::new(80.0, 150.0)),
            )))
            .unwrap();
        let outside = sheet
            .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
                Endpoint::Topic(first),
                Endpoint::Topic(stationary),
            )))
            .unwrap();
        let inside = sheet
            .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
                Endpoint::Topic(first),
                Endpoint::Topic(second),
            )))
            .unwrap();
        let before = sheet.clone();
        let layout = layout_sheet(&sheet, &ApproxMeasure, false);
        let members = members(&sheet, &[first, second]);
        let ghost = ghost_layout(&sheet, &layout, &members);
        assert_eq!(ghost.order.len(), 2);
        let original_scene = build_scene(&sheet, &layout);
        for id in [shape, ink, free, inside] {
            assert!(original_scene.item(SceneId::Object(id)).is_some());
        }
        let scene = ghost_scene(&sheet, &original_scene, &members, Vec2::ZERO, None);
        let objects: Vec<_> = scene
            .items
            .iter()
            .filter_map(|item| match item.id {
                SceneId::Object(id) => Some(id),
                _ => None,
            })
            .collect();
        assert_eq!(objects, vec![inside]);
        assert_eq!(scene.items.len(), 1);
        assert!(scene.item(SceneId::Object(outside)).is_none());
        let render = |sheet: &Sheet, scene: &Scene| {
            let ctx = egui::Context::default();
            let mut output = ctx.run_ui(Default::default(), |ui| {
                paint_ghost(
                    ui.painter(),
                    sheet,
                    &layout,
                    scene,
                    &MapView::default(),
                    &members,
                    Vec2::new(30.0, 40.0),
                    None,
                );
            });
            output.textures_delta.clear();
            output.shapes
        };
        let mut reference = sheet.clone();
        reference
            .canvas
            .objects
            .retain(|object| object.id == inside);
        let reference_scene = build_scene(&reference, &layout);
        assert_eq!(
            render(&sheet, &original_scene),
            render(&reference, &reference_scene)
        );
        assert_eq!(sheet, before);
    }

    #[test]
    fn topic_ghost_keeps_stationary_frame_clips_and_moves_the_dragged_frame_clip() {
        use thinoda_core::Rect;

        let mut sheet = Sheet::new("Map", "Root");
        let root = sheet.add_floating("Moving", Vec2::ZERO);
        let child = sheet.insert_child(root, "Child").unwrap();
        sheet.add_relationship(root, child).unwrap();
        let frame = Rect::from_min_size(Vec2::new(-180.0, -120.0), Vec2::new(360.0, 240.0));
        let zone = sheet.create_zone("Frame", vec![root], frame).unwrap();
        sheet.zones[0].clip = true;
        let layout = layout_sheet(&sheet, &ApproxMeasure, false);
        let scene = build_scene(&sheet, &layout);
        let members = members(&sheet, &[root]);
        let view = MapView::default();
        let delta = Vec2::new(100.0, 60.0);
        let xf = view.xf();

        for moving_zone in [None, Some(zone)] {
            let ctx = egui::Context::default();
            let mut painter_clip = egui::Rect::NOTHING;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    painter_clip = ui.painter().clip_rect();
                    paint_ghost(
                        ui.painter(),
                        &sheet,
                        &layout,
                        &scene,
                        &view,
                        &members,
                        delta,
                        moving_zone,
                    );
                },
            );
            let target_frame = if moving_zone.is_some() {
                frame.translate(delta)
            } else {
                frame
            };
            let expected = painter_clip.intersect(xf.rect(target_frame));
            output.textures_delta.clear();
            assert!(
                !output.shapes.is_empty()
                    && output
                        .shapes
                        .iter()
                        .all(|shape| shape.clip_rect == expected),
                "ghost shapes must be clipped to {target_frame:?}"
            );
        }
    }

    #[test]
    fn ghost_connector_clip_follows_destination_geometry_and_frame_motion() {
        use thinoda_core::{CanvasConnector, CanvasObject, Rect};

        let mut sheet = Sheet::new("Map", "Root");
        let first = sheet.add_floating("First", Vec2::new(250.0, 0.0));
        let second = sheet.add_floating("Second", Vec2::new(350.0, 0.0));
        let frame = Rect::from_min_size(Vec2::new(-100.0, -100.0), Vec2::new(200.0, 200.0));
        let zone = sheet
            .create_zone("Frame", vec![first, second], frame)
            .unwrap();
        sheet.zones[0].clip = true;
        let connector = sheet
            .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
                Endpoint::Topic(first),
                Endpoint::Topic(second),
            )))
            .unwrap();
        let layout = layout_sheet(&sheet, &ApproxMeasure, false);
        let scene = build_scene(&sheet, &layout);
        let item = scene.item(SceneId::Object(connector)).unwrap();
        assert_eq!(item.clip, None);
        let members = members(&sheet, &[first, second]);
        let delta = frame.center() - item.bounds.center();
        let stationary = ghost_scene(&sheet, &scene, &members, delta, None);
        assert_eq!(
            stationary.item(SceneId::Object(connector)).unwrap().clip,
            Some(frame.translate(delta * -1.0))
        );
        let moving = ghost_scene(&sheet, &scene, &members, delta, Some(zone));
        assert_eq!(moving.item(SceneId::Object(connector)).unwrap().clip, None);
    }

    #[test]
    fn zone_preview_includes_explicit_objects_outside_the_frame() {
        use thinoda_core::{CanvasObject, Rect, ShapeKind};
        let mut sheet = Sheet::new("Map", "Root");
        let object = sheet
            .add_canvas_object(CanvasObject::shape(
                ShapeKind::Rectangle,
                Rect::from_min_size(Vec2::new(500.0, 500.0), Vec2::new(80.0, 60.0)),
            ))
            .unwrap();
        let frame = Rect::from_min_size(Vec2::ZERO, Vec2::new(200.0, 200.0));
        let zone = sheet.create_board_frame("Frame", frame).unwrap();
        sheet.set_zone_members(zone, vec![], vec![object]).unwrap();
        sheet.zones[0].clip = true;
        assert_eq!(
            zone_board_members(&sheet, &[], &[object]).unwrap(),
            vec![BoardMember::Object(object)]
        );
        let delta = Vec2::new(20.0, 30.0);
        assert_eq!(
            moving_object_clip(&sheet, object, Vec2::new(560.0, 560.0), delta, Some(zone)),
            Some(frame.translate(delta))
        );
        assert_eq!(
            moving_object_clip(&sheet, object, Vec2::new(560.0, 560.0), delta, None),
            Some(frame)
        );
    }

    #[test]
    fn zone_group_outlines_only_actual_rigid_members_with_their_target_clips() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, Card, CardKind, Rect, ShapeKind, SheetRootMode,
        };

        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let card = |sheet: &mut Sheet, title, center| {
            sheet
                .add_card(
                    title,
                    center,
                    Card::new(CardKind::Sticky, Vec2::new(80.0, 60.0)),
                )
                .unwrap()
        };
        let inside = card(&mut sheet, "Inside", Vec2::new(-80.0, 0.0));
        let outside = card(&mut sheet, "Outside", Vec2::new(250.0, 0.0));
        let outside_child = sheet.insert_child(outside, "Child").unwrap();
        let frame = Rect::from_min_size(Vec2::new(-180.0, -150.0), Vec2::new(320.0, 300.0));
        let other_frame = Rect::from_min_size(Vec2::new(150.0, -150.0), Vec2::new(300.0, 300.0));
        let zone = sheet.create_zone("Moving", vec![inside], frame).unwrap();
        let other = sheet
            .create_zone("Stationary", vec![outside], other_frame)
            .unwrap();
        for zone in &mut sheet.zones {
            zone.clip = true;
        }
        let add_shape = |sheet: &mut Sheet, center| {
            sheet
                .add_canvas_object(CanvasObject::shape(
                    ShapeKind::Rectangle,
                    Rect::from_center_size(center, Vec2::new(40.0, 30.0)),
                ))
                .unwrap()
        };
        let in_frame = add_shape(&mut sheet, Vec2::new(-40.0, 70.0));
        let in_other = add_shape(&mut sheet, Vec2::new(300.0, 70.0));
        let stationary = add_shape(&mut sheet, Vec2::new(350.0, -70.0));
        let inside_stationary = add_shape(&mut sheet, Vec2::new(-140.0, -70.0));
        let free = sheet
            .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
                Endpoint::Point(Vec2::new(-120.0, 110.0)),
                Endpoint::Point(Vec2::new(-60.0, 110.0)),
            )))
            .unwrap();
        let stretched = sheet
            .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
                Endpoint::Topic(inside),
                Endpoint::Object(stationary),
            )))
            .unwrap();
        let grouped_link = sheet
            .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
                Endpoint::Topic(inside),
                Endpoint::Topic(outside_child),
            )))
            .unwrap();
        sheet
            .group_board_members(&[
                BoardMember::Card(inside),
                BoardMember::Card(outside),
                BoardMember::Object(in_frame),
                BoardMember::Object(in_other),
                BoardMember::Object(free),
                BoardMember::Object(stretched),
                BoardMember::Object(grouped_link),
            ])
            .unwrap();
        let before = sheet.clone();
        let board_members = zone_board_members(&sheet, &[inside], &[]).unwrap();
        assert!(!board_members.contains(&BoardMember::Object(stationary)));
        let topics = members(&sheet, &[inside]);
        let moving_topics = zone_moving_topics(&sheet, &topics, &board_members);
        assert!(moving_topics.contains(&inside));
        assert!(moving_topics.contains(&outside));
        assert!(moving_topics.contains(&outside_child));
        let layout = layout_sheet(&sheet, &ApproxMeasure, false);
        let scene = build_scene(&sheet, &layout);
        let view = MapView::default();
        let xf = view.xf();
        let delta = Vec2::new(30.0, 40.0);
        let ctx = egui::Context::default();
        let mut painter_clip = egui::Rect::NOTHING;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 900.0),
                )),
                ..Default::default()
            },
            |ui| {
                painter_clip = ui.painter().clip_rect();
                paint_zone_members(
                    ui.painter(),
                    &sheet,
                    &scene,
                    &view,
                    zone,
                    &topics,
                    &board_members,
                    &moving_topics,
                    delta,
                );
            },
        );
        output.textures_delta.clear();
        let outline = |id| {
            let rect = xf.rect(scene.item(id).unwrap().bounds.translate(delta));
            output.shapes.iter().find(|shape| {
                matches!(
                    &shape.shape,
                    egui::Shape::Rect(outline)
                        if outline.rect == rect && outline.stroke.color == view.accent
                )
            })
        };
        assert_eq!(
            outline(SceneId::Object(in_frame)).unwrap().clip_rect,
            painter_clip.intersect(xf.rect(frame.translate(delta)))
        );
        assert_eq!(
            outline(SceneId::Object(in_other)).unwrap().clip_rect,
            painter_clip.intersect(xf.rect(other_frame))
        );
        assert_eq!(
            outline(SceneId::Topic(outside)).unwrap().clip_rect,
            painter_clip.intersect(xf.rect(other_frame))
        );
        assert_eq!(
            outline(SceneId::Topic(outside_child)).unwrap().clip_rect,
            painter_clip.intersect(xf.rect(other_frame))
        );
        assert!(outline(SceneId::Object(free)).is_some());
        assert!(outline(SceneId::Object(grouped_link)).is_some());
        assert!(outline(SceneId::Object(stretched)).is_none());
        assert!(outline(SceneId::Object(stationary)).is_none());
        assert!(outline(SceneId::Object(inside_stationary)).is_none());
        assert_eq!(sheet, before);

        sheet.move_zone(zone, delta).unwrap();
        for id in [in_frame, in_other, free, stretched, grouped_link] {
            assert_eq!(
                sheet.canvas.object(id).unwrap().rect,
                before.canvas.object(id).unwrap().rect.translate(delta)
            );
        }
        assert_eq!(
            sheet.canvas.object(stationary),
            before.canvas.object(stationary)
        );
        assert_eq!(
            sheet.canvas.object(inside_stationary),
            before.canvas.object(inside_stationary)
        );
        for id in [inside, outside] {
            assert_eq!(
                sheet.topic(id).unwrap().position,
                before
                    .topic(id)
                    .unwrap()
                    .position
                    .map(|position| position + delta)
            );
        }
        let moved_layout = layout_sheet(&sheet, &ApproxMeasure, false);
        assert_eq!(
            moved_layout.rect(outside_child).unwrap().center(),
            layout.rect(outside_child).unwrap().center() + delta
        );
        assert_eq!(
            sheet
                .zones
                .iter()
                .find(|item| item.id == other)
                .unwrap()
                .frame,
            other_frame
        );
    }

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
