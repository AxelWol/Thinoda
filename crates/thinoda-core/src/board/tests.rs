use super::*;
use crate::Document;
use crate::model::{Relationship, SlideId};

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::from_min_size(Vec2::new(x, y), Vec2::new(w, h))
}

fn stroke(points: &[(f32, f32)]) -> InkStroke {
    InkStroke::new(
        InkTool::Pen,
        Color::rgb(10, 20, 30),
        4.0,
        points.iter().map(|(x, y)| Vec2::new(*x, *y)).collect(),
    )
}

/// Hidden-mode sheet with two floating roots and one card.
fn board() -> (Sheet, TopicId, TopicId, TopicId) {
    let mut s = Sheet::new("Board", "");
    s.root_mode = SheetRootMode::Hidden;
    let a = s.add_floating("A", Vec2::new(300.0, 200.0));
    let b = s.add_floating("B", Vec2::new(-200.0, 50.0));
    let card = s
        .add_card(
            "Note",
            Vec2::new(0.0, 400.0),
            Card::new(CardKind::Sticky, Vec2::new(160.0, 120.0)),
        )
        .unwrap();
    s.validate().unwrap();
    (s, a, b, card)
}

// ---- serde ------------------------------------------------------------------------------

#[test]
fn connector_markers_preserve_legacy_json_and_reject_conflicting_flags() {
    let mut connector = CanvasConnector::new(
        Endpoint::Point(Vec2::ZERO),
        Endpoint::Point(Vec2::new(100.0, 0.0)),
    );
    let legacy = serde_json::to_string(&connector).unwrap();
    assert_eq!(
        legacy,
        r#"{"from":{"point":{"x":0.0,"y":0.0}},"to":{"point":{"x":100.0,"y":0.0}},"route":"straight","arrow_end":true,"width":2.0}"#
    );
    assert!(!legacy.contains("start_marker"));
    assert!(!legacy.contains("end_marker"));
    assert_eq!(
        serde_json::from_str::<CanvasConnector>(&legacy).unwrap(),
        connector
    );

    connector.arrow_start = true;
    connector.start_marker = Some(ConnectorMarker {
        kind: ConnectorMarkerKind::Open,
    });
    connector.end_marker = Some(ConnectorMarker {
        kind: ConnectorMarkerKind::Block,
    });
    let encoded = serde_json::to_value(&connector).unwrap();
    assert_eq!(
        serde_json::from_value::<CanvasConnector>(encoded.clone()).unwrap(),
        connector
    );
    validate_object(&CanvasObject::connector(connector.clone())).unwrap();
    for (flag, marker) in [("arrow_start", "start_marker"), ("arrow_end", "end_marker")] {
        let mut forged = encoded.clone();
        forged[flag] = serde_json::json!(false);
        assert!(
            serde_json::from_value::<CanvasConnector>(forged)
                .unwrap_err()
                .to_string()
                .contains("marker conflicts")
        );
        let mut forged = encoded.clone();
        forged[marker]["unknown"] = serde_json::json!(1);
        assert!(serde_json::from_value::<CanvasConnector>(forged).is_err());
    }

    connector.set_arrow_start(false);
    connector.set_arrow_end(false);
    assert!(connector.start_marker.is_none());
    assert!(connector.end_marker.is_none());
    connector.start_marker = Some(ConnectorMarker {
        kind: ConnectorMarkerKind::Open,
    });
    assert!(
        validate_object(&CanvasObject::connector(connector))
            .unwrap_err()
            .contains("marker conflicts")
    );
}

#[test]
fn connector_waypoint_limits_and_edits_are_atomic() {
    let mut connector =
        CanvasConnector::new(Endpoint::Point(Vec2::ZERO), Endpoint::Point(Vec2::ZERO));
    let default_json = serde_json::to_string(&connector).unwrap();
    assert!(!default_json.contains("waypoints"));

    connector.waypoints = vec![Vec2::ZERO; MAX_CONNECTOR_WAYPOINTS].into();
    validate_object(&CanvasObject::connector(connector.clone())).unwrap();
    connector.waypoints = vec![Vec2::ZERO; MAX_CONNECTOR_WAYPOINTS + 1].into();
    assert!(
        validate_object(&CanvasObject::connector(connector.clone()))
            .unwrap_err()
            .contains("waypoints")
    );
    connector.waypoints = vec![Vec2::new(f32::NAN, 0.0)].into();
    assert!(
        validate_object(&CanvasObject::connector(connector.clone()))
            .unwrap_err()
            .contains("invalid waypoint")
    );

    let mut sheet = Sheet::new("Limits", "Root");
    connector.waypoints = vec![Vec2::ZERO; MAX_CONNECTOR_WAYPOINTS].into();
    for _ in 0..MAX_TOTAL_CONNECTOR_WAYPOINTS / MAX_CONNECTOR_WAYPOINTS {
        sheet
            .add_canvas_object(CanvasObject::connector(connector.clone()))
            .unwrap();
    }
    connector.waypoints =
        vec![Vec2::ZERO; MAX_TOTAL_CONNECTOR_WAYPOINTS % MAX_CONNECTOR_WAYPOINTS].into();
    let last = sheet
        .add_canvas_object(CanvasObject::connector(connector.clone()))
        .unwrap();
    assert_eq!(
        sheet.canvas.connector_waypoint_count(),
        MAX_TOTAL_CONNECTOR_WAYPOINTS
    );
    sheet.validate().unwrap();
    let before = sheet.canvas.clone();
    connector.waypoints = vec![Vec2::ZERO].into();
    assert!(
        sheet
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap_err()
            .contains("waypoints per sheet")
    );
    assert!(
        sheet
            .update_canvas_object(last, |object| {
                if let CanvasObjectKind::Connector(c) = &mut object.kind {
                    let mut points = c.waypoints.to_vec();
                    points.push(Vec2::ZERO);
                    c.waypoints = points.into();
                }
            })
            .unwrap_err()
            .contains("waypoints per sheet")
    );
    assert_eq!(sheet.canvas, before);
}

#[test]
fn connector_waypoints_follow_move_rotation_and_undo() {
    let mut sheet = Sheet::new("Waypoints", "Root");
    let mut connector = CanvasConnector::new(
        Endpoint::Point(Vec2::new(0.0, 0.0)),
        Endpoint::Point(Vec2::new(100.0, 0.0)),
    );
    connector.waypoints = vec![Vec2::new(20.0, 50.0), Vec2::new(80.0, 50.0)].into();
    let id = sheet
        .add_canvas_object(CanvasObject::connector(connector.clone()))
        .unwrap();
    let mut doc = Document::new(crate::Workbook::new(sheet));
    doc.move_canvas_objects(&[id], Vec2::new(10.0, 30.0))
        .unwrap();
    let CanvasObjectKind::Connector(moved) = &doc.sheet().canvas.object(id).unwrap().kind else {
        panic!("expected connector");
    };
    assert_eq!(
        moved.waypoints.as_ref(),
        &[Vec2::new(30.0, 80.0), Vec2::new(90.0, 80.0)]
    );
    assert_eq!(moved.from, Endpoint::Point(Vec2::new(10.0, 30.0)));
    assert!(doc.undo());
    assert_eq!(
        doc.sheet().canvas.object(id).unwrap().kind,
        CanvasObjectKind::Connector(connector)
    );

    doc.edit_sheet("Rotate connector", |sheet| {
        sheet
            .rotate_board_members(&[BoardMember::Object(id)], Vec2::ZERO, 90.0)
            .map_err(crate::EditError::Invalid)
    })
    .unwrap();
    let CanvasObjectKind::Connector(rotated) = &doc.sheet().canvas.object(id).unwrap().kind else {
        panic!("expected connector");
    };
    assert!((rotated.waypoints[0] - Vec2::new(-50.0, 20.0)).length() < 0.001);
    assert!((rotated.waypoints[1] - Vec2::new(-50.0, 80.0)).length() < 0.001);
    assert!(doc.undo());

    let mut near_limit =
        CanvasConnector::new(Endpoint::Point(Vec2::ZERO), Endpoint::Point(Vec2::ZERO));
    near_limit.waypoints = vec![Vec2::new(MAX_BOARD_COORDINATE, 0.0)].into();
    let mut sheet = Sheet::new("Limit", "Root");
    let id = sheet
        .add_canvas_object(CanvasObject::connector(near_limit))
        .unwrap();
    let before = sheet.canvas.clone();
    assert!(
        sheet
            .move_canvas_objects(&[id], Vec2::new(1.0, 0.0))
            .is_err()
    );
    assert_eq!(sheet.canvas, before);
}

#[test]
fn deleting_a_waypoint_connector_keeps_edge_attachments_on_its_path() {
    let mut sheet = Sheet::new("Attachments", "");
    let mut target = CanvasConnector::new(
        Endpoint::Point(Vec2::ZERO),
        Endpoint::Point(Vec2::new(250.0, 100.0)),
    );
    target.waypoints = vec![Vec2::new(200.0, 0.0), Vec2::new(200.0, 100.0)].into();
    let target = sheet
        .add_canvas_object(CanvasObject::connector(target))
        .unwrap();
    let attached = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(target),
            Endpoint::Point(Vec2::new(300.0, 200.0)),
        )))
        .unwrap();
    let expected = Endpoint::Point(Vec2::new(175.0, 0.0));

    let before = sheet.clone();
    sheet.canvas.objects.retain(|object| object.id != target);
    assert_eq!(sheet.fix_dangling_endpoints(&before), 1);
    let CanvasObjectKind::Connector(connector) = &sheet.canvas.object(attached).unwrap().kind
    else {
        panic!("expected connector");
    };
    assert_eq!(connector.from, expected);

    let mut document = Document::new(crate::Workbook::new(before));
    document.remove_canvas_objects(&[target]).unwrap();
    let CanvasObjectKind::Connector(connector) =
        &document.sheet().canvas.object(attached).unwrap().kind
    else {
        panic!("expected connector");
    };
    assert_eq!(connector.from, expected);
    assert!(document.undo());
    let CanvasObjectKind::Connector(connector) =
        &document.sheet().canvas.object(attached).unwrap().kind
    else {
        panic!("expected connector");
    };
    assert_eq!(connector.from, Endpoint::Object(target));
}

#[test]
fn zone_membership_transfers_mixed_members_and_retains_empty_zones_with_undo() {
    let (mut sheet, first, second, card) = board();
    let object = sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            rect(20.0, 30.0, 80.0, 60.0),
        ))
        .unwrap();
    let source = sheet
        .create_zone("Source", vec![first], rect(0.0, 0.0, 400.0, 400.0))
        .unwrap();
    let target = sheet
        .create_zone("Target", vec![second], rect(500.0, 0.0, 400.0, 400.0))
        .unwrap();
    sheet
        .set_zone_members(source, vec![first, card], vec![object])
        .unwrap();
    sheet.zones[0].keep_empty = false;
    let before = sheet.clone();
    let mut doc = Document::new(crate::Workbook::new(sheet));
    doc.set_zone_members(target, vec![first, card], vec![object])
        .unwrap();
    let source_zone = doc
        .sheet()
        .zones
        .iter()
        .find(|zone| zone.id == source)
        .unwrap();
    assert!(
        source_zone.keep_empty && source_zone.roots.is_empty() && source_zone.objects.is_empty()
    );
    assert_eq!(doc.sheet().canvas, before.canvas);
    assert_eq!(doc.sheet().topics, before.topics);
    assert_eq!(doc.sheet().zones[1].frame, before.zones[1].frame);
    doc.sheet().validate().unwrap();
    assert!(doc.undo());
    assert_eq!(doc.sheet(), &before);
    assert!(doc.redo());
    doc.set_zone_members(target, vec![], vec![]).unwrap();
    assert!(
        doc.sheet()
            .zones
            .iter()
            .all(|zone| zone.keep_empty && zone.roots.is_empty() && zone.objects.is_empty())
    );
    doc.sheet().validate().unwrap();
}

#[test]
fn zone_membership_rejects_duplicate_and_dangling_objects_atomically() {
    let (mut sheet, root, _, _) = board();
    let object = sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            rect(0.0, 0.0, 80.0, 60.0),
        ))
        .unwrap();
    let one = sheet
        .create_zone("One", vec![root], rect(0.0, 0.0, 200.0, 200.0))
        .unwrap();
    let two = sheet
        .create_board_frame("Two", rect(300.0, 0.0, 200.0, 200.0))
        .unwrap();
    sheet
        .set_zone_members(one, vec![root], vec![object])
        .unwrap();
    let before = sheet.clone();
    for (roots, objects) in [
        (vec![root, root], vec![]),
        (vec![], vec![object, object]),
        (vec![], vec![CanvasObjectId(u64::MAX)]),
        (vec![sheet.root], vec![]),
    ] {
        assert!(sheet.set_zone_members(two, roots, objects).is_err());
        assert_eq!(sheet, before);
    }
    let mut invalid = sheet.clone();
    invalid.zones[1].objects.push(object);
    assert!(invalid.validate().is_err());
    invalid.zones[1].objects = vec![CanvasObjectId(u64::MAX)];
    assert!(invalid.validate().is_err());
    sheet.remove_canvas_objects(&[object]).unwrap();
    assert!(sheet.zones.iter().all(|zone| zone.objects.is_empty()));
    sheet.validate().unwrap();
}

#[test]
fn zone_movement_moves_explicit_objects_and_groups_with_atomic_lock_failure() {
    let (mut sheet, root, _, card) = board();
    let object = sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            rect(0.0, 0.0, 80.0, 60.0),
        ))
        .unwrap();
    sheet
        .group_board_members(&[BoardMember::Object(object), BoardMember::Card(card)])
        .unwrap();
    let zone = sheet
        .create_zone("Mixed", vec![root], rect(0.0, 0.0, 200.0, 200.0))
        .unwrap();
    let ink = sheet
        .add_canvas_object(CanvasObject::ink(stroke(&[(10.0, 20.0), (30.0, 40.0)])))
        .unwrap();
    let connector = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Point(Vec2::new(15.0, 25.0)),
            Endpoint::Topic(root),
        )))
        .unwrap();
    // Partial explicit group membership still uses the shared rigid group rules.
    sheet
        .set_zone_members(zone, vec![root], vec![object, ink, connector])
        .unwrap();
    sheet.canvas.objects[0].locked = true;
    let before = sheet.clone();
    let delta = Vec2::new(30.0, 50.0);
    assert!(sheet.move_zone(zone, delta).is_err());
    assert_eq!(sheet, before);
    sheet.canvas.objects[0].locked = false;
    sheet.move_zone(zone, delta).unwrap();
    assert_eq!(
        sheet.canvas.objects[0].rect,
        before.canvas.objects[0].rect.translate(delta)
    );
    for topic in [root, card] {
        assert_eq!(
            sheet.topic(topic).unwrap().position,
            before
                .topic(topic)
                .unwrap()
                .position
                .map(|position| position + delta)
        );
    }
    assert_eq!(sheet.zones[0].frame, before.zones[0].frame.translate(delta));
    let CanvasObjectKind::Ink(actual) = &sheet.canvas.object(ink).unwrap().kind else {
        panic!("ink");
    };
    assert_eq!(
        actual.points.as_ref(),
        &[Vec2::new(10.0, 20.0) + delta, Vec2::new(30.0, 40.0) + delta]
    );
    let CanvasObjectKind::Connector(actual) = &sheet.canvas.object(connector).unwrap().kind else {
        panic!("connector");
    };
    assert_eq!(actual.from, Endpoint::Point(Vec2::new(15.0, 25.0) + delta));
    assert_eq!(actual.to, Endpoint::Topic(root));
    let before = sheet.clone();
    assert!(
        sheet
            .move_zone(zone, Vec2::new(MAX_BOARD_COORDINATE, 0.0))
            .is_err()
    );
    assert_eq!(sheet, before);
    sheet.validate().unwrap();
}

#[test]
fn zone_objects_use_explicit_clipping_and_source_slide_membership() {
    let (mut sheet, root, _, _) = board();
    let frame = rect(0.0, 0.0, 200.0, 200.0);
    let outside = sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            rect(500.0, 500.0, 80.0, 60.0),
        ))
        .unwrap();
    let spatial = sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            rect(20.0, 20.0, 80.0, 60.0),
        ))
        .unwrap();
    let zone = sheet.create_zone("Mixed", vec![root], frame).unwrap();
    sheet
        .set_zone_members(zone, vec![root], vec![outside])
        .unwrap();
    sheet.zones[0].clip = true;
    assert_eq!(
        crate::scene::object_clip_rect(&sheet, sheet.canvas.object(outside).unwrap()),
        Some(frame)
    );
    assert_eq!(
        crate::scene::object_clip_rect(&sheet, sheet.canvas.object(spatial).unwrap()),
        Some(frame)
    );
    let layout = crate::layout::layout_sheet(&sheet, &crate::layout::ApproxMeasure, false);
    let slide_id = sheet
        .add_presentation_slide("Slide", PresentationSource::Zone(zone), frame)
        .unwrap();
    sheet.presentation.slides[0].scope = crate::PresentationScope::Source;
    let resolved =
        crate::resolve_presentation_slide(&sheet, &sheet.presentation.slides[0], &layout).unwrap();
    assert_eq!(sheet.presentation.slides[0].id, slide_id);
    assert_eq!(resolved.permitted_objects, HashSet::from([outside]));
    sheet.zones[0].clip = false;
    assert_eq!(
        crate::scene::object_clip_rect(&sheet, sheet.canvas.object(outside).unwrap()),
        None
    );
    let payload = crate::clipboard::copy_board_selection(&sheet, &layout, &[root], &[outside])
        .unwrap()
        .unwrap();
    let pasted =
        crate::clipboard::paste_board_selection(&mut sheet, &payload, Vec2::new(10.0, 10.0))
            .unwrap();
    assert_eq!(sheet.zones[0].objects, vec![outside]);
    assert!(!sheet.zones[0].objects.contains(&pasted.objects[0]));
    sheet.validate().unwrap();
}

#[test]
fn legacy_sheet_json_round_trips_byte_identically() {
    let mut s = Sheet::new("Map", "Central");
    let a = s.add_child(s.root, None, "A");
    let f = s.add_floating("F", Vec2::new(10.0, 20.0));
    s.create_zone("Zone", vec![f], rect(0.0, 0.0, 100.0, 100.0))
        .unwrap();
    s.zones[0].color = None;
    s.add_presentation_slide(
        "Slide",
        PresentationSource::Branch(a),
        rect(0.0, 0.0, 100.0, 80.0),
    )
    .unwrap();
    let json = serde_json::to_string(&s).unwrap();
    for key in [
        "root_mode",
        "canvas",
        "\"board\"",
        "\"card\"",
        "\"clip\"",
        "\"color\":null",
    ] {
        assert!(
            !json.contains(key),
            "{key} leaked into legacy output: {json}"
        );
    }
    let back: Sheet = serde_json::from_str(&json).unwrap();
    assert_eq!(back, s);
    assert_eq!(serde_json::to_string(&back).unwrap(), json);
}

#[test]
fn old_json_without_board_fields_loads_with_defaults() {
    let mut value = serde_json::to_value(Sheet::new("Old", "Root")).unwrap();
    let object = value.as_object_mut().unwrap();
    object.remove("root_mode");
    object.remove("canvas");
    object.remove("board");
    let s: Sheet = serde_json::from_value(value).unwrap();
    assert_eq!(s.root_mode, SheetRootMode::Central);
    assert!(s.canvas.is_empty());
    assert!(s.board.is_default());
    assert!(s.root_topic().card.is_none());
    s.validate().unwrap();
}

#[test]
fn newly_created_zones_cycle_stable_accent_colors() {
    let mut sheet = Sheet::new("Board", "Root");
    let frame = rect(0.0, 0.0, 100.0, 100.0);
    sheet.create_board_frame("First", frame).unwrap();
    sheet.create_board_frame("Second", frame).unwrap();

    assert_eq!(sheet.zones[0].color, Some(crate::automatic_zone_color(0)));
    assert_eq!(sheet.zones[1].color, Some(crate::automatic_zone_color(1)));
    assert_ne!(sheet.zones[0].color, sheet.zones[1].color);
    assert!(
        sheet
            .zones
            .iter()
            .all(|zone| zone.background_color.is_none())
    );
}

#[test]
fn old_zone_json_defaults_independent_background_color() {
    let mut sheet = Sheet::new("Board", "Root");
    sheet
        .create_board_frame("Frame", rect(0.0, 0.0, 100.0, 100.0))
        .unwrap();
    sheet.zones[0].background_color = Some(Color::rgb(10, 20, 30));
    let mut value = serde_json::to_value(&sheet).unwrap();
    value["zones"][0]
        .as_object_mut()
        .unwrap()
        .remove("background_color");

    let back: Sheet = serde_json::from_value(value).unwrap();
    assert_eq!(back.zones[0].background_color, None);
    assert_eq!(back.zones[0].color, sheet.zones[0].color);
}

#[test]
fn board_sheet_round_trips() {
    let (mut s, a, _, card) = board();
    s.board = BoardSettings {
        grid: GridKind::Dots,
        grid_size: 32.0,
        snap: true,
    };
    s.zones.clear();
    let zone = s
        .create_zone("Frame", vec![a], rect(200.0, 100.0, 300.0, 300.0))
        .unwrap();
    s.zones[0].color = Some(Color::rgb(200, 220, 255));
    s.zones[0].background_color = Some(Color::rgb(240, 230, 220));
    s.zones[0].clip = true;
    let shape = s
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Diamond,
            rect(0.0, 0.0, 40.0, 30.0),
        ))
        .unwrap();
    let mut ink = stroke(&[(0.0, 0.0), (10.0, 5.0), (20.0, 0.0)]);
    ink.pressure = Some(vec![0.2, 0.5, 1.0].into());
    let ink = s.add_canvas_object(CanvasObject::ink(ink)).unwrap();
    let conn = s
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(shape),
            Endpoint::Topic(card),
        )))
        .unwrap();
    s.group_canvas_objects(&[shape, ink]).unwrap();
    s.rotate_canvas_object(shape, 30.0).unwrap();
    s.add_presentation_slide(
        "View",
        PresentationSource::Viewport,
        rect(0.0, 0.0, 400.0, 300.0),
    )
    .unwrap();
    s.validate().unwrap();
    let json = serde_json::to_string(&s).unwrap();
    assert!(json.contains("\"root_mode\":\"hidden\""));
    assert!(json.contains("\"viewport\""));
    let back: Sheet = serde_json::from_str(&json).unwrap();
    assert_eq!(back, s);
    assert_eq!(back.canvas.object(conn).unwrap().label(), "");
    assert_eq!(back.zones[0].id, zone);
    back.validate().unwrap();
}

#[test]
fn every_shape_kind_round_trips_and_has_geometry() {
    let mut ids = HashSet::new();
    for kind in ShapeKind::ALL {
        assert!(ids.insert(kind.id()), "duplicate id {}", kind.id());
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(serde_json::from_str::<ShapeKind>(&json).unwrap(), kind);
        let outline = kind.unit_outline();
        assert!(outline.len() >= 3, "{kind:?}");
        for p in outline.iter().chain(kind.unit_details().iter().flatten()) {
            assert!(
                p.x.is_finite() && p.y.is_finite() && (-0.01..=1.01).contains(&p.x),
                "{kind:?} {p:?}"
            );
            assert!((-0.01..=1.01).contains(&p.y), "{kind:?} {p:?}");
        }
        let label = kind.unit_label_rect();
        assert!(label.size.x > 0.0 && label.size.y > 0.0, "{kind:?}");
        let frame = rect(100.0, 50.0, 80.0, 40.0);
        let g = shape_geometry(kind, frame, 0.0);
        let bounds = points_bounds(&g.outline).unwrap();
        assert!(frame.expand(0.5).contains(bounds.min), "{kind:?}");
        assert!(frame.expand(0.5).contains(bounds.max()), "{kind:?}");
        let rotated = shape_geometry(kind, frame, 90.0);
        let rb = points_bounds(&rotated.outline).unwrap();
        assert!(
            rotated_bounds(frame, 90.0).expand(0.5).contains(rb.min),
            "{kind:?}"
        );
    }
}

// ---- ids --------------------------------------------------------------------------------

#[test]
fn allocator_is_unique_across_topics_objects_groups_and_slides() {
    let mut s = Sheet::new("Ids", "Root");
    let a = s.add_child(s.root, None, "A");
    let x = s
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            rect(0.0, 0.0, 10.0, 10.0),
        ))
        .unwrap();
    let y = s
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Ellipse,
            rect(20.0, 0.0, 10.0, 10.0),
        ))
        .unwrap();
    let g = s.group_canvas_objects(&[x, y]).unwrap();
    let t = s.add_floating("F", Vec2::ZERO);
    let mut all = vec![s.root.0, a.0, x.0, y.0, g.0, t.0];
    all.sort_unstable();
    all.dedup();
    assert_eq!(all.len(), 6);

    // Loaded data with high ids: the next allocation must exceed every one of them.
    s.presentation.slides.clear();
    s.add_presentation_slide(
        "S",
        PresentationSource::Branch(a),
        rect(0.0, 0.0, 10.0, 10.0),
    )
    .unwrap();
    s.presentation.slides[0].id = SlideId(5_000);
    s.canvas.objects[0].id = CanvasObjectId(4_000);
    s.canvas.groups[0].id = CanvasGroupId(4_500);
    for o in &mut s.canvas.objects {
        o.group = Some(CanvasGroupId(4_500));
    }
    let json = serde_json::to_string(&s).unwrap();
    let mut loaded: Sheet = serde_json::from_str(&json).unwrap();
    assert!(loaded.alloc_canvas_object_id().0 > 5_000);
    assert!(loaded.alloc_topic_id().0 > 5_000);
    let mut loaded: Sheet = serde_json::from_str(&json).unwrap();
    loaded.presentation.slides.clear();
    assert!(loaded.alloc_canvas_group_id().0 > 4_500);
}

// ---- root semantics --------------------------------------------------------------------

#[test]
fn root_queries_follow_root_mode() {
    let mut s = Sheet::new("Q", "Root");
    let a = s.add_child(s.root, None, "A");
    let f = s.add_floating("F", Vec2::new(0.0, 300.0));
    assert_eq!(s.semantic_roots(), vec![s.root]);
    assert_eq!(s.primary_content_root(), Some(s.root));
    assert_eq!(s.all_content_roots(), vec![s.root, f]);
    assert!(!s.is_anchor(s.root));
    assert!(s.visible_topic_ids().contains(&s.root));

    s.detach_subtree(a).unwrap();
    s.topic_mut(s.root).unwrap().title.clear();
    s.root_mode = SheetRootMode::Hidden;
    s.validate().unwrap();
    let late = s.add_floating("Top", Vec2::new(500.0, -100.0));
    let zoned = s.add_floating("Zoned", Vec2::new(0.0, 900.0));
    s.create_zone("Z", vec![zoned], rect(-100.0, 800.0, 300.0, 300.0))
        .unwrap();
    assert!(s.is_anchor(s.root));
    // Zone members first, then the rest by y then x.
    assert_eq!(s.semantic_roots(), vec![zoned, late, f]);
    assert_eq!(s.content_top_level_ids(), vec![zoned, late, f]);
    assert_eq!(s.primary_content_root(), Some(zoned));
    assert!(!s.all_content_roots().contains(&s.root));
    assert!(!s.visible_topic_ids().contains(&s.root));

    let empty = {
        let mut e = Sheet::new("E", "");
        e.root_mode = SheetRootMode::Hidden;
        e
    };
    assert_eq!(empty.primary_content_root(), None);
    assert!(empty.semantic_roots().is_empty());
}

#[test]
fn hidden_anchor_cannot_carry_non_title_content() {
    let make = || {
        let mut s = Sheet::new("V", "");
        s.root_mode = SheetRootMode::Hidden;
        s
    };
    let mut s = make();
    s.add_child(s.root, None, "Child");
    assert!(s.validate().unwrap_err().contains("children"));

    let mut s = make();
    s.topic_mut(s.root).unwrap().title = "Legacy title".into();
    s.topic_mut(s.root).unwrap().labels.push("x".into());
    assert!(s.validate().unwrap_err().contains("labels"));

    let mut s = make();
    s.topic_mut(s.root).unwrap().hyperlink = Some("https://example.com".into());
    assert!(s.validate().unwrap_err().contains("hyperlink"));

    let mut s = make();
    let f = s.add_floating("F", Vec2::ZERO);
    let id = s.alloc_relationship_id();
    s.relationships.push(Relationship {
        id,
        from: s.root,
        to: f,
        title: String::new(),
        color: None,
        control_points: None,
    });
    assert!(s.validate().unwrap_err().contains("relationships"));

    let mut s = make();
    s.presentation.slides.push(crate::model::PresentationSlide {
        id: SlideId(1),
        title: "S".into(),
        source: PresentationSource::Branch(s.root),
        frame: rect(0.0, 0.0, 10.0, 10.0),
        scope: crate::model::PresentationScope::Source,
        auto_frame: true,
        hidden_topics: Vec::new(),
        max_depth: None,
    });
    assert!(s.validate().is_err());

    // Connectors may not attach to the anchor.
    let mut s = make();
    s.canvas.objects.push(CanvasObject {
        id: CanvasObjectId(99),
        ..CanvasObject::connector(CanvasConnector::new(
            Endpoint::Topic(s.root),
            Endpoint::Point(Vec2::ZERO),
        ))
    });
    assert!(s.validate().unwrap_err().contains("connectors"));
}

#[test]
fn legacy_hidden_anchor_title_is_preserved_but_not_editable_or_content() {
    let mut sheet = Sheet::new("Legacy board", "Legacy title");
    sheet.root_mode = SheetRootMode::Hidden;
    let anchor = sheet.root;
    let content = sheet.add_floating("Visible", Vec2::ZERO);
    sheet.validate().unwrap();
    assert_eq!(sheet.content_ids_ordered(), vec![content]);
    assert!(
        crate::clipboard::copy_topics(&sheet, &[anchor])
            .unwrap()
            .is_none()
    );
    assert_eq!(
        sheet.set_title(anchor, "Changed"),
        Err(crate::EditError::RootNotAllowed)
    );
    assert_eq!(
        sheet.insert_child(anchor, "Child"),
        Err(crate::EditError::RootNotAllowed)
    );
    let json = serde_json::to_string(&sheet).unwrap();
    let mut restored: Sheet = serde_json::from_str(&json).unwrap();
    restored.validate().unwrap();
    assert_eq!(restored.root_topic().title, "Legacy title");
    restored.show_central_topic();
    assert_eq!(restored.root_topic().title, "Legacy title");
}

#[test]
fn hide_central_topic_moves_content_in_one_undo_step() {
    let mut s = Sheet::new("Plan", "Central");
    let a = s.add_child(s.root, None, "A");
    s.add_child(a, None, "A1");
    let b = s.add_child(s.root, None, "B");
    s.topic_mut(s.root).unwrap().labels.push("tag".into());
    s.add_presentation_slide(
        "A",
        PresentationSource::Branch(a),
        rect(0.0, 0.0, 100.0, 80.0),
    )
    .unwrap();
    let conn = s
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Topic(s.root),
            Endpoint::Point(Vec2::new(500.0, 0.0)),
        )))
        .unwrap();
    let root = s.root;
    let mut doc = Document::new(crate::Workbook {
        sheets: vec![s],
        ..Default::default()
    });
    let before = doc.sheet().clone();
    let moved = doc.hide_central_topic().unwrap().expect("content moved");
    let s = doc.sheet();
    assert_eq!(s.root_mode, SheetRootMode::Hidden);
    assert!(s.root_topic().children.is_empty());
    assert!(s.root_topic().title.is_empty());
    assert_eq!(s.topic(moved).unwrap().title, "Central");
    assert_eq!(s.children(moved), &[a, b]);
    assert_eq!(s.topic(moved).unwrap().labels, vec!["tag".to_string()]);
    assert_eq!(s.parent(a), Some(moved));
    assert!(s.floating.contains(&moved));
    assert_eq!(s.semantic_roots(), vec![moved]);
    assert_eq!(
        s.presentation.slides[0].source,
        PresentationSource::Branch(a)
    );
    assert!(s.is_presentation_branch(moved));
    let mut shown = s.clone();
    shown.show_central_topic();
    shown.validate().unwrap();
    shown
        .add_presentation_slide(
            "Moved root",
            PresentationSource::Branch(moved),
            rect(0.0, 0.0, 100.0, 80.0),
        )
        .unwrap();
    match &s.canvas.object(conn).unwrap().kind {
        CanvasObjectKind::Connector(c) => assert_eq!(c.from, Endpoint::Topic(moved)),
        _ => unreachable!(),
    }
    s.validate().unwrap();
    // The moved tree is laid out where the central topic was.
    let layout = crate::layout::layout_sheet(s, &crate::layout::ApproxMeasure, false);
    assert!(layout.rect(root).is_none());
    assert!(layout.rect(moved).unwrap().contains(Vec2::ZERO));

    assert!(doc.undo());
    assert_eq!(doc.sheet(), &before);
    assert!(!doc.can_undo());

    doc.redo();
    doc.show_central_topic().unwrap();
    assert_eq!(doc.sheet().root_mode, SheetRootMode::Central);
    doc.sheet().validate().unwrap();
}

#[test]
fn hide_empty_central_topic_only_switches_mode() {
    let mut s = Sheet::new("Empty", "");
    let f = s.add_floating("F", Vec2::ZERO);
    assert_eq!(s.central_topic_content(), None);
    assert_eq!(s.hide_central_topic().unwrap(), None);
    assert_eq!(s.root_mode, SheetRootMode::Hidden);
    assert_eq!(s.semantic_roots(), vec![f]);
    assert_eq!(s.hide_central_topic().unwrap(), None);
    s.validate().unwrap();
}

#[test]
fn hiding_an_untitled_central_topic_preserves_attached_connectors() {
    let mut sheet = Sheet::new("Board", "");
    let connector = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Topic(sheet.root),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        )))
        .unwrap();
    assert_eq!(sheet.central_topic_content(), Some("connectors"));
    let moved = sheet.hide_central_topic().unwrap().unwrap();
    match &sheet.canvas.object(connector).unwrap().kind {
        CanvasObjectKind::Connector(connector) => {
            assert_eq!(connector.from, Endpoint::Topic(moved));
        }
        _ => unreachable!(),
    }
    sheet.validate().unwrap();
}

#[test]
fn hiding_a_title_only_central_topic_preserves_it_as_a_floating_root() {
    let mut s = Sheet::new("Board", "Important");
    let moved = s.hide_central_topic().unwrap().expect("title preserved");
    assert_eq!(s.topic(moved).unwrap().title, "Important");
    assert!(s.root_topic().title.is_empty());
    assert_eq!(s.semantic_roots(), vec![moved]);
    s.validate().unwrap();
}

#[test]
fn adding_content_to_the_anchor_is_rejected_by_document_edits() {
    let (s, ..) = board();
    let root = s.root;
    let mut doc = Document::new(crate::Workbook {
        sheets: vec![s],
        ..Default::default()
    });
    let before = doc.sheet().clone();
    assert!(
        doc.edit_sheet("Add", |s| {
            s.add_child(root, None, "child");
            Ok(())
        })
        .is_err()
    );
    assert_eq!(doc.sheet(), &before);
    assert!(!doc.can_undo());
}

// ---- cards ------------------------------------------------------------------------------

#[test]
fn cards_only_on_floating_roots() {
    let (mut s, a, _, card) = board();
    let child = s.add_child(a, None, "child");
    let c = Card::new(CardKind::Text, Vec2::new(100.0, 60.0));
    assert!(s.set_topic_card(child, Some(c)).is_err());
    assert!(s.set_topic_card(s.root, Some(c)).is_err());
    assert!(
        s.set_topic_card(a, Some(Card::new(CardKind::Text, Vec2::new(4.0, 60.0))))
            .is_err()
    );
    s.set_topic_card(a, Some(c)).unwrap();
    s.validate().unwrap();
    // Attaching a card below another topic turns it back into a plain topic.
    s.move_topic(card, a, 0).unwrap();
    assert!(s.topic(card).unwrap().card.is_none());
    s.validate().unwrap();
    s.topic_mut(child).unwrap().card = Some(Card::new(CardKind::Sticky, Vec2::new(50.0, 50.0)));
    assert!(s.validate().unwrap_err().contains("not a floating root"));
}

#[test]
fn card_topics_have_fixed_size_and_wrap_text() {
    let mut s = Sheet::new("Cards", "Root");
    let long = "word ".repeat(80);
    let card = s
        .add_card(
            &long,
            Vec2::new(0.0, 500.0),
            Card::new(CardKind::Sticky, Vec2::new(180.0, 140.0)),
        )
        .unwrap();
    let plain = s.add_floating(long.as_str(), Vec2::new(0.0, -500.0));
    let layout = crate::layout::layout_sheet(&s, &crate::layout::ApproxMeasure, false);
    let r = layout.rect(card).unwrap();
    assert_eq!(r.size, Vec2::new(180.0, 140.0));
    assert!((r.center() - Vec2::new(0.0, 500.0)).length() < 1.0);
    let title = layout.topics[&card].title_rect;
    assert!(title.size.x <= 180.0, "{title:?}");
    assert!(layout.rect(plain).unwrap().size.x > 180.0);
}

// ---- limits -----------------------------------------------------------------------------

#[test]
fn limits_reject_out_of_range_objects() {
    let mut s = Sheet::new("Limits", "Root");
    let bad = [
        CanvasObject::shape(ShapeKind::Rectangle, rect(f32::NAN, 0.0, 10.0, 10.0)),
        CanvasObject::shape(ShapeKind::Rectangle, rect(2.0e6, 0.0, 10.0, 10.0)),
        CanvasObject::shape(ShapeKind::Rectangle, rect(0.0, 0.0, -1.0, 10.0)),
        CanvasObject {
            alt_text: "x".repeat(MAX_BOARD_TEXT_CHARS + 1),
            ..CanvasObject::shape(ShapeKind::Rectangle, rect(0.0, 0.0, 10.0, 10.0))
        },
        CanvasObject {
            style: ObjectStyle {
                opacity: 1.5,
                ..Default::default()
            },
            ..CanvasObject::shape(ShapeKind::Rectangle, rect(0.0, 0.0, 10.0, 10.0))
        },
        CanvasObject {
            style: ObjectStyle {
                stroke_width: 500.0,
                ..Default::default()
            },
            ..CanvasObject::shape(ShapeKind::Rectangle, rect(0.0, 0.0, 10.0, 10.0))
        },
        CanvasObject::new(
            CanvasObjectKind::Shape {
                shape: ShapeKind::Rectangle,
                label: "y".repeat(MAX_BOARD_TEXT_CHARS + 1),
            },
            rect(0.0, 0.0, 10.0, 10.0),
        ),
        CanvasObject::ink(stroke(&[])),
        CanvasObject::ink(InkStroke {
            width: 0.01,
            ..stroke(&[(0.0, 0.0), (1.0, 1.0)])
        }),
        CanvasObject::ink(InkStroke::new(
            InkTool::Pen,
            Color::rgb(0, 0, 0),
            2.0,
            vec![Vec2::ZERO; MAX_STROKE_POINTS + 1],
        )),
        CanvasObject::ink(InkStroke {
            pressure: Some(vec![0.5].into()),
            ..stroke(&[(0.0, 0.0), (1.0, 1.0)])
        }),
        CanvasObject::connector(CanvasConnector::new(
            Endpoint::Topic(TopicId(12345)),
            Endpoint::Point(Vec2::ZERO),
        )),
        CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(CanvasObjectId(777)),
            Endpoint::Point(Vec2::ZERO),
        )),
        CanvasObject::connector(CanvasConnector::new(
            Endpoint::Point(Vec2::new(f32::INFINITY, 0.0)),
            Endpoint::Point(Vec2::ZERO),
        )),
    ];
    for object in bad {
        assert!(s.add_canvas_object(object.clone()).is_err(), "{object:?}");
    }
    assert!(s.canvas.is_empty());
    assert!(
        s.set_board_settings(BoardSettings {
            grid_size: 1.0,
            ..Default::default()
        })
        .is_err()
    );
    s.validate().unwrap();
}

#[test]
fn limits_reject_invalid_imported_boards() {
    let mut s = Sheet::new("Import", "Root");
    let id = s
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            rect(0.0, 0.0, 10.0, 10.0),
        ))
        .unwrap();
    s.validate().unwrap();

    let mut dup = s.clone();
    dup.canvas.objects.push(dup.canvas.objects[0].clone());
    assert!(dup.validate().unwrap_err().contains("duplicate"));

    let mut group = s.clone();
    group.canvas.objects[0].group = Some(CanvasGroupId(999));
    assert!(group.validate().unwrap_err().contains("missing group"));

    let mut many = s.clone();
    many.canvas.objects = (0..=MAX_CANVAS_OBJECTS as u64)
        .map(|i| CanvasObject {
            id: CanvasObjectId(10_000 + i),
            ..many.canvas.objects[0].clone()
        })
        .collect();
    assert!(many.validate().unwrap_err().contains("objects"));

    let mut ink = s.clone();
    ink.canvas.objects = (0..=(MAX_TOTAL_INK_POINTS / MAX_STROKE_POINTS) as u64)
        .map(|i| CanvasObject {
            id: CanvasObjectId(10_000 + i),
            ..CanvasObject::ink(InkStroke::new(
                InkTool::Pen,
                Color::rgb(0, 0, 0),
                2.0,
                vec![Vec2::ZERO; MAX_STROKE_POINTS],
            ))
        })
        .collect();
    assert!(ink.validate().unwrap_err().contains("ink exceeds"));

    let mut grid = s.clone();
    grid.board.grid_size = f32::NAN;
    assert!(grid.validate().is_err());

    let mut rot = s.clone();
    rot.canvas.object_mut(id).unwrap().rotation = 400.0;
    assert!(rot.validate().is_err());

    // Paste/import JSON with bad data is rejected by validation, not by a panic.
    let mut json = serde_json::to_value(&s).unwrap();
    json["canvas"]["objects"][0]["rect"]["size"]["x"] = serde_json::json!(-5.0);
    let parsed: Sheet = serde_json::from_value(json).unwrap();
    assert!(parsed.validate().is_err());
}

#[test]
fn total_ink_limit_applies_to_add_and_update() {
    let mut s = Sheet::new("Ink", "Root");
    let full = vec![Vec2::ZERO; MAX_STROKE_POINTS];
    for _ in 0..(MAX_TOTAL_INK_POINTS / MAX_STROKE_POINTS) {
        s.add_canvas_object(CanvasObject::ink(InkStroke::new(
            InkTool::Pen,
            Color::rgb(0, 0, 0),
            2.0,
            full.clone(),
        )))
        .unwrap();
    }
    assert!(
        s.add_canvas_object(CanvasObject::ink(stroke(&[(0.0, 0.0)])))
            .is_err()
    );
    let first = s.canvas.objects[0].id;
    s.update_canvas_object(first, |o| {
        if let CanvasObjectKind::Ink(ink) = &mut o.kind {
            ink.points = vec![Vec2::ZERO; 10].into();
        }
    })
    .unwrap();
    s.add_canvas_object(CanvasObject::ink(stroke(&[(0.0, 0.0)])))
        .unwrap();
    s.validate().unwrap();
}

// ---- object edits -----------------------------------------------------------------------

fn three_shapes() -> (Sheet, [CanvasObjectId; 3]) {
    let mut s = Sheet::new("Z", "Root");
    let ids = [0.0, 50.0, 100.0].map(|x| {
        s.add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            rect(x, 0.0, 40.0, 40.0),
        ))
        .unwrap()
    });
    (s, ids)
}

fn order(s: &Sheet) -> Vec<CanvasObjectId> {
    s.canvas.objects.iter().map(|o| o.id).collect()
}

#[test]
fn z_order_within_bands() {
    let (mut s, [a, b, c]) = three_shapes();
    s.reorder_canvas_objects(&[a], ZOrder::Forward).unwrap();
    assert_eq!(order(&s), vec![b, a, c]);
    s.reorder_canvas_objects(&[a], ZOrder::Front).unwrap();
    assert_eq!(order(&s), vec![b, c, a]);
    s.reorder_canvas_objects(&[a], ZOrder::Forward).unwrap();
    assert_eq!(order(&s), vec![b, c, a]);
    s.reorder_canvas_objects(&[c, a], ZOrder::Back).unwrap();
    assert_eq!(order(&s), vec![c, a, b]);
    s.reorder_canvas_objects(&[b], ZOrder::Backward).unwrap();
    assert_eq!(order(&s), vec![c, b, a]);
    assert!(
        s.reorder_canvas_objects(&[CanvasObjectId(999)], ZOrder::Front)
            .is_err()
    );

    // Moving to the Below band places objects just behind the map; band order is
    // independent of Vec order across bands.
    s.set_canvas_band(&[a], Band::Below).unwrap();
    assert_eq!(s.canvas.object(a).unwrap().band, Band::Below);
    s.set_canvas_band(&[b], Band::Below).unwrap();
    let below: Vec<_> = s
        .canvas
        .objects
        .iter()
        .filter(|o| o.band == Band::Below)
        .map(|o| o.id)
        .collect();
    assert_eq!(below, vec![a, b]);
    s.set_canvas_band(&[a], Band::Above).unwrap();
    let above: Vec<_> = s
        .canvas
        .objects
        .iter()
        .filter(|o| o.band == Band::Above)
        .map(|o| o.id)
        .collect();
    assert_eq!(above, vec![a, c]);
    // Forward within the Below band never crosses into Above.
    s.reorder_canvas_objects(&[b], ZOrder::Forward).unwrap();
    assert_eq!(s.canvas.object(b).unwrap().band, Band::Below);
}

#[test]
fn group_ungroup_and_lock() {
    let (mut s, [a, b, c]) = three_shapes();
    assert!(s.group_canvas_objects(&[a]).is_err());
    let g = s.group_canvas_objects(&[a, b]).unwrap();
    assert_eq!(s.canvas.group_members(g), vec![a, b]);
    let mut expanded = s.canvas.expand_to_groups(&[a]);
    expanded.sort();
    assert_eq!(expanded, vec![a, b]);
    // Regrouping one member with another object leaves a one-member group, which is
    // dissolved.
    let g2 = s.group_canvas_objects(&[b, c]).unwrap();
    assert!(s.canvas.group(g).is_none());
    assert_eq!(s.canvas.object(a).unwrap().group, None);
    s.validate().unwrap();

    s.set_canvas_group_locked(g2, true).unwrap();
    assert!(s.canvas.is_locked(b));
    assert!(s.move_canvas_objects(&[b], Vec2::new(5.0, 0.0)).is_err());
    assert!(s.remove_canvas_objects(&[c]).is_err());
    assert!(s.rotate_canvas_object(c, 10.0).is_err());
    assert!(s.resize_canvas_object(c, rect(0.0, 0.0, 5.0, 5.0)).is_err());
    let locked = s.clone();
    assert!(
        s.update_canvas_object(b, |o| o.alt_text = "edit".into())
            .is_err()
    );
    assert!(s.reorder_canvas_objects(&[b], ZOrder::Front).is_err());
    assert!(s.set_canvas_band(&[b], Band::Below).is_err());
    assert!(s.group_canvas_objects(&[a, b]).is_err());
    assert!(s.ungroup_canvas_objects(g2).is_err());
    assert_eq!(s, locked);
    s.set_canvas_group_locked(g2, false).unwrap();
    s.set_canvas_objects_locked(&[a], true).unwrap();
    assert!(s.move_canvas_objects(&[a], Vec2::new(5.0, 0.0)).is_err());
    s.set_canvas_objects_locked(&[a], false).unwrap();
    s.move_canvas_objects(&[a, b], Vec2::new(5.0, 7.0)).unwrap();
    assert_eq!(s.canvas.object(a).unwrap().rect.min, Vec2::new(5.0, 7.0));

    s.ungroup_canvas_objects(g2).unwrap();
    assert!(s.canvas.groups.is_empty());
    assert!(s.canvas.objects.iter().all(|o| o.group.is_none()));
    assert!(s.ungroup_canvas_objects(g2).is_err());
    s.validate().unwrap();

    let g3 = s.group_canvas_objects(&[a, b]).unwrap();
    s.remove_canvas_objects(&[a]).unwrap();
    assert!(
        s.canvas.group(g3).is_none(),
        "one-member group is dissolved"
    );
    s.validate().unwrap();
}

#[test]
fn move_resize_rotate_objects() {
    let mut s = Sheet::new("Edit", "Root");
    let ink = s
        .add_canvas_object(CanvasObject::ink(stroke(&[(0.0, 0.0), (10.0, 20.0)])))
        .unwrap();
    s.move_canvas_objects(&[ink], Vec2::new(10.0, 10.0))
        .unwrap();
    let o = s.canvas.object(ink).unwrap();
    assert_eq!(o.rect, rect(10.0, 10.0, 10.0, 20.0));
    match &o.kind {
        CanvasObjectKind::Ink(i) => assert_eq!(i.points[1], Vec2::new(20.0, 30.0)),
        _ => unreachable!(),
    }
    s.resize_canvas_object(ink, rect(10.0, 10.0, 20.0, 40.0))
        .unwrap();
    match &s.canvas.object(ink).unwrap().kind {
        CanvasObjectKind::Ink(i) => assert_eq!(i.points[1], Vec2::new(30.0, 50.0)),
        _ => unreachable!(),
    }
    s.rotate_canvas_object(ink, -90.0).unwrap();
    assert_eq!(s.canvas.object(ink).unwrap().rotation, 270.0);
    assert!(s.rotate_canvas_object(ink, f32::NAN).is_err());
    assert!(
        s.move_canvas_objects(&[ink], Vec2::new(5.0e6, 0.0))
            .is_err()
    );
    assert!(
        s.resize_canvas_object(ink, rect(0.0, 0.0, -2.0, 1.0))
            .is_err()
    );

    let conn = s
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Object(ink),
        )))
        .unwrap();
    assert!(s.rotate_canvas_object(conn, 45.0).is_err());
    s.move_canvas_objects(&[conn], Vec2::new(1.0, 2.0)).unwrap();
    match &s.canvas.object(conn).unwrap().kind {
        CanvasObjectKind::Connector(c) => {
            assert_eq!(c.from, Endpoint::Point(Vec2::new(1.0, 2.0)));
            assert_eq!(c.to, Endpoint::Object(ink));
        }
        _ => unreachable!(),
    }
    s.update_canvas_object(conn, |o| {
        o.id = CanvasObjectId(1);
        o.locked = true;
        if let CanvasObjectKind::Connector(c) = &mut o.kind {
            c.label = "flows".into();
            c.route = ConnectorRoute::Elbow;
        }
    })
    .unwrap();
    let o = s.canvas.object(conn).unwrap();
    assert_eq!(o.label(), "flows");
    assert!(!o.locked, "update cannot change lock state");
    assert!(
        s.update_canvas_object(conn, |o| o.style.opacity = 2.0)
            .is_err()
    );
    s.validate().unwrap();
}

// ---- connector endpoint fix-up ------------------------------------------------------------

#[test]
fn deleting_connector_targets_converts_ends_to_points_in_one_edit() {
    let (mut s, a, b, _) = board();
    let shape = s
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            rect(100.0, -300.0, 40.0, 20.0),
        ))
        .unwrap();
    let to_topic = s
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Topic(b),
            Endpoint::Topic(a),
        )))
        .unwrap();
    let to_shape = s
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(shape),
            Endpoint::Topic(a),
        )))
        .unwrap();
    let mut doc = Document::new(crate::Workbook {
        sheets: vec![s],
        ..Default::default()
    });
    let b_center = {
        let layout = crate::layout::layout_sheet(doc.sheet(), &crate::layout::ApproxMeasure, false);
        layout.rect(b).unwrap().center()
    };
    let before = doc.sheet().clone();
    doc.edit_sheet("Delete", |s| s.remove_topics(&[b])).unwrap();
    let ends = |doc: &Document, id| match &doc.sheet().canvas.object(id).unwrap().kind {
        CanvasObjectKind::Connector(c) => (c.from, c.to),
        _ => unreachable!(),
    };
    assert_eq!(
        ends(&doc, to_topic),
        (Endpoint::Point(b_center), Endpoint::Topic(a))
    );
    doc.sheet().validate().unwrap();
    doc.undo();
    assert_eq!(doc.sheet(), &before);
    doc.redo();

    doc.remove_canvas_objects(&[shape]).unwrap();
    assert_eq!(
        ends(&doc, to_shape),
        (
            Endpoint::Point(Vec2::new(120.0, -290.0)),
            Endpoint::Topic(a)
        )
    );
    // Deleting a topic through any path (here: a raw detach) is fixed up by the
    // document safety net instead of leaving a dangling reference.
    doc.edit_sheet("Detach", |s| {
        s.detach_subtree(a).unwrap();
        Ok(())
    })
    .unwrap();
    assert!(matches!(ends(&doc, to_shape).1, Endpoint::Point(_)));
    assert!(!doc.sheet().has_dangling_endpoints());
    doc.sheet().validate().unwrap();
}

#[test]
fn moving_a_topic_keeps_connector_endpoints() {
    let (mut s, a, b, _) = board();
    let conn = s
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Topic(b),
            Endpoint::Topic(a),
        )))
        .unwrap();
    let mut doc = Document::new(crate::Workbook {
        sheets: vec![s],
        ..Default::default()
    });
    doc.edit_sheet("Move", |s| s.move_topic(b, a, 0)).unwrap();
    match &doc.sheet().canvas.object(conn).unwrap().kind {
        CanvasObjectKind::Connector(c) => assert_eq!(c.from, Endpoint::Topic(b)),
        _ => unreachable!(),
    }
}

#[test]
fn connector_cycles_are_rejected_on_update_and_deserialized_validation() {
    let mut sheet = Sheet::new("Connectors", "Root");
    let first = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(10.0, 0.0)),
        )))
        .unwrap();
    let second = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(first),
            Endpoint::Point(Vec2::new(20.0, 0.0)),
        )))
        .unwrap();
    let third = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(second),
            Endpoint::Point(Vec2::new(30.0, 0.0)),
        )))
        .unwrap();
    sheet.validate().unwrap();
    let before = sheet.clone();
    let create_cycle = |object: &mut CanvasObject| {
        let CanvasObjectKind::Connector(connector) = &mut object.kind else {
            panic!("expected connector");
        };
        connector.from = Endpoint::Object(third);
    };
    assert!(sheet.update_canvas_object(first, create_cycle).is_err());
    assert_eq!(sheet, before);
    let mut doc = Document::new(crate::Workbook::new(sheet));
    assert!(
        doc.update_canvas_object("Connect", first, create_cycle)
            .is_err()
    );
    assert_eq!(doc.sheet(), &before);
    assert!(!doc.can_undo());

    let mut malformed = before;
    create_cycle(malformed.canvas.object_mut(first).unwrap());
    let json = serde_json::to_string(&crate::Workbook::new(malformed)).unwrap();
    let loaded: crate::Workbook = serde_json::from_str(&json).unwrap();
    assert!(validate_board(loaded.active()).is_err());
    assert!(loaded.validate().is_err());
}

// ---- masking ----------------------------------------------------------------------------

#[test]
fn masked_copy_hides_canvas_text() {
    let mut s = Sheet::new("Mask", "Root");
    let id = s
        .add_canvas_object(CanvasObject::new(
            CanvasObjectKind::Shape {
                shape: ShapeKind::Cloud,
                label: "SECRET label".into(),
            },
            rect(0.0, 0.0, 10.0, 10.0),
        ))
        .unwrap();
    s.update_canvas_object(id, |o| o.alt_text = "SECRET alt".into())
        .unwrap();
    let wb = crate::Workbook {
        sheets: vec![s],
        ..Default::default()
    };
    let json = serde_json::to_string(&wb.masked_copy()).unwrap();
    assert!(!json.contains("SECRET"));
}

// ---- geometry ---------------------------------------------------------------------------

#[test]
fn rotation_helpers() {
    assert_eq!(normalize_rotation(-90.0), 270.0);
    assert_eq!(normalize_rotation(720.0), 0.0);
    let r = rect(0.0, 0.0, 100.0, 20.0);
    let b = rotated_bounds(r, 90.0);
    assert!((b.size.x - 20.0).abs() < 1e-3 && (b.size.y - 100.0).abs() < 1e-3);
    assert!(rotated_rect_contains(r, 90.0, Vec2::new(50.0, 45.0)));
    assert!(!rotated_rect_contains(r, 90.0, Vec2::new(5.0, 10.0)));
    assert!(point_in_polygon(
        Vec2::new(1.0, 1.0),
        &[Vec2::ZERO, Vec2::new(4.0, 0.0), Vec2::new(0.0, 4.0)]
    ));
    assert!(
        (distance_to_segment(Vec2::new(5.0, 3.0), Vec2::ZERO, Vec2::new(10.0, 0.0)) - 3.0).abs()
            < 1e-5
    );
}

// ---- mixed card/object groups -----------------------------------------------------------

fn mixed_group() -> (Sheet, TopicId, CanvasObjectId, CanvasGroupId) {
    let (mut sheet, _, _, card) = board();
    let object = sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            rect(20.0, 30.0, 80.0, 40.0),
        ))
        .unwrap();
    let group = sheet
        .group_board_members(&[BoardMember::Card(card), BoardMember::Object(object)])
        .unwrap();
    (sheet, card, object, group)
}

#[test]
fn mixed_groups_serialize_and_validate_without_changing_legacy_json() {
    let (mut sheet, card, object, group) = mixed_group();
    sheet
        .set_board_members_locked(&[BoardMember::Card(card)], true)
        .unwrap();
    sheet.set_canvas_group_locked(group, true).unwrap();
    sheet.validate().unwrap();
    let json = serde_json::to_string(&crate::Workbook::new(sheet.clone())).unwrap();
    assert!(json.contains("\"card_states\""));
    let restored: crate::Workbook = serde_json::from_str(&json).unwrap();
    restored.validate().unwrap();
    assert_eq!(restored.active(), &sheet);
    assert!(
        restored
            .active()
            .is_board_member_locked(BoardMember::Object(object))
    );
    assert_eq!(serde_json::to_string(&restored).unwrap(), json);
    let clipboard = crate::clipboard::encode_sheet(&sheet).unwrap();
    assert_eq!(crate::clipboard::decode_sheet(&clipboard).unwrap(), sheet);

    let (mut legacy, [a, b, _]) = three_shapes();
    let legacy_group = legacy.group_canvas_objects(&[a, b]).unwrap();
    let card = legacy
        .add_card(
            "Legacy card",
            Vec2::ZERO,
            Card::new(CardKind::Sticky, Vec2::new(80.0, 80.0)),
        )
        .unwrap();
    let json = serde_json::to_string(&legacy).unwrap();
    assert!(!json.contains("card_states"));
    let loaded: Sheet = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded, legacy);
    assert_eq!(serde_json::to_string(&loaded).unwrap(), json);
    assert!(loaded.canvas.card_state(card).is_none());
    assert_eq!(loaded.canvas.group_members(legacy_group), vec![a, b]);

    // Object-only singleton groups were accepted by previous versions.
    legacy.canvas.object_mut(b).unwrap().group = None;
    legacy.validate().unwrap();
    let loaded: Sheet = serde_json::from_str(&serde_json::to_string(&legacy).unwrap()).unwrap();
    loaded.validate().unwrap();
}

#[test]
fn mixed_selection_expands_without_exposing_the_anchor() {
    let (mut sheet, card, object, group) = mixed_group();
    let second = sheet
        .add_card(
            "Second",
            Vec2::new(100.0, 100.0),
            Card::new(CardKind::Text, Vec2::new(100.0, 80.0)),
        )
        .unwrap();
    let previous = sheet.canvas.card_state(card).unwrap();
    assert_eq!(previous.group, Some(group));
    let group = sheet
        .group_board_members(&[
            BoardMember::Card(card),
            BoardMember::Card(second),
            BoardMember::Object(object),
        ])
        .unwrap();
    let expected = vec![
        BoardMember::Object(object),
        BoardMember::Card(card),
        BoardMember::Card(second),
    ];
    assert_eq!(sheet.board_group_members(group), expected);
    for selected in [
        vec![BoardMember::Card(card)],
        vec![BoardMember::Object(object)],
        vec![BoardMember::Card(second), BoardMember::Card(second)],
    ] {
        assert_eq!(sheet.expand_board_selection(&selected).unwrap(), expected);
    }
    assert_eq!(sheet.board_member_for_topic(sheet.root), None);
    assert!(
        sheet
            .expand_board_selection(&[BoardMember::Card(sheet.root)])
            .is_err()
    );
    let plain = sheet.add_floating("Not a card", Vec2::ZERO);
    let child = sheet.add_child(card, None, "Card child");
    for id in [plain, child, TopicId(999_999)] {
        assert_eq!(sheet.board_member_for_topic(id), None);
        assert!(
            sheet
                .group_board_members(&[BoardMember::Card(id), BoardMember::Object(object)])
                .is_err()
        );
    }
    assert!(
        sheet
            .expand_board_selection(&[BoardMember::Object(CanvasObjectId(999_999))])
            .is_err()
    );
    sheet.validate().unwrap();
}

#[test]
fn legacy_group_and_ungroup_cannot_orphan_card_members() {
    let (mut sheet, card, first, old_group) = mixed_group();
    let second = sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Ellipse,
            rect(200.0, 30.0, 80.0, 40.0),
        ))
        .unwrap();
    let third = sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Triangle,
            rect(300.0, 30.0, 80.0, 40.0),
        ))
        .unwrap();
    let mixed = sheet
        .group_board_members(&[
            BoardMember::Card(card),
            BoardMember::Object(first),
            BoardMember::Object(second),
        ])
        .unwrap();
    assert!(sheet.canvas.group(old_group).is_none());
    let objects = sheet.group_canvas_objects(&[first, second]).unwrap();
    assert!(sheet.canvas.group(mixed).is_none());
    assert!(sheet.canvas.card_state(card).is_none());
    assert_eq!(sheet.canvas.group_members(objects), vec![first, second]);
    sheet.validate().unwrap();

    let mixed = sheet
        .group_board_members(&[BoardMember::Card(card), BoardMember::Object(third)])
        .unwrap();
    sheet.ungroup_canvas_objects(mixed).unwrap();
    assert!(sheet.canvas.group(mixed).is_none());
    assert!(sheet.canvas.card_state(card).is_none());
    assert_eq!(sheet.canvas.object(third).unwrap().group, None);
    sheet.validate().unwrap();
}

#[test]
fn mixed_group_move_is_atomic_and_undoes_in_one_step() {
    let (mut sheet, card, shape, _) = mixed_group();
    let ink = sheet
        .add_canvas_object(CanvasObject::ink(stroke(&[(1.0, 2.0), (3.0, 4.0)])))
        .unwrap();
    let connector = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Point(Vec2::new(10.0, 20.0)),
            Endpoint::Object(shape),
        )))
        .unwrap();
    sheet
        .group_board_members(&[
            BoardMember::Card(card),
            BoardMember::Object(shape),
            BoardMember::Object(ink),
            BoardMember::Object(connector),
        ])
        .unwrap();
    let before = sheet.clone();
    let mut doc = Document::new(crate::Workbook::new(sheet));
    let delta = Vec2::new(50.0, -25.0);
    doc.move_board_members(
        &[
            BoardMember::Card(card),
            BoardMember::Object(shape),
            BoardMember::Card(card),
        ],
        delta,
    )
    .unwrap();
    assert_eq!(
        doc.sheet().topic(card).unwrap().position,
        Some(before.topic(card).unwrap().position.unwrap() + delta)
    );
    assert_eq!(
        doc.sheet().canvas.object(shape).unwrap().rect,
        before.canvas.object(shape).unwrap().rect.translate(delta)
    );
    let CanvasObjectKind::Ink(moved_ink) = &doc.sheet().canvas.object(ink).unwrap().kind else {
        panic!("expected ink");
    };
    assert_eq!(moved_ink.points[0], Vec2::new(51.0, -23.0));
    let CanvasObjectKind::Connector(moved_connector) =
        &doc.sheet().canvas.object(connector).unwrap().kind
    else {
        panic!("expected connector");
    };
    assert_eq!(moved_connector.from, Endpoint::Point(Vec2::new(60.0, -5.0)));
    assert_eq!(moved_connector.to, Endpoint::Object(shape));
    let after = doc.sheet().clone();
    assert_eq!(doc.undo_label(), Some("Move board items"));
    assert!(doc.undo());
    assert_eq!(doc.sheet(), &before);
    assert!(!doc.can_undo());
    assert!(doc.redo());
    assert_eq!(doc.sheet(), &after);
    doc.sheet().validate().unwrap();
}

#[test]
fn mixed_grouping_and_locking_have_independent_undo_steps() {
    let (mut sheet, card, object, group) = mixed_group();
    sheet.ungroup_board_members(group).unwrap();
    let mut doc = Document::new(crate::Workbook::new(sheet));
    let before = doc.sheet().clone();
    let group = doc
        .group_board_members(&[BoardMember::Card(card), BoardMember::Object(object)])
        .unwrap();
    let grouped = doc.sheet().clone();
    assert!(doc.undo());
    assert_eq!(doc.sheet(), &before);
    assert!(!doc.can_undo());
    assert!(doc.redo());
    assert_eq!(doc.sheet(), &grouped);
    doc.set_canvas_group_locked(group, true).unwrap();
    assert!(doc.sheet().is_board_member_locked(BoardMember::Card(card)));
    assert!(doc.undo());
    assert_eq!(doc.sheet(), &grouped);
    assert!(doc.redo());
    assert!(
        doc.sheet()
            .is_board_member_locked(BoardMember::Object(object))
    );
    doc.set_canvas_group_locked(group, false).unwrap();
    doc.ungroup_board_members(group).unwrap();
    assert!(doc.sheet().canvas.card_state(card).is_none());
    assert!(doc.undo());
    assert_eq!(doc.sheet(), &grouped);
}

#[test]
fn moving_a_frame_moves_groups_once_and_rejects_locked_members() {
    let (mut sheet, card, object, _) = mixed_group();
    let second = sheet
        .add_card(
            "Outside the frame",
            Vec2::new(700.0, 0.0),
            Card::new(CardKind::Sticky, Vec2::new(80.0, 80.0)),
        )
        .unwrap();
    sheet
        .group_board_members(&[
            BoardMember::Card(card),
            BoardMember::Card(second),
            BoardMember::Object(object),
        ])
        .unwrap();
    let plain = sheet.add_floating("Ordinary topic", Vec2::new(10.0, 20.0));
    let zone = sheet
        .create_zone("Frame", vec![card, plain], rect(0.0, 0.0, 300.0, 300.0))
        .unwrap();
    let before = sheet.clone();
    let mut doc = Document::new(crate::Workbook::new(sheet));
    let delta = Vec2::new(25.0, 35.0);
    doc.move_zone(zone, delta).unwrap();
    for topic in [card, second, plain] {
        assert_eq!(
            doc.sheet().topic(topic).unwrap().position,
            Some(before.topic(topic).unwrap().position.unwrap() + delta)
        );
    }
    assert_eq!(
        doc.sheet().canvas.object(object).unwrap().rect,
        before.canvas.object(object).unwrap().rect.translate(delta)
    );
    assert!(doc.undo());
    assert_eq!(doc.sheet(), &before);
    assert!(!doc.can_undo());
    assert!(doc.redo());

    doc.set_canvas_objects_locked(&[object], true).unwrap();
    let locked = doc.sheet().clone();
    assert!(doc.move_zone(zone, delta).is_err());
    assert_eq!(doc.sheet(), &locked);
    assert_eq!(doc.undo_label(), Some("Lock"));
    doc.sheet().validate().unwrap();
}

#[test]
fn mixed_move_rejects_bad_destinations_without_partial_mutation_or_history() {
    let (mut sheet, card, object, _) = mixed_group();
    sheet.topic_mut(card).unwrap().position = Some(Vec2::new(999_999.0, 0.0));
    let mut doc = Document::new(crate::Workbook::new(sheet));
    let before = doc.sheet().clone();
    let state = doc.state_id();
    assert!(
        doc.move_board_members(&[BoardMember::Object(object)], Vec2::new(2.0, 0.0))
            .is_err()
    );
    assert_eq!(doc.sheet(), &before);
    assert_eq!(doc.state_id(), state);
    assert!(!doc.can_undo());

    let (mut sheet, card, object, _) = mixed_group();
    sheet.canvas.object_mut(object).unwrap().rect = rect(999_990.0, 0.0, 10.0, 10.0);
    let before = sheet.clone();
    assert!(
        sheet
            .move_board_members(&[BoardMember::Card(card)], Vec2::new(1.0, 0.0))
            .is_err()
    );
    assert_eq!(sheet, before);
}

#[test]
fn card_and_object_removal_or_conversion_dissolves_only_small_groups() {
    let (mut sheet, first_card, first_object, _) = mixed_group();
    let second_card = sheet
        .add_card(
            "Second",
            Vec2::new(200.0, 100.0),
            Card::new(CardKind::Sticky, Vec2::new(80.0, 80.0)),
        )
        .unwrap();
    let second_object = sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Diamond,
            rect(200.0, 30.0, 80.0, 40.0),
        ))
        .unwrap();
    let group = sheet
        .group_board_members(&[
            BoardMember::Card(first_card),
            BoardMember::Card(second_card),
            BoardMember::Object(first_object),
            BoardMember::Object(second_object),
        ])
        .unwrap();
    let mut doc = Document::new(crate::Workbook::new(sheet));
    let before = doc.sheet().clone();
    doc.remove_canvas_objects(&[first_object]).unwrap();
    let after_object = doc.sheet().clone();
    assert_eq!(doc.sheet().board_group_members(group).len(), 3);
    doc.set_topic_card(first_card, None).unwrap();
    let after_conversion = doc.sheet().clone();
    assert!(doc.sheet().canvas.card_state(first_card).is_none());
    assert_eq!(doc.sheet().board_group_members(group).len(), 2);
    doc.edit_sheet("Delete card", |sheet| sheet.remove_topics(&[second_card]))
        .unwrap();
    assert!(doc.sheet().canvas.group(group).is_none());
    assert_eq!(
        doc.sheet().canvas.object(second_object).unwrap().group,
        None
    );
    doc.sheet().validate().unwrap();
    assert!(doc.undo());
    assert_eq!(doc.sheet(), &after_conversion);
    assert!(doc.undo());
    assert_eq!(doc.sheet(), &after_object);
    assert!(doc.undo());
    assert_eq!(doc.sheet(), &before);
    assert!(doc.redo());
    assert!(doc.redo());
    assert!(doc.redo());
    doc.sheet().validate().unwrap();
}

#[test]
fn deleting_a_mixed_selection_and_repairing_connectors_is_one_edit() {
    let (mut sheet, card, object, _) = mixed_group();
    let connector = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(object),
            Endpoint::Topic(card),
        )))
        .unwrap();
    let before = sheet.clone();
    let mut doc = Document::new(crate::Workbook::new(sheet));
    doc.remove_board_members(&[BoardMember::Card(card)])
        .unwrap();
    assert!(!doc.sheet().contains(card));
    assert!(doc.sheet().canvas.object(object).is_none());
    let CanvasObjectKind::Connector(connector) =
        &doc.sheet().canvas.object(connector).unwrap().kind
    else {
        panic!("expected connector");
    };
    assert!(matches!(connector.from, Endpoint::Point(_)));
    assert!(matches!(connector.to, Endpoint::Point(_)));
    doc.sheet().validate().unwrap();
    assert!(doc.undo());
    assert_eq!(doc.sheet(), &before);
    assert!(!doc.can_undo());
    assert!(doc.redo());
    doc.sheet().validate().unwrap();
}

#[test]
fn group_and_direct_locks_reject_card_and_mixed_member_mutations() {
    let (mut sheet, card, object, group) = mixed_group();
    sheet.set_canvas_group_locked(group, true).unwrap();
    let locked = sheet.clone();
    assert!(sheet.is_board_member_locked(BoardMember::Card(card)));
    assert!(sheet.is_board_member_locked(BoardMember::Object(object)));
    assert!(sheet.set_topic_card(card, None).is_err());
    assert!(sheet.set_title(card, "Changed").is_err());
    assert!(sheet.set_position(card, Vec2::new(20.0, 30.0)).is_err());
    assert!(sheet.remove_topics(&[card]).is_err());
    assert!(sheet.detach_subtree(card).is_none());
    assert!(sheet.ungroup_canvas_objects(group).is_err());
    assert!(
        sheet
            .move_board_members(&[BoardMember::Card(card)], Vec2::new(5.0, 0.0))
            .is_err()
    );
    assert!(
        sheet
            .remove_board_members(&[BoardMember::Object(object)])
            .is_err()
    );
    assert_eq!(sheet, locked);

    sheet.set_canvas_group_locked(group, false).unwrap();
    sheet
        .set_board_members_locked(&[BoardMember::Card(card)], true)
        .unwrap();
    let other = sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Ellipse,
            rect(400.0, 40.0, 40.0, 40.0),
        ))
        .unwrap();
    let mut doc = Document::new(crate::Workbook::new(sheet));
    let before = doc.sheet().clone();
    assert!(
        doc.move_board_members(&[BoardMember::Object(object)], Vec2::new(1.0, 0.0))
            .is_err()
    );
    assert!(
        doc.group_canvas_objects(&[object, other]).is_err(),
        "regrouping must not indirectly ungroup a locked card"
    );
    assert!(
        doc.move_canvas_objects(&[object], Vec2::new(1.0, 0.0))
            .is_err()
    );
    assert!(doc.remove_canvas_objects(&[object]).is_err());
    assert_eq!(doc.sheet(), &before);
    assert!(!doc.can_undo());
    doc.set_board_members_locked(&[BoardMember::Card(card)], false)
        .unwrap();
    doc.move_board_members(&[BoardMember::Object(object)], Vec2::new(1.0, 0.0))
        .unwrap();
    assert!(doc.undo());
    assert_eq!(
        doc.sheet().topic(card).unwrap().position,
        before.topic(card).unwrap().position
    );
    assert!(doc.undo());
    assert_eq!(doc.sheet(), &before);
}

#[test]
fn locked_object_blocks_mixed_group_move_and_delete() {
    let (mut sheet, card, object, _) = mixed_group();
    sheet.set_canvas_objects_locked(&[object], true).unwrap();
    let before = sheet.clone();
    assert!(
        sheet
            .move_board_members(&[BoardMember::Card(card)], Vec2::new(1.0, 0.0))
            .is_err()
    );
    assert!(
        sheet
            .remove_board_members(&[BoardMember::Card(card)])
            .is_err()
    );
    assert!(sheet.set_topic_card(card, None).is_err());
    let mut presentation = sheet.topic(card).unwrap().card.unwrap();
    presentation.size.x += 10.0;
    assert!(sheet.set_topic_card(card, Some(presentation)).is_err());
    assert!(sheet.set_position(card, Vec2::new(1.0, 0.0)).is_err());
    assert!(sheet.remove_topics(&[card]).is_err());
    assert!(sheet.detach_subtree(card).is_none());
    assert_eq!(sheet, before);
}

#[test]
fn locked_card_blocks_changes_to_other_members_of_its_group() {
    let (mut sheet, card, object, _) = mixed_group();
    sheet
        .set_board_members_locked(&[BoardMember::Card(card)], true)
        .unwrap();
    let before = sheet.clone();
    let mut frame = sheet.canvas.object(object).unwrap().rect;
    frame.size.x += 10.0;
    assert!(sheet.resize_canvas_object(object, frame).is_err());
    assert!(sheet.rotate_canvas_object(object, 15.0).is_err());
    assert!(
        sheet
            .reorder_canvas_objects(&[object], ZOrder::Front)
            .is_err()
    );
    assert!(sheet.set_canvas_band(&[object], Band::Below).is_err());
    assert!(
        sheet
            .update_canvas_object(object, |object| object.alt_text = "Changed".into())
            .is_err()
    );
    assert_eq!(sheet, before);
}

#[test]
fn allocator_recovers_from_maximum_deserialized_ids_without_collisions() {
    let sheet = Sheet::new("Board", "Root");
    let mut json = serde_json::to_value(sheet).unwrap();
    json["next_id"] = serde_json::json!(u64::MAX);
    let mut sheet: Sheet = serde_json::from_value(json).unwrap();
    let first = sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            rect(0.0, 0.0, 40.0, 40.0),
        ))
        .unwrap();
    let second = sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            rect(50.0, 0.0, 40.0, 40.0),
        ))
        .unwrap();
    let topic = sheet.add_floating("Card", Vec2::new(0.0, 100.0));
    assert_eq!(first.0, u64::MAX);
    assert_ne!(second.0, first.0);
    assert_ne!(topic.0, first.0);
    assert_ne!(topic.0, second.0);
    assert_ne!(topic, sheet.root);
    sheet.validate().unwrap();
}

#[test]
fn raw_detach_reparent_and_clipboard_do_not_leave_card_memberships() {
    let (mut sheet, card, object, group) = mixed_group();
    let payload = crate::clipboard::copy_topics(&sheet, &[card])
        .unwrap()
        .unwrap();
    let mut copied = payload.subtrees[0].clone();
    copied.reassign_ids(&mut sheet);
    let duplicate = sheet.attach_floating(Vec2::new(600.0, 0.0), copied);
    assert!(sheet.canvas.card_state(duplicate).is_none());
    assert_eq!(
        sheet.board_member_group(BoardMember::Card(card)),
        Some(group)
    );
    sheet.validate().unwrap();

    let sub = sheet.detach_subtree(card).unwrap();
    assert!(sheet.canvas.card_state(card).is_none());
    assert!(sheet.canvas.group(group).is_none());
    assert_eq!(sheet.canvas.object(object).unwrap().group, None);
    sheet.attach_floating(Vec2::new(50.0, 50.0), sub);
    assert!(sheet.canvas.card_state(card).is_none());
    sheet.validate().unwrap();

    let group = sheet
        .group_board_members(&[BoardMember::Card(card), BoardMember::Object(object)])
        .unwrap();
    sheet
        .detach_to_floating(card, Vec2::new(20.0, 30.0))
        .unwrap();
    assert_eq!(
        sheet.board_member_group(BoardMember::Card(card)),
        Some(group)
    );
    sheet.show_central_topic();
    sheet.move_topic(card, sheet.root, 0).unwrap();
    assert!(sheet.topic(card).unwrap().card.is_none());
    assert!(sheet.canvas.group(group).is_none());
    assert!(sheet.canvas.card_state(card).is_none());
    sheet.validate().unwrap();
}

#[test]
fn malformed_card_memberships_are_rejected_after_deserialization() {
    let (sheet, card, object, group) = mixed_group();
    let mut cases = Vec::new();
    let mut duplicate = sheet.clone();
    duplicate
        .canvas
        .card_states
        .push(duplicate.canvas.card_states[0]);
    cases.push(duplicate);
    let mut missing_topic = sheet.clone();
    missing_topic.canvas.card_states[0].topic = TopicId(999_999);
    cases.push(missing_topic);
    let mut anchor = sheet.clone();
    anchor.canvas.card_states[0].topic = anchor.root;
    cases.push(anchor);
    let mut plain = sheet.clone();
    let plain_topic = plain.add_floating("Plain", Vec2::ZERO);
    plain.canvas.card_states[0].topic = plain_topic;
    cases.push(plain);
    let mut child = sheet.clone();
    let child_topic = child.add_child(card, None, "Child");
    child.canvas.card_states[0].topic = child_topic;
    cases.push(child);
    let mut converted = sheet.clone();
    converted.topic_mut(card).unwrap().card = None;
    cases.push(converted);
    let mut missing_group = sheet.clone();
    missing_group.canvas.card_states[0].group = Some(CanvasGroupId(999_999));
    cases.push(missing_group);
    let mut no_group = sheet.clone();
    no_group.canvas.groups.clear();
    cases.push(no_group);
    let mut singleton = sheet.clone();
    singleton
        .canvas
        .objects
        .retain(|candidate| candidate.id != object);
    assert_eq!(
        singleton.canvas.card_state(card).unwrap().group,
        Some(group)
    );
    cases.push(singleton);

    for broken in cases {
        let json = serde_json::to_string(&crate::Workbook::new(broken.clone())).unwrap();
        let restored: crate::Workbook = serde_json::from_str(&json).unwrap();
        assert!(
            restored.validate().is_err(),
            "accepted invalid membership: {json}"
        );
        let sheet_json = serde_json::to_string(&broken).unwrap();
        assert!(crate::clipboard::decode_sheet(&format!("THINODA-SHEET-1\n{sheet_json}")).is_err());
    }
}

#[test]
fn nonfinite_canvas_rotations_are_rejected_before_normalization() {
    let mut sheet = Sheet::new("Board", "Root");
    for rotation in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let before = sheet.clone();
        let mut object = CanvasObject::shape(ShapeKind::Rectangle, rect(0.0, 0.0, 40.0, 20.0));
        object.rotation = rotation;
        assert!(sheet.add_canvas_object(object).is_err());
        assert_eq!(sheet, before);
    }
    let mut object = CanvasObject::shape(ShapeKind::Rectangle, rect(0.0, 0.0, 40.0, 20.0));
    object.rotation = 450.0;
    let id = sheet.add_canvas_object(object).unwrap();
    assert_eq!(sheet.canvas.object(id).unwrap().rotation, 90.0);
    for rotation in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let before = sheet.clone();
        assert!(
            sheet
                .update_canvas_object(id, |object| {
                    object.rotation = rotation;
                    object.alt_text = "must roll back".into();
                })
                .is_err()
        );
        assert_eq!(sheet, before);
    }
    sheet
        .update_canvas_object(id, |object| object.rotation = -90.0)
        .unwrap();
    assert_eq!(sheet.canvas.object(id).unwrap().rotation, 270.0);
    sheet.validate().unwrap();
    let mut doc = Document::new(crate::Workbook::new(sheet));
    let before = doc.sheet().clone();
    assert!(
        doc.update_canvas_object("Invalid rotation", id, |object| {
            object.rotation = f32::INFINITY
        })
        .is_err()
    );
    assert_eq!(doc.sheet(), &before);
    assert!(!doc.can_undo());
}

#[test]
fn hidden_root_is_rejected_for_imported_fim_sheets() {
    let xml = format!(
        "<x:xdatenfelder.stammdatenschema.0102 xmlns:x='{}'><x:stammdatenschema><x:identifikation><x:id>S1</x:id><x:version>1.0.0</x:version></x:identifikation><x:name>Schema</x:name></x:stammdatenschema></x:xdatenfelder.stammdatenschema.0102>",
        crate::fim::Dialect::Xdf3.namespace()
    );
    let mut sheet = crate::fim::FimDocument::parse(xml.as_bytes())
        .unwrap()
        .into_sheet()
        .unwrap();
    sheet.validate().unwrap();
    assert!(sheet.hide_central_topic().is_err());
    sheet.root_mode = SheetRootMode::Hidden;
    assert!(validate_board(&sheet).unwrap_err().contains("FIM"));
    assert!(sheet.hide_central_topic().is_err());
    let json = serde_json::to_string(&crate::Workbook::new(sheet)).unwrap();
    let restored: crate::Workbook = serde_json::from_str(&json).unwrap();
    assert!(restored.validate().unwrap_err().contains("FIM"));
}
