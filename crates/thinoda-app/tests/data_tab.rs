use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{CenterMode, Cmd, FormatTab, Services, ThinodaApp},
    i18n::t,
};
use thinoda_platform::settings::{Language, Settings, ThemeMode};

static LANGUAGE: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn harness(app: ThinodaApp, size: egui::Vec2) -> Harness<'static, ThinodaApp> {
    let mut h = Harness::builder()
        .with_size(size)
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    h.run_steps(4);
    h
}

#[test]
fn data_is_between_style_and_map_and_separates_topic_content_in_both_views() {
    let _language = LANGUAGE.lock().unwrap();
    for language in Language::ALL {
        for theme in [ThemeMode::Light, ThemeMode::Dark] {
            for mode in [CenterMode::Map, CenterMode::Outliner] {
                for size in [egui::vec2(400.0, 360.0), egui::vec2(1280.0, 900.0)] {
                    let mut app = ThinodaApp::new(
                        Settings {
                            language,
                            theme,
                            ..Default::default()
                        },
                        Services::default(),
                        &[],
                    );
                    app.exec(&egui::Context::default(), Cmd::New);
                    app.mode = mode;
                    let before = app.doc().unwrap().doc.workbook().clone();
                    let revision = app.doc().unwrap().doc.state_id();
                    let mut h = harness(app, size);
                    let tabs: Vec<_> = ["Style", "Data", "Map"]
                        .map(|label| h.get_by_role_and_label(Role::Tab, t(label)).rect())
                        .into_iter()
                        .collect();
                    assert!(tabs[0].right() <= tabs[1].left());
                    assert!(tabs[1].right() <= tabs[2].left());
                    for rect in tabs {
                        assert!(h.ctx.content_rect().contains_rect(rect), "{rect:?}");
                    }
                    assert!(h.query_by_label(t("Task Planning")).is_none());
                    assert!(h.query_by_label(t("Hyperlink")).is_none());
                    assert!(h.query_by_label(t("Insert image")).is_none());
                    h.get_by_role_and_label(Role::Tab, t("Data")).click();
                    h.run_steps(4);
                    assert_eq!(h.state().format_tab, FormatTab::Data);
                    h.get_by_role_and_label(Role::Button, t("Insert image"));
                    h.get_by_role_and_label(Role::TextInput, t("Hyperlink"));
                    assert!(
                        h.query_all_by_role_and_label(Role::Button, t("Edit labels"))
                            .next()
                            .is_some()
                    );
                    h.get_by_role_and_label(Role::MultilineTextInput, t("Notes"));
                    h.get_by_role_and_label(Role::Button, t("Edit notes equation"));
                    let headings = ["Hyperlink", "Labels", "Notes", "Equation"]
                        .map(|label| h.get_by_role_and_label(Role::Label, t(label)).rect());
                    assert!(
                        headings
                            .windows(2)
                            .all(|pair| pair[1].top() > pair[0].bottom() + 8.0)
                    );
                    assert!(
                        h.get_by_role_and_label(Role::CheckBox, t("Markdown preview"))
                            .rect()
                            .bottom()
                            < headings[3].top(),
                        "Preview belongs to Notes, before the separate Equation group"
                    );
                    assert!(h.query_by_label(t("Topic Properties")).is_none());
                    let title = &h.state().doc().unwrap().doc.sheet().root_topic().title;
                    assert!(
                        h.query_all_by_role_and_label(Role::Label, title).count() <= 1,
                        "Data must not repeat the topic heading"
                    );
                    h.get_by_role_and_label(Role::Button, t("Task Planning"));
                    assert!(h.query_by_label(t("Quick Styles")).is_none());
                    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                    assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);
                }
            }
        }
    }
}

#[test]
fn data_task_summary_opens_the_existing_planning_editor_without_changing_the_map() {
    let _language = LANGUAGE.lock().unwrap();
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        for theme in [ThemeMode::Light, ThemeMode::Dark] {
            let mut app = ThinodaApp::headless();
            app.mode = mode;
            app.settings.theme = theme;
            app.format_tab = FormatTab::Data;
            let doc = app.doc_mut().unwrap();
            let root = doc.doc.sheet().root;
            doc.doc
                .set_task(
                    root,
                    Some(thinoda_core::Task {
                        start_date: Some("2026-10-26".parse().unwrap()),
                        duration_days: Some(5),
                        progress_percent: 40,
                        ..Default::default()
                    }),
                )
                .unwrap();
            let before = doc.doc.workbook().clone();
            let mut h = harness(app, egui::vec2(1280.0, 1000.0));
            for label in [
                format!("{}: 40%", t("Progress")),
                format!("{}: 2026-10-26", t("Start date")),
                format!("{}: 2026-10-30", t("Due date")),
            ] {
                h.get_by_label(&label);
            }
            h.get_by_role_and_label(Role::Button, t("Task Planning"))
                .scroll_to_me();
            h.run_steps(4);
            h.get_by_role_and_label(Role::Button, t("Task Planning"))
                .click();
            h.run_steps(4);
            h.get_by_role_and_label(Role::Button, t("Apply"));
            h.get_by_role_and_label(Role::Button, t("Cancel")).click();
            h.run_steps(4);
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        }
    }
}

#[test]
fn attachment_remains_a_separate_data_group_before_hyperlink() {
    let _language = LANGUAGE.lock().unwrap();
    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        let mut app = ThinodaApp::headless();
        app.settings.theme = theme;
        app.format_tab = FormatTab::Data;
        let doc = app.doc_mut().unwrap();
        let root = doc.doc.sheet().root;
        let attachment = doc
            .doc
            .edit_sheet("Fixture", |sheet| {
                thinoda_core::attachments::insert(sheet, root, "evidence.bin", b"opaque".to_vec())
            })
            .unwrap();
        doc.view.select_only(attachment);
        let mut h = harness(app, egui::vec2(1280.0, 1000.0));
        let attachment = h
            .get_by_role_and_label(Role::DisclosureTriangle, t("Attachment"))
            .rect();
        let hyperlink = h.get_by_role_and_label(Role::Label, t("Hyperlink")).rect();
        assert!(hyperlink.top() > attachment.bottom() + 16.0);
        h.get_by_role_and_label(Role::Button, t("Save/Download attachment"));
        h.get_by_role_and_label(Role::Tab, t("Style")).click();
        h.run_steps(4);
        assert!(
            h.query_by_role_and_label(Role::Button, t("Save/Download attachment"))
                .is_none()
        );
    }
}

#[test]
fn switching_from_data_commits_the_owned_draft_once_and_preserves_scroll_positions() {
    let _language = LANGUAGE.lock().unwrap();
    for mode in [CenterMode::Map, CenterMode::Outliner] {
        let mut app = ThinodaApp::headless();
        app.mode = mode;
        app.format_tab = FormatTab::Data;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tab-switch.thinoda");
        app.doc_mut().unwrap().save_to(&path, false).unwrap();
        let mut h = harness(app, egui::vec2(1100.0, 500.0));
        h.get_by_role_and_label(Role::MultilineTextInput, t("Notes"))
            .focus();
        h.run_steps(6);
        h.get_by_role_and_label(Role::MultilineTextInput, t("Notes"))
            .type_text("Owned Data draft");
        h.run_steps(4);
        let scrolled = h
            .get_by_role_and_label(Role::TextInput, t("Hyperlink"))
            .rect();
        h.get_by_role_and_label(Role::Tab, t("Style")).click();
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
            Some("Owned Data draft")
        );
        h.get_by_role_and_label(Role::Slider, t("Line Width"))
            .scroll_to_me();
        h.run_steps(8);
        let style_scroll = h
            .get_by_role_and_label(Role::Slider, t("Line Width"))
            .rect();
        h.get_by_role_and_label(Role::Tab, t("Data")).click();
        h.run_steps(6);
        assert_eq!(
            h.get_by_role_and_label(Role::TextInput, t("Hyperlink"))
                .rect()
                .top(),
            scrolled.top()
        );
        h.get_by_role_and_label(Role::Tab, t("Map")).click();
        h.run_steps(4);
        h.get_by_role_and_label(Role::Tab, t("Style")).click();
        h.run_steps(4);
        assert_eq!(
            h.get_by_role_and_label(Role::Slider, t("Line Width"))
                .rect()
                .top(),
            style_scroll.top()
        );
        let ctx = h.ctx.clone();
        h.state_mut().exec(&ctx, Cmd::Save);
        h.state_mut()
            .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Undo));
        h.run_steps(4);
        assert_eq!(
            h.state().doc().unwrap().doc.sheet().root_topic().notes,
            None
        );
        h.state_mut()
            .exec(&ctx, Cmd::Canvas(thinoda_canvas::Action::Redo));
        h.run_steps(4);
        assert_eq!(
            thinoda_app::doc::OpenDoc::open(&path)
                .unwrap()
                .doc
                .sheet()
                .root_topic()
                .notes
                .as_deref(),
            Some("Owned Data draft")
        );
    }
}
