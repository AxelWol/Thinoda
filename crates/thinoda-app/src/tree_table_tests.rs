use super::*;
use egui_kittest::{Harness, kittest::Queryable};

fn app() -> ThinodaApp {
    let mut app = ThinodaApp::headless();
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Table", |sheet| {
            sheet.structure = Structure::TreeTable;
            Ok(())
        })
        .unwrap();
    app
}

fn edit(app: &mut ThinodaApp, column: Column, value: &str) -> Result<(), String> {
    let topic = app.doc().unwrap().view.primary().unwrap();
    app.begin_table_cell(topic, column);
    let mut draft = app.tree_table.draft.take().unwrap();
    draft.value = value.into();
    app.apply_table_cell(&Context::default(), &draft)
}

#[test]
fn actual_cells_are_atomic_undoable_and_do_not_change_todo() {
    let mut app = app();
    let topic = app.doc().unwrap().view.primary().unwrap();
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("To-Do", |sheet| {
            sheet.topic_mut(topic).unwrap().todo = Some(true);
            Ok(())
        })
        .unwrap();
    for (column, value) in [
        (Column::Title, "Table title"),
        (Column::Start, "2026-10-30"),
        (Column::Duration, "2"),
        (Column::Progress, "45"),
        (Column::Due, "2026-11-03"),
    ] {
        let before = app.doc().unwrap().doc.workbook().clone();
        edit(&mut app, column, value).unwrap();
        let after = app.doc().unwrap().doc.workbook().clone();
        assert_ne!(before, after);
        let doc = app.doc_mut().unwrap();
        doc.doc.undo();
        assert_eq!(doc.doc.workbook(), &before);
        doc.doc.redo();
        assert_eq!(doc.doc.workbook(), &after);
        assert_eq!(doc.doc.sheet().topic(topic).unwrap().todo, Some(true));
    }
    let doc = app.doc().unwrap();
    assert_eq!(
        doc.doc.sheet().scheduling.tasks[&topic].duration_days,
        Some(3)
    );
    assert_eq!(
        doc.doc.sheet().compute_schedule().unwrap()[&topic]
            .finish_date
            .unwrap()
            .to_string(),
        "2026-11-03"
    );
}

#[test]
fn invalid_cells_never_mutate_or_add_history() {
    let mut app = app();
    for (column, value) in [
        (Column::Start, "2026-02-30"),
        (Column::Start, "2026-10-31"),
        (Column::Duration, "-1"),
        (Column::Duration, "1.5"),
        (Column::Duration, "3651"),
        (Column::Progress, "-1"),
        (Column::Progress, "101"),
        (Column::Progress, "999"),
        (Column::Due, "2026-10-30"),
    ] {
        let before = app.doc().unwrap().doc.workbook().clone();
        let revision = app.doc().unwrap().doc.state_id();
        assert!(edit(&mut app, column, value).is_err(), "{column:?}/{value}");
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        assert_eq!(app.doc().unwrap().doc.state_id(), revision);
    }
}

#[test]
fn blank_untouched_cells_do_not_schedule_and_duration_can_be_cleared() {
    let mut app = app();
    let before = app.doc().unwrap().doc.workbook().clone();
    for column in [Column::Start, Column::Due, Column::Duration] {
        edit(&mut app, column, "").unwrap();
    }
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    edit(&mut app, Column::Start, "2026-10-30").unwrap();
    edit(&mut app, Column::Duration, "2").unwrap();
    edit(&mut app, Column::Duration, "0").unwrap();
    let sheet = app.doc().unwrap().doc.sheet();
    let topic = sheet.root;
    assert_eq!(sheet.compute_schedule().unwrap()[&topic].finish_date, None);
}

#[test]
fn computed_dependency_dates_and_working_exceptions_use_shared_engine() {
    let mut app = app();
    let root = app.doc().unwrap().doc.sheet().root;
    let child = app
        .doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Child", |sheet| {
            Ok(sheet.add_child(root, None, "Successor"))
        })
        .unwrap();
    edit(&mut app, Column::Start, "2026-10-30").unwrap();
    edit(&mut app, Column::Duration, "1").unwrap();
    app.doc_mut().unwrap().view.select_only(child);
    edit(&mut app, Column::Duration, "1").unwrap();
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Dependency", |sheet| {
            sheet.add_task_dependency(root, child).unwrap();
            sheet
                .scheduling
                .calendar
                .exceptions
                .insert("2026-11-03".parse().unwrap(), false);
            Ok(())
        })
        .unwrap();
    edit(&mut app, Column::Due, "2026-11-04").unwrap();
    let sheet = app.doc().unwrap().doc.sheet();
    assert_eq!(sheet.scheduling.tasks[&child].start_date, None);
    assert_eq!(sheet.scheduling.tasks[&child].duration_days, Some(2));
    assert_eq!(
        sheet.compute_schedule().unwrap()[&child]
            .start_date
            .unwrap()
            .to_string(),
        "2026-11-02"
    );
    let before = app.doc().unwrap().doc.workbook().clone();
    assert!(edit(&mut app, Column::Due, "2026-11-03").is_err());
    assert!(edit(&mut app, Column::Duration, "0").is_ok());
    app.doc_mut().unwrap().doc.undo();
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
}

#[test]
fn stale_drafts_reject_revision_sheet_selection_and_view_switches() {
    let mut app = app();
    let root = app.doc().unwrap().doc.sheet().root;
    app.begin_table_cell(root, Column::Title);
    let mut draft = app.tree_table.draft.take().unwrap();
    draft.value = "Stale".into();
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Other edit", |sheet| sheet.set_title(root, "Fresh"))
        .unwrap();
    assert!(app.apply_table_cell(&Context::default(), &draft).is_err());
    app.begin_table_cell(root, Column::Title);
    let draft = app.tree_table.draft.take().unwrap();
    app.mode = CenterMode::Outliner;
    assert!(app.apply_table_cell(&Context::default(), &draft).is_err());
    app.tree_table.draft = Some(draft);
    app.guard_table_draft(&Context::default());
    assert!(app.tree_table.draft.is_none());
}

#[test]
fn sibling_sort_and_collapse_are_read_only_and_keep_identity() {
    let mut app = app();
    let root = app.doc().unwrap().doc.sheet().root;
    let (z, a) = app
        .doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Rows", |sheet| {
            let z = sheet.add_child(root, None, "Zulu");
            sheet.add_child(z, None, "Nested");
            Ok((z, sheet.add_child(root, None, "Alpha")))
        })
        .unwrap();
    let sheet = app.doc().unwrap().doc.sheet();
    let before = sheet.clone();
    let sorted = rows(
        sheet,
        &BTreeMap::new(),
        &BTreeSet::new(),
        TreeTableSort::Title,
    );
    let a_index = sorted.iter().position(|(id, _)| *id == a).unwrap();
    let z_index = sorted.iter().position(|(id, _)| *id == z).unwrap();
    assert!(a_index < z_index);
    assert_eq!(sorted[z_index], (z, 1));
    assert_eq!(sorted[z_index + 1].1, 2);
    let collapsed = rows(
        sheet,
        &BTreeMap::new(),
        &BTreeSet::from([z]),
        TreeTableSort::Title,
    );
    assert_eq!(collapsed.len(), sorted.len() - 1);
    assert_eq!(sheet, &before);
    assert!(
        sheet.topics[&root].children.iter().position(|id| *id == z)
            < sheet.topics[&root].children.iter().position(|id| *id == a)
    );
}

#[test]
fn table_surface_edits_title_and_escape_discards() {
    let app = app();
    let title = app.doc().unwrap().doc.sheet().topics[&app.doc().unwrap().doc.sheet().root]
        .title
        .clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 700.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.tree_table_ui(ui), app);
    h.run_steps(3);
    let label = format!("{}: {title}", t("Topic title"));
    h.get_by_label(&label).click();
    h.run_steps(3);
    let input = h.get_by_role_and_label(egui::Role::TextInput, &label);
    input.type_text(" edited");
    h.run_steps(2);
    assert_eq!(
        h.ctx.memory(|m| m.focused()),
        Some(h.state().tree_table.draft.as_ref().unwrap().editor_id)
    );
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert_eq!(
        h.state().doc().unwrap().doc.sheet().topics[&h.state().doc().unwrap().doc.sheet().root]
            .title,
        title
    );
    assert!(h.state().tree_table.draft.is_none());
    h.get_by_label(&label).click();
    h.run_steps(2);
    h.get_by_role_and_label(egui::Role::TextInput, &label)
        .type_text(" edited");
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_ne!(
        h.state().doc().unwrap().doc.sheet().topics[&h.state().doc().unwrap().doc.sheet().root]
            .title,
        title
    );
}

#[test]
fn keyboard_tab_applies_task_cell_and_arrows_synchronize_selection() {
    let app = app();
    let root = app.doc().unwrap().doc.sheet().root;
    let title = app.doc().unwrap().doc.sheet().topics[&root].title.clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1200.0, 700.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.tree_table_ui(ui), app);
    h.run_steps(3);
    let label = format!("{}: {title}", t("Start date"));
    h.get_by_label(&label).click();
    h.run_steps(2);
    h.get_by_role_and_label(egui::Role::TextInput, &label)
        .type_text("2026-10-30");
    h.run_steps(2);
    h.key_press(Key::Tab);
    h.run_steps(3);
    assert!(h.state().tree_table.draft.is_none());
    assert_eq!(
        h.state().doc().unwrap().doc.sheet().scheduling.tasks[&root]
            .start_date
            .unwrap()
            .to_string(),
        "2026-10-30"
    );
    h.key_press(Key::ArrowDown);
    h.run_steps(3);
    let child = h.state().doc().unwrap().doc.sheet().topics[&root].children[0];
    assert_eq!(h.state().doc().unwrap().view.primary(), Some(child));
    h.state_mut().open_gantt();
    assert_eq!(h.state().doc().unwrap().view.primary(), Some(child));
    h.state_mut().tree_table.map_preview = true;
    assert_eq!(h.state().doc().unwrap().view.primary(), Some(child));
}

#[test]
fn errors_are_visible_and_ime_candidate_keys_do_not_apply() {
    let app = app();
    let root = app.doc().unwrap().doc.sheet().root;
    let title = app.doc().unwrap().doc.sheet().topics[&root].title.clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1200.0, 700.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.tree_table_ui(ui), app);
    let before = h.state().doc().unwrap().doc.workbook().clone();
    h.run_steps(3);
    let label = format!("{}: {title}", t("Progress"));
    h.get_by_label(&label).click();
    h.run_steps(2);
    h.get_by_role_and_label(egui::Role::TextInput, &label)
        .type_text("999");
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert!(h.state().tree_table.draft.as_ref().unwrap().error.is_some());
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
        text: "9".into(),
        active_range_chars: None,
    }));
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert!(h.state().tree_table.draft.is_some());
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
}

#[test]
fn table_values_roundtrip_in_native_storage() {
    let mut app = app();
    edit(&mut app, Column::Title, "Persisted title").unwrap();
    edit(&mut app, Column::Start, "2026-10-30").unwrap();
    edit(&mut app, Column::Duration, "2").unwrap();
    edit(&mut app, Column::Progress, "55").unwrap();
    let book = app.doc().unwrap().doc.workbook();
    let bytes =
        thinoda_io::export_bytes(book, thinoda_io::Format::Thinoda, &Default::default()).unwrap();
    let reopened = thinoda_io::import_bytes(thinoda_io::Format::Thinoda, &bytes).unwrap();
    assert_eq!(&reopened, book);
    let sheet = reopened.active();
    assert_eq!(
        sheet.compute_schedule().unwrap()[&sheet.root]
            .finish_date
            .unwrap()
            .to_string(),
        "2026-11-02"
    );
}

#[test]
fn context_switches_discard_drafts_without_applying_to_another_owner() {
    let mut app = app();
    let root = app.doc().unwrap().doc.sheet().root;
    app.begin_table_cell(root, Column::Title);
    let draft = app.tree_table.draft.take().unwrap();
    let before = app.doc().unwrap().doc.workbook().clone();
    app.exec(&Context::default(), Cmd::NewSheet);
    assert!(app.apply_table_cell(&Context::default(), &draft).is_err());
    app.doc_mut().unwrap().doc.undo();
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    app.begin_table_cell(root, Column::Title);
    let draft = app.tree_table.draft.take().unwrap();
    let child = app.doc().unwrap().doc.sheet().topics[&root].children[0];
    app.doc_mut().unwrap().view.select_only(child);
    assert!(app.apply_table_cell(&Context::default(), &draft).is_err());
    app.begin_table_cell(child, Column::Title);
    let draft = app.tree_table.draft.take().unwrap();
    app.exec(&Context::default(), Cmd::New);
    assert!(app.apply_table_cell(&Context::default(), &draft).is_err());
}

#[test]
fn view_choices_and_sorting_never_edit_the_workbook() {
    let app = app();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.tree_table_ui(ui), app);
    let before = h.state().doc().unwrap().doc.workbook().clone();
    let revision = h.state().doc().unwrap().doc.state_id();
    h.run_steps(3);
    h.get_by_label(t("Table columns")).click();
    h.run_steps(2);
    h.get_by_role_and_label(egui::Role::CheckBox, t("Progress"))
        .click();
    h.run_steps(2);
    h.key_press(Key::Escape);
    h.run_steps(2);
    h.get_by_label(t("Sort siblings (view only)")).click();
    h.run_steps(2);
    h.get_by_role_and_label(egui::Role::Button, t("Topic title"))
        .click();
    h.run_steps(2);
    assert_eq!(h.state().settings.tree_table.sort, TreeTableSort::Title);
    assert!(!h.state().settings.tree_table.columns[3]);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);
}

#[test]
fn rejected_touch_gestures_never_begin_a_cell_edit() {
    let app = app();
    let title = app.doc().unwrap().doc.sheet().topics[&app.doc().unwrap().doc.sheet().root]
        .title
        .clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(360.0, 640.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.tree_table_ui(ui), app);
    h.run_steps(3);
    let label = format!("{}: {title}", t("Topic title"));
    let start = h.get_by_label(&label).rect().center();
    let before = h.state().doc().unwrap().doc.workbook().clone();
    for (phase, fraction) in [
        (egui::TouchPhase::Start, 0.0),
        (egui::TouchPhase::Move, 0.2),
        (egui::TouchPhase::Move, 1.0),
        (egui::TouchPhase::End, 1.0),
    ] {
        let pos = start + egui::vec2(-100.0, 0.0) * fraction;
        h.event(egui::Event::PointerMoved(pos));
        h.event(egui::Event::Touch {
            device_id: egui::TouchDeviceId(1),
            id: egui::TouchId(1),
            phase,
            pos,
            force: None,
        });
        if matches!(phase, egui::TouchPhase::Start | egui::TouchPhase::End) {
            h.event(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: phase == egui::TouchPhase::Start,
                modifiers: Modifiers::NONE,
            });
        }
        h.run_steps(1);
    }
    h.run_steps(8);
    assert!(h.state().tree_table.draft.is_none());
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
}

#[test]
fn row_identity_survives_reordering_and_canceled_editor_state_is_fresh() {
    let mut app = app();
    let root = app.doc().unwrap().doc.sheet().root;
    let child = app.doc().unwrap().doc.sheet().topics[&root].children[0];
    app.doc_mut().unwrap().view.select_only(child);
    app.begin_table_cell(child, Column::Title);
    let first_editor = app.tree_table.draft.as_ref().unwrap().editor_id;
    app.begin_table_cell(child, Column::Title);
    assert_ne!(
        first_editor,
        app.tree_table.draft.as_ref().unwrap().editor_id
    );
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Reorder", |sheet| {
            sheet.topic_mut(root).unwrap().children.reverse();
            Ok(())
        })
        .unwrap();
    app.guard_table_draft(&Context::default());
    assert!(app.tree_table.draft.is_none());
    assert_eq!(app.doc().unwrap().view.primary(), Some(child));
    edit(&mut app, Column::Title, "Still the same topic").unwrap();
    assert_eq!(
        app.doc().unwrap().doc.sheet().topics[&child].title,
        "Still the same topic"
    );
}

#[test]
fn keyboard_cancel_button_does_not_accidentally_apply_a_draft() {
    let app = app();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 700.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.tree_table_ui(ui), app);
    h.run_steps(3);
    let root = h.state().doc().unwrap().doc.sheet().root;
    let before = h.state().doc().unwrap().doc.workbook().clone();
    h.state_mut().begin_table_cell(root, Column::Title);
    h.state_mut().tree_table.draft.as_mut().unwrap().value = "Discard me".into();
    h.run_steps(2);
    h.get_by_role_and_label(egui::Role::Button, t("Cancel"))
        .focus();
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert!(h.state().tree_table.draft.is_none());
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
}

#[test]
fn background_viewport_draw_cannot_discard_or_apply_another_viewports_draft() {
    let mut app = app();
    let root = app.doc().unwrap().doc.sheet().root;
    app.begin_table_cell(root, Column::Title);
    let owner = app.current_viewport;
    app.current_viewport = egui::ViewportId::from_hash_of("another-table-window");
    app.guard_table_draft(&Context::default());
    let draft = app.tree_table.draft.take().unwrap();
    assert!(app.apply_table_cell(&Context::default(), &draft).is_err());
    app.tree_table.draft = Some(draft);
    app.current_viewport = owner;
    app.guard_table_draft(&Context::default());
    assert!(app.tree_table.draft.is_some());
}
