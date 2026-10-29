//! Presentation drafts and outputs share saved frames; playback never owns a document.

use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
};

use egui::Context;
use thinoda_core::{PresentationSource, Rect, Sheet, TopicId};
use thinoda_io::ExportOptions;
use thinoda_ui::{Palette, widgets};

use crate::{app::ThinodaApp, i18n::t};

mod pitch_ai;
mod pitch_motion;
mod pitch_player;
mod pitch_preview;
use pitch_preview::SlidePreview;

#[cfg(test)]
mod tests;

#[derive(Clone)]
struct PitchOwner {
    document: String,
    sheet: usize,
    root: TopicId,
    revision: u64,
    viewport: egui::ViewportId,
}

struct PitchEditor {
    owner: PitchOwner,
    sheet: Sheet,
    selected: usize,
    generation: u64,
    preview: SlidePreview,
    error: Option<String>,
    composing: bool,
    ai: pitch_ai::PitchAi,
}

struct PitchPlayer {
    owner: PitchOwner,
    sheet: Arc<Sheet>,
    layout: Arc<thinoda_core::Layout>,
    selected: usize,
    presenter: bool,
    fullscreen: bool,
    playback: pitch_motion::Playback,
}

#[derive(Default)]
pub(crate) struct PitchState {
    editor: Option<PitchEditor>,
    player: Option<PitchPlayer>,
    output: Option<mpsc::Receiver<Result<PathBuf, String>>>,
}

impl PitchState {
    pub(crate) fn ai_busy(&self) -> bool {
        self.editor
            .as_ref()
            .is_some_and(|editor| editor.ai.is_busy())
    }

    pub(crate) fn blocks_map(&self) -> bool {
        self.editor.is_some() || (crate::files::is_web() && self.player.is_some())
    }
}

impl ThinodaApp {
    pub(crate) fn begin_presentation_editor(&mut self, ctx: &Context) {
        let Some(doc) = self.doc_mut() else { return };
        if let Err(error) = doc.flush_inspector_edit(ctx) {
            self.toasts.error(error);
            return;
        }
        doc.view.commit_edit(&mut doc.doc);
        self.pitch.editor = Some(PitchEditor {
            owner: PitchOwner {
                document: doc.key.clone(),
                sheet: doc.doc.active_index(),
                root: doc.doc.sheet().root,
                revision: doc.doc.state_id(),
                viewport: ctx.viewport_id(),
            },
            sheet: doc.doc.sheet().clone(),
            selected: 0,
            generation: 0,
            preview: Default::default(),
            error: None,
            composing: false,
            ai: Default::default(),
        });
    }

    fn apply_presentation_draft(&mut self, editor: &PitchEditor) -> Result<(), String> {
        let doc = self
            .docs
            .iter_mut()
            .find(|doc| doc.key == editor.owner.document)
            .ok_or_else(|| t("The document changed. Try again.").to_string())?;
        if doc.doc.state_id() != editor.owner.revision
            || doc.doc.active_index() != editor.owner.sheet
            || doc.doc.sheet().root != editor.owner.root
            || doc.inspector_edit.is_some()
            || doc.view.editing_topic().is_some()
        {
            return Err(t("The document changed. Try again.").into());
        }
        doc.doc
            .replace_presentation(editor.sheet.presentation.clone())
            .map_err(|error| error.to_string())
    }

    fn presentation_source_frame(
        &mut self,
        owner: &PitchOwner,
        source: PresentationSource,
    ) -> Result<Rect, String> {
        let doc = self
            .docs
            .iter_mut()
            .find(|doc| doc.key == owner.document)
            .filter(|doc| {
                doc.doc.state_id() == owner.revision
                    && doc.doc.active_index() == owner.sheet
                    && doc.doc.sheet().root == owner.root
            })
            .ok_or_else(|| t("The document changed. Try again.").to_string())?;
        match source {
            PresentationSource::Branch(topic) => doc
                .view
                .topic_group_bounds(&doc.doc, &[topic])
                .map(|bounds| bounds.expand(24.0)),
            PresentationSource::Zone(id) => doc
                .doc
                .sheet()
                .zones
                .iter()
                .find(|zone| zone.id == id)
                .map(|zone| zone.frame),
        }
        .ok_or_else(|| t("Not possible here").to_string())
    }

    pub(crate) fn presentation_dialog(&mut self, ctx: &Context) {
        if self
            .pitch
            .editor
            .as_ref()
            .is_some_and(|editor| editor.owner.viewport != ctx.viewport_id())
        {
            return;
        }
        let Some(mut editor) = self.pitch.editor.take() else {
            return;
        };
        let ime = ctx.input(|input| {
            input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::Ime(_)))
        });
        let focus = ctx
            .memory(|memory| memory.focused())
            .unwrap_or(egui::Id::unique("pitch-ime"));
        thinoda_canvas::protect_ime_candidate_keys(ctx, focus, &mut editor.composing);
        let touch = widgets::touch_activation_allowed(ctx);
        let options = ExportOptions {
            dark: Palette::of(ctx).dark,
            ..Default::default()
        };
        let source = self
            .docs
            .iter()
            .find(|doc| {
                doc.key == editor.owner.document
                    && doc.doc.state_id() == editor.owner.revision
                    && doc.doc.active_index() == editor.owner.sheet
            })
            .and_then(|doc| match doc.view.selected_object {
                Some(thinoda_canvas::ObjectSelection::Zone(id)) => {
                    Some(PresentationSource::Zone(id))
                }
                _ => doc.view.primary().map(PresentationSource::Branch),
            });
        let mut apply = false;
        let mut cancel = false;
        let mut generate = false;
        let mut add = false;
        let mut fit = false;
        let mut export = None;
        let labels = crate::i18n::dialog_labels();
        let response = thinoda_ui::Dialog::new(egui::Id::unique("presentation-editor"), t("Edit Presentation"))
            .icon(thinoda_ui::icons::EDIT_PRESENTATION)
            .labels(&labels)
            .modal(true)
            .close_on_backdrop(true)
            .default_width(760.0)
            .default_height(700.0)
            .min_size(egui::vec2(400.0, 300.0))
            .scroll(false)
            .show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height((ctx.content_rect().height() - 150.0).max(40.0)).show(ui, |ui| {
                ui.label(&editor.sheet.title);
                ui.label(t("Generate replaces the slide draft. Apply saves one undoable change; Cancel keeps the saved presentation."));
                ui.horizontal_wrapped(|ui| {
                    generate = ui.button(t("Generate Slides")).clicked() && touch;
                    add = ui.add_enabled(source.is_some(), egui::Button::new(t("Add Selected Branch or Zone"))).clicked() && touch;
                });
                if editor.sheet.presentation.slides.is_empty() {
                    ui.label(t("No slides yet. Generate branch and Zone slides, or add the selected branch."));
                }
                let mut move_slide = None;
                let mut remove = false;
                egui::ScrollArea::vertical().id_salt("pitch-slide-list").max_height(140.0).show(ui, |ui| {
                    for (index, slide) in editor.sheet.presentation.slides.iter().enumerate() {
                        if ui.selectable_label(editor.selected == index, format!("{}. {}", index + 1, slide.title)).clicked() && touch {
                            editor.selected = index;
                        }
                    }
                });
                if let Some(slide) = editor.sheet.presentation.slides.get_mut(editor.selected) {
                    let previous = (slide.title.clone(), slide.frame);
                    ui.horizontal_wrapped(|ui| {
                        let label = ui.label(t("Slide title"));
                        ui.add(egui::TextEdit::singleline(&mut slide.title)).labelled_by(label.id);
                        fit = ui.button(t("Fit Slide to Content")).clicked() && touch;
                    });
                    let owner = Some(egui::Id::unique((&editor.owner.document, editor.owner.sheet,
                        editor.owner.revision, editor.owner.viewport, slide.id)));
                    ui.horizontal_wrapped(|ui| {
                        for (salt, label, value, minimum) in [
                            ("slide-x", "X", &mut slide.frame.min.x, -f32::MAX),
                            ("slide-y", "Y", &mut slide.frame.min.y, -f32::MAX),
                            ("slide-width", "Width", &mut slide.frame.size.x, 1.0),
                            ("slide-height", "Height", &mut slide.frame.size.y, 1.0),
                        ] {
                            let label = ui.label(t(label));
                            let mut staged = *value;
                            let (_, commit) = crate::ui::touch_numeric_value(ui, &mut staged, salt, owner, |ui, value| {
                                ui.add(egui::DragValue::new(value).range(minimum..=f32::MAX)).labelled_by(label.id)
                            });
                            if commit { *value = staged; }
                        }
                    });
                    if previous != (slide.title.clone(), slide.frame) {
                        editor.generation += 1;
                    }
                    let count = editor.sheet.presentation.slides.len();
                    ui.horizontal_wrapped(|ui| {
                        if ui.add_enabled(editor.selected > 0, egui::Button::new(t("Move Up"))).clicked() && touch {
                            move_slide = Some(editor.selected - 1);
                        }
                        if ui.add_enabled(editor.selected + 1 < count, egui::Button::new(t("Move Down"))).clicked() && touch {
                            move_slide = Some(editor.selected + 1);
                        }
                        remove = ui.button(t("Remove Slide")).clicked() && touch;
                    });
                    if let Some(target) = move_slide {
                        editor.sheet.presentation.slides.swap(editor.selected, target);
                        editor.selected = target;
                        editor.generation += 1;
                    }
                    if remove {
                        editor.sheet.presentation.slides.remove(editor.selected);
                        editor.selected = editor.selected.min(editor.sheet.presentation.slides.len().saturating_sub(1));
                        editor.generation += 1;
                    }
                }
                if !editor.sheet.presentation.slides.is_empty() {
                    let size = egui::vec2(ui.available_width(), (ctx.content_rect().height() * 0.3).clamp(100.0, 300.0));
                    editor.preview.show(ui, &editor.sheet, editor.selected, editor.generation, &options, size);
                }
                if let Some(error) = &editor.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                let ai_touch = touch && !ime && !editor.composing;
                self.pitch_ai_panel(ui, &mut editor, ai_touch);
                ui.add_enabled_ui(self.pitch.output.is_none() && !editor.sheet.presentation.slides.is_empty(), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if ui.button(t("Export PDF")).clicked() && touch { export = Some(false); }
                        if ui.button(t("Export Slide Images")).clicked() && touch { export = Some(true); }
                    });
                });
            });
            ui.horizontal_wrapped(|ui| {
                apply = ui.button(t("Apply")).clicked() && touch;
                cancel = ui.button(t("Cancel")).clicked() && touch;
            });
        });
        if !ime && !editor.composing {
            if generate {
                match thinoda_io::generate_presentation(&editor.sheet, &options) {
                    Ok(presentation) => {
                        editor.sheet.presentation = presentation;
                        editor.selected = 0;
                        editor.generation += 1;
                        editor.error = None;
                    }
                    Err(error) => editor.error = Some(error.to_string()),
                }
            }
            if add && let Some(source) = source {
                match self
                    .presentation_source_frame(&editor.owner, source)
                    .and_then(|frame| {
                        let title = match source {
                            PresentationSource::Branch(topic) => {
                                editor.sheet.topic(topic).map(|topic| topic.title.as_str())
                            }
                            PresentationSource::Zone(id) => editor
                                .sheet
                                .zones
                                .iter()
                                .find(|zone| zone.id == id)
                                .map(|zone| zone.name.as_str()),
                        }
                        .ok_or_else(|| t("Not possible here").to_string())?
                        .to_string();
                        editor.sheet.add_presentation_slide(&title, source, frame)
                    }) {
                    Ok(_) => {
                        editor.selected = editor.sheet.presentation.slides.len() - 1;
                        editor.generation += 1;
                        editor.error = None;
                    }
                    Err(error) => editor.error = Some(error),
                }
            }
            if fit && let Some(slide) = editor.sheet.presentation.slides.get(editor.selected) {
                match self.presentation_source_frame(&editor.owner, slide.source) {
                    Ok(frame) => {
                        editor.sheet.presentation.slides[editor.selected].frame = frame;
                        editor.generation += 1;
                        editor.error = None;
                    }
                    Err(error) => editor.error = Some(error),
                }
            }
            if let Some(images) = export {
                self.begin_pitch_export(ctx, editor.sheet.clone(), options, images);
            }
            if apply {
                match self.apply_presentation_draft(&editor) {
                    Ok(()) => {
                        ctx.request_repaint();
                        return;
                    }
                    Err(error) => {
                        log::warn!("Presentation draft: {error}");
                        editor.error = Some(error);
                    }
                }
            }
        }
        if !(cancel || response.close_requested) || ime || editor.composing {
            self.pitch.editor = Some(editor);
        }
    }

    pub(crate) fn return_pitch_viewport(&mut self, viewport: egui::ViewportId) {
        if let Some(editor) = &mut self.pitch.editor
            && editor.owner.viewport == viewport
        {
            editor.owner.viewport = egui::ViewportId::ROOT;
        }
    }

    fn begin_pitch_export(
        &mut self,
        ctx: &Context,
        sheet: Sheet,
        options: ExportOptions,
        images: bool,
    ) {
        let extension = if images { "zip" } else { "pdf" };
        let Some(path) = crate::files::save_target(
            Some(t("Export")),
            &[(extension, &[extension])],
            &format!("presentation.{extension}"),
            None,
            crate::files::Target::Download,
        ) else {
            return;
        };
        let (sender, receiver) = mpsc::channel();
        self.pitch.output = Some(receiver);
        let repaint = ctx.clone();
        if let Err(error) =
            thinoda_platform::system::run_background("thinoda-pitch-export", move || {
                let result = Self::pitch_bytes(&sheet, &options, images).and_then(|bytes| {
                    thinoda_platform::fs::atomic_write(&path, &bytes, false)
                        .map_err(|error| error.to_string())?;
                    Ok(path)
                });
                let _ = sender.send(result);
                repaint.request_repaint_of(egui::ViewportId::ROOT);
            })
        {
            self.pitch.output = None;
            self.toasts
                .error(format!("{}: {error}", t("Export failed")));
        }
    }

    fn pitch_bytes(
        sheet: &Sheet,
        options: &ExportOptions,
        images: bool,
    ) -> Result<Vec<u8>, String> {
        if images {
            thinoda_io::export_presentation_png_zip(sheet, options)
        } else {
            thinoda_io::export_presentation_pdf(sheet, options)
        }
        .map_err(crate::doc::export_error)
    }

    fn poll_pitch_export(&mut self, ctx: &Context) {
        let Some(receiver) = &self.pitch.output else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err(t("Export worker stopped unexpectedly").into())
            }
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
                return;
            }
        };
        self.pitch.output = None;
        match result.and_then(|path| {
            crate::files::deliver(&path)?;
            Ok(path)
        }) {
            Ok(_) if crate::files::is_web() => self.toasts.success(t("Download started")),
            Ok(path) => self
                .toasts
                .success(format!("{} {}", t("Exported to"), path.display())),
            Err(error) => self
                .toasts
                .error(format!("{}: {error}", t("Export failed"))),
        };
    }
}
