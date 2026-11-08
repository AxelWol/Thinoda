//! Interactive mind map canvas widget.

use std::sync::Arc;

use egui::{
    Color32, CornerRadius, EventFilter, FontId, Id, Key, Modifiers, PointerButton, Pos2, Rect,
    Response, Sense, Stroke, StrokeKind, TextEdit, Ui, Vec2 as EVec2, pos2, vec2,
};
use thinoda_core::{
    ApproxMeasure, BoundaryId, CanvasObjectId, Document, Layout, RelationshipId, ShapeKind, Sheet,
    SummaryId, TopicId,
    board::{rotate_around, rotated_bounds},
    clipboard::{BoardClipboardPayload, ClipboardPayload},
    geometry::Vec2,
    layout::{Dir, Icon, TopicLayout, cubic as bezier},
    layout_sheet,
    navigation::Direction,
    scene::{Scene, SceneHit, SceneId, build_scene, topic_clip_rect},
};

use crate::{
    actions::{Action, apply_action},
    ink_controller::{
        InkController, InkControllerError, InkMode, InkOutcome, InkRecoveryBlock, InkSettings,
        RecoveredInkGesture, RecoveredPointer,
    },
    ink_input::{InkSample, StrokeFinishOptions},
    measure::EguiMeasure,
    minimap::{
        DEFAULT_MINIMAP_SIZE, MinimapCamera, MinimapLabels, MinimapScene, MinimapState,
        bottom_right_rect, show_minimap,
    },
    render::{
        PaintOptions, PaintSceneExtras, Xf, brace_path, color32, paint_map, paint_map_scene,
        sheet_background, topic_badge_center,
    },
};

pub const MIN_ZOOM: f32 = 0.1;
pub const MAX_ZOOM: f32 = 5.0;

// The compact board toolbox occupies roughly 62 screen points from the left
// edge (outer offset, buttons, and frame). Keep this in sync with the host's
// default collapsed toolbox; the regular fit padding is applied separately.
const BOARD_TOOLBOX_SAFE_INSET: f32 = 64.0;

#[path = "image_resize.rs"]
mod image_resize;

#[path = "board_input.rs"]
pub(crate) mod board_input;

#[path = "ink_view.rs"]
mod ink_view;

#[cfg(test)]
#[path = "ink_view_tests.rs"]
mod ink_view_tests;

#[cfg(test)]
#[path = "minimap_view_tests.rs"]
mod minimap_view_tests;

/// Active freeform tool. Hosts may change it with [`MapView::set_board_tool`].
///
/// The default, `Select`, preserves the normal mind-map interaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BoardTool {
    #[default]
    Select,
    Hand,
    Sticky,
    Text,
    Shape(ShapeKind),
    Connector,
    Frame,
    Pen,
    Highlighter,
    ObjectEraser,
    PixelEraser,
    Lasso,
}

/// Drag-and-drop payload of a shape dragged from a host's shape library.
///
/// Set it with [`egui::Response::dnd_set_drag_payload`]; releasing it over
/// empty board space creates the shape there as one undoable edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShapeDrop(pub ShapeKind);

impl BoardTool {
    fn ink_mode(self) -> Option<InkMode> {
        match self {
            Self::Pen => Some(InkMode::Pen),
            Self::Highlighter => Some(InkMode::Highlighter),
            Self::ObjectEraser => Some(InkMode::ObjectEraser),
            Self::PixelEraser => Some(InkMode::PixelEraser),
            Self::Lasso => Some(InkMode::Lasso),
            _ => None,
        }
    }
}

/// Localizable names used by the board's accessible objects and controls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardLabels {
    pub sticky: String,
    pub text: String,
    pub image: String,
    /// Generic fallback when a shape has no entry in `shape_names`.
    pub shape: String,
    /// Localized, human-readable names keyed by the core shape kind.
    pub shape_names: std::collections::HashMap<ShapeKind, String>,
    pub connector: String,
    pub ink: String,
    pub frame: String,
    pub new_frame: String,
    pub selected: String,
    pub locked: String,
    pub from: String,
    pub to: String,
    pub free_point: String,
    pub resize: String,
    pub rotate: String,
    pub waypoint: String,
    pub insert_waypoint: String,
    pub waypoint_hint: String,
    pub insert_waypoint_hint: String,
    pub connect_up: String,
    pub connect_right: String,
    pub connect_down: String,
    pub connect_left: String,
    /// Tooltip line explaining the direction-arrow modifiers.
    pub connect_hint: String,
    /// Shape-picker footer line explaining how to open it.
    pub quick_hint: String,
    pub label_editor: String,
    pub label_editing: String,
    pub label_edit_hint: String,
    pub label_edit_armed_hint: String,
    pub label_placeholder: String,
}

impl Default for BoardLabels {
    fn default() -> Self {
        Self {
            sticky: "Sticky note".into(),
            text: "Text card".into(),
            image: "Image card".into(),
            shape: "Shape".into(),
            shape_names: ShapeKind::ALL
                .into_iter()
                .map(|shape| {
                    let mut name = shape.id().replace('-', " ");
                    if let Some(first) = name.get_mut(0..1) {
                        first.make_ascii_uppercase();
                    }
                    (shape, name)
                })
                .collect(),
            connector: "Connector".into(),
            ink: "Ink stroke".into(),
            frame: "Frame".into(),
            new_frame: "Frame".into(),
            selected: "Selected".into(),
            locked: "Locked".into(),
            from: "From".into(),
            to: "To".into(),
            free_point: "Free point".into(),
            resize: "Resize board item".into(),
            rotate: "Rotate board item".into(),
            waypoint: "Connector waypoint".into(),
            insert_waypoint: "Add connector waypoint".into(),
            waypoint_hint: "Drag or use arrow keys to move; double-click or press Delete to remove.".into(),
            insert_waypoint_hint: "Drag, press Enter, or use arrow keys to add a waypoint.".into(),
            connect_up: "Add connected shape above".into(),
            connect_right: "Add connected shape to the right".into(),
            connect_down: "Add connected shape below".into(),
            connect_left: "Add connected shape to the left".into(),
            connect_hint: "Click adds a connected copy; Ctrl+click adds only a copy; Shift+click adds only a connector. Drag to connect to another item; Ctrl+drag drops a connected copy. Use Command on macOS.".into(),
            quick_hint: "Hold an arrow, or press Shift+F10 on a focused arrow, to choose the shape.".into(),
            label_editor: "Board object label".into(),
            label_editing: "Editing label".into(),
            label_edit_hint: "Enter or click/tap outside to finish · Shift+Enter: new line · Esc: cancel".into(),
            label_edit_armed_hint: "Clicking or tapping empty canvas also adds the armed shape.".into(),
            label_placeholder: "Type a label".into(),
        }
    }
}

/// Call before handling focused-editor shortcuts so IME candidate keys stay in composition.
pub fn protect_ime_candidate_keys(ctx: &egui::Context, editor_id: Id, composing: &mut bool) {
    let ime_frame = ctx.input(|input| {
        let mut active = *composing;
        for event in &input.events {
            if let egui::Event::Ime(event) = event {
                #[allow(deprecated)]
                match event {
                    egui::ImeEvent::Preedit { text, .. } => {
                        active = true;
                        *composing = !text.is_empty();
                    }
                    egui::ImeEvent::Commit(_) | egui::ImeEvent::Disabled => {
                        active = true;
                        *composing = false;
                    }
                    _ => {}
                }
            }
        }
        active
    });
    if ime_frame {
        let candidate_key = ctx.input_mut(|input| {
            let mut consumed = false;
            for modifiers in [Modifiers::NONE, Modifiers::SHIFT] {
                for key in [Key::Enter, Key::Tab, Key::Escape] {
                    consumed |= input.consume_key(modifiers, key);
                }
            }
            consumed
        });
        if candidate_key {
            ctx.memory_mut(|memory| memory.request_focus(editor_id));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectSelection {
    Relationship(RelationshipId),
    Boundary(BoundaryId),
    Summary(SummaryId),
    Zone(thinoda_core::ZoneId),
}

impl ObjectSelection {
    pub fn exists(self, sheet: &Sheet) -> bool {
        match self {
            Self::Relationship(id) => sheet.relationships.iter().any(|r| r.id == id),
            Self::Boundary(id) => sheet.boundaries.iter().any(|b| b.id == id),
            Self::Summary(id) => sheet.summaries.iter().any(|s| s.id == id),
            Self::Zone(id) => sheet.zones.iter().any(|zone| zone.id == id),
        }
    }
}

/// Default titles for newly created topics (localizable by the host app).
#[derive(Debug, Clone, PartialEq)]
pub struct NewTopicTitles {
    pub main: String,
    pub sub: String,
    pub floating: String,
    pub summary: String,
}

impl Default for NewTopicTitles {
    fn default() -> Self {
        Self {
            main: "Main Topic".into(),
            sub: "Subtopic".into(),
            floating: "Floating Topic".into(),
            summary: "Summary".into(),
        }
    }
}

/// Localizable accessible names and tooltips for the canvas add controls.
#[derive(Debug, Clone, PartialEq)]
pub struct AddControlLabels {
    pub child: String,
    pub sibling: String,
}

impl Default for AddControlLabels {
    fn default() -> Self {
        Self {
            child: "Add child topic".into(),
            sibling: "Add sibling topic".into(),
        }
    }
}

#[derive(Debug, Clone)]
struct EditState {
    topic: TopicId,
    text: String,
    original: String,
    select_all: bool,
    focus_requested: bool,
    focus_retry_after: Option<(egui::ViewportId, u64)>,
    ime_composing: bool,
    companion_cursor: Option<(String, Option<egui::text::CCursorRange>)>,
}

/// An existing title edit that may be resumed after its text was committed.
#[derive(Debug, Clone)]
pub struct TitleEditContinuation {
    edit: EditState,
    serial: u64,
}

#[derive(Debug, Clone)]
enum Drag {
    None,
    Pan,
    Rubber {
        start: Pos2,
        additive: bool,
    },
    Topics {
        ids: Vec<TopicId>,
        members: std::collections::BTreeSet<TopicId>,
        selection: Vec<TopicId>,
        grab: Vec2,
        current: Vec2,
        state: u64,
        sheet: usize,
    },
    Zone {
        id: thinoda_core::ZoneId,
        frame: thinoda_core::Rect,
        members: std::collections::BTreeSet<TopicId>,
        board_members: Vec<thinoda_core::board::BoardMember>,
        moving_topics: std::collections::BTreeSet<TopicId>,
        grab: Vec2,
        current: Vec2,
        state: u64,
        sheet: usize,
        camera: (EVec2, f32),
        resize: bool,
    },
}

#[derive(Clone)]
struct LayoutCache {
    key: (u64, usize, bool, u64),
    font_key: u64,
    layout: Arc<Layout>,
}

struct MinimapCache {
    layout: Arc<Layout>,
    scene: MinimapScene,
}

/// View state of a canvas (camera, selection, editing). Not part of the document.
pub struct MapView {
    pub offset: EVec2,
    pub zoom: f32,
    pub selection: Vec<TopicId>,
    /// Selected free canvas objects. Floating cards remain in [`Self::selection`].
    pub board_selection: Vec<CanvasObjectId>,
    /// Current host-selected board tool.
    pub board_tool: BoardTool,
    /// Settings for subsequently started ink gestures.
    pub ink_settings: InkSettings,
    pub board_labels: BoardLabels,
    pub selected_object: Option<ObjectSelection>,
    pub context_marker: Option<(TopicId, usize)>,
    /// Sheet position of the latest canvas secondary click, used by context actions.
    pub context_point: Option<Vec2>,
    /// Source topic while the user is picking the target of a new relationship.
    pub relationship_from: Option<TopicId>,
    pub relationship_handle_labels: [String; 4],
    pub(crate) relationship_drag: Option<crate::relationship_editing::RelationshipDrag>,
    pub titles: NewTopicTitles,
    pub add_control_labels: AddControlLabels,
    /// Accessible name and tooltip for the selected image's resize handle.
    pub image_resize_label: String,
    /// Whether the canvas exposes connector add buttons (keyboard actions remain available).
    pub show_add_controls: bool,
    /// View-only alignment feedback during free-placement topic drags.
    pub show_alignment_guides: bool,
    /// Show the interactive board overview. Off by default until the host exposes a toggle.
    pub show_minimap: bool,
    /// Host-localizable accessible name, tooltip, and offscreen-viewport description.
    pub minimap_labels: MinimapLabels,
    /// Internal clipboard (keeps full fidelity when pasting inside Thinoda).
    pub clipboard: Option<ClipboardPayload>,
    /// Internal board clipboard, retained when switching sheets.
    pub(crate) board_clipboard: Option<BoardClipboardPayload>,
    /// Text the host should place on the OS clipboard.
    pub clipboard_out: Option<String>,
    /// Ask the host to handle canvas copy/cut/paste rather than writing plain
    /// text directly. Requests are returned in [`CanvasOutput::clipboard_requests`].
    pub host_handles_clipboard: bool,
    /// Topics shown un-dimmed (search/filter); `None` shows everything.
    pub filter: Option<Vec<TopicId>>,
    pub dark: bool,
    pub accent: Color32,
    editing: Option<EditState>,
    title_edit_serial: u64,
    title_companion_frame: Option<((egui::ViewportId, u64), bool)>,
    title_error: Option<String>,
    drag: Drag,
    image_resize: Option<image_resize::Gesture>,
    board_input: board_input::State,
    ink_controller: InkController,
    ink_events_frame: Option<(egui::ViewportId, u64)>,
    // Some(false) keeps an unowned/canceled multitouch sequence blocked until all fingers lift.
    touch_navigation: Option<bool>,
    cache: Option<LayoutCache>,
    minimap_cache: Option<MinimapCache>,
    minimap_state: MinimapState,
    pending_fit: bool,
    pending_reveal: Option<TopicId>,
    pending_board_reveal: Option<CanvasObjectId>,
    initialized: bool,
    explicit_initial_zoom: bool,
    canvas_rect: Rect,
}

impl Default for MapView {
    fn default() -> Self {
        Self {
            offset: EVec2::ZERO,
            zoom: 1.0,
            selection: Vec::new(),
            board_selection: Vec::new(),
            board_tool: BoardTool::Select,
            ink_settings: InkSettings::default(),
            board_labels: BoardLabels::default(),
            selected_object: None,
            context_marker: None,
            context_point: None,
            relationship_from: None,
            relationship_handle_labels: [
                "Relationship start",
                "First curve control",
                "Second curve control",
                "Relationship end",
            ]
            .map(str::to_string),
            relationship_drag: None,
            titles: NewTopicTitles::default(),
            add_control_labels: AddControlLabels::default(),
            image_resize_label: "Resize image".into(),
            show_add_controls: true,
            show_alignment_guides: false,
            show_minimap: false,
            minimap_labels: MinimapLabels::default(),
            clipboard: None,
            board_clipboard: None,
            clipboard_out: None,
            host_handles_clipboard: false,
            filter: None,
            dark: false,
            accent: Color32::from_rgb(0x2E, 0x7C, 0xF6),
            editing: None,
            title_edit_serial: 0,
            title_companion_frame: None,
            title_error: None,
            drag: Drag::None,
            image_resize: None,
            board_input: board_input::State::default(),
            ink_controller: InkController::default(),
            ink_events_frame: None,
            touch_navigation: None,
            cache: None,
            minimap_cache: None,
            minimap_state: MinimapState::default(),
            pending_fit: false,
            pending_reveal: None,
            pending_board_reveal: None,
            initialized: false,
            explicit_initial_zoom: false,
            canvas_rect: Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0)),
        }
    }
}

impl MapView {
    fn minimap_rect(&self, canvas_rect: Rect, clip_rect: Rect) -> Option<Rect> {
        self.show_minimap
            .then(|| {
                bottom_right_rect(canvas_rect.intersect(clip_rect), DEFAULT_MINIMAP_SIZE, 12.0)
            })
            .flatten()
    }

    /// Changes minimap visibility without allowing an in-progress gesture to leak to the canvas.
    pub fn set_minimap_visible(&mut self, visible: bool) {
        self.show_minimap = visible;
        if !visible {
            self.minimap_state.cancel();
        }
    }

    /// Preflight for tools that process pointer, touch, scroll, or zoom input before
    /// [`MapCanvas::show`]. Call with the same `Ui` immediately before showing the canvas.
    pub fn minimap_blocks_pointer(&self, ui: &Ui) -> bool {
        self.minimap_rect(ui.available_rect_before_wrap(), ui.clip_rect())
            .map_or_else(
                || self.minimap_state.has_pointer_capture(),
                |rect| self.minimap_state.blocks_canvas_pointer(ui.ctx(), rect),
            )
    }

    /// Preflight for tools that process keyboard input before [`MapCanvas::show`].
    pub fn minimap_blocks_keyboard(&self, ui: &Ui) -> bool {
        self.minimap_rect(ui.available_rect_before_wrap(), ui.clip_rect())
            .is_some()
            && ui.ctx().memory(|memory| {
                memory.has_focus(MapCanvas::id().with("minimap"))
                    || memory.has_focus(MapCanvas::id().with("minimap-tab-proxy"))
            })
    }

    /// Selects a board tool and cancels any in-progress board gesture.
    pub fn set_board_tool(&mut self, tool: BoardTool) {
        if self.board_tool != tool {
            self.board_input.cancel_gesture();
            self.board_tool = tool;
        }
    }

    /// Whether a map-topic drag is currently in progress.
    pub fn has_active_topic_drag(&self) -> bool {
        matches!(self.drag, Drag::Topics { .. })
    }

    /// Whether a canvas interaction is still owned by an unfinished gesture.
    pub fn has_active_canvas_gesture(&self) -> bool {
        !matches!(self.drag, Drag::None)
            || self.image_resize.is_some()
            || self.relationship_drag.is_some()
            || self.board_input.has_pending_gesture()
            || self.ink_controller.has_pending_gesture()
    }

    /// Whether a held or failed ink gesture is retained outside the document.
    pub fn has_pending_ink_gesture(&self) -> bool {
        self.ink_controller.has_pending_gesture()
    }

    /// Whether a pending gesture has been released and can be retried or canceled.
    pub fn has_released_ink_gesture(&self) -> bool {
        self.ink_controller.has_released_gesture()
    }

    /// The tool that started the retained gesture, even if the active tool changed.
    pub fn pending_ink_mode(&self) -> Option<InkMode> {
        self.ink_controller.pending_mode()
    }

    /// Pointer identity recognized for the retained gesture, not the selected tool.
    pub fn pending_ink_pointer(&self) -> Option<RecoveredPointer> {
        self.ink_controller.pending_pointer()
    }

    /// Settings captured for the retained gesture.
    pub fn pending_ink_settings(&self) -> Option<InkSettings> {
        self.ink_controller.pending_settings()
    }

    /// Original accepted samples, available until a successful commit or cancellation.
    pub fn pending_ink_samples(&self) -> &[InkSample] {
        self.ink_controller.pending_samples()
    }

    /// Whether a rejected sample prevents retrying the retained gesture.
    pub fn pending_ink_has_rejected_input(&self) -> bool {
        self.ink_controller.pending_has_rejected_input()
    }

    pub fn pending_ink_recovery_revision(&self) -> u64 {
        self.ink_controller.recovery_revision()
    }

    pub fn pending_ink_recovery_block(&self) -> Option<InkRecoveryBlock> {
        self.ink_controller.pending_recovery_block()
    }

    pub fn pending_ink_was_recovered(&self) -> bool {
        self.ink_controller.pending_was_recovered()
    }

    pub fn pending_ink_source_is_current(&self, doc: &Document) -> bool {
        self.ink_controller.pending_source_is_current(doc)
    }

    pub fn pending_ink_for_recovery(
        &self,
        doc: &Document,
        has_other_drafts: bool,
    ) -> Option<RecoveredInkGesture> {
        self.ink_controller
            .pending_for_recovery(doc, has_other_drafts)
    }

    pub fn restore_recovered_ink(
        &mut self,
        doc: &Document,
        recovered: &RecoveredInkGesture,
    ) -> Result<(), String> {
        self.ink_controller.restore_recovered_ink(doc, recovered)
    }

    /// Release pointer ownership without discarding the buffered gesture.
    /// Prefer [`Self::suspend_ink`] when viewport events are available.
    pub fn interrupt_ink(&mut self) -> bool {
        self.ink_controller.interrupt()
    }

    /// Whether a hidden canvas still needs to observe pointer releases.
    pub fn needs_ink_input_tracking(&self) -> bool {
        self.ink_controller.needs_input_tracking()
    }

    /// Suspend input when the host hides this canvas, preserving owned events
    /// not yet routed to it and observing releases while it stays hidden.
    pub fn suspend_ink(
        &mut self,
        doc: &Document,
        ctx: &egui::Context,
    ) -> crate::ink_controller::InkFrameOutput {
        let frame = (ctx.viewport_id(), ctx.cumulative_frame_nr());
        let (events, pen) = if self.ink_events_frame == Some(frame) {
            (Vec::new(), crate::pen_input::PenFrameData::default())
        } else {
            let events = ctx.input(|input| input.events.clone());
            // Hidden tabs observe releases without consuming the active canvas's metadata.
            let mut pen = crate::pen_input::peek(ctx, frame.0, &events);
            let touch = crate::touch_input::peek_pointer_frame(ctx, frame.0, &events);
            pen.touch_pointers = touch.pointers;
            pen.touch_issue = touch.issue;
            (events, pen)
        };
        self.ink_events_frame = Some(frame);
        let transform = self.xf();
        self.ink_controller
            .suspend_with_pen(doc, transform, &events, &pen)
    }

    /// Explicitly discard the pending gesture and suppress its held pointer until release.
    pub fn cancel_ink(&mut self) -> bool {
        self.ink_controller.cancel()
    }

    /// Retry a released ink, eraser, or lasso gesture against the current scene.
    pub fn retry_ink(&mut self, doc: &mut Document) -> Result<InkOutcome, InkControllerError> {
        let layout = self.layout_for(doc);
        let scene = build_scene(doc.sheet(), &layout);
        let outcome = self.ink_controller.retry(doc, &scene)?;
        ink_view::apply_outcome(self, doc.sheet(), &outcome);
        Ok(outcome)
    }

    /// Retry a released pen/highlighter gesture with different finish options.
    pub fn retry_ink_with_finish_options(
        &mut self,
        doc: &mut Document,
        options: StrokeFinishOptions,
    ) -> Result<InkOutcome, InkControllerError> {
        let layout = self.layout_for(doc);
        let scene = build_scene(doc.sheet(), &layout);
        let outcome = self
            .ink_controller
            .retry_with_finish_options(doc, &scene, options)?;
        ink_view::apply_outcome(self, doc.sheet(), &outcome);
        Ok(outcome)
    }

    pub fn select_only(&mut self, id: TopicId) {
        self.pending_board_reveal = None;
        if self.selection != [id] {
            self.pending_reveal = Some(id);
        }
        self.selection = vec![id];
        self.board_selection.clear();
        self.selected_object = None;
    }

    /// Selects one canvas object and schedules its visible geometry for reveal.
    ///
    /// Call after switching sheets (and resetting the view). The reveal is
    /// resolved against the new sheet's scene on a subsequent canvas frame.
    pub fn select_board_object(&mut self, id: CanvasObjectId) {
        self.selection.clear();
        self.board_selection = vec![id];
        self.selected_object = None;
        self.relationship_from = None;
        self.reveal_board_object(id);
    }

    pub fn select_object(&mut self, object: ObjectSelection) {
        self.selection.clear();
        self.board_selection.clear();
        self.selected_object = Some(object);
        self.relationship_from = None;
        self.pending_reveal = None;
        self.pending_board_reveal = None;
    }

    pub fn toggle_selected(&mut self, id: TopicId) {
        self.selected_object = None;
        if let Some(i) = self.selection.iter().position(|x| *x == id) {
            self.selection.remove(i);
        } else {
            self.selection.push(id);
        }
    }

    pub fn primary(&self) -> Option<TopicId> {
        self.selection.last().copied()
    }

    /// Screen-space symbol bounds in the current rendered document, for instance affordances.
    pub fn symbol_rect(&self, doc: &Document, id: TopicId, index: usize) -> Option<Rect> {
        let cache = self.cache.as_ref()?;
        if cache.key.0 != doc.state_id()
            || cache.key.1 != doc.active_index()
            || cache.key.2 != self.dark
        {
            return None;
        }
        cache
            .layout
            .topics
            .get(&id)?
            .markers
            .get(index)
            .map(|m| self.xf().rect(m.1))
    }

    /// Document paint layers at the marker center (selection outlines do not fill topics).
    pub fn symbol_background(
        &self,
        doc: &Document,
        id: TopicId,
        index: Option<usize>,
    ) -> thinoda_core::Color {
        use thinoda_core::{
            Color,
            theme::{Theme, resolve_style},
        };
        let sheet = doc.sheet();
        let mut bg = Theme::for_settings(&sheet.theme).background(&sheet.theme, self.dark);
        if let Some(cache) = &self.cache
            && cache.key.0 == doc.state_id()
            && cache.key.1 == doc.active_index()
            && cache.key.2 == self.dark
            && let Some(topic) = cache.layout.topics.get(&id)
        {
            let point = index
                .and_then(|i| topic.markers.get(i).map(|m| m.1.center()))
                .unwrap_or(topic.rect.center());
            for boundary in &cache.layout.boundaries {
                if boundary.rect.contains(point) {
                    let selected =
                        self.selected_object == Some(ObjectSelection::Boundary(boundary.id));
                    let c = if selected {
                        self.accent
                    } else {
                        sheet
                            .boundaries
                            .iter()
                            .find(|b| b.id == boundary.id)
                            .and_then(|b| b.color)
                            .map(color32)
                            .unwrap_or(Color32::from_rgb(0x8A, 0x9B, 0xB0))
                    }
                    .gamma_multiply(0.12)
                    .to_srgba_unmultiplied();
                    bg = Color::rgba(c[0], c[1], c[2], c[3]).over(bg);
                }
            }
        }
        resolve_style(sheet, id, self.dark)
            .fill
            .map(|fill| fill.over(bg))
            .unwrap_or(bg)
    }

    /// Screen-space bounds of the primary selected topic in the last rendered layout.
    /// Returns `None` before layout, after document changes, during dragging, for
    /// multi-selection, or for hidden/deleted topics.
    /// The current camera transform is applied, so callers can anchor overlay controls.
    pub fn selected_topic_rect(&self, doc: &Document) -> Option<Rect> {
        if self.selection.len() != 1 || !matches!(self.drag, Drag::None) {
            return None;
        }
        let cache = self.cache.as_ref()?;
        if cache.key.0 != doc.state_id()
            || cache.key.1 != doc.active_index()
            || cache.key.2 != self.dark
        {
            return None;
        }
        let topic = self.primary()?;
        if doc.sheet().topic(topic).is_none() || !doc.sheet().is_visible(topic) {
            return None;
        }
        cache.layout.rect(topic).map(|r| self.xf().rect(r))
    }

    /// Laid-out bounds of the supplied topics and their visible descendants.
    pub fn topic_group_bounds(
        &mut self,
        doc: &Document,
        roots: &[TopicId],
    ) -> Option<thinoda_core::Rect> {
        let layout = self.layout_for(doc);
        let mut pending = roots.to_vec();
        let mut visited = std::collections::BTreeSet::new();
        let mut bounds: Option<thinoda_core::Rect> = None;
        while let Some(id) = pending.pop() {
            if !visited.insert(id) {
                continue;
            }
            if let Some(rect) = layout.rect(id) {
                bounds = Some(bounds.map_or(rect, |bounds| bounds.union(&rect)));
                if let Some(topic) = doc.sheet().topic(id)
                    && !topic.collapsed
                {
                    pending.extend_from_slice(&topic.children);
                }
            }
        }
        bounds
    }

    /// Screen-space canvas bounds from the latest frame.
    pub fn canvas_rect(&self) -> Rect {
        self.canvas_rect
    }

    /// Removes deleted/hidden topics from the selection.
    pub fn prune_selection(&mut self, sheet: &Sheet) {
        self.selection.retain(|id| {
            !sheet.is_anchor(*id) && sheet.topic(*id).is_some() && sheet.is_visible(*id)
        });
        self.board_selection
            .retain(|id| sheet.canvas.object(*id).is_some());
        if self
            .selected_object
            .is_some_and(|object| !object.exists(sheet))
        {
            self.selected_object = None;
        }
        if self.pending_reveal.is_some_and(|id| sheet.is_anchor(id)) {
            self.pending_reveal = None;
        }
        if self
            .pending_board_reveal
            .is_some_and(|id| sheet.canvas.object(id).is_none())
        {
            self.pending_board_reveal = None;
        }
        if self.relationship_from.is_some_and(|id| sheet.is_anchor(id)) {
            self.relationship_from = None;
        }
        if let Some(e) = &self.editing
            && (sheet.is_anchor(e.topic) || sheet.topic(e.topic).is_none())
        {
            self.editing = None;
        }
    }

    /// Resets per-sheet state (call when switching sheets or documents).
    pub fn reset(&mut self) {
        self.selection.clear();
        self.board_selection.clear();
        self.selected_object = None;
        self.relationship_from = None;
        self.relationship_drag = None;
        self.image_resize = None;
        self.board_input = board_input::State::default();
        self.board_tool = BoardTool::Select;
        self.editing = None;
        self.drag = Drag::None;
        self.cache = None;
        self.minimap_cache = None;
        self.minimap_state.cancel();
        self.pending_reveal = None;
        self.pending_board_reveal = None;
        self.initialized = false;
        self.explicit_initial_zoom = false;
        self.zoom = 1.0;
        self.offset = EVec2::ZERO;
    }

    // ----- editing -------------------------------------------------------------------------

    pub fn begin_edit(&mut self, sheet: &Sheet, id: TopicId, select_all: bool) {
        if sheet.is_anchor(id) || !board_input::topic_is_unlocked(sheet, id) {
            return;
        }
        if self.title_error.is_some() && self.has_title_changes() {
            self.title_error =
                Some("Finish or cancel the current title edit before starting another".into());
            return;
        }
        let Some(t) = sheet.topic(id) else { return };
        self.title_edit_serial = self.title_edit_serial.wrapping_add(1);
        self.editing = Some(EditState {
            topic: id,
            text: t.title.clone(),
            original: t.title.clone(),
            select_all,
            focus_requested: false,
            focus_retry_after: None,
            ime_composing: false,
            companion_cursor: None,
        });
        self.pending_reveal = Some(id);
    }

    /// Starts editing with `text` replacing the title (type-to-edit).
    pub fn begin_edit_with(&mut self, sheet: &Sheet, id: TopicId, text: &str) {
        if (self.title_error.is_some() && self.has_title_changes())
            || sheet.is_anchor(id)
            || sheet.topic(id).is_none()
            || !board_input::topic_is_unlocked(sheet, id)
        {
            return;
        }
        self.begin_edit(sheet, id, false);
        if let Some(e) = &mut self.editing
            && e.topic == id
        {
            e.text = text.to_string();
        }
    }

    pub fn editing_topic(&self) -> Option<TopicId> {
        self.editing.as_ref().map(|e| e.topic)
    }

    /// Identifies this view's current title-edit session.
    pub fn title_edit_serial(&self) -> Option<u64> {
        self.editing.as_ref().map(|_| self.title_edit_serial)
    }

    /// Captures the current title-edit session without committing or replacing it.
    pub fn capture_title_edit_continuation(&self) -> Option<TitleEditContinuation> {
        self.editing.clone().map(|edit| TitleEditContinuation {
            edit,
            serial: self.title_edit_serial,
        })
    }

    /// Resumes the same edit only if its text was committed unchanged.
    pub fn resume_title_edit_after_commit(
        &mut self,
        doc: &Document,
        mut continuation: TitleEditContinuation,
    ) -> bool {
        if self.editing.is_some()
            || self.title_edit_serial != continuation.serial
            || !board_input::topic_is_unlocked(doc.sheet(), continuation.edit.topic)
            || !doc
                .sheet()
                .topic(continuation.edit.topic)
                .is_some_and(|topic| topic.title == continuation.edit.text)
        {
            return false;
        }
        continuation.edit.original = continuation.edit.text.clone();
        self.editing = Some(continuation.edit);
        self.title_error = None;
        true
    }

    /// Retain the title draft while a host-owned companion control has focus.
    pub fn retain_title_draft_this_frame(&mut self, ctx: &egui::Context, refocus: bool) {
        if self.editing.is_some() {
            let frame = (ctx.viewport_id(), ctx.cumulative_frame_nr());
            let already_refocusing = self
                .title_companion_frame
                .is_some_and(|(previous, requested)| previous == frame && requested);
            self.title_companion_frame = Some((frame, refocus || already_refocusing));
        }
    }

    /// Include current-frame IME events, which may precede the editor's update.
    pub fn title_ime_active(&self, ctx: &egui::Context) -> bool {
        self.editing.as_ref().is_some_and(|edit| {
            edit.ime_composing
                || ctx.input(|input| {
                    input
                        .events
                        .iter()
                        .any(|event| matches!(event, egui::Event::Ime(_)))
                })
        })
    }

    /// Begin editing the text label of an unlocked shape or connector.
    pub fn begin_board_label(&mut self, doc: &Document, id: CanvasObjectId) -> bool {
        board_input::begin_label(self, doc, id)
    }

    /// Object whose label is currently being edited, if any.
    pub fn editing_board_object(&self) -> Option<CanvasObjectId> {
        self.board_input.label.as_ref().map(|edit| edit.id)
    }

    /// Commit the object-label draft; validation failures keep the draft intact.
    pub fn try_commit_board_label(&mut self, doc: &mut Document) -> thinoda_core::EditResult<bool> {
        board_input::commit_label(self, doc)
    }

    pub fn editing_text(&self) -> Option<&str> {
        self.editing.as_ref().map(|e| e.text.as_str())
    }

    /// Call before drawing a title editor so IME candidate keys cannot finish the edit.
    pub fn protect_title_ime(&mut self, ctx: &egui::Context, editor_id: egui::Id) {
        let Some(edit) = self.editing.as_mut() else {
            return;
        };
        protect_ime_candidate_keys(ctx, editor_id, &mut edit.ime_composing);
    }

    pub fn set_editing_text(&mut self, text: &str) {
        if let Some(e) = &mut self.editing {
            e.text = text.to_string();
        }
    }

    pub fn has_title_changes(&self) -> bool {
        self.editing.as_ref().is_some_and(|e| e.text != e.original)
    }

    pub fn title_draft(&self) -> Option<(TopicId, &str, &str)> {
        self.editing
            .as_ref()
            .map(|edit| (edit.topic, edit.original.as_str(), edit.text.as_str()))
    }

    pub fn take_title_error(&mut self) -> Option<String> {
        self.title_error.take()
    }

    /// Writes the edited title into the document. Returns whether a change was made.
    pub fn commit_edit(&mut self, doc: &mut Document) -> bool {
        match self.try_commit_edit(doc) {
            Ok(changed) => changed,
            Err(error) => {
                self.title_error = Some(error.to_string());
                false
            }
        }
    }

    /// Rejected edits keep their text and ownership, so validation never discards a draft.
    pub fn try_commit_edit(&mut self, doc: &mut Document) -> thinoda_core::EditResult<bool> {
        let Some(e) = self.editing.as_ref() else {
            return Ok(false);
        };
        self.pending_reveal = Some(e.topic);
        if !board_input::topic_is_unlocked(doc.sheet(), e.topic) {
            return Err(thinoda_core::EditError::Invalid(
                "Card or group is locked; title draft retained".into(),
            ));
        }
        if e.text == e.original {
            self.editing = None;
            return Ok(false);
        }
        let current = doc.sheet().topic(e.topic).ok_or_else(|| {
            thinoda_core::EditError::Invalid("Title source no longer exists; draft retained".into())
        })?;
        if current.title != e.original {
            return Err(thinoda_core::EditError::Invalid(
                "Title source changed while editing; draft retained".into(),
            ));
        }
        doc.edit_sheet("Edit Title", |s| s.set_title(e.topic, &e.text))?;
        self.editing = None;
        self.title_error = None;
        Ok(true)
    }

    pub fn cancel_edit(&mut self) {
        self.editing = None;
        self.board_input.label = None;
        self.title_error = None;
    }

    // ----- camera --------------------------------------------------------------------------

    pub fn set_zoom(&mut self, zoom: f32) {
        let c = self.canvas_rect.center();
        let factor = zoom.clamp(MIN_ZOOM, MAX_ZOOM) / self.zoom;
        self.zoom_by(factor, Some(c));
    }

    /// Sets the zoom preference before the first frame of a new/reset view.
    /// A non-default value keeps initial centering but opts out of automatic fit.
    /// `1.0` preserves the default fit-on-overflow behavior; explicit Fit still works.
    pub fn set_initial_zoom(&mut self, zoom: f32) {
        let zoom = if zoom.is_finite() { zoom } else { 1.0 };
        self.set_zoom(zoom);
        self.explicit_initial_zoom = self.zoom != 1.0;
    }

    /// Zooms by `factor` keeping `anchor` (screen position) fixed. `None` uses the center.
    pub fn zoom_by(&mut self, factor: f32, anchor: Option<Pos2>) {
        let anchor = anchor.unwrap_or(self.canvas_rect.center());
        let new_zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let f = new_zoom / self.zoom;
        let c = self.canvas_rect.center();
        // screen = c + offset + p*zoom  => keep p under anchor fixed.
        let rel = anchor - c - self.offset;
        self.offset = anchor - c - rel * f;
        self.zoom = new_zoom;
    }

    pub fn request_fit(&mut self) {
        self.pending_fit = true;
    }

    /// Scrolls so that `id` is visible on the next frame.
    pub fn reveal(&mut self, id: TopicId) {
        self.pending_reveal = Some(id);
        self.pending_board_reveal = None;
    }

    /// Scrolls to a canvas object's visible scene bounds on a subsequent frame.
    ///
    /// Connector bounds use resolved endpoints rather than the stored object
    /// rectangle, and frame clipping is respected. Missing or wholly clipped
    /// objects do not move the camera. Selection is unchanged.
    pub fn reveal_board_object(&mut self, id: CanvasObjectId) {
        self.pending_reveal = None;
        self.pending_board_reveal = Some(id);
    }

    pub fn zoom_percent(&self) -> u32 {
        (self.zoom * 100.0).round() as u32
    }

    pub(crate) fn xf(&self) -> Xf {
        Xf {
            origin: self.canvas_rect.center() + self.offset,
            zoom: self.zoom,
        }
    }

    /// Layout of the active sheet (cached; uses approximate metrics before the first frame).
    pub fn layout_for(&mut self, doc: &Document) -> Arc<Layout> {
        let key = (doc.state_id(), doc.active_index(), self.dark, 0);
        if let Some(c) = &self.cache
            && c.key.0 == key.0
            && c.key.1 == key.1
            && c.key.2 == key.2
        {
            return c.layout.clone();
        }
        let layout = Arc::new(layout_sheet(doc.sheet(), &ApproxMeasure, self.dark));
        self.cache = Some(LayoutCache {
            key,
            font_key: 0,
            layout: layout.clone(),
        });
        layout
    }

    fn layout_with(&mut self, ctx: &egui::Context, doc: &Document) -> Arc<Layout> {
        let font_key = ctx.fonts(|fonts| {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            let defs = fonts.definitions();
            defs.families.hash(&mut hasher);
            for (name, data) in &defs.font_data {
                name.hash(&mut hasher);
                Arc::as_ptr(data).hash(&mut hasher);
            }
            hasher.finish()
        });
        let edit_hash = self.editing.as_ref().map_or(0, |e| {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            e.topic.hash(&mut h);
            e.text.hash(&mut h);
            h.finish() | 1
        });
        let key = (doc.state_id(), doc.active_index(), self.dark, edit_hash);
        if let Some(c) = &self.cache
            && c.key == key
            && c.font_key == font_key
        {
            return c.layout.clone();
        }
        let measure = EguiMeasure::new(ctx);
        let layout = match &self.editing {
            Some(e) => {
                let mut sheet = doc.sheet().clone();
                if let Some(t) = sheet.topic_mut(e.topic) {
                    t.title = if e.text.is_empty() {
                        " ".into()
                    } else {
                        e.text.clone()
                    };
                }
                layout_sheet(&sheet, &measure, self.dark)
            }
            None => layout_sheet(doc.sheet(), &measure, self.dark),
        };
        let layout = Arc::new(layout);
        self.cache = Some(LayoutCache {
            key,
            font_key,
            layout: layout.clone(),
        });
        layout
    }

    /// Camera viewport after reserving the default compact board toolbox.
    /// Plain mind maps retain their original full-width viewport.
    fn fit_viewport(&self, sheet: &Sheet) -> Rect {
        let mut viewport = self.canvas_rect;
        if sheet.uses_board_semantics() {
            // Leave a non-negative width even in very narrow windows.
            viewport.min.x += BOARD_TOOLBOX_SAFE_INSET.min(viewport.width().max(0.0));
        }
        viewport
    }

    fn fit(&mut self, sheet: &Sheet, layout: &Layout) {
        let b = fit_bounds(sheet, layout);
        if b.size.x <= 0.0 || b.size.y <= 0.0 {
            return;
        }
        let viewport = self.fit_viewport(sheet);
        let avail = viewport.size() - vec2(80.0, 80.0);
        let z = (avail.x / b.size.x)
            .min(avail.y / b.size.y)
            .clamp(MIN_ZOOM, 1.0);
        self.zoom = z;
        let c = b.center();
        self.offset = vec2(-c.x * z, -c.y * z);
        if viewport != self.canvas_rect {
            self.offset += viewport.center() - self.canvas_rect.center();
        }
    }

    fn do_reveal(&mut self, sheet: &Sheet, layout: &Layout, id: TopicId) {
        let Some(topic) = layout.topics.get(&id) else {
            return;
        };
        let rotation = topic_rotation(sheet, topic);
        let bounds = if rotation == 0.0 {
            topic.rect
        } else {
            rotated_bounds(topic.rect, rotation)
        };
        let mut r = self.xf().rect(bounds);
        if self.editing_topic() == Some(id) {
            r = r.union(self.xf().rect(topic_editor_rect(sheet, topic)).expand(2.0));
        }
        // Reserve the actual screen-space controls, including while editing hides
        // them. Never clamp a plus onto the topic's text at a viewport edge.
        for handle in topic_add_handles(self, sheet, layout, id) {
            r = r.union(handle.rect().expand(2.0));
        }
        let area = self.canvas_rect.shrink(40.0);
        let mut d = EVec2::ZERO;
        if r.width() < area.width() {
            if r.left() < area.left() {
                d.x = area.left() - r.left();
            } else if r.right() > area.right() {
                d.x = area.right() - r.right();
            }
        }
        if r.height() < area.height() {
            if r.top() < area.top() {
                d.y = area.top() - r.top();
            } else if r.bottom() > area.bottom() {
                d.y = area.bottom() - r.bottom();
            }
        }
        self.offset += d;
    }

    fn do_reveal_board_object(&mut self, scene: &Scene, id: CanvasObjectId) {
        let Some(bounds) = scene
            .item(SceneId::Object(id))
            .and_then(|item| item.visible_bounds())
        else {
            return;
        };
        let r = self.xf().rect(bounds);
        let area = self.canvas_rect.shrink(40.0);
        if !area.is_positive() {
            return;
        }
        let reveal_axis = |min: f32, max: f32, area_min: f32, area_max: f32| {
            if max - min <= area_max - area_min {
                if min < area_min {
                    area_min - min
                } else if max > area_max {
                    area_max - max
                } else {
                    0.0
                }
            } else if min >= area_max {
                area_min - min
            } else if max <= area_min {
                area_max - max
            } else {
                0.0
            }
        };
        self.offset += vec2(
            reveal_axis(r.left(), r.right(), area.left(), area.right()),
            reveal_axis(r.top(), r.bottom(), area.top(), area.bottom()),
        );
    }
}

/// Keep layout-only extents (relationships, boundaries, etc.) as well as rotated
/// and free-object scene geometry.
fn fit_bounds(sheet: &Sheet, layout: &Layout) -> thinoda_core::Rect {
    build_scene(sheet, layout)
        .bounds()
        .map_or(layout.bounds, |bounds| bounds.union(&layout.bounds))
}

/// Result of showing the canvas for one frame.
pub struct CanvasOutput {
    pub response: Response,
    context_menu_allowed: bool,
    /// Do not forward pointer, touch, scroll, or zoom input to canvas tools this frame.
    pub minimap_blocks_pointer: bool,
    /// Do not forward keyboard input to canvas tools while the minimap has focus.
    pub minimap_blocks_keyboard: bool,
    /// Hyperlink the user clicked (host decides how to open it).
    pub open_link: Option<String>,
    /// Topic the context menu refers to (if the user right-clicked a topic).
    pub context_topic: Option<TopicId>,
    pub selection_changed: bool,
    /// Last edit error (e.g. trying to delete the central topic).
    pub error: Option<thinoda_core::EditError>,
    /// Structured ink errors, including any rejected input samples.
    pub ink_errors: Vec<InkControllerError>,
    /// Completed or explicitly canceled ink gestures in this frame.
    pub ink_outcomes: Vec<InkOutcome>,
    /// Copy, cut and paste requests for hosts that enabled
    /// [`MapView::host_handles_clipboard`], in input order.
    pub clipboard_requests: Vec<Action>,
    /// FIM edits that need the host's schema-aware authoring flow.
    pub authoring_action: Option<Action>,
    pub authoring_text: Option<String>,
}

impl CanvasOutput {
    /// Show context actions without interpreting canceled or multitouch holds as clicks.
    pub fn context_menu(&self, content: impl FnOnce(&mut Ui)) -> Option<egui::InnerResponse<()>> {
        let mut popup = egui::Popup::context_menu(&self.response);
        if !self.context_menu_allowed {
            popup = popup.open_memory(Some(egui::SetOpenCommand::Bool(false)));
        }
        popup.show(content)
    }
}

/// The canvas widget.
pub struct MapCanvas;

/// Fixed-size screen-space targets remain usable at any map zoom.
struct AddHandle {
    action: Action,
    anchor: Pos2,
    center: Pos2,
    label: String,
}

fn topic_rotation(sheet: &Sheet, topic: &TopicLayout) -> f32 {
    sheet
        .topic(topic.id)
        .and_then(|topic| topic.card)
        .map_or(0.0, |card| card.rotation)
}

fn topic_editor_rect(sheet: &Sheet, topic: &TopicLayout) -> thinoda_core::Rect {
    let rotation = topic_rotation(sheet, topic);
    if rotation == 0.0 {
        return topic.title_rect;
    }
    thinoda_core::Rect::from_center_size(
        rotate_around(topic.title_rect.center(), topic.rect.center(), rotation),
        topic.title_rect.size,
    )
}

fn hit_badge(sheet: &Sheet, layout: &Layout, point: Vec2) -> Option<TopicId> {
    layout.order.iter().rev().copied().find(|id| {
        let topic = &layout.topics[id];
        topic.hidden_children > 0
            && topic_clip_rect(sheet, *id).is_none_or(|clip| clip.contains(point))
            && (topic_badge_center(sheet, topic) - point).length() <= 10.0
    })
}

impl AddHandle {
    fn rect(&self) -> Rect {
        Rect::from_center_size(self.center, vec2(28.0, 28.0))
    }
}

fn add_handles(view: &MapView, sheet: &Sheet, layout: &Layout) -> Vec<AddHandle> {
    if !view.show_add_controls
        || view.selection.len() != 1
        || !view.board_selection.is_empty()
        || view.selected_object.is_some()
        || view.editing.is_some()
        || view.editing_board_object().is_some()
        || view.board_tool != BoardTool::Select
        || view.relationship_from.is_some()
        || view.image_resize.is_some()
        || !matches!(view.drag, Drag::None)
    {
        return Vec::new();
    }
    let Some(id) = view.primary() else {
        return Vec::new();
    };
    let mut handles = topic_add_handles(view, sheet, layout, id);
    // Never expose a target outside the canvas or partially clipped by a panel.
    handles.retain(|h| view.canvas_rect.contains_rect(h.rect()));
    handles
}

/// Unclipped geometry is shared by rendering, hit testing, and camera reveal.
fn topic_add_handles(
    view: &MapView,
    sheet: &Sheet,
    layout: &Layout,
    id: TopicId,
) -> Vec<AddHandle> {
    if !view.show_add_controls {
        return Vec::new();
    }
    let Some(topic) = layout.topics.get(&id) else {
        return Vec::new();
    };
    let r = view.xf().rect(topic.rect);
    let (anchor, outward) = match topic.grow {
        Dir::Right => (r.right_center(), vec2(1.0, 0.0)),
        Dir::Left => (r.left_center(), vec2(-1.0, 0.0)),
        Dir::Down => (r.center_bottom(), vec2(0.0, 1.0)),
        Dir::Up => (r.center_top(), vec2(0.0, -1.0)),
    };
    let rotation = topic_rotation(sheet, topic);
    let (anchor, outward) = if rotation == 0.0 {
        (anchor, outward)
    } else {
        let rotation = egui::emath::Rot2::from_angle(rotation.to_radians());
        (
            r.center() + rotation * (anchor - r.center()),
            rotation * outward,
        )
    };
    // Leave room for the existing collapsed-descendant badge, even when zoomed in.
    let mut handles = vec![AddHandle {
        action: Action::InsertChild,
        anchor,
        center: anchor + outward * (20.0 * view.zoom + 18.0),
        label: view.add_control_labels.child.clone(),
    }];
    if sheet.parent(topic.id).is_some() {
        let (anchor, direction) = if topic.grow.is_horizontal() {
            (r.center_bottom(), vec2(0.0, 1.0))
        } else {
            (r.right_center(), vec2(1.0, 0.0))
        };
        handles.push(AddHandle {
            action: Action::InsertSibling,
            anchor,
            center: anchor + direction * 28.0,
            label: view.add_control_labels.sibling.clone(),
        });
    }
    handles
}

impl MapCanvas {
    pub fn id() -> Id {
        Id::unique("thinoda_map_canvas")
    }

    /// Focusable title editor, present only while a topic is being edited.
    pub fn title_editor_id() -> Id {
        Self::id().with("edit")
    }

    /// Focusable shape or connector label editor.
    pub fn board_label_editor_id() -> Id {
        Self::id().with("board-label-edit")
    }

    /// Paints the current camera without interaction, selection, search dimming or editing chrome.
    pub fn show_snapshot(ui: &mut Ui, doc: &Document, view: &mut MapView) -> Rect {
        view.drag = Drag::None;
        view.relationship_drag = None;
        view.image_resize = None;
        view.minimap_state.cancel();
        view.minimap_state.finish_frame(ui.ctx());
        ui.memory_mut(|memory| {
            memory.surrender_focus(Self::id().with("minimap"));
            memory.ignore_focus(Self::id().with("minimap-tab-proxy"));
        });
        let rect = ui.available_rect_before_wrap();
        ui.advance_cursor_after_rect(rect);
        view.canvas_rect = rect;
        let layout = view.layout_with(ui.ctx(), doc);
        let painter = ui.painter().with_clip_rect(rect);
        painter.rect_filled(rect, 0, sheet_background(doc.sheet(), view.dark));
        paint_map(
            &painter,
            doc.sheet(),
            &layout,
            view.xf(),
            &PaintOptions {
                dark: view.dark,
                accent: view.accent,
                show_grid: true,
                ..Default::default()
            },
        );
        rect
    }

    pub fn show(ui: &mut Ui, doc: &mut Document, view: &mut MapView) -> CanvasOutput {
        Self::show_with_shortcuts(ui, doc, view, &shortcut_bindings())
    }

    /// Finish an outside label edit before the host handles a pointer press.
    ///
    /// Hosts should call this for the visible canvas before processing their own
    /// controls, so an outside click cannot commit a stale label draft.
    pub fn finish_board_label_before_pointer_press(
        ui: &mut Ui,
        doc: &mut Document,
        view: &mut MapView,
    ) -> Option<thinoda_core::EditError> {
        if view.board_input.label.is_none()
            || !ui.input(|input| input.focused && input.pointer.any_pressed())
        {
            return None;
        }
        let layout = view.layout_with(ui.ctx(), doc);
        let scene = build_scene(doc.sheet(), &layout);
        let overlay_rect = if ui.is_enabled() && !egui::Popup::is_any_open(ui.ctx()) {
            view.minimap_rect(view.canvas_rect(), ui.clip_rect())
        } else {
            None
        };
        let mut error = None;
        board_input::finish_label_before_pointer_press(
            ui,
            doc,
            view,
            &scene,
            &mut error,
            overlay_rect,
        );
        error
    }

    /// The host may supply an explicitly selected keyboard profile.
    pub fn show_with_shortcuts(
        ui: &mut Ui,
        doc: &mut Document,
        view: &mut MapView,
        shortcuts: &[(egui::KeyboardShortcut, Action)],
    ) -> CanvasOutput {
        let id = Self::id();
        let rect = ui.available_rect_before_wrap();
        let ctx = ui.ctx().clone();
        board_input::begin_focus_exit(&ctx, view);
        let response = ui.interact(rect, id, Sense::click_and_drag() | Sense::FOCUSABLE);
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Canvas, true, "Mind map canvas"));
        ui.advance_cursor_after_rect(rect);
        view.canvas_rect = rect;
        view.prune_selection(doc.sheet());
        let before_sel = view.selection.clone();
        let before_board_selection = view.board_selection.clone();
        let before_object = view.selected_object;
        let mut out = CanvasOutput {
            response: response.clone(),
            context_menu_allowed: true,
            minimap_blocks_pointer: false,
            minimap_blocks_keyboard: false,
            open_link: None,
            context_topic: None,
            selection_changed: false,
            error: None,
            ink_errors: Vec::new(),
            ink_outcomes: Vec::new(),
            clipboard_requests: Vec::new(),
            authoring_action: None,
            authoring_text: None,
        };

        let mut layout = view.layout_with(&ctx, doc);
        if !view.initialized {
            view.initialized = true;
            let b = fit_bounds(doc.sheet(), &layout);
            let viewport = view.fit_viewport(doc.sheet());
            if !view.explicit_initial_zoom
                && (b.size.x * view.zoom > viewport.width()
                    || b.size.y * view.zoom > viewport.height())
            {
                view.fit(doc.sheet(), &layout);
            } else {
                let c = b.center();
                view.offset = vec2(-c.x, -c.y) * view.zoom;
                if viewport != rect {
                    view.offset += viewport.center() - rect.center();
                }
            }
        }
        if std::mem::take(&mut view.pending_fit) {
            view.fit(doc.sheet(), &layout);
        }
        if let Some(r) = view.pending_reveal.take() {
            view.do_reveal(doc.sheet(), &layout, r);
        }
        let scene = build_scene(doc.sheet(), &layout);
        if matches!(view.drag, Drag::None)
            && !view.board_input.is_active()
            && let Some(id) = view.pending_board_reveal.take()
        {
            view.do_reveal_board_object(&scene, id);
        }

        let minimap_rect = view.minimap_rect(rect, ui.clip_rect());
        if ctx.input(|input| input.pointer.any_pressed()) {
            let overlay_rect = if ui.is_enabled() && !egui::Popup::is_any_open(&ctx) {
                minimap_rect
            } else {
                None
            };
            board_input::finish_label_before_pointer_press(
                ui,
                doc,
                view,
                &scene,
                &mut out.error,
                overlay_rect,
            );
        }
        let (minimap_blocks_pointer, mut minimap_blocks_keyboard) =
            if let Some(minimap_rect) = minimap_rect {
                if view
                    .minimap_cache
                    .as_ref()
                    .is_none_or(|cache| !Arc::ptr_eq(&cache.layout, &layout))
                {
                    view.minimap_cache = Some(MinimapCache {
                        layout: layout.clone(),
                        scene: MinimapScene::from_scene(&scene),
                    });
                }
                let camera = MinimapCamera {
                    canvas_rect: rect,
                    offset: view.offset,
                    zoom: view.zoom,
                };
                let enabled = ui.is_enabled() && !egui::Popup::is_any_open(&ctx);
                let output = egui::Area::new(id.with("minimap-area"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(minimap_rect.min)
                    .movable(false)
                    .show(&ctx, |overlay| {
                        overlay.set_min_size(minimap_rect.size());
                        overlay
                            .add_enabled_ui(enabled, |overlay| {
                                show_minimap(
                                    overlay,
                                    id.with("minimap"),
                                    minimap_rect,
                                    &view.minimap_cache.as_ref().unwrap().scene,
                                    camera,
                                    &mut view.minimap_state,
                                    &view.minimap_labels,
                                )
                            })
                            .inner
                    })
                    .inner;
                if let Some(offset) = output.offset {
                    view.offset = offset;
                }
                (output.blocks_canvas_pointer, output.blocks_canvas_keyboard)
            } else {
                view.minimap_state.cancel();
                let blocked = view.minimap_state.has_pointer_capture();
                view.minimap_state.finish_frame(&ctx);
                ctx.memory_mut(|memory| {
                    memory.surrender_focus(id.with("minimap"));
                    memory.ignore_focus(id.with("minimap-tab-proxy"));
                });
                (blocked, false)
            };
        out.minimap_blocks_pointer = minimap_blocks_pointer;
        out.minimap_blocks_keyboard = minimap_blocks_keyboard;

        if !minimap_blocks_pointer
            && (response.clicked() || response.drag_started() || response.secondary_clicked())
        {
            response.request_focus();
        }
        if !minimap_blocks_pointer
            && !minimap_blocks_keyboard
            && ctx.memory(|m| m.focused()).is_none()
            && view.editing.is_none()
            && view.editing_board_object().is_none()
            && !ctx.input(|input| {
                (input.pointer.any_pressed() || input.pointer.any_released())
                    && input
                        .pointer
                        .interact_pos()
                        .is_some_and(|position| !rect.contains(position))
            })
        {
            ctx.memory_mut(|m| m.request_focus(id));
        }
        if response.has_focus() {
            ctx.memory_mut(|m| {
                m.set_focus_lock_filter(
                    id,
                    EventFilter {
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: true,
                    },
                )
            });
        }

        // ---- pointer -------------------------------------------------------------------
        let pointer = (!minimap_blocks_pointer)
            .then(|| response.hover_pos().or_else(|| ctx.pointer_latest_pos()))
            .flatten();
        let hovered = pointer
            .filter(|p| rect.contains(*p))
            .and_then(|p| scene.hit_test(view.xf().sheet(p)))
            .and_then(|hit| match hit {
                SceneHit::Topic(id) => Some(id),
                SceneHit::Object(_) => None,
            });

        let previous_touch_navigation = view.touch_navigation.is_some();
        let space =
            ctx.input(|i| i.key_down(Key::Space)) && view.editing.is_none() && response.has_focus();
        let (multi_touch, any_touches, touch_canceled, restart_after_cancel) = ctx.input(|input| {
            let canceled = input.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::Touch {
                        phase: egui::TouchPhase::Cancel,
                        ..
                    }
                )
            });
            (
                input.multi_touch(),
                input.any_touches(),
                canceled,
                (canceled && input.focused)
                    .then(|| {
                        view.ink_controller
                            .restart_after_touch_cancel(&input.events)
                    })
                    .flatten(),
            )
        });
        let pen_blocks_touch_navigation = view.ink_controller.blocks_touch_navigation()
            || (!minimap_blocks_pointer
                && (multi_touch.is_some() || touch_canceled)
                && ink_view::starts_confirmed_pen(ui, view, &response, space));
        let multi_touch = (!pen_blocks_touch_navigation)
            .then_some(multi_touch)
            .flatten();
        let touch_canceled = touch_canceled && !pen_blocks_touch_navigation;
        let restart_after_cancel =
            restart_after_cancel.filter(|_| !pen_blocks_touch_navigation && multi_touch.is_none());
        if touch_canceled {
            view.board_input
                .touch_canceled(restart_after_cancel.is_some());
            view.touch_navigation = restart_after_cancel.is_none().then_some(false);
        }
        if minimap_blocks_pointer && any_touches {
            view.touch_navigation = Some(false);
        }
        if let Some(touch) = multi_touch {
            view.touch_navigation.get_or_insert_with(|| {
                ui.is_enabled()
                    && response.contains_pointer()
                    && rect.contains(touch.start_pos)
                    && touch.num_touches == 2
                    && !minimap_blocks_pointer
                    && !egui::Popup::is_any_open(&ctx)
            });
        }
        if !touch_canceled {
            view.board_input.resume_after_touch_cancel(&ctx);
        }
        let touch_navigation = view.touch_navigation.is_some();
        out.context_menu_allowed = !touch_navigation
            && !pen_blocks_touch_navigation
            && !minimap_blocks_pointer
            && ui.is_enabled();
        if touch_navigation || minimap_blocks_pointer {
            view.drag = Drag::None;
            view.relationship_drag = None;
        }
        if !touch_navigation
            && !pen_blocks_touch_navigation
            && !minimap_blocks_pointer
            && ui.is_enabled()
            && response.hovered()
            && view.relationship_drag.is_none()
            && view.image_resize.is_none()
            && !matches!(view.drag, Drag::Topics { .. } | Drag::Zone { .. })
            && !view.board_input.is_active()
            && !view.ink_controller.has_pending_gesture()
        {
            let (scroll, zoom_delta, pos) =
                ctx.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.pointer.hover_pos()));
            if zoom_delta != 1.0 {
                view.zoom_by(zoom_delta, pos);
            }
            if scroll != EVec2::ZERO {
                view.offset += scroll;
            }
        }

        let ink_captured = ink_view::interact(
            ui,
            doc,
            view,
            &scene,
            &response,
            ink_view::Routing {
                minimap_pointer: minimap_blocks_pointer,
                minimap_keyboard: minimap_blocks_keyboard,
                touch_navigation,
                previous_touch_navigation,
                restart_after_cancel,
                space,
            },
            &mut out,
        );
        if touch_navigation {
            if !ui.is_enabled()
                || egui::Popup::is_any_open(&ctx)
                || multi_touch.is_some_and(|touch| touch.num_touches != 2)
            {
                view.touch_navigation = Some(false);
            }
            if view.touch_navigation == Some(true)
                && let Some(touch) = multi_touch
            {
                view.zoom_by(
                    touch.zoom_delta,
                    Some(touch.center_pos - touch.translation_delta),
                );
                view.offset += touch.translation_delta;
            }
        }
        let ink_tool = view.board_tool.ink_mode().is_some();
        let mut board_pointer = board_input::interact(
            ui,
            doc,
            view,
            &layout,
            &scene,
            &response,
            pointer,
            !space
                && !touch_navigation
                && !minimap_blocks_pointer
                && !ink_tool
                && !ink_captured
                && ui.is_enabled()
                && !egui::Popup::is_any_open(&ctx),
            restart_after_cancel.unwrap_or(0),
            &mut out,
        );
        board_pointer |= ink_tool || ink_captured;
        out.context_menu_allowed &= !ink_tool && !ink_captured;
        if ink_tool
            && !ink_captured
            && !touch_navigation
            && !minimap_blocks_pointer
            && ui.is_enabled()
            && !egui::Popup::is_any_open(&ctx)
            && response.drag_started()
            && (space
                || response.dragged_by(PointerButton::Middle)
                || response.dragged_by(PointerButton::Secondary))
        {
            view.drag = Drag::Pan;
        }
        let image_handle = image_resize::interact(
            ui,
            doc,
            view,
            &layout,
            &mut out.error,
            !space
                && !touch_navigation
                && !minimap_blocks_pointer
                && !board_pointer
                && view.board_tool == BoardTool::Select
                && view.board_selection.is_empty(),
        );
        let relationship_handle = crate::relationship_editing::interact(
            ui,
            doc,
            view,
            &mut layout,
            &scene,
            &mut out.error,
            !space
                && !touch_navigation
                && !minimap_blocks_pointer
                && !image_handle
                && !board_pointer,
        );
        let handles = if space
            || touch_navigation
            || minimap_blocks_pointer
            || board_pointer
            || matches!(view.drag, Drag::Topics { .. } | Drag::Zone { .. })
        {
            Vec::new()
        } else {
            add_handles(view, doc.sheet(), &layout)
        };
        let over_handle = image_handle
            || relationship_handle
            || pointer.is_some_and(|p| handles.iter().any(|h| h.rect().contains(p)));
        let press_on_handle = image_handle
            || relationship_handle
            || ctx
                .input(|i| i.pointer.press_origin())
                .is_some_and(|p| handles.iter().any(|h| h.rect().contains(p)));
        let mut handle_action = None;
        for handle in &handles {
            let target = ui.interact(
                handle.rect(),
                id.with((view.primary(), handle.action == Action::InsertChild)),
                Sense::click(),
            );
            target
                .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &handle.label));
            let keyboard_click = target.has_focus()
                && ctx.input_mut(|i| {
                    i.consume_key(Modifiers::NONE, Key::Enter)
                        || i.consume_key(Modifiers::NONE, Key::Space)
                });
            if target.clicked() || keyboard_click {
                handle_action = Some(handle.action.clone());
            }
            target
                .on_hover_text(&handle.label)
                .on_hover_cursor(egui::CursorIcon::PointingHand);
        }
        if !board_pointer
            && !touch_navigation
            && !minimap_blocks_pointer
            && response.drag_started()
            && (!press_on_handle
                || response.dragged_by(PointerButton::Middle)
                || response.dragged_by(PointerButton::Secondary))
        {
            let origin = ctx.input(|i| i.pointer.press_origin()).unwrap_or_default();
            let mods = ctx.input(|i| i.modifiers);
            let sp = view.xf().sheet(origin);
            if response.dragged_by(PointerButton::Middle)
                || response.dragged_by(PointerButton::Secondary)
                || space
            {
                view.drag = Drag::Pan;
            } else if let Some(t) = scene_topic_hit(&scene, sp) {
                if !view.selection.contains(&t) {
                    if doc.sheet().board_member_for_topic(t).is_some() {
                        board_input::select_hit(view, doc.sheet(), SceneHit::Topic(t), false);
                    } else {
                        view.select_only(t);
                    }
                }
                view.commit_edit(doc);
                let ids = crate::topic_drag::roots(doc.sheet(), &view.selection);
                if ids.is_empty() {
                    view.drag = Drag::None;
                    out.error = Some(thinoda_core::EditError::RootNotAllowed);
                } else {
                    view.drag = Drag::Topics {
                        members: crate::topic_drag::members(doc.sheet(), &ids),
                        selection: view.selection.clone(),
                        ids,
                        grab: sp,
                        current: sp,
                        state: doc.state_id(),
                        sheet: doc.active_index(),
                    };
                }
            } else if matches!(scene.hit_test(sp), Some(SceneHit::Object(_))) {
                view.drag = Drag::None;
            } else if let Some(ObjectSelection::Zone(id)) =
                hit_zone(doc.sheet(), sp, 6.0 / view.zoom)
                    .filter(|_| hit_object(doc.sheet(), &layout, sp, 6.0 / view.zoom).is_none())
            {
                view.commit_edit(doc);
                let zone = doc
                    .sheet()
                    .zones
                    .iter()
                    .find(|zone| zone.id == id)
                    .expect("hit-tested zone exists");
                let frame = zone.frame;
                let members = crate::topic_drag::members(doc.sheet(), &zone.roots);
                let resize = (sp - frame.max()).length() <= 12.0 / view.zoom;
                let board_members = if resize {
                    Ok(Vec::new())
                } else {
                    crate::topic_drag::zone_board_members(doc.sheet(), &zone.roots, &zone.objects)
                };
                view.select_object(ObjectSelection::Zone(id));
                match board_members {
                    Ok(board_members) => {
                        let moving_topics = if resize {
                            std::collections::BTreeSet::new()
                        } else {
                            crate::topic_drag::zone_moving_topics(
                                doc.sheet(),
                                &members,
                                &board_members,
                            )
                        };
                        view.drag = Drag::Zone {
                            id,
                            frame,
                            members,
                            board_members,
                            moving_topics,
                            grab: sp,
                            current: sp,
                            state: doc.state_id(),
                            sheet: doc.active_index(),
                            camera: (view.offset, view.zoom),
                            resize,
                        };
                    }
                    Err(error) => {
                        view.drag = Drag::None;
                        out.error = Some(error);
                    }
                };
            } else if any_touches {
                view.drag = Drag::Pan;
            } else {
                view.drag = Drag::Rubber {
                    start: origin,
                    additive: mods.command || mods.shift,
                };
            }
        }
        let stale = matches!(&view.drag, Drag::Topics { state, sheet, selection, .. }
            if *state != doc.state_id() || *sheet != doc.active_index() || *selection != view.selection)
            || matches!(&view.drag, Drag::Zone { id, state, sheet, camera, .. }
                if *state != doc.state_id() || *sheet != doc.active_index()
                    || view.selected_object != Some(ObjectSelection::Zone(*id))
                    || *camera != (view.offset, view.zoom));
        if stale {
            view.drag = Drag::None;
            out.error = Some(thinoda_core::EditError::Invalid(
                "canvas dragging canceled because the document, camera or selection changed".into(),
            ));
        } else if matches!(view.drag, Drag::Topics { .. } | Drag::Zone { .. })
            && (egui::Popup::is_any_open(&ctx)
                || !ui.is_enabled()
                || !ctx.input(|input| input.focused)
                || ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)))
        {
            view.drag = Drag::None;
        }
        if matches!(view.drag, Drag::Topics { .. })
            && ctx.input(|i| i.pointer.primary_down())
            && let Some(pointer) = pointer
        {
            crate::topic_drag::edge_pan(&ctx, view, pointer);
        }
        if !minimap_blocks_pointer && (response.dragged() || response.drag_stopped()) {
            let delta = response.drag_delta();
            let xf = view.xf();
            match &mut view.drag {
                Drag::Pan => view.offset += delta,
                Drag::Topics { current, .. } | Drag::Zone { current, .. } => {
                    if let Some(p) = pointer {
                        *current = xf.sheet(p);
                    }
                }
                _ => {}
            }
        }

        // ---- clicks --------------------------------------------------------------------
        let editing_before_click = view.editing.is_some();
        let mods = ctx.input(|i| i.modifiers);
        if !board_pointer
            && !touch_navigation
            && !minimap_blocks_pointer
            && response.clicked()
            && !over_handle
            && !press_on_handle
            && let Some(p) = response.interact_pointer_pos()
        {
            let sp = view.xf().sheet(p);
            if let Some(b) = hit_badge(doc.sheet(), &layout, sp) {
                view.select_only(b);
                run(doc, view, Action::ToggleCollapse, &mut out);
            } else if let Some(t) = scene_topic_hit(&scene, sp) {
                let tl = &layout.topics[&t];
                let rotation = topic_rotation(doc.sheet(), tl);
                let local = if rotation == 0.0 {
                    sp
                } else {
                    rotate_around(sp, tl.rect.center(), -rotation)
                };
                if let Some(from) = view.relationship_from.take() {
                    match doc.edit_sheet("Add Relationship", |s| s.add_relationship(from, t)) {
                        Ok(r) => {
                            view.select_object(ObjectSelection::Relationship(r));
                        }
                        Err(e) => out.error = Some(e),
                    }
                } else if tl.todo_rect.is_some_and(|r| r.expand(2.0).contains(local)) {
                    let cur = doc.sheet().topic(t).and_then(|x| x.todo);
                    let next = !tl
                        .todo_progress
                        .map_or(cur.unwrap_or(false), |p| p.complete());
                    view.commit_edit(doc);
                    if let Err(e) = doc.edit_sheet("To-Do", |s| s.set_todo_subtree(t, next)) {
                        out.error = Some(e);
                    }
                } else if let Some((Icon::Link, _)) =
                    tl.icons.iter().find(|(_, r)| r.expand(2.0).contains(local))
                {
                    out.open_link = doc.sheet().topic(t).and_then(|x| x.hyperlink.clone());
                } else {
                    if view.editing_topic() != Some(t) {
                        view.commit_edit(doc);
                    }
                    if doc.sheet().board_member_for_topic(t).is_some() {
                        board_input::select_hit(
                            view,
                            doc.sheet(),
                            SceneHit::Topic(t),
                            mods.command || mods.shift,
                        );
                    } else if mods.command || mods.shift {
                        view.toggle_selected(t);
                    } else {
                        view.select_only(t);
                    }
                }
            } else if matches!(scene.hit_test(sp), Some(SceneHit::Object(_))) {
                // Board objects are rendered and occlude the map; object editing is a
                // later interaction slice and must not fall through to a topic below.
            } else if let Some(object) = hit_object(doc.sheet(), &layout, sp, 6.0 / view.zoom)
                .or_else(|| hit_zone(doc.sheet(), sp, 6.0 / view.zoom))
            {
                view.commit_edit(doc);
                view.select_object(object);
            } else {
                view.commit_edit(doc);
                if let Some(from) = view.relationship_from.take() {
                    let title = view.titles.floating.clone();
                    match doc.edit_sheet("Add Connected Floating Topic", |s| {
                        let target = s.add_floating(title, sp);
                        s.add_relationship(from, target)?;
                        Ok(target)
                    }) {
                        Ok(target) => {
                            view.select_only(target);
                            view.begin_edit(doc.sheet(), target, true);
                        }
                        Err(e) => out.error = Some(e),
                    }
                } else if !(mods.command || mods.shift) {
                    view.selection.clear();
                    view.board_selection.clear();
                    view.selected_object = None;
                }
            }
        }
        if !board_pointer
            && !touch_navigation
            && !minimap_blocks_pointer
            && response.double_clicked()
            && !editing_before_click
            && !over_handle
            && !press_on_handle
            && let Some(p) = response.interact_pointer_pos()
        {
            let sp = view.xf().sheet(p);
            let topic_hit = scene_topic_hit(&scene, sp);
            if let Some(t) = topic_hit {
                if doc.sheet().board_member_for_topic(t).is_some() {
                    board_input::select_hit(view, doc.sheet(), SceneHit::Topic(t), false);
                } else {
                    view.select_only(t);
                }
                if doc.sheet().fim.is_some() {
                    out.authoring_action = Some(Action::StartEdit);
                } else {
                    view.begin_edit(doc.sheet(), t, true);
                }
            } else if scene.hit_test(sp).is_none()
                && hit_badge(doc.sheet(), &layout, sp).is_none()
                && hit_object(doc.sheet(), &layout, sp, 6.0 / view.zoom).is_none()
                && hit_zone(doc.sheet(), sp, 6.0 / view.zoom).is_none()
            {
                run(doc, view, Action::InsertFloating(sp), &mut out);
            }
        }
        if !board_pointer
            && !touch_navigation
            && !minimap_blocks_pointer
            && response.secondary_clicked()
            && !over_handle
            && let Some(p) = response.interact_pointer_pos()
        {
            view.context_marker = None;
            view.commit_edit(doc);
            let point = view.xf().sheet(p);
            view.context_point = Some(point);
            if let Some(t) = scene_topic_hit(&scene, point) {
                view.context_marker = layout.topics.get(&t).and_then(|topic| {
                    topic
                        .markers
                        .iter()
                        .position(|(_, r)| r.contains(point))
                        .map(|index| (t, index))
                });
                if !view.selection.contains(&t) {
                    if doc.sheet().board_member_for_topic(t).is_some() {
                        board_input::select_hit(view, doc.sheet(), SceneHit::Topic(t), false);
                    } else {
                        view.select_only(t);
                    }
                } else if doc.sheet().board_member_for_topic(t).is_some() {
                    board_input::expand_selection(view, doc.sheet());
                }
                out.context_topic = Some(t);
            } else if matches!(scene.hit_test(point), Some(SceneHit::Object(_))) {
                // Keep board-object clicks from activating a topic obscured beneath them.
            } else if let Some(object) = hit_object(doc.sheet(), &layout, point, 6.0 / view.zoom)
                .or_else(|| hit_zone(doc.sheet(), point, 6.0 / view.zoom))
            {
                view.select_object(object);
            } else {
                view.selection.clear();
                view.board_selection.clear();
                view.selected_object = None;
            }
        }

        // ---- drag end ------------------------------------------------------------------
        if !minimap_blocks_pointer && response.drag_stopped() {
            match std::mem::replace(&mut view.drag, Drag::None) {
                Drag::Rubber { start, additive } => {
                    view.selected_object = None;
                    let end = pointer.unwrap_or(start);
                    let a = view.xf().sheet(start);
                    let b = view.xf().sheet(end);
                    let area = thinoda_core::Rect::from_min_size(
                        Vec2::new(a.x.min(b.x), a.y.min(b.y)),
                        Vec2::new((a.x - b.x).abs(), (a.y - b.y).abs()),
                    );
                    if !additive {
                        view.selection.clear();
                        view.board_selection.clear();
                    }
                    for hit in scene.objects_in(area) {
                        match hit {
                            SceneHit::Topic(id) if !view.selection.contains(&id) => {
                                view.selection.push(id);
                            }
                            SceneHit::Object(id) => {
                                for member in doc.sheet().canvas.expand_to_groups(&[id]) {
                                    if !view.board_selection.contains(&member) {
                                        view.board_selection.push(member);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    board_input::expand_selection(view, doc.sheet());
                }
                Drag::Topics {
                    ids, grab, current, ..
                } => {
                    if pointer.is_some_and(|p| rect.contains(p)) {
                        drop_topics(doc, view, &layout, &ids, grab, current, &mut out);
                    } else {
                        out.error = Some(thinoda_core::EditError::Invalid(
                            "drop a topic inside the map canvas".into(),
                        ));
                    }
                }
                Drag::Zone {
                    id,
                    frame,
                    grab,
                    current,
                    resize,
                    ..
                } => {
                    if pointer.is_some_and(|point| rect.contains(point)) {
                        let delta = current - grab;
                        let result = if resize {
                            doc.resize_zone(
                                id,
                                Vec2::new(
                                    (frame.size.x + delta.x).max(100.0),
                                    (frame.size.y + delta.y).max(100.0),
                                ),
                            )
                        } else {
                            doc.move_zone(id, delta)
                        };
                        if let Err(error) = result {
                            out.error = Some(error);
                        }
                    } else {
                        out.error = Some(thinoda_core::EditError::Invalid(
                            "drop a Zone inside the map canvas".into(),
                        ));
                    }
                }
                _ => {}
            }
        } else if matches!(view.drag, Drag::Topics { .. } | Drag::Zone { .. })
            && !ctx.input(|i| i.pointer.primary_down())
        {
            view.drag = Drag::None;
        }

        // The synthesized pointer release can arrive after the final raw touch event.
        if !any_touches && (touch_canceled || !ctx.input(|input| input.pointer.primary_down())) {
            view.touch_navigation = None;
        }

        if doc.state_id() != view.cache.as_ref().map_or(0, |cache| cache.key.0) {
            layout = view.layout_with(&ctx, doc);
        }
        let keyboard_scene = build_scene(doc.sheet(), &layout);
        let board_focused = board_input::accessibility(ui, view, doc.sheet(), &keyboard_scene);

        let next_minimap = minimap_rect
            .filter(|_| {
                ui.is_enabled()
                    && !egui::Popup::is_any_open(&ctx)
                    && view.zoom.is_finite()
                    && view.zoom > 0.0
            })
            .map(|_| id.with("minimap"));
        if let Some(minimap_id) = next_minimap {
            // Foreground areas are outside the canvas' sibling Tab order.
            // This focus-only proxy bridges that order without a duplicate a11y node.
            let proxy_id = id.with("minimap-tab-proxy");
            if ctx.memory(|memory| memory.has_focus(proxy_id)) {
                ctx.memory_mut(|memory| memory.request_focus(minimap_id));
                minimap_blocks_keyboard = true;
                out.minimap_blocks_keyboard = true;
            }
            if minimap_blocks_keyboard {
                ctx.memory_mut(|memory| {
                    memory.set_focus_lock_filter(
                        minimap_id,
                        EventFilter {
                            tab: true,
                            horizontal_arrows: true,
                            vertical_arrows: true,
                            escape: true,
                        },
                    );
                });
                if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) {
                    ctx.memory_mut(|memory| memory.request_focus(id));
                }
            }
            let direction = minimap_blocks_keyboard
                .then(|| {
                    ctx.input_mut(|input| {
                        if input.consume_key(Modifiers::SHIFT, Key::Tab) {
                            Some(egui::FocusDirection::Previous)
                        } else if input.consume_key(Modifiers::NONE, Key::Tab) {
                            Some(egui::FocusDirection::Next)
                        } else {
                            None
                        }
                    })
                })
                .flatten();
            ctx.memory_mut(|memory| {
                if let Some(direction) = direction {
                    memory.request_focus(proxy_id);
                    memory.move_focus(direction);
                }
                memory.interested_in_focus(proxy_id, ui.layer_id());
            });
        }

        // ---- keyboard ------------------------------------------------------------------
        if let Some(action) = handle_action {
            run(doc, view, action, &mut out);
        }
        let canvas_keyboard = view.editing.is_none()
            && view.editing_board_object().is_none()
            && !minimap_blocks_keyboard
            && view.image_resize.is_none()
            && !egui::Popup::is_any_open(&ctx)
            && !matches!(view.drag, Drag::Topics { .. } | Drag::Zone { .. })
            && !view.board_input.is_active()
            && (response.has_focus()
                || board_focused
                || view.board_input.has_control_focus()
                || ctx.memory(|m| m.focused()).is_none());
        let ink_cancel = canvas_keyboard && ink_view::cancel_on_escape(ui, view, &mut out);
        let ink_retry = canvas_keyboard && ink_view::retry_on_enter(ui, doc, view, &mut out);
        if canvas_keyboard
            && !ink_captured
            && !ink_cancel
            && !ink_retry
            && !board_input::keyboard(ui, doc, view, &layout, &keyboard_scene, &mut out)
        {
            handle_keys(&ctx, doc, view, &mut out, shortcuts);
        }

        if doc.state_id() != view.cache.as_ref().map_or(0, |c| c.key.0) || view.editing.is_some() {
            layout = view.layout_with(&ctx, doc);
        }
        if let Some(r) = view.pending_reveal.take()
            && matches!(view.drag, Drag::None)
            && !view.board_input.is_active()
        {
            view.do_reveal(doc.sheet(), &layout, r);
        }

        // ---- paint ---------------------------------------------------------------------
        let painter = ui.painter_at(rect);
        let sheet = doc.sheet();
        painter.rect_filled(rect, CornerRadius::ZERO, sheet_background(sheet, view.dark));
        let xf = view.xf();
        let filter = view.filter.clone();
        let dimmed = if let Drag::Topics { members, .. }
        | Drag::Zone {
            members,
            resize: false,
            ..
        } = &view.drag
        {
            Some(members)
        } else {
            None
        };
        let opts = PaintOptions {
            selection: if dimmed.is_some() {
                &[]
            } else {
                &view.selection
            },
            hovered: if dimmed.is_some() { None } else { hovered },
            editing: view.editing_topic(),
            accent: view.accent,
            dark: view.dark,
            filter: filter.as_deref(),
            dimmed,
            selected_object: view.selected_object,
            omit_zones: false,
            show_grid: true,
        };
        let paint_scene = build_scene(sheet, &layout);
        paint_map_scene(
            &painter,
            sheet,
            &layout,
            xf,
            &opts,
            &paint_scene,
            PaintSceneExtras {
                editing_board_object: view.editing_board_object(),
                ..Default::default()
            },
        );
        board_input::paint(&painter, view, sheet, &layout, &paint_scene, pointer);
        if view.ink_controller.pending_preview_sheet() == Some(doc.active_index()) {
            view.ink_controller.paint_preview(&painter, xf, view.accent);
        }
        image_resize::paint(&painter, view, sheet, &layout, !space && !touch_navigation);
        crate::relationship_editing::paint_target(&painter, view, sheet, &layout);
        if !space && !matches!(view.drag, Drag::Topics { .. } | Drag::Zone { .. }) {
            for handle in add_handles(view, sheet, &layout) {
                let hot = pointer.is_some_and(|p| handle.rect().contains(p));
                painter.line_segment(
                    [handle.anchor, handle.center],
                    Stroke::new(1.5, view.accent),
                );
                painter.circle_filled(handle.anchor, 2.5, view.accent);
                painter.circle_filled(handle.center, if hot { 13.0 } else { 11.0 }, view.accent);
                if ctx.memory(|m| {
                    m.has_focus(id.with((view.primary(), handle.action == Action::InsertChild)))
                }) {
                    painter.circle_stroke(handle.center, 15.0, Stroke::new(2.0, view.accent));
                }
                let stroke = Stroke::new(2.0, Color32::WHITE);
                painter.line_segment(
                    [
                        handle.center - vec2(4.0, 0.0),
                        handle.center + vec2(4.0, 0.0),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        handle.center - vec2(0.0, 4.0),
                        handle.center + vec2(0.0, 4.0),
                    ],
                    stroke,
                );
            }
        }

        match &view.drag {
            Drag::Rubber { start, .. } => {
                if let Some(p) = pointer {
                    let r = Rect::from_two_pos(*start, p);
                    painter.rect(
                        r,
                        CornerRadius::same(2),
                        view.accent.gamma_multiply(0.12),
                        Stroke::new(1.0, view.accent),
                        StrokeKind::Inside,
                    );
                }
            }
            Drag::Topics {
                members,
                grab,
                current,
                ..
            } => {
                let d = *current - *grab;
                crate::topic_drag::paint(
                    &painter,
                    sheet,
                    &layout,
                    &paint_scene,
                    view,
                    members,
                    d,
                    *current,
                );
            }
            Drag::Zone {
                id,
                frame,
                members,
                board_members,
                moving_topics,
                grab,
                current,
                resize,
                ..
            } => {
                let delta = *current - *grab;
                let preview = if *resize {
                    thinoda_core::Rect::from_min_size(
                        frame.min,
                        Vec2::new(
                            (frame.size.x + delta.x).max(100.0),
                            (frame.size.y + delta.y).max(100.0),
                        ),
                    )
                } else {
                    crate::topic_drag::paint_ghost(
                        &painter,
                        sheet,
                        &layout,
                        &paint_scene,
                        view,
                        members,
                        delta,
                        Some(*id),
                    );
                    crate::topic_drag::paint_zone_members(
                        &painter,
                        sheet,
                        &paint_scene,
                        view,
                        *id,
                        members,
                        board_members,
                        moving_topics,
                        delta,
                    );
                    thinoda_core::Rect::from_min_size(frame.min + delta, frame.size)
                };
                painter.rect_stroke(
                    xf.rect(preview),
                    4.0,
                    Stroke::new(2.0, view.accent),
                    StrokeKind::Inside,
                );
            }
            _ => {}
        }
        if let (Some(from), Some(p)) = (view.relationship_from, pointer)
            && let Some(r) = layout.rect(from)
        {
            let s = xf.pos(r.center());
            painter.add(egui::Shape::dashed_line(
                &[s, p],
                Stroke::new(1.5, view.accent),
                6.0,
                4.0,
            ));
        }

        // ---- inline editor ---------------------------------------------------------------
        show_editor(ui, doc, view, &layout, &mut out);
        board_input::show_label_editor(
            ui,
            doc,
            view,
            &paint_scene,
            out.minimap_blocks_keyboard,
            &mut out.error,
            false,
        );

        if response.hovered() && !minimap_blocks_pointer {
            let sheet = doc.sheet();
            let zone_hover = pointer.and_then(|point| {
                let point = xf.sheet(point);
                if scene.hit_test(point).is_some()
                    || hit_object(sheet, &layout, point, 6.0 / view.zoom).is_some()
                {
                    return None;
                }
                let ObjectSelection::Zone(id) = hit_zone(sheet, point, 6.0 / view.zoom)? else {
                    return None;
                };
                sheet
                    .zones
                    .iter()
                    .find(|zone| zone.id == id)
                    .map(|zone| (point - zone.frame.max()).length() <= 12.0 / view.zoom)
            });
            let icon = match view.drag {
                Drag::Pan => egui::CursorIcon::Grabbing,
                Drag::Topics {
                    ref members,
                    current,
                    ..
                } if crate::topic_drag::target(
                    doc.sheet(),
                    &layout,
                    &paint_scene,
                    members,
                    current,
                )
                .is_err() =>
                {
                    egui::CursorIcon::NotAllowed
                }
                Drag::Topics { .. } => egui::CursorIcon::Grabbing,
                Drag::Zone { resize: true, .. } => egui::CursorIcon::ResizeNwSe,
                Drag::Zone { .. } => egui::CursorIcon::Grabbing,
                _ if space || view.board_tool == BoardTool::Hand => egui::CursorIcon::Grab,
                _ if view.board_input.outline_hover.is_some() => egui::CursorIcon::Crosshair,
                _ if view.board_tool != BoardTool::Select && !view.board_input.armed_hover => {
                    egui::CursorIcon::Crosshair
                }
                _ if zone_hover == Some(true) => egui::CursorIcon::ResizeNwSe,
                _ if zone_hover == Some(false) => egui::CursorIcon::Grab,
                _ if image_handle => image_resize::cursor(view, doc.sheet(), &layout),
                _ if hovered.is_some() || over_handle => egui::CursorIcon::PointingHand,
                _ => egui::CursorIcon::Default,
            };
            ctx.set_cursor_icon(icon);
        }

        out.selection_changed = before_sel != view.selection
            || before_board_selection != view.board_selection
            || before_object != view.selected_object;
        board_input::end_focus_exit(&ctx, view, next_minimap);
        out
    }
}

fn run(doc: &mut Document, view: &mut MapView, action: Action, out: &mut CanvasOutput) {
    if doc.sheet().fim.is_some()
        && matches!(
            action,
            Action::InsertChild
                | Action::InsertSibling
                | Action::InsertSiblingBefore
                | Action::InsertParent
                | Action::InsertFloating(_)
                | Action::Delete
                | Action::DeleteKeepChildren
                | Action::Duplicate
                | Action::StartEdit
                | Action::Paste(_)
        )
    {
        out.authoring_action = Some(action);
        return;
    }
    if let Err(e) = apply_action(doc, view, action) {
        out.error = Some(e);
    }
}

/// Ordered map key bindings, excluding clipboard events and Escape cancellation.
pub fn shortcut_bindings() -> Vec<(egui::KeyboardShortcut, Action)> {
    const CMD: Modifiers = Modifiers::COMMAND;
    let shift_cmd = Modifiers::COMMAND.plus(Modifiers::SHIFT);
    let shortcuts: &[(Modifiers, Key, Action)] = &[
        (shift_cmd, Key::Z, Action::Redo),
        (CMD, Key::Z, Action::Undo),
        (CMD, Key::Y, Action::Redo),
        (CMD, Key::Enter, Action::InsertParent),
        (Modifiers::SHIFT, Key::Enter, Action::InsertSiblingBefore),
        (Modifiers::NONE, Key::Enter, Action::InsertSibling),
        (Modifiers::NONE, Key::Tab, Action::InsertChild),
        (Modifiers::NONE, Key::Insert, Action::InsertChild),
        // XMind: Ctrl+Delete (Windows) / Cmd+Backspace (macOS) deletes only the topic.
        (CMD, Key::Delete, Action::DeleteKeepChildren),
        (CMD, Key::Backspace, Action::DeleteKeepChildren),
        (shift_cmd, Key::Delete, Action::DeleteKeepChildren),
        (Modifiers::NONE, Key::Delete, Action::Delete),
        (Modifiers::NONE, Key::Backspace, Action::Delete),
        (Modifiers::ALT, Key::ArrowUp, Action::MoveUp),
        (Modifiers::ALT, Key::ArrowDown, Action::MoveDown),
        (
            Modifiers::NONE,
            Key::ArrowUp,
            Action::Navigate(Direction::Up),
        ),
        (
            Modifiers::NONE,
            Key::ArrowDown,
            Action::Navigate(Direction::Down),
        ),
        (
            Modifiers::NONE,
            Key::ArrowLeft,
            Action::Navigate(Direction::Left),
        ),
        (
            Modifiers::NONE,
            Key::ArrowRight,
            Action::Navigate(Direction::Right),
        ),
        (
            Modifiers::COMMAND.plus(Modifiers::ALT),
            Key::Slash,
            Action::ToggleCollapseAll,
        ),
        (shift_cmd, Key::Slash, Action::ToggleCollapseAll),
        (CMD, Key::Slash, Action::ToggleCollapse),
        (CMD, Key::R, Action::SelectRoot),
        (shift_cmd, Key::A, Action::SelectNone),
        (CMD, Key::A, Action::SelectAll),
        (Modifiers::NONE, Key::Home, Action::SelectRoot),
        (Modifiers::NONE, Key::F2, Action::StartEdit),
        (Modifiers::NONE, Key::Space, Action::StartEdit),
        (CMD, Key::D, Action::Duplicate),
        (CMD, Key::Plus, Action::ZoomIn),
        (CMD, Key::Equals, Action::ZoomIn),
        (CMD, Key::Minus, Action::ZoomOut),
        (CMD, Key::Num0, Action::ZoomReset),
        (shift_cmd, Key::H, Action::Fit),
    ];
    shortcuts
        .iter()
        .map(|(modifiers, key, action)| {
            (
                egui::KeyboardShortcut::new(*modifiers, *key),
                action.clone(),
            )
        })
        .collect()
}

fn handle_keys(
    ctx: &egui::Context,
    doc: &mut Document,
    view: &mut MapView,
    out: &mut CanvasOutput,
    shortcuts: &[(egui::KeyboardShortcut, Action)],
) {
    for (shortcut, action) in shortcuts {
        if ctx.input_mut(|i| i.consume_shortcut(shortcut)) {
            if view.host_handles_clipboard
                && matches!(action, Action::Copy | Action::Cut | Action::Paste(_))
            {
                out.clipboard_requests.push(action.clone());
            } else {
                run(doc, view, action.clone(), out);
            }
            return;
        }
    }
    if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
        view.relationship_from = None;
        view.selected_object = None;
        view.selection.clear();
        return;
    }
    // Clipboard and type-to-edit come in as events.
    let events = ctx.input(|i| i.events.clone());
    for ev in events {
        match ev {
            egui::Event::Copy => {
                if view.host_handles_clipboard {
                    out.clipboard_requests.push(Action::Copy);
                } else {
                    run(doc, view, Action::Copy, out);
                    if let Some(t) = view.clipboard_out.take() {
                        ctx.copy_text(t);
                    }
                }
            }
            egui::Event::Cut => {
                if view.host_handles_clipboard {
                    out.clipboard_requests.push(Action::Cut);
                } else {
                    run(doc, view, Action::Cut, out);
                    if let Some(t) = view.clipboard_out.take() {
                        ctx.copy_text(t);
                    }
                }
            }
            egui::Event::Paste(text) => {
                let action = Action::Paste(Some(text));
                if view.host_handles_clipboard {
                    out.clipboard_requests.push(action);
                } else {
                    run(doc, view, action, out);
                }
            }
            egui::Event::Text(text) => {
                let ok = !text.trim().is_empty()
                    && !ctx.input(|i| i.modifiers.command || i.modifiers.alt);
                if ok
                    && view.selection.len() == 1
                    && let Some(t) = view.primary()
                {
                    if doc.sheet().fim.is_some() {
                        out.authoring_action = Some(Action::StartEdit);
                        out.authoring_text = Some(text);
                    } else {
                        view.begin_edit_with(doc.sheet(), t, &text);
                    }
                    break;
                }
            }
            _ => {}
        }
    }
}

fn show_editor(
    ui: &mut Ui,
    doc: &mut Document,
    view: &mut MapView,
    layout: &Layout,
    out: &mut CanvasOutput,
) {
    let Some(edit) = view.editing.as_ref() else {
        return;
    };
    let Some(tl) = layout.topics.get(&edit.topic) else {
        view.editing = None;
        return;
    };
    let xf = view.xf();
    let r = xf.rect(topic_editor_rect(doc.sheet(), tl)).expand(2.0);
    let style = &tl.style;
    let text_card = doc
        .sheet()
        .topic(edit.topic)
        .and_then(|topic| topic.card)
        .is_some_and(|card| {
            matches!(
                card.kind,
                thinoda_core::CardKind::Sticky | thinoda_core::CardKind::Text
            )
        });
    let font = FontId::new(
        (style.font_size * view.zoom).max(4.0),
        crate::measure::font_family(ui.ctx(), style.font_family.as_deref()),
    );
    let color = color32(style.text_color);
    let edit_id = MapCanvas::title_editor_id();
    let companion = view
        .title_companion_frame
        .filter(|(frame, _)| *frame == (ui.ctx().viewport_id(), ui.ctx().cumulative_frame_nr()));
    let ime_active = view.title_ime_active(ui.ctx());
    let pending_before_outside_pointer = text_card
        && !ime_active
        && !out.minimap_blocks_keyboard
        && ui.memory(|memory| memory.had_focus_last_frame(edit_id))
        && ui.input(|input| {
            let mut pending = false;
            input.events.iter().any(|event| match event {
                egui::Event::Text(_) | egui::Event::Paste(_) | egui::Event::Cut => {
                    pending = true;
                    false
                }
                egui::Event::Key {
                    key: Key::Backspace | Key::Delete,
                    pressed: true,
                    ..
                } => {
                    pending = true;
                    false
                }
                egui::Event::PointerButton { pos, .. } => pending && !r.contains(*pos),
                _ => false,
            })
        });
    let previous_focus = ui.memory(|memory| memory.focused());
    if pending_before_outside_pointer && previous_focus != Some(edit_id) {
        ui.memory_mut(|memory| memory.request_focus(edit_id));
    }

    if !out.minimap_blocks_keyboard && ui.memory(|memory| memory.has_focus(edit_id)) {
        view.protect_title_ime(ui.ctx(), edit_id);
    }
    // Enter commits, Shift+Enter inserts a line break, Tab commits and adds a child.
    let (enter, tab, esc) =
        if out.minimap_blocks_keyboard || !ui.memory(|memory| memory.has_focus(edit_id)) {
            (false, false, false)
        } else {
            ui.input_mut(|i| {
                let mut enter = false;
                i.events.retain(|event| {
                    let plain_enter = matches!(
                        event,
                        egui::Event::Key {
                            key: Key::Enter,
                            pressed: true,
                            modifiers,
                            ..
                        } if modifiers.matches_exact(Modifiers::NONE)
                    );
                    enter |= plain_enter;
                    !plain_enter
                });
                (
                    enter,
                    i.consume_key(Modifiers::NONE, Key::Tab),
                    i.consume_key(Modifiers::NONE, Key::Escape),
                )
            })
        };
    // Text queued with a finish key must reach the editor before the commit.
    let edit = view.editing.as_mut().unwrap();
    if let Some((viewport, frame)) = edit.focus_retry_after
        && (viewport != ui.ctx().viewport_id() || frame < ui.ctx().cumulative_frame_nr())
    {
        edit.focus_retry_after = None;
        if viewport == ui.ctx().viewport_id()
            && companion.is_none()
            && !out.minimap_blocks_keyboard
            && ui.memory(|memory| memory.focused().is_none())
            && ui.input(|input| {
                input.focused && !input.pointer.any_pressed() && !input.pointer.any_released()
            })
        {
            ui.memory_mut(|memory| memory.request_focus(edit_id));
        }
    }
    if companion.is_some() && !ime_active && edit.focus_requested && edit.companion_cursor.is_none()
    {
        edit.companion_cursor = TextEdit::load_state(ui.ctx(), edit_id)
            .map(|state| (edit.text.clone(), state.cursor.char_range()));
    }
    let align = match style.align {
        thinoda_core::TextAlign::Left => egui::Align::LEFT,
        thinoda_core::TextAlign::Right => egui::Align::RIGHT,
        _ => egui::Align::Center,
    };
    let line_height =
        ui.fonts_mut(|fonts| fonts.row_height(&font)) + ui.spacing().extra_text_line_spacing;
    let decoration = Stroke::new((style.font_size * view.zoom / 14.0).max(0.6), color);
    let mut layouter = |ui: &Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
        let mut job = egui::text::LayoutJob::simple(
            text.as_str().to_owned(),
            font.clone(),
            color,
            wrap_width,
        );
        job.halign = align;
        job.keep_trailing_whitespace = true;
        let format = &mut job.sections[0].format;
        format.line_height = Some(line_height);
        format.italics = style.italic;
        format.underline = if style.underline {
            decoration
        } else {
            Stroke::NONE
        };
        format.strikethrough = if style.strikethrough {
            decoration
        } else {
            Stroke::NONE
        };
        ui.fonts_mut(|fonts| fonts.layout_job(job))
    };
    let mut te = TextEdit::multiline(&mut edit.text)
        .id(edit_id)
        .font(font.clone())
        .text_color(color)
        .frame(egui::Frame::NONE)
        .margin(egui::Margin::ZERO)
        .desired_width(r.width())
        .desired_rows(1)
        .align(egui::Align2([align, egui::Align::Min]))
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
    if text_card && (style.italic || style.underline || style.strikethrough) {
        te = te.layouter(&mut layouter);
    }
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(r));
    let bold_underlay = (text_card && style.bold).then(|| child.painter().add(egui::Shape::Noop));
    let pointer = pending_before_outside_pointer
        .then(|| ui.input_mut(|input| std::mem::take(&mut input.pointer)));
    let output = te.show(&mut child);
    if let Some(pointer) = pointer {
        ui.input_mut(|input| input.pointer = pointer);
        ui.memory_mut(|memory| {
            if let Some(previous) = previous_focus {
                memory.request_focus(previous);
            } else {
                memory.surrender_focus(edit_id);
            }
            if companion.is_none() {
                memory.surrender_focus(edit_id);
            }
        });
        if companion.is_some() && output.response.changed() {
            edit.companion_cursor = Some((edit.text.clone(), output.state.cursor.char_range()));
        }
    }
    if let Some(index) = bold_underlay {
        // Keep selection and IME visuals above the unselected faux-bold copy.
        let galley = ui.fonts_mut(|fonts| fonts.layout_job((*output.galley.job).clone()));
        let pos = output.galley_pos - vec2(galley.rect.left(), 0.0) + vec2(0.45 * view.zoom, 0.0);
        child
            .painter()
            .with_clip_rect(output.text_clip_rect)
            .set(index, egui::Shape::galley(pos, galley, color));
    }
    if output.response.changed() {
        view.pending_reveal = Some(edit.topic);
    }
    output.response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::MultilineTextInput, true, "Topic title")
    });
    if companion.is_some_and(|(_, refocus)| refocus) && !(enter || tab || esc) {
        ui.memory_mut(|memory| memory.request_focus(edit_id));
    }
    let refocused = companion.is_some_and(|(_, refocus)| refocus)
        || (companion.is_none() && ui.memory(|memory| memory.has_focus(edit_id)));
    if refocused
        && !ime_active
        && let Some((saved_text, cursor)) = edit.companion_cursor.take()
    {
        let clicked_editor = ui.input(|input| {
            input.pointer.any_pressed()
                && input
                    .pointer
                    .interact_pos()
                    .is_some_and(|pos| r.contains(pos))
        });
        if saved_text == edit.text && !clicked_editor {
            let mut state = output.state.clone();
            state.cursor.set_char_range(cursor);
            state.store(ui.ctx(), edit_id);
        }
    }
    if enter || tab || esc {
        view.commit_edit(doc);
        if view.editing_topic().is_some() {
            ui.memory_mut(|m| m.request_focus(edit_id));
        } else {
            ui.memory_mut(|m| m.request_focus(MapCanvas::id()));
            if tab {
                run(doc, view, Action::InsertChild, out);
            }
        }
    } else if !edit.focus_requested && !out.minimap_blocks_keyboard {
        edit.focus_requested = true;
        if ui.input(|input| input.pointer.any_pressed() || input.pointer.any_released()) {
            // egui may clear focus at the end of the creation click's pass.
            edit.focus_retry_after = Some((ui.ctx().viewport_id(), ui.ctx().cumulative_frame_nr()));
            ui.ctx().request_repaint();
        }
        ui.memory_mut(|m| m.request_focus(edit_id));
        if edit.select_all {
            let n = edit.text.chars().count();
            let mut state = output.state.clone();
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    egui::text::CCursor::new(n),
                )));
            state.store(ui.ctx(), edit_id);
        } else {
            let n = edit.text.chars().count();
            let mut state = output.state.clone();
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(
                    egui::text::CCursor::new(n),
                )));
            state.store(ui.ctx(), edit_id);
        }
    } else if companion.is_none()
        && (output.response.lost_focus()
            || (text_card
                && ui.is_enabled()
                && edit.focus_requested
                && !out.minimap_blocks_keyboard
                && ui.memory(|memory| {
                    memory
                        .focused()
                        .is_none_or(|focused| focused == MapCanvas::id())
                })))
    {
        view.commit_edit(doc);
        if view.editing_topic().is_some()
            && !out.minimap_blocks_keyboard
            && ui.memory(|memory| {
                memory
                    .focused()
                    .is_none_or(|focused| focused == MapCanvas::id() || focused == edit_id)
            })
        {
            ui.memory_mut(|memory| memory.request_focus(edit_id));
        }
    }
}

fn drop_topics(
    doc: &mut Document,
    view: &mut MapView,
    layout: &Layout,
    ids: &[TopicId],
    grab: Vec2,
    current: Vec2,
    out: &mut CanvasOutput,
) {
    let d = current - grab;
    if d.length() < 3.0 {
        return;
    }
    let ids = crate::topic_drag::roots(doc.sheet(), ids);
    if ids.is_empty() {
        return;
    }
    let target = match crate::topic_drag::target(
        doc.sheet(),
        layout,
        &build_scene(doc.sheet(), layout),
        &crate::topic_drag::members(doc.sheet(), &ids),
        current,
    ) {
        Ok(target) => target,
        Err(e) => {
            out.error = Some(e);
            return;
        }
    };
    let res = doc.edit_sheet("Move Topic", |s| {
        if let Some(target) = &target {
            crate::topic_drag::apply(s, &ids, target, layout)?;
        } else {
            for id in &ids {
                let center = layout.rect(*id).map(|r| r.center() + d).unwrap_or(current);
                if s.is_floating(*id) {
                    s.set_position(*id, center)?;
                } else {
                    s.detach_to_floating(*id, center)?;
                }
                crate::topic_drag::preserve_size_after_move(s, *id, layout)?;
            }
        }
        Ok(())
    });
    if let Err(e) = res {
        out.error = Some(e);
    } else {
        view.selection = ids;
    }
}

fn hit_relationship(sheet: &Sheet, layout: &Layout, p: Vec2, tol: f32) -> Option<RelationshipId> {
    layout.relationships.iter().rev().find_map(|r| {
        let relationship = sheet.relationships.iter().find(|item| item.id == r.id)?;
        let clip = crate::render::relationship_clip_rect(sheet, relationship.from, relationship.to);
        if clip.is_some_and(|clip| !clip.contains(p)) {
            return None;
        }
        let mut prev = r.start;
        for i in 1..=crate::render::RELATIONSHIP_SEGMENTS {
            let q = bezier(
                r.start,
                r.c1,
                r.c2,
                r.end,
                i as f32 / crate::render::RELATIONSHIP_SEGMENTS as f32,
            );
            if dist_to_segment(p, prev, q) <= tol {
                return Some(r.id);
            }
            prev = q;
        }
        None
    })
}

fn scene_topic_hit(scene: &thinoda_core::Scene, point: Vec2) -> Option<TopicId> {
    match scene.hit_test(point) {
        Some(SceneHit::Topic(id)) => Some(id),
        Some(SceneHit::Object(_)) | None => None,
    }
}

fn hit_object(sheet: &Sheet, layout: &Layout, p: Vec2, tol: f32) -> Option<ObjectSelection> {
    if let Some(id) = hit_relationship(sheet, layout, p, tol) {
        return Some(ObjectSelection::Relationship(id));
    }
    for summary in layout.summaries.iter().rev() {
        if topic_clip_rect(sheet, summary.topic).is_some_and(|clip| !clip.contains(p)) {
            continue;
        }
        let points = brace_path(summary.start, summary.end, summary.tip, summary.dir);
        let brace_hit = points
            .windows(2)
            .any(|line| dist_to_segment(p, line[0], line[1]) <= tol);
        let connector_hit = layout.rect(summary.topic).is_some_and(|topic| {
            let end = match summary.dir {
                Dir::Right => topic.left_center(),
                Dir::Left => topic.right_center(),
                Dir::Down => topic.top_center(),
                Dir::Up => topic.bottom_center(),
            };
            dist_to_segment(p, summary.tip, end) <= tol
        });
        if brace_hit || connector_hit {
            return Some(ObjectSelection::Summary(summary.id));
        }
    }
    // Match the visible outline, not the entire enclosed area. Prefer the smallest
    // boundary when nested outlines share an edge.
    layout
        .boundaries
        .iter()
        .filter(|boundary| {
            let Some(model_boundary) = sheet.boundaries.iter().find(|item| item.id == boundary.id)
            else {
                return false;
            };
            if topic_clip_rect(sheet, model_boundary.parent).is_some_and(|clip| !clip.contains(p)) {
                return false;
            }
            let rect = boundary.rect;
            let max = rect.max();
            let points = [
                rect.min,
                Vec2::new(max.x, rect.min.y),
                max,
                Vec2::new(rect.min.x, max.y),
                rect.min,
            ];
            points
                .windows(2)
                .any(|line| dist_to_segment(p, line[0], line[1]) <= tol)
        })
        .min_by(|a, b| (a.rect.size.x * a.rect.size.y).total_cmp(&(b.rect.size.x * b.rect.size.y)))
        .map(|boundary| ObjectSelection::Boundary(boundary.id))
}

fn hit_zone(sheet: &Sheet, point: Vec2, tolerance: f32) -> Option<ObjectSelection> {
    sheet
        .zones
        .iter()
        .rev()
        .find(|zone| {
            let frame = zone.frame;
            let max = frame.max();
            let corners = [
                frame.min,
                Vec2::new(max.x, frame.min.y),
                max,
                Vec2::new(frame.min.x, max.y),
                frame.min,
            ];
            corners
                .windows(2)
                .any(|edge| dist_to_segment(point, edge[0], edge[1]) <= tolerance)
                || (point.x >= frame.min.x
                    && point.x <= max.x
                    && point.y >= frame.min.y
                    && point.y <= frame.min.y + tolerance * 3.0)
        })
        .map(|zone| ObjectSelection::Zone(zone.id))
}

fn dist_to_segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let len2 = ab.x * ab.x + ab.y * ab.y;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p - a).x * ab.x + (p - a).y * ab.y) / len2).clamp(0.0, 1.0)
    };
    (p - (a + ab * t)).length()
}

#[allow(dead_code)]
fn screen_pos(view: &MapView, p: Vec2) -> Pos2 {
    let x = view.xf();
    pos2(x.origin.x + p.x * x.zoom, x.origin.y + p.y * x.zoom)
}

#[cfg(test)]
#[path = "topic_drag_tests.rs"]
mod topic_drag_tests;

#[cfg(test)]
#[path = "zone_tests.rs"]
mod zone_tests;

#[cfg(test)]
#[path = "relationship_creation_tests.rs"]
mod relationship_creation_tests;

#[cfg(test)]
#[path = "scene_hit_tests.rs"]
mod scene_hit_tests;

#[cfg(test)]
#[path = "board_tests.rs"]
mod board_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    use thinoda_core::Workbook;

    struct State {
        doc: Document,
        view: MapView,
    }

    #[test]
    fn title_edit_continuation_preserves_session_and_rejects_stale_owners() {
        let sheet = Sheet::new("Map", "Original");
        let root = sheet.root;
        let mut doc = Document::new(Workbook::new(sheet));
        let mut view = MapView::default();
        view.begin_edit(doc.sheet(), root, false);
        view.set_editing_text("Saved");
        let continuation = view.capture_title_edit_continuation().unwrap();
        assert!(view.try_commit_edit(&mut doc).unwrap());
        assert!(view.resume_title_edit_after_commit(&doc, continuation.clone()));
        assert_eq!(view.editing_text(), Some("Saved"));
        assert!(!view.has_title_changes());
        view.set_editing_text("Later");
        assert!(view.try_commit_edit(&mut doc).unwrap());
        assert!(!view.resume_title_edit_after_commit(&doc, continuation));

        view.begin_edit(doc.sheet(), root, false);
        let continuation = view.capture_title_edit_continuation().unwrap();
        view.try_commit_edit(&mut doc).unwrap();
        doc.edit_sheet("Change title", |sheet| sheet.set_title(root, "External"))
            .unwrap();
        assert!(!view.resume_title_edit_after_commit(&doc, continuation));

        view.begin_edit(doc.sheet(), root, false);
        let continuation = view.capture_title_edit_continuation().unwrap();
        view.try_commit_edit(&mut doc).unwrap();
        view.begin_edit(doc.sheet(), root, false);
        view.cancel_edit();
        assert!(!view.resume_title_edit_after_commit(&doc, continuation));
    }

    #[test]
    fn changed_formatted_card_title_is_rejected_without_losing_its_draft_or_styles() {
        use thinoda_core::{
            CardKind, InlineParagraph, InlineText, InlineTextRun, InlineTextStyle, ListKind,
            board::Card, geometry::Vec2,
        };

        for kind in [CardKind::Sticky, CardKind::Text] {
            let mut sheet = Sheet::new("Board", "Root");
            let id = sheet
                .add_card(
                    "Sticky",
                    Vec2::ZERO,
                    Card::new(kind, Vec2::new(180.0, 100.0)),
                )
                .unwrap();
            let inline_text = InlineText {
                runs: vec![InlineTextRun {
                    start: 0,
                    end: 2,
                    style: InlineTextStyle {
                        bold: Some(true),
                        ..InlineTextStyle::default()
                    },
                }],
                paragraphs: vec![InlineParagraph {
                    index: 0,
                    list: ListKind::Bulleted,
                }],
            };
            sheet.topic_mut(id).unwrap().inline_text = inline_text.clone();
            let mut doc = Document::new(Workbook::new(sheet));
            let mut view = MapView::default();

            view.begin_edit(doc.sheet(), id, false);
            view.set_editing_text("Tricky");

            assert!(!view.commit_edit(&mut doc));
            let error = view.take_title_error().unwrap();
            assert!(
                error.contains("Formatted card titles"),
                "unexpected error: {error}"
            );
            assert_eq!(doc.sheet().topic(id).unwrap().title, "Sticky");
            assert_eq!(doc.sheet().topic(id).unwrap().inline_text, inline_text);
            assert_eq!(view.editing_topic(), Some(id));
            assert_eq!(view.editing_text(), Some("Tricky"));
            assert!(!doc.undo(), "rejected title edits must not enter history");
        }
    }

    #[test]
    fn modal_snapshot_retains_the_canvas_automatic_stroke() {
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = thinoda_core::SheetRootMode::Hidden;
        sheet.add_floating("Card", Vec2::new(0.0, 100.0));
        let mut connector = thinoda_core::CanvasConnector::new(
            thinoda_core::Endpoint::Point(Vec2::new(-100.0, 0.0)),
            thinoda_core::Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.arrow_end = false;
        sheet
            .add_canvas_object(thinoda_core::CanvasObject::connector(connector))
            .unwrap();
        sheet
            .add_canvas_object(thinoda_core::CanvasObject::shape(
                thinoda_core::ShapeKind::Rectangle,
                thinoda_core::Rect::from_center_size(Vec2::new(0.0, -100.0), Vec2::new(80.0, 60.0)),
            ))
            .unwrap();
        let doc = Document::new(Workbook::new(sheet));
        for dark in [false, true] {
            for accent in [Color32::from_rgb(221, 12, 179), Color32::TRANSPARENT] {
                let mut view = MapView {
                    accent,
                    dark,
                    ..Default::default()
                };
                let ctx = egui::Context::default();
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            vec2(500.0, 400.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        MapCanvas::show_snapshot(ui, &doc, &mut view);
                    },
                );
                output.textures_delta.clear();
                let stroke = crate::automatic_board_stroke(accent, dark);
                for closed in [false, true] {
                    assert!(
                        output.shapes.iter().any(|shape| matches!(
                            &shape.shape,
                            egui::Shape::Path(path)
                                if path.closed == closed
                                    && path.stroke.color == egui::epaint::ColorMode::Solid(stroke)
                        )),
                        "snapshot missed the {} automatic stroke",
                        if closed { "shape" } else { "connector" }
                    );
                }
            }
        }
    }

    fn harness() -> Harness<'static, State> {
        let doc = Document::new(Workbook::default());
        let mut view = MapView::default();
        view.select_only(doc.sheet().root);
        Harness::builder()
            .with_size(vec2(900.0, 600.0))
            .build_ui_state(
                |ui, s: &mut State| {
                    MapCanvas::show(ui, &mut s.doc, &mut s.view);
                },
                State { doc, view },
            )
    }

    fn empty_hidden_harness() -> Harness<'static, State> {
        let mut sheet = Sheet::new("Blank board", "");
        sheet.root_mode = thinoda_core::SheetRootMode::Hidden;
        let root = sheet.root;
        let doc = Document::new(Workbook::new(sheet));
        let mut view = MapView::default();
        view.selection.push(root);
        view.pending_reveal = Some(root);
        view.relationship_from = Some(root);
        view.editing = Some(EditState {
            topic: root,
            text: String::new(),
            original: String::new(),
            select_all: false,
            focus_requested: false,
            focus_retry_after: None,
            ime_composing: false,
            companion_cursor: None,
        });
        Harness::builder()
            .with_size(vec2(900.0, 600.0))
            .build_ui_state(
                |ui, s: &mut State| {
                    MapCanvas::show(ui, &mut s.doc, &mut s.view);
                },
                State { doc, view },
            )
    }

    #[test]
    fn empty_hidden_board_clears_anchor_selection_editor_and_accessibility_input() {
        let mut h = empty_hidden_harness();
        h.run_steps(2);
        assert!(h.state().view.selection.is_empty());
        assert_eq!(h.state().view.editing_topic(), None);
        assert_eq!(h.state().view.pending_reveal, None);
        assert_eq!(h.state().view.relationship_from, None);
        assert!(h.query_by_label("Topic title").is_none());
        assert!(h.query_by_label("Add child topic").is_none());
        assert!(h.query_by_label("Mind map canvas").is_some());

        h.key_press(Key::Home);
        h.run_steps(2);
        assert!(h.state().view.selection.is_empty());
        assert_eq!(h.state().view.editing_topic(), None);
        assert!(h.query_by_label("Topic title").is_none());
    }

    fn touch(h: &mut Harness<'_, State>, id: u64, phase: egui::TouchPhase, pos: Pos2) {
        h.event(egui::Event::Touch {
            device_id: egui::TouchDeviceId(1),
            id: egui::TouchId(id),
            phase,
            pos,
            force: None,
        });
    }

    fn touch_pointer(h: &mut Harness<'_, State>, phase: egui::TouchPhase, pos: Pos2) {
        h.event(egui::Event::PointerMoved(pos));
        touch(h, 1, phase, pos);
        if matches!(phase, egui::TouchPhase::Start | egui::TouchPhase::End) {
            h.event(egui::Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: phase == egui::TouchPhase::Start,
                modifiers: Modifiers::NONE,
            });
        }
        h.run_steps(1);
    }

    fn context_harness() -> Harness<'static, State> {
        Harness::builder()
            .with_size(vec2(900.0, 600.0))
            .with_step_dt(1.0 / 60.0)
            .build_ui_state(
                |ui, s: &mut State| {
                    let out = MapCanvas::show(ui, &mut s.doc, &mut s.view);
                    assert!(out.error.is_none(), "{:?}", out.error);
                    out.context_menu(|ui| {
                        let _ = ui.button("Topic context actions");
                    });
                },
                State {
                    doc: Document::new(Workbook::default()),
                    view: MapView::default(),
                },
            )
    }

    #[test]
    fn touch_tap_selects_and_double_tap_edits_without_changing_history() {
        let mut h = context_harness();
        h.run_steps(3);
        let root = h.state().doc.sheet().root;
        let before = h.state().doc.workbook().clone();
        let revision = h.state().doc.state_id();
        h.state_mut().view.selection.clear();
        let layout = {
            let ctx = h.ctx.clone();
            let s = h.state_mut();
            s.view.layout_with(&ctx, &s.doc)
        };
        let p = h.state().view.xf().pos(layout.topics[&root].rect.center());
        touch_pointer(&mut h, egui::TouchPhase::Start, p);
        touch_pointer(&mut h, egui::TouchPhase::End, p);
        assert_eq!(h.state().view.selection, vec![root]);
        assert_eq!(h.state().view.editing_topic(), None);
        touch_pointer(&mut h, egui::TouchPhase::Start, p);
        touch_pointer(&mut h, egui::TouchPhase::End, p);
        assert_eq!(h.state().view.editing_topic(), Some(root));
        assert_eq!(h.state().doc.workbook(), &before);
        assert_eq!(h.state().doc.state_id(), revision);
    }

    #[test]
    fn touch_hold_opens_context_without_starting_a_topic_drag() {
        let mut h = context_harness();
        h.run_steps(3);
        let root = h.state().doc.sheet().root;
        let layout = {
            let ctx = h.ctx.clone();
            let s = h.state_mut();
            s.view.layout_with(&ctx, &s.doc)
        };
        let p = h.state().view.xf().pos(layout.topics[&root].rect.center());
        let before = h.state().doc.workbook().clone();
        touch_pointer(&mut h, egui::TouchPhase::Start, p);
        h.run_steps(60);
        assert!(egui::Popup::is_any_open(&h.ctx));
        assert_eq!(h.state().view.selection, vec![root]);
        assert!(matches!(h.state().view.drag, Drag::None));
        touch_pointer(&mut h, egui::TouchPhase::End, p);
        h.run_steps(3);
        assert!(egui::Popup::is_any_open(&h.ctx));
        assert_eq!(h.state().doc.workbook(), &before);
        click_at(&mut h, pos2(20.0, 20.0), PointerButton::Primary);
        assert!(!egui::Popup::is_any_open(&h.ctx));
    }

    #[test]
    fn two_finger_hold_never_opens_a_context_menu() {
        for contacts in [2, 3] {
            let mut h = context_harness();
            h.run_steps(3);
            let p = h.state().view.canvas_rect.center();
            let before = h.state().doc.workbook().clone();
            let revision = h.state().doc.state_id();
            touch_pointer(&mut h, egui::TouchPhase::Start, p);
            for id in 2..=contacts {
                touch(
                    &mut h,
                    id,
                    egui::TouchPhase::Start,
                    p + vec2(60.0 * id as f32, 0.0),
                );
            }
            h.run_steps(60);
            assert!(!egui::Popup::is_any_open(&h.ctx));
            for id in 2..=contacts {
                touch(
                    &mut h,
                    id,
                    egui::TouchPhase::End,
                    p + vec2(60.0 * id as f32, 0.0),
                );
            }
            h.run_steps(60);
            assert!(!egui::Popup::is_any_open(&h.ctx));
            touch_pointer(&mut h, egui::TouchPhase::End, p);
            assert!(!egui::Popup::is_any_open(&h.ctx));
            assert_eq!(h.state().doc.workbook(), &before);
            assert_eq!(h.state().doc.state_id(), revision);
            click_at(&mut h, p, PointerButton::Secondary);
            assert!(egui::Popup::is_any_open(&h.ctx));
        }
    }

    #[test]
    fn multitouch_tap_does_not_select_on_a_delayed_pointer_release() {
        let mut h = context_harness();
        h.run_steps(3);
        let root = h.state().doc.sheet().root;
        let layout = {
            let ctx = h.ctx.clone();
            let s = h.state_mut();
            s.view.layout_with(&ctx, &s.doc)
        };
        let p = h.state().view.xf().pos(layout.topics[&root].rect.center());
        touch_pointer(&mut h, egui::TouchPhase::Start, p);
        touch(&mut h, 2, egui::TouchPhase::Start, p + vec2(60.0, 0.0));
        h.run_steps(2);
        touch(&mut h, 2, egui::TouchPhase::End, p + vec2(60.0, 0.0));
        touch(&mut h, 1, egui::TouchPhase::End, p);
        h.run_steps(2);
        h.event(egui::Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(2);
        assert!(h.state().view.selection.is_empty());
        assert_eq!(h.state().view.editing_topic(), None);
        assert!(!egui::Popup::is_any_open(&h.ctx));
        click_at(&mut h, p, PointerButton::Primary);
        assert_eq!(h.state().view.selection, vec![root]);
    }

    #[test]
    fn two_finger_pan_and_pinch_follow_the_centroid_without_editing() {
        for scale in [1.0, 1.5, 2.0] {
            for dark in [false, true] {
                let mut h = harness();
                h.ctx.set_pixels_per_point(scale);
                h.ctx.set_visuals(if dark {
                    egui::Visuals::dark()
                } else {
                    egui::Visuals::light()
                });
                h.run_steps(3);
                let before = h.state().doc.workbook().clone();
                let selection = h.state().view.selection.clone();
                let center = h.state().view.canvas_rect.center() + vec2(80.0, 50.0);
                let sheet_point = h.state().view.xf().sheet(center);
                let zoom = h.state().view.zoom;
                h.event(egui::Event::PointerMoved(center - vec2(50.0, 0.0)));
                touch(&mut h, 1, egui::TouchPhase::Start, center - vec2(50.0, 0.0));
                touch(&mut h, 2, egui::TouchPhase::Start, center + vec2(50.0, 0.0));
                h.run_steps(1);
                let translation = vec2(30.0, 20.0);
                touch(
                    &mut h,
                    1,
                    egui::TouchPhase::Move,
                    center + translation - vec2(75.0, 0.0),
                );
                touch(
                    &mut h,
                    2,
                    egui::TouchPhase::Move,
                    center + translation + vec2(75.0, 0.0),
                );
                h.event(egui::Event::Zoom(1.5));
                h.run_steps(1);
                assert!((h.state().view.zoom - zoom * 1.5).abs() < 0.001);
                let after = h.state().view.xf().sheet(center + translation);
                assert!((after.x - sheet_point.x).abs() < 0.01);
                assert!((after.y - sheet_point.y).abs() < 0.01);
                let offset = h.state().view.offset;
                touch(
                    &mut h,
                    1,
                    egui::TouchPhase::Move,
                    center + translation * 2.0 - vec2(75.0, 0.0),
                );
                touch(
                    &mut h,
                    2,
                    egui::TouchPhase::Move,
                    center + translation * 2.0 + vec2(75.0, 0.0),
                );
                h.run_steps(1);
                assert!((h.state().view.offset - offset - translation).length() < 0.01);
                assert_eq!(h.state().doc.workbook(), &before);
                assert_eq!(h.state().view.selection, selection);
            }
        }
    }

    #[test]
    fn single_finger_background_drag_pans_instead_of_rubber_band_selection() {
        let mut h = harness();
        h.run_steps(3);
        let before = h.state().doc.workbook().clone();
        let selection = h.state().view.selection.clone();
        let p = h.state().view.canvas_rect.left_top() + vec2(40.0, 40.0);
        h.event(egui::Event::PointerMoved(p));
        h.event(egui::Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        });
        touch(&mut h, 1, egui::TouchPhase::Start, p);
        h.run_steps(1);
        h.event(egui::Event::PointerMoved(p + vec2(30.0, 20.0)));
        touch(&mut h, 1, egui::TouchPhase::Move, p + vec2(30.0, 20.0));
        h.run_steps(1);
        assert!(matches!(h.state().view.drag, Drag::Pan));
        let offset = h.state().view.offset;
        h.event(egui::Event::PointerMoved(p + vec2(60.0, 40.0)));
        touch(&mut h, 1, egui::TouchPhase::Move, p + vec2(60.0, 40.0));
        h.run_steps(1);
        assert!((h.state().view.offset - offset - vec2(30.0, 20.0)).length() < 0.01);
        touch(&mut h, 1, egui::TouchPhase::End, p + vec2(60.0, 40.0));
        h.event(egui::Event::PointerButton {
            pos: p + vec2(60.0, 40.0),
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(3);
        assert_eq!(h.state().view.selection, selection);
        assert_eq!(h.state().doc.workbook(), &before);
    }

    #[test]
    fn trackpad_pan_and_zoom_apply_together_without_double_scaling_touch() {
        let mut h = harness();
        h.run_steps(3);
        let anchor = h.state().view.canvas_rect.center();
        let zoom = h.state().view.zoom;
        let offset = h.state().view.offset;
        h.event(egui::Event::PointerMoved(anchor));
        h.event(egui::Event::Zoom(1.5));
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(30.0, 20.0),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Trackpad,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(1);
        let expected_scroll = h.ctx.input(|input| input.smooth_scroll_delta);
        assert!(expected_scroll.length() > 0.0);
        assert!((h.state().view.zoom - zoom * 1.5).abs() < 0.001);
        assert!((h.state().view.offset - offset * 1.5 - expected_scroll).length() < 0.01);
    }

    #[test]
    fn multitouch_release_and_cancel_do_not_turn_into_clicks_or_topic_drags() {
        for canceled in [false, true] {
            let mut h = harness();
            h.run_steps(3);
            let before = h.state().doc.workbook().clone();
            let state = h.state().doc.state_id();
            let selection = h.state().view.selection.clone();
            let p = h.state().view.canvas_rect.left_top() + vec2(60.0, 60.0);
            h.event(egui::Event::PointerMoved(p));
            h.event(egui::Event::PointerButton {
                pos: p,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            });
            touch(&mut h, 1, egui::TouchPhase::Start, p);
            h.run_steps(1);
            touch(&mut h, 2, egui::TouchPhase::Start, p + vec2(80.0, 0.0));
            h.run_steps(1);
            let offset = h.state().view.offset;
            touch(&mut h, 2, egui::TouchPhase::End, p + vec2(80.0, 0.0));
            h.run_steps(1);
            assert_eq!(h.state().view.offset, offset);
            h.event(egui::Event::PointerMoved(p + vec2(100.0, 80.0)));
            touch(&mut h, 1, egui::TouchPhase::Move, p + vec2(100.0, 80.0));
            h.run_steps(1);
            assert!(matches!(h.state().view.drag, Drag::None));
            let end = if canceled {
                egui::TouchPhase::Cancel
            } else {
                egui::TouchPhase::End
            };
            touch(&mut h, 1, end, p + vec2(100.0, 80.0));
            h.event(egui::Event::PointerButton {
                pos: p + vec2(100.0, 80.0),
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            });
            h.run_steps(3);
            assert_eq!(h.state().view.selection, selection);
            assert_eq!(h.state().doc.workbook(), &before);
            assert_eq!(h.state().doc.state_id(), state);
            assert_eq!(h.state().view.touch_navigation, None);
            click_at(&mut h, p, PointerButton::Primary);
            assert!(
                h.state().view.selection.is_empty(),
                "Ordinary clicks must resume"
            );
        }
    }

    #[test]
    fn canceled_touch_topic_drag_never_applies_the_pending_drop() {
        let mut h = harness();
        let root = h.state().doc.sheet().root;
        let mut child = root;
        h.state_mut()
            .doc
            .edit_sheet("Seed", |sheet| {
                child = sheet.insert_child(root, "Child")?;
                Ok(())
            })
            .unwrap();
        h.run_steps(3);
        let before = h.state().doc.workbook().clone();
        let revision = h.state().doc.state_id();
        let layout = {
            let ctx = h.ctx.clone();
            let state = h.state_mut();
            state.view.layout_with(&ctx, &state.doc)
        };
        let p = h.state().view.xf().pos(layout.topics[&child].rect.center());
        h.event(egui::Event::PointerMoved(p));
        h.event(egui::Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        });
        touch(&mut h, 1, egui::TouchPhase::Start, p);
        h.run_steps(1);
        let end = p + vec2(70.0, 60.0);
        h.event(egui::Event::PointerMoved(end));
        touch(&mut h, 1, egui::TouchPhase::Move, end);
        h.run_steps(1);
        assert!(matches!(h.state().view.drag, Drag::Topics { .. }));
        touch(&mut h, 1, egui::TouchPhase::Cancel, end);
        h.event(egui::Event::PointerButton {
            pos: end,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(3);
        assert_eq!(h.state().doc.workbook(), &before);
        assert_eq!(h.state().doc.state_id(), revision);
        assert!(matches!(h.state().view.drag, Drag::None));
    }

    #[test]
    fn multitouch_respects_canvas_ownership_disabled_ui_and_zoom_limits() {
        for outside in [false, true] {
            for disabled in [false, true] {
                let mut h = Harness::builder()
                    .with_size(vec2(900.0, 600.0))
                    .build_ui_state(
                        move |ui, state: &mut State| {
                            ui.add_enabled_ui(!disabled, |ui| {
                                MapCanvas::show(ui, &mut state.doc, &mut state.view);
                            });
                        },
                        State {
                            doc: Document::new(Workbook::default()),
                            view: MapView::default(),
                        },
                    );
                h.run_steps(3);
                let center = if outside {
                    pos2(-100.0, -100.0)
                } else {
                    h.state().view.canvas_rect.center()
                };
                let zoom = h.state().view.zoom;
                let offset = h.state().view.offset;
                h.event(egui::Event::PointerMoved(center));
                touch(&mut h, 1, egui::TouchPhase::Start, center - vec2(1.0, 0.0));
                touch(&mut h, 2, egui::TouchPhase::Start, center + vec2(1.0, 0.0));
                h.run_steps(1);
                let owned = !outside && !disabled;
                assert_eq!(h.state().view.touch_navigation, Some(owned));
                touch(&mut h, 1, egui::TouchPhase::Move, center - vec2(100.0, 0.0));
                touch(&mut h, 2, egui::TouchPhase::Move, center + vec2(100.0, 0.0));
                h.run_steps(1);
                if owned {
                    assert_eq!(h.state().view.zoom, MAX_ZOOM);
                } else {
                    assert_eq!(h.state().view.zoom, zoom);
                    assert_eq!(h.state().view.offset, offset);
                }
                touch(&mut h, 1, egui::TouchPhase::Move, center - vec2(0.1, 0.0));
                touch(&mut h, 2, egui::TouchPhase::Move, center + vec2(0.1, 0.0));
                h.run_steps(1);
                if owned {
                    assert_eq!(h.state().view.zoom, MIN_ZOOM);
                }
                let offset = h.state().view.offset;
                touch(&mut h, 3, egui::TouchPhase::Start, center);
                h.run_steps(1);
                assert_eq!(
                    h.state().view.offset,
                    offset,
                    "Finger-count changes must not jump"
                );
                assert_eq!(h.state().view.touch_navigation, Some(false));
            }
        }
    }

    #[test]
    fn disabling_canvas_during_multitouch_cancels_ownership_until_all_fingers_lift() {
        let enabled = std::rc::Rc::new(std::cell::Cell::new(true));
        let draw_enabled = enabled.clone();
        let mut h = Harness::builder()
            .with_size(vec2(900.0, 600.0))
            .build_ui_state(
                move |ui, state: &mut State| {
                    ui.add_enabled_ui(draw_enabled.get(), |ui| {
                        MapCanvas::show(ui, &mut state.doc, &mut state.view);
                    });
                },
                State {
                    doc: Document::new(Workbook::default()),
                    view: MapView::default(),
                },
            );
        h.run_steps(3);
        let center = h.state().view.canvas_rect.center();
        h.event(egui::Event::PointerMoved(center));
        touch(&mut h, 1, egui::TouchPhase::Start, center - vec2(40.0, 0.0));
        touch(&mut h, 2, egui::TouchPhase::Start, center + vec2(40.0, 0.0));
        h.run_steps(1);
        assert_eq!(h.state().view.touch_navigation, Some(true));
        let offset = h.state().view.offset;
        let zoom = h.state().view.zoom;
        enabled.set(false);
        h.run_steps(1);
        enabled.set(true);
        touch(&mut h, 1, egui::TouchPhase::Move, center - vec2(80.0, 0.0));
        touch(&mut h, 2, egui::TouchPhase::Move, center + vec2(80.0, 0.0));
        h.run_steps(1);
        assert_eq!(h.state().view.touch_navigation, Some(false));
        assert_eq!(h.state().view.zoom, zoom);
        assert_eq!(h.state().view.offset, offset);
        touch(&mut h, 1, egui::TouchPhase::End, center);
        touch(&mut h, 2, egui::TouchPhase::End, center);
        h.run_steps(1);
        assert_eq!(h.state().view.touch_navigation, None);
    }

    #[test]
    fn ime_candidate_keys_do_not_finish_topic_title_editing() {
        for key in [Key::Enter, Key::Tab, Key::Escape] {
            let mut h = harness();
            h.ctx
                .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            let root = h.state().doc.sheet().root;
            let original = h.state().doc.sheet().root_topic().title.clone();
            let children = h.state().doc.sheet().children(root).len();
            {
                let state = h.state_mut();
                state.view.begin_edit(state.doc.sheet(), root, true);
            }
            h.run_steps(4);
            h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
                text: "nihon".into(),
                active_range_chars: None,
            }));
            h.run_steps(2);
            h.key_press(key);
            h.run_steps(2);
            assert_eq!(
                h.state().view.editing_topic(),
                Some(root),
                "{key:?} belongs to IME candidate selection, not the map"
            );
            assert_eq!(h.state().doc.sheet().root_topic().title, original);
            assert_eq!(h.state().doc.sheet().children(root).len(), children);
            h.input_mut().events.extend([
                egui::Event::Ime(egui::ImeEvent::Commit("日本".into())),
                egui::Event::Key {
                    key: Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                },
            ]);
            h.run_steps(2);
            assert_eq!(h.state().view.editing_topic(), Some(root));
            assert_eq!(h.state().view.editing_text(), Some("日本"));
            h.key_press(Key::Enter);
            h.run_steps(3);
            assert_eq!(h.state().view.editing_topic(), None);
            assert_eq!(h.state().doc.sheet().root_topic().title, "日本");
            assert!(h.state_mut().doc.undo());
            assert_eq!(h.state().doc.sheet().root_topic().title, original);
        }
    }

    #[test]
    fn pending_text_is_applied_before_finishing_topic_and_card_titles() {
        for card in [false, true] {
            for key in [Key::Enter, Key::Escape] {
                let mut h = harness();
                h.run_steps(3);
                let topic = if card {
                    h.state_mut()
                        .doc
                        .add_card(
                            "Sticky note",
                            Vec2::new(0.0, 200.0),
                            thinoda_core::Card::new(
                                thinoda_core::CardKind::Sticky,
                                Vec2::new(180.0, 150.0),
                            ),
                        )
                        .unwrap()
                } else {
                    h.state().doc.sheet().root
                };
                let original = h.state().doc.sheet().topic(topic).unwrap().title.clone();
                {
                    let state = h.state_mut();
                    state.view.select_only(topic);
                    state.view.begin_edit(state.doc.sheet(), topic, true);
                }
                h.run_steps(4);
                h.input_mut().events.extend([
                    egui::Event::Text("Fast title".into()),
                    egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers::NONE,
                    },
                ]);
                h.run_steps(3);
                assert_eq!(
                    h.state().doc.sheet().topic(topic).unwrap().title,
                    "Fast title",
                    "card={card}, key={key:?}"
                );
                assert_eq!(h.state().view.editing_topic(), None);
                assert!(h.state_mut().doc.undo());
                assert_eq!(h.state().doc.sheet().topic(topic).unwrap().title, original);
            }
        }
    }

    #[test]
    fn pending_title_text_is_applied_before_tab_adds_a_child() {
        let mut h = harness();
        h.run_steps(3);
        let root = h.state().doc.sheet().root;
        let children = h.state().doc.sheet().children(root).len();
        {
            let state = h.state_mut();
            state.view.begin_edit(state.doc.sheet(), root, true);
        }
        h.run_steps(4);
        h.input_mut().events.extend([
            egui::Event::Text("Fast parent".into()),
            egui::Event::Key {
                key: Key::Tab,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
        ]);
        h.run_steps(3);
        assert_eq!(h.state().doc.sheet().root_topic().title, "Fast parent");
        assert_eq!(h.state().doc.sheet().children(root).len(), children + 1);
    }

    #[test]
    fn failed_same_frame_title_commit_keeps_text_focus_and_does_not_add_a_child() {
        let mut h = harness();
        h.run_steps(3);
        let root = h.state().doc.sheet().root;
        let children = h.state().doc.sheet().children(root).len();
        {
            let state = h.state_mut();
            state.view.begin_edit(state.doc.sheet(), root, true);
        }
        h.run_steps(4);
        h.state_mut()
            .doc
            .edit_sheet("Concurrent edit", |sheet| sheet.set_title(root, "External"))
            .unwrap();
        h.input_mut().events.extend([
            egui::Event::Text("Pending draft".into()),
            egui::Event::Key {
                key: Key::Tab,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
        ]);
        h.run_steps(3);
        assert_eq!(h.state().doc.sheet().root_topic().title, "External");
        assert_eq!(h.state().doc.sheet().children(root).len(), children);
        assert_eq!(h.state().view.editing_topic(), Some(root));
        assert_eq!(h.state().view.editing_text(), Some("Pending draft"));
        assert_eq!(
            h.ctx.memory(|memory| memory.focused()),
            Some(MapCanvas::id().with("edit"))
        );
        assert!(
            h.state_mut()
                .view
                .take_title_error()
                .unwrap()
                .contains("changed")
        );
    }

    #[test]
    fn symbol_background_composes_theme_boundary_selection_and_explicit_fill() {
        use thinoda_core::{Color, theme::Theme};
        for dark in [false, true] {
            let mut h = harness();
            let (child, boundary) = {
                let state = h.state_mut();
                let mut sheet = Sheet::new("Colors", "Root");
                sheet.theme.background = Some(Color::rgba(220, 20, 40, 64));
                let child = sheet.insert_child(sheet.root, "Child").unwrap();
                sheet
                    .toggle_marker_color(&[child], "month-jan", Some(Color::BLACK))
                    .unwrap();
                sheet.topic_mut(child).unwrap().style.fill = Some(Color::rgba(10, 30, 200, 128));
                let boundary = sheet.add_boundary(&[child]).unwrap();
                sheet.boundaries[0].color = Some(Color::rgba(20, 190, 50, 180));
                state.doc = Document::new(Workbook::new(sheet));
                state.view = MapView::default();
                state.view.dark = dark;
                state.view.select_only(child);
                (child, boundary)
            };
            h.run_steps(4);
            for selected in [false, true] {
                if selected {
                    h.state_mut()
                        .view
                        .select_object(ObjectSelection::Boundary(boundary));
                    h.run_steps(3);
                }
                let state = h.state();
                let sheet = state.doc.sheet();
                let base = Theme::for_settings(&sheet.theme).background(&sheet.theme, dark);
                assert_eq!(base.a, 255);
                let overlay = if selected {
                    state.view.accent
                } else {
                    color32(sheet.boundaries[0].color.unwrap())
                }
                .gamma_multiply(0.12)
                .to_srgba_unmultiplied();
                let expected = Color::rgba(10, 30, 200, 128)
                    .over(Color::rgba(overlay[0], overlay[1], overlay[2], overlay[3]).over(base));
                assert_eq!(
                    state.view.symbol_background(&state.doc, child, Some(0)),
                    expected
                );
                assert!(Color::highest_contrast_on(expected).contrast_on(expected) >= 4.5);
            }
        }
    }

    #[test]
    fn symbol_context_hit_targets_one_instance_at_different_zooms() {
        use thinoda_core::Color;
        for zoom in [0.5, 1.0, 2.0] {
            let mut h = harness();
            let root = h.state().doc.sheet().root;
            {
                let state = h.state_mut();
                state
                    .doc
                    .edit_sheet("Symbols", |sheet| {
                        sheet.toggle_marker_color(&[root], "priority-1", Some(Color::BLACK))?;
                        sheet.toggle_marker_color(&[root], "flag-red", Some(Color::WHITE))
                    })
                    .unwrap();
                state.view.set_zoom(zoom);
            }
            h.run_steps(4);
            let state = h.state();
            let point = state
                .view
                .symbol_rect(&state.doc, root, 1)
                .unwrap()
                .center();
            click_at(&mut h, point, PointerButton::Secondary);
            assert_eq!(h.state().view.context_marker, Some((root, 1)));
            let state = h.state_mut();
            apply_action(
                &mut state.doc,
                &mut state.view,
                Action::RemoveMarker(root, 1),
            )
            .unwrap();
            assert_eq!(
                state.doc.sheet().topic(root).unwrap().markers,
                ["priority-1"]
            );
            assert_eq!(
                state.doc.sheet().topic(root).unwrap().marker_colors[&0],
                Color::BLACK
            );
        }
    }

    #[test]
    fn object_outlines_select_with_pointer_and_delete_with_keyboard_at_each_zoom() {
        for kind in 0..3 {
            for zoom in [0.5, 1.0, 2.0] {
                let mut h = harness();
                let object;
                {
                    let state = h.state_mut();
                    let mut book = Workbook::default();
                    let sheet = &mut book.sheets[0];
                    *sheet = Sheet::new("S", "Root");
                    sheet.structure = thinoda_core::Structure::LogicRight;
                    let a = sheet.add_child(sheet.root, None, "A");
                    let b = sheet.add_child(sheet.root, None, "B");
                    object = match kind {
                        0 => ObjectSelection::Relationship(sheet.add_relationship(a, b).unwrap()),
                        1 => ObjectSelection::Boundary(sheet.add_boundary(&[a, b]).unwrap()),
                        _ => {
                            sheet.add_summary(&[a, b], "Conclusion").unwrap();
                            ObjectSelection::Summary(sheet.summaries[0].id)
                        }
                    };
                    state.doc.replace(book);
                    state.view.reset();
                    state.view.show_add_controls = false;
                    state.view.set_initial_zoom(zoom);
                }
                h.run();
                let pos = {
                    let state = h.state_mut();
                    let layout = state.view.layout_for(&state.doc);
                    let p = match object {
                        ObjectSelection::Relationship(_) => {
                            let r = &layout.relationships[0];
                            bezier(r.start, r.c1, r.c2, r.end, 0.5)
                        }
                        ObjectSelection::Boundary(_) => layout.boundaries[0].rect.top_center(),
                        ObjectSelection::Summary(_) => layout.summaries[0].tip,
                        ObjectSelection::Zone(_) => unreachable!(
                            "this fixture creates relationships, boundaries and summaries"
                        ),
                    };
                    assert!(layout.hit_test(p).is_none());
                    assert_eq!(
                        hit_object(state.doc.sheet(), &layout, p, 6.0 / zoom),
                        Some(object)
                    );
                    state.view.xf().pos(p)
                };
                assert!(h.state().view.canvas_rect().contains(pos));
                click_at(&mut h, pos, PointerButton::Primary);
                assert_eq!(
                    h.state().view.selected_object,
                    Some(object),
                    "kind {kind}, zoom {zoom}"
                );
                assert!(h.state().view.selection.is_empty());
                assert!(h.query_by_label("Add child topic").is_none());
                let before = h.state().doc.sheet().clone();
                h.key_press(Key::Delete);
                h.run();
                assert!(!object.exists(h.state().doc.sheet()));
                h.state().doc.sheet().validate().unwrap();
                h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
                h.run();
                assert_eq!(h.state().doc.sheet(), &before);
                click_at(&mut h, pos, PointerButton::Secondary);
                assert_eq!(h.state().view.selected_object, Some(object));
                h.key_press(Key::Escape);
                h.run();
                assert!(h.state().view.selected_object.is_none());
                click_at(&mut h, pos, PointerButton::Primary);
                let topic = h.state().doc.sheet().root;
                h.state_mut().view.select_only(topic);
                assert!(h.state().view.selected_object.is_none());
            }
        }
    }

    #[test]
    fn boundary_hit_test_ignores_interior_and_prefers_nested_outline() {
        let mut sheet = Sheet::new("S", "Root");
        let a = sheet.add_child(sheet.root, None, "A");
        let b = sheet.add_child(sheet.root, None, "B");
        let outer = sheet.add_boundary(&[sheet.root]).unwrap();
        let inner = sheet.add_boundary(&[a, b]).unwrap();
        let mut layout = layout_sheet(&sheet, &ApproxMeasure, false);
        let inner_rect = layout
            .boundaries
            .iter()
            .find(|b| b.id == inner)
            .unwrap()
            .rect;
        layout
            .boundaries
            .iter_mut()
            .find(|b| b.id == outer)
            .unwrap()
            .rect = inner_rect.expand(20.0);
        assert_eq!(
            hit_object(&sheet, &layout, inner_rect.top_center(), 0.1),
            Some(ObjectSelection::Boundary(inner))
        );
        assert_eq!(hit_object(&sheet, &layout, inner_rect.center(), 0.1), None);
        let outer_rect = layout
            .boundaries
            .iter()
            .find(|b| b.id == outer)
            .unwrap()
            .rect;
        assert_eq!(
            hit_object(&sheet, &layout, outer_rect.left_center(), 0.1),
            Some(ObjectSelection::Boundary(outer))
        );
        layout
            .boundaries
            .iter_mut()
            .find(|b| b.id == outer)
            .unwrap()
            .rect = thinoda_core::Rect::from_min_size(inner_rect.min, inner_rect.size * 2.0);
        assert_eq!(
            hit_object(&sheet, &layout, inner_rect.top_center(), 0.1),
            Some(ObjectSelection::Boundary(inner))
        );
    }

    #[test]
    fn decoration_hit_tests_respect_topic_frame_clips() {
        let mut sheet = Sheet::new("S", "Root");
        let root = sheet.root;
        let a = sheet.add_child(root, None, "A");
        let aa = sheet.add_child(a, None, "AA");
        let b = sheet.add_child(root, None, "B");
        let boundary = sheet.add_boundary(&[root]).unwrap();
        let summary_topic = sheet.add_summary(&[a, b], "Conclusion").unwrap();
        let summary_id = sheet.summaries[0].id;
        let same_frame = sheet.add_relationship(a, aa).unwrap();
        let cross_frame = sheet.add_relationship(a, b).unwrap();
        let layout = layout_sheet(&sheet, &ApproxMeasure, false);

        let boundary_point = layout
            .boundaries
            .iter()
            .find(|item| item.id == boundary)
            .unwrap()
            .rect
            .top_center();
        let summary_layout = layout
            .summaries
            .iter()
            .find(|item| item.id == summary_id)
            .unwrap();
        let same_layout = layout
            .relationships
            .iter()
            .find(|item| item.id == same_frame)
            .unwrap();
        let cross_layout = layout
            .relationships
            .iter()
            .find(|item| item.id == cross_frame)
            .unwrap();
        let same_point = same_layout.mid;
        let cross_point = cross_layout.mid;
        let small_frame =
            |point: Vec2| thinoda_core::Rect::from_center_size(point, Vec2::new(2.0, 2.0));
        sheet.zones = vec![
            thinoda_core::Zone {
                id: thinoda_core::ZoneId(1),
                name: "Root clip".into(),
                frame: small_frame(boundary_point),
                roots: vec![root],
                objects: vec![],
                keep_empty: false,
                color: None,
                background_color: None,
                clip: true,
            },
            thinoda_core::Zone {
                id: thinoda_core::ZoneId(2),
                name: "A clip".into(),
                frame: small_frame(same_point),
                roots: vec![a],
                objects: vec![],
                keep_empty: false,
                color: None,
                background_color: None,
                clip: true,
            },
            thinoda_core::Zone {
                id: thinoda_core::ZoneId(3),
                name: "B clip".into(),
                frame: small_frame(cross_point + Vec2::new(200.0, 200.0)),
                roots: vec![b],
                objects: vec![],
                keep_empty: false,
                color: None,
                background_color: None,
                clip: true,
            },
            thinoda_core::Zone {
                id: thinoda_core::ZoneId(4),
                name: "Summary clip".into(),
                frame: small_frame(summary_layout.tip),
                roots: vec![summary_topic],
                objects: vec![],
                keep_empty: false,
                color: None,
                background_color: None,
                clip: true,
            },
        ];

        assert_eq!(
            hit_object(&sheet, &layout, boundary_point, 0.1),
            Some(ObjectSelection::Boundary(boundary))
        );
        let boundary_outside = layout
            .boundaries
            .iter()
            .find(|item| item.id == boundary)
            .unwrap()
            .rect
            .left_center();
        assert!(
            !topic_clip_rect(&sheet, root)
                .unwrap()
                .contains(boundary_outside)
        );
        assert_eq!(hit_object(&sheet, &layout, boundary_outside, 0.1), None);

        assert_eq!(
            hit_object(&sheet, &layout, summary_layout.tip, 0.1),
            Some(ObjectSelection::Summary(summary_id))
        );
        assert!(
            !topic_clip_rect(&sheet, summary_topic)
                .unwrap()
                .contains(summary_layout.start)
        );
        assert_eq!(hit_object(&sheet, &layout, summary_layout.start, 0.1), None);

        assert_eq!(
            hit_relationship(&sheet, &layout, same_point, 1.0),
            Some(same_frame)
        );
        let same_outside = bezier(
            same_layout.start,
            same_layout.c1,
            same_layout.c2,
            same_layout.end,
            0.1,
        );
        assert!(!topic_clip_rect(&sheet, a).unwrap().contains(same_outside));
        assert_eq!(hit_relationship(&sheet, &layout, same_outside, 0.1), None);

        assert_ne!(topic_clip_rect(&sheet, a), topic_clip_rect(&sheet, b));
        assert_eq!(
            hit_relationship(&sheet, &layout, cross_point, 1.0),
            Some(cross_frame)
        );
    }

    #[test]
    fn canvas_is_accessible() {
        let h = harness();
        h.get_by_label("Mind map canvas");
    }

    #[test]
    fn hidden_add_controls_are_absent_but_keyboard_insert_still_works() {
        let mut h = harness();
        let first = h.state().doc.sheet().children(h.state().doc.sheet().root)[0];
        h.state_mut().view.select_only(first);
        h.run();
        h.get_by_label("Add child topic");
        h.get_by_label("Add sibling topic");
        h.state_mut().view.show_add_controls = false;
        h.run();
        assert!(h.query_by_label("Add child topic").is_none());
        assert!(h.query_by_label("Add sibling topic").is_none());
        {
            let s = h.state_mut();
            let layout = s.view.layout_for(&s.doc);
            assert!(add_handles(&s.view, s.doc.sheet(), &layout).is_empty());
            assert!(topic_add_handles(&s.view, s.doc.sheet(), &layout, first).is_empty());
        }
        h.key_press(Key::Tab);
        h.run();
        let inserted = h.state().view.primary().unwrap();
        assert_eq!(h.state().doc.sheet().parent(inserted), Some(first));
    }

    #[test]
    fn initial_zoom_preference_survives_first_frame_and_reset() {
        for requested in [2.5, MAX_ZOOM * 2.0, 1.0] {
            let mut doc = Document::new(Workbook::default());
            doc.edit_sheet("Wide map", |sheet| {
                for i in 0..30 {
                    sheet.insert_child(sheet.root, &format!("Branch {i}"))?;
                }
                Ok(())
            })
            .unwrap();
            let mut view = MapView::default();
            view.set_initial_zoom(requested);
            let mut h = Harness::builder()
                .with_size(vec2(900.0, 600.0))
                .build_ui_state(
                    |ui, s: &mut State| {
                        MapCanvas::show(ui, &mut s.doc, &mut s.view);
                    },
                    State { doc, view },
                );
            h.run();
            let s = h.state();
            if requested == 1.0 {
                assert!(s.view.zoom < 1.0);
            } else {
                assert_eq!(s.view.zoom, requested.clamp(MIN_ZOOM, MAX_ZOOM));
            }
            let center = s.view.cache.as_ref().unwrap().layout.bounds.center();
            assert!((s.view.offset - vec2(-center.x, -center.y) * s.view.zoom).length() < 0.01);
            {
                let s = h.state_mut();
                s.view.request_fit();
            }
            h.run();
            assert!(h.state().view.zoom < 1.0);
            h.state_mut().view.reset();
            h.state_mut().view.set_initial_zoom(2.5);
            h.run();
            assert_eq!(h.state().view.zoom, 2.5);
        }
    }

    #[test]
    fn board_initial_and_explicit_fit_keep_kanban_clear_of_compact_toolbox() {
        for size in [vec2(900.0, 600.0), vec2(1200.0, 900.0)] {
            let doc = Document::new(thinoda_core::templates::instantiate("kanban-board"));
            let original = doc.workbook().clone();
            let original_state = doc.state_id();
            let mut h = Harness::builder().with_size(size).build_ui_state(
                |ui, state: &mut State| {
                    MapCanvas::show(ui, &mut state.doc, &mut state.view);
                },
                State {
                    doc,
                    view: MapView::default(),
                },
            );
            h.run();

            for explicit_fit in [false, true] {
                if explicit_fit {
                    let state = h.state_mut();
                    state.view.set_zoom(2.0);
                    state.view.request_fit();
                    h.run();
                }
                let state = h.state();
                let sheet = state.doc.sheet();
                assert!(sheet.uses_board_semantics());
                let frame = sheet
                    .zones
                    .iter()
                    .min_by(|a, b| a.frame.min.x.total_cmp(&b.frame.min.x))
                    .unwrap();
                let frame_rect = state.view.xf().rect(frame.frame);
                let canvas = state.view.canvas_rect;
                let safe_left = canvas.left() + BOARD_TOOLBOX_SAFE_INSET;
                assert!(
                    frame_rect.left() >= safe_left - 0.1,
                    "frame title overlaps compact toolbox: {frame_rect:?}, {canvas:?}"
                );
                let layout = &state.view.cache.as_ref().unwrap().layout;
                let bounds = state.view.xf().rect(fit_bounds(sheet, layout));
                assert!(bounds.left() >= safe_left - 0.1, "{bounds:?}");
                assert!(bounds.right() <= canvas.right() + 0.1, "{bounds:?}");
                assert_eq!(state.doc.workbook(), &original);
                assert_eq!(state.doc.state_id(), original_state);
            }
        }
    }

    #[test]
    fn board_initial_explicit_zoom_centers_in_safe_viewport_without_refitting() {
        let doc = Document::new(thinoda_core::templates::instantiate("kanban-board"));
        let original = doc.workbook().clone();
        let mut view = MapView::default();
        view.set_initial_zoom(2.5);
        let mut h = Harness::builder()
            .with_size(vec2(900.0, 600.0))
            .build_ui_state(
                |ui, state: &mut State| {
                    MapCanvas::show(ui, &mut state.doc, &mut state.view);
                },
                State { doc, view },
            );
        h.run();
        let state = h.state();
        let layout = &state.view.cache.as_ref().unwrap().layout;
        let bounds = fit_bounds(state.doc.sheet(), layout);
        let screen_center = state.view.xf().pos(bounds.center());
        assert_eq!(state.view.zoom, 2.5);
        assert!(screen_center.distance(state.view.fit_viewport(state.doc.sheet()).center()) < 0.01);
        assert_eq!(state.doc.workbook(), &original);
    }

    #[test]
    fn plain_map_initial_and_explicit_fit_keep_original_camera_math() {
        for wide in [false, true] {
            let mut doc = Document::new(Workbook::default());
            if wide {
                doc.edit_sheet("Wide map", |sheet| {
                    for i in 0..30 {
                        sheet.insert_child(sheet.root, &format!("Branch {i}"))?;
                    }
                    Ok(())
                })
                .unwrap();
            }
            assert!(!doc.sheet().uses_board_semantics());
            let original = doc.workbook().clone();
            let mut h = Harness::builder()
                .with_size(vec2(900.0, 600.0))
                .build_ui_state(
                    |ui, state: &mut State| {
                        MapCanvas::show(ui, &mut state.doc, &mut state.view);
                    },
                    State {
                        doc,
                        view: MapView::default(),
                    },
                );
            h.run();
            for explicit_fit in [false, true] {
                if explicit_fit {
                    h.state_mut().view.request_fit();
                    h.run();
                }
                let state = h.state();
                let rect = state.view.canvas_rect;
                let layout = &state.view.cache.as_ref().unwrap().layout;
                let bounds = fit_bounds(state.doc.sheet(), layout);
                let overflow = bounds.size.x > rect.width() || bounds.size.y > rect.height();
                let expected_zoom = if explicit_fit || overflow {
                    ((rect.width() - 80.0) / bounds.size.x)
                        .min((rect.height() - 80.0) / bounds.size.y)
                        .clamp(MIN_ZOOM, 1.0)
                } else {
                    1.0
                };
                let center = bounds.center();
                let expected_offset = vec2(-center.x * expected_zoom, -center.y * expected_zoom);
                assert!((state.view.zoom - expected_zoom).abs() < 0.0001);
                assert!((state.view.offset - expected_offset).length() < 0.01);
                assert_eq!(state.doc.workbook(), &original);
            }
        }
    }

    #[test]
    fn summary_topic_edit_child_and_undo_are_independent_of_source_topics() {
        let mut h = harness();
        let root = h.state().doc.sheet().root;
        let sources = h.state().doc.sheet().children(root)[..2].to_vec();
        let original_titles: Vec<_> = sources
            .iter()
            .map(|id| h.state().doc.sheet().topic(*id).unwrap().title.clone())
            .collect();
        {
            let s = h.state_mut();
            s.view.selection = sources.clone();
            apply_action(&mut s.doc, &mut s.view, Action::AddSummary).unwrap();
        }
        h.run();
        let summary = h.state().view.primary().unwrap();
        assert_eq!(h.state().view.editing_topic(), Some(summary));
        h.get_by_label("Topic title")
            .type_text("Independent conclusion");
        h.run();
        h.key_press(Key::Enter);
        h.run();
        assert_eq!(
            h.state().doc.sheet().topic(summary).unwrap().title,
            "Independent conclusion"
        );
        assert!(h.query_by_label("Add sibling topic").is_none());
        h.get_by_label("Add child topic").click();
        h.run();
        let child = h.state().view.primary().unwrap();
        assert_eq!(h.state().doc.sheet().parent(child), Some(summary));
        h.key_press(Key::Escape);
        h.run();
        {
            let s = h.state_mut();
            assert!(s.doc.undo());
            assert!(s.doc.sheet().topic(child).is_none());
            assert!(s.doc.undo());
            assert_eq!(s.doc.sheet().topic(summary).unwrap().title, "Summary");
            assert!(s.doc.redo());
            assert_eq!(
                s.doc.sheet().topic(summary).unwrap().title,
                "Independent conclusion"
            );
            assert!(s.doc.redo());
            assert_eq!(s.doc.sheet().parent(child), Some(summary));
            s.doc.sheet().validate().unwrap();
            for (id, title) in sources.iter().zip(&original_titles) {
                assert_eq!(&s.doc.sheet().topic(*id).unwrap().title, title);
            }
            assert_eq!(s.doc.sheet().summaries.len(), 1);
            assert!(s.doc.sheet().boundaries.is_empty());
            assert!(s.doc.undo()); // child insertion
            assert!(s.doc.undo()); // title edit
            assert!(s.doc.undo()); // summary creation
            assert!(s.doc.sheet().summaries.is_empty());
            assert!(s.doc.sheet().topic(summary).is_none());
            assert_eq!(s.doc.sheet().children(root)[..2], sources);
            s.doc.sheet().validate().unwrap();
        }
    }

    fn click_at(h: &mut Harness<'_, State>, pos: Pos2, button: PointerButton) {
        h.hover_at(pos);
        h.run();
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos,
                button,
                pressed,
                modifiers: Modifiers::NONE,
            });
            h.run();
        }
    }

    #[test]
    fn child_handle_pointer_click_is_localized_and_undoable() {
        for zoom in [0.25, 1.0, 2.0] {
            let mut h = harness();
            let root = h.state().doc.sheet().root;
            let n = h.state().doc.sheet().topics.len();
            {
                let s = h.state_mut();
                s.view.set_zoom(zoom);
                s.view.add_control_labels.child = "Unterthema hinzufügen".into();
                s.view.titles.main = "Neues Thema".into();
            }
            h.run();
            assert!(h.query_by_label("Add sibling topic").is_none());
            let handle = h.get_by_label("Unterthema hinzufügen").rect();
            assert_eq!(handle.size(), vec2(28.0, 28.0));
            click_at(&mut h, handle.center(), PointerButton::Primary);
            let s = h.state();
            let inserted = s.view.primary().unwrap();
            assert_ne!(inserted, root);
            assert_eq!(s.view.selection, vec![inserted]);
            assert_eq!(s.doc.sheet().parent(inserted), Some(root));
            assert_eq!(s.doc.sheet().topics.len(), n + 1);
            assert_eq!(s.doc.sheet().topic(inserted).unwrap().title, "Neues Thema");
            assert_eq!(s.view.editing_topic(), Some(inserted));
            assert!(h.query_by_label("Unterthema hinzufügen").is_none());
            h.key_press(Key::Escape);
            h.run();
            h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
            h.run();
            assert!(h.state().doc.sheet().topic(inserted).is_none());
            assert_eq!(h.state().doc.sheet().topics.len(), n);
        }
    }

    #[test]
    fn sibling_handle_pointer_click_preserves_parent_and_order() {
        let mut h = harness();
        let root = h.state().doc.sheet().root;
        let first = h.state().doc.sheet().children(root)[0];
        h.state_mut().view.select_only(first);
        h.state_mut().view.titles.main = "Sibling".into();
        h.run();
        h.get_by_label("Add sibling topic").click();
        h.run();
        let s = h.state();
        let new = s.view.primary().unwrap();
        assert_eq!(s.doc.sheet().children(root)[1], new);
        assert_eq!(s.doc.sheet().parent(new), Some(root));
        assert_eq!(s.doc.sheet().topic(new).unwrap().title, "Sibling");
        assert_eq!(s.view.editing_topic(), Some(new));
        assert_eq!(s.doc.sheet().floating.len(), 0);
    }

    #[test]
    fn edge_selection_reserves_child_handle_space_after_insert_and_rename() {
        for structure in [
            thinoda_core::Structure::LogicRight,
            thinoda_core::Structure::LogicLeft,
        ] {
            for zoom in [0.25, 1.0, 2.0] {
                let mut h = harness();
                {
                    let s = h.state_mut();
                    s.doc
                        .edit_sheet("Structure", |sheet| {
                            sheet.set_structure(None, Some(structure))
                        })
                        .unwrap();
                }
                h.run();
                let first = h.state().doc.sheet().children(h.state().doc.sheet().root)[0];
                {
                    let s = h.state_mut();
                    s.view.set_zoom(zoom);
                    let layout = s.view.layout_for(&s.doc);
                    let topic = s.view.xf().rect(layout.rect(first).unwrap());
                    let canvas = s.view.canvas_rect();
                    let dx = if structure == thinoda_core::Structure::LogicRight {
                        canvas.right() - 5.0 - topic.right()
                    } else {
                        canvas.left() + 5.0 - topic.left()
                    };
                    s.view.offset += vec2(dx, canvas.center().y - topic.center().y);
                    s.view.select_only(first);
                    // Without revealing, the outward target would be clipped.
                    assert!(!canvas.contains_rect(
                        topic_add_handles(&s.view, s.doc.sheet(), &layout, first)[0].rect()
                    ));
                }
                h.run();
                let child = h.get_by_label("Add child topic").rect();
                let sibling = h.get_by_label("Add sibling topic").rect();
                {
                    let s = h.state();
                    let topic = s.view.selected_topic_rect(&s.doc).unwrap();
                    assert!(s.view.canvas_rect().contains_rect(child));
                    assert!(s.view.canvas_rect().contains_rect(sibling));
                    assert!(!topic.intersects(child));
                    assert!(!topic.intersects(sibling));
                }
                click_at(&mut h, child.center(), PointerButton::Primary);
                let inserted = h.state().view.primary().unwrap();
                assert_eq!(h.state().doc.sheet().parent(inserted), Some(first));
                h.get_by_label("Topic title")
                    .type_text("A longer renamed child");
                h.run();
                h.key_press(Key::Enter);
                h.run();
                assert_eq!(
                    h.state().doc.sheet().topic(inserted).unwrap().title,
                    "A longer renamed child"
                );
                let next_child = h.get_by_label("Add child topic").rect();
                let s = h.state();
                assert!(s.view.canvas_rect().contains_rect(next_child));
                assert!(
                    !s.view
                        .selected_topic_rect(&s.doc)
                        .unwrap()
                        .intersects(next_child)
                );
                click_at(&mut h, next_child.center(), PointerButton::Primary);
                let s = h.state();
                assert_eq!(
                    s.doc.sheet().parent(s.view.primary().unwrap()),
                    Some(inserted)
                );
                assert!(s.doc.sheet().floating.is_empty());
            }
        }
    }

    #[test]
    fn handle_reveal_does_not_snap_back_after_deliberate_panning() {
        let mut h = harness();
        let first = h.state().doc.sheet().children(h.state().doc.sheet().root)[0];
        h.state_mut().view.select_only(first);
        h.run();
        h.state_mut().view.offset += vec2(900.0, 0.0);
        let offset = h.state().view.offset;
        h.run();
        assert_eq!(h.state().view.offset, offset);
    }

    #[test]
    fn double_clicking_handle_never_creates_a_floating_topic() {
        let mut h = harness();
        let n = h.state().doc.sheet().topics.len();
        let center = h.get_by_label("Add child topic").rect().center();
        click_at(&mut h, center, PointerButton::Primary);
        click_at(&mut h, center, PointerButton::Primary);
        assert_eq!(h.state().doc.sheet().topics.len(), n + 1);
        assert!(h.state().doc.sheet().floating.is_empty());
    }

    #[test]
    fn dragging_from_handle_does_not_start_topic_or_rubber_drag() {
        let mut h = harness();
        let selected = h.state().view.selection.clone();
        let n = h.state().doc.sheet().topics.len();
        let center = h.get_by_label("Add child topic").rect().center();
        h.hover_at(center);
        h.run();
        h.event(egui::Event::PointerButton {
            pos: center,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        });
        h.run();
        h.hover_at(center + vec2(50.0, 50.0));
        h.run();
        assert!(matches!(h.state().view.drag, Drag::None));
        h.event(egui::Event::PointerButton {
            pos: center + vec2(50.0, 50.0),
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        });
        h.run();
        assert_eq!(h.state().view.selection, selected);
        assert_eq!(h.state().doc.sheet().topics.len(), n);
    }

    #[test]
    fn focused_handle_supports_keyboard_activation() {
        for key in [Key::Enter, Key::Space] {
            let mut h = harness();
            let root = h.state().doc.sheet().root;
            let n = h.state().doc.sheet().children(root).len();
            h.get_by_label("Add child topic").focus();
            h.run();
            h.key_press(key);
            h.run();
            assert_eq!(h.state().doc.sheet().children(root).len(), n + 1);
            assert_eq!(h.state().view.editing_topic(), h.state().view.primary());
        }
    }

    #[test]
    fn child_handle_unfolds_branch_in_one_undo_transaction() {
        let mut h = harness();
        let first = h.state().doc.sheet().children(h.state().doc.sheet().root)[0];
        {
            let s = h.state_mut();
            s.doc
                .edit_sheet("Branch", |sheet| {
                    sheet.insert_child(first, "Existing child")?;
                    sheet.set_collapsed(first, true)
                })
                .unwrap();
            s.view.select_only(first);
        }
        h.run();
        h.get_by_label("Add child topic").click();
        h.run();
        let new = h.state().view.primary().unwrap();
        assert!(!h.state().doc.sheet().topic(first).unwrap().collapsed);
        assert_eq!(h.state().doc.sheet().parent(new), Some(first));
        h.key_press(Key::Escape);
        h.run();
        h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
        h.run();
        assert!(h.state().doc.sheet().topic(first).unwrap().collapsed);
        assert!(h.state().doc.sheet().topic(new).is_none());
    }

    #[test]
    fn handles_follow_layout_direction_and_hide_in_conflicting_modes() {
        let mut h = harness();
        let first = h.state().doc.sheet().children(h.state().doc.sheet().root)[0];
        h.state_mut().view.select_only(first);
        h.run();
        for dir in [Dir::Right, Dir::Left, Dir::Down, Dir::Up] {
            let s = h.state_mut();
            let mut layout = (*s.view.layout_for(&s.doc)).clone();
            layout.topics.get_mut(&first).unwrap().grow = dir;
            let handles = add_handles(&s.view, s.doc.sheet(), &layout);
            assert_eq!(handles.len(), 2);
            let child = &handles[0];
            let d = child.center - child.anchor;
            match dir {
                Dir::Right => assert!(d.x > 0.0 && d.y == 0.0),
                Dir::Left => assert!(d.x < 0.0 && d.y == 0.0),
                Dir::Down => assert!(d.y > 0.0 && d.x == 0.0),
                Dir::Up => assert!(d.y < 0.0 && d.x == 0.0),
            }
        }
        h.state_mut().view.relationship_from = Some(first);
        h.run();
        assert!(h.query_by_label("Add child topic").is_none());
        h.state_mut().view.relationship_from = None;
        let root = h.state().doc.sheet().root;
        h.state_mut().view.selection.push(root);
        h.run();
        assert!(h.query_by_label("Add child topic").is_none());
        h.state_mut().view.select_only(first);
        {
            let s = h.state_mut();
            s.view.begin_edit(s.doc.sheet(), first, true);
        }
        h.run();
        assert!(h.query_by_label("Add child topic").is_none());
    }

    #[test]
    fn topic_group_bounds_follow_visible_descendants_and_document_changes() {
        let mut h = harness();
        let s = h.state_mut();
        let (floating, child) = s
            .doc
            .edit_sheet("Floating group", |sheet| {
                let root = sheet.add_floating("Floating", Vec2::new(900.0, 400.0));
                let child = sheet.insert_child(root, "Child")?;
                Ok((root, child))
            })
            .unwrap();
        let layout = s.view.layout_for(&s.doc);
        let expected = layout
            .rect(floating)
            .unwrap()
            .union(&layout.rect(child).unwrap());
        assert_eq!(
            s.view.topic_group_bounds(&s.doc, &[floating, child]),
            Some(expected)
        );
        s.doc
            .edit_sheet("Collapse", |sheet| sheet.set_collapsed(floating, true))
            .unwrap();
        let layout = s.view.layout_for(&s.doc);
        assert_eq!(
            s.view.topic_group_bounds(&s.doc, &[floating]),
            layout.rect(floating)
        );
        assert_eq!(s.view.topic_group_bounds(&s.doc, &[]), None);
    }

    #[test]
    fn selected_geometry_uses_camera_and_rejects_stale_or_dragged_layout() {
        let mut h = harness();
        let s = h.state_mut();
        let before = s.view.selected_topic_rect(&s.doc).unwrap();
        assert!(s.view.canvas_rect().contains(before.center()));
        s.view.offset += vec2(25.0, -10.0);
        assert_eq!(
            s.view.selected_topic_rect(&s.doc).unwrap(),
            before.translate(vec2(25.0, -10.0))
        );
        s.view
            .zoom_by(2.0, Some(before.center() + vec2(25.0, -10.0)));
        let zoomed = s.view.selected_topic_rect(&s.doc).unwrap();
        assert!((zoomed.width() - before.width() * 2.0).abs() < 0.01);
        s.view.drag = Drag::Pan;
        assert!(s.view.selected_topic_rect(&s.doc).is_none());
        s.view.drag = Drag::None;
        let root = s.doc.sheet().root;
        s.doc
            .edit_sheet("Rename", |sheet| sheet.set_title(root, "Changed"))
            .unwrap();
        assert!(s.view.selected_topic_rect(&s.doc).is_none());
    }

    #[test]
    fn blank_secondary_click_clears_stale_selection() {
        let mut h = harness();
        let blank = h.state().view.canvas_rect().left_top() + vec2(10.0, 10.0);
        click_at(&mut h, blank, PointerButton::Secondary);
        assert!(h.state().view.selection.is_empty());
        assert!(h.state().view.selected_object.is_none());
    }

    #[test]
    fn topic_secondary_click_preserves_existing_multiselection() {
        let mut h = harness();
        let root = h.state().doc.sheet().root;
        let first = h.state().doc.sheet().children(root)[0];
        h.state_mut().view.selection = vec![first, root];
        h.run();
        let pos = {
            let s = h.state();
            s.view
                .xf()
                .rect(s.view.cache.as_ref().unwrap().layout.rect(first).unwrap())
                .center()
        };
        click_at(&mut h, pos, PointerButton::Secondary);
        assert_eq!(h.state().view.selection, vec![first, root]);
    }

    #[test]
    fn tab_creates_child_and_typing_edits() {
        let mut h = harness();
        let root = h.state().doc.sheet().root;
        let n = h.state().doc.sheet().children(root).len();
        h.key_press(Key::Tab);
        h.run();
        assert_eq!(h.state().doc.sheet().children(root).len(), n + 1);
        let new = h.state().view.selection[0];
        assert_eq!(h.state().view.editing_topic(), Some(new));
        h.get_by_label("Topic title").type_text("Ideas");
        h.run();
        h.key_press(Key::Enter);
        h.run();
        assert_eq!(h.state().doc.sheet().topic(new).unwrap().title, "Ideas");
        assert!(h.state().view.editing_topic().is_none());
        // Enter now adds a sibling.
        h.key_press(Key::Enter);
        h.run();
        assert_eq!(h.state().doc.sheet().children(root).len(), n + 2);
    }

    #[test]
    fn undo_via_keyboard() {
        let mut h = harness();
        let root = h.state().doc.sheet().root;
        let first = h.state().doc.sheet().children(root)[0];
        h.state_mut().view.select_only(first);
        h.key_press(Key::Delete);
        h.run();
        assert!(h.state().doc.sheet().topic(first).is_none());
        h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
        h.run();
        assert!(h.state().doc.sheet().topic(first).is_some());
    }

    #[test]
    fn contextual_aliases_dispatch_without_changing_edit_and_undo_semantics() {
        for insert in [Key::Tab, Key::Insert] {
            for delete in [Key::Delete, Key::Backspace] {
                for redo in [
                    egui::KeyboardShortcut::new(Modifiers::COMMAND, Key::Y),
                    egui::KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z),
                ] {
                    let mut h = harness();
                    let root = h.state().doc.sheet().root;
                    h.state_mut().view.select_only(root);
                    let before = h.state().doc.workbook().clone();
                    h.key_press(insert);
                    h.run();
                    let child = h.state().view.primary().unwrap();
                    assert_ne!(child, root);
                    assert_eq!(h.state().view.editing_topic(), Some(child));
                    h.key_press(Key::Escape);
                    h.run();
                    assert_eq!(h.state().view.editing_topic(), None);
                    let inserted = h.state().doc.workbook().clone();
                    assert_ne!(inserted, before);
                    h.key_press(delete);
                    h.run();
                    assert!(!h.state().doc.sheet().contains(child));
                    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
                    h.run();
                    assert_eq!(h.state().doc.workbook(), &inserted);
                    h.key_press_modifiers(redo.modifiers, redo.logical_key);
                    h.run();
                    assert!(!h.state().doc.sheet().contains(child));
                    for home in [
                        egui::KeyboardShortcut::new(Modifiers::NONE, Key::Home),
                        egui::KeyboardShortcut::new(Modifiers::COMMAND, Key::R),
                    ] {
                        h.state_mut().view.selection.clear();
                        h.key_press_modifiers(home.modifiers, home.logical_key);
                        h.run();
                        assert_eq!(h.state().view.selection, vec![root]);
                    }
                }
            }
        }
    }

    #[test]
    fn xmind_guide_shortcuts() {
        let mut h = harness();
        let root = h.state().doc.sheet().root;
        let first = h.state().doc.sheet().children(root)[0];
        // Ctrl+Enter inserts a parent; Ctrl+Delete removes only that topic again.
        h.state_mut().view.select_only(first);
        h.key_press_modifiers(Modifiers::COMMAND, Key::Enter);
        h.run();
        let parent = h.state().doc.sheet().parent(first).unwrap();
        assert_ne!(parent, root);
        h.state_mut().view.cancel_edit();
        h.state_mut().view.select_only(parent);
        h.key_press_modifiers(Modifiers::COMMAND, Key::Delete);
        h.run();
        assert!(h.state().doc.sheet().topic(parent).is_none());
        assert_eq!(h.state().doc.sheet().parent(first), Some(root));
        // Ctrl+R jumps to the central topic.
        h.key_press_modifiers(Modifiers::COMMAND, Key::R);
        h.run();
        assert_eq!(h.state().view.selection, vec![root]);
        // Ctrl+Alt+/ folds all branches, pressing again unfolds them.
        {
            let s = h.state_mut();
            s.view.select_only(first);
            crate::apply_action(&mut s.doc, &mut s.view, Action::InsertChild).unwrap();
            s.view.commit_edit(&mut s.doc);
        }
        h.key_press_modifiers(Modifiers::COMMAND | Modifiers::ALT, Key::Slash);
        h.run();
        {
            let s = h.state().doc.sheet();
            assert!(
                s.all_ids_ordered()
                    .iter()
                    .any(|id| s.topic(*id).unwrap().collapsed)
            );
        }
        h.key_press_modifiers(Modifiers::COMMAND | Modifiers::ALT, Key::Slash);
        h.run();
        let s = h.state().doc.sheet();
        assert!(
            s.all_ids_ordered()
                .iter()
                .all(|id| !s.topic(*id).unwrap().collapsed)
        );
    }

    #[test]
    fn zoom_keeps_anchor() {
        let mut v = MapView {
            canvas_rect: Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0)),
            ..Default::default()
        };
        let anchor = pos2(100.0, 100.0);
        let before = v.xf().sheet(anchor);
        v.zoom_by(2.0, Some(anchor));
        let after = v.xf().sheet(anchor);
        assert!((before.x - after.x).abs() < 1e-3 && (before.y - after.y).abs() < 1e-3);
        assert_eq!(v.zoom_percent(), 200);
    }

    #[test]
    fn relationship_hit() {
        let doc = Document::new(Workbook::default());
        let mut doc = doc;
        let root = doc.sheet().root;
        let kids = doc.sheet().children(root).to_vec();
        doc.edit_sheet("rel", |s| s.add_relationship(kids[0], kids[1]))
            .unwrap();
        let layout = layout_sheet(doc.sheet(), &ApproxMeasure, false);
        let r = &layout.relationships[0];
        assert_eq!(
            hit_relationship(doc.sheet(), &layout, r.mid, 6.0),
            Some(r.id)
        );
        assert_eq!(
            hit_relationship(doc.sheet(), &layout, Vec2::new(1e5, 1e5), 6.0),
            None
        );
    }
}
