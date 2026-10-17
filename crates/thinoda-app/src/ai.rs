//! AI assistant integration: commands, background jobs, applying results to the map,
//! the "Generate Map" prompt dialog and the AI section of the settings.

use egui::{Context, RichText, Ui};
use thinoda_ai::{AiConfig, AiError, AiJob, AiOutput, AiResult, AiTask, OutputKind, TopicContext};
use thinoda_core::outline::{OutlineNode as CoreNode, add_outline, sheet_from_outline};
use thinoda_core::{Sheet, TopicId, Workbook};
use thinoda_platform::settings::AiSettings;
use thinoda_ui::widgets;

use crate::app::ThinodaApp;
use crate::i18n::t;

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
}

impl AiCommand {
    pub const ALL: [AiCommand; 5] = [
        AiCommand::Brainstorm,
        AiCommand::Breakdown,
        AiCommand::Explain,
        AiCommand::Summarize,
        AiCommand::GenerateMap,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AiCommand::Brainstorm => "AI: Brainstorm Subtopics",
            AiCommand::Breakdown => "AI: Break Down into To-Dos",
            AiCommand::Explain => "AI: Explain Topic",
            AiCommand::Summarize => "AI: Summarize Branch",
            AiCommand::GenerateMap => "AI: Generate Map…",
        }
    }

    pub fn needs_topic(self) -> bool {
        self != AiCommand::GenerateMap
    }
}

/// A running AI request and where its result goes.
pub struct PendingAi {
    pub job: AiJob,
    pub command: AiCommand,
    /// Recovery key of the target document (`None` for new maps).
    pub doc_key: Option<String>,
    pub topic: Option<TopicId>,
}

#[derive(Default)]
pub struct AiState {
    pub pending: Option<PendingAi>,
    /// Text of the open "Generate Map" dialog.
    pub prompt: Option<String>,
    /// Running "Test connection" request.
    pub test: Option<AiJob<Vec<String>>>,
    pub test_result: Option<Result<Vec<String>, String>>,
}

pub fn config(s: &AiSettings) -> AiConfig {
    AiConfig {
        base_url: s.base_url.clone(),
        model: s.model.clone(),
        api_key: s.api_key.clone(),
        temperature: s.temperature,
        ..AiConfig::default()
    }
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

impl ThinodaApp {
    /// Starts an AI command for the current selection.
    pub fn start_ai(&mut self, cmd: AiCommand) {
        if !self.settings.ai.enabled {
            self.toasts
                .warning(t("AI assistant is disabled in Settings"));
            return;
        }
        if self.ai.pending.is_some() {
            self.toasts.info(t("AI is still working…"));
            return;
        }
        if cmd == AiCommand::GenerateMap {
            self.ai.prompt.get_or_insert_with(String::new);
            return;
        }
        let Some(d) = self.doc() else { return };
        let Some(id) = d.view.primary() else {
            self.toasts.warning(t("Select a topic first"));
            return;
        };
        let ctx = topic_context(d.doc.sheet(), id);
        let task = match cmd {
            AiCommand::Brainstorm => AiTask::Brainstorm(ctx),
            AiCommand::Breakdown => AiTask::Breakdown(ctx),
            AiCommand::Explain => AiTask::Explain(ctx),
            AiCommand::Summarize => AiTask::Summarize(ctx),
            AiCommand::GenerateMap => unreachable!(),
        };
        let doc_key = Some(d.key.clone());
        self.ai.pending = Some(PendingAi {
            job: AiJob::spawn(config(&self.settings.ai), task),
            command: cmd,
            doc_key,
            topic: Some(id),
        });
    }

    /// Starts generating a new map from a free-text prompt.
    pub fn start_generate_map(&mut self, prompt: String) {
        if prompt.trim().is_empty() || self.ai.pending.is_some() {
            return;
        }
        self.ai.pending = Some(PendingAi {
            job: AiJob::spawn(config(&self.settings.ai), AiTask::GenerateMap(prompt)),
            command: AiCommand::GenerateMap,
            doc_key: None,
            topic: None,
        });
    }

    /// Polls background AI work; call every frame.
    pub fn poll_ai(&mut self, ctx: &Context) {
        if let Some(job) = &self.ai.test {
            if let Some(r) = job.poll() {
                self.ai.test_result = Some(r.map_err(|e| error_text(&e)));
                self.ai.test = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
        let Some(p) = &self.ai.pending else { return };
        let Some(result) = p.job.poll() else {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            return;
        };
        let p = self.ai.pending.take().unwrap();
        match result {
            Ok(r) => self.apply_ai(&p, r),
            Err(e) => {
                self.toasts
                    .error(format!("{}: {}", t(p.command.label()), error_text(&e)));
            }
        }
    }

    /// Applies a finished result (one undo step).
    pub fn apply_ai(&mut self, p: &PendingAi, r: AiResult) {
        if r.kind == OutputKind::Map {
            if let AiOutput::Outline(o) = &r.output {
                self.new_doc(workbook_from_outline(o));
                if let Some(d) = self.doc_mut() {
                    d.doc.mark_dirty();
                }
                self.toasts.info(t("AI map created"));
            }
            return;
        }
        let (Some(key), Some(topic)) = (&p.doc_key, p.topic) else {
            return;
        };
        let Some(i) = self.docs.iter().position(|d| &d.key == key) else {
            return;
        };
        let d = &mut self.docs[i];
        if d.doc.sheet().topic(topic).is_none() {
            self.toasts.warning(t("The topic no longer exists"));
            return;
        }
        let label = t(p.command.label());
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
            }
            Err(e) => {
                self.toasts.error(format!("{label}: {e}"));
            }
        }
    }

    /// "Generate Map" prompt dialog.
    pub fn ai_prompt_dialog(&mut self, ctx: &Context) {
        let Some(mut text) = self.ai.prompt.take() else {
            return;
        };
        let mut open = true;
        let mut go = false;
        egui::Window::new(t("Generate Map with AI"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                let l = ui.label(t("Describe the topic of your mind map:"));
                let r = ui.add(
                    egui::TextEdit::multiline(&mut text)
                        .hint_text(t("e.g. Launch plan for a mobile app"))
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
                    if ui
                        .add_enabled(!text.trim().is_empty(), egui::Button::new(t("Generate")))
                        .clicked()
                        || enter
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
            self.start_generate_map(text);
        } else if open && !ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.ai.prompt = Some(text);
        }
    }

    /// Floating "AI is working" indicator above the status bar.
    pub fn ai_busy_overlay(&mut self, ctx: &Context) {
        let Some(p) = &self.ai.pending else { return };
        let label = format!(
            "{} ({:.0}s)",
            t(p.command.label()),
            p.job.started.elapsed().as_secs_f32()
        );
        egui::Area::new(egui::Id::unique("thinoda_ai_busy"))
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -44.0])
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new(label).small());
                    });
                });
            });
    }

    /// AI section of the settings window.
    pub fn ai_settings_section(&mut self, ui: &mut Ui) {
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
            widgets::property_field(ui, t("API Key"), |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut ai.api_key)
                        .password(true)
                        .hint_text(thinoda_ai::API_KEY_ENV),
                )
            });
            widgets::property_field(ui, t("Creativity"), |ui| {
                ui.add(egui::Slider::new(&mut ai.temperature, 0.0..=1.5))
            });
            ui.horizontal(|ui| {
                let busy = self.ai.test.is_some();
                if ui
                    .add_enabled(!busy, egui::Button::new(t("Test Connection")))
                    .clicked()
                {
                    self.ai.test_result = None;
                    self.ai.test =
                        Some(AiJob::spawn_with(config(&self.settings.ai), |c| c.models()));
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
