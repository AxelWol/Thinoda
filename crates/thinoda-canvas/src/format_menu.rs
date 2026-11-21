//! Shared geometry and navigation for selection-scoped formatting.

use egui::{Context, Id, Rect, Response, Ui, Vec2, pos2, vec2};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Section {
    Actions,
    #[default]
    Text,
    Style,
    Arrange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Producer {
    Card,
    Label,
    Selection,
}

pub fn claim(ctx: &Context, producer: Producer) -> bool {
    let id = area_id(ctx).with("producer");
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|data| {
        if data
            .get_temp::<(u64, Producer)>(id)
            .is_some_and(|(previous, owner)| previous == pass && owner != producer)
        {
            return false;
        }
        data.insert_temp(id, (pass, producer));
        true
    })
}

pub fn claimed(ctx: &Context) -> bool {
    let id = area_id(ctx).with("producer");
    let pass = ctx.cumulative_pass_nr();
    ctx.data(|data| data.get_temp::<(u64, Producer)>(id))
        .is_some_and(|(previous, _)| previous == pass)
}

/// Whether this pass right-clicked the nearby selection menu.
///
/// The menu may cover part of a small selection; right-clicking it then opens
/// the selection's full context menu, as right-clicking the object would.
/// Text-editing producers keep their own pointer handling.
pub fn secondary_clicked_selection_menu(ctx: &Context) -> bool {
    let Some(pos) = ctx.input(|input| {
        input
            .pointer
            .secondary_clicked()
            .then(|| input.pointer.interact_pos())
            .flatten()
    }) else {
        return false;
    };
    let id = area_id(ctx);
    let layer = egui::LayerId::new(ORDER, id);
    let selection = ctx
        .data(|data| data.get_temp::<(u64, Producer)>(id.with("producer")))
        .is_some_and(|(_, producer)| producer == Producer::Selection);
    selection
        && ctx.memory(|memory| {
            memory.areas().visible_last_frame(&layer)
                && memory.area_rect(id).is_some_and(|rect| rect.contains(pos))
        })
        && ctx.layer_id_at(pos) == Some(layer)
}
fn measure_id(ctx: &Context, owner: Id, section: Section, width: f32) -> Id {
    area_id(ctx).with(("measure", owner, section, width.to_bits()))
}

pub fn desired_size(ctx: &Context, owner: Id, section: Section, width: f32) -> Vec2 {
    let id = measure_id(ctx, owner, section, width);
    let height = ctx.data(|data| data.get_temp::<f32>(id)).unwrap_or(170.0);
    vec2(324.0, height)
}

pub fn has_measurement(ctx: &Context, owner: Id, section: Section, width: f32) -> bool {
    let id = measure_id(ctx, owner, section, width);
    ctx.data(|data| data.get_temp::<f32>(id).is_some())
}

pub fn measured(ctx: &Context, owner: Id, section: Section, width: f32, expected: f32, rect: Rect) {
    let id = measure_id(ctx, owner, section, width);
    ctx.data_mut(|data| data.insert_temp(id, rect.height()));
    if (expected - rect.height()).abs() > 0.5 {
        ctx.request_repaint();
    }
}

/// The nearby menu is a persistent tool surface like the old toolbars: it sits
/// below auxiliary windows and every Foreground popup, including the canvas
/// context menu opened by the same right-click that selects an object.
pub const ORDER: egui::Order = egui::Order::Middle;

pub fn area_id(ctx: &Context) -> Id {
    Id::unique(("thinoda_selection_menu", ctx.viewport_id()))
}

pub fn compact_open(ctx: &Context) -> bool {
    let id = area_id(ctx).with("compact_popup").with("open");
    ctx.data(|data| data.get_temp::<bool>(id)).unwrap_or(false)
}

pub fn set_compact_open(ctx: &Context, open: bool) {
    let id = area_id(ctx).with("compact_popup").with("open");
    ctx.data_mut(|data| data.insert_temp(id, open));
}

pub fn prepare(ctx: &Context, owner: Id) {
    let id = area_id(ctx).with("owner");
    let previous = ctx.data(|data| data.get_temp::<Id>(id));
    // A right-click both selects and opens the canvas context menu; keep it.
    let canvas_menu = egui::Popup::is_id_open(ctx, crate::MapCanvas::id().with("popup"));
    if previous.is_some_and(|previous| previous != owner) {
        set_compact_open(ctx, false);
        if !canvas_menu {
            egui::Popup::close_all(ctx);
        }
    }
    ctx.data_mut(|data| data.insert_temp(id, owner));
}

fn slot(ctx: &Context) -> Id {
    Id::unique(("thinoda_object_format_section", ctx.viewport_id()))
}

pub fn section(ctx: &Context, owner: Id, default: Section) -> Section {
    let id = slot(ctx);
    ctx.data(|data| data.get_temp::<(Id, Section)>(id))
        .filter(|(stored, _)| *stored == owner)
        .map_or(default, |(_, section)| section)
}

pub fn set_section(ctx: &Context, owner: Id, section: Section) {
    let id = slot(ctx);
    ctx.data_mut(|data| data.insert_temp(id, (owner, section)));
}

pub fn owner(doc: &thinoda_core::Document, view: &crate::MapView) -> Id {
    Id::unique((
        "thinoda_object_format_owner",
        doc.instance_id(),
        doc.active_index(),
        doc.sheet().root,
        &view.selection,
        &view.board_selection,
        view.selected_object,
    ))
}

pub fn tabs(
    ui: &mut Ui,
    selected: &mut Section,
    sections: &[(Section, &str)],
    enabled: bool,
) -> Vec<Response> {
    let mut responses = Vec::new();
    // Every producer shows the same tab order, whatever order it lists them in.
    let mut sections = sections.to_vec();
    sections.sort_by_key(|(section, _)| *section as u8);
    ui.horizontal_wrapped(|ui| {
        for (section, label) in sections {
            let response = ui.add_enabled(
                enabled,
                egui::Button::selectable(*selected == section, label),
            );
            if enabled && ui.is_enabled() && response.clicked() {
                if *selected != section {
                    egui::Popup::close_all(ui.ctx());
                }
                *selected = section;
            }
            responses.push(response);
        }
    });
    responses
}

/// Prefer a nearby unobstructed side; clamp even for oversized/offscreen selections.
pub fn place(canvas: Rect, anchor: Rect, desired: Vec2, avoid: &[Rect]) -> Rect {
    let safe = canvas.shrink(6.0);
    let size = desired.min(safe.size().max(vec2(1.0, 1.0)));
    let mut candidates = vec![
        pos2(anchor.center().x - size.x / 2.0, anchor.bottom() + 10.0),
        pos2(
            anchor.center().x - size.x / 2.0,
            anchor.top() - size.y - 10.0,
        ),
        pos2(anchor.right() + 10.0, anchor.center().y - size.y / 2.0),
        pos2(
            anchor.left() - size.x - 10.0,
            anchor.center().y - size.y / 2.0,
        ),
        safe.min,
        safe.max - size,
    ];
    for obstacle in avoid {
        candidates.extend([
            pos2(obstacle.left() - size.x - 6.0, obstacle.top()),
            pos2(obstacle.right() + 6.0, obstacle.top()),
            pos2(obstacle.left(), obstacle.top() - size.y - 6.0),
            pos2(obstacle.left(), obstacle.bottom() + 6.0),
        ]);
    }
    let mut best = Rect::from_min_size(safe.min, size);
    let mut best_overlap = f32::INFINITY;
    for candidate in candidates {
        let min = candidate.clamp(safe.min, (safe.max - size).max(safe.min));
        let rect = Rect::from_min_size(min, size);
        let overlap = std::iter::once(&anchor)
            .chain(avoid)
            .map(|other| {
                let intersection = rect.intersect(other.expand(4.0));
                if intersection.is_positive() {
                    intersection.area()
                } else {
                    0.0
                }
            })
            .sum::<f32>()
            + if covers_target(rect, anchor, canvas) {
                1.0e9
            } else {
                0.0
            };
        if overlap < best_overlap {
            best = rect;
            best_overlap = overlap;
        }
        if overlap == 0.0 {
            break;
        }
    }
    best
}

/// Whether `menu` covers the visible centre of the selection, where pointer
/// gestures such as right-click and double-click are aimed.
pub fn covers_target(menu: Rect, anchor: Rect, canvas: Rect) -> bool {
    let visible = anchor.intersect(canvas);
    visible.is_positive() && menu.contains(visible.center())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_handover_allows_only_one_producer_per_pass() {
        let ctx = Context::default();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            let ctx = ui.ctx();
            assert!(claim(ctx, Producer::Label));
            assert!(!claim(ctx, Producer::Selection));
            assert!(claim(ctx, Producer::Label));
        });
        output.textures_delta.clear();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            let ctx = ui.ctx();
            assert!(claim(ctx, Producer::Selection));
            assert!(!claim(ctx, Producer::Card));
        });
        output.textures_delta.clear();
    }

    #[test]
    fn bounded_placement_handles_corners_zoomed_and_offscreen_selections() {
        for width in [320.0, 640.0, 1280.0] {
            let canvas = Rect::from_min_size(pos2(0.0, 60.0), vec2(width, 420.0));
            for center in [
                canvas.left_top(),
                canvas.right_top(),
                canvas.left_bottom(),
                canvas.right_bottom(),
                canvas.center(),
                pos2(-500.0, -500.0),
            ] {
                for size in [vec2(80.0, 40.0), vec2(2000.0, 1800.0)] {
                    let rect = place(
                        canvas,
                        Rect::from_center_size(center, size),
                        vec2(324.0, 160.0),
                        &[],
                    );
                    assert!(canvas.contains_rect(rect), "{canvas:?} {rect:?}");
                }
            }
        }
    }

    #[test]
    fn placement_avoids_the_object_and_toolbox_when_space_exists() {
        let canvas = Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 700.0));
        let anchor = Rect::from_center_size(canvas.center(), vec2(120.0, 80.0));
        let toolbox = Rect::from_min_size(pos2(10.0, 80.0), vec2(50.0, 450.0));
        let menu = place(canvas, anchor, vec2(324.0, 160.0), &[toolbox]);
        assert!(!menu.intersects(anchor));
        assert!(!menu.intersects(toolbox));
    }

    #[test]
    fn placement_keeps_the_selection_centre_free_when_full_avoidance_fails() {
        let canvas = Rect::from_min_size(pos2(0.0, 0.0), vec2(420.0, 200.0));
        let anchor = Rect::from_min_size(pos2(300.0, 70.0), vec2(100.0, 60.0));
        let menu = place(canvas, anchor, vec2(324.0, 180.0), &[]);
        assert!(menu.intersects(anchor));
        assert!(!covers_target(menu, anchor, canvas), "{menu:?}");

        let centred = Rect::from_center_size(pos2(320.0, 120.0), vec2(100.0, 30.0));
        let canvas = Rect::from_min_size(pos2(0.0, 70.0), vec2(640.0, 150.0));
        let menu = place(canvas, centred, vec2(324.0, 108.0), &[]);
        assert!(covers_target(menu, centred, canvas));
    }
}
