use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::app::{CenterMode, Cmd, Services, ThinodaApp};
use thinoda_core::{TopicStyle, quick_styles::preset};
use thinoda_io::{Format, export_bytes, import_bytes};

#[test]
fn quick_style_gallery_saves_captured_appearance_and_applies_with_one_undo_step() {
    let app = ThinodaApp::headless();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 900.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Quick Style: Warning")
        .click();
    h.run_steps(3);
    let warning = preset("Warning").unwrap().style;
    let d = h.state().doc().unwrap();
    assert_eq!(
        d.doc
            .sheet()
            .topic(d.view.primary().unwrap())
            .unwrap()
            .style,
        warning
    );
    let before_save = d.doc.workbook().clone();
    h.get_by_role_and_label(Role::Button, "Save Quick Style…")
        .click();
    h.run_steps(3);
    h.get_by_label("Style name").click();
    h.run_steps(1);
    h.get_by_label("Style name").type_text("  Review  ");
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Save").click();
    h.run_steps(3);
    assert_eq!(h.state().settings.quick_styles.len(), 1);
    assert_eq!(h.state().settings.quick_styles[0].name, "Review");
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before_save);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::New);
    let d = h.state_mut().doc_mut().unwrap();
    let root = d.doc.sheet().root;
    let child = d.doc.sheet().children(root)[0];
    d.view.selection = vec![root, child];
    let before = d.doc.workbook().clone();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Quick Style: Review")
        .click();
    h.run_steps(3);
    let saved = h.state().settings.quick_styles[0].style.clone();
    let d = h.state_mut().doc_mut().unwrap();
    for id in [root, child] {
        assert_eq!(d.doc.sheet().topic(id).unwrap().style, saved);
    }
    let applied = d.doc.workbook().clone();
    for format in [Format::Thinoda, Format::XMind] {
        let bytes = export_bytes(&applied, format, &Default::default()).unwrap();
        assert_eq!(import_bytes(format, &bytes).unwrap(), applied);
    }
    assert!(d.doc.undo());
    assert_eq!(d.doc.workbook(), &before);
    assert!(!d.doc.can_undo());
    assert!(d.doc.redo());
    assert_eq!(d.doc.workbook(), &applied);
}

#[test]
fn quick_style_cancel_duplicates_invalid_names_and_enter_never_edit_the_map() {
    let app = ThinodaApp::headless();
    let before = app.doc().unwrap().doc.workbook().clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 900.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Save Quick Style…")
        .click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Save").click();
    h.run_steps(3);
    h.get_by_label(
        "Could not save quick style: quick style name must contain 1-80 printable characters",
    );
    h.get_by_label("Style name").click();
    h.get_by_label("Style name").type_text("Review");
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(h.state().settings.quick_styles.len(), 1);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    h.get_by_role_and_label(Role::Button, "Save Quick Style…")
        .click();
    h.run_steps(3);
    h.get_by_label("Style name").click();
    h.run_steps(1);
    h.get_by_label("Style name").type_text("review");
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Save").click();
    h.run_steps(3);
    h.get_by_label("Could not save quick style: quick style names must be unique");
    h.get_by_role_and_label(Role::Button, "Cancel").click();
    h.run_steps(3);
    assert!(h.state().command_available(&Cmd::SaveQuickStyle));
    assert_eq!(h.state().settings.quick_styles.len(), 1);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    assert!(!h.state().doc().unwrap().doc.is_dirty());
    h.get_by_role_and_label(Role::Button, "Save Quick Style…")
        .click();
    h.run_steps(3);
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(h.state().command_available(&Cmd::SaveQuickStyle));
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
}

#[test]
fn quick_style_commands_reject_stale_empty_and_structural_selections() {
    let mut app = ThinodaApp::headless();
    let ctx = egui::Context::default();
    let before = app.doc().unwrap().doc.workbook().clone();
    app.doc_mut().unwrap().view.selection.clear();
    for cmd in [
        Cmd::SaveQuickStyle,
        Cmd::ApplyQuickStyle(preset("Emphasis").unwrap().style),
    ] {
        assert!(!app.command_available(&cmd));
        app.exec(&ctx, cmd);
    }
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    let d = app.doc_mut().unwrap();
    let root = d.doc.sheet().root;
    let relationship = d
        .doc
        .edit_sheet("Link", |s| s.add_relationship(root, s.children(root)[0]))
        .unwrap();
    d.view
        .select_object(thinoda_canvas::ObjectSelection::Relationship(relationship));
    assert!(!app.command_available(&Cmd::SaveQuickStyle));
    assert!(!app.command_available(&Cmd::ApplyQuickStyle(TopicStyle::default())));
    let d = app.doc_mut().unwrap();
    d.view.selected_object = None;
    d.view.selection = vec![root, thinoda_core::TopicId(u64::MAX)];
    let before = d.doc.workbook().clone();
    app.exec(&ctx, Cmd::ApplyQuickStyle(preset("Note").unwrap().style));
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
}

#[test]
fn named_library_persists_across_relaunch_and_removal_requires_confirmation() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let paths = thinoda_platform::Paths::from_root(dir.path());
    let mut app = ThinodaApp::new(
        thinoda_platform::Settings {
            language: thinoda_platform::settings::Language::English,
            ..Default::default()
        },
        Services {
            paths: Some(paths.clone()),
            ..Default::default()
        },
        &[],
    );
    app.exec(&egui::Context::default(), Cmd::New);
    let before = app.doc().unwrap().doc.workbook().clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 900.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Save Quick Style…")
        .click();
    h.run_steps(3);
    h.get_by_label("Style name").click();
    h.run_steps(1);
    h.get_by_label("Style name").type_text("Reusable");
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Save").click();
    h.run_steps(3);
    let settings = thinoda_platform::Settings::load(&paths).unwrap();
    assert_eq!(settings.quick_styles, h.state().settings.quick_styles);
    let mut restarted = ThinodaApp::new(
        settings,
        Services {
            paths: Some(paths.clone()),
            ..Default::default()
        },
        &[],
    );
    restarted.exec(&h.ctx, Cmd::New);
    *h.state_mut() = restarted;
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Quick Style: Reusable");
    h.get_by_role_and_label(Role::Button, "Remove…").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Cancel").click();
    h.run_steps(3);
    assert_eq!(h.state().settings.quick_styles.len(), 1);
    let blocked = dir.path().join("blocked");
    std::fs::write(&blocked, "not a directory").unwrap();
    h.state_mut().services.paths = Some(thinoda_platform::Paths {
        config_dir: blocked,
        ..paths.clone()
    });
    h.get_by_role_and_label(Role::Button, "Remove…").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Remove").click();
    h.run_steps(3);
    assert_eq!(h.state().settings.quick_styles.len(), 1);
    assert_eq!(
        thinoda_platform::Settings::load(&paths)
            .unwrap()
            .quick_styles
            .len(),
        1
    );
    assert!(
        h.query_all_by_label_contains("Could not save quick style:")
            .next()
            .is_some()
    );
    h.state_mut().services.paths = Some(paths.clone());
    h.get_by_role_and_label(Role::Button, "Remove…").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Remove").click();
    h.run_steps(3);
    assert!(h.state().settings.quick_styles.is_empty());
    assert!(
        thinoda_platform::Settings::load(&paths)
            .unwrap()
            .quick_styles
            .is_empty()
    );
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    assert!(!h.state().doc().unwrap().doc.is_dirty());
}

#[test]
fn persistence_failure_keeps_the_library_and_document_unchanged_with_a_visible_error() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let paths = thinoda_platform::Paths::from_root(dir.path());
    std::fs::write(&paths.config_dir, "not a directory").unwrap();
    let mut app = ThinodaApp::headless();
    app.services.paths = Some(paths);
    let before = app.doc().unwrap().doc.workbook().clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 900.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Save Quick Style…")
        .click();
    h.run_steps(3);
    h.get_by_label("Style name").click();
    h.run_steps(1);
    h.get_by_label("Style name").type_text("Unsaved");
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Save").click();
    h.run_steps(3);
    assert!(h.state().settings.quick_styles.is_empty());
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    assert!(
        h.query_all_by_label_contains("Could not save quick style:")
            .next()
            .is_some()
    );
    h.get_by_role_and_label(Role::Button, "Cancel").click();
    h.run_steps(2);
    assert!(h.state().command_available(&Cmd::SaveQuickStyle));
}

#[test]
fn gallery_keyboard_activation_is_consumed_instead_of_inserting_topics() {
    for key in [egui::Key::Enter, egui::Key::Space] {
        let app = ThinodaApp::headless();
        let before = app.doc().unwrap().doc.workbook().clone();
        let mut h = Harness::builder()
            .with_size(egui::vec2(1280.0, 900.0))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        h.run_steps(3);
        h.get_by_role_and_label(Role::Button, "Quick Style: Note")
            .focus();
        h.run_steps(2);
        h.key_press(key);
        h.run_steps(3);
        let d = h.state_mut().doc_mut().unwrap();
        assert_eq!(
            d.doc.sheet().topic(d.doc.sheet().root).unwrap().style,
            preset("Note").unwrap().style
        );
        assert!(d.doc.undo());
        assert_eq!(d.doc.workbook(), &before);
        assert!(!d.doc.can_undo());
    }
}
#[test]
fn outliner_shortcuts_cannot_edit_the_map_behind_a_style_modal() {
    let mut app = ThinodaApp::headless();
    app.mode = CenterMode::Outliner;
    let before = app.doc().unwrap().doc.workbook().clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 900.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Save Quick Style…")
        .click();
    h.run_steps(3);
    h.get_by_label("Style name").click();
    h.run_steps(1);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    h.get_by_label(
        "Could not save quick style: quick style name must contain 1-80 printable characters",
    );
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(h.state().command_available(&Cmd::SaveQuickStyle));
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
}
