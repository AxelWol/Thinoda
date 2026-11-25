//! Native 2-D tables on the shared scene container engine (SC4, draw.io D8).
//!
//! A table is a scene container whose direct shape children are its cells.
//! Each cell is a free container that owns its content. The table stores row
//! heights, column widths, the slot and span of every cell, and stripe colours.
//! Cell frames are derived from this grid by the shared layout pass, which also
//! moves cell content with its cell. Content edits can grow rows without
//! automatically shrinking their stored heights.

use crate::board::{
    CanvasLayer, CanvasObject, CanvasObjectId, MAX_BOARD_COORDINATE, MAX_CANVAS_OBJECTS, ShapeKind,
};
use crate::board_layers::BoardLayerId;
use crate::edit::{EditError, EditResult};
use crate::geometry::{Color, Rect, Vec2};
use crate::model::Sheet;
use crate::scene_hierarchy::SceneHierarchy;
use crate::scene_layout::{ContainerLayout, HeaderPolicy, StackResizePolicy};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet};

/// Maximum number of rows, and separately of columns, in one table.
pub const MAX_TABLE_TRACKS: usize = 256;
/// Maximum number of grid slots (rows × columns) in one table.
pub const MAX_TABLE_SLOTS: usize = 16_384;
/// Smallest row height or column width.
pub const MIN_TABLE_TRACK: f32 = 4.0;
pub const DEFAULT_TABLE_ROW: f32 = 40.0;
pub const DEFAULT_TABLE_COLUMN: f32 = 120.0;
/// draw.io's light stripe colour.
pub const DEFAULT_TABLE_STRIPE: Color = Color::rgb(0xF5, 0xF5, 0xF5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TableAxis {
    Row,
    Column,
}

/// One cell object and the grid slots it covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneTableCell {
    pub id: CanvasObjectId,
    pub row: u16,
    pub column: u16,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub row_span: u16,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub column_span: u16,
}

fn one() -> u16 {
    1
}

fn is_one(value: &u16) -> bool {
    *value == 1
}

impl SceneTableCell {
    pub fn new(id: CanvasObjectId, row: usize, column: usize) -> Self {
        Self {
            id,
            row: row as u16,
            column: column as u16,
            row_span: 1,
            column_span: 1,
        }
    }

    pub fn start(&self, axis: TableAxis) -> usize {
        usize::from(match axis {
            TableAxis::Row => self.row,
            TableAxis::Column => self.column,
        })
    }

    pub fn span(&self, axis: TableAxis) -> usize {
        usize::from(match axis {
            TableAxis::Row => self.row_span,
            TableAxis::Column => self.column_span,
        })
    }

    fn set(&mut self, axis: TableAxis, start: usize, span: usize) {
        match axis {
            TableAxis::Row => {
                self.row = start as u16;
                self.row_span = span as u16;
            }
            TableAxis::Column => {
                self.column = start as u16;
                self.column_span = span as u16;
            }
        }
    }

    fn covers(&self, row: usize, column: usize) -> bool {
        (self.start(TableAxis::Row)..self.start(TableAxis::Row) + self.span(TableAxis::Row))
            .contains(&row)
            && (self.start(TableAxis::Column)
                ..self.start(TableAxis::Column) + self.span(TableAxis::Column))
                .contains(&column)
    }
}

/// Alternating fills painted on cells without their own fill. Indices count
/// from the first body row or column; draw.io calls the second one "even".
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct TableStripes {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub even_rows: Option<Color>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub odd_rows: Option<Color>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub even_columns: Option<Color>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub odd_columns: Option<Color>,
}

impl TableStripes {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Stripe colour for a cell starting at `row`/`column`; rows paint over
    /// columns as in draw.io.
    pub fn color(&self, row: usize, column: usize) -> Option<Color> {
        let pick = |index: usize, even: Option<Color>, odd: Option<Color>| {
            if index % 2 == 1 { even } else { odd }
        };
        pick(row, self.even_rows, self.odd_rows)
            .or_else(|| pick(column, self.even_columns, self.odd_columns))
    }
}

/// Grid of one table container, in sheet units, below any container header.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneTable {
    pub rows: Vec<f32>,
    pub columns: Vec<f32>,
    /// Cells ordered by row, then column; every slot is covered exactly once.
    pub cells: Vec<SceneTableCell>,
    #[serde(default, skip_serializing_if = "TableStripes::is_empty")]
    pub stripes: TableStripes,
}

impl SceneTable {
    pub fn tracks(&self, axis: TableAxis) -> &[f32] {
        match axis {
            TableAxis::Row => &self.rows,
            TableAxis::Column => &self.columns,
        }
    }

    fn tracks_mut(&mut self, axis: TableAxis) -> &mut Vec<f32> {
        match axis {
            TableAxis::Row => &mut self.rows,
            TableAxis::Column => &mut self.columns,
        }
    }

    pub fn cell(&self, id: CanvasObjectId) -> Option<&SceneTableCell> {
        self.cells.iter().find(|cell| cell.id == id)
    }

    /// The cell covering a slot, if the slot is in range.
    pub fn cell_at(&self, row: usize, column: usize) -> Option<&SceneTableCell> {
        self.cells.iter().find(|cell| cell.covers(row, column))
    }

    /// Unscaled grid size: column widths by row heights.
    pub fn size(&self) -> Vec2 {
        Vec2::new(self.columns.iter().sum(), self.rows.iter().sum())
    }

    /// Frame of `cell` when the grid fills `body` exactly.
    pub fn cell_rect(&self, body: Rect, cell: &SceneTableCell) -> Rect {
        let extent = |tracks: &[f32], start: usize, span: usize| {
            let offset: f32 = tracks[..start].iter().sum();
            let size: f32 = tracks[start..start + span].iter().sum();
            (offset, size)
        };
        let (x, width) = extent(
            &self.columns,
            cell.start(TableAxis::Column),
            cell.span(TableAxis::Column),
        );
        let (y, height) = extent(
            &self.rows,
            cell.start(TableAxis::Row),
            cell.span(TableAxis::Row),
        );
        Rect::from_min_size(
            Vec2::new(body.min.x + x, body.min.y + y),
            Vec2::new(width, height),
        )
    }

    /// Bounds, finite sizes, canonical cell order, in-range rectangular spans
    /// and exact single coverage of every slot.
    pub fn validate(&self) -> Result<(), String> {
        let (rows, columns) = (self.rows.len(), self.columns.len());
        if rows == 0 || columns == 0 {
            return Err("a table needs at least one row and one column".into());
        }
        if rows > MAX_TABLE_TRACKS || columns > MAX_TABLE_TRACKS || rows * columns > MAX_TABLE_SLOTS
        {
            return Err(format!(
                "a table supports up to {MAX_TABLE_TRACKS} rows and columns and {MAX_TABLE_SLOTS} cells"
            ));
        }
        for size in self.rows.iter().chain(&self.columns) {
            if !size.is_finite() || *size < MIN_TABLE_TRACK || *size > MAX_BOARD_COORDINATE {
                return Err("table row and column sizes are out of range".into());
            }
        }
        let size = self.size();
        if size.x > MAX_BOARD_COORDINATE || size.y > MAX_BOARD_COORDINATE {
            return Err("table exceeds board limits".into());
        }
        if self.cells.len() > rows * columns {
            return Err("table has more cells than slots".into());
        }
        let mut covered = vec![false; rows * columns];
        let mut previous = None;
        for cell in &self.cells {
            let key = (cell.row, cell.column);
            if previous.is_some_and(|previous| previous >= key) {
                return Err("table cells must be ordered by row and column".into());
            }
            previous = Some(key);
            let (row, column) = (usize::from(cell.row), usize::from(cell.column));
            let (row_span, column_span) =
                (usize::from(cell.row_span), usize::from(cell.column_span));
            if row_span == 0
                || column_span == 0
                || row + row_span > rows
                || column + column_span > columns
            {
                return Err(format!("table cell {} spans outside its table", cell.id));
            }
            for r in row..row + row_span {
                for c in column..column + column_span {
                    let slot = &mut covered[r * columns + c];
                    if *slot {
                        return Err(format!("table cell {} overlaps another cell", cell.id));
                    }
                    *slot = true;
                }
            }
        }
        if covered.contains(&false) {
            return Err("every table slot must belong to one cell".into());
        }
        Ok(())
    }

    /// Grid fitted to a `body` size, scaling every track proportionally.
    fn fitted(&self, body: Vec2) -> Result<Self, String> {
        let size = self.size();
        if approx(size.x, body.x) && approx(size.y, body.y) {
            return Ok(self.clone());
        }
        let mut table = self.clone();
        for (axis, from, to) in [
            (TableAxis::Row, size.y, body.y),
            (TableAxis::Column, size.x, body.x),
        ] {
            if approx(from, to) {
                continue;
            }
            let scale = to / from;
            for track in table.tracks_mut(axis) {
                *track *= scale;
            }
        }
        table
            .validate()
            .map_err(|_| "the table is too small for its rows and columns".to_string())?;
        Ok(table)
    }
}

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() <= 0.01 + 1e-5 * a.abs().max(b.abs())
}

fn approx_rect(a: Rect, b: Rect) -> bool {
    approx(a.min.x, b.min.x)
        && approx(a.min.y, b.min.y)
        && approx(a.size.x, b.size.x)
        && approx(a.size.y, b.size.y)
}

/// Header of a table container, from its stored free layout policy.
pub fn table_header(canvas: &CanvasLayer, table: CanvasObjectId) -> Option<HeaderPolicy> {
    canvas
        .scene
        .layout_policies
        .get(&table)
        .map(|policy| policy.header)
}

/// The body rectangle of a table frame below (or beside) its header.
pub fn table_body(frame: Rect, header: Option<HeaderPolicy>) -> Result<Rect, String> {
    match header {
        Some(header) => crate::scene_layout::body_rect(frame, header)
            .map_err(|error| format!("invalid table header: {error}")),
        None => Ok(frame),
    }
}

/// Frame size of a table whose grid exactly fills its body.
pub fn table_frame_size(table: &SceneTable, header: Option<HeaderPolicy>) -> Vec2 {
    let mut size = table.size();
    if let Some(header) = header {
        match header.side {
            crate::scene_layout::HeaderSide::Top | crate::scene_layout::HeaderSide::Bottom => {
                size.y += header.extent;
            }
            crate::scene_layout::HeaderSide::Left | crate::scene_layout::HeaderSide::Right => {
                size.x += header.extent;
            }
        }
    }
    size
}

/// The table owning a cell object.
pub fn table_of_cell(
    canvas: &CanvasLayer,
    cell: CanvasObjectId,
) -> Option<(CanvasObjectId, &SceneTable)> {
    let parent = *canvas.scene.parents.get(&cell)?;
    let table = canvas.scene.tables.get(&parent)?;
    table.cell(cell).map(|_| (parent, table))
}

/// The table selected directly, or through one of its cells.
pub fn selected_table(
    canvas: &CanvasLayer,
    id: CanvasObjectId,
) -> Option<(CanvasObjectId, &SceneTable)> {
    canvas
        .scene
        .tables
        .get(&id)
        .map(|table| (id, table))
        .or_else(|| table_of_cell(canvas, id))
}

/// Paint fill of a cell without a visible fill of its own in a striped table.
pub fn stripe_fill(canvas: &CanvasLayer, object: &CanvasObject) -> Option<Color> {
    if object.style.fill.is_some_and(|fill| fill.a > 0) || canvas.scene.tables.is_empty() {
        return None;
    }
    let parent = canvas.scene.parents.get(&object.id)?;
    let table = canvas.scene.tables.get(parent)?;
    if table.stripes.is_empty() {
        return None;
    }
    let cell = table.cell(object.id)?;
    table
        .stripes
        .color(cell.start(TableAxis::Row), cell.start(TableAxis::Column))
}

/// `object` with its table stripe as fill, borrowed when nothing changes.
pub fn with_table_stripes<'a>(
    canvas: &CanvasLayer,
    object: Cow<'a, CanvasObject>,
) -> Cow<'a, CanvasObject> {
    match stripe_fill(canvas, &object) {
        Some(fill) => {
            let mut object = object.into_owned();
            object.style.fill = Some(fill);
            Cow::Owned(object)
        }
        None => object,
    }
}

/// Structural table invariants checked with the scene hierarchy.
pub(crate) fn validate_structure(
    canvas: &CanvasLayer,
    containers: &HashSet<CanvasObjectId>,
    parents: &HashMap<CanvasObjectId, CanvasObjectId>,
    children: &HashMap<Option<CanvasObjectId>, Vec<CanvasObjectId>>,
) -> Result<(), String> {
    let tables = &canvas.scene.tables;
    if tables.is_empty() {
        return Ok(());
    }
    if tables.len() > MAX_CANVAS_OBJECTS {
        return Err("scene tables exceed canvas limits".into());
    }
    let mut owners = HashMap::new();
    for (id, table) in tables {
        table
            .validate()
            .map_err(|error| format!("invalid table {id}: {error}"))?;
        if !containers.contains(id) {
            return Err(format!("table {id} is not a scene container"));
        }
        if canvas.scene.collapsed.contains_key(id) {
            return Err(format!("table {id} cannot be collapsed"));
        }
        if let Some(policy) = canvas.scene.layout_policies.get(id)
            && policy.layout != ContainerLayout::Free
        {
            return Err(format!("table {id} arranges its own cells"));
        }
        for cell in &table.cells {
            if owners.insert(cell.id, *id).is_some() {
                return Err(format!(
                    "table cell {} belongs to more than one table",
                    cell.id
                ));
            }
            if !containers.contains(&cell.id) {
                return Err(format!("table cell {} is not a scene container", cell.id));
            }
            if parents.get(&cell.id) != Some(id) {
                return Err(format!(
                    "table cell {} is not a child of table {id}",
                    cell.id
                ));
            }
            if canvas.scene.collapsed.contains_key(&cell.id) {
                return Err(format!("table cell {} cannot be collapsed", cell.id));
            }
            if tables.contains_key(&cell.id) {
                return Err(format!("table cell {} cannot itself be a table", cell.id));
            }
            if let Some(policy) = canvas.scene.layout_policies.get(&cell.id)
                && matches!(
                    policy.layout,
                    ContainerLayout::Stack {
                        resize: StackResizePolicy::GrowParent | StackResizePolicy::FitParent,
                        ..
                    }
                )
            {
                return Err(format!(
                    "table cell {} does not grow with its content",
                    cell.id
                ));
            }
        }
        for child in children.get(&Some(*id)).into_iter().flatten() {
            if owners.get(child) != Some(id)
                && !canvas
                    .object(*child)
                    .is_some_and(CanvasObject::is_connector)
            {
                return Err(format!(
                    "table {id} may only contain its cells and connectors"
                ));
            }
        }
    }
    Ok(())
}

/// Every table frame and cell frame matches its grid.
pub(crate) fn validate_geometry(canvas: &CanvasLayer) -> Result<(), String> {
    for (id, table) in &canvas.scene.tables {
        let frame = canvas
            .object(*id)
            .ok_or_else(|| format!("table {id} not found"))?
            .rect;
        let header = table_header(canvas, *id);
        let size = table_frame_size(table, header);
        if !approx(frame.size.x, size.x) || !approx(frame.size.y, size.y) {
            return Err(format!(
                "table {id} frame does not match its rows and columns"
            ));
        }
        let body = table_body(frame, header)?;
        for cell in &table.cells {
            let rect = canvas
                .object(cell.id)
                .ok_or_else(|| format!("table cell {} not found", cell.id))?
                .rect;
            if !approx_rect(rect, table.cell_rect(body, cell)) {
                return Err(format!(
                    "table cell {} does not match its row and column",
                    cell.id
                ));
            }
        }
    }
    Ok(())
}

/// A refitted grid (when it changed) and the cell frames it places.
pub(crate) type PlacedTable = (Option<SceneTable>, Vec<(CanvasObjectId, Rect)>);

/// Cell frames of a table placed in `frame`, refitting the grid when the
/// frame no longer matches it. Returns the refitted grid when it changed.
pub(crate) fn place_table(
    table: &SceneTable,
    header: Option<HeaderPolicy>,
    frame: Rect,
) -> Result<PlacedTable, String> {
    let body = table_body(frame, header)?;
    let fitted = table.fitted(body.size)?;
    let rects = fitted
        .cells
        .iter()
        .map(|cell| (cell.id, fitted.cell_rect(body, cell)))
        .collect();
    let changed = (fitted != *table).then_some(fitted);
    Ok((changed, rects))
}

pub(crate) fn same_rect(a: Rect, b: Rect) -> bool {
    approx_rect(a, b)
}

/// Reflows only tables affected by this edit. Opening an existing sheet does
/// not change its geometry. A spanning cell's excess is added to its last row.
pub(crate) fn reconcile_content(before: &Sheet, after: &mut Sheet) -> Result<(), String> {
    if after.canvas.scene.tables.is_empty() {
        return Ok(());
    }
    let old: HashMap<_, _> = before.canvas.objects.iter().map(|o| (o.id, o)).collect();
    let new: HashMap<_, _> = after.canvas.objects.iter().map(|o| (o.id, o)).collect();
    let cell_owner: HashMap<_, _> = after
        .canvas
        .scene
        .tables
        .iter()
        .flat_map(|(id, table)| table.cells.iter().map(|cell| (cell.id, *id)))
        .collect();
    let mut affected = HashSet::new();
    let collect_tables = |sheet: &Sheet, mut id: CanvasObjectId, out: &mut HashSet<_>| {
        for _ in 0..=crate::scene_hierarchy::MAX_SCENE_DEPTH {
            if let Some(table) = cell_owner.get(&id) {
                out.insert(*table);
            }
            let Some(parent) = sheet.canvas.scene.parents.get(&id) else {
                break;
            };
            id = *parent;
        }
    };
    for (id, table) in &after.canvas.scene.tables {
        if let Some(previous) = before.canvas.scene.tables.get(id)
            && (previous.rows != table.rows || previous.columns != table.columns)
        {
            affected.insert(*id);
            collect_tables(after, *id, &mut affected);
        }
    }
    for (&id, object) in &new {
        let parent = after.canvas.scene.parents.get(&id);
        let changed = match old.get(&id) {
            None => true,
            Some(previous) => {
                let old_parent = before.canvas.scene.parents.get(&id);
                let relative =
                    |object: &CanvasObject,
                     parent: Option<&CanvasObjectId>,
                     objects: &HashMap<CanvasObjectId, &CanvasObject>| {
                        let origin = parent
                            .and_then(|id| objects.get(id))
                            .map_or(Vec2::ZERO, |parent| parent.rect.min);
                        (object.rect.min - origin, object.rect.size)
                    };
                parent != old_parent
                    || relative(object, parent, &new) != relative(previous, old_parent, &old)
                    || object.kind != previous.kind
                    || object.rotation != previous.rotation
                    || object.label_style != previous.label_style
                    || object.label_rich_text != previous.label_rich_text
            }
        };
        if changed {
            collect_tables(after, id, &mut affected);
        }
    }
    // New tables retain their authored geometry until an existing cell changes.
    affected.retain(|id| before.canvas.scene.tables.contains_key(id));
    let depth = |mut id: CanvasObjectId| {
        let mut depth = 0;
        while let Some(parent) = after.canvas.scene.parents.get(&id) {
            depth += 1;
            id = *parent;
            if depth > crate::scene_hierarchy::MAX_SCENE_DEPTH {
                break;
            }
        }
        depth
    };
    let mut tables: Vec<_> = affected.into_iter().collect();
    tables.sort_by_key(|id| (std::cmp::Reverse(depth(*id)), *id));
    for id in tables {
        after.grow_table_content(id)?;
    }
    Ok(())
}

fn require_track_count(count: usize) -> Result<(), String> {
    if count == 0 || count > MAX_TABLE_TRACKS {
        return Err(format!(
            "a table needs 1 to {MAX_TABLE_TRACKS} rows and columns"
        ));
    }
    Ok(())
}

fn new_cell_object(template: Option<&CanvasObject>, layer: Option<BoardLayerId>) -> CanvasObject {
    let mut object = CanvasObject::shape(ShapeKind::Rectangle, Rect::default());
    match template {
        Some(template) => {
            object.style = template.style;
            object.band = template.band;
            object.rounding = template.rounding;
            object.rectangle_sides = template.rectangle_sides;
            object.layer = template.layer;
        }
        None => {
            object.style.stroke = Some(Color::rgb(0x88, 0x88, 0x88));
            object.style.stroke_width = 1.0;
            object.layer = layer;
        }
    }
    object
}

impl Sheet {
    fn grow_table_content(&mut self, id: CanvasObjectId) -> Result<(), String> {
        let mut candidate = self.clone();
        let mut table = candidate.table(id)?.clone();
        let cells: HashSet<_> = table.cells.iter().map(|cell| cell.id).collect();
        let hierarchy = SceneHierarchy::from_canvas(&candidate.canvas)?;
        let locked: HashSet<_> = candidate
            .canvas
            .objects
            .iter()
            .filter(|object| {
                cells.contains(&object.id)
                    && object.label_style.is_none()
                    && (!object.label().is_empty() || object.has_label_list())
                    && hierarchy.require_unlocked_path(object.id).is_err()
            })
            .map(|object| object.id)
            .collect();
        for object in &mut candidate.canvas.objects {
            if !cells.contains(&object.id) || locked.contains(&object.id) {
                continue;
            }
            if (!object.label().is_empty() || object.has_label_list())
                && object.label_style.is_none()
            {
                // Plain labels otherwise use the legacy unwrapped renderer.
                // Pin the currently effective font size to retain a non-default
                // style that explicitly opts the cell into shared wrapping.
                object.label_style = Some(std::sync::Arc::new(crate::label_style::LabelStyle {
                    font_size: Some(crate::board::CANVAS_LABEL_FONT_SIZE),
                    ..Default::default()
                }));
            }
        }
        let mut content_bottom: HashMap<CanvasObjectId, f32> = HashMap::new();
        for child in &candidate.canvas.objects {
            if child.is_connector() || cells.contains(&child.id) {
                continue;
            }
            let mut ancestor = child.id;
            for _ in 0..crate::scene_hierarchy::MAX_SCENE_DEPTH {
                let Some(parent) = candidate.canvas.scene.parents.get(&ancestor) else {
                    break;
                };
                if cells.contains(parent) {
                    let mut bottom = crate::board::rotated_bounds(child.rect, child.rotation)
                        .max()
                        .y;
                    if let Some(layout) = crate::board::object_label_rect(child)
                        .and_then(|base| crate::label_style::layout(child, base, child.label()))
                    {
                        let rect = layout.box_rect();
                        for point in [
                            rect.min,
                            Vec2::new(rect.max().x, rect.min.y),
                            rect.max(),
                            Vec2::new(rect.min.x, rect.max().y),
                        ] {
                            bottom = bottom.max(layout.to_sheet(point).y);
                        }
                    } else if !child.label().is_empty() {
                        let lines = child.label().split('\n').count() as f32;
                        let height = lines
                            * crate::board::CANVAS_LABEL_FONT_SIZE
                            * crate::label_style::LINE_HEIGHT;
                        bottom = bottom.max(child.rect.center().y + height / 2.0);
                    }
                    content_bottom
                        .entry(*parent)
                        .and_modify(|previous| *previous = previous.max(bottom))
                        .or_insert(bottom);
                    break;
                }
                ancestor = *parent;
            }
        }
        let objects: HashMap<_, _> = candidate.canvas.objects.iter().map(|o| (o.id, o)).collect();
        for cell in &table.cells {
            let object = objects
                .get(&cell.id)
                .copied()
                .ok_or_else(|| format!("table cell {} not found", cell.id))?;
            let width: f32 = table.columns[cell.start(TableAxis::Column)
                ..cell.start(TableAxis::Column) + cell.span(TableAxis::Column)]
                .iter()
                .sum();
            let height: f32 = table.rows[cell.start(TableAxis::Row)
                ..cell.start(TableAxis::Row) + cell.span(TableAxis::Row)]
                .iter()
                .sum();
            let mut required = 0.0_f32;
            if let Some((base, layout)) = crate::board::object_label_rect(object).and_then(|base| {
                crate::label_style::layout(object, base, object.label())
                    .map(|layout| (base, layout))
            }) {
                let style = object.label_style.as_deref().cloned().unwrap_or_default();
                let angle = layout.rotation.to_radians();
                let area_fraction = base.size.y / object.rect.size.y;
                let occupied = layout.block.size.y * angle.cos().abs()
                    + layout.block.size.x * angle.sin().abs()
                    + 2.0 * style.spacing
                    + style.spacing_top
                    + style.spacing_bottom;
                if area_fraction <= 0.0 || !area_fraction.is_finite() {
                    return Err("table cell has no usable text height".into());
                }
                required = occupied.max(0.0) / area_fraction;
            }
            if let Some(bottom) = content_bottom.get(&cell.id) {
                required = required.max(*bottom - object.rect.min.y);
            }
            if !width.is_finite() || !required.is_finite() {
                return Err("table content has invalid bounds".into());
            }
            if required > height {
                let last = cell.start(TableAxis::Row) + cell.span(TableAxis::Row) - 1;
                table.rows[last] += required - height;
            }
        }
        if table != candidate.canvas.scene.tables[&id] {
            self.commit_table(candidate, id, table)
        } else {
            *self = candidate;
            Ok(())
        }
    }

    fn table(&self, id: CanvasObjectId) -> Result<&SceneTable, String> {
        self.canvas
            .scene
            .tables
            .get(&id)
            .ok_or_else(|| format!("table {id} not found"))
    }

    fn require_editable_table(&self, id: CanvasObjectId) -> Result<SceneTable, String> {
        let table = self.table(id)?.clone();
        SceneHierarchy::from_canvas(&self.canvas)?.require_unlocked_selection(&[id])?;
        Ok(table)
    }

    /// Adds a cell object under `table`; the frame is set by the next relayout.
    fn push_table_cell(
        &mut self,
        table: CanvasObjectId,
        template: Option<&CanvasObject>,
    ) -> Result<CanvasObjectId, String> {
        if self.canvas.objects.len() >= MAX_CANVAS_OBJECTS {
            return Err(format!("canvas exceeds {MAX_CANVAS_OBJECTS} objects"));
        }
        let layer = self.canvas.object(table).and_then(|object| object.layer);
        let mut object = new_cell_object(template, layer);
        object.id = self.alloc_canvas_object_id();
        let id = object.id;
        self.canvas.objects.push(object);
        self.canvas.scene.containers.insert(id);
        self.canvas.scene.parents.insert(id, table);
        Ok(id)
    }

    /// Stores `table`, sizes its frame to the grid and lays its cells out.
    fn commit_table(
        &mut self,
        mut candidate: Sheet,
        id: CanvasObjectId,
        table: SceneTable,
    ) -> Result<(), String> {
        table.validate()?;
        let header = table_header(&candidate.canvas, id);
        let size = table_frame_size(&table, header);
        candidate.canvas.scene.tables.insert(id, table);
        let object = candidate
            .canvas
            .object_mut(id)
            .ok_or_else(|| format!("table {id} not found"))?;
        object.rect.size = size;
        crate::board::validate_object(object)?;
        candidate.canvas = SceneHierarchy::from_canvas(&candidate.canvas)?.stored_canvas();
        candidate.relayout_tables()?;
        if let Some(semantic) = &candidate.semantic {
            semantic.validate(&candidate)?;
        }
        *self = candidate;
        Ok(())
    }

    /// Re-derives every table's cell frames through the shared layout pass.
    pub(crate) fn relayout_tables(&mut self) -> Result<(), String> {
        if self.canvas.scene.tables.is_empty() {
            return Ok(());
        }
        let canvas = SceneHierarchy::from_canvas(&self.canvas)?
            .preview_scene_layout(&BTreeMap::new(), |_| Ok(()))?;
        validate_geometry(&canvas)?;
        self.canvas = canvas;
        Ok(())
    }

    /// Inserts an unstyled `rows` × `columns` table with its top-left corner at `origin`.
    pub fn insert_canvas_table(
        &mut self,
        origin: Vec2,
        rows: usize,
        columns: usize,
        layer: Option<BoardLayerId>,
    ) -> Result<CanvasObjectId, String> {
        require_track_count(rows)?;
        require_track_count(columns)?;
        if rows * columns > MAX_TABLE_SLOTS {
            return Err(format!("a table supports up to {MAX_TABLE_SLOTS} cells"));
        }
        let mut candidate = self.clone();
        let mut object = CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(
                origin,
                Vec2::new(
                    DEFAULT_TABLE_COLUMN * columns as f32,
                    DEFAULT_TABLE_ROW * rows as f32,
                ),
            ),
        );
        object.style.stroke = Some(Color::rgb(0x66, 0x66, 0x66));
        object.style.stroke_width = 1.0;
        object.layer = layer;
        let id = candidate.add_canvas_object(object)?;
        candidate.canvas.scene.containers.insert(id);
        let mut cells = Vec::with_capacity(rows * columns);
        for row in 0..rows {
            for column in 0..columns {
                let cell = candidate.push_table_cell(id, None)?;
                cells.push(SceneTableCell::new(cell, row, column));
            }
        }
        let table = SceneTable {
            rows: vec![DEFAULT_TABLE_ROW; rows],
            columns: vec![DEFAULT_TABLE_COLUMN; columns],
            cells,
            stripes: TableStripes::default(),
        };
        self.commit_table(candidate, id, table)?;
        Ok(id)
    }

    /// Inserts a row or column before `index` (`index == count` appends).
    /// Cells spanning the insertion boundary grow; other slots get new cells
    /// styled like their neighbour.
    pub fn insert_canvas_table_track(
        &mut self,
        id: CanvasObjectId,
        axis: TableAxis,
        index: usize,
    ) -> Result<(), String> {
        let mut table = self.require_editable_table(id)?;
        let count = table.tracks(axis).len();
        if index > count {
            return Err("table insertion index out of range".into());
        }
        require_track_count(count + 1)?;
        let other = match axis {
            TableAxis::Row => TableAxis::Column,
            TableAxis::Column => TableAxis::Row,
        };
        let neighbour = if index > 0 { index - 1 } else { 0 };
        let size = table.tracks(axis)[neighbour];
        let mut candidate = self.clone();
        let mut fresh = Vec::new();
        let mut cross = 0;
        while cross < table.tracks(other).len() {
            let (row, column) = match axis {
                TableAxis::Row => (neighbour, cross),
                TableAxis::Column => (cross, neighbour),
            };
            let owner = *table.cell_at(row, column).ok_or("table slot has no cell")?;
            let start = owner.start(axis);
            let spans = start < index && index < start + owner.span(axis);
            if !spans {
                let template = self.canvas.object(owner.id).cloned();
                let cell = candidate.push_table_cell(id, template.as_ref())?;
                let mut cell = SceneTableCell::new(cell, 0, 0);
                cell.set(axis, index, 1);
                cell.set(other, cross, 1);
                fresh.push(cell);
            }
            cross += if spans { owner.span(other) } else { 1 };
        }
        for cell in &mut table.cells {
            let (start, span) = (cell.start(axis), cell.span(axis));
            if start >= index {
                cell.set(axis, start + 1, span);
            } else if index < start + span {
                cell.set(axis, start, span + 1);
            }
        }
        table.cells.extend(fresh);
        table.cells.sort_by_key(|cell| (cell.row, cell.column));
        table.tracks_mut(axis).insert(index, size);
        self.commit_table(candidate, id, table)
    }

    /// Deletes a row or column with the cells (and content) that only it holds.
    pub fn delete_canvas_table_track(
        &mut self,
        id: CanvasObjectId,
        axis: TableAxis,
        index: usize,
    ) -> Result<(), String> {
        let mut table = self.require_editable_table(id)?;
        let count = table.tracks(axis).len();
        if index >= count {
            return Err("table row or column out of range".into());
        }
        if count == 1 {
            return Err("a table needs at least one row and one column".into());
        }
        let removed: Vec<_> = table
            .cells
            .iter()
            .filter(|cell| cell.start(axis) == index && cell.span(axis) == 1)
            .map(|cell| cell.id)
            .collect();
        table.cells.retain(|cell| !removed.contains(&cell.id));
        for cell in &mut table.cells {
            let (start, span) = (cell.start(axis), cell.span(axis));
            if start > index {
                cell.set(axis, start - 1, span);
            } else if index < start + span {
                cell.set(axis, start, span - 1);
            }
        }
        table.cells.sort_by_key(|cell| (cell.row, cell.column));
        table.tracks_mut(axis).remove(index);
        let mut candidate = self.clone();
        if !removed.is_empty() {
            candidate.remove_canvas_objects_unchecked(&removed)?;
        }
        self.commit_table(candidate, id, table)
    }

    /// Sets one row height or column width; the table frame follows.
    pub fn resize_canvas_table_track(
        &mut self,
        id: CanvasObjectId,
        axis: TableAxis,
        index: usize,
        size: f32,
    ) -> Result<(), String> {
        let mut table = self.require_editable_table(id)?;
        let track = table
            .tracks_mut(axis)
            .get_mut(index)
            .ok_or("table row or column out of range")?;
        if *track == size {
            return Ok(());
        }
        *track = size;
        let candidate = self.clone();
        self.commit_table(candidate, id, table)
    }

    /// Merges cells covering a full rectangle into its top-left cell. Content
    /// of the other cells moves into the merged cell at its sheet position.
    pub fn merge_canvas_table_cells(
        &mut self,
        id: CanvasObjectId,
        cells: &[CanvasObjectId],
    ) -> Result<CanvasObjectId, String> {
        let mut table = self.require_editable_table(id)?;
        let selected: Vec<_> = cells
            .iter()
            .map(|cell| {
                table
                    .cell(*cell)
                    .copied()
                    .ok_or_else(|| format!("canvas object {cell} is not a cell of table {id}"))
            })
            .collect::<Result<_, _>>()?;
        if selected.len() < 2 {
            return Err("select at least two cells to merge".into());
        }
        let bounds = |axis: TableAxis| {
            let start = selected
                .iter()
                .map(|cell| cell.start(axis))
                .min()
                .unwrap_or(0);
            let end = selected
                .iter()
                .map(|cell| cell.start(axis) + cell.span(axis))
                .max()
                .unwrap_or(0);
            (start, end)
        };
        let (rows, columns) = (bounds(TableAxis::Row), bounds(TableAxis::Column));
        let inside = |cell: &SceneTableCell| {
            cell.start(TableAxis::Row) >= rows.0
                && cell.start(TableAxis::Row) + cell.span(TableAxis::Row) <= rows.1
                && cell.start(TableAxis::Column) >= columns.0
                && cell.start(TableAxis::Column) + cell.span(TableAxis::Column) <= columns.1
        };
        let overlaps = |cell: &SceneTableCell| {
            cell.start(TableAxis::Row) < rows.1
                && rows.0 < cell.start(TableAxis::Row) + cell.span(TableAxis::Row)
                && cell.start(TableAxis::Column) < columns.1
                && columns.0 < cell.start(TableAxis::Column) + cell.span(TableAxis::Column)
        };
        if table
            .cells
            .iter()
            .any(|cell| overlaps(cell) && !inside(cell))
        {
            return Err("merged cells must form a rectangle".into());
        }
        let keep = *table
            .cell_at(rows.0, columns.0)
            .ok_or("table slot has no cell")?;
        let merged: HashSet<_> = table
            .cells
            .iter()
            .filter(|cell| inside(cell) && cell.id != keep.id)
            .map(|cell| cell.id)
            .collect();
        let mut candidate = self.clone();
        for parent in candidate.canvas.scene.parents.values_mut() {
            if merged.contains(parent) {
                *parent = keep.id;
            }
        }
        let removed: Vec<_> = merged.iter().copied().collect();
        candidate.remove_canvas_objects_unchecked(&removed)?;
        table.cells.retain(|cell| !merged.contains(&cell.id));
        for cell in &mut table.cells {
            if cell.id == keep.id {
                cell.set(TableAxis::Row, rows.0, rows.1 - rows.0);
                cell.set(TableAxis::Column, columns.0, columns.1 - columns.0);
            }
        }
        self.commit_table(candidate, id, table)?;
        Ok(keep.id)
    }

    /// Splits a merged cell back into single-slot cells styled like it.
    pub fn unmerge_canvas_table_cell(
        &mut self,
        id: CanvasObjectId,
        cell: CanvasObjectId,
    ) -> Result<(), String> {
        let mut table = self.require_editable_table(id)?;
        let merged = *table
            .cell(cell)
            .ok_or_else(|| format!("canvas object {cell} is not a cell of table {id}"))?;
        if merged.row_span == 1 && merged.column_span == 1 {
            return Err("the cell is not merged".into());
        }
        let template = self.canvas.object(cell).cloned();
        let mut candidate = self.clone();
        let (row, column) = (
            merged.start(TableAxis::Row),
            merged.start(TableAxis::Column),
        );
        for r in row..row + merged.span(TableAxis::Row) {
            for c in column..column + merged.span(TableAxis::Column) {
                if (r, c) == (row, column) {
                    continue;
                }
                let fresh = candidate.push_table_cell(id, template.as_ref())?;
                table.cells.push(SceneTableCell::new(fresh, r, c));
            }
        }
        for entry in &mut table.cells {
            if entry.id == cell {
                entry.row_span = 1;
                entry.column_span = 1;
            }
        }
        table.cells.sort_by_key(|cell| (cell.row, cell.column));
        self.commit_table(candidate, id, table)
    }

    pub fn set_canvas_table_stripes(
        &mut self,
        id: CanvasObjectId,
        stripes: TableStripes,
    ) -> Result<(), String> {
        let mut table = self.require_editable_table(id)?;
        if table.stripes == stripes {
            return Ok(());
        }
        table.stripes = stripes;
        let candidate = self.clone();
        self.commit_table(candidate, id, table)
    }

    /// Resizes (and may move) a table; rows and columns scale proportionally.
    pub(crate) fn resize_canvas_table(
        &mut self,
        id: CanvasObjectId,
        rect: Rect,
    ) -> Result<(), String> {
        self.require_editable_table(id)?;
        let mut candidate = self.clone();
        let object = candidate
            .canvas
            .object_mut(id)
            .ok_or_else(|| format!("table {id} not found"))?;
        let delta = rect.min - object.rect.min;
        object.rect.size = rect.size;
        crate::board::validate_object(object)?;
        if delta != Vec2::ZERO {
            candidate.canvas = SceneHierarchy::from_canvas(&candidate.canvas)?
                .preview_translation(&[id], delta)?;
        }
        candidate.relayout_tables()?;
        *self = candidate;
        Ok(())
    }
}

impl crate::document::Document {
    pub fn insert_canvas_table(
        &mut self,
        origin: Vec2,
        rows: usize,
        columns: usize,
        layer: Option<BoardLayerId>,
    ) -> EditResult<CanvasObjectId> {
        self.edit_sheet("Insert table", |sheet| {
            sheet
                .insert_canvas_table(origin, rows, columns, layer)
                .map_err(EditError::Invalid)
        })
    }

    pub fn insert_canvas_table_track(
        &mut self,
        table: CanvasObjectId,
        axis: TableAxis,
        index: usize,
    ) -> EditResult<()> {
        let label = match axis {
            TableAxis::Row => "Insert table row",
            TableAxis::Column => "Insert table column",
        };
        self.edit_sheet(label, |sheet| {
            sheet
                .insert_canvas_table_track(table, axis, index)
                .map_err(EditError::Invalid)
        })
    }

    pub fn delete_canvas_table_track(
        &mut self,
        table: CanvasObjectId,
        axis: TableAxis,
        index: usize,
    ) -> EditResult<()> {
        let label = match axis {
            TableAxis::Row => "Delete table row",
            TableAxis::Column => "Delete table column",
        };
        self.edit_sheet(label, |sheet| {
            sheet
                .delete_canvas_table_track(table, axis, index)
                .map_err(EditError::Invalid)
        })
    }

    pub fn resize_canvas_table_track(
        &mut self,
        table: CanvasObjectId,
        axis: TableAxis,
        index: usize,
        size: f32,
    ) -> EditResult<()> {
        let label = match axis {
            TableAxis::Row => "Resize table row",
            TableAxis::Column => "Resize table column",
        };
        self.edit_sheet(label, |sheet| {
            sheet
                .resize_canvas_table_track(table, axis, index, size)
                .map_err(EditError::Invalid)
        })
    }

    pub fn merge_canvas_table_cells(
        &mut self,
        table: CanvasObjectId,
        cells: &[CanvasObjectId],
    ) -> EditResult<CanvasObjectId> {
        self.edit_sheet("Merge table cells", |sheet| {
            sheet
                .merge_canvas_table_cells(table, cells)
                .map_err(EditError::Invalid)
        })
    }

    pub fn unmerge_canvas_table_cell(
        &mut self,
        table: CanvasObjectId,
        cell: CanvasObjectId,
    ) -> EditResult<()> {
        self.edit_sheet("Unmerge table cell", |sheet| {
            sheet
                .unmerge_canvas_table_cell(table, cell)
                .map_err(EditError::Invalid)
        })
    }

    pub fn set_canvas_table_stripes(
        &mut self,
        table: CanvasObjectId,
        stripes: TableStripes,
    ) -> EditResult<()> {
        self.edit_sheet("Change table stripes", |sheet| {
            sheet
                .set_canvas_table_stripes(table, stripes)
                .map_err(EditError::Invalid)
        })
    }
}

#[cfg(test)]
mod tests;
