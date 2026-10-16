//! All app chrome: top bar, format panel, sheet tabs, status bar, outliner, home screen, dialogs.

use egui::{
    Align, Color32, Context, CornerRadius, Key, Layout, Margin, Modifiers, RichText, Sense, Stroke,
    TextEdit, Ui, vec2,
};
use ymind_canvas::{Action, MapCanvas};
use ymind_core::{
    Color, LineShape, Structure, TextAlign, TopicId, TopicShape, TopicStyle, markers, templates,
    theme::Theme,
};
use ymind_platform::settings::{Language, ThemeMode as SettingsTheme};
use ymind_ui::{DialogButton, Palette, Tokens, confirm_dialog, icons, widgets};

use crate::{
    app::{CenterMode, CloseIntent, Cmd, EXPORT_MENU, FocusField, FormatTab, YMindApp},
    i18n::{self, t},
};

type StyleChange = (&'static str, Box<dyn Fn(&mut TopicStyle)>);
type StyleToggle = fn(&mut TopicStyle, bool);

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

fn structure_label(s: Structure) -> &'static str {
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

impl YMindApp {
    /// Draws one frame of the whole application.
    pub fn draw(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        self.apply_appearance(&ctx);
        self.handle_close_request(&ctx);
        if !self.palette.open && !self.any_modal_open() {
            self.global_shortcuts(&ctx);
            self.insert_shortcuts(&ctx);
        }

        let show_doc = self.active < self.docs.len();
        if !self.zen {
            egui::Panel::top("ymind_topbar")
                .exact_size(Tokens::TOOLBAR_HEIGHT)
                .frame(
                    egui::Frame::new()
                        .fill(Palette::of(&ctx).surface)
                        .inner_margin(Margin::symmetric(8, 6)),
                )
                .show(ui, |ui| self.top_bar(ui));
            if show_doc {
                egui::Panel::bottom("ymind_statusbar")
                    .exact_size(30.0)
                    .frame(
                        egui::Frame::new()
                            .fill(Palette::of(&ctx).surface)
                            .inner_margin(Margin::symmetric(8, 3)),
                    )
                    .show(ui, |ui| self.bottom_bar(ui));
                if self.settings.show_format_panel {
                    egui::Panel::right("ymind_format")
                        .default_size(280.0)
                        .size_range(240.0..=420.0)
                        .frame(
                            egui::Frame::new()
                                .fill(Palette::of(&ctx).surface)
                                .inner_margin(Margin::same(10)),
                        )
                        .show(ui, |ui| {
                            egui::ScrollArea::vertical().show(ui, |ui| self.format_panel(ui));
                        });
                }
            }
        }

        let center = egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
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

        if show_doc && self.search.open {
            self.search_bar(&ctx);
        }
        self.update_search();

        // Command palette.
        let cmds = Self::palette_commands();
        let list: Vec<ymind_ui::Command> = cmds.iter().map(|(_, c)| c.clone()).collect();
        if let Some(id) = self.palette.show(&ctx, &list, t("Type a command…"))
            && let Some((cmd, _)) = cmds.iter().find(|(_, c)| c.id == id)
        {
            self.exec(&ctx, cmd.clone());
        }

        self.dialogs_ui(&ctx);
        self.ai_prompt_dialog(&ctx);
        self.ai_busy_overlay(&ctx);
        self.toasts.show(&ctx);
        self.housekeeping(&ctx);
    }

    fn any_modal_open(&self) -> bool {
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
                    self.doc().is_some() && self.settings.show_format_panel,
                )
                .clicked()
                {
                    cmd = Some(Cmd::ToggleFormatPanel);
                }
                ui.menu_button(RichText::new(icons::EXPORT), |ui| {
                    for f in EXPORT_MENU {
                        if ui.button(f.label()).clicked() {
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
            for tpl in templates::TEMPLATES {
                if ui.button(tpl.name).clicked() {
                    *cmd = Some(Cmd::NewFromTemplate(tpl.id));
                }
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
                if ui.button(f.label()).clicked() {
                    *cmd = Some(Cmd::Export(*f));
                }
            }
        });
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
                if item(ui, t(l), sc).clicked() {
                    *cmd = Some(c);
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
                    Action::InsertFloating(ymind_core::Vec2::new(0.0, -260.0)),
                ),
                ("Relationship", "Ctrl+Shift+L", Action::StartRelationship),
                ("Summary", "Ctrl+]", Action::AddSummary),
                ("Boundary", "Ctrl+B", Action::AddBoundary),
                ("To-Do", "", Action::ToggleTodo),
            ];
            for (l, sc, a) in e {
                if item(ui, t(l), sc).clicked() {
                    *cmd = Some(Cmd::Canvas(a));
                }
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
        ui.menu_button(t("View"), |ui| {
            let e = [
                ("Outliner", "Ctrl+Shift+O", Cmd::ToggleOutliner),
                ("Zen Mode", "F11", Cmd::ToggleZen),
                ("Format Panel", "Ctrl+Shift+R", Cmd::ToggleFormatPanel),
                ("Status Bar", "", Cmd::ToggleStatusBar),
                ("Zoom In", "Ctrl++", Cmd::Canvas(Action::ZoomIn)),
                ("Zoom Out", "Ctrl+-", Cmd::Canvas(Action::ZoomOut)),
                ("Actual Size", "Ctrl+0", Cmd::Canvas(Action::ZoomReset)),
                ("Fit Map", "Ctrl+Shift+H", Cmd::Canvas(Action::Fit)),
                ("Expand All", "", Cmd::Canvas(Action::ExpandAll)),
                ("Collapse All", "", Cmd::Canvas(Action::CollapseAll)),
            ];
            for (l, sc, c) in e {
                if item(ui, t(l), sc).clicked() {
                    *cmd = Some(c);
                }
            }
        });
        ui.separator();
        if item(ui, t("Settings…"), "Ctrl+,").clicked() {
            *cmd = Some(Cmd::Settings);
        }
        ui.menu_button(t("Help"), |ui| {
            if item(ui, t("Keyboard Shortcuts"), "F1").clicked() {
                *cmd = Some(Cmd::Shortcuts);
            }
            if ui.button(t("About yMind")).clicked() {
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
            ui.label(RichText::new(g.name).small().weak());
            ui.horizontal_wrapped(|ui| {
                for m in g.markers {
                    if ui.button(m.glyph).on_hover_text(m.name).clicked() {
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
        egui::Area::new(egui::Id::unique("ymind_editing_toolbar"))
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
        let paste = std::mem::take(&mut self.paste_requested);
        let Some(d) = self.docs.get_mut(self.active) else {
            return;
        };
        d.view.add_control_labels.child = t("Add child topic").into();
        d.view.add_control_labels.sibling = t("Add sibling topic").into();
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
        if out.error.is_some() {
            self.toasts.warning(t("Not possible here"));
        }
        let mut cmd = None;
        out.response
            .context_menu(|ui| self.context_menu(ui, &mut cmd));
        if let Some(c) = cmd {
            self.exec(&ctx, c);
        }
    }

    fn topic_toolbar(&mut self, ctx: &Context) {
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
        egui::Area::new(egui::Id::unique("ymind_topic_toolbar"))
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
        let has_sel = self.doc().is_some_and(|d| !d.view.selection.is_empty());
        let single = self.doc().is_some_and(|d| d.view.selection.len() == 1);
        let non_root = self
            .doc()
            .is_some_and(|d| has_sel && !d.view.selection.contains(&d.doc.sheet().root));
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
        }
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
        item(
            ui,
            t("Delete"),
            "Del",
            Cmd::Canvas(Action::Delete),
            non_root,
        );
        item(
            ui,
            t("Delete Topic Only"),
            "Ctrl+Del",
            Cmd::Canvas(Action::DeleteKeepChildren),
            non_root,
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
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.heading(RichText::new(t("Create a new map")).size(22.0).strong());
                    ui.add_space(12.0);
                    let gap = 8.0;
                    let columns = ((ui.available_width() + gap) / (164.0 + gap))
                        .floor()
                        .max(1.0) as usize;
                    egui::Grid::new("ymind_templates")
                        .num_columns(columns)
                        .spacing(vec2(gap, gap))
                        .show(ui, |ui| {
                            for (index, tpl) in templates::TEMPLATES.iter().enumerate() {
                                let r = widgets::card(ui).show(ui, |ui| {
                                    ui.set_width(150.0);
                                    ui.set_height(90.0);
                                    ui.vertical_centered(|ui| {
                                        ui.add_space(14.0);
                                        ui.label(
                                            RichText::new(icons::MAP).size(26.0).color(p.accent),
                                        );
                                        ui.label(RichText::new(tpl.name).strong());
                                    });
                                });
                                let r = ui.interact(
                                    r.response.rect,
                                    ui.scope_id().with(tpl.id),
                                    Sense::click(),
                                );
                                r.widget_info(|| {
                                    egui::WidgetInfo::labeled(egui::Role::Button, true, tpl.name)
                                });
                                if r.hovered() {
                                    ui.painter().rect_stroke(
                                        r.rect,
                                        CornerRadius::same(Tokens::RADIUS_M),
                                        Stroke::new(2.0, p.accent),
                                        egui::StrokeKind::Outside,
                                    );
                                }
                                if r.clicked() {
                                    cmd = Some(Cmd::NewFromTemplate(tpl.id));
                                }
                                if (index + 1) % columns == 0 {
                                    ui.end_row();
                                }
                            }
                        });
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
                            let cloud = ymind_platform::cloud::provider_for_path(
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
                            s: &ymind_core::Sheet,
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
                                let chevron = if !has_kids {
                                    " "
                                } else if topic.collapsed {
                                    icons::CHEVRON_RIGHT
                                } else {
                                    icons::CHEVRON_DOWN
                                };
                                if ui
                                    .add(
                                        egui::Button::new(chevron)
                                            .frame(false)
                                            .min_size(vec2(16.0, 16.0)),
                                    )
                                    .clicked()
                                    && has_kids
                                {
                                    d.view.select_only(id);
                                    action = Some(Action::ToggleCollapse);
                                }
                                if let Some(todo) = topic.todo {
                                    ui.label(if todo { "☑" } else { "☐" });
                                }
                                for m in &topic.markers {
                                    ui.label(markers::glyph(m));
                                }
                                if d.view.editing_topic() == Some(id) {
                                    let mut text =
                                        d.view.editing_text().unwrap_or_default().to_string();
                                    let te_id = egui::Id::unique(("outliner_edit", id));
                                    let r = ui.add(
                                        TextEdit::singleline(&mut text)
                                            .id(te_id)
                                            .desired_width(f32::INFINITY),
                                    );
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
        if d.view.editing_topic().is_none() && !ctx.text_edit_focused() {
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
            && let Err(_e) = ymind_canvas::apply_action(&mut d.doc, &mut d.view, a)
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
                    cmd = Some(Cmd::RenameSheet);
                }
                r.context_menu(|ui| {
                    if ui.button(t("Rename Sheet")).clicked() {
                        d.doc.set_active(i);
                        cmd = Some(Cmd::RenameSheet);
                    }
                    if ui.button(t("Duplicate Sheet")).clicked() {
                        d.doc.set_active(i);
                        cmd = Some(Cmd::DuplicateSheet);
                    }
                    if ui
                        .add_enabled(i > 0, egui::Button::new(t("Move Left")))
                        .clicked()
                    {
                        let _ = d.doc.move_sheet(i, i - 1);
                    }
                    if ui
                        .add_enabled(i + 1 < n, egui::Button::new(t("Move Right")))
                        .clicked()
                    {
                        let _ = d.doc.move_sheet(i, i + 1);
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
                if ui
                    .small_button(format!("{}%", d.view.zoom_percent()))
                    .on_hover_text(t("Actual Size"))
                    .clicked()
                {
                    cmd = Some(Cmd::Canvas(Action::ZoomReset));
                }
                if ui.small_button("−").on_hover_text(t("Zoom Out")).clicked() {
                    cmd = Some(Cmd::Canvas(Action::ZoomOut));
                }
                widgets::vdivider(ui);
                let sheet = d.doc.sheet();
                widgets::caption(
                    ui,
                    format!("{}: {}", t("Words"), ymind_core::search::word_count(sheet)),
                );
                widgets::caption(ui, format!("{}: {}", t("Topics"), sheet.topics.len()));
                widgets::vdivider(ui);
                if let Some(c) = d
                    .path
                    .as_ref()
                    .and_then(|p| ymind_platform::cloud::provider_for_path(&self.services.cloud, p))
                {
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

    fn format_panel(&mut self, ui: &mut Ui) {
        let mut tab = self.format_tab;
        widgets::segmented(
            ui,
            &mut tab,
            &[(FormatTab::Style, t("Style")), (FormatTab::Map, t("Map"))],
        );
        self.format_tab = tab;
        ui.add_space(8.0);
        match tab {
            FormatTab::Style => self.style_tab(ui),
            FormatTab::Map => self.map_tab(ui),
        }
    }

    fn style_tab(&mut self, ui: &mut Ui) {
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
        let resolved = ymind_core::theme::resolve_style(sheet, primary, d.view.dark);
        let mut change: Option<StyleChange> = None;

        widgets::section(ui, t("Structure"), true, |ui| {
            let cur = topic.structure;
            let mut chosen = None;
            widgets::property_field(ui, t("Structure"), |ui| {
                egui::ComboBox::from_id_salt("topic_structure")
                    .selected_text(cur.map_or(t("Auto"), |s| structure_label(s)))
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(cur.is_none(), t("Auto")).clicked() {
                            chosen = Some(None);
                        }
                        for s in Structure::ALL {
                            if ui
                                .selectable_label(cur == Some(s), structure_label(s))
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
                    .selected_text(shape.label())
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for s in TopicShape::ALL {
                            ui.selectable_value(&mut shape, s, s.label());
                        }
                    })
                    .response
            });
            if shape != resolved.shape {
                change = Some(("Shape", Box::new(move |st| st.shape = Some(shape))));
            }
            widgets::property_row(ui, t("Fill"), |ui| {
                if let Some(c) =
                    widgets::color_swatches(ui, &SWATCHES, resolved.fill.map(from_color))
                {
                    change = Some(("Fill", Box::new(move |st| st.fill = Some(to_color(c)))));
                }
            });
            widgets::property_row(ui, t("Border"), |ui| {
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
            let mut size = resolved.font_size;
            widgets::property_field(ui, t("Font Size"), |ui| {
                let r = ui.add(egui::DragValue::new(&mut size).range(6.0..=96.0).speed(0.5));
                if r.changed() {
                    change = Some(("Font Size", Box::new(move |st| st.font_size = Some(size))));
                }
                r
            });
            ui.horizontal(|ui| {
                let toggles: [(&str, bool, StyleToggle); 4] = [
                    (icons::BOLD, resolved.bold, |s, v| s.bold = Some(v)),
                    (icons::ITALIC, resolved.italic, |s, v| s.italic = Some(v)),
                    (icons::UNDERLINE, resolved.underline, |s, v| {
                        s.underline = Some(v)
                    }),
                    (icons::STRIKE, resolved.strikethrough, |s, v| {
                        s.strikethrough = Some(v)
                    }),
                ];
                for (icon, on, set) in toggles {
                    if widgets::icon_button(ui, icon, icon, on).clicked() {
                        change = Some(("Text Style", Box::new(move |st| set(st, !on))));
                    }
                }
                widgets::vdivider(ui);
                for (a, icon) in [
                    (TextAlign::Left, "⬅"),
                    (TextAlign::Center, "⬌"),
                    (TextAlign::Right, "➡"),
                ] {
                    if widgets::icon_button(ui, icon, t("Align"), resolved.align == a).clicked() {
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
                    .selected_text(ls.label())
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for s in LineShape::ALL {
                            ui.selectable_value(&mut ls, s, s.label());
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
            let l = ui.label(format!("{} ({})", t("Labels"), t("comma separated")));
            let (r, done) = buffered_text(ui, ("labels", primary), &topic.labels.join(", "), 0, "");
            r.labelled_by(l.id);
            if let Some(labels) = done {
                let v: Vec<String> = labels
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                if v != topic.labels {
                    let _ = d.doc.edit_sheet("Labels", |s| s.set_labels(primary, v));
                }
            }
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
    }

    fn map_tab(&mut self, ui: &mut Ui) {
        let Some(d) = self.docs.get_mut(self.active) else {
            return;
        };
        let sheet = d.doc.sheet();
        let mut ts = sheet.theme.clone();
        let structure = sheet.structure;
        let before = ts.clone();

        widgets::section(ui, t("Structure"), true, |ui| {
            let mut chosen = None;
            widgets::property_field(ui, t("Structure"), |ui| {
                egui::ComboBox::from_id_salt("sheet_structure")
                    .selected_text(structure_label(structure))
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for s in Structure::ALL {
                            if ui
                                .selectable_label(structure == s, structure_label(s))
                                .clicked()
                            {
                                chosen = Some(s);
                            }
                        }
                    })
                    .response
            });
            if let Some(s) = chosen {
                let _ = d
                    .doc
                    .edit_sheet("Structure", |sh| sh.set_structure(None, Some(s)));
            }
        });

        widgets::section(ui, t("Theme"), true, |ui| {
            ui.horizontal_wrapped(|ui| {
                for id in Theme::IDS {
                    let th = Theme::builtin(id);
                    let selected = ts.theme_id == id;
                    let r = widgets::card(ui).show(ui, |ui| {
                        ui.set_width(104.0);
                        let (rect, _) = ui.allocate_exact_size(vec2(100.0, 34.0), Sense::hover());
                        let painter = ui.painter();
                        painter.rect_filled(rect, CornerRadius::same(4), from_color(th.light_bg));
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
                        ui.label(RichText::new(th.name).small());
                    });
                    let r = ui.interact(r.response.rect, ui.scope_id().with(id), Sense::click());
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
                        ts.variant = 0;
                    }
                }
            });
            let th = Theme::builtin(&ts.theme_id);
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
            widgets::toggle(ui, &mut ts.rainbow, t("Rainbow Branches"));
            widgets::toggle(ui, &mut ts.tapered_lines, t("Tapered Lines"));
            widgets::toggle(ui, &mut ts.balanced, t("Balanced Map"));
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
            if let Some(c) = widgets::color_swatches(ui, &SWATCHES, ts.background.map(from_color)) {
                ts.background = Some(to_color(c));
            }
        });

        if ts != before {
            let _ = d.doc.edit_sheet(t("Theme"), |s| {
                s.theme = ts;
                Ok(())
            });
        }
    }

    // ----- search bar ---------------------------------------------------------------------------

    fn search_bar(&mut self, ctx: &Context) {
        let mut close = false;
        egui::Area::new(egui::Id::unique("ymind_search"))
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
                    ui.horizontal_wrapped(|ui| {
                        ui.checkbox(&mut self.search.opts.case_sensitive, t("Match case"));
                        ui.checkbox(&mut self.search.opts.whole_word, t("Whole word"));
                        ui.checkbox(&mut self.search.opts.include_notes, t("Include notes"));
                        ui.checkbox(&mut self.search.dim, t("Dim others"));
                        ui.checkbox(&mut self.search.show_replace, t("Replace"));
                    });
                    if self.search.show_replace {
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
            egui::Modal::new(egui::Id::unique("ymind_recovery")).show(ctx, |ui| {
                ui.set_width(460.0);
                ui.heading(t("Recover Unsaved Work"));
                ui.label(t(
                    "yMind was not closed properly. The following documents can be restored:",
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
                "ymind_close",
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
                "ymind_conflict",
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
            egui::Modal::new(egui::Id::unique("ymind_rename")).show(ctx, |ui| {
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
            egui::Window::new(t("About yMind"))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.heading(RichText::new("yMind").size(26.0).strong());
                    ui.label(t("A fast, open mind mapping app."));
                    ui.label(format!(
                        "{} {} · {} {}",
                        t("Version"),
                        ymind_platform::system::app_version(),
                        ymind_platform::system::os_name(),
                        ymind_platform::system::arch()
                    ));
                    ui.hyperlink("https://github.com/AxelWol/yMind");
                });
            self.dialogs.about = open;
        }
        if self.dialogs.shortcuts {
            let mut open = true;
            egui::Window::new(t("Keyboard Shortcuts"))
                .open(&mut open)
                .default_width(420.0)
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(480.0)
                        .show(ui, |ui| {
                            egui::Grid::new("shortcuts").striped(true).show(ui, |ui| {
                                for (_, c) in Self::palette_commands() {
                                    if let Some(sc) = &c.shortcut {
                                        ui.label(&c.title);
                                        ui.label(RichText::new(sc).monospace());
                                        ui.end_row();
                                    }
                                }
                                for (k, v) in [
                                    ("Space / F2", "Edit"),
                                    ("Arrows", "Navigate"),
                                    ("Alt+↑/↓", "Move"),
                                    ("Esc", "Select none"),
                                    ("Home", "Central Topic"),
                                ] {
                                    ui.label(t(v));
                                    ui.label(RichText::new(k).monospace());
                                    ui.end_row();
                                }
                            });
                        });
                });
            self.dialogs.shortcuts = open;
        }
    }

    fn settings_window(&mut self, ctx: &Context) {
        let mut open = true;
        let before = self.settings.clone();
        egui::Window::new(t("Settings"))
            .open(&mut open)
            .default_width(440.0)
            .collapsible(false)
            .show(ctx, |ui| {
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
                        widgets::segmented(
                            ui,
                            &mut l,
                            &[
                                (Language::Auto, t("Auto")),
                                (Language::English, "English"),
                                (Language::German, "Deutsch"),
                            ],
                        );
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
                    widgets::toggle(ui, &mut self.settings.autosave, t("Autosave"));
                    widgets::property_field(ui, t("Autosave interval (s)"), |ui| {
                        ui.add(
                            egui::DragValue::new(&mut self.settings.autosave_interval_secs)
                                .range(5..=3600),
                        )
                    });
                    widgets::toggle(
                        ui,
                        &mut self.settings.backup_on_save,
                        t("Keep backup on save"),
                    );
                    widgets::toggle(
                        ui,
                        &mut self.settings.confirm_on_delete,
                        t("Confirm before deleting"),
                    );
                    if let Some(p) = &self.services.paths
                        && ui.button(t("Open settings folder")).clicked()
                    {
                        let _ = ymind_platform::system::reveal_in_file_manager(&p.config_dir);
                    }
                });
                self.ai_settings_section(ui);
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
            });
        if self.settings != before {
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
fn buffered_text(
    ui: &mut Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    current: &str,
    rows: usize,
    hint: &str,
) -> (egui::Response, Option<String>) {
    let id = ui.make_persistent_id(("ymind_buffered_text", salt));
    let mut buf: String = ui
        .data(|d| d.get_temp(id))
        .unwrap_or_else(|| current.to_string());
    let edit = if rows == 0 {
        TextEdit::singleline(&mut buf)
    } else {
        TextEdit::multiline(&mut buf).desired_rows(rows)
    };
    let r = ui.add(edit.id(id).hint_text(hint).desired_width(f32::INFINITY));
    if r.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, buf));
        (r, None)
    } else {
        ui.data_mut(|d| d.remove::<String>(id));
        let done = (r.lost_focus() && buf != current).then_some(buf);
        (r, done)
    }
}
