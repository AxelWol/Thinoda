use super::*;
use egui::{Key, Modifiers, PointerButton, Role, accesskit::Toggled};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use thinoda_core::{
    CanvasConnector, Endpoint, Rect as BoardRect, Sheet, Vec2,
    clipboard::{
        copy_board_selection, decode_board_payload, encode_board_payload, paste_board_selection,
    },
    layout::{ApproxMeasure, layout_sheet},
    templates,
};

fn app_with_connector() -> (ThinodaApp, CanvasObjectId) {
    let mut app = ThinodaApp::headless();
    app.settings.ai.enabled = false;
    app.settings.show_format_panel = false;
    app.new_doc(templates::instantiate("blank-canvas"));
    let doc = app.doc_mut().unwrap();
    let mut connector = CanvasConnector::new(
        Endpoint::Point(Vec2::new(-120.0, -20.0)),
        Endpoint::Point(Vec2::new(120.0, 40.0)),
    );
    connector.label = "Keep connector label".into();
    connector.color = Some(Color::rgba(100, 120, 140, 96));
    let mut object = CanvasObject::connector(connector);
    object.alt_text = "Keep connector description".into();
    object.style.stroke = Some(Color::rgb(20, 40, 60));
    object.style.opacity = 0.75;
    let id = doc.doc.add_canvas_object(object).unwrap();
    doc.view.select_board_object(id);
    (app, id)
}

fn harness(app: ThinodaApp, size: egui::Vec2) -> Harness<'static, ThinodaApp> {
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

fn connector(app: &ThinodaApp, id: CanvasObjectId) -> CanvasConnector {
    let CanvasObjectKind::Connector(connector) = &app
        .doc()
        .unwrap()
        .doc
        .sheet()
        .canvas
        .object(id)
        .unwrap()
        .kind
    else {
        panic!("expected connector");
    };
    connector.clone()
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

#[test]
fn selected_connector_properties_overlay_above_stationary_toolbox() {
    for size in [vec2(320.0, 240.0), vec2(1000.0, 700.0)] {
        let (mut app, id) = app_with_connector();
        app.doc_mut().unwrap().view.board_selection.clear();
        let mut h = harness(app, size);
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
                    "thinoda_connector_options",
                    egui::ViewportId::ROOT,
                )))
            })
            .expect("selected connector has its own options strip");
        assert_eq!(h.state().doc().unwrap().view.canvas_rect(), canvas);
        assert_eq!(h.ctx.memory(|memory| memory.area_rect(rail_id)), Some(rail));
        assert!(canvas.contains_rect(strip), "{size:?}: {strip:?}");
        assert!(!rail.intersects(strip), "{size:?}: {rail:?} {strip:?}");
        let route = h
            .get_by_role_and_label(Role::Button, "Connector route")
            .rect();
        assert!(strip.contains_rect(route), "{size:?}: {route:?}");

        click(&mut h, Role::Button, "Arrow at start");
        assert!(connector(h.state(), id).arrow_start);
        assert_eq!(h.state().doc().unwrap().view.board_selection, [id]);
        assert_eq!(h.state().doc().unwrap().view.canvas_rect(), canvas);

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
fn connector_options_keep_their_popup_on_a_tiny_canvas() {
    let (app, id) = app_with_connector();
    let mut h = harness(app, vec2(320.0, 170.0));
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
    click(&mut h, Role::Button, "Connector route");
    assert!(h.query_by_role_and_label(Role::Button, "Curve").is_some());
    click(&mut h, Role::Button, "Curve");
    assert_eq!(connector(h.state(), id).route, ConnectorRoute::Curve);
    assert_eq!(h.state().doc().unwrap().view.canvas_rect(), canvas);

    h.state_mut()
        .doc_mut()
        .unwrap()
        .view
        .board_selection
        .clear();
    h.run_steps(6);
    assert!(rail_visible(&h));
    assert!(h.query_by_role_and_label(Role::Button, "Curve").is_none());
}

#[test]
fn connector_options_label_is_translated_in_every_supported_language() {
    for language in i18n::Lang::ALL {
        let translation = i18n::t_in(language, "Connector options");
        assert!(!translation.is_empty(), "{language:?}");
        if language != i18n::Lang::En {
            assert_ne!(translation, "Connector options", "{language:?}");
        }
    }
}

#[test]
fn arrow_toggle_clears_custom_marker_and_undo_restores_it() {
    let (mut app, id) = app_with_connector();
    app.doc_mut()
        .unwrap()
        .doc
        .update_canvas_object("Set marker", id, |object| {
            let CanvasObjectKind::Connector(connector) = &mut object.kind else {
                panic!("expected connector");
            };
            connector.end_marker = Some(ConnectorMarker {
                kind: ConnectorMarkerKind::Open,
            });
            connector.end_marker_size = Some(12.5);
            connector.end_marker_unfilled = true;
        })
        .unwrap();
    let mut h = harness(app, vec2(1000.0, 700.0));
    click(&mut h, Role::Button, "Arrow at end");
    let current = connector(h.state(), id);
    assert!(!current.arrow_end);
    assert_eq!(current.end_marker, None);
    assert_eq!(current.end_marker_size, None);
    assert!(!current.end_marker_unfilled);
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    let restored = connector(h.state(), id);
    assert!(restored.arrow_end);
    assert_eq!(restored.end_marker.unwrap().kind, ConnectorMarkerKind::Open);
    assert_eq!(restored.end_marker_size, Some(12.5));
    assert!(restored.end_marker_unfilled);
    assert!(h.state_mut().doc_mut().unwrap().doc.redo());
    let redone = connector(h.state(), id);
    assert_eq!(redone.end_marker, None);
    assert_eq!(redone.end_marker_size, None);
    assert!(!redone.end_marker_unfilled);
}

#[test]
fn selected_connector_controls_change_only_existing_properties_and_round_trip() {
    for expanded in [false, true] {
        let (app, id) = app_with_connector();
        let mut h = harness(app, vec2(1000.0, 700.0));
        if expanded {
            click(&mut h, Role::Button, "Expand board toolbar");
        }
        let canvas = h.state().doc().unwrap().view.canvas_rect();
        let original = h
            .state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .canvas
            .object(id)
            .unwrap()
            .clone();
        let before = h.state().doc().unwrap().doc.state_id();

        click(&mut h, Role::Button, "Connector route");
        assert_eq!(
            h.get_by_role_and_label(Role::Button, "Straight")
                .accesskit_node()
                .toggled(),
            Some(Toggled::True)
        );
        click(&mut h, Role::Button, "Curve");
        assert_eq!(connector(h.state(), id).route, ConnectorRoute::Curve);
        assert_eq!(
            h.get_by_role_and_label(Role::Button, "Curve")
                .accesskit_node()
                .toggled(),
            Some(Toggled::True)
        );
        close_popup(&mut h);
        click(&mut h, Role::Button, "Arrow at start");
        click(&mut h, Role::Button, "Arrow at end");
        assert!(connector(h.state(), id).arrow_start);
        assert!(!connector(h.state(), id).arrow_end);

        click(&mut h, Role::Button, "Connector width");
        click(&mut h, Role::Button, "Connector width: 8");
        assert_eq!(connector(h.state(), id).width, 8.0);
        close_popup(&mut h);
        click(&mut h, Role::Button, "Connector color");
        click(&mut h, Role::ColorWell, "#2E7CF6");
        assert_eq!(
            connector(h.state(), id).color,
            Some(Color::rgba(0x2e, 0x7c, 0xf6, 96))
        );
        close_popup(&mut h);

        let object = h
            .state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .canvas
            .object(id)
            .unwrap();
        assert_eq!(object.rect, original.rect);
        assert_eq!(object.rotation, original.rotation);
        assert_eq!(object.style, original.style);
        assert_eq!(object.alt_text, original.alt_text);
        let current = connector(h.state(), id);
        let CanvasObjectKind::Connector(previous) = original.kind else {
            unreachable!()
        };
        assert_eq!(
            (current.from, current.to, &current.label),
            (previous.from, previous.to, &previous.label)
        );
        assert_ne!(h.state().doc().unwrap().doc.state_id(), before);
        assert_eq!(h.state().doc().unwrap().view.canvas_rect(), canvas);
        assert_eq!(h.state().doc().unwrap().view.board_selection, [id]);

        assert!(h.state_mut().doc_mut().unwrap().doc.undo());
        assert_eq!(connector(h.state(), id).color, previous.color);
        assert!(h.state_mut().doc_mut().unwrap().doc.redo());
        assert_eq!(connector(h.state(), id), current);
        let workbook = h.state().doc().unwrap().doc.workbook();
        let native =
            thinoda_io::export_bytes(workbook, thinoda_io::Format::Thinoda, &Default::default())
                .unwrap();
        let reopened = thinoda_io::import_bytes(thinoda_io::Format::Thinoda, &native).unwrap();
        assert_eq!(reopened, *workbook);

        let sheet = workbook.active();
        let layout = layout_sheet(sheet, &ApproxMeasure, false);
        let payload = copy_board_selection(sheet, &layout, &[], &[id])
            .unwrap()
            .unwrap();
        let payload = decode_board_payload(&encode_board_payload(&payload).unwrap()).unwrap();
        let mut destination = Sheet::new("Paste", "Root");
        let pasted =
            paste_board_selection(&mut destination, &payload, Vec2::new(10.0, 20.0)).unwrap();
        let CanvasObjectKind::Connector(pasted) =
            &destination.canvas.object(pasted.objects[0]).unwrap().kind
        else {
            panic!("expected pasted connector");
        };
        assert_eq!(
            (
                pasted.route,
                pasted.arrow_start,
                pasted.arrow_end,
                pasted.color,
                pasted.width
            ),
            (
                current.route,
                current.arrow_start,
                current.arrow_end,
                current.color,
                current.width
            )
        );
        assert_eq!(pasted.label, current.label);

        let svg = String::from_utf8(
            thinoda_io::export_bytes(workbook, thinoda_io::Format::Svg, &Default::default())
                .unwrap(),
        )
        .unwrap();
        let group = svg
            .split(&format!("id=\"object-{}\"", id.0))
            .nth(1)
            .unwrap()
            .split("</g>")
            .next()
            .unwrap();
        assert!(group.contains("stroke=\"#2E7CF6"), "{group}");
        assert!(group.contains("stroke-width=\"8.00\""), "{group}");
        assert_eq!(group.matches("<path ").count(), 2, "{group}");
    }
}

#[test]
fn clear_connector_waypoints_is_guarded_undoable_and_disabled_when_empty() {
    let (mut app, id) = app_with_connector();
    app.doc_mut()
        .unwrap()
        .doc
        .update_canvas_object("Add waypoints", id, |object| {
            if let CanvasObjectKind::Connector(connector) = &mut object.kind {
                connector.waypoints = vec![Vec2::new(-20.0, 80.0), Vec2::new(50.0, 70.0)].into();
            }
        })
        .unwrap();
    let original = connector(&app, id);
    let stale = app.connector_context().unwrap().target;
    app.doc_mut()
        .unwrap()
        .doc
        .update_canvas_object("Change description", id, |object| {
            object.alt_text = "New description".into();
        })
        .unwrap();
    assert!(
        app.apply_connector_change(&stale, ConnectorChange::ClearWaypoints)
            .is_err()
    );
    assert_eq!(connector(&app, id), original);

    let mut h = harness(app, vec2(1000.0, 700.0));
    let before = h.state().doc().unwrap().doc.workbook().clone();
    click(&mut h, Role::Button, "Connector route");
    let clear = h.get_by_role_and_label(Role::Button, "Clear connector waypoints");
    assert!(!clear.accesskit_node().is_disabled());
    click(&mut h, Role::Button, "Clear connector waypoints");
    let mut expected = original.clone();
    expected.waypoints = Default::default();
    assert_eq!(connector(h.state(), id), expected);
    close_popup(&mut h);
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    assert!(h.state_mut().doc_mut().unwrap().doc.redo());
    h.run_steps(4);
    click(&mut h, Role::Button, "Connector route");
    assert!(
        h.get_by_role_and_label(Role::Button, "Clear connector waypoints")
            .accesskit_node()
            .is_disabled()
    );
}

#[test]
fn inspector_clears_connector_waypoints_without_changing_other_fields() {
    let (mut app, id) = app_with_connector();
    app.doc_mut()
        .unwrap()
        .doc
        .update_canvas_object("Add waypoint", id, |object| {
            if let CanvasObjectKind::Connector(connector) = &mut object.kind {
                connector.waypoints = vec![Vec2::new(0.0, 90.0)].into();
            }
        })
        .unwrap();
    let original = connector(&app, id);
    let mut h = harness(app, vec2(1200.0, 800.0));
    click(&mut h, Role::Button, "More formatting options");
    click(&mut h, Role::Button, "Clear connector waypoints");
    let mut expected = original.clone();
    expected.waypoints = Default::default();
    assert_eq!(connector(h.state(), id), expected);
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    assert_eq!(connector(h.state(), id), original);
}

#[test]
fn controls_support_keyboard_focus_and_exact_width() {
    let (app, id) = app_with_connector();
    let mut h = harness(app, vec2(900.0, 700.0));
    let route = h.get_by_role_and_label(Role::Button, "Connector route");
    route.focus();
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(4);
    h.get_by_role_and_label(Role::Button, "Elbow").focus();
    h.run_steps(2);
    h.key_press(Key::Space);
    h.run_steps(3);
    assert_eq!(connector(h.state(), id).route, ConnectorRoute::Elbow);
    close_popup(&mut h);
    assert!(
        h.get_by_role_and_label(Role::Button, "Connector route")
            .is_focused()
    );
    h.get_by_role_and_label(Role::Button, "Arrow at start")
        .focus();
    h.run_steps(2);
    h.key_press(Key::Space);
    h.run_steps(3);
    assert!(connector(h.state(), id).arrow_start);
    click(&mut h, Role::Button, "Connector width");
    let spin = h.get_by_role_and_label(Role::SpinButton, "Connector width");
    spin.scroll_to_me();
    h.run_steps(4);
    h.get_by_role_and_label(Role::SpinButton, "Connector width")
        .click();
    h.run_steps(2);
    assert!(
        h.query_by_role_and_label(Role::SpinButton, "Connector width")
            .is_some(),
        "width input disappeared after clicking"
    );
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    assert!(
        h.query_by_role_and_label(Role::SpinButton, "Connector width")
            .is_some(),
        "width input disappeared after selecting text"
    );
    h.get_by_role_and_label(Role::SpinButton, "Connector width")
        .type_text("13.5");
    h.key_press(Key::Enter);
    h.run_steps(4);
    assert_eq!(connector(h.state(), id).width, 13.5);
    assert_eq!(h.state().doc().unwrap().view.board_selection, [id]);
}

#[test]
fn escape_cancels_the_width_editor_without_changing_the_connector() {
    let (app, id) = app_with_connector();
    let mut h = harness(app, vec2(900.0, 700.0));
    let before = h.state().doc().unwrap().doc.state_id();
    click(&mut h, Role::Button, "Connector width");
    h.get_by_role_and_label(Role::SpinButton, "Connector width")
        .scroll_to_me();
    h.run_steps(4);
    h.get_by_role_and_label(Role::SpinButton, "Connector width")
        .click();
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.get_by_role_and_label(Role::SpinButton, "Connector width")
        .type_text("55");
    h.run_steps(2);
    assert_eq!(
        h.state().doc().unwrap().doc.state_id(),
        before,
        "typing should stay provisional until the edit is accepted"
    );
    h.key_press(Key::Escape);
    h.run_steps(5);
    assert_eq!(connector(h.state(), id).width, 2.0);
    assert_eq!(h.state().doc().unwrap().doc.state_id(), before);
    assert_eq!(h.state().doc().unwrap().view.board_selection, [id]);
    assert!(
        h.get_by_role_and_label(Role::Button, "Connector width")
            .is_focused()
    );
    assert!(!h.ctx.text_edit_focused());
}

#[test]
fn closing_a_stale_width_editor_releases_its_focus_and_draft() {
    for reason in ["selection", "revision", "mode", "modal"] {
        let (mut app, first) = app_with_connector();
        let second = app
            .doc_mut()
            .unwrap()
            .doc
            .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
                Endpoint::Point(Vec2::new(-100.0, 180.0)),
                Endpoint::Point(Vec2::new(100.0, 180.0)),
            )))
            .unwrap();
        let mut h = harness(app, vec2(900.0, 700.0));
        click(&mut h, Role::Button, "Connector width");
        h.get_by_role_and_label(Role::SpinButton, "Connector width")
            .scroll_to_me();
        h.run_steps(4);
        h.get_by_role_and_label(Role::SpinButton, "Connector width")
            .click();
        h.run_steps(2);
        let initial = h.state().doc().unwrap().doc.state_id();
        h.key_press_modifiers(Modifiers::COMMAND, Key::A);
        h.get_by_role_and_label(Role::SpinButton, "Connector width")
            .type_text("55");
        h.run_steps(3);
        assert_eq!(h.state().doc().unwrap().doc.state_id(), initial, "{reason}");
        assert!(h.ctx.text_edit_focused(), "{reason}");
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
            "mode" => h.state_mut().mode = CenterMode::Outliner,
            "modal" => h.state_mut().dialogs.settings = true,
            _ => unreachable!(),
        }
        let before = h.state().doc().unwrap().doc.state_id();
        h.run_steps(5);
        assert!(
            h.query_by_role_and_label(Role::SpinButton, "Connector width")
                .is_none(),
            "{reason}"
        );
        assert!(!h.ctx.text_edit_focused(), "{reason}");
        h.key_press(Key::Enter);
        h.run_steps(3);
        assert_eq!(h.state().doc().unwrap().doc.state_id(), before, "{reason}");
        assert_eq!(connector(h.state(), first).width, 2.0, "{reason}");
        assert_eq!(connector(h.state(), second).width, 2.0, "{reason}");
    }
}

#[test]
fn connector_controls_remain_reachable_in_compact_light_and_dark_windows() {
    for dark in [false, true] {
        for size in [vec2(320.0, 240.0), vec2(640.0, 360.0)] {
            for expanded in [false, true] {
                let (mut app, id) = app_with_connector();
                app.settings.theme = if dark {
                    thinoda_platform::settings::ThemeMode::Dark
                } else {
                    thinoda_platform::settings::ThemeMode::Light
                };
                let mut h = harness(app, size);
                let canvas = h.state().doc().unwrap().view.canvas_rect();
                if expanded {
                    click(&mut h, Role::Button, "Expand board toolbar");
                }
                for (menu, choice) in [
                    ("Connector route", "Elbow"),
                    ("Connector width", "Connector width: 8"),
                ] {
                    h.get_by_role_and_label(Role::Button, menu).focus();
                    h.run_steps(5);
                    let rect = h.get_by_role_and_label(Role::Button, menu).rect();
                    assert!(
                        h.ctx.content_rect().contains_rect(rect),
                        "{size:?}, canvas={canvas:?}, {menu}={rect:?}, expanded={expanded}, dark={dark}"
                    );
                    h.key_press(Key::Enter);
                    h.run_steps(5);
                    h.get_by_role_and_label(Role::Button, choice).focus();
                    h.run_steps(5);
                    let choice_rect = h.get_by_role_and_label(Role::Button, choice).rect();
                    assert!(
                        h.ctx.content_rect().contains_rect(choice_rect),
                        "{size:?} {choice_rect:?}"
                    );
                    h.key_press(Key::Space);
                    h.run_steps(4);
                    close_popup(&mut h);
                }
                assert_eq!(connector(h.state(), id).route, ConnectorRoute::Elbow);
                assert_eq!(connector(h.state(), id).width, 8.0);
                assert_eq!(h.state().doc().unwrap().view.board_selection, [id]);
                assert_eq!(h.state().doc().unwrap().view.canvas_rect(), canvas);
            }
        }
    }
}

#[test]
fn expanding_custom_connector_color_reveals_its_editor_in_a_compact_window() {
    let (app, _) = app_with_connector();
    let mut h = harness(app, vec2(320.0, 240.0));
    click(&mut h, Role::Button, "Connector color");
    h.get_by_label("Custom color").scroll_to_me();
    h.run_steps(5);
    h.get_by_label("Custom color").click();
    h.run_steps(12);
    let header = h.get_by_label("Custom color").rect();
    let popups = h
        .ctx
        .data(|data| data.get_temp::<ConnectorPopups>(connector_popup_slot(egui::ViewportId::ROOT)))
        .unwrap();
    let popup = h
        .ctx
        .memory(|memory| memory.area_rect(popups.ids[1]))
        .unwrap();
    assert!(
        popup.bottom() - header.bottom() >= 40.0,
        "custom color editor is hidden below its header: {header:?} in {popup:?}"
    );
}

#[cfg(feature = "gpu-tests")]
#[test]
fn connector_context_visual_previews() {
    let Some(directory) = std::env::var_os("THINODA_CONNECTOR_VISUAL_DIR") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    for dark in [false, true] {
        for (size, width) in [
            (vec2(1200.0, 700.0), "wide"),
            (vec2(320.0, 240.0), "compact"),
        ] {
            let (mut app, _) = app_with_connector();
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
                .expect("connector toolbar render")
                .save(directory.join(format!("connector-toolbar-{theme}-{width}.png")))
                .unwrap();
            for (menu, name) in [
                ("Connector route", "route"),
                ("Connector color", "color"),
                ("Connector width", "width"),
            ] {
                click(&mut h, Role::Button, menu);
                h.render()
                    .expect("connector popup render")
                    .save(directory.join(format!("connector-{name}-{theme}-{width}.png")))
                    .unwrap();
                if name == "color" {
                    h.get_by_label("Custom color").scroll_to_me();
                    h.run_steps(4);
                    h.get_by_label("Custom color").click();
                    h.run_steps(5);
                    h.render()
                        .expect("custom color render")
                        .save(directory.join(format!("connector-custom-{theme}-{width}.png")))
                        .unwrap();
                }
                close_popup(&mut h);
            }
            click(&mut h, Role::Button, "Expand board toolbar");
            h.render()
                .expect("expanded connector toolbar render")
                .save(directory.join(format!("connector-expanded-{theme}-{width}.png")))
                .unwrap();
        }
    }
}

#[test]
fn auto_color_preserves_inherited_stroke_and_is_undoable() {
    let (app, id) = app_with_connector();
    let mut h = harness(app, vec2(900.0, 700.0));
    click(&mut h, Role::Button, "Connector color");
    click(&mut h, Role::Button, "Connector color · Auto");
    assert_eq!(connector(h.state(), id).color, None);
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .canvas
            .object(id)
            .unwrap()
            .style
            .stroke,
        Some(Color::rgb(20, 40, 60))
    );
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    assert_eq!(
        connector(h.state(), id).color,
        Some(Color::rgba(100, 120, 140, 96))
    );
    assert!(h.state_mut().doc_mut().unwrap().doc.redo());
    assert_eq!(connector(h.state(), id).color, None);
    let svg = String::from_utf8(
        thinoda_io::export_bytes(
            h.state().doc().unwrap().doc.workbook(),
            thinoda_io::Format::Svg,
            &Default::default(),
        )
        .unwrap(),
    )
    .unwrap();
    let group = svg
        .split(&format!("id=\"object-{}\"", id.0))
        .nth(1)
        .unwrap()
        .split("</g>")
        .next()
        .unwrap();
    assert!(group.contains("stroke=\"#14283C"), "{group}");
}

#[test]
fn stale_revisions_documents_sheets_viewports_and_selections_are_rejected() {
    for case in ["revision", "document", "sheet", "viewport", "selection"] {
        let (mut app, id) = app_with_connector();
        let target = app.connector_context().unwrap().target;
        match case {
            "revision" => app
                .doc_mut()
                .unwrap()
                .doc
                .edit_sheet("Other edit", |sheet| {
                    sheet.board.snap = !sheet.board.snap;
                    Ok(())
                })
                .unwrap(),
            "document" => {
                let doc = app.doc_mut().unwrap();
                doc.doc.replace(doc.doc.workbook().clone());
            }
            "sheet" => {
                let doc = app.doc_mut().unwrap();
                let index = doc.doc.add_sheet(Sheet::new("Other", "Root")).unwrap();
                doc.switch_sheet(index).unwrap();
            }
            "viewport" => app.current_viewport = egui::ViewportId::from_hash_of("other window"),
            "selection" => {
                let doc = app.doc_mut().unwrap();
                let other = doc
                    .doc
                    .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
                        Endpoint::Point(Vec2::ZERO),
                        Endpoint::Point(Vec2::new(80.0, 0.0)),
                    )))
                    .unwrap();
                doc.view.select_board_object(other);
            }
            _ => unreachable!(),
        }
        let state = app.doc().unwrap().doc.state_id();
        let book = app.doc().unwrap().doc.workbook().clone();
        assert!(
            app.apply_connector_change(&target, ConnectorChange::ArrowStart(true))
                .is_err(),
            "{case}"
        );
        assert_eq!(app.doc().unwrap().doc.state_id(), state, "{case}");
        assert_eq!(app.doc().unwrap().doc.workbook(), &book, "{case}");
        if case != "sheet" {
            assert!(!connector(&app, id).arrow_start, "{case}");
        }
    }
}

#[test]
fn locked_grouped_and_mixed_selections_cannot_edit_a_connector() {
    for case in ["locked", "group", "locked-member", "locked-group", "mixed"] {
        let (mut app, id) = app_with_connector();
        if case == "locked" {
            app.doc_mut()
                .unwrap()
                .doc
                .set_canvas_objects_locked(&[id], true)
                .unwrap();
        } else {
            let doc = app.doc_mut().unwrap();
            let shape = doc
                .doc
                .add_canvas_object(CanvasObject::shape(
                    ShapeKind::Rectangle,
                    BoardRect::from_center_size(Vec2::new(300.0, 0.0), Vec2::new(60.0, 40.0)),
                ))
                .unwrap();
            if case == "mixed" {
                doc.view.board_selection.push(shape);
            } else {
                doc.doc
                    .edit_sheet("Group", |sheet| {
                        let group = sheet
                            .group_board_members(&[
                                BoardMember::Object(id),
                                BoardMember::Object(shape),
                            ])
                            .map_err(thinoda_core::EditError::Invalid)?;
                        if case == "locked-member" {
                            sheet
                                .set_board_members_locked(&[BoardMember::Object(shape)], true)
                                .map_err(thinoda_core::EditError::Invalid)?;
                        } else if case == "locked-group" {
                            sheet
                                .set_canvas_group_locked(group, true)
                                .map_err(thinoda_core::EditError::Invalid)?;
                        }
                        Ok(())
                    })
                    .unwrap();
            }
        }
        if case == "mixed" {
            assert!(app.connector_context().is_none());
            continue;
        }
        let context = app.connector_context().unwrap();
        assert!(!context.enabled, "{case}");
        let before = app.doc().unwrap().doc.workbook().clone();
        assert!(
            app.apply_connector_change(
                &context.target,
                ConnectorChange::Route(ConnectorRoute::Curve)
            )
            .is_err()
        );
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        let h = harness(app, vec2(1000.0, 700.0));
        if let Some(control) = h.query_by_role_and_label(Role::Button, "Connector route") {
            assert!(control.accesskit_node().is_disabled(), "{case}");
        }
    }
}

#[test]
fn switching_selection_or_locking_during_a_press_cannot_apply_a_stale_control() {
    for lock in [false, true] {
        let (mut app, id) = app_with_connector();
        let second = app
            .doc_mut()
            .unwrap()
            .doc
            .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
                Endpoint::Point(Vec2::new(-100.0, 180.0)),
                Endpoint::Point(Vec2::new(100.0, 180.0)),
            )))
            .unwrap();
        let mut h = harness(app, vec2(1000.0, 700.0));
        click(&mut h, Role::Button, "Connector route");
        let point = h
            .get_by_role_and_label(Role::Button, "Curve")
            .rect()
            .center();
        h.hover_at(point);
        h.run_steps(1);
        h.event(egui::Event::PointerButton {
            pos: point,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(1);
        if lock {
            h.state_mut()
                .doc_mut()
                .unwrap()
                .doc
                .set_canvas_objects_locked(&[id], true)
                .unwrap();
        } else {
            h.state_mut()
                .doc_mut()
                .unwrap()
                .view
                .select_board_object(second);
        }
        h.run_steps(3);
        let state = h.state().doc().unwrap().doc.state_id();
        h.event(egui::Event::PointerButton {
            pos: point,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(4);
        assert_eq!(h.state().doc().unwrap().doc.state_id(), state);
        assert_eq!(connector(h.state(), id).route, ConnectorRoute::Straight);
        assert_eq!(connector(h.state(), second).route, ConnectorRoute::Straight);
        assert!(h.query_by_role_and_label(Role::Button, "Curve").is_none());
    }
}

#[test]
fn selected_connector_drafts_and_modes_preserve_their_owners() {
    let (mut app, id) = app_with_connector();
    let target = app.connector_context().unwrap().target;
    {
        let doc = app.doc_mut().unwrap();
        doc.inspector_edit = Some(crate::doc::InspectorEdit {
            buffer_id: None,
            sheet_index: doc.doc.active_index(),
            sheet_root: doc.doc.sheet().root,
            topic: doc.doc.sheet().root,
            field: crate::doc::InspectorField::Notes,
            original: None,
            format_notes: false,
            text: "Pending note draft".into(),
        });
    }
    let before = app.doc().unwrap().doc.workbook().clone();
    assert!(!app.connector_context().unwrap().enabled);
    assert!(
        app.apply_connector_change(&target, ConnectorChange::Width(8.0))
            .is_err()
    );
    assert_eq!(
        app.doc().unwrap().inspector_edit.as_ref().unwrap().text,
        "Pending note draft"
    );
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    app.doc_mut().unwrap().inspector_edit = None;
    {
        let doc = app.doc_mut().unwrap();
        assert!(doc.view.begin_board_label(&doc.doc, id));
    }
    assert!(app.connector_context().is_none());
    assert!(
        app.apply_connector_change(&target, ConnectorChange::Width(8.0))
            .is_err()
    );
    assert_eq!(app.doc().unwrap().view.editing_board_object(), Some(id));
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);

    let (app, id) = app_with_connector();
    let mut h = harness(app, vec2(900.0, 700.0));
    click(&mut h, Role::Button, "Connector route");
    h.state_mut()
        .doc_mut()
        .unwrap()
        .view
        .set_board_tool(BoardTool::Pen);
    h.run_steps(5);
    assert!(h.query_by_role_and_label(Role::Button, "Curve").is_none());
    assert!(
        h.query_by_role_and_label(Role::Button, "Ink color")
            .is_some()
    );
    assert!(
        h.query_by_role_and_label(Role::Button, "Connector route")
            .is_none()
    );
    h.state_mut()
        .doc_mut()
        .unwrap()
        .view
        .set_board_tool(BoardTool::Select);
    h.run_steps(5);
    assert!(
        h.query_by_role_and_label(Role::Button, "Connector route")
            .is_some()
    );
    assert_eq!(h.state().doc().unwrap().view.board_selection, [id]);
    assert_eq!(connector(h.state(), id).route, ConnectorRoute::Straight);
}

fn touch_drag(
    h: &mut Harness<'_, ThinodaApp>,
    start: egui::Pos2,
    delta: egui::Vec2,
    contacts: u64,
    cancel: bool,
) {
    let revision = h.state().doc().unwrap().doc.state_id();
    h.event(egui::Event::PointerMoved(start));
    h.event(egui::Event::PointerButton {
        pos: start,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.event(egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(1),
        phase: egui::TouchPhase::Start,
        pos: start,
        force: None,
    });
    h.run_steps(2);
    let moved = start + delta;
    h.event(egui::Event::PointerMoved(moved));
    h.event(egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(1),
        phase: egui::TouchPhase::Move,
        pos: moved,
        force: None,
    });
    h.run_steps(2);
    assert_eq!(
        h.state().doc().unwrap().doc.state_id(),
        revision,
        "a held gesture must remain provisional"
    );
    if contacts > 1 {
        h.event(egui::Event::Touch {
            device_id: egui::TouchDeviceId(1),
            id: egui::TouchId(2),
            phase: egui::TouchPhase::Start,
            pos: moved + vec2(40.0, 0.0),
            force: None,
        });
        h.run_steps(1);
    }
    for id in 1..=contacts {
        h.event(egui::Event::Touch {
            device_id: egui::TouchDeviceId(1),
            id: egui::TouchId(id),
            phase: if cancel {
                egui::TouchPhase::Cancel
            } else {
                egui::TouchPhase::End
            },
            pos: moved + vec2((id - 1) as f32 * 40.0, 0.0),
            force: None,
        });
    }
    h.run_steps(2);
    h.event(egui::Event::PointerButton {
        pos: moved,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(5);
}

#[test]
fn custom_connector_color_drag_is_atomic_and_rejects_canceled_or_multitouch_input() {
    for (contacts, cancel) in [(1, false), (1, true), (2, false)] {
        let (app, id) = app_with_connector();
        let mut h = harness(app, vec2(1000.0, 700.0));
        click(&mut h, Role::Button, "Connector color");
        h.get_by_label("Custom color").click();
        h.run_steps(5);
        let red = h
            .query_all_by_role(Role::SpinButton)
            .find(|control| control.value().is_some_and(|value| value.starts_with("R ")))
            .expect("custom color red channel");
        red.scroll_to_me();
        h.run_steps(5);
        let rect = h
            .query_all_by_role(Role::SpinButton)
            .find(|control| control.value().is_some_and(|value| value.starts_with("R ")))
            .unwrap()
            .rect();
        let original = connector(h.state(), id).color;
        let state = h.state().doc().unwrap().doc.state_id();
        touch_drag(&mut h, rect.center(), vec2(40.0, 0.0), contacts, cancel);
        if cancel || contacts > 1 {
            assert_eq!(h.state().doc().unwrap().doc.state_id(), state);
            assert_eq!(connector(h.state(), id).color, original);
        } else {
            assert_ne!(h.state().doc().unwrap().doc.state_id(), state);
            assert_ne!(connector(h.state(), id).color, original);
            assert!(h.state_mut().doc_mut().unwrap().doc.undo());
            assert_eq!(h.state().doc().unwrap().doc.state_id(), state);
            assert_eq!(connector(h.state(), id).color, original);
        }
    }
}

#[test]
fn custom_connector_color_keyboard_edit_is_atomic_and_escape_cancels() {
    for accept in [false, true] {
        let (app, id) = app_with_connector();
        let mut h = harness(app, vec2(1000.0, 700.0));
        click(&mut h, Role::Button, "Connector color");
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
        let original = connector(h.state(), id).color;
        h.key_press_modifiers(Modifiers::COMMAND, Key::A);
        h.event(egui::Event::Text("200".into()));
        h.run_steps(3);
        assert_eq!(h.state().doc().unwrap().doc.state_id(), before);
        h.key_press(if accept { Key::Enter } else { Key::Escape });
        h.run_steps(5);
        if accept {
            let color = connector(h.state(), id).color.unwrap();
            assert_eq!(color, Color::rgba(200, 120, 140, 96));
            assert_ne!(h.state().doc().unwrap().doc.state_id(), before);
            assert!(h.state_mut().doc_mut().unwrap().doc.undo());
            assert_eq!(h.state().doc().unwrap().doc.state_id(), before);
        } else {
            assert_eq!(h.state().doc().unwrap().doc.state_id(), before);
            assert!(!h.ctx.text_edit_focused());
        }
        assert_eq!(connector(h.state(), id).color, original);
        assert_eq!(h.state().doc().unwrap().view.board_selection, [id]);
    }
}

#[test]
fn custom_connector_alpha_preserves_rgb_through_full_transparency() {
    let (app, id) = app_with_connector();
    let mut h = harness(app, vec2(1000.0, 700.0));
    click(&mut h, Role::Button, "Connector color");
    h.get_by_label("Custom color").click();
    h.run_steps(5);
    for (value, expected) in [(0, 0), (255, 255)] {
        h.query_all_by_role(Role::SpinButton)
            .find(|control| control.value().is_some_and(|text| text.starts_with("A ")))
            .unwrap()
            .scroll_to_me();
        h.run_steps(4);
        h.query_all_by_role(Role::SpinButton)
            .find(|control| control.value().is_some_and(|text| text.starts_with("A ")))
            .unwrap()
            .click();
        h.run_steps(2);
        h.key_press_modifiers(Modifiers::COMMAND, Key::A);
        h.event(egui::Event::Text(value.to_string()));
        h.run_steps(3);
        h.key_press(Key::Enter);
        h.run_steps(5);
        assert_eq!(
            connector(h.state(), id).color,
            Some(Color::rgba(100, 120, 140, expected))
        );
        if expected == 0 {
            close_popup(&mut h);
            click(&mut h, Role::Button, "Connector color");
            if !h
                .query_all_by_role(Role::SpinButton)
                .any(|control| control.value().is_some_and(|text| text.starts_with("A ")))
            {
                h.get_by_label("Custom color").click();
                h.run_steps(5);
            }
        }
    }
}

#[test]
fn opening_the_custom_color_picker_never_changes_the_connector() {
    for color in [
        Color::rgba(100, 120, 140, 96),
        Color::rgba(255, 0, 0, 0),
        Color::rgba(254, 1, 127, 1),
        Color::rgba(17, 34, 51, 255),
        Color::rgba(0, 1, 1, 0),
    ] {
        let (mut app, id) = app_with_connector();
        app.doc_mut()
            .unwrap()
            .doc
            .update_canvas_object("Set test color", id, |object| {
                let CanvasObjectKind::Connector(connector) = &mut object.kind else {
                    unreachable!()
                };
                connector.color = Some(color);
            })
            .unwrap();
        let mut h = harness(app, vec2(1000.0, 700.0));
        let before = h.state().doc().unwrap().doc.state_id();
        click(&mut h, Role::Button, "Connector color");
        h.get_by_label("Custom color").click();
        h.run_steps(10);
        assert_eq!(h.state().doc().unwrap().doc.state_id(), before, "{color:?}");
        assert_eq!(connector(h.state(), id).color, Some(color), "{color:?}");
    }
}

#[test]
fn changing_selection_discards_a_custom_color_keyboard_draft() {
    let (mut app, first) = app_with_connector();
    let second = app
        .doc_mut()
        .unwrap()
        .doc
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Point(Vec2::new(-100.0, 180.0)),
            Endpoint::Point(Vec2::new(100.0, 180.0)),
        )))
        .unwrap();
    let mut h = harness(app, vec2(1000.0, 700.0));
    click(&mut h, Role::Button, "Connector color");
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
    h.state_mut()
        .doc_mut()
        .unwrap()
        .view
        .select_board_object(second);
    h.run_steps(5);
    assert!(!h.ctx.text_edit_focused());
    assert_eq!(h.state().doc().unwrap().doc.state_id(), before);
    assert_eq!(
        connector(h.state(), first).color,
        Some(Color::rgba(100, 120, 140, 96))
    );
    assert_eq!(connector(h.state(), second).color, None);
    h.state_mut()
        .doc_mut()
        .unwrap()
        .view
        .select_board_object(first);
    h.run_steps(4);
    click(&mut h, Role::Button, "Connector color");
    h.get_by_label("Custom color").click();
    h.run_steps(4);
    assert_eq!(h.state().doc().unwrap().doc.state_id(), before);
}

#[test]
fn connector_width_drag_is_atomic_and_rejects_canceled_or_multitouch_input() {
    for (contacts, cancel) in [(1, false), (1, true), (2, false)] {
        let (app, id) = app_with_connector();
        let mut h = harness(app, vec2(1000.0, 700.0));
        click(&mut h, Role::Button, "Connector width");
        h.get_by_role_and_label(Role::SpinButton, "Connector width")
            .scroll_to_me();
        h.run_steps(5);
        let rect = h
            .get_by_role_and_label(Role::SpinButton, "Connector width")
            .rect();
        let state = h.state().doc().unwrap().doc.state_id();
        touch_drag(&mut h, rect.center(), vec2(30.0, 0.0), contacts, cancel);
        if cancel || contacts > 1 {
            assert_eq!(h.state().doc().unwrap().doc.state_id(), state);
            assert_eq!(connector(h.state(), id).width, 2.0);
        } else {
            assert_ne!(h.state().doc().unwrap().doc.state_id(), state);
            assert_ne!(connector(h.state(), id).width, 2.0);
            assert!(h.state_mut().doc_mut().unwrap().doc.undo());
            assert_eq!(h.state().doc().unwrap().doc.state_id(), state);
            assert_eq!(connector(h.state(), id).width, 2.0);
        }
    }
}

#[test]
fn changing_selection_during_a_custom_color_drag_discards_the_draft() {
    let (mut app, first) = app_with_connector();
    let second = app
        .doc_mut()
        .unwrap()
        .doc
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Point(Vec2::new(-100.0, 180.0)),
            Endpoint::Point(Vec2::new(100.0, 180.0)),
        )))
        .unwrap();
    let mut h = harness(app, vec2(1000.0, 700.0));
    click(&mut h, Role::Button, "Connector color");
    h.get_by_label("Custom color").click();
    h.run_steps(5);
    h.query_all_by_role(Role::SpinButton)
        .find(|control| control.value().is_some_and(|value| value.starts_with("R ")))
        .unwrap()
        .scroll_to_me();
    h.run_steps(5);
    let start = h
        .query_all_by_role(Role::SpinButton)
        .find(|control| control.value().is_some_and(|value| value.starts_with("R ")))
        .unwrap()
        .rect()
        .center();
    let end = start + vec2(40.0, 0.0);
    let before = h.state().doc().unwrap().doc.state_id();
    h.event(egui::Event::PointerMoved(start));
    h.event(egui::Event::PointerButton {
        pos: start,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(2);
    h.event(egui::Event::PointerMoved(end));
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.state_id(), before);
    h.state_mut()
        .doc_mut()
        .unwrap()
        .view
        .select_board_object(second);
    h.run_steps(3);
    h.event(egui::Event::PointerButton {
        pos: end,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(5);
    assert_eq!(h.state().doc().unwrap().doc.state_id(), before);
    assert_eq!(
        connector(h.state(), first).color,
        Some(Color::rgba(100, 120, 140, 96))
    );
    assert_eq!(connector(h.state(), second).color, None);
    assert!(h.query_by_label("Custom color").is_none());
    h.state_mut()
        .doc_mut()
        .unwrap()
        .view
        .select_board_object(first);
    h.run_steps(4);
    click(&mut h, Role::Button, "Connector color");
    h.get_by_label("Custom color").click();
    h.run_steps(4);
    assert_eq!(
        connector(h.state(), first).color,
        Some(Color::rgba(100, 120, 140, 96))
    );
}

#[test]
fn connector_drag_disables_controls_and_escape_cancels_the_canvas_gesture() {
    let (app, id) = app_with_connector();
    let mut h = harness(app, vec2(1000.0, 700.0));
    let target = h.state().connector_context().unwrap().target;
    let original = h.state().doc().unwrap().doc.workbook().clone();
    let canvas = h.state().doc().unwrap().view.canvas_rect();
    let view = &h.state().doc().unwrap().view;
    let point = canvas.center() + view.offset + vec2(-60.0, -5.0) * view.zoom;
    let end = point + vec2(50.0, 30.0);
    h.hover_at(point);
    h.run_steps(2);
    h.event(egui::Event::PointerButton {
        pos: point,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(1);
    h.event(egui::Event::PointerMoved(end));
    h.run_steps(3);
    assert!(h.state().doc().unwrap().view.has_active_canvas_gesture());
    assert!(!h.state().connector_context().unwrap().enabled);
    assert!(
        h.state_mut()
            .apply_connector_change(&target, ConnectorChange::ArrowStart(true))
            .is_err()
    );
    assert!(
        h.get_by_role_and_label(Role::Button, "Arrow at start")
            .accesskit_node()
            .is_disabled()
    );
    assert_eq!(h.state().doc().unwrap().view.canvas_rect(), canvas);
    h.key_press(Key::Escape);
    h.run_steps(2);
    h.event(egui::Event::PointerButton {
        pos: end,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(5);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &original);
    assert!(!connector(h.state(), id).arrow_start);
}

#[test]
fn connector_controls_reject_canceled_and_multitouch_taps() {
    for (control, value) in [
        ("Arrow at start", None),
        ("Curve", Some("Connector route")),
        ("#2E7CF6", Some("Connector color")),
        ("Connector width: 8", Some("Connector width")),
    ] {
        for (contacts, cancel) in [(1, true), (2, false), (1, false)] {
            let (app, id) = app_with_connector();
            let mut h = harness(app, vec2(1000.0, 700.0));
            if let Some(menu) = value {
                click(&mut h, Role::Button, menu);
            }
            let role = if control.starts_with('#') {
                Role::ColorWell
            } else {
                Role::Button
            };
            h.get_by_role_and_label(role, control).scroll_to_me();
            h.run_steps(4);
            let pos = h.get_by_role_and_label(role, control).rect().center();
            let before = h.state().doc().unwrap().doc.state_id();
            h.event(egui::Event::PointerMoved(pos));
            h.event(egui::Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            });
            for phase in [
                egui::TouchPhase::Start,
                if cancel {
                    egui::TouchPhase::Cancel
                } else {
                    egui::TouchPhase::End
                },
            ] {
                for id in 1..=contacts {
                    h.event(egui::Event::Touch {
                        device_id: egui::TouchDeviceId(1),
                        id: egui::TouchId(id),
                        phase,
                        pos: pos + vec2((id - 1) as f32 * 40.0, 0.0),
                        force: None,
                    });
                }
                if phase == egui::TouchPhase::End && contacts == 1 && !cancel {
                    h.event(egui::Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed: false,
                        modifiers: Modifiers::NONE,
                    });
                }
                h.run_steps(2);
            }
            if contacts != 1 || cancel {
                h.event(egui::Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                });
            }
            h.run_steps(4);
            let activated = contacts == 1 && !cancel;
            assert_eq!(
                h.state().doc().unwrap().doc.state_id() != before,
                activated,
                "{control} contacts={contacts} cancel={cancel}"
            );
            let current = connector(h.state(), id);
            match control {
                "Arrow at start" => assert_eq!(current.arrow_start, activated),
                "Curve" => assert_eq!(
                    current.route,
                    if activated {
                        ConnectorRoute::Curve
                    } else {
                        ConnectorRoute::Straight
                    }
                ),
                "#2E7CF6" => assert_eq!(
                    current.color,
                    Some(if activated {
                        Color::rgba(0x2e, 0x7c, 0xf6, 96)
                    } else {
                        Color::rgba(100, 120, 140, 96)
                    })
                ),
                "Connector width: 8" => {
                    assert_eq!(current.width, if activated { 8.0 } else { 2.0 })
                }
                _ => unreachable!(),
            }
        }
    }
}
