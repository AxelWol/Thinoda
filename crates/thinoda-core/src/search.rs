//! Find & replace and filtering across topics and freeform objects.

use crate::board::{CanvasObjectId, CanvasObjectKind};
use crate::model::{Sheet, TopicId, Workbook};
use std::borrow::Cow;
use std::collections::HashSet;
use std::ops::Range;

fn searchable_note(note: &str) -> Cow<'_, str> {
    crate::notes::plain_text_projection(note)
        .map(Cow::Owned)
        .unwrap_or(Cow::Borrowed(note))
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SearchOptions {
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub include_notes: bool,
    pub include_labels: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchField {
    Title,
    Notes,
    Label,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub sheet: usize,
    pub topic: TopicId,
    pub field: MatchField,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasMatchField {
    Label,
    AltText,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanvasSearchHit {
    pub sheet: usize,
    pub object: CanvasObjectId,
    pub field: CanvasMatchField,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Completion {
    Incomplete,
    Complete,
}

/// Values within a facet are ORed; different facets are ANDed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Facets {
    pub markers: Vec<String>,
    pub labels: Vec<String>,
    pub priorities: Vec<u8>,
    pub todos: Vec<Completion>,
    pub tasks: Vec<Completion>,
    pub notes_only: bool,
}

impl Facets {
    pub fn active(&self) -> bool {
        self.notes_only
            || !self.markers.is_empty()
            || !self.labels.is_empty()
            || !self.priorities.is_empty()
            || !self.todos.is_empty()
            || !self.tasks.is_empty()
    }

    pub fn matches(&self, sheet: &Sheet, id: TopicId) -> bool {
        self.matches_with_progress(sheet, id, &sheet.todo_progress())
    }

    fn matches_with_progress(
        &self,
        sheet: &Sheet,
        id: TopicId,
        progress: &std::collections::HashMap<TopicId, crate::todo::TodoProgress>,
    ) -> bool {
        if sheet.is_anchor(id) {
            return false;
        }
        let Some(topic) = sheet.topic(id) else {
            return false;
        };
        let completion = |done| {
            if done {
                Completion::Complete
            } else {
                Completion::Incomplete
            }
        };
        let task = sheet.scheduling.tasks.get(&id);
        (!self.notes_only || topic.has_notes())
            && (self.markers.is_empty() || topic.markers.iter().any(|m| self.markers.contains(m)))
            && (self.labels.is_empty() || topic.labels.iter().any(|l| self.labels.contains(l)))
            && (self.priorities.is_empty()
                || task
                    .and_then(|t| t.priority)
                    .is_some_and(|p| self.priorities.contains(&p))
                || topic.markers.iter().any(|m| {
                    m.strip_prefix("priority-")
                        .and_then(|p| p.parse::<u8>().ok())
                        .is_some_and(|p| self.priorities.contains(&p))
                }))
            && (self.todos.is_empty()
                || topic.todo.is_some_and(|done| {
                    let done = progress.get(&id).map_or(done, |p| p.complete());
                    self.todos.contains(&completion(done))
                }))
            && (self.tasks.is_empty()
                || task
                    .is_some_and(|t| self.tasks.contains(&completion(t.progress_percent == 100))))
    }
}

/// Search and facets share topic identities and stable document order.
pub fn search_faceted(
    sheet: &Sheet,
    sheet_index: usize,
    needle: &str,
    opts: &SearchOptions,
    facets: &Facets,
) -> Vec<SearchHit> {
    let progress = if facets.todos.is_empty() {
        Default::default()
    } else {
        sheet.todo_progress()
    };
    let matches_facets = |id| facets.matches_with_progress(sheet, id, &progress);
    if needle.is_empty() {
        if !facets.active() {
            return Vec::new();
        }
        return sheet
            .content_ids_ordered()
            .into_iter()
            .filter(|id| matches_facets(*id))
            .map(|topic| SearchHit {
                sheet: sheet_index,
                topic,
                field: if facets.notes_only {
                    MatchField::Notes
                } else {
                    MatchField::Title
                },
            })
            .collect();
    }
    if facets.notes_only {
        return sheet
            .content_ids_ordered()
            .into_iter()
            .filter(|id| {
                matches_facets(*id)
                    && sheet.topics[id]
                        .notes
                        .as_deref()
                        .is_some_and(|note| matches(&searchable_note(note), needle, opts))
            })
            .map(|topic| SearchHit {
                sheet: sheet_index,
                topic,
                field: MatchField::Notes,
            })
            .collect();
    }
    search_sheet(sheet, sheet_index, needle, opts)
        .into_iter()
        .filter(|hit| matches_facets(hit.topic))
        .collect()
}

struct CaseLengthChange {
    lowered: Range<usize>,
    original: Range<usize>,
}

enum CaseBoundaries {
    Identity,
    Changes(Vec<CaseLengthChange>),
    Dense(Vec<Option<usize>>),
}

impl CaseBoundaries {
    fn original(&self, hay: &str, offset: usize) -> Option<usize> {
        let original = match self {
            Self::Identity => offset,
            Self::Dense(offsets) => offsets[offset]?,
            Self::Changes(changes) => {
                let preceding = changes.partition_point(|change| change.lowered.end <= offset);
                if changes
                    .get(preceding)
                    .is_some_and(|change| offset > change.lowered.start)
                {
                    return None;
                }
                if preceding == 0 {
                    offset
                } else {
                    let previous = &changes[preceding - 1];
                    previous.original.end + (offset - previous.lowered.end)
                }
            }
        };
        hay.is_char_boundary(original).then_some(original)
    }
}

fn find_in(hay: &str, needle: &str, opts: &SearchOptions) -> Vec<Range<usize>> {
    if needle.is_empty() {
        return vec![];
    }
    let (h, n): (Cow<'_, str>, Cow<'_, str>) = if opts.case_sensitive {
        (Cow::Borrowed(hay), Cow::Borrowed(needle))
    } else {
        (
            Cow::Owned(hay.to_lowercase()),
            Cow::Owned(needle.to_lowercase()),
        )
    };
    let boundaries = if opts.case_sensitive || hay.is_ascii() {
        CaseBoundaries::Identity
    } else {
        let mut changes = Vec::new();
        let mut lower = 0;
        for (original, ch) in hay.char_indices() {
            let lower_len = ch.to_lowercase().map(char::len_utf8).sum::<usize>();
            if lower_len != ch.len_utf8() {
                changes.push(CaseLengthChange {
                    lowered: lower..lower + lower_len,
                    original: original..original + ch.len_utf8(),
                });
            }
            lower += lower_len;
        }
        if lower != h.len() {
            let mut offsets = vec![None; h.len() + 1];
            for (original, _) in hay.char_indices() {
                let lower = hay[..original].to_lowercase().len();
                if lower <= h.len() {
                    offsets[lower] = Some(original);
                }
            }
            offsets[h.len()] = Some(hay.len());
            CaseBoundaries::Dense(offsets)
        } else {
            CaseBoundaries::Changes(changes)
        }
    };
    let mut out = Vec::new();
    let mut start = 0;
    while let Some(pos) = h[start..].find(n.as_ref()) {
        let i = start + pos;
        let end = i + n.len();
        let original = boundaries
            .original(hay, i)
            .zip(boundaries.original(hay, end))
            .map(|(start, end)| start..end);
        if let Some(range) = original {
            let ok = !opts.whole_word
                || (hay[..range.start]
                    .chars()
                    .next_back()
                    .is_none_or(|c| !c.is_alphanumeric())
                    && hay[range.end..]
                        .chars()
                        .next()
                        .is_none_or(|c| !c.is_alphanumeric()));
            if ok {
                out.push(range);
            }
            start = end;
        } else {
            start = i + h[i..].chars().next().unwrap().len_utf8();
        }
    }
    out
}

pub fn matches(text: &str, needle: &str, opts: &SearchOptions) -> bool {
    !find_in(text, needle, opts).is_empty()
}

/// Searches one sheet in document order.
pub fn search_sheet(
    sheet: &Sheet,
    sheet_index: usize,
    needle: &str,
    opts: &SearchOptions,
) -> Vec<SearchHit> {
    let mut hits = Vec::new();
    for id in sheet.content_ids_ordered() {
        let t = &sheet.topics[&id];
        if matches(&t.title, needle, opts) {
            hits.push(SearchHit {
                sheet: sheet_index,
                topic: id,
                field: MatchField::Title,
            });
        } else if opts.include_notes
            && t.notes
                .as_deref()
                .is_some_and(|note| matches(&searchable_note(note), needle, opts))
        {
            hits.push(SearchHit {
                sheet: sheet_index,
                topic: id,
                field: MatchField::Notes,
            });
        } else if opts.include_labels && t.labels.iter().any(|l| matches(l, needle, opts)) {
            hits.push(SearchHit {
                sheet: sheet_index,
                topic: id,
                field: MatchField::Label,
            });
        }
    }
    hits
}

pub fn search_workbook(wb: &Workbook, needle: &str, opts: &SearchOptions) -> Vec<SearchHit> {
    wb.sheets
        .iter()
        .enumerate()
        .flat_map(|(i, s)| search_sheet(s, i, needle, opts))
        .collect()
}

/// Searches canvas labels and accessibility descriptions in object order.
pub fn search_canvas(
    sheet: &Sheet,
    sheet_index: usize,
    needle: &str,
    opts: &SearchOptions,
) -> Vec<CanvasSearchHit> {
    sheet
        .canvas
        .objects
        .iter()
        .filter_map(|object| {
            let field = if matches(object.label(), needle, opts) {
                CanvasMatchField::Label
            } else if matches(&object.alt_text, needle, opts) {
                CanvasMatchField::AltText
            } else {
                return None;
            };
            Some(CanvasSearchHit {
                sheet: sheet_index,
                object: object.id,
                field,
            })
        })
        .collect()
}

fn replace_text(text: &mut String, needle: &str, replacement: &str, opts: &SearchOptions) -> usize {
    let ranges = find_in(text, needle, opts);
    if ranges.is_empty() {
        return 0;
    }
    let mut out = String::new();
    let mut last = 0;
    for range in &ranges {
        out.push_str(&text[last..range.start]);
        out.push_str(replacement);
        last = range.end;
    }
    out.push_str(&text[last..]);
    *text = out;
    ranges.len()
}

/// Replaces all occurrences in titles of `ids`, rejecting locked cards atomically.
pub fn replace_in_titles(
    sheet: &mut Sheet,
    ids: &[TopicId],
    needle: &str,
    replacement: &str,
    opts: &SearchOptions,
) -> Result<usize, String> {
    let mut count = 0;
    let mut updates = Vec::new();
    for id in ids {
        if sheet.is_anchor(*id) {
            continue;
        }
        let Some(topic) = sheet.topic(*id) else {
            continue;
        };
        let mut title = topic.title.clone();
        let matches = replace_text(&mut title, needle, replacement, opts);
        if matches > 0 {
            if !topic.inline_text.is_empty() {
                return Err(
                    "Formatted card titles cannot be changed until rich-text editing is supported"
                        .into(),
                );
            }
            if let Some(member) = sheet.board_member_for_topic(*id) {
                sheet.require_unlocked_board_selection(&[member])?;
            }
            count += matches;
            updates.push((*id, title));
        }
    }
    for (id, title) in updates {
        sheet
            .set_title(id, &title)
            .map_err(|error| error.to_string())?;
    }
    Ok(count)
}

/// Atomically replaces matching canvas labels and descriptions. Locked objects and
/// invalid replacement results return an error without changing the sheet.
pub fn replace_in_canvas(
    sheet: &mut Sheet,
    ids: &[CanvasObjectId],
    needle: &str,
    replacement: &str,
    opts: &SearchOptions,
) -> Result<usize, String> {
    let mut candidate = sheet.clone();
    let wanted: HashSet<_> = ids.iter().copied().collect();
    if !wanted.is_empty() {
        let members: Vec<_> = wanted
            .iter()
            .copied()
            .map(crate::board::BoardMember::Object)
            .collect();
        candidate.require_unlocked_board_selection(&members)?;
    }
    let mut found = HashSet::new();
    let mut count = 0;
    for object in &mut candidate.canvas.objects {
        if !wanted.contains(&object.id) {
            continue;
        }
        found.insert(object.id);
        count += replace_text(&mut object.alt_text, needle, replacement, opts);
        let label = match &mut object.kind {
            CanvasObjectKind::Shape { label, .. } => Some(label),
            CanvasObjectKind::Connector(connector) => Some(&mut connector.label),
            CanvasObjectKind::Ink(_) => None,
        };
        if let Some(label) = label {
            count += replace_text(label, needle, replacement, opts);
        }
    }
    if let Some(id) = wanted.difference(&found).next() {
        return Err(format!("canvas object {id} not found"));
    }
    candidate.validate()?;
    sheet.canvas = candidate.canvas;
    Ok(count)
}

/// Filter: topics having any of the given markers or labels (and their ancestors so the
/// tree stays connected). Used by the "Filter" feature.
pub fn filter_topics(sheet: &Sheet, markers: &[String], labels: &[String]) -> Vec<TopicId> {
    let mut keep = Vec::new();
    for id in sheet.content_ids_ordered() {
        let t = &sheet.topics[&id];
        let hit = t.markers.iter().any(|m| markers.contains(m))
            || t.labels.iter().any(|l| labels.contains(l));
        if hit {
            for a in std::iter::once(id).chain(sheet.ancestors(id)) {
                if !keep.contains(&a) {
                    keep.push(a);
                }
            }
        }
    }
    keep
}

/// Topic statistics shown in the status bar ("Topics: n" / word count).
pub fn word_count(sheet: &Sheet) -> usize {
    sheet
        .content_topics()
        .map(|t| t.title.split_whitespace().count())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s() -> Sheet {
        let mut s = Sheet::new("S", "Project Plan");
        let a = s.add_child(s.root, None, "Plan budget");
        s.topic_mut(a).unwrap().notes = Some("needs approval".into());
        s.topic_mut(a).unwrap().labels = vec!["finance".into()];
        s.topic_mut(a).unwrap().markers = vec!["priority-1".into()];
        s.add_child(s.root, None, "Planet");
        s
    }

    #[test]
    fn search_options() {
        let s = s();
        let o = SearchOptions::default();
        assert_eq!(search_sheet(&s, 0, "plan", &o).len(), 3);
        let ww = SearchOptions {
            whole_word: true,
            ..Default::default()
        };
        assert_eq!(search_sheet(&s, 0, "plan", &ww).len(), 2);
        let cs = SearchOptions {
            case_sensitive: true,
            ..Default::default()
        };
        assert_eq!(search_sheet(&s, 0, "Plan", &cs).len(), 3);
        assert_eq!(search_sheet(&s, 0, "planet", &cs).len(), 0);
        let notes = SearchOptions {
            include_notes: true,
            include_labels: true,
            ..Default::default()
        };
        let h = search_sheet(&s, 0, "approval", &notes);
        assert_eq!(h[0].field, MatchField::Notes);
        assert_eq!(
            search_sheet(&s, 0, "finance", &notes)[0].field,
            MatchField::Label
        );
        assert!(search_sheet(&s, 0, "", &o).is_empty());
    }

    #[test]
    fn formatted_notes_search_visible_markdown_text_without_link_destinations() {
        let mut sheet = s();
        let topic = sheet.children(sheet.root)[0];
        let source =
            "**Café** *research* with `code` and [visible label](https://example.invalid/secret)";
        sheet.topic_mut(topic).unwrap().notes = Some(source.into());
        let before = sheet.clone();
        let include_notes = SearchOptions {
            include_notes: true,
            ..Default::default()
        };
        let hits = search_sheet(&sheet, 0, "research", &include_notes);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].topic, topic);
        assert_eq!(hits[0].field, MatchField::Notes);
        assert_eq!(
            search_sheet(&sheet, 0, "visible label", &include_notes).len(),
            1
        );
        assert!(
            search_sheet(&sheet, 0, "example.invalid/secret", &include_notes).is_empty(),
            "link destinations are not note text"
        );
        assert!(search_sheet(&sheet, 0, "**Café**", &include_notes).is_empty());
        assert_eq!(sheet, before, "search does not rewrite note source");

        let notes_only = Facets {
            notes_only: true,
            ..Default::default()
        };
        let hits = search_faceted(
            &sheet,
            0,
            "Café research",
            &SearchOptions::default(),
            &notes_only,
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].field, MatchField::Notes);
        assert!(
            search_faceted(
                &sheet,
                0,
                "example.invalid",
                &SearchOptions::default(),
                &notes_only
            )
            .is_empty()
        );
    }

    #[test]
    fn oversized_markdown_notes_fall_back_to_complete_original_text_for_search() {
        let mut sheet = s();
        let topic = sheet.children(sheet.root)[0];
        let source = format!(
            "**{}** and searchable fallback",
            "a".repeat(crate::notes::MAX_PROJECTION_BYTES)
        );
        sheet.topic_mut(topic).unwrap().notes = Some(source.clone().into());
        let options = SearchOptions {
            include_notes: true,
            ..Default::default()
        };
        let hit = search_sheet(&sheet, 0, "searchable fallback", &options);
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].topic, topic);
        assert_eq!(hit[0].field, MatchField::Notes);
        assert!(!search_sheet(&sheet, 0, "**", &options).is_empty());
        assert_eq!(
            sheet.topic(topic).unwrap().notes.as_deref(),
            Some(source.as_str())
        );
    }

    #[test]
    fn replace() {
        let mut s = s();
        let ids = s.all_ids_ordered();
        let n = replace_in_titles(&mut s, &ids, "plan", "Goal", &SearchOptions::default()).unwrap();
        assert_eq!(n, 3);
        assert_eq!(s.root_topic().title, "Project Goal");
    }

    #[test]
    fn case_insensitive_replacement_uses_original_unicode_boundaries() {
        let mut sheet = Sheet::new("Board", "İ foo and İ FOO");
        let root = sheet.root;
        let options = SearchOptions::default();
        assert!(matches(&sheet.root_topic().title, "foo", &options));
        assert_eq!(
            replace_in_titles(&mut sheet, &[root], "foo", "bar", &options).unwrap(),
            2
        );
        assert_eq!(sheet.root_topic().title, "İ bar and İ bar");
        sheet.topic_mut(root).unwrap().title = "i\u{307} İ".into();
        assert_eq!(
            replace_in_titles(&mut sheet, &[root], "İ", "X", &options).unwrap(),
            2
        );
        assert_eq!(sheet.root_topic().title, "X X");
        assert!(!matches("İ", "i", &options));
        assert!(matches("İi", "i", &options));
        let whole = SearchOptions {
            whole_word: true,
            ..options
        };
        assert!(!matches("İfoobar", "foo", &whole));
    }

    #[test]
    fn title_replacement_rejects_locked_cards_without_partial_changes() {
        use crate::{Card, CardKind, Vec2, board::BoardMember};

        let mut sheet = Sheet::new("Board", "Needle root");
        let root = sheet.root;
        let card = sheet.add_floating("Needle card", Vec2::new(0.0, 100.0));
        sheet
            .set_topic_card(
                card,
                Some(Card::new(CardKind::Sticky, Vec2::new(100.0, 80.0))),
            )
            .unwrap();
        sheet
            .set_board_members_locked(&[BoardMember::Card(card)], true)
            .unwrap();
        let before = sheet.clone();
        assert!(
            replace_in_titles(
                &mut sheet,
                &[root, card],
                "needle",
                "Goal",
                &SearchOptions::default(),
            )
            .is_err()
        );
        assert_eq!(sheet, before);
    }

    #[test]
    fn title_replacement_rejects_formatted_cards_without_partial_changes() {
        use crate::{Card, CardKind, Vec2};

        let mut sheet = Sheet::new("Board", "Needle root");
        let root = sheet.root;
        let card = sheet.add_floating("Needle card", Vec2::new(0.0, 100.0));
        sheet
            .set_topic_card(
                card,
                Some(Card::new(CardKind::Sticky, Vec2::new(100.0, 80.0))),
            )
            .unwrap();
        sheet
            .topic_mut(card)
            .unwrap()
            .inline_text
            .update_range("Needle card", 0..6, |style| style.bold = Some(true))
            .unwrap();
        let before = sheet.clone();

        assert!(
            replace_in_titles(
                &mut sheet,
                &[root, card],
                "needle",
                "Goal",
                &SearchOptions::default(),
            )
            .is_err()
        );
        assert_eq!(sheet, before);
        assert!(sheet.set_title(card, "Changed").is_err());
        assert_eq!(sheet, before);
    }

    #[test]
    fn canvas_search_and_replace_are_atomic_undoable_and_respect_locks() {
        use crate::{
            CanvasConnector, CanvasObject, Color, Document, Endpoint, InkStroke, InkTool, Rect,
            ShapeKind, SheetRootMode, Vec2, Workbook,
        };
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let mut shape = CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(80.0, 60.0)),
        );
        if let CanvasObjectKind::Shape { label, .. } = &mut shape.kind {
            *label = "alpha alpha".into();
        }
        shape.alt_text = "alpha visual".into();
        let shape = sheet.add_canvas_object(shape).unwrap();
        let mut stroke = CanvasObject::ink(InkStroke::new(
            InkTool::Pen,
            Color::rgb(0, 0, 0),
            2.0,
            vec![Vec2::ZERO, Vec2::new(10.0, 10.0)],
        ));
        stroke.alt_text = "alpha ink".into();
        let stroke = sheet.add_canvas_object(stroke).unwrap();
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.label = "alpha link".into();
        let connector = sheet
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let options = SearchOptions::default();
        let hits = search_canvas(&sheet, 2, "ALPHA", &options);
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[0].field, CanvasMatchField::Label);
        assert_eq!(hits[1].field, CanvasMatchField::AltText);
        assert!(hits.iter().all(|hit| hit.sheet == 2));
        let ids = [shape, stroke, connector];
        sheet.set_canvas_objects_locked(&[connector], true).unwrap();
        let locked = sheet.clone();
        assert!(replace_in_canvas(&mut sheet, &ids, "alpha", "beta", &options).is_err());
        assert_eq!(sheet, locked);
        sheet
            .set_canvas_objects_locked(&[connector], false)
            .unwrap();
        let mut doc = Document::new(Workbook::new(sheet));
        let before = doc.sheet().clone();
        let count = doc
            .edit_sheet("Replace board text", |sheet| {
                replace_in_canvas(sheet, &ids, "alpha", "beta", &options)
                    .map_err(crate::EditError::Invalid)
            })
            .unwrap();
        assert_eq!(count, 5);
        assert_eq!(search_canvas(doc.sheet(), 0, "beta", &options).len(), 3);
        assert!(doc.undo());
        assert_eq!(doc.sheet(), &before);
        assert!(doc.redo());
        assert_eq!(search_canvas(doc.sheet(), 0, "beta", &options).len(), 3);

        let mut invalid = before.clone();
        assert!(
            replace_in_canvas(
                &mut invalid,
                &[shape],
                "alpha",
                &"x".repeat(crate::board::MAX_BOARD_TEXT_CHARS + 1),
                &options,
            )
            .is_err()
        );
        assert_eq!(invalid, before);
    }

    #[test]
    fn filter_and_count() {
        let s = s();
        let f = filter_topics(&s, &["priority-1".into()], &[]);
        assert_eq!(f.len(), 2);
        assert_eq!(word_count(&s), 5);
    }

    #[test]
    fn hidden_anchor_is_excluded_from_search_filters_and_statistics() {
        let mut sheet = Sheet::new("Board", "Secret anchor");
        sheet.root_mode = crate::SheetRootMode::Hidden;
        let anchor = sheet.root;
        sheet
            .topic_mut(anchor)
            .unwrap()
            .markers
            .push("flag-red".into());
        let later = sheet.add_floating("Later", crate::Vec2::new(0.0, 100.0));
        let first = sheet.add_floating("First idea", crate::Vec2::new(0.0, 0.0));
        let child = sheet.add_child(first, None, "Detail");
        sheet
            .topic_mut(first)
            .unwrap()
            .markers
            .push("flag-red".into());
        let options = SearchOptions::default();
        let facets = Facets {
            markers: vec!["flag-red".into()],
            ..Default::default()
        };
        assert!(search_sheet(&sheet, 0, "Secret", &options).is_empty());
        assert_eq!(
            search_faceted(&sheet, 0, "", &options, &facets)
                .iter()
                .map(|hit| hit.topic)
                .collect::<Vec<_>>(),
            vec![first]
        );
        assert!(!facets.matches(&sheet, anchor));
        assert_eq!(sheet.content_ids_ordered(), vec![first, child, later]);
        assert_eq!(sheet.topic_count(), 3);
        assert_eq!(word_count(&sheet), 4);
        assert_eq!(
            replace_in_titles(&mut sheet, &[anchor], "Secret", "Changed", &options).unwrap(),
            0
        );
        assert_eq!(sheet.topic(anchor).unwrap().title, "Secret anchor");
    }

    #[test]
    fn facets_combine_or_values_and_groups_without_mutation() {
        let mut sheet = s();
        let topic = sheet.children(sheet.root)[0];
        sheet.topic_mut(topic).unwrap().todo = Some(false);
        sheet
            .topic_mut(topic)
            .unwrap()
            .markers
            .push("flag-red".into());
        sheet.scheduling.tasks.insert(
            topic,
            crate::schedule::Task {
                progress_percent: 100,
                priority: Some(3),
                ..Default::default()
            },
        );
        let before = sheet.clone();
        let mut facets = Facets {
            labels: vec!["finance".into(), "other".into()],
            markers: vec!["flag-blue".into(), "flag-red".into()],
            priorities: vec![3],
            todos: vec![Completion::Incomplete],
            tasks: vec![Completion::Complete],
            ..Default::default()
        };
        let hits = search_faceted(&sheet, 4, "", &SearchOptions::default(), &facets);
        assert_eq!(
            hits,
            vec![SearchHit {
                sheet: 4,
                topic,
                field: MatchField::Title
            }]
        );
        facets.priorities = vec![1];
        assert!(facets.matches(&sheet, topic), "priority markers also match");
        facets.tasks = vec![Completion::Incomplete];
        assert!(!facets.matches(&sheet, topic));
        facets.tasks.push(Completion::Complete);
        facets.labels = vec!["missing".into()];
        assert!(!facets.matches(&sheet, topic));
        assert_eq!(sheet, before);
    }

    #[test]
    fn notes_overview_searches_notes_not_titles_and_includes_hidden_and_floating_topics() {
        let mut sheet = s();
        let child = sheet.children(sheet.root)[0];
        let floating = sheet.add_floating("Floating", crate::Vec2::ZERO);
        sheet.topic_mut(floating).unwrap().notes = Some("APPROVAL memo".into());
        sheet.topic_mut(sheet.root).unwrap().collapsed = true;
        let facets = Facets {
            notes_only: true,
            ..Default::default()
        };
        let opts = SearchOptions::default();
        let hits = search_faceted(&sheet, 0, "", &opts, &facets);
        assert_eq!(
            hits.iter().map(|hit| hit.topic).collect::<Vec<_>>(),
            vec![child, floating]
        );
        assert!(hits.iter().all(|hit| hit.field == MatchField::Notes));
        assert_eq!(
            search_faceted(&sheet, 0, "approval", &opts, &facets).len(),
            2
        );
        assert!(search_faceted(&sheet, 0, "Plan budget", &opts, &facets).is_empty());
        assert_eq!(
            search_faceted(
                &sheet,
                0,
                "APPROVAL",
                &SearchOptions {
                    case_sensitive: true,
                    ..Default::default()
                },
                &facets
            )
            .len(),
            1
        );
        assert!(search_faceted(&sheet, 0, "", &opts, &Facets::default()).is_empty());
    }
}
