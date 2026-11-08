use super::*;
use egui::{PointerButton, Role, accesskit::Toggled};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use thinoda_core::{
    CanvasObject, Card, Rect as BoardRect, Sheet, Vec2, Workbook,
    board::{BoardMember, ShapeKind},
};

fn app_with_card(kind: CardKind) -> (ThinodaApp, TopicId) {
    let mut app = ThinodaApp::headless();
    app.settings.ai.enabled = false;
    app.settings.show_format_panel = false;
    app.new_doc(templates::instantiate("blank-canvas"));
    let doc = app.doc_mut().unwrap();
    let id = doc
        .doc
        .edit_sheet("Create card", |sheet| {
            sheet
                .add_card(
                    "Card title",
                    Vec2::ZERO,
                    Card::new(kind, Vec2::new(180.0, 120.0)),
                )
                .map_err(thinoda_core::EditError::Invalid)
        })
        .unwrap();
    doc.view.select_only(id);
    (app, id)
}

fn harness(app: ThinodaApp, size: egui::Vec2) -> Harness<'static, ThinodaApp> {
    let mut harness = Harness::builder()
        .with_size(size)
        .with_step_dt(1.0 / 60.0)
        .with_context_setup(|ctx| {
            ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            ctx.add_plugin(CardTextInputPlugin);
        })
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    harness.run_steps(6);
    harness
}

fn open_toolbar(harness: &mut Harness<'_, ThinodaApp>) {
    harness
        .get_by_role_and_label(Role::Button, card_text_label())
        .click();
    harness.run_steps(5);
    assert!(
        harness
            .query_by_role_and_label(Role::Button, "Bold")
            .is_some()
    );
}

fn scroll_and_click(harness: &mut Harness<'_, ThinodaApp>, role: Role, label: &str) {
    harness.get_by_role_and_label(role, label).scroll_to_me();
    harness.run_steps(4);
    harness.get_by_role_and_label(role, label).click();
    harness.run_steps(3);
}

fn open_toolbar_from_context(h: &mut Harness<'_, ThinodaApp>) {
    let pos = {
        let doc = h.state().doc().unwrap();
        doc.view.selected_topic_rect(&doc.doc).unwrap().center()
    };
    h.event(egui::Event::PointerMoved(pos));
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(1);
    }
    h.run_steps(4);
    h.get_by_role_and_label(Role::Button, "Styles").hover();
    h.run_steps(6);
    scroll_and_click(h, Role::Button, card_text_label());
}

fn style(harness: &Harness<'_, ThinodaApp>, id: TopicId) -> TopicStyle {
    harness
        .state()
        .doc()
        .unwrap()
        .doc
        .sheet()
        .topic(id)
        .unwrap()
        .style
        .clone()
}

fn raw_key_press(key: Key, modifiers: Modifiers) -> [egui::Event; 4] {
    let event = |pressed| egui::Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers,
    };
    [
        egui::Event::ModifiersChanged(modifiers),
        event(true),
        event(false),
        egui::Event::ModifiersChanged(Modifiers::NONE),
    ]
}

fn run_input_hook_steps(h: &mut Harness<'_, ThinodaApp>, steps: usize) {
    for _ in 0..steps {
        let mut input = h.input().clone();
        let ctx = h.ctx.clone();
        eframe::App::raw_input_hook(h.state_mut(), &ctx, &mut input);
        *h.input_mut() = input;
        h.run_steps(1);
    }
}

#[test]
fn raw_input_split_keeps_native_and_web_touch_packets_intact() {
    let pos = egui::pos2(20.0, 30.0);
    let touch = |phase| egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(1),
        phase,
        pos,
        force: None,
    };
    let pointer = |pressed| egui::Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    for packet in [
        vec![
            touch(egui::TouchPhase::Start),
            egui::Event::PointerMoved(pos),
            pointer(true),
        ],
        vec![pointer(true), touch(egui::TouchPhase::Start)],
        vec![
            touch(egui::TouchPhase::End),
            pointer(false),
            egui::Event::PointerGone,
        ],
        vec![
            pointer(false),
            egui::Event::PointerGone,
            touch(egui::TouchPhase::End),
        ],
        vec![
            touch(egui::TouchPhase::Cancel),
            pointer(false),
            egui::Event::PointerGone,
        ],
        vec![
            touch(egui::TouchPhase::Start),
            egui::Event::Touch {
                device_id: egui::TouchDeviceId(1),
                id: egui::TouchId(2),
                phase: egui::TouchPhase::Start,
                pos,
                force: None,
            },
            pointer(true),
            touch(egui::TouchPhase::Cancel),
        ],
    ] {
        let mut events = vec![egui::Event::Text("before".into())];
        events.extend(packet.clone());
        events.push(egui::Event::Text("after".into()));
        assert_eq!(card_text_input_split(&events), Some(1));
        let remaining = &events[1..];
        assert_eq!(card_text_input_split(remaining), Some(packet.len()));
    }
}

#[test]
fn raw_input_queue_preserves_order_and_viewport_ownership() {
    let (mut app, id) = app_with_card(CardKind::Sticky);
    {
        let doc = app.doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
    }
    let child = egui::ViewportId::from_hash_of("card-input-child");
    let key = app.doc().unwrap().key.clone();
    app.document_windows.insert(key.clone(), child);
    app.window_active_documents.insert(child, key);
    let ctx = egui::Context::default();
    ctx.add_plugin(CardTextInputPlugin);
    let mut original = vec![egui::Event::Text("before".into())];
    original.extend(raw_key_press(Key::F10, Modifiers::ALT));
    original.push(egui::Event::Text("after".into()));
    let mut child_input = egui::RawInput {
        viewport_id: child,
        events: original.clone(),
        ..Default::default()
    };
    child_input.viewports.insert(child, Default::default());
    app.prepare_card_text_input(&ctx, &mut child_input);
    let mut delivered = Vec::new();
    ctx.run_ui(child_input, |ui| {
        delivered.extend(ui.input(|input| input.events.clone()));
    })
    .drop_without_applying_deltas();

    let mut root_input = egui::RawInput {
        events: vec![egui::Event::Text("root".into())],
        ..Default::default()
    };
    app.prepare_card_text_input(&ctx, &mut root_input);
    assert_eq!(root_input.events, vec![egui::Event::Text("root".into())]);

    let later = egui::Event::Text("later".into());
    for step in 0..4 {
        let mut input = egui::RawInput {
            viewport_id: child,
            events: if step == 0 {
                vec![later.clone()]
            } else {
                vec![]
            },
            ..Default::default()
        };
        input.viewports.insert(child, Default::default());
        app.prepare_card_text_input(&ctx, &mut input);
        ctx.run_ui(input, |ui| {
            delivered.extend(ui.input(|input| input.events.clone()));
        })
        .drop_without_applying_deltas();
    }
    original.push(later);
    assert_eq!(delivered, original);
}

#[test]
fn backend_hooks_without_ui_do_not_reorder_or_duplicate_queued_input() {
    for hidden in [false, true] {
        let (mut app, id) = app_with_card(CardKind::Sticky);
        {
            let doc = app.doc_mut().unwrap();
            doc.view.begin_edit(doc.doc.sheet(), id, false);
        }
        let ctx = egui::Context::default();
        ctx.add_plugin(CardTextInputPlugin);
        let mut original = vec![egui::Event::Text("before".into())];
        original.extend(raw_key_press(Key::F10, Modifiers::ALT));
        original.push(egui::Event::Text("after".into()));
        let mut input = egui::RawInput {
            events: original.clone(),
            ..Default::default()
        };
        let info = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
        info.occluded = Some(hidden);
        info.minimized = Some(false);
        info.events.push(egui::ViewportEvent::Close);
        eframe::App::raw_input_hook(&mut app, &ctx, &mut input);
        assert_eq!(input.events, original);
        assert!(!input.viewport().close_requested());
        let output = ctx.run_logic(&input, |_| {});
        #[cfg(not(target_arch = "wasm32"))]
        assert!(
            output.viewport_commands[&egui::ViewportId::ROOT]
                .contains(&egui::ViewportCommand::RequestPaintWhileHidden)
        );
        #[cfg(target_arch = "wasm32")]
        let _ = output;

        let mut more = egui::RawInput::default();
        more.viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .occluded = Some(hidden);
        let later = egui::Event::Text("later".into());
        more.events.push(later.clone());
        input.append(more);
        eframe::App::raw_input_hook(&mut app, &ctx, &mut input);
        original.push(later);
        assert_eq!(input.events, original);
        assert!(!input.viewport().close_requested());

        let mut delivered = Vec::new();
        let mut close_count = 0;
        for step in 0..8 {
            if step > 0 {
                input = egui::RawInput::default();
                eframe::App::raw_input_hook(&mut app, &ctx, &mut input);
            }
            ctx.run_ui(input.clone(), |ui| {
                ui.input(|state| {
                    delivered.extend(state.events.clone());
                    close_count += usize::from(state.viewport().close_requested());
                });
            })
            .drop_without_applying_deltas();
        }
        assert_eq!(delivered, original, "hidden={hidden}");
        assert_eq!(close_count, 1, "hidden={hidden}");
    }
}

#[test]
fn backend_installs_input_plugin_if_context_setup_was_omitted() {
    let (mut app, _) = app_with_card(CardKind::Sticky);
    let ctx = egui::Context::default();
    assert!(ctx.plugin_opt::<CardTextInputPlugin>().is_none());
    let mut input = egui::RawInput::default();
    eframe::App::raw_input_hook(&mut app, &ctx, &mut input);
    assert!(ctx.plugin_opt::<CardTextInputPlugin>().is_some());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn queued_child_input_requests_a_hidden_root_ui_pass() {
    let child = egui::ViewportId::from_hash_of("hidden-card-child");
    let ctx = egui::Context::default();
    ctx.add_plugin(CardTextInputPlugin);
    ctx.data_mut(|data| data.insert_temp(card_text_slot(child).with("input_scope"), true));
    let mut events = vec![egui::Event::Text("before".into())];
    events.extend(raw_key_press(Key::F10, Modifiers::ALT));
    events.push(egui::Event::Text("after".into()));
    let mut input = egui::RawInput {
        viewport_id: child,
        events,
        ..Default::default()
    };
    input.viewports.entry(child).or_default().parent = Some(egui::ViewportId::ROOT);
    let output = ctx.run_ui(input, |_| {});
    assert!(
        output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::RequestPaintWhileHidden)
    );
    output.drop_without_applying_deltas();
}

#[test]
fn plugin_orders_input_and_close_in_an_immediate_document_viewport() {
    let (mut app, id) = app_with_card(CardKind::Sticky);
    {
        let doc = app.doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
    }
    let child = egui::ViewportId::from_hash_of("card-input-immediate");
    let key = app.doc().unwrap().key.clone();
    app.document_windows.insert(key.clone(), child);
    app.window_active_documents.insert(child, key.clone());
    app.current_viewport = child;
    app.drawing_document_window = true;
    let ctx = egui::Context::default();
    ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    ctx.add_plugin(CardTextInputPlugin);

    let run = |app: &mut ThinodaApp, events: Vec<egui::Event>, close: bool| {
        app.mark_card_text_input_scope(&ctx, child);
        let mut input = egui::RawInput {
            viewport_id: child,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(1000.0, 700.0),
            )),
            events,
            ..Default::default()
        };
        let info = input.viewports.entry(child).or_default();
        info.parent = Some(egui::ViewportId::ROOT);
        if close {
            info.events.push(egui::ViewportEvent::Close);
        }
        ctx.run_ui(input, |ui| app.draw(ui))
    };
    for _ in 0..6 {
        run(&mut app, vec![], false).drop_without_applying_deltas();
    }
    assert!(ctx.memory(|memory| memory.has_focus(MapCanvas::title_editor_id())));

    let mut events = vec![egui::Event::Text("!".into())];
    events.extend(raw_key_press(Key::F10, Modifiers::ALT));
    let [modifiers, down, up, clear] = raw_key_press(Key::Space, Modifiers::NONE);
    events.extend([modifiers, down, egui::Event::Text(" ".into()), up, clear]);
    let output = run(&mut app, events, true);
    assert_eq!(
        output.viewport_output[&child].repaint_delay,
        std::time::Duration::ZERO
    );
    output.drop_without_applying_deltas();
    for _ in 0..7 {
        run(&mut app, vec![], false).drop_without_applying_deltas();
    }
    let doc = app.docs.iter().find(|doc| doc.key == key).unwrap();
    assert_eq!(doc.doc.sheet().topic(id).unwrap().title, "Card title!");
    assert_eq!(doc.doc.sheet().topic(id).unwrap().style.bold, Some(true));
    assert!(!app.document_windows.contains_key(&key));
}

#[test]
fn native_window_command_waits_for_its_queued_card_input() {
    let (mut app, id) = app_with_card(CardKind::Sticky);
    {
        let doc = app.doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
    }
    let child = egui::ViewportId::from_hash_of("card-input-child-home");
    let key = app.doc().unwrap().key.clone();
    app.document_windows.insert(key.clone(), child);
    app.window_active_documents.insert(child, key.clone());
    app.current_viewport = child;
    app.drawing_document_window = true;
    let ctx = egui::Context::default();
    ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    ctx.add_plugin(CardTextInputPlugin);
    let run = |app: &mut ThinodaApp, events: Vec<egui::Event>, home: bool| {
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .drop_without_applying_deltas();
        app.mark_card_text_input_scope(&ctx, child);
        let mut input = egui::RawInput {
            viewport_id: child,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(1000.0, 700.0),
            )),
            events,
            ..Default::default()
        };
        input.viewports.entry(child).or_default().parent = Some(egui::ViewportId::ROOT);
        ctx.run_ui(input, |ui| {
            if home {
                app.exec(ui.ctx(), Cmd::Home);
            }
            app.draw(ui);
        })
        .drop_without_applying_deltas();
    };
    for _ in 0..6 {
        run(&mut app, vec![], false);
    }
    let mut events = vec![egui::Event::Text("!".into())];
    events.extend(raw_key_press(Key::F10, Modifiers::ALT));
    let [modifiers, down, up, clear] = raw_key_press(Key::Space, Modifiers::NONE);
    events.extend([modifiers, down, egui::Event::Text(" ".into()), up, clear]);
    run(&mut app, events, false);
    run(&mut app, vec![], true);
    for _ in 0..8 {
        run(&mut app, vec![], false);
    }
    let doc = app.docs.iter().find(|doc| doc.key == key).unwrap();
    assert_eq!(doc.doc.sheet().topic(id).unwrap().style.bold, Some(true));
    assert!(!app.document_windows.contains_key(&key));
    assert!(!card_text_input_needs_repaint(&ctx, child));
}

#[test]
fn root_close_waits_for_an_immediate_childs_pending_input() {
    let (mut app, id) = app_with_card(CardKind::Sticky);
    {
        let doc = app.doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
    }
    let child = egui::ViewportId::from_hash_of("card-input-child-close");
    let child_key = app.doc().unwrap().key.clone();
    let root_key = app.docs[0].key.clone();
    app.document_windows.insert(child_key.clone(), child);
    app.window_active_documents.insert(child, child_key.clone());
    app.window_active_documents
        .insert(egui::ViewportId::ROOT, root_key);
    let ctx = egui::Context::default();
    ctx.set_embed_viewports(false);
    ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    ctx.add_plugin(CardTextInputPlugin);
    let child_input = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let queued_child_input = child_input.clone();
    egui::Context::set_immediate_viewport_renderer(move |ctx, mut viewport| {
        let events = std::mem::take(&mut *queued_child_input.borrow_mut());
        let mut input = egui::RawInput {
            viewport_id: viewport.ids.this,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(1000.0, 700.0),
            )),
            events,
            ..Default::default()
        };
        input.viewports.entry(viewport.ids.this).or_default().parent = Some(viewport.ids.parent);
        ctx.run_ui(input, |ui| (viewport.viewport_ui_cb)(ui))
            .drop_without_applying_deltas();
    });
    let run_root = |app: &mut ThinodaApp, close: bool| {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(1000.0, 700.0),
            )),
            ..Default::default()
        };
        input.viewports.entry(child).or_default().parent = Some(egui::ViewportId::ROOT);
        if close {
            input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .events
                .push(egui::ViewportEvent::Close);
        }
        eframe::App::raw_input_hook(app, &ctx, &mut input);
        ctx.run_ui(input, |ui| app.draw(ui))
            .drop_without_applying_deltas();
    };
    for _ in 0..6 {
        run_root(&mut app, false);
    }
    {
        let mut events = child_input.borrow_mut();
        events.push(egui::Event::Text("!".into()));
        events.extend(raw_key_press(Key::F10, Modifiers::ALT));
        let [modifiers, down, up, clear] = raw_key_press(Key::Space, Modifiers::NONE);
        events.extend([modifiers, down, egui::Event::Text(" ".into()), up, clear]);
    }
    run_root(&mut app, true);
    for _ in 0..8 {
        run_root(&mut app, false);
    }
    let doc = app.docs.iter().find(|doc| doc.key == child_key).unwrap();
    assert_eq!(doc.doc.sheet().topic(id).unwrap().title, "Card title!");
    assert_eq!(doc.doc.sheet().topic(id).unwrap().style.bold, Some(true));
    assert!(app.dialogs.close.is_some());
}

#[test]
fn opening_a_document_in_another_window_waits_for_its_queued_input() {
    check_opening_document_window_with_pending_input(true);
    check_opening_document_window_with_pending_input(false);
}

fn check_opening_document_window_with_pending_input(input_already_queued: bool) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("detached.thinoda");
    let (mut app, id) = app_with_card(CardKind::Sticky);
    {
        let doc = app.doc_mut().unwrap();
        doc.save_to(&path, false).unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
    }
    let child = egui::ViewportId::from_hash_of("card-input-child-move");
    let child_key = app.doc().unwrap().key.clone();
    app.document_windows.insert(child_key.clone(), child);
    app.window_active_documents.insert(child, child_key.clone());
    let ctx = egui::Context::default();
    ctx.set_embed_viewports(false);
    ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    ctx.add_plugin(CardTextInputPlugin);
    let child_input = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let queued_child_input = child_input.clone();
    egui::Context::set_immediate_viewport_renderer(move |ctx, mut viewport| {
        let events = std::mem::take(&mut *queued_child_input.borrow_mut());
        let mut input = egui::RawInput {
            viewport_id: viewport.ids.this,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(1000.0, 700.0),
            )),
            events,
            ..Default::default()
        };
        input.viewports.entry(viewport.ids.this).or_default().parent = Some(viewport.ids.parent);
        ctx.run_ui(input, |ui| (viewport.viewport_ui_cb)(ui))
            .drop_without_applying_deltas();
    });
    let run_root = |app: &mut ThinodaApp, open: bool| {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(1000.0, 700.0),
            )),
            ..Default::default()
        };
        input.viewports.entry(child).or_default().parent = Some(egui::ViewportId::ROOT);
        eframe::App::raw_input_hook(app, &ctx, &mut input);
        ctx.run_ui(input, |ui| {
            if open {
                app.exec(ui.ctx(), Cmd::OpenPath(path.clone()));
            }
            app.draw(ui);
        })
        .drop_without_applying_deltas();
    };
    for _ in 0..6 {
        run_root(&mut app, false);
    }
    {
        let mut events = child_input.borrow_mut();
        events.push(egui::Event::Text("!".into()));
        events.extend(raw_key_press(Key::F10, Modifiers::ALT));
        let [modifiers, down, up, clear] = raw_key_press(Key::Space, Modifiers::NONE);
        events.extend([modifiers, down, egui::Event::Text(" ".into()), up, clear]);
    }
    if input_already_queued {
        run_root(&mut app, false);
    }
    run_root(&mut app, true);
    for _ in 0..8 {
        run_root(&mut app, false);
    }
    let doc = app.docs.iter().find(|doc| doc.key == child_key).unwrap();
    assert_eq!(doc.doc.sheet().topic(id).unwrap().style.bold, Some(true));
    assert!(!app.document_windows.contains_key(&child_key));
    assert!(!card_text_input_needs_repaint(&ctx, child));
}

#[test]
fn save_all_waits_for_another_windows_queued_card_input() {
    check_save_all_with_another_windows_input(true, false);
    check_save_all_with_another_windows_input(false, false);
    check_save_all_with_another_windows_input(false, true);
}

fn check_save_all_with_another_windows_input(input_already_queued: bool, close: bool) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("detached-save.thinoda");
    let (mut app, id) = app_with_card(CardKind::Sticky);
    {
        let doc = app.doc_mut().unwrap();
        doc.save_to(&path, false).unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
    }
    for (index, doc) in app.docs.iter_mut().enumerate() {
        if doc.path.is_none() {
            doc.save_to(
                &directory.path().join(format!("root-{index}.thinoda")),
                false,
            )
            .unwrap();
        }
    }
    let child = egui::ViewportId::from_hash_of("card-input-child-save-all");
    let child_key = app.doc().unwrap().key.clone();
    app.document_windows.insert(child_key.clone(), child);
    app.window_active_documents.insert(child, child_key);
    let ctx = egui::Context::default();
    ctx.set_embed_viewports(false);
    ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    ctx.add_plugin(CardTextInputPlugin);
    let child_input = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let queued_child_input = child_input.clone();
    egui::Context::set_immediate_viewport_renderer(move |ctx, mut viewport| {
        let events = std::mem::take(&mut *queued_child_input.borrow_mut());
        let mut input = egui::RawInput {
            viewport_id: viewport.ids.this,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(1000.0, 700.0),
            )),
            events,
            ..Default::default()
        };
        input.viewports.entry(viewport.ids.this).or_default().parent = Some(viewport.ids.parent);
        ctx.run_ui(input, |ui| (viewport.viewport_ui_cb)(ui))
            .drop_without_applying_deltas();
    });
    let run_root = |app: &mut ThinodaApp, save: bool, close: bool| {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(1000.0, 700.0),
            )),
            ..Default::default()
        };
        input.viewports.entry(child).or_default().parent = Some(egui::ViewportId::ROOT);
        if close {
            input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .events
                .push(egui::ViewportEvent::Close);
        }
        eframe::App::raw_input_hook(app, &ctx, &mut input);
        ctx.run_ui(input, |ui| {
            if save {
                app.exec(ui.ctx(), Cmd::SaveAll);
            }
            app.draw(ui);
        })
        .drop_without_applying_deltas();
    };
    for _ in 0..6 {
        run_root(&mut app, false, false);
    }
    {
        let mut events = child_input.borrow_mut();
        events.push(egui::Event::Text("!".into()));
        events.extend(raw_key_press(Key::F10, Modifiers::ALT));
        let [modifiers, down, up, clear] = raw_key_press(Key::Space, Modifiers::NONE);
        events.extend([modifiers, down, egui::Event::Text(" ".into()), up, clear]);
    }
    if input_already_queued {
        run_root(&mut app, false, false);
    }
    run_root(&mut app, true, close);
    for _ in 0..8 {
        run_root(&mut app, false, false);
    }
    let saved = thinoda_io::import_file(&path).unwrap();
    assert_eq!(saved.active().topic(id).unwrap().title, "Card title!");
    assert_eq!(saved.active().topic(id).unwrap().style.bold, Some(true));
}

#[test]
fn asynchronous_open_waits_for_queued_input_in_its_origin_window() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("another.thinoda");
    let second_path = directory.path().join("later.thinoda");
    let mut other = crate::doc::OpenDoc::new(templates::instantiate("mind-map"));
    other.save_to(&path, false).unwrap();
    let mut second = crate::doc::OpenDoc::new(templates::instantiate("mind-map"));
    second.save_to(&second_path, false).unwrap();
    let (app, id) = app_with_card(CardKind::Sticky);
    let original_key = app.doc().unwrap().key.clone();
    let mut h = harness(app, vec2(1000.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
    }
    h.run_steps(5);
    h.input_mut().events.push(egui::Event::Text("!".into()));
    h.input_mut()
        .events
        .extend(raw_key_press(Key::F10, Modifiers::ALT));
    let [modifiers, down, up, clear] = raw_key_press(Key::Space, Modifiers::NONE);
    h.input_mut()
        .events
        .extend([modifiers, down, egui::Event::Text(" ".into()), up, clear]);
    h.run_steps(1);
    assert!(card_text_input_needs_repaint(
        &h.ctx,
        egui::ViewportId::ROOT
    ));
    let ctx = h.ctx.clone();
    h.state_mut()
        .request_open_from_ui(&ctx, &path, CardOpenOrder::AfterQueuedInput);
    h.state_mut()
        .request_open_from_ui(&ctx, &second_path, CardOpenOrder::AfterQueuedInput);
    h.run_steps(8);
    let original = h
        .state()
        .docs
        .iter()
        .find(|doc| doc.key == original_key)
        .unwrap();
    assert_eq!(
        original.doc.sheet().topic(id).unwrap().style.bold,
        Some(true)
    );
    assert!(
        h.state()
            .docs
            .iter()
            .any(|doc| doc.path.as_deref() == Some(path.as_path()))
    );
    assert_eq!(
        h.state().doc().unwrap().path.as_deref(),
        Some(second_path.as_path())
    );
    assert!(!card_text_input_needs_repaint(
        &h.ctx,
        egui::ViewportId::ROOT
    ));
}

#[test]
fn clean_synthetic_root_close_is_forwarded_to_the_native_backend() {
    let directory = tempfile::tempdir().unwrap();
    let (mut app, _) = app_with_card(CardKind::Sticky);
    for (index, doc) in app.docs.iter_mut().enumerate() {
        doc.save_to(&directory.path().join(format!("{index}.thinoda")), false)
            .unwrap();
    }
    assert!(!app.any_dirty());
    let ctx = egui::Context::default();
    ctx.add_plugin(CardTextInputPlugin);
    let mut close_sent = false;
    for step in 0..4 {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(1000.0, 700.0),
            )),
            ..Default::default()
        };
        if step == 0 {
            input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .events
                .push(egui::ViewportEvent::Close);
        }
        eframe::App::raw_input_hook(&mut app, &ctx, &mut input);
        assert!(!input.viewport().close_requested());
        let output = ctx.run_ui(input, |ui| app.draw(ui));
        close_sent |= output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::Close);
        output.drop_without_applying_deltas();
        if close_sent {
            break;
        }
    }
    assert!(close_sent);
    let mut native_close = egui::RawInput::default();
    native_close
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .events
        .push(egui::ViewportEvent::Close);
    eframe::App::raw_input_hook(&mut app, &ctx, &mut native_close);
    assert!(native_close.viewport().close_requested());
}

#[test]
fn raw_input_is_unchanged_for_an_ordinary_topic() {
    let mut app = ThinodaApp::headless();
    let doc = app.doc_mut().unwrap();
    let id = doc.doc.sheet().root;
    doc.view.begin_edit(doc.doc.sheet(), id, false);
    assert_eq!(doc.view.editing_topic(), Some(id));
    let mut events = vec![egui::Event::Text("before".into())];
    events.extend(raw_key_press(Key::F10, Modifiers::ALT));
    events.push(egui::Event::Text("after".into()));
    let mut input = egui::RawInput {
        events: events.clone(),
        ..Default::default()
    };
    app.prepare_card_text_input(&egui::Context::default(), &mut input);
    assert_eq!(input.events, events);
}

#[test]
fn root_close_waits_for_deferred_card_operations() {
    let ctx = egui::Context::default();
    ctx.add_plugin(CardTextInputPlugin);
    let slot = card_text_slot(egui::ViewportId::ROOT);
    ctx.data_mut(|data| {
        data.insert_temp(
            slot.with("deferred_input"),
            DeferredCardTextInput {
                close_after_frame: Some(0),
                ..Default::default()
            },
        );
        data.insert_temp(
            slot.with("deferred_operations"),
            vec![DeferredCardTextOperation {
                origin: egui::ViewportId::ROOT,
                not_before_root_frame: 0,
                action: DeferredCardTextAction::SaveAll,
            }],
        );
    });
    let mut closed = false;
    ctx.run_ui(egui::RawInput::default(), |ui| {
        closed = ui.input(|input| input.viewport().close_requested());
    })
    .drop_without_applying_deltas();
    assert!(!closed);
    ctx.data_mut(|data| {
        data.remove::<Vec<DeferredCardTextOperation>>(slot.with("deferred_operations"));
    });
    ctx.run_ui(egui::RawInput::default(), |ui| {
        closed = ui.input(|input| input.viewport().close_requested());
    })
    .drop_without_applying_deltas();
    assert!(closed);
}

#[test]
fn moving_a_document_discards_only_its_obsolete_deferred_window_close() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("closing-child.thinoda");
    let (mut app, _) = app_with_card(CardKind::Sticky);
    app.doc_mut().unwrap().save_to(&path, false).unwrap();
    let key = app.doc().unwrap().key.clone();
    let child = egui::ViewportId::from_hash_of("closing-card-child");
    app.document_windows.insert(key.clone(), child);
    app.window_active_documents.insert(child, key.clone());
    app.current_viewport = egui::ViewportId::ROOT;
    app.activate_document_window(egui::ViewportId::ROOT);
    let ctx = egui::Context::default();
    ctx.add_plugin(CardTextInputPlugin);
    ctx.data_mut(|data| {
        data.insert_temp(
            card_text_slot(child).with("deferred_input"),
            DeferredCardTextInput {
                close_after_frame: Some(0),
                ..Default::default()
            },
        );
    });
    app.request_open_from_ui(&ctx, &path, CardOpenOrder::CurrentInput);
    assert!(!app.document_windows.contains_key(&key));
    assert!(!card_text_input_needs_repaint(&ctx, child));
    app.document_windows.insert(key.clone(), child);
    app.window_active_documents.insert(child, key);
    app.mark_card_text_input_scope(&ctx, child);
    let mut input = egui::RawInput {
        viewport_id: child,
        ..Default::default()
    };
    input.viewports.entry(child).or_default();
    ctx.run_ui(input, |ui| {
        assert!(!ui.input(|state| state.viewport().close_requested()));
    })
    .drop_without_applying_deltas();
}

#[test]
fn deferred_open_rejects_a_replaced_target_with_a_warning() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("stale-child.thinoda");
    let (mut app, _) = app_with_card(CardKind::Sticky);
    app.doc_mut().unwrap().save_to(&path, false).unwrap();
    let index = app.active;
    let key = app.doc().unwrap().key.clone();
    let child = egui::ViewportId::from_hash_of("stale-card-child");
    app.document_windows.insert(key.clone(), child);
    app.window_active_documents.insert(child, key.clone());
    app.current_viewport = egui::ViewportId::ROOT;
    app.activate_document_window(egui::ViewportId::ROOT);
    let ctx = egui::Context::default();
    app.mark_card_text_input_scope(&ctx, child);
    ctx.data_mut(|data| {
        data.insert_temp(
            card_text_slot(child).with("deferred_input"),
            DeferredCardTextInput {
                events: vec![egui::Event::Text("pending".into())],
                ..Default::default()
            },
        );
    });
    app.request_open_from_ui(&ctx, &path, CardOpenOrder::CurrentInput);
    app.docs[index].path = None;
    ctx.data_mut(|data| {
        data.remove::<DeferredCardTextInput>(card_text_slot(child).with("deferred_input"));
    });
    for _ in 0..3 {
        ctx.run_ui(egui::RawInput::default(), |ui| {
            app.finish_deferred_card_text_operations(ui.ctx());
        })
        .drop_without_applying_deltas();
    }
    assert_eq!(app.document_windows.get(&key), Some(&child));
    assert!(
        app.toasts
            .texts()
            .any(|text| text.contains(&path.to_string_lossy().to_string()))
    );
}

#[derive(Clone, Copy)]
enum DeferredTabChange {
    EarlierTabClosed,
    TargetClosed,
    TargetMoved,
}

fn check_deferred_tab_target(after_editor: bool, change: DeferredTabChange) {
    let directory = tempfile::tempdir().unwrap();
    let (mut app, card) = app_with_card(CardKind::Sticky);
    let card_index = app.active;
    {
        let doc = app.doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), card, false);
    }
    let target_index = app.docs.len();
    app.new_doc(templates::instantiate("blank-canvas"));
    app.doc_mut()
        .unwrap()
        .save_to(&directory.path().join("target.thinoda"), false)
        .unwrap();
    let target_key = app.doc().unwrap().key.clone();
    app.new_doc(templates::instantiate("blank-canvas"));
    app.doc_mut()
        .unwrap()
        .save_to(&directory.path().join("sentinel.thinoda"), false)
        .unwrap();
    let sentinel_key = app.doc().unwrap().key.clone();
    app.active = card_index;

    let ctx = egui::Context::default();
    app.mark_card_text_input_scope(&ctx, egui::ViewportId::ROOT);
    let command = Cmd::CloseTab(target_index);
    if after_editor {
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let doc = app.doc().unwrap();
            let pending = DeferredCardTextCommand {
                instance: doc.doc.instance_id(),
                sheet: doc.doc.active_index(),
                root: doc.doc.sheet().root,
                command: app.capture_card_text_command(&command),
            };
            ui.ctx().data_mut(|data| {
                data.insert_temp(
                    card_text_slot(egui::ViewportId::ROOT).with("deferred_commands"),
                    vec![pending],
                );
            });
        })
        .drop_without_applying_deltas();
    } else {
        let input = egui::RawInput {
            events: vec![egui::Event::Text("pending".into())],
            ..Default::default()
        };
        ctx.run_ui(input, |ui| app.exec(ui.ctx(), command.clone()))
            .drop_without_applying_deltas();
        assert!(ctx.data(|data| {
            data.get_temp::<Vec<DeferredCardTextOperation>>(
                card_text_slot(egui::ViewportId::ROOT).with("deferred_operations"),
            )
            .is_some()
        }));
    }

    match change {
        DeferredTabChange::EarlierTabClosed => app.close_tab_now(0),
        DeferredTabChange::TargetClosed => app.close_tab_now(target_index),
        DeferredTabChange::TargetMoved => {
            app.document_windows.insert(
                target_key.clone(),
                egui::ViewportId::from_hash_of("moved-close-target"),
            );
        }
    }
    ctx.run_ui(egui::RawInput::default(), |ui| {
        if after_editor {
            app.finish_deferred_card_text_commands(ui.ctx());
        } else {
            app.finish_deferred_card_text_operations(ui.ctx());
        }
    })
    .drop_without_applying_deltas();

    assert!(app.docs.iter().any(|doc| doc.key == sentinel_key));
    assert_eq!(
        app.docs.iter().any(|doc| doc.key == target_key),
        matches!(change, DeferredTabChange::TargetMoved)
    );
    if !matches!(change, DeferredTabChange::EarlierTabClosed) {
        assert!(
            app.toasts
                .texts()
                .any(|text| text == t("Not possible here"))
        );
    }
}

#[test]
fn deferred_tab_close_rebinds_indices_and_rejects_missing_or_moved_targets() {
    for after_editor in [false, true] {
        for change in [
            DeferredTabChange::EarlierTabClosed,
            DeferredTabChange::TargetClosed,
            DeferredTabChange::TargetMoved,
        ] {
            check_deferred_tab_target(after_editor, change);
        }
    }
}

#[test]
fn recreated_immediate_viewport_orders_input_on_its_first_frame() {
    let (mut app, id) = app_with_card(CardKind::Sticky);
    {
        let doc = app.doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
    }
    let child = egui::ViewportId::from_hash_of("recreated-card-window");
    let key = app.doc().unwrap().key.clone();
    app.document_windows.insert(key.clone(), child);
    app.window_active_documents.insert(child, key);
    let ctx = egui::Context::default();
    ctx.add_plugin(CardTextInputPlugin);
    app.mark_card_text_input_scope(&ctx, child);
    let mut first = egui::RawInput {
        viewport_id: child,
        events: vec![egui::Event::Text("first".into())],
        ..Default::default()
    };
    first.viewports.entry(child).or_default();
    ctx.run_ui(first, |_| {}).drop_without_applying_deltas();
    ctx.run_ui(egui::RawInput::default(), |_| {})
        .drop_without_applying_deltas();

    app.mark_card_text_input_scope(&ctx, child);
    let mut events = vec![egui::Event::Text("before".into())];
    events.extend(raw_key_press(Key::F10, Modifiers::ALT));
    events.push(egui::Event::Text("after".into()));
    let mut recreated = egui::RawInput {
        viewport_id: child,
        events,
        ..Default::default()
    };
    recreated.viewports.entry(child).or_default();
    let mut delivered = Vec::new();
    ctx.run_ui(recreated, |ui| {
        delivered.extend(ui.input(|input| input.events.clone()));
    })
    .drop_without_applying_deltas();
    assert_eq!(delivered, vec![egui::Event::Text("before".into())]);
}

fn assert_insert_toolbar_clears_title(h: &Harness<'_, ThinodaApp>) -> bool {
    let editor = h.get_by_label("Topic title").rect();
    let (visible, toolbar) = h.ctx.memory(|memory| {
        let id = egui::Id::unique("thinoda_editing_toolbar");
        (
            memory
                .areas()
                .visible_layer_ids()
                .iter()
                .any(|layer| layer.id == id),
            memory.area_rect(id),
        )
    });
    if visible {
        let toolbar = toolbar.unwrap();
        assert!(
            !toolbar.intersects(editor),
            "toolbar={toolbar:?}, editor={editor:?}"
        );
    }
    visible
}

#[test]
fn selected_sticky_and_text_controls_edit_whole_card_and_round_trip() {
    for kind in [CardKind::Sticky, CardKind::Text] {
        let (mut app, id) = app_with_card(kind);
        app.doc_mut()
            .unwrap()
            .doc
            .edit_sheet("Existing style", |sheet| {
                sheet.update_style(&[id], |style| {
                    style.fill = Some(Color::rgb(10, 20, 30));
                    style.strikethrough = Some(true);
                })
            })
            .unwrap();
        let mut h = harness(app, vec2(1200.0, 700.0));
        let before_open = h.state().doc().unwrap().doc.state_id();
        open_toolbar(&mut h);
        assert_eq!(h.state().doc().unwrap().doc.state_id(), before_open);
        assert_eq!(
            h.get_by_role_and_label(Role::Button, "Bold")
                .accesskit_node()
                .toggled(),
            Some(Toggled::False)
        );
        assert_eq!(
            h.get_by_role_and_label(Role::Button, "Strikethrough")
                .accesskit_node()
                .toggled(),
            Some(Toggled::True)
        );
        h.get_by_role_and_label(Role::Button, "Strikethrough")
            .click();
        h.run_steps(3);
        assert_eq!(style(&h, id).strikethrough, Some(false));
        assert!(h.state_mut().doc_mut().unwrap().doc.undo());
        assert_eq!(style(&h, id).strikethrough, Some(true));
        assert!(h.state_mut().doc_mut().unwrap().doc.redo());
        assert_eq!(style(&h, id).strikethrough, Some(false));
        h.run_steps(3);
        if h.query_by_role_and_label(Role::Button, "Bold").is_none() {
            open_toolbar(&mut h);
        }
        h.get_by_role_and_label(Role::Button, "Strikethrough")
            .click();
        h.run_steps(3);
        assert_eq!(style(&h, id).strikethrough, Some(true));
        h.get_by_role_and_label(Role::Button, "Bold").click();
        h.run_steps(3);
        assert_eq!(style(&h, id).bold, Some(true));
        assert!(h.state_mut().doc_mut().unwrap().doc.undo());
        assert_eq!(style(&h, id).bold, None);
        assert!(h.state_mut().doc_mut().unwrap().doc.redo());
        h.run_steps(3);
        assert_eq!(style(&h, id).bold, Some(true));
        assert_eq!(
            h.get_by_role_and_label(Role::Button, "Bold")
                .accesskit_node()
                .toggled(),
            Some(Toggled::True)
        );

        h.get_by_role_and_label(Role::Button, "Italic").click();
        h.run_steps(2);
        h.get_by_role_and_label(Role::Button, "Underline").click();
        h.run_steps(2);
        h.get_by_role_and_label(Role::Button, "Align Left").click();
        h.run_steps(2);
        assert_eq!(style(&h, id).italic, Some(true));
        assert_eq!(style(&h, id).underline, Some(true));
        assert_eq!(style(&h, id).align, Some(TextAlign::Left));
        assert_eq!(style(&h, id).fill, Some(Color::rgb(10, 20, 30)));
        assert_eq!(style(&h, id).strikethrough, Some(true));
        h.get_by_role_and_label(Role::Button, "Align Center")
            .click();
        h.run_steps(2);
        assert_eq!(style(&h, id).align, Some(TextAlign::Center));
        h.get_by_role_and_label(Role::Button, "Align Right").click();
        h.run_steps(2);
        assert_eq!(style(&h, id).align, Some(TextAlign::Right));
        h.get_by_role_and_label(Role::Button, "Align Left").click();
        h.run_steps(2);

        scroll_and_click(&mut h, Role::ComboBox, "Font Family");
        h.get_by_label("Monospace").click();
        h.run_steps(3);
        assert_eq!(style(&h, id).font_family.as_deref(), Some("monospace"));
        let size = h.get_by_role_and_label(Role::SpinButton, "Font Size");
        size.scroll_to_me();
        h.run_steps(3);
        h.get_by_role_and_label(Role::SpinButton, "Font Size")
            .click();
        h.run_steps(2);
        h.key_press_modifiers(Modifiers::COMMAND, Key::A);
        h.get_by_role_and_label(Role::SpinButton, "Font Size")
            .type_text("24");
        h.key_press(Key::Enter);
        h.run_steps(4);
        assert_eq!(style(&h, id).font_size, Some(24.0));
        assert_eq!(h.state().doc().unwrap().view.selection, [id]);
        assert_eq!(h.state().doc().unwrap().view.editing_topic(), None);

        scroll_and_click(&mut h, Role::ColorWell, "Text Color #E8455C");
        assert_eq!(style(&h, id).text_color, Some(Color::rgb(0xE8, 0x45, 0x5C)));
        let book = h.state().doc().unwrap().doc.workbook();
        let bytes =
            thinoda_io::export_bytes(book, thinoda_io::Format::Thinoda, &Default::default())
                .unwrap();
        let reopened = thinoda_io::import_bytes(thinoda_io::Format::Thinoda, &bytes).unwrap();
        assert_eq!(
            reopened.active().topic(id).unwrap().style,
            book.active().topic(id).unwrap().style
        );
        let svg =
            thinoda_io::export_bytes(book, thinoda_io::Format::Svg, &Default::default()).unwrap();
        let svg = String::from_utf8(svg).unwrap();
        let line = svg
            .lines()
            .find(|line| line.contains(&format!("id=\"topic-{id}-line-0\"")))
            .unwrap();
        assert!(line.contains("font-family=\"monospace\""), "{line}");
        assert!(line.contains("font-size=\"24.00\""), "{line}");
        assert!(line.contains("font-weight=\"bold\""), "{line}");
        assert!(line.contains("font-style=\"italic\""), "{line}");
        assert!(
            line.contains("text-decoration=\"underline line-through\""),
            "{line}"
        );
        assert!(line.to_ascii_uppercase().contains("#E8455C"), "{line}");

        scroll_and_click(&mut h, Role::Button, "Text Color · Auto");
        assert_eq!(style(&h, id).text_color, None);
        assert!(h.state_mut().doc_mut().unwrap().doc.undo());
        assert_eq!(style(&h, id).text_color, Some(Color::rgb(0xE8, 0x45, 0x5C)));
        h.run_steps(3);
        open_toolbar(&mut h);
        scroll_and_click(&mut h, Role::ComboBox, "Font Family");
        h.get_by_label("Auto").click();
        h.run_steps(3);
        assert_eq!(style(&h, id).font_family, None);
        assert!(h.state_mut().doc_mut().unwrap().doc.undo());
        assert_eq!(style(&h, id).font_family.as_deref(), Some("monospace"));
    }
}

#[test]
fn keyboard_controls_use_theme_state_and_close_without_changing_selection() {
    let (mut app, id) = app_with_card(CardKind::Sticky);
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Theme", |sheet| {
            let mut custom = thinoda_core::custom_theme::CustomTheme::from_settings(&sheet.theme);
            custom.roles.floating.bold = Some(true);
            custom.roles.floating.strikethrough = Some(true);
            sheet.theme.custom_theme = Some(custom);
            Ok(())
        })
        .unwrap();
    let mut h = harness(app, vec2(900.0, 600.0));
    h.get_by_role_and_label(Role::Button, card_text_label())
        .focus();
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(5);
    let bold = h.get_by_role_and_label(Role::Button, "Bold");
    assert!(bold.is_focused());
    assert_eq!(bold.accesskit_node().toggled(), Some(Toggled::True));
    h.key_press(Key::Space);
    h.run_steps(3);
    assert_eq!(style(&h, id).bold, Some(false));
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert!(h.get_by_role_and_label(Role::Button, "Italic").is_focused());
    h.key_press(Key::Space);
    h.run_steps(3);
    assert_eq!(style(&h, id).italic, Some(true));
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert!(
        h.get_by_role_and_label(Role::Button, "Underline")
            .is_focused()
    );
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert!(
        h.get_by_role_and_label(Role::Button, "Strikethrough")
            .is_focused()
    );
    assert_eq!(
        h.get_by_role_and_label(Role::Button, "Strikethrough")
            .accesskit_node()
            .toggled(),
        Some(Toggled::True)
    );
    h.key_press(Key::Space);
    h.run_steps(3);
    assert_eq!(style(&h, id).strikethrough, Some(false));
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(h.query_by_role_and_label(Role::Button, "Bold").is_none());
    assert_eq!(h.state().doc().unwrap().view.selection, [id]);
}

#[test]
fn keyboard_can_choose_and_reset_color_and_choose_a_font() {
    let (app, id) = app_with_card(CardKind::Text);
    let mut h = harness(app, vec2(640.0, 360.0));
    open_toolbar(&mut h);
    h.get_by_role_and_label(Role::ColorWell, "Text Color #E8455C")
        .focus();
    h.run_steps(5);
    h.key_press(Key::Space);
    h.run_steps(3);
    assert_eq!(style(&h, id).text_color, Some(Color::rgb(0xE8, 0x45, 0x5C)));
    h.get_by_role_and_label(Role::Button, "Text Color · Auto")
        .focus();
    h.run_steps(5);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(style(&h, id).text_color, None);

    h.get_by_role_and_label(Role::ComboBox, "Font Family")
        .focus();
    h.run_steps(4);
    h.key_press(Key::Space);
    h.run_steps(4);
    h.get_by_label("Monospace").focus();
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(style(&h, id).font_family.as_deref(), Some("monospace"));
    assert_eq!(h.state().doc().unwrap().view.selection, [id]);
}

#[test]
fn formatting_overlays_a_stationary_canvas_and_scrolls_in_compact_windows() {
    for dark in [false, true] {
        for size in [vec2(320.0, 240.0), vec2(640.0, 360.0), vec2(960.0, 540.0)] {
            let (mut app, id) = app_with_card(CardKind::Text);
            app.settings.theme = if dark {
                SettingsTheme::Dark
            } else {
                SettingsTheme::Light
            };
            let mut h = harness(app, size);
            let canvas_before = h.state().doc().unwrap().view.canvas_rect();
            let board_before = h
                .ctx
                .memory(|memory| memory.area_rect(egui::Id::unique("thinoda_board_toolbar")));
            if h.query_by_role_and_label(Role::Button, card_text_label())
                .is_some()
            {
                open_toolbar(&mut h);
            } else {
                open_toolbar_from_context(&mut h);
            }
            let canvas = h.state().doc().unwrap().view.canvas_rect();
            let bold = h.get_by_role_and_label(Role::Button, "Bold").rect();
            assert_eq!(canvas, canvas_before, "opening formatting moved the canvas");
            assert!(canvas.contains_rect(bold), "{bold:?} {canvas:?}");
            assert!(h.ctx.content_rect().contains_rect(bold));
            let board = h
                .ctx
                .memory(|memory| memory.area_rect(egui::Id::unique("thinoda_board_toolbar")));
            assert!(!board.unwrap().intersects(bold));
            assert_eq!(board, board_before, "opening formatting moved the toolbox");

            h.get_by_role_and_label(Role::ComboBox, "Font Family")
                .focus();
            h.run_steps(6);
            let family = h
                .get_by_role_and_label(Role::ComboBox, "Font Family")
                .rect();
            assert!(h.ctx.content_rect().contains_rect(family), "{family:?}");
            h.key_press(Key::Space);
            h.run_steps(4);
            h.get_by_label("Monospace").focus();
            h.key_press(Key::Enter);
            h.run_steps(3);
            assert_eq!(style(&h, id).font_family.as_deref(), Some("monospace"));

            let color = h.get_by_role_and_label(Role::ColorWell, "Text Color #FFFFFF");
            color.focus();
            h.run_steps(6);
            let rect = h
                .get_by_role_and_label(Role::ColorWell, "Text Color #FFFFFF")
                .rect();
            assert!(h.ctx.content_rect().contains_rect(rect), "{rect:?}");
            assert!(
                h.state()
                    .doc()
                    .unwrap()
                    .view
                    .canvas_rect()
                    .contains_rect(rect)
            );
            h.key_press(Key::Space);
            h.run_steps(3);
            assert_eq!(style(&h, id).text_color, Some(Color::WHITE));
        }
    }
}

#[test]
fn formatting_and_ink_options_share_a_stationary_canvas() {
    for size in [vec2(320.0, 240.0), vec2(640.0, 360.0), vec2(1000.0, 700.0)] {
        let (app, id) = app_with_card(CardKind::Text);
        let mut h = harness(app, size);
        if h.query_by_role_and_label(Role::Button, card_text_label())
            .is_some()
        {
            open_toolbar(&mut h);
        } else {
            open_toolbar_from_context(&mut h);
        }
        let canvas = h.state().doc().unwrap().view.canvas_rect();
        let board_id = egui::Id::unique("thinoda_board_toolbar");
        let board = h.ctx.memory(|memory| memory.area_rect(board_id));
        h.state_mut()
            .doc_mut()
            .unwrap()
            .view
            .set_board_tool(BoardTool::Pen);
        h.run_steps(8);
        assert_eq!(h.state().doc().unwrap().view.canvas_rect(), canvas);
        assert_eq!(h.ctx.memory(|memory| memory.area_rect(board_id)), board);
        let (card, ink) = h.ctx.memory(|memory| {
            (
                memory.area_rect(egui::Id::unique((
                    "thinoda_card_text_toolbar",
                    egui::ViewportId::ROOT,
                ))),
                memory.area_rect(egui::Id::unique((
                    "thinoda_ink_options",
                    egui::ViewportId::ROOT,
                ))),
            )
        });
        let (card, ink) = (card.unwrap(), ink.unwrap());
        assert!(canvas.contains_rect(card) && canvas.contains_rect(ink));
        assert!(!card.intersects(ink), "{size:?}: {card:?} {ink:?}");
        assert!(!board.unwrap().intersects(card));
        assert!(!board.unwrap().intersects(ink));

        h.get_by_role_and_label(Role::Button, "Bold").focus();
        h.run_steps(6);
        h.key_press(Key::Space);
        h.run_steps(4);
        assert_eq!(style(&h, id).bold, Some(true));
        h.get_by_role_and_label(Role::Button, "Ink width")
            .scroll_to_me();
        h.run_steps(6);
        h.get_by_role_and_label(Role::Button, "Ink width").click();
        h.run_steps(4);
        h.get_by_role_and_label(Role::Button, "Ink width: 8")
            .click();
        h.run_steps(4);
        assert_eq!(h.state().doc().unwrap().view.ink_settings.width, 8.0);
        assert!(
            h.state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .canvas
                .objects
                .is_empty()
        );
        assert_eq!(h.state().doc().unwrap().view.canvas_rect(), canvas);
    }
}

#[test]
fn compact_formatting_keeps_insert_actions_without_covering_the_card() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(320.0, 240.0));
    open_toolbar_from_context(&mut h);
    assert!(h.query_by_label("Whole-card text").is_some());
    assert!(
        !h.ctx.memory(|memory| {
            memory
                .areas()
                .visible_layer_ids()
                .iter()
                .any(|layer| layer.id == egui::Id::unique("thinoda_editing_toolbar"))
        }),
        "the floating insert toolbar must not obscure the card while formatting"
    );
    let action = h.get_by_role_and_label(Role::Button, "More insert actions");
    let canvas = h.state().doc().unwrap().view.canvas_rect();
    assert!(canvas.contains_rect(action.rect()));
    action.click();
    h.run_steps(4);
    h.get_by_role_and_label(Role::Button, "Subtopic").click();
    h.run_steps(4);
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .topic(id)
            .unwrap()
            .children
            .len(),
        1
    );
}

#[test]
fn formatting_overflow_keeps_marker_and_find_actions() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(320.0, 240.0));
    open_toolbar_from_context(&mut h);
    h.get_by_role_and_label(Role::Button, "More insert actions")
        .click();
    h.run_steps(4);
    scroll_and_click(&mut h, Role::Button, "Marker");
    scroll_and_click(&mut h, Role::Button, "Priority 1");
    assert!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .topic(id)
            .unwrap()
            .markers
            .iter()
            .any(|marker| marker == "priority-1")
    );
    h.run_steps(4);
    open_toolbar_from_context(&mut h);
    h.get_by_role_and_label(Role::Button, "More insert actions")
        .click();
    h.run_steps(4);
    scroll_and_click(&mut h, Role::Button, "Find and Replace…");
    assert!(h.state().search.open);
}

#[test]
fn stale_selection_document_and_locks_never_receive_an_edit() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    open_toolbar(&mut h);
    let target = h.state().card_text_target().unwrap().0;
    let second = h
        .state_mut()
        .doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Second card", |sheet| {
            sheet
                .add_card(
                    "Second",
                    Vec2::new(300.0, 0.0),
                    Card::new(CardKind::Text, Vec2::new(160.0, 90.0)),
                )
                .map_err(thinoda_core::EditError::Invalid)
        })
        .unwrap();
    h.state_mut().doc_mut().unwrap().view.select_only(second);
    let state = h.state().doc().unwrap().doc.state_id();
    assert!(
        h.state_mut()
            .apply_card_text_change(&target, CardTextChange::Bold(true))
            .is_none()
    );
    assert_eq!(h.state().doc().unwrap().doc.state_id(), state);
    assert_eq!(style(&h, id).bold, None);
    assert_eq!(style(&h, second).bold, None);
    h.run_steps(4);
    assert!(h.query_by_role_and_label(Role::Button, "Bold").is_none());

    h.state_mut().doc_mut().unwrap().view.select_only(id);
    h.run_steps(4);
    open_toolbar(&mut h);
    let target = h.state().card_text_target().unwrap().0;
    h.state_mut()
        .doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Lock card", |sheet| {
            sheet
                .set_board_members_locked(&[BoardMember::Card(id)], true)
                .map_err(thinoda_core::EditError::Invalid)
        })
        .unwrap();
    let state = h.state().doc().unwrap().doc.state_id();
    assert!(
        h.state_mut()
            .apply_card_text_change(&target, CardTextChange::Bold(true))
            .is_none()
    );
    h.run_steps(4);
    assert_eq!(h.state().doc().unwrap().doc.state_id(), state);
    assert_eq!(style(&h, id).bold, None);
    assert!(h.query_by_role_and_label(Role::Button, "Bold").is_none());
    assert!(
        h.get_by_role_and_label(Role::Button, card_text_label())
            .accesskit_node()
            .is_disabled()
    );

    h.state_mut()
        .new_doc(templates::instantiate("blank-canvas"));
    let state = h.state().doc().unwrap().doc.state_id();
    assert!(
        h.state_mut()
            .apply_card_text_change(&target, CardTextChange::Italic(true))
            .is_none()
    );
    assert_eq!(h.state().doc().unwrap().doc.state_id(), state);
}

#[test]
fn locked_group_or_member_never_enables_single_card_formatting() {
    for group_lock in [None, Some(false), Some(true)] {
        let (mut app, id) = app_with_card(CardKind::Sticky);
        app.doc_mut()
            .unwrap()
            .doc
            .edit_sheet("Group", |sheet| {
                let shape = sheet
                    .add_canvas_object(CanvasObject::shape(
                        ShapeKind::Rectangle,
                        BoardRect::from_center_size(Vec2::new(300.0, 0.0), Vec2::new(80.0, 80.0)),
                    ))
                    .map_err(thinoda_core::EditError::Invalid)?;
                let group = sheet
                    .group_board_members(&[BoardMember::Card(id), BoardMember::Object(shape)])
                    .map_err(thinoda_core::EditError::Invalid)?;
                if group_lock == Some(true) {
                    sheet
                        .set_canvas_group_locked(group, true)
                        .map_err(thinoda_core::EditError::Invalid)?;
                } else if group_lock == Some(false) {
                    sheet
                        .set_board_members_locked(&[BoardMember::Object(shape)], true)
                        .map_err(thinoda_core::EditError::Invalid)?;
                }
                Ok(())
            })
            .unwrap();
        let (target, enabled) = app.card_text_target().unwrap();
        assert!(!enabled);
        let before = app.doc().unwrap().doc.workbook().clone();
        let state = app.doc().unwrap().doc.state_id();
        assert!(
            app.apply_card_text_change(&target, CardTextChange::Underline(true))
                .is_none()
        );
        assert_eq!(app.doc().unwrap().doc.state_id(), state);
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        let h = harness(app, vec2(1000.0, 700.0));
        assert!(
            h.get_by_role_and_label(Role::Button, card_text_label())
                .accesskit_node()
                .is_disabled()
        );
    }
}

#[test]
fn active_title_draft_and_ime_are_not_taken_over_by_the_toolbar() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    open_toolbar(&mut h);
    let original = h
        .state()
        .doc()
        .unwrap()
        .doc
        .sheet()
        .topic(id)
        .unwrap()
        .title
        .clone();
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("Pending ");
    }
    h.run_steps(4);
    assert!(h.query_by_role_and_label(Role::Button, "Bold").is_some());
    assert_eq!(
        h.state().doc().unwrap().view.editing_text(),
        Some("Pending ")
    );
    h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
        text: "nihon".into(),
        active_range_chars: None,
    }));
    h.run_steps(2);
    assert!(
        h.get_by_role_and_label(Role::Button, "Bold")
            .accesskit_node()
            .is_disabled()
    );
    let pos = h
        .get_by_role_and_label(Role::Button, "Bold")
        .rect()
        .center();
    h.event(egui::Event::PointerMoved(pos));
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(1);
    }
    assert_eq!(style(&h, id).bold, None);
    assert_eq!(h.state().doc().unwrap().view.editing_topic(), Some(id));
    assert!(h.get_by_label("Topic title").is_focused());
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert_eq!(h.state().doc().unwrap().view.editing_topic(), Some(id));
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .topic(id)
            .unwrap()
            .title,
        original
    );
    h.event(egui::Event::Ime(egui::ImeEvent::Commit("日本".into())));
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Bold").click();
    h.run_steps(3);
    assert_eq!(style(&h, id).bold, Some(true));
    assert_eq!(h.state().doc().unwrap().view.editing_topic(), Some(id));
}

#[test]
fn pointer_created_cards_keep_their_title_editor_focused() {
    for tool in [BoardTool::Sticky, BoardTool::Text] {
        for batched in [false, true] {
            for same_frame in [false, true] {
                let mut app = ThinodaApp::headless();
                app.settings.ai.enabled = false;
                app.settings.show_format_panel = false;
                app.new_doc(templates::instantiate("blank-canvas"));
                app.doc_mut().unwrap().view.set_board_tool(tool);
                let mut h = harness(app, vec2(1252.0, 1278.0));
                let pos = egui::pos2(220.0, 400.0);
                h.event(egui::Event::PointerMoved(pos));
                h.event(egui::Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                });
                if !batched {
                    h.run_steps(1);
                }
                h.event(egui::Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                });
                if same_frame {
                    h.event(egui::Event::Text("Typed immediately".into()));
                }
                h.run_steps(1);
                if !same_frame {
                    h.event(egui::Event::Text("Typed immediately".into()));
                }
                h.run_steps(5);

                let doc = h.state().doc().unwrap();
                let id = doc.doc.sheet().floating[0];
                assert_eq!(
                    doc.view.editing_topic(),
                    Some(id),
                    "{tool:?}, {batched}, {same_frame}"
                );
                assert_eq!(
                    doc.view.editing_text(),
                    Some("Typed immediately"),
                    "{tool:?}, {batched}, {same_frame}"
                );
                assert!(
                    h.get_by_label("Topic title").is_focused(),
                    "{tool:?}, {batched}, {same_frame}"
                );
                h.key_press(Key::Enter);
                h.run_steps(3);
                assert_eq!(
                    h.state()
                        .doc()
                        .unwrap()
                        .doc
                        .sheet()
                        .topic(id)
                        .unwrap()
                        .title,
                    "Typed immediately",
                    "{tool:?}, {batched}, {same_frame}"
                );
            }
        }
    }
}

#[test]
fn discarded_creation_pass_keeps_one_focused_card_editor() {
    for (tool, same_frame) in [
        (BoardTool::Sticky, false),
        (BoardTool::Sticky, true),
        (BoardTool::Text, false),
        (BoardTool::Text, true),
    ] {
        let mut app = ThinodaApp::headless();
        app.settings.ai.enabled = false;
        app.settings.show_format_panel = false;
        app.new_doc(templates::instantiate("blank-canvas"));
        app.doc_mut().unwrap().view.set_board_tool(tool);
        let discard = std::rc::Rc::new(std::cell::Cell::new(false));
        let request_discard = discard.clone();
        let discarded_frame = std::rc::Rc::new(std::cell::Cell::new(None));
        let observed_frame = discarded_frame.clone();
        let extra_pass = std::rc::Rc::new(std::cell::Cell::new(false));
        let observed_pass = extra_pass.clone();
        let mut h = Harness::builder()
            .with_size(vec2(1252.0, 1278.0))
            .with_step_dt(1.0 / 60.0)
            .with_context_setup(|ctx| {
                ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
                ctx.add_plugin(CardTextInputPlugin);
                ctx.options_mut(|options| options.max_passes = 5.try_into().unwrap());
            })
            .build_ui_state(
                move |ui, app: &mut ThinodaApp| {
                    app.draw(ui);
                    if request_discard.get() && ui.input(|input| input.pointer.any_released()) {
                        request_discard.set(false);
                        observed_frame.set(Some(ui.ctx().cumulative_frame_nr()));
                        ui.ctx().request_discard("exercise card creation");
                    } else if observed_frame.get() == Some(ui.ctx().cumulative_frame_nr()) {
                        observed_pass.set(true);
                    }
                },
                app,
            );
        h.run_steps(6);
        let pos = egui::pos2(220.0, 400.0);
        h.event(egui::Event::PointerMoved(pos));
        h.event(egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(1);
        discard.set(true);
        h.event(egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        });
        if same_frame {
            h.event(egui::Event::Text("After discarded pass".into()));
        }
        h.run_steps(1);
        if !same_frame {
            h.event(egui::Event::Text("After discarded pass".into()));
        }
        h.run_steps(5);

        assert!(!discard.get());
        assert!(
            extra_pass.get(),
            "{tool:?}, {same_frame}: creation pass was not replayed"
        );
        let doc = h.state().doc().unwrap();
        assert_eq!(doc.doc.sheet().floating.len(), 1, "{tool:?}, {same_frame}");
        assert_eq!(
            doc.view.editing_topic(),
            Some(doc.doc.sheet().floating[0]),
            "{tool:?}, {same_frame}"
        );
        assert_eq!(
            doc.view.editing_text(),
            Some("After discarded pass"),
            "{tool:?}, {same_frame}"
        );
        assert!(
            h.get_by_label("Topic title").is_focused(),
            "{tool:?}, {same_frame}"
        );
    }
}

#[test]
fn pointer_after_card_creation_is_not_stolen_by_initial_editor_focus() {
    let mut app = ThinodaApp::headless();
    app.settings.ai.enabled = false;
    app.settings.show_format_panel = false;
    app.new_doc(templates::instantiate("blank-canvas"));
    app.doc_mut()
        .unwrap()
        .view
        .set_board_tool(BoardTool::Sticky);
    let mut h = harness(app, vec2(1252.0, 1278.0));
    let create = egui::pos2(220.0, 400.0);
    h.event(egui::Event::PointerMoved(create));
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: create,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(1);
    }

    let outside = egui::pos2(700.0, 550.0);
    h.event(egui::Event::PointerMoved(outside));
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: outside,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(1);
    }
    h.run_steps(3);
    let doc = h.state().doc().unwrap();
    assert_eq!(doc.doc.sheet().floating.len(), 2);
    assert_eq!(doc.view.editing_topic(), Some(doc.doc.sheet().floating[1]));
    assert!(h.get_by_label("Topic title").is_focused());
}

#[test]
fn formatting_during_a_card_title_draft_preserves_the_draft() {
    for kind in [CardKind::Sticky, CardKind::Text] {
        let (app, id) = app_with_card(kind);
        let mut h = harness(app, vec2(1000.0, 700.0));
        open_toolbar(&mut h);
        {
            let doc = h.state_mut().doc_mut().unwrap();
            doc.view.begin_edit(doc.doc.sheet(), id, false);
            doc.view.set_editing_text("Pending title");
        }
        h.run_steps(4);
        assert_eq!(
            h.state().doc().unwrap().view.editing_text(),
            Some("Pending title")
        );

        let pos = h
            .get_by_role_and_label(Role::Button, "Bold")
            .rect()
            .center();
        h.event(egui::Event::PointerMoved(pos));
        h.event(egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(1);
        assert_eq!(
            h.state().doc().unwrap().view.editing_text(),
            Some("Pending title"),
            "draft after pointer press"
        );
        h.event(egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(3);
        assert_eq!(style(&h, id).bold, Some(true));
        assert_eq!(
            h.state().doc().unwrap().view.editing_text(),
            Some("Pending title")
        );
        assert_eq!(
            h.state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .topic(id)
                .unwrap()
                .title,
            "Card title"
        );

        scroll_and_click(&mut h, Role::ColorWell, "Text Color #E8455C");
        assert_eq!(style(&h, id).text_color, Some(Color::rgb(0xE8, 0x45, 0x5C)));
        assert_eq!(
            h.state().doc().unwrap().view.editing_text(),
            Some("Pending title")
        );
        h.get_by_label("Topic title").focus();
        h.run_steps(2);
        h.event(egui::Event::Text("!".into()));
        h.run_steps(2);
        assert_eq!(
            h.state().doc().unwrap().view.editing_text(),
            Some("Pending title!")
        );
        h.key_press(Key::Enter);
        h.run_steps(3);
        assert_eq!(h.state().doc().unwrap().view.editing_topic(), None);
        assert_eq!(
            h.state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .topic(id)
                .unwrap()
                .title,
            "Pending title!"
        );
    }
}

#[test]
fn discarded_ui_pass_does_not_repeat_a_formatting_click() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let discard = std::rc::Rc::new(std::cell::Cell::new(false));
    let request_discard = discard.clone();
    let discarded_frame = std::rc::Rc::new(std::cell::Cell::new(None));
    let observed_frame = discarded_frame.clone();
    let extra_pass = std::rc::Rc::new(std::cell::Cell::new(false));
    let observed_pass = extra_pass.clone();
    let mut h = Harness::builder()
        .with_size(vec2(1000.0, 700.0))
        .with_step_dt(1.0 / 60.0)
        .with_context_setup(|ctx| {
            ctx.options_mut(|options| options.max_passes = 5.try_into().unwrap())
        })
        .build_ui_state(
            move |ui, app: &mut ThinodaApp| {
                app.draw(ui);
                if request_discard.get() && ui.input(|input| input.pointer.any_released()) {
                    request_discard.set(false);
                    observed_frame.set(Some(ui.ctx().cumulative_frame_nr()));
                    ui.ctx().request_discard("exercise formatting replay");
                } else if observed_frame.get() == Some(ui.ctx().cumulative_frame_nr()) {
                    observed_pass.set(true);
                }
            },
            app,
        );
    h.run_steps(6);
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("Discarded pass draft");
    }
    h.run_steps(5);
    discard.set(true);
    h.get_by_role_and_label(Role::Button, "Bold").click();
    h.run_steps(4);
    assert!(!discard.get());
    assert!(
        extra_pass.get(),
        "the click must be replayed in another pass"
    );
    assert_eq!(style(&h, id).bold, Some(true));
    assert_eq!(
        h.state().doc().unwrap().view.editing_text(),
        Some("Discarded pass draft")
    );
}

#[test]
fn discarded_pass_does_not_replay_queued_keyboard_input_early() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let discard = std::rc::Rc::new(std::cell::Cell::new(false));
    let request_discard = discard.clone();
    let discarded_frame = std::rc::Rc::new(std::cell::Cell::new(None));
    let observed_frame = discarded_frame.clone();
    let replayed_early = std::rc::Rc::new(std::cell::Cell::new(false));
    let observed_early = replayed_early.clone();
    let mut h = Harness::builder()
        .with_size(vec2(1000.0, 700.0))
        .with_step_dt(1.0 / 60.0)
        .with_context_setup(|ctx| {
            ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            ctx.add_plugin(CardTextInputPlugin);
            ctx.options_mut(|options| options.max_passes = 5.try_into().unwrap());
        })
        .build_ui_state(
            move |ui, app: &mut ThinodaApp| {
                let text = ui.input(|input| input.events.contains(&egui::Event::Text("!".into())));
                let shortcut = ui.input(|input| {
                    input.events.iter().any(|event| {
                        matches!(
                            event,
                            egui::Event::Key {
                                key: Key::F10,
                                pressed: true,
                                ..
                            }
                        )
                    })
                });
                if observed_frame.get() == Some(ui.ctx().cumulative_frame_nr()) && shortcut {
                    observed_early.set(true);
                }
                app.draw(ui);
                if request_discard.get() && text {
                    request_discard.set(false);
                    observed_frame.set(Some(ui.ctx().cumulative_frame_nr()));
                    ui.ctx().request_discard("exercise queued input");
                }
            },
            app,
        );
    h.run_steps(6);
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("Discarded");
    }
    h.run_steps(5);
    discard.set(true);
    h.input_mut().events.push(egui::Event::Text("!".into()));
    h.input_mut()
        .events
        .extend(raw_key_press(Key::F10, Modifiers::ALT));
    let [modifiers, down, up, clear] = raw_key_press(Key::Space, Modifiers::NONE);
    h.input_mut()
        .events
        .extend([modifiers, down, egui::Event::Text(" ".into()), up, clear]);
    run_input_hook_steps(&mut h, 7);
    assert!(!discard.get());
    assert!(!replayed_early.get());
    assert_eq!(
        h.state().doc().unwrap().view.editing_text(),
        Some("Discarded!")
    );
    assert_eq!(style(&h, id).bold, Some(true));
}

#[test]
fn keyboard_formatting_and_closing_return_to_the_title_draft() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("Keyboard draft");
    }
    h.run_steps(5);
    assert!(h.get_by_label("Topic title").is_focused());
    h.get_by_role_and_label(Role::Button, "Bold").focus();
    h.run_steps(2);
    assert_eq!(h.state().doc().unwrap().view.editing_topic(), Some(id));
    h.key_press(Key::Space);
    h.run_steps(2);
    assert_eq!(style(&h, id).bold, Some(true));
    assert!(h.get_by_role_and_label(Role::Button, "Bold").is_focused());
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert!(h.get_by_role_and_label(Role::Button, "Italic").is_focused());
    h.key_press(Key::Space);
    h.run_steps(2);
    assert_eq!(style(&h, id).italic, Some(true));
    assert_eq!(
        h.state().doc().unwrap().view.editing_text(),
        Some("Keyboard draft")
    );
    h.key_press(Key::Escape);
    h.run_steps(4);
    assert!(h.query_by_role_and_label(Role::Button, "Bold").is_none());
    assert!(h.get_by_label("Topic title").is_focused());
    assert_eq!(
        h.state().doc().unwrap().view.editing_text(),
        Some("Keyboard draft")
    );
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().view.editing_topic(), None);
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
    }
    h.run_steps(5);
    assert!(!h.get_by_role_and_label(Role::Button, "Bold").is_focused());
    assert!(h.get_by_label("Topic title").is_focused());
}

#[test]
fn keyboard_shortcut_moves_between_the_editor_and_formatting() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, true);
        doc.view.set_editing_text("Selected draft");
    }
    h.run_steps(5);
    h.key_press_modifiers(Modifiers::ALT, Key::F10);
    h.run_steps(3);
    assert!(h.get_by_role_and_label(Role::Button, "Bold").is_focused());
    assert_eq!(h.state().doc().unwrap().view.editing_topic(), Some(id));
    h.key_press(Key::Space);
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::ALT, Key::F10);
    h.run_steps(3);
    assert!(h.get_by_label("Topic title").is_focused());
    h.event(egui::Event::Text("Replacement".into()));
    h.run_steps(2);
    assert_eq!(
        h.state().doc().unwrap().view.editing_text(),
        Some("Replacement")
    );
    assert_eq!(style(&h, id).bold, Some(true));
    h.key_press_modifiers(Modifiers::ALT, Key::F10);
    h.run_steps(2);
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(h.query_by_role_and_label(Role::Button, "Bold").is_none());
    h.key_press_modifiers(Modifiers::ALT, Key::F10);
    h.run_steps(3);
    assert!(h.get_by_role_and_label(Role::Button, "Bold").is_focused());
    assert_eq!(
        h.state().doc().unwrap().view.editing_text(),
        Some("Replacement")
    );
}

#[test]
fn text_before_the_formatting_shortcut_in_one_frame_is_not_lost() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("Before");
    }
    h.run_steps(5);
    h.input_mut()
        .events
        .push(egui::Event::Text(" appended".into()));
    h.input_mut()
        .events
        .extend(raw_key_press(Key::F10, Modifiers::ALT));
    h.run_steps(3);
    assert_eq!(
        h.state().doc().unwrap().view.editing_text(),
        Some("Before appended")
    );
    assert!(h.get_by_role_and_label(Role::Button, "Bold").is_focused());
}

#[test]
fn typing_a_space_before_the_formatting_shortcut_keeps_the_title_and_style() {
    let (mut app, id) = app_with_card(CardKind::Text);
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Existing style", |sheet| {
            sheet.update_style(&[id], |style| style.bold = Some(true))
        })
        .unwrap();
    let mut h = harness(app, vec2(1000.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, true);
        doc.view.set_editing_text("Replacement");
    }
    h.run_steps(5);
    let frame = h.ctx.cumulative_frame_nr();
    let ctrl = Modifiers {
        ctrl: true,
        command: true,
        ..Modifiers::NONE
    };
    let key = |key, pressed, modifiers| egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed,
        repeat: false,
        modifiers,
    };
    let mut events = vec![
        egui::Event::ModifiersChanged(ctrl),
        key(Key::ControlLeft, true, ctrl),
        key(Key::End, true, ctrl),
        key(Key::End, false, ctrl),
        egui::Event::ModifiersChanged(Modifiers::NONE),
        key(Key::ControlLeft, false, Modifiers::NONE),
    ];
    for (letter, text) in [
        (Key::Space, " "),
        (Key::S, "s"),
        (Key::I, "i"),
        (Key::Z, "z"),
        (Key::E, "e"),
    ] {
        events.push(key(letter, true, Modifiers::NONE));
        events.push(egui::Event::Text(text.into()));
        events.push(key(letter, false, Modifiers::NONE));
    }
    events.extend([
        egui::Event::ModifiersChanged(Modifiers::ALT),
        key(Key::AltLeft, true, Modifiers::ALT),
        key(Key::F10, true, Modifiers::ALT),
        key(Key::F10, false, Modifiers::ALT),
        egui::Event::ModifiersChanged(Modifiers::NONE),
        key(Key::AltLeft, false, Modifiers::NONE),
    ]);
    h.input_mut().events.extend(events);
    h.run_steps(1);
    assert_eq!(h.ctx.cumulative_frame_nr(), frame + 1);
    h.run_steps(2);
    assert_eq!(
        h.state().doc().unwrap().view.editing_text(),
        Some("Replacement size")
    );
    assert_eq!(style(&h, id).bold, Some(true));
    assert!(h.get_by_role_and_label(Role::Button, "Bold").is_focused());
}

#[test]
fn formatting_before_the_return_shortcut_in_one_frame_is_not_lost() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("Before");
    }
    h.run_steps(5);
    h.key_press_modifiers(Modifiers::ALT, Key::F10);
    h.run_steps(3);
    h.input_mut()
        .events
        .extend(raw_key_press(Key::Space, Modifiers::NONE));
    h.input_mut()
        .events
        .extend(raw_key_press(Key::F10, Modifiers::ALT));
    h.run_steps(3);
    assert_eq!(style(&h, id).bold, Some(true));
    assert_eq!(h.state().doc().unwrap().view.editing_text(), Some("Before"));
    assert!(h.get_by_label("Topic title").is_focused());
}

#[test]
fn keyboard_formatting_after_entering_the_toolbar_in_one_frame_does_not_edit_the_title() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("Before");
    }
    h.run_steps(5);
    h.input_mut()
        .events
        .extend(raw_key_press(Key::F10, Modifiers::ALT));
    let [modifiers, down, up, clear] = raw_key_press(Key::Space, Modifiers::NONE);
    h.input_mut()
        .events
        .extend([modifiers, down, egui::Event::Text(" ".into()), up, clear]);
    run_input_hook_steps(&mut h, 4);
    assert_eq!(style(&h, id).bold, Some(true));
    assert_eq!(h.state().doc().unwrap().view.editing_text(), Some("Before"));
}

#[test]
fn typing_to_start_a_card_title_before_formatting_in_one_frame_is_ordered() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    h.input_mut().events.push(egui::Event::Text("Typed".into()));
    h.input_mut()
        .events
        .extend(raw_key_press(Key::F10, Modifiers::ALT));
    let [modifiers, down, up, clear] = raw_key_press(Key::Space, Modifiers::NONE);
    h.input_mut()
        .events
        .extend([modifiers, down, egui::Event::Text(" ".into()), up, clear]);
    run_input_hook_steps(&mut h, 7);
    assert_eq!(h.state().doc().unwrap().view.editing_text(), Some("Typed"));
    assert_eq!(style(&h, id).bold, Some(true));
}

#[test]
fn toolbar_space_before_returning_to_the_title_in_one_frame_is_not_inserted() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("Before");
    }
    h.run_steps(5);
    h.key_press_modifiers(Modifiers::ALT, Key::F10);
    h.run_steps(3);
    let [modifiers, down, up, clear] = raw_key_press(Key::Space, Modifiers::NONE);
    h.input_mut()
        .events
        .extend([modifiers, down, egui::Event::Text(" ".into()), up, clear]);
    h.input_mut()
        .events
        .extend(raw_key_press(Key::F10, Modifiers::ALT));
    run_input_hook_steps(&mut h, 4);
    assert_eq!(style(&h, id).bold, Some(true));
    assert_eq!(h.state().doc().unwrap().view.editing_text(), Some("Before"));
    assert!(h.get_by_label("Topic title").is_focused());
}

#[test]
fn typing_after_returning_to_the_title_in_one_frame_replaces_the_original_selection() {
    let (app, id) = app_with_card(CardKind::Text);
    let mut h = harness(app, vec2(1000.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, true);
        doc.view.set_editing_text("Selected title");
    }
    h.run_steps(5);
    h.key_press_modifiers(Modifiers::ALT, Key::F10);
    h.run_steps(3);
    h.input_mut()
        .events
        .extend(raw_key_press(Key::F10, Modifiers::ALT));
    h.input_mut()
        .events
        .push(egui::Event::Text("Replacement".into()));
    run_input_hook_steps(&mut h, 4);
    assert_eq!(
        h.state().doc().unwrap().view.editing_text(),
        Some("Replacement")
    );
    assert!(h.get_by_label("Topic title").is_focused());
}

#[test]
fn keyboard_tab_after_opening_formatting_in_one_frame_reaches_the_next_control() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("Before");
    }
    h.run_steps(5);
    h.input_mut()
        .events
        .extend(raw_key_press(Key::F10, Modifiers::ALT));
    h.input_mut()
        .events
        .extend(raw_key_press(Key::Tab, Modifiers::NONE));
    let [modifiers, down, up, clear] = raw_key_press(Key::Space, Modifiers::NONE);
    h.input_mut()
        .events
        .extend([modifiers, down, egui::Event::Text(" ".into()), up, clear]);
    run_input_hook_steps(&mut h, 7);
    assert_eq!(style(&h, id).italic, Some(true));
    assert_eq!(style(&h, id).bold, None);
    assert_eq!(h.state().doc().unwrap().view.editing_text(), Some("Before"));
}

#[test]
fn repeated_keyboard_tabs_in_one_frame_follow_each_control_in_order() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("Before");
    }
    h.run_steps(5);
    h.input_mut()
        .events
        .extend(raw_key_press(Key::F10, Modifiers::ALT));
    for _ in 0..2 {
        h.input_mut()
            .events
            .extend(raw_key_press(Key::Tab, Modifiers::NONE));
    }
    let [modifiers, down, up, clear] = raw_key_press(Key::Space, Modifiers::NONE);
    h.input_mut()
        .events
        .extend([modifiers, down, egui::Event::Text(" ".into()), up, clear]);
    run_input_hook_steps(&mut h, 9);
    assert_eq!(style(&h, id).underline, Some(true));
    assert_eq!(style(&h, id).bold, None);
    assert_eq!(style(&h, id).italic, None);
    assert_eq!(h.state().doc().unwrap().view.editing_text(), Some("Before"));
}

#[test]
fn ime_commit_before_a_formatting_shortcut_in_one_frame_is_preserved() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("Before");
    }
    h.run_steps(5);
    h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
        text: "candidate".into(),
        active_range_chars: None,
    }));
    h.run_steps(2);
    h.input_mut()
        .events
        .push(egui::Event::Ime(egui::ImeEvent::Commit("日本".into())));
    h.input_mut()
        .events
        .extend(raw_key_press(Key::F10, Modifiers::ALT));
    let [modifiers, down, up, clear] = raw_key_press(Key::Space, Modifiers::NONE);
    h.input_mut()
        .events
        .extend([modifiers, down, egui::Event::Text(" ".into()), up, clear]);
    run_input_hook_steps(&mut h, 7);
    assert_eq!(style(&h, id).bold, Some(true));
    assert_eq!(
        h.state().doc().unwrap().view.editing_text(),
        Some("Before日本")
    );
}

#[test]
fn font_popup_and_numeric_editor_do_not_commit_the_title_draft() {
    let (app, id) = app_with_card(CardKind::Text);
    let mut h = harness(app, vec2(1000.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("Font draft");
    }
    h.run_steps(5);
    scroll_and_click(&mut h, Role::ComboBox, "Font Family");
    assert_eq!(h.state().doc().unwrap().view.editing_topic(), Some(id));
    h.get_by_label("Monospace").click();
    h.run_steps(3);
    assert_eq!(style(&h, id).font_family.as_deref(), Some("monospace"));
    assert_eq!(h.state().doc().unwrap().view.editing_topic(), Some(id));
    scroll_and_click(&mut h, Role::SpinButton, "Font Size");
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.event(egui::Event::Text("2".into()));
    h.run_steps(1);
    assert_eq!(style(&h, id).font_size, Some(6.0));
    h.event(egui::Event::Text("4".into()));
    h.run_steps(1);
    assert_eq!(style(&h, id).font_size, Some(24.0));
    h.key_press(Key::Enter);
    h.run_steps(4);
    assert_eq!(style(&h, id).font_size, Some(24.0));
    assert_eq!(
        h.state().doc().unwrap().view.editing_text(),
        Some("Font draft")
    );
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .topic(id)
            .unwrap()
            .title,
        "Card title"
    );
}

#[test]
fn escaping_font_size_keeps_applied_style_and_title_draft() {
    let (app, id) = app_with_card(CardKind::Text);
    let mut h = harness(app, vec2(1000.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("Size draft");
    }
    h.run_steps(5);
    scroll_and_click(&mut h, Role::SpinButton, "Font Size");
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.event(egui::Event::Text("26".into()));
    h.run_steps(2);
    assert_eq!(style(&h, id).font_size, Some(26.0));
    assert_eq!(
        h.ctx.data(|data| data
            .get_temp::<bool>(card_text_slot(egui::ViewportId::ROOT).with("size_editing"))),
        Some(true)
    );
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert_eq!(style(&h, id).font_size, Some(26.0));
    assert_eq!(
        h.state().doc().unwrap().view.editing_text(),
        Some("Size draft")
    );
    assert!(h.get_by_label("Topic title").is_focused());
}

#[test]
fn applied_font_size_and_another_control_are_both_preserved() {
    for action in ["Bold", "Text Color #E8455C", "Close"] {
        let (app, id) = app_with_card(CardKind::Text);
        let mut h = harness(app, vec2(1600.0, 700.0));
        {
            let doc = h.state_mut().doc_mut().unwrap();
            doc.view.begin_edit(doc.doc.sheet(), id, false);
            doc.view.set_editing_text("Pending title");
        }
        h.run_steps(5);
        scroll_and_click(&mut h, Role::SpinButton, "Font Size");
        h.key_press_modifiers(Modifiers::COMMAND, Key::A);
        h.event(egui::Event::Text("27".into()));
        h.run_steps(2);
        assert_eq!(style(&h, id).font_size, Some(27.0));

        let role = if action == "Text Color #E8455C" {
            Role::ColorWell
        } else {
            Role::Button
        };
        let pos = h.get_by_role_and_label(role, action).rect().center();
        h.event(egui::Event::PointerMoved(pos));
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            });
        }
        h.run_steps(4);
        assert_eq!(style(&h, id).font_size, Some(27.0), "{action}");
        match action {
            "Bold" => assert_eq!(style(&h, id).bold, Some(true)),
            "Text Color #E8455C" => {
                assert_eq!(style(&h, id).text_color, Some(Color::rgb(0xE8, 0x45, 0x5C)));
            }
            "Close" => assert!(h.query_by_role_and_label(Role::Button, "Bold").is_none()),
            _ => unreachable!(),
        }
        assert_eq!(
            h.state().doc().unwrap().view.editing_text(),
            Some("Pending title")
        );
    }
}

#[test]
fn saving_while_font_size_is_being_edited_keeps_the_entered_value() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("card-size.thinoda");
    let (mut app, id) = app_with_card(CardKind::Text);
    app.doc_mut().unwrap().save_to(&path, false).unwrap();
    let mut h = harness(app, vec2(1600.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("Save title");
    }
    h.run_steps(5);
    scroll_and_click(&mut h, Role::SpinButton, "Font Size");
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.event(egui::Event::Text("27".into()));
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::S);
    h.run_steps(5);
    let doc = h.state().doc().unwrap();
    assert_eq!(
        doc.doc.sheet().topic(id).unwrap().style.font_size,
        Some(27.0)
    );
    assert_eq!(doc.doc.sheet().topic(id).unwrap().title, "Save title");
    assert!(!doc.is_dirty());
    let saved = thinoda_io::import_file(&path).unwrap();
    assert_eq!(
        saved.active().topic(id).unwrap().style.font_size,
        Some(27.0)
    );
    assert_eq!(saved.active().topic(id).unwrap().title, "Save title");
}

#[test]
fn last_font_size_keystroke_and_formatting_click_in_one_frame_are_preserved() {
    let (app, id) = app_with_card(CardKind::Text);
    let mut h = harness(app, vec2(1600.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("Fast input");
    }
    h.run_steps(5);
    scroll_and_click(&mut h, Role::SpinButton, "Font Size");
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(1);
    let bold = h
        .get_by_role_and_label(Role::Button, "Bold")
        .rect()
        .center();
    h.input_mut().events.push(egui::Event::Text("27".into()));
    h.input_mut().events.push(egui::Event::PointerMoved(bold));
    for pressed in [true, false] {
        h.input_mut().events.push(egui::Event::PointerButton {
            pos: bold,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    h.run_steps(4);
    assert_eq!(style(&h, id).font_size, Some(27.0));
    assert_eq!(style(&h, id).bold, Some(true));
    assert_eq!(
        h.state().doc().unwrap().view.editing_text(),
        Some("Fast input")
    );
}

#[test]
fn last_font_size_keystroke_and_save_in_one_frame_are_preserved() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("fast-size-save.thinoda");
    let (mut app, id) = app_with_card(CardKind::Text);
    app.doc_mut().unwrap().save_to(&path, false).unwrap();
    let mut h = harness(app, vec2(1600.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("Fast save");
    }
    h.run_steps(5);
    scroll_and_click(&mut h, Role::SpinButton, "Font Size");
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(1);
    h.input_mut().events.push(egui::Event::Text("27".into()));
    h.input_mut()
        .events
        .extend(raw_key_press(Key::S, Modifiers::COMMAND));
    h.run_steps(5);
    let saved = thinoda_io::import_file(&path).unwrap();
    assert_eq!(
        saved.active().topic(id).unwrap().style.font_size,
        Some(27.0)
    );
    assert_eq!(saved.active().topic(id).unwrap().title, "Fast save");
}

#[test]
fn selected_card_font_size_and_save_in_one_frame_are_preserved() {
    let directory = tempfile::tempdir().unwrap();
    for kind in [CardKind::Sticky, CardKind::Text] {
        let path = directory.path().join(format!("{kind:?}.thinoda"));
        let (mut app, id) = app_with_card(kind);
        app.doc_mut().unwrap().save_to(&path, false).unwrap();
        let mut h = harness(app, vec2(1600.0, 700.0));
        open_toolbar(&mut h);
        scroll_and_click(&mut h, Role::SpinButton, "Font Size");
        h.key_press_modifiers(Modifiers::COMMAND, Key::A);
        h.run_steps(1);
        h.input_mut().events.push(egui::Event::Text("27".into()));
        h.input_mut()
            .events
            .extend(raw_key_press(Key::S, Modifiers::COMMAND));
        run_input_hook_steps(&mut h, 5);
        let saved = thinoda_io::import_file(&path).unwrap();
        assert_eq!(
            saved.active().topic(id).unwrap().style.font_size,
            Some(27.0),
            "{kind:?}"
        );
    }
}

#[test]
fn last_card_title_keystroke_and_save_in_one_frame_are_preserved() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("fast-title-save.thinoda");
    let (mut app, id) = app_with_card(CardKind::Text);
    app.doc_mut().unwrap().save_to(&path, false).unwrap();
    let mut h = harness(app, vec2(1600.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("Fast save");
    }
    h.run_steps(5);
    h.input_mut().events.push(egui::Event::Text("!".into()));
    h.input_mut()
        .events
        .extend(raw_key_press(Key::S, Modifiers::COMMAND));
    h.run_steps(5);
    let saved = thinoda_io::import_file(&path).unwrap();
    assert_eq!(saved.active().topic(id).unwrap().title, "Fast save!");
}

#[test]
fn card_title_input_after_save_is_not_included_in_the_earlier_save() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ordered-save.thinoda");
    let (mut app, id) = app_with_card(CardKind::Sticky);
    app.doc_mut().unwrap().save_to(&path, false).unwrap();
    let mut h = harness(app, vec2(1600.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("Ordered save");
    }
    h.run_steps(5);
    h.input_mut().events.push(egui::Event::Text("!".into()));
    h.input_mut()
        .events
        .extend(raw_key_press(Key::S, Modifiers::COMMAND));
    h.input_mut().events.push(egui::Event::Text("?".into()));
    h.run_steps(8);
    let saved = thinoda_io::import_file(&path).unwrap();
    assert_eq!(saved.active().topic(id).unwrap().title, "Ordered save!");
    let doc = h.state().doc().unwrap();
    let current = doc
        .view
        .editing_text()
        .unwrap_or(&doc.doc.sheet().topic(id).unwrap().title);
    assert_eq!(current, "Ordered save!?");
    assert!(doc.is_dirty());
}

#[test]
fn card_title_selection_survives_save_with_queued_typing() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("selected-save.thinoda");
    let (mut app, id) = app_with_card(CardKind::Text);
    app.doc_mut().unwrap().save_to(&path, false).unwrap();
    let mut h = harness(app, vec2(1600.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("Selected title");
    }
    h.run_steps(5);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(2);
    h.input_mut()
        .events
        .extend(raw_key_press(Key::S, Modifiers::COMMAND));
    h.input_mut()
        .events
        .push(egui::Event::Text("Replacement".into()));
    h.run_steps(8);
    let saved = thinoda_io::import_file(&path).unwrap();
    assert_eq!(saved.active().topic(id).unwrap().title, "Selected title");
    let doc = h.state().doc().unwrap();
    assert_eq!(doc.view.editing_text(), Some("Replacement"));
    assert!(doc.is_dirty());
}

#[test]
fn active_font_size_input_continues_after_save_in_event_order() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("continued-size-save.thinoda");
    let (mut app, id) = app_with_card(CardKind::Sticky);
    app.doc_mut().unwrap().save_to(&path, false).unwrap();
    let mut h = harness(app, vec2(1600.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("Size draft");
    }
    h.run_steps(5);
    scroll_and_click(&mut h, Role::SpinButton, "Font Size");
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(2);
    h.input_mut().events.push(egui::Event::Text("6".into()));
    h.input_mut()
        .events
        .extend(raw_key_press(Key::S, Modifiers::COMMAND));
    h.input_mut().events.push(egui::Event::Text("4".into()));
    h.run_steps(8);
    let saved = thinoda_io::import_file(&path).unwrap();
    assert_eq!(saved.active().topic(id).unwrap().title, "Size draft");
    assert_eq!(saved.active().topic(id).unwrap().style.font_size, Some(6.0));
    assert_eq!(style(&h, id).font_size, Some(64.0));
    assert_eq!(h.state().doc().unwrap().view.editing_topic(), Some(id));
}

#[test]
fn last_card_title_keystroke_and_new_document_in_one_frame_are_preserved() {
    for kind in [CardKind::Sticky, CardKind::Text] {
        let (app, id) = app_with_card(kind);
        let key = app.doc().unwrap().key.clone();
        let count = app.docs.len();
        let mut h = harness(app, vec2(1600.0, 700.0));
        {
            let doc = h.state_mut().doc_mut().unwrap();
            doc.view.begin_edit(doc.doc.sheet(), id, false);
            doc.view.set_editing_text("Fast navigation");
        }
        h.run_steps(5);
        h.input_mut().events.push(egui::Event::Text("!".into()));
        h.input_mut()
            .events
            .extend(raw_key_press(Key::N, Modifiers::COMMAND));
        h.run_steps(5);
        assert_eq!(h.state().docs.len(), count + 1, "{kind:?}");
        let original = h.state().docs.iter().find(|doc| doc.key == key).unwrap();
        let text = original
            .view
            .editing_text()
            .unwrap_or(&original.doc.sheet().topic(id).unwrap().title);
        assert_eq!(text, "Fast navigation!", "{kind:?}");
    }
}

#[test]
fn last_card_title_keystroke_and_next_sheet_in_one_frame_are_preserved() {
    for kind in [CardKind::Sticky, CardKind::Text] {
        let (mut app, id) = app_with_card(kind);
        {
            let doc = app.doc_mut().unwrap();
            let mut workbook = templates::instantiate("mind-map");
            doc.doc.add_sheet(workbook.sheets.remove(0)).unwrap();
            doc.doc.set_active(0);
        }
        let mut h = harness(app, vec2(1600.0, 700.0));
        {
            let doc = h.state_mut().doc_mut().unwrap();
            doc.view.begin_edit(doc.doc.sheet(), id, false);
            doc.view.set_editing_text("Fast sheet");
        }
        h.run_steps(5);
        h.input_mut().events.push(egui::Event::Text("!".into()));
        h.input_mut()
            .events
            .extend(raw_key_press(Key::PageDown, Modifiers::COMMAND));
        h.run_steps(5);
        let doc = h.state().doc().unwrap();
        assert_eq!(doc.doc.active_index(), 1, "{kind:?}");
        assert_eq!(
            doc.doc.workbook().sheets[0].topic(id).unwrap().title,
            "Fast sheet!",
            "{kind:?}"
        );
    }
}

#[test]
fn last_title_keystroke_and_formatting_click_in_one_frame_are_preserved() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("Fast title");
    }
    h.run_steps(5);
    let bold = h
        .get_by_role_and_label(Role::Button, "Bold")
        .rect()
        .center();
    h.input_mut().events.extend([
        egui::Event::Text("!".into()),
        egui::Event::PointerMoved(bold),
        egui::Event::PointerButton {
            pos: bold,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos: bold,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    ]);
    h.run_steps(4);
    assert_eq!(
        h.state().doc().unwrap().view.editing_text(),
        Some("Fast title!")
    );
    assert_eq!(style(&h, id).bold, Some(true));
}

#[test]
fn last_title_keystroke_and_outside_click_in_one_frame_are_committed() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("Outside");
    }
    h.run_steps(5);
    let outside = h.state().doc().unwrap().view.canvas_rect().left_bottom() + vec2(12.0, -12.0);
    h.input_mut().events.extend([
        egui::Event::Text("!".into()),
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
    h.run_steps(4);
    let doc = h.state().doc().unwrap();
    assert_eq!(doc.view.editing_topic(), None);
    assert_eq!(doc.doc.sheet().topic(id).unwrap().title, "Outside!");
}

#[test]
fn text_after_an_outside_click_in_one_frame_does_not_enter_the_old_title() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("Outside");
    }
    h.run_steps(5);
    let outside = h.state().doc().unwrap().view.canvas_rect().left_bottom() + vec2(12.0, -12.0);
    h.input_mut().events.extend([
        egui::Event::Text("!".into()),
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
        egui::Event::Text("?".into()),
    ]);
    run_input_hook_steps(&mut h, 7);
    let doc = h.state().doc().unwrap();
    assert_eq!(doc.doc.sheet().topic(id).unwrap().title, "Outside!");
    assert_ne!(doc.view.editing_text(), Some("Outside!?"));
}

#[test]
fn text_after_committing_the_title_in_one_frame_does_not_enter_the_old_draft() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("Before");
    }
    h.run_steps(5);
    h.input_mut().events.push(egui::Event::Text("!".into()));
    h.input_mut()
        .events
        .extend(raw_key_press(Key::Enter, Modifiers::NONE));
    h.input_mut().events.push(egui::Event::Text("?".into()));
    run_input_hook_steps(&mut h, 7);
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .topic(id)
            .unwrap()
            .title,
        "Before!"
    );
}

#[test]
fn shift_enter_in_a_card_title_inserts_a_line_break() {
    let (app, id) = app_with_card(CardKind::Text);
    let mut h = harness(app, vec2(1000.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("First");
    }
    h.run_steps(5);
    h.input_mut()
        .events
        .extend(raw_key_press(Key::Enter, Modifiers::SHIFT));
    h.input_mut()
        .events
        .push(egui::Event::Text("Second".into()));
    run_input_hook_steps(&mut h, 5);
    assert_eq!(
        h.state().doc().unwrap().view.editing_text(),
        Some("First\nSecond")
    );
}

#[test]
fn shift_enter_in_an_ordinary_topic_title_inserts_a_line_break() {
    let mut app = ThinodaApp::headless();
    let doc = app.doc_mut().unwrap();
    let id = doc.doc.sheet().root;
    doc.view.select_only(id);
    doc.view.begin_edit(doc.doc.sheet(), id, false);
    doc.view.set_editing_text("First");
    let mut h = harness(app, vec2(1000.0, 700.0));
    h.input_mut()
        .events
        .extend(raw_key_press(Key::Enter, Modifiers::SHIFT));
    h.input_mut()
        .events
        .push(egui::Event::Text("Second".into()));
    h.run_steps(3);
    assert_eq!(
        h.state().doc().unwrap().view.editing_text(),
        Some("First\nSecond")
    );
}

#[test]
fn last_title_keystroke_and_close_tab_click_in_one_frame_preserve_the_draft() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("fast-close.thinoda");
    let (mut app, id) = app_with_card(CardKind::Sticky);
    app.doc_mut().unwrap().save_to(&path, false).unwrap();
    let mut h = harness(app, vec2(1000.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
    }
    h.run_steps(5);
    let label = format!("Close Tab: {}", h.state().doc().unwrap().title());
    let close = h
        .get_by_role_and_label(Role::Button, &label)
        .rect()
        .center();
    h.input_mut().events.extend([
        egui::Event::Text("!".into()),
        egui::Event::PointerMoved(close),
        egui::Event::PointerButton {
            pos: close,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos: close,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    ]);
    h.run_steps(4);
    let doc = h.state().doc().unwrap();
    assert_eq!(doc.doc.sheet().topic(id).unwrap().title, "Card title!");
    assert!(doc.is_dirty());
    assert!(h.state().dialogs.close.is_some());
}

#[test]
fn last_title_keystroke_and_window_close_in_one_frame_preserve_the_draft() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("fast-window-close.thinoda");
    let (mut app, id) = app_with_card(CardKind::Sticky);
    app.doc_mut().unwrap().save_to(&path, false).unwrap();
    let mut h = harness(app, vec2(1000.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
    }
    h.run_steps(5);
    h.input_mut().events.push(egui::Event::Text("!".into()));
    h.input_mut()
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .events
        .push(egui::ViewportEvent::Close);
    run_input_hook_steps(&mut h, 4);
    let doc = h.state().doc().unwrap();
    assert_eq!(doc.doc.sheet().topic(id).unwrap().title, "Card title!");
    assert!(doc.is_dirty());
    assert!(h.state().dialogs.close.is_some());
}

#[test]
fn queued_shortcut_input_is_processed_before_window_close() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("shortcut-window-close.thinoda");
    let (mut app, id) = app_with_card(CardKind::Sticky);
    app.doc_mut().unwrap().save_to(&path, false).unwrap();
    let mut h = harness(app, vec2(1000.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
    }
    h.run_steps(5);
    h.input_mut().events.push(egui::Event::Text("!".into()));
    h.input_mut()
        .events
        .extend(raw_key_press(Key::F10, Modifiers::ALT));
    let [modifiers, down, up, clear] = raw_key_press(Key::Space, Modifiers::NONE);
    h.input_mut()
        .events
        .extend([modifiers, down, egui::Event::Text(" ".into()), up, clear]);
    h.input_mut()
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .events
        .push(egui::ViewportEvent::Close);
    run_input_hook_steps(&mut h, 8);
    let doc = h.state().doc().unwrap();
    assert_eq!(doc.doc.sheet().topic(id).unwrap().title, "Card title!");
    assert_eq!(doc.doc.sheet().topic(id).unwrap().style.bold, Some(true));
    assert!(doc.is_dirty());
    assert!(h.state().dialogs.close.is_some());
}

#[test]
fn outside_click_after_formatting_commits_without_stealing_focus() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("Outside click");
    }
    h.run_steps(5);
    let bold = h
        .get_by_role_and_label(Role::Button, "Bold")
        .rect()
        .center();
    h.event(egui::Event::PointerMoved(bold));
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: bold,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(1);
    }
    assert_eq!(style(&h, id).bold, Some(true));
    let outside = h.state().doc().unwrap().view.canvas_rect().left_bottom() + vec2(12.0, -12.0);
    h.event(egui::Event::PointerMoved(outside));
    h.event(egui::Event::PointerButton {
        pos: outside,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(1);
    h.event(egui::Event::PointerButton {
        pos: outside,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(2);
    assert_eq!(h.state().doc().unwrap().view.editing_topic(), None);
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .topic(id)
            .unwrap()
            .title,
        "Outside click"
    );
}

#[test]
fn formatting_preserves_the_title_selection() {
    let (app, id) = app_with_card(CardKind::Text);
    let mut h = harness(app, vec2(1000.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("prefix SELECT suffix");
    }
    h.run_steps(5);
    let editor = MapCanvas::title_editor_id();
    let mut state = egui::TextEdit::load_state(&h.ctx, editor).unwrap();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(7),
            egui::text::CCursor::new(13),
        )));
    state.store(&h.ctx, editor);
    h.run_steps(1);
    h.get_by_role_and_label(Role::Button, "Bold").click();
    h.run_steps(3);
    h.event(egui::Event::Text("X".into()));
    h.run_steps(2);
    assert_eq!(
        h.state().doc().unwrap().view.editing_text(),
        Some("prefix X suffix")
    );
    assert_eq!(style(&h, id).bold, Some(true));
}

#[test]
fn canceled_toolbar_pointer_gestures_keep_the_title_draft() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("Canceled gesture");
    }
    h.run_steps(5);
    let start = h
        .get_by_role_and_label(Role::Button, "Bold")
        .rect()
        .center();
    let end = h.state().doc().unwrap().view.canvas_rect().center();
    h.event(egui::Event::PointerMoved(start));
    h.event(egui::Event::PointerButton {
        pos: start,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(1);
    h.event(egui::Event::PointerMoved(end));
    h.run_steps(1);
    h.event(egui::Event::PointerButton {
        pos: end,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(4);
    assert_eq!(style(&h, id).bold, None);
    assert_eq!(
        h.state().doc().unwrap().view.editing_text(),
        Some("Canceled gesture")
    );
    assert!(h.get_by_label("Topic title").is_focused());
}

#[test]
fn canceled_and_multitouch_formatting_keep_the_title_draft() {
    for (contacts, cancel) in [(1, true), (2, false)] {
        let (app, id) = app_with_card(CardKind::Sticky);
        let mut h = harness(app, vec2(1000.0, 700.0));
        {
            let doc = h.state_mut().doc_mut().unwrap();
            doc.view.begin_edit(doc.doc.sheet(), id, false);
            doc.view.set_editing_text("Touch draft");
        }
        h.run_steps(5);
        let pos = h
            .get_by_role_and_label(Role::Button, "Bold")
            .rect()
            .center();
        h.event(egui::Event::PointerMoved(pos));
        h.event(egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        });
        for id in 1..=contacts {
            h.event(egui::Event::Touch {
                device_id: egui::TouchDeviceId(1),
                id: egui::TouchId(id),
                phase: egui::TouchPhase::Start,
                pos: pos + vec2((id - 1) as f32 * 40.0, 0.0),
                force: None,
            });
        }
        h.run_steps(1);
        for id in 1..=contacts {
            h.event(egui::Event::Touch {
                device_id: egui::TouchDeviceId(1),
                id: egui::TouchId(id),
                phase: if cancel {
                    egui::TouchPhase::Cancel
                } else {
                    egui::TouchPhase::End
                },
                pos: pos + vec2((id - 1) as f32 * 40.0, 0.0),
                force: None,
            });
        }
        h.event(egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(4);
        assert_eq!(style(&h, id).bold, None);
        assert_eq!(
            h.state().doc().unwrap().view.editing_text(),
            Some("Touch draft")
        );
        assert!(h.get_by_label("Topic title").is_focused());
    }
}

#[test]
fn closing_the_draft_toolbar_by_pointer_keeps_the_editor_and_selection() {
    let (app, id) = app_with_card(CardKind::Text);
    let mut h = harness(app, vec2(1000.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, true);
        doc.view.set_editing_text("Selected title");
    }
    h.run_steps(5);
    h.get_by_role_and_label(Role::Button, "Close").click();
    h.run_steps(4);
    assert!(h.query_by_role_and_label(Role::Button, "Bold").is_none());
    assert!(h.get_by_label("Topic title").is_focused());
    h.event(egui::Event::Text("Replacement".into()));
    h.run_steps(2);
    assert_eq!(
        h.state().doc().unwrap().view.editing_text(),
        Some("Replacement")
    );
    h.key_press_modifiers(Modifiers::ALT, Key::F10);
    h.run_steps(3);
    assert!(h.get_by_role_and_label(Role::Button, "Bold").is_focused());
}

#[test]
fn composition_start_on_a_toolbar_release_cannot_apply_formatting() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("IME draft ");
    }
    h.run_steps(5);
    let pos = h
        .get_by_role_and_label(Role::Button, "Bold")
        .rect()
        .center();
    h.event(egui::Event::PointerMoved(pos));
    h.event(egui::Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(1);
    h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
        text: "candidate".into(),
        active_range_chars: None,
    }));
    h.event(egui::Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(3);
    assert_eq!(style(&h, id).bold, None);
    assert_eq!(h.state().doc().unwrap().view.editing_topic(), Some(id));
    assert!(h.get_by_label("Topic title").is_focused());
}

#[test]
fn title_conflicts_do_not_allow_formatting_or_discard_the_draft() {
    let (app, id) = app_with_card(CardKind::Text);
    let mut h = harness(app, vec2(1000.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("Conflicting draft");
    }
    h.run_steps(5);
    let target = h.state().card_text_target().unwrap().0;
    h.state_mut()
        .doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Concurrent title", |sheet| {
            sheet.set_title(id, "Changed elsewhere")
        })
        .unwrap();
    assert!(
        h.state_mut()
            .apply_card_text_change(&target, CardTextChange::Bold(true))
            .is_none()
    );
    h.run_steps(3);
    assert_eq!(style(&h, id).bold, None);
    assert_eq!(
        h.state().doc().unwrap().view.editing_text(),
        Some("Conflicting draft")
    );
    assert!(!h.state().card_text_target().unwrap().1);
    h.ctx
        .memory_mut(|memory| memory.request_focus(MapCanvas::id()));
    h.run_steps(3);
    assert_eq!(
        h.state().doc().unwrap().view.editing_text(),
        Some("Conflicting draft")
    );
    assert!(h.get_by_label("Topic title").is_focused());
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(
        h.state().doc().unwrap().view.editing_text(),
        Some("Conflicting draft")
    );
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .topic(id)
            .unwrap()
            .title,
        "Changed elsewhere"
    );
}

#[test]
fn formatting_and_the_title_are_separate_undo_steps() {
    let (app, id) = app_with_card(CardKind::Text);
    let mut h = harness(app, vec2(1000.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("Undo title");
    }
    h.run_steps(5);
    h.get_by_role_and_label(Role::Button, "Bold").click();
    h.run_steps(3);
    let doc = h.state().doc().unwrap();
    assert!(doc.has_pending_drafts());
    let snapshot = doc.document_with_drafts().unwrap();
    assert_eq!(snapshot.sheet().topic(id).unwrap().title, "Undo title");
    assert_eq!(snapshot.sheet().topic(id).unwrap().style.bold, Some(true));
    assert_eq!(doc.doc.sheet().topic(id).unwrap().title, "Card title");
    h.key_press(Key::Enter);
    h.run_steps(3);
    let doc = h.state_mut().doc_mut().unwrap();
    assert!(doc.doc.undo());
    assert_eq!(doc.doc.sheet().topic(id).unwrap().title, "Card title");
    assert_eq!(doc.doc.sheet().topic(id).unwrap().style.bold, Some(true));
    assert!(doc.doc.undo());
    assert_eq!(doc.doc.sheet().topic(id).unwrap().style.bold, None);
    assert!(doc.doc.redo());
    assert!(doc.doc.redo());
    assert_eq!(doc.doc.sheet().topic(id).unwrap().title, "Undo title");
    assert_eq!(doc.doc.sheet().topic(id).unwrap().style.bold, Some(true));
}

#[test]
fn leaving_the_formatting_companion_commits_the_title_normally() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("Finished draft");
    }
    h.run_steps(5);
    h.get_by_role_and_label(Role::Button, "Bold").focus();
    h.run_steps(2);
    assert_eq!(h.state().doc().unwrap().view.editing_topic(), Some(id));
    h.ctx
        .memory_mut(|memory| memory.request_focus(MapCanvas::id()));
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().view.editing_topic(), None);
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .topic(id)
            .unwrap()
            .title,
        "Finished draft"
    );
}

#[test]
fn losing_the_window_does_not_finish_the_title_draft() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("Window focus draft");
    }
    h.run_steps(5);
    h.event(egui::Event::WindowFocused(false));
    h.run_steps(3);
    assert_eq!(
        h.state().doc().unwrap().view.editing_text(),
        Some("Window focus draft")
    );
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .topic(id)
            .unwrap()
            .title,
        "Card title"
    );
    h.event(egui::Event::WindowFocused(true));
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().view.editing_topic(), Some(id));
}

#[test]
fn unrelated_focus_does_not_inherit_an_open_font_popup() {
    let (app, id) = app_with_card(CardKind::Text);
    let mut h = harness(app, vec2(1000.0, 700.0));
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("Popup draft");
    }
    h.run_steps(5);
    scroll_and_click(&mut h, Role::ComboBox, "Font Family");
    assert!(h.query_by_label("Monospace").is_some());
    h.ctx
        .memory_mut(|memory| memory.request_focus(MapCanvas::id()));
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().view.editing_topic(), None);
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .topic(id)
            .unwrap()
            .title,
        "Popup draft"
    );
}

#[test]
fn floating_insert_toolbar_never_covers_an_active_card_title() {
    for kind in [CardKind::Sticky, CardKind::Text] {
        for size in [vec2(320.0, 240.0), vec2(640.0, 360.0), vec2(1200.0, 700.0)] {
            let (mut app, id) = app_with_card(kind);
            app.doc_mut()
                .unwrap()
                .doc
                .edit_sheet("Style", |sheet| {
                    sheet.set_title(id, "A long title\nand a second line")?;
                    sheet.update_style(&[id], |style| {
                        style.bold = Some(true);
                        style.italic = Some(true);
                        style.font_size = Some(20.0);
                    })
                })
                .unwrap();
            let mut h = harness(app, size);
            {
                let doc = h.state_mut().doc_mut().unwrap();
                doc.view.begin_edit(doc.doc.sheet(), id, true);
            }
            h.run_steps(5);
            h.get_by_role_and_label(Role::Button, "Bold").focus();
            h.run_steps(2);
            h.key_press(Key::Escape);
            h.run_steps(3);
            assert_eq!(h.state().doc().unwrap().view.editing_topic(), Some(id));
            let visible = assert_insert_toolbar_clears_title(&h);
            if size.x > 1000.0 {
                assert!(visible, "the toolbar should remain available when it fits");
                for resized in [vec2(320.0, 240.0), vec2(640.0, 360.0), size] {
                    h.set_size(resized);
                    h.run_steps(1);
                    assert_insert_toolbar_clears_title(&h);
                    assert_eq!(h.state().doc().unwrap().view.editing_topic(), Some(id));
                }
                for scale in [1.5, 2.0, 1.0] {
                    h.state_mut().settings.ui_scale = scale;
                    h.run_steps(1);
                    assert_insert_toolbar_clears_title(&h);
                    assert_eq!(h.state().doc().unwrap().view.editing_topic(), Some(id));
                }
            }
            h.get_by_label("Topic title").type_text("Edited");
            h.run_steps(2);
            assert_eq!(h.state().doc().unwrap().view.editing_text(), Some("Edited"));
            h.key_press(Key::Enter);
            h.run_steps(5);
            assert_eq!(
                h.state()
                    .doc()
                    .unwrap()
                    .doc
                    .sheet()
                    .topic(id)
                    .unwrap()
                    .title,
                "Edited"
            );
            assert!(h.ctx.memory(|memory| {
                memory
                    .areas()
                    .visible_layer_ids()
                    .iter()
                    .any(|layer| layer.id == egui::Id::unique("thinoda_editing_toolbar"))
            }));
        }
    }
}

#[test]
fn canvas_context_menu_opens_the_toolbar_even_when_topic_toolbar_is_disabled() {
    let (mut app, id) = app_with_card(CardKind::Sticky);
    app.settings.show_topic_toolbar = false;
    let mut h = harness(app, vec2(1000.0, 700.0));
    open_toolbar_from_context(&mut h);
    assert!(h.query_by_role_and_label(Role::Button, "Bold").is_some());
    h.get_by_role_and_label(Role::Button, "Bold").click();
    h.run_steps(3);
    assert_eq!(style(&h, id).bold, Some(true));
}

#[test]
fn blank_canvas_tools_submenu_remains_scrollable_in_a_short_viewport() {
    let mut app = ThinodaApp::headless();
    app.settings.ai.enabled = false;
    app.new_doc(Workbook::new(Sheet::new("Board", "Root")));
    let root = app.doc().unwrap().doc.sheet().root;
    app.doc_mut().unwrap().view.select_only(root);
    let mut h = harness(app, vec2(520.0, 320.0));
    let pos = h.state().doc().unwrap().view.canvas_rect().right_bottom() - vec2(8.0, 8.0);
    h.event(egui::Event::PointerMoved(pos));
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(1);
    }
    h.run_steps(4);
    h.get_by_role_and_label(Role::Button, "Tools")
        .scroll_to_me();
    h.run_steps(3);
    assert!(
        h.ctx
            .content_rect()
            .contains_rect(h.get_by_role_and_label(Role::Button, "Tools").rect())
    );
    h.get_by_role_and_label(Role::Button, "Tools").click();
    h.run_steps(3);
    assert_eq!(
        h.query_all_by_role_and_label(Role::Button, "Pen").count(),
        2,
        "the Tools submenu and toolbox both expose Pen"
    );
    h.query_all_by_role_and_label(Role::Button, "Pen")
        .next()
        .unwrap()
        .scroll_to_me();
    h.run_steps(3);
    assert!(
        h.ctx.content_rect().contains_rect(
            h.query_all_by_role_and_label(Role::Button, "Pen")
                .next()
                .unwrap()
                .rect()
        )
    );
    h.query_all_by_role_and_label(Role::Button, "Pen")
        .next()
        .unwrap()
        .click();
    h.run_steps(3);
    assert_eq!(
        h.state().doc().unwrap().view.board_tool,
        thinoda_canvas::BoardTool::Pen
    );
}

#[test]
fn numeric_editor_escape_does_not_send_a_canvas_shortcut() {
    let (app, id) = app_with_card(CardKind::Text);
    let mut h = harness(app, vec2(1000.0, 700.0));
    open_toolbar(&mut h);
    scroll_and_click(&mut h, Role::SpinButton, "Font Size");
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.get_by_role_and_label(Role::SpinButton, "Font Size")
        .type_text("26");
    h.run_steps(2);
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().view.selection, [id]);
    assert_eq!(h.state().doc().unwrap().view.editing_topic(), None);
    assert!(h.query_by_role_and_label(Role::Button, "Bold").is_some());
}

#[test]
fn sheet_replacement_and_undo_invalidate_the_old_toolbar_owner() {
    let (mut app, id) = app_with_card(CardKind::Sticky);
    {
        let doc = app.doc_mut().unwrap();
        doc.doc.duplicate_sheet(0).unwrap();
        doc.doc.set_active(0);
        doc.view.select_only(id);
    }
    let mut h = harness(app, vec2(1000.0, 700.0));
    open_toolbar(&mut h);
    let target = h.state().card_text_target().unwrap().0;
    let root = target.root;
    h.state_mut()
        .doc_mut()
        .unwrap()
        .doc
        .remove_sheet(0)
        .unwrap();
    assert_eq!(h.state().doc().unwrap().doc.active_index(), 0);
    assert_eq!(h.state().doc().unwrap().doc.sheet().root, root);
    assert_eq!(h.state().doc().unwrap().view.selection, [id]);
    assert!(
        h.state_mut()
            .apply_card_text_change(&target, CardTextChange::Bold(true))
            .is_none()
    );
    assert_eq!(style(&h, id).bold, None);
    h.run_steps(4);
    assert!(h.query_by_role_and_label(Role::Button, "Bold").is_none());
    open_toolbar(&mut h);
    h.get_by_role_and_label(Role::Button, "Bold").click();
    h.run_steps(3);
    assert_eq!(style(&h, id).bold, Some(true));
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    h.run_steps(3);
    assert_eq!(style(&h, id).bold, None);
    assert!(h.query_by_role_and_label(Role::Button, "Bold").is_none());
    open_toolbar(&mut h);
    let unchanged = h.state().doc().unwrap().doc.state_id();
    let align = {
        let doc = h.state().doc().unwrap();
        thinoda_core::theme::resolve_style(doc.doc.sheet(), id, doc.view.dark).align
    };
    let label = match align {
        TextAlign::Left => "Align Left",
        TextAlign::Center => "Align Center",
        TextAlign::Right => "Align Right",
    };
    h.get_by_role_and_label(Role::Button, label).click();
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.state_id(), unchanged);
    assert!(h.state().doc().unwrap().doc.can_redo());
    assert!(h.state_mut().doc_mut().unwrap().doc.redo());
    h.run_steps(3);
    assert_eq!(style(&h, id).bold, Some(true));
    assert!(h.query_by_role_and_label(Role::Button, "Bold").is_none());
    open_toolbar(&mut h);
    assert_eq!(
        h.get_by_role_and_label(Role::Button, "Bold")
            .accesskit_node()
            .toggled(),
        Some(Toggled::True)
    );
}

#[test]
fn quick_style_and_formatting_preserve_each_others_fields() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    open_toolbar(&mut h);
    let quick = TopicStyle {
        fill: Some(Color::rgb(20, 40, 60)),
        bold: Some(true),
        italic: Some(true),
        underline: Some(true),
        font_size: Some(23.0),
        align: Some(TextAlign::Right),
        text_color: Some(Color::rgb(0xE8, 0x45, 0x5C)),
        ..Default::default()
    };
    h.state_mut().apply_quick_style(quick.clone());
    h.run_steps(4);
    assert!(h.query_by_role_and_label(Role::Button, "Bold").is_none());
    open_toolbar(&mut h);
    for label in ["Bold", "Italic", "Underline", "Align Right"] {
        assert_eq!(
            h.get_by_role_and_label(Role::Button, label)
                .accesskit_node()
                .toggled(),
            Some(Toggled::True),
            "{label}"
        );
    }
    let unchanged = h.state().doc().unwrap().doc.state_id();
    h.get_by_role_and_label(Role::Button, "Align Right").click();
    h.run_steps(3);
    scroll_and_click(&mut h, Role::ColorWell, "Text Color #E8455C");
    assert_eq!(h.state().doc().unwrap().doc.state_id(), unchanged);
    assert_eq!(style(&h, id), quick);
    scroll_and_click(&mut h, Role::Button, "Bold");
    let mut expected = quick.clone();
    expected.bold = Some(false);
    assert_eq!(style(&h, id), expected);
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .topic(id)
            .unwrap()
            .card
            .unwrap()
            .color,
        quick.fill
    );
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    assert_eq!(style(&h, id), quick);
}

#[test]
fn canceled_and_multitouch_releases_do_not_format_cards() {
    for (role, label) in [
        (Role::Button, "Bold"),
        (Role::ColorWell, "Text Color #E8455C"),
    ] {
        for (contacts, cancel) in [(1, true), (2, false), (1, false)] {
            let (app, id) = app_with_card(CardKind::Sticky);
            let mut h = harness(app, vec2(1000.0, 700.0));
            open_toolbar(&mut h);
            h.get_by_role_and_label(role, label).scroll_to_me();
            h.run_steps(4);
            let pos = h.get_by_role_and_label(role, label).rect().center();
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
                "{label} contacts={contacts} cancel={cancel}"
            );
            if role == Role::Button {
                assert_eq!(style(&h, id).bold, activated.then_some(true));
            } else {
                assert_eq!(
                    style(&h, id).text_color,
                    activated.then_some(Color::rgb(0xE8, 0x45, 0x5C))
                );
            }
        }
    }
}

#[test]
fn selection_or_lock_change_between_press_and_release_rejects_the_click() {
    for lock in [false, true] {
        let (mut app, first) = app_with_card(CardKind::Sticky);
        let second = app
            .doc_mut()
            .unwrap()
            .doc
            .edit_sheet("Second card", |sheet| {
                sheet
                    .add_card(
                        "Other",
                        Vec2::new(300.0, 0.0),
                        Card::new(CardKind::Text, Vec2::new(160.0, 90.0)),
                    )
                    .map_err(thinoda_core::EditError::Invalid)
            })
            .unwrap();
        let mut h = harness(app, vec2(1000.0, 700.0));
        open_toolbar(&mut h);
        let pos = h
            .get_by_role_and_label(Role::Button, "Bold")
            .rect()
            .center();
        h.hover_at(pos);
        h.run_steps(2);
        h.event(egui::Event::PointerButton {
            pos,
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
                .edit_sheet("Lock card", |sheet| {
                    sheet
                        .set_board_members_locked(&[BoardMember::Card(first)], true)
                        .map_err(thinoda_core::EditError::Invalid)
                })
                .unwrap();
        } else {
            h.state_mut().doc_mut().unwrap().view.select_only(second);
        }
        let before_release = h.state().doc().unwrap().doc.state_id();
        h.event(egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(4);
        assert_eq!(h.state().doc().unwrap().doc.state_id(), before_release);
        assert_eq!(style(&h, first).bold, None);
        assert_eq!(style(&h, second).bold, None);
        assert!(h.query_by_role_and_label(Role::Button, "Bold").is_none());
    }
}

#[test]
fn pending_ink_invalidates_the_formatting_target() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    open_toolbar(&mut h);
    let target = h.state().card_text_target().unwrap().0;
    h.state_mut()
        .doc_mut()
        .unwrap()
        .view
        .set_board_tool(BoardTool::Pen);
    h.run_steps(2);
    let canvas = h.state().doc().unwrap().view.canvas_rect();
    let pos = canvas.left_top() + vec2(canvas.width() * 0.3, canvas.height() * 0.7);
    h.hover_at(pos);
    h.run_steps(1);
    h.event(egui::Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(2);
    assert!(h.state().doc().unwrap().view.has_pending_ink_gesture());
    assert_eq!(h.state().doc().unwrap().view.selection, [id]);
    let before = h.state().doc().unwrap().doc.state_id();
    assert!(
        h.state_mut()
            .apply_card_text_change(&target, CardTextChange::Bold(true))
            .is_none()
    );
    assert_eq!(h.state().doc().unwrap().doc.state_id(), before);
    assert_eq!(style(&h, id).bold, None);
    h.run_steps(2);
    assert!(
        h.get_by_role_and_label(Role::Button, "Bold")
            .accesskit_node()
            .is_disabled()
    );
    let end = pos + vec2(40.0, 20.0);
    h.event(egui::Event::PointerMoved(end));
    h.run_steps(2);
    assert_eq!(h.state().doc().unwrap().view.canvas_rect(), canvas);
    h.event(egui::Event::PointerButton {
        pos: end,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(4);
    assert!(matches!(
        h.state().doc().unwrap().doc.sheet().canvas.objects[0].kind,
        thinoda_core::CanvasObjectKind::Ink(_)
    ));
    assert_eq!(style(&h, id).bold, None);
}

#[test]
fn active_card_resize_cannot_receive_formatting_or_lose_its_gesture() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    open_toolbar(&mut h);
    let target = h.state().card_text_target().unwrap().0;
    let before = h.state().doc().unwrap().doc.workbook().clone();
    let handle = h.get_by_label("Resize board item").rect().center();
    let end = handle + vec2(45.0, 30.0);
    h.hover_at(handle);
    h.run_steps(1);
    h.event(egui::Event::PointerButton {
        pos: handle,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(1);
    h.event(egui::Event::PointerMoved(end));
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    assert!(
        h.state_mut()
            .apply_card_text_change(&target, CardTextChange::Bold(true))
            .is_none()
    );
    h.event(egui::Event::PointerButton {
        pos: end,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(4);
    let doc = h.state().doc().unwrap();
    assert_ne!(doc.doc.workbook(), &before);
    assert_eq!(doc.doc.sheet().topic(id).unwrap().style.bold, None);
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
}

#[test]
fn escape_cancels_card_resize_before_formatting_handles_it() {
    let (app, _) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    open_toolbar(&mut h);
    let before = h.state().doc().unwrap().doc.workbook().clone();
    let handle = h.get_by_label("Resize board item").rect().center();
    let end = handle + vec2(45.0, 30.0);
    h.hover_at(handle);
    h.run_steps(1);
    h.event(egui::Event::PointerButton {
        pos: handle,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(1);
    h.event(egui::Event::PointerMoved(end));
    h.run_steps(3);
    assert!(h.state().doc().unwrap().view.has_active_canvas_gesture());
    h.key_press(Key::Escape);
    h.run_steps(2);
    h.event(egui::Event::PointerButton {
        pos: end,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(4);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
}

#[test]
fn pending_inspector_draft_is_preserved_and_blocks_formatting() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    open_toolbar(&mut h);
    let target = h.state().card_text_target().unwrap().0;
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.inspector_edit = Some(crate::doc::InspectorEdit {
            buffer_id: None,
            sheet_index: doc.doc.active_index(),
            sheet_root: doc.doc.sheet().root,
            topic: id,
            field: crate::doc::InspectorField::Notes,
            original: None,
            format_notes: false,
            text: "Pending note draft".into(),
        });
    }
    let before = h.state().doc().unwrap().doc.state_id();
    assert!(
        h.state_mut()
            .apply_card_text_change(&target, CardTextChange::Bold(true))
            .is_none()
    );
    h.run_steps(3);
    let doc = h.state().doc().unwrap();
    assert_eq!(doc.doc.state_id(), before);
    assert_eq!(doc.doc.sheet().topic(id).unwrap().style.bold, None);
    assert_eq!(
        doc.inspector_edit.as_ref().unwrap().text,
        "Pending note draft"
    );
    assert!(
        h.get_by_role_and_label(Role::Button, "Bold")
            .accesskit_node()
            .is_disabled()
    );
}

#[test]
fn multiple_selection_and_image_cards_do_not_offer_text_formatting() {
    let (app, _) = app_with_card(CardKind::Image);
    assert!(app.card_text_target().is_none());
    let h = harness(app, vec2(1000.0, 700.0));
    assert!(
        h.query_by_role_and_label(Role::Button, card_text_label())
            .is_none()
    );

    let (mut app, first) = app_with_card(CardKind::Sticky);
    let doc = app.doc_mut().unwrap();
    let second = doc
        .doc
        .edit_sheet("Second card", |sheet| {
            sheet
                .add_card(
                    "Other",
                    Vec2::new(300.0, 0.0),
                    Card::new(CardKind::Text, Vec2::new(160.0, 90.0)),
                )
                .map_err(thinoda_core::EditError::Invalid)
        })
        .unwrap();
    doc.view.selection = vec![first, second];
    assert!(app.card_text_target().is_none());
    let h = harness(app, vec2(1000.0, 700.0));
    assert!(
        h.query_by_role_and_label(Role::Button, card_text_label())
            .is_none()
    );
}

#[test]
fn deleted_selected_card_invalidates_the_toolbar() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    open_toolbar(&mut h);
    let target = h.state().card_text_target().unwrap().0;
    h.state_mut()
        .doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Delete card", |sheet| sheet.remove_topics(&[id]))
        .unwrap();
    let before = h.state().doc().unwrap().doc.state_id();
    assert!(
        h.state_mut()
            .apply_card_text_change(&target, CardTextChange::Bold(true))
            .is_none()
    );
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.state_id(), before);
    assert!(h.state().doc().unwrap().doc.sheet().topic(id).is_none());
    assert!(h.query_by_role_and_label(Role::Button, "Bold").is_none());
}

#[test]
fn logical_viewports_keep_independent_toolbar_owners() {
    let (mut app, root_card) = app_with_card(CardKind::Sticky);
    let root_key = app.doc().unwrap().key.clone();
    app.new_doc(templates::instantiate("blank-canvas"));
    let child_card = {
        let doc = app.doc_mut().unwrap();
        let id = doc
            .doc
            .edit_sheet("Create card", |sheet| {
                sheet
                    .add_card(
                        "Other window",
                        Vec2::ZERO,
                        Card::new(CardKind::Text, Vec2::new(180.0, 120.0)),
                    )
                    .map_err(thinoda_core::EditError::Invalid)
            })
            .unwrap();
        doc.view.select_only(id);
        id
    };
    let child_key = app.doc().unwrap().key.clone();
    let child_viewport = egui::ViewportId::from_hash_of("card-text-detached");
    app.document_windows
        .insert(child_key.clone(), child_viewport);
    app.window_active_documents
        .insert(child_viewport, child_key.clone());
    app.window_active_documents
        .insert(egui::ViewportId::ROOT, root_key.clone());
    let ctx = Context::default();
    app.activate_document_window(egui::ViewportId::ROOT);
    let root_target = app.card_text_target().unwrap().0;
    ThinodaApp::open_card_text_toolbar(&ctx, egui::ViewportId::ROOT, root_target.clone());
    app.activate_document_window(child_viewport);
    let child_target = app.card_text_target().unwrap().0;
    ThinodaApp::open_card_text_toolbar(&ctx, child_viewport, child_target.clone());
    for _ in 0..4 {
        for viewport in [egui::ViewportId::ROOT, child_viewport] {
            app.current_viewport = viewport;
            app.activate_document_window(viewport);
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        vec2(1000.0, 700.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    app.card_text_toolbar(ui, false);
                },
            );
            output.textures_delta.clear();
        }
    }
    assert_eq!(
        ctx.data(|data| data.get_temp::<CardTextTarget>(card_text_slot(egui::ViewportId::ROOT))),
        Some(root_target.clone())
    );
    assert_eq!(
        ctx.data(|data| data.get_temp::<CardTextTarget>(card_text_slot(child_viewport))),
        Some(child_target.clone())
    );
    app.activate_document_window(child_viewport);
    assert!(
        app.apply_card_text_change(&root_target, CardTextChange::Bold(true))
            .is_none()
    );
    assert!(
        app.apply_card_text_change(&child_target, CardTextChange::Bold(true))
            .is_some()
    );
    assert_eq!(
        app.doc()
            .unwrap()
            .doc
            .sheet()
            .topic(child_card)
            .unwrap()
            .style
            .bold,
        Some(true)
    );
    let root_doc = app.docs.iter().find(|doc| doc.key == root_key).unwrap();
    assert_eq!(
        root_doc.doc.sheet().topic(root_card).unwrap().style.bold,
        None
    );
}

#[cfg(feature = "gpu-tests")]
#[test]
fn card_text_visual_previews() {
    let Some(directory) = std::env::var_os("THINODA_CARD_TEXT_VISUAL_DIR") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    for dark in [false, true] {
        for (size, width) in [
            (vec2(1200.0, 700.0), "wide"),
            (vec2(320.0, 240.0), "compact"),
        ] {
            let (mut app, _) = app_with_card(CardKind::Sticky);
            app.settings.theme = if dark {
                SettingsTheme::Dark
            } else {
                SettingsTheme::Light
            };
            let mut h = Harness::builder()
                .wgpu()
                .with_size(size)
                .with_step_dt(1.0 / 60.0)
                .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
            h.run_steps(6);
            if h.query_by_role_and_label(Role::Button, card_text_label())
                .is_some()
            {
                open_toolbar(&mut h);
            } else {
                open_toolbar_from_context(&mut h);
            }
            let theme = if dark { "dark" } else { "light" };
            h.render()
                .expect("formatting toolbar render")
                .save(directory.join(format!("card-text-{theme}-{width}.png")))
                .unwrap();
            h.get_by_role_and_label(Role::ColorWell, "Text Color #FFFFFF")
                .scroll_to_me();
            h.run_steps(5);
            h.render()
                .expect("formatting palette render")
                .save(directory.join(format!("card-color-{theme}-{width}.png")))
                .unwrap();
        }
    }
}

#[cfg(feature = "gpu-tests")]
#[test]
fn card_title_editor_visual_previews() {
    let Some(directory) = std::env::var_os("THINODA_CARD_TEXT_VISUAL_DIR") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    for (kind, kind_name) in [(CardKind::Sticky, "sticky"), (CardKind::Text, "text")] {
        for dark in [false, true] {
            for (size, width) in [
                (vec2(1200.0, 700.0), "wide"),
                (vec2(320.0, 240.0), "compact"),
            ] {
                let (mut app, id) = app_with_card(kind);
                app.settings.theme = if dark {
                    SettingsTheme::Dark
                } else {
                    SettingsTheme::Light
                };
                app.doc_mut()
                    .unwrap()
                    .doc
                    .edit_sheet("Style preview", |sheet| {
                        sheet.set_title(id, "Ideas to explore\nA styled draft  ")?;
                        sheet.update_style(&[id], |style| {
                            style.bold = Some(true);
                            style.italic = Some(true);
                            style.underline = Some(true);
                            style.strikethrough = Some(true);
                            style.font_family = Some("monospace".into());
                            style.font_size = Some(20.0);
                            style.text_color = Some(Color::rgb(120, 45, 70));
                            style.align = Some(TextAlign::Left);
                        })
                    })
                    .unwrap();
                let mut h = Harness::builder()
                    .wgpu()
                    .with_size(size)
                    .with_step_dt(1.0 / 60.0)
                    .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
                h.run_steps(6);
                {
                    let doc = h.state_mut().doc_mut().unwrap();
                    doc.view.begin_edit(doc.doc.sheet(), id, true);
                }
                h.run_steps(4);
                assert_eq!(h.state().doc().unwrap().view.editing_topic(), Some(id));
                let theme = if dark { "dark" } else { "light" };
                h.render()
                    .expect("selected card editor render")
                    .save(directory.join(format!(
                        "card-editor-selected-{kind_name}-{theme}-{width}.png"
                    )))
                    .unwrap();
                h.key_press(Key::ArrowRight);
                h.run_steps(3);
                h.render()
                    .expect("card editor render")
                    .save(directory.join(format!("card-editor-{kind_name}-{theme}-{width}.png")))
                    .unwrap();
            }
        }
    }
}
