//! Regression coverage for rigid board rotation and its geometry primitives.

use thinoda_core::board::{
    BoardMember, MAX_BOARD_COORDINATE, normalize_rotation, rotate_around, rotated_bounds,
    rotated_corners,
};
use thinoda_core::{
    ApproxMeasure, CanvasConnector, CanvasObject, CanvasObjectId, CanvasObjectKind, Card, CardKind,
    Color, ConnectorRoute, Document, EditError, Endpoint, InkStroke, InkTool, Rect, ShapeKind,
    Sheet, Vec2, Workbook, connector_path, layout_sheet,
};

const TOLERANCE: f32 = 0.001;

fn assert_vec_close(actual: Vec2, expected: Vec2) {
    assert!(
        (actual.x - expected.x).abs() <= TOLERANCE && (actual.y - expected.y).abs() <= TOLERANCE,
        "expected {expected:?}, got {actual:?}"
    );
}

fn connector(sheet: &Sheet, id: CanvasObjectId) -> &CanvasConnector {
    let CanvasObjectKind::Connector(connector) = &sheet.canvas.object(id).unwrap().kind else {
        panic!("expected a connector");
    };
    connector
}

fn path(sheet: &Sheet, id: CanvasObjectId) -> Vec<Vec2> {
    let layout = layout_sheet(sheet, &ApproxMeasure, false);
    connector_path(sheet, &layout, connector(sheet, id))
}

fn assert_path_rotated(before: &[Vec2], after: &[Vec2], pivot: Vec2) {
    assert_eq!(before.len(), 2);
    assert_eq!(after.len(), before.len());
    for (before, after) in before.iter().zip(after) {
        assert_vec_close(*after, rotate_around(*before, pivot, 90.0));
    }
}

#[test]
fn rotation_helpers_use_screen_clockwise_angles_and_preserve_distances() {
    let pivot = Vec2::new(50.0, -40.0);
    let first = Vec2::new(80.0, -30.0);
    let second = Vec2::new(0.0, -50.0);
    let first_rotated = rotate_around(first, pivot, 90.0);
    let second_rotated = rotate_around(second, pivot, 90.0);

    assert_vec_close(first_rotated, Vec2::new(40.0, -10.0));
    assert_vec_close(second_rotated, Vec2::new(60.0, -90.0));
    assert_vec_close(rotate_around(first_rotated, pivot, -90.0), first);
    assert_vec_close(
        rotate_around(Vec2::new(60.0, -40.0), pivot, -90.0),
        Vec2::new(50.0, -50.0),
    );
    assert!(
        ((first - second).length() - (first_rotated - second_rotated).length()).abs() < TOLERANCE
    );

    let rect = Rect::from_center_size(Vec2::new(110.0, -40.0), Vec2::new(30.0, 10.0));
    let expected_corners = [
        Vec2::new(115.0, -55.0),
        Vec2::new(115.0, -25.0),
        Vec2::new(105.0, -25.0),
        Vec2::new(105.0, -55.0),
    ];
    for (actual, expected) in rotated_corners(rect, 90.0)
        .into_iter()
        .zip(expected_corners)
    {
        assert_vec_close(actual, expected);
    }
    let bounds = rotated_bounds(rect, 90.0);
    assert_vec_close(bounds.min, Vec2::new(105.0, -55.0));
    assert_vec_close(bounds.size, Vec2::new(10.0, 30.0));
}

#[test]
fn single_object_rotation_is_absolute_and_does_not_rotate_its_group() {
    let mut sheet = Sheet::new("Board", "Root");
    let mut shape = CanvasObject::shape(
        ShapeKind::Rectangle,
        Rect::from_center_size(Vec2::new(120.0, 50.0), Vec2::new(40.0, 20.0)),
    );
    shape.rotation = 30.0;
    let shape = sheet.add_canvas_object(shape).unwrap();
    let mut card = Card::new(CardKind::Sticky, Vec2::new(40.0, 20.0));
    card.rotation = 45.0;
    let card = sheet
        .add_card("Card", Vec2::new(220.0, 50.0), card)
        .unwrap();
    let group = sheet
        .group_board_members(&[BoardMember::Object(shape), BoardMember::Card(card)])
        .unwrap();
    let before = sheet.clone();

    sheet.rotate_canvas_object(shape, 450.0).unwrap();
    assert_eq!(sheet.canvas.object(shape).unwrap().rotation, 90.0);
    sheet.rotate_canvas_object(shape, -90.0).unwrap();

    let rotated = sheet.canvas.object(shape).unwrap();
    assert_eq!(rotated.rotation, 270.0);
    assert_eq!(rotated.rect, before.canvas.object(shape).unwrap().rect);
    assert_eq!(rotated.group, Some(group));
    assert_eq!(sheet.topic(card), before.topic(card));
    assert_eq!(sheet.canvas.group(group), before.canvas.group(group));
    sheet.validate().unwrap();
}

#[test]
fn rotation_validation_differs_for_objects_cards_and_connectors() {
    let mut sheet = Sheet::new("Board", "Root");
    let shape = sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_center_size(Vec2::new(100.0, 100.0), Vec2::new(30.0, 20.0)),
        ))
        .unwrap();
    let card = sheet
        .add_card(
            "Card",
            Vec2::new(200.0, 100.0),
            Card::new(CardKind::Sticky, Vec2::new(40.0, 20.0)),
        )
        .unwrap();
    let connection = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(shape),
            Endpoint::Point(Vec2::new(150.0, 100.0)),
        )))
        .unwrap();
    let before = sheet.clone();
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        // The helper has a fallback, while the editing APIs reject non-finite angles.
        assert_eq!(normalize_rotation(invalid), 0.0);
        assert!(sheet.rotate_canvas_object(shape, invalid).is_err());
        let mut presentation = sheet.topic(card).unwrap().card.unwrap();
        presentation.rotation = invalid;
        assert!(sheet.set_topic_card(card, Some(presentation)).is_err());
        assert_eq!(sheet, before);
    }
    assert!(sheet.rotate_canvas_object(connection, 90.0).is_err());
    assert_eq!(sheet, before);

    sheet.rotate_canvas_object(shape, -90.0).unwrap();
    assert_eq!(sheet.canvas.object(shape).unwrap().rotation, 270.0);
    let mut presentation = sheet.topic(card).unwrap().card.unwrap();
    presentation.rotation = -90.0;
    let before_invalid_card = sheet.clone();
    assert!(sheet.set_topic_card(card, Some(presentation)).is_err());
    assert_eq!(sheet, before_invalid_card);
    presentation.rotation = normalize_rotation(presentation.rotation);
    sheet.set_topic_card(card, Some(presentation)).unwrap();
    assert_eq!(sheet.topic(card).unwrap().card.unwrap().rotation, 270.0);
    sheet.validate().unwrap();
}

#[test]
fn connector_anchors_reclip_to_rotated_targets_without_moving_free_points() {
    let mut sheet = Sheet::new("Board", "Root");
    let shape = sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_center_size(Vec2::new(300.0, 100.0), Vec2::new(80.0, 40.0)),
        ))
        .unwrap();
    let card = sheet
        .add_card(
            "Card",
            Vec2::new(600.0, 100.0),
            Card::new(CardKind::Sticky, Vec2::new(80.0, 40.0)),
        )
        .unwrap();
    let shape_connector = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(shape),
            Endpoint::Point(Vec2::new(400.0, 100.0)),
        )))
        .unwrap();
    let card_connector = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Topic(card),
            Endpoint::Point(Vec2::new(700.0, 100.0)),
        )))
        .unwrap();
    let shape_reference = connector(&sheet, shape_connector).clone();
    let card_reference = connector(&sheet, card_connector).clone();
    assert_vec_close(path(&sheet, shape_connector)[0], Vec2::new(340.0, 100.0));
    assert_vec_close(path(&sheet, card_connector)[0], Vec2::new(640.0, 100.0));

    sheet.rotate_canvas_object(shape, 90.0).unwrap();
    let mut presentation = sheet.topic(card).unwrap().card.unwrap();
    presentation.rotation = 90.0;
    sheet.set_topic_card(card, Some(presentation)).unwrap();
    assert_vec_close(path(&sheet, shape_connector)[0], Vec2::new(320.0, 100.0));
    assert_vec_close(path(&sheet, card_connector)[0], Vec2::new(620.0, 100.0));
    assert_vec_close(path(&sheet, shape_connector)[1], Vec2::new(400.0, 100.0));
    assert_vec_close(path(&sheet, card_connector)[1], Vec2::new(700.0, 100.0));
    assert_eq!(connector(&sheet, shape_connector), &shape_reference);
    assert_eq!(connector(&sheet, card_connector), &card_reference);

    sheet
        .move_canvas_objects(&[shape], Vec2::new(20.0, 0.0))
        .unwrap();
    sheet.set_position(card, Vec2::new(620.0, 100.0)).unwrap();
    assert_vec_close(path(&sheet, shape_connector)[0], Vec2::new(340.0, 100.0));
    assert_vec_close(path(&sheet, card_connector)[0], Vec2::new(640.0, 100.0));
    assert_vec_close(path(&sheet, shape_connector)[1], Vec2::new(400.0, 100.0));
    assert_vec_close(path(&sheet, card_connector)[1], Vec2::new(700.0, 100.0));
    assert_eq!(connector(&sheet, shape_connector), &shape_reference);
    assert_eq!(connector(&sheet, card_connector), &card_reference);
    sheet.validate().unwrap();
}

#[test]
fn elbow_and_curve_paths_are_rebuilt_in_sheet_axes_after_endpoint_updates() {
    for route in [ConnectorRoute::Elbow, ConnectorRoute::Curve] {
        let mut sheet = Sheet::new("Board", "Root");
        let mut connection = CanvasConnector::new(
            Endpoint::Point(Vec2::new(20.0, 0.0)),
            Endpoint::Point(Vec2::new(100.0, 40.0)),
        );
        connection.route = route;
        let id = sheet
            .add_canvas_object(CanvasObject::connector(connection))
            .unwrap();
        let before = path(&sheet, id);

        // Move each free endpoint to its 90-degree-rotated location. Routing is
        // derived from those endpoints; it is not stored as a rotated polyline.
        sheet
            .update_canvas_object(id, |object| {
                let CanvasObjectKind::Connector(connection) = &mut object.kind else {
                    panic!("expected a connector");
                };
                connection.from = Endpoint::Point(Vec2::new(0.0, 20.0));
                connection.to = Endpoint::Point(Vec2::new(-40.0, 100.0));
                object.rect = Rect::from_min_size(Vec2::new(-40.0, 20.0), Vec2::new(40.0, 80.0));
            })
            .unwrap();
        let after = path(&sheet, id);
        assert_eq!(after.len(), before.len());
        assert_vec_close(after[0], rotate_around(before[0], Vec2::ZERO, 90.0));
        assert_vec_close(
            *after.last().unwrap(),
            rotate_around(*before.last().unwrap(), Vec2::ZERO, 90.0),
        );
        let interior = if route == ConnectorRoute::Elbow {
            assert_eq!(after.len(), 4);
            assert_vec_close(after[1], Vec2::new(-20.0, 20.0));
            assert_vec_close(after[2], Vec2::new(-20.0, 100.0));
            1
        } else {
            assert_eq!(after.len(), 17);
            4
        };
        assert!(
            (after[interior] - rotate_around(before[interior], Vec2::ZERO, 90.0)).length() > 1.0,
            "{route:?} should be re-routed, not rigidly rotated"
        );
        sheet.validate().unwrap();
    }
}

#[test]
fn moving_a_group_translates_free_connector_ends_once_and_keeps_anchors() {
    let mut sheet = Sheet::new("Board", "Root");
    let shape = sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_center_size(Vec2::new(300.0, 100.0), Vec2::new(80.0, 40.0)),
        ))
        .unwrap();
    let mixed = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(shape),
            Endpoint::Point(Vec2::new(400.0, 100.0)),
        )))
        .unwrap();
    let free = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Point(Vec2::new(340.0, 140.0)),
            Endpoint::Point(Vec2::new(420.0, 170.0)),
        )))
        .unwrap();
    sheet.group_canvas_objects(&[shape, mixed, free]).unwrap();
    let before_path = path(&sheet, mixed);
    let delta = Vec2::new(10.0, -20.0);

    // This exercises existing translation semantics, not group rotation.
    sheet
        .move_board_members(
            &[
                BoardMember::Object(shape),
                BoardMember::Object(shape),
                BoardMember::Object(mixed),
            ],
            delta,
        )
        .unwrap();
    assert_eq!(
        sheet.canvas.object(shape).unwrap().rect.center(),
        Vec2::new(310.0, 80.0)
    );
    assert_eq!(connector(&sheet, mixed).from, Endpoint::Object(shape));
    assert_eq!(
        connector(&sheet, mixed).to,
        Endpoint::Point(Vec2::new(410.0, 80.0))
    );
    assert_eq!(
        connector(&sheet, free).from,
        Endpoint::Point(Vec2::new(350.0, 120.0))
    );
    assert_eq!(
        connector(&sheet, free).to,
        Endpoint::Point(Vec2::new(430.0, 150.0))
    );
    let after_path = path(&sheet, mixed);
    assert_eq!(after_path.len(), before_path.len());
    for (before, after) in before_path.iter().zip(after_path) {
        assert_vec_close(after, *before + delta);
    }
    sheet.validate().unwrap();
}

#[test]
fn rigid_mixed_group_commits_and_undoes_as_one_snapshot() {
    let pivot = Vec2::new(100.0, 100.0);
    let mut sheet = Sheet::new("Board", "Root");
    let mut shape = CanvasObject::shape(
        ShapeKind::Rectangle,
        Rect::from_center_size(Vec2::new(140.0, 100.0), Vec2::new(20.0, 10.0)),
    );
    shape.rotation = 30.0;
    let shape = sheet.add_canvas_object(shape).unwrap();
    let mut stroke = InkStroke::new(
        InkTool::Pen,
        Color::BLACK,
        2.0,
        vec![Vec2::new(100.0, 130.0), Vec2::new(100.0, 150.0)],
    );
    stroke.pressure = Some(vec![0.25, 0.75].into());
    let mut ink = CanvasObject::ink(stroke);
    ink.rotation = 20.0;
    let ink = sheet.add_canvas_object(ink).unwrap();
    let mut presentation = Card::new(CardKind::Sticky, Vec2::new(40.0, 20.0));
    presentation.rotation = 45.0;
    let card = sheet
        .add_card("Card", Vec2::new(60.0, 100.0), presentation)
        .unwrap();
    let mixed = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(shape),
            Endpoint::Point(Vec2::new(160.0, 110.0)),
        )))
        .unwrap();
    let anchored = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Topic(card),
            Endpoint::Object(shape),
        )))
        .unwrap();
    sheet
        .group_board_members(&[
            BoardMember::Object(shape),
            BoardMember::Object(ink),
            BoardMember::Object(mixed),
            BoardMember::Object(anchored),
            BoardMember::Card(card),
        ])
        .unwrap();
    sheet.validate().unwrap();
    let before_mixed_path = path(&sheet, mixed);
    let before_anchored_path = path(&sheet, anchored);
    let before = sheet.clone();
    let mut doc = Document::new(Workbook::new(sheet));

    // Selecting one member expands the mixed group exactly once.
    doc.edit_sheet("Rotate mixed group", |sheet| {
        sheet
            .rotate_board_members(
                &[BoardMember::Object(shape), BoardMember::Object(shape)],
                pivot,
                90.0,
            )
            .map_err(EditError::Invalid)
    })
    .unwrap();

    let after = doc.sheet();
    let old_shape = before.canvas.object(shape).unwrap();
    let new_shape = after.canvas.object(shape).unwrap();
    assert_vec_close(new_shape.rect.center(), Vec2::new(100.0, 140.0));
    assert_eq!(new_shape.rect.size, old_shape.rect.size);
    for (old, new) in rotated_corners(old_shape.rect, old_shape.rotation)
        .into_iter()
        .zip(rotated_corners(new_shape.rect, new_shape.rotation))
    {
        assert_vec_close(new, rotate_around(old, pivot, 90.0));
    }
    let old_ink = before.canvas.object(ink).unwrap();
    let new_ink = after.canvas.object(ink).unwrap();
    let (CanvasObjectKind::Ink(old_stroke), CanvasObjectKind::Ink(new_stroke)) =
        (&old_ink.kind, &new_ink.kind)
    else {
        panic!("expected ink strokes");
    };
    assert_eq!(new_stroke.points.len(), old_stroke.points.len());
    assert_eq!(new_stroke.pressure, old_stroke.pressure);
    assert_eq!(new_stroke.width, old_stroke.width);
    for (old, new) in old_stroke.points.iter().zip(new_stroke.points.iter()) {
        let old_drawn = rotate_around(*old, old_ink.rect.center(), old_ink.rotation);
        let new_drawn = rotate_around(*new, new_ink.rect.center(), new_ink.rotation);
        assert_vec_close(new_drawn, rotate_around(old_drawn, pivot, 90.0));
    }
    assert_eq!(
        after.topic(card).unwrap().position,
        Some(Vec2::new(100.0, 60.0))
    );
    assert_eq!(after.topic(card).unwrap().card.unwrap().rotation, 135.0);
    assert_eq!(connector(after, mixed).from, Endpoint::Object(shape));
    assert_eq!(
        connector(after, mixed).to,
        Endpoint::Point(Vec2::new(90.0, 160.0))
    );
    assert_eq!(connector(after, anchored), connector(&before, anchored));
    assert_path_rotated(&before_mixed_path, &path(after, mixed), pivot);
    assert_path_rotated(&before_anchored_path, &path(after, anchored), pivot);
    after.validate().unwrap();
    let after = after.clone();

    assert_eq!(doc.undo_label(), Some("Rotate mixed group"));
    assert!(doc.undo());
    assert_eq!(doc.sheet(), &before);
    assert!(!doc.can_undo());
    assert!(doc.redo());
    assert_eq!(doc.sheet(), &after);
}

#[test]
fn locked_group_or_locked_card_blocks_a_composed_edit_atomically() {
    let mut base = Sheet::new("Board", "Root");
    let grouped = base
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_center_size(Vec2::new(100.0, 100.0), Vec2::new(40.0, 20.0)),
        ))
        .unwrap();
    let unrelated = base
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_center_size(Vec2::new(300.0, 100.0), Vec2::new(40.0, 20.0)),
        ))
        .unwrap();
    let card = base
        .add_card(
            "Card",
            Vec2::new(200.0, 100.0),
            Card::new(CardKind::Sticky, Vec2::new(40.0, 20.0)),
        )
        .unwrap();
    let group = base
        .group_board_members(&[BoardMember::Object(grouped), BoardMember::Card(card)])
        .unwrap();

    for group_locked in [false, true] {
        let mut sheet = base.clone();
        if group_locked {
            sheet.set_canvas_group_locked(group, true).unwrap();
        } else {
            sheet
                .set_board_members_locked(&[BoardMember::Card(card)], true)
                .unwrap();
            assert!(!sheet.is_board_member_locked(BoardMember::Object(grouped)));
        }
        assert!(sheet.is_board_member_locked(BoardMember::Card(card)));
        let before = sheet.clone();
        assert!(
            sheet
                .rotate_board_members(&[BoardMember::Object(grouped)], Vec2::ZERO, 90.0,)
                .is_err()
        );
        assert_eq!(sheet, before);
        let mut doc = Document::new(Workbook::new(sheet));
        let state = doc.state_id();
        let result = doc.edit_sheet("Rejected composition", |sheet| {
            sheet
                .rotate_canvas_object(unrelated, 30.0)
                .map_err(EditError::Invalid)?;
            sheet
                .rotate_canvas_object(grouped, 90.0)
                .map_err(EditError::Invalid)?;
            Ok(())
        });
        assert!(matches!(result, Err(EditError::Invalid(message)) if message.contains("locked")));
        assert_eq!(doc.sheet(), &before);
        assert_eq!(doc.state_id(), state);
        assert!(!doc.can_undo());
    }
}

#[test]
fn invalid_composed_edits_restore_state_and_preserve_redo() {
    let mut sheet = Sheet::new("Board", "Root");
    let shape = sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_center_size(Vec2::new(100.0, 100.0), Vec2::new(40.0, 20.0)),
        ))
        .unwrap();
    let connection = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(shape),
            Endpoint::Point(Vec2::new(200.0, 100.0)),
        )))
        .unwrap();
    let mut doc = Document::new(Workbook::new(sheet));
    doc.rotate_canvas_object(shape, 30.0).unwrap();
    let committed = doc.sheet().clone();
    assert!(doc.undo());
    let before = doc.sheet().clone();
    let state = doc.state_id();
    assert_eq!(doc.redo_label(), Some("Rotate object"));

    doc.edit_sheet("Equivalent rotation", |sheet| {
        sheet
            .rotate_canvas_object(shape, 360.0)
            .map_err(EditError::Invalid)
    })
    .unwrap();
    assert_eq!(doc.sheet(), &before);
    assert_eq!(doc.state_id(), state);
    assert!(!doc.can_undo());
    assert_eq!(doc.redo_label(), Some("Rotate object"));

    for invalid in [f32::NAN, f32::INFINITY, MAX_BOARD_COORDINATE + 1.0] {
        let rejected = doc.edit_sheet("Invalid endpoint", |sheet| {
            sheet
                .rotate_canvas_object(shape, 90.0)
                .map_err(EditError::Invalid)?;
            sheet
                .update_canvas_object(connection, |object| {
                    let CanvasObjectKind::Connector(connector) = &mut object.kind else {
                        panic!("expected a connector");
                    };
                    connector.to = Endpoint::Point(Vec2::new(invalid, 0.0));
                })
                .map_err(EditError::Invalid)?;
            Ok(())
        });
        assert!(
            matches!(rejected, Err(EditError::Invalid(message)) if message.contains("invalid end point"))
        );
        assert_eq!(doc.sheet(), &before);
        assert_eq!(doc.state_id(), state);
        assert!(!doc.can_undo());
        assert_eq!(doc.redo_label(), Some("Rotate object"));
    }

    // Even when a closure returns Ok, whole-board validation can reject it.
    let rejected = doc.edit_sheet("Invalid final state", |sheet| {
        sheet
            .rotate_canvas_object(shape, 90.0)
            .map_err(EditError::Invalid)?;
        sheet.canvas.object_mut(shape).unwrap().rotation = f32::NAN;
        Ok(())
    });
    assert!(
        matches!(rejected, Err(EditError::Invalid(message)) if message.contains("invalid rotation"))
    );
    assert_eq!(doc.sheet(), &before);
    assert_eq!(doc.state_id(), state);
    assert!(!doc.can_undo());
    assert_eq!(doc.redo_label(), Some("Rotate object"));
    assert!(doc.redo());
    assert_eq!(doc.sheet(), &committed);
}

#[test]
fn rigid_rotation_rejects_invalid_inputs_and_preserves_noop_state() {
    let mut sheet = Sheet::new("Board", "Root");
    let first = sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_center_size(Vec2::new(-20.0, 0.0), Vec2::new(20.0, 20.0)),
        ))
        .unwrap();
    let second = sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_center_size(Vec2::new(20.0, 0.0), Vec2::new(20.0, 20.0)),
        ))
        .unwrap();
    sheet.group_canvas_objects(&[first, second]).unwrap();
    let before = sheet.clone();
    for angle in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(
            sheet
                .rotate_board_members(&[BoardMember::Object(first)], Vec2::ZERO, angle)
                .is_err()
        );
        assert_eq!(sheet, before);
    }
    for pivot in [
        Vec2::new(f32::NAN, 0.0),
        Vec2::new(0.0, f32::INFINITY),
        Vec2::new(MAX_BOARD_COORDINATE + 1.0, 0.0),
    ] {
        assert!(
            sheet
                .rotate_board_members(&[BoardMember::Object(first)], pivot, 90.0)
                .is_err()
        );
        assert_eq!(sheet, before);
    }
    assert!(sheet.rotate_board_members(&[], Vec2::ZERO, 90.0).is_err());
    assert!(
        sheet
            .rotate_board_members(
                &[BoardMember::Object(CanvasObjectId(u64::MAX))],
                Vec2::ZERO,
                90.0,
            )
            .is_err()
    );
    assert_eq!(sheet, before);
    for angle in [0.0, 360.0, -720.0] {
        sheet
            .rotate_board_members(&[BoardMember::Object(first)], Vec2::ZERO, angle)
            .unwrap();
        assert_eq!(sheet, before);
    }
}

#[test]
fn rigid_rotation_rejects_out_of_range_members_without_partial_mutation() {
    let x = MAX_BOARD_COORDINATE - 100.0;
    let y = -MAX_BOARD_COORDINATE + 100.0;
    for kind in ["card", "shape", "ink", "connector"] {
        let mut sheet = Sheet::new("Board", "Root");
        let anchor = sheet
            .add_canvas_object(CanvasObject::shape(
                ShapeKind::Rectangle,
                Rect::from_center_size(Vec2::new(x, 0.0), Vec2::new(20.0, 20.0)),
            ))
            .unwrap();
        let other = match kind {
            "card" => BoardMember::Card(
                sheet
                    .add_card(
                        "Card",
                        Vec2::new(x, y),
                        Card::new(CardKind::Sticky, Vec2::new(20.0, 20.0)),
                    )
                    .unwrap(),
            ),
            "shape" => BoardMember::Object(
                sheet
                    .add_canvas_object(CanvasObject::shape(
                        ShapeKind::Rectangle,
                        Rect::from_center_size(Vec2::new(x, y), Vec2::new(20.0, 20.0)),
                    ))
                    .unwrap(),
            ),
            "ink" => BoardMember::Object(
                sheet
                    .add_canvas_object(CanvasObject::ink(InkStroke::new(
                        InkTool::Pen,
                        Color::BLACK,
                        2.0,
                        vec![Vec2::new(x, y), Vec2::new(x - 10.0, y + 10.0)],
                    )))
                    .unwrap(),
            ),
            "connector" => BoardMember::Object(
                sheet
                    .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
                        Endpoint::Object(anchor),
                        Endpoint::Point(Vec2::new(x, y)),
                    )))
                    .unwrap(),
            ),
            _ => unreachable!(),
        };
        sheet
            .group_board_members(&[BoardMember::Object(anchor), other])
            .unwrap();
        sheet.validate().unwrap();
        let before = sheet.clone();
        let result =
            sheet.rotate_board_members(&[BoardMember::Object(anchor)], Vec2::new(x, 0.0), 90.0);
        assert!(result.is_err(), "{kind} should exceed the coordinate limit");
        assert_eq!(sheet, before, "{kind} must not leave partial edits");
    }
}

#[test]
fn tiny_negative_rotation_is_not_discarded_before_its_inverse() {
    let mut sheet = Sheet::new("Board", "Root");
    let mut object = CanvasObject::shape(
        ShapeKind::Rectangle,
        Rect::from_center_size(Vec2::new(500_000.0, 200_000.0), Vec2::new(80.0, 40.0)),
    );
    object.rotation = 30.0;
    let object = sheet.add_canvas_object(object).unwrap();
    let before = sheet.canvas.object(object).unwrap().clone();
    let members = [BoardMember::Object(object)];

    sheet
        .rotate_board_members(&members, Vec2::ZERO, -0.000_01)
        .unwrap();
    assert_ne!(sheet.canvas.object(object).unwrap().rect, before.rect);
    sheet
        .rotate_board_members(&members, Vec2::ZERO, 0.000_01)
        .unwrap();
    let after = sheet.canvas.object(object).unwrap();
    assert!((after.rect.min.x - before.rect.min.x).abs() <= 0.125);
    assert!((after.rect.min.y - before.rect.min.y).abs() <= 0.125);
    assert_eq!(after.rotation, before.rotation);
}
