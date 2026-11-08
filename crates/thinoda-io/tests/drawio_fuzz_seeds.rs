#[path = "../../../fuzz/fuzz_targets/drawio_support.rs"]
mod drawio_support;
#[path = "../../../fuzz/fuzz_targets/support.rs"]
mod support;

use thinoda_io::{Format, import_bytes_with_report, import_drawio_link};

const PLAIN: &[u8] = include_bytes!("../../../fuzz/corpus/drawio_import/plain");
const WARNING: &[u8] = include_bytes!("../../../fuzz/corpus/drawio_import/warning");
const COMPRESSED: &[u8] = include_bytes!("../../../fuzz/corpus/drawio_import/compressed");
const SVG: &[u8] = include_bytes!("../../../fuzz/corpus/drawio_import/svg");
const HTML: &[u8] = include_bytes!("../../../fuzz/corpus/drawio_import/html");
const LINK: &[u8] = include_bytes!("../../../fuzz/corpus/drawio_import/link");
const COMPRESSED_LINK: &[u8] = include_bytes!("../../../fuzz/corpus/drawio_import/compressed-link");
const PNG: &[u8] = include_bytes!("../../../fuzz/corpus/drawio_import/png");

#[test]
fn document_seeds_reach_their_importers_and_native_roundtrip() {
    for (name, format, bytes) in [
        ("plain", Format::DrawIo, PLAIN),
        ("warning", Format::DrawIo, WARNING),
        ("compressed", Format::DrawIo, COMPRESSED),
        ("svg", Format::DrawIo, SVG),
        ("png", Format::DrawIo, PNG),
        ("html", Format::DrawIoHtml, HTML),
    ] {
        let report = import_bytes_with_report(format, bytes).expect(name);
        assert!(!report.workbook.sheets.is_empty(), "{name}");
        if name == "warning" {
            assert!(!report.warnings.is_empty());
        }
        support::roundtrip_report(report);
        drawio_support::exercise(bytes);
    }
}

#[test]
fn link_seeds_reach_their_importer_and_native_roundtrip() {
    for bytes in [LINK, COMPRESSED_LINK] {
        let link = std::str::from_utf8(bytes).unwrap();
        let report = import_drawio_link(link).unwrap();
        assert!(!report.workbook.sheets.is_empty());
        support::roundtrip_report(report);
        drawio_support::exercise(bytes);
    }
}

#[test]
fn arbitrary_and_oversized_inputs_do_not_escape_the_harness() {
    for bytes in [
        &b""[..],
        &b"\xff\x00<mxfile>"[..],
        &b"<!DOCTYPE mxfile [<!ENTITY x SYSTEM 'file:///secret'>]><mxfile/>"[..],
        &vec![b'x'; support::MAX_INPUT_SIZE + 1][..],
    ] {
        drawio_support::exercise(bytes);
    }
}
