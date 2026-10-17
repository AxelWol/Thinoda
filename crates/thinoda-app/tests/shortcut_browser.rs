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
