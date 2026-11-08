use std::io::{Cursor, Read};

use thinoda_core::{
    Card, CardKind, Color, PresentationSource, Rect, Sheet, SheetRootMode, TextAlign, Vec2,
    Workbook,
};
use thinoda_io::{
    ExportOptions, Format, export_bytes_with_consent, export_presentation_pptx, import_bytes,
};
use zip::ZipArchive;

fn text_element<'a>(svg: &'a str, label: &str) -> &'a str {
    let end = svg.find(&format!(">{label}</text>")).unwrap();
    let start = svg[..end].rfind("<text ").unwrap();
    &svg[start..end]
}

fn slide_xml(bytes: &[u8]) -> String {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    archive
        .by_name("ppt/slides/slide1.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    xml
}

#[test]
fn whole_card_text_formatting_is_rendered_in_visual_and_editable_exports() {
    let mut sheet = Sheet::new("Board", "Anchor");
    sheet.root_mode = SheetRootMode::Hidden;
    let id = sheet
        .add_card(
            "Longer title\nShort",
            Vec2::ZERO,
            Card::new(CardKind::Sticky, Vec2::new(240.0, 140.0)),
        )
        .unwrap();
    let style = &mut sheet.topic_mut(id).unwrap().style;
    style.font_family = Some("monospace".into());
    style.font_size = Some(22.0);
    style.bold = Some(true);
    style.italic = Some(true);
    style.underline = Some(true);
    style.strikethrough = Some(true);
    style.text_color = Some(Color::rgb(0x2e, 0x7c, 0xf6));
    sheet
        .add_presentation_slide(
            "Note",
            PresentationSource::Branch(id),
            Rect::from_min_size(Vec2::new(-100.0, -100.0), Vec2::new(400.0, 300.0)),
        )
        .unwrap();

    for dark in [false, true] {
        let options = ExportOptions {
            dark,
            ..Default::default()
        };
        let mut line_positions = Vec::new();
        for (align, pptx_align) in [
            (TextAlign::Left, "l"),
            (TextAlign::Center, "ctr"),
            (TextAlign::Right, "r"),
        ] {
            sheet.topic_mut(id).unwrap().style.align = Some(align);
            let book = Workbook::new(sheet.clone());
            let native = export_bytes_with_consent(&book, Format::Thinoda, &options, true).unwrap();
            assert_eq!(import_bytes(Format::Thinoda, &native).unwrap(), book);
            let svg = String::from_utf8(
                export_bytes_with_consent(&book, Format::Svg, &options, true).unwrap(),
            )
            .unwrap();
            let line = text_element(&svg, "Short");
            for expected in [
                "font-family=\"monospace\"",
                "font-size=\"22.00\"",
                "font-weight=\"bold\"",
                "font-style=\"italic\"",
                "text-decoration=\"underline line-through\"",
                "fill=\"#2E7CF6\"",
            ] {
                assert!(line.contains(expected), "{align:?}: {line}");
            }
            let x = line
                .split(" x=\"")
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap()
                .parse::<f32>()
                .unwrap();
            line_positions.push(x);

            let xml = slide_xml(&export_presentation_pptx(&sheet, &options).unwrap());
            let end = xml.find("<a:t>Short</a:t>").unwrap();
            let start = xml[..end].rfind("<p:sp>").unwrap();
            let run = &xml[start..end];
            for expected in [
                format!("algn=\"{pptx_align}\""),
                "b=\"1\"".into(),
                "i=\"1\"".into(),
                "u=\"sng\"".into(),
                "strike=\"sngStrike\"".into(),
                "val=\"2E7CF6\"".into(),
                "typeface=\"monospace\"".into(),
            ] {
                assert!(run.contains(&expected), "{align:?}: {run}");
            }
        }
        assert!(
            line_positions[0] < line_positions[1] && line_positions[1] < line_positions[2],
            "short line should move from left to right: {line_positions:?}"
        );
    }
}

#[test]
fn single_line_card_alignment_moves_in_svg_and_editable_slides() {
    for kind in [CardKind::Sticky, CardKind::Text] {
        for rotation in [0.0, 35.0] {
            let mut sheet = Sheet::new("Board", "Anchor");
            sheet.root_mode = SheetRootMode::Hidden;
            let mut card = Card::new(kind, Vec2::new(240.0, 140.0));
            card.rotation = rotation;
            let id = sheet.add_card("Aligned", Vec2::ZERO, card).unwrap();
            sheet
                .add_presentation_slide(
                    "Card",
                    PresentationSource::Branch(id),
                    Rect::from_min_size(Vec2::new(-200.0, -200.0), Vec2::new(400.0, 400.0)),
                )
                .unwrap();
            for dark in [false, true] {
                let options = ExportOptions {
                    dark,
                    ..Default::default()
                };
                let mut svg_positions = Vec::new();
                let mut pptx_positions = Vec::new();
                for align in [TextAlign::Left, TextAlign::Center, TextAlign::Right] {
                    sheet.topic_mut(id).unwrap().style.align = Some(align);
                    let book = Workbook::new(sheet.clone());
                    let svg = String::from_utf8(
                        export_bytes_with_consent(&book, Format::Svg, &options, true).unwrap(),
                    )
                    .unwrap();
                    let text = text_element(&svg, "Aligned");
                    let x = text
                        .split(" x=\"")
                        .nth(1)
                        .unwrap()
                        .split('"')
                        .next()
                        .unwrap()
                        .parse::<f32>()
                        .unwrap();
                    svg_positions.push(x);

                    let xml = slide_xml(&export_presentation_pptx(&sheet, &options).unwrap());
                    let end = xml.find("<a:t>Aligned</a:t>").unwrap();
                    let start = xml[..end].rfind("<p:sp>").unwrap();
                    let x = xml[start..end]
                        .split("<a:off x=\"")
                        .nth(1)
                        .unwrap()
                        .split('"')
                        .next()
                        .unwrap()
                        .parse::<i64>()
                        .unwrap();
                    pptx_positions.push(x);
                }
                assert!(
                    svg_positions[0] + 10.0 < svg_positions[1]
                        && svg_positions[1] + 10.0 < svg_positions[2],
                    "{kind:?} rotation={rotation} dark={dark}: {svg_positions:?}"
                );
                assert!(
                    pptx_positions[0] < pptx_positions[1] && pptx_positions[1] < pptx_positions[2],
                    "{kind:?} rotation={rotation} dark={dark}: {pptx_positions:?}"
                );
            }
        }
    }
}

#[test]
fn each_card_text_decoration_changes_raster_and_pdf_output() {
    for kind in [CardKind::Sticky, CardKind::Text] {
        let mut sheet = Sheet::new("Board", "Anchor");
        sheet.root_mode = SheetRootMode::Hidden;
        let id = sheet
            .add_card(
                "Visible formatting",
                Vec2::ZERO,
                Card::new(kind, Vec2::new(240.0, 140.0)),
            )
            .unwrap();
        let style = &mut sheet.topic_mut(id).unwrap().style;
        style.font_family = Some("monospace".into());
        style.font_size = Some(22.0);
        let original = Workbook::new(sheet.clone());
        let raster = |book: &Workbook, format: Format| {
            let bytes =
                export_bytes_with_consent(book, format, &ExportOptions::default(), true).unwrap();
            image::load_from_memory(&bytes).unwrap().to_rgba8()
        };
        let pdf = |book: &Workbook| {
            export_bytes_with_consent(book, Format::Pdf, &ExportOptions::default(), true).unwrap()
        };
        let plain_png = raster(&original, Format::Png);
        let plain_jpeg = raster(&original, Format::Jpeg);
        let plain_pdf = pdf(&original);
        for decoration in ["italic", "underline", "strikethrough"] {
            let mut decorated = original.clone();
            let style = &mut decorated.active_mut().topic_mut(id).unwrap().style;
            match decoration {
                "italic" => style.italic = Some(true),
                "underline" => style.underline = Some(true),
                "strikethrough" => style.strikethrough = Some(true),
                _ => unreachable!(),
            }
            for (format, plain_image) in [(Format::Png, &plain_png), (Format::Jpeg, &plain_jpeg)] {
                let decorated_image = raster(&decorated, format);
                assert_eq!(plain_image.dimensions(), decorated_image.dimensions());
                assert!(
                    plain_image
                        .pixels()
                        .zip(decorated_image.pixels())
                        .any(|(before, after)| before != after),
                    "{decoration} produced no visible {format:?} change for {kind:?}"
                );
            }
            assert_ne!(plain_pdf, pdf(&decorated), "{decoration} missing from PDF");
        }
    }
}

#[test]
#[ignore = "requires Python 3, pypdfium2 and Pillow"]
fn pdf_rasterizes_each_card_text_decoration_in_both_themes() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };

    fn rasterize(pdf: &[u8]) -> image::RgbImage {
        let script = r#"
import sys
import pypdfium2 as pdfium
document = pdfium.PdfDocument(sys.stdin.buffer.read())
assert len(document) == 1
document[0].render(scale=2).to_pil().convert("RGB").save(sys.stdout.buffer, format="PNG")
"#;
        let mut child = Command::new("python")
            .args(["-c", script])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("install Python, pypdfium2 and Pillow to run this ignored test");
        child.stdin.take().unwrap().write_all(pdf).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "PDF rasterization failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        image::load_from_memory(&output.stdout).unwrap().to_rgb8()
    }

    for kind in [CardKind::Sticky, CardKind::Text] {
        let mut sheet = Sheet::new("Board", "Anchor");
        sheet.root_mode = SheetRootMode::Hidden;
        let id = sheet
            .add_card(
                "Visible formatting",
                Vec2::ZERO,
                Card::new(kind, Vec2::new(240.0, 140.0)),
            )
            .unwrap();
        let style = &mut sheet.topic_mut(id).unwrap().style;
        style.font_family = Some("monospace".into());
        style.font_size = Some(22.0);
        let original = Workbook::new(sheet);
        for dark in [false, true] {
            let options = ExportOptions {
                dark,
                ..Default::default()
            };
            let render = |book: &Workbook| {
                rasterize(&export_bytes_with_consent(book, Format::Pdf, &options, true).unwrap())
            };
            let plain = render(&original);
            for decoration in ["italic", "underline", "strikethrough"] {
                let mut decorated = original.clone();
                let style = &mut decorated.active_mut().topic_mut(id).unwrap().style;
                match decoration {
                    "italic" => style.italic = Some(true),
                    "underline" => style.underline = Some(true),
                    "strikethrough" => style.strikethrough = Some(true),
                    _ => unreachable!(),
                }
                let image = render(&decorated);
                assert_eq!(plain.dimensions(), image.dimensions());
                assert!(
                    plain
                        .pixels()
                        .zip(image.pixels())
                        .filter(|(before, after)| before != after)
                        .count()
                        > 10,
                    "{decoration} produced no visible PDF change for {kind:?}, dark={dark}"
                );
            }
        }
    }
}
