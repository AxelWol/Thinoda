use crate::topic_drag::{DropTarget, apply, members, target};
use thinoda_core::{ApproxMeasure, Document, Sheet, Structure, Vec2, Workbook, layout_sheet};

#[test]
fn insertion_edges_follow_connector_axis_and_document_order_for_all_structures() {
    for structure in Structure::ALL {
        let mut sheet = Sheet::new("Slots", "Center");
        sheet.structure = structure;
        let root = sheet.root;
        let children: Vec<_> = (0..8)
            .map(|i| sheet.insert_child(root, &format!("Topic {i}")).unwrap())
            .collect();
        let floating = sheet.add_floating("Moving", Vec2::new(-1000.0, -1000.0));
        let layout = layout_sheet(&sheet, &ApproxMeasure, false);
        let moving = members(&sheet, &[floating]);
        for (index, id) in children.iter().enumerate() {
            let rect = layout.topics[id].rect;
            let horizontal = layout
                .connectors
                .iter()
                .find(|c| c.child == *id)
                .is_some_and(|c| c.vertical);
            let mut low = rect.center();
            let mut high = low;
            if horizontal {
                low.x = rect.min.x + 0.5;
                high.x = rect.max().x - 0.5;
            } else {
                low.y = rect.min.y + 0.5;
                high.y = rect.max().y - 0.5;
            }
            let a = target(&sheet, &layout, &moving, low).unwrap().unwrap();
            let b = target(&sheet, &layout, &moving, high).unwrap().unwrap();
            assert_eq!(a.parent, root, "{structure:?}");
            assert_eq!(b.parent, root, "{structure:?}");
            assert_eq!(
                [a.index.min(b.index), a.index.max(b.index)],
                [index, index + 1],
                "{structure:?}"
            );
            assert!(a.slot.is_some() && b.slot.is_some());
            for proposal in [a, b] {
                let mut edited = sheet.clone();
                apply(&mut edited, &[floating], &proposal).unwrap();
                assert_eq!(
                    edited.children(root)[proposal.index],
                    floating,
                    "{structure:?}"
                );
                edited.validate().unwrap();
            }
            let center = target(&sheet, &layout, &moving, rect.center())
                .unwrap()
                .unwrap();
            assert_eq!(center.parent, *id);
            assert!(center.slot.is_none());
        }
    }
}

#[test]
fn multi_topic_insertion_preserves_requested_order_and_one_step_undo() {
    for moving_indices in [vec![0, 1], vec![3, 4], vec![4, 0], vec![1, 3]] {
        for index in 0..=5 {
            let mut sheet = Sheet::new("Move", "Center");
            let root = sheet.root;
            let children: Vec<_> = (0..5)
                .map(|i| sheet.insert_child(root, &i.to_string()).unwrap())
                .collect();
            let moving: Vec<_> = moving_indices.iter().map(|i| children[*i]).collect();
            let retained_before = children[..index]
                .iter()
                .filter(|id| !moving.contains(id))
                .count();
            let mut expected: Vec<_> = children
                .iter()
                .copied()
                .filter(|id| !moving.contains(id))
                .collect();
            expected.splice(retained_before..retained_before, moving.iter().copied());
            let mut doc = Document::new(Workbook {
                sheets: vec![sheet],
                active_sheet: 0,
                metadata: Default::default(),
            });
            let before = doc.workbook().clone();
            let proposal = DropTarget {
                parent: root,
                index,
                slot: None,
                side: None,
            };
            doc.edit_sheet("Insert topics", |s| apply(s, &moving, &proposal))
                .unwrap();
            assert_eq!(
                doc.sheet().children(root),
                expected,
                "moving={moving_indices:?}, index={index}"
            );
            doc.sheet().validate().unwrap();
            assert_eq!(doc.undo(), expected != children);
            assert_eq!(doc.workbook(), &before);
        }
    }
}

#[test]
fn reversed_split_map_side_has_correct_before_and_after_slots() {
    let mut sheet = Sheet::new("Split", "Center");
    sheet.theme.balanced = false;
    let root = sheet.root;
    let children: Vec<_> = (0..8)
        .map(|i| sheet.insert_child(root, &i.to_string()).unwrap())
        .collect();
    let floating = sheet.add_floating("Moving", Vec2::new(-1000.0, -1000.0));
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let moving = members(&sheet, &[floating]);
    for pair in children.windows(2) {
        let a = &layout.topics[&pair[0]].rect;
        let b = &layout.topics[&pair[1]].rect;
        if (a.center().x > 0.0) != (b.center().x > 0.0) {
            continue;
        }

        let reversed = b.center().y < a.center().y;
        let edge = Vec2::new(
            a.center().x,
            if reversed {
                a.min.y + 0.5
            } else {
                a.max().y - 0.5
            },
        );
        let proposal = target(&sheet, &layout, &moving, edge).unwrap().unwrap();
        assert_eq!(proposal.index, sheet.index_in_parent(pair[1]).unwrap());
    }
}

#[test]
fn dropping_into_an_unchanged_slot_preserves_summary_ownership_and_history() {
    let mut sheet = Sheet::new("No-op", "Center");
    let root = sheet.root;
    let a = sheet.insert_child(root, "A").unwrap();
    sheet.insert_child(root, "B").unwrap();
    sheet.add_summary(&[a], "Keep summary").unwrap();
    sheet.add_boundary(&[a]).unwrap();
    let mut doc = Document::new(Workbook {
        sheets: vec![sheet],
        active_sheet: 0,
        metadata: Default::default(),
    });
    let before = doc.workbook().clone();
    let state = doc.state_id();
    for index in [0, 1] {
        let proposal = DropTarget {
            parent: root,
            index,
            side: None,
            slot: None,
        };
        doc.edit_sheet("No-op drop", |s| apply(s, &[a], &proposal))
            .unwrap();
        assert_eq!(doc.workbook(), &before);
        assert_eq!(doc.state_id(), state);
        assert!(!doc.undo());
    }
}
