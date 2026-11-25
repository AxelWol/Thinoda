use super::*;
use crate::board::validate_board;
use crate::document::Document;
use crate::model::Workbook;
use crate::scene_layout::{ContainerLayoutPolicy, HeaderSide, StackAxis};

fn sheet_with_table(rows: usize, columns: usize) -> (Sheet, CanvasObjectId) {
    let mut sheet = Sheet::new("Board", "Root");
    let table = sheet
        .insert_canvas_table(Vec2::new(100.0, 50.0), rows, columns, None)
        .unwrap();
    (sheet, table)
}

fn cell(sheet: &Sheet, table: CanvasObjectId, row: usize, column: usize) -> CanvasObjectId {
    sheet.canvas.scene.tables[&table]
        .cell_at(row, column)
        .unwrap()
        .id
}

fn rect(sheet: &Sheet, id: CanvasObjectId) -> Rect {
    sheet.canvas.object(id).unwrap().rect
}

#[test]
fn content_growth_is_width_sensitive_and_undoes_with_the_edit() {
    let mut doc = Document::new(Workbook::new(Sheet::new("Board", "Root")));
    let table = doc.insert_canvas_table(Vec2::ZERO, 2, 1, None).unwrap();
    let first = cell(doc.sheet(), table, 0, 0);
    let second = cell(doc.sheet(), table, 1, 0);
    let text = "one two three four five six seven eight nine ten eleven twelve";
    doc.set_canvas_object_label("Edit cell", first, text)
        .unwrap();
    let grown = doc.sheet().canvas.scene.tables[&table].rows[0];
    assert!(grown > DEFAULT_TABLE_ROW);
    assert_eq!(rect(doc.sheet(), second).min.y, grown);
    let object = doc.sheet().canvas.object(first).unwrap();
    let layout = crate::label_style::layout(
        object,
        crate::board::object_label_rect(object).unwrap(),
        text,
    )
    .unwrap();
    assert!(layout.block.size.y <= layout.frame.size.y + 0.01);
    assert!(doc.undo());
    assert_eq!(
        doc.sheet().canvas.scene.tables[&table].rows[0],
        DEFAULT_TABLE_ROW
    );
    assert!(doc.sheet().canvas.object(first).unwrap().label().is_empty());
    assert!(doc.redo());
    doc.resize_canvas_table_track(table, TableAxis::Column, 0, 60.0)
        .unwrap();
    let narrower = doc.sheet().canvas.scene.tables[&table].rows[0];
    assert!(narrower > grown);
    doc.set_canvas_object_label("Edit cell", first, "short")
        .unwrap();
    assert_eq!(doc.sheet().canvas.scene.tables[&table].rows[0], narrower);
}

#[test]
fn merged_cell_growth_uses_the_whole_span_and_last_row() {
    let mut doc = Document::new(Workbook::new(Sheet::new("Board", "Root")));
    let table = doc.insert_canvas_table(Vec2::ZERO, 2, 1, None).unwrap();
    let cells = [
        cell(doc.sheet(), table, 0, 0),
        cell(doc.sheet(), table, 1, 0),
    ];
    let merged = doc.merge_canvas_table_cells(table, &cells).unwrap();
    doc.set_canvas_object_label(
        "Edit cell",
        merged,
        "a long line of words ".repeat(20).as_str(),
    )
    .unwrap();
    let rows = &doc.sheet().canvas.scene.tables[&table].rows;
    assert_eq!(rows[0], DEFAULT_TABLE_ROW);
    assert!(rows[1] > DEFAULT_TABLE_ROW);
    assert!(doc.undo());
    assert_eq!(
        doc.sheet().canvas.scene.tables[&table].rows,
        vec![DEFAULT_TABLE_ROW; 2]
    );
}

#[test]
fn rich_cell_content_grows_for_large_runs() {
    let mut doc = Document::new(Workbook::new(Sheet::new("Board", "Root")));
    let table = doc.insert_canvas_table(Vec2::ZERO, 1, 1, None).unwrap();
    let first = cell(doc.sheet(), table, 0, 0);
    let text = "large text wraps over multiple lines";
    let mut rich = crate::rich_text::InlineText::default();
    rich.update_range(text, 0..text.chars().count(), |style| {
        style.font_size = Some(30.0);
    })
    .unwrap();
    doc.set_canvas_object_label_with_rich_text("Edit cell", first, text, Some(rich))
        .unwrap();
    let object = doc.sheet().canvas.object(first).unwrap();
    let layout = crate::label_style::layout(
        object,
        crate::board::object_label_rect(object).unwrap(),
        text,
    )
    .unwrap();
    assert!(layout.block.size.y > DEFAULT_TABLE_ROW);
    assert!(layout.frame.size.y + 0.01 >= layout.block.size.y);
    assert!(doc.undo());
    assert_eq!(
        doc.sheet().canvas.scene.tables[&table].rows[0],
        DEFAULT_TABLE_ROW
    );
}

#[test]
fn vertical_cell_label_fits_its_grown_cell() {
    let mut doc = Document::new(Workbook::new(Sheet::new("Board", "Root")));
    let table = doc.insert_canvas_table(Vec2::ZERO, 1, 1, None).unwrap();
    let first = cell(doc.sheet(), table, 0, 0);
    doc.update_canvas_object("Vertical text", first, |object| {
        object.label_style = Some(std::sync::Arc::new(crate::label_style::LabelStyle {
            vertical: true,
            ..Default::default()
        }));
    })
    .unwrap();
    let text = "one two three four five six seven eight nine ten";
    doc.set_canvas_object_label("Edit cell", first, text)
        .unwrap();
    let object = doc.sheet().canvas.object(first).unwrap();
    let base = crate::board::object_label_rect(object).unwrap();
    let layout = crate::label_style::layout(object, base, text).unwrap();
    let block = layout.block;
    for corner in [
        block.min,
        Vec2::new(block.max().x, block.min.y),
        block.max(),
        Vec2::new(block.min.x, block.max().y),
    ] {
        let point = layout.to_sheet(corner);
        assert!(point.y >= base.min.y - 0.01 && point.y <= base.max().y + 0.01);
    }
}

#[test]
fn growth_beyond_geometry_limits_rolls_back_the_text_edit() {
    let mut doc = Document::new(Workbook::new(Sheet::new("Board", "Root")));
    let table = doc.insert_canvas_table(Vec2::ZERO, 1, 1, None).unwrap();
    doc.resize_canvas_table_track(table, TableAxis::Column, 0, MIN_TABLE_TRACK)
        .unwrap();
    let first = cell(doc.sheet(), table, 0, 0);
    let text = "a".repeat(4000);
    let mut rich = crate::rich_text::InlineText::default();
    rich.update_range(&text, 0..text.len(), |style| style.font_size = Some(256.0))
        .unwrap();
    assert!(
        doc.set_canvas_object_label_with_rich_text("Edit cell", first, &text, Some(rich))
            .is_err()
    );
    assert!(doc.sheet().canvas.object(first).unwrap().label().is_empty());
    assert_eq!(
        doc.sheet().canvas.scene.tables[&table].rows[0],
        DEFAULT_TABLE_ROW
    );
    assert_eq!(doc.undo_label(), Some("Resize table column"));
}

#[test]
fn nested_content_growth_stays_in_the_parent_cell() {
    let mut doc = Document::new(Workbook::new(Sheet::new("Board", "Root")));
    let table = doc.insert_canvas_table(Vec2::ZERO, 1, 1, None).unwrap();
    let first = cell(doc.sheet(), table, 0, 0);
    doc.edit_sheet("Add content", |sheet| {
        let content = sheet
            .add_canvas_object(CanvasObject::shape(
                ShapeKind::Rectangle,
                Rect::from_min_size(Vec2::new(5.0, 5.0), Vec2::new(20.0, 80.0)),
            ))
            .map_err(EditError::Invalid)?;
        sheet
            .reparent_canvas_objects(&[content], Some(first))
            .map_err(EditError::Invalid)?;
        Ok(())
    })
    .unwrap();
    assert!(doc.sheet().canvas.scene.tables[&table].rows[0] >= 85.0);
    assert!(doc.undo());
    assert_eq!(
        doc.sheet().canvas.scene.tables[&table].rows[0],
        DEFAULT_TABLE_ROW
    );
}

#[test]
fn nested_table_growth_reflows_its_parent_in_the_same_undo() {
    let (mut sheet, outer) = sheet_with_table(1, 1);
    let outer_cell = cell(&sheet, outer, 0, 0);
    let inner = sheet
        .insert_canvas_table(Vec2::new(105.0, 55.0), 1, 1, None)
        .unwrap();
    sheet
        .reparent_canvas_objects(&[inner], Some(outer_cell))
        .unwrap();
    let inner_cell = cell(&sheet, inner, 0, 0);
    let mut doc = Document::new(Workbook::new(sheet));
    doc.set_canvas_object_label("Edit cell", inner_cell, &"wrapped content ".repeat(20))
        .unwrap();
    let inner_height = doc.sheet().canvas.scene.tables[&inner].rows[0];
    let outer_height = doc.sheet().canvas.scene.tables[&outer].rows[0];
    assert!(inner_height > DEFAULT_TABLE_ROW);
    assert!(outer_height >= inner_height + 5.0);
    assert!(doc.undo());
    assert_eq!(
        doc.sheet().canvas.scene.tables[&inner].rows[0],
        DEFAULT_TABLE_ROW
    );
    assert_eq!(
        doc.sheet().canvas.scene.tables[&outer].rows[0],
        DEFAULT_TABLE_ROW
    );
}

#[test]
fn locked_cell_cannot_be_edited_or_grow_its_table() {
    let mut doc = Document::new(Workbook::new(Sheet::new("Board", "Root")));
    let table = doc.insert_canvas_table(Vec2::ZERO, 1, 1, None).unwrap();
    let first = cell(doc.sheet(), table, 0, 0);
    doc.set_canvas_objects_locked(&[first], true).unwrap();
    assert!(
        doc.set_canvas_object_label("Edit cell", first, &"long ".repeat(100))
            .is_err()
    );
    assert!(doc.sheet().canvas.object(first).unwrap().label().is_empty());
    assert_eq!(
        doc.sheet().canvas.scene.tables[&table].rows[0],
        DEFAULT_TABLE_ROW
    );
}

#[test]
fn growth_does_not_change_locked_sibling_label_style() {
    let (mut sheet, table) = sheet_with_table(1, 2);
    let first = cell(&sheet, table, 0, 0);
    let sibling = cell(&sheet, table, 0, 1);
    sheet
        .canvas
        .object_mut(sibling)
        .unwrap()
        .set_label("existing")
        .unwrap();
    sheet.set_canvas_objects_locked(&[sibling], true).unwrap();
    let mut doc = Document::new(Workbook::new(sheet));
    doc.set_canvas_object_label("Edit cell", first, "short")
        .unwrap();
    assert!(
        doc.sheet()
            .canvas
            .object(sibling)
            .unwrap()
            .label_style
            .is_none()
    );
    assert!(
        doc.set_canvas_object_label("Edit cell", first, &"long ".repeat(100))
            .is_err()
    );
    assert_eq!(doc.sheet().canvas.object(first).unwrap().label(), "short");
    assert_eq!(
        doc.sheet().canvas.scene.tables[&table].rows[0],
        DEFAULT_TABLE_ROW
    );
}

#[test]
fn cells_in_the_same_row_share_the_largest_required_height() {
    let mut doc = Document::new(Workbook::new(Sheet::new("Board", "Root")));
    let table = doc.insert_canvas_table(Vec2::ZERO, 1, 2, None).unwrap();
    let first = cell(doc.sheet(), table, 0, 0);
    let second = cell(doc.sheet(), table, 0, 1);
    doc.set_canvas_object_label("Edit cell", first, &"one ".repeat(35))
        .unwrap();
    let height = doc.sheet().canvas.scene.tables[&table].rows[0];
    doc.set_canvas_object_label("Edit cell", second, &"two ".repeat(70))
        .unwrap();
    let grown = doc.sheet().canvas.scene.tables[&table].rows[0];
    assert!(grown > height);
    assert_eq!(rect(doc.sheet(), first).size.y, grown);
    assert_eq!(rect(doc.sheet(), second).size.y, grown);
    assert!(doc.undo());
    assert_eq!(doc.sheet().canvas.scene.tables[&table].rows[0], height);
}

#[test]
fn unrelated_edits_do_not_reflow_existing_imported_geometry() {
    let (mut sheet, table) = sheet_with_table(1, 1);
    let first = cell(&sheet, table, 0, 0);
    sheet
        .canvas
        .object_mut(first)
        .unwrap()
        .set_label("existing long text ".repeat(20).as_str())
        .unwrap();
    let mut doc = Document::new(Workbook::new(sheet));
    doc.edit_sheet("Rename sheet", |sheet| {
        sheet.title = "Renamed".to_owned();
        Ok(())
    })
    .unwrap();
    assert_eq!(
        doc.sheet().canvas.scene.tables[&table].rows[0],
        DEFAULT_TABLE_ROW
    );
    assert!(
        doc.sheet()
            .canvas
            .object(first)
            .unwrap()
            .label_style
            .is_none()
    );
}

fn content_in(sheet: &mut Sheet, cell: CanvasObjectId) -> CanvasObjectId {
    let at = rect(sheet, cell).min + Vec2::new(5.0, 5.0);
    let id = sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Ellipse,
            Rect::from_min_size(at, Vec2::new(10.0, 10.0)),
        ))
        .unwrap();
    sheet.reparent_canvas_objects(&[id], Some(cell)).unwrap();
    id
}

#[test]
fn inserted_table_lays_cells_out_on_its_grid_and_validates() {
    let (sheet, table) = sheet_with_table(2, 3);
    assert!(sheet.has_tables());
    assert_eq!(
        rect(&sheet, table),
        Rect::from_min_size(Vec2::new(100.0, 50.0), Vec2::new(360.0, 80.0))
    );
    assert_eq!(
        rect(&sheet, cell(&sheet, table, 1, 2)),
        Rect::from_min_size(Vec2::new(340.0, 90.0), Vec2::new(120.0, 40.0))
    );
    validate_board(&sheet).unwrap();
    let json = serde_json::to_string(&sheet.canvas.scene).unwrap();
    let back: crate::scene_hierarchy::SceneStructure = serde_json::from_str(&json).unwrap();
    assert_eq!(back, sheet.canvas.scene);
}

#[test]
fn inserting_a_row_grows_spanning_cells_and_moves_later_content() {
    let (mut sheet, table) = sheet_with_table(2, 2);
    let merged = sheet
        .merge_canvas_table_cells(
            table,
            &[cell(&sheet, table, 0, 0), cell(&sheet, table, 1, 0)],
        )
        .unwrap();
    let lower = cell(&sheet, table, 1, 1);
    let content = content_in(&mut sheet, lower);
    let before = rect(&sheet, content);

    sheet
        .insert_canvas_table_track(table, TableAxis::Row, 1)
        .unwrap();

    let grid = &sheet.canvas.scene.tables[&table];
    assert_eq!(grid.rows.len(), 3);
    assert_eq!(grid.cell(merged).unwrap().row_span, 3);
    assert_eq!(grid.cell(lower).unwrap().row, 2);
    assert_ne!(cell(&sheet, table, 1, 1), lower);
    assert_eq!(rect(&sheet, content).min, before.min + Vec2::new(0.0, 40.0));
    assert_eq!(rect(&sheet, table).size.y, 120.0);
    validate_board(&sheet).unwrap();
}

#[test]
fn deleting_a_column_removes_its_cells_with_content_and_shrinks_spans() {
    let (mut sheet, table) = sheet_with_table(2, 3);
    let wide = sheet
        .merge_canvas_table_cells(
            table,
            &[cell(&sheet, table, 0, 0), cell(&sheet, table, 0, 1)],
        )
        .unwrap();
    let doomed = cell(&sheet, table, 1, 1);
    let content = content_in(&mut sheet, doomed);
    let last = cell(&sheet, table, 1, 2);

    sheet
        .delete_canvas_table_track(table, TableAxis::Column, 1)
        .unwrap();

    let grid = &sheet.canvas.scene.tables[&table];
    assert_eq!(grid.columns.len(), 2);
    assert_eq!(grid.cell(wide).unwrap().column_span, 1);
    assert_eq!(grid.cell(last).unwrap().column, 1);
    assert!(sheet.canvas.object(doomed).is_none());
    assert!(sheet.canvas.object(content).is_none());
    assert_eq!(rect(&sheet, last).min.x, 220.0);
    validate_board(&sheet).unwrap();

    let (mut single, table) = sheet_with_table(1, 1);
    assert!(
        single
            .delete_canvas_table_track(table, TableAxis::Row, 0)
            .is_err()
    );
}

#[test]
fn merging_needs_a_full_rectangle_and_unmerging_restores_slots() {
    let (mut sheet, table) = sheet_with_table(3, 3);
    let wide = sheet
        .merge_canvas_table_cells(
            table,
            &[cell(&sheet, table, 0, 1), cell(&sheet, table, 0, 2)],
        )
        .unwrap();
    assert!(
        sheet
            .merge_canvas_table_cells(
                table,
                &[cell(&sheet, table, 0, 0), cell(&sheet, table, 1, 1)]
            )
            .is_err(),
        "the wide cell sticks out of the 2x2 block"
    );
    let target = cell(&sheet, table, 1, 2);
    let moved = content_in(&mut sheet, target);
    let at = rect(&sheet, moved);
    let block = sheet
        .merge_canvas_table_cells(table, &[wide, cell(&sheet, table, 1, 2)])
        .unwrap();
    assert_eq!(block, wide);
    let grid = &sheet.canvas.scene.tables[&table];
    assert_eq!(grid.cells.len(), 6);
    assert_eq!(sheet.canvas.scene.parents[&moved], wide);
    assert_eq!(rect(&sheet, moved), at);
    assert_eq!(rect(&sheet, wide).size, Vec2::new(240.0, 80.0));

    sheet.unmerge_canvas_table_cell(table, wide).unwrap();
    let grid = &sheet.canvas.scene.tables[&table];
    assert_eq!(grid.cells.len(), 9);
    assert_eq!(rect(&sheet, wide).size, Vec2::new(120.0, 40.0));
    assert!(sheet.unmerge_canvas_table_cell(table, wide).is_err());
    validate_board(&sheet).unwrap();
}

#[test]
fn table_frames_are_owned_by_the_grid() {
    let (mut sheet, table) = sheet_with_table(2, 2);
    let first = cell(&sheet, table, 0, 0);
    let target = cell(&sheet, table, 1, 1);
    let content = content_in(&mut sheet, target);

    sheet
        .resize_canvas_object(
            table,
            Rect::from_min_size(Vec2::new(0.0, 0.0), Vec2::new(480.0, 40.0)),
        )
        .unwrap();
    let grid = &sheet.canvas.scene.tables[&table];
    assert_eq!(grid.columns, vec![240.0, 240.0]);
    assert_eq!(grid.rows, vec![20.0, 20.0]);
    assert_eq!(rect(&sheet, content).size, Vec2::new(10.0, 10.0));
    assert_eq!(rect(&sheet, content).min, Vec2::new(245.0, 25.0));
    validate_board(&sheet).unwrap();

    assert!(
        sheet
            .resize_canvas_object(table, Rect::from_min_size(Vec2::ZERO, Vec2::new(4.0, 4.0)))
            .is_err()
    );
    assert!(
        sheet
            .resize_canvas_object(first, Rect::from_min_size(Vec2::ZERO, Vec2::new(9.0, 9.0)))
            .is_err()
    );
    assert!(sheet.remove_canvas_objects(&[first]).is_err());
    sheet.remove_canvas_objects(&[table]).unwrap();
    assert!(!sheet.has_tables());
    assert!(sheet.canvas.object(content).is_none());
}

#[test]
fn table_header_policies_extend_the_frame_above_the_grid() {
    let (mut sheet, table) = sheet_with_table(1, 2);
    sheet
        .set_canvas_container_layout_policy(
            table,
            Some(ContainerLayoutPolicy {
                header: HeaderPolicy {
                    side: HeaderSide::Top,
                    extent: 30.0,
                },
                layout: ContainerLayout::Free,
            }),
        )
        .unwrap();
    assert_eq!(rect(&sheet, table).size, Vec2::new(240.0, 70.0));
    assert_eq!(
        rect(&sheet, cell(&sheet, table, 0, 0)).min,
        Vec2::new(100.0, 80.0)
    );
    validate_board(&sheet).unwrap();
}

#[test]
fn forged_tables_are_rejected() {
    let (sheet, table) = sheet_with_table(2, 2);
    let check = |mutate: &dyn Fn(&mut Sheet)| {
        let mut forged = sheet.clone();
        mutate(&mut forged);
        validate_board(&forged).unwrap_err()
    };
    check(&|s| {
        let grid = s.canvas.scene.tables.get_mut(&table).unwrap();
        grid.cells[0].column_span = 2;
    });
    check(&|s| {
        let grid = s.canvas.scene.tables.get_mut(&table).unwrap();
        grid.cells.pop();
    });
    check(&|s| {
        let grid = s.canvas.scene.tables.get_mut(&table).unwrap();
        grid.cells.swap(0, 1);
    });
    check(&|s| {
        let grid = s.canvas.scene.tables.get_mut(&table).unwrap();
        grid.rows[0] = f32::NAN;
    });
    check(&|s| {
        let first = s.canvas.scene.tables[&table].cells[0].id;
        s.canvas.object_mut(first).unwrap().rect.min.x += 3.0;
    });
    check(&|s| {
        let stray = s
            .add_canvas_object(CanvasObject::shape(ShapeKind::Rectangle, Rect::default()))
            .unwrap();
        s.canvas.scene.parents.insert(stray, table);
    });
    check(&|s| {
        let first = s.canvas.scene.tables[&table].cells[0].id;
        s.canvas.scene.parents.remove(&first);
    });
    check(&|s| {
        s.canvas.scene.layout_policies.insert(
            table,
            ContainerLayoutPolicy {
                header: HeaderPolicy {
                    side: HeaderSide::Top,
                    extent: 0.0,
                },
                layout: ContainerLayout::Stack {
                    axis: StackAxis::Vertical,
                    spacing: 0.0,
                    padding: 0.0,
                    cross_axis: Default::default(),
                    resize: StackResizePolicy::Preserve,
                },
            },
        );
    });
}

#[test]
fn stripes_fill_unfilled_cells_by_row_and_column_parity() {
    let (mut sheet, table) = sheet_with_table(3, 2);
    let blue = Color::rgb(0, 0, 255);
    sheet
        .set_canvas_table_stripes(
            table,
            TableStripes {
                even_rows: Some(DEFAULT_TABLE_STRIPE),
                ..TableStripes::default()
            },
        )
        .unwrap();
    let fill = |sheet: &Sheet, row, column| {
        let object = sheet
            .canvas
            .object(cell(sheet, table, row, column))
            .unwrap();
        stripe_fill(&sheet.canvas, object)
    };
    assert_eq!(fill(&sheet, 0, 0), None);
    assert_eq!(fill(&sheet, 1, 1), Some(DEFAULT_TABLE_STRIPE));
    sheet
        .set_canvas_table_stripes(
            table,
            TableStripes {
                even_rows: Some(DEFAULT_TABLE_STRIPE),
                odd_columns: Some(blue),
                ..TableStripes::default()
            },
        )
        .unwrap();
    assert_eq!(fill(&sheet, 0, 0), Some(blue));
    assert_eq!(
        fill(&sheet, 1, 0),
        Some(DEFAULT_TABLE_STRIPE),
        "rows paint over columns"
    );
    let own = cell(&sheet, table, 2, 0);
    sheet.canvas.object_mut(own).unwrap().style.fill = Some(Color::BLACK);
    assert_eq!(fill(&sheet, 2, 0), None, "own fill wins");
}

#[test]
fn table_commands_undo_as_single_edits() {
    let mut document = Document::new(Workbook::new(Sheet::new("Board", "Root")));
    let table = document
        .insert_canvas_table(Vec2::ZERO, 2, 2, None)
        .unwrap();
    document
        .insert_canvas_table_track(table, TableAxis::Column, 2)
        .unwrap();
    document
        .resize_canvas_table_track(table, TableAxis::Column, 2, 60.0)
        .unwrap();
    let sheet = document.sheet();
    assert_eq!(
        sheet.canvas.scene.tables[&table].columns,
        vec![120.0, 120.0, 60.0]
    );
    assert_eq!(sheet.canvas.object(table).unwrap().rect.size.x, 300.0);
    assert!(
        document
            .resize_canvas_table_track(table, TableAxis::Row, 0, 1.0)
            .is_err()
    );
    document.undo();
    document.undo();
    assert_eq!(
        document.sheet().canvas.scene.tables[&table].columns.len(),
        2
    );
    document.undo();
    assert!(!document.workbook().has_tables());
}
