use super::*;

fn player_viewport() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("thinoda-pitch-player")
}

impl ThinodaApp {
    /// Follow the captured sheet and slide identity.
    /// An edit to another sheet advances the revision but does not restart this scene.
    pub(super) fn refresh_pitch(&mut self, player: &mut PitchPlayer) -> Result<bool, String> {
        let doc = self
            .docs
            .iter_mut()
            .find(|doc| doc.key == player.owner.document)
            .ok_or_else(|| t("The presentation sheet is unavailable.").to_string())?;
        // Topic IDs are sheet-local. Never search other sheets for a matching root:
        // an unrelated sheet can have the same ID.
        let index = player.owner.sheet;
        let sheet = doc
            .doc
            .workbook()
            .sheets
            .get(index)
            .filter(|sheet| sheet.root == player.owner.root)
            .ok_or_else(|| t("The presentation sheet is unavailable.").to_string())?;
        if doc.doc.state_id() == player.owner.revision && sheet != player.sheet.as_ref() {
            return Err(t("The presentation sheet is unavailable.").to_string());
        }
        player.owner.revision = doc.doc.state_id();
        player.owner.sheet = index;
        if sheet == player.sheet.as_ref() {
            return Ok(false);
        }
        thinoda_core::images::validate_visual(sheet).map_err(|error| error.to_string())?;
        let selected = player
            .sheet
            .presentation
            .slides
            .get(player.selected)
            .map(|slide| slide.id);
        let next = sheet
            .presentation
            .slides
            .iter()
            .position(|slide| Some(slide.id) == selected)
            .unwrap_or_else(|| {
                player
                    .selected
                    .min(sheet.presentation.slides.len().saturating_sub(1))
            });
        player.sheet = Arc::new(sheet.clone());
        player.layout = if doc.doc.active_index() == index {
            doc.view.layout_for(&doc.doc)
        } else {
            Arc::new(thinoda_core::layout_sheet(
                &player.sheet,
                &thinoda_core::ApproxMeasure,
                doc.view.dark,
            ))
        };
        player.selected = next;
        player.playback.refresh();
        Ok(true)
    }

    pub(crate) fn is_pitch_viewport(&self) -> bool {
        !crate::files::is_web() && self.current_viewport == player_viewport()
    }

    pub(crate) fn open_pitch(&mut self, ctx: &Context, presenter: bool) {
        let Some(doc) = self.doc_mut() else { return };
        if let Err(error) = doc.flush_inspector_edit(ctx) {
            self.toasts.error(error);
            return;
        }
        doc.view.commit_edit(&mut doc.doc);
        if let Err(error) = thinoda_core::images::validate_visual(doc.doc.sheet()) {
            self.toasts.error(error);
            return;
        }
        if doc.doc.sheet().presentation.slides.is_empty() {
            self.begin_presentation_editor(ctx);
            return;
        }
        self.pitch.player = Some(PitchPlayer {
            owner: PitchOwner {
                document: doc.key.clone(),
                sheet: doc.doc.active_index(),
                root: doc.doc.sheet().root,
                revision: doc.doc.state_id(),
                viewport: ctx.viewport_id(),
            },
            sheet: Arc::new(doc.doc.sheet().clone()),
            layout: doc.view.layout_for(&doc.doc),
            selected: 0,
            presenter,
            fullscreen: false,
            playback: Default::default(),
        });
    }

    pub(crate) fn pitch_window(&mut self, ctx: &Context) {
        self.poll_pitch_export(ctx);
        let Some(mut player) = self.pitch.player.take() else {
            return;
        };
        match self.refresh_pitch(&mut player) {
            Ok(true) => {
                self.toasts
                    .info(t("Presentation refreshed from the current map."));
            }
            Ok(false) => {}
            Err(error) => {
                self.toasts.warning(error);
                return;
            }
        }
        let mut open = true;
        #[cfg(not(target_arch = "wasm32"))]
        {
            let previous_viewport = self.current_viewport;
            let previous_active = self.active;
            self.current_viewport = player_viewport();
            if let Some(index) = self
                .docs
                .iter()
                .position(|doc| doc.key == player.owner.document)
            {
                self.active = index;
            }
            ctx.show_viewport_immediate(
                player_viewport(),
                egui::ViewportBuilder::default()
                    .with_title(format!("{} - Thinoda", t("Pitch")))
                    .with_inner_size([1100.0, 700.0])
                    .with_fullscreen(player.fullscreen),
                |ui, _| {
                    let ctx = ui.ctx().clone();
                    if ctx.input(|input| input.viewport().close_requested()) {
                        open = false;
                        return;
                    }
                    #[cfg(target_os = "macos")]
                    if ctx.input(|input| input.focused) {
                        self.poll_native_menus(&ctx);
                    }
                    if ctx.input(|input| input.focused)
                        && !self.any_modal_open()
                        && !ctx.text_edit_focused()
                    {
                        for (shortcut, command) in
                            self.profile_bindings(&self.settings.keymaps, self.mode, ctx.os())
                        {
                            if self.command_available(&command)
                                && ctx.input_mut(|input| input.consume_shortcut(&shortcut))
                            {
                                self.exec(&ctx, command);
                                break;
                            }
                        }
                    }
                    open &= self.pitch_player_ui(ui, &mut player);
                    self.toasts.show_with_modal(&ctx, self.any_modal_open());
                    #[cfg(target_os = "macos")]
                    if ctx.input(|input| input.focused) {
                        self.sync_native_menus(&ctx);
                    }
                },
            );
            self.current_viewport = previous_viewport;
            self.active = previous_active.min(self.docs.len());
        }
        #[cfg(target_arch = "wasm32")]
        {
            let labels = crate::i18n::dialog_labels();
            let requested_maximized = player.fullscreen;
            let window = thinoda_ui::Dialog::new(egui::Id::unique("pitch-player"), t("Pitch"))
                .icon(thinoda_ui::icons::PITCH)
                .labels(&labels)
                .open(&mut open)
                .default_width(1000.0)
                .default_height(650.0)
                .maximized(requested_maximized)
                .scroll(false);
            let mut keep_playing = true;
            let response = window.show(ctx, |ui| {
                keep_playing = self.pitch_player_ui(ui, &mut player);
            });
            if player.fullscreen == requested_maximized {
                player.fullscreen = response.maximized;
            }
            open &= keep_playing;
        }
        if open {
            self.pitch.player = Some(player);
        } else {
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
    }

    pub(super) fn pitch_player_ui(&mut self, ui: &mut egui::Ui, player: &mut PitchPlayer) -> bool {
        let blocked = self.any_modal_open();
        let touch = widgets::touch_activation_allowed(ui.ctx());
        let popup_was_open = egui::Popup::is_any_open(ui.ctx());
        let mut previous = false;
        let mut next = false;
        let mut first = false;
        let mut last = false;
        let mut exit = false;
        let mut fullscreen = false;
        let mut export = None;
        let options = ExportOptions {
            dark: Palette::of_ui(ui).dark,
            ..Default::default()
        };
        let count = player.sheet.presentation.slides.len();
        player.selected = player.selected.min(count.saturating_sub(1));
        let steps =
            pitch_motion::Playback::topic_order(&player.sheet, &player.layout, player.selected)
                .len()
                .max(1);
        let step = player.playback.step.min(steps);
        ui.add_enabled_ui(!blocked, |ui| {
            ui.horizontal_wrapped(|ui| {
                first = ui
                    .add_enabled(
                        player.selected > 0 || step > 1 && player.playback.build_topics,
                        egui::Button::new(t("First Slide")),
                    )
                    .clicked()
                    && touch;
                previous = ui
                    .add_enabled(
                        player.selected > 0 || player.playback.build_topics && step > 1,
                        egui::Button::new(t("Previous Slide")),
                    )
                    .clicked()
                    && touch;
                next = ui
                    .add_enabled(
                        player.selected + 1 < count || player.playback.build_topics && step < steps,
                        egui::Button::new(t("Next Slide")),
                    )
                    .clicked()
                    && touch;
                last = ui
                    .add_enabled(
                        player.selected + 1 < count || player.playback.build_topics && step < steps,
                        egui::Button::new(t("Last Slide")),
                    )
                    .clicked()
                    && touch;
                let mut presenter = player.presenter;
                if ui.checkbox(&mut presenter, t("Presenter View")).changed() && touch {
                    player.presenter = presenter;
                }
                let label = if crate::files::is_web() {
                    if player.fullscreen {
                        "Restore Player"
                    } else {
                        "Expand Player"
                    }
                } else if player.fullscreen {
                    "Leave Fullscreen"
                } else {
                    "Fullscreen"
                };
                fullscreen = ui.button(t(label)).clicked() && touch;
                exit = ui.button(t("Exit Pitch")).clicked() && touch;
                ui.add_enabled_ui(self.pitch.output.is_none(), |ui| {
                    ui.menu_button(t("Export"), |ui| {
                        if ui.button(t("Export PDF")).clicked() && touch {
                            export = Some(false);
                            ui.close();
                        }
                        if ui.button(t("Export Slide Images")).clicked() && touch {
                            export = Some(true);
                            ui.close();
                        }
                    });
                });
            });
            ui.horizontal_wrapped(|ui| {
                let label = ui.label(t("Transition"));
                let mut transition = player.playback.transition;
                egui::ComboBox::from_id_salt("pitch-transition")
                    .selected_text(t(transition.label()))
                    .show_ui(ui, |ui| {
                        for value in pitch_motion::Transition::ALL {
                            ui.selectable_value(&mut transition, value, t(value.label()));
                        }
                    })
                    .response
                    .labelled_by(label.id);
                if touch {
                    player.playback.transition = transition;
                }
                let mut reduce_motion = player.playback.reduce_motion;
                if ui
                    .checkbox(&mut reduce_motion, t("Reduce motion"))
                    .changed()
                    && touch
                {
                    player.playback.reduce_motion = reduce_motion;
                }
                let mut build = player.playback.build_topics;
                if ui.checkbox(&mut build, t("Build topics")).changed() && touch {
                    player.playback.build_topics = build;
                    player.playback.step = if build { 1 } else { usize::MAX };
                }
            });
        });
        if !blocked
            && !popup_was_open
            && !egui::Popup::is_any_open(ui.ctx())
            && !ui.ctx().text_edit_focused()
            && ui.ctx().input(|input| input.focused)
            && touch
        {
            ui.ctx().input_mut(|input| {
                exit |= input.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
                first |= input.consume_key(egui::Modifiers::NONE, egui::Key::Home);
                last |= input.consume_key(egui::Modifiers::NONE, egui::Key::End);
                previous |= input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft)
                    || input.consume_key(egui::Modifiers::NONE, egui::Key::PageUp);
                next |= input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight)
                    || input.consume_key(egui::Modifiers::NONE, egui::Key::PageDown)
                    || input.consume_key(egui::Modifiers::NONE, egui::Key::Space);
                #[cfg(not(target_arch = "wasm32"))]
                {
                    fullscreen |= input.consume_key(egui::Modifiers::NONE, egui::Key::F11);
                }
            });
        }
        if previous {
            player
                .playback
                .navigate(&player.sheet, &player.layout, &mut player.selected, false);
        }
        if next {
            player
                .playback
                .navigate(&player.sheet, &player.layout, &mut player.selected, true);
        }
        if first {
            player.selected = 0;
            player.playback.step = if player.playback.build_topics {
                1
            } else {
                usize::MAX
            };
        }
        if last {
            player.selected = count.saturating_sub(1);
            player.playback.step = usize::MAX;
        }
        if fullscreen {
            player.fullscreen = !player.fullscreen;
            #[cfg(not(target_arch = "wasm32"))]
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Fullscreen(player.fullscreen));
        }
        if let Some(images) = export {
            self.begin_pitch_export(ui.ctx(), (*player.sheet).clone(), options.clone(), images);
        }
        if count == 0 {
            ui.label(t(
                "No slides yet. Generate branch and Zone slides, or add the selected branch.",
            ));
            return !exit;
        }
        ui.label(format!(
            "{} / {} · {}",
            player.selected + 1,
            count,
            player.sheet.presentation.slides[player.selected].title
        ));
        ui.label(t(
            "Right / Space: next · Left: previous · Home / End: first / last · Escape: exit",
        ));
        if player.playback.build_topics {
            let steps =
                pitch_motion::Playback::topic_order(&player.sheet, &player.layout, player.selected)
                    .len()
                    .max(1);
            ui.label(format!(
                "{}: {} / {}",
                t("Build topics"),
                player.playback.step.min(steps),
                steps
            ));
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            let available = egui::vec2(
                ui.available_width(),
                (ui.ctx().content_rect().height() - 170.0).max(100.0),
            );
            if player.presenter && available.x >= 760.0 {
                ui.horizontal_top(|ui| {
                    player.playback.show(
                        ui,
                        &player.sheet,
                        &player.layout,
                        player.selected,
                        egui::vec2((available.x * 0.62 - 8.0).max(1.0), available.y),
                        options.dark,
                    );
                    ui.vertical(|ui| {
                        ui.strong(t("Map"));
                        presenter_map(
                            ui,
                            player,
                            egui::vec2((available.x * 0.38 - 8.0).max(1.0), available.y - 30.0),
                            options.dark,
                        );
                    });
                });
            } else {
                player.playback.show(
                    ui,
                    &player.sheet,
                    &player.layout,
                    player.selected,
                    egui::vec2(
                        available.x,
                        if player.presenter {
                            available.y * 0.6
                        } else {
                            available.y
                        },
                    ),
                    options.dark,
                );
                if player.presenter {
                    ui.strong(t("Map"));
                    presenter_map(
                        ui,
                        player,
                        egui::vec2(available.x, (available.y * 0.4).max(100.0)),
                        options.dark,
                    );
                }
            }
        });
        !exit
    }
}

fn presenter_map(ui: &mut egui::Ui, player: &PitchPlayer, size: egui::Vec2, dark: bool) {
    let Some(slide) = player.sheet.presentation.slides.get(player.selected) else {
        return;
    };
    let Ok(resolved) =
        thinoda_core::resolve_presentation_slide(&player.sheet, slide, &player.layout)
    else {
        ui.label(t("Slide source is unavailable."));
        return;
    };
    let (rect, _) = ui.allocate_exact_size(size.max(egui::vec2(1.0, 1.0)), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let background = thinoda_core::theme::Theme::for_settings(&player.sheet.theme)
        .background(&player.sheet.theme, dark);
    painter.rect_filled(rect, 4.0, thinoda_canvas::color32(background));
    let rect = rect.shrink(5.0);
    // The presenter orientation panel intentionally shows context outside the isolated
    // stage. Only its highlighted frame comes from the resolved slide.
    let bounds = player.layout.bounds.union(&resolved.frame);
    let zoom = (rect.width() / bounds.size.x.max(1.0)).min(rect.height() / bounds.size.y.max(1.0));
    let center = bounds.center();
    let xf = thinoda_canvas::Xf {
        origin: rect.center() - egui::vec2(center.x, center.y) * zoom,
        zoom,
    };
    thinoda_canvas::paint_map(
        &painter.with_clip_rect(rect),
        &player.sheet,
        &player.layout,
        xf,
        &thinoda_canvas::PaintOptions {
            dark,
            ..Default::default()
        },
    );
    painter.rect_stroke(
        xf.rect(resolved.frame),
        0.0,
        egui::Stroke::new(2.0, Palette::of_ui(ui).accent),
        egui::StrokeKind::Inside,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn painted_texts(output: &egui::FullOutput) -> Vec<String> {
        fn collect(shape: &egui::Shape, texts: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(text) => texts.push(text.galley.text().to_string()),
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, texts);
                    }
                }
                _ => {}
            }
        }
        let mut texts = Vec::new();
        for shape in &output.shapes {
            collect(&shape.shape, &mut texts);
        }
        texts
    }

    #[test]
    fn presenter_orientation_paints_full_map_and_original_zones_but_stage_stays_isolated() {
        for dark in [false, true] {
            let mut app = super::super::tests::setup(true);
            let ctx = Context::default();
            ctx.set_visuals(if dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            });
            app.open_pitch(&ctx, true);
            let mut player = app.pitch.player.take().unwrap();
            Arc::make_mut(&mut player.sheet).presentation.slides[0]
                .frame
                .min
                .x += 5000.0;
            let before = Arc::clone(&player.sheet);
            let slide = &player.sheet.presentation.slides[player.selected];
            let resolved =
                thinoda_core::resolve_presentation_slide(&player.sheet, slide, &player.layout)
                    .unwrap();
            assert!(resolved.layout.order.len() < player.layout.order.len());
            assert!(resolved.zones.is_empty());
            assert_ne!(resolved.frame, slide.frame);
            let raw = || egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 700.0),
                )),
                ..Default::default()
            };
            let mut orientation = ctx.run_ui(raw(), |ui| {
                presenter_map(ui, &player, egui::vec2(850.0, 600.0), dark);
            });
            orientation.textures_delta.clear();
            let texts = painted_texts(&orientation);
            for label in [
                "Center",
                "First branch",
                "Second branch",
                "Floating",
                "Zone",
            ] {
                assert!(
                    texts.iter().any(|text| text == label),
                    "missing orientation text: {label}: {texts:?}"
                );
            }
            // The orientation outline uses the shared effective auto-frame, not the
            // legacy saved frame. Its stroke must be present in the painted panel.
            let panel = orientation
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect) if rect.rect.size() == egui::vec2(850.0, 600.0) => {
                        Some(rect.rect)
                    }
                    _ => None,
                })
                .unwrap()
                .shrink(5.0);
            let bounds = player.layout.bounds.union(&resolved.frame);
            let zoom = (panel.width() / bounds.size.x.max(1.0))
                .min(panel.height() / bounds.size.y.max(1.0));
            let center = bounds.center();
            let xf = thinoda_canvas::Xf {
                origin: panel.center() - egui::vec2(center.x, center.y) * zoom,
                zoom,
            };
            assert!(orientation.shapes.iter().any(|shape| matches!(
                &shape.shape, egui::Shape::Rect(rect)
                    if rect.stroke.width == 2.0 && rect.stroke.color == Palette::of(&ctx).accent
                    && rect.rect == xf.rect(resolved.frame)
            )));
            let mut stage = ctx.run_ui(raw(), |ui| {
                player.playback.show(
                    ui,
                    &player.sheet,
                    &player.layout,
                    player.selected,
                    egui::vec2(850.0, 600.0),
                    dark,
                );
            });
            stage.textures_delta.clear();
            let texts = painted_texts(&stage);
            assert!(texts.iter().any(|text| text == "First branch"));
            for excluded in ["Center", "Second branch", "Floating", "Zone"] {
                assert!(
                    !texts.iter().any(|text| text == excluded),
                    "stage leaked {excluded}"
                );
            }
            assert_eq!(player.sheet, before);
        }
    }
}
