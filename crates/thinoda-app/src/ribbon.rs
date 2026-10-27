//! Desktop command ribbon uses the same hierarchy and handlers as native menus.

use crate::{
    app::{Cmd, ThinodaApp},
    i18n::t,
    menus::MenuEntry,
};
use egui::{Key, Modifiers, Ui};
use thinoda_ui::icons;
use thinoda_ui::ribbon::{CommandIcon, Detail, IconTone, command_button, menu_button};
use thinoda_ui::{tokens::Palette, widgets};

#[derive(Default)]
pub(crate) struct RibbonState {
    tab: usize,
    temporary: bool,
    snapshot: Option<crate::native_menus::MenuSnapshot>,
    rect: Option<egui::Rect>,
    pending: Option<(crate::native_menus::MenuSnapshot, Cmd)>,
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    qat_rect: Option<egui::Rect>,
}

#[cfg(test)]
#[path = "ribbon_tests.rs"]
mod tests;

use crate::command_icons::command_glyph;

fn entry_glyph(entry: &MenuEntry) -> CommandIcon {
    match entry {
        MenuEntry::Command { command, .. } => command_glyph(command),
        MenuEntry::Submenu { label, .. } if label == t("New from Template…") => {
            CommandIcon::from(icons::TEMPLATE).with_tone(IconTone::File)
        }
        MenuEntry::Submenu { label, .. } if label == t("Open Recent") => {
            CommandIcon::from(icons::RECENT).with_tone(IconTone::File)
        }
        MenuEntry::Submenu { label, .. } if label == t("Export") => {
            CommandIcon::from(icons::EXPORT).with_tone(IconTone::File)
        }
        MenuEntry::Submenu { entries, .. } => entries
            .first()
            .map_or_else(|| icons::OPEN.into(), entry_glyph),
        MenuEntry::ClearRecent => {
            CommandIcon::marked(icons::RECENT, icons::REMOVE).with_tone(IconTone::Danger)
        }
        _ => icons::CHEVRON_DOWN.into(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GroupDetail {
    Full,
    Compact,
    Menu,
}

const LABELED_COLUMN_WIDTH: f32 = 204.0;

fn group_width(detail: GroupDetail, count: usize, compact: bool) -> f32 {
    if compact {
        return 112.0
            + count.saturating_sub(1).div_ceil(2) as f32
                * match detail {
                    GroupDetail::Full => 188.0,
                    GroupDetail::Compact => 38.0,
                    GroupDetail::Menu => 0.0,
                };
    }
    match detail {
        GroupDetail::Full => 120.0 + count.saturating_sub(1).div_ceil(2) as f32 * 212.0,
        GroupDetail::Compact => 120.0 + count.saturating_sub(1).div_ceil(2) as f32 * 52.0,
        GroupDetail::Menu => 120.0,
    }
}

fn control_count(entries: &[&MenuEntry]) -> usize {
    entries
        .iter()
        .filter(|entry| !matches!(entry, MenuEntry::Caption(_)))
        .count()
}

fn group_details(counts: &[usize], available: f32, compact: bool) -> Vec<GroupDetail> {
    let mut details = vec![GroupDetail::Full; counts.len()];
    let width = |details: &[GroupDetail]| {
        details
            .iter()
            .zip(counts)
            .map(|(detail, count)| {
                group_width(*detail, *count, compact) + if compact { 16.0 } else { 24.0 }
            })
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
                Cmd::Canvas(Action::Undo | Action::Redo) => t("Editing"),
                Cmd::Canvas(Action::Copy | Action::Cut | Action::Duplicate | Action::Delete)
                | Cmd::MenuPaste => t("Clipboard"),
                Cmd::Canvas(Action::SelectAll) | Cmd::Find | Cmd::FindReplace | Cmd::Palette => {
                    t("Select")
                }
                Cmd::Canvas(Action::Indent | Action::Outdent) => t("Arrange"),
                Cmd::CopyStyle | Cmd::PasteStyle | Cmd::ResetStyle | Cmd::SaveQuickStyle => {
                    t("Styles")
                }
                Cmd::EditField(_)
                | Cmd::Labels
                | Cmd::LocalImage
                | Cmd::Attachment
                | Cmd::SaveAttachment => t("Notes"),
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
                | Cmd::NewSheetFromTopic
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
    for (_, entries) in &mut groups {
        if let Some(index) = entries.iter().position(|entry| {
            matches!(
                entry,
                MenuEntry::Command {
                    command: Cmd::MenuPaste,
                    ..
                }
            )
        }) {
            let primary = entries.remove(index);
            entries.insert(0, primary);
        }
    }
    groups
}

impl ThinodaApp {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    pub(crate) fn quick_access_toolbar(&mut self, ui: &mut Ui) {
        let snapshot = self
            .ribbon
            .snapshot
            .clone()
            .unwrap_or_else(|| self.native_menu_snapshot(ui.ctx()));
        let preferences = self.settings.quick_access;
        let response = ui.horizontal_centered(|ui| {
            let narrow = ui.ctx().content_rect().width() < 640.0;
            for (visible, command, label) in [
                (preferences.save, Cmd::Save, t("Save")),
                (
                    preferences.undo,
                    Cmd::Canvas(thinoda_canvas::Action::Undo),
                    t("Undo"),
                ),
                (
                    preferences.redo,
                    Cmd::Canvas(thinoda_canvas::Action::Redo),
                    t("Redo"),
                ),
                (preferences.print, Cmd::Print, t("Print")),
            ] {
                if !visible
                    || (narrow
                        && matches!(
                            command,
                            Cmd::Print | Cmd::Canvas(thinoda_canvas::Action::Redo)
                        ))
                {
                    continue;
                }
                let hint = Self::shortcut_hint_for_mode(ui.ctx(), &command, self.mode);
                let tooltip =
                    hint.map_or_else(|| label.to_owned(), |hint| format!("{label} ({hint})"));
                let (_, activated) = ui
                    .add_enabled_ui(snapshot.command_enabled(&command), |ui| {
                        command_button(
                            ui,
                            command_glyph(&command),
                            label,
                            Detail::Icon,
                            false,
                            Some(&tooltip),
                        )
                    })
                    .inner;
                if activated {
                    self.ribbon.pending = Some((snapshot.clone(), command));
                    ui.ctx().request_repaint();
                }
            }
            let mut preferences = preferences;
            let (response, activated) = command_button(
                ui,
                icons::CHEVRON_DOWN,
                t("Quick Access Toolbar"),
                Detail::Icon,
                false,
                None,
            );
            let popup = egui::Popup::menu(&response).open_memory(None);
            if activated {
                if popup.is_open() {
                    egui::Popup::close_id(ui.ctx(), popup.get_id());
                } else {
                    egui::Popup::open_id(ui.ctx(), popup.get_id());
                }
            }
            popup.show(|ui| {
                ui.label(t("Quick Access Toolbar"));
                ui.checkbox(&mut preferences.save, t("Save"));
                ui.checkbox(&mut preferences.undo, t("Undo"));
                ui.checkbox(&mut preferences.redo, t("Redo"));
                ui.checkbox(&mut preferences.print, t("Print"));
            });
            if preferences != self.settings.quick_access {
                self.settings.quick_access = preferences;
                self.save_settings();
            }
        });
        self.ribbon.qat_rect = Some(response.response.rect);
    }

    fn ribbon_entry(
        &mut self,
        ui: &mut Ui,
        entry: &MenuEntry,
        snapshot: &crate::native_menus::MenuSnapshot,
        detail: Detail,
        help: &str,
        command: &mut Option<Cmd>,
    ) {
        let detail = if self.settings.compact_ribbon {
            detail.compact()
        } else {
            detail
        };
        match entry {
            MenuEntry::Command {
                label,
                command: cmd,
            } => {
                let hint = Self::shortcut_hint_for_mode(ui.ctx(), cmd, self.mode);
                let mut tooltip =
                    hint.map_or_else(|| label.clone(), |hint| format!("{label} ({hint})"));
                if !help.is_empty() {
                    tooltip.push('\n');
                    tooltip.push_str(help);
                }
                let (_, activated) = ui
                    .add_enabled_ui(snapshot.command_enabled(cmd), |ui| {
                        command_button(
                            ui,
                            command_glyph(cmd),
                            label,
                            detail,
                            snapshot.command_checked(cmd),
                            Some(&tooltip),
                        )
                    })
                    .inner;
                if activated {
                    *command = Some(cmd.clone());
                }
            }
            MenuEntry::Submenu { label, entries } => {
                ui.add_enabled_ui(self.menu_entries_available(entries), |ui| {
                    let (response, activated) =
                        menu_button(ui, entry_glyph(entry), label, detail, None);
                    self.ribbon_popup(ui, &response, activated, entries, command);
                });
            }
            MenuEntry::Caption(_) => {}
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
        let help = entries
            .iter()
            .filter_map(|entry| match entry {
                MenuEntry::Caption(text) => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        let controls: Vec<_> = entries
            .iter()
            .copied()
            .filter(|entry| !matches!(entry, MenuEntry::Caption(_)))
            .collect();
        let compact = self.settings.compact_ribbon;
        let large = if compact {
            Detail::CompactLarge
        } else {
            Detail::Large
        };
        let body_height = if compact { 72.0 } else { 96.0 };
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::splat(if compact { 4.0 } else { 8.0 });
            let width = group_width(detail, controls.len(), compact);
            ui.set_width(width);
            ui.allocate_ui(egui::vec2(width, body_height), |ui| {
                ui.set_min_height(body_height);
                if detail == GroupDetail::Menu {
                    let glyph = controls
                        .first()
                        .map_or_else(|| icons::CHEVRON_DOWN.into(), |entry| entry_glyph(entry));
                    let (response, activated) = menu_button(
                        ui,
                        glyph,
                        &format!("{label}…"),
                        large,
                        if help.is_empty() { None } else { Some(&help) },
                    );
                    let entries: Vec<_> = controls.iter().map(|entry| (*entry).clone()).collect();
                    self.ribbon_popup(ui, &response, activated, &entries, command);
                } else {
                    ui.horizontal(|ui| {
                        if let Some(primary) = controls.first() {
                            self.ribbon_entry(ui, primary, snapshot, Detail::Large, &help, command);
                        }
                        for chunk in controls.get(1..).unwrap_or_default().chunks(2) {
                            ui.vertical(|ui| {
                                ui.set_width(if detail == GroupDetail::Full {
                                    if compact { 184.0 } else { LABELED_COLUMN_WIDTH }
                                } else {
                                    if compact { 34.0 } else { 44.0 }
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
                                        &help,
                                        command,
                                    );
                                }
                            });
                        }
                    });
                }
            });
            let footer = ui.add_sized(
                [width, if compact { 16.0 } else { 20.0 }],
                egui::Label::new(
                    egui::RichText::new(label)
                        .size(11.0)
                        .color(Palette::of_ui(ui).text_muted),
                )
                .truncate(),
            );
            if !help.is_empty() {
                footer.on_hover_text(&help);
            }
        });
    }

    pub fn show_ribbon(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if !ctx.input(|input| input.pointer.any_pressed() || input.pointer.any_released())
            && let Some((snapshot, command)) = self.ribbon.pending.take()
        {
            self.dispatch_ribbon_command(&ctx, &snapshot, &command);
        }
        let mut groups = self.main_menu_entries();
        for group in &mut groups {
            if let MenuEntry::Submenu { label, .. } = group {
                if label == t("Edit") {
                    *label = t("Home").into();
                } else if label == t("Tools") {
                    *label = t("Capture").into();
                }
            }
        }
        let mut snapshot = self.native_menu_snapshot(&ctx);
        // egui clears text focus on pointer press before the ribbon is drawn.
        // Keep the producing text owner through this ribbon pointer sequence.
        if !snapshot.has_text_focus()
            && let Some(previous) = &self.ribbon.snapshot
            && previous.has_text_focus()
            && ctx.input(|input| {
                (input.pointer.any_down() || input.pointer.any_released())
                    && input.pointer.interact_pos().is_some_and(|pos| {
                        let ribbon = self.ribbon.rect.is_some_and(|rect| rect.contains(pos));
                        #[cfg(any(target_os = "windows", target_os = "macos"))]
                        let ribbon =
                            ribbon || self.ribbon.qat_rect.is_some_and(|rect| rect.contains(pos));
                        ribbon
                    })
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
            ctx.memory_mut(|memory| {
                if let Some(focused) = memory.focused() {
                    memory.surrender_focus(focused);
                }
            });
            self.ribbon.tab = tab;
            self.ribbon.temporary = self.settings.ribbon_collapsed;
        }
        let room = ctx.content_rect().height() >= 420.0;
        let compact = self.settings.compact_ribbon;
        let header_height = if compact { 34.0 } else { 44.0 };
        let icon_detail = if compact {
            Detail::CompactIcon
        } else {
            Detail::Icon
        };
        let expanded = room && (!self.settings.ribbon_collapsed || self.ribbon.temporary);
        let mut command: Option<Cmd> = None;
        if !room {
            let panel = egui::Panel::top("thinoda_ribbon_compact")
                .exact_size(60.0)
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
            .exact_size(if expanded {
                if compact { 160.0 } else { 200.0 }
            } else {
                header_height + 16.0
            })
            .frame(
                egui::Frame::new()
                    .fill(Palette::of(&ctx).surface)
                    .inner_margin(egui::Margin::symmetric(8, 8)),
            )
            .show(ui, |ui| {
                ui.add_enabled_ui(!blocked, |ui| {
                    ui.horizontal_top(|ui| {
                        let label = if self.settings.ribbon_collapsed {
                            t("Expand Ribbon")
                        } else {
                            t("Collapse Ribbon")
                        };
                        let (_, toggle) = command_button(
                            ui,
                            icons::CHEVRON_DOWN,
                            label,
                            icon_detail,
                            expanded,
                            None,
                        );
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
                            .max_height(header_height)
                            .max_width((ui.available_width() - header_height - 8.0).max(0.0))
                            .show(ui, |ui| {
                                ui.horizontal_top(|ui| {
                                    ui.spacing_mut().interact_size.y = header_height;
                                    for (index, group) in groups.iter().enumerate() {
                                        let MenuEntry::Submenu { label, .. } = group else {
                                            continue;
                                        };
                                        let selected = self.ribbon.tab == index;
                                        let palette = Palette::of_ui(ui);
                                        let color = if selected {
                                            palette.accent
                                        } else {
                                            palette.text
                                        };
                                        let galley = ui.painter().layout_no_wrap(
                                            label.clone(),
                                            egui::FontId::proportional(14.0),
                                            color,
                                        );
                                        let (rect, response) = ui.allocate_exact_size(
                                            egui::vec2(
                                                galley.size().x + if compact { 12.0 } else { 16.0 },
                                                header_height,
                                            ),
                                            egui::Sense::click(),
                                        );
                                        response.widget_info(|| {
                                            egui::WidgetInfo::selected(
                                                egui::Role::Button,
                                                ui.is_enabled(),
                                                selected,
                                                label,
                                            )
                                        });
                                        if selected || response.hovered() {
                                            ui.painter().rect_filled(
                                                rect,
                                                8,
                                                if selected {
                                                    palette.accent_soft
                                                } else {
                                                    palette.surface_alt
                                                },
                                            );
                                        }
                                        if response.has_focus() {
                                            ui.painter().rect_stroke(
                                                rect.shrink(1.0),
                                                8,
                                                egui::Stroke::new(2.0, palette.text),
                                                egui::StrokeKind::Inside,
                                            );
                                        }
                                        ui.painter().galley(
                                            rect.center() - galley.size() * 0.5,
                                            galley,
                                            color,
                                        );
                                        let keyboard = response.has_focus()
                                            && ui.input_mut(|input| {
                                                input.consume_key(
                                                    egui::Modifiers::NONE,
                                                    egui::Key::Enter,
                                                ) || input.consume_key(
                                                    egui::Modifiers::NONE,
                                                    egui::Key::Space,
                                                )
                                            });
                                        let response =
                                            response.on_hover_text(format!("Alt+{}", index + 1));
                                        if response.has_focus() && self.ribbon.tab != index {
                                            self.ribbon.tab = index;
                                            self.ribbon.temporary = self.settings.ribbon_collapsed;
                                        }
                                        if selected {
                                            let rect = response.rect;
                                            ui.painter().line_segment(
                                                [rect.left_bottom(), rect.right_bottom()],
                                                egui::Stroke::new(2.0, Palette::of_ui(ui).accent),
                                            );
                                        }
                                        if (response.clicked()
                                            && widgets::touch_activation_allowed(&ctx))
                                            || keyboard
                                        {
                                            self.ribbon.tab = index;
                                            self.ribbon.temporary = self.settings.ribbon_collapsed;
                                        }
                                    }
                                    crate::ui::reveal_changed_focus(ui);
                                });
                            });
                        let pinned = !self.settings.ribbon_collapsed;
                        let label = if pinned {
                            t("Unpin Ribbon")
                        } else {
                            t("Pin Ribbon")
                        };
                        let (_, pin) =
                            command_button(ui, icons::PIN, label, icon_detail, pinned, None);
                        if pin {
                            self.settings.ribbon_collapsed = pinned;
                            self.ribbon.temporary = pinned && expanded;
                            self.save_settings();
                        }
                    });
                    if expanded {
                        ui.separator();
                        if let Some(MenuEntry::Submenu { label, entries }) =
                            groups.get(self.ribbon.tab)
                        {
                            let sections = command_groups(entries, label);
                            let counts: Vec<_> = sections
                                .iter()
                                .map(|(_, items)| control_count(items))
                                .collect();
                            let details = group_details(&counts, ui.available_width(), compact);
                            let group_slot = if compact { 128.0 } else { 144.0 };
                            let total: f32 = details
                                .iter()
                                .zip(&counts)
                                .map(|(detail, count)| {
                                    group_width(*detail, *count, compact)
                                        + if compact { 16.0 } else { 24.0 }
                                })
                                .sum();
                            let visible = if total <= ui.available_width() {
                                sections.len()
                            } else {
                                ((ui.available_width() - group_slot).max(0.0) / group_slot) as usize
                            };
                            ui.horizontal_top(|ui| {
                                ui.spacing_mut().item_spacing.x = if compact { 8.0 } else { 12.0 };
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
                                    let (response, activated) = menu_button(
                                        ui,
                                        icons::CHEVRON_DOWN,
                                        t("Ribbon"),
                                        if compact {
                                            Detail::CompactLarge
                                        } else {
                                            Detail::Large
                                        },
                                        None,
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
