use std::ops::Range;

use thinoda_core::{
    label_style::LabelStyle,
    rich_text::{InlineBaseline, InlineParagraph, InlineText, InlineTextStyle, ListKind},
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
    fn list(self) -> Option<ListKind> {
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
