use super::*;
use egui_kittest::Harness;

fn scheduled_app() -> (ThinodaApp, TopicId) {
    let mut app = ThinodaApp::headless();
    let topic = app.doc().unwrap().view.primary().unwrap();
    app.doc_mut()
        .unwrap()
        .doc
        .set_task(
            topic,
            Some(Task {
                start_date: Some("2026-10-26".parse().unwrap()),
                duration_days: Some(5),
                progress_percent: 30,
                priority: None,
            }),
        )
        .unwrap();
    (app, topic)
}

#[test]
fn chart_move_resize_and_progress_share_one_undoable_document() {
    let (mut app, topic) = scheduled_app();
    let ctx = Context::default();
    let doc = app.doc().unwrap();
    let key = doc.key.clone();
    let before = doc.doc.workbook().clone();
    let task = doc.doc.sheet().scheduling.tasks[&topic].clone();
    let computed = doc.doc.sheet().compute_schedule().unwrap()[&topic];
    let calendar = doc.doc.sheet().scheduling.calendar.clone();
    let moved = drag_task(&task, computed, &calendar, 7, false).unwrap();
    app.apply_gantt_task(
        &ctx,
        &key,
        0,
        app.doc().unwrap().doc.state_id(),
        topic,
        moved,
    )
    .unwrap();
    assert_eq!(app.docs.len(), 1);
    let doc = app.doc_mut().unwrap();
    assert_eq!(
        doc.doc.sheet().compute_schedule().unwrap()[&topic]
            .start_date
            .unwrap()
            .to_string(),
        "2026-11-02"
    );
    doc.doc.undo();
    assert_eq!(doc.doc.workbook(), &before);
    let resized = drag_task(&task, computed, &calendar, 3, true).unwrap();
    assert_eq!(resized.duration_days, Some(6));
    app.apply_gantt_task(
        &ctx,
        &key,
        0,
        app.doc().unwrap().doc.state_id(),
        topic,
        resized,
    )
    .unwrap();
    app.doc_mut().unwrap().doc.undo();
    let mut progress = task;
    progress.progress_percent = 90;
    app.apply_gantt_task(
        &ctx,
        &key,
        0,
        app.doc().unwrap().doc.state_id(),
        topic,
        progress,
    )
    .unwrap();
    assert_eq!(
        app.doc().unwrap().doc.sheet().scheduling.tasks[&topic].progress_percent,
        90
    );
    app.doc_mut().unwrap().doc.undo();
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
}

#[test]
fn stale_or_invalid_chart_edits_cannot_change_the_map() {
    let (mut app, topic) = scheduled_app();
    let ctx = Context::default();
    let doc = app.doc().unwrap();
    let key = doc.key.clone();
    let revision = doc.doc.state_id();
    let task = doc.doc.sheet().scheduling.tasks[&topic].clone();
    let computed = doc.doc.sheet().compute_schedule().unwrap()[&topic];
    assert!(
        drag_task(
            &task,
            computed,
            &doc.doc.sheet().scheduling.calendar,
            -10,
            true
        )
        .is_err()
    );
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Change", |sheet| sheet.set_title(topic, "Changed"))
        .unwrap();
    let before = app.doc().unwrap().doc.workbook().clone();
    assert!(
        app.apply_gantt_task(&ctx, &key, 0, revision, topic, task)
            .is_err()
    );
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
}

#[test]
fn chart_projection_does_not_create_a_document_owner() {
    let (mut app, _) = scheduled_app();
    let before = app.doc().unwrap().doc.workbook().clone();
    app.open_gantt();
    let (key, sheet) = app.gantt.owner.clone().unwrap();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(800.0, 500.0))
        .build_ui_state(
            |ui, app: &mut ThinodaApp| app.gantt_ui(ui, &key, sheet),
            app,
        );
    harness.run_steps(3);
    assert_eq!(harness.state().docs.len(), 1);
    assert_eq!(harness.state().doc().unwrap().doc.workbook(), &before);
}

#[test]
fn focused_chart_limits_commands_and_close_targets_only_its_viewport() {
    let (mut app, _) = scheduled_app();
    let ctx = Context::default();
    app.open_gantt();
    let (key, sheet) = app.gantt.owner.clone().unwrap();
    let viewport = egui::ViewportId::from_hash_of(("gantt", &key, sheet));
    app.current_viewport = viewport;
    assert!(app.command_available(&Cmd::CloseWindow));
    assert!(app.command_available(&Cmd::Canvas(thinoda_canvas::Action::Undo)));
    assert!(!app.command_available(&Cmd::Canvas(thinoda_canvas::Action::Delete)));
    assert!(!app.command_available(&Cmd::CloseTab(0)));
    let before = app.doc().unwrap().doc.workbook().clone();
    let mut input = egui::RawInput {
        viewport_id: viewport,
        ..Default::default()
    };
    input
        .viewports
        .insert(viewport, egui::ViewportInfo::default());
    let mut output = ctx.run_ui(input, |_| app.exec(&ctx, Cmd::CloseWindow));
    output.textures_delta.clear();
    assert!(
        output.viewport_output[&viewport]
            .commands
            .contains(&egui::ViewportCommand::Close)
    );
    assert_eq!(app.docs.len(), 1);
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
}

#[test]
fn chart_outputs_use_schedule_and_preserve_the_map() {
    let (app, _) = scheduled_app();
    let sheet = app.doc().unwrap().doc.sheet();
    let before = sheet.clone();
    assert!(
        ThinodaApp::gantt_bytes(sheet, "svg")
            .unwrap()
            .starts_with(b"<svg")
    );
    assert!(
        ThinodaApp::gantt_bytes(sheet, "pdf")
            .unwrap()
            .starts_with(b"%PDF")
    );
    assert!(
        ThinodaApp::gantt_bytes(sheet, "png")
            .unwrap()
            .starts_with(b"\x89PNG")
    );
    let csv = String::from_utf8(ThinodaApp::gantt_bytes(sheet, "csv").unwrap()).unwrap();
    assert!(csv.contains("2026-10-26"));
    assert!(csv.contains("2026-10-30"));
    assert!(ThinodaApp::gantt_bytes(sheet, "unsupported").is_err());
    assert_eq!(sheet, &before);
}

#[test]
fn bar_drag_accumulates_motion_and_commits_only_once_on_release() {
    let (mut app, topic) = scheduled_app();
    app.open_gantt();
    let (key, sheet) = app.gantt.owner.clone().unwrap();
    let before = app.doc().unwrap().doc.workbook().clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1200.0, 600.0))
        .build_ui_state(
            |ui, app: &mut ThinodaApp| app.gantt_ui(ui, &key, sheet),
            app,
        );
    h.run_steps(3);
    let bar = h
        .output()
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect)
                if (rect.rect.width() - 90.0).abs() < 0.1
                    && (rect.rect.height() - 20.0).abs() < 0.1 =>
            {
                Some(rect.rect)
            }
            _ => None,
        })
        .expect("five-day task bar");
    let start = bar.left_center() + egui::vec2(12.0, 0.0);
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    h.event(egui::Event::PointerMoved(start));
    h.event(button(start, true));
    h.run_steps(1);
    let middle = start + egui::vec2(36.0, 0.0);
    h.event(egui::Event::PointerMoved(middle));
    h.run_steps(1);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    let finish = start + egui::vec2(72.0, 0.0);
    h.event(egui::Event::PointerMoved(finish));
    h.run_steps(1);
    h.event(button(finish, false));
    h.run_steps(3);
    assert_eq!(
        h.state().doc().unwrap().doc.sheet().scheduling.tasks[&topic]
            .start_date
            .unwrap()
            .to_string(),
        "2026-10-30"
    );
    h.state_mut().doc_mut().unwrap().doc.undo();
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    h.run_steps(3);
    h.event(egui::Event::PointerMoved(start));
    h.event(button(start, true));
    h.run_steps(1);
    h.event(egui::Event::PointerMoved(middle));
    h.run_steps(1);
    h.event(egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(1);
    h.event(button(middle, false));
    h.run_steps(3);
    assert_eq!(
        h.state().doc().unwrap().doc.workbook(),
        &before,
        "Escape cancels the whole gesture"
    );

    h.event(egui::Event::PointerMoved(start));
    h.event(button(start, true));
    h.run_steps(1);
    h.event(egui::Event::PointerMoved(middle));
    h.run_steps(1);
    h.state_mut()
        .doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Concurrent change", |sheet| {
            sheet.set_title(topic, "Changed elsewhere")
        })
        .unwrap();
    let concurrent = h.state().doc().unwrap().doc.workbook().clone();
    h.event(button(middle, false));
    h.run_steps(3);
    assert_eq!(
        h.state().doc().unwrap().doc.workbook(),
        &concurrent,
        "An old pointer gesture cannot overwrite a newer revision"
    );
}
