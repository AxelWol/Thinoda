//! Locale changes are process-wide; exercise this matrix sequentially.

use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{Cmd, Services, SettingsTab, ThinodaApp},
    i18n::t,
};
use thinoda_platform::{
    Paths,
    settings::{Language, Settings, ThemeMode},
};

#[test]
fn alignment_preference_is_reachable_persisted_and_applied_without_document_edits() {
    let root = tempfile::tempdir().unwrap();
    let paths = Paths::from_root(root.path());
    paths.ensure().unwrap();
    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System] {
            for size in [egui::vec2(320.0, 240.0), egui::vec2(1000.0, 700.0)] {
                let mut app = ThinodaApp::new(
                    Settings {
                        language,
                        theme,
                        ..Default::default()
                    },
                    Services {
                        paths: Some(paths.clone()),
                        ..Default::default()
                    },
                    &[],
                );
                app.exec(&egui::Context::default(), Cmd::New);
                app.exec(&egui::Context::default(), Cmd::Settings);
                app.dialogs.settings_tab = SettingsTab::Editor;
                let before = app.doc().unwrap().doc.workbook().clone();
                let revision = app.doc().unwrap().doc.state_id();
                let mut h = Harness::builder()
                    .with_size(size)
                    .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
                h.ctx
                    .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
                h.run_steps(4);
                h.get_by_role_and_label(Role::Switch, t("Show alignment guides"))
                    .focus();
                h.run_steps(8);
                let rect = h
                    .get_by_role_and_label(Role::Switch, t("Show alignment guides"))
                    .rect();
                assert!(
                    h.ctx.content_rect().contains_rect(rect),
                    "{language:?}: {rect:?}"
                );
                h.key_press(egui::Key::Space);
                h.run_steps(4);
                assert!(h.state().settings.show_alignment_guides);
                assert!(Settings::load(&paths).unwrap().show_alignment_guides);
                h.state_mut().dialogs.settings = false;
                h.run_steps(4);
                assert!(h.state().doc().unwrap().view.show_alignment_guides);
                let book = before.clone();
                h.state_mut().new_doc(book);
                h.run_steps(4);
                assert!(h.state().doc().unwrap().view.show_alignment_guides);
                h.state_mut().active = 0;
                let ctx = h.ctx.clone();
                h.state_mut().exec(&ctx, Cmd::Settings);
                h.run_steps(4);
                h.get_by_role_and_label(Role::Switch, t("Show alignment guides"))
                    .focus();
                h.run_steps(8);
                h.get_by_role_and_label(Role::Switch, t("Show alignment guides"))
                    .click();
                h.run_steps(4);
                assert!(!Settings::load(&paths).unwrap().show_alignment_guides);
                h.state_mut().dialogs.settings = false;
                h.run_steps(4);
                assert!(!h.state().doc().unwrap().view.show_alignment_guides);
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);
            }
        }
    }
}
