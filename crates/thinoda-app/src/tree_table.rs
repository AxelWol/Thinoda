//! Task projection over real topics. View operations never edit the sheet.

use std::collections::{BTreeMap, BTreeSet};

use egui::{Context, Key, Modifiers};
use thinoda_core::{ComputedTask, Sheet, Structure, TopicId};
use thinoda_platform::settings::TreeTableSort;
use thinoda_ui::widgets;

use crate::{
    app::{CenterMode, Cmd, ThinodaApp},
    i18n::t,
};

pub(crate) const HELP: &str = "Tree Table edits real topics and tasks. Tab moves between cells; arrows navigate; Enter edits or applies; Escape cancels. Blank cells do not schedule untouched topics. Sorting and columns are local views, not hierarchy edits.";
pub(crate) const OUTPUT: &str = "Map image, PDF and print output render the map, not this table. Use Gantt export for scheduled task output.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Column {
    Title,
    Start,
    Due,
    Duration,
    Progress,
}

impl Column {
    const ALL: [Self; 5] = [
        Self::Title,
        Self::Start,
        Self::Due,
        Self::Duration,
        Self::Progress,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::Title => "Topic title",
            Self::Start => "Start date",
            Self::Due => "Due date",
            Self::Duration => "Duration",
            Self::Progress => "Progress",
        }
    }
    fn width(self) -> f32 {
        if self == Self::Title { 280.0 } else { 145.0 }
    }
}

#[derive(Clone, PartialEq, Eq)]
struct Owner {
    document: String,
    sheet: usize,
    root: TopicId,
    viewport: egui::ViewportId,
}

struct CellDraft {
    editor_id: egui::Id,
    owner: Owner,
    revision: u64,
    topic: TopicId,
    column: Column,
    value: String,
    error: Option<String>,
    composing: bool,
    focus: bool,
}

#[derive(Default)]
pub(crate) struct TreeTableState {
    pub(crate) map_preview: bool,
    owner: Option<Owner>,
    collapsed: BTreeSet<TopicId>,
    primary: Option<TopicId>,
    draft: Option<CellDraft>,
    focus: Option<(TopicId, Column)>,
    draft_sequence: u64,
}

fn missing_last<T>(value: Option<T>) -> (bool, Option<T>) {
    (value.is_none(), value)
}

fn rows(
    sheet: &Sheet,
    schedule: &BTreeMap<TopicId, ComputedTask>,
    collapsed: &BTreeSet<TopicId>,
    sort: TreeTableSort,
) -> Vec<(TopicId, usize)> {
    let mut stack: Vec<_> = sheet
        .top_level_ids()
        .into_iter()
        .rev()
        .map(|id| (id, 0))
        .collect();
    let mut rows = Vec::new();
    while let Some((id, depth)) = stack.pop() {
        let Some(topic) = sheet.topic(id) else {
            continue;
        };
        rows.push((id, depth));
        if collapsed.contains(&id) {
            continue;
        }
        let mut children = topic.children.clone();
        // Stable sibling-only sorting keeps the visible parent/child relationship intact.
        // None sorts after scheduled values. Equal values retain document order.
        children.sort_by(|a, b| {
            let task = |id| sheet.scheduling.tasks.get(&id);
            let computed = |id| schedule.get(&id);
            match sort {
                TreeTableSort::Hierarchy => std::cmp::Ordering::Equal,
                TreeTableSort::Title => sheet.topics[a].title.cmp(&sheet.topics[b].title),
                TreeTableSort::Start => missing_last(computed(*a).and_then(|v| v.start_date))
                    .cmp(&missing_last(computed(*b).and_then(|v| v.start_date))),
                TreeTableSort::Due => missing_last(computed(*a).and_then(|v| v.finish_date))
                    .cmp(&missing_last(computed(*b).and_then(|v| v.finish_date))),
                TreeTableSort::Duration => missing_last(task(*a).and_then(|v| v.duration_days))
                    .cmp(&missing_last(task(*b).and_then(|v| v.duration_days))),
                TreeTableSort::Progress => missing_last(task(*a).map(|v| v.progress_percent))
                    .cmp(&missing_last(task(*b).map(|v| v.progress_percent))),
            }
        });
        stack.extend(children.into_iter().rev().map(|id| (id, depth + 1)));
    }
    rows
}

fn cell_value(
    sheet: &Sheet,
    schedule: &BTreeMap<TopicId, ComputedTask>,
    topic: TopicId,
    column: Column,
    editing: bool,
) -> String {
    let task = sheet.scheduling.tasks.get(&topic);
    match column {
        Column::Title => sheet.topics[&topic].title.clone(),
        // Edit the explicit constraint, display the engine's effective date.
        Column::Start if editing => task
            .and_then(|v| v.start_date)
            .map(|v| v.to_string())
            .unwrap_or_default(),
        Column::Start => schedule
            .get(&topic)
            .and_then(|v| v.start_date)
            .map(|v| v.to_string())
            .unwrap_or_default(),
        Column::Due => schedule
            .get(&topic)
            .and_then(|v| v.finish_date)
            .map(|v| v.to_string())
            .unwrap_or_default(),
        Column::Duration => task
            .and_then(|v| v.duration_days)
            .map(|v| v.to_string())
            .unwrap_or_default(),
        Column::Progress => task
            .map(|v| v.progress_percent.to_string())
            .unwrap_or_default(),
    }
}

impl ThinodaApp {
    pub(crate) fn task_table_visible(&self) -> bool {
        self.mode == CenterMode::Map
            && !self.tree_table.map_preview
            && self
                .doc()
                .is_some_and(|doc| doc.doc.sheet().structure == Structure::TreeTable)
    }

    fn table_owner(&self) -> Option<Owner> {
        self.doc().map(|doc| Owner {
            document: doc.key.clone(),
            sheet: doc.doc.active_index(),
            root: doc.doc.sheet().root,
            viewport: self.current_viewport,
        })
    }

    /// Called even when another center view is drawn. Context switches discard drafts.
    pub(crate) fn guard_table_draft(&mut self, ctx: &Context) {
        // Drawing an unfocused sibling document window is not a context switch
        // in the editor's viewport. It must neither discard nor render this draft.
        if self
            .tree_table
            .draft
            .as_ref()
            .is_some_and(|draft| draft.owner.viewport != self.current_viewport)
        {
            return;
        }
        let valid = self.tree_table.draft.as_ref().is_none_or(|draft| {
            self.task_table_visible()
                && self.table_owner().as_ref() == Some(&draft.owner)
                && self.doc().is_some_and(|doc| {
                    doc.doc.state_id() == draft.revision
                        && doc.view.primary() == Some(draft.topic)
                        && doc.doc.sheet().contains(draft.topic)
                })
        });
        if !valid {
            self.tree_table.draft = None;
        }
        if let Some(draft) = &mut self.tree_table.draft {
            thinoda_canvas::protect_ime_candidate_keys(ctx, draft.editor_id, &mut draft.composing);
        }
    }

    fn begin_table_cell(&mut self, topic: TopicId, column: Column) {
        let Some(owner) = self.table_owner() else {
            return;
        };
        self.tree_table.draft_sequence = self.tree_table.draft_sequence.wrapping_add(1);
        let sequence = self.tree_table.draft_sequence;
        let Some(doc) = self.doc_mut() else { return };
        doc.view.commit_edit(&mut doc.doc);
        doc.view.select_only(topic);
        let schedule = doc.doc.sheet().compute_schedule().unwrap_or_default();
        self.tree_table.draft = Some(CellDraft {
            // TextEdit's cursor/undo state must not leak between owners or canceled drafts.
            editor_id: egui::Id::unique((
                "tree-table-cell-editor",
                &owner.document,
                owner.sheet,
                owner.root,
                owner.viewport,
                topic,
                column.label(),
                doc.doc.state_id(),
                sequence,
            )),
            owner,
            revision: doc.doc.state_id(),
            topic,
            column,
            value: cell_value(doc.doc.sheet(), &schedule, topic, column, true),
            error: None,
            composing: false,
            focus: true,
        });
    }

    fn apply_table_cell(&mut self, ctx: &Context, draft: &CellDraft) -> Result<(), String> {
        if !self.task_table_visible()
            || self.table_owner().as_ref() != Some(&draft.owner)
            || self.doc().is_none_or(|doc| {
                doc.doc.state_id() != draft.revision || doc.view.primary() != Some(draft.topic)
            })
        {
            return Err(t("The document changed. Try again.").into());
        }
        let doc = self.doc_mut().unwrap();
        if draft.column == Column::Title {
            return doc
                .doc
                .edit_sheet("Topic title", |sheet| {
                    sheet.set_title(draft.topic, &draft.value)
                })
                .map_err(|error| error.to_string());
        }
        let sheet = doc.doc.sheet();
        let mut task = sheet
            .scheduling
            .tasks
            .get(&draft.topic)
            .cloned()
            .unwrap_or_default();
        let mut start = task.start_date.map(|v| v.to_string()).unwrap_or_default();
        let mut finish = String::new();
        match draft.column {
            Column::Title => unreachable!(),
            Column::Start => start.clone_from(&draft.value),
            Column::Due => {
                finish = draft.value.trim().into();
                if finish.is_empty() {
                    task.duration_days = None;
                }
            }
            Column::Duration => {
                let value = draft.value.trim();
                task.duration_days = if value.is_empty() || value == "0" {
                    None
                } else {
                    Some(
                        value
                            .parse::<u16>()
                            .map_err(|_| t("Invalid task value").to_string())?,
                    )
                };
            }
            Column::Progress => {
                task.progress_percent = draft
                    .value
                    .trim()
                    .parse::<u8>()
                    .map_err(|_| t("Invalid task value").to_string())?;
            }
        }
        // A blank cell on an untouched topic is a no-op, not an implicit new task.
        if !sheet.scheduling.tasks.contains_key(&draft.topic)
            && task == thinoda_core::Task::default()
            && start.trim().is_empty()
            && finish.is_empty()
        {
            return Ok(());
        }
        self.apply_table_task(ctx, draft.topic, task, start, finish)
    }

    pub(crate) fn tree_table_ui(&mut self, ui: &mut egui::Ui) {
        thinoda_ui::fonts::ensure_icon_font(ui.ctx());
        let ctx = ui.ctx().clone();
        self.guard_table_draft(&ctx);
        let touch = widgets::touch_activation_allowed(&ctx);
        let Some(owner) = self.table_owner() else {
            return;
        };
        if self.tree_table.owner.as_ref() != Some(&owner) {
            self.tree_table.owner = Some(owner.clone());
            self.tree_table.collapsed.clear();
            self.tree_table.primary = None;
            self.tree_table.focus = None;
        }
        let primary = self.doc().and_then(|doc| doc.view.primary());
        if primary != self.tree_table.primary {
            if let Some(id) = primary {
                for ancestor in self.doc().unwrap().doc.sheet().ancestors(id) {
                    self.tree_table.collapsed.remove(&ancestor);
                }
                self.tree_table.focus = Some((id, Column::Title));
            }
            self.tree_table.primary = primary;
        }
        let before = self.settings.tree_table.clone();
        let mut command = None;
        let mut apply = false;
        let mut cancel = false;
        let mut tab = None;
        // Consume completion keys before TextEdit handles Escape/Tab itself.
        if ui.is_enabled()
            && self
                .tree_table
                .draft
                .as_ref()
                .is_some_and(|draft| draft.owner == owner && !draft.composing)
            && !egui::Popup::is_any_open(&ctx)
            && !ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Ime(_))))
        {
            cancel = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
            let editor_id = self.tree_table.draft.as_ref().unwrap().editor_id;
            if ctx.memory(|m| m.has_focus(editor_id)) {
                apply = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
                let backward = ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::Tab));
                if backward || ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab)) {
                    apply = true;
                    tab = Some(backward);
                }
            }
        }
        let sorts = [
            (TreeTableSort::Hierarchy, "Document order"),
            (TreeTableSort::Title, "Topic title"),
            (TreeTableSort::Start, "Start date"),
            (TreeTableSort::Due, "Due date"),
            (TreeTableSort::Duration, "Duration"),
            (TreeTableSort::Progress, "Progress"),
        ];
        ui.horizontal_wrapped(|ui| {
            ui.heading(t("Tree Table"));
            if ui.button(t("Map")).clicked() && touch {
                self.tree_table.map_preview = true;
                cancel = true;
            }
            for (label, cmd) in [
                ("Task Planning", Cmd::TaskPlanning),
                ("Work Calendar", Cmd::WorkCalendar),
                ("Gantt Chart", Cmd::Gantt),
            ] {
                if ui.button(t(label)).clicked() && touch {
                    command = Some(cmd);
                    cancel = true;
                }
            }
            ui.menu_button(t("Table columns"), |ui| {
                for (index, column) in Column::ALL[1..].iter().enumerate() {
                    let old = self.settings.tree_table.columns[index];
                    ui.checkbox(
                        &mut self.settings.tree_table.columns[index],
                        t(column.label()),
                    );
                    if !touch {
                        self.settings.tree_table.columns[index] = old;
                    }
                }
            });
            egui::ComboBox::from_id_salt("table-sort")
                .selected_text(t(sorts
                    .iter()
                    .find(|(sort, _)| *sort == self.settings.tree_table.sort)
                    .unwrap()
                    .1))
                .show_ui(ui, |ui| {
                    for (sort, label) in sorts {
                        if ui
                            .selectable_label(self.settings.tree_table.sort == sort, t(label))
                            .clicked()
                            && touch
                        {
                            self.settings.tree_table.sort = sort;
                        }
                    }
                })
                .response
                .accessible_name(t("Sort siblings (view only)"));
        });
        if self.settings.tree_table != before {
            self.save_settings();
            cancel = true;
        }
        egui::CollapsingHeader::new(t("Tree Table help")).show(ui, |ui| {
            ui.label(t(HELP));
            ui.label(t(
                "Dates use YYYY-MM-DD. Duration counts inclusive working days.",
            ));
            ui.label(t(
                "Displayed dates are computed; editing Start date changes its explicit constraint.",
            ));
            ui.label(t(OUTPUT));
        });
        if let Some(draft) = self
            .tree_table
            .draft
            .as_ref()
            .filter(|draft| draft.owner == owner)
        {
            ui.horizontal_wrapped(|ui| {
                ui.label(t(draft.column.label()));
                apply |= ui.button(t("Apply")).clicked() && touch;
                cancel |= ui.button(t("Cancel")).clicked() && touch;
                if let Some(error) = &draft.error {
                    ui.colored_label(
                        ui.visuals().error_fg_color,
                        format!("{}: {error}", t("Invalid task value")),
                    );
                    match draft.column {
                        Column::Start | Column::Due => {
                            ui.label(t(
                                "Dates use YYYY-MM-DD. Duration counts inclusive working days.",
                            ));
                        }
                        Column::Duration => {
                            ui.label(format!("{}: 0–3650", t("Working days (0 = unscheduled)")));
                        }
                        Column::Progress => {
                            ui.label(format!("{}: 0–100%", t("Progress")));
                        }
                        Column::Title => {}
                    }
                }
            });
        }
        let doc = self.doc().unwrap();
        let sheet = doc.doc.sheet().clone();
        let schedule = match sheet.compute_schedule() {
            Ok(value) => value,
            Err(error) => {
                ui.colored_label(ui.visuals().error_fg_color, error.to_string());
                BTreeMap::new()
            }
        };
        let rows = rows(
            &sheet,
            &schedule,
            &self.tree_table.collapsed,
            self.settings.tree_table.sort,
        );
        let columns: Vec<_> = Column::ALL
            .into_iter()
            .enumerate()
            .filter_map(|(index, column)| {
                (index == 0 || self.settings.tree_table.columns[index - 1]).then_some(column)
            })
            .collect();
        let mut edit = None;
        let mut select = None;
        let mut toggle = None;
        let mut focused = None;
        let request_focus = self.tree_table.focus.take();
        egui::ScrollArea::both()
            .id_salt(("task-table", &owner.document, owner.sheet, owner.root))
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                egui::Grid::new(("task-grid", &owner.document, owner.sheet, owner.root))
                    .striped(true)
                    .spacing([6.0, 3.0])
                    .show(ui, |ui| {
                        for column in &columns {
                            ui.add_sized(
                                [column.width(), 26.0],
                                egui::Label::new(egui::RichText::new(t(column.label())).strong())
                                    .truncate(),
                            );
                        }
                        ui.end_row();
                        for (topic, depth) in &rows {
                            for column in &columns {
                                ui.push_id((topic, column.label()), |ui| {
                                    ui.horizontal(|ui| {
                                        let mut width = column.width();
                                        if *column == Column::Title {
                                            let indent = (*depth as f32 * 14.0).min(84.0);
                                            ui.add_space(indent);
                                            width -= indent + 30.0;
                                            let has_children =
                                                !sheet.topics[topic].children.is_empty();
                                            let collapse = ui
                                                .add_enabled(
                                                    has_children,
                                                    egui::Button::new(
                                                        if self.tree_table.collapsed.contains(topic)
                                                        {
                                                            thinoda_ui::icons::CHEVRON_RIGHT
                                                        } else {
                                                            thinoda_ui::icons::CHEVRON_DOWN
                                                        },
                                                    )
                                                    .small(),
                                                )
                                                .accessible_name(format!(
                                                    "{}: {}",
                                                    t("Collapse / Expand"),
                                                    sheet.topics[topic].title
                                                ));
                                            if collapse.clicked() && touch {
                                                toggle = Some(*topic);
                                            }
                                        }
                                        let name = format!(
                                            "{}: {}",
                                            t(column.label()),
                                            sheet.topics[topic].title
                                        );
                                        let drafting =
                                            self.tree_table.draft.as_mut().filter(|draft| {
                                                draft.owner == owner
                                                    && draft.topic == *topic
                                                    && draft.column == *column
                                            });
                                        if let Some(draft) = drafting {
                                            let response = ui
                                                .add_sized(
                                                    [width, 28.0],
                                                    egui::TextEdit::singleline(&mut draft.value)
                                                        .id(draft.editor_id)
                                                        .lock_focus(true),
                                                )
                                                .accessible_name(&name);
                                            if draft.focus {
                                                response.request_focus();
                                                response.scroll_to_me(None);
                                                draft.focus = false;
                                            }
                                        } else {
                                            let value = cell_value(
                                                &sheet, &schedule, *topic, *column, false,
                                            );
                                            let display = if value.is_empty() {
                                                if *column == Column::Title {
                                                    " ".into()
                                                } else {
                                                    t("Unscheduled").to_string()
                                                }
                                            } else if *column == Column::Progress {
                                                format!("{value}%")
                                            } else {
                                                value
                                            };
                                            let response = ui
                                                .add_sized(
                                                    [width, 28.0],
                                                    egui::Button::new(&display)
                                                        .selected(primary == Some(*topic))
                                                        .truncate(),
                                                )
                                                .accessible_name(&name)
                                                .on_hover_text(&display);
                                            if request_focus == Some((*topic, *column)) {
                                                response.request_focus();
                                                response.scroll_to_me(None);
                                            }
                                            if response.has_focus() {
                                                focused = Some((*topic, *column));
                                                if response.gained_focus() {
                                                    select = Some(*topic);
                                                    response.scroll_to_me(None);
                                                }
                                            }
                                            if response.clicked() && touch {
                                                select = Some(*topic);
                                                edit = Some((*topic, *column));
                                            }
                                        }
                                    });
                                });
                            }
                            ui.end_row();
                        }
                    });
            });
        if ui.is_enabled()
            && !ctx.text_edit_focused()
            && !egui::Popup::is_any_open(&ctx)
            && let Some((topic, column)) = focused
        {
            let row = rows.iter().position(|(id, _)| *id == topic).unwrap_or(0);
            let col = columns.iter().position(|c| *c == column).unwrap_or(0);
            let movement = ctx.input_mut(|i| {
                if i.consume_key(Modifiers::NONE, Key::ArrowUp) {
                    Some((row.saturating_sub(1), col))
                } else if i.consume_key(Modifiers::NONE, Key::ArrowDown) {
                    Some(((row + 1).min(rows.len() - 1), col))
                } else if i.consume_key(Modifiers::NONE, Key::ArrowLeft) {
                    Some((row, col.saturating_sub(1)))
                } else if i.consume_key(Modifiers::NONE, Key::ArrowRight) {
                    Some((row, (col + 1).min(columns.len() - 1)))
                } else {
                    None
                }
            });
            if let Some((row, col)) = movement {
                select = Some(rows[row].0);
                self.tree_table.focus = Some((rows[row].0, columns[col]));
                cancel = true;
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F2)) {
                edit = Some((topic, column));
            }
        }
        if let Some(topic) = select {
            if primary != Some(topic) {
                cancel = true;
            }
            self.doc_mut().unwrap().view.select_only(topic);
            self.tree_table.primary = Some(topic);
        }
        if let Some(topic) = toggle {
            if !self.tree_table.collapsed.remove(&topic) {
                self.tree_table.collapsed.insert(topic);
            }
            self.doc_mut().unwrap().view.select_only(topic);
            self.tree_table.primary = Some(topic);
            cancel = true;
        }
        if cancel {
            self.tree_table.draft = None;
        } else if apply
            && ui.is_enabled()
            && self
                .tree_table
                .draft
                .as_ref()
                .is_some_and(|draft| draft.owner == owner && !draft.composing)
            && !ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Ime(_))))
            && let Some(mut draft) = self.tree_table.draft.take()
        {
            match self.apply_table_cell(&ctx, &draft) {
                Ok(()) => {
                    let mut focus = (draft.topic, draft.column);
                    if let Some(backward) = tab {
                        let row = rows
                            .iter()
                            .position(|(id, _)| *id == draft.topic)
                            .unwrap_or(0);
                        let col = columns.iter().position(|c| *c == draft.column).unwrap_or(0);
                        let index = row * columns.len() + col;
                        let index = if backward {
                            index.saturating_sub(1)
                        } else {
                            (index + 1).min(rows.len() * columns.len() - 1)
                        };
                        focus = (
                            rows[index / columns.len()].0,
                            columns[index % columns.len()],
                        );
                        self.doc_mut().unwrap().view.select_only(focus.0);
                        self.tree_table.primary = Some(focus.0);
                    }
                    self.tree_table.focus = Some(focus);
                }
                Err(error) => {
                    draft.error = Some(error);
                    self.tree_table.draft = Some(draft);
                }
            }
        }
        if let Some((topic, column)) = edit.filter(|_| ui.is_enabled()) {
            self.begin_table_cell(topic, column);
        }
        if let Some(command) = command {
            self.exec(&ctx, command);
        }
    }
}

#[cfg(test)]
#[path = "tree_table_tests.rs"]
mod tests;
