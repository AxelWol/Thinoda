use super::*;
use egui::{Key, Modifiers, PointerButton, Role, accesskit::Toggled};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use thinoda_core::{
    Rect as BoardRect, Sheet, Vec2,
    clipboard::{
        copy_board_selection, decode_board_payload, encode_board_payload, paste_board_selection,
    },
    layout::{ApproxMeasure, layout_sheet},
    templates,
};

fn app_with_shape() -> (ThinodaApp, CanvasObjectId) {
    let mut app = ThinodaApp::headless();
    app.settings.ai.enabled = false;
    app.settings.show_format_panel = false;
    app.new_doc(templates::instantiate("blank-canvas"));
    let doc = app.doc_mut().unwrap();
    let mut object = CanvasObject::shape(
        ShapeKind::Rectangle,
        BoardRect::from_center_size(Vec2::new(0.0, 0.0), Vec2::new(160.0, 90.0)),
    );
    object.alt_text = "Keep shape description".into();
    object.style.opacity = 0.75;
    let id = doc.doc.add_canvas_object(object).unwrap();
    doc.view.select_board_object(id);
    (app, id)
}

fn harness(app: ThinodaApp) -> Harness<'static, ThinodaApp> {
    harness_with_size(app, vec2(1000.0, 700.0))
}

fn harness_with_size(app: ThinodaApp, size: egui::Vec2) -> Harness<'static, ThinodaApp> {
    let mut harness = Harness::builder()
        .with_size(size)
        .with_step_dt(1.0 / 60.0)
        .with_context_setup(|ctx| {
            ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
        })
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    harness.run_steps(6);
    harness
}

fn object(app: &ThinodaApp, id: CanvasObjectId) -> CanvasObject {
    app.doc()
        .unwrap()
        .doc
        .sheet()
        .canvas
        .object(id)
        .unwrap()
        .clone()
}

fn shape_kind(app: &ThinodaApp, id: CanvasObjectId) -> ShapeKind {
    let CanvasObjectKind::Shape { shape, .. } = object(app, id).kind else {
        panic!("expected shape");
    };
    shape
}

fn click(h: &mut Harness<'_, ThinodaApp>, role: Role, label: &str) {
    h.get_by_role_and_label(role, label).scroll_to_me();
    h.run_steps(4);
    h.get_by_role_and_label(role, label).click();
    h.run_steps(4);
}

fn close_popup(h: &mut Harness<'_, ThinodaApp>) {
    h.key_press(Key::Escape);
    h.run_steps(3);
}

fn undo(h: &mut Harness<'_, ThinodaApp>) {
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    h.run_steps(2);
}

#[test]
fn selected_shape_options_overlay_a_stationary_canvas_and_toolbox() {
    for size in [vec2(320.0, 240.0), vec2(1000.0, 700.0)] {
        let (mut app, id) = app_with_shape();
        app.doc_mut().unwrap().view.board_selection.clear();
        let mut h = harness_with_size(app, size);
        let canvas = h.state().doc().unwrap().view.canvas_rect();
        let rail_id = egui::Id::unique("thinoda_board_toolbar");
        let rail = h.ctx.memory(|memory| memory.area_rect(rail_id)).unwrap();

        h.state_mut()
            .doc_mut()
            .unwrap()
            .view
            .select_board_object(id);
        h.run_steps(8);
        let strip = h
            .ctx
            .memory(|memory| {
                memory.area_rect(egui::Id::unique((
                    "thinoda_shape_options",
                    egui::ViewportId::ROOT,
                )))
            })
            .expect("selected shape has its own options strip");
        assert_eq!(h.state().doc().unwrap().view.canvas_rect(), canvas);
        assert_eq!(h.ctx.memory(|memory| memory.area_rect(rail_id)), Some(rail));
        assert!(canvas.contains_rect(strip), "{size:?}: {strip:?}");
        assert!(!rail.intersects(strip), "{size:?}: {rail:?} {strip:?}");
        let shape_button = h.get_by_role_and_label(Role::Button, "Change shape").rect();
        assert!(
            strip.contains_rect(shape_button),
            "{size:?}: {shape_button:?}"
        );

        let before = h.state().doc().unwrap().doc.workbook().clone();
        let blank = egui::pos2(strip.right() - 8.0, strip.top() + 5.0);
        h.event(egui::Event::PointerMoved(blank));
        h.run_steps(1);
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos: blank,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            });
            h.run_steps(1);
        }
        h.run_steps(2);
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        assert_eq!(h.state().doc().unwrap().view.board_selection, [id]);
    }
}

#[test]
fn armed_shape_tool_keeps_contextual_actions_and_repeat_placement() {
    let (mut app, id) = app_with_shape();
    let tool = BoardTool::Shape(ShapeKind::Ellipse);
    app.doc_mut().unwrap().view.set_board_tool(tool);
    let mut h = harness(app);
    let canvas = h.state().doc().unwrap().view.canvas_rect();

    click(&mut h, Role::Button, "Change shape");
    click(&mut h, Role::Button, "Diamond");
    assert_eq!(shape_kind(h.state(), id), ShapeKind::Diamond);
    close_popup(&mut h);
    click(&mut h, Role::Button, "Text Color");
    click(&mut h, Role::ColorWell, "#2E7CF6");
    assert_eq!(
        object(h.state(), id).style.text_color,
        Some(Color::rgb(0x2e, 0x7c, 0xf6))
    );
    assert_eq!(h.state().doc().unwrap().view.board_tool, tool);
    assert_eq!(h.state().doc().unwrap().doc.sheet().canvas.objects.len(), 1);
    assert!(h.query_by_label("Text Color · Auto").is_some());

    let strip = h
        .ctx
        .memory(|memory| {
            memory.area_rect(egui::Id::unique((
                "thinoda_shape_options",
                egui::ViewportId::ROOT,
            )))
        })
        .unwrap();
    for (point, expected_objects) in [
        (egui::pos2(strip.right() - 8.0, strip.top() + 5.0), 1),
        (egui::pos2(canvas.right() - 60.0, canvas.bottom() - 60.0), 2),
    ] {
        h.event(egui::Event::PointerMoved(point));
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
        h.run_steps(4);
        assert_eq!(
            h.state().doc().unwrap().doc.sheet().canvas.objects.len(),
            expected_objects
        );
        if expected_objects == 1 {
            assert_eq!(h.state().doc().unwrap().view.board_selection, [id]);
            assert!(h.query_by_label("Text Color · Auto").is_none());
        }
    }

    let doc = h.state().doc().unwrap();
    assert_eq!(doc.view.board_tool, tool);
    assert_eq!(doc.doc.sheet().canvas.objects.len(), 2);
    let selected = doc.view.board_selection[0];
    assert_ne!(selected, id);
    assert_eq!(shape_kind(h.state(), selected), ShapeKind::Ellipse);
    assert!(h.query_by_label("Text Color · Auto").is_none());
    assert!(
        h.query_by_role_and_label(Role::Button, "Text Color")
            .is_some()
    );
}

#[test]
fn shape_options_keep_their_popup_on_a_tiny_canvas() {
    let (app, id) = app_with_shape();
    let mut h = harness_with_size(app, vec2(320.0, 170.0));
    let canvas = h.state().doc().unwrap().view.canvas_rect();
    assert!(!board_toolbar_fits_below_context(canvas), "{canvas:?}");
    let rail_id = egui::Id::unique("thinoda_board_toolbar");
    let rail_visible = |h: &Harness<'_, ThinodaApp>| {
        h.ctx.memory(|memory| {
            memory
                .areas()
                .visible_layer_ids()
                .iter()
                .any(|layer| layer.id == rail_id)
        })
    };
    assert!(!rail_visible(&h));
    click(&mut h, Role::Button, "Text Color");
    click(&mut h, Role::ColorWell, "#2E7CF6");
    assert_eq!(
        object(h.state(), id).style.text_color,
        Some(Color::rgb(0x2e, 0x7c, 0xf6))
    );
    assert_eq!(h.state().doc().unwrap().view.canvas_rect(), canvas);

    h.state_mut()
        .doc_mut()
        .unwrap()
        .view
        .board_selection
        .clear();
    h.run_steps(6);
    assert!(rail_visible(&h));
    assert!(h.query_by_label("Text Color · Auto").is_none());
}

#[test]
fn text_color_menu_initially_exposes_its_custom_picker() {
    for size in [vec2(320.0, 240.0), vec2(1000.0, 700.0)] {
        let (app, _) = app_with_shape();
        let mut h = harness_with_size(app, size);
        click(&mut h, Role::Button, "Text Color");
        h.run_steps(12);
        let header = h.get_by_label("Custom color").rect();
        let popups = h
            .ctx
            .data(|data| {
                data.get_temp::<ConnectorPopups>(connector_popup_slot(egui::ViewportId::ROOT))
            })
            .unwrap();
        let popup = h
            .ctx
            .memory(|memory| memory.area_rect(popups.ids[4]))
            .unwrap();
        assert!(
            popup.contains_rect(header),
            "{size:?}: custom picker header {header:?} is clipped by {popup:?}"
        );
    }
}

#[test]
fn shape_options_and_text_color_are_translated_in_every_supported_language() {
    for language in i18n::Lang::ALL {
        for key in ["Shape options", "Text Color"] {
            let translation = i18n::t_in(language, key);
            assert!(!translation.is_empty(), "{language:?}: {key}");
            if language != i18n::Lang::En {
                assert_ne!(translation, key, "{language:?}");
            }
        }
    }
}

#[test]
fn shape_text_color_preserves_alpha_and_supports_auto_undo_redo() {
    let (mut app, id) = app_with_shape();
    let original = Color::rgba(100, 120, 140, 96);
    app.doc_mut()
        .unwrap()
        .doc
        .update_canvas_object("Set text color", id, |object| {
            object.style.text_color = Some(original);
        })
        .unwrap();
    let mut h = harness(app);
    let before = object(h.state(), id);
    click(&mut h, Role::Button, "Text Color");
    click(&mut h, Role::ColorWell, "#2E7CF6");
    let blue = Color::rgba(0x2e, 0x7c, 0xf6, 96);
    assert_eq!(object(h.state(), id).style.text_color, Some(blue));
    close_popup(&mut h);
    click(&mut h, Role::Button, "Text Color");
    click(&mut h, Role::Button, "Text Color · Auto");
    let mut auto = before.clone();
    auto.style.text_color = None;
    assert_eq!(object(h.state(), id), auto);
    close_popup(&mut h);
    undo(&mut h);
    assert_eq!(object(h.state(), id).style.text_color, Some(blue));
    undo(&mut h);
    assert_eq!(object(h.state(), id), before);
    assert!(h.state_mut().doc_mut().unwrap().doc.redo());
    assert_eq!(object(h.state(), id).style.text_color, Some(blue));
}

#[test]
fn keyboard_undo_redo_works_after_closing_shape_color_picker() {
    for tool in [BoardTool::Select, BoardTool::Shape(ShapeKind::Rectangle)] {
        let (mut app, id) = app_with_shape();
        app.doc_mut().unwrap().view.set_board_tool(tool);
        let mut h = harness(app);
        click(&mut h, Role::Button, "Text Color");
        click(&mut h, Role::ColorWell, "#2E7CF6");
        close_popup(&mut h);
        assert_eq!(
            object(h.state(), id).style.text_color,
            Some(Color::rgb(0x2e, 0x7c, 0xf6))
        );

        h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
        h.run_steps(3);
        assert_eq!(object(h.state(), id).style.text_color, None, "{tool:?}");

        h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
        h.run_steps(3);
        assert_eq!(
            object(h.state(), id).style.text_color,
            Some(Color::rgb(0x2e, 0x7c, 0xf6)),
            "{tool:?}"
        );
    }
}

#[test]
fn shape_toolbar_history_shortcuts_follow_the_active_keymap() {
    let (mut app, id) = app_with_shape();
    app.settings.keymaps.duplicate_active("Toolbar").unwrap();
    for (command, shift) in [("edit.undo", false), ("edit.redo", true)] {
        app.settings.keymaps.profiles[0].bindings.insert(
            command.into(),
            vec![thinoda_platform::keymaps::KeyBinding {
                key: "U".into(),
                command: true,
                shift,
                ..Default::default()
            }],
        );
    }
    let mut h = harness(app);
    click(&mut h, Role::Button, "Text Color");
    click(&mut h, Role::ColorWell, "#2E7CF6");
    close_popup(&mut h);
    let blue = Some(Color::rgb(0x2e, 0x7c, 0xf6));

    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(3);
    assert_eq!(object(h.state(), id).style.text_color, blue);
    h.key_press_modifiers(Modifiers::COMMAND, Key::U);
    h.run_steps(3);
    assert_eq!(object(h.state(), id).style.text_color, None);
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::U);
    h.run_steps(3);
    assert_eq!(object(h.state(), id).style.text_color, blue);
}

#[test]
fn shape_toolbar_history_shortcuts_do_not_undo_document_behind_color_draft() {
    let (mut app, id) = app_with_shape();
    let original = Color::rgba(100, 120, 140, 96);
    app.doc_mut()
        .unwrap()
        .doc
        .update_canvas_object("Set text color", id, |object| {
            object.style.text_color = Some(original);
        })
        .unwrap();
    let mut h = harness(app);
    click(&mut h, Role::Button, "Text Color");
    h.get_by_label("Custom color").click();
    h.run_steps(5);
    h.query_all_by_role(Role::SpinButton)
        .find(|control| control.value().is_some_and(|value| value.starts_with("R ")))
        .unwrap()
        .click();
    h.run_steps(2);
    let before = h.state().doc().unwrap().doc.state_id();
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.event(egui::Event::Text("200".into()));
    h.run_steps(3);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.state_id(), before);
    assert_eq!(object(h.state(), id).style.text_color, Some(original));
}

#[test]
fn custom_shape_text_color_keyboard_edit_is_atomic_and_escape_cancels() {
    for accept in [false, true] {
        let (mut app, id) = app_with_shape();
        let original = Color::rgba(100, 120, 140, 96);
        app.doc_mut()
            .unwrap()
            .doc
            .update_canvas_object("Set text color", id, |object| {
                object.style.text_color = Some(original);
            })
            .unwrap();
        let mut h = harness(app);
        click(&mut h, Role::Button, "Text Color");
        h.get_by_label("Custom color").click();
        h.run_steps(5);
        h.query_all_by_role(Role::SpinButton)
            .find(|control| control.value().is_some_and(|value| value.starts_with("R ")))
            .unwrap()
            .scroll_to_me();
        h.run_steps(4);
        h.query_all_by_role(Role::SpinButton)
            .find(|control| control.value().is_some_and(|value| value.starts_with("R ")))
            .unwrap()
            .click();
        h.run_steps(2);
        let before = h.state().doc().unwrap().doc.state_id();
        h.key_press_modifiers(Modifiers::COMMAND, Key::A);
        h.event(egui::Event::Text("200".into()));
        h.run_steps(3);
        assert_eq!(h.state().doc().unwrap().doc.state_id(), before);
        h.key_press(if accept { Key::Enter } else { Key::Escape });
        h.run_steps(5);
        if accept {
            assert_eq!(
                object(h.state(), id).style.text_color,
                Some(Color::rgba(200, 120, 140, 96))
            );
            undo(&mut h);
        } else {
            assert_eq!(h.state().doc().unwrap().doc.state_id(), before);
            assert!(!h.ctx.text_edit_focused());
        }
        assert_eq!(object(h.state(), id).style.text_color, Some(original));
        assert_eq!(h.state().doc().unwrap().view.board_selection, [id]);
    }
}

#[test]
fn stale_shape_text_color_drafts_close_without_changing_either_shape() {
    for reason in [
        "selection",
        "revision",
        "tool",
        "armed-tool",
        "connector-tool",
        "modal",
    ] {
        let (mut app, first) = app_with_shape();
        let second = app
            .doc_mut()
            .unwrap()
            .doc
            .add_canvas_object(CanvasObject::shape(
                ShapeKind::Ellipse,
                BoardRect::from_center_size(Vec2::new(300.0, 0.0), Vec2::new(60.0, 40.0)),
            ))
            .unwrap();
        app.doc_mut()
            .unwrap()
            .doc
            .update_canvas_object("Set text color", first, |object| {
                object.style.text_color = Some(Color::rgb(100, 120, 140));
            })
            .unwrap();
        let mut h = harness(app);
        click(&mut h, Role::Button, "Text Color");
        h.get_by_label("Custom color").click();
        h.run_steps(5);
        h.query_all_by_role(Role::SpinButton)
            .find(|control| control.value().is_some_and(|value| value.starts_with("R ")))
            .unwrap()
            .scroll_to_me();
        h.run_steps(4);
        h.query_all_by_role(Role::SpinButton)
            .find(|control| control.value().is_some_and(|value| value.starts_with("R ")))
            .unwrap()
            .click();
        h.run_steps(2);
        let initial = h.state().doc().unwrap().doc.state_id();
        h.key_press_modifiers(Modifiers::COMMAND, Key::A);
        h.event(egui::Event::Text("200".into()));
        h.run_steps(3);
        assert_eq!(h.state().doc().unwrap().doc.state_id(), initial, "{reason}");
        match reason {
            "selection" => h
                .state_mut()
                .doc_mut()
                .unwrap()
                .view
                .select_board_object(second),
            "revision" => h
                .state_mut()
                .doc_mut()
                .unwrap()
                .doc
                .edit_sheet("Other edit", |sheet| {
                    sheet.board.snap = !sheet.board.snap;
                    Ok(())
                })
                .unwrap(),
            "tool" => h
                .state_mut()
                .doc_mut()
                .unwrap()
                .view
                .set_board_tool(BoardTool::Pen),
            "armed-tool" => h
                .state_mut()
                .doc_mut()
                .unwrap()
                .view
                .set_board_tool(BoardTool::Shape(ShapeKind::Ellipse)),
            "connector-tool" => h
                .state_mut()
                .doc_mut()
                .unwrap()
                .view
                .set_board_tool(BoardTool::Connector),
            "modal" => h.state_mut().dialogs.settings = true,
            _ => unreachable!(),
        }
        let before = h.state().doc().unwrap().doc.state_id();
        h.run_steps(5);
        assert!(
            h.query_by_role_and_label(Role::Button, "Text Color · Auto")
                .is_none(),
            "{reason}"
        );
        assert!(!h.ctx.text_edit_focused(), "{reason}");
        assert_eq!(h.state().doc().unwrap().doc.state_id(), before, "{reason}");
        if reason == "connector-tool" {
            // Enter creates a connector in this tool; disarm it before retrying the draft key.
            h.key_press(Key::Escape);
            h.run_steps(3);
        }
        h.key_press(Key::Enter);
        h.run_steps(3);
        assert_eq!(h.state().doc().unwrap().doc.state_id(), before, "{reason}");
        assert_eq!(
            object(h.state(), first).style.text_color,
            Some(Color::rgb(100, 120, 140)),
            "{reason}"
        );
        assert_eq!(object(h.state(), second).style.text_color, None, "{reason}");
    }
}

#[test]
fn shape_text_color_is_keyboard_reachable_in_compact_light_and_dark_windows() {
    for dark in [false, true] {
        for size in [vec2(320.0, 240.0), vec2(640.0, 360.0)] {
            let (mut app, id) = app_with_shape();
            app.settings.theme = if dark {
                thinoda_platform::settings::ThemeMode::Dark
            } else {
                thinoda_platform::settings::ThemeMode::Light
            };
            let mut h = harness_with_size(app, size);
            let canvas = h.state().doc().unwrap().view.canvas_rect();
            h.get_by_role_and_label(Role::Button, "Text Color").focus();
            h.run_steps(6);
            let rect = h.get_by_role_and_label(Role::Button, "Text Color").rect();
            assert!(canvas.contains_rect(rect), "{size:?}: {rect:?}");
            h.key_press(Key::Enter);
            h.run_steps(4);
            h.get_by_role_and_label(Role::ColorWell, "#2E7CF6").focus();
            h.run_steps(5);
            h.key_press(Key::Space);
            h.run_steps(4);
            assert_eq!(
                object(h.state(), id).style.text_color,
                Some(Color::rgb(0x2e, 0x7c, 0xf6))
            );
            close_popup(&mut h);
            assert_eq!(h.state().doc().unwrap().view.board_selection, [id]);
            assert_eq!(h.state().doc().unwrap().view.canvas_rect(), canvas);
        }
    }
}

#[test]
fn automatic_shape_text_picker_matches_canvas_contrast_without_changing_the_document() {
    for (fill, expected) in [(Color::BLACK, Color::WHITE), (Color::WHITE, Color::BLACK)] {
        let (mut app, id) = app_with_shape();
        app.doc_mut()
            .unwrap()
            .doc
            .update_canvas_object("Set fill", id, |object| {
                object.style.fill = Some(fill);
            })
            .unwrap();
        assert_eq!(
            app.shape_context().unwrap().properties.auto_text_color,
            expected
        );
        let mut h = harness(app);
        let before = h.state().doc().unwrap().doc.state_id();
        click(&mut h, Role::Button, "Text Color");
        assert_eq!(
            h.get_by_role_and_label(Role::Button, "Text Color · Auto")
                .accesskit_node()
                .toggled(),
            Some(Toggled::True)
        );
        h.get_by_label("Custom color").click();
        h.run_steps(6);
        let red = h
            .query_all_by_role(Role::SpinButton)
            .find_map(|control| control.value().filter(|value| value.starts_with("R ")))
            .expect("custom color red channel");
        assert_eq!(red, format!("R {}", expected.r));
        assert_eq!(h.state().doc().unwrap().doc.state_id(), before);
        assert_eq!(object(h.state(), id).style.text_color, None);
    }
}

#[test]
fn shape_text_color_survives_native_clipboard_drawio_and_svg() {
    let (mut app, id) = app_with_shape();
    app.doc_mut()
        .unwrap()
        .doc
        .update_canvas_object("Set label", id, |object| {
            let CanvasObjectKind::Shape { label, .. } = &mut object.kind else {
                unreachable!()
            };
            *label = "Colored shape".into();
            object.style.opacity = 1.0;
        })
        .unwrap();
    let mut h = harness(app);
    click(&mut h, Role::Button, "Text Color");
    click(&mut h, Role::ColorWell, "#2E7CF6");
    let color = Some(Color::rgb(0x2e, 0x7c, 0xf6));
    let workbook = h.state().doc().unwrap().doc.workbook();
    let native =
        thinoda_io::export_bytes(workbook, thinoda_io::Format::Thinoda, &Default::default())
            .unwrap();
    assert_eq!(
        thinoda_io::import_bytes(thinoda_io::Format::Thinoda, &native).unwrap(),
        *workbook
    );
    let sheet = workbook.active();
    let layout = layout_sheet(sheet, &ApproxMeasure, false);
    let payload = copy_board_selection(sheet, &layout, &[], &[id])
        .unwrap()
        .unwrap();
    let payload = decode_board_payload(&encode_board_payload(&payload).unwrap()).unwrap();
    let mut destination = Sheet::new("Paste", "Root");
    let pasted = paste_board_selection(&mut destination, &payload, Vec2::new(10.0, 20.0)).unwrap();
    assert_eq!(
        destination
            .canvas
            .object(pasted.objects[0])
            .unwrap()
            .style
            .text_color,
        color
    );

    let drawio =
        thinoda_io::export_bytes(workbook, thinoda_io::Format::DrawIo, &Default::default())
            .unwrap();
    assert!(String::from_utf8_lossy(&drawio).contains("fontColor=#2E7CF6"));
    let reopened =
        thinoda_io::import_bytes_with_report(thinoda_io::Format::DrawIo, &drawio).unwrap();
    let shape = reopened
        .workbook
        .active()
        .canvas
        .objects
        .iter()
        .find(|object| object.label() == "Colored shape")
        .unwrap();
    assert_eq!(shape.style.text_color, color);

    let svg = String::from_utf8(
        thinoda_io::export_bytes(workbook, thinoda_io::Format::Svg, &Default::default()).unwrap(),
    )
    .unwrap();
    let end = svg.find(">Colored shape</text>").unwrap();
    let start = svg[..end].rfind("<text ").unwrap();
    assert!(svg[start..end].contains("fill=\"#2E7CF6\""));
}

#[test]
fn every_shape_kind_accepts_text_color_even_when_the_label_is_empty() {
    let (mut app, id) = app_with_shape();
    for kind in ShapeKind::ALL {
        app.doc_mut()
            .unwrap()
            .doc
            .update_canvas_object("Set kind", id, |object| {
                object.kind = CanvasObjectKind::Shape {
                    shape: kind,
                    label: String::new(),
                };
                object.style.text_color = None;
            })
            .unwrap();
        let context = app.shape_context().unwrap();
        app.apply_shape_change(
            &context.target,
            ShapeChange::TextColor(Some(Color::rgb(20, 40, 60))),
        )
        .unwrap();
        let current = object(&app, id);
        assert_eq!(current.label(), "", "{kind:?}");
        assert_eq!(current.style.text_color, Some(Color::rgb(20, 40, 60)));
    }
}

#[test]
fn selected_shape_controls_edit_shape_fill_and_outline_as_single_undo_steps() {
    for expanded in [false, true] {
        let (app, id) = app_with_shape();
        let mut h = harness(app);
        if expanded {
            click(&mut h, Role::Button, "Expand board toolbar");
        }
        assert!(h.query_by_label("Connector route").is_none());
        let original = object(h.state(), id);
        let initial = h.state().doc().unwrap().doc.state_id();

        click(&mut h, Role::Button, "Change shape");
        click(&mut h, Role::Button, "Diamond");
        assert_eq!(shape_kind(h.state(), id), ShapeKind::Diamond);
        close_popup(&mut h);

        click(&mut h, Role::Button, "Fill color");
        click(&mut h, Role::ColorWell, "#2E7CF6");
        assert_eq!(
            object(h.state(), id).style.fill,
            Some(Color::rgb(0x2e, 0x7c, 0xf6))
        );
        close_popup(&mut h);

        click(&mut h, Role::Button, "Outline color");
        click(&mut h, Role::ColorWell, "#2E7CF6");
        assert_eq!(
            object(h.state(), id).style.stroke,
            Some(Color::rgb(0x2e, 0x7c, 0xf6))
        );
        close_popup(&mut h);

        click(&mut h, Role::Button, "Outline width");
        click(&mut h, Role::Button, "Outline width: 8");
        assert_eq!(object(h.state(), id).style.stroke_width, 8.0);
        close_popup(&mut h);

        click(&mut h, Role::Button, "Fill color");
        click(&mut h, Role::Button, "Fill color · No fill");
        assert_eq!(object(h.state(), id).style.fill, None);
        close_popup(&mut h);

        let edited = object(h.state(), id);
        assert_eq!(edited.rect, original.rect);
        assert_eq!(edited.alt_text, original.alt_text);
        assert_eq!(edited.style.opacity, original.style.opacity);

        // Shape, fill, outline color, outline width and "No fill" are five undo steps.
        for _ in 0..5 {
            undo(&mut h);
        }
        assert_eq!(h.state().doc().unwrap().doc.state_id(), initial);
        assert_eq!(object(h.state(), id), original);
    }
}

#[test]
fn outline_width_pins_the_automatic_outline_color() {
    let (app, id) = app_with_shape();
    let auto = app.shape_context().unwrap().properties.auto_stroke;
    assert_eq!(object(&app, id).style.stroke, None);
    let mut h = harness(app);
    click(&mut h, Role::Button, "Outline width");
    click(&mut h, Role::Button, "Outline width: 8");
    let style = object(h.state(), id).style;
    assert_eq!(style.stroke, Some(auto));
    assert_eq!(style.stroke_width, 8.0);
    close_popup(&mut h);

    // An explicit outline color is never overwritten by a width change.
    click(&mut h, Role::Button, "Outline color");
    click(&mut h, Role::ColorWell, "#2E7CF6");
    close_popup(&mut h);
    click(&mut h, Role::Button, "Outline width");
    click(&mut h, Role::Button, "Outline width: 2");
    let style = object(h.state(), id).style;
    assert_eq!(style.stroke, Some(Color::rgb(0x2e, 0x7c, 0xf6)));
    assert_eq!(style.stroke_width, 2.0);
}

#[test]
fn outline_auto_restores_the_inherited_outline() {
    let (mut app, id) = app_with_shape();
    app.doc_mut()
        .unwrap()
        .doc
        .update_canvas_object("Outline", id, |object| {
            object.style.stroke = Some(Color::rgb(20, 40, 60));
        })
        .unwrap();
    let mut h = harness(app);
    click(&mut h, Role::Button, "Outline color");
    click(&mut h, Role::Button, "Outline color · Auto");
    assert_eq!(object(h.state(), id).style.stroke, None);
    undo(&mut h);
    assert_eq!(
        object(h.state(), id).style.stroke,
        Some(Color::rgb(20, 40, 60))
    );
}

#[test]
fn locked_grouped_and_mixed_shape_selections_cannot_be_edited() {
    for case in ["locked", "group", "mixed"] {
        let (mut app, id) = app_with_shape();
        let doc = app.doc_mut().unwrap();
        if case == "locked" {
            doc.doc.set_canvas_objects_locked(&[id], true).unwrap();
        } else {
            let other = doc
                .doc
                .add_canvas_object(CanvasObject::shape(
                    ShapeKind::Ellipse,
                    BoardRect::from_center_size(Vec2::new(300.0, 0.0), Vec2::new(60.0, 40.0)),
                ))
                .unwrap();
            if case == "mixed" {
                doc.view.board_selection.push(other);
            } else {
                doc.doc
                    .edit_sheet("Group", |sheet| {
                        sheet
                            .group_board_members(&[
                                BoardMember::Object(id),
                                BoardMember::Object(other),
                            ])
                            .map_err(thinoda_core::EditError::Invalid)?;
                        Ok(())
                    })
                    .unwrap();
            }
        }
        if case == "mixed" {
            assert!(app.shape_context().is_none());
            continue;
        }
        let context = app.shape_context().unwrap();
        assert!(!context.enabled, "{case}");
        let before = app.doc().unwrap().doc.workbook().clone();
        assert!(
            app.apply_shape_change(&context.target, ShapeChange::Fill(Some(Color::WHITE)))
                .is_err()
        );
        assert!(app.edit_board_label(&context.target).is_err());
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        let h = harness(app);
        for label in ["Change shape", "Fill color", "Text Color", "Edit label"] {
            let control = h.get_by_role_and_label(Role::Button, label);
            assert!(control.accesskit_node().is_disabled(), "{case}: {label}");
        }
    }
}

#[test]
fn shape_toolbar_preserves_or_clears_rounding_as_the_kind_changes() {
    use thinoda_core::{DrawioRectangleStencil, ShapeRounding};

    let (mut app, id) = app_with_shape();
    let rounding = ShapeRounding {
        radius: 18.0,
        stencil: DrawioRectangleStencil::Misc,
    };
    app.doc_mut()
        .unwrap()
        .doc
        .update_canvas_object("Rounding", id, |object| object.rounding = Some(rounding))
        .unwrap();
    let target = app.shape_context().unwrap().target;
    let target = app
        .apply_shape_change(&target, ShapeChange::Shape(ShapeKind::Rectangle))
        .unwrap();
    assert_eq!(object(&app, id).rounding, Some(rounding));
    app.apply_shape_change(&target, ShapeChange::Shape(ShapeKind::Ellipse))
        .unwrap();
    assert_eq!(shape_kind(&app, id), ShapeKind::Ellipse);
    assert_eq!(object(&app, id).rounding, None);
    assert!(app.doc_mut().unwrap().doc.undo());
    assert_eq!(shape_kind(&app, id), ShapeKind::Rectangle);
    assert_eq!(object(&app, id).rounding, Some(rounding));
}

#[test]
fn stale_shape_targets_are_rejected() {
    let (mut app, id) = app_with_shape();
    let target = app.shape_context().unwrap().target;
    app.doc_mut()
        .unwrap()
        .doc
        .update_canvas_object("Move", id, |object| {
            object.rect = object.rect.translate(Vec2::new(10.0, 0.0));
        })
        .unwrap();
    let state = app.doc().unwrap().doc.state_id();
    assert!(
        app.apply_shape_change(&target, ShapeChange::Shape(ShapeKind::Star))
            .is_err()
    );
    assert!(
        app.apply_shape_change(&target, ShapeChange::TextColor(Some(Color::WHITE)))
            .is_err()
    );
    assert!(app.edit_board_label(&target).is_err());
    assert_eq!(app.doc().unwrap().doc.state_id(), state);
    let fresh = app.shape_context().unwrap().target;
    assert!(
        app.apply_shape_change(&fresh, ShapeChange::Shape(ShapeKind::Star))
            .is_ok()
    );
    assert_eq!(shape_kind(&app, id), ShapeKind::Star);
}

#[test]
fn changing_selection_closes_open_shape_popups() {
    let (mut app, id) = app_with_shape();
    let other = app
        .doc_mut()
        .unwrap()
        .doc
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Ellipse,
            BoardRect::from_center_size(Vec2::new(300.0, 0.0), Vec2::new(60.0, 40.0)),
        ))
        .unwrap();
    let mut h = harness(app);
    for (next, control, item) in [
        (other, "Fill color", "Fill color · No fill"),
        (id, "Text Color", "Text Color · Auto"),
    ] {
        click(&mut h, Role::Button, control);
        assert!(h.query_by_label(item).is_some());
        h.state_mut()
            .doc_mut()
            .unwrap()
            .view
            .select_board_object(next);
        h.run_steps(4);
        assert!(h.query_by_label(item).is_none());
        assert!(h.query_by_label("Change shape").is_some());
    }
}

#[test]
fn edit_label_starts_inline_editing_and_format_shortcut_toggles_the_panel() {
    let (app, id) = app_with_shape();
    let mut h = harness(app);
    click(&mut h, Role::Button, "Edit label");
    assert_eq!(
        h.state().doc().unwrap().view.editing_board_object(),
        Some(id)
    );
    close_popup(&mut h);

    assert!(!h.state().settings.show_format_panel);
    click(&mut h, Role::Button, "More formatting options");
    assert!(h.state().settings.show_format_panel);
    assert_eq!(
        h.get_by_role_and_label(Role::Button, "More formatting options")
            .accesskit_node()
            .toggled(),
        Some(Toggled::True)
    );
    click(&mut h, Role::Button, "More formatting options");
    assert!(!h.state().settings.show_format_panel);
}

#[test]
fn connectors_get_the_same_label_and_format_shortcuts() {
    let (mut app, _) = app_with_shape();
    let doc = app.doc_mut().unwrap();
    let id = doc
        .doc
        .add_canvas_object(CanvasObject::connector(thinoda_core::CanvasConnector::new(
            thinoda_core::Endpoint::Point(Vec2::new(-120.0, 200.0)),
            thinoda_core::Endpoint::Point(Vec2::new(120.0, 240.0)),
        )))
        .unwrap();
    doc.view.select_board_object(id);
    let mut h = harness(app);
    assert!(h.query_by_label("Change shape").is_none());
    assert!(h.query_by_label("Connector route").is_some());
    click(&mut h, Role::Button, "Edit label");
    assert_eq!(
        h.state().doc().unwrap().view.editing_board_object(),
        Some(id)
    );
}

#[cfg(feature = "gpu-tests")]
#[test]
fn shape_context_visual_previews() {
    let Some(directory) = std::env::var_os("THINODA_SHAPE_VISUAL_DIR") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    for dark in [false, true] {
        for (size, width) in [
            (vec2(1200.0, 700.0), "wide"),
            (vec2(320.0, 240.0), "compact"),
        ] {
            let (mut app, _) = app_with_shape();
            app.settings.theme = if dark {
                thinoda_platform::settings::ThemeMode::Dark
            } else {
                thinoda_platform::settings::ThemeMode::Light
            };
            let mut h = Harness::builder()
                .wgpu()
                .with_size(size)
                .with_step_dt(1.0 / 60.0)
                .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
            h.run_steps(6);
            let theme = if dark { "dark" } else { "light" };
            h.render()
                .expect("shape toolbar render")
                .save(directory.join(format!("shape-toolbar-{theme}-{width}.png")))
                .unwrap();
            click(&mut h, Role::Button, "Text Color");
            h.render()
                .expect("shape text color render")
                .save(directory.join(format!("shape-text-{theme}-{width}.png")))
                .unwrap();
            h.get_by_label("Custom color").scroll_to_me();
            h.run_steps(4);
            h.get_by_label("Custom color").click();
            h.run_steps(5);
            h.render()
                .expect("shape custom text color render")
                .save(directory.join(format!("shape-custom-{theme}-{width}.png")))
                .unwrap();
        }
    }
}
