//! View-only board gestures. Document edits happen once, on release.

use super::*;
use crate::actions::{BoardAction, apply_board_action, board_actions};
use std::collections::{HashMap, HashSet, VecDeque};
use thinoda_core::{
    CanvasConnector, CanvasObject, CanvasObjectKind, Card, CardKind, ConnectorPort, EditError,
    EditResult, Endpoint, Scene, SceneId, SheetRootMode, ZOrder, Zone, ZoneId,
    board::{
        BoardMember, CanvasCardState, CanvasGroupId, MIN_CARD_SIZE, polyline_midpoint,
        rotate_around, rotated_corners, shape_geometry_with_style,
    },
    scene::{SceneGeometry, SceneItem},
};

#[derive(Default)]
pub(super) struct State {
    pub(super) label: Option<LabelEdit>,
    gesture: Option<Gesture>,
    creation_press: Option<CreationPress>,
    connector: Option<PendingConnector>,
    rotation_pivot: Option<RotationPivot>,
    last_tool: BoardTool,
    suppress_until_release: bool,
    resume_after_touch_cancel: bool,
    control_focused: bool,
    focus_exit: Option<FocusExit>,
    focus_after_duplicate: Option<SceneId>,
    /// An armed shape tool is hovering existing content or a handle, where a
    /// click selects instead of creating (no ghost, no crosshair).
    pub(super) armed_hover: bool,
    /// Unobscured screen position for the armed shape's ghost preview.
    pub(super) ghost: Option<Pos2>,
    /// Shape dragged from a library and where it would drop.
    pub(super) drop_ghost: Option<(ShapeKind, Pos2)>,
    /// Unselected shape whose draw.io-style direction arrows are shown on hover.
    hover_shape: Option<CanvasObjectId>,
    /// Open quick-shape picker of a direction arrow.
    pub(super) quick: Option<QuickShapes>,
    /// Direction arrow under the mouse and when resting there began; `None`
    /// after a press or a closed picker, until the pointer leaves the arrow.
    arrow_hover: Option<((SceneId, ArrowDir), Option<f64>)>,
    /// Shape whose outline band is hovered, where a drag draws a connector.
    pub(super) outline_hover: Option<CanvasObjectId>,
}

/// draw.io-style shape picker shown next to a direction arrow.
pub(super) struct QuickShapes {
    source: CanvasObjectId,
    dir: ArrowDir,
    /// Screen rect of the arrow the picker belongs to.
    anchor: Rect,
    /// Opened by resting the mouse, so it closes once the pointer leaves.
    hover: bool,
    /// Arrow to refocus when the picker was opened from the keyboard.
    return_focus: Option<Id>,
    focus_first: bool,
    /// Tile under the pointer or focus, previewed on the board.
    pub(super) preview: Option<ShapeKind>,
    state: u64,
    sheet: usize,
    sheet_identity: SheetIdentity,
    camera: (EVec2, f32, Rect),
}

struct RotationPivot {
    members: Vec<BoardMember>,
    selected_topics: Vec<TopicId>,
    selected_objects: Vec<CanvasObjectId>,
    states: VecDeque<u64>,
    sheet: usize,
    sheet_identity: SheetIdentity,
    center: Vec2,
}

// Bound view-only bookkeeping even during a very long run of rotation commands.
const MAX_ROTATION_PIVOT_STATES: usize = 512;

enum FocusExit {
    Previous(Id),
    Next(Id),
}

impl State {
    pub(super) fn cancel_gesture(&mut self) {
        self.suppress_until_release |= self.gesture.is_some() || self.creation_press.is_some();
        self.gesture = None;
        self.creation_press = None;
        self.connector = None;
    }

    pub(super) fn is_active(&self) -> bool {
        self.gesture.is_some() || self.creation_press.is_some() || self.suppress_until_release
    }

    pub(super) fn has_pending_gesture(&self) -> bool {
        self.is_active() || self.connector.is_some()
    }

    pub(super) fn has_control_focus(&self) -> bool {
        self.control_focused
    }

    pub(super) fn touch_canceled(&mut self, restart: bool) {
        self.cancel_gesture();
        self.suppress_until_release = !restart;
        self.resume_after_touch_cancel = !restart;
    }

    pub(super) fn resume_after_touch_cancel(&mut self, ctx: &egui::Context) {
        if self.resume_after_touch_cancel
            && ctx.input(|input| {
                (input.pointer.primary_pressed()
                    || input.events.iter().any(|event| {
                        matches!(
                            event,
                            egui::Event::Touch {
                                phase: egui::TouchPhase::Start,
                                ..
                            }
                        )
                    }))
                    && input.multi_touch().is_none()
                    && !input.events.iter().any(|event| {
                        matches!(
                            event,
                            egui::Event::Touch {
                                phase: egui::TouchPhase::Cancel,
                                ..
                            }
                        )
                    })
            })
        {
            self.suppress_until_release = false;
            self.resume_after_touch_cancel = false;
        }
    }
}

fn exit_focus(ui: &Ui, view: &mut MapView, reverse: bool) {
    if let Some(id) = ui.ctx().memory(|memory| memory.focused()) {
        view.board_input.focus_exit = Some(if reverse {
            FocusExit::Previous(id)
        } else {
            FocusExit::Next(id)
        });
        ui.ctx().request_repaint();
    }
}

/// Run before the canvas widget registers so egui can find its previous sibling.
pub(super) fn begin_focus_exit(ctx: &egui::Context, view: &mut MapView) {
    if let Some(FocusExit::Previous(source)) = view.board_input.focus_exit {
        view.board_input.focus_exit = None;
        ctx.memory_mut(|memory| {
            if memory.focused() == Some(source) {
                memory.request_focus(MapCanvas::id());
                memory.move_focus(egui::FocusDirection::Previous);
            }
        });
    }
}

/// Run after the canvas' focusable children register, optionally targeting the minimap.
pub(super) fn end_focus_exit(ctx: &egui::Context, view: &mut MapView, next: Option<Id>) {
    if let Some(FocusExit::Next(source)) = view.board_input.focus_exit {
        view.board_input.focus_exit = None;
        ctx.memory_mut(|memory| {
            if memory.focused() == Some(source) {
                if let Some(next) = next {
                    memory.request_focus(next);
                } else {
                    memory.surrender_focus(source);
                    memory.move_focus(egui::FocusDirection::Next);
                }
            }
        });
    }
}

struct PendingConnector {
    from: Endpoint,
    point: Vec2,
    state: u64,
    sheet: usize,
    sheet_identity: SheetIdentity,
}

struct Gesture {
    kind: GestureKind,
    origin: Vec2,
    guard: GestureGuard,
    moved: bool,
    canceled: bool,
}

struct GestureGuard {
    state: u64,
    sheet: usize,
    sheet_identity: SheetIdentity,
    camera: (EVec2, f32, Rect),
    topics: Vec<TopicId>,
    objects: Vec<CanvasObjectId>,
    selected_object: Option<ObjectSelection>,
    tool: BoardTool,
}

struct CreationPress {
    guard: GestureGuard,
    screen_origin: Pos2,
    touch: Option<(egui::TouchDeviceId, egui::TouchId)>,
    moved: bool,
}

enum GestureKind {
    Create {
        tool: BoardTool,
        start: Vec2,
        current: Vec2,
        from: Endpoint,
    },
    Move {
        start: Vec2,
        current: Vec2,
        anchor: Vec2,
        duplicate: bool,
    },
    Resize {
        item: SceneId,
        original: thinoda_core::Rect,
        rotation: f32,
        grab: Vec2,
        preview: thinoda_core::Rect,
    },
    Rotate {
        item: SceneId,
        center: Vec2,
        original: f32,
        grab_angle: f32,
        preview: f32,
    },
    Endpoint {
        id: CanvasObjectId,
        start: bool,
        point: Vec2,
    },
    Waypoint {
        id: CanvasObjectId,
        index: usize,
        insert: bool,
        original: Vec2,
        point: Vec2,
    },
    Arrow {
        source: CanvasObjectId,
        dir: ArrowDir,
        anchor: Vec2,
        current: Vec2,
        /// Input time of the press, for press-and-hold.
        since: f64,
        /// Moved far enough to become a drag; holding no longer opens the picker.
        far: bool,
    },
    /// Connector dragged out of a shape's outline band.
    Outline {
        source: CanvasObjectId,
        anchor: Vec2,
        current: Vec2,
    },
}

pub(super) struct LabelEdit {
    pub(super) id: CanvasObjectId,
    text: String,
    original: String,
    sheet: usize,
    sheet_identity: SheetIdentity,
    focus_requested: bool,
    focus_retry_after: Option<(egui::ViewportId, u64)>,
    /// Started by typing: the draft holds the typed text and the caret must follow it.
    seeded: bool,
    ime_composing: bool,
}

impl MapView {
    fn observe_board_rotation_selection(&mut self, doc: &Document) {
        let Some(cached) = self.board_input.rotation_pivot.as_ref() else {
            return;
        };
        if cached.sheet != doc.active_index() || cached.sheet_identity != sheet_identity(doc) {
            self.board_input.rotation_pivot = None;
            return;
        }
        // Keep the cache through undo/redo; the lookup below only uses it for
        // document states from this uninterrupted run of rotations.
        if cached.selected_topics == self.selection
            && cached.selected_objects == self.board_selection
        {
            return;
        }
        let mut members = board_actions::movable_board_members(doc.sheet(), self);
        members.sort_unstable();
        if cached.members == members {
            let cached = self.board_input.rotation_pivot.as_mut().expect("checked");
            cached.selected_topics.clone_from(&self.selection);
            cached.selected_objects.clone_from(&self.board_selection);
        } else {
            self.board_input.rotation_pivot = None;
        }
    }

    /// Returns the pivot retained after the last rotation of these members.
    pub(crate) fn board_rotation_pivot(
        &self,
        doc: &Document,
        members: &[BoardMember],
    ) -> Option<Vec2> {
        self.board_input.rotation_pivot.as_ref().and_then(|cached| {
            (cached.states.contains(&doc.state_id())
                && cached.sheet == doc.active_index()
                && cached.sheet_identity == sheet_identity(doc)
                && cached.members == members)
                .then_some(cached.center)
        })
    }

    pub(crate) fn remember_board_rotation_pivot(
        &mut self,
        doc: &Document,
        members: Vec<BoardMember>,
        center: Vec2,
        previous_state: u64,
    ) {
        if let Some(cached) = self.board_input.rotation_pivot.as_mut()
            && cached.sheet == doc.active_index()
            && cached.sheet_identity == sheet_identity(doc)
            && cached.members == members
            && cached.center == center
            && cached.states.contains(&previous_state)
        {
            cached.states.push_back(doc.state_id());
            if cached.states.len() > MAX_ROTATION_PIVOT_STATES {
                cached.states.pop_front();
            }
            cached.selected_topics.clone_from(&self.selection);
            cached.selected_objects.clone_from(&self.board_selection);
            return;
        }
        self.board_input.rotation_pivot = Some(RotationPivot {
            members,
            selected_topics: self.selection.clone(),
            selected_objects: self.board_selection.clone(),
            states: VecDeque::from([previous_state, doc.state_id()]),
            sheet: doc.active_index(),
            sheet_identity: sheet_identity(doc),
            center,
        });
    }

    pub(crate) fn clear_board_rotation_pivot(&mut self) {
        self.board_input.rotation_pivot = None;
    }

    /// Returns the active shape or connector label's id, original text, and draft.
    ///
    /// A failed commit leaves the draft available for recovery.
    pub fn board_label_draft(&self) -> Option<(CanvasObjectId, &str, &str)> {
        self.board_input
            .label
            .as_ref()
            .map(|edit| (edit.id, edit.original.as_str(), edit.text.as_str()))
    }

    /// Validates and returns the draft against the document that owns the edit.
    ///
    /// Call this with the original document *before* cloning it for a snapshot.
    /// A clone has a different sheet identity and cannot establish draft ownership.
    /// A changed, removed, or locked source is rejected without discarding the draft.
    pub fn validated_board_label_draft(
        &self,
        doc: &Document,
    ) -> EditResult<Option<(CanvasObjectId, &str, &str)>> {
        let Some(edit) = self.board_input.label.as_ref() else {
            return Ok(None);
        };
        validate_label_source(edit, doc)?;
        Ok(Some((edit.id, edit.original.as_str(), edit.text.as_str())))
    }

    /// Whether the active shape or connector label differs from its original text.
    /// Inserts a default-size `kind` shape into free space as one undoable
    /// edit: right of the selection, else near the viewport centre. The new
    /// shape is selected and revealed; the active tool is unchanged.
    pub fn insert_board_shape(
        &mut self,
        doc: &mut Document,
        kind: ShapeKind,
    ) -> EditResult<CanvasObjectId> {
        self.try_commit_edit(doc)?;
        self.try_commit_board_label(doc)?;
        let layout = self.layout_for(doc);
        let scene = thinoda_core::scene::build_scene(doc.sheet(), &layout);
        let size = default_size(BoardTool::Shape(kind));
        let center = insert_spot(doc.sheet(), self, &scene, size);
        let rect = thinoda_core::Rect::from_center_size(center, size);
        let id = doc.add_canvas_object(CanvasObject::shape(kind, rect))?;
        select_hit(self, doc.sheet(), SceneHit::Object(id), false);
        self.reveal_board_object(id);
        Ok(id)
    }

    pub fn has_board_label_changes(&self) -> bool {
        self.board_input
            .label
            .as_ref()
            .is_some_and(|edit| edit.text != edit.original)
    }
}

fn validate_label_source(edit: &LabelEdit, doc: &Document) -> EditResult<()> {
    if doc.active_index() != edit.sheet || sheet_identity(doc) != edit.sheet_identity {
        return Err(EditError::Invalid(
            "Label belongs to a different sheet; draft retained".into(),
        ));
    }
    let object = doc.sheet().canvas.object(edit.id).ok_or_else(|| {
        EditError::Invalid("Label source no longer exists; draft retained".into())
    })?;
    if !board_actions::member_is_movable(doc.sheet(), BoardMember::Object(edit.id))
        || matches!(object.kind, CanvasObjectKind::Ink(_))
        || object.label() != edit.original
    {
        return Err(EditError::Invalid(
            "Label source changed or was locked; draft retained".into(),
        ));
    }
    Ok(())
}

pub(super) fn begin_label(view: &mut MapView, doc: &Document, id: CanvasObjectId) -> bool {
    let sheet = doc.sheet();
    let Some(object) = sheet.canvas.object(id) else {
        return false;
    };
    if !board_actions::member_is_movable(sheet, BoardMember::Object(id))
        || matches!(object.kind, CanvasObjectKind::Ink(_))
    {
        return false;
    }
    if view.editing.is_some() || view.board_input.label.is_some() {
        return false;
    }
    view.board_input.label = Some(LabelEdit {
        id,
        text: object.label().to_owned(),
        original: object.label().to_owned(),
        sheet: doc.active_index(),
        sheet_identity: sheet_identity(doc),
        focus_requested: false,
        focus_retry_after: None,
        seeded: false,
        ime_composing: false,
    });
    true
}

pub(super) fn commit_label(view: &mut MapView, doc: &mut Document) -> EditResult<bool> {
    let Some(edit) = view.board_input.label.as_ref() else {
        return Ok(false);
    };
    if edit.text == edit.original {
        view.board_input.label = None;
        return Ok(false);
    }
    validate_label_source(edit, doc)?;
    let id = edit.id;
    let text = edit.text.clone();
    doc.update_canvas_object("Edit board label", id, |object| match &mut object.kind {
        CanvasObjectKind::Shape { label, .. } => *label = text,
        CanvasObjectKind::Connector(connector) => connector.label = text,
        CanvasObjectKind::Ink(_) => {}
    })?;
    view.board_input.label = None;
    Ok(true)
}

fn snapped(sheet: &Sheet, p: Vec2) -> Vec2 {
    if sheet.board.snap {
        let grid = sheet.board.grid_size;
        Vec2::new((p.x / grid).round() * grid, (p.y / grid).round() * grid)
    } else {
        p
    }
}

fn drag_alignment_guides(
    sheet: &Sheet,
    view: &MapView,
    scene: &Scene,
    movable: &[BoardMember],
    delta: Vec2,
    tolerance: f32,
) -> crate::topic_drag::DragGuides {
    let selected = board_actions::selected_board_members(sheet, view);
    let is_selected = |id| match id {
        SceneId::Topic(id) => {
            view.selection.contains(&id)
                || sheet
                    .board_member_for_topic(id)
                    .is_some_and(|member| selected.contains(&member))
        }
        SceneId::Object(id) => selected.contains(&BoardMember::Object(id)),
        SceneId::Zone(_) => false,
    };
    let is_moving = |id| match id {
        SceneId::Topic(id) => sheet.board_member_for_topic(id).map_or_else(
            || view.selection.contains(&id) && sheet.is_floating(id),
            |member| movable.contains(&member),
        ),
        SceneId::Object(id) => movable.contains(&BoardMember::Object(id)),
        SceneId::Zone(_) => false,
    };
    let Some(bounds) = scene
        .items
        .iter()
        .filter(|item| is_moving(item.id))
        .filter_map(|item| moved_visible_bounds(sheet, item, delta))
        .reduce(|a, b| a.union(&b))
    else {
        return crate::topic_drag::DragGuides::default();
    };
    let references = scene
        .items
        .iter()
        .filter(|item| {
            !is_selected(item.id)
                && match item.id {
                    SceneId::Topic(id) => sheet.is_floating(id),
                    SceneId::Object(_) | SceneId::Zone(_) => true,
                }
        })
        .filter_map(|item| item.visible_bounds());
    crate::topic_drag::drag_guides_for_bounds(bounds, references, tolerance)
}

fn moved_visible_bounds(
    sheet: &Sheet,
    item: &thinoda_core::SceneItem,
    delta: Vec2,
) -> Option<thinoda_core::Rect> {
    let bounds = item.bounds.translate(delta);
    let clip = match item.id {
        SceneId::Topic(_) => item.clip,
        SceneId::Object(id) => sheet
            .canvas
            .objects
            .iter()
            .find(|object| object.id == id)
            .and_then(|object| {
                let center = if matches!(&object.kind, CanvasObjectKind::Connector(_)) {
                    bounds.center()
                } else {
                    object.rect.center() + delta
                };
                thinoda_core::scene::object_clip_at(sheet, id, center)
            }),
        SceneId::Zone(_) => None,
    };
    let Some(clip) = clip else {
        return Some(bounds);
    };
    if !bounds.intersects(&clip) {
        return None;
    }
    let max = bounds.max();
    let clip_max = clip.max();
    let min = Vec2::new(bounds.min.x.max(clip.min.x), bounds.min.y.max(clip.min.y));
    let max = Vec2::new(max.x.min(clip_max.x), max.y.min(clip_max.y));
    Some(thinoda_core::Rect::from_min_size(min, max - min))
}

fn drag_rect(start: Vec2, end: Vec2, minimum: f32) -> thinoda_core::Rect {
    let min = Vec2::new(start.x.min(end.x), start.y.min(end.y));
    let size = Vec2::new((end.x - start.x).abs(), (end.y - start.y).abs());
    thinoda_core::Rect::from_min_size(min, Vec2::new(size.x.max(minimum), size.y.max(minimum)))
}

fn default_size(tool: BoardTool) -> Vec2 {
    match tool {
        BoardTool::Sticky => Vec2::new(180.0, 150.0),
        BoardTool::Text => Vec2::new(200.0, 80.0),
        BoardTool::Shape(_) => Vec2::new(160.0, 100.0),
        BoardTool::Frame => Vec2::new(360.0, 240.0),
        _ => Vec2::new(120.0, 0.0),
    }
}

fn endpoint_at(
    sheet: &Sheet,
    scene: &Scene,
    point: Vec2,
    except: Option<CanvasObjectId>,
) -> Endpoint {
    // Excluding the connector being edited lets a handle reach a target beneath it.
    let hit = if let Some(except) = except {
        let mut scene = scene.clone();
        scene
            .items
            .retain(|item| item.id != SceneId::Object(except));
        scene.hit_test(point)
    } else {
        scene.hit_test(point)
    };
    match hit {
        Some(SceneHit::Topic(id)) if !sheet.is_anchor(id) => Endpoint::Topic(id),
        Some(SceneHit::Object(id)) => Endpoint::Object(id),
        _ => Endpoint::Point(snapped(sheet, point)),
    }
}

fn selected_member(view: &MapView, member: BoardMember) -> bool {
    match member {
        BoardMember::Card(id) => view.selection.contains(&id),
        BoardMember::Object(id) => view.board_selection.contains(&id),
    }
}

/// Adds complete mixed groups to the existing selection, without toggling hits.
pub(crate) fn expand_selection(view: &mut MapView, sheet: &Sheet) {
    for member in board_actions::selected_board_members(sheet, view) {
        match member {
            BoardMember::Card(id) if !view.selection.contains(&id) => view.selection.push(id),
            BoardMember::Object(id) if !view.board_selection.contains(&id) => {
                view.board_selection.push(id)
            }
            _ => {}
        }
    }
}

/// Whether editing a topic is permitted by its card and group locks.
pub(super) fn topic_is_unlocked(sheet: &Sheet, id: TopicId) -> bool {
    sheet
        .board_member_for_topic(id)
        .is_none_or(|member| board_actions::member_is_movable(sheet, member))
}

/// Replaces the selection, or toggles an entire card/object group when additive.
pub(super) fn select_hit(view: &mut MapView, sheet: &Sheet, hit: SceneHit, additive: bool) {
    view.selected_object = None;
    view.relationship_from = None;
    view.pending_reveal = None;
    view.pending_board_reveal = None;
    let member = match hit {
        SceneHit::Topic(id) => sheet.board_member_for_topic(id),
        SceneHit::Object(id) if sheet.canvas.object(id).is_some() => Some(BoardMember::Object(id)),
        _ => None,
    };
    if let Some(member) = member {
        let members = sheet.expand_board_selection(&[member]).unwrap_or_default();
        if !additive {
            view.selection.clear();
            view.board_selection.clear();
        } else if members.iter().all(|member| selected_member(view, *member)) {
            view.selection
                .retain(|id| !members.contains(&BoardMember::Card(*id)));
            view.board_selection
                .retain(|id| !members.contains(&BoardMember::Object(*id)));
            return;
        }
        for member in members {
            match member {
                BoardMember::Card(id) if !view.selection.contains(&id) => view.selection.push(id),
                BoardMember::Object(id) if !view.board_selection.contains(&id) => {
                    view.board_selection.push(id)
                }
                _ => {}
            }
        }
        return;
    }
    match hit {
        SceneHit::Topic(id) => {
            if sheet.is_anchor(id) {
                return;
            }
            if additive {
                view.toggle_selected(id);
            } else {
                view.select_only(id);
            }
        }
        SceneHit::Object(_) => {}
    }
}

fn selected(view: &MapView, hit: SceneHit) -> bool {
    match hit {
        SceneHit::Topic(id) => view.selection.contains(&id),
        SceneHit::Object(id) => view.board_selection.contains(&id),
    }
}

fn is_board_topic(sheet: &Sheet, view: &MapView, id: TopicId) -> bool {
    sheet.is_floating(id)
        && (sheet.root_mode == SheetRootMode::Hidden
            || sheet.topic(id).is_some_and(|topic| topic.card.is_some())
            || !view.board_selection.is_empty())
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct SheetIdentity {
    document: u64,
    allocation: usize,
}

fn sheet_identity(doc: &Document) -> SheetIdentity {
    SheetIdentity {
        document: doc.instance_id(),
        allocation: std::ptr::from_ref(doc.sheet()) as usize,
    }
}

fn gesture(doc: &Document, view: &MapView, kind: GestureKind, origin: Vec2) -> Gesture {
    Gesture {
        kind,
        origin,
        guard: GestureGuard::new(doc, view),
        moved: false,
        canceled: false,
    }
}

impl GestureGuard {
    fn new(doc: &Document, view: &MapView) -> Self {
        Self {
            state: doc.state_id(),
            sheet: doc.active_index(),
            sheet_identity: sheet_identity(doc),
            camera: (view.offset, view.zoom, view.canvas_rect),
            topics: view.selection.clone(),
            objects: view.board_selection.clone(),
            selected_object: view.selected_object,
            tool: view.board_tool,
        }
    }

    fn changed(&self, doc: &Document, view: &MapView) -> bool {
        self.state != doc.state_id()
            || self.sheet != doc.active_index()
            || self.sheet_identity != sheet_identity(doc)
            || self.camera != (view.offset, view.zoom, view.canvas_rect)
            || self.topics != view.selection
            || self.objects != view.board_selection
            || self.selected_object != view.selected_object
            || self.tool != view.board_tool
    }
}

fn canvas_point_unobscured(ui: &Ui, response: &Response, point: Pos2) -> bool {
    response.interact_rect.contains(point)
        && ui.clip_rect().contains(point)
        && ui.ctx().layer_id_at(point) == Some(response.layer_id)
        && !ui
            .ctx()
            .interactive_rects_last_pass()
            .into_iter()
            .any(|rect| rect != response.interact_rect && rect.contains(point))
}

fn creation_input(
    ctx: &egui::Context,
    press: &mut CreationPress,
    started_here: bool,
    event_offset: usize,
) -> (Option<Pos2>, bool) {
    let threshold = ctx.options(|options| options.input_options.max_click_dist);
    ctx.input(|input| {
        let mut started = !started_here;
        let mut duplicate_touch_press = false;
        let mut released = None;
        let mut touch_ended = false;
        let mut canceled = !input.focused || input.key_pressed(Key::Escape);
        let events = &input.events[event_offset..];
        let matching_touch_start = events.iter().any(|event| {
            matches!(
                event,
                egui::Event::Touch {
                    device_id,
                    id,
                    phase: egui::TouchPhase::Start,
                    pos,
                    ..
                } if pos.distance(press.screen_origin) <= 1.0
                    && press.touch.is_none_or(|touch| touch == (*device_id, *id))
            )
        });
        for event in events {
            match event {
                egui::Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    ..
                } if !started && released.is_none() && *pos == press.screen_origin => {
                    started = true;
                }
                egui::Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    ..
                } if started
                    && released.is_none()
                    && !duplicate_touch_press
                    && matching_touch_start
                    && pos.distance(press.screen_origin) <= 1.0 =>
                {
                    // Some browsers send both pointerdown and touchstart for one contact.
                    duplicate_touch_press = true;
                }
                egui::Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: false,
                    ..
                } if started && released.is_none() => {
                    press.moved |= press.screen_origin.distance(*pos) > threshold;
                    released = Some(*pos);
                }
                egui::Event::PointerMoved(pos) if started && released.is_none() => {
                    press.moved |= press.screen_origin.distance(*pos) > threshold;
                }
                egui::Event::PointerGone if started && released.is_none() => canceled = true,
                egui::Event::PointerButton { .. } | egui::Event::WindowFocused(false) => {
                    canceled = true;
                }
                egui::Event::Touch {
                    device_id,
                    id,
                    phase,
                    ..
                } => {
                    let touch = (*device_id, *id);
                    if press.touch.is_some_and(|previous| previous != touch) {
                        canceled = true;
                    } else {
                        press.touch = Some(touch);
                    }
                    canceled |= *phase == egui::TouchPhase::Cancel;
                    touch_ended |= *phase == egui::TouchPhase::End;
                }
                _ => {}
            }
        }
        canceled |=
            !started || (released.is_none() && (!input.pointer.primary_down() || touch_ended));
        (released, canceled)
    })
}

fn create(
    doc: &mut Document,
    view: &mut MapView,
    tool: BoardTool,
    rect: thinoda_core::Rect,
    endpoints: Option<(Endpoint, Endpoint)>,
) -> EditResult<bool> {
    match tool {
        BoardTool::Sticky | BoardTool::Text => {
            let (kind, title) = if tool == BoardTool::Sticky {
                (CardKind::Sticky, view.board_labels.sticky.clone())
            } else {
                (CardKind::Text, view.board_labels.text.clone())
            };
            let id = doc.add_card(&title, rect.center(), Card::new(kind, rect.size))?;
            view.select_only(id);
            view.begin_edit(doc.sheet(), id, true);
            Ok(true)
        }
        BoardTool::Shape(kind) => {
            let id = doc.add_canvas_object(CanvasObject::shape(kind, rect))?;
            select_hit(view, doc.sheet(), SceneHit::Object(id), false);
            Ok(true)
        }
        BoardTool::Connector => {
            let Some((from, to)) = endpoints else {
                return Ok(false);
            };
            let id =
                doc.add_canvas_object(CanvasObject::connector(CanvasConnector::new(from, to)))?;
            select_hit(view, doc.sheet(), SceneHit::Object(id), false);
            Ok(true)
        }
        BoardTool::Frame => {
            let id = doc.create_board_frame(&view.board_labels.new_frame, rect)?;
            view.select_object(ObjectSelection::Zone(id));
            Ok(true)
        }
        BoardTool::Select
        | BoardTool::Hand
        | BoardTool::Pen
        | BoardTool::Highlighter
        | BoardTool::ObjectEraser
        | BoardTool::PixelEraser
        | BoardTool::Lasso => Ok(false),
    }
}

fn create_at(
    doc: &mut Document,
    view: &mut MapView,
    tool: BoardTool,
    point: Vec2,
) -> EditResult<bool> {
    let center = snapped(doc.sheet(), point);
    let size = default_size(tool);
    let rect = thinoda_core::Rect::from_center_size(center, size);
    let ends = (tool == BoardTool::Connector).then(|| {
        (
            Endpoint::Point(center - Vec2::new(60.0, 0.0)),
            Endpoint::Point(center + Vec2::new(60.0, 0.0)),
        )
    });
    create(doc, view, tool, rect, ends)
}

const INSERT_GAP: f32 = 40.0;

/// Snapped centre nearest `preferred` where a `size` item overlaps no topic,
/// card or shape, searching rings of grid cells and preferring the same row
/// and the right-hand side (draw.io inserts beside content, never on it).
fn free_spot(sheet: &Sheet, scene: &Scene, preferred: Vec2, size: Vec2) -> Vec2 {
    let blocked = |center: Vec2| {
        let target = thinoda_core::Rect::from_center_size(center, size).expand(8.0);
        scene.items.iter().any(|item| {
            let solid = match item.id {
                SceneId::Topic(_) => true,
                SceneId::Object(id) => sheet
                    .canvas
                    .object(id)
                    .is_some_and(|object| !object.is_connector()),
                SceneId::Zone(_) => false,
            };
            solid && item.bounds.intersects(&target)
        })
    };
    let step = Vec2::new(size.x + INSERT_GAP, size.y + INSERT_GAP);
    for ring in 0..=8_i32 {
        let mut cells: Vec<_> = (-ring..=ring)
            .flat_map(|dy| (-ring..=ring).map(move |dx| (dx, dy)))
            .filter(|(dx, dy)| dx.abs().max(dy.abs()) == ring)
            .collect();
        cells.sort_by_key(|&(dx, dy)| (dy.abs(), dx < 0, dx.abs(), dy < 0));
        for (dx, dy) in cells {
            let center = snapped(
                sheet,
                preferred + Vec2::new(dx as f32 * step.x, dy as f32 * step.y),
            );
            if !blocked(center) {
                return center;
            }
        }
    }
    snapped(sheet, preferred)
}

/// Free insertion centre right of the selection, else near the viewport centre.
fn insert_spot(sheet: &Sheet, view: &MapView, scene: &Scene, size: Vec2) -> Vec2 {
    let selection = scene
        .items
        .iter()
        .filter(|item| match item.id {
            SceneId::Topic(id) => selected(view, SceneHit::Topic(id)),
            SceneId::Object(id) => selected(view, SceneHit::Object(id)),
            SceneId::Zone(_) => false,
        })
        .map(|item| item.bounds)
        .reduce(|a, b| a.union(&b));
    let preferred = selection.map_or_else(
        || view.xf().sheet(view.canvas_rect.center()),
        |bounds| {
            Vec2::new(
                bounds.min.x + bounds.size.x + INSERT_GAP + size.x / 2.0,
                bounds.center().y,
            )
        },
    );
    free_spot(sheet, scene, preferred, size)
}

/// True when an armed shape must select rather than create at `point`: topics,
/// cards, objects, ink, annotations and frame edges or title bands are content.
fn armed_target(
    doc: &Document,
    view: &MapView,
    layout: &Layout,
    scene: &Scene,
    point: Vec2,
) -> bool {
    let tolerance = 6.0 / view.zoom;
    scene.hit_test(point).is_some()
        || outline_source(doc.sheet(), scene, view, point).is_some()
        || hit_zone(doc.sheet(), point, tolerance).is_some()
        || hit_object(doc.sheet(), layout, point, tolerance).is_some()
}

/// Shape whose outline band, the ring just outside its body, holds `point`.
/// Dragging from there draws a connector, as on draw.io's hover outline.
fn outline_source(
    sheet: &Sheet,
    scene: &Scene,
    view: &MapView,
    point: Vec2,
) -> Option<CanvasObjectId> {
    if scene.hit_test(point).is_some() {
        return None;
    }
    let band = thinoda_core::scene::HIT_TOLERANCE + OUTLINE_BAND / view.zoom;
    let Some(SceneHit::Object(id)) = scene.hit_test_tolerance(point, band) else {
        return None;
    };
    arrow_source(sheet, id).map(|object| object.id)
}

/// Sheet-space outline of a shape item, for the outline-band highlight.
fn item_outline(item: &SceneItem) -> Option<Vec<Vec2>> {
    match &item.geometry {
        SceneGeometry::Polygon { outline, .. } => Some(outline.clone()),
        SceneGeometry::Rect { rect, rotation } => Some(rotated_corners(*rect, *rotation).to_vec()),
        _ => None,
    }
}

fn prepare_edit(doc: &mut Document, view: &mut MapView, out: &mut CanvasOutput) -> bool {
    if let Err(error) = view
        .try_commit_edit(doc)
        .and_then(|_| view.try_commit_board_label(doc))
    {
        out.error = Some(error);
        false
    } else {
        true
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ArrowDir {
    Up,
    Right,
    Down,
    Left,
}

impl ArrowDir {
    const ALL: [ArrowDir; 4] = [
        ArrowDir::Up,
        ArrowDir::Right,
        ArrowDir::Down,
        ArrowDir::Left,
    ];

    fn unit(self) -> Vec2 {
        match self {
            ArrowDir::Up => Vec2::new(0.0, -1.0),
            ArrowDir::Right => Vec2::new(1.0, 0.0),
            ArrowDir::Down => Vec2::new(0.0, 1.0),
            ArrowDir::Left => Vec2::new(-1.0, 0.0),
        }
    }

    /// The reverse direction.
    fn opposite(self) -> ArrowDir {
        match self {
            ArrowDir::Up => ArrowDir::Down,
            ArrowDir::Right => ArrowDir::Left,
            ArrowDir::Down => ArrowDir::Up,
            ArrowDir::Left => ArrowDir::Right,
        }
    }

    /// Bounds extent along the direction.
    fn extent(self, bounds: thinoda_core::Rect) -> f32 {
        match self {
            ArrowDir::Up | ArrowDir::Down => bounds.size.y,
            ArrowDir::Right | ArrowDir::Left => bounds.size.x,
        }
    }

    /// Midpoint of the bounds side facing the direction.
    fn side(self, bounds: thinoda_core::Rect) -> Vec2 {
        match self {
            ArrowDir::Up => bounds.top_center(),
            ArrowDir::Right => bounds.right_center(),
            ArrowDir::Down => bounds.bottom_center(),
            ArrowDir::Left => bounds.left_center(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ControlKind {
    Resize,
    Rotate,
    From,
    To,
    Waypoint(usize),
    InsertWaypoint(usize, usize),
    Arrow(ArrowDir),
}

#[derive(Clone, Copy)]
struct Control {
    item: SceneId,
    kind: ControlKind,
    point: Vec2,
    rect: thinoda_core::Rect,
    rotation: f32,
    /// Shown only while hovering an unselected shape; not a Tab stop.
    hover: bool,
}

fn control_id(control: Control) -> Id {
    control_kind_id(control.item, control.kind, control.hover)
}

fn control_kind_id(item: SceneId, kind: ControlKind, hover: bool) -> Id {
    let part = match kind {
        ControlKind::Resize => (0, 0, 0),
        ControlKind::Rotate => (1, 0, 0),
        ControlKind::From => (2, 0, 0),
        ControlKind::To => (3, 0, 0),
        ControlKind::Arrow(dir) => (4, dir as usize, 0),
        ControlKind::Waypoint(index) => (5, index, 0),
        ControlKind::InsertWaypoint(index, segment) => (6, index, segment),
    };
    // Hover arrows never share an Id with the selection's, so focus cannot
    // silently move onto a non-focusable hover control.
    MapCanvas::id().with(("board-control", item, part, hover))
}

/// After keyboard activation, focus the same arrow on the new selection so
/// Enter can keep chaining; otherwise focus the new selection itself.
fn focus_next_arrow(ui: &Ui, view: &mut MapView, sheet: &Sheet, scene: &Scene, dir: ArrowDir) {
    let next = match view.board_selection[..] {
        [id] => arrow_controls(view, sheet, scene, id, false)
            .into_iter()
            .find(|control| control.kind == ControlKind::Arrow(dir))
            .filter(|control| view.canvas_rect.contains(view.xf().pos(control.point))),
        _ => None,
    };
    match next {
        Some(control) => ui
            .ctx()
            .memory_mut(|memory| memory.request_focus(control_id(control))),
        None => focus_duplicated_selection(ui, view),
    }
}

/// Screen distance between a shape's bounds and its direction arrows.
const ARROW_OFFSET: f32 = 32.0;
/// Gap between a shape and its draw.io-style connected clone (`defaultEdgeLength`).
const ARROW_GAP: f32 = 80.0;
/// Shapes smaller than this on screen get no direction arrows.
const ARROW_MIN_SCREEN: f32 = 40.0;
/// Screen movement turning an arrow press into a drag.
const ARROW_DRAG: f32 = 8.0;
/// Screen width of the band outside a shape where a drag starts a connector.
const OUTLINE_BAND: f32 = 8.0;
/// Seconds the mouse rests on, or a press holds, an arrow before its shape picker opens.
const QUICK_DELAY: f64 = 0.6;
/// Shapes offered by the picker after the source's own kind.
const QUICK_SHAPES: [ShapeKind; 10] = [
    ShapeKind::Rectangle,
    ShapeKind::RoundedRectangle,
    ShapeKind::Ellipse,
    ShapeKind::Diamond,
    ShapeKind::Triangle,
    ShapeKind::Hexagon,
    ShapeKind::Parallelogram,
    ShapeKind::Cylinder,
    ShapeKind::Cloud,
    ShapeKind::Document,
];
const QUICK_COLUMNS: usize = 5;
const QUICK_TILE: f32 = 40.0;

fn arrow_point(view: &MapView, object: &CanvasObject, dir: ArrowDir) -> Vec2 {
    dir.side(object.rotated_bounds()) + dir.unit() * (ARROW_OFFSET / view.zoom)
}

fn arrow_rect(view: &MapView, point: Vec2) -> Rect {
    Rect::from_center_size(view.xf().pos(point), vec2(28.0, 28.0))
}

/// Shapes offering direction arrows: single, unlocked and ungrouped.
fn arrow_source(sheet: &Sheet, id: CanvasObjectId) -> Option<&CanvasObject> {
    sheet
        .canvas
        .object(id)
        .filter(|object| matches!(object.kind, CanvasObjectKind::Shape { .. }))
        .filter(|_| board_actions::member_is_movable(sheet, BoardMember::Object(id)))
        .filter(|_| {
            sheet
                .expand_board_selection(&[BoardMember::Object(id)])
                .is_ok_and(|members| members.len() == 1)
        })
}

fn arrow_controls(
    view: &MapView,
    sheet: &Sheet,
    scene: &Scene,
    id: CanvasObjectId,
    hover: bool,
) -> Vec<Control> {
    let Some(object) = arrow_source(sheet, id) else {
        return Vec::new();
    };
    let bounds = object.rotated_bounds();
    if bounds.size.x * view.zoom < ARROW_MIN_SCREEN || bounds.size.y * view.zoom < ARROW_MIN_SCREEN
    {
        return Vec::new();
    }
    let item = SceneId::Object(id);
    let clip = scene.item(item).and_then(|item| item.clip);
    ArrowDir::ALL
        .into_iter()
        .map(|dir| Control {
            item,
            kind: ControlKind::Arrow(dir),
            point: arrow_point(view, object, dir),
            rect: object.rect,
            rotation: object.rotation,
            hover,
        })
        .filter(|control| clip.is_none_or(|clip| clip.contains(control.point)))
        .collect()
}

/// Keyboard-reachable controls, in Tab order.
fn tab_controls(view: &MapView, sheet: &Sheet, layout: &Layout, scene: &Scene) -> Vec<Control> {
    let mut list = controls(view, sheet, layout, scene);
    list.retain(|control| !control.hover);
    list
}

/// Tracks the unselected shape whose arrows follow the mouse, kept while the
/// pointer stays near it so the arrows can be reached.
fn update_hover_shape(
    view: &mut MapView,
    sheet: &Sheet,
    scene: &Scene,
    response: &Response,
    over_controls: bool,
) {
    if over_controls || view.board_input.is_active() || view.board_input.quick.is_some() {
        return;
    }
    let xf = view.xf();
    let current = view.board_input.hover_shape;
    view.board_input.hover_shape = response
        .hover_pos()
        .filter(|_| matches!(view.board_tool, BoardTool::Select | BoardTool::Shape(_)))
        .and_then(|pos| {
            if let Some(SceneHit::Object(id)) = scene.hit_test(xf.sheet(pos))
                && arrow_source(sheet, id).is_some()
            {
                return Some(id);
            }
            if let Some(id) = outline_source(sheet, scene, view, xf.sheet(pos)) {
                return Some(id);
            }
            current.filter(|id| {
                sheet.canvas.object(*id).is_some_and(|object| {
                    xf.rect(object.rotated_bounds())
                        .expand(ARROW_OFFSET + 18.0)
                        .contains(pos)
                })
            })
        });
}

fn controls(view: &MapView, sheet: &Sheet, layout: &Layout, scene: &Scene) -> Vec<Control> {
    if !matches!(view.board_tool, BoardTool::Select | BoardTool::Shape(_))
        || view.editing.is_some()
        || view.editing_board_object().is_some()
        || view.selected_object.is_some()
        || !matches!(view.drag, Drag::None)
    {
        return Vec::new();
    }
    let mut list = selection_controls(view, sheet, layout, scene);
    if let Some(id) = view.board_input.hover_shape
        && !list
            .iter()
            .any(|control| control.item == SceneId::Object(id))
    {
        list.extend(arrow_controls(view, sheet, scene, id, true));
    }
    list
}

fn selection_controls(
    view: &MapView,
    sheet: &Sheet,
    layout: &Layout,
    scene: &Scene,
) -> Vec<Control> {
    let (item, rect, rotation) = if view.selection.is_empty() && view.board_selection.len() == 1 {
        let id = view.board_selection[0];
        let Some(object) = sheet.canvas.object(id) else {
            return Vec::new();
        };
        if !board_actions::member_is_movable(sheet, BoardMember::Object(id))
            || !sheet
                .expand_board_selection(&[BoardMember::Object(id)])
                .is_ok_and(|members| members.len() == 1)
        {
            return Vec::new();
        }
        let item = SceneId::Object(id);
        if let CanvasObjectKind::Connector(connector) = &object.kind {
            let Some(SceneGeometry::Polyline { points, .. }) =
                scene.item(item).map(|item| &item.geometry)
            else {
                return Vec::new();
            };
            let (Some(from), Some(to)) = (points.first(), points.last()) else {
                return Vec::new();
            };
            let clip = scene.item(item).and_then(|item| item.clip);
            let make_control = |kind, point| Control {
                item,
                kind,
                point,
                rect: object.rect,
                rotation: 0.0,
                hover: false,
            };
            let mut list: Vec<_> = [(ControlKind::From, *from), (ControlKind::To, *to)]
                .into_iter()
                .filter(|(_, point)| clip.is_none_or(|clip| clip.contains(*point)))
                .map(|(kind, point)| make_control(kind, point))
                .collect();
            list.extend(
                connector
                    .waypoints
                    .iter()
                    .enumerate()
                    .filter(|(_, point)| {
                        clip.is_none_or(|clip| clip.contains(**point))
                            && view.canvas_rect.contains(view.xf().pos(**point))
                    })
                    .map(|(index, &point)| make_control(ControlKind::Waypoint(index), point)),
            );
            let xf = view.xf();
            let mut occupied: Vec<_> = list.iter().map(|control| xf.pos(control.point)).collect();
            for (segment, handle) in
                thinoda_core::scene::connector_insert_handles(sheet, layout, connector)
                    .into_iter()
                    .enumerate()
            {
                let center = xf.pos(handle.point);
                if clip.is_some_and(|clip| !clip.contains(handle.point))
                    || !view.canvas_rect.contains(center)
                    || occupied
                        .iter()
                        .any(|point| point.distance_sq(center) < 18.0 * 18.0)
                {
                    continue;
                }
                occupied.push(center);
                list.push(make_control(
                    ControlKind::InsertWaypoint(handle.insert_at, segment),
                    handle.point,
                ));
            }
            return list;
        }
        (item, object.rect, object.rotation)
    } else if view.board_selection.is_empty() && view.selection.len() == 1 {
        let id = view.selection[0];
        let Some(card) = sheet.topic(id).and_then(|topic| topic.card) else {
            return Vec::new();
        };
        if !topic_is_unlocked(sheet, id)
            || !sheet
                .expand_board_selection(&[BoardMember::Card(id)])
                .is_ok_and(|members| members.len() == 1)
        {
            return Vec::new();
        }
        let Some(rect) = layout.rect(id) else {
            return Vec::new();
        };
        (SceneId::Topic(id), rect, card.rotation)
    } else {
        return Vec::new();
    };

    let corner = rotated_corners(rect, rotation)[2];
    // draw.io keeps the rotation handle off the top-right corner, clear of the ↑ arrow.
    let bounds = thinoda_core::board::rotated_bounds(rect, rotation);
    let anchor = Vec2::new(bounds.max().x, bounds.min.y);
    let outward = anchor + Vec2::new(14.0, -14.0) * (1.0 / view.zoom);
    let clip = scene.item(item).and_then(|item| item.clip);
    let mut list: Vec<Control> = [
        (ControlKind::Resize, corner),
        (ControlKind::Rotate, outward),
    ]
    .into_iter()
    .filter(|(kind, point)| {
        clip.is_none_or(|clip| {
            clip.contains(*point) || (*kind == ControlKind::Rotate && clip.contains(anchor))
        })
    })
    .map(|(kind, point)| Control {
        item,
        kind,
        point,
        rect,
        rotation,
        hover: false,
    })
    .collect();
    if let SceneId::Object(id) = item {
        list.extend(arrow_controls(view, sheet, scene, id, false));
    }
    list
}

fn control_label(view: &MapView, kind: ControlKind) -> String {
    match kind {
        ControlKind::Resize => view.board_labels.resize.clone(),
        ControlKind::Rotate => view.board_labels.rotate.clone(),
        ControlKind::From => format!("{} {}", view.board_labels.connector, view.board_labels.from),
        ControlKind::To => format!("{} {}", view.board_labels.connector, view.board_labels.to),
        ControlKind::Waypoint(index) => format!("{} {}", view.board_labels.waypoint, index + 1),
        ControlKind::InsertWaypoint(_, segment) => {
            format!("{} {}", view.board_labels.insert_waypoint, segment + 1)
        }
        ControlKind::Arrow(dir) => match dir {
            ArrowDir::Up => view.board_labels.connect_up.clone(),
            ArrowDir::Right => view.board_labels.connect_right.clone(),
            ArrowDir::Down => view.board_labels.connect_down.clone(),
            ArrowDir::Left => view.board_labels.connect_left.clone(),
        },
    }
}

fn rotated_resize(
    rect: thinoda_core::Rect,
    rotation: f32,
    grab: Vec2,
    current: Vec2,
) -> thinoda_core::Rect {
    let local = rotate_around(current - grab, Vec2::ZERO, -rotation);
    let size = Vec2::new(
        (rect.size.x + local.x).max(MIN_CARD_SIZE),
        (rect.size.y + local.y).max(MIN_CARD_SIZE),
    );
    // Keep the opposite rotated corner fixed while the center changes.
    let anchor = rotated_corners(rect, rotation)[0];
    let center_offset = rotate_around(size * 0.5, Vec2::ZERO, rotation);
    thinoda_core::Rect::from_center_size(anchor + center_offset, size)
}

fn angle(point: Vec2, center: Vec2) -> f32 {
    (point.y - center.y).atan2(point.x - center.x).to_degrees()
}

fn set_rotation(doc: &mut Document, item: SceneId, degrees: f32) -> EditResult<()> {
    match item {
        SceneId::Object(id) => doc.rotate_canvas_object(id, degrees),
        SceneId::Topic(id) => {
            let mut card = doc
                .sheet()
                .topic(id)
                .and_then(|topic| topic.card)
                .ok_or(EditError::NoSuchTopic(id))?;
            card.rotation = thinoda_core::board::normalize_rotation(degrees);
            doc.set_topic_card(id, Some(card))
        }
        SceneId::Zone(_) => Ok(()),
    }
}

fn set_endpoint(
    doc: &mut Document,
    id: CanvasObjectId,
    start: bool,
    endpoint: Endpoint,
    port: Option<ConnectorPort>,
) -> EditResult<()> {
    if !board_actions::member_is_movable(doc.sheet(), BoardMember::Object(id)) {
        return Err(EditError::Invalid("connector is locked".into()));
    }
    doc.update_canvas_object("Reconnect connector", id, |object| {
        if let CanvasObjectKind::Connector(connector) = &mut object.kind {
            if start {
                connector.from = endpoint;
                connector.from_port = port;
            } else {
                connector.to = endpoint;
                connector.to_port = port;
            }
        }
    })
}

fn connector_end(
    sheet: &Sheet,
    id: CanvasObjectId,
    start: bool,
) -> Option<(Endpoint, Option<ConnectorPort>)> {
    let CanvasObjectKind::Connector(connector) = &sheet.canvas.object(id)?.kind else {
        return None;
    };
    Some(if start {
        (connector.from, connector.from_port)
    } else {
        (connector.to, connector.to_port)
    })
}

fn port_target(sheet: &Sheet, scene: &Scene, end: Endpoint) -> Option<(thinoda_core::Rect, f32)> {
    match end {
        Endpoint::Object(id) => {
            let object = sheet.canvas.object(id)?;
            matches!(object.kind, CanvasObjectKind::Shape { .. })
                .then_some((object.rect, object.rotation))
        }
        Endpoint::Topic(mut id) => loop {
            if sheet.is_anchor(id) {
                return None;
            }
            if let Some(SceneItem {
                geometry: SceneGeometry::Rect { rect, rotation },
                ..
            }) = scene.item(SceneId::Topic(id))
            {
                return Some((*rect, *rotation));
            }
            id = sheet.topic(id)?.parent?;
        },
        Endpoint::Point(_) => None,
    }
}

fn fixed_port_at(
    sheet: &Sheet,
    scene: &Scene,
    end: Endpoint,
    point: Vec2,
    previous: Option<ConnectorPort>,
) -> EditResult<Option<ConnectorPort>> {
    if matches!(end, Endpoint::Point(_)) {
        return Ok(None);
    }
    let (rect, rotation) = port_target(sheet, scene, end).ok_or_else(|| {
        EditError::Invalid("fixed connector ports require a shape or topic".into())
    })?;
    let local = rotate_around(point, rect.center(), -rotation);
    let old_offset = previous.map_or(Vec2::ZERO, |port| port.offset);
    let position_axis = |value: f32, min: f32, size: f32, offset: f32| {
        if size.abs() <= f32::EPSILON {
            0.5
        } else {
            ((value - min - offset) / size).clamp(0.0, 1.0)
        }
    };
    let position = Vec2::new(
        position_axis(local.x, rect.min.x, rect.size.x, old_offset.x),
        position_axis(local.y, rect.min.y, rect.size.y, old_offset.y),
    );
    let port = ConnectorPort {
        position,
        offset: Vec2::new(
            local.x - rect.min.x - rect.size.x * position.x,
            local.y - rect.min.y - rect.size.y * position.y,
        ),
        // A direct move pins the visible point rather than projecting it again.
        perimeter: false,
    };
    if !port.is_valid() {
        return Err(EditError::Invalid(
            "fixed connector port exceeds supported limits".into(),
        ));
    }
    Ok(Some(port))
}

fn toggle_fixed_endpoint(
    doc: &mut Document,
    view: &mut MapView,
    scene: &Scene,
    control: Control,
    out: &mut CanvasOutput,
) {
    let SceneId::Object(id) = control.item else {
        return;
    };
    let start = control.kind == ControlKind::From;
    let Some((end, current)) = connector_end(doc.sheet(), id, start) else {
        return;
    };
    if matches!(end, Endpoint::Point(_)) || !prepare_edit(doc, view, out) {
        return;
    }
    let port = if current.is_some() {
        Ok(None)
    } else {
        fixed_port_at(doc.sheet(), scene, end, control.point, None)
    };
    if let Err(error) = port.and_then(|port| set_endpoint(doc, id, start, end, port)) {
        out.error = Some(error);
    }
}

enum WaypointEdit {
    Insert(usize, Vec2),
    Move(usize, Vec2),
    Remove(usize),
}

fn edit_waypoint(doc: &mut Document, id: CanvasObjectId, edit: WaypointEdit) -> EditResult<bool> {
    if !board_actions::member_is_movable(doc.sheet(), BoardMember::Object(id)) {
        return Err(EditError::Invalid("connector is locked".into()));
    }
    let Some(CanvasObjectKind::Connector(connector)) =
        doc.sheet().canvas.object(id).map(|object| &object.kind)
    else {
        return Err(EditError::Invalid("connector no longer exists".into()));
    };
    let mut points = connector.waypoints.to_vec();
    match edit {
        WaypointEdit::Insert(index, point) if index <= points.len() => points.insert(index, point),
        WaypointEdit::Move(index, point) if index < points.len() => points[index] = point,
        WaypointEdit::Remove(index) if index < points.len() => {
            points.remove(index);
        }
        _ => {
            return Err(EditError::Invalid(
                "connector waypoint no longer exists".into(),
            ));
        }
    }
    if points.as_slice() == connector.waypoints.as_ref() {
        return Ok(false);
    }
    doc.update_canvas_object("Edit connector waypoints", id, |object| {
        if let CanvasObjectKind::Connector(connector) = &mut object.kind {
            connector.waypoints = points.into();
        }
    })?;
    Ok(true)
}

/// What a direction-arrow click does, following draw.io's `plusTooltip`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ArrowMode {
    /// Clone and connect, or connect to a neighbour already in that place.
    Chain,
    /// Ctrl/Cmd: clone without a connector.
    CloneOnly,
    /// Shift: connector only.
    ConnectOnly,
}

impl ArrowMode {
    fn from_modifiers(modifiers: Modifiers) -> Self {
        if modifiers.command {
            Self::CloneOnly
        } else if modifiers.shift {
            Self::ConnectOnly
        } else {
            Self::Chain
        }
    }
}

/// Unlabelled, unlocked copy of `source` centred on `center`.
fn arrow_clone(source: &CanvasObject, center: Vec2) -> CanvasObject {
    let mut clone = source.clone();
    clone.rect = thinoda_core::Rect::from_center_size(center, source.rect.size);
    clone.locked = false;
    clone.alt_text.clear();
    if let CanvasObjectKind::Shape { label, .. } = &mut clone.kind {
        label.clear();
    }
    clone
}

/// Topic or shape under `point`, other than `source`.
fn arrow_neighbour(
    sheet: &Sheet,
    scene: &Scene,
    point: Vec2,
    source: CanvasObjectId,
) -> Option<Endpoint> {
    let mut scene = scene.clone();
    scene.items.retain(|item| match item.id {
        SceneId::Topic(id) => !sheet.is_anchor(id),
        SceneId::Object(id) => {
            id != source
                && sheet
                    .canvas
                    .object(id)
                    .is_some_and(|object| matches!(object.kind, CanvasObjectKind::Shape { .. }))
        }
        SceneId::Zone(_) => false,
    });
    Some(match scene.hit_test(point)? {
        SceneHit::Topic(id) => Endpoint::Topic(id),
        SceneHit::Object(id) => Endpoint::Object(id),
    })
}

/// Offset from `source` to the first free clone position in `dir`.
fn clone_offset(sheet: &Sheet, scene: &Scene, source: &CanvasObject, dir: ArrowDir) -> Vec2 {
    let bounds = source.rotated_bounds();
    let step = dir.unit() * (dir.extent(bounds) + ARROW_GAP);
    let mut offset = step;
    for _ in 0..20 {
        let target = bounds.translate(offset).expand(8.0);
        let blocked = scene.items.iter().any(|item| {
            let solid = match item.id {
                SceneId::Topic(_) => true,
                SceneId::Object(id) => {
                    id != source.id
                        && sheet
                            .canvas
                            .object(id)
                            .is_some_and(|object| !object.is_connector())
                }
                SceneId::Zone(_) => false,
            };
            solid && item.bounds.intersects(&target)
        });
        if !blocked {
            break;
        }
        offset = offset + step;
    }
    offset
}

fn select_endpoint(view: &mut MapView, sheet: &Sheet, endpoint: Endpoint) {
    match endpoint {
        Endpoint::Topic(id) => select_hit(view, sheet, SceneHit::Topic(id), false),
        Endpoint::Object(id) => select_hit(view, sheet, SceneHit::Object(id), false),
        Endpoint::Point(_) => {}
    }
}

/// Click (or Enter) on a direction arrow; selects the result so users can keep chaining.
fn arrow_click(
    doc: &mut Document,
    view: &mut MapView,
    scene: &Scene,
    source: CanvasObjectId,
    dir: ArrowDir,
    mode: ArrowMode,
) -> EditResult<bool> {
    let Some(object) = arrow_source(doc.sheet(), source).cloned() else {
        return Ok(false);
    };
    let bounds = object.rotated_bounds();
    let center = bounds.center();
    let from = Endpoint::Object(source);
    let neighbour = if mode == ArrowMode::CloneOnly {
        None
    } else {
        let step = dir.unit() * (dir.extent(bounds) + ARROW_GAP);
        arrow_neighbour(doc.sheet(), scene, center + step, source)
    };
    match (mode, neighbour) {
        (ArrowMode::ConnectOnly, to) => {
            let to =
                to.unwrap_or_else(|| Endpoint::Point(dir.side(bounds) + dir.unit() * ARROW_GAP));
            let edge =
                doc.add_canvas_object(CanvasObject::connector(CanvasConnector::new(from, to)))?;
            select_hit(view, doc.sheet(), SceneHit::Object(edge), false);
        }
        (_, Some(to)) => {
            doc.add_canvas_object(CanvasObject::connector(CanvasConnector::new(from, to)))?;
            select_endpoint(view, doc.sheet(), to);
        }
        (mode, None) => {
            let offset = clone_offset(doc.sheet(), scene, &object, dir);
            let clone = arrow_clone(&object, center + offset);
            let id = if mode == ArrowMode::CloneOnly {
                doc.add_canvas_object(clone)?
            } else {
                doc.add_connected_canvas_object(
                    clone,
                    CanvasConnector::new(from, Endpoint::Point(center)),
                )?
                .0
            };
            select_hit(view, doc.sheet(), SceneHit::Object(id), false);
        }
    }
    Ok(true)
}

/// Drag from a direction arrow released at `point`: connect to the item there,
/// or leave a free end; Ctrl/Cmd over empty space drops a connected clone.
fn arrow_drop(
    doc: &mut Document,
    view: &mut MapView,
    scene: &Scene,
    source: CanvasObjectId,
    point: Vec2,
    modifiers: Modifiers,
) -> EditResult<bool> {
    let Some(object) = arrow_source(doc.sheet(), source).cloned() else {
        return Ok(false);
    };
    if object.rotated_bounds().contains(point) {
        return Ok(false);
    }
    let from = Endpoint::Object(source);
    match endpoint_at(doc.sheet(), scene, point, Some(source)) {
        Endpoint::Point(center) if modifiers.command => {
            let (id, _) = doc.add_connected_canvas_object(
                arrow_clone(&object, center),
                CanvasConnector::new(from, Endpoint::Point(center)),
            )?;
            select_hit(view, doc.sheet(), SceneHit::Object(id), false);
        }
        to => {
            let edge =
                doc.add_canvas_object(CanvasObject::connector(CanvasConnector::new(from, to)))?;
            select_hit(view, doc.sheet(), SceneHit::Object(edge), false);
        }
    }
    Ok(true)
}

fn quick_popup_id() -> Id {
    MapCanvas::id().with("quick-shapes")
}

fn quick_tile_id(index: usize) -> Id {
    quick_popup_id().with(("tile", index))
}

#[allow(clippy::too_many_arguments)]
fn open_quick(
    ctx: &egui::Context,
    doc: &Document,
    view: &mut MapView,
    source: CanvasObjectId,
    dir: ArrowDir,
    anchor: Rect,
    hover: bool,
    return_focus: Option<Id>,
) {
    view.board_input.quick = Some(QuickShapes {
        source,
        dir,
        anchor,
        hover,
        return_focus,
        focus_first: return_focus.is_some(),
        preview: None,
        state: doc.state_id(),
        sheet: doc.active_index(),
        sheet_identity: sheet_identity(doc),
        camera: (view.offset, view.zoom, view.canvas_rect),
    });
    view.board_input.arrow_hover = Some(((SceneId::Object(source), dir), None));
    egui::Popup::open_id(ctx, quick_popup_id());
    ctx.request_repaint();
}

fn close_quick(ctx: &egui::Context, view: &mut MapView) {
    if let Some(quick) = view.board_input.quick.take() {
        // Resting on the same arrow must not reopen it at once.
        view.board_input.arrow_hover = Some(((SceneId::Object(quick.source), quick.dir), None));
        egui::Popup::close_id(ctx, quick_popup_id());
    }
}

/// The source's own kind first, then common draw.io shapes.
fn quick_shapes(source: ShapeKind) -> Vec<ShapeKind> {
    let mut list = vec![source];
    list.extend(QUICK_SHAPES.into_iter().filter(|shape| *shape != source));
    list.truncate(QUICK_SHAPES.len());
    list
}

/// Where a connected shape added from `dir` lands, as a rect.
fn quick_target(
    sheet: &Sheet,
    scene: &Scene,
    source: &CanvasObject,
    dir: ArrowDir,
) -> thinoda_core::Rect {
    let center = source.rotated_bounds().center() + clone_offset(sheet, scene, source, dir);
    thinoda_core::Rect::from_center_size(center, source.rect.size)
}

/// Adds a `shape` copy of `source` connected in `dir`, as one undoable edit.
fn quick_insert(
    doc: &mut Document,
    view: &mut MapView,
    scene: &Scene,
    source: CanvasObjectId,
    dir: ArrowDir,
    shape: ShapeKind,
) -> EditResult<bool> {
    let Some(object) = arrow_source(doc.sheet(), source).cloned() else {
        return Ok(false);
    };
    let target = quick_target(doc.sheet(), scene, &object, dir);
    let mut clone = arrow_clone(&object, target.center());
    if let CanvasObjectKind::Shape { shape: kind, .. } = &mut clone.kind {
        if *kind != shape {
            clone.rounding = None;
            clone.rectangle_sides = None;
            clone.double_ellipse_margin = None;
        }
        *kind = shape;
    }
    let center = object.rotated_bounds().center();
    let (id, _) = doc.add_connected_canvas_object(
        clone,
        CanvasConnector::new(Endpoint::Object(source), Endpoint::Point(center)),
    )?;
    select_hit(view, doc.sheet(), SceneHit::Object(id), false);
    Ok(true)
}

#[derive(Default)]
struct QuickResult {
    chosen: Option<ShapeKind>,
    preview: Option<ShapeKind>,
    focused: bool,
}

fn quick_content(
    ui: &mut Ui,
    title: &str,
    hint: &str,
    tiles: &[(ShapeKind, String)],
    focus_first: bool,
    accent: Color32,
) -> QuickResult {
    let gap = 4.0;
    let width = QUICK_COLUMNS as f32 * (QUICK_TILE + gap) - gap;
    ui.set_max_width(width);
    ui.label(egui::RichText::new(title).strong());
    let mut result = QuickResult::default();
    let count = tiles.len();
    let focused = ui.memory(|memory| (0..count).find(|i| memory.has_focus(quick_tile_id(*i))));
    if let Some(index) = focused {
        result.focused = true;
        ui.memory_mut(|memory| {
            memory.set_focus_lock_filter(
                quick_tile_id(index),
                EventFilter {
                    tab: true,
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    escape: false,
                },
            )
        });
        let next = ui.input_mut(|input| {
            if input.consume_key(Modifiers::SHIFT, Key::Tab)
                || input.consume_key(Modifiers::NONE, Key::ArrowLeft)
            {
                Some((index + count - 1) % count)
            } else if input.consume_key(Modifiers::NONE, Key::Tab)
                || input.consume_key(Modifiers::NONE, Key::ArrowRight)
            {
                Some((index + 1) % count)
            } else if input.consume_key(Modifiers::NONE, Key::ArrowDown) {
                Some((index + QUICK_COLUMNS).min(count - 1))
            } else if input.consume_key(Modifiers::NONE, Key::ArrowUp) {
                Some(index.saturating_sub(QUICK_COLUMNS))
            } else {
                None
            }
        });
        if let Some(next) = next {
            ui.memory_mut(|memory| memory.request_focus(quick_tile_id(next)));
        }
    } else if focus_first && count > 0 {
        ui.memory_mut(|memory| memory.request_focus(quick_tile_id(0)));
    }
    for (row, chunk) in tiles.chunks(QUICK_COLUMNS).enumerate() {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for (column, (shape, name)) in chunk.iter().enumerate() {
                let index = row * QUICK_COLUMNS + column;
                let (_, rect) = ui.allocate_space(vec2(QUICK_TILE, QUICK_TILE));
                let response = ui.interact(rect, quick_tile_id(index), Sense::click());
                response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name));
                let hot = response.hovered() || response.has_focus();
                let visuals = ui.style().interact(&response);
                let fill = if hot {
                    visuals.bg_fill
                } else {
                    ui.visuals().faint_bg_color
                };
                ui.painter().rect(
                    rect.shrink(0.5),
                    6.0,
                    fill,
                    ui.visuals().widgets.noninteractive.bg_stroke,
                    StrokeKind::Inside,
                );
                if response.has_focus() {
                    ui.painter().rect_stroke(
                        rect.shrink(2.0),
                        5.0,
                        Stroke::new(2.0, accent),
                        StrokeKind::Inside,
                    );
                }
                let icon = Rect::from_center_size(
                    rect.center(),
                    vec2(QUICK_TILE * 0.62, QUICK_TILE * 0.5),
                );
                let point = |p: Vec2| icon.min + vec2(p.x * icon.width(), p.y * icon.height());
                let stroke = Stroke::new(1.5, ui.visuals().text_color());
                ui.painter().add(egui::Shape::closed_line(
                    shape.unit_outline().into_iter().map(point).collect(),
                    stroke,
                ));
                for detail in shape.unit_details() {
                    ui.painter().add(egui::Shape::line(
                        detail.into_iter().map(point).collect(),
                        stroke,
                    ));
                }
                if hot {
                    result.preview = Some(*shape);
                }
                if response.on_hover_text(name).clicked() {
                    result.chosen = Some(*shape);
                }
            }
        });
    }
    ui.add(egui::Label::new(egui::RichText::new(hint).small().weak()).wrap());
    result
}

/// Shows the open shape picker and applies its choice; returns whether it
/// consumed this frame's board input.
fn show_quick(
    ui: &mut Ui,
    doc: &mut Document,
    view: &mut MapView,
    scene: &Scene,
    out: &mut CanvasOutput,
) -> bool {
    let ctx = ui.ctx().clone();
    let Some(quick) = view.board_input.quick.as_ref() else {
        return false;
    };
    let kind = arrow_source(doc.sheet(), quick.source).and_then(|object| match object.kind {
        CanvasObjectKind::Shape { shape, .. } => Some(shape),
        _ => None,
    });
    let stale = quick.state != doc.state_id()
        || quick.sheet != doc.active_index()
        || quick.sheet_identity != sheet_identity(doc)
        || quick.camera != (view.offset, view.zoom, view.canvas_rect)
        || !egui::Popup::is_id_open(&ctx, quick_popup_id())
        || !matches!(view.board_tool, BoardTool::Select | BoardTool::Shape(_))
        || view.editing.is_some()
        || view.editing_board_object().is_some()
        || !ui.is_enabled();
    let Some(kind) = kind.filter(|_| !stale) else {
        close_quick(&ctx, view);
        return false;
    };
    let (source, dir, anchor, hover, return_focus, focus_first) = (
        quick.source,
        quick.dir,
        quick.anchor,
        quick.hover,
        quick.return_focus,
        quick.focus_first,
    );
    let title = control_label(view, ControlKind::Arrow(dir));
    let hint = format!(
        "{}\n{}",
        view.board_labels.connect_hint, view.board_labels.quick_hint
    );
    let tiles: Vec<_> = quick_shapes(kind)
        .into_iter()
        .map(|shape| {
            let name = view
                .board_labels
                .shape_names
                .get(&shape)
                .cloned()
                .unwrap_or_else(|| view.board_labels.shape.clone());
            (shape, name)
        })
        .collect();
    let align = match dir {
        ArrowDir::Up => egui::RectAlign::TOP,
        ArrowDir::Right => egui::RectAlign::RIGHT,
        ArrowDir::Down => egui::RectAlign::BOTTOM,
        ArrowDir::Left => egui::RectAlign::LEFT,
    };
    if let Some(origin) = ctx.input(|input| {
        input
            .pointer
            .primary_pressed()
            .then(|| input.pointer.press_origin())
            .flatten()
            .filter(|origin| anchor.contains(*origin))
    }) {
        // The arrow stays live under its picker: a press starts the usual
        // click/drag gesture, as in draw.io.
        close_quick(&ctx, view);
        if let Some(object) = arrow_source(doc.sheet(), source) {
            let start = view.xf().sheet(origin);
            let kind = GestureKind::Arrow {
                source,
                dir,
                anchor: dir.side(object.rotated_bounds()),
                current: start,
                since: ctx.input(|input| input.time),
                far: false,
            };
            view.board_input.gesture = Some(gesture(doc, view, kind, start));
        }
        return true;
    }
    let accent = view.accent;
    let shown = egui::Popup::new(quick_popup_id(), ctx.clone(), anchor, ui.layer_id())
        .kind(egui::PopupKind::Popup)
        .open_memory(None)
        .align(align)
        .gap(6.0)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .accessible_name(title.clone())
        .show(|ui| quick_content(ui, &title, &hint, &tiles, focus_first, accent));
    let Some(shown) = shown else {
        close_quick(&ctx, view);
        return false;
    };
    let result = shown.inner;
    if let Some(shape) = result.chosen {
        close_quick(&ctx, view);
        if prepare_edit(doc, view, out) {
            match quick_insert(doc, view, scene, source, dir, shape) {
                Ok(true) if return_focus.is_some() => {
                    focus_next_arrow(ui, view, doc.sheet(), scene, dir);
                }
                Ok(_) => {}
                Err(error) => out.error = Some(error),
            }
        }
        return false;
    }
    if !egui::Popup::is_id_open(&ctx, quick_popup_id()) {
        close_quick(&ctx, view);
        if ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) {
            if let Some(id) = return_focus {
                ctx.memory_mut(|memory| memory.request_focus(id));
            }
        } else if ctx.input(|input| input.pointer.any_down()) {
            view.board_input.suppress_until_release = true;
        }
        // The input that closed the picker must not also act on the board.
        return true;
    }
    let rect = shown.response.rect;
    if hover
        && !result.focused
        && !ctx
            .pointer_hover_pos()
            .is_some_and(|pointer| anchor.union(rect).expand(12.0).contains(pointer))
    {
        close_quick(&ctx, view);
        if ctx.input(|input| input.pointer.any_down()) {
            // A press dragged out of the picker must not reach the board.
            view.board_input.suppress_until_release = true;
            return true;
        }
        return false;
    }
    if let Some(quick) = view.board_input.quick.as_mut() {
        quick.preview = result.preview;
        quick.focus_first &= !result.focused;
    }
    true
}

fn arrow(ui: &Ui, accept: impl Fn(Vec2, Modifiers) -> bool) -> Option<(Vec2, Modifiers)> {
    for (modifiers, step) in [
        (Modifiers::COMMAND.plus(Modifiers::SHIFT), 10.0),
        (Modifiers::COMMAND, 1.0),
        (Modifiers::SHIFT, 10.0),
        (Modifiers::NONE, 1.0),
    ] {
        for (key, direction) in [
            (Key::ArrowLeft, Vec2::new(-step, 0.0)),
            (Key::ArrowRight, Vec2::new(step, 0.0)),
            (Key::ArrowUp, Vec2::new(0.0, -step)),
            (Key::ArrowDown, Vec2::new(0.0, step)),
        ] {
            if accept(direction, modifiers)
                && ui.input_mut(|input| input.consume_key(modifiers, key))
            {
                return Some((direction, modifiers));
            }
        }
    }
    None
}

fn alt_arrow_pressed(ui: &Ui) -> bool {
    ui.input(|input| {
        input.events.iter().any(|event| {
            matches!(
                event,
                egui::Event::Key {
                    key: Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.alt
            )
        })
    })
}

fn activate_waypoint_insertion(
    ui: &Ui,
    doc: &mut Document,
    view: &mut MapView,
    out: &mut CanvasOutput,
    control: Control,
) {
    let (ControlKind::InsertWaypoint(index, _), SceneId::Object(id)) = (control.kind, control.item)
    else {
        return;
    };
    if prepare_edit(doc, view, out) {
        match edit_waypoint(doc, id, WaypointEdit::Insert(index, control.point)) {
            Ok(true) => ui.ctx().memory_mut(|memory| {
                memory.request_focus(control_kind_id(
                    control.item,
                    ControlKind::Waypoint(index),
                    false,
                ))
            }),
            Ok(false) => {}
            Err(error) => out.error = Some(error),
        }
    }
}

fn keyboard_control(
    ui: &Ui,
    doc: &mut Document,
    view: &mut MapView,
    layout: &Layout,
    scene: &Scene,
    control: Control,
    out: &mut CanvasOutput,
) {
    ui.ctx().memory_mut(|memory| {
        memory.set_focus_lock_filter(
            control_id(control),
            EventFilter {
                tab: true,
                horizontal_arrows: true,
                vertical_arrows: true,
                escape: true,
            },
        )
    });
    for (modifiers, reverse) in [(Modifiers::SHIFT, true), (Modifiers::NONE, false)] {
        if ui.input_mut(|input| input.consume_key(modifiers, Key::Tab)) {
            let controls = tab_controls(view, doc.sheet(), layout, scene);
            if let Some(index) = controls
                .iter()
                .position(|candidate| control_id(*candidate) == control_id(control))
            {
                if reverse && index == 0 {
                    ui.ctx()
                        .memory_mut(|memory| memory.request_focus(item_id(control.item)));
                } else if reverse || index + 1 < controls.len() {
                    let next = if reverse { index - 1 } else { index + 1 };
                    ui.ctx()
                        .memory_mut(|memory| memory.request_focus(control_id(controls[next])));
                } else {
                    cycle_selection(ui, doc, view, layout, scene, false);
                }
            }
            return;
        }
    }
    if ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) {
        ui.ctx()
            .memory_mut(|memory| memory.request_focus(MapCanvas::id()));
        return;
    }
    if matches!(
        control.kind,
        ControlKind::Waypoint(_) | ControlKind::InsertWaypoint(_, _)
    ) && ui.input_mut(|input| {
        input.consume_key(Modifiers::NONE, Key::Delete)
            || input.consume_key(Modifiers::NONE, Key::Backspace)
    }) {
        if let (ControlKind::Waypoint(index), SceneId::Object(id)) = (control.kind, control.item)
            && prepare_edit(doc, view, out)
        {
            match edit_waypoint(doc, id, WaypointEdit::Remove(index)) {
                Ok(true) => ui
                    .ctx()
                    .memory_mut(|memory| memory.request_focus(item_id(control.item))),
                Ok(false) => {}
                Err(error) => out.error = Some(error),
            }
        }
        return;
    }
    if matches!(control.kind, ControlKind::InsertWaypoint(_, _))
        && ui.input_mut(|input| {
            input.consume_key(Modifiers::NONE, Key::Enter)
                || input.consume_key(Modifiers::NONE, Key::Space)
        })
    {
        activate_waypoint_insertion(ui, doc, view, out, control);
        return;
    }
    if matches!(control.kind, ControlKind::From | ControlKind::To)
        && ui.input_mut(|input| {
            input.consume_key(Modifiers::NONE, Key::Enter)
                || input.consume_key(Modifiers::NONE, Key::Space)
        })
    {
        toggle_fixed_endpoint(doc, view, scene, control, out);
        return;
    }
    // Let the canvas own Alt+arrow duplication/rotation even when a handle is
    // focused. `consume_key` otherwise accepts extra Alt modifiers.
    if alt_arrow_pressed(ui) {
        return;
    }
    if let ControlKind::Arrow(dir) = control.kind {
        let SceneId::Object(source) = control.item else {
            return;
        };
        if ui.input_mut(|input| input.consume_key(Modifiers::SHIFT, Key::F10)) {
            let anchor = arrow_rect(view, control.point);
            let focus = Some(control_id(control));
            open_quick(ui.ctx(), doc, view, source, dir, anchor, false, focus);
            return;
        }
        // Enter/Space act like a click; modifiers pick the same variants.
        for modifiers in [Modifiers::COMMAND, Modifiers::SHIFT, Modifiers::NONE] {
            if ui.input_mut(|input| {
                input.consume_key(modifiers, Key::Enter) || input.consume_key(modifiers, Key::Space)
            }) {
                if prepare_edit(doc, view, out) {
                    let mode = ArrowMode::from_modifiers(modifiers);
                    match arrow_click(doc, view, scene, source, dir, mode) {
                        Ok(true) => focus_next_arrow(ui, view, doc.sheet(), scene, dir),
                        Ok(false) => {}
                        Err(error) => out.error = Some(error),
                    }
                }
                return;
            }
        }
        return;
    }
    let Some((delta, modifiers)) = arrow(ui, |_, _| true) else {
        return;
    };
    let result = match control.kind {
        ControlKind::Resize => apply_board_action(doc, view, BoardAction::Resize(delta)),
        ControlKind::Rotate => {
            let sign = (delta.x + delta.y).signum();
            let amount = if modifiers.shift { 15.0 } else { 1.0 };
            apply_board_action(doc, view, BoardAction::Rotate(sign * amount))
        }
        ControlKind::From | ControlKind::To => {
            let SceneId::Object(id) = control.item else {
                return;
            };
            let start = control.kind == ControlKind::From;
            if !modifiers.command
                && let Some((end, Some(previous))) = connector_end(doc.sheet(), id, start)
            {
                if !prepare_edit(doc, view, out) {
                    return;
                }
                let result = fixed_port_at(
                    doc.sheet(),
                    scene,
                    end,
                    control.point + delta,
                    Some(previous),
                )
                .and_then(|port| set_endpoint(doc, id, start, end, port));
                if let Err(error) = result {
                    out.error = Some(error);
                }
                return;
            }
            let endpoint = if modifiers.command {
                let choices: Vec<_> = scene
                    .items
                    .iter()
                    .filter_map(|item| match item.id {
                        SceneId::Topic(topic) if !doc.sheet().is_anchor(topic) => {
                            Some(Endpoint::Topic(topic))
                        }
                        SceneId::Object(object) if object != id => Some(Endpoint::Object(object)),
                        _ => None,
                    })
                    .collect();
                if choices.is_empty() {
                    Endpoint::Point(control.point + delta)
                } else {
                    let current = doc.sheet().canvas.object(id).and_then(|object| {
                        if let CanvasObjectKind::Connector(connector) = &object.kind {
                            Some(if control.kind == ControlKind::From {
                                connector.from
                            } else {
                                connector.to
                            })
                        } else {
                            None
                        }
                    });
                    let old =
                        current.and_then(|end| choices.iter().position(|choice| *choice == end));
                    let step = if delta.x + delta.y > 0.0 { 1 } else { -1 };
                    let index = old.map_or(if step > 0 { 0 } else { choices.len() - 1 }, |old| {
                        (old as isize + step).rem_euclid(choices.len() as isize) as usize
                    });
                    choices[index]
                }
            } else {
                Endpoint::Point(control.point + delta)
            };
            set_endpoint(doc, id, start, endpoint, None).map(|()| true)
        }
        ControlKind::Waypoint(index) | ControlKind::InsertWaypoint(index, _) => {
            let SceneId::Object(id) = control.item else {
                return;
            };
            if !prepare_edit(doc, view, out) {
                return;
            }
            let point = control.point + delta;
            let edit = if matches!(control.kind, ControlKind::Waypoint(_)) {
                WaypointEdit::Move(index, point)
            } else {
                WaypointEdit::Insert(index, point)
            };
            let result = edit_waypoint(doc, id, edit);
            if matches!(result, Ok(true)) {
                let visible = view.canvas_rect.contains(view.xf().pos(point))
                    && scene
                        .item(control.item)
                        .and_then(|item| item.clip)
                        .is_none_or(|clip| clip.contains(point));
                ui.ctx().memory_mut(|memory| {
                    memory.request_focus(if visible {
                        control_kind_id(control.item, ControlKind::Waypoint(index), false)
                    } else {
                        item_id(control.item)
                    })
                });
            }
            result
        }
        ControlKind::Arrow(_) => return,
    };
    if let Err(error) = result {
        out.error = Some(error);
    }
}

fn interact_controls(
    ui: &mut Ui,
    doc: &mut Document,
    view: &mut MapView,
    layout: &Layout,
    scene: &Scene,
    enabled: bool,
    out: &mut CanvasOutput,
) -> bool {
    view.board_input.control_focused = false;
    if !enabled {
        if let Some((_, since)) = view.board_input.arrow_hover.as_mut() {
            *since = None;
        }
        return view.board_input.is_active();
    }
    let mut blocked = view.board_input.is_active();
    let mut arrow_hovered = false;
    let now = ui.input(|input| input.time);
    for control in controls(view, doc.sheet(), layout, scene) {
        let rect = arrow_rect(view, control.point);
        if !view.canvas_rect.intersects(rect) {
            continue;
        }
        let sense = if control.hover {
            Sense::click_and_drag()
        } else {
            Sense::click_and_drag() | Sense::FOCUSABLE
        };
        let response = ui.interact(rect, control_id(control), sense);
        let mut label = control_label(view, control.kind);
        if matches!(control.kind, ControlKind::From | ControlKind::To)
            && let SceneId::Object(id) = control.item
            && connector_end(doc.sheet(), id, control.kind == ControlKind::From)
                .is_some_and(|(_, port)| port.is_some())
        {
            label = format!("{label}, {}", view.board_labels.fixed_attachment);
        }
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
        let hint = match control.kind {
            ControlKind::From | ControlKind::To => Some(&view.board_labels.connector_end_hint),
            ControlKind::Waypoint(_) => Some(&view.board_labels.waypoint_hint),
            ControlKind::InsertWaypoint(_, _) => Some(&view.board_labels.insert_waypoint_hint),
            _ => None,
        };
        if let Some(hint) = hint {
            response.clone().on_hover_text(hint);
            ui.ctx().accesskit_node_builder(response.id, |node| {
                node.set_description(hint.as_str());
            });
        }
        if let ControlKind::Arrow(dir) = control.kind
            && let SceneId::Object(source) = control.item
            && response.hovered()
            && view.board_input.gesture.is_none()
        {
            // Resting the mouse on an arrow opens its shape picker, which
            // also explains the arrow; a press restarts only after leaving.
            let key = (control.item, dir);
            let since = match view.board_input.arrow_hover {
                Some((previous, since)) if previous == key => since,
                _ => Some(now),
            }
            .filter(|_| !ui.input(|input| input.pointer.any_down() || input.pointer.any_click()));
            view.board_input.arrow_hover = Some((key, since));
            arrow_hovered = true;
            if let Some(since) = since {
                if now - since >= QUICK_DELAY {
                    open_quick(ui.ctx(), doc, view, source, dir, rect, true, None);
                    return true;
                }
                ui.ctx()
                    .request_repaint_after_secs((QUICK_DELAY - (now - since)) as f32);
            }
        }
        blocked |= response.hovered()
            || response.is_pointer_button_down_on()
            || ui
                .input(|input| input.pointer.press_origin())
                .is_some_and(|point| rect.contains(point));
        if response.is_pointer_button_down_on()
            && ui.input(|input| input.pointer.primary_pressed())
            && view.board_input.gesture.is_none()
        {
            if !control.hover {
                response.request_focus();
            }
            let start = view.xf().sheet(
                ui.input(|input| input.pointer.press_origin())
                    .unwrap_or(rect.center()),
            );
            let kind = match control.kind {
                ControlKind::Resize => GestureKind::Resize {
                    item: control.item,
                    original: control.rect,
                    rotation: control.rotation,
                    grab: start,
                    preview: control.rect,
                },
                ControlKind::Rotate => GestureKind::Rotate {
                    item: control.item,
                    center: control.rect.center(),
                    original: control.rotation,
                    grab_angle: angle(start, control.rect.center()),
                    preview: control.rotation,
                },
                ControlKind::From | ControlKind::To => {
                    let SceneId::Object(id) = control.item else {
                        continue;
                    };
                    GestureKind::Endpoint {
                        id,
                        start: control.kind == ControlKind::From,
                        point: control.point,
                    }
                }
                ControlKind::Waypoint(index) | ControlKind::InsertWaypoint(index, _) => {
                    let SceneId::Object(id) = control.item else {
                        continue;
                    };
                    GestureKind::Waypoint {
                        id,
                        index,
                        insert: matches!(control.kind, ControlKind::InsertWaypoint(_, _)),
                        original: control.point,
                        point: control.point,
                    }
                }
                ControlKind::Arrow(dir) => {
                    let SceneId::Object(source) = control.item else {
                        continue;
                    };
                    let bounds =
                        thinoda_core::board::rotated_bounds(control.rect, control.rotation);
                    GestureKind::Arrow {
                        source,
                        dir,
                        anchor: dir.side(bounds),
                        current: start,
                        since: now,
                        far: false,
                    }
                }
            };
            view.board_input.gesture = Some(gesture(doc, view, kind, start));
            blocked = true;
        }
        if response.double_clicked()
            && let (ControlKind::Waypoint(index), SceneId::Object(id)) =
                (control.kind, control.item)
            && view
                .board_input
                .gesture
                .as_ref()
                .is_none_or(|drag| !drag.canceled && !drag.guard.changed(doc, view))
        {
            view.board_input.gesture = None;
            if prepare_edit(doc, view, out) {
                match edit_waypoint(doc, id, WaypointEdit::Remove(index)) {
                    Ok(true) => ui
                        .ctx()
                        .memory_mut(|memory| memory.request_focus(item_id(control.item))),
                    Ok(false) => {}
                    Err(error) => out.error = Some(error),
                }
            }
            return true;
        }
        if response.clicked()
            && view.board_input.gesture.is_none()
            && ui.input(|input| {
                input.has_accesskit_action_request(response.id, egui::accesskit::Action::Click)
            })
        {
            match control.kind {
                ControlKind::From | ControlKind::To => {
                    response.request_focus();
                    toggle_fixed_endpoint(doc, view, scene, control, out);
                    return true;
                }
                ControlKind::InsertWaypoint(_, _) => {
                    activate_waypoint_insertion(ui, doc, view, out, control);
                    return true;
                }
                ControlKind::Waypoint(_) => {
                    response.request_focus();
                    return true;
                }
                _ => {}
            }
        }
        // Press and release can land in one frame (e.g. synthetic taps).
        if response.clicked()
            && view.board_input.gesture.is_none()
            && let ControlKind::Arrow(dir) = control.kind
            && let SceneId::Object(source) = control.item
        {
            if prepare_edit(doc, view, out) {
                let mode = ArrowMode::from_modifiers(ui.input(|input| input.modifiers));
                match arrow_click(doc, view, scene, source, dir, mode) {
                    // Enter/Space on a focused arrow, or an assistive-technology click.
                    Ok(true) if response.has_focus() => {
                        focus_next_arrow(ui, view, doc.sheet(), scene, dir);
                    }
                    Ok(_) => {}
                    Err(error) => out.error = Some(error),
                }
            }
            return true;
        }
        if response.has_focus() && view.board_input.gesture.is_none() {
            view.board_input.control_focused = true;
            keyboard_control(ui, doc, view, layout, scene, control, out);
        }
    }
    if !arrow_hovered && view.board_input.quick.is_none() {
        view.board_input.arrow_hover = None;
    }
    blocked
}

#[allow(clippy::too_many_arguments)]
fn update_gesture(
    ui: &Ui,
    doc: &mut Document,
    view: &mut MapView,
    scene: &Scene,
    pointer: Option<Pos2>,
    enabled: bool,
    event_offset: usize,
    out: &mut CanvasOutput,
) -> bool {
    let Some(mut drag) = view.board_input.gesture.take() else {
        return false;
    };
    let ctx = ui.ctx();
    let (release, canceled_event) = ctx.input(|input| {
        let mut release = None;
        let mut canceled = false;
        for event in &input.events[event_offset..] {
            match event {
                egui::Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers,
                } => release = Some((*pos, *modifiers)),
                egui::Event::PointerGone if release.is_none() => canceled = true,
                egui::Event::WindowFocused(false)
                | egui::Event::Touch {
                    phase: egui::TouchPhase::Cancel,
                    ..
                } => canceled = true,
                _ => {}
            }
        }
        (release, canceled)
    });
    let release_modifiers = release.map_or(Modifiers::NONE, |(_, modifiers)| modifiers);
    let release = release.map(|(point, _)| point);
    let pointer = release.or(pointer);
    let changed = drag.guard.changed(doc, view);
    drag.canceled |= !enabled
        || !ui.is_enabled()
        || egui::Popup::is_any_open(ctx)
        || !ctx.input(|input| input.focused)
        || canceled_event
        || changed
        || ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape));
    if changed {
        out.error = Some(EditError::Invalid(
            "board gesture canceled because the document, camera, tool or selection changed".into(),
        ));
    }

    if let Some(point) = pointer {
        let current = view.xf().sheet(point);
        drag.moved |= (current - drag.origin).length() * view.zoom >= 2.0;
        match &mut drag.kind {
            GestureKind::Create { current: p, .. } | GestureKind::Move { current: p, .. } => {
                *p = current;
            }
            GestureKind::Resize {
                original,
                rotation,
                grab,
                preview,
                ..
            } => *preview = rotated_resize(*original, *rotation, *grab, current),
            GestureKind::Rotate {
                center,
                original,
                grab_angle,
                preview,
                ..
            } => {
                let degrees = *original + angle(current, *center) - *grab_angle;
                let degrees = if ctx.input(|input| input.modifiers.shift) {
                    (degrees / 15.0).round() * 15.0
                } else {
                    degrees
                };
                *preview = thinoda_core::board::normalize_rotation(degrees);
            }
            GestureKind::Endpoint { point: p, .. }
            | GestureKind::Arrow { current: p, .. }
            | GestureKind::Outline { current: p, .. } => {
                *p = current;
            }
            GestureKind::Waypoint {
                original, point, ..
            } => {
                *point = snapped(doc.sheet(), *original + current - drag.origin);
            }
        }
    }
    if release.is_none()
        && !drag.canceled
        && ctx.input(|input| input.pointer.primary_down())
        && let GestureKind::Arrow {
            source,
            dir,
            current,
            since,
            far,
            ..
        } = &mut drag.kind
    {
        // Pressing and holding an arrow (the touch path to the shape picker).
        *far |= (*current - drag.origin).length() * view.zoom >= ARROW_DRAG;
        if !*far {
            let now = ctx.input(|input| input.time);
            if now - *since >= QUICK_DELAY {
                let (source, dir) = (*source, *dir);
                if let Some(object) = arrow_source(doc.sheet(), source) {
                    let anchor = arrow_rect(view, arrow_point(view, object, dir));
                    open_quick(ctx, doc, view, source, dir, anchor, false, None);
                    view.board_input.suppress_until_release = true;
                }
                return true;
            }
            ctx.request_repaint_after_secs((QUICK_DELAY - (now - *since)) as f32);
        }
    }
    if release.is_some() {
        let is_arrow = matches!(drag.kind, GestureKind::Arrow { .. });
        if !drag.canceled
            && (drag.moved || is_arrow)
            && pointer.is_some_and(|point| view.canvas_rect.contains(point))
        {
            let duplicating = matches!(
                drag.kind,
                GestureKind::Move {
                    duplicate: true,
                    ..
                }
            );
            let waypoint_focus = match &drag.kind {
                GestureKind::Waypoint { id, index, .. } => Some((*id, *index)),
                _ => None,
            };
            let result = match drag.kind {
                GestureKind::Create {
                    tool,
                    start,
                    current,
                    from,
                } => {
                    if tool == BoardTool::Connector {
                        let to = endpoint_at(doc.sheet(), scene, current, None);
                        create(
                            doc,
                            view,
                            tool,
                            thinoda_core::Rect::default(),
                            Some((from, to)),
                        )
                    } else {
                        let minimum = if tool == BoardTool::Frame {
                            thinoda_core::MIN_ZONE_SIZE
                        } else {
                            MIN_CARD_SIZE
                        };
                        let rect = drag_rect(
                            snapped(doc.sheet(), start),
                            snapped(doc.sheet(), current),
                            minimum,
                        );
                        create(doc, view, tool, rect, None)
                    }
                }
                GestureKind::Move {
                    start,
                    current,
                    anchor,
                    duplicate,
                } => {
                    let delta = snapped(doc.sheet(), anchor + current - start) - anchor;
                    if duplicate {
                        board_actions::duplicate_selection(doc, view, delta)
                    } else {
                        board_actions::move_selection(doc, view, delta)
                    }
                }
                GestureKind::Resize {
                    item,
                    original,
                    preview,
                    ..
                } => {
                    if original == preview {
                        Ok(false)
                    } else {
                        board_actions::resize_item(doc, item, preview)
                    }
                }
                GestureKind::Rotate {
                    item,
                    original,
                    preview,
                    ..
                } => {
                    if original == preview {
                        Ok(false)
                    } else {
                        set_rotation(doc, item, preview).map(|()| true)
                    }
                }
                GestureKind::Endpoint { id, start, point } => {
                    let end = endpoint_at(doc.sheet(), scene, point, Some(id));
                    let port = if release_modifiers.shift {
                        fixed_port_at(doc.sheet(), scene, end, point, None)
                    } else {
                        Ok(None)
                    };
                    port.and_then(|port| set_endpoint(doc, id, start, end, port))
                        .map(|()| true)
                }
                GestureKind::Waypoint {
                    id,
                    index,
                    insert,
                    point,
                    ..
                } => {
                    let edit = if insert {
                        WaypointEdit::Insert(index, point)
                    } else {
                        WaypointEdit::Move(index, point)
                    };
                    edit_waypoint(doc, id, edit)
                }
                GestureKind::Arrow {
                    source,
                    dir,
                    current,
                    ..
                } => {
                    let modifiers = ctx.input(|input| input.modifiers);
                    if (current - drag.origin).length() * view.zoom >= ARROW_DRAG {
                        arrow_drop(doc, view, scene, source, current, modifiers)
                    } else {
                        let mode = ArrowMode::from_modifiers(modifiers);
                        arrow_click(doc, view, scene, source, dir, mode)
                    }
                }
                GestureKind::Outline {
                    source, current, ..
                } => {
                    if (current - drag.origin).length() * view.zoom >= ARROW_DRAG {
                        let modifiers = ctx.input(|input| input.modifiers);
                        arrow_drop(doc, view, scene, source, current, modifiers)
                    } else {
                        Ok(false)
                    }
                }
            };
            match result {
                Ok(true) if duplicating => focus_duplicated_selection(ui, view),
                Ok(true) if waypoint_focus.is_some() => {
                    let (id, index) = waypoint_focus.expect("checked");
                    ctx.memory_mut(|memory| {
                        memory.request_focus(control_kind_id(
                            SceneId::Object(id),
                            ControlKind::Waypoint(index),
                            false,
                        ))
                    });
                }
                Err(error) => out.error = Some(error),
                _ => {}
            }
        }
        view.board_input.connector = None;
    } else if ctx.input(|input| input.pointer.primary_down()) {
        view.board_input.gesture = Some(drag);
    }
    true
}

/// Returns whether legacy canvas pointer handling must be suppressed for this frame.
#[allow(clippy::too_many_arguments)]
pub(super) fn interact(
    ui: &mut Ui,
    doc: &mut Document,
    view: &mut MapView,
    layout: &Layout,
    scene: &Scene,
    response: &Response,
    pointer: Option<Pos2>,
    enabled: bool,
    event_offset: usize,
    out: &mut CanvasOutput,
) -> bool {
    view.observe_board_rotation_selection(doc);
    view.board_input.armed_hover = false;
    view.board_input.ghost = None;
    view.board_input.outline_hover = None;
    let ctx = ui.ctx().clone();
    if view.board_input.last_tool != view.board_tool {
        view.board_input.cancel_gesture();
        close_quick(&ctx, view);
        view.board_input.last_tool = view.board_tool;
    }
    if view.board_input.connector.as_ref().is_some_and(|pending| {
        pending.sheet != doc.active_index()
            || pending.sheet_identity != sheet_identity(doc)
            || pending.state != doc.state_id()
    }) {
        view.board_input.connector = None;
    }
    if show_quick(ui, doc, view, scene, out) {
        return true;
    }
    view.board_input.drop_ghost = None;
    if let Some(drop) = egui::DragAndDrop::payload::<ShapeDrop>(&ctx) {
        // A library shape previews where it lands and is created on release;
        // Escape cancels (egui clears the payload) and the tool is unchanged.
        let point = ctx
            .input(|input| input.pointer.latest_pos())
            .filter(|point| {
                ui.is_enabled()
                    && !view.board_input.is_active()
                    && canvas_point_unobscured(ui, response, *point)
            });
        if !ctx.input(|input| input.pointer.any_released()) {
            view.board_input.drop_ghost = point.map(|point| (drop.0, point));
        } else if let Some(point) = point {
            egui::DragAndDrop::clear_payload(&ctx);
            if prepare_edit(doc, view, out)
                && let Err(error) =
                    create_at(doc, view, BoardTool::Shape(drop.0), view.xf().sheet(point))
            {
                out.error = Some(error);
            }
            ctx.memory_mut(|memory| memory.request_focus(MapCanvas::id()));
        }
        return true;
    }
    if view.board_input.suppress_until_release {
        if !ctx.input(|input| input.pointer.primary_down()) {
            view.board_input.suppress_until_release = false;
            view.board_input.resume_after_touch_cancel = false;
        }
        return true;
    }
    if !enabled && (view.board_input.gesture.is_some() || view.board_input.creation_press.is_some())
    {
        view.board_input.cancel_gesture();
        return true;
    }
    let over_controls = interact_controls(ui, doc, view, layout, scene, enabled, out);
    update_hover_shape(view, doc.sheet(), scene, response, over_controls);
    if update_gesture(ui, doc, view, scene, pointer, enabled, event_offset, out) {
        return true;
    }
    if !enabled {
        return view.board_tool != BoardTool::Select;
    }

    let tool = view.board_tool;
    if response.drag_started()
        && (response.dragged_by(PointerButton::Middle)
            || response.dragged_by(PointerButton::Secondary))
    {
        if view.board_input.creation_press.is_some() {
            view.board_input.cancel_gesture();
        }
        view.drag = Drag::Pan;
        return true;
    }
    if tool == BoardTool::Hand {
        if response.drag_started() {
            view.drag = Drag::Pan;
        }
        return true;
    }
    let pressed_origin = ctx.input(|input| {
        input.events[event_offset..]
            .iter()
            .find_map(|event| match event {
                egui::Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    ..
                } => Some(*pos),
                _ => None,
            })
    });
    // An armed shape behaves like Select over existing content and handles;
    // only empty canvas creates. A press already creating keeps creating.
    let armed = matches!(tool, BoardTool::Shape(_));
    let on_content = |point: Pos2| {
        canvas_point_unobscured(ui, response, point)
            && armed_target(doc, view, layout, scene, view.xf().sheet(point))
    };
    let armed_idle =
        armed && view.board_input.creation_press.is_none() && view.board_input.connector.is_none();
    let armed_hover = armed_idle && (over_controls || pointer.is_some_and(on_content));
    let armed_passthrough = armed_idle
        && (over_controls
            || pressed_origin
                .or_else(|| ctx.input(|input| input.pointer.press_origin()))
                .or_else(|| response.interact_pointer_pos())
                .is_some_and(on_content));
    view.board_input.armed_hover = armed_hover;
    view.board_input.ghost = response.hover_pos().filter(|point| {
        armed_idle && !armed_hover && canvas_point_unobscured(ui, response, *point)
    });
    if (tool == BoardTool::Select || armed_idle) && !over_controls {
        view.board_input.outline_hover = response
            .hover_pos()
            .filter(|point| canvas_point_unobscured(ui, response, *point))
            .and_then(|point| outline_source(doc.sheet(), scene, view, view.xf().sheet(point)));
    }
    if tool != BoardTool::Select && !armed_passthrough {
        let mut started_here = false;
        if view.board_input.creation_press.is_none()
            && let Some(origin) = pressed_origin
            && canvas_point_unobscured(ui, response, origin)
        {
            view.board_input.creation_press = Some(CreationPress {
                guard: GestureGuard::new(doc, view),
                screen_origin: origin,
                touch: None,
                moved: false,
            });
            started_here = true;
        }
        if view
            .board_input
            .creation_press
            .as_ref()
            .is_some_and(|press| press.guard.changed(doc, view))
        {
            view.board_input.cancel_gesture();
            out.error = Some(EditError::Invalid(
                "board gesture canceled because the document, camera, tool or selection changed"
                    .into(),
            ));
            return true;
        }
        let (released, canceled) = view
            .board_input
            .creation_press
            .as_mut()
            .map_or((None, false), |press| {
                creation_input(&ctx, press, started_here, event_offset)
            });
        if canceled {
            view.board_input.cancel_gesture();
            return true;
        }
        let released_drag = released.is_some()
            && view
                .board_input
                .creation_press
                .as_ref()
                .is_some_and(|press| press.moved);
        if response.drag_started() || released_drag {
            let press = view.board_input.creation_press.take();
            let origin = ctx
                .input(|input| input.pointer.press_origin())
                .or_else(|| press.as_ref().map(|press| press.screen_origin));
            if let Some(origin) = origin
                && canvas_point_unobscured(ui, response, origin)
                && released.is_none_or(|point| canvas_point_unobscured(ui, response, point))
                && prepare_edit(doc, view, out)
            {
                let start = view.xf().sheet(origin);
                let from = view
                    .board_input
                    .connector
                    .as_ref()
                    .map_or_else(|| endpoint_at(doc.sheet(), scene, start, None), |p| p.from);
                let kind = GestureKind::Create {
                    tool,
                    start,
                    current: start,
                    from,
                };
                view.board_input.gesture = Some(gesture(doc, view, kind, start));
                update_gesture(ui, doc, view, scene, pointer, enabled, event_offset, out);
            }
        } else if response.clicked()
            && (pressed_origin.is_none() || view.board_input.creation_press.is_some())
            && released.is_none_or(|point| canvas_point_unobscured(ui, response, point))
            && let Some(point) = response.interact_pointer_pos()
            && prepare_edit(doc, view, out)
        {
            let point = view.xf().sheet(point);
            let result = if tool == BoardTool::Connector {
                let end = endpoint_at(doc.sheet(), scene, point, None);
                if let Some(pending) = view.board_input.connector.take() {
                    create(
                        doc,
                        view,
                        tool,
                        thinoda_core::Rect::default(),
                        Some((pending.from, end)),
                    )
                } else {
                    view.board_input.connector = Some(PendingConnector {
                        from: end,
                        point,
                        state: doc.state_id(),
                        sheet: doc.active_index(),
                        sheet_identity: sheet_identity(doc),
                    });
                    Ok(false)
                }
            } else {
                create_at(doc, view, tool, point)
            };
            if let Err(error) = result {
                out.error = Some(error);
            }
        }
        if released.is_some() {
            view.board_input.creation_press = None;
        }
        return true;
    }
    if over_controls {
        return true;
    }

    if response.drag_started() {
        let origin = ctx
            .input(|input| input.pointer.press_origin())
            .unwrap_or_default();
        let point = view.xf().sheet(origin);
        let hit = scene.hit_test(point);
        let board_hit = hit.filter(|hit| match *hit {
            SceneHit::Object(_) => true,
            SceneHit::Topic(id) => is_board_topic(doc.sheet(), view, id),
        });
        let modifiers = ctx.input(|input| input.modifiers);
        if hit.is_none()
            && let Some(source) = outline_source(doc.sheet(), scene, view, point)
        {
            if !prepare_edit(doc, view, out) {
                return true;
            }
            let kind = GestureKind::Outline {
                source,
                anchor: point,
                current: point,
            };
            view.board_input.gesture = Some(gesture(doc, view, kind, point));
            update_gesture(ui, doc, view, scene, pointer, enabled, event_offset, out);
            return true;
        }
        if let Some(hit) = board_hit {
            if !prepare_edit(doc, view, out) {
                return true;
            }
            if !selected(view, hit) {
                select_hit(view, doc.sheet(), hit, modifiers.command || modifiers.shift);
            }
            let anchor = match hit {
                SceneHit::Topic(id) => doc
                    .sheet()
                    .topic(id)
                    .and_then(|topic| topic.position)
                    .unwrap_or(point),
                SceneHit::Object(id) => doc
                    .sheet()
                    .canvas
                    .object(id)
                    .map_or(point, |object| object.rect.center()),
            };
            let kind = GestureKind::Move {
                start: point,
                current: point,
                anchor,
                duplicate: modifiers.alt,
            };
            view.board_input.gesture = Some(gesture(doc, view, kind, point));
            update_gesture(ui, doc, view, scene, pointer, enabled, event_offset, out);
            return true;
        }
        // Frames use the legacy zone hit test. Capture only board-frame
        // duplication here, leaving ordinary zone dragging and resizing alone.
        if modifiers.alt
            && hit.is_none()
            && let Some(ObjectSelection::Zone(id)) = hit_zone(doc.sheet(), point, 6.0 / view.zoom)
                .filter(|_| hit_object(doc.sheet(), layout, point, 6.0 / view.zoom).is_none())
            && let Some(frame) = doc
                .sheet()
                .zones
                .iter()
                .find(|zone| {
                    zone.id == id && (!doc.sheet().root_mode.is_central() || zone.keep_empty)
                })
                .map(|zone| zone.frame)
        {
            if !prepare_edit(doc, view, out) {
                return true;
            }
            view.select_object(ObjectSelection::Zone(id));
            let kind = GestureKind::Move {
                start: point,
                current: point,
                anchor: frame.min,
                duplicate: true,
            };
            view.board_input.gesture = Some(gesture(doc, view, kind, point));
            update_gesture(ui, doc, view, scene, pointer, enabled, event_offset, out);
            return true;
        }
    }

    if (response.clicked() || response.secondary_clicked())
        && let Some(point) = response.interact_pointer_pos()
        && let Some(SceneHit::Object(id)) = scene.hit_test(view.xf().sheet(point))
    {
        if !prepare_edit(doc, view, out) {
            return true;
        }
        let hit = SceneHit::Object(id);
        let modifiers = ctx.input(|input| input.modifiers);
        if response.secondary_clicked() {
            if !selected(view, hit) {
                select_hit(view, doc.sheet(), hit, false);
            }
        } else if response.double_clicked() {
            if !selected(view, hit) {
                select_hit(view, doc.sheet(), hit, false);
            }
            view.begin_board_label(doc, id);
        } else {
            select_hit(view, doc.sheet(), hit, modifiers.command || modifiers.shift);
        }
        return true;
    }
    false
}

fn item_id(id: SceneId) -> Id {
    MapCanvas::id().with(("board-item", id))
}

fn focus_duplicated_selection(ui: &Ui, view: &mut MapView) {
    let item = view
        .board_selection
        .last()
        .copied()
        .map(SceneId::Object)
        .or_else(|| view.selection.last().copied().map(SceneId::Topic))
        .or(match view.selected_object {
            Some(ObjectSelection::Zone(id)) => Some(SceneId::Zone(id)),
            _ => None,
        });
    if let Some(item) = item {
        view.board_input.focus_after_duplicate = Some(item);
        ui.ctx()
            .memory_mut(|memory| memory.request_focus(item_id(item)));
        ui.ctx().request_repaint();
    }
}

fn reading_order<'a>(sheet: &Sheet, scene: &'a Scene) -> Vec<&'a SceneItem> {
    let mut items: Vec<_> = scene
        .items
        .iter()
        .filter(|item| match item.id {
            SceneId::Topic(id) => !sheet.is_anchor(id) && sheet.is_visible(id),
            SceneId::Object(_) | SceneId::Zone(_) => true,
        })
        .filter(|item| item.clip.is_none_or(|clip| item.bounds.intersects(&clip)))
        .collect();
    items.sort_by(|a, b| {
        let a_frame = matches!(a.id, SceneId::Zone(_));
        let b_frame = matches!(b.id, SceneId::Zone(_));
        b_frame
            .cmp(&a_frame)
            .then(a.bounds.min.y.total_cmp(&b.bounds.min.y))
            .then(a.bounds.min.x.total_cmp(&b.bounds.min.x))
    });
    items
}

/// Per-pass lookups for accessibility labels and whole-group mobility.
struct AccessibilityIndex<'a> {
    objects: HashMap<CanvasObjectId, &'a CanvasObject>,
    zones: HashMap<ZoneId, &'a Zone>,
    card_states: HashMap<TopicId, &'a CanvasCardState>,
    floating: HashSet<TopicId>,
    locked_groups: HashSet<CanvasGroupId>,
    duplicate_ids: bool,
}

impl<'a> AccessibilityIndex<'a> {
    fn new(sheet: &'a Sheet) -> Self {
        let mut objects = HashMap::with_capacity(sheet.canvas.objects.len());
        for object in &sheet.canvas.objects {
            objects.entry(object.id).or_insert(object);
        }
        let mut zones = HashMap::with_capacity(sheet.zones.len());
        for zone in &sheet.zones {
            zones.entry(zone.id).or_insert(zone);
        }
        let mut card_states = HashMap::with_capacity(sheet.canvas.card_states.len());
        for state in &sheet.canvas.card_states {
            card_states.entry(state.topic).or_insert(state);
        }
        let floating: HashSet<_> = sheet.floating.iter().copied().collect();
        let groups: HashSet<_> = sheet.canvas.groups.iter().map(|group| group.id).collect();
        let duplicate_ids = objects.len() != sheet.canvas.objects.len()
            || zones.len() != sheet.zones.len()
            || card_states.len() != sheet.canvas.card_states.len()
            || floating.len() != sheet.floating.len()
            || groups.len() != sheet.canvas.groups.len();
        let mut locked_groups: HashSet<_> = sheet
            .canvas
            .groups
            .iter()
            .filter(|group| group.locked)
            .map(|group| group.id)
            .collect();
        for object in &sheet.canvas.objects {
            if object.locked
                && let Some(group) = object.group.filter(|group| groups.contains(group))
            {
                locked_groups.insert(group);
            }
        }
        for state in &sheet.canvas.card_states {
            if state.locked
                && let Some(group) = state.group.filter(|group| groups.contains(group))
                && state.topic != sheet.root
                && floating.contains(&state.topic)
                && sheet
                    .topic(state.topic)
                    .is_some_and(|topic| topic.parent.is_none() && topic.card.is_some())
            {
                locked_groups.insert(group);
            }
        }
        Self {
            objects,
            zones,
            card_states,
            floating,
            locked_groups,
            duplicate_ids,
        }
    }

    fn object_is_unlocked(&self, sheet: &Sheet, object: &CanvasObject) -> bool {
        if self.duplicate_ids {
            return board_actions::member_is_movable(sheet, BoardMember::Object(object.id));
        }
        object
            .group
            .map_or(!object.locked, |group| !self.locked_groups.contains(&group))
    }

    fn topic_is_unlocked(&self, sheet: &Sheet, id: TopicId) -> bool {
        if self.duplicate_ids {
            return topic_is_unlocked(sheet, id);
        }
        let Some(state) = self.card_states.get(&id) else {
            return true;
        };
        if id == sheet.root
            || !self.floating.contains(&id)
            || !sheet
                .topic(id)
                .is_some_and(|topic| topic.parent.is_none() && topic.card.is_some())
        {
            return true;
        }
        state
            .group
            .map_or(!state.locked, |group| !self.locked_groups.contains(&group))
    }
}

fn object_kind_name<'a>(view: &'a MapView, kind: &CanvasObjectKind) -> &'a str {
    match kind {
        CanvasObjectKind::Shape { shape, .. } => view
            .board_labels
            .shape_names
            .get(shape)
            .map_or(view.board_labels.shape.as_str(), String::as_str),
        CanvasObjectKind::Connector(_) => &view.board_labels.connector,
        CanvasObjectKind::Ink(_) => &view.board_labels.ink,
    }
}

fn endpoint_description(
    sheet: &Sheet,
    view: &MapView,
    index: &AccessibilityIndex<'_>,
    endpoint: Endpoint,
) -> String {
    match endpoint {
        Endpoint::Topic(id) => sheet
            .topic(id)
            .filter(|_| !sheet.is_anchor(id))
            .map_or_else(
                || view.board_labels.free_point.clone(),
                |topic| topic.title.clone(),
            ),
        Endpoint::Object(id) => index.objects.get(&id).map_or_else(
            || view.board_labels.free_point.clone(),
            |object| {
                if !object.alt_text.is_empty() {
                    object.alt_text.clone()
                } else if !object.label().is_empty() {
                    object.label().to_owned()
                } else {
                    object_kind_name(view, &object.kind).to_owned()
                }
            },
        ),
        Endpoint::Point(point) => {
            format!(
                "{} ({:.0}, {:.0})",
                view.board_labels.free_point, point.x, point.y
            )
        }
    }
}

fn item_label(
    sheet: &Sheet,
    view: &MapView,
    index: &AccessibilityIndex<'_>,
    selected_topics: &HashSet<TopicId>,
    selected_objects: &HashSet<CanvasObjectId>,
    id: SceneId,
) -> Option<(String, bool)> {
    let (mut label, selected, locked) = match id {
        SceneId::Zone(id) => {
            let zone = index.zones.get(&id)?;
            (
                format!("{}: {}", view.board_labels.frame, zone.name),
                view.selected_object == Some(ObjectSelection::Zone(id)),
                false,
            )
        }
        SceneId::Topic(id) => {
            if sheet.is_anchor(id) {
                return None;
            }
            let topic = sheet.topic(id)?;
            let kind = match topic.card.map(|card| card.kind) {
                Some(CardKind::Sticky) => format!("{}: ", view.board_labels.sticky),
                Some(CardKind::Text) => format!("{}: ", view.board_labels.text),
                Some(CardKind::Image) => format!("{}: ", view.board_labels.image),
                None => String::new(),
            };
            (
                format!("{kind}{}", topic.title),
                selected_topics.contains(&id),
                !index.topic_is_unlocked(sheet, id),
            )
        }
        SceneId::Object(id) => {
            let object = index.objects.get(&id)?;
            let kind = object_kind_name(view, &object.kind);
            let description = if !object.alt_text.is_empty() {
                object.alt_text.as_str()
            } else {
                object.label()
            };
            let mut label = if description.is_empty() {
                kind.to_owned()
            } else {
                format!("{kind}: {description}")
            };
            if let CanvasObjectKind::Connector(connector) = &object.kind {
                label.push_str(&format!(
                    ", {}: {}, {}: {}",
                    view.board_labels.from,
                    endpoint_description(sheet, view, index, connector.from),
                    view.board_labels.to,
                    endpoint_description(sheet, view, index, connector.to)
                ));
            }
            (
                label,
                selected_objects.contains(&id),
                !index.object_is_unlocked(sheet, object),
            )
        }
    };
    if selected {
        label.push_str(&format!(", {}", view.board_labels.selected));
    }
    if locked {
        label.push_str(&format!(", {}", view.board_labels.locked));
    }
    Some((label, selected))
}

fn select_item(view: &mut MapView, sheet: &Sheet, id: SceneId) {
    match id {
        SceneId::Topic(id) => select_hit(view, sheet, SceneHit::Topic(id), false),
        SceneId::Object(id) => {
            select_hit(view, sheet, SceneHit::Object(id), false);
            view.reveal_board_object(id);
        }
        SceneId::Zone(id) => view.select_object(ObjectSelection::Zone(id)),
    }
}

/// Register real, focusable AccessKit nodes without intercepting canvas pointer hits.
pub(super) fn accessibility(ui: &mut Ui, view: &mut MapView, sheet: &Sheet, scene: &Scene) -> bool {
    let has_board_content = !sheet.root_mode.is_central()
        || !sheet.canvas.objects.is_empty()
        || !sheet.zones.is_empty()
        || sheet.topics.values().any(|topic| topic.card.is_some())
        || view.board_tool != BoardTool::Select;
    if !has_board_content {
        return false;
    }
    let focus_after_duplicate = view.board_input.focus_after_duplicate.take();
    let index = AccessibilityIndex::new(sheet);
    let mut selected_topics: HashSet<_> = view.selection.iter().copied().collect();
    let mut selected_objects: HashSet<_> = view.board_selection.iter().copied().collect();
    let mut focused = false;
    for item in reading_order(sheet, scene) {
        // Valid workbooks have unique ids; preserve the old first-match path
        // for malformed sheets supplied directly by a host.
        let item = if index.duplicate_ids {
            scene.item(item.id).expect("reading order item is in scene")
        } else {
            item
        };
        let id = item.id;
        let Some((label, selected)) =
            item_label(sheet, view, &index, &selected_topics, &selected_objects, id)
        else {
            continue;
        };
        let mut rect = view.xf().rect(item.bounds);
        if let Some(clip) = item.clip {
            rect = rect.intersect(view.xf().rect(clip));
        }
        let response = ui.interact(rect, item_id(id), Sense::focusable_noninteractive());
        response
            .widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, &label));
        if response.gained_focus() && focus_after_duplicate != Some(id) {
            select_item(view, sheet, id);
            selected_topics.clear();
            selected_topics.extend(view.selection.iter().copied());
            selected_objects.clear();
            selected_objects.extend(view.board_selection.iter().copied());
        }
        if response.has_focus() {
            focused = true;
            ui.ctx().memory_mut(|memory| {
                memory.set_focus_lock_filter(
                    response.id,
                    EventFilter {
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: true,
                    },
                )
            });
        }
    }
    focused
}

fn board_context(sheet: &Sheet, view: &MapView) -> bool {
    sheet.root_mode == SheetRootMode::Hidden
        || view.board_tool != BoardTool::Select
        || !view.board_selection.is_empty()
        || view
            .selection
            .iter()
            .any(|id| sheet.topic(*id).is_some_and(|topic| topic.card.is_some()))
        || matches!(view.selected_object, Some(ObjectSelection::Zone(id))
            if sheet.zones.iter().any(|zone| zone.id == id && zone.keep_empty))
}

fn cycle_selection(
    ui: &Ui,
    doc: &Document,
    view: &mut MapView,
    layout: &Layout,
    scene: &Scene,
    reverse: bool,
) {
    let items = reading_order(doc.sheet(), scene);
    let focused = ui.ctx().memory(|memory| memory.focused());
    if items.is_empty() || (reverse && focused == Some(MapCanvas::id())) {
        exit_focus(ui, view, reverse);
        return;
    }
    let current = focused
        .and_then(|focused| items.iter().position(|item| item_id(item.id) == focused))
        .or_else(|| {
            if focused == Some(MapCanvas::id()) {
                None
            } else if let Some(id) = view.board_selection.last() {
                items
                    .iter()
                    .position(|item| item.id == SceneId::Object(*id))
            } else if let Some(id) = view.primary() {
                items.iter().position(|item| item.id == SceneId::Topic(id))
            } else if let Some(ObjectSelection::Zone(id)) = view.selected_object {
                items.iter().position(|item| item.id == SceneId::Zone(id))
            } else {
                None
            }
        });
    let index = match (current, reverse) {
        (Some(0), true) | (None, true) => {
            exit_focus(ui, view, true);
            return;
        }
        (Some(index), true) => index - 1,
        (Some(index), false) if index + 1 == items.len() => {
            exit_focus(ui, view, false);
            return;
        }
        (Some(index), false) => index + 1,
        (None, false) => 0,
    };
    let id = items[index].id;
    select_item(view, doc.sheet(), id);
    let target = if reverse {
        tab_controls(view, doc.sheet(), layout, scene)
            .last()
            .map_or_else(|| item_id(id), |control| control_id(*control))
    } else {
        item_id(id)
    };
    ui.ctx().memory_mut(|memory| memory.request_focus(target));
}

fn board_run(doc: &mut Document, view: &mut MapView, action: BoardAction, out: &mut CanvasOutput) {
    if let Err(error) = apply_board_action(doc, view, action) {
        out.error = Some(error);
    }
}

/// Returns true when a board-specific key was consumed.
pub(super) fn keyboard(
    ui: &Ui,
    doc: &mut Document,
    view: &mut MapView,
    layout: &Layout,
    scene: &Scene,
    out: &mut CanvasOutput,
) -> bool {
    if !board_context(doc.sheet(), view) {
        return false;
    }
    let ctx = ui.ctx();
    if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) {
        if view.board_tool != BoardTool::Select || view.board_input.connector.is_some() {
            view.set_board_tool(BoardTool::Select);
            view.board_input.connector = None;
        } else {
            view.selection.clear();
            view.board_selection.clear();
            view.selected_object = None;
            ctx.memory_mut(|memory| memory.request_focus(MapCanvas::id()));
        }
        return true;
    }
    for (modifiers, reverse) in [(Modifiers::SHIFT, true), (Modifiers::NONE, false)] {
        if ctx.input_mut(|input| input.consume_key(modifiers, Key::Tab)) {
            let focused = ctx.memory(|memory| memory.focused());
            let first = tab_controls(view, doc.sheet(), layout, scene)
                .into_iter()
                .next();
            if !reverse
                && let Some(control) = first
                && focused == Some(item_id(control.item))
            {
                ctx.memory_mut(|memory| memory.request_focus(control_id(control)));
            } else {
                cycle_selection(ui, doc, view, layout, scene, reverse);
            }
            return true;
        }
    }
    let armed = matches!(view.board_tool, BoardTool::Shape(_));
    if armed && ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::V)) {
        view.set_board_tool(BoardTool::Select);
        return true;
    }
    // With a selection, an armed shape edits like Select (arrows, F2, typing);
    // without one, Enter/Space still inserts the armed shape.
    let armed_editing =
        armed && !board_actions::selected_board_members(doc.sheet(), view).is_empty();
    if view.board_tool != BoardTool::Select && view.board_tool != BoardTool::Hand && !armed_editing
    {
        if ctx.input_mut(|input| {
            input.consume_key(Modifiers::NONE, Key::Enter)
                || input.consume_key(Modifiers::NONE, Key::Space)
        }) && prepare_edit(doc, view, out)
        {
            let tool = view.board_tool;
            let point = if tool == BoardTool::Connector {
                view.xf().sheet(view.canvas_rect.center())
            } else {
                insert_spot(doc.sheet(), view, scene, default_size(tool))
            };
            if let Err(error) = create_at(doc, view, tool, point) {
                out.error = Some(error);
            }
            return true;
        }
        return false;
    }

    let shift_cmd = Modifiers::COMMAND.plus(Modifiers::SHIFT);
    for (modifiers, key, action) in [
        (shift_cmd, Key::G, BoardAction::Ungroup),
        (Modifiers::COMMAND, Key::G, BoardAction::Group),
        (
            shift_cmd,
            Key::OpenBracket,
            BoardAction::ZOrder(ZOrder::Back),
        ),
        (
            shift_cmd,
            Key::CloseBracket,
            BoardAction::ZOrder(ZOrder::Front),
        ),
        (
            Modifiers::COMMAND,
            Key::OpenBracket,
            BoardAction::ZOrder(ZOrder::Backward),
        ),
        (
            Modifiers::COMMAND,
            Key::CloseBracket,
            BoardAction::ZOrder(ZOrder::Forward),
        ),
    ] {
        if action.available(doc, view) && ctx.input_mut(|input| input.consume_key(modifiers, key)) {
            board_run(doc, view, action, out);
            return true;
        }
    }
    let members = board_actions::selected_board_members(doc.sheet(), view);
    if !members.is_empty() && ctx.input_mut(|input| input.consume_key(Modifiers::COMMAND, Key::L)) {
        let lock = members
            .iter()
            .any(|member| board_actions::member_is_movable(doc.sheet(), *member));
        board_run(doc, view, BoardAction::Lock(lock), out);
        return true;
    }
    if board_actions::has_duplicate_target(doc.sheet(), view) {
        let rotate = Modifiers::COMMAND.plus(Modifiers::ALT);
        for (modifiers, step) in [(rotate.plus(Modifiers::SHIFT), 15.0), (rotate, 1.0)] {
            for (key, sign) in [(Key::ArrowLeft, -1.0), (Key::ArrowRight, 1.0)] {
                if ctx.input_mut(|input| input.consume_key(modifiers, key)) {
                    let action = BoardAction::Rotate(step * sign);
                    if action.available(doc, view) {
                        board_run(doc, view, action, out);
                    }
                    return true;
                }
            }
        }
        for (modifiers, step) in [
            (Modifiers::ALT.plus(Modifiers::SHIFT), 40.0),
            (Modifiers::ALT, 10.0),
        ] {
            for (key, delta) in [
                (Key::ArrowLeft, Vec2::new(-step, 0.0)),
                (Key::ArrowRight, Vec2::new(step, 0.0)),
                (Key::ArrowUp, Vec2::new(0.0, -step)),
                (Key::ArrowDown, Vec2::new(0.0, step)),
            ] {
                if ctx.input_mut(|input| input.consume_key(modifiers, key)) {
                    if prepare_edit(doc, view, out) {
                        match board_actions::duplicate_selection(doc, view, delta) {
                            Ok(true) => focus_duplicated_selection(ui, view),
                            Err(error) => out.error = Some(error),
                            Ok(false) => {}
                        }
                    }
                    return true;
                }
            }
        }
    }
    // Do not let the generic arrow matcher interpret an unhandled Alt chord
    // as movement or resizing; legacy map shortcuts get it instead.
    if alt_arrow_pressed(ui) {
        return false;
    }
    if let Some((delta, modifiers)) = arrow(ui, |delta, modifiers| {
        let action = if modifiers.command {
            BoardAction::Resize(delta)
        } else {
            BoardAction::Move(delta)
        };
        action.available(doc, view) || !view.board_selection.is_empty()
    }) {
        let action = if modifiers.command {
            BoardAction::Resize(delta)
        } else {
            BoardAction::Move(delta)
        };
        if action.available(doc, view) {
            board_run(doc, view, action, out);
        }
        return true;
    }
    if view.selection.is_empty()
        && view.board_selection.len() == 1
        && ctx.input_mut(|input| {
            input.consume_key(Modifiers::NONE, Key::F2)
                || input.consume_key(Modifiers::NONE, Key::Enter)
                || input.consume_key(Modifiers::NONE, Key::Space)
        })
    {
        view.begin_board_label(doc, view.board_selection[0]);
        return true;
    }
    if view.selection.is_empty() && view.board_selection.len() == 1 {
        let text = ctx.input(|input| {
            if input.modifiers.command || input.modifiers.alt {
                None
            } else {
                input.events.iter().find_map(|event| match event {
                    egui::Event::Text(text) if !text.trim().is_empty() => Some(text.clone()),
                    _ => None,
                })
            }
        });
        if let Some(text) = text
            && view.begin_board_label(doc, view.board_selection[0])
        {
            if let Some(edit) = &mut view.board_input.label {
                edit.text = text;
                edit.seeded = true;
            }
            return true;
        }
    }
    false
}

pub(super) fn paint(
    painter: &egui::Painter,
    view: &MapView,
    sheet: &Sheet,
    layout: &Layout,
    scene: &Scene,
    pointer: Option<Pos2>,
) {
    let xf = view.xf();
    for id in &view.board_selection {
        let Some(item) = scene.item(SceneId::Object(*id)) else {
            continue;
        };
        let clipped = item.clip.map(|clip| painter.with_clip_rect(xf.rect(clip)));
        let target = clipped.as_ref().unwrap_or(painter);
        let color = if !board_actions::member_is_movable(sheet, BoardMember::Object(*id)) {
            view.accent.gamma_multiply(0.55)
        } else {
            view.accent
        };
        target.rect_stroke(
            xf.rect(item.bounds).expand(3.0),
            2.0,
            Stroke::new(2.0, color),
            StrokeKind::Outside,
        );
    }

    for control in controls(view, sheet, layout, scene) {
        let center = xf.pos(control.point);
        if !view.canvas_rect.contains(center) {
            continue;
        }
        let focused = painter
            .ctx()
            .memory(|memory| memory.has_focus(control_id(control)));
        if let ControlKind::Arrow(dir) = control.kind {
            if view.board_input.gesture.is_some() {
                continue;
            }
            let picking = view.board_input.quick.as_ref().is_some_and(|quick| {
                SceneId::Object(quick.source) == control.item && quick.dir == dir
            });
            let hot = focused
                || picking
                || pointer.is_some_and(|pointer| pointer.distance(center) <= 14.0);
            let unit = dir.unit();
            let forward = vec2(unit.x, unit.y);
            let normal = vec2(-forward.y, forward.x);
            let size = if hot { 9.0 } else { 7.0 };
            let color = if hot {
                view.accent
            } else {
                view.accent.gamma_multiply(0.6)
            };
            painter.add(egui::Shape::convex_polygon(
                vec![
                    center + forward * size,
                    center - forward * (size * 0.6) + normal * size,
                    center - forward * (size * 0.6) - normal * size,
                ],
                color,
                Stroke::NONE,
            ));
            if focused {
                painter.circle_stroke(center, 12.0, Stroke::new(2.0, view.accent));
            }
            continue;
        }
        let radius = if matches!(control.kind, ControlKind::InsertWaypoint(_, _)) {
            4.0
        } else {
            6.0
        };
        painter.circle_filled(center, radius, sheet_background(sheet, view.dark));
        painter.circle_stroke(center, radius, Stroke::new(2.0, view.accent));
        if focused {
            painter.circle_stroke(center, 9.0, Stroke::new(2.0, view.accent));
        }
    }

    let armed_ghost = match view.board_tool {
        BoardTool::Shape(kind)
            if view.board_input.gesture.is_none() && view.board_input.creation_press.is_none() =>
        {
            view.board_input.ghost.map(|pointer| (kind, pointer))
        }
        _ => None,
    };
    if let Some((kind, pointer)) = view.board_input.drop_ghost.or(armed_ghost) {
        let center = snapped(sheet, xf.sheet(pointer));
        let ghost =
            thinoda_core::Rect::from_center_size(center, default_size(BoardTool::Shape(kind)));
        let geometry = thinoda_core::board::shape_geometry(kind, ghost, 0.0);
        let stroke = Stroke::new(1.5, view.accent.gamma_multiply(0.6));
        painter.add(egui::Shape::closed_line(
            geometry
                .outline
                .into_iter()
                .map(|point| xf.pos(point))
                .collect(),
            stroke,
        ));
        for detail in geometry.details {
            painter.add(egui::Shape::line(
                detail.into_iter().map(|point| xf.pos(point)).collect(),
                stroke,
            ));
        }
    }

    if let Some(quick) = &view.board_input.quick
        && let Some(shape) = quick.preview
        && let Some(object) = arrow_source(sheet, quick.source)
    {
        // Where the hovered or focused picker tile would land.
        let target = quick_target(sheet, scene, object, quick.dir);
        let stroke = Stroke::new(1.5, view.accent.gamma_multiply(0.6));
        painter.add(egui::Shape::dashed_line(
            &[
                xf.pos(quick.dir.side(object.rotated_bounds())),
                xf.pos(quick.dir.opposite().side(target)),
            ],
            stroke,
            6.0,
            4.0,
        ));
        let rounding = match &object.kind {
            CanvasObjectKind::Shape { shape: source, .. } if *source == shape => object.rounding,
            _ => None,
        };
        let margin = match &object.kind {
            CanvasObjectKind::Shape { shape: source, .. } if *source == shape => {
                object.double_ellipse_margin
            }
            _ => None,
        };
        let geometry = shape_geometry_with_style(
            shape,
            target,
            object.rotation,
            rounding,
            object.style.effective_shape_stroke_width(),
            margin,
        );
        painter.add(egui::Shape::closed_line(
            geometry
                .outline
                .into_iter()
                .map(|point| xf.pos(point))
                .collect(),
            stroke,
        ));
        for detail in geometry.details {
            painter.add(egui::Shape::line(
                detail.into_iter().map(|point| xf.pos(point)).collect(),
                stroke,
            ));
        }
    }

    if let Some(pending) = &view.board_input.connector
        && let Some(pointer) = pointer
    {
        painter.add(egui::Shape::dashed_line(
            &[xf.pos(pending.point), pointer],
            Stroke::new(1.5, view.accent),
            6.0,
            4.0,
        ));
    }
    if view.board_input.gesture.is_none()
        && let Some(id) = view.board_input.outline_hover
        && let Some(item) = scene.item(SceneId::Object(id))
        && let Some(outline) = item_outline(item)
    {
        let clipped = item.clip.map(|clip| painter.with_clip_rect(xf.rect(clip)));
        clipped
            .as_ref()
            .unwrap_or(painter)
            .add(egui::Shape::closed_line(
                outline.into_iter().map(|point| xf.pos(point)).collect(),
                Stroke::new(3.0, view.accent.gamma_multiply(0.7)),
            ));
    }
    let Some(drag) = &view.board_input.gesture else {
        return;
    };
    if drag.canceled {
        return;
    }
    let stroke = Stroke::new(2.0, view.accent);
    match &drag.kind {
        GestureKind::Create {
            tool,
            start,
            current,
            ..
        } if *tool == BoardTool::Connector => {
            painter.line_segment([xf.pos(*start), xf.pos(*current)], stroke);
        }
        GestureKind::Create {
            tool,
            start,
            current,
            ..
        } => {
            let minimum = if *tool == BoardTool::Frame {
                thinoda_core::MIN_ZONE_SIZE
            } else {
                MIN_CARD_SIZE
            };
            let rect = drag_rect(snapped(sheet, *start), snapped(sheet, *current), minimum);
            painter.rect(
                xf.rect(rect),
                3.0,
                view.accent.gamma_multiply(0.12),
                stroke,
                StrokeKind::Inside,
            );
        }
        GestureKind::Move {
            start,
            current,
            anchor,
            duplicate,
        } => {
            let delta = snapped(sheet, *anchor + *current - *start) - *anchor;
            let movable = board_actions::movable_board_members(sheet, view);
            if view.show_alignment_guides {
                let guides =
                    drag_alignment_guides(sheet, view, scene, &movable, delta, 4.0 / xf.zoom);
                crate::topic_drag::paint_drag_guides(painter, xf, view.accent, guides);
            }
            for item in &scene.items {
                let selected = match item.id {
                    SceneId::Topic(id) => sheet.board_member_for_topic(id).map_or_else(
                        || view.selection.contains(&id) && sheet.is_floating(id),
                        |member| movable.contains(&member),
                    ),
                    SceneId::Object(id) => movable.contains(&BoardMember::Object(id)),
                    SceneId::Zone(id) => {
                        *duplicate && view.selected_object == Some(ObjectSelection::Zone(id))
                    }
                };
                if selected {
                    painter.rect_stroke(
                        xf.rect(item.bounds.translate(delta)),
                        2.0,
                        stroke,
                        StrokeKind::Outside,
                    );
                }
            }
        }
        GestureKind::Resize { preview, .. } => {
            painter.rect_stroke(xf.rect(*preview), 2.0, stroke, StrokeKind::Outside);
        }
        GestureKind::Rotate { item, preview, .. } => {
            let rect = match item {
                SceneId::Topic(id) => layout.rect(*id),
                SceneId::Object(id) => sheet.canvas.object(*id).map(|object| object.rect),
                SceneId::Zone(_) => None,
            };
            if let Some(rect) = rect {
                let corners = rotated_corners(rect, *preview);
                painter.add(egui::Shape::closed_line(
                    corners.into_iter().map(|point| xf.pos(point)).collect(),
                    stroke,
                ));
            }
        }
        GestureKind::Endpoint { id, start, point } => {
            if let Some(SceneGeometry::Polyline { points, .. }) =
                scene.item(SceneId::Object(*id)).map(|item| &item.geometry)
            {
                let other = if *start {
                    points.last()
                } else {
                    points.first()
                };
                if let Some(other) = other {
                    painter.add(egui::Shape::dashed_line(
                        &[xf.pos(*other), xf.pos(*point)],
                        stroke,
                        6.0,
                        4.0,
                    ));
                }
            }
        }
        GestureKind::Waypoint {
            id,
            index,
            insert,
            point,
            ..
        } => {
            if let Some(object) = sheet.canvas.object(*id)
                && let CanvasObjectKind::Connector(connector) = &object.kind
            {
                let mut connector = connector.clone();
                let mut points = connector.waypoints.to_vec();
                if *insert && *index <= points.len() {
                    points.insert(*index, *point);
                } else if !*insert && *index < points.len() {
                    points[*index] = *point;
                } else {
                    return;
                }
                connector.waypoints = points.into();
                let preview = thinoda_core::scene::connector_path(sheet, layout, &connector);
                if preview.len() >= 2 {
                    let clipped = scene
                        .item(SceneId::Object(*id))
                        .and_then(|item| item.clip)
                        .map(|clip| painter.with_clip_rect(xf.rect(clip)));
                    let target = clipped.as_ref().unwrap_or(painter);
                    target.add(egui::Shape::dashed_line(
                        &preview
                            .into_iter()
                            .map(|point| xf.pos(point))
                            .collect::<Vec<_>>(),
                        stroke,
                        6.0,
                        4.0,
                    ));
                    target.circle_filled(xf.pos(*point), 6.0, sheet_background(sheet, view.dark));
                    target.circle_stroke(xf.pos(*point), 6.0, stroke);
                }
            }
        }
        GestureKind::Arrow {
            source,
            anchor,
            current,
            ..
        }
        | GestureKind::Outline {
            source,
            anchor,
            current,
        } => {
            if (*current - drag.origin).length() * xf.zoom < ARROW_DRAG {
                return;
            }
            painter.add(egui::Shape::dashed_line(
                &[xf.pos(*anchor), xf.pos(*current)],
                stroke,
                6.0,
                4.0,
            ));
            let target = scene
                .hit_test(*current)
                .filter(|hit| *hit != SceneHit::Object(*source))
                .and_then(|hit| {
                    scene.item(match hit {
                        SceneHit::Topic(id) => SceneId::Topic(id),
                        SceneHit::Object(id) => SceneId::Object(id),
                    })
                });
            if let Some(item) = target {
                painter.rect_stroke(
                    xf.rect(item.bounds).expand(3.0),
                    2.0,
                    stroke,
                    StrokeKind::Outside,
                );
            } else if painter.ctx().input(|input| input.modifiers.command)
                && let Some(object) = sheet.canvas.object(*source)
            {
                let rect = thinoda_core::Rect::from_center_size(
                    snapped(sheet, *current),
                    object.rect.size,
                );
                painter.add(egui::Shape::closed_line(
                    rotated_corners(rect, object.rotation)
                        .into_iter()
                        .map(|point| xf.pos(point))
                        .collect(),
                    Stroke::new(1.5, view.accent.gamma_multiply(0.6)),
                ));
            }
        }
    }
}

fn show_label_edit_hint(ui: &Ui, view: &MapView, object_bounds: egui::Rect, hint: &str) {
    let canvas = view.canvas_rect();
    let max_width = (canvas.width() - 24.0).max(0.0);
    if max_width < 1.0 {
        return;
    }
    let text_width = (max_width - 16.0).max(1.0);
    let title = ui.painter().layout_no_wrap(
        view.board_labels.label_editing.clone(),
        egui::TextStyle::Body.resolve(ui.style()),
        ui.visuals().text_color(),
    );
    let detail = ui.painter().layout(
        hint.to_owned(),
        egui::TextStyle::Small.resolve(ui.style()),
        ui.visuals().text_color(),
        text_width,
    );
    let height = title.size().y + detail.size().y + ui.spacing().item_spacing.y + 48.0;
    let bounds = object_bounds.expand(8.0);
    let below = canvas.bottom() - bounds.bottom();
    let above = bounds.top() - canvas.top();
    let (pivot, y) = if below >= height + 12.0 && below >= above {
        (egui::Align2::CENTER_BOTTOM, canvas.bottom() - 12.0)
    } else if above >= height + 64.0 {
        (egui::Align2::CENTER_TOP, canvas.top() + 64.0)
    } else if below >= height + 12.0 {
        (egui::Align2::CENTER_BOTTOM, canvas.bottom() - 12.0)
    } else {
        return;
    };
    egui::Area::new(MapCanvas::id().with("board-label-edit-hint"))
        .order(egui::Order::Foreground)
        .pivot(pivot)
        .fixed_pos(egui::pos2(canvas.center().x, y))
        .constrain_to(canvas)
        .movable(false)
        .interactable(false)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_max_width((max_width - 16.0).max(1.0));
                ui.strong(&view.board_labels.label_editing);
                ui.add(egui::Label::new(egui::RichText::new(hint).small()).wrap());
            });
        });
}

fn label_text_halo(
    position: Pos2,
    galley: std::sync::Arc<egui::Galley>,
    color: Color32,
) -> egui::Shape {
    let [r, g, b, _] = color.to_srgba_unmultiplied();
    let outline = crate::render::color32(thinoda_core::Color::highest_contrast_on(
        thinoda_core::Color::rgb(r, g, b),
    ));
    egui::Shape::Vec(
        [
            vec2(-1.0, 0.0),
            vec2(1.0, 0.0),
            vec2(0.0, -1.0),
            vec2(0.0, 1.0),
        ]
        .into_iter()
        .map(|offset| {
            egui::Shape::galley_with_override_text_color(position + offset, galley.clone(), outline)
        })
        .collect(),
    )
}

fn label_editor_text_color(text: Color32, background: Color32) -> Color32 {
    let [r, g, b, a] = text.to_srgba_unmultiplied();
    let [br, bg, bb, _] = background.to_srgba_unmultiplied();
    let background = thinoda_core::Color::rgb(br, bg, bb);
    if thinoda_core::Color::rgba(r, g, b, a).contrast_on(background) < 4.5 {
        crate::render::color32(thinoda_core::Color::highest_contrast_on(background))
    } else {
        text
    }
}

fn label_editor_rect(object: &CanvasObject, item: &SceneItem, view: &MapView) -> Option<Rect> {
    let rect = match &object.kind {
        CanvasObjectKind::Shape { shape, .. } => {
            let label_rect = thinoda_core::board::shape_label_rect_with_style(
                *shape,
                object.rect,
                object.style.effective_shape_stroke_width(),
                object.double_ellipse_margin,
            );
            thinoda_core::Rect::from_center_size(
                rotate_around(label_rect.center(), object.rect.center(), object.rotation),
                label_rect.size,
            )
        }
        CanvasObjectKind::Connector(_) => {
            let point = match &item.geometry {
                SceneGeometry::Polyline { points, .. } => {
                    polyline_midpoint(points).unwrap_or(item.bounds.center())
                }
                _ => item.bounds.center(),
            };
            thinoda_core::Rect::from_center_size(point, Vec2::new(140.0, 40.0))
        }
        CanvasObjectKind::Ink(_) => return None,
    };
    Some(view.xf().rect(rect).expand(2.0))
}

pub(super) fn label_keyboard_event(event: &egui::Event) -> bool {
    matches!(
        event,
        egui::Event::Text(_)
            | egui::Event::Paste(_)
            | egui::Event::Cut
            | egui::Event::Copy
            | egui::Event::Ime(_)
            | egui::Event::Key { .. }
    )
}

fn plain_key_press(event: &egui::Event, expected: Key) -> bool {
    matches!(
        event,
        egui::Event::Key {
            key,
            pressed: true,
            modifiers,
            ..
        } if *key == expected && modifiers.matches_exact(Modifiers::NONE)
    )
}

/// Finish a draft after applying input that preceded an outside pointer press.
pub(super) fn finish_label_before_pointer_press(
    ui: &mut Ui,
    doc: &mut Document,
    view: &mut MapView,
    scene: &Scene,
    edit_error: &mut Option<thinoda_core::EditError>,
    overlay_rect: Option<Rect>,
) {
    let edit_id = MapCanvas::board_label_editor_id();
    let (editor_focused, editor_lost_focus) = ui.memory(|memory| {
        (
            memory.has_focus(edit_id),
            memory.focused().is_none() && memory.had_focus_last_frame(edit_id),
        )
    });
    let Some(edit) = &view.board_input.label else {
        return;
    };
    let retry_pending = edit.focus_retry_after.is_some_and(|(viewport, frame)| {
        viewport == ui.ctx().viewport_id()
            && frame < ui.ctx().cumulative_frame_nr()
            && ui.memory(|memory| memory.focused().is_none())
    });
    if !editor_focused && !editor_lost_focus && !retry_pending {
        return;
    }
    let Some(object) = doc.sheet().canvas.object(edit.id) else {
        return;
    };
    let Some(item) = scene.item(SceneId::Object(edit.id)) else {
        return;
    };
    let Some(rect) = label_editor_rect(object, item, view) else {
        return;
    };
    let split = ui.input(|input| {
        if !input.focused {
            return None;
        }
        let (index, position) =
            input
                .events
                .iter()
                .enumerate()
                .find_map(|(index, event)| match event {
                    egui::Event::PointerButton {
                        pos, pressed: true, ..
                    } => Some((index, *pos)),
                    _ => None,
                })?;
        if view.canvas_rect().contains(position)
            && rect.contains(position)
            && !overlay_rect.is_some_and(|r| r.contains(position))
        {
            return None;
        }
        let prefix = &input.events[..index];
        #[allow(deprecated)]
        let ime_frame = edit.ime_composing
            || prefix.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::Ime(
                        egui::ImeEvent::Preedit { .. }
                            | egui::ImeEvent::Commit(_)
                            | egui::ImeEvent::Disabled
                    )
                )
            });
        let input_end = if ime_frame {
            index
        } else {
            prefix
                .iter()
                .position(|event| {
                    [Key::Enter, Key::Tab, Key::Escape]
                        .into_iter()
                        .any(|key| plain_key_press(event, key))
                })
                .map_or(index, |finish| finish + 1)
        };
        Some(input_end)
    });
    let Some(input_end) = split else {
        return;
    };
    if ui.input(|input| input.events[..input_end].iter().any(label_keyboard_event)) {
        // Keep the pointer and later input for their actual owners.
        let (events, pointer) = ui.input_mut(|input| {
            let events = std::mem::take(&mut input.events);
            input.events = events[..input_end].to_vec();
            (events, std::mem::take(&mut input.pointer))
        });
        if !editor_focused && !retry_pending {
            ui.memory_mut(|memory| memory.request_focus(edit_id));
        }
        // A separate Ui keeps the host's auto-generated widget IDs unchanged.
        let mut child = Ui::new(
            ui.ctx().clone(),
            edit_id.with("input-only"),
            egui::UiBuilder::new()
                .layer_id(ui.layer_id())
                .max_rect(ui.max_rect())
                .layout(*ui.layout())
                .style(ui.style().clone()),
        );
        child.set_clip_rect(Rect::NOTHING);
        show_label_editor(&mut child, doc, view, scene, false, edit_error, true);
        ui.input_mut(|input| {
            input.pointer = pointer;
            input.events = events
                .into_iter()
                .enumerate()
                .filter_map(|(index, event)| {
                    (index >= input_end || !label_keyboard_event(&event)).then_some(event)
                })
                .collect();
        });
    }
    if view.board_input.label.is_some() {
        match view.try_commit_board_label(doc) {
            Ok(_) => ui.memory_mut(|memory| memory.surrender_focus(edit_id)),
            Err(error) => *edit_error = Some(error),
        }
    }
}

pub(super) fn show_label_editor(
    ui: &mut Ui,
    doc: &mut Document,
    view: &mut MapView,
    scene: &Scene,
    minimap_blocks_keyboard: bool,
    edit_error: &mut Option<thinoda_core::EditError>,
    input_only: bool,
) {
    let Some(edit) = view.board_input.label.as_ref() else {
        return;
    };
    let Some(object) = doc.sheet().canvas.object(edit.id) else {
        // Keep the draft for recovery rather than silently discarding it.
        return;
    };
    let Some(item) = scene.item(SceneId::Object(edit.id)) else {
        return;
    };
    let Some(rect) = label_editor_rect(object, item, view) else {
        return;
    };
    let edit_id = MapCanvas::board_label_editor_id();
    if let Some(edit) = &mut view.board_input.label
        && let Some((viewport, frame)) = edit.focus_retry_after
        && (viewport != ui.ctx().viewport_id() || frame < ui.ctx().cumulative_frame_nr())
    {
        edit.focus_retry_after = None;
        if viewport == ui.ctx().viewport_id()
            && !minimap_blocks_keyboard
            && ui.memory(|memory| memory.focused().is_none())
            && ui.input(|input| {
                input.focused && !input.pointer.any_pressed() && !input.pointer.any_released()
            })
        {
            ui.memory_mut(|memory| memory.request_focus(edit_id));
        }
    }
    let editor_focused = ui.memory(|memory| memory.has_focus(edit_id));
    // On the first focused frame egui can surrender focus to an Escape key
    // before TextEdit has installed its focus filter.
    let escape_released_editor = !editor_focused
        && ui.memory(|memory| memory.focused().is_none() && memory.had_focus_last_frame(edit_id))
        && ui.input(|input| {
            input.focused
                && !input.pointer.any_pressed()
                && !input.pointer.any_released()
                && input
                    .events
                    .iter()
                    .any(|event| plain_key_press(event, Key::Escape))
        });
    let editor_owns_keyboard = editor_focused || escape_released_editor;
    if !minimap_blocks_keyboard
        && editor_owns_keyboard
        && let Some(edit) = &mut view.board_input.label
    {
        protect_ime_candidate_keys(ui.ctx(), edit_id, &mut edit.ime_composing);
    }
    let (commit, cancel) = if minimap_blocks_keyboard || !editor_owns_keyboard {
        (false, false)
    } else {
        ui.input_mut(|input| {
            let mut enter = false;
            input.events.retain(|event| {
                let plain_enter = plain_key_press(event, Key::Enter);
                enter |= plain_enter;
                !plain_enter
            });
            (
                enter || input.consume_key(Modifiers::NONE, Key::Tab),
                input.consume_key(Modifiers::NONE, Key::Escape),
            )
        })
    };
    if cancel {
        view.board_input.label = None;
        ui.memory_mut(|memory| memory.request_focus(MapCanvas::id()));
        return;
    }
    let hint = if matches!(view.board_tool, BoardTool::Shape(_)) {
        format!(
            "{}\n{}",
            view.board_labels.label_edit_hint, view.board_labels.label_edit_armed_hint
        )
    } else {
        view.board_labels.label_edit_hint.clone()
    };
    if !input_only {
        show_label_edit_hint(ui, view, view.xf().rect(item.bounds).union(rect), &hint);
    }
    // Keep the shape outline visible; connector labels retain their line backdrop.
    let (text_color, background) =
        crate::render::board_label_colors(doc.sheet(), &object.style, view.dark);
    let contrast_background = if matches!(object.kind, CanvasObjectKind::Shape { .. }) {
        crate::render::shape_label_editor_background(doc.sheet(), &object.style, view.dark)
    } else {
        background
    };
    let text_color = label_editor_text_color(text_color, contrast_background);
    if !input_only && matches!(object.kind, CanvasObjectKind::Connector(_)) {
        ui.painter().rect_filled(rect, 2.0, background);
    }
    let font_size = crate::render::shape_label_font_size(view.zoom).max(10.0);
    let font = FontId::proportional(font_size);
    let is_shape = matches!(object.kind, CanvasObjectKind::Shape { .. });
    let edit = view.board_input.label.as_mut().unwrap();
    let empty = edit.text.is_empty();
    let text_edit = TextEdit::multiline(&mut edit.text)
        .id(edit_id)
        .font(font.clone())
        .text_color(text_color)
        .frame(egui::Frame::NONE)
        .align(egui::Align2::CENTER_CENTER)
        .desired_width(rect.width().max(40.0))
        .min_size(rect.size())
        .desired_rows(1)
        .return_key(Some(egui::KeyboardShortcut::new(
            Modifiers::SHIFT,
            Key::Enter,
        )))
        .event_filter(EventFilter {
            tab: true,
            horizontal_arrows: true,
            vertical_arrows: true,
            escape: true,
        });
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    if !input_only && empty {
        let painter = child
            .painter()
            .with_clip_rect(rect.intersect(ui.clip_rect()));
        let color = text_color.gamma_multiply(0.7);
        let galley =
            painter.layout_no_wrap(view.board_labels.label_placeholder.clone(), font, color);
        let position = rect.center() - galley.size() / 2.0;
        if is_shape {
            painter.add(label_text_halo(position, galley.clone(), color));
        }
        painter.galley(position, galley, color);
    }
    // Contrast around the glyphs avoids masking a transparent shape or its outline.
    let halo = (!input_only && is_shape && !empty).then(|| child.painter().add(egui::Shape::Noop));
    // Text queued with a finish key must reach the editor before the commit.
    let output = text_edit.show(&mut child);
    if let Some(index) = halo {
        let galley = ui.fonts_mut(|fonts| fonts.layout_job((*output.galley.job).clone()));
        let position = output.galley_pos - vec2(galley.rect.left(), 0.0);
        child
            .painter()
            .with_clip_rect(output.text_clip_rect)
            .set(index, label_text_halo(position, galley, text_color));
    }
    if !input_only {
        output.response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::Role::MultilineTextInput,
                true,
                &view.board_labels.label_editor,
            )
        });
        ui.ctx().accesskit_node_builder(output.response.id, |node| {
            node.set_description(hint.as_str());
        });
    }
    if commit {
        match view.try_commit_board_label(doc) {
            Ok(_) => ui.memory_mut(|memory| memory.request_focus(MapCanvas::id())),
            Err(error) => {
                *edit_error = Some(error);
                ui.memory_mut(|memory| memory.request_focus(edit_id));
            }
        }
    } else if !edit.focus_requested && !minimap_blocks_keyboard {
        edit.focus_requested = true;
        if ui.input(|input| input.pointer.any_pressed() || input.pointer.any_released()) {
            // egui may clear focus at the end of the opening click's pass.
            edit.focus_retry_after = Some((ui.ctx().viewport_id(), ui.ctx().cumulative_frame_nr()));
            ui.ctx().request_repaint();
        }
        ui.memory_mut(|memory| memory.request_focus(edit_id));
        let end = egui::text::CCursor::new(edit.text.chars().count());
        // Select an existing label for replacement, but never the character just typed.
        let start = if edit.seeded {
            end
        } else {
            egui::text::CCursor::new(0)
        };
        let mut state = output.state.clone();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(start, end)));
        state.store(ui.ctx(), edit_id);
    } else if output.response.lost_focus()
        && let Err(error) = view.try_commit_board_label(doc)
    {
        *edit_error = Some(error);
        if !minimap_blocks_keyboard {
            ui.memory_mut(|memory| memory.request_focus(edit_id));
        }
    }
}

#[cfg(test)]
mod label_editor_color_tests {
    use super::*;

    #[test]
    fn editor_contrasts_transparent_and_low_contrast_text_without_changing_readable_colors() {
        for background in [
            Color32::WHITE,
            Color32::BLACK,
            Color32::from_rgb(50, 80, 110),
        ] {
            for text in [
                Color32::TRANSPARENT,
                Color32::from_black_alpha(102),
                background,
            ] {
                let result = label_editor_text_color(text, background);
                let [r, g, b, a] = result.to_srgba_unmultiplied();
                let [br, bg, bb, _] = background.to_srgba_unmultiplied();
                assert!(
                    thinoda_core::Color::rgba(r, g, b, a)
                        .contrast_on(thinoda_core::Color::rgb(br, bg, bb))
                        >= 4.5
                );
            }
        }
        assert_eq!(
            label_editor_text_color(Color32::BLUE, Color32::WHITE),
            Color32::BLUE
        );
    }

    #[test]
    fn shape_editor_contrast_uses_the_visible_fill_after_object_opacity() {
        let sheet = Sheet::new("Board", "");
        for (dark, fill, text) in [
            (true, thinoda_core::Color::WHITE, Color32::WHITE),
            (false, thinoda_core::Color::BLACK, Color32::BLACK),
        ] {
            let style = thinoda_core::board::ObjectStyle {
                fill: Some(fill),
                text_color: Some(fill),
                opacity: 0.0,
                ..Default::default()
            };
            let (_, unfaded) = crate::render::board_label_colors(&sheet, &style, dark);
            let visible = crate::render::shape_label_editor_background(&sheet, &style, dark);
            assert_ne!(unfaded, visible);
            assert_ne!(label_editor_text_color(text, unfaded), text);
            assert_eq!(label_editor_text_color(text, visible), text);
            assert_eq!(visible, crate::render::sheet_background(&sheet, dark));

            let opaque = thinoda_core::board::ObjectStyle {
                opacity: 1.0,
                ..style
            };
            assert_eq!(
                crate::render::shape_label_editor_background(&sheet, &opaque, dark),
                crate::render::board_label_colors(&sheet, &opaque, dark).1
            );
        }
    }
}

#[cfg(test)]
mod identity_tests {
    use super::*;

    #[test]
    fn creation_and_label_guards_reject_a_reused_sheet_address() {
        let mut workbook = thinoda_core::Workbook::default();
        let object = workbook
            .active_mut()
            .add_canvas_object(CanvasObject::shape(
                thinoda_core::ShapeKind::Rectangle,
                thinoda_core::Rect::from_center_size(Vec2::ZERO, Vec2::new(100.0, 100.0)),
            ))
            .unwrap();
        let original = Document::new(workbook);
        let replacement = original.clone();
        let replacement_address = sheet_identity(&replacement).allocation;
        let mut view = MapView::default();
        let mut guard = GestureGuard::new(&original, &view);
        guard.sheet_identity.allocation = replacement_address;
        assert!(guard.changed(&replacement, &view));

        assert!(begin_label(&mut view, &original, object));
        let draft = view.board_input.label.as_mut().unwrap();
        draft.sheet_identity.allocation = replacement_address;
        assert!(validate_label_source(draft, &replacement).is_err());
    }
}

#[cfg(test)]
mod accessibility_index_tests {
    use super::*;
    use thinoda_core::Workbook;

    fn card(sheet: &mut Sheet, title: &str, x: f32) -> TopicId {
        sheet
            .add_card(
                title,
                Vec2::new(x, 0.0),
                Card::new(CardKind::Sticky, Vec2::new(120.0, 100.0)),
            )
            .unwrap()
    }

    fn shape(sheet: &mut Sheet, label: &str, x: f32) -> CanvasObjectId {
        let mut object = CanvasObject::shape(
            ShapeKind::Rectangle,
            thinoda_core::Rect::from_center_size(Vec2::new(x, 150.0), Vec2::new(100.0, 80.0)),
        );
        if let CanvasObjectKind::Shape { label: text, .. } = &mut object.kind {
            *text = label.into();
        }
        sheet.add_canvas_object(object).unwrap()
    }

    fn assert_mobility_matches_existing_path(sheet: &Sheet) {
        let index = AccessibilityIndex::new(sheet);
        for object in &sheet.canvas.objects {
            assert_eq!(
                index.object_is_unlocked(sheet, object),
                board_actions::member_is_movable(sheet, BoardMember::Object(object.id)),
                "object {}",
                object.id
            );
        }
        for id in sheet.topics.keys().copied() {
            assert_eq!(
                index.topic_is_unlocked(sheet, id),
                topic_is_unlocked(sheet, id),
                "topic {id}"
            );
        }
    }

    #[test]
    fn indexed_locks_match_direct_and_mixed_group_mobility() {
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let free = shape(&mut sheet, "Free", 0.0);
        let locked = shape(&mut sheet, "Locked", 150.0);
        let standalone_card = card(&mut sheet, "Locked card", 300.0);
        let ordinary = sheet.add_floating("Ordinary topic", Vec2::new(450.0, 0.0));
        sheet.add_child(ordinary, None, "Child");

        let mut pairs = Vec::new();
        for (number, x) in [600.0, 750.0, 900.0, 1050.0].into_iter().enumerate() {
            let card = card(&mut sheet, &format!("Card {number}"), x);
            let object = shape(&mut sheet, &format!("Shape {number}"), x);
            let group = sheet
                .group_board_members(&[BoardMember::Card(card), BoardMember::Object(object)])
                .unwrap();
            pairs.push((card, object, group));
        }
        sheet
            .set_board_members_locked(
                &[
                    BoardMember::Object(locked),
                    BoardMember::Card(standalone_card),
                    BoardMember::Object(pairs[1].1),
                    BoardMember::Card(pairs[2].0),
                ],
                true,
            )
            .unwrap();
        sheet.set_canvas_group_locked(pairs[3].2, true).unwrap();
        Workbook::new(sheet.clone()).validate().unwrap();
        assert_mobility_matches_existing_path(&sheet);

        let index = AccessibilityIndex::new(&sheet);
        assert!(index.object_is_unlocked(&sheet, sheet.canvas.object(free).unwrap()));
        assert!(!index.object_is_unlocked(&sheet, sheet.canvas.object(locked).unwrap()));
        assert!(!index.topic_is_unlocked(&sheet, standalone_card));
        assert!(index.topic_is_unlocked(&sheet, ordinary));
        assert!(index.topic_is_unlocked(&sheet, pairs[0].0));
        for (card, object, _) in &pairs[1..] {
            assert!(!index.topic_is_unlocked(&sheet, *card));
            assert!(!index.object_is_unlocked(&sheet, sheet.canvas.object(*object).unwrap()));
        }

        sheet
            .set_board_members_locked(
                &[
                    BoardMember::Object(pairs[1].1),
                    BoardMember::Card(pairs[2].0),
                ],
                false,
            )
            .unwrap();
        sheet.set_canvas_group_locked(pairs[3].2, false).unwrap();
        assert_mobility_matches_existing_path(&sheet);
        let refreshed = AccessibilityIndex::new(&sheet);
        for (card, object, _) in &pairs {
            assert!(refreshed.topic_is_unlocked(&sheet, *card));
            assert!(refreshed.object_is_unlocked(&sheet, sheet.canvas.object(*object).unwrap()));
        }
    }

    #[test]
    fn indexed_labels_preserve_selection_lock_and_connector_descriptions() {
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let card = card(&mut sheet, "Idea", 0.0);
        let object = shape(&mut sheet, "Visible label", 150.0);
        sheet.canvas.object_mut(object).unwrap().alt_text = "Described shape".into();
        let group = sheet
            .group_board_members(&[BoardMember::Card(card), BoardMember::Object(object)])
            .unwrap();
        sheet.set_canvas_group_locked(group, true).unwrap();
        let connector = sheet
            .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
                Endpoint::Topic(card),
                Endpoint::Object(object),
            )))
            .unwrap();
        let frame = sheet
            .create_board_frame(
                "Area",
                thinoda_core::Rect::from_center_size(Vec2::ZERO, Vec2::new(500.0, 400.0)),
            )
            .unwrap();
        Workbook::new(sheet.clone()).validate().unwrap();
        let index = AccessibilityIndex::new(&sheet);
        let view = MapView {
            selected_object: Some(ObjectSelection::Zone(frame)),
            ..Default::default()
        };
        let selected_topics = HashSet::from([card]);
        let selected_objects = HashSet::from([object, connector]);
        let label = |id| {
            item_label(
                &sheet,
                &view,
                &index,
                &selected_topics,
                &selected_objects,
                id,
            )
        };
        assert_eq!(
            label(SceneId::Topic(card)),
            Some(("Sticky note: Idea, Selected, Locked".into(), true))
        );
        assert_eq!(
            label(SceneId::Object(object)),
            Some(("Rectangle: Described shape, Selected, Locked".into(), true))
        );
        assert_eq!(
            label(SceneId::Object(connector)),
            Some((
                "Connector, From: Idea, To: Described shape, Selected".into(),
                true
            ))
        );
        assert_eq!(
            label(SceneId::Zone(frame)),
            Some(("Frame: Area, Selected".into(), true))
        );
        assert_eq!(label(SceneId::Topic(sheet.root)), None);
    }

    #[test]
    fn indexed_lookups_keep_first_matches_for_rejected_duplicate_ids() {
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let object = shape(&mut sheet, "First shape", 0.0);
        let mut duplicate = sheet.canvas.object(object).unwrap().clone();
        duplicate.alt_text = "Second shape".into();
        sheet.canvas.objects.push(duplicate);
        let frame = sheet
            .create_board_frame(
                "First frame",
                thinoda_core::Rect::from_center_size(Vec2::ZERO, Vec2::new(300.0, 300.0)),
            )
            .unwrap();
        let mut duplicate = sheet.zones[0].clone();
        duplicate.name = "Second frame".into();
        sheet.zones.push(duplicate);
        let card = card(&mut sheet, "Card", 300.0);
        sheet.canvas.card_states.extend([
            CanvasCardState {
                topic: card,
                group: None,
                locked: false,
            },
            CanvasCardState {
                topic: card,
                group: None,
                locked: true,
            },
        ]);
        assert!(Workbook::new(sheet.clone()).validate().is_err());
        let index = AccessibilityIndex::new(&sheet);
        assert_eq!(index.objects[&object].label(), "First shape");
        assert_eq!(index.zones[&frame].name, "First frame");
        assert!(!index.card_states[&card].locked);
        assert!(index.duplicate_ids);
        assert_mobility_matches_existing_path(&sheet);
    }

    #[test]
    fn duplicate_group_members_use_existing_first_match_mobility() {
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let first = shape(&mut sheet, "First", 0.0);
        let peer = shape(&mut sheet, "Peer", 150.0);
        let first_group = CanvasGroupId(1000);
        let second_group = CanvasGroupId(1001);
        sheet.canvas.groups.extend([
            thinoda_core::board::CanvasGroup {
                id: first_group,
                locked: false,
            },
            thinoda_core::board::CanvasGroup {
                id: second_group,
                locked: false,
            },
        ]);
        sheet.canvas.objects[0].group = Some(first_group);
        sheet.canvas.objects[1].group = Some(second_group);
        let mut duplicate = sheet.canvas.objects[0].clone();
        duplicate.group = Some(second_group);
        duplicate.locked = true;
        sheet.canvas.objects.push(duplicate);
        assert!(Workbook::new(sheet.clone()).validate().is_err());
        assert_mobility_matches_existing_path(&sheet);
        let index = AccessibilityIndex::new(&sheet);
        assert!(index.duplicate_ids);
        assert!(index.object_is_unlocked(&sheet, sheet.canvas.object(first).unwrap()));
        assert!(index.object_is_unlocked(&sheet, sheet.canvas.object(peer).unwrap()));
    }
}

#[cfg(test)]
mod alignment_tests {
    use super::*;
    use thinoda_core::{ApproxMeasure, ShapeKind, Zone, ZoneId, layout_sheet, scene::build_scene};

    fn shape(sheet: &mut Sheet, center: Vec2) -> CanvasObjectId {
        sheet
            .add_canvas_object(CanvasObject::shape(
                ShapeKind::Rectangle,
                thinoda_core::Rect::from_center_size(center, Vec2::new(100.0, 100.0)),
            ))
            .unwrap()
    }

    #[test]
    fn guides_use_mixed_group_bounds_and_exclude_selected_items() {
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let card = sheet
            .add_card(
                "Card",
                Vec2::ZERO,
                Card::new(CardKind::Sticky, Vec2::new(100.0, 100.0)),
            )
            .unwrap();
        let object = shape(&mut sheet, Vec2::new(120.0, 0.0));
        let reference = shape(&mut sheet, Vec2::new(400.0, 250.0));
        sheet
            .group_board_members(&[BoardMember::Card(card), BoardMember::Object(object)])
            .unwrap();
        let layout = layout_sheet(&sheet, &ApproxMeasure, false);
        let scene = build_scene(&sheet, &layout);
        let mut view = MapView::default();
        view.board_selection.push(object);
        let movable = board_actions::movable_board_members(&sheet, &view);
        assert_eq!(movable.len(), 2);
        let group_bounds = scene
            .item(SceneId::Topic(card))
            .unwrap()
            .bounds
            .union(&scene.item(SceneId::Object(object)).unwrap().bounds);
        let target = scene.item(SceneId::Object(reference)).unwrap().bounds;
        let delta = Vec2::new(target.center().x - group_bounds.center().x, 0.0);
        let before = sheet.clone();
        let guides = drag_alignment_guides(&sheet, &view, &scene, &movable, delta, 4.0);
        assert_eq!(guides.alignment[0].unwrap()[0].x, target.center().x);
        assert!(guides.alignment[1].is_none());
        assert_eq!(sheet, before, "guides are visual feedback, not snapping");

        view.board_selection.push(reference);
        let movable = board_actions::movable_board_members(&sheet, &view);
        assert_eq!(
            drag_alignment_guides(&sheet, &view, &scene, &movable, delta, 4.0),
            crate::topic_drag::DragGuides::default()
        );
    }

    #[test]
    fn locked_group_does_not_produce_drag_guides() {
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let first = shape(&mut sheet, Vec2::ZERO);
        let second = shape(&mut sheet, Vec2::new(120.0, 0.0));
        shape(&mut sheet, Vec2::new(400.0, 250.0));
        sheet.group_canvas_objects(&[first, second]).unwrap();
        sheet
            .set_board_members_locked(&[BoardMember::Object(first)], true)
            .unwrap();
        let layout = layout_sheet(&sheet, &ApproxMeasure, false);
        let scene = build_scene(&sheet, &layout);
        let mut view = MapView::default();
        view.board_selection.push(second);
        let movable = board_actions::movable_board_members(&sheet, &view);
        assert!(movable.is_empty());
        assert_eq!(
            drag_alignment_guides(&sheet, &view, &scene, &movable, Vec2::ZERO, 4.0),
            crate::topic_drag::DragGuides::default()
        );
    }

    #[test]
    fn mixed_card_object_drag_gets_clipped_size_and_spacing_guides() {
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let card = sheet
            .add_card(
                "Card",
                Vec2::ZERO,
                Card::new(CardKind::Sticky, Vec2::new(100.0, 100.0)),
            )
            .unwrap();
        let selected_object = sheet
            .add_canvas_object(CanvasObject::shape(
                ShapeKind::Rectangle,
                thinoda_core::Rect::from_center_size(Vec2::new(115.0, 0.0), Vec2::new(30.0, 30.0)),
            ))
            .unwrap();
        sheet
            .group_board_members(&[
                BoardMember::Card(card),
                BoardMember::Object(selected_object),
            ])
            .unwrap();
        let left = sheet
            .add_canvas_object(CanvasObject::shape(
                ShapeKind::Rectangle,
                thinoda_core::Rect::from_center_size(Vec2::new(-90.0, 0.0), Vec2::new(20.0, 20.0)),
            ))
            .unwrap();
        let right = sheet
            .add_canvas_object(CanvasObject::shape(
                ShapeKind::Rectangle,
                thinoda_core::Rect::from_center_size(Vec2::new(170.0, 0.0), Vec2::new(20.0, 20.0)),
            ))
            .unwrap();
        let clipped_reference = sheet
            .add_canvas_object(CanvasObject::shape(
                ShapeKind::Rectangle,
                thinoda_core::Rect::from_center_size(
                    Vec2::new(500.0, 0.0),
                    Vec2::new(220.0, 140.0),
                ),
            ))
            .unwrap();
        sheet.zones.push(Zone {
            id: ZoneId(77),
            name: "Guide clip".into(),
            frame: thinoda_core::Rect::from_center_size(
                Vec2::new(500.0, 0.0),
                Vec2::new(180.0, 100.0),
            ),
            roots: vec![],
            objects: vec![],
            keep_empty: true,
            color: None,
            background_color: None,
            clip: true,
        });

        let layout = layout_sheet(&sheet, &ApproxMeasure, false);
        let scene = build_scene(&sheet, &layout);
        let mut view = MapView::default();
        view.board_selection.push(selected_object);
        let movable = board_actions::movable_board_members(&sheet, &view);
        let guides = drag_alignment_guides(&sheet, &view, &scene, &movable, Vec2::ZERO, 4.0);

        assert_eq!(
            guides.equal_spacing[0],
            Some([
                scene.item(SceneId::Object(left)).unwrap().bounds,
                scene
                    .item(SceneId::Topic(card))
                    .unwrap()
                    .bounds
                    .union(&scene.item(SceneId::Object(selected_object)).unwrap().bounds),
                scene.item(SceneId::Object(right)).unwrap().bounds,
            ])
        );
        assert_eq!(
            guides.matching_size[0].map(|bounds| bounds[1]),
            scene
                .item(SceneId::Object(clipped_reference))
                .unwrap()
                .visible_bounds()
        );
        assert!(
            guides.matching_size[1].is_some(),
            "clipped visible height matches the moving group"
        );
        assert_eq!(
            guides.matching_size[0].unwrap()[0],
            scene
                .item(SceneId::Topic(card))
                .unwrap()
                .bounds
                .union(&scene.item(SceneId::Object(selected_object)).unwrap().bounds)
        );
    }

    #[test]
    fn moving_objects_use_their_post_drag_frame_visible_bounds() {
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let id = sheet
            .add_canvas_object(CanvasObject::shape(
                ShapeKind::Rectangle,
                thinoda_core::Rect::from_center_size(Vec2::ZERO, Vec2::new(100.0, 40.0)),
            ))
            .unwrap();
        sheet.zones.extend([
            Zone {
                id: ZoneId(10),
                name: "Original clip".into(),
                frame: thinoda_core::Rect::from_center_size(Vec2::ZERO, Vec2::new(60.0, 40.0)),
                roots: vec![],
                objects: vec![],
                keep_empty: true,
                color: None,
                background_color: None,
                clip: true,
            },
            Zone {
                id: ZoneId(11),
                name: "Destination clip".into(),
                frame: thinoda_core::Rect::from_center_size(
                    Vec2::new(100.0, 0.0),
                    Vec2::new(40.0, 40.0),
                ),
                roots: vec![],
                objects: vec![],
                keep_empty: true,
                color: None,
                background_color: None,
                clip: true,
            },
        ]);
        let layout = layout_sheet(&sheet, &ApproxMeasure, false);
        let scene = build_scene(&sheet, &layout);
        let item = scene.item(SceneId::Object(id)).unwrap();
        assert_eq!(
            moved_visible_bounds(&sheet, item, Vec2::new(20.0, 0.0)),
            Some(thinoda_core::Rect::from_min_size(
                Vec2::new(-30.0, -20.0),
                Vec2::new(60.0, 40.0)
            ))
        );
        assert_eq!(
            moved_visible_bounds(&sheet, item, Vec2::new(100.0, 0.0)),
            Some(thinoda_core::Rect::from_min_size(
                Vec2::new(80.0, -20.0),
                Vec2::new(40.0, 40.0)
            )),
            "moving into another frame recomputes the applicable clip"
        );
    }
}
