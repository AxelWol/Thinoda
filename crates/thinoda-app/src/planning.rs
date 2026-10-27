//! Explicit, revision-checked planning drafts. Cancel never changes the document.

use std::collections::BTreeSet;

use chrono::Datelike;
use egui::Context;
use thinoda_core::{LocalDate, Task, TopicId, Weekday, WorkCalendar};
use thinoda_ui::{CalendarDate, widgets};

use crate::{app::ThinodaApp, i18n::t};

#[cfg(test)]
#[path = "planning_tests.rs"]
mod tests;

#[derive(Default)]
pub(crate) struct PlanningState {
    editor: Option<PlanningEditor>,
}

struct PlanningEditor {
    viewport: egui::ViewportId,
    document: String,
    sheet: usize,
    revision: u64,
    draft: PlanningDraft,
    error: Option<String>,
    composing: bool,
}

enum PlanningDraft {
    Task {
        topic: TopicId,
        task: Task,
        start: String,
        finish: String,
        predecessors: BTreeSet<TopicId>,
    },
    Calendar {
        calendar: WorkCalendar,
        date: String,
        working: bool,
    },
}

impl PlanningState {
    pub(crate) fn editing(&self) -> bool {
        self.editor.is_some()
    }
}

impl ThinodaApp {
    pub(crate) fn begin_task_planning(&mut self, ctx: &Context, calendar: bool) {
        let Some(doc) = self.doc_mut() else { return };
        doc.view.commit_edit(&mut doc.doc);
        let draft = if calendar {
            PlanningDraft::Calendar {
                calendar: doc.doc.sheet().scheduling.calendar.clone(),
                date: String::new(),
                working: false,
            }
        } else {
            let Some(topic) = doc.view.primary() else {
                return;
            };
            let task = doc
                .doc
                .sheet()
                .scheduling
                .tasks
                .get(&topic)
                .cloned()
                .unwrap_or_default();
            PlanningDraft::Task {
                topic,
                start: task
                    .start_date
                    .map(|date| date.to_string())
                    .unwrap_or_default(),
                finish: String::new(),
                task,
                predecessors: doc
                    .doc
                    .sheet()
                    .scheduling
                    .dependencies
                    .iter()
                    .filter(|dependency| dependency.successor == topic)
                    .map(|dependency| dependency.predecessor)
                    .collect(),
            }
        };
        self.planning.editor = Some(PlanningEditor {
            viewport: ctx.viewport_id(),
            document: doc.key.clone(),
            sheet: doc.doc.active_index(),
            revision: doc.doc.state_id(),
            draft,
            error: None,
            composing: false,
        });
        ctx.request_repaint();
    }

    fn apply_planning_draft(
        &mut self,
        editor: &PlanningEditor,
        remove: bool,
    ) -> Result<(), String> {
        let doc = self
            .docs
            .iter_mut()
            .find(|doc| doc.key == editor.document)
            .ok_or_else(|| t("The document changed. Try again.").to_string())?;
        if doc.doc.active_index() != editor.sheet || doc.doc.state_id() != editor.revision {
            return Err(t("The document changed. Try again.").into());
        }
        match &editor.draft {
            PlanningDraft::Task {
                topic,
                task,
                start,
                finish,
                predecessors,
            } => {
                if remove {
                    return doc
                        .doc
                        .set_task(*topic, None)
                        .map_err(|error| error.to_string());
                }
                let mut task = task.clone();
                task.start_date = if start.trim().is_empty() {
                    None
                } else {
                    Some(
                        start
                            .trim()
                            .parse::<LocalDate>()
                            .map_err(|error| error.to_string())?,
                    )
                };
                if !finish.trim().is_empty() {
                    let finish = finish
                        .trim()
                        .parse::<LocalDate>()
                        .map_err(|error| error.to_string())?;
                    let start = task
                        .start_date
                        .ok_or_else(|| t("Set a start date before a due date.").to_string())?;
                    task.duration_days = Some(
                        doc.doc
                            .sheet()
                            .scheduling
                            .calendar
                            .working_duration(start, finish)
                            .map_err(|error| error.to_string())?,
                    );
                }
                doc.doc
                    .edit_sheet("Task planning", |sheet| {
                        let previous: Vec<_> = sheet
                            .scheduling
                            .dependencies
                            .iter()
                            .filter(|dependency| dependency.successor == *topic)
                            .map(|dependency| dependency.predecessor)
                            .collect();
                        for predecessor in previous {
                            if !predecessors.contains(&predecessor) {
                                sheet.remove_task_dependency(predecessor, *topic).map_err(
                                    |error| thinoda_core::EditError::Invalid(error.to_string()),
                                )?;
                            }
                        }
                        sheet
                            .set_task(*topic, Some(task.clone()))
                            .map_err(|error| thinoda_core::EditError::Invalid(error.to_string()))?;
                        for predecessor in predecessors {
                            if !sheet.scheduling.dependencies.iter().any(|dependency| {
                                dependency.predecessor == *predecessor
                                    && dependency.successor == *topic
                            }) {
                                sheet.add_task_dependency(*predecessor, *topic).map_err(
                                    |error| thinoda_core::EditError::Invalid(error.to_string()),
                                )?;
                            }
                        }
                        Ok(())
                    })
                    .map_err(|error| error.to_string())?;
            }
            PlanningDraft::Calendar {
                calendar,
                date,
                working,
            } => {
                let mut calendar = calendar.clone();
                if !date.trim().is_empty() {
                    let date = date
                        .trim()
                        .parse::<LocalDate>()
                        .map_err(|error| error.to_string())?;
                    calendar.exceptions.insert(date, *working);
                }
                doc.doc
                    .set_work_calendar(calendar)
                    .map_err(|error| error.to_string())?;
            }
        }
        Ok(())
    }

    pub(crate) fn planning_dialog(&mut self, ctx: &Context) {
        if self
            .planning
            .editor
            .as_ref()
            .is_some_and(|editor| editor.viewport != ctx.viewport_id())
        {
            return;
        }
        let Some(mut editor) = self.planning.editor.take() else {
            return;
        };
        let ime_event = ctx.input(|input| {
            input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::Ime(_)))
        });
        let focus = ctx
            .memory(|memory| memory.focused())
            .unwrap_or(egui::Id::unique("planning-editor-ime"));
        thinoda_canvas::protect_ime_candidate_keys(ctx, focus, &mut editor.composing);
        let Some(doc) = self.docs.iter().find(|doc| doc.key == editor.document) else {
            self.toasts.warning(t("The document changed. Try again."));
            return;
        };
        let Some(sheet) = doc.doc.workbook().sheets.get(editor.sheet) else {
            self.toasts.warning(t("The document changed. Try again."));
            return;
        };
        let touch = widgets::touch_activation_allowed(ctx);
        let calendar_labels = crate::i18n::calendar_labels();
        let local = chrono::Local::now();
        let today = CalendarDate::new(local.year(), local.month() as u8, local.day() as u8);
        let mut apply = false;
        let mut remove = false;
        let mut cancel = false;
        let title = match editor.draft {
            PlanningDraft::Task { .. } => "Task Planning",
            PlanningDraft::Calendar { .. } => "Work Calendar",
        };
        let bounds = ctx.content_rect().shrink(12.0);
        let labels = crate::i18n::dialog_labels();
        let icon = if matches!(&editor.draft, PlanningDraft::Task { .. }) {
            thinoda_ui::icons::TASK_PLANNING
        } else {
            thinoda_ui::icons::CALENDAR
        };
        let response = thinoda_ui::Dialog::new(egui::Id::unique("planning-editor"), t(title))
            .icon(icon)
            .labels(&labels)
            .modal(true)
            .close_on_backdrop(true)
            .default_width(480.0_f32.min(bounds.width()))
            .default_height(560.0)
            .min_size(egui::vec2(320.0, 260.0))
            .scroll(false)
            .show(ctx, |ui| {
                let footer = 100.0;
                egui::ScrollArea::vertical()
                    .max_height((bounds.height() - footer).max(40.0))
                    .show(ui, |ui| match &mut editor.draft {
                        PlanningDraft::Task {
                            topic,
                            task,
                            start,
                            finish,
                            predecessors,
                        } => {
                            if let Some(topic) = sheet.topic(*topic) {
                                ui.label(&topic.title);
                            }
                            ui.label(t(
                                "Dates use YYYY-MM-DD. Duration counts inclusive working days.",
                            ));
                            for (label, value) in [("Start date", start), ("Due date", finish)] {
                                let label = ui.label(t(label));
                                thinoda_ui::date_field(
                                    ui,
                                    label.id,
                                    value,
                                    &calendar_labels,
                                    today,
                                    |date| {
                                        u16::try_from(date.year)
                                            .ok()
                                            .and_then(|year| {
                                                LocalDate::new(year, date.month, date.day).ok()
                                            })
                                            .is_some_and(|date| {
                                                sheet.scheduling.calendar.is_working_day(date)
                                            })
                                    },
                                )
                                .text
                                .labelled_by(label.id);
                            }
                            let owner = Some(egui::Id::unique((
                                "planning-values",
                                &editor.document,
                                editor.sheet,
                                editor.revision,
                                editor.viewport,
                                *topic,
                            )));
                            let mut duration = f32::from(task.duration_days.unwrap_or(0));
                            let label = ui.label(t("Working days (0 = unscheduled)"));
                            let (_, commit) = crate::ui::touch_numeric_value(
                                ui,
                                &mut duration,
                                "duration",
                                owner,
                                |ui, value| {
                                    ui.add(
                                        egui::DragValue::new(value).range(0..=3650).max_decimals(0),
                                    )
                                    .labelled_by(label.id)
                                },
                            );
                            if commit {
                                task.duration_days =
                                    (duration > 0.0).then_some(duration.round() as u16);
                            }
                            let label = ui.label(t("Progress"));
                            let mut progress = f32::from(task.progress_percent);
                            let (_, commit) = crate::ui::touch_numeric_value(
                                ui,
                                &mut progress,
                                "progress",
                                owner,
                                |ui, value| {
                                    ui.add(
                                        egui::Slider::new(value, 0.0..=100.0).integer().suffix("%"),
                                    )
                                    .labelled_by(label.id)
                                },
                            );
                            if commit {
                                task.progress_percent = progress.round() as u8;
                            }
                            let mut priority = f32::from(task.priority.unwrap_or(0));
                            let label = ui.label(t("Priority (0 = none)"));
                            let (_, commit) = crate::ui::touch_numeric_value(
                                ui,
                                &mut priority,
                                "priority",
                                owner,
                                |ui, value| {
                                    ui.add(
                                        egui::DragValue::new(value).range(0..=255).max_decimals(0),
                                    )
                                    .labelled_by(label.id)
                                },
                            );
                            if commit {
                                task.priority = (priority > 0.0).then_some(priority.round() as u8);
                            }
                            ui.separator();
                            ui.strong(t("Finish-to-start predecessors"));
                            for (id, _) in sheet
                                .scheduling
                                .tasks
                                .iter()
                                .filter(|(id, _)| **id != *topic)
                            {
                                let mut selected = predecessors.contains(id);
                                let label = sheet
                                    .topic(*id)
                                    .map(|topic| topic.title.clone())
                                    .unwrap_or_else(|| id.to_string());
                                if ui.checkbox(&mut selected, label).changed() && touch {
                                    if selected {
                                        predecessors.insert(*id);
                                    } else {
                                        predecessors.remove(id);
                                    }
                                }
                            }
                            ui.label(t(
                            "Removing a task also removes its dependencies. To-Do stays unchanged.",
                        ));
                            remove = ui.button(t("Remove Task")).clicked() && touch;
                        }
                        PlanningDraft::Calendar {
                            calendar,
                            date,
                            working,
                        } => {
                            ui.label(t(
                            "This calendar belongs to the current sheet and is saved with the map.",
                        ));
                            for weekday in Weekday::ALL {
                                let mut enabled = calendar.working_weekdays.contains(&weekday);
                                if ui.checkbox(&mut enabled, t(weekday.name())).changed() && touch {
                                    if enabled {
                                        calendar.working_weekdays.insert(weekday);
                                    } else {
                                        calendar.working_weekdays.remove(&weekday);
                                    }
                                }
                            }
                            ui.separator();
                            ui.strong(t("Dated exceptions"));
                            let label = ui.label(t("Exception date"));
                            thinoda_ui::date_field(
                                ui,
                                label.id,
                                date,
                                &calendar_labels,
                                today,
                                |_| true,
                            )
                            .text
                            .labelled_by(label.id);
                            let before_working = *working;
                            ui.checkbox(working, t("Working day"));
                            if !touch {
                                *working = before_working;
                            }
                            if ui.button(t("Add Exception")).clicked() && touch {
                                match date.trim().parse::<LocalDate>() {
                                    Ok(parsed) => {
                                        calendar.exceptions.insert(parsed, *working);
                                        date.clear();
                                    }
                                    Err(error) => editor.error = Some(error.to_string()),
                                }
                            }
                            let mut delete = None;
                            for (date, working) in &calendar.exceptions {
                                ui.horizontal(|ui| {
                                    ui.label(format!(
                                        "{date}: {}",
                                        t(if *working {
                                            "Working day"
                                        } else {
                                            "Non-working day"
                                        })
                                    ));
                                    if ui.button(format!("{} {date}", t("Remove"))).clicked()
                                        && touch
                                    {
                                        delete = Some(*date);
                                    }
                                });
                            }
                            if let Some(date) = delete {
                                calendar.exceptions.remove(&date);
                            }
                        }
                    });
                if let Some(error) = &editor.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                ui.horizontal_wrapped(|ui| {
                    apply = ui.button(t("Apply")).clicked() && touch;
                    cancel = ui.button(t("Cancel")).clicked() && touch;
                });
            });
        if (apply || remove) && !ime_event && !editor.composing {
            match self.apply_planning_draft(&editor, remove) {
                Ok(()) => {
                    ctx.request_repaint();
                    return;
                }
                Err(error) => {
                    log::warn!("Could not apply planning draft: {error}");
                    editor.error = Some(error);
                }
            }
        }
        if !(cancel || response.close_requested) || ime_event || editor.composing {
            self.planning.editor = Some(editor);
        }
    }

    pub(crate) fn return_planning_viewport(&mut self, viewport: egui::ViewportId) {
        if let Some(editor) = &mut self.planning.editor
            && editor.viewport == viewport
        {
            editor.viewport = egui::ViewportId::ROOT;
        }
    }

    pub(crate) fn cancel_planning_viewport(&mut self, viewport: egui::ViewportId) {
        if self
            .planning
            .editor
            .as_ref()
            .is_some_and(|editor| editor.viewport == viewport)
        {
            self.planning.editor = None;
        }
    }
}
