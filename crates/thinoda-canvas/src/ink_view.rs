//! Adapts the independent ink controller to the canvas widget's ownership rules.

use std::collections::{HashMap, HashSet};

use super::*;
use crate::ink_controller::{InkCancelReason, InkFrameInput};
use crate::pen_input::{self, PenFrameData};
use egui::{Event, TouchPhase};
use thinoda_core::EditError;

pub(super) struct Routing {
    pub minimap_pointer: bool,
    pub minimap_keyboard: bool,
    pub touch_navigation: bool,
    pub previous_touch_navigation: bool,
    pub restart_after_cancel: Option<usize>,
    pub space: bool,
}

fn attempted_start(
    events: &[Event],
    pen: &PenFrameData,
    controller: &InkController,
    canvas: Rect,
) -> Option<(usize, Pos2, Option<f32>)> {
    let frame_has_touch = events
        .iter()
        .any(|event| matches!(event, Event::Touch { .. }));
    events
        .iter()
        .enumerate()
        .filter(|(index, event)| {
            controller.accepts_start_event(event, frame_has_touch, pen.sample(*index).is_some())
        })
        .find_map(|(index, event)| {
            if pen.touch_pointer(index) && pen.touch_issue.is_none() {
                return None;
            }
            let (pos, pressure) = match event {
                Event::Touch {
                    phase: TouchPhase::Start,
                    pos,
                    force,
                    ..
                } => (*pos, *force),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    ..
                } => (*pos, pen.sample(index).map(|sample| sample.pressure)),
                _ => return None,
            };
            start_in_rect(canvas, pos).then_some((index, pos, pressure))
        })
}

pub(super) fn starts_confirmed_pen(
    ui: &Ui,
    view: &MapView,
    response: &Response,
    space: bool,
) -> bool {
    let ctx = ui.ctx();
    if view.board_tool.ink_mode().is_none()
        || view.ink_controller.has_pending_gesture()
        || view.touch_navigation.is_some()
        || !ui.is_enabled()
        || egui::Popup::is_any_open(ctx)
        || space
        || !matches!(view.drag, Drag::None)
        || view.board_input.is_active()
        || view.relationship_drag.is_some()
        || view.image_resize.is_some()
    {
        return false;
    }
    let (focused, events) = ctx.input(|input| (input.focused, input.events.clone()));
    if !focused {
        return false;
    }
    let viewport = ctx.viewport_id();
    let mut pen = pen_input::peek(ctx, viewport, &events);
    let touch = crate::touch_input::peek_pointer_frame(ctx, viewport, &events);
    if pen.issue.is_some() || pen.canceled.is_some() || touch.issue.is_some() {
        return false;
    }
    pen.touch_pointers = touch.pointers;
    let canvas = view.canvas_rect.intersect(ui.clip_rect());
    let Some((index, position, _)) = attempted_start(&events, &pen, &view.ink_controller, canvas)
    else {
        return false;
    };
    pen.sample(index).is_some()
        && canvas.contains(position)
        && !editor_contains_start(ctx, view, position)
        && !ctx
            .layer_id_at(position)
            .is_some_and(|layer| layer != ui.layer_id())
        && (response.contains_pointer()
            || events
                .iter()
                .any(|event| matches!(event, Event::Touch { .. })))
}

fn start_in_rect(rect: Rect, position: Pos2) -> bool {
    !position.x.is_finite() || !position.y.is_finite() || rect.contains(position)
}

fn editor_contains_start(ctx: &egui::Context, view: &MapView, position: Pos2) -> bool {
    let editor_id = if view.editing.is_some() {
        MapCanvas::title_editor_id()
    } else if view.editing_board_object().is_some() {
        MapCanvas::board_label_editor_id()
    } else {
        return false;
    };
    ctx.read_response(editor_id)
        .is_some_and(|response| response.rect.contains(position))
}

pub(super) fn record_error(out: &mut CanvasOutput, error: InkControllerError) {
    if out.error.is_none() {
        out.error = Some(EditError::Invalid(error.to_string()));
    }
    out.ink_errors.push(error);
}

pub(super) fn interact(
    ui: &Ui,
    doc: &mut Document,
    view: &mut MapView,
    scene: &Scene,
    response: &Response,
    routing: Routing,
    out: &mut CanvasOutput,
) -> bool {
    let ctx = ui.ctx();
    let (focused, mut events) = ctx.input(|input| (input.focused, input.events.clone()));
    let frame = (ctx.viewport_id(), ctx.cumulative_frame_nr());
    let mut pen = if view.ink_events_frame == Some(frame) {
        events.clear();
        PenFrameData::default()
    } else {
        pen_input::take(ctx, frame.0, &events)
    };
    let touch = crate::touch_input::take_pointer_frame(ctx, frame.0, &events);
    pen.touch_pointers = touch.pointers;
    pen.touch_issue = touch.issue;
    view.ink_events_frame = Some(frame);
    let remaining = routing
        .restart_after_cancel
        .filter(|offset| *offset <= events.len())
        .map(|offset| {
            let remaining_events = events.split_off(offset);
            let remaining_samples = if pen.samples.is_empty() {
                Vec::new()
            } else {
                pen.samples.split_off(offset)
            };
            let remaining_touch = if pen.touch_pointers.is_empty() {
                Vec::new()
            } else {
                pen.touch_pointers.split_off(offset)
            };
            (
                remaining_events,
                PenFrameData {
                    samples: remaining_samples,
                    canceled: pen.canceled.take(),
                    issue: pen.issue,
                    touch_pointers: remaining_touch,
                    touch_issue: pen.touch_issue,
                },
            )
        });
    let scene_state = doc.state_id();
    let first_navigation = if remaining.is_some() {
        routing.previous_touch_navigation
    } else {
        routing.touch_navigation
    };
    let mut captured = interact_events(
        ui,
        doc,
        view,
        scene,
        response,
        &routing,
        focused,
        first_navigation,
        scene_state,
        &mut events,
        &mut pen,
        out,
    );
    if let Some((mut remaining, mut remaining_pen)) = remaining {
        captured |= interact_events(
            ui,
            doc,
            view,
            scene,
            response,
            &routing,
            focused,
            routing.touch_navigation,
            scene_state,
            &mut remaining,
            &mut remaining_pen,
            out,
        );
    }
    captured
}

#[allow(clippy::too_many_arguments)]
fn interact_events(
    ui: &Ui,
    doc: &mut Document,
    view: &mut MapView,
    scene: &Scene,
    response: &Response,
    routing: &Routing,
    focused: bool,
    touch_navigation: bool,
    scene_state: u64,
    events: &mut Vec<Event>,
    pen: &mut PenFrameData,
    out: &mut CanvasOutput,
) -> bool {
    let ctx = ui.ctx();
    let mode = view.board_tool.ink_mode();
    let popup = egui::Popup::is_any_open(ctx);
    let enabled = ui.is_enabled()
        && !popup
        && !routing.minimap_pointer
        && (!routing.space || view.ink_controller.has_pending_gesture());
    let keyboard_owned = focused
        && !routing.minimap_keyboard
        && !popup
        && view.editing.is_none()
        && view.editing_board_object().is_none()
        && (response.has_focus() || ctx.memory(|memory| memory.focused()).is_none());
    // Disabled pointer events still reach the controller so it can observe
    // releases/cancellations and clear ownership without starting a gesture.
    if !keyboard_owned {
        let mut samples = Vec::with_capacity(pen.samples.len());
        let mut pointers = Vec::with_capacity(pen.touch_pointers.len());
        let mut index = 0;
        events.retain(|event| {
            let keep = !matches!(event, Event::Key { .. });
            if keep {
                if let Some(sample) = pen.samples.get(index) {
                    samples.push(*sample);
                }
                if let Some(pointer) = pen.touch_pointers.get(index) {
                    pointers.push(*pointer);
                }
            }
            index += 1;
            keep
        });
        pen.samples = samples;
        pen.touch_pointers = pointers;
    }
    if !focused
        && !events
            .iter()
            .any(|event| matches!(event, Event::WindowFocused(false)))
    {
        events.insert(0, Event::WindowFocused(false));
        if !pen.samples.is_empty() {
            pen.samples.insert(0, None);
        }
        if !pen.touch_pointers.is_empty() {
            pen.touch_pointers.insert(0, false);
        }
    }
    let touch_start = events.iter().any(|event| {
        matches!(
            event,
            Event::Touch {
                phase: TouchPhase::Start,
                ..
            }
        )
    });
    let canvas = view.canvas_rect.intersect(ui.clip_rect());
    let start = attempted_start(events, pen, &view.ink_controller, canvas);
    let can_start = enabled
        && focused
        && !touch_navigation
        && mode.is_some()
        && matches!(view.drag, Drag::None)
        && !view.board_input.is_active()
        && view.relationship_drag.is_none()
        && view.image_resize.is_none()
        && !start.is_some_and(|(_, position, _)| editor_contains_start(ctx, view, position))
        && !start.is_some_and(|(_, position, _)| {
            ctx.layer_id_at(position)
                .is_some_and(|layer| layer != ui.layer_id())
        })
        && (response.contains_pointer() || touch_start);

    let blocked_start = if can_start
        && pen.issue.is_none()
        && pen.touch_issue.is_none()
        && !view.ink_controller.has_pending_gesture()
        && let Some((_, position, pressure)) = start
        && let Err(error) = view
            .try_commit_edit(doc)
            .and_then(|_| view.try_commit_board_label(doc))
    {
        record_error(
            out,
            InkControllerError::BlockedByEditor {
                position,
                pressure,
                error,
            },
        );
        true
    } else {
        false
    };

    let updated_scene;
    let scene = if doc.state_id() != scene_state {
        let layout = view.layout_with(ctx, doc);
        updated_scene = build_scene(doc.sheet(), &layout);
        &updated_scene
    } else {
        scene
    };
    let transform = view.xf();
    let settings = view.ink_settings;
    let frame = view.ink_controller.process_frame_with_pen(
        doc,
        scene,
        mode,
        settings,
        InkFrameInput {
            events,
            canvas,
            transform,
            enabled,
            can_start: can_start && !blocked_start,
            modifiers: ctx.input(|input| input.modifiers),
        },
        pen,
    );
    if frame.captured && start.is_some() && enabled {
        response.request_focus();
    }
    for outcome in frame.outcomes {
        if matches!(outcome, InkOutcome::Canceled(InkCancelReason::Escape)) {
            for event in events.iter() {
                if let Event::Key {
                    key: Key::Escape,
                    pressed: true,
                    modifiers,
                    ..
                } = event
                {
                    ctx.input_mut(|input| input.consume_key(*modifiers, Key::Escape));
                }
            }
        }
        apply_outcome(view, doc.sheet(), &outcome);
        out.ink_outcomes.push(outcome);
    }
    for error in frame.errors {
        record_error(out, error);
    }
    frame.captured || blocked_start
}

/// Returns true if Enter was consumed for a released failed gesture.
pub(super) fn retry_on_enter(
    ui: &Ui,
    doc: &mut Document,
    view: &mut MapView,
    out: &mut CanvasOutput,
) -> bool {
    if !view.ink_controller.has_released_gesture()
        || !ui
            .ctx()
            .input_mut(|input| input.consume_key(Modifiers::NONE, Key::Enter))
    {
        return false;
    }
    let result = if matches!(
        view.ink_controller.pending_mode(),
        Some(InkMode::Pen | InkMode::Highlighter)
    ) {
        view.retry_ink_with_finish_options(doc, view.ink_settings.finish)
    } else {
        view.retry_ink(doc)
    };
    match result {
        Ok(outcome) => out.ink_outcomes.push(outcome),
        Err(error) => record_error(out, error),
    }
    true
}

/// Handle Escape when an accessible board item owns the canvas keyboard.
pub(super) fn cancel_on_escape(ui: &Ui, view: &mut MapView, out: &mut CanvasOutput) -> bool {
    if !view.ink_controller.has_pending_gesture()
        || !ui
            .ctx()
            .input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape))
    {
        return false;
    }
    if view.cancel_ink() {
        out.ink_outcomes
            .push(InkOutcome::Canceled(InkCancelReason::Escape));
    }
    true
}

pub(super) fn apply_outcome(view: &mut MapView, sheet: &Sheet, outcome: &InkOutcome) {
    match outcome {
        InkOutcome::Selected(selection) => {
            if !selection.additive {
                view.selection.clear();
                view.board_selection.clear();
            }
            view.selected_object = None;
            view.relationship_from = None;
            view.pending_reveal = None;
            view.pending_board_reveal = None;
            for hit in &selection.hits {
                match *hit {
                    SceneHit::Topic(id) if !view.selection.contains(&id) => {
                        view.selection.push(id);
                    }
                    SceneHit::Object(id) if !view.board_selection.contains(&id) => {
                        view.board_selection.push(id);
                    }
                    _ => {}
                }
            }
            view.prune_selection(sheet);
            board_input::expand_selection(view, sheet);
        }
        InkOutcome::PixelErased { replacements, .. } => {
            let replacements: HashMap<_, _> = replacements
                .iter()
                .map(|replacement| (replacement.original, &replacement.fragments))
                .collect();
            let previous = std::mem::take(&mut view.board_selection);
            let mut seen = HashSet::new();
            for id in previous {
                if let Some(fragments) = replacements.get(&id) {
                    for &fragment in *fragments {
                        if seen.insert(fragment) {
                            view.board_selection.push(fragment);
                        }
                    }
                } else if seen.insert(id) {
                    view.board_selection.push(id);
                }
            }
            view.prune_selection(sheet);
            board_input::expand_selection(view, sheet);
        }
        InkOutcome::ObjectErased(_) => {
            view.prune_selection(sheet);
            board_input::expand_selection(view, sheet);
        }
        _ => {}
    }
}
