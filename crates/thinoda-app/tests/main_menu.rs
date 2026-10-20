//! Menu navigation changes the process-wide language; keep the matrix in one test.

use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{Cmd, Services, ThinodaApp},
    i18n::t,
};
use thinoda_platform::settings::{Language, Settings, ThemeMode};

#[test]
fn compact_root_menu_and_grouped_actions_preserve_localized_navigation() {
    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System] {
            for (width, height, scale) in [(640.0, 360.0, 1.0), (1280.0, 720.0, 2.0)] {
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
                let mut h = Harness::builder()
                    .with_size(egui::vec2(width, height))
                    .with_context_setup(|ctx| {
                        ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
                    })
                    .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
                h.run_steps(4);
                let before = h.state().doc().unwrap().doc.workbook().clone();
                let selection = h.state().doc().unwrap().view.selection.clone();
                let (_, settings_command) = ThinodaApp::palette_commands()
                    .into_iter()
                    .find(|(command, _)| matches!(command, Cmd::Settings))
                    .unwrap();
                assert_eq!(settings_command.category.as_deref(), Some(t("Edit")));
                h.get_by_label(thinoda_ui::icons::MENU).click();
                h.run_steps(3);
                for category in [
                    "File", "Edit", "Sheet", "Insert", "AI", "Tools", "View", "Help",
                ] {
                    let rect = h.get_by_role_and_label(Role::Button, t(category)).rect();
                    assert!(
                        h.ctx.content_rect().contains_rect(rect),
                        "{language:?}/{theme:?}/{scale}: {category}: {rect:?}"
                    );
                }
                let settings_label = format!("{} Ctrl+,", t("Settings…"));
                assert!(
                    h.query_by_role_and_label(Role::Button, &settings_label)
                        .is_none()
                );
                h.key_press(egui::Key::Escape);
                h.run_steps(2);
                assert_eq!(h.state().doc().unwrap().view.selection, selection);
                // Submenu overflow is tracked separately from compact root navigation.
                h.set_size(egui::vec2(1280.0, 1600.0));
                h.run_steps(3);
                h.get_by_label(thinoda_ui::icons::MENU).click();
                h.run_steps(3);
                h.get_by_role_and_label(Role::Button, t("File")).click();
                h.run_steps(3);
                let quit = format!("{} Ctrl+Q", t("Quit"));
                h.get_by_role_and_label(Role::Button, &quit).scroll_to_me();
                h.run_steps(6);
                let rect = h.get_by_role_and_label(Role::Button, &quit).rect();
                assert!(
                    h.ctx.content_rect().contains_rect(rect),
                    "{language:?}/{theme:?}/{scale}: Quit out of view: {rect:?}"
                );
                h.key_press(egui::Key::Escape);
                h.run_steps(2);
                if h.query_by_role_and_label(Role::Button, t("Edit")).is_none() {
                    h.get_by_label(thinoda_ui::icons::MENU).click();
                    h.run_steps(3);
                }
                h.get_by_role_and_label(Role::Button, t("Edit")).click();
                h.run_steps(3);
                h.get_by_role_and_label(Role::Button, &settings_label)
                    .scroll_to_me();
                h.run_steps(6);
                let rect = h
                    .get_by_role_and_label(Role::Button, &settings_label)
                    .rect();
                assert!(
                    h.ctx.content_rect().contains_rect(rect),
                    "{language:?}/{theme:?}/{scale}: Settings out of view: {rect:?}"
                );
                h.get_by_role_and_label(Role::Button, &settings_label)
                    .click();
                h.run_steps(3);
                assert!(h.state().dialogs.settings);
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                assert_eq!(h.state().doc().unwrap().view.selection, selection);
            }
        }
    }
}
