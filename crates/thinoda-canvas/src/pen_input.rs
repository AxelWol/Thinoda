//! Frame-scoped pen metadata for hosts whose egui pointer events omit pressure.

use std::sync::Arc;

use egui::{Context, Event, Id, PointerButton, Pos2, ViewportId};

use crate::touch_input::TouchPointerFrame;

/// Maximum number of browser pointer records retained between egui frames.
pub const MAX_POINTER_RECORDS: usize = 4_096;
const MAX_FRAME_EVENTS: usize = 8_192;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PenSample {
    pub id: i32,
    pub pressure: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerPhase {
    Press(PointerButton),
    Move,
    Release(PointerButton),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointerRecord {
    pub phase: PointerPhase,
    pub position: Pos2,
    /// `None` means the corresponding browser pointer was not a pen.
    pub pen: Option<PenSample>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PenInputIssue {
    QueueOverflow,
    FrameTooLarge,
    PointerMismatch,
}

/// Correlates a bounded browser event stream with the exact egui frame.
#[derive(Default)]
pub struct PenInputMatcher {
    active_pen: Option<i32>,
    quarantine: Option<Quarantine>,
}

#[derive(Clone, Copy)]
enum Quarantine {
    Pen(i32),
    Unknown,
}

impl Quarantine {
    fn id(self) -> Option<i32> {
        match self {
            Self::Pen(id) => Some(id),
            Self::Unknown => None,
        }
    }

    fn ended(
        self,
        events: &[Event],
        records: &[PointerRecord],
        canceled: Option<i32>,
        touch: &TouchPointerFrame,
    ) -> bool {
        canceled.is_some_and(|id| self.id().is_none_or(|active| active == id))
            || records.iter().any(|record| {
                matches!(record.phase, PointerPhase::Release(PointerButton::Primary))
                    && record
                        .pen
                        .is_some_and(|pen| self.id().is_none_or(|active| active == pen.id))
            })
            || events.iter().enumerate().any(|(index, event)| {
                matches!(event, Event::WindowFocused(false))
                    || (matches!(event, Event::PointerGone)
                        && touch.issue.is_none()
                        && !touch.pointers.get(index).copied().unwrap_or(false))
            })
    }
}

pub struct MatchedPenFrame {
    samples: Vec<Option<PenSample>>,
    canceled: Option<i32>,
    fallback_cancel: Option<i32>,
    pub issue: Option<PenInputIssue>,
}

impl PenInputMatcher {
    pub fn match_frame(
        &mut self,
        events: &[Event],
        records: &[PointerRecord],
        canceled: Option<i32>,
        overflowed: bool,
    ) -> MatchedPenFrame {
        self.match_frame_with_touch(
            events,
            records,
            canceled,
            overflowed,
            &TouchPointerFrame::default(),
        )
    }

    pub fn match_frame_with_touch(
        &mut self,
        events: &[Event],
        records: &[PointerRecord],
        canceled: Option<i32>,
        overflowed: bool,
        touch: &TouchPointerFrame,
    ) -> MatchedPenFrame {
        if let Some(quarantine) = self.quarantine {
            if events.is_empty() && records.is_empty() && canceled.is_none() && !overflowed {
                return MatchedPenFrame::empty();
            }
            if quarantine.ended(events, records, canceled, touch) {
                self.quarantine = None;
            }
            return MatchedPenFrame {
                samples: Vec::new(),
                canceled: quarantine.id(),
                fallback_cancel: quarantine.id(),
                issue: Some(if overflowed || records.len() > MAX_POINTER_RECORDS {
                    PenInputIssue::QueueOverflow
                } else if events.len() > MAX_FRAME_EVENTS {
                    PenInputIssue::FrameTooLarge
                } else {
                    PenInputIssue::PointerMismatch
                }),
            };
        }
        let previous_pen = self.active_pen;
        let first_pen = records
            .iter()
            .find_map(|record| record.pen.map(|pen| pen.id));
        if previous_pen.is_none() && first_pen.is_none() && canceled.is_none() && !overflowed {
            return MatchedPenFrame::empty();
        }

        let mut matched = vec![None; events.len().min(MAX_FRAME_EVENTS)];
        let issue = if overflowed || records.len() > MAX_POINTER_RECORDS {
            Some(PenInputIssue::QueueOverflow)
        } else if events.len() > MAX_FRAME_EVENTS {
            Some(PenInputIssue::FrameTooLarge)
        } else if touch.issue.is_some() {
            Some(PenInputIssue::PointerMismatch)
        } else {
            let mut records = records.iter();
            let mut capturing = previous_pen;
            let mut mismatch = false;
            for (index, event) in events.iter().enumerate() {
                let Some(signature) = pointer_signature(event) else {
                    continue;
                };
                if touch
                    .synthetic_pointers
                    .get(index)
                    .copied()
                    .unwrap_or(false)
                {
                    continue;
                }
                if touch.pointers.get(index).copied().unwrap_or(false) {
                    if capturing.is_some()
                        && !records.next().is_some_and(|record| {
                            (record.phase, record.position) == signature && record.pen.is_none()
                        })
                    {
                        mismatch = true;
                        break;
                    }
                    continue;
                }
                let Some(record) = records.next() else {
                    mismatch = true;
                    break;
                };
                if (record.phase, record.position) != signature {
                    mismatch = true;
                    break;
                }
                matched[index] = Some(*record);
                match (record.phase, record.pen) {
                    (PointerPhase::Press(PointerButton::Primary), Some(pen))
                        if capturing.is_none() =>
                    {
                        capturing = Some(pen.id);
                    }
                    (PointerPhase::Release(PointerButton::Primary), Some(pen))
                        if capturing == Some(pen.id) =>
                    {
                        capturing = None;
                    }
                    _ => {}
                }
            }
            (mismatch || records.next().is_some()).then_some(PenInputIssue::PointerMismatch)
        };
        if let Some(issue) = issue {
            self.active_pen = None;
            let quarantine = previous_pen
                .or(first_pen)
                .map_or(Quarantine::Unknown, Quarantine::Pen);
            if !quarantine.ended(events, records, canceled, touch) {
                self.quarantine = Some(quarantine);
            }
            return MatchedPenFrame {
                samples: Vec::new(),
                canceled: previous_pen.or(first_pen),
                fallback_cancel: previous_pen.or(first_pen),
                issue: Some(issue),
            };
        }

        if events
            .iter()
            .any(|event| matches!(event, Event::Touch { .. }))
            && !touch.touches_verified
        {
            self.active_pen = None;
            return MatchedPenFrame {
                samples: Vec::new(),
                canceled: previous_pen,
                fallback_cancel: previous_pen,
                issue: None,
            };
        }

        let mut samples = vec![None; events.len()];
        let mut interrupted = None;
        for (index, event) in events.iter().enumerate() {
            if let Some(record) = matched[index] {
                let phase = record.phase;
                samples[index] = record.pen;
                let relevant = matches!(
                    phase,
                    PointerPhase::Move
                        | PointerPhase::Press(PointerButton::Primary)
                        | PointerPhase::Release(PointerButton::Primary)
                );
                if relevant
                    && let Some(active) = self.active_pen
                    && record.pen.is_none_or(|pen| pen.id != active)
                {
                    interrupted.get_or_insert(active);
                    self.active_pen = None;
                }
                match (phase, record.pen) {
                    (PointerPhase::Press(PointerButton::Primary), Some(pen))
                        if interrupted.is_none() =>
                    {
                        self.active_pen = Some(pen.id);
                    }
                    (PointerPhase::Release(PointerButton::Primary), Some(pen))
                        if self.active_pen == Some(pen.id) =>
                    {
                        self.active_pen = None;
                    }
                    _ => {}
                }
            } else if matches!(event, Event::WindowFocused(false))
                || (matches!(event, Event::PointerGone)
                    && !touch.pointers.get(index).copied().unwrap_or(false))
            {
                self.active_pen = None;
            }
        }
        if canceled.is_some() && canceled == self.active_pen {
            interrupted = canceled;
            self.active_pen = None;
        }
        MatchedPenFrame {
            samples,
            canceled: interrupted,
            fallback_cancel: previous_pen
                .or(interrupted)
                .or(self.active_pen)
                .or(first_pen),
            issue: None,
        }
    }
}

impl MatchedPenFrame {
    fn empty() -> Self {
        Self {
            samples: Vec::new(),
            canceled: None,
            fallback_cancel: None,
            issue: None,
        }
    }
}

fn pointer_signature(event: &Event) -> Option<(PointerPhase, Pos2)> {
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
        Event::PointerMoved(pos) => Some((PointerPhase::Move, *pos)),
        _ => None,
    }
}

fn relevant(event: &Event) -> bool {
    pointer_signature(event).is_some()
        || matches!(
            event,
            Event::PointerGone | Event::Touch { .. } | Event::WindowFocused(_)
        )
        || matches!(
            event,
            Event::Key {
                key: egui::Key::Escape,
                ..
            }
        )
}

#[derive(Clone)]
struct StoredFrame {
    number: u64,
    events: Vec<(Event, Option<PenSample>)>,
    canceled: Option<i32>,
    fallback_cancel: Option<i32>,
    issue: Option<PenInputIssue>,
}

fn slot(viewport: ViewportId) -> Id {
    Id::unique("thinoda_pen_input").with(viewport)
}

/// Publish once after host input preparation and before egui starts the frame.
/// A later input-hook rewrite or another viewport cannot reuse the metadata.
pub fn publish(ctx: &Context, viewport: ViewportId, events: &[Event], frame: MatchedPenFrame) {
    let id = slot(viewport);
    let number = ctx.cumulative_frame_nr_for(viewport);
    ctx.data_mut(|data| {
        data.remove::<Arc<StoredFrame>>(id);
        if frame.samples.iter().all(Option::is_none)
            && frame.canceled.is_none()
            && frame.issue.is_none()
        {
            return;
        }
        let events = if events.len() <= MAX_FRAME_EVENTS {
            events
                .iter()
                .enumerate()
                .filter(|(_, event)| relevant(event))
                .map(|(index, event)| (event.clone(), frame.samples.get(index).copied().flatten()))
                .collect()
        } else {
            Vec::new()
        };
        data.insert_temp(
            id,
            Arc::new(StoredFrame {
                number,
                events,
                canceled: frame.canceled,
                fallback_cancel: frame.fallback_cancel,
                issue: frame.issue,
            }),
        );
    });
}

#[derive(Default)]
pub(crate) struct PenFrameData {
    pub samples: Vec<Option<PenSample>>,
    pub canceled: Option<i32>,
    pub issue: Option<PenInputIssue>,
    pub touch_pointers: Vec<bool>,
    pub touch_issue: Option<crate::touch_input::TouchPointerIssue>,
}

impl PenFrameData {
    pub fn sample(&self, index: usize) -> Option<PenSample> {
        self.samples.get(index).copied().flatten()
    }

    pub fn touch_pointer(&self, index: usize) -> bool {
        // An ambiguous frame must not attribute any button to mouse or pen.
        self.touch_issue.is_some() || self.touch_pointers.get(index).copied().unwrap_or(false)
    }
}

pub(crate) fn take(ctx: &Context, viewport: ViewportId, events: &[Event]) -> PenFrameData {
    read_frame(ctx, viewport, events, true)
}

pub(crate) fn peek(ctx: &Context, viewport: ViewportId, events: &[Event]) -> PenFrameData {
    read_frame(ctx, viewport, events, false)
}

fn read_frame(
    ctx: &Context,
    viewport: ViewportId,
    events: &[Event],
    consume: bool,
) -> PenFrameData {
    let id = slot(viewport);
    let stored = ctx.data_mut(|data| {
        let frame = data.get_temp::<Arc<StoredFrame>>(id);
        if consume {
            data.remove::<Arc<StoredFrame>>(id);
        }
        frame
    });
    let Some(stored) = stored else {
        return PenFrameData::default();
    };
    if stored.number != ctx.cumulative_frame_nr_for(viewport)
        || !events
            .iter()
            .filter(|event| relevant(event))
            .eq(stored.events.iter().map(|(event, _)| event))
    {
        return PenFrameData {
            samples: Vec::new(),
            canceled: stored.fallback_cancel,
            issue: stored.issue.or(Some(PenInputIssue::PointerMismatch)),
            ..PenFrameData::default()
        };
    }
    let mut samples = vec![None; events.len()];
    let mut saved = stored.events.iter();
    for (index, event) in events.iter().enumerate() {
        if relevant(event) {
            samples[index] = saved.next().and_then(|(_, sample)| *sample);
        }
    }
    PenFrameData {
        samples,
        canceled: stored.canceled,
        issue: stored.issue,
        ..PenFrameData::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::touch_input::{TouchForceRecord, TouchPointerRecord, publish_pointer_frame};
    use egui::{Modifiers, pos2};

    fn button(position: Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos: position,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }
    }

    fn record(phase: PointerPhase, position: Pos2, pen: Option<(i32, f32)>) -> PointerRecord {
        PointerRecord {
            phase,
            position,
            pen: pen.map(|(id, pressure)| PenSample { id, pressure }),
        }
    }

    #[test]
    fn pressure_is_matched_in_order_and_consumed_once_per_viewport_and_frame() {
        let ctx = Context::default();
        let position = pos2(20.0, 30.0);
        let events = vec![button(position, true), Event::PointerMoved(position)];
        let records = [
            record(
                PointerPhase::Press(PointerButton::Primary),
                position,
                Some((7, 0.2)),
            ),
            record(PointerPhase::Move, position, Some((7, 0.6))),
        ];
        let mut matcher = PenInputMatcher::default();
        let frame = matcher.match_frame(&events, &records, None, false);
        assert_eq!(frame.issue, None);
        publish(&ctx, ViewportId::ROOT, &events, frame);
        let other = ViewportId::from_hash_of("other");
        assert!(take(&ctx, other, &events).samples.is_empty());
        assert_eq!(
            peek(&ctx, ViewportId::ROOT, &events).sample(0),
            Some(PenSample {
                id: 7,
                pressure: 0.2
            })
        );
        let data = take(&ctx, ViewportId::ROOT, &events);
        assert_eq!(
            data.sample(0),
            Some(PenSample {
                id: 7,
                pressure: 0.2
            })
        );
        assert_eq!(
            data.sample(1),
            Some(PenSample {
                id: 7,
                pressure: 0.6
            })
        );
        assert!(take(&ctx, ViewportId::ROOT, &events).samples.is_empty());
    }

    #[test]
    fn mismatches_interrupt_the_bound_pen_without_reusing_stale_pressure() {
        let position = pos2(20.0, 30.0);
        let mut matcher = PenInputMatcher::default();
        let down = [button(position, true)];
        let start_record = [record(
            PointerPhase::Press(PointerButton::Primary),
            position,
            Some((7, 0.4)),
        )];
        assert_eq!(
            matcher.match_frame(&down, &start_record, None, false).issue,
            None
        );

        let moved = [Event::PointerMoved(position)];
        let wrong = [record(PointerPhase::Move, pos2(21.0, 30.0), Some((7, 0.8)))];
        let frame = matcher.match_frame(&moved, &wrong, None, false);
        assert_eq!(frame.issue, Some(PenInputIssue::PointerMismatch));
        assert_eq!(frame.canceled, Some(7));
        assert!(frame.samples.is_empty());
        let mouse = [record(PointerPhase::Move, position, None)];
        assert!(
            matcher
                .match_frame(&moved, &mouse, None, false)
                .samples
                .is_empty()
        );
    }

    #[test]
    fn an_unmatched_first_pen_and_an_overflow_remain_visible_to_the_host() {
        let ctx = Context::default();
        let position = pos2(20.0, 30.0);
        let down = [button(position, true)];
        let wrong = [record(
            PointerPhase::Press(PointerButton::Primary),
            pos2(21.0, 30.0),
            Some((7, 0.4)),
        )];
        let mut matcher = PenInputMatcher::default();
        let frame = matcher.match_frame(&down, &wrong, None, false);
        assert_eq!(frame.issue, Some(PenInputIssue::PointerMismatch));
        publish(&ctx, ViewportId::ROOT, &down, frame);
        let data = take(&ctx, ViewportId::ROOT, &down);
        assert_eq!(data.canceled, Some(7));
        assert_eq!(data.issue, Some(PenInputIssue::PointerMismatch));

        let frame = matcher.match_frame(&down, &[], None, true);
        assert_eq!(frame.issue, Some(PenInputIssue::QueueOverflow));
        publish(&ctx, ViewportId::ROOT, &down, frame);
        assert_eq!(
            take(&ctx, ViewportId::ROOT, &down).issue,
            Some(PenInputIssue::QueueOverflow)
        );
    }

    #[test]
    fn an_unmatched_press_stays_quarantined_until_the_pen_releases() {
        let p = pos2(20.0, 30.0);
        let down = [button(p, true)];
        let moved = [Event::PointerMoved(p)];
        let records = [
            record(
                PointerPhase::Press(PointerButton::Primary),
                p,
                Some((7, 0.4)),
            ),
            record(PointerPhase::Move, p, None),
        ];
        let mut matcher = PenInputMatcher::default();
        assert_eq!(
            matcher.match_frame(&moved, &records, None, false).issue,
            Some(PenInputIssue::PointerMismatch)
        );
        assert_eq!(
            matcher.match_frame(&down, &[], None, false).issue,
            Some(PenInputIssue::PointerMismatch)
        );
        let up = [button(p, false)];
        let release = [record(
            PointerPhase::Release(PointerButton::Primary),
            p,
            Some((7, 0.0)),
        )];
        assert_eq!(
            matcher.match_frame(&up, &release, None, false).issue,
            Some(PenInputIssue::PointerMismatch)
        );
        assert_eq!(matcher.match_frame(&down, &[], None, false).issue, None);
    }

    #[test]
    fn overflow_without_an_identified_pen_is_quarantined_until_release_or_focus_loss() {
        let p = pos2(20.0, 30.0);
        let down = [button(p, true)];
        let mut matcher = PenInputMatcher::default();
        assert_eq!(
            matcher.match_frame(&down, &[], None, true).issue,
            Some(PenInputIssue::QueueOverflow)
        );
        assert_eq!(
            matcher.match_frame(&down, &[], None, false).issue,
            Some(PenInputIssue::PointerMismatch)
        );
        let up = [button(p, false)];
        let release = [record(
            PointerPhase::Release(PointerButton::Primary),
            p,
            Some((7, 0.0)),
        )];
        assert!(
            matcher
                .match_frame(&up, &release, None, false)
                .issue
                .is_some()
        );
        assert_eq!(matcher.match_frame(&down, &[], None, false).issue, None);

        matcher.match_frame(&down, &[], None, true);
        matcher.match_frame(&[Event::WindowFocused(false)], &[], None, false);
        assert_eq!(matcher.match_frame(&down, &[], None, false).issue, None);
    }

    #[test]
    fn mouse_touch_cancel_and_overflow_never_inherit_pen_pressure() {
        let p = pos2(40.0, 50.0);
        let down = [button(p, true)];
        let pen_down = [record(
            PointerPhase::Press(PointerButton::Primary),
            p,
            Some((9, 0.3)),
        )];
        let moved = [Event::PointerMoved(p)];
        let mouse_move = [record(PointerPhase::Move, p, None)];
        let mut matcher = PenInputMatcher::default();
        matcher.match_frame(&down, &pen_down, None, false);
        let mouse = matcher.match_frame(&moved, &mouse_move, None, false);
        assert_eq!(mouse.canceled, Some(9));
        assert_eq!(mouse.samples, vec![None]);

        matcher.match_frame(&down, &pen_down, None, false);
        let touch = [Event::Touch {
            device_id: egui::TouchDeviceId(1),
            id: egui::TouchId(1),
            phase: egui::TouchPhase::Start,
            pos: p,
            force: Some(0.7),
        }];
        let mixed = matcher.match_frame(&touch, &[], None, false);
        assert_eq!(mixed.canceled, Some(9));
        assert!(mixed.samples.is_empty());

        matcher.match_frame(&down, &pen_down, None, false);
        let cancel = matcher.match_frame(&[], &[], Some(9), false);
        assert_eq!(cancel.canceled, Some(9));
        matcher.match_frame(&down, &pen_down, None, false);
        let overflow = matcher.match_frame(&[], &[], None, true);
        assert_eq!(overflow.issue, Some(PenInputIssue::QueueOverflow));
        assert_eq!(overflow.canceled, Some(9));
    }

    #[test]
    fn proven_touch_packets_do_not_steal_a_correlated_pen() {
        let ctx = Context::default();
        let mut matcher = PenInputMatcher::default();
        let pen_pos = pos2(20.0, 30.0);
        let touch_pos = pos2(90.0, 100.0);
        let down = [button(pen_pos, true)];
        let pen_down = [record(
            PointerPhase::Press(PointerButton::Primary),
            pen_pos,
            Some((7, 0.2)),
        )];
        matcher.match_frame(&down, &pen_down, None, false);

        let touch_down = [button(touch_pos, true)];
        let touch_record = [TouchPointerRecord {
            phase: PointerPhase::Press(PointerButton::Primary),
            position: touch_pos,
            is_touch: true,
        }];
        assert_eq!(
            publish_pointer_frame(
                &ctx,
                ViewportId::ROOT,
                &touch_down,
                &touch_record,
                false,
                &[],
                false,
            ),
            None
        );
        let touch = crate::touch_input::peek_pointer_frame(&ctx, ViewportId::ROOT, &touch_down);
        let frame = matcher.match_frame_with_touch(
            &touch_down,
            &[record(
                PointerPhase::Press(PointerButton::Primary),
                touch_pos,
                None,
            )],
            None,
            false,
            &touch,
        );
        assert_eq!(frame.issue, None);
        assert_eq!(frame.canceled, None);
        assert_eq!(frame.samples, [None]);

        let event = |phase, pos| Event::Touch {
            device_id: egui::TouchDeviceId(0),
            id: egui::TouchId(1),
            phase,
            pos,
            force: Some(0.5),
        };
        let captured = |phase, position| TouchForceRecord {
            device: egui::TouchDeviceId(0),
            id: egui::TouchId(1),
            phase,
            position,
            force_missing: false,
        };
        let start = [
            button(touch_pos, true),
            event(egui::TouchPhase::Start, touch_pos),
        ];
        assert_eq!(
            publish_pointer_frame(
                &ctx,
                ViewportId::ROOT,
                &start,
                &[],
                false,
                &[captured(egui::TouchPhase::Start, touch_pos)],
                false,
            ),
            None
        );
        let touch = crate::touch_input::peek_pointer_frame(&ctx, ViewportId::ROOT, &start);
        let frame = matcher.match_frame_with_touch(&start, &[], None, false, &touch);
        assert_eq!(frame.issue, None);
        assert_eq!(frame.canceled, None);

        let moved_pen = pen_pos + egui::vec2(20.0, 10.0);
        let moved_touch = touch_pos + egui::vec2(3.0, 2.0);
        let mixed = [
            Event::PointerMoved(moved_pen),
            Event::PointerMoved(moved_touch),
            event(egui::TouchPhase::Move, moved_touch),
        ];
        assert_eq!(
            publish_pointer_frame(
                &ctx,
                ViewportId::ROOT,
                &mixed,
                &[],
                false,
                &[captured(egui::TouchPhase::Move, moved_touch)],
                false,
            ),
            None
        );
        let touch = crate::touch_input::peek_pointer_frame(&ctx, ViewportId::ROOT, &mixed);
        assert_eq!(touch.pointers, [false, true, false]);
        let frame = matcher.match_frame_with_touch(
            &mixed,
            &[record(PointerPhase::Move, moved_pen, Some((7, 0.7)))],
            None,
            false,
            &touch,
        );
        assert_eq!(frame.issue, None);
        assert_eq!(frame.canceled, None);
        assert_eq!(
            frame.samples,
            [
                Some(PenSample {
                    id: 7,
                    pressure: 0.7
                }),
                None,
                None
            ]
        );

        let touch_up = [button(moved_touch, false)];
        let touch_record = [TouchPointerRecord {
            phase: PointerPhase::Release(PointerButton::Primary),
            position: moved_touch,
            is_touch: true,
        }];
        assert_eq!(
            publish_pointer_frame(
                &ctx,
                ViewportId::ROOT,
                &touch_up,
                &touch_record,
                false,
                &[],
                false,
            ),
            None
        );
        let touch = crate::touch_input::peek_pointer_frame(&ctx, ViewportId::ROOT, &touch_up);
        let frame = matcher.match_frame_with_touch(
            &touch_up,
            &[record(
                PointerPhase::Release(PointerButton::Primary),
                moved_touch,
                None,
            )],
            None,
            false,
            &touch,
        );
        assert_eq!(frame.issue, None);
        assert_eq!(frame.canceled, None);

        let end = [
            button(moved_touch, false),
            Event::PointerGone,
            event(egui::TouchPhase::End, moved_touch),
        ];
        assert_eq!(
            publish_pointer_frame(
                &ctx,
                ViewportId::ROOT,
                &end,
                &[],
                false,
                &[captured(egui::TouchPhase::End, moved_touch)],
                false,
            ),
            None
        );
        let touch = crate::touch_input::peek_pointer_frame(&ctx, ViewportId::ROOT, &end);
        let frame = matcher.match_frame_with_touch(&end, &[], None, false, &touch);
        assert_eq!(frame.issue, None);
        assert_eq!(frame.canceled, None);

        let up = [button(moved_pen, false)];
        let pen_up = [record(
            PointerPhase::Release(PointerButton::Primary),
            moved_pen,
            Some((7, 0.0)),
        )];
        let frame = matcher.match_frame(&up, &pen_up, None, false);
        assert_eq!(frame.issue, None);
        assert_eq!(frame.canceled, None);
        assert_eq!(
            frame.samples,
            [Some(PenSample {
                id: 7,
                pressure: 0.0
            })]
        );
    }

    #[test]
    fn pen_move_precedes_nonprimary_touch_move_with_different_positions() {
        let ctx = Context::default();
        let mut matcher = PenInputMatcher::default();
        let pen_start = pos2(20.0, 30.0);
        matcher.match_frame(
            &[button(pen_start, true)],
            &[record(
                PointerPhase::Press(PointerButton::Primary),
                pen_start,
                Some((7, 0.2)),
            )],
            None,
            false,
        );

        let pen_move = pos2(25.0, 35.0);
        let primary_touch = pos2(90.0, 100.0);
        let changed_touch = pos2(120.0, 130.0);
        let events = [
            Event::PointerMoved(pen_move),
            Event::PointerMoved(primary_touch),
            Event::Touch {
                device_id: egui::TouchDeviceId(0),
                id: egui::TouchId(2),
                phase: egui::TouchPhase::Move,
                pos: changed_touch,
                force: Some(0.5),
            },
        ];
        let touches = [TouchForceRecord {
            device: egui::TouchDeviceId(0),
            id: egui::TouchId(2),
            phase: egui::TouchPhase::Move,
            position: changed_touch,
            force_missing: false,
        }];
        assert_eq!(
            publish_pointer_frame(&ctx, ViewportId::ROOT, &events, &[], false, &touches, false),
            None
        );
        let touch = crate::touch_input::peek_pointer_frame(&ctx, ViewportId::ROOT, &events);
        assert_eq!(touch.synthetic_pointers, [false, true, false]);
        let frame = matcher.match_frame_with_touch(
            &events,
            &[record(PointerPhase::Move, pen_move, Some((7, 0.7)))],
            None,
            false,
            &touch,
        );
        assert_eq!(frame.issue, None);
        assert_eq!(frame.canceled, None);
        assert_eq!(
            frame.samples,
            [
                Some(PenSample {
                    id: 7,
                    pressure: 0.7
                }),
                None,
                None
            ]
        );
    }

    #[test]
    fn ambiguous_touch_move_without_eframe_pointer_fails_closed() {
        let ctx = Context::default();
        let mut matcher = PenInputMatcher::default();
        let pen_start = pos2(20.0, 30.0);
        matcher.match_frame(
            &[button(pen_start, true)],
            &[record(
                PointerPhase::Press(PointerButton::Primary),
                pen_start,
                Some((7, 0.2)),
            )],
            None,
            false,
        );

        let pen_move = pos2(25.0, 35.0);
        let touch_move = pos2(90.0, 100.0);
        let events = [
            Event::PointerMoved(pen_move),
            Event::Touch {
                device_id: egui::TouchDeviceId(0),
                id: egui::TouchId(1),
                phase: egui::TouchPhase::Move,
                pos: touch_move,
                force: Some(0.5),
            },
        ];
        let touches = [TouchForceRecord {
            device: egui::TouchDeviceId(0),
            id: egui::TouchId(1),
            phase: egui::TouchPhase::Move,
            position: touch_move,
            force_missing: false,
        }];
        assert_eq!(
            publish_pointer_frame(&ctx, ViewportId::ROOT, &events, &[], false, &touches, false),
            None
        );
        let touch = crate::touch_input::peek_pointer_frame(&ctx, ViewportId::ROOT, &events);
        let frame = matcher.match_frame_with_touch(
            &events,
            &[record(PointerPhase::Move, pen_move, Some((7, 0.7)))],
            None,
            false,
            &touch,
        );
        assert_eq!(frame.issue, Some(PenInputIssue::PointerMismatch));
        assert_eq!(frame.canceled, Some(7));
        assert!(frame.samples.is_empty());
    }

    #[test]
    fn ambiguous_touch_proof_still_quarantines_the_pen() {
        let p = pos2(20.0, 30.0);
        let down = [button(p, true)];
        let pen_down = [record(
            PointerPhase::Press(PointerButton::Primary),
            p,
            Some((7, 0.2)),
        )];
        let mut matcher = PenInputMatcher::default();
        matcher.match_frame(&down, &pen_down, None, false);
        let touch = TouchPointerFrame {
            issue: Some(crate::touch_input::TouchPointerIssue::EventMismatch),
            ..TouchPointerFrame::default()
        };
        let frame = matcher.match_frame_with_touch(&[Event::PointerGone], &[], None, false, &touch);
        assert_eq!(frame.issue, Some(PenInputIssue::PointerMismatch));
        assert_eq!(frame.canceled, Some(7));

        let moved = [Event::PointerMoved(p + egui::vec2(10.0, 0.0))];
        let records = [record(
            PointerPhase::Move,
            p + egui::vec2(10.0, 0.0),
            Some((7, 0.6)),
        )];
        assert_eq!(
            matcher.match_frame(&moved, &records, None, false).issue,
            Some(PenInputIssue::PointerMismatch)
        );
        let release = [button(p, false)];
        let release_record = [record(
            PointerPhase::Release(PointerButton::Primary),
            p,
            Some((7, 0.0)),
        )];
        assert!(
            matcher
                .match_frame(&release, &release_record, None, false)
                .issue
                .is_some()
        );
        assert_eq!(
            matcher.match_frame(&down, &pen_down, None, false).issue,
            None
        );
    }

    #[test]
    fn rewritten_or_later_frames_fail_closed() {
        let ctx = Context::default();
        let p = pos2(10.0, 20.0);
        let down = [button(p, true)];
        let pen = [record(
            PointerPhase::Press(PointerButton::Primary),
            p,
            Some((4, 0.5)),
        )];
        let mut matcher = PenInputMatcher::default();
        let frame = matcher.match_frame(&down, &pen, None, false);
        publish(&ctx, ViewportId::ROOT, &down, frame);
        let changed = [button(pos2(11.0, 20.0), true)];
        let data = take(&ctx, ViewportId::ROOT, &changed);
        assert!(data.samples.is_empty());
        assert_eq!(data.canceled, Some(4));
        assert_eq!(data.issue, Some(PenInputIssue::PointerMismatch));

        let frame = matcher.match_frame(&down, &pen, None, false);
        publish(&ctx, ViewportId::ROOT, &down, frame);
        let mut output = ctx.run_pass(Default::default(), |_| {});
        output.textures_delta.clear();
        let data = take(&ctx, ViewportId::ROOT, &down);
        assert!(data.samples.is_empty());
        assert_eq!(data.canceled, Some(4));
        assert_eq!(data.issue, Some(PenInputIssue::PointerMismatch));
    }
}
