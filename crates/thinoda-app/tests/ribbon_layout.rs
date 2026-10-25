#![cfg(any(target_os = "windows", target_os = "macos"))]

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
        for (theme, compact_ribbon) in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System]
            .into_iter()
            .flat_map(|theme| [false, true].map(|compact| (theme, compact)))
        {
            for scale in [1.0, 1.5, 2.0] {
                let mut app = ThinodaApp::new(
                    Settings {
                        language,
                        theme,
                        compact_ribbon,
                        ..Default::default()
                    },
                    Services::default(),
                    &[],
                );
                app.exec(&egui::Context::default(), Cmd::New);
                app.exec(&egui::Context::default(), Cmd::NewSheet);
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
                                    ui.available_height()
                                        >= if app.settings.compact_ribbon {
                                            240.0
                                        } else {
                                            200.0
                                        },
                                    "Ribbon consumed canvas height: {} remaining in {:?}",
                                    ui.available_height(),
                                    ui.ctx().content_rect()
                                );
                            });
                        },
                        app,
                    );
                h.run_steps(4);
                for (category, command, group, action) in [
                    ("File", "New", "Open…", Cmd::New),
                    (
                        "Home",
                        "Undo",
                        "Editing",
                        Cmd::Canvas(thinoda_canvas::Action::Undo),
                    ),
                    ("Sheet", "New Sheet", "Sheet", Cmd::NewSheet),
                    (
                        "Insert",
                        "Topic",
                        "Insert",
                        Cmd::Canvas(thinoda_canvas::Action::InsertSibling),
                    ),
                    (
                        "AI",
                        "AI: Brainstorm Subtopics",
                        "AI",
                        Cmd::Ai(thinoda_app::ai::AiCommand::Brainstorm),
                    ),
                    ("Capture", "Map Shot", "Capture", Cmd::MapShot),
                    ("View", "Previous Sheet", "Sheet", Cmd::PreviousSheet),
                    ("Help", "Quick Start", "Help", Cmd::QuickStart),
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
                    let mut command_label = t(command).to_owned();
                    if h.query_by_role_and_label(Role::Button, &command_label)
                        .is_none()
                    {
                        h.get_by_role_and_label(Role::Button, &format!("{}…", t(group)))
                            .click();
                        h.run_steps(3);
                        if let Some(hint) = ThinodaApp::shortcut_hint(&h.ctx, &action) {
                            command_label = format!("{command_label} {hint}");
                        }
                    }
                    assert!(
                        h.query_by_role_and_label(Role::Button, &command_label)
                            .is_some(),
                        "{language:?}/{theme:?}/{scale}/{category}: {:?}",
                        h.get_by_role_and_label(Role::Button, t(category)).rect()
                    );
                    h.get_by_role_and_label(Role::Button, &command_label)
                        .scroll_to_me();
                    h.run_steps(16);
                    let rect = h.get_by_role_and_label(Role::Button, &command_label).rect();
                    assert!(
                        h.ctx.content_rect().contains_rect(rect),
                        "{language:?}/{theme:?}/{scale}/{category}: {rect:?}"
                    );
                    h.key_press(egui::Key::Escape);
                    h.run_steps(2);
                }
            }
        }
    }
}
