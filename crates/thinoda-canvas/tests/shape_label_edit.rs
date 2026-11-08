use egui::{Key, Modifiers, PointerButton, Pos2, vec2};
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
