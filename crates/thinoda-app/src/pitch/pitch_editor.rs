use super::*;
use thinoda_core::PresentationScope;

#[cfg(test)]
#[path = "editor_tests.rs"]
mod tests;

impl PitchEditor {
    fn allocate_id(&mut self) -> Result<thinoda_core::SlideId, String> {
        let id = self
            .next_slide_id
            .ok_or_else(|| t("Not possible here").to_string())?;
        self.next_slide_id = id.0.checked_add(1).map(thinoda_core::SlideId);
        Ok(id)
    }

    pub(super) fn add_slide(
        &mut self,
        title: &str,
        source: PresentationSource,
        frame: Rect,
    ) -> Result<thinoda_core::SlideId, String> {
        let id = self.allocate_id()?;
        self.sheet.add_presentation_slide(title, source, frame)?;
        self.sheet
            .presentation
            .slides
            .last_mut()
            .expect("slide was added")
            .id = id;
        Ok(id)
    }

    pub(super) fn replace_generated(
        &mut self,
        mut presentation: thinoda_core::Presentation,
    ) -> Result<(), String> {
        for slide in &mut presentation.slides {
            slide.id = self.allocate_id()?;
        }
        self.sheet.presentation = presentation;
        self.selected = 0;
        self.generation += 1;
        Ok(())
    }

    /// Selection moves with the slide, not its position in the filmstrip.
    fn move_slide(&mut self, target: usize) {
        if self.selected < self.sheet.presentation.slides.len()
            && target < self.sheet.presentation.slides.len()
        {
            self.sheet.presentation.slides.swap(self.selected, target);
            self.selected = target;
        }
    }

    fn duplicate_slide(&mut self) -> Result<(), String> {
        let Some(mut copy) = self.sheet.presentation.slides.get(self.selected).cloned() else {
            return Ok(());
        };
        let id = self.add_slide(&copy.title, copy.source, copy.frame)?;
        copy.id = id;
        self.sheet.presentation.slides.pop();
        self.selected += 1;
        self.sheet.presentation.slides.insert(self.selected, copy);
        Ok(())
    }

    pub(super) fn workspace(&mut self, ui: &mut egui::Ui, touch: bool) {
        self.selected = self
            .selected
            .min(self.sheet.presentation.slides.len().saturating_sub(1));
        let before = self.sheet.presentation.clone();
        if ui.available_width() >= 720.0 {
            let width = ui.available_width();
            let filmstrip_height =
                430.0 + ui.text_style_height(&egui::TextStyle::Body) + ui.spacing().item_spacing.y;
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2((width * 0.22).min(210.0), filmstrip_height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| self.filmstrip(ui, touch, false),
                );
                ui.separator();
                ui.vertical(|ui| {
                    ui.set_width((width - (width * 0.22).min(210.0) - 24.0).max(1.0));
                    self.slide_workspace(ui, touch);
                });
            });
        } else {
            self.filmstrip(ui, touch, true);
            self.slide_workspace(ui, touch);
        }
        if before != self.sheet.presentation {
            self.generation += 1;
        }
    }

    fn filmstrip(&mut self, ui: &mut egui::Ui, touch: bool, horizontal: bool) {
        ui.strong(t("Slides"));
        let render = |ui: &mut egui::Ui, this: &mut Self| {
            for (index, slide) in this.sheet.presentation.slides.iter().enumerate() {
                let selected = this.selected == index;
                let label = format!("{} · {}", index + 1, slide.title);
                let width = if horizontal {
                    160.0
                } else {
                    ui.available_width()
                };
                let palette = Palette::of_ui(ui);
                ui.push_id(slide.id, |ui| {
                    egui::Frame::new()
                        .fill(palette.surface_alt)
                        .stroke(egui::Stroke::new(
                            if selected { 2.0 } else { 1.0 },
                            if selected {
                                palette.accent
                            } else {
                                ui.visuals().widgets.noninteractive.bg_stroke.color
                            },
                        ))
                        .corner_radius(8.0)
                        .inner_margin(6.0)
                        .show(ui, |ui| {
                            ui.set_width((width - 16.0).max(1.0));
                            let clicked = SlidePreview.thumbnail(
                                ui,
                                &this.sheet,
                                &this.layout,
                                index,
                                egui::vec2(ui.available_width(), 78.0),
                            );
                            if (ui
                                .add(egui::Button::selectable(selected, label).wrap())
                                .clicked()
                                || clicked)
                                && touch
                            {
                                this.selected = index;
                            }
                        });
                });
            }
        };
        if horizontal {
            egui::ScrollArea::horizontal()
                .id_salt("pitch-filmstrip-row")
                .show(ui, |ui| {
                    ui.horizontal(|ui| render(ui, self));
                });
        } else {
            egui::ScrollArea::vertical()
                .id_salt("pitch-filmstrip-column")
                .max_height(430.0)
                .show(ui, |ui| render(ui, self));
        }
    }

    fn slide_workspace(&mut self, ui: &mut egui::Ui, touch: bool) {
        let count = self.sheet.presentation.slides.len();
        if count == 0 {
            ui.label(t(
                "No slides yet. Generate branch and Zone slides, or add the selected branch.",
            ));
            return;
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(self.selected > 0, egui::Button::new(t("Move Up")))
                .clicked()
                && touch
            {
                self.move_slide(self.selected - 1);
            }
            if ui
                .add_enabled(self.selected + 1 < count, egui::Button::new(t("Move Down")))
                .clicked()
                && touch
            {
                self.move_slide(self.selected + 1);
            }
            if ui.button(t("Duplicate Slide")).clicked()
                && touch
                && let Err(error) = self.duplicate_slide()
            {
                self.error = Some(error);
            }
            if ui.button(t("Remove Slide")).clicked() && touch {
                self.sheet.presentation.slides.remove(self.selected);
                self.selected = self
                    .selected
                    .min(self.sheet.presentation.slides.len().saturating_sub(1));
            }
        });
        let Some(slide) = self.sheet.presentation.slides.get_mut(self.selected) else {
            return;
        };
        let label = ui.label(t("Slide title"));
        ui.add(egui::TextEdit::singleline(&mut slide.title).desired_width(f32::INFINITY))
            .labelled_by(label.id);
        ui.label(t("Click a topic to hide its subtree."));
        let width = ui.available_width();
        let size = egui::vec2(width, (width * 0.56).clamp(100.0, 330.0));
        if let Some(topic) = self
            .preview
            .show(ui, &self.sheet, &self.layout, self.selected, size)
            && touch
        {
            let slide = &mut self.sheet.presentation.slides[self.selected];
            if !slide.hidden_topics.contains(&topic) {
                slide.hidden_topics.push(topic);
            }
        }
        self.slide_settings(ui, touch);
    }

    fn slide_settings(&mut self, ui: &mut egui::Ui, touch: bool) {
        let current_source = self.sheet.presentation.slides[self.selected].source;
        let source_title = match current_source {
            PresentationSource::Branch(topic) => {
                self.sheet.topic(topic).map(|topic| topic.title.clone())
            }
            PresentationSource::Zone(id) => self
                .sheet
                .zones
                .iter()
                .find(|zone| zone.id == id)
                .map(|zone| zone.name.clone()),
        }
        .unwrap_or_else(|| t("Slide source is unavailable.").to_string());
        ui.horizontal_wrapped(|ui| {
            let label = ui.label(t("Slide source"));
            let mut source = current_source;
            egui::ComboBox::from_id_salt("pitch-source")
                .width(ui.available_width().min(280.0))
                .selected_text(source_title)
                .show_ui(ui, |ui| {
                    for topic in &self.layout.order {
                        if let Some(topic_data) = self.sheet.topic(*topic) {
                            ui.selectable_value(
                                &mut source,
                                PresentationSource::Branch(*topic),
                                &topic_data.title,
                            );
                        }
                    }
                    for zone in &self.sheet.zones {
                        ui.selectable_value(
                            &mut source,
                            PresentationSource::Zone(zone.id),
                            &zone.name,
                        );
                    }
                })
                .response
                .labelled_by(label.id);
            if source != current_source && touch {
                let slide = &mut self.sheet.presentation.slides[self.selected];
                slide.source = source;
                slide.auto_frame = true;
            }
        });
        let slide = &mut self.sheet.presentation.slides[self.selected];
        ui.horizontal_wrapped(|ui| {
            ui.strong(t("Slide content"));
            let mut scope = slide.scope;
            if ui
                .selectable_value(&mut scope, PresentationScope::Source, t("Source subtree"))
                .changed()
                && touch
            {
                slide.scope = scope;
            }
            if ui
                .selectable_value(&mut scope, PresentationScope::Context, t("Map context"))
                .changed()
                && touch
            {
                slide.scope = scope;
            }
        });
        let auto_frame = slide.auto_frame;
        ui.horizontal_wrapped(|ui| {
            let mut auto = auto_frame;
            if ui.checkbox(&mut auto, t("Auto-fit content")).changed() && touch {
                // Preserve the currently visible composition when switching to manual.
                if !auto
                    && let Ok(resolved) = thinoda_core::resolve_presentation_slide(
                        &self.sheet,
                        &self.sheet.presentation.slides[self.selected],
                        &self.layout,
                    )
                {
                    self.sheet.presentation.slides[self.selected].frame = resolved.frame;
                }
                self.sheet.presentation.slides[self.selected].auto_frame = auto;
            }
            if ui.button(t("Fit Slide to Content")).clicked() && touch {
                self.sheet.presentation.slides[self.selected].auto_frame = true;
            }
        });
        let slide = &mut self.sheet.presentation.slides[self.selected];
        let owner = Some(egui::Id::unique((
            &self.owner.document,
            self.owner.sheet,
            self.owner.revision,
            self.owner.viewport,
            slide.id,
        )));
        ui.add_enabled_ui(!slide.auto_frame, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (salt, text, value, minimum) in [
                    ("slide-x", "X", &mut slide.frame.min.x, -f32::MAX),
                    ("slide-y", "Y", &mut slide.frame.min.y, -f32::MAX),
                    ("slide-width", "Width", &mut slide.frame.size.x, 1.0),
                    ("slide-height", "Height", &mut slide.frame.size.y, 1.0),
                ] {
                    let label = ui.label(t(text));
                    let mut staged = *value;
                    let (_, commit) = crate::ui::touch_numeric_value(
                        ui,
                        &mut staged,
                        salt,
                        owner,
                        |ui, value| {
                            ui.add(
                                egui::DragValue::new(value)
                                    .max_decimals(1)
                                    .range(minimum..=f32::MAX),
                            )
                            .labelled_by(label.id)
                        },
                    );
                    if commit && touch {
                        *value = staged;
                    }
                }
            });
        });
        ui.horizontal_wrapped(|ui| {
            let mut limited = slide.max_depth.is_some();
            if ui.checkbox(&mut limited, t("Limit topic depth")).changed() && touch {
                slide.max_depth = limited.then_some(1);
            }
            if let Some(depth) = &mut slide.max_depth {
                let label = ui.label(t("Topic depth"));
                let mut staged = *depth as f32;
                let (_, commit) = crate::ui::touch_numeric_value(
                    ui,
                    &mut staged,
                    "slide-depth",
                    owner,
                    |ui, value| {
                        ui.add(
                            egui::DragValue::new(value)
                                .range(0.0..=1024.0)
                                .max_decimals(0),
                        )
                        .labelled_by(label.id)
                    },
                );
                if commit && touch {
                    *depth = staged as usize;
                }
            }
        });
        // A text/keyboard alternative to every preview gesture, including restoring hidden roots.
        let hidden = slide.hidden_topics.clone();
        if !hidden.is_empty() {
            ui.strong(t("Hidden topics"));
            for topic in hidden {
                let title = self
                    .sheet
                    .topic(topic)
                    .map(|topic| topic.title.clone())
                    .unwrap_or_else(|| t("Missing topic").to_string());
                ui.push_id(topic, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(title);
                        if ui.button(t("Restore topic")).clicked() && touch {
                            self.sheet.presentation.slides[self.selected]
                                .hidden_topics
                                .retain(|id| *id != topic);
                        }
                    });
                });
            }
            if ui.button(t("Restore all topics")).clicked() && touch {
                self.sheet.presentation.slides[self.selected]
                    .hidden_topics
                    .clear();
            }
        }
        egui::CollapsingHeader::new(t("Topic visibility")).show(ui, |ui| {
            if let Ok(resolved) = thinoda_core::resolve_presentation_slide(
                &self.sheet,
                &self.sheet.presentation.slides[self.selected],
                &self.layout,
            ) {
                for topic in resolved.layout.order {
                    if let Some(topic_data) = self.sheet.topic(topic) {
                        let mut visible = true;
                        if ui
                            .push_id(topic, |ui| ui.checkbox(&mut visible, &topic_data.title))
                            .inner
                            .changed()
                            && touch
                        {
                            self.sheet.presentation.slides[self.selected]
                                .hidden_topics
                                .push(topic);
                        }
                    }
                }
            }
        });
    }
}
