use super::*;

#[cfg(test)]
#[path = "preview_tests.rs"]
mod tests;

type PreviewKey = (u64, usize, bool);

#[derive(Default)]
pub(super) struct SlidePreview {
    pending: Option<(PreviewKey, mpsc::Receiver<Result<egui::ColorImage, String>>)>,
    rendered: Option<(PreviewKey, Result<egui::TextureHandle, String>)>,
}

impl SlidePreview {
    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        sheet: &Sheet,
        index: usize,
        generation: u64,
        options: &ExportOptions,
        available: egui::Vec2,
    ) {
        let key = (generation, index, options.dark);
        if let Some((requested, receiver)) = &self.pending {
            let result = match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err(t("Export worker stopped unexpectedly").into()))
                }

                Err(mpsc::TryRecvError::Empty) => None,
            };
            if let Some(result) = result {
                let requested = *requested;
                self.pending = None;
                if requested == key {
                    self.rendered = Some((
                        key,
                        result.map(|image| {
                            ui.ctx().load_texture(
                                format!("pitch-slide-{}-{}", generation, index),
                                image,
                                egui::TextureOptions::LINEAR,
                            )
                        }),
                    ));
                }
            }
        }
        if self.pending.is_none()
            && self
                .rendered
                .as_ref()
                .is_none_or(|(rendered, _)| *rendered != key)
        {
            let sheet = sheet.clone();
            let mut options = options.clone();
            if let Some(slide) = sheet.presentation.slides.get(index) {
                options.scale =
                    (1600.0 / slide.frame.size.x.max(slide.frame.size.y)).clamp(0.1, 8.0);
            }
            let (sender, receiver) = mpsc::channel();
            let repaint = ui.ctx().clone();
            let viewport = ui.ctx().viewport_id();
            self.pending = Some((key, receiver));
            if let Err(error) =
                thinoda_platform::system::run_background("thinoda-slide-preview", move || {
                    let result = thinoda_io::render_presentation_slide_png(&sheet, index, &options)
                        .map_err(|error| error.to_string())
                        .and_then(|bytes| {
                            image::load_from_memory(&bytes).map_err(|error| error.to_string())
                        })
                        .map(|image| {
                            let image = image.to_rgba8();
                            egui::ColorImage::from_rgba_unmultiplied(
                                [image.width() as usize, image.height() as usize],
                                image.as_raw(),
                            )
                        });
                    let _ = sender.send(result);
                    repaint.request_repaint_of(viewport);
                })
            {
                self.pending = None;
                self.rendered = Some((key, Err(error.to_string())));
            }
        }
        if self.pending.is_some() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        let mut retry = false;
        egui::Frame::new()
            .fill(Palette::of_ui(ui).surface_alt)
            .inner_margin(8.0)
            .show(ui, |ui| {
                let size = (available - egui::vec2(16.0, 16.0)).max(egui::vec2(1.0, 1.0));
                ui.set_max_width(size.x);
                if let Some((rendered, result)) = &self.rendered
                    && *rendered == key
                {
                    match result {
                        Ok(texture) => {
                            let dimensions = texture.size_vec2();
                            let scale = (size.x / dimensions.x).min(size.y / dimensions.y);
                            let (region, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                            let image_rect =
                                egui::Rect::from_center_size(region.center(), dimensions * scale);
                            ui.put(
                                image_rect,
                                egui::Image::new(texture)
                                    .fit_to_exact_size(dimensions * scale)
                                    .alt_text(t("Slide preview")),
                            );
                        }
                        Err(error) => {
                            ui.colored_label(
                                ui.visuals().error_fg_color,
                                format!("{}: {error}", t("Slide preview failed")),
                            );
                            retry = ui.button(t("Retry")).clicked()
                                && widgets::touch_activation_allowed(ui.ctx());
                        }
                    }
                } else {
                    ui.allocate_ui(size, |ui| {
                        ui.spinner();
                        ui.label(t("Rendering slide…"));
                    });
                    if retry {
                        self.rendered = None;
                        ui.ctx().request_repaint();
                    }
                }
            });
    }
}
