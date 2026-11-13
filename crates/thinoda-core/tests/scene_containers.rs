use thinoda_core::board::BoardMember;
use thinoda_core::{
    ApproxMeasure, CanvasConnector, CanvasObject, CanvasObjectId, CanvasObjectKind, Card, CardKind,
    Document, EditError, Endpoint, Rect, ShapeKind, Sheet, Vec2, Workbook, clipboard, layout_sheet,
};

fn nested() -> (Sheet, [CanvasObjectId; 5]) {
    let mut sheet = Sheet::new("Scene", "Root");
    let mut ids = Vec::new();
    for (index, label) in ["Pool", "Lane", "Sub-lane", "Task", "External"]
        .into_iter()
        .enumerate()
    {
        let mut object = CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::new(index as f32 * 30.0, 40.0), Vec2::new(100.0, 60.0)),
        );
        if let CanvasObjectKind::Shape { label: value, .. } = &mut object.kind {
            *value = label.into();
        }
        ids.push(sheet.add_canvas_object(object).unwrap());
    }
    for id in &ids[..3] {
        sheet.set_canvas_container(*id, true).unwrap();
    }
    for index in 1..4 {
        sheet
            .reparent_canvas_objects(&[ids[index]], Some(ids[index - 1]))
            .unwrap();
    }
    sheet.validate().unwrap();
    (sheet, ids.try_into().unwrap())
}

#[test]
fn nested_sheet_roundtrip_validates_structure_and_refuses_incomplete_container_conversion() {
    let (mut sheet, [pool, lane, _, task, _]) = nested();
    let json = serde_json::to_string(&sheet).unwrap();
    assert_eq!(serde_json::from_str::<Sheet>(&json).unwrap(), sheet);
    let before = sheet.clone();
    assert!(sheet.set_canvas_container(pool, false).is_err());
    assert_eq!(sheet, before);
    sheet.canvas.scene.parents.insert(pool, lane);
    assert!(sheet.validate().unwrap_err().contains("cycle"));
    sheet = before.clone();
    sheet
        .canvas
        .scene
        .parents
        .insert(task, CanvasObjectId(99999));
    assert!(sheet.validate().is_err());
    assert_eq!(before.canvas.scene.parents.len(), 3);
}

#[test]
fn translation_of_a_reparent_preview_stores_its_current_ancestry_not_the_borrowed_sources() {
    use thinoda_core::scene_hierarchy::SceneHierarchy;
    let (sheet, [_, lane, _, task, _]) = nested();
    let hierarchy = SceneHierarchy::from_canvas(&sheet.canvas).unwrap();
    let detached = hierarchy.preview_reparent(&[task], None).unwrap();
    let moved = detached
        .preview_translation(&[lane], Vec2::new(10.0, 20.0))
        .unwrap();
    assert!(!moved.scene.parents.contains_key(&task));
    assert_eq!(
        moved.object(task).unwrap().rect,
        sheet.canvas.object(task).unwrap().rect
    );
    SceneHierarchy::from_canvas(&moved).unwrap();
    assert!(sheet.canvas.scene.parents.contains_key(&task));
}

#[test]
fn sheet_commands_move_nested_locked_descendants_once_and_keep_legacy_mixed_groups_whole() {
    let (mut sheet, [pool, _, _, task, external]) = nested();
    sheet.set_canvas_objects_locked(&[task], true).unwrap();
    let card = sheet
        .add_card(
            "Card",
            Vec2::new(400.0, 400.0),
            Card::new(CardKind::Sticky, Vec2::new(100.0, 60.0)),
        )
        .unwrap();
    sheet
        .group_board_members(&[BoardMember::Object(external), BoardMember::Card(card)])
        .unwrap();
    let before = sheet.clone();
    let delta = Vec2::new(20.0, 30.0);
    sheet
        .move_board_members(
            &[
                BoardMember::Object(pool),
                BoardMember::Object(task),
                BoardMember::Object(external),
            ],
            delta,
        )
        .unwrap();
    for object in &before.canvas.objects {
        assert_eq!(
            sheet.canvas.object(object.id).unwrap().rect,
            object.rect.translate(delta)
        );
    }
    assert_eq!(
        sheet.topic(card).unwrap().position,
        before.topic(card).unwrap().position.map(|p| p + delta)
    );
    assert!(sheet.move_canvas_objects(&[task], delta).is_err());
    sheet.set_canvas_objects_locked(&[pool], true).unwrap();
    assert!(sheet.canvas.is_locked(task));
    let locked = sheet.clone();
    assert!(sheet.move_canvas_objects(&[task], delta).is_err());
    assert_eq!(sheet, locked);
    sheet.validate().unwrap();
}

#[test]
fn deleting_a_container_removes_its_owned_subtree_and_detaches_external_edges() {
    let (mut sheet, [pool, _, _, task, external]) = nested();
    let edge = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(task),
            Endpoint::Object(external),
        )))
        .unwrap();
    sheet.remove_canvas_objects(&[pool]).unwrap();
    assert_eq!(sheet.canvas.objects.len(), 2);
    assert!(!sheet.has_scene_hierarchy());
    let CanvasObjectKind::Connector(connector) = &sheet.canvas.object(edge).unwrap().kind else {
        panic!("connector");
    };
    assert!(matches!(connector.from, Endpoint::Point(_)));
    assert_eq!(connector.to, Endpoint::Object(external));
    sheet.validate().unwrap();
}

#[test]
fn nested_clipboards_include_descendants_and_remap_ownership_but_partial_copy_detaches_parent() {
    let (sheet, [_, lane, _, task, _]) = nested();
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let payload = clipboard::copy_board_selection(&sheet, &layout, &[], &[lane])
        .unwrap()
        .unwrap();
    let encoded = clipboard::encode_board_payload(&payload).unwrap();
    assert!(encoded.starts_with("THINODA-BOARD-CLIPBOARD-20\n"));
    assert!(
        clipboard::decode_board_payload(&encoded.replacen("CLIPBOARD-20", "CLIPBOARD-19", 1))
            .is_err()
    );
    let payload = clipboard::decode_board_payload(&encoded).unwrap();
    assert_eq!(
        clipboard::decode_board_payload(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
        payload
    );
    let mut destination = sheet.clone();
    let pasted =
        clipboard::paste_board_selection(&mut destination, &payload, Vec2::new(500.0, 500.0))
            .unwrap();
    assert_eq!(pasted.objects.len(), 3);
    let copied_lane = pasted
        .objects
        .iter()
        .copied()
        .find(|id| destination.canvas.object(*id).unwrap().label() == "Lane")
        .unwrap();
    let copied_task = pasted
        .objects
        .iter()
        .copied()
        .find(|id| destination.canvas.object(*id).unwrap().label() == "Task")
        .unwrap();
    assert!(!destination.canvas.scene.parents.contains_key(&copied_lane));
    let parent = destination.canvas.scene.parents[&copied_task];
    assert_eq!(destination.canvas.scene.parents[&parent], copied_lane);
    assert_eq!(
        destination.canvas.object(copied_task).unwrap().rect.min
            - destination.canvas.object(copied_lane).unwrap().rect.min,
        sheet.canvas.object(task).unwrap().rect.min - sheet.canvas.object(lane).unwrap().rect.min
    );
    assert!(
        pasted
            .objects
            .iter()
            .all(|id| !sheet.canvas.objects.iter().any(|object| object.id == *id))
    );
    destination.validate().unwrap();

    let partial = clipboard::copy_board_selection(&sheet, &layout, &[], &[task])
        .unwrap()
        .unwrap();
    let mut plain = Sheet::new("Partial", "Root");
    clipboard::paste_board_selection(&mut plain, &partial, Vec2::ZERO).unwrap();
    assert!(!plain.has_scene_hierarchy());
    assert_eq!(plain.canvas.objects.len(), 1);
}

#[test]
fn sheet_clipboard_uses_scene_aware_envelope_and_plain_documents_keep_legacy_shape() {
    let (sheet, _) = nested();
    let encoded = clipboard::encode_sheet(&sheet).unwrap();
    assert!(encoded.starts_with("THINODA-SHEET-23\n"));
    assert_eq!(clipboard::decode_sheet(&encoded).unwrap(), sheet);
    assert_eq!(
        clipboard::decode_sheet(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
        sheet
    );
    assert!(clipboard::decode_sheet(&encoded.replacen("SHEET-23", "SHEET-22", 1)).is_err());
    let plain = Sheet::new("Plain", "Root");
    assert!(!serde_json::to_string(&plain).unwrap().contains("\"scene\""));
}

#[test]
fn history_preserves_hierarchy_and_invalid_generic_mutation_rolls_back() {
    let (sheet, [pool, _, _, task, external]) = nested();
    let mut document = Document::new(Workbook::new(sheet.clone()));
    document
        .edit_sheet("Move", |sheet| {
            sheet
                .move_canvas_objects(&[pool], Vec2::new(10.0, 20.0))
                .map_err(EditError::Invalid)
        })
        .unwrap();
    let moved = document.sheet().clone();
    document.undo();
    assert_eq!(document.sheet(), &sheet);
    document.redo();
    assert_eq!(document.sheet(), &moved);
    document
        .edit_sheet("Detach", |sheet| {
            sheet
                .reparent_canvas_objects(&[task], None)
                .map_err(EditError::Invalid)
        })
        .unwrap();
    document.undo();
    assert_eq!(document.sheet(), &moved);
    let result = document.edit_sheet("Malformed delete", |sheet| {
        sheet.canvas.objects.retain(|object| object.id != pool);
        Ok(())
    });
    assert!(result.is_err());
    assert_eq!(document.sheet(), &moved);
    assert!(document.sheet().canvas.object(external).is_some());
}

#[test]
fn unsupported_edits_are_explicit_and_container_style_edits_remain_safe() {
    let (mut sheet, [pool, lane, _, _, _]) = nested();
    let before = sheet.clone();
    assert!(sheet.rotate_canvas_object(pool, 90.0).is_err());
    assert!(sheet.group_canvas_objects(&[pool, lane]).is_err());
    assert!(
        sheet
            .update_canvas_object(lane, |object| object.band = thinoda_core::Band::Below)
            .is_err()
    );
    assert!(
        sheet
            .update_canvas_object(pool, |object| object.rect.min.x += 50.0)
            .is_err()
    );
    assert_eq!(sheet, before);
    sheet
        .update_canvas_object(pool, |object| object.alt_text = "Pool description".into())
        .unwrap();
    assert_eq!(
        sheet.canvas.object(pool).unwrap().alt_text,
        "Pool description"
    );
    let lane_rect = sheet.canvas.object(lane).unwrap().rect;
    sheet
        .resize_canvas_object(
            pool,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(500.0, 400.0)),
        )
        .unwrap();
    assert_eq!(sheet.canvas.object(lane).unwrap().rect, lane_rect);
    sheet.validate().unwrap();
}

#[test]
fn newly_created_edges_get_common_owners_and_failed_edits_are_atomic() {
    let (mut sheet, [_, lane, _, task, _]) = nested();
    let edge = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(task),
            Endpoint::Object(lane),
        )))
        .unwrap();
    assert_eq!(sheet.canvas.scene.parents[&edge], lane);
    let before = sheet.clone();
    assert!(
        sheet
            .update_canvas_object(edge, |object| {
                if let CanvasObjectKind::Connector(connector) = &mut object.kind {
                    connector.to = Endpoint::Object(CanvasObjectId(99999));
                }
            })
            .is_err()
    );
    assert!(
        sheet
            .reparent_canvas_objects_with_validator(&[task], None, |_| Err(
                "scope policy rejected".into()
            ))
            .is_err()
    );
    assert_eq!(sheet, before);
    sheet.set_canvas_objects_locked(&[lane], true).unwrap();
    let locked = sheet.clone();
    assert!(
        sheet
            .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
                Endpoint::Object(task),
                Endpoint::Object(lane)
            )))
            .is_err()
    );
    assert_eq!(sheet.canvas, locked.canvas);
    let mut document = Document::new(Workbook::new(locked.clone()));
    assert!(
        document
            .edit_sheet("Create edge", |sheet| {
                sheet
                    .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
                        Endpoint::Object(task),
                        Endpoint::Object(lane),
                    )))
                    .map_err(EditError::Invalid)
            })
            .is_err()
    );
    assert_eq!(document.sheet(), &locked);
}

#[test]
fn duplicated_merged_and_masked_sheets_keep_the_same_structural_relation() {
    let (sheet, _) = nested();
    let mut document = Document::new(Workbook::new(sheet.clone()));
    document.duplicate_sheet(0).unwrap();
    assert_eq!(
        document.workbook().sheets[1].canvas.scene,
        sheet.canvas.scene
    );
    document.merge_sheets(vec![sheet.clone()]).unwrap();
    assert_eq!(
        document.workbook().sheets[2].canvas.scene,
        sheet.canvas.scene
    );
    let masked = document.workbook().masked_copy();
    masked.validate().unwrap();
    for masked_sheet in &masked.sheets {
        assert_eq!(masked_sheet.canvas.scene, sheet.canvas.scene);
        assert_ne!(
            masked_sheet.canvas.objects[0].label(),
            sheet.canvas.objects[0].label()
        );
    }
}

#[test]
fn topic_deletion_prunes_imported_sankey_edge_ownership_without_dangling_scene_ids() {
    let mut book = thinoda_core::templates::instantiate("sankey-budget");
    let sheet = book.active_mut();
    let link = sheet.canvas.objects[0].id;
    let CanvasObjectKind::Connector(connector) = &sheet.canvas.objects[0].kind else {
        panic!("flow");
    };
    let Endpoint::Topic(source) = connector.from else {
        panic!("source topic");
    };
    let pool = sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(300.0, 200.0)),
        ))
        .unwrap();
    sheet.set_canvas_container(pool, true).unwrap();
    sheet.canvas.scene.parents.insert(link, pool);
    sheet.validate().unwrap();
    let before = sheet.clone();
    let mut document = Document::new(book);
    document
        .edit_sheet("Delete source", |sheet| sheet.remove_topics(&[source]))
        .unwrap();
    assert!(document.sheet().canvas.object(link).is_none());
    assert!(!document.sheet().canvas.scene.parents.contains_key(&link));
    document.sheet().validate().unwrap();
    document.undo();
    assert_eq!(document.sheet(), &before);
}

fn child(sheet: &mut Sheet, parent: CanvasObjectId) -> CanvasObjectId {
    let id = sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::new(200.0, 100.0), Vec2::new(80.0, 40.0)),
        ))
        .unwrap();
    sheet.reparent_canvas_objects(&[id], Some(parent)).unwrap();
    id
}

#[test]
fn paint_commands_order_sibling_subtrees_stably_without_moving_locked_descendants_twice() {
    use thinoda_core::{ZOrder, scene_hierarchy::SceneHierarchy};
    let (mut sheet, [pool, lane, sub, task, _]) = nested();
    let left = child(&mut sheet, pool);
    let right = child(&mut sheet, pool);
    sheet.set_canvas_objects_locked(&[task], true).unwrap();
    for (order, selection, expected) in [
        (
            ZOrder::Front,
            vec![lane, task, lane],
            vec![left, right, lane],
        ),
        (ZOrder::Forward, vec![lane, task], vec![left, lane, right]),
        (ZOrder::Back, vec![right], vec![right, lane, left]),
        (ZOrder::Backward, vec![right], vec![lane, right, left]),
        (ZOrder::Forward, vec![lane, right], vec![left, lane, right]),
        (ZOrder::Back, vec![lane, right], vec![lane, right, left]),
    ] {
        let mut candidate = sheet.clone();
        candidate.reorder_canvas_objects(&selection, order).unwrap();
        let hierarchy = SceneHierarchy::from_canvas(&candidate.canvas).unwrap();
        assert_eq!(hierarchy.children(pool).unwrap(), expected);
        assert_eq!(hierarchy.children(lane).unwrap(), &[sub]);
        assert_eq!(hierarchy.children(sub).unwrap(), &[task]);
        for object in &sheet.canvas.objects {
            assert_eq!(candidate.canvas.object(object.id), Some(object));
        }
        assert_eq!(candidate.canvas.scene, sheet.canvas.scene);
        candidate.validate().unwrap();
    }
}

#[test]
fn sibling_ordering_keeps_other_bands_and_other_parents_separate() {
    use thinoda_core::{Band, ZOrder, scene_hierarchy::SceneHierarchy};
    let (mut sheet, [pool, lane, _, _, external]) = nested();
    let below = sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
        ))
        .unwrap();
    sheet.set_canvas_band(&[below], Band::Below).unwrap();
    sheet.reparent_canvas_objects(&[below], Some(pool)).unwrap();
    let last = child(&mut sheet, pool);
    let outer = sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
        ))
        .unwrap();
    sheet
        .reorder_canvas_objects(&[lane, below, external], ZOrder::Front)
        .unwrap();
    let hierarchy = SceneHierarchy::from_canvas(&sheet.canvas).unwrap();
    assert_eq!(hierarchy.children(pool).unwrap(), &[last, below, lane]);
    assert_eq!(hierarchy.roots(), &[pool, outer, external]);
    assert_eq!(sheet.canvas.object(below).unwrap().band, Band::Below);
}

#[test]
fn paint_validation_runs_once_with_consistent_stored_order_and_rejects_atomically() {
    use std::cell::Cell;
    use thinoda_core::{
        ZOrder,
        scene_hierarchy::{SceneEditKind, SceneHierarchy},
    };
    let (mut sheet, [pool, lane, sub, task, _]) = nested();
    let last = child(&mut sheet, pool);
    let before = sheet.clone();
    let calls = Cell::new(0);
    let result = sheet.reorder_canvas_objects_with_validator(&[lane], ZOrder::Front, |context| {
        calls.set(calls.get() + 1);
        assert_eq!(
            context.kind,
            SceneEditKind::Reorder {
                order: ZOrder::Front
            }
        );
        assert_eq!(context.before.children(pool).unwrap(), &[lane, last]);
        assert_eq!(context.after.children(pool).unwrap(), &[last, lane]);
        assert_eq!(context.affected, &[lane, sub, task]);
        assert_eq!(
            SceneHierarchy::from_canvas(context.geometry)
                .unwrap()
                .preorder(),
            context.after.preorder()
        );
        Err("bound paint order rejected".into())
    });
    assert_eq!(result.unwrap_err(), "bound paint order rejected");
    assert_eq!(calls.get(), 1);
    assert_eq!(sheet, before);
    sheet.set_canvas_objects_locked(&[pool], true).unwrap();
    assert!(
        sheet
            .reorder_canvas_objects(&[task], ZOrder::Forward)
            .is_err()
    );
}

fn owned_edges() -> (Sheet, [CanvasObjectId; 4]) {
    let (mut sheet, [_, _, sub, task, external]) = nested();
    let mut connector = CanvasConnector::new(Endpoint::Object(task), Endpoint::Object(sub));
    connector.waypoints = vec![Vec2::new(20.0, 30.0), Vec2::new(40.0, 60.0)].into();
    connector.label = "Human/service exchange".into();
    connector.from_port = Some(thinoda_core::ConnectorPort {
        position: Vec2::new(1.0, 0.5),
        offset: Vec2::new(3.0, 4.0),
        perimeter: false,
    });
    let edge = sheet
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    let dependent = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(edge),
            Endpoint::Object(task),
        )))
        .unwrap();
    (sheet, [edge, dependent, task, external])
}

#[test]
fn reconnection_updates_dependent_ownership_in_prior_paint_order_not_dependency_order() {
    use thinoda_core::scene_hierarchy::{SceneEditKind, SceneHierarchy};
    let (mut sheet, [edge, dependent, task, external]) = owned_edges();
    let sub = sheet.canvas.scene.parents[&edge];
    let unrelated = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 100.0)),
        )))
        .unwrap();
    sheet.canvas.scene.parents.insert(unrelated, sub);
    let edge_slot = sheet
        .canvas
        .objects
        .iter()
        .position(|object| object.id == edge)
        .unwrap();
    let dependent_slot = sheet
        .canvas
        .objects
        .iter()
        .position(|object| object.id == dependent)
        .unwrap();
    sheet.canvas.objects.swap(edge_slot, dependent_slot);
    sheet.validate().unwrap();
    let before = sheet.clone();
    let mut replacement = sheet.canvas.object(edge).unwrap().clone();
    let CanvasObjectKind::Connector(connector) = &mut replacement.kind else {
        panic!("connector");
    };
    connector.to = Endpoint::Object(external);
    let hierarchy = SceneHierarchy::from_canvas(&sheet.canvas).unwrap();
    let preview = hierarchy
        .preview_reconnect_with_validator(&replacement, |context| {
            assert_eq!(context.kind, SceneEditKind::Reconnect { object: edge });
            assert_eq!(context.affected, &[dependent, edge]);
            assert_eq!(
                context
                    .edge_parent_changes
                    .iter()
                    .map(|change| change.object)
                    .collect::<Vec<_>>(),
                vec![dependent, edge]
            );
            assert_eq!(context.before.parent(edge).unwrap(), Some(sub));
            assert_eq!(context.after.parent(edge).unwrap(), None);
            assert_eq!(context.after.parent(dependent).unwrap(), None);
            assert_eq!(
                SceneHierarchy::from_canvas(context.geometry)
                    .unwrap()
                    .preorder(),
                context.after.preorder()
            );
            Ok(())
        })
        .unwrap();
    sheet
        .update_canvas_object(edge, |object| *object = replacement.clone())
        .unwrap();
    assert_eq!(sheet.canvas, preview);
    assert_eq!(sheet.canvas.object(edge).unwrap(), &replacement);
    assert_eq!(
        sheet.canvas.object(dependent),
        before.canvas.object(dependent)
    );
    assert_eq!(sheet.canvas.scene.parents[&unrelated], sub);
    assert_eq!(sheet.canvas.scene.parents[&task], sub);
    let roots = SceneHierarchy::from_canvas(&sheet.canvas)
        .unwrap()
        .roots()
        .to_vec();
    assert!(roots.ends_with(&[dependent, edge]));
    sheet.validate().unwrap();
}

#[test]
fn reconnect_rejects_induced_locks_and_bad_targets_without_mutating_the_sheet() {
    let (mut source, [edge, dependent, _, external]) = owned_edges();
    let sub = source.canvas.scene.parents[&edge];
    source.set_canvas_container(external, true).unwrap();
    let target = child(&mut source, external);
    for locked in [edge, dependent, sub, external] {
        let mut sheet = source.clone();
        sheet.set_canvas_objects_locked(&[locked], true).unwrap();
        let before = sheet.clone();
        assert!(
            sheet
                .update_canvas_object(edge, |object| {
                    let CanvasObjectKind::Connector(connector) = &mut object.kind else {
                        panic!("connector");
                    };
                    connector.from = Endpoint::Object(target);
                    connector.to = Endpoint::Object(external);
                })
                .is_err(),
            "lock on {locked}"
        );
        assert_eq!(sheet, before);
    }
    for endpoint in [
        Endpoint::Object(edge),
        Endpoint::Object(dependent),
        Endpoint::Object(CanvasObjectId(99999)),
    ] {
        let before = source.clone();
        assert!(
            source
                .update_canvas_object(edge, |object| {
                    let CanvasObjectKind::Connector(connector) = &mut object.kind else {
                        panic!("connector");
                    };
                    connector.from = endpoint;
                })
                .is_err()
        );
        assert_eq!(source, before);
    }
}

#[test]
fn reconnect_validator_cannot_observe_a_partial_owner_update_or_commit_on_rejection() {
    use std::cell::Cell;
    use thinoda_core::scene_hierarchy::SceneHierarchy;
    let (sheet, [edge, dependent, _, external]) = owned_edges();
    let before = sheet.clone();
    let mut replacement = sheet.canvas.object(edge).unwrap().clone();
    let CanvasObjectKind::Connector(connector) = &mut replacement.kind else {
        panic!("connector");
    };
    connector.to = Endpoint::Object(external);
    let calls = Cell::new(0);
    let result = SceneHierarchy::from_canvas(&sheet.canvas)
        .unwrap()
        .preview_reconnect_with_validator(&replacement, |context| {
            calls.set(calls.get() + 1);
            assert_eq!(context.after.parent(edge).unwrap(), None);
            assert_eq!(context.after.parent(dependent).unwrap(), None);
            Err("exchange policy rejected".into())
        });
    assert_eq!(result.unwrap_err(), "exchange policy rejected");
    assert_eq!(calls.get(), 1);
    assert_eq!(sheet, before);
}

#[test]
fn reconnect_depth_overflow_is_rejected_after_induced_owner_changes() {
    use thinoda_core::scene_hierarchy::MAX_SCENE_DEPTH;
    let mut sheet = Sheet::new("Deep", "Root");
    let mut parent = None;
    for _ in 0..=MAX_SCENE_DEPTH {
        let id = sheet
            .add_canvas_object(CanvasObject::shape(
                ShapeKind::Rectangle,
                Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
            ))
            .unwrap();
        sheet.set_canvas_container(id, true).unwrap();
        sheet.reparent_canvas_objects(&[id], parent).unwrap();
        parent = Some(id);
    }
    let edge = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 100.0)),
        )))
        .unwrap();
    let before = sheet.clone();
    let error = sheet
        .update_canvas_object(edge, |object| {
            let CanvasObjectKind::Connector(connector) = &mut object.kind else {
                panic!("connector");
            };
            connector.from = Endpoint::Object(parent.unwrap());
            connector.to = connector.from;
        })
        .unwrap_err();
    assert!(error.contains("32 parent links"), "{error}");
    assert_eq!(sheet, before);
}

#[test]
fn hierarchy_paint_and_reconnect_are_single_undo_steps_and_survive_clipboard_roundtrip() {
    use thinoda_core::ZOrder;
    let (mut sheet, [edge, _, task, external]) = owned_edges();
    let sub = sheet.canvas.scene.parents[&task];
    let other = child(&mut sheet, sub);
    let original = sheet.clone();
    let mut document = Document::new(Workbook::new(sheet));
    document
        .reorder_canvas_objects(&[task], ZOrder::Front)
        .unwrap();
    let reordered = document.sheet().clone();
    assert!(
        reordered
            .canvas
            .objects
            .iter()
            .position(|object| object.id == task)
            .unwrap()
            > reordered
                .canvas
                .objects
                .iter()
                .position(|object| object.id == other)
                .unwrap()
    );
    document
        .update_canvas_object("Reconnect", edge, |object| {
            let CanvasObjectKind::Connector(connector) = &mut object.kind else {
                panic!("connector");
            };
            connector.to = Endpoint::Object(external);
        })
        .unwrap();
    let reconnected = document.sheet().clone();
    document.undo();
    assert_eq!(document.sheet(), &reordered);
    document.undo();
    assert_eq!(document.sheet(), &original);
    document.redo();
    document.redo();
    assert_eq!(document.sheet(), &reconnected);
    assert_eq!(
        clipboard::decode_sheet(&clipboard::encode_sheet(document.sheet()).unwrap()).unwrap(),
        reconnected
    );
}

#[test]
fn free_and_topic_terminals_clear_owners_without_replacing_the_real_attachment() {
    let (source, [edge, dependent, _, _]) = owned_edges();
    for endpoint in [
        Endpoint::Point(Vec2::new(700.0, 800.0)),
        Endpoint::Topic(source.root),
    ] {
        let mut sheet = source.clone();
        sheet
            .update_canvas_object(edge, |object| {
                let CanvasObjectKind::Connector(connector) = &mut object.kind else {
                    panic!("connector");
                };
                connector.to = endpoint;
            })
            .unwrap();
        let CanvasObjectKind::Connector(connector) = &sheet.canvas.object(edge).unwrap().kind
        else {
            panic!("connector");
        };
        assert_eq!(connector.to, endpoint);
        assert!(!sheet.canvas.scene.parents.contains_key(&edge));
        assert!(!sheet.canvas.scene.parents.contains_key(&dependent));
        assert_eq!(
            sheet.canvas.object(dependent),
            source.canvas.object(dependent)
        );
        sheet.validate().unwrap();
    }
}

#[test]
fn unchanged_terminals_preserve_noncanonical_imported_owners() {
    use thinoda_core::scene_hierarchy::SceneHierarchy;
    let (mut sheet, [edge, dependent, task, _]) = owned_edges();
    let sub = sheet.canvas.scene.parents[&task];
    let lane = sheet.canvas.scene.parents[&sub];
    sheet.canvas.scene.parents.insert(edge, lane);
    sheet.validate().unwrap();
    let mut replacement = sheet.canvas.object(edge).unwrap().clone();
    let CanvasObjectKind::Connector(connector) = &mut replacement.kind else {
        panic!("connector");
    };
    connector.label = "Updated label".into();
    let candidate = SceneHierarchy::from_canvas(&sheet.canvas)
        .unwrap()
        .preview_reconnect_with_validator(&replacement, |context| {
            assert!(context.edge_parent_changes.is_empty());
            assert_eq!(context.affected, &[edge]);
            Ok(())
        })
        .unwrap();
    assert_eq!(candidate.scene, sheet.canvas.scene);
    assert_eq!(candidate.object(dependent), sheet.canvas.object(dependent));
    sheet
        .update_canvas_object(edge, |object| *object = replacement.clone())
        .unwrap();
    assert_eq!(sheet.canvas.scene, candidate.scene);
    assert_eq!(
        SceneHierarchy::from_canvas(&sheet.canvas)
            .unwrap()
            .preorder(),
        SceneHierarchy::from_canvas(&candidate).unwrap().preorder()
    );
    assert_eq!(sheet.canvas.object(edge), candidate.object(edge));
}
