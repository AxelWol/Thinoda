//! Built-in template previews, generated from the actual documents that Create opens.

use std::sync::OnceLock;

use egui::Ui;
use thinoda_core::{
    Layout, Sheet,
    layout::{ApproxMeasure, layout_sheet},
    templates::{self, TemplateCategory, TemplateInfo},
};

use crate::i18n::t;

fn samples(dark: bool) -> &'static [(Sheet, Layout)] {
    static LIGHT: OnceLock<Vec<(Sheet, Layout)>> = OnceLock::new();
    static DARK: OnceLock<Vec<(Sheet, Layout)>> = OnceLock::new();
    let cache = if dark { &DARK } else { &LIGHT };
    cache.get_or_init(|| {
        templates::TEMPLATES
            .iter()
            .map(|info| {
                let sheet = templates::instantiate(info.id).sheets.remove(0);
                let layout = layout_sheet(&sheet, &ApproxMeasure, dark);
                (sheet, layout)
            })
            .collect()
    })
}

fn cards(ui: &mut Ui, entries: &[&TemplateInfo], choice: &mut Option<&'static str>, recent: bool) {
    let width = ui.available_width();
    let columns = ((width + 8.0) / 172.0).floor().max(1.0) as usize;
    let card_width = (width - 8.0 * (columns - 1) as f32) / columns as f32;
    egui::Grid::new(("template_grid", recent))
        .num_columns(columns)
        .spacing(egui::vec2(8.0, 8.0))
        .show(ui, |ui| {
            for (index, template) in entries.iter().enumerate() {
                ui.push_id(template.id, |ui| {
                    let sample_index = templates::TEMPLATES
                        .iter()
                        .position(|info| info.id == template.id)
                        .unwrap();
                    let dark = ui.visuals().dark_mode;
                    let (sheet, layout) = &samples(dark)[sample_index];
                    let (response, activated) = thinoda_ui::widgets::preview_card(
                        ui,
                        t(template.name),
                        *choice == Some(template.id),
                        card_width,
                        94.0,
                        |ui, rect| {
                            thinoda_canvas::paint_thumbnail(ui.painter(), sheet, layout, rect, dark)
                        },
                    );
                    if recent {
                        response.accessible_name(format!(
                            "{}: {}",
                            t("Recent template"),
                            t(template.name)
                        ));
                    }
                    if activated {
                        *choice = Some(template.id);
                    }
                });
                if (index + 1) % columns == 0 {
                    ui.end_row();
                }
            }
        });
}

pub(crate) fn show(
    ui: &mut Ui,
    category: Option<TemplateCategory>,
    choice: &mut Option<&'static str>,
    recent: &[String],
) -> Option<TemplateCategory> {
    let mut navigate = None;
    let recent: Vec<_> = recent
        .iter()
        .filter_map(|id| templates::TEMPLATES.iter().find(|info| info.id == id))
        .filter(|info| category.is_none_or(|category| category == info.category))
        .take(8)
        .collect();
    if !recent.is_empty() {
        ui.heading(t("Recently used templates"));
        ui.push_id("recent_templates", |ui| cards(ui, &recent, choice, true));
        ui.add_space(16.0);
    }
    for group in TemplateCategory::ALL {
        if category.is_some_and(|category| category != group) {
            continue;
        }
        let entries: Vec<_> = templates::TEMPLATES
            .iter()
            .filter(|info| info.category == group)
            .collect();
        ui.horizontal(|ui| {
            ui.heading(t(group.label()));
            if category.is_none() {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = format!("{}: {}", t("Browse templates"), t(group.label()));
                    if ui
                        .small_button(thinoda_ui::icons::CHEVRON_RIGHT)
                        .on_hover_text(&label)
                        .accessible_name(label)
                        .clicked()
                    {
                        navigate = Some(group);
                    }
                    ui.label(entries.len().to_string());
                });
            }
        });
        ui.push_id(group.label(), |ui| cards(ui, &entries, choice, false));
        ui.add_space(16.0);
    }
    navigate
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_match_every_instantiated_template_and_are_cached_per_appearance() {
        for dark in [false, true] {
            assert_eq!(samples(dark).len(), templates::TEMPLATES.len());
            for (info, (sheet, layout)) in templates::TEMPLATES.iter().zip(samples(dark)) {
                assert_eq!(sheet, templates::instantiate(info.id).active());
                assert_eq!(layout.topics.len(), sheet.topics.len());
                assert!(layout.bounds.size.x.is_finite() && layout.bounds.size.x > 0.0);
                assert!(layout.bounds.size.y.is_finite() && layout.bounds.size.y > 0.0);
            }
        }
        assert!(std::ptr::eq(samples(false), samples(false)));
        assert!(!std::ptr::eq(samples(false), samples(true)));
    }
}
