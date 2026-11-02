use std::io::{Cursor, Read};

use image::{GenericImageView, RgbaImage};
use thinoda_core::{
    CanvasConnector, CanvasObject, Card, CardKind, Color, Endpoint, InkStroke, InkTool,
    PresentationScope, PresentationSource, Rect, ShapeKind, Sheet, SheetRootMode, TopicId, Vec2,
    Workbook, ZoneId,
    board::{Band, CanvasObjectKind},
    resources::Resource,
};
use thinoda_io::{
    ExportOptions, Format, IoError, PresentationAspectRatio, export_bytes, export_presentation_pdf,
    export_presentation_png_zip, export_presentation_pptx, generate_presentation, import_bytes,
    presentation::render_presentation_pptx_slide_png, render_presentation_slide_png,
    render_presentation_slide_svg,
};
use zip::ZipArchive;

const BLUE: [u8; 3] = [30, 80, 230];
const RED: [u8; 3] = [225, 20, 30];
const GREEN: [u8; 3] = [20, 220, 40];

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect::from_min_size(Vec2::new(x, y), Vec2::new(width, height))
}

fn shape(frame: Rect, color: [u8; 3]) -> CanvasObject {
    let mut object = CanvasObject::shape(ShapeKind::Rectangle, frame);
    object.style.fill = Some(Color::rgb(color[0], color[1], color[2]));
    object
}

fn board() -> Sheet {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;

    let mut card = Card::new(CardKind::Sticky, Vec2::new(80.0, 60.0));
    card.color = Some(Color::rgb(245, 220, 80));
    card.rotation = 25.0;
    sheet
        .add_card("Sticky", Vec2::new(90.0, 90.0), card)
        .unwrap();
    let floating = sheet.add_floating("Floating root", Vec2::new(220.0, 150.0));
    let zone = sheet
        .create_zone(
            "Scene zone",
            vec![floating],
            rect(155.0, 105.0, 130.0, 100.0),
        )
        .unwrap();
    sheet.zones.iter_mut().find(|z| z.id == zone).unwrap().color = Some(Color::rgb(150, 30, 160));

    let mut below = shape(rect(10.0, 10.0, 35.0, 30.0), BLUE);
    below.band = Band::Below;
    sheet.add_canvas_object(below).unwrap();
    // This object crosses the right edge, while the green objects are completely
    // outside but would be exposed by fitting the viewport to either deck ratio.
    sheet
        .add_canvas_object(shape(rect(290.0, 20.0, 40.0, 40.0), RED))
        .unwrap();
    sheet
        .add_canvas_object(shape(rect(305.0, 140.0, 40.0, 35.0), GREEN))
        .unwrap();
    sheet
        .add_canvas_object(shape(rect(115.0, 205.0, 40.0, 35.0), GREEN))
        .unwrap();
    sheet
        .add_canvas_object(CanvasObject::ink(InkStroke::new(
            InkTool::Pen,
            Color::rgb(5, 125, 155),
            4.0,
            vec![Vec2::new(145.0, 25.0), Vec2::new(200.0, 50.0)],
        )))
        .unwrap();
    let mut connector = CanvasConnector::new(
        Endpoint::Point(Vec2::new(240.0, 70.0)),
        Endpoint::Point(Vec2::new(360.0, 70.0)),
    );
    connector.label = "Scene link".into();
    sheet
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    sheet
        .add_presentation_slide(
            "Scene",
            PresentationSource::Viewport,
            rect(0.0, 0.0, 300.0, 200.0),
        )
        .unwrap();
    assert!(sheet.presentation.slides[0].auto_frame);
    sheet.validate().unwrap();
    sheet
}

fn entry(bytes: &[u8], name: &str) -> Vec<u8> {
    let mut zip = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut data = Vec::new();
    zip.by_name(name).unwrap().read_to_end(&mut data).unwrap();
    data
}

fn has_color(image: &RgbaImage, color: [u8; 3]) -> bool {
    image
        .pixels()
        .any(|pixel| pixel.0[..3] == color && pixel.0[3] == 255)
}

fn picture_geometry(slide: &str) -> (u64, u64, u64, u64) {
    let picture = slide
        .split_once("<p:pic>")
        .unwrap()
        .1
        .split_once("</p:pic>")
        .unwrap()
        .0;
    let attr = |tag: &str, name: &str| {
        picture
            .split_once(&format!("<a:{tag} "))
            .unwrap()
            .1
            .split_once("/>")
            .unwrap()
            .0
            .split_once(&format!("{name}=\""))
            .unwrap()
            .1
            .split('"')
            .next()
            .unwrap()
            .parse::<u64>()
            .unwrap()
    };
    (
        attr("off", "x"),
        attr("off", "y"),
        attr("ext", "cx"),
        attr("ext", "cy"),
    )
}

#[test]
fn viewport_uses_saved_frame_and_the_complete_board_scene() {
    let sheet = board();
    let before = sheet.clone();
    let options = ExportOptions::default();
    let svg = render_presentation_slide_svg(&sheet, 0, &options).unwrap();
    assert!(svg.contains("viewBox=\"0.00 0.00 300.00 200.00\""));
    assert!(svg.contains("Scene zone"));
    assert!(svg.contains("Floating root"));
    assert!(svg.contains("Sticky"));
    assert!(svg.contains("rotate(25.00"));
    assert!(svg.contains("fill=\"#1E50E6\""));
    assert!(!svg.contains(&format!("topic-{}-", sheet.root)));
    let below = sheet
        .canvas
        .objects
        .iter()
        .find(|object| object.band == Band::Below)
        .unwrap();
    let above = sheet
        .canvas
        .objects
        .iter()
        .find(|object| object.band == Band::Above)
        .unwrap();
    let card = svg.find("class=\"card\"").unwrap();
    assert!(svg.find(&format!("id=\"object-{}\"", below.id)).unwrap() < card);
    assert!(svg.find(&format!("id=\"object-{}\"", above.id)).unwrap() > card);
    for kind in [
        |kind: &CanvasObjectKind| matches!(kind, CanvasObjectKind::Ink(_)),
        |kind: &CanvasObjectKind| matches!(kind, CanvasObjectKind::Connector(_)),
    ] {
        let object = sheet
            .canvas
            .objects
            .iter()
            .find(|object| kind(&object.kind))
            .unwrap();
        assert!(svg.contains(&format!("id=\"object-{}\"", object.id)));
    }

    let png = render_presentation_slide_png(&sheet, 0, &options).unwrap();
    let image = image::load_from_memory(&png).unwrap().to_rgba8();
    assert_eq!(image.dimensions(), (300, 200));
    assert!(has_color(&image, BLUE), "below-map canvas object missing");
    assert!(has_color(&image, RED), "intersecting canvas object missing");
    assert!(
        !has_color(&image, GREEN),
        "objects outside the saved viewport were painted"
    );
    assert_eq!(image.get_pixel(295, 30).0[..3], RED);
    assert_eq!(image.get_pixel(47, 97).0[..3], [245, 220, 80]);
    assert_eq!(sheet, before);

    let archive = export_presentation_png_zip(&sheet, &options).unwrap();
    assert_eq!(entry(&archive, "slides/slide-001.png"), png);
}

#[test]
fn viewport_pdf_and_powerpoint_clip_scene_instead_of_revealing_ratio_padding() {
    let sheet = board();
    let before = sheet.clone();
    let options = ExportOptions::default();
    let pdf = export_presentation_pdf(&sheet, &options).unwrap();
    let pdf_text = String::from_utf8_lossy(&pdf);
    assert!(pdf_text.starts_with("%PDF-"));
    assert!(pdf_text.contains("/MediaBox [0 0 225 150]"));
    assert!(pdf_text.contains("/Subtype /Form"));
    assert!(!pdf_text.contains("/Subtype /Image"));
    let mut no_objects = sheet.clone();
    no_objects.canvas.objects.clear();
    assert_ne!(
        pdf,
        export_presentation_pdf(&no_objects, &options).unwrap(),
        "canvas geometry must contribute to the PDF form"
    );

    for ratio in [
        PresentationAspectRatio::Widescreen16x9,
        PresentationAspectRatio::Standard4x3,
    ] {
        let options = ExportOptions {
            presentation_aspect_ratio: ratio,
            ..ExportOptions::default()
        };
        let preview = render_presentation_pptx_slide_png(&sheet, 0, &options).unwrap();
        let preview = image::load_from_memory(&preview).unwrap().to_rgba8();
        assert!(has_color(&preview, BLUE));
        assert!(has_color(&preview, RED));
        assert!(
            !has_color(&preview, GREEN),
            "PowerPoint preview leaked off-frame content for {ratio:?}"
        );
        if ratio == PresentationAspectRatio::Widescreen16x9 {
            assert_eq!(preview.dimensions(), (640, 360));
            let margin = preview.get_pixel(610, 60);
            assert_eq!(
                margin,
                preview.get_pixel(10, 60),
                "the part of a crossing object outside the viewport must be clipped"
            );
        }

        let pptx = export_presentation_pptx(&sheet, &options).unwrap();
        let slide = String::from_utf8(entry(&pptx, "ppt/slides/slide1.xml")).unwrap();
        let rels = String::from_utf8(entry(&pptx, "ppt/slides/_rels/slide1.xml.rels")).unwrap();
        assert!(slide.contains("<p:pic>"));
        assert!(slide.contains("Flattened viewport scene"));
        assert!(rels.contains("../media/slide1-viewport.png"));
        let (x, y, width, height) = picture_geometry(&slide);
        assert!((width as f64 / height as f64 - 1.5).abs() < 0.0001);
        match ratio {
            PresentationAspectRatio::Widescreen16x9 => {
                assert!(x > 0);
                assert_eq!(y, 0);
                assert_eq!(height, 6_858_000);
                assert!((2 * x + width).abs_diff(12_192_000) < 100);
            }
            PresentationAspectRatio::Standard4x3 => {
                assert_eq!(x, 0);
                assert!(y > 0);
                assert_eq!(width, 9_144_000);
                assert!((2 * y + height).abs_diff(6_858_000) < 100);
            }
        }
        let raster = image::load_from_memory(&entry(&pptx, "ppt/media/slide1-viewport.png"))
            .unwrap()
            .to_rgba8();
        assert!(has_color(&raster, BLUE));
        assert!(has_color(&raster, RED));
        assert!(!has_color(&raster, GREEN));
        assert!((raster.width() as f32 / raster.height() as f32 - 1.5).abs() < 0.01);
    }
    assert_eq!(sheet, before);
}

#[test]
fn translated_viewport_stays_clipped_in_powerpoint_preview() {
    let mut sheet = board();
    sheet.presentation.slides[0].frame = rect(50.0, 20.0, 200.0, 100.0);
    sheet
        .add_canvas_object(shape(rect(120.0, 122.0, 20.0, 10.0), GREEN))
        .unwrap();
    let options = ExportOptions::default();
    let svg = render_presentation_slide_svg(&sheet, 0, &options).unwrap();
    assert!(svg.contains("viewBox=\"50.00 20.00 200.00 100.00\""));
    let png = render_presentation_slide_png(&sheet, 0, &options).unwrap();
    let image = image::load_from_memory(&png).unwrap().to_rgba8();
    assert_eq!(image.dimensions(), (200, 100));
    assert!(!has_color(&image, BLUE));
    assert!(!has_color(&image, GREEN));
    assert!(has_color(&image, [245, 220, 80]));
    let preview = render_presentation_pptx_slide_png(&sheet, 0, &options).unwrap();
    let preview = image::load_from_memory(&preview).unwrap().to_rgba8();
    assert!(!has_color(&preview, BLUE));
    assert!(!has_color(&preview, GREEN));
    assert!(has_color(&preview, [245, 220, 80]));
}

#[test]
fn distant_unconnected_canvas_geometry_is_not_embedded_in_vector_output() {
    let mut sheet = board();
    let options = ExportOptions::default();
    let svg = render_presentation_slide_svg(&sheet, 0, &options).unwrap();
    let pdf = export_presentation_pdf(&sheet, &options).unwrap();
    sheet
        .add_canvas_object(shape(rect(800.0, 800.0, 40.0, 40.0), [3, 5, 7]))
        .unwrap();
    assert_eq!(
        render_presentation_slide_svg(&sheet, 0, &options).unwrap(),
        svg
    );
    assert_eq!(export_presentation_pdf(&sheet, &options).unwrap(), pdf);
}

#[test]
fn crossing_connector_keeps_its_distant_object_target() {
    let mut sheet = Sheet::new("Connectors", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let target = sheet
        .add_canvas_object(shape(rect(500.0, 25.0, 40.0, 50.0), RED))
        .unwrap();
    let mut connector = CanvasConnector::new(
        Endpoint::Point(Vec2::new(50.0, 50.0)),
        Endpoint::Object(target),
    );
    connector.width = 4.0;
    connector.color = Some(Color::rgb(10, 20, 30));
    connector.arrow_end = false;
    let connector = sheet
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    sheet
        .add_presentation_slide(
            "View",
            PresentationSource::Viewport,
            rect(0.0, 0.0, 100.0, 100.0),
        )
        .unwrap();
    let options = ExportOptions::default();
    let svg = render_presentation_slide_svg(&sheet, 0, &options).unwrap();
    assert!(svg.contains(&format!("id=\"object-{connector}\"")));
    let png = render_presentation_slide_png(&sheet, 0, &options).unwrap();
    let image = image::load_from_memory(&png).unwrap().to_rgba8();
    assert_eq!(image.get_pixel(90, 50).0[..3], [10, 20, 30]);
    let pptx = export_presentation_pptx(&sheet, &options).unwrap();
    let image = image::load_from_memory(&entry(&pptx, "ppt/media/slide1-viewport.png"))
        .unwrap()
        .to_rgba8();
    assert!(has_color(&image, [10, 20, 30]));
}

#[test]
fn rotated_image_card_is_visible_in_the_clipped_viewport() {
    let mut sheet = Sheet::new("Images", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut card = Card::new(CardKind::Image, Vec2::new(120.0, 80.0));
    card.rotation = 30.0;
    let topic = sheet
        .add_card("Photo", Vec2::new(140.0, 90.0), card)
        .unwrap();
    let color = [195, 40, 185];
    let mut encoded = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(8, 8, image::Rgb(color)))
        .write_to(&mut encoded, image::ImageFormat::Png)
        .unwrap();
    let resource = Resource::new("Photo", "image/png", encoded.into_inner()).unwrap();
    thinoda_core::images::set_image(&mut sheet, topic, Some((resource, Vec2::new(64.0, 64.0))))
        .unwrap();
    sheet
        .add_presentation_slide(
            "Image",
            PresentationSource::Viewport,
            rect(100.0, 50.0, 80.0, 80.0),
        )
        .unwrap();
    let options = ExportOptions::default();
    let svg = render_presentation_slide_svg(&sheet, 0, &options).unwrap();
    assert!(svg.contains("rotate(30.00"));
    assert!(svg.contains("<use xlink:href=\"#image-"));
    let png = render_presentation_slide_png(&sheet, 0, &options).unwrap();
    let image = image::load_from_memory(&png).unwrap().to_rgba8();
    assert_eq!(image.dimensions(), (80, 80));
    assert!(has_color(&image, color));
    let pdf = export_presentation_pdf(&sheet, &options).unwrap();
    assert!(String::from_utf8_lossy(&pdf).contains("/Subtype /Image"));
    let mut off_frame = sheet.clone();
    off_frame.presentation.slides[0].frame = rect(400.0, 400.0, 80.0, 80.0);
    let hidden_svg = render_presentation_slide_svg(&off_frame, 0, &options).unwrap();
    assert!(!hidden_svg.contains("data:image/png;base64,"));
    let hidden_pdf = export_presentation_pdf(&off_frame, &options).unwrap();
    assert!(
        !String::from_utf8_lossy(&hidden_pdf).contains("/Subtype /Image"),
        "off-frame source photo remains embedded"
    );
    let pptx = export_presentation_pptx(&sheet, &options).unwrap();
    let image = image::load_from_memory(&entry(&pptx, "ppt/media/slide1-viewport.png"))
        .unwrap()
        .to_rgba8();
    assert!(has_color(&image, color));
    let book = Workbook::new(sheet);
    let native = export_bytes(&book, Format::Thinoda, &options).unwrap();
    assert_eq!(import_bytes(Format::Thinoda, &native).unwrap(), book);
}

#[test]
fn malformed_off_frame_image_is_not_silently_discarded() {
    let mut sheet = board();
    let topic = sheet.floating[0];
    let reference = sheet
        .resources
        .insert(Resource::new("Invalid image", "image/png", vec![1]).unwrap())
        .unwrap();
    let topic = sheet.topic_mut(topic).unwrap();
    topic.image = Some(reference);
    topic.image_size = Some(Vec2::new(40.0, 40.0));
    sheet.presentation.slides[0].frame = rect(4000.0, 4000.0, 200.0, 200.0);
    sheet.validate().unwrap();
    let options = ExportOptions::default();
    assert!(matches!(
        render_presentation_slide_svg(&sheet, 0, &options),
        Err(IoError::Invalid(_))
    ));
    assert!(matches!(
        export_presentation_pdf(&sheet, &options),
        Err(IoError::Invalid(_))
    ));
    assert!(matches!(
        export_presentation_pptx(&sheet, &options),
        Err(IoError::Invalid(_))
    ));
}

#[test]
fn viewport_honors_hidden_subtrees_and_depth_without_connector_leaks() {
    let mut sheet = board();
    let sticky = sheet.floating[0];
    let floating = sheet.floating[1];
    let child = sheet.add_child(floating, None, "Excluded descendant");
    let mut link = CanvasConnector::new(
        Endpoint::Topic(child),
        Endpoint::Point(Vec2::new(220.0, 40.0)),
    );
    link.label = "Excluded link".into();
    let link = sheet
        .add_canvas_object(CanvasObject::connector(link))
        .unwrap();
    sheet.presentation.slides[0].hidden_topics = vec![child, sticky];
    let before = sheet.clone();
    let options = ExportOptions::default();
    let svg = render_presentation_slide_svg(&sheet, 0, &options).unwrap();
    assert!(svg.contains("Floating root"));
    assert!(!svg.contains("Sticky"));
    assert!(!svg.contains("Excluded descendant"));
    assert!(!svg.contains("Excluded link"));
    assert!(!svg.contains(&format!("id=\"object-{link}\"")));
    assert!(!svg.contains("Scene zone"));
    let png = render_presentation_slide_png(&sheet, 0, &options).unwrap();
    let image = image::load_from_memory(&png).unwrap().to_rgba8();
    assert!(!has_color(&image, [245, 220, 80]));
    let pptx = export_presentation_pptx(&sheet, &options).unwrap();
    let scene = entry(&pptx, "ppt/media/slide1-viewport.png");
    let image = image::load_from_memory(&scene).unwrap().to_rgba8();
    assert!(!has_color(&image, [245, 220, 80]));
    assert_eq!(sheet, before);

    sheet.presentation.slides[0].hidden_topics.clear();
    sheet.presentation.slides[0].max_depth = Some(0);
    let svg = render_presentation_slide_svg(&sheet, 0, &options).unwrap();
    assert!(svg.contains("Floating root"));
    assert!(svg.contains("Sticky"));
    assert!(!svg.contains("Excluded descendant"));
    assert!(!svg.contains("Excluded link"));
    assert!(!svg.contains(&format!("id=\"object-{link}\"")));
    assert!(!svg.contains("Scene zone"));
}

#[test]
fn stale_hidden_ids_preserve_connectors_to_collapsed_descendants() {
    let mut sheet = board();
    let floating = sheet.floating[1];
    let child = sheet.add_child(floating, None, "Collapsed child");
    sheet.topic_mut(floating).unwrap().collapsed = true;
    let link = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Topic(child),
            Endpoint::Point(Vec2::new(220.0, 40.0)),
        )))
        .unwrap();
    sheet.presentation.slides[0].hidden_topics = vec![TopicId(u64::MAX)];
    let svg = render_presentation_slide_svg(&sheet, 0, &ExportOptions::default()).unwrap();
    assert!(!svg.contains("Collapsed child"));
    assert!(svg.contains(&format!("id=\"object-{link}\"")));

    sheet.presentation.slides[0].hidden_topics = vec![child];
    let svg = render_presentation_slide_svg(&sheet, 0, &ExportOptions::default()).unwrap();
    assert!(!svg.contains(&format!("id=\"object-{link}\"")));
    sheet.presentation.slides[0].hidden_topics.clear();
    sheet.presentation.slides[0].max_depth = Some(0);
    let svg = render_presentation_slide_svg(&sheet, 0, &ExportOptions::default()).unwrap();
    assert!(!svg.contains(&format!("id=\"object-{link}\"")));
}

#[test]
fn viewport_depth_accounts_for_owned_summary_trees() {
    let mut sheet = Sheet::new("Map", "Central");
    let branch = sheet.add_child(sheet.root, None, "Branch");
    let first = sheet.add_child(branch, None, "First");
    let second = sheet.add_child(branch, None, "Second");
    let summary = sheet.add_summary(&[first, second], "Summary").unwrap();
    let child = sheet.add_child(summary, None, "Summary child");
    sheet.topic_mut(summary).unwrap().collapsed = true;
    let link = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Topic(child),
            Endpoint::Point(Vec2::new(0.0, 0.0)),
        )))
        .unwrap();
    sheet
        .add_presentation_slide(
            "Scene",
            PresentationSource::Viewport,
            rect(-400.0, -300.0, 800.0, 600.0),
        )
        .unwrap();
    sheet.presentation.slides[0].max_depth = Some(3);
    let svg = render_presentation_slide_svg(&sheet, 0, &ExportOptions::default()).unwrap();
    assert!(svg.contains(&format!("id=\"object-{link}\"")));
    sheet.presentation.slides[0].max_depth = Some(2);
    let svg = render_presentation_slide_svg(&sheet, 0, &ExportOptions::default()).unwrap();
    assert!(!svg.contains(&format!("id=\"object-{link}\"")));
}

#[test]
fn native_v6_round_trip_preserves_the_viewport_and_its_rendering() {
    let sheet = board();
    let book = Workbook::new(sheet.clone());
    let options = ExportOptions::default();
    let bytes = export_bytes(&book, Format::Thinoda, &options).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&entry(&bytes, "manifest.json")).unwrap();
    assert_eq!(manifest["version"], 6);
    let restored = import_bytes(Format::Thinoda, &bytes).unwrap();
    assert_eq!(restored, book);
    assert_eq!(
        restored.sheets[0].presentation.slides[0].source,
        PresentationSource::Viewport
    );
    assert_eq!(
        restored.sheets[0].presentation.slides[0].frame,
        rect(0.0, 0.0, 300.0, 200.0)
    );
    assert_eq!(
        render_presentation_slide_png(&restored.sheets[0], 0, &options).unwrap(),
        render_presentation_slide_png(&sheet, 0, &options).unwrap()
    );
}

#[test]
fn central_viewport_keeps_central_and_floating_content() {
    let mut sheet = Sheet::new("Central map", "Central topic");
    sheet.add_floating("Floating content", Vec2::new(200.0, 50.0));
    sheet
        .add_presentation_slide(
            "View",
            PresentationSource::Viewport,
            rect(-200.0, -150.0, 600.0, 400.0),
        )
        .unwrap();
    let svg = render_presentation_slide_svg(&sheet, 0, &ExportOptions::default()).unwrap();
    assert!(svg.contains("viewBox=\"-200.00 -150.00 600.00 400.00\""));
    assert!(svg.contains("Central topic"));
    assert!(svg.contains("Floating content"));
    let book = Workbook::new(sheet);
    let bytes = export_bytes(&book, Format::Thinoda, &ExportOptions::default()).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&entry(&bytes, "manifest.json")).unwrap();
    assert_eq!(manifest["version"], 6);
    assert_eq!(import_bytes(Format::Thinoda, &bytes).unwrap(), book);
}

#[test]
fn hidden_anchor_is_rejected_as_a_presentation_source() {
    let mut sheet = board();
    sheet.presentation.slides[0].source = PresentationSource::Branch(sheet.root);
    let options = ExportOptions::default();

    assert!(sheet.validate().is_err());
    assert!(render_presentation_slide_svg(&sheet, 0, &options).is_err());
    assert!(render_presentation_slide_png(&sheet, 0, &options).is_err());
    assert!(export_presentation_pdf(&sheet, &options).is_err());
    assert!(export_presentation_pptx(&sheet, &options).is_err());
}

#[test]
fn viewport_does_not_append_equations_outside_the_board_scene() {
    let mut sheet = board();
    let root = sheet.floating[0];
    let notes = thinoda_core::equations::replace_note("", Some("\\frac{a}{b}")).unwrap();
    sheet.set_markdown_notes(root, Some(notes)).unwrap();
    let options = ExportOptions::default();
    let svg = render_presentation_slide_svg(&sheet, 0, &options).unwrap();
    assert!(svg.contains("viewBox=\"0.00 0.00 300.00 200.00\""));
    assert!(!svg.contains("notes-equations"));
    assert!(!svg.contains("Notes equations"));
    assert_eq!(
        image::load_from_memory(&render_presentation_slide_png(&sheet, 0, &options).unwrap())
            .unwrap()
            .dimensions(),
        (300, 200)
    );
    assert!(export_presentation_pdf(&sheet, &options).is_ok());
    assert!(export_presentation_pptx(&sheet, &options).is_ok());

    sheet
        .set_notes(
            root,
            Some("```thinoda-math-v1\n\\input{not-executed}\n```\n".into()),
        )
        .unwrap();
    assert!(matches!(
        render_presentation_slide_svg(&sheet, 0, &options),
        Err(IoError::EquationNotesUnsupported)
    ));
    assert!(matches!(
        export_presentation_pdf(&sheet, &options),
        Err(IoError::EquationNotesUnsupported)
    ));
    assert!(matches!(
        export_presentation_pptx(&sheet, &options),
        Err(IoError::EquationNotesUnsupported)
    ));
}

#[test]
fn blank_boards_and_invalid_frames_or_sources_are_handled_predictably() {
    let mut blank = Sheet::new("Blank board", "");
    blank.root_mode = SheetRootMode::Hidden;
    blank
        .add_presentation_slide(
            "Empty",
            PresentationSource::Viewport,
            rect(5.0, 6.0, 120.0, 90.0),
        )
        .unwrap();
    let options = ExportOptions::default();
    let svg = render_presentation_slide_svg(&blank, 0, &options).unwrap();
    assert!(svg.contains("viewBox=\"5.00 6.00 120.00 90.00\""));
    assert!(!svg.contains(&format!("topic-{}-", blank.root)));
    let png = render_presentation_slide_png(&blank, 0, &options).unwrap();
    let image = image::load_from_memory(&png).unwrap().to_rgba8();
    assert_eq!(image.dimensions(), (120, 90));
    assert!(image.pixels().all(|pixel| pixel == image.get_pixel(0, 0)));
    assert!(export_presentation_pdf(&blank, &options).is_ok());
    assert!(export_presentation_pptx(&blank, &options).is_ok());
    assert!(matches!(
        render_presentation_slide_svg(&blank, 1, &options),
        Err(IoError::Invalid(_))
    ));

    for frame in [
        rect(0.0, 0.0, 0.0, 100.0),
        rect(f32::NAN, 0.0, 100.0, 100.0),
        rect(0.0, 0.0, f32::INFINITY, 100.0),
    ] {
        let mut invalid = blank.clone();
        invalid.presentation.slides[0].frame = frame;
        assert!(matches!(
            render_presentation_slide_svg(&invalid, 0, &options),
            Err(IoError::Invalid(_))
        ));
        assert!(matches!(
            export_presentation_pdf(&invalid, &options),
            Err(IoError::Invalid(_))
        ));
        assert!(matches!(
            export_presentation_pptx(&invalid, &options),
            Err(IoError::Invalid(_))
        ));
    }
    let mut oversized = blank.clone();
    oversized.presentation.slides[0].frame = rect(0.0, 0.0, 500_000.0, 500_000.0);
    assert!(matches!(
        render_presentation_slide_png(&oversized, 0, &options),
        Err(IoError::PresentationLimit(_))
    ));
    let mut invalid = blank.clone();
    invalid.presentation.slides[0].source = PresentationSource::Branch(TopicId(u64::MAX));
    assert!(matches!(
        render_presentation_slide_png(&invalid, 0, &options),
        Err(IoError::Invalid(_))
    ));
    assert!(matches!(
        export_presentation_pptx(&invalid, &options),
        Err(IoError::Invalid(_))
    ));
    let mut invalid = blank.clone();
    invalid.presentation.slides[0].source = PresentationSource::Zone(ZoneId(u64::MAX));
    assert!(matches!(
        export_presentation_pdf(&invalid, &options),
        Err(IoError::Invalid(_))
    ));
    assert!(matches!(
        export_presentation_pptx(&invalid, &options),
        Err(IoError::Invalid(_))
    ));
}

#[test]
fn viewport_renders_standalone_board_frames_without_member_topics() {
    let mut sheet = Sheet::new("Blank board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let zone = sheet
        .create_board_frame("Standalone frame", rect(20.0, 20.0, 120.0, 100.0))
        .unwrap();
    sheet
        .zones
        .iter_mut()
        .find(|item| item.id == zone)
        .unwrap()
        .color = Some(Color::rgb(110, 30, 180));
    sheet
        .add_presentation_slide(
            "View",
            PresentationSource::Viewport,
            rect(0.0, 0.0, 180.0, 150.0),
        )
        .unwrap();

    let options = ExportOptions::default();
    let svg = render_presentation_slide_svg(&sheet, 0, &options).unwrap();
    assert!(svg.contains("Standalone frame"));
    assert!(svg.contains("fill=\"#6E1EB414\" stroke=\"#6E1EB4\""));
    let assert_tinted_frame = |image: &RgbaImage| {
        let sample = |x, y| {
            image
                .get_pixel(image.width() * x / 180, image.height() * y / 150)
                .0
        };
        let inside = sample(80, 80);
        let background = sample(10, 10);
        assert_ne!(inside, background, "frame fill must be visible");
        assert!(
            inside[0] > inside[1] && inside[2] > inside[1],
            "frame fill must retain its purple tint: {inside:?}"
        );
    };
    let image =
        image::load_from_memory(&render_presentation_slide_png(&sheet, 0, &options).unwrap())
            .unwrap()
            .to_rgba8();
    assert_tinted_frame(&image);
    let pdf = export_presentation_pdf(&sheet, &options).unwrap();
    let mut without_frame = sheet.clone();
    without_frame.zones.clear();
    assert_ne!(
        pdf,
        export_presentation_pdf(&without_frame, &options).unwrap()
    );
    let pptx = export_presentation_pptx(&sheet, &options).unwrap();
    let image = image::load_from_memory(&entry(&pptx, "ppt/media/slide1-viewport.png"))
        .unwrap()
        .to_rgba8();
    assert_tinted_frame(&image);
}

#[test]
fn generated_hidden_board_slides_follow_semantic_roots_then_frames() {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let later = sheet.add_floating("Later", Vec2::new(500.0, 100.0));
    let first = sheet.add_floating("First", Vec2::new(100.0, 100.0));
    let first_child = sheet.add_child(first, None, "First child");
    let zone = sheet
        .create_zone("Zone", vec![first], rect(40.0, 40.0, 200.0, 150.0))
        .unwrap();
    let empty = sheet
        .create_board_frame("Empty frame", rect(700.0, 40.0, 150.0, 120.0))
        .unwrap();

    let presentation = generate_presentation(&sheet, &ExportOptions::default()).unwrap();
    assert_eq!(
        presentation
            .slides
            .iter()
            .map(|slide| slide.source)
            .collect::<Vec<_>>(),
        vec![
            PresentationSource::Branch(first),
            PresentationSource::Branch(later),
            PresentationSource::Zone(zone),
            PresentationSource::Zone(empty),
        ]
    );
    assert!(
        !presentation
            .slides
            .iter()
            .any(|slide| slide.source == PresentationSource::Branch(sheet.root))
    );
    presentation.validate(&sheet).unwrap();
    sheet.presentation = presentation;
    let branch = render_presentation_slide_svg(&sheet, 0, &ExportOptions::default()).unwrap();
    assert!(branch.contains("First"));
    assert!(branch.contains("First child"));
    assert!(!branch.contains("Later"));
    let empty_frame = render_presentation_slide_svg(&sheet, 3, &ExportOptions::default()).unwrap();
    assert!(empty_frame.contains("Empty frame"));
    assert!(export_presentation_pdf(&sheet, &ExportOptions::default()).is_ok());
    assert!(export_presentation_pptx(&sheet, &ExportOptions::default()).is_ok());
    assert!(sheet.topic(first_child).is_some());

    let mut blank = Sheet::new("Blank", "");
    blank.root_mode = SheetRootMode::Hidden;
    assert!(
        generate_presentation(&blank, &ExportOptions::default())
            .unwrap()
            .slides
            .is_empty()
    );
}

#[test]
fn generated_central_slides_append_floating_roots_after_legacy_order() {
    let mut sheet = Sheet::new("Map", "Central");
    let first = sheet.add_child(sheet.root, None, "First branch");
    let second = sheet.add_child(sheet.root, None, "Second branch");
    let zoned = sheet.add_floating("Zoned root", Vec2::new(500.0, 100.0));
    let floating = sheet.add_floating("Floating root", Vec2::new(900.0, 100.0));
    sheet.add_child(floating, None, "Floating detail");
    let zone = sheet
        .create_zone("Zone", vec![zoned], rect(400.0, 0.0, 300.0, 200.0))
        .unwrap();
    let options = ExportOptions::default();
    let presentation = generate_presentation(&sheet, &options).unwrap();
    assert_eq!(
        presentation
            .slides
            .iter()
            .map(|slide| slide.source)
            .collect::<Vec<_>>(),
        vec![
            PresentationSource::Branch(first),
            PresentationSource::Branch(second),
            PresentationSource::Zone(zone),
            PresentationSource::Branch(floating),
        ]
    );
    presentation.validate(&sheet).unwrap();
    sheet.presentation = presentation;
    let svg = render_presentation_slide_svg(&sheet, 3, &options).unwrap();
    assert!(svg.contains("Floating root"));
    assert!(svg.contains("Floating detail"));
    assert!(!svg.contains("First branch"));
    let pptx = export_presentation_pptx(&sheet, &options).unwrap();
    let slide = String::from_utf8(entry(&pptx, "ppt/slides/slide4.xml")).unwrap();
    assert!(slide.contains("Floating root"));
    assert!(slide.contains("Floating detail"));
}

#[test]
fn presentation_branch_keeps_its_scope_through_hide_and_show() {
    let mut sheet = Sheet::new("Map", "Central");
    let branch = sheet.add_child(sheet.root, None, "Chosen branch");
    sheet.add_child(branch, None, "Chosen child");
    sheet.add_child(sheet.root, None, "Other branch");
    sheet
        .add_presentation_slide(
            "Branch",
            PresentationSource::Branch(branch),
            rect(-300.0, -200.0, 600.0, 400.0),
        )
        .unwrap();
    sheet.hide_central_topic().unwrap();
    let options = ExportOptions::default();
    for mode in 0..2 {
        sheet.validate().unwrap();
        let svg = render_presentation_slide_svg(&sheet, 0, &options).unwrap();
        assert!(svg.contains("Chosen branch"));
        assert!(svg.contains("Chosen child"));
        assert!(!svg.contains("Other branch"));
        assert!(!svg.contains(">Central</text>"));
        let pptx = export_presentation_pptx(&sheet, &options).unwrap();
        let slide = String::from_utf8(entry(&pptx, "ppt/slides/slide1.xml")).unwrap();
        assert!(slide.contains("Chosen branch"));
        assert!(!slide.contains("Other branch"));
        assert!(export_presentation_pdf(&sheet, &options).is_ok());
        if mode == 0 {
            sheet.show_central_topic();
        }
    }
}

#[test]
fn viewport_honors_scene_item_frame_clipping_in_svg_png_pdf_and_pptx() {
    let mut sheet = Sheet::new("Clipped board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let zone = sheet
        .create_board_frame("Clip", rect(0.0, 0.0, 100.0, 100.0))
        .unwrap();
    sheet
        .zones
        .iter_mut()
        .find(|item| item.id == zone)
        .unwrap()
        .clip = true;
    let object = sheet
        .add_canvas_object(shape(rect(60.0, 20.0, 60.0, 30.0), RED))
        .unwrap();
    sheet
        .add_presentation_slide(
            "View",
            PresentationSource::Viewport,
            rect(0.0, 0.0, 160.0, 120.0),
        )
        .unwrap();
    let options = ExportOptions::default();
    let svg = render_presentation_slide_svg(&sheet, 0, &options).unwrap();
    assert!(svg.contains(&format!("scene-clip-object-{}", object.0)));
    let png = render_presentation_slide_png(&sheet, 0, &options).unwrap();
    let image = image::load_from_memory(&png).unwrap().to_rgba8();
    assert_eq!(image.get_pixel(80, 35).0[..3], RED);
    assert_ne!(image.get_pixel(110, 35).0[..3], RED);
    let pdf = export_presentation_pdf(&sheet, &options).unwrap();
    let mut unclipped = sheet.clone();
    unclipped.zones[0].clip = false;
    assert_ne!(pdf, export_presentation_pdf(&unclipped, &options).unwrap());
    let pptx = export_presentation_pptx(&sheet, &options).unwrap();
    let image = image::load_from_memory(&entry(&pptx, "ppt/media/slide1-viewport.png"))
        .unwrap()
        .to_rgba8();
    let point = |x: u32, y: u32| {
        image
            .get_pixel(x * image.width() / 160, y * image.height() / 120)
            .0[..3]
            .to_vec()
    };
    assert_eq!(point(80, 35), RED);
    assert_ne!(point(110, 35), RED);
}

#[test]
fn mixed_deck_keeps_legacy_branch_and_zone_slides_editable() {
    let mut sheet = Sheet::new("Map", "Central");
    let branch = sheet.add_child(sheet.root, None, "Legacy branch");
    let floating = sheet.add_floating("Legacy floating", Vec2::new(240.0, 120.0));
    let zone = sheet
        .create_zone(
            "Legacy zone",
            vec![floating],
            rect(160.0, 60.0, 160.0, 120.0),
        )
        .unwrap();
    for (name, source, frame) in [
        (
            "Branch",
            PresentationSource::Branch(branch),
            rect(-100.0, -100.0, 300.0, 200.0),
        ),
        (
            "Zone",
            PresentationSource::Zone(zone),
            rect(160.0, 60.0, 160.0, 120.0),
        ),
        (
            "Scene",
            PresentationSource::Viewport,
            rect(-100.0, -100.0, 300.0, 200.0),
        ),
    ] {
        sheet.add_presentation_slide(name, source, frame).unwrap();
    }

    for slide in &mut sheet.presentation.slides[..2] {
        slide.scope = PresentationScope::Context;
        slide.auto_frame = false;
    }
    let options = ExportOptions::default();
    let branch_svg = render_presentation_slide_svg(&sheet, 0, &options).unwrap();
    assert!(branch_svg.contains("viewBox=\"-100.00 -100.00 300.00 200.00\""));
    assert!(branch_svg.contains("Central"));
    let pptx = export_presentation_pptx(&sheet, &options).unwrap();
    let branch_xml = String::from_utf8(entry(&pptx, "ppt/slides/slide1.xml")).unwrap();
    let zone_xml = String::from_utf8(entry(&pptx, "ppt/slides/slide2.xml")).unwrap();
    assert!(branch_xml.contains("<a:t>Legacy branch</a:t>"));
    assert!(zone_xml.contains("<a:t>Legacy zone</a:t>"));
    assert!(!branch_xml.contains("<p:pic>"));
    assert!(!zone_xml.contains("<p:pic>"));
    assert!(
        String::from_utf8(entry(&pptx, "ppt/slides/slide3.xml"))
            .unwrap()
            .contains("<p:pic>")
    );
}

#[test]
fn clipped_relationships_do_not_make_auto_framed_exports_too_large() {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let root = sheet.add_floating("Root", Vec2::new(100.0, 100.0));
    let first = sheet.add_child(root, None, "First");
    let second = sheet.add_child(root, None, "Second");
    let relationship = sheet.add_relationship(first, second).unwrap();
    sheet
        .set_relationship_control_points(relationship, Some([Vec2::new(30_000.0, 30_000.0); 2]))
        .unwrap();
    let frame = rect(0.0, 0.0, 320.0, 240.0);
    let zone = sheet.create_zone("Frame", vec![root], frame).unwrap();
    sheet.zones[0].clip = true;
    sheet
        .add_canvas_object(shape(rect(20.0, 20.0, 20.0, 20.0), BLUE))
        .unwrap();
    for (title, source) in [
        ("Branch", PresentationSource::Branch(root)),
        ("Zone", PresentationSource::Zone(zone)),
    ] {
        sheet.add_presentation_slide(title, source, frame).unwrap();
    }

    let options = ExportOptions::default();
    for index in 0..2 {
        let svg = render_presentation_slide_svg(&sheet, index, &options).unwrap();
        assert!(svg.contains("viewBox=\"-40.00 -40.00 400.00 320.00\""));
        assert!(svg.contains(&format!("scene-clip-relationship-{relationship}")));
        let png = render_presentation_slide_png(&sheet, index, &options).unwrap();
        let png = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(png.dimensions(), (400, 320));
        assert!(has_color(&png, BLUE));
    }
    assert!(export_presentation_pdf(&sheet, &options).is_ok());

    // The existing no-clip path continues to frame the full control hull.
    sheet.zones[0].clip = false;
    let svg = render_presentation_slide_svg(&sheet, 0, &options).unwrap();
    let width: f32 = svg
        .split_once("viewBox=\"")
        .unwrap()
        .1
        .split('"')
        .next()
        .unwrap()
        .split_whitespace()
        .nth(2)
        .unwrap()
        .parse()
        .unwrap();
    assert!(width > 30_000.0);
    assert!(matches!(
        render_presentation_slide_png(&sheet, 0, &options),
        Err(IoError::PresentationLimit(_))
    ));
}

#[test]
fn coincident_distinct_frames_do_not_clip_context_relationships() {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let first = sheet.add_floating("First", Vec2::new(100.0, 100.0));
    let second = sheet.add_floating("Second", Vec2::new(160.0, 160.0));
    let frame = rect(0.0, 0.0, 320.0, 240.0);
    sheet
        .create_zone("First frame", vec![first], frame)
        .unwrap();
    sheet
        .create_zone("Second frame", vec![second], frame)
        .unwrap();
    for zone in &mut sheet.zones {
        zone.clip = true;
    }
    let relationship = sheet.add_relationship(first, second).unwrap();
    sheet
        .set_relationship_control_points(relationship, Some([Vec2::new(30_000.0, 30_000.0); 2]))
        .unwrap();
    sheet
        .add_presentation_slide("Context", PresentationSource::Branch(first), frame)
        .unwrap();
    sheet.presentation.slides[0].scope = PresentationScope::Context;

    let options = ExportOptions::default();
    let svg = render_presentation_slide_svg(&sheet, 0, &options).unwrap();
    assert!(!svg.contains(&format!("scene-clip-relationship-{relationship}")));
    let width: f32 = svg
        .split_once("viewBox=\"")
        .unwrap()
        .1
        .split('"')
        .next()
        .unwrap()
        .split_whitespace()
        .nth(2)
        .unwrap()
        .parse()
        .unwrap();
    assert!(width > 30_000.0);
    assert!(matches!(
        render_presentation_slide_png(&sheet, 0, &options),
        Err(IoError::PresentationLimit(_))
    ));
}

#[test]
fn generated_branch_frames_clip_deep_boundary_and_summary_bounds() {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let root = sheet.add_floating("Root", Vec2::new(100.0, 100.0));
    let first = sheet.add_child(root, None, "First");
    let mut parent = first;
    for _ in 0..24 {
        parent = sheet.add_child(parent, None, "Deep descendant");
    }
    sheet.add_boundary(&[first]).unwrap();
    sheet.add_summary(&[first], "Summary").unwrap();
    let frame = rect(0.0, 0.0, 320.0, 240.0);
    sheet.create_zone("Frame", vec![root], frame).unwrap();
    sheet.zones[0].clip = true;
    let options = ExportOptions::default();

    let clipped = generate_presentation(&sheet, &options).unwrap();
    let branch = clipped
        .slides
        .iter()
        .find(|slide| slide.source == PresentationSource::Branch(root))
        .unwrap();
    assert!(branch.frame.min.x >= frame.min.x - 80.0);
    assert!(branch.frame.min.y >= frame.min.y - 80.0);
    assert!(branch.frame.max().x <= frame.max().x + 80.0);
    assert!(branch.frame.max().y <= frame.max().y + 80.0);

    sheet.zones[0].clip = false;
    let legacy = generate_presentation(&sheet, &options).unwrap();
    let branch = legacy
        .slides
        .iter()
        .find(|slide| slide.source == PresentationSource::Branch(root))
        .unwrap();
    assert!(
        branch.frame.max().x > frame.max().x + 1_000.0,
        "unclipped generation must retain the full deep branch"
    );
}

#[test]
fn generated_branch_frames_include_clipped_relationships_and_their_labels() {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let root = sheet.add_floating("Root", Vec2::new(1_200.0, 300.0));
    let first = sheet.add_child(root, None, "First");
    let second = sheet.add_child(root, None, "Second");
    let relationship = sheet.add_relationship(first, second).unwrap();
    sheet
        .set_relationship_control_points(relationship, Some([Vec2::new(-2_400.0, 0.0); 2]))
        .unwrap();
    let clip = rect(0.0, 0.0, 1_500.0, 600.0);
    sheet.create_zone("Frame", vec![root], clip).unwrap();
    sheet.zones[0].clip = true;
    let options = ExportOptions::default();

    let generated = generate_presentation(&sheet, &options).unwrap();
    let index = generated
        .slides
        .iter()
        .position(|slide| slide.source == PresentationSource::Branch(root))
        .unwrap();
    let frame = generated.slides[index].frame;
    assert_eq!(frame.min.x, clip.min.x - 80.0);
    assert!(frame.max().x <= clip.max().x + 80.0);
    let mut manual = sheet.clone();
    manual.presentation = generated;
    manual.presentation.slides[index].auto_frame = false;
    let svg = render_presentation_slide_svg(&manual, index, &options).unwrap();
    assert!(svg.contains(&format!("scene-clip-relationship-{relationship}")));
    assert!(render_presentation_slide_png(&manual, index, &options).is_ok());

    // Legacy generated frames did not include relationship control points.
    sheet.zones[0].clip = false;
    let legacy = generate_presentation(&sheet, &options).unwrap();
    let legacy = legacy
        .slides
        .iter()
        .find(|slide| slide.source == PresentationSource::Branch(root))
        .unwrap();
    assert!(legacy.frame.min.x > 800.0);

    // A clipped label is unmeasured, so conservatively retain the whole clip.
    sheet.zones[0].clip = true;
    sheet
        .set_relationship_control_points(relationship, None)
        .unwrap();
    sheet
        .set_relationship_title(relationship, &"A long label ".repeat(128))
        .unwrap();
    let labelled = generate_presentation(&sheet, &options).unwrap();
    let labelled = labelled
        .slides
        .iter()
        .find(|slide| slide.source == PresentationSource::Branch(root))
        .unwrap();
    assert_eq!(labelled.frame, clip.expand(80.0));
}

#[test]
fn generated_branch_frames_include_a_clipped_fishbone_spine() {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let root = sheet
        .add_card(
            "Root",
            Vec2::new(1_400.0, 500.0),
            Card::new(CardKind::Sticky, Vec2::new(80.0, 140.0)),
        )
        .unwrap();
    sheet.topic_mut(root).unwrap().structure = Some(thinoda_core::Structure::FishboneRight);
    for index in 0..50 {
        sheet.add_child(root, None, format!("Long child {index}"));
    }
    let clip = rect(0.0, 450.0, 1_500.0, 100.0);
    sheet.create_zone("Frame", vec![root], clip).unwrap();
    sheet.zones[0].clip = true;

    let layout = thinoda_core::layout_sheet(&sheet, &thinoda_core::ApproxMeasure, false);
    let spine = layout
        .connectors
        .iter()
        .find(|connector| connector.parent == root && connector.child == root)
        .unwrap();
    assert!(spine.start.x > clip.min.x && spine.end.x < clip.min.x);
    let scene = thinoda_core::build_scene(&sheet, &layout);
    assert!(scene.items.iter().all(|item| {
        !matches!(item.id, thinoda_core::SceneId::Topic(id) if id != root)
            || item.visible_bounds().is_none()
    }));

    let generated = generate_presentation(&sheet, &ExportOptions::default()).unwrap();
    let branch = generated
        .slides
        .iter()
        .find(|slide| slide.source == PresentationSource::Branch(root))
        .unwrap();
    assert_eq!(branch.frame.min.x, clip.min.x - 80.0);
    assert!(branch.frame.max().x <= clip.max().x + 80.0);
}

#[test]
fn generated_branch_frames_include_strokes_touching_the_clip_from_outside() {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let root = sheet
        .add_card(
            "Root",
            Vec2::new(1_400.0, 500.0),
            Card::new(CardKind::Sticky, Vec2::new(80.0, 160.0)),
        )
        .unwrap();
    let topic = sheet.topic_mut(root).unwrap();
    topic.structure = Some(thinoda_core::Structure::FishboneRight);
    topic.style.line_width = Some(8.0);
    let mut child = sheet.add_child(root, None, "Long branch");
    for index in 0..30 {
        child = sheet.add_child(child, None, format!("Long descendant {index}"));
    }
    // The spine centerline is above the frame, but its four-unit half-stroke
    // overlaps it. All the descendant topics remain completely above the clip.
    let clip = rect(0.0, 503.0, 1_500.0, 100.0);
    sheet.create_zone("Frame", vec![root], clip).unwrap();
    sheet.zones[0].clip = true;
    let layout = thinoda_core::layout_sheet(&sheet, &thinoda_core::ApproxMeasure, false);
    let spine = layout
        .connectors
        .iter()
        .find(|connector| connector.parent == root && connector.child == root)
        .unwrap();
    assert!(spine.start.y < clip.min.y && spine.end.y < clip.min.y);
    assert!(spine.start.y + spine.width / 2.0 > clip.min.y);
    assert!(spine.start.x > clip.min.x && spine.end.x < clip.min.x);
    let scene = thinoda_core::build_scene(&sheet, &layout);
    assert!(scene.items.iter().all(|item| {
        !matches!(item.id, thinoda_core::SceneId::Topic(id) if id != root)
            || item.visible_bounds().is_none()
    }));

    let generated = generate_presentation(&sheet, &ExportOptions::default()).unwrap();
    let branch = generated
        .slides
        .iter()
        .find(|slide| slide.source == PresentationSource::Branch(root))
        .unwrap();
    assert_eq!(branch.frame.min.x, clip.min.x - 80.0);
    assert!(branch.frame.max().x <= clip.max().x + 80.0);
}

#[test]
fn generated_branch_with_all_topics_clipped_uses_its_owning_frame() {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let root = sheet.add_floating("Far away", Vec2::new(5_000.0, 5_000.0));
    sheet.add_child(root, None, "Also far away");
    let clip = rect(0.0, 0.0, 160.0, 120.0);
    sheet.create_zone("Frame", vec![root], clip).unwrap();
    sheet.zones[0].clip = true;
    let options = ExportOptions::default();

    let generated = generate_presentation(&sheet, &options).unwrap();
    let branch = generated
        .slides
        .iter()
        .find(|slide| slide.source == PresentationSource::Branch(root))
        .unwrap();
    assert_eq!(branch.frame, clip.expand(80.0));

    sheet.zones[0].clip = false;
    let legacy = generate_presentation(&sheet, &options).unwrap();
    let branch = legacy
        .slides
        .iter()
        .find(|slide| slide.source == PresentationSource::Branch(root))
        .unwrap();
    assert!(branch.frame.min.x > 4_000.0);
}
