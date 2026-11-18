use std::{any::TypeId, cell::RefCell, ops::Range};

use egui::{TextBuffer, text::CharIndex};
use thinoda_core::{
    label_style::LabelStyle,
    rich_text::{InlineBaseline, InlineParagraph, InlineText, InlineTextStyle, ListKind},
    rich_text_draft::{DraftSelection, RichTextDraft},
};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Format {
    Bold,
    Italic,
    Underline,
    Strike,
    Subscript,
    Superscript,
    Bulleted,
    Numbered,
}

impl Format {
    pub(super) fn list(self) -> Option<ListKind> {
        match self {
            Self::Bulleted => Some(ListKind::Bulleted),
            Self::Numbered => Some(ListKind::Numbered),
            _ => None,
        }
    }

    fn enabled(self, style: &InlineTextStyle, base: &LabelStyle) -> bool {
        match self {
            Self::Bold => style.bold.unwrap_or(base.bold),
            Self::Italic => style.italic.unwrap_or(base.italic),
            Self::Underline => style.underline.unwrap_or(base.underline),
            Self::Strike => style.strikethrough.unwrap_or(base.strikethrough),
            Self::Subscript => style.baseline == Some(InlineBaseline::Subscript),
            Self::Superscript => style.baseline == Some(InlineBaseline::Superscript),
            Self::Bulleted | Self::Numbered => false,
        }
    }

    fn set(self, style: &mut InlineTextStyle, base: &LabelStyle, enabled: bool) {
        match self {
            Self::Bold => style.bold = (enabled != base.bold).then_some(enabled),
            Self::Italic => style.italic = (enabled != base.italic).then_some(enabled),
            Self::Underline => style.underline = (enabled != base.underline).then_some(enabled),
            Self::Strike => {
                style.strikethrough = (enabled != base.strikethrough).then_some(enabled)
            }
            Self::Subscript => {
                style.baseline = enabled.then_some(InlineBaseline::Subscript);
            }
            Self::Superscript => {
                style.baseline = enabled.then_some(InlineBaseline::Superscript);
            }
            Self::Bulleted | Self::Numbered => {}
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct TypingStyle {
    pub position: usize,
    pub style: InlineTextStyle,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum HistoryInput {
    Typing,
    Ime,
    Other,
}

pub(super) struct HistoryEvent {
    pub input: HistoryInput,
    pub time: f64,
    pub ime_composing: bool,
}

#[derive(Clone, Copy)]
enum HistoryGroup {
    Typing { started: f64, last_change: f64 },
    Ime,
}

pub(super) struct LabelHistory {
    draft: RichTextDraft,
    group: Option<HistoryGroup>,
}

impl LabelHistory {
    pub(super) fn new(
        text: String,
        rich: InlineText,
        selection: DraftSelection,
        typing: Option<InlineTextStyle>,
    ) -> Result<Self, String> {
        let mut draft = RichTextDraft::new(text, rich, selection)?;
        draft.set_typing_style(typing)?;
        Ok(Self { draft, group: None })
    }

    pub(super) fn draft(&self) -> &RichTextDraft {
        &self.draft
    }

    pub(super) fn set_context(
        &mut self,
        selection: DraftSelection,
        typing: Option<InlineTextStyle>,
    ) -> Result<(), String> {
        self.draft.set_selection(selection)?;
        self.draft.set_typing_style(typing)
    }

    fn finish_group(&mut self) -> Result<(), String> {
        if self.group.take().is_some() {
            self.draft.commit_composition()?;
        }
        Ok(())
    }

    pub(super) fn record(
        &mut self,
        text: &str,
        rich: &InlineText,
        selection: DraftSelection,
        typing: Option<InlineTextStyle>,
        event: HistoryEvent,
    ) -> Result<(), String> {
        let HistoryEvent {
            input,
            time: now,
            ime_composing,
        } = event;
        let changed = self.draft.text() != text || self.draft.formatting() != rich;
        if changed || self.draft.typing_style() != typing.as_ref() {
            // Validate before opening a group so rejection cannot disturb history.
            let mut candidate = RichTextDraft::new(text.to_owned(), rich.clone(), selection)?;
            candidate.set_typing_style(typing.clone())?;
        }
        if matches!(self.group, Some(HistoryGroup::Typing { started, last_change })
            if now - last_change >= 1.0 || now - started >= 30.0)
            || matches!(
                (self.group, input),
                (Some(HistoryGroup::Typing { .. }), HistoryInput::Ime)
                    | (Some(HistoryGroup::Ime), HistoryInput::Typing)
            )
            || (changed && input == HistoryInput::Other && self.group.is_some())
        {
            self.finish_group()?;
        }
        if changed {
            if self.group.is_none() {
                match input {
                    HistoryInput::Typing => {
                        self.draft.begin_composition()?;
                        self.group = Some(HistoryGroup::Typing {
                            started: now,
                            last_change: now,
                        });
                    }
                    HistoryInput::Ime => {
                        self.draft.begin_composition()?;
                        self.group = Some(HistoryGroup::Ime);
                    }
                    HistoryInput::Other => {}
                }
            }
            self.draft
                .replace_content(text.to_owned(), rich.clone(), selection)?;
            if let Some(HistoryGroup::Typing { last_change, .. }) = &mut self.group {
                *last_change = now;
            }
        } else {
            self.draft.set_selection(selection)?;
        }
        self.draft.set_typing_style(typing)?;
        if matches!(self.group, Some(HistoryGroup::Ime)) && !ime_composing {
            self.finish_group()?;
        }
        Ok(())
    }

    pub(super) fn undo(&mut self) -> Result<bool, String> {
        self.finish_group()?;
        self.draft.undo()
    }

    pub(super) fn redo(&mut self) -> Result<bool, String> {
        self.finish_group()?;
        self.draft.redo()
    }
}

#[derive(Clone, Copy)]
pub(super) enum Deletion {
    Backward,
    Forward,
    Selection,
}

#[derive(Debug)]
pub(super) enum TextOperation {
    Insert { index: usize, text: String },
    Delete(Range<usize>),
    Replace,
}

pub(super) struct TrackedBuffer<'a> {
    pub text: &'a mut String,
    pub operations: &'a RefCell<Vec<TextOperation>>,
    pub track: bool,
}

impl TextBuffer for TrackedBuffer<'_> {
    fn is_mutable(&self) -> bool {
        true
    }

    fn as_str(&self) -> &str {
        self.text
    }

    fn insert_text(&mut self, text: &str, index: CharIndex) -> usize {
        if self.track {
            self.operations.borrow_mut().push(TextOperation::Insert {
                index: index.0,
                text: text.to_owned(),
            });
        }
        self.text.insert_text(text, index)
    }

    fn delete_char_range(&mut self, range: Range<CharIndex>) {
        if self.track {
            self.operations
                .borrow_mut()
                .push(TextOperation::Delete(range.start.0..range.end.0));
        }
        self.text.delete_char_range(range);
    }

    fn replace_with(&mut self, text: &str) {
        if self.track {
            self.operations.borrow_mut().push(TextOperation::Replace);
        }
        self.text.replace_with(text);
    }

    fn type_id(&self) -> TypeId {
        TypeId::of::<TrackedBuffer<'static>>()
    }
}

pub(super) fn reconcile_tracked_surrounding_deletion(
    rich: &InlineText,
    before: &str,
    after: &str,
    typing: &Option<TypingStyle>,
    operations: &[TextOperation],
) -> Result<Option<(InlineText, Option<TypingStyle>)>, String> {
    let mut text = before.to_owned();
    let mut updated = rich.clone();
    let mut typing = typing.clone();
    for operation in operations {
        let TextOperation::Delete(range) = operation else {
            return Ok(None);
        };
        let length = text.chars().count();
        let range = range.start.min(length)..range.end.min(length);
        if range.start > range.end {
            return Ok(None);
        }
        (text, updated) = updated.replace_label_range(&text, range.clone(), "")?;
        if let Some(pending) = &mut typing {
            pending.position = if pending.position > range.end {
                pending.position - (range.end - range.start)
            } else {
                pending.position.min(range.start)
            };
        }
    }
    Ok((text == after).then_some((updated, typing)))
}

pub(super) fn reconcile_tracked_edit(
    rich: &InlineText,
    before: &str,
    after: &str,
    typing: &Option<TypingStyle>,
    operations: &[TextOperation],
    deletion: Option<Deletion>,
) -> Result<Option<(InlineText, Option<TypingStyle>)>, String> {
    let (range, replacement) = match operations {
        [TextOperation::Insert { index, text }] if deletion.is_none() => {
            (*index..*index, text.as_str())
        }
        [
            TextOperation::Delete(range),
            TextOperation::Insert { index, text },
        ] if deletion.is_none() && range.start == *index => (range.clone(), text.as_str()),
        [TextOperation::Delete(range)] if deletion.is_some() => (range.clone(), ""),
        _ => return Ok(None),
    };
    if range.start > range.end || range.end > before.chars().count() {
        return Ok(None);
    }
    let byte_at = |index: usize| {
        before
            .char_indices()
            .nth(index)
            .map_or(before.len(), |(byte, _)| byte)
    };
    let start = byte_at(range.start);
    let end = byte_at(range.end);
    if after.len() != start + replacement.len() + before.len() - end
        || !after.starts_with(&before[..start])
        || !after.ends_with(&before[end..])
        || after.get(start..start + replacement.len()) != Some(replacement)
    {
        return Ok(None);
    }
    let pending = typing.as_ref().filter(|pending| match deletion {
        Some(Deletion::Backward) => pending.position == range.end,
        Some(Deletion::Forward) => pending.position == range.start,
        Some(Deletion::Selection) => false,
        None => range.is_empty() && pending.position == range.start,
    });
    let (text, mut updated) = if let Some(pending) = pending {
        if deletion.is_some() {
            rich.replace_label_range(before, range.clone(), replacement)?
        } else {
            rich.replace_label_range_with_style(before, range.clone(), replacement, &pending.style)?
        }
    } else {
        rich.replace_label_range(before, range.clone(), replacement)?
    };
    debug_assert_eq!(text, after);
    let mut next_typing = pending.cloned();
    if let Some(pending) = &mut next_typing {
        let inserted = range.start..range.start + replacement.chars().count();
        if deletion.is_none() {
            let graphemes = grapheme_range(after, inserted.clone())?;
            if graphemes != inserted {
                updated.update_range(after, graphemes, |style| *style = pending.style.clone())?;
            }
        }
        pending.position = inserted.end;
    }
    Ok(Some((updated, next_typing)))
}

pub(super) fn style_at_caret(rich: &InlineText, position: usize) -> InlineTextStyle {
    style_at(rich, position.saturating_sub(1))
}

fn style_at(rich: &InlineText, index: usize) -> InlineTextStyle {
    rich.runs
        .iter()
        .find(|run| run.start as usize <= index && index < run.end as usize)
        .map_or_else(InlineTextStyle::default, |run| run.style.clone())
}

pub(super) fn selection_value<T: PartialEq>(
    rich: &InlineText,
    text: &str,
    range: Range<usize>,
    value: impl Fn(&InlineTextStyle) -> T,
) -> Option<T> {
    let range = grapheme_range(text, range).ok()?;
    if range.is_empty() {
        return None;
    }
    let first = value(&style_at(rich, range.start));
    let inherited = value(&InlineTextStyle::default());
    let mut cursor = range.start;
    for run in &rich.runs {
        let start = run.start as usize;
        let end = run.end as usize;
        if end <= range.start {
            continue;
        }
        if start >= range.end {
            break;
        }
        if cursor < start && inherited != first || value(&run.style) != first {
            return None;
        }
        cursor = cursor.max(end);
    }
    (cursor >= range.end || inherited == first).then_some(first)
}

pub(super) fn active_at_caret(style: &InlineTextStyle, base: &LabelStyle, format: Format) -> bool {
    format.enabled(style, base)
}

pub(super) fn toggle_at_caret(style: &mut InlineTextStyle, base: &LabelStyle, format: Format) {
    let enabled = !format.enabled(style, base);
    format.set(style, base, enabled);
}

fn grapheme_range(text: &str, range: Range<usize>) -> Result<Range<usize>, String> {
    let count = text.chars().count();
    if range.start > range.end || range.end > count {
        return Err("inline formatting selection is out of range".into());
    }
    if range.is_empty() {
        return Ok(range);
    }
    let mut result = range.clone();
    let mut start = 0;
    for grapheme in text.graphemes(true) {
        let end = start + grapheme.chars().count();
        if start < range.start && range.start < end {
            result.start = start;
        }
        if start < range.end && range.end < end {
            result.end = end;
        }
        start = end;
    }
    Ok(result)
}

pub(super) fn grapheme_deletion_range(
    text: &str,
    selection: Range<usize>,
    backward: bool,
) -> Result<Range<usize>, String> {
    let length = text.chars().count();
    if selection.start > selection.end || selection.end > length {
        return Err("inline deletion selection is out of range".into());
    }
    let range = if !selection.is_empty() {
        selection
    } else if backward && selection.start > 0 {
        selection.start - 1..selection.start
    } else if !backward && selection.start < length {
        selection.start..selection.start + 1
    } else {
        selection
    };
    grapheme_range(text, range)
}

pub(super) fn grapheme_replacement_range(
    text: &str,
    selection: Range<usize>,
    snap_caret: bool,
) -> Result<Range<usize>, String> {
    let length = text.chars().count();
    if selection.start > selection.end || selection.end > length {
        return Err("inline replacement selection is out of range".into());
    }
    if !selection.is_empty() {
        grapheme_range(text, selection)
    } else if snap_caret && selection.start < length {
        let cluster = grapheme_range(text, selection.start..selection.start + 1)?;
        let position = if cluster.start < selection.start {
            cluster.end
        } else {
            selection.start
        };
        Ok(position..position)
    } else {
        Ok(selection)
    }
}

pub(super) fn apply_style(
    rich: &mut InlineText,
    text: &str,
    range: Range<usize>,
    update: impl FnMut(&mut InlineTextStyle),
) -> Result<(), String> {
    let range = grapheme_range(text, range)?;
    rich.update_range(text, range, update)
}

pub(super) fn reconcile_typing(
    rich: &InlineText,
    before: &str,
    after: &str,
    typing: &mut Option<TypingStyle>,
    deletion: Option<Deletion>,
    require_pending_match: bool,
) -> Result<InlineText, String> {
    if before == after {
        return Ok(rich.clone());
    }
    if let Some(pending) = typing.as_mut() {
        let before_count = before.chars().count();
        let after_count = after.chars().count();
        if pending.position <= before_count
            && let Some(added) = after_count
                .checked_sub(before_count)
                .filter(|added| *added > 0)
        {
            let byte_at = |text: &str, index: usize| {
                text.char_indices()
                    .nth(index)
                    .map_or(text.len(), |(byte, _)| byte)
            };
            let before_at = byte_at(before, pending.position);
            let after_at = byte_at(after, pending.position);
            let after_end = byte_at(after, pending.position + added);
            if before[..before_at] == after[..after_at] && before[before_at..] == after[after_end..]
            {
                let inserted_range = pending.position..pending.position + added;
                let (text, mut updated) = rich.replace_label_range_with_style(
                    before,
                    pending.position..pending.position,
                    &after[after_at..after_end],
                    &pending.style,
                )?;
                debug_assert_eq!(text, after);
                let range = grapheme_range(after, inserted_range.clone())?;
                if range != inserted_range {
                    updated.update_range(after, range, |style| *style = pending.style.clone())?;
                }
                pending.position += added;
                return Ok(updated);
            }
        }
        if let Some(removed) = before_count
            .checked_sub(after_count)
            .filter(|removed| *removed > 0)
        {
            let range = match deletion {
                Some(Deletion::Backward) => pending
                    .position
                    .checked_sub(removed)
                    .map(|start| start..pending.position),
                Some(Deletion::Forward) => pending
                    .position
                    .checked_add(removed)
                    .filter(|end| *end <= before_count)
                    .map(|end| pending.position..end),
                Some(Deletion::Selection) | None => None,
            };
            if let Some(range) = range {
                let (text, updated) = rich.replace_label_range(before, range.clone(), "")?;
                if text == after {
                    pending.position = range.start;
                    return Ok(updated);
                }
            }
        }
        if require_pending_match {
            return Err(
                "Pending formatting could not be reconciled; correct or undo the edit".into(),
            );
        }
        *typing = None;
    }
    rich.reconcile_label(before, after)
}

fn paragraphs(text: &str, range: &Range<usize>) -> Range<usize> {
    let first = text
        .chars()
        .take(range.start)
        .filter(|ch| *ch == '\n')
        .count();
    let mut last = text
        .chars()
        .take(range.end)
        .filter(|ch| *ch == '\n')
        .count();
    if !range.is_empty() && text.chars().nth(range.end - 1) == Some('\n') {
        last -= 1;
    }
    first..last + 1
}

pub(super) fn active(
    rich: &InlineText,
    text: &str,
    range: Range<usize>,
    base: &LabelStyle,
    format: Format,
) -> bool {
    let Ok(range) = grapheme_range(text, range) else {
        return false;
    };
    if let Some(list) = format.list() {
        return paragraphs(text, &range).all(|index| {
            rich.paragraphs
                .iter()
                .any(|paragraph| paragraph.index as usize == index && paragraph.list == list)
        });
    }
    if range.is_empty() {
        return false;
    }
    let inherited = format.enabled(&InlineTextStyle::default(), base);
    let mut cursor = range.start;
    for run in &rich.runs {
        let start = run.start as usize;
        let end = run.end as usize;
        if end <= range.start {
            continue;
        }
        if start >= range.end {
            break;
        }
        if cursor < start && !inherited || !format.enabled(&run.style, base) {
            return false;
        }
        cursor = cursor.max(end);
    }
    cursor >= range.end || inherited
}

pub(super) fn apply(
    rich: &mut InlineText,
    text: &str,
    range: Range<usize>,
    base: &LabelStyle,
    format: Format,
) -> Result<(), String> {
    rich.validate(text)?;
    let range = grapheme_range(text, range)?;
    let enable = !active(rich, text, range.clone(), base, format);
    let mut candidate = rich.clone();
    if let Some(list) = format.list() {
        let paragraphs = paragraphs(text, &range);
        candidate
            .paragraphs
            .retain(|paragraph| !paragraphs.contains(&(paragraph.index as usize)));
        if enable {
            for index in paragraphs {
                candidate.paragraphs.push(InlineParagraph {
                    index: u32::try_from(index)
                        .map_err(|_| "inline paragraph index is too large")?,
                    list,
                });
            }
        }
        candidate.paragraphs.sort_unstable_by_key(|item| item.index);
        candidate.validate(text)?;
    } else {
        if range.is_empty() {
            return Err("select text before applying inline formatting".into());
        }
        candidate.update_range(text, range, |style| format.set(style, base, enable))?;
    }
    *rich = candidate;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surrounding_deletion_clamps_ranges_and_remaps_pending_caret() {
        let mut rich = InlineText::default();
        rich.update_range("aaa", 1..2, |style| style.bold = Some(true))
            .unwrap();
        let typing = Some(TypingStyle {
            position: 2,
            style: InlineTextStyle::default(),
        });
        let operations = [
            TextOperation::Delete(2..usize::MAX),
            TextOperation::Delete(0..1),
        ];
        let (updated, pending) =
            reconcile_tracked_surrounding_deletion(&rich, "aaa", "a", &typing, &operations)
                .unwrap()
                .unwrap();
        assert_eq!((updated.runs[0].start, updated.runs[0].end), (0, 1));
        assert_eq!(pending.unwrap().position, 1);
        assert!(
            reconcile_tracked_surrounding_deletion(&rich, "aaa", "aa", &typing, &operations)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn label_history_groups_typing_and_composition_without_losing_styles() {
        let selection = |index| DraftSelection {
            anchor: index,
            focus: index,
        };
        let mut original = InlineText::default();
        original
            .update_range("aaa", 1..2, |style| style.bold = Some(true))
            .unwrap();
        let mut history =
            LabelHistory::new("aaa".into(), original.clone(), selection(1), None).unwrap();
        let (text, first) = original.replace_label_range("aaa", 1..1, "a").unwrap();
        history
            .record(
                &text,
                &first,
                selection(2),
                None,
                HistoryEvent {
                    input: HistoryInput::Typing,
                    time: 0.1,
                    ime_composing: false,
                },
            )
            .unwrap();
        let (text, second) = first.replace_label_range(&text, 2..2, "a").unwrap();
        history
            .record(
                &text,
                &second,
                selection(3),
                None,
                HistoryEvent {
                    input: HistoryInput::Typing,
                    time: 0.2,
                    ime_composing: false,
                },
            )
            .unwrap();
        assert!(history.undo().unwrap());
        assert_eq!(history.draft().text(), "aaa");
        assert_eq!(history.draft().formatting(), &original);
        assert_eq!(history.draft().selection(), selection(1));
        assert!(history.redo().unwrap());
        assert_eq!(history.draft().text(), text);
        assert_eq!(history.draft().formatting(), &second);

        history
            .record(
                "preedit",
                &InlineText::default(),
                selection(7),
                None,
                HistoryEvent {
                    input: HistoryInput::Ime,
                    time: 0.3,
                    ime_composing: true,
                },
            )
            .unwrap();
        history
            .record(
                "commit",
                &InlineText::default(),
                selection(6),
                None,
                HistoryEvent {
                    input: HistoryInput::Ime,
                    time: 0.4,
                    ime_composing: false,
                },
            )
            .unwrap();
        assert!(history.undo().unwrap());
        assert_eq!(history.draft().text(), text);
        assert_eq!(history.draft().formatting(), &second);
    }

    #[test]
    fn label_history_bounds_groups_and_keeps_redo_after_rejection() {
        let selection = |index| DraftSelection {
            anchor: index,
            focus: index,
        };
        let plain = InlineText::default();
        let event = |input, time| HistoryEvent {
            input,
            time,
            ime_composing: false,
        };
        let mut history = LabelHistory::new("a".into(), plain.clone(), selection(1), None).unwrap();
        history
            .record(
                "ab",
                &plain,
                selection(2),
                None,
                event(HistoryInput::Typing, 0.1),
            )
            .unwrap();
        history
            .record(
                "ab",
                &plain,
                selection(2),
                None,
                event(HistoryInput::Other, 1.2),
            )
            .unwrap();
        history
            .record(
                "abc",
                &plain,
                selection(3),
                None,
                event(HistoryInput::Typing, 1.3),
            )
            .unwrap();
        assert!(history.undo().unwrap());
        assert_eq!(history.draft().text(), "ab");
        assert!(
            history
                .record(
                    &"x".repeat(thinoda_core::MAX_INLINE_TEXT_CHARS + 1),
                    &plain,
                    selection(0),
                    None,
                    event(HistoryInput::Typing, 1.4),
                )
                .is_err()
        );
        assert!(history.redo().unwrap());
        assert_eq!(history.draft().text(), "abc");
        assert!(history.undo().unwrap());
        history
            .record(
                "abd",
                &plain,
                selection(3),
                None,
                event(HistoryInput::Typing, 1.5),
            )
            .unwrap();
        assert!(!history.redo().unwrap());
    }

    #[test]
    fn tracked_insertions_preserve_pending_grapheme_style_and_reject_other_traces() {
        let rich = InlineText::default();
        let typing = Some(TypingStyle {
            position: 1,
            style: InlineTextStyle {
                bold: Some(true),
                ..Default::default()
            },
        });
        let operations = [
            TextOperation::Delete(1..1),
            TextOperation::Insert {
                index: 1,
                text: "\u{301}".into(),
            },
        ];
        let (updated, pending) =
            reconcile_tracked_edit(&rich, "ab", "a\u{301}b", &typing, &operations, None)
                .unwrap()
                .unwrap();
        assert_eq!((updated.runs[0].start, updated.runs[0].end), (0, 2));
        assert_eq!(updated.runs[0].style.bold, Some(true));
        assert_eq!(pending.unwrap().position, 2);

        assert!(
            reconcile_tracked_edit(&rich, "ab", "axb", &typing, &operations, None)
                .unwrap()
                .is_none()
        );
        assert!(
            reconcile_tracked_edit(
                &rich,
                "ab",
                "a\u{301}b",
                &typing,
                &[TextOperation::Replace],
                None,
            )
            .unwrap()
            .is_none()
        );
    }

    #[test]
    fn tracked_deletion_keeps_pending_typing_style_at_the_deleted_range() {
        for (direction, position) in [(Deletion::Backward, 2), (Deletion::Forward, 1)] {
            let typing = Some(TypingStyle {
                position,
                style: InlineTextStyle {
                    bold: Some(true),
                    ..Default::default()
                },
            });
            let (_, pending) = reconcile_tracked_edit(
                &InlineText::default(),
                "aaa",
                "aa",
                &typing,
                &[TextOperation::Delete(1..2)],
                Some(direction),
            )
            .unwrap()
            .unwrap();
            let pending = pending.unwrap();
            assert_eq!(pending.position, 1);
            assert_eq!(pending.style.bold, Some(true));
        }
    }

    #[test]
    fn toggle_preserves_other_overrides_and_whole_label_inheritance() {
        let mut rich = InlineText::default();
        let base = LabelStyle {
            bold: true,
            ..Default::default()
        };
        rich.update_range("abcd", 1..3, |style| style.italic = Some(true))
            .unwrap();
        apply(&mut rich, "abcd", 0..4, &base, Format::Bold).unwrap();
        assert!(rich.runs.iter().all(|run| run.style.bold == Some(false)));
        assert_eq!(rich.runs[1].style.italic, Some(true));
        apply(&mut rich, "abcd", 0..4, &base, Format::Bold).unwrap();
        assert_eq!(rich.runs.len(), 1);
        assert_eq!(rich.runs[0].style.italic, Some(true));
        assert_eq!(rich.runs[0].style.bold, None);
    }

    #[test]
    fn formatting_expands_combining_and_joined_emoji_selections() {
        let text = "a\u{301}👩\u{200d}💻z";
        let mut rich = InlineText::default();
        apply(&mut rich, text, 1..2, &LabelStyle::default(), Format::Bold).unwrap();
        apply(
            &mut rich,
            text,
            3..4,
            &LabelStyle::default(),
            Format::Italic,
        )
        .unwrap();
        assert_eq!((rich.runs[0].start, rich.runs[0].end), (0, 2));
        assert_eq!((rich.runs[1].start, rich.runs[1].end), (2, 5));
    }

    #[test]
    fn selected_values_include_inherited_gaps_and_complete_graphemes() {
        let text = "a\u{301}bc";
        let mut rich = InlineText::default();
        rich.update_range(text, 0..1, |style| style.font_size = Some(15.0))
            .unwrap();
        rich.update_range(text, 2..3, |style| style.font_size = Some(15.0))
            .unwrap();
        let size = |style: &InlineTextStyle| style.font_size.unwrap_or(12.0);
        assert_eq!(selection_value(&rich, text, 0..1, size), None);
        assert_eq!(selection_value(&rich, text, 1..2, size), None);
        assert_eq!(selection_value(&rich, text, 2..3, size), Some(15.0));
        assert_eq!(selection_value(&rich, text, 2..4, size), None);
        assert_eq!(selection_value(&rich, text, 3..4, size), Some(12.0));
        assert_eq!(selection_value(&rich, text, 4..4, size), None);
    }

    #[test]
    fn typography_updates_cover_the_whole_selected_grapheme() {
        let text = "a\u{301}b";
        let mut rich = InlineText::default();
        apply_style(&mut rich, text, 1..2, |style| {
            style.font_size = Some(18.0);
            style.color = Some(thinoda_core::Color::rgb(10, 20, 30));
        })
        .unwrap();
        assert_eq!((rich.runs[0].start, rich.runs[0].end), (0, 2));
        assert_eq!(rich.runs[0].style.font_size, Some(18.0));
        assert_eq!(
            rich.runs[0].style.color,
            Some(thinoda_core::Color::rgb(10, 20, 30))
        );
    }

    #[test]
    fn pending_typing_style_applies_to_insertions_and_joined_graphemes() {
        for (before, after, end) in [("ab", "aXb", 2), ("ab", "a\u{301}b", 2)] {
            let mut rich = InlineText::default();
            rich.update_range(before, 0..1, |style| style.italic = Some(true))
                .unwrap();
            let mut typing = Some(TypingStyle {
                position: 1,
                style: InlineTextStyle {
                    bold: Some(true),
                    ..Default::default()
                },
            });
            let updated = reconcile_typing(&rich, before, after, &mut typing, None, true).unwrap();
            let run = updated
                .runs
                .iter()
                .find(|run| run.start as usize <= 1 && 1 < run.end as usize)
                .unwrap();
            assert_eq!(run.style.bold, Some(true));
            assert_eq!(run.style.italic, None);
            assert_eq!(run.end as usize, end);
            if after.contains('\u{301}') {
                assert_eq!(run.start, 0);
            } else {
                assert_eq!(run.start, 1);
                assert_eq!(updated.runs[0].style.italic, Some(true));
            }
            assert_eq!(typing.unwrap().position, 2);
        }
    }

    #[test]
    fn lists_follow_paragraph_selection_and_do_not_include_the_next_paragraph() {
        let text = "one\ntwo\nthree";
        let mut rich = InlineText::default();
        let base = LabelStyle::default();
        apply(&mut rich, text, 0..8, &base, Format::Bulleted).unwrap();
        assert_eq!(
            rich.paragraphs.iter().map(|p| p.index).collect::<Vec<_>>(),
            vec![0, 1]
        );
        apply(&mut rich, text, 4..4, &base, Format::Numbered).unwrap();
        assert_eq!(rich.paragraphs[0].list, ListKind::Bulleted);
        assert_eq!(rich.paragraphs[1].list, ListKind::Numbered);
        apply(&mut rich, text, 4..4, &base, Format::Numbered).unwrap();
        assert_eq!(rich.paragraphs.len(), 1);
    }
}
