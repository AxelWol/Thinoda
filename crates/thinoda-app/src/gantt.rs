//! A synchronized projection of sheet tasks, never a second document owner.

use egui::{Context, RichText};
use std::{path::PathBuf, sync::mpsc};
use thinoda_core::{ComputedTask, LocalDate, Task, TopicId, WorkCalendar};
use thinoda_ui::{Palette, widgets};

use crate::{
    app::{Cmd, ThinodaApp},
    i18n::t,
};

#[cfg(test)]
#[path = "gantt_tests.rs"]
mod tests;

#[derive(Default)]
pub(crate) struct GanttState {
    owner: Option<(String, usize)>,
    root: Option<TopicId>,
    day_width: f32,
    output: Option<mpsc::Receiver<Result<PathBuf, String>>>,
}

impl ThinodaApp {
    pub(crate) fn is_gantt_viewport(&self) -> bool {
        !crate::files::is_web()
            && self.gantt.owner.as_ref().is_some_and(|(key, sheet)| {
                self.current_viewport == egui::ViewportId::from_hash_of(("gantt", key, sheet))
            })
    }

    pub(crate) fn open_gantt(&mut self) {
        self.gantt.root = self.doc().map(|doc| doc.doc.sheet().root);
        self.gantt.owner = self
            .doc()
            .map(|doc| (doc.key.clone(), doc.doc.active_index()));
        if self.gantt.day_width == 0.0 {
            self.gantt.day_width = 18.0;
        }
    }

    pub(crate) fn gantt_window(&mut self, ctx: &Context) {
        if let Some(receiver) = &self.gantt.output {
            let result = match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err(t("Export worker stopped unexpectedly").into()))
                }
                Err(mpsc::TryRecvError::Empty) => None,
            };
            if let Some(result) = result {
                self.gantt.output = None;
                match result.and_then(|path| {
                    crate::files::deliver(&path)?;
                    Ok(path)
                }) {
                    Ok(_) if crate::files::is_web() => self.toasts.success(t("Download started")),
                    Ok(path) => {
                        self.toasts
                            .success(format!("{} {}", t("Exported to"), path.display()))
                    }
                    Err(error) => self
                        .toasts
                        .error(format!("{}: {error}", t("Export failed"))),
                };
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
        let Some((key, sheet)) = self.gantt.owner.clone() else {
            return;
        };
        if !self.docs.iter().any(|doc| {
            doc.key == key
                && doc
                    .doc
                    .workbook()
                    .sheets
                    .get(sheet)
                    .is_some_and(|sheet| Some(sheet.root) == self.gantt.root)
        }) {
            let viewport = egui::ViewportId::from_hash_of(("gantt", &key, sheet));
            self.cancel_planning_viewport(viewport);
            self.return_print_viewport(viewport);
            self.gantt.owner = None;
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let viewport = egui::ViewportId::from_hash_of(("gantt", &key, sheet));
            let previous_viewport = self.current_viewport;
            let previous_active = self.active;
            self.current_viewport = viewport;
            if let Some(index) = self.docs.iter().position(|doc| doc.key == key) {
                self.active = index;
            }
            ctx.show_viewport_immediate(
                viewport,
                egui::ViewportBuilder::default()
                    .with_title(format!("{} - Thinoda", t("Gantt Chart")))
                    .with_inner_size([1100.0, 650.0]),
                |ui, _class| {
                    let ctx = ui.ctx().clone();
                    if ctx.input(|input| input.viewport().close_requested()) {
                        self.cancel_planning_viewport(viewport);
                        self.return_print_viewport(viewport);
                        self.gantt.owner = None;
                        return;
                    }
                    if ctx.input(|input| input.focused)
                        && !self.any_modal_open()
                        && let Some(doc) = self.doc_mut()
                        && doc.doc.active_index() != sheet
                    {
                        doc.view.commit_edit(&mut doc.doc);
                        doc.doc.set_active(sheet);
                        doc.view.reset();
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
                    self.gantt_ui(ui, &key, sheet);
                    self.planning_dialog(&ctx);
                    self.print_dialog(&ctx);
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
        self.gantt_embedded(ctx, &key, sheet);
    }

    #[cfg(target_arch = "wasm32")]
    fn gantt_embedded(&mut self, ctx: &Context, key: &str, sheet: usize) {
        let mut open = true;
        let labels = crate::i18n::dialog_labels();
        let _ = thinoda_ui::Dialog::new(egui::Id::unique(("gantt", key, sheet)), t("Gantt Chart"))
            .icon(thinoda_ui::icons::GANTT)
            .labels(&labels)
            .open(&mut open)
            .default_width(1000.0)
            .default_height(600.0)
            .scroll(false)
            .show(ctx, |ui| self.gantt_ui(ui, key, sheet));
        if !open {
            self.gantt.owner = None;
        }
    }

    fn gantt_ui(&mut self, ui: &mut egui::Ui, key: &str, sheet_index: usize) {
        let Some(index) = self.docs.iter().position(|doc| doc.key == key) else {
            return;
        };
        let blocked = self.any_modal_open();
        let touch = widgets::touch_activation_allowed(ui.ctx());
        let mut task_edit = None;
        let mut calendar = false;
        let mut undo = false;
        let mut redo = false;
        let mut print = false;
        let mut export = None;
        let mut selected = None;
        let mut task_change = None;
        let mut interaction_error = None;
        let revision = self.docs[index].doc.state_id();
        ui.add_enabled_ui(!blocked, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading(t("Gantt Chart"));
                calendar = ui.button(t("Work Calendar")).clicked() && touch;
                undo = ui.button(t("Undo")).clicked() && touch;
                redo = ui.button(t("Redo")).clicked() && touch;
                print = ui.button(t("Print")).clicked() && touch;
                ui.add_enabled_ui(self.gantt.output.is_none(), |ui| {
                    ui.menu_button(t("Export"), |ui| {
                        for extension in ["svg", "png", "pdf", "csv"] {
                            if ui.button(extension.to_uppercase()).clicked() && touch {
                                export = Some(extension);
                                ui.close();
                            }
                        }
                    });
                });
                let label = ui.label(t("Timeline zoom"));
                ui.add(egui::Slider::new(&mut self.gantt.day_width, 4.0..=48.0))
                    .labelled_by(label.id);
            });
            let doc = &self.docs[index];
            let sheet = &doc.doc.workbook().sheets[sheet_index];
            ui.label(&sheet.title);
            ui.label(t("Select a task to edit dates, progress and dependencies. Changes update the map and Undo history."));
            if sheet.scheduling.tasks.is_empty() {
                ui.label(t("No scheduled tasks yet. Select a topic in the map and choose Insert > Task Planning."));
                return;
            }
            if sheet.scheduling.tasks.len() > 1000 {
                ui.colored_label(ui.visuals().error_fg_color, t("Too many tasks for the interactive chart. Use Gantt export."));
                return;
            }
            let schedule = match sheet.compute_schedule() {
                Ok(schedule) => schedule,
                Err(error) => { ui.colored_label(ui.visuals().error_fg_color, error.to_string()); return; }
            };
            let range = timeline_range(schedule.values().copied());
            let origin = range.map(|(start, _)| start);
            let days = range.map_or(1, |(start, finish)| start.days_until(finish) + 1);
            let width = (days as f32 * self.gantt.day_width).max(400.0);
            if width > 100_000.0 {
                ui.colored_label(ui.visuals().error_fg_color, t("Timeline too wide. Reduce zoom or narrow the schedule."));
                return;
            }
            let palette = Palette::of(ui.ctx());
            let mut bars = std::collections::BTreeMap::new();
            egui::ScrollArea::both().id_salt(("gantt-scroll", key, sheet_index))
                .show(ui, |ui| {
                    ui.set_min_width(560.0 + width);
                    ui.horizontal(|ui| {
                        ui.add_sized([240.0, 24.0], egui::Label::new(RichText::new(t("Task")).strong()));
                        ui.add_sized([90.0, 24.0], egui::Label::new(t("Start date")));
                        ui.add_sized([90.0, 24.0], egui::Label::new(t("Due date")));
                        ui.add_sized([100.0, 24.0], egui::Label::new(t("Progress")));
                        let (axis, _) = ui.allocate_exact_size(egui::vec2(width, 24.0), egui::Sense::hover());
                        if let Some((start, _)) = range {
                            let step = (90.0 / self.gantt.day_width).ceil() as i32;
                            for day in (0..days).step_by(step.max(1) as usize) {
                                if let Ok(date) = start.checked_add_days(day) {
                                    let x = axis.left() + day as f32 * self.gantt.day_width;
                                    ui.painter().text(egui::pos2(x, axis.center().y), egui::Align2::LEFT_CENTER, date.to_string(), egui::FontId::proportional(11.0), ui.visuals().text_color());
                                }
                            }
                        } else {
                            ui.painter().text(axis.left_center(), egui::Align2::LEFT_CENTER, t("Unscheduled"), egui::FontId::proportional(12.0), ui.visuals().text_color());
                        }
                    });
                    for (topic, task) in &sheet.scheduling.tasks {
                        let computed = schedule.get(topic).copied();
                        let title = sheet.topic(*topic).map(|topic| topic.title.as_str()).unwrap_or("");
                        ui.horizontal(|ui| {
                            let response = ui.add_sized([240.0, 30.0],
                                egui::Button::new(title).selected(doc.view.primary() == Some(*topic)));
                            if response.clicked() && touch { selected = Some(*topic); task_edit = Some(*topic); }
                            let date = |date: Option<LocalDate>| date.map(|date| date.to_string()).unwrap_or_else(|| "-".into());
                            ui.add_sized([90.0, 30.0], egui::Label::new(date(computed.and_then(|task| task.start_date))));
                            ui.add_sized([90.0, 30.0], egui::Label::new(date(computed.and_then(|task| task.finish_date))));
                            ui.push_id(("gantt-progress", topic), |ui| {
                                ui.set_width(100.0);
                                let mut progress = f32::from(task.progress_percent);
                                let owner = egui::Id::unique(("gantt-progress-owner", key, sheet_index, topic, doc.doc.state_id()));
                                let enabled_owner = (!ui.input(|input| input.key_pressed(egui::Key::Escape) || !input.focused)).then_some(owner);
                                let (_, commit) = crate::ui::touch_numeric_value(ui, &mut progress, "progress", enabled_owner, |ui, value| {
                                    ui.add_sized([90.0, 30.0], egui::Slider::new(value, 0.0..=100.0).integer().show_value(true).suffix("%"))
                                        .accessible_name(format!("{}: {title}", t("Progress")))
                                });
                                if commit && progress.round() as u8 != task.progress_percent {
                                    let mut edited = task.clone();
                                    edited.progress_percent = progress.round() as u8;
                                    task_change = Some((*topic, edited));
                                }
                            });
                            let (rect, _) = ui.allocate_exact_size(
                                egui::vec2(width, 30.0), egui::Sense::hover());
                            ui.painter().hline(rect.x_range(), rect.bottom(), ui.visuals().widgets.noninteractive.bg_stroke);
                            if let (Some(origin), Some(ComputedTask { start_date: Some(start), finish_date: Some(finish) })) = (origin, computed) {
                                let x = rect.left() + origin.days_until(start) as f32 * self.gantt.day_width;
                                let end = rect.left() + (origin.days_until(finish) + 1) as f32 * self.gantt.day_width;
                                let bar = egui::Rect::from_min_max(egui::pos2(x, rect.top()+5.0), egui::pos2(end, rect.bottom()-5.0)).intersect(rect);
                                let owner = egui::Id::unique(("gantt-bar-owner", key, sheet_index, topic, revision, self.gantt.day_width.to_bits()));
                                let mut preview = bar;
                                ui.push_id(("gantt-bar", topic), |ui| {
                                    let handle_width = bar.width().min(8.0);
                                    let body = egui::Rect::from_min_max(bar.min, bar.max - egui::vec2(handle_width, 0.0));
                                    let handle = egui::Rect::from_min_max(egui::pos2(body.right(), bar.top()), bar.max);
                                    for (resize, area, salt) in [(false, body, "move"), (true, handle, "resize")] {
                                        let mut days = 0.0;
                                        let enabled_owner = (!ui.input(|input| input.key_pressed(egui::Key::Escape) || !input.focused)).then_some(owner);
                                        let (_, commit) = crate::ui::touch_numeric_value(ui, &mut days, salt, enabled_owner, |ui, value| {
                                            let mut response = ui.interact(area, ui.scope_id().with(salt), egui::Sense::drag());
                                            if response.dragged() {
                                                *value += response.drag_delta().x / self.gantt.day_width;
                                                response.mark_changed();
                                            }
                                            response.on_hover_cursor(if resize { egui::CursorIcon::ResizeHorizontal } else { egui::CursorIcon::Grab })
                                        });
                                        if resize { preview.max.x += days * self.gantt.day_width; }
                                        else { preview = preview.translate(egui::vec2(days * self.gantt.day_width, 0.0)); }
                                        if commit && days.round() != 0.0 {
                                            match drag_task(task, ComputedTask { start_date: Some(start), finish_date: Some(finish) }, &sheet.scheduling.calendar, days.round() as i32, resize) {
                                                Ok(edited) => task_change = Some((*topic, edited)),
                                                Err(error) => interaction_error = Some(error),
                                            }
                                        }
                                    }
                                });
                                let bar = preview;
                                ui.painter().rect_filled(bar, 3, palette.surface_alt);
                                bars.insert(*topic, bar);
                                let progress = egui::Rect::from_min_size(bar.min, egui::vec2(bar.width()*f32::from(task.progress_percent)/100.0, bar.height()));
                                ui.painter().rect_filled(progress, 3, self.accent);
                                ui.painter().text(bar.center(), egui::Align2::CENTER_CENTER, format!("{}%", task.progress_percent), egui::FontId::proportional(12.0), ui.visuals().text_color());
                            }
                        });
                    }
                    for dependency in &sheet.scheduling.dependencies {
                        if let (Some(from), Some(to)) = (bars.get(&dependency.predecessor), bars.get(&dependency.successor)) {
                            let start = from.right_center();
                            let end = to.left_center();
                            let mid = (start.x + 8.0).max(end.x - 8.0);
                            let stroke = egui::Stroke::new(1.0, ui.visuals().text_color());
                            ui.painter().add(egui::Shape::line(vec![
                                start, egui::pos2(mid, start.y), egui::pos2(mid, end.y), end,
                            ], stroke));
                            ui.painter().line_segment([end, end + egui::vec2(-4.0, -3.0)], stroke);
                            ui.painter().line_segment([end, end + egui::vec2(-4.0, 3.0)], stroke);
                        }
                    }
                });
        });
        if let Some(error) = interaction_error {
            self.toasts.error(error);
        }
        if let Some((topic, task)) = task_change {
            if let Err(error) =
                self.apply_gantt_task(ui.ctx(), key, sheet_index, revision, topic, task)
            {
                self.toasts.error(error);
            }
            ui.ctx().request_repaint_of(egui::ViewportId::ROOT);
        }
        if undo || redo {
            let doc = &mut self.docs[index];
            if let Err(error) = doc.flush_inspector_edit(ui.ctx()) {
                self.toasts.error(error);
            } else {
                doc.view.commit_edit(&mut doc.doc);
                if undo {
                    doc.doc.undo();
                } else {
                    doc.doc.redo();
                }
                ui.ctx().request_repaint_of(egui::ViewportId::ROOT);
            }
        }
        if let Some(extension) = export {
            self.begin_gantt_export(ui.ctx(), key, sheet_index, extension);
        }
        if task_edit.is_some() || calendar || print {
            let active = self.active;
            self.active = index;
            if self.docs[index].doc.active_index() != sheet_index {
                self.docs[index].view.reset();
            }
            self.docs[index].doc.set_active(sheet_index);
            if let Some(topic) = selected {
                self.docs[index].view.select_only(topic);
            }
            if print {
                self.begin_gantt_print(ui.ctx());
            } else {
                self.exec(
                    ui.ctx(),
                    if calendar {
                        Cmd::WorkCalendar
                    } else {
                        Cmd::TaskPlanning
                    },
                );
            }
            self.active = active;
            ui.ctx().request_repaint_of(egui::ViewportId::ROOT);
        }
    }

    fn begin_gantt_export(
        &mut self,
        ctx: &Context,
        key: &str,
        sheet: usize,
        extension: &'static str,
    ) {
        let Some(doc) = self.docs.iter_mut().find(|doc| doc.key == key) else {
            return;
        };
        if let Err(error) = doc.flush_inspector_edit(ctx) {
            self.toasts.error(error);
            return;
        }
        doc.view.commit_edit(&mut doc.doc);
        let Some(sheet) = doc.doc.workbook().sheets.get(sheet).cloned() else {
            return;
        };
        let Some(path) = crate::files::save_target(
            Some(t("Export")),
            &[(extension, &[extension])],
            &format!("gantt.{extension}"),
            None,
            crate::files::Target::Download,
        ) else {
            return;
        };
        let repaint = ctx.clone();
        let (sender, receiver) = mpsc::channel();
        self.gantt.output = Some(receiver);
        if let Err(error) =
            thinoda_platform::system::run_background("thinoda-gantt-export", move || {
                let result = Self::gantt_bytes(&sheet, extension).and_then(|bytes| {
                    thinoda_platform::fs::atomic_write(&path, &bytes, false)
                        .map_err(|error| error.to_string())?;
                    Ok(path)
                });
                let _ = sender.send(result);
                repaint.request_repaint_of(egui::ViewportId::ROOT);
            })
        {
            self.gantt.output = None;
            self.toasts
                .error(format!("{}: {error}", t("Export failed")));
        }
    }
    fn gantt_bytes(sheet: &thinoda_core::Sheet, extension: &str) -> Result<Vec<u8>, String> {
        if extension == "csv" {
            return thinoda_io::export_gantt_csv(sheet).map_err(crate::doc::export_error);
        }
        let document = thinoda_io::render_gantt(sheet).map_err(crate::doc::export_error)?;
        match extension {
            "svg" => Ok(document.svg.into_bytes()),
            "png" => document.png(1.0).map_err(|error| error.to_string()),
            "pdf" => document.pdf().map_err(|error| error.to_string()),
            _ => Err(format!("Unsupported Gantt output: {extension}")),
        }
    }

    fn apply_gantt_task(
        &mut self,
        ctx: &Context,
        key: &str,
        sheet: usize,
        revision: u64,
        topic: TopicId,
        task: Task,
    ) -> Result<(), String> {
        let doc = self
            .docs
            .iter_mut()
            .find(|doc| doc.key == key)
            .ok_or_else(|| t("The document changed. Try again.").to_string())?;
        doc.flush_inspector_edit(ctx)?;
        doc.view.commit_edit(&mut doc.doc);
        if doc.doc.state_id() != revision || sheet >= doc.doc.workbook().sheets.len() {
            return Err(t("The document changed. Try again.").into());
        }
        doc.doc
            .edit_book("Task planning", |book| {
                book.sheets[sheet]
                    .set_task(topic, Some(task))
                    .map_err(|error| thinoda_core::EditError::Invalid(error.to_string()))
            })
            .map_err(|error| error.to_string())
    }
}

fn drag_task(
    task: &Task,
    computed: ComputedTask,
    calendar: &WorkCalendar,
    days: i32,
    resize: bool,
) -> Result<Task, String> {
    let start = computed
        .start_date
        .ok_or_else(|| t("Set a start date before a due date.").to_string())?;
    let mut edited = task.clone();
    if resize {
        let finish = computed
            .finish_date
            .ok_or_else(|| t("Set a start date before a due date.").to_string())?
            .checked_add_days(days)
            .map_err(|error| error.to_string())?;
        edited.duration_days = Some(
            calendar
                .working_duration(start, finish)
                .map_err(|error| error.to_string())?,
        );
    } else {
        edited.start_date = Some(
            start
                .checked_add_days(days)
                .map_err(|error| error.to_string())?,
        );
    }
    Ok(edited)
}

fn timeline_range(tasks: impl Iterator<Item = ComputedTask>) -> Option<(LocalDate, LocalDate)> {
    tasks
        .filter_map(|task| Some((task.start_date?, task.finish_date?)))
        .fold(None, |range, (start, finish)| {
            Some(match range {
                Some((first, last)) => (first.min(start), last.max(finish)),
                None => (start, finish),
            })
        })
}
