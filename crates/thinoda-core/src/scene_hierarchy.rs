//! Shared, object-only container prototype for draw.io and socio-technical scenes.
//!
//! The persisted structure has one parent relation and canvas-ordered siblings.
//! Immutable previews validate complete candidates before sheet commands commit
//! them. Header/layout policies and semantic bindings remain separate.

use crate::board::{
    Band, CanvasLayer, CanvasObject, CanvasObjectId, CanvasObjectKind, Endpoint, MAX_CANVAS_GROUPS,
    MAX_CANVAS_OBJECTS, MAX_CARD_STATES, ZOrder, validate_object,
};
use crate::geometry::{Rect, Vec2};
use crate::model::{Sheet, valid_position};
use crate::scene_layout::{
    ContainerLayout, ContainerLayoutPolicy, StackResizePolicy, preview_container_layout,
};
use crate::scene_table::SceneTable;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

/// Maximum number of parent links from any object to its layer.
pub const MAX_SCENE_DEPTH: usize = 32;

/// Read-only containment navigation in stored sibling order, without wrapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneNavigation {
    Parent,
    FirstChild,
    PreviousSibling,
    NextSibling,
}

/// Authoritative object-only ancestry. Sibling order is stored by the canvas
/// object vector; geometry remains in sheet coordinates.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SceneStructure {
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub containers: BTreeSet<CanvasObjectId>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub parents: BTreeMap<CanvasObjectId, CanvasObjectId>,
    /// Expanded frames of collapsed containers; their objects hold compact frames.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub collapsed: BTreeMap<CanvasObjectId, Rect>,
    /// Persistent header and free/stack layout overrides for containers.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub layout_policies: BTreeMap<CanvasObjectId, ContainerLayoutPolicy>,
    /// Native table grids, keyed by their table container.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub tables: BTreeMap<CanvasObjectId, SceneTable>,
}

impl Sheet {
    pub(crate) fn commit_scene_canvas(&mut self, candidate: CanvasLayer) -> Result<(), String> {
        let previous = std::mem::replace(&mut self.canvas, candidate);
        let semantic_error = self
            .semantic
            .as_ref()
            .and_then(|semantic| semantic.validate(self).err());
        if let Some(error) = semantic_error {
            self.canvas = previous;
            return Err(error);
        }
        Ok(())
    }

    /// Collapses to an explicit compact frame, or restores the saved expanded frame.
    /// Collapse changes presentation only; descendants and real endpoints stay put.
    pub fn set_canvas_container_collapsed(
        &mut self,
        id: CanvasObjectId,
        compact: Option<Rect>,
    ) -> Result<(), String> {
        let hierarchy = SceneHierarchy::from_canvas(&self.canvas)?;
        hierarchy.require_container(id)?;
        hierarchy.require_unlocked_selection(&[id])?;
        if compact.is_some() && self.has_sankey() {
            return Err("collapse does not support Sankey sheets".into());
        }
        let mut candidate = self.canvas.clone();
        let object = candidate
            .object_mut(id)
            .ok_or("scene container not found")?;
        if let Some(rect) = compact {
            if self.canvas.scene.collapsed.contains_key(&id) {
                if rect == object.rect {
                    return Ok(());
                }
                return Err("expand the container before changing its compact frame".into());
            }
            let expanded = object.rect;
            object.rect = rect;
            validate_object(object)?;
            candidate.scene.collapsed.insert(id, expanded);
        } else if let Some(expanded) = self.canvas.scene.collapsed.get(&id) {
            object.rect = *expanded;
            candidate.scene.collapsed.remove(&id);
        }
        SceneHierarchy::from_canvas(&candidate)?;
        self.commit_scene_canvas(candidate)
    }

    /// Marks an existing unrotated shape as a free container. Clearing a nonempty
    /// container requires explicitly reparenting or deleting its contents first.
    pub fn set_canvas_container(
        &mut self,
        id: CanvasObjectId,
        enabled: bool,
    ) -> Result<(), String> {
        SceneHierarchy::from_canvas(&self.canvas)?.require_unlocked_selection(&[id])?;
        if !enabled && self.canvas.scene.collapsed.contains_key(&id) {
            return Err("expand the container before clearing its container state".into());
        }
        if !enabled
            && self
                .canvas
                .scene
                .parents
                .values()
                .any(|parent| *parent == id)
        {
            return Err("reparent or delete the container contents first".into());
        }
        let mut candidate = self.canvas.clone();
        if enabled {
            candidate.scene.containers.insert(id);
        } else {
            candidate.scene.containers.remove(&id);
            candidate.scene.layout_policies.remove(&id);
        }
        let candidate = SceneHierarchy::from_canvas(&candidate)?.stored_canvas();
        self.commit_scene_canvas(candidate)
    }

    /// Sets or clears a persistent header/layout policy on a free container.
    pub fn set_canvas_container_layout_policy(
        &mut self,
        id: CanvasObjectId,
        policy: Option<ContainerLayoutPolicy>,
    ) -> Result<(), String> {
        let hierarchy = SceneHierarchy::from_canvas(&self.canvas)?;
        hierarchy.require_container(id)?;
        hierarchy.require_unlocked_selection(&[id])?;
        let mut candidate = self.canvas.clone();
        if let Some(policy) = policy {
            candidate.scene.layout_policies.insert(id, policy);
        } else {
            candidate.scene.layout_policies.remove(&id);
        }
        if let Some(table) = candidate.scene.tables.get(&id) {
            let size = crate::scene_table::table_frame_size(table, policy.map(|p| p.header));
            let object = candidate.object_mut(id).ok_or("scene table not found")?;
            object.rect.size = size;
            validate_object(object)?;
            candidate = SceneHierarchy::from_canvas(&candidate)?
                .preview_scene_layout(&BTreeMap::new(), |_| Ok(()))?;
        }
        SceneHierarchy::from_canvas(&candidate)?;
        self.commit_scene_canvas(candidate)
    }

    pub fn reparent_canvas_objects(
        &mut self,
        ids: &[CanvasObjectId],
        target: Option<CanvasObjectId>,
    ) -> Result<(), String> {
        self.reparent_canvas_objects_with_validator(ids, target, |_| Ok(()))
    }

    pub fn reparent_canvas_objects_with_validator(
        &mut self,
        ids: &[CanvasObjectId],
        target: Option<CanvasObjectId>,
        validate: impl FnOnce(&SceneEditContext<'_, '_>) -> Result<(), String>,
    ) -> Result<(), String> {
        let hierarchy = SceneHierarchy::from_canvas(&self.canvas)?;
        let candidate = hierarchy
            .preview_reparent_with_validator(ids, target, validate)?
            .stored_canvas();
        self.commit_scene_canvas(candidate)
    }

    pub fn reorder_canvas_objects_with_validator(
        &mut self,
        ids: &[CanvasObjectId],
        order: ZOrder,
        validate: impl FnOnce(&SceneEditContext<'_, '_>) -> Result<(), String>,
    ) -> Result<(), String> {
        let candidate = SceneHierarchy::from_canvas(&self.canvas)?
            .preview_reorder_with_validator(ids, order, validate)?
            .stored_canvas();
        self.commit_scene_canvas(candidate)
    }

    /// Applies stored policies for a container subtree as one sheet edit.
    pub fn layout_canvas_container(&mut self, id: CanvasObjectId) -> Result<(), String> {
        let candidate = SceneHierarchy::from_canvas(&self.canvas)?
            .preview_stored_layout_with_validator(id, |_| Ok(()))?;
        self.commit_scene_canvas(candidate)
    }
}

impl SceneStructure {
    pub fn is_empty(&self) -> bool {
        self.containers.is_empty()
            && self.parents.is_empty()
            && self.collapsed.is_empty()
            && self.layout_policies.is_empty()
            && self.tables.is_empty()
    }

    pub fn contains(&self, id: CanvasObjectId) -> bool {
        self.containers.contains(&id)
            || self.parents.contains_key(&id)
            || self.layout_policies.contains_key(&id)
            || self.tables.contains_key(&id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SceneEditKind {
    Reparent { target: Option<CanvasObjectId> },
    Translate { delta: Vec2 },
    Reorder { order: ZOrder },
    Reconnect { object: CanvasObjectId },
    Layout,
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
    pub fn from_canvas(canvas: &'a CanvasLayer) -> Result<Self, String> {
        if canvas.scene.containers.len() > MAX_CANVAS_OBJECTS
            || canvas.scene.parents.len() > MAX_CANVAS_OBJECTS
            || canvas.scene.collapsed.len() > MAX_CANVAS_OBJECTS
        {
            return Err("scene hierarchy exceeds canvas limits".into());
        }
        Self::new(
            canvas,
            &canvas.scene.containers.iter().copied().collect::<Vec<_>>(),
            &canvas
                .scene
                .parents
                .iter()
                .map(|(child, parent)| (*child, *parent))
                .collect::<Vec<_>>(),
        )
    }

    pub(crate) fn stored_canvas(&self) -> CanvasLayer {
        let mut canvas = self.canvas.clone();
        canvas.scene = SceneStructure {
            containers: self.containers.iter().copied().collect(),
            parents: self
                .parents
                .iter()
                .map(|(child, parent)| (*child, *parent))
                .collect(),
            collapsed: self.canvas.scene.collapsed.clone(),
            layout_policies: self.canvas.scene.layout_policies.clone(),
            tables: self.canvas.scene.tables.clone(),
        };
        let ranks: HashMap<_, _> = self
            .preorder()
            .into_iter()
            .enumerate()
            .map(|(rank, id)| (id, rank))
            .collect();
        canvas.objects.sort_by_key(|object| ranks[&object.id]);
        canvas.normalize_layer_order();
        canvas
    }

    pub(crate) fn require_unlocked_selection(&self, ids: &[CanvasObjectId]) -> Result<(), String> {
        let selected = self.selection(ids)?;
        for id in self.selection_roots(&selected) {
            self.require_unlocked_path(id)?;
        }
        Ok(())
    }

    pub(crate) fn copy_closure(
        &self,
        ids: &[CanvasObjectId],
    ) -> Result<Vec<CanvasObjectId>, String> {
        let mut included = self.closure(&self.selection(ids)?);
        self.include_internal_connectors(&mut included);
        Ok(self
            .preorder()
            .into_iter()
            .filter(|id| included.contains(id))
            .collect())
    }

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
                || canvas.is_layer_locked(object.layer)
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
        for (id, expanded) in &canvas.scene.collapsed {
            if !container_set.contains(id) {
                return Err(format!("collapsed object {id} is not a scene container"));
            }
            let object = &canvas.objects[objects[id]];
            if object.rect.size.x <= 0.0 || object.rect.size.y <= 0.0 {
                return Err(format!(
                    "collapsed container {id} requires a positive compact frame"
                ));
            }
            let mut restored = object.clone();
            restored.rect = *expanded;
            validate_object(&restored)?;
        }
        if canvas.scene.layout_policies.len() > MAX_CANVAS_OBJECTS {
            return Err("scene layout policies exceed canvas limits".into());
        }
        for (id, policy) in &canvas.scene.layout_policies {
            if !container_set.contains(id) {
                return Err(format!(
                    "layout policy object {id} is not a scene container"
                ));
            }
            let object = &canvas.objects[objects[id]];
            let frame = canvas
                .scene
                .collapsed
                .get(id)
                .copied()
                .unwrap_or(object.rect);
            preview_container_layout(frame, *policy, &[])
                .map_err(|error| format!("invalid layout policy for container {id}: {error}"))?;
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
        crate::scene_table::validate_structure(canvas, &container_set, &parent_map, &children)?;
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

    /// Indexed inherited-lock lookup over the validated, bounded parent chain.
    pub fn is_locked(&self, id: CanvasObjectId) -> bool {
        self.require_unlocked_path(id).is_err()
    }

    /// Locks do not hide navigation targets. The layer itself is not an object.
    pub fn navigate(
        &self,
        id: CanvasObjectId,
        direction: SceneNavigation,
    ) -> Result<Option<CanvasObjectId>, String> {
        self.require_object(id)?;
        let parent = self.parents.get(&id).copied();
        match direction {
            SceneNavigation::Parent => Ok(parent),
            SceneNavigation::FirstChild => {
                if self.canvas.scene.collapsed.contains_key(&id) {
                    return Ok(None);
                }
                Ok(self
                    .children
                    .get(&Some(id))
                    .and_then(|children| children.first().copied()))
            }
            SceneNavigation::PreviousSibling | SceneNavigation::NextSibling => {
                let siblings = self
                    .children
                    .get(&parent)
                    .ok_or("scene siblings not found")?;
                let position = siblings
                    .iter()
                    .position(|sibling| *sibling == id)
                    .ok_or("scene object missing from siblings")?;
                let target = match direction {
                    SceneNavigation::PreviousSibling => position.checked_sub(1),
                    _ => position.checked_add(1),
                };
                Ok(target.and_then(|position| siblings.get(position).copied()))
            }
        }
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
        let edge_parent_changes =
            candidate.reconcile_edge_parents(self, &affected, None, &order)?;
        affected.extend(edge_parent_changes.iter().map(|change| change.object));
        candidate.validate_topology()?;
        let affected: Vec<_> = order
            .into_iter()
            .filter(|id| affected.contains(id))
            .collect();
        let geometry = candidate.stored_canvas();
        validate(&SceneEditContext {
            kind: SceneEditKind::Reparent { target },
            before: self,
            after: &candidate,
            geometry: &geometry,
            selection: ids,
            affected: &affected,
            edge_parent_changes: &edge_parent_changes,
        })?;
        Ok(candidate)
    }

    /// Reorders selection roots only among siblings in the same paint band.
    /// Descendants remain a unit and keep their internal order and geometry.
    pub fn preview_reorder_with_validator(
        &self,
        ids: &[CanvasObjectId],
        order: ZOrder,
        validate: impl FnOnce(&SceneEditContext<'_, 'a>) -> Result<(), String>,
    ) -> Result<Self, String> {
        let selected = self.selection(ids)?;
        let roots: HashSet<_> = self.selection_roots(&selected).into_iter().collect();
        for id in &roots {
            self.require_unlocked_path(*id)?;
        }
        let mut candidate = self.clone();
        let ranks = self.canvas.layer_ranks();
        let keys: Vec<_> = (0..=self.canvas.layers.len())
            .flat_map(|rank| [(Band::Below, rank), (Band::Above, rank)])
            .collect();
        for siblings in candidate.children.values_mut() {
            if !siblings.iter().any(|id| roots.contains(id)) {
                continue;
            }
            for &(band, rank) in &keys {
                let slots: Vec<_> = siblings
                    .iter()
                    .enumerate()
                    .filter(|(_, id)| {
                        let object = &self.canvas.objects[self.objects[id]];
                        object.band == band
                            && crate::board_layers::rank_of(&ranks, object.layer) == rank
                    })
                    .map(|(index, _)| index)
                    .collect();
                match order {
                    ZOrder::Front | ZOrder::Back => {
                        let mut ordered: Vec<_> =
                            slots.iter().map(|slot| siblings[*slot]).collect();
                        ordered.sort_by_key(|id| roots.contains(id) != (order == ZOrder::Back));
                        for (slot, id) in slots.into_iter().zip(ordered) {
                            siblings[slot] = id;
                        }
                    }
                    ZOrder::Forward => {
                        for index in (0..slots.len().saturating_sub(1)).rev() {
                            if roots.contains(&siblings[slots[index]])
                                && !roots.contains(&siblings[slots[index + 1]])
                            {
                                siblings.swap(slots[index], slots[index + 1]);
                            }
                        }
                    }
                    ZOrder::Backward => {
                        for index in 1..slots.len() {
                            if roots.contains(&siblings[slots[index]])
                                && !roots.contains(&siblings[slots[index - 1]])
                            {
                                siblings.swap(slots[index], slots[index - 1]);
                            }
                        }
                    }
                }
            }
        }
        let affected_set = self.closure(&roots);
        let affected: Vec<_> = self
            .preorder()
            .into_iter()
            .filter(|id| affected_set.contains(id))
            .collect();
        let geometry = candidate.stored_canvas();
        validate(&SceneEditContext {
            kind: SceneEditKind::Reorder { order },
            before: self,
            after: &candidate,
            geometry: &geometry,
            selection: ids,
            affected: &affected,
            edge_parent_changes: &[],
        })?;
        Ok(candidate)
    }

    /// Reconciles a checked connector replacement, including dependent edges.
    /// Topic endpoints and sheet-wide resource budgets are checked by the sheet.
    pub fn preview_reconnect_with_validator(
        &self,
        object: &CanvasObject,
        validate: impl FnOnce(&SceneEditContext<'_, '_>) -> Result<(), String>,
    ) -> Result<CanvasLayer, String> {
        self.require_object(object.id)?;
        self.require_unlocked_path(object.id)?;
        let original = &self.canvas.objects[self.objects[&object.id]];
        let (CanvasObjectKind::Connector(before), CanvasObjectKind::Connector(after)) =
            (&original.kind, &object.kind)
        else {
            return Err("only connectors can be reconnected".into());
        };
        if object.group != original.group || object.locked != original.locked {
            return Err("reconnection cannot change group or lock state".into());
        }
        if self.parents.contains_key(&object.id) && object.band != original.band {
            return Err("changing paint bands of scene members is not supported yet".into());
        }
        let order = self.preorder();
        let mut canvas = self.stored_canvas();
        *canvas
            .object_mut(object.id)
            .ok_or("scene connector not found")? = object.clone();
        let mut candidate = SceneHierarchy::from_canvas(&canvas)?;
        let reconnected = (before.from != after.from || before.to != after.to).then_some(object.id);
        let changes =
            candidate.reconcile_edge_parents(self, &HashSet::new(), reconnected, &order)?;
        candidate.validate_topology()?;
        let mut affected_set: HashSet<_> = changes.iter().map(|change| change.object).collect();
        affected_set.insert(object.id);
        let affected: Vec<_> = order
            .into_iter()
            .filter(|id| affected_set.contains(id))
            .collect();
        let geometry = candidate.stored_canvas();
        validate(&SceneEditContext {
            kind: SceneEditKind::Reconnect { object: object.id },
            before: self,
            after: &candidate,
            geometry: &geometry,
            selection: &[object.id],
            affected: &affected,
            edge_parent_changes: &changes,
        })?;
        Ok(geometry)
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
        reconnected: Option<CanvasObjectId>,
        order: &[CanvasObjectId],
    ) -> Result<Vec<SceneParentChange>, String> {
        if carried.is_empty() && reconnected.is_none() {
            return Ok(Vec::new());
        }
        let mut affected = carried.clone();
        let mut changed = HashSet::new();
        for index in self.connector_dependency_order()? {
            let object = &self.canvas.objects[index];
            let CanvasObjectKind::Connector(connector) = &object.kind else {
                continue;
            };
            if reconnected != Some(object.id)
                && ![connector.from, connector.to]
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
        self.preview_translation_with_cards(ids, &HashSet::new(), delta, validate)
    }

    /// Previews one container's layout as a single sheet-space transaction.
    /// Any configured descendant policies run top-down in the same candidate.
    /// Policies are input-only and are not persisted.
    pub fn preview_layout_with_validator(
        &self,
        container: CanvasObjectId,
        policy: ContainerLayoutPolicy,
        validate: impl FnOnce(&SceneEditContext<'_, 'a>) -> Result<(), String>,
    ) -> Result<CanvasLayer, String> {
        self.preview_layout_tree_with_validator(&BTreeMap::from([(container, policy)]), validate)
    }

    /// Previews stored policies on the container and its descendants.
    pub fn preview_stored_layout_with_validator(
        &self,
        container: CanvasObjectId,
        validate: impl FnOnce(&SceneEditContext<'_, 'a>) -> Result<(), String>,
    ) -> Result<CanvasLayer, String> {
        self.require_container(container)?;
        let closure = self.closure(&HashSet::from([container]));
        let policies: BTreeMap<_, _> = self
            .canvas
            .scene
            .layout_policies
            .iter()
            .filter(|(id, _)| closure.contains(id))
            .map(|(id, policy)| (*id, *policy))
            .collect();
        if policies.is_empty() {
            return Err("container subtree has no stored layout policies".into());
        }
        self.preview_layout_tree_with_validator(&policies, validate)
    }

    /// Previews nested free/stack policies with bottom-up parent growth and a
    /// deterministic top-down placement pass. Each object's net translation
    /// and explicit resize are applied once to the candidate geometry.
    /// Policies are input-only and are not persisted.
    pub fn preview_layout_tree_with_validator(
        &self,
        policies: &BTreeMap<CanvasObjectId, ContainerLayoutPolicy>,
        validate: impl FnOnce(&SceneEditContext<'_, 'a>) -> Result<(), String>,
    ) -> Result<CanvasLayer, String> {
        if policies.is_empty() {
            return Err("no scene layout policies supplied".into());
        }
        self.preview_scene_layout(policies, validate)
    }

    /// Layout pass over `policies` and every table grid, which always refits
    /// its cells to the table frame.
    pub(crate) fn preview_scene_layout(
        &self,
        policies: &BTreeMap<CanvasObjectId, ContainerLayoutPolicy>,
        validate: impl FnOnce(&SceneEditContext<'_, 'a>) -> Result<(), String>,
    ) -> Result<CanvasLayer, String> {
        if policies.len() > MAX_CANVAS_OBJECTS {
            return Err("scene layout policy count exceeds canvas limits".into());
        }
        let order = self.preorder();
        for id in policies.keys() {
            self.require_container(*id)?;
            self.require_unlocked_path(*id)?;
            let mut ancestor = Some(*id);
            while let Some(id) = ancestor {
                if self.canvas.scene.collapsed.contains_key(&id) {
                    return Err("expand collapsed ancestors before applying a layout".into());
                }
                ancestor = self.parents.get(&id).copied();
            }
        }

        let mut rects: HashMap<_, _> = self
            .canvas
            .objects
            .iter()
            .map(|object| (object.id, object.rect))
            .collect();
        let mut deltas = HashMap::new();
        let mut resized = HashMap::new();
        let mut scheduled = Vec::with_capacity(policies.len());
        let mut fitted_tables = BTreeMap::new();

        for &container in order.iter().rev() {
            let Some(policy) = policies.get(&container).copied() else {
                continue;
            };
            if !matches!(
                policy.layout,
                ContainerLayout::Stack {
                    resize: StackResizePolicy::GrowParent | StackResizePolicy::FitParent,
                    ..
                }
            ) {
                continue;
            }
            let parent_rect = rects
                .get(&container)
                .copied()
                .ok_or("scene layout container not found")?;
            let children: Vec<_> = self
                .children(container)?
                .iter()
                .filter_map(|id| {
                    let object = &self.canvas.objects[self.objects[id]];
                    matches!(object.kind, CanvasObjectKind::Shape { .. }).then_some(*id)
                })
                .collect();
            let before_rects: Vec<_> = children
                .iter()
                .map(|id| rects.get(id).copied().ok_or("scene layout child not found"))
                .collect::<Result<_, _>>()?;
            let preview = preview_container_layout(parent_rect, policy, &before_rects)
                .map_err(|error| error.to_string())?;
            if preview.parent.size != parent_rect.size {
                self.require_unlocked_path(container)?;
                rects.insert(container, preview.parent);
                resized.insert(container, preview.parent.size);
            }
        }

        for container in order {
            if let Some(table) = self.canvas.scene.tables.get(&container) {
                scheduled.push(container);
                let frame = rects
                    .get(&container)
                    .copied()
                    .ok_or("scene table not found")?;
                let header = crate::scene_table::table_header(self.canvas, container);
                let (fitted, placements) = crate::scene_table::place_table(table, header, frame)?;
                if let Some(fitted) = fitted {
                    fitted_tables.insert(container, fitted);
                }
                let placements: Vec<_> = placements
                    .into_iter()
                    .filter(|(child, rect)| {
                        rects
                            .get(child)
                            .is_none_or(|before| !crate::scene_table::same_rect(*before, *rect))
                    })
                    .collect();
                self.place_layout_children(placements, &mut rects, &mut deltas, &mut resized)?;
                continue;
            }
            let Some(policy) = policies.get(&container).copied() else {
                continue;
            };
            scheduled.push(container);

            let parent_rect = rects
                .get(&container)
                .copied()
                .ok_or("scene layout container not found")?;
            let children: Vec<_> = self
                .children(container)?
                .iter()
                .filter_map(|id| {
                    let object = &self.canvas.objects[self.objects[id]];
                    matches!(object.kind, CanvasObjectKind::Shape { .. }).then_some(*id)
                })
                .collect();
            let before_rects: Vec<_> = children
                .iter()
                .map(|id| rects.get(id).copied().ok_or("scene layout child not found"))
                .collect::<Result<_, _>>()?;
            let preview = preview_container_layout(parent_rect, policy, &before_rects)
                .map_err(|error| error.to_string())?;
            if preview.parent.size != parent_rect.size {
                return Err("nested parent growth did not converge".into());
            }
            self.place_layout_children(
                children.into_iter().zip(preview.children).collect(),
                &mut rects,
                &mut deltas,
                &mut resized,
            )?;
        }

        self.add_rigid_connector_deltas(&mut deltas);
        let mut candidate = self.stored_canvas();
        Sheet::translate_canvas_objects_by_delta(&mut candidate, &deltas)?;
        for (&id, &size) in &resized {
            candidate.objects[self.objects[&id]].rect.size = size;
        }
        candidate.scene.tables.extend(fitted_tables);
        let affected: Vec<_> = self
            .preorder()
            .into_iter()
            .filter(|id| deltas.contains_key(id) || resized.contains_key(id))
            .collect();
        validate(&SceneEditContext {
            kind: SceneEditKind::Layout,
            before: self,
            after: self,
            geometry: &candidate,
            selection: &scheduled,
            affected: &affected,
            edge_parent_changes: &[],
        })?;
        Ok(candidate)
    }

    /// Moves and resizes laid-out children; descendants follow each move.
    fn place_layout_children(
        &self,
        placements: Vec<(CanvasObjectId, Rect)>,
        rects: &mut HashMap<CanvasObjectId, Rect>,
        deltas: &mut HashMap<CanvasObjectId, Vec2>,
        resized: &mut HashMap<CanvasObjectId, Vec2>,
    ) -> Result<(), String> {
        {
            for (child, rect) in placements {
                let before = rects[&child];
                let delta = rect.min - before.min;
                let size_changed = rect.size != before.size;
                if delta == Vec2::ZERO && !size_changed {
                    continue;
                }
                if size_changed && self.canvas.scene.collapsed.contains_key(&child) {
                    return Err(
                        "expand collapsed children before resizing them through layout".into(),
                    );
                }
                self.require_unlocked_path(child)?;
                if delta != Vec2::ZERO {
                    let closure = self.closure(&HashSet::from([child]));
                    for id in closure {
                        let accumulated = deltas.get(&id).copied().unwrap_or(Vec2::ZERO) + delta;
                        if !valid_position(accumulated) {
                            return Err("layout movement out of range".into());
                        }
                        deltas.insert(id, accumulated);
                        if let Some(object_rect) = rects.get_mut(&id) {
                            let source = &self.canvas.objects[self.objects[&id]];
                            let size = object_rect.size;
                            *object_rect = source.rect.translate(accumulated);
                            object_rect.size = size;
                        }
                    }
                }
                if size_changed {
                    rects.insert(child, rect);
                    resized.insert(child, rect.size);
                }
            }
        }
        Ok(())
    }

    fn add_rigid_connector_deltas(&self, deltas: &mut HashMap<CanvasObjectId, Vec2>) {
        if !deltas.values().any(|delta| *delta != Vec2::ZERO) {
            return;
        }
        let mut rigid: HashSet<_> = deltas
            .iter()
            .filter(|(_, delta)| **delta != Vec2::ZERO)
            .filter_map(|(id, _)| {
                (!self.canvas.objects[self.objects[id]].is_connector()).then_some(*id)
            })
            .collect();
        let mut remaining = vec![0_u8; self.canvas.objects.len()];
        let mut dependents: HashMap<CanvasObjectId, Vec<usize>> = HashMap::new();
        let mut pending = VecDeque::new();
        let mut blocked = HashSet::new();

        for (index, object) in self.canvas.objects.iter().enumerate() {
            let CanvasObjectKind::Connector(connector) = &object.kind else {
                continue;
            };
            let structurally_moved = deltas.contains_key(&object.id);
            for endpoint in [connector.from, connector.to] {
                match endpoint {
                    Endpoint::Topic(_) => {
                        blocked.insert(index);
                    }
                    Endpoint::Point(_) if !structurally_moved => {
                        blocked.insert(index);
                    }
                    Endpoint::Object(id) if !rigid.contains(&id) => {
                        remaining[index] += 1;
                        dependents.entry(id).or_default().push(index);
                    }
                    Endpoint::Object(_) | Endpoint::Point(_) => {}
                }
            }
            if remaining[index] == 0 && !blocked.contains(&index) {
                pending.push_back(index);
            }
        }

        while let Some(index) = pending.pop_front() {
            let object = &self.canvas.objects[index];
            let CanvasObjectKind::Connector(connector) = &object.kind else {
                continue;
            };
            let endpoints = [connector.from, connector.to];
            let endpoint_deltas: Vec<_> = endpoints
                .into_iter()
                .filter_map(|endpoint| match endpoint {
                    Endpoint::Object(id) if rigid.contains(&id) => deltas.get(&id).copied(),
                    Endpoint::Object(_) | Endpoint::Topic(_) | Endpoint::Point(_) => None,
                })
                .collect();
            let delta = if let Some(delta) = endpoint_deltas.first().copied() {
                delta
            } else if deltas.contains_key(&object.id) {
                deltas[&object.id]
            } else {
                continue;
            };
            if delta == Vec2::ZERO || endpoint_deltas.iter().any(|other| *other != delta) {
                continue;
            }
            if let Some(structural_delta) = deltas.get(&object.id) {
                if *structural_delta != delta {
                    continue;
                }
            } else {
                deltas.insert(object.id, delta);
            }
            if !rigid.insert(object.id) {
                continue;
            }
            if let Some(dependents) = dependents.get(&object.id) {
                for dependent in dependents {
                    remaining[*dependent] -= 1;
                    if remaining[*dependent] == 0 && !blocked.contains(dependent) {
                        pending.push_back(*dependent);
                    }
                }
            }
        }
    }

    pub(crate) fn preview_translation_with_cards(
        &self,
        ids: &[CanvasObjectId],
        cards: &HashSet<crate::model::TopicId>,
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
        if self.canvas.card_states.iter().any(|state| {
            state.group.is_some_and(|group| groups.contains(&group))
                && !cards.contains(&state.topic)
        }) {
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
        let mut candidate = self.stored_canvas();
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
