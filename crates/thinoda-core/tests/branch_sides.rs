use thinoda_core::{
    ApproxMeasure, BranchSide, Document, Sheet, Structure, Topic, TopicId, Vec2, Workbook,
    layout_sheet,
};

#[test]
fn manual_sides_override_automatic_balancing_and_preserve_controls() {
    for structure in [
        Structure::MapClockwise,
        Structure::MapAnticlockwise,
        Structure::MapUnbalanced,
    ] {
        for balanced in [false, true] {
            let mut sheet = Sheet::new("Sides", "Center");
            sheet.structure = structure;
            sheet.theme.balanced = balanced;
            let root = sheet.root;
            let children: Vec<_> = (0..8)
                .map(|i| sheet.insert_child(root, &format!("Branch {i}")).unwrap())
                .collect();
            for (i, id) in children.iter().enumerate() {
                sheet
                    .set_branch_side(
                        *id,
                        Some(if i % 2 == 0 {
                            BranchSide::Left
                        } else {
                            BranchSide::Right
                        }),
                    )
                    .unwrap();
                sheet.insert_child(*id, "Child").unwrap();
            }
            let layout = layout_sheet(&sheet, &ApproxMeasure, false);
            for (i, id) in children.iter().enumerate() {
                let x = layout.topics[id].rect.center().x - layout.topics[&root].rect.center().x;
                assert_eq!(x < 0.0, i % 2 == 0, "{structure:?}, balanced={balanced}");
            }
            // Explicit sides must survive sibling insertion and automatic rebalance.
            sheet.insert_child(root, "Additional").unwrap();
            let layout = layout_sheet(&sheet, &ApproxMeasure, false);
            for (i, id) in children.iter().enumerate() {
                assert_eq!(
                    layout.topics[id].rect.center().x < layout.topics[&root].rect.center().x,
                    i % 2 == 0
                );
            }
            sheet.validate().unwrap();
        }
    }
}

#[test]
fn side_reset_undo_and_legacy_serialization_are_compatible() {
    let legacy_theme: thinoda_core::theme::ThemeSettings = serde_json::from_str("{}").unwrap();
    assert!(!legacy_theme.flexible_floating_topics);
    let mut sheet = Sheet::new("Sides", "Center");
    let root = sheet.root;
    let child = sheet.insert_child(root, "Main").unwrap();
    let mut doc = Document::new(Workbook {
        sheets: vec![sheet],
        active_sheet: 0,
        metadata: Default::default(),
    });
    let before = doc.workbook().clone();
    assert!(
        !serde_json::to_string(&before)
            .unwrap()
            .contains("branch_side")
    );
    doc.edit_sheet("Left branch", |s| {
        s.set_branch_side(child, Some(BranchSide::Left))
    })
    .unwrap();
    let left = doc.workbook().clone();
    assert_eq!(
        serde_json::from_str::<Workbook>(&serde_json::to_string(&left).unwrap()).unwrap(),
        left
    );
    assert!(doc.undo());
    assert_eq!(doc.workbook(), &before);
    assert!(doc.redo());
    assert_eq!(doc.workbook(), &left);
    doc.edit_sheet("Automatic branch", |s| s.set_branch_side(child, None))
        .unwrap();
    assert_eq!(doc.workbook(), &before);
    assert!(
        doc.edit_sheet("Root side", |s| s
            .set_branch_side(root, Some(BranchSide::Right)))
            .is_err()
    );
    assert!(
        doc.edit_sheet("Missing side", |s| s
            .set_branch_side(TopicId(u64::MAX), None))
            .is_err()
    );
    assert_eq!(doc.workbook(), &before);
    let legacy: Topic = serde_json::from_str(r#"{"id":1,"title":"Old topic"}"#).unwrap();
    assert_eq!(legacy.branch_side, None);
}

#[test]
fn sides_do_not_rewrite_non_split_structures_or_floating_positions() {
    let mut sheet = Sheet::new("Sides", "Center");
    let root = sheet.root;
    let child = sheet.insert_child(root, "Main").unwrap();
    let floating = sheet.add_floating("Floating", Vec2::new(800.0, 800.0));
    for structure in Structure::ALL {
        if matches!(
            structure,
            Structure::MapClockwise | Structure::MapAnticlockwise | Structure::MapUnbalanced
        ) {
            continue;
        }
        sheet.structure = structure;
        sheet.set_branch_side(child, None).unwrap();
        let before = layout_sheet(&sheet, &ApproxMeasure, false);
        sheet
            .set_branch_side(child, Some(BranchSide::Left))
            .unwrap();
        assert_eq!(
            layout_sheet(&sheet, &ApproxMeasure, false),
            before,
            "{structure:?}"
        );
        assert_eq!(
            sheet.topic(floating).unwrap().position,
            Some(Vec2::new(800.0, 800.0))
        );
    }
}
