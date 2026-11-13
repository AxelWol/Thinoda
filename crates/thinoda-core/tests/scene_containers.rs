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
    let (mut sheet, [_, lane, _, task, external]) = nested();
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
                    connector.to = Endpoint::Object(external);
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
