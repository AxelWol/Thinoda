//! The inspector's label text section: font, decorations, alignment, label
//! position, spacing, wrap, label box and vertical text for selected shapes.

use std::sync::Arc;

use egui::{Color32, Ui};
use thinoda_core::{
    CanvasObject, CanvasObjectId, CanvasObjectKind, Color, TextAlign,
    board::BoardMember,
    label_style::{
        LabelStyle, LabelVAlign, MAX_LABEL_FONT_SIZE, MAX_LABEL_SPACING, MIN_LABEL_FONT_SIZE,
    },
};
use thinoda_ui::widgets;

use crate::{app::ThinodaApp, i18n::t};

const DEFAULT_BACKGROUND: Color = Color::rgb(255, 255, 255);
const DEFAULT_BORDER: Color = Color::rgb(51, 51, 51);
const ALIGNMENTS: [(TextAlign, &str); 3] = [
    (TextAlign::Left, "Left"),
    (TextAlign::Center, "Center"),
    (TextAlign::Right, "Right"),
];
const VERTICAL_ALIGNMENTS: [(LabelVAlign, &str); 3] = [
    (LabelVAlign::Top, "Top"),
    (LabelVAlign::Middle, "Middle"),
    (LabelVAlign::Bottom, "Bottom"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    All,
    Top,
    Left,
    Bottom,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
enum LabelStyleChange {
    Family(Option<String>),
    Size(f32),
    Bold(bool),
    Italic(bool),
    Underline(bool),
    Strikethrough(bool),
    Align(TextAlign),
    VerticalAlign(LabelVAlign),
    Position(TextAlign),
    VerticalPosition(LabelVAlign),
    Spacing(Side, f32),
    Wrap(bool),
    Background(Option<Color>),
    Border(Option<Color>),
    Vertical(bool),
}

impl LabelStyleChange {
    fn apply(&self, style: &mut LabelStyle) {
        match self {
            Self::Family(family) => style.font_family = family.clone(),
            Self::Size(size) => style.font_size = Some(*size),
            Self::Bold(on) => style.bold = *on,
            Self::Italic(on) => style.italic = *on,
            Self::Underline(on) => style.underline = *on,
            Self::Strikethrough(on) => style.strikethrough = *on,
            Self::Align(align) => style.align = *align,
            Self::VerticalAlign(align) => style.vertical_align = *align,
            Self::Position(position) => style.position = *position,
            Self::VerticalPosition(position) => style.vertical_position = *position,
            Self::Spacing(side, value) => {
                let field = match side {
                    Side::All => &mut style.spacing,
                    Side::Top => &mut style.spacing_top,
                    Side::Left => &mut style.spacing_left,
                    Side::Bottom => &mut style.spacing_bottom,
                    Side::Right => &mut style.spacing_right,
                };
                *field = *value;
            }
            Self::Wrap(on) => style.wrap = *on,
            Self::Background(color) => style.background = *color,
            Self::Border(color) => style.border = *color,
            Self::Vertical(on) => style.vertical = *on,
        }
    }

    fn apply_to(&self, object: &mut CanvasObject) {
        let mut style = object.label_style.as_deref().cloned().unwrap_or_default();
        self.apply(&mut style);
        object.label_style = (!style.is_default()).then(|| Arc::new(style));
    }
}

/// Numeric field that commits once, when dragging or typing finishes.
fn draft_number(
    ui: &mut Ui,
    label: &'static str,
    id: CanvasObjectId,
    stored: f32,
    range: std::ops::RangeInclusive<f32>,
) -> Option<f32> {
    let slot = ui.make_persistent_id(("label_style_draft", id, label));
    let mut value = ui.data(|data| data.get_temp::<f32>(slot)).unwrap_or(stored);
    let response = widgets::property_field(ui, t(label), |ui| {
        ui.add(egui::DragValue::new(&mut value).speed(0.5).range(range))
    });
    let editing = response.dragged() || response.has_focus();
    if editing {
        ui.data_mut(|data| data.insert_temp(slot, value));
        return None;
    }
    ui.data_mut(|data| data.remove::<f32>(slot));
    ((value - stored).abs() > 0.01).then_some(value)
}

fn choice<T: Copy + PartialEq>(
    ui: &mut Ui,
    salt: &str,
    label: &'static str,
    current: T,
    options: &[(T, &'static str)],
) -> Option<T> {
    let mut picked = None;
    let selected = options
        .iter()
        .find(|(value, _)| *value == current)
        .map_or("", |(_, name)| name);
    widgets::property_field(ui, t(label), |ui| {
        egui::ComboBox::from_id_salt(salt)
            .selected_text(t(selected))
            .show_ui(ui, |ui| {
                for (value, name) in options {
                    if ui.selectable_label(*value == current, t(name)).clicked()
                        && *value != current
                    {
                        picked = Some(*value);
                    }
                }
            })
            .response
    });
    picked
}

fn color_option(
    ui: &mut Ui,
    label: &'static str,
    current: Option<Color>,
    fallback: Color,
) -> Option<Option<Color>> {
    let mut change = None;
    let mut on = current.is_some();
    if ui.checkbox(&mut on, t(label)).changed() {
        change = Some(on.then_some(fallback));
    }
    if let Some(color) = current {
        let selected = Some(Color32::from_rgb(color.r, color.g, color.b));
        if let Some(picked) = widgets::color_swatches(ui, &crate::ui::SWATCHES, selected) {
            let [r, g, b, _] = picked.to_srgba_unmultiplied();
            let picked = Color::rgb(r, g, b);
            if picked != color {
                change = Some(Some(picked));
            }
        }
    }
    change
}

fn label_style_controls(
    ui: &mut Ui,
    fonts: &mut thinoda_ui::fonts::SystemFonts,
    id: CanvasObjectId,
    style: &LabelStyle,
) -> Option<LabelStyleChange> {
    let mut change = None;
    ui.strong(t("Label text style"));
    let mut family = style.font_family.clone();
    widgets::property_field(ui, t("Font Family"), |ui| {
        crate::ui::font_selector(ui, fonts, &mut family, t("Default"))
    });
    if family != style.font_family {
        change = Some(LabelStyleChange::Family(family));
    }
    if let Some(size) = draft_number(
        ui,
        "Font Size",
        id,
        style.size(),
        MIN_LABEL_FONT_SIZE..=MAX_LABEL_FONT_SIZE,
    ) {
        change = Some(LabelStyleChange::Size(size));
    }
    type Toggle = (&'static str, bool, fn(bool) -> LabelStyleChange);
    let toggles: [Toggle; 4] = [
        ("Bold", style.bold, LabelStyleChange::Bold),
        ("Italic", style.italic, LabelStyleChange::Italic),
        ("Underline", style.underline, LabelStyleChange::Underline),
        (
            "Strikethrough",
            style.strikethrough,
            LabelStyleChange::Strikethrough,
        ),
    ];
    ui.horizontal_wrapped(|ui| {
        for (label, mut value, make) in toggles {
            if ui.checkbox(&mut value, t(label)).changed() {
                change = Some(make(value));
            }
        }
    });
    if let Some(align) = choice(ui, "label_align", "Alignment", style.align, &ALIGNMENTS) {
        change = Some(LabelStyleChange::Align(align));
    }
    if let Some(align) = choice(
        ui,
        "label_vertical_align",
        "Vertical alignment",
        style.vertical_align,
        &VERTICAL_ALIGNMENTS,
    ) {
        change = Some(LabelStyleChange::VerticalAlign(align));
    }
    if let Some(position) = choice(
        ui,
        "label_position",
        "Label position",
        style.position,
        &ALIGNMENTS,
    ) {
        change = Some(LabelStyleChange::Position(position));
    }
    if let Some(position) = choice(
        ui,
        "label_vertical_position",
        "Vertical label position",
        style.vertical_position,
        &VERTICAL_ALIGNMENTS,
    ) {
        change = Some(LabelStyleChange::VerticalPosition(position));
    }
    let sides = [
        (Side::All, "Label spacing", style.spacing),
        (Side::Top, "Top spacing", style.spacing_top),
        (Side::Left, "Left spacing", style.spacing_left),
        (Side::Bottom, "Bottom spacing", style.spacing_bottom),
        (Side::Right, "Right spacing", style.spacing_right),
    ];
    for (side, label, stored) in sides {
        if let Some(value) = draft_number(ui, label, id, stored, 0.0..=MAX_LABEL_SPACING) {
            change = Some(LabelStyleChange::Spacing(side, value));
        }
    }
    let mut wrap = style.wrap;
    if ui.checkbox(&mut wrap, t("Wrap text")).changed() {
        change = Some(LabelStyleChange::Wrap(wrap));
    }
    let mut vertical = style.vertical;
    if ui.checkbox(&mut vertical, t("Vertical text")).changed() {
        change = Some(LabelStyleChange::Vertical(vertical));
    }
    if let Some(color) = color_option(ui, "Label background", style.background, DEFAULT_BACKGROUND)
    {
        change = Some(LabelStyleChange::Background(color));
    }
    ui.push_id("label_border", |ui| {
        if let Some(color) = color_option(ui, "Label border", style.border, DEFAULT_BORDER) {
            change = Some(LabelStyleChange::Border(color));
        }
    });
    change
}

impl ThinodaApp {
    /// Label text section for selected shapes; one change edits every
    /// selected shape in a single undo step. Returns whether it was shown.
    pub(crate) fn board_label_style_properties(&mut self, ui: &mut Ui) -> bool {
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
        let style = shapes[0]
            .label_style
            .as_deref()
            .cloned()
            .unwrap_or_default();
        let members: Vec<_> = ids.iter().map(|id| BoardMember::Object(*id)).collect();
        let locked = sheet
            .expand_board_selection(&members)
            .map_or(true, |members| {
                members
                    .iter()
                    .any(|member| sheet.is_board_member_locked(*member))
            });
        let enabled = !locked && !view.has_active_canvas_gesture();
        let fonts = &mut self.fonts;
        let change = ui
            .push_id(("board_label_style", ids[0]), |ui| {
                ui.separator();
                ui.add_enabled_ui(enabled, |ui| {
                    label_style_controls(ui, fonts, ids[0], &style)
                })
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
                    .update_canvas_objects("Label style", &ids, |object| change.apply_to(object))
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
