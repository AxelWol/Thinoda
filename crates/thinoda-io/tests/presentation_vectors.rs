use std::io::{Cursor, Read};

use thinoda_core::{
    ApproxMeasure, CanvasConnector, CanvasObject, Card, CardKind, Color, Endpoint,
    PresentationScope, PresentationSource, Rect, ShapeKind, Sheet, SheetRootMode, Structure,
    TopicShape, Vec2, board::CanvasObjectKind, connector_path, layout_sheet, resources::Resource,
};
use thinoda_io::{
    ExportOptions, Format, IoError, PresentationAspectRatio, export_presentation_pdf,
    export_presentation_png_zip, export_presentation_pptx, fit_presentation_frame,
    generate_presentation, import_bytes_with_report, render_presentation_slide_svg,
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
fn editable_image_card_backgrounds_preserve_transparency_and_native_appearance() {
    for dark in [false, true] {
        for shape in [TopicShape::Ellipse, TopicShape::Underline] {
            let mut native_background = None;
            for color in [
                None,
                Some(Color::rgb(10, 20, 30)),
                Some(Color::rgba(10, 20, 30, 128)),
                Some(Color::rgba(10, 20, 30, 0)),
            ] {
                let mut sheet = Sheet::new("Image", "");
                sheet.root_mode = SheetRootMode::Hidden;
                let mut card = Card::new(CardKind::Image, Vec2::new(180.0, 130.0));
                card.color = color;
                card.rotation = 30.0;
                let id = sheet.add_card("Caption", Vec2::ZERO, card).unwrap();
                let topic = sheet.topic_mut(id).unwrap();
                topic.style.shape = Some(shape);
                topic.style.border_color = Some(Color::rgb(251, 10, 20));
                topic.style.border_width = Some(4.0);
                photo(&mut sheet, id, image::ImageFormat::Png);
                sheet
                    .add_presentation_slide(
                        "Image",
                        PresentationSource::Branch(id),
                        Rect::from_min_size(Vec2::new(-200.0, -200.0), Vec2::new(400.0, 400.0)),
                    )
                    .unwrap();
                let options = ExportOptions {
                    dark,
                    ..Default::default()
                };
                let output = export_presentation_pptx(&sheet, &options).unwrap();
                let slide = xml(&output, "ppt/slides/slide1.xml");
                let background = element(&slide, "sp", &format!("name=\"Topic {id}\""));
                let (interior, line) = background.split_once("<a:ln ").unwrap();
                assert!(background.contains("rot=\"1800000\""));
                assert!(element(&slide, "pic", "name=\"Topic photo\"").contains("rot=\"1800000\""));
                if color.is_some_and(|color| color.a == 0) {
                    assert!(background.contains("prst=\"rect\""), "{background}");
                    assert!(
                        interior.contains("<a:srgbClr val=\"0A141E\"><a:alpha val=\"0\"/>"),
                        "{interior}"
                    );
                    assert!(line.contains("<a:alpha val=\"0\"/>"), "{line}");
                    assert!(!slide.contains("name=\"Topic underline\""));
                    let page = thinoda_core::theme::Theme::for_settings(&sheet.theme)
                        .background(&sheet.theme, dark);
                    let text_color = Color::highest_contrast_on(page);
                    let text = element(&slide, "sp", "<a:t>Caption</a:t>");
                    assert!(
                        text.contains(&format!(
                            "val=\"{:02X}{:02X}{:02X}\"",
                            text_color.r, text_color.g, text_color.b
                        )),
                        "{text}"
                    );
                    let mut explicit = sheet.clone();
                    explicit.topic_mut(id).unwrap().style.text_color =
                        Some(Color::rgb(40, 70, 100));
                    let explicit_output = export_presentation_pptx(&explicit, &options).unwrap();
                    let explicit_slide = xml(&explicit_output, "ppt/slides/slide1.xml");
                    assert!(
                        element(&explicit_slide, "sp", "<a:t>Caption</a:t>")
                            .contains("val=\"284664\"")
                    );
                } else {
                    if let Some(expected) = &native_background {
                        assert_eq!(background, expected);
                    } else {
                        native_background = Some(background.to_owned());
                    }
                    assert!(
                        background.contains(if shape == TopicShape::Ellipse {
                            "prst=\"ellipse\""
                        } else {
                            "prst=\"rect\""
                        }),
                        "{background}"
                    );
                    if shape == TopicShape::Ellipse {
                        assert!(line.contains("val=\"FB0A14\""), "{line}");
                    } else {
                        assert!(slide.contains("name=\"Topic underline\""));
                    }
                }
            }
        }
    }
}

#[test]
fn drawio_tutorial_images_stay_borderless_in_editable_slides() {
    let mut wb = import_bytes_with_report(
        Format::DrawIo,
        include_bytes!("../../../docs/draw-io-examples/board-visual-tutorial.drawio"),
    )
    .unwrap()
    .workbook;
    let sheet = wb.active_mut();
    let images: Vec<_> = sheet
        .content_topics()
        .filter(|topic| {
            topic.card.is_some_and(|card| {
                card.kind == CardKind::Image && card.color.is_some_and(|color| color.a == 0)
            })
        })
        .map(|topic| topic.id)
        .collect();
    assert_eq!(images.len(), 3);
    for id in &images {
        sheet
            .add_presentation_slide(
                "Image",
                PresentationSource::Branch(*id),
                Rect::from_min_size(Vec2::new(-2000.0, -2000.0), Vec2::new(5000.0, 5000.0)),
            )
            .unwrap();
    }

    for dark in [false, true] {
        let options = ExportOptions {
            dark,
            ..Default::default()
        };
        let output = export_presentation_pptx(sheet, &options).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(&output)).unwrap();
        for (index, id) in images.iter().enumerate() {
            let slide = xml(&output, &format!("ppt/slides/slide{}.xml", index + 1));
            let background = element(&slide, "sp", &format!("name=\"Topic {id}\""));
            let (interior, line) = background.split_once("<a:ln ").unwrap();
            assert!(
                interior.contains("<a:srgbClr val=\"000000\"><a:alpha val=\"0\"/>"),
                "{interior}"
            );
            assert!(line.contains("<a:alpha val=\"0\"/>"), "{line}");
            assert!(background.contains("prst=\"rect\""), "{background}");
            element(&slide, "pic", "name=\"Topic photo\"");
            let mut media = Vec::new();
            archive
                .by_name(&format!("ppt/media/slide{}-photo1.png", index + 1))
                .unwrap()
                .read_to_end(&mut media)
                .unwrap();
            image::load_from_memory(&media).unwrap();
        }
    }
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
fn svg_and_editable_powerpoint_use_rotated_card_connector_anchors() {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut card = Card::new(CardKind::Sticky, Vec2::new(180.0, 100.0));
    card.rotation = 90.0;
    let center = Vec2::new(100.0, 100.0);
    let root = sheet.add_card("Card", center, card).unwrap();
    sheet.topic_mut(root).unwrap().structure = Some(Structure::LogicRight);
    let child = sheet.add_child(root, None, "Child");
    let mut other_card = Card::new(CardKind::Text, Vec2::new(140.0, 80.0));
    other_card.rotation = 45.0;
    let other = sheet
        .add_card("Other card", Vec2::new(500.0, 300.0), other_card)
        .unwrap();
    let relationship = sheet.add_relationship(root, other).unwrap();
    sheet
        .set_relationship_control_points(
            relationship,
            Some([Vec2::new(100.0, 60.0), Vec2::new(-100.0, -30.0)]),
        )
        .unwrap();
    let mut connector = CanvasConnector::new(
        Endpoint::Topic(root),
        Endpoint::Point(Vec2::new(100.0, 400.0)),
    );
    connector.arrow_end = false;
    let free_path = connector_path(
        &sheet,
        &layout_sheet(&sheet, &ApproxMeasure, false),
        &connector,
    );
    let connector_id = sheet
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    let frame = Rect::from_min_size(Vec2::new(-200.0, -200.0), Vec2::new(1000.0, 800.0));
    let zone = sheet
        .create_zone("Frame", vec![root, other], frame)
        .unwrap();
    sheet
        .add_presentation_slide("Editable", PresentationSource::Zone(zone), frame)
        .unwrap();
    sheet.presentation.slides[0].scope = PresentationScope::Context;
    sheet.presentation.slides[0].auto_frame = false;
    sheet.validate().unwrap();
    let before = sheet.clone();

    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let branch = layout
        .connectors
        .iter()
        .find(|edge| edge.parent == root && edge.child == child)
        .unwrap();
    let curve = layout
        .relationships
        .iter()
        .find(|curve| curve.id == relationship)
        .unwrap();
    for ratio in [
        PresentationAspectRatio::Widescreen16x9,
        PresentationAspectRatio::Standard4x3,
    ] {
        let options = ExportOptions {
            presentation_aspect_ratio: ratio,
            ..ExportOptions::default()
        };
        let svg = render_presentation_slide_svg(&sheet, 0, &options).unwrap();
        let move_to = |point: Vec2| format!("M {:.2},{:.2}", point.x, point.y);
        assert!(svg.contains(&move_to(branch.start)));
        assert!(svg.contains(&format!(
            "{} C {:.2},{:.2} {:.2},{:.2} {:.2},{:.2}",
            move_to(curve.start),
            curve.c1.x,
            curve.c1.y,
            curve.c2.x,
            curve.c2.y,
            curve.end.x,
            curve.end.y
        )));
        let object = svg
            .split_once(&format!("id=\"object-{}\"", connector_id.0))
            .unwrap()
            .1
            .split_once("</g>")
            .unwrap()
            .0;
        assert!(object.contains(&move_to(free_path[0])));

        let output = export_presentation_pptx(&sheet, &options).unwrap();
        let slide = xml(&output, "ppt/slides/slide1.xml");
        let card_shape = transform(element(&slide, "sp", &format!("name=\"Topic {root}\"")));
        let scale = card_shape.width / f64::from(card.size.x);
        let pivot = card_shape.center();
        let sheet_point = |point: (f64, f64)| {
            (
                f64::from(center.x) + (point.0 - pivot.0) / scale,
                f64::from(center.y) + (point.1 - pivot.1) / scale,
            )
        };
        let assert_point = |actual: (f64, f64), expected: Vec2| {
            let actual = sheet_point(actual);
            assert!((actual.0 - f64::from(expected.x)).abs() < 0.02);
            assert!((actual.1 - f64::from(expected.y)).abs() < 0.02);
        };
        let branch_points = points(element(&slide, "sp", "name=\"Branch connector\""));
        assert_point(branch_points[0], branch.start);
        let relationship_points = points(element(&slide, "sp", "name=\"Relationship\""));
        assert_eq!(relationship_points.len(), 4);
        for (actual, expected) in
            relationship_points
                .into_iter()
                .zip([curve.start, curve.c1, curve.c2, curve.end])
        {
            assert_point(actual, expected);
        }
        let free_points = points(element(
            &slide,
            "sp",
            &format!("name=\"Canvas object {connector_id}\""),
        ));
        assert_eq!(free_points.len(), free_path.len());
        for (actual, expected) in free_points.into_iter().zip(&free_path) {
            assert_point(actual, *expected);
        }
        assert_eq!(sheet, before);
    }
}

#[test]
fn connector_colors_and_opacity_match_in_svg_and_editable_powerpoint() {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let root = sheet
        .add_card(
            "Context",
            Vec2::new(0.0, -100.0),
            Card::new(CardKind::Sticky, Vec2::new(120.0, 60.0)),
        )
        .unwrap();
    let mut objects = Vec::new();
    for (index, label, explicit, expected, alpha) in [
        (0, "Inherited", None, "14283C", 100_u8),
        (
            1,
            "Explicit",
            Some(Color::rgba(90, 120, 150, 180)),
            "5A7896",
            90_u8,
        ),
    ] {
        let y = index as f32 * 100.0;
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::new(-100.0, y)),
            Endpoint::Point(Vec2::new(100.0, y)),
        );
        connector.label = label.into();
        connector.arrow_start = true;
        connector.arrow_end = true;
        connector.width = 8.0;
        connector.color = explicit;
        let mut object = CanvasObject::connector(connector);
        object.style.stroke = Some(Color::rgba(20, 40, 60, 200));
        object.style.opacity = 0.5;
        let id = sheet.add_canvas_object(object).unwrap();
        objects.push((id, label, expected, alpha));
    }
    let frame = Rect::from_min_size(Vec2::new(-250.0, -250.0), Vec2::new(700.0, 600.0));
    let zone = sheet.create_zone("Frame", vec![root], frame).unwrap();
    sheet
        .add_presentation_slide("Editable", PresentationSource::Zone(zone), frame)
        .unwrap();
    sheet.presentation.slides[0].scope = PresentationScope::Context;
    sheet.presentation.slides[0].auto_frame = false;
    sheet.validate().unwrap();
    let before = sheet.clone();

    let options = ExportOptions::default();
    let svg = render_presentation_slide_svg(&sheet, 0, &options).unwrap();
    let pptx = export_presentation_pptx(&sheet, &options).unwrap();
    let slide = xml(&pptx, "ppt/slides/slide1.xml");
    for (id, label, rgb, alpha) in objects {
        let svg_object = svg
            .split_once(&format!("id=\"object-{}\"", id.0))
            .unwrap()
            .1
            .split_once("</g>")
            .unwrap()
            .0;
        let color = format!("#{rgb}{alpha:02X}");
        assert!(svg_object.contains(&format!("stroke=\"{color}\"")));
        assert!(svg_object.contains("stroke-width=\"8.00\""));
        assert_eq!(svg_object.matches(&format!("fill=\"{color}\"")).count(), 3);
        assert!(svg_object.contains(&format!("fill=\"{color}\">{label}</text>")));

        let name = format!("name=\"Canvas object {id}\"");
        let shapes: Vec<_> = slide
            .split("<p:sp>")
            .skip(1)
            .filter_map(|part| part.split_once("</p:sp>").map(|(shape, _)| shape))
            .filter(|shape| shape.contains(&name))
            .collect();
        assert_eq!(shapes.len(), 3, "{name}");
        let fill = format!(
            "<a:srgbClr val=\"{rgb}\"><a:alpha val=\"{}\"",
            u32::from(alpha) * 100_000 / 255
        );
        assert!(shapes.iter().all(|shape| shape.contains(&fill)), "{name}");
        let text = element(&slide, "sp", &format!("<a:t>{label}</a:t>"));
        assert!(text.contains(&fill), "{label}");
    }
    assert_eq!(sheet, before);
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
