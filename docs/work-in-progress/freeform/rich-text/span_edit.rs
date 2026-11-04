//! Isolated span-editing experiment. This is not part of the application.

use std::ops::Range;

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
}
