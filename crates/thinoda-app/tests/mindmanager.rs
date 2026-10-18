use egui::{Key, accesskit::Role};
use egui_kittest::{
    Harness,
    kittest::{By, NodeT, Queryable},
};
use thinoda_app::{
    app::{Cmd, Services, ThinodaApp},
    doc::{OpenDoc, convert, convert_with_consent},
    i18n::t,
};
use thinoda_core::{Sheet, Workbook};
use thinoda_io::{ExportOptions, Format, export_bytes};
use thinoda_platform::settings::{Language, Settings};

const SOURCE: &str = r#"<ap:Map xmlns:ap="http://schemas.mindjet.com/MindManager/Application/2003"><ap:OneTopic><ap:Topic><ap:Text PlainText="Original"/><ap:AttachmentGroup/></ap:Topic></ap:OneTopic></ap:Map>"#;
static UI_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn import_consent_is_localized_cancelable_and_preserves_editor_and_source() {
    let _guard = UI_LOCK.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.xmmap");
    std::fs::write(&path, SOURCE).unwrap();
    for language in [
        Language::English,
        Language::German,
        Language::Spanish,
        Language::French,
        Language::Danish,
        Language::Dutch,
        Language::Portuguese,
        Language::PortugueseBrazil,
        Language::Italian,
        Language::Polish,
    ] {
        let mut app = ThinodaApp::new(
            Settings {
                language,
                ..Default::default()
            },
            Services::default(),
            &[],
        );
        app.exec(&egui::Context::default(), Cmd::New);
        let before = app.doc().unwrap().doc.workbook().clone();
        app.open_path(&path);
        assert_eq!(app.dialogs.imports.len(), 1);
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        let mut h = Harness::builder()
            .with_size(egui::vec2(1100.0, 800.0))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        h.run_steps(3);
        h.query_all(By::new().role(Role::Button).label(t("Cancel")))
            .find(|node| !node.accesskit_node().is_disabled())
            .unwrap()
            .click();
        h.run_steps(3);
        assert!(h.state().dialogs.imports.is_empty());
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        h.state_mut().open_path(&path);
        h.run_steps(3);
        h.key_press(Key::Insert);
        h.run_steps(3);
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        h.get_by_role_and_label(Role::Button, t("Import converted copy"))
            .click();
        h.run_steps(3);
        assert!(h.state().dialogs.imports.is_empty());
        let doc = h.state().doc().unwrap();
        assert!(doc.path.is_none());
        assert_eq!(
            doc.import_source.as_deref(),
            Some(path.canonicalize().unwrap().as_path())
        );
        assert!(doc.is_dirty());
        assert_eq!(doc.doc.sheet().root_topic().title, "Original");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), SOURCE);
    }
}

#[test]
fn saves_require_native_destination_and_cannot_replace_imported_source() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.mmap");
    let original = export_bytes(
        &Workbook::new(Sheet::new("Map", "Original")),
        Format::MindManager,
        &ExportOptions::default(),
    )
    .unwrap();
    std::fs::write(&source, &original).unwrap();
    let mut doc = OpenDoc::open(&source).unwrap();
    assert!(doc.path.is_none() && doc.is_dirty());
    assert!(doc.save_to(&source, false).is_err());
    assert!(doc.save_sheet_to(&source, false).is_err());
    assert_eq!(std::fs::read(&source).unwrap(), original);
    #[cfg(unix)]
    {
        let alias = dir.path().join("alias.mmap");
        std::os::unix::fs::symlink(&source, &alias).unwrap();
        assert!(doc.save_to(&alias, false).is_err());
        assert!(convert_with_consent(&source, &alias, &ExportOptions::default(), true).is_err());
        assert_eq!(std::fs::read(&source).unwrap(), original);
    }
    let bad = dir.path().join("disguised.xml");
    assert!(doc.save_to(&bad, false).is_err());
    assert!(!bad.exists());
    let copy = dir.path().join("copy.mmap");
    doc.save_to(&copy, false).unwrap();
    assert!(!doc.is_dirty());
    assert_eq!(
        OpenDoc::open(&copy).unwrap().doc.sheet().root_topic().title,
        "Original"
    );
    doc.doc
        .edit_book("another sheet", |book| {
            book.sheets.push(Sheet::new("Second", "Second"));
            Ok(())
        })
        .unwrap();
    assert!(doc.save_to(&copy, false).is_err());
    assert!(doc.is_dirty());
    let full = dir.path().join("complete.thinoda");
    doc.save_to(&full, false).unwrap();
    assert_eq!(OpenDoc::open(&full).unwrap().doc.workbook().sheets.len(), 2);
    assert_eq!(std::fs::read(&source).unwrap(), original);
}

#[test]
fn cli_conversion_requires_loss_consent_and_separate_destination() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source.xmmap");
    let output = dir.path().join("converted.mmap");
    std::fs::write(&input, SOURCE).unwrap();
    assert!(convert(&input, &output, &ExportOptions::default()).is_err());
    assert!(!output.exists());
    convert_with_consent(&input, &output, &ExportOptions::default(), true).unwrap();
    assert_eq!(
        OpenDoc::open(&output)
            .unwrap()
            .doc
            .sheet()
            .root_topic()
            .title,
        "Original"
    );
    assert!(convert_with_consent(&input, &input, &ExportOptions::default(), true).is_err());
    assert_eq!(std::fs::read_to_string(&input).unwrap(), SOURCE);
    let process_output = dir.path().join("cli.thinoda");
    let cli = std::process::Command::new(env!("CARGO_BIN_EXE_thinoda"))
        .args(["--convert"])
        .arg(&input)
        .arg(&process_output)
        .output()
        .unwrap();
    assert!(!cli.status.success() && !process_output.exists());
    let cli = std::process::Command::new(env!("CARGO_BIN_EXE_thinoda"))
        .args(["--convert"])
        .arg(&input)
        .arg(&process_output)
        .arg("--allow-lossy")
        .output()
        .unwrap();
    assert!(
        cli.status.success(),
        "{}",
        String::from_utf8_lossy(&cli.stderr)
    );
    assert!(process_output.exists());
    let mut book = Workbook::default();
    book.metadata.created = Some("2026-01-01T00:00:00Z".into());
    let full = dir.path().join("full.thinoda");
    std::fs::write(
        &full,
        export_bytes(&book, Format::Thinoda, &ExportOptions::default()).unwrap(),
    )
    .unwrap();
    let mapped = dir.path().join("export.mmap");
    assert!(
        convert(&full, &mapped, &ExportOptions::default())
            .unwrap_err()
            .contains("--allow-lossy")
    );
    assert!(!mapped.exists());
    convert_with_consent(&full, &mapped, &ExportOptions::default(), true).unwrap();
}

#[test]
fn export_scope_warning_is_cancelable_and_does_not_modify_source() {
    let _guard = UI_LOCK.lock().unwrap();
    let mut app = ThinodaApp::new(Settings::default(), Services::default(), &[]);
    app.exec(&egui::Context::default(), Cmd::New);
    let before = app.doc().unwrap().doc.workbook().clone();
    app.exec(&egui::Context::default(), Cmd::Export(Format::MindManager));
    assert!(app.dialogs.mindmanager_export);
    let mut h = Harness::builder().build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, t("Cancel")).click();
    h.run_steps(3);
    assert!(!h.state().dialogs.mindmanager_export);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
}

fn click_enabled(h: &mut Harness<'_, ThinodaApp>, label: &str) {
    h.query_all(By::new().role(Role::Button).label(label))
        .find(|node| !node.accesskit_node().is_disabled())
        .unwrap()
        .click();
    h.run_steps(3);
}

#[test]
fn native_save_confirmation_cancel_accept_and_close_are_safe() {
    let _guard = UI_LOCK.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("copy.mmap");
    let mut app = ThinodaApp::new(Settings::default(), Services::default(), &[]);
    app.docs.clear();
    app.new_doc(Workbook::default());
    app.doc_mut().unwrap().save_to(&path, false).unwrap();
    let original = std::fs::read(&path).unwrap();
    let root = app.doc().unwrap().doc.sheet().root;
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("title", |sheet| sheet.set_title(root, "Edited"))
        .unwrap();
    assert!(!app.save_active(false));
    let mut h = Harness::builder().build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(3);
    click_enabled(&mut h, t("Cancel"));
    assert!(h.state().doc().unwrap().is_dirty());
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert!(!h.state_mut().save_active(false));
    h.run_steps(3);
    click_enabled(&mut h, t("Save"));
    assert!(!h.state().doc().unwrap().is_dirty());
    assert_eq!(
        OpenDoc::open(&path).unwrap().doc.sheet().root_topic().title,
        "Edited"
    );
    h.state_mut()
        .doc_mut()
        .unwrap()
        .doc
        .edit_sheet("title", |sheet| sheet.set_title(root, "Close safely"))
        .unwrap();
    h.state_mut().request_close_tab(0);
    h.run_steps(3);
    click_enabled(&mut h, t("Save"));
    assert!(h.state().dialogs.mindmanager_save.is_some());
    click_enabled(&mut h, t("Save"));
    assert!(h.state().docs.is_empty());
    assert_eq!(
        OpenDoc::open(&path).unwrap().doc.sheet().root_topic().title,
        "Close safely"
    );
}

#[test]
fn sheet_copy_confirms_scope_without_retargeting_and_full_multi_sheet_save_is_refused() {
    let _guard = UI_LOCK.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("single.mmap");
    let sheet_copy = dir.path().join("sheet.mmap");
    let mut app = ThinodaApp::new(Settings::default(), Services::default(), &[]);
    app.docs.clear();
    app.new_doc(Workbook::default());
    app.doc_mut().unwrap().save_to(&path, false).unwrap();
    let original = std::fs::read(&path).unwrap();
    app.doc_mut()
        .unwrap()
        .doc
        .edit_book("sheet", |book| {
            book.sheets.push(Sheet::new("Second", "Second"));
            book.active_sheet = 1;
            Ok(())
        })
        .unwrap();
    let before = app.doc().unwrap().doc.workbook().clone();
    assert!(!app.save_sheet_to(&sheet_copy));
    let mut h = Harness::builder().build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(3);
    assert!(!sheet_copy.exists());
    click_enabled(&mut h, t("Save"));
    assert_eq!(
        OpenDoc::open(&sheet_copy)
            .unwrap()
            .doc
            .sheet()
            .root_topic()
            .title,
        "Second"
    );
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    assert_eq!(
        h.state().doc().unwrap().path.as_deref(),
        Some(path.as_path())
    );
    assert!(h.state().doc().unwrap().is_dirty());
    assert!(!h.state_mut().save_active(false));
    h.run_steps(3);
    click_enabled(&mut h, t("Save"));
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert!(h.state().doc().unwrap().is_dirty());
}
