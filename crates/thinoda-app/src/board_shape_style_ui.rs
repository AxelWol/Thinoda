//! The inspector's Shape style section: dash, gradient, fill pattern, shadow,
//! glass and flip for one or more selected shapes.

use egui::{Color32, Ui};
use thinoda_core::{
    CanvasObject, CanvasObjectKind, Color, ObjectStyle,
    board::BoardMember,
    shape_style::{DashPattern, FillPattern, GradientDirection, ShapeDash, ShapeGradient},
};
use thinoda_ui::widgets;

use crate::{app::ThinodaApp, i18n::t};

/// Dash presets offered in the inspector, as draw.io `dashPattern` values.
const DASH_PRESETS: [&[f32]; 5] = [
    &[3.0, 3.0],
    &[1.0, 2.0],
    &[8.0, 4.0],
    &[8.0, 4.0, 1.0, 4.0],
    &[12.0, 6.0],
];
const DEFAULT_GRADIENT: Color = Color::rgb(126, 166, 224);

#[derive(Clone, Copy, Debug, PartialEq)]
enum ShapeStyleChange {
    Dash(Option<ShapeDash>),
    Gradient(Option<ShapeGradient>),
    FillPattern(FillPattern),
    Shadow(bool),
    Glass(bool),
    FlipH(bool),
    FlipV(bool),
}

impl ShapeStyleChange {
    fn apply(self, style: &mut ObjectStyle) {
        match self {
            Self::Dash(dash) => style.dash = dash,
            Self::Gradient(gradient) => style.gradient = gradient,
            Self::FillPattern(pattern) => style.fill_pattern = pattern,
            Self::Shadow(on) => style.shadow = on,
            Self::Glass(on) => style.glass = on,
            Self::FlipH(on) => style.flip_h = on,
            Self::FlipV(on) => style.flip_v = on,
        }
    }
}

fn shape_style_controls(ui: &mut Ui, style: &ObjectStyle) -> Option<ShapeStyleChange> {
    let mut change = None;
    ui.strong(t("Shape style"));
    let mut dashed = style.dash.is_some();
    if ui.checkbox(&mut dashed, t("Dashed outline")).changed() {
        change = Some(ShapeStyleChange::Dash(dashed.then(ShapeDash::default)));
    }
    if let Some(dash) = style.dash {
        widgets::property_field(ui, t("Dash pattern"), |ui| {
            egui::ComboBox::from_id_salt("shape_dash_pattern")
                .selected_text(dash.pattern.to_drawio())
                .show_ui(ui, |ui| {
                    for values in DASH_PRESETS {
                        let pattern = DashPattern::new(values).expect("preset dash is valid");
                        if ui
                            .selectable_label(pattern == dash.pattern, pattern.to_drawio())
                            .clicked()
                            && pattern != dash.pattern
                        {
                            change =
                                Some(ShapeStyleChange::Dash(Some(ShapeDash { pattern, ..dash })));
                        }
                    }
                })
                .response
        });
        let mut fixed = dash.fixed;
        if ui.checkbox(&mut fixed, t("Fixed dash length")).changed() {
            change = Some(ShapeStyleChange::Dash(Some(ShapeDash { fixed, ..dash })));
        }
    }
    let mut gradient = style.gradient.is_some();
    if ui.checkbox(&mut gradient, t("Gradient")).changed() {
        change = Some(ShapeStyleChange::Gradient(gradient.then_some(
            ShapeGradient {
                color: DEFAULT_GRADIENT,
                direction: GradientDirection::default(),
            },
        )));
    }
    if let Some(gradient) = style.gradient {
        widgets::property_field(ui, t("Gradient direction"), |ui| {
            egui::ComboBox::from_id_salt("shape_gradient_direction")
                .selected_text(t(gradient.direction.label()))
                .show_ui(ui, |ui| {
                    for direction in GradientDirection::ALL {
                        if ui
                            .selectable_label(direction == gradient.direction, t(direction.label()))
                            .clicked()
                            && direction != gradient.direction
                        {
                            change = Some(ShapeStyleChange::Gradient(Some(ShapeGradient {
                                direction,
                                ..gradient
                            })));
                        }
                    }
                })
                .response
        });
        ui.label(t("Gradient color"));
        let color = gradient.color;
        let selected = Some(Color32::from_rgb(color.r, color.g, color.b));
        if let Some(picked) = widgets::color_swatches(ui, &crate::ui::SWATCHES, selected) {
            let [r, g, b, _] = picked.to_srgba_unmultiplied();
            let color = Color::rgba(r, g, b, color.a);
            if color != gradient.color {
                change = Some(ShapeStyleChange::Gradient(Some(ShapeGradient {
                    color,
                    ..gradient
                })));
            }
        }
    }
    widgets::property_field(ui, t("Fill pattern"), |ui| {
        egui::ComboBox::from_id_salt("shape_fill_pattern")
            .selected_text(t(style.fill_pattern.label()))
            .show_ui(ui, |ui| {
                for pattern in FillPattern::ALL {
                    if ui
                        .selectable_label(pattern == style.fill_pattern, t(pattern.label()))
                        .clicked()
                        && pattern != style.fill_pattern
                    {
                        change = Some(ShapeStyleChange::FillPattern(pattern));
                    }
                }
            })
            .response
    });
    type Toggle = (&'static str, bool, fn(bool) -> ShapeStyleChange);
    let toggles: [Toggle; 4] = [
        ("Shadow", style.shadow, ShapeStyleChange::Shadow),
        ("Glass", style.glass, ShapeStyleChange::Glass),
        ("Flip horizontally", style.flip_h, ShapeStyleChange::FlipH),
        ("Flip vertically", style.flip_v, ShapeStyleChange::FlipV),
    ];
    for (label, mut value, make) in toggles {
        if ui.checkbox(&mut value, t(label)).changed() {
            change = Some(make(value));
        }
    }
    change
}

impl ThinodaApp {
    /// Shape style section for selected shapes; one change edits every
    /// selected shape in a single undo step. Returns whether it was shown.
    pub(crate) fn board_shape_style_properties(&mut self, ui: &mut Ui) -> bool {
        let Some(doc) = self.doc() else { return false };
        let view = &doc.view;
        if view.board_selection.is_empty() || !view.selection.is_empty() {
            return false;
        }
        let sheet = doc.doc.sheet();
        let shapes: Option<Vec<&CanvasObject>> = view
            .board_selection
            .iter()
            .map(|id| {
                sheet
                    .canvas
                    .object(*id)
                    .filter(|object| matches!(object.kind, CanvasObjectKind::Shape { .. }))
            })
            .collect();
        let Some(shapes) = shapes else { return false };
        let ids: Vec<_> = shapes.iter().map(|object| object.id).collect();
        let style = shapes[0].style;
        let members: Vec<_> = ids.iter().map(|id| BoardMember::Object(*id)).collect();
        let locked = sheet
            .expand_board_selection(&members)
            .map_or(true, |members| {
                members
                    .iter()
                    .any(|member| sheet.is_board_member_locked(*member))
            });
        let enabled = !locked && !view.has_active_canvas_gesture();
        let change = ui
            .push_id(("board_shape_style", ids[0]), |ui| {
                ui.separator();
                ui.add_enabled_ui(enabled, |ui| shape_style_controls(ui, &style))
                    .inner
            })
            .inner;
        if let Some(change) = change {
            let result = (|| {
                let doc = self
                    .doc_mut()
                    .ok_or_else(|| t("Not possible here").to_owned())?;
                doc.commit_canvas_drafts()?;
                doc.doc
                    .update_canvas_objects("Shape style", &ids, |object| {
                        change.apply(&mut object.style)
                    })
                    .map_err(|error| error.to_string())
            })();
            if let Err(error) = result {
                self.toasts.error(error);
            }
        }
        true
    }
}

#[cfg(test)]
mod tests;
