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

#[test]
fn stored_nested_collapse_moves_and_expands_atomically_with_history() {
    use thinoda_core::{Document, Workbook};
    let (mut sheet, [pool, _, sublane, task, _, _, outside]) = nested();
    let crossing = edge(
        &mut sheet,
        Endpoint::Object(task),
        Endpoint::Object(outside),
    );
    sheet.canvas.object_mut(task).unwrap().locked = true;
    let original = sheet.clone();
    let mut doc = Document::new(Workbook::new(sheet));
    let inner = doc.sheet().canvas.object(sublane).unwrap().rect;
    doc.set_canvas_container_collapsed(
        sublane,
        Some(Rect::from_min_size(inner.min, Vec2::new(300.0, 30.0))),
    )
    .unwrap();
    doc.set_canvas_container_collapsed(pool, Some(compact()))
        .unwrap();
    let collapsed = doc.sheet().clone();
    let layout = layout_sheet(doc.sheet(), &ApproxMeasure, false);
    let scene = build_scene(doc.sheet(), &layout);
    assert!(scene.item(SceneId::Object(task)).is_none());
    assert!(on_border(compact(), path(&scene, crossing)[0]));
    assert_eq!(
        doc.sheet().canvas.scene.collapsed[&pool],
        original.canvas.object(pool).unwrap().rect
    );
    assert_eq!(
        doc.sheet().canvas.object(task),
        original.canvas.object(task)
    );
    assert_eq!(
        thinoda_core::scene_hierarchy::SceneHierarchy::from_canvas(&doc.sheet().canvas)
            .unwrap()
            .navigate(
                pool,
                thinoda_core::scene_hierarchy::SceneNavigation::FirstChild
            )
            .unwrap(),
        None
    );
    let delta = Vec2::new(25.0, 35.0);
    doc.move_canvas_objects(&[pool, task], delta).unwrap();
    let moved = doc.sheet().clone();
    assert_eq!(
        moved.canvas.scene.collapsed[&pool],
        original.canvas.object(pool).unwrap().rect.translate(delta)
    );
    assert!(doc.undo());
    assert_eq!(doc.sheet(), &collapsed);
    assert!(doc.redo());
    assert_eq!(doc.sheet(), &moved);
    doc.set_canvas_container_collapsed(pool, None).unwrap();
    assert!(doc.sheet().canvas.scene.collapsed.contains_key(&sublane));
    assert!(!doc.sheet().canvas.scene.collapsed.contains_key(&pool));
    doc.set_canvas_container_collapsed(sublane, None).unwrap();
    let mut expected = original;
    expected.move_canvas_objects(&[pool, task], delta).unwrap();
    assert_eq!(doc.sheet(), &expected);
    assert!(doc.undo());
    assert!(doc.sheet().canvas.scene.collapsed.contains_key(&sublane));
    doc.remove_canvas_objects(&[pool]).unwrap();
    assert!(doc.sheet().canvas.scene.collapsed.is_empty());
    assert!(doc.undo());
    assert!(doc.sheet().canvas.scene.collapsed.contains_key(&sublane));
}

#[test]
fn stored_collapse_rejects_bad_frames_locks_resize_and_malformed_saved_state() {
    let (mut sheet, [pool, _, _, task, _, _, _]) = nested();
    let original = sheet.clone();
    for frame in [
        Rect::from_min_size(Vec2::ZERO, Vec2::ZERO),
        compact().translate(Vec2::new(f32::INFINITY, 0.0)),
        Rect::from_min_size(Vec2::ZERO, Vec2::new(f32::NAN, 40.0)),
    ] {
        assert!(
            sheet
                .set_canvas_container_collapsed(pool, Some(frame))
                .is_err()
        );
        assert_eq!(sheet, original);
    }
    assert!(
        sheet
            .set_canvas_container_collapsed(task, Some(compact()))
            .is_err()
    );
    sheet.canvas.object_mut(pool).unwrap().locked = true;
    assert!(
        sheet
            .set_canvas_container_collapsed(pool, Some(compact()))
            .is_err()
    );
    sheet.canvas.object_mut(pool).unwrap().locked = false;
    sheet
        .set_canvas_container_collapsed(pool, Some(compact()))
        .unwrap();
    let collapsed = sheet.clone();
    sheet
        .set_canvas_container_collapsed(pool, Some(compact()))
        .unwrap();
    assert_eq!(sheet, collapsed);
    assert!(
        sheet
            .resize_canvas_object(pool, compact())
            .unwrap_err()
            .contains("expand")
    );
    assert!(
        sheet
            .set_canvas_container(pool, false)
            .unwrap_err()
            .contains("expand")
    );
    assert!(
        sheet
            .set_canvas_container_collapsed(
                pool,
                Some(Rect::from_min_size(Vec2::ZERO, Vec2::new(400.0, 40.0)))
            )
            .is_err()
    );
    assert_eq!(sheet, collapsed);
    for (id, frame) in [
        (task, compact()),
        (CanvasObjectId(99999), compact()),
        (pool, compact().translate(Vec2::new(999_999.0, 0.0))),
        (pool, Rect::from_min_size(Vec2::ZERO, Vec2::new(-1.0, 40.0))),
    ] {
        let mut bad = collapsed.clone();
        bad.canvas.scene.collapsed.insert(id, frame);
        assert!(bad.validate().is_err());
    }
    let mut bad = collapsed.clone();
    bad.canvas.object_mut(pool).unwrap().rect.size.x = 0.0;
    assert!(bad.validate().is_err());
    let mut bad = collapsed.clone();
    bad.sankey = Some(Default::default());
    assert!(bad.validate().unwrap_err().contains("Sankey"));
    sheet.set_canvas_container_collapsed(pool, None).unwrap();
    assert_eq!(sheet, original);
    sheet.canvas.object_mut(pool).unwrap().rect.size.x = 999_990.0;
    sheet
        .set_canvas_container_collapsed(pool, Some(compact()))
        .unwrap();
    let before = sheet.clone();
    assert!(
        sheet
            .move_canvas_objects(&[pool], Vec2::new(20.0, 0.0))
            .is_err()
    );
    assert_eq!(sheet, before);
}

#[test]
fn offset_compact_frames_restore_expanded_origins_after_movement() {
    let (mut sheet, [pool, _, _, task, _, _, _]) = nested();
    let original = sheet.clone();
    let frame = Rect::from_min_size(Vec2::new(460.0, 0.0), Vec2::new(40.0, 300.0));
    sheet
        .set_canvas_container_collapsed(pool, Some(frame))
        .unwrap();
    assert_eq!(sheet.canvas.object(task), original.canvas.object(task));
    let encoded = thinoda_core::clipboard::encode_sheet(&sheet).unwrap();
    assert_eq!(
        thinoda_core::clipboard::decode_sheet(&encoded).unwrap(),
        sheet
    );
    let delta = Vec2::new(-40.0, 80.0);
    sheet.move_canvas_objects(&[pool], delta).unwrap();
    assert_eq!(
        sheet.canvas.object(pool).unwrap().rect,
        frame.translate(delta)
    );
    assert_eq!(
        sheet.canvas.scene.collapsed[&pool],
        original.canvas.object(pool).unwrap().rect.translate(delta)
    );
    sheet.set_canvas_container_collapsed(pool, None).unwrap();
    let mut expected = original;
    expected.move_canvas_objects(&[pool], delta).unwrap();
    assert_eq!(sheet, expected);
}

#[test]
fn stored_collapse_bounds_and_waypoint_helpers_follow_the_visible_route() {
    let mut sheet = Sheet::new("Collapse helpers", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let pool = shape(&mut sheet, "Pool", compact());
    let task = shape(
        &mut sheet,
        "Far hidden task",
        Rect::from_min_size(Vec2::new(1000.0, 1000.0), Vec2::new(80.0, 40.0)),
    );
    sheet.set_canvas_container(pool, true).unwrap();
    sheet.reparent_canvas_objects(&[task], Some(pool)).unwrap();
    let mut connector = CanvasConnector::new(
        Endpoint::Object(task),
        Endpoint::Point(Vec2::new(600.0, 100.0)),
    );
    connector.route = ConnectorRoute::Straight;
    let crossing = sheet
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    sheet
        .set_canvas_container_collapsed(pool, Some(compact()))
        .unwrap();
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let scene = build_scene(&sheet, &layout);
    assert_eq!(
        thinoda_core::scene::canvas_bounds(&sheet, &layout),
        scene.bounds()
    );
    assert!(
        thinoda_core::scene::object_bounds(&sheet, &layout, sheet.canvas.object(task).unwrap())
            .is_none()
    );
    assert_eq!(
        thinoda_core::scene::object_bounds(&sheet, &layout, sheet.canvas.object(crossing).unwrap()),
        scene
            .item(SceneId::Object(crossing))
            .unwrap()
            .visible_bounds(),
    );
    let mut preview = sheet.canvas.object(pool).unwrap().clone();
    preview.rect = preview.rect.translate(Vec2::new(100.0, -30.0));
    assert_eq!(
        thinoda_core::scene::object_bounds(&sheet, &layout, &preview),
        Some(preview.rect)
    );
    let CanvasObjectKind::Connector(connector) = &sheet.canvas.object(crossing).unwrap().kind
    else {
        panic!("connector")
    };
    assert_eq!(
        thinoda_core::scene::connector_path(&sheet, &layout, connector),
        path(&scene, crossing)
    );
    let handles = thinoda_core::scene::connector_insert_handles(&sheet, &layout, connector);
    assert_eq!(handles.len(), 1);
    assert_eq!(
        handles[0].point,
        thinoda_core::board::polyline_midpoint(path(&scene, crossing)).unwrap()
    );
}

#[test]
fn stored_collapse_clipboards_preserve_hidden_geometry_remap_ids_and_refuse_downgrades() {
    use thinoda_core::clipboard;
    let (mut sheet, [pool, _, sublane, task, _, _, _]) = nested();
    let inner = sheet.canvas.object(sublane).unwrap().rect;
    sheet
        .set_canvas_container_collapsed(
            sublane,
            Some(Rect::from_min_size(inner.min, Vec2::new(300.0, 30.0))),
        )
        .unwrap();
    sheet
        .set_canvas_container_collapsed(pool, Some(compact()))
        .unwrap();
    let encoded = clipboard::encode_sheet(&sheet).unwrap();
    assert!(encoded.starts_with("THINODA-SHEET-24\n"));
    assert_eq!(clipboard::decode_sheet(&encoded).unwrap(), sheet);
    assert_eq!(
        clipboard::decode_sheet(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
        sheet
    );
    assert!(
        clipboard::decode_sheet(&encoded.replacen("SHEET-24", "SHEET-23", 1))
            .unwrap_err()
            .contains("collapsed")
    );
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let payload = clipboard::copy_board_selection(&sheet, &layout, &[], &[pool])
        .unwrap()
        .unwrap();
    let encoded = clipboard::encode_board_payload(&payload).unwrap();
    assert!(encoded.starts_with("THINODA-BOARD-CLIPBOARD-23\n"));
    assert!(
        clipboard::decode_board_payload(&encoded.replacen("CLIPBOARD-23", "CLIPBOARD-22", 1))
            .unwrap_err()
            .contains("collapsed")
    );
    let payload = clipboard::decode_board_payload(&encoded.replacen('\n', "\r\n", 1)).unwrap();
    let mut pasted = Sheet::new("Paste", "");
    shape(&mut pasted, "Existing", compact());
    let result =
        clipboard::paste_board_selection(&mut pasted, &payload, Vec2::new(1000.0, 500.0)).unwrap();
    let copied_pool = result.objects[0];
    let copied_task = pasted.canvas.objects.iter().find(|object| matches!(&object.kind, CanvasObjectKind::Shape { label, .. } if label == "Task")).unwrap().id;
    let delta = pasted.canvas.object(copied_pool).unwrap().rect.min - compact().min;
    assert_ne!(copied_pool, pool);
    assert_eq!(pasted.canvas.scene.collapsed.len(), 2);
    assert_eq!(
        pasted.canvas.scene.collapsed[&copied_pool],
        sheet.canvas.scene.collapsed[&pool].translate(delta)
    );
    assert_eq!(
        pasted.canvas.object(copied_task).unwrap().rect,
        sheet.canvas.object(task).unwrap().rect.translate(delta)
    );
    pasted
        .set_canvas_container_collapsed(copied_pool, None)
        .unwrap();
    assert_eq!(
        pasted.canvas.object(copied_pool).unwrap().rect,
        sheet.canvas.scene.collapsed[&pool].translate(delta)
    );
    pasted.validate().unwrap();
}
