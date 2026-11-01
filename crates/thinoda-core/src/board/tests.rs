use super::*;
use crate::Document;
use crate::model::{Relationship, SlideId};

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::from_min_size(Vec2::new(x, y), Vec2::new(w, h))
}

fn stroke(points: &[(f32, f32)]) -> InkStroke {
    InkStroke::new(
        InkTool::Pen,
        Color::rgb(10, 20, 30),
        4.0,
        points.iter().map(|(x, y)| Vec2::new(*x, *y)).collect(),
    )
}

/// Hidden-mode sheet with two floating roots and one card.
fn board() -> (Sheet, TopicId, TopicId, TopicId) {
    let mut s = Sheet::new("Board", "");
    s.root_mode = SheetRootMode::Hidden;
    let a = s.add_floating("A", Vec2::new(300.0, 200.0));
    let b = s.add_floating("B", Vec2::new(-200.0, 50.0));
    let card = s
        .add_card(
            "Note",
            Vec2::new(0.0, 400.0),
            Card::new(CardKind::Sticky, Vec2::new(160.0, 120.0)),
        )
        .unwrap();
    s.validate().unwrap();
    (s, a, b, card)
}

// ---- serde ------------------------------------------------------------------------------

#[test]
fn legacy_sheet_json_round_trips_byte_identically() {
    let mut s = Sheet::new("Map", "Central");
    let a = s.add_child(s.root, None, "A");
    let f = s.add_floating("F", Vec2::new(10.0, 20.0));
    s.create_zone("Zone", vec![f], rect(0.0, 0.0, 100.0, 100.0))
        .unwrap();
    s.add_presentation_slide(
        "Slide",
        PresentationSource::Branch(a),
        rect(0.0, 0.0, 100.0, 80.0),
    )
    .unwrap();
    let json = serde_json::to_string(&s).unwrap();
    for key in [
        "root_mode",
        "canvas",
        "\"board\"",
        "\"card\"",
        "\"clip\"",
        "\"color\":null",
    ] {
        assert!(
            !json.contains(key),
            "{key} leaked into legacy output: {json}"
        );
    }
    let back: Sheet = serde_json::from_str(&json).unwrap();
    assert_eq!(back, s);
    assert_eq!(serde_json::to_string(&back).unwrap(), json);
}

#[test]
fn old_json_without_board_fields_loads_with_defaults() {
    let mut value = serde_json::to_value(Sheet::new("Old", "Root")).unwrap();
    let object = value.as_object_mut().unwrap();
    object.remove("root_mode");
    object.remove("canvas");
    object.remove("board");
    let s: Sheet = serde_json::from_value(value).unwrap();
    assert_eq!(s.root_mode, SheetRootMode::Central);
    assert!(s.canvas.is_empty());
    assert!(s.board.is_default());
    assert!(s.root_topic().card.is_none());
    s.validate().unwrap();
}

#[test]
fn board_sheet_round_trips() {
    let (mut s, a, _, card) = board();
    s.board = BoardSettings {
        grid: GridKind::Dots,
        grid_size: 32.0,
        snap: true,
    };
    s.zones.clear();
    let zone = s
        .create_zone("Frame", vec![a], rect(200.0, 100.0, 300.0, 300.0))
        .unwrap();
    s.zones[0].color = Some(Color::rgb(200, 220, 255));
    s.zones[0].clip = true;
    let shape = s
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Diamond,
            rect(0.0, 0.0, 40.0, 30.0),
        ))
        .unwrap();
    let mut ink = stroke(&[(0.0, 0.0), (10.0, 5.0), (20.0, 0.0)]);
    ink.pressure = Some(vec![0.2, 0.5, 1.0].into());
    let ink = s.add_canvas_object(CanvasObject::ink(ink)).unwrap();
    let conn = s
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(shape),
            Endpoint::Topic(card),
        )))
        .unwrap();
    s.group_canvas_objects(&[shape, ink]).unwrap();
    s.rotate_canvas_object(shape, 30.0).unwrap();
    s.add_presentation_slide(
        "View",
        PresentationSource::Viewport,
        rect(0.0, 0.0, 400.0, 300.0),
    )
    .unwrap();
    s.validate().unwrap();
    let json = serde_json::to_string(&s).unwrap();
    assert!(json.contains("\"root_mode\":\"hidden\""));
    assert!(json.contains("\"viewport\""));
    let back: Sheet = serde_json::from_str(&json).unwrap();
    assert_eq!(back, s);
    assert_eq!(back.canvas.object(conn).unwrap().label(), "");
    assert_eq!(back.zones[0].id, zone);
    back.validate().unwrap();
}

#[test]
fn every_shape_kind_round_trips_and_has_geometry() {
    let mut ids = HashSet::new();
    for kind in ShapeKind::ALL {
        assert!(ids.insert(kind.id()), "duplicate id {}", kind.id());
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(serde_json::from_str::<ShapeKind>(&json).unwrap(), kind);
        let outline = kind.unit_outline();
        assert!(outline.len() >= 3, "{kind:?}");
        for p in outline.iter().chain(kind.unit_details().iter().flatten()) {
            assert!(
                p.x.is_finite() && p.y.is_finite() && (-0.01..=1.01).contains(&p.x),
                "{kind:?} {p:?}"
            );
            assert!((-0.01..=1.01).contains(&p.y), "{kind:?} {p:?}");
        }
        let label = kind.unit_label_rect();
        assert!(label.size.x > 0.0 && label.size.y > 0.0, "{kind:?}");
        let frame = rect(100.0, 50.0, 80.0, 40.0);
        let g = shape_geometry(kind, frame, 0.0);
        let bounds = points_bounds(&g.outline).unwrap();
        assert!(frame.expand(0.5).contains(bounds.min), "{kind:?}");
        assert!(frame.expand(0.5).contains(bounds.max()), "{kind:?}");
        let rotated = shape_geometry(kind, frame, 90.0);
        let rb = points_bounds(&rotated.outline).unwrap();
        assert!(
            rotated_bounds(frame, 90.0).expand(0.5).contains(rb.min),
            "{kind:?}"
        );
    }
}

// ---- ids --------------------------------------------------------------------------------

#[test]
fn allocator_is_unique_across_topics_objects_groups_and_slides() {
    let mut s = Sheet::new("Ids", "Root");
    let a = s.add_child(s.root, None, "A");
    let x = s
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            rect(0.0, 0.0, 10.0, 10.0),
        ))
        .unwrap();
    let y = s
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Ellipse,
            rect(20.0, 0.0, 10.0, 10.0),
        ))
        .unwrap();
    let g = s.group_canvas_objects(&[x, y]).unwrap();
    let t = s.add_floating("F", Vec2::ZERO);
    let mut all = vec![s.root.0, a.0, x.0, y.0, g.0, t.0];
    all.sort_unstable();
    all.dedup();
    assert_eq!(all.len(), 6);

    // Loaded data with high ids: the next allocation must exceed every one of them.
    s.presentation.slides.clear();
    s.add_presentation_slide(
        "S",
        PresentationSource::Branch(a),
        rect(0.0, 0.0, 10.0, 10.0),
    )
    .unwrap();
    s.presentation.slides[0].id = SlideId(5_000);
    s.canvas.objects[0].id = CanvasObjectId(4_000);
    s.canvas.groups[0].id = CanvasGroupId(4_500);
    for o in &mut s.canvas.objects {
        o.group = Some(CanvasGroupId(4_500));
    }
    let json = serde_json::to_string(&s).unwrap();
    let mut loaded: Sheet = serde_json::from_str(&json).unwrap();
    assert!(loaded.alloc_canvas_object_id().0 > 5_000);
    assert!(loaded.alloc_topic_id().0 > 5_000);
    let mut loaded: Sheet = serde_json::from_str(&json).unwrap();
    loaded.presentation.slides.clear();
    assert!(loaded.alloc_canvas_group_id().0 > 4_500);
}

// ---- root semantics --------------------------------------------------------------------

#[test]
fn root_queries_follow_root_mode() {
    let mut s = Sheet::new("Q", "Root");
    let a = s.add_child(s.root, None, "A");
    let f = s.add_floating("F", Vec2::new(0.0, 300.0));
    assert_eq!(s.semantic_roots(), vec![s.root]);
    assert_eq!(s.primary_content_root(), Some(s.root));
    assert_eq!(s.all_content_roots(), vec![s.root, f]);
    assert!(!s.is_anchor(s.root));
    assert!(s.visible_topic_ids().contains(&s.root));

    s.detach_subtree(a).unwrap();
    s.topic_mut(s.root).unwrap().title.clear();
    s.root_mode = SheetRootMode::Hidden;
    s.validate().unwrap();
    let late = s.add_floating("Top", Vec2::new(500.0, -100.0));
    let zoned = s.add_floating("Zoned", Vec2::new(0.0, 900.0));
    s.create_zone("Z", vec![zoned], rect(-100.0, 800.0, 300.0, 300.0))
        .unwrap();
    assert!(s.is_anchor(s.root));
    // Zone members first, then the rest by y then x.
    assert_eq!(s.semantic_roots(), vec![zoned, late, f]);
    assert_eq!(s.content_top_level_ids(), vec![zoned, late, f]);
    assert_eq!(s.primary_content_root(), Some(zoned));
    assert!(!s.all_content_roots().contains(&s.root));
    assert!(!s.visible_topic_ids().contains(&s.root));

    let empty = {
        let mut e = Sheet::new("E", "");
        e.root_mode = SheetRootMode::Hidden;
        e
    };
    assert_eq!(empty.primary_content_root(), None);
    assert!(empty.semantic_roots().is_empty());
}

#[test]
fn hidden_anchor_cannot_carry_non_title_content() {
    let make = || {
        let mut s = Sheet::new("V", "");
        s.root_mode = SheetRootMode::Hidden;
        s
    };
    let mut s = make();
    s.add_child(s.root, None, "Child");
    assert!(s.validate().unwrap_err().contains("children"));

    let mut s = make();
    s.topic_mut(s.root).unwrap().title = "Legacy title".into();
    s.topic_mut(s.root).unwrap().labels.push("x".into());
    assert!(s.validate().unwrap_err().contains("labels"));

    let mut s = make();
    s.topic_mut(s.root).unwrap().hyperlink = Some("https://example.com".into());
    assert!(s.validate().unwrap_err().contains("hyperlink"));

    let mut s = make();
    let f = s.add_floating("F", Vec2::ZERO);
    let id = s.alloc_relationship_id();
    s.relationships.push(Relationship {
        id,
        from: s.root,
        to: f,
        title: String::new(),
        color: None,
        control_points: None,
    });
    assert!(s.validate().unwrap_err().contains("relationships"));

    let mut s = make();
    s.presentation.slides.push(crate::model::PresentationSlide {
        id: SlideId(1),
        title: "S".into(),
        source: PresentationSource::Branch(s.root),
        frame: rect(0.0, 0.0, 10.0, 10.0),
        scope: crate::model::PresentationScope::Source,
        auto_frame: true,
        hidden_topics: Vec::new(),
        max_depth: None,
    });
    assert!(s.validate().is_err());

    // Connectors may not attach to the anchor.
    let mut s = make();
    s.canvas.objects.push(CanvasObject {
        id: CanvasObjectId(99),
        ..CanvasObject::connector(CanvasConnector::new(
            Endpoint::Topic(s.root),
            Endpoint::Point(Vec2::ZERO),
        ))
    });
    assert!(s.validate().unwrap_err().contains("connectors"));
}

#[test]
fn legacy_hidden_anchor_title_is_preserved_but_not_editable_or_content() {
    let mut sheet = Sheet::new("Legacy board", "Legacy title");
    sheet.root_mode = SheetRootMode::Hidden;
    let anchor = sheet.root;
    let content = sheet.add_floating("Visible", Vec2::ZERO);
    sheet.validate().unwrap();
    assert_eq!(sheet.content_ids_ordered(), vec![content]);
    assert!(crate::clipboard::copy_topics(&sheet, &[anchor]).is_none());
    assert_eq!(
        sheet.set_title(anchor, "Changed"),
        Err(crate::EditError::RootNotAllowed)
    );
    assert_eq!(
        sheet.insert_child(anchor, "Child"),
        Err(crate::EditError::RootNotAllowed)
    );
    let json = serde_json::to_string(&sheet).unwrap();
    let mut restored: Sheet = serde_json::from_str(&json).unwrap();
    restored.validate().unwrap();
    assert_eq!(restored.root_topic().title, "Legacy title");
    restored.show_central_topic();
    assert_eq!(restored.root_topic().title, "Legacy title");
}

#[test]
fn hide_central_topic_moves_content_in_one_undo_step() {
    let mut s = Sheet::new("Plan", "Central");
    let a = s.add_child(s.root, None, "A");
    s.add_child(a, None, "A1");
    let b = s.add_child(s.root, None, "B");
    s.topic_mut(s.root).unwrap().labels.push("tag".into());
    s.add_presentation_slide(
        "A",
        PresentationSource::Branch(a),
        rect(0.0, 0.0, 100.0, 80.0),
    )
    .unwrap();
    let conn = s
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Topic(s.root),
            Endpoint::Point(Vec2::new(500.0, 0.0)),
        )))
        .unwrap();
    let root = s.root;
    let mut doc = Document::new(crate::Workbook {
        sheets: vec![s],
        ..Default::default()
    });
    let before = doc.sheet().clone();
    let moved = doc.hide_central_topic().unwrap().expect("content moved");
    let s = doc.sheet();
    assert_eq!(s.root_mode, SheetRootMode::Hidden);
    assert!(s.root_topic().children.is_empty());
    assert!(s.root_topic().title.is_empty());
    assert_eq!(s.topic(moved).unwrap().title, "Central");
    assert_eq!(s.children(moved), &[a, b]);
    assert_eq!(s.topic(moved).unwrap().labels, vec!["tag".to_string()]);
    assert_eq!(s.parent(a), Some(moved));
    assert!(s.floating.contains(&moved));
    assert_eq!(s.semantic_roots(), vec![moved]);
    assert_eq!(
        s.presentation.slides[0].source,
        PresentationSource::Branch(a)
    );
    assert!(s.is_presentation_branch(moved));
    let mut shown = s.clone();
    shown.show_central_topic();
    shown.validate().unwrap();
    shown
        .add_presentation_slide(
            "Moved root",
            PresentationSource::Branch(moved),
            rect(0.0, 0.0, 100.0, 80.0),
        )
        .unwrap();
    match &s.canvas.object(conn).unwrap().kind {
        CanvasObjectKind::Connector(c) => assert_eq!(c.from, Endpoint::Topic(moved)),
        _ => unreachable!(),
    }
    s.validate().unwrap();
    // The moved tree is laid out where the central topic was.
    let layout = crate::layout::layout_sheet(s, &crate::layout::ApproxMeasure, false);
    assert!(layout.rect(root).is_none());
    assert!(layout.rect(moved).unwrap().contains(Vec2::ZERO));

    assert!(doc.undo());
    assert_eq!(doc.sheet(), &before);
    assert!(!doc.can_undo());

    doc.redo();
    doc.show_central_topic().unwrap();
    assert_eq!(doc.sheet().root_mode, SheetRootMode::Central);
    doc.sheet().validate().unwrap();
}

#[test]
fn hide_empty_central_topic_only_switches_mode() {
    let mut s = Sheet::new("Empty", "");
    let f = s.add_floating("F", Vec2::ZERO);
    assert_eq!(s.central_topic_content(), None);
    assert_eq!(s.hide_central_topic().unwrap(), None);
    assert_eq!(s.root_mode, SheetRootMode::Hidden);
    assert_eq!(s.semantic_roots(), vec![f]);
    assert_eq!(s.hide_central_topic().unwrap(), None);
    s.validate().unwrap();
}

#[test]
fn hiding_an_untitled_central_topic_preserves_attached_connectors() {
    let mut sheet = Sheet::new("Board", "");
    let connector = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Topic(sheet.root),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        )))
        .unwrap();
    assert_eq!(sheet.central_topic_content(), Some("connectors"));
    let moved = sheet.hide_central_topic().unwrap().unwrap();
    match &sheet.canvas.object(connector).unwrap().kind {
        CanvasObjectKind::Connector(connector) => {
            assert_eq!(connector.from, Endpoint::Topic(moved));
        }
        _ => unreachable!(),
    }
    sheet.validate().unwrap();
}

#[test]
fn hiding_a_title_only_central_topic_preserves_it_as_a_floating_root() {
    let mut s = Sheet::new("Board", "Important");
    let moved = s.hide_central_topic().unwrap().expect("title preserved");
    assert_eq!(s.topic(moved).unwrap().title, "Important");
    assert!(s.root_topic().title.is_empty());
    assert_eq!(s.semantic_roots(), vec![moved]);
    s.validate().unwrap();
}

#[test]
fn adding_content_to_the_anchor_is_rejected_by_document_edits() {
    let (s, ..) = board();
    let root = s.root;
    let mut doc = Document::new(crate::Workbook {
        sheets: vec![s],
        ..Default::default()
    });
    let before = doc.sheet().clone();
    assert!(
        doc.edit_sheet("Add", |s| {
            s.add_child(root, None, "child");
            Ok(())
        })
        .is_err()
    );
    assert_eq!(doc.sheet(), &before);
    assert!(!doc.can_undo());
}

// ---- cards ------------------------------------------------------------------------------

#[test]
fn cards_only_on_floating_roots() {
    let (mut s, a, _, card) = board();
    let child = s.add_child(a, None, "child");
    let c = Card::new(CardKind::Text, Vec2::new(100.0, 60.0));
    assert!(s.set_topic_card(child, Some(c)).is_err());
    assert!(s.set_topic_card(s.root, Some(c)).is_err());
    assert!(
        s.set_topic_card(a, Some(Card::new(CardKind::Text, Vec2::new(4.0, 60.0))))
            .is_err()
    );
    s.set_topic_card(a, Some(c)).unwrap();
    s.validate().unwrap();
    // Attaching a card below another topic turns it back into a plain topic.
    s.move_topic(card, a, 0).unwrap();
    assert!(s.topic(card).unwrap().card.is_none());
    s.validate().unwrap();
    s.topic_mut(child).unwrap().card = Some(Card::new(CardKind::Sticky, Vec2::new(50.0, 50.0)));
    assert!(s.validate().unwrap_err().contains("not a floating root"));
}

#[test]
fn card_topics_have_fixed_size_and_wrap_text() {
    let mut s = Sheet::new("Cards", "Root");
    let long = "word ".repeat(80);
    let card = s
        .add_card(
            &long,
            Vec2::new(0.0, 500.0),
            Card::new(CardKind::Sticky, Vec2::new(180.0, 140.0)),
        )
        .unwrap();
    let plain = s.add_floating(long.as_str(), Vec2::new(0.0, -500.0));
    let layout = crate::layout::layout_sheet(&s, &crate::layout::ApproxMeasure, false);
    let r = layout.rect(card).unwrap();
    assert_eq!(r.size, Vec2::new(180.0, 140.0));
    assert!((r.center() - Vec2::new(0.0, 500.0)).length() < 1.0);
    let title = layout.topics[&card].title_rect;
    assert!(title.size.x <= 180.0, "{title:?}");
    assert!(layout.rect(plain).unwrap().size.x > 180.0);
}

// ---- limits -----------------------------------------------------------------------------

#[test]
fn limits_reject_out_of_range_objects() {
    let mut s = Sheet::new("Limits", "Root");
    let bad = [
        CanvasObject::shape(ShapeKind::Rectangle, rect(f32::NAN, 0.0, 10.0, 10.0)),
        CanvasObject::shape(ShapeKind::Rectangle, rect(2.0e6, 0.0, 10.0, 10.0)),
        CanvasObject::shape(ShapeKind::Rectangle, rect(0.0, 0.0, -1.0, 10.0)),
        CanvasObject {
            alt_text: "x".repeat(MAX_BOARD_TEXT_CHARS + 1),
            ..CanvasObject::shape(ShapeKind::Rectangle, rect(0.0, 0.0, 10.0, 10.0))
        },
        CanvasObject {
            style: ObjectStyle {
                opacity: 1.5,
                ..Default::default()
            },
            ..CanvasObject::shape(ShapeKind::Rectangle, rect(0.0, 0.0, 10.0, 10.0))
        },
        CanvasObject {
            style: ObjectStyle {
                stroke_width: 500.0,
                ..Default::default()
            },
            ..CanvasObject::shape(ShapeKind::Rectangle, rect(0.0, 0.0, 10.0, 10.0))
        },
        CanvasObject::new(
            CanvasObjectKind::Shape {
                shape: ShapeKind::Rectangle,
                label: "y".repeat(MAX_BOARD_TEXT_CHARS + 1),
            },
            rect(0.0, 0.0, 10.0, 10.0),
        ),
        CanvasObject::ink(stroke(&[])),
        CanvasObject::ink(InkStroke {
            width: 0.01,
            ..stroke(&[(0.0, 0.0), (1.0, 1.0)])
        }),
        CanvasObject::ink(InkStroke::new(
            InkTool::Pen,
            Color::rgb(0, 0, 0),
            2.0,
            vec![Vec2::ZERO; MAX_STROKE_POINTS + 1],
        )),
        CanvasObject::ink(InkStroke {
            pressure: Some(vec![0.5].into()),
            ..stroke(&[(0.0, 0.0), (1.0, 1.0)])
        }),
        CanvasObject::connector(CanvasConnector::new(
            Endpoint::Topic(TopicId(12345)),
            Endpoint::Point(Vec2::ZERO),
        )),
        CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(CanvasObjectId(777)),
            Endpoint::Point(Vec2::ZERO),
        )),
        CanvasObject::connector(CanvasConnector::new(
            Endpoint::Point(Vec2::new(f32::INFINITY, 0.0)),
            Endpoint::Point(Vec2::ZERO),
        )),
    ];
    for object in bad {
        assert!(s.add_canvas_object(object.clone()).is_err(), "{object:?}");
    }
    assert!(s.canvas.is_empty());
    assert!(
        s.set_board_settings(BoardSettings {
            grid_size: 1.0,
            ..Default::default()
        })
        .is_err()
    );
    s.validate().unwrap();
}

#[test]
fn limits_reject_invalid_imported_boards() {
    let mut s = Sheet::new("Import", "Root");
    let id = s
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            rect(0.0, 0.0, 10.0, 10.0),
        ))
        .unwrap();
    s.validate().unwrap();

    let mut dup = s.clone();
    dup.canvas.objects.push(dup.canvas.objects[0].clone());
    assert!(dup.validate().unwrap_err().contains("duplicate"));

    let mut group = s.clone();
    group.canvas.objects[0].group = Some(CanvasGroupId(999));
    assert!(group.validate().unwrap_err().contains("missing group"));

    let mut many = s.clone();
    many.canvas.objects = (0..=MAX_CANVAS_OBJECTS as u64)
        .map(|i| CanvasObject {
            id: CanvasObjectId(10_000 + i),
            ..many.canvas.objects[0].clone()
        })
        .collect();
    assert!(many.validate().unwrap_err().contains("objects"));

    let mut ink = s.clone();
    ink.canvas.objects = (0..=(MAX_TOTAL_INK_POINTS / MAX_STROKE_POINTS) as u64)
        .map(|i| CanvasObject {
            id: CanvasObjectId(10_000 + i),
            ..CanvasObject::ink(InkStroke::new(
                InkTool::Pen,
                Color::rgb(0, 0, 0),
                2.0,
                vec![Vec2::ZERO; MAX_STROKE_POINTS],
            ))
        })
        .collect();
    assert!(ink.validate().unwrap_err().contains("ink exceeds"));

    let mut grid = s.clone();
    grid.board.grid_size = f32::NAN;
    assert!(grid.validate().is_err());

    let mut rot = s.clone();
    rot.canvas.object_mut(id).unwrap().rotation = 400.0;
    assert!(rot.validate().is_err());

    // Paste/import JSON with bad data is rejected by validation, not by a panic.
    let mut json = serde_json::to_value(&s).unwrap();
    json["canvas"]["objects"][0]["rect"]["size"]["x"] = serde_json::json!(-5.0);
    let parsed: Sheet = serde_json::from_value(json).unwrap();
    assert!(parsed.validate().is_err());
}

#[test]
fn total_ink_limit_applies_to_add_and_update() {
    let mut s = Sheet::new("Ink", "Root");
    let full = vec![Vec2::ZERO; MAX_STROKE_POINTS];
    for _ in 0..(MAX_TOTAL_INK_POINTS / MAX_STROKE_POINTS) {
        s.add_canvas_object(CanvasObject::ink(InkStroke::new(
            InkTool::Pen,
            Color::rgb(0, 0, 0),
            2.0,
            full.clone(),
        )))
        .unwrap();
    }
    assert!(
        s.add_canvas_object(CanvasObject::ink(stroke(&[(0.0, 0.0)])))
            .is_err()
    );
    let first = s.canvas.objects[0].id;
    s.update_canvas_object(first, |o| {
        if let CanvasObjectKind::Ink(ink) = &mut o.kind {
            ink.points = vec![Vec2::ZERO; 10].into();
        }
    })
    .unwrap();
    s.add_canvas_object(CanvasObject::ink(stroke(&[(0.0, 0.0)])))
        .unwrap();
    s.validate().unwrap();
}

// ---- object edits -----------------------------------------------------------------------

fn three_shapes() -> (Sheet, [CanvasObjectId; 3]) {
    let mut s = Sheet::new("Z", "Root");
    let ids = [0.0, 50.0, 100.0].map(|x| {
        s.add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            rect(x, 0.0, 40.0, 40.0),
        ))
        .unwrap()
    });
    (s, ids)
}

fn order(s: &Sheet) -> Vec<CanvasObjectId> {
    s.canvas.objects.iter().map(|o| o.id).collect()
}

#[test]
fn z_order_within_bands() {
    let (mut s, [a, b, c]) = three_shapes();
    s.reorder_canvas_objects(&[a], ZOrder::Forward).unwrap();
    assert_eq!(order(&s), vec![b, a, c]);
    s.reorder_canvas_objects(&[a], ZOrder::Front).unwrap();
    assert_eq!(order(&s), vec![b, c, a]);
    s.reorder_canvas_objects(&[a], ZOrder::Forward).unwrap();
    assert_eq!(order(&s), vec![b, c, a]);
    s.reorder_canvas_objects(&[c, a], ZOrder::Back).unwrap();
    assert_eq!(order(&s), vec![c, a, b]);
    s.reorder_canvas_objects(&[b], ZOrder::Backward).unwrap();
    assert_eq!(order(&s), vec![c, b, a]);
    assert!(
        s.reorder_canvas_objects(&[CanvasObjectId(999)], ZOrder::Front)
            .is_err()
    );

    // Moving to the Below band places objects just behind the map; band order is
    // independent of Vec order across bands.
    s.set_canvas_band(&[a], Band::Below).unwrap();
    assert_eq!(s.canvas.object(a).unwrap().band, Band::Below);
    s.set_canvas_band(&[b], Band::Below).unwrap();
    let below: Vec<_> = s
        .canvas
        .objects
        .iter()
        .filter(|o| o.band == Band::Below)
        .map(|o| o.id)
        .collect();
    assert_eq!(below, vec![a, b]);
    s.set_canvas_band(&[a], Band::Above).unwrap();
    let above: Vec<_> = s
        .canvas
        .objects
        .iter()
        .filter(|o| o.band == Band::Above)
        .map(|o| o.id)
        .collect();
    assert_eq!(above, vec![a, c]);
    // Forward within the Below band never crosses into Above.
    s.reorder_canvas_objects(&[b], ZOrder::Forward).unwrap();
    assert_eq!(s.canvas.object(b).unwrap().band, Band::Below);
}

#[test]
fn group_ungroup_and_lock() {
    let (mut s, [a, b, c]) = three_shapes();
    assert!(s.group_canvas_objects(&[a]).is_err());
    let g = s.group_canvas_objects(&[a, b]).unwrap();
    assert_eq!(s.canvas.group_members(g), vec![a, b]);
    let mut expanded = s.canvas.expand_to_groups(&[a]);
    expanded.sort();
    assert_eq!(expanded, vec![a, b]);
    // Regrouping one member with another object leaves a one-member group, which is
    // dissolved.
    let g2 = s.group_canvas_objects(&[b, c]).unwrap();
    assert!(s.canvas.group(g).is_none());
    assert_eq!(s.canvas.object(a).unwrap().group, None);
    s.validate().unwrap();

    s.set_canvas_group_locked(g2, true).unwrap();
    assert!(s.canvas.is_locked(b));
    assert!(s.move_canvas_objects(&[b], Vec2::new(5.0, 0.0)).is_err());
    assert!(s.remove_canvas_objects(&[c]).is_err());
    assert!(s.rotate_canvas_object(c, 10.0).is_err());
    assert!(s.resize_canvas_object(c, rect(0.0, 0.0, 5.0, 5.0)).is_err());
    let locked = s.clone();
    assert!(
        s.update_canvas_object(b, |o| o.alt_text = "edit".into())
            .is_err()
    );
    assert!(s.reorder_canvas_objects(&[b], ZOrder::Front).is_err());
    assert!(s.set_canvas_band(&[b], Band::Below).is_err());
    assert!(s.group_canvas_objects(&[a, b]).is_err());
    assert!(s.ungroup_canvas_objects(g2).is_err());
    assert_eq!(s, locked);
    s.set_canvas_group_locked(g2, false).unwrap();
    s.set_canvas_objects_locked(&[a], true).unwrap();
    assert!(s.move_canvas_objects(&[a], Vec2::new(5.0, 0.0)).is_err());
    s.set_canvas_objects_locked(&[a], false).unwrap();
    s.move_canvas_objects(&[a, b], Vec2::new(5.0, 7.0)).unwrap();
    assert_eq!(s.canvas.object(a).unwrap().rect.min, Vec2::new(5.0, 7.0));

    s.ungroup_canvas_objects(g2).unwrap();
    assert!(s.canvas.groups.is_empty());
    assert!(s.canvas.objects.iter().all(|o| o.group.is_none()));
    assert!(s.ungroup_canvas_objects(g2).is_err());
    s.validate().unwrap();

    let g3 = s.group_canvas_objects(&[a, b]).unwrap();
    s.remove_canvas_objects(&[a]).unwrap();
    assert!(
        s.canvas.group(g3).is_none(),
        "one-member group is dissolved"
    );
    s.validate().unwrap();
}

#[test]
fn move_resize_rotate_objects() {
    let mut s = Sheet::new("Edit", "Root");
    let ink = s
        .add_canvas_object(CanvasObject::ink(stroke(&[(0.0, 0.0), (10.0, 20.0)])))
        .unwrap();
    s.move_canvas_objects(&[ink], Vec2::new(10.0, 10.0))
        .unwrap();
    let o = s.canvas.object(ink).unwrap();
    assert_eq!(o.rect, rect(10.0, 10.0, 10.0, 20.0));
    match &o.kind {
        CanvasObjectKind::Ink(i) => assert_eq!(i.points[1], Vec2::new(20.0, 30.0)),
        _ => unreachable!(),
    }
    s.resize_canvas_object(ink, rect(10.0, 10.0, 20.0, 40.0))
        .unwrap();
    match &s.canvas.object(ink).unwrap().kind {
        CanvasObjectKind::Ink(i) => assert_eq!(i.points[1], Vec2::new(30.0, 50.0)),
        _ => unreachable!(),
    }
    s.rotate_canvas_object(ink, -90.0).unwrap();
    assert_eq!(s.canvas.object(ink).unwrap().rotation, 270.0);
    assert!(s.rotate_canvas_object(ink, f32::NAN).is_err());
    assert!(
        s.move_canvas_objects(&[ink], Vec2::new(5.0e6, 0.0))
            .is_err()
    );
    assert!(
        s.resize_canvas_object(ink, rect(0.0, 0.0, -2.0, 1.0))
            .is_err()
    );

    let conn = s
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Object(ink),
        )))
        .unwrap();
    assert!(s.rotate_canvas_object(conn, 45.0).is_err());
    s.move_canvas_objects(&[conn], Vec2::new(1.0, 2.0)).unwrap();
    match &s.canvas.object(conn).unwrap().kind {
        CanvasObjectKind::Connector(c) => {
            assert_eq!(c.from, Endpoint::Point(Vec2::new(1.0, 2.0)));
            assert_eq!(c.to, Endpoint::Object(ink));
        }
        _ => unreachable!(),
    }
    s.update_canvas_object(conn, |o| {
        o.id = CanvasObjectId(1);
        o.locked = true;
        if let CanvasObjectKind::Connector(c) = &mut o.kind {
            c.label = "flows".into();
            c.route = ConnectorRoute::Elbow;
        }
    })
    .unwrap();
    let o = s.canvas.object(conn).unwrap();
    assert_eq!(o.label(), "flows");
    assert!(!o.locked, "update cannot change lock state");
    assert!(
        s.update_canvas_object(conn, |o| o.style.opacity = 2.0)
            .is_err()
    );
    s.validate().unwrap();
}

// ---- connector endpoint fix-up ------------------------------------------------------------

#[test]
fn deleting_connector_targets_converts_ends_to_points_in_one_edit() {
    let (mut s, a, b, _) = board();
    let shape = s
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            rect(100.0, -300.0, 40.0, 20.0),
        ))
        .unwrap();
    let to_topic = s
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Topic(b),
            Endpoint::Topic(a),
        )))
        .unwrap();
    let to_shape = s
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Object(shape),
            Endpoint::Topic(a),
        )))
        .unwrap();
    let mut doc = Document::new(crate::Workbook {
        sheets: vec![s],
        ..Default::default()
    });
    let b_center = {
        let layout = crate::layout::layout_sheet(doc.sheet(), &crate::layout::ApproxMeasure, false);
        layout.rect(b).unwrap().center()
    };
    let before = doc.sheet().clone();
    doc.edit_sheet("Delete", |s| s.remove_topics(&[b])).unwrap();
    let ends = |doc: &Document, id| match &doc.sheet().canvas.object(id).unwrap().kind {
        CanvasObjectKind::Connector(c) => (c.from, c.to),
        _ => unreachable!(),
    };
    assert_eq!(
        ends(&doc, to_topic),
        (Endpoint::Point(b_center), Endpoint::Topic(a))
    );
    doc.sheet().validate().unwrap();
    doc.undo();
    assert_eq!(doc.sheet(), &before);
    doc.redo();

    doc.remove_canvas_objects(&[shape]).unwrap();
    assert_eq!(
        ends(&doc, to_shape),
        (
            Endpoint::Point(Vec2::new(120.0, -290.0)),
            Endpoint::Topic(a)
        )
    );
    // Deleting a topic through any path (here: a raw detach) is fixed up by the
    // document safety net instead of leaving a dangling reference.
    doc.edit_sheet("Detach", |s| {
        s.detach_subtree(a).unwrap();
        Ok(())
    })
    .unwrap();
    assert!(matches!(ends(&doc, to_shape).1, Endpoint::Point(_)));
    assert!(!doc.sheet().has_dangling_endpoints());
    doc.sheet().validate().unwrap();
}

#[test]
fn moving_a_topic_keeps_connector_endpoints() {
    let (mut s, a, b, _) = board();
    let conn = s
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Topic(b),
            Endpoint::Topic(a),
        )))
        .unwrap();
    let mut doc = Document::new(crate::Workbook {
        sheets: vec![s],
        ..Default::default()
    });
    doc.edit_sheet("Move", |s| s.move_topic(b, a, 0)).unwrap();
    match &doc.sheet().canvas.object(conn).unwrap().kind {
        CanvasObjectKind::Connector(c) => assert_eq!(c.from, Endpoint::Topic(b)),
        _ => unreachable!(),
    }
}

// ---- masking ----------------------------------------------------------------------------

#[test]
fn masked_copy_hides_canvas_text() {
    let mut s = Sheet::new("Mask", "Root");
    let id = s
        .add_canvas_object(CanvasObject::new(
            CanvasObjectKind::Shape {
                shape: ShapeKind::Cloud,
                label: "SECRET label".into(),
            },
            rect(0.0, 0.0, 10.0, 10.0),
        ))
        .unwrap();
    s.update_canvas_object(id, |o| o.alt_text = "SECRET alt".into())
        .unwrap();
    let wb = crate::Workbook {
        sheets: vec![s],
        ..Default::default()
    };
    let json = serde_json::to_string(&wb.masked_copy()).unwrap();
    assert!(!json.contains("SECRET"));
}

// ---- geometry ---------------------------------------------------------------------------

#[test]
fn rotation_helpers() {
    assert_eq!(normalize_rotation(-90.0), 270.0);
    assert_eq!(normalize_rotation(720.0), 0.0);
    let r = rect(0.0, 0.0, 100.0, 20.0);
    let b = rotated_bounds(r, 90.0);
    assert!((b.size.x - 20.0).abs() < 1e-3 && (b.size.y - 100.0).abs() < 1e-3);
    assert!(rotated_rect_contains(r, 90.0, Vec2::new(50.0, 45.0)));
    assert!(!rotated_rect_contains(r, 90.0, Vec2::new(5.0, 10.0)));
    assert!(point_in_polygon(
        Vec2::new(1.0, 1.0),
        &[Vec2::ZERO, Vec2::new(4.0, 0.0), Vec2::new(0.0, 4.0)]
    ));
    assert!(
        (distance_to_segment(Vec2::new(5.0, 3.0), Vec2::ZERO, Vec2::new(10.0, 0.0)) - 3.0).abs()
            < 1e-5
    );
}
