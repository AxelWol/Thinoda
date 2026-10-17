use egui::{Context, Ui, vec2};
use ymind_core::{
    Color, LineShape, Sheet, TextAlign, TopicShape, TopicStyle,
    custom_theme::CustomTheme,
    layout::layout_sheet,
    quick_styles::{PRESETS, preset},
    theme::{Level, ResolvedStyle, Theme, resolve_style},
};
use ymind_ui::{DialogButton, confirm_dialog, widgets};

use crate::{
    app::{Cmd, YMindApp},
    i18n::t,
    ui::{font_selector, from_color, to_color},
};

const ROLES: [(Level, &str); 5] = [
    (Level::Central, "Central Topic"),
    (Level::Main, "Main Topic"),
    (Level::Sub, "Subtopic"),
    (Level::Floating, "Floating Topic"),
    (Level::Summary, "Summary"),
];

pub(crate) enum ThemeDialog {
    Edit(Box<ThemeDraft>),
    Remove(String),
}

pub(crate) struct ThemeDraft {
    source: (String, usize, u64),
    theme: CustomTheme,
    role: usize,
    error: Option<String>,
}

fn preview(theme: &CustomTheme) -> (Sheet, [ymind_core::TopicId; 5]) {
    let mut sheet = Sheet::new("Preview", "Idea");
    sheet.theme.custom_theme = Some(theme.clone());
    let main = sheet.add_child(sheet.root, None, "Plan");
    let other = sheet.add_child(sheet.root, None, "Review");
    let sub = sheet.add_child(main, None, "Build");
    let floating = sheet.add_floating("Note", ymind_core::Vec2::new(0.0, 180.0));
    let summary = sheet.add_summary(&[main, other], "Summary").unwrap();
    let ids = [sheet.root, main, sub, floating, summary];
    (sheet, ids)
}

impl YMindApp {
    pub(crate) fn begin_theme_editor(&mut self) {
        let Some(doc) = self.doc() else {
            self.toasts.warning(t("Not possible here"));
            return;
        };
        let theme = CustomTheme::from_settings(&doc.doc.sheet().theme);
        self.edit_custom_theme(theme);
    }

    fn edit_custom_theme(&mut self, theme: CustomTheme) {
        if !self.command_available(&Cmd::EditCustomTheme) {
            self.toasts.warning(t("Not possible here"));
            return;
        }
        let doc = self.doc_mut().unwrap();
        doc.view.commit_edit(&mut doc.doc);
        let source = (doc.key.clone(), doc.doc.active_index(), doc.doc.state_id());
        self.theme_dialog = Some(ThemeDialog::Edit(Box::new(ThemeDraft {
            source,
            theme,
            role: 0,
            error: None,
        })));
    }

    pub(crate) fn apply_custom_theme(&mut self, theme: CustomTheme) -> bool {
        if !self.command_available(&Cmd::ApplyCustomTheme(Box::new(theme.clone()))) {
            self.toasts.warning(t("Not possible here"));
            return false;
        }
        if let Err(error) = theme.validate() {
            self.toasts.error(error);
            return false;
        }
        let doc = self.doc_mut().unwrap();
        doc.view.commit_edit(&mut doc.doc);
        match doc.doc.edit_sheet("Custom Theme", |sheet| {
            let mut settings = sheet.theme.clone();
            settings.theme_id = theme.base_theme_id.clone();
            settings.custom_theme = Some(theme);
            settings
                .validate()
                .map_err(ymind_core::EditError::Invalid)?;
            sheet.theme = settings;
            Ok(())
        }) {
            Ok(()) => true,
            Err(error) => {
                self.toasts.error(error.to_string());
                false
            }
        }
    }

    fn store_custom_themes(&mut self, themes: Vec<CustomTheme>) -> Result<(), String> {
        ymind_platform::Settings::validate_custom_themes(&themes)?;
        let mut settings = self.settings.clone();
        settings.custom_themes = themes;
        if let Some(paths) = &self.services.paths {
            settings.save(paths).map_err(|error| error.to_string())?;
        }
        self.settings = settings;
        self.font_source = None;
        Ok(())
    }

    pub(crate) fn add_custom_theme_commands(&self, commands: &mut Vec<(Cmd, ymind_ui::Command)>) {
        for (index, theme) in self.settings.custom_themes.iter().enumerate() {
            commands.push((
                Cmd::ApplyCustomTheme(Box::new(theme.clone())),
                ymind_ui::Command::new(
                    format!("custom-theme-{index}"),
                    format!("{}: {}", t("Custom Theme"), theme.name),
                ),
            ));
        }
    }

    pub(crate) fn custom_theme_gallery(&mut self, ui: &mut Ui) {
        let mut apply = None;
        let mut edit = None;
        let mut remove = None;
        widgets::section(ui, t("Custom Themes"), true, |ui| {
            if let Some(theme) = self
                .doc()
                .and_then(|doc| doc.doc.sheet().theme.custom_theme.as_ref())
            {
                ui.label(format!("{}: {}", t("Current theme"), theme.name));
            }
            if ui
                .add_enabled(
                    self.command_available(&Cmd::EditCustomTheme),
                    egui::Button::new(t("Edit Custom Theme…")),
                )
                .clicked()
            {
                self.begin_theme_editor();
            }
            let columns = ((ui.available_width() + 8.0) / 116.0).floor().max(1.0) as usize;
            egui::Grid::new("custom_themes")
                .num_columns(columns)
                .spacing(vec2(8.0, 8.0))
                .show(ui, |ui| {
                    for (index, theme) in self.settings.custom_themes.iter().enumerate() {
                        ui.vertical(|ui| {
                            ui.set_width(108.0);
                            let selected = self.doc().is_some_and(|doc| {
                                doc.doc.sheet().theme.custom_theme.as_ref() == Some(theme)
                            });
                            let (_, activated) = widgets::preview_card(
                                ui,
                                &format!("{}: {}", t("Custom Theme"), theme.name),
                                selected,
                                108.0,
                                54.0,
                                |ui, rect| {
                                    let (sheet, _) = preview(theme);
                                    let layout = layout_sheet(
                                        &sheet,
                                        &ymind_canvas::EguiMeasure::new(ui.ctx()),
                                        ui.visuals().dark_mode,
                                    );
                                    ymind_canvas::paint_thumbnail(
                                        ui.painter(),
                                        &sheet,
                                        &layout,
                                        rect,
                                        ui.visuals().dark_mode,
                                    );
                                },
                            );
                            if activated {
                                apply = Some(theme.clone());
                            }
                            ui.horizontal(|ui| {
                                if ui.small_button(t("Edit…")).clicked() {
                                    edit = Some(theme.clone());
                                }
                                if ui.small_button(t("Remove…")).clicked() {
                                    remove = Some(theme.name.clone());
                                }
                            });
                        });
                        if (index + 1) % columns == 0 {
                            ui.end_row();
                        }
                    }
                });
            widgets::caption(
                ui,
                t(
                    "Topic overrides and explicit map settings take precedence. Choose a built-in theme to remove custom defaults.",
                ),
            );
        });
        if let Some(theme) = apply {
            self.apply_custom_theme(theme);
        }
        if let Some(theme) = edit {
            self.edit_custom_theme(theme);
        }
        if let Some(name) = remove {
            self.theme_dialog = Some(ThemeDialog::Remove(name));
        }
    }

    pub(crate) fn theme_editor_dialog(&mut self, ctx: &Context) {
        let Some(mut dialog) = self.theme_dialog.take() else {
            return;
        };
        if !egui::Popup::is_any_open(ctx)
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            return;
        }
        let mut keep = true;
        match &mut dialog {
            ThemeDialog::Remove(name) => {
                if let Some(choice) = confirm_dialog(
                    ctx,
                    "remove_custom_theme",
                    t("Remove Custom Theme?"),
                    &format!("{}: {name}", t("Custom Theme")),
                    &[
                        (t("Remove"), DialogButton::Danger),
                        (t("Cancel"), DialogButton::Secondary),
                    ],
                ) {
                    keep = false;
                    if choice == 0 {
                        let themes = self
                            .settings
                            .custom_themes
                            .iter()
                            .filter(|theme| theme.name != *name)
                            .cloned()
                            .collect();
                        if let Err(error) = self.store_custom_themes(themes) {
                            self.toasts
                                .error(format!("{}: {error}", t("Could not save theme")));
                        }
                    }
                }
            }
            ThemeDialog::Edit(draft) => {
                let valid = self.doc().is_some_and(|doc| {
                    draft.source == (doc.key.clone(), doc.doc.active_index(), doc.doc.state_id())
                });
                if !valid {
                    self.toasts
                        .warning(t("Theme editing canceled because the document changed."));
                    return;
                }
                let families = draft
                    .theme
                    .font_family
                    .iter()
                    .chain(draft.theme.roles.fonts())
                    .cloned()
                    .collect::<Vec<_>>();
                for name in self.fonts.ensure(ctx, families) {
                    log::warn!("Font unavailable in theme preview: {name}");
                    self.toasts
                        .warning(format!("{}: {name}", t("Font unavailable")));
                }
                let mut apply = false;
                let mut save = false;
                let response =
                    egui::Modal::new(egui::Id::unique("custom_theme_editor")).show(ctx, |ui| {
                        ui.set_width(600.0_f32.min(ctx.content_rect().width() - 48.0).max(240.0));
                        ui.heading(t("Custom Theme Editor"));
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .min_scrolled_height((ctx.content_rect().height() - 200.0).max(180.0))
                            .max_height((ctx.content_rect().height() - 200.0).max(180.0))
                            .show(ui, |ui| {
                                let label = ui.label(t("Theme name"));
                                ui.text_edit_singleline(&mut draft.theme.name)
                                    .labelled_by(label.id);
                                widgets::property_field(ui, t("Base theme"), |ui| {
                                    egui::ComboBox::from_id_salt("custom_base")
                                        .selected_text(
                                            Theme::builtin(&draft.theme.base_theme_id).name,
                                        )
                                        .show_ui(ui, |ui| {
                                            for id in Theme::IDS {
                                                ui.selectable_value(
                                                    &mut draft.theme.base_theme_id,
                                                    id.into(),
                                                    Theme::builtin(id).name,
                                                );
                                            }
                                        })
                                        .response
                                });
                                widgets::property_field(ui, t("Theme Font"), |ui| {
                                    font_selector(
                                        ui,
                                        &mut self.fonts,
                                        &mut draft.theme.font_family,
                                        t("Default"),
                                    )
                                });
                                edit_color(
                                    ui,
                                    t("Light background"),
                                    &mut draft.theme.light_background,
                                );
                                edit_color(
                                    ui,
                                    t("Dark background"),
                                    &mut draft.theme.dark_background,
                                );
                                let columns =
                                    ((ui.available_width() / 180.0).floor() as usize).max(1);
                                egui::Grid::new("theme_palette").num_columns(columns).show(
                                    ui,
                                    |ui| {
                                        for (index, color) in
                                            draft.theme.palette.iter_mut().enumerate()
                                        {
                                            ui.vertical(|ui| {
                                                ui.set_width(170.0);
                                                edit_color(
                                                    ui,
                                                    &format!(
                                                        "{} {}",
                                                        t("Palette color"),
                                                        index + 1
                                                    ),
                                                    color,
                                                );
                                            });
                                            if (index + 1) % columns == 0 {
                                                ui.end_row();
                                            }
                                        }
                                    },
                                );
                                ui.horizontal(|ui| {
                                    if ui
                                        .add_enabled(
                                            draft.theme.palette.len() < 16,
                                            egui::Button::new(t("Add color")),
                                        )
                                        .clicked()
                                    {
                                        draft.theme.palette.push(Color::rgb(46, 124, 246));
                                    }
                                    if ui
                                        .add_enabled(
                                            draft.theme.palette.len() > 1,
                                            egui::Button::new(t("Remove last color")),
                                        )
                                        .clicked()
                                    {
                                        draft.theme.palette.pop();
                                    }
                                });
                                let (sheet, ids) = preview(&draft.theme);
                                let layout = layout_sheet(
                                    &sheet,
                                    &ymind_canvas::EguiMeasure::new(ui.ctx()),
                                    ui.visuals().dark_mode,
                                );
                                let (rect, _) = ui.allocate_exact_size(
                                    vec2(ui.available_width(), 160.0),
                                    egui::Sense::hover(),
                                );
                                ymind_canvas::paint_thumbnail(
                                    ui.painter(),
                                    &sheet,
                                    &layout,
                                    rect,
                                    ui.visuals().dark_mode,
                                );
                                widgets::property_field(ui, t("Topic role"), |ui| {
                                    egui::ComboBox::from_id_salt("custom_role")
                                        .selected_text(t(ROLES[draft.role].1))
                                        .show_ui(ui, |ui| {
                                            for (index, (_, label)) in ROLES.into_iter().enumerate()
                                            {
                                                ui.selectable_value(
                                                    &mut draft.role,
                                                    index,
                                                    t(label),
                                                );
                                            }
                                        })
                                        .response
                                });
                                let resolved =
                                    resolve_style(&sheet, ids[draft.role], ui.visuals().dark_mode);
                                let style = draft.theme.roles.get_mut(ROLES[draft.role].0);
                                ui.push_id(draft.role, |ui| {
                                    edit_role(ui, style, &resolved, &mut self.fonts)
                                });
                                ui.horizontal(|ui| {
                                    if ui.button(t("Reset role")).clicked() {
                                        *style = TopicStyle::default();
                                    }
                                    egui::ComboBox::from_id_salt("role_preset")
                                        .selected_text(t("Role preset"))
                                        .show_ui(ui, |ui| {
                                            for name in PRESETS {
                                                if ui.button(t(name)).clicked() {
                                                    *style = preset(name).unwrap().style;
                                                    ui.close();
                                                }
                                            }
                                        })
                                        .response
                                        .accessible_name(t("Role preset"));
                                });
                            });
                        if let Some(error) = &draft.error {
                            ui.colored_label(ui.visuals().error_fg_color, error);
                        }
                        ui.horizontal_wrapped(|ui| {
                            apply = widgets::primary_button(ui, t("Apply Theme")).clicked();
                            save = widgets::secondary_button(ui, t("Save to Library")).clicked();
                            if widgets::secondary_button(ui, t("Cancel")).clicked() {
                                keep = false;
                            }
                        });
                    });
                if response.should_close() {
                    keep = false;
                }
                if keep && (apply || save) {
                    draft.theme.name = draft.theme.name.trim().into();
                    if let Err(error) = draft.theme.validate() {
                        draft.error = Some(error);
                    } else if save {
                        let mut themes = self.settings.custom_themes.clone();
                        themes.push(draft.theme.clone());
                        match self.store_custom_themes(themes) {
                            Ok(()) => {
                                draft.error = None;
                                self.toasts.success(if self.services.paths.is_some() {
                                    t("Theme saved")
                                } else {
                                    t("Theme saved for this session")
                                });
                            }
                            Err(error) => {
                                draft.error =
                                    Some(format!("{}: {error}", t("Could not save theme")))
                            }
                        }
                    } else {
                        keep = !self.apply_custom_theme(draft.theme.clone());
                    }
                }
            }
        }
        if keep {
            self.theme_dialog = Some(dialog);
        }
    }
}

fn edit_color(ui: &mut Ui, label: &str, color: &mut Color) {
    ui.horizontal(|ui| {
        let label = ui.label(label);
        let mut value = from_color(*color);
        if ui
            .color_edit_button_srgba(&mut value)
            .labelled_by(label.id)
            .changed()
        {
            *color = to_color(value);
        }
    });
}

fn role_color(ui: &mut Ui, label: &'static str, color: &mut Option<Color>, fallback: Color) {
    ui.horizontal(|ui| {
        let mut custom = color.is_some();
        let toggle = ui.checkbox(&mut custom, t(label));
        if toggle.changed() {
            *color = custom.then_some(fallback);
        }
        if let Some(color) = color {
            let mut value = from_color(*color);
            if ui
                .color_edit_button_srgba(&mut value)
                .labelled_by(toggle.id)
                .changed()
            {
                *color = to_color(value);
            }
        }
    });
}

fn role_number(
    ui: &mut Ui,
    label: &'static str,
    value: &mut Option<f32>,
    fallback: f32,
    range: std::ops::RangeInclusive<f32>,
) {
    ui.horizontal(|ui| {
        let mut custom = value.is_some();
        let toggle = ui.checkbox(&mut custom, t(label));
        if toggle.changed() {
            *value = custom.then_some(fallback);
        }
        if let Some(value) = value {
            ui.add(egui::DragValue::new(value).speed(0.1).range(range))
                .labelled_by(toggle.id);
        }
    });
}

fn role_bool(ui: &mut Ui, label: &'static str, value: &mut Option<bool>) {
    widgets::property_field(ui, t(label), |ui| {
        egui::ComboBox::from_id_salt(label)
            .selected_text(match value {
                None => t("Auto"),
                Some(true) => t("On"),
                Some(false) => t("Off"),
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(value, None, t("Auto"));
                ui.selectable_value(value, Some(true), t("On"));
                ui.selectable_value(value, Some(false), t("Off"));
            })
            .response
    });
}

fn edit_role(
    ui: &mut Ui,
    style: &mut TopicStyle,
    resolved: &ResolvedStyle,
    fonts: &mut ymind_ui::fonts::SystemFonts,
) {
    role_color(
        ui,
        "Override Fill",
        &mut style.fill,
        resolved.fill.unwrap_or(Color::TRANSPARENT),
    );
    role_color(
        ui,
        "Override Text Color",
        &mut style.text_color,
        resolved.text_color,
    );
    role_color(
        ui,
        "Override Border Color",
        &mut style.border_color,
        resolved.border_color.unwrap_or(Color::TRANSPARENT),
    );
    role_color(
        ui,
        "Override Line Color",
        &mut style.line_color,
        resolved.line_color,
    );
    role_number(
        ui,
        "Override Font Size",
        &mut style.font_size,
        resolved.font_size,
        6.0..=96.0,
    );
    role_number(
        ui,
        "Override Border Width",
        &mut style.border_width,
        resolved.border_width,
        0.0..=16.0,
    );
    role_number(
        ui,
        "Override Line Width",
        &mut style.line_width,
        resolved.line_width,
        0.0..=16.0,
    );
    role_number(
        ui,
        "Override Topic Width",
        &mut style.width,
        resolved.width.unwrap_or(160.0),
        1.0..=10000.0,
    );
    widgets::property_field(ui, t("Font Family"), |ui| {
        font_selector(ui, fonts, &mut style.font_family, t("Auto"))
    });
    widgets::property_field(ui, t("Shape"), |ui| {
        egui::ComboBox::from_id_salt("role_shape")
            .selected_text(
                style
                    .shape
                    .map(|shape| t(shape.label()))
                    .unwrap_or_else(|| t("Auto")),
            )
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut style.shape, None, t("Auto"));
                for shape in [
                    TopicShape::RoundedRect,
                    TopicShape::Rect,
                    TopicShape::Ellipse,
                    TopicShape::Capsule,
                    TopicShape::Diamond,
                    TopicShape::Underline,
                    TopicShape::None,
                ] {
                    ui.selectable_value(&mut style.shape, Some(shape), t(shape.label()));
                }
            })
            .response
    });
    widgets::property_field(ui, t("Line Shape"), |ui| {
        egui::ComboBox::from_id_salt("role_line_shape")
            .selected_text(
                style
                    .line_shape
                    .map(|shape| t(shape.label()))
                    .unwrap_or_else(|| t("Auto")),
            )
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut style.line_shape, None, t("Auto"));
                for shape in [
                    LineShape::Curve,
                    LineShape::Straight,
                    LineShape::Elbow,
                    LineShape::RoundedElbow,
                ] {
                    ui.selectable_value(&mut style.line_shape, Some(shape), t(shape.label()));
                }
            })
            .response
    });
    widgets::property_field(ui, t("Alignment"), |ui| {
        egui::ComboBox::from_id_salt("role_align")
            .selected_text(
                style
                    .align
                    .map(|align| format!("{align:?}"))
                    .unwrap_or_else(|| t("Auto").into()),
            )
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut style.align, None, t("Auto"));
                for align in [TextAlign::Left, TextAlign::Center, TextAlign::Right] {
                    ui.selectable_value(&mut style.align, Some(align), format!("{align:?}"));
                }
            })
            .response
    });
    for (label, value) in [
        ("Bold", &mut style.bold),
        ("Italic", &mut style.italic),
        ("Underline", &mut style.underline),
        ("Strikethrough", &mut style.strikethrough),
    ] {
        role_bool(ui, label, value);
    }
}
