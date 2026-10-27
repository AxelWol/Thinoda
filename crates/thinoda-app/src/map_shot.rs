//! Native GPU readback of the visible canvas, followed by a local, non-destructive crop.

use std::{io::Cursor, path::Path, sync::mpsc, time::Duration};
use web_time::Instant;

use egui::{
    Color32, ColorImage, Context, Key, Modifiers, Pos2, Rect, Sense, Stroke, TextureHandle,
};

use crate::{app::ThinodaApp, i18n::t};

const MAX_PIXELS: usize = 32 * 1024 * 1024;
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Default)]
pub(crate) struct MapShot {
    pub requested: bool,
    pub capturing: bool,
    pending: Option<(Instant, mpsc::Receiver<Result<ColorImage, String>>)>,
    editor: Option<ShotEditor>,
}

impl MapShot {
    pub fn active(&self) -> bool {
        self.requested || self.capturing || self.pending.is_some() || self.editor.is_some()
    }

    fn cancel(&mut self) {
        *self = Self::default();
    }
}

struct ShotEditor {
    image: ColorImage,
    texture: TextureHandle,
    crop: PixelCrop,
    square: bool,
    frame: u32,
    frame_color: Color32,
    drag_start: Option<Pos2>,
    gesture: Option<ShotGesture>,
}

struct ShotGesture {
    crop: PixelCrop,
    square: bool,
    frame: u32,
    frame_color: Color32,
    last_frame: u64,
    canceled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PixelCrop {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

impl PixelCrop {
    fn full(image: &ColorImage) -> Self {
        Self {
            x: 0,
            y: 0,
            width: image.width(),
            height: image.height(),
        }
    }

    fn constrain(&mut self, size: [usize; 2], square: bool) {
        self.x = self.x.min(size[0] - 1);
        self.y = self.y.min(size[1] - 1);
        self.width = self.width.clamp(1, size[0] - self.x);
        self.height = self.height.clamp(1, size[1] - self.y);
        if square {
            let side = self.width.min(self.height);
            self.width = side;
            self.height = side;
        }
    }
}

fn validate_image(image: &ColorImage) -> Result<(), String> {
    let [width, height] = image.size;
    if width == 0
        || height == 0
        || width
            .checked_mul(height)
            .is_none_or(|n| n > MAX_PIXELS || n != image.pixels.len())
    {
        return Err(t("Map Shot image exceeds limits or has invalid dimensions").into());
    }
    Ok(())
}

fn extract(
    image: &ColorImage,
    crop: PixelCrop,
    frame: u32,
    color: Color32,
) -> Result<ColorImage, String> {
    validate_image(image)?;
    if crop.width == 0
        || crop.height == 0
        || crop
            .x
            .checked_add(crop.width)
            .is_none_or(|right| right > image.width())
        || crop
            .y
            .checked_add(crop.height)
            .is_none_or(|bottom| bottom > image.height())
        || frame > 32
    {
        return Err(t("Invalid Map Shot region").into());
    }
    let border = frame as usize;
    let width = crop.width + 2 * border;
    let height = crop.height + 2 * border;
    if width.checked_mul(height).is_none_or(|n| n > MAX_PIXELS) {
        return Err(t("Map Shot image exceeds limits or has invalid dimensions").into());
    }
    let mut out = ColorImage::filled([width, height], color);
    for row in 0..crop.height {
        let start = (crop.y + row) * image.width() + crop.x;
        let dest = (border + row) * width + border;
        out.pixels[dest..dest + crop.width]
            .copy_from_slice(&image.pixels[start..start + crop.width]);
    }
    Ok(out)
}

/// The readback is the whole client viewport. Only pixels fully inside the canvas are retained.
fn canvas_pixels(image: &ColorImage, canvas: Rect, viewport: Rect) -> Result<ColorImage, String> {
    validate_image(image)?;
    if !canvas.is_finite()
        || !viewport.is_finite()
        || !canvas.is_positive()
        || !viewport.is_positive()
    {
        return Err(t("Invalid Map Shot region").into());
    }
    let rect = canvas.intersect(viewport);
    if !rect.is_positive() {
        return Err(t("Invalid Map Shot region").into());
    }
    let sx = image.width() as f32 / viewport.width();
    let sy = image.height() as f32 / viewport.height();
    let x = ((rect.min.x - viewport.min.x) * sx).ceil() as usize;
    let y = ((rect.min.y - viewport.min.y) * sy).ceil() as usize;
    let right = (((rect.max.x - viewport.min.x) * sx).floor() as usize).min(image.width());
    let bottom = (((rect.max.y - viewport.min.y) * sy).floor() as usize).min(image.height());
    extract(
        image,
        PixelCrop {
            x,
            y,
            width: right.saturating_sub(x),
            height: bottom.saturating_sub(y),
        },
        0,
        Color32::TRANSPARENT,
    )
}

fn png_bytes(image: &ColorImage) -> Result<Vec<u8>, String> {
    validate_image(image)?;
    let rgba: Vec<u8> = image
        .pixels
        .iter()
        .flat_map(|c| c.to_srgba_unmultiplied())
        .collect();
    let image = image::RgbaImage::from_raw(image.width() as u32, image.height() as u32, rgba)
        .ok_or_else(|| t("Invalid Map Shot region").to_string())?;
    let mut out = Cursor::new(Vec::new());
    image
        .write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

fn save_png(path: &Path, image: &ColorImage) -> Result<(), String> {
    if !path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
    {
        return Err(t("Map Shot must be saved with a .png extension").into());
    }
    let bytes = png_bytes(image)?;
    thinoda_platform::fs::atomic_write(path, &bytes, false).map_err(|e| e.to_string())
}

impl ShotEditor {
    fn new(ctx: &Context, image: ColorImage) -> Self {
        let texture = ctx.load_texture("map-shot", image.clone(), egui::TextureOptions::LINEAR);
        Self {
            crop: PixelCrop::full(&image),
            image,
            texture,
            square: false,
            frame: 0,
            frame_color: Color32::WHITE,
            drag_start: None,
            gesture: None,
        }
    }

    fn output(&self) -> Result<ColorImage, String> {
        extract(&self.image, self.crop, self.frame, self.frame_color)
    }

    fn preview(&mut self, ui: &mut egui::Ui) {
        let available = egui::vec2(
            ui.available_width().max(1.0),
            330.0_f32.min(ui.ctx().content_rect().height() * 0.4),
        );
        let scale = (available.x / self.image.width() as f32)
            .min(available.y / self.image.height() as f32)
            .min(1.0);
        let size = egui::vec2(self.image.width() as f32, self.image.height() as f32) * scale;
        let (rect, response) = ui.allocate_exact_size(size, Sense::drag());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Image, true, t("Map Shot preview"))
        });
        ui.painter().image(
            self.texture.id(),
            rect,
            Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        if response.drag_started() {
            self.drag_start = ui
                .input(|i| i.pointer.press_origin())
                .map(|p| p.clamp(rect.min, rect.max));
        }
        if response.dragged()
            && let (Some(start), Some(end)) = (self.drag_start, response.interact_pointer_pos())
        {
            let region = Rect::from_two_pos(start, end.clamp(rect.min, rect.max));
            self.crop = PixelCrop {
                x: ((region.min.x - rect.min.x) / scale).floor() as usize,
                y: ((region.min.y - rect.min.y) / scale).floor() as usize,
                width: (region.width() / scale).ceil().max(1.0) as usize,
                height: (region.height() / scale).ceil().max(1.0) as usize,
            };
            self.crop.constrain(self.image.size, self.square);
        }
        if response.drag_stopped() {
            self.drag_start = None;
        }
        let crop = Rect::from_min_size(
            rect.min + egui::vec2(self.crop.x as f32, self.crop.y as f32) * scale,
            egui::vec2(self.crop.width as f32, self.crop.height as f32) * scale,
        );
        for dim in [
            Rect::from_min_max(rect.min, egui::pos2(rect.max.x, crop.min.y)),
            Rect::from_min_max(egui::pos2(rect.min.x, crop.max.y), rect.max),
            Rect::from_min_max(
                egui::pos2(rect.min.x, crop.min.y),
                egui::pos2(crop.min.x, crop.max.y),
            ),
            Rect::from_min_max(
                egui::pos2(crop.max.x, crop.min.y),
                egui::pos2(rect.max.x, crop.max.y),
            ),
        ] {
            ui.painter()
                .rect_filled(dim, 0, Color32::from_black_alpha(130));
        }
        ui.painter().rect_stroke(
            crop,
            0,
            Stroke::new(2.0, ui.visuals().selection.bg_fill),
            egui::StrokeKind::Inside,
        );
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        let mut square_side = None;
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.square, t("Square (1:1)"));
            if ui.button(t("Reset region")).clicked() {
                self.crop = PixelCrop::full(&self.image);
            }
        });
        egui::Grid::new("map-shot-region")
            .num_columns(2)
            .show(ui, |ui| {
                for (key, value, range) in [
                    ("Region X", &mut self.crop.x, 0..=self.image.width() - 1),
                    ("Region Y", &mut self.crop.y, 0..=self.image.height() - 1),
                    ("Region width", &mut self.crop.width, 1..=self.image.width()),
                    (
                        "Region height",
                        &mut self.crop.height,
                        1..=self.image.height(),
                    ),
                ] {
                    let label = ui.label(t(key));
                    let response = ui
                        .add(egui::DragValue::new(&mut *value).range(range))
                        .labelled_by(label.id);
                    if response.changed() && matches!(key, "Region width" | "Region height") {
                        square_side = Some(*value);
                    }
                    ui.end_row();
                }
                let label = ui.label(t("Frame thickness"));
                ui.add(egui::DragValue::new(&mut self.frame).range(0..=32))
                    .labelled_by(label.id);
                ui.end_row();
                let label = ui.label(t("Frame color"));
                ui.color_edit_button_srgba(&mut self.frame_color)
                    .labelled_by(label.id);
                ui.end_row();
            });
        if self.square
            && let Some(side) = square_side
        {
            self.crop.width = side;
            self.crop.height = side;
        }
        self.crop.constrain(self.image.size, self.square);
        ui.label(format!(
            "{} × {} px",
            self.crop.width + 2 * self.frame as usize,
            self.crop.height + 2 * self.frame as usize
        ));
    }
}

impl ThinodaApp {
    pub(crate) fn poll_map_shot(&mut self, ctx: &Context) {
        let result = self
            .map_shot
            .pending
            .as_ref()
            .and_then(|(_, rx)| match rx.try_recv() {
                Ok(image) => Some(image),
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err(t("Map Shot capture failed").into()))
                }
                Err(mpsc::TryRecvError::Empty) => None,
            });
        if let Some(result) = result {
            self.map_shot.pending = None;
            match result {
                Ok(image) => self.map_shot.editor = Some(ShotEditor::new(ctx, image)),
                Err(e) => {
                    self.toasts.error(e);
                }
            }
        } else if self
            .map_shot
            .pending
            .as_ref()
            .is_some_and(|(start, _)| start.elapsed() >= CAPTURE_TIMEOUT)
        {
            self.map_shot.cancel();
            self.toasts.error(t(
                "Map Shot capture timed out; this renderer may not support screenshots",
            ));
        }
        // Defer readback until a fresh frame, so menu/palette pixels cannot leak into the canvas.
        if self.map_shot.requested {
            self.map_shot.requested = false;
            self.map_shot.capturing = true;
        }
        if self.map_shot.active() && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
        {
            self.map_shot.cancel();
        }
    }

    pub(crate) fn capture_map_shot(&mut self, ctx: &Context, canvas: Rect) {
        if !self.map_shot.capturing {
            return;
        }
        self.map_shot.capturing = false;
        let (tx, rx) = mpsc::sync_channel(1);
        self.map_shot.pending = Some((Instant::now(), rx));
        let viewport = ctx.input(|i| i.viewport_rect());
        let repaint = ctx.clone();
        ctx.request_screenshot(move |image| {
            let result = canvas_pixels(&image, canvas, viewport);
            let _ = tx.send(result); // Cancellation drops the receiver; late results must be discarded.
            repaint.request_repaint();
        });
        ctx.request_repaint_after(Duration::from_millis(100));
    }

    pub(crate) fn map_shot_dialog(&mut self, ctx: &Context) {
        let touch_allowed = thinoda_ui::widgets::touch_activation_allowed(ctx);
        if self.map_shot.pending.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
            let labels = crate::i18n::dialog_labels();
            let response = thinoda_ui::Dialog::new(
                egui::Id::unique("map-shot-wait"),
                t("Capturing visible map…"),
            )
            .icon(thinoda_ui::icons::CAMERA)
            .labels(&labels)
            .modal(true)
            .close_on_backdrop(true)
            .default_width(340.0)
            .default_height(120.0)
            .min_size(egui::vec2(260.0, 96.0))
            .resizable(false)
            .show(ctx, |ui| {
                if ui.button(t("Cancel")).clicked() && touch_allowed {
                    self.map_shot.cancel();
                }
            });
            if response.close_requested && touch_allowed {
                self.map_shot.cancel();
            }
            return;
        }
        let Some(editor) = self.map_shot.editor.as_mut() else {
            return;
        };
        let pointer_down = ctx.input(|input| input.pointer.primary_down());
        let focused = ctx.input(|input| {
            input.focused
                && !input
                    .events
                    .iter()
                    .any(|event| matches!(event, egui::Event::WindowFocused(false)))
        });
        let frame = ctx.cumulative_frame_nr();
        if pointer_down && editor.gesture.is_none() {
            editor.gesture = Some(ShotGesture {
                crop: editor.crop,
                square: editor.square,
                frame: editor.frame,
                frame_color: editor.frame_color,
                last_frame: frame,
                canceled: false,
            });
        }
        if let Some(gesture) = &mut editor.gesture {
            gesture.canceled |=
                !touch_allowed || !focused || frame > gesture.last_frame.saturating_add(1);
            gesture.last_frame = frame;
        }
        let activation_allowed = touch_allowed
            && focused
            && editor
                .gesture
                .as_ref()
                .is_none_or(|gesture| !gesture.canceled);
        let before_controls = (editor.crop, editor.square, editor.frame, editor.frame_color);
        let mut close = false;
        let labels = crate::i18n::dialog_labels();
        let response = thinoda_ui::Dialog::new(egui::Id::unique("map-shot-editor"), t("Map Shot"))
            .icon(thinoda_ui::icons::CAMERA)
            .labels(&labels)
            .modal(true)
            .close_on_backdrop(true)
            .default_width(660.0)
            .default_height(700.0)
            .min_size(egui::vec2(360.0, 300.0))
            .scroll(false)
            .show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height((ctx.content_rect().height()*0.7).max(100.0)).show(ui, |ui| {
                ui.label(t("Visible map only. Drag a region or edit pixel coordinates. Use Export for the complete sheet."));
                editor.preview(ui);
                editor.controls(ui);
            });
            if !activation_allowed {
                let original = editor.gesture.as_ref().map_or(before_controls, |gesture| {
                    (gesture.crop, gesture.square, gesture.frame, gesture.frame_color)
                });
                (editor.crop, editor.square, editor.frame, editor.frame_color) = original;
                editor.drag_start = None;
            }
            ui.horizontal_wrapped(|ui| {
                if ui.button(t("Cancel")).clicked() && activation_allowed { close = true; }
                if ui.button(t("Copy image")).clicked() && activation_allowed {
                    match editor.output() {
                        Ok(image) => {
                            ctx.copy_image(image);
                            self.toasts.info(t("Image sent to system clipboard"));
                        }
                        Err(e) => { self.toasts.error(e); }
                    }
                }
                if ui.button(t("Save PNG…")).clicked() && activation_allowed
                    && let Some(path) = crate::files::save_target(Some(t("Save PNG…")),
                        &[("PNG", &["png"])], "map-shot.png", None, crate::files::Target::Download)
                {
                    match editor.output().and_then(|image| save_png(&path, &image)).and_then(|()| crate::files::deliver(&path)) {
                        Ok(()) if crate::files::is_web() => self.toasts.success(t("Download started")),
                        Ok(()) => self.toasts.success(format!("{} {}", t("Exported to"), path.display())),
                        Err(e) => self.toasts.error(format!("{}: {e}", t("Export failed"))),
                    };
                }
            });
            if !pointer_down {
                editor.gesture = None;
            }
        });
        if close || (response.close_requested && activation_allowed) {
            self.map_shot.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Cmd;
    use egui_kittest::Harness;
    #[cfg(not(feature = "gpu-tests"))]
    use egui_kittest::kittest::Queryable;

    fn harness() -> Harness<'static, ThinodaApp> {
        let mut h = Harness::builder()
            .with_size(egui::vec2(1100.0, 800.0))
            .with_step_dt(1.0 / 60.0)
            .with_context_setup(|ctx| {
                ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu)
            })
            .build_ui_state(
                |ui, app: &mut ThinodaApp| app.draw(ui),
                ThinodaApp::headless(),
            );
        h.run_steps(3);
        h
    }

    #[cfg(not(feature = "gpu-tests"))]
    fn callback(h: &Harness<'_, ThinodaApp>) -> egui::ScreenshotCallback {
        h.output()
            .viewport_output
            .values()
            .flat_map(|v| &v.commands)
            .find_map(|cmd| match cmd {
                egui::ViewportCommand::Screenshot(callback) => Some(callback.clone()),
                _ => None,
            })
            .expect("native readback requested")
    }

    #[test]
    #[cfg(not(feature = "gpu-tests"))]
    fn batched_canceled_screenshot_click_cannot_change_settings() {
        use egui::{Event, Modifiers, PointerButton, TouchDeviceId, TouchId, TouchPhase};
        let mut h = harness();
        h.state_mut().map_shot.editor = Some(ShotEditor::new(
            &h.ctx,
            ColorImage::filled([240, 180], Color32::GREEN),
        ));
        h.run_steps(4);
        let original = h.state().map_shot.editor.as_ref().unwrap().crop;
        h.get_by_label("Square (1:1)").scroll_to_me();
        h.run_steps(4);
        let pos = h.get_by_label("Square (1:1)").rect().center();
        let mut events = vec![Event::PointerMoved(pos)];
        for pressed in [true, false] {
            events.push(Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            });
            events.push(Event::Touch {
                device_id: TouchDeviceId(1),
                id: TouchId(1),
                pos,
                force: None,
                phase: if pressed {
                    TouchPhase::Start
                } else {
                    TouchPhase::Cancel
                },
            });
        }
        h.input_mut().events.extend(events);
        h.run_steps(4);
        let editor = h.state().map_shot.editor.as_ref().unwrap();
        assert!(!editor.square);
        assert_eq!(editor.crop, original);
    }

    #[test]
    #[cfg(not(feature = "gpu-tests"))]
    fn interrupted_crop_drag_restores_the_pre_gesture_region() {
        use egui::{Event, Modifiers, PointerButton, TouchDeviceId, TouchId, TouchPhase};
        for reject in ["cancel", "focus", "clean"] {
            let mut h = harness();
            h.state_mut().map_shot.editor = Some(ShotEditor::new(
                &h.ctx,
                ColorImage::filled([240, 180], Color32::GREEN),
            ));
            h.run_steps(4);
            let original = h.state().map_shot.editor.as_ref().unwrap().crop;
            let rect = h.get_by_label("Map Shot preview").rect();
            let start = rect.min + egui::vec2(10.0, 10.0);
            let end = rect.center();
            h.event(Event::PointerMoved(start));
            h.event(Event::Touch {
                device_id: TouchDeviceId(1),
                id: TouchId(1),
                phase: TouchPhase::Start,
                pos: start,
                force: None,
            });
            h.event(Event::PointerButton {
                pos: start,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            });
            h.run_steps(2);
            for fraction in [0.4, 0.8, 1.0] {
                let pos = start.lerp(end, fraction);
                h.event(Event::PointerMoved(pos));
                h.event(Event::Touch {
                    device_id: TouchDeviceId(1),
                    id: TouchId(1),
                    phase: TouchPhase::Move,
                    pos,
                    force: None,
                });
                h.run_steps(2);
            }
            assert_ne!(h.state().map_shot.editor.as_ref().unwrap().crop, original);
            if reject == "focus" {
                h.event(Event::WindowFocused(false));
                h.run_steps(1);
            }
            h.event(Event::Touch {
                device_id: TouchDeviceId(1),
                id: TouchId(1),
                phase: if reject == "cancel" {
                    TouchPhase::Cancel
                } else {
                    TouchPhase::End
                },
                pos: end,
                force: None,
            });
            h.run_steps(2);
            h.event(Event::PointerButton {
                pos: end,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            });
            h.run_steps(4);
            let editor = h.state().map_shot.editor.as_ref().unwrap();
            if reject == "clean" {
                assert_ne!(editor.crop, original);
            } else {
                assert_eq!(editor.crop, original);
                assert!(editor.drag_start.is_none());
            }
        }
    }

    #[test]
    #[cfg(not(feature = "gpu-tests"))]
    fn rejected_map_shot_controls_preserve_crop_and_cannot_copy_or_dismiss() {
        use egui::{Event, Modifiers, PointerButton, TouchDeviceId, TouchId, TouchPhase};
        for dark in [false, true] {
            let mut h = harness();
            h.state_mut().settings.theme = if dark {
                thinoda_platform::settings::ThemeMode::Dark
            } else {
                thinoda_platform::settings::ThemeMode::Light
            };
            h.state_mut().map_shot.editor = Some(ShotEditor::new(
                &h.ctx,
                ColorImage::filled([240, 180], Color32::GREEN),
            ));
            h.run_steps(4);
            let original = h.state().map_shot.editor.as_ref().unwrap().crop;
            for label in ["Square (1:1)", "Copy image", "Save PNG…", "Cancel"] {
                h.get_by_label(label).scroll_to_me();
                h.run_steps(6);
                let pos = h.get_by_label(label).rect().center();
                for (contacts, cancel) in [(1, true), (2, false), (3, false)] {
                    h.event(Event::PointerMoved(pos));
                    h.event(Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed: true,
                        modifiers: Modifiers::NONE,
                    });
                    for phase in [
                        TouchPhase::Start,
                        if cancel {
                            TouchPhase::Cancel
                        } else {
                            TouchPhase::End
                        },
                    ] {
                        for id in 1..=contacts {
                            h.event(Event::Touch {
                                device_id: TouchDeviceId(1),
                                id: TouchId(id),
                                phase,
                                pos: pos + egui::vec2((id - 1) as f32 * 40.0, 0.0),
                                force: None,
                            });
                        }
                        h.run_steps(2);
                    }
                    h.event(Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed: false,
                        modifiers: Modifiers::NONE,
                    });
                    h.run_steps(1);
                    assert!(
                        !h.output()
                            .platform_output
                            .commands
                            .iter()
                            .any(|cmd| matches!(cmd, egui::OutputCommand::CopyImage(_)))
                    );
                    h.run_steps(3);
                    let editor = h
                        .state()
                        .map_shot
                        .editor
                        .as_ref()
                        .expect("rejected Cancel retains editor");
                    assert_eq!(editor.crop, original);
                    assert!(!editor.square);
                }
            }
            h.get_by_label("Square (1:1)").click();
            h.run_steps(3);
            let editor = h.state().map_shot.editor.as_ref().unwrap();
            assert!(editor.square);
            assert_eq!(editor.crop.width, editor.crop.height);
            h.get_by_label("Cancel").click();
            h.run_steps(3);
            assert!(!h.state().map_shot.active());
        }
    }

    #[test]
    #[cfg(not(feature = "gpu-tests"))]
    fn map_shot_ui_freezes_source_copies_region_and_cancels_without_edits() {
        let mut h = harness();
        let before = h.state().doc().unwrap().doc.workbook().clone();
        let selected = h.state().doc().unwrap().view.selection.clone();
        let state = h.state().doc().unwrap().doc.state_id();
        let ctx = h.ctx.clone();
        h.state_mut().exec(&ctx, Cmd::MapShot);
        h.run_steps(1);
        let native = callback(&h);
        let bounds = h.state().doc().unwrap().view.canvas_rect();
        assert!(h.state().map_shot.pending.is_some());
        assert!(h.state().map_shot.editor.is_none());
        native.complete(std::sync::Arc::new(ColorImage::filled(
            [1100, 800],
            Color32::GREEN,
        )));
        h.run_steps(3);
        let editor = h.state().map_shot.editor.as_ref().unwrap();
        let expected = canvas_pixels(
            &ColorImage::filled([1100, 800], Color32::GREEN),
            bounds,
            ctx.input(|i| i.viewport_rect()),
        )
        .unwrap();
        assert_eq!(editor.image, expected);
        h.get_by_label("Square (1:1)").scroll_to_me();
        h.run_steps(6);
        h.get_by_label("Square (1:1)").click();
        h.run_steps(2);
        assert!(h.state().map_shot.editor.as_ref().unwrap().square);
        let crop = h.state().map_shot.editor.as_ref().unwrap().crop;
        assert_eq!(crop.width, crop.height);
        h.get_by_label("Copy image").click();
        h.run_steps(1);
        let copied = h
            .output()
            .platform_output
            .commands
            .iter()
            .find_map(|cmd| match cmd {
                egui::OutputCommand::CopyImage(image) => Some(image),
                _ => None,
            })
            .expect("native image, not text or PNG bytes, placed on clipboard");
        assert_eq!(copied.size, [crop.width, crop.height]);
        assert!(copied.pixels.iter().all(|p| *p == Color32::GREEN));
        h.get_by_label("Cancel").click();
        h.run_steps(3);
        assert!(!h.state().map_shot.active());
        let d = h.state().doc().unwrap();
        assert_eq!(d.doc.workbook(), &before);
        assert_eq!(d.doc.state_id(), state);
        assert_eq!(d.view.selection, selected);
    }

    #[test]
    #[cfg(not(feature = "gpu-tests"))]
    fn map_shot_timeout_cancel_and_late_readback_do_not_reopen_editor() {
        let mut h = harness();
        let ctx = h.ctx.clone();
        h.state_mut().exec(&ctx, Cmd::MapShot);
        h.run_steps(1);
        let native = callback(&h);
        h.state_mut().map_shot.pending.as_mut().unwrap().0 = Instant::now() - CAPTURE_TIMEOUT;
        h.run_steps(1);
        assert!(!h.state().map_shot.active());
        native.complete(std::sync::Arc::new(ColorImage::filled(
            [1100, 800],
            Color32::WHITE,
        )));
        h.run_steps(2);
        assert!(!h.state().map_shot.active());
        h.state_mut().exec(&ctx, Cmd::MapShot);
        h.run_steps(1);
        let native = callback(&h);
        h.key_press(Key::Escape);
        h.run_steps(2);
        assert!(!h.state().map_shot.active());
        native.complete(std::sync::Arc::new(ColorImage::filled(
            [1100, 800],
            Color32::WHITE,
        )));
        h.run_steps(2);
        assert!(!h.state().map_shot.active());
    }

    #[test]
    #[cfg(not(feature = "gpu-tests"))]
    fn map_shot_drag_numeric_square_growth_and_narrow_footer() {
        let mut h = harness();
        let ctx = h.ctx.clone();
        h.state_mut().map_shot.editor = Some(ShotEditor::new(
            &ctx,
            ColorImage::filled([400, 300], Color32::BLUE),
        ));
        h.run_steps(4);
        h.get_by_label("Map Shot preview").scroll_to_me();
        h.run_steps(6);
        let rect = h.get_by_label("Map Shot preview").rect();
        let start = rect.min + egui::vec2(30.0, 30.0);
        let end = rect.min + egui::vec2(150.0, 120.0);
        h.hover_at(start);
        h.drag_at(start);
        h.run_steps(1);
        h.hover_at(end);
        h.run_steps(2);
        h.drop_at(end);
        h.run_steps(2);
        let crop = h.state().map_shot.editor.as_ref().unwrap().crop;
        assert_eq!(
            crop,
            PixelCrop {
                x: 30,
                y: 30,
                width: 120,
                height: 90
            }
        );
        h.get_by_label("Square (1:1)").scroll_to_me();
        h.run_steps(6);
        h.get_by_label("Square (1:1)").click();
        h.run_steps(2);
        h.get_by_label("Region width").set_value_accesskit(140.0);
        h.run_steps(2);
        assert_eq!(h.state().map_shot.editor.as_ref().unwrap().crop.width, 140);
        assert_eq!(h.state().map_shot.editor.as_ref().unwrap().crop.height, 140);
        h.get_by_label("Frame thickness").set_value_accesskit(3.0);
        h.run_steps(2);
        assert_eq!(
            h.state()
                .map_shot
                .editor
                .as_ref()
                .unwrap()
                .output()
                .unwrap()
                .size,
            [146, 146]
        );
        h.set_size(egui::vec2(420.0, 620.0));
        h.run_steps(5);
        for label in ["Cancel", "Copy image", "Save PNG…"] {
            let rect = h.get_by_label(label).rect();
            assert!(rect.min.x >= 0.0 && rect.max.x <= ctx.content_rect().max.x);
            assert!(rect.min.y >= 0.0 && rect.max.y <= ctx.content_rect().max.y);
        }
    }

    #[test]
    fn map_shot_availability_and_keyboard_entry_follow_map_mode() {
        let mut h = harness();
        let ctx = h.ctx.clone();
        h.state_mut().exec(&ctx, Cmd::ToggleOutliner);
        assert!(!h.state().command_available(&Cmd::MapShot));
        h.state_mut().exec(&ctx, Cmd::ToggleOutliner);
        h.key_press(Key::F7);
        h.run_steps(2);
        assert!(h.state().map_shot.active());
        assert!(!h.state().command_available(&Cmd::MapShot));
        h.key_press(Key::Escape);
        h.run_steps(2);
        h.state_mut().exec(&ctx, Cmd::Home);
        assert!(!h.state().command_available(&Cmd::MapShot));
    }

    #[cfg(feature = "gpu-tests")]
    #[test]
    fn map_shot_gpu_readback_contains_real_map_pixels() {
        let mut h = Harness::builder()
            .wgpu()
            .with_size(egui::vec2(1100.0, 800.0))
            .with_context_setup(|ctx| {
                ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu)
            })
            .build_ui_state(
                |ui, app: &mut ThinodaApp| app.draw(ui),
                ThinodaApp::headless(),
            );
        h.run_steps(3);
        let ctx = h.ctx.clone();
        h.state_mut().exec(&ctx, Cmd::MapShot);
        h.run_steps(3);
        let image = &h
            .state()
            .map_shot
            .editor
            .as_ref()
            .expect("GPU callback delivered")
            .image;
        assert!(image.width() > 300 && image.height() > 300);
        let bg = image[(0, 0)];
        assert!(
            image.pixels.iter().filter(|p| **p != bg).count() > 500,
            "real topic text, shapes and connectors"
        );
    }

    #[test]
    fn crop_roundtrip_preserves_exact_pixels_alpha_and_frame() {
        let image = ColorImage::from_rgba_unmultiplied(
            [3, 2],
            &[
                255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 255, 255, 255, 255, 255, 0, 0, 0, 255,
                255, 0, 255, 255,
            ],
        );
        let before = image.clone();
        let out = extract(
            &image,
            PixelCrop {
                x: 1,
                y: 0,
                width: 2,
                height: 2,
            },
            1,
            Color32::YELLOW,
        )
        .unwrap();
        assert_eq!(out.size, [4, 4]);
        assert_eq!(out[(1, 1)], image[(1, 0)]);
        assert_eq!(out[(2, 2)], image[(2, 1)]);
        assert_eq!(out[(0, 0)], Color32::YELLOW);
        let bytes = png_bytes(&out).unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (4, 4));
        assert_eq!(
            decoded.get_pixel(1, 1).0,
            image[(1, 0)].to_srgba_unmultiplied()
        );
        assert_eq!(image, before);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("shot.png");
        save_png(&path, &out).unwrap();
        assert_eq!(std::fs::read(path).unwrap(), bytes);
        assert!(save_png(&temp.path().join("shot.png").join("child.png"), &out).is_err());
    }

    #[test]
    fn viewport_crop_excludes_chrome_at_fractional_dpi_and_nonzero_origin() {
        let image = ColorImage::filled([150, 120], Color32::WHITE);
        let viewport = Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(100.0, 80.0));
        let canvas = Rect::from_min_max(egui::pos2(20.1, 30.1), egui::pos2(90.1, 90.1));
        let out = canvas_pixels(&image, canvas, viewport).unwrap();
        assert_eq!(out.size, [104, 89]);
        assert!(canvas_pixels(&image, Rect::NOTHING, viewport).is_err());
        assert!(
            canvas_pixels(&image, viewport.translate(egui::vec2(500.0, 0.0)), viewport).is_err()
        );
    }

    #[test]
    fn rejects_empty_invalid_oversized_crops_and_constrains_square() {
        let image = ColorImage::filled([4, 3], Color32::WHITE);
        for crop in [
            PixelCrop {
                x: 0,
                y: 0,
                width: 0,
                height: 1,
            },
            PixelCrop {
                x: 3,
                y: 0,
                width: 2,
                height: 1,
            },
            PixelCrop {
                x: usize::MAX,
                y: 0,
                width: 1,
                height: 1,
            },
        ] {
            assert!(extract(&image, crop, 0, Color32::WHITE).is_err());
        }
        assert!(extract(&image, PixelCrop::full(&image), 33, Color32::WHITE).is_err());
        let invalid = ColorImage {
            size: [MAX_PIXELS, 2],
            pixels: vec![],
            source_size: egui::Vec2::ZERO,
        };
        assert!(validate_image(&invalid).is_err());
        let mut crop = PixelCrop {
            x: 99,
            y: 0,
            width: 99,
            height: 99,
        };
        crop.constrain(image.size, true);
        assert_eq!(
            crop,
            PixelCrop {
                x: 3,
                y: 0,
                width: 1,
                height: 1
            }
        );
    }
}
