use thinoda_io::{
    ExportOptions, Format, ImportReport, export_bytes, import_bytes, import_bytes_with_report,
};

pub const MAX_INPUT_SIZE: usize = 1024 * 1024;

pub fn roundtrip(format: Format, bytes: &[u8]) {
    if bytes.len() > MAX_INPUT_SIZE {
        return;
    }
    if let Ok(report) = import_bytes_with_report(format, bytes) {
        roundtrip_report(report);
    }
}

pub fn roundtrip_report(report: ImportReport) {
    let workbook = report.workbook;
    workbook
        .validate()
        .expect("imported workbook must validate");
    let native = export_bytes(&workbook, Format::Thinoda, &ExportOptions::default())
        .expect("validated workbook must export to native format");
    let decoded = import_bytes(Format::Thinoda, &native)
        .expect("native export of a validated workbook must be importable");
    assert_eq!(
        workbook, decoded,
        "native round-trip must preserve the workbook"
    );
}
