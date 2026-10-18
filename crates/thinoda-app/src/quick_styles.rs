use egui::{Context, Ui, vec2};
use thinoda_core::{
    Sheet, TopicStyle,
    layout::layout_sheet,
    quick_styles::{PRESETS, QuickStyle, preset},
    theme::resolve_style,
};
use thinoda_ui::{DialogButton, confirm_dialog, widgets};

use crate::{
    app::{Cmd, ThinodaApp},
    i18n::t,
};

pub(crate) enum StyleDialog {
    Save {
        draft: QuickStyle,
        error: Option<String>,
    },
    Remove(String),
}

impl ThinodaApp {
    pub(crate) fn begin_quick_style(&mut self) {
        if !self.command_available(&Cmd::SaveQuickStyle) {
            self.toasts
                .warning(t("Select one topic to save a quick style."));
            return;
        }
        let doc = self.doc_mut().unwrap();
        doc.view.commit_edit(&mut doc.doc);
        let style = resolve_style(doc.doc.sheet(), doc.view.primary().unwrap(), doc.view.dark);
        self.quick_style_dialog = Some(StyleDialog::Save {
            draft: QuickStyle::from_appearance(String::new(), &style),
            error: None,
        });
    }

    pub(crate) fn apply_quick_style(&mut self, style: TopicStyle) {
        if !self.command_available(&Cmd::ApplyQuickStyle(style.clone())) {
            self.toasts.warning(t("Not possible here"));
            return;
        }
        if let Err(error) = (QuickStyle {
            name: "Quick style".into(),
            style: style.clone(),
        })
        .validate()
        {
            self.toasts.error(error);
            return;
        }
        let doc = self.doc_mut().unwrap();
        doc.view.commit_edit(&mut doc.doc);
        let selection = doc.view.selection.clone();
        if let Err(error) = doc.doc.edit_sheet("Quick Style", |sheet| {
            sheet.update_style(&selection, |target| *target = style.clone())
        }) {
            self.toasts.error(error.to_string());
        }
    }

    fn store_quick_styles(&mut self, styles: Vec<QuickStyle>) -> Result<(), String> {
        thinoda_platform::Settings::validate_quick_styles(&styles)?;
        let mut settings = self.settings.clone();
        settings.quick_styles = styles;
        if let Some(paths) = &self.services.paths {
            settings
                .save_with_credentials(paths, &thinoda_platform::credentials::NativeCredentialStore)
                .map_err(|error| error.to_string())?;
        }
        self.settings = settings;
        self.font_source = None;
        Ok(())
    }

    pub(crate) fn add_quick_style_commands(&self, commands: &mut Vec<(Cmd, thinoda_ui::Command)>) {
        let styles = PRESETS
            .into_iter()
            .map(|name| {
                let mut style = preset(name).unwrap();
                style.name = t(name).into();
                style
            })
            .chain(self.settings.quick_styles.iter().cloned());
        for (index, style) in styles.enumerate() {
            commands.push((
                Cmd::ApplyQuickStyle(style.style),
                thinoda_ui::Command::new(
                    format!("quick-style-{index}"),
                    format!("{}: {}", t("Quick Style"), style.name),
                ),
            ));
        }
    }

    pub(crate) fn quick_styles(&mut self, ui: &mut Ui) {
        let enabled = self.command_available(&Cmd::ApplyQuickStyle(TopicStyle::default()));
        let styles: Vec<_> = PRESETS
            .into_iter()
            .map(|name| {
                let mut style = preset(name).unwrap();
                style.name = t(name).into();
                (style, false)
            })
            .chain(
                self.settings
                    .quick_styles
                    .iter()
                    .cloned()
                    .map(|style| (style, true)),
            )
            .collect();
        let mut apply = None;
        let mut remove = None;
        let mut save = false;
        widgets::section(ui, t("Quick Styles"), true, |ui| {
            widgets::caption(
                ui,
                t("Replaces topic appearance only. Reset Style restores theme inheritance."),
            );
            let columns = ((ui.available_width() + 8.0) / 116.0).floor().max(1.0) as usize;
            egui::Grid::new("quick_styles")
                .num_columns(columns)
                .spacing(vec2(8.0, 8.0))
                .show(ui, |ui| {
                    for (index, (style, custom)) in styles.iter().enumerate() {
                        ui.vertical(|ui| {
                            ui.set_width(108.0);
                            ui.add_enabled_ui(enabled, |ui| {
                                let selected = enabled
                                    && self.doc().is_some_and(|doc| {
                                        doc.view.selection.iter().all(|id| {
                                            doc.doc
                                                .sheet()
                                                .topic(*id)
                                                .is_some_and(|topic| topic.style == style.style)
                                        })
                                    });
                                let (response, activated) = widgets::preview_card(
                                    ui,
                                    &format!("{}: {}", t("Quick Style"), style.name),
                                    selected,
                                    108.0,
                                    40.0,
                                    |ui, rect| {
                                        let mut sample = Sheet::new("Preview", "Idea");
                                        sample.topic_mut(sample.root).unwrap().style =
                                            style.style.clone();
                                        let layout = layout_sheet(
                                            &sample,
                                            &thinoda_canvas::EguiMeasure::new(ui.ctx()),
                                            ui.visuals().dark_mode,
                                        );
                                        thinoda_canvas::paint_thumbnail(
                                            ui.painter(),
                                            &sample,
                                            &layout,
                                            rect,
                                            ui.visuals().dark_mode,
                                        );
                                    },
                                );
                                if activated {
                                    apply = Some(style.style.clone());
                                }
                                if *custom {
                                    response.context_menu(|ui| {
                                        if ui.button(t("Remove Quick Style…")).clicked() {
                                            remove = Some(style.name.clone());
                                            ui.close();
                                        }
                                    });
                                }
                            });
                            if *custom && ui.small_button(t("Remove…")).clicked() {
                                remove = Some(style.name.clone());
                            }
                        });
                        if (index + 1) % columns == 0 {
                            ui.end_row();
                        }
                    }
                });
            save = ui
                .add_enabled(
                    self.command_available(&Cmd::SaveQuickStyle),
                    egui::Button::new(t("Save Quick Style…")),
                )
                .clicked();
        });
        if let Some(style) = apply {
            self.apply_quick_style(style);
        }
        if save {
            self.begin_quick_style();
        }
        if let Some(name) = remove {
            self.quick_style_dialog = Some(StyleDialog::Remove(name));
        }
    }

    pub(crate) fn quick_style_dialog(&mut self, ctx: &Context) {
        let Some(mut dialog) = self.quick_style_dialog.take() else {
            return;
        };
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            return;
        }
        let mut keep = true;
        match &mut dialog {
            StyleDialog::Save { draft, error } => {
                let mut save = false;
                let response =
                    egui::Modal::new(egui::Id::unique("save_quick_style")).show(ctx, |ui| {
                        ui.set_width(360.0);
                        ui.heading(t("Save Quick Style"));
                        ui.label(t(
                            "Saves fixed colors and typography, without topic content.",
                        ));
                        let label = ui.label(t("Style name"));
                        let edit = ui
                            .text_edit_singleline(&mut draft.name)
                            .labelled_by(label.id);
                        if let Some(error) = error.as_ref() {
                            ui.colored_label(ui.visuals().error_fg_color, error);
                        }
                        ui.horizontal(|ui| {
                            save = widgets::primary_button(ui, t("Save")).clicked()
                                || (edit.lost_focus()
                                    && ui.input_mut(|input| {
                                        input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                                    }));
                            if widgets::secondary_button(ui, t("Cancel")).clicked() {
                                keep = false;
                            }
                        });
                    });
                if response.should_close() {
                    keep = false;
                }
                if save && keep {
                    draft.name = draft.name.trim().into();
                    let mut styles = self.settings.quick_styles.clone();
                    styles.push(draft.clone());
                    match self.store_quick_styles(styles) {
                        Ok(()) => {
                            keep = false;
                            self.toasts.success(if self.services.paths.is_some() {
                                t("Quick style saved")
                            } else {
                                t("Quick style saved for this session")
                            });
                        }
                        Err(message) => {
                            log::warn!("Could not save quick style: {message}");
                            *error =
                                Some(format!("{}: {message}", t("Could not save quick style")));
                        }
                    }
                }
            }
            StyleDialog::Remove(name) => {
                if let Some(choice) = confirm_dialog(
                    ctx,
                    "remove_quick_style",
                    t("Remove Quick Style?"),
                    &format!("{}: {name}", t("Quick Style")),
                    &[
                        (t("Remove"), DialogButton::Danger),
                        (t("Cancel"), DialogButton::Secondary),
                    ],
                ) {
                    keep = false;
                    if choice == 0 {
                        let styles = self
                            .settings
                            .quick_styles
                            .iter()
                            .filter(|style| style.name != *name)
                            .cloned()
                            .collect();
                        if let Err(error) = self.store_quick_styles(styles) {
                            self.toasts
                                .error(format!("{}: {error}", t("Could not save quick style")));
                        }
                    }
                }
            }
        }
        if keep {
            self.quick_style_dialog = Some(dialog);
        }
    }
}
