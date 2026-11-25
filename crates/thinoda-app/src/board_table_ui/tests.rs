use egui::{Ui, vec2};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use thinoda_core::Vec2;

use super::*;

fn click(harness: &mut Harness<'_, ThinodaApp>, label: &str) {
    harness.get_by_label(label).click();
    harness.run_steps(3);
}

fn table(app: &ThinodaApp, id: CanvasObjectId) -> thinoda_core::scene_table::SceneTable {
    app.doc().unwrap().doc.sheet().canvas.scene.tables[&id].clone()
}

#[test]
fn board_toolbar_inserts_a_selected_table() {
    let mut harness = Harness::builder()
        .with_size(vec2(800.0, 800.0))
        .build_ui_state(
            |ui: &mut Ui, app: &mut ThinodaApp| {
                app.board_toolbar(ui.ctx(), ui.max_rect(), None);
            },
            ThinodaApp::headless(),
        );
    harness.run_steps(3);
    click(&mut harness, "Insert table");
    click(&mut harness, "Insert");
    let doc = harness.state().doc().unwrap();
    let [id] = doc.view.board_selection[..] else {
        panic!("one table is selected");
    };
    let table = &doc.doc.sheet().canvas.scene.tables[&id];
    assert_eq!((table.rows.len(), table.columns.len()), DEFAULT_INSERT_SIZE);
    assert_eq!(doc.doc.undo_label(), Some("Insert table"));
}

#[test]
fn table_section_merges_inserts_stripes_and_resizes_selected_cells() {
    let mut app = ThinodaApp::headless();
    let id = app
        .doc_mut()
        .unwrap()
        .doc
        .insert_canvas_table(Vec2::ZERO, 2, 3, None)
        .unwrap();
    let first = table(&app, id).cells[..2]
        .iter()
        .map(|cell| cell.id)
        .collect();
    app.doc_mut().unwrap().view.selection.clear();
    app.doc_mut().unwrap().view.board_selection = first;
    let mut harness = Harness::builder()
        .with_size(vec2(450.0, 1000.0))
        .build_ui_state(
            |ui: &mut Ui, app: &mut ThinodaApp| {
                app.board_object_properties(ui);
            },
            app,
        );
    harness.run_steps(3);
    assert!(
        harness
            .query_by_label("Select one board object to edit its properties.")
            .is_none()
    );
    assert!(
        harness
            .get_by_label("Unmerge cells")
            .accesskit_node()
            .is_disabled()
    );

    click(&mut harness, "Merge cells");
    let merged = table(harness.state(), id);
    assert_eq!(merged.cells.len(), 5);
    assert_eq!(merged.cells[0].column_span, 2);
    assert_eq!(
        harness.state().doc().unwrap().view.board_selection,
        vec![merged.cells[0].id]
    );

    click(&mut harness, "Insert column right");
    let widened = table(harness.state(), id);
    assert_eq!(widened.columns.len(), 4);
    assert_eq!(widened.cell_at(0, 2).unwrap().column, 2);

    click(&mut harness, "Row stripes");
    assert!(table(harness.state(), id).stripes.even_rows.is_some());

    click(&mut harness, "Delete row");
    assert_eq!(table(harness.state(), id).rows.len(), 1);
    assert_eq!(
        harness.state().doc().unwrap().view.board_selection,
        vec![id]
    );

    let app = harness.state_mut();
    for _ in 0..3 {
        assert!(app.doc_mut().unwrap().doc.undo());
    }
    let restored = table(app, id);
    assert_eq!((restored.rows.len(), restored.columns.len()), (2, 3));
    assert_eq!(restored.cells[0].column_span, 2);
}

#[test]
fn track_size_commits_typed_row_height_once() {
    let mut app = ThinodaApp::headless();
    let id = app
        .doc_mut()
        .unwrap()
        .doc
        .insert_canvas_table(Vec2::ZERO, 2, 2, None)
        .unwrap();
    let cell = table(&app, id).cells[2].id;
    app.doc_mut().unwrap().view.selection.clear();
    app.doc_mut().unwrap().view.board_selection = vec![cell];
    let mut harness = Harness::builder()
        .with_size(vec2(450.0, 1000.0))
        .build_ui_state(
            |ui: &mut Ui, app: &mut ThinodaApp| {
                app.board_object_properties(ui);
            },
            app,
        );
    harness.run_steps(3);
    harness.get_by_label("Row height").focus();
    harness.run_steps(2);
    harness.get_by_label("Row height").type_text("55");
    harness.run_steps(2);
    assert_eq!(table(harness.state(), id).rows, vec![40.0, 40.0]);
    harness.key_press(egui::Key::Enter);
    harness.run_steps(3);
    let resized = table(harness.state(), id);
    assert_eq!(resized.rows, vec![40.0, 55.0]);
    assert_eq!(
        harness.state().doc().unwrap().doc.undo_label(),
        Some("Resize table row")
    );
}

#[test]
fn table_context_actions_target_the_selected_cell_and_are_undoable() {
    let mut app = ThinodaApp::headless();
    let id = app
        .doc_mut()
        .unwrap()
        .doc
        .insert_canvas_table(Vec2::ZERO, 2, 2, None)
        .unwrap();
    let cell = table(&app, id).cell_at(1, 0).unwrap().id;
    app.doc_mut().unwrap().view.selection.clear();
    app.doc_mut().unwrap().view.board_selection = vec![cell];
    let mut harness = Harness::builder()
        .with_size(vec2(450.0, 1000.0))
        .build_ui_state(
            |ui: &mut Ui, app: &mut ThinodaApp| {
                app.board_table_context_menu(ui);
            },
            app,
        );
    harness.run_steps(3);
    click(&mut harness, "Insert row above");
    let result = table(harness.state(), id);
    assert_eq!(result.rows.len(), 3);
    assert_eq!(result.cell(cell).unwrap().row, 2);
    assert_eq!(
        harness.state().doc().unwrap().doc.undo_label(),
        Some("Insert table row")
    );
    assert!(harness.state_mut().doc_mut().unwrap().doc.undo());
    assert_eq!(table(harness.state(), id).rows.len(), 2);
}

#[test]
fn table_context_prevents_deleting_the_last_track() {
    let mut app = ThinodaApp::headless();
    let id = app
        .doc_mut()
        .unwrap()
        .doc
        .insert_canvas_table(Vec2::ZERO, 1, 1, None)
        .unwrap();
    let cell = table(&app, id).cells[0].id;
    app.doc_mut().unwrap().view.selection.clear();
    app.doc_mut().unwrap().view.board_selection = vec![cell];
    let mut harness = Harness::builder()
        .with_size(vec2(450.0, 1000.0))
        .build_ui_state(
            |ui: &mut Ui, app: &mut ThinodaApp| {
                app.board_table_context_menu(ui);
            },
            app,
        );
    harness.run_steps(3);
    assert!(
        harness
            .get_by_label("Delete row")
            .accesskit_node()
            .is_disabled()
    );
    assert!(
        harness
            .get_by_label("Delete column")
            .accesskit_node()
            .is_disabled()
    );
}

#[test]
fn table_context_inserts_after_merged_cell_span() {
    let mut app = ThinodaApp::headless();
    let doc = &mut app.doc_mut().unwrap().doc;
    let id = doc.insert_canvas_table(Vec2::ZERO, 2, 2, None).unwrap();
    let cells: Vec<_> = doc.sheet().canvas.scene.tables[&id].cells[..2]
        .iter()
        .map(|cell| cell.id)
        .collect();
    let merged = doc.merge_canvas_table_cells(id, &cells).unwrap();
    let view = &mut app.doc_mut().unwrap().view;
    view.selection.clear();
    view.board_selection = vec![merged];
    let mut harness = Harness::builder()
        .with_size(vec2(450.0, 1000.0))
        .build_ui_state(
            |ui: &mut Ui, app: &mut ThinodaApp| {
                app.board_table_context_menu(ui);
            },
            app,
        );
    harness.run_steps(3);
    click(&mut harness, "Insert column right");
    let result = table(harness.state(), id);
    assert_eq!(result.columns.len(), 3);
    assert_eq!(result.cell(merged).unwrap().column_span, 2);
    assert_eq!(result.cell_at(0, 2).unwrap().column, 2);
}

#[test]
fn table_context_disables_actions_when_table_is_locked() {
    let mut app = ThinodaApp::headless();
    let doc = &mut app.doc_mut().unwrap().doc;
    let id = doc.insert_canvas_table(Vec2::ZERO, 2, 2, None).unwrap();
    let cell = doc.sheet().canvas.scene.tables[&id].cells[0].id;
    doc.set_canvas_objects_locked(&[id], true).unwrap();
    let view = &mut app.doc_mut().unwrap().view;
    view.selection.clear();
    view.board_selection = vec![cell];
    let mut harness = Harness::builder()
        .with_size(vec2(450.0, 1000.0))
        .build_ui_state(
            |ui: &mut Ui, app: &mut ThinodaApp| {
                app.board_table_context_menu(ui);
            },
            app,
        );
    harness.run_steps(3);
    for label in [
        "Insert row above",
        "Insert column right",
        "Delete row",
        "Delete column",
    ] {
        assert!(harness.get_by_label(label).accesskit_node().is_disabled());
    }
}

#[test]
fn right_clicking_a_cell_opens_table_actions_on_the_canvas() {
    let mut app = ThinodaApp::headless();
    app.settings.ai.enabled = false;
    app.settings.show_format_panel = false;
    app.new_doc(thinoda_core::templates::instantiate("blank-canvas"));
    let id = app
        .doc_mut()
        .unwrap()
        .doc
        .insert_canvas_table(Vec2::ZERO, 2, 2, None)
        .unwrap();
    let cell = table(&app, id).cell_at(0, 0).unwrap().id;
    app.doc_mut().unwrap().view.select_board_object(cell);
    let mut harness = Harness::builder()
        .with_size(vec2(1200.0, 900.0))
        .build_ui_state(|ui: &mut Ui, app: &mut ThinodaApp| app.draw(ui), app);
    harness.run_steps(6);
    let point = {
        let doc = harness.state_mut().doc_mut().unwrap();
        doc.view.selected_board_rect(&doc.doc).unwrap().center()
    };
    harness.event(egui::Event::PointerMoved(point));
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        harness.run_steps(1);
    }
    harness.run_steps(4);
    click(&mut harness, "Insert row above");
    assert_eq!(table(harness.state(), id).rows.len(), 3);
}
