//! Separate process because the app's current UI language is process-wide.

use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::app::{Cmd, Services, ThinodaApp};
use thinoda_platform::settings::{Language, Settings, ThemeMode};

#[test]
fn theme_and_language_selectors_change_independently() {
    let settings = Settings {
        language: Language::English,
        theme: ThemeMode::System,
        ..Settings::default()
    };
    let mut app = ThinodaApp::new(settings, Services::default(), &[]);
    let ctx = egui::Context::default();
    app.exec(&ctx, Cmd::New);
    app.exec(&ctx, Cmd::Settings);
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 720.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(4);

    // Toggles used to render as bare switches at the left edge without their
    // caption; in a labelled row they are right-aligned like the other fields.
    let field_right = h
        .get_by_role_and_label(Role::SpinButton, "Autosave interval (s)")
        .rect()
        .right();
    for label in ["Autosave", "Keep backup on save", "Confirm before deleting"] {
        let switch = h.get_by_role_and_label(Role::Switch, label).rect();
        assert!(
            (switch.right() - field_right).abs() < 2.0,
            "switch '{label}' is not in a labelled row: {switch:?}"
        );
    }
    h.get_by_role_and_label(Role::Tab, "Dark").click();
    h.run_steps(3);
    assert_eq!(h.state().settings.theme, ThemeMode::Dark);
    assert_eq!(h.state().settings.language, Language::English);

    h.get_by_role_and_label(Role::ComboBox, "Language").click();
    h.run_steps(2);
    h.get_by_label("Deutsch").click();
    h.run_steps(3);
    assert_eq!(h.state().settings.language, Language::German);
    assert_eq!(h.state().settings.theme, ThemeMode::Dark);

    h.get_by_role_and_label(Role::Tab, "Hell").click();
    h.run_steps(3);
    assert_eq!(h.state().settings.theme, ThemeMode::Light);
    assert_eq!(h.state().settings.language, Language::German);
    drop(h);
    language_dropdown_switches_immediately_and_survives_restart_in_both_themes();
}

fn language_dropdown_switches_immediately_and_survives_restart_in_both_themes() {
    // Run in the same process sequentially: the UI language is process-wide.
    use thinoda_app::i18n::{resolve, t, t_in};
    use thinoda_platform::Paths;

    let root = tempfile::tempdir().unwrap();
    let paths = Paths::from_root(root.path());
    paths.ensure().unwrap();
    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        for language in Language::ALL {
            let mut app = ThinodaApp::new(
                Settings {
                    language: Language::English,
                    theme,
                    ..Settings::default()
                },
                Services {
                    paths: Some(paths.clone()),
                    ..Services::default()
                },
                &[],
            );
            let ctx = egui::Context::default();
            app.exec(&ctx, Cmd::New);
            app.save_settings();
            app.exec(&ctx, Cmd::Settings);
            let mut h = Harness::builder()
                .with_size(egui::vec2(1280.0, 800.0))
                .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
            h.run_steps(3);
            let combo = h.get_by_role_and_label(Role::ComboBox, "Language");
            assert!(h.ctx.content_rect().contains_rect(combo.rect()));
            combo.click();
            h.run_steps(2);
            h.key_press(egui::Key::Escape);
            h.run_steps(2);
            assert_eq!(h.state().settings.language, Language::English);
            h.get_by_role_and_label(Role::ComboBox, "Language").click();
            h.run_steps(2);
            let option = if language == Language::Auto {
                "Auto"
            } else {
                language.native_name()
            };
            h.get_by_role_and_label(Role::Button, option).scroll_to_me();
            h.run_steps(3);
            h.get_by_role_and_label(Role::Button, option).click();
            h.run_steps(3);
            assert_eq!(h.state().settings.language, language);
            assert_eq!(h.state().settings.theme, theme);
            let lang = resolve(language);
            assert_eq!(t("Save"), t_in(lang, "Save"));
            assert_eq!(
                h.state().doc().unwrap().view.titles.main,
                t_in(lang, "Main Topic")
            );
            let dropdown = h.get_by_role_and_label(Role::ComboBox, t_in(lang, "Language"));
            assert!(h.ctx.content_rect().contains_rect(dropdown.rect()));
            let persisted = Settings::load(&paths).unwrap();
            assert_eq!(persisted.language, language);
            h.state_mut().dialogs.settings = false;
            let ctx = h.ctx.clone();
            h.state_mut().exec(&ctx, Cmd::UserGuide);
            h.run_steps(3);
            h.get_by_label(t_in(lang, "Search the guide"));
            h.get_by_role_and_label(Role::Label, t_in(lang, "Platform and Privacy"));
            let close = h
                .get_by_role_and_label(Role::Button, t_in(lang, "Close"))
                .rect();
            assert!(
                h.ctx.content_rect().contains_rect(close),
                "{language:?}/{theme:?}: {close:?}"
            );
            let restarted = ThinodaApp::new(persisted, Services::default(), &[]);
            assert_eq!(restarted.settings.theme, theme);
            assert_eq!(thinoda_app::i18n::lang(), lang);
        }
    }
}
