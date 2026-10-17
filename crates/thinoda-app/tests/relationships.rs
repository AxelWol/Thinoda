use egui::{Key, Modifiers, accesskit::Role};
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::app::{Cmd, ThinodaApp};
use thinoda_canvas::{Action, ObjectSelection};
use thinoda_core::{Color, Vec2};
use thinoda_io::{Format, export_bytes, import_bytes};

fn harness() -> Harness<'static, ThinodaApp> {
    let mut app = ThinodaApp::headless();
    let d = app.doc_mut().unwrap();
    let root = d.doc.sheet().root;
    let id = d
        .doc
        .edit_sheet("Fixture", |s| {
            let end = s.add_floating("Destination", Vec2::new(300.0, -130.0));
            s.add_floating("Alternative", Vec2::new(200.0, 150.0));
            let id = s.add_relationship(root, end)?;
            s.set_relationship_color(id, Some(Color::rgb(9, 18, 27)))?;
            s.set_relationship_control_points(
                id,
                Some([Vec2::new(80.0, -50.0), Vec2::new(-80.0, -50.0)]),
            )?;
            Ok(id)
        })
        .unwrap();
    d.view.select_object(ObjectSelection::Relationship(id));
    d.doc.mark_saved();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 900.0))
        .with_context_setup(|ctx| {
            ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu)
        })
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(4);
    h
}

fn replace_text(h: &mut Harness<'_, ThinodaApp>, label: &str, value: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(5);
    h.get_by_label(label).click();
    h.run_steps(1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(1);
    h.get_by_label(label).type_text(value);
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
}

#[test]
fn relationship_sidebar_edits_title_controls_endpoints_and_reset_with_undo() {
    let mut h = harness();
    let before = h.state().doc().unwrap().doc.workbook().clone();
    replace_text(&mut h, "Relationship title", "Association");
    assert_eq!(
        h.state().doc().unwrap().doc.sheet().relationships[0].title,
        "Association"
    );
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::Canvas(Action::Undo));
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    h.run_steps(2);
    replace_text(&mut h, "First control X", "123.5");
    assert_eq!(
        h.state().doc().unwrap().doc.sheet().relationships[0]
            .control_points
            .unwrap()[0]
            .x,
        123.5
    );
    h.state_mut().exec(&ctx, Cmd::Canvas(Action::Undo));
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    h.run_steps(2);
    let target = h
        .state()
        .doc()
        .unwrap()
        .doc
        .sheet()
        .topics
        .values()
        .find(|t| t.title == "Alternative")
        .unwrap()
        .id;
    h.get_by_label("From topic").scroll_to_me();
    h.run_steps(5);
    h.get_by_label("From topic").click();
    h.run_steps(2);
    h.get_by_label(&format!("Alternative ({target})")).click();
    h.run_steps(3);
    assert_eq!(
        h.state().doc().unwrap().doc.sheet().relationships[0].from,
        target
    );
    h.state_mut().exec(&ctx, Cmd::Canvas(Action::Undo));
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Reset curve")
        .scroll_to_me();
    h.run_steps(5);
    h.get_by_role_and_label(Role::Button, "Reset curve").click();
    h.run_steps(3);
    assert!(
        h.state().doc().unwrap().doc.sheet().relationships[0]
            .control_points
            .is_none()
    );
    h.state_mut().exec(&ctx, Cmd::Canvas(Action::Undo));
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    for format in [Format::Thinoda, Format::XMind] {
        let bytes = export_bytes(&before, format, &Default::default()).unwrap();
        assert_eq!(import_bytes(format, &bytes).unwrap(), before);
    }
}

#[test]
fn invalid_curve_text_non_finite_and_out_of_range_values_surface_errors_without_edits() {
    let mut h = harness();
    let before = h.state().doc().unwrap().doc.workbook().clone();
    let state = h.state().doc().unwrap().doc.state_id();
    for value in ["not a number", "NaN", "inf", "1000001"] {
        replace_text(&mut h, "First control X", value);
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        assert_eq!(h.state().doc().unwrap().doc.state_id(), state);
        if let Err(e) = value.parse::<f32>() {
            h.get_by_label(&format!("Invalid curve coordinate: {e}"));
        } else {
            assert!(h.query_all_by_label(
                "Not possible here: curve controls must be finite and within +/- 1000000 map units",
            ).next().is_some());
        }
    }
}

#[test]
fn relationship_command_connects_two_selected_topics_without_target_picking() {
    let mut h = harness();
    let d = h.state_mut().doc_mut().unwrap();
    let source = d.doc.sheet().root;
    let target = d
        .doc
        .sheet()
        .topics
        .values()
        .find(|t| t.title == "Alternative")
        .unwrap()
        .id;
    d.view.selected_object = None;
    d.view.selection = vec![source, target];
    let before = d.doc.workbook().clone();
    let ctx = h.ctx.clone();
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(Action::StartRelationship));
    h.run_steps(2);
    let d = h.state().doc().unwrap();
    assert_eq!(
        d.doc.sheet().relationships.len(),
        before.sheets[0].relationships.len() + 1
    );
    let relationship = d.doc.sheet().relationships.last().unwrap();
    assert_eq!((relationship.from, relationship.to), (source, target));
    assert!(d.view.relationship_from.is_none());
    assert_eq!(
        d.view.selected_object,
        Some(ObjectSelection::Relationship(relationship.id))
    );
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
}

#[test]
fn relationship_toolbar_creates_connected_floating_targets_on_blank_canvas() {
    let mut h = harness();
    let source = h.state().doc().unwrap().doc.sheet().root;
    h.state_mut().doc_mut().unwrap().view.select_only(source);
    h.run_steps(3);
    let before = h.state().doc().unwrap().doc.workbook().clone();
    h.get_by_label("Connect topics").click();
    h.run_steps(2);
    assert_eq!(
        h.state().doc().unwrap().view.relationship_from,
        Some(source)
    );
    let point = h.state().doc().unwrap().view.canvas_rect().left_top() + egui::vec2(120.0, 100.0);
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
    let d = h.state().doc().unwrap();
    assert_eq!(
        d.doc.sheet().floating.len(),
        before.sheets[0].floating.len() + 1
    );
    assert_eq!(
        d.doc.sheet().relationships.len(),
        before.sheets[0].relationships.len() + 1
    );
    let relationship = d.doc.sheet().relationships.last().unwrap();
    assert_eq!(relationship.from, source);
    assert_eq!(d.view.editing_topic(), Some(relationship.to));
    assert!(d.doc.sheet().is_floating(relationship.to));
    assert!(d.view.relationship_from.is_none());
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
}
