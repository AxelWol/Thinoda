//! Locale coverage is sequential because the application language is process-wide.

use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{Cmd, Services, SettingsTab, ThinodaApp},
    i18n::t,
};
use thinoda_platform::settings::{Language, Settings, ThemeMode};

#[test]
fn native_cjk_preference_is_reachable_and_missing_faces_preserve_maps() {
    let app = ThinodaApp::new(
        Settings {
            default_cjk_font: Some("Thinoda Missing Home Family".into()),
            ..Default::default()
        },
        Services::default(),
        &[],
    );
    let mut home = Harness::builder().build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    home.run_steps(4);
    assert!(home.state().doc().is_none());
    assert_eq!(
        home.query_all_by_label(&format!(
            "{}: Thinoda Missing Home Family. {}",
            t("Font unavailable"),
            t("Using the default font; the requested name is preserved.")
        ))
        .count(),
        1
    );
    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark] {
            for (size, scale) in [
                (egui::vec2(320.0, 240.0), 2.0),
                (egui::vec2(1000.0, 700.0), 1.0),
            ] {
                let mut app = ThinodaApp::new(
                    Settings {
                        language,
                        theme,
                        ui_scale: scale,
                        ..Default::default()
                    },
                    Services::default(),
                    &[],
                );
                app.exec(&egui::Context::default(), Cmd::New);
                let before = app.doc().unwrap().doc.workbook().clone();
                let revision = app.doc().unwrap().doc.state_id();
                app.exec(&egui::Context::default(), Cmd::Settings);
                app.dialogs.settings_tab = SettingsTab::Editor;
                let mut h = Harness::builder()
                    .with_size(size * scale)
                    .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
                h.ctx
                    .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
                h.run_steps(4);
                h.set_size(size);
                h.run_steps(4);
                h.get_by_role_and_label(Role::ComboBox, t("Default CJK font"))
                    .focus();
                h.run_steps(8);
                let rect = h
                    .get_by_role_and_label(Role::ComboBox, t("Default CJK font"))
                    .rect();
                assert!(h.ctx.content_rect().contains_rect(rect), "{rect:?}");
                h.get_by_role_and_label(Role::ComboBox, t("Default CJK font"))
                    .click();
                h.run_steps(3);
                h.get_by_role_and_label(Role::Button, t("Auto")).click();
                h.run_steps(4);
                assert!(h.state().settings.default_cjk_font.is_none());
                h.state_mut().settings.default_cjk_font = Some("Thinoda Missing CJK Family".into());
                h.run_steps(4);
                let warning = format!(
                    "{}: Thinoda Missing CJK Family. {}",
                    t("Font unavailable"),
                    t("Using the default font; the requested name is preserved.")
                );
                assert_eq!(h.query_all_by_label(&warning).count(), 1);
                h.run_steps(4);
                assert_eq!(h.query_all_by_label(&warning).count(), 1);
                assert_eq!(
                    h.state().settings.default_cjk_font.as_deref(),
                    Some("Thinoda Missing CJK Family")
                );
                h.get_by_role_and_label(Role::ComboBox, t("Default CJK font"))
                    .focus();
                h.run_steps(8);
                h.key_press(egui::Key::Enter);
                h.run_steps(3);
                h.get_by_role_and_label(Role::Button, t("Auto")).click();
                h.run_steps(4);
                assert!(h.state().settings.default_cjk_font.is_none());
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);
            }
        }
    }
}
