//! Adapts the independent ink controller to the canvas widget's ownership rules.

use std::collections::{HashMap, HashSet};

use super::*;
use crate::ink_controller::{InkCancelReason, InkFrameInput};
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
    controller: &InkController,
    canvas: Rect,
) -> Option<(Pos2, Option<f32>)> {
    let frame_has_touch = events
        .iter()
        .any(|event| matches!(event, Event::Touch { .. }));
    events
        .iter()
        .filter(|event| controller.accepts_start_event(event, frame_has_touch))
        .find_map(|event| {
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
                } => (*pos, None),
                _ => return None,
            };
            start_in_rect(canvas, pos).then_some((pos, pressure))
        })
}

fn start_in_rect(rect: Rect, position: Pos2) -> bool {
    !position.x.is_finite() || !position.y.is_finite() || rect.contains(position)
}

fn editor_contains_start(ctx: &egui::Context, view: &MapView, position: Pos2) -> bool {
    let editor_id = if view.editing.is_some() {
        MapCanvas::id().with("edit")
    } else if view.editing_board_object().is_some() {
        MapCanvas::id().with("board-label-edit")
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
    if view.ink_events_frame == Some(frame) {
        events.clear();
    }
    view.ink_events_frame = Some(frame);
    let remaining = routing
        .restart_after_cancel
        .filter(|offset| *offset <= events.len())
        .map(|offset| events.split_off(offset));
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
        out,
    );
    if let Some(mut remaining) = remaining {
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
    events.retain(|event| keyboard_owned || !matches!(event, Event::Key { .. }));
    if !focused
        && !events
            .iter()
            .any(|event| matches!(event, Event::WindowFocused(false)))
    {
        events.insert(0, Event::WindowFocused(false));
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
    let start = attempted_start(events, &view.ink_controller, canvas);
    let can_start = enabled
        && focused
        && !touch_navigation
        && mode.is_some()
        && matches!(view.drag, Drag::None)
        && !view.board_input.is_active()
        && view.relationship_drag.is_none()
        && view.image_resize.is_none()
        && !start.is_some_and(|(position, _)| editor_contains_start(ctx, view, position))
        && !start.is_some_and(|(position, _)| {
            ctx.layer_id_at(position)
                .is_some_and(|layer| layer != ui.layer_id())
        })
        && (response.contains_pointer() || touch_start);

    let blocked_start = if can_start
        && !view.ink_controller.has_pending_gesture()
        && let Some((position, pressure)) = start
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
    let frame = view.ink_controller.process_frame(
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
