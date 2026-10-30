//! Read-only local corpus check. Does not copy or print source document content.
use thinoda_io::{ExportOptions, Format, fim};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut count = 0usize;
    let mut args = std::env::args_os().skip(1).peekable();
    let validator = match args.peek().and_then(|a| a.to_str()) {
        Some("--xdf2") => {
            args.next();
            let archive = std::fs::read(args.next().ok_or("missing XDF2 schema archive")?)?;
            Some(fim::ExchangeValidator::from_official_archives(
                fim::Dialect::Xdf2,
                &archive,
                None,
            )?)
        }
        Some("--xdf3") => {
            args.next();
            let archive = std::fs::read(args.next().ok_or("missing XDF3 schema archive")?)?;
            let supplement = std::fs::read(args.next().ok_or("missing XDF3 supplement")?)?;
            Some(fim::ExchangeValidator::from_official_archives(
                fim::Dialect::Xdf3,
                &archive,
                Some(&supplement),
            )?)
        }
        _ => None,
    };
    for path in args {
        let bytes = std::fs::read(path)?;
        let book = fim::import_document(&bytes)?;
        book.validate()?;
        assert_eq!(fim::export_document(book.active())?, bytes);
        let native = thinoda_io::export_bytes(&book, Format::Thinoda, &ExportOptions::default())?;
        let reopened = thinoda_io::import_bytes(Format::Thinoda, &native)?;
        assert_eq!(reopened, book);
        assert_eq!(fim::export_document(reopened.active())?, bytes);
        let model = book.active().fim.as_ref().unwrap();
        let mut edited_check = None;
        if matches!(
            model.topic_metadata(book.active().root).map(|m| m.kind),
            Some(
                thinoda_core::fim::FimKind::Schema
                    | thinoda_core::fim::FimKind::Group
                    | thinoda_core::fim::FimKind::Field
            )
        ) {
            let mut document = thinoda_core::Document::new(book.clone());
            document.edit_sheet("Corpus rename", |sheet| {
                sheet
                    .topic_mut(sheet.root)
                    .unwrap()
                    .title
                    .push_str(" (local test)");
                Ok(())
            })?;
            let edited = fim::export_document(document.sheet())?;
            if let Some(validator) = &validator {
                let original_valid = validator.validate(&bytes)?.is_exchange_conformant();
                let edited_valid = validator.validate(&edited)?.is_exchange_conformant();
                if original_valid {
                    assert!(
                        edited_valid,
                        "editing a conformant name broke exchange-XSD conformance"
                    );
                }
                if original_valid
                    && document
                        .sheet()
                        .fim
                        .as_ref()
                        .unwrap()
                        .diagnostics()
                        .iter()
                        .all(|d| d.severity != thinoda_core::fim::DiagnosticSeverity::Error)
                {
                    assert_eq!(fim::export_validated(document.sheet(), validator)?, edited);
                }
                edited_check = Some((original_valid, edited_valid));
            }
            let native = thinoda_io::export_bytes(
                document.workbook(),
                Format::Thinoda,
                &ExportOptions::default(),
            )?;
            let reopened = thinoda_io::import_bytes(Format::Thinoda, &native)?;
            assert_eq!(reopened, *document.workbook());
            assert_eq!(fim::export_document(reopened.active())?, edited);
            assert!(document.undo());
            assert_eq!(fim::export_document(document.sheet())?, bytes);
        }
        println!(
            "case={} dialect={:?} message={:?} bytes={} topics={} objects={} diagnostics={} codes={:?} exact=true native=true edit_xsd={edited_check:?}",
            count + 1,
            model.dialect,
            model.message_kind,
            bytes.len(),
            book.active().topic_count(),
            model.records().len(),
            model.diagnostics().len(),
            model
                .diagnostics()
                .iter()
                .map(|d| d.code)
                .collect::<Vec<_>>()
        );
        count += 1;
    }
    if count == 0 {
        return Err("supply one or more local XML corpus paths".into());
    }
    println!("{count} lossless/native corpus cases passed");
    Ok(())
}
