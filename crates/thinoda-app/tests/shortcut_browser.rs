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
        let modifier = if os == OperatingSystem::Mac {
            "Cmd"
        } else {
            "Ctrl"
        };
        h.get_by_role_and_label(Role::Button, &format!("Settings… {modifier}+,"));
        h.get_by_role_and_label(Role::Button, &format!("Undo {modifier}+Z"));
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
