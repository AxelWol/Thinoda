use thinoda_core::{
    Workbook,
    numbering::{NumberFormat, Numbering},
};
use thinoda_io::{ExportOptions, Format, export_bytes, import_bytes};

fn numbered() -> (Workbook, thinoda_core::TopicId) {
    let mut book = Workbook {
        sheets: vec![thinoda_core::Sheet::new("Plan", "Release")],
        ..Default::default()
    };
    let sheet = book.active_mut();
    let first = sheet.add_child(sheet.root, None, "Plan");
    let second = sheet.add_child(sheet.root, None, "Build");
    let nested = sheet.add_child(second, None, "Test");
    sheet
        .set_sibling_numbering(first, Some(Numbering::default()))
        .unwrap();
    sheet
        .set_sibling_numbering(
            nested,
            Some(Numbering {
                format: NumberFormat::LowerAlpha,
                tiered: true,
                start: 1,
            }),
        )
        .unwrap();
    (book, nested)
}

#[test]
fn native_and_xmind_roundtrips_keep_numbering_out_of_titles() {
    let (book, nested) = numbered();
    for format in [Format::Thinoda, Format::XMind] {
        let bytes = export_bytes(&book, format, &ExportOptions::default()).unwrap();
        let restored = import_bytes(format, &bytes).unwrap();
        assert_eq!(
            restored.active().numbering_labels()[&nested],
            "2.a",
            "{format:?}"
        );
        assert_eq!(restored.active().topics[&nested].title, "Test");
    }
}

#[test]
fn graphical_exports_draw_numbers_and_text_outlines_keep_titles() {
    let (book, _) = numbered();
    let before = book.clone();
    for dark in [false, true] {
        let options = ExportOptions {
            dark,
            ..Default::default()
        };
        let svg = String::from_utf8(export_bytes(&book, Format::Svg, &options).unwrap()).unwrap();
        assert!(svg.contains("class=\"topic-number\""));
        assert!(svg.contains(">2.a</text>"));
        for format in [Format::Png, Format::Pdf] {
            assert!(!export_bytes(&book, format, &options).unwrap().is_empty());
        }
    }
    let markdown = String::from_utf8(
        export_bytes(&book, Format::Markdown, &ExportOptions::default()).unwrap(),
    )
    .unwrap();
    assert!(markdown.contains("Test"));
    assert!(!markdown.contains("2.a"));
    assert_eq!(book, before);
}
