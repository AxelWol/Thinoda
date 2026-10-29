use thinoda_core::{Workbook, clipboard, equations, notes::NoteContent, outline};
use thinoda_io::{
    ExportOptions, Format, IoError, export_bytes, export_bytes_with_consent, import_bytes,
};

fn book() -> Workbook {
    let mut book = Workbook::default();
    let root = book.active().root;
    let source =
        equations::replace_note("**Research**\r\n", Some("\\frac{a}{b}+x_{i}^{2}")).unwrap();
    book.active_mut()
        .set_markdown_notes(root, Some(source))
        .unwrap();
    book
}

#[test]
fn equations_native_clipboard_outline_and_masking_preserve_or_remove_complete_source() {
    let book = book();
    let bytes = export_bytes(&book, Format::Thinoda, &Default::default()).unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_reader(archive.by_name("manifest.json").unwrap()).unwrap();
    assert_eq!(manifest["version"], 4);
    assert_eq!(import_bytes(Format::Thinoda, &bytes).unwrap(), book);
    let payload = clipboard::encode_sheet(book.active()).unwrap();
    assert!(payload.starts_with("THINODA-SHEET-3\n"));
    assert_eq!(clipboard::decode_sheet(&payload).unwrap(), *book.active());
    assert!(clipboard::decode_sheet(&payload.replacen("SHEET-3", "SHEET-2", 1)).is_err());
    let copied = clipboard::copy_topics(book.active(), &[book.active().root]).unwrap();
    let pasted = clipboard::resolve_paste(Some(&copied), Some(&copied.text));
    assert_eq!(pasted[0].topics[0].notes, book.active().root_topic().notes);
    let outline = outline::to_outline(book.active(), book.active().root);
    let restored = outline::sheet_from_outline("copy", &outline);
    assert_eq!(
        restored.root_topic().notes,
        book.active().root_topic().notes
    );
    let masked = book.masked_copy();
    assert!(!equations::sheet_has_equations(masked.active()));
    assert_eq!(masked.active().root_topic().notes, None);
    assert!(equations::sheet_has_equations(book.active()));
}

#[test]
fn equations_all_foreign_exports_fail_even_with_loss_consent_before_destination_replacement() {
    let book = book();
    let before = book.clone();
    let opts = ExportOptions {
        allow_note_format_loss: true,
        allow_symbol_color_loss: true,
        ..Default::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("existing-output");
    std::fs::write(&path, b"keep existing output").unwrap();
    for &format in Format::EXPORTABLE {
        if format == Format::Thinoda {
            continue;
        }
        assert!(
            matches!(
                export_bytes_with_consent(&book, format, &opts, true),
                Err(IoError::EquationNotesUnsupported)
            ),
            "{format:?}"
        );
        assert!(
            matches!(
                thinoda_io::export_file(&book, &path, format, &opts),
                Err(IoError::EquationNotesUnsupported)
            ),
            "{format:?}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"keep existing output");
    }
    assert_eq!(book, before);
}

#[test]
fn equations_direct_print_and_presentation_exports_cannot_bypass_guards() {
    let mut book = book();
    let root = book.active().root;
    let branch = book.active_mut().add_child(root, None, "Branch");
    book.active_mut()
        .add_presentation_slide(
            "Slide",
            thinoda_core::PresentationSource::Branch(branch),
            thinoda_core::Rect::from_min_size(
                thinoda_core::Vec2::ZERO,
                thinoda_core::Vec2::new(300.0, 200.0),
            ),
        )
        .unwrap();
    let opts = ExportOptions::default();
    let sheet = book.active();
    assert!(matches!(
        thinoda_io::print::render_print(&book, 0, Default::default()),
        Err(IoError::EquationNotesUnsupported)
    ));
    assert!(matches!(
        thinoda_io::render_presentation_slide_svg(sheet, 0, &opts),
        Err(IoError::EquationNotesUnsupported)
    ));
    assert!(matches!(
        thinoda_io::render_presentation_slide_png(sheet, 0, &opts),
        Err(IoError::EquationNotesUnsupported)
    ));
    assert!(matches!(
        thinoda_io::presentation::render_presentation_pptx_slide_png(sheet, 0, &opts),
        Err(IoError::EquationNotesUnsupported)
    ));
    assert!(matches!(
        thinoda_io::export_presentation_pptx(sheet, &opts),
        Err(IoError::EquationNotesUnsupported)
    ));
    assert!(matches!(
        thinoda_io::export_presentation_pdf(sheet, &opts),
        Err(IoError::EquationNotesUnsupported)
    ));
    assert!(matches!(
        thinoda_io::export_presentation_png_zip(sheet, &opts),
        Err(IoError::EquationNotesUnsupported)
    ));
    assert!(matches!(
        thinoda_io::render_gantt(sheet),
        Err(IoError::EquationNotesUnsupported)
    ));
    assert!(matches!(
        thinoda_io::export_gantt_csv(sheet),
        Err(IoError::EquationNotesUnsupported)
    ));
}

#[test]
fn equations_invalid_imported_source_stays_lossless_and_plain_legacy_notes_stay_literal() {
    let mut book = book();
    let root = book.active().root;
    let malformed = "```thinoda-math-v1\n\\input{not-executed}\n```\n";
    book.active_mut()
        .set_notes(root, Some(malformed.into()))
        .unwrap();
    let native = export_bytes(&book, Format::Thinoda, &Default::default()).unwrap();
    assert_eq!(import_bytes(Format::Thinoda, &native).unwrap(), book);
    assert!(matches!(
        export_bytes(&book, Format::Markdown, &Default::default()),
        Err(IoError::EquationNotesUnsupported)
    ));
    book.active_mut().topic_mut(root).unwrap().notes = Some(NoteContent::Plain(malformed.into()));
    assert!(!equations::sheet_has_equations(book.active()));
    assert!(export_bytes(&book, Format::Markdown, &Default::default()).is_ok());
}

#[test]
fn equations_export_guard_covers_unselected_sheets_and_floating_topics() {
    let mut book = Workbook::default();
    let mut other = thinoda_core::Sheet::new("Other", "Root");
    let floating = other.add_floating("Hidden evidence", thinoda_core::Vec2::new(900.0, 900.0));
    other
        .set_markdown_notes(floating, Some("unfinished thinoda-math-v1".into()))
        .unwrap();
    book.sheets.push(other);
    let opts = ExportOptions {
        sheet: Some(0),
        ..Default::default()
    };
    assert!(matches!(
        export_bytes(&book, Format::Svg, &opts),
        Err(IoError::EquationNotesUnsupported)
    ));
    assert!(export_bytes(&book, Format::Thinoda, &opts).is_ok());
}
