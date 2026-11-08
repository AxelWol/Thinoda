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
const CLIPBOARD_MULTIPAGE: &[u8] =
    include_bytes!("../../../fuzz/corpus/drawio_import/clipboard-multipage");
const CLIPBOARD_ENCODED: &[u8] =
    include_bytes!("../../../fuzz/corpus/drawio_import/clipboard-encoded");
const DETACHED_CONNECTOR: &[u8] =
    include_bytes!("../../../fuzz/corpus/drawio_import/connector-detached");
const MALFORMED_POINT: &[u8] = include_bytes!("../../../fuzz/corpus/drawio_import/malformed-point");
const MALFORMED_GEOMETRY: &[u8] =
    include_bytes!("../../../fuzz/corpus/drawio_import/malformed-geometry");
const OUT_OF_RANGE_POINTS: &[u8] =
    include_bytes!("../../../fuzz/corpus/drawio_import/out-of-range-points");

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
        ("detached connector", Format::DrawIo, DETACHED_CONNECTOR),
        ("malformed point", Format::DrawIo, MALFORMED_POINT),
        ("malformed geometry", Format::DrawIo, MALFORMED_GEOMETRY),
        ("out-of-range points", Format::DrawIo, OUT_OF_RANGE_POINTS),
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
        if name == "detached connector" {
            assert!(
                report
                    .warnings
                    .contains(&thinoda_io::ImportWarning::DetachedConnectors)
            );
            let pasted =
                thinoda_io::drawio_clipboard::decode(std::str::from_utf8(bytes).unwrap()).unwrap();
            assert!(
                pasted
                    .warnings
                    .contains(&thinoda_io::ImportWarning::DetachedConnectors)
            );
        }
        if name == "malformed point" {
            assert_eq!(report.warnings, vec![thinoda_io::ImportWarning::Other]);
            assert!(
                report.workbook.sheets[0]
                    .canvas
                    .objects
                    .iter()
                    .all(|object| {
                        !matches!(
                            object.kind,
                            thinoda_core::board::CanvasObjectKind::Connector(_)
                        )
                    })
            );
            let pasted =
                thinoda_io::drawio_clipboard::decode(std::str::from_utf8(bytes).unwrap()).unwrap();
            assert_eq!(pasted.warnings, vec![thinoda_io::ImportWarning::Other]);
        }
        if name == "malformed geometry" {
            assert_eq!(report.warnings, vec![thinoda_io::ImportWarning::Other]);
            let objects = &report.workbook.sheets[0].canvas.objects;
            assert_eq!(objects.len(), 1);
            assert_eq!(objects[0].label(), "Safe");
            let pasted =
                thinoda_io::drawio_clipboard::decode(std::str::from_utf8(bytes).unwrap()).unwrap();
            assert_eq!(pasted.warnings, vec![thinoda_io::ImportWarning::Other]);
        }
        if name == "out-of-range points" {
            assert_eq!(report.warnings, vec![thinoda_io::ImportWarning::Other]);
            let objects = &report.workbook.sheets[0].canvas.objects;
            assert_eq!(objects.len(), 2);
            assert!(objects.iter().any(|object| matches!(
                &object.kind,
                thinoda_core::CanvasObjectKind::Connector(connector)
                    if connector.waypoints.len() == 1
            )));
            assert!(objects.iter().any(|object| matches!(
                &object.kind,
                thinoda_core::CanvasObjectKind::Ink(ink) if ink.points.len() == 2
            )));
            let pasted =
                thinoda_io::drawio_clipboard::decode(std::str::from_utf8(bytes).unwrap()).unwrap();
            assert_eq!(pasted.warnings, vec![thinoda_io::ImportWarning::Other]);
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
fn clipboard_seeds_reach_the_decoder_and_validate() {
    use thinoda_io::drawio_clipboard::{self, DrawioPasteLoss};

    for (bytes, expected) in [
        (
            CLIPBOARD_MULTIPAGE,
            vec![
                DrawioPasteLoss::ContainerFrames(1),
                DrawioPasteLoss::AdditionalPages(1),
            ],
        ),
        (CLIPBOARD_ENCODED, vec![]),
    ] {
        let text = std::str::from_utf8(bytes).unwrap();
        let pasted = drawio_clipboard::decode(text).unwrap();
        pasted.payload.validate().unwrap();
        assert_eq!(pasted.losses, expected);
        if bytes == CLIPBOARD_MULTIPAGE {
            let mut sheet = thinoda_core::Sheet::new("Paste", "");
            sheet.root_mode = thinoda_core::SheetRootMode::Hidden;
            thinoda_core::clipboard::paste_board_selection(
                &mut sheet,
                &pasted.payload,
                thinoda_core::Vec2::ZERO,
            )
            .unwrap();
            assert_eq!(sheet.canvas.objects.len(), 1);
            assert_eq!(sheet.canvas.objects[0].label(), "Child");
            assert_eq!(
                sheet.canvas.objects[0].rect.min,
                thinoda_core::Vec2::new(120.0, 230.0)
            );
        }
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
