use crate::{
    app::{Cmd, ThinodaApp},
    i18n::t,
};
use thinoda_canvas::Action;

pub(crate) fn platform_hint(ctx: &egui::Context, shortcut: &str) -> String {
    if ctx.os().is_mac() {
        shortcut.replace("Ctrl+", "Cmd+").replace("Alt+", "Option+")
    } else {
        shortcut.to_owned()
    }
}

#[derive(Default)]
pub struct ShortcutBrowser {
    pub query: String,
    pub category: Option<String>,
    pub unassigned_only: bool,
    reset_scroll: bool,
}

impl ThinodaApp {
    pub fn shortcut_reference(&self, ctx: &egui::Context) -> Vec<thinoda_ui::Command> {
        let bindings = Self::contextual_shortcut_bindings();
        let hint = |actions: &[Action]| {
            bindings
                .iter()
                .filter(|(_, action)| actions.contains(action))
                .map(|(shortcut, _)| ctx.format_shortcut(shortcut))
                .collect::<Vec<_>>()
                .join(" / ")
        };
        let mut commands: Vec<_> = self
            .command_catalog(ctx)
            .into_iter()
            .map(|(command, mut metadata)| {
                if let Cmd::Canvas(action) = command {
                    let aliases = hint(&[action]);
                    if !aliases.is_empty() {
                        metadata.shortcut = Some(aliases);
                    }
                }
                metadata
            })
            .collect();
        use thinoda_core::navigation::Direction;
        for (id, title, category, actions) in [
            ("map-edit", "Edit", "Navigation", vec![Action::StartEdit]),
            (
                "map-navigate",
                "Navigate",
                "Navigation",
                vec![
                    Action::Navigate(Direction::Up),
                    Action::Navigate(Direction::Down),
                    Action::Navigate(Direction::Left),
                    Action::Navigate(Direction::Right),
                ],
            ),
            (
                "map-priority-1",
                "Priority 1",
                "Insert",
                vec![Action::ToggleMarker("priority-1".into())],
            ),
            (
                "map-priority-2",
                "Priority 2",
                "Insert",
                vec![Action::ToggleMarker("priority-2".into())],
            ),
            (
                "map-priority-3",
                "Priority 3",
                "Insert",
                vec![Action::ToggleMarker("priority-3".into())],
            ),
        ] {
            commands.push(
                thinoda_ui::Command::new(id, t(title))
                    .category(t(category))
                    .shortcut(hint(&actions)),
            );
        }
        commands.push(
            thinoda_ui::Command::new("map-cancel", t("Cancel"))
                .category(t("Navigation"))
                .shortcut(ctx.format_shortcut(&egui::KeyboardShortcut::new(
                    egui::Modifiers::NONE,
                    egui::Key::Escape,
                ))),
        );
        commands
    }

    pub(crate) fn shortcuts_dialog(&mut self, ctx: &egui::Context) {
        if !self.dialogs.shortcuts {
            return;
        }
        if !egui::Popup::is_any_open(ctx)
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.dialogs.shortcuts = false;
            return;
        }
        let commands = self.shortcut_reference(ctx);
        let categories: std::collections::BTreeSet<_> = commands
            .iter()
            .filter_map(|command| command.category.clone())
            .collect();
        let state = &mut self.dialogs.shortcut_browser;
        let mut close = false;
        let frame = egui::Frame::popup(&ctx.global_style());
        let response = egui::Modal::new(egui::Id::unique("shortcuts"))
            .frame(frame)
            .show(ctx, |ui| {
                let available = (ctx.content_rect().size()
                    - frame.total_margin().sum()
                    - egui::Vec2::splat(16.0))
                .max(egui::Vec2::ZERO);
                ui.set_width(available.x.min(640.0));
                ui.set_height(available.y.min(640.0));
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    let close_label = egui::WidgetText::from(t("Close"));
                    let close_text = close_label.clone().into_galley(
                        ui,
                        Some(egui::TextWrapMode::Extend),
                        f32::INFINITY,
                        egui::TextStyle::Button,
                    );
                    let close_size = (close_text.size() + ui.spacing().button_padding * 2.0)
                        .max(ui.spacing().interact_size);
                    // Reserve the footer first, but register its button last for natural Tab order.
                    let (_, close_rect) = ui.allocate_space(close_size);
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        let mut scroll = egui::ScrollArea::vertical()
                            .id_salt("shortcut_reference")
                            .min_scrolled_height(0.0)
                            .max_height(ui.available_height())
                            .auto_shrink([false, false]);
                        if std::mem::take(&mut state.reset_scroll) {
                            scroll = scroll.vertical_scroll_offset(0.0);
                        }
                        scroll.show(ui, |ui| {
                            ui.heading(t("Keyboard Shortcuts"));
                            let search = ui
                                .add(
                                    egui::TextEdit::singleline(&mut state.query)
                                        .hint_text(t("Search commands or shortcuts")),
                                )
                                .accessible_name(t("Search commands or shortcuts"));
                            let previous = (state.category.clone(), state.unassigned_only);
                            ui.horizontal_wrapped(|ui| {
                                egui::ComboBox::from_id_salt("shortcut_category")
                                    .selected_text(state.category.as_deref().unwrap_or(t("All")))
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut state.category, None, t("All"));
                                        for category in categories {
                                            ui.selectable_value(
                                                &mut state.category,
                                                Some(category.clone()),
                                                category,
                                            );
                                        }
                                    })
                                    .response
                                    .accessible_name(t("Shortcut category"));
                                ui.checkbox(&mut state.unassigned_only, t("Unassigned only"));
                            });
                            let query = state.query.trim().to_lowercase();
                            let entries: Vec<_> = commands
                                .iter()
                                .filter(|command| {
                                    state.category.as_ref().is_none_or(|category| {
                                        command.category.as_ref() == Some(category)
                                    }) && (!state.unassigned_only || command.shortcut.is_none())
                                        && format!(
                                            "{} {} {}",
                                            command.title,
                                            command.category.as_deref().unwrap_or(""),
                                            command.shortcut.as_deref().unwrap_or(t("Unassigned"))
                                        )
                                        .to_lowercase()
                                        .contains(&query)
                                })
                                .collect();
                            ui.label(format!("{}: {}", t("Commands"), entries.len()));
                            state.reset_scroll = search.changed()
                                || previous != (state.category.clone(), state.unassigned_only);
                            if entries.is_empty() {
                                ui.label(t("No matching commands"));
                            }
                            for command in entries {
                                ui.push_id(&command.id, |ui| {
                                    ui.group(|ui| {
                                        ui.set_width(ui.available_width() - 8.0);
                                        ui.label(&command.title);
                                        ui.horizontal_wrapped(|ui| {
                                            ui.monospace(
                                                command
                                                    .shortcut
                                                    .as_deref()
                                                    .unwrap_or(t("Unassigned")),
                                            );
                                            if let Some(category) = &command.category {
                                                ui.weak(category);
                                            }
                                        });
                                    });
                                });
                            }
                            ui.label(t(
                                "This is a reference list. Shortcut rebinding is not available yet.",
                            ));
                            crate::ui::reveal_changed_focus(ui);
                        });
                    });
                    if ui.put(close_rect, egui::Button::new(close_label)).clicked() {
                        close = true;
                    }
                });
            });
        if close || response.should_close() {
            self.dialogs.shortcuts = false;
        }
    }
}
