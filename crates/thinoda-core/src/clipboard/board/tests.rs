use super::*;
use crate::board::{
    Band, CanvasConnector, CanvasObject, CanvasObjectKind, Card, CardKind, InkStroke, InkTool,
    ShapeKind,
};
use crate::clipboard::MAX_TOPIC_CLIPBOARD_DEPTH;
use crate::geometry::{Color, Rect};
use crate::layout::{ApproxMeasure, layout_sheet};
use crate::{Document, EditError, Workbook};

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect::from_min_size(Vec2::new(x, y), Vec2::new(width, height))
}

fn hidden_sheet(title: &str, anchor_title: &str) -> Sheet {
    let mut sheet = Sheet::new(title, anchor_title);
    sheet.root_mode = SheetRootMode::Hidden;
    sheet
}

fn copy(sheet: &Sheet, topics: &[TopicId], objects: &[CanvasObjectId]) -> BoardClipboardPayload {
    let layout = layout_sheet(sheet, &ApproxMeasure, false);
    copy_board_selection(sheet, &layout, topics, objects)
        .unwrap()
        .unwrap()
}

#[test]
fn copied_content_bounds_match_the_pasted_rotated_card_and_shape() {
    let mut source = hidden_sheet("Source", "Hidden anchor");
    let mut card = Card::new(CardKind::Sticky, Vec2::new(100.0, 60.0));
    card.rotation = 35.0;
    let card = source
        .add_card("Card", Vec2::new(400.0, 300.0), card)
        .unwrap();
    let object = source
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Ellipse,
            rect(600.0, 350.0, 80.0, 40.0),
        ))
        .unwrap();
    let payload = copy(&source, &[card], &[object]);
    let mut pasted = hidden_sheet("Destination", "Another hidden anchor");
    paste_board_selection(&mut pasted, &payload, Vec2::ZERO).unwrap();
    let layout = layout_sheet(&pasted, &ApproxMeasure, false);
    let expected = build_scene(&pasted, &layout)
        .items
        .iter()
        .filter_map(|item| item.visible_bounds())
        .reduce(|a, b| a.union(&b))
        .unwrap();
    let bounds = payload.content_bounds().unwrap();
    assert_eq!(bounds, expected);
    assert!(bounds.min.x > 300.0, "the hidden anchor is not content");
    assert!(bounds.min.y < 270.0, "the card's rotation is included");
}

fn depth_fixture(depth: usize, summaries: bool) -> (Sheet, TopicId) {
    let mut sheet = hidden_sheet("", "");
    let root = sheet.add_floating("Root", Vec2::new(100.0, 100.0));
    let (mut parent, start) = if summaries {
        let covered = sheet.add_child(root, None, "Covered");
        let summary = sheet.add_summary(&[covered], "Summary").unwrap();
        let covered = sheet.add_child(summary, None, "Covered again");
        (sheet.add_summary(&[covered], "Nested summary").unwrap(), 2)
    } else {
        (root, 0)
    };
    for level in start..depth {
        let id = TopicId(100 + level as u64);
        let mut topic = Topic::new(id, "Child");
        topic.parent = Some(parent);
        sheet.topic_mut(parent).unwrap().children.push(id);
        sheet.topics.insert(id, topic);
        parent = id;
    }
    (sheet, root)
}

#[test]
fn mixed_clipboard_bounds_owned_depth_and_preserves_summary_trees() {
    for summaries in [false, true] {
        let (source, root) = depth_fixture(MAX_TOPIC_CLIPBOARD_DEPTH, summaries);
        source.validate().unwrap();
        let payload = copy_board_selection(&source, &Layout::default(), &[root], &[])
            .unwrap()
            .unwrap();
        let encoded = encode_board_payload(&payload).unwrap();
        let decoded = decode_board_payload(&encoded).unwrap();
        assert_eq!(decoded, payload);
        assert_eq!(
            decoded.fragment.summaries.len(),
            if summaries { 2 } else { 0 }
        );

        let mut doc = Document::new(Workbook::new(hidden_sheet("Destination", "")));
        let before = doc.workbook().clone();
        let (too_deep, too_deep_root) = depth_fixture(MAX_TOPIC_CLIPBOARD_DEPTH + 1, summaries);
        too_deep.validate().unwrap();
        let error = match copy_board_selection(&too_deep, &Layout::default(), &[too_deep_root], &[])
        {
            Err(error) => error,
            Ok(_) => panic!("over-deep board selection should be rejected"),
        };
        assert!(error.contains("depth"), "{error}");

        let invalid = BoardClipboardPayload {
            text: "Root\n".into(),
            fragment: too_deep,
        };
        assert!(invalid.validate().unwrap_err().contains("depth"));
        assert!(
            encode_board_payload(&invalid)
                .unwrap_err()
                .contains("depth")
        );
        assert!(
            resolve_board_paste(Some(&invalid), None, None)
                .unwrap_err()
                .contains("depth")
        );
        let forged = format!("{BOARD_PREFIX}{}", serde_json::to_string(&invalid).unwrap());
        assert!(decode_board_payload(&forged).unwrap_err().contains("depth"));
        assert!(
            doc.edit_sheet("Rejected paste", |sheet| {
                paste_board_selection(sheet, &invalid, Vec2::ZERO).map_err(EditError::Invalid)
            })
            .unwrap_err()
            .to_string()
            .contains("depth")
        );
        assert_eq!(doc.workbook(), &before);
        assert!(!doc.can_undo());

        doc.edit_sheet("Paste", |sheet| {
            paste_board_selection(sheet, &decoded, Vec2::ZERO).map_err(EditError::Invalid)
        })
        .unwrap();
        assert_eq!(doc.sheet().summaries.len(), if summaries { 2 } else { 0 });
        let after = doc.workbook().clone();
        assert!(doc.undo());
        assert_eq!(doc.workbook(), &before);
        assert!(doc.redo());
        assert_eq!(doc.workbook(), &after);
    }
}

#[test]
fn mixed_copy_rejects_deep_sources_without_recursive_layout() {
    let (source, root) = depth_fixture(4_096, true);
    let error = copy_board_selection(&source, &Layout::default(), &[root], &[]).unwrap_err();
    assert!(error.contains("depth"), "{error}");
}

#[test]
fn mixed_copy_preflight_checks_cards_added_by_group_expansion() {
    let (mut source, root) = depth_fixture(MAX_TOPIC_CLIPBOARD_DEPTH + 1, true);
    source.topic_mut(root).unwrap().card =
        Some(Card::new(CardKind::Sticky, Vec2::new(160.0, 100.0)));
    let shape = source
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            rect(10.0, 20.0, 50.0, 40.0),
        ))
        .unwrap();
    source
        .group_board_members(&[BoardMember::Card(root), BoardMember::Object(shape)])
        .unwrap();

    match prepare_board_selection(&source, &[], &[shape]) {
        Err(error) => assert!(error.contains("depth"), "{error}"),
        Ok(_) => panic!("a grouped over-deep card should be rejected"),
    }
}

#[test]
fn formatted_cards_survive_mixed_and_legacy_floating_clipboards_with_undo() {
    let mut source = hidden_sheet("Source", "");
    let title = "A🦀é\n第二";
    let mut cards = Vec::new();
    for (index, kind) in [CardKind::Sticky, CardKind::Text].into_iter().enumerate() {
        let id = source
            .add_card(
                title,
                Vec2::new(100.0 + index as f32 * 200.0, 100.0),
                Card::new(kind, Vec2::new(160.0, 100.0)),
            )
            .unwrap();
        let topic = source.topic_mut(id).unwrap();
        topic
            .inline_text
            .update_range(title, 0..3, |style| {
                style.italic = Some(true);
                style.color = Some(Color::rgb(30, 60, 90));
            })
            .unwrap();
        topic.inline_text.paragraphs.push(crate::InlineParagraph {
            index: 1,
            list: crate::ListKind::Bulleted,
        });
        cards.push(id);
    }
    let shape = source
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            rect(20.0, 40.0, 50.0, 60.0),
        ))
        .unwrap();
    source.validate().unwrap();

    let payload = copy(&source, &cards, &[shape]);
    let encoded = encode_board_payload(&payload).unwrap();
    assert!(encoded.starts_with(INLINE_BOARD_PREFIX));
    assert_eq!(decode_board_payload(&encoded).unwrap(), payload);
    assert_eq!(
        decode_board_payload(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
        payload
    );
    let old = encoded.replacen("THINODA-BOARD-CLIPBOARD-2", "THINODA-BOARD-CLIPBOARD-1", 1);
    assert!(
        decode_board_payload(&old)
            .unwrap_err()
            .contains("inline card text requires board clipboard version 2")
    );
    let mut invalid = payload.clone();
    let bad_id = invalid.fragment.floating[0];
    invalid.fragment.topic_mut(bad_id).unwrap().inline_text.runs[0].end = 100;
    assert!(encode_board_payload(&invalid).is_err());
    let forged = format!(
        "{INLINE_BOARD_PREFIX}{}",
        serde_json::to_string(&invalid).unwrap()
    );
    assert!(decode_board_payload(&forged).is_err());

    let mut destination = hidden_sheet("Destination", "");
    destination.add_floating("Existing", Vec2::new(600.0, 200.0));
    let mut doc = Document::new(Workbook::new(destination));
    let before = doc.workbook().clone();
    assert!(
        doc.edit_sheet("Bad paste", |sheet| {
            paste_board_selection(sheet, &invalid, Vec2::ZERO).map_err(EditError::Invalid)
        })
        .is_err()
    );
    assert_eq!(doc.workbook(), &before);
    assert!(!doc.can_undo());
    let pasted = doc
        .edit_sheet("Paste", |sheet| {
            paste_board_selection(sheet, &payload, Vec2::new(20.0, 20.0))
                .map_err(EditError::Invalid)
        })
        .unwrap();
    for (&original, &new) in cards.iter().zip(&pasted.topics) {
        assert_ne!(original, new);
        let actual = doc.sheet().topic(new).unwrap();
        let expected = source.topic(original).unwrap();
        assert_eq!(actual.inline_text, expected.inline_text);
        assert_eq!(actual.card, expected.card);
        assert_eq!(actual.title, expected.title);
    }
    doc.workbook().validate().unwrap();
    let after = doc.workbook().clone();
    assert!(doc.undo());
    assert_eq!(doc.workbook(), &before);
    assert!(doc.redo());
    assert_eq!(doc.workbook(), &after);

    let legacy = crate::clipboard::copy_topics(&source, &[cards[0]])
        .unwrap()
        .unwrap();
    let mut legacy_destination = doc.sheet().clone();
    let roots = paste_floating_subtrees(
        &mut legacy_destination,
        &legacy.subtrees,
        Vec2::new(700.0, 200.0),
    )
    .unwrap();
    assert_eq!(
        legacy_destination.topic(roots[0]).unwrap().inline_text,
        source.topic(cards[0]).unwrap().inline_text
    );
    legacy_destination.validate().unwrap();
}

#[test]
fn formatted_board_clipboard_enforces_the_exact_encoded_size_limit() {
    let mut source = hidden_sheet("Source", "");
    let card = source
        .add_card(
            "A🦀",
            Vec2::new(100.0, 100.0),
            Card::new(CardKind::Sticky, Vec2::new(100.0, 80.0)),
        )
        .unwrap();
    source
        .topic_mut(card)
        .unwrap()
        .inline_text
        .update_range("A🦀", 1..2, |style| style.bold = Some(true))
        .unwrap();
    let mut payload = copy(&source, &[card], &[]);
    payload.text = "x".into();
    let base_len = encode_board_payload(&payload).unwrap().len();
    payload
        .text
        .push_str(&"x".repeat(MAX_BOARD_CLIPBOARD_BYTES - base_len));
    let mut encoded = encode_board_payload(&payload).unwrap();
    assert_eq!(encoded.len(), MAX_BOARD_CLIPBOARD_BYTES);
    assert_eq!(decode_board_payload(&encoded).unwrap(), payload);
    assert!(decode_board_payload(&encoded.replacen('\n', "\r\n", 1)).is_err());
    payload.text.pop();
    let windows = encode_board_payload(&payload)
        .unwrap()
        .replacen('\n', "\r\n", 1);
    assert_eq!(windows.len(), MAX_BOARD_CLIPBOARD_BYTES);
    assert_eq!(decode_board_payload(&windows).unwrap(), payload);
    payload.text.push_str("xx");
    assert!(encode_board_payload(&payload).is_err());
    encoded.push(' ');
    assert!(decode_board_payload(&encoded).is_err());
}

#[test]
fn object_only_copy_has_plain_text_and_legacy_fallback() {
    let mut sheet = Sheet::new("source", "root");
    let object = sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            rect(10.0, 20.0, 50.0, 40.0),
        ))
        .unwrap();
    let payload = copy(&sheet, &[], &[object]);
    assert!(!payload.text.trim().is_empty());
    assert!(matches!(
        resolve_board_paste(
            Some(&payload),
            None,
            Some(&payload.text.replace('\n', "\r\n"))
        )
        .unwrap(),
        ResolvedBoardPaste::Board(_)
    ));
    let ResolvedBoardPaste::Topics(topics) =
        resolve_board_paste(Some(&payload), None, Some("One\n\tChild\n")).unwrap()
    else {
        panic!("different OS text should use the legacy text parser");
    };
    assert_eq!(topics[0].root_topic().title, "One");
    assert_eq!(topics[0].len(), 2);
}

#[test]
fn mixed_copy_preserves_resources_annotations_groups_styles_and_ink() {
    let mut source = hidden_sheet("Source", "hidden source anchor");
    let card = source
        .add_card(
            "Card",
            Vec2::new(100.0, 100.0),
            Card {
                kind: CardKind::Sticky,
                size: Vec2::new(140.0, 90.0),
                rotation: 15.0,
                color: Some(Color::rgb(250, 220, 100)),
            },
        )
        .unwrap();
    source.topic_mut(card).unwrap().style.bold = Some(true);
    let child = source.add_child(card, None, "Child");
    let other = source.add_floating("Other", Vec2::new(430.0, 160.0));
    let resource = source
        .resources
        .insert(
            crate::resources::Resource::new("evidence.txt", "text/plain", b"evidence".to_vec())
                .unwrap(),
        )
        .unwrap();
    source.topic_mut(child).unwrap().attachment = Some(crate::attachments::Attachment {
        resource: resource.clone(),
        name: "evidence.txt".into(),
    });
    let unused_resource = source
        .resources
        .insert(
            crate::resources::Resource::new("unused.txt", "text/plain", b"unused".to_vec())
                .unwrap(),
        )
        .unwrap();
    source.add_relationship(child, other).unwrap();
    source.relationships[0].title = "relates".into();
    source.add_boundary(&[child]).unwrap();
    let summary = source.add_summary(&[child], "Summary").unwrap();
    let summary_child = source.add_child(summary, None, "Summary child");
    source
        .add_summary(&[summary_child], "Nested summary")
        .unwrap();
    source
        .set_task(child, Some(crate::schedule::Task::default()))
        .unwrap();
    source
        .set_task(other, Some(crate::schedule::Task::default()))
        .unwrap();
    source.add_task_dependency(child, other).unwrap();

    let mut shape = CanvasObject::shape(ShapeKind::Diamond, rect(230.0, 40.0, 60.0, 50.0));
    shape.band = Band::Below;
    shape.rotation = 30.0;
    shape.style.fill = Some(Color::rgb(10, 20, 30));
    shape.style.opacity = 0.6;
    shape.alt_text = "A diamond".into();
    let shape = source.add_canvas_object(shape).unwrap();
    let mut ink = InkStroke::new(
        InkTool::Highlighter,
        Color::rgb(50, 60, 70),
        6.0,
        vec![Vec2::new(1.0, 2.0), Vec2::new(5.0, 8.0)],
    );
    ink.pressure = Some(vec![0.25, 0.75].into());
    let ink = source.add_canvas_object(CanvasObject::ink(ink)).unwrap();
    let connector = source
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Topic(child),
            Endpoint::Object(shape),
        )))
        .unwrap();
    let group = source
        .group_board_members(&[BoardMember::Card(card), BoardMember::Object(shape)])
        .unwrap();
    source
        .set_board_members_locked(&[BoardMember::Card(card)], true)
        .unwrap();
    source.set_canvas_group_locked(group, true).unwrap();
    source.validate().unwrap();

    // The card pulls its grouped shape into the copy; the duplicate and
    // selected descendant must not duplicate the card subtree.
    let payload = copy(
        &source,
        &[source.root, summary, card, child, other, card],
        &[ink, connector, connector],
    );
    assert_eq!(payload.source_topic_roots(), &[card, other]);
    assert_eq!(
        payload.source_object_ids().collect::<Vec<_>>(),
        vec![shape, ink, connector]
    );
    assert!(!payload.text.contains("hidden source anchor"));
    let encoded = encode_board_payload(&payload).unwrap();
    assert!(!encoded.contains("hidden source anchor"));
    assert!(!encoded.contains("unused.txt"));
    assert_eq!(decode_board_payload(&encoded).unwrap(), payload);
    assert_eq!(
        decode_board_payload(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
        payload
    );

    let mut destination = hidden_sheet("Destination", "hidden destination anchor");
    let delta = Vec2::new(35.0, 45.0);
    let pasted = paste_board_selection(&mut destination, &payload, delta).unwrap();
    assert_eq!(pasted.topics.len(), 2);
    assert_eq!(pasted.objects.len(), 3);
    let new_card = pasted.topics[0];
    let new_other = pasted.topics[1];
    let new_child = destination.children(new_card)[0];
    let new_shape = pasted.objects[0];
    let new_ink = pasted.objects[1];
    let new_connector = pasted.objects[2];
    assert_ne!(new_card, card);
    assert_ne!(new_child, child);
    assert_ne!(new_other, other);
    assert_ne!(new_shape, shape);
    assert_ne!(new_ink, ink);
    assert_ne!(new_connector, connector);
    assert_eq!(destination.root_mode, SheetRootMode::Hidden);
    assert!(destination.children(destination.root).is_empty());
    assert_eq!(destination.floating, pasted.topics);
    assert_eq!(
        destination.topic(new_card).unwrap().position,
        Some(Vec2::new(135.0, 145.0))
    );
    assert_eq!(
        destination.topic(new_card).unwrap().card,
        source.topic(card).unwrap().card
    );
    assert_eq!(
        destination.topic(new_card).unwrap().style,
        source.topic(card).unwrap().style
    );
    assert_eq!(
        destination.topic(new_child).unwrap().attachment,
        source.topic(child).unwrap().attachment
    );
    assert_eq!(
        destination.resources.get(&resource).unwrap().bytes(),
        b"evidence"
    );
    assert!(destination.resources.get(&unused_resource).is_err());
    assert_eq!(destination.relationships.len(), 1);
    assert_ne!(destination.relationships[0].id, source.relationships[0].id);
    assert_eq!(destination.relationships[0].from, new_child);
    assert_eq!(destination.relationships[0].to, new_other);
    assert_eq!(destination.relationships[0].title, "relates");
    assert_eq!(destination.boundaries.len(), 1);
    assert_ne!(destination.boundaries[0].id, source.boundaries[0].id);
    assert_eq!(destination.boundaries[0].parent, new_card);
    assert_eq!(destination.summaries.len(), 2);
    assert_ne!(destination.summaries[0].id, source.summaries[0].id);
    assert_eq!(destination.summaries[0].parent, new_card);
    let new_summary = destination.summaries[0].topic;
    assert_eq!(destination.topic(new_summary).unwrap().title, "Summary");
    assert_eq!(destination.children(new_summary).len(), 1);
    assert!(destination.scheduling.tasks.contains_key(&new_child));
    assert!(destination.scheduling.tasks.contains_key(&new_other));
    assert_eq!(
        destination.scheduling.dependencies,
        vec![crate::schedule::TaskDependency {
            predecessor: new_child,
            successor: new_other,
        }]
    );

    let old_shape = source.canvas.object(shape).unwrap();
    let new_shape_obj = destination.canvas.object(new_shape).unwrap();
    assert_eq!(new_shape_obj.rect, old_shape.rect.translate(delta));
    assert_eq!(new_shape_obj.band, old_shape.band);
    assert_eq!(new_shape_obj.rotation, old_shape.rotation);
    assert_eq!(new_shape_obj.style, old_shape.style);
    assert_eq!(new_shape_obj.alt_text, old_shape.alt_text);
    let new_group = new_shape_obj.group.unwrap();
    assert_ne!(new_group, group);
    assert!(destination.canvas.group(new_group).unwrap().locked);
    let new_card_state = destination.canvas.card_state(new_card).unwrap();
    assert_eq!(new_card_state.group, Some(new_group));
    assert!(new_card_state.locked);
    assert!(destination.is_board_member_locked(BoardMember::Object(new_shape)));
    assert!(destination.is_board_member_locked(BoardMember::Card(new_card)));
    let before_locked_move = destination.clone();
    assert!(
        destination
            .move_board_members(&[BoardMember::Object(new_shape)], Vec2::new(1.0, 0.0))
            .is_err()
    );
    assert_eq!(destination, before_locked_move);

    let CanvasObjectKind::Ink(original_ink) = &source.canvas.object(ink).unwrap().kind else {
        panic!("expected ink");
    };
    let CanvasObjectKind::Ink(copied_ink) = &destination.canvas.object(new_ink).unwrap().kind
    else {
        panic!("expected ink");
    };
    assert_eq!(copied_ink.tool, original_ink.tool);
    assert_eq!(copied_ink.color, original_ink.color);
    assert_eq!(copied_ink.width, original_ink.width);
    assert_eq!(copied_ink.pressure, original_ink.pressure);
    assert_eq!(
        copied_ink.points.as_ref(),
        &[Vec2::new(36.0, 47.0), Vec2::new(40.0, 53.0)]
    );
    let CanvasObjectKind::Connector(copied_connector) =
        &destination.canvas.object(new_connector).unwrap().kind
    else {
        panic!("expected connector");
    };
    assert_eq!(copied_connector.from, Endpoint::Topic(new_child));
    assert_eq!(copied_connector.to, Endpoint::Object(new_shape));
    destination.validate().unwrap();
}

#[test]
fn connector_waypoints_follow_cross_document_clipboard_paste() {
    let mut source = Sheet::new("Source", "Root");
    let target = source
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            rect(300.0, 40.0, 90.0, 90.0),
        ))
        .unwrap();
    let mut connector = CanvasConnector::new(Endpoint::Point(Vec2::ZERO), Endpoint::Object(target));
    connector.waypoints = vec![Vec2::new(30.0, 50.0), Vec2::new(150.0, 80.0)].into();
    let id = source
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    let payload = copy(&source, &[], &[id]);
    let encoded = encode_board_payload(&payload).unwrap();
    assert!(encoded.starts_with(WAYPOINT_BOARD_PREFIX));
    let decoded = decode_board_payload(&encoded).unwrap();
    assert_eq!(
        decode_board_payload(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
        payload
    );
    for version in 1..=2 {
        let forged = encoded.replacen(
            "THINODA-BOARD-CLIPBOARD-3",
            &format!("THINODA-BOARD-CLIPBOARD-{version}"),
            1,
        );
        assert!(
            decode_board_payload(&forged)
                .unwrap_err()
                .contains("connector waypoints require board clipboard version 3")
        );
    }

    let card = source
        .add_card(
            "Formatted",
            Vec2::new(100.0, 200.0),
            Card::new(CardKind::Text, Vec2::new(80.0, 50.0)),
        )
        .unwrap();
    source
        .topic_mut(card)
        .unwrap()
        .inline_text
        .update_range("Formatted", 0..3, |style| style.bold = Some(true))
        .unwrap();
    let combined = copy(&source, &[card], &[id]);
    let combined_wire = encode_board_payload(&combined).unwrap();
    assert!(combined_wire.starts_with(WAYPOINT_BOARD_PREFIX));
    assert_eq!(decode_board_payload(&combined_wire).unwrap(), combined);

    let mut destination = Sheet::new("Destination", "Root");
    let delta = Vec2::new(100.0, -20.0);
    let pasted = paste_board_selection(&mut destination, &decoded, delta).unwrap();
    let CanvasObjectKind::Connector(connector) =
        &destination.canvas.object(pasted.objects[0]).unwrap().kind
    else {
        panic!("expected connector");
    };
    assert_eq!(
        connector.waypoints.as_ref(),
        &[Vec2::new(130.0, 30.0), Vec2::new(250.0, 60.0)]
    );
    assert!(matches!(connector.to, Endpoint::Point(_)));
    destination.validate().unwrap();
}

#[test]
fn connector_markers_follow_cross_document_clipboard_paste() {
    let mut source = Sheet::new("Source", "Root");
    let mut connector = CanvasConnector::new(
        Endpoint::Point(Vec2::ZERO),
        Endpoint::Point(Vec2::new(100.0, 0.0)),
    );
    connector.end_marker = Some(crate::ConnectorMarker {
        kind: crate::ConnectorMarkerKind::Open,
    });
    connector.waypoints = vec![Vec2::new(50.0, 30.0)].into();
    let id = source
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    let payload = copy(&source, &[], &[id]);
    let encoded = encode_board_payload(&payload).unwrap();
    assert!(encoded.starts_with(MARKER_BOARD_PREFIX));
    assert_eq!(decode_board_payload(&encoded).unwrap(), payload);
    assert_eq!(
        decode_board_payload(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
        payload
    );
    for version in 1..=3 {
        let forged = encoded.replacen(
            "THINODA-BOARD-CLIPBOARD-4",
            &format!("THINODA-BOARD-CLIPBOARD-{version}"),
            1,
        );
        assert!(
            decode_board_payload(&forged)
                .unwrap_err()
                .contains("connector markers require board clipboard version 4")
        );
    }

    let card = source
        .add_card(
            "Formatted",
            Vec2::new(100.0, 200.0),
            Card::new(CardKind::Text, Vec2::new(80.0, 50.0)),
        )
        .unwrap();
    source
        .topic_mut(card)
        .unwrap()
        .inline_text
        .update_range("Formatted", 0..3, |style| style.bold = Some(true))
        .unwrap();
    let combined = copy(&source, &[card], &[id]);
    let combined_wire = encode_board_payload(&combined).unwrap();
    assert!(combined_wire.starts_with(MARKER_BOARD_PREFIX));
    assert_eq!(decode_board_payload(&combined_wire).unwrap(), combined);

    let mut destination = Sheet::new("Destination", "Root");
    let pasted = paste_board_selection(&mut destination, &payload, Vec2::new(10.0, 20.0)).unwrap();
    let CanvasObjectKind::Connector(connector) =
        &destination.canvas.object(pasted.objects[0]).unwrap().kind
    else {
        panic!("expected connector");
    };
    assert_eq!(
        connector.end_marker.unwrap().kind,
        crate::ConnectorMarkerKind::Open
    );
    assert_eq!(connector.waypoints.as_ref(), &[Vec2::new(60.0, 50.0)]);
    destination.validate().unwrap();
}

#[test]
fn classic_and_thin_marker_board_clipboards_require_version_five() {
    for kind in [
        crate::ConnectorMarkerKind::Classic,
        crate::ConnectorMarkerKind::ClassicThin,
        crate::ConnectorMarkerKind::OpenThin,
        crate::ConnectorMarkerKind::BlockThin,
    ] {
        let mut source = Sheet::new("Source", "Root");
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.end_marker = Some(crate::ConnectorMarker { kind });
        let id = source
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let payload = copy(&source, &[], &[id]);
        let encoded = encode_board_payload(&payload).unwrap();
        assert!(encoded.starts_with(EXTENDED_MARKER_BOARD_PREFIX));
        assert_eq!(decode_board_payload(&encoded).unwrap(), payload);
        assert_eq!(
            decode_board_payload(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
            payload
        );
        let forged = encoded.replacen("THINODA-BOARD-CLIPBOARD-5", "THINODA-BOARD-CLIPBOARD-4", 1);
        assert!(
            decode_board_payload(&forged)
                .unwrap_err()
                .contains("classic and thin connector markers require board clipboard version 5")
        );
        let mut destination = Sheet::new("Destination", "Root");
        let pasted =
            paste_board_selection(&mut destination, &payload, Vec2::new(10.0, 20.0)).unwrap();
        let CanvasObjectKind::Connector(connector) =
            &destination.canvas.object(pasted.objects[0]).unwrap().kind
        else {
            panic!("expected connector");
        };
        assert_eq!(connector.end_marker.unwrap().kind, kind);
    }
}

#[test]
fn sized_marker_board_clipboards_require_version_six() {
    let mut source = Sheet::new("Source", "Root");
    let mut connector = CanvasConnector::new(
        Endpoint::Point(Vec2::ZERO),
        Endpoint::Point(Vec2::new(100.0, 0.0)),
    );
    connector.end_marker = Some(crate::ConnectorMarker {
        kind: crate::ConnectorMarkerKind::Open,
    });
    connector.end_marker_size = Some(12.5);
    let id = source
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    let payload = copy(&source, &[], &[id]);
    let encoded = encode_board_payload(&payload).unwrap();
    assert!(encoded.starts_with(SIZED_MARKER_BOARD_PREFIX));
    assert_eq!(decode_board_payload(&encoded).unwrap(), payload);
    assert_eq!(
        decode_board_payload(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
        payload
    );
    let forged = encoded.replacen("THINODA-BOARD-CLIPBOARD-6", "THINODA-BOARD-CLIPBOARD-5", 1);
    assert!(
        decode_board_payload(&forged)
            .unwrap_err()
            .contains("connector marker sizes require board clipboard version 6")
    );
    let mut destination = Sheet::new("Destination", "Root");
    let pasted = paste_board_selection(&mut destination, &payload, Vec2::new(10.0, 20.0)).unwrap();
    let CanvasObjectKind::Connector(connector) =
        &destination.canvas.object(pasted.objects[0]).unwrap().kind
    else {
        panic!("expected connector");
    };
    assert_eq!(connector.end_marker_size, Some(12.5));
}

#[test]
fn hollow_marker_board_clipboards_require_version_seven() {
    let mut source = Sheet::new("Source", "Root");
    let mut connector = CanvasConnector::new(
        Endpoint::Point(Vec2::ZERO),
        Endpoint::Point(Vec2::new(100.0, 0.0)),
    );
    connector.end_marker = Some(crate::ConnectorMarker {
        kind: crate::ConnectorMarkerKind::Classic,
    });
    connector.end_marker_unfilled = true;
    let id = source
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    let payload = copy(&source, &[], &[id]);
    let encoded = encode_board_payload(&payload).unwrap();
    assert!(encoded.starts_with(FILLED_MARKER_BOARD_PREFIX));
    assert_eq!(decode_board_payload(&encoded).unwrap(), payload);
    assert_eq!(
        decode_board_payload(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
        payload
    );
    let forged = encoded.replacen("THINODA-BOARD-CLIPBOARD-7", "THINODA-BOARD-CLIPBOARD-6", 1);
    assert!(
        decode_board_payload(&forged)
            .unwrap_err()
            .contains("connector marker fills require board clipboard version 7")
    );
    let mut destination = Sheet::new("Destination", "Root");
    let pasted = paste_board_selection(&mut destination, &payload, Vec2::new(10.0, 20.0)).unwrap();
    let CanvasObjectKind::Connector(connector) =
        &destination.canvas.object(pasted.objects[0]).unwrap().kind
    else {
        panic!("expected connector");
    };
    assert!(connector.end_marker_unfilled);
}

#[test]
fn diamond_marker_board_clipboards_require_version_eight() {
    for kind in [
        crate::ConnectorMarkerKind::Diamond,
        crate::ConnectorMarkerKind::DiamondThin,
    ] {
        let mut source = Sheet::new("Source", "Root");
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.end_marker = Some(crate::ConnectorMarker { kind });
        connector.end_marker_size = Some(12.5);
        connector.end_marker_unfilled = true;
        let id = source
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let payload = copy(&source, &[], &[id]);
        let encoded = encode_board_payload(&payload).unwrap();
        assert!(encoded.starts_with(DIAMOND_MARKER_BOARD_PREFIX));
        assert_eq!(decode_board_payload(&encoded).unwrap(), payload);
        assert_eq!(
            decode_board_payload(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
            payload
        );
        let forged = encoded.replacen("THINODA-BOARD-CLIPBOARD-8", "THINODA-BOARD-CLIPBOARD-7", 1);
        assert!(
            decode_board_payload(&forged)
                .unwrap_err()
                .contains("diamond connector markers require board clipboard version 8")
        );
        let mut destination = Sheet::new("Destination", "Root");
        let pasted =
            paste_board_selection(&mut destination, &payload, Vec2::new(10.0, 20.0)).unwrap();
        let CanvasObjectKind::Connector(connector) =
            &destination.canvas.object(pasted.objects[0]).unwrap().kind
        else {
            panic!("expected connector");
        };
        assert_eq!(connector.end_marker.unwrap().kind, kind);
        assert_eq!(connector.end_marker_size, Some(12.5));
        assert!(connector.end_marker_unfilled);
    }
}

#[test]
fn oval_marker_board_clipboards_require_version_nine() {
    let mut source = Sheet::new("Source", "Root");
    let mut connector = CanvasConnector::new(
        Endpoint::Point(Vec2::ZERO),
        Endpoint::Point(Vec2::new(100.0, 0.0)),
    );
    connector.arrow_start = true;
    connector.start_marker = Some(crate::ConnectorMarker {
        kind: crate::ConnectorMarkerKind::Diamond,
    });
    connector.end_marker = Some(crate::ConnectorMarker {
        kind: crate::ConnectorMarkerKind::Oval,
    });
    connector.end_marker_size = Some(12.5);
    connector.end_marker_unfilled = true;
    let id = source
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    let payload = copy(&source, &[], &[id]);
    let encoded = encode_board_payload(&payload).unwrap();
    assert!(encoded.starts_with(OVAL_MARKER_BOARD_PREFIX));
    assert_eq!(decode_board_payload(&encoded).unwrap(), payload);
    assert_eq!(
        decode_board_payload(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
        payload
    );
    let forged = encoded.replacen("THINODA-BOARD-CLIPBOARD-9", "THINODA-BOARD-CLIPBOARD-8", 1);
    assert!(
        decode_board_payload(&forged)
            .unwrap_err()
            .contains("oval connector markers require board clipboard version 9")
    );
    let mut destination = Sheet::new("Destination", "Root");
    let pasted = paste_board_selection(&mut destination, &payload, Vec2::new(10.0, 20.0)).unwrap();
    let CanvasObjectKind::Connector(connector) =
        &destination.canvas.object(pasted.objects[0]).unwrap().kind
    else {
        panic!("expected connector");
    };
    assert_eq!(
        connector.start_marker.unwrap().kind,
        crate::ConnectorMarkerKind::Diamond
    );
    assert_eq!(
        connector.end_marker.unwrap().kind,
        crate::ConnectorMarkerKind::Oval
    );
    assert_eq!(connector.end_marker_size, Some(12.5));
    assert!(connector.end_marker_unfilled);
}

#[test]
fn half_circle_board_clipboards_require_version_ten() {
    let mut source = Sheet::new("Source", "Root");
    let mut connector = CanvasConnector::new(
        Endpoint::Point(Vec2::ZERO),
        Endpoint::Point(Vec2::new(100.0, 0.0)),
    );
    connector.arrow_start = true;
    connector.start_marker = Some(crate::ConnectorMarker {
        kind: crate::ConnectorMarkerKind::Oval,
    });
    connector.end_marker = Some(crate::ConnectorMarker {
        kind: crate::ConnectorMarkerKind::HalfCircle,
    });
    connector.start_marker_size = Some(8.0);
    connector.end_marker_size = Some(12.5);
    connector.end_marker_unfilled = true;
    let id = source
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    let payload = copy(&source, &[], &[id]);
    let encoded = encode_board_payload(&payload).unwrap();
    assert!(encoded.starts_with(HALF_CIRCLE_MARKER_BOARD_PREFIX));
    assert_eq!(decode_board_payload(&encoded).unwrap(), payload);
    assert_eq!(
        decode_board_payload(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
        payload
    );
    let forged = encoded.replacen("THINODA-BOARD-CLIPBOARD-10", "THINODA-BOARD-CLIPBOARD-9", 1);
    assert!(
        decode_board_payload(&forged)
            .unwrap_err()
            .contains("half-circle connector markers require board clipboard version 10")
    );
    for version in 1..9 {
        let forged = encoded.replacen(
            "THINODA-BOARD-CLIPBOARD-10",
            &format!("THINODA-BOARD-CLIPBOARD-{version}"),
            1,
        );
        assert!(decode_board_payload(&forged).is_err(), "version {version}");
    }
    let mut destination = Sheet::new("Destination", "Root");
    let pasted = paste_board_selection(&mut destination, &payload, Vec2::new(10.0, 20.0)).unwrap();
    let CanvasObjectKind::Connector(connector) =
        &destination.canvas.object(pasted.objects[0]).unwrap().kind
    else {
        panic!("expected connector");
    };
    assert_eq!(
        connector.start_marker.unwrap().kind,
        crate::ConnectorMarkerKind::Oval
    );
    assert_eq!(
        connector.end_marker.unwrap().kind,
        crate::ConnectorMarkerKind::HalfCircle
    );
    assert_eq!(connector.start_marker_size, Some(8.0));
    assert_eq!(connector.end_marker_size, Some(12.5));
    assert!(connector.end_marker_unfilled);
}

#[test]
fn bar_marker_board_clipboards_require_version_eleven() {
    for kind in [
        crate::ConnectorMarkerKind::BaseDash,
        crate::ConnectorMarkerKind::Dash,
    ] {
        for bar_at_start in [false, true] {
            let mut source = Sheet::new("Source", "Root");
            let mut connector = CanvasConnector::new(
                Endpoint::Point(Vec2::ZERO),
                Endpoint::Point(Vec2::new(100.0, 0.0)),
            );
            connector.arrow_start = true;
            let bar = Some(crate::ConnectorMarker { kind });
            let half_circle = Some(crate::ConnectorMarker {
                kind: crate::ConnectorMarkerKind::HalfCircle,
            });
            (connector.start_marker, connector.end_marker) = if bar_at_start {
                (bar, half_circle)
            } else {
                (half_circle, bar)
            };
            connector.start_marker_size = Some(0.0);
            connector.end_marker_size = Some(12.5);
            connector.start_marker_unfilled = true;
            connector.end_marker_unfilled = true;
            let id = source
                .add_canvas_object(CanvasObject::connector(connector.clone()))
                .unwrap();
            let payload = copy(&source, &[], &[id]);
            let encoded = encode_board_payload(&payload).unwrap();
            assert!(encoded.starts_with(BAR_MARKER_BOARD_PREFIX));
            assert_eq!(decode_board_payload(&encoded).unwrap(), payload);
            assert_eq!(
                decode_board_payload(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
                payload
            );
            for version in 1..11 {
                let forged = encoded.replacen(
                    "THINODA-BOARD-CLIPBOARD-11",
                    &format!("THINODA-BOARD-CLIPBOARD-{version}"),
                    1,
                );
                let error = decode_board_payload(&forged).unwrap_err();
                if version == 10 {
                    assert!(
                        error.contains("bar connector markers require board clipboard version 11")
                    );
                }
            }
            let mut destination = Sheet::new("Destination", "Root");
            let pasted =
                paste_board_selection(&mut destination, &payload, Vec2::new(10.0, 20.0)).unwrap();
            let CanvasObjectKind::Connector(pasted) =
                &destination.canvas.object(pasted.objects[0]).unwrap().kind
            else {
                panic!("expected connector");
            };
            assert_eq!(pasted.start_marker, connector.start_marker);
            assert_eq!(pasted.end_marker, connector.end_marker);
            assert_eq!(pasted.start_marker_size, connector.start_marker_size);
            assert_eq!(pasted.end_marker_size, connector.end_marker_size);
            assert!(pasted.start_marker_unfilled && pasted.end_marker_unfilled);
        }
    }
}

#[test]
fn shape_rounding_board_clipboard_requires_version_twelve() {
    use crate::{DrawioRectangleStencil, Rect, ShapeRounding};

    let mut source = Sheet::new("Source", "Root");
    let rounding = ShapeRounding {
        radius: 15.0,
        stencil: DrawioRectangleStencil::Graphics,
        ..Default::default()
    };
    let mut shape = CanvasObject::shape(
        ShapeKind::RoundedRectangle,
        Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
    );
    shape.rounding = Some(rounding);
    let id = source.add_canvas_object(shape).unwrap();
    let mut connector = CanvasConnector::new(
        Endpoint::Object(id),
        Endpoint::Point(Vec2::new(200.0, 100.0)),
    );
    connector.end_marker = Some(crate::ConnectorMarker {
        kind: crate::ConnectorMarkerKind::Dash,
    });
    let edge = source
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    let payload = copy(&source, &[], &[id, edge]);
    let encoded = encode_board_payload(&payload).unwrap();
    assert!(encoded.starts_with(ROUNDING_BOARD_PREFIX));
    assert_eq!(decode_board_payload(&encoded).unwrap(), payload);
    assert_eq!(
        decode_board_payload(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
        payload
    );
    for version in 1..12 {
        let forged = encoded.replacen(
            "THINODA-BOARD-CLIPBOARD-12",
            &format!("THINODA-BOARD-CLIPBOARD-{version}"),
            1,
        );
        assert!(decode_board_payload(&forged).is_err(), "version {version}");
    }
    let mut destination = Sheet::new("Destination", "Root");
    let pasted = paste_board_selection(&mut destination, &payload, Vec2::new(10.0, 20.0)).unwrap();
    assert_eq!(
        destination
            .canvas
            .object(pasted.objects[0])
            .unwrap()
            .rounding,
        Some(rounding)
    );
}

#[test]
fn final_state_board_clipboard_requires_version_seventeen() {
    use crate::Rect;

    let mut source = Sheet::new("Source", "Root");
    let mut shape = CanvasObject::shape(
        ShapeKind::FinalState,
        Rect::from_min_size(Vec2::ZERO, Vec2::new(30.0, 30.0)),
    );
    shape.rotation = 37.0;
    let original = shape.clone();
    let id = source.add_canvas_object(shape).unwrap();
    let payload = copy(&source, &[], &[id]);
    let encoded = encode_board_payload(&payload).unwrap();
    assert!(encoded.starts_with(FINAL_STATE_BOARD_PREFIX));
    assert_eq!(decode_board_payload(&encoded).unwrap(), payload);
    assert_eq!(
        decode_board_payload(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
        payload
    );
    for version in 1..17 {
        let forged = encoded.replacen(
            "THINODA-BOARD-CLIPBOARD-17",
            &format!("THINODA-BOARD-CLIPBOARD-{version}"),
            1,
        );
        assert!(decode_board_payload(&forged).is_err(), "version {version}");
    }
    let mut destination = Sheet::new("Destination", "Root");
    let pasted = paste_board_selection(&mut destination, &payload, Vec2::new(10.0, 20.0)).unwrap();
    let restored = destination.canvas.object(pasted.objects[0]).unwrap();
    assert_eq!(restored.kind, original.kind);
    assert_eq!(restored.style, original.style);
    assert_eq!(restored.rotation, original.rotation);
}

#[test]
fn drawio_rectangle_rounding_board_clipboard_requires_version_sixteen() {
    use crate::{DrawioRectangleStencil, Rect, ShapeRounding, ShapeRoundingMode};

    let mut source = Sheet::new("Source", "Root");
    let rounding = ShapeRounding {
        radius: 21.5,
        stencil: DrawioRectangleStencil::Standard,
        mode: ShapeRoundingMode::AbsoluteArc,
        rounded_perimeter: true,
    };
    let mut shape = CanvasObject::shape(
        ShapeKind::RoundedRectangle,
        Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
    );
    shape.rounding = Some(rounding);
    let id = source.add_canvas_object(shape).unwrap();
    let payload = copy(&source, &[], &[id]);
    let encoded = encode_board_payload(&payload).unwrap();
    assert!(encoded.starts_with(DRAWIO_ROUNDING_BOARD_PREFIX));
    assert_eq!(decode_board_payload(&encoded).unwrap(), payload);
    assert_eq!(
        decode_board_payload(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
        payload
    );
    for version in 1..16 {
        let forged = encoded.replacen(
            "THINODA-BOARD-CLIPBOARD-16",
            &format!("THINODA-BOARD-CLIPBOARD-{version}"),
            1,
        );
        assert!(decode_board_payload(&forged).is_err(), "version {version}");
    }
    let mut destination = Sheet::new("Destination", "Root");
    let pasted = paste_board_selection(&mut destination, &payload, Vec2::new(10.0, 20.0)).unwrap();
    assert_eq!(
        destination
            .canvas
            .object(pasted.objects[0])
            .unwrap()
            .rounding,
        Some(rounding)
    );
}

#[test]
fn rectangle_sides_board_clipboard_requires_version_thirteen() {
    use crate::{Rect, RectangleSides};

    let mut source = Sheet::new("Source", "Root");
    let sides = RectangleSides {
        top: false,
        right: false,
        bottom: true,
        left: true,
    };
    let mut shape = CanvasObject::shape(
        ShapeKind::Rectangle,
        Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
    );
    shape.rectangle_sides = Some(sides);
    let id = source.add_canvas_object(shape).unwrap();
    let payload = copy(&source, &[], &[id]);
    let encoded = encode_board_payload(&payload).unwrap();
    assert!(encoded.starts_with(RECTANGLE_SIDES_BOARD_PREFIX));
    assert_eq!(decode_board_payload(&encoded).unwrap(), payload);
    assert_eq!(
        decode_board_payload(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
        payload
    );
    for version in 1..13 {
        let forged = encoded.replacen(
            "THINODA-BOARD-CLIPBOARD-13",
            &format!("THINODA-BOARD-CLIPBOARD-{version}"),
            1,
        );
        assert!(decode_board_payload(&forged).is_err(), "version {version}");
    }
    let mut destination = Sheet::new("Destination", "Root");
    let pasted = paste_board_selection(&mut destination, &payload, Vec2::new(10.0, 20.0)).unwrap();
    assert_eq!(
        destination
            .canvas
            .object(pasted.objects[0])
            .unwrap()
            .rectangle_sides,
        Some(sides)
    );
}

#[test]
fn fixed_connector_port_clipboards_preserve_or_detach_the_attachment() {
    use crate::ConnectorPort;

    let mut source = Sheet::new("Source", "Root");
    let shape = source
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            rect(100.0, 100.0, 100.0, 60.0),
        ))
        .unwrap();
    let port = ConnectorPort {
        position: Vec2::new(1.0, 0.25),
        offset: Vec2::ZERO,
        perimeter: false,
    };
    let mut connector = CanvasConnector::new(
        Endpoint::Object(shape),
        Endpoint::Point(Vec2::new(300.0, 100.0)),
    );
    connector.from_port = Some(port);
    let edge = source
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();

    let payload = copy(&source, &[], &[shape, edge]);
    let encoded = encode_board_payload(&payload).unwrap();
    assert!(encoded.starts_with(FIXED_PORT_BOARD_PREFIX));
    assert_eq!(decode_board_payload(&encoded).unwrap(), payload);
    assert_eq!(
        decode_board_payload(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
        payload
    );
    for version in 1..15 {
        let forged = encoded.replacen(
            "THINODA-BOARD-CLIPBOARD-15",
            &format!("THINODA-BOARD-CLIPBOARD-{version}"),
            1,
        );
        assert!(decode_board_payload(&forged).is_err(), "version {version}");
    }

    let mut destination = Sheet::new("Destination", "Root");
    let pasted = paste_board_selection(&mut destination, &payload, Vec2::new(10.0, 20.0)).unwrap();
    let CanvasObjectKind::Connector(connector) =
        &destination.canvas.object(pasted.objects[1]).unwrap().kind
    else {
        panic!("expected connector");
    };
    assert_eq!(connector.from, Endpoint::Object(pasted.objects[0]));
    assert_eq!(connector.from_port, Some(port));
    destination.validate().unwrap();

    let detached = copy(&source, &[], &[edge]);
    let CanvasObjectKind::Connector(connector) = &detached.fragment.canvas.objects[0].kind else {
        panic!("expected connector");
    };
    assert_eq!(connector.from, Endpoint::Point(Vec2::new(200.0, 115.0)));
    assert_eq!(connector.from_port, None);
    assert!(
        !encode_board_payload(&detached)
            .unwrap()
            .starts_with(FIXED_PORT_BOARD_PREFIX)
    );
}

#[test]
fn double_ellipse_board_clipboard_requires_version_fourteen() {
    use crate::{Rect, RectangleSides};

    for margin in [None, Some(8.5)] {
        let mut source = Sheet::new("Source", "Root");
        let mut ellipse = CanvasObject::shape(
            ShapeKind::DoubleEllipse,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
        );
        ellipse.double_ellipse_margin = margin;
        let id = source.add_canvas_object(ellipse).unwrap();
        let mut rectangle = CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::new(120.0, 0.0), Vec2::new(100.0, 60.0)),
        );
        rectangle.rectangle_sides = Some(RectangleSides::default());
        let rectangle_id = source.add_canvas_object(rectangle).unwrap();
        let payload = copy(&source, &[], &[id, rectangle_id]);
        let encoded = encode_board_payload(&payload).unwrap();
        assert!(encoded.starts_with(DOUBLE_ELLIPSE_BOARD_PREFIX));
        assert_eq!(decode_board_payload(&encoded).unwrap(), payload);
        assert_eq!(
            decode_board_payload(&encoded.replacen('\n', "\r\n", 1)).unwrap(),
            payload
        );
        for version in 1..14 {
            let forged = encoded.replacen(
                "THINODA-BOARD-CLIPBOARD-14",
                &format!("THINODA-BOARD-CLIPBOARD-{version}"),
                1,
            );
            assert!(decode_board_payload(&forged).is_err(), "version {version}");
        }
        for value in [serde_json::json!(-1), serde_json::json!("invalid")] {
            let mut malformed = serde_json::to_value(&payload).unwrap();
            malformed["fragment"]["canvas"]["objects"][0]["double_ellipse_margin"] = value;
            let forged = format!("{DOUBLE_ELLIPSE_BOARD_PREFIX}{malformed}");
            assert!(decode_board_payload(&forged).is_err());
        }
        let mut destination = Sheet::new("Destination", "Root");
        let pasted =
            paste_board_selection(&mut destination, &payload, Vec2::new(10.0, 20.0)).unwrap();
        let restored = destination.canvas.object(pasted.objects[0]).unwrap();
        assert!(matches!(
            restored.kind,
            CanvasObjectKind::Shape {
                shape: ShapeKind::DoubleEllipse,
                ..
            }
        ));
        assert_eq!(restored.double_ellipse_margin, margin);
    }
}

#[test]
fn connectors_keep_selected_targets_and_detach_external_targets_at_rendered_points() {
    let mut source = Sheet::new("Source", "Root");
    let external_topic = source.add_child(source.root, None, "External");
    let card = source
        .add_card(
            "Included",
            Vec2::new(100.0, 220.0),
            Card::new(CardKind::Text, Vec2::new(80.0, 50.0)),
        )
        .unwrap();
    let external_object = source
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            rect(300.0, 40.0, 90.0, 90.0),
        ))
        .unwrap();
    let first = source
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Topic(external_topic),
            Endpoint::Object(external_object),
        )))
        .unwrap();
    let second = source
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(first),
            Endpoint::Topic(card),
        )))
        .unwrap();
    let layout = layout_sheet(&source, &ApproxMeasure, false);
    let CanvasObjectKind::Connector(first_connector) = &source.canvas.object(first).unwrap().kind
    else {
        panic!("expected connector");
    };
    let first_path = crate::scene::connector_path(&source, &layout, first_connector);
    let CanvasObjectKind::Connector(second_connector) = &source.canvas.object(second).unwrap().kind
    else {
        panic!("expected connector");
    };
    let second_path = crate::scene::connector_path(&source, &layout, second_connector);
    assert!(first_path.len() >= 2 && second_path.len() >= 2);

    let payload = copy_board_selection(&source, &layout, &[card], &[first, second])
        .unwrap()
        .unwrap();
    let mut target = Sheet::new("Target", "Unrelated");
    let delta = Vec2::new(20.0, 30.0);
    let result = paste_board_selection(&mut target, &payload, delta).unwrap();
    let CanvasObjectKind::Connector(copied_first) =
        &target.canvas.object(result.objects[0]).unwrap().kind
    else {
        panic!("expected connector");
    };
    assert_eq!(copied_first.from, Endpoint::Point(first_path[0] + delta));
    assert_eq!(
        copied_first.to,
        Endpoint::Point(*first_path.last().unwrap() + delta)
    );
    let CanvasObjectKind::Connector(copied_second) =
        &target.canvas.object(result.objects[1]).unwrap().kind
    else {
        panic!("expected connector");
    };
    assert_eq!(copied_second.from, Endpoint::Object(result.objects[0]));
    assert_eq!(copied_second.to, Endpoint::Topic(result.topics[0]));
    target.validate().unwrap();

    // A reference to an unselected connector is resolved just like an
    // unselected shape or topic, not left as a source-document ID.
    let detached = copy_board_selection(&source, &layout, &[], &[second])
        .unwrap()
        .unwrap();
    let mut target = Sheet::new("Target", "Root");
    let result = paste_board_selection(&mut target, &detached, delta).unwrap();
    let CanvasObjectKind::Connector(copied) =
        &target.canvas.object(result.objects[0]).unwrap().kind
    else {
        panic!("expected connector");
    };
    assert_eq!(copied.from, Endpoint::Point(second_path[0] + delta));
    assert_eq!(
        copied.to,
        Endpoint::Point(*second_path.last().unwrap() + delta)
    );
    target.validate().unwrap();

    // Never invent a fallback point if the caller's layout cannot resolve a
    // detached target; leave the caller's sheet untouched.
    let before = source.clone();
    assert!(copy_board_selection(&source, &Layout::default(), &[], &[first]).is_err());
    assert_eq!(source, before);
}

#[test]
fn rootless_and_central_topic_copies_never_expose_or_attach_to_anchor() {
    let mut source = hidden_sheet("Hidden", "private old anchor title");
    let first = source.add_floating("First", Vec2::new(20.0, 40.0));
    let second = source.add_floating("Second", Vec2::new(200.0, 60.0));
    let layout = layout_sheet(&source, &ApproxMeasure, false);
    assert!(
        copy_board_selection(&source, &layout, &[source.root], &[])
            .unwrap()
            .is_none()
    );
    let payload = copy_board_selection(&source, &layout, &[source.root, first, second], &[])
        .unwrap()
        .unwrap();
    assert_eq!(payload.text, "First\nSecond\n");
    assert!(
        !encode_board_payload(&payload)
            .unwrap()
            .contains("private old anchor title")
    );

    let mut central = Sheet::new("Central", "A central topic");
    let central_root = central.root;
    let pasted = paste_board_selection(&mut central, &payload, Vec2::ZERO).unwrap();
    assert_eq!(central.root, central_root);
    assert_eq!(central.root_topic().title, "A central topic");
    assert_eq!(central.floating, pasted.topics);
    central.validate().unwrap();

    // A visible root is ordinary selected content, not a hidden anchor. Its
    // layout position is used when it becomes a floating root.
    let visible_layout = layout_sheet(&central, &ApproxMeasure, false);
    let root_payload = copy_board_selection(&central, &visible_layout, &[central.root], &[])
        .unwrap()
        .unwrap();
    let mut hidden_target = hidden_sheet("Target", "different private anchor");
    let result = paste_board_selection(&mut hidden_target, &root_payload, Vec2::ZERO).unwrap();
    assert_eq!(
        hidden_target.topic(result.topics[0]).unwrap().title,
        "A central topic"
    );
    assert_eq!(hidden_target.floating, result.topics);
    assert!(hidden_target.children(hidden_target.root).is_empty());
    hidden_target.validate().unwrap();
}

#[test]
fn copying_locked_groups_is_read_only_but_cut_and_move_remain_blocked() {
    let mut sheet = Sheet::new("Source", "Root");
    let card = sheet
        .add_card(
            "Card",
            Vec2::new(100.0, 80.0),
            Card::new(CardKind::Sticky, Vec2::new(80.0, 60.0)),
        )
        .unwrap();
    let shape = sheet
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Ellipse,
            rect(200.0, 60.0, 60.0, 60.0),
        ))
        .unwrap();
    let group = sheet
        .group_board_members(&[BoardMember::Card(card), BoardMember::Object(shape)])
        .unwrap();
    sheet.set_canvas_group_locked(group, true).unwrap();
    let before = sheet.clone();
    let payload = copy(&sheet, &[], &[shape]);
    assert_eq!(sheet, before);
    assert_eq!(payload.source_topic_roots(), &[card]);
    assert_eq!(payload.source_object_ids().collect::<Vec<_>>(), vec![shape]);
    assert!(
        sheet
            .remove_board_members(&[BoardMember::Object(shape)])
            .is_err()
    );
    assert_eq!(sheet, before);
    let mut target = Sheet::new("Target", "Root");
    let result = paste_board_selection(&mut target, &payload, Vec2::ZERO).unwrap();
    assert!(target.is_board_member_locked(BoardMember::Card(result.topics[0])));
    assert!(target.is_board_member_locked(BoardMember::Object(result.objects[0])));
}

#[test]
fn paste_is_one_undoable_edit_and_failed_paste_does_not_change_history() {
    let mut source = hidden_sheet("Source", "");
    let topic = source.add_floating("Idea", Vec2::new(100.0, 100.0));
    let object = source
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            rect(20.0, 40.0, 50.0, 60.0),
        ))
        .unwrap();
    let payload = copy(&source, &[topic], &[object]);
    let destination = hidden_sheet("Target", "");
    let mut doc = Document::new(Workbook::new(destination));
    let before = doc.sheet().clone();
    assert!(
        doc.edit_sheet("Paste", |sheet| {
            paste_board_selection(sheet, &payload, Vec2::new(f32::NAN, 0.0))
                .map_err(EditError::Invalid)
        })
        .is_err()
    );
    assert_eq!(doc.sheet(), &before);
    assert!(!doc.can_undo());
    doc.edit_sheet("Paste", |sheet| {
        paste_board_selection(sheet, &payload, Vec2::new(20.0, 20.0)).map_err(EditError::Invalid)
    })
    .unwrap();
    let after = doc.sheet().clone();
    assert_eq!(doc.undo_label(), Some("Paste"));
    assert!(doc.undo());
    assert_eq!(doc.sheet(), &before);
    assert!(!doc.can_undo());
    assert!(doc.redo());
    assert_eq!(doc.sheet(), &after);
}

#[test]
fn invalid_payloads_and_out_of_range_pastes_are_rejected_atomically() {
    let mut source = hidden_sheet("Source", "");
    let topic = source.add_floating("Idea", Vec2::new(10.0, 20.0));
    let object = source
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            rect(10.0, 20.0, 30.0, 40.0),
        ))
        .unwrap();
    let payload = copy(&source, &[topic], &[object]);
    let mut destination = hidden_sheet("Target", "");
    let before = destination.clone();

    for delta in [
        Vec2::new(f32::NAN, 0.0),
        Vec2::new(f32::INFINITY, 0.0),
        Vec2::new(crate::board::MAX_BOARD_COORDINATE, 0.0),
    ] {
        assert!(paste_board_selection(&mut destination, &payload, delta).is_err());
        assert_eq!(destination, before);
    }

    let mut bad = payload.clone();
    bad.fragment.topic_mut(bad.fragment.root).unwrap().title = "leak".into();
    assert!(paste_board_selection(&mut destination, &bad, Vec2::ZERO).is_err());
    assert_eq!(destination, before);

    let mut bad = payload.clone();
    bad.fragment.canvas.objects[0].kind = CanvasObjectKind::Connector(CanvasConnector::new(
        Endpoint::Topic(bad.fragment.root),
        Endpoint::Point(Vec2::ZERO),
    ));
    assert!(paste_board_selection(&mut destination, &bad, Vec2::ZERO).is_err());
    assert_eq!(destination, before);

    let mut bad = payload.clone();
    bad.fragment.canvas.objects[0].style.opacity = f32::NAN;
    assert!(paste_board_selection(&mut destination, &bad, Vec2::ZERO).is_err());
    assert_eq!(destination, before);

    let mut bad = payload.clone();
    bad.fragment.floating.push(bad.fragment.floating[0]);
    assert!(paste_board_selection(&mut destination, &bad, Vec2::ZERO).is_err());
    assert_eq!(destination, before);

    let mut bad = payload.clone();
    bad.fragment.canvas.objects[0].group = Some(CanvasGroupId(999));
    assert!(paste_board_selection(&mut destination, &bad, Vec2::ZERO).is_err());
    assert_eq!(destination, before);

    let mut bad = payload.clone();
    bad.fragment.canvas.objects[0].id = CanvasObjectId(u64::MAX);
    assert!(paste_board_selection(&mut destination, &bad, Vec2::ZERO).is_err());
    assert_eq!(destination, before);

    let mut bad = payload.clone();
    bad.text = "x".repeat(MAX_BOARD_CLIPBOARD_BYTES + 1);
    assert!(paste_board_selection(&mut destination, &bad, Vec2::ZERO).is_err());
    assert_eq!(destination, before);
    assert!(encode_board_payload(&bad).is_err());
    assert!(decode_board_payload(&"x".repeat(MAX_BOARD_CLIPBOARD_BYTES + 1)).is_err());
    assert!(decode_board_payload("THINODA-BOARD-CLIPBOARD-1\n{").is_err());
    assert!(decode_board_payload("THINODA-BOARD-CLIPBOARD-2\n{}").is_err());
}

#[test]
fn old_topic_payload_and_unrelated_plain_text_still_resolve_as_before() {
    let mut sheet = Sheet::new("S", "Root");
    let topic = sheet.add_child(sheet.root, None, "Legacy");
    sheet
        .topic_mut(topic)
        .unwrap()
        .markers
        .push("flag-red".into());
    let old = crate::clipboard::copy_topics(&sheet, &[topic])
        .unwrap()
        .unwrap();
    let serialized = serde_json::to_string(&old).unwrap();
    let old: ClipboardPayload = serde_json::from_str(&serialized).unwrap();
    let ResolvedBoardPaste::Topics(topics) =
        resolve_board_paste(None, Some(&old), Some("Legacy\r\n")).unwrap()
    else {
        panic!("expected legacy topic clipboard");
    };
    assert_eq!(topics[0].root_topic().markers, vec!["flag-red"]);
    let ResolvedBoardPaste::Topics(topics) =
        resolve_board_paste(None, Some(&old), Some("External")).unwrap()
    else {
        panic!("expected external text");
    };
    assert_eq!(topics[0].root_topic().title, "External");
    assert!(topics[0].root_topic().markers.is_empty());
}

#[test]
fn plain_text_with_reused_ids_pastes_as_separate_floating_roots() {
    let subtrees = crate::clipboard::subtrees_from_text("One\n\tChild\nTwo").unwrap();
    // Each line root was parsed in a separate temporary Sheet.
    assert_eq!(subtrees[0].root, subtrees[1].root);
    let mut destination = hidden_sheet("Target", "private anchor");
    let root = destination.root;
    let pasted =
        paste_floating_subtrees(&mut destination, &subtrees, Vec2::new(100.0, 200.0)).unwrap();
    assert_eq!(pasted.len(), 2);
    assert_ne!(pasted[0], pasted[1]);
    assert_ne!(pasted[0], subtrees[0].root);
    assert_ne!(pasted[1], subtrees[1].root);
    assert_eq!(destination.topic(pasted[0]).unwrap().title, "One");
    assert_eq!(destination.topic(pasted[1]).unwrap().title, "Two");
    let child = destination.children(pasted[0])[0];
    assert_eq!(destination.topic(child).unwrap().title, "Child");
    assert_eq!(
        destination.topic(pasted[0]).unwrap().position,
        Some(Vec2::new(100.0, 200.0))
    );
    assert_eq!(
        destination.topic(pasted[1]).unwrap().position,
        Some(Vec2::new(100.0, 268.0))
    );
    assert_eq!(destination.root, root);
    assert_eq!(destination.root_topic().title, "private anchor");
    assert!(destination.children(root).is_empty());
    destination.validate().unwrap();
}

#[test]
fn floating_legacy_subtrees_preserve_cards_and_resources_and_validate_atomically() {
    let mut source = Sheet::new("Source", "Root");
    let card = source
        .add_card(
            "Legacy card",
            Vec2::new(10.0, 20.0),
            Card::new(CardKind::Sticky, Vec2::new(100.0, 80.0)),
        )
        .unwrap();
    source.topic_mut(card).unwrap().style.bold = Some(true);
    let reference = source
        .resources
        .insert(
            crate::resources::Resource::new("file.txt", "text/plain", b"legacy".to_vec()).unwrap(),
        )
        .unwrap();
    source.topic_mut(card).unwrap().attachment = Some(crate::attachments::Attachment {
        resource: reference.clone(),
        name: "file.txt".into(),
    });
    let payload = crate::clipboard::copy_topics(&source, &[card])
        .unwrap()
        .unwrap();
    let mut destination = hidden_sheet("Target", "");
    let pasted =
        paste_floating_subtrees(&mut destination, &payload.subtrees, Vec2::new(40.0, 60.0))
            .unwrap();
    let copied = destination.topic(pasted[0]).unwrap();
    assert_eq!(copied.card, source.topic(card).unwrap().card);
    assert_eq!(copied.style, source.topic(card).unwrap().style);
    assert_eq!(copied.attachment, source.topic(card).unwrap().attachment);
    assert_eq!(copied.position, Some(Vec2::new(40.0, 60.0)));
    assert_eq!(
        destination.resources.get(&reference).unwrap().bytes(),
        b"legacy"
    );
    destination.validate().unwrap();

    // A legacy payload may omit bytes already present in the destination.
    let mut existing_resource = payload.subtrees.clone();
    existing_resource[0].resources.entries.clear();
    let additional =
        paste_floating_subtrees(&mut destination, &existing_resource, Vec2::new(200.0, 60.0))
            .unwrap();
    assert_eq!(
        destination.topic(additional[0]).unwrap().attachment,
        source.topic(card).unwrap().attachment
    );
    destination.validate().unwrap();

    let before = destination.clone();
    let mut malformed = payload.subtrees.clone();
    let duplicate = malformed[0].topics[0].clone();
    malformed[0].topics.push(duplicate);
    assert!(paste_floating_subtrees(&mut destination, &malformed, Vec2::ZERO).is_err());
    assert_eq!(destination, before);
    let mut malformed = payload.subtrees.clone();
    malformed[0].root = TopicId(u64::MAX);
    assert!(paste_floating_subtrees(&mut destination, &malformed, Vec2::ZERO).is_err());
    assert_eq!(destination, before);
    let mut malformed = payload.subtrees.clone();
    malformed[0].topics.clear();
    assert!(paste_floating_subtrees(&mut destination, &malformed, Vec2::ZERO).is_err());
    assert_eq!(destination, before);
    assert!(
        paste_floating_subtrees(
            &mut destination,
            &payload.subtrees,
            Vec2::new(f32::NAN, 0.0)
        )
        .is_err()
    );
    assert_eq!(destination, before);
}

#[test]
fn floating_subtrees_merge_multiple_resources_atomically_and_preserve_undo() {
    let mut source = hidden_sheet("Source", "");
    let mut cards = Vec::new();
    let bytes = [b"shared".as_slice(), b"second", b"third", b"second"];
    for (index, bytes) in bytes.into_iter().enumerate() {
        let name = format!("source-{index}.txt");
        let reference = source
            .resources
            .insert(crate::resources::Resource::new(&name, "text/plain", bytes.to_vec()).unwrap())
            .unwrap();
        let card = source
            .add_card(
                &name,
                Vec2::new(index as f32 * 160.0, 0.0),
                Card::new(CardKind::Sticky, Vec2::new(120.0, 80.0)),
            )
            .unwrap();
        source.topic_mut(card).unwrap().attachment = Some(crate::attachments::Attachment {
            resource: reference,
            name,
        });
        cards.push(card);
    }
    let mut payload = crate::clipboard::copy_topics(&source, &cards)
        .unwrap()
        .unwrap();
    payload.subtrees[3]
        .resources
        .entries
        .values_mut()
        .next()
        .unwrap()
        .name = "later-duplicate.txt".into();
    let mut destination = hidden_sheet("Destination", "");
    let shared = destination
        .resources
        .insert(
            crate::resources::Resource::new("destination.txt", "text/plain", b"shared".to_vec())
                .unwrap(),
        )
        .unwrap();
    let mut doc = Document::new(Workbook::new(destination));
    let before = doc.workbook().clone();
    let mut malformed = payload.subtrees.clone();
    let resources = &mut malformed[1].resources.entries;
    let (id, resource) = resources.pop_first().unwrap();
    assert_ne!(id, "0".repeat(64));
    resources.insert("0".repeat(64), resource);
    assert!(
        doc.edit_sheet("Invalid paste", |sheet| {
            paste_floating_subtrees(sheet, &malformed, Vec2::ZERO).map_err(EditError::Invalid)
        })
        .is_err()
    );
    assert_eq!(doc.workbook(), &before);
    assert!(!doc.can_undo());

    let pasted = doc
        .edit_sheet("Paste", |sheet| {
            paste_floating_subtrees(sheet, &payload.subtrees, Vec2::new(50.0, 60.0))
                .map_err(EditError::Invalid)
        })
        .unwrap();
    assert_eq!(pasted.len(), 4);
    assert_eq!(doc.sheet().resources.entries.len(), 3);
    assert_eq!(
        doc.sheet().resources.get(&shared).unwrap().name,
        "destination.txt"
    );
    assert!(
        doc.sheet()
            .resources
            .entries
            .values()
            .any(|resource| resource.name == "source-1.txt")
    );
    for (index, id) in pasted.into_iter().enumerate() {
        let attachment = doc.sheet().topic(id).unwrap().attachment.as_ref().unwrap();
        assert_eq!(attachment.name, format!("source-{index}.txt"));
        assert_eq!(
            doc.sheet()
                .resources
                .get(&attachment.resource)
                .unwrap()
                .bytes(),
            bytes[index]
        );
    }
    doc.workbook().validate().unwrap();
    let after = doc.workbook().clone();
    assert!(doc.undo());
    assert_eq!(doc.workbook(), &before);
    assert!(doc.redo());
    assert_eq!(doc.workbook(), &after);
}

#[test]
fn floating_plain_text_paste_undoes_in_one_step() {
    let subtrees = crate::clipboard::subtrees_from_text("One\nTwo").unwrap();
    let mut doc = Document::new(Workbook::new(hidden_sheet("Target", "")));
    let before = doc.sheet().clone();
    doc.edit_sheet("Paste", |sheet| {
        paste_floating_subtrees(sheet, &subtrees, Vec2::new(50.0, 50.0)).map_err(EditError::Invalid)
    })
    .unwrap();
    let after = doc.sheet().clone();
    assert!(doc.undo());
    assert_eq!(doc.sheet(), &before);
    assert!(!doc.can_undo());
    assert!(doc.redo());
    assert_eq!(doc.sheet(), &after);
}

#[test]
fn connector_can_precede_its_selected_target_in_paint_order() {
    let mut source = Sheet::new("Source", "Root");
    let shape = source
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            rect(40.0, 40.0, 50.0, 50.0),
        ))
        .unwrap();
    let connector = source
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(shape),
            Endpoint::Point(Vec2::new(200.0, 100.0)),
        )))
        .unwrap();
    source.canvas.objects.swap(0, 1);
    source.validate().unwrap();
    let payload = copy(&source, &[], &[connector, shape]);
    let mut destination = Sheet::new("Target", "Root");
    let pasted = paste_board_selection(&mut destination, &payload, Vec2::new(1.0, 2.0)).unwrap();
    let CanvasObjectKind::Connector(copied) =
        &destination.canvas.object(pasted.objects[0]).unwrap().kind
    else {
        panic!("expected connector first in paint order");
    };
    assert_eq!(copied.from, Endpoint::Object(pasted.objects[1]));
    assert_eq!(copied.to, Endpoint::Point(Vec2::new(201.0, 102.0)));
    destination.validate().unwrap();
}

#[test]
fn invalid_or_excessive_resources_do_not_partially_paste() {
    let mut source = Sheet::new("Source", "Root");
    let topic = source.add_floating("Attachment", Vec2::new(10.0, 20.0));
    let reference = source
        .resources
        .insert(crate::resources::Resource::new("new.txt", "text/plain", vec![255]).unwrap())
        .unwrap();
    source.topic_mut(topic).unwrap().attachment = Some(crate::attachments::Attachment {
        resource: reference,
        name: "new.txt".into(),
    });
    let payload = copy(&source, &[topic], &[]);

    let mut destination = Sheet::new("Target", "Root");
    for number in 0..crate::resources::MAX_RESOURCES {
        destination
            .resources
            .insert(
                crate::resources::Resource::new(
                    &format!("resource-{number}.txt"),
                    "text/plain",
                    vec![number as u8],
                )
                .unwrap(),
            )
            .unwrap();
    }
    let before = destination.clone();
    assert!(paste_board_selection(&mut destination, &payload, Vec2::ZERO).is_err());
    assert_eq!(destination, before);

    let mut missing = payload.clone();
    missing.fragment.resources.entries.clear();
    assert!(paste_board_selection(&mut destination, &missing, Vec2::ZERO).is_err());
    assert_eq!(destination, before);
    let malformed = format!("{BOARD_PREFIX}{}", serde_json::to_string(&missing).unwrap());
    assert!(decode_board_payload(&malformed).is_err());
}

#[test]
fn pasting_over_the_object_limit_is_atomic() {
    let mut source = Sheet::new("Source", "Root");
    let object = source
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            rect(1.0, 2.0, 3.0, 4.0),
        ))
        .unwrap();
    let payload = copy(&source, &[], &[object]);
    let mut target = Sheet::new("Target", "Root");
    for id in 0..crate::board::MAX_CANVAS_OBJECTS {
        let mut object = CanvasObject::shape(ShapeKind::Rectangle, rect(0.0, 0.0, 1.0, 1.0));
        object.id = CanvasObjectId(id as u64 + 2);
        target.canvas.objects.push(object);
    }
    target.validate().unwrap();
    let before = target.clone();
    assert!(paste_board_selection(&mut target, &payload, Vec2::ZERO).is_err());
    assert_eq!(target, before);
}

#[test]
fn destination_with_exhausted_entity_ids_is_rejected_without_allocation() {
    let mut source = Sheet::new("Source", "Root");
    let object = source
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            rect(1.0, 2.0, 3.0, 4.0),
        ))
        .unwrap();
    let payload = copy(&source, &[], &[object]);
    for id in [u64::MAX - 1, u64::MAX] {
        let mut target = Sheet::new("Target", "Root");
        let mut existing = CanvasObject::shape(ShapeKind::Rectangle, rect(0.0, 0.0, 1.0, 1.0));
        existing.id = CanvasObjectId(id);
        target.canvas.objects.push(existing);
        target.validate().unwrap();
        let before = target.clone();
        assert!(paste_board_selection(&mut target, &payload, Vec2::ZERO).is_err());
        assert_eq!(target, before);
    }
}
