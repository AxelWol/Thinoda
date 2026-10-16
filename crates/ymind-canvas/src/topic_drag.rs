//! View-only subtree previews; the document changes only on a valid drop.

use std::collections::BTreeSet;

use egui::{Color32, Painter, Stroke};
use ymind_core::{EditError, Layout, Sheet, TopicId, Vec2};

use crate::{MapView, PaintOptions, Xf, paint_map};

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
    layout: &Layout,
    members: &BTreeSet<TopicId>,
    at: Vec2,
) -> Result<Option<TopicId>, EditError> {
    let target = layout.hit_test(at);
    if target.is_some_and(|id| members.contains(&id)) {
        Err(EditError::Cycle)
    } else {
        Ok(target)
    }
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

fn ghost_layout(sheet: &Sheet, layout: &Layout, members: &BTreeSet<TopicId>) -> Layout {
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
            ..Default::default()
        },
    );
    let target = target(layout, members, current);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use ymind_core::{ApproxMeasure, layout_sheet};

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
