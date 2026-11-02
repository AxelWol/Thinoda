use egui::{Context, Ui, vec2};
use std::collections::HashSet;
use thinoda_canvas::MapView;
use thinoda_core::{
    CanvasObjectId, CanvasObjectKind, Card, CardKind, Color, Sheet, Topic, TopicId, TopicStyle,
    board::{BoardMember, MIN_STROKE_WIDTH},
    layout::layout_sheet,
    quick_styles::{PRESETS, QuickStyle, preset},
    theme::{Theme, resolve_style},
};
use thinoda_ui::{DialogButton, confirm_dialog_with_labels as confirm_dialog, widgets};

use crate::{
    app::{Cmd, ThinodaApp},
    i18n::t,
};

/// The styleable part of a selection. Groups containing any locked member are
/// kept intact and skipped, just as they are for board gestures.
#[derive(Default)]
pub(crate) struct StyleTargets {
    pub(crate) topics: Vec<TopicId>,
    pub(crate) shapes: Vec<CanvasObjectId>,
    pub(crate) board: bool,
    pub(crate) locked: bool,
    pub(crate) styleable_board: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum StyleColor {
    Fill,
    Border,
    Text,
}

impl StyleColor {
    fn label(self) -> &'static str {
        match self {
            Self::Fill => "Fill",
            Self::Border => "Border",
            Self::Text => "Text Color",
        }
    }
}

impl StyleTargets {
    pub(crate) fn from_view(sheet: &Sheet, view: &MapView) -> Option<Self> {
        if view.selected_object.is_some()
            || view.selection.iter().any(|id| !sheet.contains(*id))
            || view
                .board_selection
                .iter()
                .any(|id| sheet.canvas.object(*id).is_none())
        {
            return None;
        }

        let mut targets = Self::default();
        let mut seen_topics = HashSet::new();
        let direct_topics: HashSet<_> = view.selection.iter().copied().collect();
        let mut direct_cards = HashSet::new();
        let mut members = Vec::new();
        for id in &view.selection {
            if let Some(member) = sheet.board_member_for_topic(*id) {
                direct_cards.insert(*id);
                members.push(member);
            }
        }
        members.extend(
            view.board_selection
                .iter()
                .copied()
                .map(BoardMember::Object),
        );
        if members.is_empty() {
            targets.topics = view
                .selection
                .iter()
                .copied()
                .filter(|id| seen_topics.insert(*id))
                .collect();
            return Some(targets);
        }
        targets.board = true;
        let members = sheet.expand_board_selection(&members).ok()?;
        let locked_groups: HashSet<_> = members
            .iter()
            .filter(|member| sheet.is_board_member_locked(**member))
            .filter_map(|member| sheet.board_member_group(*member))
            .collect();
        let mut eligible_cards = HashSet::new();
        for member in members {
            let styleable = match member {
                BoardMember::Card(_) => true,
                BoardMember::Object(id) => sheet
                    .canvas
                    .object(id)
                    .is_some_and(|object| matches!(object.kind, CanvasObjectKind::Shape { .. })),
            };
            targets.styleable_board |= styleable;
            let locked = sheet.is_board_member_locked(member)
                || sheet
                    .board_member_group(member)
                    .is_some_and(|group| locked_groups.contains(&group));
            if locked {
                targets.locked = true;
                continue;
            }
            if !styleable {
                continue;
            }
            match member {
                BoardMember::Card(id) => {
                    eligible_cards.insert(id);
                    if !direct_topics.contains(&id) && seen_topics.insert(id) {
                        targets.topics.push(id);
                    }
                }
                BoardMember::Object(id) => targets.shapes.push(id),
            }
        }
        // Expanded group members come first; keep directly selected topics in
        // their original order so the last eligible topic remains primary.
        for id in &view.selection {
            if (!direct_cards.contains(id) || eligible_cards.contains(id))
                && seen_topics.insert(*id)
            {
                targets.topics.push(*id);
            }
        }
        Some(targets)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.topics.is_empty() && self.shapes.is_empty()
    }

    pub(crate) fn supports_color(&self, sheet: &Sheet, field: StyleColor) -> bool {
        !self.shapes.is_empty()
            || self.topics.iter().any(|id| {
                field != StyleColor::Border
                    || sheet.topic(*id).is_some_and(|topic| topic.card.is_none())
            })
    }

    /// Only show a selected swatch when all applicable selected items agree.
    pub(crate) fn common_color(
        &self,
        sheet: &Sheet,
        dark: bool,
        field: StyleColor,
    ) -> Option<Color> {
        let mut common = None;
        let mut include = |color: Option<Color>| match common {
            None => {
                common = Some(color);
                true
            }
            Some(previous) => previous == color,
        };
        for id in &self.topics {
            let topic = sheet.topic(*id)?;
            if field == StyleColor::Border && topic.card.is_some() {
                continue;
            }
            let resolved = resolve_style(sheet, *id, dark);
            let color = match field {
                StyleColor::Fill => topic
                    .card
                    .map_or(resolved.fill, |card| Some(card_fill(card, dark))),
                StyleColor::Border => resolved.border_color,
                StyleColor::Text => Some(topic.card.map_or(resolved.text_color, |card| {
                    card_text_color(sheet, topic, card, dark)
                })),
            };
            if !include(color) {
                return None;
            }
        }
        for id in &self.shapes {
            let object = sheet.canvas.object(*id)?;
            let color = match field {
                StyleColor::Fill => object.style.fill,
                StyleColor::Border => object.style.stroke,
                StyleColor::Text => object.style.text_color,
            };
            if !include(color) {
                return None;
            }
        }
        common.flatten()
    }

    fn matches_quick_style(&self, sheet: &Sheet, style: &TopicStyle) -> bool {
        !self.is_empty()
            && self.topics.iter().all(|id| {
                sheet.topic(*id).is_some_and(|topic| {
                    topic.style == *style && topic.card.is_none_or(|card| card.color == style.fill)
                })
            })
            && self.shapes.iter().all(|id| {
                sheet.canvas.object(*id).is_some_and(|object| {
                    let stroke = quick_style_stroke(style);
                    object.style.fill == style.fill
                        && object.style.text_color == style.text_color
                        && object.style.stroke == stroke
                        && (stroke.is_none()
                            || object.style.stroke_width == quick_style_stroke_width(style))
                })
            })
    }
}

fn card_fill(card: Card, dark: bool) -> Color {
    let fallback = match (card.kind, dark) {
        (CardKind::Sticky, false) => Color::rgb(0xFF, 0xF2, 0x9E),
        (CardKind::Sticky, true) => Color::rgb(0x9B, 0x7B, 0x24),
        (CardKind::Text, false) => Color::rgb(0xF5, 0xF7, 0xFA),
        (CardKind::Text, true) => Color::rgb(0x3E, 0x48, 0x58),
        (CardKind::Image, false) => Color::rgb(0xD9, 0xEA, 0xF7),
        (CardKind::Image, true) => Color::rgb(0x35, 0x4C, 0x65),
    };
    card.color.unwrap_or(fallback)
}

fn card_text_color(sheet: &Sheet, topic: &Topic, card: Card, dark: bool) -> Color {
    topic.style.text_color.unwrap_or_else(|| {
        let background = Theme::for_settings(&sheet.theme).background(&sheet.theme, dark);
        Color::highest_contrast_on(card_fill(card, dark).over(background))
    })
}

fn quick_style_stroke(style: &TopicStyle) -> Option<Color> {
    if style.border_width == Some(0.0) {
        None
    } else {
        style.border_color
    }
}

fn quick_style_stroke_width(style: &TopicStyle) -> f32 {
    style
        .border_width
        .filter(|width| *width > 0.0)
        .unwrap_or(2.0)
        .max(MIN_STROKE_WIDTH)
}

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
        if let Err(error) = doc.commit_canvas_drafts() {
            self.toasts.error(error);
            return;
        }
        let id = doc.view.primary().unwrap();
        let sheet = doc.doc.sheet();
        let mut style = resolve_style(sheet, id, doc.view.dark);
        if let Some(topic) = sheet.topic(id)
            && let Some(card) = topic.card
        {
            style.fill = Some(card_fill(card, doc.view.dark));
            style.text_color = card_text_color(sheet, topic, card, doc.view.dark);
        }
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
        let result = (|| {
            let doc = self.doc_mut().unwrap();
            doc.commit_canvas_drafts()?;
            let targets = StyleTargets::from_view(doc.doc.sheet(), &doc.view)
                .filter(|targets| !targets.is_empty())
                .ok_or_else(|| t("Not possible here").to_owned())?;
            doc.doc
                .edit_sheet("Quick Style", |sheet| {
                    sheet.update_style(&targets.topics, |target| *target = style.clone())?;
                    for id in &targets.topics {
                        if let Some(mut card) = sheet.topic(*id).and_then(|topic| topic.card) {
                            card.color = style.fill;
                            sheet
                                .set_topic_card(*id, Some(card))
                                .map_err(thinoda_core::EditError::Invalid)?;
                        }
                    }
                    for id in &targets.shapes {
                        sheet
                            .update_canvas_object(*id, |object| {
                                object.style.fill = style.fill;
                                object.style.stroke = quick_style_stroke(&style);
                                object.style.stroke_width = quick_style_stroke_width(&style);
                                object.style.text_color = style.text_color;
                            })
                            .map_err(thinoda_core::EditError::Invalid)?;
                    }
                    Ok(())
                })
                .map_err(|error| error.to_string())
        })();
        if let Err(error) = result {
            self.toasts.error(error);
        }
    }

    /// Apply the existing Shape/Text swatches to every compatible, unlocked
    /// selection member in a single document transaction.
    pub(crate) fn apply_style_color(&mut self, field: StyleColor, color: Color) {
        let result = (|| {
            let doc = self
                .doc_mut()
                .ok_or_else(|| t("Not possible here").to_owned())?;
            doc.commit_canvas_drafts()?;
            let targets = StyleTargets::from_view(doc.doc.sheet(), &doc.view)
                .filter(|targets| targets.supports_color(doc.doc.sheet(), field))
                .ok_or_else(|| t("Not possible here").to_owned())?;
            doc.doc
                .edit_sheet(field.label(), |sheet| {
                    for id in &targets.topics {
                        let card = sheet.topic(*id).and_then(|topic| topic.card);
                        if field == StyleColor::Border && card.is_some() {
                            continue;
                        }
                        sheet.update_style(&[*id], |style| match field {
                            StyleColor::Fill => style.fill = Some(color),
                            StyleColor::Border => {
                                style.border_color = Some(color);
                                if style.border_width.is_none_or(|width| width <= 0.0) {
                                    style.border_width = Some(2.0);
                                }
                            }
                            StyleColor::Text => style.text_color = Some(color),
                        })?;
                        if field == StyleColor::Fill
                            && let Some(mut card) = card
                        {
                            card.color = Some(color);
                            sheet
                                .set_topic_card(*id, Some(card))
                                .map_err(thinoda_core::EditError::Invalid)?;
                        }
                    }
                    for id in &targets.shapes {
                        sheet
                            .update_canvas_object(*id, |object| match field {
                                StyleColor::Fill => object.style.fill = Some(color),
                                StyleColor::Border => object.style.stroke = Some(color),
                                StyleColor::Text => object.style.text_color = Some(color),
                            })
                            .map_err(thinoda_core::EditError::Invalid)?;
                    }
                    Ok(())
                })
                .map_err(|error| error.to_string())
        })();
        if let Err(error) = result {
            self.toasts.error(error);
        }
    }

    pub(crate) fn reset_selection_style(&mut self) {
        let result = (|| {
            let doc = self
                .doc_mut()
                .ok_or_else(|| t("Not possible here").to_owned())?;
            doc.commit_canvas_drafts()?;
            let targets = StyleTargets::from_view(doc.doc.sheet(), &doc.view)
                .filter(|targets| !targets.is_empty())
                .ok_or_else(|| t("Not possible here").to_owned())?;
            doc.doc
                .edit_sheet("Reset Style", |sheet| {
                    sheet.update_style(&targets.topics, |style| *style = TopicStyle::default())?;
                    for id in &targets.topics {
                        if let Some(mut card) = sheet.topic(*id).and_then(|topic| topic.card) {
                            card.color = None;
                            sheet
                                .set_topic_card(*id, Some(card))
                                .map_err(thinoda_core::EditError::Invalid)?;
                        }
                    }
                    for id in &targets.shapes {
                        sheet
                            .update_canvas_object(*id, |object| {
                                object.style = Default::default();
                            })
                            .map_err(thinoda_core::EditError::Invalid)?;
                    }
                    Ok(())
                })
                .map_err(|error| error.to_string())
        })();
        if let Err(error) = result {
            self.toasts.error(error);
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
        let touch_allowed = widgets::touch_activation_allowed(ui.ctx());
        let enabled = self.command_available(&Cmd::ApplyQuickStyle(TopicStyle::default()));
        let targets = self
            .doc()
            .and_then(|doc| StyleTargets::from_view(doc.doc.sheet(), &doc.view));
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
            let board = targets.as_ref().is_some_and(|targets| targets.board);
            widgets::caption(
                ui,
                t(if board {
                    "Changes apply to unlocked selected items that support each setting."
                } else {
                    "Replaces topic appearance only. Reset Style restores theme inheritance."
                }),
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
                                        targets.as_ref().is_some_and(|targets| {
                                            targets
                                                .matches_quick_style(doc.doc.sheet(), &style.style)
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
                                        crate::ui::scroll_menu(ui, |ui| {
                                            if ui.button(t("Remove Quick Style…")).clicked()
                                                && touch_allowed
                                            {
                                                remove = Some(style.name.clone());
                                                ui.close();
                                            }
                                        });
                                    });
                                }
                            });
                            if *custom && ui.small_button(t("Remove…")).clicked() && touch_allowed
                            {
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
                .clicked()
                && touch_allowed;
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
        let touch_allowed = widgets::touch_activation_allowed(ctx);
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
                let labels = crate::i18n::dialog_labels();
                let response = thinoda_ui::Dialog::new(
                    egui::Id::unique("save_quick_style"),
                    t("Save Quick Style"),
                )
                .icon(thinoda_ui::icons::QUICK_STYLE)
                .labels(&labels)
                .modal(true)
                .close_on_backdrop(true)
                .default_width(360.0)
                .default_height(220.0)
                .min_size(egui::vec2(300.0, 160.0))
                .resizable(false)
                .show(ctx, |ui| {
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
                        save = (widgets::primary_button(ui, t("Save")).clicked()
                            || (edit.lost_focus()
                                && ui.input_mut(|input| {
                                    input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                                })))
                            && touch_allowed;
                        if widgets::secondary_button(ui, t("Cancel")).clicked() && touch_allowed {
                            keep = false;
                        }
                    });
                });
                if response.close_requested && touch_allowed {
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
                    &crate::i18n::dialog_labels(),
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
