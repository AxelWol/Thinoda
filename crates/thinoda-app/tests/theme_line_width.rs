use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::app::ThinodaApp;
use thinoda_io::{Format, export_bytes, import_bytes};

#[test]
fn map_width_control_is_undoable_and_automatic_restores_theme_defaults() {
    let app = ThinodaApp::headless();
    let before = app.doc().unwrap().doc.workbook().clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 900.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(3);
    h.get_by_role_and_label(Role::Tab, "Map").click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Switch, "Automatic branch width")
        .scroll_to_me();
    h.run_steps(6);
    h.get_by_role_and_label(Role::Switch, "Automatic branch width")
        .click();
    h.run_steps(3);
    assert_eq!(
        h.state().doc().unwrap().doc.sheet().theme.line_width,
        Some(1.5)
    );
    let manual = h.state().doc().unwrap().doc.workbook().clone();
    for format in [Format::Thinoda, Format::XMind] {
        let bytes = export_bytes(&manual, format, &Default::default()).unwrap();
        assert_eq!(import_bytes(format, &bytes).unwrap(), manual);
    }
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    assert!(h.state_mut().doc_mut().unwrap().doc.redo());
    h.run_steps(3);
    h.get_by_label("Branch Line Width").click();
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(1);
    h.get_by_label("Branch Line Width").type_text("3.5");
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(
        h.state().doc().unwrap().doc.sheet().theme.line_width,
        Some(3.5)
    );
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    assert_eq!(
        h.state().doc().unwrap().doc.sheet().theme.line_width,
        Some(1.5)
    );
    assert!(h.state_mut().doc_mut().unwrap().doc.redo());
    h.run_steps(3);
    h.get_by_role_and_label(Role::Switch, "Automatic branch width")
        .click();
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
}

#[test]
fn invalid_numeric_drafts_are_visible_errors_without_dirtying_or_partial_edits() {
    let mut app = ThinodaApp::headless();
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Width", |sheet| {
            sheet.theme.line_width = Some(3.5);
            Ok(())
        })
        .unwrap();
    app.doc_mut().unwrap().doc.mark_saved();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 900.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(3);
    h.get_by_role_and_label(Role::Tab, "Map").click();
    h.run_steps(2);
    h.get_by_label("Branch Line Width").scroll_to_me();
    h.run_steps(6);
    let before = h.state().doc().unwrap().doc.workbook().clone();
    for value in ["17", "NaN", "abc", "0"] {
        h.get_by_label("Branch Line Width").click();
        h.run_steps(1);
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        h.run_steps(1);
        h.get_by_label("Branch Line Width").type_text(value);
        h.run_steps(2);
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        h.key_press(egui::Key::Enter);
        h.run_steps(3);
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        assert!(!h.state().doc().unwrap().doc.is_dirty());
    }
    assert!(
        h.query_all_by_label("theme branch line width must be between 0.5 and 16")
            .next()
            .is_some()
    );
    assert!(
        h.query_by_label("Invalid branch width: invalid float literal")
            .is_some()
    );
    h.get_by_label("Branch Line Width").click();
    h.run_steps(1);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(1);
    h.get_by_label("Branch Line Width").type_text("7");
    h.run_steps(2);
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    assert!(!h.state().doc().unwrap().doc.is_dirty());
}
