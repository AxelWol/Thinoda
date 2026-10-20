//! Browser-only pointer feedback for both egui and custom-drawn controls.

use egui::{Context, CursorIcon};

pub fn install(ctx: &Context) {
    ctx.on_end_pass(
        "web hover cursor",
        std::sync::Arc::new(|ui| update(ui.ctx())),
    );
}

fn update(ctx: &Context) {
    // Specialized cursors (text, resize, grab, etc.) take precedence.
    if ctx.output(|output| output.cursor_icon != CursorIcon::Default) {
        return;
    }
    let hovered = ctx.interaction_snapshot(|interaction| interaction.hovered.clone());
    let clickable = hovered.into_iter().any(|id| {
        ctx.read_response(id).is_some_and(|response| {
            response.enabled()
                && response.hovered()
                && response.sense.senses_click()
                // The canvas and text editors also sense clicks, but are not buttons.
                && !response.sense.senses_drag()
        })
    });
    if clickable {
        ctx.set_cursor_icon(CursorIcon::PointingHand);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Color32, Role, Sense, Theme, vec2};
    use egui_kittest::{Harness, kittest::Queryable};
    use thinoda_ui::{ThemeMode, widgets};

    fn harness(web: bool, theme: ThemeMode) -> Harness<'static, ()> {
        Harness::builder()
            .with_size(vec2(800.0, 1000.0))
            .with_context_setup(move |ctx| {
                thinoda_ui::install(ctx, theme, Color32::BLUE);
                if web {
                    install(ctx);
                }
            })
            .build_ui(|ui| {
                let _ = ui.button("Button");
                ui.add_enabled(false, egui::Button::new("Disabled button"));
                widgets::segmented(ui, &mut false, &[(false, "Style"), (true, "Map")]);
                widgets::icon_button(ui, "+", "Icon button", false);
                widgets::toolbar_button(ui, "+", "Toolbar button", true);
                widgets::toolbar_button(ui, "+", "Disabled toolbar button", false);
                widgets::toggle(ui, &mut false, "Toggle");
                widgets::preview_card(ui, "Preview", false, 120.0, 40.0, |_, _| {});
                widgets::chip(ui, "Chip", Color32::BLUE);
                widgets::color_swatches(ui, &[Color32::RED], None);
                ui.checkbox(&mut false, "Checkbox");
                let _ = ui.selectable_label(false, "Selectable");
                egui::ComboBox::from_label("Combo")
                    .selected_text("Choose")
                    .show_ui(ui, |ui| {
                        let _ = ui.button("Popup option");
                    });
                widgets::section(ui, "Section", false, |_| {});
                ui.add(egui::Label::new("Plain label").selectable(false));
                ui.add(egui::TextEdit::singleline(&mut String::new()))
                    .accessible_name("Text");
                let (_, drag) = ui.allocate_exact_size(vec2(120.0, 30.0), Sense::click_and_drag());
                drag.widget_info(|| egui::WidgetInfo::labeled(Role::Button, true, "Canvas"));
                let (_, resize) = ui.allocate_exact_size(vec2(120.0, 30.0), Sense::click());
                resize.widget_info(|| egui::WidgetInfo::labeled(Role::Button, true, "Resize"));
                resize.on_hover_cursor(CursorIcon::ResizeHorizontal);
                let (_, grab) = ui.allocate_exact_size(vec2(120.0, 30.0), Sense::click_and_drag());
                grab.widget_info(|| egui::WidgetInfo::labeled(Role::Button, true, "Grab"));
                grab.on_hover_cursor(CursorIcon::Grab);
                ui.add_enabled_ui(false, |ui| {
                    widgets::segmented(ui, &mut false, &[(false, "Disabled tab")]);
                    widgets::icon_button(ui, "+", "Disabled icon", false);
                    widgets::toggle(ui, &mut false, "Disabled toggle");
                });
            })
    }

    #[test]
    fn web_clickable_controls_use_pointer_in_both_themes() {
        for theme in [ThemeMode::Light, ThemeMode::Dark] {
            let mut h = harness(true, theme);
            for label in [
                "Button",
                "Style",
                "Map",
                "Icon button",
                "Toolbar button",
                "Toggle",
                "Preview",
                "Chip",
                "#FF0000",
                "Checkbox",
                "Selectable",
                "Combo",
                "Section",
            ] {
                h.get_by_label(label).hover();
                h.run_steps(3);
                assert_eq!(
                    h.output().platform_output.cursor_icon,
                    CursorIcon::PointingHand,
                    "{theme:?}: {label}"
                );
            }
        }
    }

    #[test]
    fn web_pointer_clears_on_disabled_controls_plain_content_and_pointer_exit() {
        let mut h = harness(true, ThemeMode::Light);
        for label in [
            "Disabled button",
            "Disabled toolbar button",
            "Disabled tab",
            "Disabled icon",
            "Disabled toggle",
            "Plain label",
            "Canvas",
        ] {
            h.get_by_label("Button").hover();
            h.run_steps(3);
            assert_eq!(
                h.output().platform_output.cursor_icon,
                CursorIcon::PointingHand
            );
            h.get_by_label(label).hover();
            h.run_steps(3);
            assert_eq!(
                h.output().platform_output.cursor_icon,
                CursorIcon::Default,
                "{label}"
            );
        }
        h.get_by_label("Button").hover();
        h.run_steps(3);
        h.input_mut()
            .events
            .push(egui::Event::PointerMoved(egui::pos2(790.0, 990.0)));
        h.run_steps(3);
        assert_eq!(h.output().platform_output.cursor_icon, CursorIcon::Default);
        h.get_by_label("Button").hover();
        h.run_steps(3);
        h.input_mut().events.push(egui::Event::PointerGone);
        h.run_steps(3);
        assert_eq!(h.output().platform_output.cursor_icon, CursorIcon::Default);
    }

    #[test]
    fn web_preserves_specialized_cursors() {
        let mut h = harness(true, ThemeMode::Light);
        for (label, cursor) in [
            ("Text", CursorIcon::Text),
            ("Resize", CursorIcon::ResizeHorizontal),
            ("Grab", CursorIcon::Grab),
        ] {
            h.get_by_label(label).hover();
            h.run_steps(3);
            assert_eq!(h.output().platform_output.cursor_icon, cursor, "{label}");
        }
    }

    #[test]
    fn web_policy_survives_theme_reinstallation_and_handles_popups() {
        let mut h = harness(true, ThemeMode::Light);
        thinoda_ui::install(&h.ctx, ThemeMode::Dark, Color32::RED);
        h.run_steps(3);
        assert_eq!(h.ctx.theme(), Theme::Dark);
        h.get_by_label("Map").hover();
        h.run_steps(3);
        assert_eq!(
            h.output().platform_output.cursor_icon,
            CursorIcon::PointingHand
        );
        h.get_by_label("Combo").click();
        h.run_steps(3);
        h.get_by_label("Popup option").hover();
        h.run_steps(3);
        assert_eq!(
            h.output().platform_output.cursor_icon,
            CursorIcon::PointingHand
        );
    }

    #[test]
    fn shared_desktop_controls_keep_the_default_cursor() {
        let mut h = harness(false, ThemeMode::Light);
        for label in [
            "Button",
            "Style",
            "Map",
            "Icon button",
            "Toolbar button",
            "Toggle",
        ] {
            h.get_by_label(label).hover();
            h.run_steps(3);
            assert_eq!(
                h.output().platform_output.cursor_icon,
                CursorIcon::Default,
                "{label}"
            );
        }
    }

    #[test]
    fn web_does_not_show_pointer_for_controls_behind_a_modal() {
        let mut h = Harness::builder()
            .with_context_setup(install)
            .build_ui_state(
                |ui, open: &mut bool| {
                    let _ = ui.button("Behind modal");
                    if *open {
                        egui::Modal::new(egui::Id::unique("modal")).show(ui.ctx(), |ui| {
                            ui.label("Modal content");
                        });
                    }
                },
                false,
            );
        h.get_by_label("Behind modal").hover();
        h.run_steps(3);
        assert_eq!(
            h.output().platform_output.cursor_icon,
            CursorIcon::PointingHand
        );
        *h.state_mut() = true;
        h.run_steps(3);
        assert_eq!(h.output().platform_output.cursor_icon, CursorIcon::Default);
    }
}
