use egui::{Key, Modifiers, PointerButton, Pos2, Role, vec2};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use thinoda_canvas::{BoardTool, MapCanvas, MapView};
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
