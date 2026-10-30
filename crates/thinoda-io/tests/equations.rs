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

fn xml_attributes(source: &str, tag: &str) -> Vec<std::collections::HashMap<String, String>> {
    let mut reader = quick_xml::Reader::from_str(source);
    let mut result = Vec::new();
    loop {
        match reader.read_event().unwrap() {
            quick_xml::events::Event::Start(node) | quick_xml::events::Event::Empty(node)
                if node.name().as_ref() == tag =>
            {
                result.push(
                    node.attributes()
                        .map(|attribute| {
                            let attribute = attribute.unwrap();
                            (
                                attribute.key.as_ref().to_owned(),
                                attribute
                                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                    .unwrap()
                                    .into_owned(),
                            )
                        })
                        .collect(),
                );
            }
            quick_xml::events::Event::Eof => break,
            _ => {}
        }
    }
    result
}

#[test]
fn equations_unicode_and_long_titles_have_disjoint_bounded_output_geometry() {
    use thinoda_core::{PresentationSource, Rect, Vec2};

    let options = ExportOptions::default();
    let shaping = resvg::usvg::Options {
        fontdb: thinoda_io::fonts::database(),
        ..Default::default()
    };
    for title in [
        "漢字題名全幅研究結果".repeat(18),
        "研究 Ω Ж W a\u{301} العربية 한글 ".repeat(12),
        "W".repeat(240),
    ] {
        let mut book = Workbook::default();
        let root = book.active().root;
        let topic = book.active_mut().add_child(root, None, &title);
        let note = equations::replace_note("", Some("\\frac{a_{i}^{2}}{b^{3}}+x_{j}^{4}")).unwrap();
        book.active_mut()
            .set_markdown_notes(topic, Some(note))
            .unwrap();
        book.active_mut()
            .add_presentation_slide(
                "Bounds",
                PresentationSource::Branch(topic),
                Rect::from_min_size(Vec2::ZERO, Vec2::new(300.0, 200.0)),
            )
            .unwrap();
        let direct =
            String::from_utf8(export_bytes(&book, Format::Svg, &options).unwrap()).unwrap();
        let slide = thinoda_io::render_presentation_slide_svg(book.active(), 0, &options).unwrap();
        let mut slide_size = Vec2::ZERO;
        for svg in [&direct, &slide] {
            let tree = resvg::usvg::Tree::from_str(svg, &shaping).unwrap();
            let title_bounds = tree
                .node_by_id(&format!("equation-{topic}-title"))
                .unwrap()
                .abs_bounding_box();
            let formula_bounds = tree
                .node_by_id(&format!("equation-{topic}-formula"))
                .unwrap()
                .abs_stroke_bounding_box();
            assert!(title_bounds.height() > 28.0, "title must actually wrap");
            assert!(
                title_bounds.bottom() + 4.0 < formula_bounds.top(),
                "shaped title must end above fraction/scripts: {title_bounds:?}, {formula_bounds:?}"
            );
            let root = &xml_attributes(svg, "svg")[0];
            let view: Vec<f32> = root["viewBox"]
                .split_whitespace()
                .map(|value| value.parse().unwrap())
                .collect();
            for bounds in [title_bounds, formula_bounds] {
                assert!(bounds.left() >= 0.0 && bounds.top() >= 0.0);
                assert!(bounds.right() <= tree.size().width(), "{bounds:?}");
                assert!(bounds.bottom() <= tree.size().height(), "{bounds:?}");
            }
            slide_size = Vec2::new(view[2], view[3]);
        }

        let pptx = thinoda_io::export_presentation_pptx(book.active(), &options).unwrap();
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(pptx)).unwrap();
        let mut slide = String::new();
        archive
            .by_name("ppt/slides/slide1.xml")
            .unwrap()
            .read_to_string(&mut slide)
            .unwrap();
        let mut presentation = String::new();
        archive
            .by_name("ppt/presentation.xml")
            .unwrap()
            .read_to_string(&mut presentation)
            .unwrap();
        let size = &xml_attributes(&presentation, "p:sldSz")[0];
        let page_width: f32 = size["cx"].parse().unwrap();
        let page_height: f32 = size["cy"].parse().unwrap();
        let mut title_bottom = 0.0_f32;
        let mut formula_top = f32::INFINITY;
        let mut title_lines = 0;
        let mut glyphs = 0;
        let mut rules = 0;
        for shape in slide.split("<p:sp>").skip(1) {
            let shape = shape.split("</p:sp>").next().unwrap();
            let name = &xml_attributes(shape, "p:cNvPr")[0]["name"];
            let is_title = name.starts_with(&format!("Notes equation topic {topic} line "));
            let is_glyph = name == "Equation glyph";
            if is_title || is_glyph {
                let offset = &xml_attributes(shape, "a:off")[0];
                let extent = &xml_attributes(shape, "a:ext")[0];
                let x: f32 = offset["x"].parse().unwrap();
                let y: f32 = offset["y"].parse().unwrap();
                let width: f32 = extent["cx"].parse().unwrap();
                let height: f32 = extent["cy"].parse().unwrap();
                assert!(x >= 0.0 && y >= 0.0);
                assert!(x + width <= page_width && y + height <= page_height);
                if is_title {
                    title_lines += 1;
                    title_bottom = title_bottom.max(y + height);
                } else {
                    glyphs += 1;
                    formula_top = formula_top.min(y);
                }
            } else if name == "Equation fraction rule" {
                rules += 1;
                for point in xml_attributes(shape, "a:pt") {
                    let x: f32 = point["x"].parse().unwrap();
                    let y: f32 = point["y"].parse().unwrap();
                    assert!((0.0..=page_width).contains(&x));
                    assert!((0.0..=page_height).contains(&y));
                    formula_top = formula_top.min(y);
                }
            }
        }
        assert!(title_lines > 1 && glyphs >= 8 && rules == 1);
        assert!(
            title_bottom < formula_top,
            "native title/glyph boxes overlap"
        );

        let pdf = thinoda_io::export_presentation_pdf(book.active(), &options).unwrap();
        let pdf = String::from_utf8_lossy(&pdf);
        let media = pdf.split("/MediaBox [").nth(1).unwrap();
        let media: Vec<f32> = media
            .split(']')
            .next()
            .unwrap()
            .split_whitespace()
            .map(|value| value.parse().unwrap())
            .collect();
        assert!((media[2] - slide_size.x * 0.75).abs() < 0.02);
        assert!((media[3] - slide_size.y * 0.75).abs() < 0.02);
        assert!(!pdf.contains("/Subtype /Image"));
    }
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
