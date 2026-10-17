use egui::{Key, Modifiers, accesskit::Role};
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::app::{CenterMode, Cmd, ThinodaApp};

fn harness(mut app: ThinodaApp, size: egui::Vec2) -> Harness<'static, ThinodaApp> {
    app.exec(&egui::Context::default(), Cmd::UserGuide);
    let mut h = Harness::builder()
        .with_size(size)
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    h.run_steps(5);
    h
}

#[test]
fn guide_is_available_without_a_document_and_keeps_footer_inside_narrow_windows() {
    for size in [egui::vec2(420.0, 540.0), egui::vec2(1280.0, 900.0)] {
        let mut app = ThinodaApp::headless();
        app.active = app.docs.len();
        let mut h = harness(app, size);
        h.get_by_label("Search the guide");
        h.get_by_label("Guide section");
        let close = h.get_by_role_and_label(Role::Button, "Close").rect();
        assert!(
            h.ctx.content_rect().contains_rect(close),
            "{size:?}: {close:?}"
        );
        h.get_by_role_and_label(Role::Button, "Close").focus();
        h.run_steps(2);
        h.key_press(Key::Enter);
        h.run_steps(3);
        assert!(!h.state().dialogs.guide.open);
        assert!(h.state().doc().is_none());
    }
}

#[test]
fn guide_search_and_escape_do_not_edit_the_map_or_outliner_behind_it() {
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        let mut app = ThinodaApp::headless();
        app.mode = mode;
        let root = app.doc().unwrap().doc.sheet().root;
        app.doc_mut().unwrap().view.selection = vec![root];
        let before = app.doc().unwrap().doc.workbook().clone();
        let mut h = harness(app, egui::vec2(1000.0, 800.0));
        for key in [Key::Delete, Key::Enter, Key::Tab] {
            h.key_press(key);
            h.run_steps(3);
        }
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        h.get_by_label("Search the guide").click();
        h.run_steps(2);
        h.get_by_label("Search the guide").type_text("clipboard");
        h.run_steps(3);
        h.get_by_role_and_label(Role::Label, "Import and Export");
        assert!(
            h.query_by_role_and_label(Role::Label, "Quick Start")
                .is_none()
        );
        h.key_press_modifiers(Modifiers::COMMAND, Key::A);
        h.get_by_label("Search the guide")
            .type_text("no-such-guide-entry");
        h.run_steps(3);
        h.get_by_label("No matching guide sections");
        h.key_press(Key::Escape);
        h.run_steps(3);
        assert!(!h.state().dialogs.guide.open);
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        assert_eq!(h.state().doc().unwrap().view.selection, vec![root]);
        assert!(!h.state().doc().unwrap().doc.can_undo());
    }
}

#[test]
fn quick_start_opens_only_intro_and_can_navigate_to_all_sections() {
    let mut h = harness(ThinodaApp::headless(), egui::vec2(1000.0, 800.0));
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::QuickStart);
    h.run_steps(4);
    h.get_by_role_and_label(Role::Label, "Quick Start");
    assert!(
        h.query_by_role_and_label(Role::Label, "Files and Recovery")
            .is_none()
    );
    h.get_by_role_and_label(Role::Button, "All sections")
        .click();
    h.run_steps(4);
    h.get_by_role_and_label(Role::Label, "Files and Recovery");
    h.get_by_label("Guide section").click();
    h.run_steps(3);
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(h.state().dialogs.guide.open);
    h.get_by_label("Guide section").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Styles and Themes")
        .click();
    h.run_steps(4);
    h.get_by_role_and_label(Role::Label, "Styles and Themes");
    assert!(
        h.query_by_role_and_label(Role::Label, "Quick Start")
            .is_none()
    );
    let commands = ThinodaApp::palette_commands();
    for command in [
        Cmd::UserGuide,
        Cmd::QuickStart,
        Cmd::Feedback,
        Cmd::ProjectNews,
    ] {
        assert!(commands.iter().any(|(cmd, _)| cmd == &command));
    }
}

#[test]
fn external_help_links_open_only_fixed_public_urls_without_document_content() {
    let mut app = ThinodaApp::headless();
    let before = app.doc().unwrap().doc.workbook().clone();
    let ctx = egui::Context::default();
    for (command, expected) in [
        (
            Cmd::Feedback,
            "https://github.com/AxelWol/Thinoda/issues/new/choose",
        ),
        (
            Cmd::ProjectNews,
            "https://github.com/AxelWol/Thinoda/commits",
        ),
    ] {
        let mut output = ctx.run_ui(Default::default(), |ui| app.exec(ui.ctx(), command.clone()));
        output.textures_delta.clear();
        let urls: Vec<_> = output
            .platform_output
            .commands
            .iter()
            .filter_map(|command| {
                if let egui::OutputCommand::OpenUrl(url) = command {
                    Some(url)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].url, expected);
        assert!(urls[0].new_tab);
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    }
}

#[test]
fn help_menu_opens_quick_start_and_guide_without_network_or_map_changes() {
    let app = ThinodaApp::headless();
    let before = app.doc().unwrap().doc.workbook().clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 900.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    h.run_steps(4);
    for entry in ["Quick Start", "User Guide"] {
        h.get_by_role_and_label(Role::Button, "☰").click();
        h.run_steps(3);
        h.get_by_role_and_label(Role::Button, "Help").click();
        h.run_steps(3);
        h.get_by_role_and_label(Role::Button, entry).click();
        h.run_steps(4);
        assert!(h.state().dialogs.guide.open);
        h.get_by_role_and_label(Role::Image, "Thinoda logo");
        assert_eq!(
            h.state().dialogs.guide.section.is_some(),
            entry == "Quick Start"
        );
        h.get_by_role_and_label(Role::Button, "Close").click();
        h.run_steps(4);
        assert!(!h.state().dialogs.guide.open);
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    }
}
