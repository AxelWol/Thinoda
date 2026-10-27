use super::*;
use thinoda_platform::ai_prompts::{self, SavedPrompt};

fn prompt_error(message: &str) -> String {
    match message {
        "Save at most 32 prompts." => t("Save at most 32 prompts."),
        "Saved prompt IDs must be unique and valid." => {
            t("Saved prompt IDs must be unique and valid.")
        }
        "Enter a prompt name of at most 80 bytes." => t("Enter a prompt name of at most 80 bytes."),
        "Prompt names must be unique." => t("Prompt names must be unique."),
        "Enter a prompt of at most 8192 bytes." => t("Enter a prompt of at most 8192 bytes."),
        "Saved prompts exceed the 128 KiB library limit." => {
            t("Saved prompts exceed the 128 KiB library limit.")
        }
        "Saved prompts changed elsewhere. Cancel and reload prompts." => {
            t("Saved prompts changed elsewhere. Cancel and reload prompts.")
        }
        "Secure legacy credentials in Settings before saving prompts." => {
            t("Secure legacy credentials in Settings before saving prompts.")
        }
        _ => return format!("{}: {message}", t("Could not save prompts")),
    }
    .into()
}

pub struct PromptEditor {
    original: Vec<SavedPrompt>,
    prompt: SavedPrompt,
    existing: bool,
    confirm_delete: bool,
    error: Option<String>,
    viewport: egui::ViewportId,
    composing: bool,
}

impl ThinodaApp {
    pub(super) fn begin_prompt_editor(&mut self, prompt: Option<SavedPrompt>) {
        let existing = prompt.is_some();
        self.ai.prompt_editor = Some(PromptEditor {
            original: self.settings.ai.saved_prompts.clone(),
            prompt: prompt.unwrap_or_else(|| SavedPrompt {
                id: uuid::Uuid::new_v4().simple().to_string(),
                name: String::new(),
                text: String::new(),
            }),
            existing,
            confirm_delete: false,
            error: None,
            viewport: self.current_viewport,
            composing: false,
        });
    }

    pub(super) fn reload_saved_prompts(&mut self) {
        let Some(paths) = &self.services.paths else {
            return;
        };
        match ai_prompts::load(paths) {
            Ok(prompts) => self.settings.ai.saved_prompts = prompts,
            Err(error) => {
                log::warn!("could not reload saved prompts: {error}");
                self.toasts
                    .error(format!("{}: {error}", t("Could not load saved prompts")));
            }
        }
    }

    fn store_saved_prompts(
        &mut self,
        original: &[SavedPrompt],
        prompts: Vec<SavedPrompt>,
    ) -> Result<(), String> {
        ai_prompts::validate(&prompts).map_err(|error| prompt_error(&error))?;
        if self.settings.ai.saved_prompts != original {
            return Err(t("Saved prompts changed elsewhere. Cancel and reload prompts.").into());
        }
        if let Some(paths) = &self.services.paths {
            ai_prompts::save(paths, original, &prompts).map_err(|error| match error {
                thinoda_platform::Error::InvalidAiPrompts(message) => prompt_error(&message),
                other => format!("{}: {other}", t("Could not save prompts")),
            })?;
        }
        self.settings.ai.saved_prompts = prompts;
        Ok(())
    }

    pub(super) fn protect_prompt_editor_ime(&mut self, ctx: &Context) {
        if let Some(editor) = &mut self.ai.prompt_editor
            && editor.viewport == self.current_viewport
        {
            for name in ["saved_prompt_name", "saved_prompt_text"] {
                let id = egui::Id::unique(name);
                if ctx.memory(|memory| memory.has_focus(id)) {
                    thinoda_canvas::protect_ime_candidate_keys(ctx, id, &mut editor.composing);
                    break;
                }
            }
        }
    }

    pub(crate) fn saved_prompt_dialog(&mut self, ctx: &Context) {
        if self
            .ai
            .prompt_editor
            .as_ref()
            .is_none_or(|editor| editor.viewport != self.current_viewport)
        {
            return;
        }
        let mut editor = self.ai.prompt_editor.take().unwrap();
        let allowed = widgets::touch_activation_allowed(ctx);
        let mut save = false;
        let mut delete = false;
        let mut cancel = false;
        let labels = crate::i18n::dialog_labels();
        let response = thinoda_ui::Dialog::new(
            egui::Id::unique("saved_prompt_editor"),
            t("Edit saved prompt"),
        )
        .icon(thinoda_ui::icons::AI)
        .labels(&labels)
        .modal(true)
        .close_on_backdrop(true)
        .default_width(440.0)
        .default_height(520.0)
        .min_size(egui::vec2(320.0, 240.0))
        .scroll(false)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .max_height((ctx.content_rect().height() - 160.0).max(80.0))
                .show(ui, |ui| {
                    widgets::caption(ui, t("Prompts are saved locally in settings. Do not include passwords or secrets."));
                    let name = ui.label(t("Prompt name"));
                    ui.add(egui::TextEdit::singleline(&mut editor.prompt.name)
                        .id(egui::Id::unique("saved_prompt_name"))
                        .char_limit(ai_prompts::MAX_NAME_BYTES)
                        .desired_width(f32::INFINITY)).labelled_by(name.id);
                    let text = ui.label(t("Prompt text"));
                    ui.add(egui::TextEdit::multiline(&mut editor.prompt.text)
                        .id(egui::Id::unique("saved_prompt_text"))
                        .char_limit(ai_prompts::MAX_TEXT_BYTES)
                        .desired_rows(6).desired_width(f32::INFINITY)).labelled_by(text.id);
                    if let Some(error) = &editor.error {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                    if editor.confirm_delete {
                        ui.label(t("Delete this saved prompt?"));
                        ui.horizontal_wrapped(|ui| {
                            delete = ui.button(t("Confirm Delete")).clicked() && allowed;
                            if ui.button(t("Keep prompt")).clicked() && allowed {
                                editor.confirm_delete = false;
                            }
                        });
                    }
                });
            ui.horizontal_wrapped(|ui| {
                save = ui.add_enabled(!editor.confirm_delete && !editor.composing, egui::Button::new(t("Save prompt"))).clicked() && allowed;
                cancel = ui.button(t("Cancel")).clicked() && allowed;
                if editor.existing && ui.button(t("Delete prompt")).clicked() && allowed {
                    editor.confirm_delete = true;
                }
            });
        });
        if cancel || (response.close_requested && allowed && !editor.composing) {
            return;
        }
        if save || delete {
            let mut prompts = editor.original.clone();
            if delete {
                prompts.retain(|prompt| prompt.id != editor.prompt.id);
            } else {
                editor.prompt.name = editor.prompt.name.trim().into();
                if let Some(prompt) = prompts
                    .iter_mut()
                    .find(|prompt| prompt.id == editor.prompt.id)
                {
                    *prompt = editor.prompt.clone();
                } else {
                    prompts.push(editor.prompt.clone());
                }
            }
            match self.store_saved_prompts(&editor.original, prompts) {
                Ok(()) => return,
                Err(error) => {
                    log::warn!("could not save prompt library: {error}");
                    editor.error = Some(error);
                }
            }
        }
        self.ai.prompt_editor = Some(editor);
    }

    pub(super) fn return_prompt_editor_viewport(&mut self, viewport: egui::ViewportId) {
        if let Some(editor) = &mut self.ai.prompt_editor
            && editor.viewport == viewport
        {
            editor.viewport = egui::ViewportId::ROOT;
        }
    }
}
