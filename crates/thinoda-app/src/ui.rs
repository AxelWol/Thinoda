//! All app chrome: top bar, format panel, sheet tabs, status bar, outliner, home screen, dialogs.

use egui::{
    Align, Color32, Context, CornerRadius, Key, Layout, Margin, Modifiers, RichText, Sense, Stroke,
    TextEdit, Ui, vec2,
};
use thinoda_canvas::{Action, MapCanvas};
use thinoda_core::{
    Color, LineShape, Structure, TextAlign, TopicId, TopicShape, TopicStyle, markers, templates,
    theme::Theme,
};
use thinoda_platform::settings::{Language, ThemeMode as SettingsTheme};
use thinoda_ui::{DialogButton, Palette, Tokens, confirm_dialog, icons, widgets};

use crate::{
    app::{
        CenterMode, CloseIntent, Cmd, EXPORT_MENU, FocusField, FormatTab, SettingsTab,
        SidebarFamily, ThinodaApp,
    },
    i18n::{self, t},
};

type StyleChange = (&'static str, Box<dyn Fn(&mut TopicStyle)>);
type StyleToggle = fn(&mut TopicStyle, bool);

pub(crate) fn font_selector(
    ui: &mut Ui,
    fonts: &mut thinoda_ui::fonts::SystemFonts,
    value: &mut Option<String>,
    default_label: &str,
) -> egui::Response {
    egui::ComboBox::from_id_salt("font_family")
        .selected_text(match value.as_deref() {
            None => default_label,
            Some("monospace") => t("Monospace"),
            Some(name) => name,
        })
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            ui.selectable_value(value, None, default_label);
            ui.selectable_value(value, Some("monospace".into()), t("Monospace"));
            for name in fonts.names() {
                ui.selectable_value(value, Some(name.clone()), name);
            }
        })
        .response
}

/// Swatch colors offered in the format panel (XMind-like palette).
pub const SWATCHES: [Color32; 12] = [
    Color32::from_rgb(0x2E, 0x7C, 0xF6),
    Color32::from_rgb(0x00, 0xA8, 0x9D),
    Color32::from_rgb(0x2B, 0xB6, 0x73),
    Color32::from_rgb(0xF5, 0xA6, 0x23),
    Color32::from_rgb(0xF2, 0x6B, 0x3A),
    Color32::from_rgb(0xE8, 0x45, 0x5C),
    Color32::from_rgb(0xA3, 0x5B, 0xE0),
    Color32::from_rgb(0x5B, 0x6B, 0xE0),
    Color32::from_rgb(0x33, 0x33, 0x33),
    Color32::from_rgb(0x88, 0x88, 0x88),
    Color32::from_rgb(0xEE, 0xEE, 0xEE),
    Color32::WHITE,
];

pub fn to_color(c: Color32) -> Color {
    Color {
        r: c.r(),
        g: c.g(),
        b: c.b(),
        a: c.a(),
    }
}

pub fn from_color(c: Color) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

pub(crate) fn structure_label(s: Structure) -> &'static str {
    match s {
        Structure::MapClockwise => "Mind Map",
        Structure::MapAnticlockwise => "Mind Map (anticlockwise)",
        Structure::MapUnbalanced => "Mind Map (one side)",
        Structure::LogicRight => "Logic Chart (right)",
        Structure::LogicLeft => "Logic Chart (left)",
        Structure::BraceRight => "Brace Map (right)",
        Structure::BraceLeft => "Brace Map (left)",
        Structure::OrgDown => "Org Chart (down)",
        Structure::OrgUp => "Org Chart (up)",
        Structure::TreeRight => "Tree Chart (right)",
        Structure::TreeLeft => "Tree Chart (left)",
        Structure::TimelineHorizontal => "Timeline (horizontal)",
        Structure::TimelineVertical => "Timeline (vertical)",
        Structure::FishboneRight => "Fishbone (right)",
        Structure::FishboneLeft => "Fishbone (left)",
        Structure::Matrix => "Matrix",
        Structure::TreeTable => "Tree Table",
    }
}

impl ThinodaApp {
    /// Draws one frame of the whole application.
    pub fn draw(&mut self, ui: &mut Ui) {
        if !self.launch_frame(ui) {
            return;
        }
        let ctx = ui.ctx().clone();
        let font_source = self.doc().map(|doc| (doc.key.clone(), doc.doc.state_id()));
        if font_source != self.font_source {
            let families: std::collections::BTreeSet<_> =
                self.doc()
                    .into_iter()
                    .flat_map(|doc| doc.doc.workbook().sheets.iter())
                    .flat_map(|sheet| {
                        sheet.theme.fonts().chain(
                            sheet
                                .topics
                                .values()
                                .filter_map(|topic| topic.style.font_family.as_ref()),
                        )
                    })
                    .chain(
                        self.settings
                            .quick_styles
                            .iter()
                            .filter_map(|style| style.style.font_family.as_ref()),
                    )
                    .chain(
                        self.settings.custom_themes.iter().flat_map(|custom| {
                            custom.font_family.iter().chain(custom.roles.fonts())
                        }),
                    )
                    .cloned()
                    .collect();
            for name in self.fonts.ensure(&ctx, families) {
                log::warn!("Font unavailable: {name}");
                self.toasts.warning(format!(
                    "{}: {name}. {}",
                    t("Font unavailable"),
                    t("Using the default font; the requested name is preserved.")
                ));
            }
            self.font_source = font_source;
        }
        self.apply_appearance(&ctx);
        self.handle_close_request(&ctx);
        self.handle_sheet_paste(&ctx);
        self.poll_map_shot(&ctx);
        if !self.palette.open && !self.any_modal_open() && !egui::Popup::is_any_open(&ctx) {
            self.global_shortcuts(&ctx);
            self.insert_shortcuts(&ctx);
        }

        let show_doc = self.active < self.docs.len();
        if !self.zen {
            egui::Panel::top("thinoda_topbar")
                .exact_size(Tokens::TOOLBAR_HEIGHT)
                .frame(
                    egui::Frame::new()
                        .fill(Palette::of(&ctx).surface)
                        .inner_margin(Margin::symmetric(8, 6)),
                )
                .show(ui, |ui| {
                    if self.map_shot.active() || self.printing.active() || self.dialogs.legal.open {
                        ui.disable();
                    }
                    self.top_bar(ui);
                });
            if show_doc {
                egui::Panel::bottom("thinoda_statusbar")
                    .exact_size(30.0)
                    .frame(
                        egui::Frame::new()
                            .fill(Palette::of(&ctx).surface)
                            .inner_margin(Margin::symmetric(8, 3)),
                    )
                    .show(ui, |ui| {
                        if self.map_shot.active()
                            || self.printing.active()
                            || self.dialogs.legal.open
                        {
                            ui.disable();
                        }
                        self.bottom_bar(ui);
                    });
                if self.settings.show_format_panel {
                    egui::Panel::right("thinoda_format")
                        .default_size(280.0)
                        .size_range(240.0..=420.0)
                        .frame(
                            egui::Frame::new()
                                .fill(Palette::of(&ctx).surface)
                                .inner_margin(Margin::same(10)),
                        )
                        .show(ui, |ui| {
                            if self.map_shot.active()
                                || self.printing.active()
                                || self.dialogs.legal.open
                            {
                                ui.disable();
                            }
                            self.sidebar(ui);
                        });
                }
            }
        }

        let center = egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                if self.label_editor.is_some()
                    || self.printing.active()
                    || self.quick_style_dialog.is_some()
                    || self.theme_dialog.is_some()
                    || self.dialogs.guide.open
                    || self.dialogs.shortcuts
                    || self.dialogs.legal.open
                {
                    ui.disable();
                }
                if !show_doc {
                    self.home(ui);
                } else if self.mode == CenterMode::Outliner {
                    self.outliner(ui);
                } else {
                    self.map(ui);
                }
            });

        if show_doc && !self.zen && !self.any_modal_open() && !self.dialogs.settings {
            self.editing_toolbar(&ctx, center.response.rect);
        }

        if show_doc
            && self.mode == CenterMode::Map
            && !self.search.open
            && !self.palette.open
            && !self.any_modal_open()
            && !self.dialogs.settings
        {
            self.topic_toolbar(&ctx);
        }

        if show_doc && self.search.open && !self.map_shot.active() && !self.printing.active() {
            self.search_bar(&ctx);
        }
        self.update_search();

        // Command palette.
        let mut cmds = Self::palette_commands();
        self.add_quick_style_commands(&mut cmds);
        self.add_custom_theme_commands(&mut cmds);
        let list: Vec<thinoda_ui::Command> = cmds
            .iter()
            .filter(|(cmd, _)| self.command_available(cmd))
            .map(|(_, c)| c.clone())
            .collect();
        if let Some(id) = self.palette.show(&ctx, &list, t("Type a command…"))
            && let Some((cmd, _)) = cmds.iter().find(|(_, c)| c.id == id)
        {
            self.exec(&ctx, cmd.clone());
        }

        self.labels_popup(&ctx);
        self.quick_style_dialog(&ctx);
        self.theme_editor_dialog(&ctx);
        self.guide_dialog(&ctx);
        self.shortcuts_dialog(&ctx);
        if !self.map_shot.active() {
            self.dialogs_ui(&ctx);
            self.ai_prompt_dialog(&ctx);
            self.ai_busy_overlay(&ctx);
        }
        self.map_shot_dialog(&ctx);
        self.print_dialog(&ctx);
        self.legal_dialog(&ctx);
        if !self.map_shot.requested && !self.map_shot.capturing {
            self.toasts.show_with_modal(&ctx, self.any_modal_open());
        }
        if let Some(d) = self.doc() {
            self.capture_map_shot(&ctx, d.view.canvas_rect());
        }
        self.housekeeping(&ctx);
    }

    pub(crate) fn any_modal_open(&self) -> bool {
        if self.dialogs.masked_copy
            || self.printing.active()
            || self.map_shot.active()
            || self.label_editor.is_some()
            || self.quick_style_dialog.is_some()
            || self.theme_dialog.is_some()
            || self.dialogs.guide.open
            || self.dialogs.shortcuts
            || self.dialogs.legal.open
        {
            return true;
        }
        let d = &self.dialogs;
        d.close.is_some()
            || d.conflict.is_some()
            || d.rename_sheet.is_some()
            || !d.recovery.is_empty()
            || self.ai.prompt.is_some()
    }

    // ----- top bar ----------------------------------------------------------------------------

    fn top_bar(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let mut cmd: Option<Cmd> = None;
        ui.horizontal_centered(|ui| {
            ui.menu_button(RichText::new(icons::MENU).size(18.0), |ui| {
                self.main_menu(ui, &mut cmd)
            });
            let (can_undo, can_redo) = self
                .doc()
                .map_or((false, false), |d| (d.doc.can_undo(), d.doc.can_redo()));
            if ui
                .add_enabled_ui(can_undo, |ui| {
                    widgets::icon_button(ui, icons::UNDO, t("Undo"), false)
                })
                .inner
                .clicked()
            {
                cmd = Some(Cmd::Canvas(Action::Undo));
            }
            if ui
                .add_enabled_ui(can_redo, |ui| {
                    widgets::icon_button(ui, icons::REDO, t("Redo"), false)
                })
                .inner
                .clicked()
            {
                cmd = Some(Cmd::Canvas(Action::Redo));
            }
            widgets::vdivider(ui);
            // Document tabs.
            egui::ScrollArea::horizontal()
                .max_width(ui.available_width() * 0.4)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for i in 0..self.docs.len() {
                            let d = &self.docs[i];
                            let label =
                                format!("{}{}", if d.is_dirty() { "• " } else { "" }, d.title());
                            let sel = i == self.active;
                            let r = ui.selectable_label(sel, label);
                            if r.clicked() {
                                self.active = i;
                            }
                            if r.middle_clicked() {
                                cmd = Some(Cmd::CloseTab(i));
                            }
                            if let Some(p) = &d.path {
                                r.clone().on_hover_text(p.display().to_string());
                            }
                            if sel
                                && widgets::icon_button(ui, icons::CLOSE, t("Close Tab"), false)
                                    .clicked()
                            {
                                cmd = Some(Cmd::CloseTab(i));
                            }
                        }
                        if widgets::icon_button(ui, icons::ADD, t("New Tab"), false).clicked() {
                            cmd = Some(Cmd::Home);
                        }
                    });
                });

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::icon_button(
                    ui,
                    icons::FORMAT,
                    t("Format Panel"),
                    self.doc().is_some()
                        && self.settings.show_format_panel
                        && self.sidebar_family == SidebarFamily::Format,
                )
                .clicked()
                {
                    cmd = Some(Cmd::ToggleFormatPanel);
                }
                if widgets::icon_button(
                    ui,
                    icons::MARKER,
                    t("Symbols"),
                    self.doc().is_some()
                        && self.settings.show_format_panel
                        && self.sidebar_family == SidebarFamily::Symbols,
                )
                .clicked()
                {
                    cmd = Some(Cmd::ToggleSymbolsPanel);
                }
                ui.menu_button(RichText::new(icons::EXPORT), |ui| {
                    for f in EXPORT_MENU {
                        if ui.button(t(f.label())).clicked() {
                            cmd = Some(Cmd::Export(*f));
                        }
                    }
                })
                .response
                .on_hover_text(t("Export"));
                if widgets::icon_button(ui, icons::ZEN, t("Zen Mode"), self.zen).clicked() {
                    cmd = Some(Cmd::ToggleZen);
                }
                let outl = self.mode == CenterMode::Outliner;
                if widgets::icon_button(
                    ui,
                    if outl { icons::MAP } else { icons::OUTLINER },
                    t("Outliner"),
                    outl,
                )
                .clicked()
                {
                    cmd = Some(Cmd::ToggleOutliner);
                }
                if widgets::icon_button(ui, icons::SEARCH, t("Find…"), self.search.open).clicked()
                {
                    cmd = Some(Cmd::Find);
                }
            });
        });
        if let Some(c) = cmd {
            self.exec(&ctx, c);
        }
    }

    fn main_menu(&mut self, ui: &mut Ui, cmd: &mut Option<Cmd>) {
        ui.set_min_width(220.0);
        let item =
            |ui: &mut Ui, label: &str, sc: &str| ui.add(egui::Button::new(label).shortcut_text(sc));
        if item(ui, t("New"), "Ctrl+N").clicked() {
            *cmd = Some(Cmd::New);
        }
        ui.menu_button(t("New from Template…"), |ui| {
            for category in templates::TemplateCategory::ALL {
                let category_label = t(category.label());
                ui.menu_button(category_label, |ui| {
                    for template in templates::TEMPLATES
                        .iter()
                        .filter(|template| template.category == category)
                    {
                        if ui.button(t(template.name)).clicked() {
                            *cmd = Some(Cmd::NewFromTemplate(template.id));
                        }
                    }
                });
            }
        });
        if item(ui, t("Open…"), "Ctrl+O").clicked() {
            *cmd = Some(Cmd::Open);
        }
        ui.menu_button(t("Open Recent"), |ui| {
            let recent: Vec<_> = self
                .settings
                .existing_recent()
                .into_iter()
                .map(|r| r.path.clone())
                .collect();
            if recent.is_empty() {
                ui.label(t("No recent files"));
            }
            for p in recent.iter().take(15) {
                if ui
                    .button(
                        p.file_name()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                    )
                    .on_hover_text(p.display().to_string())
                    .clicked()
                {
                    *cmd = Some(Cmd::OpenPath(p.clone()));
                }
            }
            ui.separator();
            if ui.button(t("Clear Recent")).clicked() {
                self.settings.clear_recent();
                self.save_settings();
            }
        });
        ui.separator();
        if item(ui, t("Save"), "Ctrl+S").clicked() {
            *cmd = Some(Cmd::Save);
        }
        if item(ui, t("Save As…"), "Ctrl+Shift+S").clicked() {
            *cmd = Some(Cmd::SaveAs);
        }
        if ui.button(t("Save All")).clicked() {
            *cmd = Some(Cmd::SaveAll);
        }
        if ui.button(t("Import")).clicked() {
            *cmd = Some(Cmd::Import);
        }
        ui.menu_button(t("Export"), |ui| {
            for f in EXPORT_MENU {
                if ui.button(t(f.label())).clicked() {
                    *cmd = Some(Cmd::Export(*f));
                }
            }
        });
        if ui
            .add_enabled(
                self.command_available(&Cmd::Print),
                egui::Button::new(t("Print")).shortcut_text("Ctrl+P"),
            )
            .clicked()
        {
            *cmd = Some(Cmd::Print);
            ui.close();
        }
        if self.doc().is_some_and(|d| d.path.is_some()) && ui.button(t("Show in Folder")).clicked()
        {
            *cmd = Some(Cmd::ShowInFolder);
        }
        ui.separator();
        ui.menu_button(t("Edit"), |ui| {
            let e = [
                ("Undo", "Ctrl+Z", Cmd::Canvas(Action::Undo)),
                ("Redo", "Ctrl+Shift+Z", Cmd::Canvas(Action::Redo)),
                ("Cut", "Ctrl+X", Cmd::Canvas(Action::Cut)),
                ("Copy", "Ctrl+C", Cmd::Canvas(Action::Copy)),
                ("Paste", "Ctrl+V", Cmd::MenuPaste),
                ("Duplicate", "Ctrl+D", Cmd::Canvas(Action::Duplicate)),
                ("Delete", "Del", Cmd::Canvas(Action::Delete)),
                ("Select All", "Ctrl+A", Cmd::Canvas(Action::SelectAll)),
                ("Find…", "Ctrl+F", Cmd::Find),
                ("Find and Replace…", "Ctrl+H", Cmd::FindReplace),
                ("Command Palette…", "Ctrl+Shift+P", Cmd::Palette),
            ];
            for (l, sc, c) in e {
                let button = egui::Button::new(t(l));
                let button = if sc.is_empty() { button } else { button.shortcut_text(sc) };
                if ui.add_enabled(self.command_available(&c), button).clicked() {
                    *cmd = Some(c);
                }
            }
            ui.menu_button(t("Select"), |ui| {
                for (action, label) in Self::selection_commands() {
                    let command = Cmd::Canvas(action);
                    if ui.add_enabled(self.command_available(&command), egui::Button::new(t(label))).clicked() {
                        *cmd = Some(command);
                    }
                }
            });
            ui.menu_button(t("Arrange"), |ui| {
                for (action, label) in Self::arrangement_commands() {
                    let command = Cmd::Canvas(action);
                    if ui.add_enabled(self.command_available(&command), egui::Button::new(t(label))).clicked() {
                        *cmd = Some(command);
                    }
                }
                widgets::caption(ui, t("Alignment needs floating topics only; distribution spaces centers, not edges."));
            });
            ui.separator();
            for (label, shortcut, command) in [
                ("Indent", "", Cmd::Canvas(Action::Indent)),
                ("Outdent", "", Cmd::Canvas(Action::Outdent)),
                ("Copy Style", "Ctrl+Alt+C", Cmd::CopyStyle),
                ("Paste Style", "Ctrl+Alt+V", Cmd::PasteStyle),
                ("Reset Style", "", Cmd::ResetStyle),
                ("Save Quick Style…", "", Cmd::SaveQuickStyle),
                ("New Sheet from Topic", "", Cmd::NewSheetFromTopic),
            ] {
                let button = egui::Button::new(t(label));
                let button = if shortcut.is_empty() {
                    button
                } else {
                    button.shortcut_text(shortcut)
                };
                if ui
                    .add_enabled(self.command_available(&command), button)
                    .clicked()
                {
                    *cmd = Some(command);
                }
            }
            widgets::caption(
                ui,
                t("Copies explicit overrides; inherited theme styles stay automatic."),
            );
        });
        ui.menu_button(t("Sheet"), |ui| {
            for (label, command) in [
                ("New Sheet", Cmd::NewSheet),
                ("Rename Sheet", Cmd::RenameSheet),
                ("Copy Sheet", Cmd::CopySheet),
                ("Paste Sheet", Cmd::PasteSheet),
                ("Duplicate Sheet", Cmd::DuplicateSheet),
                ("Save Sheet As…", Cmd::SaveSheetAs),
                ("Delete Sheet", Cmd::DeleteSheet),
            ] {
                if ui
                    .add_enabled(
                        self.command_available(&command),
                        egui::Button::new(t(label)),
                    )
                    .clicked()
                {
                    *cmd = Some(command);
                }
            }
        });
        ui.menu_button(t("Insert"), |ui| {
            let e = [
                ("Topic", "Enter", Action::InsertSibling),
                ("Subtopic", "Tab", Action::InsertChild),
                ("Topic Before", "Shift+Enter", Action::InsertSiblingBefore),
                ("Parent Topic", "Ctrl+Enter", Action::InsertParent),
                (
                    "Floating Topic",
                    "",
                    Action::InsertFloating(thinoda_core::Vec2::new(0.0, -260.0)),
                ),
                ("Relationship", "Ctrl+Shift+L", Action::StartRelationship),
                ("Summary", "Ctrl+]", Action::AddSummary),
                ("Boundary", "Ctrl+B", Action::AddBoundary),
                ("To-Do", "", Action::ToggleTodo),
            ];
            for (l, sc, a) in e {
                let command = Cmd::Canvas(a);
                let button = egui::Button::new(t(l));
                let button = if sc.is_empty() {
                    button
                } else {
                    button.shortcut_text(sc)
                };
                if ui
                    .add_enabled(self.command_available(&command), button)
                    .clicked()
                {
                    *cmd = Some(command);
                }
            }
            if ui
                .add_enabled(
                    self.command_available(&Cmd::Labels),
                    egui::Button::new(t("Labels")),
                )
                .clicked()
            {
                *cmd = Some(Cmd::Labels);
            }
        });
        ui.menu_button(t("AI"), |ui| {
            for a in crate::ai::AiCommand::ALL {
                let sc = if a == crate::ai::AiCommand::Brainstorm {
                    "Ctrl+Shift+I"
                } else {
                    ""
                };
                if item(ui, t(a.label()), sc).clicked() {
                    *cmd = Some(Cmd::Ai(a));
                }
            }
        });
        ui.menu_button(t("Tools"), |ui| {
            if ui.add_enabled(self.command_available(&Cmd::MapShot), egui::Button::new(t("Map Shot")).shortcut_text("F7")).clicked() {
                *cmd = Some(Cmd::MapShot);
            }
            if ui.add_enabled(self.command_available(&Cmd::MaskedCopy), egui::Button::new(t("Create Masked Copy…"))).clicked() {
                *cmd = Some(Cmd::MaskedCopy);
            }
            if ui.add_enabled(self.command_available(&Cmd::MergeFiles), egui::Button::new(t("Merge Map Files…"))).clicked() {
                *cmd = Some(Cmd::MergeFiles);
            }
            widgets::caption(ui, t("Appends complete sheets. Destination metadata is kept; external image references are not embedded."));
        });
        ui.menu_button(t("View"), |ui| {
            let e = [
                ("Previous Sheet", "Ctrl+PageUp", Cmd::PreviousSheet),
                ("Next Sheet", "Ctrl+PageDown", Cmd::NextSheet),
                ("Outliner", "Ctrl+Shift+O", Cmd::ToggleOutliner),
                ("Zen Mode", "F11", Cmd::ToggleZen),
                ("Format Panel", "Ctrl+Shift+R", Cmd::ToggleFormatPanel),
                ("Symbols", "", Cmd::ToggleSymbolsPanel),
                ("Status Bar", "", Cmd::ToggleStatusBar),
                ("Zoom In", "Ctrl++", Cmd::Canvas(Action::ZoomIn)),
                ("Zoom Out", "Ctrl+-", Cmd::Canvas(Action::ZoomOut)),
                ("Actual Size", "Ctrl+0", Cmd::Canvas(Action::ZoomReset)),
                ("Fit Map", "Ctrl+Shift+H", Cmd::Canvas(Action::Fit)),
                ("Expand All", "", Cmd::Canvas(Action::ExpandAll)),
                ("Collapse All", "", Cmd::Canvas(Action::CollapseAll)),
            ];
            for (l, sc, c) in e {
                if ui
                    .add_enabled(
                        self.command_available(&c),
                        egui::Button::new(t(l)).shortcut_text(sc),
                    )
                    .clicked()
                {
                    *cmd = Some(c);
                }
            }
        });
        ui.separator();
        if item(ui, t("Settings…"), "Ctrl+,").clicked() {
            *cmd = Some(Cmd::Settings);
        }
        ui.menu_button(t("Help"), |ui| {
            for (label, command) in [
                ("Quick Start", Cmd::QuickStart),
                ("User Guide", Cmd::UserGuide),
                ("Send Feedback…", Cmd::Feedback),
                ("Project News…", Cmd::ProjectNews),
            ] {
                if ui.button(t(label)).clicked() {
                    *cmd = Some(command);
                }
            }
            ui.separator();
            if item(ui, t("Keyboard Shortcuts"), "F1").clicked() {
                *cmd = Some(Cmd::Shortcuts);
            }
            if ui.button(t("About Thinoda")).clicked() {
                *cmd = Some(Cmd::About);
            }
        });
        ui.separator();
        if item(ui, t("Quit"), "Ctrl+Q").clicked() {
            *cmd = Some(Cmd::Quit);
        }
    }

    fn marker_menu(&mut self, ui: &mut Ui, cmd: &mut Option<Cmd>) {
        ui.set_max_width(320.0);
        for g in markers::GROUPS {
            ui.label(RichText::new(t(g.name)).small().weak());
            ui.horizontal_wrapped(|ui| {
                for m in g.markers {
                    if ui
                        .button(m.glyph)
                        .accessible_name(t(m.name))
                        .on_hover_text(t(m.name))
                        .clicked()
                    {
                        *cmd = Some(Cmd::Canvas(Action::ToggleMarker(m.id.to_string())));
                    }
                }
            });
        }
    }

    // ----- center -------------------------------------------------------------------------------

    fn editing_toolbar(&mut self, ctx: &Context, canvas: egui::Rect) {
        let Some(d) = self.doc() else { return };
        let has_selection = !d.view.selection.is_empty();
        let single = d.view.selection.len() == 1;
        let mut cmd = None;
        egui::Area::new(egui::Id::unique("thinoda_editing_toolbar"))
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::CENTER_TOP)
            .fixed_pos(canvas.center_top() + vec2(0.0, 12.0))
            .constrain_to(canvas)
            .movable(false)
            .show(ctx, |ui| {
                widgets::card(ui).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add_enabled_ui(has_selection, |ui| {
                            for (icon, label, action) in [
                                (icons::TOPIC, "Topic", Action::InsertSibling),
                                (icons::SUBTOPIC, "Subtopic", Action::InsertChild),
                                (
                                    icons::RELATIONSHIP,
                                    "Relationship",
                                    Action::StartRelationship,
                                ),
                                (icons::BOUNDARY, "Boundary", Action::AddBoundary),
                                (icons::SUMMARY, "Summary", Action::AddSummary),
                            ] {
                                if widgets::icon_button(ui, icon, t(label), false).clicked() {
                                    cmd = Some(Cmd::Canvas(action));
                                }
                            }
                        });
                        widgets::vdivider(ui);
                        ui.menu_button(icons::MARKER, |ui| self.marker_menu(ui, &mut cmd))
                            .response
                            .accessible_name(t("Marker"))
                            .on_hover_text(t("Marker"));
                        ui.menu_button(format!("{} {}", icons::ADD, icons::CHEVRON_DOWN), |ui| {
                            ui.set_min_width(190.0);
                            for (icon, label, enabled, command) in [
                                (
                                    icons::NOTE,
                                    "Notes",
                                    single,
                                    Cmd::EditField(FocusField::Notes),
                                ),
                                (
                                    icons::LINK,
                                    "Hyperlink",
                                    single,
                                    Cmd::EditField(FocusField::Hyperlink),
                                ),
                                (
                                    icons::LABEL,
                                    "Labels",
                                    self.command_available(&Cmd::Labels),
                                    Cmd::Labels,
                                ),
                                (
                                    icons::TODO,
                                    "To-Do",
                                    has_selection,
                                    Cmd::Canvas(Action::ToggleTodo),
                                ),
                                (icons::FORMAT, "Topic style", has_selection, Cmd::TopicStyle),
                            ] {
                                if ui
                                    .add_enabled(
                                        enabled,
                                        egui::Button::new(format!("{icon} {}", t(label))),
                                    )
                                    .accessible_name(t(label))
                                    .clicked()
                                {
                                    cmd = Some(command);
                                    ui.close();
                                }
                            }
                            ui.separator();
                            if ui.button(t("Find and Replace…")).clicked() {
                                cmd = Some(Cmd::FindReplace);
                                ui.close();
                            }
                        })
                        .response
                        .accessible_name(t("More insert actions"))
                        .on_hover_text(t("More insert actions"));
                    });
                });
            });
        if let Some(command) = cmd {
            self.exec(ctx, command);
        }
    }

    fn map(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if self.map_shot.active()
            || self.printing.active()
            || self.label_editor.is_some()
            || self.quick_style_dialog.is_some()
            || self.theme_dialog.is_some()
            || self.dialogs.guide.open
            || self.dialogs.shortcuts
            || self.dialogs.legal.open
        {
            if let Some(d) = self.docs.get_mut(self.active) {
                MapCanvas::show_snapshot(ui, &d.doc, &mut d.view);
            }
            return;
        }
        let paste = std::mem::take(&mut self.paste_requested);
        let Some(d) = self.docs.get_mut(self.active) else {
            return;
        };
        d.view.add_control_labels.child = t("Add child topic").into();
        d.view.add_control_labels.sibling = t("Add sibling topic").into();
        d.view.relationship_handle_labels = [
            "Relationship start",
            "First curve control",
            "Second curve control",
            "Relationship end",
        ]
        .map(|label| t(label).to_string());
        d.view.show_add_controls = self.settings.show_topic_add_buttons;
        if paste {
            // Menu "Paste": the OS text arrives as an event this frame and is handled by the canvas.
            ctx.memory_mut(|m| m.request_focus(MapCanvas::id()));
        }
        let out = MapCanvas::show(ui, &mut d.doc, &mut d.view);
        if let Some(text) = d.view.clipboard_out.take() {
            ctx.copy_text(text);
        }
        if let Some(link) = out.open_link {
            self.open_link(&link);
        }
        if let Some(error) = out.error {
            self.toasts
                .warning(format!("{}: {error}", t("Not possible here")));
        }
        let mut cmd = None;
        out.response
            .context_menu(|ui| self.context_menu(ui, &mut cmd));
        if let Some(c) = cmd {
            self.exec(&ctx, c);
        }
    }

    fn topic_toolbar(&mut self, ctx: &Context) {
        if !self.settings.show_topic_toolbar {
            return;
        }
        let Some(d) = self.doc() else { return };
        if d.view.selection.len() != 1
            || d.view.editing_topic().is_some()
            || d.view.relationship_from.is_some()
        {
            return;
        }
        let Some(rect) = d.view.selected_topic_rect(&d.doc) else {
            return;
        };
        let canvas = d.view.canvas_rect();
        let topic = d.view.primary().and_then(|id| d.doc.sheet().topic(id));
        let has_link = topic.is_some_and(|topic| topic.hyperlink.is_some());
        let todo = topic.is_some_and(|topic| topic.todo.is_some());
        let below = rect.top() - 52.0 < canvas.top();
        let position = egui::pos2(
            rect.center().x,
            if below {
                rect.bottom() + 12.0
            } else {
                rect.top() - 12.0
            },
        );
        let mut cmd = None;
        egui::Area::new(egui::Id::unique("thinoda_topic_toolbar"))
            .order(egui::Order::Foreground)
            .pivot(if below {
                egui::Align2::CENTER_TOP
            } else {
                egui::Align2::CENTER_BOTTOM
            })
            .fixed_pos(position)
            .constrain_to(canvas.shrink(4.0))
            .movable(false)
            .show(ctx, |ui| {
                widgets::card(ui).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if self.settings.ai.enabled {
                            if ui.button(t("Expand ideas")).clicked() {
                                cmd = Some(Cmd::Ai(crate::ai::AiCommand::Brainstorm));
                            }
                            widgets::vdivider(ui);
                        }
                        for (icon, label, selected, command) in [
                            (
                                icons::SUBTOPIC,
                                "Add subtopic",
                                false,
                                Cmd::Canvas(Action::InsertChild),
                            ),
                            (
                                icons::RELATIONSHIP,
                                "Connect topics",
                                false,
                                Cmd::Canvas(Action::StartRelationship),
                            ),
                            (
                                icons::LINK,
                                "Edit hyperlink",
                                has_link,
                                Cmd::EditField(FocusField::Hyperlink),
                            ),
                            (
                                icons::NOTE,
                                "Edit notes",
                                false,
                                Cmd::EditField(FocusField::Notes),
                            ),
                            (icons::LABEL, "Edit labels", false, Cmd::Labels),
                            (
                                icons::TODO,
                                "Toggle to-do",
                                todo,
                                Cmd::Canvas(Action::ToggleTodo),
                            ),
                            (icons::FORMAT, "Topic style", false, Cmd::TopicStyle),
                        ] {
                            if widgets::icon_button(ui, icon, t(label), selected).clicked() {
                                cmd = Some(command);
                            }
                        }
                    });
                });
            });
        if let Some(command) = cmd {
            self.exec(ctx, command);
        }
    }

    fn context_menu(&mut self, ui: &mut Ui, cmd: &mut Option<Cmd>) {
        if let Some(object) = self.doc().and_then(|d| d.view.selected_object) {
            let label = match object {
                thinoda_canvas::ObjectSelection::Relationship(_) => "Delete Relationship",
                thinoda_canvas::ObjectSelection::Boundary(_) => "Delete Boundary",
                thinoda_canvas::ObjectSelection::Summary(_) => "Delete Summary",
            };
            if matches!(object, thinoda_canvas::ObjectSelection::Summary(_)) {
                widgets::caption(
                    ui,
                    t("Deletes the summary topic and its children, not the covered branches."),
                );
            }
            if ui.button(t(label)).clicked() {
                *cmd = Some(Cmd::Canvas(Action::Delete));
                ui.close();
            }
            return;
        }
        let has_sel = self.doc().is_some_and(|d| !d.view.selection.is_empty());
        let single = self.doc().is_some_and(|d| d.view.selection.len() == 1);
        let non_root = self
            .doc()
            .is_some_and(|d| has_sel && !d.view.selection.contains(&d.doc.sheet().root));
        let summary_topic = self.doc().is_some_and(|d| {
            single
                && d.doc
                    .sheet()
                    .summaries
                    .iter()
                    .any(|s| Some(s.topic) == d.view.primary())
        });
        let has_parent = self.doc().is_some_and(|d| {
            single
                && d.view
                    .primary()
                    .and_then(|id| d.doc.sheet().parent(id))
                    .is_some()
        });
        let has_children = self.doc().is_some_and(|d| {
            d.view
                .selection
                .iter()
                .any(|id| !d.doc.sheet().children(*id).is_empty())
        });
        let has_link = self.doc().is_some_and(|d| {
            d.view
                .primary()
                .and_then(|id| d.doc.sheet().topic(id))
                .is_some_and(|t| t.hyperlink.is_some())
        });
        let mut item = |ui: &mut Ui, label: &str, sc: &str, c: Cmd, enabled: bool| {
            if ui
                .add_enabled(enabled, egui::Button::new(label).shortcut_text(sc))
                .accessible_name(label)
                .clicked()
            {
                *cmd = Some(c);
                ui.close();
            }
        };
        item(
            ui,
            t("Topic"),
            "Enter",
            Cmd::Canvas(Action::InsertSibling),
            has_sel,
        );
        item(
            ui,
            t("Subtopic"),
            "Tab",
            Cmd::Canvas(Action::InsertChild),
            has_sel,
        );
        item(
            ui,
            t("Parent Topic"),
            "Ctrl+Enter",
            Cmd::Canvas(Action::InsertParent),
            single && non_root,
        );
        ui.separator();
        item(
            ui,
            t("Relationship"),
            "Ctrl+Shift+L",
            Cmd::Canvas(Action::StartRelationship),
            single,
        );
        item(
            ui,
            t("Summary"),
            "Ctrl+]",
            Cmd::Canvas(Action::AddSummary),
            has_sel,
        );
        item(
            ui,
            t("Boundary"),
            "Ctrl+B",
            Cmd::Canvas(Action::AddBoundary),
            has_sel,
        );
        item(ui, t("To-Do"), "", Cmd::Canvas(Action::ToggleTodo), has_sel);
        if single {
            item(
                ui,
                t("Edit hyperlink"),
                "Ctrl+K",
                Cmd::EditField(FocusField::Hyperlink),
                true,
            );
            item(
                ui,
                t("Edit notes"),
                "Ctrl+Shift+N",
                Cmd::EditField(FocusField::Notes),
                true,
            );
            item(ui, t("Topic style"), "", Cmd::TopicStyle, true);
            item(
                ui,
                t("Labels"),
                "",
                Cmd::Labels,
                self.command_available(&Cmd::Labels),
            );
        }
        for (label, shortcut, command) in [
            ("Indent", "", Cmd::Canvas(Action::Indent)),
            ("Outdent", "", Cmd::Canvas(Action::Outdent)),
            ("Copy Style", "Ctrl+Alt+C", Cmd::CopyStyle),
            ("Paste Style", "Ctrl+Alt+V", Cmd::PasteStyle),
            ("Reset Style", "", Cmd::ResetStyle),
            ("Save Quick Style…", "", Cmd::SaveQuickStyle),
            ("New Sheet from Topic", "", Cmd::NewSheetFromTopic),
        ] {
            let available = self.command_available(&command);
            item(ui, t(label), shortcut, command, available);
        }
        ui.menu_button(t("Select"), |ui| {
            for (action, label) in Self::selection_commands() {
                let command = Cmd::Canvas(action);
                let available = self.command_available(&command);
                item(ui, t(label), "", command, available);
            }
        });
        ui.menu_button(t("Arrange"), |ui| {
            for (action, label) in Self::arrangement_commands() {
                let command = Cmd::Canvas(action);
                let available = self.command_available(&command);
                item(ui, t(label), "", command, available);
            }
        });
        if self.settings.ai.enabled {
            ui.menu_button(format!("✨ {}", t("AI")), |ui| {
                for a in crate::ai::AiCommand::ALL {
                    let sc = if a == crate::ai::AiCommand::Brainstorm {
                        "Ctrl+Shift+I"
                    } else {
                        ""
                    };
                    item(
                        ui,
                        t(a.label()),
                        sc,
                        Cmd::Ai(a),
                        has_sel || !a.needs_topic(),
                    );
                }
            });
        }
        if has_link {
            item(ui, t("Open Link"), "", Cmd::OpenLink, true);
        }
        ui.separator();
        item(ui, t("Cut"), "Ctrl+X", Cmd::Canvas(Action::Cut), non_root);
        item(ui, t("Copy"), "Ctrl+C", Cmd::Canvas(Action::Copy), has_sel);
        item(ui, t("Paste"), "Ctrl+V", Cmd::MenuPaste, true);
        item(
            ui,
            t("Duplicate"),
            "Ctrl+D",
            Cmd::Canvas(Action::Duplicate),
            non_root,
        );
        if summary_topic {
            widgets::caption(
                ui,
                t("Deletes the summary topic and its children, not the covered branches."),
            );
        }
        item(
            ui,
            t(if summary_topic {
                "Delete Summary"
            } else {
                "Delete"
            }),
            "Del",
            Cmd::Canvas(Action::Delete),
            non_root,
        );
        item(
            ui,
            t("Delete Topic Only"),
            "Ctrl+Del",
            Cmd::Canvas(Action::DeleteKeepChildren),
            has_parent,
        );
        ui.separator();
        item(
            ui,
            t("Collapse / Expand"),
            "Ctrl+/",
            Cmd::Canvas(Action::ToggleCollapse),
            has_children,
        );
        item(
            ui,
            t("Expand All"),
            "",
            Cmd::Canvas(Action::ExpandAll),
            true,
        );
        item(
            ui,
            t("Collapse All"),
            "",
            Cmd::Canvas(Action::CollapseAll),
            true,
        );
        item(
            ui,
            t("Fit Map"),
            "Ctrl+Shift+H",
            Cmd::Canvas(Action::Fit),
            true,
        );
    }

    fn home(&mut self, ui: &mut Ui) {
        let mut cmd = None;
        let p = Palette::of_ui(ui);
        egui::Frame::new()
            .fill(p.bg)
            .inner_margin(Margin::same(32))
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                ui.heading(RichText::new(t("Create a new map")).size(22.0).strong());
                let previous = self.template_category;
                ui.horizontal_wrapped(|ui| {
                    egui::ComboBox::from_id_salt("template_category")
                        .selected_text(
                            self.template_category
                                .map_or(t("All"), |category| t(category.label())),
                        )
                        .width(ui.available_width().min(360.0))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.template_category, None, t("All"));
                            for category in templates::TemplateCategory::ALL {
                                ui.selectable_value(
                                    &mut self.template_category,
                                    Some(category),
                                    t(category.label()),
                                );
                            }
                        })
                        .response
                        .accessible_name(t("Template Category"));
                    if self.template_category.is_some() && ui.button(t("All templates")).clicked() {
                        self.template_category = None;
                    }
                });
                if previous != self.template_category {
                    self.template_choice = None;
                }
                ui.add_space(12.0);
                egui::Panel::bottom("template_actions")
                    .exact_size(64.0)
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        ui.separator();
                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .add_enabled(
                                    self.template_choice.is_some(),
                                    egui::Button::new(t("Create")),
                                )
                                .clicked()
                            {
                                cmd = self.template_choice.map(Cmd::NewFromTemplate);
                            }
                            if ui.button(t("Cancel")).clicked() {
                                self.template_choice = None;
                                if let Some(index) = self.home_return_to.as_ref().and_then(|key| {
                                    self.docs.iter().position(|doc| &doc.key == key)
                                }) {
                                    self.active = index;
                                }
                            }
                            if let Some(template) = self.template_choice.and_then(|id| {
                                templates::TEMPLATES
                                    .iter()
                                    .find(|template| template.id == id)
                            }) {
                                ui.label(t(template.name));
                            } else {
                                widgets::caption(ui, t("Select a template, then choose Create."));
                            }
                        });
                    });
                egui::ScrollArea::vertical()
                    .id_salt((
                        "home_templates",
                        self.template_category
                            .map_or("all", |category| category.label()),
                    ))
                    .show(ui, |ui| {
                        if let Some(action) = crate::template_gallery::show(
                            ui,
                            self.template_category,
                            &mut self.template_choice,
                            &self.settings.recent_templates,
                        ) {
                            match action {
                                crate::template_gallery::GalleryAction::Navigate(category) => {
                                    self.template_category = Some(category);
                                    self.template_choice = None;
                                }
                                crate::template_gallery::GalleryAction::Create(id) => {
                                    cmd = Some(Cmd::NewFromTemplate(id));
                                }
                            }
                        }
                        ui.add_space(24.0);
                        ui.horizontal(|ui| {
                            ui.heading(RichText::new(t("Recent")).size(18.0).strong());
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if widgets::secondary_button(ui, t("Open File…")).clicked() {
                                    cmd = Some(Cmd::Open);
                                }
                            });
                        });
                        ui.add_space(6.0);
                        let recent: Vec<_> = self
                            .settings
                            .existing_recent()
                            .into_iter()
                            .cloned()
                            .collect();
                        if recent.is_empty() {
                            widgets::caption(ui, t("No recent files"));
                        }
                        for r in recent.iter().take(20) {
                            ui.horizontal(|ui| {
                                if ui
                                    .small_button(if r.pinned { icons::PIN } else { "·" })
                                    .clicked()
                                {
                                    self.settings.pin_recent(&r.path, !r.pinned);
                                    self.save_settings();
                                }
                                let name = r
                                    .path
                                    .file_name()
                                    .map(|s| s.to_string_lossy().into_owned())
                                    .unwrap_or_default();
                                if ui.link(name).clicked() {
                                    cmd = Some(Cmd::OpenPath(r.path.clone()));
                                }
                                let cloud = thinoda_platform::cloud::provider_for_path(
                                    &self.services.cloud,
                                    &r.path,
                                );
                                if let Some(c) = cloud {
                                    widgets::chip(
                                        ui,
                                        &format!("{} {}", icons::CLOUD, c.name),
                                        p.accent,
                                    );
                                }
                                widgets::caption(
                                    ui,
                                    r.path
                                        .parent()
                                        .map(|x| x.display().to_string())
                                        .unwrap_or_default(),
                                );
                            });
                        }
                    });
            });
        if let Some(c) = cmd {
            let ctx = ui.ctx().clone();
            self.exec(&ctx, c);
        }
    }

    fn outliner(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let p = Palette::of_ui(ui);
        let Some(d) = self.docs.get_mut(self.active) else {
            return;
        };
        let mut action: Option<Action> = None;
        let selected_title = d
            .view
            .primary()
            .and_then(|id| d.doc.sheet().topic(id))
            .map(|topic| topic.title.clone());
        ui.label(selected_title.as_ref().map_or_else(
            || t("Select a topic to edit").to_owned(),
            |title| format!("{}: {title}", t("Selected Topic")),
        ));
        let mut action_buttons = |ui: &mut Ui, buttons: &[(&'static str, Action)]| {
            for (label, command) in buttons {
                let enabled = command.available(&d.doc, &d.view);
                if ui
                    .add_enabled(enabled, egui::Button::new(t(label)))
                    .clicked()
                {
                    action = Some(command.clone());
                }
            }
        };
        ui.horizontal(|ui| {
            action_buttons(
                ui,
                &[
                    ("Add Sibling", Action::InsertSibling),
                    ("Add Child", Action::InsertChild),
                    ("Edit Topic", Action::StartEdit),
                    ("Move Up", Action::MoveUp),
                    ("Move Down", Action::MoveDown),
                ],
            );
        });
        ui.horizontal(|ui| {
            action_buttons(
                ui,
                &[
                    ("Indent", Action::Indent),
                    ("Outdent", Action::Outdent),
                    ("Delete Topic", Action::Delete),
                    ("Delete Topic Only", Action::DeleteKeepChildren),
                ],
            );
        });
        egui::Frame::new()
            .fill(p.bg)
            .inner_margin(Margin::symmetric(48, 24))
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        ui.set_max_width(820.0);
                        let sheet = d.doc.sheet().clone();
                        let mut rows: Vec<(TopicId, usize)> = Vec::new();
                        fn walk(
                            s: &thinoda_core::Sheet,
                            id: TopicId,
                            depth: usize,
                            out: &mut Vec<(TopicId, usize)>,
                        ) {
                            out.push((id, depth));
                            if s.topic(id).is_some_and(|t| t.collapsed) {
                                return;
                            }
                            for c in s.children(id) {
                                walk(s, *c, depth + 1, out);
                            }
                        }
                        for top in sheet.top_level_ids() {
                            walk(&sheet, top, 0, &mut rows);
                        }
                        for (id, depth) in rows {
                            let Some(topic) = sheet.topic(id) else {
                                continue;
                            };
                            ui.horizontal(|ui| {
                                ui.add_space(depth as f32 * 22.0);
                                let has_kids = !sheet.children(id).is_empty();
                                let chevron = if topic.collapsed {
                                    icons::CHEVRON_RIGHT
                                } else {
                                    icons::CHEVRON_DOWN
                                };
                                if has_kids {
                                    if ui
                                        .add(
                                            egui::Button::new(chevron)
                                                .frame(false)
                                                .min_size(vec2(16.0, 16.0)),
                                        )
                                        .accessible_name(format!(
                                            "{}: {}",
                                            t("Collapse / Expand"),
                                            topic.title
                                        ))
                                        .clicked()
                                    {
                                        d.view.select_only(id);
                                        action = Some(Action::ToggleCollapse);
                                    }
                                } else {
                                    ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                                }
                                if let Some(todo) = topic.todo {
                                    ui.label(if todo { "☑" } else { "☐" });
                                }
                                for m in &topic.markers {
                                    ui.label(markers::glyph(m));
                                }
                                if ui.is_enabled() && d.view.editing_topic() == Some(id) {
                                    let mut text =
                                        d.view.editing_text().unwrap_or_default().to_string();
                                    let te_id = egui::Id::unique(("outliner_edit", id));
                                    let r = ui
                                        .add(
                                            TextEdit::singleline(&mut text)
                                                .id(te_id)
                                                .hint_text(t("Topic title"))
                                                .desired_width(f32::INFINITY),
                                        )
                                        .accessible_name(t("Topic title"));
                                    if !r.has_focus() && !r.lost_focus() {
                                        r.request_focus();
                                    }
                                    d.view.set_editing_text(&text);
                                    let (enter, tab, esc) = ui.input_mut(|i| {
                                        (
                                            i.consume_key(Modifiers::NONE, Key::Enter),
                                            i.consume_key(Modifiers::NONE, Key::Tab),
                                            i.consume_key(Modifiers::NONE, Key::Escape),
                                        )
                                    });
                                    if enter || tab || esc || r.lost_focus() {
                                        d.view.commit_edit(&mut d.doc);
                                        if enter {
                                            action = Some(Action::InsertSibling);
                                        } else if tab {
                                            action = Some(Action::InsertChild);
                                        }
                                    }
                                } else {
                                    let selected = d.view.selection.contains(&id);
                                    let title = if topic.title.is_empty() {
                                        " "
                                    } else {
                                        topic.title.as_str()
                                    };
                                    let text = if depth == 0 {
                                        RichText::new(title).size(18.0).strong()
                                    } else {
                                        RichText::new(title)
                                    };
                                    let r = ui.selectable_label(selected, text);
                                    if r.clicked() {
                                        d.view.select_only(id);
                                    }
                                    if r.double_clicked() {
                                        d.view.select_only(id);
                                        d.view.begin_edit(&sheet, id, false);
                                    }
                                }
                                if topic.notes.is_some() {
                                    ui.label(RichText::new(icons::NOTE).weak());
                                }
                            });
                        }
                    });
            });
        // Keyboard in outliner when not editing.
        if ui.is_enabled()
            && d.view.editing_topic().is_none()
            && !ctx.text_edit_focused()
            && !egui::Popup::is_any_open(&ctx)
        {
            let keys = [
                (Modifiers::NONE, Key::Enter, Action::InsertSibling),
                (Modifiers::NONE, Key::Tab, Action::InsertChild),
                (Modifiers::NONE, Key::Delete, Action::Delete),
                (Modifiers::NONE, Key::F2, Action::StartEdit),
                (Modifiers::ALT, Key::ArrowUp, Action::MoveUp),
                (Modifiers::ALT, Key::ArrowDown, Action::MoveDown),
                (Modifiers::COMMAND, Key::Z, Action::Undo),
                (Modifiers::COMMAND, Key::Y, Action::Redo),
            ];
            for (m, k, a) in keys {
                if ctx.input_mut(|i| i.consume_key(m, k)) {
                    action = Some(a);
                    break;
                }
            }
            if action.is_none() {
                let (up, down) = ctx.input_mut(|i| {
                    (
                        i.consume_key(Modifiers::NONE, Key::ArrowUp),
                        i.consume_key(Modifiers::NONE, Key::ArrowDown),
                    )
                });
                if (up || down)
                    && let Some(cur) = d.view.primary()
                {
                    let sheet = d.doc.sheet();
                    let order: Vec<TopicId> = sheet
                        .all_ids_ordered()
                        .into_iter()
                        .filter(|x| sheet.is_visible(*x))
                        .collect();
                    if let Some(i) = order.iter().position(|x| *x == cur) {
                        let j = if up {
                            i.saturating_sub(1)
                        } else {
                            (i + 1).min(order.len() - 1)
                        };
                        d.view.select_only(order[j]);
                    }
                }
            }
        }
        if let Some(a) = action
            && let Err(_e) = thinoda_canvas::apply_action(&mut d.doc, &mut d.view, a)
        {
            self.toasts.warning(t("Not possible here"));
        }
    }

    // ----- bottom bar ---------------------------------------------------------------------------

    fn bottom_bar(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let mut cmd = None;
        let Some(d) = self.docs.get_mut(self.active) else {
            return;
        };
        ui.horizontal_centered(|ui| {
            // Sheet tabs.
            let n = d.doc.workbook().sheets.len();
            for i in 0..n {
                let title = d.doc.workbook().sheets[i].title.clone();
                let r = ui.selectable_label(i == d.doc.active_index(), &title);
                if r.clicked() && i != d.doc.active_index() {
                    d.view.commit_edit(&mut d.doc);
                    d.doc.set_active(i);
                    d.view.reset();
                }
                if r.double_clicked() {
                    if i != d.doc.active_index() {
                        d.view.commit_edit(&mut d.doc);
                        d.doc.set_active(i);
                        d.view.reset();
                    }
                    cmd = Some(Cmd::RenameSheet);
                }
                r.context_menu(|ui| {
                    if i != d.doc.active_index() {
                        d.view.commit_edit(&mut d.doc);
                        d.doc.set_active(i);
                        d.view.reset();
                    }
                    if ui.button(t("Rename Sheet")).clicked() {
                        d.doc.set_active(i);
                        cmd = Some(Cmd::RenameSheet);
                    }
                    if ui.button(t("Duplicate Sheet")).clicked() {
                        d.doc.set_active(i);
                        cmd = Some(Cmd::DuplicateSheet);
                    }
                    for (label, command) in [
                        ("Copy Sheet", Cmd::CopySheet),
                        ("Paste Sheet", Cmd::PasteSheet),
                        ("Save Sheet As…", Cmd::SaveSheetAs),
                        ("New Sheet", Cmd::NewSheet),
                    ] {
                        if ui.button(t(label)).clicked() {
                            cmd = Some(command);
                        }
                    }
                    if ui
                        .add_enabled(i > 0, egui::Button::new(t("Move Left")))
                        .clicked()
                    {
                        d.view.commit_edit(&mut d.doc);
                        if let Err(e) = d.doc.move_sheet(i, i - 1) {
                            self.toasts
                                .error(format!("{}: {e}", t("Not possible here")));
                        }
                        d.view.reset();
                    }
                    if ui
                        .add_enabled(i + 1 < n, egui::Button::new(t("Move Right")))
                        .clicked()
                    {
                        d.view.commit_edit(&mut d.doc);
                        if let Err(e) = d.doc.move_sheet(i, i + 1) {
                            self.toasts
                                .error(format!("{}: {e}", t("Not possible here")));
                        }
                        d.view.reset();
                    }
                    if ui
                        .add_enabled(n > 1, egui::Button::new(t("Delete Sheet")))
                        .clicked()
                    {
                        d.doc.set_active(i);
                        cmd = Some(Cmd::DeleteSheet);
                    }
                });
            }
            if ui
                .small_button(icons::ADD)
                .on_hover_text(t("New Sheet"))
                .clicked()
            {
                cmd = Some(Cmd::NewSheet);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if !self.settings.show_status_bar {
                    return;
                }
                if ui
                    .small_button(icons::FIT)
                    .on_hover_text(t("Fit Map"))
                    .clicked()
                {
                    cmd = Some(Cmd::Canvas(Action::Fit));
                }
                if ui.small_button("+").on_hover_text(t("Zoom In")).clicked() {
                    cmd = Some(Cmd::Canvas(Action::ZoomIn));
                }
                let current = d.view.zoom_percent();
                ui.menu_button(format!("{current}%"), |ui| {
                    let mut presets = vec![500, 400, 300, 200, 150, 120, 100, 80, 50, 20, 10];
                    if !presets.contains(&current) {
                        presets.push(current);
                        presets.sort_unstable_by(|a, b| b.cmp(a));
                    }
                    for percent in presets {
                        if ui
                            .selectable_label(current == percent, format!("{percent}%"))
                            .clicked()
                        {
                            d.view.set_zoom(percent as f32 / 100.0);
                            ui.close();
                        }
                    }
                    ui.separator();
                    if ui.button(t("Fit Map")).clicked() {
                        cmd = Some(Cmd::Canvas(Action::Fit));
                        ui.close();
                    }
                })
                .response
                .accessible_name(t("Zoom"));
                if ui.small_button("−").on_hover_text(t("Zoom Out")).clicked() {
                    cmd = Some(Cmd::Canvas(Action::ZoomOut));
                }
                widgets::vdivider(ui);
                let sheet = d.doc.sheet();
                widgets::caption(
                    ui,
                    format!(
                        "{}: {}",
                        t("Words"),
                        thinoda_core::search::word_count(sheet)
                    ),
                );
                ui.menu_button(format!("{}: {}", t("Topics"), sheet.topics.len()), |ui| {
                    ui.strong(t("Current sheet"));
                    for (label, value) in [
                        ("Topics", sheet.topics.len()),
                        ("Words", thinoda_core::search::word_count(sheet)),
                        (
                            "Characters",
                            sheet
                                .topics
                                .values()
                                .map(|topic| topic.title.chars().count())
                                .sum(),
                        ),
                    ] {
                        ui.label(format!("{}: {value}", t(label)));
                    }
                    ui.separator();
                    widgets::caption(
                        ui,
                        t("Topic titles only, including collapsed and floating topics."),
                    );
                    widgets::caption(
                        ui,
                        t("Words split at whitespace; characters count Unicode scalar values."),
                    );
                })
                .response
                .accessible_name(t("Statistics"));
                widgets::vdivider(ui);
                if let Some(c) = d.path.as_ref().and_then(|p| {
                    thinoda_platform::cloud::provider_for_path(&self.services.cloud, p)
                }) {
                    widgets::caption(ui, format!("{} {}", icons::CLOUD, c.name));
                }
                widgets::caption(
                    ui,
                    if d.is_dirty() {
                        t("Unsaved changes")
                    } else {
                        t("Saved")
                    },
                );
            });
        });
        if let Some(c) = cmd {
            self.exec(&ctx, c);
        }
    }

    // ----- format panel -------------------------------------------------------------------------

    fn sidebar(&mut self, ui: &mut Ui) {
        match self.sidebar_family {
            SidebarFamily::Format => self.format_panel(ui),
            SidebarFamily::Symbols => {
                ui.heading(t("Symbols"));
                ui.add_space(8.0);
                egui::ScrollArea::vertical()
                    .id_salt("symbols_scroll")
                    .show(ui, |ui| self.symbols_panel(ui));
            }
        }
    }

    fn symbols_panel(&mut self, ui: &mut Ui) {
        let selection = self
            .doc()
            .map(|d| d.view.selection.clone())
            .unwrap_or_default();
        if selection.is_empty() {
            widgets::caption(ui, t("Select a topic to add symbols."));
        } else {
            widgets::caption(ui, t("Click a symbol to add or remove it. One per group."));
        }
        let mut command = None;
        for group in markers::GROUPS {
            widgets::section(ui, t(group.name), true, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for marker in group.markers {
                        let selected = !selection.is_empty()
                            && self.doc().is_some_and(|d| {
                                selection.iter().all(|id| {
                                    d.doc.sheet().topic(*id).is_some_and(|topic| {
                                        topic.markers.iter().any(|m| m == marker.id)
                                    })
                                })
                            });
                        let response = ui
                            .add_enabled(
                                !selection.is_empty(),
                                egui::Button::new(RichText::new(marker.glyph).size(20.0))
                                    .selected(selected)
                                    .min_size(vec2(32.0, 32.0)),
                            )
                            .accessible_name(t(marker.name))
                            .on_hover_text(t(marker.name));
                        if response.clicked() {
                            command = Some(Cmd::Canvas(Action::ToggleMarker(marker.id.into())));
                        }
                    }
                });
            });
        }
        if let Some(command) = command {
            let ctx = ui.ctx().clone();
            self.exec(&ctx, command);
        }
    }

    fn format_panel(&mut self, ui: &mut Ui) {
        let mut tab = self.format_tab;
        widgets::segmented(
            ui,
            &mut tab,
            &[(FormatTab::Style, t("Style")), (FormatTab::Map, t("Map"))],
        );
        self.format_tab = tab;
        ui.add_space(8.0);
        egui::ScrollArea::vertical()
            .id_salt(match tab {
                FormatTab::Style => "style_scroll",
                FormatTab::Map => "map_scroll",
            })
            .show(ui, |ui| {
                ui.push_id(
                    match tab {
                        FormatTab::Style => "style_content",
                        FormatTab::Map => "map_content",
                    },
                    |ui| match tab {
                        FormatTab::Style => self.style_tab(ui),
                        FormatTab::Map => self.map_tab(ui),
                    },
                );
            });
    }

    fn style_tab(&mut self, ui: &mut Ui) {
        if let Some(thinoda_canvas::ObjectSelection::Relationship(id)) =
            self.doc().and_then(|d| d.view.selected_object)
        {
            self.relationship_style(ui, id);
            return;
        }
        self.quick_styles(ui);
        let Some(d) = self.docs.get_mut(self.active) else {
            return;
        };
        let sel = d.view.selection.clone();
        let Some(&primary) = sel.last() else {
            widgets::caption(ui, t("Select a topic to edit its style."));
            return;
        };
        let sheet = d.doc.sheet();
        let Some(topic) = sheet.topic(primary).cloned() else {
            return;
        };
        let resolved = thinoda_core::theme::resolve_style(sheet, primary, d.view.dark);
        let can_set_branch_side = sel.iter().all(|id| sheet.parent(*id) == Some(sheet.root))
            && matches!(
                sheet.structure,
                Structure::MapClockwise | Structure::MapAnticlockwise | Structure::MapUnbalanced
            );
        let mut change: Option<StyleChange> = None;
        ui.label(RichText::new(&topic.title).strong());
        if sel.len() > 1 {
            widgets::caption(
                ui,
                t("Showing the primary topic; edits apply to all selected topics."),
            );
        }

        widgets::section(ui, t("Structure"), true, |ui| {
            if can_set_branch_side {
                let mut side = topic.branch_side;
                widgets::property_field(ui, t("Branch side"), |ui| {
                    egui::ComboBox::from_id_salt(("branch_side", primary))
                        .selected_text(t(match side {
                            None => "Auto",
                            Some(thinoda_core::BranchSide::Left) => "Left",
                            Some(thinoda_core::BranchSide::Right) => "Right",
                        }))
                        .show_ui(ui, |ui| {
                            for (value, label) in [
                                (None, "Auto"),
                                (Some(thinoda_core::BranchSide::Left), "Left"),
                                (Some(thinoda_core::BranchSide::Right), "Right"),
                            ] {
                                ui.selectable_value(&mut side, value, t(label));
                            }
                        })
                        .response
                });
                if side != topic.branch_side {
                    let result = d.doc.edit_sheet("Branch side", |s| {
                        for id in &sel {
                            s.set_branch_side(*id, side)?;
                        }
                        Ok(())
                    });
                    if let Err(error) = result {
                        self.toasts
                            .warning(format!("{}: {error}", t("Not possible here")));
                    }
                }
            }
            let cur = topic.structure;
            let mut chosen = None;
            widgets::property_field(ui, t("Structure"), |ui| {
                egui::ComboBox::from_id_salt("topic_structure")
                    .selected_text(cur.map_or(t("Auto"), |s| t(structure_label(s))))
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(cur.is_none(), t("Auto")).clicked() {
                            chosen = Some(None);
                        }
                        for s in Structure::ALL {
                            if ui
                                .selectable_label(cur == Some(s), t(structure_label(s)))
                                .clicked()
                            {
                                chosen = Some(Some(s));
                            }
                        }
                    })
                    .response
            });
            if let Some(s) = chosen {
                let _ = d
                    .doc
                    .edit_sheet("Structure", |sh| sh.set_structure(Some(primary), s));
            }
        });

        widgets::section(ui, t("Shape"), true, |ui| {
            let mut shape = resolved.shape;
            widgets::property_field(ui, t("Shape"), |ui| {
                egui::ComboBox::from_id_salt("topic_shape")
                    .selected_text(t(shape.label()))
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for s in TopicShape::ALL {
                            ui.selectable_value(&mut shape, s, t(s.label()));
                        }
                    })
                    .response
            });
            if shape != resolved.shape {
                change = Some(("Shape", Box::new(move |st| st.shape = Some(shape))));
            }
            widgets::property_row(ui, t("Fill"), |ui| {
                if resolved.fill.is_none() {
                    ui.label(t("None"));
                }
                if let Some(c) =
                    widgets::color_swatches(ui, &SWATCHES, resolved.fill.map(from_color))
                {
                    change = Some(("Fill", Box::new(move |st| st.fill = Some(to_color(c)))));
                }
            });
            widgets::property_row(ui, t("Border"), |ui| {
                if resolved.border_color.is_none() {
                    ui.label(t("None"));
                }
                if let Some(c) =
                    widgets::color_swatches(ui, &SWATCHES, resolved.border_color.map(from_color))
                {
                    change = Some((
                        "Border",
                        Box::new(move |st| {
                            st.border_color = Some(to_color(c));
                            if st.border_width.is_none_or(|w| w <= 0.0) {
                                st.border_width = Some(2.0);
                            }
                        }),
                    ));
                }
            });
            let mut bw = resolved.border_width;
            widgets::property_field(ui, t("Width"), |ui| {
                let r = ui.add(egui::Slider::new(&mut bw, 0.0..=8.0).step_by(0.5));
                if r.changed() {
                    change = Some(("Border", Box::new(move |st| st.border_width = Some(bw))));
                }
                r
            });
        });

        widgets::section(ui, t("Text"), true, |ui| {
            let mut family = topic.style.font_family.clone();
            widgets::property_field(ui, t("Font Family"), |ui| {
                font_selector(ui, &mut self.fonts, &mut family, t("Auto"))
            });
            if family != topic.style.font_family {
                change = Some((
                    "Font Family",
                    Box::new(move |st| st.font_family = family.clone()),
                ));
            }
            let mut size = resolved.font_size;
            widgets::property_field(ui, t("Font Size"), |ui| {
                let r = ui.add(egui::DragValue::new(&mut size).range(6.0..=96.0).speed(0.5));
                if r.changed() {
                    change = Some(("Font Size", Box::new(move |st| st.font_size = Some(size))));
                }
                r
            });
            ui.horizontal(|ui| {
                let toggles: [(&str, &str, bool, StyleToggle); 4] = [
                    (icons::BOLD, "Bold", resolved.bold, |s, v| s.bold = Some(v)),
                    (icons::ITALIC, "Italic", resolved.italic, |s, v| {
                        s.italic = Some(v)
                    }),
                    (icons::UNDERLINE, "Underline", resolved.underline, |s, v| {
                        s.underline = Some(v)
                    }),
                    (
                        icons::STRIKE,
                        "Strikethrough",
                        resolved.strikethrough,
                        |s, v| s.strikethrough = Some(v),
                    ),
                ];
                for (icon, label, on, set) in toggles {
                    if widgets::icon_button(ui, icon, t(label), on).clicked() {
                        change = Some(("Text Style", Box::new(move |st| set(st, !on))));
                    }
                }
                widgets::vdivider(ui);
                for (a, icon, label) in [
                    (TextAlign::Left, "⬅", "Align Left"),
                    (TextAlign::Center, "⬌", "Align Center"),
                    (TextAlign::Right, "➡", "Align Right"),
                ] {
                    if widgets::icon_button(ui, icon, t(label), resolved.align == a).clicked() {
                        change = Some(("Align", Box::new(move |st| st.align = Some(a))));
                    }
                }
            });
            widgets::property_row(ui, t("Text Color"), |ui| {
                if let Some(c) =
                    widgets::color_swatches(ui, &SWATCHES, Some(from_color(resolved.text_color)))
                {
                    change = Some((
                        "Text Color",
                        Box::new(move |st| st.text_color = Some(to_color(c))),
                    ));
                }
            });
        });

        widgets::section(ui, t("Branch"), true, |ui| {
            let mut ls = resolved.line_shape;
            widgets::property_field(ui, t("Line"), |ui| {
                egui::ComboBox::from_id_salt("line_shape")
                    .selected_text(t(ls.label()))
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for s in LineShape::ALL {
                            ui.selectable_value(&mut ls, s, t(s.label()));
                        }
                    })
                    .response
            });
            if ls != resolved.line_shape {
                change = Some(("Line", Box::new(move |st| st.line_shape = Some(ls))));
            }
            widgets::property_row(ui, t("Line Color"), |ui| {
                if let Some(c) =
                    widgets::color_swatches(ui, &SWATCHES, Some(from_color(resolved.line_color)))
                {
                    change = Some((
                        "Line Color",
                        Box::new(move |st| st.line_color = Some(to_color(c))),
                    ));
                }
            });
            let mut lw = resolved.line_width;
            widgets::property_field(ui, t("Line Width"), |ui| {
                let r = ui.add(egui::Slider::new(&mut lw, 0.5..=8.0).step_by(0.5));
                if r.changed() {
                    change = Some(("Line Width", Box::new(move |st| st.line_width = Some(lw))));
                }
                r
            });
        });

        if !topic.style.is_default() && ui.button(t("Reset Style")).clicked() {
            change = Some(("Reset Style", Box::new(|st| *st = TopicStyle::default())));
        }

        let focus = self.focus_field.take();
        let mut edit_labels = false;
        widgets::section(ui, t("Topic Properties"), true, |ui| {
            let l = ui.label(t("Hyperlink"));
            let current = topic.hyperlink.clone().unwrap_or_default();
            let (r, done) = buffered_text(ui, ("hyperlink", primary), &current, 0, "https://…");
            let r = r.labelled_by(l.id);
            if focus == Some(FocusField::Hyperlink) {
                r.request_focus();
                r.scroll_to_me(Some(egui::Align::Center));
            }
            if let Some(link) = done {
                let v = (!link.trim().is_empty()).then(|| link.trim().to_string());
                let _ = d
                    .doc
                    .edit_sheet("Hyperlink", |s| s.set_hyperlink(primary, v));
            }
            ui.label(t("Labels"));
            ui.horizontal_wrapped(|ui| {
                for label in &topic.labels {
                    ui.label(label);
                }
            });
            edit_labels = ui
                .add_enabled(sel.len() == 1, egui::Button::new(t("Edit labels")))
                .clicked();
            let l = ui.label(t("Notes"));
            let current = topic.notes.clone().unwrap_or_default();
            let (r, done) = buffered_text(ui, ("notes", primary), &current, 5, "");
            let r = r.labelled_by(l.id);
            if focus == Some(FocusField::Notes) {
                r.request_focus();
                r.scroll_to_me(Some(egui::Align::Center));
            }
            if let Some(notes) = done {
                let v = (!notes.trim().is_empty()).then_some(notes);
                if v != topic.notes {
                    let _ = d.doc.edit_sheet("Notes", |s| s.set_notes(primary, v));
                }
            }
            if !topic.markers.is_empty() {
                ui.label(t("Markers"));
                ui.horizontal_wrapped(|ui| {
                    for m in &topic.markers {
                        if ui.button(format!("{} ✖", markers::glyph(m))).clicked() {
                            let _ = d
                                .doc
                                .edit_sheet("Marker", |s| s.toggle_marker(&[primary], m));
                        }
                    }
                });
            }
        });

        if let Some((label, f)) = change {
            let _ = d
                .doc
                .edit_sheet(label, |s| s.update_style(&sel, |st| f(st)));
        }
        if edit_labels {
            self.exec(ui.ctx(), Cmd::Labels);
        }
    }

    fn relationship_style(&mut self, ui: &mut Ui, id: thinoda_core::RelationshipId) {
        let Some(d) = self.doc_mut() else { return };
        let Some(rel) = d
            .doc
            .sheet()
            .relationships
            .iter()
            .find(|r| r.id == id)
            .cloned()
        else {
            return;
        };
        ui.heading(t("Relationship"));
        ui.label(t(
            "Drag the round endpoints to reconnect. Drag square controls to reshape.",
        ));
        ui.label(t("Focused handles: arrows move controls by 1 map unit (Shift: 10); endpoint arrows cycle visible topics."));
        let salt = (d.key.clone(), d.doc.active_index(), id);
        let label = ui.label(t("Relationship title"));
        let (response, title) = buffered_text(ui, (&salt, "title"), &rel.title, 0, "");
        response.labelled_by(label.id);
        let mut from = rel.from;
        let mut to = rel.to;
        for (name, value, other) in [
            ("From topic", &mut from, rel.to),
            ("To topic", &mut to, rel.from),
        ] {
            widgets::property_field(ui, t(name), |ui| {
                egui::ComboBox::from_id_salt((&salt, name))
                    .selected_text(
                        d.doc
                            .sheet()
                            .topic(*value)
                            .map(|topic| topic.title.as_str())
                            .unwrap_or(""),
                    )
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for topic in d
                            .doc
                            .sheet()
                            .topics
                            .values()
                            .filter(|topic| topic.id != other)
                        {
                            ui.selectable_value(
                                value,
                                topic.id,
                                format!("{} ({})", topic.title, topic.id),
                            );
                        }
                    })
                    .response
            });
        }
        let mut color = None;
        widgets::property_row(ui, t("Line Color"), |ui| {
            color = widgets::color_swatches(ui, &SWATCHES, rel.color.map(from_color))
                .map(|color| Some(to_color(color)));
        });
        if rel.color.is_some() && ui.button(t("Automatic line color")).clicked() {
            color = Some(None);
        }
        let layout = d.view.layout_for(&d.doc);
        let points = rel.control_points.or_else(|| {
            let curve = layout.relationships.iter().find(|r| r.id == id)?;
            Some([
                curve.c1 - layout.topics[&rel.from].rect.center(),
                curve.c2 - layout.topics[&rel.to].rect.center(),
            ])
        });
        ui.label(t(
            "Curve coordinates are offsets from each endpoint topic center.",
        ));
        let mut changed_points = None;
        let mut parse_error = None;
        if let Some(mut points) = points {
            let mut edited = false;
            for (index, name) in [
                "First control X",
                "First control Y",
                "Second control X",
                "Second control Y",
            ]
            .into_iter()
            .enumerate()
            {
                let point = &mut points[index / 2];
                let value = if index % 2 == 0 {
                    &mut point.x
                } else {
                    &mut point.y
                };
                widgets::property_field(ui, t(name), |ui| {
                    let (response, done) =
                        buffered_text(ui, (&salt, index), &value.to_string(), 0, "");
                    if let Some(done) = done {
                        match done.trim().parse::<f32>() {
                            Ok(next) => {
                                *value = next;
                                edited = true;
                            }
                            Err(e) => parse_error = Some(e.to_string()),
                        }
                    }
                    response
                });
            }
            if edited {
                changed_points = Some(points);
            }
        } else {
            widgets::caption(
                ui,
                t("Expand both endpoints to edit automatic curve controls."),
            );
        }
        let reset = ui
            .add_enabled(
                rel.control_points.is_some(),
                egui::Button::new(t("Reset curve")),
            )
            .clicked();
        let changed = title.is_some()
            || color.is_some()
            || reset
            || changed_points.is_some()
            || from != rel.from
            || to != rel.to;
        let result = if changed {
            d.doc.edit_sheet("Edit Relationship", |s| {
                if let Some(title) = title {
                    s.set_relationship_title(id, &title)?;
                }
                if from != rel.from || to != rel.to {
                    s.reconnect_relationship(id, from, to)?;
                }
                if let Some(color) = color {
                    s.set_relationship_color(id, color)?;
                }
                if reset {
                    s.set_relationship_control_points(id, None)?;
                } else if let Some(points) = changed_points {
                    s.set_relationship_control_points(id, Some(points))?;
                }
                Ok(())
            })
        } else {
            Ok(())
        };
        if let Err(e) = result {
            self.toasts
                .warning(format!("{}: {e}", t("Not possible here")));
        }
        if let Some(e) = parse_error {
            self.toasts
                .warning(format!("{}: {e}", t("Invalid curve coordinate")));
        }
    }

    fn map_tab(&mut self, ui: &mut Ui) {
        self.custom_theme_gallery(ui);
        let Some(d) = self.docs.get_mut(self.active) else {
            return;
        };
        let sheet = d.doc.sheet();
        let mut ts = sheet.theme.clone();
        let structure = sheet.structure;
        let before = ts.clone();
        widgets::caption(ui, t("Map settings apply to the entire sheet."));

        widgets::section(ui, t("Structure"), true, |ui| {
            let (_, chosen) = crate::structure_picker::show(ui, structure);
            if let Some(s) = chosen.filter(|&s| s != structure) {
                d.view.commit_edit(&mut d.doc);
                match d
                    .doc
                    .edit_sheet("Structure", |sh| sh.set_structure(None, Some(s)))
                {
                    Ok(()) => d.view.request_fit(),
                    Err(error) => {
                        self.toasts.error(error.to_string());
                    }
                }
            }
        });

        widgets::section(ui, t("Theme"), true, |ui| {
            widgets::property_field(ui, t("Global Font"), |ui| {
                font_selector(ui, &mut self.fonts, &mut ts.font_family, t("Default"))
            });
            let columns = ((ui.available_width() + 8.0) / 126.0).floor().max(1.0) as usize;
            egui::Grid::new("sheet_themes")
                .num_columns(columns)
                .spacing(vec2(8.0, 8.0))
                .show(ui, |ui| {
                    for (index, id) in Theme::IDS.into_iter().enumerate() {
                        let th = Theme::builtin(id);
                        let selected = ts.custom_theme.is_none() && ts.theme_id == id;
                        let r = widgets::card(ui).show(ui, |ui| {
                            ui.set_width(104.0);
                            let (rect, _) =
                                ui.allocate_exact_size(vec2(100.0, 34.0), Sense::hover());
                            let painter = ui.painter();
                            painter.rect_filled(
                                rect,
                                CornerRadius::same(4),
                                from_color(th.light_bg),
                            );
                            let c = rect.center();
                            painter.rect_filled(
                                egui::Rect::from_center_size(c, vec2(30.0, 14.0)),
                                CornerRadius::same(3),
                                from_color(th.central_fill),
                            );
                            for (k, col) in th.palette.iter().take(4).enumerate() {
                                let x = if k % 2 == 0 {
                                    rect.left() + 14.0
                                } else {
                                    rect.right() - 14.0
                                };
                                let y = if k < 2 {
                                    rect.top() + 9.0
                                } else {
                                    rect.bottom() - 9.0
                                };
                                painter.rect_filled(
                                    egui::Rect::from_center_size(egui::pos2(x, y), vec2(18.0, 7.0)),
                                    CornerRadius::same(2),
                                    from_color(*col),
                                );
                            }
                            ui.label(RichText::new(t(th.name)).small());
                        });
                        let r =
                            ui.interact(r.response.rect, ui.scope_id().with(id), Sense::click());
                        r.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::Role::Button,
                                true,
                                selected,
                                t(th.name),
                            )
                        });
                        if selected {
                            ui.painter().rect_stroke(
                                r.rect,
                                CornerRadius::same(8),
                                Stroke::new(2.0, d.view.accent),
                                egui::StrokeKind::Outside,
                            );
                        }
                        if r.clicked() {
                            ts.theme_id = id.to_string();
                            ts.custom_theme = None;
                            ts.variant = 0;
                        }
                        if (index + 1) % columns == 0 {
                            ui.end_row();
                        }
                    }
                });
            let th = Theme::for_settings(&ts);
            widgets::property_field(ui, t("Color Variant"), |ui| {
                egui::ComboBox::from_id_salt("variant")
                    .selected_text(if ts.variant == 0 {
                        t("Multicolor").to_string()
                    } else {
                        format!("#{}", ts.variant)
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut ts.variant, 0, t("Multicolor"));
                        for v in 1..th.variant_count() {
                            ui.selectable_value(&mut ts.variant, v, format!("#{v}"));
                        }
                    })
                    .response
            });
            widgets::property_field(ui, t("Rainbow Branches"), |ui| {
                widgets::toggle(ui, &mut ts.rainbow, t("Rainbow Branches"))
            });
            widgets::property_field(ui, t("Tapered Lines"), |ui| {
                widgets::toggle(ui, &mut ts.tapered_lines, t("Tapered Lines"))
            });
            widgets::property_field(ui, t("Balanced Map"), |ui| {
                widgets::toggle(ui, &mut ts.balanced, t("Balanced Map"))
            });
        });

        widgets::section(ui, t("Branches"), true, |ui| {
            let mut automatic = ts.line_width.is_none();
            widgets::property_field(ui, t("Automatic branch width"), |ui| {
                widgets::toggle(ui, &mut automatic, t("Automatic branch width"))
            });
            if automatic {
                ts.line_width = None;
            } else {
                let width = ts.line_width.unwrap_or(1.5);
                ts.line_width = Some(width);
                widgets::property_field(ui, t("Branch Line Width"), |ui| {
                    let (response, done) = buffered_text(
                        ui,
                        (
                            &d.key,
                            d.doc.active_index(),
                            d.doc.state_id(),
                            "branch-width",
                        ),
                        &width.to_string(),
                        0,
                        "0.5-16",
                    );
                    if let Some(done) = done {
                        match done.trim().parse::<f32>() {
                            Ok(value) => ts.line_width = Some(value),
                            Err(error) => {
                                self.toasts
                                    .warning(format!("{}: {error}", t("Invalid branch width")));
                            }
                        }
                    }
                    response
                });
            }
        });

        widgets::section(ui, t("Advanced Layout"), true, |ui| {
            widgets::property_field(ui, t("Flexible Floating Topics"), |ui| {
                widgets::toggle(
                    ui,
                    &mut ts.flexible_floating_topics,
                    t("Flexible Floating Topics"),
                )
            });
            widgets::caption(
                ui,
                t(
                    "Place floating topics without attaching. Turn off to convert between attached and floating topics.",
                ),
            );
        });

        widgets::section(ui, t("Background"), true, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(ts.background.is_none(), t("Auto"))
                    .clicked()
                {
                    ts.background = None;
                }
            });
            let background = Theme::for_settings(&ts).background(&ts, d.view.dark);
            if let Some(c) = widgets::color_swatches(ui, &SWATCHES, Some(from_color(background))) {
                ts.background = Some(to_color(c));
            }
        });

        if ts != before {
            let result = d.doc.edit_sheet(t("Theme"), |s| {
                ts.validate().map_err(thinoda_core::EditError::Invalid)?;
                s.theme = ts;
                Ok(())
            });
            if let Err(error) = result {
                self.toasts.error(error.to_string());
            }
        }
    }

    // ----- search bar ---------------------------------------------------------------------------

    fn search_bar(&mut self, ctx: &Context) {
        let mut close = false;
        egui::Area::new(egui::Id::unique("thinoda_search"))
            .anchor(
                egui::Align2::RIGHT_TOP,
                vec2(
                    -(if self.settings.show_format_panel && !self.zen {
                        300.0
                    } else {
                        16.0
                    }),
                    56.0,
                ),
            )
            .show(ctx, |ui| {
                widgets::card(ui).show(ui, |ui| {
                    ui.set_width(340.0);
                    ui.horizontal(|ui| {
                        let r = ui
                            .allocate_ui(vec2(230.0, 28.0), |ui| {
                                widgets::search_field(ui, &mut self.search.query, t("Search"))
                            })
                            .inner;
                        if std::mem::take(&mut self.search.focus) {
                            r.request_focus();
                        }
                        if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                            let back = ui.input(|i| i.modifiers.shift);
                            self.goto_hit(if back { -1 } else { 1 });
                            r.request_focus();
                        }
                        if ui
                            .small_button("▲")
                            .accessible_name(t("Previous result"))
                            .clicked()
                        {
                            self.goto_hit(-1);
                        }
                        if ui
                            .small_button("▼")
                            .accessible_name(t("Next result"))
                            .clicked()
                        {
                            self.goto_hit(1);
                        }
                        if ui
                            .small_button(icons::CLOSE)
                            .accessible_name(t("Close search"))
                            .clicked()
                        {
                            close = true;
                        }
                    });
                    let n = self.search.hits.len();
                    widgets::caption(
                        ui,
                        if self.search.query.is_empty() {
                            String::new()
                        } else if n == 0 {
                            t("No results").to_string()
                        } else {
                            format!("{}/{} {}", self.search.current + 1, n, t("results"))
                        },
                    );
                    if self.search.all_sheets
                        && let Some(hit) = self.search.hits.get(self.search.current)
                        && let Some(sheet) = self
                            .doc()
                            .and_then(|d| d.doc.workbook().sheets.get(hit.sheet))
                    {
                        widgets::caption(ui, format!("{}: {}", t("Sheet"), sheet.title));
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.checkbox(&mut self.search.all_sheets, t("All sheets"));
                        ui.checkbox(&mut self.search.opts.case_sensitive, t("Match case"));
                        ui.checkbox(&mut self.search.opts.whole_word, t("Whole word"));
                        ui.checkbox(&mut self.search.opts.include_notes, t("Include notes"));
                        ui.checkbox(&mut self.search.opts.include_labels, t("Include labels"));
                        ui.checkbox(&mut self.search.dim, t("Dim others"));
                        ui.checkbox(&mut self.search.show_replace, t("Replace"));
                    });
                    if self.search.show_replace {
                        widgets::caption(ui, t("Replaces topic titles only."));
                        ui.horizontal(|ui| {
                            ui.add(
                                TextEdit::singleline(&mut self.search.replace)
                                    .hint_text(t("Replace"))
                                    .desired_width(220.0),
                            )
                            .accessible_name(t("Replace with"));
                            if ui
                                .add_enabled(n > 0, egui::Button::new(t("Replace All")))
                                .clicked()
                            {
                                self.replace_all();
                            }
                        });
                    }
                });
            });
        if close || ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.search.open = false;
        }
        self.apply_search_filter();
    }

    // ----- dialogs ------------------------------------------------------------------------------

    fn dialogs_ui(&mut self, ctx: &Context) {
        // Crash recovery.
        if !self.dialogs.recovery.is_empty() {
            let mut action: Option<(bool, Option<usize>)> = None;
            egui::Modal::new(egui::Id::unique("thinoda_recovery")).show(ctx, |ui| {
                ui.set_width(460.0);
                ui.heading(t("Recover Unsaved Work"));
                ui.label(t(
                    "Thinoda was not closed properly. The following documents can be restored:",
                ));
                ui.add_space(8.0);
                for (i, s) in self.dialogs.recovery.iter().enumerate() {
                    for doc in &s.docs {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&doc.meta.title).strong());
                            if let Some(p) = &doc.meta.original_path {
                                widgets::caption(ui, p.display().to_string());
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.button(t("Restore")).clicked() {
                                    action = Some((true, Some(i)));
                                }
                            });
                        });
                    }
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if widgets::primary_button(ui, t("Restore All")).clicked() {
                        action = Some((true, None));
                    }
                    if widgets::secondary_button(ui, t("Discard All")).clicked() {
                        action = Some((false, None));
                    }
                });
            });
            match action {
                Some((true, which)) => self.restore_orphans(which),
                Some((false, _)) => self.discard_orphans(),
                None => {}
            }
        }

        // Save changes?
        if let Some(intent) = self.dialogs.close.clone() {
            let buttons = [
                (t("Save"), DialogButton::Primary),
                (t("Don't Save"), DialogButton::Danger),
                (t("Cancel"), DialogButton::Secondary),
            ];
            if let Some(b) = confirm_dialog(
                ctx,
                "thinoda_close",
                t("Save changes?"),
                t(
                    "Do you want to save the changes you made? Your changes will be lost if you don't save them.",
                ),
                &buttons,
            ) {
                self.dialogs.close = None;
                match (intent, b) {
                    (CloseIntent::Tab(i), 0) => {
                        self.active = i;
                        if self.save_active(false) {
                            self.close_tab_now(i);
                        }
                    }
                    (CloseIntent::Tab(i), 1) => self.close_tab_now(i),
                    (CloseIntent::Quit, 0) => {
                        self.exec(ctx, Cmd::SaveAll);
                        if !self.any_dirty() {
                            self.quitting = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                    (CloseIntent::Quit, 1) => {
                        for d in &mut self.docs {
                            d.doc.mark_saved();
                        }
                        self.quitting = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    _ => {}
                }
            }
        }

        // External modification conflict.
        if let Some(i) = self.dialogs.conflict {
            let buttons = [
                (t("Overwrite"), DialogButton::Danger),
                (t("Save As Copy"), DialogButton::Primary),
                (t("Cancel"), DialogButton::Secondary),
            ];
            if let Some(b) = confirm_dialog(
                ctx,
                "thinoda_conflict",
                t("File changed on disk"),
                t("The file was modified by another application or a sync client. Overwrite it?"),
                &buttons,
            ) {
                self.dialogs.conflict = None;
                self.active = i;
                match b {
                    0 => {
                        if let Some(d) = self.docs.get_mut(i) {
                            d.ignore_conflict = true;
                        }
                        self.save_active(false);
                    }
                    1 => {
                        self.save_active(true);
                    }
                    _ => {}
                }
            }
        }

        // Rename sheet.
        if let Some((i, mut name)) = self.dialogs.rename_sheet.take() {
            let mut done = None;
            egui::Modal::new(egui::Id::unique("thinoda_rename")).show(ctx, |ui| {
                ui.heading(t("Rename Sheet"));
                let r = ui.add(TextEdit::singleline(&mut name).desired_width(280.0));
                r.request_focus();
                ui.horizontal(|ui| {
                    if widgets::primary_button(ui, t("OK")).clicked()
                        || ui.input(|i| i.key_pressed(Key::Enter))
                    {
                        done = Some(true);
                    }
                    if widgets::secondary_button(ui, t("Cancel")).clicked()
                        || ui.input(|i| i.key_pressed(Key::Escape))
                    {
                        done = Some(false);
                    }
                });
            });
            match done {
                Some(true) => {
                    if let Some(d) = self.docs.get_mut(self.active) {
                        let prev = d.doc.active_index();
                        d.doc.set_active(i);
                        let n = name.clone();
                        let _ = d.doc.edit_sheet("Rename Sheet", |s| {
                            s.title = n;
                            Ok(())
                        });
                        d.doc.set_active(prev);
                    }
                }
                Some(false) => {}
                None => self.dialogs.rename_sheet = Some((i, name)),
            }
        }

        if self.dialogs.settings {
            self.settings_window(ctx);
        }
        if self.dialogs.about {
            let mut open = true;
            egui::Window::new(t("About Thinoda"))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        crate::brand::logo(ui, 72.0);
                        ui.add_space(8.0);
                        ui.vertical(|ui| {
                            ui.heading(RichText::new("Thinoda").size(26.0).strong());
                            ui.label(RichText::new(t(crate::brand::TAGLINE)).italics());
                        });
                    });
                    ui.add_space(6.0);
                    ui.label(t("A fast, open mind mapping app."));
                    ui.label(format!("{} Axel Wolters", t("Written by")));
                    ui.hyperlink_to(
                        format!("{}: {}", t("Contact"), crate::brand::CONTACT_EMAIL),
                        format!("mailto:{}", crate::brand::CONTACT_EMAIL),
                    )
                    .on_hover_text(t("Write an email"));
                    ui.label(format!(
                        "{} {} · {} {}",
                        t("Version"),
                        thinoda_platform::system::app_version(),
                        thinoda_platform::system::os_name(),
                        thinoda_platform::system::arch()
                    ));
                    ui.hyperlink_to(crate::brand::PROJECT_URL, crate::brand::PROJECT_URL)
                        .on_hover_text(t("Open in web browser"));
                    ui.separator();
                    if ui.button(t("Legal documents")).clicked() {
                        self.dialogs.legal.open = true;
                    }
                });
            self.dialogs.about = open;
        }
        if self.dialogs.masked_copy {
            let mut open = true;
            let mut create = false;
            let mut cancel = false;
            egui::Window::new(t("Create Masked Copy…"))
                .open(&mut open)
                .collapsible(false)
                .default_width(460.0)
                .show(ctx, |ui| {
                    ui.label(t("Creates a new unsaved document. The original is not changed."));
                    ui.label(t("Replaces all topic, sheet and relationship titles. Removes notes, labels, links, images, markers, custom font names, boundary titles and document metadata."));
                    ui.label(t("Structure, colors and task states remain visible. This does not guarantee anonymity; inspect every sheet before sharing."));
                    ui.horizontal(|ui| {
                        if ui.button(t("Cancel")).clicked() { cancel = true; }
                        if ui.button(t("Create Preview")).clicked() { create = true; }
                    });
                });
            self.dialogs.masked_copy = open && !cancel;
            if create {
                self.create_masked_copy();
            }
        }
    }

    fn settings_window(&mut self, ctx: &Context) {
        let mut open = true;
        let before = self.settings.clone();
        let mut credentials_saved = false;
        egui::Window::new(t("Settings"))
            .open(&mut open)
            .default_width(440.0)
            .collapsible(false)
            .vscroll(true)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (tab, label) in [
                        (SettingsTab::General, "General"),
                        (SettingsTab::Editor, "Editor"),
                        (SettingsTab::Ai, "AI Assistant"),
                        (SettingsTab::Cloud, "Cloud"),
                    ] {
                        ui.selectable_value(&mut self.dialogs.settings_tab, tab, t(label));
                    }
                });
                ui.separator();
                if self.dialogs.settings_tab == SettingsTab::General {
                    widgets::section(ui, t("Appearance"), true, |ui| {
                        widgets::property_row(ui, t("Theme"), |ui| {
                            widgets::segmented(
                                ui,
                                &mut self.settings.theme,
                                &[
                                    (SettingsTheme::System, t("System")),
                                    (SettingsTheme::Light, t("Light")),
                                    (SettingsTheme::Dark, t("Dark")),
                                ],
                            );
                        });
                        widgets::property_row(ui, t("Accent Color"), |ui| {
                            if let Some(c) =
                                widgets::color_swatches(ui, &SWATCHES[..8], Some(self.accent))
                            {
                                self.accent = c;
                            }
                        });
                        widgets::property_row(ui, t("Language"), |ui| {
                            let mut l = self.settings.language;
                            egui::ComboBox::from_id_salt("settings_language")
                                .selected_text(if l == Language::Auto {
                                    t("Auto")
                                } else {
                                    l.native_name()
                                })
                                .show_ui(ui, |ui| {
                                    for language in Language::ALL {
                                        ui.selectable_value(
                                            &mut l,
                                            language,
                                            if language == Language::Auto {
                                                t("Auto")
                                            } else {
                                                language.native_name()
                                            },
                                        );
                                    }
                                })
                                .response
                                .accessible_name(t("Language"));
                            if l != self.settings.language {
                                self.settings.language = l;
                                i18n::set_lang(i18n::resolve(l));
                                let titles = crate::app::new_topic_titles();
                                for d in &mut self.docs {
                                    d.view.titles = titles.clone();
                                }
                            }
                        });
                        widgets::property_field(ui, t("UI Scale"), |ui| {
                            ui.add(
                                egui::Slider::new(&mut self.settings.ui_scale, 0.75..=2.0)
                                    .step_by(0.05),
                            )
                        });
                    });
                    widgets::section(ui, t("Files"), true, |ui| {
                        widgets::property_field(ui, t("Autosave"), |ui| {
                            widgets::toggle(ui, &mut self.settings.autosave, t("Autosave"))
                        });
                        widgets::property_field(ui, t("Autosave interval (s)"), |ui| {
                            ui.add(
                                egui::DragValue::new(&mut self.settings.autosave_interval_secs)
                                    .range(5..=3600),
                            )
                        });
                        widgets::property_field(ui, t("Keep backup on save"), |ui| {
                            widgets::toggle(
                                ui,
                                &mut self.settings.backup_on_save,
                                t("Keep backup on save"),
                            )
                        });
                        widgets::property_field(ui, t("Confirm before deleting"), |ui| {
                            widgets::toggle(
                                ui,
                                &mut self.settings.confirm_on_delete,
                                t("Confirm before deleting"),
                            )
                        });
                        if let Some(p) = &self.services.paths
                            && ui.button(t("Open settings folder")).clicked()
                        {
                            let _ = thinoda_platform::system::reveal_in_file_manager(&p.config_dir);
                        }
                    });
                }
                if self.dialogs.settings_tab == SettingsTab::Editor {
                    widgets::section(ui, t("Editor"), true, |ui| {
                        widgets::property_field(ui, t("Show topic toolbar"), |ui| {
                            widgets::toggle(
                                ui,
                                &mut self.settings.show_topic_toolbar,
                                t("Show topic toolbar"),
                            )
                        });
                        widgets::property_field(ui, t("Show topic add buttons"), |ui| {
                            widgets::toggle(
                                ui,
                                &mut self.settings.show_topic_add_buttons,
                                t("Show topic add buttons"),
                            )
                        });
                        widgets::property_field(ui, t("Initial canvas zoom"), |ui| {
                            ui.add(
                                egui::Slider::new(
                                    &mut self.settings.initial_canvas_zoom,
                                    0.1..=5.0,
                                )
                                .step_by(0.1)
                                .custom_formatter(|value, _| format!("{:.0}%", value * 100.0)),
                            )
                        });
                        widgets::caption(
                            ui,
                            t("Applies to new and newly opened maps, not current views."),
                        );
                        widgets::property_field(
                            ui,
                            t("Reuse last structure for new blank maps"),
                            |ui| {
                                widgets::toggle(
                                    ui,
                                    &mut self.settings.reuse_last_structure,
                                    t("Reuse last structure for new blank maps"),
                                )
                            },
                        );
                        widgets::property_field(
                            ui,
                            t("Reuse last theme for new blank maps"),
                            |ui| {
                                widgets::toggle(
                                    ui,
                                    &mut self.settings.reuse_last_theme,
                                    t("Reuse last theme for new blank maps"),
                                )
                            },
                        );
                    });
                }
                if self.dialogs.settings_tab == SettingsTab::Ai {
                    credentials_saved = self.ai_settings_section(ui);
                }
                if self.dialogs.settings_tab == SettingsTab::Cloud {
                    widgets::section(ui, t("Cloud"), true, |ui| {
                        ui.label(RichText::new(t("Detected cloud folders")).strong());
                        if self.services.cloud.is_empty() {
                            widgets::caption(ui, t("No cloud folders detected"));
                        }
                        for f in self.services.cloud.clone() {
                            ui.horizontal(|ui| {
                                let sel =
                                    self.settings.cloud.preferred_folder.as_ref() == Some(&f.path);
                                ui.label(format!("{} {}", icons::CLOUD, f.name));
                                widgets::caption(ui, f.path.display().to_string());
                                if ui.add_enabled(!sel, egui::Button::new(t("Use"))).clicked() {
                                    self.settings.cloud.preferred_folder = Some(f.path.clone());
                                }
                            });
                        }
                        widgets::property_row(ui, t("Default save folder"), |ui| {
                            let label = self
                                .settings
                                .cloud
                                .preferred_folder
                                .as_ref()
                                .map(|p| p.display().to_string())
                                .unwrap_or_else(|| "—".into());
                            widgets::caption(ui, label);
                            if ui.button(t("Choose…")).clicked()
                                && let Some(dir) = rfd::FileDialog::new().pick_folder()
                            {
                                self.settings.cloud.preferred_folder = Some(dir);
                            }
                        });
                    });
                }
            });
        if self.settings != before && !credentials_saved {
            self.save_settings();
        }
        self.dialogs.settings = open;
    }
}

/// Paints a colored dot (used for sheet tab status etc.).
pub fn dot(ui: &mut Ui, color: Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
    ui.painter().circle_filled(r.center(), 4.0, color);
}

/// A text field that edits a private buffer while focused and reports the new value once
/// focus is lost (so typing is not reset by the model value on every frame).
/// `rows == 0` renders a single-line field.
pub(crate) fn buffered_text(
    ui: &mut Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    current: &str,
    rows: usize,
    hint: &str,
) -> (egui::Response, Option<String>) {
    let id = ui.make_persistent_id(("thinoda_buffered_text", salt));
    let mut buf: String = ui
        .data(|d| d.get_temp(id))
        .unwrap_or_else(|| current.to_string());
    let edit = if rows == 0 {
        TextEdit::singleline(&mut buf)
    } else {
        TextEdit::multiline(&mut buf).desired_rows(rows)
    };
    let r = ui.add(edit.id(id).hint_text(hint).desired_width(f32::INFINITY));
    if rows == 0 && r.lost_focus() {
        let cancel = ui.input_mut(|input| {
            input.consume_key(Modifiers::NONE, Key::Enter);
            input.consume_key(Modifiers::NONE, Key::Escape)
        });
        if cancel {
            ui.data_mut(|data| data.remove::<String>(id));
            return (r, None);
        }
    }
    if r.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, buf));
        (r, None)
    } else {
        ui.data_mut(|d| d.remove::<String>(id));
        let done = (r.lost_focus() && buf != current).then_some(buf);
        (r, done)
    }
}
