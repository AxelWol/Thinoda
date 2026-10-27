use super::*;
use crate::app::Cmd;
use egui_kittest::{Harness, kittest::Queryable};

fn draft(app: &mut ThinodaApp, ctx: &Context) -> PlanningEditor {
    app.begin_task_planning(ctx, false);
    app.planning.editor.take().unwrap()
}

#[test]
fn exception_picker_accepts_nonworking_days_and_escape_only_closes_calendar() {
    let app = ThinodaApp::headless();
    let mut h = Harness::builder()
        .with_size(egui::vec2(600.0, 720.0))
        .build_ui_state(
            |ui, app: &mut ThinodaApp| app.planning_dialog(ui.ctx()),
            app,
        );
    let ctx = h.ctx.clone();
    h.state_mut().begin_task_planning(&ctx, true);
    if let PlanningDraft::Calendar { date, .. } =
        &mut h.state_mut().planning.editor.as_mut().unwrap().draft
    {
        *date = "2026-10-26".into();
    }
    h.run_steps(3);
    h.get_by_label(t("Open calendar")).click();
    h.run_steps(3);
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert!(h.state().planning.editing());
    h.get_by_label(t("Open calendar")).click();
    h.run_steps(2);
    h.get_by_label("2026-10-31").click();
    h.run_steps(2);
    h.get_by_label(t("Apply")).click();
    h.run_steps(2);
    assert!(!h.state().planning.editing());
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .scheduling
            .calendar
            .exceptions
            .get(&"2026-10-31".parse().unwrap()),
        Some(&false)
    );
}

#[test]
fn typed_nonworking_dates_and_inverted_ranges_are_rejected_without_edits() {
    let mut app = ThinodaApp::headless();
    let ctx = Context::default();
    let before = app.doc().unwrap().doc.workbook().clone();
    for (start_value, due_value) in [
        ("2026-10-31", ""),
        ("2026-10-26", "2026-10-25"),
        ("2026-10-26", "2026-10-31"),
        ("", "2026-10-30"),
    ] {
        let mut editor = draft(&mut app, &ctx);
        if let PlanningDraft::Task {
            start,
            finish,
            task,
            ..
        } = &mut editor.draft
        {
            *start = start_value.into();
            *finish = due_value.into();
            task.duration_days = Some(1);
        }
        assert!(app.apply_planning_draft(&editor, false).is_err());
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    }
}

#[test]
fn task_edits_are_atomic_undoable_and_preserve_simple_todo() {
    let mut app = ThinodaApp::headless();
    let ctx = Context::default();
    let topic = app.doc().unwrap().view.primary().unwrap();
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("To-do", |sheet| {
            sheet.topic_mut(topic).unwrap().todo = Some(true);
            Ok(())
        })
        .unwrap();
    let before = app.doc().unwrap().doc.workbook().clone();
    let mut editor = draft(&mut app, &ctx);
    if let PlanningDraft::Task { task, start, .. } = &mut editor.draft {
        *start = "2026-10-26".into();
        task.duration_days = Some(5);
        task.progress_percent = 40;
    }
    app.apply_planning_draft(&editor, false).unwrap();
    let doc = &mut app.docs[app.active];
    assert_eq!(doc.doc.sheet().topic(topic).unwrap().todo, Some(true));
    let computed = doc.doc.sheet().compute_schedule().unwrap();
    assert_eq!(
        computed[&topic].finish_date.unwrap().to_string(),
        "2026-10-30"
    );
    doc.doc.undo();
    assert_eq!(doc.doc.workbook(), &before);
    doc.doc.redo();
    let saved = doc.doc.workbook().clone();
    let mut editor = draft(&mut app, &ctx);
    if let PlanningDraft::Task { start, .. } = &mut editor.draft {
        *start = "2026-02-30".into();
    }
    assert!(app.apply_planning_draft(&editor, false).is_err());
    assert_eq!(app.doc().unwrap().doc.workbook(), &saved);
    app.apply_planning_draft(&editor, true).unwrap();
    assert!(
        !app.doc()
            .unwrap()
            .doc
            .sheet()
            .scheduling
            .tasks
            .contains_key(&topic)
    );
    assert_eq!(
        app.doc().unwrap().doc.sheet().topic(topic).unwrap().todo,
        Some(true)
    );
}

#[test]
fn stale_and_cyclic_drafts_never_partially_apply() {
    let mut app = ThinodaApp::headless();
    let ctx = Context::default();
    let topic = app.doc().unwrap().view.primary().unwrap();
    let editor = draft(&mut app, &ctx);
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Changed", |sheet| sheet.set_title(topic, "Changed"))
        .unwrap();
    let before = app.doc().unwrap().doc.workbook().clone();
    assert!(app.apply_planning_draft(&editor, false).is_err());
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    let mut editor = draft(&mut app, &ctx);
    if let PlanningDraft::Task { predecessors, .. } = &mut editor.draft {
        predecessors.insert(topic);
    }
    assert!(app.apply_planning_draft(&editor, false).is_err());
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
}

#[test]
fn short_viewport_cancel_discards_task_draft() {
    let app = ThinodaApp::headless();
    let mut h = Harness::builder()
        .with_size(egui::vec2(360.0, 300.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    let before = h.state().doc().unwrap().doc.workbook().clone();
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::TaskPlanning);
    h.run_steps(3);
    let cancel = h.get_by_role_and_label(egui::accesskit::Role::Button, "Cancel");
    assert!(h.ctx.content_rect().contains_rect(cancel.rect()));
    cancel.click();
    h.run_steps(3);
    assert!(!h.state().planning.editing());
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
}

#[test]
fn calendar_changes_recompute_tasks_and_undo_together() {
    let mut app = ThinodaApp::headless();
    let ctx = Context::default();
    let topic = app.doc().unwrap().view.primary().unwrap();
    app.doc_mut()
        .unwrap()
        .doc
        .set_task(
            topic,
            Some(Task {
                start_date: Some("2026-10-26".parse().unwrap()),
                duration_days: Some(5),
                ..Task::default()
            }),
        )
        .unwrap();
    let before = app.doc().unwrap().doc.workbook().clone();
    app.begin_task_planning(&ctx, true);
    let mut editor = app.planning.editor.take().unwrap();
    if let PlanningDraft::Calendar { calendar, .. } = &mut editor.draft {
        calendar
            .exceptions
            .insert("2026-10-28".parse().unwrap(), false);
    }
    app.apply_planning_draft(&editor, false).unwrap();
    assert_eq!(
        app.doc().unwrap().doc.sheet().compute_schedule().unwrap()[&topic]
            .finish_date
            .unwrap()
            .to_string(),
        "2026-11-02"
    );
    app.doc_mut().unwrap().doc.undo();
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    app.begin_task_planning(&ctx, true);
    let mut editor = app.planning.editor.take().unwrap();
    if let PlanningDraft::Calendar { calendar, .. } = &mut editor.draft {
        calendar.working_weekdays.clear();
    }
    assert!(app.apply_planning_draft(&editor, false).is_err());
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
}

#[test]
fn planning_draft_stays_with_its_opening_viewport() {
    let mut app = ThinodaApp::headless();
    let ctx = Context::default();
    app.begin_task_planning(&ctx, false);
    let before = app.doc().unwrap().doc.workbook().clone();
    app.cancel_planning_viewport(egui::ViewportId::from_hash_of("other"));
    assert!(app.planning.editing());
    app.cancel_planning_viewport(egui::ViewportId::ROOT);
    assert!(!app.planning.editing());
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    app.begin_task_planning(&ctx, false);
    let child = egui::ViewportId::from_hash_of("document-child");
    app.planning.editor.as_mut().unwrap().viewport = child;
    app.return_planning_viewport(child);
    assert_eq!(
        app.planning.editor.as_ref().unwrap().viewport,
        egui::ViewportId::ROOT
    );
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
}

#[test]
fn ime_candidate_escape_does_not_dismiss_a_planning_draft() {
    let app = ThinodaApp::headless();
    let mut h = Harness::builder()
        .with_size(egui::vec2(600.0, 600.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::TaskPlanning);
    h.run_steps(3);
    h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
        text: "2026".into(),
        active_range_chars: None,
    }));
    h.event(egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(1);
    assert!(h.state().planning.editing());
}

#[test]
fn task_draft_keeps_the_underlying_canvas_read_only() {
    let app = ThinodaApp::headless();
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 700.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::TaskPlanning);
    h.run_steps(3);
    let before = h.state().doc().unwrap().doc.workbook().clone();
    h.ctx
        .memory_mut(|memory| memory.request_focus(thinoda_canvas::MapCanvas::id()));
    h.key_press(egui::Key::Tab);
    h.run_steps(3);
    assert!(h.state().planning.editing());
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
}

#[test]
fn task_dependencies_recompute_successors_with_one_undo() {
    let mut app = ThinodaApp::headless();
    let ctx = Context::default();
    let predecessor = app.doc().unwrap().view.primary().unwrap();
    app.doc_mut()
        .unwrap()
        .doc
        .set_task(
            predecessor,
            Some(Task {
                start_date: Some("2026-10-26".parse().unwrap()),
                duration_days: Some(5),
                ..Task::default()
            }),
        )
        .unwrap();
    app.exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::InsertChild));
    let successor = app.doc().unwrap().view.primary().unwrap();
    assert_ne!(predecessor, successor);
    let mut editor = draft(&mut app, &ctx);
    let before = app.doc().unwrap().doc.workbook().clone();
    if let PlanningDraft::Task {
        task, predecessors, ..
    } = &mut editor.draft
    {
        task.duration_days = Some(2);
        predecessors.insert(predecessor);
    }
    app.apply_planning_draft(&editor, false).unwrap();
    let computed = app.doc().unwrap().doc.sheet().compute_schedule().unwrap()[&successor];
    assert_eq!(computed.start_date.unwrap().to_string(), "2026-11-02");
    assert_eq!(computed.finish_date.unwrap().to_string(), "2026-11-03");
    app.doc_mut().unwrap().doc.undo();
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
}
