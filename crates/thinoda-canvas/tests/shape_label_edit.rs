use egui::{Key, Modifiers, PointerButton, Pos2, Role, vec2};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use thinoda_canvas::{BoardLabelTarget, BoardTool, MapCanvas, MapView};
use thinoda_core::{
    CanvasObject, CanvasObjectId, CanvasObjectKind, Document, ShapeKind, Sheet, SheetRootMode,
    Vec2, Workbook,
};

struct State {
    doc: Document,
    view: MapView,
    shape: CanvasObjectId,
    errors: Vec<String>,
}

fn harness(label: &str) -> Harness<'static, State> {
    harness_with_size(label, vec2(800.0, 600.0))
}

fn harness_with_size(label: &str, size: egui::Vec2) -> Harness<'static, State> {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut shape = CanvasObject::shape(
        ShapeKind::RoundedRectangle,
        thinoda_core::Rect::from_center_size(Vec2::ZERO, Vec2::new(240.0, 150.0)),
    );
    if let CanvasObjectKind::Shape { label: text, .. } = &mut shape.kind {
        *text = label.into();
    }
    let shape = sheet.add_canvas_object(shape).unwrap();
    let mut h = Harness::builder()
        .with_size(size)
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(
            |ui, state: &mut State| {
                if let Some(error) = MapCanvas::show(ui, &mut state.doc, &mut state.view).error {
                    state.errors.push(error.to_string());
                }
            },
            State {
                doc: Document::new(Workbook::new(sheet)),
                view: MapView::default(),
                shape,
                errors: Vec::new(),
            },
        );
    h.run_steps(4);
    h
}

fn click(h: &mut Harness<'_, State>, position: Pos2) {
    h.hover_at(position);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: position,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(1);
    }
}

fn double_click_shape(h: &mut Harness<'_, State>) {
    let center = h.state().view.canvas_rect().center() + h.state().view.offset;
    click(h, center);
    click(h, center);
    h.run_steps(3);
    assert_eq!(h.state().view.editing_board_object(), Some(h.state().shape));
}

fn begin_connector_label(h: &mut Harness<'_, State>, label: &str) -> CanvasObjectId {
    use thinoda_core::{Endpoint, board::CanvasConnector};

    let state = h.state_mut();
    let mut object = CanvasObject::connector(CanvasConnector::new(
        Endpoint::Point(Vec2::new(-100.0, -180.0)),
        Endpoint::Point(Vec2::new(100.0, -180.0)),
    ));
    object.set_label(label).unwrap();
    let id = state.doc.add_canvas_object(object).unwrap();
    assert!(state.view.begin_board_label(&state.doc, id));
    h.run_steps(3);
    id
}

fn add_positioned_connector(h: &mut Harness<'_, State>) -> CanvasObjectId {
    use thinoda_core::{
        Endpoint,
        board::{CanvasConnector, ConnectorLabel, ConnectorLabelPlacement},
    };

    let mut connector = CanvasConnector::new(
        Endpoint::Point(Vec2::new(-150.0, 100.0)),
        Endpoint::Point(Vec2::new(150.0, 100.0)),
    );
    connector.label = "Primary".into();
    connector.extra_labels = vec![
        ConnectorLabel {
            text: "First".into(),
            placement: ConnectorLabelPlacement {
                along: -0.5,
                orthogonal: -80.0,
                ..Default::default()
            },
            rich_text: None,
        },
        ConnectorLabel {
            text: "Second".into(),
            placement: ConnectorLabelPlacement {
                along: 0.5,
                orthogonal: -130.0,
                ..Default::default()
            },
            rich_text: None,
        },
    ];
    let id = h
        .state_mut()
        .doc
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    h.run_steps(3);
    id
}

fn set_label_cursor(h: &mut Harness<'_, State>, index: usize) {
    let id = MapCanvas::board_label_editor_id();
    let mut state = egui::TextEdit::load_state(&h.ctx, id).unwrap();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(index),
        )));
    state.store(&h.ctx, id);
    h.run_steps(2);
}

fn accessible_replace(text: &str) -> egui::Event {
    egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
        action: egui::accesskit::Action::ReplaceSelectedText,
        target_node: MapCanvas::board_label_editor_id().accesskit_id(),
        target_tree: egui::accesskit::TreeId::ROOT,
        data: Some(egui::accesskit::ActionData::Value(text.into())),
    })
}

fn accessible_set_value(text: &str) -> egui::Event {
    egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
        action: egui::accesskit::Action::SetValue,
        target_node: MapCanvas::board_label_editor_id().accesskit_id(),
        target_tree: egui::accesskit::TreeId::ROOT,
        data: Some(egui::accesskit::ActionData::Value(text.into())),
    })
}

fn set_label_selection(h: &mut Harness<'_, State>, start: usize, end: usize) {
    let id = MapCanvas::board_label_editor_id();
    let mut state = egui::TextEdit::load_state(&h.ctx, id).unwrap();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(start),
            egui::text::CCursor::new(end),
        )));
    state.store(&h.ctx, id);
    h.run_steps(2);
}

#[test]
fn selected_formatting_has_local_rich_undo_and_directional_redo() {
    let mut h = harness("abcd");
    double_click_shape(&mut h);
    set_label_selection(&mut h, 3, 1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::B);
    h.run_steps(2);
    let rich = h.state().view.board_rich_label_draft().unwrap().3.unwrap();
    assert_eq!((rich.runs[0].start, rich.runs[0].end), (1, 3));
    assert_eq!(rich.runs[0].style.bold, Some(true));
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    assert!(h.state().view.board_rich_label_draft().unwrap().3.is_none());
    let cursor = egui::TextEdit::load_state(&h.ctx, MapCanvas::board_label_editor_id())
        .unwrap()
        .cursor
        .char_range()
        .unwrap();
    assert_eq!((cursor.primary.index.0, cursor.secondary.index.0), (3, 1));
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    h.run_steps(2);
    let rich = h.state().view.board_rich_label_draft().unwrap().3.unwrap();
    assert_eq!((rich.runs[0].start, rich.runs[0].end), (1, 3));
    assert_eq!(rich.runs[0].style.bold, Some(true));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn repeated_input_and_deletion_preserve_extra_connector_label_formatting() {
    let mut h = harness("Shape");
    let id = add_positioned_connector(&mut h);
    let mut rich = thinoda_core::InlineText::default();
    rich.update_range("aaa", 1..2, |style| style.bold = Some(true))
        .unwrap();
    h.state_mut()
        .doc
        .update_canvas_object("Set extra label", id, |object| {
            let CanvasObjectKind::Connector(connector) = &mut object.kind else {
                panic!("expected connector");
            };
            connector.extra_labels[0].text = "aaa".into();
            connector.extra_labels[0].rich_text = Some(std::sync::Arc::new(rich.clone()));
        })
        .unwrap();
    {
        let state = h.state_mut();
        assert!(state.view.begin_board_extra_label(&state.doc, id, 0));
    }
    h.run_steps(3);
    set_label_cursor(&mut h, 1);
    h.event(egui::Event::Text("a".into()));
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    let (_, _, text, restored) = h.state().view.board_rich_label_draft().unwrap();
    assert_eq!(text, "aaa");
    assert_eq!(restored, Some(&rich));
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
    let object = h.state().doc.sheet().canvas.object(id).unwrap();
    let CanvasObjectKind::Connector(connector) = &object.kind else {
        panic!("expected connector");
    };
    assert_eq!(connector.extra_labels[0].text, "aaaa");
    let run = &connector.extra_labels[0].rich_text.as_ref().unwrap().runs[0];
    assert_eq!((run.start, run.end), (2, 3));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);

    {
        let state = h.state_mut();
        assert!(state.view.begin_board_extra_label(&state.doc, id, 0));
    }
    h.run_steps(3);
    set_label_cursor(&mut h, 1);
    h.key_press(Key::Backspace);
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
    let object = h.state().doc.sheet().canvas.object(id).unwrap();
    let CanvasObjectKind::Connector(connector) = &object.kind else {
        panic!("expected connector");
    };
    assert_eq!(connector.extra_labels[0].text, "aaa");
    assert_eq!(connector.extra_labels[0].rich_text.as_deref(), Some(&rich));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn caret_formatting_styles_new_text_and_toggles_off() {
    for connector in [false, true] {
        let mut h = harness("");
        let id = if connector {
            begin_connector_label(&mut h, "")
        } else {
            double_click_shape(&mut h);
            h.state().shape
        };
        let before = h.state().doc.state_id();
        h.key_press_modifiers(Modifiers::COMMAND, Key::B);
        h.run_steps(2);
        assert_eq!(h.state().doc.state_id(), before);
        h.event(egui::Event::Text("Bold".into()));
        h.run_steps(2);
        h.key_press_modifiers(Modifiers::COMMAND, Key::B);
        h.run_steps(2);
        h.event(egui::Event::Text(" plain".into()));
        h.run_steps(2);
        let (_, text, rich) = h
            .state()
            .view
            .validated_board_rich_label_draft(&h.state().doc)
            .unwrap()
            .unwrap();
        assert_eq!(text, "Bold plain");
        let rich = rich.unwrap();
        assert_eq!(rich.runs.len(), 1);
        assert_eq!((rich.runs[0].start, rich.runs[0].end), (0, 4));
        assert_eq!(rich.runs[0].style.bold, Some(true));
        h.key_press(Key::Enter);
        h.run_steps(2);
        assert_eq!(
            h.state().doc.sheet().canvas.object(id).unwrap().label(),
            "Bold plain"
        );
        assert!(h.state_mut().doc.undo());
        assert_eq!(h.state().doc.sheet().canvas.object(id).unwrap().label(), "");
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn caret_formatting_preserves_repeated_text_and_existing_runs() {
    let mut h = harness("aaa");
    let shape = h.state().shape;
    let mut rich = thinoda_core::InlineText::default();
    rich.update_range("aaa", 0..1, |style| style.bold = Some(true))
        .unwrap();
    rich.update_range("aaa", 2..3, |style| style.underline = Some(true))
        .unwrap();
    h.state_mut()
        .doc
        .update_canvas_object("Initial formatting", shape, |object| {
            object.label_rich_text = Some(std::sync::Arc::new(rich));
        })
        .unwrap();
    double_click_shape(&mut h);
    set_label_cursor(&mut h, 0);
    h.key_press_modifiers(Modifiers::COMMAND, Key::I);
    h.run_steps(2);
    h.event(egui::Event::Text("a".into()));
    h.run_steps(2);
    let (_, text, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(text, "aaaa");
    let rich = rich.unwrap();
    assert_eq!((rich.runs[0].start, rich.runs[0].end), (0, 1));
    assert_eq!(rich.runs[0].style.bold, Some(true));
    assert_eq!(rich.runs[0].style.italic, Some(true));
    assert_eq!((rich.runs[1].start, rich.runs[1].end), (1, 2));
    assert_eq!(rich.runs[1].style.bold, Some(true));
    assert_eq!((rich.runs[2].start, rich.runs[2].end), (3, 4));
    assert_eq!(rich.runs[2].style.underline, Some(true));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn caret_formatting_can_override_and_restore_the_whole_label_style() {
    let mut h = harness("AB");
    let shape = h.state().shape;
    h.state_mut()
        .doc
        .update_canvas_object("Whole label bold", shape, |object| {
            object.label_style = Some(std::sync::Arc::new(thinoda_core::label_style::LabelStyle {
                bold: true,
                ..Default::default()
            }));
        })
        .unwrap();
    double_click_shape(&mut h);
    set_label_cursor(&mut h, 2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::B);
    h.run_steps(2);
    h.event(egui::Event::Text("x".into()));
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::B);
    h.run_steps(2);
    h.event(egui::Event::Text("y".into()));
    h.run_steps(2);
    let (_, text, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(text, "ABxy");
    let rich = rich.unwrap();
    assert_eq!(rich.runs.len(), 1);
    assert_eq!((rich.runs[0].start, rich.runs[0].end), (2, 3));
    assert_eq!(rich.runs[0].style.bold, Some(false));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn caret_formatting_survives_backspace_and_list_toggles() {
    let mut h = harness("AB");
    let shape = h.state().shape;
    let mut rich = thinoda_core::InlineText::default();
    rich.update_range("AB", 1..2, |style| style.bold = Some(true))
        .unwrap();
    h.state_mut()
        .doc
        .update_canvas_object("Initial formatting", shape, |object| {
            object.label_rich_text = Some(std::sync::Arc::new(rich));
        })
        .unwrap();
    double_click_shape(&mut h);
    set_label_cursor(&mut h, 2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::I);
    h.run_steps(2);
    h.key_press(Key::Backspace);
    h.run_steps(2);
    let bullet = h
        .get_by_role_and_label(Role::Button, "Bulleted list")
        .rect()
        .center();
    click(&mut h, bullet);
    h.run_steps(2);
    h.event(egui::Event::Text("X".into()));
    h.run_steps(2);
    let (_, text, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(text, "AX");
    let rich = rich.unwrap();
    assert_eq!(rich.paragraphs[0].list, thinoda_core::ListKind::Bulleted);
    assert_eq!((rich.runs[0].start, rich.runs[0].end), (1, 2));
    assert_eq!(rich.runs[0].style.bold, Some(true));
    assert_eq!(rich.runs[0].style.italic, Some(true));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn caret_formatting_survives_forward_delete() {
    let mut h = harness("ABC");
    double_click_shape(&mut h);
    set_label_cursor(&mut h, 1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::I);
    h.run_steps(2);
    h.key_press(Key::Delete);
    h.run_steps(2);
    h.event(egui::Event::Text("x".into()));
    h.run_steps(2);
    let (_, text, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(text, "AxC");
    let rich = rich.unwrap();
    assert_eq!((rich.runs[0].start, rich.runs[0].end), (1, 2));
    assert_eq!(rich.runs[0].style.italic, Some(true));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn caret_formatting_expands_new_combining_graphemes() {
    let mut h = harness("a");
    double_click_shape(&mut h);
    set_label_cursor(&mut h, 1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::B);
    h.run_steps(2);
    h.event(egui::Event::Text("\u{301}".into()));
    h.run_steps(2);
    let (_, text, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(text, "a\u{301}");
    let rich = rich.unwrap();
    assert_eq!((rich.runs[0].start, rich.runs[0].end), (0, 2));
    assert_eq!(rich.runs[0].style.bold, Some(true));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn rich_label_backspace_and_delete_keep_graphemes_intact() {
    for (original, cursor, key, expected) in [
        ("a\u{301}b", 2, Key::Backspace, "b"),
        ("a\u{301}b", 0, Key::Delete, "b"),
        ("a🇩🇪b", 3, Key::Backspace, "ab"),
        ("a🇩🇪b", 1, Key::Delete, "ab"),
        ("a👩\u{200d}💻b", 4, Key::Backspace, "ab"),
        ("a👩\u{200d}💻b", 2, Key::Delete, "ab"),
    ] {
        let mut h = harness(original);
        h.ctx
            .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
        let shape = h.state().shape;
        let mut rich = thinoda_core::InlineText::default();
        let last = original.chars().count();
        rich.update_range(original, last - 1..last, |style| style.bold = Some(true))
            .unwrap();
        h.state_mut()
            .doc
            .update_canvas_object("Set formatting", shape, |object| {
                object.label_rich_text = Some(std::sync::Arc::new(rich.clone()));
            })
            .unwrap();
        double_click_shape(&mut h);
        set_label_cursor(&mut h, cursor);
        h.key_press(key);
        h.run_steps(2);
        let (_, text, changed) = h
            .state()
            .view
            .validated_board_rich_label_draft(&h.state().doc)
            .unwrap()
            .unwrap();
        assert_eq!(text, expected, "key={key:?}");
        let changed = changed.unwrap();
        assert_eq!(changed.runs[0].end as usize, expected.chars().count());
        h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
        h.run_steps(2);
        let (_, text, restored) = h
            .state()
            .view
            .validated_board_rich_label_draft(&h.state().doc)
            .unwrap()
            .unwrap();
        assert_eq!(text, original);
        assert_eq!(restored, Some(rich.clone()));
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn rich_label_delete_expands_partial_grapheme_selection() {
    let mut h = harness("a👩\u{200d}💻b");
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    double_click_shape(&mut h);
    set_label_selection(&mut h, 2, 1);
    h.key_press(Key::Delete);
    h.run_steps(2);
    assert_eq!(h.state().view.board_label_draft().unwrap().2, "ab");
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    assert_eq!(
        h.state().view.board_label_draft().unwrap().2,
        "a👩\u{200d}💻b"
    );
    let cursor = egui::TextEdit::load_state(&h.ctx, MapCanvas::board_label_editor_id())
        .unwrap()
        .cursor
        .char_range()
        .unwrap();
    assert_eq!((cursor.primary.index.0, cursor.secondary.index.0), (2, 1));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn rich_label_modified_delete_expands_partial_grapheme_selection() {
    for (modifiers, key) in [
        (Modifiers::CTRL, Key::Backspace),
        (Modifiers::CTRL, Key::Delete),
        (Modifiers::ALT, Key::Backspace),
        (Modifiers::ALT, Key::Delete),
        (Modifiers::COMMAND, Key::Backspace),
        (Modifiers::COMMAND, Key::Delete),
    ] {
        let mut h = harness("a👩\u{200d}💻b");
        h.ctx
            .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
        let shape = h.state().shape;
        let mut rich = thinoda_core::InlineText::default();
        rich.update_range("a👩\u{200d}💻b", 4..5, |style| style.bold = Some(true))
            .unwrap();
        h.state_mut()
            .doc
            .update_canvas_object("Set formatting", shape, |object| {
                object.label_rich_text = Some(std::sync::Arc::new(rich.clone()));
            })
            .unwrap();
        double_click_shape(&mut h);
        set_label_selection(&mut h, 2, 1);
        h.key_press_modifiers(modifiers, key);
        h.run_steps(2);
        assert_eq!(
            h.state().view.board_label_draft().unwrap().2,
            "ab",
            "{modifiers:?} {key:?}"
        );
        let (_, _, changed) = h
            .state()
            .view
            .validated_board_rich_label_draft(&h.state().doc)
            .unwrap()
            .unwrap();
        let changed = changed.unwrap();
        assert_eq!((changed.runs[0].start, changed.runs[0].end), (1, 2));
        assert_eq!(changed.runs[0].style.bold, Some(true));
        h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
        h.run_steps(2);
        let (_, text, restored) = h
            .state()
            .view
            .validated_board_rich_label_draft(&h.state().doc)
            .unwrap()
            .unwrap();
        assert_eq!(text, "a👩\u{200d}💻b");
        assert_eq!(restored, Some(rich));
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn rich_label_modified_delete_keeps_word_deletion_at_collapsed_caret() {
    let mut h = harness("hello world");
    double_click_shape(&mut h);
    set_label_cursor(&mut h, 11);
    h.key_press_modifiers(Modifiers::CTRL, Key::Backspace);
    h.run_steps(2);
    assert_eq!(h.state().view.board_label_draft().unwrap().2, "hello ");
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn rich_label_modified_delete_at_interior_caret_keeps_graphemes_intact() {
    for (key, expected) in [(Key::Backspace, "b"), (Key::Delete, "a")] {
        let mut h = harness("a👩\u{200d}💻b");
        h.ctx
            .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
        double_click_shape(&mut h);
        set_label_cursor(&mut h, 2);
        h.key_press_modifiers(Modifiers::CTRL, key);
        h.run_steps(2);
        assert_eq!(
            h.state().view.board_label_draft().unwrap().2,
            expected,
            "{key:?}"
        );
        h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
        h.run_steps(2);
        assert_eq!(
            h.state().view.board_label_draft().unwrap().2,
            "a👩\u{200d}💻b"
        );
        let range = egui::TextEdit::load_state(&h.ctx, MapCanvas::board_label_editor_id())
            .unwrap()
            .cursor
            .char_range()
            .unwrap();
        assert_eq!((range.primary.index.0, range.secondary.index.0), (2, 2));
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn rich_label_modified_delete_at_interior_caret_keeps_typing_style() {
    for key in [Key::Backspace, Key::Delete] {
        let mut h = harness("a👩\u{200d}💻b");
        h.ctx
            .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
        double_click_shape(&mut h);
        set_label_cursor(&mut h, 2);
        h.key_press_modifiers(Modifiers::COMMAND, Key::B);
        h.run_steps(2);
        h.key_press_modifiers(Modifiers::CTRL, key);
        h.run_steps(2);
        h.event(egui::Event::Text("x".into()));
        h.run_steps(2);
        let (_, text, rich) = h
            .state()
            .view
            .validated_board_rich_label_draft(&h.state().doc)
            .unwrap()
            .unwrap();
        assert_eq!(text, if key == Key::Backspace { "xb" } else { "ax" });
        let rich = rich.unwrap();
        let start = if key == Key::Backspace { 0 } else { 1 };
        assert_eq!((rich.runs[0].start, rich.runs[0].end), (start, start + 1));
        assert_eq!(rich.runs[0].style.bold, Some(true));
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn rich_label_modified_delete_snaps_interior_caret_in_both_directions() {
    for (text, interior, start, end) in [
        ("p a\u{301} z", 3, 2, 4),
        ("a🇩🇪b", 2, 1, 3),
        ("a👩\u{200d}💻b", 2, 1, 4),
    ] {
        for modifiers in [Modifiers::CTRL, Modifiers::ALT, Modifiers::COMMAND] {
            for key in [Key::Backspace, Key::Delete] {
                let run = |cursor| {
                    let mut h = harness(text);
                    h.ctx
                        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
                    double_click_shape(&mut h);
                    set_label_cursor(&mut h, cursor);
                    h.key_press_modifiers(modifiers, key);
                    h.run_steps(2);
                    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
                    h.state().view.board_label_draft().unwrap().2.to_owned()
                };
                let boundary = if key == Key::Backspace { end } else { start };
                assert_eq!(
                    run(interior),
                    run(boundary),
                    "{text:?} {modifiers:?} {key:?}"
                );
            }
        }
    }
}

#[test]
fn connector_label_deletes_whole_graphemes_and_undoes() {
    let mut h = harness("Shape");
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    let id = add_positioned_connector(&mut h);
    h.state_mut()
        .doc
        .update_canvas_object("Set connector label", id, |object| {
            object.set_label("a🇩🇪b").unwrap();
        })
        .unwrap();
    {
        let state = h.state_mut();
        assert!(state.view.begin_board_label(&state.doc, id));
    }
    h.run_steps(3);
    set_label_cursor(&mut h, 3);
    h.key_press(Key::Backspace);
    h.run_steps(2);
    assert_eq!(h.state().view.board_label_draft().unwrap().2, "ab");
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    assert_eq!(h.state().view.board_label_draft().unwrap().2, "a🇩🇪b");
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn connector_label_modified_delete_expands_partial_selection() {
    let mut h = harness("Shape");
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    let id = add_positioned_connector(&mut h);
    h.state_mut()
        .doc
        .update_canvas_object("Set connector label", id, |object| {
            object.set_label("a🇩🇪b").unwrap();
        })
        .unwrap();
    {
        let state = h.state_mut();
        assert!(state.view.begin_board_label(&state.doc, id));
    }
    h.run_steps(3);
    set_label_selection(&mut h, 2, 1);
    h.key_press_modifiers(Modifiers::CTRL, Key::Delete);
    h.run_steps(2);
    assert_eq!(h.state().view.board_label_draft().unwrap().2, "ab");
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    assert_eq!(h.state().view.board_label_draft().unwrap().2, "a🇩🇪b");
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn rich_label_replacement_and_cut_expand_partial_graphemes() {
    for mode in 0..3 {
        let mut h = harness("a👩\u{200d}💻b");
        h.ctx
            .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
        double_click_shape(&mut h);
        set_label_selection(&mut h, 2, 1);
        h.event(match mode {
            0 => egui::Event::Text("x".into()),
            1 => egui::Event::Paste("x".into()),
            _ => egui::Event::Cut,
        });
        h.run_steps(1);
        if mode == 2 {
            assert!(
                h.output()
                    .platform_output
                    .commands
                    .contains(&egui::OutputCommand::CopyText("👩\u{200d}💻".into()))
            );
        }
        assert_eq!(
            h.state().view.board_label_draft().unwrap().2,
            if mode == 2 { "ab" } else { "axb" }
        );
        h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
        h.run_steps(2);
        assert_eq!(
            h.state().view.board_label_draft().unwrap().2,
            "a👩\u{200d}💻b"
        );
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn rich_label_insertion_snaps_a_caret_inside_a_grapheme() {
    let mut h = harness("a👩\u{200d}💻b");
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    double_click_shape(&mut h);
    set_label_cursor(&mut h, 2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::B);
    h.run_steps(2);
    h.event(egui::Event::Text("x".into()));
    h.run_steps(2);
    let (_, text, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(text, "a👩\u{200d}💻xb");
    let rich = rich.unwrap();
    assert_eq!((rich.runs[0].start, rich.runs[0].end), (4, 5));
    assert_eq!(rich.runs[0].style.bold, Some(true));
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    assert_eq!(
        h.state().view.board_label_draft().unwrap().2,
        "a👩\u{200d}💻b"
    );
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn rich_label_accessibility_replacement_preserves_graphemes_and_formatting() {
    let mut h = harness("a👩\u{200d}💻b");
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    let shape = h.state().shape;
    let mut rich = thinoda_core::InlineText::default();
    rich.update_range("a👩\u{200d}💻b", 4..5, |style| style.bold = Some(true))
        .unwrap();
    h.state_mut()
        .doc
        .update_canvas_object("Set formatting", shape, |object| {
            object.label_rich_text = Some(std::sync::Arc::new(rich.clone()));
        })
        .unwrap();
    double_click_shape(&mut h);
    set_label_selection(&mut h, 2, 1);
    h.event(accessible_replace("x"));
    h.run_steps(2);
    let (_, text, changed) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(text, "axb");
    let changed = changed.unwrap();
    assert_eq!((changed.runs[0].start, changed.runs[0].end), (2, 3));
    assert_eq!(changed.runs[0].style.bold, Some(true));
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    let (_, text, restored) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(text, "a👩\u{200d}💻b");
    assert_eq!(restored, Some(rich));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn rich_label_accessibility_empty_replacement_deletes_only_selection() {
    for selected in [true, false] {
        let mut h = harness("a👩\u{200d}💻b");
        h.ctx
            .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
        let shape = h.state().shape;
        let mut rich = thinoda_core::InlineText::default();
        rich.update_range("a👩\u{200d}💻b", 4..5, |style| style.bold = Some(true))
            .unwrap();
        h.state_mut()
            .doc
            .update_canvas_object("Set formatting", shape, |object| {
                object.label_rich_text = Some(std::sync::Arc::new(rich.clone()));
            })
            .unwrap();
        double_click_shape(&mut h);
        if selected {
            set_label_selection(&mut h, 2, 1);
        } else {
            set_label_cursor(&mut h, 2);
        }
        h.event(accessible_replace(""));
        h.run_steps(2);
        assert_eq!(
            h.state().view.board_label_draft().unwrap().2,
            if selected { "ab" } else { "a👩\u{200d}💻b" }
        );
        if selected {
            let (_, _, changed) = h
                .state()
                .view
                .validated_board_rich_label_draft(&h.state().doc)
                .unwrap()
                .unwrap();
            let changed = changed.unwrap();
            assert_eq!((changed.runs[0].start, changed.runs[0].end), (1, 2));
            assert_eq!(changed.runs[0].style.bold, Some(true));
            h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
            h.run_steps(2);
            assert_eq!(
                h.state().view.board_label_draft().unwrap().2,
                "a👩\u{200d}💻b"
            );
        } else {
            let range = egui::TextEdit::load_state(&h.ctx, MapCanvas::board_label_editor_id())
                .unwrap()
                .cursor
                .char_range()
                .unwrap();
            assert_eq!((range.primary.index.0, range.secondary.index.0), (2, 2));
        }
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn rich_label_accessibility_and_keyboard_input_follow_event_order() {
    let mut h = harness("aaa");
    let shape = h.state().shape;
    let mut rich = thinoda_core::InlineText::default();
    rich.update_range("aaa", 1..2, |style| style.bold = Some(true))
        .unwrap();
    h.state_mut()
        .doc
        .update_canvas_object("Set formatting", shape, |object| {
            object.label_rich_text = Some(std::sync::Arc::new(rich.clone()));
        })
        .unwrap();
    double_click_shape(&mut h);
    set_label_cursor(&mut h, 1);
    h.input_mut()
        .events
        .extend([accessible_replace("a"), egui::Event::Text("x".into())]);
    h.run_steps(2);
    let (_, text, changed) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(text, "aaxaa");
    let changed = changed.unwrap();
    assert_eq!((changed.runs[0].start, changed.runs[0].end), (3, 4));
    assert_eq!(changed.runs[0].style.bold, Some(true));
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    let (_, text, restored) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(text, "aaa");
    assert_eq!(restored, Some(rich));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn rich_label_accessibility_replacement_snaps_pending_caret_style() {
    let mut h = harness("a👩\u{200d}💻b");
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    double_click_shape(&mut h);
    set_label_cursor(&mut h, 2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::B);
    h.run_steps(2);
    h.event(accessible_replace("x"));
    h.run_steps(2);
    let (_, text, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(text, "a👩\u{200d}💻xb");
    let rich = rich.unwrap();
    assert_eq!((rich.runs[0].start, rich.runs[0].end), (4, 5));
    assert_eq!(rich.runs[0].style.bold, Some(true));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn connector_accessibility_replacement_preserves_graphemes_and_undo() {
    let mut h = harness("Shape");
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    let id = add_positioned_connector(&mut h);
    h.state_mut()
        .doc
        .update_canvas_object("Set connector label", id, |object| {
            object.set_label("a🇩🇪b").unwrap();
        })
        .unwrap();
    {
        let state = h.state_mut();
        assert!(state.view.begin_board_label(&state.doc, id));
    }
    h.run_steps(3);
    set_label_selection(&mut h, 2, 1);
    h.event(accessible_replace("x"));
    h.run_steps(2);
    assert_eq!(h.state().view.board_label_draft().unwrap().2, "axb");
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    assert_eq!(h.state().view.board_label_draft().unwrap().2, "a🇩🇪b");
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn rich_label_accessibility_set_value_matches_select_all_replacement() {
    let run = |accessible, replacement| {
        let mut h = harness("aaa");
        let shape = h.state().shape;
        let mut rich = thinoda_core::InlineText::default();
        rich.update_range("aaa", 1..2, |style| style.bold = Some(true))
            .unwrap();
        h.state_mut()
            .doc
            .update_canvas_object("Set formatting", shape, |object| {
                object.label_rich_text = Some(std::sync::Arc::new(rich.clone()));
            })
            .unwrap();
        double_click_shape(&mut h);
        if accessible {
            h.event(accessible_set_value(replacement));
        } else {
            set_label_selection(&mut h, 0, 3);
            h.event(egui::Event::Text(replacement.into()));
        }
        h.run_steps(2);
        let (_, text, changed) = h
            .state()
            .view
            .validated_board_rich_label_draft(&h.state().doc)
            .unwrap()
            .unwrap();
        assert_eq!(text, replacement);
        let changed = changed.clone();
        h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
        h.run_steps(2);
        let (_, text, restored) = h
            .state()
            .view
            .validated_board_rich_label_draft(&h.state().doc)
            .unwrap()
            .unwrap();
        assert_eq!(text, "aaa");
        assert_eq!(restored, Some(rich));
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
        changed
    };
    for replacement in ["aaaa", "aaa"] {
        assert_eq!(run(true, replacement), run(false, replacement));
    }
}

#[test]
fn rich_label_accessibility_set_value_works_with_toolbar_focus() {
    let mut h = harness("aaa");
    let shape = h.state().shape;
    let mut rich = thinoda_core::InlineText::default();
    rich.update_range("aaa", 1..2, |style| style.bold = Some(true))
        .unwrap();
    h.state_mut()
        .doc
        .update_canvas_object("Set formatting", shape, |object| {
            object.label_rich_text = Some(std::sync::Arc::new(rich.clone()));
        })
        .unwrap();
    double_click_shape(&mut h);
    h.key_press(Key::F6);
    h.run_steps(2);
    assert!(h.get_by_role_and_label(Role::Button, "Bold").is_focused());
    h.event(accessible_set_value("aaaa"));
    h.run_steps(2);
    let (_, text, changed) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(text, "aaaa");
    assert_eq!(changed, None);
    h.key_press(Key::F6);
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    let (_, text, restored) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(text, "aaa");
    assert_eq!(restored, Some(rich));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn rich_label_accessibility_set_value_follows_keyboard_event_order() {
    for set_first in [true, false] {
        let mut h = harness("aaa");
        double_click_shape(&mut h);
        set_label_cursor(&mut h, 1);
        let value = accessible_set_value("b");
        let text = egui::Event::Text("x".into());
        h.input_mut().events.extend(if set_first {
            vec![value, text]
        } else {
            vec![text, value]
        });
        h.run_steps(2);
        assert_eq!(
            h.state().view.board_label_draft().unwrap().2,
            if set_first { "bx" } else { "b" }
        );
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn rich_label_accessibility_set_value_follows_toolbar_focus_change() {
    for text_before_focus in [false, true] {
        let mut h = harness("aaa");
        double_click_shape(&mut h);
        h.key_press(Key::F6);
        h.run_steps(2);
        assert!(h.get_by_role_and_label(Role::Button, "Bold").is_focused());
        h.input_mut().events.push(accessible_set_value("b"));
        if text_before_focus {
            h.input_mut()
                .events
                .push(egui::Event::Text("ignored".into()));
        }
        h.input_mut().events.extend([
            egui::Event::Key {
                key: Key::F6,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
            egui::Event::Text("x".into()),
        ]);
        h.run_steps(2);
        assert_eq!(
            h.state().view.board_label_draft().unwrap().2,
            "bx",
            "text_before_focus={text_before_focus}"
        );
        assert!(h.get_by_label("Board object label").is_focused());
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn rich_label_accessibility_set_value_precedes_toolbar_formatting() {
    let mut h = harness("aaa");
    double_click_shape(&mut h);
    h.key_press(Key::F6);
    h.run_steps(2);
    assert!(h.get_by_role_and_label(Role::Button, "Bold").is_focused());
    h.input_mut().events.extend([
        accessible_set_value("b"),
        egui::Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        },
        egui::Event::Text("x".into()),
    ]);
    h.run_steps(2);
    let (_, text, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(text, "bx");
    let rich = rich.unwrap();
    assert_eq!((rich.runs[0].start, rich.runs[0].end), (1, 2));
    assert_eq!(rich.runs[0].style.bold, Some(true));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn rich_label_accessibility_set_empty_value_can_be_undone() {
    let mut h = harness("aaa");
    let shape = h.state().shape;
    let mut rich = thinoda_core::InlineText::default();
    rich.update_range("aaa", 1..2, |style| style.bold = Some(true))
        .unwrap();
    h.state_mut()
        .doc
        .update_canvas_object("Set formatting", shape, |object| {
            object.label_rich_text = Some(std::sync::Arc::new(rich.clone()));
        })
        .unwrap();
    double_click_shape(&mut h);
    h.event(accessible_set_value(""));
    h.run_steps(2);
    assert_eq!(h.state().view.board_label_draft().unwrap().2, "");
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    let (_, text, restored) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(text, "aaa");
    assert_eq!(restored, Some(rich));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn rich_label_line_break_replaces_whole_selected_grapheme() {
    let mut h = harness("a👩\u{200d}💻b");
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    double_click_shape(&mut h);
    set_label_selection(&mut h, 2, 1);
    h.key_press_modifiers(Modifiers::SHIFT, Key::Enter);
    h.run_steps(2);
    assert_eq!(h.state().view.board_label_draft().unwrap().2, "a\nb");
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    assert_eq!(
        h.state().view.board_label_draft().unwrap().2,
        "a👩\u{200d}💻b"
    );
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn rich_label_new_ime_composition_respects_graphemes() {
    for (selected, preedit) in [(false, true), (true, true), (false, false), (true, false)] {
        let mut h = harness("a👩\u{200d}💻b");
        h.ctx
            .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
        double_click_shape(&mut h);
        if selected {
            set_label_selection(&mut h, 2, 1);
        } else {
            set_label_cursor(&mut h, 2);
        }
        if preedit {
            h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
                text: "x".into(),
                active_range_chars: None,
            }));
            h.run_steps(1);
        }
        h.event(egui::Event::Ime(egui::ImeEvent::Commit("x".into())));
        h.run_steps(2);
        assert_eq!(
            h.state().view.board_label_draft().unwrap().2,
            if selected {
                "axb"
            } else {
                "a👩\u{200d}💻xb"
            }
        );
        h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
        h.run_steps(2);
        assert_eq!(
            h.state().view.board_label_draft().unwrap().2,
            "a👩\u{200d}💻b"
        );
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn rich_label_empty_initial_ime_preserves_partial_selection() {
    for preedit in [true, false] {
        let mut h = harness("a👩\u{200d}💻b");
        h.ctx
            .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
        double_click_shape(&mut h);
        set_label_selection(&mut h, 2, 1);
        h.event(egui::Event::Ime(if preedit {
            egui::ImeEvent::Preedit {
                text: String::new(),
                active_range_chars: None,
            }
        } else {
            egui::ImeEvent::Commit(String::new())
        }));
        h.run_steps(1);
        assert_eq!(
            h.state().view.board_label_draft().unwrap().2,
            "a👩\u{200d}💻b"
        );
        let range = egui::TextEdit::load_state(&h.ctx, MapCanvas::board_label_editor_id())
            .unwrap()
            .cursor
            .char_range()
            .unwrap();
        let range = range.as_sorted_char_range();
        assert_eq!((range.start.0, range.end.0), (1, 2));
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn rich_label_active_ime_update_preserves_composition_range() {
    let mut h = harness("ab");
    double_click_shape(&mut h);
    set_label_cursor(&mut h, 1);
    for (preedit, expected) in [("\u{301}", "a\u{301}b"), ("x", "axb")] {
        h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
            text: preedit.into(),
            active_range_chars: None,
        }));
        h.run_steps(1);
        assert_eq!(h.state().view.board_label_draft().unwrap().2, expected);
    }
    h.event(egui::Event::Ime(egui::ImeEvent::Commit("y".into())));
    h.run_steps(2);
    assert_eq!(h.state().view.board_label_draft().unwrap().2, "ayb");
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    assert_eq!(h.state().view.board_label_draft().unwrap().2, "ab");
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn rich_label_line_break_and_ime_keep_snapped_caret_style() {
    for mode in 0..3 {
        let mut h = harness("a👩\u{200d}💻b");
        h.ctx
            .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
        double_click_shape(&mut h);
        set_label_cursor(&mut h, 2);
        h.key_press_modifiers(Modifiers::COMMAND, Key::B);
        h.run_steps(2);
        match mode {
            0 => h.key_press_modifiers(Modifiers::SHIFT, Key::Enter),
            1 => h.event(egui::Event::Ime(egui::ImeEvent::Commit("x".into()))),
            _ => {
                h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
                    text: "x".into(),
                    active_range_chars: None,
                }));
                h.run_steps(1);
                h.event(egui::Event::Ime(egui::ImeEvent::Commit("x".into())));
            }
        }
        h.run_steps(2);
        let (_, text, rich) = h
            .state()
            .view
            .validated_board_rich_label_draft(&h.state().doc)
            .unwrap()
            .unwrap();
        assert_eq!(
            text,
            if mode == 0 {
                "a👩\u{200d}💻\nb"
            } else {
                "a👩\u{200d}💻xb"
            }
        );
        let rich = rich.unwrap();
        assert_eq!((rich.runs[0].start, rich.runs[0].end), (4, 5));
        assert_eq!(rich.runs[0].style.bold, Some(true));
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn plain_label_processes_text_and_undo_in_event_order() {
    let mut h = harness("a");
    double_click_shape(&mut h);
    set_label_cursor(&mut h, 1);
    h.event(egui::Event::Text("b".into()));
    h.event(egui::Event::Key {
        key: Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    h.run_steps(2);
    assert_eq!(h.state().view.board_label_draft().unwrap().2, "a");
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn long_plain_label_keeps_egui_undo_when_it_becomes_short() {
    let original = "a".repeat(thinoda_core::MAX_INLINE_TEXT_CHARS);
    let mut h = harness(&original);
    double_click_shape(&mut h);
    set_label_cursor(&mut h, original.len());
    h.event(egui::Event::Text("x".into()));
    h.run_steps(70);
    set_label_selection(&mut h, 0, original.len() + 1);
    h.event(egui::Event::Text("b".into()));
    h.run_steps(2);
    assert_eq!(h.state().view.board_label_draft().unwrap().2, "b");
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    assert_eq!(
        h.state().view.board_label_draft().unwrap().2,
        format!("{original}x")
    );
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    h.run_steps(2);
    assert_eq!(h.state().view.board_label_draft().unwrap().2, "b");
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    assert_eq!(
        h.state().view.board_label_draft().unwrap().2,
        format!("{original}x")
    );
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    h.run_steps(2);
    set_label_selection(&mut h, 0, 1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    assert_eq!(
        h.state()
            .view
            .board_label_draft()
            .unwrap()
            .2
            .chars()
            .count(),
        original.len() + 1
    );
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    h.run_steps(2);
    set_label_selection(&mut h, 0, 1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::B);
    h.run_steps(2);
    assert!(h.state().view.board_rich_label_draft().unwrap().3.is_none());
    assert!(
        h.state().errors.iter().any(|error| error
            .to_string()
            .contains("plain-text undo history is active")),
        "{:?}",
        h.state().errors
    );
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    assert_eq!(
        h.state()
            .view
            .board_label_draft()
            .unwrap()
            .2
            .chars()
            .count(),
        original.len() + 1
    );
}

#[test]
fn local_undo_recovers_a_rejected_rich_label_insertion() {
    let original = "a".repeat(thinoda_core::MAX_INLINE_TEXT_CHARS);
    let mut h = harness(&original);
    double_click_shape(&mut h);
    set_label_cursor(&mut h, original.len());
    h.key_press_modifiers(Modifiers::COMMAND, Key::B);
    h.run_steps(2);
    h.event(egui::Event::Text("x".into()));
    h.run_steps(2);
    assert!(
        h.state()
            .view
            .validated_board_rich_label_draft(&h.state().doc)
            .is_err()
    );
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    let (_, text, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(text, original);
    assert!(rich.is_none());
    h.key_press(Key::Backspace);
    h.run_steps(2);
    h.event(egui::Event::Text("b".into()));
    h.run_steps(2);
    let (_, text, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert!(text.ends_with('b'));
    assert_eq!(rich.unwrap().runs[0].style.bold, Some(true));
}

#[test]
fn caret_formatting_limit_failure_cannot_be_committed_without_the_pending_style() {
    let original = "a".repeat(thinoda_core::rich_text::MAX_INLINE_TEXT_CHARS);
    let mut h = harness(&original);
    double_click_shape(&mut h);
    set_label_cursor(&mut h, original.len());
    h.key_press_modifiers(Modifiers::COMMAND, Key::B);
    h.run_steps(2);
    h.event(egui::Event::Text("x".into()));
    h.run_steps(2);
    assert!(
        h.state()
            .errors
            .iter()
            .any(|error| error.contains("formatted text exceeds"))
    );
    assert_eq!(
        h.state().view.board_label_draft().unwrap().2,
        format!("{original}x")
    );
    assert!(h.state().view.board_rich_label_draft().unwrap().3.is_none());
    assert!(
        h.state()
            .view
            .validated_board_rich_label_draft(&h.state().doc)
            .is_err()
    );
    assert!(h.get_by_label("Board object label").is_focused());
    let position = h.state().view.canvas_rect().center() + h.state().view.offset;
    click(&mut h, position);
    let state = h.state_mut();
    assert!(
        state
            .view
            .validated_board_rich_label_draft(&state.doc)
            .is_err()
    );
    assert!(state.view.try_commit_board_label(&mut state.doc).is_err());
    assert_eq!(
        state.view.board_label_draft().unwrap().2,
        format!("{original}x")
    );
    assert_eq!(
        state
            .doc
            .sheet()
            .canvas
            .object(state.shape)
            .unwrap()
            .label(),
        original
    );

    set_label_cursor(&mut h, original.len() + 1);
    h.key_press(Key::Backspace);
    h.run_steps(2);
    h.key_press(Key::Backspace);
    h.run_steps(2);
    h.event(egui::Event::Text("b".into()));
    h.run_steps(2);
    let (_, text, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(text, format!("{}b", "a".repeat(original.len() - 1)));
    let rich = rich.unwrap();
    assert_eq!(
        (rich.runs[0].start as usize, rich.runs[0].end as usize),
        (original.len() - 1, original.len())
    );
    assert_eq!(rich.runs[0].style.bold, Some(true));
}

#[test]
fn caret_formatting_run_limit_failure_preserves_the_uncommitted_style() {
    use thinoda_core::rich_text::{InlineTextRun, InlineTextStyle, MAX_INLINE_TEXT_RUNS};

    let original = "a".repeat(MAX_INLINE_TEXT_RUNS * 2);
    let mut h = harness(&original);
    let shape = h.state().shape;
    let rich = thinoda_core::InlineText {
        runs: (0..MAX_INLINE_TEXT_RUNS)
            .map(|index| InlineTextRun {
                start: (index * 2) as u32,
                end: (index * 2 + 2) as u32,
                style: if index % 2 == 0 {
                    InlineTextStyle {
                        bold: Some(true),
                        ..Default::default()
                    }
                } else {
                    InlineTextStyle {
                        italic: Some(true),
                        ..Default::default()
                    }
                },
            })
            .collect(),
        ..Default::default()
    };
    h.state_mut()
        .doc
        .update_canvas_object("Initial formatting", shape, |object| {
            object.label_rich_text = Some(std::sync::Arc::new(rich));
        })
        .unwrap();
    double_click_shape(&mut h);
    set_label_cursor(&mut h, 1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::U);
    h.run_steps(2);
    h.event(egui::Event::Text("x".into()));
    h.run_steps(2);
    assert!(
        h.state()
            .errors
            .iter()
            .any(|error| error.contains("style runs"))
    );
    h.run_steps(40);
    let position = h.state().view.canvas_rect().center() + h.state().view.offset;
    h.input_mut().events.extend([
        egui::Event::PointerMoved(position),
        egui::Event::PointerButton {
            pos: position,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos: position,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
        egui::Event::Text("z".into()),
    ]);
    h.run_steps(2);
    let state = h.state_mut();
    assert!(
        state
            .view
            .validated_board_rich_label_draft(&state.doc)
            .is_err()
    );
    assert!(state.view.try_commit_board_label(&mut state.doc).is_err());
    assert!(state.view.board_label_draft().unwrap().2.contains('x'));
    assert!(state.view.board_label_draft().unwrap().2.contains('z'));
    assert_eq!(
        state.doc.sheet().canvas.object(shape).unwrap().label(),
        original
    );
}

#[test]
fn moving_the_caret_clears_pending_typing_format_even_with_batched_input() {
    let mut h = harness("AB");
    double_click_shape(&mut h);
    set_label_cursor(&mut h, 2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::I);
    h.run_steps(2);
    h.input_mut().events.extend([
        egui::Event::Key {
            key: Key::ArrowLeft,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        },
        egui::Event::Text("x".into()),
    ]);
    h.run_steps(2);
    let (_, text, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(text, "AxB");
    assert!(rich.is_none());
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn editor_click_resets_pending_style_before_batched_text_input() {
    let mut h = harness("abc");
    double_click_shape(&mut h);
    set_label_cursor(&mut h, 3);
    h.key_press_modifiers(Modifiers::COMMAND, Key::I);
    h.run_steps(2);
    let editor = h.get_by_label("Board object label").rect();
    let position = egui::pos2(editor.left() + 2.0, editor.center().y);
    h.input_mut().events.extend([
        egui::Event::PointerMoved(position),
        egui::Event::PointerButton {
            pos: position,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos: position,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
        egui::Event::Text("x".into()),
    ]);
    h.run_steps(2);
    let (_, text, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(text, "xabc");
    assert!(rich.is_none());
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn editor_click_keeps_preceding_batched_text_in_its_pending_style() {
    let mut h = harness("AB");
    double_click_shape(&mut h);
    set_label_cursor(&mut h, 2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::I);
    h.run_steps(2);
    let editor = h.get_by_label("Board object label").rect();
    let position = egui::pos2(editor.left() + 2.0, editor.center().y);
    h.input_mut().events.extend([
        egui::Event::Text("x".into()),
        egui::Event::PointerMoved(position),
        egui::Event::PointerButton {
            pos: position,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos: position,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
        egui::Event::Text("y".into()),
    ]);
    h.run_steps(2);
    let (_, text, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(text, "yABx");
    let rich = rich.unwrap();
    assert_eq!(rich.runs.len(), 1);
    assert_eq!((rich.runs[0].start, rich.runs[0].end), (3, 4));
    assert_eq!(rich.runs[0].style.italic, Some(true));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn editor_click_precedes_batched_format_shortcut_and_text() {
    let mut h = harness("AB");
    double_click_shape(&mut h);
    set_label_cursor(&mut h, 2);
    let editor = h.get_by_label("Board object label").rect();
    let position = egui::pos2(editor.left() + 2.0, editor.center().y);
    h.input_mut().events.extend([
        egui::Event::PointerMoved(position),
        egui::Event::PointerButton {
            pos: position,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos: position,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
        egui::Event::Key {
            key: Key::I,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        },
        egui::Event::Text("x".into()),
    ]);
    h.run_steps(2);
    let (_, text, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(text, "xAB");
    let rich = rich.unwrap();
    assert_eq!((rich.runs[0].start, rich.runs[0].end), (0, 1));
    assert_eq!(rich.runs[0].style.italic, Some(true));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn editor_click_orders_ime_input_before_and_after_the_click() {
    for ime_before_click in [false, true] {
        let mut h = harness("aaa");
        let shape = h.state().shape;
        let mut rich = thinoda_core::InlineText::default();
        rich.update_range("aaa", 1..2, |style| style.bold = Some(true))
            .unwrap();
        h.state_mut()
            .doc
            .update_canvas_object("Set formatting", shape, |object| {
                object.label_rich_text = Some(std::sync::Arc::new(rich));
            })
            .unwrap();
        double_click_shape(&mut h);
        set_label_cursor(&mut h, 1);
        let editor = h.get_by_label("Board object label").rect();
        let position = egui::pos2(editor.left() + 2.0, editor.center().y);
        let ime = [
            egui::Event::Ime(egui::ImeEvent::Preedit {
                text: "a".into(),
                active_range_chars: None,
            }),
            egui::Event::Ime(egui::ImeEvent::Commit("a".into())),
        ];
        let click = [
            egui::Event::PointerMoved(position),
            egui::Event::PointerButton {
                pos: position,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos: position,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ];
        if ime_before_click {
            h.input_mut().events.extend(ime);
            h.input_mut().events.extend(click);
            h.input_mut().events.push(egui::Event::Text("x".into()));
        } else {
            h.input_mut().events.extend(click);
            h.input_mut().events.extend(ime);
        }
        h.run_steps(2);
        let (_, text, changed) = h
            .state()
            .view
            .validated_board_rich_label_draft(&h.state().doc)
            .unwrap()
            .unwrap();
        assert_eq!(text, if ime_before_click { "xaaaa" } else { "aaaa" });
        let changed = changed.unwrap();
        assert_eq!(
            (changed.runs[0].start, changed.runs[0].end),
            if ime_before_click { (3, 4) } else { (2, 3) }
        );
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn formatting_focus_follows_ime_event_order() {
    for during_composition in [false, true] {
        let mut h = harness("aaa");
        let shape = h.state().shape;
        let mut rich = thinoda_core::InlineText::default();
        rich.update_range("aaa", 1..2, |style| style.bold = Some(true))
            .unwrap();
        h.state_mut()
            .doc
            .update_canvas_object("Set formatting", shape, |object| {
                object.label_rich_text = Some(std::sync::Arc::new(rich));
            })
            .unwrap();
        double_click_shape(&mut h);
        set_label_cursor(&mut h, 1);
        let preedit = egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "a".into(),
            active_range_chars: None,
        });
        let commit = egui::Event::Ime(egui::ImeEvent::Commit("a".into()));
        let f6 = egui::Event::Key {
            key: Key::F6,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        h.input_mut().events.extend(if during_composition {
            vec![preedit, f6, commit]
        } else {
            vec![preedit, commit, f6, egui::Event::Text("x".into())]
        });
        h.run_steps(2);
        let (_, text, changed) = h
            .state()
            .view
            .validated_board_rich_label_draft(&h.state().doc)
            .unwrap()
            .unwrap();
        assert_eq!(text, "aaaa");
        let changed = changed.unwrap();
        assert_eq!((changed.runs[0].start, changed.runs[0].end), (2, 3));
        if during_composition {
            assert!(h.get_by_label("Board object label").is_focused());
        } else {
            assert!(h.get_by_role_and_label(Role::Button, "Bold").is_focused());
        }
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn formatting_focus_before_ime_keeps_input_out_of_the_label() {
    let mut h = harness("aaa");
    double_click_shape(&mut h);
    set_label_cursor(&mut h, 1);
    h.input_mut().events.extend([
        egui::Event::Key {
            key: Key::F6,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        },
        egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "x".into(),
            active_range_chars: None,
        }),
        egui::Event::Ime(egui::ImeEvent::Commit("x".into())),
    ]);
    h.run_steps(2);
    assert_eq!(h.state().view.board_label_draft().unwrap().2, "aaa");
    assert!(h.get_by_role_and_label(Role::Button, "Bold").is_focused());
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn formatting_focus_follows_prior_text_in_same_frame() {
    let mut h = harness("aaa");
    double_click_shape(&mut h);
    set_label_cursor(&mut h, 1);
    h.input_mut().events.extend([
        egui::Event::Text("x".into()),
        egui::Event::Key {
            key: Key::F6,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        },
    ]);
    h.run_steps(2);
    assert_eq!(h.state().view.board_label_draft().unwrap().2, "axaa");
    assert!(h.get_by_role_and_label(Role::Button, "Bold").is_focused());
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn formatting_focus_precedes_later_text_in_same_frame() {
    let mut h = harness("aaa");
    double_click_shape(&mut h);
    set_label_cursor(&mut h, 1);
    h.input_mut().events.extend([
        egui::Event::Key {
            key: Key::F6,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        },
        egui::Event::Text("x".into()),
    ]);
    h.run_steps(2);
    assert_eq!(h.state().view.board_label_draft().unwrap().2, "aaa");
    assert!(h.get_by_role_and_label(Role::Button, "Bold").is_focused());
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn formatting_focus_routes_same_frame_toolbar_activation_before_text() {
    let mut h = harness("aaa");
    double_click_shape(&mut h);
    set_label_cursor(&mut h, 1);
    let key = |key| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    h.input_mut()
        .events
        .extend([key(Key::F6), key(Key::Enter), egui::Event::Text("x".into())]);
    h.run_steps(2);
    let (_, text, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(text, "axaa");
    let rich = rich.unwrap();
    assert_eq!((rich.runs[0].start, rich.runs[0].end), (1, 2));
    assert_eq!(rich.runs[0].style.bold, Some(true));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn outside_click_commits_preceding_rich_ime_input_in_order() {
    let mut h = harness("aaa");
    let shape = h.state().shape;
    let mut rich = thinoda_core::InlineText::default();
    rich.update_range("aaa", 1..2, |style| style.bold = Some(true))
        .unwrap();
    h.state_mut()
        .doc
        .update_canvas_object("Set formatting", shape, |object| {
            object.label_rich_text = Some(std::sync::Arc::new(rich));
        })
        .unwrap();
    double_click_shape(&mut h);
    set_label_cursor(&mut h, 1);
    let outside = h.state().view.canvas_rect().right_bottom() - vec2(20.0, 20.0);
    h.input_mut().events.extend([
        egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "a".into(),
            active_range_chars: None,
        }),
        egui::Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        },
        egui::Event::Ime(egui::ImeEvent::Commit("a".into())),
        egui::Event::PointerMoved(outside),
        egui::Event::PointerButton {
            pos: outside,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos: outside,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    ]);
    h.run_steps(3);
    let object = h.state().doc.sheet().canvas.object(shape).unwrap();
    assert_eq!(object.label(), "aaaa");
    let rich = object.label_rich_text.as_ref().unwrap();
    assert_eq!((rich.runs[0].start, rich.runs[0].end), (2, 3));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn caret_formatting_survives_ime_composition() {
    let mut h = harness("");
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    double_click_shape(&mut h);
    h.key_press_modifiers(Modifiers::COMMAND, Key::B);
    h.run_steps(2);
    h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
        text: "composition".into(),
        active_range_chars: None,
    }));
    h.run_steps(2);
    h.event(egui::Event::Ime(egui::ImeEvent::Commit("日本".into())));
    h.run_steps(2);
    let (_, text, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(text, "日本");
    let rich = rich.unwrap();
    assert_eq!((rich.runs[0].start, rich.runs[0].end), (0, 2));
    assert_eq!(rich.runs[0].style.bold, Some(true));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn caret_formatting_survives_ime_surrounding_deletion() {
    let mut h = harness("aaa");
    double_click_shape(&mut h);
    set_label_cursor(&mut h, 1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::B);
    h.run_steps(2);
    h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
        text: "n".into(),
        active_range_chars: None,
    }));
    h.run_steps(1);
    h.event(egui::Event::Ime(egui::ImeEvent::DeleteSurrounding {
        before_chars: 0,
        after_chars: 1,
    }));
    h.run_steps(1);
    h.event(egui::Event::Ime(egui::ImeEvent::Commit("x".into())));
    h.run_steps(1);
    let (_, text, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(text, "axa");
    let rich = rich.unwrap();
    assert_eq!((rich.runs[0].start, rich.runs[0].end), (1, 2));
    assert_eq!(rich.runs[0].style.bold, Some(true));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn caret_formatting_applies_to_pasted_list_paragraphs() {
    let mut h = harness("");
    double_click_shape(&mut h);
    h.key_press_modifiers(Modifiers::COMMAND, Key::B);
    h.run_steps(2);
    let bullet = h
        .get_by_role_and_label(Role::Button, "Bulleted list")
        .rect()
        .center();
    click(&mut h, bullet);
    h.run_steps(2);
    h.event(egui::Event::Paste("one\ntwo".into()));
    h.run_steps(2);
    let (_, text, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(text, "one\ntwo");
    let rich = rich.unwrap();
    assert_eq!((rich.runs[0].start, rich.runs[0].end), (0, 7));
    assert_eq!(rich.runs[0].style.bold, Some(true));
    assert_eq!(
        rich.paragraphs
            .iter()
            .map(|paragraph| (paragraph.index, paragraph.list))
            .collect::<Vec<_>>(),
        vec![
            (0, thinoda_core::ListKind::Bulleted),
            (1, thinoda_core::ListKind::Bulleted),
        ]
    );
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

fn click_toolbar_button(h: &mut Harness<'_, State>, label: &str) {
    let position = h.get_by_role_and_label(Role::Button, label).rect().center();
    click(h, position);
    h.run_steps(2);
}

fn replace_toolbar_value(h: &mut Harness<'_, State>, label: &str, value: &str) {
    let position = h.get_by_label(label).rect().center();
    click(h, position);
    h.run_steps(2);
    assert!(h.get_by_label(label).is_focused());
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(1);
    h.event(egui::Event::Text(value.into()));
    h.run_steps(2);
}

fn enter_direct_typography(h: &mut Harness<'_, State>, size: &str, color: &str) {
    click_toolbar_button(h, "Font Size / Text Color / Font Family");
    click_toolbar_button(h, "Font Size");
    replace_toolbar_value(h, "Font Size · Value", size);
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert!(h.get_by_label("Board object label").is_focused());
    click_toolbar_button(h, "Label formatting (F6)");
    click_toolbar_button(h, "Text Color");
    click_toolbar_button(h, "Custom color");
    replace_toolbar_value(h, "Text Color · Value", color);
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert!(h.get_by_label("Board object label").is_focused());
}

#[test]
fn direct_size_and_color_are_atomic_for_selected_and_typed_text() {
    for connector in [false, true] {
        for caret in [false, true] {
            let mut h = harness("Label");
            let id = if connector {
                begin_connector_label(&mut h, "Label")
            } else {
                double_click_shape(&mut h);
                h.state().shape
            };
            if caret {
                set_label_cursor(&mut h, 5);
            }
            let before = h.state().doc.state_id();
            enter_direct_typography(&mut h, "24.5", "#12A4C080");
            assert_eq!(h.state().doc.state_id(), before);
            if caret {
                assert!(!h.state().view.has_board_label_changes());
                h.event(egui::Event::Text("x".into()));
                h.run_steps(2);
            }
            let (_, text, rich) = h
                .state()
                .view
                .validated_board_rich_label_draft(&h.state().doc)
                .unwrap()
                .unwrap();
            let rich = rich.unwrap();
            assert_eq!(text, if caret { "Labelx" } else { "Label" });
            assert_eq!(
                (rich.runs[0].start, rich.runs[0].end),
                if caret { (5, 6) } else { (0, 5) }
            );
            assert_eq!(rich.runs[0].style.font_size, Some(24.5));
            assert_eq!(
                rich.runs[0].style.color,
                Some(thinoda_core::Color::rgba(0x12, 0xA4, 0xC0, 0x80))
            );
            h.key_press(Key::Enter);
            h.run_steps(2);
            assert_eq!(
                h.state()
                    .doc
                    .sheet()
                    .canvas
                    .object(id)
                    .unwrap()
                    .label_rich_text
                    .as_deref(),
                Some(&rich)
            );
            assert!(h.state_mut().doc.undo());
            let object = h.state().doc.sheet().canvas.object(id).unwrap();
            assert_eq!(object.label(), "Label");
            assert!(object.label_rich_text.is_none());
            assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
        }
    }
}

#[test]
fn invalid_direct_typography_stays_in_the_field_until_corrected() {
    let mut h = harness("Label");
    double_click_shape(&mut h);
    let before = h.state().doc.state_id();
    click_toolbar_button(&mut h, "Font Size / Text Color / Font Family");
    click_toolbar_button(&mut h, "Font Size");
    for value in ["NaN", "5.99", "257"] {
        replace_toolbar_value(&mut h, "Font Size · Value", value);
        h.key_press(Key::Enter);
        h.run_steps(2);
        assert!(h.get_by_label("Font Size · Value").is_focused());
        assert!(h.query_by_label("Invalid font size (6–256)").is_some());
        assert_eq!(h.state().doc.state_id(), before);
        assert!(!h.state().view.has_board_label_changes());
    }
    replace_toolbar_value(&mut h, "Font Size · Value", "6");
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert!(h.get_by_label("Board object label").is_focused());
    click_toolbar_button(&mut h, "Label formatting (F6)");
    click_toolbar_button(&mut h, "Text Color");
    click_toolbar_button(&mut h, "Custom color");
    assert!(h.query_by_label("Invalid color").is_none());
    click_toolbar_button(&mut h, "Apply");
    assert!(h.get_by_label("Text Color · Value").is_focused());
    assert!(h.query_by_label("Invalid color").is_some());
    replace_toolbar_value(&mut h, "Text Color · Value", "#12345");
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert!(h.get_by_label("Text Color · Value").is_focused());
    assert!(h.query_by_label("Invalid color").is_some());
    replace_toolbar_value(&mut h, "Text Color · Value", "#abc");
    h.key_press(Key::Enter);
    h.run_steps(2);
    let (_, _, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    let style = &rich.unwrap().runs[0].style;
    assert_eq!(style.font_size, Some(6.0));
    assert_eq!(
        style.color,
        Some(thinoda_core::Color::rgb(0xAA, 0xBB, 0xCC))
    );
    assert_eq!(h.state().doc.state_id(), before);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn direct_typography_apply_button_rejects_invalid_values_without_finishing() {
    let mut h = harness("Label");
    double_click_shape(&mut h);
    click_toolbar_button(&mut h, "Font Size / Text Color / Font Family");
    click_toolbar_button(&mut h, "Font Size");
    replace_toolbar_value(&mut h, "Font Size · Value", "999");
    click_toolbar_button(&mut h, "Apply");
    assert_eq!(h.state().view.editing_board_object(), Some(h.state().shape));
    assert!(h.get_by_label("Font Size · Value").is_focused());
    assert!(h.query_by_label("Invalid font size (6–256)").is_some());
    assert!(!h.state().view.has_board_label_changes());
    replace_toolbar_value(&mut h, "Font Size · Value", "17.25");
    click_toolbar_button(&mut h, "Apply");
    assert!(h.get_by_label("Board object label").is_focused());
    let (_, _, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(rich.unwrap().runs[0].style.font_size, Some(17.25));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn toolbar_keyboard_input_preceding_an_outside_click_is_applied_in_order() {
    for direct in [false, true] {
        let mut h = harness("Label");
        double_click_shape(&mut h);
        click_toolbar_button(&mut h, "Font Size / Text Color / Font Family");
        if direct {
            click_toolbar_button(&mut h, "Font Size");
            let position = h.get_by_label("Font Size · Value").rect().center();
            click(&mut h, position);
            h.run_steps(2);
            h.key_press_modifiers(Modifiers::COMMAND, Key::A);
            h.run_steps(1);
            h.input_mut().events.push(egui::Event::Text("27.5".into()));
        } else {
            h.key_press(Key::F6);
            h.run_steps(2);
            for _ in 0..3 {
                h.key_press(Key::Tab);
                h.run_steps(1);
            }
            assert!(
                h.get_by_role_and_label(Role::Button, "Font Size +")
                    .is_focused()
            );
        }
        let outside = h.state().view.canvas_rect().right_bottom() - vec2(20.0, 20.0);
        h.input_mut().events.extend([
            egui::Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
            egui::Event::PointerMoved(outside),
            egui::Event::PointerButton {
                pos: outside,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos: outside,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
            egui::Event::Text("outside".into()),
        ]);
        h.run_steps(3);
        assert_eq!(h.state().view.editing_board_object(), None);
        let object = h
            .state()
            .doc
            .sheet()
            .canvas
            .object(h.state().shape)
            .unwrap();
        assert_eq!(object.label(), "Label");
        assert_eq!(
            object.label_rich_text.as_ref().unwrap().runs[0]
                .style
                .font_size,
            Some(if direct { 27.5 } else { 13.0 })
        );
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn text_after_toolbar_enter_and_before_an_outside_click_stays_in_the_label() {
    let mut h = harness("Label");
    double_click_shape(&mut h);
    click_toolbar_button(&mut h, "Font Size / Text Color / Font Family");
    click_toolbar_button(&mut h, "Font Size");
    let position = h.get_by_label("Font Size · Value").rect().center();
    click(&mut h, position);
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(1);
    let outside = h.state().view.canvas_rect().right_bottom() - vec2(20.0, 20.0);
    h.input_mut().events.extend([
        egui::Event::Text("27.5".into()),
        egui::Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        },
        egui::Event::Text("x".into()),
        egui::Event::PointerMoved(outside),
        egui::Event::PointerButton {
            pos: outside,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos: outside,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    ]);
    h.run_steps(3);
    let object = h
        .state()
        .doc
        .sheet()
        .canvas
        .object(h.state().shape)
        .unwrap();
    assert_eq!(object.label(), "x");
    assert_eq!(
        object.label_rich_text.as_ref().unwrap().runs[0]
            .style
            .font_size,
        Some(27.5)
    );
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn invalid_toolbar_submit_before_an_outside_click_is_reported() {
    let mut h = harness("Label");
    double_click_shape(&mut h);
    click_toolbar_button(&mut h, "Font Size / Text Color / Font Family");
    click_toolbar_button(&mut h, "Font Size");
    let position = h.get_by_label("Font Size · Value").rect().center();
    click(&mut h, position);
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(1);
    let outside = h.state().view.canvas_rect().right_bottom() - vec2(20.0, 20.0);
    h.input_mut().events.extend([
        egui::Event::Text("999".into()),
        egui::Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        },
        egui::Event::PointerMoved(outside),
        egui::Event::PointerButton {
            pos: outside,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos: outside,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    ]);
    h.run_steps(3);
    let object = h
        .state()
        .doc
        .sheet()
        .canvas
        .object(h.state().shape)
        .unwrap();
    assert_eq!(object.label(), "Label");
    assert!(object.label_rich_text.is_none());
    assert!(
        h.state()
            .errors
            .iter()
            .any(|error| error.contains("Invalid font size"))
    );
}

#[test]
fn corrected_toolbar_submit_before_an_outside_click_does_not_report_a_stale_error() {
    let mut h = harness("Label");
    double_click_shape(&mut h);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(1);
    click_toolbar_button(&mut h, "Font Size / Text Color / Font Family");
    click_toolbar_button(&mut h, "Font Size");
    let position = h.get_by_label("Font Size · Value").rect().center();
    click(&mut h, position);
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(1);
    let outside = h.state().view.canvas_rect().right_bottom() - vec2(20.0, 20.0);
    let key = |key, modifiers| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    };
    h.input_mut().events.extend([
        egui::Event::Text("999".into()),
        key(Key::Enter, Modifiers::NONE),
        key(Key::A, Modifiers::COMMAND),
        egui::Event::Text("24".into()),
        key(Key::Enter, Modifiers::NONE),
        egui::Event::PointerMoved(outside),
        egui::Event::PointerButton {
            pos: outside,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos: outside,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    ]);
    h.run_steps(3);
    let object = h
        .state()
        .doc
        .sheet()
        .canvas
        .object(h.state().shape)
        .unwrap();
    assert_eq!(
        object.label_rich_text.as_ref().unwrap().runs[0]
            .style
            .font_size,
        Some(24.0)
    );
    assert!(h.state().errors.is_empty());
}

#[test]
fn direct_typography_pages_fit_compact_canvases() {
    let mut h = harness_with_size("Label", vec2(260.0, 180.0));
    h.state_mut().view.board_labels.label_invalid_size = "Nieprawidłowy rozmiar czcionki".into();
    double_click_shape(&mut h);
    click_toolbar_button(&mut h, "Font Size / Text Color / Font Family");
    click_toolbar_button(&mut h, "Font Size");
    for label in ["Font Size · Value", "Apply"] {
        assert!(
            h.state()
                .view
                .canvas_rect()
                .contains_rect(h.get_by_label(label).rect()),
            "{label}"
        );
    }
    replace_toolbar_value(&mut h, "Font Size · Value", "999");
    let toolbar = h
        .ctx
        .memory(|memory| memory.area_rect(MapCanvas::id().with("board-label-formatting")))
        .unwrap();
    assert!(h.state().view.canvas_rect().contains_rect(toolbar));
    assert!(!toolbar.contains(h.get_by_label("Board object label").rect().center()));
    assert!(
        h.state().view.canvas_rect().contains_rect(
            h.get_by_label("Nieprawidłowy rozmiar czcionki (6–256)")
                .rect()
        )
    );
    replace_toolbar_value(&mut h, "Font Size · Value", "20");
    click_toolbar_button(&mut h, "Apply");
    click_toolbar_button(&mut h, "Label formatting (F6)");
    click_toolbar_button(&mut h, "Text Color");
    click_toolbar_button(&mut h, "Custom color");
    for label in ["Text Color · Value", "Apply"] {
        assert!(
            h.state()
                .view
                .canvas_rect()
                .contains_rect(h.get_by_label(label).rect()),
            "{label}"
        );
    }
    let toolbar = h
        .ctx
        .memory(|memory| memory.area_rect(MapCanvas::id().with("board-label-formatting")))
        .unwrap();
    assert!(!toolbar.contains(h.get_by_label("Board object label").rect().center()));
    assert!(h.state().view.canvas_rect().contains_rect(toolbar));
    replace_toolbar_value(&mut h, "Text Color · Value", "123456");
    click_toolbar_button(&mut h, "Apply");
    let (_, _, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    let style = &rich.unwrap().runs[0].style;
    assert_eq!(style.font_size, Some(20.0));
    assert_eq!(
        style.color,
        Some(thinoda_core::Color::rgb(0x12, 0x34, 0x56))
    );
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn direct_typography_field_is_keyboard_and_ime_accessible() {
    let mut h = harness("Label");
    double_click_shape(&mut h);
    h.key_press(Key::F6);
    h.run_steps(2);
    for _ in 0..8 {
        h.key_press(Key::Tab);
        h.run_steps(1);
    }
    h.key_press(Key::Enter);
    h.run_steps(2);
    h.key_press(Key::F6);
    h.run_steps(2);
    for _ in 0..2 {
        h.key_press(Key::Tab);
        h.run_steps(1);
    }
    assert!(
        h.get_by_role_and_label(Role::Button, "Font Size")
            .is_focused()
    );
    h.key_press(Key::Enter);
    h.run_steps(2);
    h.key_press(Key::F6);
    h.run_steps(2);
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert!(h.get_by_label("Font Size · Value").is_focused());
    h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
        text: "25".into(),
        active_range_chars: None,
    }));
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert!(h.get_by_label("Font Size · Value").is_focused());
    assert!(!h.state().view.has_board_label_changes());
    h.event(egui::Event::Ime(egui::ImeEvent::Commit("25".into())));
    h.run_steps(2);
    replace_toolbar_value(&mut h, "Font Size · Value", "256");
    h.key_press(Key::Enter);
    h.run_steps(2);
    let (_, _, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(rich.unwrap().runs[0].style.font_size, Some(256.0));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn selected_label_size_and_color_are_atomic_for_shapes_and_connectors() {
    for connector in [false, true] {
        let mut h = harness("Label");
        let id = if connector {
            begin_connector_label(&mut h, "Label")
        } else {
            double_click_shape(&mut h);
            h.state().shape
        };
        let before = h.state().doc.state_id();
        h.key_press_modifiers(Modifiers::COMMAND, Key::B);
        h.run_steps(2);
        click_toolbar_button(&mut h, "Font Size / Text Color / Font Family");
        click_toolbar_button(&mut h, "Font Size +");
        click_toolbar_button(&mut h, "Text Color");
        let red = h
            .get_by_role_and_label(Role::ColorWell, "Text Color #E8455C")
            .rect()
            .center();
        click(&mut h, red);
        h.run_steps(2);

        assert_eq!(h.state().doc.state_id(), before);
        let (_, text, rich) = h
            .state()
            .view
            .validated_board_rich_label_draft(&h.state().doc)
            .unwrap()
            .unwrap();
        assert_eq!(text, "Label");
        let rich = rich.unwrap();
        assert_eq!(rich.runs.len(), 1);
        assert_eq!((rich.runs[0].start, rich.runs[0].end), (0, 5));
        assert_eq!(rich.runs[0].style.bold, Some(true));
        assert_eq!(rich.runs[0].style.font_size, Some(13.0));
        assert_eq!(
            rich.runs[0].style.color,
            Some(thinoda_core::Color::rgb(0xE8, 0x45, 0x5C))
        );
        h.key_press(Key::Enter);
        h.run_steps(2);
        assert_eq!(
            h.state()
                .doc
                .sheet()
                .canvas
                .object(id)
                .unwrap()
                .label_rich_text
                .as_deref(),
            Some(&rich)
        );
        assert!(h.state_mut().doc.undo());
        assert!(
            h.state()
                .doc
                .sheet()
                .canvas
                .object(id)
                .unwrap()
                .label_rich_text
                .is_none()
        );
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn label_size_and_color_at_the_caret_apply_only_to_later_text() {
    let mut h = harness("A");
    double_click_shape(&mut h);
    set_label_cursor(&mut h, 1);
    click_toolbar_button(&mut h, "Font Size / Text Color / Font Family");
    click_toolbar_button(&mut h, "Font Size +");
    click_toolbar_button(&mut h, "Text Color");
    let blue = h
        .get_by_role_and_label(Role::ColorWell, "Text Color #2E7CF6")
        .rect()
        .center();
    click(&mut h, blue);
    h.run_steps(2);
    assert!(!h.state().view.has_board_label_changes());
    h.event(egui::Event::Text("x".into()));
    h.run_steps(2);
    let (_, text, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(text, "Ax");
    let rich = rich.unwrap();
    assert_eq!((rich.runs[0].start, rich.runs[0].end), (1, 2));
    assert_eq!(rich.runs[0].style.font_size, Some(13.0));
    assert_eq!(
        rich.runs[0].style.color,
        Some(thinoda_core::Color::rgb(0x2E, 0x7C, 0xF6))
    );
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn selected_label_size_and_color_can_return_to_inherited_values() {
    let mut h = harness("Label");
    let shape = h.state().shape;
    let mut rich = thinoda_core::InlineText::default();
    rich.update_range("Label", 0..5, |style| {
        style.underline = Some(true);
        style.font_size = Some(18.0);
        style.color = Some(thinoda_core::Color::rgb(0xE8, 0x45, 0x5C));
    })
    .unwrap();
    h.state_mut()
        .doc
        .update_canvas_object("Initial formatting", shape, |object| {
            object.label_rich_text = Some(std::sync::Arc::new(rich));
        })
        .unwrap();
    double_click_shape(&mut h);
    click_toolbar_button(&mut h, "Font Size / Text Color / Font Family");
    click_toolbar_button(&mut h, "Font Size · Default");
    click_toolbar_button(&mut h, "Text Color");
    click_toolbar_button(&mut h, "Text Color · Default");
    let (_, _, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    let rich = rich.unwrap();
    assert_eq!(rich.runs[0].style.underline, Some(true));
    assert_eq!(rich.runs[0].style.font_size, None);
    assert_eq!(rich.runs[0].style.color, None);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn selection_typography_limit_failure_keeps_the_original_draft() {
    use thinoda_core::rich_text::{InlineTextRun, InlineTextStyle, MAX_INLINE_TEXT_RUNS};

    let text = "a".repeat(MAX_INLINE_TEXT_RUNS * 2 + 1);
    let rich = thinoda_core::InlineText {
        runs: (0..MAX_INLINE_TEXT_RUNS)
            .map(|index| InlineTextRun {
                start: (index * 2) as u32,
                end: (index * 2 + 1) as u32,
                style: InlineTextStyle {
                    bold: Some(true),
                    ..Default::default()
                },
            })
            .collect(),
        paragraphs: Vec::new(),
    };
    let mut h = harness(&text);
    let shape = h.state().shape;
    h.state_mut()
        .doc
        .update_canvas_object("Initial formatting", shape, |object| {
            object.label_rich_text = Some(std::sync::Arc::new(rich.clone()));
        })
        .unwrap();
    double_click_shape(&mut h);
    set_label_selection(&mut h, text.len() - 1, text.len());
    click_toolbar_button(&mut h, "Font Size / Text Color / Font Family");
    click_toolbar_button(&mut h, "Font Size +");
    let (_, current_text, current_rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(current_text, text);
    assert_eq!(current_rich, Some(rich));
    assert_eq!(h.state().view.editing_board_object(), Some(shape));
    assert!(!h.state().errors.is_empty());
}

#[test]
fn keyboard_can_navigate_typography_pages_without_committing_the_draft() {
    let mut h = harness("Label");
    double_click_shape(&mut h);
    h.key_press(Key::F6);
    h.run_steps(2);
    for _ in 0..8 {
        h.key_press(Key::Tab);
        h.run_steps(1);
    }
    assert!(
        h.get_by_role_and_label(Role::Button, "Font Size / Text Color / Font Family")
            .is_focused()
    );
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert!(h.get_by_label("Board object label").is_focused());
    assert_eq!(h.state().view.editing_board_object(), Some(h.state().shape));
    h.key_press(Key::F6);
    h.run_steps(2);
    h.key_press(Key::Tab);
    h.run_steps(1);
    h.key_press(Key::Tab);
    h.run_steps(1);
    h.key_press(Key::Tab);
    h.run_steps(1);
    assert!(
        h.get_by_role_and_label(Role::Button, "Font Size +")
            .is_focused()
    );
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert!(h.get_by_label("Board object label").is_focused());
    h.key_press(Key::F6);
    h.run_steps(2);
    h.key_press(Key::F6);
    h.run_steps(2);
    assert!(h.get_by_label("Board object label").is_focused());
    assert_eq!(h.state().view.editing_board_object(), Some(h.state().shape));
    let (_, _, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(rich.unwrap().runs[0].style.font_size, Some(13.0));
    h.key_press(Key::F6);
    h.run_steps(2);
    for _ in 0..5 {
        h.key_press(Key::Tab);
        h.run_steps(1);
    }
    assert!(
        h.get_by_role_and_label(Role::Button, "Text Color")
            .is_focused()
    );
    h.key_press(Key::Enter);
    h.run_steps(2);
    h.key_press(Key::F6);
    h.run_steps(2);
    for _ in 0..2 {
        h.key_press(Key::Tab);
        h.run_steps(1);
    }
    assert!(
        h.get_by_role_and_label(Role::ColorWell, "Text Color #2E7CF6")
            .is_focused()
    );
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert!(h.get_by_label("Board object label").is_focused());
    let (_, _, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(
        rich.unwrap().runs[0].style.color,
        Some(thinoda_core::Color::rgb(0x2E, 0x7C, 0xF6))
    );
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn compact_canvas_keeps_typography_pages_inside_the_canvas() {
    let mut h = harness_with_size("Label", vec2(260.0, 180.0));
    double_click_shape(&mut h);
    click_toolbar_button(&mut h, "Font Size / Text Color / Font Family");
    for label in ["Font Size +", "Text Color"] {
        let control = h.get_by_role_and_label(Role::Button, label).rect();
        assert!(
            h.state().view.canvas_rect().contains_rect(control),
            "{control:?}"
        );
    }
    click_toolbar_button(&mut h, "Text Color");
    for label in ["Text Color #2E7CF6", "Text Color #FFFFFF"] {
        let control = h.get_by_role_and_label(Role::ColorWell, label).rect();
        assert!(
            h.state().view.canvas_rect().contains_rect(control),
            "{control:?}"
        );
    }
    let toolbar = h
        .ctx
        .memory(|memory| memory.area_rect(MapCanvas::id().with("board-label-formatting")))
        .unwrap();
    let editor = h.get_by_label("Board object label").rect();
    assert!(!toolbar.contains(editor.center()), "{toolbar:?} {editor:?}");
    assert!(h.state().view.canvas_rect().contains_rect(toolbar));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

fn open_family_page(h: &mut Harness<'_, State>) {
    click_toolbar_button(h, "Font Size / Text Color / Font Family");
    click_toolbar_button(h, "Font Family");
}

fn search_families(h: &mut Harness<'_, State>, query: &str) {
    let position = h.get_by_label("Font Family · Search").rect().center();
    click(h, position);
    h.run_steps(2);
    assert!(h.get_by_label("Font Family · Search").is_focused());
    h.event(egui::Event::Text(query.into()));
    h.run_steps(3);
}

#[test]
fn selected_label_family_is_searchable_and_undoable_for_shapes_and_connectors() {
    for connector in [false, true] {
        let mut h = harness("Label");
        h.state_mut().view.label_font_families =
            Some(vec!["Alpha Family".to_owned(), "Beta Family".to_owned()].into());
        let id = if connector {
            begin_connector_label(&mut h, "Label")
        } else {
            double_click_shape(&mut h);
            h.state().shape
        };
        let before = h.state().doc.state_id();
        h.key_press_modifiers(Modifiers::COMMAND, Key::B);
        h.run_steps(2);
        open_family_page(&mut h);
        search_families(&mut h, "bEtA");
        assert!(h.query_by_label("Font Family · Alpha Family").is_none());
        click_toolbar_button(&mut h, "Font Family · Beta Family");
        assert_eq!(h.state().doc.state_id(), before);
        let (_, text, rich) = h
            .state()
            .view
            .validated_board_rich_label_draft(&h.state().doc)
            .unwrap()
            .unwrap();
        assert_eq!(text, "Label");
        let rich = rich.unwrap();
        assert_eq!(rich.runs[0].style.bold, Some(true));
        assert_eq!(
            rich.runs[0].style.font_family.as_deref(),
            Some("Beta Family")
        );
        assert_eq!(
            h.state()
                .view
                .board_label_draft_font_families()
                .collect::<Vec<_>>(),
            vec!["Beta Family"]
        );
        h.key_press(Key::Enter);
        h.run_steps(2);
        assert_eq!(
            h.state()
                .doc
                .sheet()
                .canvas
                .object(id)
                .unwrap()
                .label_rich_text
                .as_deref(),
            Some(&rich)
        );
        assert!(h.state_mut().doc.undo());
        assert!(
            h.state()
                .doc
                .sheet()
                .canvas
                .object(id)
                .unwrap()
                .label_rich_text
                .is_none()
        );
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn caret_family_is_exposed_for_preview_before_later_text_is_typed() {
    let mut h = harness("A");
    h.state_mut().view.label_font_families = Some(vec!["Example Family".to_owned()].into());
    double_click_shape(&mut h);
    set_label_cursor(&mut h, 1);
    open_family_page(&mut h);
    search_families(&mut h, "example");
    click_toolbar_button(&mut h, "Font Family · Example Family");
    assert!(!h.state().view.has_board_label_changes());
    assert_eq!(
        h.state()
            .view
            .board_label_draft_font_families()
            .collect::<Vec<_>>(),
        vec!["Example Family"]
    );
    h.event(egui::Event::Text("x".into()));
    h.run_steps(2);
    let (_, text, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(text, "Ax");
    let rich = rich.unwrap();
    assert_eq!((rich.runs[0].start, rich.runs[0].end), (1, 2));
    assert_eq!(
        rich.runs[0].style.font_family.as_deref(),
        Some("Example Family")
    );
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn selected_label_family_can_return_to_inheritance_without_losing_other_styles() {
    let mut h = harness("Label");
    let shape = h.state().shape;
    let mut rich = thinoda_core::InlineText::default();
    rich.update_range("Label", 0..5, |style| {
        style.italic = Some(true);
        style.font_family = Some("Unavailable Family".into());
    })
    .unwrap();
    h.state_mut()
        .doc
        .update_canvas_object("Initial formatting", shape, |object| {
            object.label_rich_text = Some(std::sync::Arc::new(rich));
        })
        .unwrap();
    double_click_shape(&mut h);
    open_family_page(&mut h);
    assert!(
        h.query_by_label("Font Family · Unavailable Family")
            .is_some()
    );
    click_toolbar_button(&mut h, "Font Family · Default");
    let (_, _, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    let rich = rich.unwrap();
    assert_eq!(rich.runs[0].style.font_family, None);
    assert_eq!(rich.runs[0].style.italic, Some(true));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn family_search_keeps_ime_candidate_keys_out_of_the_label_editor() {
    let mut h = harness("Label");
    h.state_mut().view.label_font_families = Some(vec!["Example Family".to_owned()].into());
    double_click_shape(&mut h);
    open_family_page(&mut h);
    let position = h.get_by_label("Font Family · Search").rect().center();
    click(&mut h, position);
    h.run_steps(2);
    h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
        text: "example".into(),
        active_range_chars: None,
    }));
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert!(h.get_by_label("Font Family · Search").is_focused());
    assert_eq!(h.state().view.board_label_draft().unwrap().2, "Label");
    h.event(egui::Event::Ime(egui::ImeEvent::Commit("example".into())));
    h.run_steps(3);
    click_toolbar_button(&mut h, "Font Family · Example Family");
    let (_, text, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(text, "Label");
    assert_eq!(
        rich.unwrap().runs[0].style.font_family.as_deref(),
        Some("Example Family")
    );
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn family_search_and_choice_are_keyboard_accessible() {
    let mut h = harness("Label");
    h.state_mut().view.label_font_families = Some(vec!["Example Family".to_owned()].into());
    double_click_shape(&mut h);
    open_family_page(&mut h);
    h.key_press(Key::F6);
    h.run_steps(2);
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert!(h.get_by_label("Font Family · Search").is_focused());
    h.event(egui::Event::Text("example".into()));
    h.run_steps(2);
    assert_eq!(h.state().view.board_label_draft().unwrap().2, "Label");
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert!(
        h.get_by_role_and_label(Role::Button, "Font Family · Example Family")
            .is_focused()
    );
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert!(h.get_by_label("Board object label").is_focused());
    let (_, _, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(
        rich.unwrap().runs[0].style.font_family.as_deref(),
        Some("Example Family")
    );
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn keyboard_can_reach_families_beyond_the_visible_rows() {
    let mut h = harness("Label");
    h.state_mut().view.label_font_families =
        Some((0..30).map(|i| format!("Family {i:02}")).collect());
    double_click_shape(&mut h);
    open_family_page(&mut h);
    h.key_press(Key::F6);
    h.run_steps(2);
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert!(h.get_by_label("Font Family · Search").is_focused());
    for _ in 0..18 {
        h.key_press(Key::Tab);
        h.run_steps(2);
    }
    let choice = h.get_by_role_and_label(Role::Button, "Font Family · Family 15");
    assert!(choice.is_focused());
    let toolbar = h
        .ctx
        .memory(|memory| memory.area_rect(MapCanvas::id().with("board-label-formatting")))
        .unwrap();
    assert!(toolbar.contains(choice.rect().center()));
    h.key_press(Key::Enter);
    h.run_steps(2);
    let (_, _, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(
        rich.unwrap().runs[0].style.font_family.as_deref(),
        Some("Family 15")
    );
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn family_list_scrolls_without_moving_the_canvas() {
    let mut h = harness("Label");
    h.state_mut().view.label_font_families =
        Some((0..30).map(|i| format!("Family {i:02}")).collect());
    double_click_shape(&mut h);
    open_family_page(&mut h);
    let point = h
        .get_by_role_and_label(Role::Button, "Font Family · Family 02")
        .rect()
        .center();
    let before = h.state().view.offset;
    let before_y = h
        .get_by_role_and_label(Role::Button, "Font Family · Family 15")
        .rect()
        .center()
        .y;
    h.hover_at(point);
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, -320.0),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Trackpad,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(30);
    let choice = h.get_by_role_and_label(Role::Button, "Font Family · Family 15");
    let toolbar = h
        .ctx
        .memory(|memory| memory.area_rect(MapCanvas::id().with("board-label-formatting")))
        .unwrap();
    assert!(choice.rect().center().y < before_y);
    assert!(toolbar.contains(choice.rect().center()));
    assert_eq!(h.state().view.offset, before);
    let point = choice.rect().center();
    click(&mut h, point);
    let (_, _, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(
        rich.unwrap().runs[0].style.font_family.as_deref(),
        Some("Family 15")
    );
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn immediate_outside_click_after_family_choice_finishes_without_recapturing_focus() {
    for connector in [false, true] {
        let mut h = harness("Label");
        h.state_mut().view.label_font_families = Some(vec!["Example Family".to_owned()].into());
        let id = if connector {
            begin_connector_label(&mut h, "Label")
        } else {
            double_click_shape(&mut h);
            h.state().shape
        };
        open_family_page(&mut h);
        search_families(&mut h, "example");
        let family = h
            .get_by_role_and_label(Role::Button, "Font Family · Example Family")
            .rect()
            .center();
        click(&mut h, family);
        let outside = h.state().view.canvas_rect().right_bottom() - vec2(20.0, 20.0);
        h.input_mut().events.extend([
            egui::Event::PointerMoved(outside),
            egui::Event::PointerButton {
                pos: outside,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos: outside,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
            egui::Event::Text("outside".into()),
        ]);
        h.run_steps(4);
        assert_eq!(h.state().view.editing_board_object(), None);
        assert!(h.query_by_label("Board object label").is_none());
        let object = h.state().doc.sheet().canvas.object(id).unwrap();
        assert_eq!(object.label(), "Label");
        assert_eq!(
            object.label_rich_text.as_ref().unwrap().runs[0]
                .style
                .font_family
                .as_deref(),
            Some("Example Family")
        );
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn immediate_outside_click_after_toolbar_action_finishes_the_draft() {
    for action in ["Bold", "Font Size / Text Color / Font Family"] {
        let mut h = harness("Label");
        double_click_shape(&mut h);
        let position = h
            .get_by_role_and_label(Role::Button, action)
            .rect()
            .center();
        click(&mut h, position);
        let outside = h.state().view.canvas_rect().right_bottom() - vec2(20.0, 20.0);
        h.input_mut().events.extend([
            egui::Event::PointerMoved(outside),
            egui::Event::PointerButton {
                pos: outside,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos: outside,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
            egui::Event::Text("outside".into()),
        ]);
        h.run_steps(4);
        assert_eq!(h.state().view.editing_board_object(), None);
        let object = h
            .state()
            .doc
            .sheet()
            .canvas
            .object(h.state().shape)
            .unwrap();
        assert_eq!(object.label(), "Label");
        if action == "Bold" {
            assert_eq!(
                object.label_rich_text.as_ref().unwrap().runs[0].style.bold,
                Some(true)
            );
        }
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn compact_canvas_keeps_the_family_picker_operable() {
    let mut h = harness_with_size("Label", vec2(260.0, 180.0));
    h.state_mut().view.label_font_families = Some(vec!["Example Family".to_owned()].into());
    double_click_shape(&mut h);
    open_family_page(&mut h);
    let toolbar = h
        .ctx
        .memory(|memory| memory.area_rect(MapCanvas::id().with("board-label-formatting")))
        .unwrap();
    let editor = h.get_by_label("Board object label").rect();
    assert!(!toolbar.contains(editor.center()), "{toolbar:?} {editor:?}");
    assert!(h.state().view.canvas_rect().contains_rect(toolbar));
    search_families(&mut h, "example");
    click_toolbar_button(&mut h, "Font Family · Example Family");
    let (_, _, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(
        rich.unwrap().runs[0].style.font_family.as_deref(),
        Some("Example Family")
    );
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn rich_label_toolbar_keeps_the_draft_and_commits_one_undoable_change() {
    let mut h = harness("Formatted label");
    let before = h.state().doc.state_id();
    double_click_shape(&mut h);
    let bold = h
        .get_by_role_and_label(Role::Button, "Bold")
        .rect()
        .center();
    click(&mut h, bold);
    h.run_steps(2);
    assert_eq!(h.state().view.editing_board_object(), Some(h.state().shape));
    assert!(h.state().view.has_board_label_changes());
    assert_eq!(h.state().doc.state_id(), before);
    let (_, text, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(text, "Formatted label");
    let rich = rich.unwrap();
    assert_eq!((rich.runs[0].start, rich.runs[0].end), (0, 15));
    assert_eq!(rich.runs[0].style.bold, Some(true));

    h.key_press(Key::Enter);
    h.run_steps(2);
    let shape = h.state().shape;
    assert_eq!(
        h.state()
            .doc
            .sheet()
            .canvas
            .object(shape)
            .unwrap()
            .label_rich_text
            .as_deref(),
        Some(&rich)
    );
    assert!(h.state_mut().doc.undo());
    assert!(
        h.state()
            .doc
            .sheet()
            .canvas
            .object(shape)
            .unwrap()
            .label_rich_text
            .is_none()
    );
    assert!(h.state_mut().doc.redo());
    assert_eq!(
        h.state()
            .doc
            .sheet()
            .canvas
            .object(shape)
            .unwrap()
            .label_rich_text
            .as_deref(),
        Some(&rich)
    );
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn primary_connector_label_formats_and_commits_as_one_undoable_change() {
    let mut h = harness("Shape");
    let id = begin_connector_label(&mut h, "Connector");
    h.key_press_modifiers(Modifiers::COMMAND, Key::B);
    h.key_press_modifiers(Modifiers::COMMAND, Key::I);
    h.run_steps(2);
    assert_eq!(h.state().view.editing_board_object(), Some(id));
    let (_, text, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(text, "Connector");
    let rich = rich.unwrap();
    assert_eq!(rich.runs[0].style.bold, Some(true));
    assert_eq!(rich.runs[0].style.italic, Some(true));
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert_eq!(
        h.state()
            .doc
            .sheet()
            .canvas
            .object(id)
            .unwrap()
            .label_rich_text
            .as_deref(),
        Some(&rich)
    );
    assert!(h.state_mut().doc.undo());
    assert!(
        h.state()
            .doc
            .sheet()
            .canvas
            .object(id)
            .unwrap()
            .label_rich_text
            .is_none()
    );
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn double_click_extra_connector_label_edits_only_that_label_and_undoes() {
    let mut h = harness("Shape");
    let id = add_positioned_connector(&mut h);
    let position = {
        let state = h.state_mut();
        let layout = state.view.layout_for(&state.doc);
        let scene = thinoda_core::scene::build_scene(state.doc.sheet(), &layout);
        let item = scene.item(thinoda_core::SceneId::Object(id)).unwrap();
        let thinoda_core::scene::SceneGeometry::Polyline { points, .. } = &item.geometry else {
            panic!("expected connector path");
        };
        let CanvasObjectKind::Connector(connector) =
            &state.doc.sheet().canvas.object(id).unwrap().kind
        else {
            panic!("expected connector");
        };
        let anchor =
            thinoda_core::board::connector_label_point(points, connector.extra_labels[1].placement)
                .unwrap();
        state.view.canvas_rect().center()
            + state.view.offset
            + vec2(anchor.x, anchor.y) * state.view.zoom
    };
    click(&mut h, position);
    click(&mut h, position);
    h.run_steps(3);
    assert_eq!(
        h.state().view.editing_board_label_target(),
        Some(BoardLabelTarget {
            object: id,
            extra_label: Some(1)
        })
    );
    assert!(
        h.get_by_label("Board object label")
            .rect()
            .contains(position)
    );
    h.key_press_modifiers(Modifiers::COMMAND, Key::B);
    h.run_steps(2);
    let (target, text, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(target.extra_label, Some(1));
    assert_eq!(text, "Second");
    assert_eq!(rich.unwrap().runs[0].style.bold, Some(true));
    h.key_press(Key::Enter);
    h.run_steps(2);
    let CanvasObjectKind::Connector(connector) =
        &h.state().doc.sheet().canvas.object(id).unwrap().kind
    else {
        panic!("expected connector");
    };
    assert_eq!(connector.label, "Primary");
    assert!(connector.extra_labels[0].rich_text.is_none());
    assert_eq!(
        connector.extra_labels[1].rich_text.as_ref().unwrap().runs[0]
            .style
            .bold,
        Some(true)
    );
    assert!(h.state_mut().doc.undo());
    let CanvasObjectKind::Connector(connector) =
        &h.state().doc.sheet().canvas.object(id).unwrap().kind
    else {
        panic!("expected connector");
    };
    assert!(connector.extra_labels[1].rich_text.is_none());
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn extra_connector_label_reorder_conflict_retains_the_draft() {
    let mut h = harness("Shape");
    let id = add_positioned_connector(&mut h);
    {
        let state = h.state_mut();
        assert!(state.view.begin_board_extra_label(&state.doc, id, 0));
    }
    h.run_steps(3);
    h.key_press_modifiers(Modifiers::COMMAND, Key::I);
    h.run_steps(2);
    h.state_mut()
        .doc
        .update_canvas_object("Reorder labels", id, |object| {
            let CanvasObjectKind::Connector(connector) = &mut object.kind else {
                panic!()
            };
            connector.extra_labels.reverse();
        })
        .unwrap();
    let state = h.state_mut();
    assert!(
        state
            .view
            .validated_board_rich_label_draft(&state.doc)
            .is_err()
    );
    assert!(state.view.try_commit_board_label(&mut state.doc).is_err());
    assert!(state.view.has_board_label_changes());
    assert_eq!(state.view.board_label_draft().unwrap().2, "First");
    assert!(state.doc.undo());
    assert!(state.view.try_commit_board_label(&mut state.doc).unwrap());
    let CanvasObjectKind::Connector(connector) = &state.doc.sheet().canvas.object(id).unwrap().kind
    else {
        panic!()
    };
    assert_eq!(
        connector.extra_labels[0].rich_text.as_ref().unwrap().runs[0]
            .style
            .italic,
        Some(true)
    );
    assert!(connector.extra_labels[1].rich_text.is_none());
}

#[test]
fn extra_connector_label_can_commit_an_empty_list_without_deleting_it() {
    let mut h = harness("Shape");
    let id = add_positioned_connector(&mut h);
    {
        let state = h.state_mut();
        assert!(state.view.begin_board_extra_label(&state.doc, id, 0));
    }
    h.run_steps(3);
    let bullet = h
        .get_by_role_and_label(Role::Button, "Bulleted list")
        .rect()
        .center();
    click(&mut h, bullet);
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.key_press(Key::Backspace);
    h.run_steps(2);
    let (_, text, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert!(text.is_empty());
    assert_eq!(
        rich.unwrap().paragraphs[0].list,
        thinoda_core::ListKind::Bulleted
    );
    h.key_press(Key::Enter);
    h.run_steps(2);
    let CanvasObjectKind::Connector(connector) =
        &h.state().doc.sheet().canvas.object(id).unwrap().kind
    else {
        panic!()
    };
    assert!(connector.extra_labels[0].text.is_empty());
    assert!(connector.extra_labels[0].has_list());
    assert_eq!(connector.extra_labels[1].text, "Second");
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn empty_plain_extra_label_failure_keeps_the_draft() {
    let mut h = harness("Shape");
    let id = add_positioned_connector(&mut h);
    {
        let state = h.state_mut();
        assert!(state.view.begin_board_extra_label(&state.doc, id, 0));
    }
    h.run_steps(3);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.key_press(Key::Backspace);
    h.run_steps(2);
    {
        let state = h.state_mut();
        assert!(state.view.try_commit_board_label(&mut state.doc).is_err());
        assert_eq!(state.view.board_label_draft().unwrap().2, "");
        let CanvasObjectKind::Connector(connector) =
            &state.doc.sheet().canvas.object(id).unwrap().kind
        else {
            panic!()
        };
        assert_eq!(connector.extra_labels[0].text, "First");
    }
    h.event(egui::Event::Text("Corrected".into()));
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(2);
    let CanvasObjectKind::Connector(connector) =
        &h.state().doc.sheet().canvas.object(id).unwrap().kind
    else {
        panic!()
    };
    assert_eq!(connector.extra_labels[0].text, "Corrected");
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn double_click_sankey_extra_label_uses_the_ribbon_centerline() {
    use thinoda_core::board::{ConnectorLabel, ConnectorLabelPlacement};

    let mut h = harness("Shape");
    let mut book = thinoda_core::templates::instantiate("sankey-budget");
    let object = &mut book.active_mut().canvas.objects[0];
    let id = object.id;
    let CanvasObjectKind::Connector(connector) = &mut object.kind else {
        panic!()
    };
    connector.extra_labels.push(ConnectorLabel {
        text: "Ribbon label".into(),
        placement: ConnectorLabelPlacement {
            orthogonal: -60.0,
            ..Default::default()
        },
        rich_text: None,
    });
    h.state_mut().doc = Document::new(book);
    h.state_mut().view = MapView::default();
    h.run_steps(4);
    let position = {
        let state = h.state_mut();
        let layout = state.view.layout_for(&state.doc);
        let scene = thinoda_core::scene::build_scene(state.doc.sheet(), &layout);
        let item = scene.item(thinoda_core::SceneId::Object(id)).unwrap();
        let thinoda_core::scene::SceneGeometry::Ribbon { outline, .. } = &item.geometry else {
            panic!("expected Sankey ribbon");
        };
        let CanvasObjectKind::Connector(connector) =
            &state.doc.sheet().canvas.object(id).unwrap().kind
        else {
            panic!()
        };
        let centerline = thinoda_core::sankey::ribbon_centerline(outline);
        let anchor = thinoda_core::board::connector_label_point(
            &centerline,
            connector.extra_labels[0].placement,
        )
        .unwrap();
        assert_eq!(
            scene.hit_test(anchor),
            Some(thinoda_core::SceneHit::Object(id))
        );
        state.view.canvas_rect().center()
            + state.view.offset
            + vec2(anchor.x, anchor.y) * state.view.zoom
    };
    click(&mut h, position);
    click(&mut h, position);
    h.run_steps(3);
    assert_eq!(
        h.state().view.editing_board_label_target(),
        Some(BoardLabelTarget {
            object: id,
            extra_label: Some(0)
        })
    );
    assert!(
        h.get_by_label("Board object label")
            .rect()
            .contains(position)
    );
}

#[test]
fn connector_toolbar_and_batched_input_commit_before_an_outside_click() {
    let mut h = harness("Shape");
    let id = begin_connector_label(&mut h, "Connector");
    h.key_press(Key::F6);
    h.run_steps(2);
    assert!(h.get_by_role_and_label(Role::Button, "Bold").is_focused());
    h.key_press(Key::Tab);
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert!(h.get_by_label("Board object label").is_focused());

    let outside = h.state().view.canvas_rect().right_bottom() - vec2(20.0, 20.0);
    h.input_mut().events.extend([
        egui::Event::Key {
            key: Key::B,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        },
        egui::Event::Text("x".into()),
        egui::Event::PointerMoved(outside),
        egui::Event::PointerButton {
            pos: outside,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos: outside,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    ]);
    h.run_steps(3);
    let object = h.state().doc.sheet().canvas.object(id).unwrap();
    assert_eq!(object.label(), "x");
    let style = &object.label_rich_text.as_ref().unwrap().runs[0].style;
    assert_eq!(style.bold, Some(true));
    assert_eq!(style.italic, Some(true));
    assert_eq!(h.state().view.editing_board_object(), None);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn connector_rich_label_conflict_keeps_the_draft_until_resolved() {
    let mut h = harness("Shape");
    let id = begin_connector_label(&mut h, "Connector");
    h.key_press_modifiers(Modifiers::COMMAND, Key::B);
    h.run_steps(2);
    let mut competing = thinoda_core::InlineText::default();
    competing
        .update_range("Connector", 0..9, |style| style.italic = Some(true))
        .unwrap();
    h.state_mut()
        .doc
        .update_canvas_object("Concurrent formatting", id, |object| {
            object.label_rich_text = Some(std::sync::Arc::new(competing));
        })
        .unwrap();
    let state = h.state_mut();
    assert!(
        state
            .view
            .validated_board_rich_label_draft(&state.doc)
            .is_err()
    );
    assert!(state.view.try_commit_board_label(&mut state.doc).is_err());
    assert!(state.view.has_board_label_changes());
    assert!(state.doc.undo());
    assert!(state.view.try_commit_board_label(&mut state.doc).unwrap());
    let rich = state
        .doc
        .sheet()
        .canvas
        .object(id)
        .unwrap()
        .label_rich_text
        .as_ref()
        .unwrap();
    assert_eq!(rich.runs[0].style.bold, Some(true));
    assert_eq!(rich.runs[0].style.italic, None);
}

#[test]
fn connector_ime_commit_precedes_batched_finish_and_outside_click() {
    for finish in [None, Some(Key::Enter), Some(Key::Escape)] {
        let mut h = harness("Shape");
        let id = begin_connector_label(&mut h, "Original");
        h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "composition".into(),
            active_range_chars: None,
        }));
        h.run_steps(2);
        let outside = h.state().view.canvas_rect().right_bottom() - vec2(20.0, 20.0);
        h.input_mut()
            .events
            .push(egui::Event::Ime(egui::ImeEvent::Commit("Composed".into())));
        if let Some(key) = finish {
            h.input_mut().events.push(egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            });
        }
        h.input_mut().events.extend([
            egui::Event::PointerMoved(outside),
            egui::Event::PointerButton {
                pos: outside,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos: outside,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ]);
        h.run_steps(3);
        assert_eq!(
            h.state().doc.sheet().canvas.object(id).unwrap().label(),
            "Composed",
            "IME candidate key={finish:?}"
        );
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn rich_label_selection_expands_graphemes_and_rejects_source_conflicts() {
    let mut h = harness("a\u{301}bc");
    double_click_shape(&mut h);
    let edit_id = MapCanvas::board_label_editor_id();
    let mut state = egui::TextEdit::load_state(&h.ctx, edit_id).unwrap();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(1),
            egui::text::CCursor::new(2),
        )));
    state.store(&h.ctx, edit_id);
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::I);
    h.run_steps(2);
    let (_, _, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    let rich = rich.unwrap();
    assert_eq!((rich.runs[0].start, rich.runs[0].end), (0, 2));
    assert_eq!(rich.runs[0].style.italic, Some(true));

    let shape = h.state().shape;
    let mut competing = thinoda_core::rich_text::InlineText::default();
    competing
        .update_range("a\u{301}bc", 2..4, |style| style.bold = Some(true))
        .unwrap();
    h.state_mut()
        .doc
        .update_canvas_object("Concurrent formatting", shape, |object| {
            object.label_rich_text = Some(std::sync::Arc::new(competing));
        })
        .unwrap();
    {
        let state = h.state_mut();
        assert!(
            state
                .view
                .validated_board_rich_label_draft(&state.doc)
                .is_err()
        );
        assert!(state.view.try_commit_board_label(&mut state.doc).is_err());
        assert!(state.view.has_board_label_changes());
        assert!(state.doc.undo());
        assert!(state.view.try_commit_board_label(&mut state.doc).unwrap());
        assert_eq!(
            state
                .doc
                .sheet()
                .canvas
                .object(shape)
                .unwrap()
                .label_rich_text
                .as_deref(),
            Some(&rich)
        );
    }
}

#[test]
fn keyboard_can_reach_rich_label_controls_and_apply_a_list() {
    let mut h = harness("one\ntwo");
    double_click_shape(&mut h);
    h.key_press(Key::F6);
    h.run_steps(2);
    assert!(h.get_by_role_and_label(Role::Button, "Bold").is_focused());
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert!(h.get_by_role_and_label(Role::Button, "Italic").is_focused());
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert!(h.get_by_label("Board object label").is_focused());
    let bullet = h
        .get_by_role_and_label(Role::Button, "Bulleted list")
        .rect()
        .center();
    click(&mut h, bullet);
    h.run_steps(2);
    let (_, _, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    let rich = rich.unwrap();
    assert_eq!(rich.runs[0].style.italic, Some(true));
    assert_eq!(
        rich.paragraphs
            .iter()
            .map(|paragraph| paragraph.index)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn shortcut_before_a_batched_outside_click_is_committed() {
    let mut h = harness("Before");
    double_click_shape(&mut h);
    let empty = h.state().view.canvas_rect().right_bottom() - vec2(20.0, 20.0);
    h.input_mut().events.extend([
        egui::Event::Key {
            key: Key::B,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        },
        egui::Event::PointerMoved(empty),
        egui::Event::PointerButton {
            pos: empty,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos: empty,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    ]);
    h.run_steps(3);
    let object = h
        .state()
        .doc
        .sheet()
        .canvas
        .object(h.state().shape)
        .unwrap();
    assert_eq!(
        object.label_rich_text.as_ref().unwrap().runs[0].style.bold,
        Some(true)
    );
    assert_eq!(h.state().view.editing_board_object(), None);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn batched_format_shortcuts_are_applied_in_order() {
    let mut h = harness("Text");
    double_click_shape(&mut h);
    for key in [Key::B, Key::I, Key::B] {
        h.input_mut().events.push(egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        });
    }
    h.run_steps(2);
    let (_, _, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    let rich = rich.unwrap();
    assert_eq!(rich.runs[0].style.italic, Some(true));
    assert_ne!(rich.runs[0].style.bold, Some(true));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn formatting_shortcuts_and_text_follow_their_batched_event_order() {
    let mut h = harness("Text");
    double_click_shape(&mut h);
    h.input_mut().events.extend([
        egui::Event::Key {
            key: Key::B,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        },
        egui::Event::Text("x".into()),
        egui::Event::Key {
            key: Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        },
        egui::Event::Key {
            key: Key::I,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        },
    ]);
    h.run_steps(2);
    let (_, text, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(text, "x");
    let rich = rich.unwrap();
    assert_eq!((rich.runs[0].start, rich.runs[0].end), (0, 1));
    assert_eq!(rich.runs[0].style.bold, Some(true));
    assert_eq!(rich.runs[0].style.italic, Some(true));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn formatted_text_before_a_batched_outside_click_is_committed() {
    let mut h = harness("Text");
    double_click_shape(&mut h);
    let outside = h.state().view.canvas_rect().right_bottom() - vec2(20.0, 20.0);
    h.input_mut().events.extend([
        egui::Event::Key {
            key: Key::B,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        },
        egui::Event::Text("x".into()),
        egui::Event::PointerMoved(outside),
        egui::Event::PointerButton {
            pos: outside,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos: outside,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    ]);
    h.run_steps(3);
    let object = h
        .state()
        .doc
        .sheet()
        .canvas
        .object(h.state().shape)
        .unwrap();
    assert_eq!(object.label(), "x");
    assert_eq!(
        object.label_rich_text.as_ref().unwrap().runs[0].style.bold,
        Some(true)
    );
    assert_eq!(h.state().view.editing_board_object(), None);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn disjoint_text_edits_in_one_frame_preserve_intervening_formatting() {
    let mut h = harness("AémiddleB");
    let shape = h.state().shape;
    let mut rich = thinoda_core::InlineText::default();
    rich.update_range("AémiddleB", 1..8, |style| style.italic = Some(true))
        .unwrap();
    h.state_mut()
        .doc
        .update_canvas_object("Initial formatting", shape, |object| {
            object.label_rich_text = Some(std::sync::Arc::new(rich));
        })
        .unwrap();
    double_click_shape(&mut h);
    let edit_id = MapCanvas::board_label_editor_id();
    let mut state = egui::TextEdit::load_state(&h.ctx, edit_id).unwrap();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(0),
        )));
    state.store(&h.ctx, edit_id);
    h.input_mut().events.extend([
        egui::Event::Text("x".into()),
        egui::Event::Key {
            key: Key::End,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        },
        egui::Event::Text("y".into()),
    ]);
    h.run_steps(2);
    let (_, text, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(text, "xAémiddleBy");
    let rich = rich.unwrap();
    assert_eq!((rich.runs[0].start, rich.runs[0].end), (2, 9));
    assert_eq!(rich.runs[0].style.italic, Some(true));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn double_click_with_realistic_press_duration_opens_label_editing() {
    let mut h = harness("");
    let center = h.state().view.canvas_rect().center() + h.state().view.offset;
    h.hover_at(center);
    let mut releases = Vec::new();
    for _ in 0..2 {
        h.event(egui::Event::PointerButton {
            pos: center,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(6);
        h.event(egui::Event::PointerButton {
            pos: center,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(1);
        releases.push(h.ctx.input(|input| input.time));
    }
    let interval = releases[1] - releases[0];
    assert!((0.1..0.3).contains(&interval), "interval: {interval}");
    h.run_steps(3);
    assert_eq!(h.state().view.editing_board_object(), Some(h.state().shape));
    assert!(
        h.ctx
            .memory(|memory| memory.has_focus(MapCanvas::board_label_editor_id()))
    );
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn double_click_explains_label_editing_and_commits_multiline_text() {
    let mut h = harness("");
    double_click_shape(&mut h);
    assert!(h.query_by_label("Editing label").is_some());
    assert!(
        h.query_by_label(
            "Enter or click/tap outside to finish · Shift+Enter: new line · Esc: cancel"
        )
        .is_some()
    );
    assert_eq!(
        h.get_by_label("Board object label")
            .accesskit_node()
            .description(),
        Some("Enter or click/tap outside to finish · Shift+Enter: new line · Esc: cancel".into())
    );
    h.input_mut().events.push(egui::Event::Text("First".into()));
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::SHIFT, Key::Enter);
    h.run_steps(2);
    assert_eq!(
        h.state().view.board_label_draft(),
        Some((h.state().shape, "", "First\n"))
    );
    h.input_mut()
        .events
        .push(egui::Event::Text("Second".into()));
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(
        h.state()
            .doc
            .sheet()
            .canvas
            .object(h.state().shape)
            .unwrap()
            .label(),
        "First\nSecond"
    );
    assert!(h.query_by_label("Editing label").is_none());
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn double_click_can_reopen_a_committed_label_with_keyboard_focus() {
    let mut h = harness("");
    h.state_mut()
        .view
        .set_board_tool(BoardTool::Shape(ShapeKind::Rectangle));
    h.run_steps(3);
    double_click_shape(&mut h);
    h.get_by_label("Board object label").type_text("First");
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(
        h.state()
            .doc
            .sheet()
            .canvas
            .object(h.state().shape)
            .unwrap()
            .label(),
        "First"
    );
    h.run_steps(30);
    double_click_shape(&mut h);
    assert!(
        h.ctx
            .memory(|memory| memory.has_focus(MapCanvas::board_label_editor_id())),
        "the reopened label editor must receive keyboard focus"
    );
    h.input_mut()
        .events
        .push(egui::Event::Text("Second".into()));
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(
        h.state()
            .doc
            .sheet()
            .canvas
            .object(h.state().shape)
            .unwrap()
            .label(),
        "Second"
    );
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn another_focused_control_receives_keys_during_label_editing() {
    struct StateWithControl {
        doc: Document,
        view: MapView,
        control_id: Option<egui::Id>,
        key: Key,
        key_seen: bool,
        activated: bool,
    }

    for key in [Key::Enter, Key::Tab, Key::Escape] {
        for composing in [false, true] {
            let mut sheet = Sheet::new("Board", "");
            sheet.root_mode = SheetRootMode::Hidden;
            let shape = sheet
                .add_canvas_object(CanvasObject::shape(
                    ShapeKind::Rectangle,
                    thinoda_core::Rect::from_center_size(Vec2::ZERO, Vec2::new(240.0, 150.0)),
                ))
                .unwrap();
            let doc = Document::new(Workbook::new(sheet));
            let mut view = MapView::default();
            assert!(view.begin_board_label(&doc, shape));
            let mut h = Harness::builder()
                .with_size(vec2(800.0, 600.0))
                .with_step_dt(1.0 / 60.0)
                .build_ui_state(
                    |ui, state: &mut StateWithControl| {
                        MapCanvas::show(ui, &mut state.doc, &mut state.view);
                        state.key_seen |= ui.input(|input| {
                            input.events.iter().any(|event| {
                                matches!(
                                    event,
                                    egui::Event::Key { key, pressed: true, .. }
                                        if *key == state.key
                                )
                            })
                        });
                        egui::Area::new(egui::Id::unique("other-control"))
                            .fixed_pos(egui::pos2(20.0, 20.0))
                            .show(ui.ctx(), |ui| {
                                let response = ui.button("Other control");
                                state.control_id = Some(response.id);
                                state.activated |= response.clicked();
                            });
                    },
                    StateWithControl {
                        doc,
                        view,
                        control_id: None,
                        key,
                        key_seen: false,
                        activated: false,
                    },
                );
            h.run_steps(4);
            assert!(
                h.ctx
                    .memory(|memory| memory.has_focus(MapCanvas::board_label_editor_id()))
            );
            if composing {
                h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
                    text: "composition".into(),
                    active_range_chars: None,
                }));
                h.run_steps(1);
            }
            let other = h.state().control_id.unwrap();
            h.ctx.memory_mut(|memory| memory.request_focus(other));
            h.key_press(key);
            h.run_steps(2);
            assert!(
                h.state().key_seen,
                "{key:?} was intercepted after focus moved, composing={composing}"
            );
            if key == Key::Enter {
                assert!(
                    h.state().activated,
                    "the focused control must receive Enter, composing={composing}"
                );
            }
        }
    }
}

#[test]
fn edit_guidance_disappears_after_cancel_or_outside_commit() {
    for cancel in [false, true] {
        let mut h = harness("Original");
        double_click_shape(&mut h);
        assert!(h.query_by_label("Editing label").is_some());
        h.get_by_label("Board object label").type_text("Draft");
        h.run_steps(2);
        assert!(
            h.ctx
                .memory(|memory| memory.has_focus(MapCanvas::board_label_editor_id())),
            "typing must leave focus in the label editor"
        );
        if cancel {
            h.key_press(Key::Escape);
        } else {
            let canvas = h.state().view.canvas_rect();
            click(&mut h, canvas.right_bottom() - vec2(20.0, 20.0));
        }

        h.run_steps(3);
        assert_eq!(
            h.state()
                .doc
                .sheet()
                .canvas
                .object(h.state().shape)
                .unwrap()
                .label(),
            if cancel { "Original" } else { "Draft" }
        );
        assert!(h.query_by_label("Editing label").is_none());
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn immediate_finish_keys_after_starting_label_editing_handle_the_seeded_draft() {
    for key in [Key::Enter, Key::Tab, Key::Escape] {
        let mut h = harness("Original");
        h.get_by_label("Rounded rectangle: Original").focus();
        h.run_steps(2);
        h.input_mut().events.push(egui::Event::Text("Draft".into()));
        h.run_steps(1);
        assert_eq!(
            h.state().view.board_label_draft(),
            Some((h.state().shape, "Original", "Draft"))
        );
        assert!(
            h.ctx
                .memory(|memory| memory.has_focus(MapCanvas::board_label_editor_id()))
        );
        h.key_press(key);
        h.run_steps(3);
        assert_eq!(h.state().view.editing_board_object(), None, "{key:?}");
        assert_eq!(
            h.state()
                .doc
                .sheet()
                .canvas
                .object(h.state().shape)
                .unwrap()
                .label(),
            if key == Key::Escape {
                "Original"
            } else {
                "Draft"
            },
            "{key:?}"
        );
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn clicking_the_guidance_passes_through_and_finishes_editing() {
    let mut h = harness("Original");
    double_click_shape(&mut h);
    h.get_by_label("Board object label").type_text("Changed");
    h.run_steps(2);
    let hint = h.get_by_label("Editing label").rect().center();
    assert!(!h.get_by_label("Board object label").rect().contains(hint));
    click(&mut h, hint);
    h.run_steps(3);
    assert_eq!(
        h.state()
            .doc
            .sheet()
            .canvas
            .object(h.state().shape)
            .unwrap()
            .label(),
        "Changed"
    );
    assert!(h.query_by_label("Editing label").is_none());
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn floating_guidance_uses_free_space_above_or_below_the_edited_shape() {
    for (offset, above) in [(-120.0, false), (120.0, true)] {
        let mut h = harness("");
        h.state_mut().view.offset.y = offset;
        h.run_steps(3);
        double_click_shape(&mut h);
        let hint_id = MapCanvas::id().with("board-label-edit-hint");
        let hint = h.ctx.memory(|memory| memory.area_rect(hint_id)).unwrap();
        let view = &h.state().view;
        let center = view.canvas_rect().center() + view.offset;
        let bounds = egui::Rect::from_center_size(center, vec2(240.0, 150.0) * view.zoom);
        assert!(
            !hint.expand(8.0).intersects(bounds),
            "guidance overlaps the edited shape: {hint:?}, {bounds:?}"
        );
        assert_eq!(hint.center().y < bounds.top(), above);
        assert!(
            !hint.intersects(h.get_by_label("Board object label").rect()),
            "guidance overlaps the label editor"
        );
    }
}

#[test]
fn clicking_empty_canvas_while_editing_also_places_the_armed_shape() {
    let mut h = harness("Original");
    h.state_mut()
        .view
        .set_board_tool(BoardTool::Shape(ShapeKind::Rectangle));
    h.run_steps(2);
    double_click_shape(&mut h);
    h.get_by_label("Board object label").type_text("Changed");
    h.run_steps(2);
    assert_eq!(h.state().doc.sheet().canvas.objects.len(), 1);
    assert!(
        h.get_by_label("Board object label")
            .accesskit_node()
            .description()
            .unwrap()
            .contains("Clicking or tapping empty canvas also adds the armed shape.")
    );
    let canvas = h.state().view.canvas_rect();
    click(&mut h, canvas.right_bottom() - vec2(20.0, 20.0));
    h.run_steps(3);

    assert_eq!(h.state().doc.sheet().canvas.objects.len(), 2);
    assert_eq!(
        h.state()
            .doc
            .sheet()
            .canvas
            .object(h.state().shape)
            .unwrap()
            .label(),
        "Changed"
    );
    assert!(h.query_by_label("Editing label").is_none());
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    assert!(h.state_mut().doc.undo());
    assert_eq!(h.state().doc.sheet().canvas.objects.len(), 1);
    assert_eq!(
        h.state()
            .doc
            .sheet()
            .canvas
            .object(h.state().shape)
            .unwrap()
            .label(),
        "Changed"
    );
    assert!(h.state_mut().doc.undo());
    assert_eq!(
        h.state()
            .doc
            .sheet()
            .canvas
            .object(h.state().shape)
            .unwrap()
            .label(),
        "Original"
    );
}

#[test]
fn text_before_an_outside_press_is_not_lost() {
    for armed in [false, true] {
        for same_frame in [false, true] {
            let mut h = harness("Original");
            if armed {
                h.state_mut()
                    .view
                    .set_board_tool(BoardTool::Shape(ShapeKind::Rectangle));
                h.run_steps(2);
            }
            double_click_shape(&mut h);
            let empty = h.state().view.canvas_rect().right_bottom() - vec2(20.0, 20.0);
            h.input_mut().events.extend([
                egui::Event::Text("Pending".into()),
                egui::Event::PointerMoved(empty),
                egui::Event::PointerButton {
                    pos: empty,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ]);
            if !same_frame {
                h.run_steps(1);
            }
            h.event(egui::Event::PointerButton {
                pos: empty,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            });
            h.run_steps(3);
            assert_eq!(
                h.state()
                    .doc
                    .sheet()
                    .canvas
                    .object(h.state().shape)
                    .unwrap()
                    .label(),
                "Pending",
                "armed={armed}, same_frame={same_frame}"
            );
            assert_eq!(
                h.state().doc.sheet().canvas.objects.len(),
                if armed { 2 } else { 1 }
            );
            assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
        }
    }
}

#[test]
fn keyboard_events_after_a_finish_key_remain_available_to_the_next_owner() {
    struct InputState {
        doc: Document,
        view: MapView,
        shape: CanvasObjectId,
        later_text_available: bool,
        errors: Vec<String>,
    }

    for finish in [Key::Enter, Key::Tab, Key::Escape] {
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let mut object = CanvasObject::shape(
            ShapeKind::Rectangle,
            thinoda_core::Rect::from_center_size(Vec2::ZERO, Vec2::new(240.0, 150.0)),
        );
        if let CanvasObjectKind::Shape { label, .. } = &mut object.kind {
            *label = "Original".into();
        }
        let shape = sheet.add_canvas_object(object).unwrap();
        let doc = Document::new(Workbook::new(sheet));
        let mut view = MapView::default();
        assert!(view.begin_board_label(&doc, shape));
        let mut h = Harness::builder()
            .with_size(vec2(800.0, 600.0))
            .build_ui_state(
                |ui, state: &mut InputState| {
                    if let Some(error) = MapCanvas::finish_board_label_before_pointer_press(
                        ui,
                        &mut state.doc,
                        &mut state.view,
                    ) {
                        state.errors.push(error.to_string());
                    }
                    state.later_text_available |= ui.input(|input| {
                        input.events.iter().any(
                            |event| matches!(event, egui::Event::Text(text) if text == "Later"),
                        )
                    });
                    if let Some(error) = MapCanvas::show(ui, &mut state.doc, &mut state.view).error
                    {
                        state.errors.push(error.to_string());
                    }
                },
                InputState {
                    doc,
                    view,
                    shape,
                    later_text_available: false,
                    errors: Vec::new(),
                },
            );
        h.run_steps(3);
        let target = h.state().view.canvas_rect().right_bottom() - vec2(20.0, 20.0);
        h.input_mut().events.extend([
            egui::Event::Text("Before".into()),
            egui::Event::Key {
                key: finish,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
            egui::Event::Text("Later".into()),
            egui::Event::PointerMoved(target),
            egui::Event::PointerButton {
                pos: target,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ]);
        h.run_steps(1);
        assert!(h.state().later_text_available, "{finish:?}");
        assert_eq!(
            h.state()
                .doc
                .sheet()
                .canvas
                .object(h.state().shape)
                .unwrap()
                .label(),
            if finish == Key::Escape {
                "Original"
            } else {
                "Before"
            },
            "{finish:?}"
        );
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn text_before_selecting_another_shape_is_not_lost() {
    for (same_frame, drag) in [(false, false), (true, false), (true, true)] {
        let mut h = harness("Original");
        let other = h
            .state_mut()
            .doc
            .add_canvas_object(CanvasObject::shape(
                ShapeKind::Rectangle,
                thinoda_core::Rect::from_center_size(Vec2::new(280.0, 0.0), Vec2::new(80.0, 80.0)),
            ))
            .unwrap();
        h.run_steps(3);
        double_click_shape(&mut h);
        let view = &h.state().view;
        let position = view.canvas_rect().center() + view.offset + vec2(280.0, 0.0) * view.zoom;
        h.input_mut().events.extend([
            egui::Event::Text("Pending".into()),
            egui::Event::PointerMoved(position),
            egui::Event::PointerButton {
                pos: position,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ]);
        if !same_frame || drag {
            h.run_steps(1);
        }
        if drag {
            h.event(egui::Event::PointerMoved(position + vec2(30.0, 0.0)));
            h.run_steps(1);
        }
        h.event(egui::Event::PointerButton {
            pos: position
                + if drag {
                    vec2(30.0, 0.0)
                } else {
                    vec2(0.0, 0.0)
                },
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(3);
        assert_eq!(
            h.state()
                .doc
                .sheet()
                .canvas
                .object(h.state().shape)
                .unwrap()
                .label(),
            "Pending",
            "same_frame={same_frame}, drag={drag}"
        );
        assert!(
            h.state().view.board_selection.contains(&other),
            "the other shape should be selected"
        );
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn composition_and_finish_keys_precede_a_batched_canvas_click() {
    for finish in [None, Some(Key::Enter), Some(Key::Escape)] {
        let mut h = harness("Original");
        h.state_mut()
            .view
            .set_board_tool(BoardTool::Shape(ShapeKind::Rectangle));
        h.run_steps(2);
        double_click_shape(&mut h);
        h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "composition".into(),
            active_range_chars: None,
        }));
        h.run_steps(2);
        let empty = h.state().view.canvas_rect().right_bottom() - vec2(20.0, 20.0);
        h.input_mut()
            .events
            .push(egui::Event::Ime(egui::ImeEvent::Commit("Composed".into())));
        if let Some(key) = finish {
            h.input_mut().events.push(egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            });
        }
        h.input_mut().events.extend([
            egui::Event::PointerMoved(empty),
            egui::Event::PointerButton {
                pos: empty,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos: empty,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ]);
        h.run_steps(3);
        assert_eq!(
            h.state()
                .doc
                .sheet()
                .canvas
                .object(h.state().shape)
                .unwrap()
                .label(),
            "Composed",
            "IME candidate key={finish:?}"
        );
        assert_eq!(h.state().doc.sheet().canvas.objects.len(), 2);
        assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
    }
}

#[test]
fn cancellation_before_a_batched_canvas_click_discards_the_draft() {
    let mut h = harness("Original");
    h.state_mut()
        .view
        .set_board_tool(BoardTool::Shape(ShapeKind::Rectangle));
    h.run_steps(2);
    double_click_shape(&mut h);
    let empty = h.state().view.canvas_rect().right_bottom() - vec2(20.0, 20.0);
    h.input_mut().events.extend([
        egui::Event::Text("Discarded".into()),
        egui::Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        },
        egui::Event::PointerMoved(empty),
        egui::Event::PointerButton {
            pos: empty,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos: empty,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    ]);
    h.run_steps(3);
    assert_eq!(
        h.state()
            .doc
            .sheet()
            .canvas
            .object(h.state().shape)
            .unwrap()
            .label(),
        "Original"
    );
    assert_eq!(h.state().doc.sheet().canvas.objects.len(), 2);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn immediate_escape_before_a_batched_canvas_click_discards_the_seeded_draft() {
    let mut h = harness("Original");
    h.state_mut()
        .view
        .set_board_tool(BoardTool::Shape(ShapeKind::Rectangle));
    h.run_steps(2);
    h.get_by_label("Rounded rectangle: Original").focus();
    h.run_steps(2);
    h.input_mut().events.push(egui::Event::Text("Draft".into()));
    h.run_steps(1);
    assert_eq!(
        h.state().view.board_label_draft(),
        Some((h.state().shape, "Original", "Draft"))
    );
    let empty = h.state().view.canvas_rect().right_bottom() - vec2(20.0, 20.0);
    h.input_mut().events.extend([
        egui::Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        },
        egui::Event::PointerMoved(empty),
        egui::Event::PointerButton {
            pos: empty,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos: empty,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    ]);
    h.run_steps(3);
    assert_eq!(
        h.state()
            .doc
            .sheet()
            .canvas
            .object(h.state().shape)
            .unwrap()
            .label(),
        "Original"
    );
    assert_eq!(h.state().doc.sheet().canvas.objects.len(), 2);
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn text_after_a_batched_outside_click_does_not_change_the_old_label() {
    let mut h = harness("Original");
    double_click_shape(&mut h);
    let empty = h.state().view.canvas_rect().right_bottom() - vec2(20.0, 20.0);
    h.input_mut().events.extend([
        egui::Event::Text("Before".into()),
        egui::Event::PointerMoved(empty),
        egui::Event::PointerButton {
            pos: empty,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos: empty,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
        egui::Event::Text("After".into()),
    ]);
    h.run_steps(3);
    assert_eq!(
        h.state()
            .doc
            .sheet()
            .canvas
            .object(h.state().shape)
            .unwrap()
            .label(),
        "Before"
    );
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn rejected_batched_click_keeps_the_queued_label_draft() {
    let mut h = harness("Original");
    h.state_mut()
        .view
        .set_board_tool(BoardTool::Shape(ShapeKind::Rectangle));
    h.run_steps(2);
    double_click_shape(&mut h);
    let shape = h.state().shape;
    h.state_mut()
        .doc
        .update_canvas_object("Concurrent edit", shape, |object| {
            if let CanvasObjectKind::Shape { label, .. } = &mut object.kind {
                *label = "External".into();
            }
        })
        .unwrap();
    let empty = h.state().view.canvas_rect().right_bottom() - vec2(20.0, 20.0);
    h.input_mut().events.extend([
        egui::Event::Text("Pending".into()),
        egui::Event::PointerMoved(empty),
        egui::Event::PointerButton {
            pos: empty,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos: empty,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    ]);
    h.run_steps(3);
    assert_eq!(
        h.state().doc.sheet().canvas.object(shape).unwrap().label(),
        "External"
    );
    assert_eq!(
        h.state().view.board_label_draft(),
        Some((shape, "Original", "Pending"))
    );
    assert_eq!(h.state().doc.sheet().canvas.objects.len(), 1);
    assert!(
        h.ctx
            .memory(|memory| memory.has_focus(MapCanvas::board_label_editor_id()))
    );
    assert!(!h.state().errors.is_empty());
}

#[test]
fn compact_canvas_keeps_the_editor_description_without_covering_the_shape() {
    let mut h = harness_with_size("", vec2(260.0, 180.0));
    double_click_shape(&mut h);
    assert!(h.query_by_label("Editing label").is_none());
    assert_eq!(
        h.get_by_label("Board object label")
            .accesskit_node()
            .description(),
        Some("Enter or click/tap outside to finish · Shift+Enter: new line · Esc: cancel".into())
    );
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(h.query_by_label("Editing label").is_none());
}

#[test]
fn compact_formatting_toolbar_leaves_the_label_visible_and_operable() {
    let mut h = harness_with_size("Compact label", vec2(260.0, 180.0));
    double_click_shape(&mut h);
    let toolbar = h
        .ctx
        .memory(|memory| memory.area_rect(MapCanvas::id().with("board-label-formatting")))
        .unwrap();
    let editor = h.get_by_label("Board object label").rect();
    assert!(!toolbar.contains(editor.center()), "{toolbar:?} {editor:?}");
    assert!(h.state().view.canvas_rect().contains_rect(toolbar));
    let bold = h
        .get_by_role_and_label(Role::Button, "Bold")
        .rect()
        .center();
    click(&mut h, bold);
    h.run_steps(2);
    let (_, _, rich) = h
        .state()
        .view
        .validated_board_rich_label_draft(&h.state().doc)
        .unwrap()
        .unwrap();
    assert_eq!(rich.unwrap().runs[0].style.bold, Some(true));
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}

#[test]
fn empty_label_placeholder_is_not_saved_as_content() {
    let mut h = harness("");
    let before = h.state().doc.state_id();
    double_click_shape(&mut h);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(
        h.state()
            .doc
            .sheet()
            .canvas
            .object(h.state().shape)
            .unwrap()
            .label(),
        ""
    );
    assert_eq!(h.state().doc.state_id(), before);
    assert!(h.query_by_label("Editing label").is_none());
    assert!(h.state().errors.is_empty(), "{:?}", h.state().errors);
}
