//! Relationship edits stay in view state until the pointer is released.

use crate::{MapCanvas, MapView, ObjectSelection};
use egui::{Id, Key, Modifiers, Sense, Stroke, StrokeKind};
use std::sync::Arc;
use thinoda_core::{
    Document, EditError, Layout, Rect, Relationship, TopicId, Vec2,
    layout::{RelationshipLayout, relationship_curve},
};

pub(crate) struct RelationshipDrag {
    source: Relationship,
    part: usize,
    state: u64,
    sheet: usize,
    grab: Vec2,
    origin: Vec2,
    point: Vec2,
    candidate: Option<TopicId>,
}

fn controls(rel: &Relationship, curve: &RelationshipLayout, layout: &Layout) -> [Vec2; 2] {
    rel.control_points.unwrap_or([
        curve.c1 - layout.topics[&rel.from].rect.center(),
        curve.c2 - layout.topics[&rel.to].rect.center(),
    ])
}

fn edited(drag: &RelationshipDrag, curve: &RelationshipLayout, layout: &Layout) -> Relationship {
    let mut rel = drag.source.clone();
    match drag.part {
        0 => {
            if let Some(topic) = drag.candidate {
                rel.from = topic;
            }
        }
        3 => {
            if let Some(topic) = drag.candidate {
                rel.to = topic;
            }
        }
        part => {
            let mut points = controls(&rel, curve, layout);
            let index = part - 1;
            let topic = if index == 0 { rel.from } else { rel.to };
            points[index] = drag.point - layout.topics[&topic].rect.center();
            rel.control_points = Some(points);
        }
    }
    rel
}

fn arrow(ui: &egui::Ui) -> Option<Vec2> {
    for (modifiers, step) in [(Modifiers::SHIFT, 10.0), (Modifiers::NONE, 1.0)] {
        for (key, delta) in [
            (Key::ArrowLeft, Vec2::new(-step, 0.0)),
            (Key::ArrowRight, Vec2::new(step, 0.0)),
            (Key::ArrowUp, Vec2::new(0.0, -step)),
            (Key::ArrowDown, Vec2::new(0.0, step)),
        ] {
            if ui.input_mut(|i| i.consume_key(modifiers, key)) {
                return Some(delta);
            }
        }
    }
    None
}

fn next_endpoint(rel: &Relationship, part: usize, delta: Vec2, layout: &Layout) -> TopicId {
    let (current, other) = if part == 0 {
        (rel.from, rel.to)
    } else {
        (rel.to, rel.from)
    };
    let choices: Vec<_> = layout
        .order
        .iter()
        .copied()
        .filter(|id| *id != other)
        .collect();
    let index = choices
        .iter()
        .position(|id| *id == current)
        .expect("visible relationship endpoint is present in the layout");
    let step = if delta.x + delta.y > 0.0 { 1 } else { -1 };
    choices[(index as isize + step).rem_euclid(choices.len() as isize) as usize]
}

pub(crate) fn interact(
    ui: &mut egui::Ui,
    doc: &mut Document,
    view: &mut MapView,
    layout: &mut Arc<Layout>,
    error: &mut Option<EditError>,
    enabled: bool,
) -> bool {
    let Some(ObjectSelection::Relationship(id)) = view.selected_object else {
        view.relationship_drag = None;
        return false;
    };
    let Some(rel) = doc
        .sheet()
        .relationships
        .iter()
        .find(|r| r.id == id)
        .cloned()
    else {
        view.relationship_drag = None;
        return false;
    };
    let Some(curve) = layout.relationships.iter().find(|r| r.id == id).cloned() else {
        view.relationship_drag = None;
        return false;
    };
    let ctx = ui.ctx().clone();
    if !ui.is_enabled() || egui::Popup::is_any_open(&ctx) {
        view.relationship_drag = None;
        return false;
    }
    let xf = view.xf();
    let mut active = view.relationship_drag.take();
    let mut blocked = active.is_some();
    if active.as_ref().is_some_and(|drag| {
        drag.source.id != id || drag.state != doc.state_id() || drag.sheet != doc.active_index()
    }) {
        *error = Some(EditError::Invalid(
            "relationship editing canceled because the document changed".into(),
        ));
        return true;
    }
    if active.is_some() && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
        ctx.memory_mut(|m| m.request_focus(MapCanvas::id()));
        return true;
    }
    if !enabled && active.is_none() {
        return false;
    }
    let positions = [curve.start, curve.c1, curve.c2, curve.end];
    let mut released = false;
    // Endpoints win when a control lies on a topic edge.
    for part in [1, 2, 0, 3] {
        let point = positions[part];
        let rect = egui::Rect::from_center_size(xf.pos(point), egui::vec2(22.0, 22.0));
        if !view.canvas_rect().intersects(rect)
            && active.as_ref().is_none_or(|drag| drag.part != part)
        {
            continue;
        }
        let response = ui.interact(
            rect,
            Id::unique("relationship-handle").with((id.0, part)),
            Sense::click_and_drag() | Sense::FOCUSABLE,
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::Role::Button,
                true,
                &view.relationship_handle_labels[part],
            )
        });
        blocked |= response.hovered()
            || response.dragged()
            || response.drag_stopped()
            || ctx
                .input(|i| i.pointer.press_origin())
                .is_some_and(|p| rect.contains(p));
        if response.clicked() || response.drag_started() {
            response.request_focus();
        }
        if response.drag_started() && active.is_none() {
            active = Some(RelationshipDrag {
                source: rel.clone(),
                part,
                state: doc.state_id(),
                sheet: doc.active_index(),
                grab: xf.sheet(
                    ctx.input(|i| i.pointer.press_origin())
                        .unwrap_or(rect.center()),
                ),
                origin: point,
                point,
                candidate: None,
            });
        }
        released |= response.drag_stopped();
        if response.has_focus() && active.is_none() {
            ctx.memory_mut(|m| {
                m.set_focus_lock_filter(
                    response.id,
                    egui::EventFilter {
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: true,
                        ..Default::default()
                    },
                );
            });
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
                ctx.memory_mut(|m| m.request_focus(MapCanvas::id()));
            }
            if let Some(delta) = arrow(ui) {
                let result = if part == 0 || part == 3 {
                    let topic = next_endpoint(&rel, part, delta, layout);
                    let (from, to) = if part == 0 {
                        (topic, rel.to)
                    } else {
                        (rel.from, topic)
                    };
                    doc.edit_sheet("Reconnect Relationship", |s| {
                        s.reconnect_relationship(id, from, to)
                    })
                } else {
                    let mut points = controls(&rel, &curve, layout);
                    points[part - 1] = points[part - 1] + delta;
                    doc.edit_sheet("Reshape Relationship", |s| {
                        s.set_relationship_control_points(id, Some(points))
                    })
                };
                if let Err(e) = result {
                    *error = Some(e);
                }
                blocked = true;
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Delete)) {
                if let Err(e) = doc.edit_sheet("Delete Relationship", |s| s.remove_relationship(id))
                {
                    *error = Some(e);
                }
                view.selected_object = None;
                return true;
            }
        }
        response.on_hover_cursor(egui::CursorIcon::Grab);
    }
    if let Some(mut drag) = active {
        if let Some(point) = ctx.pointer_latest_pos() {
            drag.point = drag.origin + xf.sheet(point) - drag.grab;
            drag.candidate = view
                .canvas_rect()
                .contains(point)
                .then(|| layout.hit_test(drag.point))
                .flatten();
        }
        let changed = edited(&drag, &curve, layout);
        if released {
            let result = if drag.part == 0 || drag.part == 3 {
                if drag.candidate.is_none() {
                    Err(EditError::Invalid(
                        "drop a relationship endpoint on another topic".into(),
                    ))
                } else {
                    doc.edit_sheet("Reconnect Relationship", |s| {
                        s.reconnect_relationship(id, changed.from, changed.to)
                    })
                }
            } else {
                doc.edit_sheet("Reshape Relationship", |s| {
                    s.set_relationship_control_points(id, changed.control_points)
                })
            };
            if let Err(e) = result {
                *error = Some(e);
            }
            ctx.memory_mut(|m| m.request_focus(MapCanvas::id()));
        } else if ctx.input(|i| i.pointer.primary_down()) {
            let mut a = layout.topics[&changed.from].rect;
            let mut b = layout.topics[&changed.to].rect;
            if drag.candidate.is_none() {
                if drag.part == 0 {
                    a = Rect::from_center_size(drag.point, Vec2::ZERO);
                }
                if drag.part == 3 {
                    b = Rect::from_center_size(drag.point, Vec2::ZERO);
                }
            }
            if let Some(target) = Arc::make_mut(layout)
                .relationships
                .iter_mut()
                .find(|r| r.id == id)
            {
                *target = relationship_curve(&changed, a, b);
            }
            view.relationship_drag = Some(drag);
        }
    }
    blocked
}

pub(crate) fn paint_target(painter: &egui::Painter, view: &MapView, layout: &Layout) {
    let Some(drag) = view
        .relationship_drag
        .as_ref()
        .filter(|drag| drag.part == 0 || drag.part == 3)
    else {
        return;
    };
    if let Some(id) = drag.candidate {
        let other = if drag.part == 0 {
            drag.source.to
        } else {
            drag.source.from
        };
        painter.rect_stroke(
            view.xf().rect(layout.topics[&id].rect).expand(3.0),
            6.0,
            Stroke::new(
                2.0,
                if id == other {
                    egui::Color32::RED
                } else {
                    view.accent
                },
            ),
            StrokeKind::Outside,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Color32, Key};
    use egui_kittest::{Harness, kittest::Queryable};
    use thinoda_core::{Color, Sheet, Workbook};

    struct State {
        doc: Document,
        view: MapView,
        error: Option<EditError>,
    }

    fn harness(zoom: f32) -> Harness<'static, State> {
        let mut sheet = Sheet::new("Relationships", "Center");
        let a = sheet.add_floating("A", Vec2::new(-250.0, -150.0));
        let b = sheet.add_floating("B", Vec2::new(250.0, -150.0));
        sheet.add_floating("Target", Vec2::new(180.0, 160.0));
        let id = sheet.add_relationship(a, b).unwrap();
        sheet
            .set_relationship_color(id, Some(Color::rgb(9, 18, 27)))
            .unwrap();
        let doc = Document::new(Workbook {
            sheets: vec![sheet],
            active_sheet: 0,
            metadata: Default::default(),
        });
        let mut view = MapView::default();
        view.select_object(ObjectSelection::Relationship(id));
        let mut h = Harness::builder()
            .with_size(egui::vec2(1600.0, 1100.0))
            .build_ui_state(
                |ui, s: &mut State| {
                    s.error = MapCanvas::show(ui, &mut s.doc, &mut s.view).error;
                },
                State {
                    doc,
                    view,
                    error: None,
                },
            );
        h.run_steps(3);
        h.state_mut().view.set_zoom(zoom);
        h.run_steps(2);
        h
    }

    fn start_drag(h: &mut Harness<'_, State>, name: &str, delta: egui::Vec2) -> egui::Pos2 {
        let start = h.get_by_label(name).rect().center();
        h.hover_at(start);
        h.drag_at(start);
        h.run_steps(1);
        let end = start + delta;
        h.hover_at(end);
        h.run_steps(2);
        assert!(
            h.state().view.relationship_drag.is_some(),
            "handle drag did not start: {name}"
        );
        end
    }

    #[test]
    fn relationship_curve_drags_preview_without_edits_and_commit_once_at_each_zoom() {
        for zoom in [0.5, 1.0, 2.0] {
            for (index, name) in ["First curve control", "Second curve control"]
                .into_iter()
                .enumerate()
            {
                let mut h = harness(zoom);
                let before = h.state().doc.workbook().clone();
                let state = h.state().doc.state_id();
                let layout = h
                    .state_mut()
                    .view
                    .layout_for(&Document::new(before.clone()));
                let rel = &before.sheets[0].relationships[0];
                let original = controls(rel, &layout.relationships[0], &layout);
                let end = start_drag(&mut h, name, egui::vec2(40.0, -30.0));
                assert_eq!(h.state().doc.state_id(), state);
                assert_eq!(h.state().doc.workbook(), &before);
                h.drop_at(end);
                h.run_steps(2);
                assert!(h.state().error.is_none());
                assert!(h.state().view.relationship_drag.is_none());
                let points = h.state().doc.sheet().relationships[0]
                    .control_points
                    .unwrap();
                let expected = original[index] + Vec2::new(40.0 / zoom, -30.0 / zoom);
                assert!((points[index].x - expected.x).abs() < 0.01);
                assert!((points[index].y - expected.y).abs() < 0.01);
                assert_eq!(points[1 - index], original[1 - index]);
                let after = h.state().doc.workbook().clone();
                assert!(h.state_mut().doc.undo());
                assert_eq!(h.state().doc.workbook(), &before);
                assert!(h.state_mut().doc.redo());
                assert_eq!(h.state().doc.workbook(), &after);
            }
        }
    }

    #[test]
    fn relationship_endpoints_reconnect_without_moving_topics_at_each_zoom() {
        for zoom in [0.5, 1.0, 2.0] {
            for (part, name) in [(0, "Relationship start"), (3, "Relationship end")] {
                let mut h = harness(zoom);
                let before = h.state().doc.workbook().clone();
                let target = h
                    .state()
                    .doc
                    .sheet()
                    .topics
                    .values()
                    .find(|t| t.title == "Target")
                    .unwrap()
                    .id;
                let layout = h
                    .state_mut()
                    .view
                    .layout_for(&Document::new(before.clone()));
                let target_pos = h
                    .state()
                    .view
                    .xf()
                    .pos(layout.topics[&target].rect.center());
                let start = h.get_by_label(name).rect().center();
                start_drag(&mut h, name, target_pos - start);
                assert_eq!(h.state().doc.workbook(), &before);
                h.drop_at(target_pos);
                h.run_steps(2);
                assert!(h.state().error.is_none());
                let rel = &h.state().doc.sheet().relationships[0];
                assert_eq!(if part == 0 { rel.from } else { rel.to }, target);
                assert_eq!(h.state().doc.sheet().topics, before.sheets[0].topics);
                assert_eq!(rel.color, before.sheets[0].relationships[0].color);
                assert!(h.state_mut().doc.undo());
                assert_eq!(h.state().doc.workbook(), &before);
            }
        }
    }

    #[test]
    fn relationship_escape_invalid_drop_and_stale_preview_leave_source_unchanged() {
        for how in 0..4 {
            let mut h = harness(1.0);
            let start = h.get_by_label("Relationship start").rect().center();
            let end = if how == 1 {
                h.get_by_label("Relationship end").rect().center()
            } else {
                start + egui::vec2(0.0, -90.0)
            };
            start_drag(&mut h, "Relationship start", end - start);
            if how == 0 {
                h.key_press(Key::Escape);
                h.run_steps(1);
            }
            if how == 3 {
                let root = h.state().doc.sheet().root;
                h.state_mut()
                    .doc
                    .edit_sheet("External edit", |s| s.set_title(root, "Newer title"))
                    .unwrap();
                h.run_steps(1);
                assert!(h.state().view.relationship_drag.is_none());
                assert!(h.state().error.is_some());
            }
            let before = h.state().doc.workbook().clone();
            h.drop_at(end);
            h.run_steps(2);
            assert!(h.state().view.relationship_drag.is_none());
            assert_eq!(h.state().doc.workbook(), &before);
        }
    }

    #[test]
    fn relationship_focused_handles_support_arrow_edits_and_delete() {
        let mut h = harness(1.0);
        h.get_by_label("First curve control").click();
        h.run_steps(2);
        let before = h.state().doc.workbook().clone();
        let layout = {
            let s = h.state_mut();
            s.view.layout_for(&s.doc)
        };
        let original = controls(
            &before.sheets[0].relationships[0],
            &layout.relationships[0],
            &layout,
        );
        h.key_press(Key::ArrowRight);
        h.run_steps(2);
        assert!(
            h.state().doc.sheet().relationships[0]
                .control_points
                .is_some()
        );
        let moved = h.state().doc.sheet().relationships[0]
            .control_points
            .unwrap();
        assert_eq!(moved[0], original[0] + Vec2::new(1.0, 0.0));
        h.key_press_modifiers(Modifiers::SHIFT, Key::ArrowDown);
        h.run_steps(2);
        assert_eq!(
            h.state().doc.sheet().relationships[0]
                .control_points
                .unwrap()[0],
            moved[0] + Vec2::new(0.0, 10.0)
        );
        assert!(h.state_mut().doc.undo());
        assert_eq!(
            h.state().doc.sheet().relationships[0]
                .control_points
                .unwrap(),
            moved
        );
        assert!(h.state_mut().doc.undo());
        assert_eq!(h.state().doc.workbook(), &before);
        h.run_steps(2);
        h.get_by_label("Relationship end").click();
        h.run_steps(2);
        h.key_press(Key::ArrowRight);
        h.run_steps(2);
        assert_ne!(
            h.state().doc.sheet().relationships[0].to,
            before.sheets[0].relationships[0].to
        );
        h.key_press(Key::Delete);
        h.run_steps(2);
        assert!(h.state().doc.sheet().relationships.is_empty());
        assert!(h.state_mut().doc.undo());
        assert_eq!(h.state().doc.sheet().relationships.len(), 1);
    }

    #[test]
    fn relationship_selection_preserves_original_color_and_exposes_four_handles() {
        for dark in [false, true] {
            let mut h = harness(1.0);
            h.state_mut().view.dark = dark;
            h.run_steps(2);
            for name in [
                "Relationship start",
                "First curve control",
                "Second curve control",
                "Relationship end",
            ] {
                h.get_by_label(name);
            }
            assert!(h.output().shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::LineSegment { stroke, .. } if stroke.color == Color32::from_rgb(9,18,27)
        )));
            h.state_mut().view.selected_object = None;
            h.run_steps(2);
            assert!(h.query_by_label("First curve control").is_none());
        }
    }

    #[test]
    fn reshaped_relationship_hit_testing_follows_zoom_and_pan() {
        for zoom in [0.5, 1.0, 2.0] {
            let mut h = harness(zoom);
            let id = h.state().doc.sheet().relationships[0].id;
            h.state_mut()
                .doc
                .edit_sheet("Custom curve", |s| {
                    s.set_relationship_control_points(
                        id,
                        Some([Vec2::new(120.0, -120.0), Vec2::new(-120.0, -120.0)]),
                    )
                })
                .unwrap();
            h.state_mut().view.selected_object = None;
            h.state_mut().view.offset += egui::vec2(27.0, 18.0);
            h.run_steps(2);
            let layout = {
                let s = h.state_mut();
                s.view.layout_for(&s.doc)
            };
            let point = h.state().view.xf().pos(layout.relationships[0].mid);
            assert!(h.state().view.canvas_rect().contains(point));
            h.hover_at(point);
            h.run_steps(1);
            for pressed in [true, false] {
                h.event(egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                });
                h.run_steps(1);
            }
            assert_eq!(
                h.state().view.selected_object,
                Some(ObjectSelection::Relationship(id))
            );
        }
    }

    #[test]
    fn relationship_hit_testing_matches_rendered_segments_on_large_curves() {
        for zoom in [0.5, 1.0, 2.0] {
            let mut h = harness(zoom);
            let id = h.state().doc.sheet().relationships[0].id;
            h.state_mut()
                .doc
                .edit_sheet("Large curve", |s| {
                    s.set_relationship_control_points(
                        id,
                        Some([
                            Vec2::new(100_000.0, 100_000.0),
                            Vec2::new(-100_000.0, 100_000.0),
                        ]),
                    )
                })
                .unwrap();
            let before = h.state().doc.workbook().clone();
            let layout = {
                let s = h.state_mut();
                s.view.layout_for(&s.doc)
            };
            let curve = &layout.relationships[0];
            for segment in [7, 15, 23] {
                let sample = |i| {
                    thinoda_core::layout::cubic(
                        curve.start,
                        curve.c1,
                        curve.c2,
                        curve.end,
                        i as f32 / crate::render::RELATIONSHIP_SEGMENTS as f32,
                    )
                };
                let a = sample(segment);
                let point = a + (sample(segment + 1) - a) * 0.37;
                h.state_mut().view.selected_object = None;
                h.state_mut().view.offset = egui::vec2(-point.x, -point.y) * zoom;
                h.run_steps(2);
                let point = h.state().view.xf().pos(point);
                assert!(h.state().view.canvas_rect().contains(point));
                h.hover_at(point);
                h.run_steps(1);
                for pressed in [true, false] {
                    h.event(egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    });
                    h.run_steps(1);
                }
                assert_eq!(
                    h.state().view.selected_object,
                    Some(ObjectSelection::Relationship(id))
                );
                assert_eq!(h.state().doc.workbook(), &before);
            }
        }
    }

    #[test]
    fn relationship_handle_arrows_do_not_leak_through_open_popups() {
        let mut h = harness(1.0);
        h.get_by_label("First curve control").click();
        h.run_steps(2);
        let before = h.state().doc.workbook().clone();
        egui::Popup::open_id(&h.ctx, Id::unique("blocking-popup"));
        h.key_press(Key::ArrowRight);
        h.run_steps(1);
        assert_eq!(h.state().doc.workbook(), &before);
        egui::Popup::close_all(&h.ctx);
    }
}
