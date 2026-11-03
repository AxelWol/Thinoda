use std::io::{Cursor, Read};

use thinoda_core::{
    CanvasObject, Card, CardKind, Color, PresentationScope, PresentationSource, Rect, ShapeKind,
    Sheet, SheetRootMode, TopicShape, Vec2, board::CanvasObjectKind, resources::Resource,
};
use thinoda_io::{
    ExportOptions, IoError, PresentationAspectRatio, export_presentation_pdf,
    export_presentation_png_zip, export_presentation_pptx, fit_presentation_frame,
    generate_presentation, render_presentation_slide_svg,
};
use zip::ZipArchive;

fn xml(bytes: &[u8], path: &str) -> String {
    let mut zip = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut value = String::new();
    zip.by_name(path)
        .unwrap()
        .read_to_string(&mut value)
        .unwrap();
    value
}

fn photo(sheet: &mut Sheet, id: thinoda_core::TopicId, format: image::ImageFormat) -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
        8,
        4,
        image::Rgb([30, 120, 210]),
    ))
    .write_to(&mut bytes, format)
    .unwrap();
    let bytes = bytes.into_inner();
    let media = if format == image::ImageFormat::Png {
        "image/png"
    } else {
        "image/jpeg"
    };
    let resource = Resource::new("Original test photo", media, bytes.clone()).unwrap();
    thinoda_core::images::set_image(sheet, id, Some((resource, Vec2::new(80.0, 40.0)))).unwrap();
    bytes
}

#[derive(Debug)]
struct Transform {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    rotation: Option<i64>,
}

impl Transform {
    fn center(&self) -> (f64, f64) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }
}

fn element<'a>(slide: &'a str, tag: &str, marker: &str) -> &'a str {
    let start = format!("<p:{tag}>");
    let end = format!("</p:{tag}>");
    slide
        .split(&start)
        .skip(1)
        .filter_map(|part| part.split_once(&end).map(|(element, _)| element))
        .find(|element| element.contains(marker))
        .unwrap_or_else(|| panic!("missing {tag} containing {marker}"))
}

fn attr(tag: &str, name: &str) -> Option<i64> {
    tag.split_once(&format!("{name}=\""))
        .map(|(_, rest)| rest.split('"').next().unwrap().parse().unwrap())
}

fn transform(element: &str) -> Transform {
    let after_start = element.split_once("<a:xfrm").unwrap().1;
    let (opening, body) = after_start.split_once('>').unwrap();
    let offset = body
        .split_once("<a:off ")
        .unwrap()
        .1
        .split_once("/>")
        .unwrap()
        .0;
    let extent = body
        .split_once("<a:ext ")
        .unwrap()
        .1
        .split_once("/>")
        .unwrap()
        .0;
    Transform {
        x: attr(offset, "x").unwrap() as f64,
        y: attr(offset, "y").unwrap() as f64,
        width: attr(extent, "cx").unwrap() as f64,
        height: attr(extent, "cy").unwrap() as f64,
        rotation: attr(opening, "rot"),
    }
}

fn assert_rotated(original: &Transform, actual: &Transform, pivot: (f64, f64), degrees: f64) {
    assert_eq!(original.rotation, None);
    assert_eq!(actual.rotation, Some((degrees * 60_000.0).round() as i64));
    assert_eq!(actual.width, original.width);
    assert_eq!(actual.height, original.height);
    let center = original.center();
    let (sin, cos) = degrees.to_radians().sin_cos();
    let delta = (center.0 - pivot.0, center.1 - pivot.1);
    let expected = (
        pivot.0 + delta.0 * cos - delta.1 * sin,
        pivot.1 + delta.0 * sin + delta.1 * cos,
    );
    let actual_center = actual.center();
    assert!(
        (actual_center.0 - expected.0).abs() <= 20.0
            && (actual_center.1 - expected.1).abs() <= 20.0,
        "rotated center {actual_center:?} differs from {expected:?}; original {original:?}"
    );
}

fn points(element: &str) -> Vec<(f64, f64)> {
    element
        .split("<a:pt ")
        .skip(1)
        .map(|part| {
            let tag = part.split_once("/>").unwrap().0;
            (
                attr(tag, "x").unwrap() as f64,
                attr(tag, "y").unwrap() as f64,
            )
        })
        .collect()
}

fn assert_rotated_points(
    original: &[(f64, f64)],
    actual: &[(f64, f64)],
    pivot: (f64, f64),
    degrees: f64,
) {
    assert_eq!(original.len(), actual.len());
    assert!(original.len() >= 2);
    let (sin, cos) = degrees.to_radians().sin_cos();
    for (before, after) in original.iter().zip(actual) {
        let delta = (before.0 - pivot.0, before.1 - pivot.1);
        let expected = (
            pivot.0 + delta.0 * cos - delta.1 * sin,
            pivot.1 + delta.0 * sin + delta.1 * cos,
        );
        assert!(
            (after.0 - expected.0).abs() <= 20.0 && (after.1 - expected.1).abs() <= 20.0,
            "rotated point {after:?} differs from {expected:?}"
        );
    }
}

#[test]
fn editable_slides_rotate_cards_photos_and_multiline_shape_labels_about_their_pivots() {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let sticky = sheet
        .add_card(
            "Sticky first\nSticky second",
            Vec2::new(50.0, 60.0),
            Card::new(CardKind::Sticky, Vec2::new(180.0, 100.0)),
        )
        .unwrap();
    sheet.topic_mut(sticky).unwrap().labels = vec!["Card tag".into()];
    let image_card = sheet
        .add_card(
            "Image caption",
            Vec2::new(350.0, 90.0),
            Card::new(CardKind::Image, Vec2::new(180.0, 130.0)),
        )
        .unwrap();
    sheet.topic_mut(image_card).unwrap().style.shape = Some(TopicShape::Underline);
    let jpeg = photo(&mut sheet, image_card, image::ImageFormat::Jpeg);
    let untouched = sheet
        .add_card(
            "Unrotated card",
            Vec2::new(600.0, 100.0),
            Card::new(CardKind::Text, Vec2::new(150.0, 100.0)),
        )
        .unwrap();
    let mut shape = CanvasObject::shape(
        ShapeKind::Triangle,
        Rect::from_min_size(Vec2::new(120.0, 290.0), Vec2::new(180.0, 130.0)),
    );
    if let CanvasObjectKind::Shape { label, .. } = &mut shape.kind {
        *label = "Shape A\nShape B".into();
    }
    let shape_id = sheet.add_canvas_object(shape).unwrap();
    let frame = Rect::from_min_size(Vec2::new(-250.0, -250.0), Vec2::new(1200.0, 800.0));
    let zone = sheet
        .create_zone("Frame", vec![sticky, image_card, untouched], frame)
        .unwrap();
    sheet
        .add_presentation_slide("Editable", PresentationSource::Zone(zone), frame)
        .unwrap();
    sheet.presentation.slides[0].auto_frame = false;
    sheet.validate().unwrap();

    let mut rotated = sheet.clone();
    rotated
        .topic_mut(sticky)
        .unwrap()
        .card
        .as_mut()
        .unwrap()
        .rotation = 90.0;
    rotated
        .topic_mut(image_card)
        .unwrap()
        .card
        .as_mut()
        .unwrap()
        .rotation = 315.0;
    rotated.canvas.object_mut(shape_id).unwrap().rotation = 45.0;
    rotated.validate().unwrap();
    let snapshot = rotated.clone();

    for ratio in [
        PresentationAspectRatio::Widescreen16x9,
        PresentationAspectRatio::Standard4x3,
    ] {
        let options = ExportOptions {
            presentation_aspect_ratio: ratio,
            ..ExportOptions::default()
        };
        let before = xml(
            &export_presentation_pptx(&sheet, &options).unwrap(),
            "ppt/slides/slide1.xml",
        );
        let output = export_presentation_pptx(&rotated, &options).unwrap();
        let after = xml(&output, "ppt/slides/slide1.xml");
        let sticky_marker = format!("name=\"Topic {sticky}\"");
        let sticky_before = transform(element(&before, "sp", &sticky_marker));
        let sticky_after = transform(element(&after, "sp", &sticky_marker));
        let sticky_pivot = sticky_before.center();
        assert_rotated(&sticky_before, &sticky_after, sticky_pivot, 90.0);
        for text in ["Sticky first", "Sticky second", "Card tag"] {
            let marker = format!("<a:t>{text}</a:t>");
            assert_rotated(
                &transform(element(&before, "sp", &marker)),
                &transform(element(&after, "sp", &marker)),
                sticky_pivot,
                90.0,
            );
        }

        let image_marker = format!("name=\"Topic {image_card}\"");
        let image_before = transform(element(&before, "sp", &image_marker));
        let image_pivot = image_before.center();
        assert_rotated(
            &image_before,
            &transform(element(&after, "sp", &image_marker)),
            image_pivot,
            315.0,
        );
        assert_rotated(
            &transform(element(&before, "pic", "name=\"Topic photo\"")),
            &transform(element(&after, "pic", "name=\"Topic photo\"")),
            image_pivot,
            315.0,
        );
        assert_rotated(
            &transform(element(&before, "sp", "<a:t>Image caption</a:t>")),
            &transform(element(&after, "sp", "<a:t>Image caption</a:t>")),
            image_pivot,
            315.0,
        );
        assert_rotated_points(
            &points(element(&before, "sp", "name=\"Topic underline\"")),
            &points(element(&after, "sp", "name=\"Topic underline\"")),
            image_pivot,
            315.0,
        );
        let mut archive = ZipArchive::new(Cursor::new(&output)).unwrap();
        let mut embedded = Vec::new();
        archive
            .by_name("ppt/media/slide1-photo1.jpg")
            .unwrap()
            .read_to_end(&mut embedded)
            .unwrap();
        assert_eq!(embedded, jpeg);

        let first = "<a:t>Shape A</a:t>";
        let second = "<a:t>Shape B</a:t>";
        let first_before = transform(element(&before, "sp", first));
        let second_before = transform(element(&before, "sp", second));
        let outline = points(element(
            &before,
            "sp",
            &format!("name=\"Canvas object {shape_id}\""),
        ));
        let shape_pivot = (
            (outline.iter().map(|p| p.0).fold(f64::INFINITY, f64::min)
                + outline
                    .iter()
                    .map(|p| p.0)
                    .fold(f64::NEG_INFINITY, f64::max))
                / 2.0,
            (outline.iter().map(|p| p.1).fold(f64::INFINITY, f64::min)
                + outline
                    .iter()
                    .map(|p| p.1)
                    .fold(f64::NEG_INFINITY, f64::max))
                / 2.0,
        );
        assert_rotated(
            &first_before,
            &transform(element(&after, "sp", first)),
            shape_pivot,
            45.0,
        );
        assert_rotated(
            &second_before,
            &transform(element(&after, "sp", second)),
            shape_pivot,
            45.0,
        );
        assert_eq!(
            transform(element(
                &after,
                "sp",
                &format!("name=\"Topic {untouched}\"")
            ))
            .rotation,
            None
        );
        assert_eq!(rotated, snapshot);
    }
}

#[test]
fn live_source_slide_filters_text_relationships_zones_and_hidden_photo_bytes() {
    let mut sheet = Sheet::new("Map", "Unrelated central");
    let source = sheet.add_child(sheet.root, None, "Focused branch");
    let child = sheet.add_child(source, None, "Visible descendant");
    let sibling = sheet.add_child(sheet.root, None, "Excluded sibling secret");
    let floating = sheet.add_floating("Excluded floating secret", Vec2::new(300.0, 100.0));
    sheet
        .create_zone(
            "Excluded zone secret",
            vec![floating],
            Rect::from_min_size(Vec2::new(200.0, 0.0), Vec2::new(300.0, 300.0)),
        )
        .unwrap();
    let relationship = sheet.add_relationship(child, sibling).unwrap();
    sheet
        .relationships
        .iter_mut()
        .find(|r| r.id == relationship)
        .unwrap()
        .title = "Excluded relationship secret".into();
    photo(&mut sheet, sibling, image::ImageFormat::Png);
    sheet.presentation = generate_presentation(&sheet, &ExportOptions::default()).unwrap();
    sheet.presentation.slides.truncate(1);
    assert_eq!(
        sheet.presentation.slides[0].scope,
        PresentationScope::Source
    );
    assert!(sheet.presentation.slides[0].auto_frame);
    let options = ExportOptions::default();
    let svg = render_presentation_slide_svg(&sheet, 0, &options).unwrap();
    assert!(svg.contains("Visible descendant"));
    assert!(!svg.contains("secret"));
    assert!(!svg.contains("Unrelated central"));
    assert!(!svg.contains("base64,"));
    let pptx = export_presentation_pptx(&sheet, &options).unwrap();
    let slide = xml(&pptx, "ppt/slides/slide1.xml");
    assert!(slide.contains("Visible descendant"));
    assert!(!slide.contains("secret"));
    assert!(!slide.contains("<p:pic>"));
    assert!(slide.contains("<a:custGeom>"));
    assert!(!slide.contains("<a:blip "));
    assert!(
        export_presentation_pdf(&sheet, &options)
            .unwrap()
            .starts_with(b"%PDF-")
    );
    assert!(
        !export_presentation_png_zip(&sheet, &options)
            .unwrap()
            .is_empty()
    );

    let saved_id = sheet.presentation.slides[0].id;
    sheet.topic_mut(child).unwrap().title = "Changed live title".into();
    let added = sheet.add_child(source, None, "New live descendant");
    let svg = render_presentation_slide_svg(&sheet, 0, &options).unwrap();
    assert!(svg.contains("Changed live title"));
    assert!(svg.contains("New live descendant"));
    sheet.presentation.slides[0].hidden_topics = vec![added, thinoda_core::TopicId(u64::MAX)];
    let svg = render_presentation_slide_svg(&sheet, 0, &options).unwrap();
    assert!(!svg.contains("New live descendant"));
    assert_eq!(sheet.presentation.slides[0].id, saved_id);
    assert!(sheet.validate().is_ok());
}

#[test]
fn pptx_has_editable_shape_text_curve_annotations_and_only_original_photos() {
    let mut sheet = Sheet::new("Map", "Central");
    let root = sheet.add_child(sheet.root, None, "Source");
    let first = sheet.add_child(root, None, "Editable italic & text");
    let second = sheet.add_child(root, None, "Second");
    let topic = sheet.topic_mut(first).unwrap();
    topic.style.shape = Some(TopicShape::Diamond);
    topic.style.fill = Some(Color::rgba(10, 20, 30, 128));
    topic.style.italic = Some(true);
    topic.style.underline = Some(true);
    topic.style.strikethrough = Some(true);
    topic.labels = vec!["Editable label".into()];
    sheet.add_boundary(&[first, second]).unwrap();
    sheet
        .add_summary(&[first, second], "Editable summary")
        .unwrap();
    let relationship = sheet.add_relationship(first, second).unwrap();
    sheet
        .relationships
        .iter_mut()
        .find(|r| r.id == relationship)
        .unwrap()
        .title = "Editable relationship".into();
    let png = photo(&mut sheet, first, image::ImageFormat::Png);
    let jpg = photo(&mut sheet, second, image::ImageFormat::Jpeg);
    sheet.presentation = generate_presentation(&sheet, &ExportOptions::default()).unwrap();
    let before = sheet.clone();
    let bytes = export_presentation_pptx(&sheet, &ExportOptions::default()).unwrap();
    assert_eq!(sheet, before);
    let slide = xml(&bytes, "ppt/slides/slide1.xml");
    for expected in [
        "<p:sp>",
        "<a:custGeom>",
        "<a:cubicBezTo>",
        "<a:t>Editable italic &amp; text</a:t>",
        "<a:t>Editable label</a:t>",
        "<a:t>Editable summary</a:t>",
        "<a:t>Editable relationship</a:t>",
        "prst=\"diamond\"",
        "i=\"1\"",
        "u=\"sng\"",
        "strike=\"sngStrike\"",
    ] {
        assert!(slide.contains(expected), "missing {expected}");
    }
    assert_eq!(slide.matches("<p:pic>").count(), 2);
    // Native PowerPoint repairs custGeom on p:cxnSp despite SDK schema
    // acceptance. Curves must be editable freeform p:sp objects instead.
    assert!(!slide.contains("<p:cxnSp>"));
    assert!(!slide.contains("<a:stCxn"));
    assert!(!slide.contains("<a:endCxn"));
    let mut reader = quick_xml::Reader::from_str(&slide);
    let mut in_shape = false;
    let mut native_paths = 0;
    loop {
        match reader.read_event().unwrap() {
            quick_xml::events::Event::Start(node) if node.name().as_ref() == "p:sp" => {
                in_shape = true
            }
            quick_xml::events::Event::End(node) if node.name().as_ref() == "p:sp" => {
                in_shape = false
            }
            quick_xml::events::Event::Start(node) if node.name().as_ref() == "a:custGeom" => {
                assert!(
                    in_shape,
                    "custom paths must belong to native freeform shapes"
                );
                native_paths += 1;
            }
            quick_xml::events::Event::Eof => break,
            _ => {}
        }
    }
    assert!(
        native_paths >= 4,
        "branch/relationship/summary paths are retained"
    );
    let mut archive = ZipArchive::new(Cursor::new(&bytes)).unwrap();
    let mut embedded = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        if entry.name().unwrap().starts_with("ppt/media/") {
            let mut data = Vec::new();
            entry.read_to_end(&mut data).unwrap();
            embedded.push(data);
        }
    }
    assert_eq!(embedded.len(), 2);
    assert!(embedded.contains(&png));
    assert!(embedded.contains(&jpg));
    let types = xml(&bytes, "[Content_Types].xml");
    assert!(types.contains("image/jpeg"));
}

#[test]
fn ratio_fit_and_export_guards_remain() {
    let mut sheet = Sheet::new("Map", "Central");
    let branch = sheet.add_child(sheet.root, None, "Branch");
    let frame = Rect::from_min_size(Vec2::ZERO, Vec2::new(200.0, 150.0));
    sheet
        .add_presentation_slide("Slide", PresentationSource::Branch(branch), frame)
        .unwrap();
    sheet.presentation.slides[0].auto_frame = false;
    let fitted = fit_presentation_frame(frame, PresentationAspectRatio::Widescreen16x9).unwrap();
    assert_eq!(fitted.center(), frame.center());
    assert!(fitted.size.x >= frame.size.x && fitted.size.y >= frame.size.y);
    assert!((fitted.size.x / fitted.size.y - 16.0 / 9.0).abs() < 0.001);
    assert!(fit_presentation_frame(Rect::default(), PresentationAspectRatio::Standard4x3).is_err());
    let options = ExportOptions {
        scale: f32::NAN,
        ..Default::default()
    };
    assert!(export_presentation_pptx(&sheet, &options).is_err());
    sheet.presentation.slides[0].frame.size = Vec2::new(1_000_000.0, 1_000_000.0);
    assert!(matches!(
        export_presentation_pptx(&sheet, &Default::default()),
        Err(IoError::PresentationLimit(_))
    ));
}

#[test]
fn multipage_pdf_has_vector_forms_not_slide_images_and_only_actual_photos_are_raster() {
    let mut sheet = Sheet::new("Map", "Central");
    let first = sheet.add_child(sheet.root, None, "First vector slide");
    sheet.add_child(first, None, "Editable map content becomes vector outlines");
    let second = sheet.add_child(sheet.root, None, "Second vector slide");
    sheet.presentation = generate_presentation(&sheet, &Default::default()).unwrap();
    let before = sheet.clone();
    let pdf = export_presentation_pdf(&sheet, &Default::default()).unwrap();
    assert_eq!(sheet, before);
    let source = String::from_utf8_lossy(&pdf);
    assert!(source.starts_with("%PDF-"));
    assert!(source.contains("/Count 2"));
    assert_eq!(source.matches("/Type /Page\n").count(), 2);
    assert!(source.matches("/Subtype /Form").count() >= 2);
    assert!(!source.contains("/Subtype /Image"));
    assert!(!source.contains("/DCTDecode"));
    photo(&mut sheet, second, image::ImageFormat::Jpeg);
    let pdf = export_presentation_pdf(&sheet, &Default::default()).unwrap();
    let source = String::from_utf8_lossy(&pdf);
    assert_eq!(source.matches("/Type /Page\n").count(), 2);
    assert_eq!(source.matches("/Subtype /Image").count(), 1);
    assert!(source.contains("/Width 8"));
    assert!(source.contains("/Height 4"));
}
