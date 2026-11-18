//! Bounded, transactional draft state for an ordered rich-text input stream.

use std::{collections::VecDeque, ops::Range};

use crate::rich_text::{InlineText, InlineTextStyle, MAX_INLINE_TEXT_CHARS};

const MAX_HISTORY_STATES: usize = 64;
const MAX_HISTORY_BYTES: usize = 8 * 1024 * 1024;

/// A directional selection in Unicode scalar offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DraftSelection {
    pub anchor: usize,
    pub focus: usize,
}

impl DraftSelection {
    fn validate(self, text: &str) -> Result<(), String> {
        let length = text.chars().count();
        if self.anchor > length || self.focus > length {
            return Err("rich-text draft selection is out of range".into());
        }
        Ok(())
    }
}

/// One operation, applied in order against the result of the preceding operation.
#[derive(Debug, Clone, PartialEq)]
pub enum DraftOperation {
    Replace {
        range: Range<usize>,
        text: String,
        style: InlineTextStyle,
    },
    SetFormatting(InlineText),
}

#[derive(Debug, Clone, PartialEq)]
struct DraftState {
    text: String,
    formatting: InlineText,
    selection: DraftSelection,
    typing_style: Option<InlineTextStyle>,
}

impl DraftState {
    fn estimated_heap_bytes(&self) -> usize {
        self.text
            .capacity()
            .saturating_add(self.formatting.estimated_heap_bytes())
            .saturating_add(
                self.typing_style
                    .as_ref()
                    .and_then(|style| style.font_family.as_ref())
                    .map_or(0, String::capacity),
            )
    }

    fn same_content(&self, other: &Self) -> bool {
        self.text == other.text && self.formatting == other.formatting
    }
}

/// Rich draft content and local history; it does not mutate a document.
///
/// The caller supplies ordered operations and final selections. It must enforce
/// grapheme boundaries, route IME transactions, and check document ownership
/// and locks before applying the resulting content to the document.
#[derive(Debug, Clone)]
pub struct RichTextDraft {
    state: DraftState,
    undo: VecDeque<DraftState>,
    redo: VecDeque<DraftState>,
    composition: Option<DraftState>,
}

impl RichTextDraft {
    pub fn new(
        text: String,
        formatting: InlineText,
        selection: DraftSelection,
    ) -> Result<Self, String> {
        if text.chars().count() > MAX_INLINE_TEXT_CHARS {
            return Err(format!(
                "rich-text draft exceeds {MAX_INLINE_TEXT_CHARS} characters"
            ));
        }
        formatting.validate(&text)?;
        selection.validate(&text)?;
        Ok(Self {
            state: DraftState {
                text,
                formatting,
                selection,
                typing_style: None,
            },
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            composition: None,
        })
    }

    pub fn text(&self) -> &str {
        &self.state.text
    }

    pub fn formatting(&self) -> &InlineText {
        &self.state.formatting
    }

    pub fn selection(&self) -> DraftSelection {
        self.state.selection
    }

    pub fn typing_style(&self) -> Option<&InlineTextStyle> {
        self.state.typing_style.as_ref()
    }

    pub fn set_selection(&mut self, selection: DraftSelection) -> Result<(), String> {
        selection.validate(&self.state.text)?;
        self.state.selection = selection;
        Ok(())
    }

    pub fn set_typing_style(&mut self, style: Option<InlineTextStyle>) -> Result<(), String> {
        if let Some(style) = &style {
            style.validate()?;
        }
        self.state.typing_style = style;
        Ok(())
    }

    /// Apply one input transaction atomically; failed operations change nothing.
    pub fn apply(
        &mut self,
        operations: &[DraftOperation],
        selection: DraftSelection,
    ) -> Result<bool, String> {
        let mut candidate = self.state.clone();
        for operation in operations {
            match operation {
                DraftOperation::Replace { range, text, style } => {
                    let length = candidate.text.chars().count();
                    if range.start > range.end || range.end > length {
                        return Err("rich-text draft replacement is out of range".into());
                    }
                    let updated_length = length - (range.end - range.start);
                    if text.chars().count() > MAX_INLINE_TEXT_CHARS - updated_length {
                        return Err(format!(
                            "rich-text draft exceeds {MAX_INLINE_TEXT_CHARS} characters"
                        ));
                    }
                    (candidate.text, candidate.formatting) =
                        candidate.formatting.replace_label_range_with_style(
                            &candidate.text,
                            range.clone(),
                            text,
                            style,
                        )?;
                }
                DraftOperation::SetFormatting(formatting) => {
                    formatting.validate(&candidate.text)?;
                    candidate.formatting = formatting.clone();
                }
            }
        }
        selection.validate(&candidate.text)?;
        candidate.selection = selection;
        let changed = !candidate.same_content(&self.state);
        if changed && self.composition.is_none() {
            push_history(&mut self.undo, self.state.clone());
            self.redo.clear();
        }
        self.state = candidate;
        Ok(changed)
    }

    /// Group successive IME updates into one undo entry until commit.
    pub fn begin_composition(&mut self) -> Result<(), String> {
        if self.composition.is_some() {
            return Err("rich-text composition is already active".into());
        }
        self.composition = Some(self.state.clone());
        Ok(())
    }

    pub fn commit_composition(&mut self) -> Result<bool, String> {
        let original = self
            .composition
            .take()
            .ok_or_else(|| "rich-text composition is not active".to_owned())?;
        let changed = !self.state.same_content(&original);
        if changed {
            push_history(&mut self.undo, original);
            self.redo.clear();
        }
        Ok(changed)
    }

    pub fn cancel_composition(&mut self) -> Result<(), String> {
        self.state = self
            .composition
            .take()
            .ok_or_else(|| "rich-text composition is not active".to_owned())?;
        Ok(())
    }

    pub fn undo(&mut self) -> Result<bool, String> {
        if self.composition.is_some() {
            return Err("cannot undo during rich-text composition".into());
        }
        let Some(previous) = self.undo.pop_back() else {
            return Ok(false);
        };
        push_history(&mut self.redo, self.state.clone());
        self.state = previous;
        Ok(true)
    }

    pub fn redo(&mut self) -> Result<bool, String> {
        if self.composition.is_some() {
            return Err("cannot redo during rich-text composition".into());
        }
        let Some(next) = self.redo.pop_back() else {
            return Ok(false);
        };
        push_history(&mut self.undo, self.state.clone());
        self.state = next;
        Ok(true)
    }
}

fn push_history(history: &mut VecDeque<DraftState>, state: DraftState) {
    history.push_back(state);
    let mut bytes = history.iter().fold(0usize, |total, state| {
        total.saturating_add(state.estimated_heap_bytes())
    });
    while history.len() > MAX_HISTORY_STATES || bytes > MAX_HISTORY_BYTES {
        if let Some(oldest) = history.pop_front() {
            bytes = bytes.saturating_sub(oldest.estimated_heap_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rich_text::{InlineParagraph, InlineTextRun, ListKind, MAX_INLINE_TEXT_RUNS};

    fn selection(anchor: usize, focus: usize) -> DraftSelection {
        DraftSelection { anchor, focus }
    }

    fn replace(range: Range<usize>, text: &str, style: InlineTextStyle) -> DraftOperation {
        DraftOperation::Replace {
            range,
            text: text.into(),
            style,
        }
    }

    #[test]
    fn ordered_edits_preserve_repeated_text_styles_and_directional_selection() {
        let mut rich = InlineText::default();
        rich.update_range("aaa", 1..2, |style| style.bold = Some(true))
            .unwrap();
        let mut draft = RichTextDraft::new("aaa".into(), rich.clone(), selection(2, 1)).unwrap();
        let italic = InlineTextStyle {
            italic: Some(true),
            ..Default::default()
        };
        draft
            .apply(
                &[
                    replace(1..2, "", InlineTextStyle::default()),
                    replace(1..1, "a", italic.clone()),
                ],
                selection(2, 2),
            )
            .unwrap();
        assert_eq!(draft.text(), "aaa");
        assert_eq!(draft.formatting().runs[0].style, italic);
        assert!(draft.undo().unwrap());
        assert_eq!(draft.formatting(), &rich);
        assert_eq!(draft.selection(), selection(2, 1));
        assert!(draft.redo().unwrap());
        assert_eq!(draft.formatting().runs[0].style, italic);
        assert_eq!(draft.selection(), selection(2, 2));
    }

    #[test]
    fn rejected_transaction_keeps_text_selection_styles_and_history() {
        let mut draft =
            RichTextDraft::new("a".into(), InlineText::default(), selection(1, 0)).unwrap();
        let before = draft.clone();
        assert!(
            draft
                .apply(
                    &[
                        replace(0..1, "", InlineTextStyle::default()),
                        replace(1..1, "x", InlineTextStyle::default()),
                    ],
                    selection(1, 1),
                )
                .is_err()
        );
        assert_eq!(draft.state, before.state);
        assert!(!draft.undo().unwrap());
        assert!(
            draft
                .apply(
                    &[replace(
                        1..1,
                        &"x".repeat(MAX_INLINE_TEXT_CHARS),
                        InlineTextStyle::default()
                    )],
                    selection(1, 1),
                )
                .is_err()
        );
        assert_eq!(draft.state, before.state);
    }

    #[test]
    fn accepted_prior_event_survives_a_later_rejected_event() {
        let mut draft =
            RichTextDraft::new("a".into(), InlineText::default(), selection(1, 1)).unwrap();
        draft
            .apply(
                &[replace(1..1, "b", InlineTextStyle::default())],
                selection(2, 2),
            )
            .unwrap();
        assert!(
            draft
                .apply(
                    &[replace(
                        1..2,
                        &"x".repeat(MAX_INLINE_TEXT_CHARS),
                        InlineTextStyle::default()
                    )],
                    selection(1, 1),
                )
                .is_err()
        );
        assert_eq!(draft.text(), "ab");
        assert_eq!(draft.selection(), selection(2, 2));
        draft.undo().unwrap();
        assert_eq!(draft.text(), "a");
    }

    #[test]
    fn formatting_only_edits_are_undoable_and_invalid_selection_is_atomic() {
        let mut draft =
            RichTextDraft::new("abc".into(), InlineText::default(), selection(3, 0)).unwrap();
        let mut formatted = InlineText::default();
        formatted
            .update_range("abc", 1..2, |style| style.bold = Some(true))
            .unwrap();
        assert!(
            draft
                .apply(
                    &[DraftOperation::SetFormatting(formatted.clone())],
                    selection(0, 4)
                )
                .is_err()
        );
        assert!(draft.formatting().is_empty());
        draft
            .apply(
                &[DraftOperation::SetFormatting(formatted.clone())],
                selection(3, 0),
            )
            .unwrap();
        assert!(draft.undo().unwrap());
        assert!(draft.formatting().is_empty());
        assert!(draft.redo().unwrap());
        assert_eq!(draft.formatting(), &formatted);
    }

    #[test]
    fn composition_groups_updates_and_restores_cancelled_redo() {
        let mut draft =
            RichTextDraft::new("a".into(), InlineText::default(), selection(1, 1)).unwrap();
        draft
            .apply(
                &[replace(1..1, "b", InlineTextStyle::default())],
                selection(2, 2),
            )
            .unwrap();
        draft.undo().unwrap();
        draft.begin_composition().unwrap();
        draft
            .apply(
                &[replace(1..1, "n", InlineTextStyle::default())],
                selection(2, 2),
            )
            .unwrap();
        assert!(draft.undo().is_err());
        draft.cancel_composition().unwrap();
        assert!(draft.redo().unwrap());
        assert_eq!(draft.text(), "ab");
        draft.begin_composition().unwrap();
        draft
            .apply(
                &[replace(2..2, "ni", InlineTextStyle::default())],
                selection(4, 2),
            )
            .unwrap();
        draft
            .apply(
                &[replace(2..4, "日本", InlineTextStyle::default())],
                selection(4, 2),
            )
            .unwrap();
        assert!(draft.commit_composition().unwrap());
        assert_eq!(draft.text(), "ab日本");
        assert!(draft.undo().unwrap());
        assert_eq!(draft.text(), "ab");
        assert!(draft.redo().unwrap());
        assert_eq!(draft.text(), "ab日本");
        assert_eq!(draft.selection(), selection(4, 2));
    }

    #[test]
    fn edits_and_undo_preserve_list_paragraphs_and_typing_style() {
        let rich = InlineText {
            paragraphs: vec![InlineParagraph {
                index: 0,
                list: ListKind::Bulleted,
            }],
            ..Default::default()
        };
        let mut draft = RichTextDraft::new("one".into(), rich.clone(), selection(3, 3)).unwrap();
        let style = InlineTextStyle {
            italic: Some(true),
            ..Default::default()
        };
        draft.set_typing_style(Some(style.clone())).unwrap();
        draft
            .apply(&[replace(3..3, "\n🙂", style.clone())], selection(5, 5))
            .unwrap();
        assert_eq!(draft.formatting().paragraphs.len(), 2);
        assert_eq!(draft.formatting().runs[0].style, style);
        draft.set_typing_style(None).unwrap();
        draft.undo().unwrap();
        assert_eq!(draft.formatting(), &rich);
        assert_eq!(draft.typing_style(), Some(&style));
        draft.redo().unwrap();
        assert_eq!(draft.text(), "one\n🙂");
    }

    #[test]
    fn history_is_bounded_and_new_edits_clear_redo() {
        let mut draft =
            RichTextDraft::new(String::new(), InlineText::default(), selection(0, 0)).unwrap();
        for index in 0..MAX_HISTORY_STATES + 10 {
            draft
                .apply(
                    &[replace(index..index, "x", InlineTextStyle::default())],
                    selection(index + 1, index + 1),
                )
                .unwrap();
        }
        assert_eq!(draft.undo.len(), MAX_HISTORY_STATES);
        draft.undo().unwrap();
        let length = draft.text().chars().count();
        draft
            .apply(
                &[replace(length..length, "y", InlineTextStyle::default())],
                selection(length + 1, length + 1),
            )
            .unwrap();
        assert!(!draft.redo().unwrap());
    }

    #[test]
    fn history_limits_retained_run_and_font_memory() {
        let text = "x".repeat(MAX_INLINE_TEXT_RUNS);
        let rich = InlineText {
            runs: (0..MAX_INLINE_TEXT_RUNS)
                .map(|index| InlineTextRun {
                    start: index as u32,
                    end: index as u32 + 1,
                    style: InlineTextStyle {
                        bold: Some(index % 2 == 0),
                        font_family: Some("f".repeat(128)),
                        ..Default::default()
                    },
                })
                .collect(),
            paragraphs: Vec::new(),
        };
        let mut draft = RichTextDraft::new(text, rich, selection(0, 0)).unwrap();
        for index in 0..MAX_HISTORY_STATES {
            let end = MAX_INLINE_TEXT_RUNS + index;
            draft
                .apply(
                    &[replace(end..end, "x", InlineTextStyle::default())],
                    selection(end + 1, end + 1),
                )
                .unwrap();
        }
        let bytes: usize = draft
            .undo
            .iter()
            .map(DraftState::estimated_heap_bytes)
            .sum();
        assert!(bytes <= MAX_HISTORY_BYTES);
        assert!(draft.undo.len() < MAX_HISTORY_STATES);
        assert!(draft.undo().unwrap());
    }
}
