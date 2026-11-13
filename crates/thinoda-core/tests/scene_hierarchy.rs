use std::cell::Cell;
use thinoda_core::board::{BoardMember, MAX_BOARD_COORDINATE, MAX_CANVAS_OBJECTS};
use thinoda_core::scene_hierarchy::{
    MAX_SCENE_DEPTH, SceneEditKind, SceneHierarchy, SceneParentChange,
};
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
    let stored = moved.preview_translation(&[id(2)], Vec2::ZERO).unwrap();
    for object in &before.objects {
        assert_eq!(stored.object(object.id), Some(object));
    }
    assert_eq!(
        SceneHierarchy::from_canvas(&stored).unwrap().preorder(),
        moved.preorder()
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

#[test]
fn reparent_appends_in_current_traversal_order_and_preserves_order_across_commands() {
    let source = layer(&[3, 4, 5, 1, 2, 6]);
    let tree = SceneHierarchy::new(
        &source,
        &[id(1), id(2)],
        &[
            (id(3), id(1)),
            (id(4), id(1)),
            (id(5), id(1)),
            (id(6), id(2)),
        ],
    )
    .unwrap();
    let moved = tree.preview_reparent(&[id(4), id(3)], Some(id(2))).unwrap();
    assert_eq!(moved.children(id(1)).unwrap(), &[id(5)]);
    assert_eq!(moved.children(id(2)).unwrap(), &[id(6), id(3), id(4)]);
    let same_parent = moved.preview_reparent(&[id(3)], Some(id(2))).unwrap();
    assert_eq!(same_parent.preorder(), moved.preorder());
    let next = same_parent.preview_reparent(&[id(5)], Some(id(2))).unwrap();
    assert_eq!(next.children(id(2)).unwrap(), &[id(6), id(3), id(4), id(5)]);
    let detached = next.preview_reparent(&[id(4)], None).unwrap();
    assert_eq!(detached.roots(), &[id(1), id(2), id(4)]);
    assert_eq!(detached.children(id(2)).unwrap(), &[id(6), id(3), id(5)]);
    assert_eq!(tree.children(id(1)).unwrap(), &[id(3), id(4), id(5)]);
}

#[test]
fn endpoint_reparent_reconciles_only_affected_edges_and_preserves_controls() {
    let mut source = layer(&[1, 2, 3, 4, 5, 6, 7, 11]);
    source.objects.push(edge(8, 3, Endpoint::Object(id(7))));
    source.objects.push(edge(9, 3, Endpoint::Object(id(6))));
    source.objects.push(edge(10, 7, Endpoint::Object(id(11))));
    source.objects.push(edge(12, 6, Endpoint::Object(id(6))));
    source
        .objects
        .push(edge(13, 3, Endpoint::Point(Vec2::new(200.0, 300.0))));
    let before = source.clone();
    let tree = SceneHierarchy::new(
        &source,
        &[id(1), id(2), id(4), id(5)],
        &[
            (id(2), id(1)),
            (id(3), id(2)),
            (id(7), id(2)),
            (id(11), id(2)),
            (id(5), id(4)),
            (id(6), id(5)),
            (id(8), id(2)),
            (id(10), id(4)),
            (id(12), id(5)),
            (id(13), id(2)),
        ],
    )
    .unwrap();
    let moved = tree
        .preview_reparent_with_validator(&[id(3)], Some(id(5)), |change| {
            assert_eq!(
                change.kind,
                SceneEditKind::Reparent {
                    target: Some(id(5))
                }
            );
            assert_eq!(change.selection, &[id(3)]);
            assert_eq!(change.affected, &[id(3), id(8), id(13), id(9)]);
            assert_eq!(
                change.edge_parent_changes,
                &[
                    SceneParentChange {
                        object: id(8),
                        from: Some(id(2)),
                        to: None
                    },
                    SceneParentChange {
                        object: id(13),
                        from: Some(id(2)),
                        to: None
                    },
                    SceneParentChange {
                        object: id(9),
                        from: None,
                        to: Some(id(5))
                    },
                ]
            );
            assert_eq!(change.geometry, &before);
            Ok(())
        })
        .unwrap();
    assert_eq!(moved.parent(id(8)).unwrap(), None);
    assert_eq!(moved.parent(id(13)).unwrap(), None);
    assert_eq!(moved.parent(id(9)).unwrap(), Some(id(5)));
    assert_eq!(moved.parent(id(10)).unwrap(), Some(id(4)));
    assert_eq!(
        moved.children(id(5)).unwrap(),
        &[id(6), id(12), id(3), id(9)]
    );
    assert_eq!(moved.roots(), &[id(1), id(4), id(8), id(13)]);
    assert_eq!(source, before);
}

#[test]
fn common_container_includes_container_endpoints_but_not_ordinary_objects() {
    let source = layer(&[1, 2, 3, 4, 5]);
    let tree = SceneHierarchy::new(
        &source,
        &[id(1), id(2), id(5)],
        &[(id(2), id(1)), (id(3), id(2)), (id(4), id(2))],
    )
    .unwrap();
    assert_eq!(
        tree.nearest_common_container(id(3), id(4)).unwrap(),
        Some(id(2))
    );
    assert_eq!(
        tree.nearest_common_container(id(2), id(3)).unwrap(),
        Some(id(2))
    );
    assert_eq!(
        tree.nearest_common_container(id(1), id(3)).unwrap(),
        Some(id(1))
    );
    assert_eq!(
        tree.nearest_common_container(id(3), id(3)).unwrap(),
        Some(id(2))
    );
    assert_eq!(
        tree.nearest_common_container(id(5), id(5)).unwrap(),
        Some(id(5))
    );
    assert_eq!(tree.nearest_common_container(id(3), id(5)).unwrap(), None);
    assert!(tree.nearest_common_container(id(3), id(99)).is_err());
}

#[test]
fn edge_parent_dependencies_are_reconciled_before_stable_insertion_not_by_source_order() {
    let mut source = layer(&[1, 2, 3, 4, 5]);
    for (number, from, to) in [(8, 7, 5), (7, 6, 5), (6, 2, 5), (9, 5, 5), (10, 6, 6)] {
        source
            .objects
            .push(edge(number, from, Endpoint::Object(id(to))));
    }
    let tree = SceneHierarchy::new(
        &source,
        &[id(1), id(4)],
        &[
            (id(2), id(1)),
            (id(3), id(1)),
            (id(5), id(4)),
            (id(9), id(4)),
        ],
    )
    .unwrap();
    let moved = tree
        .preview_reparent_with_validator(&[id(2)], Some(id(4)), |change| {
            assert_eq!(
                change
                    .edge_parent_changes
                    .iter()
                    .map(|change| change.object)
                    .collect::<Vec<_>>(),
                vec![id(8), id(7), id(6), id(10)]
            );
            Ok(())
        })
        .unwrap();
    for number in [6, 7, 8, 10] {
        assert_eq!(moved.parent(id(number)).unwrap(), Some(id(4)));
    }
    assert_eq!(
        moved.children(id(4)).unwrap(),
        &[id(5), id(9), id(2), id(8), id(7), id(6), id(10)]
    );
    let returned = moved.preview_reparent(&[id(2)], Some(id(1))).unwrap();
    assert_eq!(
        returned.roots(),
        &[id(1), id(4), id(8), id(7), id(6), id(10)]
    );
    assert_eq!(returned.children(id(4)).unwrap(), &[id(5), id(9)]);
}

#[test]
fn locked_external_edge_or_owner_rejects_reconciliation_without_partial_changes() {
    let mut source = layer(&[1, 2, 3, 4, 6]);
    let mut external = edge(5, 2, Endpoint::Object(id(3)));
    external.locked = true;
    source.objects.push(external);
    let before = source.clone();
    let tree = SceneHierarchy::new(
        &source,
        &[id(1), id(4), id(6)],
        &[(id(2), id(1)), (id(3), id(4))],
    )
    .unwrap();
    assert!(
        tree.preview_reparent(&[id(2)], Some(id(4)))
            .unwrap_err()
            .contains("locked")
    );
    assert_eq!(tree.parent(id(2)).unwrap(), Some(id(1)));
    assert_eq!(source, before);

    source.object_mut(id(5)).unwrap().locked = false;
    source.object_mut(id(6)).unwrap().locked = true;
    let tree = SceneHierarchy::new(
        &source,
        &[id(1), id(4), id(6)],
        &[(id(2), id(1)), (id(3), id(4)), (id(5), id(6))],
    )
    .unwrap();
    assert!(
        tree.preview_reparent(&[id(2)], Some(id(4)))
            .unwrap_err()
            .contains("locked")
    );
    assert_eq!(tree.parent(id(5)).unwrap(), Some(id(6)));
}

#[test]
fn carried_locked_edge_keeps_lock_state_during_induced_owner_change() {
    let mut source = layer(&[1, 2, 3, 4]);
    let mut carried = edge(5, 3, Endpoint::Object(id(4)));
    carried.locked = true;
    source.objects.push(carried);
    let tree = SceneHierarchy::new(
        &source,
        &[id(1), id(2), id(4)],
        &[(id(2), id(1)), (id(3), id(2)), (id(5), id(2))],
    )
    .unwrap();
    let moved = tree
        .preview_reparent_with_validator(&[id(2)], Some(id(4)), |change| {
            assert!(change.geometry.object(id(5)).unwrap().locked);
            assert_eq!(
                change.edge_parent_changes,
                &[SceneParentChange {
                    object: id(5),
                    from: Some(id(2)),
                    to: Some(id(4)),
                }]
            );
            Ok(())
        })
        .unwrap();
    assert_eq!(moved.parent(id(5)).unwrap(), Some(id(4)));
    assert_eq!(tree.parent(id(5)).unwrap(), Some(id(2)));
}

#[test]
fn binding_validator_observes_inherited_scope_changes_and_can_reject_the_whole_preview() {
    let source = layer(&[1, 2, 3, 4, 5]);
    let before = source.clone();
    let tree = SceneHierarchy::new(
        &source,
        &[id(1), id(2), id(3), id(5)],
        &[(id(2), id(1)), (id(3), id(2)), (id(4), id(3))],
    )
    .unwrap();
    let scope = |tree: &SceneHierarchy<'_>, object| {
        let mut cursor = Some(object);
        while let Some(current) = cursor {
            if [id(1), id(5)].contains(&current) {
                return Some(current);
            }
            cursor = tree.parent(current).unwrap();
        }
        None
    };
    let calls = Cell::new(0);
    let result = tree.preview_reparent_with_validator(&[id(2)], Some(id(5)), |change| {
        calls.set(calls.get() + 1);
        assert_eq!(change.affected, &[id(2), id(3), id(4)]);
        assert_eq!(scope(change.before, id(4)), Some(id(1)));
        assert_eq!(scope(change.after, id(4)), Some(id(5)));
        Err("scope change requires confirmation".into())
    });
    assert_eq!(result.unwrap_err(), "scope change requires confirmation");
    assert_eq!(calls.get(), 1);
    assert_eq!(scope(&tree, id(4)), Some(id(1)));
    assert_eq!(source, before);
    let accepted = tree
        .preview_reparent_with_validator(&[id(2)], Some(id(5)), |_| Ok(()))
        .unwrap();
    assert_eq!(scope(&accepted, id(4)), Some(id(5)));
}

#[test]
fn shared_binding_hook_sees_geometry_only_after_successful_structural_validation() {
    let source = layer(&[1, 2, 3]);
    let before = source.clone();
    let tree =
        SceneHierarchy::new(&source, &[id(1), id(2)], &[(id(2), id(1)), (id(3), id(2))]).unwrap();
    let delta = Vec2::new(10.0, 20.0);
    let calls = Cell::new(0);
    let result = tree.preview_translation_with_validator(&[id(2)], delta, |change| {
        calls.set(calls.get() + 1);
        assert_eq!(change.kind, SceneEditKind::Translate { delta });
        assert_eq!(change.affected, &[id(2), id(3)]);
        assert_eq!(change.before.preorder(), change.after.preorder());
        assert_eq!(
            change.geometry.object(id(3)).unwrap().rect,
            source.object(id(3)).unwrap().rect.translate(delta)
        );
        assert!(change.edge_parent_changes.is_empty());
        Err("bound geometry preview rejected".into())
    });
    assert_eq!(result.unwrap_err(), "bound geometry preview rejected");
    assert_eq!(calls.get(), 1);
    assert_eq!(source, before);
    assert!(
        tree.preview_reparent_with_validator(&[id(1)], Some(id(2)), |_| {
            calls.set(calls.get() + 1);
            Ok(())
        })
        .is_err()
    );
    assert!(
        tree.preview_translation_with_validator(&[id(1)], Vec2::new(f32::NAN, 0.0), |_| {
            calls.set(calls.get() + 1);
            Ok(())
        })
        .is_err()
    );
    assert_eq!(calls.get(), 1);
}

#[test]
fn induced_edge_depth_overflow_is_rejected_before_binding_validation() {
    let deepest = MAX_SCENE_DEPTH as u64 + 1;
    let mut source = layer(&(1..=deepest).collect::<Vec<_>>());
    source
        .objects
        .push(edge(deepest + 1, deepest, Endpoint::Object(id(deepest))));
    let containers: Vec<_> = (1..=deepest).map(id).collect();
    let parents: Vec<_> = (2..deepest)
        .map(|child| (id(child), id(child - 1)))
        .collect();
    let tree = SceneHierarchy::new(&source, &containers, &parents).unwrap();
    let called = Cell::new(false);
    let result =
        tree.preview_reparent_with_validator(&[id(deepest)], Some(id(deepest - 1)), |_| {
            called.set(true);
            Ok(())
        });
    assert!(result.unwrap_err().contains("exceeds 32 parent links"));
    assert!(!called.get());
    assert_eq!(tree.parent(id(deepest)).unwrap(), None);
    assert_eq!(tree.parent(id(deepest + 1)).unwrap(), None);
}

#[test]
fn maximum_edge_fanout_reparents_atomically_in_stable_order() {
    let mut source = layer(&[1, 2, 3, 4]);
    for number in 5..=MAX_CANVAS_OBJECTS as u64 {
        source
            .objects
            .push(edge(number, 3, Endpoint::Object(id(4))));
    }
    let tree =
        SceneHierarchy::new(&source, &[id(1), id(2)], &[(id(3), id(1)), (id(4), id(2))]).unwrap();
    let moved = tree.preview_reparent(&[id(3)], Some(id(2))).unwrap();
    assert_eq!(moved.roots(), &[id(1), id(2)]);
    let children = moved.children(id(2)).unwrap();
    assert_eq!(children.len(), MAX_CANVAS_OBJECTS - 2);
    assert_eq!(&children[..3], &[id(4), id(3), id(5)]);
    assert_eq!(children.last(), Some(&id(MAX_CANVAS_OBJECTS as u64)));
    assert_eq!(
        moved.parent(id(MAX_CANVAS_OBJECTS as u64)).unwrap(),
        Some(id(2))
    );
    assert_eq!(tree.parent(id(MAX_CANVAS_OBJECTS as u64)).unwrap(), None);
}

#[test]
fn topic_endpoint_stays_attached_and_contributes_layer_ownership_without_an_occurrence() {
    let mut sheet = Sheet::new("Board", "Root");
    sheet.canvas = layer(&[1, 2, 3, 4]);
    sheet
        .canvas
        .objects
        .push(edge(5, 2, Endpoint::Topic(sheet.root)));
    sheet.validate().unwrap();
    let tree = SceneHierarchy::new(
        &sheet.canvas,
        &[id(1), id(4)],
        &[(id(2), id(1)), (id(5), id(1))],
    )
    .unwrap();
    let moved = tree.preview_reparent(&[id(2)], Some(id(4))).unwrap();
    assert_eq!(moved.parent(id(5)).unwrap(), None);
    assert_eq!(connector(&sheet.canvas, 5).to, Endpoint::Topic(sheet.root));
    assert_eq!(tree.parent(id(5)).unwrap(), Some(id(1)));
}

#[test]
fn invalid_connector_dependencies_are_rejected_before_binding_validation() {
    for (edges, message) in [
        (
            vec![
                edge(5, 6, Endpoint::Object(id(2))),
                edge(6, 5, Endpoint::Object(id(3))),
            ],
            "attachment cycle",
        ),
        (
            vec![edge(5, 99, Endpoint::Object(id(2)))],
            "missing scene object",
        ),
    ] {
        let mut source = layer(&[1, 2, 3, 4]);
        source.objects.extend(edges);
        let tree = SceneHierarchy::new(&source, &[id(1), id(4)], &[(id(2), id(1))]).unwrap();
        let called = Cell::new(false);
        let error = tree
            .preview_reparent_with_validator(&[id(2)], Some(id(4)), |_| {
                called.set(true);
                Ok(())
            })
            .unwrap_err();
        assert!(error.contains(message), "{error}");
        assert!(!called.get());
        assert_eq!(tree.parent(id(2)).unwrap(), Some(id(1)));
    }
}

#[test]
fn explicitly_selected_edge_normalized_to_original_owner_still_exposes_order_change() {
    let mut source = layer(&[1, 2, 3, 4]);
    source.objects.push(edge(5, 2, Endpoint::Object(id(3))));
    source.objects.push(edge(6, 3, Endpoint::Object(id(3))));
    let tree =
        SceneHierarchy::new(&source, &[id(1), id(4)], &[(id(2), id(1)), (id(3), id(1))]).unwrap();
    let moved = tree
        .preview_reparent_with_validator(&[id(2), id(5)], Some(id(4)), |change| {
            assert!(change.edge_parent_changes.is_empty());
            assert!(change.affected.contains(&id(5)));
            assert_eq!(change.before.roots(), &[id(1), id(4), id(5), id(6)]);
            assert_eq!(change.after.roots(), &[id(1), id(4), id(6), id(5)]);
            Ok(())
        })
        .unwrap();
    assert_eq!(moved.parent(id(5)).unwrap(), None);
    assert_eq!(moved.children(id(4)).unwrap(), &[id(2)]);
}
