//! Full-map print preview and an explicit, local browser/system-print handoff.
//! Rendering and document encoding stay off the UI thread.

use std::sync::{Arc, mpsc};

use base64::{Engine, engine::general_purpose::STANDARD};
use egui::{Context, RichText};
use thinoda_core::Workbook;
use thinoda_io::print::{Paper, PrintDocument, PrintSettings, render_print};
use thinoda_ui::{Palette, widgets};

use crate::{
    app::{Cmd, ThinodaApp},
    i18n::t,
};

type RenderResult = Result<PrintDocument, String>;
type OutputResult = (Option<tempfile::TempDir>, Result<&'static str, String>);

#[derive(Default)]
pub(crate) struct PrintState {
    editor: Option<PrintEditor>,
    output: Option<mpsc::Receiver<OutputResult>>,
    // External browsers may lazily read or reload files. Keep them for this session;
    // TempDir removes the private files on normal application exit.
    documents: Vec<tempfile::TempDir>,
    message: Option<Result<&'static str, String>>,
}

impl PrintState {
    pub(crate) fn active(&self) -> bool {
        self.editor.is_some()
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
                Ok((directory, result)) => {
                    if let Some(directory) = directory {
                        self.printing.documents.push(directory);
                    }
                    Some(result)
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err(t("Print worker stopped unexpectedly").into()))
                }
                Err(mpsc::TryRecvError::Empty) => None,
            };
            if let Some(result) = result {
                match &result {
                    Ok(message) => self.toasts.info(t(message)),
                    Err(error) => self
                        .toasts
                        .error(format!("{}: {error}", t("Print preparation failed"))),
                };
                self.printing.message = Some(result);
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
        let busy = self.printing.output.is_some();
        let response = egui::Modal::new(egui::Id::unique("print-map")).show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 60.0).clamp(260.0, 860.0));
            ui.heading(t("Print"));
            ui.label(RichText::new(&editor.title).weak());
            ui.separator();
            egui::ScrollArea::vertical().max_height((ctx.content_rect().height() - 190.0).max(140.0))
                .show(ui, |ui| {
                    if ui.available_width() >= 680.0 {
                        ui.columns(2, |columns| {
                            editor.controls(&mut columns[0]);
                            editor.preview_ui(&mut columns[1]);
                        });
                    } else {
                        editor.controls(ui);
                        ui.add_space(12.0);
                        editor.preview_ui(ui);
                    }
                    ui.add_space(12.0);
                    ui.label(RichText::new(t("Continue opens a local document in your default HTML application (normally a browser) and requests its print dialog. If blocked, click Print there. Choose the same paper and orientation, 100% scale, no margins or headers. Printer settings may change the result.")).small().weak());
                    if editor.failed.is_some() && ui.button(t("Retry preview")).clicked() {
                        editor.failed = None;
                    }
                    if let Some(message) = &self.printing.message {
                        match message {
                            Ok(key) => { ui.label(t(key)); }
                            Err(error) => { ui.colored_label(Palette::of(ctx).danger, format!("{}: {error}", t("Print preparation failed"))); }
                        }
                    }
                });
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.add_enabled_ui(editor.ready() && !busy, |ui| {
                    print = widgets::primary_button(ui, t("Continue to print…")).clicked();
                    save = widgets::secondary_button(ui, t("Save print PDF…")).clicked();
                });
                close = ui.button(t("Close")).clicked();
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
                // Capture localized text before the worker runs; no user content becomes code.
                let print_label = t("Print");
                let instructions = t(
                    "If the print dialog did not open, click Print. Use the same paper and orientation, 100% scale, no margins and no headers or footers. Keep Thinoda open until printing is finished.",
                );
                let repaint = ctx.clone();
                let (sender, receiver) = mpsc::channel();
                self.printing.message = None;
                self.printing.output = Some(receiver);
                std::thread::spawn(move || {
                    let result = if let Some(path) = path {
                        let result = save_pdf(&document, &path).map(|()| "Print PDF saved");
                        (None, result)
                    } else {
                        prepare_browser_print(&document, &title, print_label, instructions)
                    };
                    let _ = sender.send(result);
                    repaint.request_repaint();
                });
            }
        }
        if !close && !response.should_close() {
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

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn browser_document(
    document: &PrintDocument,
    title: &str,
    print_label: &str,
    instructions: &str,
) -> Result<String, String> {
    let png = document.png().map_err(|e| e.to_string())?;
    let [width, height] = document.settings.size_mm();
    // Paper pixels already contain margins/background. No network, fonts, scripts
    // or unescaped map content are loaded by the browser.
    Ok(format!(
        r#"<!doctype html>
<html lang="{language}"><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data:; style-src 'unsafe-inline'; script-src 'unsafe-inline'; base-uri 'none'; form-action 'none'">
<title>{title} — Thinoda</title>
<style>
@page {{ size: {width}mm {height}mm; margin: 0; }}
* {{ box-sizing: border-box; }}
body {{ margin: 0; background: #e6e7eb; font: 16px system-ui, sans-serif; color: #171923; }}
aside {{ max-width: 800px; margin: 24px auto; padding: 20px; background: white; border-radius: 12px; }}
button {{ font: inherit; padding: 10px 24px; cursor: pointer; }}
img {{ display: block; width: {width}mm; height: {height}mm; margin: 0 auto; background: white; }}
@media (prefers-color-scheme: dark) {{ body {{ background: #202228; color: #eee; }} aside {{ background: #30333b; }} }}
@media screen {{ img {{ max-width: 95vw; height: auto; box-shadow: 0 2px 14px #0005; }} }}
@media print {{ html, body {{ width: {width}mm; height: {height}mm; background: white; }}
aside {{ display: none; }} img {{ margin: 0; print-color-adjust: exact; -webkit-print-color-adjust: exact; }} }}
</style></head><body>
<aside><h1>{title}</h1><p>{instructions}</p><p>{width} × {height} mm</p>
<button id="print" type="button">{print_label}</button></aside>
<img alt="{title}" src="data:image/png;base64,{image}">
<script>
document.getElementById('print').addEventListener('click', () => window.print());
window.addEventListener('load', () => window.print());
</script></body></html>"#,
        title = escape_html(title),
        instructions = escape_html(instructions),
        print_label = escape_html(print_label),
        image = STANDARD.encode(png),
        language = match crate::i18n::lang() {
            crate::i18n::Lang::En => "en",
            crate::i18n::Lang::De => "de",
        },
    ))
}

fn prepare_browser_print(
    document: &PrintDocument,
    title: &str,
    print_label: &str,
    instructions: &str,
) -> OutputResult {
    prepare_browser_print_with(
        document,
        title,
        print_label,
        instructions,
        thinoda_platform::system::open_print_document,
    )
}

fn prepare_browser_print_with(
    document: &PrintDocument,
    title: &str,
    print_label: &str,
    instructions: &str,
    open: impl FnOnce(&std::path::Path) -> std::io::Result<()>,
) -> OutputResult {
    let prepare = || -> Result<tempfile::TempDir, String> {
        let html = browser_document(document, title, print_label, instructions)?;
        let directory = tempfile::Builder::new()
            .prefix("thinoda-print-")
            .tempdir()
            .map_err(|e| e.to_string())?;
        std::fs::write(directory.path().join("map.html"), html).map_err(|e| e.to_string())?;
        Ok(directory)
    };
    match prepare() {
        Ok(directory) => {
            let result = open(&directory.path().join("map.html"))
                .map(|()| "Print document handed to the system. Printing is not confirmed. Keep Thinoda open until finished.")
                .map_err(|e| e.to_string());
            (Some(directory), result)
        }
        Err(error) => (None, Err(error)),
    }
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
    fn browser_document_is_local_escaped_and_matches_preview_pixels() {
        let document = tiny_document();
        let html = browser_document(
            &document,
            "<script>alert('title')</script>",
            "Drucken",
            "Hinweis & Hilfe",
        )
        .unwrap();
        assert!(!html.contains("<script>alert"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("Hinweis &amp; Hilfe"));
        assert!(html.contains(">Drucken</button>"));
        assert!(html.contains("size: 297mm 210mm"));
        assert!(html.contains("window.print()"));
        assert!(html.contains("default-src 'none'"));
        assert!(html.contains("prefers-color-scheme: dark"));
        assert!(html.contains("aside { display: none; }"));
        let data = html
            .split("data:image/png;base64,")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        let decoded = STANDARD.decode(data).unwrap();
        assert_eq!(
            image::load_from_memory(&decoded).unwrap().to_rgb8(),
            document.image
        );
    }

    #[test]
    fn launcher_failures_are_errors_not_fallback_success_and_files_live_until_dropped() {
        let document = tiny_document();
        let (directory, result) =
            prepare_browser_print_with(&document, "Map", "Print", "Instructions", |path| {
                assert!(path.is_absolute() && path.is_file());
                assert!(
                    std::fs::read_to_string(path)
                        .unwrap()
                        .contains("window.print()")
                );
                Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "browser unavailable",
                ))
            });
        assert_eq!(result.unwrap_err(), "browser unavailable");
        let directory = directory.unwrap();
        let path = directory.path().join("map.html");
        assert!(path.exists());
        drop(directory);
        assert!(!path.exists());
        let (directory, result) =
            prepare_browser_print_with(&document, "Map", "Print", "Instructions", |_| Ok(()));
        assert!(directory.is_some());
        assert!(result.unwrap().contains("Printing is not confirmed"));
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
            assert!(h.state().printing.documents.is_empty());
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
