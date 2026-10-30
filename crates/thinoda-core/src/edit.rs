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

/// Trims and deduplicates labels case-sensitively, retaining their original order.
pub fn normalize_labels(labels: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut clean = Vec::new();
    for label in labels {
        let label = label.trim();
        if !label.is_empty() && !clean.iter().any(|existing| existing == label) {
            clean.push(label.to_string());
        }
    }
    clean
}

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

    /// Includes the topic tree and all recursively owned Summary trees.
    pub fn owned_subtree_ids(&self, id: TopicId) -> std::collections::HashSet<TopicId> {
        let mut owned: std::collections::HashSet<_> = self.subtree_ids(id).into_iter().collect();
        loop {
            let before = owned.len();
            for summary in &self.summaries {
                if owned.contains(&summary.parent) {
                    owned.extend(self.subtree_ids(summary.topic));
                }
            }
            if before == owned.len() {
                break;
            }
        }
        owned
    }

    /// Like `detach_subtree` but retains relationships and recursively owned annotations.
    fn detach_subtree_keep_refs(&mut self, id: TopicId) -> EditResult<Subtree> {
        if self.summary_for_topic(id).is_some() {
            return Err(EditError::RootNotAllowed);
        }
        let owned = self.owned_subtree_ids(id);
        let boundaries: Vec<_> = self
            .boundaries
            .iter()
            .filter(|b| owned.contains(&b.parent))
            .cloned()
            .collect();
        let summaries: Vec<_> = self
            .summaries
            .iter()
            .filter(|s| owned.contains(&s.parent))
            .cloned()
            .collect();
        let annotation_topics: Vec<_> = owned
            .iter()
            .filter(|t| !self.is_ancestor_or_self(id, **t))
            .filter_map(|t| self.topic(*t).cloned())
            .collect();
        let rels = self.relationships.clone();
        let tasks = self.scheduling.tasks.clone();
        let dependencies = self.scheduling.dependencies.clone();
        let sub = self.detach_subtree(id).ok_or(EditError::RootNotAllowed)?;
        let sub_ids: std::collections::HashSet<_> =
            sub.topics.iter().map(|topic| topic.id).collect();
        self.scheduling.tasks = tasks;
        self.scheduling.dependencies = dependencies
            .into_iter()
            .filter(|dependency| {
                !sub_ids.contains(&dependency.predecessor)
                    || !sub_ids.contains(&dependency.successor)
            })
            .collect();
        for topic in annotation_topics {
            self.topics.insert(topic.id, topic);
        }
        self.boundaries.extend(boundaries);
        self.summaries.extend(summaries);
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
        if self.fim.is_some() {
            let roots: Vec<_> = ids
                .iter()
                .copied()
                .filter(|id| *id != self.root && self.contains(*id))
                .filter(|id| {
                    !ids.iter()
                        .any(|other| other != id && self.is_ancestor_or_self(*other, *id))
                })
                .collect();
            if roots.is_empty() {
                return Err(EditError::RootNotAllowed);
            }
            let selection = self.parent(roots[0]).or(Some(self.root));
            let mut candidate = self.clone();
            for topic in roots {
                if candidate.contains(topic) {
                    candidate
                        .apply_fim_change(crate::fim::FimChange::Remove { topic })
                        .map_err(|e| EditError::Invalid(e.to_string()))?;
                }
            }
            *self = candidate;
            return Ok(selection
                .filter(|id| self.contains(*id))
                .or(Some(self.root)));
        }
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
        if self.fim.is_some() {
            let parent = self.parent(id).ok_or(EditError::RootNotAllowed)?;
            let index = self.index_in_parent(id).ok_or(EditError::NotFound)?;
            let mut candidate = self.clone();
            for (offset, topic) in self.children(id).iter().copied().enumerate() {
                candidate
                    .apply_fim_change(crate::fim::FimChange::Move {
                        topic,
                        parent,
                        index: index + offset,
                    })
                    .map_err(|e| EditError::Invalid(e.to_string()))?;
            }
            candidate
                .apply_fim_change(crate::fim::FimChange::Remove { topic: id })
                .map_err(|e| EditError::Invalid(e.to_string()))?;
            *self = candidate;
            return Ok(());
        }
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
        if self.fim.is_some() {
            let previous: std::collections::BTreeSet<_> = self.topics.keys().copied().collect();
            self.apply_fim_change(crate::fim::FimChange::Duplicate { topic: id })
                .map_err(|e| EditError::Invalid(e.to_string()))?;
            return self
                .topics
                .keys()
                .copied()
                .find(|id| !previous.contains(id))
                .ok_or_else(|| {
                    EditError::Invalid("FIM duplicate did not create an occurrence".into())
                });
        }
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
        if self.owned_subtree_ids(id).contains(&new_parent) {
            return Err(EditError::Cycle);
        }
        if let (Some(parent), Some(index)) = (self.parent(id), self.index_in_parent(id))
            && self.summaries.iter().any(|summary| {
                summary.parent == parent
                    && summary.range.start == index
                    && summary.range.end == index
                    && self.owned_subtree_ids(summary.topic).contains(&new_parent)
            })
        {
            return Err(EditError::Invalid(
                "destination belongs to a Summary removed with its last source topic".into(),
            ));
        }
        let mut index = index;
        if self.parent(id) == Some(new_parent) {
            let cur = self.index_in_parent(id).unwrap();
            if cur < index {
                index -= 1;
            }
            if index.min(self.children(new_parent).len() - 1) == cur {
                return Ok(());
            }
        }
        if self.fim.is_some() {
            return self
                .apply_fim_change(crate::fim::FimChange::Move {
                    topic: id,
                    parent: new_parent,
                    index,
                })
                .map_err(|e| EditError::Invalid(e.to_string()));
        }
        let sub = self.detach_subtree_keep_refs(id)?;
        self.attach_child(new_parent, index, sub);
        Ok(())
    }

    pub fn indent(&mut self, id: TopicId) -> EditResult<()> {
        let parent = self.parent(id).ok_or(EditError::RootNotAllowed)?;
        let index = self.index_in_parent(id).ok_or(EditError::NotFound)?;
        let previous = index
            .checked_sub(1)
            .and_then(|i| self.children(parent).get(i))
            .copied()
            .ok_or_else(|| EditError::Invalid("no previous sibling to indent under".into()))?;
        self.move_topic(id, previous, self.children(previous).len())?;
        self.set_collapsed(previous, false)
    }

    pub fn outdent(&mut self, id: TopicId) -> EditResult<()> {
        let parent = self.parent(id).ok_or(EditError::RootNotAllowed)?;
        let grandparent = self.parent(parent).ok_or(EditError::RootNotAllowed)?;
        let index = self.index_in_parent(parent).ok_or(EditError::NotFound)? + 1;
        self.move_topic(id, grandparent, index)
    }

    /// Copies a branch with its owned annotations; unrelated floating trees and
    /// relationships to topics outside the branch remain only in the source.
    pub fn copy_branch_as_sheet(&self, id: TopicId) -> EditResult<Sheet> {
        if self.fim.is_some() {
            return Err(EditError::Invalid("FIM branch extraction requires an explicit closed-document workflow; duplicate/copy the complete sheet instead".into()));
        }
        self.require(id)?;
        let owned = self.owned_subtree_ids(id);
        let mut sheet = self.clone();
        sheet.resources = self.copy_subtree(id).ok_or(EditError::NotFound)?.resources;
        sheet.root = id;
        sheet.title = self.topic(id).unwrap().title.clone();
        sheet.topics.retain(|id, _| owned.contains(id));
        let root = sheet.topic_mut(id).unwrap();
        root.parent = None;
        root.position = None;
        sheet.floating.clear();
        sheet.boundaries.retain(|b| owned.contains(&b.parent));
        sheet
            .summaries
            .retain(|s| owned.contains(&s.parent) && s.topic != id);
        sheet
            .relationships
            .retain(|r| owned.contains(&r.from) && owned.contains(&r.to));
        sheet
            .scheduling
            .tasks
            .retain(|topic, _| owned.contains(topic));
        sheet.scheduling.dependencies.retain(|dependency| {
            owned.contains(&dependency.predecessor) && owned.contains(&dependency.successor)
        });
        sheet.validate().map_err(EditError::Invalid)?;
        Ok(sheet)
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
        let remap = |range: ChildRange| -> EditResult<ChildRange> {
            let original = &self.children(parent)[range.start..=range.end];
            let mut positions: Vec<_> = original
                .iter()
                .map(|id| kids.iter().position(|kid| kid == id).unwrap())
                .collect();
            positions.sort_unstable();
            let start = positions[0];
            let end = *positions.last().unwrap();
            if end - start + 1 != original.len() {
                return Err(EditError::Invalid(
                    "sorting would split a Summary or boundary source range".into(),
                ));
            }
            Ok(ChildRange { start, end })
        };
        let boundaries: Vec<_> = self
            .boundaries
            .iter()
            .enumerate()
            .filter(|(_, b)| b.parent == parent)
            .filter_map(|(index, b)| {
                b.range
                    .map(|range| remap(range).map(|range| (index, range)))
            })
            .collect::<EditResult<_>>()?;
        let summaries: Vec<_> = self
            .summaries
            .iter()
            .enumerate()
            .filter(|(_, s)| s.parent == parent)
            .map(|(index, s)| remap(s.range).map(|range| (index, range)))
            .collect::<EditResult<_>>()?;
        for (index, range) in boundaries {
            self.boundaries[index].range = Some(range);
        }
        for (index, range) in summaries {
            self.summaries[index].range = range;
        }
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
        let topic = self.topic_mut(id).unwrap();
        topic.notes = notes
            .filter(|n| !n.trim().is_empty())
            .map(|source| match &topic.notes {
                Some(note) => note.with_source(source),
                None => source.into(),
            });
        Ok(())
    }

    pub fn set_markdown_notes(&mut self, id: TopicId, notes: Option<String>) -> EditResult<()> {
        self.require(id)?;
        self.topic_mut(id).unwrap().notes = notes
            .filter(|n| !n.trim().is_empty())
            .map(crate::notes::NoteContent::markdown);
        Ok(())
    }

    pub fn set_hyperlink(&mut self, id: TopicId, link: Option<String>) -> EditResult<()> {
        self.require(id)?;
        self.topic_mut(id).unwrap().hyperlink = link.filter(|n| !n.trim().is_empty());
        Ok(())
    }

    pub fn set_labels(&mut self, id: TopicId, labels: Vec<String>) -> EditResult<()> {
        self.require(id)?;
        self.topic_mut(id).unwrap().labels = normalize_labels(labels);
        Ok(())
    }

    pub fn toggle_marker(&mut self, ids: &[TopicId], marker: &str) -> EditResult<()> {
        self.toggle_marker_color(ids, marker, None)
    }

    pub fn toggle_marker_color(
        &mut self,
        ids: &[TopicId],
        marker: &str,
        color: Option<Color>,
    ) -> EditResult<()> {
        for id in ids {
            self.require(*id)?;
        }
        for id in ids {
            let topic = self.topic(*id).unwrap();
            if let Some(index) = topic.markers.iter().position(|m| m == marker) {
                self.remove_marker(*id, index)?;
            } else {
                let index = topic
                    .markers
                    .iter()
                    .position(|m| crate::markers::group_of(m) == crate::markers::group_of(marker))
                    .unwrap_or(topic.markers.len());
                let topic = self.topic_mut(*id).unwrap();
                crate::markers::toggle(&mut topic.markers, marker);
                topic.marker_colors.remove(&index);
                if let Some(color) = color {
                    topic.marker_colors.insert(index, color);
                }
            }
        }
        Ok(())
    }

    pub fn set_marker_color(
        &mut self,
        id: TopicId,
        index: usize,
        color: Option<Color>,
    ) -> EditResult<()> {
        self.require(id)?;
        let topic = self.topic_mut(id).unwrap();
        if index >= topic.markers.len() {
            return Err(EditError::NotFound);
        }
        if let Some(color) = color {
            topic.marker_colors.insert(index, color);
        } else {
            topic.marker_colors.remove(&index);
        }
        Ok(())
    }

    pub fn remove_marker(&mut self, id: TopicId, index: usize) -> EditResult<()> {
        self.require(id)?;
        let topic = self.topic_mut(id).unwrap();
        if index >= topic.markers.len() {
            return Err(EditError::NotFound);
        }
        topic.markers.remove(index);
        topic.marker_colors = std::mem::take(&mut topic.marker_colors)
            .into_iter()
            .filter_map(|(i, c)| (i != index).then_some((if i > index { i - 1 } else { i }, c)))
            .collect();
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

    pub fn set_branch_side(
        &mut self,
        id: TopicId,
        side: Option<crate::BranchSide>,
    ) -> EditResult<()> {
        self.require(id)?;
        if self.parent(id).is_none() {
            return Err(EditError::RootNotAllowed);
        }
        self.topic_mut(id).unwrap().branch_side = side;
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

    pub fn set_todo_subtree(&mut self, id: TopicId, done: bool) -> EditResult<()> {
        self.require(id)?;
        if self.topics[&id].todo.is_none() {
            return Err(EditError::Invalid("topic has no To-Do".into()));
        }
        for id in self.subtree_ids(id) {
            let topic = self.topic_mut(id).unwrap();
            if topic.todo.is_some() {
                topic.todo = Some(done);
            }
        }
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
            control_points: None,
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

    /// Reconnects both endpoints atomically, retaining title, color and relative controls.
    pub fn reconnect_relationship(
        &mut self,
        id: RelationshipId,
        from: TopicId,
        to: TopicId,
    ) -> EditResult<()> {
        self.require(from)?;
        self.require(to)?;
        if from == to {
            return Err(EditError::Invalid(
                "relationship needs two different topics".into(),
            ));
        }
        let r = self
            .relationships
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or(EditError::NotFound)?;
        r.from = from;
        r.to = to;
        Ok(())
    }

    pub fn set_relationship_control_points(
        &mut self,
        id: RelationshipId,
        points: Option<[Vec2; 2]>,
    ) -> EditResult<()> {
        if !Relationship::valid_control_points(points) {
            return Err(EditError::Invalid(
                "curve controls must be finite and within +/- 1000000 map units".into(),
            ));
        }
        self.relationships
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or(EditError::NotFound)?
            .control_points = points;
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
        if self.fim.is_some() && !subtrees.is_empty() {
            return Err(EditError::Invalid(
                "FIM subtree paste requires an explicit identity and dependency decision; use Duplicate or Shared definition".into(),
            ));
        }
        self.require(parent)?;
        let resources = self
            .resources
            .merged(
                subtrees
                    .iter()
                    .flat_map(|subtree| subtree.resources.entries.values().cloned()),
            )
            .map_err(EditError::Invalid)?;
        for subtree in subtrees {
            subtree.resources.validate().map_err(EditError::Invalid)?;
            for topic in &subtree.topics {
                if let Some(attachment) = &topic.attachment {
                    if attachment.name != crate::attachments::safe_name(&attachment.name) {
                        return Err(EditError::Invalid("invalid attachment filename".into()));
                    }
                    resources
                        .get(&attachment.resource)
                        .map_err(EditError::Invalid)?;
                }
                if let Some(reference) = topic.image.as_deref()
                    && reference.starts_with(crate::resources::RESOURCE_PREFIX)
                {
                    resources.get(reference).map_err(EditError::Invalid)?;
                }
            }
        }
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
    fn indent_and_outdent_preserve_owned_annotations_and_content() {
        let (mut s, kids) = sheet();
        let branch = kids[1];
        let a = s.add_child(branch, None, "A");
        let b = s.add_child(branch, None, "B");
        let summary = s.add_summary(&[a, b], "Summary").unwrap();
        let summary_child = s.add_child(summary, None, "Summary child");
        let nested = s.add_summary(&[summary_child], "Nested").unwrap();
        s.add_boundary(&[a, b]).unwrap();
        s.add_boundary(&[summary_child]).unwrap();
        s.add_boundary(&[nested]).unwrap();
        s.add_relationship(a, summary_child).unwrap();
        s.add_relationship(nested, kids[0]).unwrap();
        s.topic_mut(branch).unwrap().notes = Some("Keep notes".into());
        s.topic_mut(branch).unwrap().style.bold = Some(true);
        s.set_collapsed(kids[0], true).unwrap();
        let before = s.clone();
        s.indent(branch).unwrap();
        assert_eq!(s.parent(branch), Some(kids[0]));
        assert!(!s.topic(kids[0]).unwrap().collapsed);
        assert_eq!(s.boundaries, before.boundaries);
        assert_eq!(s.summaries, before.summaries);
        assert_eq!(s.relationships, before.relationships);
        assert_eq!(s.topics.len(), before.topics.len());
        for id in [a, b, summary, summary_child, nested] {
            assert_eq!(s.topic(id), before.topic(id));
        }
        s.validate().unwrap();
        s.outdent(branch).unwrap();
        assert_eq!(s.children(s.root), kids);
        assert_eq!(s.boundaries, before.boundaries);
        assert_eq!(s.summaries, before.summaries);
        assert_eq!(s.relationships, before.relationships);
        s.validate().unwrap();
        let unchanged = s.clone();
        assert!(s.indent(kids[0]).is_err());
        assert!(s.outdent(kids[0]).is_err());
        assert!(s.indent(s.root).is_err());
        assert_eq!(s, unchanged);
    }

    #[test]
    fn sorting_preserves_annotation_members_and_rejects_split_ranges_atomically() {
        let (mut s, kids) = sheet();
        for (id, title) in kids.iter().zip(["C", "B", "A"]) {
            s.set_title(*id, title).unwrap();
        }
        s.add_summary(&kids[..2], "Summary").unwrap();
        s.add_boundary(&kids[..2]).unwrap();
        let before = s.clone();
        s.sort_children(s.root, false).unwrap();
        assert_eq!(s.children(s.root), &[kids[2], kids[1], kids[0]]);
        assert_eq!(s.summaries[0].range, ChildRange { start: 1, end: 2 });
        assert_eq!(s.boundaries[0].range, Some(ChildRange { start: 1, end: 2 }));
        s.sort_children(s.root, true).unwrap();
        assert_eq!(s, before);
        s.set_title(kids[1], "A").unwrap();
        s.set_title(kids[2], "B").unwrap();
        let before = s.clone();
        assert!(
            s.sort_children(s.root, false)
                .unwrap_err()
                .to_string()
                .contains("split")
        );
        assert_eq!(s, before);
        s.validate().unwrap();
    }

    #[test]
    fn reparent_rejects_annotation_cycles_without_mutating_and_cleans_source_ranges() {
        let (mut s, kids) = sheet();
        let child = s.add_child(kids[1], None, "child");
        let own_summary = s.add_summary(&[child], "owned").unwrap();
        let own_child = s.add_child(own_summary, None, "owned child");
        let source_summary = s.add_summary(&[kids[1]], "source").unwrap();
        let source_child = s.add_child(source_summary, None, "source child");
        let nested = s.add_summary(&[source_child], "nested source").unwrap();
        s.add_boundary(&[source_child]).unwrap();
        s.add_boundary(&[nested]).unwrap();
        let before = s.clone();
        for destination in [own_summary, own_child, source_summary, source_child, nested] {
            assert!(s.move_topic(kids[1], destination, 0).is_err());
            assert_eq!(s, before);
        }
        assert!(s.move_topic(own_summary, kids[0], 0).is_err());
        assert_eq!(s, before);
        s.indent(kids[1]).unwrap();
        assert!(!s.contains(source_summary));
        assert!(!s.contains(source_child));
        assert!(!s.contains(nested));
        assert!(s.contains(own_summary));
        assert!(s.contains(own_child));
        s.validate().unwrap();
    }

    #[test]
    fn copy_branch_to_sheet_handles_tree_summary_and_floating_roots() {
        let (mut s, kids) = sheet();
        let child = s.add_child(kids[1], None, "child");
        let summary = s.add_summary(&[child], "summary").unwrap();
        s.add_child(summary, None, "summary child");
        let floating = s.add_floating("Floating", Vec2::new(10.0, 20.0));
        s.add_boundary(&[child]).unwrap();
        s.add_relationship(child, summary).unwrap();
        s.add_relationship(child, kids[0]).unwrap();
        let before = s.clone();
        for id in [s.root, kids[1], summary, floating] {
            let copied = s.copy_branch_as_sheet(id).unwrap();
            copied.validate().unwrap();
            assert_eq!(copied.root, id);
            assert_eq!(copied.root_topic().parent, None);
            assert_eq!(copied.root_topic().position, None);
            assert_eq!(copied.title, s.topic(id).unwrap().title);
            assert!(copied.floating.is_empty());
            assert_eq!(copied.theme, s.theme);
            assert_eq!(copied.structure, s.structure);
            if id == kids[1] {
                assert_eq!(copied.topic_count(), 4);
                assert_eq!(copied.summaries.len(), 1);
                assert_eq!(copied.boundaries.len(), 1);
                assert_eq!(copied.relationships.len(), 1);
            }
        }
        assert_eq!(s, before);
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
