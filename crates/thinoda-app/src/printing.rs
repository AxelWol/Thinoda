//! Full-map preview and native printer UI. Rendering/encoding run on workers;
//! OS dialogs and submission run synchronously on the window event-loop thread.

use std::sync::{Arc, mpsc};

use egui::{Context, RichText};
use raw_window_handle::HasWindowHandle;
use thinoda_core::Workbook;
use thinoda_io::print::{Paper, PrintDocument, PrintSettings, render_print};
use thinoda_platform::printing::{PrintOutcome, PrintPage};
use thinoda_ui::{Palette, widgets};

use crate::{
    app::{Cmd, ThinodaApp},
    i18n::t,
};

type RenderResult = Result<PrintDocument, String>;
type OutputResult = Result<PreparedOutput, String>;

enum PreparedOutput {
    Native(PrintPage, String),
    PdfSaved,
}

#[derive(Default)]
pub(crate) struct PrintState {
    editor: Option<PrintEditor>,
    output: Option<mpsc::Receiver<OutputResult>>,
    native: Option<(PrintPage, String)>,
    native_open: bool,
    message: Option<Result<&'static str, String>>,
}

impl PrintState {
    pub(crate) fn active(&self) -> bool {
        self.editor.is_some() || self.busy()
    }

    fn busy(&self) -> bool {
        self.output.is_some() || self.native.is_some() || self.native_open
    }

    fn native_result(&mut self, result: Result<PrintOutcome, String>) {
        self.message = Some(result.map(|outcome| match outcome {
            PrintOutcome::Cancelled => "Printing cancelled",
            PrintOutcome::Submitted => {
                "Print job submitted to the OS. Physical output is not confirmed."
            }
        }));
    }
}

struct PrintEditor {
    workbook: Arc<Workbook>,
    sheet: usize,
    title: String,
    settings: PrintSettings,
    pending: Option<(PrintSettings, mpsc::Receiver<RenderResult>)>,
    rendered: Option<Arc<PrintDocument>>,
    preview: Option<egui::TextureHandle>,
    failed: Option<(PrintSettings, String)>,
}

impl PrintEditor {
    fn ready(&self) -> bool {
        self.rendered
            .as_ref()
            .is_some_and(|d| d.settings == self.settings)
            && self.pending.is_none()
    }

    fn poll(&mut self, ctx: &Context) {
        if let Some((settings, receiver)) = &self.pending {
            match receiver.try_recv() {
                Ok(result) => {
                    let requested = *settings;
                    self.pending = None;
                    // Do not show or print a stale preview after a settings change.
                    if requested == self.settings {
                        match result {
                            Ok(document) => {
                                let thumb = preview_thumbnail(&document.image);
                                let image = egui::ColorImage::from_rgb(
                                    [thumb.width() as usize, thumb.height() as usize],
                                    thumb.as_raw(),
                                );
                                self.preview = Some(ctx.load_texture(
                                    "print-preview",
                                    image,
                                    egui::TextureOptions::LINEAR,
                                ));
                                self.rendered = Some(Arc::new(document));
                                self.failed = None;
                            }
                            Err(error) => self.failed = Some((requested, error)),
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.failed = Some((*settings, t("Print worker stopped unexpectedly").into()));
                    self.pending = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.pending.is_none()
            && !self.ready()
            && self
                .failed
                .as_ref()
                .is_none_or(|(s, _)| *s != self.settings)
        {
            let workbook = Arc::clone(&self.workbook);
            let sheet = self.sheet;
            let settings = self.settings;
            let repaint = ctx.clone();
            let (sender, receiver) = mpsc::channel();
            self.pending = Some((settings, receiver));
            std::thread::spawn(move || {
                let result = render_print(&workbook, sheet, settings).map_err(|e| e.to_string());
                let _ = sender.send(result);
                repaint.request_repaint();
            });
        }
        if self.pending.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        ui.strong(t("Page setup"));
        ui.add_space(8.0);
        egui::Grid::new("print-settings")
            .spacing([12.0, 12.0])
            .show(ui, |ui| {
                ui.label(t("Paper size"));
                egui::ComboBox::from_id_salt("print-paper")
                    .selected_text(t(self.settings.paper.label()))
                    .show_ui(ui, |ui| {
                        for paper in Paper::ALL {
                            ui.selectable_value(&mut self.settings.paper, paper, t(paper.label()));
                        }
                    })
                    .response
                    .accessible_name(t("Paper size"));
                ui.end_row();
                ui.label(t("Orientation"));
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.settings.landscape, false, t("Portrait"));
                    ui.selectable_value(&mut self.settings.landscape, true, t("Landscape"));
                });
                ui.end_row();
                let label = ui.label(t("Margins"));
                ui.add(
                    egui::DragValue::new(&mut self.settings.margin_mm)
                        .range(0.0..=40.0)
                        .speed(0.5)
                        .suffix(" mm"),
                )
                .labelled_by(label.id);
                ui.end_row();
                ui.label(t("Resolution"));
                egui::ComboBox::from_id_salt("print-resolution")
                    .selected_text(format!("{} dpi", self.settings.dpi))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.settings.dpi, 150, "150 dpi");
                        ui.selectable_value(&mut self.settings.dpi, 300, "300 dpi");
                    })
                    .response
                    .accessible_name(t("Resolution"));
                ui.end_row();
            });
        ui.add_space(16.0);
        ui.strong(t("Map appearance"));
        if ui
            .checkbox(&mut self.settings.dark, t("Dark map"))
            .changed()
            && self.settings.dark
        {
            self.settings.background = true;
        }
        ui.add_enabled_ui(!self.settings.dark, |ui| {
            ui.checkbox(&mut self.settings.background, t("Map background"));
        });
        ui.checkbox(&mut self.settings.grayscale, t("Grayscale"));
        ui.add_space(8.0);
        ui.label(RichText::new(t("Fit the current sheet on one page. Offscreen topics are included; collapsed branches stay collapsed.")).small().weak());
    }

    fn preview_ui(&self, ui: &mut egui::Ui) {
        let palette = Palette::of(ui.ctx());
        egui::Frame::new()
            .fill(palette.surface_alt)
            .corner_radius(10)
            .inner_margin(14.0)
            .show(ui, |ui| {
                ui.strong(t("Print preview"));
                ui.add_space(8.0);
                let max_height = (ui.ctx().content_rect().height() * 0.43).clamp(120.0, 380.0);
                let [w, h] = self.settings.size_mm();
                let page = egui::vec2(w, h);
                let scale = (ui.available_width() / page.x).min(max_height / page.y);
                let size = page * scale;
                if self.ready()
                    && let Some(texture) = &self.preview
                {
                    ui.add(
                        egui::Image::new(texture)
                            .fit_to_exact_size(size)
                            .alt_text(t("Print preview")),
                    );
                } else {
                    // Keep the paper footprint stable while rendering, but never
                    // mislabel a stale page as the newly selected settings.
                    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                    ui.painter().rect_filled(rect, 0, egui::Color32::WHITE);
                }
                if !self.ready() {
                    if let Some((settings, error)) = &self.failed
                        && *settings == self.settings
                    {
                        ui.colored_label(
                            palette.danger,
                            format!("{}: {error}", t("Could not prepare print preview")),
                        );
                    } else {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(t("Preparing print preview…"));
                        });
                    }
                }
                ui.label(
                    RichText::new(format!(
                        "{} · {w:.1} × {h:.1} mm · {} dpi",
                        t("1 page"),
                        self.settings.dpi
                    ))
                    .small()
                    .weak(),
                );
            });
    }
}

fn preview_thumbnail(image: &image::RgbImage) -> image::RgbImage {
    // imageops::thumbnail takes exact dimensions, unlike DynamicImage::thumbnail.
    // Preserve paper aspect ratio rather than squashing every preview to a square.
    let scale = (900.0 / image.width() as f32)
        .min(900.0 / image.height() as f32)
        .min(1.0);
    image::imageops::thumbnail(
        image,
        (image.width() as f32 * scale).round().max(1.0) as u32,
        (image.height() as f32 * scale).round().max(1.0) as u32,
    )
}

impl ThinodaApp {
    /// Only the real eframe adapter calls this, after drawing the modal preview.
    /// Headless UI tests never invoke native UI or send jobs. Frame lends its live
    /// native window; no raw handle is cached or sent to a worker.
    pub(crate) fn run_native_print(&mut self, frame: &eframe::Frame, ctx: &Context) {
        let Some((page, title)) = self.printing.native.take() else {
            return;
        };
        self.printing.native_open = true;
        let result = frame
            .window_handle()
            .map_err(|e| format!("Cannot access the application print window: {e}"))
            .and_then(|owner| thinoda_platform::printing::print(owner, &page, &title));
        self.printing.native_open = false;
        match &result {
            Ok(PrintOutcome::Cancelled) => self.toasts.info(t("Printing cancelled")),
            Ok(PrintOutcome::Submitted) => self.toasts.info(t(
                "Print job submitted to the OS. Physical output is not confirmed.",
            )),
            Err(error) => self
                .toasts
                .error(format!("{}: {error}", t("Printing failed"))),
        };
        self.printing.native_result(result);
        ctx.request_repaint();
        // Keep the preview open on cancellation/error so settings can be changed
        // or printing retried. Status toasts remain visible on small windows
        // even when the inline result is below the preview's scroll position.
    }

    pub(crate) fn begin_print(&mut self, ctx: &Context) {
        if !self.command_available(&Cmd::Print) {
            self.toasts.warning(t("Not possible here"));
            return;
        }
        let Some(doc) = self.doc_mut() else { return };
        doc.view.commit_edit(&mut doc.doc);
        let sheet = doc.doc.active_index();
        let title = doc.doc.sheet().title.clone();
        let workbook = Arc::new(doc.doc.workbook().clone());
        self.printing.message = None;
        self.printing.editor = Some(PrintEditor {
            workbook,
            sheet,
            title,
            settings: PrintSettings::default(),
            pending: None,
            rendered: None,
            preview: None,
            failed: None,
        });
        ctx.request_repaint();
    }

    pub(crate) fn print_dialog(&mut self, ctx: &Context) {
        if let Some(receiver) = &self.printing.output {
            let result = match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err(t("Print worker stopped unexpectedly").into()))
                }
                Err(mpsc::TryRecvError::Empty) => None,
            };
            if let Some(result) = result {
                match result {
                    Ok(PreparedOutput::Native(page, title)) => {
                        self.printing.native = Some((page, title));
                    }
                    Ok(PreparedOutput::PdfSaved) => {
                        self.toasts.info(t("Print PDF saved"));
                        self.printing.message = Some(Ok("Print PDF saved"));
                    }
                    Err(error) => {
                        self.toasts
                            .error(format!("{}: {error}", t("Printing failed")));
                        self.printing.message = Some(Err(error));
                    }
                }
                self.printing.output = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
        let Some(mut editor) = self.printing.editor.take() else {
            return;
        };
        editor.poll(ctx);
        let mut close = false;
        let mut print = false;
        let mut save = false;
        let busy = self.printing.busy();
        let response = egui::Modal::new(egui::Id::unique("print-map")).show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 60.0).clamp(260.0, 860.0));
            ui.heading(t("Print"));
            ui.label(RichText::new(&editor.title).weak());
            ui.separator();
            egui::ScrollArea::vertical().max_height((ctx.content_rect().height() - 190.0).max(140.0))
                .show(ui, |ui| {
                    if ui.available_width() >= 680.0 {
                        ui.columns(2, |columns| {
                            columns[0].add_enabled_ui(!busy, |ui| editor.controls(ui));
                            editor.preview_ui(&mut columns[1]);
                        });
                    } else {
                        ui.add_enabled_ui(!busy, |ui| editor.controls(ui));
                        ui.add_space(12.0);
                        editor.preview_ui(ui);
                    }
                    ui.add_space(12.0);
                    ui.label(RichText::new(t("Continue opens the OS print dialog with printer selection and driver settings. The whole preview page, including margins, is fitted proportionally inside the selected printable area. Changing OS paper or orientation scales the page without reflow; driver options may further change output. Resolution here controls source detail, not printer quality.")).small().weak());
                    if editor.failed.is_some() && ui.button(t("Retry preview")).clicked() {
                        editor.failed = None;
                    }
                    if let Some(message) = &self.printing.message {
                        match message {
                            Ok(key) => { ui.label(t(key)); }
                            Err(error) => { ui.colored_label(Palette::of(ctx).danger, format!("{}: {error}", t("Printing failed"))); }
                        }
                    }
                });
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.add_enabled_ui(editor.ready() && !busy, |ui| {
                    print = widgets::primary_button(ui, t("Continue to print…")).clicked();
                    save = widgets::secondary_button(ui, t("Save print PDF…")).clicked();
                });
                close = ui.add_enabled(!busy, egui::Button::new(t("Close"))).clicked();
                if busy {
                    ui.spinner();
                    ui.label(t("Preparing print document…"));
                }
            });
        });
        if print || save {
            let path = if save {
                rfd::FileDialog::new()
                    .set_title(t("Save print PDF…"))
                    .add_filter("PDF", &["pdf"])
                    .set_file_name("map-print.pdf")
                    .save_file()
            } else {
                None
            };
            if print || path.is_some() {
                let document = Arc::clone(editor.rendered.as_ref().expect("ready print document"));
                let title = editor.title.clone();
                let repaint = ctx.clone();
                let (sender, receiver) = mpsc::channel();
                self.printing.message = None;
                self.printing.output = Some(receiver);
                std::thread::spawn(move || {
                    let result = if let Some(path) = path {
                        save_pdf(&document, &path).map(|()| PreparedOutput::PdfSaved)
                    } else {
                        prepare_native_page(&document)
                            .map(|page| PreparedOutput::Native(page, title))
                    };
                    let _ = sender.send(result);
                    repaint.request_repaint();
                });
            }
        }
        if self.printing.busy() || (!close && !response.should_close()) {
            self.printing.editor = Some(editor);
        }
    }
}

fn save_pdf(document: &PrintDocument, path: &std::path::Path) -> Result<(), String> {
    if path
        .extension()
        .is_none_or(|e| !e.eq_ignore_ascii_case("pdf"))
    {
        return Err(t("Print PDF must be saved with a .pdf extension").into());
    }
    let bytes = document.pdf().map_err(|e| e.to_string())?;
    thinoda_platform::fs::atomic_write(path, &bytes, false).map_err(|e| e.to_string())
}

fn prepare_native_page(document: &PrintDocument) -> Result<PrintPage, String> {
    PrintPage::from_rgb(
        document.image.as_raw(),
        document.image.width(),
        document.image.height(),
        document.settings.size_mm().map(f64::from),
        document.settings.dpi,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Key, Modifiers, accesskit::Role};
    use egui_kittest::{Harness, kittest::Queryable};
    use thinoda_platform::settings::ThemeMode;

    fn tiny_document() -> PrintDocument {
        PrintDocument {
            settings: PrintSettings::default(),
            image: image::RgbImage::from_pixel(12, 8, image::Rgb([10, 90, 150])),
        }
    }

    #[test]
    fn preview_thumbnail_preserves_page_aspect_ratio_without_upscaling() {
        for dimensions in [(1754, 1240), (1240, 1754), (3508, 4961), (12, 8)] {
            let image = image::RgbImage::new(dimensions.0, dimensions.1);
            let thumb = preview_thumbnail(&image);
            assert!(thumb.width() <= 900 && thumb.height() <= 900);
            assert!(thumb.width() <= image.width() && thumb.height() <= image.height());
            let ratio = image.width() as f32 / image.height() as f32;
            assert!((thumb.width() as f32 / thumb.height() as f32 - ratio).abs() < 0.003);
        }
    }

    #[test]
    fn stale_render_results_cannot_enable_print_and_worker_errors_are_visible() {
        let settings = PrintSettings::default();
        let (sender, receiver) = mpsc::channel();
        let mut editor = PrintEditor {
            workbook: Arc::new(Workbook::default()),
            sheet: 0,
            title: "Map".into(),
            settings,
            pending: Some((settings, receiver)),
            rendered: None,
            preview: None,
            failed: None,
        };
        let ctx = Context::default();
        assert!(sender.send(Err("Rendering failed".into())).is_ok());
        editor.poll(&ctx);
        assert_eq!(editor.failed, Some((settings, "Rendering failed".into())));
        assert!(!editor.ready() && editor.pending.is_none());

        let (sender, receiver) = mpsc::channel();
        editor.pending = Some((settings, receiver));
        editor.settings.landscape = false;
        editor.failed = None;
        assert!(sender.send(Ok(tiny_document())).is_ok());
        editor.poll(&ctx);
        assert!(editor.rendered.is_none() && editor.preview.is_none() && !editor.ready());
        assert_eq!(editor.pending.as_ref().unwrap().0, editor.settings);

        let (sender, receiver) = mpsc::channel();
        editor.pending = Some((editor.settings, receiver));
        drop(sender);
        editor.poll(&ctx);
        assert!(editor.failed.is_some() && !editor.ready());
        assert!(editor.pending.is_none());
    }

    #[test]
    fn native_page_uses_rendered_geometry_and_rejects_mismatched_pixels() {
        assert!(prepare_native_page(&tiny_document()).is_err());
        for paper in Paper::ALL {
            for landscape in [false, true] {
                let document = render_print(
                    &Workbook::default(),
                    0,
                    PrintSettings {
                        paper,
                        landscape,
                        grayscale: true,
                        ..Default::default()
                    },
                )
                .unwrap();
                assert!(prepare_native_page(&document).is_ok());
            }
        }
    }

    #[test]
    fn native_outcomes_do_not_claim_physical_completion() {
        let mut state = PrintState::default();
        state.native_result(Ok(PrintOutcome::Cancelled));
        assert_eq!(state.message, Some(Ok("Printing cancelled")));
        state.native_result(Err("driver unavailable".into()));
        assert_eq!(state.message, Some(Err("driver unavailable".into())));
        state.native_result(Ok(PrintOutcome::Submitted));
        assert_eq!(
            state.message,
            Some(Ok(
                "Print job submitted to the OS. Physical output is not confirmed."
            ))
        );
        assert!(!state.active());
        state.native_open = true;
        assert!(state.active());
        state.native_open = false;
        let (_sender, receiver) = mpsc::channel();
        state.output = Some(receiver);
        assert!(state.active());
    }

    #[test]
    fn pdf_save_is_atomic_and_rejects_wrong_extension() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("printed.pdf");
        save_pdf(&tiny_document(), &path).unwrap();
        assert!(std::fs::read(path).unwrap().starts_with(b"%PDF-1.4"));
        let wrong = directory.path().join("printed.png");
        assert!(save_pdf(&tiny_document(), &wrong).is_err());
        assert!(!wrong.exists());
        let file = directory.path().join("not-a-directory");
        std::fs::write(&file, b"occupied").unwrap();
        assert!(save_pdf(&tiny_document(), &file.join("print.pdf")).is_err());
    }

    fn harness(theme: ThemeMode, size: egui::Vec2) -> Harness<'static, ThinodaApp> {
        let mut app = ThinodaApp::headless();
        app.settings.theme = theme;
        let mut harness = Harness::builder()
            .with_size(size)
            .with_context_setup(|ctx| {
                ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu)
            })
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        harness.run_steps(3);
        harness
    }

    fn wait_ready(h: &mut Harness<'_, ThinodaApp>) {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            h.run_steps(1);
            let editor = h.state().printing.editor.as_ref().unwrap();
            assert!(editor.failed.is_none(), "{:?}", editor.failed);
            if editor.ready() {
                break;
            }
            assert!(std::time::Instant::now() < until, "print preview timed out");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        h.run_steps(2);
    }

    #[test]
    fn prepared_native_page_stays_modal_and_cancel_or_error_preserves_retry() {
        let mut h = harness(ThemeMode::Light, egui::vec2(1280.0, 850.0));
        let ctx = h.ctx.clone();
        h.state_mut().exec(&ctx, Cmd::Print);
        wait_ready(&mut h);
        let before = h.state().doc().unwrap().doc.workbook().clone();
        h.get_by_role_and_label(Role::Button, "Continue to print…")
            .click();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            h.run_steps(1);
            if h.state().printing.native.is_some() {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "native page preparation timed out"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        // Headless harness has no eframe adapter: it can test the native request
        // and input gating without ever opening a dialog or submitting a job.
        h.key_press(Key::Escape);
        h.key_press(Key::Delete);
        h.key_press_modifiers(Modifiers::COMMAND, Key::N);
        h.run_steps(2);
        assert!(h.state().printing.editor.is_some());
        assert!(!h.state().command_available(&Cmd::Print));
        assert_eq!(h.state().docs.len(), 1);
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        h.state_mut().printing.native.take().unwrap();
        h.state_mut()
            .printing
            .native_result(Ok(PrintOutcome::Cancelled));
        h.run_steps(2);
        h.get_by_label("Printing cancelled");
        assert!(h.state().printing.editor.as_ref().unwrap().ready());
        h.state_mut()
            .printing
            .native_result(Err("test driver failed".into()));
        h.run_steps(2);
        h.get_by_label("Printing failed: test driver failed");
        h.get_by_role_and_label(Role::Button, "Continue to print…");
        h.get_by_role_and_label(Role::Button, "Close").click();
        h.run_steps(2);
        assert!(!h.state().printing.active());
    }

    #[test]
    fn menu_preview_settings_and_close_work_in_both_themes_without_edits() {
        for theme in [ThemeMode::Light, ThemeMode::Dark] {
            let mut h = harness(theme, egui::vec2(1280.0, 850.0));
            let before = h.state().doc().unwrap().doc.workbook().clone();
            let selected = h.state().doc().unwrap().view.selection.clone();
            let state = h.state().doc().unwrap().doc.state_id();
            h.get_by_label(thinoda_ui::icons::MENU).click();
            h.run_steps(2);
            h.get_by_role_and_label(Role::Button, "Print Ctrl+P")
                .click();
            h.run_steps(2);
            assert!(h.state().printing.active());
            assert!(!h.state().command_available(&Cmd::Print));
            wait_ready(&mut h);
            h.get_by_role_and_label(Role::Image, "Print preview");
            let old = h
                .state()
                .printing
                .editor
                .as_ref()
                .unwrap()
                .rendered
                .as_ref()
                .unwrap()
                .image
                .dimensions();
            h.get_by_label("Portrait").click();
            h.run_steps(2);
            wait_ready(&mut h);
            let editor = h.state().printing.editor.as_ref().unwrap();
            assert_eq!(
                editor.rendered.as_ref().unwrap().image.dimensions(),
                (old.1, old.0)
            );
            h.get_by_label("Grayscale").click();
            h.run_steps(2);
            wait_ready(&mut h);
            let editor = h.state().printing.editor.as_ref().unwrap();
            assert!(editor.settings.grayscale);
            assert!(
                editor
                    .rendered
                    .as_ref()
                    .unwrap()
                    .image
                    .pixels()
                    .all(|p| p.0[0] == p.0[1] && p.0[1] == p.0[2])
            );
            h.get_by_role_and_label(Role::Button, "Continue to print…");
            h.get_by_role_and_label(Role::Button, "Save print PDF…");
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
            assert_eq!(h.state().doc().unwrap().doc.state_id(), state);
            assert_eq!(h.state().doc().unwrap().view.selection, selected);
            h.get_by_role_and_label(Role::Button, "Close").click();
            h.run_steps(2);
            assert!(!h.state().printing.active());
            assert!(h.state().printing.native.is_none());
        }
    }

    #[test]
    fn keyboard_entry_source_snapshot_modal_and_small_window() {
        let mut h = harness(ThemeMode::Dark, egui::vec2(720.0, 480.0));
        let ctx = h.ctx.clone();
        h.state_mut().exec(&ctx, Cmd::NewSheet);
        h.state_mut().exec(&ctx, Cmd::ToggleOutliner);
        let sheet = h.state().doc().unwrap().doc.active_index();
        h.key_press_modifiers(Modifiers::COMMAND, Key::P);
        h.run_steps(2);
        assert_eq!(h.state().printing.editor.as_ref().unwrap().sheet, sheet);
        let before = h.state().doc().unwrap().doc.state_id();
        h.key_press(Key::Tab);
        h.key_press(Key::Delete);
        h.key_press_modifiers(Modifiers::COMMAND, Key::N);
        h.run_steps(2);
        assert_eq!(h.state().docs.len(), 1);
        assert_eq!(h.state().doc().unwrap().doc.state_id(), before);
        for label in ["Close", "Continue to print…", "Save print PDF…"] {
            let rect = h.get_by_role_and_label(Role::Button, label).rect();
            assert!(ctx.content_rect().contains_rect(rect), "{label}: {rect:?}");
        }
        h.key_press(Key::Escape);
        h.run_steps(2);
        assert!(!h.state().printing.active());
        h.state_mut().exec(&ctx, Cmd::Home);
        assert!(!h.state().command_available(&Cmd::Print));
        h.state_mut().exec(&ctx, Cmd::Print);
        assert!(!h.state().printing.active());
        assert!(
            ThinodaApp::palette_commands()
                .iter()
                .any(|(cmd, _)| *cmd == Cmd::Print)
        );
    }

    #[test]
    fn opening_commits_title_and_freezes_only_current_sheet() {
        let mut app = ThinodaApp::headless();
        let ctx = Context::default();
        app.exec(&ctx, Cmd::NewSheet);
        let index = app.doc().unwrap().doc.active_index();
        let doc = app.doc_mut().unwrap();
        let sheet = doc.doc.sheet().clone();
        doc.view
            .begin_edit_with(&sheet, sheet.root, "Unsaved title for printing");
        app.exec(&ctx, Cmd::Print);
        let editor = app.printing.editor.as_ref().unwrap();
        assert_eq!(editor.sheet, index);
        assert_eq!(editor.workbook.as_ref(), app.doc().unwrap().doc.workbook());
        let sheet = &editor.workbook.sheets[index];
        assert_eq!(
            sheet.topics[&sheet.root].title,
            "Unsaved title for printing"
        );
        assert!(app.doc().unwrap().view.editing_topic().is_none());
    }
}
