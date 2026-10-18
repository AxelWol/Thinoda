//! Language changes are global; keep the translated legal viewer cases in one process.
use egui::{Key, accesskit::Role};
use egui_kittest::{
    Harness,
    kittest::{By, NodeT, Queryable},
};
use thinoda_app::{
    app::{Cmd, Services, ThinodaApp},
    i18n::t,
};
use thinoda_platform::settings::{Language, Settings, ThemeMode};

#[test]
fn about_shows_complete_offline_legal_texts_with_modal_isolation_and_visible_close() {
    for language in [Language::English, Language::German] {
        for theme in [ThemeMode::Light, ThemeMode::Dark] {
            let mut app = ThinodaApp::new(
                Settings {
                    language,
                    theme,
                    ..Default::default()
                },
                Services::default(),
                &[],
            );
            app.exec(&egui::Context::default(), Cmd::New);
            app.exec(&egui::Context::default(), Cmd::About);
            let before = app.doc().unwrap().doc.workbook().clone();
            let mut h = Harness::builder()
                .with_size(egui::vec2(1100.0, 800.0))
                .with_context_setup(|ctx| {
                    ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu)
                })
                .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
            h.run_steps(3);
            h.get_by_role_and_label(Role::Button, t("Legal documents"))
                .click();
            h.run_steps(3);
            assert!(h.state().dialogs.legal.open);
            assert!(h.state().dialogs.about);
            assert!(
                !h.output()
                    .platform_output
                    .commands
                    .iter()
                    .any(|command| matches!(command, egui::OutputCommand::OpenUrl(_)))
            );

            for (title, text) in [
                ("License", include_str!("../../../LICENSE.md")),
                (
                    "Third-party notices",
                    include_str!("../../../THIRD_PARTY_NOTICES.md"),
                ),
                ("Legal notice", include_str!("../../../IMPRESSUM.md")),
            ] {
                h.get_by_role_and_label(Role::Button, t(title)).click();
                h.run_steps(3);
                let body = h
                    .query_all(By::new().role(Role::Label).value(text))
                    .next()
                    .expect("complete legal document is present");
                assert_eq!(body.accesskit_node().value().as_deref(), Some(text));
                h.key_press(Key::Enter);
                h.key_press(Key::Tab);
                h.run_steps(3);
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
            }

            h.set_size(egui::vec2(480.0, 420.0));
            h.run_steps(3);
            assert!(
                h.ctx
                    .content_rect()
                    .contains_rect(h.get_by_role_and_label(Role::Button, t("Close")).rect())
            );
            h.key_press(Key::Escape);
            h.run_steps(3);
            assert!(!h.state().dialogs.legal.open);
            assert!(h.state().dialogs.about);
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
            h.get_by_role_and_label(Role::Button, t("Legal documents"))
                .click();
            h.run_steps(3);
            h.get_by_role_and_label(Role::Button, t("Close")).click();
            h.run_steps(3);
            assert!(!h.state().dialogs.legal.open);
        }
    }
}
