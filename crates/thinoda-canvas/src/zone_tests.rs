use super::*;
use egui_kittest::Harness;
use thinoda_core::{Workbook, ZoneId};

struct State {
    doc: Document,
    view: MapView,
    errors: Vec<thinoda_core::EditError>,
    zone: ZoneId,
    topic: TopicId,
    enabled: bool,
}

fn harness(zoom: f32) -> Harness<'static, State> {
    let mut sheet = Sheet::new("Zones", "Center");
    let topic = sheet.add_floating("Floating", Vec2::new(-100.0, -80.0));
    sheet.insert_child(topic, "Child").unwrap();
    let zone = sheet
        .create_zone(
            "Group",
            vec![topic],
            thinoda_core::Rect::from_min_size(Vec2::new(-260.0, -210.0), Vec2::new(520.0, 360.0)),
        )
        .unwrap();
    let mut view = MapView::default();
    view.select_object(ObjectSelection::Zone(zone));
    let mut h = Harness::builder()
        .with_size(vec2(1800.0, 1200.0))
        .build_ui_state(
            |ui, state: &mut State| {
                ui.add_enabled_ui(state.enabled, |ui| {
                    if let Some(error) = MapCanvas::show(ui, &mut state.doc, &mut state.view).error
                    {
                        state.errors.push(error);
                    }
                });
            },
            State {
                doc: Document::new(Workbook::new(sheet)),
                view,
                errors: Vec::new(),
                zone,
                topic,
                enabled: true,
            },
        );
    h.run_steps(3);
    h.state_mut().view.set_zoom(zoom);
    h.run_steps(2);
    h
}

fn drag(h: &mut Harness<'_, State>, resize: bool, delta: EVec2) -> Pos2 {
    let state = h.state();
    let frame = state.doc.sheet().zones[0].frame;
    let start = state.view.xf().pos(if resize {
        frame.max()
    } else {
        frame.top_center()
    });
    h.hover_at(start);
    h.drag_at(start);
    h.run_steps(1);
    let end = start + delta;
    h.hover_at(start + delta * 0.5);
    h.run_steps(2);
    h.hover_at(end);
    h.run_steps(2);
    assert!(matches!(h.state().view.drag, Drag::Zone { .. }));
    end
}

#[test]
fn zone_move_and_resize_stage_until_release_and_undo_as_one_edit() {
    for zoom in [0.5, 1.0, 2.0] {
        for resize in [false, true] {
            let mut h = harness(zoom);
            let before = h.state().doc.workbook().clone();
            let end = drag(&mut h, resize, vec2(70.0, 40.0) * zoom);
            assert_eq!(h.state().doc.workbook(), &before);
            assert!(!h.state().doc.is_dirty());
            h.drop_at(end);
            h.run_steps(3);
            assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
            let state = h.state();
            let old = &before.sheets[0].zones[0];
            let current = &state.doc.sheet().zones[0];
            let delta = Vec2::new(70.0, 40.0);
            if resize {
                assert_eq!(current.frame.min, old.frame.min);
                assert_eq!(current.frame.size, old.frame.size + delta);
                assert_eq!(
                    state.doc.sheet().topic(state.topic),
                    before.sheets[0].topic(state.topic)
                );
            } else {
                assert_eq!(current.frame.min, old.frame.min + delta);
                assert_eq!(current.frame.size, old.frame.size);
                assert_eq!(
                    state
                        .doc
                        .sheet()
                        .topic(state.topic)
                        .unwrap()
                        .position
                        .unwrap(),
                    before.sheets[0]
                        .topic(state.topic)
                        .unwrap()
                        .position
                        .unwrap()
                        + delta
                );
            }
            assert!(h.state_mut().doc.undo());
            assert_eq!(h.state().doc.workbook(), &before);
            assert!(
                !h.state_mut().doc.undo(),
                "a pointer gesture must create only one Undo entry"
            );
        }
    }
}

#[test]
fn zone_resize_stops_at_minimum_without_moving_topics() {
    let mut h = harness(1.0);
    let topic = h.state().topic;
    let before = h.state().doc.sheet().topic(topic).unwrap().clone();
    let end = drag(&mut h, true, vec2(-500.0, -500.0));
    h.drop_at(end);
    h.run_steps(2);
    assert_eq!(
        h.state().doc.sheet().zones[0].frame.size,
        Vec2::new(100.0, 100.0)
    );
    assert_eq!(h.state().doc.sheet().topic(topic), Some(&before));
}

#[test]
fn zone_gestures_cancel_on_escape_revision_camera_focus_touch_disabled_or_outside_release() {
    for cause in 0..7 {
        let mut h = harness(1.0);
        let end = drag(&mut h, false, vec2(60.0, 30.0));
        match cause {
            0 => h.key_press(Key::Escape),
            1 => {
                let state = h.state_mut();
                state
                    .doc
                    .edit_sheet("External change", |sheet| {
                        sheet.set_title(state.topic, "Changed elsewhere")
                    })
                    .unwrap();
            }
            2 => h.state_mut().view.offset += vec2(10.0, 0.0),
            3 => h.input_mut().focused = false,
            4 => h.event(egui::Event::Touch {
                device_id: egui::TouchDeviceId(0),
                id: egui::TouchId(1),
                phase: egui::TouchPhase::Cancel,
                pos: end,
                force: None,
            }),
            5 => h.state_mut().enabled = false,
            _ => {}
        }
        let before = h.state().doc.workbook().clone();
        h.run_steps(2);
        let outside = h.state().view.canvas_rect.max + vec2(20.0, 20.0);
        h.drop_at(if cause == 6 { outside } else { end });
        h.run_steps(3);
        assert_eq!(
            h.state().doc.workbook(),
            &before,
            "cancellation cause {cause}"
        );
        assert!(matches!(h.state().view.drag, Drag::None));
    }
}

#[test]
fn zone_hit_testing_prefers_topmost_frame_without_claiming_its_interior() {
    let mut h = harness(1.0);
    let frame = h.state().doc.sheet().zones[0].frame;
    let first = h.state().zone;
    assert_eq!(
        hit_zone(h.state().doc.sheet(), frame.top_center(), 6.0),
        Some(ObjectSelection::Zone(first))
    );
    assert_eq!(hit_zone(h.state().doc.sheet(), frame.center(), 6.0), None);
    let second = {
        let state = h.state_mut();
        let root = state
            .doc
            .edit_sheet("Floating", |sheet| {
                Ok(sheet.add_floating("Second", Vec2::new(120.0, 0.0)))
            })
            .unwrap();
        state
            .doc
            .create_zone("Overlapping", vec![root], frame)
            .unwrap()
    };
    assert_eq!(
        hit_zone(h.state().doc.sheet(), frame.top_center(), 6.0),
        Some(ObjectSelection::Zone(second))
    );
    let state = h.state_mut();
    state.doc.reorder_zone(first, 1).unwrap();
    assert_eq!(
        hit_zone(state.doc.sheet(), frame.top_center(), 6.0),
        Some(ObjectSelection::Zone(first))
    );
}
