#![cfg(target_os = "windows")]

use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{Cmd, Services, ThinodaApp},
    i18n::t,
};
use thinoda_platform::settings::{Language, Settings, ThemeMode};

#[test]
fn translated_ribbon_tabs_and_commands_remain_bounded_at_desktop_scales() {
    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System] {
            for scale in [1.0, 1.5, 2.0] {
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
                let mut h = Harness::builder()
                    .with_size(egui::vec2(720.0, 480.0) * scale)
                    .with_step_dt(1.0 / 60.0)
                    .with_context_setup(move |ctx| {
                        ctx.set_zoom_factor(scale);
                        ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
                    })
                    .build_ui_state(
                        |ui, app: &mut ThinodaApp| {
                            app.show_ribbon(ui);
                            egui::CentralPanel::default().show(ui, |ui| {
                                ui.label("Canvas workspace");
                                assert!(
                                    ui.available_height() > 240.0,
                                    "Ribbon consumed canvas height"
                                );
                            });
                        },
                        app,
                    );
                h.run_steps(4);
                for (category, command) in [
                    ("File", "New"),
                    ("Edit", "Undo"),
                    ("Sheet", "New Sheet"),
                    ("Insert", "Topic"),
                    ("AI", "AI: Brainstorm Subtopics"),
                    ("Tools", "Map Shot"),
                    ("View", "Previous Sheet"),
                    ("Help", "Quick Start"),
                ] {
                    h.get_by_role_and_label(Role::Button, t(category))
                        .scroll_to_me();
                    h.run_steps(16);
                    assert!(
                        h.ctx.content_rect().contains_rect(
                            h.get_by_role_and_label(Role::Button, t(category)).rect()
                        ),
                        "{language:?}/{theme:?}/{scale}/{category}"
                    );
                    h.get_by_role_and_label(Role::Button, t(category)).click();
                    h.run_steps(3);
                    assert!(
                        h.query_by_role_and_label(Role::Button, t(command))
                            .is_some(),
                        "{language:?}/{theme:?}/{scale}/{category}: {:?}",
                        h.get_by_role_and_label(Role::Button, t(category)).rect()
                    );
                    h.get_by_role_and_label(Role::Button, t(command))
                        .scroll_to_me();
                    h.run_steps(16);
                    let rect = h.get_by_role_and_label(Role::Button, t(command)).rect();
                    assert!(
                        h.ctx.content_rect().contains_rect(rect),
                        "{language:?}/{theme:?}/{scale}/{category}: {rect:?}"
                    );
                }
            }
        }
    }
}
