use super::*;
use crate::ink_controller::{InkControllerError, InkOutcome};
use crate::ink_input::{InkInputError, StrokeFinishOptions};
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_core::{
    CanvasObject, CanvasObjectKind, Card, CardKind, Color, InkStroke, InkTool, SheetRootMode,
    Workbook, board::BoardMember,
};

struct State {
    doc: Document,
    view: MapView,
    ink_errors: Vec<InkControllerError>,
    outcomes: Vec<InkOutcome>,
}

fn board() -> Sheet {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    sheet
}

fn harness(sheet: Sheet) -> Harness<'static, State> {
    let view = MapView {
        show_add_controls: false,
        ..Default::default()
    };
    let mut harness = Harness::builder()
        .with_size(vec2(1200.0, 900.0))
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(
            |ui, state: &mut State| {
                let out = MapCanvas::show(ui, &mut state.doc, &mut state.view);
                state.ink_errors.extend(out.ink_errors);
                state.outcomes.extend(out.ink_outcomes);
            },
            State {
                doc: Document::new(Workbook::new(sheet)),
                view,
                ink_errors: Vec::new(),
                outcomes: Vec::new(),
            },
        );
    harness.run_steps(3);
    harness.state_mut().view.set_zoom(1.0);
    harness.run_steps(2);
    harness
}

fn point(h: &Harness<'_, State>, p: Vec2) -> Pos2 {
    h.state().view.xf().pos(p)
}

fn tool(h: &mut Harness<'_, State>, tool: BoardTool) {
    h.state_mut().view.set_board_tool(tool);
    h.run_steps(2);
}

fn press(h: &mut Harness<'_, State>, p: Pos2, modifiers: Modifiers) {
    h.hover_at(p);
    h.run_steps(1);
    h.event(egui::Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed: true,
        modifiers,
    });
    h.run_steps(1);
}

fn move_to(h: &mut Harness<'_, State>, p: Pos2) {
    h.event(egui::Event::PointerMoved(p));
    h.run_steps(1);
}

fn release(h: &mut Harness<'_, State>, p: Pos2, modifiers: Modifiers) {
    h.event(egui::Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed: false,
        modifiers,
    });
    h.run_steps(2);
}

fn touch(
    h: &mut Harness<'_, State>,
    phase: egui::TouchPhase,
    position: Pos2,
    pressure: Option<f32>,
) {
    h.event(egui::Event::PointerMoved(position));
    h.event(egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(1),
        phase,
        pos: position,
        force: pressure,
    });
    if matches!(phase, egui::TouchPhase::Start | egui::TouchPhase::End) {
        h.event(egui::Event::PointerButton {
            pos: position,
            button: PointerButton::Primary,
            pressed: phase == egui::TouchPhase::Start,
            modifiers: Modifiers::NONE,
        });
    }
    h.run_steps(1);
}

fn stroke(sheet: &mut Sheet, start: Vec2, end: Vec2) -> CanvasObjectId {
    sheet
        .add_canvas_object(CanvasObject::ink(InkStroke::new(
            InkTool::Pen,
            Color::BLACK,
            2.0,
            vec![start, end],
        )))
        .unwrap()
}

#[test]
fn pen_preview_commits_one_undoable_stroke_without_moving_the_canvas() {
    let mut h = harness(board());
    tool(&mut h, BoardTool::Pen);
    let start = point(&h, Vec2::new(-100.0, -100.0));
    let end = start + vec2(120.0, 40.0);
    let offset = h.state().view.offset;
    press(&mut h, start, Modifiers::NONE);
    move_to(&mut h, start + vec2(60.0, 20.0));
    assert!(h.state().doc.sheet().canvas.objects.is_empty());
    assert!(h.state().view.has_pending_ink_gesture());
    assert!(h.state().view.pending_ink_samples().len() >= 2);
    assert_eq!(h.state().view.offset, offset);
    move_to(&mut h, end);
    release(&mut h, end, Modifiers::NONE);
    assert!(
        h.state().ink_errors.is_empty(),
        "{:?}",
        h.state().ink_errors
    );
    assert_eq!(h.state().doc.sheet().canvas.objects.len(), 1);
    assert!(matches!(
        h.state().outcomes.last(),
        Some(InkOutcome::Drawn(_))
    ));
    let committed = h.state().doc.sheet().canvas.objects.clone();
    assert!(!h.state().view.has_pending_ink_gesture());
    assert_eq!(h.state().view.offset, offset);
    assert!(h.state_mut().doc.undo());
    assert!(h.state().doc.sheet().canvas.objects.is_empty());
    assert!(!h.state().doc.can_undo());
    assert!(h.state_mut().doc.redo());
    assert_eq!(h.state().doc.sheet().canvas.objects, committed);
}

#[test]
fn ink_settings_are_captured_per_gesture_and_survive_undo() {
    let mut h = harness(board());
    tool(&mut h, BoardTool::Pen);
    {
        let settings = &mut h.state_mut().view.ink_settings;
        settings.color = Color::rgb(200, 30, 40);
        settings.width = 4.0;
        settings.opacity = 0.4;
    }
    let start = point(&h, Vec2::new(-100.0, -100.0));
    press(&mut h, start, Modifiers::NONE);
    move_to(&mut h, start + vec2(30.0, 0.0));
    {
        let settings = &mut h.state_mut().view.ink_settings;
        settings.color = Color::rgb(20, 100, 220);
        settings.width = 8.0;
        settings.opacity = 0.8;
    }
    let pending = h.state().view.pending_ink_settings().unwrap();
    assert_eq!(pending.color, Color::rgb(200, 30, 40));
    assert_eq!(pending.width, 4.0);
    assert_eq!(pending.opacity, 0.4);
    release(&mut h, start + vec2(60.0, 0.0), Modifiers::NONE);

    let next = start + vec2(0.0, 60.0);
    press(&mut h, next, Modifiers::NONE);
    move_to(&mut h, next + vec2(30.0, 0.0));
    release(&mut h, next + vec2(60.0, 0.0), Modifiers::NONE);
    assert!(
        h.state().ink_errors.is_empty(),
        "{:?}",
        h.state().ink_errors
    );
    let objects = &h.state().doc.sheet().canvas.objects;
    assert_eq!(objects.len(), 2);
    for (object, color, width, opacity) in [
        (&objects[0], Color::rgb(200, 30, 40), 4.0, 0.4),
        (&objects[1], Color::rgb(20, 100, 220), 8.0, 0.8),
    ] {
        let CanvasObjectKind::Ink(ink) = &object.kind else {
            panic!("expected an ink stroke");
        };
        assert_eq!(ink.color, color);
        assert_eq!(ink.width, width);
        assert_eq!(object.style.opacity, opacity);
    }
    let committed = objects.clone();
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.sheet().canvas.objects.len(), 1);
    assert!(h.state_mut().doc.undo());
    assert!(h.state().doc.sheet().canvas.objects.is_empty());
    assert!(h.state_mut().doc.redo());
    assert!(h.state_mut().doc.redo());
    assert_eq!(h.state().doc.sheet().canvas.objects, committed);
}

#[test]
fn space_does_not_steal_an_in_progress_stroke() {
    let mut h = harness(board());
    tool(&mut h, BoardTool::Pen);
    let start = point(&h, Vec2::new(-100.0, -100.0));
    let offset = h.state().view.offset;
    press(&mut h, start, Modifiers::NONE);
    move_to(&mut h, start + vec2(30.0, 0.0));
    h.event(egui::Event::Key {
        key: Key::Space,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(1);
    move_to(&mut h, start + vec2(60.0, 0.0));
    release(&mut h, start + vec2(90.0, 0.0), Modifiers::NONE);
    assert_eq!(h.state().doc.sheet().canvas.objects.len(), 1);
    assert_eq!(h.state().view.offset, offset);
    assert!(matches!(
        h.state().outcomes.last(),
        Some(InkOutcome::Drawn(_))
    ));
    assert!(
        h.state().ink_errors.is_empty(),
        "{:?}",
        h.state().ink_errors
    );
}

#[test]
fn navigation_drags_remain_available_with_an_ink_tool_selected() {
    for (button, use_space) in [
        (PointerButton::Primary, true),
        (PointerButton::Middle, false),
        (PointerButton::Secondary, false),
    ] {
        let mut h = harness(board());
        tool(&mut h, BoardTool::Pen);
        let start = point(&h, Vec2::new(-100.0, -100.0));
        let initial_offset = h.state().view.offset;
        if use_space {
            h.key_down(Key::Space);
            h.run_steps(1);
        }
        h.hover_at(start);
        h.run_steps(1);
        h.event(egui::Event::PointerButton {
            pos: start,
            button,
            pressed: true,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(1);
        move_to(&mut h, start + vec2(70.0, 40.0));
        move_to(&mut h, start + vec2(100.0, 60.0));
        h.event(egui::Event::PointerButton {
            pos: start + vec2(100.0, 60.0),
            button,
            pressed: false,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(1);
        if use_space {
            h.key_up(Key::Space);
            h.run_steps(1);
        }
        assert_ne!(h.state().view.offset, initial_offset, "{button:?}");
        assert!(
            h.state().doc.sheet().canvas.objects.is_empty(),
            "{button:?}"
        );
        assert!(!h.state().view.has_pending_ink_gesture(), "{button:?}");
        assert!(
            h.state().ink_errors.is_empty(),
            "{button:?}: {:?}",
            h.state().ink_errors
        );
    }
}

#[test]
fn touch_highlighter_keeps_pressure_and_ignores_synthetic_mouse() {
    let mut h = harness(board());
    h.state_mut().view.ink_settings.finish.tolerance = 0.0;
    tool(&mut h, BoardTool::Highlighter);
    let start = point(&h, Vec2::new(-150.0, -100.0));
    touch(&mut h, egui::TouchPhase::Start, start, Some(0.2));
    touch(
        &mut h,
        egui::TouchPhase::Move,
        start + vec2(40.0, 20.0),
        Some(0.6),
    );
    touch(
        &mut h,
        egui::TouchPhase::End,
        start + vec2(80.0, 40.0),
        Some(0.9),
    );
    assert!(
        h.state().ink_errors.is_empty(),
        "{:?}",
        h.state().ink_errors
    );
    assert_eq!(h.state().doc.sheet().canvas.objects.len(), 1);
    let CanvasObjectKind::Ink(ink) = &h.state().doc.sheet().canvas.objects[0].kind else {
        panic!("expected highlighter")
    };
    assert_eq!(ink.tool, InkTool::Highlighter);
    assert_eq!(&**ink.pressure.as_ref().unwrap(), &[0.2, 0.6, 0.9]);
    assert_eq!(ink.points.len(), 3);
    assert!(h.state_mut().doc.undo());
    assert!(!h.state().doc.can_undo());
}

#[test]
fn new_touch_can_draw_after_canceling_an_interrupted_stroke() {
    let mut h = harness(board());
    tool(&mut h, BoardTool::Pen);
    let start = point(&h, Vec2::new(-100.0, -100.0));
    touch(&mut h, egui::TouchPhase::Start, start, Some(0.5));
    touch(
        &mut h,
        egui::TouchPhase::Move,
        start + vec2(30.0, 10.0),
        Some(0.5),
    );
    touch(
        &mut h,
        egui::TouchPhase::Cancel,
        start + vec2(30.0, 10.0),
        None,
    );
    assert!(matches!(
        h.state().ink_errors.last(),
        Some(InkControllerError::Interrupted(
            crate::ink_controller::InkCancelReason::TouchCanceled,
        ))
    ));
    assert!(h.state().view.has_released_ink_gesture());
    assert!(h.state_mut().view.cancel_ink());

    touch(&mut h, egui::TouchPhase::Start, start, Some(0.5));
    touch(
        &mut h,
        egui::TouchPhase::Move,
        start + vec2(40.0, 20.0),
        Some(0.5),
    );
    touch(
        &mut h,
        egui::TouchPhase::End,
        start + vec2(80.0, 40.0),
        Some(0.5),
    );
    assert_eq!(h.state().doc.sheet().canvas.objects.len(), 1);
    assert_eq!(h.state().ink_errors.len(), 1);
    assert!(h.state_mut().doc.undo());
    assert!(!h.state().doc.can_undo());
}

#[test]
fn batched_touch_restart_retains_the_interrupted_stroke() {
    let mut h = harness(board());
    tool(&mut h, BoardTool::Pen);
    let start = point(&h, Vec2::new(-100.0, -100.0));
    let end = start + vec2(60.0, 40.0);
    touch(&mut h, egui::TouchPhase::Start, start, Some(0.5));
    touch(&mut h, egui::TouchPhase::Move, end, Some(0.5));
    h.input_mut().events.extend([
        egui::Event::Touch {
            device_id: egui::TouchDeviceId(1),
            id: egui::TouchId(1),
            phase: egui::TouchPhase::Cancel,
            pos: end,
            force: Some(0.5),
        },
        egui::Event::PointerButton {
            pos: start,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        egui::Event::Touch {
            device_id: egui::TouchDeviceId(1),
            id: egui::TouchId(2),
            phase: egui::TouchPhase::Start,
            pos: start,
            force: Some(0.5),
        },
    ]);
    h.run_steps(2);
    assert!(h.state().view.has_released_ink_gesture());
    assert!(h.state().view.pending_ink_samples().len() >= 2);
    assert!(h.state().doc.sheet().canvas.objects.is_empty());
    assert!(h.state().ink_errors.iter().any(|error| matches!(
        error,
        InkControllerError::Interrupted(crate::ink_controller::InkCancelReason::TouchCanceled)
    )));
    assert!(
        h.state()
            .ink_errors
            .iter()
            .any(|error| matches!(error, InkControllerError::StartWhilePending { .. }))
    );
}

#[test]
fn batched_cancel_allows_new_touch_or_mouse_ink_after_explicit_discard() {
    for replacement_touch in [false, true] {
        let mut h = harness(board());
        tool(&mut h, BoardTool::Pen);
        let start = point(&h, Vec2::new(-100.0, -100.0));
        let end = start + vec2(60.0, 40.0);
        touch(&mut h, egui::TouchPhase::Start, start, Some(0.5));
        touch(&mut h, egui::TouchPhase::Move, end, Some(0.5));
        assert!(h.state_mut().view.cancel_ink());
        let event = |id, phase, pos| egui::Event::Touch {
            device_id: egui::TouchDeviceId(1),
            id: egui::TouchId(id),
            phase,
            pos,
            force: Some(0.5),
        };
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        h.input_mut()
            .events
            .extend([event(1, egui::TouchPhase::Cancel, end), button(start, true)]);
        if replacement_touch {
            h.input_mut()
                .events
                .push(event(2, egui::TouchPhase::Start, start));
        }
        h.input_mut().events.push(egui::Event::PointerMoved(end));
        if replacement_touch {
            h.input_mut()
                .events
                .push(event(2, egui::TouchPhase::Move, end));
        }
        h.input_mut().events.push(button(end, false));
        if replacement_touch {
            h.input_mut().events.extend([
                egui::Event::PointerGone,
                event(2, egui::TouchPhase::End, end),
            ]);
        }
        h.run_steps(3);
        assert_eq!(h.state().doc.sheet().canvas.objects.len(), 1);
        assert!(!h.state().view.has_pending_ink_gesture());
        assert!(
            h.state().ink_errors.is_empty(),
            "{:?}",
            h.state().ink_errors
        );
        assert!(h.state_mut().doc.undo());
        assert!(h.state().doc.sheet().canvas.objects.is_empty());
    }
}

#[test]
fn second_touch_retains_stroke_and_yields_to_pinch_navigation() {
    let mut h = harness(board());
    tool(&mut h, BoardTool::Pen);
    let start = point(&h, Vec2::new(-100.0, -100.0));
    touch(&mut h, egui::TouchPhase::Start, start, Some(0.5));
    touch(
        &mut h,
        egui::TouchPhase::Move,
        start + vec2(15.0, 0.0),
        Some(0.5),
    );
    assert!(h.state().view.has_pending_ink_gesture());
    h.event(egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(2),
        phase: egui::TouchPhase::Start,
        pos: start + vec2(50.0, 0.0),
        force: None,
    });
    h.run_steps(1);
    assert!(matches!(
        h.state().ink_errors.last(),
        Some(InkControllerError::Interrupted(
            crate::ink_controller::InkCancelReason::Multitouch,
        ))
    ));
    assert!(h.state().view.has_released_ink_gesture());
    assert!(h.state().view.pending_ink_samples().len() >= 2);
    assert!(h.state().doc.sheet().canvas.objects.is_empty());
    assert_eq!(h.state().view.touch_navigation, Some(true));
    touch(&mut h, egui::TouchPhase::End, start, Some(0.5));
    h.event(egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(2),
        phase: egui::TouchPhase::End,
        pos: start + vec2(50.0, 0.0),
        force: None,
    });
    h.run_steps(2);
    assert_eq!(h.state().ink_errors.len(), 1);
    assert!(h.state().view.has_released_ink_gesture());
    assert!(!h.state().doc.can_undo());
}

#[test]
fn losing_window_focus_retains_a_held_stroke_even_without_a_focus_event() {
    let mut h = harness(board());
    tool(&mut h, BoardTool::Pen);
    let start = point(&h, Vec2::new(-100.0, -100.0));
    press(&mut h, start, Modifiers::NONE);
    move_to(&mut h, start + vec2(20.0, 0.0));
    assert!(h.state().view.has_pending_ink_gesture());
    h.input_mut().focused = false;
    h.run_steps(1);
    assert!(h.state().view.has_released_ink_gesture());
    assert!(h.state().view.pending_ink_samples().len() >= 2);
    assert!(matches!(
        h.state().ink_errors.last(),
        Some(InkControllerError::Interrupted(
            crate::ink_controller::InkCancelReason::FocusLost,
        ))
    ));
    assert!(h.state().doc.sheet().canvas.objects.is_empty());
}

#[test]
fn invalid_touch_pressure_on_release_never_commits_a_partial_stroke() {
    let mut h = harness(board());
    tool(&mut h, BoardTool::Pen);
    let start = point(&h, Vec2::new(-100.0, -100.0));

    touch(&mut h, egui::TouchPhase::Start, start, Some(0.2));
    touch(
        &mut h,
        egui::TouchPhase::Move,
        start + vec2(20.0, 0.0),
        Some(0.6),
    );
    touch(
        &mut h,
        egui::TouchPhase::End,
        start + vec2(40.0, 0.0),
        Some(1.5),
    );

    assert!(h.state().ink_errors.iter().any(|error| matches!(
        error,
        InkControllerError::InvalidSample(rejected)
            if rejected.sample.pressure == Some(1.5)
    )));
    assert!(
        h.state()
            .ink_errors
            .iter()
            .any(|error| matches!(error, InkControllerError::RejectedInputPending))
    );
    assert!(h.state().doc.sheet().canvas.objects.is_empty());
    assert!(h.state().view.has_released_ink_gesture());
    let samples = h.state().view.pending_ink_samples();
    assert_eq!(samples.len(), 2);
    assert_eq!(samples[0].pressure, Some(0.2));
    assert_eq!(samples[1].pressure, Some(0.6));
}

#[test]
fn rejected_touch_samples_report_input_and_never_commit_a_partial_stroke() {
    let mut h = harness(board());
    tool(&mut h, BoardTool::Pen);
    let start = point(&h, Vec2::new(-100.0, -100.0));
    touch(&mut h, egui::TouchPhase::Start, start, Some(0.2));
    touch(
        &mut h,
        egui::TouchPhase::Move,
        start + vec2(20.0, 0.0),
        Some(1.5),
    );
    touch(
        &mut h,
        egui::TouchPhase::End,
        start + vec2(40.0, 0.0),
        Some(0.9),
    );
    assert!(h.state().ink_errors.iter().any(|error| matches!(
        error,
        InkControllerError::InvalidSample(rejected)
            if rejected.sample.pressure == Some(1.5)
    )));
    assert!(
        h.state()
            .ink_errors
            .iter()
            .any(|error| matches!(error, InkControllerError::RejectedInputPending))
    );
    assert!(h.state().doc.sheet().canvas.objects.is_empty());
    assert!(h.state().view.has_released_ink_gesture());
    let samples = h.state().view.pending_ink_samples().to_vec();
    assert_eq!(samples.first().unwrap().pressure, Some(0.2));
    assert_eq!(samples.last().unwrap().pressure, Some(0.9));
    let retry = {
        let state = h.state_mut();
        state.view.retry_ink(&mut state.doc)
    };
    assert!(matches!(
        retry,
        Err(InkControllerError::RejectedInputPending)
    ));
    assert_eq!(h.state().view.pending_ink_samples(), samples);
    assert!(h.state_mut().view.cancel_ink());
    assert!(!h.state().view.has_pending_ink_gesture());
}

#[test]
fn failed_stroke_is_reported_and_enter_retries_with_updated_settings() {
    let mut h = harness(board());
    h.state_mut().view.ink_settings.finish = StrokeFinishOptions {
        tolerance: -1.0,
        smooth: false,
    };
    tool(&mut h, BoardTool::Pen);
    let start = point(&h, Vec2::new(-100.0, -100.0));
    press(&mut h, start, Modifiers::NONE);
    move_to(&mut h, start + vec2(50.0, 0.0));
    release(&mut h, start + vec2(100.0, 0.0), Modifiers::NONE);
    assert!(matches!(
        h.state().ink_errors.last(),
        Some(InkControllerError::Ink(InkInputError::InvalidTolerance(
            -1.0
        )))
    ));
    let samples = h.state().view.pending_ink_samples().to_vec();
    assert!(samples.len() >= 2);
    assert!(h.state().doc.sheet().canvas.objects.is_empty());
    h.state_mut().view.ink_settings.finish = StrokeFinishOptions::default();
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert!(!h.state().view.has_pending_ink_gesture());
    assert_eq!(h.state().doc.sheet().canvas.objects.len(), 1);
    assert!(h.state_mut().doc.undo());
    assert!(h.state().doc.sheet().canvas.objects.is_empty());
}

#[test]
fn escape_cancels_failed_ink_while_an_accessible_board_item_has_focus() {
    let mut sheet = board();
    let id = sheet
        .add_canvas_object(CanvasObject::shape(
            thinoda_core::ShapeKind::Rectangle,
            thinoda_core::Rect::from_center_size(Vec2::ZERO, Vec2::new(120.0, 80.0)),
        ))
        .unwrap();
    let mut h = harness(sheet);
    h.state_mut().view.ink_settings.finish.tolerance = -1.0;
    tool(&mut h, BoardTool::Pen);
    let start = point(&h, Vec2::new(-200.0, -180.0));
    press(&mut h, start, Modifiers::NONE);
    release(&mut h, start + vec2(50.0, 0.0), Modifiers::NONE);
    assert!(h.state().view.has_released_ink_gesture());
    assert_eq!(h.state().view.pending_ink_mode(), Some(InkMode::Pen));
    h.get_by_label("Rectangle").focus();
    h.run_steps(2);
    assert!(h.get_by_label("Rectangle, Selected").is_focused());
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert!(!h.state().view.has_pending_ink_gesture());
    assert_eq!(h.state().view.board_tool, BoardTool::Pen);
    assert_eq!(h.state().view.board_selection, vec![id]);
    assert!(matches!(
        h.state().outcomes.last(),
        Some(InkOutcome::Canceled(
            crate::ink_controller::InkCancelReason::Escape
        ))
    ));
}

#[test]
fn rejected_editor_commit_reports_the_touch_start_and_preserves_the_draft() {
    let mut sheet = board();
    let id = sheet
        .add_canvas_object(CanvasObject::shape(
            thinoda_core::ShapeKind::Rectangle,
            thinoda_core::Rect::from_center_size(Vec2::ZERO, Vec2::new(120.0, 80.0)),
        ))
        .unwrap();
    let mut h = harness(sheet);
    {
        let state = h.state_mut();
        assert!(state.view.begin_board_label(&state.doc, id));
    }
    h.run_steps(2);
    let draft = "x".repeat(thinoda_core::board::MAX_BOARD_TEXT_CHARS + 1);
    h.get_by_label("Board object label").type_text(&draft);
    h.run_steps(2);
    tool(&mut h, BoardTool::Pen);
    let start = point(&h, Vec2::new(-200.0, -180.0));
    touch(&mut h, egui::TouchPhase::Start, start, Some(0.75));
    assert!(
        h.state().ink_errors.iter().any(|error| matches!(
            error,
            InkControllerError::BlockedByEditor {
                position,
                pressure: Some(0.75),
                ..
            } if *position == start
        )),
        "errors: {:?}",
        h.state().ink_errors
    );
    assert_eq!(
        h.state()
            .ink_errors
            .iter()
            .filter(|error| matches!(error, InkControllerError::BlockedByEditor { .. }))
            .count(),
        1,
        "the synthetic mouse event must not retry the editor commit"
    );
    assert_eq!(
        h.state().view.board_label_draft(),
        Some((id, "", draft.as_str()))
    );
    assert_eq!(h.state().doc.sheet().canvas.objects.len(), 1);
    touch(&mut h, egui::TouchPhase::End, start, Some(0.75));
    assert!(!h.state().view.has_pending_ink_gesture());
}

#[test]
fn touching_the_label_editor_does_not_start_ink_or_commit_the_draft() {
    let mut sheet = board();
    let id = sheet
        .add_canvas_object(CanvasObject::shape(
            thinoda_core::ShapeKind::Rectangle,
            thinoda_core::Rect::from_center_size(Vec2::ZERO, Vec2::new(120.0, 80.0)),
        ))
        .unwrap();
    let mut h = harness(sheet);
    {
        let state = h.state_mut();
        assert!(state.view.begin_board_label(&state.doc, id));
    }
    h.run_steps(2);
    h.get_by_label("Board object label").type_text("Draft");
    h.run_steps(2);
    tool(&mut h, BoardTool::Pen);
    let pos = h.get_by_label("Board object label").rect().center();
    touch(&mut h, egui::TouchPhase::Start, pos, Some(0.5));
    touch(&mut h, egui::TouchPhase::End, pos, Some(0.5));
    assert_eq!(h.state().view.board_label_draft(), Some((id, "", "Draft")));
    assert!(!h.state().view.has_pending_ink_gesture());
    assert_eq!(h.state().doc.sheet().canvas.objects.len(), 1);
    assert!(
        h.state().ink_errors.is_empty(),
        "{:?}",
        h.state().ink_errors
    );
}

#[test]
fn batched_touch_restart_over_label_editor_preserves_its_draft() {
    let mut sheet = board();
    let id = sheet
        .add_canvas_object(CanvasObject::shape(
            thinoda_core::ShapeKind::Rectangle,
            thinoda_core::Rect::from_center_size(Vec2::ZERO, Vec2::new(120.0, 80.0)),
        ))
        .unwrap();
    let mut h = harness(sheet);
    {
        let state = h.state_mut();
        assert!(state.view.begin_board_label(&state.doc, id));
    }
    h.run_steps(2);
    h.get_by_label("Board object label").type_text("Draft");
    h.run_steps(2);
    tool(&mut h, BoardTool::Pen);
    let pos = h.get_by_label("Board object label").rect().center();
    touch(&mut h, egui::TouchPhase::Start, pos, Some(0.5));
    let event = |id, phase| egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(id),
        phase,
        pos,
        force: Some(0.5),
    };
    let button = |pressed| egui::Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    h.input_mut().events.extend([
        event(1, egui::TouchPhase::Cancel),
        button(true),
        event(2, egui::TouchPhase::Start),
        button(false),
        egui::Event::PointerGone,
        event(2, egui::TouchPhase::End),
    ]);
    h.run_steps(2);
    assert_eq!(h.state().view.board_label_draft(), Some((id, "", "Draft")));
    assert!(!h.state().view.has_pending_ink_gesture());
    assert_eq!(h.state().doc.sheet().canvas.objects.len(), 1);
    assert!(
        h.state().ink_errors.is_empty(),
        "{:?}",
        h.state().ink_errors
    );
}

#[test]
fn object_eraser_reports_locked_hit_and_escape_discards_the_pending_path() {
    let mut sheet = board();
    let id = stroke(
        &mut sheet,
        Vec2::new(-80.0, -100.0),
        Vec2::new(80.0, -100.0),
    );
    let mut h = harness(sheet);
    h.state_mut()
        .doc
        .set_canvas_objects_locked(&[id], true)
        .unwrap();
    h.run_steps(1);
    tool(&mut h, BoardTool::ObjectEraser);
    let p = point(&h, Vec2::new(0.0, -100.0));
    press(&mut h, p, Modifiers::NONE);
    release(&mut h, p, Modifiers::NONE);
    assert!(matches!(
        h.state().ink_errors.last(),
        Some(InkControllerError::Locked(members))
            if members.contains(&BoardMember::Object(id))
    ));
    assert!(h.state().view.has_pending_ink_gesture());
    assert!(h.state().doc.sheet().canvas.object(id).is_some());
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert!(!h.state().view.has_pending_ink_gesture());
    assert!(h.state().doc.sheet().canvas.object(id).is_some());
}

#[test]
fn object_eraser_removes_whole_strokes_and_reconciles_selection_in_one_edit() {
    let mut sheet = board();
    let erased = stroke(
        &mut sheet,
        Vec2::new(-100.0, -100.0),
        Vec2::new(100.0, -100.0),
    );
    let other = stroke(&mut sheet, Vec2::new(-20.0, 100.0), Vec2::new(20.0, 100.0));
    let mut h = harness(sheet);
    h.state_mut().view.board_selection = vec![erased, other];
    tool(&mut h, BoardTool::ObjectEraser);
    let p = point(&h, Vec2::new(0.0, -100.0));
    press(&mut h, p, Modifiers::NONE);
    release(&mut h, p, Modifiers::NONE);
    assert!(matches!(
        h.state().outcomes.last(),
        Some(InkOutcome::ObjectErased(ids)) if ids == &[erased]
    ));
    assert_eq!(h.state().view.board_selection, vec![other]);
    assert!(h.state().doc.sheet().canvas.object(erased).is_none());
    assert!(h.state_mut().doc.undo());
    assert!(h.state().doc.sheet().canvas.object(erased).is_some());
    assert!(!h.state().doc.can_undo());
    assert!(h.state_mut().doc.redo());
    assert!(h.state().doc.sheet().canvas.object(erased).is_none());
    assert!(
        h.state().ink_errors.is_empty(),
        "{:?}",
        h.state().ink_errors
    );
}

#[test]
fn pixel_erase_preserves_selection_of_each_replacement_and_undoes_once() {
    let mut sheet = board();
    let line = stroke(
        &mut sheet,
        Vec2::new(-100.0, -100.0),
        Vec2::new(100.0, -100.0),
    );
    let other = stroke(&mut sheet, Vec2::new(-20.0, 100.0), Vec2::new(20.0, 100.0));
    let mut h = harness(sheet);
    h.state_mut().view.board_selection = vec![line, other];
    h.state_mut().view.ink_settings.eraser_radius = 5.0;
    tool(&mut h, BoardTool::PixelEraser);
    let p = point(&h, Vec2::new(0.0, -100.0));
    press(&mut h, p, Modifiers::NONE);
    release(&mut h, p, Modifiers::NONE);
    assert!(
        h.state().ink_errors.is_empty(),
        "{:?}",
        h.state().ink_errors
    );
    let InkOutcome::PixelErased { replacements, .. } = h.state().outcomes.last().unwrap() else {
        panic!("expected pixel erase")
    };
    assert_eq!(replacements.len(), 1);
    assert_eq!(replacements[0].original, line);
    assert_eq!(replacements[0].fragments.len(), 2);
    for id in &replacements[0].fragments {
        assert!(h.state().view.board_selection.contains(id));
    }
    assert!(h.state().view.board_selection.contains(&other));
    assert_eq!(h.state().doc.sheet().canvas.objects.len(), 3);
    let erased = h.state().doc.sheet().canvas.objects.clone();
    assert!(h.state_mut().doc.undo());
    h.run_steps(1);
    assert_eq!(h.state().doc.sheet().canvas.objects.len(), 2);
    assert_eq!(h.state().view.board_selection, vec![line, other]);
    assert!(!h.state().doc.can_undo());
    assert!(h.state_mut().doc.redo());
    assert_eq!(h.state().doc.sheet().canvas.objects, erased);
}

#[test]
fn lasso_replaces_then_adds_to_the_board_selection_without_an_edit() {
    let mut sheet = board();
    let a = stroke(
        &mut sheet,
        Vec2::new(-120.0, -100.0),
        Vec2::new(-80.0, -100.0),
    );
    let b = stroke(
        &mut sheet,
        Vec2::new(80.0, -100.0),
        Vec2::new(120.0, -100.0),
    );
    let mut h = harness(sheet);
    tool(&mut h, BoardTool::Lasso);
    let a0 = point(&h, Vec2::new(-140.0, -120.0));
    press(&mut h, a0, Modifiers::NONE);
    move_to(&mut h, a0 + vec2(80.0, 0.0));
    move_to(&mut h, a0 + vec2(80.0, 40.0));
    move_to(&mut h, a0 + vec2(0.0, 40.0));
    release(&mut h, a0, Modifiers::NONE);
    assert_eq!(h.state().view.board_selection, vec![a]);
    let b0 = point(&h, Vec2::new(60.0, -120.0));
    press(&mut h, b0, Modifiers::SHIFT);
    move_to(&mut h, b0 + vec2(80.0, 0.0));
    move_to(&mut h, b0 + vec2(80.0, 40.0));
    move_to(&mut h, b0 + vec2(0.0, 40.0));
    release(&mut h, b0, Modifiers::SHIFT);
    assert_eq!(h.state().view.board_selection, vec![a, b]);
    assert!(
        h.state().ink_errors.is_empty(),
        "{:?}",
        h.state().ink_errors
    );
    assert!(!h.state().doc.can_undo());
}

#[test]
fn lasso_expands_a_card_hit_to_its_mixed_board_group() {
    let mut sheet = board();
    let card = sheet
        .add_card(
            "Grouped card",
            Vec2::new(-150.0, -100.0),
            Card::new(CardKind::Sticky, Vec2::new(100.0, 60.0)),
        )
        .unwrap();
    let object = sheet
        .add_canvas_object(CanvasObject::shape(
            thinoda_core::ShapeKind::Rectangle,
            thinoda_core::Rect::from_center_size(Vec2::new(150.0, 100.0), Vec2::new(80.0, 60.0)),
        ))
        .unwrap();
    sheet
        .group_board_members(&[BoardMember::Card(card), BoardMember::Object(object)])
        .unwrap();
    let mut h = harness(sheet);
    tool(&mut h, BoardTool::Lasso);
    let start = point(&h, Vec2::new(-220.0, -150.0));
    press(&mut h, start, Modifiers::NONE);
    move_to(&mut h, start + vec2(140.0, 0.0));
    move_to(&mut h, start + vec2(140.0, 100.0));
    move_to(&mut h, start + vec2(0.0, 100.0));
    release(&mut h, start, Modifiers::NONE);
    assert_eq!(h.state().view.selection, vec![card]);
    assert_eq!(h.state().view.board_selection, vec![object]);
    assert!(
        h.state().ink_errors.is_empty(),
        "{:?}",
        h.state().ink_errors
    );
    assert!(!h.state().doc.can_undo());
}

#[test]
fn minimap_does_not_start_ink_and_external_ink_drag_can_cross_it() {
    let mut h = harness(board());
    h.state_mut().view.set_minimap_visible(true);
    tool(&mut h, BoardTool::Pen);
    h.run_steps(2);
    let mini = h
        .state()
        .view
        .minimap_rect(h.state().view.canvas_rect, h.state().view.canvas_rect)
        .unwrap();
    let inside = mini.center();
    press(&mut h, inside, Modifiers::NONE);
    release(&mut h, inside, Modifiers::NONE);
    assert!(h.state().doc.sheet().canvas.objects.is_empty());
    assert!(!h.state().view.has_pending_ink_gesture());

    let start = mini.min - vec2(120.0, 80.0);
    press(&mut h, start, Modifiers::NONE);
    move_to(&mut h, start + vec2(20.0, 20.0));
    move_to(&mut h, inside);
    release(&mut h, inside, Modifiers::NONE);
    assert!(
        h.state().ink_errors.is_empty(),
        "{:?}",
        h.state().ink_errors
    );
    assert_eq!(h.state().doc.sheet().canvas.objects.len(), 1);
    assert!(!h.state().view.has_pending_ink_gesture());
}

#[test]
fn minimap_keyboard_focus_does_not_retry_or_cancel_pending_ink() {
    let mut sheet = board();
    stroke(
        &mut sheet,
        Vec2::new(-100.0, -100.0),
        Vec2::new(100.0, -100.0),
    );
    let mut h = harness(sheet);
    h.state_mut().view.set_minimap_visible(true);
    h.state_mut().view.ink_settings.finish.tolerance = -1.0;
    tool(&mut h, BoardTool::Pen);
    let start = point(&h, Vec2::new(-200.0, -180.0));
    press(&mut h, start, Modifiers::NONE);
    release(&mut h, start + vec2(50.0, 0.0), Modifiers::NONE);
    let samples = h.state().view.pending_ink_samples().to_vec();
    assert!(h.state().view.has_released_ink_gesture());
    let minimap_name = h.state().view.minimap_labels.accessible_name.clone();
    h.get_by_label(&minimap_name).focus();
    h.run_steps(2);
    assert!(h.get_by_label(&minimap_name).is_focused());
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert!(h.state().view.has_released_ink_gesture(), "Enter leaked");
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert_eq!(h.state().view.pending_ink_samples(), samples);
    assert!(h.state().view.has_released_ink_gesture());
    assert!(h.get_by_label("Mind map canvas").is_focused());
    assert_eq!(h.state().doc.sheet().canvas.objects.len(), 1);
    {
        let state = h.state_mut();
        state
            .view
            .retry_ink_with_finish_options(&mut state.doc, StrokeFinishOptions::default())
            .unwrap();
    }
    assert_eq!(h.state().doc.sheet().canvas.objects.len(), 2);
    assert!(!h.state().view.has_pending_ink_gesture());
}

#[test]
fn minimap_owned_touch_release_clears_earlier_external_touches() {
    let mut h = harness(board());
    h.state_mut().view.set_minimap_visible(true);
    tool(&mut h, BoardTool::Pen);
    let mini = h
        .state()
        .view
        .minimap_rect(h.state().view.canvas_rect, h.state().view.canvas_rect)
        .unwrap();
    let send_touch = |h: &mut Harness<'_, State>, id, phase, pos| {
        h.event(egui::Event::Touch {
            device_id: egui::TouchDeviceId(1),
            id: egui::TouchId(id),
            phase,
            pos,
            force: None,
        });
        h.run_steps(1);
    };
    let outside = h.state().view.canvas_rect.min - vec2(20.0, 20.0);
    send_touch(&mut h, 1, egui::TouchPhase::Start, outside);
    send_touch(&mut h, 2, egui::TouchPhase::Start, mini.center());
    send_touch(&mut h, 1, egui::TouchPhase::End, outside);
    send_touch(&mut h, 2, egui::TouchPhase::End, mini.center());
    h.run_steps(2);
    assert!(h.state().doc.sheet().canvas.objects.is_empty());

    let start = point(&h, Vec2::new(-100.0, -100.0));
    send_touch(&mut h, 3, egui::TouchPhase::Start, start);
    send_touch(&mut h, 3, egui::TouchPhase::End, start + vec2(40.0, 0.0));
    assert!(
        h.state().ink_errors.is_empty(),
        "{:?}",
        h.state().ink_errors
    );
    assert_eq!(h.state().doc.sheet().canvas.objects.len(), 1);
}
