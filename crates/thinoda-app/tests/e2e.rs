//! End-to-end tests: drive the real application UI headlessly with egui_kittest,
//! exercise file round-trips, crash recovery and the command-line interface.

use std::path::Path;
use std::sync::Arc;

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use thinoda_app::app::{
    CloseIntent, Cmd, FocusField, FormatTab, Services, SidebarFamily, ThinodaApp,
};
use thinoda_core::Workbook;
use thinoda_io::Format;
use thinoda_platform::paths::Paths;
use thinoda_platform::recovery::RecoveryManager;
use thinoda_platform::settings::{Language, Settings, ThemeMode as Theme};

mod support;
use support::menu_action;

fn harness(app: ThinodaApp) -> Harness<'static, ThinodaApp> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    // Symbol coverage depends on installed system fonts; render tofu instead of panicking.
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    h.run_steps(3);
    h
}

fn root_children(app: &ThinodaApp) -> usize {
    let s = app.doc().unwrap().doc.sheet();
    s.children(s.root).len()
}

fn english() -> Settings {
    Settings {
        language: Language::English,
        ..Settings::default()
    }
}

fn rename_root(app: &mut ThinodaApp, title: &str) {
    let d = app.doc_mut().unwrap();
    let root = d.doc.sheet().root;
    d.doc
        .edit_sheet("Rename", |s| s.set_title(root, title))
        .unwrap();
}

fn root_title(wb: &Workbook) -> String {
    let s = &wb.sheets[0];
    s.topic(s.root).unwrap().title.clone()
}

fn rich_sheet() -> thinoda_core::Sheet {
    let mut sheet = Workbook::default().sheets.remove(0);
    sheet.title = "Rich sheet".into();
    let children = sheet.children(sheet.root).to_vec();
    let summary = sheet.add_summary(&children[..2], "Summary").unwrap();
    let child = sheet.add_child(summary, None, "Summary child");
    sheet.add_summary(&[child], "Nested Summary").unwrap();
    sheet.add_boundary(&children[..2]).unwrap();
    let floating = sheet.add_floating("Floating", thinoda_core::Vec2::new(100.0, 200.0));
    sheet.add_relationship(child, floating).unwrap();
    sheet.topic_mut(children[0]).unwrap().notes = Some("Keep notes".into());
    sheet.topic_mut(children[1]).unwrap().style.italic = Some(true);
    sheet
}

#[test]
fn sheet_clipboard_crosses_app_instances_and_pastes_once_with_undo() {
    let mut source = ThinodaApp::headless();
    let ctx = egui::Context::default();
    let sheet = rich_sheet();
    source.doc_mut().unwrap().doc.add_sheet(sheet.clone());
    let source_state = source.doc().unwrap().doc.state_id();
    source.exec(&ctx, Cmd::CopySheet);
    assert_eq!(source.doc().unwrap().doc.state_id(), source_state);
    let payload = ctx
        .output(|output| {
            output.commands.iter().find_map(|command| match command {
                egui::OutputCommand::CopyText(text) => Some(text.clone()),
                _ => None,
            })
        })
        .unwrap();
    let mut target = harness(ThinodaApp::headless());
    let before = target.state().doc().unwrap().doc.workbook().clone();
    let ctx = target.ctx.clone();
    target.state_mut().exec(&ctx, Cmd::PasteSheet);
    target.event(egui::Event::Paste(payload.clone()));
    target.run_steps(3);
    let d = target.state().doc().unwrap();
    assert_eq!(d.doc.workbook().sheets.len(), 2);
    assert_eq!(d.doc.sheet(), &sheet);
    assert!(target.state().sheet_paste_request.is_none());
    target
        .state_mut()
        .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
    assert_eq!(target.state().doc().unwrap().doc.workbook(), &before);
    target
        .state_mut()
        .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Redo));
    assert_eq!(target.state().doc().unwrap().doc.sheet(), &sheet);
    // A normal paste event on the canvas also recognizes our versioned sheet payload.
    target.event(egui::Event::Paste(payload));
    target.run_steps(3);
    assert_eq!(target.state().doc().unwrap().doc.workbook().sheets.len(), 3);
    assert_eq!(target.state().doc().unwrap().doc.sheet(), &sheet);
}

#[test]
fn sheet_context_menu_exposes_complete_clipboard_and_separate_save_actions() {
    let mut h = harness(ThinodaApp::headless());
    let title = h.state().doc().unwrap().doc.sheet().title.clone();
    let point = h.get_by_label(&title).rect().center();
    right_click_at(&mut h, point);
    h.get_by_role_and_label(Role::Button, "Paste Sheet");
    h.get_by_role_and_label(Role::Button, "Save Sheet As…");
    menu_action(&h, "New Sheet");
    h.get_by_role_and_label(Role::Button, "Copy Sheet").click();
    h.run_steps(3);
    h.get_by_label("Copied sheet to clipboard");
    assert_eq!(h.state().doc().unwrap().doc.workbook().sheets.len(), 1);
}

#[test]
fn new_sheet_from_topic_copies_owned_annotations_without_changing_the_source() {
    let mut app = ThinodaApp::headless();
    let ctx = egui::Context::default();
    let mut sheet = rich_sheet();
    let summary = sheet.summaries[0].topic;
    let child = sheet.children(summary)[0];
    let nested = sheet.summaries[1].topic;
    sheet.add_boundary(&[child]).unwrap();
    sheet.add_relationship(child, nested).unwrap();
    let expected = sheet.copy_branch_as_sheet(summary).unwrap();
    assert_eq!(expected.root, summary);
    assert!(expected.floating.is_empty());
    assert_eq!(expected.summaries.len(), 1);
    assert_eq!(expected.boundaries.len(), 1);
    assert_eq!(expected.relationships.len(), 1);
    assert_eq!(expected.topic_count(), 3);
    app.doc_mut().unwrap().doc.add_sheet(sheet);
    app.doc_mut().unwrap().view.select_only(summary);
    let before = app.doc().unwrap().doc.workbook().clone();
    app.exec(&ctx, Cmd::NewSheetFromTopic);
    assert_eq!(app.doc().unwrap().doc.workbook().sheets[..2], before.sheets);
    assert_eq!(app.doc().unwrap().doc.sheet(), &expected);
    app.exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    app.exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Redo));
    assert_eq!(app.doc().unwrap().doc.sheet(), &expected);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("branch.thinoda");
    assert!(app.save_sheet_to(&path));
    assert_eq!(thinoda_io::import_file(&path).unwrap().sheets[0], expected);
    let d = app.doc_mut().unwrap();
    d.view.selection = vec![expected.root, child];
    assert!(!app.command_available(&Cmd::NewSheetFromTopic));
    let before_invalid = app.doc().unwrap().doc.workbook().clone();
    app.exec(&ctx, Cmd::NewSheetFromTopic);
    assert_eq!(app.doc().unwrap().doc.workbook(), &before_invalid);
}

#[test]
fn sheet_paste_rejects_invalid_data_document_switches_and_timeouts() {
    let mut h = harness(ThinodaApp::headless());
    let ctx = h.ctx.clone();
    let before = h.state().doc().unwrap().doc.workbook().clone();
    for text in ["ordinary text", "THINODA-SHEET-2\n{}", "THINODA-SHEET-1\n{"] {
        h.state_mut().exec(&ctx, Cmd::PasteSheet);
        h.event(egui::Event::Paste(text.into()));
        h.run_steps(3);
        assert!(h.state().sheet_paste_request.is_none());
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    }
    h.state_mut().exec(&ctx, Cmd::PasteSheet);
    h.state_mut().exec(&ctx, Cmd::New);
    h.event(egui::Event::Paste(
        thinoda_core::clipboard::encode_sheet(&rich_sheet()).unwrap(),
    ));
    h.run_steps(3);
    assert!(h.state().sheet_paste_request.is_none());
    assert_eq!(h.state().doc().unwrap().doc.workbook().sheets.len(), 1);
    let key = h.state().doc().unwrap().key.clone();
    h.state_mut().sheet_paste_request = Some((
        std::time::Instant::now() - std::time::Duration::from_secs(3),
        key,
    ));
    h.run_steps(3);
    assert!(h.state().sheet_paste_request.is_none());
    assert_eq!(h.state().doc().unwrap().doc.workbook().sheets.len(), 1);
    assert!(h.state().discard_late_sheet_paste);
    h.event(egui::Event::Paste(
        thinoda_core::clipboard::encode_sheet(&rich_sheet()).unwrap(),
    ));
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.workbook().sheets.len(), 1);
    assert!(!h.state().discard_late_sheet_paste);
    h.state_mut().exec(&ctx, Cmd::PasteSheet);
    h.state_mut().exec(&ctx, Cmd::New);
    h.run_steps(3);
    assert!(h.state().discard_late_sheet_paste);
    h.event(egui::Event::Paste(
        thinoda_core::clipboard::encode_sheet(&rich_sheet()).unwrap(),
    ));
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.workbook().sheets.len(), 1);
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::StartEdit));
    h.run_steps(3);
    h.event(egui::Event::Paste(
        thinoda_core::clipboard::encode_sheet(&rich_sheet()).unwrap(),
    ));
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.workbook().sheets.len(), 1);
    assert!(h.state().sheet_paste_request.is_none());
}

#[test]
fn separate_sheet_save_roundtrips_rich_content_without_retargeting_or_marking_saved() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("original.thinoda");
    let mut app = ThinodaApp::headless();
    app.doc_mut().unwrap().doc.add_sheet(rich_sheet());
    app.doc_mut()
        .unwrap()
        .doc
        .edit_book("Metadata", |book| {
            book.metadata.creator = "Sheet author".into();
            book.metadata.description = Some("Workbook description".into());
            Ok(())
        })
        .unwrap();
    app.doc_mut().unwrap().save_to(&original, false).unwrap();
    let original_bytes = std::fs::read(&original).unwrap();
    let d = app.doc_mut().unwrap();
    let root = d.doc.sheet().root;
    d.doc
        .edit_sheet("Rename active root", |s| {
            s.set_title(root, "Unsaved change")
        })
        .unwrap();
    let state = app.doc().unwrap().doc.state_id();
    let before = app.doc().unwrap().doc.workbook().clone();
    for extension in ["thinoda", "xmind"] {
        let path = dir.path().join(format!("sheet.{extension}"));
        assert!(app.save_sheet_to(&path));
        let imported = thinoda_io::import_file(&path).unwrap();
        assert_eq!(imported.sheets.len(), 1);
        assert_eq!(imported.active_sheet, 0);
        assert_eq!(imported.metadata, before.metadata);
        assert_eq!(imported.sheets[0], *before.active());
        assert_eq!(app.doc().unwrap().path.as_ref(), Some(&original));
        assert!(app.doc().unwrap().is_dirty());
        assert_eq!(app.doc().unwrap().doc.state_id(), state);
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    }
    for path in [
        original.clone(),
        dir.path().join(".").join("original.thinoda"),
        dir.path().join("sheet.md"),
    ] {
        assert!(!app.save_sheet_to(&path));
    }
    assert!(!dir.path().join("sheet.md").exists());
    assert_eq!(std::fs::read(&original).unwrap(), original_bytes);
    assert!(!app.save_sheet_to(&original.join("sheet.thinoda")));
    assert_eq!(app.doc().unwrap().doc.state_id(), state);
}

#[test]
fn masked_copy_preview_is_cancelable_and_exports_only_derived_content() {
    let mut app = ThinodaApp::headless();
    app.doc_mut().unwrap().doc.add_sheet(rich_sheet());
    app.doc_mut()
        .unwrap()
        .doc
        .edit_book("Sensitive data", |book| {
            book.metadata.description = Some("CONFIDENTIAL description".into());
            for sheet in &mut book.sheets {
                sheet.title = "CONFIDENTIAL sheet".into();
                for topic in sheet.topics.values_mut() {
                    topic.title = "CONFIDENTIAL title".into();
                    topic.notes = Some("CONFIDENTIAL notes".into());
                    topic.hyperlink = Some("https://example.test/CONFIDENTIAL".into());
                    topic.image = Some("CONFIDENTIAL/image.png".into());
                    topic.labels = vec!["CONFIDENTIAL label".into()];
                    topic.markers = vec!["CONFIDENTIAL marker".into()];
                }
            }
            Ok(())
        })
        .unwrap();
    let source = app.doc().unwrap().doc.workbook().clone();
    let revision = app.doc().unwrap().doc.state_id();
    let mut h = harness(app);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::MaskedCopy);
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Cancel").click();
    h.run_steps(3);
    assert_eq!(h.state().docs.len(), 1);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &source);
    h.state_mut().exec(&ctx, Cmd::MaskedCopy);
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Create Preview")
        .click();
    h.run_steps(3);
    assert_eq!(h.state().docs.len(), 2);
    let copy = h.state().doc().unwrap();
    assert!(copy.is_dirty());
    assert!(copy.path.is_none());
    assert_eq!(copy.doc.workbook(), &source.masked_copy());
    assert_eq!(h.state().docs[0].doc.workbook(), &source);
    assert_eq!(h.state().docs[0].doc.state_id(), revision);
    let dir = tempfile::tempdir().unwrap();
    for extension in ["thinoda", "xmind"] {
        let path = dir.path().join(format!("masked.{extension}"));
        h.state_mut()
            .doc_mut()
            .unwrap()
            .save_to(&path, false)
            .unwrap();
        let reopened = thinoda_io::import_file(&path).unwrap();
        assert_eq!(reopened, source.masked_copy());
    }
}

#[test]
fn merge_map_files_is_atomic_rich_and_preserves_sources_and_destination_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = ThinodaApp::headless();
    let source_book = Workbook::new(rich_sheet());
    let paths: Vec<_> = ["source.thinoda", "source.xmind"]
        .iter()
        .map(|name| dir.path().join(name))
        .collect();
    for path in &paths {
        thinoda_io::export_file(
            &source_book,
            path,
            Format::from_path(path).unwrap(),
            &Default::default(),
        )
        .unwrap();
    }
    let originals: Vec<_> = paths
        .iter()
        .map(|path| std::fs::read(path).unwrap())
        .collect();
    let before = app.doc().unwrap().doc.workbook().clone();
    let revision = app.doc().unwrap().doc.state_id();
    let mut bad_paths = paths.clone();
    bad_paths.push(dir.path().join("missing.xmind"));
    assert!(!app.merge_files(&bad_paths));
    assert_eq!(app.doc().unwrap().doc.state_id(), revision);
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    assert!(!app.merge_files(&[dir.path().join("text.txt")]));
    assert!(!app.merge_files(&[]));
    assert!(app.merge_files(&paths));
    let merged = app.doc().unwrap().doc.workbook().clone();
    assert_eq!(merged.sheets.len(), 3);
    assert_eq!(merged.active_sheet, 1);
    assert_eq!(merged.sheets[0], before.sheets[0]);
    assert_eq!(merged.sheets[1], source_book.sheets[0]);
    assert_eq!(merged.sheets[2], source_book.sheets[0]);
    assert_eq!(merged.metadata, before.metadata);
    for (path, bytes) in paths.iter().zip(originals) {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    assert!(app.doc_mut().unwrap().doc.undo());
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    assert!(app.doc_mut().unwrap().doc.redo());
    assert_eq!(app.doc().unwrap().doc.workbook(), &merged);
}

#[test]
fn arrange_menu_aligns_floating_topics_and_persists_with_single_undo() {
    let mut app = ThinodaApp::headless();
    app.settings.show_format_panel = false;
    let ids = app
        .doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Floating", |s| {
            Ok(vec![
                s.add_floating("First", thinoda_core::Vec2::new(100.0, 300.0)),
                s.add_floating("Second", thinoda_core::Vec2::new(400.0, 600.0)),
            ])
        })
        .unwrap();
    app.doc_mut().unwrap().view.selection = ids.clone();
    let before = app.doc().unwrap().doc.sheet().clone();
    let mut h = harness(app);
    h.get_by_label(thinoda_ui::icons::MENU).click();
    h.run_steps(2);
    h.get_by_label("Edit").click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Arrange").click();
    h.run_steps(2);
    assert!(
        h.get_by_role_and_label(Role::Button, "Distribute Horizontal Centers")
            .accesskit_node()
            .is_disabled()
    );
    h.get_by_role_and_label(Role::Button, "Align Top").click();
    h.run_steps(3);
    let aligned = h.state().doc().unwrap().doc.sheet().clone();
    assert_ne!(aligned, before);
    let ctx = h.ctx.clone();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("aligned.thinoda");
    assert!(h.state_mut().save_sheet_to(&path));
    assert_eq!(thinoda_io::import_file(&path).unwrap().sheets[0], aligned);
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
    assert_eq!(h.state().doc().unwrap().doc.sheet(), &before);
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Redo));
    assert_eq!(h.state().doc().unwrap().doc.sheet(), &aligned);
}

#[test]
fn select_context_submenu_preserves_content_and_uses_same_availability() {
    let mut h = harness(ThinodaApp::headless());
    let d = h.state().doc().unwrap();
    let point = d.view.selected_topic_rect(&d.doc).unwrap().center();
    let before = d.doc.workbook().clone();
    let state = d.doc.state_id();
    right_click_at(&mut h, point);
    h.get_by_role_and_label(Role::Button, "Select").click();
    h.run_steps(2);
    assert!(
        h.get_by_role_and_label(Role::Button, "Select Siblings")
            .accesskit_node()
            .is_disabled()
    );
    h.get_by_role_and_label(Role::Button, "Select Descendants")
        .click();
    h.run_steps(3);
    let d = h.state().doc().unwrap();
    assert_eq!(d.view.selection, d.doc.sheet().children(d.doc.sheet().root));
    assert_eq!(d.doc.workbook(), &before);
    assert_eq!(d.doc.state_id(), state);
}

#[test]
fn style_clipboard_is_session_wide_and_multitopic_edits_undo_atomically() {
    let mut app = ThinodaApp::headless();
    let ctx = egui::Context::default();
    let source = app.doc().unwrap().doc.sheet().root;
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Source style", |s| {
            s.update_style(&[source], |style| {
                style.fill = thinoda_core::Color::from_hex("#123456");
                style.font_size = Some(24.0);
                style.italic = Some(true);
                style.line_width = Some(3.0);
                style.width = Some(150.0);
            })
        })
        .unwrap();
    let source_style = app
        .doc()
        .unwrap()
        .doc
        .sheet()
        .topic(source)
        .unwrap()
        .style
        .clone();
    let source_revision = app.doc().unwrap().doc.state_id();
    app.exec(&ctx, Cmd::CopyStyle);
    assert_eq!(app.doc().unwrap().doc.state_id(), source_revision);
    app.exec(&ctx, Cmd::New);
    let topics = app
        .doc()
        .unwrap()
        .doc
        .sheet()
        .children(app.doc().unwrap().doc.sheet().root)[..2]
        .to_vec();
    app.doc_mut().unwrap().view.selection = topics.clone();
    let before = app.doc().unwrap().doc.sheet().clone();
    app.exec(&ctx, Cmd::PasteStyle);
    let pasted = app.doc().unwrap().doc.sheet().clone();
    for topic in &topics {
        assert_eq!(pasted.topic(*topic).unwrap().style, source_style);
        assert_eq!(
            pasted.topic(*topic).unwrap().title,
            before.topic(*topic).unwrap().title
        );
    }
    assert!(!app.command_available(&Cmd::CopyStyle));
    app.exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
    assert_eq!(app.doc().unwrap().doc.sheet(), &before);
    app.exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Redo));
    assert_eq!(app.doc().unwrap().doc.sheet(), &pasted);
    app.exec(&ctx, Cmd::ResetStyle);
    for topic in &topics {
        assert!(
            app.doc()
                .unwrap()
                .doc
                .sheet()
                .topic(*topic)
                .unwrap()
                .style
                .is_default()
        );
    }
    app.exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
    assert_eq!(app.doc().unwrap().doc.sheet(), &pasted);
    // An empty override style is a valid clipboard, not a missing clipboard.
    app.doc_mut().unwrap().view.select_only(before.root);
    app.exec(&ctx, Cmd::CopyStyle);
    assert!(app.style_clipboard.as_ref().unwrap().is_default());
    assert!(app.command_available(&Cmd::PasteStyle));
}

#[test]
fn style_shortcuts_and_context_actions_share_availability() {
    let mut h = harness(ThinodaApp::headless());
    let d = h.state().doc().unwrap();
    let root_rect = d.view.selected_topic_rect(&d.doc).unwrap();
    right_click_at(&mut h, root_rect.center());
    assert!(
        h.get_by_role_and_label(Role::Button, "Paste Style")
            .accesskit_node()
            .is_disabled()
    );
    assert!(
        !h.get_by_role_and_label(Role::Button, "Copy Style")
            .accesskit_node()
            .is_disabled()
    );
    h.get_by_role_and_label(Role::Button, "Copy Style").click();
    h.run_steps(3);
    assert!(h.state().style_clipboard.is_some());
    let first = h
        .state()
        .doc()
        .unwrap()
        .doc
        .sheet()
        .children(h.state().doc().unwrap().doc.sheet().root)[0];
    h.state_mut().doc_mut().unwrap().view.select_only(first);
    h.key_press_modifiers(
        egui::Modifiers::COMMAND.plus(egui::Modifiers::ALT),
        egui::Key::V,
    );
    h.run_steps(3);
    assert!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .topic(first)
            .unwrap()
            .style
            .is_default()
    );
    let d = h.state().doc().unwrap();
    let rect = d.view.selected_topic_rect(&d.doc).unwrap();
    right_click_at(&mut h, rect.center());
    h.get_by_role_and_label(Role::Button, "Reset Style").click();
    h.run_steps(3);
    h.state_mut().doc_mut().unwrap().view.selection.clear();
    assert!(!h.state().command_available(&Cmd::ResetStyle));
    assert!(!h.state().command_available(&Cmd::CopyStyle));
    assert!(!h.state().command_available(&Cmd::PasteStyle));
}

#[test]
fn sheet_shortcuts_commit_title_edits_wrap_and_preserve_last_sheet() {
    let mut h = harness(ThinodaApp::headless());
    let ctx = h.ctx.clone();
    assert!(!h.state().command_available(&Cmd::NextSheet));
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::StartEdit));
    h.run_steps(3);
    h.get_by_label("Topic title")
        .type_text("Committed before new sheet");
    h.run_steps(2);
    h.state_mut().exec(&ctx, Cmd::NewSheet);
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.active_index(), 1);
    assert_eq!(
        root_title(h.state().doc().unwrap().doc.workbook()),
        "Committed before new sheet"
    );
    assert!(h.state().doc().unwrap().view.selection.is_empty());
    let revision = h.state().doc().unwrap().doc.state_id();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::PageDown);
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.active_index(), 0);
    assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::PageUp);
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.active_index(), 1);
    h.state_mut().exec(&ctx, Cmd::DeleteSheet);
    h.state_mut().exec(&ctx, Cmd::DeleteSheet);
    assert_eq!(h.state().doc().unwrap().doc.workbook().sheets.len(), 1);
    assert!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .contains(h.state().doc().unwrap().doc.sheet().root)
    );
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
    assert_eq!(h.state().doc().unwrap().doc.workbook().sheets.len(), 2);
}

#[test]
fn style_shortcuts_do_not_intercept_topic_text_input() {
    let mut h = harness(ThinodaApp::headless());
    let ctx = h.ctx.clone();
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::StartEdit));
    h.run_steps(3);
    h.key_press_modifiers(
        egui::Modifiers::COMMAND.plus(egui::Modifiers::ALT),
        egui::Key::C,
    );
    h.run_steps(3);
    assert!(h.state().style_clipboard.is_none());
    h.get_by_label("Topic title").type_text("Still editing");
    h.run_steps(3);
    h.state_mut().exec(&ctx, Cmd::DuplicateSheet);
    assert_eq!(h.state().doc().unwrap().doc.workbook().sheets.len(), 2);
    for sheet in &h.state().doc().unwrap().doc.workbook().sheets {
        assert_eq!(sheet.root_topic().title, "Still editing");
    }
}

#[test]
fn indent_outdent_context_commands_are_single_undo_steps() {
    let mut h = harness(ThinodaApp::headless());
    let children = h
        .state()
        .doc()
        .unwrap()
        .doc
        .sheet()
        .children(h.state().doc().unwrap().doc.sheet().root)
        .to_vec();
    let ctx = h.ctx.clone();
    h.state_mut()
        .doc_mut()
        .unwrap()
        .view
        .select_only(children[1]);
    h.run_steps(3);
    assert!(
        h.state()
            .command_available(&Cmd::Canvas(thinoda_canvas::Action::Indent))
    );
    assert!(
        !h.state()
            .command_available(&Cmd::Canvas(thinoda_canvas::Action::Outdent))
    );
    let d = h.state().doc().unwrap();
    let point = d.view.selected_topic_rect(&d.doc).unwrap().center();
    right_click_at(&mut h, point);
    h.get_by_role_and_label(Role::Button, "Indent").click();
    h.run_steps(3);
    assert_eq!(
        h.state().doc().unwrap().doc.sheet().parent(children[1]),
        Some(children[0])
    );
    assert!(
        h.state()
            .command_available(&Cmd::Canvas(thinoda_canvas::Action::Outdent))
    );
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Outdent));
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .children(h.state().doc().unwrap().doc.sheet().root),
        children
    );
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
    assert_eq!(
        h.state().doc().unwrap().doc.sheet().parent(children[1]),
        Some(children[0])
    );
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .children(h.state().doc().unwrap().doc.sheet().root),
        children
    );
}

#[test]
fn about_dialog_shows_author_and_contact() {
    let mut h = harness(ThinodaApp::headless());
    h.state_mut().dialogs.about = true;
    h.run_steps(3);
    h.get_by_role_and_label(Role::Image, "Thinoda logo");
    h.get_by_label("Give your thoughts shape.");
    h.get_by_label("Written by Axel Wolters");
    h.get_by_role_and_label(Role::Link, "Contact: axelwolters@hotmail.com");
}

#[test]
fn editor_preferences_independently_control_toolbar_and_add_handles() {
    let mut h = harness(ThinodaApp::headless());
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::Settings);
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Editor").click();
    h.run_steps(3);
    let toggle_rect = h
        .get_by_role_and_label(Role::Switch, "Show topic toolbar")
        .rect();
    let window_rect = h
        .ctx
        .memory(|memory| memory.area_rect(egui::Id::unique("thinoda_settings")))
        .unwrap();
    assert!(
        window_rect.contains_rect(toggle_rect),
        "toggle {toggle_rect:?}; window {window_rect:?}"
    );
    h.get_by_role_and_label(Role::Switch, "Show topic toolbar")
        .click();
    h.run_steps(3);
    assert!(!h.state().settings.show_topic_toolbar);
    assert!(h.state().settings.show_topic_add_buttons);
    h.state_mut().dialogs.settings = false;
    h.run_steps(3);
    assert!(h.query_by_label("Edit notes").is_none());
    h.get_by_label("Add child topic");
    h.state_mut().settings.show_topic_toolbar = true;
    h.state_mut().settings.show_topic_add_buttons = false;
    h.run_steps(3);
    h.get_by_label("Edit notes");
    assert!(h.query_by_label("Add child topic").is_none());
    assert!(h.query_by_label("Add sibling topic").is_none());
}

#[test]
fn blank_map_defaults_are_opt_in_and_do_not_override_named_templates() {
    let mut app = ThinodaApp::headless();
    let ctx = egui::Context::default();
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Map defaults", |s| {
            s.structure = thinoda_core::Structure::LogicLeft;
            s.theme.theme_id = "ocean".into();
            Ok(())
        })
        .unwrap();
    app.exec(&ctx, Cmd::New);
    assert_eq!(
        app.doc().unwrap().doc.sheet().structure,
        thinoda_core::Structure::MapClockwise
    );
    assert_eq!(app.doc().unwrap().doc.sheet().theme.theme_id, "clarity");
    app.active = 0;
    app.settings.reuse_last_structure = true;
    app.settings.reuse_last_theme = true;
    app.settings.initial_canvas_zoom = 0.8;
    app.exec(&ctx, Cmd::New);
    assert_eq!(
        app.doc().unwrap().doc.sheet().structure,
        thinoda_core::Structure::LogicLeft
    );
    assert_eq!(app.doc().unwrap().doc.sheet().theme.theme_id, "ocean");
    let mut h = harness(app);
    assert_eq!(h.state().doc().unwrap().view.zoom_percent(), 80);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::NewFromTemplate("org-chart"));
    h.run_steps(3);
    assert_eq!(
        h.state().doc().unwrap().doc.sheet().structure,
        thinoda_core::Structure::OrgDown
    );
    assert_eq!(
        h.state().doc().unwrap().doc.sheet().theme.theme_id,
        "clarity"
    );
}

#[test]
fn zoom_menu_has_presets_current_value_and_fit_without_changing_ui_scale() {
    let mut h = harness(ThinodaApp::headless());
    h.state_mut().doc_mut().unwrap().view.set_zoom(1.12);
    h.run_steps(3);
    h.get_by_label("Zoom").click();
    h.run_steps(3);
    assert_eq!(h.query_all_by_label("112%").count(), 1);
    for preset in [
        "500%", "400%", "300%", "200%", "150%", "120%", "100%", "80%", "50%", "20%", "10%",
    ] {
        h.get_by_role_and_label(Role::Button, preset);
    }
    h.get_by_role_and_label(Role::Button, "150%").click();
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().view.zoom_percent(), 150);
    assert_eq!(h.state().settings.ui_scale, 1.0);
    h.get_by_label("Zoom").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "500%").click();
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().view.zoom_percent(), 500);
    h.get_by_label("Zoom").click();
    h.run_steps(3);
    assert_eq!(h.query_all_by_label("500%").count(), 1);
    h.get_by_role_and_label(Role::Button, "10%").click();
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().view.zoom_percent(), 10);
    h.get_by_label("Zoom").click();
    h.run_steps(3);
    menu_action(&h, "Fit Map").click();
    h.run_steps(3);
    assert!(h.state().doc().unwrap().view.zoom > 0.0);
}

#[test]
fn statistics_popup_is_live_sheet_scoped_and_counts_unicode_titles() {
    let mut app = ThinodaApp::headless();
    let ctx = egui::Context::default();
    app.exec(&ctx, Cmd::NewSheet);
    app.doc_mut()
        .unwrap()
        .doc
        .edit_book("Statistics sheet", |wb| {
            wb.sheets[1] = thinoda_core::Sheet::new("Counts", "Root");
            Ok(())
        })
        .unwrap();
    let mut h = harness(app);
    h.state_mut()
        .doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Statistics", |s| {
            s.set_title(s.root, "Grüße 世界")?;
            s.topic_mut(s.root).unwrap().notes = Some("Not counted".into());
            s.add_floating("😀", thinoda_core::Vec2::new(100.0, 100.0));
            Ok(())
        })
        .unwrap();
    h.run_steps(3);
    h.get_by_label("Statistics").click();
    h.run_steps(3);
    h.get_by_label("Current sheet");
    h.get_by_label("Topics: 2");
    assert!(h.query_all_by_label("Words: 3").count() >= 1);
    h.get_by_label("Characters: 9");
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    rename_root(h.state_mut(), "A");
    h.get_by_label("Statistics").click();
    h.run_steps(3);
    h.get_by_label("Characters: 2");
    assert!(h.query_all_by_label("Words: 2").count() >= 1);
}

#[test]
fn initial_zoom_applies_to_opened_documents_but_not_existing_tabs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("zoom.thinoda");
    let mut app = ThinodaApp::headless();
    app.doc_mut().unwrap().save_to(&path, false).unwrap();
    app.settings.initial_canvas_zoom = 0.5;
    app.new_doc(thinoda_core::templates::instantiate("mind-map"));
    app.settings.initial_canvas_zoom = 0.8;
    assert_eq!(app.doc().unwrap().view.zoom_percent(), 50);
    // Opening an already open document preserves its current view.
    app.open_path(&path);
    assert_eq!(app.doc().unwrap().view.zoom_percent(), 100);
    // Reopening after closing the tab uses the new preference.
    app.docs.remove(0);
    app.active = 0;
    app.open_path(&path);
    let h = harness(app);
    assert_eq!(h.state().doc().unwrap().view.zoom_percent(), 80);
    assert_eq!(h.state().settings.ui_scale, 1.0);
}

#[test]
fn style_sidebar_tracks_selected_topic_and_undo_without_changing_properties() {
    use egui::accesskit::Toggled;
    use thinoda_core::{geometry::Color, model::TextAlign};
    let mut app = ThinodaApp::headless();
    let d = app.doc_mut().unwrap();
    let root = d.doc.sheet().root;
    let child = d.doc.sheet().children(root)[1];
    d.doc
        .edit_sheet("Custom style", |s| {
            s.update_style(&[child], |st| {
                st.fill = Color::from_hex("#123456");
                st.text_color = Color::from_hex("#ABCDEF");
                st.border_color = Color::from_hex("#234567");
                st.line_color = Color::from_hex("#345678");
                st.font_size = Some(27.0);
                st.italic = Some(true);
                st.align = Some(TextAlign::Right);
            })
        })
        .unwrap();
    d.view.select_only(child);
    let before = d.doc.state_id();
    let mut h = harness(app);
    let rect = {
        let d = h.state().doc().unwrap();
        d.view.selected_topic_rect(&d.doc).unwrap()
    };
    for hex in ["#123456", "#ABCDEF", "#234567", "#345678"] {
        let rect = h.get_by_role_and_label(Role::ColorWell, hex).rect();
        assert!(rect.min.x >= h.state().doc().unwrap().view.canvas_rect().right());
        assert!(rect.max.x <= 1280.0);
        assert_eq!(
            h.get_by_role_and_label(Role::ColorWell, hex)
                .accesskit_node()
                .toggled(),
            Some(Toggled::True)
        );
    }
    assert_eq!(
        h.get_by_label("Italic").accesskit_node().toggled(),
        Some(Toggled::True)
    );
    assert_eq!(
        h.get_by_label("Align Right").accesskit_node().toggled(),
        Some(Toggled::True)
    );
    assert_eq!(h.get_by_label("Font Size").value().as_deref(), Some("27.0"));
    assert_eq!(h.state().doc().unwrap().doc.state_id(), before);
    let ctx = h.ctx.clone();
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::SelectRoot));
    h.run_steps(3);
    assert_eq!(
        h.get_by_label("Bold").accesskit_node().toggled(),
        Some(Toggled::True)
    );
    assert_eq!(
        h.get_by_label("Italic").accesskit_node().toggled(),
        Some(Toggled::False)
    );
    assert!(
        h.query_by_role_and_label(Role::ColorWell, "#123456")
            .is_none()
    );
    assert_eq!(
        h.query_all_by_role_and_label(Role::Label, "None").count(),
        2
    );
    h.event(egui::Event::PointerMoved(rect.center()));
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: rect.center(),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().view.primary(), Some(child));
    assert_eq!(
        h.get_by_role_and_label(Role::ColorWell, "#123456")
            .accesskit_node()
            .toggled(),
        Some(Toggled::True)
    );
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
    h.run_steps(3);
    assert!(
        h.query_by_role_and_label(Role::ColorWell, "#123456")
            .is_none()
    );
    let d = h.state().doc().unwrap();
    let fill = thinoda_core::theme::resolve_style(d.doc.sheet(), child, d.view.dark)
        .fill
        .unwrap();
    let hex = format!("#{:02X}{:02X}{:02X}", fill.r, fill.g, fill.b);
    assert_eq!(
        h.query_all_by_role_and_label(Role::ColorWell, &hex)
            .next()
            .unwrap()
            .accesskit_node()
            .toggled(),
        Some(Toggled::True)
    );
}

#[test]
fn map_sidebar_shows_sheet_values_not_selected_topic_overrides() {
    use egui::accesskit::Toggled;
    let mut app = ThinodaApp::headless();
    let d = app.doc_mut().unwrap();
    d.doc
        .edit_sheet("Map settings", |s| {
            s.theme.background = thinoda_core::geometry::Color::from_hex("#123456");
            s.theme.rainbow = false;
            s.theme.balanced = true;
            Ok(())
        })
        .unwrap();
    let mut h = harness(app);
    h.get_by_role_and_label(Role::Tab, "Map").click();
    h.run_steps(3);
    h.get_by_label("Map settings apply to the entire sheet.");
    assert_eq!(
        h.get_by_role_and_label(Role::ColorWell, "#123456")
            .accesskit_node()
            .toggled(),
        Some(Toggled::True)
    );
    assert_eq!(
        h.get_by_role_and_label(Role::Switch, "Rainbow Branches")
            .accesskit_node()
            .toggled(),
        Some(Toggled::False)
    );
    assert_eq!(
        h.get_by_role_and_label(Role::Switch, "Balanced Map")
            .accesskit_node()
            .toggled(),
        Some(Toggled::True)
    );
    let ctx = h.ctx.clone();
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
    h.run_steps(3);
    assert!(
        h.query_by_role_and_label(Role::ColorWell, "#123456")
            .is_none()
    );
    let d = h.state().doc().unwrap();
    let ts = &d.doc.sheet().theme;
    let bg = thinoda_core::theme::Theme::builtin(&ts.theme_id).background(ts, d.view.dark);
    let hex = format!("#{:02X}{:02X}{:02X}", bg.r, bg.g, bg.b);
    assert_eq!(
        h.get_by_role_and_label(Role::ColorWell, &hex)
            .accesskit_node()
            .toggled(),
        Some(Toggled::True)
    );
}

#[test]
fn structure_gallery_preserves_rich_content_selection_and_undo_for_every_layout() {
    use thinoda_core::Structure;
    let mut app = ThinodaApp::headless();
    let sheet = rich_sheet();
    app.doc_mut().unwrap().doc.add_sheet(sheet.clone());
    let selected = sheet.children(sheet.root)[1];
    app.doc_mut().unwrap().view.select_only(selected);
    let before = app.doc().unwrap().doc.workbook().clone();
    let mut h = harness(app);
    let ctx = h.ctx.clone();
    h.get_by_role_and_label(Role::Tab, "Map").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Choose Structure: Mind Map")
        .click();
    h.run_steps(3);
    assert!(egui::Popup::is_any_open(&h.ctx));
    h.get_by_role_and_label(Role::DisclosureTriangle, "Mind maps")
        .click();
    h.run_steps(8);
    assert!(
        h.query_by_role_and_label(Role::Button, "Mind Map")
            .is_none()
    );
    assert!(egui::Popup::is_any_open(&h.ctx));
    h.get_by_role_and_label(Role::DisclosureTriangle, "Mind maps")
        .click();
    h.run_steps(8);
    h.get_by_role_and_label(Role::Button, "Mind Map");
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(!egui::Popup::is_any_open(&h.ctx));
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    let labels = [
        "Mind Map",
        "Mind Map (anticlockwise)",
        "Mind Map (one side)",
        "Logic Chart (right)",
        "Logic Chart (left)",
        "Brace Map (right)",
        "Brace Map (left)",
        "Org Chart (down)",
        "Org Chart (up)",
        "Tree Chart (right)",
        "Tree Chart (left)",
        "Timeline (horizontal)",
        "Timeline (vertical)",
        "Fishbone (right)",
        "Fishbone (left)",
        "Matrix",
        "Tree Table",
    ];
    for (structure, label) in Structure::ALL.into_iter().zip(labels) {
        h.get_by_role_and_label(Role::Button, "Choose Structure: Mind Map")
            .click();
        h.run_steps(3);
        h.get_by_role_and_label(Role::Button, label).scroll_to_me();
        h.run_steps(8);
        h.get_by_role_and_label(Role::Button, label).click();
        h.run_steps(4);
        assert!(!egui::Popup::is_any_open(&h.ctx), "{label}");
        let mut expected = before.clone();
        expected.active_mut().structure = structure;
        assert_eq!(
            h.state().doc().unwrap().doc.workbook(),
            &expected,
            "{label}"
        );
        assert_eq!(h.state().doc().unwrap().view.primary(), Some(selected));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("layout.thinoda");
        h.state_mut()
            .doc_mut()
            .unwrap()
            .save_to(&path, false)
            .unwrap();
        assert_eq!(thinoda_io::import_file(&path).unwrap(), expected);
        if structure != Structure::MapClockwise {
            h.state_mut()
                .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
            h.run_steps(3);
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &before, "{label}");
            h.state_mut()
                .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Redo));
            h.run_steps(3);
            assert_eq!(
                h.state().doc().unwrap().doc.workbook(),
                &expected,
                "{label}"
            );
            h.state_mut()
                .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
            h.run_steps(3);
        }
    }
}

#[test]
fn structure_gallery_is_bounded_and_keyboard_does_not_edit_the_map() {
    use egui::accesskit::Toggled;
    for (width, scale, dark) in [
        (480.0, 1.0, false),
        (640.0, 1.5, true),
        (1280.0, 1.0, false),
    ] {
        let mut app = ThinodaApp::headless();
        if width < 500.0 {
            app.mode = thinoda_app::app::CenterMode::Outliner;
        }
        app.settings.theme = if dark { Theme::Dark } else { Theme::Light };
        let before = app.doc().unwrap().doc.workbook().clone();
        let selection = app.doc().unwrap().view.selection.clone();
        let mut h = Harness::builder()
            .with_size(egui::vec2(width, 800.0))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        h.ctx
            .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
        h.ctx.set_pixels_per_point(scale);
        h.run_steps(4);
        h.get_by_role_and_label(Role::Tab, "Map").click();
        h.run_steps(4);
        h.get_by_role_and_label(Role::Button, "Choose Structure: Mind Map")
            .focus();
        h.run_steps(2);
        h.key_press(egui::Key::Enter);
        h.run_steps(4);
        assert!(egui::Popup::is_any_open(&h.ctx));
        assert_eq!(
            h.get_by_role_and_label(Role::Button, "Mind Map")
                .accesskit_node()
                .toggled(),
            Some(Toggled::True)
        );
        for label in [
            "Mind Map",
            "Mind Map (anticlockwise)",
            "Mind Map (one side)",
            "Logic Chart (right)",
            "Tree Table",
        ] {
            let rect = h.get_by_role_and_label(Role::Button, label).rect();
            assert!(
                rect.left() >= 0.0 && rect.right() <= h.ctx.content_rect().right(),
                "{width}/{scale}: {label} {rect:?}"
            );
        }
        h.key_press(egui::Key::Tab);
        h.key_press(egui::Key::ArrowDown);
        h.run_steps(3);
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        assert_eq!(h.state().doc().unwrap().view.selection, selection);
        h.get_by_role_and_label(Role::Button, "Tree Table").focus();
        h.get_by_role_and_label(Role::Button, "Tree Table")
            .scroll_to_me();
        h.run_steps(8);
        let card = h.get_by_role_and_label(Role::Button, "Tree Table").rect();
        assert!(
            card.top() >= 0.0 && card.bottom() <= h.ctx.content_rect().bottom(),
            "{card:?}"
        );
        assert!(
            h.get_by_role_and_label(Role::Button, "Tree Table")
                .is_focused()
        );
        h.key_press(egui::Key::Enter);
        h.run_steps(3);
        assert!(!egui::Popup::is_any_open(&h.ctx));
        let mut expected = before;
        expected.active_mut().structure = thinoda_core::Structure::TreeTable;
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &expected);
        assert_eq!(h.state().doc().unwrap().view.selection, selection);
    }
}

#[test]
fn map_panel_themes_fit_and_can_be_selected() {
    let mut h = harness(ThinodaApp::headless());
    h.get_by_role_and_label(Role::Tab, "Map").click();
    h.run_steps(4);
    assert_eq!(h.state().format_tab, FormatTab::Map);
    let map_tab = h.get_by_role_and_label(Role::Tab, "Map").rect();
    assert!(h.state().doc().unwrap().view.canvas_rect().max.x <= map_tab.min.x);
    for id in thinoda_core::theme::Theme::IDS {
        let theme = thinoda_core::theme::Theme::builtin(id);
        let rect = h.get_by_role_and_label(Role::Button, theme.name).rect();
        assert!(
            rect.min.x >= map_tab.min.x - map_tab.width(),
            "{} left sidebar",
            theme.name
        );
        assert!(rect.max.x <= 1280.0, "{} overflowed: {rect:?}", theme.name);
    }
    let ctx = h.ctx.clone();
    let before = h.state().doc().unwrap().doc.sheet().theme.clone();
    h.get_by_role_and_label(Role::Button, "Classic").click();
    h.run_steps(3);
    assert_eq!(
        h.state().doc().unwrap().doc.sheet().theme.theme_id,
        "classic"
    );
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.sheet().theme, before);
}

#[test]
fn map_tab_stays_reachable_after_scrolling_style_at_different_scales() {
    for (width, scale) in [(640.0, 1.0), (1280.0, 1.0), (1280.0, 1.5)] {
        let app = ThinodaApp::headless();
        let mut h = Harness::builder()
            .with_size(egui::vec2(width, 800.0))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        h.ctx
            .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
        h.ctx.set_pixels_per_point(scale);
        h.run_steps(4);
        let style = h.get_by_role_and_label(Role::Tab, "Style").rect();
        h.event(egui::Event::PointerMoved(
            style.center() + egui::vec2(0.0, 100.0),
        ));
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -1200.0),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(12);
        let tab = h.get_by_role_and_label(Role::Tab, "Map").rect();
        assert!(tab.min.y > 0.0 && tab.max.y < 800.0);
        h.get_by_role_and_label(Role::Tab, "Map").click();
        h.run_steps(4);
        let tab = h.get_by_role_and_label(Role::Tab, "Map").rect();
        let clarity = h.get_by_role_and_label(Role::Button, "Clarity").rect();
        assert!(clarity.min.y >= tab.max.y && clarity.max.y < 800.0);
        for id in thinoda_core::theme::Theme::IDS {
            let theme = thinoda_core::theme::Theme::builtin(id);
            let rect = h.get_by_role_and_label(Role::Button, theme.name).rect();
            assert!(
                rect.min.x >= tab.min.x - tab.width() && rect.max.x <= h.ctx.content_rect().right(),
                "{width}/{scale}: {} overflowed: {rect:?}",
                theme.name
            );
        }
    }
}

#[test]
fn sidebar_families_switch_hide_and_reopen_without_losing_selection() {
    let mut h = harness(ThinodaApp::headless());
    let selected = h.state().doc().unwrap().view.selection.clone();
    h.get_by_role_and_label(Role::Tab, "Map").click();
    h.run_steps(3);
    let ctx = h.ctx.clone();
    for iteration in 0..3 {
        if iteration == 1 {
            h.state_mut().exec(&ctx, Cmd::ToggleOutliner);
            h.run_steps(3);
        }
        let mode = h.state().mode;
        h.get_by_role_and_label(Role::Button, "Symbols").click();
        h.run_steps(3);
        assert_eq!(h.state().sidebar_family, SidebarFamily::Symbols);
        assert!(h.query_by_role_and_label(Role::Tab, "Map").is_none());
        h.get_by_role_and_label(Role::Button, "Priority 1");
        h.get_by_role_and_label(Role::Button, "Symbols").click();
        h.run_steps(3);
        assert!(!h.state().settings.show_format_panel);
        assert!(
            h.query_by_role_and_label(Role::Button, "Priority 1")
                .is_none()
        );
        h.get_by_role_and_label(Role::Button, "Symbols").click();
        h.run_steps(3);
        h.get_by_role_and_label(Role::Button, "Priority 1");
        h.get_by_role_and_label(Role::Button, "Format Panel")
            .click();
        h.run_steps(3);
        assert_eq!(h.state().sidebar_family, SidebarFamily::Format);
        assert!(h.state().settings.show_format_panel);
        assert_eq!(h.state().format_tab, FormatTab::Map);
        h.get_by_role_and_label(Role::Button, "Clarity");
        assert_eq!(h.state().doc().unwrap().view.selection, selected);
        assert_eq!(h.state().mode, mode);
    }
}

#[test]
fn symbols_insert_replace_remove_and_undo_for_selected_topics() {
    let mut h = harness(ThinodaApp::headless());
    let ctx = h.ctx.clone();
    let root = h.state().doc().unwrap().doc.sheet().root;
    let child = h.state().doc().unwrap().doc.sheet().children(root)[0];
    h.state_mut().doc_mut().unwrap().view.selection = vec![root, child];
    h.get_by_role_and_label(Role::Button, "Symbols").click();
    h.run_steps(3);
    for (name, expected) in [
        ("Priority 1", vec!["priority-1"]),
        ("Priority 2", vec!["priority-2"]),
        ("Priority 2", vec![]),
    ] {
        h.get_by_role_and_label(Role::Button, name).click();
        h.run_steps(3);
        for id in [root, child] {
            assert_eq!(
                h.state()
                    .doc()
                    .unwrap()
                    .doc
                    .sheet()
                    .topic(id)
                    .unwrap()
                    .markers,
                expected
            );
        }
    }
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
    h.run_steps(3);
    for id in [root, child] {
        assert_eq!(
            h.state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .topic(id)
                .unwrap()
                .markers,
            vec!["priority-2"]
        );
    }
    assert_eq!(h.state().doc().unwrap().view.selection, vec![root, child]);
    h.state_mut().exec(&ctx, Cmd::TopicStyle);
    h.run_steps(3);
    assert_eq!(h.state().sidebar_family, SidebarFamily::Format);
    assert_eq!(h.state().format_tab, FormatTab::Style);
    h.state_mut().doc_mut().unwrap().view.selection = vec![root];
    h.get_by_role_and_label(Role::Button, "Symbols").click();
    h.run_steps(3);
    h.state_mut()
        .exec(&ctx, Cmd::EditField(FocusField::Hyperlink));
    h.run_steps(4);
    assert_eq!(h.state().sidebar_family, SidebarFamily::Format);
    assert!(h.get_by_label("Hyperlink").is_focused());
}

#[test]
fn symbols_disable_without_selection_and_explain_availability_on_home() {
    let mut h = harness(ThinodaApp::headless());
    h.get_by_role_and_label(Role::Button, "Symbols").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Priority 1").click();
    h.run_steps(3);
    let root = h.state().doc().unwrap().doc.sheet().root;
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .topic(root)
            .unwrap()
            .markers,
        vec!["priority-1"]
    );
    h.state_mut().doc_mut().unwrap().view.selection.clear();
    h.run_steps(3);
    assert!(
        h.get_by_role_and_label(Role::Button, "Priority 1")
            .accesskit_node()
            .is_disabled()
    );
    h.get_by_label("Select a topic to add symbols.");
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::Home);
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Symbols").click();
    h.run_steps(3);
    h.get_by_label("Open or select a map to show Symbols");
    assert!(h.state().settings.show_format_panel);
}

#[test]
fn new_from_template_menu_groups_every_template_by_its_category() {
    use thinoda_core::templates::{TEMPLATES, TemplateCategory};
    let mut h = harness(ThinodaApp::headless());
    for category in TemplateCategory::ALL {
        h.get_by_role_and_label(Role::Button, "☰").click();
        h.run_steps(2);
        h.get_by_role_and_label(Role::Button, "File").click();
        h.run_steps(2);
        h.get_by_role_and_label(Role::Button, "New from Template…")
            .click();
        h.run_steps(2);
        h.get_by_role_and_label(Role::Button, category.label())
            .click();
        h.run_steps(2);

        let category_templates: Vec<_> = TEMPLATES
            .iter()
            .filter(|template| template.category == category)
            .collect();
        assert!(
            !category_templates.is_empty(),
            "empty menu category: {category:?}"
        );
        for template in &category_templates {
            h.get_by_role_and_label(Role::Button, template.name);
        }
        let selected = category_templates[0];
        h.get_by_role_and_label(Role::Button, selected.name).click();
        h.run_steps(4);
        assert_eq!(
            h.state().doc().unwrap().doc.workbook(),
            &thinoda_core::templates::instantiate(selected.id)
        );
    }
}

#[test]
fn template_chooser_filters_selects_before_creating_and_cancels_without_touching_sources() {
    let mut app = ThinodaApp::headless();
    let before = app.doc().unwrap().doc.workbook().clone();
    let ctx = egui::Context::default();
    app.exec(&ctx, Cmd::Home);
    let mut h = harness(app);
    let ctx = h.ctx.clone();
    assert!(
        h.get_by_role_and_label(Role::Button, "Create")
            .accesskit_node()
            .is_disabled()
    );
    h.get_by_role_and_label(Role::Button, "Mind Map").click();
    h.run_steps(3);
    assert_eq!(h.state().docs.len(), 1);
    assert_eq!(h.state().template_choice, Some("mind-map"));
    assert!(h.state().settings.recent_templates.is_empty());
    h.get_by_role_and_label(Role::Button, "Cancel").click();
    h.run_steps(3);
    assert_eq!(h.state().active, 0);
    assert_eq!(h.state().docs[0].doc.workbook(), &before);
    h.state_mut().exec(&ctx, Cmd::Home);
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Mind Map").click();
    h.run_steps(3);
    h.get_by_label("Template Category").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Analysis and decision")
        .scroll_to_me();
    h.run_steps(8);
    h.get_by_role_and_label(Role::Button, "Analysis and decision")
        .click();
    h.run_steps(3);
    assert_eq!(
        h.state().template_category,
        Some(thinoda_core::templates::TemplateCategory::Analysis)
    );
    assert!(h.state().template_choice.is_none());
    assert!(
        h.query_by_role_and_label(Role::Button, "Mind Map")
            .is_none()
    );
    h.get_by_role_and_label(Role::Button, "SWOT Analysis")
        .click();
    h.run_steps(3);
    assert_eq!(h.state().docs.len(), 1);
    assert_eq!(h.state().template_choice, Some("swot"));
    h.get_by_role_and_label(Role::Button, "Create").focus();
    h.run_steps(2);
    h.key_press(egui::Key::Space);
    h.run_steps(3);
    assert_eq!(h.state().docs.len(), 2);
    assert_eq!(
        h.state().doc().unwrap().doc.workbook(),
        &thinoda_core::templates::instantiate("swot")
    );
    assert_eq!(h.state().docs[0].doc.workbook(), &before);
    assert_eq!(h.state().settings.recent_templates, ["swot"]);
    assert!(h.state().settings.recent_files.is_empty());
    h.state_mut().exec(&ctx, Cmd::Home);
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Recent template: SWOT Analysis");
    h.get_by_role_and_label(Role::Button, "Cancel").click();
    h.run_steps(3);
    assert_eq!(h.state().active, 1);
    let count = h.state().docs.len();
    h.state_mut()
        .exec(&ctx, Cmd::NewFromTemplate("missing-template"));
    assert_eq!(h.state().docs.len(), count);
    assert_eq!(h.state().settings.recent_templates, ["swot"]);
    let expected_key = h.state().docs[1].key.clone();
    h.state_mut()
        .new_doc(thinoda_core::templates::instantiate("meeting"));
    h.state_mut().active = 1;
    h.state_mut().exec(&ctx, Cmd::Home);
    h.state_mut().close_tab_now(0);
    h.run_steps(3);
    assert!(h.state().doc().is_none());
    h.get_by_role_and_label(Role::Button, "Cancel").click();
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().key, expected_key);
}

#[test]
fn template_sections_navigate_with_keyboard_and_return_without_changing_maps() {
    use thinoda_core::templates::{TEMPLATES, TemplateCategory};
    for width in [480.0, 1280.0] {
        let mut app = ThinodaApp::headless();
        let source = app.doc().unwrap().doc.workbook().clone();
        app.exec(&egui::Context::default(), Cmd::Home);
        let mut h = Harness::builder()
            .with_size(egui::vec2(width, 900.0))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        h.ctx
            .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
        h.run_steps(4);
        for category in TemplateCategory::ALL {
            let label = format!("Browse templates: {}", category.label());
            h.get_by_role_and_label(Role::Button, &label).scroll_to_me();
            h.run_steps(10);
            h.get_by_role_and_label(Role::Button, &label).focus();
            h.run_steps(2);
            h.key_press(egui::Key::Enter);
            h.run_steps(4);
            assert_eq!(h.state().template_category, Some(category));
            assert!(h.state().template_choice.is_none());
            let info = TEMPLATES
                .iter()
                .find(|info| info.category == category)
                .unwrap();
            let card = h.get_by_role_and_label(Role::Button, info.name);
            assert!(
                h.ctx.content_rect().contains_rect(card.rect()),
                "{} at {width}",
                info.name
            );
            card.click();
            h.run_steps(3);
            assert_eq!(h.state().template_choice, Some(info.id));
            let back = h.get_by_role_and_label(Role::Button, "All templates");
            assert!(h.ctx.content_rect().contains_rect(back.rect()));
            back.click();
            h.run_steps(4);
            assert!(h.state().template_category.is_none());
            assert!(h.state().template_choice.is_none());
            assert!(
                h.get_by_role_and_label(Role::Button, "Create")
                    .accesskit_node()
                    .is_disabled()
            );
            assert!(
                h.ctx
                    .content_rect()
                    .contains_rect(h.get_by_role_and_label(Role::Button, &label).rect())
            );
            assert_eq!(h.state().docs.len(), 1);
            assert_eq!(h.state().docs[0].doc.workbook(), &source);
            assert!(h.state().settings.recent_templates.is_empty());
        }
        h.get_by_role_and_label(Role::Button, "Cancel").click();
        h.run_steps(3);
        assert_eq!(h.state().active, 0);
    }
}

#[test]
fn every_template_is_created_from_the_preview_and_recents_survive_restart() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::from_root(dir.path());
    paths.ensure().unwrap();
    let mut app = ThinodaApp::new(
        english(),
        Services {
            paths: Some(paths.clone()),
            ..Default::default()
        },
        &[],
    );
    app.settings.reuse_last_theme = true;
    app.settings.default_theme = "ocean".into();
    app.settings.reuse_last_structure = true;
    app.settings.default_structure = thinoda_core::Structure::FishboneLeft.xmind_class().into();
    let mut h = harness(app);
    let ctx = h.ctx.clone();
    for template in thinoda_core::templates::TEMPLATES {
        h.state_mut().exec(&ctx, Cmd::Home);
        h.state_mut().template_category = Some(template.category);
        h.run_steps(3);
        h.get_by_role_and_label(Role::Button, template.name)
            .scroll_to_me();
        h.run_steps(8);
        h.get_by_role_and_label(Role::Button, template.name).focus();
        h.run_steps(2);
        h.key_press(egui::Key::Enter);
        h.run_steps(3);
        assert_eq!(h.state().template_choice, Some(template.id));
        assert_eq!(h.state().active, h.state().docs.len());
        h.get_by_role_and_label(Role::Button, "Create").click();
        h.run_steps(3);
        assert_eq!(
            h.state().doc().unwrap().doc.workbook(),
            &thinoda_core::templates::instantiate(template.id)
        );
        assert!(h.state().doc().unwrap().path.is_none());
        for extension in ["thinoda", "xmind"] {
            let path = dir.path().join(format!("{}.{extension}", template.id));
            h.state_mut()
                .doc_mut()
                .unwrap()
                .save_to(&path, false)
                .unwrap();
            assert_eq!(
                thinoda_io::import_file(&path).unwrap(),
                thinoda_core::templates::instantiate(template.id)
            );
        }
    }
    let settings = Settings::load(&paths).unwrap();
    assert_eq!(
        settings.recent_templates,
        h.state().settings.recent_templates
    );
    assert_eq!(settings.recent_templates.len(), 8);
    assert!(settings.recent_files.is_empty());
    let mut restarted = harness(ThinodaApp::new(settings, Services::default(), &[]));
    restarted.run_steps(3);
    let latest = thinoda_core::templates::TEMPLATES.last().unwrap();
    restarted.get_by_role_and_label(Role::Button, &format!("Recent template: {}", latest.name));
    let count = restarted.state().docs.len();
    restarted
        .get_by_role_and_label(Role::Button, "Cancel")
        .click();
    restarted.run_steps(3);
    assert_eq!(restarted.state().docs.len(), count);
    assert!(restarted.state().doc().is_none());
}

#[test]
fn home_templates_wrap_within_the_window() {
    for width in [480.0, 800.0, 1280.0, 2400.0] {
        let mut app = ThinodaApp::headless();
        app.active = app.docs.len();
        let mut h = Harness::builder()
            .with_size(egui::vec2(width, 1600.0))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        h.ctx
            .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
        h.run_steps(4);
        let first = h.get_by_role_and_label(Role::Button, "Mind Map").rect();
        let last = h.get_by_role_and_label(Role::Button, "Tree Table").rect();
        for template in thinoda_core::templates::TEMPLATES {
            let rect = h.get_by_role_and_label(Role::Button, template.name).rect();
            assert!(
                rect.min.x >= 0.0 && rect.max.x <= width,
                "{width}: {} at {rect:?}",
                template.name
            );
        }
        if width < 2400.0 {
            assert!(last.min.y > first.min.y, "templates must wrap at {width}");
        }
        h.get_by_role_and_label(Role::Button, "Project Plan")
            .scroll_to_me();
        h.run_steps(8);
        h.get_by_role_and_label(Role::Button, "Project Plan")
            .click();
        h.run_steps(3);
        assert!(h.state().doc().is_none());
        let create = h.get_by_role_and_label(Role::Button, "Create").rect();
        assert!(create.min.y >= 0.0 && create.max.y <= 1600.0);
        h.get_by_role_and_label(Role::Button, "Create").click();
        h.run_steps(3);
        assert!(h.state().doc().is_some());
    }
}

#[test]
fn format_panel_can_be_hidden_and_restored_repeatedly() {
    let mut h = harness(ThinodaApp::headless());
    for _ in 0..3 {
        assert!(h.query_by_role_and_label(Role::Tab, "Style").is_some());
        h.get_by_role_and_label(Role::Button, "Format Panel")
            .click();
        h.run_steps(3);
        assert!(!h.state().settings.show_format_panel);
        assert!(h.query_by_role_and_label(Role::Tab, "Style").is_none());
        h.get_by_role_and_label(Role::Button, "Format Panel")
            .click();
        h.run_steps(3);
        assert!(h.state().settings.show_format_panel);
        assert!(h.query_by_role_and_label(Role::Tab, "Style").is_some());
    }
}

#[test]
fn format_panel_on_home_explains_why_it_is_hidden() {
    let mut h = harness(ThinodaApp::headless());
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::Home);
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Format Panel")
        .click();
    h.run_steps(3);
    assert!(h.state().settings.show_format_panel);
    h.get_by_label("Open or select a map to show the Format Panel");
    assert!(h.query_by_role_and_label(Role::Tab, "Style").is_none());
    h.get_by_role_and_label(Role::Button, "Central Topic")
        .click();
    h.run_steps(3);
    assert!(h.query_by_role_and_label(Role::Tab, "Style").is_some());
}

#[test]
fn toolbar_inserts_subtopic_and_topic() {
    let mut h = harness(ThinodaApp::headless());
    let before = root_children(h.state());
    h.get_by_role_and_label(Role::Button, "Subtopic").click();
    h.run_steps(2);
    assert_eq!(
        root_children(h.state()),
        before + 1,
        "Subtopic adds a child to the selected root"
    );
    assert!(h.state().doc().unwrap().is_dirty());

    // The new topic is selected, so "Topic" inserts a sibling under the root.
    h.get_by_role_and_label(Role::Button, "Topic").click();
    h.run_steps(2);
    assert_eq!(root_children(h.state()), before + 2);
}

#[test]
fn undo_redo_via_commands() {
    let mut h = harness(ThinodaApp::headless());
    let before = root_children(h.state());
    let ctx = h.ctx.clone();
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::InsertChild));
    assert_eq!(root_children(h.state()), before + 1);
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
    assert_eq!(root_children(h.state()), before);
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Redo));
    assert_eq!(root_children(h.state()), before + 1);
}

#[test]
fn closing_dirty_tab_asks_and_dont_save_closes() {
    let mut app = ThinodaApp::headless();
    rename_root(&mut app, "Dirty");
    let mut h = harness(app);
    h.get_by_role_and_label(Role::Button, "Close Tab").click();
    h.run_steps(2);
    assert_eq!(h.state().dialogs.close, Some(CloseIntent::Tab(0)));
    h.get_by_label("Save changes?");
    h.get_by_role_and_label(Role::Button, "Don't Save").click();
    h.run_steps(2);
    assert!(h.state().docs.is_empty(), "tab closed without saving");
    // With no document open, the home screen with templates is shown.
    assert!(h.query_all_by_label_contains("Mind Map").next().is_some());
}

#[test]
fn outliner_toolbar_adds_edits_reorders_reparents_and_deletes_topics() {
    let mut app = ThinodaApp::headless();
    app.mode = thinoda_app::app::CenterMode::Outliner;
    let mut h = harness(app);
    h.get_by_role_and_label(Role::Button, "Main Topic 2")
        .click();
    h.run_steps(3);
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .children(h.state().doc().unwrap().doc.sheet().root)
            .len(),
        4
    );

    h.get_by_role_and_label(Role::Button, "Add Sibling").click();
    h.run_steps(3);
    assert!(h.state().doc().unwrap().doc.can_undo());
    assert!(h.state().doc().unwrap().view.editing_topic().is_some());
    h.get_by_label("Topic title").click();
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.get_by_label("Topic title").type_text("Added sibling");
    h.run_steps(2);
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    let d = h.state().doc().unwrap();
    let root = d.doc.sheet().root;
    let added = d.view.primary().unwrap();
    assert_eq!(d.doc.sheet().topic(added).unwrap().title, "Added sibling");
    assert_eq!(d.doc.sheet().parent(added), Some(root));
    assert_eq!(d.doc.sheet().index_in_parent(added), Some(2));

    assert!(thinoda_canvas::Action::MoveUp.available(
        &h.state().doc().unwrap().doc,
        &h.state().doc().unwrap().view,
    ));
    h.get_by_role_and_label(Role::Button, "Move Up").click();
    h.run_steps(3);
    let d = h.state().doc().unwrap();
    let added = d.view.primary().unwrap();
    assert_eq!(d.doc.sheet().index_in_parent(added), Some(1));

    h.get_by_role_and_label(Role::Button, "Indent").click();
    h.run_steps(3);
    let d = h.state().doc().unwrap();
    let added = d.view.primary().unwrap();
    let parent = d.doc.sheet().parent(added).unwrap();
    assert_eq!(d.doc.sheet().topic(parent).unwrap().title, "Main Topic 1");

    h.get_by_role_and_label(Role::Button, "Outdent").click();
    h.run_steps(3);
    let d = h.state().doc().unwrap();
    let added = d.view.primary().unwrap();
    assert_eq!(d.doc.sheet().parent(added), Some(d.doc.sheet().root));

    h.get_by_role_and_label(Role::Button, "Edit Topic").click();
    h.run_steps(3);
    h.get_by_label("Topic title").click();
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.get_by_label("Topic title").type_text("Edited sibling");
    h.run_steps(2);
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    let d = h.state().doc().unwrap();
    let added = d.view.primary().unwrap();
    assert_eq!(d.doc.sheet().topic(added).unwrap().title, "Edited sibling");

    h.get_by_role_and_label(Role::Button, "Add Child").click();
    h.run_steps(3);
    let child = h.state().doc().unwrap().view.primary().unwrap();
    assert_eq!(
        h.state().doc().unwrap().doc.sheet().parent(child),
        Some(added)
    );
    h.get_by_role_and_label(Role::Button, "Edited sibling")
        .click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Delete Topic")
        .click();
    h.run_steps(3);
    let d = h.state().doc().unwrap();
    assert!(!d.doc.sheet().contains(child));
    assert!(!d.doc.sheet().contains(added));
    assert!(d.doc.can_undo());
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run_steps(3);
    assert!(h.state().doc().unwrap().doc.sheet().contains(child));
    assert!(h.state().doc().unwrap().doc.sheet().contains(added));

    h.get_by_role_and_label(Role::Button, "Edited sibling")
        .click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Delete Topic Only")
        .click();
    h.run_steps(3);
    let d = h.state().doc().unwrap();
    assert!(!d.doc.sheet().contains(added));
    assert!(d.doc.sheet().contains(child));
    assert_eq!(d.doc.sheet().parent(child), Some(root));
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run_steps(3);
    assert!(h.state().doc().unwrap().doc.sheet().contains(added));
    assert_eq!(
        h.state().doc().unwrap().doc.sheet().parent(child),
        Some(added)
    );
}

#[test]
fn outliner_and_zen_toggle() {
    let mut h = harness(ThinodaApp::headless());
    h.get_by_role_and_label(Role::Button, "Outliner").click();
    h.run_steps(2);
    assert_eq!(h.state().mode, thinoda_app::app::CenterMode::Outliner);
    h.get_by_role_and_label(Role::Button, "Zen Mode").click();
    h.run_steps(2);
    assert!(h.state().zen);
    assert!(
        h.state()
            .toasts
            .texts()
            .any(|text| text == "Zen Mode: press F11 to exit")
    );
    h.get_by_label("Zen Mode: press F11 to exit");
    assert!(
        h.query_by_role_and_label(Role::Button, "Outliner")
            .is_none(),
        "chrome hidden in Zen mode"
    );
    h.state_mut().toasts.tick(10.0);
    h.key_press(egui::Key::F11);
    h.run_steps(2);
    assert!(!h.state().zen);
    assert!(h.state().toasts.is_empty(), "no entry toast when exiting");
    h.key_press(egui::Key::F11);
    h.run_steps(2);
    assert!(h.state().zen);
    assert_eq!(h.state().toasts.len(), 1);
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert!(!h.state().zen);
}

#[test]
fn search_bar_finds_topics() {
    let mut app = ThinodaApp::headless();
    rename_root(&mut app, "Quarterly Roadmap");
    let mut h = harness(app);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::Find);
    h.state_mut().search.query = "roadmap".into();
    h.run_steps(2);
    let root = h.state().doc().unwrap().doc.sheet().root;
    assert_eq!(h.state().search.hits.len(), 1);
    assert_eq!(h.state().search.hits[0].topic, root);
}

#[test]
fn search_icon_opens_accessible_search_and_can_reopen() {
    let mut app = ThinodaApp::headless();
    rename_root(&mut app, "Quarterly Roadmap");
    let mut h = harness(app);
    for attempt in 0..3 {
        h.get_by_role_and_label(Role::Button, "Find…").click();
        h.run_steps(3);
        assert!(
            h.get_by_label("Search").is_focused(),
            "search focus on reopen {attempt}: {:?}",
            h.ctx
                .memory(|m| m.focused())
                .and_then(|id| h.ctx.read_response(id))
                .map(|r| r.rect)
        );
        h.get_by_label("Search").type_text("roadmap");
        h.run_steps(2);
        let root = h.state().doc().unwrap().doc.sheet().root;
        assert_eq!(h.state().search.hits.len(), 1);
        assert_eq!(h.state().search.hits[0].topic, root);
        h.get_by_label("Next result").click();
        h.run_steps(2);
        assert_eq!(h.state().doc().unwrap().view.primary(), Some(root));
        h.get_by_label("Close search").click();
        h.run_steps(2);
        assert!(!h.state().search.open);
        h.state_mut().search.query.clear();
    }
}

#[test]
fn ctrl_h_replaces_and_undo_restores_the_topic() {
    let mut app = ThinodaApp::headless();
    rename_root(&mut app, "Quarterly Roadmap");
    let mut h = harness(app);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::H);
    h.run_steps(3);
    h.get_by_label("Search").type_text("Roadmap");
    h.run_steps(2);
    h.get_by_label("Replace with").click();
    h.run_steps(2);
    h.get_by_label("Replace with").type_text("Plan");
    h.run_steps(2);
    h.get_by_label("Replace All").click();
    h.run_steps(2);
    assert_eq!(
        root_title(h.state().doc().unwrap().doc.workbook()),
        "Quarterly Plan"
    );
    let ctx = h.ctx.clone();
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
    assert_eq!(
        root_title(h.state().doc().unwrap().doc.workbook()),
        "Quarterly Roadmap"
    );
}

#[test]
fn search_refreshes_when_switching_between_documents_at_the_same_revision() {
    let mut app = ThinodaApp::headless();
    rename_root(&mut app, "Roadmap");
    let mut h = harness(app);
    h.state_mut().search.query = "Roadmap".into();
    h.run_steps(2);
    let old_root = h.state().doc().unwrap().doc.sheet().root;
    assert_eq!(h.state().search.hits.len(), 1);
    assert_eq!(h.state().search.hits[0].topic, old_root);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::New);
    rename_root(h.state_mut(), "Different document");
    h.run_steps(2);
    assert!(h.state().search.hits.is_empty());
}

#[test]
fn search_refreshes_when_switching_sheets_without_an_edit() {
    let mut app = ThinodaApp::headless();
    rename_root(&mut app, "Roadmap");
    let ctx = egui::Context::default();
    app.exec(&ctx, Cmd::NewSheet);
    rename_root(&mut app, "Meeting");
    app.doc_mut().unwrap().doc.set_active(0);
    let mut h = harness(app);
    h.state_mut().search.query = "Roadmap".into();
    h.run_steps(2);
    assert_eq!(h.state().search.hits.len(), 1);
    let d = h.state_mut().doc_mut().unwrap();
    let revision = d.doc.state_id();
    d.doc.set_active(1);
    d.view.reset();
    assert_eq!(d.doc.state_id(), revision);
    h.run_steps(2);
    assert!(h.state().search.hits.is_empty());
}

#[test]
fn cross_sheet_search_navigates_wraps_and_filters_only_the_current_sheet() {
    let mut app = ThinodaApp::headless();
    rename_root(&mut app, "Roadmap Alpha");
    let ctx = egui::Context::default();
    app.exec(&ctx, Cmd::NewSheet);
    rename_root(&mut app, "Roadmap Beta");
    app.doc_mut().unwrap().doc.set_active(0);
    let mut h = harness(app);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::Find);
    h.state_mut().search.query = "Roadmap".into();
    h.run_steps(3);
    assert_eq!(h.state().search.hits.len(), 1);
    h.get_by_label("All sheets").click();
    h.run_steps(3);
    h.get_by_label("Dim others").click();
    h.run_steps(3);
    assert!(h.state().search.dim);
    assert_eq!(h.state().search.hits.len(), 2);
    h.get_by_label("Next result").click();
    h.run_steps(3);
    assert_eq!(h.state().search.current, 1);
    let d = h.state().doc().unwrap();
    assert_eq!(d.doc.active_index(), 1);
    assert_eq!(d.view.primary(), Some(d.doc.sheet().root));
    assert_eq!(d.view.filter, Some(vec![d.doc.sheet().root]));
    // Rendering more frames must not reset the result index on a sheet switch.
    h.run_steps(3);
    assert_eq!(h.state().search.current, 1);
    h.get_by_label("Next result").click();
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.active_index(), 0);
    assert_eq!(h.state().search.current, 0);
    h.get_by_label("Previous result").click();
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.active_index(), 1);
    h.get_by_label("Close search").click();
    h.run_steps(3);
    assert!(h.state().doc().unwrap().view.filter.is_none());
}

#[test]
fn cross_sheet_replace_is_one_undo_step_and_keeps_notes_labels_and_other_tabs() {
    let mut app = ThinodaApp::headless();
    let ctx = egui::Context::default();
    rename_root(&mut app, "Roadmap Alpha");
    app.exec(&ctx, Cmd::NewSheet);
    rename_root(&mut app, "Roadmap Beta");
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Notes", |s| {
            s.topic_mut(s.root).unwrap().notes = Some("Roadmap in notes".into());
            s.topic_mut(s.root).unwrap().labels = vec!["Roadmap label".into()];
            Ok(())
        })
        .unwrap();
    let first_book = app.doc().unwrap().doc.workbook().clone();
    app.exec(&ctx, Cmd::New);
    rename_root(&mut app, "Roadmap Other Document");
    app.active = 0;
    app.search.query = "Roadmap".into();
    app.search.replace = "Plan".into();
    app.search.all_sheets = true;
    app.search.opts.include_notes = true;
    app.search.opts.include_labels = true;
    app.replace_all();
    assert_eq!(app.doc().unwrap().doc.active_index(), 1);
    for s in &app.doc().unwrap().doc.workbook().sheets {
        assert!(s.topic(s.root).unwrap().title.starts_with("Plan"));
    }
    let s = app.doc().unwrap().doc.sheet();
    assert_eq!(
        s.topic(s.root).unwrap().notes.as_deref(),
        Some("Roadmap in notes")
    );
    assert_eq!(s.topic(s.root).unwrap().labels, vec!["Roadmap label"]);
    assert_eq!(
        root_title(app.docs[1].doc.workbook()),
        "Roadmap Other Document"
    );
    assert!(app.doc_mut().unwrap().doc.undo());
    assert_eq!(app.doc().unwrap().doc.workbook(), &first_book);
    assert!(app.doc_mut().unwrap().doc.redo());
    assert_eq!(root_title(app.doc().unwrap().doc.workbook()), "Plan Alpha");
    // Do not apply stale hits after the user changes the query.
    app.search.query = "Missing".into();
    app.search.replace = "Wrong".into();
    let revision = app.doc().unwrap().doc.state_id();
    app.replace_all();
    assert_eq!(app.doc().unwrap().doc.state_id(), revision);
}

#[test]
fn cross_sheet_search_refreshes_notes_labels_and_reveals_collapsed_results() {
    let mut app = ThinodaApp::headless();
    let ctx = egui::Context::default();
    app.exec(&ctx, Cmd::NewSheet);
    let mut target = None;
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Search targets", |s| {
            let branch = s.add_child(s.root, None, "Branch");
            let child = s.add_child(branch, None, "Hidden");
            s.topic_mut(child).unwrap().notes = Some("Needle".into());
            s.topic_mut(s.root).unwrap().labels = vec!["Needle".into()];
            s.set_collapsed(branch, true)?;
            target = Some((branch, child));
            Ok(())
        })
        .unwrap();
    app.doc_mut().unwrap().doc.set_active(0);
    app.search.open = true;
    app.search.all_sheets = true;
    app.search.query = "Needle".into();
    app.update_search();
    assert!(app.search.hits.is_empty());
    app.search.opts.include_notes = true;
    app.update_search();
    assert_eq!(app.search.hits.len(), 1);
    app.search.opts.include_labels = true;
    app.update_search();
    assert_eq!(app.search.hits.len(), 2);
    app.goto_hit(1);
    let (branch, child) = target.unwrap();
    let d = app.doc().unwrap();
    assert_eq!(d.doc.active_index(), 1);
    assert_eq!(d.view.primary(), Some(child));
    assert!(!d.doc.sheet().topic(branch).unwrap().collapsed);
    assert_eq!(app.search.current, 1);
    app.update_search();
    assert_eq!(app.search.current, 1);
    app.search.dim = true;
    app.apply_search_filter();
    assert!(app.doc().unwrap().view.filter.is_some());
    app.exec(&ctx, Cmd::New);
    app.update_search();
    assert!(
        app.docs[0].view.filter.is_none(),
        "inactive tabs must not retain stale dimming"
    );
}

#[test]
fn contextual_toolbar_follows_selection_and_edits_topic_properties() {
    let mut h = harness(ThinodaApp::headless());
    let before = root_children(h.state());
    h.get_by_label("Add subtopic").click();
    h.run_steps(2);
    assert_eq!(root_children(h.state()), before + 1);
    assert!(
        h.query_by_label("Edit notes").is_none(),
        "hidden during title editing"
    );
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    h.get_by_label("Toggle to-do").click();
    h.run_steps(2);
    let d = h.state().doc().unwrap();
    let id = d.view.primary().unwrap();
    assert_eq!(d.doc.sheet().topic(id).unwrap().todo, Some(false));
    h.get_by_label("Edit notes").click();
    h.run_steps(3);
    assert!(h.get_by_label("Notes").is_focused());
    h.get_by_label("Notes").type_text("Contextual editing");
    h.run_steps(2);
    // Moving focus commits the buffered property.
    h.get_by_label("Connect topics").click();
    h.run_steps(2);
    assert_eq!(h.state().doc().unwrap().view.relationship_from, Some(id));
    assert!(
        h.query_by_label("Toggle to-do").is_none(),
        "hidden while connecting"
    );
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .topic(id)
            .unwrap()
            .notes
            .as_deref(),
        Some("Contextual editing")
    );
}

#[test]
fn floating_insert_menu_opens_notes_and_preserves_selection() {
    let mut h = harness(ThinodaApp::headless());
    let selected = h.state().doc().unwrap().view.primary();
    h.get_by_label("More insert actions").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Notes").click();
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().view.primary(), selected);
    assert!(
        h.get_by_role_and_label(Role::MultilineTextInput, "Notes")
            .is_focused()
    );
}

fn right_click_at(h: &mut Harness<'static, ThinodaApp>, pos: egui::Pos2) {
    h.event(egui::Event::PointerMoved(pos));
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    h.run_steps(3);
}

#[test]
fn structural_objects_delete_from_context_and_keyboard_and_remain_deleted_on_disk() {
    use thinoda_canvas::ObjectSelection;
    for dark in [false, true] {
        for kind in 0..3 {
            let mut app = ThinodaApp::headless();
            app.settings.show_topic_add_buttons = false;
            app.settings.theme = if dark { Theme::Dark } else { Theme::Light };
            let d = app.doc_mut().unwrap();
            let mut book = Workbook::default();
            let s = &mut book.sheets[0];
            *s = thinoda_core::Sheet::new("S", "Root");
            s.structure = thinoda_core::Structure::LogicRight;
            let a = s.add_child(s.root, None, "Covered A");
            let b = s.add_child(s.root, None, "Covered B");
            let (object, label) = match kind {
                0 => (
                    ObjectSelection::Relationship(s.add_relationship(a, b).unwrap()),
                    "Delete Relationship",
                ),
                1 => (
                    ObjectSelection::Boundary(s.add_boundary(&[a, b]).unwrap()),
                    "Delete Boundary",
                ),
                _ => {
                    let topic = s.add_summary(&[a, b], "Conclusion").unwrap();
                    s.add_child(topic, None, "Summary child");
                    (
                        ObjectSelection::Summary(s.summaries[0].id),
                        "Delete Summary",
                    )
                }
            };
            d.doc.replace(book);
            d.view.reset();
            let mut h = harness(app);
            let before = h.state().doc().unwrap().doc.sheet().clone();
            let point = {
                let d = h.state_mut().doc_mut().unwrap();
                let layout = d.view.layout_for(&d.doc);
                let point = match object {
                    ObjectSelection::Relationship(_) => {
                        let r = &layout.relationships[0];
                        thinoda_core::layout::cubic(r.start, r.c1, r.c2, r.end, 0.5)
                    }
                    ObjectSelection::Boundary(_) => layout.boundaries[0].rect.top_center(),
                    ObjectSelection::Summary(_) => layout.summaries[0].tip,
                };
                d.view.canvas_rect().center()
                    + d.view.offset
                    + egui::vec2(point.x, point.y) * d.view.zoom
            };
            right_click_at(&mut h, point);
            assert_eq!(h.state().doc().unwrap().view.selected_object, Some(object));
            if kind == 2 {
                h.get_by_label(
                    "Deletes the summary topic and its children, not the covered branches.",
                );
            }
            h.get_by_role_and_label(Role::Button, label).click();
            h.run_steps(3);
            assert!(!object.exists(h.state().doc().unwrap().doc.sheet()));
            let ctx = h.ctx.clone();
            h.state_mut()
                .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
            h.run_steps(3);
            assert_eq!(h.state().doc().unwrap().doc.sheet(), &before);
            h.event(egui::Event::PointerMoved(point));
            for pressed in [true, false] {
                h.event(egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            h.run_steps(3);
            assert_eq!(h.state().doc().unwrap().view.selected_object, Some(object));
            h.key_press(egui::Key::Delete);
            h.run_steps(3);
            let sheet = h.state().doc().unwrap().doc.sheet();
            assert!(!object.exists(sheet));
            assert_eq!(sheet.topic(a), before.topic(a));
            assert_eq!(sheet.topic(b), before.topic(b));
            sheet.validate().unwrap();
            let dir = tempfile::tempdir().unwrap();
            for extension in ["thinoda", "xmind"] {
                let path = dir.path().join(format!("deleted.{extension}"));
                h.state_mut()
                    .doc_mut()
                    .unwrap()
                    .save_to(&path, false)
                    .unwrap();
                let imported = thinoda_io::import_file(&path).unwrap();
                let sheet = imported.active();
                assert!(!object.exists(sheet));
                assert!(sheet.contains(a) && sheet.contains(b));
                sheet.validate().unwrap();
            }
        }
    }
}

#[test]
fn contextual_menu_disables_root_parent_and_blank_canvas_topic_actions() {
    let mut h = harness(ThinodaApp::headless());
    let d = h.state().doc().unwrap();
    let root_rect = d.view.selected_topic_rect(&d.doc).unwrap();
    let before = d.doc.sheet().all_ids_ordered().len();
    right_click_at(&mut h, root_rect.center());
    h.get_by_label("Parent Topic").click();
    h.run_steps(2);
    assert_eq!(
        h.state().doc().unwrap().doc.sheet().all_ids_ordered().len(),
        before
    );
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    let canvas = h.state().doc().unwrap().view.canvas_rect();
    right_click_at(&mut h, canvas.left_top() + egui::vec2(16.0, 80.0));
    assert!(h.state().doc().unwrap().view.selection.is_empty());
    h.query_all_by_role_and_label(Role::Button, "Subtopic")
        .last()
        .unwrap()
        .click();
    h.run_steps(2);
    assert_eq!(
        h.state().doc().unwrap().doc.sheet().all_ids_ordered().len(),
        before
    );
}

#[test]
fn dark_theme_is_applied() {
    let mut h = harness(ThinodaApp::headless());
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::SetTheme(Theme::Dark));
    h.run_steps(2);
    assert!(h.ctx.global_style().visuals.dark_mode);
    h.state_mut().exec(&ctx, Cmd::SetTheme(Theme::Light));
    h.run_steps(2);
    assert!(!h.ctx.global_style().visuals.dark_mode);
}

#[test]
fn save_and_reopen_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Plan.thinoda");
    let mut app = ThinodaApp::headless();
    rename_root(&mut app, "Launch Plan");
    let d = app.doc_mut().unwrap();
    d.save_to(&path, false).unwrap();
    assert!(!d.is_dirty());

    let app2 = ThinodaApp::new(english(), Services::default(), std::slice::from_ref(&path));
    assert_eq!(app2.docs.len(), 1);
    let d2 = app2.doc().unwrap();
    assert_eq!(d2.title(), "Plan");
    assert!(!d2.is_dirty());
    assert_eq!(root_title(d2.doc.workbook()), "Launch Plan");
}

#[test]
fn xmind_export_reimports_and_foreign_import_is_dirty() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = ThinodaApp::headless();
    rename_root(&mut app, "Interop");
    let wb = app.doc().unwrap().doc.workbook().clone();
    for format in [
        Format::XMind,
        Format::FreeMind,
        Format::Opml,
        Format::Markdown,
    ] {
        let p = dir.path().join(format!("map.{}", format.extensions()[0]));
        let bytes = thinoda_io::export_bytes(&wb, format, &Default::default()).unwrap();
        std::fs::write(&p, bytes).unwrap();
        let mut a = ThinodaApp::new(english(), Services::default(), &[]);
        a.open_path(&p);
        let d = a.doc().unwrap_or_else(|| panic!("{format:?} did not open"));
        assert_eq!(root_title(d.doc.workbook()), "Interop", "{format:?}");
        // Foreign formats must be saved under a new native name.
        assert_eq!(d.is_dirty(), format != Format::XMind, "{format:?}");
    }
}

#[test]
fn crash_recovery_restores_unsaved_work() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::from_root(dir.path());
    paths.ensure().unwrap();

    {
        let recovery = Arc::new(RecoveryManager::start(&paths).unwrap());
        let services = Services {
            recovery: Some(recovery.clone()),
            ..Default::default()
        };
        let mut app = ThinodaApp::new(english(), services, &[]);
        app.new_doc(thinoda_core::templates::instantiate("mind-map"));
        rename_root(&mut app, "Unsaved Idea");
        app.doc_mut().unwrap().snapshot(&recovery);
        // Simulate a crash: the panic hook flushes and marks the session for preservation.
        assert!(recovery.try_flush_for_crash());
        // This e2e test discovers recovery in-process, so release the native session lock.
        recovery.suspend().unwrap();
        drop(app);
    }

    let orphans = RecoveryManager::find_orphans(&paths);
    assert_eq!(orphans.len(), 1, "crashed session is found on next start");
    let mut app = ThinodaApp::new(english(), Services::default(), &[]);
    app.dialogs.recovery = orphans;
    let mut h = harness(app);
    h.get_by_role_and_label(Role::Button, "Restore").click();
    h.run_steps(2);
    let app = h.state();
    assert!(app.dialogs.recovery.is_empty());
    let d = app.doc().expect("restored document is open");
    assert!(
        d.is_dirty(),
        "restored work stays unsaved until the user saves"
    );
    assert_eq!(root_title(d.doc.workbook()), "Unsaved Idea");
    drop(h);
    assert!(
        RecoveryManager::find_orphans(&paths).is_empty(),
        "restored session is cleaned up"
    );
}

fn thinoda_bin() -> &'static str {
    env!("CARGO_BIN_EXE_thinoda")
}

#[test]
fn cli_version_and_help() {
    let out = std::process::Command::new(thinoda_bin())
        .arg("--version")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains(env!("CARGO_PKG_VERSION")));
    let out = std::process::Command::new(thinoda_bin())
        .arg("--help")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("--convert"));
}

#[test]
fn cli_converts_between_formats() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.md");
    std::fs::write(&input, "# Project\n\n- Scope\n- Budget\n  - Q1\n").unwrap();
    for ext in ["xmind", "opml", "pdf", "png", "svg", "docx"] {
        let output = dir.path().join(format!("out.{ext}"));
        let st = std::process::Command::new(thinoda_bin())
            .arg("--convert")
            .arg(&input)
            .arg(&output)
            .status()
            .unwrap();
        assert!(st.success(), "convert to {ext}");
        assert!(
            std::fs::metadata(&output).unwrap().len() > 0,
            "{ext} written"
        );
    }
    let wb = thinoda_io::import_file(Path::new(&dir.path().join("out.xmind"))).unwrap();
    assert_eq!(root_title(&wb), "Project");

    let st = std::process::Command::new(thinoda_bin())
        .arg("--convert")
        .arg(dir.path().join("missing.md"))
        .arg(dir.path().join("x.opml"))
        .status()
        .unwrap();
    assert!(!st.success(), "missing input fails with non-zero exit code");
}

#[test]
fn ctrl_k_and_ctrl_shift_n_focus_hyperlink_and_notes() {
    let mut h = harness(ThinodaApp::headless());
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::K);
    h.run_steps(3);
    assert!(
        h.state().settings.show_format_panel,
        "Ctrl+K opens the format panel"
    );
    let link = h.get_by_label("Hyperlink");
    assert!(link.is_focused(), "Ctrl+K focuses the hyperlink field");
    link.type_text("https://example.com");
    h.run_steps(2);
    h.key_press_modifiers(
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        egui::Key::N,
    );
    h.run_steps(3);
    assert!(
        h.get_by_label("Notes").is_focused(),
        "Ctrl+Shift+N focuses the notes field"
    );
    let d = h.state().doc().unwrap();
    let s = d.doc.sheet();
    assert_eq!(
        s.topic(s.root).unwrap().hyperlink.as_deref(),
        Some("https://example.com")
    );
    assert_eq!(h.state().focus_field, None::<FocusField>);
}

#[test]
fn ctrl_b_adds_boundary() {
    let mut h = harness(ThinodaApp::headless());
    let first = {
        let s = h.state().doc().unwrap().doc.sheet();
        s.children(s.root)[0]
    };
    h.state_mut().doc_mut().unwrap().view.select_only(first);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::B);
    h.run_steps(2);
    assert_eq!(h.state().doc().unwrap().doc.sheet().boundaries.len(), 1);
}

// ---------------------------------------------------------------------------
// AI assistant (against an in-process OpenAI-compatible mock server)
// ---------------------------------------------------------------------------

fn ai_harness(reply: &str) -> (Harness<'static, ThinodaApp>, thinoda_ai::mock::MockServer) {
    let server = thinoda_ai::mock::MockServer::start(&["text-embedding-x", "mock-llm"], reply);
    let mut app = ThinodaApp::headless();
    app.settings.ai.base_url = server.base_url.clone();
    let mut h = harness(app);
    let ctx = h.ctx.clone();
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::SelectRoot));
    (h, server)
}

fn wait_for_ai(h: &mut Harness<'static, ThinodaApp>) {
    let start = std::time::Instant::now();
    while h.state().ai.pending.is_some() {
        assert!(start.elapsed().as_secs() < 20, "AI job did not finish");
        std::thread::sleep(std::time::Duration::from_millis(20));
        h.run_steps(1);
    }
    h.run_steps(1);
}

#[test]
fn ai_brainstorm_adds_subtopics_in_one_undo_step() {
    let (mut h, server) = ai_harness("- Alpha\n- Beta\n  - Beta one\n- Gamma");
    let before = root_children(h.state());
    h.key_press_modifiers(
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        egui::Key::I,
    );
    h.run_steps(1);
    assert!(
        h.state().ai.pending.is_some() || root_children(h.state()) > before,
        "Ctrl+Shift+I starts brainstorm"
    );
    wait_for_ai(&mut h);
    assert_eq!(root_children(h.state()), before + 3);
    let req = server.requests.lock().unwrap();
    let chat = req
        .iter()
        .find(|r| r.path.ends_with("/chat/completions"))
        .unwrap();
    assert!(
        chat.body.contains("mock-llm"),
        "embedding model must be skipped"
    );
    drop(req);
    let ctx = h.ctx.clone();
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
    assert_eq!(root_children(h.state()), before);
}

#[test]
fn ai_breakdown_creates_todos_and_explain_sets_notes() {
    let (mut h, _server) = ai_harness("1. Plan\n2. Build");
    let ctx = h.ctx.clone();
    h.state_mut()
        .exec(&ctx, Cmd::Ai(thinoda_app::ai::AiCommand::Breakdown));
    wait_for_ai(&mut h);
    let s = h.state().doc().unwrap().doc.sheet();
    let kids = s.children(s.root);
    let todos = kids
        .iter()
        .filter(|id| s.topic(**id).unwrap().todo.is_some())
        .count();
    assert_eq!(todos, 2);

    let (mut h, _server) = ai_harness("A short explanation.");
    let ctx = h.ctx.clone();
    h.state_mut()
        .exec(&ctx, Cmd::Ai(thinoda_app::ai::AiCommand::Explain));
    wait_for_ai(&mut h);
    let s = h.state().doc().unwrap().doc.sheet();
    let notes = s.topic(s.root).unwrap().notes.clone().unwrap_or_default();
    assert!(notes.contains("A short explanation."), "{notes}");
}

#[test]
fn ai_generate_map_opens_new_document() {
    let (mut h, _server) = ai_harness("# Trip\n## Packing\n- Clothes\n## Route\n## Budget");
    let docs = h.state().docs.len();
    let ctx = h.ctx.clone();
    h.state_mut()
        .exec(&ctx, Cmd::Ai(thinoda_app::ai::AiCommand::GenerateMap));
    h.run_steps(2);
    assert!(h.state().ai.prompt.is_some(), "prompt dialog opens");
    h.state_mut().start_generate_map("Trip".into());
    h.state_mut().ai.prompt = None;
    wait_for_ai(&mut h);
    assert_eq!(h.state().docs.len(), docs + 1);
    assert_eq!(root_children(h.state()), 3);
}

#[test]
fn ai_unreachable_server_shows_error_and_keeps_map() {
    let mut app = ThinodaApp::headless();
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    app.settings.ai.base_url = format!("http://127.0.0.1:{port}/v1");
    let mut h = harness(app);
    let ctx = h.ctx.clone();
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::SelectRoot));
    let before = root_children(h.state());
    h.state_mut()
        .exec(&ctx, Cmd::Ai(thinoda_app::ai::AiCommand::Brainstorm));
    wait_for_ai(&mut h);
    assert_eq!(root_children(h.state()), before);
}
