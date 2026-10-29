//! Find & replace and filtering across topics.

use crate::model::{Sheet, TopicId, Workbook};
use std::borrow::Cow;

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
            .all_ids_ordered()
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
            .all_ids_ordered()
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

fn find_in(hay: &str, needle: &str, opts: &SearchOptions) -> Vec<usize> {
    if needle.is_empty() {
        return vec![];
    }
    let (h, n) = if opts.case_sensitive {
        (hay.to_string(), needle.to_string())
    } else {
        (hay.to_lowercase(), needle.to_lowercase())
    };
    // lowercase may change byte lengths for some scripts; fall back to no matches then
    if h.len() != hay.len() || n.len() != needle.len() {
        return if h.contains(&n) { vec![0] } else { vec![] };
    }
    let mut out = Vec::new();
    let mut start = 0;
    while let Some(pos) = h[start..].find(&n) {
        let i = start + pos;
        let ok = !opts.whole_word || {
            let before = h[..i]
                .chars()
                .next_back()
                .is_none_or(|c| !c.is_alphanumeric());
            let after = h[i + n.len()..]
                .chars()
                .next()
                .is_none_or(|c| !c.is_alphanumeric());
            before && after
        };
        if ok {
            out.push(i);
        }
        start = i + n.len().max(1);
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
    for id in sheet.all_ids_ordered() {
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

/// Replaces all occurrences in titles of `ids`; returns the number of replacements.
pub fn replace_in_titles(
    sheet: &mut Sheet,
    ids: &[TopicId],
    needle: &str,
    replacement: &str,
    opts: &SearchOptions,
) -> usize {
    let mut count = 0;
    for id in ids {
        let Some(t) = sheet.topic_mut(*id) else {
            continue;
        };
        let positions = find_in(&t.title, needle, opts);
        if positions.is_empty() || (positions == [0] && !t.title.is_char_boundary(needle.len())) {
            continue;
        }
        let mut out = String::new();
        let mut last = 0;
        for p in &positions {
            if *p < last
                || !t.title.is_char_boundary(*p)
                || !t.title.is_char_boundary(p + needle.len())
            {
                continue;
            }
            out.push_str(&t.title[last..*p]);
            out.push_str(replacement);
            last = p + needle.len();
            count += 1;
        }
        out.push_str(&t.title[last..]);
        t.title = out;
    }
    count
}

/// Filter: topics having any of the given markers or labels (and their ancestors so the
/// tree stays connected). Used by the "Filter" feature.
pub fn filter_topics(sheet: &Sheet, markers: &[String], labels: &[String]) -> Vec<TopicId> {
    let mut keep = Vec::new();
    for id in sheet.all_ids_ordered() {
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
        .topics
        .values()
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
        let n = replace_in_titles(&mut s, &ids, "plan", "Goal", &SearchOptions::default());
        assert_eq!(n, 3);
        assert_eq!(s.root_topic().title, "Project Goal");
    }

    #[test]
    fn filter_and_count() {
        let s = s();
        let f = filter_topics(&s, &["priority-1".into()], &[]);
        assert_eq!(f.len(), 2);
        assert_eq!(word_count(&s), 5);
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
