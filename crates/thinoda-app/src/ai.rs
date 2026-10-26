//! AI assistant integration: commands, background jobs, applying results to the map,
//! the "Generate Map" prompt dialog and the AI section of the settings.

use egui::{Context, RichText, Ui};
use thinoda_ai::{
    AiConfig, AiError, AiJob, AiOutput, AiResult, AiStreamEvent, AiStreamJob, AiTask, OutputKind,
    TopicContext,
};
use thinoda_core::outline::{OutlineNode as CoreNode, add_outline, sheet_from_outline};
use thinoda_core::{Sheet, TopicId, Workbook};
use thinoda_platform::settings::AiSettings;
use thinoda_ui::widgets;

use crate::app::ThinodaApp;
use crate::i18n::t;

mod chat;
mod prompts;
mod review;
mod semantic;
#[cfg(test)]
mod transform_tests;
pub use chat::ChatState;
pub use review::{AiReview, AiTarget};

/// AI commands (XMind AI equivalents).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiCommand {
    /// New subtopic ideas for the selected topic.
    Brainstorm,
    /// Actionable to-do subtopics for the selected topic.
    Breakdown,
    /// Short explanation stored as the topic's note.
    Explain,
    /// Summary of the selected branch stored as the topic's note.
    Summarize,
    /// Opens the prompt dialog to create a whole new map.
    GenerateMap,
    Rewrite,
    Translate,
    Grow,
    Reorganize,
    Refine,
}

impl AiCommand {
    pub const ALL: [AiCommand; 10] = [
        AiCommand::Brainstorm,
        AiCommand::Breakdown,
        AiCommand::Explain,
        AiCommand::Summarize,
        AiCommand::GenerateMap,
        AiCommand::Rewrite,
        AiCommand::Translate,
        AiCommand::Grow,
        AiCommand::Reorganize,
        AiCommand::Refine,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AiCommand::Brainstorm => "AI: Brainstorm Subtopics",
            AiCommand::Breakdown => "AI: Break Down into To-Dos",
            AiCommand::Explain => "AI: Explain Topic",
            AiCommand::Summarize => "AI: Summarize Branch",
            AiCommand::GenerateMap => "AI: Generate Map…",
            AiCommand::Rewrite => "AI: Rewrite Topic…",
            AiCommand::Translate => "AI: Translate Branch…",
            AiCommand::Grow => "AI: Grow Ideas…",
            AiCommand::Reorganize => "AI: Reorganize Branch…",
            AiCommand::Refine => "AI: Refine Branch…",
        }
    }

    pub fn needs_topic(self) -> bool {
        self != AiCommand::GenerateMap
    }

    fn transformation(self) -> Option<thinoda_ai::TransformAction> {
        use thinoda_ai::TransformAction as T;
        match self {
            Self::Rewrite => Some(T::Rewrite),
            Self::Translate => Some(T::Translate),
            Self::Grow => Some(T::Grow),
            Self::Reorganize => Some(T::Reorganize),
            Self::Refine => Some(T::Refine),
            _ => None,
        }
    }

    fn output_kind(self) -> OutputKind {
        match self {
            Self::Brainstorm | Self::Grow => OutputKind::Subtopics,
            Self::Breakdown => OutputKind::Todos,
            Self::Explain | Self::Summarize => OutputKind::Note,
            Self::GenerateMap => OutputKind::Map,
            Self::Rewrite => OutputKind::Title,
            Self::Translate | Self::Reorganize | Self::Refine => OutputKind::Sheet,
        }
    }
}

/// A running AI request and where its result goes.
pub struct PendingAi {
    pub job: AiStreamJob<AiResult>,
    pub target: AiTarget,
    pub preview: String,
    pub started: web_time::Instant,
}

#[derive(Default)]
pub struct AiState {
    pub pending: Option<PendingAi>,
    pub review: Option<AiReview>,
    pub chat: ChatState,
    pub prompt_editor: Option<prompts::PromptEditor>,
    pub semantic: semantic::SemanticState,
    /// Text of the open "Generate Map" dialog.
    pub prompt: Option<String>,
    prompt_viewport: Option<egui::ViewportId>,
    prompt_composing: bool,
    prompted_target: Option<(AiTarget, String)>,
    /// Unsaved API key text; credentials are committed only on explicit save.
    pub api_key_draft: Option<String>,
    /// Running "Test connection" request.
    pub test: Option<AiJob<Vec<String>>>,
    pub test_result: Option<Result<Vec<String>, String>>,
    test_owner: Option<(String, String)>,
}

pub fn config(s: &AiSettings) -> AiConfig {
    AiConfig {
        base_url: s.base_url.clone(),
        model: s.model.clone(),
        api_key: if s.credentials_ready {
            s.api_key.clone()
        } else {
            String::new()
        },
        temperature: s.temperature,
        reply_language: crate::i18n::lang().english_name().into(),
        ..AiConfig::default()
    }
}

fn config_with_api_key(s: &AiSettings, api_key: &str) -> AiConfig {
    let mut config = config(s);
    config.api_key = if !s.credentials_ready && api_key == s.api_key {
        String::new()
    } else {
        api_key.to_owned()
    };
    config
}

/// Topic path, children and subtree used as model context.
pub fn topic_context(sheet: &Sheet, id: TopicId) -> TopicContext {
    let title = |id: TopicId| {
        sheet
            .topic(id)
            .map(|t| t.title.replace('\n', " "))
            .unwrap_or_default()
    };
    let mut path: Vec<String> = sheet.ancestors(id).into_iter().rev().map(title).collect();
    path.push(title(id));
    fn walk(sheet: &Sheet, id: TopicId, depth: usize) -> Vec<thinoda_ai::OutlineNode> {
        if depth > 6 {
            return Vec::new();
        }
        sheet
            .children(id)
            .iter()
            .map(|c| thinoda_ai::OutlineNode {
                title: sheet
                    .topic(*c)
                    .map(|t| t.title.replace('\n', " "))
                    .unwrap_or_default(),
                children: walk(sheet, *c, depth + 1),
            })
            .collect()
    }
    TopicContext {
        path,
        children: sheet.children(id).iter().map(|c| title(*c)).collect(),
        subtree: walk(sheet, id, 0),
    }
}

fn to_core(n: &thinoda_ai::OutlineNode) -> CoreNode {
    CoreNode::new(n.title.clone()).with_children(n.children.iter().map(to_core).collect())
}

/// Builds a new workbook from a generated outline.
pub fn workbook_from_outline(o: &thinoda_ai::Outline) -> Workbook {
    let title = o
        .title
        .clone()
        .unwrap_or_else(|| t("Central Topic").to_string());
    let root = CoreNode::new(title.clone()).with_children(o.items.iter().map(to_core).collect());
    Workbook::new(sheet_from_outline(&title, &root))
}

fn error_text(e: &AiError) -> String {
    match e {
        AiError::Http(_) => format!(
            "{} ({e})",
            t("AI server not reachable — check Settings › AI")
        ),
        _ => e.to_string(),
    }
}

pub(crate) fn poll_stream<T>(
    job: &AiStreamJob<T>,
    preview: &mut String,
) -> (bool, Option<Result<T, AiError>>) {
    let mut progress = false;
    for _ in 0..128 {
        match job.poll() {
            Some(AiStreamEvent::Delta(text)) => {
                preview.push_str(&text);
                progress = true;
            }
            Some(AiStreamEvent::Finished(result)) => return (progress, Some(result)),
            None => break,
        }
    }
    (progress, None)
}

impl ThinodaApp {
    pub(crate) fn protect_ai_prompt_input(&mut self, ctx: &Context) {
        self.protect_prompt_editor_ime(ctx);
        self.protect_semantic_ime(ctx);
        self.protect_ai_chat_input(ctx);
        if self
            .ai
            .prompt_viewport
            .is_some_and(|owner| owner != self.current_viewport)
        {
            return;
        }
        if self.ai.prompt.is_some() {
            let id = egui::Id::unique("thinoda_ai_prompt");
            if self.ai.prompt_composing || ctx.memory(|memory| memory.has_focus(id)) {
                thinoda_canvas::protect_ime_candidate_keys(ctx, id, &mut self.ai.prompt_composing);
            }
            if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                self.ai.prompt = None;
                self.ai.prompted_target = None;
                self.ai.prompt_composing = false;
            }
        } else {
            self.ai.prompt_composing = false;
        }
    }

    /// Starts an AI command for the current selection.
    pub fn start_ai(&mut self, cmd: AiCommand) {
        if !self.settings.ai.enabled {
            self.toasts
                .warning(t("AI assistant is disabled in Settings"));
            return;
        }
        if self.ai.pending.is_some() || self.ai.chat.is_busy() || self.pitch.ai_busy() {
            self.toasts.info(t("AI is still working…"));
            return;
        }
        if self.ai.review.is_some() {
            self.toasts.info(t("Review the current AI result first."));
            return;
        }
        if cmd == AiCommand::GenerateMap {
            self.ai.prompted_target = None;
            self.ai.prompt.get_or_insert_with(String::new);
            self.ai.prompt_viewport = Some(self.current_viewport);
            return;
        }
        if let Some(d) = self.doc_mut() {
            d.view.commit_edit(&mut d.doc);
        }
        let Some(d) = self.doc() else { return };
        let Some(id) = d.view.primary() else {
            self.toasts.warning(t("Select a topic first"));
            return;
        };
        if cmd.transformation().is_some() {
            let source = match chat::context(d, chat::Scope::Selection) {
                Ok(source) => source,
                Err(error) => {
                    self.toasts.warning(error);
                    return;
                }
            };
            let source = format!("Primary selected topic ID: {id:?}\n{source}");
            self.ai.prompted_target = Some((
                AiTarget {
                    command: cmd,
                    doc_key: Some(d.key.clone()),
                    topic: Some(id),
                    source: Some((d.doc.active_index(), d.doc.sheet().root, d.doc.state_id())),
                    viewport: self.current_viewport,
                    config: config(&self.settings.ai),
                },
                source,
            ));
            self.ai.prompt = Some(String::new());
            self.ai.prompt_viewport = Some(self.current_viewport);
            return;
        }
        let ctx = topic_context(d.doc.sheet(), id);
        let task = match cmd {
            AiCommand::Brainstorm => AiTask::Brainstorm(ctx),
            AiCommand::Breakdown => AiTask::Breakdown(ctx),
            AiCommand::Explain => AiTask::Explain(ctx),
            AiCommand::Summarize => AiTask::Summarize(ctx),
            _ => unreachable!(),
        };
        let doc_key = Some(d.key.clone());
        let source = Some((d.doc.active_index(), d.doc.sheet().root, d.doc.state_id()));
        self.ai.pending = Some(PendingAi {
            job: AiStreamJob::spawn(config(&self.settings.ai), task),
            target: AiTarget {
                command: cmd,
                doc_key,
                topic: Some(id),
                source,
                viewport: self.current_viewport,
                config: config(&self.settings.ai),
            },
            preview: String::new(),
            started: web_time::Instant::now(),
        });
    }

    /// Starts generating a new map from a free-text prompt.
    pub fn start_generate_map(&mut self, prompt: String) {
        if !self.settings.ai.enabled {
            self.toasts
                .warning(t("AI assistant is disabled in Settings"));
            return;
        }

        if prompt.trim().is_empty()
            || self.ai.pending.is_some()
            || self.ai.review.is_some()
            || self.ai.chat.is_busy()
            || self.pitch.ai_busy()
        {
            return;
        }
        self.ai.pending = Some(PendingAi {
            job: AiStreamJob::spawn(config(&self.settings.ai), AiTask::GenerateMap(prompt)),
            target: AiTarget {
                command: AiCommand::GenerateMap,
                doc_key: None,
                topic: None,
                source: None,
                viewport: self.current_viewport,
                config: config(&self.settings.ai),
            },
            preview: String::new(),
            started: web_time::Instant::now(),
        });
    }

    fn start_prompted_ai(&mut self, instruction: &str) -> Result<(), String> {
        if self.ai.pending.is_some()
            || self.ai.review.is_some()
            || self.ai.chat.is_busy()
            || self.pitch.ai_busy()
        {
            return Err(t("Review the current AI result first.").into());
        }
        if instruction.trim().is_empty() || instruction.len() > 8192 {
            return Err(t("Enter a prompt of at most 8192 bytes.").into());
        }
        let Some((target, _)) = &self.ai.prompted_target else {
            return Err(t("The document changed. Try again.").into());
        };
        if target.viewport != self.current_viewport {
            return Err(t("Not possible here").into());
        }
        self.validate_ai_target(target).map_err(str::to_owned)?;
        let (target, source) = self.ai.prompted_target.take().unwrap();
        let task = AiTask::Transform {
            action: target.command.transformation().unwrap(),
            source,
            instruction: instruction.into(),
        };
        self.ai.pending = Some(PendingAi {
            job: AiStreamJob::spawn(config(&self.settings.ai), task),
            target,
            preview: String::new(),
            started: web_time::Instant::now(),
        });
        Ok(())
    }

    /// Polls background AI work; call every frame.
    pub fn poll_ai(&mut self, ctx: &Context) {
        self.poll_pitch_ai(ctx);
        self.poll_semantic(ctx);
        self.poll_ai_chat(ctx);
        if !self.settings.ai.enabled && self.ai.pending.is_some() {
            self.cancel_ai();
            self.toasts
                .warning(t("AI assistant is disabled in Settings"));
        }
        if let Some(owner) = &self.ai.test_owner {
            let draft = self
                .ai
                .api_key_draft
                .as_deref()
                .unwrap_or(&self.settings.ai.api_key);
            let config = config_with_api_key(&self.settings.ai, draft);
            if !self.settings.ai.enabled || owner != &(config.base_url, config.api_key) {
                self.ai.test = None;
                self.ai.test_result = None;
                self.ai.test_owner = None;
                self.toasts
                    .warning(t("AI settings changed. Test the connection again."));
            }
        }
        if let Some(job) = &self.ai.test {
            if let Some(r) = job.poll() {
                self.ai.test_result = Some(r.map_err(|e| error_text(&e)));
                self.ai.test = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
        let Some(p) = &mut self.ai.pending else {
            return;
        };
        let (progress, finished) = poll_stream(&p.job, &mut p.preview);
        if progress {
            ctx.request_repaint_of(p.target.viewport);
        }
        let Some(result) = finished else {
            ctx.request_repaint_after(std::time::Duration::from_millis(if progress {
                16
            } else {
                100
            }));
            return;
        };
        let p = self.ai.pending.take().unwrap();
        ctx.request_repaint_of(p.target.viewport);
        match result {
            Ok(result) => {
                self.ai.review = Some(AiReview {
                    target: p.target,
                    result,
                });
                ctx.request_repaint();
            }
            Err(e) => {
                self.toasts.error(format!(
                    "{}: {}",
                    t(p.target.command.label()),
                    error_text(&e)
                ));
            }
        }
    }

    /// Cancels transport and discards partial output without applying it.
    pub fn cancel_ai(&mut self) {
        if let Some(pending) = self.ai.pending.take() {
            pending.job.cancel();
        }
    }

    /// Applies a finished result (one undo step).
    pub fn apply_ai(&mut self, p: &AiTarget, r: &AiResult) -> Result<(), String> {
        if self.current_viewport != p.viewport {
            return Err(t("Not possible here").to_owned());
        }
        self.validate_ai_target(p).map_err(str::to_owned)?;
        if r.kind != p.command.output_kind()
            || matches!(r.output, AiOutput::Text(_))
                != matches!(r.kind, OutputKind::Note | OutputKind::Title)
        {
            return Err(t("The AI response has an unexpected format.").to_owned());
        }
        if r.kind == OutputKind::Map {
            if let AiOutput::Outline(o) = &r.output {
                self.new_doc(workbook_from_outline(o));
                if let Some(d) = self.doc_mut() {
                    d.doc.mark_dirty();
                }
                self.toasts.info(t("AI map created"));
                return Ok(());
            }
            return Err(t("The AI response has an unexpected format.").to_owned());
        }
        let (Some(key), Some(topic)) = (&p.doc_key, p.topic) else {
            return Err(t("The topic no longer exists").to_owned());
        };
        let Some(i) = self.docs.iter().position(|d| &d.key == key) else {
            return Err(t("The topic no longer exists").to_owned());
        };
        let d = &mut self.docs[i];
        let label = t(p.command.label());
        if r.kind == OutputKind::Sheet {
            let AiOutput::Outline(outline) = &r.output else {
                unreachable!()
            };
            let mut workbook = workbook_from_outline(outline);
            let sheet = workbook.sheets.remove(0);
            d.doc.add_sheet(sheet);
            d.view.reset();
            return Ok(());
        }
        let res = match (&r.output, r.kind) {
            (AiOutput::Outline(o), kind) => {
                let todo = kind == OutputKind::Todos;
                d.doc.edit_sheet(label, |s| {
                    s.set_collapsed(topic, false)?;
                    for n in &o.items {
                        let id = add_outline(s, topic, &to_core(n));
                        if todo {
                            s.set_todo(id, Some(false))?;
                        }
                    }
                    Ok(())
                })
            }
            (AiOutput::Text(text), OutputKind::Title) => {
                if text.trim().is_empty() || text.len() > 1024 || text.lines().count() != 1 {
                    return Err(t("The AI response has an unexpected format.").into());
                }
                d.doc.edit_sheet(label, |s| s.set_title(topic, text))
            }
            (AiOutput::Text(text), _) => {
                let note = match d
                    .doc
                    .sheet()
                    .topic(topic)
                    .and_then(|t| t.notes.clone())
                    .filter(|n| !n.trim().is_empty())
                {
                    Some(old) => format!("{old}\n\n{text}"),
                    None => text.clone(),
                };
                d.doc.edit_sheet(label, |s| s.set_notes(topic, Some(note)))
            }
        };
        match res {
            Ok(()) => {
                if i == self.active {
                    self.docs[i].view.reveal(topic);
                }
                self.toasts.info(format!("{} ✔", label));
                Ok(())
            }
            Err(e) => Err(format!("{label}: {e}")),
        }
    }

    /// "Generate Map" prompt dialog.
    pub fn ai_prompt_dialog(&mut self, ctx: &Context) {
        if self
            .ai
            .prompt_viewport
            .is_some_and(|owner| owner != self.current_viewport)
        {
            return;
        }
        let Some(mut text) = self.ai.prompt.take() else {
            return;
        };
        let mut open = true;
        let mut go = false;
        let touch_allowed = widgets::touch_activation_allowed(ctx);
        let escape =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        let command = self
            .ai
            .prompted_target
            .as_ref()
            .map(|(target, _)| target.command);
        egui::Window::new(command.map_or(t("Generate Map with AI"), |cmd| t(cmd.label())))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .max_height((ctx.content_rect().height() - 32.0).max(80.0))
            .vscroll(true)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                let l = ui.label(if command == Some(AiCommand::Translate) {
                    t("Target language and translation instructions")
                } else if command.is_some() {
                    t("Instructions for this transformation")
                } else {
                    t("Describe the topic of your mind map:")
                });
                if let Some((target, source)) = &self.ai.prompted_target {
                    widgets::caption(
                        ui,
                        t("Only titles and hierarchy are sent. Notes and attachments stay local."),
                    );
                    if target.command.output_kind() == OutputKind::Sheet {
                        widgets::caption(
                            ui,
                            t("Apply creates a new sheet. The source stays unchanged."),
                        );
                    }
                    if text.len() > 8192 {
                        ui.colored_label(
                            ui.visuals().error_fg_color,
                            t("Enter a prompt of at most 8192 bytes."),
                        );
                    }
                    egui::CollapsingHeader::new(t("Context to send")).show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(120.0)
                            .show(ui, |ui| {
                                ui.add(
                                    egui::TextEdit::multiline(&mut source.as_str())
                                        .desired_width(f32::INFINITY),
                                );
                            });
                    });
                    if let Err(error) = self.validate_ai_target(target) {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                }
                let r = ui.add(
                    egui::TextEdit::multiline(&mut text)
                        .id(egui::Id::unique("thinoda_ai_prompt"))
                        .hint_text(t("e.g. Launch plan for a mobile app"))
                        .char_limit(8192)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY),
                );
                let r = r.labelled_by(l.id);
                if !r.has_focus() && text.is_empty() {
                    r.request_focus();
                }
                let enter = r.has_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.command);
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if (ui
                        .add_enabled(
                            self.settings.ai.enabled
                                && self.ai.pending.is_none()
                                && self.ai.review.is_none()
                                && !self.ai.chat.is_busy()
                                && text.len() <= 8192
                                && self.ai.prompted_target.as_ref().is_none_or(|(target, _)| {
                                    self.validate_ai_target(target).is_ok()
                                })
                                && !text.trim().is_empty(),
                            egui::Button::new(if command.is_some() {
                                t("Send")
                            } else {
                                t("Generate")
                            }),
                        )
                        .clicked()
                        || (enter
                            && self.settings.ai.enabled
                            && self.ai.pending.is_none()
                            && self.ai.review.is_none()
                            && !self.ai.chat.is_busy()))
                        && touch_allowed
                    {
                        go = true;
                    }
                    widgets::caption(
                        ui,
                        format!("{}: {}", t("Server"), self.settings.ai.base_url),
                    );
                });
            });
        if go {
            if command.is_some() {
                if let Err(error) = self.start_prompted_ai(&text) {
                    self.toasts.error(error);
                    self.ai.prompt = Some(text);
                }
            } else {
                self.start_generate_map(text);
            }
        } else if (open || !touch_allowed) && !escape {
            self.ai.prompt = Some(text);
        } else {
            self.ai.prompted_target = None;
        }
    }

    /// Nonmodal streaming preview; closing it cancels the request.
    pub fn ai_busy_overlay(&mut self, ctx: &Context) {
        self.ai_stream_window(ctx);
    }

    /// AI section of the settings window.
    pub fn ai_settings_section(&mut self, ui: &mut Ui, activation_allowed: bool) -> bool {
        let mut api_key_draft = self
            .ai
            .api_key_draft
            .clone()
            .unwrap_or_else(|| self.settings.ai.api_key.clone());
        let mut save_api_key = false;
        let mut clear_api_key = false;
        let mut credentials_saved = false;
        widgets::section(ui, t("AI Assistant"), false, |ui| {
            let ai = &mut self.settings.ai;
            widgets::property_field(ui, t("Enable AI assistant"), |ui| {
                widgets::toggle(ui, &mut ai.enabled, t("Enable AI assistant"))
            });
            widgets::caption(
                ui,
                t("Any OpenAI-compatible server: LM Studio, Ollama, llama.cpp, OpenAI…"),
            );
            widgets::property_field(ui, t("Server URL"), |ui| {
                ui.text_edit_singleline(&mut ai.base_url)
            });
            widgets::property_field(ui, t("Model"), |ui| {
                ui.add(egui::TextEdit::singleline(&mut ai.model).hint_text(t("automatic")))
            });
            widgets::property_field(ui, t("Embedding model"), |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut ai.embedding_model)
                        .hint_text(t("Required for semantic tools")),
                )
            });
            let key_label = ui.label(t("API Key"));
            ui.horizontal_wrapped(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut api_key_draft)
                        .password(true)
                        .hint_text(thinoda_ai::API_KEY_ENV),
                )
                .labelled_by(key_label.id);
                if ui.button(t("Save API Key")).clicked() && activation_allowed {
                    save_api_key = true;
                }
                if ui.button(t("Clear")).clicked() && activation_allowed {
                    api_key_draft.clear();
                    clear_api_key = true;
                }
            });
            widgets::caption(
                ui,
                t("API keys are stored in your operating system's secure credential store."),
            );
            if !ai.credentials_ready {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    t("Secure credential storage is unavailable. Save a key to retry, or clear the saved key."),
                );
            }
            widgets::property_field(ui, t("Creativity"), |ui| {
                ui.add(egui::Slider::new(&mut ai.temperature, 0.0..=1.5))
            });
            ui.horizontal(|ui| {
                let busy = self.ai.test.is_some();
                if ui
                    .add_enabled(
                        !busy && self.settings.ai.enabled,
                        egui::Button::new(t("Test Connection")),
                    )
                    .clicked()
                    && activation_allowed
                {
                    self.ai.test_result = None;
                    let config = config_with_api_key(&self.settings.ai, &api_key_draft);
                    self.ai.test_owner = Some((config.base_url.clone(), config.api_key.clone()));
                    self.ai.test = Some(AiJob::models(config));
                }
                if busy {
                    ui.spinner();
                }
            });
            match &self.ai.test_result {
                Some(Ok(models)) => {
                    ui.label(
                        RichText::new(format!("✔ {} ({})", t("Connected"), models.len()))
                            .color(ui.visuals().widgets.active.bg_fill),
                    );
                    let mut pick = None;
                    ui.horizontal_wrapped(|ui| {
                        for m in models.iter().filter(|m| !m.contains("embed")) {
                            if ui
                                .selectable_label(self.settings.ai.model == *m, m)
                                .clicked()
                            {
                                pick = Some(m.clone());
                            }
                        }
                    });
                    if let Some(m) = pick {
                        self.settings.ai.model = m;
                    }
                }
                Some(Err(e)) => {
                    ui.colored_label(ui.visuals().error_fg_color, e);
                }
                None => {}
            }
        });
        self.ai.api_key_draft = Some(api_key_draft.clone());
        if save_api_key || clear_api_key {
            let mut settings = self.settings.clone();
            settings.ai.set_api_key(if clear_api_key {
                String::new()
            } else {
                api_key_draft
            });
            if clear_api_key {
                settings.ai.credentials_ready = true;
            }
            if let Some(paths) = &self.services.paths {
                match settings.save_with_credentials(
                    paths,
                    &thinoda_platform::credentials::NativeCredentialStore,
                ) {
                    Ok(()) => {
                        self.settings = settings;
                        self.ai.api_key_draft = Some(self.settings.ai.api_key.clone());
                        credentials_saved = true;
                    }
                    Err(error) => {
                        self.toasts
                            .error(format!("{}: {error}", t("Could not save API key securely")));
                    }
                }
            } else {
                self.toasts
                    .error(t("Secure credential storage is unavailable"));
            }
        }
        credentials_saved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn finished_ai_does_not_overwrite_an_uncommitted_inspector_draft() {
        use crate::doc::{InspectorEdit, InspectorField};
        for field in [InspectorField::Notes, InspectorField::Hyperlink] {
            let mut app = ThinodaApp::headless();
            let request_config = config(&app.settings.ai);
            let d = app.doc_mut().unwrap();
            let root = d.doc.sheet().root;
            let pending = AiTarget {
                command: AiCommand::Explain,
                doc_key: Some(d.key.clone()),
                topic: Some(root),
                source: Some((d.doc.active_index(), root, d.doc.state_id())),
                viewport: egui::ViewportId::ROOT,
                config: request_config,
            };
            let draft = InspectorEdit {
                buffer_id: None,
                sheet_index: d.doc.active_index(),
                sheet_root: root,
                topic: root,
                field,
                original: None,
                text: "Local inspector draft".into(),
            };
            d.inspector_edit = Some(draft.clone());
            let before = (d.doc.workbook().clone(), d.doc.state_id());
            assert!(
                app.apply_ai(
                    &pending,
                    &AiResult {
                        kind: OutputKind::Note,
                        output: AiOutput::Text("Delayed AI note".into()),
                        model: "local-mock".into(),
                    },
                )
                .is_err()
            );
            let d = app.doc().unwrap();
            assert_eq!((d.doc.workbook().clone(), d.doc.state_id()), before);
            assert_eq!(d.inspector_edit.as_ref(), Some(&draft));
        }
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn connection_results_are_bound_to_endpoint_and_credential_draft() {
        for change in ["endpoint", "key", "disabled", "model", "unchanged"] {
            let mut app = ThinodaApp::headless();
            app.ai.api_key_draft = Some("local-test-key".into());
            app.ai.test_owner = Some((app.settings.ai.base_url.clone(), "local-test-key".into()));
            app.ai.test_result = Some(Ok(vec!["old-model".into()]));
            app.ai.test = Some(AiJob::spawn_with(AiConfig::default(), |_| {
                Ok(vec!["completed-model".into()])
            }));
            match change {
                "endpoint" => app.settings.ai.base_url = "http://127.0.0.1:9/v1".into(),
                "key" => app.ai.api_key_draft = Some("new-local-test-key".into()),
                "disabled" => app.settings.ai.enabled = false,
                "model" => app.settings.ai.model = "chosen-model".into(),
                "unchanged" => {}
                _ => unreachable!(),
            }
            app.poll_ai(&Context::default());
            if matches!(change, "endpoint" | "key" | "disabled") {
                assert!(app.ai.test.is_none(), "{change}");
                assert!(app.ai.test_result.is_none(), "{change}");
                assert!(app.ai.test_owner.is_none(), "{change}");
            } else {
                assert!(app.ai.test_owner.is_some(), "{change}");
                assert!(app.ai.test_result.as_ref().unwrap().is_ok(), "{change}");
            }
        }
    }

    #[test]
    fn context_contains_path_children_and_subtree() {
        let mut wb = Workbook::default();
        let s = &mut wb.sheets[0];
        let root = s.root;
        let a = s.insert_child(root, "A").unwrap();
        let a1 = s.insert_child(a, "A1").unwrap();
        s.insert_child(a1, "A1x").unwrap();
        let c = topic_context(s, a);
        assert_eq!(c.path.last().unwrap(), "A");
        assert_eq!(c.path.len(), 2);
        assert_eq!(c.children, ["A1"]);
        assert_eq!(c.subtree[0].children[0].title, "A1x");
    }

    #[test]
    fn outline_to_workbook() {
        let o = thinoda_ai::parse_outline("# Trip\n- Plan\n  - Budget\n- Go").into_map();
        let wb = workbook_from_outline(&o);
        let s = &wb.sheets[0];
        assert_eq!(s.topic(s.root).unwrap().title, "Trip");
        assert_eq!(s.children(s.root).len(), 2);
        assert_eq!(s.children(s.children(s.root)[0]).len(), 1);
    }

    #[test]
    fn unprotected_api_keys_are_not_used_by_ai_commands() {
        let settings = AiSettings {
            api_key: "legacy-secret".into(),
            credentials_ready: false,
            ..AiSettings::default()
        };
        assert!(config(&settings).api_key.is_empty());
        assert!(
            config_with_api_key(&settings, "legacy-secret")
                .api_key
                .is_empty()
        );
        assert_eq!(
            config_with_api_key(&settings, "draft-secret").api_key,
            "draft-secret"
        );
    }
}
