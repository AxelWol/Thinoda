use egui::{Key, Modifiers, accesskit::Role};
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{CenterMode, Cmd, ThinodaApp},
    doc::OpenDoc,
    i18n::t,
};
use thinoda_canvas::Action;
use thinoda_platform::settings::ThemeMode;

#[test]
fn copying_plain_text_requires_preview_and_does_not_change_the_note_or_undo_history() {
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        let mut app = ThinodaApp::headless();
        app.mode = mode;
        let doc = app.doc_mut().unwrap();
        let root = doc.doc.sheet().root;
        let source = "**café** and [label](https://example.invalid/private)\n\n- first\n- second";
        doc.doc
            .edit_sheet("Fixture", |sheet| {
                sheet.set_notes(root, Some(source.into()))
            })
            .unwrap();
        let revision = doc.doc.state_id();
        let mut h = Harness::builder()
            .with_size(egui::vec2(1280.0, 1000.0))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        h.run_steps(4);
        assert!(
            h.query_by_role_and_label(Role::Button, t("Copy note as plain text"))
                .is_none()
        );
        h.get_by_role_and_label(Role::CheckBox, t("Markdown preview"))
            .scroll_to_me();
        h.run_steps(8);
        h.get_by_role_and_label(Role::CheckBox, t("Markdown preview"))
            .click();
        h.run_steps(4);
        h.get_by_role_and_label(Role::Button, t("Copy note as plain text"))
            .scroll_to_me();
        h.run_steps(8);
        h.get_by_role_and_label(Role::Button, t("Copy note as plain text"))
            .click();
        h.step();
        assert!(h.output().platform_output.commands.iter().any(|command| {
            matches!(command, egui::OutputCommand::CopyText(text) if text == "café and label\nfirst\nsecond")
        }));
        let doc = h.state().doc().unwrap();
        assert_eq!(doc.doc.state_id(), revision);
        assert_eq!(doc.doc.sheet().root_topic().notes.as_deref(), Some(source));
    }
}

#[test]
fn list_actions_convert_and_toggle_as_single_edits_in_both_views() {
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        let mut app = ThinodaApp::headless();
        app.mode = mode;
        let doc = app.doc_mut().unwrap();
        let root = doc.doc.sheet().root;
        let source = "café\n日本語";
        doc.doc
            .edit_sheet("Fixture", |sheet| {
                sheet.set_notes(root, Some(source.into()))
            })
            .unwrap();
        let style = doc.doc.sheet().root_topic().style.clone();
        let mut h = Harness::builder()
            .with_size(egui::vec2(1280.0, 1000.0))
            .build_ui_state(
                |ui, app: &mut ThinodaApp| {
                    ui.ctx()
                        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
                    app.draw(ui);
                },
                app,
            );
        h.run_steps(4);
        h.get_by_role_and_label(Role::MultilineTextInput, t("Notes"))
            .focus();
        h.run_steps(3);
        h.key_press_modifiers(Modifiers::CTRL | Modifiers::COMMAND, Key::A);
        h.run_steps(3);
        for (label, expected) in [
            ("Bulleted list", "- café\n- 日本語"),
            ("Numbered list", "1. café\n2. 日本語"),
            ("Numbered list", source),
        ] {
            h.get_by_role_and_label(Role::Button, t(label))
                .scroll_to_me();
            h.run_steps(8);
            h.get_by_role_and_label(Role::Button, t(label)).click();
            h.run_steps(4);
            let doc = h.state().doc().unwrap();
            assert_eq!(
                doc.doc.sheet().root_topic().notes.as_deref(),
                Some(expected)
            );
            assert_eq!(doc.doc.sheet().root_topic().style, style);
        }
        let ctx = h.ctx.clone();
        h.state_mut().exec(&ctx, Cmd::Canvas(Action::Undo));
        assert_eq!(
            h.state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .root_topic()
                .notes
                .as_deref(),
            Some("1. café\n2. 日本語")
        );
        h.state_mut().exec(&ctx, Cmd::Canvas(Action::Redo));
        assert_eq!(
            h.state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .root_topic()
                .notes
                .as_deref(),
            Some(source)
        );
    }
}

#[test]
fn notes_formatting_commits_focused_unicode_draft_and_round_trips_with_undo() {
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        let mut app = ThinodaApp::headless();
        app.mode = mode;
        let mut h = Harness::builder()
            .with_size(egui::vec2(1280.0, 1000.0))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        h.run_steps(4);
        h.get_by_role_and_label(Role::MultilineTextInput, t("Notes"))
            .focus();
        h.run_steps(2);
        h.get_by_role_and_label(Role::MultilineTextInput, t("Notes"))
            .type_text("café research");
        h.run_steps(2);
        h.key_press_modifiers(Modifiers::CTRL | Modifiers::COMMAND, Key::A);
        h.run_steps(2);
        let focused = h.ctx.memory(|memory| memory.focused()).unwrap();
        let selected = egui::TextEdit::load_state(&h.ctx, focused)
            .unwrap()
            .cursor
            .char_range()
            .unwrap()
            .as_sorted_char_range();
        assert_eq!(selected.start.0..selected.end.0, 0..13);
        h.get_by_role_and_label(Role::Button, t("Bold notes"))
            .scroll_to_me();
        h.run_steps(8);
        h.get_by_role_and_label(Role::Button, t("Bold notes"))
            .click();
        h.run_steps(3);
        assert_eq!(
            h.state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .root_topic()
                .notes
                .as_deref(),
            Some("**café research**")
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.thinoda");
        h.state_mut()
            .doc_mut()
            .unwrap()
            .save_to(&path, false)
            .unwrap();
        assert_eq!(
            OpenDoc::open(&path)
                .unwrap()
                .doc
                .sheet()
                .root_topic()
                .notes
                .as_deref(),
            Some("**café research**")
        );
        let ctx = h.ctx.clone();
        h.state_mut().exec(&ctx, Cmd::Canvas(Action::Undo));
        assert_eq!(
            h.state().doc().unwrap().doc.sheet().root_topic().notes,
            None
        );
        h.state_mut().exec(&ctx, Cmd::Canvas(Action::Redo));
        assert_eq!(
            h.state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .root_topic()
                .notes
                .as_deref(),
            Some("**café research**")
        );
    }
}

#[test]
fn notes_preview_is_opt_in_read_only_and_theme_aware() {
    for theme in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System] {
        let mut app = ThinodaApp::headless();
        app.settings.theme = theme;
        let doc = app.doc_mut().unwrap();
        let root = doc.doc.sheet().root;
        let source = "**Research**\n\n- *First finding*\n- Second finding";
        doc.doc
            .edit_sheet("Fixture", |sheet| {
                sheet.set_notes(root, Some(source.into()))
            })
            .unwrap();
        let state = doc.doc.state_id();
        let mut h = Harness::builder()
            .with_size(egui::vec2(1280.0, 1100.0))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        h.run_steps(4);
        assert!(h.query_by_label("Research").is_none());
        h.get_by_role_and_label(Role::CheckBox, t("Markdown preview"))
            .scroll_to_me();
        h.run_steps(8);
        h.get_by_role_and_label(Role::CheckBox, t("Markdown preview"))
            .click();
        h.run_steps(3);
        assert!(h.query_by_label("Research").is_some());
        assert!(h.query_by_label("First finding").is_some());
        assert_eq!(h.state().doc().unwrap().doc.state_id(), state);
        assert_eq!(
            h.state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .root_topic()
                .notes
                .as_deref(),
            Some(source)
        );
    }
}

#[test]
fn repeated_emphasis_restores_the_original_note_without_changing_topic_style() {
    let mut app = ThinodaApp::headless();
    let doc = app.doc_mut().unwrap();
    let root = doc.doc.sheet().root;
    doc.doc
        .edit_sheet("Fixture", |sheet| {
            sheet.set_notes(root, Some("café research".into()))
        })
        .unwrap();
    let style = doc.doc.sheet().root_topic().style.clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 1000.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(4);
    h.get_by_role_and_label(Role::MultilineTextInput, t("Notes"))
        .focus();
    h.run_steps(3);
    h.key_press_modifiers(Modifiers::CTRL | Modifiers::COMMAND, Key::A);
    h.run_steps(3);
    for expected in ["**café research**", "café research"] {
        h.get_by_role_and_label(Role::Button, t("Bold notes"))
            .scroll_to_me();
        h.run_steps(8);
        h.get_by_role_and_label(Role::Button, t("Bold notes"))
            .click();
        h.run_steps(4);
        let doc = h.state().doc().unwrap();
        assert_eq!(
            doc.doc.sheet().root_topic().notes.as_deref(),
            Some(expected)
        );
        assert_eq!(doc.doc.sheet().root_topic().style, style);
    }
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::Canvas(Action::Undo));
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .root_topic()
            .notes
            .as_deref(),
        Some("**café research**")
    );
}

#[test]
fn preview_links_are_inert_and_excessive_nesting_keeps_the_original_note() {
    let mut app = ThinodaApp::headless();
    let doc = app.doc_mut().unwrap();
    let root = doc.doc.sheet().root;
    doc.doc.edit_sheet("Fixture", |sheet| {
            sheet.set_notes(root, Some("[external](https://example.invalid/private)\n\n![image](file:///private.png)\n\n<script>no execution</script>".into()))
        }).unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 1100.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(4);
    h.get_by_role_and_label(Role::CheckBox, t("Markdown preview"))
        .scroll_to_me();
    h.run_steps(8);
    h.get_by_role_and_label(Role::CheckBox, t("Markdown preview"))
        .click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Link, "external").click();
    h.run_steps(1);
    assert!(
        !h.output()
            .platform_output
            .commands
            .iter()
            .any(|command| { matches!(command, egui::OutputCommand::OpenUrl(_)) })
    );
    let source = "> ".repeat(40);
    h.state_mut()
        .doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Nested note", |sheet| {
            sheet.set_notes(root, Some(source.clone()))
        })
        .unwrap();
    h.run_steps(3);
    assert!(
        h.query_by_label(t(
            "Note preview limit reached. The full source is preserved and editable."
        ))
        .is_some()
    );
    assert!(
        h.query_by_role_and_label(Role::Button, t("Copy note as plain text"))
            .is_none()
    );
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .root_topic()
            .notes
            .as_deref(),
        Some(source.as_str())
    );
}
