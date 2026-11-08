use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use egui::{
    Event, Modifiers, PointerButton, Pos2, TouchDeviceId, TouchId, TouchPhase, ViewportCommand,
    ViewportEvent, ViewportId, accesskit::Role, vec2,
};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use thinoda_canvas::{
    BoardTool,
    ink_controller::InkControllerError,
    ink_input::{InkSample, MAX_INPUT_SAMPLES},
    pen_input::{self, PenInputIssue, PenInputMatcher, PenSample, PointerPhase, PointerRecord},
    touch_input::{self, TouchForceRecord, TouchPointerIssue, TouchPointerRecord},
};
use thinoda_core::{CanvasObjectKind, Sheet, Vec2, Workbook};
use thinoda_io::Format;
use thinoda_platform::{
    Paths,
    recovery::{RecoveryManager, Snapshot},
};

use crate::{
    app::{CloseIntent, Cmd, ThinodaApp},
    export_preflight,
    file_ops::FileWorkMode,
};

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::current_dir()
            .unwrap()
            .join(format!(".ink-host-check-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn harness() -> Harness<'static, ThinodaApp> {
    let mut app = ThinodaApp::headless();
    let doc = app.doc_mut().unwrap();
    doc.doc
        .edit_book("Second sheet", |book| {
            book.sheets.push(Sheet::new("Other", "Other"));
            Ok(())
        })
        .unwrap();
    doc.doc.mark_saved();
    let mut h = Harness::builder()
        .with_size(vec2(1400.0, 900.0))
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(3);
    h.state_mut()
        .doc_mut()
        .unwrap()
        .view
        .set_board_tool(BoardTool::Pen);
    h.run_steps(2);
    h
}

fn restore_after_crash(
    source: RecoveryManager,
    paths: &Paths,
) -> (ThinodaApp, Arc<RecoveryManager>) {
    assert!(source.try_flush_for_crash());
    source.suspend().unwrap();
    drop(source);
    let orphans = RecoveryManager::find_orphans(paths);
    assert_eq!(orphans.len(), 1);
    let target = Arc::new(RecoveryManager::start(paths).unwrap());
    let mut app = ThinodaApp::headless();
    app.services.recovery = Some(target.clone());
    app.dialogs.recovery = orphans;
    app.restore_orphans(None);
    assert!(app.dialogs.recovery.is_empty());
    (app, target)
}

fn restored_harness(app: ThinodaApp) -> Harness<'static, ThinodaApp> {
    let mut h = Harness::builder()
        .with_size(vec2(1400.0, 900.0))
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(3);
    h
}

fn start(h: &mut Harness<'_, ThinodaApp>) -> Pos2 {
    let canvas = h.state().doc().unwrap().view.canvas_rect();
    let position = canvas.left_top() + vec2(canvas.width() * 0.3, canvas.height() * 0.65);
    h.hover_at(position);
    h.run_steps(1);
    h.event(Event::PointerButton {
        pos: position,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(1);
    assert!(h.state().doc().unwrap().view.has_pending_ink_gesture());
    position
}

fn move_to(h: &mut Harness<'_, ThinodaApp>, position: Pos2) {
    h.event(Event::PointerMoved(position));
    h.run_steps(1);
}

fn release(h: &mut Harness<'_, ThinodaApp>, position: Pos2) {
    h.event(Event::PointerButton {
        pos: position,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(2);
}

fn touch(h: &mut Harness<'_, ThinodaApp>, phase: TouchPhase, pos: Pos2, force: Option<f32>) {
    h.event(Event::PointerMoved(pos));
    h.event(Event::Touch {
        device_id: TouchDeviceId(1),
        id: TouchId(1),
        phase,
        pos,
        force,
    });
    if matches!(phase, TouchPhase::Start | TouchPhase::End) {
        h.event(Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: phase == TouchPhase::Start,
            modifiers: Modifiers::NONE,
        });
    }
    h.run_steps(1);
}

fn pen(
    h: &mut Harness<'_, ThinodaApp>,
    matcher: &mut PenInputMatcher,
    phase: PointerPhase,
    pos: Pos2,
    pressure: f32,
) {
    let event = match phase {
        PointerPhase::Move => Event::PointerMoved(pos),
        PointerPhase::Press(button) | PointerPhase::Release(button) => Event::PointerButton {
            pos,
            button,
            pressed: matches!(phase, PointerPhase::Press(_)),
            modifiers: Modifiers::NONE,
        },
    };
    let events = [event];
    let records = [PointerRecord {
        phase,
        position: pos,
        pen: Some(PenSample { id: 7, pressure }),
    }];
    let matched = matcher.match_frame(&events, &records, None, false);
    assert_eq!(matched.issue, None);
    pen_input::publish(&h.ctx, ViewportId::ROOT, &events, matched);
    h.event(events[0].clone());
    h.run_steps(1);
}

fn touch_pointer(
    h: &mut Harness<'_, ThinodaApp>,
    position: Pos2,
    pressed: bool,
    recorded_position: Pos2,
) -> Option<TouchPointerIssue> {
    let event = Event::PointerButton {
        pos: position,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    let record = TouchPointerRecord {
        phase: if pressed {
            PointerPhase::Press(PointerButton::Primary)
        } else {
            PointerPhase::Release(PointerButton::Primary)
        },
        position: recorded_position,
        is_touch: true,
    };
    let issue = touch_input::publish_pointer_frame(
        &h.ctx,
        ViewportId::ROOT,
        std::slice::from_ref(&event),
        &[record],
        false,
        &[],
        false,
    );
    h.event(event);
    h.run_steps(1);
    issue
}

fn failed_drawing(h: &mut Harness<'_, ThinodaApp>) {
    h.state_mut()
        .doc_mut()
        .unwrap()
        .view
        .ink_settings
        .finish
        .tolerance = -1.0;
    let p = start(h);
    move_to(h, p + vec2(30.0, 12.0));
    release(h, p + vec2(60.0, 20.0));
    assert!(h.state().doc().unwrap().view.has_released_ink_gesture());
}

fn close_event(app: &mut ThinodaApp, viewport: ViewportId) -> Vec<ViewportCommand> {
    let ctx = egui::Context::default();
    let mut input = egui::RawInput {
        viewport_id: viewport,
        ..Default::default()
    };
    input
        .viewports
        .entry(viewport)
        .or_default()
        .events
        .push(ViewportEvent::Close);
    let mut output = ctx.run_ui(input, |ui| app.handle_close_request(ui.ctx()));
    output.textures_delta.clear();
    output.viewport_output[&viewport].commands.clone()
}

fn choose_close(h: &mut Harness<'_, ThinodaApp>, label: &str) {
    let area = h
        .ctx
        .memory(|memory| memory.area_rect(egui::Id::unique("thinoda_close")))
        .unwrap();
    h.query_all_by_role_and_label(Role::Button, label)
        .find(|button| area.contains_rect(button.rect()))
        .unwrap()
        .click();
    h.run_steps(2);
}

#[test]
fn pending_ink_blocks_saves_drafts_exports_sheet_changes_and_tab_close() {
    let scratch = Scratch::new();
    let path = scratch.path().join("existing.thinoda");
    let sheet_path = scratch.path().join("sheet.thinoda");
    std::fs::write(&path, b"preserve existing file").unwrap();
    std::fs::write(&sheet_path, b"preserve sheet file").unwrap();
    let mut h = harness();
    let p = start(&mut h);
    move_to(&mut h, p + vec2(25.0, 10.0));
    let samples = h.state().doc().unwrap().view.pending_ink_samples().to_vec();
    let workbook = h.state().doc().unwrap().doc.workbook().clone();
    assert!(h.state().doc().unwrap().is_dirty());
    {
        let doc = h.state_mut().doc_mut().unwrap();
        assert!(doc.save_to(&path, false).is_err());
        assert!(doc.save_sheet_to(&sheet_path, false).is_err());
        assert!(doc.document_with_drafts().is_err());
        assert!(doc.commit_canvas_drafts().is_err());
        assert!(doc.switch_sheet(1).is_err());
        assert!(export_preflight::save_losses(doc, true, Format::Thinoda).is_err());
        assert_eq!(doc.view.pending_ink_samples(), samples);
        assert_eq!(doc.doc.workbook(), &workbook);
    }
    assert_eq!(std::fs::read(&path).unwrap(), b"preserve existing file");
    assert_eq!(std::fs::read(&sheet_path).unwrap(), b"preserve sheet file");
    let ctx = h.ctx.clone();
    for cmd in [
        Cmd::NewSheet,
        Cmd::DuplicateSheet,
        Cmd::DeleteSheet,
        Cmd::NewSheetFromTopic,
        Cmd::CopySheet,
        Cmd::PasteSheet,
        Cmd::Print,
        Cmd::Export(Format::Svg),
    ] {
        h.state_mut().exec(&ctx, cmd);
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &workbook);
        assert_eq!(h.state().doc().unwrap().view.pending_ink_samples(), samples);
    }
    h.state_mut()
        .request_export_preflight(Format::Svg, false, false);
    assert!(h.state().dialogs.board_export.is_none());
    h.state_mut().request_close_tab(0);
    assert_eq!(h.state().docs.len(), 1);
    assert!(h.state().dialogs.close.is_none());
    assert!(h.state().doc().unwrap().view.has_released_ink_gesture());
    h.state_mut().close_tab_now(0);
    assert_eq!(h.state().docs.len(), 1);
    assert_eq!(h.state().doc().unwrap().view.pending_ink_samples(), samples);
}

#[test]
fn accessible_retry_preserves_a_failed_gesture_and_cancel_is_explicit() {
    let scratch = Scratch::new();
    let path = scratch.path().join("finished.thinoda");
    let mut h = harness();
    failed_drawing(&mut h);
    let samples = h.state().doc().unwrap().view.pending_ink_samples().to_vec();
    assert!(h.query_by_label("Retry ink").is_some());
    assert!(h.query_by_label("Discard ink").is_some());
    assert!(h.query_by_label("Copy ink samples").is_some());
    assert!(
        h.query_by_label(
            "Adjust ink simplification and retry, or copy the samples and discard the gesture."
        )
        .is_some()
    );
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.ink_settings.finish.tolerance = -2.0;
        assert!(doc.retry_pending_ink().is_err());
        assert_eq!(doc.view.pending_ink_samples(), samples);
        assert!(doc.doc.sheet().canvas.objects.is_empty());
        doc.view.ink_settings.finish.tolerance = 0.5;
    }
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Retry ink").click();
    h.run_steps(3);
    assert!(
        !h.state().doc().unwrap().has_unresolved_ink(),
        "pending={}, feedback={:?}",
        h.state().doc().unwrap().view.has_pending_ink_gesture(),
        h.state().doc().unwrap().ink_feedback
    );
    assert!(matches!(
        h.state().doc().unwrap().doc.sheet().canvas.objects[0].kind,
        CanvasObjectKind::Ink(_)
    ));
    assert!(h.state_mut().save_active_to(&path, false));
    assert!(!h.state().doc().unwrap().is_dirty());
    let saved = std::fs::read(&path).unwrap();
    failed_drawing(&mut h);
    assert!(h.state().doc().unwrap().is_dirty());
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Discard ink").click();
    h.run_steps(3);
    assert!(
        !h.state().doc().unwrap().has_unresolved_ink(),
        "pending={}, feedback={:?}",
        h.state().doc().unwrap().view.has_pending_ink_gesture(),
        h.state().doc().unwrap().ink_feedback
    );
    assert!(!h.state().doc().unwrap().is_dirty());
    assert_eq!(std::fs::read(&path).unwrap(), saved);
    assert_eq!(h.state().doc().unwrap().doc.sheet().canvas.objects.len(), 1);
}

#[test]
fn successful_touch_ink_preserves_style_pressure_undo_redo_and_native_save_reopen() {
    let scratch = Scratch::new();
    let path = scratch.path().join("touch-ink.thinoda");
    let mut h = harness();
    h.get_by_label("Ink color").click();
    h.run_steps(2);
    h.get_by_label("Auto").click();
    h.run_steps(2);
    h.key_press(egui::Key::Escape);
    h.run_steps(2);

    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.ink_settings.color = thinoda_core::Color::rgb(200, 30, 40);
        doc.view.ink_settings.width = 4.0;
        doc.view.ink_settings.opacity = 0.4;
        doc.view.ink_settings.finish.tolerance = 0.0;
    }

    let canvas = h.state().doc().unwrap().view.canvas_rect();
    let start = canvas.left_top() + vec2(canvas.width() * 0.3, canvas.height() * 0.65);
    touch(&mut h, TouchPhase::Start, start, Some(0.2));
    touch(
        &mut h,
        TouchPhase::Move,
        start + vec2(40.0, 20.0),
        Some(0.6),
    );
    touch(&mut h, TouchPhase::End, start + vec2(80.0, 40.0), Some(0.9));

    assert!(!h.state().doc().unwrap().has_unresolved_ink());
    assert!(!h.state().doc().unwrap().view.has_pending_ink_gesture());
    let committed = h.state().doc().unwrap().doc.sheet().canvas.objects.clone();
    assert_eq!(committed.len(), 1);
    let CanvasObjectKind::Ink(ink) = &committed[0].kind else {
        panic!("expected a committed touch ink stroke");
    };
    assert_eq!(ink.tool, thinoda_core::InkTool::Pen);
    assert_eq!(ink.color, thinoda_core::Color::rgb(200, 30, 40));
    assert_eq!(ink.width, 4.0);
    assert_eq!(&**ink.pressure.as_ref().unwrap(), &[0.2, 0.6, 0.9]);
    assert_eq!(committed[0].style.opacity, 0.4);

    {
        let doc = h.state_mut().doc_mut().unwrap();
        assert!(doc.doc.undo());
        assert!(doc.doc.sheet().canvas.objects.is_empty());
        assert!(doc.doc.redo());
        assert_eq!(doc.doc.sheet().canvas.objects, committed);
    }
    assert!(h.state_mut().save_active_to(&path, false));
    assert!(!h.state().doc().unwrap().is_dirty());
    let reopened = crate::doc::OpenDoc::open(&path).unwrap();
    assert!(!reopened.is_dirty());
    assert_eq!(reopened.doc.sheet().canvas.objects, committed);
}

#[test]
fn matched_pen_pressure_survives_host_commit_undo_redo_and_native_save_reopen() {
    let scratch = Scratch::new();
    let path = scratch.path().join("pen-ink.thinoda");
    let mut h = harness();
    h.state_mut()
        .doc_mut()
        .unwrap()
        .view
        .ink_settings
        .finish
        .tolerance = 0.0;
    let canvas = h.state().doc().unwrap().view.canvas_rect();
    let start = canvas.left_top() + vec2(canvas.width() * 0.3, canvas.height() * 0.65);
    h.hover_at(start);
    h.run_steps(1);
    let mut matcher = PenInputMatcher::default();
    pen(
        &mut h,
        &mut matcher,
        PointerPhase::Press(PointerButton::Primary),
        start,
        0.2,
    );
    pen(
        &mut h,
        &mut matcher,
        PointerPhase::Move,
        start + vec2(40.0, 20.0),
        0.6,
    );
    pen(
        &mut h,
        &mut matcher,
        PointerPhase::Release(PointerButton::Primary),
        start + vec2(80.0, 40.0),
        0.9,
    );

    let doc = h.state().doc().unwrap();
    assert!(!doc.has_unresolved_ink());
    let committed = doc.doc.sheet().canvas.objects.clone();
    assert_eq!(committed.len(), 1);
    let CanvasObjectKind::Ink(ink) = &committed[0].kind else {
        panic!("expected a committed pen ink stroke");
    };
    assert_eq!(&**ink.pressure.as_ref().unwrap(), &[0.2, 0.6, 0.9]);
    {
        let doc = h.state_mut().doc_mut().unwrap();
        assert!(doc.doc.undo());
        assert!(doc.doc.sheet().canvas.objects.is_empty());
        assert!(doc.doc.redo());
        assert_eq!(doc.doc.sheet().canvas.objects, committed);
    }
    assert!(h.state_mut().save_active_to(&path, false));
    let reopened = crate::doc::OpenDoc::open(&path).unwrap();
    assert!(!reopened.is_dirty());
    assert_eq!(reopened.doc.sheet().canvas.objects, committed);
}

#[test]
fn separated_touch_pointer_events_commit_once_and_preserve_host_history() {
    let scratch = Scratch::new();
    let path = scratch.path().join("touch-pointer-ink.thinoda");
    let mut h = harness();
    let canvas = h.state().doc().unwrap().view.canvas_rect();
    let start = canvas.left_top() + vec2(canvas.width() * 0.3, canvas.height() * 0.65);
    let end = start + vec2(50.0, 25.0);
    h.hover_at(start);
    h.run_steps(1);
    assert_eq!(touch_pointer(&mut h, start, true, start), None);
    assert!(!h.state().doc().unwrap().view.has_pending_ink_gesture());

    h.event(Event::Touch {
        device_id: TouchDeviceId(1),
        id: TouchId(1),
        phase: TouchPhase::Start,
        pos: start,
        force: Some(0.3),
    });
    h.run_steps(1);
    assert!(h.state().doc().unwrap().view.has_pending_ink_gesture());
    assert_eq!(touch_pointer(&mut h, end, false, end), None);
    assert!(!h.state().doc().unwrap().view.has_released_ink_gesture());
    h.event(Event::Touch {
        device_id: TouchDeviceId(1),
        id: TouchId(1),
        phase: TouchPhase::End,
        pos: end,
        force: Some(0.7),
    });
    h.run_steps(2);
    let doc = h.state().doc().unwrap();
    assert!(!doc.has_unresolved_ink());
    let committed = doc.doc.sheet().canvas.objects.clone();
    assert_eq!(committed.len(), 1);
    let CanvasObjectKind::Ink(ink) = &committed[0].kind else {
        panic!("expected touch ink");
    };
    assert_eq!(&**ink.pressure.as_ref().unwrap(), &[0.3, 0.7]);
    {
        let doc = h.state_mut().doc_mut().unwrap();
        assert!(doc.doc.undo());
        assert!(doc.doc.sheet().canvas.objects.is_empty());
        assert!(doc.doc.redo());
        assert_eq!(doc.doc.sheet().canvas.objects, committed);
    }
    assert!(h.state_mut().save_active_to(&path, false));
    let reopened = crate::doc::OpenDoc::open(&path).unwrap();
    assert_eq!(reopened.doc.sheet().canvas.objects, committed);
}

#[test]
fn queued_synthetic_touch_buttons_do_not_block_a_real_touch_stroke() {
    let mut h = harness();
    let canvas = h.state().doc().unwrap().view.canvas_rect();
    let position = canvas.left_top() + vec2(canvas.width() * 0.3, canvas.height() * 0.65);
    h.hover_at(position);
    h.run_steps(1);
    assert_eq!(touch_pointer(&mut h, position, true, position), None);

    let button = |pressed| Event::PointerButton {
        pos: position,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    let touch = |phase, force| Event::Touch {
        device_id: TouchDeviceId(1),
        id: TouchId(1),
        phase,
        pos: position,
        force,
    };
    let touch_record = |phase| TouchForceRecord {
        device: TouchDeviceId(1),
        id: TouchId(1),
        phase,
        position,
        force_missing: false,
    };
    let start = [
        button(true),
        touch(TouchPhase::Start, Some(0.3)),
        button(false),
    ];
    let release = [TouchPointerRecord {
        phase: PointerPhase::Release(PointerButton::Primary),
        position,
        is_touch: true,
    }];
    assert_eq!(
        touch_input::publish_pointer_frame(
            &h.ctx,
            ViewportId::ROOT,
            &start,
            &release,
            false,
            &[touch_record(TouchPhase::Start)],
            false,
        ),
        None
    );
    h.input_mut().events.extend(start);
    h.run_steps(1);
    assert!(h.state().doc().unwrap().view.has_pending_ink_gesture());

    let end = [
        button(false),
        Event::PointerGone,
        touch(TouchPhase::End, Some(0.7)),
    ];
    assert_eq!(
        touch_input::publish_pointer_frame(
            &h.ctx,
            ViewportId::ROOT,
            &end,
            &[],
            false,
            &[touch_record(TouchPhase::End)],
            false,
        ),
        None
    );
    h.input_mut().events.extend(end);
    h.run_steps(2);
    let doc = h.state().doc().unwrap();
    assert!(
        !doc.has_unresolved_ink(),
        "feedback={:?}, pending={}, released={}, samples={:?}",
        doc.ink_feedback,
        doc.view.has_pending_ink_gesture(),
        doc.view.has_released_ink_gesture(),
        doc.view.pending_ink_samples()
    );
    assert_eq!(doc.doc.sheet().canvas.objects.len(), 1);
    let CanvasObjectKind::Ink(ink) = &doc.doc.sheet().canvas.objects[0].kind else {
        panic!("expected touch ink");
    };
    assert_eq!(&**ink.pressure.as_ref().unwrap(), &[0.3, 0.7]);
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    assert!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .canvas
            .objects
            .is_empty()
    );
}

#[test]
fn unmatched_touch_pointer_reports_feedback_before_allowing_mouse_input() {
    let scratch = Scratch::new();
    let path = scratch.path().join("unmatched-touch-pointer.thinoda");
    let mut h = harness();
    let canvas = h.state().doc().unwrap().view.canvas_rect();
    let position = canvas.left_top() + vec2(canvas.width() * 0.3, canvas.height() * 0.65);
    h.hover_at(position);
    h.run_steps(1);
    assert_eq!(
        touch_pointer(&mut h, position, true, position + vec2(1.0, 0.0)),
        Some(TouchPointerIssue::EventMismatch)
    );
    h.run_steps(1);
    let doc = h.state().doc().unwrap();
    assert!(doc.has_unresolved_ink());
    assert!(!doc.view.has_pending_ink_gesture());
    assert!(doc.doc.sheet().canvas.objects.is_empty());
    assert!(
        h.query_by_label(
            "Touch pointer input could not be matched. Check the gesture or dismiss this message."
        )
        .is_some()
    );
    assert!(!h.state_mut().save_active_to(&path, false));
    assert!(!path.exists());
    h.get_by_role_and_label(Role::Button, "Dismiss ink feedback")
        .click();
    h.run_steps(2);
    let mouse_start = start(&mut h);
    move_to(&mut h, mouse_start + vec2(40.0, 20.0));
    release(&mut h, mouse_start + vec2(80.0, 40.0));
    let CanvasObjectKind::Ink(ink) = &h.state().doc().unwrap().doc.sheet().canvas.objects[0].kind
    else {
        panic!("expected mouse ink");
    };
    assert!(ink.pressure.is_none());
}

#[test]
fn unmatched_first_pen_reports_the_lost_start_and_does_not_commit_mouse_ink() {
    let scratch = Scratch::new();
    let path = scratch.path().join("unmatched-pen.thinoda");
    let mut h = harness();
    let canvas = h.state().doc().unwrap().view.canvas_rect();
    let start_pos = canvas.left_top() + vec2(canvas.width() * 0.3, canvas.height() * 0.65);
    let end = start_pos + vec2(80.0, 40.0);
    h.hover_at(start_pos);
    h.run_steps(1);
    let events = [
        Event::PointerButton {
            pos: start_pos,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        Event::PointerMoved(end),
        Event::PointerButton {
            pos: end,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    ];
    let records = [PointerRecord {
        phase: PointerPhase::Press(PointerButton::Primary),
        position: start_pos + vec2(1.0, 0.0),
        pen: Some(PenSample {
            id: 7,
            pressure: 0.2,
        }),
    }];
    let mut matcher = PenInputMatcher::default();
    let matched = matcher.match_frame(&events, &records, None, false);
    assert_eq!(matched.issue, Some(PenInputIssue::PointerMismatch));
    pen_input::publish(&h.ctx, ViewportId::ROOT, &events, matched);
    for event in events {
        h.event(event);
    }
    h.run_steps(2);
    let doc = h.state().doc().unwrap();
    assert!(doc.has_unresolved_ink());
    assert!(doc.doc.sheet().canvas.objects.is_empty());
    assert!(!doc.view.has_pending_ink_gesture());
    assert!(
        h.query_by_label(
            "Pen input could not be matched. No stroke was started; dismiss this message and try again."
        )
        .is_some()
    );
    assert!(!h.state_mut().save_active_to(&path, false));
    assert!(!path.exists());
    h.get_by_role_and_label(Role::Button, "Dismiss ink feedback")
        .click();
    h.run_steps(2);
    assert!(!h.state().doc().unwrap().has_unresolved_ink());
    let mouse_start = start(&mut h);
    move_to(&mut h, mouse_start + vec2(40.0, 20.0));
    release(&mut h, mouse_start + vec2(80.0, 40.0));
    let CanvasObjectKind::Ink(ink) = &h.state().doc().unwrap().doc.sheet().canvas.objects[0].kind
    else {
        panic!("expected mouse ink");
    };
    assert!(ink.pressure.is_none());
}

#[test]
fn rejected_touch_samples_stay_visible_and_copyable_until_explicit_discard() {
    let mut h = harness();
    let canvas = h.state().doc().unwrap().view.canvas_rect();
    let p = canvas.left_top() + vec2(canvas.width() * 0.3, canvas.height() * 0.65);
    h.hover_at(p);
    h.run_steps(1);
    touch(&mut h, TouchPhase::Start, p, Some(0.2));
    touch(&mut h, TouchPhase::Move, p + vec2(15.0, 0.0), Some(1.5));
    touch(&mut h, TouchPhase::End, p + vec2(30.0, 0.0), Some(0.8));
    h.run_steps(2);
    let doc = h.state().doc().unwrap();
    assert!(doc.view.has_released_ink_gesture());
    assert!(doc.view.pending_ink_has_rejected_input());
    assert!(doc.is_dirty());
    assert!(
        h.get_by_role_and_label(Role::Button, "Retry ink")
            .accesskit_node()
            .is_disabled()
    );
    h.get_by_role_and_label(Role::Button, "Copy ink samples")
        .click();
    h.run_steps(1);
    let copied = h
        .output()
        .platform_output
        .commands
        .iter()
        .find_map(|command| match command {
            egui::OutputCommand::CopyText(text) => Some(text.as_str()),
            _ => None,
        })
        .unwrap();
    assert!(copied.contains("InvalidSample"));
    assert!(copied.contains("Input source: Some(Touch"));
    assert!(copied.contains("1.5"));
    assert!(copied.contains("0.2"));
    assert!(copied.contains("0.8"));
    let before = h.state().doc().unwrap().view.pending_ink_samples().to_vec();
    h.state_mut().approved_close = Some(CloseIntent::Quit);
    let commands = close_event(h.state_mut(), ViewportId::ROOT);
    assert!(commands.contains(&ViewportCommand::CancelClose));
    assert!(h.state().approved_close.is_none());
    assert_eq!(h.state().docs.len(), 1);
    assert_eq!(h.state().doc().unwrap().view.pending_ink_samples(), before);
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Discard ink").click();
    h.run_steps(2);
    assert!(!h.state().doc().unwrap().has_unresolved_ink());
    assert!(!h.state().doc().unwrap().is_dirty());
}

#[test]
fn rejected_first_sample_is_dirty_and_blocks_close_even_without_a_gesture() {
    let mut h = harness();
    h.state_mut().doc_mut().unwrap().view.ink_settings.width = -1.0;
    let canvas = h.state().doc().unwrap().view.canvas_rect();
    let p = canvas.left_top() + vec2(canvas.width() * 0.3, canvas.height() * 0.65);
    h.hover_at(p);
    h.run_steps(1);
    h.event(Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(2);
    assert!(!h.state().doc().unwrap().view.has_pending_ink_gesture());
    assert!(h.state().doc().unwrap().has_unresolved_ink());
    assert!(h.state().doc().unwrap().is_dirty());
    h.state_mut().request_close_tab(0);
    assert_eq!(h.state().docs.len(), 1);
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Dismiss ink feedback")
        .click();
    h.run_steps(2);
    assert!(!h.state().doc().unwrap().has_unresolved_ink());
    assert!(!h.state().doc().unwrap().is_dirty());
}

#[test]
fn root_and_child_close_and_stale_discard_dialog_cannot_drop_ink() {
    for child in [false, true] {
        let mut h = harness();
        let p = start(&mut h);
        move_to(&mut h, p + vec2(20.0, 0.0));
        let key = h.state().doc().unwrap().key.clone();
        let samples = h.state().doc().unwrap().view.pending_ink_samples().to_vec();
        let viewport = if child {
            let id = ViewportId::from_hash_of(("ink-close", &key));
            h.state_mut().document_windows.insert(key.clone(), id);
            id
        } else {
            ViewportId::ROOT
        };
        let commands = close_event(h.state_mut(), viewport);
        assert!(commands.contains(&ViewportCommand::CancelClose));
        assert_eq!(h.state().docs.len(), 1);
        assert_eq!(h.state().docs[0].view.pending_ink_samples(), samples);
        assert!(h.state().docs[0].view.has_released_ink_gesture());
        if child {
            assert_eq!(h.state().document_windows.get(&key), Some(&viewport));
        } else {
            h.state_mut().dialogs.close = Some(CloseIntent::Tab(key));
            h.run_steps(3);
            choose_close(&mut h, "Don't Save");
            assert_eq!(h.state().docs.len(), 1);
            assert_eq!(h.state().docs[0].view.pending_ink_samples(), samples);
        }
    }
}

#[test]
fn root_close_does_not_interrupt_another_viewports_unrouted_touch() {
    let mut h = harness();
    let canvas = h.state().doc().unwrap().view.canvas_rect();
    let p = canvas.left_top() + vec2(canvas.width() * 0.3, canvas.height() * 0.65);
    h.hover_at(p);
    h.run_steps(1);
    touch(&mut h, TouchPhase::Start, p, Some(0.2));
    let key = h.state().doc().unwrap().key.clone();
    let child = ViewportId::from_hash_of(("ink-close", &key));
    h.state_mut().document_windows.insert(key, child);
    let commands = close_event(h.state_mut(), ViewportId::ROOT);
    assert!(commands.contains(&ViewportCommand::CancelClose));
    assert!(!h.state().doc().unwrap().view.has_released_ink_gesture());

    let ctx = egui::Context::default();
    let mut input = egui::RawInput {
        viewport_id: child,
        ..Default::default()
    };
    input
        .viewports
        .entry(child)
        .or_default()
        .events
        .push(ViewportEvent::Close);
    input.events.push(Event::Touch {
        device_id: TouchDeviceId(1),
        id: TouchId(1),
        phase: TouchPhase::End,
        pos: p + vec2(20.0, 0.0),
        force: Some(0.8),
    });
    let mut output = ctx.run_ui(input, |ui| h.state_mut().handle_close_request(ui.ctx()));
    output.textures_delta.clear();
    assert!(
        output.viewport_output[&child]
            .commands
            .contains(&ViewportCommand::CancelClose)
    );
    let doc = h.state().doc().unwrap();
    assert!(doc.view.has_released_ink_gesture());
    assert_eq!(
        doc.view.pending_ink_samples().last().unwrap().pressure,
        Some(0.8)
    );
}

#[test]
fn stale_window_discard_choice_does_not_mark_a_pending_document_saved() {
    let mut h = harness();
    {
        let doc = h.state_mut().doc_mut().unwrap();
        let root = doc.doc.sheet().root;
        doc.doc
            .edit_sheet("Unsaved", |sheet| sheet.set_title(root, "Unsaved"))
            .unwrap();
    }
    let p = start(&mut h);
    move_to(&mut h, p + vec2(20.0, 0.0));
    let samples = h.state().doc().unwrap().view.pending_ink_samples().to_vec();
    h.state_mut().dialogs.close = Some(CloseIntent::Window);
    h.run_steps(3);
    choose_close(&mut h, "Don't Save");
    let doc = h.state().doc().unwrap();
    assert!(doc.doc.is_dirty());
    assert!(doc.is_dirty());
    assert_eq!(doc.view.pending_ink_samples(), samples);
    assert!(h.state().approved_close.is_none());
    assert_eq!(h.state().docs.len(), 1);
}

#[test]
fn modal_interruption_retains_owned_samples_from_the_same_frame() {
    let mut h = harness();
    let p = start(&mut h);
    let first = h.state().doc().unwrap().view.pending_ink_samples()[0];
    let zoom = h.state().doc().unwrap().view.zoom;
    h.state_mut().dialogs.close = Some(CloseIntent::Window);
    h.event(Event::PointerMoved(p + vec2(25.0, 12.0)));
    h.run_steps(1);
    let doc = h.state().doc().unwrap();
    assert!(doc.view.has_released_ink_gesture());
    assert_eq!(
        doc.view.pending_ink_samples().last().unwrap().position,
        first.position + Vec2::new(25.0 / zoom, 12.0 / zoom)
    );
}

#[test]
fn close_request_samples_owned_input_once_before_interrupting() {
    let mut h = harness();
    let p = start(&mut h);
    let before = h.state().doc().unwrap().view.pending_ink_samples().len();
    let end = p + vec2(20.0, 10.0);
    h.input_mut().events.extend([
        Event::PointerMoved(end),
        Event::PointerButton {
            pos: end,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    ]);
    h.input_mut()
        .viewports
        .entry(ViewportId::ROOT)
        .or_default()
        .events
        .push(ViewportEvent::Close);
    h.run_steps(1);
    let doc = h.state().doc().unwrap();
    assert!(doc.view.has_released_ink_gesture());
    assert_eq!(doc.view.pending_ink_samples().len(), before + 1);
    assert!(doc.doc.sheet().canvas.objects.is_empty());
    assert_eq!(h.state().docs.len(), 1);
}

#[test]
fn touch_released_while_modal_is_open_does_not_stick_pointer_ownership() {
    for released_in_interruption_frame in [true, false] {
        let mut h = harness();
        let canvas = h.state().doc().unwrap().view.canvas_rect();
        let p = canvas.left_top() + vec2(canvas.width() * 0.3, canvas.height() * 0.65);
        h.hover_at(p);
        h.run_steps(1);
        touch(&mut h, TouchPhase::Start, p, Some(0.2));
        h.state_mut().dialogs.close = Some(CloseIntent::Window);
        if !released_in_interruption_frame {
            h.run_steps(1);
        }
        let end = p + vec2(20.0, 0.0);
        h.input_mut().events.extend([
            Event::PointerMoved(end),
            Event::Touch {
                device_id: TouchDeviceId(1),
                id: TouchId(1),
                phase: TouchPhase::End,
                pos: end,
                force: Some(0.8),
            },
            Event::PointerButton {
                pos: end,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ]);
        h.run_steps(1);
        let doc = h.state().doc().unwrap();
        assert!(doc.view.has_released_ink_gesture());
        assert_eq!(
            doc.view.pending_ink_samples().last().unwrap().pressure,
            Some(if released_in_interruption_frame {
                0.8
            } else {
                0.2
            })
        );
        h.state_mut().dialogs.close = None;
        h.state_mut().doc_mut().unwrap().discard_pending_ink();
        h.run_steps(3);
        touch(&mut h, TouchPhase::Start, p, Some(0.4));
        assert!(h.state().doc().unwrap().view.has_pending_ink_gesture());
        touch(&mut h, TouchPhase::End, p + vec2(15.0, 0.0), Some(0.7));
        assert_eq!(h.state().doc().unwrap().doc.sheet().canvas.objects.len(), 1);
    }
}

#[test]
fn rejected_touch_sample_during_interruption_is_retained_for_copy() {
    let mut h = harness();
    let canvas = h.state().doc().unwrap().view.canvas_rect();
    let p = canvas.left_top() + vec2(canvas.width() * 0.3, canvas.height() * 0.65);
    h.hover_at(p);
    h.run_steps(1);
    touch(&mut h, TouchPhase::Start, p, Some(0.2));
    h.state_mut().dialogs.close = Some(CloseIntent::Window);
    h.input_mut().events.extend([
        Event::PointerGone,
        Event::Touch {
            device_id: TouchDeviceId(1),
            id: TouchId(1),
            phase: TouchPhase::End,
            pos: p + vec2(20.0, 0.0),
            force: Some(1.5),
        },
        Event::PointerButton {
            pos: p + vec2(20.0, 0.0),
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    ]);
    h.run_steps(1);
    assert!(
        h.state()
            .doc()
            .unwrap()
            .view
            .pending_ink_has_rejected_input()
    );
    h.state_mut().dialogs.close = None;
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Copy ink samples")
        .click();
    h.run_steps(1);
    let copied = h
        .output()
        .platform_output
        .commands
        .iter()
        .find_map(|command| match command {
            egui::OutputCommand::CopyText(text) => Some(text.as_str()),
            _ => None,
        })
        .unwrap();
    assert!(copied.contains("InvalidSample"));
    assert!(copied.contains("1.5"));
    assert!(copied.contains("0.2"));
}

#[test]
fn inactive_tab_tracks_touch_release_and_accepts_a_later_gesture() {
    let mut h = harness();
    let mut other = Workbook::default();
    let root = other.active().root;
    other
        .active_mut()
        .set_title(root, "Second document")
        .unwrap();
    h.state_mut().new_doc(other);
    h.state_mut().active = 0;
    h.run_steps(3);
    let canvas = h.state().doc().unwrap().view.canvas_rect();
    let p = canvas.left_top() + vec2(canvas.width() * 0.3, canvas.height() * 0.65);
    h.hover_at(p);
    h.run_steps(1);
    touch(&mut h, TouchPhase::Start, p, Some(0.2));
    h.state_mut().active = 1;
    h.run_steps(1);
    assert!(h.state().docs[0].view.has_released_ink_gesture());
    touch(&mut h, TouchPhase::End, p + vec2(10.0, 0.0), Some(0.8));
    h.state_mut().active = 0;
    h.state_mut().docs[0].discard_pending_ink();
    h.run_steps(3);
    touch(&mut h, TouchPhase::Start, p, Some(0.4));
    assert!(h.state().docs[0].view.has_pending_ink_gesture());
    touch(&mut h, TouchPhase::End, p + vec2(15.0, 0.0), Some(0.7));
    assert_eq!(h.state().docs[0].doc.sheet().canvas.objects.len(), 1);
}

#[test]
fn inactive_pending_ink_does_not_consume_active_tab_pointer_metadata() {
    let mut h = harness();
    h.state_mut().new_doc(Workbook::default());
    h.state_mut().active = 0;
    h.run_steps(3);
    let old_start = start(&mut h);
    h.state_mut().active = 1;
    h.run_steps(1);
    assert!(h.state().docs[0].view.has_released_ink_gesture());
    release(&mut h, old_start);
    h.state_mut().docs[1].view.set_board_tool(BoardTool::Pen);
    h.run_steps(2);
    let canvas = h.state().docs[1].view.canvas_rect();
    let start = canvas.left_top() + vec2(canvas.width() * 0.3, canvas.height() * 0.65);
    let end = start + vec2(50.0, 25.0);
    h.hover_at(start);
    h.run_steps(1);
    assert_eq!(touch_pointer(&mut h, start, true, start), None);
    assert!(!h.state().docs[1].view.has_pending_ink_gesture());
    h.event(Event::Touch {
        device_id: TouchDeviceId(1),
        id: TouchId(2),
        phase: TouchPhase::Start,
        pos: start,
        force: Some(0.3),
    });
    h.run_steps(1);
    assert!(h.state().docs[1].view.has_pending_ink_gesture());
    assert_eq!(touch_pointer(&mut h, end, false, end), None);
    h.event(Event::Touch {
        device_id: TouchDeviceId(1),
        id: TouchId(2),
        phase: TouchPhase::End,
        pos: end,
        force: Some(0.7),
    });
    h.run_steps(2);
    assert!(!h.state().docs[1].has_unresolved_ink());
    assert_eq!(h.state().docs[1].doc.sheet().canvas.objects.len(), 1);

    let mut matcher = PenInputMatcher::default();
    pen(
        &mut h,
        &mut matcher,
        PointerPhase::Press(PointerButton::Primary),
        start,
        0.2,
    );
    pen(
        &mut h,
        &mut matcher,
        PointerPhase::Release(PointerButton::Primary),
        end,
        0.8,
    );
    let second = &h.state().docs[1].doc.sheet().canvas.objects[1];
    let CanvasObjectKind::Ink(ink) = &second.kind else {
        panic!("expected pen ink");
    };
    assert_eq!(&**ink.pressure.as_ref().unwrap(), &[0.2, 0.8]);
    assert!(h.state().docs[0].view.has_released_ink_gesture());
    assert!(h.state().docs[0].doc.sheet().canvas.objects.is_empty());
}

#[test]
fn feedback_overflow_is_visible_copyable_and_requires_acknowledgement() {
    let mut h = harness();
    {
        let doc = h.state_mut().doc_mut().unwrap();
        for i in 0..MAX_INPUT_SAMPLES + 3 {
            doc.ink_feedback
                .record(InkControllerError::InvalidPathSample(InkSample {
                    position: Vec2::new(i as f32, 1.0),
                    pressure: Some(0.5),
                }));
        }
        assert!(doc.has_unresolved_ink());
        assert!(doc.is_dirty());
    }
    h.run_steps(2);
    assert!(
        h.query_by_label("Additional rejected input details could not be retained. (3)")
            .is_some()
    );
    h.get_by_role_and_label(Role::Button, "Copy ink samples")
        .click();
    h.run_steps(1);
    let copied = h
        .output()
        .platform_output
        .commands
        .iter()
        .find_map(|command| match command {
            egui::OutputCommand::CopyText(text) => Some(text.as_str()),
            _ => None,
        })
        .unwrap();
    assert!(copied.contains("Additional rejected input details could not be retained. (3)"));
    assert_eq!(
        copied.matches("InvalidPathSample").count(),
        MAX_INPUT_SAMPLES
    );
    h.get_by_role_and_label(Role::Button, "Dismiss ink feedback")
        .click();
    h.run_steps(2);
    assert!(!h.state().doc().unwrap().has_unresolved_ink());
    assert!(!h.state().doc().unwrap().is_dirty());
}

#[test]
fn save_all_focuses_unresolved_ink_in_another_tab() {
    let mut h = harness();
    {
        let app = h.state_mut();
        let mut other = Workbook::default();
        let root = other.active().root;
        other
            .active_mut()
            .set_title(root, "Second document")
            .unwrap();
        app.new_doc(other);
        app.active = 0;
        app.docs[0].view.set_board_tool(BoardTool::Pen);
    }
    h.run_steps(3);
    let p = start(&mut h);
    move_to(&mut h, p + vec2(20.0, 5.0));
    let samples = h.state().docs[0].view.pending_ink_samples().to_vec();
    h.get_by_role_and_label(Role::Button, "Second document")
        .click();
    h.run_steps(3);
    assert_eq!(h.state().active, 1);
    assert!(h.state().docs[0].view.has_released_ink_gesture());
    let after_switch = h.state().docs[0].view.pending_ink_samples().to_vec();
    assert!(after_switch.starts_with(&samples));
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::SaveAll);
    assert_eq!(h.state().active, 0);
    assert!(h.state().save_all.is_none());
    assert_eq!(h.state().docs[0].view.pending_ink_samples(), after_switch);
}

#[test]
fn keyboard_focused_recovery_buttons_retry_and_discard() {
    let mut h = harness();
    failed_drawing(&mut h);
    h.state_mut()
        .doc_mut()
        .unwrap()
        .view
        .ink_settings
        .finish
        .tolerance = 0.5;
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Retry ink").focus();
    h.run_steps(1);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert!(!h.state().doc().unwrap().has_unresolved_ink());
    assert_eq!(h.state().doc().unwrap().doc.sheet().canvas.objects.len(), 1);
    failed_drawing(&mut h);
    h.get_by_role_and_label(Role::Button, "Discard ink").focus();
    h.run_steps(1);
    h.key_press(egui::Key::Space);
    h.run_steps(3);
    assert!(!h.state().doc().unwrap().has_unresolved_ink());
    assert_eq!(h.state().doc().unwrap().doc.sheet().canvas.objects.len(), 1);
}

#[test]
fn successful_retry_does_not_erase_a_separate_rejected_start() {
    let mut h = harness();
    failed_drawing(&mut h);
    let canvas = h.state().doc().unwrap().view.canvas_rect();
    let p = canvas.left_top() + vec2(canvas.width() * 0.28, canvas.height() * 0.62);
    touch(&mut h, TouchPhase::Start, p, Some(0.7));
    touch(&mut h, TouchPhase::End, p + vec2(10.0, 0.0), Some(0.8));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.ink_settings.finish.tolerance = 0.5;
        doc.retry_pending_ink().unwrap();
        assert!(!doc.view.has_pending_ink_gesture());
        assert!(doc.has_unresolved_ink());
    }
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Copy ink samples")
        .click();
    h.run_steps(1);
    let copied = h
        .output()
        .platform_output
        .commands
        .iter()
        .find_map(|command| match command {
            egui::OutputCommand::CopyText(text) => Some(text.as_str()),
            _ => None,
        })
        .unwrap();
    assert!(copied.contains("StartWhilePending"));
    assert!(copied.contains("0.7"));
    h.get_by_role_and_label(Role::Button, "Dismiss ink feedback")
        .click();
    h.run_steps(2);
    assert!(!h.state().doc().unwrap().has_unresolved_ink());
}

#[test]
fn recovery_preserves_concurrent_edits_and_marks_stale_ink_review_only() {
    let scratch = Scratch::new();
    let recovery = RecoveryManager::start(&Paths::from_root(scratch.path())).unwrap();
    let mut h = harness();
    {
        let doc = h.state_mut().doc_mut().unwrap();
        let root = doc.doc.sheet().root;
        doc.doc
            .edit_sheet("First", |sheet| sheet.set_title(root, "First"))
            .unwrap();
        doc.snapshot(&recovery);
    }
    recovery.flush().unwrap();
    let key = h.state().doc().unwrap().key.clone();
    let snapshot = recovery
        .session_dir()
        .join("docs")
        .join(format!("{key}.json"));
    let first = std::fs::read(&snapshot).unwrap();
    let p = start(&mut h);
    move_to(&mut h, p + vec2(10.0, 5.0));
    let samples = h.state().doc().unwrap().view.pending_ink_samples().to_vec();
    {
        let doc = h.state_mut().doc_mut().unwrap();
        let root = doc.doc.sheet().root;
        doc.doc
            .edit_sheet("Concurrent edit", |sheet| sheet.set_title(root, "Second"))
            .unwrap();
        doc.snapshot(&recovery);
        assert_eq!(doc.view.pending_ink_samples(), samples);
    }
    recovery.flush().unwrap();
    let latest = std::fs::read(&snapshot).unwrap();
    assert!(latest != first);
    let recovered: Snapshot = serde_json::from_slice(&latest).unwrap();
    assert_eq!(recovered.workbook.active().root_topic().title, "Second");
    let payload: serde_json::Value =
        serde_json::from_str(recovered.pending_ink.as_deref().unwrap()).unwrap();
    assert_eq!(payload["gesture"]["source_current"], false);
    assert_eq!(
        payload["gesture"]["samples"].as_array().unwrap().len(),
        samples.len()
    );
    h.state_mut().doc_mut().unwrap().discard_pending_ink();
    h.state_mut().doc_mut().unwrap().snapshot(&recovery);
    recovery.flush().unwrap();
    let resolved: Snapshot = serde_json::from_slice(&std::fs::read(&snapshot).unwrap()).unwrap();
    assert_eq!(resolved.workbook.active().root_topic().title, "Second");
    assert!(resolved.pending_ink.is_none());
    recovery.finish().unwrap();
}

#[test]
fn held_touch_ink_survives_restart_and_can_be_retried_with_undo() {
    let scratch = Scratch::new();
    let paths = Paths::from_root(scratch.path());
    let recovery = RecoveryManager::start(&paths).unwrap();
    let mut original = harness();
    let canvas = original.state().doc().unwrap().view.canvas_rect();
    let point = canvas.left_top() + vec2(canvas.width() * 0.3, canvas.height() * 0.65);
    original.hover_at(point);
    original.run_steps(1);
    touch(&mut original, TouchPhase::Start, point, Some(0.2));
    touch(
        &mut original,
        TouchPhase::Move,
        point + vec2(30.0, 10.0),
        Some(0.8),
    );
    let doc = original.state_mut().doc_mut().unwrap();
    let samples = doc.view.pending_ink_samples().to_vec();
    let settings = doc.view.pending_ink_settings().unwrap();
    doc.snapshot(&recovery);
    recovery.flush().unwrap();
    let source_path = recovery
        .session_dir()
        .join("docs")
        .join(format!("{}.json", doc.key));
    let source: Snapshot = serde_json::from_slice(&std::fs::read(&source_path).unwrap()).unwrap();
    let original_payload = source.pending_ink.unwrap();
    let (app, target) = restore_after_crash(recovery, &paths);
    assert!(!source_path.exists());
    let restored = app.doc().unwrap();
    assert!(restored.is_dirty());
    assert!(restored.view.has_released_ink_gesture());
    assert!(restored.view.pending_ink_was_recovered());
    assert_eq!(restored.view.pending_ink_settings(), Some(settings));
    assert_eq!(restored.view.pending_ink_samples(), samples);
    assert!(restored.doc.sheet().canvas.objects.is_empty());
    let target_path = target
        .session_dir()
        .join("docs")
        .join(format!("{}.json", restored.key));
    let handoff: Snapshot = serde_json::from_slice(&std::fs::read(target_path).unwrap()).unwrap();
    assert_eq!(
        handoff.pending_ink.as_deref(),
        Some(original_payload.as_str())
    );

    let mut h = restored_harness(app);
    assert!(
        h.query_by_label("Recovered ink may be incomplete. Review it before retrying.")
            .is_some()
    );
    let retry = h.get_by_role_and_label(Role::Button, "Retry ink");
    assert!(!retry.accesskit_node().is_disabled());
    retry.click();
    h.run_steps(2);
    let doc = h.state_mut().doc_mut().unwrap();
    assert!(!doc.has_unresolved_ink());
    let CanvasObjectKind::Ink(stroke) = &doc.doc.sheet().canvas.objects[0].kind else {
        panic!("expected recovered ink")
    };
    assert_eq!(stroke.pressure.as_deref(), Some(&[0.2, 0.8][..]));
    assert!(doc.doc.undo());
    assert!(doc.doc.sheet().canvas.objects.is_empty());
    assert!(doc.doc.redo());
    assert_eq!(doc.doc.sheet().canvas.objects.len(), 1);
    h.state_mut().services.recovery = None;
    target.finish().unwrap();
}

#[test]
fn rejected_touch_input_remains_copyable_after_restart_and_discard() {
    let scratch = Scratch::new();
    let paths = Paths::from_root(scratch.path());
    let recovery = RecoveryManager::start(&paths).unwrap();
    let mut original = harness();
    let canvas = original.state().doc().unwrap().view.canvas_rect();
    let point = canvas.left_top() + vec2(canvas.width() * 0.3, canvas.height() * 0.65);
    original.hover_at(point);
    original.run_steps(1);
    touch(&mut original, TouchPhase::Start, point, Some(0.2));
    touch(
        &mut original,
        TouchPhase::Move,
        point + vec2(15.0, 0.0),
        Some(1.5),
    );
    touch(
        &mut original,
        TouchPhase::End,
        point + vec2(30.0, 0.0),
        Some(0.8),
    );
    let doc = original.state_mut().doc_mut().unwrap();
    assert!(doc.view.pending_ink_has_rejected_input());
    let samples = doc.view.pending_ink_samples().to_vec();
    doc.snapshot(&recovery);
    recovery.flush().unwrap();
    let (app, target) = restore_after_crash(recovery, &paths);
    assert_eq!(app.doc().unwrap().view.pending_ink_samples(), samples);
    let mut h = restored_harness(app);
    assert!(
        h.get_by_role_and_label(Role::Button, "Retry ink")
            .accesskit_node()
            .is_disabled()
    );
    h.get_by_role_and_label(Role::Button, "Copy ink samples")
        .click();
    h.run_steps(1);
    let copied = h
        .output()
        .platform_output
        .commands
        .iter()
        .find_map(|command| match command {
            egui::OutputCommand::CopyText(text) => Some(text.as_str()),
            _ => None,
        })
        .unwrap();
    assert!(copied.contains("InvalidSample"));
    assert!(copied.contains("1.5"));
    assert!(copied.contains("0.2"));
    assert!(copied.contains("0.8"));
    h.get_by_role_and_label(Role::Button, "Discard ink").click();
    h.run_steps(2);
    let doc = h.state_mut().doc_mut().unwrap();
    assert!(!doc.has_unresolved_ink());
    doc.snapshot(&target);
    target.flush().unwrap();
    let path = target
        .session_dir()
        .join("docs")
        .join(format!("{}.json", doc.key));
    let snapshot: Snapshot = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert!(snapshot.pending_ink.is_none());
    h.state_mut().services.recovery = None;
    target.finish().unwrap();
}

#[test]
fn standalone_nonfinite_ink_feedback_survives_restart() {
    let scratch = Scratch::new();
    let paths = Paths::from_root(scratch.path());
    let recovery = RecoveryManager::start(&paths).unwrap();
    let mut original = harness();
    let doc = original.state_mut().doc_mut().unwrap();
    doc.ink_feedback
        .record(InkControllerError::InvalidSettings {
            reason: "invalid input".into(),
            sample: InkSample::new(Vec2::new(f32::NAN, f32::INFINITY), Some(f32::NEG_INFINITY)),
        });
    doc.snapshot(&recovery);
    recovery.flush().unwrap();
    let (app, target) = restore_after_crash(recovery, &paths);
    assert!(app.doc().unwrap().has_unresolved_ink());
    assert!(!app.doc().unwrap().view.has_pending_ink_gesture());
    let mut h = restored_harness(app);
    h.get_by_role_and_label(Role::Button, "Copy ink samples")
        .click();
    h.run_steps(1);
    let copied = h
        .output()
        .platform_output
        .commands
        .iter()
        .find_map(|command| match command {
            egui::OutputCommand::CopyText(text) => Some(text.as_str()),
            _ => None,
        })
        .unwrap();
    assert!(copied.contains("InvalidSettings"));
    assert!(copied.contains("NaN"));
    assert!(copied.contains("inf"));
    h.get_by_role_and_label(Role::Button, "Dismiss ink feedback")
        .click();
    h.run_steps(2);
    assert!(!h.state().doc().unwrap().has_unresolved_ink());
    h.state_mut().services.recovery = None;
    target.finish().unwrap();
}

#[test]
fn ongoing_ink_recovery_is_throttled_but_does_not_wait_for_idle() {
    let scratch = Scratch::new();
    let recovery = RecoveryManager::start(&Paths::from_root(scratch.path())).unwrap();
    let mut h = harness();
    let point = start(&mut h);
    {
        let doc = h.state_mut().doc_mut().unwrap();
        assert!(doc.recovery_due(1.0));
        doc.snapshot(&recovery);
        assert!(!doc.recovery_due(1.0));
    }
    recovery.flush().unwrap();
    let path = recovery
        .session_dir()
        .join("docs")
        .join(format!("{}.json", h.state().doc().unwrap().key));
    h.state_mut().doc_mut().unwrap().snapshot(&recovery);
    assert!(!recovery.has_unflushed_snapshots());

    move_to(&mut h, point + vec2(20.0, 10.0));
    let doc = h.state_mut().doc_mut().unwrap();
    let root = doc.doc.sheet().root;
    doc.doc
        .edit_sheet("Concurrent", |sheet| sheet.set_title(root, "Still active"))
        .unwrap();
    doc.tick();
    assert!(doc.idle_secs() < 1.0);
    doc.backdate_ink_recovery_attempt(2);
    assert!(doc.recovery_due(1.0));
    doc.snapshot(&recovery);
    recovery.flush().unwrap();
    let snapshot: Snapshot = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        snapshot.workbook.active().root_topic().title,
        "Still active"
    );
    let payload: serde_json::Value =
        serde_json::from_str(snapshot.pending_ink.as_deref().unwrap()).unwrap();
    assert_eq!(payload["gesture"]["samples"].as_array().unwrap().len(), 2);

    doc.view.ink_settings.finish.tolerance = 1.25;
    doc.snapshot(&recovery);
    recovery.flush().unwrap();
    let snapshot: Snapshot = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let payload: serde_json::Value =
        serde_json::from_str(snapshot.pending_ink.as_deref().unwrap()).unwrap();
    assert_eq!(payload["retry_tolerance_bits"], 1.25_f32.to_bits());
    doc.discard_pending_ink();
    recovery.finish().unwrap();
}

#[test]
fn ordinary_recovery_keeps_its_existing_idle_cap() {
    let mut h = harness();
    let doc = h.state_mut().doc_mut().unwrap();
    doc.backdate_change(3);
    assert!(doc.recovery_due(2.0));
    assert!(doc.recovery_due(30.0));
}

#[test]
fn resolved_ink_recovery_retries_cleanup_without_waiting_for_ordinary_idle() {
    let scratch = Scratch::new();
    let recovery = RecoveryManager::start(&Paths::from_root(scratch.path())).unwrap();
    let mut h = harness();
    start(&mut h);
    let doc = h.state_mut().doc_mut().unwrap();
    doc.snapshot(&recovery);
    recovery.flush().unwrap();
    let path = recovery
        .session_dir()
        .join("docs")
        .join(format!("{}.json", doc.key));
    assert!(path.exists());

    recovery.suspend().unwrap();
    doc.discard_pending_ink();
    doc.backdate_ink_recovery_attempt(3);
    assert!(doc.recovery_due(30.0));
    doc.snapshot(&recovery);
    assert!(doc.recovery_cleanup_failed);
    assert!(path.exists());
    assert!(!doc.recovery_due(30.0));

    recovery.resume().unwrap();
    doc.backdate_ink_recovery_attempt(3);
    assert!(doc.recovery_due(30.0));
    doc.snapshot(&recovery);
    assert!(!doc.recovery_cleanup_failed);
    assert!(!path.exists());
    recovery.finish().unwrap();
}

#[test]
fn stale_recovered_ink_stays_copyable_and_retry_is_disabled() {
    let scratch = Scratch::new();
    let paths = Paths::from_root(scratch.path());
    let recovery = RecoveryManager::start(&paths).unwrap();
    let mut original = harness();
    let point = start(&mut original);
    move_to(&mut original, point + vec2(10.0, 5.0));
    let doc = original.state_mut().doc_mut().unwrap();
    let samples = doc.view.pending_ink_samples().to_vec();
    let root = doc.doc.sheet().root;
    doc.doc
        .edit_sheet("Concurrent", |sheet| sheet.set_title(root, "Changed"))
        .unwrap();
    doc.snapshot(&recovery);
    recovery.flush().unwrap();
    let (app, target) = restore_after_crash(recovery, &paths);
    let doc = app.doc().unwrap();
    assert_eq!(doc.doc.sheet().root_topic().title, "Changed");
    assert_eq!(doc.view.pending_ink_samples(), samples);
    let mut h = restored_harness(app);
    assert!(
        h.query_by_label(
            "The document changed during the gesture. Copy its samples or discard it."
        )
        .is_some()
    );
    assert!(
        h.get_by_role_and_label(Role::Button, "Retry ink")
            .accesskit_node()
            .is_disabled()
    );
    assert!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .canvas
            .objects
            .is_empty()
    );
    h.state_mut().services.recovery = None;
    target.finish().unwrap();
}

#[test]
fn recovered_eraser_and_lasso_are_review_only() {
    for tool in [
        BoardTool::ObjectEraser,
        BoardTool::PixelEraser,
        BoardTool::Lasso,
    ] {
        let scratch = Scratch::new();
        let paths = Paths::from_root(scratch.path());
        let recovery = RecoveryManager::start(&paths).unwrap();
        let mut original = harness();
        original
            .state_mut()
            .doc_mut()
            .unwrap()
            .view
            .set_board_tool(tool);
        original.run_steps(1);
        let point = start(&mut original);
        move_to(&mut original, point + vec2(15.0, 5.0));
        original.state_mut().doc_mut().unwrap().snapshot(&recovery);
        recovery.flush().unwrap();
        let (app, target) = restore_after_crash(recovery, &paths);
        let mut h = restored_harness(app);
        assert!(
            h.query_by_label(
                "Recovered eraser or lasso input cannot be retried. Copy its samples or discard it."
            )
            .is_some()
        );
        assert!(
            h.get_by_role_and_label(Role::Button, "Retry ink")
                .accesskit_node()
                .is_disabled()
        );
        assert!(
            h.state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .canvas
                .objects
                .is_empty()
        );
        h.state_mut().services.recovery = None;
        target.finish().unwrap();
    }
}

#[test]
fn failed_ink_recovery_flush_keeps_previous_snapshot_and_retries() {
    let scratch = Scratch::new();
    let paths = Paths::from_root(scratch.path());
    let recovery = RecoveryManager::start(&paths).unwrap();
    let mut h = harness();
    let doc = h.state_mut().doc_mut().unwrap();
    let root = doc.doc.sheet().root;
    doc.doc
        .edit_sheet("Before", |sheet| sheet.set_title(root, "Before"))
        .unwrap();
    doc.snapshot(&recovery);
    recovery.flush().unwrap();
    let path = recovery
        .session_dir()
        .join("docs")
        .join(format!("{}.json", doc.key));
    let before = std::fs::read(&path).unwrap();
    recovery.suspend().unwrap();
    let point = start(&mut h);
    move_to(&mut h, point + vec2(15.0, 5.0));
    h.state_mut().doc_mut().unwrap().snapshot(&recovery);
    assert!(recovery.flush().is_err());
    assert!(recovery.last_flush_failed());
    assert!(recovery.has_unflushed_snapshots());
    assert!(std::fs::read(&path).unwrap() == before);
    assert!(h.state().doc().unwrap().view.has_pending_ink_gesture());
    recovery.resume().unwrap();
    recovery.flush().unwrap();
    assert!(!recovery.last_flush_failed());
    let snapshot: Snapshot = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert!(snapshot.pending_ink.is_some());
    h.state_mut().doc_mut().unwrap().discard_pending_ink();
    recovery.finish().unwrap();
}

#[test]
fn resolving_ink_cleans_a_now_saved_recovery_snapshot() {
    let scratch = Scratch::new();
    let recovery = RecoveryManager::start(&Paths::from_root(scratch.path())).unwrap();
    let mut h = harness();
    {
        let doc = h.state_mut().doc_mut().unwrap();
        let root = doc.doc.sheet().root;
        doc.doc
            .edit_sheet("Before save", |sheet| sheet.set_title(root, "Before save"))
            .unwrap();
        doc.snapshot(&recovery);
    }
    recovery.flush().unwrap();
    let key = h.state().doc().unwrap().key.clone();
    let snapshot = recovery
        .session_dir()
        .join("docs")
        .join(format!("{key}.json"));
    assert!(snapshot.exists());
    let p = start(&mut h);
    move_to(&mut h, p + vec2(12.0, 0.0));
    let doc = h.state_mut().doc_mut().unwrap();
    doc.snapshot(&recovery);
    doc.doc.mark_saved();
    doc.discard_pending_ink();
    doc.snapshot(&recovery);
    recovery.flush().unwrap();
    assert!(!snapshot.exists());
    recovery.finish().unwrap();
}

#[test]
fn background_save_cannot_close_a_tab_after_new_ink_arrives() {
    let scratch = Scratch::new();
    let path = scratch.path().join("background.thinoda");
    let mut h = harness();
    {
        let app = h.state_mut();
        app.doc_mut().unwrap().save_to(&path, false).unwrap();
        let doc = app.doc_mut().unwrap();
        let root = doc.doc.sheet().root;
        doc.doc
            .edit_sheet("Before save", |sheet| sheet.set_title(root, "Before save"))
            .unwrap();
        app.set_file_work_mode(FileWorkMode::Background);
        app.pause_file_workers(true);
        app.request_close_tab(0);
    }
    h.run_steps(3);
    choose_close(&mut h, "Save");
    assert!(h.state().file_ops.saving());
    let p = start(&mut h);
    move_to(&mut h, p + vec2(15.0, 10.0));
    let samples = h.state().doc().unwrap().view.pending_ink_samples().to_vec();
    h.state().pause_file_workers(false);
    let deadline = Instant::now() + Duration::from_secs(20);
    while h.state().file_ops.saving() {
        assert!(Instant::now() < deadline, "background save did not finish");
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(h.state().docs.len(), 1);
    assert!(h.state().doc().unwrap().is_dirty());
    assert_eq!(h.state().doc().unwrap().view.pending_ink_samples(), samples);
    assert!(h.state().dialogs.save_close_tab.is_none());
    assert!(h.state().toasts.texts().any(|text| {
        text == crate::i18n::t(
            "Pending ink is not in the saved file. Crash recovery may miss recent samples.",
        )
    }));
    assert!(path.exists());
}

#[test]
fn failed_retry_records_a_structured_error_without_discarding_samples() {
    let mut h = harness();
    failed_drawing(&mut h);
    let doc = h.state_mut().doc_mut().unwrap();
    let before = doc.view.pending_ink_samples().to_vec();
    doc.view.ink_settings.finish.tolerance = f32::NAN;
    assert!(matches!(
        doc.retry_pending_ink(),
        Err(InkControllerError::Ink(_))
    ));
    assert_eq!(doc.view.pending_ink_samples(), before);
    assert!(doc.has_unresolved_ink());
}
