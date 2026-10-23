use egui::{Key, Modifiers, accesskit::Role};
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::app::{CenterMode, CloseIntent, Cmd, ThinodaApp};

fn harness(mode: CenterMode, size: egui::Vec2) -> Harness<'static, ThinodaApp> {
    let mut app = ThinodaApp::headless();
    app.mode = mode;
    let mut h = Harness::builder()
        .with_size(size)
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::Shortcuts);
    h.run_steps(5);
    h
}

#[test]
fn global_dispatch_catalog_is_complete_unique_and_platform_aware() {
    use egui::os::OperatingSystem;
    for os in [
        OperatingSystem::Windows,
        OperatingSystem::Mac,
        OperatingSystem::Nix,
    ] {
        let mut app = ThinodaApp::headless();
        app.new_doc(thinoda_core::Workbook::default());
        let ctx = egui::Context::default();
        ctx.set_os(os);
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
        output.textures_delta.clear();
        let bindings = ThinodaApp::global_shortcut_bindings(app.active);
        let catalog = app.command_catalog(&ctx);
        assert_eq!(bindings.len(), 24);
        for (index, (shortcut, command)) in bindings.iter().enumerate() {
            assert!(
                !bindings[..index].iter().any(|(other, _)| other == shortcut),
                "Duplicate binding: {shortcut:?}"
            );
            let entries: Vec<_> = catalog.iter().filter(|(c, _)| c == command).collect();
            assert_eq!(entries.len(), 1, "Missing/duplicate command: {command:?}");
            assert_eq!(entries[0].1.shortcut, Some(ctx.format_shortcut(shortcut)));
        }
        assert!(catalog.iter().any(|(c, _)| *c == Cmd::CloseTab(1)));
        assert!(!catalog.iter().any(|(c, _)| *c == Cmd::CloseTab(0)));
        let undo = catalog
            .iter()
            .find(|(c, _)| *c == Cmd::Canvas(thinoda_canvas::Action::Undo))
            .unwrap()
            .1
            .shortcut
            .as_deref()
            .unwrap();
        assert_eq!(
            undo,
            if os == OperatingSystem::Mac {
                "Cmd+Z"
            } else {
                "Ctrl+Z"
            }
        );
        let notes = bindings
            .iter()
            .position(|(_, c)| *c == Cmd::EditField(thinoda_app::app::FocusField::Notes))
            .unwrap();
        let new = bindings.iter().position(|(_, c)| *c == Cmd::New).unwrap();
        let save_as = bindings
            .iter()
            .position(|(_, c)| *c == Cmd::SaveAs)
            .unwrap();
        let save = bindings.iter().position(|(_, c)| *c == Cmd::Save).unwrap();
        assert!(
            notes < new && save_as < save,
            "Specific modifiers must dispatch first"
        );
    }
}

#[test]
fn contextual_dispatch_inventory_and_reference_include_every_alias_once() {
    use egui::os::OperatingSystem;
    use thinoda_canvas::Action;
    for os in [
        OperatingSystem::Windows,
        OperatingSystem::Mac,
        OperatingSystem::Nix,
    ] {
        let app = ThinodaApp::headless();
        let ctx = egui::Context::default();
        ctx.set_os(os);
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
        output.textures_delta.clear();
        let bindings = ThinodaApp::contextual_shortcut_bindings();
        let reference = app.shortcut_reference(&ctx);
        let catalog = app.command_catalog(&ctx);
        let static_catalog = ThinodaApp::palette_commands();
        for (index, (shortcut, action)) in bindings.iter().enumerate() {
            assert!(!bindings[..index].iter().any(|(other, _)| other == shortcut));
            let hint = ctx.format_shortcut(shortcut);
            if ThinodaApp::default_shortcut_command(CenterMode::Map, os, *shortcut)
                .is_some_and(|command| command != Cmd::Canvas(action.clone()))
            {
                assert!(!reference.iter().any(|entry| {
                    entry
                        .shortcut
                        .as_deref()
                        .is_some_and(|value| value.split(" / ").any(|alias| alias == hint))
                }));
                continue;
            }
            assert!(
                reference.iter().any(|entry| entry
                    .shortcut
                    .as_deref()
                    .is_some_and(|value| value.split(" / ").any(|alias| alias == hint))),
                "Missing reference binding: {shortcut:?} -> {action:?}"
            );
            if let Some((_, metadata)) = catalog
                .iter()
                .find(|(c, _)| *c == Cmd::Canvas(action.clone()))
            {
                let entry = reference
                    .iter()
                    .find(|entry| entry.id == metadata.id)
                    .unwrap();
                assert!(
                    entry
                        .shortcut
                        .as_deref()
                        .unwrap()
                        .split(" / ")
                        .any(|alias| alias == hint)
                );
                let first = bindings.iter().find(|(_, a)| a == action).unwrap();
                assert_eq!(metadata.shortcut, Some(ctx.format_shortcut(&first.0)));
                let static_entry = static_catalog
                    .iter()
                    .find(|(_, c)| c.id == metadata.id)
                    .unwrap();
                assert_eq!(
                    static_entry.1.shortcut,
                    Some(first.0.format(&egui::ModifierNames::NAMES, false))
                );
            }
        }
        for (index, entry) in reference.iter().enumerate() {
            assert!(!reference[..index].iter().any(|other| other.id == entry.id));
        }
        for (action, aliases) in [
            (Action::Redo, 2),
            (Action::InsertChild, 2),
            (Action::Delete, 2),
            (Action::DeleteKeepChildren, 3),
            (Action::SelectRoot, 2),
            (Action::ToggleCollapseAll, 2),
            (Action::ZoomIn, 2),
            (Action::AddBoundary, 2),
        ] {
            assert_eq!(
                bindings.iter().filter(|(_, a)| *a == action).count(),
                aliases
            );
        }
    }
}

#[test]
fn intercepted_fit_binding_is_unassigned_but_command_remains_available() {
    use egui::os::OperatingSystem;
    use thinoda_canvas::Action;
    for os in [
        OperatingSystem::Windows,
        OperatingSystem::Mac,
        OperatingSystem::Nix,
    ] {
        let app = ThinodaApp::headless();
        let ctx = egui::Context::default();
        ctx.set_os(os);
        let mut output = ctx.run_ui(Default::default(), |_| {});
        output.textures_delta.clear();
        let command = Cmd::Canvas(Action::Fit);
        assert!(app.command_available(&command));
        assert_eq!(
            ThinodaApp::shortcut_binding_for_platform(&command, CenterMode::Map, os),
            None
        );
        assert_eq!(ThinodaApp::shortcut_hint(&ctx, &command), None);
        let entry = app
            .command_catalog(&ctx)
            .into_iter()
            .find(|(cmd, _)| cmd == &command)
            .unwrap()
            .1;
        let reference = app.shortcut_reference(&ctx);
        assert!(entry.shortcut.is_none());
        assert!(
            reference
                .iter()
                .find(|item| item.id == entry.id)
                .unwrap()
                .shortcut
                .is_none()
        );
        let static_entry = ThinodaApp::palette_commands()
            .into_iter()
            .find(|(cmd, _)| cmd == &command)
            .unwrap()
            .1;
        assert_eq!(static_entry.id, entry.id);
        assert!(static_entry.shortcut.is_none());
        assert_eq!(
            ThinodaApp::default_shortcut_conflicts(CenterMode::Map, os).len(),
            1
        );
    }
    let mut h = harness(CenterMode::Map, egui::vec2(640.0, 360.0));
    h.state_mut().dialogs.shortcut_browser.unassigned_only = true;
    h.state_mut().dialogs.shortcut_browser.query = "Fit Map".into();
    h.run_steps(4);
    h.get_by_role_and_label(Role::Label, "Fit Map");
    h.get_by_role_and_label(Role::Label, "Unassigned");
}

#[test]
fn outline_inventory_uses_only_its_declared_bindings_and_keeps_catalog_ids() {
    use egui::{KeyboardShortcut, os::OperatingSystem};
    use thinoda_canvas::Action;
    let expected: Vec<_> = [
        (Modifiers::COMMAND, Key::Y, Action::Redo),
        (Modifiers::COMMAND | Modifiers::SHIFT, Key::Z, Action::Redo),
        (Modifiers::COMMAND, Key::Z, Action::Undo),
        (Modifiers::NONE, Key::Enter, Action::InsertSibling),
        (Modifiers::NONE, Key::Tab, Action::InsertChild),
        (Modifiers::NONE, Key::Delete, Action::Delete),
        (Modifiers::NONE, Key::F2, Action::StartEdit),
        (Modifiers::ALT, Key::ArrowUp, Action::MoveUp),
        (Modifiers::ALT, Key::ArrowDown, Action::MoveDown),
    ]
    .into_iter()
    .map(|(modifiers, key, action)| (KeyboardShortcut::new(modifiers, key), action))
    .collect();
    assert_eq!(ThinodaApp::outline_shortcut_bindings(), expected);
    for os in [
        OperatingSystem::Windows,
        OperatingSystem::Mac,
        OperatingSystem::Nix,
    ] {
        let mut app = ThinodaApp::headless();
        let ctx = egui::Context::default();
        ctx.set_os(os);
        let mut output = ctx.run_ui(Default::default(), |_| {});
        output.textures_delta.clear();
        let map_catalog = app.command_catalog(&ctx);
        app.mode = CenterMode::Outliner;
        let catalog = app.command_catalog(&ctx);
        let reference = app.shortcut_reference(&ctx);
        assert_eq!(
            map_catalog
                .iter()
                .map(|(c, m)| (c, &m.id))
                .collect::<Vec<_>>(),
            catalog.iter().map(|(c, m)| (c, &m.id)).collect::<Vec<_>>()
        );
        let bindings = ThinodaApp::contextual_shortcut_bindings_for_mode(app.mode);
        for (index, (shortcut, action)) in bindings.iter().enumerate() {
            assert!(!bindings[..index].iter().any(|(other, _)| other == shortcut));
            let hint = ctx.format_shortcut(shortcut);
            assert!(
                reference.iter().any(|entry| entry
                    .shortcut
                    .as_deref()
                    .is_some_and(|aliases| aliases.split(" / ").any(|alias| alias == hint))),
                "Missing Outline binding: {shortcut:?} -> {action:?}"
            );
        }
        for (index, entry) in reference.iter().enumerate() {
            assert!(!reference[..index].iter().any(|other| other.id == entry.id));
        }
        for action in [
            Action::DeleteKeepChildren,
            Action::SelectRoot,
            Action::SelectAll,
            Action::ZoomIn,
            Action::ZoomOut,
            Action::Fit,
            Action::ToggleCollapse,
        ] {
            let command = Cmd::Canvas(action);
            assert!(ThinodaApp::shortcut_binding_for_mode(&command, app.mode).is_none());
            assert!(
                catalog
                    .iter()
                    .filter(|(c, _)| c == &command)
                    .all(|(_, m)| m.shortcut.is_none())
            );
        }
        for id in ["map-cancel", "map-cut", "map-copy", "map-paste"] {
            assert!(!reference.iter().any(|entry| entry.id == id));
        }
        for (action, key) in [
            (Action::Redo, Key::Y),
            (Action::InsertChild, Key::Tab),
            (Action::Delete, Key::Delete),
        ] {
            let shortcut = KeyboardShortcut::new(
                if action == Action::Redo {
                    Modifiers::COMMAND
                } else {
                    Modifiers::NONE
                },
                key,
            );
            let command = Cmd::Canvas(action);
            let entry = catalog.iter().find(|(c, _)| c == &command).unwrap();
            assert_eq!(entry.1.shortcut, Some(ctx.format_shortcut(&shortcut)));
            let reference_hint = if command == Cmd::Canvas(Action::Redo) {
                format!(
                    "{} / {}",
                    ctx.format_shortcut(&shortcut),
                    ctx.format_shortcut(&KeyboardShortcut::new(
                        Modifiers::COMMAND | Modifiers::SHIFT,
                        Key::Z
                    ))
                )
            } else {
                ctx.format_shortcut(&shortcut)
            };
            assert_eq!(
                reference
                    .iter()
                    .find(|m| m.id == entry.1.id)
                    .unwrap()
                    .shortcut,
                Some(reference_hint)
            );
        }
        let navigation = reference.iter().find(|m| m.id == "map-navigate").unwrap();
        assert_eq!(
            navigation.shortcut,
            Some(
                ThinodaApp::outline_navigation_bindings()
                    .iter()
                    .map(|shortcut| ctx.format_shortcut(shortcut))
                    .collect::<Vec<_>>()
                    .join(" / ")
            )
        );
        assert_eq!(
            ThinodaApp::shortcut_binding(&Cmd::Canvas(Action::Redo)),
            ThinodaApp::shortcut_binding_for_mode(&Cmd::Canvas(Action::Redo), CenterMode::Map)
        );
    }
}

#[test]
fn outline_reference_shows_scope_and_does_not_advertise_map_aliases() {
    let mut h = harness(CenterMode::Outliner, egui::vec2(640.0, 360.0));
    h.get_by_role_and_label(Role::Label, "View: Outliner");
    let before = h.state().doc().unwrap().doc.workbook().clone();
    for query in ["Backspace", "Insert"] {
        h.state_mut().dialogs.shortcut_browser.category = Some("Edit".into());
        h.state_mut().dialogs.shortcut_browser.query = query.into();
        h.run_steps(4);
        h.get_by_role_and_label(Role::Label, "No matching commands");
    }
    for query in ["Ctrl+Y", "Ctrl+Shift+Z"] {
        h.state_mut().dialogs.shortcut_browser.query = query.into();
        h.run_steps(4);
        h.get_by_role_and_label(Role::Label, "Redo");
    }
    h.key_press_modifiers(Modifiers::COMMAND, Key::Y);
    h.key_press(Key::Delete);
    h.run_steps(4);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
}

#[test]
fn reference_search_finds_aliases_without_dispatching_into_the_map() {
    let mut h = harness(CenterMode::Map, egui::vec2(640.0, 360.0));
    let before = h.state().doc().unwrap().doc.workbook().clone();
    for (query, title) in [
        ("Ctrl+Y", "Redo"),
        ("Insert", "Subtopic"),
        ("Ctrl+3", "Priority 3"),
    ] {
        h.state_mut().dialogs.shortcut_browser.query = query.into();
        h.run_steps(4);
        h.get_by_role_and_label(Role::Label, title).scroll_to_me();
        h.run_steps(4);
        assert!(
            h.ctx
                .content_rect()
                .contains_rect(h.get_by_role_and_label(Role::Label, title).rect())
        );
    }
    h.key_press_modifiers(Modifiers::COMMAND, Key::Num3);
    h.key_press(Key::Insert);
    h.run_steps(4);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    assert!(h.state().dialogs.shortcuts);
}

#[test]
fn insertion_marker_bindings_preserve_title_input_and_undo() {
    let app = ThinodaApp::headless();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 800.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    h.run_steps(3);
    let root = h.state().doc().unwrap().doc.sheet().root;
    let before = h.state().doc().unwrap().doc.workbook().clone();
    let ctx = h.ctx.clone();
    for (key, marker) in [
        (Key::Num1, "priority-1"),
        (Key::Num2, "priority-2"),
        (Key::Num3, "priority-3"),
    ] {
        h.key_press_modifiers(Modifiers::COMMAND, key);
        h.run_steps(3);
        assert_eq!(
            h.state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .topic(root)
                .unwrap()
                .markers,
            vec![marker]
        );
        h.state_mut()
            .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
        h.run_steps(3);
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    }
    let doc = h.state_mut().doc_mut().unwrap();
    doc.view.begin_edit(doc.doc.sheet(), root, false);
    doc.view.set_editing_text("Uncommitted title");
    h.run_steps(3);
    for key in [Key::Num1, Key::Num2, Key::Num3, Key::B, Key::L] {
        h.key_press_modifiers(Modifiers::COMMAND, key);
        h.run_steps(3);
    }
    let doc = h.state().doc().unwrap();
    assert_eq!(doc.doc.workbook(), &before);
    assert_eq!(doc.view.editing_text(), Some("Uncommitted title"));
}

#[test]
fn platform_menu_hints_and_global_dispatch_keep_the_same_commands() {
    use egui::os::OperatingSystem;
    for os in [
        OperatingSystem::Windows,
        OperatingSystem::Mac,
        OperatingSystem::Nix,
    ] {
        let app = ThinodaApp::headless();
        let mut h = Harness::builder()
            .with_size(egui::vec2(1200.0, 900.0))
            .build_ui_state(
                move |ui, app: &mut ThinodaApp| {
                    ui.ctx().set_os(os);
                    app.draw(ui);
                },
                app,
            );
        h.run_steps(3);
        h.get_by_label(thinoda_ui::icons::MENU).click();
        h.run_steps(3);
        h.get_by_role_and_label(Role::Button, "Edit").click();
        h.run_steps(3);
        for (label, command) in [
            ("Settings…", Cmd::Settings),
            ("Undo", Cmd::Canvas(thinoda_canvas::Action::Undo)),
        ] {
            let hint = ThinodaApp::shortcut_hint(&h.ctx, &command).unwrap();
            h.get_by_role_and_label(Role::Button, &format!("{label} {hint}"));
        }
        h.key_press(Key::Escape);
        h.run_steps(3);
        h.key_press_modifiers(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::P);
        h.run_steps(3);
        assert!(h.state().palette.open);
        h.key_press(Key::Escape);
        h.run_steps(3);
        let count = h.state().docs.len();
        h.key_press_modifiers(Modifiers::COMMAND, Key::N);
        h.run_steps(3);
        assert_eq!(h.state().docs.len(), count + 1);
        h.key_press_modifiers(Modifiers::COMMAND, Key::W);
        h.run_steps(3);
        assert_eq!(h.state().docs.len(), count);
    }
}

#[test]
fn palette_close_targets_the_active_document_and_reference_includes_missing_globals() {
    let mut h = harness(CenterMode::Map, egui::vec2(1000.0, 800.0));
    h.get_by_role_and_label(Role::Label, "Command Palette…");
    h.get_by_role_and_label(Role::Label, "Next Document");
    h.get_by_role_and_label(Role::Label, "Close Tab");
    h.key_press(Key::Escape);
    h.run_steps(3);
    h.state_mut().new_doc(thinoda_core::Workbook::default());
    let first = h.state().docs[0].key.clone();
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::Palette);
    h.state_mut().palette.set_query("Close Tab");
    h.run_steps(4);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(h.state().docs.len(), 1);
    assert_eq!(h.state().docs[0].key, first);
    h.state_mut().new_doc(thinoda_core::Workbook::default());
    let doc = h.state_mut().doc_mut().unwrap();
    let root = doc.doc.sheet().root;
    doc.doc
        .edit_sheet("Title", |sheet| sheet.set_title(root, "Unsaved"))
        .unwrap();
    let before = doc.doc.workbook().clone();
    h.state_mut().exec(&ctx, Cmd::Palette);
    h.state_mut().palette.set_query("Close Tab");
    h.run_steps(4);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(h.state().dialogs.close, Some(CloseIntent::Tab(1)));
    assert_eq!(h.state().docs.len(), 2);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    assert!(h.state().doc().unwrap().is_dirty());
}

#[test]
fn shortcuts_search_category_and_unassigned_filters_use_the_command_catalog() {
    let mut h = harness(CenterMode::Map, egui::vec2(1000.0, 800.0));
    let ctx = h.ctx.clone();
    h.get_by_label("Search commands or shortcuts").click();
    h.run_steps(2);
    h.get_by_label("Search commands or shortcuts")
        .type_text("quick style");
    h.run_steps(3);
    h.get_by_role_and_label(Role::Label, "Save Quick Style…");
    assert!(h.query_by_role_and_label(Role::Label, "Save").is_none());
    h.get_by_role_and_label(Role::CheckBox, "Unassigned only")
        .click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Label, "Save Quick Style…");
    h.get_by_label("Search commands or shortcuts").click();
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.key_press(Key::Backspace);
    h.run_steps(3);
    h.get_by_label("Shortcut category").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Insert")
        .scroll_to_me();
    h.run_steps(8);
    h.get_by_role_and_label(Role::Button, "Insert").click();
    h.run_steps(3);
    assert_eq!(
        h.state().dialogs.shortcut_browser.category.as_deref(),
        Some("Insert")
    );
    h.get_by_role_and_label(Role::Label, "To-Do");
    assert!(h.query_by_role_and_label(Role::Label, "Subtopic").is_none());
    assert!(
        h.query_by_role_and_label(Role::Label, "Save Quick Style…")
            .is_none()
    );
    h.get_by_label("Search commands or shortcuts").click();
    h.run_steps(2);
    h.get_by_label("Search commands or shortcuts")
        .type_text("does-not-match");
    h.run_steps(3);
    h.get_by_label("No matching commands");
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(!h.state().dialogs.shortcuts);
    h.state_mut().exec(&ctx, Cmd::Shortcuts);
    h.run_steps(3);
    assert!(h.state().dialogs.shortcut_browser.query.is_empty());
    assert!(h.state().dialogs.shortcut_browser.category.is_none());
    assert!(!h.state().dialogs.shortcut_browser.unassigned_only);
}

#[test]
fn shortcuts_modal_preserves_map_and_outliner_and_has_a_visible_close_button() {
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        let mut h = harness(mode, egui::vec2(420.0, 540.0));
        let before = h.state().doc().unwrap().doc.workbook().clone();
        let selection = h.state().doc().unwrap().view.selection.clone();
        let close = h.get_by_role_and_label(Role::Button, "Close").rect();
        assert!(h.ctx.content_rect().contains_rect(close), "{close:?}");
        for key in [Key::Delete, Key::Enter, Key::Tab] {
            h.key_press(key);
            h.run_steps(2);
        }
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        h.key_press(Key::Escape);
        h.run_steps(3);
        assert!(!h.state().dialogs.shortcuts);
        assert_eq!(h.state().doc().unwrap().view.selection, selection);
    }
}

#[test]
fn advertised_quit_shortcut_requests_native_close_and_dirty_documents_keep_save_prompt() {
    let mut app = ThinodaApp::headless();
    let root = app.doc().unwrap().doc.sheet().root;
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Title", |sheet| sheet.set_title(root, "Unsaved"))
        .unwrap();
    let before = app.doc().unwrap().doc.workbook().clone();
    let ctx = egui::Context::default();
    let mut input = egui::RawInput::default();
    input.events.push(egui::Event::Key {
        key: Key::Q,
        physical_key: Some(Key::Q),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    let mut output = ctx.run_ui(input, |ui| app.global_shortcuts(ui.ctx()));
    output.textures_delta.clear();
    assert!(
        output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::Close)
    );
    let mut input = egui::RawInput::default();
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .events
        .push(egui::ViewportEvent::Close);
    let mut output = ctx.run_ui(input, |ui| app.handle_close_request(ui.ctx()));
    output.textures_delta.clear();
    assert_eq!(app.dialogs.close, Some(CloseIntent::Quit));
    assert!(
        output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::CancelClose)
    );
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    assert!(app.doc().unwrap().is_dirty());
    assert!(
        ThinodaApp::palette_commands().iter().any(
            |(cmd, command)| cmd == &Cmd::Quit && command.shortcut.as_deref() == Some("Ctrl+Q")
        )
    );
}
