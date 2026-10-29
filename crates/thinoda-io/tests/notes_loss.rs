use thinoda_core::{Sheet, Workbook};
use thinoda_io::{
    ExportOptions, Format, IoError, export_bytes, export_bytes_with_consent, export_file,
    import_bytes, note_format_loss,
};

fn book(source: &str) -> Workbook {
    let mut sheet = Sheet::new("Notes", "Research");
    sheet
        .set_markdown_notes(sheet.root, Some(source.into()))
        .unwrap();
    Workbook::new(sheet)
}

#[test]
fn csv_and_opml_require_separate_explicit_note_format_consent_before_writing() {
    let book = book("**café**, \"日本語\"\n\n3. First\tTab\r\n4. Second & <literal>");
    let before = book.clone();
    let directory = tempfile::tempdir().unwrap();
    for format in [Format::Csv, Format::Opml] {
        let path = directory
            .path()
            .join(format!("notes.{}", format.extensions()[0]));
        std::fs::write(&path, b"existing").unwrap();
        assert!(note_format_loss(&book, format, &Default::default()));
        assert!(matches!(
            export_file(&book, &path, format, &Default::default()),
            Err(IoError::NoteFormatLossNeedsConfirmation)
        ));
        assert_eq!(std::fs::read(&path).unwrap(), b"existing");
        assert!(matches!(
            export_bytes_with_consent(
                &book,
                format,
                &ExportOptions {
                    allow_symbol_color_loss: true,
                    ..Default::default()
                },
                true
            ),
            Err(IoError::NoteFormatLossNeedsConfirmation)
        ));
        let options = ExportOptions {
            allow_note_format_loss: true,
            ..Default::default()
        };
        let bytes = export_bytes(&book, format, &options).unwrap();
        if format == Format::Opml {
            let imported = import_bytes(format, &bytes).unwrap();
            assert_eq!(
                imported.active().root_topic().notes.as_deref(),
                book.active().root_topic().notes.as_deref()
            );
            assert!(
                !imported
                    .active()
                    .root_topic()
                    .notes
                    .as_ref()
                    .unwrap()
                    .is_markdown()
            );
        } else {
            let text = String::from_utf8(bytes).unwrap();
            assert!(text.contains(
                "\"**café**, \"\"日本語\"\"\n\n3. First\tTab\r\n4. Second & <literal>\""
            ));
        }
        assert_eq!(book, before);
    }
}

#[test]
fn detection_follows_selected_reachable_tree_and_ignores_literal_notes() {
    let mut book = book("**Formatted**");
    book.sheets.push(Sheet::new("Plain", "Other"));
    let selected = ExportOptions {
        sheet: Some(1),
        ..Default::default()
    };
    assert!(!note_format_loss(&book, Format::Csv, &selected));
    book.active_sheet = 1;
    let floating = book
        .active_mut()
        .add_floating("Excluded", thinoda_core::Vec2::ZERO);
    book.active_mut()
        .set_markdown_notes(floating, Some("**Excluded**".into()))
        .unwrap();
    assert!(!note_format_loss(&book, Format::Csv, &Default::default()));
    let root = book.active().root;
    book.active_mut().topic_mut(root).unwrap().notes = Some("**Literal**".into());
    assert!(!note_format_loss(&book, Format::Opml, &Default::default()));
    assert!(export_bytes(&book, Format::Opml, &Default::default()).is_ok());
}

#[test]
fn formats_with_note_support_do_not_require_this_opt_in() {
    let book = book("**Supported**");
    for format in [
        Format::Thinoda,
        Format::XMind,
        Format::Markdown,
        Format::TextBundle,
        Format::Word,
        Format::Excel,
        Format::FreeMind,
    ] {
        assert!(!note_format_loss(&book, format, &Default::default()));
        assert!(
            export_bytes(&book, format, &Default::default()).is_ok(),
            "{format:?}"
        );
    }
}

#[test]
fn opml_retains_plain_note_attribute_whitespace_without_requiring_consent() {
    let mut book = Workbook::default();
    let root = book.active().root;
    let source = "Literal **text**\twith CR\rLF\nand CRLF\r\n& <xml> \"quotes\"";
    book.active_mut()
        .set_notes(root, Some(source.into()))
        .unwrap();
    let bytes = export_bytes(&book, Format::Opml, &Default::default()).unwrap();
    let imported = import_bytes(Format::Opml, &bytes).unwrap();
    assert_eq!(
        imported.active().root_topic().notes.as_deref(),
        Some(source)
    );
}
