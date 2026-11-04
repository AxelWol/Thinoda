//! Language changes are process-wide; exercise this matrix in one test.

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
fn settings_controls_and_recovery_warning_remain_reachable() {
    let root = tempfile::tempdir().unwrap();
    let paths = Paths::from_root(root.path());
    paths.ensure().unwrap();
    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark] {
            for size in [
                egui::vec2(240.0, 200.0),
                egui::vec2(320.0, 240.0),
                egui::vec2(1000.0, 700.0),
            ] {
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
                let mut h = Harness::builder()
                    .with_size(size)
                    .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
                h.ctx
                    .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
                h.run_steps(8);
                let warning = t("Crash recovery is unavailable. Save or download your work.");
                assert!(h.state().toasts.texts().any(|text| text == warning));
                let toggle = h.get_by_role_and_label(Role::Switch, t("Show alignment guides"));
                toggle.focus();
                h.run_steps(8);
                let toggle_rect = h
                    .get_by_role_and_label(Role::Switch, t("Show alignment guides"))
                    .rect();
                let toast_rect = h
                    .ctx
                    .memory(|memory| memory.area_rect(egui::Id::unique(("thinoda_toasts", false))))
                    .expect("recovery warning is rendered");
                assert!(
                    h.ctx.content_rect().contains_rect(toast_rect),
                    "{language:?} {size:?} {theme:?}: {toast_rect:?}"
                );
                assert!(toast_rect.intersects(h.get_by_label(warning).rect()));
                assert!(
                    !toast_rect.intersects(toggle_rect),
                    "{language:?} {size:?} {theme:?}: toast {toast_rect:?}, switch {toggle_rect:?}"
                );
                h.get_by_role_and_label(Role::Switch, t("Show alignment guides"))
                    .click();
                h.run_steps(4);
                assert!(
                    h.state().settings.show_alignment_guides,
                    "{language:?} {size:?} {theme:?}: switch {toggle_rect:?}, toast {toast_rect:?}, settings {:?}, layer {:?}",
                    h.ctx
                        .memory(|memory| memory.area_rect(egui::Id::unique("thinoda_settings"))),
                    h.ctx.layer_id_at(toggle_rect.center()),
                );
                assert!(h.state().toasts.texts().any(|text| text == warning));
                if language == Language::Auto
                    && theme == ThemeMode::Light
                    && size == egui::vec2(320.0, 240.0)
                {
                    h.get_by_role_and_label(Role::Button, t("Keyboard Shortcuts"))
                        .scroll_to_me();
                    h.run_steps(8);
                    h.get_by_role_and_label(Role::Button, t("Keyboard Shortcuts"))
                        .click();
                    h.run_steps(4);
                    assert!(h.state().dialogs.shortcuts);
                    let search = t("Search commands or shortcuts");
                    h.get_by_role_and_label(Role::TextInput, search)
                        .scroll_to_me();
                    h.run_steps(8);
                    h.get_by_role_and_label(Role::TextInput, search).click();
                    h.run_steps(2);
                    h.get_by_role_and_label(Role::TextInput, search)
                        .type_text("save");
                    h.run_steps(4);
                    assert_eq!(h.state().dialogs.shortcut_browser.query, "save");
                    h.get_by_role_and_label(Role::Button, t("Close")).click();
                    h.run_steps(4);
                    assert!(!h.state().dialogs.shortcuts);
                    assert!(h.state().dialogs.settings);
                    assert!(h.state().toasts.texts().any(|text| text == warning));
                }
                h.get_by_label(thinoda_ui::icons::CLOSE).click();
                h.run_steps(4);
                assert!(!h.state().toasts.texts().any(|text| text == warning));
                h.get_by_role_and_label(Role::Switch, t("Show alignment guides"))
                    .scroll_to_me();
                h.run_steps(8);
                h.get_by_role_and_label(Role::Switch, t("Show alignment guides"))
                    .focus();
                h.run_steps(8);
                h.get_by_role_and_label(Role::Switch, t("Show alignment guides"))
                    .click();
                h.run_steps(4);
                assert!(!h.state().settings.show_alignment_guides);
            }
        }
    }
}
