use thinoda_app::app::ThinodaApp;
use thinoda_core::{Sheet, Workbook};
use thinoda_io::{Format, ImportWarning, export_bytes};

#[test]
fn supported_notes_open_editably_while_lossy_notes_still_require_cancelable_consent() {
    let directory = tempfile::tempdir().unwrap();
    let supported = directory.path().join("supported.mm");
    let lossy = directory.path().join("lossy.mm");
    let mut sheet = Sheet::new("Notes", "Imported");
    sheet
        .set_markdown_notes(
            sheet.root,
            Some("**Bold**\n\n3. First\n\n   - Nested".into()),
        )
        .unwrap();
    let bytes = export_bytes(&Workbook::new(sheet), Format::FreeMind, &Default::default()).unwrap();
    std::fs::write(&supported, &bytes).unwrap();
    let lossy_bytes = String::from_utf8(bytes.clone())
        .unwrap()
        .replace("<b>", "<b style=\"color:red\">");
    std::fs::write(&lossy, &lossy_bytes).unwrap();
    let mut app = ThinodaApp::headless();
    app.open_path(&supported);
    assert!(app.dialogs.imports.is_empty());
    let doc = app.doc().unwrap();
    assert!(doc.is_dirty());
    assert!(doc.path.is_none());
    assert!(
        doc.doc
            .sheet()
            .root_topic()
            .notes
            .as_ref()
            .unwrap()
            .is_markdown()
    );
    let before = doc.doc.workbook().clone();
    app.open_path(&lossy);
    assert_eq!(app.dialogs.imports.len(), 1);
    assert_eq!(
        app.dialogs.imports.front().unwrap().2,
        vec![ImportWarning::Formatting]
    );
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    app.resolve_import(false);
    assert!(app.dialogs.imports.is_empty());
    assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    app.open_path(&lossy);
    app.resolve_import(true);
    assert!(app.dialogs.imports.is_empty());
    let doc = app.doc().unwrap();
    assert!(doc.is_dirty());
    assert!(doc.path.is_none());
    let notes = doc.doc.sheet().root_topic().notes.as_ref().unwrap();
    assert!(!notes.is_markdown());
    assert_eq!(&**notes, "Bold\nFirst\nNested");
    assert_eq!(std::fs::read(&supported).unwrap(), bytes);
    assert_eq!(std::fs::read_to_string(&lossy).unwrap(), lossy_bytes);
}
