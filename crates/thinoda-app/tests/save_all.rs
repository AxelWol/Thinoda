use std::path::Path;

use egui::{ViewportCommand, accesskit::Role};
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{CloseIntent, Cmd, Services, ThinodaApp},
    doc::OpenDoc,
};
use thinoda_core::Workbook;
use thinoda_io::{ExportOptions, Format};
use thinoda_platform::settings::{Language, Settings};

fn fixture(directory: &Path, formats: &[Format]) -> ThinodaApp {
    let mut app = ThinodaApp::new(
        Settings {
            language: Language::English,
            backup_on_save: false,
            ..Default::default()
        },
        Services::default(),
        &[],
    );
    for (index, format) in formats.iter().enumerate() {
        app.new_doc(Workbook::default());
        let doc = app.doc_mut().unwrap();
        doc.save_to(
            &directory.join(format!("map-{index}.{}", format.extensions()[0])),
            false,
        )
        .unwrap();
        let root = doc.doc.sheet().root;
        doc.doc
            .edit_sheet("Title", |sheet| {
                sheet.set_title(root, &format!("Edited {index}"))
            })
            .unwrap();
    }
    app.active = 0;
    app
}

fn external_change(app: &ThinodaApp, index: usize) -> Vec<u8> {
    let doc = &app.docs[index];
    let mut book = doc.doc.workbook().clone();
    let sheet = book.active_mut();
    sheet
        .set_title(
            sheet.root,
            &format!("External version {index}: {}", "different ".repeat(40)),
        )
        .unwrap();
    let bytes = thinoda_io::export_bytes(&book, doc.format, &ExportOptions::default()).unwrap();
    std::fs::write(doc.path.as_ref().unwrap(), &bytes).unwrap();
    assert!(doc.changed_on_disk());
    bytes
}

fn start(mut app: ThinodaApp, quit: bool, home: bool) -> Harness<'static, ThinodaApp> {
    if home {
        app.exec(&egui::Context::default(), Cmd::Home);
    }
    if quit {
        app.dialogs.close = Some(CloseIntent::Quit);
    } else {
        app.exec(&egui::Context::default(), Cmd::SaveAll);
    }
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    h.run_steps(4);
    if quit {
        choose(&mut h, "thinoda_close", "Save");
    }
    h
}

fn choose(h: &mut Harness<'_, ThinodaApp>, dialog: &str, label: &str) -> Vec<ViewportCommand> {
    let area = h
        .ctx
        .memory(|memory| memory.area_rect(egui::Id::unique(dialog)))
        .unwrap();
    h.query_all_by_role_and_label(Role::Button, label)
        .find(|button| area.contains_rect(button.rect()))
        .expect("dialog choice")
        .click();
    let mut commands = Vec::new();
    for _ in 0..4 {
        h.step();
        commands.extend(
            h.output().viewport_output[&egui::ViewportId::ROOT]
                .commands
                .clone(),
        );
    }
    commands
}

fn assert_saved(app: &ThinodaApp) {
    assert!(!app.any_dirty());
    assert!(app.command_available(&Cmd::SaveAll));
    assert!(app.dialogs.conflict.is_none());
    assert!(app.dialogs.mindmanager_save.is_none());
    for doc in &app.docs {
        assert_eq!(
            OpenDoc::open(doc.path.as_ref().unwrap())
                .unwrap()
                .doc
                .workbook(),
            doc.doc.workbook()
        );
    }
}

#[test]
fn save_all_pauses_resumes_and_only_quits_after_every_document_is_saved() {
    for quit in [false, true] {
        for home in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let app = fixture(dir.path(), &[Format::Thinoda, Format::Thinoda]);
            let keys: Vec<_> = app.docs.iter().map(|doc| doc.key.clone()).collect();
            let external: Vec<_> = (0..2).map(|index| external_change(&app, index)).collect();
            let mut h = start(app, quit, home);
            assert_eq!(
                h.state().dialogs.conflict.as_deref(),
                Some(keys[0].as_str())
            );
            assert_eq!(h.state().active, if home { 2 } else { 0 });
            for (doc, bytes) in h.state().docs.iter().zip(&external) {
                assert_eq!(std::fs::read(doc.path.as_ref().unwrap()).unwrap(), *bytes);
            }
            for command in [
                Cmd::Save,
                Cmd::SaveAs,
                Cmd::SaveAll,
                Cmd::Export(Format::MindManager),
                Cmd::Quit,
                Cmd::CloseTab(0),
            ] {
                assert!(!h.state().command_available(&command));
                let ctx = h.ctx.clone();
                h.state_mut().exec(&ctx, command);
                assert_eq!(
                    h.state().dialogs.conflict.as_deref(),
                    Some(keys[0].as_str())
                );
                assert_eq!(h.state().docs.len(), 2);
            }
            let ctx = egui::Context::default();
            let mut input = egui::RawInput::default();
            input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .events
                .push(egui::ViewportEvent::Close);
            let mut output = ctx.run_ui(input, |ui| h.state_mut().handle_close_request(ui.ctx()));
            output.textures_delta.clear();
            assert!(
                output.viewport_output[&egui::ViewportId::ROOT]
                    .commands
                    .contains(&ViewportCommand::CancelClose)
            );
            assert!(h.state().dialogs.close.is_none());

            // Conflict identity and original-tab restoration survive a reorder.
            h.state_mut().docs.swap(0, 1);
            let output = choose(&mut h, "thinoda_conflict", "Overwrite");
            assert!(!output.contains(&ViewportCommand::Close));
            assert_eq!(
                h.state().dialogs.conflict.as_deref(),
                Some(keys[1].as_str())
            );
            assert_eq!(h.state().active, if home { 2 } else { 1 });
            let output = choose(&mut h, "thinoda_conflict", "Overwrite");
            assert_eq!(output.contains(&ViewportCommand::Close), quit);
            assert_saved(h.state());
            assert_eq!(h.state().active, if home { 2 } else { 1 });
        }
    }

    for quit in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let app = fixture(
            dir.path(),
            &[Format::Thinoda, Format::MindManager, Format::Thinoda],
        );
        let first = app.docs[0].key.clone();
        let middle = app.docs[1].key.clone();
        external_change(&app, 0);
        let last_path = app.docs[2].path.clone().unwrap();
        let last_bytes = std::fs::read(&last_path).unwrap();
        let mut h = start(app, quit, true);
        assert_eq!(h.state().dialogs.conflict.as_deref(), Some(first.as_str()));
        assert!(!choose(&mut h, "thinoda_conflict", "Overwrite").contains(&ViewportCommand::Close));
        assert_eq!(
            h.state().dialogs.mindmanager_save.as_ref().unwrap().0,
            middle
        );
        assert_eq!(std::fs::read(&last_path).unwrap(), last_bytes);
        assert_eq!(h.state().active, 3);
        let output = choose(&mut h, "mindmanager_save", "Save");
        assert_eq!(output.contains(&ViewportCommand::Close), quit);
        assert_saved(h.state());
        assert_eq!(h.state().active, 3);
    }

    // A fresh external modification during MindManager consent returns to the
    // conflict decision and still retains the batch and its pending Quit.
    let dir = tempfile::tempdir().unwrap();
    let app = fixture(dir.path(), &[Format::MindManager, Format::Thinoda]);
    let mut h = start(app, true, true);
    let key = h.state().docs[0].key.clone();
    external_change(h.state(), 0);
    let output = choose(&mut h, "mindmanager_save", "Save");
    assert!(!output.contains(&ViewportCommand::Close));
    assert_eq!(h.state().dialogs.conflict.as_deref(), Some(key.as_str()));
    assert!(!choose(&mut h, "thinoda_conflict", "Overwrite").contains(&ViewportCommand::Close));
    assert!(h.state().dialogs.mindmanager_save.is_some());
    let output = choose(&mut h, "mindmanager_save", "Save");
    assert!(output.contains(&ViewportCommand::Close));
    assert_saved(h.state());

    // Cancelling after Overwrite also revokes that attempt's overwrite consent.
    let dir = tempfile::tempdir().unwrap();
    let app = fixture(dir.path(), &[Format::MindManager, Format::Thinoda]);
    external_change(&app, 0);
    let mut h = start(app, true, true);
    choose(&mut h, "thinoda_conflict", "Overwrite");
    assert!(h.state().docs[0].ignore_conflict);
    assert!(!choose(&mut h, "mindmanager_save", "Cancel").contains(&ViewportCommand::Close));
    assert!(!h.state().docs[0].ignore_conflict);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::SaveAll);
    h.run_steps(4);
    assert!(h.state().dialogs.conflict.is_some());
    assert!(h.state().dialogs.mindmanager_save.is_none());

    // Autosave must not write later documents while a decision is pending.
    let dir = tempfile::tempdir().unwrap();
    let mut app = fixture(dir.path(), &[Format::Thinoda, Format::Thinoda]);
    external_change(&app, 0);
    let later = app.docs[1].path.clone().unwrap();
    let bytes = std::fs::read(&later).unwrap();
    app.settings.autosave = true;
    app.settings.autosave_interval_secs = 5;
    let mut h = start(app, false, true);
    std::thread::sleep(std::time::Duration::from_millis(5100));
    h.run_steps(4);
    assert!(h.state().docs[1].is_dirty());
    assert_eq!(std::fs::read(later).unwrap(), bytes);

    // A document added outside the original queue must still veto pending Quit.
    let dir = tempfile::tempdir().unwrap();
    let app = fixture(dir.path(), &[Format::Thinoda, Format::Thinoda]);
    external_change(&app, 0);
    let mut h = start(app, true, true);
    h.state_mut().new_doc(Workbook::default());
    h.state_mut().doc_mut().unwrap().doc.mark_dirty();
    let output = choose(&mut h, "thinoda_conflict", "Overwrite");
    assert!(!output.contains(&ViewportCommand::Close));
    assert!(h.state().command_available(&Cmd::SaveAll));
    assert!(!h.state().docs[0].is_dirty() && !h.state().docs[1].is_dirty());
    assert!(h.state().docs[2].is_dirty());
    assert_eq!(h.state().active, 3);

    for conflict in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let format = if conflict {
            Format::Thinoda
        } else {
            Format::MindManager
        };
        let app = fixture(dir.path(), &[Format::Thinoda, format, Format::Thinoda]);
        let pending_path = app.docs[1].path.clone().unwrap();
        let last_path = app.docs[2].path.clone().unwrap();
        let pending_bytes = if conflict {
            external_change(&app, 1)
        } else {
            std::fs::read(&pending_path).unwrap()
        };
        let last_bytes = std::fs::read(&last_path).unwrap();
        let mut h = start(app, true, true);
        assert!(!h.state().docs[0].is_dirty());
        assert!(h.state().docs[1].is_dirty() && h.state().docs[2].is_dirty());
        let output = choose(
            &mut h,
            if conflict {
                "thinoda_conflict"
            } else {
                "mindmanager_save"
            },
            "Cancel",
        );
        assert!(!output.contains(&ViewportCommand::Close));
        assert!(h.state().command_available(&Cmd::SaveAll));
        assert!(h.state().dialogs.conflict.is_none());
        assert!(h.state().dialogs.mindmanager_save.is_none());
        assert_eq!(h.state().active, 3);
        assert_eq!(std::fs::read(&pending_path).unwrap(), pending_bytes);
        assert_eq!(std::fs::read(&last_path).unwrap(), last_bytes);
        assert!(h.state().docs[1].is_dirty() && h.state().docs[2].is_dirty());
    }

    // An I/O failure aborts explicitly without proceeding to later documents.
    let dir = tempfile::tempdir().unwrap();
    let app = fixture(dir.path(), &[Format::Thinoda, Format::Thinoda]);
    let broken = app.docs[0].path.clone().unwrap();
    let later = app.docs[1].path.clone().unwrap();
    let bytes = std::fs::read(&later).unwrap();
    std::fs::remove_file(&broken).unwrap();
    std::fs::create_dir(&broken).unwrap();
    let mut h = start(app, true, true);
    let output = choose(&mut h, "thinoda_conflict", "Overwrite");
    assert!(!output.contains(&ViewportCommand::Close));
    let app = h.state();
    assert!(app.command_available(&Cmd::SaveAll));
    assert!(app.docs.iter().all(OpenDoc::is_dirty));
    assert!(!app.docs[0].ignore_conflict);
    assert!(
        app.toasts
            .texts()
            .any(|text| text.contains("Could not save"))
    );
    assert_eq!(std::fs::read(later).unwrap(), bytes);
    assert_eq!(app.active, 2);

    // Stale dialog identities never target whatever document now occupies its index.
    let dir = tempfile::tempdir().unwrap();
    let app = fixture(dir.path(), &[Format::Thinoda, Format::Thinoda]);
    external_change(&app, 0);
    let later = app.docs[1].path.clone().unwrap();
    let bytes = std::fs::read(&later).unwrap();
    let mut h = start(app, true, true);
    h.state_mut().docs.remove(0);
    let output = choose(&mut h, "thinoda_conflict", "Overwrite");
    assert!(!output.contains(&ViewportCommand::Close));
    assert!(h.state().command_available(&Cmd::SaveAll));
    assert_eq!(h.state().docs.len(), 1);
    assert!(h.state().docs[0].is_dirty());
    assert_eq!(std::fs::read(later).unwrap(), bytes);
    assert_eq!(h.state().active, 1);
}
