use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::Color;

pub const MAX_INLINE_TEXT_CHARS: usize = 10_000;
pub const MAX_INLINE_TEXT_RUNS: usize = 1_024;
pub const MAX_INLINE_TEXT_PARAGRAPHS: usize = 1_024;
pub const MAX_INLINE_FONT_FAMILY_BYTES: usize = 128;
pub const MIN_INLINE_FONT_SIZE: f32 = 6.0;
pub const MAX_INLINE_FONT_SIZE: f32 = 256.0;
pub const FORMATTED_CARD_REQUIRED: &str =
    "Formatted card text must remain on a floating sticky note or text card.";

/// Baseline override for a formatted text run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InlineBaseline {
    Normal,
    Subscript,
    Superscript,
}

/// Sparse formatting overrides for a contiguous Unicode-scalar range.
/// `None` inherits the whole-text style; `Some(false)` explicitly disables a style.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InlineTextStyle {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underline: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strikethrough: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline: Option<InlineBaseline>,
}

impl InlineTextStyle {
    pub fn is_empty(&self) -> bool {
        self.bold.is_none()
            && self.italic.is_none()
            && self.underline.is_none()
            && self.strikethrough.is_none()
            && self.font_family.is_none()
            && self.font_size.is_none()
            && self.color.is_none()
            && self.baseline.is_none()
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.font_family.as_ref().is_some_and(|family| {
            family.trim().is_empty()
                || family.len() > MAX_INLINE_FONT_FAMILY_BYTES
                || family.chars().any(char::is_control)
        }) {
            return Err("inline font family is invalid".into());
        }
        if self.font_size.is_some_and(|size| {
            !size.is_finite() || !(MIN_INLINE_FONT_SIZE..=MAX_INLINE_FONT_SIZE).contains(&size)
        }) {
            return Err("inline font size is out of range".into());
        }
        Ok(())
    }
}

/// One non-overlapping run of inline style overrides.
///
/// Offsets are Unicode scalar indices, matching egui's text cursor model; they are
/// never UTF-8 byte offsets or browser UTF-16 code-unit offsets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InlineTextRun {
    pub start: u32,
    pub end: u32,
    pub style: InlineTextStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ListKind {
    Bulleted,
    Numbered,
}

/// List formatting for one newline-delimited paragraph, indexed from zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InlineParagraph {
    pub index: u32,
    pub list: ListKind,
}

/// Bounded inline formatting and paragraph semantics for a card title or shape label.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InlineText {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<InlineTextRun>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub paragraphs: Vec<InlineParagraph>,
}

impl InlineText {
    /// Whether any run carries a baseline override.
    pub fn has_baseline_overrides(&self) -> bool {
        self.runs.iter().any(|run| run.style.baseline.is_some())
    }

    pub fn is_empty(&self) -> bool {
        self.runs.is_empty() && self.paragraphs.is_empty()
    }

    pub fn validate(&self, text: &str) -> Result<(), String> {
        if self.runs.len() > MAX_INLINE_TEXT_RUNS {
            return Err(format!(
                "inline text exceeds {MAX_INLINE_TEXT_RUNS} style runs"
            ));
        }
        if self.paragraphs.len() > MAX_INLINE_TEXT_PARAGRAPHS {
            return Err(format!(
                "inline text exceeds {MAX_INLINE_TEXT_PARAGRAPHS} paragraph styles"
            ));
        }
        let char_count = text.chars().count();
        if !self.is_empty() && char_count > MAX_INLINE_TEXT_CHARS {
            return Err(format!(
                "formatted text exceeds {MAX_INLINE_TEXT_CHARS} characters"
            ));
        }

        let mut previous_end = 0;
        for (index, run) in self.runs.iter().enumerate() {
            if run.start >= run.end || run.end as usize > char_count {
                return Err(format!("inline text run {index} has an invalid range"));
            }
            if index > 0 && run.start < previous_end {
                return Err(format!(
                    "inline text run {index} overlaps or is out of order"
                ));
            }
            if run.style.is_empty() {
                return Err(format!("inline text run {index} has no style overrides"));
            }
            run.style.validate()?;
            previous_end = run.end;
        }

        let paragraph_count = text.chars().filter(|ch| *ch == '\n').count() + 1;
        let mut previous_index = None;
        for paragraph in &self.paragraphs {
            if paragraph.index as usize >= paragraph_count {
                return Err("inline paragraph refers to a missing paragraph".into());
            }
            if previous_index.is_some_and(|previous| paragraph.index <= previous) {
                return Err("inline paragraph styles are duplicated or out of order".into());
            }
            previous_index = Some(paragraph.index);
        }
        Ok(())
    }

    /// Applies a sparse style update to a non-empty scalar range.
    ///
    /// Existing properties outside the update are retained, and adjacent identical
    /// runs are coalesced. The caller must expand the range to grapheme boundaries.
    pub fn update_range(
        &mut self,
        text: &str,
        range: Range<usize>,
        mut update: impl FnMut(&mut InlineTextStyle),
    ) -> Result<(), String> {
        self.validate(text)?;
        let char_count = text.chars().count();
        if range.start >= range.end || range.end > char_count {
            return Err("inline formatting requires a valid non-empty text range".into());
        }

        let mut boundaries = vec![range.start, range.end];
        for run in &self.runs {
            boundaries.push(run.start as usize);
            boundaries.push(run.end as usize);
        }
        boundaries.sort_unstable();
        boundaries.dedup();

        let mut runs = Vec::new();
        for pair in boundaries.windows(2) {
            let start = pair[0];
            let end = pair[1];
            if start == end {
                continue;
            }
            let mut style = self
                .runs
                .iter()
                .find(|run| run.start as usize <= start && end <= run.end as usize)
                .map_or_else(InlineTextStyle::default, |run| run.style.clone());
            if range.start <= start && end <= range.end {
                update(&mut style);
            }
            if !style.is_empty() {
                push_coalesced_run(&mut runs, start, end, style)?;
            }
        }
        let candidate = Self {
            runs,
            paragraphs: self.paragraphs.clone(),
        };
        candidate.validate(text)?;
        self.runs = candidate.runs;
        Ok(())
    }

    /// Replaces a Unicode-scalar range and shifts or preserves surrounding styles.
    ///
    /// The replacement style is explicit; callers decide caret-style inheritance
    /// and must expand ranges to grapheme boundaries. Edits that change paragraph
    /// boundaries are rejected while list metadata exists.
    pub fn replace_range(
        &self,
        text: &str,
        range: Range<usize>,
        replacement: &str,
        replacement_style: &InlineTextStyle,
    ) -> Result<(String, Self), String> {
        self.validate(text)?;
        let char_count = text.chars().count();
        if range.start > range.end || range.end > char_count {
            return Err("inline text replacement has an invalid range".into());
        }

        let start_byte = scalar_byte_index(text, range.start);
        let end_byte = scalar_byte_index(text, range.end);
        let replaced = &text[start_byte..end_byte];
        if !self.paragraphs.is_empty() && (replaced.contains('\n') || replacement.contains('\n')) {
            return Err("inline text replacement cannot change listed paragraphs".into());
        }

        let replacement_chars = replacement.chars().count();
        let inserted_end = range
            .start
            .checked_add(replacement_chars)
            .ok_or_else(|| "inline text replacement range is too large".to_owned())?;
        let byte_capacity = start_byte
            .checked_add(replacement.len())
            .and_then(|bytes| bytes.checked_add(text.len() - end_byte))
            .ok_or_else(|| "inline text replacement is too large".to_owned())?;
        let mut updated_text = String::with_capacity(byte_capacity);
        updated_text.push_str(&text[..start_byte]);
        updated_text.push_str(replacement);
        updated_text.push_str(&text[end_byte..]);

        let mut runs = Vec::new();
        for run in &self.runs {
            if (run.start as usize) < range.start {
                push_coalesced_run(
                    &mut runs,
                    run.start as usize,
                    (run.end as usize).min(range.start),
                    run.style.clone(),
                )?;
            }
        }
        if replacement_chars > 0 && !replacement_style.is_empty() {
            push_coalesced_run(
                &mut runs,
                range.start,
                inserted_end,
                replacement_style.clone(),
            )?;
        }
        for run in &self.runs {
            if (run.end as usize) > range.end {
                let start = inserted_end
                    .checked_add((run.start as usize).max(range.end) - range.end)
                    .ok_or_else(|| "inline text replacement range is too large".to_owned())?;
                let end = inserted_end
                    .checked_add(run.end as usize - range.end)
                    .ok_or_else(|| "inline text replacement range is too large".to_owned())?;
                push_coalesced_run(&mut runs, start, end, run.style.clone())?;
            }
        }

        let updated = Self {
            runs,
            paragraphs: self.paragraphs.clone(),
        };
        updated.validate(&updated_text)?;
        Ok((updated_text, updated))
    }

    /// Replaces a label range while carrying list formatting across paragraph edits.
    pub fn replace_label_range(
        &self,
        text: &str,
        range: Range<usize>,
        replacement: &str,
    ) -> Result<(String, Self), String> {
        let style_position = if range.start == range.end && range.start > 0 {
            range.start - 1
        } else {
            range.start
        };
        let replacement_style = self
            .runs
            .iter()
            .find(|run| run.start as usize <= style_position && style_position < run.end as usize)
            .map_or_else(InlineTextStyle::default, |run| run.style.clone());

        self.replace_label_range_with_style(text, range, replacement, &replacement_style)
    }

    /// Replaces a label range with an explicit typing style while preserving lists.
    ///
    /// The caller controls style inheritance and grapheme boundaries.
    pub fn replace_label_range_with_style(
        &self,
        text: &str,
        range: Range<usize>,
        replacement: &str,
        replacement_style: &InlineTextStyle,
    ) -> Result<(String, Self), String> {
        self.validate(text)?;
        let char_count = text.chars().count();
        if range.start > range.end || range.end > char_count {
            return Err("inline text replacement has an invalid range".into());
        }
        replacement_style.validate()?;
        let first = text
            .chars()
            .take(range.start)
            .filter(|ch| *ch == '\n')
            .count();
        let last = text
            .chars()
            .take(range.end)
            .filter(|ch| *ch == '\n')
            .count();
        let added = replacement.chars().filter(|ch| *ch == '\n').count();
        let without_paragraphs = Self {
            runs: self.runs.clone(),
            paragraphs: Vec::new(),
        };
        let (updated_text, mut updated) =
            without_paragraphs.replace_range(text, range, replacement, replacement_style)?;
        let first_list = self
            .paragraphs
            .iter()
            .find(|paragraph| paragraph.index as usize == first)
            .map(|paragraph| paragraph.list);
        for paragraph in &self.paragraphs {
            let index = paragraph.index as usize;
            if index <= first {
                updated.paragraphs.push(*paragraph);
            } else if index > last {
                let shifted = index - (last - first) + added;
                let index =
                    u32::try_from(shifted).map_err(|_| "inline paragraph index is too large")?;
                updated.paragraphs.push(InlineParagraph {
                    index,
                    list: paragraph.list,
                });
            }
        }
        if let Some(list) = first_list {
            for offset in 1..=added {
                let index = u32::try_from(first + offset)
                    .map_err(|_| "inline paragraph index is too large")?;
                updated.paragraphs.push(InlineParagraph { index, list });
            }
        }
        updated.paragraphs.sort_unstable_by_key(|item| item.index);
        updated.validate(&updated_text)?;
        Ok((updated_text, updated))
    }

    /// Preserves formatting outside the changed span of a plain label edit.
    pub fn reconcile_label(&self, before: &str, after: &str) -> Result<Self, String> {
        self.validate(before)?;
        if before == after {
            return Ok(self.clone());
        }
        let before_chars: Vec<_> = before.chars().collect();
        let after_chars: Vec<_> = after.chars().collect();
        let prefix = before_chars
            .iter()
            .zip(&after_chars)
            .take_while(|(left, right)| left == right)
            .count();
        let suffix = before_chars[prefix..]
            .iter()
            .rev()
            .zip(after_chars[prefix..].iter().rev())
            .take_while(|(left, right)| left == right)
            .count();
        let replacement: String = after_chars[prefix..after_chars.len() - suffix]
            .iter()
            .collect();
        let (updated_text, updated) =
            self.replace_label_range(before, prefix..before_chars.len() - suffix, &replacement)?;
        debug_assert_eq!(updated_text, after);
        Ok(updated)
    }

    /// Estimates heap bytes retained with a document-history snapshot.
    pub fn estimated_heap_bytes(&self) -> usize {
        self.runs
            .iter()
            .map(|run| {
                run.style
                    .font_family
                    .as_ref()
                    .map_or(0, |family| family.capacity())
            })
            .sum::<usize>()
            + self.runs.capacity() * std::mem::size_of::<InlineTextRun>()
            + self.paragraphs.capacity() * std::mem::size_of::<InlineParagraph>()
    }
}

fn scalar_byte_index(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map_or(text.len(), |(byte, _)| byte)
}

fn push_coalesced_run(
    runs: &mut Vec<InlineTextRun>,
    start: usize,
    end: usize,
    style: InlineTextStyle,
) -> Result<(), String> {
    let start = u32::try_from(start).map_err(|_| "inline text range is too large")?;
    let end = u32::try_from(end).map_err(|_| "inline text range is too large")?;
    if let Some(previous) = runs.last_mut()
        && previous.end == start
        && previous.style == style
    {
        previous.end = end;
        return Ok(());
    }
    runs.push(InlineTextRun { start, end, style });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_labels_validate_ranges_and_mask_formatting() {
        use crate::board::{CanvasObject, CanvasObjectKind, ShapeKind, validate_object};
        use crate::{Rect, Vec2, Workbook};
        use std::sync::Arc;

        let mut object = CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(40.0, 30.0)),
        );
        let CanvasObjectKind::Shape { label, .. } = &mut object.kind else {
            unreachable!()
        };
        *label = "A🙂B".into();
        let mut rich = InlineText::default();
        rich.update_range(label, 1..2, |style| {
            style.bold = Some(true);
            style.baseline = Some(InlineBaseline::Superscript);
        })
        .unwrap();
        object.label_rich_text = Some(Arc::new(rich.clone()));
        assert!(validate_object(&object).is_ok());

        rich.runs[0].end = 4;
        object.label_rich_text = Some(Arc::new(rich));
        assert!(validate_object(&object).is_err());
        object.label_rich_text = Some(Arc::new(InlineText::default()));
        assert!(validate_object(&object).is_err());

        let mut book = Workbook::default();
        let mut rich = InlineText::default();
        rich.update_range(object.label(), 1..2, |style| style.bold = Some(true))
            .unwrap();
        object.label_rich_text = Some(Arc::new(rich));
        book.active_mut().add_canvas_object(object).unwrap();
        assert!(book.has_rich_labels());
        let masked = book.masked_copy();
        assert!(!masked.has_rich_labels());
        masked.validate().unwrap();
    }

    #[test]
    fn connector_label_formatting_edits_and_undoes_atomically() {
        use crate::board::{CanvasConnector, CanvasObject};
        use crate::{Document, Endpoint, Vec2, Workbook};

        let mut workbook = Workbook::default();
        let connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        let mut object = CanvasObject::connector(connector);
        object.set_label("A🙂B").unwrap();
        let id = workbook.active_mut().add_canvas_object(object).unwrap();
        let mut document = Document::new(workbook);
        let mut rich = InlineText::default();
        rich.update_range("A🙂B", 1..2, |style| style.bold = Some(true))
            .unwrap();
        document
            .set_canvas_object_label_with_rich_text("Format", id, "A🙂B", Some(rich.clone()))
            .unwrap();
        document
            .set_canvas_object_label("Edit", id, "AA🙂B")
            .unwrap();
        let object = document.sheet().canvas.object(id).unwrap();
        assert_eq!(object.label(), "AA🙂B");
        assert_eq!(
            (
                object.label_rich_text.as_ref().unwrap().runs[0].start,
                object.label_rich_text.as_ref().unwrap().runs[0].end
            ),
            (2, 3)
        );
        assert!(document.undo());
        assert_eq!(
            document
                .sheet()
                .canvas
                .object(id)
                .unwrap()
                .label_rich_text
                .as_deref(),
            Some(&rich)
        );
        assert!(document.undo());
        assert!(
            document
                .sheet()
                .canvas
                .object(id)
                .unwrap()
                .label_rich_text
                .is_none()
        );
        assert!(document.redo());
        assert!(document.redo());
        document.sheet().validate().unwrap();
    }

    fn bold() -> InlineTextStyle {
        InlineTextStyle {
            bold: Some(true),
            ..InlineTextStyle::default()
        }
    }

    #[test]
    fn validates_scalar_ranges_and_sparse_style_values() {
        let text = "A🙂e\u{301}";
        let rich = InlineText {
            runs: vec![InlineTextRun {
                start: 1,
                end: 3,
                style: bold(),
            }],
            paragraphs: Vec::new(),
        };
        assert!(rich.validate(text).is_ok());

        for invalid in [
            InlineText {
                runs: vec![InlineTextRun {
                    start: 2,
                    end: 2,
                    style: bold(),
                }],
                paragraphs: Vec::new(),
            },
            InlineText {
                runs: vec![InlineTextRun {
                    start: 0,
                    end: 5,
                    style: bold(),
                }],
                paragraphs: Vec::new(),
            },
            InlineText {
                runs: vec![InlineTextRun {
                    start: 0,
                    end: 1,
                    style: InlineTextStyle {
                        font_size: Some(f32::NAN),
                        ..InlineTextStyle::default()
                    },
                }],
                paragraphs: Vec::new(),
            },
        ] {
            assert!(invalid.validate(text).is_err());
        }
    }

    #[test]
    fn update_range_preserves_other_properties_and_coalesces_runs() {
        let text = "one two three";
        let mut rich = InlineText::default();
        rich.update_range(text, 0..3, |style| style.bold = Some(true))
            .unwrap();
        rich.update_range(text, 0..3, |style| style.italic = Some(true))
            .unwrap();
        rich.update_range(text, 4..7, |style| style.bold = Some(true))
            .unwrap();
        rich.update_range(text, 3..4, |style| style.bold = Some(true))
            .unwrap();

        assert_eq!(
            rich.runs,
            vec![
                InlineTextRun {
                    start: 0,
                    end: 3,
                    style: InlineTextStyle {
                        bold: Some(true),
                        italic: Some(true),
                        ..InlineTextStyle::default()
                    }
                },
                InlineTextRun {
                    start: 3,
                    end: 7,
                    style: bold()
                }
            ]
        );
        rich.validate(text).unwrap();
    }

    #[test]
    fn invalid_style_update_does_not_mutate_existing_runs() {
        let text = "title";
        let mut rich = InlineText::default();
        rich.update_range(text, 0..2, |style| style.bold = Some(true))
            .unwrap();
        let before = rich.clone();

        assert!(
            rich.update_range(text, 0..5, |style| style.font_size = Some(f32::NAN))
                .is_err()
        );
        assert_eq!(rich, before);
    }

    #[test]
    fn replacement_preserves_and_shifts_unicode_style_runs() {
        let text = "a🙂e\u{301}z";
        let rich = InlineText {
            runs: vec![
                InlineTextRun {
                    start: 1,
                    end: 4,
                    style: bold(),
                },
                InlineTextRun {
                    start: 4,
                    end: 5,
                    style: InlineTextStyle {
                        underline: Some(true),
                        ..InlineTextStyle::default()
                    },
                },
            ],
            paragraphs: Vec::new(),
        };
        let inserted_style = InlineTextStyle {
            bold: Some(false),
            italic: Some(true),
            ..InlineTextStyle::default()
        };

        let (updated_text, updated) = rich
            .replace_range(text, 2..4, "漢字", &inserted_style)
            .unwrap();

        assert_eq!(updated_text, "a🙂漢字z");
        assert_eq!(
            updated.runs,
            vec![
                InlineTextRun {
                    start: 1,
                    end: 2,
                    style: bold(),
                },
                InlineTextRun {
                    start: 2,
                    end: 4,
                    style: inserted_style,
                },
                InlineTextRun {
                    start: 4,
                    end: 5,
                    style: InlineTextStyle {
                        underline: Some(true),
                        ..InlineTextStyle::default()
                    },
                },
            ]
        );
        updated.validate(&updated_text).unwrap();
        rich.validate(text).unwrap();
    }

    #[test]
    fn replacement_coalesces_matching_runs_after_deletion() {
        let text = "abcdef";
        let rich = InlineText {
            runs: vec![
                InlineTextRun {
                    start: 0,
                    end: 2,
                    style: bold(),
                },
                InlineTextRun {
                    start: 4,
                    end: 6,
                    style: bold(),
                },
            ],
            paragraphs: Vec::new(),
        };

        let (updated_text, updated) = rich
            .replace_range(text, 2..4, "", &InlineTextStyle::default())
            .unwrap();

        assert_eq!(updated_text, "abef");
        assert_eq!(
            updated.runs,
            vec![InlineTextRun {
                start: 0,
                end: 4,
                style: bold(),
            }]
        );
    }

    #[test]
    fn replacement_rejects_list_boundary_changes_but_allows_in_paragraph_edits() {
        let rich = InlineText {
            runs: Vec::new(),
            paragraphs: vec![InlineParagraph {
                index: 1,
                list: ListKind::Bulleted,
            }],
        };

        assert!(
            rich.replace_range("one\ntwo", 3..4, "", &InlineTextStyle::default())
                .is_err()
        );
        assert!(
            rich.replace_range("one\ntwo", 4..4, "\n", &InlineTextStyle::default())
                .is_err()
        );

        let (updated_text, updated) = rich
            .replace_range("one\ntwo", 4..7, "second", &InlineTextStyle::default())
            .unwrap();
        assert_eq!(updated_text, "one\nsecond");
        assert_eq!(updated.paragraphs, rich.paragraphs);
    }

    #[test]
    fn label_edits_shift_unicode_runs_and_list_paragraphs() {
        let text = "one\n🙂two\nlast";
        let mut rich = InlineText {
            runs: Vec::new(),
            paragraphs: vec![
                InlineParagraph {
                    index: 1,
                    list: ListKind::Bulleted,
                },
                InlineParagraph {
                    index: 2,
                    list: ListKind::Numbered,
                },
            ],
        };
        rich.update_range(text, 4..8, |style| style.bold = Some(true))
            .unwrap();
        let (split, rich) = rich.replace_label_range(text, 5..5, "\nnew").unwrap();
        assert_eq!(split, "one\n🙂\nnewtwo\nlast");
        assert_eq!(
            rich.paragraphs,
            vec![
                InlineParagraph {
                    index: 1,
                    list: ListKind::Bulleted,
                },
                InlineParagraph {
                    index: 2,
                    list: ListKind::Bulleted,
                },
                InlineParagraph {
                    index: 3,
                    list: ListKind::Numbered,
                },
            ]
        );
        assert_eq!(rich.runs[0].start, 4);
        assert_eq!(rich.runs[0].end, 12);
        let (joined, rich) = rich.replace_label_range(&split, 5..6, "").unwrap();
        assert_eq!(joined, "one\n🙂newtwo\nlast");
        assert_eq!(rich.paragraphs[0].index, 1);
        assert_eq!(rich.paragraphs[1].index, 2);
        assert_eq!(rich.paragraphs[1].list, ListKind::Numbered);
        rich.validate(&joined).unwrap();
    }

    #[test]
    fn explicit_label_typing_style_survives_paragraph_replacement() {
        let text = "a🙂\nlast";
        let mut rich = InlineText {
            runs: Vec::new(),
            paragraphs: vec![InlineParagraph {
                index: 0,
                list: ListKind::Bulleted,
            }],
        };
        rich.update_range(text, 0..2, |style| style.bold = Some(true))
            .unwrap();
        let typing = InlineTextStyle {
            italic: Some(true),
            ..Default::default()
        };
        let (updated_text, updated) = rich
            .replace_label_range_with_style(text, 1..2, "X\nY", &typing)
            .unwrap();
        assert_eq!(updated_text, "aX\nY\nlast");
        assert_eq!(
            updated.runs,
            vec![
                InlineTextRun {
                    start: 0,
                    end: 1,
                    style: bold(),
                },
                InlineTextRun {
                    start: 1,
                    end: 4,
                    style: typing,
                },
            ]
        );
        assert_eq!(
            updated.paragraphs,
            vec![
                InlineParagraph {
                    index: 0,
                    list: ListKind::Bulleted,
                },
                InlineParagraph {
                    index: 1,
                    list: ListKind::Bulleted,
                }
            ]
        );
        updated.validate(&updated_text).unwrap();
        assert_eq!(rich.runs[0].end, 2);
    }

    #[test]
    fn explicit_label_typing_style_rejects_invalid_input_without_mutation() {
        let mut rich = InlineText::default();
        rich.update_range("ab", 0..2, |style| style.bold = Some(true))
            .unwrap();
        let before = rich.clone();
        let invalid = InlineTextStyle {
            font_size: Some(f32::NAN),
            ..Default::default()
        };
        assert!(
            rich.replace_label_range_with_style("ab", 1..1, "x", &invalid)
                .is_err()
        );
        assert!(
            rich.replace_label_range_with_style("ab", 3..3, "x", &InlineTextStyle::default())
                .is_err()
        );
        assert!(
            rich.replace_label_range_with_style(
                "ab",
                1..1,
                &"x".repeat(MAX_INLINE_TEXT_CHARS),
                &InlineTextStyle::default()
            )
            .is_err()
        );
        assert_eq!(rich, before);
    }

    #[test]
    fn reconciling_a_label_retains_unedited_styles_and_is_atomic_on_limits() {
        let mut rich = InlineText::default();
        rich.update_range("A🙂B", 2..3, |style| style.italic = Some(true))
            .unwrap();
        let updated = rich.reconcile_label("A🙂B", "A🙂helloB").unwrap();
        assert_eq!(updated.runs[0].start, 7);
        assert_eq!(updated.runs[0].end, 8);
        assert!(
            rich.reconcile_label("A🙂B", &format!("{}B", "x".repeat(MAX_INLINE_TEXT_CHARS)))
                .is_err()
        );
        assert_eq!(rich.runs[0].start, 2);
    }

    #[test]
    fn validates_ordered_paragraph_styles_and_rejects_missing_paragraphs() {
        let rich = InlineText {
            runs: Vec::new(),
            paragraphs: vec![
                InlineParagraph {
                    index: 0,
                    list: ListKind::Bulleted,
                },
                InlineParagraph {
                    index: 2,
                    list: ListKind::Numbered,
                },
            ],
        };
        assert!(rich.validate("first\nsecond\nthird").is_ok());
        assert!(rich.validate("first\nsecond").is_err());
    }

    #[test]
    fn empty_inline_text_is_omitted_and_defaults_from_legacy_data() {
        let serialized = serde_json::to_value(InlineText::default()).unwrap();
        assert_eq!(serialized, serde_json::json!({}));
        assert_eq!(
            serde_json::from_value::<InlineText>(serde_json::json!({})).unwrap(),
            InlineText::default()
        );
    }

    #[test]
    fn sheet_accepts_inline_text_only_on_sticky_and_text_cards() {
        let mut sheet = crate::model::Sheet::new("Board", "Anchor");
        let sticky = sheet
            .add_card(
                "Sticky title",
                crate::Vec2::new(40.0, 40.0),
                crate::Card::new(crate::CardKind::Sticky, crate::Vec2::new(160.0, 100.0)),
            )
            .unwrap();
        sheet
            .topics
            .get_mut(&sticky)
            .unwrap()
            .inline_text
            .update_range("Sticky title", 0..6, |style| style.bold = Some(true))
            .unwrap();
        sheet.validate().unwrap();

        sheet.topics.get_mut(&sticky).unwrap().card = Some(crate::Card::new(
            crate::CardKind::Image,
            crate::Vec2::new(160.0, 100.0),
        ));
        assert!(
            sheet
                .validate()
                .unwrap_err()
                .contains("without a sticky or text card")
        );

        let mut map = crate::model::Sheet::new("Map", "Ordinary topic");
        map.topics
            .get_mut(&map.root)
            .unwrap()
            .inline_text
            .update_range("Ordinary topic", 0..8, |style| style.italic = Some(true))
            .unwrap();
        assert!(
            map.validate()
                .unwrap_err()
                .contains("without a sticky or text card")
        );
    }

    #[test]
    fn card_title_and_rich_text_commit_atomically_and_respect_locks() {
        let mut sheet = crate::Sheet::new("Board", "Root");
        let card = sheet
            .add_card(
                "Old",
                crate::Vec2::ZERO,
                crate::Card::new(crate::CardKind::Sticky, crate::Vec2::new(160.0, 100.0)),
            )
            .unwrap();
        let image = sheet
            .add_card(
                "Image",
                crate::Vec2::new(200.0, 0.0),
                crate::Card::new(crate::CardKind::Image, crate::Vec2::new(160.0, 100.0)),
            )
            .unwrap();
        let text_card = sheet
            .add_card(
                "Text",
                crate::Vec2::new(400.0, 0.0),
                crate::Card::new(crate::CardKind::Text, crate::Vec2::new(160.0, 100.0)),
            )
            .unwrap();
        let root = sheet.root;
        let mut doc = crate::Document::new(crate::Workbook::new(sheet));
        let original = doc.workbook().clone();
        let mut rich = InlineText::default();
        rich.update_range("A🙂\nB", 1..2, |style| {
            style.bold = Some(true);
            style.baseline = Some(InlineBaseline::Superscript);
        })
        .unwrap();
        rich.paragraphs.push(InlineParagraph {
            index: 1,
            list: ListKind::Bulleted,
        });

        let mut candidate = original.active().clone();
        assert!(
            candidate
                .set_card_title_with_rich_text(card, "A", rich.clone())
                .is_err()
        );
        assert_eq!(&candidate, original.active());
        assert!(
            doc.set_card_title_with_rich_text(card, "A", rich.clone())
                .is_err()
        );
        assert!(
            doc.set_card_title_with_rich_text(image, "A🙂\nB", rich.clone())
                .is_err()
        );
        assert!(
            doc.set_card_title_with_rich_text(root, "A🙂\nB", rich.clone())
                .is_err()
        );
        assert_eq!(doc.workbook(), &original);
        assert!(!doc.undo());
        doc.set_card_title_with_rich_text(card, "A🙂\nB", rich.clone())
            .unwrap();
        assert_eq!(doc.sheet().topic(card).unwrap().title, "A🙂\nB");
        assert_eq!(doc.sheet().topic(card).unwrap().inline_text, rich);
        doc.workbook().validate().unwrap();
        assert!(doc.undo());
        assert_eq!(doc.workbook(), &original);
        assert!(doc.redo());
        assert_eq!(doc.sheet().topic(card).unwrap().inline_text, rich);
        doc.set_card_title_with_rich_text(card, "A🙂\nB", rich.clone())
            .unwrap();
        assert!(doc.undo());
        assert_eq!(doc.workbook(), &original);
        assert!(doc.redo());

        doc.set_board_members_locked(&[crate::board::BoardMember::Card(card)], true)
            .unwrap();
        let locked = doc.workbook().clone();
        assert!(
            doc.set_card_title_with_rich_text(card, "Changed", InlineText::default())
                .is_err()
        );
        assert_eq!(doc.workbook(), &locked);
        doc.set_card_title_with_rich_text(text_card, "A🙂\nB", rich.clone())
            .unwrap();
        assert_eq!(doc.sheet().topic(text_card).unwrap().inline_text, rich);
    }

    #[test]
    fn formatted_card_reparenting_conversion_and_mixed_paste_are_atomic() {
        let mut sheet = crate::Sheet::new("Board", "Root");
        let root = sheet.root;
        let plain = sheet.add_child(root, None, "Plain");
        let target = sheet.add_child(root, None, "Target");
        let card = sheet
            .add_card(
                "A🦀\nList",
                crate::Vec2::new(120.0, 100.0),
                crate::Card::new(crate::CardKind::Sticky, crate::Vec2::new(160.0, 100.0)),
            )
            .unwrap();
        let topic = sheet.topic_mut(card).unwrap();
        topic
            .inline_text
            .update_range("A🦀\nList", 1..2, |style| style.bold = Some(true))
            .unwrap();
        topic.inline_text.paragraphs.push(InlineParagraph {
            index: 1,
            list: ListKind::Numbered,
        });
        sheet.add_child(card, None, "Card child");
        sheet.add_relationship(card, plain).unwrap();
        sheet
            .create_zone(
                "Zone",
                vec![card],
                crate::Rect::from_min_size(crate::Vec2::ZERO, crate::Vec2::new(300.0, 240.0)),
            )
            .unwrap();
        sheet.validate().unwrap();
        let before = sheet.clone();
        let expected = FORMATTED_CARD_REQUIRED;

        assert_eq!(
            sheet.move_topic(card, target, 0).unwrap_err().to_string(),
            expected
        );
        assert_eq!(sheet, before);
        let copied = sheet.copy_branch_as_sheet(card).unwrap();
        assert_eq!(copied.root_mode, crate::SheetRootMode::Hidden);
        assert_eq!(copied.floating, vec![card]);
        assert_eq!(copied.topic(card), sheet.topic(card));
        copied.validate().unwrap();
        assert_eq!(sheet, before);

        let subtree = sheet.copy_subtree(card).unwrap();
        let rejected = sheet.attach_child(target, 0, subtree.clone()).unwrap_err();
        assert_eq!(rejected.to_string(), expected);
        assert_eq!(*rejected.subtree, subtree);
        assert_eq!(sheet, before);

        let plain_subtree = sheet.copy_subtree(plain).unwrap();
        assert_eq!(
            sheet
                .paste(target, &[plain_subtree, subtree])
                .unwrap_err()
                .to_string(),
            expected
        );
        assert_eq!(sheet, before);

        let mut malformed = sheet.copy_subtree(target).unwrap();
        let mut descendant = crate::Topic::new(crate::TopicId(999), "Malformed");
        descendant
            .inline_text
            .update_range("Malformed", 0..3, |style| style.italic = Some(true))
            .unwrap();
        malformed.topics[0].children.push(descendant.id);
        descendant.parent = Some(malformed.root);
        malformed.topics.push(descendant);
        let rejected = sheet
            .attach_child(target, 0, malformed.clone())
            .unwrap_err();
        assert_eq!(rejected.to_string(), expected);
        assert_eq!(*rejected.subtree, malformed);
        assert_eq!(sheet, before);
        assert_eq!(
            sheet
                .paste(target, &[sheet.copy_subtree(plain).unwrap(), malformed])
                .unwrap_err()
                .to_string(),
            expected
        );
        assert_eq!(sheet, before);

        let mut invalid_sheet = before.clone();
        let child = invalid_sheet.add_child(plain, None, "Child");
        let grandchild = invalid_sheet.add_child(child, None, "Invalid");
        invalid_sheet
            .topic_mut(grandchild)
            .unwrap()
            .inline_text
            .update_range("Invalid", 0..3, |style| style.bold = Some(true))
            .unwrap();
        let invalid_before = invalid_sheet.clone();
        assert_eq!(
            invalid_sheet
                .move_topic(plain, target, 0)
                .unwrap_err()
                .to_string(),
            expected
        );
        assert_eq!(invalid_sheet, invalid_before);
        assert_eq!(
            invalid_sheet
                .insert_parent(plain, "Parent")
                .unwrap_err()
                .to_string(),
            expected
        );
        assert_eq!(invalid_sheet, invalid_before);
        assert_eq!(
            invalid_sheet
                .remove_topic_keep_children(plain)
                .unwrap_err()
                .to_string(),
            expected
        );
        assert_eq!(invalid_sheet, invalid_before);

        let mut missing = before.clone();
        missing
            .topic_mut(plain)
            .unwrap()
            .children
            .push(crate::TopicId(999));
        let missing_before = missing.clone();
        assert_eq!(
            missing.move_topic(plain, target, 0).unwrap_err(),
            crate::EditError::NoSuchTopic(crate::TopicId(999))
        );
        assert_eq!(missing, missing_before);

        for replacement in [
            None,
            Some(crate::Card::new(
                crate::CardKind::Image,
                crate::Vec2::new(160.0, 100.0),
            )),
        ] {
            assert_eq!(
                sheet.set_topic_card(card, replacement).unwrap_err(),
                expected
            );
            assert_eq!(sheet, before);
        }
        let mut doc = crate::Document::new(crate::Workbook::new(sheet));
        let before = doc.workbook().clone();
        let state = doc.state_id();
        assert!(
            doc.edit_sheet("Move", |sheet| {
                sheet.move_topic(plain, target, 0)?;
                sheet.move_topic(card, target, 1)
            })
            .is_err()
        );
        assert_eq!(doc.workbook(), &before);
        assert_eq!(doc.state_id(), state);
        assert!(!doc.can_undo());
        doc.set_topic_card(
            card,
            Some(crate::Card::new(
                crate::CardKind::Text,
                crate::Vec2::new(160.0, 100.0),
            )),
        )
        .unwrap();
        let after = doc.workbook().clone();
        assert_eq!(
            doc.sheet().topic(card).unwrap().inline_text,
            before.active().topic(card).unwrap().inline_text
        );
        doc.workbook().validate().unwrap();
        assert!(doc.undo());
        assert_eq!(doc.workbook(), &before);
        assert!(doc.redo());
        assert_eq!(doc.workbook(), &after);
    }
}
