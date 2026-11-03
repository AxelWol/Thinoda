use std::io::{Cursor, Read};

use thinoda_core::{
    CanvasConnector, CanvasObject, CanvasObjectId, Card, CardKind, Color, Endpoint,
    PresentationScope, PresentationSource, Rect, ShapeKind, Sheet, SheetRootMode, Vec2, Workbook,
};
use thinoda_io::{
    ExportOptions, Format, VisualExportContext, export_bytes, export_bytes_with_visual_context,
    export_presentation_pdf, export_presentation_pdf_with_visual_context,
    export_presentation_png_zip_with_visual_context, export_presentation_pptx_with_visual_context,
    print::{PrintSettings, render_print, render_print_with_visual_context},
    render_presentation_slide_png_with_visual_context, render_presentation_slide_svg,
    render_presentation_slide_svg_with_visual_context,
};
use zip::ZipArchive;

struct Fixture {
    sheet: Sheet,
    automatic: CanvasObjectId,
    inherited: CanvasObjectId,
    explicit: CanvasObjectId,
    invisible: CanvasObjectId,
    outline: CanvasObjectId,
    filled: CanvasObjectId,
    stroked: CanvasObjectId,
}

fn fixture() -> Fixture {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let root = sheet
        .add_card(
            "Context",
            Vec2::new(0.0, -180.0),
            Card::new(CardKind::Sticky, Vec2::new(100.0, 60.0)),
        )
        .unwrap();
    let mut connectors = Vec::new();
    for (index, label) in ["Automatic", "Inherited", "Explicit", "Invisible"]
        .into_iter()
        .enumerate()
    {
        let y = index as f32 * 60.0;
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::new(-150.0, y)),
            Endpoint::Point(Vec2::new(150.0, y)),
        );
        connector.label = label.into();
        connector.arrow_start = true;
        connector.arrow_end = true;
        connector.width = 12.0;
        if index == 2 {
            connector.color = Some(Color::rgba(90, 120, 150, 180));
        }
        let mut object = CanvasObject::connector(connector);
        if index == 1 || index == 2 {
            object.style.stroke = Some(Color::rgba(20, 40, 60, 200));
        }
        object.style.opacity = if index == 3 { 0.0 } else { 0.5 };
        connectors.push(sheet.add_canvas_object(object).unwrap());
    }

    let mut shapes = Vec::new();
    for index in 0..3 {
        let mut shape = CanvasObject::shape(
            ShapeKind::PredefinedProcess,
            Rect::from_min_size(
                Vec2::new(-150.0 + index as f32 * 120.0, 250.0),
                Vec2::new(90.0, 70.0),
            ),
        );
        shape.style.stroke_width = 9.0;
        if index == 1 {
            shape.style.fill = Some(Color::rgb(190, 210, 220));
        }
        if index == 2 {
            shape.style.stroke = Some(Color::rgb(80, 110, 140));
        }
        shapes.push(sheet.add_canvas_object(shape).unwrap());
    }

    let frame = Rect::from_min_size(Vec2::new(-250.0, -250.0), Vec2::new(600.0, 650.0));
    let zone = sheet.create_zone("Frame", vec![root], frame).unwrap();
    sheet
        .add_presentation_slide("Editable", PresentationSource::Zone(zone), frame)
        .unwrap();
    sheet.presentation.slides[0].scope = PresentationScope::Context;
    sheet.presentation.slides[0].auto_frame = false;
    sheet
        .add_presentation_slide("Viewport", PresentationSource::Viewport, frame)
        .unwrap();
    sheet.validate().unwrap();
    Fixture {
        sheet,
        automatic: connectors[0],
        inherited: connectors[1],
        explicit: connectors[2],
        invisible: connectors[3],
        outline: shapes[0],
        filled: shapes[1],
        stroked: shapes[2],
    }
}

fn svg_object(svg: &str, id: CanvasObjectId) -> &str {
    svg.split_once(&format!("id=\"object-{}\"", id.0))
        .unwrap()
        .1
        .split_once("</g>")
        .unwrap()
        .0
}

fn archive_entry(bytes: &[u8], name: &str) -> Vec<u8> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut output = Vec::new();
    archive
        .by_name(name)
        .unwrap()
        .read_to_end(&mut output)
        .unwrap();
    output
}

fn xml_object_shapes(slide: &str, id: CanvasObjectId) -> Vec<&str> {
    let name = format!("name=\"Canvas object {id}\"");
    slide
        .split("<p:sp>")
        .skip(1)
        .filter_map(|part| part.split_once("</p:sp>").map(|(shape, _)| shape))
        .filter(|shape| shape.contains(&name))
        .collect()
}

fn assert_connector(svg: &str, id: CanvasObjectId, label: &str, color: &str) {
    let object = svg_object(svg, id);
    assert!(object.contains(&format!("stroke=\"{color}\"")));
    assert_eq!(object.matches(&format!("fill=\"{color}\"")).count(), 3);
    assert!(object.contains(&format!("fill=\"{color}\">{label}</text>")));
}

#[test]
fn contextual_strokes_preserve_precedence_opacity_shape_details_and_legacy_defaults() {
    let fixture = fixture();
    let before = fixture.sheet.clone();
    let workbook = Workbook::new(fixture.sheet.clone());
    let visual =
        VisualExportContext::default().with_automatic_board_stroke(Color::rgba(238, 20, 180, 200));

    for dark in [false, true] {
        let options = ExportOptions {
            dark,
            ..Default::default()
        };
        let legacy =
            String::from_utf8(export_bytes(&workbook, Format::Svg, &options).unwrap()).unwrap();
        let legacy_context = export_bytes_with_visual_context(
            &workbook,
            Format::Svg,
            &options,
            VisualExportContext::default(),
        )
        .unwrap();
        assert_eq!(legacy.as_bytes(), legacy_context);
        let fallback = if dark { "#C8D0DA80" } else { "#4A556880" };
        assert_connector(&legacy, fixture.automatic, "Automatic", fallback);
        assert!(svg_object(&legacy, fixture.outline).contains("stroke-width=\"9.00\""));
        assert!(svg_object(&legacy, fixture.filled).contains(if dark {
            "stroke=\"#C8D0DA\""
        } else {
            "stroke=\"#4A5568\""
        }));

        let svg = String::from_utf8(
            export_bytes_with_visual_context(&workbook, Format::Svg, &options, visual).unwrap(),
        )
        .unwrap();
        assert_connector(&svg, fixture.automatic, "Automatic", "#EE14B464");
        assert_connector(&svg, fixture.inherited, "Inherited", "#14283C64");
        assert_connector(&svg, fixture.explicit, "Explicit", "#5A78965A");
        assert_connector(&svg, fixture.invisible, "Invisible", "#EE14B400");
        let outline = svg_object(&svg, fixture.outline);
        assert_eq!(outline.matches("stroke=\"#EE14B4C8\"").count(), 3);
        assert_eq!(outline.matches("stroke-width=\"2.00\"").count(), 3);
        assert_eq!(
            svg_object(&svg, fixture.filled)
                .matches("stroke=\"none\"")
                .count(),
            3
        );
        let stroked = svg_object(&svg, fixture.stroked);
        assert_eq!(stroked.matches("stroke=\"#506E8C\"").count(), 3);
        assert_eq!(stroked.matches("stroke-width=\"9.00\"").count(), 3);

        let slide_svg =
            render_presentation_slide_svg_with_visual_context(&fixture.sheet, 0, &options, visual)
                .unwrap();
        assert_connector(&slide_svg, fixture.automatic, "Automatic", "#EE14B464");
        assert_eq!(
            render_presentation_slide_svg(&fixture.sheet, 0, &options).unwrap(),
            render_presentation_slide_svg_with_visual_context(
                &fixture.sheet,
                0,
                &options,
                VisualExportContext::default(),
            )
            .unwrap()
        );
        let pptx =
            export_presentation_pptx_with_visual_context(&fixture.sheet, &options, visual).unwrap();
        let slide = String::from_utf8(archive_entry(&pptx, "ppt/slides/slide1.xml")).unwrap();
        for (id, rgb, alpha) in [
            (fixture.automatic, "EE14B4", 100_u32),
            (fixture.inherited, "14283C", 100),
            (fixture.explicit, "5A7896", 90),
            (fixture.invisible, "EE14B4", 0),
        ] {
            let shapes = xml_object_shapes(&slide, id);
            assert_eq!(shapes.len(), 3);
            let fill = format!(
                "<a:srgbClr val=\"{rgb}\"><a:alpha val=\"{}\"",
                alpha * 100_000 / 255
            );
            assert!(shapes.iter().all(|shape| shape.contains(&fill)));
            let label = match id {
                id if id == fixture.automatic => "Automatic",
                id if id == fixture.inherited => "Inherited",
                id if id == fixture.explicit => "Explicit",
                _ => "Invisible",
            };
            let label_shape = slide
                .split("<p:sp>")
                .skip(1)
                .filter_map(|part| part.split_once("</p:sp>").map(|(shape, _)| shape))
                .find(|shape| shape.contains(&format!("<a:t>{label}</a:t>")))
                .unwrap();
            assert!(label_shape.contains(&fill));
        }
        let filled = xml_object_shapes(&slide, fixture.filled);
        assert_eq!(filled.len(), 3);
        assert!(
            filled
                .iter()
                .all(|shape| shape.contains("<a:ln><a:noFill/></a:ln>"))
        );
        let outline = xml_object_shapes(&slide, fixture.outline);
        assert_eq!(outline.len(), 3);
        assert!(outline.iter().all(|shape| shape.contains("val=\"EE14B4\"")));
    }
    assert_eq!(fixture.sheet, before);
    assert_eq!(workbook.active(), &before);
}

fn magenta_pixels(image: &image::RgbImage) -> usize {
    image
        .pixels()
        .filter(|pixel| pixel[0] > 170 && pixel[1] < 100 && pixel[2] > 100)
        .count()
}

fn assert_magenta(bytes: &[u8]) {
    let image = image::load_from_memory(bytes).unwrap().to_rgb8();
    assert!(magenta_pixels(&image) > 20);
}

#[test]
fn visual_context_reaches_raster_pdf_presentations_and_print_without_mutating_sources() {
    let fixture = fixture();
    let before = fixture.sheet.clone();
    let workbook = Workbook::new(fixture.sheet.clone());
    let options = ExportOptions::default();
    let visual =
        VisualExportContext::default().with_automatic_board_stroke(Color::rgb(255, 0, 180));

    let legacy_native = export_bytes(&workbook, Format::Thinoda, &options).unwrap();
    assert_eq!(
        legacy_native,
        export_bytes_with_visual_context(&workbook, Format::Thinoda, &options, visual).unwrap()
    );
    for format in [Format::Png, Format::Jpeg] {
        let legacy = export_bytes(&workbook, format, &options).unwrap();
        let contextual =
            export_bytes_with_visual_context(&workbook, format, &options, visual).unwrap();
        assert_magenta(&contextual);
        assert!(
            magenta_pixels(&image::load_from_memory(&legacy).unwrap().to_rgb8())
                < magenta_pixels(&image::load_from_memory(&contextual).unwrap().to_rgb8())
        );
    }
    let legacy_pdf = export_bytes(&workbook, Format::Pdf, &options).unwrap();
    let contextual_pdf =
        export_bytes_with_visual_context(&workbook, Format::Pdf, &options, visual).unwrap();
    assert!(contextual_pdf.starts_with(b"%PDF-"));
    assert_ne!(contextual_pdf, legacy_pdf);
    assert!(!String::from_utf8_lossy(&contextual_pdf).contains("/Subtype /Image"));

    for index in [0, 1] {
        let png = render_presentation_slide_png_with_visual_context(
            &fixture.sheet,
            index,
            &options,
            visual,
        )
        .unwrap();
        assert_magenta(&png);
        let archive =
            export_presentation_png_zip_with_visual_context(&fixture.sheet, &options, visual)
                .unwrap();
        assert_eq!(
            archive_entry(&archive, &format!("slides/slide-{:03}.png", index + 1)),
            png
        );
        let pptx_preview =
            thinoda_io::presentation::render_presentation_pptx_slide_png_with_visual_context(
                &fixture.sheet,
                index,
                &options,
                visual,
            )
            .unwrap();
        assert_magenta(&pptx_preview);
    }
    let pptx =
        export_bytes_with_visual_context(&workbook, Format::PowerPoint, &options, visual).unwrap();
    assert_magenta(&archive_entry(&pptx, "ppt/media/slide2-viewport.png"));
    let presentation_pdf =
        export_presentation_pdf_with_visual_context(&fixture.sheet, &options, visual).unwrap();
    assert!(presentation_pdf.starts_with(b"%PDF-"));
    assert_ne!(
        presentation_pdf,
        export_presentation_pdf(&fixture.sheet, &options).unwrap()
    );

    let settings = PrintSettings::default();
    let print = render_print_with_visual_context(&workbook, 0, settings, visual).unwrap();
    let legacy_print = render_print(&workbook, 0, settings).unwrap();
    assert!(magenta_pixels(&print.image) > magenta_pixels(&legacy_print.image) + 20);
    assert!(print.pdf().unwrap().starts_with(b"%PDF-"));
    assert_eq!(fixture.sheet, before);
    assert_eq!(workbook.active(), &before);
}

#[test]
#[ignore = "requires Python 3, pypdfium2 and Pillow"]
fn pdf_renderers_show_the_contextual_stroke_in_both_themes() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };

    fn rasterize(pdf: &[u8], page: usize) -> image::RgbImage {
        let script = r#"
import sys
import pypdfium2 as pdfium
document = pdfium.PdfDocument(sys.stdin.buffer.read())
document[int(sys.argv[1])].render(scale=2).to_pil().convert("RGB").save(sys.stdout.buffer, format="PNG")
"#;
        let mut child = Command::new("python")
            .args(["-c", script, &page.to_string()])
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

    let fixture = fixture();
    let workbook = Workbook::new(fixture.sheet.clone());
    let visual =
        VisualExportContext::default().with_automatic_board_stroke(Color::rgb(255, 0, 180));
    for dark in [false, true] {
        let options = ExportOptions {
            dark,
            ..Default::default()
        };
        let map =
            export_bytes_with_visual_context(&workbook, Format::Pdf, &options, visual).unwrap();
        assert!(magenta_pixels(&rasterize(&map, 0)) > 20);
        let deck =
            export_presentation_pdf_with_visual_context(&fixture.sheet, &options, visual).unwrap();
        for page in [0, 1] {
            assert!(magenta_pixels(&rasterize(&deck, page)) > 20);
        }
        let print = render_print_with_visual_context(
            &workbook,
            0,
            PrintSettings {
                dark,
                ..Default::default()
            },
            visual,
        )
        .unwrap()
        .pdf()
        .unwrap();
        assert!(magenta_pixels(&rasterize(&print, 0)) > 20);
    }
}
