//! End-to-end tests: drive the real application UI headlessly with egui_kittest,
//! exercise file round-trips, crash recovery and the command-line interface.

use std::path::Path;
use std::sync::Arc;

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use ymind_app::app::{CloseIntent, Cmd, FocusField, FormatTab, Services, SidebarFamily, YMindApp};
use ymind_core::Workbook;
use ymind_io::Format;
use ymind_platform::paths::Paths;
use ymind_platform::recovery::RecoveryManager;
use ymind_platform::settings::{Language, Settings, ThemeMode as Theme};

fn harness(app: YMindApp) -> Harness<'static, YMindApp> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .build_ui_state(|ui, app: &mut YMindApp| app.draw(ui), app);
    // Symbol coverage depends on installed system fonts; render tofu instead of panicking.
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    h.run_steps(3);
    h
}

fn root_children(app: &YMindApp) -> usize {
    let s = app.doc().unwrap().doc.sheet();
    s.children(s.root).len()
}

fn english() -> Settings {
    Settings {
        language: Language::English,
        ..Settings::default()
    }
}

fn rename_root(app: &mut YMindApp, title: &str) {
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

#[test]
fn map_panel_themes_fit_and_can_be_selected() {
    let mut h = harness(YMindApp::headless());
    h.get_by_role_and_label(Role::Tab, "Map").click();
    h.run_steps(4);
    assert_eq!(h.state().format_tab, FormatTab::Map);
    let map_tab = h.get_by_role_and_label(Role::Tab, "Map").rect();
    assert!(h.state().doc().unwrap().view.canvas_rect().max.x <= map_tab.min.x);
    for id in ymind_core::theme::Theme::IDS {
        let theme = ymind_core::theme::Theme::builtin(id);
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
        .exec(&ctx, Cmd::Canvas(ymind_canvas::Action::Undo));
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.sheet().theme, before);
}

#[test]
fn map_tab_stays_reachable_after_scrolling_style_at_different_scales() {
    for (width, scale) in [(640.0, 1.0), (1280.0, 1.0), (1280.0, 1.5)] {
        let app = YMindApp::headless();
        let mut h = Harness::builder()
            .with_size(egui::vec2(width, 800.0))
            .build_ui_state(|ui, app: &mut YMindApp| app.draw(ui), app);
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
        for id in ymind_core::theme::Theme::IDS {
            let theme = ymind_core::theme::Theme::builtin(id);
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
    let mut h = harness(YMindApp::headless());
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
    let mut h = harness(YMindApp::headless());
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
        .exec(&ctx, Cmd::Canvas(ymind_canvas::Action::Undo));
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
    let mut h = harness(YMindApp::headless());
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
fn home_templates_wrap_within_the_window() {
    for width in [480.0, 800.0, 1280.0, 2400.0] {
        let mut app = YMindApp::headless();
        app.active = app.docs.len();
        let mut h = Harness::builder()
            .with_size(egui::vec2(width, 1600.0))
            .build_ui_state(|ui, app: &mut YMindApp| app.draw(ui), app);
        h.ctx
            .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
        h.run_steps(4);
        let first = h.get_by_role_and_label(Role::Button, "Mind Map").rect();
        let last = h.get_by_role_and_label(Role::Button, "Project Plan").rect();
        for template in ymind_core::templates::TEMPLATES {
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
            .click();
        h.run_steps(3);
        assert!(h.state().doc().is_some());
    }
}

#[test]
fn format_panel_can_be_hidden_and_restored_repeatedly() {
    let mut h = harness(YMindApp::headless());
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
    let mut h = harness(YMindApp::headless());
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
    let mut h = harness(YMindApp::headless());
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
    let mut h = harness(YMindApp::headless());
    let before = root_children(h.state());
    let ctx = h.ctx.clone();
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(ymind_canvas::Action::InsertChild));
    assert_eq!(root_children(h.state()), before + 1);
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(ymind_canvas::Action::Undo));
    assert_eq!(root_children(h.state()), before);
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(ymind_canvas::Action::Redo));
    assert_eq!(root_children(h.state()), before + 1);
}

#[test]
fn closing_dirty_tab_asks_and_dont_save_closes() {
    let mut app = YMindApp::headless();
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
fn outliner_and_zen_toggle() {
    let mut h = harness(YMindApp::headless());
    h.get_by_role_and_label(Role::Button, "Outliner").click();
    h.run_steps(2);
    assert_eq!(h.state().mode, ymind_app::app::CenterMode::Outliner);
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
    let mut app = YMindApp::headless();
    rename_root(&mut app, "Quarterly Roadmap");
    let mut h = harness(app);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::Find);
    h.state_mut().search.query = "roadmap".into();
    h.run_steps(2);
    let root = h.state().doc().unwrap().doc.sheet().root;
    assert_eq!(h.state().search.hits, vec![root]);
}

#[test]
fn search_icon_opens_accessible_search_and_can_reopen() {
    let mut app = YMindApp::headless();
    rename_root(&mut app, "Quarterly Roadmap");
    let mut h = harness(app);
    for _ in 0..3 {
        h.get_by_role_and_label(Role::Button, "Find…").click();
        h.run_steps(3);
        assert!(h.get_by_label("Search").is_focused());
        h.get_by_label("Search").type_text("roadmap");
        h.run_steps(2);
        let root = h.state().doc().unwrap().doc.sheet().root;
        assert_eq!(h.state().search.hits, vec![root]);
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
    let mut app = YMindApp::headless();
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
        .exec(&ctx, Cmd::Canvas(ymind_canvas::Action::Undo));
    assert_eq!(
        root_title(h.state().doc().unwrap().doc.workbook()),
        "Quarterly Roadmap"
    );
}

#[test]
fn search_refreshes_when_switching_between_documents_at_the_same_revision() {
    let mut app = YMindApp::headless();
    rename_root(&mut app, "Roadmap");
    let mut h = harness(app);
    h.state_mut().search.query = "Roadmap".into();
    h.run_steps(2);
    let old_root = h.state().doc().unwrap().doc.sheet().root;
    assert_eq!(h.state().search.hits, vec![old_root]);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::New);
    rename_root(h.state_mut(), "Different document");
    h.run_steps(2);
    assert!(h.state().search.hits.is_empty());
}

#[test]
fn search_refreshes_when_switching_sheets_without_an_edit() {
    let mut app = YMindApp::headless();
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
fn contextual_toolbar_follows_selection_and_edits_topic_properties() {
    let mut h = harness(YMindApp::headless());
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
    let mut h = harness(YMindApp::headless());
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

fn right_click_at(h: &mut Harness<'static, YMindApp>, pos: egui::Pos2) {
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
fn contextual_menu_disables_root_parent_and_blank_canvas_topic_actions() {
    let mut h = harness(YMindApp::headless());
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
    let mut h = harness(YMindApp::headless());
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
    let path = dir.path().join("Plan.ymind");
    let mut app = YMindApp::headless();
    rename_root(&mut app, "Launch Plan");
    let d = app.doc_mut().unwrap();
    d.save_to(&path, false).unwrap();
    assert!(!d.is_dirty());

    let app2 = YMindApp::new(english(), Services::default(), std::slice::from_ref(&path));
    assert_eq!(app2.docs.len(), 1);
    let d2 = app2.doc().unwrap();
    assert_eq!(d2.title(), "Plan");
    assert!(!d2.is_dirty());
    assert_eq!(root_title(d2.doc.workbook()), "Launch Plan");
}

#[test]
fn xmind_export_reimports_and_foreign_import_is_dirty() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = YMindApp::headless();
    rename_root(&mut app, "Interop");
    let wb = app.doc().unwrap().doc.workbook().clone();
    for format in [
        Format::XMind,
        Format::FreeMind,
        Format::Opml,
        Format::Markdown,
    ] {
        let p = dir.path().join(format!("map.{}", format.extensions()[0]));
        let bytes = ymind_io::export_bytes(&wb, format, &Default::default()).unwrap();
        std::fs::write(&p, bytes).unwrap();
        let mut a = YMindApp::new(english(), Services::default(), &[]);
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
        let mut app = YMindApp::new(english(), services, &[]);
        app.new_doc(ymind_core::templates::instantiate("mind-map"));
        rename_root(&mut app, "Unsaved Idea");
        app.doc_mut().unwrap().snapshot(&recovery);
        // Simulate a crash: the panic hook flushes and marks the session for preservation.
        assert!(recovery.try_flush_for_crash());
        drop(app);
    }

    let orphans = RecoveryManager::find_orphans(&paths);
    assert_eq!(orphans.len(), 1, "crashed session is found on next start");
    let mut app = YMindApp::new(english(), Services::default(), &[]);
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

fn ymind_bin() -> &'static str {
    env!("CARGO_BIN_EXE_ymind")
}

#[test]
fn cli_version_and_help() {
    let out = std::process::Command::new(ymind_bin())
        .arg("--version")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains(env!("CARGO_PKG_VERSION")));
    let out = std::process::Command::new(ymind_bin())
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
        let st = std::process::Command::new(ymind_bin())
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
    let wb = ymind_io::import_file(Path::new(&dir.path().join("out.xmind"))).unwrap();
    assert_eq!(root_title(&wb), "Project");

    let st = std::process::Command::new(ymind_bin())
        .arg("--convert")
        .arg(dir.path().join("missing.md"))
        .arg(dir.path().join("x.opml"))
        .status()
        .unwrap();
    assert!(!st.success(), "missing input fails with non-zero exit code");
}

#[test]
fn ctrl_k_and_ctrl_shift_n_focus_hyperlink_and_notes() {
    let mut h = harness(YMindApp::headless());
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
    let mut h = harness(YMindApp::headless());
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

fn ai_harness(reply: &str) -> (Harness<'static, YMindApp>, ymind_ai::mock::MockServer) {
    let server = ymind_ai::mock::MockServer::start(&["text-embedding-x", "mock-llm"], reply);
    let mut app = YMindApp::headless();
    app.settings.ai.base_url = server.base_url.clone();
    let mut h = harness(app);
    let ctx = h.ctx.clone();
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(ymind_canvas::Action::SelectRoot));
    (h, server)
}

fn wait_for_ai(h: &mut Harness<'static, YMindApp>) {
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
        .exec(&ctx, Cmd::Canvas(ymind_canvas::Action::Undo));
    assert_eq!(root_children(h.state()), before);
}

#[test]
fn ai_breakdown_creates_todos_and_explain_sets_notes() {
    let (mut h, _server) = ai_harness("1. Plan\n2. Build");
    let ctx = h.ctx.clone();
    h.state_mut()
        .exec(&ctx, Cmd::Ai(ymind_app::ai::AiCommand::Breakdown));
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
        .exec(&ctx, Cmd::Ai(ymind_app::ai::AiCommand::Explain));
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
        .exec(&ctx, Cmd::Ai(ymind_app::ai::AiCommand::GenerateMap));
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
    let mut app = YMindApp::headless();
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    app.settings.ai.base_url = format!("http://127.0.0.1:{port}/v1");
    let mut h = harness(app);
    let ctx = h.ctx.clone();
    h.state_mut()
        .exec(&ctx, Cmd::Canvas(ymind_canvas::Action::SelectRoot));
    let before = root_children(h.state());
    h.state_mut()
        .exec(&ctx, Cmd::Ai(ymind_app::ai::AiCommand::Brainstorm));
    wait_for_ai(&mut h);
    assert_eq!(root_children(h.state()), before);
}
