//! End-to-end tests: drive the real application UI headlessly with egui_kittest,
//! exercise file round-trips, crash recovery and the command-line interface.

use std::path::Path;
use std::sync::Arc;

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use ymind_app::app::{CloseIntent, Cmd, FocusField, Services, YMindApp};
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
        h.query_by_role_and_label(Role::Button, "Outliner")
            .is_none(),
        "chrome hidden in Zen mode"
    );
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
