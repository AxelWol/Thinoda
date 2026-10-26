//! Sequential locale coverage for the Settings-to-shortcut-reference route.

use egui::{Key, accesskit::Role};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use thinoda_app::{
    app::{CenterMode, Cmd, Services, SettingsTab, ThinodaApp},
    i18n::t,
};
use thinoda_platform::settings::{Language, Settings, ThemeMode};

fn open_shortcuts(h: &mut Harness<'_, ThinodaApp>) {
    h.get_by_role_and_label(Role::Button, t("Keyboard Shortcuts"))
        .scroll_to_me();
    h.run_steps(8);
    let entry = h.get_by_role_and_label(Role::Button, t("Keyboard Shortcuts"));
    assert!(h.ctx.content_rect().contains_rect(entry.rect()));
    entry.click();
    h.run_steps(5);
    assert!(h.state().dialogs.settings);
    assert!(h.state().dialogs.shortcuts);
    assert!(
        h.get_by_role_and_label(Role::Button, t("Keyboard Shortcuts"))
            .accesskit_node()
            .is_disabled()
    );
}

#[test]
fn settings_opens_existing_reference_and_returns_without_losing_context() {
    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark] {
            for (mode, tab) in [
                (None, SettingsTab::General),
                (Some(CenterMode::Map), SettingsTab::Ai),
                (Some(CenterMode::Outliner), SettingsTab::Editor),
                (None, SettingsTab::Cloud),
            ] {
                let mut app = ThinodaApp::new(
                    Settings {
                        language,
                        theme,
                        ui_scale: 2.0,
                        show_format_panel: false,
                        ..Default::default()
                    },
                    Services::default(),
                    &[],
                );
                let ctx = egui::Context::default();
                if let Some(mode) = mode {
                    app.exec(&ctx, Cmd::New);
                    app.mode = mode;
                }
                app.exec(&ctx, Cmd::Settings);
                app.dialogs.settings_tab = tab;
                let size = egui::vec2(320.0, 240.0);
                let mut h = Harness::builder()
                    .with_size(size * 2.0)
                    .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
                h.ctx
                    .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
                h.run_steps(4);
                h.set_size(size);
                h.run_steps(4);
                assert!((h.ctx.content_rect().width() - size.x).abs() < 0.01);
                if tab == SettingsTab::Ai {
                    h.get_by_role_and_label(Role::PasswordInput, t("API Key"))
                        .scroll_to_me();
                    h.run_steps(8);
                    h.get_by_role_and_label(Role::PasswordInput, t("API Key"))
                        .click();
                    h.run_steps(2);
                    h.get_by_role_and_label(Role::PasswordInput, t("API Key"))
                        .type_text("unsaved-test-draft");
                    h.run_steps(4);
                    assert_eq!(
                        h.state().ai.api_key_draft.as_deref(),
                        Some("unsaved-test-draft")
                    );
                }
                let before = h.state().doc().map(|doc| {
                    (
                        doc.doc.workbook().clone(),
                        doc.doc.state_id(),
                        doc.view.selection.clone(),
                    )
                });
                let settings = h.state().settings.clone();
                let draft = h.state().ai.api_key_draft.clone();
                for escape in [true, false] {
                    open_shortcuts(&mut h);
                    let search = t("Search commands or shortcuts");
                    h.get_by_role_and_label(Role::TextInput, search)
                        .scroll_to_me();
                    h.run_steps(8);
                    h.get_by_role_and_label(Role::TextInput, search).click();
                    h.run_steps(2);
                    h.get_by_role_and_label(Role::TextInput, search)
                        .type_text("save");
                    h.run_steps(3);
                    assert_eq!(h.state().dialogs.shortcut_browser.query, "save");
                    h.key_press(Key::Delete);
                    h.run_steps(2);
                    let notice = t("Choose Edit mappings to create or change a keyboard profile.");
                    h.get_by_role_and_label(Role::Label, notice).scroll_to_me();
                    h.run_steps(8);
                    assert!(
                        h.ctx
                            .content_rect()
                            .contains_rect(h.get_by_role_and_label(Role::Label, notice).rect())
                    );
                    if escape {
                        h.key_press(Key::Escape);
                    } else {
                        h.get_by_role_and_label(Role::Button, t("Close")).click();
                    }
                    h.run_steps(4);
                    assert!(!h.state().dialogs.shortcuts);
                    assert!(h.state().dialogs.settings);
                    assert_eq!(h.state().dialogs.settings_tab, tab);
                    assert_eq!(h.state().settings, settings);
                    assert_eq!(h.state().ai.api_key_draft, draft);
                    assert!(h.state().ai.test.is_none());
                    assert_eq!(
                        h.state().doc().map(|doc| (
                            doc.doc.workbook().clone(),
                            doc.doc.state_id(),
                            doc.view.selection.clone(),
                        )),
                        before
                    );
                    assert!(
                        !h.get_by_role_and_label(Role::Button, t("Keyboard Shortcuts"))
                            .accesskit_node()
                            .is_disabled()
                    );
                }
            }
        }
    }
}
