use super::*;
use crate::board::{CanvasObjectId, Card, CardKind, InkStroke, InkTool, ShapeKind, SheetRootMode};
use crate::geometry::Color;
use crate::layout::{ApproxMeasure, layout_sheet};
use crate::model::Sheet;

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::from_min_size(Vec2::new(x, y), Vec2::new(w, h))
}

fn scene(s: &Sheet) -> (Layout, Scene) {
    let layout = layout_sheet(s, &ApproxMeasure, false);
    let scene = build_scene(s, &layout);
    (layout, scene)
}

fn shape(s: &mut Sheet, kind: ShapeKind, r: Rect, band: Band) -> CanvasObjectId {
    s.add_canvas_object(CanvasObject {
        band,
        ..CanvasObject::shape(kind, r)
    })
    .unwrap()
}

#[test]
fn bands_paint_in_order() {
    let mut s = Sheet::new("Bands", "Root");
    let f = s.add_floating("F", Vec2::new(0.0, 400.0));
    let zone = s
        .create_zone("Z", vec![f], rect(-200.0, 300.0, 400.0, 200.0))
        .unwrap();
    // Insert Above before Below to prove ordering is by band, not Vec order.
    let above = shape(
        &mut s,
        ShapeKind::Rectangle,
        rect(-20.0, -20.0, 40.0, 40.0),
        Band::Above,
    );
    let below = shape(
        &mut s,
        ShapeKind::Ellipse,
        rect(-20.0, -20.0, 40.0, 40.0),
        Band::Below,
    );
    s.board.grid = GridKind::Lines;
    let (_, sc) = scene(&s);
    let pos = |id: SceneId| sc.items.iter().position(|i| i.id == id).unwrap();
    let bands: Vec<_> = sc.items.iter().map(|i| i.band).collect();
    assert!(bands.windows(2).all(|w| w[0] <= w[1]), "{bands:?}");
    assert!(pos(SceneId::Zone(zone)) < pos(SceneId::Object(below)));
    assert!(pos(SceneId::Object(below)) < pos(SceneId::Topic(s.root)));
    assert!(pos(SceneId::Topic(s.root)) < pos(SceneId::Object(above)));
    assert_eq!(sc.grid, Some((GridKind::Lines, s.board.grid_size)));

    // The Above object covers the root; the Below object is covered by it.
    assert_eq!(sc.hit_test(Vec2::ZERO), Some(SceneHit::Object(above)));
    s.set_canvas_band(&[above], Band::Below).unwrap();
    let (_, sc) = scene(&s);
    assert_eq!(sc.hit_test(Vec2::ZERO), Some(SceneHit::Topic(s.root)));
    // Zones never hit; an empty spot inside the frame is a miss.
    assert_eq!(sc.hit_test(Vec2::new(-190.0, 310.0)), None);
}

#[test]
fn clipping_frames_constrain_hits_selection_and_visible_bounds() {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let root = sheet.add_floating("Card", Vec2::new(50.0, 50.0));
    sheet
        .set_topic_card(
            root,
            Some(Card::new(CardKind::Sticky, Vec2::new(140.0, 80.0))),
        )
        .unwrap();
    let zone = sheet
        .create_zone("Frame", vec![root], rect(0.0, 0.0, 100.0, 100.0))
        .unwrap();
    sheet
        .zones
        .iter_mut()
        .find(|frame| frame.id == zone)
        .unwrap()
        .clip = true;
    let object = shape(
        &mut sheet,
        ShapeKind::Rectangle,
        rect(-30.0, 20.0, 80.0, 40.0),
        Band::Above,
    );
    let (_, scene) = scene(&sheet);
    let clip = rect(0.0, 0.0, 100.0, 100.0);
    assert_eq!(scene.item(SceneId::Topic(root)).unwrap().clip, Some(clip));
    assert_eq!(
        scene.item(SceneId::Object(object)).unwrap().clip,
        Some(clip)
    );
    assert_eq!(
        scene.hit_test(Vec2::new(20.0, 40.0)),
        Some(SceneHit::Object(object))
    );
    assert_eq!(scene.hit_test(Vec2::new(-10.0, 40.0)), None);
    assert!(
        !scene
            .objects_in(rect(-25.0, 25.0, 10.0, 10.0))
            .contains(&SceneHit::Object(object))
    );
    assert_eq!(scene.bounds(), Some(clip));
}

#[test]
fn summary_topics_inherit_their_owner_frame_clip() {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let root = sheet.add_floating("Root", Vec2::new(50.0, 50.0));
    let first = sheet.add_child(root, None, "First");
    let second = sheet.add_child(root, None, "Second");
    let summary = sheet.add_summary(&[first, second], "Summary").unwrap();
    let nested = sheet.add_child(summary, None, "Detail");
    let clip = rect(0.0, 0.0, 100.0, 100.0);
    let zone = sheet.create_zone("Frame", vec![root], clip).unwrap();
    sheet
        .zones
        .iter_mut()
        .find(|frame| frame.id == zone)
        .unwrap()
        .clip = true;
    assert_eq!(topic_clip_rect(&sheet, summary), Some(clip));
    assert_eq!(topic_clip_rect(&sheet, nested), Some(clip));
}

#[test]
fn attached_connectors_use_resolved_path_for_frame_clipping() {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let clip = rect(900.0, 900.0, 200.0, 200.0);
    let frame = sheet.create_board_frame("Frame", clip).unwrap();
    sheet
        .zones
        .iter_mut()
        .find(|zone| zone.id == frame)
        .unwrap()
        .clip = true;
    let first = shape(
        &mut sheet,
        ShapeKind::Rectangle,
        rect(940.0, 940.0, 20.0, 20.0),
        Band::Above,
    );
    let second = shape(
        &mut sheet,
        ShapeKind::Rectangle,
        rect(1140.0, 940.0, 20.0, 20.0),
        Band::Above,
    );
    let connector = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(first),
            Endpoint::Object(second),
        )))
        .unwrap();
    let (_, clipped) = scene(&sheet);
    assert_eq!(
        clipped.item(SceneId::Object(connector)).unwrap().clip,
        Some(clip)
    );
    assert_eq!(clipped.hit_test(Vec2::new(1120.0, 950.0)), None);

    sheet
        .move_canvas_objects(&[first, second], Vec2::new(500.0, 0.0))
        .unwrap();
    let (_, moved) = scene(&sheet);
    assert_eq!(moved.item(SceneId::Object(connector)).unwrap().clip, None);
}

#[test]
fn rotated_shape_hit_test() {
    let mut s = Sheet::new("Rot", "Root");
    let id = shape(
        &mut s,
        ShapeKind::Rectangle,
        rect(1000.0, 0.0, 200.0, 20.0),
        Band::Above,
    );
    s.rotate_canvas_object(id, 90.0).unwrap();
    let (_, sc) = scene(&s);
    // Center (1100,10); rotated it spans x 1090..1110, y -90..110.
    assert_eq!(
        sc.hit_test(Vec2::new(1100.0, 100.0)),
        Some(SceneHit::Object(id))
    );
    assert_eq!(sc.hit_test(Vec2::new(1005.0, 10.0)), None);
    assert_eq!(sc.hit_test(Vec2::new(1190.0, 10.0)), None);
    let b = sc.item(SceneId::Object(id)).unwrap().bounds;
    assert!(
        (b.size.x - 20.0).abs() < 0.5 && (b.size.y - 200.0).abs() < 0.5,
        "{b:?}"
    );

    // Non-rectangular outline: a diamond's corners are outside.
    let d = shape(
        &mut s,
        ShapeKind::Diamond,
        rect(2000.0, 0.0, 100.0, 100.0),
        Band::Above,
    );
    let (_, sc) = scene(&s);
    assert_eq!(
        sc.hit_test(Vec2::new(2050.0, 50.0)),
        Some(SceneHit::Object(d))
    );
    assert_eq!(sc.hit_test(Vec2::new(2005.0, 5.0)), None);
}

#[test]
fn ink_hit_respects_width_and_rotation() {
    let mut s = Sheet::new("Ink", "Root");
    let id = s
        .add_canvas_object(CanvasObject::ink(InkStroke::new(
            InkTool::Pen,
            Color::rgb(0, 0, 0),
            10.0,
            vec![Vec2::new(1000.0, 0.0), Vec2::new(1100.0, 0.0)],
        )))
        .unwrap();
    let (_, sc) = scene(&s);
    assert_eq!(
        sc.hit_test(Vec2::new(1050.0, 7.0)),
        Some(SceneHit::Object(id))
    );
    assert_eq!(sc.hit_test(Vec2::new(1050.0, 9.0)), None);
    s.rotate_canvas_object(id, 90.0).unwrap();
    let (_, sc) = scene(&s);
    assert_eq!(
        sc.hit_test(Vec2::new(1050.0, 40.0)),
        Some(SceneHit::Object(id))
    );
    assert_eq!(sc.hit_test(Vec2::new(1090.0, 0.0)), None);
}

#[test]
fn connectors_resolve_endpoints_and_hit() {
    let mut s = Sheet::new("Conn", "Root");
    let f = s.add_floating("F", Vec2::new(600.0, 0.0));
    let box_id = shape(
        &mut s,
        ShapeKind::Rectangle,
        rect(280.0, 280.0, 40.0, 40.0),
        Band::Above,
    );
    let straight = s
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Topic(s.root),
            Endpoint::Topic(f),
        )))
        .unwrap();
    let elbow = s
        .add_canvas_object(CanvasObject::connector(CanvasConnector {
            route: ConnectorRoute::Elbow,
            ..CanvasConnector::new(
                Endpoint::Point(Vec2::new(0.0, 600.0)),
                Endpoint::Object(box_id),
            )
        }))
        .unwrap();
    let curve = CanvasConnector {
        route: ConnectorRoute::Curve,
        ..CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 100.0)),
        )
    };
    let (layout, sc) = scene(&s);
    let conn = |id| match &s.canvas.object(id).unwrap().kind {
        CanvasObjectKind::Connector(c) => c.clone(),
        _ => unreachable!(),
    };
    let path = connector_path(&s, &layout, &conn(straight));
    assert_eq!(path.len(), 2);
    // Clipped to the topic boundaries rather than their centers.
    let root = layout.rect(s.root).unwrap();
    let fr = layout.rect(f).unwrap();
    assert!((path[0].x - root.max().x).abs() < 1.0, "{path:?} {root:?}");
    assert!((path[1].x - fr.min.x).abs() < 1.0, "{path:?} {fr:?}");
    let mid = (path[0] + path[1]) * 0.5;
    assert_eq!(
        sc.hit_test(mid + Vec2::new(0.0, 2.0)),
        Some(SceneHit::Object(straight))
    );
    assert_eq!(sc.hit_test(mid + Vec2::new(0.0, 20.0)), None);

    let path = connector_path(&s, &layout, &conn(elbow));
    assert_eq!(path.len(), 4);
    assert_eq!(path[0], Vec2::new(0.0, 600.0));
    assert!(rect(280.0, 280.0, 40.0, 40.0).expand(0.5).contains(path[3]));
    assert!(sc.item(SceneId::Object(elbow)).is_some());
    assert_eq!(connector_path(&s, &layout, &curve).len(), 17);

    // An unresolvable endpoint yields no path and no scene item, never a panic.
    let dangling = CanvasConnector::new(
        Endpoint::Object(CanvasObjectId(99_999)),
        Endpoint::Point(Vec2::ZERO),
    );
    assert!(connector_path(&s, &layout, &dangling).is_empty());
    let mut broken = s.clone();
    if let CanvasObjectKind::Connector(c) = &mut broken.canvas.object_mut(straight).unwrap().kind {
        c.to = Endpoint::Topic(TopicId(99_999));
    }
    let layout = layout_sheet(&broken, &ApproxMeasure, false);
    assert!(
        build_scene(&broken, &layout)
            .item(SceneId::Object(straight))
            .is_none()
    );
}

#[test]
fn connectors_end_on_shape_perimeters() {
    let mut s = Sheet::new("Perimeters", "");
    s.root_mode = SheetRootMode::Hidden;
    let corner = Endpoint::Point(Vec2::new(300.0, 300.0));
    let mut end_on = |kind, rotation: f32, toward: Endpoint| {
        let id = s
            .add_canvas_object(CanvasObject {
                rotation,
                ..CanvasObject::shape(kind, rect(0.0, 0.0, 100.0, 100.0))
            })
            .unwrap();
        let layout = layout_sheet(&s, &ApproxMeasure, false);
        let start = connector_path(
            &s,
            &layout,
            &CanvasConnector::new(Endpoint::Object(id), toward),
        )[0];
        s.remove_canvas_objects(&[id]).unwrap();
        start
    };
    let near = |a: Vec2, b: Vec2| (a - b).length() < 1.0;

    // Rectangles keep their box corner; a diamond and an ellipse end on their edge.
    assert!(near(
        end_on(ShapeKind::Rectangle, 0.0, corner),
        Vec2::new(100.0, 100.0)
    ));
    assert!(near(
        end_on(ShapeKind::Diamond, 0.0, corner),
        Vec2::new(75.0, 75.0)
    ));
    let diagonal = 50.0 + 50.0 / 2f32.sqrt();
    let ellipse = end_on(ShapeKind::Ellipse, 0.0, corner);
    assert!(near(ellipse, Vec2::new(diagonal, diagonal)), "{ellipse:?}");

    // Rotation turns the outline: a diamond rotated 45 degrees is an upright square.
    let rotated = end_on(
        ShapeKind::Diamond,
        45.0,
        Endpoint::Point(Vec2::new(300.0, 50.0)),
    );
    assert!(near(rotated, Vec2::new(diagonal, 50.0)), "{rotated:?}");

    // Every shape's end lies on its outline, never inside or on the bare box.
    for kind in ShapeKind::ALL {
        let end = end_on(kind, 30.0, corner);
        let outline = shape_geometry(kind, rect(0.0, 0.0, 100.0, 100.0), 30.0).outline;
        assert!(
            distance_to_outline(end, &outline) < 0.5,
            "{kind:?} ends at {end:?}"
        );
    }
}

#[test]
fn polygon_clip_takes_the_outermost_crossing() {
    // A notch cut from the top: the ray to the right leaves at x 60, re-enters at 80
    // and exits at 100.
    let notched = [
        Vec2::new(0.0, 0.0),
        Vec2::new(60.0, 0.0),
        Vec2::new(60.0, 55.0),
        Vec2::new(80.0, 55.0),
        Vec2::new(80.0, 0.0),
        Vec2::new(100.0, 0.0),
        Vec2::new(100.0, 100.0),
        Vec2::new(0.0, 100.0),
    ];
    let center = Vec2::new(50.0, 50.0);
    assert_eq!(
        clip_to_polygon(&notched, center, Vec2::new(200.0, 50.0)),
        Some(Vec2::new(100.0, 50.0))
    );
    assert_eq!(
        clip_to_polygon(&notched, center, Vec2::new(50.0, -200.0)),
        Some(Vec2::new(50.0, 0.0))
    );
    assert_eq!(clip_to_polygon(&notched, center, center), None);
}

#[test]
fn connectors_follow_resolved_connector_targets_and_detect_cycles() {
    let mut sheet = Sheet::new("Connectors", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let target = shape(
        &mut sheet,
        ShapeKind::Rectangle,
        rect(100.0, 0.0, 20.0, 20.0),
        Band::Above,
    );
    let base = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(target),
            Endpoint::Point(Vec2::new(300.0, 10.0)),
        )))
        .unwrap();
    let dependent = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(base),
            Endpoint::Point(Vec2::new(400.0, 100.0)),
        )))
        .unwrap();

    let (layout, built_scene) = scene(&sheet);
    let CanvasObjectKind::Connector(connector) = &sheet.canvas.object(dependent).unwrap().kind
    else {
        unreachable!();
    };
    let path = connector_path(&sheet, &layout, connector);
    assert_eq!(path[0], Vec2::new(210.0, 10.0));
    assert_eq!(
        built_scene
            .item(SceneId::Object(dependent))
            .unwrap()
            .geometry,
        SceneGeometry::Polyline {
            points: path,
            width: connector.width
        }
    );

    sheet
        .move_canvas_objects(&[target], Vec2::new(100.0, 0.0))
        .unwrap();
    let (layout, _) = scene(&sheet);
    let CanvasObjectKind::Connector(connector) = &sheet.canvas.object(dependent).unwrap().kind
    else {
        unreachable!();
    };
    assert_eq!(
        connector_path(&sheet, &layout, connector)[0],
        Vec2::new(260.0, 10.0)
    );
    assert!(validate_connector_cycles(&sheet).is_ok());

    let mut deleted = sheet.clone();
    deleted.remove_canvas_objects(&[base]).unwrap();
    let CanvasObjectKind::Connector(connector) = &deleted.canvas.object(dependent).unwrap().kind
    else {
        unreachable!();
    };
    assert_eq!(connector.from, Endpoint::Point(Vec2::new(260.0, 10.0)));

    let mut doc = crate::Document::new(crate::Workbook::new(sheet.clone()));
    doc.edit_sheet("Remove", |sheet| {
        sheet.canvas.objects.retain(|object| object.id != base);
        Ok(())
    })
    .unwrap();
    let CanvasObjectKind::Connector(connector) =
        &doc.sheet().canvas.object(dependent).unwrap().kind
    else {
        unreachable!();
    };
    assert_eq!(connector.from, Endpoint::Point(Vec2::new(260.0, 10.0)));
    assert!(doc.undo());
    assert_eq!(doc.sheet(), &sheet);

    let CanvasObjectKind::Connector(connector) = &mut sheet.canvas.object_mut(base).unwrap().kind
    else {
        unreachable!();
    };
    connector.from = Endpoint::Object(dependent);
    assert!(validate_connector_cycles(&sheet).is_err());
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let CanvasObjectKind::Connector(connector) = &sheet.canvas.object(dependent).unwrap().kind
    else {
        unreachable!();
    };
    assert!(connector_path(&sheet, &layout, connector).is_empty());
}

#[test]
fn long_connector_chains_resolve_without_recursion() {
    let mut sheet = Sheet::new("Connectors", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut previous = Endpoint::Point(Vec2::ZERO);
    for n in 0..4_000 {
        let id = CanvasObjectId(n + 10);
        let mut object = CanvasObject::connector(CanvasConnector::new(
            previous,
            Endpoint::Point(Vec2::new(100.0, 100.0)),
        ));
        object.id = id;
        sheet.canvas.objects.push(object);
        previous = Endpoint::Object(id);
    }
    assert!(validate_connector_cycles(&sheet).is_ok());
    let (_, scene) = scene(&sheet);
    assert_eq!(scene.items.len(), 4_000);
    assert!(scene.item(SceneId::Object(CanvasObjectId(4_009))).is_some());
}

#[test]
fn hidden_root_is_excluded_everywhere() {
    let mut s = Sheet::new("Hidden", "");
    s.root_mode = SheetRootMode::Hidden;
    let f = s.add_floating("F", Vec2::new(400.0, 400.0));
    let (layout, sc) = scene(&s);
    assert!(layout.rect(s.root).is_none());
    assert!(sc.item(SceneId::Topic(s.root)).is_none());
    assert_eq!(sc.hit_test(Vec2::ZERO), None);
    assert_eq!(
        sc.objects_in(rect(-1000.0, -1000.0, 2000.0, 2000.0)),
        vec![SceneHit::Topic(f)]
    );
    let b = sc.bounds().unwrap();
    assert!(!b.contains(Vec2::ZERO), "{b:?}");
    assert!(!layout.bounds.contains(Vec2::ZERO), "{:?}", layout.bounds);
    assert_eq!(topic_anchor_rect(&s, &layout, s.root), None);

    // Empty hidden board: no items, no bounds, no panic.
    let mut empty = Sheet::new("Empty", "");
    empty.root_mode = SheetRootMode::Hidden;
    let (_, sc) = scene(&empty);
    assert!(sc.items.is_empty());
    assert_eq!(sc.bounds(), None);
    assert_eq!(sc.hit_test(Vec2::ZERO), None);
}

#[test]
fn objects_in_and_bounds_include_canvas_objects() {
    let mut s = Sheet::new("Area", "Root");
    let far = shape(
        &mut s,
        ShapeKind::Star,
        rect(5000.0, 5000.0, 100.0, 100.0),
        Band::Below,
    );
    let (layout, sc) = scene(&s);
    assert!(layout.bounds.contains(Vec2::new(5050.0, 5050.0)));
    let b = sc.bounds().unwrap();
    assert!(
        b.contains(Vec2::new(5050.0, 5050.0)) && b.max().x > 5090.0,
        "{b:?}"
    );
    assert_eq!(
        sc.objects_in(rect(4900.0, 4900.0, 150.0, 150.0)),
        vec![SceneHit::Object(far)]
    );
    let all = sc.objects_in(rect(-100.0, -100.0, 6000.0, 6000.0));
    assert_eq!(all, vec![SceneHit::Object(far), SceneHit::Topic(s.root)]);
}

#[test]
fn rotated_card_topic_hit_test() {
    let mut s = Sheet::new("Card", "Root");
    let mut card = crate::board::Card::new(crate::board::CardKind::Sticky, Vec2::new(200.0, 20.0));
    card.rotation = 90.0;
    let id = s.add_card("C", Vec2::new(1000.0, 0.0), card).unwrap();
    let (_, sc) = scene(&s);
    assert_eq!(
        sc.hit_test(Vec2::new(1000.0, 90.0)),
        Some(SceneHit::Topic(id))
    );
    assert_eq!(sc.hit_test(Vec2::new(1090.0, 0.0)), None);
}
