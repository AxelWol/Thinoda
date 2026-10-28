use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{CenterMode, Cmd, Services, ThinodaApp},
    i18n::t,
};
use thinoda_core::{Sheet, search::Completion};
use thinoda_platform::settings::{Language, Settings, ThemeMode};

#[test]
fn facets_and_notes_work_across_languages_themes_views_and_scope() {
    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System] {
            let mut app = ThinodaApp::new(
                Settings {
                    language,
                    theme,
                    ..Default::default()
                },
                Services::default(),
                &[],
            );
            let ctx = egui::Context::default();
            app.exec(&ctx, Cmd::New);
            let doc = app.doc_mut().unwrap();
            let root = doc.doc.sheet().root;
            let target = doc
                .doc
                .edit_sheet("Fixture", |sheet| {
                    let target = sheet.add_child(root, None, "Review budget");
                    let topic = sheet.topic_mut(target).unwrap();
                    topic.notes = Some("Meeting decisions".into());
                    topic.labels = vec!["finance".into()];
                    topic.markers = vec!["flag-red".into(), "priority-1".into()];
                    topic.todo = Some(false);
                    sheet.add_child(root, None, "Other");
                    Ok(target)
                })
                .unwrap();
            doc.doc
                .edit_book("Second sheet", |book| {
                    let mut sheet = Sheet::new("Second", "Second root");
                    sheet.topic_mut(sheet.root).unwrap().notes = Some("Other meeting".into());
                    book.sheets.push(sheet);
                    Ok(())
                })
                .unwrap();
            let original = app.doc().unwrap().doc.workbook().clone();
            let revision = app.doc().unwrap().doc.state_id();
            let mut h = Harness::builder()
                .with_size(egui::vec2(1000.0, 800.0))
                .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
            h.ctx
                .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
            h.run_steps(3);
            let ctx = h.ctx.clone();
            h.state_mut().exec(&ctx, Cmd::Find);
            h.run_steps(3);
            h.get_by_role_and_label(Role::CheckBox, t("View All Notes"))
                .focus();
            h.key_press(egui::Key::Space);
            h.run_steps(4);
            assert!(h.state().search.facets.notes_only);
            assert_eq!(h.state().search.hits.len(), 1);
            assert_eq!(h.state().search.hits[0].topic, target);
            h.state_mut().search.all_sheets = true;
            h.run_steps(4);
            assert_eq!(h.state().search.hits.len(), 2);
            h.get_by_label(t("Filters")).focus();
            h.key_press(egui::Key::Enter);
            h.run_steps(5);
            let incomplete = format!("{}: {}", t("To-Do"), t("Incomplete"));
            h.get_by_role_and_label(Role::CheckBox, &incomplete).focus();
            h.run_steps(5);
            h.key_press(egui::Key::Space);
            h.run_steps(4);
            assert_eq!(h.state().search.facets.todos, vec![Completion::Incomplete]);
            for label in [
                "finance".to_owned(),
                "1".to_owned(),
                format!(
                    "{} {}",
                    thinoda_core::markers::glyph("flag-red"),
                    t("Red flag")
                ),
            ] {
                h.get_by_role_and_label(Role::CheckBox, &label).focus();
                h.run_steps(5);
                h.key_press(egui::Key::Space);
                h.run_steps(3);
                assert_eq!(h.state().search.hits.len(), 1);
            }
            assert_eq!(h.state().search.facets.labels, vec!["finance"]);
            assert_eq!(h.state().search.facets.priorities, vec![1]);
            assert_eq!(h.state().search.facets.markers, vec!["flag-red"]);
            h.state_mut().search.dim = true;
            h.run_steps(4);
            assert_eq!(h.state().search.hits.len(), 1);
            assert_eq!(
                h.state().doc().unwrap().view.filter.as_ref().unwrap(),
                &vec![root, target]
            );
            for mode in [CenterMode::Map, CenterMode::Outliner] {
                h.state_mut().mode = mode;
                h.set_size(egui::vec2(360.0, 640.0));
                h.run_steps(4);
                let control = h.get_by_role_and_label(Role::Button, t("Clear search and filters"));
                control.focus();
                h.run_steps(8);
                assert!(
                    h.ctx.content_rect().contains_rect(
                        h.get_by_role_and_label(Role::Button, t("Clear search and filters"))
                            .rect()
                    )
                );
            }
            h.key_press(egui::Key::Enter);
            h.run_steps(4);
            assert!(!h.state().search.facets.active());
            assert!(h.state().doc().unwrap().view.filter.is_none());
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &original);
            assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);
        }
    }
}

#[test]
fn changed_source_scope_and_document_cannot_reuse_stale_hits() {
    let mut app = ThinodaApp::new(Settings::default(), Services::default(), &[]);
    let ctx = egui::Context::default();
    app.exec(&ctx, Cmd::New);
    let root = app.doc().unwrap().doc.sheet().root;
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Note", |sheet| {
            sheet.topic_mut(root).unwrap().notes = Some("Memo".into());
            Ok(())
        })
        .unwrap();
    app.search.open = true;
    app.search.facets.notes_only = true;
    app.update_search();
    assert_eq!(app.search.hits.len(), 1);
    app.search.query = "Memo".into();
    app.search.replace = "Changed".into();
    let revision = app.doc().unwrap().doc.state_id();
    app.replace_all();
    assert_eq!(
        app.doc().unwrap().doc.state_id(),
        revision,
        "notes are read-only"
    );
    app.search.query.clear();
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Remove note", |sheet| {
            sheet.topic_mut(root).unwrap().notes = None;
            Ok(())
        })
        .unwrap();
    app.update_search();
    assert!(app.search.hits.is_empty());
    app.exec(&ctx, Cmd::New);
    app.update_search();
    assert!(app.search.hits.is_empty());
    assert!(app.doc().unwrap().view.filter.is_none());
}

#[test]
fn find_searches_formatted_note_text_without_rewriting_source_or_matching_link_targets() {
    let mut app = ThinodaApp::new(Settings::default(), Services::default(), &[]);
    let ctx = egui::Context::default();
    app.exec(&ctx, Cmd::New);
    let root = app.doc().unwrap().doc.sheet().root;
    let source = "**Café** and [visible label](https://example.invalid/private)";
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Formatted note", |sheet| {
            sheet.topic_mut(root).unwrap().notes = Some(source.into());
            Ok(())
        })
        .unwrap();
    app.search.open = true;
    app.search.opts.include_notes = true;
    app.search.query = "Café".into();
    app.update_search();
    assert_eq!(app.search.hits.len(), 1);
    assert_eq!(
        app.search.hits[0].field,
        thinoda_core::search::MatchField::Notes
    );
    app.search.query = "visible label".into();
    app.update_search();
    assert_eq!(app.search.hits.len(), 1);
    app.search.query = "example.invalid/private".into();
    app.update_search();
    assert!(app.search.hits.is_empty());
    app.search.facets.notes_only = true;
    app.search.query = "Café".into();
    app.update_search();
    assert_eq!(app.search.hits.len(), 1);
    assert_eq!(
        app.doc().unwrap().doc.sheet().root_topic().notes.as_deref(),
        Some(source)
    );

    let oversized_source = format!("{} oversizedneedle", "x".repeat(16 * 1024));
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Oversized note", |sheet| {
            sheet.topic_mut(root).unwrap().notes = Some(oversized_source.clone());
            Ok(())
        })
        .unwrap();
    app.search.query = "oversizedneedle".into();
    app.update_search();
    assert_eq!(
        app.search.hits.len(),
        1,
        "notes beyond the projection bound remain searchable as source text"
    );
}

#[test]
fn search_ui_finds_visible_markdown_note_text_when_include_notes_is_enabled() {
    let mut app = ThinodaApp::new(Settings::default(), Services::default(), &[]);
    let ctx = egui::Context::default();
    app.exec(&ctx, Cmd::New);
    let root = app.doc().unwrap().doc.sheet().root;
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Formatted note", |sheet| {
            sheet.topic_mut(root).unwrap().notes =
                Some("**Café** [visible label](https://example.invalid/private)".into());
            Ok(())
        })
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 800.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    h.run_steps(3);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::Find);
    h.run_steps(4);
    h.get_by_label(t("Search")).type_text("Café");
    h.run_steps(3);
    h.get_by_role_and_label(Role::CheckBox, t("Include notes"))
        .click();
    h.run_steps(4);
    assert_eq!(h.state().search.hits.len(), 1);
    assert_eq!(h.state().search.hits[0].topic, root);
    h.get_by_label(t(
        "Notes search uses visible Markdown text; oversized notes search the original source.",
    ));
}

#[test]
fn note_result_navigation_keeps_sheet_and_topic_identity() {
    let mut app = ThinodaApp::new(Settings::default(), Services::default(), &[]);
    let ctx = egui::Context::default();
    app.exec(&ctx, Cmd::New);
    app.doc_mut()
        .unwrap()
        .doc
        .edit_book("Notes", |book| {
            let mut sheet = Sheet::new("Notes sheet", "Target topic");
            sheet.topic_mut(sheet.root).unwrap().notes = Some("Local memo".into());
            book.sheets.push(sheet);
            Ok(())
        })
        .unwrap();
    let before = app.doc().unwrap().doc.workbook().clone();
    let revision = app.doc().unwrap().doc.state_id();
    app.search.all_sheets = true;
    app.search.facets.notes_only = true;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 800.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::Find);
    h.run_steps(5);
    h.get_by_role_and_label(Role::Button, "Notes sheet / Target topic")
        .focus();
    h.run_steps(5);
    h.key_press(egui::Key::Enter);
    h.run_steps(4);
    let doc = h.state().doc().unwrap();
    assert_eq!(doc.doc.active_index(), 1);
    assert_eq!(doc.view.primary(), Some(doc.doc.sheet().root));
    assert_eq!(doc.doc.state_id(), revision);
    // Active-sheet navigation is view state; compare saved sheet contents.
    assert_eq!(doc.doc.workbook().sheets, before.sheets);
}
