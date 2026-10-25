//! Settings entry, persisted profile editing and real application dispatch.

use egui::{Key, Modifiers, accesskit::Role};
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::app::{CenterMode, Cmd, Services, SettingsTab, ThinodaApp};
use thinoda_platform::{
    Paths, Settings,
    settings::{Language, ThemeMode},
};

fn click(h: &mut Harness<'_, ThinodaApp>, label: &str) {
    h.get_by_role_and_label(Role::Button, label).scroll_to_me();
    h.run_steps(4);
    h.get_by_role_and_label(Role::Button, label).click();
    h.run_steps(4);
}

fn harness(settings: Settings, paths: Paths) -> Harness<'static, ThinodaApp> {
    let mut app = ThinodaApp::new(
        settings,
        Services {
            paths: Some(paths),
            ..Default::default()
        },
        &[],
    );
    app.exec(&egui::Context::default(), Cmd::New);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 850.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    h.run_steps(4);
    h
}

#[test]
fn settings_profile_apply_dispatches_after_closing_dialogs_and_after_restart() {
    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        for mode in [CenterMode::Map, CenterMode::Outliner] {
            let home = tempfile::tempdir().unwrap();
            let paths = Paths::from_root(home.path());
            paths.ensure().unwrap();
            let mut h = harness(
                Settings {
                    language: Language::English,
                    theme,
                    show_format_panel: false,
                    ..Default::default()
                },
                paths.clone(),
            );
            h.state_mut().mode = mode;
            let ctx = h.ctx.clone();
            h.state_mut().exec(&ctx, Cmd::Settings);
            h.state_mut().dialogs.settings_tab = SettingsTab::Editor;
            h.run_steps(4);
            let before = h.state().doc().unwrap().doc.workbook().clone();
            let before_id = h.state().doc().unwrap().doc.state_id();
            click(&mut h, "Keyboard Shortcuts");
            click(&mut h, "Edit mappings");
            h.get_by_role_and_label(Role::TextInput, "Profile name")
                .click();
            h.get_by_role_and_label(Role::TextInput, "Profile name")
                .type_text("Acceptance");
            h.run_steps(3);
            click(&mut h, "Duplicate");
            h.get_by_role_and_label(Role::TextInput, "Search mappings")
                .click();
            h.get_by_role_and_label(Role::TextInput, "Search mappings")
                .type_text("New");
            h.run_steps(3);
            click(&mut h, "Assign: New");
            h.key_press_modifiers(Modifiers::COMMAND | Modifiers::ALT, Key::J);
            h.run_steps(4);
            assert_eq!(h.state().docs.len(), 1, "Capture must not dispatch New");
            click(&mut h, "Apply");
            assert_eq!(
                h.state().settings.keymaps.profile().unwrap().name,
                "Acceptance"
            );
            click(&mut h, "Close");
            assert!(h.state().dialogs.settings);
            assert_eq!(h.state().dialogs.settings_tab, SettingsTab::Editor);
            click(&mut h, "Close window");
            assert!(!h.state().dialogs.settings);
            assert!(!h.state().dialogs.shortcuts);
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
            assert_eq!(h.state().doc().unwrap().doc.state_id(), before_id);
            h.key_press_modifiers(Modifiers::COMMAND | Modifiers::ALT, Key::J);
            h.run_steps(4);
            assert_eq!(h.state().docs.len(), 2, "{theme:?}/{mode:?}");
            assert_eq!(h.state().docs[0].doc.workbook(), &before);

            let saved = Settings::load(&paths).unwrap();
            assert_eq!(saved.keymaps, h.state().settings.keymaps);
            drop(h);
            let mut restarted = harness(saved, paths);
            restarted.state_mut().mode = mode;
            restarted.run_steps(4);
            restarted.key_press_modifiers(Modifiers::COMMAND | Modifiers::ALT, Key::J);
            restarted.run_steps(4);
            assert_eq!(restarted.state().docs.len(), 2, "{theme:?}/{mode:?}");
        }
    }
}
