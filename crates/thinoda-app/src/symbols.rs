//! Symbol color controls shared by the insertion surfaces and instance editors.

use egui::{RichText, Ui};
use thinoda_canvas::Action;
use thinoda_core::{Color, TopicId, markers};

use crate::{
    app::{Cmd, ThinodaApp},
    i18n::t,
};

pub(crate) const EXPORT_WARNING: &str = "Editable exports lose symbol colors, except Thinoda and unchanged XMind files reopened in Thinoda. XMind itself shows standard icons; third-party edits can discard custom colors.";

fn color_control(ui: &mut Ui, color: &mut Option<Color>, fallback: Color, submenu: bool) -> bool {
    let before = *color;
    ui.horizontal_wrapped(|ui| {
        let label = ui.label(t("Symbol color"));
        let mut value = thinoda_canvas::color32(color.unwrap_or(fallback));
        let changed = if submenu {
            // The standard color button replaces the context menu's root popup state.
            let mut changed = false;
            let mut button = egui::containers::menu::SubMenuButton::new(t("Symbol color"));
            button.sub_menu = button.sub_menu.config(
                egui::containers::menu::MenuConfig::new()
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
            );
            button.ui(ui, |ui| {
                ui.spacing_mut().slider_width = 275.0;
                crate::ui::scroll_menu(ui, |ui| {
                    changed = egui::color_picker::color_picker_color32(
                        ui,
                        &mut value,
                        egui::color_picker::Alpha::BlendOrAdditive,
                    );
                });
            });
            changed
        } else {
            ui.color_edit_button_srgba(&mut value)
                .labelled_by(label.id)
                .changed()
        };
        if changed {
            let [r, g, b, a] = value.to_srgba_unmultiplied();
            *color = Some(Color::rgba(r, g, b, a));
        }
        let id = ui.scope_id().with("symbol_hex");
        let current = color.unwrap_or(fallback);
        let mut state = ui
            .data_mut(|d| d.get_temp::<(Color, String)>(id))
            .unwrap_or((current, current.to_hex()));
        if state.0 != current {
            state = (current, current.to_hex());
        }
        let response = ui
            .add(egui::TextEdit::singleline(&mut state.1).desired_width(90.0))
            .labelled_by(label.id);
        if response.changed()
            && let Some(c) = Color::from_hex(&state.1)
        {
            *color = Some(c);
            state.0 = c;
        }
        if Color::from_hex(&state.1).is_none() {
            ui.label(t("Invalid color"));
        }
        ui.data_mut(|d| d.insert_temp(id, state));
        if ui.button(t("Default symbol color")).clicked() {
            *color = None;
            ui.data_mut(|d| d.remove::<(Color, String)>(id));
        }
    });
    before != *color
}

fn contrast(ui: &mut Ui, id: &str, color: Color, background: Color) -> Option<Color> {
    let ratio = color.contrast_on(background);
    let threshold = markers::contrast_threshold(id);
    let pass = ratio >= threshold;
    let status = if pass { t("Pass") } else { t("Low contrast") };
    ui.label(format!("{}: {ratio:.2}:1 / {threshold}:1 — {status}", t("Contrast")))
        .on_hover_text(t("Contrast compares the symbol with its topic background; it is not an overall accessibility assessment."));
    if !pass && ui.button(t("Improve contrast")).clicked() {
        return Some(Color::highest_contrast_on(background));
    }
    None
}

impl ThinodaApp {
    pub(crate) fn symbol_insertion_color(&mut self, ui: &mut Ui) {
        color_control(
            ui,
            &mut self.symbol_color,
            markers::default_color("symbol-plus"),
            false,
        );
        ui.small(t("Graphical symbols: 3:1; month and week text: 4.5:1."));
        ui.small(t(EXPORT_WARNING));
        if let Some(d) = self.doc()
            && let Some(id) = d.view.primary()
        {
            let bg = d.view.symbol_background(&d.doc, id, None);
            if let Some(color) = contrast(
                ui,
                "month-jan",
                self.symbol_color
                    .unwrap_or(markers::default_color("symbol-plus")),
                bg,
            ) {
                self.symbol_color = Some(color);
            }
        }
    }

    pub(crate) fn symbol_button(
        &self,
        ui: &mut Ui,
        marker: &markers::Marker,
        selected: bool,
    ) -> bool {
        let selection = self
            .doc()
            .map(|d| !d.view.selection.is_empty())
            .unwrap_or(false);
        let color = self
            .symbol_color
            .unwrap_or_else(|| markers::default_color(marker.id));
        let ratio = self.doc().and_then(|d| {
            d.view
                .selection
                .iter()
                .map(|id| {
                    let index = d
                        .doc
                        .sheet()
                        .topic(*id)
                        .and_then(|topic| topic.markers.iter().position(|m| m == marker.id));
                    color.contrast_on(d.view.symbol_background(&d.doc, *id, index))
                })
                .reduce(f32::min)
        });
        let threshold = markers::contrast_threshold(marker.id);
        let mut name = t(marker.name).to_string();
        if let Some(ratio) = ratio {
            let status = if ratio >= threshold {
                t("Pass")
            } else {
                t("Low contrast")
            };
            name.push_str(&format!(
                " — {}: {ratio:.2}:1 / {threshold}:1 — {status}",
                t("Contrast")
            ));
        }
        let response = ui
            .add_enabled(
                selection,
                egui::Button::new(" ")
                    .selected(selected)
                    .min_size(egui::vec2(32.0, 32.0)),
            )
            .accessible_name(t(marker.name))
            .on_hover_text(&name);
        // Match the actual canvas icons rather than platform emoji colors.
        if ui.is_rect_visible(response.rect) {
            ui.painter().rect_filled(
                response.rect.shrink(3.0),
                3.0,
                ui.visuals().widgets.inactive.bg_fill,
            );
            thinoda_canvas::paint_marker_colored(
                ui.painter(),
                marker.id,
                response.rect.shrink(7.0),
                false,
                self.symbol_color,
            );
        }
        response.clicked()
    }

    pub(crate) fn symbol_instance(
        &self,
        ui: &mut Ui,
        id: TopicId,
        index: usize,
        submenu: bool,
    ) -> Option<Cmd> {
        let d = self.doc()?;
        let topic = d.doc.sheet().topic(id)?;
        let marker = topic.markers.get(index)?;
        let mut color = topic.marker_colors.get(&index).copied();
        let fallback = markers::default_color(marker);
        let bg = d.view.symbol_background(&d.doc, id, Some(index));
        let mut command = None;
        ui.push_id((id, index), |ui| {
            ui.horizontal(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(32.0, 32.0), egui::Sense::hover());
                ui.painter()
                    .rect_filled(rect, 2.0, thinoda_canvas::color32(bg));
                thinoda_canvas::paint_marker_colored(
                    ui.painter(),
                    marker,
                    rect.shrink(6.0),
                    false,
                    color,
                );
                ui.label(
                    RichText::new(markers::find(marker).map(|m| t(m.name)).unwrap_or(marker))
                        .strong(),
                );
            });
            if color_control(ui, &mut color, fallback, submenu) {
                command = Some(Cmd::Canvas(Action::SetMarkerColor(id, index, color)));
            }
            if let Some(improved) = contrast(ui, marker, color.unwrap_or(fallback), bg) {
                command = Some(Cmd::Canvas(Action::SetMarkerColor(
                    id,
                    index,
                    Some(improved),
                )));
            }
            if ui.button(t("Remove Symbol")).clicked() {
                command = Some(Cmd::Canvas(Action::RemoveMarker(id, index)));
                ui.close();
            }
        });
        command
    }

    pub(crate) fn placed_symbols(&self, ui: &mut Ui, command: &mut Option<Cmd>) {
        let Some(d) = self.doc() else { return };
        for id in &d.view.selection {
            let Some(topic) = d.doc.sheet().topic(*id) else {
                continue;
            };
            if topic.markers.is_empty() {
                continue;
            }
            ui.heading(t("Placed symbols"));
            ui.label(&topic.title);
            for index in 0..topic.markers.len() {
                ui.group(|ui| {
                    if let Some(cmd) = self.symbol_instance(ui, *id, index, false) {
                        *command = Some(cmd);
                    }
                });
            }
        }
    }
}
