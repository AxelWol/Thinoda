use thinoda_core::board::{BoardMember, MAX_BOARD_COORDINATE, MAX_CANVAS_OBJECTS};
use thinoda_core::scene_hierarchy::{MAX_SCENE_DEPTH, SceneHierarchy};
use thinoda_core::{
    CanvasConnector, CanvasLayer, CanvasObject, CanvasObjectId, CanvasObjectKind, Card, CardKind,
    Color, Endpoint, InkStroke, InkTool, Rect, ShapeKind, Sheet, Vec2,
};

fn shape(id: u64) -> CanvasObject {
    let mut object = CanvasObject::shape(
        ShapeKind::Rectangle,
        Rect::from_center_size(Vec2::new(id as f32 * 10.0, 20.0), Vec2::new(8.0, 6.0)),
    );
    object.id = CanvasObjectId(id);
    object
}

fn layer(ids: &[u64]) -> CanvasLayer {
    CanvasLayer {
        objects: ids.iter().copied().map(shape).collect(),
        ..Default::default()
    }
}

fn id(value: u64) -> CanvasObjectId {
    CanvasObjectId(value)
}

fn connector(layer: &CanvasLayer, object: u64) -> &CanvasConnector {
    let CanvasObjectKind::Connector(connector) = &layer.object(id(object)).unwrap().kind else {
        panic!("expected connector");
    };
    connector
}

fn edge(number: u64, from: u64, to: Endpoint) -> CanvasObject {
    let mut connector = CanvasConnector::new(Endpoint::Object(id(from)), to);
    connector.waypoints = vec![Vec2::new(50.0, 60.0)].into();
    let mut object = CanvasObject::connector(connector);
    object.id = id(number);
    object
}

#[test]
fn nested_traversal_preserves_sibling_order_and_deduplicates_descendants() {
    let source = layer(&[4, 3, 1, 2, 5]);
    let tree = SceneHierarchy::new(
        &source,
        &[id(1), id(2)],
        &[(id(2), id(1)), (id(3), id(2)), (id(4), id(1))],
    )
    .unwrap();
    assert_eq!(tree.roots(), &[id(1), id(5)]);
    assert_eq!(tree.children(id(1)).unwrap(), &[id(4), id(2)]);
    assert_eq!(tree.preorder(), vec![id(1), id(4), id(2), id(3), id(5)]);
    assert_eq!(
        tree.descendant_closure(&[id(1), id(2), id(3), id(1)])
            .unwrap(),
        vec![id(4), id(3), id(1), id(2)]
    );
    assert_eq!(tree.parent(id(3)).unwrap(), Some(id(2)));
    assert!(tree.parent(id(99)).is_err());
    assert!(tree.children(id(3)).is_err());
    assert!(tree.descendant_closure(&[]).is_err());
    assert!(tree.descendant_closure(&[id(99)]).is_err());
}

#[test]
fn malformed_parent_trees_are_rejected() {
    let source = layer(&[1, 2, 3]);
    let cases = [
        (vec![id(99)], vec![], "not found"),
        (vec![id(1), id(1)], vec![], "duplicate"),
        (vec![id(1)], vec![(id(99), id(1))], "not found"),
        (vec![id(1)], vec![(id(2), id(3))], "not a container"),
        (vec![id(1)], vec![(id(2), id(99))], "not a container"),
        (vec![id(1)], vec![(id(1), id(1))], "cycle"),
        (
            vec![id(1), id(2)],
            vec![(id(1), id(2)), (id(2), id(1))],
            "cycle",
        ),
        (
            vec![id(1), id(2)],
            vec![(id(3), id(1)), (id(3), id(2))],
            "more than one parent",
        ),
    ];
    for (containers, parents, message) in cases {
        let error = SceneHierarchy::new(&source, &containers, &parents).unwrap_err();
        assert!(error.contains(message), "{error}");
    }
    let mut duplicate = source.clone();
    duplicate.objects[1].id = id(1);
    assert!(
        SceneHierarchy::new(&duplicate, &[], &[])
            .unwrap_err()
            .contains("duplicate canvas object")
    );
    let mut rotated = source.clone();
    rotated.objects[0].rotation = 90.0;
    assert!(
        SceneHierarchy::new(&rotated, &[id(1)], &[])
            .unwrap_err()
            .contains("rotated")
    );
    let mut non_shape = source.clone();
    non_shape.objects.push(edge(4, 1, Endpoint::Object(id(2))));
    assert!(
        SceneHierarchy::new(&non_shape, &[id(4)], &[])
            .unwrap_err()
            .contains("not a shape")
    );
}

#[test]
fn exact_depth_limit_and_next_parent_link_are_distinguished() {
    let ids: Vec<_> = (1..=MAX_SCENE_DEPTH as u64 + 2).collect();
    let source = layer(&ids);
    let containers: Vec<_> = ids.iter().copied().map(id).collect();
    let mut parents: Vec<_> = (2..=MAX_SCENE_DEPTH as u64 + 1)
        .map(|child| (id(child), id(child - 1)))
        .collect();
    let tree = SceneHierarchy::new(&source, &containers, &parents).unwrap();
    assert_eq!(tree.preorder().len(), source.objects.len());
    assert!(
        tree.preview_reparent(
            &[id(MAX_SCENE_DEPTH as u64 + 2)],
            Some(id(MAX_SCENE_DEPTH as u64 + 1))
        )
        .unwrap_err()
        .contains("exceeds 32 parent links")
    );
    assert_eq!(tree.parent(id(MAX_SCENE_DEPTH as u64 + 2)).unwrap(), None);
    parents.push((
        id(MAX_SCENE_DEPTH as u64 + 2),
        id(MAX_SCENE_DEPTH as u64 + 1),
    ));
    assert!(
        SceneHierarchy::new(&source, &containers, &parents)
            .unwrap_err()
            .contains("exceeds 32 parent links")
    );
}

#[test]
fn move_translates_nested_ink_and_internal_controls_once_but_not_crossing_controls() {
    let mut source = layer(&[1, 2, 3, 4]);
    let mut ink = CanvasObject::ink(InkStroke::new(
        InkTool::Pen,
        Color::BLACK,
        2.0,
        vec![Vec2::new(8.0, 9.0), Vec2::new(10.0, 11.0)],
    ));
    ink.id = id(5);
    source.objects.push(ink);
    source.objects.push(edge(6, 3, Endpoint::Object(id(5))));
    source.objects.push(edge(7, 3, Endpoint::Object(id(4))));
    source
        .objects
        .push(edge(8, 3, Endpoint::Point(Vec2::new(90.0, 100.0))));
    let before = source.clone();
    let tree = SceneHierarchy::new(
        &source,
        &[id(1), id(2)],
        &[
            (id(2), id(1)),
            (id(3), id(2)),
            (id(5), id(2)),
            (id(6), id(2)),
        ],
    )
    .unwrap();
    let delta = Vec2::new(15.0, -7.0);
    let moved = tree
        .preview_translation(&[id(1), id(2), id(3), id(6), id(1)], delta)
        .unwrap();
    for number in [1, 2, 3, 5, 6] {
        assert_eq!(
            moved.object(id(number)).unwrap().rect,
            source.object(id(number)).unwrap().rect.translate(delta)
        );
    }
    for number in [4, 7, 8] {
        assert_eq!(moved.object(id(number)), source.object(id(number)));
    }
    let CanvasObjectKind::Ink(ink) = &moved.object(id(5)).unwrap().kind else {
        panic!("expected ink");
    };
    assert_eq!(&*ink.points, &[Vec2::new(23.0, 2.0), Vec2::new(25.0, 4.0)]);
    assert_eq!(&*connector(&moved, 6).waypoints, &[Vec2::new(65.0, 53.0)]);
    assert_eq!(connector(&moved, 6).from, Endpoint::Object(id(3)));
    assert_eq!(connector(&moved, 6).to, Endpoint::Object(id(5)));
    assert_eq!(source, before);
}

#[test]
fn layer_owned_internal_edges_follow_endpoints_and_selected_free_ends_follow_the_edge() {
    let mut source = layer(&[1, 2, 3]);
    source.objects.push(edge(8, 5, Endpoint::Object(id(3))));
    source.objects.push(edge(7, 6, Endpoint::Object(id(3))));
    source.objects.push(edge(6, 4, Endpoint::Object(id(3))));
    source.objects.push(edge(4, 2, Endpoint::Object(id(3))));
    source
        .objects
        .push(edge(5, 2, Endpoint::Point(Vec2::new(90.0, 100.0))));
    let tree = SceneHierarchy::new(&source, &[id(1)], &[(id(2), id(1)), (id(3), id(1))]).unwrap();
    let delta = Vec2::new(5.0, 6.0);
    let moved = tree.preview_translation(&[id(1), id(5)], delta).unwrap();
    for number in [4, 5, 6, 7, 8] {
        assert_eq!(
            &*connector(&moved, number).waypoints,
            &[Vec2::new(55.0, 66.0)]
        );
        assert_eq!(
            connector(&moved, number).from,
            connector(&source, number).from
        );
    }
    assert_eq!(
        connector(&moved, 5).to,
        Endpoint::Point(Vec2::new(95.0, 106.0))
    );
}

#[test]
fn moved_controls_with_stationary_terminals_do_not_imply_rigid_downstream_movement() {
    let mut source = layer(&[1, 2, 3]);
    source.objects.push(edge(4, 2, Endpoint::Object(id(3))));
    source.objects.push(edge(5, 4, Endpoint::Object(id(2))));
    let tree = SceneHierarchy::new(&source, &[id(1)], &[(id(2), id(1))]).unwrap();
    let moved = tree
        .preview_translation(&[id(1), id(4)], Vec2::new(5.0, 6.0))
        .unwrap();
    assert_eq!(&*connector(&moved, 4).waypoints, &[Vec2::new(55.0, 66.0)]);
    assert_eq!(moved.object(id(5)), source.object(id(5)));
    assert_eq!(moved.object(id(3)), source.object(id(3)));
}

#[test]
fn reparent_retains_geometry_and_nested_selection_structure_without_mutating_the_source() {
    let source = layer(&[1, 2, 3, 4]);
    let before = source.clone();
    let tree = SceneHierarchy::new(
        &source,
        &[id(1), id(2), id(4)],
        &[(id(2), id(1)), (id(3), id(2))],
    )
    .unwrap();
    let moved = tree.preview_reparent(&[id(2), id(3)], Some(id(4))).unwrap();
    assert_eq!(moved.parent(id(2)).unwrap(), Some(id(4)));
    assert_eq!(moved.parent(id(3)).unwrap(), Some(id(2)));
    assert_eq!(moved.preorder(), vec![id(1), id(4), id(2), id(3)]);
    assert_eq!(
        moved.preview_translation(&[id(2)], Vec2::ZERO).unwrap(),
        before
    );
    assert_eq!(tree.parent(id(2)).unwrap(), Some(id(1)));
    let detached = tree.preview_reparent(&[id(3)], None).unwrap();
    assert_eq!(detached.parent(id(3)).unwrap(), None);
    assert_eq!(source, before);
}

#[test]
fn invalid_reparent_and_out_of_bounds_moves_leave_source_unchanged() {
    let mut source = layer(&[1, 2, 3]);
    source.objects[2].rect.min.x = MAX_BOARD_COORDINATE - 10.0;
    let before = source.clone();
    let tree =
        SceneHierarchy::new(&source, &[id(1), id(2)], &[(id(2), id(1)), (id(3), id(2))]).unwrap();
    assert!(tree.preview_reparent(&[id(1)], Some(id(2))).is_err());
    assert!(tree.preview_reparent(&[id(2)], Some(id(2))).is_err());
    assert!(tree.preview_reparent(&[id(3)], Some(id(99))).is_err());
    assert!(tree.preview_reparent(&[], None).is_err());
    assert!(
        tree.preview_translation(&[id(1)], Vec2::new(20.0, 0.0))
            .is_err()
    );
    assert!(
        tree.preview_translation(&[id(1)], Vec2::new(f32::NAN, 0.0))
            .is_err()
    );
    assert_eq!(source, before);
}

#[test]
fn locked_descendants_follow_parent_but_cannot_be_moved_or_reparented_directly() {
    let mut source = layer(&[1, 2, 3, 4]);
    source.objects[2].locked = true;
    let tree = SceneHierarchy::new(
        &source,
        &[id(1), id(2), id(4)],
        &[(id(2), id(1)), (id(3), id(2))],
    )
    .unwrap();
    let moved = tree
        .preview_translation(&[id(1), id(3)], Vec2::new(10.0, 20.0))
        .unwrap();
    assert_eq!(
        moved.object(id(3)).unwrap().rect.min,
        source.object(id(3)).unwrap().rect.min + Vec2::new(10.0, 20.0)
    );
    assert!(moved.object(id(3)).unwrap().locked);
    assert!(tree.preview_translation(&[id(3)], Vec2::ZERO).is_err());
    assert!(tree.preview_reparent(&[id(3)], Some(id(4))).is_err());
    assert!(tree.preview_reparent(&[id(1), id(3)], Some(id(4))).is_ok());

    source.objects[0].locked = true;
    let tree = SceneHierarchy::new(
        &source,
        &[id(1), id(2), id(4)],
        &[(id(2), id(1)), (id(3), id(2))],
    )
    .unwrap();
    assert!(
        tree.preview_translation(&[id(2)], Vec2::new(1.0, 0.0))
            .is_err()
    );
    assert!(tree.preview_reparent(&[id(2)], None).is_err());
    assert!(tree.preview_reparent(&[id(4)], Some(id(1))).is_err());
}

#[test]
fn prototype_refuses_flat_group_membership_and_does_not_split_mixed_card_groups() {
    let mut sheet = Sheet::new("Board", "Root");
    let first = sheet.add_canvas_object(shape(1)).unwrap();
    let second = sheet.add_canvas_object(shape(2)).unwrap();
    let group = sheet.group_canvas_objects(&[first, second]).unwrap();
    let tree = SceneHierarchy::new(&sheet.canvas, &[], &[]).unwrap();
    let delta = Vec2::new(10.0, 20.0);
    let moved = tree.preview_translation(&[first], delta).unwrap();
    for object in &sheet.canvas.objects {
        assert_eq!(
            moved.object(object.id).unwrap().rect,
            object.rect.translate(delta)
        );
    }
    assert!(SceneHierarchy::new(&sheet.canvas, &[first], &[(second, first)]).is_err());
    sheet.set_canvas_group_locked(group, true).unwrap();
    let tree = SceneHierarchy::new(&sheet.canvas, &[], &[]).unwrap();
    assert!(tree.preview_translation(&[first], delta).is_err());

    sheet.set_canvas_group_locked(group, false).unwrap();
    sheet.ungroup_canvas_objects(group).unwrap();
    let card = sheet
        .add_card(
            "Card",
            Vec2::new(50.0, 60.0),
            Card::new(CardKind::Sticky, Vec2::new(40.0, 20.0)),
        )
        .unwrap();
    sheet
        .group_board_members(&[BoardMember::Object(first), BoardMember::Card(card)])
        .unwrap();
    let tree = SceneHierarchy::new(&sheet.canvas, &[], &[]).unwrap();
    assert!(
        tree.preview_translation(&[first], delta)
            .unwrap_err()
            .contains("scene occurrence adapter")
    );
}

#[test]
fn maximum_canvas_size_and_depth_remain_iterative_and_reject_one_more_object() {
    let ids: Vec<_> = (1..=MAX_CANVAS_OBJECTS as u64).collect();
    let mut source = layer(&ids);
    let containers: Vec<_> = (1..=MAX_SCENE_DEPTH as u64).map(id).collect();
    let parents: Vec<_> = ids
        .iter()
        .copied()
        .skip(1)
        .map(|child| (id(child), id((child - 1).min(MAX_SCENE_DEPTH as u64))))
        .collect();
    let tree = SceneHierarchy::new(&source, &containers, &parents).unwrap();
    assert_eq!(tree.preorder().len(), MAX_CANVAS_OBJECTS);
    assert_eq!(
        tree.descendant_closure(&[id(1)]).unwrap().len(),
        MAX_CANVAS_OBJECTS
    );
    let moved = tree
        .preview_translation(&[id(1)], Vec2::new(1.0, 2.0))
        .unwrap();
    assert_eq!(
        moved.objects.last().unwrap().rect,
        source
            .objects
            .last()
            .unwrap()
            .rect
            .translate(Vec2::new(1.0, 2.0))
    );
    source.objects.push(shape(MAX_CANVAS_OBJECTS as u64 + 1));
    assert!(SceneHierarchy::new(&source, &[], &[]).is_err());
}
