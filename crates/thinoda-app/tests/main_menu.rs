//! Menu navigation changes the process-wide language; keep the matrix in one test.

use egui::{accesskit::Role, os::OperatingSystem};
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{Cmd, Services, ThinodaApp},
    i18n::t,
};
use thinoda_platform::settings::{Language, Settings, ThemeMode};

mod support;
use support::menu_action;

fn reveal(h: &mut Harness<'_, ThinodaApp>, label: &str) {
    menu_action(h, label).scroll_to_me();
    h.run_steps(8);
    let rect = menu_action(h, label).rect();
    assert!(
        h.ctx.content_rect().contains_rect(rect),
        "{label} out of view: {rect:?}"
    );
    let viewport = h.ctx.content_rect().expand(0.5 / h.ctx.pixels_per_point());
    h.ctx.memory(|memory| {
        for layer in memory
            .layer_ids()
            .filter(|layer| layer.order == egui::Order::Foreground)
        {
            if let Some(rect) = memory.area_rect(layer.id) {
                assert!(
                    viewport.contains_rect(rect),
                    "Popup near {label} exceeds viewport: {rect:?}"
                );
            }
        }
    });
}

#[test]
fn compact_root_menu_and_grouped_actions_preserve_localized_navigation() {
    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System] {
            for (width, height, scale) in [
                (640.0, 360.0, 1.0),
                (960.0, 540.0, 1.5),
                (1280.0, 720.0, 2.0),
                (1280.0, 480.0, 2.0),
            ] {
                let mut app = ThinodaApp::new(
                    Settings {
                        language,
                        theme,
                        ui_scale: scale,
                        show_format_panel: false,
                        ..Default::default()
                    },
                    Services::default(),
                    &[],
                );
                app.exec(&egui::Context::default(), Cmd::New);
                let mut h = Harness::builder()
                    .with_size(egui::vec2(width, height))
                    .with_context_setup(|ctx| {
                        ctx.set_os(OperatingSystem::Windows);
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
                h.get_by_label(thinoda_app::i18n::t("Main Menu")).click();
                h.run_steps(3);
                for category in [
                    "File", "Edit", "Sheet", "Insert", "AI", "Tools", "View", "Help",
                ] {
                    reveal(&mut h, t(category));
                    let rect = h.get_by_role_and_label(Role::Button, t(category)).rect();
                    assert!(
                        h.ctx.content_rect().contains_rect(rect),
                        "{language:?}/{theme:?}/{scale}: {category}: {rect:?}"
                    );
                }

                let settings_label = format!(
                    "{} {}",
                    t("Settings…"),
                    ThinodaApp::shortcut_hint(&h.ctx, &Cmd::Settings).unwrap()
                );
                h.key_press(egui::Key::Escape);
                h.run_steps(2);
                assert_eq!(h.state().doc().unwrap().view.selection, selection);
                for (category, first, command, last) in [
                    ("Sheet", "New Sheet", Cmd::NewSheet, "Delete Sheet"),
                    (
                        "Insert",
                        "Topic",
                        Cmd::Canvas(thinoda_canvas::Action::InsertSibling),
                        "Labels",
                    ),
                    (
                        "AI",
                        thinoda_app::ai::AiCommand::Brainstorm.label(),
                        Cmd::Ai(thinoda_app::ai::AiCommand::Brainstorm),
                        thinoda_app::ai::AiCommand::ALL.last().unwrap().label(),
                    ),
                    ("Tools", "Map Shot", Cmd::MapShot, "Merge Map Files…"),
                    ("View", "Previous Sheet", Cmd::PreviousSheet, "Collapse All"),
                    ("Help", "Quick Start", Cmd::QuickStart, "About Thinoda"),
                ] {
                    h.get_by_label(thinoda_app::i18n::t("Main Menu")).click();
                    h.run_steps(3);
                    reveal(&mut h, t(category));
                    h.get_by_role_and_label(Role::Button, t(category)).click();
                    h.run_steps(3);
                    reveal(&mut h, t(last));
                    let label = ThinodaApp::shortcut_hint(&h.ctx, &command).map_or_else(
                        || t(first).to_owned(),
                        |hint| format!("{} {hint}", t(first)),
                    );
                    reveal(&mut h, &label);
                    h.key_press(egui::Key::Escape);
                    h.run_steps(3);
                }
                h.get_by_label(thinoda_app::i18n::t("Main Menu")).click();
                h.run_steps(3);
                reveal(&mut h, t("File"));
                h.get_by_role_and_label(Role::Button, t("File")).click();
                h.run_steps(3);
                let quit = format!(
                    "{} {}",
                    t("Quit"),
                    ThinodaApp::shortcut_hint(&h.ctx, &Cmd::Quit).unwrap()
                );
                reveal(&mut h, &quit);
                let templates = t("New from Template…");
                reveal(&mut h, templates);
                h.get_by_role_and_label(Role::Button, templates).click();
                h.run_steps(3);
                let category = thinoda_core::templates::TemplateCategory::ALL[0];
                reveal(&mut h, t(category.label()));
                h.get_by_role_and_label(Role::Button, t(category.label()))
                    .click();
                h.run_steps(3);
                let template = thinoda_core::templates::TEMPLATES
                    .iter()
                    .rfind(|template| template.category == category)
                    .unwrap();
                reveal(&mut h, t(template.name));
                h.key_press(egui::Key::Escape);
                h.run_steps(2);
                if h.query_by_role_and_label(Role::Button, t("Edit")).is_none() {
                    h.get_by_label(thinoda_app::i18n::t("Main Menu")).click();
                    h.run_steps(3);
                }
                reveal(&mut h, t("Edit"));
                h.get_by_role_and_label(Role::Button, t("Edit")).click();
                h.run_steps(3);
                reveal(&mut h, t("Arrange"));
                h.get_by_role_and_label(Role::Button, t("Arrange")).click();
                h.run_steps(3);
                for label in ["Distribute Vertical Centers", "Align Left"] {
                    reveal(&mut h, t(label));
                }
                h.key_press(egui::Key::Escape);
                h.run_steps(3);
                h.get_by_label(thinoda_app::i18n::t("Main Menu")).click();
                h.run_steps(3);
                reveal(&mut h, t("Edit"));
                h.get_by_role_and_label(Role::Button, t("Edit")).click();
                h.run_steps(3);
                let undo = format!(
                    "{} {}",
                    t("Undo"),
                    ThinodaApp::shortcut_hint(&h.ctx, &Cmd::Canvas(thinoda_canvas::Action::Undo))
                        .unwrap()
                );
                reveal(&mut h, &undo);
                h.get_by_role_and_label(Role::Button, &settings_label)
                    .focus();
                h.run_steps(8);
                let rect = h
                    .get_by_role_and_label(Role::Button, &settings_label)
                    .rect();
                assert!(
                    h.ctx.content_rect().contains_rect(rect),
                    "Focused Settings out of view: {rect:?}"
                );
                let settings_focus = h.ctx.memory(|memory| memory.focused()).unwrap();
                reveal(&mut h, &undo);
                h.key_press(egui::Key::ArrowUp);
                h.run_steps(8);
                let previous_focus = h.ctx.memory(|memory| memory.focused()).unwrap();
                assert_ne!(previous_focus, settings_focus);
                let previous_rect = h.ctx.read_response(previous_focus).unwrap().rect;
                assert!(h.ctx.content_rect().contains_rect(previous_rect));
                h.key_press(egui::Key::ArrowDown);
                h.run_steps(8);
                assert_eq!(
                    h.ctx.memory(|memory| memory.focused()),
                    Some(settings_focus)
                );
                let rect = h
                    .get_by_role_and_label(Role::Button, &settings_label)
                    .rect();
                assert!(h.ctx.content_rect().contains_rect(rect));
                h.key_press(egui::Key::Enter);
                h.run_steps(3);
                assert!(h.state().dialogs.settings);
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                assert_eq!(h.state().doc().unwrap().view.selection, selection);
            }
        }
    }

    // Keep language-sensitive menu tests together because the app locale is global.
    let categories = [
        "File", "Edit", "Sheet", "Insert", "AI", "Tools", "View", "Help",
    ];
    let mut app = ThinodaApp::headless();
    app.settings.show_format_panel = false;
    let mut wide = Harness::builder()
        .with_size(egui::vec2(1600.0, 720.0))
        .build_ui_state(
            |ui, app: &mut ThinodaApp| {
                ui.ctx().set_os(OperatingSystem::Nix);
                app.draw(ui);
            },
            app,
        );
    wide.run_steps(4);
    for category in categories {
        wide.get_by_role_and_label(Role::Button, t(category));
    }
    wide.get_by_role_and_label(Role::Button, t("Edit")).click();
    wide.run_steps(3);
    let settings = format!(
        "{} {}",
        t("Settings…"),
        ThinodaApp::shortcut_hint(&wide.ctx, &Cmd::Settings).unwrap()
    );
    wide.get_by_role_and_label(Role::Button, &settings).click();
    wide.run_steps(3);
    assert!(wide.state().dialogs.settings);

    let mut app = ThinodaApp::headless();
    app.settings.show_format_panel = false;
    let mut narrow = Harness::builder()
        .with_size(egui::vec2(640.0, 360.0))
        .build_ui_state(
            |ui, app: &mut ThinodaApp| {
                ui.ctx().set_os(OperatingSystem::Nix);
                app.draw(ui);
            },
            app,
        );
    narrow.run_steps(4);
    narrow
        .get_by_label(thinoda_app::i18n::t("Main Menu"))
        .click();
    narrow.run_steps(3);
    for category in categories {
        narrow.get_by_role_and_label(Role::Button, t(category));
    }
}
