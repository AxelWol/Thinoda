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
use std::collections::{HashMap, HashSet};

/// Maximum number of parent links from any object to its layer.
pub const MAX_SCENE_DEPTH: usize = 32;

/// One structural parent tree, with geometry still authoritative on the canvas.
///
/// Unlisted objects are layer roots. Sibling order comes from the source canvas.
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
        for object in &canvas.objects {
            if object.group.is_some()
                && (container_set.contains(&object.id) || parent_map.contains_key(&object.id))
            {
                return Err("scene hierarchy members in legacy flat groups are not supported by this prototype".into());
            }
            let mut cursor = object.id;
            let mut visited = HashSet::new();
            let mut depth = 0;
            while let Some(parent) = parent_map.get(&cursor) {
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
        let mut children: HashMap<_, Vec<_>> = HashMap::new();
        for object in &canvas.objects {
            children
                .entry(parent_map.get(&object.id).copied())
                .or_default()
                .push(object.id);
        }
        Ok(Self {
            canvas,
            objects,
            locked,
            containers: container_set,
            parents: parent_map,
            children,
        })
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
    /// untouched. Edge-parent reconciliation is a later live-command gate.
    pub fn preview_reparent(
        &self,
        ids: &[CanvasObjectId],
        target: Option<CanvasObjectId>,
    ) -> Result<Self, String> {
        let selected = self.selection(ids)?;
        if let Some(target) = target {
            self.require_container(target)?;
            self.require_unlocked_path(target)?;
        }
        let roots = self.selection_roots(&selected);
        let mut parents = self.parents.clone();
        for id in roots {
            self.require_unlocked_path(id)?;
            if let Some(target) = target {
                parents.insert(id, target);
            } else {
                parents.remove(&id);
            }
        }
        Self::new(
            self.canvas,
            &self.containers.iter().copied().collect::<Vec<_>>(),
            &parents.into_iter().collect::<Vec<_>>(),
        )
    }

    /// Previews one sheet-space translation per object. Locked descendants move
    /// with their selected ancestor; directly moving a locked child is refused.
    /// Internal connector controls follow both moved endpoints, while controls
    /// of unselected crossing connectors remain fixed.
    ///
    /// Unrelated legacy object-only groups still expand as units. A selection
    /// requiring a card occurrence is explicitly refused by this object-only
    /// prototype rather than splitting the mixed group.
    pub fn preview_translation(
        &self,
        ids: &[CanvasObjectId],
        delta: Vec2,
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
        self.canvas
            .objects
            .iter()
            .filter(|object| {
                if !selected.contains(&object.id) {
                    return false;
                }
                let mut cursor = self.parents.get(&object.id);
                while let Some(parent) = cursor {
                    if selected.contains(parent) {
                        return false;
                    }
                    cursor = self.parents.get(parent);
                }
                true
            })
            .map(|object| object.id)
            .collect()
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
