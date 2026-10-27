use std::sync::mpsc;

use egui::{ColorImage, TextureHandle, TextureOptions, Ui, vec2};
use thinoda_core::Sheet;
use thinoda_io::{ExportOptions, PresentationAspectRatio};
use thinoda_ui::Palette;

use crate::i18n::t;

#[derive(Clone, PartialEq, Eq)]
struct PreviewKey {
    document: String,
    sheet: usize,
    revision: u64,
    ratio: PresentationAspectRatio,
}

#[derive(Default)]
pub(crate) struct PowerPointPreview {
    pending: Option<(PreviewKey, mpsc::Receiver<Result<ColorImage, String>>)>,
    rendered: Option<(PreviewKey, Result<TextureHandle, String>)>,
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
        let key = PreviewKey {
            document: owner.0.clone(),
            sheet: owner.1,
            revision: owner.2,
            ratio,
        };
        if let Some((requested, receiver)) = &self.pending {
            let result = match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err(t("Export worker stopped unexpectedly").into()))
                }
                Err(mpsc::TryRecvError::Empty) => None,
            };
            if let Some(result) = result {
                let requested = requested.clone();
                self.pending = None;
                if requested == key {
                    self.rendered = Some((
                        key.clone(),
                        result.map(|image| {
                            ui.ctx().load_texture(
                                format!("powerpoint-preview-{}-{:?}", key.revision, key.ratio),
                                image,
                                TextureOptions::LINEAR,
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
            let options = ExportOptions {
                presentation_aspect_ratio: ratio,
                scale: 0.5,
                ..Default::default()
            };
            let (sender, receiver) = mpsc::channel();
            let repaint = ui.ctx().clone();
            let viewport = ui.ctx().viewport_id();
            self.pending = Some((key.clone(), receiver));
            if let Err(error) =
                thinoda_platform::system::run_background("thinoda-powerpoint-preview", move || {
                    let result = thinoda_io::presentation::render_presentation_pptx_slide_png(
                        &sheet, 0, &options,
                    )
                    .map_err(|error| error.to_string())
                    .and_then(|bytes| {
                        image::load_from_memory(&bytes).map_err(|error| error.to_string())
                    })
                    .map(|image| {
                        let image = image.to_rgba8();
                        ColorImage::from_rgba_unmultiplied(
                            [image.width() as usize, image.height() as usize],
                            image.as_raw(),
                        )
                    });
                    let _ = sender.send(result);
                    repaint.request_repaint_of(viewport);
                })
            {
                self.pending = None;
                self.rendered = Some((key.clone(), Err(error.to_string())));
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
                let size = (available - vec2(16.0, 16.0)).max(vec2(1.0, 1.0));
                if let Some((rendered, result)) = &self.rendered
                    && *rendered == key
                {
                    match result {
                        Ok(texture) => {
                            let dimensions = texture.size_vec2();
                            let scale = (size.x / dimensions.x).min(size.y / dimensions.y);
                            let image_size = dimensions * scale;
                            let (region, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                            let image_rect =
                                egui::Rect::from_center_size(region.center(), image_size);
                            ui.put(
                                image_rect,
                                egui::Image::new(texture)
                                    .fit_to_exact_size(image_size)
                                    .alt_text(t("Slide preview")),
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
                } else {
                    ui.allocate_ui(size, |ui| {
                        ui.spinner();
                        ui.label(t("Rendering slide…"));
                    });
                }
            });
        if retry {
            self.rendered = None;
            ui.ctx().request_repaint();
        }
    }
}
