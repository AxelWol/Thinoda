//! Derived checklist progress; never rewrites saved parent flags or scheduled Tasks.

use std::collections::{HashMap, HashSet};

use crate::{Sheet, TopicId};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TodoProgress {
    pub completed: usize,
    pub total: usize,
    pub nested: bool,
}

impl TodoProgress {
    pub fn complete(self) -> bool {
        self.total > 0 && self.completed == self.total
    }
}

impl Sheet {
    /// Count deepest To-Dos, crossing plain grouping topics but excluding Tasks.
    /// Intermediate To-Dos aggregate their descendants rather than count twice.
    pub fn todo_progress(&self) -> HashMap<TopicId, TodoProgress> {
        let mut order = Vec::new();
        let mut seen = HashSet::new();
        let mut stack = self.top_level_ids();
        while let Some(id) = stack.pop() {
            if seen.insert(id)
                && let Some(topic) = self.topic(id)
            {
                order.push(id);
                stack.extend(topic.children.iter().copied());
            }
        }
        let mut progress: HashMap<TopicId, TodoProgress> = HashMap::new();
        for id in order.into_iter().rev() {
            let mut count = TodoProgress::default();
            for child in self.children(id) {
                if let Some(child) = progress.get(child) {
                    count.completed += child.completed;
                    count.total += child.total;
                }
            }
            count.nested = count.total > 0;
            if !count.nested
                && let Some(done) = self.topics[&id].todo
            {
                count.total = 1;
                count.completed = usize::from(done);
            }
            if count.total > 0 {
                progress.insert(id, count);
            }
        }
        progress
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ApproxMeasure, Document, Workbook, layout_sheet,
        schedule::Task,
        search::{Completion, Facets, SearchOptions, search_faceted},
    };

    fn fixture() -> (Sheet, TopicId, TopicId) {
        let mut sheet = Sheet::new("Checklist", "Project");
        sheet.set_todo(sheet.root, Some(false)).unwrap();
        let group = sheet.add_child(sheet.root, None, "Grouping topic");
        let intermediate = sheet.add_child(group, None, "Intermediate");
        sheet.set_todo(intermediate, Some(true)).unwrap();
        let complete = sheet.add_child(intermediate, None, "Done");
        sheet.set_todo(complete, Some(true)).unwrap();
        let open = sheet.add_child(group, None, "Open");
        sheet.set_todo(open, Some(false)).unwrap();
        sheet.scheduling.tasks.insert(
            group,
            Task {
                progress_percent: 37,
                ..Default::default()
            },
        );
        (sheet, complete, open)
    }

    #[test]
    fn deepest_checkboxes_cross_plain_groups_without_counting_tasks_or_intermediate_flags() {
        let (mut sheet, _, _) = fixture();
        let floating = sheet.add_floating("Independent", crate::Vec2::ZERO);
        sheet.set_todo(floating, Some(false)).unwrap();
        sheet.topic_mut(sheet.root).unwrap().collapsed = true;
        let before = sheet.clone();
        let counts = sheet.todo_progress();
        assert_eq!(
            counts[&sheet.root],
            TodoProgress {
                completed: 1,
                total: 2,
                nested: true
            }
        );
        assert_eq!(
            counts[&floating],
            TodoProgress {
                completed: 0,
                total: 1,
                nested: false
            }
        );
        assert_eq!(sheet, before);
        for dark in [false, true] {
            let layout = layout_sheet(&sheet, &ApproxMeasure, dark);
            let root = &layout.topics[&sheet.root];
            assert_eq!(root.todo_progress, Some(counts[&sheet.root]));
            assert!(root.labels.iter().any(|(label, _)| label == "1/2"));
            assert!(sheet.topics[&sheet.root].labels.is_empty());
        }
    }

    #[test]
    fn subtree_completion_is_one_undo_preserves_tasks_and_old_saved_flags() {
        let (sheet, _, _) = fixture();
        let encoded = serde_json::to_string(&sheet).unwrap();
        assert!(!encoded.contains("todo_progress"));
        let restored: Sheet = serde_json::from_str(&encoded).unwrap();
        assert_eq!(restored, sheet);
        let root = sheet.root;
        let mut doc = Document::new(Workbook {
            sheets: vec![sheet.clone()],
            ..Default::default()
        });
        doc.edit_sheet("Complete", |s| s.set_todo_subtree(root, true))
            .unwrap();
        assert!(doc.sheet().todo_progress()[&root].complete());
        assert_eq!(doc.sheet().scheduling, sheet.scheduling);
        assert!(
            doc.sheet()
                .topics
                .values()
                .filter_map(|t| t.todo)
                .all(|done| done)
        );
        let revision = doc.state_id();
        doc.edit_sheet("Complete", |s| s.set_todo_subtree(root, true))
            .unwrap();
        assert_eq!(doc.state_id(), revision, "already complete is a no-op");
        assert!(doc.undo());
        assert_eq!(doc.sheet(), &sheet);
        assert!(!doc.undo(), "one completion is one history entry");
        assert!(doc.redo());
        doc.edit_sheet("Reopen", |s| s.set_todo_subtree(root, false))
            .unwrap();
        assert_eq!(doc.sheet().todo_progress()[&root].completed, 0);
        assert!(doc.undo());
        assert!(doc.sheet().todo_progress()[&root].complete());
    }

    #[test]
    fn adding_removing_and_reparenting_recomputes_without_cached_or_stored_parent_changes() {
        let (mut sheet, complete, open) = fixture();
        let root = sheet.root;
        let other = sheet.add_floating("Other tree", crate::Vec2::ZERO);
        sheet.move_topic(open, other, 0).unwrap();
        assert!(sheet.todo_progress()[&root].complete());
        assert_eq!(sheet.topics[&root].todo, Some(false));
        sheet.remove_topics(&[complete]).unwrap();
        // The intermediate checkbox is now the deepest To-Do.
        assert!(sheet.todo_progress()[&root].complete());
        let added = sheet.add_child(root, None, "New item");
        sheet.set_todo(added, Some(false)).unwrap();
        assert_eq!(sheet.todo_progress()[&root].total, 2);
        assert!(!sheet.todo_progress()[&root].complete());
        assert!(
            sheet.set_todo_subtree(other, true).is_err(),
            "do not silently add checkboxes"
        );
    }

    #[test]
    fn filters_use_derived_completion_not_the_saved_parent_flag() {
        let (mut sheet, _, open) = fixture();
        let root = sheet.root;
        sheet.set_todo(root, Some(true)).unwrap();
        let facets = Facets {
            todos: vec![Completion::Incomplete],
            ..Default::default()
        };
        assert!(
            search_faceted(&sheet, 0, "", &SearchOptions::default(), &facets)
                .iter()
                .any(|hit| hit.topic == root)
        );
        sheet.set_todo(open, Some(true)).unwrap();
        assert!(!facets.matches(&sheet, root));
        let facets = Facets {
            todos: vec![Completion::Complete],
            ..Default::default()
        };
        assert!(facets.matches(&sheet, root));
    }

    #[test]
    fn progress_aggregation_does_not_recurse_on_deep_trees() {
        let mut sheet = Sheet::new("Deep", "Root");
        let mut parent = sheet.root;
        sheet.set_todo(parent, Some(false)).unwrap();
        for _ in 0..5_000 {
            parent = sheet.add_child(parent, None, "Group");
        }
        sheet.set_todo(parent, Some(true)).unwrap();
        assert_eq!(
            sheet.todo_progress()[&sheet.root],
            TodoProgress {
                completed: 1,
                total: 1,
                nested: true
            }
        );
    }
}
