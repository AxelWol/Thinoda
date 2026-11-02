use egui::{Key, Modifiers, PointerButton, Pos2, Rect, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_canvas::{
    BoardTool, MapCanvas, MapView,
    minimap::{DEFAULT_MINIMAP_SIZE, bottom_right_rect},
};
use thinoda_core::{
    CanvasObject, CanvasObjectId, Document, ShapeKind, Sheet, SheetRootMode, TopicId, Vec2,
    Workbook,
};

struct CanvasState {
    doc: Document,
    view: MapView,
    card: TopicId,
    shape: CanvasObjectId,
    enabled: bool,
    snapshot: bool,
    preblocks_pointer: bool,
    preblocks_keyboard: bool,
    minimap_blocks_pointer: bool,
    errors: Vec<String>,
}

fn canvas_harness() -> Harness<'static, CanvasState> {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let card = sheet.add_floating("Floating card", Vec2::ZERO);
    let shape = sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            thinoda_core::Rect::from_center_size(Vec2::new(220.0, 100.0), Vec2::new(120.0, 80.0)),
        ))
        .unwrap();
    let mut view = MapView::default();
    view.set_minimap_visible(true);
    view.minimap_labels.accessible_name = "Board navigator".into();
    view.minimap_labels.tooltip = "Move the board view".into();
    view.minimap_labels.viewport_offscreen = "View outside overview".into();
    let mut harness = Harness::builder()
        .with_size(vec2(640.0, 480.0))
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(
            |ui, state: &mut CanvasState| {
                let _ = ui.button("Before canvas");
                let size = vec2(
                    ui.available_width(),
                    (ui.available_height() - 32.0).max(0.0),
                );
                ui.allocate_ui(size, |ui| {
                    ui.add_enabled_ui(state.enabled, |ui| {
                        if state.snapshot {
                            MapCanvas::show_snapshot(ui, &state.doc, &mut state.view);
                        } else {
                            state.preblocks_pointer = state.view.minimap_blocks_pointer(ui);
                            state.preblocks_keyboard = state.view.minimap_blocks_keyboard(ui);
                            let output = MapCanvas::show(ui, &mut state.doc, &mut state.view);
                            state.minimap_blocks_pointer = output.minimap_blocks_pointer;
                            output.context_menu(|ui| {
                                ui.label("Canvas context menu");
                            });
                            if let Some(error) = output.error {
                                state.errors.push(format!("{error:?}"));
                            }
                        }
                    });
                });
                let _ = ui.button("After canvas");
            },
            CanvasState {
                doc: Document::new(Workbook::new(sheet)),
                view,
                card,
                shape,
                enabled: true,
                snapshot: false,
                preblocks_pointer: false,
                preblocks_keyboard: false,
                minimap_blocks_pointer: false,
                errors: Vec::new(),
            },
        );
    harness.run_steps(4);
    harness
}

fn minimap_rect(harness: &Harness<'_, CanvasState>) -> Rect {
    bottom_right_rect(
        harness.state().view.canvas_rect(),
        DEFAULT_MINIMAP_SIZE,
        12.0,
    )
    .unwrap()
}

fn put_world_at(harness: &mut Harness<'_, CanvasState>, world: Vec2, target: Pos2) {
    let state = harness.state_mut();
    state.view.offset =
        target - state.view.canvas_rect().center() - vec2(world.x, world.y) * state.view.zoom;
    harness.run_steps(3);
}

fn click(harness: &mut Harness<'_, CanvasState>, point: Pos2, button: PointerButton) {
    harness.hover_at(point);
    harness.run_steps(1);
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos: point,
            button,
            pressed,
            modifiers: Modifiers::NONE,
        });
        harness.run_steps(1);
    }
}

fn touch(harness: &mut Harness<'_, CanvasState>, id: u64, phase: egui::TouchPhase, pos: Pos2) {
    harness.event(egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(id),
        phase,
        pos,
        force: None,
    });
}

#[test]
fn host_toggle_and_localized_accessible_name_do_not_affect_snapshots() {
    assert!(!MapView::default().show_minimap);
    let mut h = canvas_harness();
    h.get_by_label("Board navigator");

    h.get_by_label("Board navigator").focus();
    h.run_steps(1);
    let minimap_id = MapCanvas::id().with("minimap");
    assert_eq!(h.ctx.memory(|memory| memory.focused()), Some(minimap_id));
    h.state_mut().view.set_minimap_visible(false);
    h.run_steps(3);
    assert!(h.query_by_label("Board navigator").is_none());
    assert_ne!(h.ctx.memory(|memory| memory.focused()), Some(minimap_id));
    h.state_mut().view.set_minimap_visible(true);
    h.run_steps(3);
    h.get_by_label("Board navigator");

    h.state_mut().view.offset = vec2(100_000.0, 100_000.0);
    h.run_steps(2);
    h.get_by_label("Board navigator View outside overview");
    h.state_mut().snapshot = true;
    h.run_steps(3);
    assert!(
        h.query_by_label("Board navigator View outside overview")
            .is_none()
    );
}

#[test]
fn clicking_over_a_topic_pans_without_selecting_or_editing_the_topic() {
    let mut h = canvas_harness();
    let target = minimap_rect(&h).center();
    let world = {
        let state = h.state_mut();
        state
            .view
            .layout_for(&state.doc)
            .rect(state.card)
            .unwrap()
            .center()
    };
    put_world_at(&mut h, world, target);
    let offset = h.state().view.offset;
    let revision = h.state().doc.state_id();
    click(&mut h, target, PointerButton::Primary);
    assert_ne!(h.state().view.offset, offset);
    assert!(h.state().preblocks_pointer);
    assert!(h.state().view.selection.is_empty());
    assert!(h.state().view.board_selection.is_empty());
    assert_eq!(h.state().view.editing_topic(), None);
    assert_eq!(h.state().doc.state_id(), revision);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn minimap_blocks_board_handles_context_menu_double_click_and_creation_tools() {
    let mut h = canvas_harness();
    let target = minimap_rect(&h).center();
    let object = h.state().shape;
    let corner = h
        .state()
        .doc
        .sheet()
        .canvas
        .object(object)
        .unwrap()
        .rect
        .max();
    h.state_mut().view.select_board_object(object);
    put_world_at(&mut h, corner, target);
    let before = h.state().doc.workbook().clone();
    let selection = h.state().view.board_selection.clone();
    h.hover_at(target);
    h.drag_at(target);
    h.run_steps(1);
    h.hover_at(target + vec2(35.0, 25.0));
    h.run_steps(2);
    h.drop_at(target + vec2(35.0, 25.0));
    h.run_steps(2);
    assert_eq!(h.state().doc.workbook(), &before);
    assert_eq!(h.state().view.board_selection, selection);

    click(&mut h, target, PointerButton::Secondary);
    assert!(h.state().minimap_blocks_pointer);
    assert!(!egui::Popup::is_any_open(&h.ctx));
    click(&mut h, target, PointerButton::Primary);
    click(&mut h, target, PointerButton::Primary);
    assert_eq!(h.state().view.editing_board_object(), None);
    assert_eq!(h.state().doc.workbook(), &before);

    h.state_mut().view.set_board_tool(BoardTool::Sticky);
    h.run_steps(2);
    click(&mut h, target, PointerButton::Primary);
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn canvas_drag_started_outside_is_not_stolen_by_the_minimap() {
    let mut h = canvas_harness();
    h.state_mut().view.set_board_tool(BoardTool::Hand);
    h.run_steps(2);
    let start = h.state().view.canvas_rect().left_top() + vec2(35.0, 35.0);
    let end = minimap_rect(&h).center();
    let before = h.state().view.offset;
    h.hover_at(start);
    h.drag_at(start);
    h.run_steps(1);
    h.hover_at(end);
    h.run_steps(2);
    assert!(!h.state().preblocks_pointer);
    h.drop_at(end);
    h.run_steps(2);
    let delta = h.state().view.offset - before;
    assert!((delta - (end - start)).length() < 1.0, "{delta:?}");
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn enabling_minimap_mid_drag_does_not_claim_a_preexisting_canvas_gesture() {
    let mut h = canvas_harness();
    h.state_mut().view.set_minimap_visible(false);
    h.state_mut().view.set_board_tool(BoardTool::Hand);
    h.run_steps(3);
    let start = minimap_rect(&h).center();
    let middle = start + vec2(15.0, 0.0);
    let end = middle + vec2(35.0, -20.0);
    let before = h.state().view.offset;
    h.hover_at(start);
    h.drag_at(start);
    h.run_steps(1);
    h.hover_at(middle);
    h.run_steps(2);
    h.state_mut().view.set_minimap_visible(true);
    h.hover_at(end);
    h.run_steps(2);
    assert!(!h.state().preblocks_pointer);
    h.drop_at(end);
    h.run_steps(2);
    let delta = h.state().view.offset - before;
    assert!((delta - (end - start)).length() < 1.0, "{delta:?}");
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn minimap_blocks_trackpad_scroll_and_zoom() {
    let mut h = canvas_harness();
    let point = minimap_rect(&h).center();
    h.hover_at(point);
    h.run_steps(2);
    let offset = h.state().view.offset;
    let zoom = h.state().view.zoom;
    h.event(egui::Event::Zoom(1.5));
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(40.0, 30.0),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Trackpad,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(1);
    assert_eq!(h.state().view.offset, offset);
    assert_eq!(h.state().view.zoom, zoom);
}

#[test]
fn focused_minimap_routes_arrow_keys_without_canvas_shortcuts_and_allows_tab_exit() {
    let mut h = canvas_harness();
    let card = h.state().card;
    h.state_mut().view.select_only(card);
    h.run_steps(2);
    let before = h.state().doc.workbook().clone();
    h.get_by_label("Board navigator").focus();
    h.run_steps(2);
    assert!(h.state().preblocks_keyboard);
    let offset = h.state().view.offset;
    h.key_press(Key::ArrowRight);
    h.run_steps(1);
    assert!(h.state().view.offset.x < offset.x);
    assert_eq!(h.state().view.selection, vec![card]);
    h.key_press(Key::Delete);
    h.run_steps(1);
    assert_eq!(h.state().doc.workbook(), &before);
    assert_eq!(h.state().view.selection, vec![card]);

    let id = MapCanvas::id().with("minimap");
    assert_eq!(h.ctx.memory(|memory| memory.focused()), Some(id));
    h.key_press(Key::Tab);
    h.run_steps(3);
    assert_ne!(h.ctx.memory(|memory| memory.focused()), Some(id));
}

#[test]
fn keyboard_tab_traversal_can_reach_the_minimap_from_the_canvas_controls() {
    let mut h = canvas_harness();
    h.get_by_label("After canvas").focus();
    h.run_steps(1);
    let minimap_id = MapCanvas::id().with("minimap");
    let mut reached = false;
    let mut trace = Vec::new();
    for _ in 0..16 {
        h.key_press(Key::Tab);
        h.run_steps(2);
        let focus = h.ctx.memory(|memory| memory.focused());
        trace.push(focus);
        if focus == Some(minimap_id) {
            reached = true;
            break;
        }
    }
    assert!(
        reached,
        "Tab traversal must reach {minimap_id:?}; focus sequence: {trace:?}"
    );
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert_ne!(h.ctx.memory(|memory| memory.focused()), Some(minimap_id));
    assert!(h.get_by_label("After canvas").is_focused());

    h.key_press_modifiers(Modifiers::SHIFT, Key::Tab);
    h.run_steps(3);
    assert_eq!(h.ctx.memory(|memory| memory.focused()), Some(minimap_id));
    h.key_press_modifiers(Modifiers::SHIFT, Key::Tab);
    h.run_steps(3);
    let last_board_item =
        MapCanvas::id().with(("board-item", thinoda_core::SceneId::Object(h.state().shape)));
    assert_eq!(
        h.ctx.memory(|memory| memory.focused()),
        Some(last_board_item)
    );
}

#[test]
fn a_retained_title_draft_does_not_intercept_minimap_keyboard_navigation() {
    let mut h = canvas_harness();
    let card = h.state().card;
    {
        let state = h.state_mut();
        state.view.begin_edit(state.doc.sheet(), card, false);
    }
    h.run_steps(2);
    h.state_mut().view.set_editing_text("Unsaved draft");
    h.state_mut()
        .doc
        .edit_sheet("Concurrent title", |sheet| {
            sheet.set_title(card, "Externally changed")
        })
        .unwrap();
    h.run_steps(2);
    h.get_by_label("Board navigator").focus();
    h.run_steps(2);
    assert_eq!(h.state().view.editing_text(), Some("Unsaved draft"));
    let before = h.state().doc.workbook().clone();
    let minimap_id = MapCanvas::id().with("minimap");
    assert_eq!(h.ctx.memory(|memory| memory.focused()), Some(minimap_id));
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert_ne!(h.ctx.memory(|memory| memory.focused()), Some(minimap_id));
    assert_eq!(h.state().view.editing_text(), Some("Unsaved draft"));
    assert_eq!(h.state().doc.workbook(), &before);
}

#[test]
fn a_retained_board_label_draft_does_not_reclaim_minimap_focus_or_consume_keys() {
    let mut h = canvas_harness();
    let shape = h.state().shape;
    {
        let state = h.state_mut();
        assert!(state.view.begin_board_label(&state.doc, shape));
    }
    h.run_steps(2);
    let draft = "x".repeat(thinoda_core::board::MAX_BOARD_TEXT_CHARS + 1);
    h.get_by_label("Board object label").type_text(&draft);
    h.run_steps(2);
    h.get_by_label("Board navigator").focus();
    h.run_steps(2);
    assert_eq!(h.state().view.editing_board_object(), Some(shape));
    assert_eq!(h.state().view.board_label_draft().unwrap().2, draft);
    let minimap_id = MapCanvas::id().with("minimap");
    assert_eq!(h.ctx.memory(|memory| memory.focused()), Some(minimap_id));
    let before = h.state().doc.workbook().clone();
    let offset = h.state().view.offset;
    h.key_press(Key::ArrowRight);
    h.run_steps(1);
    assert!(h.state().view.offset.x < offset.x);
    h.key_press(Key::Enter);
    h.run_steps(1);
    assert_eq!(h.state().view.editing_board_object(), Some(shape));
    assert_eq!(h.ctx.memory(|memory| memory.focused()), Some(minimap_id));
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert_ne!(h.ctx.memory(|memory| memory.focused()), Some(minimap_id));
    assert_eq!(h.state().view.board_label_draft().unwrap().2, draft);
    assert_eq!(h.state().doc.workbook(), &before);
}

#[test]
fn hiding_during_a_minimap_drag_blocks_release_then_canvas_clicks_work_again() {
    let mut h = canvas_harness();
    let point = minimap_rect(&h).center();
    let outside = h.state().view.canvas_rect().left_top() + vec2(40.0, 40.0);
    h.hover_at(point);
    h.drag_at(point);
    h.run_steps(1);
    h.state_mut().view.set_minimap_visible(false);
    h.hover_at(outside);
    h.run_steps(1);
    assert!(h.state().minimap_blocks_pointer);
    h.event(egui::Event::PointerButton {
        pos: outside,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(1);
    assert!(h.state().minimap_blocks_pointer);
    assert!(h.state().preblocks_pointer);
    assert!(h.state().view.selection.is_empty());
    assert!(h.state().view.board_selection.is_empty());
    h.run_steps(2);
    assert!(!h.state().minimap_blocks_pointer);
    let card = h.state().card;
    let card_world = {
        let state = h.state_mut();
        state
            .view
            .layout_for(&state.doc)
            .rect(card)
            .unwrap()
            .center()
    };
    put_world_at(&mut h, card_world, outside);
    click(&mut h, outside, PointerButton::Primary);
    assert_eq!(h.state().view.selection, vec![card]);
}

#[test]
fn resizing_to_a_tiny_canvas_cancels_capture_without_leaking_the_release() {
    let mut h = canvas_harness();
    let start = minimap_rect(&h).center();
    h.hover_at(start);
    h.drag_at(start);
    h.run_steps(1);
    h.set_size(vec2(120.0, 100.0));
    h.run_steps(2);
    assert!(h.state().minimap_blocks_pointer);
    assert!(h.query_by_label("Board navigator").is_none());
    let end = h.state().view.canvas_rect().center();
    h.event(egui::Event::PointerButton {
        pos: end,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(1);
    assert!(h.state().minimap_blocks_pointer);
    assert!(h.state().view.selection.is_empty());
    h.run_steps(2);
    assert!(!h.state().minimap_blocks_pointer);
    h.set_size(vec2(640.0, 480.0));
    h.run_steps(3);
    h.get_by_label("Board navigator");
}

#[test]
fn empty_board_minimap_stays_usable_and_keeps_the_camera_finite() {
    let mut h = canvas_harness();
    let mut empty = Sheet::new("Empty", "");
    empty.root_mode = SheetRootMode::Hidden;
    h.state_mut().doc = Document::new(Workbook::new(empty));
    h.state_mut().view.reset();
    h.run_steps(3);
    h.get_by_label("Board navigator");
    let target = minimap_rect(&h).right_center() - vec2(15.0, 0.0);
    click(&mut h, target, PointerButton::Primary);
    let offset = h.state().view.offset;
    assert!(offset.x.is_finite() && offset.y.is_finite());
    assert!(h.state().view.selection.is_empty());
    assert!(h.state().view.board_selection.is_empty());
}

#[test]
fn touch_cancellation_and_mixed_multitouch_do_not_reach_the_canvas() {
    let mut h = canvas_harness();
    let point = minimap_rect(&h).center();
    let before = h.state().doc.workbook().clone();
    h.event(egui::Event::PointerMoved(point));
    h.event(egui::Event::PointerButton {
        pos: point,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    touch(&mut h, 1, egui::TouchPhase::Start, point);
    h.run_steps(1);
    touch(&mut h, 1, egui::TouchPhase::Cancel, point);
    h.event(egui::Event::PointerButton {
        pos: point,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(2);
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state().view.selection.is_empty());
    assert!(h.state().view.board_selection.is_empty());

    let outside = h.state().view.canvas_rect().left_top() + vec2(50.0, 50.0);
    h.event(egui::Event::PointerMoved(outside));
    h.event(egui::Event::PointerButton {
        pos: outside,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    touch(&mut h, 1, egui::TouchPhase::Start, outside);
    h.run_steps(1);
    touch(&mut h, 2, egui::TouchPhase::Start, point);
    h.run_steps(1);
    let zoom = h.state().view.zoom;
    let offset = h.state().view.offset;
    touch(&mut h, 2, egui::TouchPhase::Move, point + vec2(20.0, 20.0));
    h.event(egui::Event::Zoom(1.5));
    h.run_steps(1);
    assert_eq!(h.state().view.zoom, zoom);
    assert_eq!(h.state().view.offset, offset);
    touch(&mut h, 2, egui::TouchPhase::End, point + vec2(20.0, 20.0));
    touch(&mut h, 1, egui::TouchPhase::End, outside);
    h.run_steps(1);
    assert!(h.state().minimap_blocks_pointer);
    h.event(egui::Event::PointerButton {
        pos: outside,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(1);
    assert!(h.state().minimap_blocks_pointer);
    h.run_steps(2);
    assert!(!h.state().minimap_blocks_pointer);
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state().view.selection.is_empty());
}

#[test]
fn touch_drag_pans_minimap_without_creating_a_board_item() {
    let mut h = canvas_harness();
    h.state_mut().view.set_board_tool(BoardTool::Sticky);
    h.run_steps(2);
    let start = minimap_rect(&h).center();
    let end = start + vec2(30.0, 20.0);
    let before = h.state().doc.workbook().clone();
    let offset = h.state().view.offset;
    h.event(egui::Event::PointerMoved(start));
    h.event(egui::Event::PointerButton {
        pos: start,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    touch(&mut h, 1, egui::TouchPhase::Start, start);
    h.run_steps(1);
    h.event(egui::Event::PointerMoved(end));
    touch(&mut h, 1, egui::TouchPhase::Move, end);
    h.run_steps(2);
    touch(&mut h, 1, egui::TouchPhase::End, end);
    h.event(egui::Event::PointerButton {
        pos: end,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(2);
    assert_ne!(h.state().view.offset, offset);
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state().view.selection.is_empty());
}

#[test]
fn hidden_minimap_suppresses_a_delayed_synthetic_touch_release() {
    let mut h = canvas_harness();
    h.state_mut().view.set_board_tool(BoardTool::Sticky);
    h.run_steps(2);
    let start = minimap_rect(&h).center();
    let end = h.state().view.canvas_rect().left_top() + vec2(40.0, 40.0);
    let before = h.state().doc.workbook().clone();
    h.event(egui::Event::PointerMoved(start));
    h.event(egui::Event::PointerButton {
        pos: start,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    touch(&mut h, 1, egui::TouchPhase::Start, start);
    h.run_steps(1);
    h.state_mut().view.set_minimap_visible(false);
    h.event(egui::Event::PointerMoved(end));
    touch(&mut h, 1, egui::TouchPhase::End, end);
    h.run_steps(1);
    assert!(h.state().minimap_blocks_pointer);
    h.event(egui::Event::PointerButton {
        pos: end,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(1);
    assert!(h.state().minimap_blocks_pointer);
    h.run_steps(2);
    assert!(!h.state().minimap_blocks_pointer);
    assert_eq!(h.state().doc.workbook(), &before);
}

#[test]
fn disabled_canvas_does_not_allow_minimap_panning() {
    let mut h = canvas_harness();
    let target = minimap_rect(&h).right_center() - vec2(15.0, 0.0);
    h.state_mut().enabled = false;
    h.run_steps(2);
    let offset = h.state().view.offset;
    click(&mut h, target, PointerButton::Primary);
    assert_eq!(h.state().view.offset, offset);
    assert!(h.state().view.selection.is_empty());
}
