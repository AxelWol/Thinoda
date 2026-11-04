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
