use super::*;
use egui::{Key, Role, accesskit::Toggled};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use thinoda_core::{Rect as BoardRect, Vec2, templates};

fn app_with_shape() -> (ThinodaApp, CanvasObjectId) {
    let mut app = ThinodaApp::headless();
    app.settings.ai.enabled = false;
    app.settings.show_format_panel = false;
    app.new_doc(templates::instantiate("blank-canvas"));
    let doc = app.doc_mut().unwrap();
    let mut object = CanvasObject::shape(
        ShapeKind::Rectangle,
        BoardRect::from_center_size(Vec2::new(0.0, 0.0), Vec2::new(160.0, 90.0)),
    );
    object.alt_text = "Keep shape description".into();
    object.style.opacity = 0.75;
    let id = doc.doc.add_canvas_object(object).unwrap();
    doc.view.select_board_object(id);
    (app, id)
}

fn harness(app: ThinodaApp) -> Harness<'static, ThinodaApp> {
    let mut harness = Harness::builder()
        .with_size(vec2(1000.0, 700.0))
        .with_step_dt(1.0 / 60.0)
        .with_context_setup(|ctx| {
            ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
        })
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    harness.run_steps(6);
    harness
}

fn object(app: &ThinodaApp, id: CanvasObjectId) -> CanvasObject {
    app.doc()
        .unwrap()
        .doc
        .sheet()
        .canvas
        .object(id)
        .unwrap()
        .clone()
}

fn shape_kind(app: &ThinodaApp, id: CanvasObjectId) -> ShapeKind {
    let CanvasObjectKind::Shape { shape, .. } = object(app, id).kind else {
        panic!("expected shape");
    };
    shape
}

fn click(h: &mut Harness<'_, ThinodaApp>, role: Role, label: &str) {
    h.get_by_role_and_label(role, label).scroll_to_me();
    h.run_steps(4);
    h.get_by_role_and_label(role, label).click();
    h.run_steps(4);
}

fn close_popup(h: &mut Harness<'_, ThinodaApp>) {
    h.key_press(Key::Escape);
    h.run_steps(3);
}

fn undo(h: &mut Harness<'_, ThinodaApp>) {
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    h.run_steps(2);
}

#[test]
fn selected_shape_controls_edit_shape_fill_and_outline_as_single_undo_steps() {
    for expanded in [false, true] {
        let (app, id) = app_with_shape();
        let mut h = harness(app);
        if expanded {
            click(&mut h, Role::Button, "Expand board toolbar");
        }
        assert!(h.query_by_label("Connector route").is_none());
        let original = object(h.state(), id);
        let initial = h.state().doc().unwrap().doc.state_id();

        click(&mut h, Role::Button, "Change shape");
        click(&mut h, Role::Button, "Diamond");
        assert_eq!(shape_kind(h.state(), id), ShapeKind::Diamond);
        close_popup(&mut h);

        click(&mut h, Role::Button, "Fill color");
        click(&mut h, Role::ColorWell, "#2E7CF6");
        assert_eq!(
            object(h.state(), id).style.fill,
            Some(Color::rgb(0x2e, 0x7c, 0xf6))
        );
        close_popup(&mut h);

        click(&mut h, Role::Button, "Outline color");
        click(&mut h, Role::ColorWell, "#2E7CF6");
        assert_eq!(
            object(h.state(), id).style.stroke,
            Some(Color::rgb(0x2e, 0x7c, 0xf6))
        );
        close_popup(&mut h);

        click(&mut h, Role::Button, "Outline width");
        click(&mut h, Role::Button, "Outline width: 8");
        assert_eq!(object(h.state(), id).style.stroke_width, 8.0);
        close_popup(&mut h);

        click(&mut h, Role::Button, "Fill color");
        click(&mut h, Role::Button, "Fill color · No fill");
        assert_eq!(object(h.state(), id).style.fill, None);
        close_popup(&mut h);

        let edited = object(h.state(), id);
        assert_eq!(edited.rect, original.rect);
        assert_eq!(edited.alt_text, original.alt_text);
        assert_eq!(edited.style.opacity, original.style.opacity);

        // Shape, fill, outline color, outline width and "No fill" are five undo steps.
        for _ in 0..5 {
            undo(&mut h);
        }
        assert_eq!(h.state().doc().unwrap().doc.state_id(), initial);
        assert_eq!(object(h.state(), id), original);
    }
}

#[test]
fn outline_width_pins_the_automatic_outline_color() {
    let (app, id) = app_with_shape();
    let auto = app.shape_context().unwrap().properties.auto_stroke;
    assert_eq!(object(&app, id).style.stroke, None);
    let mut h = harness(app);
    click(&mut h, Role::Button, "Outline width");
    click(&mut h, Role::Button, "Outline width: 8");
    let style = object(h.state(), id).style;
    assert_eq!(style.stroke, Some(auto));
    assert_eq!(style.stroke_width, 8.0);
    close_popup(&mut h);

    // An explicit outline color is never overwritten by a width change.
    click(&mut h, Role::Button, "Outline color");
    click(&mut h, Role::ColorWell, "#2E7CF6");
    close_popup(&mut h);
    click(&mut h, Role::Button, "Outline width");
    click(&mut h, Role::Button, "Outline width: 2");
    let style = object(h.state(), id).style;
    assert_eq!(style.stroke, Some(Color::rgb(0x2e, 0x7c, 0xf6)));
    assert_eq!(style.stroke_width, 2.0);
}

#[test]
fn outline_auto_restores_the_inherited_outline() {
    let (mut app, id) = app_with_shape();
    app.doc_mut()
        .unwrap()
        .doc
        .update_canvas_object("Outline", id, |object| {
            object.style.stroke = Some(Color::rgb(20, 40, 60));
        })
        .unwrap();
    let mut h = harness(app);
    click(&mut h, Role::Button, "Outline color");
    click(&mut h, Role::Button, "Outline color · Auto");
    assert_eq!(object(h.state(), id).style.stroke, None);
    undo(&mut h);
    assert_eq!(
        object(h.state(), id).style.stroke,
        Some(Color::rgb(20, 40, 60))
    );
}

#[test]
fn locked_grouped_and_mixed_shape_selections_cannot_be_edited() {
    for case in ["locked", "group", "mixed"] {
        let (mut app, id) = app_with_shape();
        let doc = app.doc_mut().unwrap();
        if case == "locked" {
            doc.doc.set_canvas_objects_locked(&[id], true).unwrap();
        } else {
            let other = doc
                .doc
                .add_canvas_object(CanvasObject::shape(
                    ShapeKind::Ellipse,
                    BoardRect::from_center_size(Vec2::new(300.0, 0.0), Vec2::new(60.0, 40.0)),
                ))
                .unwrap();
            if case == "mixed" {
                doc.view.board_selection.push(other);
            } else {
                doc.doc
                    .edit_sheet("Group", |sheet| {
                        sheet
                            .group_board_members(&[
                                BoardMember::Object(id),
                                BoardMember::Object(other),
                            ])
                            .map_err(thinoda_core::EditError::Invalid)?;
                        Ok(())
                    })
                    .unwrap();
            }
        }
        if case == "mixed" {
            assert!(app.shape_context().is_none());
            continue;
        }
        let context = app.shape_context().unwrap();
        assert!(!context.enabled, "{case}");
        let before = app.doc().unwrap().doc.workbook().clone();
        assert!(
            app.apply_shape_change(&context.target, ShapeChange::Fill(Some(Color::WHITE)))
                .is_err()
        );
        assert!(app.edit_board_label(&context.target).is_err());
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        let h = harness(app);
        for label in ["Change shape", "Fill color", "Edit label"] {
            let control = h.get_by_role_and_label(Role::Button, label);
            assert!(control.accesskit_node().is_disabled(), "{case}: {label}");
        }
    }
}

#[test]
fn stale_shape_targets_are_rejected() {
    let (mut app, id) = app_with_shape();
    let target = app.shape_context().unwrap().target;
    app.doc_mut()
        .unwrap()
        .doc
        .update_canvas_object("Move", id, |object| {
            object.rect = object.rect.translate(Vec2::new(10.0, 0.0));
        })
        .unwrap();
    let state = app.doc().unwrap().doc.state_id();
    assert!(
        app.apply_shape_change(&target, ShapeChange::Shape(ShapeKind::Star))
            .is_err()
    );
    assert!(app.edit_board_label(&target).is_err());
    assert_eq!(app.doc().unwrap().doc.state_id(), state);
    let fresh = app.shape_context().unwrap().target;
    assert!(
        app.apply_shape_change(&fresh, ShapeChange::Shape(ShapeKind::Star))
            .is_ok()
    );
    assert_eq!(shape_kind(&app, id), ShapeKind::Star);
}

#[test]
fn changing_selection_closes_open_shape_popups() {
    let (mut app, id) = app_with_shape();
    let other = app
        .doc_mut()
        .unwrap()
        .doc
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Ellipse,
            BoardRect::from_center_size(Vec2::new(300.0, 0.0), Vec2::new(60.0, 40.0)),
        ))
        .unwrap();
    let mut h = harness(app);
    for next in [other, id] {
        click(&mut h, Role::Button, "Fill color");
        assert!(h.query_by_label("Fill color · No fill").is_some());
        h.state_mut()
            .doc_mut()
            .unwrap()
            .view
            .select_board_object(next);
        h.run_steps(4);
        assert!(h.query_by_label("Fill color · No fill").is_none());
        assert!(h.query_by_label("Change shape").is_some());
    }
}

#[test]
fn edit_label_starts_inline_editing_and_format_shortcut_toggles_the_panel() {
    let (app, id) = app_with_shape();
    let mut h = harness(app);
    click(&mut h, Role::Button, "Edit label");
    assert_eq!(
        h.state().doc().unwrap().view.editing_board_object(),
        Some(id)
    );
    close_popup(&mut h);

    assert!(!h.state().settings.show_format_panel);
    click(&mut h, Role::Button, "More formatting options");
    assert!(h.state().settings.show_format_panel);
    assert_eq!(
        h.get_by_role_and_label(Role::Button, "More formatting options")
            .accesskit_node()
            .toggled(),
        Some(Toggled::True)
    );
    click(&mut h, Role::Button, "More formatting options");
    assert!(!h.state().settings.show_format_panel);
}

#[test]
fn connectors_get_the_same_label_and_format_shortcuts() {
    let (mut app, _) = app_with_shape();
    let doc = app.doc_mut().unwrap();
    let id = doc
        .doc
        .add_canvas_object(CanvasObject::connector(thinoda_core::CanvasConnector::new(
            thinoda_core::Endpoint::Point(Vec2::new(-120.0, 200.0)),
            thinoda_core::Endpoint::Point(Vec2::new(120.0, 240.0)),
        )))
        .unwrap();
    doc.view.select_board_object(id);
    let mut h = harness(app);
    assert!(h.query_by_label("Change shape").is_none());
    assert!(h.query_by_label("Connector route").is_some());
    click(&mut h, Role::Button, "Edit label");
    assert_eq!(
        h.state().doc().unwrap().view.editing_board_object(),
        Some(id)
    );
}
