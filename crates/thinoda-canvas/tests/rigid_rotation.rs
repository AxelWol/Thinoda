use egui::{Key, Modifiers, vec2};
use egui_kittest::Harness;
use thinoda_canvas::{BoardAction, MapCanvas, MapView, apply_board_action};
use thinoda_core::{
    CanvasConnector, CanvasObject, CanvasObjectId, CanvasObjectKind, Card, CardKind, Color,
    ConnectorRoute, Document, Endpoint, InkStroke, InkTool, Rect, Scene, SceneId, ShapeKind, Sheet,
    SheetRootMode, TopicId, Vec2, Workbook,
    board::{BoardMember, MAX_BOARD_COORDINATE, rotate_around, rotated_corners},
    scene::{SceneGeometry, build_scene},
};

const TOLERANCE: f32 = 0.002;

fn close(actual: Vec2, expected: Vec2) {
    assert!(
        (actual.x - expected.x).abs() < TOLERANCE && (actual.y - expected.y).abs() < TOLERANCE,
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
    let mut presentation = Card::new(CardKind::Sticky, size);
    presentation.rotation = rotation;
    sheet.add_card("Card", center, presentation).unwrap()
}

fn connector(sheet: &mut Sheet, from: Endpoint, to: Endpoint) -> CanvasObjectId {
    sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(from, to)))
        .unwrap()
}

fn connection(sheet: &Sheet, id: CanvasObjectId) -> &CanvasConnector {
    let CanvasObjectKind::Connector(connection) = &sheet.canvas.object(id).unwrap().kind else {
        panic!("expected a connector");
    };
    connection
}

fn view_with_selection(topics: &[TopicId], objects: &[CanvasObjectId]) -> MapView {
    let mut view = MapView::default();
    view.selection.extend_from_slice(topics);
    view.board_selection.extend_from_slice(objects);
    view
}

fn scene(doc: &Document, view: &mut MapView) -> Scene {
    let layout = view.layout_for(doc);
    build_scene(doc.sheet(), &layout)
}

fn pivot(scene: &Scene, members: &[SceneId]) -> Vec2 {
    members
        .iter()
        .map(|id| {
            scene
                .item(*id)
                .expect("member must have a scene item")
                .bounds
        })
        .reduce(|a, b| a.union(&b))
        .unwrap()
        .center()
}

fn path(scene: &Scene, id: CanvasObjectId) -> Vec<Vec2> {
    let SceneGeometry::Polyline { points, .. } = &scene.item(SceneId::Object(id)).unwrap().geometry
    else {
        panic!("expected a connector path");
    };
    points.clone()
}

fn assert_path_rotated(before: &[Vec2], after: &[Vec2], pivot: Vec2, angle: f32) {
    assert_eq!(before.len(), after.len());
    for (before, after) in before.iter().zip(after) {
        close(*after, rotate_around(*before, pivot, angle));
    }
}

#[test]
fn mixed_group_uses_one_pivot_and_one_undo_even_when_selected_twice() {
    let mut sheet = board();
    let object = shape(
        &mut sheet,
        Vec2::new(-100.0, 0.0),
        Vec2::new(40.0, 20.0),
        30.0,
    );
    let topic = card(
        &mut sheet,
        Vec2::new(100.0, 0.0),
        Vec2::new(40.0, 20.0),
        30.0,
    );
    sheet
        .group_board_members(&[BoardMember::Object(object), BoardMember::Card(topic)])
        .unwrap();
    let before = sheet.clone();
    let mut doc = Document::new(Workbook::new(sheet));
    let mut view = view_with_selection(&[topic], &[object, object]);
    let center = pivot(
        &scene(&doc, &mut view),
        &[SceneId::Object(object), SceneId::Topic(topic)],
    );
    close(center, Vec2::ZERO);

    assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(90.0)).unwrap());
    let rotated = doc.sheet();
    let old = before.canvas.object(object).unwrap();
    let new = rotated.canvas.object(object).unwrap();
    close(new.rect.center(), Vec2::new(0.0, -100.0));
    assert_eq!(new.rect.size, old.rect.size);
    assert_eq!(new.rotation, 120.0);
    for (old, new) in rotated_corners(old.rect, old.rotation)
        .into_iter()
        .zip(rotated_corners(new.rect, new.rotation))
    {
        close(new, rotate_around(old, center, 90.0));
    }
    close(
        rotated.topic(topic).unwrap().position.unwrap(),
        Vec2::new(0.0, 100.0),
    );
    assert_eq!(rotated.topic(topic).unwrap().card.unwrap().rotation, 120.0);
    rotated.validate().unwrap();
    let after = rotated.clone();
    assert_eq!(doc.undo_label(), Some("Rotate board items"));
    assert!(doc.undo());
    assert_eq!(doc.sheet(), &before);
    assert!(!doc.can_undo());
    assert!(doc.redo());
    assert_eq!(doc.sheet(), &after);
}

#[test]
fn ungrouped_multi_selection_rotates_together_but_one_item_keeps_its_center() {
    let mut sheet = board();
    let a = shape(
        &mut sheet,
        Vec2::new(-70.0, 40.0),
        Vec2::new(60.0, 30.0),
        15.0,
    );
    let b = shape(
        &mut sheet,
        Vec2::new(130.0, -20.0),
        Vec2::new(40.0, 70.0),
        45.0,
    );
    let before = sheet.clone();
    let mut doc = Document::new(Workbook::new(sheet));
    let mut view = view_with_selection(&[], &[a, b]);
    let center = pivot(
        &scene(&doc, &mut view),
        &[SceneId::Object(a), SceneId::Object(b)],
    );
    assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(-90.0)).unwrap());
    for id in [a, b] {
        let old = before.canvas.object(id).unwrap();
        let new = doc.sheet().canvas.object(id).unwrap();
        close(
            new.rect.center(),
            rotate_around(old.rect.center(), center, -90.0),
        );
        for (old, new) in rotated_corners(old.rect, old.rotation)
            .into_iter()
            .zip(rotated_corners(new.rect, new.rotation))
        {
            close(new, rotate_around(old, center, -90.0));
        }
    }
    assert_eq!(doc.sheet().canvas.object(a).unwrap().rotation, 285.0);
    assert_eq!(doc.sheet().canvas.object(b).unwrap().rotation, 315.0);
    assert!(doc.undo());

    view.board_selection = vec![a];
    assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(90.0)).unwrap());
    let old = before.canvas.object(a).unwrap();
    let new = doc.sheet().canvas.object(a).unwrap();
    assert_eq!(new.rect, old.rect);
    assert_eq!(new.rotation, 105.0);
    assert_eq!(doc.sheet().canvas.object(b), before.canvas.object(b));
    let unchanged = doc.sheet().clone();
    let state = doc.state_id();
    // Keep the existing single-item return value for a full turn.
    assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(360.0)).unwrap());
    assert_eq!(doc.sheet(), &unchanged);
    assert_eq!(doc.state_id(), state);
}

#[test]
fn two_mixed_groups_share_one_selection_pivot_at_an_arbitrary_angle() {
    let mut sheet = board();
    let a = shape(
        &mut sheet,
        Vec2::new(-200.0, -50.0),
        Vec2::new(60.0, 20.0),
        0.0,
    );
    let a_card = card(
        &mut sheet,
        Vec2::new(-100.0, -50.0),
        Vec2::new(40.0, 30.0),
        0.0,
    );
    let b = shape(
        &mut sheet,
        Vec2::new(100.0, 50.0),
        Vec2::new(30.0, 60.0),
        0.0,
    );
    let b_card = card(
        &mut sheet,
        Vec2::new(200.0, 50.0),
        Vec2::new(40.0, 30.0),
        0.0,
    );
    sheet
        .group_board_members(&[BoardMember::Object(a), BoardMember::Card(a_card)])
        .unwrap();
    sheet
        .group_board_members(&[BoardMember::Object(b), BoardMember::Card(b_card)])
        .unwrap();
    let before = sheet.clone();
    let mut doc = Document::new(Workbook::new(sheet));
    let mut view = view_with_selection(&[a_card], &[b]);
    let center = pivot(
        &scene(&doc, &mut view),
        &[
            SceneId::Object(a),
            SceneId::Topic(a_card),
            SceneId::Object(b),
            SceneId::Topic(b_card),
        ],
    );
    assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(37.0)).unwrap());
    for id in [a, b] {
        let old = before.canvas.object(id).unwrap();
        let new = doc.sheet().canvas.object(id).unwrap();
        close(
            new.rect.center(),
            rotate_around(old.rect.center(), center, 37.0),
        );
        assert_eq!(new.rotation, 37.0);
    }
    for id in [a_card, b_card] {
        close(
            doc.sheet().topic(id).unwrap().position.unwrap(),
            rotate_around(before.topic(id).unwrap().position.unwrap(), center, 37.0),
        );
        assert_eq!(doc.sheet().topic(id).unwrap().card.unwrap().rotation, 37.0);
    }
    assert!(doc.undo());
    assert_eq!(doc.sheet(), &before);
    assert!(!doc.can_undo());
}

#[test]
fn grouped_connectors_rotate_free_ends_but_preserve_internal_and_external_anchors() {
    let mut sheet = board();
    let object = shape(
        &mut sheet,
        Vec2::new(-100.0, 0.0),
        Vec2::new(40.0, 20.0),
        0.0,
    );
    let topic = card(
        &mut sheet,
        Vec2::new(100.0, 0.0),
        Vec2::new(40.0, 20.0),
        0.0,
    );
    let external = shape(
        &mut sheet,
        Vec2::new(400.0, 0.0),
        Vec2::new(40.0, 20.0),
        0.0,
    );
    let free = connector(
        &mut sheet,
        Endpoint::Point(Vec2::new(-50.0, -80.0)),
        Endpoint::Point(Vec2::new(70.0, -60.0)),
    );
    let mixed = connector(
        &mut sheet,
        Endpoint::Object(object),
        Endpoint::Point(Vec2::new(0.0, 100.0)),
    );
    let internal = connector(&mut sheet, Endpoint::Topic(topic), Endpoint::Object(object));
    let dependent = connector(&mut sheet, Endpoint::Object(free), Endpoint::Topic(topic));
    let outside = connector(
        &mut sheet,
        Endpoint::Object(external),
        Endpoint::Point(Vec2::new(150.0, 80.0)),
    );
    let members = [
        BoardMember::Object(object),
        BoardMember::Card(topic),
        BoardMember::Object(free),
        BoardMember::Object(mixed),
        BoardMember::Object(internal),
        BoardMember::Object(dependent),
        BoardMember::Object(outside),
    ];
    sheet.group_board_members(&members).unwrap();
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
        SceneId::Object(outside),
    ];
    let before_scene = scene(&doc, &mut view);
    let center = pivot(&before_scene, &ids);

    assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(90.0)).unwrap());
    let after = doc.sheet();
    assert_eq!(
        after.canvas.object(external),
        before.canvas.object(external)
    );
    for id in [free, mixed, internal, dependent, outside] {
        let old = connection(&before, id);
        let new = connection(after, id);
        for (old, new) in [(old.from, new.from), (old.to, new.to)] {
            match (old, new) {
                (Endpoint::Point(old), Endpoint::Point(new)) => {
                    close(new, rotate_around(old, center, 90.0));
                }
                (old, new) => assert_eq!(new, old),
            }
        }
    }
    assert_eq!(connection(after, outside).from, Endpoint::Object(external));
    after.validate().unwrap();
    let after_scene = scene(&doc, &mut view);
    for id in [free, mixed, internal, dependent] {
        assert_path_rotated(
            &path(&before_scene, id),
            &path(&after_scene, id),
            center,
            90.0,
        );
    }
    assert!(doc.undo());
    assert_eq!(doc.sheet(), &before);
}

#[test]
fn connector_only_groups_rotate_but_an_individual_connector_does_not() {
    let mut sheet = board();
    let first = connector(
        &mut sheet,
        Endpoint::Point(Vec2::new(-100.0, -40.0)),
        Endpoint::Point(Vec2::new(-50.0, -40.0)),
    );
    let second = connector(
        &mut sheet,
        Endpoint::Point(Vec2::new(50.0, 40.0)),
        Endpoint::Point(Vec2::new(100.0, 40.0)),
    );
    let alone = connector(
        &mut sheet,
        Endpoint::Point(Vec2::new(200.0, 0.0)),
        Endpoint::Point(Vec2::new(250.0, 0.0)),
    );
    sheet.group_canvas_objects(&[first, second]).unwrap();
    let before = sheet.clone();
    let mut doc = Document::new(Workbook::new(sheet));
    let mut view = view_with_selection(&[], &[first]);
    let center = pivot(
        &scene(&doc, &mut view),
        &[SceneId::Object(first), SceneId::Object(second)],
    );
    assert!(BoardAction::Rotate(90.0).available(&doc, &view));
    assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(90.0)).unwrap());
    for id in [first, second] {
        let old = connection(&before, id);
        let new = connection(doc.sheet(), id);
        for (old, new) in [(old.from, new.from), (old.to, new.to)] {
            let (Endpoint::Point(old), Endpoint::Point(new)) = (old, new) else {
                panic!("expected free ends");
            };
            close(new, rotate_around(old, center, 90.0));
        }
    }
    assert_eq!(
        doc.sheet().canvas.object(alone),
        before.canvas.object(alone)
    );
    assert!(doc.undo());
    view.board_selection = vec![alone];
    assert!(!BoardAction::Rotate(90.0).available(&doc, &view));
    assert!(!apply_board_action(&mut doc, &mut view, BoardAction::Rotate(90.0)).unwrap());
}

#[test]
fn curved_and_elbow_connectors_keep_their_route_and_reroute_after_rotation() {
    for route in [ConnectorRoute::Elbow, ConnectorRoute::Curve] {
        let mut sheet = board();
        let object = shape(
            &mut sheet,
            Vec2::new(-200.0, -40.0),
            Vec2::new(40.0, 20.0),
            0.0,
        );
        let mut line = CanvasConnector::new(
            Endpoint::Point(Vec2::new(-100.0, -20.0)),
            Endpoint::Point(Vec2::new(100.0, 70.0)),
        );
        line.route = route;
        let line = sheet
            .add_canvas_object(CanvasObject::connector(line))
            .unwrap();
        sheet.group_canvas_objects(&[object, line]).unwrap();
        let mut doc = Document::new(Workbook::new(sheet));
        let mut view = view_with_selection(&[], &[line]);
        let before_scene = scene(&doc, &mut view);
        let center = pivot(
            &before_scene,
            &[SceneId::Object(object), SceneId::Object(line)],
        );
        let before_path = path(&before_scene, line);

        assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(90.0)).unwrap());
        assert_eq!(connection(doc.sheet(), line).route, route);
        let after_path = path(&scene(&doc, &mut view), line);
        close(after_path[0], rotate_around(before_path[0], center, 90.0));
        close(
            *after_path.last().unwrap(),
            rotate_around(*before_path.last().unwrap(), center, 90.0),
        );
        let interior = if route == ConnectorRoute::Elbow { 1 } else { 4 };
        assert!(
            (after_path[interior] - rotate_around(before_path[interior], center, 90.0)).length()
                > 1.0,
            "{route:?} routing is recomputed in sheet axes"
        );
    }
}

#[test]
fn grouped_ink_keeps_pressure_and_its_rendered_points_rotate_rigidly() {
    let mut sheet = board();
    let mut stroke = InkStroke::new(
        InkTool::Pen,
        Color::BLACK,
        4.0,
        vec![Vec2::new(70.0, 20.0), Vec2::new(100.0, 40.0)],
    );
    stroke.pressure = Some(vec![0.25, 0.75].into());
    let mut ink = CanvasObject::ink(stroke);
    ink.rotation = 25.0;
    let ink = sheet.add_canvas_object(ink).unwrap();
    let object = shape(
        &mut sheet,
        Vec2::new(-70.0, -20.0),
        Vec2::new(40.0, 30.0),
        10.0,
    );
    sheet.group_canvas_objects(&[ink, object]).unwrap();
    let before = sheet.clone();
    let mut doc = Document::new(Workbook::new(sheet));
    let mut view = view_with_selection(&[], &[ink]);
    let center = pivot(
        &scene(&doc, &mut view),
        &[SceneId::Object(ink), SceneId::Object(object)],
    );
    assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(90.0)).unwrap());
    let old = before.canvas.object(ink).unwrap();
    let new = doc.sheet().canvas.object(ink).unwrap();
    let (CanvasObjectKind::Ink(old_stroke), CanvasObjectKind::Ink(new_stroke)) =
        (&old.kind, &new.kind)
    else {
        panic!("expected ink");
    };
    assert_eq!(new_stroke.pressure, old_stroke.pressure);
    assert_eq!(new_stroke.width, old_stroke.width);
    for (old_point, new_point) in old_stroke.points.iter().zip(new_stroke.points.iter()) {
        let old_world = rotate_around(*old_point, old.rect.center(), old.rotation);
        let new_world = rotate_around(*new_point, new.rect.center(), new.rotation);
        close(new_world, rotate_around(old_world, center, 90.0));
    }
    doc.sheet().validate().unwrap();
}

#[test]
fn locked_group_is_excluded_from_the_pivot_and_never_partially_rotates() {
    for lock_group in [false, true] {
        let mut sheet = board();
        let grouped = shape(&mut sheet, Vec2::ZERO, Vec2::new(40.0, 20.0), 0.0);
        let topic = card(
            &mut sheet,
            Vec2::new(100.0, 0.0),
            Vec2::new(40.0, 20.0),
            0.0,
        );
        let group = sheet
            .group_board_members(&[BoardMember::Object(grouped), BoardMember::Card(topic)])
            .unwrap();
        if lock_group {
            sheet.set_canvas_group_locked(group, true).unwrap();
        } else {
            sheet
                .set_board_members_locked(&[BoardMember::Card(topic)], true)
                .unwrap();
        }
        let a = shape(
            &mut sheet,
            Vec2::new(400.0, -100.0),
            Vec2::new(40.0, 20.0),
            0.0,
        );
        let b = shape(
            &mut sheet,
            Vec2::new(400.0, 100.0),
            Vec2::new(40.0, 20.0),
            0.0,
        );
        let before = sheet.clone();
        let mut doc = Document::new(Workbook::new(sheet));
        let mut view = view_with_selection(&[], &[grouped]);
        assert!(!BoardAction::Rotate(90.0).available(&doc, &view));
        assert!(!apply_board_action(&mut doc, &mut view, BoardAction::Rotate(90.0)).unwrap());
        assert!(!doc.can_undo());

        view.board_selection = vec![grouped, a, b];
        let center = pivot(
            &scene(&doc, &mut view),
            &[SceneId::Object(a), SceneId::Object(b)],
        );
        assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(90.0)).unwrap());
        assert_eq!(
            doc.sheet().canvas.object(grouped),
            before.canvas.object(grouped)
        );
        assert_eq!(doc.sheet().topic(topic), before.topic(topic));
        for id in [a, b] {
            close(
                doc.sheet().canvas.object(id).unwrap().rect.center(),
                rotate_around(
                    before.canvas.object(id).unwrap().rect.center(),
                    center,
                    90.0,
                ),
            );
        }
        assert!(doc.undo());
        assert_eq!(doc.sheet(), &before);
    }
}

#[test]
fn a_directly_locked_connector_blocks_its_whole_group() {
    let mut sheet = board();
    let object = shape(
        &mut sheet,
        Vec2::new(-100.0, 0.0),
        Vec2::new(40.0, 20.0),
        0.0,
    );
    let line = connector(
        &mut sheet,
        Endpoint::Object(object),
        Endpoint::Point(Vec2::new(100.0, 0.0)),
    );
    sheet.group_canvas_objects(&[object, line]).unwrap();
    sheet
        .set_board_members_locked(&[BoardMember::Object(line)], true)
        .unwrap();
    let before = sheet.clone();
    let mut doc = Document::new(Workbook::new(sheet));
    let mut view = view_with_selection(&[], &[object]);
    assert!(!BoardAction::Rotate(90.0).available(&doc, &view));
    assert!(!apply_board_action(&mut doc, &mut view, BoardAction::Rotate(90.0)).unwrap());
    assert_eq!(doc.sheet(), &before);
    assert!(!doc.can_undo());
}

#[test]
fn clipping_does_not_change_the_selection_pivot_or_zone_membership() {
    let mut sheet = board();
    let topic = card(
        &mut sheet,
        Vec2::new(-90.0, 0.0),
        Vec2::new(100.0, 40.0),
        0.0,
    );
    let object = shape(&mut sheet, Vec2::new(60.0, 0.0), Vec2::new(40.0, 20.0), 0.0);
    let frame = Rect::from_min_size(Vec2::new(-100.0, -50.0), Vec2::new(200.0, 100.0));
    let zone = sheet.create_zone("Frame", vec![topic], frame).unwrap();
    sheet.zones.iter_mut().find(|z| z.id == zone).unwrap().clip = true;
    sheet
        .group_board_members(&[BoardMember::Card(topic), BoardMember::Object(object)])
        .unwrap();
    let mut doc = Document::new(Workbook::new(sheet));
    let mut view = view_with_selection(&[topic], &[]);
    let before_scene = scene(&doc, &mut view);
    let center = pivot(
        &before_scene,
        &[SceneId::Topic(topic), SceneId::Object(object)],
    );
    close(center, Vec2::new(-30.0, 0.0));
    assert_eq!(
        before_scene.item(SceneId::Topic(topic)).unwrap().clip,
        Some(frame)
    );
    assert_eq!(
        before_scene.item(SceneId::Object(object)).unwrap().clip,
        Some(frame)
    );

    assert!(apply_board_action(&mut doc, &mut view, BoardAction::Rotate(90.0)).unwrap());
    close(
        doc.sheet().topic(topic).unwrap().position.unwrap(),
        Vec2::new(-30.0, -60.0),
    );
    close(
        doc.sheet().canvas.object(object).unwrap().rect.center(),
        Vec2::new(-30.0, 90.0),
    );
    let actual_zone = doc.sheet().zones.iter().find(|z| z.id == zone).unwrap();
    assert_eq!(actual_zone.frame, frame);
    assert_eq!(actual_zone.roots, vec![topic]);
    let after_scene = scene(&doc, &mut view);
    assert_eq!(
        after_scene.item(SceneId::Topic(topic)).unwrap().clip,
        Some(frame)
    );
    assert_eq!(
        after_scene.item(SceneId::Object(object)).unwrap().clip,
        None
    );
}

#[test]
fn invalid_angles_and_out_of_range_results_leave_state_and_redo_intact() {
    let mut sheet = board();
    let x = MAX_BOARD_COORDINATE - 50.0;
    let y = MAX_BOARD_COORDINATE - 100.0;
    let a = shape(&mut sheet, Vec2::new(x, y), Vec2::new(20.0, 20.0), 0.0);
    let b = card(&mut sheet, Vec2::new(x, -y), Vec2::new(20.0, 20.0), 0.0);
    sheet
        .group_board_members(&[BoardMember::Object(a), BoardMember::Card(b)])
        .unwrap();
    let mut doc = Document::new(Workbook::new(sheet));
    doc.rotate_canvas_object(a, 30.0).unwrap();
    let committed = doc.sheet().clone();
    assert!(doc.undo());
    let before = doc.sheet().clone();
    let state = doc.state_id();
    let mut view = view_with_selection(&[], &[a]);
    for angle in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(!BoardAction::Rotate(angle).available(&doc, &view));
        assert!(!apply_board_action(&mut doc, &mut view, BoardAction::Rotate(angle)).unwrap());
        assert_eq!(doc.sheet(), &before);
        assert_eq!(doc.state_id(), state);
        assert!(doc.can_redo());
    }
    for angle in [0.0, 360.0, -720.0] {
        assert!(!apply_board_action(&mut doc, &mut view, BoardAction::Rotate(angle)).unwrap());
        assert_eq!(doc.sheet(), &before);
        assert_eq!(doc.state_id(), state);
        assert!(doc.can_redo());
    }
    let result = apply_board_action(&mut doc, &mut view, BoardAction::Rotate(90.0));
    assert!(result.is_err());
    assert_eq!(doc.sheet(), &before);
    assert_eq!(doc.state_id(), state);
    assert!(!doc.can_undo());
    assert!(doc.can_redo());
    assert!(doc.redo());
    assert_eq!(doc.sheet(), &committed);
}

#[test]
fn keyboard_rotates_an_expanded_mixed_group_and_undo_restores_it() {
    struct State {
        doc: Document,
        view: MapView,
        errors: Vec<thinoda_core::EditError>,
    }

    let mut sheet = board();
    let object = shape(
        &mut sheet,
        Vec2::new(-100.0, 0.0),
        Vec2::new(40.0, 20.0),
        0.0,
    );
    let topic = card(
        &mut sheet,
        Vec2::new(100.0, 0.0),
        Vec2::new(40.0, 20.0),
        0.0,
    );
    sheet
        .group_board_members(&[BoardMember::Object(object), BoardMember::Card(topic)])
        .unwrap();
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
                view: view_with_selection(&[], &[object]),
                errors: Vec::new(),
            },
        );
    harness.run_steps(3);
    harness
        .ctx
        .memory_mut(|memory| memory.request_focus(MapCanvas::id()));
    harness.run_steps(1);
    let center = {
        let state = harness.state_mut();
        pivot(
            &scene(&state.doc, &mut state.view),
            &[SceneId::Object(object), SceneId::Topic(topic)],
        )
    };
    harness.key_press_modifiers(
        Modifiers::COMMAND
            .plus(Modifiers::ALT)
            .plus(Modifiers::SHIFT),
        Key::ArrowRight,
    );
    harness.run_steps(2);
    let after = harness.state().doc.sheet();
    close(
        after.canvas.object(object).unwrap().rect.center(),
        rotate_around(
            before.canvas.object(object).unwrap().rect.center(),
            center,
            15.0,
        ),
    );
    close(
        after.topic(topic).unwrap().position.unwrap(),
        rotate_around(before.topic(topic).unwrap().position.unwrap(), center, 15.0),
    );
    assert_eq!(after.canvas.object(object).unwrap().rotation, 15.0);
    assert_eq!(after.topic(topic).unwrap().card.unwrap().rotation, 15.0);
    assert!(harness.state().errors.is_empty());
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run_steps(2);
    assert_eq!(harness.state().doc.sheet(), &before);
    assert!(!harness.state().doc.can_undo());
    assert!(harness.state().errors.is_empty());
}
