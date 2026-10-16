use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use ymind_app::app::{Cmd, YMindApp};
use ymind_core::{custom_theme::CustomTheme, theme::resolve_style};
use ymind_io::{Format, export_bytes, import_bytes};

#[test]
fn theme_editor_stages_role_presets_and_applies_in_one_undoable_transaction() {
    let app = YMindApp::headless();
    let before = app.doc().unwrap().doc.workbook().clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 1000.0))
        .build_ui_state(|ui, app: &mut YMindApp| app.draw(ui), app);
    h.run_steps(3);
    h.get_by_role_and_label(Role::Tab, "Map").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Edit Custom Theme…")
        .click();
    h.run_steps(3);
    h.get_by_label("Topic role").scroll_to_me();
    h.run_steps(5);
    h.get_by_label("Topic role").click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Main Topic").click();
    h.run_steps(3);
    h.get_by_label("Role preset").scroll_to_me();
    h.run_steps(5);
    h.get_by_label("Role preset").click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Warning").click();
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    h.get_by_role_and_label(Role::Button, "Apply Theme").click();
    h.run_steps(3);
    let d = h.state_mut().doc_mut().unwrap();
    assert!(d.doc.sheet().theme.custom_theme.is_some());
    let root = d.doc.sheet().root;
    let main = d.doc.sheet().children(root)[0];
    assert_eq!(resolve_style(d.doc.sheet(), main, false).font_size, 16.0);
    assert!(resolve_style(d.doc.sheet(), main, false).bold);
    assert_eq!(
        d.doc.sheet().topic(main).unwrap().style,
        before.active().topic(main).unwrap().style
    );
    let applied = d.doc.workbook().clone();
    for format in [Format::YMind, Format::XMind] {
        let bytes = export_bytes(&applied, format, &Default::default()).unwrap();
        assert_eq!(import_bytes(format, &bytes).unwrap(), applied);
    }
    assert!(d.doc.undo());
    assert_eq!(d.doc.workbook(), &before);
    assert!(!d.doc.can_undo());
    assert!(d.doc.redo());
    assert_eq!(d.doc.workbook(), &applied);
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Classic")
        .scroll_to_me();
    h.run_steps(5);
    h.get_by_role_and_label(Role::Button, "Classic").click();
    h.run_steps(3);
    assert!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .theme
            .custom_theme
            .is_none()
    );
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &applied);
}

#[test]
fn saved_theme_library_does_not_dirty_maps_and_removal_does_not_modify_embedded_themes() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let paths = ymind_platform::Paths::from_root(dir.path());
    let mut app = YMindApp::headless();
    app.services.paths = Some(paths.clone());
    let before = app.doc().unwrap().doc.workbook().clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 1000.0))
        .build_ui_state(|ui, app: &mut YMindApp| app.draw(ui), app);
    h.run_steps(3);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::EditCustomTheme);
    h.run_steps(3);
    let footer = h
        .get_by_role_and_label(Role::Button, "Save to Library")
        .rect();
    for _ in 0..4 {
        h.run_steps(1);
        assert_eq!(
            h.get_by_role_and_label(Role::Button, "Save to Library")
                .rect(),
            footer
        );
    }
    h.get_by_role_and_label(Role::Button, "Save to Library")
        .click();
    h.run_steps(3);
    assert_eq!(h.state().settings.custom_themes.len(), 1);
    assert_eq!(
        ymind_platform::Settings::load(&paths)
            .unwrap()
            .custom_themes,
        h.state().settings.custom_themes
    );
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    h.get_by_role_and_label(Role::Button, "Save to Library")
        .click();
    h.run_steps(3);
    h.get_by_label("Could not save theme: custom theme names must be unique");
    h.get_by_role_and_label(Role::Button, "Apply Theme").click();
    h.run_steps(3);
    let applied = h.state().doc().unwrap().doc.workbook().clone();
    h.get_by_role_and_label(Role::Tab, "Map").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Remove…").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Cancel").click();
    h.run_steps(3);
    assert_eq!(h.state().settings.custom_themes.len(), 1);
    let blocked = dir.path().join("blocked");
    std::fs::write(&blocked, "not a directory").unwrap();
    h.state_mut().services.paths = Some(ymind_platform::Paths {
        config_dir: blocked,
        ..paths.clone()
    });
    h.get_by_role_and_label(Role::Button, "Remove…").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Remove").click();
    h.run_steps(3);
    assert_eq!(h.state().settings.custom_themes.len(), 1);
    assert_eq!(
        ymind_platform::Settings::load(&paths)
            .unwrap()
            .custom_themes
            .len(),
        1
    );
    assert!(
        h.query_all_by_label_contains("Could not save theme:")
            .next()
            .is_some()
    );
    h.state_mut().services.paths = Some(paths.clone());
    h.get_by_role_and_label(Role::Button, "Remove…").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Remove").click();
    h.run_steps(3);
    assert!(h.state().settings.custom_themes.is_empty());
    assert!(
        ymind_platform::Settings::load(&paths)
            .unwrap()
            .custom_themes
            .is_empty()
    );
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &applied);
}

#[test]
fn custom_theme_commands_preserve_topic_and_map_overrides_and_opt_in_recent_theme() {
    let mut app = YMindApp::headless();
    let ctx = egui::Context::default();
    let d = app.doc_mut().unwrap();
    let root = d.doc.sheet().root;
    let main = d.doc.sheet().children(root)[0];
    d.doc
        .edit_sheet("Overrides", |s| {
            s.topic_mut(main).unwrap().style.font_size = Some(40.0);
            s.theme.line_width = Some(2.0);
            Ok(())
        })
        .unwrap();
    let mut theme = CustomTheme::from_settings(&d.doc.sheet().theme);
    theme.roles.main.font_size = Some(28.0);
    theme.roles.main.line_width = Some(4.0);
    app.exec(&ctx, Cmd::ApplyCustomTheme(Box::new(theme.clone())));
    assert_eq!(
        resolve_style(app.doc().unwrap().doc.sheet(), main, false).font_size,
        40.0
    );
    assert_eq!(
        resolve_style(app.doc().unwrap().doc.sheet(), main, false).line_width,
        2.0
    );
    app.settings.reuse_last_theme = true;
    app.exec(&ctx, Cmd::New);
    assert_eq!(
        app.doc().unwrap().doc.sheet().theme.custom_theme,
        Some(theme.clone())
    );
    app.settings.reuse_last_theme = false;
    app.exec(&ctx, Cmd::New);
    assert!(app.doc().unwrap().doc.sheet().theme.custom_theme.is_none());
    let before = app.doc().unwrap().doc.workbook().clone();
    theme.roles.main.font_size = Some(f32::NAN);
    app.exec(&ctx, Cmd::ApplyCustomTheme(Box::new(theme)));
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
}

#[test]
fn theme_editor_cancels_drafts_and_stale_sources_without_modifying_maps() {
    let app = YMindApp::headless();
    let before = app.doc().unwrap().doc.workbook().clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(760.0, 680.0))
        .build_ui_state(|ui, app: &mut YMindApp| app.draw(ui), app);
    h.run_steps(3);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::EditCustomTheme);
    h.run_steps(3);
    assert!(
        h.ctx
            .content_rect()
            .contains_rect(h.get_by_role_and_label(Role::Button, "Apply Theme").rect())
    );
    h.key_press(egui::Key::Delete);
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    h.get_by_role_and_label(Role::Button, "Cancel").click();
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    h.state_mut().exec(&ctx, Cmd::EditCustomTheme);
    h.run_steps(3);
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(h.state().command_available(&Cmd::EditCustomTheme));
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    h.state_mut().exec(&ctx, Cmd::EditCustomTheme);
    h.run_steps(3);
    let d = h.state_mut().doc_mut().unwrap();
    d.doc
        .edit_sheet("External change", |s| s.set_title(s.root, "New title"))
        .unwrap();
    let changed = d.doc.workbook().clone();
    h.run_steps(3);
    h.get_by_label("Theme editing canceled because the document changed.");
    assert!(h.state().command_available(&Cmd::EditCustomTheme));
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &changed);
}

#[test]
fn role_font_changes_remeasure_actual_canvas_text_without_writing_topic_overrides() {
    let mut app = YMindApp::headless();
    let d = app.doc_mut().unwrap();
    let root = d.doc.sheet().root;
    d.doc
        .edit_sheet("Title", |s| s.set_title(root, "iiiiiiiiiiiiiiiiiiii"))
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 900.0))
        .build_ui_state(|ui, app: &mut YMindApp| app.draw(ui), app);
    h.run_steps(3);
    let initial = {
        let d = h.state_mut().doc_mut().unwrap();
        d.view.layout_for(&d.doc).topics[&root].title_rect.size.x
    };
    let mut theme = CustomTheme::from_settings(&h.state().doc().unwrap().doc.sheet().theme);
    theme.roles.central.font_family = Some("monospace".into());
    let ctx = h.ctx.clone();
    h.state_mut()
        .exec(&ctx, Cmd::ApplyCustomTheme(Box::new(theme)));
    h.run_steps(3);
    let d = h.state_mut().doc_mut().unwrap();
    let mono = d.view.layout_for(&d.doc).topics[&root].title_rect.size.x;
    assert!(mono > initial * 1.5, "{initial} -> {mono}");
    assert!(
        d.doc
            .sheet()
            .topic(root)
            .unwrap()
            .style
            .font_family
            .is_none()
    );
    assert!(d.doc.undo());
    h.run_steps(3);
    let d = h.state_mut().doc_mut().unwrap();
    assert_eq!(
        d.view.layout_for(&d.doc).topics[&root].title_rect.size.x,
        initial
    );
}

#[test]
fn failed_theme_save_and_empty_names_remain_visible_without_changing_maps() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let paths = ymind_platform::Paths::from_root(dir.path());
    std::fs::write(&paths.config_dir, "not a directory").unwrap();
    let mut app = YMindApp::headless();
    app.services.paths = Some(paths);
    let before = app.doc().unwrap().doc.workbook().clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 1000.0))
        .build_ui_state(|ui, app: &mut YMindApp| app.draw(ui), app);
    h.run_steps(3);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::EditCustomTheme);
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Save to Library")
        .click();
    h.run_steps(3);
    assert!(
        h.query_all_by_label_contains("Could not save theme:")
            .next()
            .is_some()
    );
    assert!(h.state().settings.custom_themes.is_empty());
    h.get_by_label("Theme name").click();
    h.run_steps(1);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(1);
    h.key_press(egui::Key::Backspace);
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Apply Theme").click();
    h.run_steps(3);
    h.get_by_label("custom theme name must contain 1-80 printable characters");
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    h.get_by_label("Base theme").click();
    h.run_steps(3);
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Cancel").click();
    h.run_steps(3);
    assert!(h.state().command_available(&Cmd::EditCustomTheme));
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
}
