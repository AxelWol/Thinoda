//! View-only board gestures. Document edits happen once, on release.

use super::*;
use crate::actions::{BoardAction, apply_board_action, board_actions};
use std::collections::{HashMap, HashSet, VecDeque};
use thinoda_core::{
    CanvasConnector, CanvasObject, CanvasObjectKind, Card, CardKind, EditError, EditResult,
    Endpoint, Scene, SceneId, SheetRootMode, ZOrder, Zone, ZoneId,
    board::{
        BoardMember, CanvasCardState, CanvasGroupId, MIN_CARD_SIZE, rotate_around, rotated_corners,
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
}

pub(super) struct LabelEdit {
    pub(super) id: CanvasObjectId,
    text: String,
    original: String,
    sheet: usize,
    sheet_identity: SheetIdentity,
    focus_requested: bool,
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum ControlKind {
    Resize,
    Rotate,
    From,
    To,
}

#[derive(Clone, Copy)]
struct Control {
    item: SceneId,
    kind: ControlKind,
    point: Vec2,
    rect: thinoda_core::Rect,
    rotation: f32,
}

fn control_id(control: Control) -> Id {
    let part = match control.kind {
        ControlKind::Resize => 0,
        ControlKind::Rotate => 1,
        ControlKind::From => 2,
        ControlKind::To => 3,
    };
    MapCanvas::id().with(("board-control", control.item, part))
}

fn controls(view: &MapView, sheet: &Sheet, layout: &Layout, scene: &Scene) -> Vec<Control> {
    if view.board_tool != BoardTool::Select
        || view.editing.is_some()
        || view.editing_board_object().is_some()
        || view.selected_object.is_some()
        || !matches!(view.drag, Drag::None)
    {
        return Vec::new();
    }
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
        if object.is_connector() {
            let Some(SceneGeometry::Polyline { points, .. }) =
                scene.item(item).map(|item| &item.geometry)
            else {
                return Vec::new();
            };
            let (Some(from), Some(to)) = (points.first(), points.last()) else {
                return Vec::new();
            };
            return [(ControlKind::From, *from), (ControlKind::To, *to)]
                .into_iter()
                .filter(|(_, point)| {
                    scene.item(item).is_none_or(|scene_item| {
                        scene_item.clip.is_none_or(|clip| clip.contains(*point))
                    })
                })
                .map(|(kind, point)| Control {
                    item,
                    kind,
                    point,
                    rect: object.rect,
                    rotation: 0.0,
                })
                .collect();
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
    let top = rotate_around(rect.top_center(), rect.center(), rotation);
    let outward = rotate_around(
        rect.top_center() - Vec2::new(0.0, 25.0 / view.zoom),
        rect.center(),
        rotation,
    );
    let clip = scene.item(item).and_then(|item| item.clip);
    [
        (ControlKind::Resize, corner),
        (ControlKind::Rotate, outward),
    ]
    .into_iter()
    .filter(|(kind, point)| {
        clip.is_none_or(|clip| {
            clip.contains(*point) || (*kind == ControlKind::Rotate && clip.contains(top))
        })
    })
    .map(|(kind, point)| Control {
        item,
        kind,
        point,
        rect,
        rotation,
    })
    .collect()
}

fn control_label(view: &MapView, kind: ControlKind) -> String {
    match kind {
        ControlKind::Resize => view.board_labels.resize.clone(),
        ControlKind::Rotate => view.board_labels.rotate.clone(),
        ControlKind::From => format!("{} {}", view.board_labels.connector, view.board_labels.from),
        ControlKind::To => format!("{} {}", view.board_labels.connector, view.board_labels.to),
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
) -> EditResult<()> {
    if !board_actions::member_is_movable(doc.sheet(), BoardMember::Object(id)) {
        return Err(EditError::Invalid("connector is locked".into()));
    }
    doc.update_canvas_object("Reconnect connector", id, |object| {
        if let CanvasObjectKind::Connector(connector) = &mut object.kind {
            if start {
                connector.from = endpoint;
            } else {
                connector.to = endpoint;
            }
        }
    })
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
            let controls = controls(view, doc.sheet(), layout, scene);
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
    // Let the canvas own Alt+arrow duplication/rotation even when a handle is
    // focused. `consume_key` otherwise accepts extra Alt modifiers.
    if alt_arrow_pressed(ui) {
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
            set_endpoint(doc, id, control.kind == ControlKind::From, endpoint).map(|()| true)
        }
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
        return view.board_input.is_active();
    }
    let mut blocked = view.board_input.is_active();
    for control in controls(view, doc.sheet(), layout, scene) {
        let rect = Rect::from_center_size(view.xf().pos(control.point), vec2(28.0, 28.0));
        if !view.canvas_rect.intersects(rect) {
            continue;
        }
        let response = ui.interact(
            rect,
            control_id(control),
            Sense::click_and_drag() | Sense::FOCUSABLE,
        );
        let label = control_label(view, control.kind);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
        blocked |= response.hovered()
            || response.is_pointer_button_down_on()
            || ui
                .input(|input| input.pointer.press_origin())
                .is_some_and(|point| rect.contains(point));
        if response.is_pointer_button_down_on()
            && ui.input(|input| input.pointer.primary_pressed())
            && view.board_input.gesture.is_none()
        {
            response.request_focus();
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
            };
            view.board_input.gesture = Some(gesture(doc, view, kind, start));
            blocked = true;
        }
        if response.has_focus() && view.board_input.gesture.is_none() {
            view.board_input.control_focused = true;
            keyboard_control(ui, doc, view, layout, scene, control, out);
        }
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
                    ..
                } => release = Some(*pos),
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
            GestureKind::Endpoint { point: p, .. } => *p = current,
        }
    }
    if release.is_some() {
        if !drag.canceled
            && drag.moved
            && pointer.is_some_and(|point| view.canvas_rect.contains(point))
        {
            let duplicating = matches!(
                drag.kind,
                GestureKind::Move {
                    duplicate: true,
                    ..
                }
            );
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
                    set_endpoint(doc, id, start, end).map(|()| true)
                }
            };
            match result {
                Ok(true) if duplicating => focus_duplicated_selection(ui, view),
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
    let ctx = ui.ctx().clone();
    if view.board_input.last_tool != view.board_tool {
        view.board_input.cancel_gesture();
        view.board_input.last_tool = view.board_tool;
    }
    if view.board_input.connector.as_ref().is_some_and(|pending| {
        pending.sheet != doc.active_index()
            || pending.sheet_identity != sheet_identity(doc)
            || pending.state != doc.state_id()
    }) {
        view.board_input.connector = None;
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
    if tool != BoardTool::Select {
        let mut started_here = false;
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
        controls(view, doc.sheet(), layout, scene)
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
            let first = controls(view, doc.sheet(), layout, scene)
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
    if view.board_tool != BoardTool::Select && view.board_tool != BoardTool::Hand {
        if ctx.input_mut(|input| {
            input.consume_key(Modifiers::NONE, Key::Enter)
                || input.consume_key(Modifiers::NONE, Key::Space)
        }) && prepare_edit(doc, view, out)
        {
            let point = view.xf().sheet(view.canvas_rect.center());
            if let Err(error) = create_at(doc, view, view.board_tool, point) {
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
        painter.circle_filled(center, 6.0, sheet_background(sheet, view.dark));
        painter.circle_stroke(center, 6.0, Stroke::new(2.0, view.accent));
        if painter
            .ctx()
            .memory(|memory| memory.has_focus(control_id(control)))
        {
            painter.circle_stroke(center, 9.0, Stroke::new(2.0, view.accent));
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
    }
}

pub(super) fn show_label_editor(
    ui: &mut Ui,
    doc: &mut Document,
    view: &mut MapView,
    scene: &Scene,
    out: &mut CanvasOutput,
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
    let rect = match &object.kind {
        CanvasObjectKind::Shape { shape, .. } => {
            let label_rect = thinoda_core::board::shape_label_rect(*shape, object.rect);
            thinoda_core::Rect::from_center_size(
                rotate_around(label_rect.center(), object.rect.center(), object.rotation),
                label_rect.size,
            )
        }
        CanvasObjectKind::Connector(_) => {
            let point = match &item.geometry {
                SceneGeometry::Polyline { points, .. } => points[points.len() / 2],
                _ => item.bounds.center(),
            };
            thinoda_core::Rect::from_center_size(point, Vec2::new(140.0, 40.0))
        }
        CanvasObjectKind::Ink(_) => return,
    };
    let rect = view.xf().rect(rect).expand(2.0);
    let edit_id = MapCanvas::id().with("board-label-edit");
    if !out.minimap_blocks_keyboard
        && let Some(edit) = &mut view.board_input.label
    {
        protect_ime_candidate_keys(ui.ctx(), edit_id, &mut edit.ime_composing);
    }
    let (commit, cancel) = if out.minimap_blocks_keyboard {
        (false, false)
    } else {
        ui.input_mut(|input| {
            (
                input.consume_key(Modifiers::NONE, Key::Enter)
                    || input.consume_key(Modifiers::NONE, Key::Tab),
                input.consume_key(Modifiers::NONE, Key::Escape),
            )
        })
    };
    if cancel {
        view.board_input.label = None;
        ui.memory_mut(|memory| memory.request_focus(MapCanvas::id()));
        return;
    }
    let background = object
        .style
        .fill
        .map(color32)
        .unwrap_or_else(|| sheet_background(doc.sheet(), view.dark));
    ui.painter().rect_filled(rect, 2.0, background);
    let edit = view.board_input.label.as_mut().unwrap();
    let text_edit = TextEdit::multiline(&mut edit.text)
        .id(edit_id)
        .font(FontId::proportional((12.0 * view.zoom).max(10.0)))
        .frame(egui::Frame::NONE)
        .desired_width(rect.width().max(40.0))
        .desired_rows(1)
        .return_key(Some(egui::KeyboardShortcut::new(
            Modifiers::SHIFT,
            Key::Enter,
        )))
        .lock_focus(true);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    // Text queued with a finish key must reach the editor before the commit.
    let output = text_edit.show(&mut child);
    output.response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::MultilineTextInput,
            true,
            &view.board_labels.label_editor,
        )
    });
    if commit {
        match view.try_commit_board_label(doc) {
            Ok(_) => ui.memory_mut(|memory| memory.request_focus(MapCanvas::id())),
            Err(error) => {
                out.error = Some(error);
                ui.memory_mut(|memory| memory.request_focus(edit_id));
            }
        }
    } else if !edit.focus_requested && !out.minimap_blocks_keyboard {
        edit.focus_requested = true;
        ui.memory_mut(|memory| memory.request_focus(edit_id));
        let end = edit.text.chars().count();
        let mut state = output.state.clone();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(end),
            )));
        state.store(ui.ctx(), edit_id);
    } else if output.response.lost_focus()
        && let Err(error) = view.try_commit_board_label(doc)
    {
        out.error = Some(error);
        if !out.minimap_blocks_keyboard {
            ui.memory_mut(|memory| memory.request_focus(edit_id));
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
