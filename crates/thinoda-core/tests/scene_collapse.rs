use thinoda_core::{
    ApproxMeasure, CanvasConnector, CanvasObject, CanvasObjectId, CanvasObjectKind, Card, CardKind,
    ConnectorPort, ConnectorRoute, Endpoint, Rect, Scene, SceneHit, SceneId, ShapeKind, Sheet,
    SheetRootMode, Vec2,
    board::{MAX_CANVAS_OBJECTS, distance_to_outline},
    build_scene, layout_sheet,
    scene::{CollapsePreview, SceneGeometry},
    scene_hierarchy::MAX_SCENE_DEPTH,
};

fn shape(sheet: &mut Sheet, label: &str, rect: Rect) -> CanvasObjectId {
    let mut object = CanvasObject::shape(ShapeKind::Rectangle, rect);
    if let CanvasObjectKind::Shape { label: value, .. } = &mut object.kind {
        *value = label.into();
    }
    sheet.add_canvas_object(object).unwrap()
}

fn nested() -> (Sheet, [CanvasObjectId; 7]) {
    let mut sheet = Sheet::new("Collapse", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let pool = shape(
        &mut sheet,
        "Pool",
        Rect::from_min_size(Vec2::ZERO, Vec2::new(500.0, 300.0)),
    );
    let lane = shape(
        &mut sheet,
        "Lane",
        Rect::from_min_size(Vec2::new(40.0, 70.0), Vec2::new(400.0, 200.0)),
    );
    let sublane = shape(
        &mut sheet,
        "Sub-lane",
        Rect::from_min_size(Vec2::new(80.0, 100.0), Vec2::new(300.0, 120.0)),
    );
    let task = shape(
        &mut sheet,
        "Task",
        Rect::from_min_size(Vec2::new(100.0, 140.0), Vec2::new(80.0, 40.0)),
    );
    let peer = shape(
        &mut sheet,
        "Peer",
        Rect::from_min_size(Vec2::new(240.0, 140.0), Vec2::new(80.0, 40.0)),
    );
    let other = shape(
        &mut sheet,
        "Other",
        Rect::from_min_size(Vec2::new(650.0, 0.0), Vec2::new(400.0, 300.0)),
    );
    let outside = shape(
        &mut sheet,
        "Outside",
        Rect::from_min_size(Vec2::new(700.0, 140.0), Vec2::new(80.0, 40.0)),
    );
    for container in [pool, lane, sublane, other] {
        sheet.set_canvas_container(container, true).unwrap();
    }
    for (child, parent) in [
        (lane, pool),
        (sublane, lane),
        (task, sublane),
        (peer, sublane),
        (outside, other),
    ] {
        sheet
            .reparent_canvas_objects(&[child], Some(parent))
            .unwrap();
    }
    (sheet, [pool, lane, sublane, task, peer, other, outside])
}

fn compact() -> Rect {
    Rect::from_min_size(Vec2::ZERO, Vec2::new(500.0, 40.0))
}

fn edge(sheet: &mut Sheet, from: Endpoint, to: Endpoint) -> CanvasObjectId {
    sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(from, to)))
        .unwrap()
}

fn path(scene: &Scene, id: CanvasObjectId) -> &[Vec2] {
    let SceneGeometry::Polyline { points, .. } = &scene.item(SceneId::Object(id)).unwrap().geometry
    else {
        panic!("expected connector");
    };
    points
}

fn on_border(rect: Rect, point: Vec2) -> bool {
    let tolerance = 0.001;
    rect.expand(tolerance).contains(point)
        && [
            (point.x - rect.min.x).abs(),
            (point.x - rect.max().x).abs(),
            (point.y - rect.min.y).abs(),
            (point.y - rect.max().y).abs(),
        ]
        .into_iter()
        .any(|distance| distance < tolerance)
}

#[test]
fn compact_frames_hide_descendants_from_scene_hits_and_bounds_without_changing_source() {
    let (mut sheet, [pool, lane, sublane, task, peer, _, _]) = nested();
    sheet.canvas.object_mut(pool).unwrap().locked = true;
    sheet.canvas.object_mut(pool).unwrap().alt_text = "Pool description".into();
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let original = sheet.clone();
    let expanded = build_scene(&sheet, &layout);
    let preview = CollapsePreview::new(&sheet, &[(pool, compact())]).unwrap();
    let scene = preview.build_scene(&layout);
    assert!(std::ptr::eq(preview.source_sheet(), &sheet));
    let frame = preview.object(pool).unwrap();
    assert_eq!(frame.rect, compact());
    assert!(frame.locked);
    assert_eq!(frame.alt_text, "Pool description");
    for id in [lane, sublane, task, peer] {
        assert_eq!(preview.visible_representative(id), Some(pool));
        assert!(preview.object(id).is_none());
        assert!(scene.item(SceneId::Object(id)).is_none());
    }
    assert_eq!(preview.visible_representative(CanvasObjectId(99999)), None);
    assert!(preview.object(CanvasObjectId(99999)).is_none());
    assert_eq!(
        scene.hit_test(Vec2::new(250.0, 20.0)),
        Some(SceneHit::Object(pool))
    );
    assert_eq!(scene.hit_test(Vec2::new(140.0, 160.0)), None);
    assert!(
        scene
            .objects_in(sheet.canvas.object(task).unwrap().rect)
            .is_empty()
    );
    assert_eq!(scene.item(SceneId::Object(pool)).unwrap().bounds, compact());
    assert_eq!(sheet, original);
    assert_eq!(
        CollapsePreview::new(&sheet, &[])
            .unwrap()
            .build_scene(&layout),
        expanded
    );
}

#[test]
fn nested_collapse_uses_the_nearest_visible_ancestor_and_restores_inner_state() {
    let (sheet, [pool, lane, sublane, task, _, _, _]) = nested();
    let inner = Rect::from_min_size(Vec2::new(80.0, 100.0), Vec2::new(300.0, 30.0));
    let outer = CollapsePreview::new(&sheet, &[(sublane, inner), (pool, compact())]).unwrap();
    assert_eq!(outer.visible_representative(task), Some(pool));
    assert_eq!(outer.visible_representative(sublane), Some(pool));
    assert!(outer.object(sublane).is_none());
    let inner_only = CollapsePreview::new(&sheet, &[(sublane, inner)]).unwrap();
    assert_eq!(inner_only.visible_representative(task), Some(sublane));
    assert_eq!(inner_only.object(sublane).unwrap().rect, inner);
    assert_eq!(
        inner_only.object(pool).unwrap(),
        sheet.canvas.object(pool).unwrap()
    );
    assert_eq!(
        inner_only.object(lane).unwrap(),
        sheet.canvas.object(lane).unwrap()
    );
}

#[test]
fn external_routes_fold_real_fixed_endpoints_to_compact_boundaries_in_all_route_modes() {
    let (mut sheet, [pool, _, _, task, _, other, outside]) = nested();
    let crossing = edge(
        &mut sheet,
        Endpoint::Object(task),
        Endpoint::Object(outside),
    );
    // Preserve a supported imported, noncanonical edge parent.
    sheet.canvas.scene.parents.insert(crossing, pool);
    let right = Rect::from_min_size(Vec2::new(650.0, 0.0), Vec2::new(400.0, 40.0));
    for route in [
        ConnectorRoute::Straight,
        ConnectorRoute::Elbow,
        ConnectorRoute::Curve,
    ] {
        let CanvasObjectKind::Connector(connector) =
            &mut sheet.canvas.object_mut(crossing).unwrap().kind
        else {
            panic!("connector")
        };
        connector.route = route;
        connector.from_port = Some(ConnectorPort {
            position: Vec2::ZERO,
            offset: Vec2::new(-100.0, -100.0),
            perimeter: false,
        });
        let original = sheet.clone();
        let layout = layout_sheet(&sheet, &ApproxMeasure, false);
        let expanded = build_scene(&sheet, &layout);
        let preview = CollapsePreview::new(&sheet, &[(pool, compact()), (other, right)]).unwrap();
        assert_eq!(preview.visible_representative(crossing), Some(crossing));
        let scene = preview.build_scene(&layout);
        let points = path(&scene, crossing);
        assert!(on_border(compact(), points[0]), "{route:?}: {points:?}");
        assert!(
            on_border(right, *points.last().unwrap()),
            "{route:?}: {points:?}"
        );
        assert_eq!(points[0], Vec2::new(500.0, 20.0));
        assert_eq!(*points.last().unwrap(), Vec2::new(650.0, 20.0));
        assert_eq!(sheet, original);
        assert_eq!(build_scene(&sheet, &layout), expanded);
    }
}

#[test]
fn visible_edge_dependencies_use_projected_path_centers_even_in_reverse_paint_order() {
    let (mut sheet, [pool, _, _, task, _, _, outside]) = nested();
    let crossing = edge(
        &mut sheet,
        Endpoint::Object(task),
        Endpoint::Object(outside),
    );
    sheet.canvas.scene.parents.insert(crossing, pool);
    let dependent = edge(
        &mut sheet,
        Endpoint::Object(crossing),
        Endpoint::Point(Vec2::new(1200.0, 400.0)),
    );
    let position = sheet
        .canvas
        .objects
        .iter()
        .position(|object| object.id == dependent)
        .unwrap();
    let object = sheet.canvas.objects.remove(position);
    sheet.canvas.objects.insert(0, object);
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let expanded = build_scene(&sheet, &layout);
    let original = sheet.clone();
    let preview = CollapsePreview::new(&sheet, &[(pool, compact())]).unwrap();
    let scene = preview.build_scene(&layout);
    let center = thinoda_core::board::polyline_midpoint(path(&scene, crossing)).unwrap();
    assert_eq!(path(&scene, dependent)[0], center);
    assert_ne!(path(&expanded, dependent)[0], center);
    assert_eq!(sheet, original);
    assert_eq!(
        CollapsePreview::new(&sheet, &[])
            .unwrap()
            .build_scene(&layout),
        expanded
    );
}

#[test]
fn hidden_internal_edges_fold_dependents_but_free_points_and_real_self_loops_are_not_rewritten() {
    let (mut sheet, [pool, _, _, task, peer, _, outside]) = nested();
    let internal = edge(&mut sheet, Endpoint::Object(task), Endpoint::Object(peer));
    sheet.canvas.scene.parents.remove(&internal);
    let dependent = edge(
        &mut sheet,
        Endpoint::Object(internal),
        Endpoint::Object(outside),
    );
    let to_hidden = edge(&mut sheet, Endpoint::Object(pool), Endpoint::Object(task));
    let real_loop = edge(&mut sheet, Endpoint::Object(pool), Endpoint::Object(pool));
    let free = Vec2::new(10.0, 10.0);
    let owned_free = edge(
        &mut sheet,
        Endpoint::Point(free),
        Endpoint::Point(Vec2::new(100.0, 200.0)),
    );
    sheet.canvas.scene.parents.insert(owned_free, pool);
    let layer_free = edge(
        &mut sheet,
        Endpoint::Point(free),
        Endpoint::Point(Vec2::new(100.0, 200.0)),
    );
    let crossing_free = edge(&mut sheet, Endpoint::Point(free), Endpoint::Object(outside));
    sheet.canvas.scene.parents.insert(crossing_free, pool);
    let card = sheet
        .add_card(
            "Visible card",
            Vec2::new(1000.0, 350.0),
            Card::new(CardKind::Sticky, Vec2::new(100.0, 60.0)),
        )
        .unwrap();
    let to_card = edge(&mut sheet, Endpoint::Object(task), Endpoint::Topic(card));
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let original = sheet.clone();
    let preview = CollapsePreview::new(&sheet, &[(pool, compact())]).unwrap();
    let scene = preview.build_scene(&layout);
    for id in [internal, to_hidden, owned_free] {
        assert_eq!(preview.visible_representative(id), Some(pool));
        assert!(scene.item(SceneId::Object(id)).is_none());
    }
    for id in [dependent, real_loop, layer_free, crossing_free, to_card] {
        assert_eq!(preview.visible_representative(id), Some(id));
        assert!(scene.item(SceneId::Object(id)).is_some());
    }
    assert!(on_border(compact(), path(&scene, dependent)[0]));
    assert!(on_border(compact(), path(&scene, to_card)[0]));
    assert!(scene.item(SceneId::Topic(card)).is_some());
    assert_eq!(path(&scene, crossing_free)[0], free);
    assert_eq!(sheet, original);
}

#[test]
fn moving_an_ancestor_carries_hidden_locked_geometry_once_and_keeps_manual_controls() {
    let (mut sheet, [pool, lane, sublane, task, peer, _, outside]) = nested();
    let crossing = edge(
        &mut sheet,
        Endpoint::Object(task),
        Endpoint::Object(outside),
    );
    let CanvasObjectKind::Connector(connector) =
        &mut sheet.canvas.object_mut(crossing).unwrap().kind
    else {
        panic!("connector")
    };
    connector.waypoints = vec![Vec2::new(540.0, 80.0), Vec2::new(600.0, 150.0)].into();
    sheet.canvas.object_mut(task).unwrap().locked = true;
    let before = sheet.clone();
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let collapsed = CollapsePreview::new(&sheet, &[(pool, compact())])
        .unwrap()
        .build_scene(&layout);
    assert!(path(&collapsed, crossing).contains(&Vec2::new(540.0, 80.0)));
    assert_eq!(sheet, before);
    let delta = Vec2::new(30.0, 50.0);
    sheet.move_canvas_objects(&[pool, task], delta).unwrap();
    for id in [pool, lane, sublane, task, peer] {
        assert_eq!(
            sheet.canvas.object(id).unwrap().rect,
            before.canvas.object(id).unwrap().rect.translate(delta)
        );
    }
    assert_eq!(
        sheet.canvas.object(crossing),
        before.canvas.object(crossing)
    );
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let moved = CollapsePreview::new(&sheet, &[(pool, compact().translate(delta))]).unwrap();
    assert_eq!(moved.object(pool).unwrap().rect, compact().translate(delta));
    let expanded = CollapsePreview::new(&sheet, &[])
        .unwrap()
        .build_scene(&layout);
    assert_eq!(
        expanded.item(SceneId::Object(task)).unwrap().bounds,
        before.canvas.object(task).unwrap().rect.translate(delta)
    );
}

#[test]
fn compact_shape_geometry_and_terminal_perimeters_use_the_same_resized_outline() {
    let (mut sheet, [pool, _, _, task, _, _, outside]) = nested();
    let CanvasObjectKind::Shape { shape, .. } = &mut sheet.canvas.object_mut(pool).unwrap().kind
    else {
        panic!("shape")
    };
    *shape = ShapeKind::Diamond;
    let crossing = edge(
        &mut sheet,
        Endpoint::Object(task),
        Endpoint::Object(outside),
    );
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let scene = CollapsePreview::new(&sheet, &[(pool, compact())])
        .unwrap()
        .build_scene(&layout);
    let SceneGeometry::Polygon { outline, .. } =
        &scene.item(SceneId::Object(pool)).unwrap().geometry
    else {
        panic!("shape")
    };
    assert!(distance_to_outline(path(&scene, crossing)[0], outline) < 0.001);
}

#[test]
fn compact_stencil_frames_share_the_interpreted_outline_with_projected_terminals() {
    let mut sheet = Sheet::new("Stencil collapse", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let package = thinoda_core::stencil_object::find_package("thinoda.general").unwrap();
    let frame = thinoda_core::stencil_object::new_object(
        package,
        package.definition("message").unwrap(),
        Vec2::ZERO,
    );
    let pool = sheet.add_canvas_object(frame).unwrap();
    sheet.set_canvas_container(pool, true).unwrap();
    let task = shape(
        &mut sheet,
        "Task",
        Rect::from_min_size(Vec2::new(40.0, 60.0), Vec2::new(80.0, 40.0)),
    );
    sheet.reparent_canvas_objects(&[task], Some(pool)).unwrap();
    let crossing = edge(
        &mut sheet,
        Endpoint::Object(task),
        Endpoint::Point(Vec2::new(600.0, 200.0)),
    );
    let original = sheet.clone();
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let preview = CollapsePreview::new(&sheet, &[(pool, compact())]).unwrap();
    let scene = preview.build_scene(&layout);
    let SceneGeometry::Polygon { outline, details } =
        &scene.item(SceneId::Object(pool)).unwrap().geometry
    else {
        panic!("stencil")
    };
    let geometry =
        thinoda_core::board::object_shape_geometry(preview.object(pool).unwrap()).unwrap();
    assert_eq!(outline, &geometry.outline);
    assert_eq!(details, &geometry.details);
    assert!(!details.is_empty());
    assert!(distance_to_outline(path(&scene, crossing)[0], outline) < 0.001);
    assert_eq!(sheet, original);
}

#[test]
fn maximum_connector_dependency_chain_is_folded_without_recursion_or_paint_order_assumptions() {
    let mut sheet = Sheet::new("Bounded collapse", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let (base, _) = nested();
    sheet.canvas.objects = base.canvas.objects[..4].to_vec();
    for (index, object) in sheet.canvas.objects.iter_mut().enumerate() {
        object.id = CanvasObjectId(index as u64 + 1);
    }
    sheet.canvas.objects[0].rect = Rect::from_min_size(Vec2::ZERO, Vec2::new(500.0, 300.0));
    sheet.canvas.objects[3].rect =
        Rect::from_min_size(Vec2::new(700.0, 140.0), Vec2::new(80.0, 40.0));
    let id = CanvasObjectId;
    sheet.canvas.scene.containers.insert(id(1));
    sheet.canvas.scene.parents.insert(id(2), id(1));
    sheet.canvas.scene.parents.insert(id(3), id(1));
    for number in 5..=MAX_CANVAS_OBJECTS as u64 {
        let mut object = CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(id(if number == 5 { 2 } else { number - 1 })),
            Endpoint::Object(id(if number == MAX_CANVAS_OBJECTS as u64 {
                4
            } else {
                3
            })),
        ));
        object.id = id(number);
        sheet.canvas.objects.push(object);
    }
    sheet.canvas.objects.reverse();
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let preview = CollapsePreview::new(&sheet, &[(id(1), compact())]).unwrap();
    assert_eq!(
        preview.visible_representative(id(MAX_CANVAS_OBJECTS as u64 - 1)),
        Some(id(1))
    );
    let scene = preview.build_scene(&layout);
    assert_eq!(
        scene
            .items
            .iter()
            .filter(|item| matches!(item.id, SceneId::Object(_)))
            .count(),
        3
    );
    assert!(on_border(
        compact(),
        path(&scene, id(MAX_CANVAS_OBJECTS as u64))[0]
    ));
}

#[test]
fn collapse_rejects_bad_frames_targets_budgets_and_invalid_graphs_without_touching_source() {
    let (sheet, [pool, lane, _, task, _, _, _]) = nested();
    let before = sheet.clone();
    for frame in [
        Rect::from_min_size(Vec2::ZERO, Vec2::ZERO),
        Rect::from_min_size(Vec2::ZERO, Vec2::new(-1.0, 40.0)),
        Rect::from_min_size(Vec2::ZERO, Vec2::new(f32::NAN, 40.0)),
        Rect::from_min_size(Vec2::new(999_999.0, 0.0), Vec2::new(100.0, 40.0)),
    ] {
        assert!(CollapsePreview::new(&sheet, &[(pool, frame)]).is_err());
    }
    for target in [task, CanvasObjectId(99999)] {
        assert!(CollapsePreview::new(&sheet, &[(target, compact())]).is_err());
    }
    assert!(
        CollapsePreview::new(&sheet, &[(pool, compact()), (pool, compact())])
            .unwrap_err()
            .contains("duplicate")
    );
    assert!(
        CollapsePreview::new(&sheet, &vec![(pool, compact()); MAX_CANVAS_OBJECTS + 1])
            .unwrap_err()
            .contains("limits")
    );
    assert_eq!(sheet, before);
    let mut invalid = sheet.clone();
    invalid.canvas.scene.parents.insert(pool, lane);
    assert!(
        CollapsePreview::new(&invalid, &[(pool, compact())])
            .unwrap_err()
            .contains("cycle")
    );
    let mut invalid = sheet.clone();
    let a = edge(
        &mut invalid,
        Endpoint::Point(Vec2::ZERO),
        Endpoint::Point(Vec2::new(10.0, 0.0)),
    );
    let b = edge(
        &mut invalid,
        Endpoint::Object(a),
        Endpoint::Point(Vec2::new(20.0, 0.0)),
    );
    let CanvasObjectKind::Connector(connector) = &mut invalid.canvas.object_mut(a).unwrap().kind
    else {
        panic!("connector")
    };
    connector.from = Endpoint::Object(b);
    assert!(
        CollapsePreview::new(&invalid, &[])
            .unwrap_err()
            .contains("cycle")
    );
    let mut weighted = sheet.clone();
    weighted.sankey = Some(Default::default());
    let layout = layout_sheet(&weighted, &ApproxMeasure, false);
    assert_eq!(
        CollapsePreview::new(&weighted, &[])
            .unwrap()
            .build_scene(&layout),
        build_scene(&weighted, &layout)
    );
    assert!(
        CollapsePreview::new(&weighted, &[(pool, compact())])
            .unwrap_err()
            .contains("Sankey")
    );
}

#[test]
fn maximum_ancestry_depth_projects_to_the_visible_outer_container() {
    let mut sheet = Sheet::new("Deep collapse", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let id = CanvasObjectId;
    for number in 1..=MAX_SCENE_DEPTH as u64 + 1 {
        let mut object = CanvasObject::shape(ShapeKind::Rectangle, compact());
        object.id = id(number);
        sheet.canvas.objects.push(object);
        if number <= MAX_SCENE_DEPTH as u64 {
            sheet.canvas.scene.containers.insert(id(number));
        }
        if number > 1 {
            sheet
                .canvas
                .scene
                .parents
                .insert(id(number), id(number - 1));
        }
    }
    let preview = CollapsePreview::new(
        &sheet,
        &[(id(1), compact()), (id(MAX_SCENE_DEPTH as u64), compact())],
    )
    .unwrap();
    assert_eq!(
        preview.visible_representative(id(MAX_SCENE_DEPTH as u64 + 1)),
        Some(id(1))
    );
}
