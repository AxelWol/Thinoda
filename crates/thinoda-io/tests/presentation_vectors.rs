use std::io::{Cursor, Read};

use thinoda_core::{
    Color, PresentationScope, PresentationSource, Rect, Sheet, TopicShape, Vec2,
    resources::Resource,
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
