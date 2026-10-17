use crate::{app::ThinodaApp, i18n::t};

#[derive(Default)]
pub struct ShortcutBrowser {
    pub query: String,
    pub category: Option<String>,
    pub unassigned_only: bool,
}

impl ThinodaApp {
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
        let mut commands: Vec<_> = Self::palette_commands()
            .into_iter()
            .map(|(_, command)| command)
            .collect();
        for (binding, title) in [
            ("Space / F2", "Edit"),
            ("Arrows", "Navigate"),
            ("Alt+↑/↓", "Move"),
            ("Esc", "Select none"),
            ("Home", "Central Topic"),
        ] {
            commands.push(
                thinoda_ui::Command::new(binding, t(title))
                    .category(t("Navigation"))
                    .shortcut(binding),
            );
        }
        let categories: std::collections::BTreeSet<_> = commands
            .iter()
            .filter_map(|command| command.category.clone())
            .collect();
        let state = &mut self.dialogs.shortcut_browser;
        let mut close = false;
        let response = egui::Modal::new(egui::Id::unique("shortcuts")).show(ctx, |ui| {
            ui.set_width(640.0_f32.min(ctx.content_rect().width() - 48.0).max(240.0));
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
                    state
                        .category
                        .as_ref()
                        .is_none_or(|category| command.category.as_ref() == Some(category))
                        && (!state.unassigned_only || command.shortcut.is_none())
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
            let height = (ctx.content_rect().height() - 300.0).max(100.0);
            let mut scroll = egui::ScrollArea::vertical()
                .min_scrolled_height(height)
                .max_height(height)
                .auto_shrink([false, false]);
            if search.changed() || previous != (state.category.clone(), state.unassigned_only) {
                scroll = scroll.vertical_scroll_offset(0.0);
            }
            scroll.show(ui, |ui| {
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
                                    command.shortcut.as_deref().unwrap_or(t("Unassigned")),
                                );
                                if let Some(category) = &command.category {
                                    ui.weak(category);
                                }
                            });
                        });
                    });
                }
            });
            ui.label(t(
                "This is a reference list. Shortcut rebinding is not available yet.",
            ));
            if ui.button(t("Close")).clicked() {
                close = true;
            }
        });
        if close || response.should_close() {
            self.dialogs.shortcuts = false;
        }
    }
}
