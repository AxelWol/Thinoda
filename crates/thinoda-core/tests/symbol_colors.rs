use thinoda_core::{Color, Document, Sheet, Topic, TopicId, Workbook, clipboard, markers, outline};

#[test]
fn legacy_serialization_is_unchanged_and_colors_validate() {
    let topic = Topic::new(TopicId(1), "Legacy");
    let json = serde_json::to_value(&topic).unwrap();
    assert!(json.get("marker_colors").is_none());
    assert_eq!(serde_json::from_value::<Topic>(json).unwrap(), topic);
    let mut sheet = Sheet::new("Sheet", "Root");
    sheet
        .topic_mut(sheet.root)
        .unwrap()
        .marker_colors
        .insert(0, Color::BLACK);
    assert!(sheet.validate().unwrap_err().contains("missing marker"));
}

#[test]
fn instance_edit_remove_reindex_undo_redo_and_replacement() {
    let mut sheet = Sheet::new("Sheet", "Root");
    let root = sheet.root;
    let other = sheet.insert_child(root, "Other").unwrap();
    sheet.topic_mut(root).unwrap().markers =
        vec!["star-red".into(), "star-red".into(), "month-jan".into()];
    sheet.set_marker_color(root, 0, Some(Color::BLACK)).unwrap();
    sheet.set_marker_color(root, 1, Some(Color::WHITE)).unwrap();
    sheet
        .set_marker_color(root, 2, Some(Color::rgb(10, 20, 30)))
        .unwrap();
    let mut doc = Document::new(Workbook::new(sheet));
    let before = doc.workbook().clone();
    doc.edit_sheet("Color", |s| {
        s.set_marker_color(root, 1, Some(Color::rgb(200, 0, 0)))
    })
    .unwrap();
    let edited = doc.workbook().clone();
    assert_eq!(
        doc.sheet().topic(root).unwrap().marker_colors[&0],
        Color::BLACK
    );
    doc.edit_sheet("Remove", |s| s.remove_marker(root, 1))
        .unwrap();
    let topic = doc.sheet().topic(root).unwrap();
    assert_eq!(topic.markers, ["star-red", "month-jan"]);
    assert_eq!(topic.marker_colors[&1], Color::rgb(10, 20, 30));
    assert!(!topic.marker_colors.contains_key(&2));
    assert_eq!(
        doc.sheet().topic(other).unwrap(),
        before.active().topic(other).unwrap()
    );
    assert!(doc.undo());
    assert_eq!(doc.workbook(), &edited);
    assert!(doc.undo());
    assert_eq!(doc.workbook(), &before);
    assert!(doc.redo());
    assert_eq!(doc.workbook(), &edited);
    assert!(doc.redo());
    doc.edit_sheet("Replace", |s| {
        s.toggle_marker_color(&[root], "star-blue", Some(Color::WHITE))
    })
    .unwrap();
    assert_eq!(
        doc.sheet().topic(root).unwrap().markers,
        ["star-blue", "month-jan"]
    );
    doc.edit_sheet("Toggle", |s| s.toggle_marker(&[root], "star-blue"))
        .unwrap();
    let topic = doc.sheet().topic(root).unwrap();
    assert_eq!(topic.markers, ["month-jan"]);
    assert_eq!(topic.marker_colors[&0], Color::rgb(10, 20, 30));
    let snapshot = doc.workbook().clone();
    assert!(
        doc.edit_sheet("Invalid", |s| s.remove_marker(root, 12))
            .is_err()
    );
    assert_eq!(doc.workbook(), &snapshot);
    doc.sheet().validate().unwrap();
}

#[test]
fn subtree_sheet_clipboard_outline_duplicate_and_json_preserve_colors() {
    let mut sheet = Sheet::new("Sheet", "Root");
    let child = sheet.insert_child(sheet.root, "Child").unwrap();
    sheet
        .toggle_marker_color(&[child], "task-half", Some(Color::rgba(15, 30, 90, 150)))
        .unwrap();
    let expected = sheet.topic(child).unwrap().marker_colors.clone();
    let payload = clipboard::copy_topics(&sheet, &[child]).unwrap();
    let pasted = sheet.paste(sheet.root, &payload.subtrees).unwrap()[0];
    assert_eq!(sheet.topic(pasted).unwrap().marker_colors, expected);
    let duplicate = sheet.duplicate(child).unwrap();
    assert_eq!(sheet.topic(duplicate).unwrap().marker_colors, expected);
    let outline = outline::to_outline(&sheet, child);
    let from_outline = outline::sheet_from_outline("Outline", &outline);
    assert_eq!(from_outline.root_topic().marker_colors, expected);
    let text = clipboard::encode_sheet(&sheet).unwrap();
    assert_eq!(clipboard::decode_sheet(&text).unwrap(), sheet);
    let json = serde_json::to_string(&sheet).unwrap();
    assert_eq!(serde_json::from_str::<Sheet>(&json).unwrap(), sheet);
}

#[test]
fn wcag_math_thresholds_compositing_and_explicit_mitigation() {
    assert!((Color::BLACK.contrast_on(Color::WHITE) - 21.0).abs() < 0.0001);
    assert_eq!(Color::TRANSPARENT.contrast_on(Color::WHITE), 1.0);
    assert_eq!(
        Color::rgba(0, 0, 0, 128).over(Color::WHITE),
        Color::rgb(127, 127, 127)
    );
    assert_eq!(
        Color::TRANSPARENT.over(Color::TRANSPARENT),
        Color::TRANSPARENT
    );
    assert_eq!(Color::WHITE.over(Color::BLACK), Color::WHITE);
    assert_eq!(
        Color::rgba(255, 0, 0, 128).over(Color::rgba(0, 0, 255, 128)),
        Color::rgba(170, 0, 85, 192)
    );
    assert!(Color::rgb(118, 118, 118).contrast_on(Color::WHITE) >= 4.5);
    assert!(Color::rgb(119, 119, 119).contrast_on(Color::WHITE) < 4.5);
    assert!(Color::rgb(148, 148, 148).contrast_on(Color::WHITE) >= 3.0);
    assert!(Color::rgb(149, 149, 149).contrast_on(Color::WHITE) < 3.0);
    for group in markers::GROUPS {
        for marker in group.markers {
            let threshold = if matches!(group.id, "month" | "week") {
                4.5
            } else {
                3.0
            };
            assert_eq!(markers::contrast_threshold(marker.id), threshold);
        }
    }
    for value in 0..=255 {
        let bg = Color::rgb(value, value, value);
        let correction = Color::highest_contrast_on(bg);
        assert!(correction.contrast_on(bg) >= 4.5);
        assert!(correction.contrast_on(bg) > bg.contrast_on(bg));
    }
}
