use egui::{Key, Modifiers, accesskit::Role};
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{CenterMode, Cmd, ThinodaApp},
    doc::OpenDoc,
    i18n::t,
};
use thinoda_canvas::Action;
use thinoda_platform::{Paths, recovery::RecoveryManager, settings::ThemeMode};

#[test]
fn copying_plain_text_requires_preview_and_does_not_change_the_note_or_undo_history() {
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        let mut app = ThinodaApp::headless();
        app.mode = mode;
        app.format_tab = thinoda_app::app::FormatTab::Data;
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
        app.format_tab = thinoda_app::app::FormatTab::Data;
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
fn list_actions_reject_code_and_unselected_paragraph_changes_without_committing() {
    let warning = "Cannot apply formatting to this selection. The formatting action was canceled.";
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        for (source, selection, markdown) in
            [("one\ntwo", 0..3, false), ("```\ncode\n```", 4..8, true)]
        {
            for label in ["Bulleted list", "Numbered list"] {
                let mut app = ThinodaApp::headless();
                app.mode = mode;
                app.format_tab = thinoda_app::app::FormatTab::Data;
                let doc = app.doc_mut().unwrap();
                let root = doc.doc.sheet().root;
                doc.doc
                    .edit_sheet("Fixture", |sheet| {
                        if markdown {
                            sheet.set_markdown_notes(root, Some(source.into()))
                        } else {
                            sheet.set_notes(root, Some(source.into()))
                        }
                    })
                    .unwrap();
                let revision = doc.doc.state_id();
                let mut h = Harness::builder()
                    .with_size(egui::vec2(1280.0, 1000.0))
                    .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
                h.run_steps(4);
                h.get_by_role_and_label(Role::MultilineTextInput, t("Notes"))
                    .focus();
                h.run_steps(3);
                let focused = h.ctx.memory(|memory| memory.focused()).unwrap();
                let mut state = egui::TextEdit::load_state(&h.ctx, focused).unwrap();
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::two(
                        egui::text::CCursor::new(selection.start),
                        egui::text::CCursor::new(selection.end),
                    )));
                state.store(&h.ctx, focused);
                h.run_steps(3);
                h.get_by_role_and_label(Role::Button, t(label))
                    .scroll_to_me();
                h.run_steps(8);
                h.get_by_role_and_label(Role::Button, t(label)).click();
                h.run_steps(4);

                let doc = h.state().doc().unwrap();
                assert_eq!(doc.doc.state_id(), revision, "{mode:?} {label}");
                let note = doc.doc.sheet().root_topic().notes.as_ref().unwrap();
                assert_eq!(&**note, source);
                assert_eq!(note.is_markdown(), markdown);
                assert!(h.query_by_label(t(warning)).is_some());
                assert_eq!(h.ctx.memory(|memory| memory.focused()), Some(focused));
                let actual = egui::TextEdit::load_state(&h.ctx, focused)
                    .unwrap()
                    .cursor
                    .char_range()
                    .unwrap()
                    .as_sorted_char_range();
                assert_eq!(actual.start.0..actual.end.0, selection);
            }
        }
    }
}

#[test]
fn notes_formatting_commits_focused_unicode_draft_and_round_trips_with_undo() {
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        let mut app = ThinodaApp::headless();
        app.mode = mode;
        app.format_tab = thinoda_app::app::FormatTab::Data;
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
        assert!(
            h.state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .root_topic()
                .notes
                .as_ref()
                .unwrap()
                .is_markdown()
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.thinoda");
        h.state_mut()
            .doc_mut()
            .unwrap()
            .save_to(&path, false)
            .unwrap();
        let reopened = OpenDoc::open(&path).unwrap();
        assert_eq!(
            reopened.doc.sheet().root_topic().notes,
            h.state().doc().unwrap().doc.sheet().root_topic().notes
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
        assert!(
            h.state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .root_topic()
                .notes
                .as_ref()
                .unwrap()
                .is_markdown()
        );
    }
}

#[test]
fn formatting_plain_notes_preserves_literal_markup_and_selection() {
    let source = "*literal* [link](https://example.invalid) café";
    let start = source[..source.find("café").unwrap()].chars().count();
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        let mut app = ThinodaApp::headless();
        app.mode = mode;
        app.format_tab = thinoda_app::app::FormatTab::Data;
        let doc = app.doc_mut().unwrap();
        let root = doc.doc.sheet().root;
        doc.doc
            .edit_sheet("Fixture", |sheet| {
                sheet.set_notes(root, Some(source.into()))
            })
            .unwrap();
        let mut h = Harness::builder()
            .with_size(egui::vec2(1280.0, 1000.0))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        h.run_steps(4);
        h.get_by_role_and_label(Role::MultilineTextInput, t("Notes"))
            .focus();
        h.run_steps(3);
        let focused = h.ctx.memory(|memory| memory.focused()).unwrap();
        let mut state = egui::TextEdit::load_state(&h.ctx, focused).unwrap();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(start),
                egui::text::CCursor::new(start + 4),
            )));
        state.store(&h.ctx, focused);
        h.run_steps(3);
        h.get_by_role_and_label(Role::Button, t("Bold notes"))
            .scroll_to_me();
        h.run_steps(8);
        h.get_by_role_and_label(Role::Button, t("Bold notes"))
            .click();
        h.run_steps(4);

        let note = h
            .state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .root_topic()
            .notes
            .as_ref()
            .unwrap();
        assert!(note.is_markdown());
        assert_eq!(
            thinoda_core::notes::plain_text_projection(note).as_deref(),
            Some(source)
        );
        let formatted = thinoda_core::notes::formatted_document(note).unwrap();
        assert!(
            formatted
                .paragraphs
                .iter()
                .flat_map(|paragraph| &paragraph.runs)
                .any(|run| run.text == "café" && run.style.bold)
        );
        let selected = egui::TextEdit::load_state(&h.ctx, focused)
            .unwrap()
            .cursor
            .char_range()
            .unwrap()
            .as_sorted_char_range();
        assert_eq!(
            note.chars()
                .skip(selected.start.0)
                .take(selected.end.0 - selected.start.0)
                .collect::<String>(),
            "café"
        );
        let saved = note.clone();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("literal-notes.thinoda");
        h.state_mut()
            .doc_mut()
            .unwrap()
            .save_to(&path, false)
            .unwrap();
        assert_eq!(
            OpenDoc::open(&path).unwrap().doc.sheet().root_topic().notes,
            Some(saved.clone())
        );
        let ctx = h.ctx.clone();
        h.state_mut().exec(&ctx, Cmd::Canvas(Action::Undo));
        let original = h
            .state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .root_topic()
            .notes
            .as_ref()
            .unwrap();
        assert_eq!(&**original, source);
        assert!(!original.is_markdown());
        h.state_mut().exec(&ctx, Cmd::Canvas(Action::Redo));
        assert_eq!(
            h.state().doc().unwrap().doc.sheet().root_topic().notes,
            Some(saved)
        );
    }
}

#[test]
fn formatting_plain_focused_draft_preserves_literal_markup() {
    let source = "*literal* café";
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        let mut app = ThinodaApp::headless();
        app.mode = mode;
        app.format_tab = thinoda_app::app::FormatTab::Data;
        let mut h = Harness::builder()
            .with_size(egui::vec2(1280.0, 1000.0))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        h.run_steps(4);
        h.get_by_role_and_label(Role::MultilineTextInput, t("Notes"))
            .focus();
        h.run_steps(2);
        h.get_by_role_and_label(Role::MultilineTextInput, t("Notes"))
            .type_text(source);
        h.run_steps(2);
        h.key_press_modifiers(Modifiers::CTRL | Modifiers::COMMAND, Key::A);
        h.run_steps(2);
        h.get_by_role_and_label(Role::Button, t("Bold notes"))
            .scroll_to_me();
        h.run_steps(8);
        h.get_by_role_and_label(Role::Button, t("Bold notes"))
            .click();
        h.run_steps(4);
        let note = h
            .state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .root_topic()
            .notes
            .as_ref()
            .unwrap();
        assert!(note.is_markdown());
        assert_eq!(
            thinoda_core::notes::plain_text_projection(note).as_deref(),
            Some(source)
        );
        let saved = note.clone();
        let ctx = h.ctx.clone();
        h.state_mut().exec(&ctx, Cmd::Canvas(Action::Undo));
        assert!(
            h.state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .root_topic()
                .notes
                .is_none()
        );
        h.state_mut().exec(&ctx, Cmd::Canvas(Action::Redo));
        assert_eq!(
            h.state().doc().unwrap().doc.sheet().root_topic().notes,
            Some(saved)
        );
    }
}

#[test]
fn formatting_plain_notes_rejects_unrepresentable_literal_blocks() {
    let source = "    literal\ncafé";
    let warning = "Cannot apply formatting to this selection. The formatting action was canceled.";
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        let mut app = ThinodaApp::headless();
        app.mode = mode;
        app.format_tab = thinoda_app::app::FormatTab::Data;
        let doc = app.doc_mut().unwrap();
        let root = doc.doc.sheet().root;
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
        h.get_by_role_and_label(Role::MultilineTextInput, t("Notes"))
            .focus();
        h.run_steps(3);
        let focused = h.ctx.memory(|memory| memory.focused()).unwrap();
        let mut state = egui::TextEdit::load_state(&h.ctx, focused).unwrap();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(12),
                egui::text::CCursor::new(16),
            )));
        state.store(&h.ctx, focused);
        h.run_steps(3);
        h.get_by_role_and_label(Role::Button, t("Bold notes"))
            .scroll_to_me();
        h.run_steps(8);
        h.get_by_role_and_label(Role::Button, t("Bold notes"))
            .click();
        h.run_steps(4);
        let doc = h.state().doc().unwrap();
        let note = doc.doc.sheet().root_topic().notes.as_ref().unwrap();
        assert_eq!(doc.doc.state_id(), revision);
        assert_eq!(&**note, source);
        assert!(!note.is_markdown());
        assert!(h.query_by_label(t(warning)).is_some());
        assert_eq!(h.ctx.memory(|memory| memory.focused()), Some(focused));
        let selection = egui::TextEdit::load_state(&h.ctx, focused)
            .unwrap()
            .cursor
            .char_range()
            .unwrap()
            .as_sorted_char_range();
        assert_eq!(selection.start.0..selection.end.0, 12..16);
    }
}

#[test]
fn notes_preview_is_opt_in_read_only_and_theme_aware() {
    for theme in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System] {
        let mut app = ThinodaApp::headless();
        app.settings.theme = theme;
        app.format_tab = thinoda_app::app::FormatTab::Data;
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
    app.format_tab = thinoda_app::app::FormatTab::Data;
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
fn notes_strikethrough_preserves_unicode_in_both_views_save_and_undo() {
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        let mut app = ThinodaApp::headless();
        app.mode = mode;
        app.format_tab = thinoda_app::app::FormatTab::Data;
        let doc = app.doc_mut().unwrap();
        let root = doc.doc.sheet().root;
        let source = " café 🌱 ";
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
        for expected in [" ~~café 🌱~~ ", source] {
            h.get_by_role_and_label(Role::Button, t("Strikethrough notes"))
                .scroll_to_me();
            h.run_steps(8);
            h.get_by_role_and_label(Role::Button, t("Strikethrough notes"))
                .click();
            h.run_steps(4);
            let doc = h.state().doc().unwrap();
            let note = doc.doc.sheet().root_topic().notes.as_ref().unwrap();
            assert_eq!(&**note, expected);
            assert!(note.is_markdown());
            assert_eq!(doc.doc.sheet().root_topic().style, style);

            if expected != source {
                let parsed = thinoda_core::notes::formatted_document(expected).unwrap();
                assert!(
                    parsed
                        .paragraphs
                        .iter()
                        .flat_map(|p| &p.runs)
                        .any(|run| { run.text == "café 🌱" && run.style.strike })
                );
                let bytes = thinoda_io::export_bytes(
                    doc.doc.workbook(),
                    thinoda_io::Format::FreeMind,
                    &Default::default(),
                )
                .unwrap();
                assert!(
                    std::str::from_utf8(&bytes)
                        .unwrap()
                        .contains("<strike>café 🌱</strike>")
                );
                let dir = tempfile::tempdir().unwrap();
                let path = dir.path().join("notes.thinoda");
                h.state_mut()
                    .doc_mut()
                    .unwrap()
                    .save_to(&path, false)
                    .unwrap();
                let reopened = OpenDoc::open(&path).unwrap();
                assert_eq!(
                    reopened.doc.sheet().root_topic().notes,
                    h.state().doc().unwrap().doc.sheet().root_topic().notes
                );
            }
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
            Some(" ~~café 🌱~~ ")
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
fn notes_strikethrough_commits_focused_unicode_draft_as_one_undo() {
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        let mut app = ThinodaApp::headless();
        app.mode = mode;
        app.format_tab = thinoda_app::app::FormatTab::Data;
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
        h.get_by_role_and_label(Role::Button, t("Strikethrough notes"))
            .scroll_to_me();
        h.run_steps(8);
        h.get_by_role_and_label(Role::Button, t("Strikethrough notes"))
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
            Some("~~café research~~")
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
            Some("~~café research~~")
        );
    }
}

#[test]
fn notes_emphasis_rejects_unrenderable_selection_without_editing() {
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        for (label, warning, expected) in [
            (
                "Bold notes",
                "Cannot apply formatting to this selection. The formatting action was canceled.",
                "**first**\n\nsecond",
            ),
            (
                "Italic notes",
                "Cannot apply formatting to this selection. The formatting action was canceled.",
                "*first*\n\nsecond",
            ),
            (
                "Strikethrough notes",
                "Cannot apply formatting to this selection. The formatting action was canceled.",
                "~~first~~\n\nsecond",
            ),
        ] {
            let mut app = ThinodaApp::headless();
            app.mode = mode;
            app.format_tab = thinoda_app::app::FormatTab::Data;
            let doc = app.doc_mut().unwrap();
            let root = doc.doc.sheet().root;
            let source = "first\n\nsecond";
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
            h.get_by_role_and_label(Role::MultilineTextInput, t("Notes"))
                .focus();
            h.run_steps(3);
            h.key_press_modifiers(Modifiers::CTRL | Modifiers::COMMAND, Key::A);
            h.run_steps(3);
            h.get_by_role_and_label(Role::Button, t(label))
                .scroll_to_me();
            h.run_steps(8);
            h.get_by_role_and_label(Role::Button, t(label)).click();
            h.run_steps(4);
            let doc = h.state().doc().unwrap();
            assert_eq!(doc.doc.state_id(), revision, "{mode:?} {label}");
            let note = doc.doc.sheet().root_topic().notes.as_ref().unwrap();
            assert_eq!(&**note, source);
            assert!(!note.is_markdown());
            assert!(h.query_by_label(t(warning)).is_some());

            let focused = h.ctx.memory(|memory| memory.focused()).unwrap();
            let mut state = egui::TextEdit::load_state(&h.ctx, focused).unwrap();
            let selection = state.cursor.char_range().unwrap().as_sorted_char_range();
            assert_eq!(selection.start.0..selection.end.0, 0..13);
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    egui::text::CCursor::new(5),
                )));
            state.store(&h.ctx, focused);
            h.run_steps(3);
            h.get_by_role_and_label(Role::Button, t(label)).click();
            h.run_steps(4);
            let note = h
                .state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .root_topic()
                .notes
                .as_ref()
                .unwrap();
            assert_eq!(&**note, expected);
            assert!(note.is_markdown());
            assert!(h.query_by_label(t(warning)).is_none());
            let ctx = h.ctx.clone();
            h.state_mut().exec(&ctx, Cmd::Canvas(Action::Undo));
            let note = h
                .state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .root_topic()
                .notes
                .as_ref()
                .unwrap();
            assert_eq!(&**note, source);
            assert!(!note.is_markdown());
            h.state_mut().exec(&ctx, Cmd::Canvas(Action::Redo));
            let note = h
                .state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .root_topic()
                .notes
                .as_ref()
                .unwrap();
            assert_eq!(&**note, expected);
            assert!(note.is_markdown());
        }
    }
}

#[test]
fn notes_nested_bold_and_italic_remain_reversible_in_both_views() {
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        for (label, source, selection) in [
            ("Bold notes", "*café*", 0..6),
            ("Italic notes", "**café**", 2..6),
            ("Italic notes", "**café**", 1..7),
        ] {
            let mut app = ThinodaApp::headless();
            app.mode = mode;
            app.format_tab = thinoda_app::app::FormatTab::Data;
            let doc = app.doc_mut().unwrap();
            let root = doc.doc.sheet().root;
            doc.doc
                .edit_sheet("Fixture", |sheet| {
                    sheet.set_markdown_notes(root, Some(source.into()))
                })
                .unwrap();
            let mut h = Harness::builder()
                .with_size(egui::vec2(1280.0, 1000.0))
                .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
            h.run_steps(4);
            h.get_by_role_and_label(Role::MultilineTextInput, t("Notes"))
                .focus();
            h.run_steps(3);
            let focused = h.ctx.memory(|memory| memory.focused()).unwrap();
            let mut state = egui::TextEdit::load_state(&h.ctx, focused).unwrap();
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(selection.start),
                    egui::text::CCursor::new(selection.end),
                )));
            state.store(&h.ctx, focused);
            h.run_steps(3);
            h.get_by_role_and_label(Role::Button, t(label))
                .scroll_to_me();
            h.run_steps(8);
            h.get_by_role_and_label(Role::Button, t(label)).click();
            h.run_steps(4);
            let note = h
                .state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .root_topic()
                .notes
                .as_ref()
                .unwrap();
            assert_eq!(&**note, "***café***", "{mode:?} {label}");
            let document = thinoda_core::notes::formatted_document(note).unwrap();
            assert!(
                document
                    .paragraphs
                    .iter()
                    .flat_map(|p| &p.runs)
                    .any(|run| { run.text == "café" && run.style.bold && run.style.italic })
            );
            h.get_by_role_and_label(Role::Button, t(label)).click();
            h.run_steps(4);
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
                Some("***café***")
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
}

#[test]
fn notes_reject_partial_delimiters_that_would_change_other_styles() {
    let warning = "Cannot apply formatting to this selection. The formatting action was canceled.";
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        let mut app = ThinodaApp::headless();
        app.mode = mode;
        app.format_tab = thinoda_app::app::FormatTab::Data;
        let doc = app.doc_mut().unwrap();
        let root = doc.doc.sheet().root;
        doc.doc
            .edit_sheet("Fixture", |sheet| {
                sheet.set_markdown_notes(root, Some("__a__ b".into()))
            })
            .unwrap();
        let revision = doc.doc.state_id();
        let mut h = Harness::builder()
            .with_size(egui::vec2(1280.0, 1000.0))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        h.run_steps(4);
        h.get_by_role_and_label(Role::MultilineTextInput, t("Notes"))
            .focus();
        h.run_steps(3);
        let focused = h.ctx.memory(|memory| memory.focused()).unwrap();
        let mut state = egui::TextEdit::load_state(&h.ctx, focused).unwrap();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(1),
                egui::text::CCursor::new(4),
            )));
        state.store(&h.ctx, focused);
        h.run_steps(3);
        h.get_by_role_and_label(Role::Button, t("Bold notes"))
            .scroll_to_me();
        h.run_steps(8);
        h.get_by_role_and_label(Role::Button, t("Bold notes"))
            .click();
        h.run_steps(4);
        let doc = h.state().doc().unwrap();
        assert_eq!(doc.doc.state_id(), revision);
        let note = doc.doc.sheet().root_topic().notes.as_ref().unwrap();
        assert_eq!(&**note, "__a__ b");
        assert!(note.is_markdown());
        assert!(h.query_by_label(t(warning)).is_some());
        let editor = h.get_by_role_and_label(Role::MultilineTextInput, t("Notes"));
        assert!(editor.is_focused());
        let focused = h.ctx.memory(|memory| memory.focused()).unwrap();
        let selection = egui::TextEdit::load_state(&h.ctx, focused)
            .unwrap()
            .cursor
            .char_range()
            .unwrap()
            .as_sorted_char_range();
        assert_eq!(selection.start.0..selection.end.0, 1..4);
    }
}

#[test]
fn notes_empty_emphasis_rejects_code_and_existing_marker_boundaries() {
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        for (label, source, caret, warning) in [
            (
                "Bold notes",
                "**café**",
                1,
                "Cannot apply formatting to this selection. The formatting action was canceled.",
            ),
            (
                "Italic notes",
                "`café`",
                3,
                "Cannot apply formatting to this selection. The formatting action was canceled.",
            ),
            (
                "Italic notes",
                "**café**",
                4,
                "Cannot apply formatting to this selection. The formatting action was canceled.",
            ),
            (
                "Strikethrough notes",
                "~~café~~",
                1,
                "Cannot apply formatting to this selection. The formatting action was canceled.",
            ),
        ] {
            let mut app = ThinodaApp::headless();
            app.mode = mode;
            app.format_tab = thinoda_app::app::FormatTab::Data;
            let doc = app.doc_mut().unwrap();
            let root = doc.doc.sheet().root;
            doc.doc
                .edit_sheet("Fixture", |sheet| {
                    sheet.set_markdown_notes(root, Some(source.into()))
                })
                .unwrap();
            let revision = doc.doc.state_id();
            let mut h = Harness::builder()
                .with_size(egui::vec2(1280.0, 1000.0))
                .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
            h.run_steps(4);
            h.get_by_role_and_label(Role::MultilineTextInput, t("Notes"))
                .focus();
            h.run_steps(3);
            let focused = h.ctx.memory(|memory| memory.focused()).unwrap();
            let mut state = egui::TextEdit::load_state(&h.ctx, focused).unwrap();
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(
                    egui::text::CCursor::new(caret),
                )));
            state.store(&h.ctx, focused);
            h.run_steps(3);
            h.get_by_role_and_label(Role::Button, t(label))
                .scroll_to_me();
            h.run_steps(8);
            h.get_by_role_and_label(Role::Button, t(label)).click();
            h.run_steps(4);
            let doc = h.state().doc().unwrap();
            assert_eq!(doc.doc.state_id(), revision, "{mode:?} {label}");
            assert_eq!(doc.doc.sheet().root_topic().notes.as_deref(), Some(source));
            assert!(h.query_by_label(t(warning)).is_some());
            assert_eq!(h.ctx.memory(|memory| memory.focused()), Some(focused));
            let selection = egui::TextEdit::load_state(&h.ctx, focused)
                .unwrap()
                .cursor
                .char_range()
                .unwrap()
                .as_sorted_char_range();
            assert_eq!(selection.start.0..selection.end.0, caret..caret);
        }
    }
}

#[test]
fn empty_notes_emphasis_allows_typing_and_undo_in_both_views() {
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        for (label, placeholder, formatted, caret) in [
            ("Bold notes", "a****b", "a**é**b", 3),
            ("Italic notes", "a**b", "a*é*b", 2),
            ("Strikethrough notes", "a~~~~b", "a~~é~~b", 3),
        ] {
            let mut app = ThinodaApp::headless();
            app.mode = mode;
            app.format_tab = thinoda_app::app::FormatTab::Data;
            let doc = app.doc_mut().unwrap();
            let root = doc.doc.sheet().root;
            doc.doc
                .edit_sheet("Fixture", |sheet| sheet.set_notes(root, Some("ab".into())))
                .unwrap();
            let mut h = Harness::builder()
                .with_size(egui::vec2(1280.0, 1000.0))
                .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
            h.run_steps(4);
            h.get_by_role_and_label(Role::MultilineTextInput, t("Notes"))
                .focus();
            h.run_steps(3);
            let focused = h.ctx.memory(|memory| memory.focused()).unwrap();
            let mut state = egui::TextEdit::load_state(&h.ctx, focused).unwrap();
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(
                    egui::text::CCursor::new(1),
                )));
            state.store(&h.ctx, focused);
            h.run_steps(3);
            h.get_by_role_and_label(Role::Button, t(label))
                .scroll_to_me();
            h.run_steps(8);
            h.get_by_role_and_label(Role::Button, t(label)).click();
            h.run_steps(4);
            let note = h
                .state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .root_topic()
                .notes
                .as_ref()
                .unwrap();
            assert_eq!(&**note, placeholder, "{mode:?} {label}");
            assert!(note.is_markdown());
            let selection = egui::TextEdit::load_state(&h.ctx, focused)
                .unwrap()
                .cursor
                .char_range()
                .unwrap()
                .as_sorted_char_range();
            assert_eq!(selection.start.0..selection.end.0, caret..caret);
            h.get_by_role_and_label(Role::MultilineTextInput, t("Notes"))
                .type_text("é");
            h.run_steps(2);
            h.ctx.memory_mut(|memory| memory.surrender_focus(focused));
            h.run_steps(4);
            assert_eq!(
                h.state()
                    .doc()
                    .unwrap()
                    .doc
                    .sheet()
                    .root_topic()
                    .notes
                    .as_deref(),
                Some(formatted),
                "{mode:?} {label}"
            );
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
                Some(placeholder)
            );
            h.state_mut().exec(&ctx, Cmd::Canvas(Action::Undo));
            let note = h
                .state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .root_topic()
                .notes
                .as_ref()
                .unwrap();
            assert_eq!(&**note, "ab");
            assert!(!note.is_markdown());
        }
    }
}

#[test]
fn rejected_notes_formatting_preserves_a_focused_draft() {
    let warning = "Cannot apply formatting to this selection. The formatting action was canceled.";
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        for (label, source, selection) in [
            ("Bold notes", "first\n\nsecond", None),
            ("Italic notes", "first\n\nsecond", None),
            ("Strikethrough notes", "first\n\nsecond", None),
            ("Bulleted list", "one\ntwo", Some(0..3)),
            ("Numbered list", "one\ntwo", Some(0..3)),
        ] {
            let mut app = ThinodaApp::headless();
            app.mode = mode;
            app.format_tab = thinoda_app::app::FormatTab::Data;
            let mut h = Harness::builder()
                .with_size(egui::vec2(1280.0, 1000.0))
                .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
            h.run_steps(4);
            h.get_by_role_and_label(Role::MultilineTextInput, t("Notes"))
                .focus();
            h.run_steps(2);
            h.get_by_role_and_label(Role::MultilineTextInput, t("Notes"))
                .type_text(source);
            h.run_steps(2);
            if let Some(selection) = selection {
                let focused = h.ctx.memory(|memory| memory.focused()).unwrap();
                let mut state = egui::TextEdit::load_state(&h.ctx, focused).unwrap();
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::two(
                        egui::text::CCursor::new(selection.start),
                        egui::text::CCursor::new(selection.end),
                    )));
                state.store(&h.ctx, focused);
            } else {
                h.key_press_modifiers(Modifiers::CTRL | Modifiers::COMMAND, Key::A);
            }
            h.run_steps(2);
            h.get_by_role_and_label(Role::Button, t(label))
                .scroll_to_me();
            h.run_steps(8);
            h.get_by_role_and_label(Role::Button, t(label)).click();
            h.run_steps(4);
            let editor = h.get_by_role_and_label(Role::MultilineTextInput, t("Notes"));
            assert!(editor.is_focused());
            assert_eq!(editor.value().as_deref(), Some(source));
            assert!(h.state().doc().unwrap().is_dirty(), "{mode:?} {label}");
            assert!(
                h.state()
                    .doc()
                    .unwrap()
                    .doc
                    .sheet()
                    .root_topic()
                    .notes
                    .is_none()
            );
            let focused = h.ctx.memory(|memory| memory.focused()).unwrap();
            h.ctx.memory_mut(|memory| memory.surrender_focus(focused));
            h.run_steps(4);
            let note = h
                .state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .root_topic()
                .notes
                .as_ref()
                .unwrap();
            assert_eq!(&**note, source, "{mode:?} {label}");
            assert!(!note.is_markdown());
            assert!(h.query_by_label(t(warning)).is_some());
            let ctx = h.ctx.clone();
            h.state_mut().exec(&ctx, Cmd::Canvas(Action::Undo));
            assert!(
                h.state()
                    .doc()
                    .unwrap()
                    .doc
                    .sheet()
                    .root_topic()
                    .notes
                    .is_none()
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
}

#[test]
fn rejected_notes_formatting_keeps_draft_for_recovery_and_native_save() {
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        for (label, source, selection) in [
            ("Bold notes", "first\n\nsecond", None),
            ("Italic notes", "first\n\nsecond", None),
            ("Strikethrough notes", "first\n\nsecond", None),
            ("Bulleted list", "one\ntwo", Some(0..3)),
            ("Numbered list", "one\ntwo", Some(0..3)),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("draft.thinoda");
            let mut app = ThinodaApp::headless();
            app.mode = mode;
            app.format_tab = thinoda_app::app::FormatTab::Data;
            app.doc_mut().unwrap().save_to(&path, false).unwrap();
            let mut h = Harness::builder()
                .with_size(egui::vec2(1280.0, 1000.0))
                .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
            h.run_steps(4);
            h.get_by_role_and_label(Role::MultilineTextInput, t("Notes"))
                .focus();
            h.run_steps(2);
            h.get_by_role_and_label(Role::MultilineTextInput, t("Notes"))
                .type_text(source);
            h.run_steps(2);
            if let Some(selection) = selection {
                let focused = h.ctx.memory(|memory| memory.focused()).unwrap();
                let mut state = egui::TextEdit::load_state(&h.ctx, focused).unwrap();
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::two(
                        egui::text::CCursor::new(selection.start),
                        egui::text::CCursor::new(selection.end),
                    )));
                state.store(&h.ctx, focused);
            } else {
                h.key_press_modifiers(Modifiers::CTRL | Modifiers::COMMAND, Key::A);
            }
            h.run_steps(2);
            h.get_by_role_and_label(Role::Button, t(label))
                .scroll_to_me();
            h.run_steps(8);
            h.get_by_role_and_label(Role::Button, t(label)).click();
            h.run_steps(4);
            assert!(h.state().doc().unwrap().is_dirty(), "{mode:?} {label}");
            let paths = Paths::from_root(dir.path().join("profile"));
            let recovery = RecoveryManager::start(&paths).unwrap();
            h.state_mut().doc_mut().unwrap().snapshot(&recovery);
            recovery.flush().unwrap();
            recovery.suspend().unwrap();
            let orphans = RecoveryManager::find_orphans(&paths);
            let recovered = orphans[0].docs[0].load().unwrap();
            let note = recovered.active().root_topic().notes.as_ref().unwrap();
            assert_eq!(&**note, source, "{mode:?} {label}");
            assert!(!note.is_markdown());
            assert!(
                h.state()
                    .doc()
                    .unwrap()
                    .doc
                    .sheet()
                    .root_topic()
                    .notes
                    .is_none()
            );
            drop(orphans);
            recovery.resume().unwrap();
            recovery.finish().unwrap();
            h.key_press_modifiers(Modifiers::CTRL | Modifiers::COMMAND, Key::S);
            h.run_steps(4);
            let saved = OpenDoc::open(&path).unwrap();
            let note = saved.doc.sheet().root_topic().notes.as_ref().unwrap();
            assert_eq!(&**note, source, "{mode:?} {label}");
            assert!(!note.is_markdown());
        }
    }
}

#[test]
fn preview_links_are_inert_and_excessive_nesting_keeps_the_original_note() {
    let mut app = ThinodaApp::headless();
    app.format_tab = thinoda_app::app::FormatTab::Data;
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
