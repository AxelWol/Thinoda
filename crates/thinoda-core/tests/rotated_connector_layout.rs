//! Connector anchors for rotated floating cards, independent of any renderer.

use thinoda_core::board::rotate_around;
use thinoda_core::layout::{clip_to_rect, relationship_curve, relationship_curve_for_sheet};
use thinoda_core::scene::topic_clip_zone;
use thinoda_core::{
    ApproxMeasure, CanvasConnector, Card, CardKind, ConnectorRoute, Endpoint, LineShape, Rect,
    SceneId, Sheet, Structure, TopicId, TopicShape, Vec2, build_scene, connector_path,
    layout_sheet,
};

const TOLERANCE: f32 = 0.01;

fn card(sheet: &mut Sheet, center: Vec2, size: Vec2, rotation: f32) -> TopicId {
    let mut presentation = Card::new(CardKind::Sticky, size);
    presentation.rotation = rotation;
    let id = sheet.add_card("Card", center, presentation).unwrap();
    assert_ne!(id, sheet.root);
    assert!(sheet.floating.contains(&id));
    assert_eq!(sheet.topic(id).unwrap().parent, None);
    id
}

fn assert_on_rotated_border(rect: Rect, rotation: f32, point: Vec2) {
    assert_on_rotated_border_with_tolerance(rect, rotation, point, TOLERANCE);
}

fn assert_on_rotated_border_with_tolerance(rect: Rect, rotation: f32, point: Vec2, tolerance: f32) {
    let local = rotate_around(point, rect.center(), -rotation) - rect.center();
    let half = rect.size * 0.5;
    assert!(
        local.x.abs() <= half.x + tolerance && local.y.abs() <= half.y + tolerance,
        "{point:?} is outside {rect:?} rotated {rotation} degrees (local {local:?})"
    );
    assert!(
        (local.x.abs() - half.x).abs() <= tolerance || (local.y.abs() - half.y).abs() <= tolerance,
        "{point:?} is not on {rect:?} rotated {rotation} degrees (local {local:?})"
    );
}

fn assert_exits_card(rect: Rect, rotation: f32, point: Vec2, direction: Vec2) {
    // Branch paths initially travel along this sheet axis. They must leave the
    // card rather than start on its outline and immediately cross its interior.
    let local = rotate_around(point + direction, rect.center(), -rotation) - rect.center();
    let half = rect.size * 0.5;
    assert!(
        local.x.abs() >= half.x - TOLERANCE || local.y.abs() >= half.y - TOLERANCE,
        "branch from {point:?} in direction {direction:?} enters rotated {rect:?}"
    );
}

fn assert_ray(center: Vec2, toward: Vec2, point: Vec2) {
    let direction = toward - center;
    let offset = point - center;
    let cross = direction.x * offset.y - direction.y * offset.x;
    let scale = (direction.length() * offset.length()).max(1.0);
    assert!(cross.abs() <= scale * 0.0001, "{point:?} is off the ray");
    assert!(direction.x * offset.x + direction.y * offset.y >= 0.0);
}

#[test]
fn ordinary_relationships_follow_both_rotated_card_outlines() {
    for (from_angle, to_angle, target) in [
        (45.0, 90.0, Vec2::new(500.0, 200.0)),
        (135.0, 270.0, Vec2::new(-500.0, 50.0)),
        (90.0, 30.0, Vec2::new(0.0, -500.0)),
        (270.0, 315.0, Vec2::new(0.0, 500.0)),
    ] {
        let mut sheet = Sheet::new("Board", "Root");
        let from = card(&mut sheet, Vec2::ZERO, Vec2::new(160.0, 60.0), from_angle);
        let to = card(&mut sheet, target, Vec2::new(80.0, 140.0), to_angle);
        let id = sheet.add_relationship(from, to).unwrap();
        let layout = layout_sheet(&sheet, &ApproxMeasure, false);
        let curve = layout.relationships.iter().find(|r| r.id == id).unwrap();
        assert_on_rotated_border(layout.rect(from).unwrap(), from_angle, curve.start);
        assert_on_rotated_border(layout.rect(to).unwrap(), to_angle, curve.end);
        assert_ray(layout.rect(from).unwrap().center(), curve.c1, curve.start);
        assert_ray(layout.rect(to).unwrap().center(), curve.c2, curve.end);

        // Explicit control offsets are in sheet axes, not card-local axes.
        let offsets = [Vec2::new(60.0, -100.0), Vec2::new(-90.0, 120.0)];
        sheet
            .set_relationship_control_points(id, Some(offsets))
            .unwrap();
        let layout = layout_sheet(&sheet, &ApproxMeasure, false);
        let curve = layout.relationships.iter().find(|r| r.id == id).unwrap();
        assert_eq!(curve.c1, layout.rect(from).unwrap().center() + offsets[0]);
        assert_eq!(curve.c2, layout.rect(to).unwrap().center() + offsets[1]);
        assert_on_rotated_border(layout.rect(from).unwrap(), from_angle, curve.start);
        assert_on_rotated_border(layout.rect(to).unwrap(), to_angle, curve.end);
        assert_ray(layout.rect(from).unwrap().center(), curve.c1, curve.start);
        assert_ray(layout.rect(to).unwrap().center(), curve.c2, curve.end);
    }
}

#[test]
fn rotated_card_branch_roots_attach_in_every_structure() {
    for structure in Structure::ALL {
        for angle in [15.0, 45.0, 90.0, 135.0, 180.0, 270.0, 359.999] {
            let mut sheet = Sheet::new("Board", "Root");
            let root = card(
                &mut sheet,
                Vec2::new(900.0, 450.0),
                Vec2::new(120.0, 60.0),
                0.0,
            );
            sheet.topic_mut(root).unwrap().structure = Some(structure);
            let children: Vec<_> = (0..4)
                .map(|_| sheet.add_child(root, None, "Child"))
                .collect();
            let before = layout_sheet(&sheet, &ApproxMeasure, false);
            sheet
                .topic_mut(root)
                .unwrap()
                .card
                .as_mut()
                .unwrap()
                .rotation = angle;
            sheet.validate().unwrap();
            let after = layout_sheet(&sheet, &ApproxMeasure, false);
            let rect = after.rect(root).unwrap();
            let fishbone = matches!(
                structure,
                Structure::FishboneRight | Structure::FishboneLeft
            );
            let timeline = matches!(
                structure,
                Structure::TimelineHorizontal | Structure::TimelineVertical
            );

            assert_eq!(before.order, after.order);
            for id in &before.order {
                assert_eq!(
                    before.rect(*id),
                    after.rect(*id),
                    "{structure:?} {angle}: rotation should not reflow the tree"
                );
            }
            assert_eq!(before.connectors.len(), after.connectors.len());
            for (original, changed) in before.connectors.iter().zip(&after.connectors) {
                assert_eq!(changed.end, original.end, "{structure:?} {angle}");
                let starts_at_card = changed.parent == root
                    && if fishbone {
                        changed.child == root // Other fishbone segments start on the spine.
                    } else if timeline {
                        changed.child == children[0] // Later segments start at preceding siblings.
                    } else {
                        children.contains(&changed.child)
                    };
                if !starts_at_card {
                    assert_eq!(changed.start, original.start, "{structure:?} {angle}");
                    continue;
                }
                assert_on_rotated_border(rect, angle, changed.start);
                let direction = match structure {
                    Structure::TreeLeft
                    | Structure::TreeRight
                    | Structure::TimelineVertical
                    | Structure::OrgDown => Vec2::new(0.0, 1.0),
                    Structure::OrgUp => Vec2::new(0.0, -1.0),
                    Structure::FishboneRight | Structure::LogicLeft | Structure::BraceLeft => {
                        Vec2::new(-1.0, 0.0)
                    }
                    Structure::MapClockwise
                    | Structure::MapAnticlockwise
                    | Structure::MapUnbalanced => {
                        let x = after.rect(changed.child).unwrap().center().x - rect.center().x;
                        Vec2::new(x.signum(), 0.0)
                    }
                    _ => Vec2::new(1.0, 0.0),
                };
                assert_exits_card(rect, angle, changed.start, direction);
            }
        }
    }
}

#[test]
fn tree_branch_anchor_handles_an_indent_wider_than_the_rotated_card() {
    for structure in [Structure::TreeRight, Structure::TreeLeft] {
        let mut sheet = Sheet::new("Board", "Root");
        sheet.theme.spacing = 4.0;
        let root = card(
            &mut sheet,
            Vec2::new(100.0, 200.0),
            Vec2::new(16.0, 20.0),
            90.0,
        );
        sheet.topic_mut(root).unwrap().structure = Some(structure);
        let child = sheet.add_child(root, None, "Child");
        let layout = layout_sheet(&sheet, &ApproxMeasure, false);
        let branch = layout
            .connectors
            .iter()
            .find(|c| c.parent == root && c.child == child)
            .unwrap();
        let rect = layout.rect(root).unwrap();
        assert_on_rotated_border(rect, 90.0, branch.start);
        assert_exits_card(rect, 90.0, branch.start, Vec2::new(0.0, 1.0));
    }
}

#[test]
fn tree_branch_keeps_its_sheet_axis_offset_when_it_intersects_the_card() {
    for structure in [Structure::TreeRight, Structure::TreeLeft] {
        let mut sheet = Sheet::new("Board", "Root");
        let root = card(
            &mut sheet,
            Vec2::new(100.0, 200.0),
            Vec2::new(120.0, 60.0),
            0.0,
        );
        sheet.topic_mut(root).unwrap().structure = Some(structure);
        let child = sheet.add_child(root, None, "Child");
        let before = layout_sheet(&sheet, &ApproxMeasure, false);
        sheet
            .topic_mut(root)
            .unwrap()
            .card
            .as_mut()
            .unwrap()
            .rotation = 15.0;
        let after = layout_sheet(&sheet, &ApproxMeasure, false);
        let branch = |layout: &thinoda_core::Layout| {
            layout
                .connectors
                .iter()
                .find(|c| c.parent == root && c.child == child)
                .unwrap()
                .start
        };
        assert_eq!(branch(&before).x, branch(&after).x);
        assert_on_rotated_border(after.rect(root).unwrap(), 15.0, branch(&after));
        assert_exits_card(
            after.rect(root).unwrap(),
            15.0,
            branch(&after),
            Vec2::new(0.0, 1.0),
        );
    }
}

#[test]
fn branch_line_styles_and_underline_offsets_exit_rotated_cards() {
    for (structure, direction) in [
        (Structure::LogicRight, Vec2::new(1.0, 0.0)),
        (Structure::LogicLeft, Vec2::new(-1.0, 0.0)),
        (Structure::OrgDown, Vec2::new(0.0, 1.0)),
        (Structure::OrgUp, Vec2::new(0.0, -1.0)),
    ] {
        for line in LineShape::ALL {
            for angle in [15.0, 45.0, 135.0, 270.0] {
                let mut sheet = Sheet::new("Board", "Root");
                let root = card(
                    &mut sheet,
                    Vec2::new(100.0, 200.0),
                    Vec2::new(130.0, 60.0),
                    angle,
                );
                let topic = sheet.topic_mut(root).unwrap();
                topic.structure = Some(structure);
                topic.style.line_shape = Some(line);
                topic.style.shape = Some(TopicShape::Underline);
                let child = sheet.add_child(root, None, "Child");
                let layout = layout_sheet(&sheet, &ApproxMeasure, false);
                let branch = layout
                    .connectors
                    .iter()
                    .find(|c| c.parent == root && c.child == child)
                    .unwrap();
                assert_eq!(branch.kind, line.into());
                let rect = layout.rect(root).unwrap();
                assert_on_rotated_border(rect, angle, branch.start);
                assert_exits_card(rect, angle, branch.start, direction);
            }
        }
    }
}

#[test]
fn zero_rotation_preserves_ordinary_relationships_and_branch_anchors() {
    for (structure, expected_start, expected_end) in [
        (
            Structure::LogicRight,
            Rect::right_center as fn(&Rect) -> Vec2,
            Rect::left_center as fn(&Rect) -> Vec2,
        ),
        (
            Structure::LogicLeft,
            Rect::left_center as fn(&Rect) -> Vec2,
            Rect::right_center as fn(&Rect) -> Vec2,
        ),
        (
            Structure::OrgDown,
            Rect::bottom_center as fn(&Rect) -> Vec2,
            Rect::top_center as fn(&Rect) -> Vec2,
        ),
        (
            Structure::OrgUp,
            Rect::top_center as fn(&Rect) -> Vec2,
            Rect::bottom_center as fn(&Rect) -> Vec2,
        ),
    ] {
        let mut sheet = Sheet::new("Map", "Root");
        sheet.structure = structure;
        sheet.topic_mut(sheet.root).unwrap().style.shape = Some(TopicShape::Rect);
        let child = sheet.add_child(sheet.root, None, "Child");
        sheet.topic_mut(child).unwrap().style.shape = Some(TopicShape::Rect);
        let relationship = sheet.add_relationship(sheet.root, child).unwrap();
        let layout = layout_sheet(&sheet, &ApproxMeasure, false);
        let root_rect = layout.rect(sheet.root).unwrap();
        let child_rect = layout.rect(child).unwrap();
        let branch = layout
            .connectors
            .iter()
            .find(|c| c.parent == sheet.root && c.child == child)
            .unwrap();
        assert_eq!(branch.start, expected_start(&root_rect), "{structure:?}");
        assert_eq!(branch.end, expected_end(&child_rect), "{structure:?}");
        let model = sheet
            .relationships
            .iter()
            .find(|r| r.id == relationship)
            .unwrap();
        assert_eq!(
            layout.relationships[0],
            relationship_curve(model, root_rect, child_rect)
        );
        assert_eq!(
            relationship_curve_for_sheet(&sheet, model, root_rect, child_rect),
            relationship_curve(model, root_rect, child_rect)
        );
    }

    // The historical underline anchor must stay at the lower corner when
    // rotation is zero, even though a card is painted as a rectangle.
    let mut sheet = Sheet::new("Board", "Root");
    let root = card(
        &mut sheet,
        Vec2::new(200.0, 100.0),
        Vec2::new(100.0, 60.0),
        0.0,
    );
    sheet.topic_mut(root).unwrap().style.shape = Some(TopicShape::Underline);
    let child = sheet.add_child(root, None, "Child");
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let branch = layout
        .connectors
        .iter()
        .find(|c| c.parent == root && c.child == child)
        .unwrap();
    assert_eq!(branch.start, layout.rect(root).unwrap().max());
}

#[test]
fn preview_geometry_handles_free_points_coincident_centers_and_control_points() {
    let mut sheet = Sheet::new("Board", "Root");
    let from = card(
        &mut sheet,
        Vec2::new(300.0, 200.0),
        Vec2::new(120.0, 50.0),
        90.0,
    );
    let to = card(
        &mut sheet,
        Vec2::new(600.0, 200.0),
        Vec2::new(80.0, 150.0),
        45.0,
    );
    let candidate = card(
        &mut sheet,
        Vec2::new(100.0, 300.0),
        Vec2::new(90.0, 50.0),
        270.0,
    );
    let id = sheet.add_relationship(from, to).unwrap();
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let model = sheet.relationships.iter().find(|r| r.id == id).unwrap();
    let a = layout.rect(from).unwrap();
    let b = layout.rect(to).unwrap();
    assert_eq!(
        relationship_curve_for_sheet(&sheet, model, a, b),
        layout.relationships[0]
    );
    let pointer = Vec2::new(810.0, -135.0);
    let point = Rect::from_center_size(pointer, Vec2::ZERO);
    let dragging_from = relationship_curve_for_sheet(&sheet, model, point, b);
    let dragging_to = relationship_curve_for_sheet(&sheet, model, a, point);
    assert_eq!(dragging_from.start, pointer);
    assert_on_rotated_border(b, 45.0, dragging_from.end);
    assert_eq!(dragging_to.end, pointer);
    assert_on_rotated_border(a, 90.0, dragging_to.start);

    let mut reconnected = model.clone();
    reconnected.from = candidate;
    let candidate_rect = layout.rect(candidate).unwrap();
    let preview = relationship_curve_for_sheet(&sheet, &reconnected, candidate_rect, b);
    let mut committed = sheet.clone();
    committed.reconnect_relationship(id, candidate, to).unwrap();
    assert_eq!(
        preview,
        layout_sheet(&committed, &ApproxMeasure, false).relationships[0]
    );
    assert_on_rotated_border(candidate_rect, 270.0, preview.start);

    for size in [Vec2::ZERO, Vec2::new(0.0, 40.0), Vec2::new(40.0, 0.0)] {
        let degenerate = Rect::from_center_size(a.center(), size);
        let curve = relationship_curve_for_sheet(&sheet, model, degenerate, b);
        assert!(curve.start.x.is_finite() && curve.start.y.is_finite());
        assert_on_rotated_border(degenerate, 90.0, curve.start);
    }

    // A ray has no direction when a control point equals its center. Keep the
    // legacy center fallback rather than manufacture an arbitrary edge.
    let mut changed = model.clone();
    changed.control_points = Some([Vec2::ZERO, Vec2::ZERO]);
    let curve = relationship_curve_for_sheet(&sheet, &changed, a, b);
    assert_eq!(curve.start, a.center());
    assert_eq!(curve.end, b.center());
    changed.control_points = None;
    let coincident = Rect::from_center_size(a.center(), b.size);
    let curve = relationship_curve_for_sheet(&sheet, &changed, a, coincident);
    for point in [curve.start, curve.c1, curve.c2, curve.end, curve.mid] {
        assert!(point.x.is_finite() && point.y.is_finite());
        assert_eq!(point, a.center());
    }
}

#[test]
fn free_board_and_relationship_connectors_share_rotated_clipping() {
    for rotation in [0.0, 30.0, 90.0, 145.0, 270.0] {
        let mut sheet = Sheet::new("Board", "Root");
        let root = card(
            &mut sheet,
            Vec2::new(300.0, 200.0),
            Vec2::new(120.0, 50.0),
            rotation,
        );
        let child = sheet.add_child(root, None, "Child");
        let target = sheet.add_floating("Target", Vec2::new(600.0, 400.0));
        let id = sheet.add_relationship(root, target).unwrap();
        let hidden_relationship = sheet.add_relationship(child, target).unwrap();
        let toward = Vec2::new(500.0, 300.0);
        sheet
            .set_relationship_control_points(
                id,
                Some([toward - Vec2::new(300.0, 200.0), Vec2::new(-30.0, -30.0)]),
            )
            .unwrap();
        sheet.set_collapsed(root, true).unwrap();
        sheet.validate().unwrap();
        let layout = layout_sheet(&sheet, &ApproxMeasure, false);
        assert!(layout.rect(child).is_none());
        assert!(
            layout
                .relationships
                .iter()
                .all(|r| r.id != hidden_relationship)
        );
        for route in [
            ConnectorRoute::Straight,
            ConnectorRoute::Elbow,
            ConnectorRoute::Curve,
        ] {
            let mut from_card =
                CanvasConnector::new(Endpoint::Topic(root), Endpoint::Point(toward));
            from_card.route = route;
            let mut from_hidden_child =
                CanvasConnector::new(Endpoint::Topic(child), Endpoint::Point(toward));
            from_hidden_child.route = route;
            let card_path = connector_path(&sheet, &layout, &from_card);
            let child_path = connector_path(&sheet, &layout, &from_hidden_child);
            if route != ConnectorRoute::Elbow {
                assert_eq!(layout.relationships[0].start, card_path[0]);
            }
            assert_eq!(
                child_path[0], card_path[0],
                "collapsed children should resolve to their visible card's rotation"
            );
            assert_eq!(*card_path.last().unwrap(), toward);
            assert_on_rotated_border(layout.rect(root).unwrap(), rotation, card_path[0]);
        }
    }
}

#[test]
fn frame_clipping_changes_ownership_not_connector_geometry() {
    let mut sheet = Sheet::new("Board", "Root");
    let a = card(
        &mut sheet,
        Vec2::new(200.0, 200.0),
        Vec2::new(120.0, 60.0),
        45.0,
    );
    let b = card(
        &mut sheet,
        Vec2::new(400.0, 200.0),
        Vec2::new(120.0, 60.0),
        90.0,
    );
    let outside = sheet.add_floating("Outside", Vec2::new(800.0, 200.0));
    let child = sheet.add_child(a, None, "Child");
    sheet.add_relationship(a, b).unwrap();
    sheet.add_relationship(a, outside).unwrap();
    let frame = Rect::from_min_size(Vec2::new(100.0, 100.0), Vec2::new(450.0, 240.0));
    let zone = sheet.create_zone("Frame", vec![a, b], frame).unwrap();
    let unclipped = layout_sheet(&sheet, &ApproxMeasure, false);
    sheet.zones.iter_mut().find(|z| z.id == zone).unwrap().clip = true;
    sheet.validate().unwrap();
    let clipped = layout_sheet(&sheet, &ApproxMeasure, false);
    assert_eq!(clipped.connectors, unclipped.connectors);
    assert_eq!(clipped.relationships, unclipped.relationships);
    for id in [a, b, child] {
        assert_eq!(topic_clip_zone(&sheet, id), Some((zone, frame)));
    }
    assert_eq!(topic_clip_zone(&sheet, outside), None);
    let scene = build_scene(&sheet, &clipped);
    assert_eq!(scene.item(SceneId::Topic(a)).unwrap().clip, Some(frame));
    assert_eq!(scene.item(SceneId::Topic(child)).unwrap().clip, Some(frame));
    assert_eq!(scene.item(SceneId::Topic(outside)).unwrap().clip, None);
    assert_on_rotated_border(
        clipped.rect(a).unwrap(),
        45.0,
        clipped.relationships[0].start,
    );
    assert_on_rotated_border(clipped.rect(b).unwrap(), 90.0, clipped.relationships[0].end);
    // The unrotated external topic still uses the original rectangle clip.
    let cross_frame = &clipped.relationships[1];
    assert_eq!(
        cross_frame.end,
        clip_to_rect(clipped.rect(outside).unwrap(), cross_frame.c2)
    );
}

#[test]
fn large_coordinates_and_near_quarter_turns_keep_finite_anchors() {
    for size in [
        Vec2::new(16.0, 20.0),
        Vec2::new(1_000_000.0, 16.0),
        Vec2::new(16.0, 1_000_000.0),
    ] {
        for angle in [0.000_01, 89.999, 90.0, 90.001, 179.999, 270.0, 359.999] {
            for structure in [
                Structure::LogicRight,
                Structure::LogicLeft,
                Structure::OrgDown,
                Structure::OrgUp,
                Structure::TreeRight,
                Structure::TreeLeft,
                Structure::FishboneRight,
                Structure::FishboneLeft,
            ] {
                let mut sheet = Sheet::new("Board", "Root");
                let root = card(&mut sheet, Vec2::new(999_000.0, -999_000.0), size, angle);
                sheet.topic_mut(root).unwrap().structure = Some(structure);
                let child = sheet.add_child(root, None, "Child");
                let target = sheet.add_floating("Target", Vec2::new(-999_000.0, 999_000.0));
                sheet.add_relationship(root, target).unwrap();
                sheet.validate().unwrap();
                let layout = layout_sheet(&sheet, &ApproxMeasure, false);
                let branch = layout
                    .connectors
                    .iter()
                    .find(|c| {
                        c.parent == root
                            && if matches!(
                                structure,
                                Structure::FishboneRight | Structure::FishboneLeft
                            ) {
                                c.child == root
                            } else {
                                c.child == child
                            }
                    })
                    .unwrap();
                let relationship = &layout.relationships[0];
                for point in [branch.start, relationship.start, relationship.end] {
                    assert!(point.x.is_finite() && point.y.is_finite());
                }
                // Sheet-space f32 positions near a million have sub-unit
                // quantization; this tolerance is not a visual assertion.
                let rect = layout.rect(root).unwrap();
                assert_on_rotated_border_with_tolerance(rect, angle, branch.start, 0.5);
                assert_on_rotated_border_with_tolerance(rect, angle, relationship.start, 0.5);
            }
        }
    }
}

#[test]
fn near_axis_control_ray_stays_on_extremely_thin_rotated_card() {
    let mut sheet = Sheet::new("Board", "Root");
    let root = card(&mut sheet, Vec2::ZERO, Vec2::new(16.0, 1_000_000.0), 90.0);
    let target = sheet.add_floating("Target", Vec2::new(500.0, 500.0));
    let id = sheet.add_relationship(root, target).unwrap();
    // The local x component is smaller than the legacy axis threshold, but it
    // reaches the narrow side before the ray can reach the much taller side.
    let toward = rotate_around(Vec2::new(0.000_000_9, 0.01), Vec2::ZERO, 90.0);
    sheet
        .set_relationship_control_points(id, Some([toward, Vec2::new(-1.0, 0.0)]))
        .unwrap();
    sheet.validate().unwrap();
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let curve = &layout.relationships[0];
    assert_on_rotated_border_with_tolerance(layout.rect(root).unwrap(), 90.0, curve.start, 0.1);
    assert_ray(layout.rect(root).unwrap().center(), curve.c1, curve.start);
}
