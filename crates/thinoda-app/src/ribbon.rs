//! Windows command ribbon uses the same hierarchy and handlers as native menus.

use crate::{
    app::{Cmd, ThinodaApp},
    i18n::t,
    menus::MenuEntry,
};
use egui::{Key, Modifiers, Ui};
use thinoda_ui::{tokens::Palette, widgets};

#[derive(Default)]
pub(crate) struct RibbonState {
    tab: usize,
    temporary: bool,
    snapshot: Option<crate::native_menus::MenuSnapshot>,
    rect: Option<egui::Rect>,
    pending: Option<(crate::native_menus::MenuSnapshot, Cmd)>,
}

#[cfg(test)]
#[path = "ribbon_tests.rs"]
mod tests;

fn command_groups<'a>(
    entries: &'a [MenuEntry],
    category: &str,
) -> Vec<(String, Vec<&'a MenuEntry>)> {
    use thinoda_canvas::Action;
    let mut groups: Vec<(String, Vec<&MenuEntry>)> = Vec::new();
    for entry in entries {
        let label = match entry {
            MenuEntry::Command { command, .. } => match command {
                Cmd::New | Cmd::Open => t("Open…"),
                Cmd::Save | Cmd::SaveAs | Cmd::SaveAll => t("Save"),
                Cmd::Import | Cmd::Print | Cmd::ShowInFolder => t("Export"),
                Cmd::Canvas(Action::Undo | Action::Redo) => t("Undo"),
                Cmd::Canvas(Action::Copy | Action::Cut | Action::Duplicate | Action::Delete)
                | Cmd::MenuPaste => t("Copy"),
                Cmd::Canvas(Action::SelectAll) | Cmd::Find | Cmd::FindReplace | Cmd::Palette => {
                    t("Select")
                }
                Cmd::Canvas(Action::Indent | Action::Outdent) => t("Arrange"),
                Cmd::CopyStyle | Cmd::PasteStyle | Cmd::ResetStyle | Cmd::SaveQuickStyle => {
                    t("Copy Style")
                }
                Cmd::EditField(_) | Cmd::Labels => t("Notes"),
                Cmd::Canvas(
                    Action::StartRelationship
                    | Action::AddSummary
                    | Action::AddBoundary
                    | Action::ToggleTodo,
                ) => t("Relationship"),
                Cmd::ToggleOutliner
                | Cmd::ToggleZen
                | Cmd::ToggleFormatPanel
                | Cmd::ToggleSymbolsPanel
                | Cmd::ToggleStatusBar => t("View"),
                Cmd::Canvas(Action::ZoomIn | Action::ZoomOut | Action::ZoomReset | Action::Fit) => {
                    t("Zoom In")
                }
                Cmd::Canvas(Action::ExpandAll | Action::CollapseAll) => t("Expand All"),
                Cmd::NextSheet
                | Cmd::PreviousSheet
                | Cmd::NewSheet
                | Cmd::RenameSheet
                | Cmd::DuplicateSheet
                | Cmd::CopySheet
                | Cmd::PasteSheet
                | Cmd::SaveSheetAs
                | Cmd::DeleteSheet => t("Sheet"),
                _ => category,
            },
            MenuEntry::Submenu { label, .. }
                if label == t("Open Recent") || label == t("New from Template…") =>
            {
                t("Open…")
            }
            MenuEntry::Submenu { label, .. } if label == t("Export") => t("Export"),
            MenuEntry::Submenu { label, .. } if label == t("Select") => t("Select"),
            MenuEntry::Submenu { label, .. } if label == t("Arrange") => t("Arrange"),
            MenuEntry::Separator => continue,
            MenuEntry::Caption(_) => groups.last().map_or(category, |(label, _)| label.as_str()),
            _ => category,
        }
        .to_owned();
        if let Some((_, items)) = groups.iter_mut().find(|(name, _)| name == &label) {
            items.push(entry);
        } else {
            groups.push((label, vec![entry]));
        }
    }
    groups
}

impl ThinodaApp {
    fn ribbon_entry(
        &mut self,
        ui: &mut Ui,
        entry: &MenuEntry,
        snapshot: &crate::native_menus::MenuSnapshot,
        command: &mut Option<Cmd>,
    ) {
        match entry {
            MenuEntry::Command {
                label,
                command: cmd,
            } => {
                let response = ui.add_enabled(
                    snapshot.command_enabled(cmd),
                    egui::Button::new(label).truncate(),
                );
                let hint = Self::shortcut_hint_for_mode(ui.ctx(), cmd, self.mode);
                let response = response.on_hover_text(
                    hint.map_or_else(|| label.clone(), |hint| format!("{label} ({hint})")),
                );
                if response.clicked() && widgets::touch_activation_allowed(ui.ctx()) {
                    *command = Some(cmd.clone());
                }
            }
            MenuEntry::Submenu { label, entries } => {
                ui.add_enabled_ui(self.menu_entries_available(entries), |ui| {
                    crate::ui::guarded_menu_button(ui, label, |ui| {
                        self.render_menu_entries(ui, entries, command);
                    });
                });
            }
            MenuEntry::Caption(text) => {
                ui.add(egui::Label::new(text).truncate());
            }
            MenuEntry::Separator | MenuEntry::ClearRecent => {
                self.render_menu_entries(ui, std::slice::from_ref(entry), command);
            }
        }
    }

    pub fn show_ribbon(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if !ctx.input(|input| input.pointer.any_pressed() || input.pointer.any_released())
            && let Some((snapshot, command)) = self.ribbon.pending.take()
        {
            self.dispatch_ribbon_command(&ctx, &snapshot, &command);
        }
        let groups = self.main_menu_entries();
        let mut snapshot = self.native_menu_snapshot(&ctx);
        // egui clears text focus on pointer press before the ribbon is drawn.
        // Keep the producing text owner through this ribbon pointer sequence.
        if !snapshot.has_text_focus()
            && let Some(previous) = &self.ribbon.snapshot
            && previous.has_text_focus()
            && ctx.input(|input| {
                (input.pointer.any_down() || input.pointer.any_released())
                    && input
                        .pointer
                        .interact_pos()
                        .is_some_and(|pos| self.ribbon.rect.is_some_and(|rect| rect.contains(pos)))
            })
        {
            snapshot = previous.clone();
        }
        self.ribbon.snapshot = Some(snapshot.clone());
        let blocked = self.any_modal_open()
            || self.dialogs.settings
            || self.dialogs.about
            || self.palette.open
            || self.launch.is_some();
        let mut selected = None;
        if !blocked && !ctx.text_edit_focused() {
            if ctx.input_mut(|input| input.consume_key(Modifiers::CTRL, Key::F1)) {
                self.settings.ribbon_collapsed = !self.settings.ribbon_collapsed;
                self.ribbon.temporary = false;
                self.save_settings();
            }

            for (index, key) in [
                Key::Num1,
                Key::Num2,
                Key::Num3,
                Key::Num4,
                Key::Num5,
                Key::Num6,
                Key::Num7,
                Key::Num8,
            ]
            .into_iter()
            .enumerate()
            {
                if ctx.input_mut(|input| input.consume_key(Modifiers::ALT, key)) {
                    selected = Some(index);
                }
            }
            if self.ribbon.temporary
                && !egui::Popup::is_any_open(&ctx)
                && ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape))
            {
                self.ribbon.temporary = false;
            }
        }
        if let Some(tab) = selected {
            self.ribbon.tab = tab;
            self.ribbon.temporary = self.settings.ribbon_collapsed;
        }
        let room = ctx.content_rect().height() >= 420.0;
        let expanded = room && (!self.settings.ribbon_collapsed || self.ribbon.temporary);
        let mut command: Option<Cmd> = None;
        if !room {
            let panel = egui::Panel::top("thinoda_ribbon_compact")
                .exact_size(38.0)
                .show(ui, |ui| {
                    ui.add_enabled_ui(!blocked, |ui| {
                        crate::ui::guarded_menu_button(ui, t("Ribbon"), |ui| {
                            self.render_menu_entries(ui, &groups, &mut command);
                        });
                    });
                });
            self.ribbon.rect = Some(panel.response.rect);
            if let Some(command) = command {
                self.ribbon.pending = Some((snapshot, command));
                ctx.request_repaint();
            }
            return;
        }
        let panel = egui::Panel::top("thinoda_ribbon")
            .exact_size(if expanded { 166.0 } else { 38.0 })
            .frame(
                egui::Frame::new()
                    .fill(Palette::of(&ctx).surface)
                    .inner_margin(egui::Margin::symmetric(8, 4)),
            )
            .show(ui, |ui| {
                ui.add_enabled_ui(!blocked, |ui| {
                    ui.horizontal(|ui| {
                        let label = if self.settings.ribbon_collapsed {
                            t("Expand Ribbon")
                        } else {
                            t("Collapse Ribbon")
                        };
                        if ui.button(label).on_hover_text("Ctrl+F1").clicked()
                            && widgets::touch_activation_allowed(&ctx)
                        {
                            self.settings.ribbon_collapsed = !self.settings.ribbon_collapsed;
                            self.ribbon.temporary = false;
                            self.save_settings();
                        }
                        egui::ScrollArea::horizontal()
                            .id_salt("ribbon_tabs")
                            .scroll_bar_visibility(
                                egui::scroll_area::ScrollBarVisibility::AlwaysHidden,
                            )
                            .max_height(28.0)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    for (index, group) in groups.iter().enumerate() {
                                        let MenuEntry::Submenu { label, .. } = group else {
                                            continue;
                                        };
                                        if ui
                                            .selectable_label(self.ribbon.tab == index, label)
                                            .on_hover_text(format!("Alt+{}", index + 1))
                                            .clicked()
                                            && widgets::touch_activation_allowed(&ctx)
                                        {
                                            self.ribbon.tab = index;
                                            self.ribbon.temporary = self.settings.ribbon_collapsed;
                                        }
                                    }
                                    crate::ui::reveal_changed_focus(ui);
                                });
                            });
                    });
                    if expanded {
                        ui.separator();
                        if let Some(MenuEntry::Submenu { label, entries }) =
                            groups.get(self.ribbon.tab)
                        {
                            egui::ScrollArea::horizontal()
                                .id_salt(("ribbon_commands", self.ribbon.tab))
                                .max_height(122.0)
                                .show(ui, |ui| {
                                    ui.horizontal_top(|ui| {
                                        for (index, (label, items)) in
                                            command_groups(entries, label).into_iter().enumerate()
                                        {
                                            ui.push_id(index, |ui| {
                                                ui.vertical(|ui| {
                                                    ui.label(
                                                        egui::RichText::new(label).small().strong(),
                                                    );
                                                    ui.horizontal_top(|ui| {
                                                        for chunk in items.chunks(3) {
                                                            ui.vertical(|ui| {
                                                                ui.set_width(196.0);
                                                                ui.set_max_width(196.0);
                                                                for entry in chunk {
                                                                    self.ribbon_entry(
                                                                        ui,
                                                                        entry,
                                                                        &snapshot,
                                                                        &mut command,
                                                                    );
                                                                }
                                                            });
                                                        }
                                                    });
                                                });
                                            });
                                            ui.separator();
                                        }
                                    });
                                });
                        }
                    }
                });
            });
        self.ribbon.rect = Some(panel.response.rect);
        if self.settings.ribbon_collapsed
            && self.ribbon.temporary
            && !egui::Popup::is_any_open(&ctx)
            && ctx.input(|input| {
                input.pointer.any_pressed()
                    && input
                        .pointer
                        .interact_pos()
                        .is_some_and(|pos| !panel.response.rect.contains(pos))
            })
        {
            self.ribbon.temporary = false;
        }
        if let Some(command) = command {
            self.ribbon.temporary = false;
            self.ribbon.pending = Some((snapshot, command));
            ctx.request_repaint();
        }
    }
}
