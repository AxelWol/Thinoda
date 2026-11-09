//! Preserve the distinction between absent and invalid browser touch force.

use std::sync::Arc;

use egui::{
    Context, Event, Id, PointerButton, Pos2, TouchDeviceId, TouchId, TouchPhase, ViewportId,
};

use crate::pen_input::PointerPhase;

pub const MAX_TOUCH_RECORDS: usize = 4_096;
const MAX_FRAME_EVENTS: usize = 8_192;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TouchForceRecord {
    pub device: TouchDeviceId,
    pub id: TouchId,
    pub phase: TouchPhase,
    pub position: Pos2,
    pub force_missing: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchForceIssue {
    QueueOverflow,
    FrameTooLarge,
    EventMismatch,
    ForceMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TouchPointerRecord {
    pub phase: PointerPhase,
    pub position: Pos2,
    pub is_touch: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchPointerIssue {
    QueueOverflow,
    FrameTooLarge,
    EventMismatch,
}

#[derive(Clone)]
struct StoredPointerFrame {
    number: u64,
    events: Vec<PointerFrameSignature>,
    buttons: Vec<(bool, bool)>,
    touches_verified: bool,
    issue: Option<TouchPointerIssue>,
}

#[derive(Clone, Copy, PartialEq)]
enum PointerFrameSignature {
    Button(PointerPhase, Pos2),
    TouchMove(Pos2),
    Touch(TouchDeviceId, TouchId, TouchPhase, Pos2),
    Gone,
}

#[derive(Default)]
pub struct TouchPointerFrame {
    pub pointers: Vec<bool>,
    pub synthetic_pointers: Vec<bool>,
    pub touches_verified: bool,
    pub issue: Option<TouchPointerIssue>,
}

fn pointer_slot(viewport: ViewportId) -> Id {
    Id::unique("thinoda_touch_pointer_input").with(viewport)
}

fn pointer_button(event: &Event) -> Option<(PointerPhase, Pos2)> {
    match event {
        Event::PointerButton {
            pos,
            button,
            pressed,
            ..
        } => Some((
            if *pressed {
                PointerPhase::Press(*button)
            } else {
                PointerPhase::Release(*button)
            },
            *pos,
        )),
        _ => None,
    }
}

fn pointer_frame_signature(events: &[Event], index: usize) -> Option<PointerFrameSignature> {
    let event = &events[index];
    if let Some((phase, position)) = pointer_button(event) {
        Some(PointerFrameSignature::Button(phase, position))
    } else if let Event::PointerMoved(position) = event
        && synthetic_touch_move(events, index)
    {
        Some(PointerFrameSignature::TouchMove(*position))
    } else if let Some((device, id, phase, position)) = touch_signature(event) {
        Some(PointerFrameSignature::Touch(device, id, phase, position))
    } else if matches!(event, Event::PointerGone) {
        Some(PointerFrameSignature::Gone)
    } else {
        None
    }
}

// eframe surrounds legacy touch starts/ends with these primary pointer events.
fn synthetic_touch_button(events: &[Event], index: usize) -> bool {
    matches!(
        (
            events.get(index),
            events.get(index + 1),
            events.get(index + 2),
        ),
        (
            Some(Event::PointerButton {
                button: PointerButton::Primary,
                pressed: true,
                ..
            }),
            Some(Event::Touch {
                phase: TouchPhase::Start,
                ..
            }),
            _,
        ) | (
            Some(Event::PointerButton {
                button: PointerButton::Primary,
                pressed: false,
                ..
            }),
            Some(Event::PointerGone),
            Some(Event::Touch {
                phase: TouchPhase::End,
                ..
            }),
        )
    )
}

// Eframe emits the primary pointer move immediately before the changed touches.
// The primary contact can differ from the first changed contact.
fn synthetic_touch_move(events: &[Event], index: usize) -> bool {
    matches!(
        (events.get(index), events.get(index + 1)),
        (
            Some(Event::PointerMoved(_)),
            Some(Event::Touch {
                phase: TouchPhase::Move,
                ..
            })
        )
    )
}

fn synthetic_touch_gone(events: &[Event], index: usize) -> bool {
    matches!(events.get(index), Some(Event::PointerGone))
        && index > 0
        && synthetic_touch_button(events, index - 1)
}

fn touch_records_match(events: &[Event], records: &[TouchForceRecord]) -> bool {
    events.iter().filter_map(touch_signature).eq(records
        .iter()
        .map(|record| (record.device, record.id, record.phase, record.position)))
}

/// Mark browser touch-pointer buttons without changing egui's widget input.
pub fn publish_pointer_frame(
    ctx: &Context,
    viewport: ViewportId,
    events: &[Event],
    records: &[TouchPointerRecord],
    overflowed: bool,
    touches: &[TouchForceRecord],
    touches_overflowed: bool,
) -> Option<TouchPointerIssue> {
    let id = pointer_slot(viewport);
    let number = ctx.cumulative_frame_nr_for(viewport);
    let synthetic = events
        .iter()
        .enumerate()
        .any(|(index, _)| synthetic_touch_button(events, index));
    let has_touch = events
        .iter()
        .any(|event| matches!(event, Event::Touch { .. }));
    let touches_verified = has_touch
        && !touches_overflowed
        && touches.len() <= MAX_TOUCH_RECORDS
        && touch_records_match(events, touches);
    let relevant = synthetic || has_touch || records.iter().any(|record| record.is_touch);
    let mut buttons = Vec::new();
    let issue = if overflowed || records.len() > MAX_TOUCH_RECORDS {
        Some(TouchPointerIssue::QueueOverflow)
    } else if !relevant {
        None
    } else if synthetic && (touches_overflowed || touches.len() > MAX_TOUCH_RECORDS) {
        Some(TouchPointerIssue::QueueOverflow)
    } else if events.len() > MAX_FRAME_EVENTS {
        Some(TouchPointerIssue::FrameTooLarge)
    } else if synthetic && !touches_verified {
        Some(TouchPointerIssue::EventMismatch)
    } else {
        let mut records = records.iter().peekable();
        let mut mismatch = false;
        for (index, event) in events.iter().enumerate() {
            let Some(signature) = pointer_button(event) else {
                continue;
            };
            let captured = records
                .peek()
                .is_some_and(|record| (record.phase, record.position) == signature);
            let synthetic = synthetic_touch_button(events, index);
            match (captured, synthetic) {
                (true, false) => {
                    buttons.push((records.next().expect("record was peeked").is_touch, false));
                }
                (false, true) => buttons.push((true, true)),
                // A captured button with the same signature has ambiguous ownership.
                _ => {
                    mismatch = true;
                    break;
                }
            }
        }
        (mismatch || records.next().is_some()).then_some(TouchPointerIssue::EventMismatch)
    };
    ctx.data_mut(|data| {
        data.remove::<Arc<StoredPointerFrame>>(id);
        if !relevant && issue.is_none() {
            return;
        }
        let events = if events.len() <= MAX_FRAME_EVENTS {
            events
                .iter()
                .enumerate()
                .filter_map(|(index, _)| pointer_frame_signature(events, index))
                .collect()
        } else {
            Vec::new()
        };
        data.insert_temp(
            id,
            Arc::new(StoredPointerFrame {
                number,
                events,
                buttons,
                touches_verified,
                issue,
            }),
        );
    });
    issue
}

pub(crate) fn take_pointer_frame(
    ctx: &Context,
    viewport: ViewportId,
    events: &[Event],
) -> TouchPointerFrame {
    read_pointer_frame(ctx, viewport, events, true)
}

pub fn peek_pointer_frame(
    ctx: &Context,
    viewport: ViewportId,
    events: &[Event],
) -> TouchPointerFrame {
    read_pointer_frame(ctx, viewport, events, false)
}

fn read_pointer_frame(
    ctx: &Context,
    viewport: ViewportId,
    events: &[Event],
    consume: bool,
) -> TouchPointerFrame {
    let id = pointer_slot(viewport);
    let stored = ctx.data_mut(|data| {
        let frame = data.get_temp::<Arc<StoredPointerFrame>>(id);
        if consume {
            data.remove::<Arc<StoredPointerFrame>>(id);
        }
        frame
    });
    let Some(stored) = stored else {
        return TouchPointerFrame::default();
    };
    let mut buttons = stored.buttons.iter();
    let proofs_match = events.iter().enumerate().all(|(index, event)| {
        pointer_button(event).is_none()
            || buttons
                .next()
                .is_some_and(|(_, synthetic)| !*synthetic || synthetic_touch_button(events, index))
    }) && buttons.next().is_none();
    let issue = if let Some(issue) = stored.issue {
        Some(issue)
    } else if events.len() > MAX_FRAME_EVENTS {
        Some(TouchPointerIssue::FrameTooLarge)
    } else if stored.number != ctx.cumulative_frame_nr_for(viewport)
        || !events
            .iter()
            .enumerate()
            .filter_map(|(index, _)| pointer_frame_signature(events, index))
            .eq(stored.events.iter().copied())
        || !proofs_match
    {
        Some(TouchPointerIssue::EventMismatch)
    } else {
        None
    };
    TouchPointerFrame {
        pointers: if issue.is_none() {
            let mut buttons = stored.buttons.iter();
            events
                .iter()
                .enumerate()
                .map(|(index, event)| {
                    if pointer_button(event).is_some() {
                        buttons.next().expect("button signatures were matched").0
                    } else {
                        stored.touches_verified
                            && (synthetic_touch_move(events, index)
                                || synthetic_touch_gone(events, index))
                    }
                })
                .collect()
        } else {
            Vec::new()
        },
        synthetic_pointers: if issue.is_none() {
            let mut buttons = stored.buttons.iter();
            events
                .iter()
                .enumerate()
                .map(|(index, event)| {
                    if pointer_button(event).is_some() {
                        buttons.next().expect("button signatures were matched").1
                    } else {
                        stored.touches_verified
                            && (synthetic_touch_move(events, index)
                                || synthetic_touch_gone(events, index))
                    }
                })
                .collect()
        } else {
            Vec::new()
        },
        touches_verified: issue.is_none() && stored.touches_verified,
        issue,
    }
}

/// Normalize only a proven absent JS `Touch.force`; invalid values stay invalid.
pub fn normalize_missing_force(
    events: &mut [Event],
    records: &[TouchForceRecord],
    overflowed: bool,
) -> Result<(), TouchForceIssue> {
    if overflowed || records.len() > MAX_TOUCH_RECORDS {
        return Err(TouchForceIssue::QueueOverflow);
    }
    if !records.iter().any(|record| record.force_missing) {
        return Ok(());
    }
    if events.len() > MAX_FRAME_EVENTS {
        return Err(TouchForceIssue::FrameTooLarge);
    }
    if !touch_records_match(events, records) {
        return Err(TouchForceIssue::EventMismatch);
    }

    let mut issue = None;
    for (event, record) in events
        .iter_mut()
        .filter(|event| matches!(event, Event::Touch { .. }))
        .zip(records)
    {
        let Event::Touch { force, .. } = event else {
            unreachable!("the iterator contains only touch events");
        };
        if record.force_missing {
            match *force {
                Some(value) if value.is_nan() => *force = None,
                None => {}
                Some(_) => {
                    *force = Some(f32::NAN);
                    issue = Some(TouchForceIssue::ForceMismatch);
                }
            }
        }
    }
    issue.map_or(Ok(()), Err)
}

fn touch_signature(event: &Event) -> Option<(TouchDeviceId, TouchId, TouchPhase, Pos2)> {
    match event {
        Event::Touch {
            device_id,
            id,
            phase,
            pos,
            ..
        } => Some((*device_id, *id, *phase, *pos)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Modifiers, PointerButton, pos2};

    fn touch(id: u64, phase: TouchPhase, position: Pos2, force: Option<f32>) -> Event {
        Event::Touch {
            device_id: TouchDeviceId(0),
            id: TouchId(id),
            phase,
            pos: position,
            force,
        }
    }

    fn record(id: u64, phase: TouchPhase, position: Pos2, force_missing: bool) -> TouchForceRecord {
        TouchForceRecord {
            device: TouchDeviceId(0),
            id: TouchId(id),
            phase,
            position,
            force_missing,
        }
    }

    #[test]
    fn only_a_proven_missing_force_is_made_optional() {
        let p = pos2(20.0, 30.0);
        let mut events = [
            Event::PointerMoved(p),
            touch(1, TouchPhase::Start, p, Some(f32::NAN)),
            touch(1, TouchPhase::Move, p, Some(0.7)),
            touch(1, TouchPhase::End, p, Some(f32::NAN)),
        ];
        let records = [
            record(1, TouchPhase::Start, p, true),
            record(1, TouchPhase::Move, p, false),
            record(1, TouchPhase::End, p, false),
        ];
        assert_eq!(
            normalize_missing_force(&mut events, &records, false),
            Ok(())
        );
        assert!(matches!(events[1], Event::Touch { force: None, .. }));
        assert!(matches!(
            events[2],
            Event::Touch {
                force: Some(0.7),
                ..
            }
        ));
        assert!(matches!(events[3], Event::Touch { force: Some(value), .. } if value.is_nan()));
    }

    #[test]
    fn multiple_contacts_keep_each_force_attached_to_its_own_event() {
        let first = pos2(20.0, 30.0);
        let second = pos2(40.0, 50.0);
        let mut events = [
            touch(1, TouchPhase::Start, first, Some(f32::NAN)),
            touch(2, TouchPhase::Start, second, Some(0.7)),
            touch(1, TouchPhase::Move, first, Some(f32::NAN)),
            touch(2, TouchPhase::End, second, Some(f32::NAN)),
        ];
        let records = [
            record(1, TouchPhase::Start, first, true),
            record(2, TouchPhase::Start, second, false),
            record(1, TouchPhase::Move, first, false),
            record(2, TouchPhase::End, second, true),
        ];
        assert_eq!(
            normalize_missing_force(&mut events, &records, false),
            Ok(())
        );
        assert!(matches!(events[0], Event::Touch { force: None, .. }));
        assert!(matches!(
            events[1],
            Event::Touch {
                force: Some(0.7),
                ..
            }
        ));
        assert!(matches!(events[2], Event::Touch { force: Some(value), .. } if value.is_nan()));
        assert!(matches!(events[3], Event::Touch { force: None, .. }));
    }

    #[test]
    fn mismatches_and_limits_never_convert_an_unverified_force() {
        let p = pos2(20.0, 30.0);
        let wrong = [record(2, TouchPhase::Start, p, true)];
        let mut events = [touch(1, TouchPhase::Start, p, Some(f32::NAN))];
        assert_eq!(
            normalize_missing_force(&mut events, &wrong, false),
            Err(TouchForceIssue::EventMismatch)
        );
        assert!(matches!(events[0], Event::Touch { force: Some(value), .. } if value.is_nan()));
        assert_eq!(
            normalize_missing_force(&mut events, &[], true),
            Err(TouchForceIssue::QueueOverflow)
        );
        let mut wrong_device = record(1, TouchPhase::Start, p, true);
        wrong_device.device = TouchDeviceId(1);
        assert_eq!(
            normalize_missing_force(&mut events, &[wrong_device], false),
            Err(TouchForceIssue::EventMismatch)
        );
        assert_eq!(
            normalize_missing_force(
                &mut events,
                &[
                    record(1, TouchPhase::Start, p, true),
                    record(2, TouchPhase::Start, p, true),
                ],
                false,
            ),
            Err(TouchForceIssue::EventMismatch)
        );
        let excessive = vec![record(1, TouchPhase::Start, p, true); MAX_TOUCH_RECORDS + 1];
        assert_eq!(
            normalize_missing_force(&mut events, &excessive, false),
            Err(TouchForceIssue::QueueOverflow)
        );
        assert!(matches!(events[0], Event::Touch { force: Some(value), .. } if value.is_nan()));
        let mut large = vec![Event::PointerMoved(p); MAX_FRAME_EVENTS];
        large.push(touch(1, TouchPhase::Start, p, Some(f32::NAN)));
        assert_eq!(
            normalize_missing_force(&mut large, &[record(1, TouchPhase::Start, p, true)], false),
            Err(TouchForceIssue::FrameTooLarge)
        );
    }

    #[test]
    fn a_missing_force_with_a_contradictory_value_is_rejected() {
        let p = pos2(20.0, 30.0);
        let mut events = [touch(1, TouchPhase::Start, p, Some(0.5))];
        assert_eq!(
            normalize_missing_force(&mut events, &[record(1, TouchPhase::Start, p, true)], false),
            Err(TouchForceIssue::ForceMismatch)
        );
        assert!(matches!(events[0], Event::Touch { force: Some(value), .. } if value.is_nan()));
    }

    #[test]
    fn unmatched_invalid_force_is_never_treated_as_absent() {
        let p = pos2(20.0, 30.0);
        let mut events = [touch(1, TouchPhase::Start, p, Some(f32::NAN))];
        assert_eq!(normalize_missing_force(&mut events, &[], false), Ok(()));
        assert!(matches!(events[0], Event::Touch { force: Some(value), .. } if value.is_nan()));
    }

    fn button(position: Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos: position,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }
    }

    #[test]
    fn touch_pointer_metadata_is_matched_and_consumed_per_frame_and_viewport() {
        let ctx = Context::default();
        let p = pos2(20.0, 30.0);
        let events = [Event::PointerMoved(p), button(p, true)];
        let records = [TouchPointerRecord {
            phase: PointerPhase::Press(PointerButton::Primary),
            position: p,
            is_touch: true,
        }];
        assert_eq!(
            publish_pointer_frame(&ctx, ViewportId::ROOT, &events, &records, false, &[], false),
            None
        );
        let wrong_viewport = take_pointer_frame(&ctx, ViewportId::from_hash_of("other"), &events);
        assert!(wrong_viewport.pointers.is_empty());
        assert_eq!(
            peek_pointer_frame(&ctx, ViewportId::ROOT, &events).pointers,
            [false, true]
        );
        let matched = take_pointer_frame(&ctx, ViewportId::ROOT, &events);
        assert_eq!(matched.pointers, [false, true]);
        assert_eq!(matched.issue, None);
        assert!(
            take_pointer_frame(&ctx, ViewportId::ROOT, &events)
                .pointers
                .is_empty()
        );
    }

    #[test]
    fn ambiguous_touch_pointer_frames_suppress_buttons_and_report_the_problem() {
        let ctx = Context::default();
        let p = pos2(20.0, 30.0);
        let records = [TouchPointerRecord {
            phase: PointerPhase::Press(PointerButton::Primary),
            position: p,
            is_touch: true,
        }];
        let events = [button(p, true), button(p, false)];
        assert_eq!(
            publish_pointer_frame(&ctx, ViewportId::ROOT, &events, &records, false, &[], false),
            Some(TouchPointerIssue::EventMismatch)
        );
        let frame = take_pointer_frame(&ctx, ViewportId::ROOT, &events);
        assert!(frame.pointers.is_empty());
        assert_eq!(frame.issue, Some(TouchPointerIssue::EventMismatch));

        assert_eq!(
            publish_pointer_frame(
                &ctx,
                ViewportId::ROOT,
                &events[..1],
                &records,
                false,
                &[],
                false,
            ),
            None
        );
        let rewritten = [button(p + egui::vec2(1.0, 0.0), true)];
        let frame = take_pointer_frame(&ctx, ViewportId::ROOT, &rewritten);
        assert!(frame.pointers.is_empty());
        assert_eq!(frame.issue, Some(TouchPointerIssue::EventMismatch));

        assert_eq!(
            publish_pointer_frame(&ctx, ViewportId::ROOT, &events, &[], true, &[], false),
            Some(TouchPointerIssue::QueueOverflow)
        );
        let frame = take_pointer_frame(&ctx, ViewportId::ROOT, &events);
        assert!(frame.pointers.is_empty());
        assert_eq!(frame.issue, Some(TouchPointerIssue::QueueOverflow));
    }

    #[test]
    fn touch_pointer_limits_and_stale_frames_fail_closed_without_affecting_unrelated_input() {
        let ctx = Context::default();
        let p = pos2(20.0, 30.0);
        let record = TouchPointerRecord {
            phase: PointerPhase::Press(PointerButton::Primary),
            position: p,
            is_touch: true,
        };
        let mut events = vec![Event::PointerMoved(p); MAX_FRAME_EVENTS];
        events.push(button(p, true));
        assert_eq!(
            publish_pointer_frame(&ctx, ViewportId::ROOT, &events, &[], false, &[], false),
            None
        );
        assert!(
            take_pointer_frame(&ctx, ViewportId::ROOT, &events)
                .pointers
                .is_empty()
        );
        assert_eq!(
            publish_pointer_frame(
                &ctx,
                ViewportId::ROOT,
                &events,
                &[record],
                false,
                &[],
                false
            ),
            Some(TouchPointerIssue::FrameTooLarge)
        );
        let large = take_pointer_frame(&ctx, ViewportId::ROOT, &events);
        assert_eq!(large.issue, Some(TouchPointerIssue::FrameTooLarge));
        assert!(large.pointers.is_empty());
        let excess = vec![record; MAX_TOUCH_RECORDS + 1];
        assert_eq!(
            publish_pointer_frame(
                &ctx,
                ViewportId::ROOT,
                &events[..1],
                &excess,
                false,
                &[],
                false,
            ),
            Some(TouchPointerIssue::QueueOverflow)
        );
        let _ = take_pointer_frame(&ctx, ViewportId::ROOT, &events[..1]);

        let events = [button(p, true)];
        assert_eq!(
            publish_pointer_frame(
                &ctx,
                ViewportId::ROOT,
                &events,
                &[record],
                false,
                &[],
                false
            ),
            None
        );
        let mut output = ctx.run_pass(egui::RawInput::default(), |_| {});
        output.textures_delta.clear();
        let stale = take_pointer_frame(&ctx, ViewportId::ROOT, &events);
        assert!(stale.pointers.is_empty());
        assert_eq!(stale.issue, Some(TouchPointerIssue::EventMismatch));
    }

    #[test]
    fn queued_synthetic_touch_buttons_are_correlated_with_captured_touches() {
        let ctx = Context::default();
        let touch_pos = pos2(20.0, 30.0);
        let pointer_pos = pos2(40.0, 50.0);
        let start = [
            button(pointer_pos, true),
            touch(1, TouchPhase::Start, touch_pos, Some(f32::NAN)),
            button(touch_pos, false),
        ];
        let release = [TouchPointerRecord {
            phase: PointerPhase::Release(PointerButton::Primary),
            position: touch_pos,
            is_touch: true,
        }];
        let start_touch = [record(1, TouchPhase::Start, touch_pos, true)];
        assert_eq!(
            publish_pointer_frame(
                &ctx,
                ViewportId::ROOT,
                &start,
                &release,
                false,
                &start_touch,
                false,
            ),
            None
        );
        let frame = take_pointer_frame(&ctx, ViewportId::ROOT, &start);
        assert_eq!(frame.pointers, [true, false, true]);
        assert_eq!(frame.issue, None);

        let end = [
            button(pointer_pos, false),
            Event::PointerGone,
            touch(1, TouchPhase::End, touch_pos, None),
        ];
        let end_touch = [record(1, TouchPhase::End, touch_pos, false)];
        assert_eq!(
            publish_pointer_frame(&ctx, ViewportId::ROOT, &end, &[], false, &end_touch, false),
            None
        );
        let frame = take_pointer_frame(&ctx, ViewportId::ROOT, &end);
        assert_eq!(frame.pointers, [true, true, false]);
        assert_eq!(frame.issue, None);
    }

    #[test]
    fn mixed_mouse_and_touch_buttons_remain_distinct_or_fail_when_ambiguous() {
        let ctx = Context::default();
        let touch_pos = pos2(20.0, 30.0);
        let synthetic_pos = pos2(40.0, 50.0);
        let mouse_pos = pos2(60.0, 70.0);
        let touches = [record(1, TouchPhase::Start, touch_pos, false)];
        let records = [
            TouchPointerRecord {
                phase: PointerPhase::Press(PointerButton::Primary),
                position: mouse_pos,
                is_touch: false,
            },
            TouchPointerRecord {
                phase: PointerPhase::Release(PointerButton::Primary),
                position: touch_pos,
                is_touch: true,
            },
        ];
        let events = [
            button(synthetic_pos, true),
            touch(1, TouchPhase::Start, touch_pos, None),
            button(mouse_pos, true),
            button(touch_pos, false),
        ];
        assert_eq!(
            publish_pointer_frame(
                &ctx,
                ViewportId::ROOT,
                &events,
                &records,
                false,
                &touches,
                false,
            ),
            None
        );
        let frame = take_pointer_frame(&ctx, ViewportId::ROOT, &events);
        assert_eq!(frame.pointers, [true, false, false, true]);
        assert_eq!(frame.issue, None);

        let mouse_only = [button(mouse_pos, true)];
        assert_eq!(
            publish_pointer_frame(
                &ctx,
                ViewportId::ROOT,
                &mouse_only,
                &records[..1],
                false,
                &[],
                false,
            ),
            None
        );
        assert!(
            take_pointer_frame(&ctx, ViewportId::ROOT, &mouse_only)
                .pointers
                .is_empty()
        );

        let ambiguous = [
            button(mouse_pos, true),
            touch(1, TouchPhase::Start, touch_pos, None),
            button(mouse_pos, true),
            button(touch_pos, false),
        ];
        assert_eq!(
            publish_pointer_frame(
                &ctx,
                ViewportId::ROOT,
                &ambiguous,
                &records,
                false,
                &touches,
                false,
            ),
            Some(TouchPointerIssue::EventMismatch)
        );
        let frame = take_pointer_frame(&ctx, ViewportId::ROOT, &ambiguous);
        assert!(frame.pointers.is_empty());
        assert_eq!(frame.issue, Some(TouchPointerIssue::EventMismatch));
    }

    #[test]
    fn synthetic_buttons_require_matching_touch_records_and_bounded_queues() {
        let ctx = Context::default();
        let p = pos2(20.0, 30.0);
        let events = [button(p, true), touch(1, TouchPhase::Start, p, None)];
        let wrong = [record(2, TouchPhase::Start, p, false)];
        assert_eq!(
            publish_pointer_frame(&ctx, ViewportId::ROOT, &events, &[], false, &wrong, false),
            Some(TouchPointerIssue::EventMismatch)
        );
        let _ = take_pointer_frame(&ctx, ViewportId::ROOT, &events);
        assert_eq!(
            publish_pointer_frame(&ctx, ViewportId::ROOT, &events, &[], false, &[], true),
            Some(TouchPointerIssue::QueueOverflow)
        );
        let frame = take_pointer_frame(&ctx, ViewportId::ROOT, &events);
        assert!(frame.pointers.is_empty());
        assert_eq!(frame.issue, Some(TouchPointerIssue::QueueOverflow));

        let mut large = vec![Event::PointerMoved(p); MAX_FRAME_EVENTS];
        large.extend(events);
        assert_eq!(
            publish_pointer_frame(
                &ctx,
                ViewportId::ROOT,
                &large,
                &[],
                false,
                &[record(1, TouchPhase::Start, p, false)],
                false,
            ),
            Some(TouchPointerIssue::FrameTooLarge)
        );
        let frame = take_pointer_frame(&ctx, ViewportId::ROOT, &large);
        assert!(frame.pointers.is_empty());
        assert_eq!(frame.issue, Some(TouchPointerIssue::FrameTooLarge));
    }

    #[test]
    fn touch_frame_rewrites_cannot_invalidate_a_synthetic_button_proof() {
        let ctx = Context::default();
        let p = pos2(20.0, 30.0);
        let events = [
            button(p, false),
            Event::PointerGone,
            touch(1, TouchPhase::End, p, Some(f32::NAN)),
        ];
        let records = [record(1, TouchPhase::End, p, false)];
        let publish = || {
            assert_eq!(
                publish_pointer_frame(&ctx, ViewportId::ROOT, &events, &[], false, &records, false),
                None
            );
        };
        publish();
        let force_changed = [
            button(p, false),
            Event::PointerGone,
            touch(1, TouchPhase::End, p, None),
        ];
        assert_eq!(
            take_pointer_frame(&ctx, ViewportId::ROOT, &force_changed).pointers,
            [true, true, false]
        );

        publish();
        let unrelated_input = [
            Event::PointerMoved(p),
            button(p, false),
            Event::PointerGone,
            touch(1, TouchPhase::End, p, None),
            Event::PointerMoved(p),
        ];
        assert_eq!(
            take_pointer_frame(&ctx, ViewportId::ROOT, &unrelated_input).pointers,
            [false, true, true, false, false]
        );

        publish();
        let changed_touch = [
            button(p, false),
            Event::PointerGone,
            touch(2, TouchPhase::End, p, None),
        ];
        let frame = take_pointer_frame(&ctx, ViewportId::ROOT, &changed_touch);
        assert_eq!(frame.issue, Some(TouchPointerIssue::EventMismatch));
        assert!(frame.pointers.is_empty());

        publish();
        let changed_order = [
            button(p, false),
            Event::PointerMoved(p),
            Event::PointerGone,
            touch(1, TouchPhase::End, p, None),
        ];
        let frame = take_pointer_frame(&ctx, ViewportId::ROOT, &changed_order);
        assert_eq!(frame.issue, Some(TouchPointerIssue::EventMismatch));
        assert!(frame.pointers.is_empty());

        publish();
        let mut oversized = vec![Event::PointerMoved(p); MAX_FRAME_EVENTS];
        oversized.extend(events);
        let frame = take_pointer_frame(&ctx, ViewportId::ROOT, &oversized);
        assert_eq!(frame.issue, Some(TouchPointerIssue::FrameTooLarge));
        assert!(frame.pointers.is_empty());
    }
}
