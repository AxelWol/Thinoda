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
        .with_size(egui::vec2(1280.0, 800.0))
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

    h.get_by_role_and_label(Role::Tab, "Deutsch").click();
    h.run_steps(3);
    assert_eq!(h.state().settings.language, Language::German);
    assert_eq!(h.state().settings.theme, ThemeMode::Dark);

    h.get_by_role_and_label(Role::Tab, "Hell").click();
    h.run_steps(3);
    assert_eq!(h.state().settings.theme, ThemeMode::Light);
    assert_eq!(h.state().settings.language, Language::German);
}
