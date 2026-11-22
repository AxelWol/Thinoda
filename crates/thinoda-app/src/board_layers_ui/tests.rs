use egui::{Ui, vec2};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use thinoda_core::{CanvasObject, CanvasObjectId, Rect as BoardRect, ShapeKind, Vec2};

use super::*;

fn add_shape(app: &mut ThinodaApp, x: f32) -> CanvasObjectId {
    let mut id = None;
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Add shape", |sheet| {
            id = Some(
                sheet
                    .add_canvas_object(CanvasObject::shape(
                        ShapeKind::Rectangle,
                        BoardRect::from_min_size(Vec2::new(x, 300.0), Vec2::new(60.0, 40.0)),
                    ))
                    .map_err(thinoda_core::EditError::Invalid)?,
            );
            Ok(())
        })
        .unwrap();
    id.unwrap()
}

fn click(harness: &mut Harness<'_, ThinodaApp>, label: &str) {
    harness.get_by_label(label).click();
    harness.run_steps(3);
}

fn panel_rect(harness: &Harness<'_, ThinodaApp>) -> Rect {
    let id = panel_id(&harness.ctx);
    harness.ctx.memory(|memory| memory.area_rect(id)).unwrap()
}

fn drag(harness: &mut Harness<'_, ThinodaApp>, start: egui::Pos2, delta: egui::Vec2) {
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    harness.event(egui::Event::PointerMoved(start));
    harness.event(button(start, true));
    harness.run_steps(1);
    harness.event(egui::Event::PointerMoved(start + delta * 0.4));
    harness.run_steps(1);
    harness.event(egui::Event::PointerMoved(start + delta));
    harness.run_steps(2);
    harness.event(button(start + delta, false));
    harness.run_steps(3);
}

#[test]
fn layers_panel_shows_six_named_layers_without_scrolling() {
    for add_interactively in [false, true] {
        let mut app = ThinodaApp::headless();
        let doc = app.doc_mut().unwrap();
        doc.show_layers = true;
        if !add_interactively {
            for number in 1..=6 {
                doc.doc.add_board_layer(&format!("Layer {number}")).unwrap();
            }
        }
        let mut harness = Harness::builder()
            .with_size(vec2(900.0, 320.0))
            .build_ui_state(
                |ui: &mut Ui, app: &mut ThinodaApp| {
                    app.board_layers_panel(ui.ctx(), ui.max_rect());
                },
                app,
            );
        harness.run_steps(5);
        if add_interactively {
            for _ in 0..6 {
                click(&mut harness, "Add layer");
            }
        }
        let panel = panel_rect(&harness);
        for number in 1..=6 {
            let label = format!("Hide layer: Layer {number}");
            let rect = harness.get_by_label(&label).rect();
            assert!(
                panel.contains_rect(rect),
                "{label}: {rect:?} outside {panel:?}"
            );
        }
        click(&mut harness, "Hide layer: Layer 1");
        assert!(
            !harness.state().doc().unwrap().doc.sheet().canvas.layers[0].visible,
            "the sixth row must actually be clickable"
        );
    }
}

#[cfg(target_os = "windows")]
#[test]
fn layers_panel_shows_six_rows_with_the_native_ribbon_at_minimum_window_size() {
    for scale in [1.0, 1.5, 2.0] {
        let mut app = ThinodaApp::headless();
        app.desktop_ribbon = true;
        app.settings.show_format_panel = false;
        app.new_doc(thinoda_core::templates::instantiate("blank-canvas"));
        let doc = app.doc_mut().unwrap();
        doc.show_layers = true;
        for number in 1..=6 {
            doc.doc.add_board_layer(&format!("Layer {number}")).unwrap();
        }
        let mut harness = Harness::builder()
            .with_size(vec2(720.0, 480.0))
            .with_context_setup(move |ctx| ctx.set_zoom_factor(scale))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        harness.run_steps(8);
        let panel = panel_rect(&harness);
        for number in 1..=6 {
            let label = format!("Hide layer: Layer {number}");
            let rect = harness.get_by_label(&label).rect();
            assert!(
                panel.contains_rect(rect),
                "{scale} {label}: {rect:?} outside {panel:?}"
            );
        }
        click(&mut harness, "Hide layer: Layer 1");
        assert!(!harness.state().doc().unwrap().doc.sheet().canvas.layers[0].visible);
    }
}

#[test]
fn layers_panel_can_be_dragged_and_reopened_in_place() {
    let directory = tempfile::tempdir().unwrap();
    let paths = thinoda_platform::paths::Paths::from_root(directory.path());
    paths.ensure().unwrap();
    let mut app = ThinodaApp::headless();
    app.services.paths = Some(paths.clone());
    app.doc_mut().unwrap().show_layers = true;
    let mut harness = Harness::builder()
        .with_size(vec2(900.0, 500.0))
        .build_ui_state(
            |ui: &mut Ui, app: &mut ThinodaApp| {
                app.board_layers_panel(ui.ctx(), ui.max_rect());
            },
            app,
        );
    harness.run_steps(4);
    let before = panel_rect(&harness);
    let start = harness
        .get_by_label("Drag or use arrow keys to move the Layers panel")
        .rect()
        .center();
    let start_layer = harness.ctx.layer_id_at(start);
    drag(&mut harness, start, vec2(-160.0, 100.0));
    let title_moved = panel_rect(&harness);
    assert!(
        (title_moved.min - before.min).length() > 100.0,
        "drag did not move the panel: {before:?} -> {title_moved:?}, start {start:?}, layer {start_layer:?}"
    );
    drag(
        &mut harness,
        title_moved.min + vec2(100.0, 15.0),
        vec2(-40.0, -30.0),
    );
    let moved = panel_rect(&harness);
    assert!((moved.min - title_moved.min).length() > 30.0);
    let saved = thinoda_platform::settings::Settings::load(&paths).unwrap();
    assert_eq!(
        saved.layers_panel_position,
        harness.state().settings.layers_panel_position
    );
    assert!(saved.layers_panel_position.is_some());
    click(&mut harness, "Close");
    assert!(!harness.state().doc().unwrap().show_layers);
    harness.state_mut().doc_mut().unwrap().show_layers = true;
    harness.run_steps(4);
    assert_eq!(panel_rect(&harness), moved);

    let mut restarted = ThinodaApp::headless();
    restarted.settings = saved;
    restarted.doc_mut().unwrap().show_layers = true;
    let mut reopened = Harness::builder()
        .with_size(vec2(900.0, 500.0))
        .build_ui_state(
            |ui: &mut Ui, app: &mut ThinodaApp| {
                app.board_layers_panel(ui.ctx(), ui.max_rect());
            },
            restarted,
        );
    reopened.run_steps(5);
    assert_eq!(panel_rect(&reopened), moved);
}

#[test]
fn layers_panel_can_be_moved_with_keyboard_and_restored() {
    let directory = tempfile::tempdir().unwrap();
    let paths = thinoda_platform::paths::Paths::from_root(directory.path());
    paths.ensure().unwrap();
    let mut app = ThinodaApp::headless();
    app.services.paths = Some(paths.clone());
    app.doc_mut().unwrap().show_layers = true;
    let mut harness = Harness::builder()
        .with_size(vec2(900.0, 500.0))
        .build_ui_state(
            |ui: &mut Ui, app: &mut ThinodaApp| {
                app.board_layers_panel(ui.ctx(), ui.max_rect());
            },
            app,
        );
    harness.run_steps(4);
    let title = "Drag or use arrow keys to move the Layers panel";
    harness.get_by_label(title).focus();
    harness.run_steps(2);
    assert!(harness.get_by_label(title).is_focused());
    let before = panel_rect(&harness);
    harness.key_press(egui::Key::ArrowLeft);
    harness.run_steps(3);
    assert_eq!(panel_rect(&harness).min, before.min - vec2(10.0, 0.0));
    harness.key_press(egui::Key::ArrowDown);
    harness.run_steps(3);
    let moved = panel_rect(&harness);
    assert_eq!(moved.min, before.min + vec2(-10.0, 10.0));
    let saved = thinoda_platform::settings::Settings::load(&paths).unwrap();
    assert_eq!(
        saved.layers_panel_position,
        harness.state().settings.layers_panel_position
    );
    harness.key_press(egui::Key::Tab);
    harness.run_steps(2);
    assert!(!harness.get_by_label(title).is_focused());
    harness.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::Tab);
    harness.run_steps(2);
    assert!(harness.get_by_label(title).is_focused());
    harness.key_press(egui::Key::Tab);
    harness.run_steps(2);
    harness.key_press(egui::Key::ArrowLeft);
    harness.run_steps(2);
    assert_eq!(panel_rect(&harness), moved);
    click(&mut harness, "Close");
    harness.state_mut().doc_mut().unwrap().show_layers = true;
    harness.run_steps(3);
    assert_eq!(panel_rect(&harness), moved);
    let mut restarted = ThinodaApp::headless();
    restarted.settings = saved;
    restarted.doc_mut().unwrap().show_layers = true;
    let mut reopened = Harness::builder()
        .with_size(vec2(900.0, 500.0))
        .build_ui_state(
            |ui: &mut Ui, app: &mut ThinodaApp| {
                app.board_layers_panel(ui.ctx(), ui.max_rect());
            },
            restarted,
        );
    reopened.run_steps(4);
    assert_eq!(panel_rect(&reopened), moved);
}

#[test]
fn keyboard_panel_movement_stays_inside_the_viewport() {
    let mut app = ThinodaApp::headless();
    app.settings.layers_panel_position = Some([0.0, 0.0]);
    app.doc_mut().unwrap().show_layers = true;
    let mut harness = Harness::builder()
        .with_size(vec2(900.0, 500.0))
        .build_ui_state(
            |ui: &mut Ui, app: &mut ThinodaApp| {
                app.board_layers_panel(ui.ctx(), ui.max_rect());
            },
            app,
        );
    harness.run_steps(4);
    harness
        .get_by_label("Drag or use arrow keys to move the Layers panel")
        .focus();
    harness.run_steps(2);
    let before = panel_rect(&harness);
    harness.key_press(egui::Key::ArrowLeft);
    harness.run_steps(2);
    harness.key_press(egui::Key::ArrowUp);
    harness.run_steps(2);
    assert_eq!(panel_rect(&harness), before);
    assert!(harness.ctx.content_rect().contains_rect(before));
}

#[test]
fn keyboard_panel_movement_does_not_move_selected_canvas_objects() {
    let mut app = ThinodaApp::headless();
    app.desktop_ribbon = true;
    app.new_doc(thinoda_core::templates::instantiate("blank-canvas"));
    let shape = add_shape(&mut app, 0.0);
    {
        let doc = app.doc_mut().unwrap();
        doc.show_layers = true;
        doc.view.board_selection = vec![shape];
    }
    let object = app
        .doc()
        .unwrap()
        .doc
        .sheet()
        .canvas
        .object(shape)
        .unwrap()
        .clone();
    let mut harness = Harness::builder()
        .with_size(vec2(900.0, 600.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    harness.run_steps(8);
    harness
        .get_by_label("Drag or use arrow keys to move the Layers panel")
        .focus();
    harness.run_steps(2);
    let before = panel_rect(&harness);
    harness.key_press(egui::Key::ArrowLeft);
    harness.run_steps(3);
    assert_eq!(panel_rect(&harness).min, before.min - vec2(10.0, 0.0));
    assert_eq!(
        harness
            .state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .canvas
            .object(shape)
            .unwrap(),
        &object
    );
}

#[test]
fn layers_panel_scrolls_extra_rows_in_a_compact_window() {
    let mut app = ThinodaApp::headless();
    let doc = app.doc_mut().unwrap();
    doc.show_layers = true;
    for number in 1..=9 {
        doc.doc.add_board_layer(&format!("Layer {number}")).unwrap();
    }
    let mut harness = Harness::builder()
        .with_size(vec2(320.0, 240.0))
        .build_ui_state(
            |ui: &mut Ui, app: &mut ThinodaApp| {
                app.board_layers_panel(ui.ctx(), ui.max_rect());
            },
            app,
        );
    harness.run_steps(5);
    assert!(
        harness
            .ctx
            .content_rect()
            .contains_rect(panel_rect(&harness))
    );
    harness.get_by_label("Hide layer: Layer 1").scroll_to_me();
    harness.run_steps(5);
    click(&mut harness, "Hide layer: Layer 1");
    assert!(!harness.state().doc().unwrap().doc.sheet().canvas.layers[0].visible);
    click(&mut harness, "Close");
    assert!(!harness.state().doc().unwrap().show_layers);
}

#[test]
fn restored_layers_panel_is_kept_within_a_smaller_window() {
    let mut app = ThinodaApp::headless();
    app.settings.layers_panel_position = Some([5000.0, 5000.0]);
    app.doc_mut().unwrap().show_layers = true;
    let mut harness = Harness::builder()
        .with_size(vec2(480.0, 320.0))
        .build_ui_state(
            |ui: &mut Ui, app: &mut ThinodaApp| {
                app.board_layers_panel(ui.ctx(), ui.max_rect());
            },
            app,
        );
    harness.run_steps(5);
    assert!(
        harness
            .ctx
            .content_rect()
            .contains_rect(panel_rect(&harness))
    );
    click(&mut harness, "Add layer");
    assert_eq!(
        harness
            .state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .canvas
            .layers
            .len(),
        1
    );
}

#[test]
fn board_toolbar_button_toggles_the_layers_panel() {
    let mut harness = Harness::builder()
        .with_size(vec2(800.0, 800.0))
        .build_ui_state(
            |ui: &mut Ui, app: &mut ThinodaApp| {
                app.board_toolbar(ui.ctx(), ui.max_rect(), None);
            },
            ThinodaApp::headless(),
        );
    harness.run_steps(3);
    click(&mut harness, "Layers");
    assert!(harness.state().doc().unwrap().show_layers);
    click(&mut harness, "Layers");
    assert!(!harness.state().doc().unwrap().show_layers);
}

#[test]
fn layers_panel_adds_moves_hides_and_locks_layers() {
    let mut app = ThinodaApp::headless();
    let shape = add_shape(&mut app, 0.0);
    let other = add_shape(&mut app, 100.0);
    {
        let doc = app.doc_mut().unwrap();
        doc.show_layers = true;
        doc.view.board_selection = vec![shape];
    }
    let mut harness = Harness::builder()
        .with_size(vec2(1400.0, 900.0))
        .build_ui_state(
            |ui: &mut Ui, app: &mut ThinodaApp| {
                app.board_layers_panel(ui.ctx(), ui.max_rect());
            },
            app,
        );
    harness.run_steps(3);
    assert!(
        harness
            .query_by_label_contains("Only the base layer exists")
            .is_some()
    );

    click(&mut harness, "Add layer");
    let layer = {
        let canvas = &harness.state().doc().unwrap().doc.sheet().canvas;
        assert_eq!(canvas.layers.len(), 1);
        assert_eq!(canvas.layers[0].name, "Layer 1");
        canvas.layers[0].id
    };

    click(&mut harness, "Move selection here: Layer 1");
    {
        let canvas = &harness.state().doc().unwrap().doc.sheet().canvas;
        assert_eq!(canvas.object(shape).unwrap().layer, Some(layer));
        assert_eq!(canvas.object(other).unwrap().layer, None);
    }

    click(&mut harness, "Hide layer: Layer 1");
    {
        let doc = harness.state().doc().unwrap();
        assert!(!doc.doc.sheet().canvas.is_layer_visible(Some(layer)));
        assert!(doc.view.board_selection.is_empty());
    }
    assert!(harness.query_by_label("Show layer: Layer 1").is_some());

    click(&mut harness, "Lock layer: Layer 1");
    let doc = harness.state().doc().unwrap();
    assert!(doc.doc.sheet().canvas.is_layer_locked(Some(layer)));
    assert!(
        doc.doc
            .sheet()
            .is_board_member_locked(thinoda_core::board::BoardMember::Object(shape))
    );
    assert!(
        harness
            .get_by_label("Delete layer: Layer 1")
            .accesskit_node()
            .is_disabled()
    );
}
