//! Isolated span-editing experiment. This is not part of the application.

use std::{collections::VecDeque, ops::Range};

#[derive(Clone, Debug, PartialEq)]
struct Span<S> {
    range: Range<usize>,
    style: S,
}

#[derive(Clone, Copy, Debug)]
struct Limits {
    text_bytes: usize,
    spans: usize,
}

#[derive(Clone, Debug, PartialEq)]
struct StyledText<S> {
    text: String,
    spans: Vec<Span<S>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditError {
    InvalidRange,
    InvalidSpans,
    TextLimit,
    SpanLimit,
    SizeOverflow,
}

impl<S: Clone + Default + PartialEq> StyledText<S> {
    fn new(text: String, spans: Vec<Span<S>>, limits: Limits) -> Result<Self, EditError> {
        let value = Self { text, spans };
        value.validate(limits)?;
        Ok(value)
    }

    fn validate(&self, limits: Limits) -> Result<(), EditError> {
        if self.text.len() > limits.text_bytes {
            return Err(EditError::TextLimit);
        }
        if self.spans.len() > limits.spans {
            return Err(EditError::SpanLimit);
        }
        let length = self.text.chars().count();
        let mut previous: Option<&Span<S>> = None;
        for span in &self.spans {
            if span.range.start >= span.range.end
                || span.range.end > length
                || span.style == S::default()
                || previous.is_some_and(|prior| {
                    prior.range.end > span.range.start
                        || (prior.range.end == span.range.start && prior.style == span.style)
                })
            {
                return Err(EditError::InvalidSpans);
            }
            previous = Some(span);
        }
        Ok(())
    }

    fn checked_range(&self, range: &Range<usize>) -> Result<(), EditError> {
        if range.start > range.end || range.end > self.text.chars().count() {
            return Err(EditError::InvalidRange);
        }
        Ok(())
    }

    fn replace(
        &mut self,
        range: Range<usize>,
        inserted: &str,
        inserted_style: S,
        limits: Limits,
    ) -> Result<(), EditError> {
        self.checked_range(&range)?;
        let start = byte_index(&self.text, range.start);
        let end = byte_index(&self.text, range.end);
        let bytes = start
            .checked_add(inserted.len())
            .and_then(|n| n.checked_add(self.text.len() - end))
            .ok_or(EditError::SizeOverflow)?;
        if bytes > limits.text_bytes {
            return Err(EditError::TextLimit);
        }
        let inserted_chars = inserted.chars().count();
        let inserted_end = range
            .start
            .checked_add(inserted_chars)
            .ok_or(EditError::SizeOverflow)?;
        let mut text = String::with_capacity(bytes);
        text.push_str(&self.text[..start]);
        text.push_str(inserted);
        text.push_str(&self.text[end..]);

        let mut spans = Vec::new();
        for span in &self.spans {
            if span.range.start < range.start {
                push_span(
                    &mut spans,
                    span.range.start..span.range.end.min(range.start),
                    span.style.clone(),
                );
            }
        }
        push_span(&mut spans, range.start..inserted_end, inserted_style);
        for span in &self.spans {
            if span.range.end > range.end {
                let start = inserted_end
                    .checked_add(span.range.start.max(range.end) - range.end)
                    .ok_or(EditError::SizeOverflow)?;
                let end = inserted_end
                    .checked_add(span.range.end - range.end)
                    .ok_or(EditError::SizeOverflow)?;
                push_span(&mut spans, start..end, span.style.clone());
            }
        }
        let candidate = Self { text, spans };
        candidate.validate(limits)?;
        *self = candidate;
        Ok(())
    }

    fn format(
        &mut self,
        range: Range<usize>,
        change: impl Fn(&S) -> S,
        limits: Limits,
    ) -> Result<(), EditError> {
        self.checked_range(&range)?;
        if range.is_empty() {
            return self.validate(limits);
        }
        let length = self.text.chars().count();
        let mut boundaries = vec![0, range.start, range.end, length];
        for span in &self.spans {
            boundaries.extend([span.range.start, span.range.end]);
        }
        boundaries.sort_unstable();
        boundaries.dedup();
        let mut spans = Vec::new();
        let mut index = 0;
        for pair in boundaries.windows(2) {
            let segment = pair[0]..pair[1];
            while index < self.spans.len() && self.spans[index].range.end <= segment.start {
                index += 1;
            }
            let style = self
                .spans
                .get(index)
                .filter(|span| span.range.start <= segment.start && span.range.end >= segment.end)
                .map_or_else(S::default, |span| span.style.clone());
            let style = if segment.start >= range.start && segment.end <= range.end {
                change(&style)
            } else {
                style
            };
            push_span(&mut spans, segment, style);
        }
        let candidate = Self {
            text: self.text.clone(),
            spans,
        };
        candidate.validate(limits)?;
        *self = candidate;
        Ok(())
    }
}

fn byte_index(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map_or(text.len(), |(byte, _)| byte)
}

fn push_span<S: Default + PartialEq>(spans: &mut Vec<Span<S>>, range: Range<usize>, style: S) {
    if range.is_empty() || style == S::default() {
        return;
    }
    if let Some(previous) = spans.last_mut()
        && previous.range.end == range.start
        && previous.style == style
    {
        previous.range.end = range.end;
    } else {
        spans.push(Span { range, style });
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Selection {
    anchor: usize,
    active: usize,
}

impl Selection {
    fn caret(position: usize) -> Self {
        Self {
            anchor: position,
            active: position,
        }
    }

    fn ordered(self) -> Range<usize> {
        self.anchor.min(self.active)..self.anchor.max(self.active)
    }

    fn valid(self, length: usize) -> bool {
        self.anchor <= length && self.active <= length
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Draft<S> {
    value: StyledText<S>,
    selection: Selection,
    typing_style: S,
}

#[derive(Clone, Debug, PartialEq)]
struct HistoryEntry<S> {
    before: Draft<S>,
    after: Draft<S>,
}

#[derive(Clone, Debug, PartialEq)]
struct Composition<S> {
    before: Draft<S>,
    range: Range<usize>,
    style: S,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HistoryError {
    Edit(EditError),
    InvalidSelection,
    InvalidHistoryLimit,
    CompositionActive,
    CompositionMissing,
    UndoUnavailable,
    RedoUnavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommitOutcome {
    NoDocumentChange,
    Provisional,
    Recorded,
    RecordedAfterEviction,
}

struct DraftHistory<S> {
    current: Draft<S>,
    undo: VecDeque<HistoryEntry<S>>,
    redo: Vec<HistoryEntry<S>>,
    composition: Option<Composition<S>>,
    limits: Limits,
    max_entries: usize,
}

impl<S: Clone + Default + PartialEq> DraftHistory<S> {
    fn new(
        value: StyledText<S>,
        selection: Selection,
        typing_style: S,
        limits: Limits,
        max_entries: usize,
    ) -> Result<Self, HistoryError> {
        value.validate(limits).map_err(HistoryError::Edit)?;
        if !selection.valid(value.text.chars().count()) {
            return Err(HistoryError::InvalidSelection);
        }
        if max_entries == 0 {
            return Err(HistoryError::InvalidHistoryLimit);
        }
        Ok(Self {
            current: Draft {
                value,
                selection,
                typing_style,
            },
            undo: VecDeque::new(),
            redo: Vec::new(),
            composition: None,
            limits,
            max_entries,
        })
    }

    fn ensure_idle(&self) -> Result<(), HistoryError> {
        if self.composition.is_some() {
            Err(HistoryError::CompositionActive)
        } else {
            Ok(())
        }
    }

    fn set_selection(&mut self, selection: Selection) -> Result<(), HistoryError> {
        self.ensure_idle()?;
        if !selection.valid(self.current.value.text.chars().count()) {
            return Err(HistoryError::InvalidSelection);
        }
        self.current.selection = selection;
        Ok(())
    }

    fn record(&mut self, before: Draft<S>, after: Draft<S>) -> CommitOutcome {
        if before.value == after.value {
            self.current = after;
            return CommitOutcome::NoDocumentChange;
        }
        let evicted = self.undo.len() == self.max_entries;
        if evicted {
            self.undo.pop_front();
        }
        self.redo.clear();
        self.current = after.clone();
        self.undo.push_back(HistoryEntry { before, after });
        if evicted {
            CommitOutcome::RecordedAfterEviction
        } else {
            CommitOutcome::Recorded
        }
    }

    fn replace_selection(&mut self, inserted: &str) -> Result<CommitOutcome, HistoryError> {
        self.ensure_idle()?;
        let before = self.current.clone();
        let mut after = before.clone();
        let range = before.selection.ordered();
        let end = range
            .start
            .checked_add(inserted.chars().count())
            .ok_or(HistoryError::Edit(EditError::SizeOverflow))?;
        after
            .value
            .replace(range, inserted, before.typing_style.clone(), self.limits)
            .map_err(HistoryError::Edit)?;
        after.selection = Selection::caret(end);
        Ok(self.record(before, after))
    }

    fn format_selection(
        &mut self,
        change: impl Fn(&S) -> S,
    ) -> Result<CommitOutcome, HistoryError> {
        self.ensure_idle()?;
        let range = self.current.selection.ordered();
        if range.is_empty() {
            self.current.typing_style = change(&self.current.typing_style);
            return Ok(CommitOutcome::NoDocumentChange);
        }
        let before = self.current.clone();
        let mut after = before.clone();
        after
            .value
            .format(range, change, self.limits)
            .map_err(HistoryError::Edit)?;
        Ok(self.record(before, after))
    }

    fn begin_composition(&mut self) -> Result<(), HistoryError> {
        self.ensure_idle()?;
        self.composition = Some(Composition {
            before: self.current.clone(),
            range: self.current.selection.ordered(),
            style: self.current.typing_style.clone(),
        });
        Ok(())
    }

    fn update_composition(&mut self, text: &str, selection: Selection) -> Result<(), HistoryError> {
        let composition = self
            .composition
            .as_mut()
            .ok_or(HistoryError::CompositionMissing)?;
        let length = text.chars().count();
        if !selection.valid(length) {
            return Err(HistoryError::InvalidSelection);
        }
        let start = composition.range.start;
        let end = start
            .checked_add(length)
            .ok_or(HistoryError::Edit(EditError::SizeOverflow))?;
        let mut after = self.current.clone();
        after
            .value
            .replace(
                composition.range.clone(),
                text,
                composition.style.clone(),
                self.limits,
            )
            .map_err(HistoryError::Edit)?;
        after.selection = Selection {
            anchor: start + selection.anchor,
            active: start + selection.active,
        };
        composition.range = start..end;
        self.current = after;
        Ok(())
    }

    fn delete_surrounding(
        &mut self,
        before_chars: usize,
        after_chars: usize,
    ) -> Result<CommitOutcome, HistoryError> {
        let range = self.composition.as_ref().map_or_else(
            || self.current.selection.ordered(),
            |item| item.range.clone(),
        );
        let start = range
            .start
            .checked_sub(before_chars)
            .ok_or(HistoryError::Edit(EditError::InvalidRange))?;
        let end = range
            .end
            .checked_add(after_chars)
            .ok_or(HistoryError::Edit(EditError::SizeOverflow))?;
        if end > self.current.value.text.chars().count() {
            return Err(HistoryError::Edit(EditError::InvalidRange));
        }
        if before_chars == 0 && after_chars == 0 {
            return Ok(CommitOutcome::NoDocumentChange);
        }
        let before = self.current.clone();
        let mut after = before.clone();
        after
            .value
            .replace(range.end..end, "", S::default(), self.limits)
            .map_err(HistoryError::Edit)?;
        after
            .value
            .replace(start..range.start, "", S::default(), self.limits)
            .map_err(HistoryError::Edit)?;
        after.selection = Selection {
            anchor: before.selection.anchor - before_chars,
            active: before.selection.active - before_chars,
        };
        if let Some(composition) = self.composition.as_mut() {
            composition.range = start..range.end - before_chars;
            self.current = after;
            Ok(CommitOutcome::Provisional)
        } else {
            Ok(self.record(before, after))
        }
    }

    fn commit_composition(
        &mut self,
        text: &str,
        selection: Selection,
    ) -> Result<CommitOutcome, HistoryError> {
        self.update_composition(text, selection)?;
        let composition = self
            .composition
            .take()
            .ok_or(HistoryError::CompositionMissing)?;
        Ok(self.record(composition.before, self.current.clone()))
    }

    fn cancel_composition(&mut self) -> Result<(), HistoryError> {
        let composition = self
            .composition
            .take()
            .ok_or(HistoryError::CompositionMissing)?;
        self.current = composition.before;
        Ok(())
    }

    fn undo(&mut self) -> Result<(), HistoryError> {
        self.ensure_idle()?;
        let entry = self.undo.pop_back().ok_or(HistoryError::UndoUnavailable)?;
        self.current = entry.before.clone();
        self.redo.push(entry);
        Ok(())
    }

    fn redo(&mut self) -> Result<(), HistoryError> {
        self.ensure_idle()?;
        let entry = self.redo.pop().ok_or(HistoryError::RedoUnavailable)?;
        self.current = entry.after.clone();
        self.undo.push_back(entry);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMITS: Limits = Limits {
        text_bytes: 4096,
        spans: 256,
    };

    #[derive(Clone, Debug, Default, PartialEq)]
    struct Style {
        bold: Option<bool>,
        italic: Option<bool>,
    }

    fn bold(value: bool) -> Style {
        Style {
            bold: Some(value),
            ..Style::default()
        }
    }

    fn flatten(value: &StyledText<Style>) -> Vec<Style> {
        let mut styles = vec![Style::default(); value.text.chars().count()];
        for span in &value.spans {
            for style in &mut styles[span.range.clone()] {
                *style = span.style.clone();
            }
        }
        styles
    }

    fn history(
        text: &str,
        spans: Vec<Span<Style>>,
        selection: Selection,
        typing_style: Style,
        max_entries: usize,
    ) -> DraftHistory<Style> {
        DraftHistory::new(
            StyledText::new(text.into(), spans, LIMITS).unwrap(),
            selection,
            typing_style,
            LIMITS,
            max_entries,
        )
        .unwrap()
    }

    #[derive(Clone, Debug, PartialEq)]
    struct CharacterDraft {
        chars: Vec<char>,
        styles: Vec<Style>,
        selection: Selection,
        typing_style: Style,
    }

    fn oracle_record(
        before: CharacterDraft,
        after: &CharacterDraft,
        undo: &mut VecDeque<(CharacterDraft, CharacterDraft)>,
        redo: &mut Vec<(CharacterDraft, CharacterDraft)>,
        max_entries: usize,
    ) -> CommitOutcome {
        if before.chars == after.chars && before.styles == after.styles {
            return CommitOutcome::NoDocumentChange;
        }
        let evicted = undo.len() == max_entries;
        if evicted {
            undo.pop_front();
        }
        redo.clear();
        undo.push_back((before, after.clone()));
        if evicted {
            CommitOutcome::RecordedAfterEviction
        } else {
            CommitOutcome::Recorded
        }
    }

    fn oracle_replace(
        draft: &mut CharacterDraft,
        range: Range<usize>,
        inserted: &str,
        style: &Style,
        selection: Selection,
    ) {
        let chars: Vec<_> = inserted.chars().collect();
        draft.chars.splice(range.clone(), chars.iter().copied());
        draft
            .styles
            .splice(range, std::iter::repeat_n(style.clone(), chars.len()));
        draft.selection = selection;
    }

    fn oracle_delete_surrounding(
        draft: &mut CharacterDraft,
        range: Range<usize>,
        before: usize,
        after: usize,
    ) {
        draft.chars.drain(range.end..range.end + after);
        draft.styles.drain(range.end..range.end + after);
        draft.chars.drain(range.start - before..range.start);
        draft.styles.drain(range.start - before..range.start);
        draft.selection.anchor -= before;
        draft.selection.active -= before;
    }

    fn assert_draft_matches(actual: &Draft<Style>, expected: &CharacterDraft) {
        assert_eq!(actual.value.text, expected.chars.iter().collect::<String>());
        assert_eq!(flatten(&actual.value), expected.styles);
        assert_eq!(actual.selection, expected.selection);
        assert_eq!(actual.typing_style, expected.typing_style);
        actual.value.validate(LIMITS).unwrap();
    }

    fn assert_history_matches(
        actual: &DraftHistory<Style>,
        current: &CharacterDraft,
        undo: &VecDeque<(CharacterDraft, CharacterDraft)>,
        redo: &[(CharacterDraft, CharacterDraft)],
    ) {
        assert_draft_matches(&actual.current, current);
        assert_eq!(actual.undo.len(), undo.len());
        assert_eq!(actual.redo.len(), redo.len());
        for (entry, (before, after)) in actual.undo.iter().zip(undo) {
            assert_draft_matches(&entry.before, before);
            assert_draft_matches(&entry.after, after);
        }
        for (entry, (before, after)) in actual.redo.iter().zip(redo) {
            assert_draft_matches(&entry.before, before);
            assert_draft_matches(&entry.after, after);
        }
    }

    #[test]
    fn history_restores_unicode_styles_and_directional_selection() {
        let mut history = history(
            "aé🙂z",
            vec![Span {
                range: 1..3,
                style: bold(true),
            }],
            Selection {
                anchor: 3,
                active: 1,
            },
            Style {
                italic: Some(true),
                ..Style::default()
            },
            4,
        );
        let before = history.current.clone();
        assert_eq!(history.replace_selection("漢"), Ok(CommitOutcome::Recorded));
        let after = history.current.clone();
        assert_eq!(after.value.text, "a漢z");
        assert_eq!(
            flatten(&after.value),
            vec![
                Style::default(),
                Style {
                    italic: Some(true),
                    ..Style::default()
                },
                Style::default()
            ]
        );
        history.set_selection(Selection::caret(0)).unwrap();
        history.undo().unwrap();
        assert_eq!(history.current, before);
        history.redo().unwrap();
        assert_eq!(history.current, after);

        history.undo().unwrap();
        assert_eq!(
            history.replace_selection("é🙂"),
            Ok(CommitOutcome::Recorded)
        );
        assert_eq!(history.current.value.text, before.value.text);
        assert_ne!(history.current.value.spans, before.value.spans);
        assert_eq!(history.redo(), Err(HistoryError::RedoUnavailable));
    }

    #[test]
    fn selected_format_and_caret_typing_style_have_distinct_history() {
        let mut history = history(
            "ab",
            vec![],
            Selection {
                anchor: 0,
                active: 2,
            },
            Style::default(),
            4,
        );
        assert_eq!(
            history.format_selection(|style| Style {
                bold: Some(false),
                ..style.clone()
            }),
            Ok(CommitOutcome::Recorded)
        );
        assert_eq!(flatten(&history.current.value), vec![bold(false); 2]);
        history.set_selection(Selection::caret(1)).unwrap();
        assert_eq!(
            history.format_selection(|style| Style {
                italic: Some(true),
                ..style.clone()
            }),
            Ok(CommitOutcome::NoDocumentChange)
        );
        assert_eq!(history.undo.len(), 1);
        let before_typing = history.current.clone();
        history.replace_selection("🙂").unwrap();
        assert_eq!(history.current.value.text, "a🙂b");
        assert_eq!(
            flatten(&history.current.value),
            vec![
                bold(false),
                Style {
                    italic: Some(true),
                    ..Style::default()
                },
                bold(false)
            ]
        );
        history.undo().unwrap();
        assert_eq!(history.current, before_typing);
        history.undo().unwrap();
        assert_eq!(flatten(&history.current.value), vec![Style::default(); 2]);
    }

    #[test]
    fn composition_preedits_are_one_transaction_and_cancel_preserves_redo() {
        let mut history = history(
            "aé🙂z",
            vec![Span {
                range: 1..3,
                style: bold(true),
            }],
            Selection {
                anchor: 3,
                active: 1,
            },
            bold(false),
            4,
        );
        let before = history.current.clone();
        history.begin_composition().unwrap();
        history
            .update_composition("に", Selection::caret(1))
            .unwrap();
        history
            .update_composition(
                "日本",
                Selection {
                    anchor: 2,
                    active: 0,
                },
            )
            .unwrap();
        assert_eq!(history.current.value.text, "a日本z");
        assert_eq!(
            history.current.selection,
            Selection {
                anchor: 3,
                active: 1
            }
        );
        assert_eq!(history.undo(), Err(HistoryError::CompositionActive));
        assert_eq!(history.redo(), Err(HistoryError::CompositionActive));
        assert_eq!(
            history.set_selection(Selection::caret(0)),
            Err(HistoryError::CompositionActive)
        );
        assert_eq!(
            history.begin_composition(),
            Err(HistoryError::CompositionActive)
        );
        assert_eq!(
            history.format_selection(|style| style.clone()),
            Err(HistoryError::CompositionActive)
        );
        assert_eq!(
            history.replace_selection("unexpected"),
            Err(HistoryError::CompositionActive)
        );
        let preview = history.current.clone();
        let composition = history.composition.clone();
        assert_eq!(
            history.update_composition("x", Selection::caret(2)),
            Err(HistoryError::InvalidSelection)
        );
        assert_eq!(
            history.update_composition(&"x".repeat(4097), Selection::caret(0)),
            Err(HistoryError::Edit(EditError::TextLimit))
        );
        assert_eq!(
            history.commit_composition(&"x".repeat(4097), Selection::caret(0)),
            Err(HistoryError::Edit(EditError::TextLimit))
        );
        assert_eq!(history.current, preview);
        assert_eq!(history.composition, composition);
        assert_eq!(
            history.commit_composition(
                "日本",
                Selection {
                    anchor: 2,
                    active: 0
                }
            ),
            Ok(CommitOutcome::Recorded)
        );
        assert_eq!(history.undo.len(), 1);
        history.undo().unwrap();
        assert_eq!(history.current, before);
        history.redo().unwrap();
        assert_eq!(history.current, preview);
        history.undo().unwrap();
        history.begin_composition().unwrap();
        history
            .update_composition("漢", Selection::caret(1))
            .unwrap();
        history.cancel_composition().unwrap();
        assert_eq!(history.current, before);
        history.redo().unwrap();
        assert_eq!(history.current, preview);
        assert_eq!(
            history.cancel_composition(),
            Err(HistoryError::CompositionMissing)
        );
    }

    #[test]
    fn failed_composition_span_budget_leaves_preview_and_history_unchanged() {
        let limits = Limits {
            text_bytes: 32,
            spans: 1,
        };
        let value = StyledText::new(
            "abc".into(),
            vec![Span {
                range: 0..3,
                style: bold(true),
            }],
            limits,
        )
        .unwrap();
        let mut history = DraftHistory::new(
            value,
            Selection {
                anchor: 1,
                active: 2,
            },
            bold(false),
            limits,
            2,
        )
        .unwrap();
        let before = history.current.clone();
        history.begin_composition().unwrap();
        let composition = history.composition.clone();
        assert_eq!(
            history.update_composition("X", Selection::caret(1)),
            Err(HistoryError::Edit(EditError::SpanLimit))
        );
        assert_eq!(history.current, before);
        assert_eq!(history.composition, composition);
        assert!(history.undo.is_empty());
        history.cancel_composition().unwrap();
        assert_eq!(history.current, before);
    }

    #[test]
    fn composition_commits_explicit_final_text_instead_of_the_last_preview() {
        let mut history = history(
            "abcd",
            vec![],
            Selection {
                anchor: 1,
                active: 3,
            },
            bold(true),
            3,
        );
        let before = history.current.clone();
        history.begin_composition().unwrap();
        history
            .update_composition("preedit", Selection::caret(7))
            .unwrap();
        assert_eq!(
            history.commit_composition("日", Selection::caret(1)),
            Ok(CommitOutcome::Recorded)
        );
        assert_eq!(history.current.value.text, "a日d");
        assert_eq!(
            flatten(&history.current.value),
            vec![Style::default(), bold(true), Style::default()]
        );
        history.undo().unwrap();
        assert_eq!(history.current, before);

        history.begin_composition().unwrap();
        assert_eq!(
            history.commit_composition("", Selection::caret(0)),
            Ok(CommitOutcome::Recorded)
        );
        assert_eq!(history.current.value.text, "ad");
        history.undo().unwrap();
        assert_eq!(history.current, before);
    }

    #[test]
    fn surrounding_ime_deletions_follow_the_composition_as_one_transaction() {
        let mut history = history(
            "Aé🙂xyz",
            vec![Span {
                range: 1..4,
                style: bold(true),
            }],
            Selection {
                anchor: 4,
                active: 3,
            },
            bold(false),
            4,
        );
        let before = history.current.clone();
        history.begin_composition().unwrap();
        history
            .update_composition("日", Selection::caret(1))
            .unwrap();
        assert_eq!(
            history.delete_surrounding(2, 1),
            Ok(CommitOutcome::Provisional)
        );
        assert_eq!(history.current.value.text, "A日z");
        assert_eq!(history.current.selection, Selection::caret(2));
        assert_eq!(history.composition.as_ref().unwrap().range, 1..2);
        assert!(history.undo.is_empty());
        let preview = history.current.clone();
        let composition = history.composition.clone();
        assert_eq!(
            history.delete_surrounding(2, 0),
            Err(HistoryError::Edit(EditError::InvalidRange))
        );
        assert_eq!(
            history.delete_surrounding(0, usize::MAX),
            Err(HistoryError::Edit(EditError::SizeOverflow))
        );
        assert_eq!(history.current, preview);
        assert_eq!(history.composition, composition);
        assert_eq!(
            history.commit_composition("本", Selection::caret(1)),
            Ok(CommitOutcome::Recorded)
        );
        assert_eq!(history.current.value.text, "A本z");
        let after = history.current.clone();
        history.undo().unwrap();
        assert_eq!(history.current, before);
        history.redo().unwrap();
        assert_eq!(history.current, after);

        history.undo().unwrap();
        history.begin_composition().unwrap();
        history
            .update_composition("日", Selection::caret(1))
            .unwrap();
        history.delete_surrounding(2, 1).unwrap();
        history.cancel_composition().unwrap();
        assert_eq!(history.current, before);
        history.redo().unwrap();
        assert_eq!(history.current, after);
    }

    #[test]
    fn surrounding_deletion_without_composition_keeps_selection_and_undo() {
        let mut history = history(
            "abcde",
            vec![Span {
                range: 1..4,
                style: bold(true),
            }],
            Selection {
                anchor: 3,
                active: 2,
            },
            Style::default(),
            4,
        );
        let before = history.current.clone();
        assert_eq!(
            history.delete_surrounding(1, 1),
            Ok(CommitOutcome::Recorded)
        );
        assert_eq!(history.current.value.text, "ace");
        assert_eq!(
            history.current.selection,
            Selection {
                anchor: 2,
                active: 1
            }
        );
        assert_eq!(
            flatten(&history.current.value),
            vec![Style::default(), bold(true), Style::default()]
        );
        let after = history.current.clone();
        history.undo().unwrap();
        assert_eq!(history.current, before);
        history.redo().unwrap();
        assert_eq!(history.current, after);
    }

    #[test]
    fn bounded_history_reports_eviction_and_failed_edits_keep_redo() {
        assert!(matches!(
            DraftHistory::new(
                StyledText::<Style>::new(String::new(), vec![], LIMITS).unwrap(),
                Selection::caret(0),
                Style::default(),
                LIMITS,
                0
            ),
            Err(HistoryError::InvalidHistoryLimit)
        ));
        let mut history = history("", vec![], Selection::caret(0), Style::default(), 2);
        assert_eq!(history.replace_selection("a"), Ok(CommitOutcome::Recorded));
        assert_eq!(history.replace_selection("b"), Ok(CommitOutcome::Recorded));
        assert_eq!(
            history.replace_selection("c"),
            Ok(CommitOutcome::RecordedAfterEviction)
        );
        history.undo().unwrap();
        history.undo().unwrap();
        assert_eq!(history.current.value.text, "a");
        assert_eq!(history.undo(), Err(HistoryError::UndoUnavailable));
        let before = history.current.clone();
        assert_eq!(
            history.set_selection(Selection::caret(9)),
            Err(HistoryError::InvalidSelection)
        );
        assert_eq!(
            history.replace_selection(&"x".repeat(4097)),
            Err(HistoryError::Edit(EditError::TextLimit))
        );
        assert_eq!(history.current, before);
        history.redo().unwrap();
        assert_eq!(history.current.value.text, "ab");
        history.replace_selection("X").unwrap();
        assert_eq!(history.current.value.text, "abX");
        assert_eq!(history.redo(), Err(HistoryError::RedoUnavailable));
    }

    #[test]
    fn caret_changes_and_noop_edits_preserve_a_redo_branch() {
        let mut history = history("", vec![], Selection::caret(0), Style::default(), 2);
        history.replace_selection("abc").unwrap();
        let after = history.current.clone();
        history.undo().unwrap();
        assert_eq!(
            history.format_selection(|style| Style {
                bold: Some(false),
                ..style.clone()
            }),
            Ok(CommitOutcome::NoDocumentChange)
        );
        assert_eq!(
            history.replace_selection(""),
            Ok(CommitOutcome::NoDocumentChange)
        );
        history.redo().unwrap();
        assert_eq!(history.current, after);
    }

    #[test]
    fn unicode_edits_split_shift_and_coalesce_without_losing_explicit_false() {
        let mut value = StyledText::new(
            "aé🙂z".into(),
            vec![Span {
                range: 1..3,
                style: bold(true),
            }],
            LIMITS,
        )
        .unwrap();
        value.replace(2..2, "漢字", bold(true), LIMITS).unwrap();
        assert_eq!(value.text, "aé漢字🙂z");
        assert_eq!(
            value.spans,
            vec![Span {
                range: 1..5,
                style: bold(true)
            }]
        );
        value.replace(2..4, "", Style::default(), LIMITS).unwrap();
        value
            .format(
                0..2,
                |style| Style {
                    bold: Some(false),
                    ..style.clone()
                },
                LIMITS,
            )
            .unwrap();
        assert_eq!(value.text, "aé🙂z");
        assert_eq!(
            flatten(&value),
            vec![bold(false), bold(false), bold(true), Style::default()]
        );
    }

    #[test]
    fn replacements_use_explicit_operations_even_for_repeated_text() {
        let mut value = StyledText::new(
            "aaaa".into(),
            vec![Span {
                range: 1..3,
                style: bold(true),
            }],
            LIMITS,
        )
        .unwrap();
        value.replace(1..2, "a", bold(false), LIMITS).unwrap();
        assert_eq!(value.text, "aaaa");
        assert_eq!(
            flatten(&value),
            vec![Style::default(), bold(false), bold(true), Style::default()]
        );
    }

    #[test]
    fn successive_unicode_replacements_keep_styles_outside_the_changed_range() {
        let mut value = StyledText::new(
            "a🙂b".into(),
            vec![Span {
                range: 2..3,
                style: bold(true),
            }],
            LIMITS,
        )
        .unwrap();
        value.replace(1..2, "n", Style::default(), LIMITS).unwrap();
        value.replace(1..2, "に", Style::default(), LIMITS).unwrap();
        value
            .replace(1..2, "日本", Style::default(), LIMITS)
            .unwrap();
        assert_eq!(value.text, "a日本b");
        assert_eq!(
            flatten(&value),
            vec![
                Style::default(),
                Style::default(),
                Style::default(),
                bold(true)
            ]
        );
    }

    #[test]
    fn clearing_one_override_preserves_other_attributes_and_inheritance() {
        let both = Style {
            bold: Some(true),
            italic: Some(true),
        };
        let mut value = StyledText::new(
            "abc".into(),
            vec![
                Span {
                    range: 0..1,
                    style: both,
                },
                Span {
                    range: 1..2,
                    style: bold(false),
                },
            ],
            LIMITS,
        )
        .unwrap();
        value
            .format(
                0..3,
                |style| Style {
                    bold: None,
                    ..style.clone()
                },
                LIMITS,
            )
            .unwrap();
        assert_eq!(
            value.spans,
            vec![Span {
                range: 0..1,
                style: Style {
                    bold: None,
                    italic: Some(true),
                }
            }]
        );
    }

    #[test]
    fn invalid_ranges_or_limits_leave_text_and_styles_unchanged() {
        let mut value = StyledText::new(
            "abcd".into(),
            vec![Span {
                range: 0..4,
                style: bold(true),
            }],
            LIMITS,
        )
        .unwrap();
        let before = value.clone();
        assert_eq!(
            value.replace(3..2, "", Style::default(), LIMITS),
            Err(EditError::InvalidRange)
        );
        assert_eq!(
            value.replace(
                1..2,
                "long",
                Style::default(),
                Limits {
                    text_bytes: 4,
                    spans: 256,
                }
            ),
            Err(EditError::TextLimit)
        );
        assert_eq!(
            value.format(
                1..3,
                |_| bold(false),
                Limits {
                    text_bytes: 4096,
                    spans: 1,
                }
            ),
            Err(EditError::SpanLimit)
        );
        assert_eq!(
            value.format(
                2..2,
                |_| Style::default(),
                Limits {
                    text_bytes: 3,
                    spans: 256,
                }
            ),
            Err(EditError::TextLimit)
        );
        assert_eq!(value, before);
    }

    #[test]
    fn rejects_noncanonical_or_out_of_bounds_input() {
        for spans in [
            vec![Span {
                range: 1..1,
                style: bold(true),
            }],
            vec![Span {
                range: 0..3,
                style: bold(true),
            }],
            vec![Span {
                range: 0..1,
                style: Style::default(),
            }],
            vec![
                Span {
                    range: 0..1,
                    style: bold(true),
                },
                Span {
                    range: 1..2,
                    style: bold(true),
                },
            ],
            vec![
                Span {
                    range: 0..2,
                    style: bold(true),
                },
                Span {
                    range: 1..2,
                    style: bold(false),
                },
            ],
        ] {
            assert_eq!(
                StyledText::new("ab".into(), spans, LIMITS),
                Err(EditError::InvalidSpans)
            );
        }
    }

    #[test]
    fn operations_match_a_per_character_oracle() {
        let mut value = StyledText::<Style>::new(String::new(), vec![], LIMITS).unwrap();
        let mut chars = Vec::<char>::new();
        let mut styles = Vec::<Style>::new();
        let alphabet = ["a", "é", "🙂", "\u{301}", "\n", ""];
        let mut seed = 0x1265_6a09_u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed as usize
        };
        for _ in 0..2500 {
            let first = next() % (chars.len() + 1);
            let second = next() % (chars.len() + 1);
            let range = first.min(second)..first.max(second);
            if next() % 3 == 0 {
                let state = match next() % 3 {
                    0 => None,
                    1 => Some(false),
                    _ => Some(true),
                };
                value
                    .format(
                        range.clone(),
                        |style| Style {
                            italic: state,
                            ..style.clone()
                        },
                        LIMITS,
                    )
                    .unwrap();
                for style in &mut styles[range] {
                    style.italic = state;
                }
            } else {
                let inserted = alphabet[next() % alphabet.len()];
                let style = match next() % 3 {
                    0 => Style::default(),
                    1 => bold(false),
                    _ => bold(true),
                };
                value
                    .replace(range.clone(), inserted, style.clone(), LIMITS)
                    .unwrap();
                let inserted_chars: Vec<_> = inserted.chars().collect();
                chars.splice(range.clone(), inserted_chars.iter().copied());
                styles.splice(range, std::iter::repeat_n(style, inserted_chars.len()));
            }
            assert_eq!(value.text, chars.iter().collect::<String>());
            assert_eq!(flatten(&value), styles);
            value.validate(LIMITS).unwrap();
        }
    }

    #[test]
    fn history_and_composition_match_a_per_character_oracle() {
        let capacity = 8;
        let mut history = history(
            "aé🙂",
            vec![],
            Selection::caret(0),
            Style::default(),
            capacity,
        );
        let mut current = CharacterDraft {
            chars: "aé🙂".chars().collect(),
            styles: vec![Style::default(); 3],
            selection: Selection::caret(0),
            typing_style: Style::default(),
        };
        let mut undo = VecDeque::new();
        let mut redo = Vec::new();
        let mut seed = 0x4ea2_67af_u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed as usize
        };
        let alphabet = ["a", "é", "🙂", "e\u{301}", "\n", ""];
        for _ in 0..2500 {
            if current.chars.len() > 50 {
                let selection = Selection {
                    anchor: 0,
                    active: current.chars.len(),
                };
                history.set_selection(selection).unwrap();
                current.selection = selection;
            }
            match next() % 7 {
                0 => {
                    let selection = Selection {
                        anchor: next() % (current.chars.len() + 1),
                        active: next() % (current.chars.len() + 1),
                    };
                    history.set_selection(selection).unwrap();
                    current.selection = selection;
                }
                1 => {
                    let before = current.clone();
                    let range = current.selection.ordered();
                    let text = alphabet[next() % alphabet.len()];
                    let style = current.typing_style.clone();
                    let selection = Selection::caret(range.start + text.chars().count());
                    let outcome = history.replace_selection(text).unwrap();
                    oracle_replace(&mut current, range, text, &style, selection);
                    assert_eq!(
                        outcome,
                        oracle_record(before, &current, &mut undo, &mut redo, capacity)
                    );
                }
                2 => {
                    let before = current.clone();
                    let range = current.selection.ordered();
                    let bold_attribute = next().is_multiple_of(2);
                    let value = match next() % 3 {
                        0 => None,
                        1 => Some(false),
                        _ => Some(true),
                    };
                    let change = |style: &Style| {
                        let mut style = style.clone();
                        if bold_attribute {
                            style.bold = value;
                        } else {
                            style.italic = value;
                        }
                        style
                    };
                    let outcome = history.format_selection(change).unwrap();
                    if range.is_empty() {
                        current.typing_style = change(&current.typing_style);
                    } else {
                        for style in &mut current.styles[range] {
                            *style = change(style);
                        }
                    }
                    assert_eq!(
                        outcome,
                        oracle_record(before, &current, &mut undo, &mut redo, capacity)
                    );
                }
                3 => match undo.pop_back() {
                    Some(entry) => {
                        history.undo().unwrap();
                        current = entry.0.clone();
                        redo.push(entry);
                    }
                    None => assert_eq!(history.undo(), Err(HistoryError::UndoUnavailable)),
                },
                4 => match redo.pop() {
                    Some(entry) => {
                        history.redo().unwrap();
                        current = entry.1.clone();
                        undo.push_back(entry);
                    }
                    None => assert_eq!(history.redo(), Err(HistoryError::RedoUnavailable)),
                },
                5 => {
                    let before = current.clone();
                    let range = current.selection.ordered();
                    let left = next() % (range.start + 1);
                    let right = next() % (current.chars.len() - range.end + 1);
                    let outcome = history.delete_surrounding(left, right).unwrap();
                    oracle_delete_surrounding(&mut current, range, left, right);
                    assert_eq!(
                        outcome,
                        oracle_record(before, &current, &mut undo, &mut redo, capacity)
                    );
                }
                _ => {
                    let before = current.clone();
                    let mut range = current.selection.ordered();
                    let style = current.typing_style.clone();
                    history.begin_composition().unwrap();
                    for _ in 0..1 + next() % 3 {
                        let text = alphabet[next() % alphabet.len()];
                        let length = text.chars().count();
                        let relative = Selection {
                            anchor: next() % (length + 1),
                            active: next() % (length + 1),
                        };
                        history.update_composition(text, relative).unwrap();
                        let start = range.start;
                        oracle_replace(
                            &mut current,
                            range,
                            text,
                            &style,
                            Selection {
                                anchor: start + relative.anchor,
                                active: start + relative.active,
                            },
                        );
                        range = start..start + length;
                        assert_draft_matches(&history.current, &current);
                    }
                    if next().is_multiple_of(2) {
                        let left = next() % (range.start + 1);
                        let right = next() % (current.chars.len() - range.end + 1);
                        let outcome = history.delete_surrounding(left, right).unwrap();
                        assert_eq!(
                            outcome,
                            if left == 0 && right == 0 {
                                CommitOutcome::NoDocumentChange
                            } else {
                                CommitOutcome::Provisional
                            }
                        );
                        oracle_delete_surrounding(&mut current, range.clone(), left, right);
                        range = range.start - left..range.end - left;
                        assert_draft_matches(&history.current, &current);
                    }
                    if next().is_multiple_of(2) {
                        history.cancel_composition().unwrap();
                        current = before;
                    } else {
                        let text = alphabet[next() % alphabet.len()];
                        let length = text.chars().count();
                        let relative = Selection {
                            anchor: next() % (length + 1),
                            active: next() % (length + 1),
                        };
                        let start = range.start;
                        let outcome = history.commit_composition(text, relative).unwrap();
                        oracle_replace(
                            &mut current,
                            range,
                            text,
                            &style,
                            Selection {
                                anchor: start + relative.anchor,
                                active: start + relative.active,
                            },
                        );
                        assert_eq!(
                            outcome,
                            oracle_record(before, &current, &mut undo, &mut redo, capacity)
                        );
                    }
                }
            }
            assert_history_matches(&history, &current, &undo, &redo);
        }
    }
}
