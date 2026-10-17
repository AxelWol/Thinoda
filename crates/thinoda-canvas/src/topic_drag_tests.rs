use super::*;
use egui_kittest::Harness;
use thinoda_core::{EditError, Workbook};

struct State {
    doc: Document,
    view: MapView,
    errors: Vec<EditError>,
    source: TopicId,
    child: TopicId,
    target: TopicId,
    summary: TopicId,
}

fn harness(zoom: f32) -> Harness<'static, State> {
    let mut sheet = Sheet::new("Drag", "Center");
    let source = sheet.add_floating("Source", Vec2::new(-400.0, -220.0));
    let child = sheet.insert_child(source, "Child").unwrap();
    sheet.insert_child(child, "Grandchild").unwrap();
    sheet
        .set_notes(child, Some("Retain these notes".into()))
        .unwrap();
    sheet
        .set_labels(child, vec!["Retain label".into()])
        .unwrap();
    let summary = sheet.add_summary(&[child], "Owned summary").unwrap();
    sheet.insert_child(summary, "Summary child").unwrap();
    sheet.add_boundary(&[child]).unwrap();
    let target = sheet.add_floating("Target", Vec2::new(450.0, 220.0));
    sheet.add_relationship(child, target).unwrap();
    sheet.add_relationship(source, child).unwrap();
    let mut doc = Document::new(Workbook {
        sheets: vec![sheet],
        active_sheet: 0,
        metadata: Default::default(),
    });
    doc.mark_saved();
    let mut view = MapView::default();
    view.select_only(source);
    let mut h = Harness::builder()
        .with_size(vec2(2400.0, 1600.0))
        .build_ui_state(
            |ui, s: &mut State| {
                if let Some(error) = MapCanvas::show(ui, &mut s.doc, &mut s.view).error {
                    s.errors.push(error);
                }
            },
            State {
                doc,
                view,
                errors: Vec::new(),
                source,
                child,
                target,
                summary,
            },
        );
    h.run_steps(3);
    h.state_mut().view.set_zoom(zoom);
    h.run_steps(2);
    h
}

fn center(h: &mut Harness<'_, State>, id: TopicId) -> Pos2 {
    let s = h.state_mut();
    let layout = s.view.layout_for(&s.doc);
    s.view.xf().pos(layout.topics[&id].rect.center())
}

fn start_drag(h: &mut Harness<'_, State>, id: TopicId, end: Pos2) {
    let start = center(h, id);
    h.hover_at(start);
    h.drag_at(start);
    h.run_steps(1);
    h.hover_at(end);
    h.run_steps(2);
}

#[test]
fn subtree_drag_paints_full_content_without_edits_then_moves_once_at_each_zoom() {
    for zoom in [0.5, 1.0, 2.0] {
        let mut h = harness(zoom);
        let source = h.state().source;
        let start = center(&mut h, source);
        let before = h.state().doc.workbook().clone();
        let end = start + vec2(80.0, -60.0);
        start_drag(&mut h, source, end);
        assert!(matches!(h.state().view.drag, Drag::Topics { .. }));
        assert_eq!(h.state().doc.workbook(), &before);
        assert!(!h.state().doc.is_dirty());
        for title in ["Source", "Grandchild", "Owned summary", "Summary child"] {
            let count = h
                .output()
                .shapes
                .iter()
                .filter(|shape| {
                    matches!(&shape.shape,
                        egui::Shape::Text(text) if text.galley.text() == title
                    )
                })
                .count();
            assert_eq!(count, 2, "source and ghost text for {title}");
        }
        h.drop_at(end);
        h.run_steps(2);
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
        let expected = before.sheets[0].topic(source).unwrap().position.unwrap()
            + Vec2::new(80.0 / zoom, -60.0 / zoom);
        let actual = h
            .state()
            .doc
            .sheet()
            .topic(source)
            .unwrap()
            .position
            .unwrap();
        assert!((actual - expected).length() < 0.01);
        let after = h.state().doc.workbook().clone();
        assert!(h.state_mut().doc.undo());
        assert_eq!(h.state().doc.workbook(), &before);
        assert!(h.state_mut().doc.redo());
        assert_eq!(h.state().doc.workbook(), &after);
    }
}

#[test]
fn reparenting_moves_only_selected_roots_and_retains_owned_content_and_links() {
    let mut h = harness(1.0);
    let (source, child, summary, target) = (
        h.state().source,
        h.state().child,
        h.state().summary,
        h.state().target,
    );
    h.state_mut().view.selection = vec![source, child, summary];
    let before = h.state().doc.workbook().clone();
    let end = center(&mut h, target);
    start_drag(&mut h, source, end);
    let Drag::Topics { ids, .. } = &h.state().view.drag else {
        panic!("missing preview")
    };
    assert_eq!(ids, &[source]);
    assert_eq!(h.state().doc.workbook(), &before);
    h.drop_at(end);
    h.run_steps(2);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    let sheet = h.state().doc.sheet();
    assert_eq!(sheet.parent(source), Some(target));
    assert_eq!(sheet.parent(child), Some(source));
    assert_eq!(sheet.relationships, before.sheets[0].relationships);
    assert_eq!(sheet.boundaries, before.sheets[0].boundaries);
    assert_eq!(sheet.summaries, before.sheets[0].summaries);
    for id in sheet.owned_subtree_ids(source) {
        if id != source {
            assert_eq!(sheet.topic(id), before.sheets[0].topic(id));
        }
    }
    sheet.validate().unwrap();
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
}

#[test]
fn escape_popup_stale_revision_and_selection_changes_cancel_without_overwriting_source() {
    for how in 0..4 {
        let mut h = harness(1.0);
        let source = h.state().source;
        let target = h.state().target;
        let end = center(&mut h, target);
        start_drag(&mut h, source, end);
        assert!(matches!(h.state().view.drag, Drag::Topics { .. }));
        match how {
            0 => {
                h.key_press(Key::Delete);
                h.run_steps(1);
                assert!(h.state().doc.sheet().contains(source));
                h.key_press(Key::Escape);
            }
            1 => egui::Popup::open_id(&h.ctx, Id::unique("drag-blocking-popup")),
            2 => {
                let root = h.state().doc.sheet().root;
                h.state_mut()
                    .doc
                    .edit_sheet("Newer edit", |s| s.set_title(root, "Newer"))
                    .unwrap();
            }
            _ => h.state_mut().view.select_only(target),
        }
        let before = h.state().doc.workbook().clone();
        h.run_steps(2);
        assert!(matches!(h.state().view.drag, Drag::None));
        h.drop_at(end);
        h.run_steps(2);
        assert_eq!(h.state().doc.workbook(), &before);
        assert_eq!(!h.state().errors.is_empty(), how >= 2);
        egui::Popup::close_all(&h.ctx);
    }
}

#[test]
fn self_descendant_and_outside_drops_report_errors_instead_of_detaching() {
    for how in 0..3 {
        let mut h = harness(1.0);
        let (source, child) = (h.state().source, h.state().child);
        let start = center(&mut h, source);
        let end = match how {
            0 => start + vec2(20.0, 0.0),
            1 => center(&mut h, child),
            _ => h.state().view.canvas_rect().right_center() + vec2(100.0, 0.0),
        };
        let before = h.state().doc.workbook().clone();
        start_drag(&mut h, source, end);
        h.drop_at(end);
        h.run_steps(2);
        assert_eq!(h.state().doc.workbook(), &before);
        assert!(
            !h.state().errors.is_empty(),
            "missing error for invalid drop {how}"
        );
        assert!(matches!(h.state().view.drag, Drag::None));
        assert!(!h.state().doc.is_dirty());
    }
}

#[test]
fn central_and_summary_roots_cannot_start_a_move() {
    for summary in [false, true] {
        let mut h = harness(1.0);
        let id = if summary {
            h.state().summary
        } else {
            h.state().doc.sheet().root
        };
        h.state_mut().view.select_only(id);
        let start = center(&mut h, id);
        let before = h.state().doc.workbook().clone();
        start_drag(&mut h, id, start + vec2(60.0, 60.0));
        assert!(matches!(h.state().view.drag, Drag::None));
        assert!(!h.state().errors.is_empty());
        h.drop_at(start + vec2(60.0, 60.0));
        h.run_steps(2);
        assert_eq!(h.state().doc.workbook(), &before);
    }
}

#[test]
fn blank_drop_detaches_regular_branches_and_reparenting_expands_collapsed_targets() {
    for detach in [false, true] {
        let mut h = harness(1.0);
        let (source, target) = (h.state().source, h.state().target);
        let root = h.state().doc.sheet().root;
        h.state_mut()
            .doc
            .edit_sheet("Prepare", |s| {
                s.insert_child(target, "Existing child")?;
                s.set_collapsed(target, true)?;
                if detach {
                    s.move_topic(source, root, 0)?;
                }
                Ok(())
            })
            .unwrap();
        h.run_steps(2);
        h.state_mut().doc.mark_saved();
        let before = h.state().doc.workbook().clone();
        let start = center(&mut h, source);
        let end = if detach {
            start + vec2(-100.0, -100.0)
        } else {
            center(&mut h, target)
        };
        start_drag(&mut h, source, end);
        assert_eq!(h.state().doc.workbook(), &before);
        h.drop_at(end);
        h.run_steps(2);
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
        let sheet = h.state().doc.sheet();
        if detach {
            assert!(sheet.is_floating(source));
            let actual = h
                .state()
                .view
                .xf()
                .pos(sheet.topic(source).unwrap().position.unwrap());
            assert!((actual - end).length() < 0.01);
        } else {
            assert_eq!(sheet.parent(source), Some(target));
            assert!(!sheet.topic(target).unwrap().collapsed);
        }
        sheet.validate().unwrap();
        assert!(h.state_mut().doc.undo());
        assert_eq!(h.state().doc.workbook(), &before);
    }
}

#[test]
fn sibling_slot_preview_commits_at_the_highlighted_index_and_undoes_once() {
    for zoom in [0.5, 1.0, 2.0] {
        let mut h = harness(zoom);
        let (source, parent) = (h.state().source, h.state().target);
        let target = h
            .state_mut()
            .doc
            .edit_sheet("Fixture", |s| {
                s.insert_child(parent, "Before")?;
                let target = s.insert_child(parent, "Slot target")?;
                s.insert_child(parent, "After")?;
                Ok(target)
            })
            .unwrap();
        h.run_steps(2);
        let s = h.state_mut();
        let layout = s.view.layout_for(&s.doc);
        let rect = layout.topics[&target].rect;
        let end = s
            .view
            .xf()
            .pos(Vec2::new(rect.center().x, rect.min.y + 1.0));
        let before = h.state().doc.workbook().clone();
        start_drag(&mut h, source, end);
        assert_eq!(h.state().doc.workbook(), &before);
        assert!(h.output().shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::LineSegment { stroke, .. } if stroke.width == 3.0 && stroke.color == h.state().view.accent
        )), "missing insertion slot at zoom {zoom}");
        h.drop_at(end);
        h.run_steps(2);
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
        assert_eq!(h.state().doc.sheet().children(parent)[1], source);
        assert_eq!(h.state().doc.sheet().children(parent)[2], target);
        assert!(h.state_mut().doc.undo());
        assert_eq!(h.state().doc.workbook(), &before);
    }
}

#[test]
fn root_drop_chooses_and_persists_the_pointer_side() {
    for left in [false, true] {
        let mut h = harness(1.0);
        let source = h.state().source;
        let root = h.state().doc.sheet().root;
        let s = h.state_mut();
        let layout = s.view.layout_for(&s.doc);
        let rect = layout.topics[&root].rect;
        let end = s
            .view
            .xf()
            .pos(rect.center() + Vec2::new(if left { -15.0 } else { 15.0 }, 0.0));
        let before = h.state().doc.workbook().clone();
        start_drag(&mut h, source, end);
        h.drop_at(end);
        h.run_steps(2);
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
        let s = h.state_mut();
        let layout = s.view.layout_for(&s.doc);
        assert_eq!(s.doc.sheet().parent(source), Some(root));
        assert_eq!(
            s.doc.sheet().topic(source).unwrap().branch_side,
            Some(if left {
                thinoda_core::BranchSide::Left
            } else {
                thinoda_core::BranchSide::Right
            })
        );
        assert_eq!(
            layout.topics[&source].rect.center().x < layout.topics[&root].rect.center().x,
            left
        );
        assert!(s.doc.undo());
        assert_eq!(s.doc.workbook(), &before);
    }
}

#[test]
fn edge_autopan_is_bounded_and_keeps_ghost_under_stationary_pointer() {
    let mut h = harness(1.0);
    let source = h.state().source;
    let before = h.state().doc.workbook().clone();
    let end = h.state().view.canvas_rect().right_center() - vec2(4.0, 0.0);
    start_drag(&mut h, source, end);
    let offset = h.state().view.offset;
    h.run_steps(1);
    let delta = offset.x - h.state().view.offset.x;
    assert!(delta > 0.0 && delta <= 24.01, "pan per frame: {delta}");
    h.run_steps(6);
    assert!(h.state().view.offset.x < offset.x - delta);
    let s = h.state_mut();
    let (grab, current) = match s.view.drag {
        Drag::Topics { grab, current, .. } => (grab, current),
        _ => panic!("missing preview"),
    };
    let layout = s.view.layout_for(&s.doc);
    let ghost = s
        .view
        .xf()
        .pos(layout.topics[&source].rect.center() + current - grab);
    assert!((ghost - end).length() < 0.01);
    assert_eq!(h.state().doc.workbook(), &before);
    h.key_press(Key::Escape);
    h.run_steps(2);
    h.drop_at(end);
    h.run_steps(2);
    assert_eq!(h.state().doc.workbook(), &before);
}

#[test]
fn flexible_floating_topics_overlap_targets_without_attachment_at_each_zoom() {
    for zoom in [0.5, 1.0, 2.0] {
        let mut h = harness(zoom);
        h.state_mut()
            .doc
            .edit_sheet("Flexible", |s| {
                s.theme.flexible_floating_topics = true;
                Ok(())
            })
            .unwrap();
        h.run_steps(2);
        let (source, target) = (h.state().source, h.state().target);
        let end = center(&mut h, target);
        let before = h.state().doc.workbook().clone();
        start_drag(&mut h, source, end);
        assert_eq!(h.state().doc.workbook(), &before);
        assert!(!h.output().shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::LineSegment { stroke, .. } if stroke.width == 3.0 && stroke.color == h.state().view.accent
        )));
        h.drop_at(end);
        h.run_steps(2);
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
        let s = h.state_mut();
        assert!(s.doc.sheet().is_floating(source));
        assert_eq!(s.doc.sheet().relationships, before.sheets[0].relationships);
        let actual = s
            .view
            .xf()
            .pos(s.doc.sheet().topic(source).unwrap().position.unwrap());
        assert!((actual - end).length() < 0.01);
        assert!(s.doc.undo());
        assert_eq!(s.doc.workbook(), &before);
    }
}

#[test]
fn flexible_mode_rejects_detach_and_mixed_root_moves_but_allows_regular_reparenting() {
    for how in 0..3 {
        let mut h = harness(1.0);
        let (source, child, target) = (h.state().source, h.state().child, h.state().target);
        h.state_mut()
            .doc
            .edit_sheet("Flexible", |s| {
                s.theme.flexible_floating_topics = true;
                Ok(())
            })
            .unwrap();
        if how == 1 {
            h.state_mut().view.selection = vec![child, target];
        } else {
            h.state_mut().view.select_only(child);
        }
        h.run_steps(2);
        let start = center(&mut h, child);
        let end = if how == 0 {
            start + vec2(-100.0, -100.0)
        } else {
            center(&mut h, target)
        };
        let before = h.state().doc.workbook().clone();
        start_drag(&mut h, child, end);
        h.drop_at(end);
        h.run_steps(2);
        if how == 2 {
            assert_eq!(h.state().doc.sheet().parent(child), Some(target));
            assert!(h.state().doc.sheet().is_floating(source));
            assert!(h.state().errors.is_empty());
            assert!(h.state_mut().doc.undo());
        } else {
            assert!(
                !h.state().errors.is_empty(),
                "mode {how} needs a visible error"
            );
        }
        assert_eq!(h.state().doc.workbook(), &before);
    }
}

#[test]
fn changing_flexible_setting_during_drag_cancels_without_committing_stale_target() {
    let mut h = harness(1.0);
    let (source, target) = (h.state().source, h.state().target);
    let end = center(&mut h, target);
    start_drag(&mut h, source, end);
    h.state_mut()
        .doc
        .edit_sheet("Flexible", |s| {
            s.theme.flexible_floating_topics = true;
            Ok(())
        })
        .unwrap();
    let before = h.state().doc.workbook().clone();
    h.drop_at(end);
    h.run_steps(2);
    assert_eq!(h.state().doc.workbook(), &before);
    assert!(matches!(h.state().view.drag, Drag::None));
    assert!(!h.state().errors.is_empty());
}
