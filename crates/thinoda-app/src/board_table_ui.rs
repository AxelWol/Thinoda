//! Board table insertion and the inspector's Table section.

use egui::{Context, Ui};
use thinoda_canvas::BoardTool;
use thinoda_core::{
    CanvasObjectId, ShapeKind,
    scene_table::{
        DEFAULT_TABLE_STRIPE, MAX_TABLE_SLOTS, MAX_TABLE_TRACKS, MIN_TABLE_TRACK, SceneTableCell,
        TableAxis, TableStripes, selected_table,
    },
};
use thinoda_ui::widgets;

use crate::{
    app::{Cmd, ThinodaApp},
    i18n::t,
};

const DEFAULT_INSERT_SIZE: (usize, usize) = (3, 3);

#[derive(Clone, Debug, PartialEq)]
enum TableAction {
    Insert(TableAxis, usize),
    Delete(TableAxis, usize),
    Merge(Vec<CanvasObjectId>),
    Unmerge(CanvasObjectId),
    Stripes(TableStripes),
    Resize(TableAxis, usize, f32),
}

/// Grid slots covered by the selected cells, as half-open row and column ranges.
fn covered(cells: &[SceneTableCell]) -> Option<([usize; 2], [usize; 2])> {
    let mut ranges: Option<([usize; 2], [usize; 2])> = None;
    for cell in cells {
        let row = [
            cell.start(TableAxis::Row),
            cell.start(TableAxis::Row) + cell.span(TableAxis::Row),
        ];
        let column = [
            cell.start(TableAxis::Column),
            cell.start(TableAxis::Column) + cell.span(TableAxis::Column),
        ];
        ranges = Some(match ranges {
            None => (row, column),
            Some((r, c)) => (
                [r[0].min(row[0]), r[1].max(row[1])],
                [c[0].min(column[0]), c[1].max(column[1])],
            ),
        });
    }
    ranges
}

/// Rows × columns picker for a new table; returns the size once confirmed.
pub(crate) fn insert_table_controls(ui: &mut Ui) -> Option<(usize, usize)> {
    let slot = ui.make_persistent_id("insert_table_size");
    let (mut rows, mut columns) = ui
        .data(|data| data.get_temp::<(usize, usize)>(slot))
        .unwrap_or(DEFAULT_INSERT_SIZE);
    ui.strong(t("Insert table"));
    widgets::property_field(ui, t("Rows"), |ui| {
        ui.add(egui::DragValue::new(&mut rows).range(1..=MAX_TABLE_TRACKS))
    });
    widgets::property_field(ui, t("Columns"), |ui| {
        ui.add(egui::DragValue::new(&mut columns).range(1..=MAX_TABLE_TRACKS))
    });
    ui.data_mut(|data| data.insert_temp(slot, (rows, columns)));
    ui.button(t("Insert")).clicked().then_some((rows, columns))
}

impl ThinodaApp {
    /// Table-specific actions for a single selected table or cell.
    pub(crate) fn board_table_context_menu(&mut self, ui: &mut Ui) -> bool {
        let Some(doc) = self.doc() else { return false };
        let [selected] = doc.view.board_selection.as_slice() else {
            return false;
        };
        if !doc.view.selection.is_empty() {
            return false;
        }
        let canvas = &doc.doc.sheet().canvas;
        let Some((table_id, table)) = selected_table(canvas, *selected) else {
            return false;
        };
        let rows = table.rows.len();
        let columns = table.columns.len();
        let cell = table.cell(*selected).copied();
        let enabled = !canvas.is_locked(table_id) && !doc.view.has_active_canvas_gesture();
        let mut action = None;
        ui.strong(t("Table"));
        if let Some(cell) = cell {
            for (label, next, available) in [
                (
                    "Insert row above",
                    TableAction::Insert(TableAxis::Row, cell.start(TableAxis::Row)),
                    rows < MAX_TABLE_TRACKS && (rows + 1) * columns <= MAX_TABLE_SLOTS,
                ),
                (
                    "Insert row below",
                    TableAction::Insert(
                        TableAxis::Row,
                        cell.start(TableAxis::Row) + cell.span(TableAxis::Row),
                    ),
                    rows < MAX_TABLE_TRACKS && (rows + 1) * columns <= MAX_TABLE_SLOTS,
                ),
                (
                    "Insert column left",
                    TableAction::Insert(TableAxis::Column, cell.start(TableAxis::Column)),
                    columns < MAX_TABLE_TRACKS && rows * (columns + 1) <= MAX_TABLE_SLOTS,
                ),
                (
                    "Insert column right",
                    TableAction::Insert(
                        TableAxis::Column,
                        cell.start(TableAxis::Column) + cell.span(TableAxis::Column),
                    ),
                    columns < MAX_TABLE_TRACKS && rows * (columns + 1) <= MAX_TABLE_SLOTS,
                ),
                (
                    "Delete row",
                    TableAction::Delete(TableAxis::Row, cell.start(TableAxis::Row)),
                    rows > 1,
                ),
                (
                    "Delete column",
                    TableAction::Delete(TableAxis::Column, cell.start(TableAxis::Column)),
                    columns > 1,
                ),
            ] {
                if ui
                    .add_enabled(enabled && available, egui::Button::new(t(label)))
                    .clicked()
                {
                    action = Some(next);
                }
            }
            if (cell.row_span > 1 || cell.column_span > 1)
                && ui
                    .add_enabled(enabled, egui::Button::new(t("Unmerge cells")))
                    .clicked()
            {
                action = Some(TableAction::Unmerge(cell.id));
            }
        } else {
            for (label, axis, index, available) in [
                (
                    "Insert row below",
                    TableAxis::Row,
                    rows,
                    rows < MAX_TABLE_TRACKS && (rows + 1) * columns <= MAX_TABLE_SLOTS,
                ),
                (
                    "Insert column right",
                    TableAxis::Column,
                    columns,
                    columns < MAX_TABLE_TRACKS && rows * (columns + 1) <= MAX_TABLE_SLOTS,
                ),
            ] {
                if ui
                    .add_enabled(enabled && available, egui::Button::new(t(label)))
                    .clicked()
                {
                    action = Some(TableAction::Insert(axis, index));
                }
            }
        }
        if let Some(action) = action {
            self.apply_table_action(table_id, action);
            ui.close();
        }
        ui.separator();
        true
    }

    pub(crate) fn insert_board_table(&mut self, ctx: &Context, rows: usize, columns: usize) {
        if !self.command_available(&Cmd::BoardTool(BoardTool::Shape(ShapeKind::Rectangle))) {
            self.toasts.warning(t("Not possible here"));
            return;
        }
        if !self.finish_canvas_text_edit() {
            return;
        }
        let Some(doc) = self.doc_mut() else { return };
        match doc.view.insert_board_table(&mut doc.doc, rows, columns) {
            Ok(_) => {
                ctx.memory_mut(|memory| memory.request_focus(thinoda_canvas::MapCanvas::id()));
            }
            Err(error) => {
                self.toasts
                    .error(format!("{}: {error}", t("Not possible here")));
            }
        }
    }

    /// Table section for a selected table or cells of one table. Returns
    /// whether it was shown.
    pub(crate) fn board_table_properties(&mut self, ui: &mut Ui) -> bool {
        let Some(doc) = self.doc() else { return false };
        if doc.view.board_selection.is_empty() || !doc.view.selection.is_empty() {
            return false;
        }
        let canvas = &doc.doc.sheet().canvas;
        let mut owner = None;
        let mut cells = Vec::new();
        for id in &doc.view.board_selection {
            let Some((table_id, table)) = selected_table(canvas, *id) else {
                return false;
            };
            if owner.is_some_and(|(owner, _)| owner != table_id) {
                return false;
            }
            owner = Some((table_id, table));
            if let Some(cell) = table.cell(*id) {
                cells.push(*cell);
            }
        }
        let Some((table_id, table)) = owner else {
            return false;
        };
        let rows = table.rows.len();
        let columns = table.columns.len();
        let stripes = table.stripes;
        let enabled = !canvas.is_locked(table_id) && !doc.view.has_active_canvas_gesture();
        let ranges = covered(&cells);
        let ([row_start, row_end], [column_start, column_end]) =
            ranges.unwrap_or(([0, rows], [0, columns]));
        let row_size = table.rows[row_start];
        let column_size = table.columns[column_start];
        let unmerge = match cells.as_slice() {
            [cell] if cell.row_span > 1 || cell.column_span > 1 => Some(cell.id),
            _ => None,
        };
        let mut action = None;
        ui.push_id(("board_table", table_id), |ui| {
            ui.strong(t("Table"));
            ui.label(format!(
                "{}: {rows}   {}: {columns}",
                t("Rows"),
                t("Columns")
            ));
            ui.add_enabled_ui(enabled, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (label, axis, index) in [
                        ("Insert row above", TableAxis::Row, row_start),
                        ("Insert row below", TableAxis::Row, row_end),
                        ("Insert column left", TableAxis::Column, column_start),
                        ("Insert column right", TableAxis::Column, column_end),
                    ] {
                        if ui.button(t(label)).clicked() {
                            action = Some(TableAction::Insert(axis, index));
                        }
                    }
                });
                if ranges.is_some() {
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .add_enabled(rows > 1, egui::Button::new(t("Delete row")))
                            .clicked()
                        {
                            action = Some(TableAction::Delete(TableAxis::Row, row_start));
                        }
                        if ui
                            .add_enabled(columns > 1, egui::Button::new(t("Delete column")))
                            .clicked()
                        {
                            action = Some(TableAction::Delete(TableAxis::Column, column_start));
                        }
                        if ui
                            .add_enabled(cells.len() > 1, egui::Button::new(t("Merge cells")))
                            .clicked()
                        {
                            action = Some(TableAction::Merge(
                                cells.iter().map(|cell| cell.id).collect(),
                            ));
                        }
                        if ui
                            .add_enabled(unmerge.is_some(), egui::Button::new(t("Unmerge cells")))
                            .clicked()
                            && let Some(cell) = unmerge
                        {
                            action = Some(TableAction::Unmerge(cell));
                        }
                    });
                    for (label, axis, index, stored) in [
                        ("Row height", TableAxis::Row, row_start, row_size),
                        ("Column width", TableAxis::Column, column_start, column_size),
                    ] {
                        if let Some(size) =
                            track_size_field(ui, label, (table_id, axis, index), stored)
                        {
                            action = Some(TableAction::Resize(axis, index, size));
                        }
                    }
                } else {
                    widgets::caption(
                        ui,
                        t("Select cells to delete, merge or resize rows and columns.").to_owned(),
                    );
                }
                let mut striped_rows = stripes.even_rows.is_some() || stripes.odd_rows.is_some();
                if ui.checkbox(&mut striped_rows, t("Row stripes")).changed() {
                    let mut next = stripes;
                    next.even_rows = striped_rows.then_some(DEFAULT_TABLE_STRIPE);
                    next.odd_rows = None;
                    action = Some(TableAction::Stripes(next));
                }
                let mut striped_columns =
                    stripes.even_columns.is_some() || stripes.odd_columns.is_some();
                if ui
                    .checkbox(&mut striped_columns, t("Column stripes"))
                    .changed()
                {
                    let mut next = stripes;
                    next.even_columns = striped_columns.then_some(DEFAULT_TABLE_STRIPE);
                    next.odd_columns = None;
                    action = Some(TableAction::Stripes(next));
                }
            });
            ui.separator();
        });
        if let Some(action) = action {
            self.apply_table_action(table_id, action);
        }
        true
    }

    fn apply_table_action(&mut self, table: CanvasObjectId, action: TableAction) {
        let Some(doc) = self.doc_mut() else { return };
        let result = match action {
            TableAction::Insert(axis, index) => {
                doc.doc.insert_canvas_table_track(table, axis, index)
            }
            TableAction::Delete(axis, index) => {
                doc.doc.delete_canvas_table_track(table, axis, index)
            }
            TableAction::Merge(cells) => doc
                .doc
                .merge_canvas_table_cells(table, &cells)
                .map(|kept| doc.view.board_selection = vec![kept]),
            TableAction::Unmerge(cell) => doc.doc.unmerge_canvas_table_cell(table, cell),
            TableAction::Stripes(stripes) => doc.doc.set_canvas_table_stripes(table, stripes),
            TableAction::Resize(axis, index, size) => {
                doc.doc.resize_canvas_table_track(table, axis, index, size)
            }
        };
        match result {
            Ok(()) => {
                let canvas = &doc.doc.sheet().canvas;
                doc.view
                    .board_selection
                    .retain(|id| canvas.object(*id).is_some());
                if doc.view.board_selection.is_empty() {
                    doc.view.board_selection.push(table);
                }
                doc.view.reveal_board_object(table);
            }
            Err(error) => {
                self.toasts
                    .error(format!("{}: {error}", t("Not possible here")));
            }
        }
    }
}

/// Row height or column width editor that commits once, when dragging or
/// typing finishes.
fn track_size_field(
    ui: &mut Ui,
    label: &'static str,
    key: (CanvasObjectId, TableAxis, usize),
    stored: f32,
) -> Option<f32> {
    let slot = ui.make_persistent_id(("table_track_draft", key));
    let mut value = ui.data(|data| data.get_temp::<f32>(slot)).unwrap_or(stored);
    let response = widgets::property_field(ui, t(label), |ui| {
        ui.add(
            egui::DragValue::new(&mut value)
                .speed(1.0)
                .range(MIN_TABLE_TRACK..=thinoda_core::board::MAX_BOARD_COORDINATE),
        )
    });
    let editing = response.dragged() || response.has_focus();
    if editing {
        ui.data_mut(|data| data.insert_temp(slot, value));
        return None;
    }
    ui.data_mut(|data| data.remove::<f32>(slot));
    ((value - stored).abs() > 0.01).then_some(value)
}

#[cfg(test)]
mod tests;
