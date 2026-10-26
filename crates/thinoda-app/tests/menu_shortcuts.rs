use egui::{Event, Key, PointerButton, os::OperatingSystem};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use thinoda_app::app::{CenterMode, Cmd, FocusField, ThinodaApp};
use thinoda_canvas::Action;

mod support;
use support::menu_action;

fn paint_hint(h: &Harness<'_, ThinodaApp>, hint: &str, rect: egui::Rect) {
    fn find(shape: &egui::Shape, hint: &str, rect: egui::Rect) -> Option<egui::Rect> {
        match shape {
            egui::Shape::Text(text) if text.galley.text() == hint && rect.contains(text.pos) => {
                Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| find(shape, hint, rect)),
            _ => None,
        }
    }
    let (shape, bounds) = h
        .output()
        .shapes
        .iter()
        .find_map(|shape| find(&shape.shape, hint, rect).map(|bounds| (shape, bounds)))
        .expect("command hint must actually be painted inside its button");
    assert!(shape.clip_rect.expand(1.0).contains_rect(bounds));
    assert!(h.ctx.content_rect().expand(1.0).contains_rect(bounds));
}

#[test]
fn menus_and_context_hints_match_catalog_bindings_on_each_platform() {
    for os in [
        OperatingSystem::Windows,
        OperatingSystem::Mac,
        OperatingSystem::Nix,
    ] {
        for size in [
            egui::vec2(640.0, 360.0),
            egui::vec2(640.0, 240.0),
            egui::vec2(1200.0, 800.0),
        ] {
            let mut app = ThinodaApp::headless();
            app.settings.show_format_panel = false;
            let root = app.doc().unwrap().doc.sheet().root;
            let child = app.doc().unwrap().doc.sheet().children(root)[0];
            app.doc_mut().unwrap().view.select_only(child);
            let mut h = Harness::builder().with_size(size).build_ui_state(
                move |ui, app: &mut ThinodaApp| {
                    ui.ctx().set_os(os);
                    app.draw(ui);
                },
                app,
            );
            h.run_steps(4);
            let before = h.state().doc().unwrap().doc.workbook().clone();
            for (category, commands) in [
                (
                    "File",
                    vec![
                        ("New", Cmd::New),
                        ("Save", Cmd::Save),
                        ("Save As…", Cmd::SaveAs),
                        ("Print", Cmd::Print),
                        ("Quit", Cmd::Quit),
                    ],
                ),
                (
                    "Edit",
                    vec![
                        ("Undo", Cmd::Canvas(Action::Undo)),
                        ("Redo", Cmd::Canvas(Action::Redo)),
                        ("Cut", Cmd::Canvas(Action::Cut)),
                        ("Copy", Cmd::Canvas(Action::Copy)),
                        ("Paste", Cmd::MenuPaste),
                        ("Settings…", Cmd::Settings),
                    ],
                ),
                (
                    "Insert",
                    vec![
                        ("Topic", Cmd::Canvas(Action::InsertSibling)),
                        ("Boundary", Cmd::Canvas(Action::AddBoundary)),
                        ("Summary", Cmd::Canvas(Action::AddSummary)),
                        ("Notes", Cmd::EditField(FocusField::Notes)),
                        ("Hyperlink", Cmd::EditField(FocusField::Hyperlink)),
                    ],
                ),
                (
                    "View",
                    vec![
                        ("Previous Sheet", Cmd::PreviousSheet),
                        ("Zoom In", Cmd::Canvas(Action::ZoomIn)),
                        ("Actual Size", Cmd::Canvas(Action::ZoomReset)),
                        ("Fit Map", Cmd::Canvas(Action::Fit)),
                    ],
                ),
                ("Help", vec![("Keyboard Shortcuts", Cmd::Shortcuts)]),
                ("Tools", vec![("Map Shot", Cmd::MapShot)]),
                (
                    "AI",
                    vec![(
                        thinoda_app::ai::AiCommand::Brainstorm.label(),
                        Cmd::Ai(thinoda_app::ai::AiCommand::Brainstorm),
                    )],
                ),
            ] {
                h.get_by_label(thinoda_app::i18n::t("Main Menu")).click();
                h.run_steps(3);
                menu_action(&h, category).scroll_to_me();
                h.run_steps(5);
                menu_action(&h, category).click();
                h.run_steps(3);
                for (label, command) in commands {
                    let hint = ThinodaApp::shortcut_hint(&h.ctx, &command);
                    let full_label = hint
                        .as_ref()
                        .map_or_else(|| label.to_string(), |hint| format!("{label} {hint}"));
                    menu_action(&h, &full_label).scroll_to_me();
                    h.run_steps(6);
                    let button = menu_action(&h, &full_label);
                    assert_eq!(
                        button.accesskit_node().is_disabled(),
                        !h.state().command_available(&command)
                    );
                    if let Some(hint) = &hint {
                        paint_hint(&h, hint, button.rect());
                    }
                    let catalog = h.state().command_catalog(&h.ctx);
                    if let Some((_, metadata)) = catalog.iter().find(|(c, _)| c == &command) {
                        assert_eq!(metadata.shortcut, hint);
                    } else {
                        let reference = h.state().shortcut_reference(&h.ctx);
                        assert_eq!(
                            reference
                                .iter()
                                .find(|entry| entry.title == label)
                                .unwrap()
                                .shortcut
                                .as_deref(),
                            hint.as_deref()
                        );
                    }
                }
                h.key_press(Key::Escape);
                h.run_steps(3);
            }
            for (submenu, label, command) in [
                ("Select", "Central Topic", Cmd::Canvas(Action::SelectRoot)),
                ("Arrange", "Move Down", Cmd::Canvas(Action::MoveDown)),
            ] {
                h.get_by_label(thinoda_app::i18n::t("Main Menu")).click();
                h.run_steps(3);
                menu_action(&h, "Edit").click();
                h.run_steps(3);
                menu_action(&h, submenu).scroll_to_me();
                h.run_steps(5);
                menu_action(&h, submenu).click();
                h.run_steps(3);
                let hint = ThinodaApp::shortcut_hint(&h.ctx, &command).unwrap();
                let label = format!("{label} {hint}");
                menu_action(&h, &label).scroll_to_me();
                h.run_steps(6);
                paint_hint(&h, &hint, menu_action(&h, &label).rect());
                h.key_press(Key::Escape);
                h.run_steps(3);
            }
            let pos = {
                let doc = h.state().doc().unwrap();
                doc.view.selected_topic_rect(&doc.doc).unwrap().center()
            };
            h.event(Event::PointerMoved(pos));
            for pressed in [true, false] {
                h.event(Event::PointerButton {
                    pos,
                    button: PointerButton::Secondary,
                    pressed,
                    modifiers: Default::default(),
                });
                h.run_steps(1);
            }
            h.run_steps(4);
            for (label, command) in [
                ("Boundary", Cmd::Canvas(Action::AddBoundary)),
                ("Copy", Cmd::Canvas(Action::Copy)),
                ("Paste", Cmd::MenuPaste),
                ("Delete Topic Only", Cmd::Canvas(Action::DeleteKeepChildren)),
                ("Edit notes", Cmd::EditField(FocusField::Notes)),
                ("Fit Map", Cmd::Canvas(Action::Fit)),
            ] {
                menu_action(&h, label).scroll_to_me();
                h.run_steps(6);
                if let Some(hint) = ThinodaApp::shortcut_hint(&h.ctx, &command) {
                    paint_hint(&h, &hint, menu_action(&h, label).rect());
                } else {
                    assert_eq!(command, Cmd::Canvas(Action::Fit));
                    assert!(!menu_action(&h, label).accesskit_node().is_disabled());
                }
            }
            h.key_press(Key::Escape);
            h.run_steps(3);
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
            assert_eq!(h.state().doc().unwrap().view.selection, vec![child]);
        }
    }
}

#[test]
fn outline_menus_use_outline_redo_and_leave_map_only_shortcuts_unassigned() {
    for os in [
        OperatingSystem::Windows,
        OperatingSystem::Mac,
        OperatingSystem::Nix,
    ] {
        let mut app = ThinodaApp::headless();
        app.mode = CenterMode::Outliner;
        app.settings.show_format_panel = false;
        let mut h = Harness::builder()
            .with_size(egui::vec2(640.0, 360.0))
            .build_ui_state(
                move |ui, app: &mut ThinodaApp| {
                    ui.ctx().set_os(os);
                    app.draw(ui);
                },
                app,
            );
        h.run_steps(4);
        let before = h.state().doc().unwrap().doc.workbook().clone();
        h.get_by_label(thinoda_app::i18n::t("Main Menu")).click();
        h.run_steps(3);
        menu_action(&h, "Edit").click();
        h.run_steps(3);
        let command = Cmd::Canvas(Action::Redo);
        let hint =
            ThinodaApp::shortcut_hint_for_mode(&h.ctx, &command, CenterMode::Outliner).unwrap();
        assert_eq!(
            hint,
            h.ctx.format_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::COMMAND,
                Key::Y
            ))
        );
        let label = format!("Redo {hint}");
        menu_action(&h, &label).scroll_to_me();
        h.run_steps(5);
        paint_hint(&h, &hint, menu_action(&h, &label).rect());
        menu_action(&h, "Cut");
        menu_action(&h, "Copy");
        menu_action(&h, "Paste");
        h.key_press(Key::Escape);
        h.run_steps(3);
        h.get_by_label(thinoda_app::i18n::t("Main Menu")).click();
        h.run_steps(3);
        menu_action(&h, "View").scroll_to_me();
        h.run_steps(5);
        menu_action(&h, "View").click();
        h.run_steps(3);
        menu_action(&h, "Zoom In");
        menu_action(&h, "Fit Map");
        h.key_press(Key::Escape);
        h.run_steps(3);
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    }
}

#[test]
fn clipboard_reference_metadata_keeps_event_driven_copy_cut_paste_and_undo() {
    let mut app = ThinodaApp::headless();
    let ctx = egui::Context::default();
    let root = app.doc().unwrap().doc.sheet().root;
    let child = app.doc().unwrap().doc.sheet().children(root)[0];
    app.doc_mut().unwrap().view.select_only(child);
    let before = app.doc().unwrap().doc.workbook().clone();
    let mut output = ctx.run_ui(Default::default(), |_| {});
    output.textures_delta.clear();
    for (title, command, key) in [
        ("Cut", Cmd::Canvas(Action::Cut), Key::X),
        ("Copy", Cmd::Canvas(Action::Copy), Key::C),
        ("Paste", Cmd::MenuPaste, Key::V),
    ] {
        let reference = app.shortcut_reference(&ctx);
        assert_eq!(
            reference
                .iter()
                .find(|entry| entry.title == title)
                .unwrap()
                .shortcut,
            Some(ctx.format_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, key)))
        );
        assert!(
            !app.command_catalog(&ctx).iter().any(|(c, _)| c == &command),
            "Event-only clipboard metadata must not add palette actions"
        );
    }
    app.exec(&ctx, Cmd::Canvas(Action::Copy));
    let copied = ctx
        .output(|output| {
            output.commands.iter().find_map(|command| match command {
                egui::OutputCommand::CopyText(text) => Some(text.clone()),
                _ => None,
            })
        })
        .unwrap();
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    app.exec(&ctx, Cmd::Canvas(Action::Cut));
    assert!(!app.doc().unwrap().doc.sheet().contains(child));
    app.exec(&ctx, Cmd::Canvas(Action::Undo));
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    app.doc_mut().unwrap().view.select_only(root);
    let mut output = ctx.run_ui(Default::default(), |ui| app.exec(ui.ctx(), Cmd::MenuPaste));
    assert!(
        output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::RequestPaste)
    );
    output.textures_delta.clear();
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    let input = egui::RawInput {
        events: vec![Event::Paste(copied)],
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| app.draw(ui));
    output.textures_delta.clear();
    assert_ne!(app.doc().unwrap().doc.workbook(), &before);
    app.exec(&ctx, Cmd::Canvas(Action::Undo));
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
}
