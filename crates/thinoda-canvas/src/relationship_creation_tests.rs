use super::*;
use egui_kittest::Harness;
use thinoda_core::{EditError, Workbook};

struct State {
    doc: Document,
    view: MapView,
    errors: Vec<EditError>,
}

fn harness(zoom: f32) -> Harness<'static, State> {
    let mut doc = Document::new(Workbook {
        sheets: vec![Sheet::new("Relationships", "Source")],
        active_sheet: 0,
        metadata: Default::default(),
    });
    doc.mark_saved();
    let mut view = MapView::default();
    view.select_only(doc.sheet().root);
    view.titles.floating = "Connected idea".into();
    let mut h = Harness::builder()
        .with_size(vec2(1200.0, 900.0))
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
            },
        );
    h.run_steps(3);
    h.state_mut().view.set_zoom(zoom);
    h.state_mut().view.offset += vec2(37.0, -21.0);
    h.run_steps(2);
    let s = h.state_mut();
    crate::apply_action(&mut s.doc, &mut s.view, Action::StartRelationship).unwrap();
    h
}

fn click(h: &mut Harness<'_, State>, point: Pos2) {
    h.hover_at(point);
    h.run_steps(1);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: point,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(1);
    }
}

#[test]
fn exactly_two_selected_topics_connect_immediately_in_one_undo_step() {
    let mut h = harness(1.0);
    let source = h.state().doc.sheet().root;
    let target = h
        .state_mut()
        .doc
        .edit_sheet("Fixture", |s| {
            Ok(s.add_floating("Existing", Vec2::new(200.0, 130.0)))
        })
        .unwrap();
    h.state_mut().view.relationship_from = None;
    h.state_mut().view.selection = vec![source, target];
    let before = h.state().doc.workbook().clone();
    let s = h.state_mut();
    crate::apply_action(&mut s.doc, &mut s.view, Action::StartRelationship).unwrap();
    let sheet = h.state().doc.sheet();
    assert_eq!(sheet.relationships.len(), 1);
    assert_eq!(
        (sheet.relationships[0].from, sheet.relationships[0].to),
        (source, target)
    );
    assert!(h.state().view.relationship_from.is_none());
    assert_eq!(
        h.state().view.selected_object,
        Some(ObjectSelection::Relationship(sheet.relationships[0].id))
    );
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.workbook(), &before);
}

#[test]
fn blank_relationship_target_creates_one_connected_floating_topic_at_each_zoom() {
    for zoom in [0.5, 1.0, 2.0] {
        let mut h = harness(zoom);
        let before = h.state().doc.workbook().clone();
        let source = h.state().doc.sheet().root;
        let at = Vec2::new(200.0, 130.0);
        let point = h.state().view.xf().pos(at);
        click(&mut h, point);
        assert!(h.state().errors.is_empty());
        let sheet = h.state().doc.sheet();
        assert_eq!(sheet.floating.len(), 1);
        assert_eq!(sheet.relationships.len(), 1);
        let target = sheet.floating[0];
        let topic = sheet.topic(target).unwrap();
        assert_eq!(topic.title, "Connected idea");
        assert!((topic.position.unwrap() - at).length() < 0.01);
        assert_eq!(
            (sheet.relationships[0].from, sheet.relationships[0].to),
            (source, target)
        );
        assert_eq!(h.state().view.primary(), Some(target));
        assert_eq!(h.state().view.editing_topic(), Some(target));
        assert!(h.state().view.relationship_from.is_none());
        sheet.validate().unwrap();
        let after = h.state().doc.workbook().clone();
        assert!(h.state_mut().doc.undo());
        assert_eq!(h.state().doc.workbook(), &before);
        assert!(h.state_mut().doc.redo());
        assert_eq!(h.state().doc.workbook(), &after);
    }
}

#[test]
fn escape_and_outside_clicks_do_not_create_targets() {
    for cancel in [false, true] {
        let mut h = harness(1.0);
        let before = h.state().doc.workbook().clone();
        let point = if cancel {
            h.key_press(Key::Escape);
            h.run_steps(2);
            h.state().view.xf().pos(Vec2::new(200.0, 130.0))
        } else {
            h.state().view.canvas_rect().right_center() + vec2(80.0, 0.0)
        };
        click(&mut h, point);
        assert_eq!(h.state().doc.workbook(), &before);
        assert!(!h.state().doc.is_dirty());
        assert!(h.state().errors.is_empty());
    }
}

#[test]
fn invalid_source_rolls_back_both_creation_and_connection() {
    let mut h = harness(1.0);
    h.state_mut().view.relationship_from = Some(TopicId(u64::MAX));
    let before = h.state().doc.workbook().clone();
    let state = h.state().doc.state_id();
    let point = h.state().view.xf().pos(Vec2::new(200.0, 130.0));
    click(&mut h, point);
    assert_eq!(h.state().doc.workbook(), &before);
    assert_eq!(h.state().doc.state_id(), state);
    assert!(!h.state().errors.is_empty());
}

#[test]
fn existing_topic_target_is_connected_and_self_target_reports_an_error() {
    for self_target in [false, true] {
        let mut h = harness(1.0);
        let source = h.state().doc.sheet().root;
        let target = if self_target {
            source
        } else {
            h.state_mut()
                .doc
                .edit_sheet("Fixture", |s| {
                    Ok(s.add_floating("Existing", Vec2::new(200.0, 130.0)))
                })
                .unwrap()
        };
        h.run_steps(2);
        let s = h.state_mut();
        let layout = s.view.layout_for(&s.doc);
        let point = s.view.xf().pos(layout.topics[&target].rect.center());
        let before = h.state().doc.workbook().clone();
        click(&mut h, point);
        if self_target {
            assert_eq!(h.state().doc.workbook(), &before);
            assert!(!h.state().errors.is_empty());
        } else {
            assert_eq!(h.state().doc.sheet().floating.len(), 1);
            assert_eq!(h.state().doc.sheet().relationships.len(), 1);
            assert!(matches!(
                h.state().view.selected_object,
                Some(ObjectSelection::Relationship(_))
            ));
            assert!(h.state().errors.is_empty());
        }
    }
}

#[test]
fn rapid_double_click_does_not_create_a_second_floating_topic() {
    let mut h = harness(1.0);
    let point = h.state().view.canvas_rect().right_bottom() - vec2(8.0, 8.0);
    click(&mut h, point);
    click(&mut h, point);
    assert_eq!(h.state().doc.sheet().floating.len(), 1);
    assert_eq!(h.state().doc.sheet().relationships.len(), 1);
    assert!(h.state().errors.is_empty());
}
