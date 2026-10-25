use super::*;

fn player_viewport() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("thinoda-pitch-player")
}

impl ThinodaApp {
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
            preview: Default::default(),
        });
    }

    pub(crate) fn pitch_window(&mut self, ctx: &Context) {
        self.poll_pitch_export(ctx);
        let Some(mut player) = self.pitch.player.take() else {
            return;
        };
        let valid = self.docs.iter().any(|doc| {
            doc.key == player.owner.document
                && doc.doc.state_id() == player.owner.revision
                && doc
                    .doc
                    .workbook()
                    .sheets
                    .get(player.owner.sheet)
                    .is_some_and(|sheet| sheet.root == player.owner.root)
        });
        if !valid {
            self.toasts.warning(t(
                "The document changed. Restart Pitch to show the current map.",
            ));
            return;
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
            let mut window = egui::Window::new(t("Pitch"))
                .id(egui::Id::unique("pitch-player"))
                .open(&mut open)
                .default_size([1000.0, 650.0])
                .max_size(ctx.content_rect().size() - egui::vec2(16.0, 16.0));
            if player.fullscreen {
                window = window
                    .fixed_pos(ctx.content_rect().min + egui::vec2(8.0, 8.0))
                    .fixed_size(ctx.content_rect().size() - egui::vec2(16.0, 16.0));
            }
            let mut keep_playing = true;
            window.show(ctx, |ui| {
                keep_playing = self.pitch_player_ui(ui, &mut player);
            });
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
        let mut previous = false;
        let mut next = false;
        let mut exit = false;
        let mut fullscreen = false;
        let mut export = None;
        let options = ExportOptions {
            dark: Palette::of_ui(ui).dark,
            ..Default::default()
        };
        let count = player.sheet.presentation.slides.len();
        ui.add_enabled_ui(!blocked, |ui| {
            ui.horizontal_wrapped(|ui| {
                previous = ui
                    .add_enabled(player.selected > 0, egui::Button::new(t("Previous Slide")))
                    .clicked()
                    && touch;
                next = ui
                    .add_enabled(
                        player.selected + 1 < count,
                        egui::Button::new(t("Next Slide")),
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
        });
        if !blocked
            && !egui::Popup::is_any_open(ui.ctx())
            && !ui.ctx().text_edit_focused()
            && ui.ctx().input(|input| input.focused)
            && touch
        {
            ui.ctx().input_mut(|input| {
                exit |= input.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
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
            player.selected = player.selected.saturating_sub(1);
        }
        if next {
            player.selected = (player.selected + 1).min(count.saturating_sub(1));
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
        ui.label(format!(
            "{} / {} · {}",
            player.selected + 1,
            count,
            player.sheet.presentation.slides[player.selected].title
        ));
        ui.label(t("Right / Space: next · Left: previous · Escape: exit"));
        egui::ScrollArea::vertical().show(ui, |ui| {
            let available = egui::vec2(
                ui.available_width(),
                (ui.ctx().content_rect().height() - 170.0).max(100.0),
            );
            if player.presenter && available.x >= 760.0 {
                ui.horizontal_top(|ui| {
                    player.preview.show(
                        ui,
                        &player.sheet,
                        player.selected,
                        0,
                        &options,
                        egui::vec2((available.x * 0.62 - 8.0).max(1.0), available.y),
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
                player.preview.show(
                    ui,
                    &player.sheet,
                    player.selected,
                    0,
                    &options,
                    egui::vec2(
                        available.x,
                        if player.presenter {
                            available.y * 0.6
                        } else {
                            available.y
                        },
                    ),
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
    let (rect, _) = ui.allocate_exact_size(size.max(egui::vec2(1.0, 1.0)), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let background = thinoda_core::theme::Theme::for_settings(&player.sheet.theme)
        .background(&player.sheet.theme, dark);
    painter.rect_filled(rect, 4.0, thinoda_canvas::color32(background));
    let rect = rect.shrink(5.0);
    let bounds = player
        .layout
        .bounds
        .union(&player.sheet.presentation.slides[player.selected].frame);
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
        xf.rect(player.sheet.presentation.slides[player.selected].frame),
        0.0,
        egui::Stroke::new(2.0, Palette::of_ui(ui).accent),
        egui::StrokeKind::Inside,
    );
}
