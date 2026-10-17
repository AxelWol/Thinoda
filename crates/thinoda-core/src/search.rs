//! Find & replace and filtering across topics.

use crate::model::{Sheet, TopicId, Workbook};

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
        } else if opts.include_notes && t.notes.as_deref().is_some_and(|n| matches(n, needle, opts))
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
}
