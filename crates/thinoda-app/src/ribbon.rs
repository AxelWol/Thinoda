//! Desktop command ribbon uses the same hierarchy and handlers as native menus.

use crate::{
    app::{Cmd, ThinodaApp},
    i18n::t,
    menus::MenuEntry,
};
use egui::{Key, Modifiers, Ui};
use thinoda_ui::ribbon::{Detail, Glyph, command_button};
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

use crate::command_icons::command_glyph;

fn entry_glyph(entry: &MenuEntry) -> Glyph {
    match entry {
        MenuEntry::Command { command, .. } => command_glyph(command),
        MenuEntry::Submenu { entries, .. } => entries.first().map_or(Glyph::Folder, entry_glyph),
        _ => Glyph::Chevron,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GroupDetail {
    Full,
    Compact,
    Menu,
}

fn group_width(detail: GroupDetail, count: usize) -> f32 {
    match detail {
        GroupDetail::Full => 84.0 + count.saturating_sub(1).div_ceil(3) as f32 * 146.0,
        GroupDetail::Compact => 84.0 + count.saturating_sub(1).div_ceil(3) as f32 * 34.0,
        GroupDetail::Menu => 78.0,
    }
}

fn group_details(counts: &[usize], available: f32) -> Vec<GroupDetail> {
    let mut details = vec![GroupDetail::Full; counts.len()];
    let width = |details: &[GroupDetail]| {
        details
            .iter()
            .zip(counts)
            .map(|(detail, count)| group_width(*detail, *count) + 12.0)
            .sum::<f32>()
    };
    for next in [GroupDetail::Compact, GroupDetail::Menu] {
        for index in (0..details.len()).rev() {
            if width(&details) <= available {
                return details;
            }
            details[index] = next;
        }
    }
    details
}

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
        detail: Detail,
        command: &mut Option<Cmd>,
    ) {
        match entry {
            MenuEntry::Command {
                label,
                command: cmd,
            } => {
                let (response, activated) = ui
                    .add_enabled_ui(snapshot.command_enabled(cmd), |ui| {
                        command_button(
                            ui,
                            command_glyph(cmd),
                            label,
                            detail,
                            snapshot.command_checked(cmd),
                        )
                    })
                    .inner;
                let hint = Self::shortcut_hint_for_mode(ui.ctx(), cmd, self.mode);
                response.on_hover_text(
                    hint.map_or_else(|| label.clone(), |hint| format!("{label} ({hint})")),
                );
                if activated {
                    *command = Some(cmd.clone());
                }
            }
            MenuEntry::Submenu { label, entries } => {
                ui.add_enabled_ui(self.menu_entries_available(entries), |ui| {
                    let (response, activated) =
                        command_button(ui, entry_glyph(entry), label, detail, false);
                    self.ribbon_popup(ui, &response, activated, entries, command);
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

    fn ribbon_popup(
        &mut self,
        ui: &mut Ui,
        response: &egui::Response,
        activated: bool,
        entries: &[MenuEntry],
        command: &mut Option<Cmd>,
    ) {
        Glyph::Chevron.paint(
            ui,
            egui::Rect::from_min_size(
                response.rect.right_top() + egui::vec2(-11.0, 3.0),
                egui::vec2(8.0, 8.0),
            ),
            ui.visuals().weak_text_color(),
        );
        let popup = egui::Popup::menu(response).open_memory(None);
        if activated {
            if popup.is_open() {
                egui::Popup::close_id(ui.ctx(), popup.get_id());
            } else {
                egui::Popup::open_id(ui.ctx(), popup.get_id());
            }
        }
        popup.show(|ui| {
            crate::ui::scroll_menu(ui, |ui| self.render_menu_entries(ui, entries, command))
        });
    }

    fn ribbon_group(
        &mut self,
        ui: &mut Ui,
        label: &str,
        entries: &[&MenuEntry],
        detail: GroupDetail,
        snapshot: &crate::native_menus::MenuSnapshot,
        command: &mut Option<Cmd>,
    ) {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
            let width = group_width(detail, entries.len());
            ui.set_width(width);
            ui.allocate_ui(egui::vec2(width, 94.0), |ui| {
                if detail == GroupDetail::Menu {
                    let glyph = entries
                        .first()
                        .map_or(Glyph::Chevron, |entry| entry_glyph(entry));
                    let (response, activated) =
                        command_button(ui, glyph, &format!("{label}…"), Detail::Large, false);
                    let entries: Vec<_> = entries.iter().map(|entry| (*entry).clone()).collect();
                    self.ribbon_popup(ui, &response, activated, &entries, command);
                } else {
                    ui.horizontal_top(|ui| {
                        if let Some(first) = entries.first() {
                            self.ribbon_entry(ui, first, snapshot, Detail::Large, command);
                        }
                        for chunk in entries.get(1..).unwrap_or_default().chunks(3) {
                            ui.vertical(|ui| {
                                ui.set_width(if detail == GroupDetail::Full {
                                    142.0
                                } else {
                                    30.0
                                });
                                for entry in chunk {
                                    self.ribbon_entry(
                                        ui,
                                        entry,
                                        snapshot,
                                        if detail == GroupDetail::Full {
                                            Detail::Labeled
                                        } else {
                                            Detail::Icon
                                        },
                                        command,
                                    );
                                }
                            });
                        }
                    });
                }
            });
            ui.add_sized(
                [width, 16.0],
                egui::Label::new(
                    egui::RichText::new(label)
                        .size(10.0)
                        .color(Palette::of_ui(ui).text_muted),
                )
                .truncate(),
            );
        });
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
            .exact_size(if expanded { 162.0 } else { 38.0 })
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
                        let (_, toggle) =
                            command_button(ui, Glyph::Chevron, label, Detail::Icon, expanded);
                        if toggle {
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
                                        let selected = self.ribbon.tab == index;
                                        let response = ui
                                            .selectable_label(selected, label)
                                            .on_hover_text(format!("Alt+{}", index + 1));
                                        if selected {
                                            let rect = response.rect;
                                            ui.painter().line_segment(
                                                [rect.left_bottom(), rect.right_bottom()],
                                                egui::Stroke::new(2.0, Palette::of_ui(ui).accent),
                                            );
                                        }
                                        if response.clicked()
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
                            let sections = command_groups(entries, label);
                            let counts: Vec<_> =
                                sections.iter().map(|(_, items)| items.len()).collect();
                            let details = group_details(&counts, ui.available_width());
                            let total: f32 = details
                                .iter()
                                .zip(&counts)
                                .map(|(detail, count)| group_width(*detail, *count) + 12.0)
                                .sum();
                            let visible = if total <= ui.available_width() {
                                sections.len()
                            } else {
                                ((ui.available_width() - 90.0).max(0.0) / 90.0) as usize
                            };
                            ui.horizontal_top(|ui| {
                                ui.spacing_mut().item_spacing.x = 6.0;
                                for (index, ((label, items), detail)) in
                                    sections.iter().zip(&details).take(visible).enumerate()
                                {
                                    ui.push_id(index, |ui| {
                                        self.ribbon_group(
                                            ui,
                                            label,
                                            items,
                                            *detail,
                                            &snapshot,
                                            &mut command,
                                        )
                                    });
                                    ui.separator();
                                }
                                if visible < sections.len() {
                                    let overflow: Vec<_> = sections
                                        .iter()
                                        .skip(visible)
                                        .map(|(label, items)| MenuEntry::Submenu {
                                            label: label.to_string(),
                                            entries: items
                                                .iter()
                                                .map(|entry| (*entry).clone())
                                                .collect(),
                                        })
                                        .collect();
                                    let (response, activated) = command_button(
                                        ui,
                                        Glyph::Chevron,
                                        t("Ribbon"),
                                        Detail::Large,
                                        false,
                                    );
                                    self.ribbon_popup(
                                        ui,
                                        &response,
                                        activated,
                                        &overflow,
                                        &mut command,
                                    );
                                }
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
