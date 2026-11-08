#[path = "../../../fuzz/fuzz_targets/drawio_support.rs"]
mod drawio_support;
#[path = "../../../fuzz/fuzz_targets/support.rs"]
mod support;

use thinoda_io::{Format, import_bytes_with_report, import_drawio_link};

const PLAIN: &[u8] = include_bytes!("../../../fuzz/corpus/drawio_import/plain");
const PARTIAL_RECTANGLE: &[u8] =
    include_bytes!("../../../fuzz/corpus/drawio_import/partial-rectangle");
const PARTIAL_RECTANGLE_CONTAINER: &[u8] =
    include_bytes!("../../../fuzz/corpus/drawio_import/partial-rectangle-container");
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
        ("partial rectangle", Format::DrawIo, PARTIAL_RECTANGLE),
        (
            "partial rectangle container",
            Format::DrawIo,
            PARTIAL_RECTANGLE_CONTAINER,
        ),
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
        if name == "partial rectangle" {
            let object = &report.workbook.sheets[0].canvas.objects[0];
            assert_eq!(object.rotation, 37.0);
            assert_eq!(
                object.rectangle_sides,
                Some(thinoda_core::RectangleSides {
                    top: true,
                    right: false,
                    bottom: true,
                    left: false,
                })
            );
        }
        if name == "partial rectangle container" {
            assert!(
                report
                    .warnings
                    .contains(&thinoda_io::ImportWarning::Formatting)
            );
            assert_eq!(report.workbook.sheets[0].zones.len(), 1);
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
fn partial_rectangle_style_and_byte_mutations_reach_the_fuzz_harness() {
    let seed = std::str::from_utf8(PARTIAL_RECTANGLE).unwrap();
    for (side, original) in [("top", "1"), ("right", "0"), ("bottom", "1"), ("left", "0")] {
        let original_style = format!("{side}={original};");
        assert_eq!(seed.matches(&original_style).count(), 1, "{original_style}");
        for value in [
            "", "0", "1", "1.0", "1e0", "none", "true", "0x1", "-1", "NaN",
        ] {
            let input = seed.replacen(&original_style, &format!("{side}={value};"), 1);
            let report = import_bytes_with_report(Format::DrawIo, input.as_bytes()).unwrap();
            assert!(
                report.workbook.sheets[0].canvas.objects[0]
                    .rectangle_sides
                    .is_some(),
                "{side}={value}"
            );
            support::roundtrip_report(report);
            drawio_support::exercise(input.as_bytes());
        }
    }

    for index in (0..PARTIAL_RECTANGLE.len()).step_by(3) {
        for byte in [0, b'<', 0xff] {
            let mut input = PARTIAL_RECTANGLE.to_vec();
            input[index] = byte;
            drawio_support::exercise(&input);
        }
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
