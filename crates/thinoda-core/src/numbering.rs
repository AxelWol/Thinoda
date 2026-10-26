//! Structural numbering derived from topic order; never stored in titles.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{EditError, EditResult, Sheet, TopicId};

/// Largest accepted first number for a sibling group.
pub const MAX_START: u32 = 9_999;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NumberFormat {
    #[default]
    Decimal,
    UpperAlpha,
    LowerAlpha,
    UpperRoman,
    LowerRoman,
}

impl NumberFormat {
    pub const ALL: [NumberFormat; 5] = [
        NumberFormat::Decimal,
        NumberFormat::UpperAlpha,
        NumberFormat::LowerAlpha,
        NumberFormat::UpperRoman,
        NumberFormat::LowerRoman,
    ];

    /// Language-neutral sample shown in format pickers.
    pub fn sample(self) -> &'static str {
        match self {
            NumberFormat::Decimal => "1, 2, 3",
            NumberFormat::UpperAlpha => "A, B, C",
            NumberFormat::LowerAlpha => "a, b, c",
            NumberFormat::UpperRoman => "I, II, III",
            NumberFormat::LowerRoman => "i, ii, iii",
        }
    }

    /// Formats a positive ordinal. Letters continue as Z, AA, AB; Roman numerals
    /// above 3999 fall back to decimal because standard Roman forms end there.
    pub fn format(self, n: u32) -> String {
        match self {
            NumberFormat::Decimal => n.to_string(),
            NumberFormat::UpperAlpha => alpha(n),
            NumberFormat::LowerAlpha => alpha(n).to_ascii_lowercase(),
            NumberFormat::UpperRoman => roman(n),
            NumberFormat::LowerRoman => roman(n).to_ascii_lowercase(),
        }
    }
}

fn alpha(mut n: u32) -> String {
    if n == 0 {
        return "0".into();
    }
    let mut out = Vec::new();
    while n > 0 {
        n -= 1;
        out.push(b'A' + (n % 26) as u8);
        n /= 26;
    }
    out.reverse();
    String::from_utf8(out).expect("ASCII letters")
}

fn roman(n: u32) -> String {
    if !(1..=3999).contains(&n) {
        return n.to_string();
    }
    const TABLE: [(u32, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut n = n;
    let mut out = String::new();
    for (value, glyph) in TABLE {
        while n >= value {
            out.push_str(glyph);
            n -= value;
        }
    }
    out
}

fn default_start() -> u32 {
    1
}

fn is_default_start(start: &u32) -> bool {
    *start == 1
}

/// Numbering of one sibling group, stored on the parent topic so every sibling
/// shares one style and position changes renumber deterministically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Numbering {
    pub format: NumberFormat,
    /// Prefix each number with the parent's number, e.g. `2.1`.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub tiered: bool,
    /// First number of the group; restarts numbering independently of cousins.
    #[serde(skip_serializing_if = "is_default_start")]
    pub start: u32,
}

impl Default for Numbering {
    fn default() -> Self {
        Self {
            format: NumberFormat::Decimal,
            tiered: false,
            start: default_start(),
        }
    }
}

impl Numbering {
    pub fn valid(&self) -> bool {
        (1..=MAX_START).contains(&self.start)
    }
}

impl Sheet {
    /// Display labels for every numbered topic. Labels are derived from the
    /// current child order, so insert, reorder and reparent need no rewrite.
    pub fn numbering_labels(&self) -> HashMap<TopicId, String> {
        let mut labels = HashMap::new();
        let mut stack = self.top_level_ids();
        while let Some(id) = stack.pop() {
            let Some(topic) = self.topic(id) else {
                continue;
            };
            if let Some(numbering) = topic.child_numbering {
                let start = numbering.start.clamp(1, MAX_START);
                let prefix = numbering.tiered.then(|| labels.get(&id).cloned()).flatten();
                for (index, child) in topic.children.iter().enumerate() {
                    let n = start.saturating_add(index as u32);
                    let own = numbering.format.format(n);
                    labels.insert(
                        *child,
                        match &prefix {
                            Some(prefix) => format!("{prefix}.{own}"),
                            None => own,
                        },
                    );
                }
            }
            stack.extend(topic.children.iter().rev().copied());
        }
        labels
    }

    /// Sets the numbering shared by `id` and its siblings.
    pub fn set_sibling_numbering(
        &mut self,
        id: TopicId,
        numbering: Option<Numbering>,
    ) -> EditResult<()> {
        if !self.contains(id) {
            return Err(EditError::NoSuchTopic(id));
        }
        let parent = self
            .parent(id)
            .ok_or_else(|| EditError::Invalid("numbering needs a parent topic".into()))?;
        if numbering.is_some_and(|n| !n.valid()) {
            return Err(EditError::Invalid(format!(
                "numbering must start between 1 and {MAX_START}"
            )));
        }
        self.topic_mut(parent)
            .expect("parent exists")
            .child_numbering = numbering;
        Ok(())
    }

    /// Numbering of the group containing `id`, if any.
    pub fn sibling_numbering(&self, id: TopicId) -> Option<Numbering> {
        self.parent(id)
            .and_then(|parent| self.topic(parent))
            .and_then(|parent| parent.child_numbering)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Document, Workbook};

    fn fixture() -> (Sheet, [TopicId; 4]) {
        let mut sheet = Sheet::new("Agenda", "Meeting");
        let a = sheet.add_child(sheet.root, None, "Welcome");
        let b = sheet.add_child(sheet.root, None, "Review");
        let b1 = sheet.add_child(b, None, "Budget");
        let b2 = sheet.add_child(b, None, "Risks");
        (sheet, [a, b, b1, b2])
    }

    #[test]
    fn formats_cover_long_sequences() {
        assert_eq!(NumberFormat::Decimal.format(12), "12");
        assert_eq!(NumberFormat::UpperAlpha.format(1), "A");
        assert_eq!(NumberFormat::UpperAlpha.format(26), "Z");
        assert_eq!(NumberFormat::UpperAlpha.format(27), "AA");
        assert_eq!(NumberFormat::LowerAlpha.format(703), "aaa");
        assert_eq!(NumberFormat::UpperRoman.format(1994), "MCMXCIV");
        assert_eq!(NumberFormat::LowerRoman.format(4), "iv");
        assert_eq!(NumberFormat::UpperRoman.format(4000), "4000");
    }

    #[test]
    fn labels_follow_order_tiers_and_restarts_without_touching_titles() {
        let (mut sheet, [a, b, b1, b2]) = fixture();
        sheet
            .set_sibling_numbering(a, Some(Numbering::default()))
            .unwrap();
        sheet
            .set_sibling_numbering(
                b1,
                Some(Numbering {
                    format: NumberFormat::LowerAlpha,
                    tiered: true,
                    start: 3,
                }),
            )
            .unwrap();
        let labels = sheet.numbering_labels();
        assert_eq!(labels[&a], "1");
        assert_eq!(labels[&b], "2");
        assert_eq!(labels[&b1], "2.c");
        assert_eq!(labels[&b2], "2.d");
        assert!(!labels.contains_key(&sheet.root));
        assert_eq!(sheet.topics[&b1].title, "Budget");

        sheet.move_topic(b, sheet.root, 0).unwrap();
        let labels = sheet.numbering_labels();
        assert_eq!(labels[&b], "1");
        assert_eq!(labels[&a], "2");
        assert_eq!(labels[&b1], "1.c");

        sheet.move_topic(b2, a, 0).unwrap();
        let labels = sheet.numbering_labels();
        assert!(!labels.contains_key(&b2), "the new group is not numbered");
        assert_eq!(labels[&b1], "1.c");
    }

    #[test]
    fn untiered_children_ignore_unnumbered_parents_and_invalid_input_is_rejected() {
        let (mut sheet, [a, b, b1, _]) = fixture();
        sheet
            .set_sibling_numbering(
                b1,
                Some(Numbering {
                    tiered: true,
                    ..Default::default()
                }),
            )
            .unwrap();
        assert_eq!(sheet.numbering_labels()[&b1], "1");
        assert!(sheet.set_sibling_numbering(sheet.root, None).is_err());
        assert!(
            sheet
                .set_sibling_numbering(
                    a,
                    Some(Numbering {
                        start: 0,
                        ..Default::default()
                    })
                )
                .is_err()
        );
        assert!(sheet.sibling_numbering(b).is_none());
        let mut corrupt = sheet.clone();
        corrupt.topic_mut(corrupt.root).unwrap().child_numbering = Some(Numbering {
            start: 0,
            ..Default::default()
        });
        assert!(corrupt.validate().is_err());
    }

    #[test]
    fn layout_places_number_before_checkbox_and_markers() {
        let (mut sheet, [a, ..]) = fixture();
        sheet
            .set_sibling_numbering(a, Some(Numbering::default()))
            .unwrap();
        sheet.set_todo(a, Some(false)).unwrap();
        sheet
            .topic_mut(a)
            .unwrap()
            .markers
            .push("priority-1".into());
        let plain = crate::layout_sheet(&sheet, &crate::ApproxMeasure, false);
        let topic = &plain.topics[&a];
        let (label, number) = topic.number.clone().unwrap();
        assert_eq!(label, "1");
        assert!(number.max().x <= topic.todo_rect.unwrap().min.x);
        assert!(topic.todo_rect.unwrap().max().x <= topic.markers[0].1.min.x);
        assert!(topic.markers[0].1.max().x <= topic.title_rect.min.x);
        assert!(topic.rect.contains(number.center()));
        assert!(plain.topics[&sheet.root].number.is_none());
    }

    #[test]
    fn numbering_persists_and_undoes_as_one_edit() {
        let (sheet, [a, ..]) = fixture();
        let mut doc = Document::new(Workbook {
            sheets: vec![sheet],
            ..Default::default()
        });
        doc.edit_sheet("Numbering", |s| {
            s.set_sibling_numbering(a, Some(Numbering::default()))
        })
        .unwrap();
        let json = serde_json::to_string(doc.sheet()).unwrap();
        assert!(json.contains("\"child_numbering\":{\"format\":\"decimal\"}"));
        let restored: Sheet = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.numbering_labels()[&a], "1");
        assert!(doc.undo());
        assert!(doc.sheet().numbering_labels().is_empty());
        assert!(!doc.undo(), "setting numbering is one history entry");
        assert!(doc.redo());
        assert_eq!(doc.sheet().numbering_labels()[&a], "1");
    }
}
