//! Frame-scoped pen metadata for hosts whose egui pointer events omit pressure.

use std::sync::Arc;

use egui::{Context, Event, Id, PointerButton, Pos2, ViewportId};

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

    fn ended(self, events: &[Event], records: &[PointerRecord], canceled: Option<i32>) -> bool {
        canceled.is_some_and(|id| self.id().is_none_or(|active| active == id))
            || records.iter().any(|record| {
                matches!(record.phase, PointerPhase::Release(PointerButton::Primary))
                    && record
                        .pen
                        .is_some_and(|pen| self.id().is_none_or(|active| active == pen.id))
            })
            || events
                .iter()
                .any(|event| matches!(event, Event::PointerGone | Event::WindowFocused(false)))
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
        if let Some(quarantine) = self.quarantine {
            if events.is_empty() && records.is_empty() && canceled.is_none() && !overflowed {
                return MatchedPenFrame::empty();
            }
            if quarantine.ended(events, records, canceled) {
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

        let issue = if overflowed || records.len() > MAX_POINTER_RECORDS {
            Some(PenInputIssue::QueueOverflow)
        } else if events.len() > MAX_FRAME_EVENTS {
            Some(PenInputIssue::FrameTooLarge)
        } else {
            let mut raw = events.iter().filter_map(pointer_signature);
            let matches = records.iter().all(|record| {
                raw.next()
                    .is_some_and(|signature| signature == (record.phase, record.position))
            }) && raw.next().is_none();
            (!matches).then_some(PenInputIssue::PointerMismatch)
        };
        if let Some(issue) = issue {
            self.active_pen = None;
            let quarantine = previous_pen
                .or(first_pen)
                .map_or(Quarantine::Unknown, Quarantine::Pen);
            if !quarantine.ended(events, records, canceled) {
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
        let mut records = records.iter();
        let mut interrupted = None;
        for (index, event) in events.iter().enumerate() {
            if let Some((phase, _)) = pointer_signature(event) {
                let record = records.next().expect("pointer records were matched above");
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
            } else if matches!(event, Event::PointerGone | Event::WindowFocused(false)) {
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
