//! Named board layers above the implicit base layer.
//!
//! Only canvas objects live on layers; topics, cards and zones always stay on
//! the base layer. `CanvasLayer::objects` is kept stably ordered by layer rank
//! (base first), so paint order, hit testing and z-order inside a band keep
//! working on the plain object list. Scene children always share the layer of
//! their scene parent.

use crate::board::{CanvasLayer, CanvasObjectId};
use crate::document::Document;
use crate::edit::{EditError, EditResult};
use crate::model::{Sheet, id_type};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;

id_type!(BoardLayerId);

/// Maximum number of named layers per sheet.
pub const MAX_BOARD_LAYERS: usize = 64;
/// Maximum characters of a layer name.
pub const MAX_BOARD_LAYER_NAME_CHARS: usize = 128;

/// A named layer; objects reference it through `CanvasObject::layer`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardLayer {
    pub id: BoardLayerId,
    pub name: String,
    #[serde(default = "visible_default", skip_serializing_if = "is_visible")]
    pub visible: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub locked: bool,
    /// draw.io cell id of an imported layer, reused on export.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
}

fn visible_default() -> bool {
    true
}

fn is_visible(visible: &bool) -> bool {
    *visible
}

impl BoardLayer {
    pub fn new(id: BoardLayerId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            visible: true,
            locked: false,
            source_id: None,
        }
    }
}

/// Paint rank of `layer` in `ranks` (base layer and unknown layers are 0).
pub(crate) fn rank_of(ranks: &HashMap<BoardLayerId, usize>, layer: Option<BoardLayerId>) -> usize {
    layer.and_then(|id| ranks.get(&id).copied()).unwrap_or(0)
}

fn validate_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("layer name must not be empty".into());
    }
    if name.chars().count() > MAX_BOARD_LAYER_NAME_CHARS {
        return Err(format!(
            "layer name exceeds {MAX_BOARD_LAYER_NAME_CHARS} characters"
        ));
    }
    if name.chars().any(char::is_control) {
        return Err("layer name contains control characters".into());
    }
    Ok(())
}

impl CanvasLayer {
    /// Whether this canvas needs layer-aware persistence.
    pub fn has_layers(&self) -> bool {
        !self.layers.is_empty() || self.objects.iter().any(|object| object.layer.is_some())
    }

    pub fn board_layer(&self, id: BoardLayerId) -> Option<&crate::board_layers::BoardLayer> {
        self.layers.iter().find(|layer| layer.id == id)
    }

    fn board_layer_mut(&mut self, id: BoardLayerId) -> Result<&mut BoardLayer, String> {
        self.layers
            .iter_mut()
            .find(|layer| layer.id == id)
            .ok_or_else(|| format!("board layer {id} not found"))
    }

    /// Paint rank of every named layer; the base layer is rank 0.
    pub fn layer_ranks(&self) -> HashMap<BoardLayerId, usize> {
        self.layers
            .iter()
            .enumerate()
            .map(|(index, layer)| (layer.id, index + 1))
            .collect()
    }

    /// The base layer is always visible.
    pub fn is_layer_visible(&self, layer: Option<BoardLayerId>) -> bool {
        layer
            .and_then(|id| self.board_layer(id))
            .is_none_or(|layer| layer.visible)
    }

    /// The base layer is never locked.
    pub fn is_layer_locked(&self, layer: Option<BoardLayerId>) -> bool {
        layer
            .and_then(|id| self.board_layer(id))
            .is_some_and(|layer| layer.locked)
    }

    /// Whether the object is on a hidden layer.
    pub fn is_on_hidden_layer(&self, id: CanvasObjectId) -> bool {
        self.object(id)
            .is_some_and(|object| !self.is_layer_visible(object.layer))
    }

    /// Scene descendants take the layer of their top-level ancestor, and objects
    /// are stably sorted by layer rank. Order within a layer is unchanged.
    pub fn normalize_layer_order(&mut self) {
        if self.layers.is_empty() {
            return;
        }
        if !self.scene.parents.is_empty() {
            let layers: HashMap<_, _> = self
                .objects
                .iter()
                .map(|object| (object.id, object.layer))
                .collect();
            let root_layer = |mut id: CanvasObjectId| {
                for _ in 0..=crate::scene_hierarchy::MAX_SCENE_DEPTH {
                    match self.scene.parents.get(&id) {
                        Some(parent) => id = *parent,
                        None => break,
                    }
                }
                layers.get(&id).copied().flatten()
            };
            let updates: Vec<_> = self
                .objects
                .iter()
                .filter(|object| self.scene.parents.contains_key(&object.id))
                .map(|object| root_layer(object.id))
                .collect();
            let mut updates = updates.into_iter();
            for object in &mut self.objects {
                if self.scene.parents.contains_key(&object.id) {
                    object.layer = updates.next().flatten();
                }
            }
        }
        let ranks = self.layer_ranks();
        self.objects
            .sort_by_key(|object| rank_of(&ranks, object.layer));
    }
}

/// Validates layer definitions, references, scene and group consistency and
/// the rank order of `canvas.objects`.
pub(crate) fn validate_layers(canvas: &CanvasLayer) -> Result<(), String> {
    if canvas.layers.len() > MAX_BOARD_LAYERS {
        return Err(format!("canvas exceeds {MAX_BOARD_LAYERS} layers"));
    }
    let mut ids = HashSet::new();
    let mut sources = HashSet::new();
    for layer in &canvas.layers {
        if !ids.insert(layer.id) {
            return Err(format!("duplicate board layer id {}", layer.id));
        }
        validate_name(&layer.name)?;
        if let Some(source) = &layer.source_id {
            crate::drawio_passthrough::valid_id(source, "draw.io layer id")?;
            if !sources.insert(source.as_str()) {
                return Err(format!("duplicate draw.io layer id {source:?}"));
            }
        }
    }
    if canvas.objects.iter().all(|object| object.layer.is_none()) {
        return Ok(());
    }
    let ranks = canvas.layer_ranks();
    let mut previous = 0;
    let mut layers = HashMap::with_capacity(canvas.objects.len());
    let mut group_layers = HashMap::new();
    for object in &canvas.objects {
        if let Some(layer) = object.layer
            && !ranks.contains_key(&layer)
        {
            return Err(format!(
                "canvas object {} references missing layer {layer}",
                object.id
            ));
        }
        let rank = rank_of(&ranks, object.layer);
        if rank < previous {
            return Err("canvas objects are not ordered by layer".into());
        }
        previous = rank;
        layers.insert(object.id, object.layer);
        if let Some(group) = object.group
            && *group_layers.entry(group).or_insert(object.layer) != object.layer
        {
            return Err(format!("canvas group {group} spans several layers"));
        }
    }
    for (child, parent) in &canvas.scene.parents {
        if layers.get(child) != layers.get(parent) {
            return Err(format!(
                "scene child {child} is not on the layer of its parent {parent}"
            ));
        }
    }
    Ok(())
}

impl Sheet {
    /// Adds a named layer on top of all layers.
    pub fn add_board_layer(&mut self, name: &str) -> Result<BoardLayerId, String> {
        if self.canvas.layers.len() >= MAX_BOARD_LAYERS {
            return Err(format!("canvas exceeds {MAX_BOARD_LAYERS} layers"));
        }
        validate_name(name)?;
        let id = BoardLayerId(self.alloc());
        self.canvas.layers.push(BoardLayer::new(id, name));
        Ok(id)
    }

    pub fn rename_board_layer(&mut self, id: BoardLayerId, name: &str) -> Result<(), String> {
        validate_name(name)?;
        self.canvas.board_layer_mut(id)?.name = name.to_owned();
        Ok(())
    }

    pub fn set_board_layer_visible(
        &mut self,
        id: BoardLayerId,
        visible: bool,
    ) -> Result<(), String> {
        self.canvas.board_layer_mut(id)?.visible = visible;
        Ok(())
    }

    pub fn set_board_layer_locked(&mut self, id: BoardLayerId, locked: bool) -> Result<(), String> {
        self.canvas.board_layer_mut(id)?.locked = locked;
        Ok(())
    }

    /// Moves a named layer to `index` among the named layers (0 = just above
    /// the base layer); objects follow their layer.
    pub fn reorder_board_layer(&mut self, id: BoardLayerId, index: usize) -> Result<(), String> {
        let from = self
            .canvas
            .layers
            .iter()
            .position(|layer| layer.id == id)
            .ok_or_else(|| format!("board layer {id} not found"))?;
        if index >= self.canvas.layers.len() {
            return Err("layer position is out of range".into());
        }
        let layer = self.canvas.layers.remove(from);
        self.canvas.layers.insert(index, layer);
        self.canvas.normalize_layer_order();
        Ok(())
    }

    /// Deletes a named layer. Its objects move on top of the layer below (the
    /// base layer for the lowest named layer), so nothing is lost.
    pub fn remove_board_layer(&mut self, id: BoardLayerId) -> Result<(), String> {
        let index = self
            .canvas
            .layers
            .iter()
            .position(|layer| layer.id == id)
            .ok_or_else(|| format!("board layer {id} not found"))?;
        if self.canvas.layers[index].locked {
            return Err("unlock the layer before deleting it".into());
        }
        let below = index.checked_sub(1).map(|i| self.canvas.layers[i].id);
        for object in &mut self.canvas.objects {
            if object.layer == Some(id) {
                object.layer = below;
            }
        }
        self.canvas.layers.remove(index);
        if self.canvas.layers.is_empty() {
            return Ok(());
        }
        self.canvas.normalize_layer_order();
        Ok(())
    }

    /// Objects that move together with `ids` to another layer: flat group
    /// members and scene descendants. A scene child cannot leave its parent's
    /// layer on its own.
    pub fn board_layer_move_closure(
        &self,
        ids: &[CanvasObjectId],
    ) -> Result<Vec<CanvasObjectId>, String> {
        if ids.is_empty() {
            return Err("nothing selected".into());
        }
        let mut moved: HashSet<_> = self.canvas.expand_to_groups(ids).into_iter().collect();
        for id in ids {
            if self.canvas.object(*id).is_none() {
                return Err(format!("canvas object {id} not found"));
            }
        }
        if !self.canvas.scene.is_empty() {
            for id in moved.clone() {
                if let Some(parent) = self.canvas.scene.parents.get(&id)
                    && !moved.contains(parent)
                {
                    return Err("objects inside a container stay on the container's layer".into());
                }
            }
            let hierarchy = crate::scene_hierarchy::SceneHierarchy::from_canvas(&self.canvas)?;
            let roots: Vec<_> = moved.iter().copied().collect();
            moved.extend(hierarchy.descendant_closure(&roots)?);
        }
        Ok(self
            .canvas
            .objects
            .iter()
            .filter(|object| moved.contains(&object.id))
            .map(|object| object.id)
            .collect())
    }

    /// Moves objects with their group members and scene descendants on top of
    /// `layer` (`None` = base layer).
    pub fn move_canvas_objects_to_layer(
        &mut self,
        ids: &[CanvasObjectId],
        layer: Option<BoardLayerId>,
    ) -> Result<(), String> {
        if let Some(layer) = layer {
            let target = self
                .canvas
                .board_layer(layer)
                .ok_or_else(|| format!("board layer {layer} not found"))?;
            if target.locked {
                return Err("the target layer is locked".into());
            }
        }
        let moved: HashSet<_> = self.board_layer_move_closure(ids)?.into_iter().collect();
        let roots: Vec<_> = moved.iter().copied().collect();
        self.require_unlocked(&roots)?;
        let (mut picked, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.canvas.objects)
            .into_iter()
            .partition(|object| moved.contains(&object.id) && object.layer != layer);
        for object in &mut picked {
            object.layer = layer;
        }
        self.canvas.objects = rest.into_iter().chain(picked).collect();
        self.canvas.normalize_layer_order();
        Ok(())
    }
}

impl Document {
    pub fn add_board_layer(&mut self, name: &str) -> EditResult<BoardLayerId> {
        self.edit_sheet("Add layer", |sheet| {
            sheet.add_board_layer(name).map_err(EditError::Invalid)
        })
    }

    pub fn rename_board_layer(&mut self, id: BoardLayerId, name: &str) -> EditResult<()> {
        self.edit_sheet("Rename layer", |sheet| {
            sheet
                .rename_board_layer(id, name)
                .map_err(EditError::Invalid)
        })
    }

    pub fn set_board_layer_visible(&mut self, id: BoardLayerId, visible: bool) -> EditResult<()> {
        self.edit_sheet(if visible { "Show layer" } else { "Hide layer" }, |sheet| {
            sheet
                .set_board_layer_visible(id, visible)
                .map_err(EditError::Invalid)
        })
    }

    pub fn set_board_layer_locked(&mut self, id: BoardLayerId, locked: bool) -> EditResult<()> {
        self.edit_sheet(
            if locked { "Lock layer" } else { "Unlock layer" },
            |sheet| {
                sheet
                    .set_board_layer_locked(id, locked)
                    .map_err(EditError::Invalid)
            },
        )
    }

    pub fn reorder_board_layer(&mut self, id: BoardLayerId, index: usize) -> EditResult<()> {
        self.edit_sheet("Reorder layers", |sheet| {
            sheet
                .reorder_board_layer(id, index)
                .map_err(EditError::Invalid)
        })
    }

    pub fn remove_board_layer(&mut self, id: BoardLayerId) -> EditResult<()> {
        self.edit_sheet("Delete layer", |sheet| {
            sheet.remove_board_layer(id).map_err(EditError::Invalid)
        })
    }

    pub fn move_canvas_objects_to_layer(
        &mut self,
        ids: &[CanvasObjectId],
        layer: Option<BoardLayerId>,
    ) -> EditResult<()> {
        self.edit_sheet("Move to layer", |sheet| {
            sheet
                .move_canvas_objects_to_layer(ids, layer)
                .map_err(EditError::Invalid)
        })
    }
}

#[cfg(test)]
mod tests;
