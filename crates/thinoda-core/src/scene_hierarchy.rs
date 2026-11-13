//! Shared, object-only container prototype for draw.io and socio-technical scenes.
//!
//! This is an immutable view over an existing canvas, not persisted document
//! state. Previews never mutate the source. Integration into live commands must
//! wait for the persistence, clipboard, history and binding gates in
//! `docs/SCENE-CONTAINERS.md`; there is deliberately no serialization interface.

use crate::board::{
    CanvasLayer, CanvasObjectId, CanvasObjectKind, Endpoint, MAX_CANVAS_GROUPS, MAX_CANVAS_OBJECTS,
    MAX_CARD_STATES, validate_object,
};
use crate::geometry::Vec2;
use crate::model::{Sheet, valid_position};
use std::collections::{HashMap, HashSet, VecDeque};

/// Maximum number of parent links from any object to its layer.
pub const MAX_SCENE_DEPTH: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SceneEditKind {
    Reparent { target: Option<CanvasObjectId> },
    Translate { delta: Vec2 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneParentChange {
    pub object: CanvasObjectId,
    /// Parent in the original hierarchy, before the complete edit.
    pub from: Option<CanvasObjectId>,
    /// Final reconciled parent.
    pub to: Option<CanvasObjectId>,
}

/// Read-only, fully reconciled candidate for a one-shot binding validator.
///
/// `geometry` is the proposed canvas; a translation leaves structural ancestry
/// unchanged. The callback may reject the entire preview, not mutate its inputs.
/// Live command integration must supply the document's binding validator rather
/// than using the unbound convenience methods.
pub struct SceneEditContext<'view, 'canvas> {
    pub kind: SceneEditKind,
    pub before: &'view SceneHierarchy<'canvas>,
    pub after: &'view SceneHierarchy<'canvas>,
    pub geometry: &'view CanvasLayer,
    pub selection: &'view [CanvasObjectId],
    pub affected: &'view [CanvasObjectId],
    /// Final edge-owner deltas in prior traversal order. Sibling order changes
    /// are observable through `before` and `after`, even if the owner is unchanged.
    pub edge_parent_changes: &'view [SceneParentChange],
}

/// One structural parent tree, with geometry still authoritative on the canvas.
///
/// Unlisted objects are layer roots. Initial sibling order comes from the source
/// canvas; reparenting appends moved siblings in their previous traversal order.
/// Only shapes can be containers. This first spike excludes card membership,
/// rotated containers, and hierarchy members in legacy flat groups.
#[derive(Debug, Clone)]
pub struct SceneHierarchy<'a> {
    canvas: &'a CanvasLayer,
    objects: HashMap<CanvasObjectId, usize>,
    locked: HashSet<CanvasObjectId>,
    containers: HashSet<CanvasObjectId>,
    parents: HashMap<CanvasObjectId, CanvasObjectId>,
    children: HashMap<Option<CanvasObjectId>, Vec<CanvasObjectId>>,
}

impl<'a> SceneHierarchy<'a> {
    /// Validates container IDs and `(child, parent)` links without changing the
    /// source. The canvas should come from a validated sheet; topic endpoints
    /// and other sheet-wide constraints remain the sheet validator's concern.
    pub fn new(
        canvas: &'a CanvasLayer,
        containers: &[CanvasObjectId],
        parents: &[(CanvasObjectId, CanvasObjectId)],
    ) -> Result<Self, String> {
        if canvas.objects.len() > MAX_CANVAS_OBJECTS
            || canvas.groups.len() > MAX_CANVAS_GROUPS
            || canvas.card_states.len() > MAX_CARD_STATES
            || containers.len() > canvas.objects.len()
            || parents.len() > canvas.objects.len()
        {
            return Err("scene hierarchy exceeds canvas limits".into());
        }
        let mut objects = HashMap::new();
        let mut groups = HashSet::new();
        let locked_groups: HashSet<_> = canvas
            .groups
            .iter()
            .filter(|group| group.locked)
            .map(|group| group.id)
            .collect();
        let mut locked = HashSet::new();
        for group in &canvas.groups {
            if !groups.insert(group.id) {
                return Err(format!("duplicate canvas group id {}", group.id));
            }
        }
        for (index, object) in canvas.objects.iter().enumerate() {
            validate_object(object)?;
            if objects.insert(object.id, index).is_some() {
                return Err(format!("duplicate canvas object id {}", object.id));
            }
            if object.locked
                || object
                    .group
                    .is_some_and(|group| locked_groups.contains(&group))
            {
                locked.insert(object.id);
            }
            if let Some(group) = object.group
                && !groups.contains(&group)
            {
                return Err(format!(
                    "canvas object {} references missing group {group}",
                    object.id
                ));
            }
        }
        let mut container_set = HashSet::new();
        for id in containers {
            if !container_set.insert(*id) {
                return Err(format!("duplicate scene container {id}"));
            }
            let object = objects
                .get(id)
                .map(|index| &canvas.objects[*index])
                .ok_or_else(|| format!("scene container {id} not found"))?;
            if !matches!(object.kind, CanvasObjectKind::Shape { .. }) {
                return Err(format!("scene container {id} is not a shape"));
            }
            if object.rotation != 0.0 {
                return Err(format!(
                    "rotated scene container {id} is not supported by this prototype"
                ));
            }
        }
        let mut parent_map = HashMap::new();
        for (child, parent) in parents {
            if !objects.contains_key(child) {
                return Err(format!("scene child {child} not found"));
            }
            if !container_set.contains(parent) {
                return Err(format!("scene parent {parent} is not a container"));
            }
            if parent_map.insert(*child, *parent).is_some() {
                return Err(format!("scene child {child} has more than one parent"));
            }
        }
        let mut children: HashMap<_, Vec<_>> = HashMap::new();
        for object in &canvas.objects {
            children
                .entry(parent_map.get(&object.id).copied())
                .or_default()
                .push(object.id);
        }
        let hierarchy = Self {
            canvas,
            objects,
            locked,
            containers: container_set,
            parents: parent_map,
            children,
        };
        hierarchy.validate_topology()?;
        Ok(hierarchy)
    }

    /// Ordered objects immediately under the layer.
    pub fn roots(&self) -> &[CanvasObjectId] {
        self.children.get(&None).map_or(&[], Vec::as_slice)
    }

    pub fn parent(&self, id: CanvasObjectId) -> Result<Option<CanvasObjectId>, String> {
        self.require_object(id)?;
        Ok(self.parents.get(&id).copied())
    }

    pub fn children(&self, container: CanvasObjectId) -> Result<&[CanvasObjectId], String> {
        self.require_container(container)?;
        Ok(self
            .children
            .get(&Some(container))
            .map_or(&[], Vec::as_slice))
    }

    /// Closest common container, including an endpoint itself when it is a
    /// container. Ordinary object endpoints start at their structural parent.
    pub fn nearest_common_container(
        &self,
        first: CanvasObjectId,
        second: CanvasObjectId,
    ) -> Result<Option<CanvasObjectId>, String> {
        self.require_object(first)?;
        self.require_object(second)?;
        let start = |id| {
            if self.containers.contains(&id) {
                Some(id)
            } else {
                self.parents.get(&id).copied()
            }
        };
        let mut ancestors = HashSet::new();
        let mut cursor = start(first);
        while let Some(id) = cursor {
            ancestors.insert(id);
            cursor = self.parents.get(&id).copied();
        }
        let mut cursor = start(second);
        while let Some(id) = cursor {
            if ancestors.contains(&id) {
                return Ok(Some(id));
            }
            cursor = self.parents.get(&id).copied();
        }
        Ok(None)
    }

    /// Parent-first traversal with stable sibling order, without recursion.
    pub fn preorder(&self) -> Vec<CanvasObjectId> {
        let mut pending: Vec<_> = self.roots().iter().rev().copied().collect();
        let mut ordered = Vec::with_capacity(self.objects.len());
        while let Some(id) = pending.pop() {
            ordered.push(id);
            if let Some(children) = self.children.get(&Some(id)) {
                pending.extend(children.iter().rev().copied());
            }
        }
        ordered
    }

    /// Unique descendant closure, including the selection, in source paint order.
    pub fn descendant_closure(
        &self,
        ids: &[CanvasObjectId],
    ) -> Result<Vec<CanvasObjectId>, String> {
        let selected = self.selection(ids)?;
        let closure = self.closure(&selected);
        Ok(self
            .canvas
            .objects
            .iter()
            .filter(|object| closure.contains(&object.id))
            .map(|object| object.id)
            .collect())
    }

    /// Previews reparenting selection roots; selected descendants stay attached
    /// to their selected ancestor. Sheet-space rectangles and controls are
    /// untouched. Affected edge parents are reconciled atomically and appended
    /// after existing destination siblings, without normalizing unrelated edges.
    /// This convenience method is for unbound scenes only.
    pub fn preview_reparent(
        &self,
        ids: &[CanvasObjectId],
        target: Option<CanvasObjectId>,
    ) -> Result<Self, String> {
        self.preview_reparent_with_validator(ids, target, |_| Ok(()))
    }

    pub fn preview_reparent_with_validator(
        &self,
        ids: &[CanvasObjectId],
        target: Option<CanvasObjectId>,
        validate: impl FnOnce(&SceneEditContext<'_, 'a>) -> Result<(), String>,
    ) -> Result<Self, String> {
        let selected = self.selection(ids)?;
        if let Some(target) = target {
            self.require_container(target)?;
            self.require_unlocked_path(target)?;
        }
        let roots = self.selection_roots(&selected);
        for id in &roots {
            self.require_unlocked_path(*id)?;
        }
        let order = self.preorder();
        let mut candidate = self.clone();
        let changed: HashSet<_> = roots
            .into_iter()
            .filter(|id| self.parents.get(id).copied() != target)
            .collect();
        for id in &changed {
            candidate.set_parent(*id, target);
        }
        candidate.append_reparented(&changed, &order);
        candidate.validate_topology()?;
        let mut affected = self.closure(&changed);
        let edge_parent_changes = candidate.reconcile_edge_parents(self, &affected, &order)?;
        affected.extend(edge_parent_changes.iter().map(|change| change.object));
        candidate.validate_topology()?;
        let affected: Vec<_> = order
            .into_iter()
            .filter(|id| affected.contains(id))
            .collect();
        validate(&SceneEditContext {
            kind: SceneEditKind::Reparent { target },
            before: self,
            after: &candidate,
            geometry: self.canvas,
            selection: ids,
            affected: &affected,
            edge_parent_changes: &edge_parent_changes,
        })?;
        Ok(candidate)
    }

    fn set_parent(&mut self, id: CanvasObjectId, parent: Option<CanvasObjectId>) {
        if let Some(parent) = parent {
            self.parents.insert(id, parent);
        } else {
            self.parents.remove(&id);
        }
    }

    fn append_reparented(&mut self, changed: &HashSet<CanvasObjectId>, order: &[CanvasObjectId]) {
        for siblings in self.children.values_mut() {
            siblings.retain(|id| !changed.contains(id));
        }
        for id in order.iter().filter(|id| changed.contains(id)) {
            self.children
                .entry(self.parents.get(id).copied())
                .or_default()
                .push(*id);
        }
    }

    fn reconcile_edge_parents(
        &mut self,
        before: &Self,
        carried: &HashSet<CanvasObjectId>,
        order: &[CanvasObjectId],
    ) -> Result<Vec<SceneParentChange>, String> {
        if carried.is_empty() {
            return Ok(Vec::new());
        }
        let mut affected = carried.clone();
        let mut changed = HashSet::new();
        for index in self.connector_dependency_order()? {
            let object = &self.canvas.objects[index];
            let CanvasObjectKind::Connector(connector) = &object.kind else {
                continue;
            };
            if ![connector.from, connector.to]
                .into_iter()
                .any(|end| matches!(end, Endpoint::Object(id) if affected.contains(&id)))
            {
                continue;
            }
            let parent = match (connector.from, connector.to) {
                (Endpoint::Object(from), Endpoint::Object(to)) => {
                    self.nearest_common_container(from, to)?
                }
                // Cards and free points have no scene occurrence in this spike.
                _ => None,
            };
            if parent == self.parents.get(&object.id).copied() {
                continue;
            }
            let id = object.id;
            if !carried.contains(&id) {
                before.require_unlocked_path(id)?;
                if let Some(parent) = parent {
                    self.require_unlocked_path(parent)?;
                }
            }
            self.set_parent(id, parent);
            affected.insert(id);
            changed.insert(id);
        }
        self.append_reparented(&changed, order);
        Ok(order
            .iter()
            .filter(|id| changed.contains(id) && before.parents.get(id) != self.parents.get(id))
            .map(|id| SceneParentChange {
                object: *id,
                from: before.parents.get(id).copied(),
                to: self.parents.get(id).copied(),
            })
            .collect())
    }

    fn connector_dependency_order(&self) -> Result<Vec<usize>, String> {
        let mut remaining = vec![0u8; self.canvas.objects.len()];
        let mut dependents: HashMap<_, Vec<_>> = HashMap::new();
        let mut ready = VecDeque::new();
        let mut count = 0;
        for (index, object) in self.canvas.objects.iter().enumerate() {
            let CanvasObjectKind::Connector(connector) = &object.kind else {
                continue;
            };
            count += 1;
            for end in [connector.from, connector.to] {
                if let Endpoint::Object(id) = end {
                    let target = self.objects.get(&id).ok_or_else(|| {
                        format!(
                            "connector {} references missing scene object {id}",
                            object.id
                        )
                    })?;
                    if self.canvas.objects[*target].is_connector() {
                        remaining[index] += 1;
                        dependents.entry(*target).or_default().push(index);
                    }
                }
            }
            if remaining[index] == 0 {
                ready.push_back(index);
            }
        }
        let mut ordered = Vec::with_capacity(count);
        while let Some(index) = ready.pop_front() {
            ordered.push(index);
            if let Some(dependents) = dependents.get(&index) {
                for dependent in dependents {
                    remaining[*dependent] -= 1;
                    if remaining[*dependent] == 0 {
                        ready.push_back(*dependent);
                    }
                }
            }
        }
        if ordered.len() != count {
            return Err("connector attachment cycle in scene hierarchy".into());
        }
        Ok(ordered)
    }

    /// Previews one sheet-space translation per object. Locked descendants move
    /// with their selected ancestor; directly moving a locked child is refused.
    /// Internal connector controls follow both moved endpoints, while controls
    /// of unselected crossing connectors remain fixed.
    ///
    /// Unrelated legacy object-only groups still expand as units. A selection
    /// requiring a card occurrence is explicitly refused by this object-only
    /// prototype rather than splitting the mixed group. This convenience method
    /// is for unbound scenes only.
    pub fn preview_translation(
        &self,
        ids: &[CanvasObjectId],
        delta: Vec2,
    ) -> Result<CanvasLayer, String> {
        self.preview_translation_with_validator(ids, delta, |_| Ok(()))
    }

    pub fn preview_translation_with_validator(
        &self,
        ids: &[CanvasObjectId],
        delta: Vec2,
        validate: impl FnOnce(&SceneEditContext<'_, 'a>) -> Result<(), String>,
    ) -> Result<CanvasLayer, String> {
        if !valid_position(delta) {
            return Err("movement out of range".into());
        }
        let selected = self.selection(ids)?;
        let groups: HashSet<_> = selected
            .iter()
            .filter_map(|id| self.canvas.objects[self.objects[id]].group)
            .collect();
        if self
            .canvas
            .card_states
            .iter()
            .any(|state| state.group.is_some_and(|group| groups.contains(&group)))
        {
            return Err("mixed card groups require a scene occurrence adapter".into());
        }
        let selected = self
            .canvas
            .objects
            .iter()
            .filter(|object| {
                selected.contains(&object.id)
                    || object.group.is_some_and(|group| groups.contains(&group))
            })
            .map(|object| object.id)
            .collect();
        for id in self.selection_roots(&selected) {
            self.require_unlocked_path(id)?;
        }
        let mut moved = self.closure(&selected);
        self.include_internal_connectors(&mut moved);
        let mut candidate = self.canvas.clone();
        Sheet::translate_canvas_objects(&mut candidate, &moved, delta)?;
        let affected: Vec<_> = self
            .preorder()
            .into_iter()
            .filter(|id| moved.contains(id))
            .collect();
        validate(&SceneEditContext {
            kind: SceneEditKind::Translate { delta },
            before: self,
            after: self,
            geometry: &candidate,
            selection: ids,
            affected: &affected,
            edge_parent_changes: &[],
        })?;
        Ok(candidate)
    }

    fn include_internal_connectors(&self, moved: &mut HashSet<CanvasObjectId>) {
        // Moving controls alone does not rigidly translate a connector with
        // stationary terminals, so downstream attachments need a separate set.
        let mut rigid: HashSet<_> = moved
            .iter()
            .copied()
            .filter(|id| !self.canvas.objects[self.objects[id]].is_connector())
            .collect();
        let mut waiting: HashMap<_, Vec<_>> = HashMap::new();
        let mut remaining = vec![0u8; self.canvas.objects.len()];
        let mut pending = Vec::new();
        for (index, object) in self.canvas.objects.iter().enumerate() {
            let CanvasObjectKind::Connector(connector) = &object.kind else {
                continue;
            };
            let endpoints = [connector.from, connector.to];
            if endpoints.iter().any(|endpoint| match endpoint {
                Endpoint::Topic(_) => true,
                Endpoint::Point(_) => !moved.contains(&object.id),
                Endpoint::Object(_) => false,
            }) {
                continue;
            }
            for endpoint in endpoints {
                if let Endpoint::Object(endpoint) = endpoint
                    && !rigid.contains(&endpoint)
                {
                    waiting.entry(endpoint).or_default().push(index);
                    remaining[index] += 1;
                }
            }
            if remaining[index] == 0 {
                pending.push(object.id);
            }
        }
        // Connector terminals may themselves target connectors. Propagate
        // attachment movement by dependency, not source order or fixed-point scans.
        while let Some(id) = pending.pop() {
            if !rigid.insert(id) {
                continue;
            }
            moved.insert(id);
            if let Some(dependents) = waiting.get(&id) {
                for index in dependents {
                    remaining[*index] -= 1;
                    if remaining[*index] == 0 {
                        pending.push(self.canvas.objects[*index].id);
                    }
                }
            }
        }
    }

    fn require_object(&self, id: CanvasObjectId) -> Result<(), String> {
        if self.objects.contains_key(&id) {
            Ok(())
        } else {
            Err(format!("scene object {id} not found"))
        }
    }

    fn require_container(&self, id: CanvasObjectId) -> Result<(), String> {
        if self.containers.contains(&id) {
            Ok(())
        } else {
            Err(format!("scene parent {id} is not a container"))
        }
    }

    fn selection(&self, ids: &[CanvasObjectId]) -> Result<HashSet<CanvasObjectId>, String> {
        if ids.is_empty() {
            return Err("nothing selected".into());
        }
        if ids.len() > MAX_CANVAS_OBJECTS {
            return Err("scene selection exceeds canvas limits".into());
        }
        for id in ids {
            self.require_object(*id)?;
        }
        Ok(ids.iter().copied().collect())
    }

    fn selection_roots(&self, selected: &HashSet<CanvasObjectId>) -> Vec<CanvasObjectId> {
        self.preorder()
            .into_iter()
            .filter(|id| {
                if !selected.contains(id) {
                    return false;
                }
                let mut cursor = self.parents.get(id);
                while let Some(parent) = cursor {
                    if selected.contains(parent) {
                        return false;
                    }
                    cursor = self.parents.get(parent);
                }
                true
            })
            .collect()
    }

    fn validate_topology(&self) -> Result<(), String> {
        for object in &self.canvas.objects {
            if object.group.is_some()
                && (self.containers.contains(&object.id) || self.parents.contains_key(&object.id))
            {
                return Err("scene hierarchy members in legacy flat groups are not supported by this prototype".into());
            }
            let mut cursor = object.id;
            let mut visited = HashSet::new();
            let mut depth = 0;
            while let Some(parent) = self.parents.get(&cursor) {
                if !visited.insert(cursor) {
                    return Err(format!("scene parent cycle at {cursor}"));
                }
                depth += 1;
                if depth > MAX_SCENE_DEPTH {
                    return Err(format!(
                        "scene hierarchy exceeds {MAX_SCENE_DEPTH} parent links"
                    ));
                }
                cursor = *parent;
            }
        }
        Ok(())
    }

    fn closure(&self, selected: &HashSet<CanvasObjectId>) -> HashSet<CanvasObjectId> {
        let mut closure = selected.clone();
        let mut pending: Vec<_> = selected.iter().copied().collect();
        while let Some(id) = pending.pop() {
            if let Some(children) = self.children.get(&Some(id)) {
                for child in children {
                    if closure.insert(*child) {
                        pending.push(*child);
                    }
                }
            }
        }
        closure
    }

    fn require_unlocked_path(&self, id: CanvasObjectId) -> Result<(), String> {
        let mut cursor = Some(id);
        while let Some(id) = cursor {
            if self.locked.contains(&id) {
                return Err(format!("scene object {id} is locked"));
            }
            cursor = self.parents.get(&id).copied();
        }
        Ok(())
    }
}
