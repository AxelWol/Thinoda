use ymind_core::{
    ApproxMeasure, Color, Document, Relationship, Sheet, TopicId, Vec2, Workbook, layout_sheet,
};

fn document() -> Document {
    let mut sheet = Sheet::new("Relationships", "Center");
    let a = sheet.add_floating("A", Vec2::new(-200.0, -100.0));
    let b = sheet.add_floating("B", Vec2::new(200.0, -100.0));
    let id = sheet.add_relationship(a, b).unwrap();
    sheet.set_relationship_title(id, "Association").unwrap();
    sheet
        .set_relationship_color(id, Some(Color::rgb(23, 45, 67)))
        .unwrap();
    Document::new(Workbook {
        sheets: vec![sheet],
        active_sheet: 0,
        metadata: Default::default(),
    })
}

#[test]
fn reconnect_is_atomic_preserves_properties_and_undoes_once() {
    let mut doc = document();
    let rel = doc.sheet().relationships[0].clone();
    let points = Some([Vec2::new(15.0, -90.0), Vec2::new(-40.0, 80.0)]);
    doc.edit_sheet("Curve", |s| {
        s.set_relationship_control_points(rel.id, points)
    })
    .unwrap();
    let before = doc.workbook().clone();
    let root = doc.sheet().root;
    doc.edit_sheet("Reconnect", |s| {
        s.reconnect_relationship(rel.id, root, rel.to)
    })
    .unwrap();
    let after = doc.workbook().clone();
    let changed = &doc.sheet().relationships[0];
    assert_eq!(changed.from, root);
    assert_eq!(changed.to, rel.to);
    assert_eq!(changed.title, rel.title);
    assert_eq!(changed.color, rel.color);
    assert_eq!(changed.control_points, points);
    doc.sheet().validate().unwrap();
    assert!(doc.undo());
    assert_eq!(doc.workbook(), &before);
    assert!(doc.redo());
    assert_eq!(doc.workbook(), &after);
    let state = doc.state_id();
    assert!(
        doc.edit_sheet("Invalid self", |s| s
            .reconnect_relationship(rel.id, root, root))
            .is_err()
    );
    assert!(
        doc.edit_sheet("Missing", |s| s.reconnect_relationship(
            rel.id,
            TopicId(u64::MAX),
            root
        ))
        .is_err()
    );
    assert_eq!(doc.state_id(), state);
    assert_eq!(doc.workbook(), &after);
}

#[test]
fn curve_offsets_follow_topics_and_reset_to_automatic_with_undo() {
    let mut doc = document();
    let rel = doc.sheet().relationships[0].clone();
    let automatic = layout_sheet(doc.sheet(), &ApproxMeasure, false).relationships[0].clone();
    let points = [Vec2::new(15.0, -90.0), Vec2::new(-40.0, 80.0)];
    doc.edit_sheet("Curve", |s| {
        s.set_relationship_control_points(rel.id, Some(points))
    })
    .unwrap();
    for structure in ymind_core::Structure::ALL {
        let mut sheet = doc.sheet().clone();
        sheet.structure = structure;
        let layout = layout_sheet(&sheet, &ApproxMeasure, false);
        let curve = &layout.relationships[0];
        assert_eq!(curve.c1, layout.topics[&rel.from].rect.center() + points[0]);
        assert_eq!(curve.c2, layout.topics[&rel.to].rect.center() + points[1]);
        assert!(layout.bounds.contains(curve.c1) && layout.bounds.contains(curve.c2));
    }
    let before = doc.workbook().clone();
    doc.edit_sheet("Reset", |s| s.set_relationship_control_points(rel.id, None))
        .unwrap();
    assert_eq!(
        layout_sheet(doc.sheet(), &ApproxMeasure, false).relationships[0],
        automatic
    );
    assert!(doc.undo());
    assert_eq!(doc.workbook(), &before);
}

#[test]
fn invalid_controls_fail_without_mutation_and_legacy_fields_default() {
    let mut doc = document();
    let id = doc.sheet().relationships[0].id;
    let before = doc.workbook().clone();
    assert!(Relationship::valid_control_points(Some([
        Vec2::new(-1_000_000.0, 1_000_000.0),
        Vec2::ZERO
    ])));
    for bad in [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        1_000_001.0,
        -1_000_001.0,
    ] {
        let points = Some([Vec2::new(bad, 1.0), Vec2::ZERO]);
        assert!(!Relationship::valid_control_points(points));
        assert!(
            doc.edit_sheet("Invalid curve", |s| s
                .set_relationship_control_points(id, points))
                .is_err()
        );
        assert_eq!(doc.workbook(), &before);
        let mut corrupt = doc.sheet().clone();
        corrupt.relationships[0].control_points = points;
        assert!(corrupt.validate().is_err());
    }
    let rel: Relationship =
        serde_json::from_str(r#"{"id":1,"from":1,"to":2,"title":"Legacy"}"#).unwrap();
    assert_eq!(rel.control_points, None);
    let json = serde_json::to_string(doc.workbook()).unwrap();
    assert!(!json.contains("control_points"));
}
