//! Locale changes are process-wide; keep this matrix sequential.

use egui::{Event, PointerButton, accesskit::Role};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use thinoda_app::{
    app::{Cmd, Services, ThinodaApp},
    i18n::t,
};
use thinoda_platform::settings::{Language, Settings, ThemeMode};

mod support;
use support::menu_action;

fn right_click(h: &mut Harness<'_, ThinodaApp>, pos: egui::Pos2) {
    h.event(Event::PointerMoved(pos));
    for pressed in [true, false] {
        h.event(Event::PointerButton {
            pos,
            button: PointerButton::Secondary,
            pressed,
            modifiers: Default::default(),
        });
        h.run_steps(1);
    }
    h.run_steps(4);
}

fn assert_popups_fit(h: &Harness<'_, ThinodaApp>) {
    let viewport = h.ctx.content_rect().expand(0.5 / h.ctx.pixels_per_point());
    h.ctx.memory(|memory| {
        for layer in memory
            .layer_ids()
            .filter(|layer| layer.order == egui::Order::Foreground)
        {
            if let Some(rect) = memory.area_rect(layer.id) {
                assert!(
                    viewport.contains_rect(rect),
                    "Popup outside viewport: {rect:?}"
                );
            }
        }
    });
}

fn reveal(h: &mut Harness<'_, ThinodaApp>, label: &str) {
    menu_action(h, label).scroll_to_me();
    h.run_steps(8);
    let rect = menu_action(h, label).rect();
    assert!(
        h.ctx.content_rect().contains_rect(rect),
        "{label} outside viewport: {rect:?}"
    );
    assert_popups_fit(h);
}

#[test]
fn context_actions_fit_short_localized_windows_without_losing_commands() {
    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System] {
            for (width, height, scale) in [
                (640.0, 360.0, 1.0),
                (960.0, 540.0, 1.5),
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
                app.settings.ai.enabled = true;
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
                let pos = {
                    let d = h.state().doc().unwrap();
                    d.view.selected_topic_rect(&d.doc).unwrap().center()
                };
                right_click(&mut h, pos);
                for label in ["Fit Map", "Topic", "Delete", "Paste Style", "Arrange"] {
                    reveal(&mut h, t(label));
                }
                h.get_by_role_and_label(Role::Button, t("Arrange")).click();
                h.run_steps(3);
                reveal(&mut h, t("Distribute Vertical Centers"));
                reveal(&mut h, t("Align Left"));
                h.key_press(egui::Key::Escape);
                h.run_steps(3);
                assert_eq!(h.state().doc().unwrap().view.selection, selection);
                right_click(&mut h, pos);
                let ai = t("AI").to_string();
                reveal(&mut h, &ai);
                h.get_by_role_and_label(Role::Button, &ai).click();
                h.run_steps(3);
                reveal(&mut h, t(thinoda_app::ai::AiCommand::GenerateMap.label()));
                h.key_press(egui::Key::Escape);
                h.run_steps(3);
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                assert_eq!(h.state().doc().unwrap().view.selection, selection);

                let title = h.state().doc().unwrap().doc.sheet().title.clone();
                let tab = h.get_by_label(&title).rect().center();
                right_click(&mut h, tab);
                reveal(&mut h, t("Delete Sheet"));
                reveal(&mut h, t("Rename Sheet"));
                menu_action(&h, t("New Sheet")).focus();
                h.run_steps(8);
                let rect = menu_action(&h, t("New Sheet")).rect();
                assert!(h.ctx.content_rect().contains_rect(rect));
                h.key_press(egui::Key::Enter);
                h.run_steps(3);
                assert_eq!(h.state().doc().unwrap().doc.workbook().sheets.len(), 2);
                let ctx = h.ctx.clone();
                h.state_mut()
                    .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                h.run_steps(3);
                let canvas = h.state().doc().unwrap().view.canvas_rect();
                for edge in [
                    canvas.left_top() + egui::vec2(8.0, 80.0),
                    canvas.right_bottom() - egui::vec2(8.0, 8.0),
                ] {
                    right_click(&mut h, edge);
                    reveal(&mut h, t("Fit Map"));
                    reveal(&mut h, t("Topic"));
                    assert!(menu_action(&h, t("Topic")).accesskit_node().is_disabled());
                    h.key_press(egui::Key::Escape);
                    h.run_steps(3);
                    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                }
            }
        }
    }
}
