use std::path::Path;
use thinoda_core::{
    Workbook,
    fim::{Dialect, FimChange, FimDocument},
};
use thinoda_io::{ExportOptions, Format, detect_format, export_bytes, fim, import_bytes};

fn source() -> Vec<u8> {
    format!("\u{feff}<?xml version='1.0'?>\n<x:xdatenfelder.datenfeld.0104 xmlns:x='{}'><x:header/><x:datenfeld><x:identifikation><x:id>F123</x:id><x:version>1.0.0</x:version></x:identifikation><x:name>Standalone</x:name><x:beschreibung><![CDATA[keep & all]]></x:beschreibung></x:datenfeld></x:xdatenfelder.datenfeld.0104>",Dialect::Xdf3.namespace()).into_bytes()
}
#[test]
fn import_detects_exact_root_and_dialect_not_filename() {
    let source = source();
    for path in ["field.xml", "field.xdf2", "field.xdf3", "field.xdf3.xml"] {
        assert_eq!(
            detect_format(Path::new(path), &source),
            Some(Format::FimXdf3)
        );
    }
    assert!(detect_format(Path::new("unknown.xdf3"), b"<not-fim/>").is_none());
    assert!(
        detect_format(
            Path::new("unknown.xml"),
            b"<root xmlns='urn:xoev-de:fim:standard:xdatenfelder_3.99'/>"
        )
        .is_none()
    );
    assert!(import_bytes(Format::FimXdf2, &source).is_err());
    let book = import_bytes(Format::FimXdf3, &source).unwrap();
    assert_eq!(book.active().topic_count(), 1);
    assert_eq!(fim::export_document(book.active()).unwrap(), source);
}
#[test]
fn native_v5_and_clipboard_v4_preserve_drafts_and_original_bytes() {
    let mut sheet = FimDocument::parse(&source()).unwrap().into_sheet().unwrap();
    let record = sheet
        .fim
        .as_ref()
        .unwrap()
        .record_for_topic(sheet.root)
        .unwrap();
    sheet
        .apply_fim_change(FimChange::RenameDefinition {
            record,
            name: "Draft".into(),
        })
        .unwrap();
    let book = Workbook::new(sheet);
    let bytes = export_bytes(&book, Format::Thinoda, &ExportOptions::default()).unwrap();
    let reopened = import_bytes(Format::Thinoda, &bytes).unwrap();
    assert_eq!(reopened, book);
    assert_eq!(
        reopened.active().fim.as_ref().unwrap().original_bytes(),
        source()
    );
    let clipboard = thinoda_core::clipboard::encode_sheet(reopened.active()).unwrap();
    assert!(clipboard.starts_with("THINODA-SHEET-4\n"));
    assert_eq!(
        thinoda_core::clipboard::decode_sheet(&clipboard).unwrap(),
        *reopened.active()
    );
    assert!(
        thinoda_core::clipboard::decode_sheet(&clipboard.replacen(
            "THINODA-SHEET-4",
            "THINODA-SHEET-3",
            1
        ))
        .is_err()
    );
    let archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut archive = archive;
    let manifest: serde_json::Value =
        serde_json::from_reader(archive.by_name("manifest.json").unwrap()).unwrap();
    assert_eq!(manifest["version"], 5);
}
#[test]
fn generic_xdf_export_cannot_bypass_gate_or_claim_conversion() {
    let book = fim::import_document(&source()).unwrap();
    for format in [Format::FimXdf2, Format::FimXdf3] {
        assert!(matches!(
            export_bytes(&book, format, &ExportOptions::default()),
            Err(thinoda_io::IoError::FimValidationRequired)
        ));
    }
    assert!(
        fim::assess_conversion(book.active(), Dialect::Xdf3)
            .unwrap()
            .supported
    );
    let conversion = fim::assess_conversion(book.active(), Dialect::Xdf2).unwrap();
    assert!(!conversion.supported && !conversion.reasons.is_empty());
    assert!(export_bytes(&book, Format::XMind, &ExportOptions::default()).is_err());
    let masked = book.masked_copy();
    let bytes = export_bytes(&masked, Format::Thinoda, &ExportOptions::default()).unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let content: serde_json::Value =
        serde_json::from_reader(archive.by_name("content.json").unwrap()).unwrap();
    assert!(!content.to_string().contains("keep & all"));
    assert!(fim::export_document(masked.active()).is_err());
}
