use egui::{Key, Modifiers, accesskit::Role};
use egui_kittest::{Harness, kittest::Queryable};
use ymind_app::app::{Cmd, YMindApp};
use ymind_canvas::{Action, ObjectSelection};
use ymind_core::{Color, Vec2};
use ymind_io::{Format, export_bytes, import_bytes};

fn harness() -> Harness<'static, YMindApp> {
    let mut app = YMindApp::headless();
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
        .build_ui_state(|ui, app: &mut YMindApp| app.draw(ui), app);
    h.run_steps(4);
    h
}

fn replace_text(h: &mut Harness<'_, YMindApp>, label: &str, value: &str) {
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
    for format in [Format::YMind, Format::XMind] {
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
