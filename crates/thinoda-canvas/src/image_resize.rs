//! Image resizing is view-only until an owned primary-pointer release.
use super::*;
use thinoda_core::images::{MAX_DISPLAY_SIZE, valid_size};

pub(super) struct Gesture {
    topic: TopicId,
    root: TopicId,
    state: u64,
    sheet: usize,
    camera: (EVec2, f32, Rect),
    geometry: Geometry,
    reference: String,
    grab: Pos2,
    size: Vec2,
    preview: Vec2,
    dragged: bool,
    canceled: bool,
}

#[derive(Clone, Copy, PartialEq)]
struct Geometry {
    topic: TopicId,
    image: thinoda_core::Rect,
    pivot: Vec2,
    rotation: f32,
    clip: Option<thinoda_core::Rect>,
}

/// Project movement onto the original diagonal. Normalizing first avoids
/// underflow for tiny imported sizes; both axes participate at every zoom.
fn resized(size: Vec2, delta: Vec2) -> Vec2 {
    let longest = size.x.max(size.y);
    let unit = Vec2::new(size.x / longest, size.y / longest);
    let movement = (delta.x * unit.x + delta.y * unit.y) / (unit.x * unit.x + unit.y * unit.y);
    if movement == 0.0 {
        return size;
    }
    let length = (longest + movement).clamp(longest.min(1.0), MAX_DISPLAY_SIZE);
    unit * length
}

fn geometry(view: &MapView, sheet: &Sheet, layout: &Layout) -> Option<Geometry> {
    if view.selection.len() != 1
        || view.selected_object.is_some()
        || view.editing.is_some()
        || view.relationship_from.is_some()
        || view.relationship_drag.is_some()
        || !matches!(view.drag, Drag::None)
    {
        return None;
    }
    let id = view.primary()?;
    let topic = sheet.topic(id)?;
    // Legacy path/URL images have no supported preview or resize interaction.
    sheet.resources.get(topic.image.as_deref()?).ok()?;
    let layout = layout.topics.get(&id)?;
    let image = layout.image_rect?;
    if !valid_size(image.size) {
        return None;
    }
    let geometry = Geometry {
        topic: id,
        image,
        pivot: layout.rect.center(),
        rotation: topic.card.map_or(0.0, |card| card.rotation),
        clip: topic_clip_rect(sheet, id),
    };
    let corner = image_corner(geometry, image);
    geometry
        .clip
        .is_none_or(|clip| clip.contains(corner))
        .then_some(geometry)
}

fn image_corner(geometry: Geometry, image: thinoda_core::Rect) -> Vec2 {
    if geometry.rotation == 0.0 {
        image.max()
    } else {
        rotate_around(image.max(), geometry.pivot, geometry.rotation)
    }
}

fn handle_rect(view: &MapView, geometry: Geometry, image: thinoda_core::Rect) -> Rect {
    // Constant screen-space target, including when the map is zoomed out.
    Rect::from_center_size(
        view.xf().pos(image_corner(geometry, image)),
        vec2(28.0, 28.0),
    )
}

fn handle_visible(view: &MapView, geometry: Geometry, rect: Rect) -> bool {
    view.canvas_rect.contains_rect(rect)
        && geometry
            .clip
            .is_none_or(|clip| view.xf().rect(clip).contains_rect(rect))
}

fn resize_cursor(geometry: Geometry) -> egui::CursorIcon {
    if geometry.rotation == 0.0 {
        return egui::CursorIcon::ResizeNwSe;
    }
    let angle = (geometry
        .image
        .size
        .y
        .atan2(geometry.image.size.x)
        .to_degrees()
        + geometry.rotation)
        .rem_euclid(180.0);
    if !(22.5..157.5).contains(&angle) {
        egui::CursorIcon::ResizeHorizontal
    } else if angle < 67.5 {
        egui::CursorIcon::ResizeNwSe
    } else if angle < 112.5 {
        egui::CursorIcon::ResizeVertical
    } else {
        egui::CursorIcon::ResizeNeSw
    }
}

pub(super) fn cursor(view: &MapView, sheet: &Sheet, layout: &Layout) -> egui::CursorIcon {
    geometry(view, sheet, layout).map_or(egui::CursorIcon::ResizeNwSe, resize_cursor)
}

fn handle_id(topic: TopicId) -> Id {
    MapCanvas::id().with(("image-resize", topic))
}

fn commit(doc: &mut Document, topic: TopicId, size: Vec2) -> Result<(), thinoda_core::EditError> {
    if !valid_size(size) {
        return Ok(());
    }
    let current = doc.sheet().topic(topic).and_then(|t| t.image_size);
    if current == Some(size) {
        return Ok(());
    }
    doc.edit_sheet("Resize image", |sheet| {
        let topic = sheet
            .topics
            .get_mut(&topic)
            .ok_or(thinoda_core::EditError::NotFound)?;
        topic.image_size = Some(size);
        Ok(())
    })
}

pub(super) fn interact(
    ui: &mut Ui,
    doc: &mut Document,
    view: &mut MapView,
    layout: &Layout,
    error: &mut Option<thinoda_core::EditError>,
    enabled: bool,
) -> bool {
    let ctx = ui.ctx().clone();
    let enabled = enabled && ui.is_enabled() && !egui::Popup::is_any_open(&ctx);
    let geometry = geometry(view, doc.sheet(), layout);
    let mut active = view.image_resize.take();
    let was_active = active.is_some();
    if let Some(drag) = &mut active {
        drag.canceled |= !enabled
            || !ctx.input(|i| i.focused)
            || !ctx.memory(|m| m.has_focus(handle_id(drag.topic)))
            || drag.state != doc.state_id()
            || drag.sheet != doc.active_index()
            || drag.root != doc.sheet().root
            || geometry != Some(drag.geometry)
            || doc
                .sheet()
                .topic(drag.topic)
                .and_then(|t| t.image.as_deref())
                != Some(drag.reference.as_str())
            || drag.camera != (view.offset, view.zoom, view.canvas_rect)
            || ctx.input(|i| {
                i.events.iter().any(|e| {
                    matches!(
                        e,
                        egui::Event::PointerGone
                            | egui::Event::WindowFocused(false)
                            | egui::Event::Touch {
                                phase: egui::TouchPhase::Cancel,
                                ..
                            }
                    )
                })
            })
            || ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
    }
    let Some(geometry) = geometry.filter(|_| enabled) else {
        // Keep ownership after cancellation until release, so a held pointer
        // cannot become a topic drag or a click on a different selection.
        view.image_resize = active.filter(|_| ctx.input(|i| i.pointer.primary_down()));
        return was_active;
    };
    let topic = geometry.topic;
    let image = geometry.image;
    let rect = handle_rect(view, geometry, image);
    if active.is_none() && !handle_visible(view, geometry, rect) {
        return false;
    }
    let response = ui.interact(
        rect,
        handle_id(topic),
        Sense::click_and_drag() | Sense::FOCUSABLE,
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Button, enabled, &view.image_resize_label)
    });
    let blocked = was_active
        || response.hovered()
        || response.is_pointer_button_down_on()
        || ctx
            .input(|i| i.pointer.press_origin())
            .is_some_and(|p| rect.contains(p));
    if response.is_pointer_button_down_on() && ctx.input(|i| i.pointer.primary_pressed()) {
        response.request_focus();
        active = Some(Gesture {
            topic,
            root: doc.sheet().root,
            state: doc.state_id(),
            sheet: doc.active_index(),
            camera: (view.offset, view.zoom, view.canvas_rect),
            geometry,
            reference: doc
                .sheet()
                .topic(topic)
                .and_then(|t| t.image.clone())
                .expect("resize geometry has an embedded image"),
            grab: ctx
                .input(|i| i.pointer.press_origin())
                .unwrap_or(rect.center()),
            size: image.size,
            preview: image.size,
            dragged: false,
            canceled: false,
        });
    }
    if response.has_focus() && active.is_none() {
        ctx.memory_mut(|m| {
            m.set_focus_lock_filter(
                response.id,
                EventFilter {
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    escape: true,
                    ..Default::default()
                },
            )
        });
        'keys: for (modifiers, step) in [(Modifiers::SHIFT, 10.0), (Modifiers::NONE, 1.0)] {
            for (key, sign) in [
                (Key::ArrowRight, 1.0),
                (Key::ArrowDown, 1.0),
                (Key::ArrowLeft, -1.0),
                (Key::ArrowUp, -1.0),
            ] {
                if ctx.input_mut(|i| i.consume_key(modifiers, key)) {
                    let longest = image.size.x.max(image.size.y);
                    let delta =
                        Vec2::new(image.size.x / longest, image.size.y / longest) * (sign * step);
                    let size = resized(image.size, delta);
                    if let Err(e) = commit(doc, topic, size) {
                        *error = Some(e);
                    }
                    break 'keys;
                }
            }
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            ctx.memory_mut(|m| m.request_focus(MapCanvas::id()));
        }
    }
    if let Some(mut drag) = active {
        drag.dragged |= response.dragged_by(PointerButton::Primary);
        if !drag.canceled
            && let Some(point) = ctx.pointer_latest_pos()
        {
            let delta = (point - drag.grab) / drag.camera.1;
            let delta = Vec2::new(delta.x, delta.y);
            let local = if drag.geometry.rotation == 0.0 {
                delta
            } else {
                rotate_around(delta, Vec2::ZERO, -drag.geometry.rotation)
            };
            drag.preview = resized(drag.size, local);
        }
        if ctx.input(|i| i.pointer.primary_released()) {
            if !drag.canceled
                && drag.dragged
                && ctx
                    .pointer_latest_pos()
                    .is_some_and(|p| view.canvas_rect.contains(p))
                && drag.preview != drag.size
                && let Err(e) = commit(doc, drag.topic, drag.preview)
            {
                *error = Some(e);
            }
        } else if ctx.input(|i| i.pointer.primary_down()) {
            view.image_resize = Some(drag);
        }
    }
    response
        .on_hover_text(&view.image_resize_label)
        .on_hover_cursor(resize_cursor(geometry));
    blocked
}

pub(super) fn paint(
    painter: &egui::Painter,
    view: &MapView,
    sheet: &Sheet,
    layout: &Layout,
    enabled: bool,
) {
    if !enabled {
        return;
    }
    let Some(geometry) = geometry(view, sheet, layout) else {
        return;
    };
    let topic = geometry.topic;
    let image = geometry.image;
    let image = if let Some(drag) = &view.image_resize {
        if drag.canceled {
            return;
        }
        thinoda_core::Rect::from_min_size(drag.geometry.image.min, drag.preview)
    } else {
        image
    };
    let target = handle_rect(view, geometry, image);
    if view.image_resize.is_none() && !handle_visible(view, geometry, target) {
        return;
    }
    let clipped = geometry
        .clip
        .map(|clip| painter.with_clip_rect(view.xf().rect(clip)));
    let painter = clipped.as_ref().unwrap_or(painter);
    let stroke = Stroke::new(2.0, view.accent);
    if geometry.rotation == 0.0 {
        painter.rect_stroke(view.xf().rect(image), 0.0, stroke, StrokeKind::Outside);
    } else {
        let min = image.min;
        let max = image.max();
        let points = [min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)].map(|point| {
            view.xf()
                .pos(rotate_around(point, geometry.pivot, geometry.rotation))
        });
        painter.add(egui::Shape::closed_line(points.to_vec(), stroke));
    }
    if !handle_visible(view, geometry, target) {
        return;
    }
    let corner = target.center();
    painter.rect_filled(
        Rect::from_center_size(corner, vec2(10.0, 10.0)),
        2.0,
        view.accent,
    );
    painter.rect_stroke(
        Rect::from_center_size(corner, vec2(10.0, 10.0)),
        2.0,
        Stroke::new(1.0, Color32::WHITE),
        StrokeKind::Inside,
    );
    if painter.ctx().memory(|m| m.has_focus(handle_id(topic))) {
        painter.rect_stroke(target.shrink(2.0), 3.0, stroke, StrokeKind::Inside);
    }
}

#[cfg(test)]
#[path = "image_resize_tests.rs"]
mod tests;
