//! Interactive mind map canvas widget.

use std::sync::Arc;

use egui::{
    Color32, CornerRadius, EventFilter, FontId, Id, Key, Modifiers, PointerButton, Pos2, Rect,
    Response, Sense, Stroke, StrokeKind, TextEdit, Ui, Vec2 as EVec2, pos2, vec2,
};
use ymind_core::{
    ApproxMeasure, BoundaryId, Document, Layout, RelationshipId, Sheet, SummaryId, TopicId,
    clipboard::ClipboardPayload,
    geometry::Vec2,
    layout::{Dir, Icon, cubic as bezier},
    layout_sheet,
    navigation::Direction,
};

use crate::{
    actions::{Action, apply_action},
    measure::EguiMeasure,
    render::{PaintOptions, Xf, brace_path, color32, paint_map, sheet_background},
};

pub const MIN_ZOOM: f32 = 0.1;
pub const MAX_ZOOM: f32 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectSelection {
    Relationship(RelationshipId),
    Boundary(BoundaryId),
    Summary(SummaryId),
}

impl ObjectSelection {
    pub fn exists(self, sheet: &Sheet) -> bool {
        match self {
            Self::Relationship(id) => sheet.relationships.iter().any(|r| r.id == id),
            Self::Boundary(id) => sheet.boundaries.iter().any(|b| b.id == id),
            Self::Summary(id) => sheet.summaries.iter().any(|s| s.id == id),
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
}

#[derive(Clone)]
struct LayoutCache {
    key: (u64, usize, bool, u64),
    layout: Arc<Layout>,
}

/// View state of a canvas (camera, selection, editing). Not part of the document.
pub struct MapView {
    pub offset: EVec2,
    pub zoom: f32,
    pub selection: Vec<TopicId>,
    pub selected_object: Option<ObjectSelection>,
    /// Source topic while the user is picking the target of a new relationship.
    pub relationship_from: Option<TopicId>,
    pub relationship_handle_labels: [String; 4],
    pub(crate) relationship_drag: Option<crate::relationship_editing::RelationshipDrag>,
    pub titles: NewTopicTitles,
    pub add_control_labels: AddControlLabels,
    /// Whether the canvas exposes connector add buttons (keyboard actions remain available).
    pub show_add_controls: bool,
    /// Internal clipboard (keeps full fidelity when pasting inside yMind).
    pub clipboard: Option<ClipboardPayload>,
    /// Text the host should place on the OS clipboard.
    pub clipboard_out: Option<String>,
    /// Topics shown un-dimmed (search/filter); `None` shows everything.
    pub filter: Option<Vec<TopicId>>,
    pub dark: bool,
    pub accent: Color32,
    editing: Option<EditState>,
    drag: Drag,
    cache: Option<LayoutCache>,
    pending_fit: bool,
    pending_reveal: Option<TopicId>,
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
            selected_object: None,
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
            show_add_controls: true,
            clipboard: None,
            clipboard_out: None,
            filter: None,
            dark: false,
            accent: Color32::from_rgb(0x2E, 0x7C, 0xF6),
            editing: None,
            drag: Drag::None,
            cache: None,
            pending_fit: false,
            pending_reveal: None,
            initialized: false,
            explicit_initial_zoom: false,
            canvas_rect: Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0)),
        }
    }
}

impl MapView {
    pub fn select_only(&mut self, id: TopicId) {
        if self.selection != [id] {
            self.pending_reveal = Some(id);
        }
        self.selection = vec![id];
        self.selected_object = None;
    }

    pub fn select_object(&mut self, object: ObjectSelection) {
        self.selection.clear();
        self.selected_object = Some(object);
        self.relationship_from = None;
        self.pending_reveal = None;
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

    /// Screen-space canvas bounds from the latest frame.
    pub fn canvas_rect(&self) -> Rect {
        self.canvas_rect
    }

    /// Removes deleted/hidden topics from the selection.
    pub fn prune_selection(&mut self, sheet: &Sheet) {
        self.selection
            .retain(|id| sheet.topic(*id).is_some() && sheet.is_visible(*id));
        if self
            .selected_object
            .is_some_and(|object| !object.exists(sheet))
        {
            self.selected_object = None;
        }
        if let Some(e) = &self.editing
            && sheet.topic(e.topic).is_none()
        {
            self.editing = None;
        }
    }

    /// Resets per-sheet state (call when switching sheets or documents).
    pub fn reset(&mut self) {
        self.selection.clear();
        self.selected_object = None;
        self.relationship_from = None;
        self.relationship_drag = None;
        self.editing = None;
        self.drag = Drag::None;
        self.cache = None;
        self.initialized = false;
        self.explicit_initial_zoom = false;
        self.zoom = 1.0;
        self.offset = EVec2::ZERO;
    }

    // ----- editing -------------------------------------------------------------------------

    pub fn begin_edit(&mut self, sheet: &Sheet, id: TopicId, select_all: bool) {
        let Some(t) = sheet.topic(id) else { return };
        self.editing = Some(EditState {
            topic: id,
            text: t.title.clone(),
            original: t.title.clone(),
            select_all,
            focus_requested: false,
        });
        self.pending_reveal = Some(id);
    }

    /// Starts editing with `text` replacing the title (type-to-edit).
    pub fn begin_edit_with(&mut self, sheet: &Sheet, id: TopicId, text: &str) {
        self.begin_edit(sheet, id, false);
        if let Some(e) = &mut self.editing {
            e.text = text.to_string();
        }
    }

    pub fn editing_topic(&self) -> Option<TopicId> {
        self.editing.as_ref().map(|e| e.topic)
    }

    pub fn editing_text(&self) -> Option<&str> {
        self.editing.as_ref().map(|e| e.text.as_str())
    }

    pub fn set_editing_text(&mut self, text: &str) {
        if let Some(e) = &mut self.editing {
            e.text = text.to_string();
        }
    }

    /// Writes the edited title into the document. Returns whether a change was made.
    pub fn commit_edit(&mut self, doc: &mut Document) -> bool {
        let Some(e) = self.editing.take() else {
            return false;
        };
        // The final title can be much wider than it was when editing started.
        self.pending_reveal = Some(e.topic);
        if e.text == e.original || doc.sheet().topic(e.topic).is_none() {
            return false;
        }
        doc.edit_sheet("Edit Title", |s| s.set_title(e.topic, &e.text))
            .is_ok()
    }

    pub fn cancel_edit(&mut self) {
        self.editing = None;
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
            layout: layout.clone(),
        });
        layout
    }

    fn layout_with(&mut self, ctx: &egui::Context, doc: &Document) -> Arc<Layout> {
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
            layout: layout.clone(),
        });
        layout
    }

    fn fit(&mut self, layout: &Layout) {
        let b = layout.bounds;
        if b.size.x <= 0.0 || b.size.y <= 0.0 {
            return;
        }
        let avail = self.canvas_rect.size() - vec2(80.0, 80.0);
        let z = (avail.x / b.size.x)
            .min(avail.y / b.size.y)
            .clamp(MIN_ZOOM, 1.0);
        self.zoom = z;
        let c = b.center();
        self.offset = vec2(-c.x * z, -c.y * z);
    }

    fn do_reveal(&mut self, sheet: &Sheet, layout: &Layout, id: TopicId) {
        let Some(r) = layout.rect(id) else { return };
        let mut r = self.xf().rect(r);
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
}

/// Result of showing the canvas for one frame.
pub struct CanvasOutput {
    pub response: Response,
    /// Hyperlink the user clicked (host decides how to open it).
    pub open_link: Option<String>,
    /// Topic the context menu refers to (if the user right-clicked a topic).
    pub context_topic: Option<TopicId>,
    pub selection_changed: bool,
    /// Last edit error (e.g. trying to delete the central topic).
    pub error: Option<ymind_core::EditError>,
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

impl AddHandle {
    fn rect(&self) -> Rect {
        Rect::from_center_size(self.center, vec2(28.0, 28.0))
    }
}

fn add_handles(view: &MapView, sheet: &Sheet, layout: &Layout) -> Vec<AddHandle> {
    if !view.show_add_controls
        || view.selection.len() != 1
        || view.selected_object.is_some()
        || view.editing.is_some()
        || view.relationship_from.is_some()
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
        Id::unique("ymind_map_canvas")
    }

    /// Paints the current camera without interaction, selection, search dimming or editing chrome.
    pub fn show_snapshot(ui: &mut Ui, doc: &Document, view: &mut MapView) -> Rect {
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
                ..Default::default()
            },
        );
        rect
    }

    pub fn show(ui: &mut Ui, doc: &mut Document, view: &mut MapView) -> CanvasOutput {
        let id = Self::id();
        let rect = ui.available_rect_before_wrap();
        let response = ui.interact(rect, id, Sense::click_and_drag() | Sense::FOCUSABLE);
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Canvas, true, "Mind map canvas"));
        ui.advance_cursor_after_rect(rect);
        let ctx = ui.ctx().clone();
        view.canvas_rect = rect;
        view.prune_selection(doc.sheet());
        let before_sel = view.selection.clone();
        let before_object = view.selected_object;
        let mut out = CanvasOutput {
            response: response.clone(),
            open_link: None,
            context_topic: None,
            selection_changed: false,
            error: None,
        };

        if response.clicked() || response.drag_started() || response.secondary_clicked() {
            response.request_focus();
        }
        if ctx.memory(|m| m.focused()).is_none() && view.editing.is_none() {
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

        let mut layout = view.layout_with(&ctx, doc);
        if !view.initialized {
            view.initialized = true;
            let b = layout.bounds;
            if !view.explicit_initial_zoom
                && (b.size.x * view.zoom > rect.width() || b.size.y * view.zoom > rect.height())
            {
                view.fit(&layout);
            } else {
                let c = b.center();
                view.offset = vec2(-c.x, -c.y) * view.zoom;
            }
        }
        if std::mem::take(&mut view.pending_fit) {
            view.fit(&layout);
        }
        if let Some(r) = view.pending_reveal.take() {
            view.do_reveal(doc.sheet(), &layout, r);
        }

        // ---- pointer -------------------------------------------------------------------
        let pointer = response.hover_pos().or_else(|| ctx.pointer_latest_pos());
        let hovered = pointer
            .filter(|p| rect.contains(*p))
            .and_then(|p| layout.hit_test(view.xf().sheet(p)));

        if response.hovered()
            && view.relationship_drag.is_none()
            && !matches!(view.drag, Drag::Topics { .. })
        {
            let (scroll, zoom_delta, pos) =
                ctx.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.pointer.hover_pos()));
            if zoom_delta != 1.0 {
                view.zoom_by(zoom_delta, pos);
            } else if scroll != EVec2::ZERO {
                view.offset += scroll;
            }
        }

        let space =
            ctx.input(|i| i.key_down(Key::Space)) && view.editing.is_none() && response.has_focus();
        let relationship_handle = crate::relationship_editing::interact(
            ui,
            doc,
            view,
            &mut layout,
            &mut out.error,
            !space,
        );
        let handles = if space || matches!(view.drag, Drag::Topics { .. }) {
            Vec::new()
        } else {
            add_handles(view, doc.sheet(), &layout)
        };
        let over_handle = relationship_handle
            || pointer.is_some_and(|p| handles.iter().any(|h| h.rect().contains(p)));
        let press_on_handle = relationship_handle
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
        if response.drag_started()
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
            } else if let Some(t) = layout.hit_test(sp) {
                if !view.selection.contains(&t) {
                    view.select_only(t);
                }
                view.commit_edit(doc);
                let ids = crate::topic_drag::roots(doc.sheet(), &view.selection);
                if ids.is_empty() {
                    view.drag = Drag::None;
                    out.error = Some(ymind_core::EditError::RootNotAllowed);
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
            } else {
                view.drag = Drag::Rubber {
                    start: origin,
                    additive: mods.command || mods.shift,
                };
            }
        }
        let stale = matches!(&view.drag, Drag::Topics { state, sheet, selection, .. }
            if *state != doc.state_id() || *sheet != doc.active_index() || *selection != view.selection);
        if stale {
            view.drag = Drag::None;
            out.error = Some(ymind_core::EditError::Invalid(
                "topic dragging canceled because the document or selection changed".into(),
            ));
        } else if matches!(view.drag, Drag::Topics { .. })
            && (egui::Popup::is_any_open(&ctx)
                || !ui.is_enabled()
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
        if response.dragged() || response.drag_stopped() {
            let delta = response.drag_delta();
            let xf = view.xf();
            match &mut view.drag {
                Drag::Pan => view.offset += delta,
                Drag::Topics { current, .. } => {
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
        if response.clicked()
            && !over_handle
            && !press_on_handle
            && let Some(p) = response.interact_pointer_pos()
        {
            let sp = view.xf().sheet(p);
            if let Some(b) = layout.hit_badge(sp) {
                view.select_only(b);
                run(doc, view, Action::ToggleCollapse, &mut out);
            } else if let Some(t) = layout.hit_test(sp) {
                let tl = &layout.topics[&t];
                if let Some(from) = view.relationship_from.take() {
                    match doc.edit_sheet("Add Relationship", |s| s.add_relationship(from, t)) {
                        Ok(r) => {
                            view.select_object(ObjectSelection::Relationship(r));
                        }
                        Err(e) => out.error = Some(e),
                    }
                } else if tl.todo_rect.is_some_and(|r| r.expand(2.0).contains(sp)) {
                    let cur = doc.sheet().topic(t).and_then(|x| x.todo);
                    let next = Some(!cur.unwrap_or(false));
                    if let Err(e) = doc.edit_sheet("To-Do", |s| s.set_todo(t, next)) {
                        out.error = Some(e);
                    }
                } else if let Some((Icon::Link, _)) =
                    tl.icons.iter().find(|(_, r)| r.expand(2.0).contains(sp))
                {
                    out.open_link = doc.sheet().topic(t).and_then(|x| x.hyperlink.clone());
                } else {
                    if view.editing_topic() != Some(t) {
                        view.commit_edit(doc);
                    }
                    if mods.command || mods.shift {
                        view.toggle_selected(t);
                    } else {
                        view.select_only(t);
                    }
                }
            } else if let Some(object) = hit_object(&layout, sp, 6.0 / view.zoom) {
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
                    view.selected_object = None;
                }
            }
        }
        if response.double_clicked()
            && !editing_before_click
            && !over_handle
            && !press_on_handle
            && let Some(p) = response.interact_pointer_pos()
        {
            let sp = view.xf().sheet(p);
            if let Some(t) = layout.hit_test(sp) {
                view.select_only(t);
                view.begin_edit(doc.sheet(), t, true);
            } else if layout.hit_badge(sp).is_none()
                && hit_object(&layout, sp, 6.0 / view.zoom).is_none()
            {
                run(doc, view, Action::InsertFloating(sp), &mut out);
            }
        }
        if response.secondary_clicked()
            && !over_handle
            && let Some(p) = response.interact_pointer_pos()
        {
            view.commit_edit(doc);
            if let Some(t) = layout.hit_test(view.xf().sheet(p)) {
                if !view.selection.contains(&t) {
                    view.select_only(t);
                }
                out.context_topic = Some(t);
            } else if let Some(object) = hit_object(&layout, view.xf().sheet(p), 6.0 / view.zoom) {
                view.select_object(object);
            } else {
                view.selection.clear();
                view.selected_object = None;
            }
        }

        // ---- drag end ------------------------------------------------------------------
        if response.drag_stopped() {
            match std::mem::replace(&mut view.drag, Drag::None) {
                Drag::Rubber { start, additive } => {
                    view.selected_object = None;
                    let end = pointer.unwrap_or(start);
                    let a = view.xf().sheet(start);
                    let b = view.xf().sheet(end);
                    let area = ymind_core::Rect::from_min_size(
                        Vec2::new(a.x.min(b.x), a.y.min(b.y)),
                        Vec2::new((a.x - b.x).abs(), (a.y - b.y).abs()),
                    );
                    let hits = layout.topics_in(&area);
                    if !additive {
                        view.selection.clear();
                    }
                    for h in hits {
                        if !view.selection.contains(&h) {
                            view.selection.push(h);
                        }
                    }
                }
                Drag::Topics {
                    ids, grab, current, ..
                } => {
                    if pointer.is_some_and(|p| rect.contains(p)) {
                        drop_topics(doc, view, &layout, &ids, grab, current, &mut out);
                    } else {
                        out.error = Some(ymind_core::EditError::Invalid(
                            "drop a topic inside the map canvas".into(),
                        ));
                    }
                }
                _ => {}
            }
        } else if matches!(view.drag, Drag::Topics { .. })
            && !ctx.input(|i| i.pointer.primary_down())
        {
            view.drag = Drag::None;
        }

        // ---- keyboard ------------------------------------------------------------------
        if let Some(action) = handle_action {
            run(doc, view, action, &mut out);
        }
        if view.editing.is_none()
            && !egui::Popup::is_any_open(&ctx)
            && !matches!(view.drag, Drag::Topics { .. })
            && (response.has_focus() || ctx.memory(|m| m.focused()).is_none())
        {
            handle_keys(&ctx, doc, view, &mut out);
        }

        if doc.state_id() != view.cache.as_ref().map_or(0, |c| c.key.0) || view.editing.is_some() {
            layout = view.layout_with(&ctx, doc);
        }
        if let Some(r) = view.pending_reveal.take()
            && matches!(view.drag, Drag::None)
        {
            view.do_reveal(doc.sheet(), &layout, r);
        }

        // ---- paint ---------------------------------------------------------------------
        let painter = ui.painter_at(rect);
        let sheet = doc.sheet();
        painter.rect_filled(rect, CornerRadius::ZERO, sheet_background(sheet, view.dark));
        let xf = view.xf();
        let filter = view.filter.clone();
        let dimmed = if let Drag::Topics { members, .. } = &view.drag {
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
        };
        paint_map(&painter, sheet, &layout, xf, &opts);
        crate::relationship_editing::paint_target(&painter, view, &layout);
        if !space && !matches!(view.drag, Drag::Topics { .. }) {
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
                crate::topic_drag::paint(&painter, sheet, &layout, view, members, d, *current);
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

        if response.hovered() {
            let icon = match view.drag {
                Drag::Pan => egui::CursorIcon::Grabbing,
                Drag::Topics {
                    ref members,
                    current,
                    ..
                } if crate::topic_drag::target(&layout, members, current).is_err() => {
                    egui::CursorIcon::NotAllowed
                }
                Drag::Topics { .. } => egui::CursorIcon::Grabbing,
                _ if space => egui::CursorIcon::Grab,
                _ if hovered.is_some() || over_handle => egui::CursorIcon::PointingHand,
                _ => egui::CursorIcon::Default,
            };
            ctx.set_cursor_icon(icon);
        }

        out.selection_changed =
            before_sel != view.selection || before_object != view.selected_object;
        out
    }
}

fn run(doc: &mut Document, view: &mut MapView, action: Action, out: &mut CanvasOutput) {
    if let Err(e) = apply_action(doc, view, action) {
        out.error = Some(e);
    }
}

fn handle_keys(
    ctx: &egui::Context,
    doc: &mut Document,
    view: &mut MapView,
    out: &mut CanvasOutput,
) {
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
    for (m, k, a) in shortcuts {
        if ctx.input_mut(|i| i.consume_key(*m, *k)) {
            run(doc, view, a.clone(), out);
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
                run(doc, view, Action::Copy, out);
                if let Some(t) = view.clipboard_out.take() {
                    ctx.copy_text(t);
                }
            }
            egui::Event::Cut => {
                run(doc, view, Action::Cut, out);
                if let Some(t) = view.clipboard_out.take() {
                    ctx.copy_text(t);
                }
            }
            egui::Event::Paste(text) => run(doc, view, Action::Paste(Some(text)), out),
            egui::Event::Text(text) => {
                let ok = !text.trim().is_empty()
                    && !ctx.input(|i| i.modifiers.command || i.modifiers.alt);
                if ok
                    && view.selection.len() == 1
                    && let Some(t) = view.primary()
                {
                    view.begin_edit_with(doc.sheet(), t, &text);
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
    let r = xf.rect(tl.title_rect).expand(2.0);
    let style = &tl.style;
    let font = FontId::proportional((style.font_size * view.zoom).max(4.0));
    let color = color32(style.text_color);
    let edit_id = MapCanvas::id().with("edit");

    // Enter commits, Shift+Enter inserts a line break, Tab commits and adds a child.
    let (enter, tab, esc) = ui.input_mut(|i| {
        (
            i.consume_key(Modifiers::NONE, Key::Enter),
            i.consume_key(Modifiers::NONE, Key::Tab),
            i.consume_key(Modifiers::NONE, Key::Escape),
        )
    });
    if enter || tab || esc {
        view.commit_edit(doc);
        ui.memory_mut(|m| m.request_focus(MapCanvas::id()));
        if tab {
            run(doc, view, Action::InsertChild, out);
        }
        return;
    }

    let edit = view.editing.as_mut().unwrap();
    let align = match style.align {
        ymind_core::TextAlign::Left => egui::Align::LEFT,
        ymind_core::TextAlign::Right => egui::Align::RIGHT,
        _ => egui::Align::Center,
    };
    let te = TextEdit::multiline(&mut edit.text)
        .id(edit_id)
        .font(font)
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
        .lock_focus(true);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(r));
    let output = te.show(&mut child);
    if output.response.changed() {
        view.pending_reveal = Some(edit.topic);
    }
    output.response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::MultilineTextInput, true, "Topic title")
    });
    if !edit.focus_requested {
        edit.focus_requested = true;
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
    } else if output.response.lost_focus() {
        view.commit_edit(doc);
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
        layout,
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
        for id in &ids {
            let center = layout.rect(*id).map(|r| r.center() + d).unwrap_or(current);
            match target {
                Some(t) => {
                    let n = s.children(t).len();
                    s.move_topic(*id, t, n)?;
                }
                None if s.is_floating(*id) => s.set_position(*id, center)?,
                None => s.detach_to_floating(*id, center)?,
            }
        }
        if let Some(target) = target {
            s.set_collapsed(target, false)?;
        }
        Ok(())
    });
    if let Err(e) = res {
        out.error = Some(e);
    } else {
        view.selection = ids;
    }
}

fn hit_relationship(layout: &Layout, p: Vec2, tol: f32) -> Option<RelationshipId> {
    layout.relationships.iter().rev().find_map(|r| {
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

fn hit_object(layout: &Layout, p: Vec2, tol: f32) -> Option<ObjectSelection> {
    if let Some(id) = hit_relationship(layout, p, tol) {
        return Some(ObjectSelection::Relationship(id));
    }
    for summary in layout.summaries.iter().rev() {
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
#[path = "relationship_creation_tests.rs"]
mod relationship_creation_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    use ymind_core::Workbook;

    struct State {
        doc: Document,
        view: MapView,
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
                    sheet.structure = ymind_core::Structure::LogicRight;
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
                    };
                    assert!(layout.hit_test(p).is_none());
                    assert_eq!(hit_object(&layout, p, 6.0 / zoom), Some(object));
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
            hit_object(&layout, inner_rect.top_center(), 0.1),
            Some(ObjectSelection::Boundary(inner))
        );
        assert_eq!(hit_object(&layout, inner_rect.center(), 0.1), None);
        let outer_rect = layout
            .boundaries
            .iter()
            .find(|b| b.id == outer)
            .unwrap()
            .rect;
        assert_eq!(
            hit_object(&layout, outer_rect.left_center(), 0.1),
            Some(ObjectSelection::Boundary(outer))
        );
        layout
            .boundaries
            .iter_mut()
            .find(|b| b.id == outer)
            .unwrap()
            .rect = ymind_core::Rect::from_min_size(inner_rect.min, inner_rect.size * 2.0);
        assert_eq!(
            hit_object(&layout, inner_rect.top_center(), 0.1),
            Some(ObjectSelection::Boundary(inner))
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
            ymind_core::Structure::LogicRight,
            ymind_core::Structure::LogicLeft,
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
                    let dx = if structure == ymind_core::Structure::LogicRight {
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
        assert_eq!(hit_relationship(&layout, r.mid, 6.0), Some(r.id));
        assert_eq!(hit_relationship(&layout, Vec2::new(1e5, 1e5), 6.0), None);
    }
}
