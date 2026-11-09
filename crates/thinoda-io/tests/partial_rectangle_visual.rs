use std::io::{Cursor, Read, Write};

use thinoda_core::{
    CanvasObject, Color, PresentationSource, Rect, RectangleSides, ShapeKind, Sheet, SheetRootMode,
    Vec2, Workbook,
};
use thinoda_io::{
    ExportOptions, Format, export_bytes_with_consent, export_presentation_pdf,
    export_presentation_pptx,
};
use zip::ZipArchive;

fn entry(bytes: &[u8], name: &str) -> Vec<u8> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut output = Vec::new();
    archive
        .by_name(name)
        .unwrap()
        .read_to_end(&mut output)
        .unwrap();
    output
}

#[test]
fn partial_rectangle_presentation_preserves_compound_paths_and_viewport_pixels() {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut shape = CanvasObject::shape(
        ShapeKind::Rectangle,
        Rect::from_min_size(Vec2::new(20.0, 20.0), Vec2::new(80.0, 40.0)),
    );
    shape.rectangle_sides = Some(RectangleSides {
        top: true,
        right: false,
        bottom: true,
        left: false,
    });
    shape.style.fill = Some(Color::TRANSPARENT);
    shape.style.stroke = Some(Color::rgba(220, 0, 20, 128));
    shape.style.stroke_width = 8.0;
    let partial = sheet.add_canvas_object(shape).unwrap();
    let mut collapsed = CanvasObject::shape(
        ShapeKind::Rectangle,
        Rect::from_min_size(Vec2::new(150.0, 20.0), Vec2::new(0.0, 40.0)),
    );
    collapsed.rectangle_sides = Some(RectangleSides::default());
    collapsed.style.fill = Some(Color::TRANSPARENT);
    collapsed.style.stroke = Some(Color::rgba(220, 0, 20, 128));
    collapsed.style.stroke_width = 8.0;
    let collapsed = sheet.add_canvas_object(collapsed).unwrap();
    let root = sheet.add_floating("Slide", Vec2::new(300.0, 300.0));
    let frame = Rect::from_min_size(Vec2::new(-20.0, -20.0), Vec2::new(220.0, 120.0));
    sheet
        .add_presentation_slide("Editable", PresentationSource::Branch(root), frame)
        .unwrap();
    sheet
        .add_presentation_slide("Viewport", PresentationSource::Viewport, frame)
        .unwrap();
    let original = sheet.clone();

    let pptx = export_presentation_pptx(&sheet, &ExportOptions::default()).unwrap();
    let slide = String::from_utf8(entry(&pptx, "ppt/slides/slide1.xml")).unwrap();
    let outline = |id| {
        slide
            .split("<p:sp>")
            .find(|fragment| {
                fragment.contains(&format!("Canvas object {id}")) && fragment.contains("cap=\"sq\"")
            })
            .unwrap_or_else(|| panic!("missing editable outline for {id}: {slide}"))
    };
    let partial_path = outline(partial);
    assert_eq!(partial_path.matches("<a:moveTo>").count(), 2);
    assert_eq!(partial_path.matches("<a:lnTo>").count(), 2);
    assert!(!partial_path.contains("<a:close/>"));
    assert!(partial_path.contains("<a:miter/>"));
    assert!(partial_path.contains("val=\"DC0014\""));
    assert!(partial_path.contains("<a:alpha val=\"50196\"/>"));
    let collapsed_path = outline(collapsed);
    assert_eq!(collapsed_path.matches("<a:moveTo>").count(), 1);
    assert_eq!(collapsed_path.matches("<a:lnTo>").count(), 4);
    assert!(!collapsed_path.contains("<a:close/>"));

    let viewport = String::from_utf8(entry(&pptx, "ppt/slides/slide2.xml")).unwrap();
    assert!(viewport.contains("Flattened viewport scene"));
    let raster = image::load_from_memory(&entry(&pptx, "ppt/media/slide2-viewport.png"))
        .unwrap()
        .to_rgba8();
    assert!(
        raster
            .pixels()
            .filter(|pixel| pixel[0] > pixel[1].saturating_add(30))
            .count()
            > 30,
        "partial rectangle outline should appear in the flattened viewport"
    );
    let mut hidden = sheet.clone();
    for object in &mut hidden.canvas.objects {
        object.rectangle_sides = Some(RectangleSides {
            top: false,
            right: false,
            bottom: false,
            left: false,
        });
    }
    let without_sides = export_presentation_pptx(&hidden, &ExportOptions::default()).unwrap();
    assert_ne!(
        entry(&pptx, "ppt/media/slide2-viewport.png"),
        entry(&without_sides, "ppt/media/slide2-viewport.png")
    );
    assert_ne!(
        export_presentation_pdf(&sheet, &ExportOptions::default()).unwrap(),
        export_presentation_pdf(&hidden, &ExportOptions::default()).unwrap()
    );
    assert_eq!(sheet, original);
}

#[test]
#[ignore = "requires Python 3, pypdfium2 and Pillow"]
fn partial_rectangle_pdf_rasterizes_visible_sides_without_double_alpha() {
    use std::process::{Command, Stdio};

    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut shape = CanvasObject::shape(
        ShapeKind::Rectangle,
        Rect::from_min_size(Vec2::new(20.0, 20.0), Vec2::new(80.0, 40.0)),
    );
    shape.rectangle_sides = Some(RectangleSides {
        top: true,
        right: false,
        bottom: false,
        left: true,
    });
    shape.style.fill = Some(Color::TRANSPARENT);
    shape.style.stroke = Some(Color::rgba(220, 0, 20, 128));
    shape.style.stroke_width = 8.0;
    sheet.add_canvas_object(shape).unwrap();
    let book = Workbook::new(sheet);
    let options = ExportOptions {
        scale: 4.0,
        ..Default::default()
    };
    let export = |format| export_bytes_with_consent(&book, format, &options, true).unwrap();
    let svg = String::from_utf8(export(Format::Svg)).unwrap();
    let bounds: Vec<f32> = svg
        .split("viewBox=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap()
        .split_whitespace()
        .map(|value| value.parse().unwrap())
        .collect();
    let png = image::load_from_memory(&export(Format::Png))
        .unwrap()
        .to_rgb8();
    let script = r#"
import sys
import pypdfium2 as pdfium
document = pdfium.PdfDocument(sys.stdin.buffer.read())
assert len(document) == 1
document[0].render(scale=4).to_pil().convert("RGB").save(sys.stdout.buffer, format="PNG")
"#;
    let mut child = Command::new("python")
        .args(["-c", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("install Python, pypdfium2 and Pillow to run this ignored test");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&export(Format::Pdf))
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "PDF rasterization failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let pdf = image::load_from_memory(&output.stdout).unwrap().to_rgb8();
    assert_eq!(pdf.dimensions(), png.dimensions());
    let sample = |image: &image::RgbImage, point: Vec2| {
        let x = ((point.x - bounds[0]) / bounds[2] * image.width() as f32).floor() as u32;
        let y = ((point.y - bounds[1]) / bounds[3] * image.height() as f32).floor() as u32;
        image.get_pixel(x, y).0
    };
    let stroke = sample(&png, Vec2::new(60.0, 20.0));
    let background = sample(&png, Vec2::new(100.0, 40.0));
    assert!(stroke[1] + 30 < background[1]);
    for (point, expected) in [
        (Vec2::new(60.0, 20.0), stroke),
        (Vec2::new(20.0, 40.0), stroke),
        (Vec2::new(20.0, 20.0), stroke),
        (Vec2::new(103.0, 20.0), stroke),
        (Vec2::new(20.0, 63.0), stroke),
        (Vec2::new(100.0, 40.0), background),
        (Vec2::new(60.0, 60.0), background),
    ] {
        for image in [&png, &pdf] {
            let actual = sample(image, point);
            assert!(
                actual
                    .into_iter()
                    .zip(expected)
                    .all(|(left, right)| left.abs_diff(right) <= 4),
                "{point:?}: {actual:?} != {expected:?}"
            );
        }
    }
}
