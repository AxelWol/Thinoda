use egui::{Ui, vec2};
use thinoda_core::{Layout, Rect, Sheet};
use thinoda_io::PresentationAspectRatio;
use thinoda_ui::Palette;

use crate::i18n::t;

#[derive(Clone, PartialEq)]
struct PreviewKey {
    document: String,
    sheet: usize,
    revision: u64,
    ratio: PresentationAspectRatio,
    dark: bool,
    pixels_per_point: f32,
}

struct VectorPreview {
    sheet: Sheet,
    layout: Layout,
    frame: Rect,
}

#[derive(Default)]
pub(crate) struct PowerPointPreview {
    rendered: Option<(PreviewKey, Result<VectorPreview, String>)>,
}

impl PowerPointPreview {
    pub(crate) fn show(
        &mut self,
        ui: &mut Ui,
        sheet: &Sheet,
        owner: &(String, usize, u64),
        ratio: PresentationAspectRatio,
        available: egui::Vec2,
    ) {
        let dark = Palette::of_ui(ui).dark;
        let key = PreviewKey {
            document: owner.0.clone(),
            sheet: owner.1,
            revision: owner.2,
            ratio,
            dark,
            pixels_per_point: ui.ctx().pixels_per_point(),
        };
        if self
            .rendered
            .as_ref()
            .is_none_or(|(rendered, _)| *rendered != key)
        {
            let result = (|| {
                thinoda_core::images::validate_visual(sheet)?;
                let slide = sheet
                    .presentation
                    .slides
                    .first()
                    .ok_or_else(|| t("No slides yet. Generate branch and Zone slides, or add the selected branch.").to_owned())?;
                let layout = thinoda_core::layout_sheet(
                    sheet,
                    &thinoda_canvas::EguiMeasure::new(ui.ctx()),
                    dark,
                );
                let resolved = thinoda_core::resolve_presentation_slide(sheet, slide, &layout)?;
                let frame = thinoda_io::presentation::fit_presentation_frame(resolved.frame, ratio)
                    .map_err(|error| error.to_string())?;
                let mut sheet = sheet.clone();
                sheet.zones = resolved.zones;
                Ok(VectorPreview {
                    sheet,
                    layout: resolved.layout,
                    frame,
                })
            })();
            self.rendered = Some((key, result));
        }

        let mut retry = false;
        egui::Frame::new()
            .fill(Palette::of_ui(ui).surface_alt)
            .inner_margin(8.0)
            .show(ui, |ui| {
                let size = (available - vec2(16.0, 16.0)).max(vec2(1.0, 1.0));
                if let Some((_, result)) = &self.rendered {
                    match result {
                        Ok(scene) => {
                            let (region, response) =
                                ui.allocate_exact_size(size, egui::Sense::hover());
                            response.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::Role::Image,
                                    true,
                                    t("Slide preview"),
                                )
                            });
                            let zoom = (region.width() / scene.frame.size.x)
                                .min(region.height() / scene.frame.size.y);
                            let center = scene.frame.center();
                            let xf = thinoda_canvas::Xf {
                                origin: region.center() - vec2(center.x, center.y) * zoom,
                                zoom,
                            };
                            let crop = xf.rect(scene.frame).intersect(region);
                            let painter = ui.painter_at(crop);
                            let background =
                                thinoda_core::theme::Theme::for_settings(&scene.sheet.theme)
                                    .background(&scene.sheet.theme, dark);
                            painter.rect_filled(crop, 0.0, thinoda_canvas::color32(background));
                            thinoda_canvas::paint_map(
                                &painter,
                                &scene.sheet,
                                &scene.layout,
                                xf,
                                &thinoda_canvas::PaintOptions {
                                    dark,
                                    ..Default::default()
                                },
                            );
                        }
                        Err(error) => {
                            ui.colored_label(
                                ui.visuals().error_fg_color,
                                format!("{}: {error}", t("Slide preview failed")),
                            );
                            retry = ui.button(t("Retry")).clicked()
                                && thinoda_ui::widgets::touch_activation_allowed(ui.ctx());
                        }
                    }
                }
            });
        if retry {
            self.rendered = None;
            ui.ctx().request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_is_vector_and_rekeys_for_ratio_theme_and_revision() {
        let mut sheet = Sheet::new("Preview", "Center");
        sheet.insert_child(sheet.root, "Editable topic").unwrap();
        sheet.presentation =
            thinoda_io::generate_presentation(&sheet, &Default::default()).unwrap();
        let original = sheet.clone();
        let mut h = egui_kittest::Harness::builder().build_ui_state(
            move |ui, state: &mut (PowerPointPreview, PresentationAspectRatio, u64)| {
                state.0.show(
                    ui,
                    &sheet,
                    &("source".into(), 0, state.2),
                    state.1,
                    vec2(400.0, 250.0),
                );
            },
            (
                PowerPointPreview::default(),
                PresentationAspectRatio::Widescreen16x9,
                1,
            ),
        );
        h.run_steps(2);
        let scene = h.state().0.rendered.as_ref().unwrap().1.as_ref().unwrap();
        assert!(!scene.layout.topics.is_empty());
        assert!((scene.frame.size.x / scene.frame.size.y - 16.0 / 9.0).abs() < 0.001);
        assert_eq!(scene.sheet.presentation, original.presentation);
        h.state_mut().1 = PresentationAspectRatio::Standard4x3;
        h.state_mut().2 = 2;
        h.ctx.set_visuals(egui::Visuals::dark());
        h.run_steps(2);
        let (key, scene) = h.state().0.rendered.as_ref().unwrap();
        assert!(key.dark);
        assert_eq!(key.revision, 2);
        let scene = scene.as_ref().unwrap();
        assert!((scene.frame.size.x / scene.frame.size.y - 4.0 / 3.0).abs() < 0.001);
    }

    #[test]
    fn preview_reports_an_empty_deck_without_a_fallback_map() {
        let sheet = Sheet::new("Empty", "Must not appear");
        let mut h = egui_kittest::Harness::builder().build_ui_state(
            move |ui, preview: &mut PowerPointPreview| {
                preview.show(
                    ui,
                    &sheet,
                    &("source".into(), 0, 1),
                    PresentationAspectRatio::default(),
                    vec2(400.0, 250.0),
                );
            },
            PowerPointPreview::default(),
        );
        h.run_steps(2);
        assert!(h.state().rendered.as_ref().unwrap().1.is_err());
    }
}
