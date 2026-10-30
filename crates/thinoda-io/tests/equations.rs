use std::io::Read;
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
fn equations_export_as_vectors_or_refuse_unrepresentable_formats_atomically() {
    let book = book();
    let opts = ExportOptions {
        allow_note_format_loss: true,
        allow_symbol_color_loss: true,
        ..Default::default()
    };
    let svg = export_bytes_with_consent(&book, Format::Svg, &opts, true).unwrap();
    let svg = String::from_utf8(svg).unwrap();
    assert!(svg.contains("class=\"notes-equations\""));
    assert!(svg.contains(">Notes equations</text>"));
    assert!(svg.contains(">a</text>") && svg.contains(">b</text>"));
    assert!(svg.contains("<path d=\"M ") && svg.contains("stroke-width=\"1.2\""));
    assert!(!svg.contains("thinoda-math-v1"));
    assert!(!svg.contains("\\frac"));

    let png = export_bytes_with_consent(&book, Format::Png, &opts, true).unwrap();
    assert!(image::load_from_memory(&png).unwrap().width() > 0);
    let pdf = export_bytes_with_consent(&book, Format::Pdf, &opts, true).unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
    assert!(!String::from_utf8_lossy(&pdf).contains("/Subtype /Image"));

    let xmind = export_bytes_with_consent(&book, Format::XMind, &opts, true).unwrap();
    assert_eq!(import_bytes(Format::XMind, &xmind).unwrap(), book);

    // CSV and OPML retain the exact canonical source bytes. Their existing
    // Markdown-format-loss consent still applies; OPML imports it as literal text.
    let source = book.active().root_topic().notes.as_deref().unwrap();
    assert!(matches!(
        export_bytes(&book, Format::Opml, &Default::default()),
        Err(IoError::NoteFormatLossNeedsConfirmation)
    ));
    let opml = export_bytes_with_consent(&book, Format::Opml, &opts, true).unwrap();
    let imported = import_bytes(Format::Opml, &opml).unwrap();
    assert_eq!(
        imported.active().root_topic().notes.as_deref(),
        Some(source)
    );
    let csv = export_bytes_with_consent(&book, Format::Csv, &opts, true).unwrap();
    assert!(
        String::from_utf8(csv)
            .unwrap()
            .contains("```thinoda-math-v1")
    );

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("existing-output");
    std::fs::write(&path, b"keep existing output").unwrap();
    for format in [
        Format::FreeMind,
        Format::Markdown,
        Format::PlainText,
        Format::MindManager,
        Format::TextBundle,
        Format::Word,
        Format::Excel,
    ] {
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
}

#[test]
fn equations_are_visible_in_direct_print_and_presentation_vector_exports() {
    let mut book = book();
    let root = book.active().root;
    let branch = book.active_mut().add_child(root, None, "Branch");
    let branch_equation = equations::replace_note("", Some("z^{3}")).unwrap();
    book.active_mut()
        .set_markdown_notes(branch, Some(branch_equation))
        .unwrap();
    let hidden = book.active_mut().add_child(branch, None, "Hidden");
    let hidden_equation = equations::replace_note("", Some("q^{9}")).unwrap();
    book.active_mut()
        .set_markdown_notes(hidden, Some(hidden_equation))
        .unwrap();
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
    book.active_mut().presentation.slides[0].hidden_topics = vec![hidden];
    let opts = ExportOptions::default();
    let sheet = book.active();
    let page = thinoda_io::print::render_print(&book, 0, Default::default()).unwrap();
    assert!(page.image.pixels().any(|pixel| pixel.0 != [255; 3]));
    let svg = thinoda_io::render_presentation_slide_svg(sheet, 0, &opts).unwrap();
    assert!(svg.contains("class=\"notes-equations\""));
    assert!(svg.contains(">z</text>"));
    assert!(!svg.contains(">a</text>"));
    assert!(!svg.contains(">q</text>"));
    let png = thinoda_io::render_presentation_slide_png(sheet, 0, &opts).unwrap();
    assert!(image::load_from_memory(&png).is_ok());
    let preview =
        thinoda_io::presentation::render_presentation_pptx_slide_png(sheet, 0, &opts).unwrap();
    assert!(image::load_from_memory(&preview).is_ok());
    let pptx = thinoda_io::export_presentation_pptx(sheet, &opts).unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(pptx)).unwrap();
    let mut slide = String::new();
    archive
        .by_name("ppt/slides/slide1.xml")
        .unwrap()
        .read_to_string(&mut slide)
        .unwrap();
    assert!(slide.contains("Notes equations"));
    assert!(slide.contains("<a:t>z</a:t>"));
    assert!(!slide.contains("<a:t>a</a:t>"));
    assert!(!slide.contains("<a:t>q</a:t>"));
    let pdf = thinoda_io::export_presentation_pdf(sheet, &opts).unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
    let zip = thinoda_io::export_presentation_png_zip(sheet, &opts).unwrap();
    assert!(zip.starts_with(b"PK"));

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
fn equations_invalid_imported_source_is_rejected_without_replacing_output() {
    let mut book = book();
    let root = book.active().root;
    let malformed = "```thinoda-math-v1\n\\input{not-executed}\n```\n";
    book.active_mut()
        .set_notes(root, Some(malformed.into()))
        .unwrap();
    let branch = book.active_mut().add_child(root, None, "Branch");
    book.active_mut()
        .add_presentation_slide(
            "Malformed source",
            thinoda_core::PresentationSource::Branch(branch),
            thinoda_core::Rect::from_min_size(
                thinoda_core::Vec2::ZERO,
                thinoda_core::Vec2::new(300.0, 200.0),
            ),
        )
        .unwrap();
    let native = export_bytes(&book, Format::Thinoda, &Default::default()).unwrap();
    assert_eq!(import_bytes(Format::Thinoda, &native).unwrap(), book);
    assert!(matches!(
        export_bytes(&book, Format::Svg, &Default::default()),
        Err(IoError::EquationNotesUnsupported)
    ));
    assert!(matches!(
        export_bytes(&book, Format::XMind, &Default::default()),
        Err(IoError::EquationNotesUnsupported)
    ));
    assert!(matches!(
        thinoda_io::print::render_print(&book, 0, Default::default()),
        Err(IoError::EquationNotesUnsupported)
    ));
    assert!(matches!(
        thinoda_io::render_presentation_slide_svg(book.active(), 0, &ExportOptions::default()),
        Err(IoError::EquationNotesUnsupported)
    ));
    assert!(matches!(
        thinoda_io::export_presentation_pptx(book.active(), &ExportOptions::default()),
        Err(IoError::EquationNotesUnsupported)
    ));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("existing-output");
    std::fs::write(&path, b"unchanged").unwrap();
    assert!(matches!(
        thinoda_io::export_file(&book, &path, Format::Svg, &Default::default()),
        Err(IoError::EquationNotesUnsupported)
    ));
    assert_eq!(std::fs::read(&path).unwrap(), b"unchanged");

    book.active_mut().topic_mut(root).unwrap().notes = Some(NoteContent::Plain(malformed.into()));
    assert!(!equations::sheet_has_equations(book.active()));
    assert!(export_bytes(&book, Format::Svg, &Default::default()).is_ok());
}

#[test]
fn equations_appendix_total_geometry_is_bounded_before_destination_replacement() {
    let mut book = Workbook::default();
    let root = book.active().root;
    for index in 0..70 {
        let topic = book
            .active_mut()
            .add_child(root, None, format!("Topic {index}"));
        let note = equations::replace_note("", Some("x")).unwrap();
        book.active_mut()
            .set_markdown_notes(topic, Some(note))
            .unwrap();
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("existing-output");
    std::fs::write(&path, b"still here").unwrap();
    assert!(matches!(
        thinoda_io::export_file(&book, &path, Format::Svg, &Default::default()),
        Err(IoError::EquationNotesUnsupported)
    ));
    assert_eq!(std::fs::read(&path).unwrap(), b"still here");
}

#[test]
fn equations_on_detached_topics_are_not_silently_omitted_from_outline_formats() {
    let mut book = Workbook::default();
    let floating = book
        .active_mut()
        .add_floating("Detached", thinoda_core::Vec2::new(900.0, 900.0));
    let note = equations::replace_note("", Some("x^{2}")).unwrap();
    book.active_mut()
        .set_markdown_notes(floating, Some(note))
        .unwrap();
    let opts = ExportOptions {
        allow_note_format_loss: true,
        ..Default::default()
    };
    for format in [Format::Opml, Format::Csv] {
        assert!(matches!(
            export_bytes(&book, format, &opts),
            Err(IoError::EquationNotesUnsupported)
        ));
    }
}

#[test]
fn equations_in_unexported_sheets_do_not_block_selected_sheet_vector_export() {
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
    assert!(export_bytes(&book, Format::Svg, &opts).is_ok());
    assert!(export_bytes(&book, Format::Thinoda, &opts).is_ok());
}
