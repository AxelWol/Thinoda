//! Original structure samples rendered with the same layout and painter as the canvas.

use std::sync::OnceLock;

use egui::{Color32, Response, Sense, Ui, vec2};
use thinoda_canvas::paint_thumbnail;
use thinoda_core::{
    Layout, Sheet, Structure,
    layout::{ApproxMeasure, layout_sheet},
};

use crate::{i18n::t, ui::structure_label};

const FAMILIES: &[(&str, &[Structure])] = &[
    (
        "Mind maps",
        &[
            Structure::MapClockwise,
            Structure::MapAnticlockwise,
            Structure::MapUnbalanced,
        ],
    ),
    (
        "Logic charts",
        &[Structure::LogicRight, Structure::LogicLeft],
    ),
    ("Brace maps", &[Structure::BraceRight, Structure::BraceLeft]),
    ("Org charts", &[Structure::OrgDown, Structure::OrgUp]),
    ("Tree charts", &[Structure::TreeRight, Structure::TreeLeft]),
    (
        "Timelines",
        &[Structure::TimelineHorizontal, Structure::TimelineVertical],
    ),
    (
        "Fishbones",
        &[Structure::FishboneRight, Structure::FishboneLeft],
    ),
    ("Tree tables", &[Structure::TreeTable]),
    ("Matrices", &[Structure::Matrix]),
];

fn samples(dark: bool) -> &'static [(Sheet, Layout)] {
    static LIGHT: OnceLock<Vec<(Sheet, Layout)>> = OnceLock::new();
    static DARK: OnceLock<Vec<(Sheet, Layout)>> = OnceLock::new();
    let cache = if dark { &DARK } else { &LIGHT };
    cache.get_or_init(|| {
        Structure::ALL
            .into_iter()
            .map(|structure| {
                let mut sheet = Sheet::new("Sample", "Idea");
                sheet.structure = structure;
                for title in ["Plan", "Build", "Review", "Share"] {
                    let branch = sheet.add_child(sheet.root, None, title);
                    sheet.add_child(branch, None, "One");
                    sheet.add_child(branch, None, "Two");
                }
                let layout = layout_sheet(&sheet, &ApproxMeasure, dark);
                (sheet, layout)
            })
            .collect()
    })
}

fn paint_preview(ui: &Ui, rect: egui::Rect, structure: Structure) {
    let dark = ui.visuals().dark_mode;
    let (sheet, layout) = samples(dark)
        .iter()
        .find(|(sheet, _)| sheet.structure == structure)
        .unwrap();
    paint_thumbnail(ui.painter(), sheet, layout, rect, dark);
}

pub(crate) fn show(ui: &mut Ui, current: Structure) -> (Response, Option<Structure>) {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, 55.0), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Button,
            ui.is_enabled(),
            format!("{}: {}", t("Choose Structure"), t(structure_label(current))),
        )
    });
    let visuals = ui.style().interact(&response);
    ui.painter().rect(
        rect,
        6,
        visuals.bg_fill,
        visuals.bg_stroke,
        egui::StrokeKind::Inside,
    );
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            6,
            ui.visuals().selection.stroke,
            egui::StrokeKind::Inside,
        );
    }
    paint_preview(
        ui,
        egui::Rect::from_min_size(rect.min, vec2(70.0, rect.height())).shrink(2.0),
        current,
    );
    let label = ui.painter().layout(
        format!("{}\n{}", t("Choose Structure"), t(structure_label(current))),
        egui::FontId::proportional(12.0),
        visuals.text_color(),
        (width - 80.0).max(1.0),
    );
    ui.painter()
        .galley(rect.min + vec2(76.0, 7.0), label, Color32::PLACEHOLDER);
    let mut chosen = None;
    let viewport = ui.ctx().content_rect();
    let popup_width = (viewport.width() - 32.0).clamp(80.0, 390.0);
    let popup_frame = egui::Frame::popup(ui.style());
    let content_width = popup_width - popup_frame.total_margin().sum().x;
    let mut popup = egui::Popup::menu(&response)
        .width(popup_width)
        .frame(popup_frame)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    if !thinoda_ui::widgets::touch_activation_allowed(ui.ctx()) {
        popup = popup.open_memory(None);
    }
    let was_open = popup.is_open();
    let popup_id = popup.get_id();
    let escape = was_open && ui.input(|input| input.key_pressed(egui::Key::Escape));
    popup.show(|ui| {
        // Popup::width is only an initial size. Do not feed rounded grid widths
        // back into the next frame's right-aligned popup position.
        ui.set_width(content_width);
        egui::ScrollArea::vertical()
            .id_salt("structure_gallery_scroll")
            .auto_shrink([false, true])
            .max_height((viewport.height() * 0.65).min(420.0))
            .show(ui, |ui| {
                for (family, structures) in FAMILIES {
                    egui::CollapsingHeader::new(t(family))
                        .default_open(true)
                        .show(ui, |ui| {
                            let width = ui.available_width();
                            let columns = ((width + 8.0) / 120.0).floor().clamp(1.0, 3.0) as usize;
                            let card_width = (width - 8.0 * (columns - 1) as f32) / columns as f32;
                            egui::Grid::new(family)
                                .num_columns(columns)
                                .spacing(vec2(8.0, 8.0))
                                .show(ui, |ui| {
                                    for (index, &structure) in structures.iter().enumerate() {
                                        let (_, activated) = thinoda_ui::widgets::preview_card(
                                            ui,
                                            t(structure_label(structure)),
                                            structure == current,
                                            card_width,
                                            72.0,
                                            |ui, rect| paint_preview(ui, rect, structure),
                                        );
                                        if activated {
                                            chosen = Some(structure);
                                            ui.close_kind(egui::UiKind::Menu);
                                        }
                                        if (index + 1) % columns == 0 {
                                            ui.end_row();
                                        }
                                    }
                                });
                        });
                }
            });
    });
    if escape {
        egui::Popup::close_id(ui.ctx(), popup_id);
        // The popup can close before the canvas draws in the same frame.
        ui.input_mut(|input| {
            input.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
        });
    }
    (response, chosen)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn popup_width_and_cards_remain_stable_at_fractional_positions() {
        crate::i18n::set_lang(crate::i18n::Lang::En);
        for scale in [1.0, 1.25, 1.5, 1.75, 2.0] {
            for offset in [0.0, 0.25, 0.5, 0.75] {
                let mut h = Harness::builder()
                    .with_size(vec2(800.0 + offset, 800.0))
                    .with_pixels_per_point(scale)
                    .build_ui_state(
                        |ui, size: &mut Option<egui::Vec2>| {
                            ui.scope_builder(
                                egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
                                    egui::pos2(480.0 + offset, 100.0),
                                    vec2(300.0, 600.0),
                                )),
                                |ui| {
                                    let (response, _) = show(ui, Structure::MapClockwise);
                                    let popup = egui::Popup::menu(&response);
                                    *size = popup.get_expected_size();
                                },
                            );
                        },
                        None,
                    );
                h.get_by_role_and_label(egui::Role::Button, "Choose Structure: Mind Map")
                    .click();
                h.run_steps(20);
                let size = h.state().unwrap();
                assert!(
                    (size.x - 390.0).abs() <= 1.0 / scale,
                    "popup width drifted at {scale}/{offset}: {size:?}"
                );
                let labels = ["Mind Map", "Mind Map (one side)", "Logic Chart (right)"];
                let rects =
                    labels.map(|label| h.get_by_role_and_label(egui::Role::Button, label).rect());
                for frame in 0..30 {
                    h.run_steps(1);
                    assert_eq!(*h.state(), Some(size), "{scale}/{offset}/{frame}");
                    for (label, rect) in labels.into_iter().zip(rects) {
                        assert_eq!(
                            h.get_by_role_and_label(egui::Role::Button, label).rect(),
                            rect,
                            "{label} moved at {scale}/{offset}/{frame}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn families_cover_all_structures_once_and_previews_are_valid() {
        let structures: Vec<_> = FAMILIES
            .iter()
            .flat_map(|(_, structures)| structures.iter().copied())
            .collect();
        assert_eq!(structures.len(), Structure::ALL.len());
        for structure in Structure::ALL {
            assert_eq!(
                structures
                    .iter()
                    .filter(|&&value| value == structure)
                    .count(),
                1
            );
        }
        for dark in [false, true] {
            for (sheet, layout) in samples(dark) {
                sheet.validate().unwrap();
                assert_eq!(layout.topics.len(), sheet.topics.len());
                assert!(layout.bounds.size.x.is_finite() && layout.bounds.size.x > 0.0);
                assert!(layout.bounds.size.y.is_finite() && layout.bounds.size.y > 0.0);
                for topic in layout.topics.values() {
                    assert!(topic.rect.min.x.is_finite() && topic.rect.min.y.is_finite());
                    assert!(topic.rect.size.x > 0.0 && topic.rect.size.y > 0.0);
                }
            }
        }
    }
}
