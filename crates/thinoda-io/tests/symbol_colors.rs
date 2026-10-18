use thinoda_core::{Color, Sheet, Workbook, markers};
use thinoda_io::{ExportOptions, Format, export_bytes, import_bytes, symbol_color_loss};

fn sample() -> Workbook {
    let mut s = Sheet::new("Colors", "Root");
    let root = s.root;
    for group in markers::GROUPS {
        let id = s.insert_child(root, group.name).unwrap();
        s.toggle_marker_color(
            &[id],
            group.markers[0].id,
            Some(Color::rgba(12, 34, 56, 180)),
        )
        .unwrap();
    }
    Workbook::new(s)
}

#[test]
fn native_and_unchanged_xmind_roundtrip_every_group_with_alpha() {
    let book = sample();
    for format in [Format::Thinoda, Format::XMind] {
        let bytes = export_bytes(&book, format, &Default::default()).unwrap();
        assert_eq!(import_bytes(format, &bytes).unwrap(), book);
    }
}

#[test]
fn editable_color_loss_requires_explicit_acknowledgement() {
    let book = sample();
    for format in [
        Format::FreeMind,
        Format::Opml,
        Format::Markdown,
        Format::PlainText,
        Format::TextBundle,
        Format::Word,
        Format::Excel,
        Format::Csv,
    ] {
        assert!(symbol_color_loss(&book, format), "{format:?}");
        let error = export_bytes(&book, format, &Default::default()).unwrap_err();
        assert!(error.to_string().contains("cannot preserve symbol colors"));
        let opts = ExportOptions {
            allow_symbol_color_loss: true,
            ..Default::default()
        };
        assert!(!export_bytes(&book, format, &opts).unwrap().is_empty());
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("existing.md");
    std::fs::write(&path, b"original file").unwrap();
    assert!(thinoda_io::export_file(&book, &path, Format::Markdown, &Default::default()).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"original file");
}

#[test]
fn visual_exports_keep_color_alpha_and_vector_progress() {
    let book = sample();
    for dark in [false, true] {
        let opts = ExportOptions {
            dark,
            ..Default::default()
        };
        let svg = String::from_utf8(export_bytes(&book, Format::Svg, &opts).unwrap()).unwrap();
        assert_eq!(svg.matches("#0C2238B4").count(), 9);
        assert!(svg.contains("<circle"));
        assert!(svg.contains("<polygon"));
        assert!(svg.contains(">Jan</text>"));
        assert!(svg.contains(">Mo</text>"));
        for format in [Format::Png, Format::Jpeg, Format::Pdf] {
            assert!(!symbol_color_loss(&book, format));
            assert!(!export_bytes(&book, format, &opts).unwrap().is_empty());
        }
    }
}

#[test]
fn raster_exports_and_print_pages_contain_the_custom_ink() {
    let mut sheet = Sheet::new("Ink", "Root");
    let ink = Color::rgb(13, 197, 83);
    sheet
        .toggle_marker_color(&[sheet.root], "star-red", Some(ink))
        .unwrap();
    let book = Workbook::new(sheet);
    for dark in [false, true] {
        let png = export_bytes(
            &book,
            Format::Png,
            &ExportOptions {
                dark,
                ..Default::default()
            },
        )
        .unwrap();
        let image = image::load_from_memory(&png).unwrap().into_rgb8();
        assert!(image.pixels().any(|p| p.0 == [ink.r, ink.g, ink.b]));
        let print = thinoda_io::print::render_print(
            &book,
            0,
            thinoda_io::print::PrintSettings {
                dark,
                background: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(print.image.pixels().any(|p| p.0 == [ink.r, ink.g, ink.b]));
        assert!(print.pdf().unwrap().starts_with(b"%PDF"));
    }
}
