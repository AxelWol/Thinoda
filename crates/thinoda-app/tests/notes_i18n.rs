use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{Cmd, ThinodaApp},
    i18n::t,
};
use thinoda_platform::settings::{Language, Settings};

#[test]
fn topic_style_and_notes_actions_have_distinct_names_in_every_language() {
    for language in Language::ALL {
        let mut app = ThinodaApp::new(
            Settings {
                language,
                ..Default::default()
            },
            Default::default(),
            &[],
        );
        app.exec(&egui::Context::default(), Cmd::New);
        let mut h = Harness::builder()
            .with_size(egui::vec2(1280.0, 1000.0))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        h.run_steps(3);
        assert_ne!(t("Bold"), t("Bold notes"));
        assert_ne!(t("Italic"), t("Italic notes"));
        for label in ["Bold", "Italic", "Bold notes", "Italic notes"] {
            assert_eq!(
                h.query_all_by_role_and_label(Role::Button, t(label))
                    .count(),
                1,
                "{language:?}: ambiguous action {label}"
            );
        }
        h.get_by_role_and_label(Role::CheckBox, t("Markdown preview"))
            .scroll_to_me();
        h.run_steps(8);
        h.get_by_role_and_label(Role::CheckBox, t("Markdown preview"))
            .click();
        h.run_steps(4);
        assert_eq!(
            h.query_all_by_role_and_label(Role::Button, t("Copy note as plain text"))
                .count(),
            1,
            "{language:?}: missing localized plain-text action"
        );
    }
}
