//! Busy indicator for document opens and saves: work runs while frames keep painting,
//! and documents only change state once the file work really completed.
use std::{
    path::Path,
    time::{Duration, Instant},
};

use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{Cmd, ThinodaApp},
    doc::OpenDoc,
    file_ops::{FileBusy, FileWorkMode},
};
use thinoda_core::Workbook;
use thinoda_io::{ExportOptions, Format};
use thinoda_platform::settings::{Language, Settings};

/// A clean native model whose root is titled `title`.
fn write_model(path: &Path, title: &str) {
    let mut book = Workbook::default();
    let root = book.active().root;
    book.sheets[0].set_title(root, title).unwrap();
    let bytes =
        thinoda_io::export_bytes(&book, Format::Thinoda, &ExportOptions::default()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn app() -> ThinodaApp {
    let mut app = ThinodaApp::new(
        Settings {
            language: Language::English,
            backup_on_save: false,
            autosave: false,
            ..Default::default()
        },
        thinoda_app::app::Services::default(),
        &[],
    );
    app.set_file_work_mode(FileWorkMode::Background);
    app
}

fn harness(app: ThinodaApp) -> Harness<'static, ThinodaApp> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1200.0, 860.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(2);
    h
}

/// Steps until no file work is left; panics instead of hanging.
fn step_until_idle(h: &mut Harness<'_, ThinodaApp>) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !h.state().file_activity().is_empty() {
        assert!(Instant::now() < deadline, "file work never finished");
        h.step();
        std::thread::sleep(Duration::from_millis(2));
    }
    h.run_steps(2);
}

/// A document saved to `path` with an unsaved title edit.
fn edited_doc(app: &mut ThinodaApp, path: &Path, title: &str) {
    app.new_doc(Workbook::default());
    let doc = app.doc_mut().unwrap();
    doc.save_to(path, false).unwrap();
    rename_root(doc, title);
    assert!(doc.is_dirty());
}

fn rename_root(doc: &mut OpenDoc, title: &str) {
    let root = doc.doc.sheet().root;
    doc.doc
        .edit_sheet("Title", |sheet| sheet.set_title(root, title))
        .unwrap();
}

fn saved_root(path: &Path) -> String {
    let book = thinoda_io::import_file(path).unwrap();
    let sheet = book.active();
    sheet.topic(sheet.root).unwrap().title.clone()
}

fn has_toast(h: &Harness<'_, ThinodaApp>, text: &str) -> bool {
    h.state().toasts.texts().any(|toast| toast.contains(text))
}

#[test]
fn opening_paints_progress_before_documents_appear_and_reports_failures() {
    let dir = tempfile::tempdir().unwrap();
    let good = dir.path().join("big.thinoda");
    write_model(&good, "Opened root");
    let missing = dir.path().join("missing.opml");
    let mut h = harness(app());
    h.state().pause_file_workers(true);
    h.state_mut().request_open(&good);
    h.state_mut().request_open(&missing);
    // Requests for a file that is already queued do not open it twice.
    h.state_mut().request_open(&good);
    h.run_steps(4);
    assert_eq!(
        h.state().file_activity(),
        vec![FileBusy::Opening {
            index: 1,
            total: 2,
            name: "big.thinoda".into(),
        }]
    );
    h.get_by_label("Opening file: big.thinoda");
    h.get_by_label("1 / 2");
    assert!(
        h.state()
            .docs
            .iter()
            .all(|doc| doc.path.as_deref() != Some(good.as_path())),
        "no document before the import completed"
    );
    h.state().pause_file_workers(false);
    step_until_idle(&mut h);
    let opened: Vec<_> = h
        .state()
        .docs
        .iter()
        .filter(|doc| doc.path.as_deref() == Some(good.as_path()))
        .collect();
    assert_eq!(opened.len(), 1);
    assert!(!opened[0].is_dirty());
    assert!(has_toast(&h, "Could not open"));
    assert!(h.query_by_label("Opening file: big.thinoda").is_none());
    // Already open: activated right away, nothing queued.
    h.state_mut().active = h.state().docs.len();
    h.state_mut().request_open(&good);
    assert!(h.state().file_activity().is_empty());
    assert_eq!(
        h.state().doc().unwrap().path.as_deref(),
        Some(good.as_path())
    );
}

#[test]
fn deferred_opening_runs_only_after_the_indicator_was_presented() {
    let dir = tempfile::tempdir().unwrap();
    let good = dir.path().join("deferred.thinoda");
    write_model(&good, "Deferred root");
    let mut app = app();
    app.set_file_work_mode(FileWorkMode::Deferred);
    let mut h = harness(app);
    h.state_mut().request_open(&good);
    // The overlay area sizes itself invisibly in its first frame; the work waits for that.
    h.run_steps(2);
    assert!(h.state().docs.iter().all(|doc| doc.path.is_none()));
    assert_eq!(h.state().file_activity().len(), 1);
    h.get_by_label("Opening file: deferred.thinoda");
    step_until_idle(&mut h);
    assert!(
        h.state()
            .docs
            .iter()
            .any(|doc| doc.path.as_deref() == Some(good.as_path()))
    );
}

#[test]
fn save_shows_progress_and_only_marks_the_written_state_saved() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("model.thinoda");
    let mut app = app();
    edited_doc(&mut app, &path, "Written");
    let mut h = harness(app);
    h.state().pause_file_workers(true);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::Save);
    h.run_steps(4);
    assert_eq!(
        h.state().file_activity(),
        vec![FileBusy::Saving {
            name: "model.thinoda".into()
        }]
    );
    h.get_by_label("Saving file: model.thinoda");
    assert!(
        h.state().doc().unwrap().is_dirty(),
        "not saved before the write"
    );
    assert_ne!(saved_root(&path), "Written");
    for cmd in [
        Cmd::Save,
        Cmd::SaveAs,
        Cmd::SaveAll,
        Cmd::Quit,
        Cmd::CloseTab(0),
    ] {
        assert!(!h.state().command_available(&cmd), "{cmd:?} while saving");
    }
    // Editing stays possible; the newer content must not be reported as saved.
    rename_root(h.state_mut().doc_mut().unwrap(), "Edited while saving");
    h.state().pause_file_workers(false);
    step_until_idle(&mut h);
    assert_eq!(saved_root(&path), "Written");
    assert!(h.state().doc().unwrap().is_dirty());
    assert!(has_toast(&h, "Saved to"));
    assert!(h.query_by_label("Saving file: model.thinoda").is_none());
    assert!(h.state().command_available(&Cmd::Save));
    h.state_mut().exec(&ctx, Cmd::Save);
    step_until_idle(&mut h);
    assert_eq!(saved_root(&path), "Edited while saving");
    assert!(!h.state().doc().unwrap().is_dirty());
}

#[test]
fn failed_save_keeps_the_document_and_file_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("blocked.thinoda");
    let mut app = app();
    edited_doc(&mut app, &path, "Not written");
    let original = std::fs::read(&path).unwrap();
    let backup = thinoda_platform::fs::backup_path(&path);
    std::fs::create_dir(&backup).unwrap();
    std::fs::write(backup.join("keep"), b"blocker").unwrap();
    app.settings.backup_on_save = true;
    let mut h = harness(app);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::Save);
    step_until_idle(&mut h);
    assert!(h.state().doc().unwrap().is_dirty());
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert!(has_toast(&h, "Could not save"));
    assert!(!has_toast(&h, "Saved to"));
}

#[test]
fn notes_draft_typed_during_a_save_stays_unsaved_work() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.thinoda");
    let mut app = ThinodaApp::headless();
    app.set_file_work_mode(FileWorkMode::Background);
    app.settings.autosave = false;
    app.format_tab = thinoda_app::app::FormatTab::Data;
    app.doc_mut().unwrap().save_to(&path, false).unwrap();
    let mut h = harness(app);
    let notes = egui::accesskit::Role::MultilineTextInput;
    h.get_by_role_and_label(notes, "Notes").focus();
    h.run_steps(2);
    h.get_by_role_and_label(notes, "Notes")
        .type_text("First draft");
    h.run_steps(2);
    h.state().pause_file_workers(true);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::Save);
    h.run_steps(4);
    assert!(h.state().doc().unwrap().is_dirty());
    h.get_by_role_and_label(notes, "Notes")
        .type_text(" and more");
    h.run_steps(2);
    h.state().pause_file_workers(false);
    step_until_idle(&mut h);
    let doc = h.state().doc().unwrap();
    assert!(doc.is_dirty(), "the newer draft is unsaved work");
    let saved = thinoda_io::import_file(&path).unwrap();
    let sheet = saved.active();
    let note = format!("{:?}", sheet.topic(sheet.root).unwrap().notes);
    assert!(note.contains("First draft"));
    assert!(!note.contains("and more"));
    h.state_mut().exec(&ctx, Cmd::Save);
    step_until_idle(&mut h);
    assert!(!h.state().doc().unwrap().is_dirty());
}

#[test]
fn close_tab_and_save_all_wait_for_background_writes() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.thinoda");
    let second = dir.path().join("second.thinoda");
    let mut app = app();
    edited_doc(&mut app, &first, "First");
    edited_doc(&mut app, &second, "Second");
    let mut h = harness(app);
    h.state().pause_file_workers(true);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::SaveAll);
    h.run_steps(4);
    assert_eq!(
        h.state().file_activity(),
        vec![FileBusy::Saving {
            name: "first.thinoda".into()
        }]
    );
    assert!(h.state().docs.iter().all(OpenDoc::is_dirty));
    h.state().pause_file_workers(false);
    step_until_idle(&mut h);
    assert!(!h.state().any_dirty());
    assert_eq!(saved_root(&first), "First");
    assert_eq!(saved_root(&second), "Second");

    rename_root(h.state_mut().doc_mut().unwrap(), "Closing");
    let index = h.state().active;
    h.state_mut().request_close_tab(index);
    h.run_steps(4);
    h.state().pause_file_workers(true);
    let area = h
        .ctx
        .memory(|memory| memory.area_rect(egui::Id::unique("thinoda_close")))
        .unwrap();
    h.query_all_by_role_and_label(egui::accesskit::Role::Button, "Save")
        .find(|button| area.contains_rect(button.rect()))
        .expect("close dialog Save")
        .click();
    h.run_steps(4);
    assert_eq!(h.state().docs.len(), 2, "tab stays open while writing");
    assert!(h.state().docs[index].is_dirty());
    h.state().pause_file_workers(false);
    step_until_idle(&mut h);
    assert_eq!(h.state().docs.len(), 1, "tab closes once written");
    assert_eq!(saved_root(&second), "Closing");
}
