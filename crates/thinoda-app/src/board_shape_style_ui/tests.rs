use egui::{Ui, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_core::{
    CanvasObjectId, Endpoint, Rect, ShapeKind, Vec2, board::CanvasConnector, templates,
};

use super::*;

fn app_with_shapes() -> (ThinodaApp, Vec<CanvasObjectId>) {
    let mut app = ThinodaApp::headless();
    app.new_doc(templates::instantiate("blank-canvas"));
    let doc = app.doc_mut().unwrap();
    let ids: Vec<_> = [0.0, 200.0]
        .into_iter()
        .map(|x| {
            doc.doc
                .add_canvas_object(CanvasObject::shape(
                    ShapeKind::Rectangle,
                    Rect::from_min_size(Vec2::new(x, 0.0), Vec2::new(120.0, 80.0)),
                ))
                .unwrap()
        })
        .collect();
    doc.view.selection.clear();
    doc.view.board_selection = ids.clone();
    (app, ids)
}

fn style(app: &ThinodaApp, id: CanvasObjectId) -> ObjectStyle {
    app.doc()
        .unwrap()
        .doc
        .sheet()
        .canvas
        .object(id)
        .unwrap()
        .style
}

fn inspector(app: ThinodaApp) -> Harness<'static, ThinodaApp> {
    let mut harness = Harness::builder()
        .with_size(vec2(450.0, 1400.0))
        .build_ui_state(
            |ui: &mut Ui, app: &mut ThinodaApp| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    app.board_object_properties(ui);
                });
            },
            app,
        );
    harness.run_steps(3);
    harness
}

fn click(harness: &mut Harness<'_, ThinodaApp>, label: &str) {
    harness.get_by_label(label).click();
    harness.run_steps(3);
}

#[test]
fn shape_style_section_edits_every_selected_shape_in_one_undo_step() {
    let (app, ids) = app_with_shapes();
    let mut harness = inspector(app);
    assert!(
        harness
            .query_by_label("Select one board object to edit its properties.")
            .is_none()
    );
    for label in [
        "Shadow",
        "Flip horizontally",
        "Dashed outline",
        "Fixed dash length",
        "Gradient",
    ] {
        click(&mut harness, label);
    }
    for id in &ids {
        let style = style(harness.state(), *id);
        assert!(style.shadow && style.flip_h && !style.flip_v);
        assert_eq!(
            style.dash,
            Some(ShapeDash {
                pattern: DashPattern::default(),
                fixed: true
            })
        );
        assert_eq!(style.gradient.unwrap().color, DEFAULT_GRADIENT);
    }
    let app = harness.state_mut();
    assert_eq!(app.doc().unwrap().doc.undo_label(), Some("Shape style"));
    assert!(app.doc_mut().unwrap().doc.undo());
    for id in &ids {
        let style = style(app, *id);
        assert!(style.gradient.is_none() && style.dash.is_some());
    }
    for _ in 0..4 {
        assert!(app.doc_mut().unwrap().doc.undo());
    }
    for id in &ids {
        assert!(!style(app, *id).has_shape_style_effects());
    }
}

#[test]
fn shape_style_section_follows_single_and_mixed_selection() {
    let (mut app, ids) = app_with_shapes();
    app.doc_mut().unwrap().view.board_selection = vec![ids[1]];
    let mut harness = inspector(app);
    click(&mut harness, "Glass");
    assert!(style(harness.state(), ids[1]).glass);
    assert!(!style(harness.state(), ids[0]).glass);

    let (mut app, ids) = app_with_shapes();
    let doc = app.doc_mut().unwrap();
    let connector = doc
        .doc
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(ids[0]),
            Endpoint::Object(ids[1]),
        )))
        .unwrap();
    doc.view.board_selection.push(connector);
    let harness = inspector(app);
    assert!(harness.query_by_label("Shadow").is_none());
    assert!(
        harness
            .query_by_label("Select one board object to edit its properties.")
            .is_some()
    );
}
