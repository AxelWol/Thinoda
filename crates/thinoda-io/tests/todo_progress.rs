use thinoda_core::Workbook;
use thinoda_io::{ExportOptions, Format, export_bytes, import_bytes};

#[test]
fn native_roundtrip_keeps_old_flags_and_recomputes_progress_without_serializing_badges() {
    let mut book = Workbook::default();
    let sheet = book.active_mut();
    let root = sheet.root;
    sheet.set_todo(root, Some(false)).unwrap();
    let child = sheet.add_child(root, None, "Done");
    sheet.set_todo(child, Some(true)).unwrap();
    let bytes = export_bytes(&book, Format::Thinoda, &ExportOptions::default()).unwrap();
    let restored = import_bytes(Format::Thinoda, &bytes).unwrap();
    assert_eq!(restored, book);
    assert!(restored.active().todo_progress()[&root].complete());
    assert_eq!(restored.active().topics[&root].todo, Some(false));
    assert!(restored.active().topics[&root].labels.is_empty());
}

#[test]
fn graphical_exports_include_derived_checklist_badges_without_modifying_sources() {
    let mut book = Workbook::default();
    let sheet = book.active_mut();
    sheet.set_todo(sheet.root, Some(false)).unwrap();
    for done in [false, true] {
        let child = sheet.add_child(sheet.root, None, "Item");
        sheet.set_todo(child, Some(done)).unwrap();
    }
    let before = book.clone();
    for dark in [false, true] {
        let options = ExportOptions {
            dark,
            ..Default::default()
        };
        let svg = export_bytes(&book, Format::Svg, &options).unwrap();
        assert!(String::from_utf8(svg).unwrap().contains(">1/2</text>"));
        for format in [Format::Png, Format::Pdf] {
            assert!(!export_bytes(&book, format, &options).unwrap().is_empty());
        }
        assert_eq!(book, before);
    }
}
