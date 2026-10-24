use crate::{
    app::{CenterMode, Cmd, ThinodaApp},
    i18n::t,
};
use thinoda_canvas::Action;

#[derive(Default)]
pub struct ShortcutBrowser {
    pub query: String,
    pub category: Option<String>,
    pub unassigned_only: bool,
    reset_scroll: bool,
}

impl ThinodaApp {
    pub fn shortcut_reference(&self, ctx: &egui::Context) -> Vec<thinoda_ui::Command> {
        let bindings: Vec<_> = Self::contextual_shortcut_bindings_for_mode(self.mode)
            .into_iter()
            .filter(|(shortcut, action)| {
                !Self::default_shortcut_command(self.mode, ctx.os(), *shortcut)
                    .is_some_and(|command| command != Cmd::Canvas(action.clone()))
            })
            .collect();
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
        if self.mode == CenterMode::Map {
            commands.push(
                thinoda_ui::Command::new("map-cancel", t("Cancel"))
                    .category(t("Navigation"))
                    .shortcut(ctx.format_shortcut(&egui::KeyboardShortcut::new(
                        egui::Modifiers::NONE,
                        egui::Key::Escape,
                    ))),
            );
            for (id, title, command) in [
                ("map-cut", "Cut", Cmd::Canvas(Action::Cut)),
                ("map-copy", "Copy", Cmd::Canvas(Action::Copy)),
                ("map-paste", "Paste", Cmd::MenuPaste),
            ] {
                let mut metadata = thinoda_ui::Command::new(id, t(title)).category(t("Edit"));
                metadata.shortcut = Self::shortcut_hint_for_mode(ctx, &command, self.mode);
                commands.push(metadata);
            }
        }
        commands
    }

    pub(crate) fn shortcuts_dialog(&mut self, ctx: &egui::Context) {
        if !self.dialogs.shortcuts {
            return;
        }
        let touch_allowed = thinoda_ui::widgets::touch_activation_allowed(ctx);
        if !egui::Popup::is_any_open(ctx)
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.dialogs.shortcuts = false;
            return;
        }
        let commands = self.shortcut_reference(ctx);
        let scope = t(match self.mode {
            CenterMode::Map => "Mind Map",
            CenterMode::Outliner => "Outliner",
        });
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
                            ui.label(format!("{}: {scope}", t("View")));
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
                            if !touch_allowed {
                                (state.category, state.unassigned_only) = previous.clone();
                            }
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
                    if ui.put(close_rect, egui::Button::new(close_label)).clicked() && touch_allowed {
                        close = true;
                    }
                });
            });
        if close || (response.should_close() && touch_allowed) {
            self.dialogs.shortcuts = false;
        }
    }
}
