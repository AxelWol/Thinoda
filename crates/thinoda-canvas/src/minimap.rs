//! A small, cached overview of a canvas scene.
//!
//! Build [`MinimapScene`] when the scene or layout changes, not on every camera
//! movement. It scans visible items to obtain exact bounds and samples
//! at most [`MAX_MINIMAP_ITEMS`] items for painting. Both the scan and the sample
//! use [`SceneItem::visible_bounds`], including frame clipping.
//!
//! The host supplies the minimap's screen rectangle and a stable, per-view ID.
//! Call [`show_minimap`] on a foreground layer before dispatching canvas input,
//! apply its returned offset to `MapView::offset`, and suppress canvas pointer
//! handling (including scroll and touch) while `blocks_canvas_pointer` is true.
//! If the host must dispatch canvas input before painting the overlay, query
//! [`MinimapState::blocks_canvas_pointer`] first using the same rectangle and
//! call `show_minimap` later in that frame. Keep the state even when a gesture
//! leaves the rectangle. If the minimap is hidden mid-gesture, cancel it, gate
//! canvas input with [`MinimapState::has_pointer_capture`], and call
//! [`MinimapState::finish_frame`] after dispatching input until it is released.
//! A focused minimap consumes arrow keys and reports `blocks_canvas_keyboard`;
//! do not also route those keys to the canvas.
//!
//! [`crate::MapCanvas`] hosts this component when
//! [`crate::MapView::set_minimap_visible`] is enabled. Custom tools that run
//! before the canvas can use [`crate::MapView::minimap_blocks_pointer`] and
//! [`crate::MapView::minimap_blocks_keyboard`] as input preflights.
//!
//! All three label strings are supplied by the host so they can be localized.

use egui::{
    CursorIcon, Event, EventFilter, Id, Key, Modifiers, PointerButton, Pos2, Rect, Response, Sense,
    Stroke, StrokeKind, Ui, Vec2, pos2, vec2,
};
use thinoda_core::{Scene, SceneId, SceneItem};

/// Upper bound on the number of scene items painted in one frame.
pub const MAX_MINIMAP_ITEMS: usize = 1_024;
/// A recommended maximum overlay size, in logical screen points.
pub const DEFAULT_MINIMAP_SIZE: Vec2 = Vec2::new(220.0, 150.0);
const VIEW_INFLUENCE: f64 = 4.0;
const WORLD_MARGIN: f64 = 0.05;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ItemKind {
    Frame,
    Topic,
    Object,
}

impl ItemKind {
    fn index(self) -> usize {
        match self {
            Self::Frame => 0,
            Self::Topic => 1,
            Self::Object => 2,
        }
    }

    fn of(id: SceneId) -> Self {
        match id {
            SceneId::Zone(_) => Self::Frame,
            SceneId::Topic(_) => Self::Topic,
            SceneId::Object(_) => Self::Object,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Bounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl Bounds {
    fn from_item(item: &SceneItem) -> Option<Self> {
        let rect = item.visible_bounds()?;
        if !rect.min.x.is_finite()
            || !rect.min.y.is_finite()
            || !rect.size.x.is_finite()
            || !rect.size.y.is_finite()
            || rect.size.x < 0.0
            || rect.size.y < 0.0
        {
            return None;
        }
        let min_x = f64::from(rect.min.x);
        let min_y = f64::from(rect.min.y);
        Some(Self {
            min_x,
            min_y,
            max_x: min_x + f64::from(rect.size.x),
            max_y: min_y + f64::from(rect.size.y),
        })
    }

    fn width(self) -> f64 {
        self.max_x - self.min_x
    }

    fn height(self) -> f64 {
        self.max_y - self.min_y
    }

    fn center(self) -> (f64, f64) {
        (
            self.min_x + self.width() / 2.0,
            self.min_y + self.height() / 2.0,
        )
    }

    fn union(self, other: Self) -> Self {
        Self {
            min_x: self.min_x.min(other.min_x),
            min_y: self.min_y.min(other.min_y),
            max_x: self.max_x.max(other.max_x),
            max_y: self.max_y.max(other.max_y),
        }
    }

    fn intersection(self, other: Self) -> Option<Self> {
        let result = Self {
            min_x: self.min_x.max(other.min_x),
            min_y: self.min_y.max(other.min_y),
            max_x: self.max_x.min(other.max_x),
            max_y: self.max_y.min(other.max_y),
        };
        (result.min_x <= result.max_x && result.min_y <= result.max_y).then_some(result)
    }

    fn clamp_to(self, limits: Self) -> Self {
        Self {
            min_x: self.min_x.clamp(limits.min_x, limits.max_x),
            min_y: self.min_y.clamp(limits.min_y, limits.max_y),
            max_x: self.max_x.clamp(limits.min_x, limits.max_x),
            max_y: self.max_y.clamp(limits.min_y, limits.max_y),
        }
    }

    // Keep point and line scenes usable, including at coordinates where adding a
    // small world-space width would be lost to floating-point rounding.
    fn stabilized(self) -> Self {
        let (cx, cy) = self.center();
        let minimum = (self.width().max(self.height()) * 0.02).max(1.0);
        let width = self
            .width()
            .max(minimum)
            .max(cx.abs() * f64::EPSILON * 16.0);
        let height = self
            .height()
            .max(minimum)
            .max(cy.abs() * f64::EPSILON * 16.0);
        Self {
            min_x: cx - width / 2.0,
            min_y: cy - height / 2.0,
            max_x: cx + width / 2.0,
            max_y: cy + height / 2.0,
        }
    }

    fn expand(self, x: f64, y: f64) -> Self {
        Self {
            min_x: self.min_x - x,
            min_y: self.min_y - y,
            max_x: self.max_x + x,
            max_y: self.max_y + y,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct OverviewItem {
    kind: ItemKind,
    bounds: Bounds,
}

/// A compact snapshot. Rebuild whenever the scene's layout, clipping or items change.
///
/// Construction is O(scene items); subsequent paints are bounded independently
/// of scene size. Sampling is deterministic, preserves paint order, and reserves
/// space for each nonempty item category.
#[derive(Debug, Clone, Default)]
pub struct MinimapScene {
    bounds: Option<Bounds>,
    items: Vec<OverviewItem>,
    visible_count: usize,
}

impl MinimapScene {
    pub fn from_scene(scene: &Scene) -> Self {
        let mut result = Self::default();
        let mut counts = [0_usize; 3];
        for item in &scene.items {
            if let Some(bounds) = Bounds::from_item(item) {
                let index = ItemKind::of(item.id).index();
                counts[index] += 1;
                result.visible_count += 1;
                result.bounds = Some(result.bounds.map_or(bounds, |old| old.union(bounds)));
            }
        }

        let quotas = sample_quotas(counts);
        result.items.reserve(quotas.iter().sum());
        let mut seen = [0_usize; 3];
        let mut selected = [0_usize; 3];
        for item in &scene.items {
            let Some(bounds) = Bounds::from_item(item) else {
                continue;
            };
            let kind = ItemKind::of(item.id);
            let index = kind.index();
            if selected[index] < quotas[index]
                && seen[index] == sample_index(selected[index], counts[index], quotas[index])
            {
                result.items.push(OverviewItem { kind, bounds });
                selected[index] += 1;
            }
            seen[index] += 1;
        }
        result
    }

    /// Number of valid, visible items, before sampling.
    pub fn visible_count(&self) -> usize {
        self.visible_count
    }

    /// Number of items that will be painted.
    pub fn sampled_count(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.visible_count == 0
    }
}

fn sample_quotas(counts: [usize; 3]) -> [usize; 3] {
    let mut quotas = [0; 3];
    let mut remaining = MAX_MINIMAP_ITEMS;
    for index in 0..3 {
        let groups = counts[index..].iter().filter(|count| **count > 0).count();
        if groups > 0 {
            let share = remaining.div_ceil(groups);
            quotas[index] = counts[index].min(share);
            remaining -= quotas[index];
        }
    }
    for index in 0..3 {
        let extra = (counts[index] - quotas[index]).min(remaining);
        quotas[index] += extra;
        remaining -= extra;
    }
    quotas
}

fn sample_index(index: usize, count: usize, quota: usize) -> usize {
    if quota <= 1 {
        0
    } else {
        ((index as u128 * (count - 1) as u128) / (quota - 1) as u128) as usize
    }
}

/// The same camera transform as `MapView`:
/// `screen = canvas_rect.center() + offset + sheet_position * zoom`.
#[derive(Debug, Clone, Copy)]
pub struct MinimapCamera {
    pub canvas_rect: Rect,
    pub offset: Vec2,
    pub zoom: f32,
}

impl MinimapCamera {
    fn valid_zoom(self) -> bool {
        self.zoom.is_finite() && self.zoom > 0.0
    }

    fn viewport(self) -> Option<Bounds> {
        if !self.valid_zoom()
            || !valid_rect(self.canvas_rect)
            || !self.offset.x.is_finite()
            || !self.offset.y.is_finite()
        {
            return None;
        }
        let zoom = f64::from(self.zoom);
        let half_width =
            (f64::from(self.canvas_rect.max.x) - f64::from(self.canvas_rect.min.x)) / (2.0 * zoom);
        let half_height =
            (f64::from(self.canvas_rect.max.y) - f64::from(self.canvas_rect.min.y)) / (2.0 * zoom);
        let center_x = -f64::from(self.offset.x) / zoom;
        let center_y = -f64::from(self.offset.y) / zoom;
        Some(Bounds {
            min_x: center_x - half_width,
            min_y: center_y - half_height,
            max_x: center_x + half_width,
            max_y: center_y + half_height,
        })
    }
}

/// Accessible and tooltip text. Supply translated strings from the host catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinimapLabels {
    /// Screen-reader name, preferably including the keyboard interaction hint.
    pub accessible_name: String,
    pub tooltip: String,
    /// Appended to the accessible name and tooltip if the viewport is offscreen.
    pub viewport_offscreen: String,
}

impl Default for MinimapLabels {
    fn default() -> Self {
        Self {
            accessible_name: "Board minimap. Click or drag to pan; arrow keys move the view; Shift makes larger steps.".into(),
            tooltip: "Click or drag to pan. Arrow keys move the view; Shift makes larger steps.".into(),
            viewport_offscreen: "The current view is outside the minimap.".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum PointerOwner {
    #[default]
    None,
    Minimap,
    Elsewhere,
}

#[derive(Debug, Clone, Copy)]
struct Gesture {
    transform: Transform,
    minimap_rect: Rect,
    zoom: f32,
    start: Pos2,
    start_center: (f64, f64),
    grabbed_viewport: bool,
    dragged: bool,
}

/// Per-view interaction state. Keep it across frames and give each view its own state.
#[derive(Debug, Default)]
pub struct MinimapState {
    owner: PointerOwner,
    touch_captured: bool,
    gesture: Option<Gesture>,
    gesture_canceled: bool,
}

impl MinimapState {
    /// Whether canvas pointer, scroll and touch processing must be suppressed.
    ///
    /// This can be queried *before* showing the minimap. An existing canvas
    /// drag which started outside the minimap is not stolen when it crosses it.
    /// A gesture which started inside remains blocked through its release, even
    /// outside the rectangle. Continue querying while dismissing an active
    /// minimap gesture so its release cannot fall through to the canvas. When
    /// the minimap is hidden, use [`Self::has_pointer_capture`] instead so its
    /// former rectangle does not keep blocking unrelated input.
    pub fn blocks_canvas_pointer(&self, ctx: &egui::Context, rect: Rect) -> bool {
        ctx.input(|input| {
            let touch_started_inside = input.events.iter().any(|event| {
                matches!(
                    event,
                    Event::Touch {
                        phase: egui::TouchPhase::Start,
                        pos,
                        ..
                    } if valid_rect(rect) && rect.contains(*pos)
                )
            });
            if self.touch_captured || touch_started_inside || self.owner == PointerOwner::Minimap {
                return true;
            }
            if self.owner == PointerOwner::Elsewhere || !valid_rect(rect) {
                return false;
            }
            let pointer = &input.pointer;
            if self.owner == PointerOwner::None
                && !pointer.any_pressed()
                && (pointer.any_down() || pointer.any_released())
            {
                return false;
            }
            if pointer.any_down() || pointer.any_released() {
                pointer
                    .press_origin()
                    .is_some_and(|position| rect.contains(position))
            } else {
                pointer
                    .hover_pos()
                    .is_some_and(|position| rect.contains(position))
            }
        })
    }

    /// Cancel panning without relinquishing a held pointer to the canvas.
    pub fn cancel(&mut self) {
        self.gesture = None;
        self.gesture_canceled = self.owner == PointerOwner::Minimap || self.touch_captured;
    }

    /// Whether a pointer or touch sequence that began on the minimap is held.
    /// This remains true for the release frame until [`Self::finish_frame`].
    pub fn has_pointer_capture(&self) -> bool {
        self.owner == PointerOwner::Minimap || self.touch_captured
    }

    /// Retire released input after the host has gated canvas processing.
    /// `show_minimap` does this automatically; call it explicitly while hidden.
    pub fn finish_frame(&mut self, ctx: &egui::Context) {
        let (primary_down, any_down, any_touches) = ctx.input(|input| {
            (
                input.pointer.primary_down(),
                input.pointer.any_down(),
                input.any_touches(),
            )
        });
        if !primary_down {
            self.gesture = None;
            self.gesture_canceled = false;
        }
        if !any_down {
            self.owner = PointerOwner::None;
        }
        // A synthesized pointer release can arrive after the last raw touch.
        if !any_touches && !any_down {
            self.touch_captured = false;
        }
    }
}

/// Result for the host to apply before processing the underlying canvas.
pub struct MinimapOutput {
    pub response: Response,
    /// A replacement `MapView::offset`, if this frame panned the camera.
    pub offset: Option<Vec2>,
    pub blocks_canvas_pointer: bool,
    pub blocks_canvas_keyboard: bool,
    /// The actual viewport lies entirely beyond the bounded overview.
    pub viewport_offscreen: bool,
}

#[derive(Debug, Clone, Copy)]
struct Transform {
    world: Bounds,
    screen: Rect,
    scale: f64,
}

impl Transform {
    fn new(world: Bounds, target: Rect) -> Option<Self> {
        if !valid_rect(target) {
            return None;
        }
        let world = world.stabilized();
        let target_width = f64::from(target.max.x) - f64::from(target.min.x);
        let target_height = f64::from(target.max.y) - f64::from(target.min.y);
        let scale = (target_width / world.width()).min(target_height / world.height());
        if !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        let center_x = (f64::from(target.min.x) + f64::from(target.max.x)) / 2.0;
        let center_y = (f64::from(target.min.y) + f64::from(target.max.y)) / 2.0;
        let half_width = world.width() * scale / 2.0;
        let half_height = world.height() * scale / 2.0;
        let screen = Rect::from_min_max(
            pos2(
                (center_x - half_width) as f32,
                (center_y - half_height) as f32,
            ),
            pos2(
                (center_x + half_width) as f32,
                (center_y + half_height) as f32,
            ),
        );
        valid_rect(screen).then_some(Self {
            world,
            screen,
            scale,
        })
    }

    fn screen_point(self, x: f64, y: f64) -> Pos2 {
        let (world_x, world_y) = self.world.center();
        let (screen_x, screen_y) = screen_center(self.screen);
        pos2(
            (screen_x + (x - world_x) * self.scale) as f32,
            (screen_y + (y - world_y) * self.scale) as f32,
        )
    }

    fn world_point(self, point: Pos2) -> (f64, f64) {
        let point = self.screen.clamp(point);
        let (world_x, world_y) = self.world.center();
        let (screen_x, screen_y) = screen_center(self.screen);
        (
            world_x + (f64::from(point.x) - screen_x) / self.scale,
            world_y + (f64::from(point.y) - screen_y) / self.scale,
        )
    }

    fn rect(self, bounds: Bounds) -> Option<Rect> {
        let visible = bounds.intersection(self.world)?;
        Some(Rect::from_min_max(
            self.screen_point(visible.min_x, visible.min_y),
            self.screen_point(visible.max_x, visible.max_y),
        ))
    }

    fn viewport_marker(self, viewport: Option<Bounds>) -> (Option<Rect>, bool) {
        let Some(viewport) = viewport else {
            return (None, false);
        };
        if let Some(rect) = self.rect(viewport) {
            return (Some(minimum_screen_size(rect, self.screen, 2.0)), false);
        }
        let (x, y) = viewport.center();
        let point = self.screen_point(
            x.clamp(self.world.min_x, self.world.max_x),
            y.clamp(self.world.min_y, self.world.max_y),
        );
        let point_rect = Rect::from_min_max(point, point);
        (
            Some(minimum_screen_size(point_rect, self.screen, 5.0)),
            true,
        )
    }
}

fn valid_rect(rect: Rect) -> bool {
    rect.min.x.is_finite()
        && rect.min.y.is_finite()
        && rect.max.x.is_finite()
        && rect.max.y.is_finite()
        && rect.max.x > rect.min.x
        && rect.max.y > rect.min.y
        && rect.width().is_finite()
        && rect.height().is_finite()
}

/// Place a minimap at the lower right without covering more than 40% of either
/// canvas dimension. Returns `None` when there is not enough room for a useful
/// 64 × 48-point target; the host can hide its toggleable overlay in that case.
pub fn bottom_right_rect(canvas_rect: Rect, preferred_size: Vec2, margin: f32) -> Option<Rect> {
    if !valid_rect(canvas_rect)
        || !preferred_size.x.is_finite()
        || !preferred_size.y.is_finite()
        || preferred_size.x <= 0.0
        || preferred_size.y <= 0.0
        || !margin.is_finite()
    {
        return None;
    }
    let margin = margin.max(0.0);
    let width = preferred_size
        .x
        .min(canvas_rect.width() * 0.4)
        .min(canvas_rect.width() - 2.0 * margin);
    let height = preferred_size
        .y
        .min(canvas_rect.height() * 0.4)
        .min(canvas_rect.height() - 2.0 * margin);
    if width < 64.0 || height < 48.0 {
        return None;
    }
    let max = canvas_rect.max - vec2(margin, margin);
    let result = Rect::from_min_max(max - vec2(width, height), max);
    (valid_rect(result) && canvas_rect.contains_rect(result)).then_some(result)
}

fn screen_center(rect: Rect) -> (f64, f64) {
    (
        (f64::from(rect.min.x) + f64::from(rect.max.x)) / 2.0,
        (f64::from(rect.min.y) + f64::from(rect.max.y)) / 2.0,
    )
}

fn minimum_screen_size(rect: Rect, limits: Rect, minimum: f32) -> Rect {
    let width = rect.width().max(minimum).min(limits.width());
    let height = rect.height().max(minimum).min(limits.height());
    let center = rect.center();
    let left =
        (center.x - width / 2.0).clamp(limits.left(), (limits.right() - width).max(limits.left()));
    let top =
        (center.y - height / 2.0).clamp(limits.top(), (limits.bottom() - height).max(limits.top()));
    Rect::from_min_size(pos2(left, top), vec2(width, height))
}

fn overview_bounds(scene: &MinimapScene, viewport: Option<Bounds>) -> Bounds {
    let bounds = if let Some(scene_bounds) = scene.bounds {
        let scene_bounds = scene_bounds.stabilized();
        let limits = scene_bounds.expand(
            scene_bounds.width() * VIEW_INFLUENCE,
            scene_bounds.height() * VIEW_INFLUENCE,
        );
        viewport.map_or(scene_bounds, |view| {
            scene_bounds.union(view.clamp_to(limits))
        })
    } else {
        viewport.unwrap_or(Bounds {
            min_x: -100.0,
            min_y: -100.0,
            max_x: 100.0,
            max_y: 100.0,
        })
    }
    .stabilized();
    bounds
        .expand(
            bounds.width() * WORLD_MARGIN,
            bounds.height() * WORLD_MARGIN,
        )
        .stabilized()
}

fn offset_for_center(center: (f64, f64), zoom: f32) -> Option<Vec2> {
    if !zoom.is_finite() || zoom <= 0.0 || !center.0.is_finite() || !center.1.is_finite() {
        return None;
    }
    let limit = f64::from(f32::MAX);
    Some(vec2(
        (-center.0 * f64::from(zoom)).clamp(-limit, limit) as f32,
        (-center.1 * f64::from(zoom)).clamp(-limit, limit) as f32,
    ))
}

fn drag_center(gesture: Gesture, point: Pos2) -> (f64, f64) {
    let current = gesture.transform.world_point(point);
    if gesture.grabbed_viewport {
        let start = gesture.transform.world_point(gesture.start);
        (
            gesture.start_center.0 + current.0 - start.0,
            gesture.start_center.1 + current.1 - start.1,
        )
    } else {
        current
    }
}

fn keyboard_offset(ui: &Ui, response: &Response, camera: MinimapCamera) -> Option<Vec2> {
    if !response.has_focus()
        || !ui.is_enabled()
        || !camera.valid_zoom()
        || !valid_rect(camera.canvas_rect)
        || !camera.offset.x.is_finite()
        || !camera.offset.y.is_finite()
    {
        return None;
    }
    ui.ctx().memory_mut(|memory| {
        memory.set_focus_lock_filter(
            response.id,
            EventFilter {
                horizontal_arrows: true,
                vertical_arrows: true,
                ..Default::default()
            },
        )
    });
    let mut dx = 0.0_f64;
    let mut dy = 0.0_f64;
    for (modifiers, factor) in [(Modifiers::NONE, 1.0), (Modifiers::SHIFT, 5.0)] {
        for (key, x, y) in [
            (Key::ArrowLeft, -1.0, 0.0),
            (Key::ArrowRight, 1.0, 0.0),
            (Key::ArrowUp, 0.0, -1.0),
            (Key::ArrowDown, 0.0, 1.0),
        ] {
            if ui.input_mut(|input| input.consume_key(modifiers, key)) {
                dx += x * factor;
                dy += y * factor;
            }
        }
    }
    if dx == 0.0 && dy == 0.0 {
        return None;
    }
    let width = f64::from(camera.canvas_rect.max.x) - f64::from(camera.canvas_rect.min.x);
    let height = f64::from(camera.canvas_rect.max.y) - f64::from(camera.canvas_rect.min.y);
    let limit = f64::from(f32::MAX);
    Some(vec2(
        (f64::from(camera.offset.x) - dx * width * 0.1).clamp(-limit, limit) as f32,
        (f64::from(camera.offset.y) - dy * height * 0.1).clamp(-limit, limit) as f32,
    ))
}

/// Draw and interact with a minimap inside an explicitly positioned screen rect.
///
/// `id` must be stable and unique for the view. `camera.canvas_rect` is the
/// *underlying* canvas rectangle, not the minimap rectangle. The returned
/// `offset` is already in screen pixels and must replace `MapView::offset`.
pub fn show_minimap(
    ui: &mut Ui,
    id: Id,
    rect: Rect,
    scene: &MinimapScene,
    camera: MinimapCamera,
    state: &mut MinimapState,
    labels: &MinimapLabels,
) -> MinimapOutput {
    let usable = valid_rect(rect);
    let safe_rect = if usable {
        rect
    } else {
        Rect::from_min_size(ui.max_rect().min, Vec2::ZERO)
    };
    let enabled = usable && camera.valid_zoom() && ui.is_enabled();
    let sense = if enabled {
        Sense::click_and_drag() | Sense::FOCUSABLE
    } else {
        Sense::hover()
    };
    let mut response = ui.interact(safe_rect, id, sense);
    let ctx = ui.ctx().clone();
    let (pressed, down, origin, primary_pressed, primary_down, pointer, touch_start, canceled) =
        ctx.input(|input| {
            (
                input.pointer.any_pressed(),
                input.pointer.any_down(),
                input
                    .pointer
                    .press_origin()
                    .or(input.pointer.interact_pos()),
                input.pointer.primary_pressed(),
                input.pointer.primary_down(),
                input.pointer.interact_pos(),
                input.events.iter().any(|event| {
                    matches!(
                        event,
                        Event::Touch {
                            phase: egui::TouchPhase::Start,
                            pos,
                            ..
                        } if usable && rect.contains(*pos)
                    )
                }),
                !input.focused
                    || input.events.iter().any(|event| {
                        matches!(
                            event,
                            Event::PointerGone
                                | Event::WindowFocused(false)
                                | Event::Touch {
                                    phase: egui::TouchPhase::Cancel,
                                    ..
                                }
                        )
                    }),
            )
        });
    if state.owner == PointerOwner::None && (pressed || down) {
        state.owner = if pressed && origin.is_some_and(|point| usable && rect.contains(point)) {
            PointerOwner::Minimap
        } else {
            PointerOwner::Elsewhere
        };
    }
    state.touch_captured |= touch_start;
    let blocks_canvas_pointer = state.blocks_canvas_pointer(&ctx, rect);

    if canceled
        || !enabled
        || state
            .gesture
            .is_some_and(|gesture| gesture.minimap_rect != rect || gesture.zoom != camera.zoom)
    {
        state.cancel();
    }

    let viewport = camera.viewport();
    let inset = if usable {
        (rect.width().min(rect.height()) * 0.04).min(4.0)
    } else {
        0.0
    };
    let new_transform = usable
        .then(|| Transform::new(overview_bounds(scene, viewport), rect.shrink(inset)))
        .flatten();
    let transform = state
        .gesture
        .map(|gesture| gesture.transform)
        .or(new_transform);
    let (marker, viewport_offscreen) = transform.map_or((None, false), |transform| {
        transform.viewport_marker(viewport)
    });

    if enabled
        && !state.gesture_canceled
        && state.gesture.is_none()
        && state.owner == PointerOwner::Minimap
        && ((primary_pressed && response.is_pointer_button_down_on())
            || (primary_down && response.drag_started()))
        && let (Some(transform), Some(start)) = (transform, origin)
    {
        response.request_focus();
        state.gesture = Some(Gesture {
            transform,
            minimap_rect: rect,
            zoom: camera.zoom,
            start,
            start_center: viewport.map_or_else(|| transform.world_point(start), Bounds::center),
            grabbed_viewport: !viewport_offscreen && marker.is_some_and(|r| r.contains(start)),
            dragged: false,
        });
    }

    let mut offset = None;
    if enabled && !canceled && !state.gesture_canceled {
        if let Some(gesture) = &mut state.gesture {
            if response.dragged_by(PointerButton::Primary) {
                gesture.dragged = true;
            }
            if gesture.dragged
                && (primary_down || response.drag_stopped())
                && let Some(point) = pointer
            {
                offset = offset_for_center(drag_center(*gesture, point), camera.zoom);
            }
        }
        if response.clicked()
            && let (Some(transform), Some(point)) = (transform, response.interact_pointer_pos())
        {
            offset = offset_for_center(transform.world_point(point), camera.zoom);
        }
        if offset.is_none() {
            offset = keyboard_offset(ui, &response, camera);
        }
    }
    if offset.is_some() {
        response.mark_changed();
    }

    let effective_camera = MinimapCamera {
        offset: offset.unwrap_or(camera.offset),
        ..camera
    };
    let (marker, viewport_offscreen) = transform.map_or((None, false), |transform| {
        transform.viewport_marker(effective_camera.viewport())
    });
    let accessible_name = if viewport_offscreen {
        format!("{} {}", labels.accessible_name, labels.viewport_offscreen)
    } else {
        labels.accessible_name.clone()
    };
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Canvas, enabled, &accessible_name));

    if usable {
        let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        let visuals = ui.visuals();
        painter.rect_filled(rect, 3.0, visuals.extreme_bg_color);
        if let Some(transform) = transform {
            for item in &scene.items {
                let Some(item_rect) = transform.rect(item.bounds) else {
                    continue;
                };
                let item_rect = minimum_screen_size(item_rect, transform.screen, 1.0);
                match item.kind {
                    ItemKind::Frame => {
                        painter.rect_stroke(
                            item_rect,
                            0.0,
                            Stroke::new(1.0, visuals.weak_text_color()),
                            StrokeKind::Inside,
                        );
                    }
                    ItemKind::Topic => {
                        painter.rect_filled(item_rect, 0.0, visuals.text_color());
                    }
                    ItemKind::Object => {
                        painter.rect_filled(item_rect, 0.0, visuals.selection.stroke.color);
                    }
                }
            }
            if let Some(marker) = marker {
                let color = visuals.selection.stroke.color;
                painter.rect_filled(marker, 0.0, color.gamma_multiply(0.12));
                painter.rect_stroke(
                    marker,
                    0.0,
                    Stroke::new(if viewport_offscreen { 2.0 } else { 1.5 }, color),
                    StrokeKind::Inside,
                );
            }
        }
        let border = if response.has_focus() {
            Stroke::new(2.0, visuals.selection.stroke.color)
        } else {
            visuals.widgets.noninteractive.bg_stroke
        };
        painter.rect_stroke(rect, 3.0, border, StrokeKind::Inside);
    }

    let blocks_canvas_keyboard = response.has_focus();
    let tooltip = if viewport_offscreen {
        format!("{} {}", labels.tooltip, labels.viewport_offscreen)
    } else {
        labels.tooltip.clone()
    };
    response = response.on_hover_text(tooltip).on_hover_cursor(
        if state.gesture.is_some_and(|gesture| gesture.dragged) {
            CursorIcon::Grabbing
        } else {
            CursorIcon::Grab
        },
    );

    state.finish_frame(&ctx);

    MinimapOutput {
        response,
        offset,
        blocks_canvas_pointer,
        blocks_canvas_keyboard,
        viewport_offscreen,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    use thinoda_core::{CanvasObjectId, SceneBand, TopicId, ZoneId, scene::SceneGeometry};

    fn core_rect(x: f32, y: f32, width: f32, height: f32) -> thinoda_core::Rect {
        thinoda_core::Rect::from_min_size(
            thinoda_core::Vec2::new(x, y),
            thinoda_core::Vec2::new(width, height),
        )
    }

    fn item(id: SceneId, rect: thinoda_core::Rect, clip: Option<thinoda_core::Rect>) -> SceneItem {
        SceneItem {
            id,
            band: match id {
                SceneId::Zone(_) => SceneBand::Frames,
                SceneId::Topic(_) => SceneBand::Map,
                SceneId::Object(_) => SceneBand::AboveMap,
            },
            bounds: rect,
            clip,
            geometry: SceneGeometry::Rect {
                rect,
                rotation: 0.0,
            },
        }
    }

    fn sample_scene() -> MinimapScene {
        MinimapScene::from_scene(&Scene {
            items: vec![
                item(
                    SceneId::Zone(ZoneId(1)),
                    core_rect(-1_000.0, -1_000.0, 2_000.0, 2_000.0),
                    None,
                ),
                item(
                    SceneId::Topic(TopicId(2)),
                    core_rect(-200.0, -100.0, 100.0, 60.0),
                    None,
                ),
                item(
                    SceneId::Object(CanvasObjectId(3)),
                    core_rect(200.0, 100.0, 80.0, 60.0),
                    None,
                ),
            ],
            ..Default::default()
        })
    }

    fn camera() -> MinimapCamera {
        MinimapCamera {
            canvas_rect: Rect::from_min_size(pos2(100.0, 50.0), vec2(800.0, 600.0)),
            offset: Vec2::ZERO,
            zoom: 1.0,
        }
    }

    fn close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 0.001,
            "{actual} differs from {expected}"
        );
    }

    #[test]
    fn mapping_round_trips_with_letterboxing() {
        let world = Bounds {
            min_x: -100.0,
            min_y: -50.0,
            max_x: 100.0,
            max_y: 50.0,
        };
        let target = Rect::from_min_size(pos2(10.0, 20.0), vec2(200.0, 200.0));
        let transform = Transform::new(world, target).unwrap();
        assert_eq!(
            transform.screen,
            Rect::from_min_max(pos2(10.0, 70.0), pos2(210.0, 170.0))
        );
        let point = transform.screen_point(50.0, 25.0);
        assert_eq!(point, pos2(160.0, 145.0));
        let mapped = transform.world_point(point);
        close(mapped.0, 50.0);
        close(mapped.1, 25.0);
        let clamped = transform.world_point(pos2(10.0, 0.0));
        close(clamped.0, -100.0);
        close(clamped.1, -50.0);
    }

    #[test]
    fn placement_stays_within_canvas_and_hides_when_too_small() {
        let large = Rect::from_min_size(pos2(10.0, 20.0), vec2(1_000.0, 800.0));
        let placed = bottom_right_rect(large, DEFAULT_MINIMAP_SIZE, 12.0).unwrap();
        assert_eq!(placed.size(), DEFAULT_MINIMAP_SIZE);
        assert_eq!(placed.max, pos2(998.0, 808.0));
        assert!(large.contains_rect(placed));
        let small = Rect::from_min_size(Pos2::ZERO, vec2(200.0, 150.0));
        let placed = bottom_right_rect(small, DEFAULT_MINIMAP_SIZE, 0.0).unwrap();
        assert_eq!(placed.size(), vec2(80.0, 60.0));
        assert!(small.contains_rect(placed));
        assert!(
            bottom_right_rect(
                Rect::from_min_size(Pos2::ZERO, vec2(100.0, 100.0)),
                DEFAULT_MINIMAP_SIZE,
                0.0
            )
            .is_none()
        );
        assert!(bottom_right_rect(large, DEFAULT_MINIMAP_SIZE, f32::NAN).is_none());
    }

    #[test]
    fn scene_snapshot_uses_visible_clipped_bounds_for_every_kind() {
        let clip = core_rect(0.0, 0.0, 100.0, 100.0);
        let snapshot = MinimapScene::from_scene(&Scene {
            items: vec![
                item(SceneId::Zone(ZoneId(1)), clip, None),
                item(
                    SceneId::Topic(TopicId(2)),
                    core_rect(-20.0, 20.0, 50.0, 40.0),
                    Some(clip),
                ),
                item(
                    SceneId::Object(CanvasObjectId(3)),
                    core_rect(95.0, 95.0, 20.0, 20.0),
                    Some(clip),
                ),
                item(
                    SceneId::Object(CanvasObjectId(4)),
                    core_rect(200.0, 200.0, 20.0, 20.0),
                    Some(clip),
                ),
                item(
                    SceneId::Topic(TopicId(5)),
                    core_rect(f32::NAN, 0.0, 5.0, 5.0),
                    None,
                ),
            ],
            ..Default::default()
        });
        assert_eq!(snapshot.visible_count(), 3);
        assert_eq!(snapshot.sampled_count(), 3);
        assert_eq!(
            snapshot.bounds,
            Some(Bounds {
                min_x: 0.0,
                min_y: 0.0,
                max_x: 100.0,
                max_y: 100.0,
            })
        );
        assert_eq!(
            snapshot.items[1].bounds,
            Bounds {
                min_x: 0.0,
                min_y: 20.0,
                max_x: 30.0,
                max_y: 60.0,
            }
        );
        assert_eq!(
            snapshot.items[2].bounds,
            Bounds {
                min_x: 95.0,
                min_y: 95.0,
                max_x: 100.0,
                max_y: 100.0,
            }
        );
    }

    #[test]
    fn viewport_and_pan_follow_map_view_camera_semantics() {
        let camera = MinimapCamera {
            offset: vec2(40.0, -60.0),
            zoom: 2.0,
            ..camera()
        };
        let view = camera.viewport().unwrap();
        assert_eq!(
            view,
            Bounds {
                min_x: -220.0,
                min_y: -120.0,
                max_x: 180.0,
                max_y: 180.0,
            }
        );
        assert_eq!(
            offset_for_center(view.center(), camera.zoom),
            Some(camera.offset)
        );
        let transform = Transform::new(
            Bounds {
                min_x: -100.0,
                min_y: -50.0,
                max_x: 100.0,
                max_y: 50.0,
            },
            Rect::from_min_size(pos2(0.0, 0.0), vec2(200.0, 100.0)),
        )
        .unwrap();
        let clicked = transform.world_point(transform.screen_point(50.0, -25.0));
        assert_eq!(offset_for_center(clicked, 2.0), Some(vec2(-100.0, 50.0)));
        let gesture = Gesture {
            transform,
            minimap_rect: transform.screen,
            zoom: 2.0,
            start: transform.screen_point(10.0, 10.0),
            start_center: (0.0, 0.0),
            grabbed_viewport: true,
            dragged: true,
        };
        assert_eq!(
            offset_for_center(drag_center(gesture, transform.screen_point(35.0, 0.0)), 2.0),
            Some(vec2(-50.0, 20.0))
        );
        let outside = Gesture {
            grabbed_viewport: false,
            ..gesture
        };
        assert_eq!(
            offset_for_center(drag_center(outside, transform.screen_point(35.0, 0.0)), 2.0),
            Some(vec2(-70.0, 0.0))
        );
    }

    #[test]
    fn empty_tiny_and_extreme_bounds_remain_finite() {
        let empty = MinimapScene::default();
        assert!(empty.is_empty());
        let viewport = camera().viewport().unwrap();
        let bounds = overview_bounds(&empty, Some(viewport));
        assert!(bounds.intersection(viewport).is_some());
        let tiny = Rect::from_min_size(pos2(0.0, 0.0), vec2(0.001, 0.002));
        let transform = Transform::new(bounds, tiny).unwrap();
        assert!(valid_rect(transform.screen));
        assert!(transform.viewport_marker(Some(viewport)).0.is_some());
        assert!(Transform::new(bounds, Rect::from_min_size(Pos2::ZERO, Vec2::ZERO)).is_none());
        assert!(
            MinimapCamera {
                zoom: 0.0,
                ..camera()
            }
            .viewport()
            .is_none()
        );
        assert!(
            MinimapCamera {
                canvas_rect: Rect::from_min_size(Pos2::ZERO, Vec2::ZERO),
                ..camera()
            }
            .viewport()
            .is_none()
        );

        let far = MinimapCamera {
            offset: vec2(f32::MAX, -f32::MAX),
            zoom: f32::from_bits(1),
            ..camera()
        };
        let viewport = far.viewport().unwrap();
        let scene = sample_scene();
        let bounded = overview_bounds(&scene, Some(viewport));
        assert!(bounded.width() < 20_000.0 && bounded.height() < 20_000.0);
        let transform = Transform::new(
            bounded,
            Rect::from_min_size(pos2(20.0, 20.0), vec2(200.0, 150.0)),
        )
        .unwrap();
        let (marker, outside) = transform.viewport_marker(Some(viewport));
        assert!(outside);
        assert!(transform.screen.contains_rect(marker.unwrap()));
        let recentered =
            offset_for_center(transform.world_point(transform.screen.center()), far.zoom).unwrap();
        assert!(recentered.x.is_finite() && recentered.y.is_finite());

        let extreme_empty = overview_bounds(&empty, Some(viewport));
        let transform = Transform::new(extreme_empty, tiny).unwrap();
        assert!(transform.scale.is_finite());
        assert!(transform.viewport_marker(Some(viewport)).0.is_some());
        assert!(
            offset_for_center((f64::MAX, -f64::MAX), 10.0)
                .unwrap()
                .x
                .is_finite()
        );
    }

    #[test]
    fn large_scene_has_exact_bounds_and_bounded_stratified_paint_work() {
        let mut items = vec![item(
            SceneId::Zone(ZoneId(1)),
            core_rect(-100.0, -100.0, 200.0, 200.0),
            None,
        )];
        for index in 0..4_000_u64 {
            items.push(item(
                SceneId::Topic(TopicId(index + 2)),
                core_rect(index as f32, 0.0, 1.0, 1.0),
                None,
            ));
        }
        for index in 0..9_000_u64 {
            items.push(item(
                SceneId::Object(CanvasObjectId(index + 4_002)),
                core_rect(index as f32, 500.0, 1.0, 1.0),
                None,
            ));
        }
        let snapshot = MinimapScene::from_scene(&Scene {
            items,
            ..Default::default()
        });
        assert_eq!(snapshot.visible_count(), 13_001);
        assert_eq!(snapshot.sampled_count(), MAX_MINIMAP_ITEMS);
        assert_eq!(snapshot.bounds.unwrap().max_x, 9_000.0);
        for kind in [ItemKind::Frame, ItemKind::Topic, ItemKind::Object] {
            assert!(snapshot.items.iter().any(|item| item.kind == kind));
        }
        assert_eq!(snapshot.items.first().unwrap().kind, ItemKind::Frame);
        assert_eq!(snapshot.items.last().unwrap().bounds.max_x, 9_000.0);
    }

    struct TestState {
        scene: MinimapScene,
        camera: MinimapCamera,
        minimap: MinimapState,
        rect: Rect,
        blocked: bool,
        keyboard_blocked: bool,
        canvas_presses: usize,
        canvas_releases: usize,
        visible: bool,
    }

    fn harness() -> Harness<'static, TestState> {
        let mut h = Harness::builder()
            .with_size(vec2(500.0, 400.0))
            .with_step_dt(1.0 / 60.0)
            .build_ui_state(
                |ui, state: &mut TestState| {
                    if !state.visible {
                        state.blocked = state.minimap.has_pointer_capture();
                        if !state.blocked {
                            state.canvas_presses +=
                                usize::from(ui.input(|input| input.pointer.any_pressed()));
                            state.canvas_releases +=
                                usize::from(ui.input(|input| input.pointer.any_released()));
                        }
                        state.minimap.finish_frame(ui.ctx());
                        return;
                    }
                    let preblocked = state.minimap.blocks_canvas_pointer(ui.ctx(), state.rect);
                    let output = egui::Area::new(Id::unique("test-minimap-area"))
                        .order(egui::Order::Foreground)
                        .fixed_pos(state.rect.min)
                        .movable(false)
                        .show(ui.ctx(), |overlay| {
                            overlay.set_min_size(state.rect.size());
                            show_minimap(
                                overlay,
                                Id::unique("test-minimap"),
                                state.rect,
                                &state.scene,
                                state.camera,
                                &mut state.minimap,
                                &MinimapLabels {
                                    accessible_name: "Map navigator".into(),
                                    tooltip: "Pan the map".into(),
                                    viewport_offscreen: "View outside overview".into(),
                                },
                            )
                        })
                        .inner;
                    let _underlying_canvas = ui.interact(
                        Rect::from_min_size(Pos2::ZERO, vec2(500.0, 400.0)),
                        Id::unique("test-canvas"),
                        Sense::click_and_drag(),
                    );
                    assert_eq!(preblocked, output.blocks_canvas_pointer);
                    if let Some(offset) = output.offset {
                        state.camera.offset = offset;
                    }
                    state.blocked = output.blocks_canvas_pointer;
                    state.keyboard_blocked = output.blocks_canvas_keyboard;
                    if !state.blocked {
                        state.canvas_presses += usize::from(ui.input(|i| i.pointer.any_pressed()));
                        state.canvas_releases +=
                            usize::from(ui.input(|i| i.pointer.any_released()));
                    }
                },
                TestState {
                    scene: sample_scene(),
                    camera: camera(),
                    minimap: MinimapState::default(),
                    rect: Rect::from_min_size(pos2(30.0, 30.0), vec2(220.0, 160.0)),
                    blocked: false,
                    keyboard_blocked: false,
                    canvas_presses: 0,
                    canvas_releases: 0,
                    visible: true,
                },
            );
        h.run_steps(3);
        h
    }

    fn click(h: &mut Harness<'_, TestState>, point: Pos2) {
        h.hover_at(point);
        for pressed in [true, false] {
            h.event(Event::PointerButton {
                pos: point,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            });
            h.run_steps(1);
        }
    }

    #[test]
    fn click_pans_without_starting_or_releasing_canvas_gestures() {
        let mut h = harness();
        h.get_by_label("Map navigator");
        click(&mut h, pos2(200.0, 110.0));
        assert!(h.state().camera.offset.x < -100.0);
        assert_eq!(h.state().canvas_presses, 0);
        assert_eq!(h.state().canvas_releases, 0);
        assert!(h.state().blocked);
    }

    #[test]
    fn dragging_viewport_remains_captured_outside_the_minimap() {
        let mut h = harness();
        let start = h.state().rect.center();
        let end = pos2(320.0, start.y);
        h.hover_at(start);
        h.drag_at(start);
        h.run_steps(1);
        h.hover_at(end);
        h.run_steps(2);
        assert!(h.state().camera.offset.x < 0.0);
        assert!(h.state().blocked);
        let held_offset = h.state().camera.offset;
        h.run_steps(3);
        assert_eq!(h.state().camera.offset, held_offset);
        h.drop_at(end);
        h.run_steps(1);
        assert_eq!(h.state().canvas_presses, 0);
        assert_eq!(h.state().canvas_releases, 0);
        assert!(!h.state().blocked);
    }

    #[test]
    fn canvas_drag_started_elsewhere_is_not_stolen_by_the_overlay() {
        let mut h = harness();
        let outside = pos2(350.0, 110.0);
        let inside = h.state().rect.center();
        h.hover_at(outside);
        h.drag_at(outside);
        h.run_steps(1);
        assert!(!h.state().blocked);
        h.hover_at(inside);
        h.run_steps(2);
        assert!(!h.state().blocked);
        h.drop_at(inside);
        h.run_steps(1);
        assert!(!h.state().blocked);
        assert_eq!(h.state().canvas_presses, 1);
        assert_eq!(h.state().canvas_releases, 1);
    }

    #[test]
    fn showing_overlay_mid_drag_does_not_claim_an_existing_pointer() {
        let mut h = harness();
        h.state_mut().visible = false;
        h.run_steps(1);
        let point = h.state().rect.center();
        h.hover_at(point);
        h.drag_at(point);
        h.run_steps(1);
        h.state_mut().visible = true;
        h.hover_at(point + vec2(30.0, 0.0));
        h.run_steps(2);
        assert!(!h.state().blocked);
        assert_eq!(h.state().camera.offset, Vec2::ZERO);
        h.drop_at(point + vec2(30.0, 0.0));
        h.run_steps(1);
        assert!(!h.state().blocked);
        assert_eq!(h.state().canvas_presses, 1);
        assert_eq!(h.state().canvas_releases, 1);
    }

    #[test]
    fn canceled_press_does_not_pan_or_leak_a_release() {
        let mut h = harness();
        let point = pos2(200.0, 110.0);
        h.hover_at(point);
        h.drag_at(point);
        h.run_steps(1);
        assert_eq!(h.state().minimap.owner, PointerOwner::Minimap);
        h.state_mut().minimap.cancel();
        h.drop_at(point);
        h.run_steps(1);
        assert_eq!(h.state().camera.offset, Vec2::ZERO);
        assert_eq!(h.state().canvas_presses, 0);
        assert_eq!(h.state().canvas_releases, 0);
        click(&mut h, point);
        assert!(h.state().camera.offset.x < -100.0);
    }

    #[test]
    fn hiding_mid_gesture_keeps_release_blocked_then_retires_capture() {
        let mut h = harness();
        let start = h.state().rect.center();
        let end = pos2(320.0, start.y);
        h.hover_at(start);
        h.drag_at(start);
        h.run_steps(1);
        assert!(h.state().minimap.has_pointer_capture());
        h.state_mut().minimap.cancel();
        h.state_mut().visible = false;
        h.hover_at(end);
        h.run_steps(1);
        assert!(h.state().blocked);
        h.drop_at(end);
        h.run_steps(1);
        assert_eq!(h.state().canvas_releases, 0);
        assert!(!h.state().minimap.has_pointer_capture());
        click(&mut h, start);
        assert_eq!(h.state().canvas_presses, 1);
        assert_eq!(h.state().canvas_releases, 1);
    }

    fn touch(h: &mut Harness<'_, TestState>, phase: egui::TouchPhase, position: Pos2) {
        h.event(Event::PointerMoved(position));
        h.event(Event::Touch {
            device_id: egui::TouchDeviceId(1),
            id: egui::TouchId(1),
            phase,
            pos: position,
            force: None,
        });
        if matches!(phase, egui::TouchPhase::Start | egui::TouchPhase::End) {
            h.event(Event::PointerButton {
                pos: position,
                button: PointerButton::Primary,
                pressed: phase == egui::TouchPhase::Start,
                modifiers: Modifiers::NONE,
            });
        }
        h.run_steps(1);
    }

    #[test]
    fn touch_drag_stays_captured_outside_until_release() {
        let mut h = harness();
        let start = h.state().rect.center();
        let end = pos2(320.0, start.y);
        touch(&mut h, egui::TouchPhase::Start, start);
        touch(&mut h, egui::TouchPhase::Move, end);
        assert!(h.state().blocked);
        assert!(h.state().camera.offset.x < 0.0);
        touch(&mut h, egui::TouchPhase::End, end);
        assert_eq!(h.state().canvas_presses, 0);
        assert_eq!(h.state().canvas_releases, 0);
        h.run_steps(1);
        assert!(!h.state().blocked);
    }

    #[test]
    fn canceled_touch_cannot_restart_or_fall_through() {
        let mut h = harness();
        let start = h.state().rect.center();
        let end = pos2(200.0, start.y);
        touch(&mut h, egui::TouchPhase::Start, start);
        touch(&mut h, egui::TouchPhase::Move, end);
        let before_cancel = h.state().camera.offset;
        touch(&mut h, egui::TouchPhase::Cancel, end);
        h.hover_at(pos2(230.0, start.y));
        h.run_steps(2);
        assert_eq!(h.state().camera.offset, before_cancel);
        h.drop_at(pos2(230.0, start.y));
        h.run_steps(1);
        assert_eq!(h.state().canvas_presses, 0);
        assert_eq!(h.state().canvas_releases, 0);
    }

    #[test]
    fn distant_view_can_be_recentered_and_announces_its_status() {
        let mut h = harness();
        h.state_mut().camera.offset = vec2(1.0e20, -1.0e20);
        h.run_steps(2);
        h.get_by_label("Map navigator View outside overview");
        let point = h.state().rect.center();
        click(&mut h, point);
        let offset = h.state().camera.offset;
        assert!(offset.x.is_finite() && offset.y.is_finite());
        assert!(offset.x.abs() < 10_000.0 && offset.y.abs() < 10_000.0);
    }

    #[test]
    fn accessible_focus_supports_keyboard_pan() {
        let mut h = harness();
        h.get_by_label("Map navigator").focus();
        h.run_steps(1);
        assert!(h.state().keyboard_blocked);
        h.key_press(Key::ArrowRight);
        h.run_steps(1);
        assert_eq!(h.state().camera.offset, vec2(-80.0, 0.0));
    }

    #[test]
    fn paints_scene_and_viewport_in_light_and_dark_themes() {
        for dark in [false, true] {
            let mut h = harness();
            let visuals = if dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            };
            let background = visuals.extreme_bg_color;
            let topic = visuals.text_color();
            let object = visuals.selection.stroke.color;
            let viewport_fill = object.gamma_multiply(0.12);
            h.ctx.set_visuals(visuals);
            h.run_steps(2);
            let widget_rect = h.state().rect;
            let contains_fill = |fill| {
                h.output().shapes.iter().any(|shape| {
                    shape.clip_rect == widget_rect
                        && matches!(&shape.shape, egui::Shape::Rect(rect) if rect.fill == fill)
                })
            };
            assert!(contains_fill(background));
            assert!(contains_fill(topic));
            assert!(contains_fill(object));
            assert!(contains_fill(viewport_fill));
            assert!(h.output().shapes.iter().any(|shape| {
                shape.clip_rect == widget_rect
                    && matches!(&shape.shape, egui::Shape::Rect(rect)
                        if rect.fill == egui::Color32::TRANSPARENT && rect.stroke.width > 0.0)
            }));
        }
    }

    #[test]
    fn tiny_widget_and_empty_scene_can_be_shown() {
        let mut h = harness();
        h.state_mut().scene = MinimapScene::default();
        h.state_mut().rect = Rect::from_min_size(pos2(30.0, 30.0), vec2(0.001, 0.001));
        h.run_steps(2);
        h.state_mut().rect = Rect::from_min_size(pos2(30.0, 30.0), Vec2::ZERO);
        h.run_steps(2);
        assert_eq!(h.state().camera.offset, Vec2::ZERO);
    }
}
