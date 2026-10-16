//! High-level editing operations on a [`Sheet`]. All operations validate their input and
//! keep the sheet consistent; they are wrapped by [`crate::Document`] for undo/redo.

use crate::geometry::{Color, Vec2};
use crate::model::*;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EditError {
    #[error("topic {0} not found")]
    NoSuchTopic(TopicId),
    #[error("the central topic cannot be used for this operation")]
    RootNotAllowed,
    #[error("cannot move a topic into its own subtree")]
    Cycle,
    #[error("topics must be siblings")]
    NotSiblings,
    #[error("nothing selected")]
    Empty,
    #[error("item not found")]
    NotFound,
    #[error("{0}")]
    Invalid(String),
}

pub type EditResult<T> = Result<T, EditError>;

impl Sheet {
    fn require(&self, id: TopicId) -> EditResult<()> {
        if self.contains(id) {
            Ok(())
        } else {
            Err(EditError::NoSuchTopic(id))
        }
    }

    /// Adds a new last child (Tab).
    pub fn insert_child(&mut self, parent: TopicId, title: &str) -> EditResult<TopicId> {
        self.require(parent)?;
        self.topic_mut(parent).unwrap().collapsed = false;
        Ok(self.add_child(parent, None, title))
    }

    /// Adds a sibling after `id` (Enter). On a top-level topic a child is added instead.
    pub fn insert_sibling_after(&mut self, id: TopicId, title: &str) -> EditResult<TopicId> {
        self.require(id)?;
        match self.parent(id) {
            Some(p) => {
                let i = self.index_in_parent(id).unwrap();
                Ok(self.add_child(p, Some(i + 1), title))
            }
            None => self.insert_child(id, title),
        }
    }

    /// Adds a sibling before `id` (Shift+Enter).
    pub fn insert_sibling_before(&mut self, id: TopicId, title: &str) -> EditResult<TopicId> {
        self.require(id)?;
        match self.parent(id) {
            Some(p) => {
                let i = self.index_in_parent(id).unwrap();
                Ok(self.add_child(p, Some(i), title))
            }
            None => self.insert_child(id, title),
        }
    }

    /// Inserts a new parent between `id` and its parent (Ctrl+Enter).
    pub fn insert_parent(&mut self, id: TopicId, title: &str) -> EditResult<TopicId> {
        self.require(id)?;
        let p = self.parent(id).ok_or(EditError::RootNotAllowed)?;
        let i = self.index_in_parent(id).unwrap();
        let sub = self.detach_subtree_keep_refs(id)?;
        let new = self.add_child(p, Some(i), title);
        self.attach_child(new, 0, sub);
        Ok(new)
    }

    /// Like `detach_subtree` but keeps relationships (used for moves).
    fn detach_subtree_keep_refs(&mut self, id: TopicId) -> EditResult<Subtree> {
        let rels = self.relationships.clone();
        let sub = self.detach_subtree(id).ok_or(EditError::RootNotAllowed)?;
        // restore relationships whose endpoints are inside the moved subtree or still alive
        let ids: std::collections::HashSet<_> = sub.topics.iter().map(|t| t.id).collect();
        let alive = |t: TopicId, s: &Sheet| s.contains(t) || ids.contains(&t);
        let keep: Vec<_> = rels
            .into_iter()
            .filter(|r| alive(r.from, self) && alive(r.to, self))
            .collect();
        self.relationships = keep;
        Ok(sub)
    }

    /// Removes topics with their subtrees. The root is skipped. Returns the topic that
    /// should be selected afterwards.
    pub fn remove_topics(&mut self, ids: &[TopicId]) -> EditResult<Option<TopicId>> {
        let ids: Vec<_> = ids
            .iter()
            .copied()
            .filter(|i| *i != self.root && self.contains(*i))
            .collect();
        if ids.is_empty() {
            return Err(EditError::RootNotAllowed);
        }
        let first = ids[0];
        let next_sel = self.parent(first).map(|p| {
            let i = self.index_in_parent(first).unwrap_or(0);
            let sibs = self.children(p);
            sibs.iter()
                .skip(i + 1)
                .chain(sibs[..i].iter().rev())
                .copied()
                .find(|s| !ids.contains(s))
                .unwrap_or(p)
        });
        for id in ids {
            if self.contains(id) {
                self.detach_subtree(id);
            }
        }
        Ok(next_sel.filter(|s| self.contains(*s)).or(Some(self.root)))
    }

    /// Removes a topic but keeps its children (moved to the topic's parent).
    pub fn remove_topic_keep_children(&mut self, id: TopicId) -> EditResult<()> {
        let p = self.parent(id).ok_or(EditError::RootNotAllowed)?;
        let mut i = self.index_in_parent(id).unwrap();
        let kids = self.children(id).to_vec();
        for k in kids {
            let sub = self.detach_subtree_keep_refs(k)?;
            i += 1;
            self.attach_child(p, i, sub);
        }
        self.detach_subtree(id);
        Ok(())
    }

    /// Duplicates `id` (with subtree) as the next sibling.
    pub fn duplicate(&mut self, id: TopicId) -> EditResult<TopicId> {
        self.require(id)?;
        let mut sub = self.copy_subtree(id).unwrap();
        sub.reassign_ids(self);
        match self.parent(id) {
            Some(p) => {
                let i = self.index_in_parent(id).unwrap();
                Ok(self.attach_child(p, i + 1, sub))
            }
            None => {
                let pos = self.topic(id).and_then(|t| t.position).unwrap_or_default()
                    + Vec2::new(40.0, 40.0);
                Ok(self.attach_floating(pos, sub))
            }
        }
    }

    /// Moves `id` under `new_parent` at `index` (drag & drop).
    pub fn move_topic(&mut self, id: TopicId, new_parent: TopicId, index: usize) -> EditResult<()> {
        self.require(id)?;
        self.require(new_parent)?;
        if id == self.root {
            return Err(EditError::RootNotAllowed);
        }
        if self.is_ancestor_or_self(id, new_parent) {
            return Err(EditError::Cycle);
        }
        let mut index = index;
        if self.parent(id) == Some(new_parent) {
            let cur = self.index_in_parent(id).unwrap();
            if cur < index {
                index -= 1;
            }
        }
        let sub = self.detach_subtree_keep_refs(id)?;
        self.attach_child(new_parent, index, sub);
        Ok(())
    }

    /// Moves `id` to be a floating topic at `pos`.
    pub fn detach_to_floating(&mut self, id: TopicId, pos: Vec2) -> EditResult<()> {
        self.require(id)?;
        if id == self.root {
            return Err(EditError::RootNotAllowed);
        }
        let sub = self.detach_subtree_keep_refs(id)?;
        self.attach_floating(pos, sub);
        Ok(())
    }

    /// Sets the position of a floating topic.
    pub fn set_position(&mut self, id: TopicId, pos: Vec2) -> EditResult<()> {
        self.require(id)?;
        if !self.is_floating(id) {
            return Err(EditError::Invalid("not a floating topic".into()));
        }
        self.topic_mut(id).unwrap().position = Some(pos);
        Ok(())
    }

    /// Moves a topic one place up (`-1`) or down (`+1`) among its siblings.
    pub fn reorder(&mut self, id: TopicId, delta: isize) -> EditResult<()> {
        let p = self.parent(id).ok_or(EditError::RootNotAllowed)?;
        let i = self.index_in_parent(id).unwrap() as isize;
        let n = self.children(p).len() as isize;
        let j = i + delta;
        if j < 0 || j >= n {
            return Ok(());
        }
        self.move_topic(
            id,
            p,
            if delta > 0 {
                (j + 1) as usize
            } else {
                j as usize
            },
        )
    }

    /// Sorts the children of `parent` by title (case-insensitive, natural order for digits).
    pub fn sort_children(&mut self, parent: TopicId, descending: bool) -> EditResult<()> {
        self.require(parent)?;
        let mut kids = self.children(parent).to_vec();
        kids.sort_by(|a, b| {
            let ta = natural_key(&self.topics[a].title);
            let tb = natural_key(&self.topics[b].title);
            if descending { tb.cmp(&ta) } else { ta.cmp(&tb) }
        });
        self.topic_mut(parent).unwrap().children = kids;
        Ok(())
    }

    pub fn set_title(&mut self, id: TopicId, title: &str) -> EditResult<()> {
        self.require(id)?;
        self.topic_mut(id).unwrap().title = title.to_string();
        Ok(())
    }

    pub fn set_notes(&mut self, id: TopicId, notes: Option<String>) -> EditResult<()> {
        self.require(id)?;
        self.topic_mut(id).unwrap().notes = notes.filter(|n| !n.trim().is_empty());
        Ok(())
    }

    pub fn set_hyperlink(&mut self, id: TopicId, link: Option<String>) -> EditResult<()> {
        self.require(id)?;
        self.topic_mut(id).unwrap().hyperlink = link.filter(|n| !n.trim().is_empty());
        Ok(())
    }

    pub fn set_labels(&mut self, id: TopicId, labels: Vec<String>) -> EditResult<()> {
        self.require(id)?;
        let mut clean: Vec<String> = Vec::new();
        for l in labels
            .into_iter()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
        {
            if !clean.contains(&l) {
                clean.push(l);
            }
        }
        self.topic_mut(id).unwrap().labels = clean;
        Ok(())
    }

    pub fn toggle_marker(&mut self, ids: &[TopicId], marker: &str) -> EditResult<()> {
        for id in ids {
            self.require(*id)?;
            crate::markers::toggle(&mut self.topic_mut(*id).unwrap().markers, marker);
        }
        Ok(())
    }

    pub fn set_collapsed(&mut self, id: TopicId, collapsed: bool) -> EditResult<()> {
        self.require(id)?;
        self.topic_mut(id).unwrap().collapsed = collapsed && !self.children(id).is_empty();
        Ok(())
    }

    /// Collapses (`true`) or expands every topic below `id`.
    pub fn set_collapsed_all(&mut self, id: TopicId, collapsed: bool) -> EditResult<()> {
        self.require(id)?;
        for t in self.subtree_ids(id) {
            if t == id && collapsed {
                continue; // keep the starting topic itself expanded
            }
            let has_kids = !self.children(t).is_empty();
            self.topic_mut(t).unwrap().collapsed = collapsed && has_kids;
        }
        Ok(())
    }

    /// Applies a style mutation to all `ids`.
    pub fn update_style(&mut self, ids: &[TopicId], f: impl Fn(&mut TopicStyle)) -> EditResult<()> {
        for id in ids {
            self.require(*id)?;
            f(&mut self.topic_mut(*id).unwrap().style);
        }
        Ok(())
    }

    pub fn set_structure(
        &mut self,
        id: Option<TopicId>,
        structure: Option<Structure>,
    ) -> EditResult<()> {
        match id {
            Some(id) if id != self.root => {
                self.require(id)?;
                self.topic_mut(id).unwrap().structure = structure;
            }
            _ => {
                self.structure = structure.unwrap_or_default();
                let root = self.root;
                if let Some(r) = self.topic_mut(root) {
                    r.structure = None;
                }
            }
        }
        Ok(())
    }

    pub fn set_todo(&mut self, id: TopicId, todo: Option<bool>) -> EditResult<()> {
        self.require(id)?;
        self.topic_mut(id).unwrap().todo = todo;
        Ok(())
    }

    pub fn add_relationship(&mut self, from: TopicId, to: TopicId) -> EditResult<RelationshipId> {
        self.require(from)?;
        self.require(to)?;
        if from == to {
            return Err(EditError::Invalid("relationship needs two topics".into()));
        }
        let id = self.alloc_relationship_id();
        self.relationships.push(Relationship {
            id,
            from,
            to,
            title: String::new(),
            color: None,
        });
        Ok(id)
    }

    pub fn remove_relationship(&mut self, id: RelationshipId) -> EditResult<()> {
        let n = self.relationships.len();
        self.relationships.retain(|r| r.id != id);
        if n == self.relationships.len() {
            Err(EditError::NotFound)
        } else {
            Ok(())
        }
    }

    pub fn set_relationship_title(&mut self, id: RelationshipId, title: &str) -> EditResult<()> {
        let r = self
            .relationships
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or(EditError::NotFound)?;
        r.title = title.to_string();
        Ok(())
    }

    pub fn set_relationship_color(
        &mut self,
        id: RelationshipId,
        color: Option<Color>,
    ) -> EditResult<()> {
        let r = self
            .relationships
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or(EditError::NotFound)?;
        r.color = color;
        Ok(())
    }

    /// Validates that `ids` are siblings and returns their (parent, range).
    fn sibling_range(&self, ids: &[TopicId]) -> EditResult<(TopicId, ChildRange)> {
        let first = *ids.first().ok_or(EditError::Empty)?;
        self.require(first)?;
        let p = self.parent(first).ok_or(EditError::RootNotAllowed)?;
        let mut idx = Vec::new();
        for id in ids {
            if self.parent(*id) != Some(p) {
                return Err(EditError::NotSiblings);
            }
            idx.push(self.index_in_parent(*id).unwrap());
        }
        let start = *idx.iter().min().unwrap();
        let end = *idx.iter().max().unwrap();
        Ok((p, ChildRange { start, end }))
    }

    /// Adds a boundary around the given sibling topics, or around a single top level topic.
    pub fn add_boundary(&mut self, ids: &[TopicId]) -> EditResult<BoundaryId> {
        let first = *ids.first().ok_or(EditError::Empty)?;
        self.require(first)?;
        let (parent, range) = if ids.len() == 1 && self.parent(first).is_none() {
            (first, None)
        } else {
            let (p, r) = self.sibling_range(ids)?;
            (p, Some(r))
        };
        let id = self.alloc_boundary_id();
        self.boundaries.push(Boundary {
            id,
            parent,
            range,
            title: None,
            color: None,
        });
        Ok(id)
    }

    pub fn remove_boundary(&mut self, id: BoundaryId) -> EditResult<()> {
        let n = self.boundaries.len();
        self.boundaries.retain(|b| b.id != id);
        if n == self.boundaries.len() {
            Err(EditError::NotFound)
        } else {
            Ok(())
        }
    }

    pub fn set_boundary_title(&mut self, id: BoundaryId, title: Option<String>) -> EditResult<()> {
        let b = self
            .boundaries
            .iter_mut()
            .find(|b| b.id == id)
            .ok_or(EditError::NotFound)?;
        b.title = title.filter(|t| !t.is_empty());
        Ok(())
    }

    /// Adds a summary for the given sibling topics; returns the new summary topic.
    pub fn add_summary(&mut self, ids: &[TopicId], title: &str) -> EditResult<TopicId> {
        let (parent, range) = self.sibling_range(ids)?;
        let topic = self.alloc_topic_id();
        self.topics.insert(topic, Topic::new(topic, title));
        let id = self.alloc_summary_id();
        self.summaries.push(Summary {
            id,
            parent,
            range,
            topic,
        });
        Ok(topic)
    }

    /// Removes the annotation and its owned topic tree, preserving the covered branches.
    pub fn remove_summary(&mut self, id: SummaryId) -> EditResult<()> {
        let topic = self
            .summaries
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.topic)
            .ok_or(EditError::NotFound)?;
        self.detach_subtree(topic).ok_or(EditError::NotFound)?;
        Ok(())
    }

    /// Pastes subtrees as children of `parent` (fresh ids). Returns the new roots.
    pub fn paste(&mut self, parent: TopicId, subtrees: &[Subtree]) -> EditResult<Vec<TopicId>> {
        self.require(parent)?;
        let mut out = Vec::new();
        for s in subtrees {
            let mut s = s.clone();
            s.reassign_ids(self);
            out.push(self.attach_child(parent, usize::MAX, s));
        }
        self.topic_mut(parent).unwrap().collapsed = false;
        Ok(out)
    }
}

fn natural_key(s: &str) -> Vec<(u8, String, u64)> {
    let mut out = Vec::new();
    let mut num = String::new();
    let mut txt = String::new();
    for ch in s.to_lowercase().chars() {
        if ch.is_ascii_digit() {
            if !txt.is_empty() {
                out.push((1, std::mem::take(&mut txt), 0));
            }
            num.push(ch);
        } else {
            if !num.is_empty() {
                out.push((0, String::new(), num.parse().unwrap_or(u64::MAX)));
                num.clear();
            }
            txt.push(ch);
        }
    }
    if !num.is_empty() {
        out.push((0, String::new(), num.parse().unwrap_or(u64::MAX)));
    }
    if !txt.is_empty() {
        out.push((1, txt, 0));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet() -> (Sheet, Vec<TopicId>) {
        let mut s = Sheet::new("S", "R");
        let kids = (0..3)
            .map(|i| s.add_child(s.root, None, format!("T{i}")))
            .collect();
        (s, kids)
    }

    #[test]
    fn sibling_and_child_insertion() {
        let (mut s, k) = sheet();
        let a = s.insert_sibling_after(k[0], "after").unwrap();
        assert_eq!(s.index_in_parent(a), Some(1));
        let b = s.insert_sibling_before(k[0], "before").unwrap();
        assert_eq!(s.index_in_parent(b), Some(0));
        let c = s.insert_child(k[0], "child").unwrap();
        assert_eq!(s.parent(c), Some(k[0]));
        let r = s.insert_sibling_after(s.root, "x").unwrap();
        assert_eq!(s.parent(r), Some(s.root));
        s.validate().unwrap();
    }

    #[test]
    fn insert_parent_wraps_topic() {
        let (mut s, k) = sheet();
        let p = s.insert_parent(k[1], "P").unwrap();
        assert_eq!(s.index_in_parent(p), Some(1));
        assert_eq!(s.children(p), &[k[1]]);
        assert_eq!(s.insert_parent(s.root, "x"), Err(EditError::RootNotAllowed));
        s.validate().unwrap();
    }

    #[test]
    fn remove_selects_neighbour() {
        let (mut s, k) = sheet();
        assert_eq!(s.remove_topics(&[k[1]]).unwrap(), Some(k[2]));
        assert_eq!(s.remove_topics(&[k[2]]).unwrap(), Some(k[0]));
        assert_eq!(s.remove_topics(&[k[0]]).unwrap(), Some(s.root));
        assert_eq!(s.remove_topics(&[s.root]), Err(EditError::RootNotAllowed));
        s.validate().unwrap();
    }

    #[test]
    fn remove_keep_children() {
        let (mut s, k) = sheet();
        let c1 = s.add_child(k[0], None, "c1");
        let c2 = s.add_child(k[0], None, "c2");
        s.remove_topic_keep_children(k[0]).unwrap();
        assert_eq!(s.children(s.root), &[c1, c2, k[1], k[2]]);
        s.validate().unwrap();
    }

    #[test]
    fn move_and_cycle_detection() {
        let (mut s, k) = sheet();
        let c = s.add_child(k[0], None, "c");
        assert_eq!(s.move_topic(k[0], c, 0), Err(EditError::Cycle));
        s.move_topic(k[2], k[0], 0).unwrap();
        assert_eq!(s.children(k[0]), &[k[2], c]);
        s.move_topic(k[1], s.root, 0).unwrap();
        assert_eq!(s.children(s.root), &[k[1], k[0]]);
        s.validate().unwrap();
    }

    #[test]
    fn move_keeps_relationships() {
        let (mut s, k) = sheet();
        let r = s.add_relationship(k[0], k[1]).unwrap();
        s.move_topic(k[0], k[2], 0).unwrap();
        assert_eq!(s.relationships.len(), 1);
        assert_eq!(s.relationships[0].id, r);
        s.validate().unwrap();
    }

    #[test]
    fn reorder_and_sort() {
        let (mut s, k) = sheet();
        s.reorder(k[0], 1).unwrap();
        assert_eq!(s.children(s.root), &[k[1], k[0], k[2]]);
        s.reorder(k[2], -1).unwrap();
        assert_eq!(s.children(s.root), &[k[1], k[2], k[0]]);
        s.reorder(k[1], -1).unwrap(); // no-op at the edge
        let t10 = s.add_child(s.root, None, "T10");
        s.sort_children(s.root, false).unwrap();
        assert_eq!(s.children(s.root), &[k[0], k[1], k[2], t10]);
        s.sort_children(s.root, true).unwrap();
        assert_eq!(s.children(s.root)[0], t10);
    }

    #[test]
    fn duplicate_and_paste() {
        let (mut s, k) = sheet();
        s.add_child(k[0], None, "c");
        let d = s.duplicate(k[0]).unwrap();
        assert_eq!(s.index_in_parent(d), Some(1));
        assert_eq!(s.children(d).len(), 1);
        let sub = s.copy_subtree(k[0]).unwrap();
        let pasted = s.paste(k[2], &[sub.clone(), sub]).unwrap();
        assert_eq!(pasted.len(), 2);
        assert_ne!(pasted[0], pasted[1]);
        s.validate().unwrap();
    }

    #[test]
    fn floating_topics() {
        let (mut s, k) = sheet();
        s.detach_to_floating(k[0], Vec2::new(100.0, 50.0)).unwrap();
        assert!(s.is_floating(k[0]));
        s.set_position(k[0], Vec2::new(1.0, 2.0)).unwrap();
        assert!(s.set_position(k[1], Vec2::ZERO).is_err());
        s.move_topic(k[0], s.root, 0).unwrap();
        assert!(!s.is_floating(k[0]));
        assert!(s.topic(k[0]).unwrap().position.is_none());
        s.validate().unwrap();
    }

    #[test]
    fn boundaries_and_summaries() {
        let (mut s, k) = sheet();
        assert_eq!(s.add_boundary(&[k[0], s.root]), Err(EditError::NotSiblings));
        let b = s.add_boundary(&[k[2], k[1]]).unwrap();
        assert_eq!(s.boundaries[0].range, Some(ChildRange { start: 1, end: 2 }));
        let st = s.add_summary(&[k[0], k[1]], "Summary").unwrap();
        assert_eq!(s.top_of(st), st);
        let sc = s.insert_child(st, "below summary").unwrap();
        s.validate().unwrap();
        s.remove_topics(&[k[0]]).unwrap();
        assert_eq!(s.summaries[0].range, ChildRange { start: 0, end: 0 });
        s.remove_topics(&[k[1]]).unwrap();
        assert!(s.summaries.is_empty());
        assert!(!s.contains(sc));
        s.remove_boundary(b).unwrap();
        s.validate().unwrap();
    }

    #[test]
    fn summary_range_validation_and_sibling_edits_preserve_ownership() {
        let (mut s, k) = sheet();
        let before = s.clone();
        assert_eq!(s.add_summary(&[], "No range"), Err(EditError::Empty));
        assert_eq!(
            s.add_summary(&[s.root], "Root"),
            Err(EditError::RootNotAllowed)
        );
        assert_eq!(
            s.add_summary(&[k[0], s.root], "Mixed"),
            Err(EditError::NotSiblings)
        );
        assert_eq!(s, before);
        // The existing range contract includes intermediate siblings.
        let summary = s.add_summary(&[k[2], k[0]], "Conclusion").unwrap();
        assert_eq!(s.summaries[0].range, ChildRange { start: 0, end: 2 });
        assert_eq!(s.parent(summary), None);
        let child = s.insert_child(summary, "Summary child").unwrap();
        s.set_title(summary, "Edited conclusion").unwrap();
        let inserted = s.insert_sibling_before(k[1], "Within range").unwrap();
        assert_eq!(s.summaries[0].range, ChildRange { start: 0, end: 3 });
        s.remove_topics(&[k[0], inserted]).unwrap();
        assert_eq!(s.summaries[0].range, ChildRange { start: 0, end: 1 });
        assert_eq!(s.parent(child), Some(summary));
        assert_eq!(s.topic(summary).unwrap().title, "Edited conclusion");
        s.validate().unwrap();
        s.remove_topics(&[k[1], k[2]]).unwrap();
        assert!(s.summaries.is_empty());
        assert!(!s.contains(summary));
        assert!(!s.contains(child));
        s.validate().unwrap();
    }

    #[test]
    fn collapse_all() {
        let (mut s, k) = sheet();
        s.add_child(k[0], None, "c");
        s.set_collapsed_all(s.root, true).unwrap();
        assert!(!s.root_topic().collapsed);
        assert!(s.topic(k[0]).unwrap().collapsed);
        assert!(!s.topic(k[1]).unwrap().collapsed); // no children
        s.set_collapsed_all(s.root, false).unwrap();
        assert!(!s.topic(k[0]).unwrap().collapsed);
    }

    #[test]
    fn labels_dedup_and_markers() {
        let (mut s, k) = sheet();
        s.set_labels(k[0], vec!["a".into(), " a ".into(), "".into(), "b".into()])
            .unwrap();
        assert_eq!(s.topic(k[0]).unwrap().labels, vec!["a", "b"]);
        s.toggle_marker(&[k[0], k[1]], "priority-1").unwrap();
        assert_eq!(s.topic(k[1]).unwrap().markers, vec!["priority-1"]);
    }

    #[test]
    fn structure_on_root_sets_sheet_structure() {
        let (mut s, k) = sheet();
        s.set_structure(Some(s.root), Some(Structure::OrgDown))
            .unwrap();
        assert_eq!(s.structure, Structure::OrgDown);
        s.set_structure(Some(k[0]), Some(Structure::LogicRight))
            .unwrap();
        assert_eq!(
            s.topic(k[0]).unwrap().structure,
            Some(Structure::LogicRight)
        );
    }
    #[test]
    fn deleting_summary_cleans_owned_annotations_and_preserves_covered_branches() {
        let mut s = Sheet::new("S", "Root");
        let a = s.add_child(s.root, None, "A");
        let b = s.add_child(s.root, None, "B");
        let topic = s.add_summary(&[a, b], "Summary").unwrap();
        let summary_id = s.summaries[0].id;
        let child = s.add_child(topic, None, "Child");
        let other = s.add_child(topic, None, "Other");
        let nested = s.add_summary(&[child, other], "Nested").unwrap();
        let nested_child = s.add_child(nested, None, "Nested child");
        s.add_boundary(&[topic]).unwrap();
        s.add_boundary(&[nested]).unwrap();
        s.add_boundary(&[nested_child]).unwrap();
        s.add_relationship(a, child).unwrap();
        s.add_relationship(b, nested_child).unwrap();
        let kept_boundary = s.add_boundary(&[a, b]).unwrap();
        let kept_relationship = s.add_relationship(a, b).unwrap();
        s.remove_summary(summary_id).unwrap();
        assert_eq!(s.all_ids_ordered(), vec![s.root, a, b]);
        assert_eq!(s.topics.len(), 3);
        assert!(s.summaries.is_empty());
        assert_eq!(s.boundaries.len(), 1);
        assert_eq!(s.boundaries[0].id, kept_boundary);
        assert_eq!(s.relationships.len(), 1);
        assert_eq!(s.relationships[0].id, kept_relationship);
        s.validate().unwrap();
        assert_eq!(s.remove_summary(summary_id), Err(EditError::NotFound));
    }
}
