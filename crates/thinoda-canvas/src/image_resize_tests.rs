use super::*;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_core::{Workbook, resources::Resource};

struct State {
    doc: Document,
    view: MapView,
    enabled: bool,
    errors: Vec<thinoda_core::EditError>,
}

fn harness(zoom: f32, below: bool) -> Harness<'static, State> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(160, 80)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    let mut sheet = Sheet::new("Images", "Center");
    let root = sheet.root;
    thinoda_core::images::set_image(
        &mut sheet,
        root,
        Some((
            Resource::new("test.png", "image/png", bytes.into_inner()).unwrap(),
            Vec2::new(160.0, 80.0),
        )),
    )
    .unwrap();
    sheet.topics.get_mut(&root).unwrap().image_below = below;
    let mut view = MapView::default();
    view.select_only(root);
    let mut h = Harness::builder()
        .with_size(vec2(1200.0, 900.0))
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
                enabled: true,
                errors: Vec::new(),
            },
        );
    h.run_steps(3);
    h.state_mut().view.set_zoom(zoom);
    h.run_steps(2);
    h
}

fn size(h: &Harness<'_, State>) -> Vec2 {
    h.state()
        .doc
        .sheet()
        .topic(h.state().doc.sheet().root)
        .unwrap()
        .image_size
        .unwrap()
}

fn drag(h: &mut Harness<'_, State>, delta: EVec2) -> Pos2 {
    let start = h.get_by_label("Resize image").rect().center();
    h.hover_at(start);
    h.drag_at(start);
    h.run_steps(1);
    h.hover_at(start + delta * 0.5);
    h.run_steps(2);
    h.hover_at(start + delta);
    h.run_steps(2);
    assert!(h.state().view.image_resize.is_some());
    assert!(matches!(h.state().view.drag, Drag::None));
    start + delta
}

#[test]
fn resize_math_projects_diagonal_preserves_ratio_and_clamps_both_axes() {
    for original in [
        Vec2::new(160.0, 80.0),
        Vec2::new(80.0, 160.0),
        Vec2::new(100.0, 100.0),
        Vec2::new(0.01, 40.0),
    ] {
        assert_eq!(resized(original, Vec2::ZERO), original);
        for delta in [
            Vec2::new(40.0, 20.0),
            Vec2::new(0.0, 80.0),
            Vec2::new(100000.0, 100000.0),
            Vec2::new(-100000.0, -100000.0),
        ] {
            let result = resized(original, delta);
            assert!(valid_size(result), "{result:?}");
            assert!((result.x / result.y - original.x / original.y).abs() < 0.0001);
        }
    }
    assert_eq!(
        resized(Vec2::new(160.0, 80.0), Vec2::new(40.0, 20.0)),
        Vec2::new(200.0, 100.0)
    );
    assert_eq!(
        resized(Vec2::new(160.0, 80.0), Vec2::new(10000.0, 0.0)),
        Vec2::new(1024.0, 512.0)
    );
    assert_eq!(
        resized(Vec2::new(80.0, 160.0), Vec2::new(0.0, 10000.0)),
        Vec2::new(512.0, 1024.0)
    );
}

#[test]
fn image_resize_stages_until_release_and_is_one_undo_step_at_each_zoom_and_placement() {
    for zoom in [0.5, 1.0, 2.0] {
        for below in [false, true] {
            let mut h = harness(zoom, below);
            let original = h.state().doc.workbook().clone();
            let end = drag(&mut h, vec2(40.0, 20.0) * zoom);
            assert_eq!(h.state().doc.workbook(), &original);
            assert!(!h.state().doc.is_dirty());
            h.drop_at(end);
            h.run_steps(3);
            assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
            assert_eq!(size(&h), Vec2::new(200.0, 100.0));
            let resized = h.state().doc.workbook().clone();
            assert!(h.state_mut().doc.undo());
            assert_eq!(h.state().doc.workbook(), &original);
            assert!(!h.state_mut().doc.undo());
            assert!(h.state_mut().doc.redo());
            assert_eq!(h.state().doc.workbook(), &resized);
        }
    }
}

#[test]
fn image_resize_cancels_stale_or_interrupted_gestures_without_an_edit() {
    for interruption in 0..16 {
        let mut h = harness(1.0, false);
        let end = drag(&mut h, vec2(40.0, 20.0));
        match interruption {
            0 => h.state_mut().view.zoom = 1.1,
            1 => h.state_mut().view.offset.x += 1.0,
            2 => h.state_mut().view.selection.clear(),
            3 => h.state_mut().enabled = false,
            4 => h.event(egui::Event::WindowFocused(false)),
            5 => h.key_press(Key::Escape),
            6 => h.event(egui::Event::PointerGone),
            7 => h.event(egui::Event::Touch {
                device_id: egui::TouchDeviceId(1),
                id: egui::TouchId(1),
                phase: egui::TouchPhase::Cancel,
                pos: end,
                force: None,
            }),
            8 => {
                let id = h.state().doc.sheet().root;
                h.state_mut()
                    .doc
                    .edit_sheet("Title", |s| s.set_title(id, "Changed"))
                    .unwrap();
            }
            9 => {
                // A fresh document can have the same revision and sheet index.
                h.state_mut().doc = Document::new(Workbook::default());
            }
            10 => {
                let state = h.state_mut();
                state
                    .view
                    .begin_edit_with(state.doc.sheet(), state.doc.sheet().root, "draft");
            }
            11 => {
                h.set_size(vec2(1000.0, 800.0));
            }
            12 => h.input_mut().focused = false,
            13 => h.ctx.memory_mut(|m| m.request_focus(MapCanvas::id())),
            14 => egui::Popup::open_id(&h.ctx, Id::unique("resize-blocking-popup")),
            _ => {
                let mut book = h.state().doc.workbook().clone();
                book.sheets.push(book.sheets[0].clone());
                h.state_mut().doc = Document::new(book);
                h.state_mut().doc.set_active(1);
            }
        }
        h.run_steps(2);
        let before = h.state().doc.workbook().clone();
        let revision = h.state().doc.state_id();
        h.drop_at(end);
        h.run_steps(3);
        assert_eq!(
            h.state().doc.workbook(),
            &before,
            "interruption {interruption}"
        );
        assert_eq!(
            h.state().doc.state_id(),
            revision,
            "interruption {interruption}"
        );
        assert!(h.state().view.image_resize.is_none());
        egui::Popup::close_all(&h.ctx);
    }
}

#[test]
fn image_handle_keyboard_and_click_noop_are_accessible_and_undoable() {
    let mut h = harness(1.0, false);
    h.get_by_label("Resize image").click();
    h.run_steps(2);
    assert_eq!(size(&h), Vec2::new(160.0, 80.0));
    assert!(!h.state().doc.is_dirty());
    h.get_by_label("Resize image").focus();
    h.key_press(Key::ArrowRight);
    h.run_steps(2);
    assert_eq!(size(&h), Vec2::new(161.0, 80.5));
    assert!(h.state_mut().doc.undo());
    assert_eq!(size(&h), Vec2::new(160.0, 80.0));
    h.state_mut().view.image_resize_label = "Bildgröße ändern".into();
    h.run_steps(2);
    h.get_by_label("Bildgröße ändern");
}

#[test]
fn outside_release_or_pointer_loss_never_commits_and_hidden_selection_has_no_handle() {
    let mut h = harness(1.0, false);
    drag(&mut h, vec2(40.0, 20.0));
    h.drop_at(pos2(-100.0, -100.0));
    h.run_steps(3);
    assert!(!h.state().doc.is_dirty());
    h.state_mut().view.selection.clear();
    h.run_steps(2);
    assert!(h.query_by_label("Resize image").is_none());
}

#[test]
fn direct_resize_reaches_core_maximum_and_positive_minimum() {
    for (delta, expected) in [
        (vec2(200.0, 100.0), Vec2::new(1024.0, 512.0)),
        (vec2(-200.0, -100.0), Vec2::new(1.0, 0.5)),
    ] {
        let mut h = harness(0.1, false);
        let end = drag(&mut h, delta);
        h.drop_at(end);
        h.run_steps(3);
        assert_eq!(size(&h), expected);
        assert!(h.state().errors.is_empty());
        assert!(h.state_mut().doc.undo());
        assert_eq!(size(&h), Vec2::new(160.0, 80.0));
    }
}

fn touch(h: &mut Harness<'_, State>, phase: egui::TouchPhase, pos: Pos2) {
    h.event(egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(1),
        phase,
        pos,
        force: None,
    });
    h.event(egui::Event::PointerMoved(pos));
    if matches!(phase, egui::TouchPhase::Start | egui::TouchPhase::End) {
        h.event(egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: phase == egui::TouchPhase::Start,
            modifiers: Modifiers::NONE,
        });
    }
    h.run_steps(1);
}

#[test]
fn single_touch_resizes_and_second_touch_cancels_without_becoming_a_topic_drag() {
    for second_touch in [false, true] {
        let mut h = harness(1.0, true);
        let start = h.get_by_label("Resize image").rect().center();
        touch(&mut h, egui::TouchPhase::Start, start);
        touch(&mut h, egui::TouchPhase::Move, start + vec2(20.0, 10.0));
        let end = start + vec2(40.0, 20.0);
        touch(&mut h, egui::TouchPhase::Move, end);
        if second_touch {
            h.event(egui::Event::Touch {
                device_id: egui::TouchDeviceId(1),
                id: egui::TouchId(2),
                phase: egui::TouchPhase::Start,
                pos: start - vec2(30.0, 0.0),
                force: None,
            });
            h.run_steps(2);
            h.event(egui::Event::Touch {
                device_id: egui::TouchDeviceId(1),
                id: egui::TouchId(2),
                phase: egui::TouchPhase::End,
                pos: start - vec2(30.0, 0.0),
                force: None,
            });
        }
        touch(&mut h, egui::TouchPhase::End, end);
        h.run_steps(3);
        assert_eq!(
            size(&h),
            if second_touch {
                Vec2::new(160.0, 80.0)
            } else {
                Vec2::new(200.0, 100.0)
            }
        );
        assert!(matches!(h.state().view.drag, Drag::None));
        assert_eq!(h.state_mut().doc.undo(), !second_touch);
    }
}
