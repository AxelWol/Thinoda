use egui::{Key, Modifiers, vec2};
use egui_kittest::Harness;
use thinoda_canvas::{BoardAction, MapCanvas, MapView, apply_board_action};
use thinoda_core::{
    CanvasConnector, CanvasObject, CanvasObjectId, CanvasObjectKind, Card, CardKind, Color,
    ConnectorRoute, Document, Endpoint, InkStroke, InkTool, Rect, SceneId, ShapeKind, Sheet,
    SheetRootMode, TopicId, Vec2, Workbook,
    board::{BoardMember, MAX_BOARD_COORDINATE, rotate_around},
    scene::{SceneGeometry, build_scene},
};

const TOLERANCE: f32 = 0.003;

fn close(actual: Vec2, expected: Vec2, tolerance: f32) {
    assert!(
        (actual.x - expected.x).abs() <= tolerance && (actual.y - expected.y).abs() <= tolerance,
        "expected {expected:?}, got {actual:?}"
    );
}

fn board() -> Sheet {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    sheet
}

fn shape(sheet: &mut Sheet, center: Vec2, size: Vec2, rotation: f32) -> CanvasObjectId {
    let mut object =
        CanvasObject::shape(ShapeKind::Rectangle, Rect::from_center_size(center, size));
    object.rotation = rotation;
    sheet.add_canvas_object(object).unwrap()
}

fn card(sheet: &mut Sheet, center: Vec2, size: Vec2, rotation: f32) -> TopicId {
    let mut card = Card::new(CardKind::Sticky, size);
    card.rotation = rotation;
    sheet.add_card("Card", center, card).unwrap()
}

fn connector(
    sheet: &mut Sheet,
    from: Endpoint,
    to: Endpoint,
    route: ConnectorRoute,
) -> CanvasObjectId {
    let mut connector = CanvasConnector::new(from, to);
    connector.route = route;
    sheet
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap()
}

fn connection(sheet: &Sheet, id: CanvasObjectId) -> &CanvasConnector {
    let CanvasObjectKind::Connector(connection) = &sheet.canvas.object(id).unwrap().kind else {
        panic!("expected connector");
    };
    connection
}

fn view_with_selection(topics: &[TopicId], objects: &[CanvasObjectId]) -> MapView {
    let mut view = MapView::default();
    view.selection.extend_from_slice(topics);
    view.board_selection.extend_from_slice(objects);
    view
}

fn bounds_center(doc: &Document, view: &mut MapView, ids: &[SceneId]) -> Vec2 {
    let layout = view.layout_for(doc);
    let scene = build_scene(doc.sheet(), &layout);
    ids.iter()
        .map(|id| scene.item(*id).expect("selected scene item").bounds)
        .reduce(|a, b| a.union(&b))
        .unwrap()
        .center()
}

fn assert_object_geometry_close(before: &Sheet, after: &Sheet, id: CanvasObjectId, tolerance: f32) {
    let old = before.canvas.object(id).unwrap();
    let new = after.canvas.object(id).unwrap();
    close(new.rect.min, old.rect.min, tolerance);
    assert_eq!(new.rect.size, old.rect.size);
    assert_eq!(new.rotation, old.rotation);
    if let (CanvasObjectKind::Ink(old), CanvasObjectKind::Ink(new)) = (&old.kind, &new.kind) {
        assert_eq!(new.pressure, old.pressure);
        assert_eq!(new.width, old.width);
        assert_eq!(new.points.len(), old.points.len());
        for (&old_point, &new_point) in old.points.iter().zip(new.points.iter()) {
            close(new_point, old_point, tolerance);
        }
    }
}

fn assert_connector_end_close(before: Endpoint, after: Endpoint, tolerance: f32) {
    match (before, after) {
        (Endpoint::Point(old), Endpoint::Point(new)) => close(new, old, tolerance),
        (old, new) => assert_eq!(new, old),
    }
}

#[test]
fn opposite_rotations_restore_asymmetric_mixed_group_and_each_is_undoable() {
    let mut sheet = board();
    let first = shape(
        &mut sheet,
        Vec2::new(-180.0, -75.0),
        Vec2::new(90.0, 35.0),
        25.0,
    );
    let second = shape(
        &mut sheet,
        Vec2::new(240.0, 65.0),
        Vec2::new(45.0, 140.0),
        348.0,
    );
    let topic = card(
        &mut sheet,
        Vec2::new(80.0, 180.0),
        Vec2::new(90.0, 55.0),
        30.0,
    );
    let mut stroke = InkStroke::new(
        InkTool::Pen,
        Color::BLACK,
        4.0,
        vec![
            Vec2::new(-20.0, 130.0),
            Vec2::new(5.0, 180.0),
            Vec2::new(45.0, 150.0),
        ],
    );
    stroke.pressure = Some(vec![0.25, 0.75, 0.5].into());
    let mut ink = CanvasObject::ink(stroke);
    ink.rotation = 40.0;
    let ink = sheet.add_canvas_object(ink).unwrap();
    sheet
        .group_board_members(&[
            BoardMember::Object(first),
            BoardMember::Object(ink),
            BoardMember::Card(topic),
        ])
        .unwrap();
    let before = sheet.clone();
    let mut doc = Document::new(Workbook::new(sheet));
    let mut view = view_with_selection(&[topic], &[first, second]);
    let pivot = bounds_center(
        &doc,
        &mut view,
        &[
            SceneId::Object(first),
            SceneId::Object(second),
            SceneId::Object(ink),
            SceneId::Topic(topic),
        ],
    );

    assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(17.0)).unwrap());
    let first_rotation = doc.sheet().clone();
    for id in [first, second, ink] {
        close(
            first_rotation.canvas.object(id).unwrap().rect.center(),
            rotate_around(before.canvas.object(id).unwrap().rect.center(), pivot, 17.0),
            TOLERANCE,
        );
    }
    close(
        first_rotation.topic(topic).unwrap().position.unwrap(),
        rotate_around(before.topic(topic).unwrap().position.unwrap(), pivot, 17.0),
        TOLERANCE,
    );

    // Reordering a selection does not change its membership or rotation pivot.
    view.board_selection.reverse();
    assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(-17.0)).unwrap());
    for id in [first, second, ink] {
        assert_object_geometry_close(&before, doc.sheet(), id, TOLERANCE);
    }
    close(
        doc.sheet().topic(topic).unwrap().position.unwrap(),
        before.topic(topic).unwrap().position.unwrap(),
        TOLERANCE,
    );
    assert_eq!(
        doc.sheet().topic(topic).unwrap().card,
        before.topic(topic).unwrap().card
    );
    doc.sheet().validate().unwrap();

    assert!(doc.undo());
    assert_eq!(doc.sheet(), &first_rotation);
    assert!(doc.undo());
    assert_eq!(doc.sheet(), &before);
    assert!(!doc.can_undo());
    assert!(doc.redo());
    assert_eq!(doc.sheet(), &first_rotation);
    assert!(doc.redo());
    for id in [first, second, ink] {
        assert_object_geometry_close(&before, doc.sheet(), id, TOLERANCE);
    }
}

#[test]
fn opposite_rotations_restore_connector_points_and_keep_anchors_attached() {
    let mut sheet = board();
    let object = shape(
        &mut sheet,
        Vec2::new(-130.0, -35.0),
        Vec2::new(60.0, 30.0),
        20.0,
    );
    let topic = card(
        &mut sheet,
        Vec2::new(65.0, 95.0),
        Vec2::new(80.0, 45.0),
        10.0,
    );
    let external = shape(
        &mut sheet,
        Vec2::new(475.0, -120.0),
        Vec2::new(50.0, 40.0),
        0.0,
    );
    let free = connector(
        &mut sheet,
        Endpoint::Point(Vec2::new(-60.0, 20.0)),
        Endpoint::Point(Vec2::new(135.0, 160.0)),
        ConnectorRoute::Curve,
    );
    let mixed = connector(
        &mut sheet,
        Endpoint::Object(external),
        Endpoint::Point(Vec2::new(15.0, -110.0)),
        ConnectorRoute::Elbow,
    );
    let internal = connector(
        &mut sheet,
        Endpoint::Object(object),
        Endpoint::Topic(topic),
        ConnectorRoute::Straight,
    );
    let dependent = connector(
        &mut sheet,
        Endpoint::Object(free),
        Endpoint::Topic(topic),
        ConnectorRoute::Straight,
    );
    sheet
        .group_board_members(&[
            BoardMember::Object(object),
            BoardMember::Card(topic),
            BoardMember::Object(free),
            BoardMember::Object(mixed),
            BoardMember::Object(internal),
            BoardMember::Object(dependent),
        ])
        .unwrap();
    let before = sheet.clone();
    let mut doc = Document::new(Workbook::new(sheet));
    let mut view = view_with_selection(&[], &[mixed]);
    let ids = [
        SceneId::Object(object),
        SceneId::Topic(topic),
        SceneId::Object(free),
        SceneId::Object(mixed),
        SceneId::Object(internal),
        SceneId::Object(dependent),
    ];
    let pivot = bounds_center(&doc, &mut view, &ids);
    let before_layout = view.layout_for(&doc);
    let before_scene = build_scene(&before, &before_layout);

    assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(13.0)).unwrap());
    assert_eq!(
        connection(doc.sheet(), mixed).from,
        Endpoint::Object(external)
    );
    for id in [free, mixed, internal, dependent] {
        let old = connection(&before, id);
        let new = connection(doc.sheet(), id);
        for (old, new) in [(old.from, new.from), (old.to, new.to)] {
            if let (Endpoint::Point(old), Endpoint::Point(new)) = (old, new) {
                close(new, rotate_around(old, pivot, 13.0), TOLERANCE);
            } else {
                assert_eq!(new, old);
            }
        }
    }

    assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(-13.0)).unwrap());
    assert_object_geometry_close(&before, doc.sheet(), object, TOLERANCE);
    close(
        doc.sheet().topic(topic).unwrap().position.unwrap(),
        before.topic(topic).unwrap().position.unwrap(),
        TOLERANCE,
    );
    assert_eq!(
        doc.sheet().canvas.object(external),
        before.canvas.object(external)
    );
    for id in [free, mixed, internal, dependent] {
        let old = connection(&before, id);
        let new = connection(doc.sheet(), id);
        assert_eq!(new.route, old.route);
        assert_connector_end_close(old.from, new.from, TOLERANCE);
        assert_connector_end_close(old.to, new.to, TOLERANCE);
    }
    let after_layout = view.layout_for(&doc);
    let after_scene = build_scene(doc.sheet(), &after_layout);
    for id in [free, mixed, internal, dependent] {
        let SceneGeometry::Polyline { points: old, .. } =
            &before_scene.item(SceneId::Object(id)).unwrap().geometry
        else {
            panic!("expected connector path");
        };
        let SceneGeometry::Polyline { points: new, .. } =
            &after_scene.item(SceneId::Object(id)).unwrap().geometry
        else {
            panic!("expected connector path");
        };
        assert_eq!(new.len(), old.len());
        for (&old, &new) in old.iter().zip(new) {
            close(new, old, TOLERANCE);
        }
    }
    doc.sheet().validate().unwrap();
}

#[test]
fn opposite_rotations_restore_a_connector_only_group_with_an_external_anchor() {
    let mut sheet = board();
    let external = shape(
        &mut sheet,
        Vec2::new(500.0, -150.0),
        Vec2::new(40.0, 40.0),
        0.0,
    );
    let first = connector(
        &mut sheet,
        Endpoint::Object(external),
        Endpoint::Point(Vec2::new(-160.0, -20.0)),
        ConnectorRoute::Elbow,
    );
    let second = connector(
        &mut sheet,
        Endpoint::Point(Vec2::new(30.0, 80.0)),
        Endpoint::Point(Vec2::new(150.0, 170.0)),
        ConnectorRoute::Straight,
    );
    sheet.group_canvas_objects(&[first, second]).unwrap();
    let before = sheet.clone();
    let mut doc = Document::new(Workbook::new(sheet));
    let mut view = view_with_selection(&[], &[first]);

    assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(15.0)).unwrap());
    assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(-15.0)).unwrap());
    for id in [first, second] {
        let old = connection(&before, id);
        let new = connection(doc.sheet(), id);
        assert_connector_end_close(old.from, new.from, TOLERANCE);
        assert_connector_end_close(old.to, new.to, TOLERANCE);
    }
    assert_eq!(
        doc.sheet().canvas.object(external),
        before.canvas.object(external)
    );
}

#[test]
fn pivot_is_recalculated_after_selection_or_document_changes() {
    let mut sheet = board();
    let first = shape(
        &mut sheet,
        Vec2::new(-200.0, -70.0),
        Vec2::new(80.0, 25.0),
        10.0,
    );
    let second = shape(
        &mut sheet,
        Vec2::new(50.0, 110.0),
        Vec2::new(30.0, 100.0),
        20.0,
    );
    let third = shape(
        &mut sheet,
        Vec2::new(300.0, -140.0),
        Vec2::new(45.0, 50.0),
        0.0,
    );
    let mut doc = Document::new(Workbook::new(sheet));
    let mut view = view_with_selection(&[], &[first, second]);
    assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(25.0)).unwrap());

    view.board_selection = vec![first, third];
    let pivot = bounds_center(
        &doc,
        &mut view,
        &[SceneId::Object(first), SceneId::Object(third)],
    );
    let old_center = doc.sheet().canvas.object(third).unwrap().rect.center();
    assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(20.0)).unwrap());
    close(
        doc.sheet().canvas.object(third).unwrap().rect.center(),
        rotate_around(old_center, pivot, 20.0),
        TOLERANCE,
    );

    doc.move_canvas_objects(&[third], Vec2::new(80.0, 35.0))
        .unwrap();
    let pivot = bounds_center(
        &doc,
        &mut view,
        &[SceneId::Object(first), SceneId::Object(third)],
    );
    let old_center = doc.sheet().canvas.object(third).unwrap().rect.center();
    assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(-20.0)).unwrap());
    close(
        doc.sheet().canvas.object(third).unwrap().rect.center(),
        rotate_around(old_center, pivot, -20.0),
        TOLERANCE,
    );
}

#[test]
fn locked_group_stays_fixed_and_does_not_affect_a_round_trip_pivot() {
    let mut sheet = board();
    let locked = shape(
        &mut sheet,
        Vec2::new(1000.0, -500.0),
        Vec2::new(40.0, 20.0),
        0.0,
    );
    let locked_card = card(
        &mut sheet,
        Vec2::new(1100.0, -500.0),
        Vec2::new(70.0, 40.0),
        0.0,
    );
    sheet
        .group_board_members(&[BoardMember::Object(locked), BoardMember::Card(locked_card)])
        .unwrap();
    sheet
        .set_board_members_locked(&[BoardMember::Card(locked_card)], true)
        .unwrap();
    let first = shape(
        &mut sheet,
        Vec2::new(-130.0, -40.0),
        Vec2::new(60.0, 25.0),
        0.0,
    );
    let second = shape(
        &mut sheet,
        Vec2::new(180.0, 90.0),
        Vec2::new(40.0, 100.0),
        20.0,
    );
    let before = sheet.clone();
    let mut doc = Document::new(Workbook::new(sheet));
    let mut view = view_with_selection(&[], &[locked, first, second]);
    let pivot = bounds_center(
        &doc,
        &mut view,
        &[SceneId::Object(first), SceneId::Object(second)],
    );

    assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(19.0)).unwrap());
    close(
        doc.sheet().canvas.object(first).unwrap().rect.center(),
        rotate_around(
            before.canvas.object(first).unwrap().rect.center(),
            pivot,
            19.0,
        ),
        TOLERANCE,
    );
    assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(-19.0)).unwrap());
    assert_eq!(
        doc.sheet().canvas.object(locked),
        before.canvas.object(locked)
    );
    assert_eq!(doc.sheet().topic(locked_card), before.topic(locked_card));
    for id in [first, second] {
        assert_object_geometry_close(&before, doc.sheet(), id, TOLERANCE);
    }
}

#[test]
fn rejected_rotation_keeps_geometry_and_the_last_successful_pivot() {
    let mut sheet = board();
    let x = MAX_BOARD_COORDINATE - 5_000.0;
    let first = shape(
        &mut sheet,
        Vec2::new(x, -10_000.0),
        Vec2::new(100.0, 20.0),
        0.0,
    );
    let second = shape(
        &mut sheet,
        Vec2::new(x, 10_000.0),
        Vec2::new(20.0, 100.0),
        0.0,
    );
    let before = sheet.clone();
    let mut doc = Document::new(Workbook::new(sheet));
    let mut view = view_with_selection(&[], &[first, second]);

    assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(10.0)).unwrap());
    let after_first = doc.sheet().clone();
    let state = doc.state_id();
    assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(90.0)).is_err());
    assert_eq!(doc.sheet(), &after_first);
    assert_eq!(doc.state_id(), state);
    doc.move_canvas_objects(&[second], Vec2::new(0.0, 1_000.0))
        .unwrap();
    let after_move = doc.sheet().clone();
    let moved_state = doc.state_id();
    assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(90.0)).is_err());
    assert_eq!(doc.sheet(), &after_move);
    assert_eq!(doc.state_id(), moved_state);
    assert!(doc.undo());
    assert_eq!(doc.sheet(), &after_first);
    assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(-10.0)).unwrap());
    for id in [first, second] {
        assert_object_geometry_close(&before, doc.sheet(), id, 0.25);
    }
    assert!(doc.undo());
    assert_eq!(doc.sheet(), &after_first);
    assert!(doc.undo());
    assert_eq!(doc.sheet(), &before);
}

#[test]
fn very_small_opposite_angles_remain_inverse_operations() {
    let mut sheet = board();
    let first = shape(
        &mut sheet,
        Vec2::new(-300_000.0, -180_000.0),
        Vec2::new(80.0, 40.0),
        0.0,
    );
    let second = shape(
        &mut sheet,
        Vec2::new(290_000.0, 170_000.0),
        Vec2::new(50.0, 90.0),
        0.0,
    );
    let before = sheet.clone();
    let mut doc = Document::new(Workbook::new(sheet));
    let mut view = view_with_selection(&[], &[first, second]);

    assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(0.000_01)).unwrap());
    assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(-0.000_01)).unwrap());
    for id in [first, second] {
        assert_object_geometry_close(&before, doc.sheet(), id, 0.125);
    }
}

#[test]
fn pivot_does_not_leak_between_documents_with_matching_revisions() {
    let mut first_sheet = board();
    let first = shape(
        &mut first_sheet,
        Vec2::new(-150.0, -80.0),
        Vec2::new(90.0, 30.0),
        0.0,
    );
    let second = shape(
        &mut first_sheet,
        Vec2::new(160.0, 100.0),
        Vec2::new(50.0, 75.0),
        0.0,
    );
    let mut first_doc = Document::new(Workbook::new(first_sheet));
    let mut view = view_with_selection(&[], &[first, second]);
    assert!(apply_board_action(&mut first_doc, &mut view, BoardAction::Rotate(25.0)).unwrap());

    let mut other_sheet = board();
    let other_first = shape(
        &mut other_sheet,
        Vec2::new(350.0, 270.0),
        Vec2::new(90.0, 30.0),
        0.0,
    );
    let other_second = shape(
        &mut other_sheet,
        Vec2::new(600.0, 350.0),
        Vec2::new(50.0, 75.0),
        0.0,
    );
    assert_eq!((first, second), (other_first, other_second));
    let mut other_doc = Document::new(Workbook::new(other_sheet));
    other_doc.rotate_canvas_object(other_second, 10.0).unwrap();
    assert_eq!(first_doc.state_id(), other_doc.state_id());
    let pivot = bounds_center(
        &other_doc,
        &mut view,
        &[SceneId::Object(other_first), SceneId::Object(other_second)],
    );
    let before = other_doc
        .sheet()
        .canvas
        .object(other_first)
        .unwrap()
        .rect
        .center();
    assert!(apply_board_action(&mut other_doc, &mut view, BoardAction::Rotate(-25.0)).unwrap());
    close(
        other_doc
            .sheet()
            .canvas
            .object(other_first)
            .unwrap()
            .rect
            .center(),
        rotate_around(before, pivot, -25.0),
        TOLERANCE,
    );

    view.reset();
    view.board_selection = vec![first, second];
    let pivot = bounds_center(
        &first_doc,
        &mut view,
        &[SceneId::Object(first), SceneId::Object(second)],
    );
    let before = first_doc
        .sheet()
        .canvas
        .object(first)
        .unwrap()
        .rect
        .center();
    assert!(apply_board_action(&mut first_doc, &mut view, BoardAction::Rotate(-25.0)).unwrap());
    close(
        first_doc
            .sheet()
            .canvas
            .object(first)
            .unwrap()
            .rect
            .center(),
        rotate_around(before, pivot, -25.0),
        TOLERANCE,
    );
}

#[test]
fn opposite_keyboard_rotations_restore_an_asymmetric_selection() {
    struct State {
        doc: Document,
        view: MapView,
        errors: Vec<thinoda_core::EditError>,
    }

    let mut sheet = board();
    let first = shape(
        &mut sheet,
        Vec2::new(-150.0, -90.0),
        Vec2::new(80.0, 30.0),
        0.0,
    );
    let second = shape(
        &mut sheet,
        Vec2::new(220.0, 110.0),
        Vec2::new(45.0, 100.0),
        15.0,
    );
    let before = sheet.clone();
    let mut harness = Harness::builder()
        .with_size(vec2(800.0, 600.0))
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(
            |ui, state: &mut State| {
                if let Some(error) = MapCanvas::show(ui, &mut state.doc, &mut state.view).error {
                    state.errors.push(error);
                }
            },
            State {
                doc: Document::new(Workbook::new(sheet)),
                view: view_with_selection(&[], &[first, second]),
                errors: Vec::new(),
            },
        );
    harness.run_steps(3);
    harness
        .ctx
        .memory_mut(|memory| memory.request_focus(MapCanvas::id()));
    harness.run_steps(1);
    let modifiers = Modifiers::COMMAND.plus(Modifiers::ALT);
    harness.key_press_modifiers(modifiers, Key::ArrowRight);
    harness.run_steps(2);
    harness.state_mut().view.board_selection.reverse();
    harness.run_steps(1);
    harness.key_press_modifiers(modifiers, Key::ArrowLeft);
    harness.run_steps(2);

    let state = harness.state();
    assert!(state.errors.is_empty());
    for id in [first, second] {
        assert_object_geometry_close(&before, state.doc.sheet(), id, TOLERANCE);
    }
    assert_eq!(state.doc.undo_label(), Some("Rotate board items"));
}

#[test]
fn selecting_away_and_back_starts_a_new_rotation_pivot() {
    struct State {
        doc: Document,
        view: MapView,
    }

    let mut sheet = board();
    let first = shape(
        &mut sheet,
        Vec2::new(-200.0, -70.0),
        Vec2::new(80.0, 25.0),
        10.0,
    );
    let second = shape(
        &mut sheet,
        Vec2::new(50.0, 110.0),
        Vec2::new(30.0, 100.0),
        20.0,
    );
    let third = shape(
        &mut sheet,
        Vec2::new(300.0, -140.0),
        Vec2::new(45.0, 50.0),
        0.0,
    );
    let mut harness = Harness::builder()
        .with_size(vec2(800.0, 600.0))
        .build_ui_state(
            |ui, state: &mut State| {
                MapCanvas::show(ui, &mut state.doc, &mut state.view);
            },
            State {
                doc: Document::new(Workbook::new(sheet)),
                view: view_with_selection(&[], &[first, second]),
            },
        );
    harness.run_steps(3);
    {
        let state = harness.state_mut();
        assert!(
            apply_board_action(&mut state.doc, &mut state.view, BoardAction::Rotate(25.0)).unwrap()
        );
    }

    harness.run_steps(1);
    harness.state_mut().view.board_selection = vec![third];
    harness.run_steps(1);
    harness.state_mut().view.board_selection = vec![first, second];
    harness.run_steps(1);

    let state = harness.state_mut();
    let pivot = bounds_center(
        &state.doc,
        &mut state.view,
        &[SceneId::Object(first), SceneId::Object(second)],
    );
    let before = state
        .doc
        .sheet()
        .canvas
        .object(first)
        .unwrap()
        .rect
        .center();
    assert!(
        apply_board_action(&mut state.doc, &mut state.view, BoardAction::Rotate(-25.0)).unwrap()
    );
    close(
        state
            .doc
            .sheet()
            .canvas
            .object(first)
            .unwrap()
            .rect
            .center(),
        rotate_around(before, pivot, -25.0),
        TOLERANCE,
    );
}

#[test]
fn selecting_another_member_of_the_same_group_keeps_the_pivot() {
    struct State {
        doc: Document,
        view: MapView,
    }

    let mut sheet = board();
    let first = shape(
        &mut sheet,
        Vec2::new(-200.0, -70.0),
        Vec2::new(80.0, 25.0),
        10.0,
    );
    let second = shape(
        &mut sheet,
        Vec2::new(50.0, 110.0),
        Vec2::new(30.0, 100.0),
        20.0,
    );
    let topic = card(
        &mut sheet,
        Vec2::new(250.0, -140.0),
        Vec2::new(45.0, 50.0),
        0.0,
    );
    sheet
        .group_board_members(&[
            BoardMember::Object(first),
            BoardMember::Object(second),
            BoardMember::Card(topic),
        ])
        .unwrap();
    let before = sheet.clone();
    let mut harness = Harness::builder()
        .with_size(vec2(800.0, 600.0))
        .build_ui_state(
            |ui, state: &mut State| {
                MapCanvas::show(ui, &mut state.doc, &mut state.view);
            },
            State {
                doc: Document::new(Workbook::new(sheet)),
                view: view_with_selection(&[], &[first]),
            },
        );
    harness.run_steps(3);
    {
        let state = harness.state_mut();
        assert!(
            apply_board_action(&mut state.doc, &mut state.view, BoardAction::Rotate(21.0)).unwrap()
        );
    }
    harness.run_steps(1);
    harness.state_mut().view.board_selection.clear();
    harness.state_mut().view.selection = vec![topic];
    harness.run_steps(1);

    let state = harness.state_mut();
    assert!(
        apply_board_action(&mut state.doc, &mut state.view, BoardAction::Rotate(-21.0)).unwrap()
    );
    for id in [first, second] {
        assert_object_geometry_close(&before, state.doc.sheet(), id, TOLERANCE);
    }
    close(
        state.doc.sheet().topic(topic).unwrap().position.unwrap(),
        before.topic(topic).unwrap().position.unwrap(),
        TOLERANCE,
    );
}

#[test]
fn repeated_small_rotations_and_full_turn_noop_share_the_pivot() {
    let mut sheet = board();
    let first = shape(
        &mut sheet,
        Vec2::new(-200.0, -70.0),
        Vec2::new(80.0, 25.0),
        10.0,
    );
    let second = shape(
        &mut sheet,
        Vec2::new(50.0, 110.0),
        Vec2::new(30.0, 100.0),
        20.0,
    );
    let third = shape(
        &mut sheet,
        Vec2::new(300.0, -140.0),
        Vec2::new(45.0, 50.0),
        0.0,
    );
    let before = sheet.clone();
    let mut doc = Document::new(Workbook::new(sheet));
    let mut view = view_with_selection(&[], &[first, second, third]);

    for _ in 0..10 {
        assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(1.0)).unwrap());
    }
    let state = doc.state_id();
    assert!(!apply_board_action(&mut doc, &mut view, BoardAction::Rotate(360.0)).unwrap());
    assert_eq!(doc.state_id(), state);
    for _ in 0..10 {
        assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(-1.0)).unwrap());
    }
    for id in [first, second, third] {
        assert_object_geometry_close(&before, doc.sheet(), id, TOLERANCE);
    }
    for _ in 0..20 {
        assert!(doc.undo());
    }
    assert_eq!(doc.sheet(), &before);
    assert!(!doc.can_undo());
}

#[test]
fn clipped_members_round_trip_even_when_their_clip_changes() {
    let mut sheet = board();
    let topic = card(
        &mut sheet,
        Vec2::new(-90.0, 0.0),
        Vec2::new(100.0, 40.0),
        0.0,
    );
    let object = shape(&mut sheet, Vec2::new(90.0, 0.0), Vec2::new(40.0, 20.0), 0.0);
    let frame = Rect::from_min_size(Vec2::new(-100.0, -50.0), Vec2::new(200.0, 100.0));
    let zone_id = sheet.create_zone("Frame", vec![topic], frame).unwrap();
    sheet
        .zones
        .iter_mut()
        .find(|zone| zone.id == zone_id)
        .unwrap()
        .clip = true;
    sheet
        .group_board_members(&[BoardMember::Card(topic), BoardMember::Object(object)])
        .unwrap();
    let before = sheet.clone();
    let mut doc = Document::new(Workbook::new(sheet));
    let mut view = view_with_selection(&[topic], &[]);

    assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(30.0)).unwrap());
    let layout = view.layout_for(&doc);
    let scene = build_scene(doc.sheet(), &layout);
    assert_eq!(scene.item(SceneId::Object(object)).unwrap().clip, None);
    assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(-30.0)).unwrap());
    assert_object_geometry_close(&before, doc.sheet(), object, TOLERANCE);
    close(
        doc.sheet().topic(topic).unwrap().position.unwrap(),
        before.topic(topic).unwrap().position.unwrap(),
        TOLERANCE,
    );
    assert_eq!(doc.sheet().zones, before.zones);
    let layout = view.layout_for(&doc);
    let scene = build_scene(doc.sheet(), &layout);
    assert_eq!(
        scene.item(SceneId::Object(object)).unwrap().clip,
        Some(frame)
    );
}

#[test]
fn undo_then_redo_preserves_the_pivot_for_the_restored_rotation_state() {
    struct State {
        doc: Document,
        view: MapView,
    }

    let mut sheet = board();
    let first = shape(
        &mut sheet,
        Vec2::new(-200.0, -70.0),
        Vec2::new(80.0, 25.0),
        10.0,
    );
    let second = shape(
        &mut sheet,
        Vec2::new(50.0, 110.0),
        Vec2::new(30.0, 100.0),
        20.0,
    );
    let before = sheet.clone();
    let mut harness = Harness::builder()
        .with_size(vec2(800.0, 600.0))
        .build_ui_state(
            |ui, state: &mut State| {
                MapCanvas::show(ui, &mut state.doc, &mut state.view);
            },
            State {
                doc: Document::new(Workbook::new(sheet)),
                view: view_with_selection(&[], &[first, second]),
            },
        );
    harness.run_steps(3);
    {
        let state = harness.state_mut();
        assert!(
            apply_board_action(&mut state.doc, &mut state.view, BoardAction::Rotate(25.0)).unwrap()
        );
    }
    harness.run_steps(1);
    assert!(harness.state_mut().doc.undo());
    harness.run_steps(1);
    assert!(harness.state_mut().doc.redo());
    harness.run_steps(1);
    let state = harness.state_mut();
    assert!(
        apply_board_action(&mut state.doc, &mut state.view, BoardAction::Rotate(-25.0)).unwrap()
    );
    for id in [first, second] {
        assert_object_geometry_close(&before, state.doc.sheet(), id, TOLERANCE);
    }
}

#[test]
fn undoing_one_of_several_rotations_keeps_the_earlier_pivot() {
    let mut sheet = board();
    let first = shape(
        &mut sheet,
        Vec2::new(-200.0, -70.0),
        Vec2::new(80.0, 25.0),
        10.0,
    );
    let second = shape(
        &mut sheet,
        Vec2::new(50.0, 110.0),
        Vec2::new(30.0, 100.0),
        20.0,
    );
    let before = sheet.clone();
    let mut doc = Document::new(Workbook::new(sheet));
    let mut view = view_with_selection(&[], &[first, second]);

    assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(25.0)).unwrap());
    assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(10.0)).unwrap());
    assert!(doc.undo());
    assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(-25.0)).unwrap());
    for id in [first, second] {
        assert_object_geometry_close(&before, doc.sheet(), id, TOLERANCE);
    }
}
