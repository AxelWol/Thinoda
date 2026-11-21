use egui::{Ui, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_core::{
    CanvasObjectId, Endpoint, Rect, ShapeKind, Vec2, board::CanvasConnector, templates,
};

use super::*;

#[test]
fn whole_label_change_clears_only_the_conflicting_run_property() {
    let mut object = CanvasObject::shape(
        ShapeKind::Rectangle,
        Rect::from_min_size(Vec2::ZERO, Vec2::new(120.0, 80.0)),
    );
    object.set_label("Styled").unwrap();
    let mut rich = thinoda_core::InlineText::default();
    rich.update_range("Styled", 0..6, |style| {
        style.bold = Some(true);
        style.italic = Some(true);
        style.color = Some(Color::rgb(200, 30, 50));
    })
    .unwrap();
    object.label_rich_text = Some(Arc::new(rich));
    LabelStyleChange::Bold(false).apply_to(&mut object);
    let run = &object.label_rich_text.as_ref().unwrap().runs[0];
    assert_eq!(run.style.bold, None);
    assert_eq!(run.style.italic, Some(true));
    assert_eq!(run.style.color, Some(Color::rgb(200, 30, 50)));
    LabelStyleChange::Color(None).apply_to(&mut object);
    let run = &object.label_rich_text.as_ref().unwrap().runs[0];
    assert_eq!(run.style.color, None);
    assert_eq!(run.style.italic, Some(true));
}

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

fn label_style(app: &ThinodaApp, id: CanvasObjectId) -> Option<Arc<LabelStyle>> {
    app.doc()
        .unwrap()
        .doc
        .sheet()
        .canvas
        .object(id)
        .unwrap()
        .label_style
        .clone()
}

fn inspector(app: ThinodaApp) -> Harness<'static, ThinodaApp> {
    let mut harness = Harness::builder()
        .with_size(vec2(450.0, 2400.0))
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
fn label_text_section_edits_every_selected_shape_in_one_undo_step() {
    let (app, ids) = app_with_shapes();
    let mut harness = inspector(app);
    for label in ["Bold", "Underline", "Wrap text", "Label background"] {
        click(&mut harness, label);
    }
    for id in &ids {
        let style = label_style(harness.state(), *id).unwrap();
        assert!(style.bold && style.underline && !style.italic && !style.wrap);
        assert_eq!(style.background, Some(DEFAULT_BACKGROUND));
    }
    let app = harness.state_mut();
    assert_eq!(app.doc().unwrap().doc.undo_label(), Some("Label style"));
    assert!(app.doc_mut().unwrap().doc.undo());
    for id in &ids {
        let style = label_style(app, *id).unwrap();
        assert!(style.background.is_none() && !style.wrap);
    }
    for _ in 0..3 {
        assert!(app.doc_mut().unwrap().doc.undo());
    }
    for id in &ids {
        assert!(label_style(app, *id).is_none());
    }
}

#[test]
fn label_text_changes_keep_other_fields_and_reset_to_no_style() {
    let (mut app, ids) = app_with_shapes();
    let doc = app.doc_mut().unwrap();
    doc.doc
        .update_canvas_objects("seed", &ids[..1], |object| {
            object.label_style = Some(Arc::new(LabelStyle {
                italic: true,
                ..LabelStyle::default()
            }))
        })
        .unwrap();
    let mut harness = inspector(app);
    // A mixed toggle first applies to all, then clears the property on all.
    click(&mut harness, "Italic");
    for id in &ids {
        assert!(label_style(harness.state(), *id).unwrap().italic);
    }
    click(&mut harness, "Italic");
    assert!(label_style(harness.state(), ids[0]).is_none());
    assert!(label_style(harness.state(), ids[1]).is_none());
    click(&mut harness, "Vertical text");
    for id in &ids {
        let style = label_style(harness.state(), *id).unwrap();
        assert!(style.vertical && !style.italic);
    }

    let mut object = CanvasObject::shape(
        ShapeKind::Rectangle,
        Rect::from_min_size(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0)),
    );
    object.label_style = Some(Arc::new(LabelStyle {
        bold: true,
        ..LabelStyle::default()
    }));
    LabelStyleChange::Spacing(Side::Left, 4.0).apply_to(&mut object);
    LabelStyleChange::Position(TextAlign::Right).apply_to(&mut object);
    let style = object.label_style.as_deref().unwrap();
    assert!(style.bold && style.spacing_left == 4.0 && style.position == TextAlign::Right);
}

#[test]
fn label_text_section_is_hidden_for_mixed_selection() {
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
    assert!(harness.query_by_label("Wrap text").is_none());
}

#[test]
fn style_clipboard_copies_and_clears_label_text_style() {
    let (app, ids) = app_with_shapes();
    let doc = &app.doc().unwrap().doc;
    let mut styled = doc.sheet().canvas.object(ids[0]).unwrap().clone();
    let mut plain = doc.sheet().canvas.object(ids[1]).unwrap().clone();
    styled.label_style = Some(Arc::new(LabelStyle {
        font_family: Some("Courier New".into()),
        vertical_position: LabelVAlign::Bottom,
        ..LabelStyle::default()
    }));
    let copied = crate::board_ui::BoardStyleClipboard::from_object(&styled);
    copied.apply(&mut plain);
    assert_eq!(plain.label_style, styled.label_style);
    crate::board_ui::BoardStyleClipboard::from_object(doc.sheet().canvas.object(ids[1]).unwrap())
        .apply(&mut styled);
    assert!(styled.label_style.is_none());
}
