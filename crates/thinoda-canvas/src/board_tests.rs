use super::*;
use crate::actions::{BoardAction, apply_board_action, board_actions};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use thinoda_core::{
    Band, CanvasConnector, CanvasObject, CanvasObjectKind, Card, CardKind, ConnectorPort,
    ConnectorRoute, Endpoint, SceneId, ShapeKind, SheetRootMode, Workbook, ZOrder,
    board::BoardMember,
};

struct State {
    doc: Document,
    view: MapView,
    errors: Vec<thinoda_core::EditError>,
    clipboard_requests: Vec<Action>,
    open_links: Vec<String>,
}

fn board() -> Sheet {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    sheet
}

fn harness(sheet: Sheet) -> Harness<'static, State> {
    harness_with_overlay(sheet, None)
}

fn harness_with_overlay(sheet: Sheet, overlay: Option<Rect>) -> Harness<'static, State> {
    let view = MapView {
        show_add_controls: false,
        ..Default::default()
    };
    let mut doc = Document::new(Workbook::new(sheet));
    doc.mark_saved();
    let mut harness = Harness::builder()
        .with_size(vec2(1200.0, 900.0))
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(
            move |ui, state: &mut State| {
                let out = MapCanvas::show(ui, &mut state.doc, &mut state.view);
                if let Some(error) = out.error {
                    state.errors.push(error);
                }
                state.clipboard_requests.extend(out.clipboard_requests);
                if let Some(link) = out.open_link {
                    state.open_links.push(link);
                }
                if let Some(rect) = overlay {
                    ui.interact(rect, Id::unique("test-overlay"), Sense::click_and_drag());
                }
            },
            State {
                doc,
                view,
                errors: Vec::new(),
                clipboard_requests: Vec::new(),
                open_links: Vec::new(),
            },
        );
    harness.run_steps(3);
    harness.state_mut().view.set_zoom(1.0);
    harness.run_steps(2);
    harness
}

fn point(h: &Harness<'_, State>, p: Vec2) -> Pos2 {
    h.state().view.xf().pos(p)
}

fn set_tool(h: &mut Harness<'_, State>, tool: BoardTool) {
    h.state_mut().view.set_board_tool(tool);
    h.run_steps(2);
}

fn drag(h: &mut Harness<'_, State>, start: Pos2, end: Pos2) {
    h.hover_at(start);
    h.drag_at(start);
    h.run_steps(1);
    h.hover_at(start + (end - start) * 0.5);
    h.run_steps(2);
    h.hover_at(end);
    h.run_steps(2);
}

fn finish_drag(h: &mut Harness<'_, State>, end: Pos2) {
    h.drop_at(end);
    h.run_steps(3);
}

fn begin_alt_drag(h: &mut Harness<'_, State>, start: Pos2, end: Pos2) {
    h.event(egui::Event::ModifiersChanged(Modifiers::ALT));
    h.hover_at(start);
    h.event(egui::Event::PointerButton {
        pos: start,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::ALT,
    });
    h.run_steps(1);
    h.hover_at(start + (end - start) * 0.5);
    h.run_steps(2);
    h.hover_at(end);
    h.run_steps(2);
}

fn finish_alt_drag(h: &mut Harness<'_, State>, end: Pos2, modifiers: Modifiers) {
    h.event(egui::Event::PointerButton {
        pos: end,
        button: PointerButton::Primary,
        pressed: false,
        modifiers,
    });
    h.event(egui::Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(3);
}

fn click(h: &mut Harness<'_, State>, point: Pos2, modifiers: Modifiers) {
    h.event(egui::Event::ModifiersChanged(modifiers));
    h.hover_at(point);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: point,
            button: PointerButton::Primary,
            pressed,
            modifiers,
        });
        h.run_steps(1);
    }
    h.event(egui::Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(1);
}

fn shape(sheet: &mut Sheet, x: f32, y: f32, label: &str) -> CanvasObjectId {
    let mut object = CanvasObject::shape(
        ShapeKind::Rectangle,
        thinoda_core::Rect::from_center_size(Vec2::new(x, y), Vec2::new(120.0, 80.0)),
    );
    if let CanvasObjectKind::Shape { label: text, .. } = &mut object.kind {
        *text = label.into();
    }
    sheet.add_canvas_object(object).unwrap()
}

#[test]
fn deep_board_duplication_fails_before_layout_and_preserves_history() {
    let mut sheet = board();
    let card = sheet
        .add_card(
            "Card",
            Vec2::ZERO,
            Card::new(CardKind::Sticky, Vec2::new(120.0, 100.0)),
        )
        .unwrap();
    let mut parent = card;
    for _ in 0..=thinoda_core::clipboard::MAX_TOPIC_CLIPBOARD_DEPTH {
        parent = sheet.add_child(parent, None, "Child");
    }
    let frame = sheet
        .create_board_frame(
            "Frame",
            thinoda_core::Rect::from_min_size(Vec2::new(-100.0, -100.0), Vec2::new(200.0, 200.0)),
        )
        .unwrap();
    sheet.set_zone_members(frame, vec![card], vec![]).unwrap();
    let mut doc = Document::new(Workbook::new(sheet));
    let mut view = MapView::default();
    let before = doc.workbook().clone();

    view.select_only(card);
    for action in [Action::Copy, Action::Cut, Action::Duplicate] {
        let error = apply_action(&mut doc, &mut view, action).unwrap_err();
        assert!(error.to_string().contains("depth"), "{error}");
        assert_eq!(doc.workbook(), &before);
        assert!(!doc.can_undo());
    }
    let error =
        board_actions::duplicate_selection(&mut doc, &mut view, Vec2::new(20.0, 20.0)).unwrap_err();
    assert!(error.to_string().contains("depth"), "{error}");
    assert_eq!(doc.workbook(), &before);

    view.select_object(ObjectSelection::Zone(frame));
    let error =
        board_actions::duplicate_selection(&mut doc, &mut view, Vec2::new(20.0, 20.0)).unwrap_err();
    assert!(error.to_string().contains("depth"), "{error}");
    assert_eq!(doc.workbook(), &before);
    assert!(!doc.can_undo());
}

#[test]
#[ignore = "manual full canvas CPU measurement"]
fn measure_large_board_canvas_pass() {
    use std::{hint::black_box, time::Instant};
    use thinoda_core::{Color, InkStroke, InkTool};

    for strokes in [1_000, 10_000] {
        let mut sheet = board();
        for index in 0..strokes {
            let origin = Vec2::new((index % 100) as f32 * 80.0, (index / 100) as f32 * 80.0);
            let points = (0..64)
                .map(|point| {
                    let x = point as f32;
                    origin + Vec2::new(x, (x * 0.2).sin() * 6.0)
                })
                .collect();
            sheet
                .add_canvas_object(CanvasObject::ink(InkStroke::new(
                    InkTool::Pen,
                    Color::rgb(40, 100, 220),
                    2.0,
                    points,
                )))
                .unwrap();
        }
        for index in 0..50 {
            sheet
                .add_card(
                    &format!("Image {index}"),
                    Vec2::new(index as f32 * 160.0, -180.0),
                    Card::new(CardKind::Image, Vec2::new(140.0, 100.0)),
                )
                .unwrap();
        }
        let workbook = Workbook::new(sheet);
        workbook.validate().unwrap();
        for accesskit in [false, true] {
            let mut doc = Document::new(workbook.clone());
            let mut view = MapView {
                show_add_controls: false,
                ..Default::default()
            };
            let ctx = egui::Context::default();
            if accesskit {
                ctx.enable_accesskit();
            }
            let mut samples = Vec::new();
            for sample in 0..23 {
                let start = Instant::now();
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 900.0))),
                        ..Default::default()
                    },
                    |ui| {
                        assert!(MapCanvas::show(ui, &mut doc, &mut view).error.is_none());
                    },
                );
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                black_box(output);
                if sample >= 3 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            eprintln!(
                "strokes={strokes} cards=50 accesskit={accesskit} p50_ms={:.3} p95_ms={:.3}",
                samples[9], samples[18]
            );
        }
    }
}

#[test]
fn board_duplication_transaction_remaps_mixed_groups_and_skips_locked_items() {
    let mut sheet = board();
    let card = sheet
        .add_card(
            "Card",
            Vec2::new(-200.0, 0.0),
            Card::new(CardKind::Sticky, Vec2::new(120.0, 100.0)),
        )
        .unwrap();
    let object = shape(&mut sheet, 0.0, 0.0, "Grouped shape");
    let connector = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Topic(card),
            Endpoint::Object(object),
        )))
        .unwrap();
    let group = sheet
        .group_board_members(&[
            BoardMember::Card(card),
            BoardMember::Object(object),
            BoardMember::Object(connector),
        ])
        .unwrap();
    let locked = shape(&mut sheet, 350.0, 0.0, "Locked");
    sheet
        .set_board_members_locked(&[BoardMember::Object(locked)], true)
        .unwrap();
    let locked_card = sheet
        .add_card(
            "Locked group card",
            Vec2::new(500.0, 0.0),
            Card::new(CardKind::Sticky, Vec2::new(120.0, 100.0)),
        )
        .unwrap();
    let locked_group_object = shape(&mut sheet, 500.0, 180.0, "Locked group member");
    sheet
        .group_board_members(&[
            BoardMember::Card(locked_card),
            BoardMember::Object(locked_group_object),
        ])
        .unwrap();
    sheet
        .set_board_members_locked(&[BoardMember::Card(locked_card)], true)
        .unwrap();
    let standalone = shape(&mut sheet, 200.0, 200.0, "Standalone");
    let mut h = harness(sheet);
    {
        let state = h.state_mut();
        state.view.selection = vec![card];
        state.view.board_selection = vec![locked, locked_group_object, standalone];
        let layout = state.view.layout_for(&state.doc);
        let payload =
            thinoda_core::clipboard::copy_board_selection(state.doc.sheet(), &layout, &[card], &[])
                .unwrap()
                .unwrap();
        state.view.set_board_clipboard(Some(payload));
    }
    let before = h.state().doc.workbook().clone();
    let clipboard = h.state().view.board_clipboard().cloned();
    let delta = Vec2::new(60.0, 40.0);
    {
        let state = h.state_mut();
        assert!(
            board_actions::duplicate_selection(&mut state.doc, &mut state.view, delta).unwrap()
        );
    }
    assert_eq!(h.state().view.board_clipboard(), clipboard.as_ref());
    let sheet = h.state().doc.sheet();
    let copied_card = h.state().view.selection[0];
    assert_ne!(copied_card, card);
    assert_eq!(
        sheet.topic(copied_card).unwrap().position,
        Some(Vec2::new(-140.0, 40.0))
    );
    let copied_objects = &h.state().view.board_selection;
    assert_eq!(copied_objects.len(), 3);
    let copied_shape = copied_objects
        .iter()
        .copied()
        .find(|id| sheet.canvas.object(*id).unwrap().label() == "Grouped shape")
        .unwrap();
    let copied_standalone = copied_objects
        .iter()
        .copied()
        .find(|id| sheet.canvas.object(*id).unwrap().label() == "Standalone")
        .unwrap();
    let copied_connector = copied_objects
        .iter()
        .copied()
        .find(|id| sheet.canvas.object(*id).unwrap().is_connector())
        .unwrap();
    assert_ne!(copied_shape, object);
    assert_ne!(copied_standalone, standalone);
    assert_eq!(
        sheet.canvas.object(copied_shape).unwrap().rect.center(),
        Vec2::new(60.0, 40.0)
    );
    assert_eq!(
        sheet
            .canvas
            .object(copied_standalone)
            .unwrap()
            .rect
            .center(),
        Vec2::new(260.0, 240.0)
    );
    let copied_group = sheet.canvas.object(copied_shape).unwrap().group.unwrap();
    assert_ne!(copied_group, group);
    assert_eq!(
        sheet.board_member_group(BoardMember::Card(copied_card)),
        Some(copied_group)
    );
    assert_eq!(
        sheet.canvas.object(copied_connector).unwrap().group,
        Some(copied_group)
    );
    let CanvasObjectKind::Connector(copied_line) =
        &sheet.canvas.object(copied_connector).unwrap().kind
    else {
        panic!("expected copied connector");
    };
    assert_eq!(copied_line.from, Endpoint::Topic(copied_card));
    assert_eq!(copied_line.to, Endpoint::Object(copied_shape));
    assert_eq!(
        sheet.canvas.object(locked),
        before.sheets[0].canvas.object(locked)
    );
    assert_eq!(
        sheet.topic(locked_card),
        before.sheets[0].topic(locked_card)
    );
    assert_eq!(
        sheet.canvas.object(locked_group_object),
        before.sheets[0].canvas.object(locked_group_object)
    );
    sheet.validate().unwrap();
    let after = h.state().doc.workbook().clone();
    assert_eq!(h.state().doc.undo_label(), Some("Duplicate board items"));
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(!h.state().doc.can_undo());
    assert!(h.state_mut().doc.redo());
    assert_eq!(h.state().doc.workbook(), &after);
}

#[test]
fn board_duplication_transaction_copies_empty_frame_in_one_edit() {
    let mut sheet = board();
    let rect =
        thinoda_core::Rect::from_min_size(Vec2::new(-200.0, -100.0), Vec2::new(400.0, 200.0));
    let frame = sheet.create_board_frame("Empty frame", rect).unwrap();
    let mut h = harness(sheet);
    h.state_mut()
        .view
        .select_object(ObjectSelection::Zone(frame));
    let before = h.state().doc.workbook().clone();
    {
        let state = h.state_mut();
        assert!(
            board_actions::duplicate_selection(
                &mut state.doc,
                &mut state.view,
                Vec2::new(30.0, 40.0)
            )
            .unwrap()
        );
    }
    let ObjectSelection::Zone(copy) = h.state().view.selected_object.unwrap() else {
        panic!("expected copied frame selection");
    };
    let copied = h
        .state()
        .doc
        .sheet()
        .zones
        .iter()
        .find(|zone| zone.id == copy)
        .unwrap();
    assert_ne!(copy, frame);
    assert!(copied.roots.is_empty() && copied.keep_empty);
    assert_eq!(copied.frame, rect.translate(Vec2::new(30.0, 40.0)));
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(!h.state().doc.can_undo());
}

#[test]
fn board_duplication_transaction_is_atomic_for_locks_and_invalid_coordinates() {
    let mut sheet = board();
    let first = shape(&mut sheet, -200.0, 0.0, "Locked group");
    let second = shape(&mut sheet, 0.0, 0.0, "Group member");
    sheet
        .group_board_members(&[BoardMember::Object(first), BoardMember::Object(second)])
        .unwrap();
    sheet
        .set_board_members_locked(&[BoardMember::Object(first)], true)
        .unwrap();
    let near_edge = shape(&mut sheet, 999_930.0, 0.0, "Edge");
    let mut h = harness(sheet);
    h.state_mut().view.board_selection = vec![second];
    let before = h.state().doc.workbook().clone();
    {
        let state = h.state_mut();
        assert!(
            !board_actions::duplicate_selection(
                &mut state.doc,
                &mut state.view,
                Vec2::new(10.0, 0.0)
            )
            .unwrap()
        );
    }
    assert_eq!(h.state().view.board_selection, vec![second]);
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(!h.state().doc.can_undo());

    h.state_mut().view.board_selection = vec![near_edge];
    {
        let state = h.state_mut();
        assert!(
            board_actions::duplicate_selection(
                &mut state.doc,
                &mut state.view,
                Vec2::new(40.0, 0.0)
            )
            .is_err()
        );
        assert!(
            board_actions::duplicate_selection(
                &mut state.doc,
                &mut state.view,
                Vec2::new(f32::NAN, 0.0)
            )
            .is_err()
        );
    }
    assert_eq!(h.state().view.board_selection, vec![near_edge]);
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(!h.state().doc.can_undo());
}

#[test]
fn board_duplication_transaction_copies_frames_and_rejects_locked_contents() {
    let mut sheet = board();
    let card = sheet
        .add_card(
            "Framed card",
            Vec2::ZERO,
            Card::new(CardKind::Sticky, Vec2::new(100.0, 80.0)),
        )
        .unwrap();
    let object = shape(&mut sheet, 160.0, 0.0, "Grouped shape");
    sheet
        .group_board_members(&[BoardMember::Card(card), BoardMember::Object(object)])
        .unwrap();
    let rect =
        thinoda_core::Rect::from_min_size(Vec2::new(-150.0, -120.0), Vec2::new(400.0, 240.0));
    let frame = sheet.create_board_frame("Frame", rect).unwrap();
    sheet
        .set_zone_members(frame, vec![card], vec![object])
        .unwrap();
    sheet.zones[0].color = Some(thinoda_core::Color::rgb(10, 20, 30));
    sheet.zones[0].clip = true;
    let mut h = harness(sheet);
    h.state_mut()
        .view
        .select_object(ObjectSelection::Zone(frame));
    let before = h.state().doc.workbook().clone();
    let delta = Vec2::new(80.0, 40.0);
    {
        let state = h.state_mut();
        assert!(
            board_actions::duplicate_selection(&mut state.doc, &mut state.view, delta).unwrap()
        );
    }
    let sheet = h.state().doc.sheet();
    let ObjectSelection::Zone(copied_id) = h.state().view.selected_object.unwrap() else {
        panic!("expected copied frame selection");
    };
    let copied = sheet
        .zones
        .iter()
        .find(|zone| zone.id == copied_id)
        .unwrap();
    assert_ne!(copied.id, frame);
    assert_eq!(copied.frame, rect.translate(delta));
    assert_eq!(copied.name, "Frame");
    assert_eq!(copied.color, sheet.zones[0].color);
    assert!(copied.clip && copied.keep_empty);
    assert_eq!(copied.roots.len(), 1);
    assert_ne!(copied.roots[0], card);
    assert_eq!(copied.objects.len(), 1);
    assert_ne!(copied.objects[0], object);
    assert_eq!(sheet.topic(copied.roots[0]).unwrap().position, Some(delta));
    let copied_group = sheet
        .board_member_group(BoardMember::Card(copied.roots[0]))
        .unwrap();
    assert!(sheet.canvas.objects.iter().any(|item| {
        item.id != object
            && item.group == Some(copied_group)
            && item.rect.center() == Vec2::new(240.0, 40.0)
    }));
    sheet.validate().unwrap();
    assert_eq!(h.state().doc.undo_label(), Some("Duplicate board frame"));
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(!h.state().doc.can_undo());

    {
        let state = h.state_mut();
        state
            .doc
            .edit_sheet("Lock", |sheet| {
                sheet
                    .set_board_members_locked(&[BoardMember::Object(object)], true)
                    .map_err(thinoda_core::EditError::Invalid)
            })
            .unwrap();
        state.view.select_object(ObjectSelection::Zone(frame));
    }
    let locked = h.state().doc.workbook().clone();
    {
        let state = h.state_mut();
        let error =
            board_actions::duplicate_selection(&mut state.doc, &mut state.view, delta).unwrap_err();
        assert!(error.to_string().contains("locked"));
    }
    assert_eq!(h.state().doc.workbook(), &locked);
    assert_eq!(
        h.state().view.selected_object,
        Some(ObjectSelection::Zone(frame))
    );
}

#[test]
fn board_alt_drag_copies_mixed_group_on_release_and_undoes_once() {
    let mut sheet = board();
    let card = sheet
        .add_card(
            "Card",
            Vec2::new(-200.0, 0.0),
            Card::new(CardKind::Sticky, Vec2::new(120.0, 100.0)),
        )
        .unwrap();
    let object = shape(&mut sheet, 0.0, 0.0, "Shape");
    let line = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Topic(card),
            Endpoint::Object(object),
        )))
        .unwrap();
    sheet
        .group_board_members(&[
            BoardMember::Card(card),
            BoardMember::Object(object),
            BoardMember::Object(line),
        ])
        .unwrap();
    let mut h = harness(sheet);
    let start = point(&h, Vec2::ZERO);
    click(&mut h, start, Modifiers::NONE);
    let before = h.state().doc.workbook().clone();
    let end = start + vec2(80.0, 55.0);
    begin_alt_drag(&mut h, start, end);
    assert_eq!(h.state().doc.workbook(), &before);
    // The operation is latched at drag start, even if Alt is released first.
    h.event(egui::Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(1);
    finish_alt_drag(&mut h, end, Modifiers::NONE);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    let sheet = h.state().doc.sheet();
    assert_eq!(
        sheet.canvas.object(object),
        before.sheets[0].canvas.object(object)
    );
    assert_eq!(sheet.topic(card), before.sheets[0].topic(card));
    assert_eq!(h.state().view.selection.len(), 1);
    assert_eq!(h.state().view.board_selection.len(), 2);
    let copied_card = h.state().view.selection[0];
    let copied_shape = h
        .state()
        .view
        .board_selection
        .iter()
        .copied()
        .find(|id| sheet.canvas.object(*id).unwrap().label() == "Shape")
        .unwrap();
    let copied_line = h
        .state()
        .view
        .board_selection
        .iter()
        .copied()
        .find(|id| sheet.canvas.object(*id).unwrap().is_connector())
        .unwrap();
    assert_eq!(
        sheet.topic(copied_card).unwrap().position,
        Some(Vec2::new(-120.0, 55.0))
    );
    assert_eq!(
        sheet.canvas.object(copied_shape).unwrap().rect.center(),
        Vec2::new(80.0, 55.0)
    );
    let CanvasObjectKind::Connector(copied) = &sheet.canvas.object(copied_line).unwrap().kind
    else {
        panic!("expected copied connector");
    };
    assert_eq!(copied.from, Endpoint::Topic(copied_card));
    assert_eq!(copied.to, Endpoint::Object(copied_shape));
    let after = h.state().doc.workbook().clone();
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(!h.state().doc.can_undo());
    assert!(h.state_mut().doc.redo());
    assert_eq!(h.state().doc.workbook(), &after);
}

#[test]
fn board_alt_arrow_duplicates_in_each_direction_and_focuses_the_copy() {
    for (modifiers, key, delta) in [
        (Modifiers::ALT, Key::ArrowLeft, Vec2::new(-10.0, 0.0)),
        (Modifiers::ALT, Key::ArrowRight, Vec2::new(10.0, 0.0)),
        (Modifiers::ALT, Key::ArrowUp, Vec2::new(0.0, -10.0)),
        (Modifiers::ALT, Key::ArrowDown, Vec2::new(0.0, 10.0)),
        (
            Modifiers::ALT.plus(Modifiers::SHIFT),
            Key::ArrowRight,
            Vec2::new(40.0, 0.0),
        ),
    ] {
        let mut sheet = board();
        let id = shape(&mut sheet, 0.0, 0.0, "Keyboard copy");
        let mut h = harness(sheet);
        h.get_by_label("Rectangle: Keyboard copy").focus();
        h.run_steps(2);
        let before = h.state().doc.workbook().clone();
        h.key_press_modifiers(modifiers, key);
        h.run_steps(3);
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
        assert_eq!(h.state().view.board_selection.len(), 1);
        let copied = h.state().view.board_selection[0];
        assert_ne!(copied, id);
        assert_eq!(
            h.state()
                .doc
                .sheet()
                .canvas
                .object(copied)
                .unwrap()
                .rect
                .center(),
            delta
        );
        assert_eq!(
            h.state()
                .doc
                .sheet()
                .canvas
                .object(id)
                .unwrap()
                .rect
                .center(),
            Vec2::ZERO
        );
        assert!(
            h.get_by_label("Rectangle: Keyboard copy, Selected")
                .is_focused()
        );
        assert!(h.state_mut().doc.undo());
        assert_eq!(h.state().doc.workbook(), &before);
        assert!(!h.state().doc.can_undo());
    }
}

#[test]
fn board_alt_arrow_repeats_without_collapsing_a_mixed_group_selection() {
    let mut sheet = board();
    let card = sheet
        .add_card(
            "Group card",
            Vec2::new(-200.0, 0.0),
            Card::new(CardKind::Sticky, Vec2::new(120.0, 100.0)),
        )
        .unwrap();
    let object = shape(&mut sheet, 0.0, 0.0, "Group shape");
    let line = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Topic(card),
            Endpoint::Object(object),
        )))
        .unwrap();
    sheet
        .group_board_members(&[
            BoardMember::Card(card),
            BoardMember::Object(object),
            BoardMember::Object(line),
        ])
        .unwrap();
    let mut h = harness(sheet);
    let shape_point = point(&h, Vec2::ZERO);
    click(&mut h, shape_point, Modifiers::NONE);
    let original = h.state().doc.workbook().clone();
    h.key_press_modifiers(Modifiers::ALT, Key::ArrowDown);
    h.run_steps(3);
    assert_eq!(h.state().view.selection.len(), 1);
    assert_eq!(h.state().view.board_selection.len(), 2);
    let first_card = h.state().view.selection[0];
    let first_group = h.state().view.board_selection.clone();
    let first = h.state().doc.workbook().clone();
    h.key_press_modifiers(Modifiers::ALT, Key::ArrowDown);
    h.run_steps(3);
    assert_eq!(h.state().view.selection.len(), 1);
    assert_eq!(h.state().view.board_selection.len(), 2);
    let second_card = h.state().view.selection[0];
    assert_ne!(second_card, first_card);
    assert!(
        h.state()
            .view
            .board_selection
            .iter()
            .all(|id| !first_group.contains(id))
    );
    assert_eq!(
        h.state().doc.sheet().topic(second_card).unwrap().position,
        Some(Vec2::new(-200.0, 20.0))
    );
    let second = h.state().doc.workbook().clone();
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &first);
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &original);
    assert!(!h.state().doc.can_undo());
    assert!(h.state_mut().doc.redo());
    assert!(h.state_mut().doc.redo());
    assert_eq!(h.state().doc.workbook(), &second);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn board_alt_arrow_from_focused_handles_copies_instead_of_editing() {
    for label in ["Resize board item", "Rotate board item"] {
        let mut sheet = board();
        let id = shape(&mut sheet, 0.0, 0.0, "Handle");
        let mut h = harness(sheet);
        let start = point(&h, Vec2::ZERO);
        click(&mut h, start, Modifiers::NONE);
        h.get_by_label(label).focus();
        h.run_steps(2);
        let original = h.state().doc.sheet().canvas.object(id).unwrap().clone();
        h.key_press_modifiers(Modifiers::ALT, Key::ArrowRight);
        h.run_steps(3);
        assert_eq!(h.state().doc.sheet().canvas.object(id), Some(&original));
        let copied = h.state().view.board_selection[0];
        assert_ne!(copied, id);
        assert_eq!(
            h.state().doc.sheet().canvas.object(copied).unwrap().rect,
            original.rect.translate(Vec2::new(10.0, 0.0))
        );
        assert!(h.get_by_label("Rectangle: Handle, Selected").is_focused());
    }
}

#[test]
fn board_alt_arrow_from_focused_connector_endpoint_copies_without_retargeting() {
    let mut sheet = board();
    let id = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Point(Vec2::new(-80.0, 0.0)),
            Endpoint::Point(Vec2::new(80.0, 0.0)),
        )))
        .unwrap();
    let mut h = harness(sheet);
    let midpoint = point(&h, Vec2::ZERO);
    click(&mut h, midpoint, Modifiers::NONE);
    h.get_by_label("Connector From").focus();
    h.run_steps(2);
    let original = h.state().doc.sheet().canvas.object(id).unwrap().clone();
    h.key_press_modifiers(Modifiers::ALT, Key::ArrowDown);
    h.run_steps(3);
    assert_eq!(h.state().doc.sheet().canvas.object(id), Some(&original));
    let copied = h.state().view.board_selection[0];
    assert_ne!(copied, id);
    let CanvasObjectKind::Connector(connector) =
        &h.state().doc.sheet().canvas.object(copied).unwrap().kind
    else {
        panic!("expected copied connector");
    };
    assert_eq!(connector.from, Endpoint::Point(Vec2::new(-80.0, 10.0)));
    assert_eq!(connector.to, Endpoint::Point(Vec2::new(80.0, 10.0)));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn board_alt_gestures_respect_locks_and_cancel_without_an_edit() {
    let mut sheet = board();
    let first = shape(&mut sheet, -200.0, 0.0, "Locked");
    let second = shape(&mut sheet, 0.0, 0.0, "Grouped");
    sheet
        .group_board_members(&[BoardMember::Object(first), BoardMember::Object(second)])
        .unwrap();
    sheet
        .set_board_members_locked(&[BoardMember::Object(first)], true)
        .unwrap();
    let mut h = harness(sheet);
    let start = point(&h, Vec2::ZERO);
    let end = start + vec2(70.0, 30.0);
    click(&mut h, start, Modifiers::NONE);
    let before = h.state().doc.workbook().clone();
    h.key_press_modifiers(Modifiers::ALT, Key::ArrowRight);
    h.run_steps(2);
    begin_alt_drag(&mut h, start, end);
    finish_alt_drag(&mut h, end, Modifiers::ALT);
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(!h.state().doc.can_undo());
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);

    {
        let state = h.state_mut();
        apply_board_action(&mut state.doc, &mut state.view, BoardAction::Lock(false)).unwrap();
    }
    let unlocked = h.state().doc.workbook().clone();
    begin_alt_drag(&mut h, start, end);
    h.key_press(Key::Escape);
    h.run_steps(1);
    finish_alt_drag(&mut h, end, Modifiers::ALT);
    assert_eq!(h.state().doc.workbook(), &unlocked);
    assert_eq!(h.state().doc.undo_label(), Some("Unlock"));
}

#[test]
fn board_alt_drag_cancels_when_selection_camera_tool_or_pointer_changes() {
    for change in 0..5 {
        let mut sheet = board();
        let id = shape(&mut sheet, 0.0, 0.0, "Source");
        let other = shape(&mut sheet, 300.0, 0.0, "Other");
        let mut h = harness(sheet);
        let start = point(&h, Vec2::ZERO);
        let end = start + vec2(70.0, 30.0);
        click(&mut h, start, Modifiers::NONE);
        let before = h.state().doc.workbook().clone();
        begin_alt_drag(&mut h, start, end);
        match change {
            0 => h.state_mut().view.board_selection = vec![other],
            1 => h.state_mut().view.offset += vec2(10.0, 0.0),
            2 => h.state_mut().view.set_board_tool(BoardTool::Hand),
            3 => h.event(egui::Event::PointerGone),
            _ => h
                .state_mut()
                .doc
                .update_canvas_object("Concurrent label change", other, |object| {
                    if let CanvasObjectKind::Shape { label, .. } = &mut object.kind {
                        *label = "Changed".into();
                    }
                })
                .unwrap(),
        }
        let expected = h.state().doc.workbook().clone();
        h.run_steps(1);
        finish_alt_drag(&mut h, end, Modifiers::ALT);
        assert_eq!(
            h.state().doc.workbook(),
            &expected,
            "unexpected document change for cancellation case {change}"
        );
        assert_eq!(h.state().doc.sheet().canvas.objects.len(), 2);
        assert!(h.state().doc.sheet().canvas.object(id).is_some());
        if change == 4 {
            assert_eq!(h.state().doc.undo_label(), Some("Concurrent label change"));
            assert!(h.state_mut().doc.undo());
            assert_eq!(h.state().doc.workbook(), &before);
        }
        assert!(!h.state().doc.can_undo());
    }
}

#[test]
fn board_alt_drag_cancels_if_the_selected_frame_changes_mid_gesture() {
    let mut sheet = board();
    let first = sheet
        .create_board_frame(
            "First",
            thinoda_core::Rect::from_min_size(Vec2::new(-250.0, -100.0), Vec2::new(200.0, 200.0)),
        )
        .unwrap();
    let second = sheet
        .create_board_frame(
            "Second",
            thinoda_core::Rect::from_min_size(Vec2::new(50.0, -100.0), Vec2::new(200.0, 200.0)),
        )
        .unwrap();
    let mut h = harness(sheet);
    let before = h.state().doc.workbook().clone();
    let start = point(&h, Vec2::new(-150.0, -100.0));
    let end = start + vec2(60.0, 40.0);
    begin_alt_drag(&mut h, start, end);
    assert_eq!(
        h.state().view.selected_object,
        Some(ObjectSelection::Zone(first))
    );
    h.state_mut()
        .view
        .select_object(ObjectSelection::Zone(second));
    h.run_steps(1);
    finish_alt_drag(&mut h, end, Modifiers::ALT);
    assert_eq!(h.state().doc.workbook(), &before);
    assert_eq!(
        h.state().view.selected_object,
        Some(ObjectSelection::Zone(second))
    );
    assert!(!h.state().doc.can_undo());
}

#[test]
fn board_alt_duplication_preserves_retained_pending_ink() {
    let mut sheet = board();
    let id = shape(&mut sheet, 200.0, 0.0, "Source");
    let mut h = harness(sheet);
    h.state_mut().view.ink_settings.finish.tolerance = -1.0;
    set_tool(&mut h, BoardTool::Pen);
    let start = point(&h, Vec2::new(-200.0, -100.0));
    let end = start + vec2(80.0, 0.0);
    drag(&mut h, start, end);
    finish_drag(&mut h, end);
    assert!(h.state().view.has_released_ink_gesture());
    let samples = h.state().view.pending_ink_samples().to_vec();
    assert!(samples.len() >= 2);
    let before = h.state().doc.workbook().clone();
    {
        let state = h.state_mut();
        state.view.set_board_tool(BoardTool::Select);
        state.view.board_selection = vec![id];
        let error = board_actions::duplicate_selection(
            &mut state.doc,
            &mut state.view,
            Vec2::new(10.0, 0.0),
        )
        .unwrap_err();
        assert!(error.to_string().contains("pending ink"));
    }
    h.ctx
        .memory_mut(|memory| memory.request_focus(MapCanvas::id()));
    h.run_steps(1);
    h.key_press_modifiers(Modifiers::ALT, Key::ArrowRight);
    h.run_steps(2);
    assert_eq!(h.state().doc.workbook(), &before);
    assert_eq!(h.state().view.pending_ink_samples(), samples);
    assert!(h.state().view.has_released_ink_gesture());
    assert!(!h.state().doc.can_undo());
}

#[test]
fn board_alt_arrow_reports_coordinate_errors_without_losing_redo_or_selection() {
    let mut sheet = board();
    let id = shape(&mut sheet, 999_940.0, 0.0, "Edge");
    let mut h = harness(sheet);
    {
        let state = h.state_mut();
        state
            .doc
            .update_canvas_object("Change label", id, |object| {
                if let CanvasObjectKind::Shape { label, .. } = &mut object.kind {
                    *label = "Changed".into();
                }
            })
            .unwrap();
        assert!(state.doc.undo());
        state.view.board_selection = vec![id];
    }
    h.ctx
        .memory_mut(|memory| memory.request_focus(MapCanvas::id()));
    h.run_steps(1);
    let before = h.state().doc.workbook().clone();
    h.key_press_modifiers(Modifiers::ALT, Key::ArrowRight);
    h.run_steps(2);
    assert_eq!(h.state().doc.workbook(), &before);
    assert_eq!(h.state().view.board_selection, vec![id]);
    assert!(h.state().doc.can_redo());
    assert!(
        h.state()
            .errors
            .iter()
            .any(|error| error.to_string().contains("range")
                || error.to_string().contains("coordinate")
                || error.to_string().contains("invalid")),
        "{:?}",
        h.state().errors
    );
}

#[test]
fn board_alt_drag_and_arrow_duplicate_a_persistent_empty_frame() {
    let mut sheet = board();
    let rect =
        thinoda_core::Rect::from_min_size(Vec2::new(-200.0, -100.0), Vec2::new(400.0, 200.0));
    let frame = sheet.create_board_frame("Empty", rect).unwrap();
    let mut h = harness(sheet);
    let start = point(&h, rect.min + Vec2::new(100.0, 0.0));
    let end = start + vec2(60.0, 40.0);
    let before = h.state().doc.workbook().clone();
    begin_alt_drag(&mut h, start, end);
    assert_eq!(h.state().doc.workbook(), &before);
    finish_alt_drag(&mut h, end, Modifiers::ALT);
    let ObjectSelection::Zone(copied) = h.state().view.selected_object.unwrap() else {
        panic!("expected copied frame");
    };
    assert_ne!(copied, frame);
    assert_eq!(
        h.state().doc.sheet().zones[1].frame,
        rect.translate(Vec2::new(60.0, 40.0))
    );
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(!h.state().doc.can_undo());

    h.state_mut()
        .view
        .select_object(ObjectSelection::Zone(frame));
    h.ctx
        .memory_mut(|memory| memory.request_focus(MapCanvas::id()));
    h.run_steps(1);
    h.key_press_modifiers(Modifiers::ALT, Key::ArrowDown);
    h.run_steps(2);
    assert_eq!(
        h.state().doc.sheet().zones[1].frame,
        rect.translate(Vec2::new(0.0, 10.0))
    );
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn board_alt_shortcuts_leave_legacy_tree_reordering_unchanged() {
    for hidden in [false, true] {
        let mut sheet = Sheet::new("Map", "Root");
        let parent = if hidden {
            sheet.root_mode = SheetRootMode::Hidden;
            sheet.add_floating("Tree", Vec2::ZERO)
        } else {
            sheet.root
        };
        let first = sheet.add_child(parent, None, "First");
        let second = sheet.add_child(parent, None, "Second");
        let mut h = harness(sheet);
        h.state_mut().view.select_only(second);
        h.ctx
            .memory_mut(|memory| memory.request_focus(MapCanvas::id()));
        h.run_steps(2);
        h.key_press_modifiers(Modifiers::ALT, Key::ArrowUp);
        h.run_steps(2);
        assert_eq!(h.state().doc.sheet().children(parent), &[second, first]);
        assert!(h.state_mut().doc.undo());
        assert_eq!(h.state().doc.sheet().children(parent), &[first, second]);
        assert!(!h.state().doc.can_undo());
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

fn touch(h: &mut Harness<'_, State>, phase: egui::TouchPhase, position: Pos2) {
    h.event(egui::Event::PointerMoved(position));
    h.event(egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(1),
        phase,
        pos: position,
        force: None,
    });
    if matches!(phase, egui::TouchPhase::Start | egui::TouchPhase::End) {
        h.event(egui::Event::PointerButton {
            pos: position,
            button: PointerButton::Primary,
            pressed: phase == egui::TouchPhase::Start,
            modifiers: Modifiers::NONE,
        });
    }
    h.run_steps(1);
}

fn connector<'a>(h: &'a Harness<'_, State>, id: CanvasObjectId) -> &'a CanvasConnector {
    let CanvasObjectKind::Connector(connector) =
        &h.state().doc.sheet().canvas.object(id).unwrap().kind
    else {
        panic!("expected connector");
    };
    connector
}

#[test]
fn mouse_shape_creation_and_drag_are_each_single_undoable_edits() {
    let mut h = harness(board());
    set_tool(&mut h, BoardTool::Shape(ShapeKind::Diamond));
    let before = h.state().doc.workbook().clone();
    let start = point(&h, Vec2::new(-200.0, -100.0));
    let end = point(&h, Vec2::new(-40.0, 20.0));
    drag(&mut h, start, end);
    assert_eq!(h.state().doc.workbook(), &before);
    finish_drag(&mut h, end);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    let object = &h.state().doc.sheet().canvas.objects[0];
    assert!(matches!(
        object.kind,
        CanvasObjectKind::Shape {
            shape: ShapeKind::Diamond,
            ..
        }
    ));
    assert_eq!(
        object.rect,
        thinoda_core::Rect::from_min_size(Vec2::new(-200.0, -100.0), Vec2::new(160.0, 120.0))
    );
    let id = object.id;
    let created = h.state().doc.workbook().clone();
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(!h.state_mut().doc.undo());
    assert!(h.state_mut().doc.redo());
    h.run_steps(2);
    set_tool(&mut h, BoardTool::Select);
    let center = point(&h, Vec2::new(-120.0, -40.0));
    let end = center + vec2(70.0, 35.0);
    drag(&mut h, center, end);
    assert_eq!(h.state().doc.workbook(), &created);
    finish_drag(&mut h, end);
    assert_eq!(
        h.state().doc.sheet().canvas.object(id).unwrap().rect.min,
        Vec2::new(-130.0, -65.0)
    );
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &created);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn armed_shape_click_on_content_selects_and_shows_handles_without_creating() {
    let mut sheet = board();
    let existing = shape(&mut sheet, 0.0, 0.0, "Existing");
    let mut h = harness(sheet);
    set_tool(&mut h, BoardTool::Shape(ShapeKind::Ellipse));
    let before = h.state().doc.workbook().clone();
    let center = point(&h, Vec2::ZERO);
    h.hover_at(center);
    h.run_steps(2);
    assert!(h.state().view.board_input.armed_hover);
    assert_eq!(h.state().view.board_input.ghost, None);

    click(&mut h, center, Modifiers::NONE);
    assert_eq!(h.state().doc.workbook(), &before);
    assert_eq!(h.state().view.board_selection, vec![existing]);
    assert_eq!(
        h.state().view.board_tool,
        BoardTool::Shape(ShapeKind::Ellipse)
    );
    assert!(h.query_by_label("Resize board item").is_some());
    assert!(h.query_by_label("Rotate board item").is_some());

    let end = center + vec2(70.0, 35.0);
    drag(&mut h, center, end);
    assert_eq!(h.state().doc.workbook(), &before);
    finish_drag(&mut h, end);
    assert_eq!(h.state().doc.sheet().canvas.objects.len(), 1);
    assert_eq!(
        h.state()
            .doc
            .sheet()
            .canvas
            .object(existing)
            .unwrap()
            .rect
            .min,
        Vec2::new(10.0, -5.0)
    );
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn armed_shape_click_on_empty_space_creates_one_undoable_shape_and_shows_a_ghost() {
    let mut sheet = board();
    shape(&mut sheet, 0.0, 0.0, "Existing");
    let mut h = harness(sheet);
    set_tool(&mut h, BoardTool::Shape(ShapeKind::Ellipse));
    let before = h.state().doc.workbook().clone();
    let empty = point(&h, Vec2::new(-360.0, -240.0));
    h.hover_at(empty);
    h.run_steps(2);
    assert!(!h.state().view.board_input.armed_hover);
    assert_eq!(h.state().view.board_input.ghost, Some(empty));

    click(&mut h, empty, Modifiers::NONE);
    let objects = &h.state().doc.sheet().canvas.objects;
    assert_eq!(objects.len(), 2);
    assert!(objects.iter().any(|object| matches!(
        object.kind,
        CanvasObjectKind::Shape {
            shape: ShapeKind::Ellipse,
            ..
        }
    )));
    assert_eq!(
        h.state().view.board_tool,
        BoardTool::Shape(ShapeKind::Ellipse)
    );
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn double_ellipse_armed_ghost_matches_the_created_inner_outline() {
    let mut h = harness(board());
    set_tool(&mut h, BoardTool::Shape(ShapeKind::DoubleEllipse));
    let empty = point(&h, Vec2::new(-300.0, -200.0));
    h.hover_at(empty);
    h.run_steps(3);
    let ghost_paths: Vec<Vec<Pos2>> = h
        .output()
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Path(path) if !path.closed && path.points.len() == 49 => {
                Some(path.points.clone())
            }
            _ => None,
        })
        .collect();
    click(&mut h, empty, Modifiers::NONE);
    let object = &h.state().doc.sheet().canvas.objects[0];
    let expected = thinoda_core::board::shape_geometry_with_style(
        ShapeKind::DoubleEllipse,
        object.rect,
        object.rotation,
        object.rounding,
        object.style.effective_shape_stroke_width(),
        object.double_ellipse_margin,
    )
    .details
    .remove(0)
    .into_iter()
    .map(|point| h.state().view.xf().pos(point))
    .collect::<Vec<_>>();
    assert!(
        ghost_paths.iter().any(|path| {
            path.iter()
                .zip(&expected)
                .all(|(actual, expected)| actual.distance(*expected) < 0.001)
        }),
        "the ghost must use the final shape's absolute inset"
    );
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

/// One-finger tap with the touch and pointer events of each phase in the same
/// frame, as platform backends deliver them.
fn tap(h: &mut Harness<'_, State>, position: Pos2) {
    for (phase, pressed) in [
        (egui::TouchPhase::Start, true),
        (egui::TouchPhase::End, false),
    ] {
        h.input_mut().events.extend([
            egui::Event::PointerMoved(position),
            egui::Event::Touch {
                device_id: egui::TouchDeviceId(1),
                id: egui::TouchId(1),
                phase,
                pos: position,
                force: None,
            },
            egui::Event::PointerButton {
                pos: position,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            },
        ]);
        h.run_steps(1);
    }
    h.run_steps(2);
}

#[test]
fn armed_shape_touch_tap_selects_content_and_creates_on_empty_space() {
    let mut sheet = board();
    let existing = shape(&mut sheet, 0.0, 0.0, "Existing");
    let mut h = harness(sheet);
    set_tool(&mut h, BoardTool::Shape(ShapeKind::Rectangle));
    let before = h.state().doc.workbook().clone();
    let center = point(&h, Vec2::ZERO);
    tap(&mut h, center);
    assert_eq!(h.state().doc.workbook(), &before);
    assert_eq!(h.state().view.board_selection, vec![existing]);

    let empty = point(&h, Vec2::new(300.0, 220.0));
    tap(&mut h, empty);
    assert_eq!(h.state().doc.sheet().canvas.objects.len(), 2);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn armed_shape_exits_with_escape_or_v_and_edits_a_selection_with_enter() {
    let mut sheet = board();
    let existing = shape(&mut sheet, 0.0, 0.0, "Existing");
    let mut h = harness(sheet);
    for key in [Key::V, Key::Escape] {
        set_tool(&mut h, BoardTool::Shape(ShapeKind::Hexagon));
        // Keep repeated clicks on the same shape out of the double-click window.
        h.run_steps(30);
        let center = point(&h, Vec2::ZERO);
        click(&mut h, center, Modifiers::NONE);
        assert_eq!(h.state().view.board_selection, vec![existing]);
        h.key_press(key);
        h.run_steps(2);
        assert_eq!(h.state().view.board_tool, BoardTool::Select, "{key:?}");
        assert_eq!(h.state().doc.sheet().canvas.objects.len(), 1);
    }

    set_tool(&mut h, BoardTool::Shape(ShapeKind::Hexagon));
    h.run_steps(30);
    let center = point(&h, Vec2::ZERO);
    click(&mut h, center, Modifiers::NONE);
    assert_eq!(h.state().view.board_selection, vec![existing]);
    let objects = h.state().doc.sheet().canvas.objects.len();
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert_eq!(h.state().doc.sheet().canvas.objects.len(), objects);
    assert_eq!(h.state().view.editing_board_object(), Some(existing));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

fn release(h: &mut Harness<'_, State>, position: Pos2) {
    h.event(egui::Event::PointerButton {
        pos: position,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(2);
}

fn object_rects(h: &Harness<'_, State>) -> Vec<thinoda_core::Rect> {
    h.state()
        .doc
        .sheet()
        .canvas
        .objects
        .iter()
        .map(|object| object.rect)
        .collect()
}

#[test]
fn library_drop_previews_then_creates_one_undoable_shape_without_changing_the_tool() {
    let mut sheet = board();
    shape(&mut sheet, 0.0, 0.0, "Existing");
    let mut h = harness(sheet);
    let before = h.state().doc.workbook().clone();
    egui::DragAndDrop::set_payload(&h.ctx, ShapeDrop(ShapeKind::Ellipse));
    let target = point(&h, Vec2::new(-360.0, -240.0));
    h.hover_at(target);
    h.run_steps(2);
    assert_eq!(
        h.state().view.board_input.drop_ghost,
        Some((ShapeKind::Ellipse, target))
    );
    assert_eq!(h.state().doc.workbook(), &before);

    release(&mut h, target);
    let objects = &h.state().doc.sheet().canvas.objects;
    assert_eq!(objects.len(), 2);
    let created = objects.last().unwrap();
    assert!(matches!(
        created.kind,
        CanvasObjectKind::Shape {
            shape: ShapeKind::Ellipse,
            ..
        }
    ));
    let board = &h.state().doc.sheet().board;
    let expected = if board.snap {
        let grid = board.grid_size;
        Vec2::new(
            (-360.0 / grid).round() * grid,
            (-240.0 / grid).round() * grid,
        )
    } else {
        Vec2::new(-360.0, -240.0)
    };
    assert!((created.rect.center() - expected).length() < 0.01);
    assert_eq!(h.state().view.board_selection, vec![created.id]);
    assert_eq!(h.state().view.board_tool, BoardTool::Select);
    assert_eq!(h.state().view.board_input.drop_ghost, None);
    assert!(egui::DragAndDrop::payload::<ShapeDrop>(&h.ctx).is_none());
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn library_drop_is_canceled_by_escape_or_obscured_targets() {
    let overlay = Rect::from_min_size(pos2(40.0, 40.0), vec2(120.0, 120.0));
    let mut h = harness_with_overlay(board(), Some(overlay));
    set_tool(&mut h, BoardTool::Shape(ShapeKind::Hexagon));
    let before = h.state().doc.workbook().clone();

    egui::DragAndDrop::set_payload(&h.ctx, ShapeDrop(ShapeKind::Ellipse));
    let target = point(&h, Vec2::new(200.0, 160.0));
    h.hover_at(target);
    h.run_steps(2);
    assert!(h.state().view.board_input.drop_ghost.is_some());
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert_eq!(h.state().view.board_input.drop_ghost, None);
    release(&mut h, target);
    assert_eq!(h.state().doc.workbook(), &before);
    // Escape canceled only the drag; the armed tool stays armed.
    assert_eq!(
        h.state().view.board_tool,
        BoardTool::Shape(ShapeKind::Hexagon)
    );

    egui::DragAndDrop::set_payload(&h.ctx, ShapeDrop(ShapeKind::Ellipse));
    h.hover_at(overlay.center());
    h.run_steps(2);
    assert_eq!(h.state().view.board_input.drop_ghost, None);
    release(&mut h, overlay.center());
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(egui::DragAndDrop::payload::<ShapeDrop>(&h.ctx).is_none());
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn inserted_shapes_land_in_free_space_beside_the_selection() {
    let mut sheet = board();
    let mut h = harness(board());
    let center = {
        let view = &h.state().view;
        view.xf().sheet(view.canvas_rect.center())
    };
    shape(&mut sheet, center.x, center.y, "Blocker");
    h.state_mut().doc = Document::new(Workbook::new(sheet));
    h.run_steps(2);
    let before = h.state().doc.workbook().clone();

    let state = h.state_mut();
    let first = state
        .view
        .insert_board_shape(&mut state.doc, ShapeKind::Diamond)
        .unwrap();
    let second = state
        .view
        .insert_board_shape(&mut state.doc, ShapeKind::Cloud)
        .unwrap();
    assert_eq!(state.view.board_selection, vec![second]);
    assert_eq!(state.view.board_tool, BoardTool::Select);
    let sheet = state.doc.sheet();
    let rect = |id| sheet.canvas.object(id).unwrap().rect;
    let blocker = sheet.canvas.objects[0].rect;
    for id in [first, second] {
        assert!(!rect(id).expand(4.0).intersects(&blocker), "{id:?}");
    }
    assert!(!rect(first).expand(4.0).intersects(&rect(second)));
    // Same row, each one to the right of what was selected before it.
    assert!(rect(first).center().x > blocker.center().x);
    assert!(rect(second).center().x > rect(first).center().x);
    assert!((rect(second).center().y - rect(first).center().y).abs() < 0.01);
    assert!(state.doc.undo());
    assert_eq!(state.doc.sheet().canvas.objects.len(), 2);
    assert!(state.doc.undo());
    assert_eq!(state.doc.workbook(), &before);
}

#[test]
fn armed_keyboard_insert_avoids_content_at_the_viewport_centre() {
    let mut sheet = board();
    let mut h = harness(board());
    let center = {
        let view = &h.state().view;
        view.xf().sheet(view.canvas_rect.center())
    };
    shape(&mut sheet, center.x, center.y, "Blocker");
    h.state_mut().doc = Document::new(Workbook::new(sheet));
    h.run_steps(2);
    set_tool(&mut h, BoardTool::Shape(ShapeKind::Ellipse));
    h.key_press(Key::Enter);
    h.run_steps(2);
    let rects = object_rects(&h);
    assert_eq!(rects.len(), 2);
    assert!(!rects[1].expand(4.0).intersects(&rects[0]));
    assert_eq!(
        h.state().view.board_tool,
        BoardTool::Shape(ShapeKind::Ellipse)
    );
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn touch_creates_and_moves_a_card_without_reparenting() {
    let mut h = harness(board());
    set_tool(&mut h, BoardTool::Sticky);
    let start = point(&h, Vec2::new(-180.0, -110.0));
    let end = point(&h, Vec2::new(0.0, 40.0));
    touch(&mut h, egui::TouchPhase::Start, start);
    touch(&mut h, egui::TouchPhase::Move, start + vec2(90.0, 75.0));
    touch(&mut h, egui::TouchPhase::Move, end);
    assert!(h.state().doc.sheet().floating.is_empty());
    touch(&mut h, egui::TouchPhase::End, end);
    h.run_steps(3);
    let card = h.state().doc.sheet().floating[0];
    assert_eq!(
        h.state()
            .doc
            .sheet()
            .topic(card)
            .unwrap()
            .card
            .unwrap()
            .kind,
        CardKind::Sticky
    );
    assert!(!h.state().doc.sheet().is_anchor(card));
    h.key_press(Key::Enter);
    h.run_steps(3);
    set_tool(&mut h, BoardTool::Select);
    let before = h.state().doc.workbook().clone();
    let start = point(&h, Vec2::new(-90.0, -35.0));
    let end = start + vec2(50.0, 30.0);
    touch(&mut h, egui::TouchPhase::Start, start);
    touch(&mut h, egui::TouchPhase::Move, start + vec2(25.0, 15.0));
    touch(&mut h, egui::TouchPhase::Move, end);
    assert_eq!(h.state().doc.workbook(), &before);
    touch(&mut h, egui::TouchPhase::End, end);
    h.run_steps(3);
    assert_eq!(
        h.state().doc.sheet().topic(card).unwrap().position,
        Some(Vec2::new(-40.0, -5.0))
    );
    assert_eq!(h.state().doc.sheet().parent(card), None);
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn frame_tool_uses_persistent_empty_zone_and_localized_name() {
    let mut h = harness(board());
    h.state_mut().view.board_labels.new_frame = "Area".into();
    set_tool(&mut h, BoardTool::Frame);
    let start = point(&h, Vec2::new(-250.0, -180.0));
    let end = point(&h, Vec2::new(200.0, 160.0));
    drag(&mut h, start, end);
    finish_drag(&mut h, end);
    let frame = &h.state().doc.sheet().zones[0];
    assert_eq!(frame.name, "Area");
    assert!(frame.keep_empty && frame.roots.is_empty());
    assert_eq!(
        h.state().view.selected_object,
        Some(ObjectSelection::Zone(frame.id))
    );
    assert!(h.state_mut().doc.undo());
    assert!(h.state().doc.sheet().zones.is_empty());
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

fn assert_drag_created(h: &Harness<'_, State>, tool: BoardTool) {
    let start = Vec2::new(-200.0, -100.0);
    let end = Vec2::new(100.0, 100.0);
    let size = end - start;
    match tool {
        BoardTool::Sticky | BoardTool::Text => {
            assert_eq!(h.state().doc.sheet().floating.len(), 1, "{tool:?}");
            let topic = h
                .state()
                .doc
                .sheet()
                .topic(h.state().doc.sheet().floating[0])
                .unwrap();
            assert_eq!(topic.position, Some((start + end) * 0.5), "{tool:?}");
            assert_eq!(topic.card.unwrap().size, size, "{tool:?}");
        }
        BoardTool::Frame => {
            assert_eq!(h.state().doc.sheet().zones.len(), 1);
            let frame = &h.state().doc.sheet().zones[0];
            assert_eq!(frame.frame.min, start);
            assert_eq!(frame.frame.size, size);
        }
        BoardTool::Shape(_) => {
            assert_eq!(h.state().doc.sheet().canvas.objects.len(), 1);
            let object = &h.state().doc.sheet().canvas.objects[0];
            assert_eq!(object.rect.min, start);
            assert_eq!(object.rect.size, size);
        }
        BoardTool::Connector => {
            assert_eq!(h.state().doc.sheet().canvas.objects.len(), 1);
            let CanvasObjectKind::Connector(connector) =
                &h.state().doc.sheet().canvas.objects[0].kind
            else {
                panic!("expected connector");
            };
            assert_eq!(connector.from, Endpoint::Point(start));
            assert_eq!(connector.to, Endpoint::Point(end));
        }
        _ => unreachable!(),
    }
}

#[test]
fn same_frame_creation_drags_are_not_lost() {
    for tool in [
        BoardTool::Sticky,
        BoardTool::Text,
        BoardTool::Shape(ShapeKind::Rectangle),
        BoardTool::Frame,
        BoardTool::Connector,
    ] {
        let mut h = harness(board());
        set_tool(&mut h, tool);
        let before = h.state().doc.workbook().clone();
        let start = point(&h, Vec2::new(-200.0, -100.0));
        let end = point(&h, Vec2::new(100.0, 100.0));
        h.input_mut().events.extend([
            egui::Event::PointerMoved(start),
            egui::Event::PointerButton {
                pos: start,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
            egui::Event::PointerMoved(end),
            egui::Event::PointerButton {
                pos: end,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ]);
        h.run_steps(3);
        assert_drag_created(&h, tool);
        assert!(h.state_mut().doc.undo(), "{tool:?}");
        assert_eq!(h.state().doc.workbook(), &before, "{tool:?}");
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn release_frame_movement_after_canvas_press_is_not_lost() {
    for tool in [
        BoardTool::Sticky,
        BoardTool::Text,
        BoardTool::Shape(ShapeKind::Rectangle),
        BoardTool::Frame,
        BoardTool::Connector,
    ] {
        let mut h = harness(board());
        set_tool(&mut h, tool);
        let before = h.state().doc.workbook().clone();
        let start = point(&h, Vec2::new(-200.0, -100.0));
        let end = point(&h, Vec2::new(100.0, 100.0));
        h.hover_at(start);
        h.event(egui::Event::PointerButton {
            pos: start,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(1);
        h.input_mut().events.extend([
            egui::Event::PointerMoved(end),
            egui::Event::PointerButton {
                pos: end,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ]);
        h.run_steps(3);
        assert_drag_created(&h, tool);
        assert!(h.state_mut().doc.undo(), "{tool:?}");
        assert_eq!(h.state().doc.workbook(), &before, "{tool:?}");
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn batched_creation_drag_does_not_start_on_an_overlapping_control() {
    let start = pos2(400.0, 350.0);
    let end = pos2(650.0, 500.0);
    let overlay = Rect::from_center_size(start, vec2(80.0, 80.0));
    let mut h = harness_with_overlay(board(), Some(overlay));
    set_tool(&mut h, BoardTool::Shape(ShapeKind::Rectangle));
    let before = h.state().doc.workbook().clone();
    h.input_mut().events.extend([
        egui::Event::PointerMoved(start),
        egui::Event::PointerButton {
            pos: start,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        egui::Event::PointerMoved(end),
        egui::Event::PointerButton {
            pos: end,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    ]);
    h.run_steps(3);
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn batched_creation_drag_does_not_end_on_an_overlapping_control() {
    let start = pos2(400.0, 350.0);
    let end = pos2(650.0, 500.0);
    let overlay = Rect::from_center_size(end, vec2(80.0, 80.0));
    let mut h = harness_with_overlay(board(), Some(overlay));
    set_tool(&mut h, BoardTool::Shape(ShapeKind::Rectangle));
    let before = h.state().doc.workbook().clone();
    h.input_mut().events.extend([
        egui::Event::PointerMoved(start),
        egui::Event::PointerButton {
            pos: start,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        egui::Event::PointerMoved(end),
        egui::Event::PointerButton {
            pos: end,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    ]);
    h.run_steps(3);
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn batched_creation_click_does_not_start_on_an_overlapping_control() {
    let start = pos2(438.0, 350.0);
    let end = pos2(442.0, 350.0);
    let overlay = Rect::from_center_size(pos2(400.0, 350.0), vec2(80.0, 80.0));
    let mut h = harness_with_overlay(board(), Some(overlay));
    set_tool(&mut h, BoardTool::Shape(ShapeKind::Rectangle));
    let before = h.state().doc.workbook().clone();
    h.input_mut().events.extend([
        egui::Event::PointerMoved(start),
        egui::Event::PointerButton {
            pos: start,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        egui::Event::PointerMoved(end),
        egui::Event::PointerButton {
            pos: end,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    ]);
    h.run_steps(3);
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn batched_creation_respects_the_configured_click_distance() {
    let mut h = harness(board());
    set_tool(&mut h, BoardTool::Shape(ShapeKind::Rectangle));
    h.ctx
        .options_mut(|options| options.input_options.max_click_dist = 30.0);
    let start = point(&h, Vec2::new(-200.0, -100.0));
    let end = start + vec2(15.0, 10.0);
    h.input_mut().events.extend([
        egui::Event::PointerMoved(start),
        egui::Event::PointerButton {
            pos: start,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        egui::Event::PointerMoved(end),
        egui::Event::PointerButton {
            pos: end,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    ]);
    h.run_steps(3);
    let objects = &h.state().doc.sheet().canvas.objects;
    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].rect.size, Vec2::new(160.0, 100.0));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn batched_creation_rejects_cancellation_and_multiple_pointers() {
    let start = pos2(400.0, 350.0);
    let end = pos2(650.0, 500.0);
    let press = egui::Event::PointerButton {
        pos: start,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    };
    let release = egui::Event::PointerButton {
        pos: end,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    };
    let touch = |id, phase, pos| egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(id),
        phase,
        pos,
        force: None,
    };
    let cases = [
        (
            "pointer gone before release",
            vec![
                press.clone(),
                egui::Event::PointerMoved(end),
                egui::Event::PointerGone,
                release.clone(),
            ],
        ),
        (
            "touch cancellation after release",
            vec![
                press.clone(),
                touch(1, egui::TouchPhase::Start, start),
                egui::Event::PointerMoved(end),
                release.clone(),
                touch(1, egui::TouchPhase::Cancel, end),
            ],
        ),
        (
            "focus loss after release",
            vec![
                press.clone(),
                egui::Event::PointerMoved(end),
                release.clone(),
                egui::Event::WindowFocused(false),
            ],
        ),
        (
            "escape",
            vec![
                press.clone(),
                egui::Event::PointerMoved(end),
                egui::Event::Key {
                    key: Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                },
                release.clone(),
            ],
        ),
        (
            "second pointer button",
            vec![
                press.clone(),
                egui::Event::PointerButton {
                    pos: start,
                    button: PointerButton::Secondary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
                egui::Event::PointerMoved(end),
                release.clone(),
            ],
        ),
        (
            "duplicate primary press without a touch",
            vec![
                press.clone(),
                press.clone(),
                egui::Event::PointerMoved(end),
                release.clone(),
            ],
        ),
        (
            "second touch",
            vec![
                press.clone(),
                touch(1, egui::TouchPhase::Start, start),
                touch(2, egui::TouchPhase::Start, end),
                egui::Event::PointerMoved(end),
                touch(2, egui::TouchPhase::End, end),
                release.clone(),
                egui::Event::PointerGone,
                touch(1, egui::TouchPhase::End, end),
            ],
        ),
        (
            "release outside canvas",
            vec![
                press.clone(),
                egui::Event::PointerMoved(end),
                egui::Event::PointerButton {
                    pos: pos2(-20.0, -20.0),
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                },
            ],
        ),
    ];
    for (name, events) in cases {
        let mut h = harness(board());
        set_tool(&mut h, BoardTool::Shape(ShapeKind::Rectangle));
        let before = h.state().doc.workbook().clone();
        h.input_mut().events = events;
        h.run_steps(3);
        assert_eq!(h.state().doc.workbook(), &before, "{name}");
        assert!(!h.state_mut().doc.undo(), "{name}");
        assert!(
            h.state().errors.is_empty(),
            "{name}: {:?}",
            h.state().errors
        );
    }
}

#[test]
fn browser_touch_creation_survives_fully_batched_input() {
    for tool in [
        BoardTool::Sticky,
        BoardTool::Text,
        BoardTool::Shape(ShapeKind::Rectangle),
        BoardTool::Frame,
        BoardTool::Connector,
    ] {
        let mut h = harness(board());
        set_tool(&mut h, tool);
        let before = h.state().doc.workbook().clone();
        let start = point(&h, Vec2::new(-200.0, -100.0));
        let end = point(&h, Vec2::new(100.0, 100.0));
        let touch = |phase, pos| egui::Event::Touch {
            device_id: egui::TouchDeviceId(1),
            id: egui::TouchId(1),
            phase,
            pos,
            force: None,
        };
        h.input_mut().events.extend([
            egui::Event::PointerButton {
                pos: start,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
            touch(egui::TouchPhase::Start, start),
            egui::Event::PointerMoved(end),
            touch(egui::TouchPhase::Move, end),
            egui::Event::PointerButton {
                pos: end,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
            egui::Event::PointerGone,
            touch(egui::TouchPhase::End, end),
        ]);
        h.run_steps(3);
        assert_drag_created(&h, tool);
        assert!(h.state_mut().doc.undo(), "{tool:?}");
        assert_eq!(h.state().doc.workbook(), &before, "{tool:?}");
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn browser_pointer_and_touch_events_do_not_cancel_the_same_contact() {
    let mut h = harness(board());
    set_tool(&mut h, BoardTool::Shape(ShapeKind::Rectangle));
    let before = h.state().doc.workbook().clone();
    let start = point(&h, Vec2::new(-200.0, -100.0));
    let end = point(&h, Vec2::new(100.0, 100.0));
    let touch = |phase, pos| egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(1),
        phase,
        pos,
        force: None,
    };
    h.input_mut().events.push(egui::Event::PointerButton {
        pos: start,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(1);
    h.input_mut().events.extend([
        egui::Event::PointerButton {
            pos: start,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        touch(egui::TouchPhase::Start, start),
    ]);
    h.run_steps(1);
    h.input_mut().events.extend([
        egui::Event::PointerMoved(end),
        touch(egui::TouchPhase::Move, end),
        egui::Event::PointerButton {
            pos: end,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    ]);
    h.run_steps(1);
    h.input_mut().events.extend([
        egui::Event::PointerButton {
            pos: end,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
        egui::Event::PointerGone,
        touch(egui::TouchPhase::End, end),
    ]);
    h.run_steps(3);
    assert_drag_created(&h, BoardTool::Shape(ShapeKind::Rectangle));
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn canceled_browser_touch_does_not_block_the_next_contact() {
    let mut h = harness(board());
    set_tool(&mut h, BoardTool::Shape(ShapeKind::Rectangle));
    let before = h.state().doc.workbook().clone();
    let start = point(&h, Vec2::new(-200.0, -100.0));
    let end = point(&h, Vec2::new(100.0, 100.0));
    let touch = |id, phase, pos| egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(id),
        phase,
        pos,
        force: None,
    };
    h.input_mut().events.extend([
        egui::Event::PointerButton {
            pos: start,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        touch(1, egui::TouchPhase::Start, start),
    ]);
    h.run_steps(1);
    h.input_mut().events.extend([
        egui::Event::PointerMoved(end),
        touch(1, egui::TouchPhase::Move, end),
    ]);
    h.run_steps(1);
    h.input_mut()
        .events
        .push(touch(1, egui::TouchPhase::Cancel, end));
    h.run_steps(2);
    assert_eq!(h.state().doc.workbook(), &before);

    h.input_mut().events.extend([
        egui::Event::PointerButton {
            pos: start,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        touch(2, egui::TouchPhase::Start, start),
    ]);
    h.run_steps(1);
    h.input_mut().events.extend([
        egui::Event::PointerMoved(end),
        touch(2, egui::TouchPhase::Move, end),
        egui::Event::PointerButton {
            pos: end,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
        egui::Event::PointerGone,
        touch(2, egui::TouchPhase::End, end),
    ]);
    h.run_steps(3);
    assert_drag_created(&h, BoardTool::Shape(ShapeKind::Rectangle));
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn canceled_touch_and_next_contact_can_share_a_frame() {
    for batched_release in [false, true] {
        let mut h = harness(board());
        set_tool(&mut h, BoardTool::Shape(ShapeKind::Rectangle));
        let before = h.state().doc.workbook().clone();
        let start = point(&h, Vec2::new(-200.0, -100.0));
        let end = point(&h, Vec2::new(100.0, 100.0));
        let touch = |id, phase, pos| egui::Event::Touch {
            device_id: egui::TouchDeviceId(1),
            id: egui::TouchId(id),
            phase,
            pos,
            force: None,
        };
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        h.input_mut().events.extend([
            button(start, true),
            touch(1, egui::TouchPhase::Start, start),
        ]);
        h.run_steps(1);
        h.input_mut().events.extend([
            egui::Event::PointerMoved(end),
            touch(1, egui::TouchPhase::Move, end),
        ]);
        h.run_steps(1);
        h.input_mut().events.extend([
            touch(1, egui::TouchPhase::Cancel, end),
            button(start, true),
            touch(2, egui::TouchPhase::Start, start),
        ]);
        if !batched_release {
            h.run_steps(1);
            assert_eq!(h.state().doc.workbook(), &before);
        }
        h.input_mut().events.extend([
            egui::Event::PointerMoved(end),
            touch(2, egui::TouchPhase::Move, end),
            button(end, false),
            egui::Event::PointerGone,
            touch(2, egui::TouchPhase::End, end),
        ]);
        h.run_steps(3);
        assert_drag_created(&h, BoardTool::Shape(ShapeKind::Rectangle));
        assert!(h.state_mut().doc.undo());
        assert_eq!(h.state().doc.workbook(), &before);
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn batched_touch_restart_does_not_escape_an_active_multitouch() {
    let mut h = harness(board());
    set_tool(&mut h, BoardTool::Shape(ShapeKind::Rectangle));
    let before = h.state().doc.workbook().clone();
    let start = point(&h, Vec2::new(-200.0, -100.0));
    let end = point(&h, Vec2::new(100.0, 100.0));
    let touch = |id, phase, pos| egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(id),
        phase,
        pos,
        force: None,
    };
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    h.input_mut().events.extend([
        button(start, true),
        touch(1, egui::TouchPhase::Start, start),
    ]);
    h.run_steps(1);
    h.input_mut()
        .events
        .push(touch(2, egui::TouchPhase::Start, end));
    h.run_steps(1);
    h.input_mut().events.extend([
        touch(1, egui::TouchPhase::Cancel, start),
        button(start, true),
        touch(3, egui::TouchPhase::Start, start),
        egui::Event::PointerMoved(end),
        touch(3, egui::TouchPhase::Move, end),
        button(end, false),
        egui::Event::PointerGone,
        touch(3, egui::TouchPhase::End, end),
    ]);
    h.run_steps(2);
    assert_eq!(h.state().doc.workbook(), &before);
    assert_eq!(h.state().view.touch_navigation, Some(false));
    h.input_mut()
        .events
        .push(touch(2, egui::TouchPhase::End, end));
    h.run_steps(2);
    assert_eq!(h.state().view.touch_navigation, None);
    assert!(!h.state_mut().doc.undo());
}

#[test]
fn stray_release_after_touch_cancel_does_not_create_an_object() {
    let mut h = harness(board());
    set_tool(&mut h, BoardTool::Shape(ShapeKind::Rectangle));
    let before = h.state().doc.workbook().clone();
    let start = point(&h, Vec2::new(-200.0, -100.0));
    let end = point(&h, Vec2::new(100.0, 100.0));
    let touch = |phase, pos| egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(1),
        phase,
        pos,
        force: None,
    };
    h.input_mut().events.extend([
        egui::Event::PointerButton {
            pos: start,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        touch(egui::TouchPhase::Start, start),
    ]);
    h.run_steps(1);
    h.input_mut().events.extend([
        egui::Event::PointerMoved(end),
        touch(egui::TouchPhase::Cancel, end),
    ]);
    h.run_steps(1);
    h.input_mut().events.extend([
        egui::Event::PointerButton {
            pos: end,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
        egui::Event::PointerGone,
    ]);
    h.run_steps(2);
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(!h.state_mut().doc.undo());
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn touch_cancel_during_multitouch_stays_blocked_until_all_contacts_end() {
    let mut h = harness(board());
    set_tool(&mut h, BoardTool::Shape(ShapeKind::Rectangle));
    let before = h.state().doc.workbook().clone();
    let start = point(&h, Vec2::new(-200.0, -100.0));
    let end = point(&h, Vec2::new(100.0, 100.0));
    let touch = |id, phase, pos| egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(id),
        phase,
        pos,
        force: None,
    };
    h.input_mut().events.extend([
        egui::Event::PointerButton {
            pos: start,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        touch(1, egui::TouchPhase::Start, start),
    ]);
    h.run_steps(1);
    h.input_mut()
        .events
        .push(touch(2, egui::TouchPhase::Start, end));
    h.run_steps(1);
    h.input_mut()
        .events
        .push(touch(1, egui::TouchPhase::Cancel, start));
    h.run_steps(1);
    assert_eq!(h.state().view.touch_navigation, Some(false));
    h.input_mut().events.extend([
        egui::Event::PointerButton {
            pos: start,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        touch(3, egui::TouchPhase::Start, start),
        egui::Event::PointerMoved(end),
        touch(3, egui::TouchPhase::Move, end),
    ]);
    h.run_steps(1);
    h.input_mut().events.extend([
        egui::Event::PointerButton {
            pos: end,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
        egui::Event::PointerGone,
        touch(2, egui::TouchPhase::End, end),
        touch(3, egui::TouchPhase::End, end),
    ]);
    h.run_steps(2);
    assert_eq!(h.state().doc.workbook(), &before);
    assert_eq!(h.state().view.touch_navigation, None);
    assert!(!h.state_mut().doc.undo());
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn pending_fast_creation_rejects_camera_changes() {
    let mut h = harness(board());
    set_tool(&mut h, BoardTool::Shape(ShapeKind::Rectangle));
    let before = h.state().doc.workbook().clone();
    let start = point(&h, Vec2::new(-200.0, -100.0));
    let end = point(&h, Vec2::new(100.0, 100.0));
    h.event(egui::Event::PointerButton {
        pos: start,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(1);
    h.state_mut().view.offset += vec2(10.0, 0.0);
    h.input_mut().events.extend([
        egui::Event::PointerMoved(end),
        egui::Event::PointerButton {
            pos: end,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    ]);
    h.run_steps(3);
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(
        h.state()
            .errors
            .iter()
            .any(|error| error.to_string().contains("canceled"))
    );
}

#[test]
fn pending_creation_click_rejects_camera_changes() {
    let mut h = harness(board());
    set_tool(&mut h, BoardTool::Shape(ShapeKind::Rectangle));
    let before = h.state().doc.workbook().clone();
    let start = point(&h, Vec2::new(-200.0, -100.0));
    h.event(egui::Event::PointerMoved(start));
    h.event(egui::Event::PointerButton {
        pos: start,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(1);
    h.state_mut().view.offset += vec2(10.0, 0.0);
    h.event(egui::Event::PointerButton {
        pos: start,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(3);
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(
        h.state()
            .errors
            .iter()
            .any(|error| error.to_string().contains("canceled"))
    );
}

#[test]
fn pending_creation_click_rejects_a_replaced_document_at_the_same_revision() {
    let mut h = harness(board());
    set_tool(&mut h, BoardTool::Shape(ShapeKind::Rectangle));
    let start = point(&h, Vec2::new(-200.0, -100.0));
    h.event(egui::Event::PointerMoved(start));
    h.event(egui::Event::PointerButton {
        pos: start,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(1);
    let before = h.state().doc.workbook().clone();
    for _ in 0..2 {
        let replacement = Document::new(h.state().doc.workbook().clone());
        assert_eq!(replacement.state_id(), h.state().doc.state_id());
        h.state_mut().doc = replacement;
    }
    h.event(egui::Event::PointerButton {
        pos: start,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(3);
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(
        h.state()
            .errors
            .iter()
            .any(|error| error.to_string().contains("canceled"))
    );
}

#[test]
fn fast_creation_preserves_an_active_label_draft() {
    let mut sheet = board();
    let existing = shape(&mut sheet, 0.0, 0.0, "Original");
    let mut h = harness(sheet);
    {
        let state = h.state_mut();
        assert!(state.view.begin_board_label(&state.doc, existing));
    }
    h.run_steps(3);
    h.get_by_label("Board object label").type_text("Updated");
    h.run_steps(2);
    set_tool(&mut h, BoardTool::Shape(ShapeKind::Rectangle));
    let start = point(&h, Vec2::new(-200.0, -100.0));
    let end = point(&h, Vec2::new(100.0, 100.0));
    h.input_mut().events.push(egui::Event::PointerButton {
        pos: start,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(1);
    h.input_mut().events.extend([
        egui::Event::PointerMoved(end),
        egui::Event::PointerButton {
            pos: end,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    ]);
    h.run_steps(3);
    assert_eq!(h.state().doc.sheet().canvas.objects.len(), 2);
    assert_eq!(
        h.state()
            .doc
            .sheet()
            .canvas
            .object(existing)
            .unwrap()
            .label(),
        "Updated"
    );
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.sheet().canvas.objects.len(), 1);
    assert!(h.state_mut().doc.undo());
    assert_eq!(
        h.state()
            .doc
            .sheet()
            .canvas
            .object(existing)
            .unwrap()
            .label(),
        "Original"
    );
}

#[test]
fn browser_touch_release_commits_the_frame_before_pointer_gone() {
    let mut h = harness(board());
    set_tool(&mut h, BoardTool::Frame);
    let before = h.state().doc.workbook().clone();
    let start = point(&h, Vec2::new(-200.0, -100.0));
    let end = point(&h, Vec2::new(100.0, 100.0));
    let touch_event = |phase, pos| egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(1),
        phase,
        pos,
        force: None,
    };
    h.input_mut().events.extend([
        egui::Event::PointerMoved(start),
        egui::Event::PointerButton {
            pos: start,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        touch_event(egui::TouchPhase::Start, start),
    ]);
    h.run_steps(1);
    let intermediate = start + (end - start) * 0.75;
    h.input_mut().events.extend([
        egui::Event::PointerMoved(intermediate),
        touch_event(egui::TouchPhase::Move, intermediate),
    ]);
    h.run_steps(2);
    h.input_mut().events.extend([
        egui::Event::PointerButton {
            pos: end,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
        egui::Event::PointerGone,
        touch_event(egui::TouchPhase::End, end),
    ]);
    h.run_steps(3);
    assert_drag_created(&h, BoardTool::Frame);
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn scene_selection_multiselect_lock_z_order_and_undo() {
    let mut sheet = board();
    let bottom = shape(&mut sheet, -100.0, 0.0, "Bottom");
    let top = shape(&mut sheet, -100.0, 0.0, "Top");
    let other = shape(&mut sheet, 130.0, 0.0, "Other");
    let mut h = harness(sheet);
    let overlap = point(&h, Vec2::new(-100.0, 0.0));
    click(&mut h, overlap, Modifiers::NONE);
    h.run_steps(2);
    assert_eq!(h.state().view.board_selection, vec![top]);

    {
        let state = h.state_mut();
        apply_board_action(
            &mut state.doc,
            &mut state.view,
            BoardAction::ZOrder(ZOrder::Back),
        )
        .unwrap();
    }
    h.run_steps(2);
    click(&mut h, overlap, Modifiers::NONE);
    h.run_steps(2);
    assert_eq!(h.state().view.board_selection, vec![bottom]);

    h.state_mut().view.board_selection = vec![bottom, other];
    let before = h.state().doc.workbook().clone();
    {
        let state = h.state_mut();
        apply_board_action(&mut state.doc, &mut state.view, BoardAction::Lock(true)).unwrap();
    }
    let locked = h.state().doc.workbook().clone();
    {
        let state = h.state_mut();
        assert!(
            !apply_board_action(
                &mut state.doc,
                &mut state.view,
                BoardAction::Move(Vec2::new(20.0, 0.0))
            )
            .unwrap()
        );
        assert!(!apply_action(&mut state.doc, &mut state.view, Action::Delete).unwrap());
    }
    assert_eq!(h.state().doc.workbook(), &locked);
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn fixed_connector_attachment_shift_drag_uses_release_modifiers_and_undo() {
    let mut sheet = board();
    let target = shape(&mut sheet, 300.0, 120.0, "Target");
    sheet.rotate_canvas_object(target, 90.0).unwrap();
    let edge = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Point(Vec2::new(-200.0, 0.0)),
            Endpoint::Point(Vec2::new(0.0, 0.0)),
        )))
        .unwrap();
    let mut h = harness(sheet);
    h.state_mut().view.board_selection = vec![edge];
    h.run_steps(3);
    let original = h.state().doc.workbook().clone();
    let handle = h.get_by_label("Connector To").rect().center();
    let target_point =
        thinoda_core::board::rotate_around(Vec2::new(270.0, 100.0), Vec2::new(300.0, 120.0), 90.0);
    let destination = point(&h, target_point);
    drag(&mut h, handle, destination);
    assert_eq!(h.state().doc.workbook(), &original);
    finish_alt_drag(&mut h, destination, Modifiers::SHIFT);
    let current = connector(&h, edge);
    assert_eq!(current.to, Endpoint::Object(target));
    let port = current.to_port.unwrap();
    assert!((port.position.x - 0.25).abs() < 0.001, "{port:?}");
    assert!((port.position.y - 0.25).abs() < 0.001, "{port:?}");
    assert!(!port.perimeter);
    assert!(
        h.get_by_label("Connector To, Fixed attachment")
            .accesskit_node()
            .description()
            .unwrap()
            .contains("Shift-drag")
    );
    let fixed = h.state().doc.workbook().clone();
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &original);
    assert!(h.state_mut().doc.redo());
    assert_eq!(h.state().doc.workbook(), &fixed);
    h.run_steps(3);
    let handle = h
        .get_by_label("Connector To, Fixed attachment")
        .rect()
        .center();
    let destination = point(&h, Vec2::new(300.0, 120.0));
    drag(&mut h, handle, destination);
    finish_drag(&mut h, destination);
    assert_eq!(connector(&h, edge).to, Endpoint::Object(target));
    assert_eq!(connector(&h, edge).to_port, None);
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &fixed);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn fixed_connector_attachment_keyboard_controls_are_discoverable_and_undoable() {
    let mut sheet = board();
    let target = shape(&mut sheet, 200.0, 0.0, "Target");
    let edge = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Point(Vec2::new(-200.0, 0.0)),
            Endpoint::Object(target),
        )))
        .unwrap();
    let mut h = harness(sheet);
    h.state_mut().view.board_selection = vec![edge];
    h.run_steps(3);
    let original = h.state().doc.workbook().clone();
    h.get_by_label("Connector To").focus();
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(connector(&h, edge).to, Endpoint::Object(target));
    let port = connector(&h, edge).to_port.unwrap();
    assert!(!port.perimeter);
    let fixed = h.state().doc.workbook().clone();
    let fixed_handle = h.get_by_label("Connector To, Fixed attachment");
    assert!(fixed_handle.is_focused());
    assert!(
        fixed_handle
            .accesskit_node()
            .description()
            .unwrap()
            .contains("Press Enter")
    );
    h.key_press(Key::ArrowDown);
    h.run_steps(3);
    assert_eq!(connector(&h, edge).to, Endpoint::Object(target));
    let moved = connector(&h, edge).to_port.unwrap();
    assert!((moved.position.y - port.position.y - 1.0 / 80.0).abs() < 0.001);
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &fixed);
    assert!(h.state_mut().doc.redo());
    h.run_steps(3);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(connector(&h, edge).to_port, None);
    assert!(h.state_mut().doc.undo());
    assert_eq!(connector(&h, edge).to_port, Some(moved));
    h.run_steps(3);
    h.get_by_label("Connector To, Fixed attachment").focus();
    h.run_steps(2);
    let original_connector = h.state().doc.sheet().canvas.object(edge).unwrap().clone();
    h.key_press_modifiers(Modifiers::ALT, Key::ArrowDown);
    h.run_steps(3);
    assert_eq!(
        h.state().doc.sheet().canvas.object(edge),
        Some(&original_connector)
    );
    assert_ne!(h.state().view.board_selection[0], edge);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    assert_ne!(h.state().doc.workbook(), &original);
}

#[test]
fn fixed_connector_attachment_keyboard_nudges_projected_ports_at_the_visible_point() {
    let mut sheet = board();
    let target = shape(&mut sheet, 200.0, 0.0, "Target");
    let mut initial_connector = CanvasConnector::new(
        Endpoint::Point(Vec2::new(-200.0, 0.0)),
        Endpoint::Object(target),
    );
    initial_connector.to_port = Some(ConnectorPort {
        position: Vec2::new(0.5, 0.5),
        offset: Vec2::ZERO,
        perimeter: true,
    });
    let edge = sheet
        .add_canvas_object(CanvasObject::connector(initial_connector))
        .unwrap();
    let mut h = harness(sheet);
    h.state_mut().view.board_selection = vec![edge];
    h.run_steps(3);
    let before = h.state().doc.workbook().clone();
    let endpoint = |h: &Harness<'static, State>| {
        let sheet = h.state().doc.sheet();
        let layout = thinoda_core::layout_sheet(sheet, &thinoda_core::ApproxMeasure, false);
        *thinoda_core::scene::connector_path(sheet, &layout, connector(h, edge))
            .last()
            .unwrap()
    };
    let original = endpoint(&h);
    h.get_by_label("Connector To, Fixed attachment").focus();
    h.run_steps(2);
    h.key_press(Key::ArrowLeft);
    h.run_steps(3);
    assert!((endpoint(&h) - (original + Vec2::new(-1.0, 0.0))).length() < 0.01);
    assert!(!connector(&h, edge).to_port.unwrap().perimeter);
    let moved = h.state().doc.workbook().clone();
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state_mut().doc.redo());
    assert_eq!(h.state().doc.workbook(), &moved);
    assert!((endpoint(&h) - (original + Vec2::new(-1.0, 0.0))).length() < 0.01);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn fixed_connector_attachment_toggles_with_accessibility_click() {
    let mut sheet = board();
    let target = shape(&mut sheet, 200.0, 0.0, "Target");
    let edge = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Point(Vec2::new(-200.0, 0.0)),
            Endpoint::Object(target),
        )))
        .unwrap();
    let mut h = harness(sheet);
    h.state_mut().view.board_selection = vec![edge];
    h.run_steps(3);
    let before = h.state().doc.workbook().clone();
    h.get_by_label("Connector To").click_accesskit();
    h.run_steps(3);
    assert!(connector(&h, edge).to_port.is_some());
    assert!(
        h.get_by_label("Connector To, Fixed attachment")
            .is_focused()
    );
    h.get_by_label("Connector To, Fixed attachment")
        .click_accesskit();
    h.run_steps(3);
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn connector_creation_and_endpoint_edits_are_undoable() {
    let mut sheet = board();
    let left = shape(&mut sheet, -300.0, 0.0, "Left");
    let right = shape(&mut sheet, 80.0, 0.0, "Right");
    let target = shape(&mut sheet, 300.0, 120.0, "Target");
    let mut h = harness(sheet);
    set_tool(&mut h, BoardTool::Connector);
    let before = h.state().doc.workbook().clone();
    let start = point(&h, Vec2::new(-300.0, 0.0));
    let end = point(&h, Vec2::new(80.0, 0.0));
    drag(&mut h, start, end);
    assert_eq!(h.state().doc.workbook(), &before);
    finish_drag(&mut h, end);
    let id = h.state().view.board_selection[0];
    assert_eq!(connector(&h, id).from, Endpoint::Object(left));
    assert_eq!(connector(&h, id).to, Endpoint::Object(right));
    let created = h.state().doc.workbook().clone();
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state_mut().doc.redo());
    h.run_steps(2);
    set_tool(&mut h, BoardTool::Select);

    let handle = h.get_by_label("Connector To").rect().center();
    let destination = point(&h, Vec2::new(300.0, 120.0));
    drag(&mut h, handle, destination);
    assert_eq!(h.state().doc.workbook(), &created);
    finish_drag(&mut h, destination);
    assert_eq!(connector(&h, id).to, Endpoint::Object(target));
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &created);
    h.run_steps(2);

    h.get_by_label("Connector, From: Left, To: Right, Selected")
        .focus();
    h.run_steps(2);
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert!(h.get_by_label("Connector From").is_focused());
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert!(h.get_by_label("Connector To").is_focused());
    h.key_press(Key::ArrowDown);
    h.run_steps(2);
    assert!(matches!(connector(&h, id).to, Endpoint::Point(_)));
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    assert_eq!(h.state().doc.workbook(), &created);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

fn waypoint_harness(route: ConnectorRoute) -> (Harness<'static, State>, CanvasObjectId) {
    let mut sheet = board();
    sheet.board.snap = false;
    let mut edge = CanvasConnector::new(
        Endpoint::Point(Vec2::new(-240.0, -80.0)),
        Endpoint::Point(Vec2::new(240.0, 80.0)),
    );
    edge.route = route;
    let id = sheet
        .add_canvas_object(CanvasObject::connector(edge))
        .unwrap();
    let mut h = harness(sheet);
    h.state_mut().view.select_board_object(id);
    h.run_steps(3);
    (h, id)
}

#[test]
fn waypoint_drag_move_and_double_click_are_undoable_for_all_routes() {
    for route in [
        ConnectorRoute::Straight,
        ConnectorRoute::Elbow,
        ConnectorRoute::Curve,
    ] {
        let (mut h, id) = waypoint_harness(route);
        let before = h.state().doc.workbook().clone();
        let handle = h.get_by_label("Add connector waypoint 1").rect().center();
        let destination = handle + vec2(20.0, 65.0);
        drag(&mut h, handle, destination);
        assert_eq!(h.state().doc.workbook(), &before, "{route:?}: preview");
        finish_drag(&mut h, destination);
        assert_eq!(connector(&h, id).waypoints.len(), 1, "{route:?}");
        let inserted = h.state().doc.workbook().clone();

        let handle = h.get_by_label("Connector waypoint 1").rect().center();
        let destination = handle + vec2(35.0, -30.0);
        let old = connector(&h, id).waypoints[0];
        drag(&mut h, handle, destination);
        assert_eq!(h.state().doc.workbook(), &inserted, "{route:?}: preview");
        finish_drag(&mut h, destination);
        assert_eq!(connector(&h, id).waypoints[0], old + Vec2::new(35.0, -30.0));
        let moved = h.state().doc.workbook().clone();

        let handle = h.get_by_label("Connector waypoint 1").rect().center();
        click(&mut h, handle, Modifiers::NONE);
        assert_eq!(
            h.state().doc.workbook(),
            &moved,
            "{route:?}: click is not an edit"
        );
        click(&mut h, handle, Modifiers::NONE);
        assert!(connector(&h, id).waypoints.is_empty(), "{route:?}");
        assert!(h.state_mut().doc.undo());
        assert_eq!(h.state().doc.workbook(), &moved);
        assert!(h.state_mut().doc.undo());
        assert_eq!(h.state().doc.workbook(), &inserted);
        assert!(h.state_mut().doc.undo());
        assert_eq!(h.state().doc.workbook(), &before);
        assert!(h.state_mut().doc.redo());
        assert!(h.state_mut().doc.redo());
        assert!(h.state_mut().doc.redo());
        assert!(connector(&h, id).waypoints.is_empty());
        assert!(
            h.state().errors.is_empty(),
            "{route:?}: {:?}",
            h.state().errors
        );
    }
}

#[test]
fn waypoint_keyboard_controls_preserve_endpoint_tab_order_and_focus() {
    let (mut h, id) = waypoint_harness(ConnectorRoute::Straight);
    let before = h.state().doc.workbook().clone();
    let connector_label =
        "Connector, From: Free point (-240, -80), To: Free point (240, 80), Selected";
    h.get_by_label(connector_label).focus();
    h.run_steps(2);
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert!(h.get_by_label("Connector From").is_focused());
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert!(h.get_by_label("Connector To").is_focused());
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert!(h.get_by_label("Add connector waypoint 1").is_focused());
    assert_eq!(
        h.get_by_label("Add connector waypoint 1")
            .accesskit_node()
            .description(),
        Some("Drag, press Enter, or use arrow keys to add a waypoint.".into())
    );
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(connector(&h, id).waypoints.len(), 1);
    assert!(h.get_by_label("Connector waypoint 1").is_focused());
    assert_eq!(
        h.get_by_label("Connector waypoint 1")
            .accesskit_node()
            .description(),
        Some("Drag or use arrow keys to move; double-click or press Delete to remove.".into())
    );
    let initial = connector(&h, id).waypoints[0];
    h.key_press(Key::ArrowRight);
    h.run_steps(2);
    assert_eq!(
        connector(&h, id).waypoints[0],
        initial + Vec2::new(1.0, 0.0)
    );
    h.key_press(Key::Delete);
    h.run_steps(3);
    assert!(connector(&h, id).waypoints.is_empty());
    assert!(h.state().doc.sheet().canvas.object(id).is_some());
    assert!(h.get_by_label(connector_label).is_focused());
    assert!(h.state_mut().doc.undo());
    assert!(h.state_mut().doc.undo());
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn waypoint_keyboard_move_offscreen_returns_focus_to_connector() {
    let (mut h, id) = waypoint_harness(ConnectorRoute::Straight);
    let canvas = h.state().view.canvas_rect;
    let point = h
        .state()
        .view
        .xf()
        .sheet(pos2(canvas.right() - 1.0, canvas.center().y));
    h.state_mut()
        .doc
        .update_canvas_object("Add control", id, |object| {
            if let CanvasObjectKind::Connector(connector) = &mut object.kind {
                connector.waypoints = vec![point].into();
            }
        })
        .unwrap();
    h.run_steps(3);
    h.get_by_label("Connector waypoint 1").focus();
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::SHIFT, Key::ArrowRight);
    h.run_steps(3);
    assert_eq!(connector(&h, id).waypoints[0], point + Vec2::new(10.0, 0.0));
    assert!(
        h.get_by_label(
            "Connector, From: Free point (-240, -80), To: Free point (240, 80), Selected"
        )
        .is_focused()
    );
}

#[test]
fn waypoint_insertion_accepts_accessibility_click() {
    let (mut h, id) = waypoint_harness(ConnectorRoute::Straight);
    h.get_by_label("Add connector waypoint 1").click();
    h.run_steps(3);
    assert!(connector(&h, id).waypoints.is_empty());
    let position = h.get_by_label("Add connector waypoint 1").rect().center();
    click(&mut h, position, Modifiers::NONE);
    assert!(connector(&h, id).waypoints.is_empty());
    h.get_by_label("Add connector waypoint 1").click_accesskit();
    h.run_steps(3);
    assert_eq!(connector(&h, id).waypoints.len(), 1);
    assert!(h.get_by_label("Connector waypoint 1").is_focused());

    h.get_by_label("Connector, From: Free point (-240, -80), To: Free point (240, 80), Selected")
        .focus();
    h.run_steps(2);
    h.get_by_label("Connector waypoint 1").click_accesskit();
    h.run_steps(2);
    assert!(h.get_by_label("Connector waypoint 1").is_focused());
    assert_eq!(connector(&h, id).waypoints.len(), 1);
}

#[test]
fn waypoint_drag_cancel_and_stale_document_never_commit() {
    let (mut h, id) = waypoint_harness(ConnectorRoute::Straight);
    let before = h.state().doc.workbook().clone();
    let handle = h.get_by_label("Add connector waypoint 1").rect().center();
    let destination = handle + vec2(20.0, 50.0);
    drag(&mut h, handle, destination);
    h.key_press(Key::Escape);
    h.run_steps(2);
    finish_drag(&mut h, destination);
    assert_eq!(h.state().doc.workbook(), &before);

    h.run_steps(3);
    let handle = h.get_by_label("Add connector waypoint 1").rect().center();
    drag(&mut h, handle, destination);
    h.state_mut()
        .doc
        .update_canvas_object("External edit", id, |object| {
            object.alt_text = "Changed".into()
        })
        .unwrap();
    let changed = h.state().doc.workbook().clone();
    h.run_steps(2);
    finish_drag(&mut h, destination);
    assert_eq!(h.state().doc.workbook(), &changed);
    assert!(connector(&h, id).waypoints.is_empty());
    assert!(
        h.state()
            .errors
            .iter()
            .any(|error| error.to_string().contains("gesture canceled"))
    );
}

#[test]
fn waypoint_touch_drag_commits_once_and_touch_cancel_discards_preview() {
    let (mut h, id) = waypoint_harness(ConnectorRoute::Straight);
    let before = h.state().doc.workbook().clone();
    let start = h.get_by_label("Add connector waypoint 1").rect().center();
    let end = start + vec2(30.0, 60.0);
    touch(&mut h, egui::TouchPhase::Start, start);
    touch(&mut h, egui::TouchPhase::Move, end);
    assert_eq!(h.state().doc.workbook(), &before);
    touch(&mut h, egui::TouchPhase::End, end);
    h.run_steps(3);
    assert_eq!(connector(&h, id).waypoints.len(), 1);
    let inserted = h.state().doc.workbook().clone();
    let start = h.get_by_label("Connector waypoint 1").rect().center();
    let end = start + vec2(60.0, 0.0);
    touch(&mut h, egui::TouchPhase::Start, start);
    touch(&mut h, egui::TouchPhase::Move, end);
    touch(&mut h, egui::TouchPhase::Cancel, end);
    touch(&mut h, egui::TouchPhase::End, end);
    h.run_steps(3);
    assert_eq!(h.state().doc.workbook(), &inserted);
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn coincident_waypoints_remain_individually_keyboard_editable() {
    let (mut h, id) = waypoint_harness(ConnectorRoute::Curve);
    let location = Vec2::new(0.0, 60.0);
    h.state_mut()
        .doc
        .update_canvas_object("Add controls", id, |object| {
            if let CanvasObjectKind::Connector(connector) = &mut object.kind {
                connector.waypoints = vec![location, location].into();
            }
        })
        .unwrap();
    h.run_steps(3);
    assert!(h.query_by_label("Connector waypoint 1").is_some());
    h.get_by_label("Connector waypoint 2").focus();
    h.run_steps(2);
    h.key_press(Key::ArrowRight);
    h.run_steps(2);
    assert_eq!(
        connector(&h, id).waypoints.as_ref(),
        &[location, location + Vec2::new(1.0, 0.0)]
    );
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn waypoint_drag_snapping_to_its_original_position_is_a_noop() {
    let (mut h, id) = waypoint_harness(ConnectorRoute::Straight);
    h.state_mut()
        .doc
        .edit_sheet("Enable snapping", |sheet| {
            sheet.board.snap = true;
            sheet.board.grid_size = 20.0;
            sheet
                .update_canvas_object(id, |object| {
                    if let CanvasObjectKind::Connector(connector) = &mut object.kind {
                        connector.waypoints = vec![Vec2::ZERO].into();
                    }
                })
                .map_err(thinoda_core::EditError::Invalid)
        })
        .unwrap();
    h.run_steps(3);
    let before = h.state().doc.workbook().clone();
    let revision = h.state().doc.state_id();
    let start = h.get_by_label("Connector waypoint 1").rect().center();
    let end = start + vec2(5.0, 5.0);
    drag(&mut h, start, end);
    finish_drag(&mut h, end);
    assert_eq!(h.state().doc.workbook(), &before);
    assert_eq!(h.state().doc.state_id(), revision);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn locked_connector_has_no_waypoint_controls() {
    let mut sheet = board();
    let mut edge = CanvasConnector::new(
        Endpoint::Point(Vec2::new(-240.0, 0.0)),
        Endpoint::Point(Vec2::new(240.0, 0.0)),
    );
    edge.waypoints = vec![Vec2::new(0.0, 50.0)].into();
    let mut object = CanvasObject::connector(edge);
    object.locked = true;
    let id = sheet.add_canvas_object(object).unwrap();
    let mut h = harness(sheet);
    h.state_mut().view.select_board_object(id);
    h.run_steps(3);
    assert!(h.query_by_label("Connector waypoint 1").is_none());
    assert!(h.query_by_label("Add connector waypoint 1").is_none());
}

#[test]
fn waypoint_limit_reports_an_error_without_changing_history() {
    let mut sheet = board();
    let mut edge = CanvasConnector::new(
        Endpoint::Point(Vec2::new(-300.0, 0.0)),
        Endpoint::Point(Vec2::new(300.0, 0.0)),
    );
    edge.waypoints = vec![Vec2::ZERO; thinoda_core::board::MAX_CONNECTOR_WAYPOINTS].into();
    let id = sheet
        .add_canvas_object(CanvasObject::connector(edge))
        .unwrap();
    let mut h = harness(sheet);
    h.state_mut().view.select_board_object(id);
    h.run_steps(3);
    let before = h.state().doc.workbook().clone();
    let revision = h.state().doc.state_id();
    h.get_by_label("Add connector waypoint 1").focus();
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(h.state().doc.workbook(), &before);
    assert_eq!(h.state().doc.state_id(), revision);
    assert!(
        h.state()
            .errors
            .iter()
            .any(|error| error.to_string().contains("1024"))
    );
}

#[test]
fn waypoint_controls_respect_frame_clipping() {
    let mut sheet = board();
    let mut edge = CanvasConnector::new(
        Endpoint::Point(Vec2::new(-180.0, 0.0)),
        Endpoint::Point(Vec2::new(180.0, 0.0)),
    );
    edge.waypoints = vec![Vec2::new(-95.0, 20.0), Vec2::new(120.0, 20.0)].into();
    let id = sheet
        .add_canvas_object(CanvasObject::connector(edge))
        .unwrap();
    let frame = sheet
        .create_board_frame(
            "Clip",
            thinoda_core::Rect::from_min_size(Vec2::new(-100.0, -100.0), Vec2::new(200.0, 200.0)),
        )
        .unwrap();
    sheet.set_zone_members(frame, vec![], vec![id]).unwrap();
    sheet.zones[0].clip = true;
    let mut h = harness(sheet);
    h.state_mut().view.select_board_object(id);
    h.run_steps(3);
    assert!(h.query_by_label("Connector waypoint 1").is_some());
    assert!(h.query_by_label("Connector waypoint 2").is_none());
    h.get_by_label("Connector waypoint 1").focus();
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::SHIFT, Key::ArrowLeft);
    h.run_steps(3);
    assert_eq!(connector(&h, id).waypoints[0], Vec2::new(-105.0, 20.0));
    assert!(h.query_by_label("Connector waypoint 1").is_none());
    assert!(
        h.get_by_label("Connector, From: Free point (-180, 0), To: Free point (180, 0), Selected")
            .is_focused()
    );
}

#[test]
fn mixed_card_object_selection_moves_together_with_one_undo() {
    let mut sheet = board();
    let card = sheet
        .add_card(
            "Note",
            Vec2::new(-200.0, 0.0),
            Card::new(CardKind::Sticky, Vec2::new(120.0, 100.0)),
        )
        .unwrap();
    let object = shape(&mut sheet, 150.0, 0.0, "Box");
    let mut h = harness(sheet);
    let card_point = point(&h, Vec2::new(-200.0, 0.0));
    click(&mut h, card_point, Modifiers::NONE);
    let object_point = point(&h, Vec2::new(150.0, 0.0));
    click(&mut h, object_point, Modifiers::SHIFT);
    assert_eq!(h.state().view.selection, vec![card]);
    assert_eq!(h.state().view.board_selection, vec![object]);
    let before = h.state().doc.workbook().clone();

    let end = object_point + vec2(45.0, 25.0);
    drag(&mut h, object_point, end);
    assert_eq!(h.state().doc.workbook(), &before);
    finish_drag(&mut h, end);
    assert_eq!(
        h.state().doc.sheet().topic(card).unwrap().position,
        Some(Vec2::new(-155.0, 25.0))
    );
    assert_eq!(
        h.state()
            .doc
            .sheet()
            .canvas
            .object(object)
            .unwrap()
            .rect
            .center(),
        Vec2::new(195.0, 25.0)
    );
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn mixed_group_click_toggle_drag_and_rubber_selection() {
    let mut sheet = board();
    let card = sheet
        .add_card(
            "Grouped card",
            Vec2::new(-200.0, 0.0),
            Card::new(CardKind::Sticky, Vec2::new(120.0, 100.0)),
        )
        .unwrap();
    let object = shape(&mut sheet, 130.0, 0.0, "Grouped shape");
    let other = sheet.add_floating("Other", Vec2::new(-400.0, 200.0));
    sheet
        .group_board_members(&[BoardMember::Card(card), BoardMember::Object(object)])
        .unwrap();
    let mut h = harness(sheet);
    let card_point = point(&h, Vec2::new(-200.0, 0.0));
    let object_point = point(&h, Vec2::new(130.0, 0.0));
    let other_point = point(&h, Vec2::new(-400.0, 200.0));

    click(&mut h, card_point, Modifiers::NONE);
    assert_eq!(h.state().view.selection, vec![card]);
    assert_eq!(h.state().view.board_selection, vec![object]);
    click(&mut h, object_point, Modifiers::SHIFT);
    assert!(h.state().view.selection.is_empty());
    assert!(h.state().view.board_selection.is_empty());
    click(&mut h, other_point, Modifiers::NONE);
    click(&mut h, object_point, Modifiers::SHIFT);
    assert!(h.state().view.selection.contains(&other));
    assert!(h.state().view.selection.contains(&card));
    assert_eq!(h.state().view.board_selection, vec![object]);
    click(&mut h, card_point, Modifiers::SHIFT);
    assert_eq!(h.state().view.selection, vec![other]);
    assert!(h.state().view.board_selection.is_empty());

    click(&mut h, card_point, Modifiers::NONE);
    let before = h.state().doc.workbook().clone();
    let end = card_point + vec2(45.0, 25.0);
    drag(&mut h, card_point, end);
    assert_eq!(h.state().doc.workbook(), &before);
    finish_drag(&mut h, end);
    assert_eq!(
        h.state().doc.sheet().topic(card).unwrap().position,
        Some(Vec2::new(-155.0, 25.0))
    );
    assert_eq!(
        h.state()
            .doc
            .sheet()
            .canvas
            .object(object)
            .unwrap()
            .rect
            .center(),
        Vec2::new(175.0, 25.0)
    );
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
    h.run_steps(2);

    let start = point(&h, Vec2::new(-275.0, -70.0));
    let end = point(&h, Vec2::new(-125.0, 70.0));
    drag(&mut h, start, end);
    finish_drag(&mut h, end);
    assert_eq!(h.state().view.selection, vec![card]);
    assert_eq!(h.state().view.board_selection, vec![object]);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn mixed_group_commands_are_atomic_and_arrange_groups_as_units() {
    let mut sheet = board();
    let card = sheet
        .add_card(
            "Card",
            Vec2::new(80.0, 20.0),
            Card::new(CardKind::Sticky, Vec2::new(120.0, 100.0)),
        )
        .unwrap();
    let object = shape(&mut sheet, 250.0, 80.0, "Shape");
    let other = shape(&mut sheet, -350.0, 160.0, "Other");
    let mut h = harness(sheet);
    h.state_mut().view.selection = vec![card];
    h.state_mut().view.board_selection = vec![object];
    let before_group = h.state().doc.workbook().clone();
    {
        let state = h.state_mut();
        assert!(BoardAction::Group.available(&state.doc, &state.view));
        assert!(apply_board_action(&mut state.doc, &mut state.view, BoardAction::Group).unwrap());
    }
    let group = h
        .state()
        .doc
        .sheet()
        .board_member_group(BoardMember::Card(card))
        .unwrap();
    assert_eq!(
        h.state()
            .doc
            .sheet()
            .board_member_group(BoardMember::Object(object)),
        Some(group)
    );
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before_group);
    assert!(h.state_mut().doc.redo());
    h.state_mut().view.board_selection.clear();
    let grouped = h.state().doc.workbook().clone();

    {
        let state = h.state_mut();
        assert!(
            apply_board_action(
                &mut state.doc,
                &mut state.view,
                BoardAction::Move(Vec2::new(30.0, -20.0))
            )
            .unwrap()
        );
    }
    assert_eq!(
        h.state().doc.sheet().topic(card).unwrap().position,
        Some(Vec2::new(110.0, 0.0))
    );
    assert_eq!(
        h.state()
            .doc
            .sheet()
            .canvas
            .object(object)
            .unwrap()
            .rect
            .center(),
        Vec2::new(280.0, 60.0)
    );
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &grouped);

    h.state_mut().view.board_selection = vec![other];
    let before = h.state().doc.workbook().clone();
    let card_before = h.state().doc.sheet().topic(card).unwrap().position.unwrap();
    let shape_before = h
        .state()
        .doc
        .sheet()
        .canvas
        .object(object)
        .unwrap()
        .rect
        .center();
    {
        let state = h.state_mut();
        assert!(
            apply_board_action(
                &mut state.doc,
                &mut state.view,
                BoardAction::Arrange(crate::FloatingArrangement::Left)
            )
            .unwrap()
        );
    }
    let card_after = h.state().doc.sheet().topic(card).unwrap().position.unwrap();
    let shape_after = h
        .state()
        .doc
        .sheet()
        .canvas
        .object(object)
        .unwrap()
        .rect
        .center();
    assert_ne!(card_after, card_before);
    assert_eq!(card_after - card_before, shape_after - shape_before);
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);

    {
        let state = h.state_mut();
        let action = Action::AlignFloating(crate::FloatingArrangement::Left);
        assert!(action.available(&state.doc, &state.view));
        assert!(apply_action(&mut state.doc, &mut state.view, action).unwrap());
        let card_after = state.doc.sheet().topic(card).unwrap().position.unwrap();
        let shape_after = state
            .doc
            .sheet()
            .canvas
            .object(object)
            .unwrap()
            .rect
            .center();
        assert_eq!(card_after - card_before, shape_after - shape_before);
        assert!(state.doc.undo());
        assert_eq!(state.doc.workbook(), &before);
    }

    h.state_mut().view.board_selection.clear();
    {
        let state = h.state_mut();
        assert!(apply_board_action(&mut state.doc, &mut state.view, BoardAction::Ungroup).unwrap());
    }
    assert_eq!(
        h.state()
            .doc
            .sheet()
            .board_member_group(BoardMember::Card(card)),
        None
    );
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
    {
        let state = h.state_mut();
        assert!(apply_action(&mut state.doc, &mut state.view, Action::Delete).unwrap());
    }
    assert!(!h.state().doc.sheet().contains(card));
    assert!(h.state().doc.sheet().canvas.object(object).is_none());
    assert!(h.state().doc.sheet().canvas.object(other).is_some());
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn partially_locked_mixed_group_blocks_edits_and_can_be_unlocked_together() {
    let mut sheet = board();
    let card = sheet
        .add_card(
            "Card",
            Vec2::new(-200.0, 0.0),
            Card::new(CardKind::Sticky, Vec2::new(120.0, 100.0)),
        )
        .unwrap();
    let object = shape(&mut sheet, 130.0, 0.0, "Shape");
    let group = sheet
        .group_board_members(&[BoardMember::Card(card), BoardMember::Object(object)])
        .unwrap();
    sheet
        .set_board_members_locked(&[BoardMember::Object(object)], true)
        .unwrap();
    let mut h = harness(sheet);
    let card_point = point(&h, Vec2::new(-200.0, 0.0));
    click(&mut h, card_point, Modifiers::NONE);
    let before = h.state().doc.workbook().clone();
    assert_eq!(h.state().view.selection, vec![card]);
    assert_eq!(h.state().view.board_selection, vec![object]);
    assert!(
        h.query_by_label("Sticky note: Card, Selected, Locked")
            .is_some()
    );
    assert!(
        h.query_by_label("Rectangle: Shape, Selected, Locked")
            .is_some()
    );
    {
        let state = h.state_mut();
        assert!(!BoardAction::Move(Vec2::new(10.0, 0.0)).available(&state.doc, &state.view));
        assert!(!BoardAction::Ungroup.available(&state.doc, &state.view));
        assert!(!Action::Delete.available(&state.doc, &state.view));
        assert!(!Action::Cut.available(&state.doc, &state.view));
        assert!(!state.view.begin_board_label(&state.doc, object));
        state.view.begin_edit(state.doc.sheet(), card, true);
        assert_eq!(state.view.editing_topic(), None);
        assert!(!apply_action(&mut state.doc, &mut state.view, Action::Delete).unwrap());
    }
    h.key_press(Key::ArrowRight);
    h.key_press(Key::Delete);
    h.run_steps(2);
    assert_eq!(h.state().doc.workbook(), &before);

    h.key_press_modifiers(Modifiers::COMMAND, Key::L);
    h.run_steps(2);
    let sheet = h.state().doc.sheet();
    assert!(!sheet.is_board_member_locked(BoardMember::Card(card)));
    assert!(!sheet.is_board_member_locked(BoardMember::Object(object)));
    assert!(!sheet.canvas.group(group).unwrap().locked);
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn keyboard_moves_deletes_and_undoes_shapes_but_respects_locks() {
    let mut sheet = board();
    let id = shape(&mut sheet, 0.0, 0.0, "Keyboard");
    let mut h = harness(sheet);
    let center = point(&h, Vec2::ZERO);
    click(&mut h, center, Modifiers::NONE);
    let before = h.state().doc.workbook().clone();
    h.key_press(Key::ArrowRight);
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::SHIFT, Key::ArrowDown);
    h.run_steps(2);
    assert_eq!(
        h.state()
            .doc
            .sheet()
            .canvas
            .object(id)
            .unwrap()
            .rect
            .center(),
        Vec2::new(1.0, 10.0)
    );
    h.key_press(Key::Delete);
    h.run_steps(2);
    assert!(h.state().doc.sheet().canvas.object(id).is_none());
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    assert!(h.state().doc.sheet().canvas.object(id).is_some());
    assert!(h.state_mut().doc.undo());
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);

    h.state_mut().view.board_selection = vec![id];
    {
        let state = h.state_mut();
        apply_board_action(&mut state.doc, &mut state.view, BoardAction::Lock(true)).unwrap();
    }
    let locked = h.state().doc.workbook().clone();
    h.run_steps(2);
    h.key_press(Key::ArrowRight);
    h.run_steps(2);
    h.key_press(Key::Delete);
    h.run_steps(2);
    assert_eq!(h.state().doc.workbook(), &locked);
    assert!(!Action::StartEdit.available(&h.state().doc, &h.state().view));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn shape_accessibility_and_keyboard_label_editing() {
    let mut sheet = board();
    let anchor = sheet.root;
    let id = shape(&mut sheet, 0.0, 0.0, "Original");
    let mut h = harness(sheet);
    assert!(h.query_by_label("Rectangle: Original").is_some());
    assert!(
        h.query_by_label(&h.state().doc.sheet().topic(anchor).unwrap().title)
            .is_none()
    );
    h.get_by_label("Rectangle: Original").focus();
    h.run_steps(2);
    assert_eq!(h.state().view.board_selection, vec![id]);
    assert!(Action::StartEdit.available(&h.state().doc, &h.state().view));
    h.key_press(Key::F2);
    h.run_steps(2);
    assert_eq!(h.state().view.editing_board_object(), Some(id));
    h.get_by_label("Board object label").type_text("Updated");
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert_eq!(
        h.state().doc.sheet().canvas.object(id).unwrap().label(),
        "Updated"
    );
    assert!(h.query_by_label("Rectangle: Updated, Selected").is_some());
    assert!(h.state_mut().doc.undo());
    assert_eq!(
        h.state().doc.sheet().canvas.object(id).unwrap().label(),
        "Original"
    );
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn rotated_shape_label_editor_tracks_the_label_and_keeps_keyboard_editing() {
    let mut sheet = board();
    let mut object = CanvasObject::shape(
        ShapeKind::Triangle,
        thinoda_core::Rect::from_center_size(Vec2::ZERO, Vec2::new(200.0, 120.0)),
    );
    object.rotation = 90.0;
    if let CanvasObjectKind::Shape { label, .. } = &mut object.kind {
        *label = "Original".into();
    }
    let id = sheet.add_canvas_object(object).unwrap();
    let mut h = harness(sheet);
    h.get_by_label("Triangle: Original").focus();
    h.run_steps(2);
    h.key_press(Key::F2);
    h.run_steps(2);
    let object = h.state().doc.sheet().canvas.object(id).unwrap();
    let label_rect = thinoda_core::board::shape_label_rect(ShapeKind::Triangle, object.rect);
    let center = thinoda_core::board::rotate_around(
        label_rect.center(),
        object.rect.center(),
        object.rotation,
    );
    let expected = h
        .state()
        .view
        .xf()
        .rect(thinoda_core::Rect::from_center_size(
            center,
            label_rect.size,
        ))
        .expand(2.0);
    let editor = h.get_by_label("Board object label").rect();
    assert!(
        editor.min.distance(expected.min) < 1.0,
        "editor {editor:?} should start at {expected:?}"
    );
    h.get_by_label("Board object label").type_text("Updated");
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert_eq!(
        h.state().doc.sheet().canvas.object(id).unwrap().label(),
        "Updated"
    );
    assert!(h.state_mut().doc.undo());
    assert_eq!(
        h.state().doc.sheet().canvas.object(id).unwrap().label(),
        "Original"
    );
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn typing_on_a_selected_shape_keeps_every_character() {
    for (steps, original) in [(1, ""), (1, "Old"), (2, "Old"), (3, "T")] {
        let mut sheet = board();
        let id = shape(&mut sheet, 0.0, 0.0, original);
        let mut h = harness(sheet);
        let name = if original.is_empty() {
            "Rectangle".to_owned()
        } else {
            format!("Rectangle: {original}")
        };
        h.get_by_label(&name).focus();
        h.run_steps(2);
        for ch in ["T", "e", "s", "t"] {
            h.input_mut().events.push(egui::Event::Text(ch.into()));
            h.run_steps(steps);
        }
        h.key_press(Key::Enter);
        h.run_steps(2);
        assert_eq!(
            h.state().doc.sheet().canvas.object(id).unwrap().label(),
            "Test",
            "steps={steps}, original={original:?}"
        );
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn label_editor_text_contrasts_with_the_shape_fill_in_both_themes() {
    let sheet = board();
    let luminance = |c: egui::Color32| {
        0.2126 * f32::from(c.r()) + 0.7152 * f32::from(c.g()) + 0.0722 * f32::from(c.b())
    };
    for dark in [false, true] {
        for fill in [
            Some(thinoda_core::Color::rgb(255, 245, 200)),
            Some(thinoda_core::Color::rgb(30, 40, 60)),
            None,
        ] {
            let style = thinoda_core::board::ObjectStyle {
                fill,
                ..Default::default()
            };
            let (text, background) = crate::render::board_label_colors(&sheet, &style, dark);
            assert_eq!(background.a(), 255, "the editor backdrop is opaque");
            assert!(
                (luminance(text) - luminance(background)).abs() > 100.0,
                "dark={dark}, fill={fill:?}: {text:?} on {background:?}"
            );
        }
    }
}

fn rotated_image_card(angle: f32) -> (Sheet, TopicId) {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(160, 80)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    let mut sheet = board();
    let mut card = Card::new(CardKind::Image, Vec2::new(340.0, 260.0));
    card.rotation = angle;
    let id = sheet.add_card("Original", Vec2::ZERO, card).unwrap();
    thinoda_core::images::set_image(
        &mut sheet,
        id,
        Some((
            thinoda_core::resources::Resource::new("test.png", "image/png", bytes.into_inner())
                .unwrap(),
            Vec2::new(160.0, 80.0),
        )),
    )
    .unwrap();
    let topic = sheet.topic_mut(id).unwrap();
    topic.todo = Some(false);
    topic.hyperlink = Some("https://example.com".into());
    (sheet, id)
}

#[test]
fn rotated_card_title_editor_tracks_the_title_and_keeps_undo() {
    for angle in [45.0_f32, 90.0, 270.0] {
        for zoom in [0.5, 1.0, 2.0] {
            let (sheet, id) = rotated_image_card(angle);
            let mut h = harness(sheet);
            h.state_mut().view.set_zoom(zoom);
            {
                let state = h.state_mut();
                state.view.select_only(id);
                state.view.begin_edit(state.doc.sheet(), id, true);
            }
            h.run_steps(3);
            let layout = {
                let state = h.state_mut();
                state.view.layout_for(&state.doc)
            };
            let topic = &layout.topics[&id];
            let center = thinoda_core::board::rotate_around(
                topic.title_rect.center(),
                topic.rect.center(),
                angle,
            );
            let expected = h
                .state()
                .view
                .xf()
                .rect(thinoda_core::Rect::from_center_size(
                    center,
                    topic.title_rect.size,
                ))
                .expand(2.0);
            let editor = h.get_by_label("Topic title").rect();
            assert!(
                editor.min.distance(expected.min) < 1.0,
                "editor {editor:?} should start at {expected:?} for {angle} degrees, {zoom} zoom"
            );
            h.get_by_label("Topic title").type_text("Updated");
            h.run_steps(2);
            h.key_press(Key::Enter);
            h.run_steps(2);
            assert_eq!(h.state().doc.sheet().topic(id).unwrap().title, "Updated");
            assert!(h.state_mut().doc.undo());
            assert_eq!(h.state().doc.sheet().topic(id).unwrap().title, "Original");
            assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
        }
    }
}

#[test]
fn rotated_card_title_editor_keeps_ime_composition() {
    let (sheet, id) = rotated_image_card(90.0);
    let mut h = harness(sheet);
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    {
        let state = h.state_mut();
        state.view.select_only(id);
        state.view.begin_edit(state.doc.sheet(), id, true);
    }
    h.run_steps(3);
    h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
        text: "nihon".into(),
        active_range_chars: None,
    }));
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert_eq!(h.state().view.editing_topic(), Some(id));
    assert_eq!(h.state().doc.sheet().topic(id).unwrap().title, "Original");
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
    assert_eq!(h.state().view.editing_topic(), Some(id));
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(h.state().doc.sheet().topic(id).unwrap().title, "日本");
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.sheet().topic(id).unwrap().title, "Original");
}

#[test]
fn rotated_card_todo_and_link_hit_their_visible_positions() {
    for angle in [45.0_f32, 90.0, 270.0] {
        let (sheet, id) = rotated_image_card(angle);
        let mut h = harness(sheet);
        let layout = {
            let state = h.state_mut();
            state.view.layout_for(&state.doc)
        };
        let topic = &layout.topics[&id];
        let rotate = |point| thinoda_core::board::rotate_around(point, topic.rect.center(), angle);
        let todo = point(&h, rotate(topic.todo_rect.unwrap().center()));
        click(&mut h, todo, Modifiers::NONE);
        assert_eq!(h.state().doc.sheet().topic(id).unwrap().todo, Some(true));
        let link = topic
            .icons
            .iter()
            .find_map(|(icon, rect)| (*icon == Icon::Link).then_some(rect.center()))
            .expect("card link icon");
        let link = point(&h, rotate(link));
        click(&mut h, link, Modifiers::NONE);
        assert_eq!(h.state().open_links, ["https://example.com"]);
        assert!(h.state_mut().doc.undo());
        assert_eq!(h.state().doc.sheet().topic(id).unwrap().todo, Some(false));
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn rotated_card_touch_cancel_does_not_activate_the_todo() {
    let (sheet, id) = rotated_image_card(90.0);
    let mut h = harness(sheet);
    let layout = {
        let state = h.state_mut();
        state.view.layout_for(&state.doc)
    };
    let topic = &layout.topics[&id];
    let todo = thinoda_core::board::rotate_around(
        topic.todo_rect.unwrap().center(),
        topic.rect.center(),
        90.0,
    );
    let todo = point(&h, todo);
    touch(&mut h, egui::TouchPhase::Start, todo);
    touch(&mut h, egui::TouchPhase::Cancel, todo);
    touch(&mut h, egui::TouchPhase::End, todo);
    h.run_steps(2);
    assert_eq!(h.state().doc.sheet().topic(id).unwrap().todo, Some(false));
    touch(&mut h, egui::TouchPhase::Start, todo);
    touch(&mut h, egui::TouchPhase::End, todo);
    h.run_steps(2);
    assert_eq!(h.state().doc.sheet().topic(id).unwrap().todo, Some(true));
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.sheet().topic(id).unwrap().todo, Some(false));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn rotated_card_double_click_does_not_edit_its_old_axis_aligned_footprint() {
    let mut sheet = board();
    let mut card = Card::new(CardKind::Sticky, Vec2::new(300.0, 100.0));
    card.rotation = 90.0;
    let id = sheet.add_card("Card", Vec2::ZERO, card).unwrap();
    let mut h = harness(sheet);
    let layout = {
        let state = h.state_mut();
        state.view.layout_for(&state.doc)
    };
    let rect = layout.topics[&id].rect;
    let ghost = rect.center() + Vec2::new(120.0, 0.0);
    assert!(rect.contains(ghost));
    assert!(!thinoda_core::board::rotated_rect_contains(
        rect, 90.0, ghost
    ));
    let ghost = point(&h, ghost);
    click(&mut h, ghost, Modifiers::NONE);
    click(&mut h, ghost, Modifiers::NONE);
    assert_ne!(h.state().view.editing_topic(), Some(id));
    assert_eq!(h.state().doc.sheet().topic(id).unwrap().title, "Card");
}

#[test]
fn rotated_card_collapse_badge_moves_with_its_card() {
    let mut sheet = board();
    let mut card = Card::new(CardKind::Sticky, Vec2::new(220.0, 120.0));
    card.rotation = 90.0;
    let id = sheet.add_card("Card", Vec2::ZERO, card).unwrap();
    sheet.add_child(id, None, "Child");
    sheet.set_collapsed(id, true).unwrap();
    let mut h = harness(sheet);
    let layout = {
        let state = h.state_mut();
        state.view.layout_for(&state.doc)
    };
    let topic = &layout.topics[&id];
    let badge = thinoda_core::board::rotate_around(topic.badge_center(), topic.rect.center(), 90.0);
    let badge = point(&h, badge);
    click(&mut h, badge, Modifiers::NONE);
    assert!(!h.state().doc.sheet().topic(id).unwrap().collapsed);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn rotated_card_add_child_handle_follows_its_anchor() {
    let mut sheet = board();
    let card = sheet
        .add_card(
            "Card",
            Vec2::ZERO,
            Card::new(CardKind::Sticky, Vec2::new(220.0, 120.0)),
        )
        .unwrap();
    let layout = thinoda_core::layout_sheet(&sheet, &thinoda_core::ApproxMeasure, false);
    let mut view = MapView {
        canvas_rect: Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 900.0)),
        ..Default::default()
    };
    for zoom in [0.5, 1.0, 2.0] {
        view.set_zoom(zoom);
        sheet
            .topic_mut(card)
            .unwrap()
            .card
            .as_mut()
            .unwrap()
            .rotation = 0.0;
        let original = topic_add_handles(&view, &sheet, &layout, card);
        assert_eq!(original.len(), 1);
        let pivot = view.xf().pos(layout.topics[&card].rect.center());
        for angle in [45.0_f32, 90.0, 270.0] {
            sheet
                .topic_mut(card)
                .unwrap()
                .card
                .as_mut()
                .unwrap()
                .rotation = angle;
            let actual = topic_add_handles(&view, &sheet, &layout, card);
            let rotation = egui::emath::Rot2::from_angle(angle.to_radians());
            for (original, actual) in original.iter().zip(actual) {
                let anchor = pivot + rotation * (original.anchor - pivot);
                let center = pivot + rotation * (original.center - pivot);
                assert!(actual.anchor.distance(anchor) < 0.01);
                assert!(actual.center.distance(center) < 0.01);
            }
        }
    }
}

#[test]
fn rotated_card_child_handle_remains_clickable_and_undoable() {
    for structure in [
        thinoda_core::Structure::LogicRight,
        thinoda_core::Structure::OrgUp,
    ] {
        let mut sheet = board();
        sheet.structure = structure;
        let mut card = Card::new(CardKind::Sticky, Vec2::new(220.0, 120.0));
        card.rotation = 90.0;
        let card = sheet.add_card("Card", Vec2::ZERO, card).unwrap();
        let mut h = harness(sheet);
        h.state_mut().view.show_add_controls = true;
        h.state_mut().view.select_only(card);
        h.run_steps(3);
        let before = h.state().doc.workbook().clone();
        let handle = h.get_by_label("Add child topic").rect().center();
        click(&mut h, handle, Modifiers::NONE);
        let added = h.state().view.primary().unwrap();
        assert_ne!(added, card, "{structure:?}: {:?}", h.state().errors);
        assert_eq!(h.state().doc.sheet().parent(added), Some(card));
        assert!(h.state_mut().doc.undo());
        assert_eq!(h.state().doc.workbook(), &before);
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn image_card_accessible_name_preserves_its_kind_when_renamed() {
    let mut sheet = board();
    let id = sheet
        .add_card(
            "Diagram",
            Vec2::ZERO,
            Card::new(CardKind::Image, Vec2::new(120.0, 100.0)),
        )
        .unwrap();
    let mut h = harness(sheet);
    h.get_by_label("Image card: Diagram").focus();
    h.run_steps(2);
    assert_eq!(h.state().view.primary(), Some(id));

    h.state_mut()
        .doc
        .edit_sheet("Rename image", |sheet| {
            sheet.set_title(id, "Renamed diagram")
        })
        .unwrap();
    h.run_steps(2);
    h.get_by_label("Image card: Renamed diagram, Selected");

    h.state_mut().view.board_labels.image = "Bildkarte".into();
    h.run_steps(2);
    h.get_by_label("Bildkarte: Renamed diagram, Selected");
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn pending_text_is_applied_before_finishing_board_labels() {
    for connector in [false, true] {
        for key in [Key::Enter, Key::Tab, Key::Escape] {
            let mut sheet = board();
            let id = if connector {
                let mut line = CanvasConnector::new(
                    Endpoint::Point(Vec2::new(-100.0, 0.0)),
                    Endpoint::Point(Vec2::new(100.0, 0.0)),
                );
                line.label = "Original".into();
                sheet
                    .add_canvas_object(CanvasObject::connector(line))
                    .unwrap()
            } else {
                shape(&mut sheet, 0.0, 0.0, "Original")
            };
            let mut h = harness(sheet);
            {
                let state = h.state_mut();
                assert!(state.view.begin_board_label(&state.doc, id));
            }
            h.run_steps(3);
            h.input_mut().events.extend([
                egui::Event::Text("Fast label".into()),
                egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                },
            ]);
            h.run_steps(3);
            let expected = if key == Key::Escape {
                "Original"
            } else {
                "Fast label"
            };
            assert_eq!(
                h.state().doc.sheet().canvas.object(id).unwrap().label(),
                expected,
                "connector={connector}, key={key:?}"
            );
            assert_eq!(h.state().view.editing_board_object(), None);
            assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
            if key != Key::Escape {
                assert!(h.state_mut().doc.undo());
                assert_eq!(
                    h.state().doc.sheet().canvas.object(id).unwrap().label(),
                    "Original"
                );
            }
        }
    }
}

#[test]
fn shift_enter_inserts_a_newline_in_shape_and_connector_labels() {
    for connector in [false, true] {
        let mut sheet = board();
        let id = if connector {
            let mut line = CanvasConnector::new(
                Endpoint::Point(Vec2::new(-100.0, 0.0)),
                Endpoint::Point(Vec2::new(100.0, 0.0)),
            );
            line.label = "Original".into();
            sheet
                .add_canvas_object(CanvasObject::connector(line))
                .unwrap()
        } else {
            shape(&mut sheet, 0.0, 0.0, "Original")
        };
        let mut h = harness(sheet);
        {
            let state = h.state_mut();
            assert!(state.view.begin_board_label(&state.doc, id));
        }
        h.run_steps(3);
        h.input_mut().events.push(egui::Event::Text("First".into()));
        h.run_steps(2);
        h.key_press_modifiers(Modifiers::SHIFT, Key::Enter);
        h.run_steps(2);
        assert_eq!(
            h.state().view.board_label_draft(),
            Some((id, "Original", "First\n")),
            "connector={connector}"
        );
        h.input_mut()
            .events
            .push(egui::Event::Text("Second".into()));
        h.run_steps(2);
        h.key_press(Key::Enter);
        h.run_steps(3);
        assert_eq!(
            h.state().doc.sheet().canvas.object(id).unwrap().label(),
            "First\nSecond",
            "connector={connector}"
        );
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
        assert!(h.state_mut().doc.undo());
        assert_eq!(
            h.state().doc.sheet().canvas.object(id).unwrap().label(),
            "Original"
        );
    }
}

#[test]
fn ime_candidate_keys_do_not_finish_board_label_editing() {
    for key in [Key::Enter, Key::Tab, Key::Escape] {
        let mut sheet = board();
        let id = shape(&mut sheet, 0.0, 0.0, "Original");
        let mut h = harness(sheet);
        {
            let state = h.state_mut();
            assert!(state.view.begin_board_label(&state.doc, id));
        }
        h.run_steps(3);
        h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "composition".into(),
            active_range_chars: None,
        }));
        h.run_steps(2);
        h.key_press(key);
        h.run_steps(2);
        assert_eq!(h.state().view.editing_board_object(), Some(id), "{key:?}");
        assert_eq!(
            h.state().doc.sheet().canvas.object(id).unwrap().label(),
            "Original"
        );
        h.input_mut().events.extend([
            egui::Event::Ime(egui::ImeEvent::Commit("Committed".into())),
            egui::Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
        ]);
        h.run_steps(2);
        assert_eq!(h.state().view.editing_board_object(), Some(id));
        assert_eq!(
            h.state().view.board_label_draft(),
            Some((id, "Original", "Committed"))
        );
        h.key_press(Key::Enter);
        h.run_steps(3);
        assert_eq!(h.state().view.editing_board_object(), None);
        assert_eq!(
            h.state().doc.sheet().canvas.object(id).unwrap().label(),
            "Committed"
        );
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn failed_same_frame_board_label_commit_keeps_text_and_focus() {
    let mut sheet = board();
    let id = shape(&mut sheet, 0.0, 0.0, "Original");
    let mut h = harness(sheet);
    {
        let state = h.state_mut();
        assert!(state.view.begin_board_label(&state.doc, id));
    }
    h.run_steps(3);
    h.state_mut()
        .doc
        .update_canvas_object("Concurrent edit", id, |object| {
            if let CanvasObjectKind::Shape { label, .. } = &mut object.kind {
                *label = "External".into();
            }
        })
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
    assert_eq!(
        h.state().doc.sheet().canvas.object(id).unwrap().label(),
        "External"
    );
    assert_eq!(
        h.state().view.board_label_draft(),
        Some((id, "Original", "Pending draft"))
    );
    assert_eq!(
        h.ctx.memory(|memory| memory.focused()),
        Some(MapCanvas::board_label_editor_id())
    );
    assert!(!h.state().errors.is_empty());
}

#[test]
fn escape_and_touch_cancel_discard_in_progress_gestures() {
    let mut sheet = board();
    let id = shape(&mut sheet, 0.0, 0.0, "Move");
    let mut h = harness(sheet);
    let before = h.state().doc.workbook().clone();
    let start = point(&h, Vec2::ZERO);
    let end = start + vec2(75.0, 40.0);
    drag(&mut h, start, end);
    h.key_press(Key::Escape);
    h.run_steps(1);
    finish_drag(&mut h, end);
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state().doc.sheet().canvas.object(id).is_some());

    set_tool(&mut h, BoardTool::Text);
    let start = point(&h, Vec2::new(-200.0, -180.0));
    let end = start + vec2(150.0, 100.0);
    touch(&mut h, egui::TouchPhase::Start, start);
    touch(&mut h, egui::TouchPhase::Move, end);
    touch(&mut h, egui::TouchPhase::Cancel, end);
    touch(&mut h, egui::TouchPhase::End, end);
    h.run_steps(2);
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn groups_lock_unlock_band_and_arrange_as_units() {
    let mut sheet = board();
    let first = shape(&mut sheet, -200.0, -100.0, "First");
    let second = shape(&mut sheet, -40.0, 0.0, "Second");
    let third = shape(&mut sheet, 250.0, 180.0, "Third");
    let mut h = harness(sheet);
    h.state_mut().view.board_selection = vec![first, second];
    {
        let state = h.state_mut();
        assert!(apply_board_action(&mut state.doc, &mut state.view, BoardAction::Group).unwrap());
    }
    let group = h.state().doc.sheet().canvas.object(first).unwrap().group;
    assert!(group.is_some());
    assert_eq!(
        h.state().doc.sheet().canvas.object(second).unwrap().group,
        group
    );
    {
        let state = h.state_mut();
        assert!(
            apply_board_action(&mut state.doc, &mut state.view, BoardAction::Lock(true)).unwrap()
        );
    }
    let locked = h.state().doc.workbook().clone();
    assert!(h.state().doc.sheet().canvas.is_locked(first));
    assert!(h.state().doc.sheet().canvas.is_locked(second));
    {
        let state = h.state_mut();
        assert!(!BoardAction::Rotate(15.0).available(&state.doc, &state.view));
        assert!(
            !apply_board_action(
                &mut state.doc,
                &mut state.view,
                BoardAction::ZOrder(ZOrder::Front)
            )
            .unwrap()
        );
        assert!(
            !apply_board_action(&mut state.doc, &mut state.view, BoardAction::Ungroup).unwrap()
        );
    }
    assert_eq!(h.state().doc.workbook(), &locked);
    {
        let state = h.state_mut();
        assert!(
            apply_board_action(&mut state.doc, &mut state.view, BoardAction::Lock(false)).unwrap()
        );
        assert!(
            apply_board_action(
                &mut state.doc,
                &mut state.view,
                BoardAction::SetBand(Band::Below)
            )
            .unwrap()
        );
    }
    for id in [first, second] {
        assert_eq!(
            h.state().doc.sheet().canvas.object(id).unwrap().band,
            Band::Below
        );
        assert!(!h.state().doc.sheet().canvas.is_locked(id));
    }
    assert!(h.state_mut().doc.undo());
    assert_eq!(
        h.state().doc.sheet().canvas.object(first).unwrap().band,
        Band::Above
    );

    h.state_mut().view.board_selection = vec![first, third];
    let before_arrange = h.state().doc.workbook().clone();
    let group_delta = h
        .state()
        .doc
        .sheet()
        .canvas
        .object(second)
        .unwrap()
        .rect
        .center()
        - h.state()
            .doc
            .sheet()
            .canvas
            .object(first)
            .unwrap()
            .rect
            .center();
    {
        let state = h.state_mut();
        assert!(
            apply_board_action(
                &mut state.doc,
                &mut state.view,
                BoardAction::Arrange(crate::FloatingArrangement::Left)
            )
            .unwrap()
        );
    }
    let after_delta = h
        .state()
        .doc
        .sheet()
        .canvas
        .object(second)
        .unwrap()
        .rect
        .center()
        - h.state()
            .doc
            .sheet()
            .canvas
            .object(first)
            .unwrap()
            .rect
            .center();
    assert_eq!(after_delta, group_delta);
    assert_ne!(h.state().doc.workbook(), &before_arrange);
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before_arrange);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn shape_resize_rotation_and_invalid_commands_preserve_undo_boundaries() {
    let mut sheet = board();
    let id = shape(&mut sheet, 0.0, 0.0, "Resize");
    let mut h = harness(sheet);
    let center = point(&h, Vec2::ZERO);
    click(&mut h, center, Modifiers::NONE);
    let before = h.state().doc.workbook().clone();
    let handle = h.get_by_label("Resize board item").rect().center();
    let end = handle + vec2(60.0, 30.0);
    drag(&mut h, handle, end);
    assert_eq!(h.state().doc.workbook(), &before);
    finish_drag(&mut h, end);
    let rect = h.state().doc.sheet().canvas.object(id).unwrap().rect;
    assert_eq!(rect.size, Vec2::new(180.0, 110.0));
    let resized = h.state().doc.workbook().clone();
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state_mut().doc.redo());
    h.run_steps(2);

    let handle = h.get_by_label("Rotate board item").rect().center();
    let center = point(&h, rect.center());
    let offset = handle - center;
    let end = center + vec2(-offset.y, offset.x);
    drag(&mut h, handle, end);
    assert_eq!(h.state().doc.workbook(), &resized);
    finish_drag(&mut h, end);
    assert!((h.state().doc.sheet().canvas.object(id).unwrap().rotation - 90.0).abs() < 0.1);
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &resized);
    {
        let state = h.state_mut();
        assert!(!BoardAction::Rotate(f32::NAN).available(&state.doc, &state.view));
        assert!(
            !apply_board_action(
                &mut state.doc,
                &mut state.view,
                BoardAction::Rotate(f32::NAN)
            )
            .unwrap()
        );
    }
    assert_eq!(h.state().doc.workbook(), &resized);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn existing_image_card_has_separate_undoable_frame_and_image_resize_handles() {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(160, 80)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    let mut sheet = board();
    let id = sheet
        .add_card(
            "Image",
            Vec2::ZERO,
            Card::new(CardKind::Image, Vec2::new(300.0, 240.0)),
        )
        .unwrap();
    thinoda_core::images::set_image(
        &mut sheet,
        id,
        Some((
            thinoda_core::resources::Resource::new("test.png", "image/png", bytes.into_inner())
                .unwrap(),
            Vec2::new(160.0, 80.0),
        )),
    )
    .unwrap();
    let mut h = harness(sheet);
    h.state_mut().view.select_only(id);
    h.run_steps(2);
    let before = h.state().doc.workbook().clone();
    let image_handle = h.get_by_label("Resize image").rect().center();
    let end = image_handle + vec2(40.0, 20.0);
    drag(&mut h, image_handle, end);
    finish_drag(&mut h, end);
    let topic = h.state().doc.sheet().topic(id).unwrap();
    assert_eq!(topic.image_size, Some(Vec2::new(200.0, 100.0)));
    assert_eq!(topic.card.unwrap().size, Vec2::new(300.0, 240.0));
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
    h.run_steps(2);

    let card_handle = h.get_by_label("Resize board item").rect().center();
    let end = card_handle + vec2(40.0, 20.0);
    drag(&mut h, card_handle, end);
    finish_drag(&mut h, end);
    let topic = h.state().doc.sheet().topic(id).unwrap();
    assert_eq!(topic.card.unwrap().size, Vec2::new(340.0, 260.0));
    assert_eq!(topic.image_size, Some(Vec2::new(160.0, 80.0)));
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn keyboard_tool_creation_and_failed_label_edits_keep_the_draft() {
    let mut h = harness(board());
    h.state_mut()
        .view
        .board_labels
        .shape_names
        .insert(ShapeKind::Diamond, "Localized diamond".into());
    set_tool(&mut h, BoardTool::Shape(ShapeKind::Diamond));
    h.key_press(Key::Enter);
    h.run_steps(2);
    let id = h.state().view.board_selection[0];
    assert!(matches!(
        h.state().doc.sheet().canvas.object(id).unwrap().kind,
        CanvasObjectKind::Shape {
            shape: ShapeKind::Diamond,
            ..
        }
    ));
    set_tool(&mut h, BoardTool::Select);
    assert!(h.query_by_label("Localized diamond, Selected").is_some());
    h.key_press(Key::F2);
    h.run_steps(2);
    let too_long = "x".repeat(thinoda_core::board::MAX_BOARD_TEXT_CHARS + 1);
    h.get_by_label("Board object label").type_text(&too_long);
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert_eq!(h.state().view.editing_board_object(), Some(id));
    assert_eq!(h.state().doc.sheet().canvas.object(id).unwrap().label(), "");
    assert!(h.state().errors.last().is_some());
    assert!(h.state().view.has_board_label_changes());
    assert_eq!(
        h.state().view.board_label_draft(),
        Some((id, "", too_long.as_str()))
    );
    assert_eq!(
        h.get_by_label("Board object label").value().as_deref(),
        Some(too_long.as_str())
    );
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert_eq!(h.state().view.editing_board_object(), None);
    assert_eq!(h.state().view.board_label_draft(), None);
    assert!(!h.state().view.has_board_label_changes());
    assert_eq!(h.state().doc.sheet().canvas.object(id).unwrap().label(), "");
}

#[test]
fn board_label_draft_validates_original_document_and_survives_rejected_commits() {
    let mut sheet = board();
    let card = sheet
        .add_card(
            "Card",
            Vec2::new(-200.0, 0.0),
            Card::new(CardKind::Sticky, Vec2::new(120.0, 100.0)),
        )
        .unwrap();
    let id = shape(&mut sheet, 130.0, 0.0, "Original");
    sheet
        .group_board_members(&[BoardMember::Card(card), BoardMember::Object(id)])
        .unwrap();
    let mut h = harness(sheet);
    let other_sheet = h.state_mut().doc.add_sheet(board()).unwrap();
    h.state_mut().doc.set_active(0);
    {
        let state = h.state_mut();
        assert_eq!(state.view.board_label_draft(), None);
        assert!(!state.view.has_board_label_changes());
        assert_eq!(
            state.view.validated_board_label_draft(&state.doc).unwrap(),
            None
        );
        assert!(state.view.begin_board_label(&state.doc, id));
        assert_eq!(
            state.view.board_label_draft(),
            Some((id, "Original", "Original"))
        );
        assert!(!state.view.has_board_label_changes());
    }
    h.run_steps(2);
    h.get_by_label("Board object label").type_text("Draft");
    h.run_steps(2);
    {
        let state = h.state();
        assert!(state.view.has_board_label_changes());
        assert_eq!(
            state.view.validated_board_label_draft(&state.doc).unwrap(),
            Some((id, "Original", "Draft"))
        );
        let cloned = Document::new(state.doc.workbook().clone());
        assert!(state.view.validated_board_label_draft(&cloned).is_err());
    }

    h.state_mut().doc.set_active(other_sheet);
    assert!(
        h.state()
            .view
            .validated_board_label_draft(&h.state().doc)
            .is_err()
    );
    assert_eq!(
        h.state().view.board_label_draft(),
        Some((id, "Original", "Draft"))
    );
    h.state_mut().doc.set_active(0);
    h.state_mut()
        .doc
        .set_board_members_locked(&[BoardMember::Card(card)], true)
        .unwrap();
    {
        let state = h.state_mut();
        assert!(state.view.validated_board_label_draft(&state.doc).is_err());
        assert!(state.view.try_commit_board_label(&mut state.doc).is_err());
        assert_eq!(
            state.view.board_label_draft(),
            Some((id, "Original", "Draft"))
        );
        assert!(state.doc.undo());
        assert_eq!(
            state.view.validated_board_label_draft(&state.doc).unwrap(),
            Some((id, "Original", "Draft"))
        );
        assert!(state.view.try_commit_board_label(&mut state.doc).unwrap());
        assert_eq!(
            state.doc.sheet().canvas.object(id).unwrap().label(),
            "Draft"
        );
        assert_eq!(state.view.board_label_draft(), None);
        assert!(!state.view.has_board_label_changes());
    }
}

#[test]
fn connector_label_draft_is_available_for_snapshot_recovery() {
    let mut sheet = board();
    let mut connector = CanvasConnector::new(
        Endpoint::Point(Vec2::new(-100.0, 0.0)),
        Endpoint::Point(Vec2::new(100.0, 0.0)),
    );
    connector.label = "Before".into();
    let id = sheet
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    let mut h = harness(sheet);
    {
        let state = h.state_mut();
        assert!(state.view.begin_board_label(&state.doc, id));
    }
    h.run_steps(2);
    h.get_by_label("Board object label").type_text("After");
    h.run_steps(2);
    {
        let state = h.state_mut();
        assert_eq!(
            state.view.validated_board_label_draft(&state.doc).unwrap(),
            Some((id, "Before", "After"))
        );
        assert!(state.view.try_commit_board_label(&mut state.doc).unwrap());
        assert_eq!(
            state.doc.sheet().canvas.object(id).unwrap().label(),
            "After"
        );
        assert!(state.doc.undo());
        assert_eq!(
            state.doc.sheet().canvas.object(id).unwrap().label(),
            "Before"
        );
    }
}

#[test]
fn transparent_shape_and_connector_labels_remain_editable_without_recoloring() {
    for dark in [false, true] {
        for connector in [false, true] {
            let mut sheet = board();
            let id = if connector {
                let mut edge = CanvasConnector::new(
                    Endpoint::Point(Vec2::new(-100.0, 0.0)),
                    Endpoint::Point(Vec2::new(100.0, 0.0)),
                );
                edge.label = "Before".into();
                sheet
                    .add_canvas_object(CanvasObject::connector(edge))
                    .unwrap()
            } else {
                shape(&mut sheet, 0.0, 0.0, "Before")
            };
            sheet.canvas.object_mut(id).unwrap().style.text_color =
                Some(thinoda_core::Color::TRANSPARENT);
            let mut h = harness(sheet);
            h.state_mut().view.dark = dark;
            {
                let state = h.state_mut();
                assert!(state.view.begin_board_label(&state.doc, id));
            }
            h.run_steps(2);
            h.get_by_label("Board object label").type_text("After");
            h.run_steps(2);
            h.key_press(Key::Enter);
            h.run_steps(2);
            let object = h.state().doc.sheet().canvas.object(id).unwrap();
            assert_eq!(
                object.label(),
                "After",
                "dark={dark}, connector={connector}"
            );
            assert_eq!(
                object.style.text_color,
                Some(thinoda_core::Color::TRANSPARENT)
            );
            assert!(h.state_mut().doc.undo());
            let object = h.state().doc.sheet().canvas.object(id).unwrap();
            assert_eq!(object.label(), "Before");
            assert_eq!(
                object.style.text_color,
                Some(thinoda_core::Color::TRANSPARENT)
            );
            assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
        }
    }
}

#[test]
fn connector_label_editor_tracks_asymmetric_waypoint_path() {
    let mut sheet = board();
    let mut connector = CanvasConnector::new(
        Endpoint::Point(Vec2::new(-200.0, -100.0)),
        Endpoint::Point(Vec2::new(100.0, 100.0)),
    );
    connector.waypoints = vec![Vec2::new(0.0, -100.0), Vec2::new(0.0, 100.0)].into();
    connector.label = "Before".into();
    let id = sheet
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    let mut h = harness(sheet);
    {
        let state = h.state_mut();
        assert!(state.view.begin_board_label(&state.doc, id));
    }
    h.run_steps(2);
    let expected = h
        .state()
        .view
        .xf()
        .rect(thinoda_core::Rect::from_center_size(
            Vec2::new(0.0, -50.0),
            Vec2::new(140.0, 40.0),
        ))
        .expand(2.0);
    let editor = h.get_by_label("Board object label").rect();
    assert!(
        editor.min.distance(expected.min) < 1.0,
        "editor {editor:?} should start at {expected:?}"
    );
    h.get_by_label("Board object label").type_text("After");
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert_eq!(
        h.state().doc.sheet().canvas.object(id).unwrap().label(),
        "After"
    );
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn changed_board_label_source_rejects_snapshot_but_retains_draft() {
    let mut sheet = board();
    let id = shape(&mut sheet, 0.0, 0.0, "Original");
    let mut h = harness(sheet);
    {
        let state = h.state_mut();
        assert!(state.view.begin_board_label(&state.doc, id));
    }
    h.run_steps(2);
    h.get_by_label("Board object label").type_text("Draft");
    h.run_steps(2);
    {
        let state = h.state_mut();
        state
            .doc
            .update_canvas_object("External edit", id, |object| {
                if let CanvasObjectKind::Shape { label, .. } = &mut object.kind {
                    *label = "External".into();
                }
            })
            .unwrap();
        assert!(state.view.validated_board_label_draft(&state.doc).is_err());
        assert!(state.view.try_commit_board_label(&mut state.doc).is_err());
        assert_eq!(
            state.view.board_label_draft(),
            Some((id, "Original", "Draft"))
        );
        assert!(state.doc.undo());
        assert_eq!(
            state.view.validated_board_label_draft(&state.doc).unwrap(),
            Some((id, "Original", "Draft"))
        );
    }
}

#[test]
fn external_camera_change_cancels_the_uncommitted_move() {
    let mut sheet = board();
    shape(&mut sheet, 0.0, 0.0, "Move");
    let mut h = harness(sheet);
    let before = h.state().doc.workbook().clone();
    let start = point(&h, Vec2::ZERO);
    let end = start + vec2(60.0, 30.0);
    drag(&mut h, start, end);
    h.state_mut().view.offset += vec2(10.0, 0.0);
    h.run_steps(1);
    finish_drag(&mut h, end);
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(
        h.state()
            .errors
            .iter()
            .any(|error| error.to_string().contains("canceled"))
    );
}

#[test]
fn replacing_document_at_same_revision_does_not_commit_a_gesture_or_label_draft() {
    let mut sheet = board();
    let id = shape(&mut sheet, 0.0, 0.0, "Original");
    let mut h = harness(sheet);
    let before = h.state().doc.workbook().clone();
    let start = point(&h, Vec2::ZERO);
    let end = start + vec2(60.0, 30.0);
    drag(&mut h, start, end);
    for _ in 0..2 {
        let replacement = Document::new(h.state().doc.workbook().clone());
        assert_eq!(replacement.state_id(), h.state().doc.state_id());
        h.state_mut().doc = replacement;
    }
    h.run_steps(1);
    finish_drag(&mut h, end);
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(
        h.state()
            .errors
            .iter()
            .any(|error| error.to_string().contains("canceled"))
    );

    h.state_mut().view.board_selection = vec![id];
    h.run_steps(2);
    h.key_press(Key::F2);
    h.run_steps(2);
    h.get_by_label("Board object label").type_text("Draft");
    h.run_steps(2);
    for _ in 0..2 {
        let replacement = Document::new(h.state().doc.workbook().clone());
        h.state_mut().doc = replacement;
    }
    {
        let state = h.state_mut();
        assert!(state.view.try_commit_board_label(&mut state.doc).is_err());
        assert_eq!(state.view.editing_board_object(), Some(id));
        assert_eq!(
            state.doc.sheet().canvas.object(id).unwrap().label(),
            "Original"
        );
    }
}

#[test]
fn persistent_frame_moves_by_touch_and_resizes_by_mouse() {
    let mut sheet = board();
    let rect =
        thinoda_core::Rect::from_min_size(Vec2::new(-250.0, -180.0), Vec2::new(400.0, 300.0));
    let id = sheet.create_board_frame("Frame", rect).unwrap();
    let mut h = harness(sheet);
    let before = h.state().doc.workbook().clone();
    let start = point(&h, rect.min + Vec2::new(100.0, 0.0));
    let end = start + vec2(50.0, 30.0);
    touch(&mut h, egui::TouchPhase::Start, start);
    touch(&mut h, egui::TouchPhase::Move, start + vec2(25.0, 15.0));
    touch(&mut h, egui::TouchPhase::Move, end);
    assert_eq!(h.state().doc.workbook(), &before);
    touch(&mut h, egui::TouchPhase::End, end);
    h.run_steps(2);
    assert_eq!(
        h.state().view.selected_object,
        Some(ObjectSelection::Zone(id))
    );
    let moved = h.state().doc.sheet().zones[0].frame;
    assert_eq!(moved, rect.translate(Vec2::new(50.0, 30.0)));
    let after_move = h.state().doc.workbook().clone();

    let corner = point(&h, moved.max());
    let end = corner + vec2(60.0, 40.0);
    drag(&mut h, corner, end);
    assert_eq!(h.state().doc.workbook(), &after_move);
    finish_drag(&mut h, end);
    assert_eq!(
        h.state().doc.sheet().zones[0].frame,
        thinoda_core::Rect::from_min_size(moved.min, moved.size + Vec2::new(60.0, 40.0))
    );
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &after_move);
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn frame_clipping_limits_pointer_selection_and_accessible_items() {
    let mut sheet = board();
    let hidden = sheet
        .add_card(
            "Clipped card",
            Vec2::new(400.0, 0.0),
            Card::new(CardKind::Sticky, Vec2::new(100.0, 80.0)),
        )
        .unwrap();
    let frame =
        thinoda_core::Rect::from_min_size(Vec2::new(-100.0, -100.0), Vec2::new(200.0, 200.0));
    sheet
        .create_zone("Clipping frame", vec![hidden], frame)
        .unwrap();
    sheet.zones[0].clip = true;
    let object = sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            thinoda_core::Rect::from_min_size(Vec2::new(-80.0, -40.0), Vec2::new(240.0, 80.0)),
        ))
        .unwrap();
    let mut h = harness(sheet);
    assert!(h.query_by_label("Sticky note: Clipped card").is_none());
    let outside = point(&h, Vec2::new(130.0, 0.0));
    click(&mut h, outside, Modifiers::NONE);
    assert!(!h.state().view.board_selection.contains(&object));
    let inside = point(&h, Vec2::ZERO);
    click(&mut h, inside, Modifiers::NONE);
    assert_eq!(h.state().view.board_selection, vec![object]);
    {
        let state = h.state_mut();
        apply_action(&mut state.doc, &mut state.view, Action::SelectAll).unwrap();
    }
    assert!(!h.state().view.selection.contains(&hidden));
    assert!(
        !h.state()
            .view
            .selection
            .contains(&h.state().doc.sheet().root)
    );
    assert!(h.state().view.board_selection.contains(&object));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn select_all_on_a_central_map_with_board_objects_includes_both_bands() {
    let mut sheet = Sheet::new("Map", "Central");
    let root = sheet.root;
    let object = shape(&mut sheet, 250.0, 0.0, "Board object");
    let mut h = harness(sheet);
    {
        let state = h.state_mut();
        assert!(apply_action(&mut state.doc, &mut state.view, Action::SelectAll).unwrap());
    }
    assert!(h.state().view.selection.contains(&root));
    assert_eq!(h.state().view.board_selection, vec![object]);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn zoom_fit_unions_rotated_scene_bounds_with_layout_extents() {
    let mut sheet = board();
    let mut card = Card::new(CardKind::Sticky, Vec2::new(400.0, 80.0));
    card.rotation = 90.0;
    let card = sheet.add_card("Rotated", Vec2::ZERO, card).unwrap();
    let object = shape(&mut sheet, 700.0, 0.0, "Rotated shape");
    sheet.rotate_canvas_object(object, 45.0).unwrap();
    let mut h = harness(sheet);
    let state = h.state_mut();
    let layout = state.view.layout_for(&state.doc);
    let scene = build_scene(state.doc.sheet(), &layout);
    let bounds = fit_bounds(state.doc.sheet(), &layout);
    assert!(bounds.contains(layout.bounds.min));
    assert!(bounds.contains(layout.bounds.max()));
    for item in [SceneId::Topic(card), SceneId::Object(object)] {
        let item = scene.item(item).unwrap();
        assert!(bounds.contains(item.bounds.min), "{item:?}");
        assert!(bounds.contains(item.bounds.max()), "{item:?}");
    }
    state.view.request_fit();
    h.run_steps(2);
    let state = h.state();
    let available = state.view.canvas_rect.shrink(39.0);
    for item in [SceneId::Topic(card), SceneId::Object(object)] {
        let item = scene.item(item).unwrap();
        assert!(available.contains_rect(state.view.xf().rect(item.bounds)));
    }
}

#[test]
fn host_reveals_resolved_connector_after_sheet_switch_and_reset() {
    let mut next = board();
    shape(&mut next, -1500.0, 0.0, "Other side");
    let from = shape(&mut next, 2000.0, 0.0, "From");
    let to = shape(&mut next, 2250.0, 0.0, "To");
    let connector = next
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(from),
            Endpoint::Object(to),
        )))
        .unwrap();
    let stored = next.canvas.object(connector).unwrap().rect;

    let mut h = harness(board());
    {
        let state = h.state_mut();
        state.doc.add_sheet(next).unwrap();
        state.view.reset();
        state.view.set_initial_zoom(2.0);
        let before = state.view.offset;
        state.view.select_board_object(connector);
        state.view.reveal_board_object(connector);
        assert_eq!(state.view.offset, before, "reveal must wait for layout");
        assert_eq!(state.view.board_selection, vec![connector]);
        assert!(state.view.selection.is_empty());
        assert_eq!(state.view.selected_object, None);
    }
    h.run_steps(2);
    {
        let state = h.state_mut();
        let layout = state.view.layout_for(&state.doc);
        let scene = build_scene(state.doc.sheet(), &layout);
        let bounds = scene
            .item(SceneId::Object(connector))
            .unwrap()
            .visible_bounds()
            .unwrap();
        assert_ne!(stored, bounds, "connector endpoints must be resolved");
        assert!(
            state
                .view
                .canvas_rect
                .shrink(39.0)
                .contains_rect(state.view.xf().rect(bounds))
        );
    }

    {
        let state = h.state_mut();
        state
            .doc
            .move_canvas_objects(&[from, to], Vec2::new(1500.0, 300.0))
            .unwrap();
        let before = state.view.offset;
        state.view.reveal_board_object(connector);
        assert_eq!(state.view.offset, before);
    }
    h.run_steps(2);
    let state = h.state_mut();
    let layout = state.view.layout_for(&state.doc);
    let scene = build_scene(state.doc.sheet(), &layout);
    let bounds = scene
        .item(SceneId::Object(connector))
        .unwrap()
        .visible_bounds()
        .unwrap();
    assert!(
        state
            .view
            .canvas_rect
            .shrink(39.0)
            .contains_rect(state.view.xf().rect(bounds))
    );
    assert_eq!(state.view.board_selection, vec![connector]);
    assert!(state.errors.is_empty(), "{:?}", state.errors);
}

#[test]
fn host_reveal_uses_only_frame_clipped_object_bounds() {
    let mut sheet = board();
    let frame =
        thinoda_core::Rect::from_min_size(Vec2::new(900.0, -100.0), Vec2::new(200.0, 200.0));
    sheet.create_board_frame("Clip", frame).unwrap();
    sheet.zones[0].clip = true;
    let original =
        thinoda_core::Rect::from_min_size(Vec2::new(0.0, -40.0), Vec2::new(2000.0, 80.0));
    let object = sheet
        .add_canvas_object(CanvasObject::shape(ShapeKind::Rectangle, original))
        .unwrap();
    let mut h = harness(sheet);
    {
        let state = h.state_mut();
        state.view.offset = vec2(-1000.0, 0.0);
        let layout = state.view.layout_for(&state.doc);
        let scene = build_scene(state.doc.sheet(), &layout);
        let item = scene.item(SceneId::Object(object)).unwrap();
        assert_eq!(item.clip, Some(frame));
        assert!(
            state
                .view
                .canvas_rect
                .shrink(39.0)
                .contains_rect(state.view.xf().rect(item.visible_bounds().unwrap()))
        );
        assert!(
            !state
                .view
                .canvas_rect
                .contains_rect(state.view.xf().rect(original))
        );
        let before = state.view.offset;
        state.view.reveal_board_object(object);
        assert_eq!(state.view.offset, before);
    }
    let before = h.state().view.offset;
    h.run_steps(2);
    assert_eq!(h.state().view.offset, before);
    assert!(h.state().view.board_selection.is_empty());
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn mixed_clipboard_copy_paste_remaps_group_and_connector_endpoints() {
    let mut sheet = board();
    let card = sheet
        .add_card(
            "Card",
            Vec2::new(-100.0, 0.0),
            Card::new(CardKind::Sticky, Vec2::new(120.0, 80.0)),
        )
        .unwrap();
    let object = shape(&mut sheet, 100.0, 0.0, "Shape");
    let connector = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Topic(card),
            Endpoint::Object(object),
        )))
        .unwrap();
    let group = sheet
        .group_board_members(&[
            BoardMember::Card(card),
            BoardMember::Object(object),
            BoardMember::Object(connector),
        ])
        .unwrap();
    let mut h = harness(sheet);
    h.state_mut().view.selection = vec![card];
    let before = h.state().doc.workbook().clone();
    let text = {
        let state = h.state_mut();
        assert!(Action::Copy.available(&state.doc, &state.view));
        assert!(apply_action(&mut state.doc, &mut state.view, Action::Copy).unwrap());
        assert_eq!(state.doc.workbook(), &before);
        assert!(state.view.clipboard.is_none());
        let payload = state.view.board_clipboard().unwrap();
        assert_eq!(payload.source_topic_roots(), &[card]);
        assert_eq!(payload.source_object_ids().count(), 2);
        let text = state.view.clipboard_out.take().unwrap();
        assert_eq!(text, state.view.board_clipboard().unwrap().text);
        thinoda_core::clipboard::encode_board_payload(state.view.board_clipboard().unwrap())
            .unwrap()
    };
    {
        let state = h.state_mut();
        assert!(
            state
                .view
                .paste_encoded_board(&mut state.doc, &text.replace('\n', "\r\n"))
                .unwrap()
        );
        assert_eq!(state.view.selection.len(), 1);
        assert_eq!(state.view.board_selection.len(), 2);
        let new_card = state.view.selection[0];
        let new_shape = state
            .view
            .board_selection
            .iter()
            .copied()
            .find(|id| {
                matches!(
                    state.doc.sheet().canvas.object(*id).unwrap().kind,
                    CanvasObjectKind::Shape { .. }
                )
            })
            .unwrap();
        let new_connector = state
            .view
            .board_selection
            .iter()
            .copied()
            .find(|id| {
                matches!(
                    state.doc.sheet().canvas.object(*id).unwrap().kind,
                    CanvasObjectKind::Connector(_)
                )
            })
            .unwrap();
        assert_ne!(new_card, card);
        assert_ne!(new_shape, object);
        assert_ne!(new_connector, connector);
        assert_eq!(
            state.doc.sheet().topic(new_card).unwrap().position,
            Some(Vec2::new(-60.0, 40.0))
        );
        assert_eq!(
            state
                .doc
                .sheet()
                .canvas
                .object(new_shape)
                .unwrap()
                .rect
                .center(),
            Vec2::new(140.0, 40.0)
        );
        let new_group = state
            .doc
            .sheet()
            .board_member_group(BoardMember::Card(new_card))
            .unwrap();
        assert_ne!(new_group, group);
        assert_eq!(
            state
                .doc
                .sheet()
                .board_member_group(BoardMember::Object(new_shape)),
            Some(new_group)
        );
        let CanvasObjectKind::Connector(new_connector) =
            &state.doc.sheet().canvas.object(new_connector).unwrap().kind
        else {
            panic!("expected connector");
        };
        assert_eq!(new_connector.from, Endpoint::Topic(new_card));
        assert_eq!(new_connector.to, Endpoint::Object(new_shape));
        assert!(state.doc.undo());
        assert_eq!(state.doc.workbook(), &before);
    }
}

#[test]
fn formatted_card_clipboard_actions_preserve_runs_lists_and_history() {
    let mut sheet = board();
    let title = "Aéß\nÉlan";
    let card = sheet
        .add_card(
            title,
            Vec2::new(-100.0, 0.0),
            Card::new(CardKind::Sticky, Vec2::new(160.0, 100.0)),
        )
        .unwrap();
    let topic = sheet.topic_mut(card).unwrap();
    topic
        .inline_text
        .update_range(title, 0..3, |style| style.bold = Some(true))
        .unwrap();
    topic
        .inline_text
        .paragraphs
        .push(thinoda_core::InlineParagraph {
            index: 1,
            list: thinoda_core::ListKind::Bulleted,
        });
    let mut h = harness(sheet);
    h.state_mut().view.selection = vec![card];
    let before = h.state().doc.workbook().clone();
    let state = h.state_mut();
    assert!(apply_action(&mut state.doc, &mut state.view, Action::Copy).unwrap());
    let encoded =
        thinoda_core::clipboard::encode_board_payload(state.view.board_clipboard().unwrap())
            .unwrap();
    assert!(encoded.starts_with("THINODA-BOARD-CLIPBOARD-2\n"));
    assert!(
        state
            .view
            .paste_encoded_board(&mut state.doc, &encoded)
            .unwrap()
    );
    let pasted = state.view.selection[0];
    assert_ne!(pasted, card);
    assert_eq!(
        state.doc.sheet().topic(pasted).unwrap().inline_text,
        before.active().topic(card).unwrap().inline_text
    );
    let after = state.doc.workbook().clone();
    assert!(state.doc.undo());
    assert_eq!(state.doc.workbook(), &before);
    assert!(state.doc.redo());
    assert_eq!(state.doc.workbook(), &after);
    state.doc.workbook().validate().unwrap();
}

#[test]
fn mixed_clipboard_cut_is_atomic_and_locked_members_preserve_the_clipboard() {
    let mut sheet = board();
    let card = sheet
        .add_card(
            "Card",
            Vec2::new(-100.0, 0.0),
            Card::new(CardKind::Sticky, Vec2::new(120.0, 80.0)),
        )
        .unwrap();
    let object = shape(&mut sheet, 100.0, 0.0, "Shape");
    let other = shape(&mut sheet, 300.0, 0.0, "Other");
    sheet
        .group_board_members(&[BoardMember::Card(card), BoardMember::Object(object)])
        .unwrap();
    sheet
        .set_board_members_locked(&[BoardMember::Object(object)], true)
        .unwrap();
    let mut h = harness(sheet);
    h.state_mut().view.selection = vec![card];
    h.state_mut().view.board_selection = vec![other];
    let locked = h.state().doc.workbook().clone();
    {
        let state = h.state_mut();
        assert!(apply_action(&mut state.doc, &mut state.view, Action::Copy).unwrap());
        let payload = state.view.board_clipboard().cloned();
        state.view.clipboard_out = None;
        assert!(!Action::Cut.available(&state.doc, &state.view));
        assert!(!apply_action(&mut state.doc, &mut state.view, Action::Cut).unwrap());
        assert_eq!(state.doc.workbook(), &locked);
        assert_eq!(state.view.board_clipboard().cloned(), payload);
        assert!(state.view.clipboard_out.is_none());
        assert!(
            apply_board_action(&mut state.doc, &mut state.view, BoardAction::Lock(false)).unwrap()
        );
        let unlocked = state.doc.workbook().clone();
        assert!(Action::Cut.available(&state.doc, &state.view));
        assert!(apply_action(&mut state.doc, &mut state.view, Action::Cut).unwrap());
        assert!(!state.doc.sheet().contains(card));
        assert!(state.doc.sheet().canvas.object(object).is_none());
        assert!(state.doc.sheet().canvas.object(other).is_none());
        assert!(state.view.selection.is_empty());
        assert!(state.view.board_selection.is_empty());
        assert!(state.doc.undo());
        assert_eq!(state.doc.workbook(), &unlocked);
    }
}

#[test]
fn oversized_board_cut_is_rejected_before_removing_anything() {
    let mut sheet = board();
    let existing = shape(&mut sheet, -200.0, 0.0, "Previous");
    let mut cards = Vec::new();
    for (index, byte) in [0_u8, 1].into_iter().enumerate() {
        let id = sheet
            .add_card(
                &format!("Card {index}"),
                Vec2::new(index as f32 * 200.0, 0.0),
                Card::new(CardKind::Sticky, Vec2::new(120.0, 80.0)),
            )
            .unwrap();
        let resource = sheet
            .resources
            .insert(
                thinoda_core::resources::Resource::new(
                    &format!("attachment-{index}.bin"),
                    "application/octet-stream",
                    vec![byte; 6 * 1024 * 1024],
                )
                .unwrap(),
            )
            .unwrap();
        sheet.topic_mut(id).unwrap().attachment = Some(thinoda_core::attachments::Attachment {
            resource,
            name: format!("attachment-{index}.bin"),
        });
        cards.push(id);
    }
    let mut h = harness(sheet);
    {
        let state = h.state_mut();
        state.view.board_selection = vec![existing];
        assert!(apply_action(&mut state.doc, &mut state.view, Action::Copy).unwrap());
        let previous = state.view.board_clipboard().cloned();
        state.view.clipboard_out = None;
        state.view.selection = cards;
        state.view.board_selection.clear();
        let before = state.doc.workbook().clone();
        let revision = state.doc.state_id();
        assert!(apply_action(&mut state.doc, &mut state.view, Action::Cut).is_err());
        assert_eq!(state.doc.workbook(), &before);
        assert_eq!(state.doc.state_id(), revision);
        assert_eq!(state.view.board_clipboard().cloned(), previous);
        assert!(state.view.clipboard_out.is_none());
    }
}

#[test]
fn host_clipboard_hook_defers_canvas_events_without_mutating_the_document() {
    let mut sheet = board();
    let object = shape(&mut sheet, 0.0, 0.0, "Shape");
    let mut h = harness(sheet);
    h.state_mut().view.board_selection = vec![object];
    h.state_mut().view.host_handles_clipboard = true;
    let before = h.state().doc.workbook().clone();
    for event in [
        egui::Event::Copy,
        egui::Event::Cut,
        egui::Event::Paste("external text".into()),
    ] {
        h.event(event);
        h.run_steps(1);
    }
    assert_eq!(
        h.state().clipboard_requests,
        vec![
            Action::Copy,
            Action::Cut,
            Action::Paste(Some("external text".into()))
        ]
    );
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state().view.clipboard_out.is_none());
}

#[test]
fn host_clipboard_hook_does_not_intercept_a_focused_label_editor() {
    let mut sheet = board();
    let object = shape(&mut sheet, 0.0, 0.0, "Shape");
    let mut h = harness(sheet);
    {
        let state = h.state_mut();
        state.view.host_handles_clipboard = true;
        assert!(state.view.begin_board_label(&state.doc, object));
    }
    h.run_steps(2);
    h.event(egui::Event::Copy);
    h.run_steps(1);
    assert!(h.state().clipboard_requests.is_empty());
    assert!(h.state().view.editing_board_object().is_some());
}

#[test]
fn board_duplicate_does_not_replace_clipboard_and_can_cross_views() {
    let mut sheet = board();
    let id = shape(&mut sheet, 100.0, 0.0, "Shape");
    let mut h = harness(sheet);
    h.state_mut().view.board_selection = vec![id];
    let before = h.state().doc.workbook().clone();
    let payload = {
        let state = h.state_mut();
        assert!(apply_action(&mut state.doc, &mut state.view, Action::Copy).unwrap());
        let payload = state.view.board_clipboard().unwrap().clone();
        assert!(state.view.clipboard_out.take().is_some());
        assert!(apply_action(&mut state.doc, &mut state.view, Action::Duplicate).unwrap());
        assert_eq!(state.view.board_clipboard(), Some(&payload));
        assert!(state.view.clipboard_out.is_none());
        assert_ne!(state.view.board_selection[0], id);
        assert!(state.doc.undo());
        assert_eq!(state.doc.workbook(), &before);
        payload
    };
    let mut target = harness(board());
    let encoded = thinoda_core::clipboard::encode_board_payload(&payload).unwrap();
    target.state_mut().view.set_board_clipboard(Some(payload));
    target.state_mut().view.reset();
    {
        let state = target.state_mut();
        assert!(state.view.board_clipboard().is_some());
        state.view.invalidate_clipboard();
        assert!(
            state
                .view
                .paste_encoded_board(&mut state.doc, &encoded)
                .unwrap()
        );
        assert_eq!(state.view.board_selection.len(), 1);
        assert_eq!(state.doc.sheet().canvas.objects.len(), 1);
        assert_eq!(state.doc.sheet().canvas.objects[0].label(), "Shape");
        assert!(state.doc.undo());
        assert!(state.doc.sheet().canvas.objects.is_empty());
    }
}

#[test]
fn hidden_board_plain_text_paste_creates_floating_roots_and_is_undoable() {
    let mut sheet = board();
    let shape = shape(&mut sheet, 0.0, 0.0, "Shape");
    let mut h = harness(sheet);
    h.state_mut().view.board_selection = vec![shape];
    let before = h.state().doc.workbook().clone();
    {
        let state = h.state_mut();
        assert!(apply_action(&mut state.doc, &mut state.view, Action::Copy).unwrap());
        assert!(
            apply_action(
                &mut state.doc,
                &mut state.view,
                Action::Paste(Some("New root\n\tChild\nOther root".into()))
            )
            .unwrap()
        );
        let roots = &state.view.selection;
        assert_eq!(roots.len(), 2);
        assert!(roots.iter().all(|id| state.doc.sheet().is_floating(*id)));
        assert_eq!(state.doc.sheet().topic(roots[0]).unwrap().title, "New root");
        let child = state.doc.sheet().children(roots[0])[0];
        assert_eq!(state.doc.sheet().topic(child).unwrap().title, "Child");
        assert_eq!(
            state.doc.sheet().topic(roots[1]).unwrap().title,
            "Other root"
        );
        assert!(state.view.board_clipboard().is_none());
        assert!(
            state
                .doc
                .sheet()
                .children(state.doc.sheet().root)
                .is_empty()
        );
        assert_eq!(state.doc.sheet().canvas.objects.len(), 1);
        assert!(state.doc.undo());
        assert_eq!(state.doc.workbook(), &before);
    }
}

#[test]
fn board_clipboard_copy_event_rich_paste_and_duplicate_shortcut_work() {
    let mut sheet = board();
    let id = shape(&mut sheet, 0.0, 0.0, "Shape");
    let mut h = harness(sheet);
    let center = point(&h, Vec2::ZERO);
    click(&mut h, center, Modifiers::NONE);
    let before = h.state().doc.workbook().clone();
    h.event(egui::Event::Copy);
    h.run_steps(2);
    let encoded =
        thinoda_core::clipboard::encode_board_payload(h.state().view.board_clipboard().unwrap())
            .unwrap();
    {
        let state = h.state_mut();
        assert!(
            state
                .view
                .paste_encoded_board(&mut state.doc, &encoded)
                .unwrap()
        );
    }
    h.run_steps(2);
    assert_eq!(h.state().doc.sheet().canvas.objects.len(), 2);
    assert_eq!(h.state().view.board_selection.len(), 1);
    assert_ne!(h.state().view.board_selection[0], id);
    h.key_press_modifiers(Modifiers::COMMAND, Key::D);
    h.run_steps(2);
    assert_eq!(h.state().doc.sheet().canvas.objects.len(), 3);
    assert!(h.state_mut().doc.undo());
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn unverified_identical_text_never_reuses_a_stale_board_payload() {
    let mut sheet = board();
    let shape = shape(&mut sheet, 0.0, 0.0, "Identical text");
    let mut h = harness(sheet);
    {
        let state = h.state_mut();
        state.view.board_selection = vec![shape];
        assert!(apply_action(&mut state.doc, &mut state.view, Action::Copy).unwrap());
        let external_text = state.view.board_clipboard().unwrap().text.clone();
        assert!(
            apply_action(
                &mut state.doc,
                &mut state.view,
                Action::Paste(Some(external_text))
            )
            .unwrap()
        );
        assert_eq!(state.doc.sheet().canvas.objects.len(), 1);
        assert_eq!(state.view.selection.len(), 1);
        assert_eq!(
            state
                .doc
                .sheet()
                .topic(state.view.selection[0])
                .unwrap()
                .title,
            "Identical text"
        );
        assert!(state.view.board_clipboard().is_none());
        assert!(state.view.clipboard_out.is_none());
        assert!(!apply_action(&mut state.doc, &mut state.view, Action::Paste(None)).unwrap());
    }
}

#[test]
fn plain_text_with_a_board_prefix_is_pasted_as_external_text() {
    let mut h = harness(board());
    let text = "THINODA-BOARD-CLIPBOARD-1\n{not a board payload";
    {
        let state = h.state_mut();
        assert!(
            apply_action(
                &mut state.doc,
                &mut state.view,
                Action::Paste(Some(text.into()))
            )
            .unwrap()
        );
        assert_eq!(state.view.selection.len(), 2);
        assert_eq!(
            state
                .doc
                .sheet()
                .topic(state.view.selection[0])
                .unwrap()
                .title,
            "THINODA-BOARD-CLIPBOARD-1"
        );
        assert_eq!(
            state
                .doc
                .sheet()
                .topic(state.view.selection[1])
                .unwrap()
                .title,
            "{not a board payload"
        );
    }
}

#[test]
fn central_map_plain_text_paste_does_not_restore_stale_board_selection() {
    let mut sheet = Sheet::new("Map", "Root");
    let root = sheet.root;
    let object = shape(&mut sheet, 0.0, 0.0, "Identical text");
    let mut h = harness(sheet);
    {
        let state = h.state_mut();
        state.view.board_selection = vec![object];
        assert!(apply_action(&mut state.doc, &mut state.view, Action::Copy).unwrap());
        let text = state.view.board_clipboard().unwrap().text.clone();
        assert!(apply_action(&mut state.doc, &mut state.view, Action::Paste(Some(text))).unwrap());
        assert!(state.view.board_clipboard().is_none());
        assert!(state.view.board_selection.is_empty());
        assert_eq!(state.doc.sheet().canvas.objects.len(), 1);
        assert_eq!(state.doc.sheet().children(root), state.view.selection);
        assert_eq!(
            state
                .doc
                .sheet()
                .topic(state.view.selection[0])
                .unwrap()
                .title,
            "Identical text"
        );
    }
}

#[test]
fn identical_external_text_does_not_reuse_legacy_topic_content() {
    let mut sheet = Sheet::new("Map", "Root");
    let root = sheet.root;
    let original = sheet.add_child(root, None, "Same text");
    sheet.topic_mut(original).unwrap().style.bold = Some(true);
    let mut h = harness(sheet);
    {
        let state = h.state_mut();
        state.view.select_only(original);
        assert!(apply_action(&mut state.doc, &mut state.view, Action::Copy).unwrap());
        let external_text = state.view.clipboard.as_ref().unwrap().text.clone();
        state.view.select_only(root);
        assert!(
            apply_action(
                &mut state.doc,
                &mut state.view,
                Action::Paste(Some(external_text))
            )
            .unwrap()
        );
        assert!(state.view.clipboard.is_none());
        assert_eq!(state.view.selection.len(), 1);
        let pasted = state.doc.sheet().topic(state.view.selection[0]).unwrap();
        assert_eq!(pasted.title, "Same text");
        assert_eq!(pasted.style.bold, None);
    }
}

#[test]
fn invalid_encoded_board_payload_rejects_without_reusing_old_content() {
    let mut sheet = board();
    let shape = shape(&mut sheet, 0.0, 0.0, "Shape");
    let mut h = harness(sheet);
    {
        let state = h.state_mut();
        state.view.board_selection = vec![shape];
        assert!(apply_action(&mut state.doc, &mut state.view, Action::Copy).unwrap());
        let before = state.doc.workbook().clone();
        assert!(
            state
                .view
                .paste_encoded_board(&mut state.doc, "THINODA-BOARD-CLIPBOARD-1\n{")
                .is_err()
        );
        assert_eq!(state.doc.workbook(), &before);
        assert!(state.view.board_clipboard().is_none());
        assert!(state.view.clipboard.is_none());
        assert!(state.view.clipboard_out.is_none());
        assert!(!apply_action(&mut state.doc, &mut state.view, Action::Paste(None)).unwrap());
    }
}

#[test]
fn invalid_board_payload_does_not_commit_a_pending_title_draft() {
    let mut sheet = board();
    let shape = shape(&mut sheet, 0.0, 0.0, "Shape");
    let topic = sheet.add_floating("Original", Vec2::new(200.0, 0.0));
    let mut h = harness(sheet);
    let state = h.state_mut();
    state.view.board_selection = vec![shape];
    assert!(apply_action(&mut state.doc, &mut state.view, Action::Copy).unwrap());
    let mut payload = state.view.board_clipboard().unwrap().clone();
    payload.text.clear();
    state
        .view
        .begin_edit_with(state.doc.sheet(), topic, "Draft");
    let before = state.doc.workbook().clone();

    assert!(
        state
            .view
            .paste_board_payload(&mut state.doc, payload)
            .is_err()
    );
    assert_eq!(state.doc.workbook(), &before);
    assert_eq!(state.view.editing_text(), Some("Draft"));
    assert!(state.view.board_clipboard().is_none());
}

#[test]
fn external_clipboard_change_invalidates_internal_and_pending_content() {
    let mut sheet = board();
    let object = shape(&mut sheet, 0.0, 0.0, "Board item");
    let mut board_view = harness(sheet);
    {
        let state = board_view.state_mut();
        state.view.board_selection = vec![object];
        assert!(apply_action(&mut state.doc, &mut state.view, Action::Copy).unwrap());
        assert!(state.view.board_clipboard().is_some());
        assert!(state.view.clipboard_out.is_some());
        state.view.invalidate_clipboard();
        assert!(state.view.board_clipboard().is_none());
        assert!(state.view.clipboard.is_none());
        assert!(state.view.clipboard_out.is_none());
        assert!(!apply_action(&mut state.doc, &mut state.view, Action::Paste(None)).unwrap());
        assert_eq!(state.doc.sheet().canvas.objects.len(), 1);
    }

    let mut sheet = Sheet::new("Map", "Root");
    let root = sheet.root;
    let child = sheet.add_child(root, None, "Map topic");
    let mut map_view = harness(sheet);
    {
        let state = map_view.state_mut();
        state.view.select_only(child);
        assert!(apply_action(&mut state.doc, &mut state.view, Action::Copy).unwrap());
        assert!(state.view.clipboard.is_some());
        assert!(state.view.clipboard_out.is_some());
        state.view.invalidate_clipboard();
        assert!(state.view.clipboard.is_none());
        assert!(state.view.board_clipboard().is_none());
        assert!(state.view.clipboard_out.is_none());
        state.view.select_only(root);
        assert!(!apply_action(&mut state.doc, &mut state.view, Action::Paste(None)).unwrap());
        assert_eq!(state.doc.sheet().children(root), [child]);
    }
}

#[test]
fn central_map_copy_replaces_stale_board_clipboard_and_stays_legacy() {
    let mut sheet = Sheet::new("Map", "Root");
    let root = sheet.root;
    let child = sheet.add_child(root, None, "Map topic");
    let object = shape(&mut sheet, 250.0, 100.0, "Shape");
    let mut h = harness(sheet);
    {
        let state = h.state_mut();
        state.view.board_selection = vec![object];
        assert!(apply_action(&mut state.doc, &mut state.view, Action::Copy).unwrap());
        assert!(state.view.board_clipboard().is_some());
        state.view.select_only(child);
        assert!(apply_action(&mut state.doc, &mut state.view, Action::Copy).unwrap());
        assert!(state.view.board_clipboard().is_none());
        assert!(state.view.clipboard.is_some());
        let text = state.view.clipboard_out.take();
        state.view.select_only(root);
        assert!(apply_action(&mut state.doc, &mut state.view, Action::Paste(text)).unwrap());
        let children = state.doc.sheet().children(root);
        assert_eq!(children.len(), 2);
        assert_eq!(
            state.doc.sheet().topic(children[1]).unwrap().title,
            "Map topic"
        );
        assert_eq!(state.doc.sheet().canvas.objects.len(), 1);
    }
}

#[test]
fn localized_generated_names_and_hidden_tree_navigation() {
    let mut sheet = board();
    let root = sheet.add_floating("Tree", Vec2::new(-200.0, 0.0));
    let child = sheet.add_child(root, None, "Child");
    let object = shape(&mut sheet, 250.0, 0.0, "");
    let mut h = harness(sheet);
    h.state_mut()
        .view
        .board_labels
        .shape_names
        .insert(ShapeKind::Rectangle, "Localized rectangle".into());
    h.run_steps(2);
    assert!(h.query_by_label("Localized rectangle").is_some());
    h.get_by_label("Localized rectangle").focus();
    h.run_steps(2);
    assert_eq!(h.state().view.board_selection, vec![object]);
    let layout = {
        let state = h.state_mut();
        state.view.layout_for(&state.doc)
    };
    let child_center = layout.rect(child).unwrap().center();
    let root_center = layout.rect(root).unwrap().center();
    let direction = if root_center.x < child_center.x {
        Key::ArrowLeft
    } else {
        Key::ArrowRight
    };
    let child_point = point(&h, child_center);
    click(&mut h, child_point, Modifiers::NONE);
    assert_eq!(h.state().view.selection, vec![child]);
    h.key_press(direction);
    h.run_steps(2);
    assert_eq!(h.state().view.selection, vec![root]);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn keyboard_tab_order_reaches_frames_objects_and_their_controls() {
    let mut sheet = board();
    let frame = sheet
        .create_board_frame(
            "Area",
            thinoda_core::Rect::from_min_size(Vec2::new(-300.0, -250.0), Vec2::new(600.0, 500.0)),
        )
        .unwrap();
    let first = shape(&mut sheet, -100.0, -100.0, "First");
    let second = shape(&mut sheet, 100.0, 120.0, "Second");
    let mut h = harness(sheet);
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert_eq!(
        h.state().view.selected_object,
        Some(ObjectSelection::Zone(frame))
    );
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert_eq!(h.state().view.board_selection, vec![first]);
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert!(h.get_by_label("Resize board item").is_focused());
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert!(h.get_by_label("Rotate board item").is_focused());
    for label in [
        "Add connected shape above",
        "Add connected shape to the right",
        "Add connected shape below",
        "Add connected shape to the left",
    ] {
        h.key_press(Key::Tab);
        h.run_steps(2);
        assert!(h.get_by_label(label).is_focused(), "{label}");
    }
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert_eq!(h.state().view.board_selection, vec![second]);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn focusing_group_member_updates_later_accessible_labels_in_same_frame() {
    let mut sheet = board();
    let focused = shape(&mut sheet, -200.0, -180.0, "Group focus");
    let peer = shape(&mut sheet, 0.0, 120.0, "Group peer");
    let previous = shape(&mut sheet, 250.0, 200.0, "Previous");
    let card = sheet
        .add_card(
            "Group card",
            Vec2::new(-200.0, 40.0),
            Card::new(CardKind::Sticky, Vec2::new(120.0, 100.0)),
        )
        .unwrap();
    sheet
        .group_board_members(&[
            BoardMember::Object(focused),
            BoardMember::Object(peer),
            BoardMember::Card(card),
        ])
        .unwrap();
    let mut h = harness(sheet);
    h.state_mut().view.board_selection = vec![previous];
    h.run_steps(2);

    h.get_by_label("Rectangle: Group focus").focus();
    h.run_steps(1);

    assert_eq!(h.state().view.selection, vec![card]);
    assert!(h.state().view.board_selection.contains(&focused));
    assert!(h.state().view.board_selection.contains(&peer));
    assert!(!h.state().view.board_selection.contains(&previous));
    assert!(
        h.query_by_label("Sticky note: Group card, Selected")
            .is_some()
    );
    assert!(
        h.query_by_label("Rectangle: Group peer, Selected")
            .is_some()
    );
    assert!(h.query_by_label("Rectangle: Previous").is_some());
}

#[test]
fn keyboard_tab_can_leave_board_in_both_directions() {
    let mut sheet = board();
    shape(&mut sheet, -100.0, 0.0, "First");
    shape(&mut sheet, 100.0, 0.0, "Last");
    let mut doc = Document::new(Workbook::new(sheet));
    doc.mark_saved();
    let mut h = Harness::builder()
        .with_size(vec2(1200.0, 900.0))
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(
            |ui, state: &mut State| {
                let _ = ui.button("Before board");
                ui.allocate_ui(vec2(1000.0, 700.0), |ui| {
                    if let Some(error) = MapCanvas::show(ui, &mut state.doc, &mut state.view).error
                    {
                        state.errors.push(error);
                    }
                });
                let _ = ui.button("After board");
            },
            State {
                doc,
                view: MapView {
                    show_add_controls: false,
                    ..Default::default()
                },
                errors: Vec::new(),
                clipboard_requests: Vec::new(),
                open_links: Vec::new(),
            },
        );
    h.run_steps(3);

    h.get_by_label("Rectangle: First").focus();
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::SHIFT, Key::Tab);
    h.run_steps(3);
    assert!(h.get_by_label("Before board").is_focused());

    h.get_by_label("Rectangle: Last").focus();
    h.run_steps(2);
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert!(h.get_by_label("Resize board item").is_focused());
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert!(h.get_by_label("Rotate board item").is_focused());
    for _ in 0..4 {
        h.key_press(Key::Tab);
        h.run_steps(2);
    }
    assert!(
        h.get_by_label("Add connected shape to the left")
            .is_focused()
    );
    h.key_press(Key::Tab);
    h.run_steps(3);
    assert!(h.get_by_label("After board").is_focused());

    h.key_press_modifiers(Modifiers::SHIFT, Key::Tab);
    h.run_steps(3);
    assert!(h.get_by_label("Rectangle: Last, Selected").is_focused());
    h.key_press_modifiers(Modifiers::SHIFT, Key::Tab);
    h.run_steps(2);
    assert!(
        h.get_by_label("Add connected shape to the left")
            .is_focused()
    );
    assert_eq!(
        h.state().view.board_selection,
        vec![h.state().doc.sheet().canvas.objects[0].id]
    );

    {
        let state = h.state_mut();
        state.doc.replace(Workbook::new(board()));
        state.view.reset();
        state.view.set_board_tool(BoardTool::Sticky);
    }
    h.run_steps(3);
    h.get_by_label("Mind map canvas").focus();
    h.run_steps(2);
    h.key_press(Key::Tab);
    h.run_steps(3);
    assert!(h.get_by_label("After board").is_focused());
    h.get_by_label("Mind map canvas").focus();
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::SHIFT, Key::Tab);
    h.run_steps(3);
    assert!(h.get_by_label("Before board").is_focused());
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn keyboard_group_lock_ungroup_and_hand_pan() {
    let mut sheet = board();
    let first = shape(&mut sheet, -150.0, 0.0, "First");
    let second = shape(&mut sheet, 150.0, 0.0, "Second");
    let mut h = harness(sheet);
    let start = point(&h, Vec2::new(-150.0, 0.0));
    let end = point(&h, Vec2::new(150.0, 0.0));
    click(&mut h, start, Modifiers::NONE);
    click(&mut h, end, Modifiers::SHIFT);
    assert_eq!(h.state().view.board_selection, vec![first, second]);
    h.key_press_modifiers(Modifiers::COMMAND, Key::G);
    h.run_steps(2);
    assert!(
        h.state()
            .doc
            .sheet()
            .canvas
            .object(first)
            .unwrap()
            .group
            .is_some()
    );
    h.key_press_modifiers(Modifiers::COMMAND, Key::L);
    h.run_steps(2);
    assert!(h.state().doc.sheet().canvas.is_locked(first));
    assert!(h.state().doc.sheet().canvas.is_locked(second));
    h.key_press_modifiers(Modifiers::COMMAND, Key::L);
    h.run_steps(2);
    assert!(!h.state().doc.sheet().canvas.is_locked(first));
    h.key_press_modifiers(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::G);
    h.run_steps(2);
    assert!(
        h.state()
            .doc
            .sheet()
            .canvas
            .object(first)
            .unwrap()
            .group
            .is_none()
    );
    assert!(
        h.state()
            .doc
            .sheet()
            .canvas
            .object(second)
            .unwrap()
            .group
            .is_none()
    );

    set_tool(&mut h, BoardTool::Hand);
    let before = h.state().doc.workbook().clone();
    let offset = h.state().view.offset;
    let start = point(&h, Vec2::new(0.0, 200.0));
    let end = start + vec2(80.0, -30.0);
    drag(&mut h, start, end);
    finish_drag(&mut h, end);
    assert_eq!(h.state().view.offset, offset + vec2(80.0, -30.0));
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

fn connector_ends(sheet: &Sheet, id: CanvasObjectId) -> (Endpoint, Endpoint) {
    let CanvasObjectKind::Connector(connector) = &sheet.canvas.object(id).unwrap().kind else {
        panic!("not a connector");
    };
    (connector.from, connector.to)
}

fn connectors(sheet: &Sheet) -> Vec<CanvasObjectId> {
    sheet
        .canvas
        .objects
        .iter()
        .filter(|object| object.is_connector())
        .map(|object| object.id)
        .collect()
}

/// Selects the shape at `at` and waits out the double-click window.
fn select_shape_at(h: &mut Harness<'_, State>, at: Vec2) {
    let target = point(h, at);
    click(h, target, Modifiers::NONE);
    h.run_steps(30);
}

fn arrow_center(h: &Harness<'_, State>, label: &str) -> Pos2 {
    h.get_by_label(label).rect().center()
}

#[test]
fn direction_arrow_click_chains_a_connected_clone_in_one_undo() {
    let mut sheet = board();
    let source = shape(&mut sheet, 0.0, 0.0, "Source");
    let mut h = harness(sheet);
    select_shape_at(&mut h, Vec2::ZERO);
    assert_eq!(h.state().view.board_selection, vec![source]);
    let before = h.state().doc.workbook().clone();

    let arrow = arrow_center(&h, "Add connected shape to the right");
    click(&mut h, arrow, Modifiers::NONE);

    let sheet = h.state().doc.sheet().clone();
    assert_eq!(sheet.canvas.objects.len(), 3);
    let [clone] = h.state().view.board_selection[..] else {
        panic!("expected one selected clone");
    };
    assert_ne!(clone, source);
    let object = sheet.canvas.object(clone).unwrap();
    assert_eq!(object.rect.center(), Vec2::new(200.0, 0.0));
    let CanvasObjectKind::Shape { label, shape, .. } = &object.kind else {
        panic!("clone is not a shape");
    };
    assert!(label.is_empty());
    assert_eq!(*shape, ShapeKind::Rectangle);
    let edges = connectors(&sheet);
    assert_eq!(
        connector_ends(&sheet, edges[0]),
        (Endpoint::Object(source), Endpoint::Object(clone))
    );
    assert_eq!(h.state().doc.undo_label(), Some("Add connected shape"));
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn direction_arrow_click_connects_to_a_neighbour_without_cloning() {
    let mut sheet = board();
    let source = shape(&mut sheet, 0.0, 0.0, "Source");
    let neighbour = shape(&mut sheet, 200.0, 0.0, "Neighbour");
    let mut h = harness(sheet);
    select_shape_at(&mut h, Vec2::ZERO);

    let arrow = arrow_center(&h, "Add connected shape to the right");
    click(&mut h, arrow, Modifiers::NONE);

    let sheet = h.state().doc.sheet().clone();
    assert_eq!(sheet.canvas.objects.len(), 3);
    let edges = connectors(&sheet);
    assert_eq!(
        connector_ends(&sheet, edges[0]),
        (Endpoint::Object(source), Endpoint::Object(neighbour))
    );
    assert_eq!(h.state().view.board_selection, vec![neighbour]);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn direction_arrow_clone_skips_occupied_positions() {
    let mut sheet = board();
    shape(&mut sheet, 0.0, 0.0, "Source");
    // Blocks the first slot without being the neighbour hit at its centre.
    let mut blocker = CanvasObject::shape(
        ShapeKind::Rectangle,
        thinoda_core::Rect::from_center_size(Vec2::new(200.0, 60.0), Vec2::new(40.0, 40.0)),
    );
    blocker.locked = true;
    sheet.add_canvas_object(blocker).unwrap();
    let mut h = harness(sheet);
    select_shape_at(&mut h, Vec2::ZERO);

    let arrow = arrow_center(&h, "Add connected shape to the right");
    click(&mut h, arrow, Modifiers::NONE);

    let [clone] = h.state().view.board_selection[..] else {
        panic!("expected one selected clone");
    };
    let sheet = h.state().doc.sheet();
    assert_eq!(
        sheet.canvas.object(clone).unwrap().rect.center(),
        Vec2::new(400.0, 0.0)
    );
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn direction_arrow_modifiers_clone_only_or_connect_only() {
    let mut sheet = board();
    let source = shape(&mut sheet, 0.0, 0.0, "Source");
    let mut h = harness(sheet);
    select_shape_at(&mut h, Vec2::ZERO);
    let before = h.state().doc.workbook().clone();

    let arrow = arrow_center(&h, "Add connected shape below");
    click(&mut h, arrow, Modifiers::COMMAND);
    let sheet = h.state().doc.sheet().clone();
    assert_eq!(sheet.canvas.objects.len(), 2);
    assert!(connectors(&sheet).is_empty());
    let [clone] = h.state().view.board_selection[..] else {
        panic!("expected one selected clone");
    };
    assert_eq!(
        sheet.canvas.object(clone).unwrap().rect.center(),
        Vec2::new(0.0, 160.0)
    );
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);

    h.run_steps(30);
    select_shape_at(&mut h, Vec2::ZERO);
    let arrow = arrow_center(&h, "Add connected shape below");
    click(&mut h, arrow, Modifiers::SHIFT);
    let sheet = h.state().doc.sheet().clone();
    assert_eq!(sheet.canvas.objects.len(), 2);
    let edges = connectors(&sheet);
    assert_eq!(
        connector_ends(&sheet, edges[0]),
        (
            Endpoint::Object(source),
            Endpoint::Point(Vec2::new(0.0, 120.0))
        )
    );
    assert_eq!(h.state().view.board_selection, vec![edges[0]]);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn direction_arrow_drag_connects_shapes_free_points_and_ctrl_clones() {
    let mut sheet = board();
    let source = shape(&mut sheet, 0.0, 0.0, "Source");
    let target = shape(&mut sheet, 300.0, 250.0, "Target");
    let mut h = harness(sheet);
    select_shape_at(&mut h, Vec2::ZERO);
    let before = h.state().doc.workbook().clone();

    let arrow = arrow_center(&h, "Add connected shape to the right");
    let end = point(&h, Vec2::new(300.0, 250.0));
    drag(&mut h, arrow, end);
    finish_drag(&mut h, end);
    let sheet = h.state().doc.sheet().clone();
    let edges = connectors(&sheet);
    assert_eq!(edges.len(), 1);
    assert_eq!(
        connector_ends(&sheet, edges[0]),
        (Endpoint::Object(source), Endpoint::Object(target))
    );
    assert_eq!(h.state().view.board_selection, vec![edges[0]]);
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);

    h.run_steps(30);
    select_shape_at(&mut h, Vec2::ZERO);
    let arrow = arrow_center(&h, "Add connected shape to the right");
    let empty = point(&h, Vec2::new(320.0, -260.0));
    drag(&mut h, arrow, empty);
    finish_drag(&mut h, empty);
    let sheet = h.state().doc.sheet().clone();
    let edges = connectors(&sheet);
    assert_eq!(sheet.canvas.objects.len(), 3);
    let (from, to) = connector_ends(&sheet, edges[0]);
    assert_eq!(from, Endpoint::Object(source));
    assert!(matches!(to, Endpoint::Point(_)), "{to:?}");
    assert!(h.state_mut().doc.undo());

    h.run_steps(30);
    select_shape_at(&mut h, Vec2::ZERO);
    let arrow = arrow_center(&h, "Add connected shape to the right");
    h.event(egui::Event::ModifiersChanged(Modifiers::COMMAND));
    drag(&mut h, arrow, empty);
    h.event(egui::Event::PointerButton {
        pos: empty,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::COMMAND,
    });
    h.run_steps(1);
    h.event(egui::Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(2);
    let sheet = h.state().doc.sheet().clone();
    assert_eq!(sheet.canvas.objects.len(), 4);
    let [clone] = h.state().view.board_selection[..] else {
        panic!("expected one selected clone");
    };
    let center = sheet.canvas.object(clone).unwrap().rect.center();
    assert!(
        (center - Vec2::new(320.0, -260.0)).length() < 20.0,
        "{center:?}"
    );
    let edges = connectors(&sheet);
    assert_eq!(
        connector_ends(&sheet, edges[0]),
        (Endpoint::Object(source), Endpoint::Object(clone))
    );
    assert_eq!(h.state().doc.undo_label(), Some("Add connected shape"));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn dragging_from_a_shape_outline_draws_a_connector_and_the_body_still_moves() {
    let mut sheet = board();
    let source = shape(&mut sheet, 0.0, 0.0, "Source");
    let target = shape(&mut sheet, 300.0, 250.0, "Target");
    let mut h = harness(sheet);
    let before = h.state().doc.workbook().clone();

    // Just outside the right edge (x = 60), beyond the body's hit tolerance.
    let band = point(&h, Vec2::new(67.0, 0.0));
    h.hover_at(band);
    h.run_steps(2);
    assert_eq!(h.state().view.board_input.outline_hover, Some(source));
    let end = point(&h, Vec2::new(300.0, 250.0));
    drag(&mut h, band, end);
    assert_eq!(h.state().doc.workbook(), &before);
    finish_drag(&mut h, end);
    let sheet = h.state().doc.sheet().clone();
    let edges = connectors(&sheet);
    assert_eq!(edges.len(), 1);
    assert_eq!(
        connector_ends(&sheet, edges[0]),
        (Endpoint::Object(source), Endpoint::Object(target))
    );
    assert_eq!(h.state().view.board_selection, vec![edges[0]]);
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);

    h.run_steps(30);
    let empty = point(&h, Vec2::new(320.0, -260.0));
    drag(&mut h, band, empty);
    finish_drag(&mut h, empty);
    let sheet = h.state().doc.sheet().clone();
    let edges = connectors(&sheet);
    assert_eq!(sheet.canvas.objects.len(), 3);
    let (from, to) = connector_ends(&sheet, edges[0]);
    assert_eq!(from, Endpoint::Object(source));
    assert!(matches!(to, Endpoint::Point(_)), "{to:?}");
    assert!(h.state_mut().doc.undo());

    // Inside the body a drag still moves the shape.
    h.run_steps(30);
    let body = point(&h, Vec2::new(30.0, 0.0));
    drag(&mut h, body, body + vec2(50.0, 0.0));
    finish_drag(&mut h, body + vec2(50.0, 0.0));
    let sheet = h.state().doc.sheet();
    assert!(connectors(sheet).is_empty());
    assert_eq!(
        sheet.canvas.object(source).unwrap().rect.center(),
        Vec2::new(50.0, 0.0)
    );
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn outline_drag_onto_a_connector_attaches_edge_to_edge() {
    let mut sheet = board();
    let source = shape(&mut sheet, 0.0, 0.0, "Source");
    let left = shape(&mut sheet, -300.0, 300.0, "Left");
    let right = shape(&mut sheet, 300.0, 300.0, "Right");
    let edge = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(left),
            Endpoint::Object(right),
        )))
        .unwrap();
    let mut h = harness(sheet);

    // Below the bottom edge (y = 40), then onto the middle of the edge.
    let band = point(&h, Vec2::new(0.0, 47.0));
    let end = point(&h, Vec2::new(0.0, 300.0));
    drag(&mut h, band, end);
    finish_drag(&mut h, end);
    let sheet = h.state().doc.sheet().clone();
    let added: Vec<_> = connectors(&sheet)
        .into_iter()
        .filter(|id| *id != edge)
        .collect();
    let [added] = added[..] else {
        panic!("expected one new connector");
    };
    assert_eq!(
        connector_ends(&sheet, added),
        (Endpoint::Object(source), Endpoint::Object(edge))
    );
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn armed_shapes_treat_the_outline_band_as_content() {
    let mut sheet = board();
    let source = shape(&mut sheet, 0.0, 0.0, "Source");
    let mut h = harness(sheet);
    set_tool(&mut h, BoardTool::Shape(ShapeKind::Ellipse));
    let band = point(&h, Vec2::new(67.0, 0.0));
    h.hover_at(band);
    h.run_steps(2);
    assert!(h.state().view.board_input.armed_hover);
    assert_eq!(h.state().view.board_input.ghost, None);
    assert_eq!(h.state().view.board_input.outline_hover, Some(source));
    click(&mut h, band, Modifiers::NONE);
    h.run_steps(2);
    assert_eq!(h.state().doc.sheet().canvas.objects.len(), 1);

    let end = point(&h, Vec2::new(300.0, 0.0));
    h.run_steps(30);
    drag(&mut h, band, end);
    finish_drag(&mut h, end);
    let sheet = h.state().doc.sheet();
    let edges = connectors(sheet);
    assert_eq!(edges.len(), 1);
    assert_eq!(connector_ends(sheet, edges[0]).0, Endpoint::Object(source));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn direction_arrow_keyboard_and_touch_activation() {
    let mut sheet = board();
    let source = shape(&mut sheet, 0.0, 0.0, "Source");
    let mut h = harness(sheet);
    select_shape_at(&mut h, Vec2::ZERO);
    h.get_by_label("Add connected shape below").focus();
    h.run_steps(2);
    assert!(h.get_by_label("Add connected shape below").is_focused());
    h.key_press(Key::Enter);
    h.run_steps(2);
    let [clone] = h.state().view.board_selection[..] else {
        panic!("expected one selected clone");
    };
    let sheet = h.state().doc.sheet().clone();
    assert_eq!(
        sheet.canvas.object(clone).unwrap().rect.center(),
        Vec2::new(0.0, 160.0)
    );
    assert_eq!(
        connector_ends(&sheet, connectors(&sheet)[0]),
        (Endpoint::Object(source), Endpoint::Object(clone))
    );
    // Focus follows the new shape so Enter can keep chaining.
    let focused = h
        .query_all_by_label("Add connected shape below")
        .find(|node| node.is_focused())
        .expect("focused arrow");
    assert!(focused.rect().center().y > point(&h, Vec2::new(0.0, 200.0)).y);
    // Leave the source so only the selection keeps its arrows.
    h.hover_at(point(&h, Vec2::new(-500.0, -400.0)));
    h.run_steps(3);
    assert!(h.get_by_label("Add connected shape below").is_focused());
    h.key_press(Key::Enter);
    h.run_steps(2);
    let [second] = h.state().view.board_selection[..] else {
        panic!("expected one selected clone");
    };
    assert_eq!(
        h.state()
            .doc
            .sheet()
            .canvas
            .object(second)
            .unwrap()
            .rect
            .center(),
        Vec2::new(0.0, 320.0)
    );

    h.run_steps(30);
    let arrow = arrow_center(&h, "Add connected shape to the left");
    tap(&mut h, arrow);
    let sheet = h.state().doc.sheet().clone();
    assert_eq!(sheet.canvas.objects.len(), 7);
    let [left] = h.state().view.board_selection[..] else {
        panic!("expected one selected clone");
    };
    assert_eq!(
        sheet.canvas.object(left).unwrap().rect.center(),
        Vec2::new(-200.0, 320.0)
    );
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn hover_arrows_show_on_eligible_unselected_shapes() {
    let mut sheet = board();
    shape(&mut sheet, 0.0, 0.0, "Plain");
    let locked = shape(&mut sheet, 400.0, 0.0, "Locked");
    sheet.canvas.object_mut(locked).unwrap().locked = true;
    let first = shape(&mut sheet, 0.0, 300.0, "Grouped A");
    let second = shape(&mut sheet, 200.0, 300.0, "Grouped B");
    sheet.group_canvas_objects(&[first, second]).unwrap();
    let mut tiny = CanvasObject::shape(
        ShapeKind::Ellipse,
        thinoda_core::Rect::from_center_size(Vec2::new(-300.0, 0.0), Vec2::new(20.0, 20.0)),
    );
    tiny.alt_text = "Tiny".into();
    sheet.add_canvas_object(tiny).unwrap();
    let mut h = harness(sheet);
    let label = "Add connected shape to the right";

    for at in [
        Vec2::new(400.0, 0.0),
        Vec2::new(0.0, 300.0),
        Vec2::new(-300.0, 0.0),
    ] {
        let target = point(&h, at);
        h.hover_at(target);
        h.run_steps(3);
        assert!(h.query_by_label(label).is_none(), "{at:?}");
    }

    let plain = point(&h, Vec2::ZERO);
    h.hover_at(plain);
    h.run_steps(3);
    assert!(h.state().view.board_selection.is_empty());
    let arrow = arrow_center(&h, label);
    // Moving towards the arrow keeps it visible.
    h.hover_at(arrow);
    h.run_steps(3);
    assert!(h.query_by_label(label).is_some());
    click(&mut h, arrow, Modifiers::NONE);
    assert_eq!(h.state().doc.sheet().canvas.objects.len(), 7);
    assert_eq!(h.state().doc.undo_label(), Some("Add connected shape"));

    // The hovered source and the selected clone both show arrows until the pointer leaves.
    assert_eq!(h.query_all_by_label("Add connected shape above").count(), 2);
    h.hover_at(point(&h, Vec2::new(0.0, -400.0)));
    h.run_steps(3);
    assert_eq!(h.query_all_by_label("Add connected shape above").count(), 1);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

fn tile_open(h: &Harness<'_, State>) -> bool {
    h.query_by_label("Diamond").is_some()
}

#[test]
fn quick_picker_opens_on_arrow_hover_and_inserts_the_chosen_shape() {
    let mut sheet = board();
    let source = shape(&mut sheet, 0.0, 0.0, "Source");
    let mut h = harness(sheet);
    select_shape_at(&mut h, Vec2::ZERO);
    let before = h.state().doc.workbook().clone();

    let arrow = arrow_center(&h, "Add connected shape to the right");
    h.hover_at(arrow);
    h.run_steps(10);
    assert!(!tile_open(&h), "the picker waits for the pointer to rest");
    h.run_steps(40);
    assert!(tile_open(&h));
    // The source's own kind comes first.
    assert!(h.query_by_label("Rectangle").is_some());

    // Leaving both the arrow and the picker closes it without editing.
    h.hover_at(point(&h, Vec2::new(-500.0, -400.0)));
    h.run_steps(3);
    assert!(!tile_open(&h));
    assert_eq!(h.state().doc.workbook(), &before);

    h.hover_at(arrow);
    h.run_steps(50);
    assert!(tile_open(&h));
    let tile = h.get_by_label("Diamond").rect().center();
    h.hover_at(tile);
    h.run_steps(3);
    assert!(tile_open(&h), "moving into the picker keeps it open");
    assert_eq!(
        h.state().view.board_input.quick.as_ref().unwrap().preview,
        Some(ShapeKind::Diamond)
    );
    click(&mut h, tile, Modifiers::NONE);
    h.run_steps(2);
    assert!(!tile_open(&h));

    let sheet = h.state().doc.sheet().clone();
    assert_eq!(sheet.canvas.objects.len(), 3);
    let [clone] = h.state().view.board_selection[..] else {
        panic!("expected one selected clone");
    };
    let object = sheet.canvas.object(clone).unwrap();
    assert_eq!(object.rect.center(), Vec2::new(200.0, 0.0));
    let CanvasObjectKind::Shape { shape, label, .. } = &object.kind else {
        panic!("clone is not a shape");
    };
    assert_eq!(*shape, ShapeKind::Diamond);
    assert!(label.is_empty());
    assert_eq!(
        connector_ends(&sheet, connectors(&sheet)[0]),
        (Endpoint::Object(source), Endpoint::Object(clone))
    );
    assert_eq!(h.state().doc.undo_label(), Some("Add connected shape"));
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn quick_shape_picker_preserves_rounding_only_for_the_same_kind() {
    use thinoda_core::{DrawioRectangleStencil, ShapeRounding};

    let rounding = ShapeRounding {
        radius: 25.0,
        stencil: DrawioRectangleStencil::Mockup,
    };
    for (chosen, expected) in [("Rectangle", Some(rounding)), ("Diamond", None)] {
        let mut sheet = board();
        let source = shape(&mut sheet, 0.0, 0.0, "Source");
        sheet.canvas.object_mut(source).unwrap().rounding = Some(rounding);
        let mut h = harness(sheet);
        select_shape_at(&mut h, Vec2::ZERO);
        let before = h.state().doc.workbook().clone();
        let arrow = arrow_center(&h, "Add connected shape to the right");
        h.hover_at(arrow);
        h.run_steps(50);
        let tile = h.get_by_label(chosen).rect().center();
        h.hover_at(tile);
        h.run_steps(3);
        click(&mut h, tile, Modifiers::NONE);
        h.run_steps(2);
        let [clone] = h.state().view.board_selection[..] else {
            panic!("expected one selected shape");
        };
        assert_eq!(
            h.state().doc.sheet().canvas.object(clone).unwrap().rounding,
            expected,
            "{chosen}"
        );
        assert!(h.state_mut().doc.undo());
        assert_eq!(h.state().doc.workbook(), &before);
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn quick_shape_picker_preserves_rectangle_sides_only_for_the_same_kind() {
    use thinoda_core::RectangleSides;

    let sides = RectangleSides {
        top: false,
        right: true,
        bottom: true,
        left: false,
    };
    for (chosen, expected) in [("Rectangle", Some(sides)), ("Diamond", None)] {
        let mut sheet = board();
        let source = shape(&mut sheet, 0.0, 0.0, "Source");
        sheet.canvas.object_mut(source).unwrap().rectangle_sides = Some(sides);
        let mut h = harness(sheet);
        select_shape_at(&mut h, Vec2::ZERO);
        let before = h.state().doc.workbook().clone();
        let arrow = arrow_center(&h, "Add connected shape to the right");
        h.hover_at(arrow);
        h.run_steps(50);
        let tile = h.get_by_label(chosen).rect().center();
        h.hover_at(tile);
        h.run_steps(3);
        click(&mut h, tile, Modifiers::NONE);
        h.run_steps(2);
        let [clone] = h.state().view.board_selection[..] else {
            panic!("expected one selected shape");
        };
        assert_eq!(
            h.state()
                .doc
                .sheet()
                .canvas
                .object(clone)
                .unwrap()
                .rectangle_sides,
            expected,
            "{chosen}"
        );
        assert!(h.state_mut().doc.undo());
        assert_eq!(h.state().doc.workbook(), &before);
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn quick_shape_picker_preserves_double_ellipse_inset_only_for_the_same_kind() {
    for (chosen, expected) in [("Double ellipse", Some(8.5)), ("Diamond", None)] {
        let mut sheet = board();
        let mut object = CanvasObject::shape(
            ShapeKind::DoubleEllipse,
            thinoda_core::Rect::from_center_size(Vec2::ZERO, Vec2::new(120.0, 80.0)),
        );
        object.double_ellipse_margin = Some(8.5);
        sheet.add_canvas_object(object).unwrap();
        let mut h = harness(sheet);
        select_shape_at(&mut h, Vec2::ZERO);
        let before = h.state().doc.workbook().clone();
        let arrow = arrow_center(&h, "Add connected shape to the right");
        h.hover_at(arrow);
        h.run_steps(50);
        let tile = h.get_by_label(chosen).rect().center();
        h.hover_at(tile);
        h.run_steps(3);
        click(&mut h, tile, Modifiers::NONE);
        h.run_steps(2);
        let [clone] = h.state().view.board_selection[..] else {
            panic!("expected one selected shape");
        };
        assert_eq!(
            h.state()
                .doc
                .sheet()
                .canvas
                .object(clone)
                .unwrap()
                .double_ellipse_margin,
            expected,
            "{chosen}"
        );
        assert!(h.state_mut().doc.undo());
        assert_eq!(h.state().doc.workbook(), &before);
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn quick_picker_opens_on_press_and_hold_without_clicking_the_arrow() {
    let mut sheet = board();
    shape(&mut sheet, 0.0, 0.0, "Source");
    let mut h = harness(sheet);
    select_shape_at(&mut h, Vec2::ZERO);
    let before = h.state().doc.workbook().clone();

    let arrow = arrow_center(&h, "Add connected shape below");
    h.hover_at(arrow);
    h.event(egui::Event::PointerButton {
        pos: arrow,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(45);
    assert!(tile_open(&h));
    h.event(egui::Event::PointerButton {
        pos: arrow,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(3);
    assert!(tile_open(&h), "releasing the hold keeps the picker open");
    assert_eq!(h.state().doc.workbook(), &before);

    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(!tile_open(&h));
    assert_eq!(h.state().doc.workbook(), &before);
    assert_eq!(h.state().view.board_selection.len(), 1);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn quick_picker_keyboard_navigation_inserts_and_escape_refocuses_the_arrow() {
    let mut sheet = board();
    let source = shape(&mut sheet, 0.0, 0.0, "Source");
    let mut h = harness(sheet);
    select_shape_at(&mut h, Vec2::ZERO);
    // Keep the pointer away so only the keyboard drives the picker.
    h.hover_at(point(&h, Vec2::new(-500.0, -400.0)));
    h.run_steps(3);
    let before = h.state().doc.workbook().clone();
    let label = "Add connected shape below";

    h.get_by_label(label).focus();
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::SHIFT, Key::F10);
    h.run_steps(3);
    assert!(tile_open(&h));
    assert!(h.get_by_label("Rectangle").is_focused());
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(!tile_open(&h));
    assert!(h.get_by_label(label).is_focused());
    assert_eq!(h.state().doc.workbook(), &before);

    h.key_press_modifiers(Modifiers::SHIFT, Key::F10);
    h.run_steps(3);
    for _ in 0..2 {
        h.key_press(Key::ArrowRight);
        h.run_steps(2);
    }
    assert!(h.get_by_label("Ellipse").is_focused());
    h.key_press(Key::ArrowDown);
    h.run_steps(2);
    assert!(h.get_by_label("Cylinder").is_focused());
    h.key_press(Key::ArrowUp);
    h.run_steps(2);
    assert!(h.get_by_label("Ellipse").is_focused());
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert!(!tile_open(&h));

    let sheet = h.state().doc.sheet().clone();
    let [clone] = h.state().view.board_selection[..] else {
        panic!("expected one selected clone");
    };
    let object = sheet.canvas.object(clone).unwrap();
    assert_eq!(object.rect.center(), Vec2::new(0.0, 160.0));
    assert!(matches!(
        object.kind,
        CanvasObjectKind::Shape {
            shape: ShapeKind::Ellipse,
            ..
        }
    ));
    assert_eq!(
        connector_ends(&sheet, connectors(&sheet)[0]),
        (Endpoint::Object(source), Endpoint::Object(clone))
    );
    assert_eq!(h.state().doc.undo_label(), Some("Add connected shape"));
    // Focus follows the new shape's arrow, ready to chain.
    let focused = h
        .query_all_by_label(label)
        .find(|node| node.is_focused())
        .expect("focused arrow");
    assert!(focused.rect().center().y > point(&h, Vec2::new(0.0, 200.0)).y);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn quick_picker_outside_click_closes_without_acting_on_the_board() {
    let mut sheet = board();
    let source = shape(&mut sheet, 0.0, 0.0, "Source");
    let mut h = harness(sheet);
    select_shape_at(&mut h, Vec2::ZERO);
    let before = h.state().doc.workbook().clone();
    h.get_by_label("Add connected shape above").focus();
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::SHIFT, Key::F10);
    h.run_steps(3);
    assert!(tile_open(&h));

    let outside = point(&h, Vec2::new(-400.0, 300.0));
    click(&mut h, outside, Modifiers::NONE);
    h.run_steps(3);
    assert!(!tile_open(&h));
    assert_eq!(h.state().view.board_selection, vec![source]);
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn quick_picker_keeps_its_arrow_clickable_and_draggable() {
    let mut sheet = board();
    let source = shape(&mut sheet, 0.0, 0.0, "Source");
    let target = shape(&mut sheet, 300.0, 250.0, "Target");
    let mut h = harness(sheet);
    select_shape_at(&mut h, Vec2::ZERO);

    let arrow = arrow_center(&h, "Add connected shape to the right");
    h.hover_at(arrow);
    h.run_steps(45);
    assert!(tile_open(&h));
    let end = point(&h, Vec2::new(300.0, 250.0));
    drag(&mut h, arrow, end);
    finish_drag(&mut h, end);
    assert!(!tile_open(&h));
    let sheet = h.state().doc.sheet().clone();
    let edges = connectors(&sheet);
    assert_eq!(edges.len(), 1);
    assert_eq!(
        connector_ends(&sheet, edges[0]),
        (Endpoint::Object(source), Endpoint::Object(target))
    );
    assert!(h.state_mut().doc.undo());

    h.run_steps(30);
    select_shape_at(&mut h, Vec2::ZERO);
    let arrow = arrow_center(&h, "Add connected shape above");
    h.hover_at(arrow);
    h.run_steps(45);
    assert!(tile_open(&h));
    click(&mut h, arrow, Modifiers::NONE);
    h.run_steps(3);
    assert!(!tile_open(&h));
    let [clone] = h.state().view.board_selection[..] else {
        panic!("expected one selected clone");
    };
    let object = h.state().doc.sheet().canvas.object(clone).unwrap();
    assert_eq!(object.rect.center(), Vec2::new(0.0, -160.0));
    assert_eq!(h.state().doc.undo_label(), Some("Add connected shape"));
    // Still resting on the clicked arrow does not reopen the picker.
    h.run_steps(60);
    assert!(!tile_open(&h));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}
