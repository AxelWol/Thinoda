use std::{any::TypeId, ops::Range};

use egui::{
    Event, ImeEvent, Key, Modifiers, TextBuffer, TextEdit,
    accesskit::Role,
    text::{CCursor, CCursorRange, CharIndex},
};
use egui_kittest::{Harness, kittest::Queryable};

#[derive(Debug, PartialEq, Eq)]
enum Operation {
    Insert { index: usize, text: String },
    Delete(Range<usize>),
    Replace(String),
}

struct RecordingBuffer {
    text: String,
    operations: Vec<Operation>,
}

impl TextBuffer for RecordingBuffer {
    fn is_mutable(&self) -> bool {
        true
    }

    fn as_str(&self) -> &str {
        &self.text
    }

    fn insert_text(&mut self, text: &str, index: CharIndex) -> usize {
        self.operations.push(Operation::Insert {
            index: index.0,
            text: text.to_owned(),
        });
        self.text.insert_text(text, index)
    }

    fn delete_char_range(&mut self, range: Range<CharIndex>) {
        self.operations
            .push(Operation::Delete(range.start.0..range.end.0));
        self.text.delete_char_range(range);
    }

    fn replace_with(&mut self, text: &str) {
        self.operations.push(Operation::Replace(text.to_owned()));
        self.text.clear();
        self.text.push_str(text);
    }

    fn type_id(&self) -> TypeId {
        TypeId::of::<Self>()
    }
}

fn harness(text: &str) -> Harness<'static, RecordingBuffer> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(400.0, 100.0))
        .build_ui_state(
            |ui, buffer: &mut RecordingBuffer| {
                ui.ctx()
                    .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
                let label = ui.label("Input");
                ui.add(TextEdit::singleline(buffer).id(egui::Id::unique("recording-buffer")))
                    .labelled_by(label.id);
            },
            RecordingBuffer {
                text: text.to_owned(),
                operations: Vec::new(),
            },
        );
    h.run_steps(3);
    h.get_by_role_and_label(Role::TextInput, "Input").focus();
    h.run_steps(3);
    h
}

fn select(h: &mut Harness<'_, RecordingBuffer>, start: usize, end: usize) {
    let id = h.ctx.memory(|memory| memory.focused()).unwrap();
    let mut state = TextEdit::load_state(&h.ctx, id).unwrap();
    state.cursor.set_char_range(Some(CCursorRange::two(
        CCursor::new(start),
        CCursor::new(end),
    )));
    state.store(&h.ctx, id);
    h.state_mut().operations.clear();
}

fn input(h: &mut Harness<'_, RecordingBuffer>, event: Event) {
    h.event(event);
    h.run_steps(1);
}

#[test]
fn selected_typing_and_paste_use_scalar_indexed_deletes_then_inserts() {
    for event in [Event::Text("漢".into()), Event::Paste("漢".into())] {
        let mut h = harness("aé🙂z");
        select(&mut h, 3, 1);
        input(&mut h, event);
        assert_eq!(h.state().text, "a漢z");
        assert_eq!(
            h.state().operations,
            [
                Operation::Delete(1..3),
                Operation::Insert {
                    index: 1,
                    text: "漢".into(),
                },
            ]
        );
    }
}

#[test]
fn preedit_replacement_surrounding_deletion_and_commit_are_ordered() {
    for same_frame in [false, true] {
        let mut h = harness("aé🙂z");
        select(&mut h, 1, 3);
        let events = [
            ImeEvent::Preedit {
                text: "ni".into(),
                active_range_chars: Some(0..2),
            },
            ImeEvent::Preedit {
                text: "日本".into(),
                active_range_chars: Some(0..2),
            },
            ImeEvent::DeleteSurrounding {
                before_chars: 1,
                after_chars: 1,
            },
            ImeEvent::Commit("語".into()),
        ];
        if same_frame {
            h.input_mut()
                .events
                .extend(events.into_iter().map(Event::Ime));
            h.run_steps(1);
        } else {
            for event in events {
                input(&mut h, Event::Ime(event));
            }
        }
        assert_eq!(h.state().text, "語", "same frame: {same_frame}");
        assert_eq!(
            h.state().operations,
            [
                Operation::Delete(1..3),
                Operation::Insert {
                    index: 1,
                    text: "ni".into(),
                },
                Operation::Delete(1..3),
                Operation::Insert {
                    index: 1,
                    text: "日本".into(),
                },
                Operation::Delete(3..4),
                Operation::Delete(0..1),
                Operation::Delete(0..2),
                Operation::Insert {
                    index: 0,
                    text: "語".into(),
                },
            ],
            "same frame: {same_frame}"
        );
    }
}

#[test]
fn empty_ime_events_do_not_restore_the_selection_replaced_by_preedit() {
    for empty in [
        ImeEvent::Preedit {
            text: String::new(),
            active_range_chars: None,
        },
        ImeEvent::Commit(String::new()),
    ] {
        let mut h = harness("aé🙂z");
        select(&mut h, 1, 3);
        input(&mut h, Event::Ime(empty.clone()));
        assert_eq!(h.state().text, "aé🙂z");
        assert!(h.state().operations.is_empty());
        input(
            &mut h,
            Event::Ime(ImeEvent::Preedit {
                text: "n".into(),
                active_range_chars: None,
            }),
        );
        input(&mut h, Event::Ime(empty));
        assert_eq!(h.state().text, "az");
        assert_eq!(
            h.state().operations,
            [
                Operation::Delete(1..3),
                Operation::Insert {
                    index: 1,
                    text: "n".into(),
                },
                Operation::Delete(1..2),
            ]
        );
    }
}

#[test]
fn surrounding_deletion_can_pass_an_oversized_range_to_the_buffer() {
    let mut h = harness("aéz");
    select(&mut h, 1, 2);
    input(
        &mut h,
        Event::Ime(ImeEvent::DeleteSurrounding {
            before_chars: usize::MAX,
            after_chars: usize::MAX,
        }),
    );
    assert_eq!(h.state().text, "é");
    assert_eq!(
        h.state().operations,
        [Operation::Delete(2..usize::MAX), Operation::Delete(0..1)]
    );
}

#[test]
fn egui_undo_and_redo_replace_the_whole_plain_text_buffer() {
    let mut h = harness("a");
    select(&mut h, 1, 1);
    input(&mut h, Event::Text("b".into()));
    assert_eq!(h.state().text, "ab");
    h.state_mut().operations.clear();
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(3);
    assert_eq!(h.state().text, "a");
    assert_eq!(h.state().operations, [Operation::Replace("a".into())]);
    h.state_mut().operations.clear();
    h.key_press_modifiers(Modifiers::COMMAND, Key::Y);
    h.run_steps(3);
    assert_eq!(h.state().text, "ab");
    assert_eq!(h.state().operations, [Operation::Replace("ab".into())]);
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StyledDraft {
    text: String,
    styles: Vec<u8>,
}

struct GuardedBuffer {
    draft: StyledDraft,
    max_bytes: usize,
    rejected: bool,
    rejected_frames: usize,
    accepted_frames: usize,
    rejected_response_changed: bool,
}

impl TextBuffer for GuardedBuffer {
    fn is_mutable(&self) -> bool {
        true
    }

    fn as_str(&self) -> &str {
        &self.draft.text
    }

    fn insert_text(&mut self, text: &str, index: CharIndex) -> usize {
        let length = self.draft.text.len().checked_add(text.len());
        if self.rejected
            || index.0 > self.draft.styles.len()
            || length.is_none_or(|length| length > self.max_bytes)
        {
            self.rejected = true;
            return 0;
        }
        let byte = self
            .draft
            .text
            .char_indices()
            .nth(index.0)
            .map_or(self.draft.text.len(), |(byte, _)| byte);
        let inserted = text.chars().count();
        self.draft.text.insert_str(byte, text);
        self.draft
            .styles
            .splice(index.0..index.0, std::iter::repeat_n(9, inserted));
        inserted
    }

    fn delete_char_range(&mut self, range: Range<CharIndex>) {
        if self.rejected || range.start.0 > range.end.0 || range.end.0 > self.draft.styles.len() {
            self.rejected = true;
            return;
        }
        self.draft.text.delete_char_range(range.clone());
        self.draft.styles.drain(range.start.0..range.end.0);
    }

    fn replace_with(&mut self, text: &str) {
        if self.rejected || text.len() > self.max_bytes {
            self.rejected = true;
            return;
        }
        // This deliberately naive implementation demonstrates plain undo losing styles.
        self.draft.text = text.to_owned();
        self.draft.styles = vec![0; text.chars().count()];
    }

    fn type_id(&self) -> TypeId {
        TypeId::of::<Self>()
    }
}

fn guarded_harness(
    text: &str,
    styles: Vec<u8>,
    max_bytes: usize,
) -> Harness<'static, GuardedBuffer> {
    assert_eq!(text.chars().count(), styles.len());
    let mut h = Harness::builder()
        .with_size(egui::vec2(400.0, 100.0))
        .build_ui_state(
            |ui, buffer: &mut GuardedBuffer| {
                ui.ctx()
                    .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
                let id = egui::Id::unique("guarded-buffer");
                let previous_draft = buffer.draft.clone();
                let previous_state = TextEdit::load_state(ui.ctx(), id);
                // TextEditState clones share the undoer, so its contents need a separate copy.
                let previous_undo = previous_state.as_ref().map(|state| state.undoer());
                buffer.rejected = false;
                let label = ui.label("Guarded input");
                let response = ui
                    .add(TextEdit::singleline(buffer).id(id))
                    .labelled_by(label.id);
                if buffer.rejected {
                    buffer.draft = previous_draft;
                    if let (Some(mut state), Some(undo)) = (previous_state, previous_undo) {
                        state.set_undoer(undo);
                        state.store(ui.ctx(), id);
                    }
                    buffer.rejected_frames += 1;
                    buffer.rejected_response_changed = response.changed();
                    ui.ctx().request_repaint();
                } else if response.changed() {
                    buffer.accepted_frames += 1;
                }
            },
            GuardedBuffer {
                draft: StyledDraft {
                    text: text.to_owned(),
                    styles,
                },
                max_bytes,
                rejected: false,
                rejected_frames: 0,
                accepted_frames: 0,
                rejected_response_changed: false,
            },
        );
    h.run_steps(3);
    h.get_by_role_and_label(Role::TextInput, "Guarded input")
        .focus();
    h.run_steps(3);
    h
}

fn select_guarded(h: &mut Harness<'_, GuardedBuffer>, start: usize, end: usize) {
    let id = h.ctx.memory(|memory| memory.focused()).unwrap();
    let mut state = TextEdit::load_state(&h.ctx, id).unwrap();
    state.cursor.set_char_range(Some(CCursorRange::two(
        CCursor::new(start),
        CCursor::new(end),
    )));
    state.store(&h.ctx, id);
}

#[test]
fn guarded_frame_restores_unicode_text_styles_and_selection_after_rejected_paste() {
    for (start, end) in [(1, 3), (3, 1)] {
        let mut h = guarded_harness("aé🙂z", vec![1, 2, 3, 4], 8);
        select_guarded(&mut h, start, end);
        let id = h.ctx.memory(|memory| memory.focused()).unwrap();
        let before = TextEdit::load_state(&h.ctx, id)
            .unwrap()
            .cursor
            .char_range();
        h.event(Event::Paste("日本語".into()));
        h.run_steps(1);
        assert_eq!(
            h.get_by_role_and_label(Role::TextInput, "Guarded input")
                .value()
                .as_deref(),
            Some("az")
        );
        assert_eq!(
            h.state().draft,
            StyledDraft {
                text: "aé🙂z".into(),
                styles: vec![1, 2, 3, 4],
            }
        );
        assert_eq!(h.state().rejected_frames, 1);
        assert_eq!(h.state().accepted_frames, 0);
        assert!(h.state().rejected_response_changed);
        assert_eq!(
            TextEdit::load_state(&h.ctx, id)
                .unwrap()
                .cursor
                .char_range(),
            before
        );
        h.run_steps(1);
        assert_eq!(
            h.get_by_role_and_label(Role::TextInput, "Guarded input")
                .value()
                .as_deref(),
            Some("aé🙂z")
        );

        h.event(Event::Text("x".into()));
        h.run_steps(1);
        assert_eq!(
            h.state().draft,
            StyledDraft {
                text: "axz".into(),
                styles: vec![1, 9, 4],
            }
        );
    }
}

#[test]
fn guarded_frame_also_discards_valid_input_before_a_rejected_same_frame_event() {
    let mut h = guarded_harness("a", vec![1], 4);
    select_guarded(&mut h, 1, 1);
    h.input_mut()
        .events
        .extend([Event::Text("b".into()), Event::Paste("🙂".into())]);
    h.run_steps(1);
    assert_eq!(
        h.state().draft,
        StyledDraft {
            text: "a".into(),
            styles: vec![1],
        }
    );
    assert_eq!(h.state().rejected_frames, 1);
    assert_eq!(h.state().accepted_frames, 0);
}

#[test]
fn guarded_frame_restores_selection_after_invalid_ime_operations() {
    for event in [
        ImeEvent::Preedit {
            text: "日本語".into(),
            active_range_chars: Some(0..3),
        },
        ImeEvent::DeleteSurrounding {
            before_chars: usize::MAX,
            after_chars: usize::MAX,
        },
    ] {
        let mut h = guarded_harness("aé🙂z", vec![1, 2, 3, 4], 8);
        select_guarded(&mut h, 1, 3);
        h.event(Event::Ime(event));
        h.run_steps(1);
        assert_eq!(
            h.state().draft,
            StyledDraft {
                text: "aé🙂z".into(),
                styles: vec![1, 2, 3, 4],
            }
        );
        assert_eq!(h.state().rejected_frames, 1);
        h.event(Event::Text("x".into()));
        h.run_steps(1);
        assert_eq!(
            h.state().draft,
            StyledDraft {
                text: "axz".into(),
                styles: vec![1, 9, 4],
            }
        );
    }
}

#[test]
fn egui_plain_undo_after_guarded_edits_does_not_restore_inserted_styles() {
    let mut h = guarded_harness("a", vec![0], 4);
    select_guarded(&mut h, 1, 1);
    h.event(Event::Text("b".into()));
    h.run_steps(3);
    assert_eq!(h.state().draft.styles, [0, 9]);
    select_guarded(&mut h, 1, 2);
    h.event(Event::Paste("🙂🙂".into()));
    h.run_steps(1);
    assert_eq!(h.state().draft.text, "ab");
    assert_eq!(h.state().draft.styles, [0, 9]);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(3);
    assert_eq!(h.state().draft.text, "a");
    h.key_press_modifiers(Modifiers::COMMAND, Key::Y);
    h.run_steps(3);
    assert_eq!(h.state().draft.text, "ab");
    assert_eq!(h.state().draft.styles, [0, 0]);
}
