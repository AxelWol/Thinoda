use super::*;

/// Request ownership survives transport completion, but never enters saved files.
pub struct AiTarget {
    pub command: AiCommand,
    pub doc_key: Option<String>,
    pub topic: Option<TopicId>,
    pub(crate) source: Option<(usize, TopicId, u64)>,
    pub(crate) viewport: egui::ViewportId,
    pub(super) config: AiConfig,
}

pub struct AiReview {
    pub target: AiTarget,
    pub result: AiResult,
}

fn outline_text(nodes: &[thinoda_ai::OutlineNode], depth: usize, text: &mut String) {
    use std::fmt::Write;
    for node in nodes {
        let _ = writeln!(text, "{}- {}", "  ".repeat(depth), node.title);
        outline_text(&node.children, depth + 1, text);
    }
}

impl ThinodaApp {
    pub(super) fn validate_ai_target(&self, target: &AiTarget) -> Result<(), &'static str> {
        if !self.settings.ai.enabled {
            return Err(t("AI assistant is disabled in Settings"));
        }
        let current = config(&self.settings.ai);
        if current.base_url != target.config.base_url
            || current.model != target.config.model
            || current.api_key != target.config.api_key
            || current.temperature != target.config.temperature
        {
            return Err(t("AI settings changed. Try again."));
        }
        if target.command == AiCommand::GenerateMap {
            return Ok(());
        }
        let Some(doc) = self
            .docs
            .iter()
            .find(|doc| Some(&doc.key) == target.doc_key.as_ref())
        else {
            return Err(t("The topic no longer exists"));
        };
        if target.source
            != Some((
                doc.doc.active_index(),
                doc.doc.sheet().root,
                doc.doc.state_id(),
            ))
            || doc.inspector_edit.is_some()
            || doc.view.editing_topic().is_some()
        {
            return Err(t("The document changed. Try again."));
        }
        if target
            .topic
            .is_none_or(|topic| doc.doc.sheet().topic(topic).is_none())
        {
            return Err(t("The topic no longer exists"));
        }
        Ok(())
    }

    /// Review is nonmodal: map edits remain possible and invalidate Apply.
    pub(crate) fn ai_review_window(&mut self, ctx: &Context) {
        if self
            .ai
            .review
            .as_ref()
            .is_none_or(|review| review.target.viewport != self.current_viewport)
        {
            return;
        }
        let review = self.ai.review.take().unwrap();
        let validation = self.validate_ai_target(&review.target);
        let mut open = true;
        let mut apply = false;
        let mut discard = false;
        let allowed = widgets::touch_activation_allowed(ctx);
        let labels = crate::i18n::dialog_labels();
        let _ = thinoda_ui::Dialog::new(
            egui::Id::unique("thinoda_ai_response"),
            t("Review AI Result"),
        )
        .icon(thinoda_ui::icons::AI)
        .labels(&labels)
        .open(&mut open)
        .default_width(520.0)
        .scroll(false)
        .show(ctx, |ui| {
            ui.set_max_width((ctx.content_rect().width() - 48.0).max(1.0));
            ui.label(t(review.target.command.label()));
            if let Some(doc) = self
                .docs
                .iter()
                .find(|doc| Some(&doc.key) == review.target.doc_key.as_ref())
                && let Some(topic) = review.target.topic.and_then(|id| doc.doc.sheet().topic(id))
            {
                ui.label(&topic.title);
            }
            widgets::caption(ui, t("Nothing changes until you apply this result."));
            ui.label(format!("{}: {}", t("Model"), review.result.model));
            if review.result.kind == OutputKind::Map {
                ui.label(t("This result creates a new map."));
            }
            if review.result.kind == OutputKind::Sheet {
                ui.label(t("Apply creates a new sheet. The source stays unchanged."));
            }
            if let Err(error) = validation {
                ui.label(RichText::new(error).color(ui.visuals().error_fg_color));
            }
            ui.horizontal_wrapped(|ui| {
                apply = ui
                    .add_enabled(validation.is_ok(), egui::Button::new(t("Apply AI Result")))
                    .clicked()
                    && allowed;
                discard = ui.button(t("Discard AI Result")).clicked() && allowed;
            });
            ui.separator();
            let text = match &review.result.output {
                AiOutput::Text(text) => text.clone(),
                AiOutput::Outline(outline) => {
                    let mut text = outline.title.clone().unwrap_or_default();
                    text.push('\n');
                    outline_text(&outline.items, 0, &mut text);
                    text
                }
            };
            egui::ScrollArea::vertical()
                .max_height((ctx.content_rect().height() - 250.0).max(40.0))
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut text.as_str()).desired_width(f32::INFINITY),
                    );
                });
        });
        if apply {
            if let Err(error) = self.apply_ai(&review.target, &review.result) {
                self.toasts.error(error);
                self.ai.review = Some(review);
            }
        } else if !discard && (open || !allowed) {
            self.ai.review = Some(review);
        }
    }

    pub(super) fn ai_stream_window(&mut self, ctx: &Context) {
        let Some(pending) = &self.ai.pending else {
            return;
        };
        if pending.target.viewport != self.current_viewport {
            return;
        }
        let mut open = true;
        let mut cancel = false;
        let allowed = widgets::touch_activation_allowed(ctx);
        let labels = crate::i18n::dialog_labels();
        let _ = thinoda_ui::Dialog::new(
            egui::Id::unique("thinoda_ai_response"),
            t("AI is still working…"),
        )
        .icon(thinoda_ui::icons::AI)
        .labels(&labels)
        .open(&mut open)
        .default_width(520.0)
        .scroll(false)
        .show(ctx, |ui| {
            ui.set_max_width((ctx.content_rect().width() - 48.0).max(1.0));
            ui.label(t(pending.target.command.label()));
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(format!("{:.0}s", pending.started.elapsed().as_secs_f32()));
                cancel = ui.button(t("Cancel AI Request")).clicked() && allowed;
            });
            widgets::caption(ui, t("Nothing changes until you apply this result."));
            egui::ScrollArea::vertical()
                .max_height((ctx.content_rect().height() - 220.0).max(40.0))
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut pending.preview.as_str())
                            .desired_width(f32::INFINITY),
                    );
                });
        });
        if cancel || (!open && allowed) {
            self.cancel_ai();
        }
    }

    pub(crate) fn return_ai_viewport(&mut self, viewport: egui::ViewportId) {
        self.return_ai_chat_viewport(viewport);
        self.return_prompt_editor_viewport(viewport);
        self.return_semantic_viewport(viewport);
        if let Some((target, _)) = &mut self.ai.prompted_target
            && target.viewport == viewport
        {
            target.viewport = egui::ViewportId::ROOT;
        }
        if self.ai.prompt_viewport == Some(viewport) {
            self.ai.prompt_viewport = Some(egui::ViewportId::ROOT);
        }
        if let Some(pending) = &mut self.ai.pending
            && pending.target.viewport == viewport
        {
            pending.target.viewport = egui::ViewportId::ROOT;
        }
        if let Some(review) = &mut self.ai.review
            && review.target.viewport == viewport
        {
            review.target.viewport = egui::ViewportId::ROOT;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    fn result(app: &ThinodaApp) -> AiReview {
        let doc = app.doc().unwrap();
        let root = doc.doc.sheet().root;
        AiReview {
            target: AiTarget {
                command: AiCommand::Explain,
                doc_key: Some(doc.key.clone()),
                topic: Some(root),
                source: Some((doc.doc.active_index(), root, doc.doc.state_id())),
                viewport: egui::ViewportId::ROOT,
                config: config(&app.settings.ai),
            },
            result: AiResult {
                kind: OutputKind::Note,
                output: AiOutput::Text("Reviewed explanation\n".repeat(200)),
                model: "local-test".into(),
            },
        }
    }

    #[test]
    fn ai_review_requires_apply_and_keeps_small_window_discard_reachable() {
        for apply in [false, true] {
            let mut app = ThinodaApp::headless();
            let before = app.doc().unwrap().doc.workbook().clone();
            app.ai.review = Some(result(&app));
            let mut h = Harness::builder()
                .with_size(egui::vec2(360.0, 320.0))
                .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
            h.ctx
                .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            h.run_steps(3);
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
            let button = h.get_by_role_and_label(
                egui::accesskit::Role::Button,
                if apply {
                    "Apply AI Result"
                } else {
                    "Discard AI Result"
                },
            );
            assert!(h.ctx.content_rect().contains_rect(button.rect()));
            button.click();
            h.run_steps(2);
            assert!(h.state().ai.review.is_none());
            let doc = h.state_mut().doc_mut().unwrap();
            if apply {
                assert_ne!(doc.doc.workbook(), &before);
                assert!(doc.doc.undo());
            }
            assert_eq!(doc.doc.workbook(), &before);
        }
    }

    #[test]
    fn ai_review_rejects_changed_sources_and_settings_at_apply() {
        for change in [
            "revision",
            "sheet",
            "title",
            "closed",
            "disabled",
            "endpoint",
            "key",
            "model",
            "temperature",
            "viewport",
            "format",
        ] {
            let mut app = ThinodaApp::headless();
            app.settings.ai.credentials_ready = true;
            let mut review = result(&app);
            match change {
                "revision" => {
                    app.doc_mut()
                        .unwrap()
                        .doc
                        .edit_sheet("Local edit", |sheet| sheet.set_title(sheet.root, "Changed"))
                        .unwrap();
                }
                "sheet" => {
                    let doc = app.doc_mut().unwrap();
                    doc.doc.duplicate_sheet(0).unwrap();
                    doc.doc.set_active(1);
                }
                "title" => {
                    let doc = app.doc_mut().unwrap();
                    let root = doc.doc.sheet().root;
                    doc.view
                        .begin_edit_with(doc.doc.sheet(), root, "Local title draft");
                }
                "closed" => app.docs.clear(),
                "disabled" => app.settings.ai.enabled = false,
                "endpoint" => app.settings.ai.base_url.push_str("/changed"),
                "key" => app.settings.ai.api_key = "changed-test-key".into(),
                "model" => app.settings.ai.model = "changed-model".into(),
                "temperature" => app.settings.ai.temperature += 0.1,
                "viewport" => app.current_viewport = egui::ViewportId::from_hash_of("other"),
                "format" => review.result.kind = OutputKind::Map,
                _ => unreachable!(),
            }
            let before: Vec<_> = app
                .docs
                .iter()
                .map(|doc| (doc.doc.workbook().clone(), doc.doc.state_id()))
                .collect();
            assert!(
                app.apply_ai(&review.target, &review.result).is_err(),
                "{change}"
            );
            let after: Vec<_> = app
                .docs
                .iter()
                .map(|doc| (doc.doc.workbook().clone(), doc.doc.state_id()))
                .collect();
            assert_eq!(before, after, "{change}");
        }
    }

    #[test]
    fn ai_review_returns_from_closed_child_without_losing_output() {
        let mut app = ThinodaApp::headless();
        let child = egui::ViewportId::from_hash_of("ai-source-window");
        let mut review = result(&app);
        review.target.viewport = child;
        app.ai.review = Some(review);
        app.ai.prompt = Some("Unsaved prompt".into());
        app.ai.prompt_viewport = Some(child);
        app.return_ai_viewport(child);
        assert_eq!(
            app.ai.review.as_ref().unwrap().target.viewport,
            egui::ViewportId::ROOT
        );
        assert_eq!(app.ai.prompt.as_deref(), Some("Unsaved prompt"));
        assert_eq!(app.ai.prompt_viewport, Some(egui::ViewportId::ROOT));
    }
}
