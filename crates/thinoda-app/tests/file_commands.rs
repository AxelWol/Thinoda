//! Keep command and locale checks sequential: translation selection is process-wide.

use egui::accesskit::Role;
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use thinoda_app::{
    app::{Cmd, EXPORT_MENU, Services, ThinodaApp},
    doc::OpenDoc,
    i18n::t,
};
use thinoda_platform::settings::{Language, Settings, ThemeMode};

fn active_file_commands() -> Vec<Cmd> {
    [Cmd::Save, Cmd::SaveAs]
        .into_iter()
        .chain(EXPORT_MENU.iter().copied().map(Cmd::Export))
        .collect()
}

fn assert_rejected(app: &mut ThinodaApp, command: Cmd) {
    assert!(!app.command_available(&command));
    let before: Vec<_> = app
        .docs
        .iter()
        .map(|doc| {
            (
                doc.doc.workbook().clone(),
                doc.doc.state_id(),
                doc.path.clone(),
            )
        })
        .collect();
    let active = app.active;
    app.toasts.tick(60.0);
    app.exec(&egui::Context::default(), command);
    assert_eq!(app.active, active);
    assert_eq!(
        app.docs
            .iter()
            .map(|doc| (
                doc.doc.workbook().clone(),
                doc.doc.state_id(),
                doc.path.clone()
            ))
            .collect::<Vec<_>>(),
        before
    );
    assert!(!app.dialogs.mindmanager_export);
    assert!(app.dialogs.mindmanager_save.is_none());
    assert!(app.dialogs.symbol_export.is_none());
    assert!(app.dialogs.conflict.is_none());
    assert_eq!(
        app.toasts.texts().collect::<Vec<_>>(),
        [t("Not possible here")]
    );
}

fn exec(h: &mut Harness<'_, ThinodaApp>, command: Cmd) {
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, command);
    h.run_steps(4);
}

fn assert_palette(h: &mut Harness<'_, ThinodaApp>, command: Cmd, expected: bool) {
    let (_, entry) = ThinodaApp::palette_commands()
        .into_iter()
        .find(|(candidate, _)| candidate == &command)
        .unwrap();
    exec(h, Cmd::Palette);
    h.state_mut().palette.set_query(&entry.title);
    h.run_steps(4);
    let label = format!(
        "{} {}",
        entry.title,
        entry.shortcut.as_deref().unwrap_or_default()
    );
    assert_eq!(
        h.query_by_role_and_label(Role::Button, &label).is_some(),
        expected,
        "palette: {command:?}"
    );
    h.key_press(egui::Key::Escape);
    h.run_steps(4);
    assert!(!h.state().palette.open);
}

#[test]
fn file_commands_require_active_documents_and_save_all_preserves_home() {
    let ctx = egui::Context::default();
    let mut app = ThinodaApp::headless();
    app.docs.clear();
    for command in active_file_commands().into_iter().chain([Cmd::SaveAll]) {
        assert_rejected(&mut app, command);
    }

    let dir = tempfile::tempdir().unwrap();
    let mut app = ThinodaApp::headless();
    for index in 0..2 {
        if index > 0 {
            app.exec(&ctx, Cmd::New);
        }
        app.doc_mut()
            .unwrap()
            .save_to(&dir.path().join(format!("map-{index}.thinoda")), false)
            .unwrap();
    }
    app.exec(&ctx, Cmd::Home);
    let home = app.active;
    assert_eq!(home, app.docs.len());
    assert!(app.command_available(&Cmd::SaveAll));
    for command in active_file_commands() {
        assert_rejected(&mut app, command);
    }
    app.exec(&ctx, Cmd::SaveAll);
    assert_eq!(app.active, home, "even a no-op Save All must preserve Home");
    for (index, doc) in app.docs.iter_mut().enumerate() {
        let root = doc.doc.sheet().root;
        doc.doc
            .edit_sheet("Rename", |sheet| {
                sheet.set_title(root, &format!("Saved from Home {index}"))
            })
            .unwrap();
    }
    app.exec(&ctx, Cmd::SaveAll);
    assert_eq!(app.active, home);
    assert!(app.doc().is_none());
    assert!(!app.any_dirty());
    for doc in &app.docs {
        let reopened = OpenDoc::open(doc.path.as_ref().unwrap()).unwrap();
        assert_eq!(reopened.doc.workbook(), doc.doc.workbook());
    }
    app.active = 0;
    app.exec(&ctx, Cmd::SaveAll);
    assert_eq!(app.active, 0);

    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System] {
            let app = ThinodaApp::new(
                Settings {
                    language,
                    theme,
                    show_format_panel: false,
                    ..Default::default()
                },
                Services::default(),
                &[],
            );
            let mut h = Harness::builder()
                .with_size(egui::vec2(1000.0, 700.0))
                .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
            h.ctx
                .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            h.run_steps(4);
            for state in 0..3 {
                let active = state == 2;
                let save_all = state != 0;
                assert_eq!(h.state().command_available(&Cmd::SaveAll), save_all);
                assert_eq!(h.state().command_available(&Cmd::Save), active);
                assert_eq!(h.state().command_available(&Cmd::SaveAs), active);
                for format in EXPORT_MENU {
                    assert_eq!(h.state().command_available(&Cmd::Export(*format)), active);
                }
                let export = h.get_by_role_and_label(Role::Button, t("Export"));
                assert_eq!(export.accesskit_node().is_disabled(), !active);
                h.get_by_label(thinoda_ui::icons::MENU).click();
                h.run_steps(3);
                h.get_by_role_and_label(Role::Button, t("File")).click();
                h.run_steps(3);
                for (label, shortcut, enabled) in [
                    ("Save", "Ctrl+S", active),
                    ("Save As…", "Ctrl+Shift+S", active),
                    ("Save All", "", save_all),
                ] {
                    let accessible = if shortcut.is_empty() {
                        t(label).to_owned()
                    } else {
                        format!("{} {shortcut}", t(label))
                    };
                    assert_eq!(
                        h.get_by_role_and_label(Role::Button, &accessible)
                            .accesskit_node()
                            .is_disabled(),
                        !enabled,
                        "{label}"
                    );
                }
                let exports: Vec<_> = h
                    .query_all_by_role_and_label(Role::Button, t("Export"))
                    .collect();
                assert_eq!(exports.len(), 2);
                for export in exports {
                    assert_eq!(export.accesskit_node().is_disabled(), !active);
                }
                h.key_press(egui::Key::Escape);
                h.run_steps(3);
                h.key_press(egui::Key::Escape);
                h.run_steps(3);
                for command in [
                    Cmd::Save,
                    Cmd::SaveAs,
                    Cmd::SaveAll,
                    Cmd::Export(EXPORT_MENU[0]),
                ] {
                    let expected = if command == Cmd::SaveAll {
                        save_all
                    } else {
                        active
                    };
                    assert_palette(&mut h, command, expected);
                }
                match state {
                    0 => {
                        exec(&mut h, Cmd::New);
                        exec(&mut h, Cmd::Home);
                    }
                    1 => {
                        h.state_mut().active = 0;
                        h.run_steps(3);
                    }
                    _ => {}
                }
            }
        }
    }
}
