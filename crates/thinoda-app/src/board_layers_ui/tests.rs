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
