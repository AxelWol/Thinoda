//! Interactive mind map canvas widget.

use std::sync::Arc;

use egui::{
    Color32, CornerRadius, EventFilter, FontId, Id, Key, Modifiers, PointerButton, Pos2, Rect,
    Response, Sense, Stroke, StrokeKind, TextEdit, Ui, Vec2 as EVec2, pos2, vec2,
};
use ymind_core::{
    ApproxMeasure, Document, Layout, RelationshipId, Sheet, TopicId, clipboard::ClipboardPayload,
    geometry::Vec2, layout::Icon, layout_sheet, navigation::Direction,
};

use crate::{
    actions::{Action, apply_action},
    measure::EguiMeasure,
    render::{PaintOptions, Xf, color32, paint_map, sheet_background},
};

pub const MIN_ZOOM: f32 = 0.1;
pub const MAX_ZOOM: f32 = 4.0;

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
        grab: Vec2,
        current: Vec2,
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
    pub selected_relationship: Option<RelationshipId>,
    /// Source topic while the user is picking the target of a new relationship.
    pub relationship_from: Option<TopicId>,
    pub titles: NewTopicTitles,
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
    canvas_rect: Rect,
}

impl Default for MapView {
    fn default() -> Self {
        Self {
            offset: EVec2::ZERO,
            zoom: 1.0,
            selection: Vec::new(),
            selected_relationship: None,
            relationship_from: None,
            titles: NewTopicTitles::default(),
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
            canvas_rect: Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0)),
        }
    }
}

impl MapView {
    pub fn select_only(&mut self, id: TopicId) {
        self.selection = vec![id];
        self.selected_relationship = None;
    }

    pub fn toggle_selected(&mut self, id: TopicId) {
        if let Some(i) = self.selection.iter().position(|x| *x == id) {
            self.selection.remove(i);
        } else {
            self.selection.push(id);
        }
    }

    pub fn primary(&self) -> Option<TopicId> {
        self.selection.last().copied()
    }

    /// Removes deleted/hidden topics from the selection.
    pub fn prune_selection(&mut self, sheet: &Sheet) {
        self.selection
            .retain(|id| sheet.topic(*id).is_some() && sheet.is_visible(*id));
        if self
            .selected_relationship
            .is_some_and(|r| !sheet.relationships.iter().any(|x| x.id == r))
        {
            self.selected_relationship = None;
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
        self.selected_relationship = None;
        self.relationship_from = None;
        self.editing = None;
        self.drag = Drag::None;
        self.cache = None;
        self.initialized = false;
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

    fn xf(&self) -> Xf {
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

    fn do_reveal(&mut self, layout: &Layout, id: TopicId) {
        let Some(r) = layout.rect(id) else { return };
        let r = self.xf().rect(r);
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

impl MapCanvas {
    pub fn id() -> Id {
        Id::unique("ymind_map_canvas")
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
            if b.size.x * view.zoom > rect.width() || b.size.y * view.zoom > rect.height() {
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
            view.do_reveal(&layout, r);
        }

        // ---- pointer -------------------------------------------------------------------
        let pointer = response.hover_pos().or_else(|| ctx.pointer_latest_pos());
        let hovered = pointer
            .filter(|p| rect.contains(*p))
            .and_then(|p| layout.hit_test(view.xf().sheet(p)));

        if response.hovered() {
            let (scroll, zoom_delta, pos) =
                ctx.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.pointer.hover_pos()));
            if zoom_delta != 1.0 {
                view.zoom_by(zoom_delta, pos);
            } else if scroll != EVec2::ZERO {
                view.offset += scroll;
            }
        }

        let space = ctx.input(|i| i.key_down(Key::Space)) && view.editing.is_none();
        if response.drag_started() {
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
                let ids: Vec<TopicId> = view.selection.clone();
                view.commit_edit(doc);
                view.drag = Drag::Topics {
                    ids,
                    grab: sp,
                    current: sp,
                };
            } else {
                view.drag = Drag::Rubber {
                    start: origin,
                    additive: mods.command || mods.shift,
                };
            }
        }
        if response.dragged() {
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
        let mods = ctx.input(|i| i.modifiers);
        if response.clicked()
            && let Some(p) = response.interact_pointer_pos()
        {
            let sp = view.xf().sheet(p);
            if let Some(b) = layout.hit_badge(sp) {
                view.select_only(b);
                run(doc, view, Action::ToggleCollapse, &mut out);
            } else if let Some(t) = layout.hit_test(sp) {
                let tl = &layout.topics[&t];
                if let Some(from) = view.relationship_from.take() {
                    if from != t {
                        match doc.edit_sheet("Add Relationship", |s| s.add_relationship(from, t)) {
                            Ok(r) => {
                                view.selection.clear();
                                view.selected_relationship = Some(r);
                            }
                            Err(e) => out.error = Some(e),
                        }
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
            } else if let Some(r) = hit_relationship(&layout, sp, 6.0 / view.zoom) {
                view.commit_edit(doc);
                view.selection.clear();
                view.selected_relationship = Some(r);
            } else {
                view.commit_edit(doc);
                view.relationship_from = None;
                if !(mods.command || mods.shift) {
                    view.selection.clear();
                    view.selected_relationship = None;
                }
            }
        }
        if response.double_clicked()
            && let Some(p) = response.interact_pointer_pos()
        {
            let sp = view.xf().sheet(p);
            if let Some(t) = layout.hit_test(sp) {
                view.select_only(t);
                view.begin_edit(doc.sheet(), t, true);
            } else if layout.hit_badge(sp).is_none() {
                run(doc, view, Action::InsertFloating(sp), &mut out);
            }
        }
        if response.secondary_clicked()
            && let Some(p) = response.interact_pointer_pos()
        {
            view.commit_edit(doc);
            if let Some(t) = layout.hit_test(view.xf().sheet(p)) {
                if !view.selection.contains(&t) {
                    view.select_only(t);
                }
                out.context_topic = Some(t);
            }
        }

        // ---- drag end ------------------------------------------------------------------
        if response.drag_stopped() {
            match std::mem::replace(&mut view.drag, Drag::None) {
                Drag::Rubber { start, additive } => {
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
                Drag::Topics { ids, grab, current } => {
                    drop_topics(doc, view, &layout, &ids, grab, current, &mut out);
                }
                _ => {}
            }
        }

        // ---- keyboard ------------------------------------------------------------------
        if view.editing.is_none() && (response.has_focus() || ctx.memory(|m| m.focused()).is_none())
        {
            handle_keys(&ctx, doc, view, &mut out);
        }

        if doc.state_id() != view.cache.as_ref().map_or(0, |c| c.key.0) || view.editing.is_some() {
            layout = view.layout_with(&ctx, doc);
        }
        if let Some(r) = view.pending_reveal.take() {
            view.do_reveal(&layout, r);
        }

        // ---- paint ---------------------------------------------------------------------
        let painter = ui.painter_at(rect);
        let sheet = doc.sheet();
        painter.rect_filled(rect, CornerRadius::ZERO, sheet_background(sheet, view.dark));
        let xf = view.xf();
        let filter = view.filter.clone();
        let opts = PaintOptions {
            selection: &view.selection,
            hovered,
            editing: view.editing_topic(),
            accent: view.accent,
            dark: view.dark,
            filter: filter.as_deref(),
            selected_relationship: view.selected_relationship,
        };
        paint_map(&painter, sheet, &layout, xf, &opts);

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
            Drag::Topics { ids, grab, current } => {
                let d = *current - *grab;
                for t in ids {
                    if let Some(r) = layout.rect(*t) {
                        let r = xf.rect(r.translate(d));
                        painter.rect(
                            r,
                            CornerRadius::same(6),
                            view.accent.gamma_multiply(0.15),
                            Stroke::new(1.5, view.accent),
                            StrokeKind::Outside,
                        );
                    }
                }
                if let Some(target) = drop_target(sheet, &layout, ids, *current)
                    && let Some(r) = layout.rect(target)
                {
                    painter.rect_stroke(
                        xf.rect(r).expand(4.0),
                        CornerRadius::same(8),
                        Stroke::new(2.5, view.accent),
                        StrokeKind::Outside,
                    );
                }
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
                Drag::Topics { .. } => egui::CursorIcon::Grabbing,
                _ if space => egui::CursorIcon::Grab,
                _ if hovered.is_some() => egui::CursorIcon::PointingHand,
                _ => egui::CursorIcon::Default,
            };
            ctx.set_cursor_icon(icon);
        }

        out.selection_changed = before_sel != view.selection;
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
        if view.selected_relationship.is_some() {
            view.selected_relationship = None;
        } else {
            view.selection.clear();
        }
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

fn drop_target(sheet: &Sheet, layout: &Layout, ids: &[TopicId], at: Vec2) -> Option<TopicId> {
    let t = layout.hit_test(at)?;
    if ids
        .iter()
        .any(|id| *id == t || sheet.subtree_ids(*id).contains(&t))
    {
        return None;
    }
    Some(t)
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
    let sheet = doc.sheet();
    // Only move the top-most selected topics (children move with their parents).
    let ids: Vec<TopicId> = ids
        .iter()
        .copied()
        .filter(|id| {
            *id != sheet.root
                && !ids
                    .iter()
                    .any(|o| o != id && sheet.subtree_ids(*o).contains(id))
        })
        .collect();
    if ids.is_empty() {
        return;
    }
    let target = drop_target(sheet, layout, &ids, current);
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
        Ok(())
    });
    if let Err(e) = res {
        out.error = Some(e);
    }
    view.selection = ids;
}

fn bezier(a: Vec2, b: Vec2, c: Vec2, d: Vec2, t: f32) -> Vec2 {
    let u = 1.0 - t;
    a * (u * u * u) + b * (3.0 * u * u * t) + c * (3.0 * u * t * t) + d * (t * t * t)
}

fn hit_relationship(layout: &Layout, p: Vec2, tol: f32) -> Option<RelationshipId> {
    layout.relationships.iter().rev().find_map(|r| {
        let mut prev = r.start;
        for i in 1..=24 {
            let q = bezier(r.start, r.c1, r.c2, r.end, i as f32 / 24.0);
            if dist_to_segment(p, prev, q) <= tol {
                return Some(r.id);
            }
            prev = q;
        }
        None
    })
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
    fn canvas_is_accessible() {
        let h = harness();
        h.get_by_label("Mind map canvas");
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
