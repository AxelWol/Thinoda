//! Visual frame clipping follows the shared scene, not an object's intersection
//! with an arbitrary frame. Clipping is visual and is not secure redaction.

use std::io::{Cursor, Read, Write};

use image::{DynamicImage, ImageFormat, RgbaImage};
use thinoda_core::{
    CanvasConnector, CanvasObject, Card, CardKind, Color, Endpoint, InkStroke, InkTool,
    PresentationSource, Rect, ShapeKind, Sheet, SheetRootMode, Vec2, Workbook, board::Band,
    resources::Resource,
};
use thinoda_io::{
    ExportLoss, ExportOptions, Format, export_bytes, export_losses, export_presentation_pdf,
    export_presentation_png_zip, export_presentation_pptx,
    print::{PrintSettings, render_print},
    render_presentation_slide_png, render_presentation_slide_svg,
};
use zip::ZipArchive;

const RED: [u8; 3] = [231, 25, 35];
const BLUE: [u8; 3] = [25, 65, 231];
const PURPLE: [u8; 3] = [201, 25, 181];
const CYAN: [u8; 3] = [15, 195, 205];
const GREEN: [u8; 3] = [20, 215, 45];

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect::from_min_size(Vec2::new(x, y), Vec2::new(width, height))
}

fn shape(r: Rect, color: [u8; 3], band: Band) -> CanvasObject {
    let mut object = CanvasObject::shape(ShapeKind::Rectangle, r);
    object.band = band;
    object.style.fill = Some(Color::rgb(color[0], color[1], color[2]));
    object
}

fn view_box(svg: &str) -> [f32; 4] {
    let value = svg
        .split_once("viewBox=\"")
        .unwrap()
        .1
        .split('"')
        .next()
        .unwrap();
    let values: Vec<f32> = value
        .split_whitespace()
        .map(|part| part.parse().unwrap())
        .collect();
    [values[0], values[1], values[2], values[3]]
}

fn pixel(image: &RgbaImage, view: [f32; 4], x: f32, y: f32) -> [u8; 3] {
    let ix = ((x - view[0]) / view[2] * image.width() as f32).floor() as u32;
    let iy = ((y - view[1]) / view[3] * image.height() as f32).floor() as u32;
    image.get_pixel(ix, iy).0[..3].try_into().unwrap()
}

fn near(actual: [u8; 3], expected: [u8; 3], tolerance: u8) -> bool {
    actual
        .iter()
        .zip(expected)
        .all(|(a, b)| a.abs_diff(b) <= tolerance)
}

fn entry(bytes: &[u8], name: &str) -> Vec<u8> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut out = Vec::new();
    archive
        .by_name(name)
        .unwrap()
        .read_to_end(&mut out)
        .unwrap();
    out
}

fn objects_board() -> (Workbook, Vec<thinoda_core::CanvasObjectId>) {
    let mut sheet = Sheet::new("Objects", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let zone = sheet
        .create_board_frame("Frame", rect(0.0, 0.0, 100.0, 200.0))
        .unwrap();
    sheet.zones.iter_mut().find(|z| z.id == zone).unwrap().clip = true;

    let below = sheet
        .add_canvas_object(shape(rect(70.0, 20.0, 50.0, 20.0), RED, Band::Below))
        .unwrap();
    let above = sheet
        .add_canvas_object(shape(rect(70.0, 55.0, 50.0, 20.0), BLUE, Band::Above))
        .unwrap();
    let ink = sheet
        .add_canvas_object(CanvasObject::ink(InkStroke::new(
            InkTool::Pen,
            Color::rgb(PURPLE[0], PURPLE[1], PURPLE[2]),
            10.0,
            vec![Vec2::new(70.0, 100.0), Vec2::new(120.0, 100.0)],
        )))
        .unwrap();
    let mut connector = CanvasConnector::new(
        Endpoint::Point(Vec2::new(70.0, 140.0)),
        Endpoint::Point(Vec2::new(120.0, 140.0)),
    );
    connector.color = Some(Color::rgb(CYAN[0], CYAN[1], CYAN[2]));
    connector.width = 8.0;
    let connector = sheet
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    // This overlaps the frame but its center is outside. The scene does not
    // assign it a clip, and export must not lose its unrelated outside part.
    let unrelated = sheet
        .add_canvas_object(shape(rect(90.0, 160.0, 60.0, 20.0), GREEN, Band::Above))
        .unwrap();
    let workbook = Workbook::new(sheet);
    workbook.validate().unwrap();
    (workbook, vec![below, above, ink, connector, unrelated])
}

#[test]
fn svg_png_and_jpeg_clip_each_owned_object_without_clipping_unrelated_objects() {
    let (book, objects) = objects_board();
    let options = ExportOptions::default();
    let svg = String::from_utf8(export_bytes(&book, Format::Svg, &options).unwrap()).unwrap();
    let view = view_box(&svg);
    for id in &objects[..4] {
        assert!(svg.contains(&format!("scene-clip-object-{}", id.0)));
    }
    assert!(!svg.contains(&format!("scene-clip-object-{}", objects[4].0)));

    for (format, tolerance) in [(Format::Png, 0), (Format::Jpeg, 35)] {
        let bytes = export_bytes(&book, format, &options).unwrap();
        let image = image::load_from_memory(&bytes).unwrap().to_rgba8();
        for (y, color) in [(30.0, RED), (65.0, BLUE), (100.0, PURPLE), (140.0, CYAN)] {
            let inside = pixel(&image, view, 80.0, y);
            let outside = pixel(&image, view, 110.0, y);
            assert!(
                near(inside, color, tolerance),
                "{format:?} at {y}: {inside:?}"
            );
            assert!(
                !near(outside, color, tolerance),
                "{format:?} painted a clipped part at {y}: {outside:?}"
            );
        }
        assert!(
            near(pixel(&image, view, 130.0, 170.0), GREEN, tolerance),
            "{format:?} dropped an unrelated object"
        );
    }
}

#[test]
fn attached_connectors_use_resolved_scene_bounds_not_their_stored_rectangle() {
    let mut sheet = Sheet::new("Connectors", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let frame = sheet
        .create_board_frame("Frame", rect(900.0, 900.0, 200.0, 200.0))
        .unwrap();
    sheet.zones.iter_mut().find(|z| z.id == frame).unwrap().clip = true;
    let inside = sheet
        .add_canvas_object(shape(rect(940.0, 940.0, 20.0, 20.0), GREEN, Band::Above))
        .unwrap();
    let outside = sheet
        .add_canvas_object(shape(rect(1140.0, 940.0, 20.0, 20.0), GREEN, Band::Above))
        .unwrap();
    let mut connector = CanvasConnector::new(Endpoint::Object(inside), Endpoint::Object(outside));
    connector.width = 8.0;
    connector.color = Some(Color::rgb(CYAN[0], CYAN[1], CYAN[2]));
    let id = sheet
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    let stored = sheet
        .canvas
        .objects
        .iter()
        .find(|o| o.id == id)
        .unwrap()
        .rect;
    assert!(!rect(900.0, 900.0, 200.0, 200.0).contains(stored.center()));
    let options = ExportOptions::default();
    let book = Workbook::new(sheet.clone());
    let svg = String::from_utf8(export_bytes(&book, Format::Svg, &options).unwrap()).unwrap();
    assert!(svg.contains(&format!("scene-clip-object-{}", id.0)));
    let image = image::load_from_memory(&export_bytes(&book, Format::Png, &options).unwrap())
        .unwrap()
        .to_rgba8();
    let view = view_box(&svg);
    assert_eq!(pixel(&image, view, 1080.0, 950.0), CYAN);
    assert_ne!(pixel(&image, view, 1120.0, 950.0), CYAN);
    assert_eq!(pixel(&image, view, 1150.0, 950.0), GREEN);

    sheet
        .move_canvas_objects(&[inside, outside], Vec2::new(500.0, 0.0))
        .unwrap();
    let moved = Workbook::new(sheet);
    let svg = String::from_utf8(export_bytes(&moved, Format::Svg, &options).unwrap()).unwrap();
    assert!(!svg.contains(&format!("scene-clip-object-{}", id.0)));
}

#[test]
fn overlapping_frames_use_the_first_scene_clip_not_a_union() {
    let mut sheet = Sheet::new("Overlapping", "");
    sheet.root_mode = SheetRootMode::Hidden;
    for frame in [rect(0.0, 0.0, 100.0, 100.0), rect(80.0, 0.0, 100.0, 100.0)] {
        let zone = sheet.create_board_frame("Frame", frame).unwrap();
        sheet.zones.iter_mut().find(|z| z.id == zone).unwrap().clip = true;
    }
    let object = sheet
        .add_canvas_object(shape(rect(60.0, 35.0, 80.0, 30.0), RED, Band::Above))
        .unwrap();
    let options = ExportOptions::default();
    let inspect = |sheet: &Sheet| {
        let book = Workbook::new(sheet.clone());
        let svg = String::from_utf8(export_bytes(&book, Format::Svg, &options).unwrap()).unwrap();
        let image = image::load_from_memory(&export_bytes(&book, Format::Png, &options).unwrap())
            .unwrap()
            .to_rgba8();
        assert!(svg.contains(&format!("scene-clip-object-{}", object.0)));
        (image, view_box(&svg))
    };
    let (image, view) = inspect(&sheet);
    assert_eq!(pixel(&image, view, 70.0, 50.0), RED);
    assert_ne!(pixel(&image, view, 120.0, 50.0), RED);
    sheet.zones.swap(0, 1);
    let (image, view) = inspect(&sheet);
    assert_ne!(pixel(&image, view, 70.0, 50.0), RED);
    assert_eq!(pixel(&image, view, 120.0, 50.0), RED);
}

#[test]
fn export_bounds_include_visible_objects_but_not_a_clipped_objects_full_extent() {
    let mut sheet = Sheet::new("Bounds", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let zone = sheet
        .create_board_frame("Frame", rect(0.0, 0.0, 100.0, 100.0))
        .unwrap();
    sheet.zones.iter_mut().find(|z| z.id == zone).unwrap().clip = true;
    sheet
        .create_board_frame("Unrelated frame", rect(600.0, 0.0, 100.0, 100.0))
        .unwrap();
    sheet
        .add_canvas_object(shape(rect(-5000.0, 20.0, 10_100.0, 40.0), RED, Band::Below))
        .unwrap();
    sheet
        .add_canvas_object(shape(rect(300.0, 20.0, 30.0, 30.0), GREEN, Band::Above))
        .unwrap();
    let book = Workbook::new(sheet.clone());
    let options = ExportOptions::default();
    let svg = String::from_utf8(export_bytes(&book, Format::Svg, &options).unwrap()).unwrap();
    let [x, _, width, _] = view_box(&svg);
    assert!(x >= -25.0, "clipped geometry enlarged the frame: {x}");
    assert!(x + width >= 700.0, "unrelated empty frame was cropped");
    assert!(x + width < 800.0, "clipped geometry enlarged the frame");

    sheet.zones[0].clip = false;
    let svg =
        String::from_utf8(export_bytes(&Workbook::new(sheet), Format::Svg, &options).unwrap())
            .unwrap();
    assert!(view_box(&svg)[0] < -4_000.0);
}

fn cards_board() -> (Sheet, thinoda_core::TopicId, thinoda_core::TopicId) {
    let mut sheet = Sheet::new("Cards", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut card = Card::new(CardKind::Sticky, Vec2::new(100.0, 60.0));
    card.color = Some(Color::rgb(RED[0], RED[1], RED[2]));
    card.rotation = 25.0;
    let partial = sheet
        .add_card("Partial", Vec2::new(85.0, 55.0), card)
        .unwrap();
    let mut card = Card::new(CardKind::Sticky, Vec2::new(80.0, 50.0));
    card.color = Some(Color::rgb(BLUE[0], BLUE[1], BLUE[2]));
    let hidden = sheet
        .add_card("Fully clipped", Vec2::new(300.0, 55.0), card)
        .unwrap();
    let zone = sheet
        .create_zone("Frame", vec![partial, hidden], rect(0.0, 0.0, 100.0, 120.0))
        .unwrap();
    sheet.zones.iter_mut().find(|z| z.id == zone).unwrap().clip = true;
    sheet
        .add_canvas_object(shape(rect(150.0, 20.0, 35.0, 35.0), GREEN, Band::Above))
        .unwrap();
    sheet.validate().unwrap();
    (sheet, partial, hidden)
}

#[test]
fn rotated_and_fully_clipped_cards_use_topic_ownership_not_position() {
    let (sheet, partial, hidden) = cards_board();
    let book = Workbook::new(sheet);
    let options = ExportOptions::default();
    let svg = String::from_utf8(export_bytes(&book, Format::Svg, &options).unwrap()).unwrap();
    assert!(svg.contains(&format!("scene-clip-topic-{}", partial.0)));
    assert!(svg.contains(&format!("scene-clip-topic-{}", hidden.0)));
    assert!(svg.contains("rotate(25.00"));
    // A clipPath preserves the source text; it must not be presented as redaction.
    assert!(svg.contains(&format!("topic-{hidden}-line-0")));

    let image = image::load_from_memory(&export_bytes(&book, Format::Png, &options).unwrap())
        .unwrap()
        .to_rgba8();
    let view = view_box(&svg);
    assert_eq!(pixel(&image, view, 70.0, 55.0), RED);
    assert_ne!(pixel(&image, view, 110.0, 55.0), RED);
    assert!(!image.pixels().any(|pixel| pixel.0[..3] == BLUE));
    assert_eq!(pixel(&image, view, 165.0, 35.0), GREEN);
}

#[test]
fn distant_fully_clipped_topic_does_not_crop_unrelated_raster_content() {
    let (mut sheet, _, hidden) = cards_board();
    sheet.topic_mut(hidden).unwrap().position = Some(Vec2::new(-30_000.0, 55.0));
    sheet.zones[0].name = "W".repeat(36);
    let book = Workbook::new(sheet);
    let options = ExportOptions::default();
    let svg = String::from_utf8(export_bytes(&book, Format::Svg, &options).unwrap()).unwrap();
    let view = view_box(&svg);
    assert!(
        view[0] > -1_000.0,
        "clipped topic expanded the view: {view:?}"
    );
    assert!(
        view[2] < 1_000.0,
        "clipped topic expanded the view: {view:?}"
    );
    assert!(
        view[0] + view[2] > 350.0,
        "frame title was cropped: {view:?}"
    );

    let image = image::load_from_memory(&export_bytes(&book, Format::Png, &options).unwrap())
        .unwrap()
        .to_rgba8();
    assert_eq!(pixel(&image, view, 70.0, 55.0), RED);
    assert_eq!(pixel(&image, view, 165.0, 35.0), GREEN);
}

#[test]
fn clipping_bounds_preserve_cross_frame_relationships_and_their_titles() {
    let (mut sheet, partial, hidden) = cards_board();
    sheet.topic_mut(hidden).unwrap().position = Some(Vec2::new(-30_000.0, 55.0));
    let free = sheet.add_floating("Unframed", Vec2::new(550.0, 55.0));
    let link = sheet.add_relationship(partial, free).unwrap();
    sheet.set_relationship_title(link, &"W".repeat(80)).unwrap();
    let mut without_link = sheet.clone();
    without_link.relationships.clear();
    let book = Workbook::new(sheet);
    let options = ExportOptions::default();
    let svg = String::from_utf8(export_bytes(&book, Format::Svg, &options).unwrap()).unwrap();
    assert!(!svg.contains(&format!("scene-clip-relationship-{}", link.0)));
    let view = view_box(&svg);
    assert!(
        view[0] > -1_000.0,
        "clipped topic expanded the view: {view:?}"
    );
    assert!(
        view[0] + view[2] > 900.0,
        "relationship title was cropped: {view:?}"
    );
    assert!(
        view[2] < 2_000.0,
        "clipped topic expanded the view: {view:?}"
    );
    let image = image::load_from_memory(&export_bytes(&book, Format::Png, &options).unwrap())
        .unwrap()
        .to_rgba8();
    assert_eq!(pixel(&image, view, 70.0, 55.0), RED);
    assert_eq!(pixel(&image, view, 165.0, 35.0), GREEN);

    let without_link = Workbook::new(without_link);
    let other_svg =
        String::from_utf8(export_bytes(&without_link, Format::Svg, &options).unwrap()).unwrap();
    let other_view = view_box(&other_svg);
    let other =
        image::load_from_memory(&export_bytes(&without_link, Format::Png, &options).unwrap())
            .unwrap()
            .to_rgba8();
    let changed = (0..110)
        .flat_map(|y| (110..400).map(move |x| (x as f32 + 0.5, y as f32 + 0.5)))
        .filter(|&(x, y)| {
            pixel(&image, view, x, y)
                .iter()
                .zip(pixel(&other, other_view, x, y))
                .any(|(a, b)| a.abs_diff(b) > 30)
        })
        .count();
    assert!(
        changed > 20,
        "cross-frame relationship disappeared outside its frame"
    );
}

#[test]
fn coincident_frames_keep_cross_frame_relationships_and_bounds() {
    let mut sheet = Sheet::new("Coincident frames", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let first = sheet.add_floating("First", Vec2::new(35.0, 70.0));
    let child = sheet.add_child(first, None, "Child");
    let second = sheet.add_floating("Second", Vec2::new(125.0, 70.0));
    let same_zone = sheet.add_relationship(first, child).unwrap();
    let cross_zone = sheet.add_relationship(first, second).unwrap();
    sheet
        .set_relationship_title(cross_zone, &format!("Cross frame {}", "W".repeat(45)))
        .unwrap();
    let frame = rect(0.0, 0.0, 160.0, 140.0);
    sheet
        .create_zone("First frame", vec![first], frame)
        .unwrap();
    sheet
        .create_zone("Second frame", vec![second], frame)
        .unwrap();
    for zone in &mut sheet.zones {
        zone.clip = true;
    }
    let viewport = rect(-30.0, -30.0, 680.0, 200.0);
    sheet
        .add_presentation_slide("Viewport", PresentationSource::Viewport, viewport)
        .unwrap();
    sheet.presentation.slides[0].auto_frame = false;
    sheet.validate().unwrap();

    let options = ExportOptions::default();
    let book = Workbook::new(sheet.clone());
    let svg = String::from_utf8(export_bytes(&book, Format::Svg, &options).unwrap()).unwrap();
    assert!(svg.contains(&format!("scene-clip-relationship-{}", same_zone.0)));
    assert!(
        !svg.contains(&format!("scene-clip-relationship-{}", cross_zone.0)),
        "identical rectangles do not make different Zones the same owner"
    );
    let view = view_box(&svg);
    assert!(
        view[0] + view[2] > 300.0,
        "auto bounds must include the un-clipped relationship title: {view:?}"
    );
    let png = image::load_from_memory(&export_bytes(&book, Format::Png, &options).unwrap())
        .unwrap()
        .to_rgba8();
    let dark_outside = png
        .enumerate_pixels()
        .filter(|(x, y, pixel)| {
            let x = view[0] + (*x as f32 + 0.5) * view[2] / png.width() as f32;
            let y = view[1] + (*y as f32 + 0.5) * view[3] / png.height() as f32;
            x > 170.0 && (20.0..140.0).contains(&y) && pixel.0[..3].iter().all(|c| *c < 180)
        })
        .count();
    assert!(dark_outside > 20, "cross-frame title was cropped in PNG");

    let viewport_svg = render_presentation_slide_svg(&sheet, 0, &options).unwrap();
    assert!(viewport_svg.contains(&format!("scene-clip-relationship-{}", same_zone.0)));
    assert!(!viewport_svg.contains(&format!("scene-clip-relationship-{}", cross_zone.0)));
    let rendered =
        image::load_from_memory(&render_presentation_slide_png(&sheet, 0, &options).unwrap())
            .unwrap()
            .to_rgba8();
    let mut without_cross_zone = sheet.clone();
    without_cross_zone
        .relationships
        .retain(|relationship| relationship.id != cross_zone);
    let control = image::load_from_memory(
        &render_presentation_slide_png(&without_cross_zone, 0, &options).unwrap(),
    )
    .unwrap()
    .to_rgba8();
    assert_eq!(rendered.dimensions(), control.dimensions());
    let changed_outside = rendered
        .enumerate_pixels()
        .filter(|(x, y, pixel)| {
            let world_x =
                viewport.min.x + (*x as f32 + 0.5) * viewport.size.x / rendered.width() as f32;
            world_x > frame.max().x + 5.0
                && pixel.0[..3]
                    .iter()
                    .zip(&control.get_pixel(*x, *y).0[..3])
                    .any(|(actual, previous)| actual.abs_diff(*previous) > 30)
        })
        .count();
    assert!(changed_outside > 20, "viewport cropped a cross-frame link");
}

#[test]
fn clipping_bounds_keep_the_frame_even_if_its_color_is_transparent() {
    let (mut sheet, _, hidden) = cards_board();
    sheet.topic_mut(hidden).unwrap().position = Some(Vec2::new(-30_000.0, 55.0));
    sheet.zones[0].color = Some(Color::rgba(0, 0, 0, 0));
    sheet.canvas.objects.clear();
    let book = Workbook::new(sheet);
    let options = ExportOptions {
        padding: 0.0,
        transparent: true,
        ..Default::default()
    };
    let svg = String::from_utf8(export_bytes(&book, Format::Svg, &options).unwrap()).unwrap();
    let view = view_box(&svg);
    assert!(view[0] <= 0.0 && view[0] + view[2] >= 100.0);
    assert!(view[1] <= 0.0 && view[1] + view[3] >= 120.0);
    assert!(view[2] < 110.0 && view[3] < 130.0, "{view:?}");
    let image = image::load_from_memory(&export_bytes(&book, Format::Png, &options).unwrap())
        .unwrap()
        .to_rgba8();
    assert_eq!(pixel(&image, view, 70.0, 55.0), RED);
}

#[test]
fn clipping_bounds_include_unclipped_object_labels_and_equation_appendix() {
    let (mut sheet, partial, hidden) = cards_board();
    sheet.topic_mut(hidden).unwrap().position = Some(Vec2::new(-30_000.0, 55.0));
    let mut object = shape(rect(250.0, 120.0, 40.0, 30.0), BLUE, Band::Above);
    if let thinoda_core::CanvasObjectKind::Shape { label, .. } = &mut object.kind {
        *label = "Wide label\n".repeat(12);
    }
    let object = sheet.add_canvas_object(object).unwrap();
    let equation = thinoda_core::equations::replace_note("", Some("\\frac{a}{b}")).unwrap();
    sheet.set_markdown_notes(partial, Some(equation)).unwrap();
    let svg = String::from_utf8(
        export_bytes(
            &Workbook::new(sheet),
            Format::Svg,
            &ExportOptions::default(),
        )
        .unwrap(),
    )
    .unwrap();
    let tree = resvg::usvg::Tree::from_str(
        &svg,
        &resvg::usvg::Options {
            fontdb: thinoda_io::fonts::database(),
            ..Default::default()
        },
    )
    .unwrap();
    for id in [
        format!("object-{}", object.0),
        format!("equation-{partial}-title"),
        format!("equation-{partial}-formula"),
    ] {
        let bounds = tree.node_by_id(&id).unwrap().abs_stroke_bounding_box();
        assert!(
            bounds.left() >= 0.0 && bounds.top() >= 0.0,
            "{id}: {bounds:?}"
        );
        assert!(
            bounds.right() <= tree.size().width() && bounds.bottom() <= tree.size().height(),
            "{id}: {bounds:?}"
        );
    }
}

#[test]
fn viewport_and_legacy_source_slides_honor_the_same_topic_clip() {
    let (mut sheet, partial, _) = cards_board();
    let zone = sheet.zones[0].id;
    let frame = rect(-20.0, -20.0, 400.0, 160.0);
    for source in [
        PresentationSource::Viewport,
        PresentationSource::Branch(partial),
        PresentationSource::Zone(zone),
    ] {
        sheet
            .add_presentation_slide("Slide", source, frame)
            .unwrap();
        sheet.presentation.slides.last_mut().unwrap().auto_frame = false;
    }
    let options = ExportOptions::default();
    for index in 0..3 {
        let svg = render_presentation_slide_svg(&sheet, index, &options).unwrap();
        assert!(svg.contains(&format!("scene-clip-topic-{}", partial.0)));
        let png = render_presentation_slide_png(&sheet, index, &options).unwrap();
        let image = image::load_from_memory(&png).unwrap().to_rgba8();
        let view = view_box(&svg);
        assert_eq!(view, [-20.0, -20.0, 400.0, 160.0], "slide {index}");
        assert_eq!(image.dimensions(), (400, 160), "slide {index}");
        assert_eq!(pixel(&image, view, 70.0, 55.0), RED, "slide {index}");
        assert_ne!(pixel(&image, view, 110.0, 55.0), RED, "slide {index}");
    }
    let zip = export_presentation_png_zip(&sheet, &options).unwrap();
    assert_eq!(
        entry(&zip, "slides/slide-001.png"),
        render_presentation_slide_png(&sheet, 0, &options).unwrap()
    );
    assert!(
        export_presentation_pdf(&sheet, &options)
            .unwrap()
            .starts_with(b"%PDF-")
    );
    let pptx = export_presentation_pptx(&sheet, &options).unwrap();
    let viewport = image::load_from_memory(&entry(&pptx, "ppt/media/slide1-viewport.png"))
        .unwrap()
        .to_rgba8();
    let view = [frame.min.x, frame.min.y, frame.size.x, frame.size.y];
    assert_eq!(pixel(&viewport, view, 70.0, 55.0), RED);
    assert_ne!(pixel(&viewport, view, 110.0, 55.0), RED);
}

#[test]
fn filtered_presentation_keeps_the_source_sheets_board_summary_semantics() {
    let mut sheet = Sheet::new("Presentation", "Root");
    let branch = sheet.add_floating("Branch", Vec2::ZERO);
    let first = sheet.add_child(branch, None, "First");
    let second = sheet.add_child(branch, None, "Second");
    let summary = sheet.add_summary(&[first, second], "Summary").unwrap();
    let other = sheet.add_child(sheet.root, None, "Other");
    let connector = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Topic(summary),
            Endpoint::Topic(other),
        )))
        .unwrap();
    let frame = rect(-500.0, -500.0, 1600.0, 1000.0);
    let zone = sheet.create_zone("Zone", vec![branch], frame).unwrap();
    // Keep the connector as the only feature enabling board semantics.
    sheet.zones[0].color = None;
    for source in [
        PresentationSource::Branch(branch),
        PresentationSource::Zone(zone),
    ] {
        sheet
            .add_presentation_slide("Slide", source, frame)
            .unwrap();
        sheet.presentation.slides.last_mut().unwrap().auto_frame = false;
    }
    sheet.validate().unwrap();
    assert!(thinoda_io::uses_board_semantics(&sheet));

    let mut plain = sheet.clone();
    plain.canvas.objects.clear();
    assert!(!thinoda_io::uses_board_semantics(&plain));
    let options = ExportOptions::default();
    for index in 0..2 {
        let plain_svg = render_presentation_slide_svg(&plain, index, &options).unwrap();
        assert!(plain_svg.contains("Summary"), "slide {index}");
        assert!(
            !plain_svg.contains("class=\"summary-connector\""),
            "slide {index}"
        );

        let board_svg = render_presentation_slide_svg(&sheet, index, &options).unwrap();
        assert!(board_svg.contains("Summary"), "slide {index}");
        assert!(
            !board_svg.contains(&format!("id=\"object-{}\"", connector.0)),
            "the connector with a hidden endpoint must remain filtered on slide {index}"
        );
        assert!(
            board_svg.contains("class=\"summary-connector\""),
            "filtering the only canvas object must not erase the source board semantics on slide {index}"
        );
    }
}

#[test]
fn ordinary_and_presentation_exports_include_clipped_decoration_owners() {
    let mut sheet = Sheet::new("Decorations", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let root = sheet.add_floating("Root", Vec2::new(70.0, 70.0));
    let first = sheet.add_child(root, None, "First");
    let second = sheet.add_child(root, None, "Second");
    let summary = sheet.add_summary(&[first, second], "Summary").unwrap();
    let nested = sheet.add_child(summary, None, "Summary child");
    let summary_id = sheet.summaries[0].id;
    let boundary = sheet.add_boundary(&[first, second]).unwrap();
    let same = sheet.add_relationship(first, second).unwrap();
    let other = sheet.add_floating("Other", Vec2::new(400.0, 70.0));
    let cross = sheet.add_relationship(first, other).unwrap();
    let free = sheet.add_floating("Unframed", Vec2::new(700.0, 70.0));
    let mixed = sheet.add_relationship(first, free).unwrap();
    sheet
        .set_relationship_title(cross, "Cross-frame link")
        .unwrap();
    sheet
        .set_relationship_title(mixed, "Frame-to-free link")
        .unwrap();
    let first_zone = sheet
        .create_zone("First", vec![root], rect(0.0, 0.0, 180.0, 160.0))
        .unwrap();
    let second_zone = sheet
        .create_zone("Second", vec![other], rect(300.0, 0.0, 180.0, 160.0))
        .unwrap();
    for zone in &mut sheet.zones {
        zone.clip = true;
    }
    let frame = rect(-40.0, -40.0, 900.0, 360.0);
    for source in [
        PresentationSource::Viewport,
        PresentationSource::Branch(root),
        PresentationSource::Zone(first_zone),
    ] {
        sheet
            .add_presentation_slide("Slide", source, frame)
            .unwrap();
        sheet.presentation.slides.last_mut().unwrap().auto_frame = false;
    }
    sheet.validate().unwrap();
    assert_ne!(first_zone, second_zone);
    let options = ExportOptions::default();
    let check = |svg: &str| {
        for (kind, id) in [
            ("map-edge", first.0),
            ("map-edge", second.0),
            ("map-edge", nested.0),
            ("boundary", boundary.0),
            ("summary", summary_id.0),
            ("relationship", same.0),
            ("topic", summary.0),
            ("topic", nested.0),
        ] {
            assert!(
                svg.contains(&format!("scene-clip-{kind}-{id}")),
                "{kind} {id}"
            );
        }
        let summary_group = format!(
            "<g clip-path=\"url(#scene-clip-summary-{})\">",
            summary_id.0
        );
        let summary_content = svg
            .split_once(&summary_group)
            .unwrap()
            .1
            .split_once("</g>")
            .unwrap()
            .0;
        assert!(summary_content.contains("class=\"summary-connector\""));
        for id in [cross, mixed] {
            assert!(
                !svg.contains(&format!("scene-clip-relationship-{}", id.0)),
                "cross-frame relationship {} must remain un-clipped",
                id.0
            );
        }
    };
    let svg = String::from_utf8(
        export_bytes(&Workbook::new(sheet.clone()), Format::Svg, &options).unwrap(),
    )
    .unwrap();
    check(&svg);
    assert!(svg.contains("Cross-frame link"));
    assert!(svg.contains("Frame-to-free link"));
    for index in 0..3 {
        let svg = render_presentation_slide_svg(&sheet, index, &options).unwrap();
        assert_eq!(view_box(&svg), [-40.0, -40.0, 900.0, 360.0]);
        check(&svg);
        if index == 0 {
            assert!(svg.contains("Cross-frame link"));
            assert!(svg.contains("Frame-to-free link"));
        }
        let image = image::load_from_memory(
            &render_presentation_slide_png(&sheet, index, &options).unwrap(),
        )
        .unwrap();
        assert_eq!((image.width(), image.height()), (900, 360));
    }
}

#[test]
fn print_pixels_use_the_same_scene_clip_as_svg() {
    let (book, _) = objects_board();
    let settings = PrintSettings::default();
    let document = render_print(&book, 0, settings).unwrap();
    let options = ExportOptions {
        padding: 12.0,
        transparent: true,
        ..ExportOptions::default()
    };
    let svg = String::from_utf8(export_bytes(&book, Format::Svg, &options).unwrap()).unwrap();
    let view = view_box(&svg);
    let [x, y, width, _] = settings.placement([view[2], view[3]]).unwrap();
    let pixels_per_mm = settings.dpi as f32 / 25.4;
    let scale = width * pixels_per_mm / view[2];
    let point = |sx: f32, sy: f32| {
        let px = (x * pixels_per_mm + (sx - view[0]) * scale).floor() as u32;
        let py = (y * pixels_per_mm + (sy - view[1]) * scale).floor() as u32;
        document.image.get_pixel(px, py).0
    };
    assert_eq!(point(80.0, 30.0), RED);
    assert_ne!(point(110.0, 30.0), RED);
    assert_eq!(point(130.0, 170.0), GREEN);
    assert!(document.pdf().unwrap().starts_with(b"%PDF-"));
}

#[test]
fn visual_clipping_losses_are_reported_only_for_editable_powerpoint() {
    let (mut book, _) = objects_board();
    let zone = book.sheets[0]
        .create_board_frame("Second", rect(300.0, 0.0, 100.0, 100.0))
        .unwrap();
    book.sheets[0]
        .zones
        .iter_mut()
        .find(|z| z.id == zone)
        .unwrap()
        .clip = true;
    book.sheets.push(Sheet::new("Other", "Root"));
    let options = ExportOptions::default();
    for format in [Format::Svg, Format::Png, Format::Jpeg, Format::Pdf] {
        assert_eq!(export_losses(&book, format, &options), vec![], "{format:?}");
    }
    assert_eq!(
        export_losses(&book, Format::PowerPoint, &options),
        vec![ExportLoss::ZoneClipDropped(2)]
    );
    assert_eq!(
        export_losses(&book, Format::Csv, &options),
        vec![
            ExportLoss::ShapesDropped(3),
            ExportLoss::InkDropped(1),
            ExportLoss::ConnectorsDropped(1),
            ExportLoss::ZonesDropped(2),
            ExportLoss::SynthesizedRoot {
                sheet: 0,
                title: "Objects".into()
            }
        ]
    );
    let other = ExportOptions {
        sheet: Some(1),
        ..options
    };
    for format in [
        Format::Svg,
        Format::Png,
        Format::Jpeg,
        Format::Pdf,
        Format::PowerPoint,
    ] {
        assert_eq!(export_losses(&book, format, &other), vec![], "{format:?}");
    }
}

#[test]
fn powerpoint_loss_distinguishes_flattened_viewports_from_editable_slides() {
    let (mut sheet, partial, _) = cards_board();
    let frame = rect(-20.0, -20.0, 400.0, 160.0);
    sheet
        .add_presentation_slide("Viewport", PresentationSource::Viewport, frame)
        .unwrap();
    let options = ExportOptions::default();
    let viewport_only = Workbook::new(sheet.clone());
    assert_eq!(
        export_losses(&viewport_only, Format::PowerPoint, &options),
        vec![]
    );
    let pptx = export_presentation_pptx(&sheet, &options).unwrap();
    let image = image::load_from_memory(&entry(&pptx, "ppt/media/slide1-viewport.png"))
        .unwrap()
        .to_rgba8();
    let view = [frame.min.x, frame.min.y, frame.size.x, frame.size.y];
    assert_eq!(pixel(&image, view, 70.0, 55.0), RED);
    assert_ne!(pixel(&image, view, 110.0, 55.0), RED);

    sheet
        .add_presentation_slide("Another viewport", PresentationSource::Viewport, frame)
        .unwrap();
    assert_eq!(
        export_losses(&Workbook::new(sheet.clone()), Format::PowerPoint, &options),
        vec![]
    );
    for source in [
        PresentationSource::Branch(partial),
        PresentationSource::Zone(sheet.zones[0].id),
    ] {
        let mut mixed = sheet.clone();
        mixed
            .add_presentation_slide("Editable", source, frame)
            .unwrap();
        assert_eq!(
            export_losses(&Workbook::new(mixed.clone()), Format::PowerPoint, &options),
            vec![ExportLoss::ZoneClipDropped(1)]
        );
        mixed
            .presentation
            .slides
            .retain(|slide| slide.source != PresentationSource::Viewport);
        assert_eq!(
            export_losses(&Workbook::new(mixed), Format::PowerPoint, &options),
            vec![ExportLoss::ZoneClipDropped(1)]
        );
    }
}

#[test]
fn embedded_image_cards_are_visually_clipped_but_remain_in_svg_source() {
    let mut sheet = Sheet::new("Image", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut card = Card::new(CardKind::Image, Vec2::new(120.0, 100.0));
    card.rotation = 15.0;
    let topic = sheet
        .add_card("Image", Vec2::new(90.0, 60.0), card)
        .unwrap();
    let source = image::RgbImage::from_pixel(8, 8, image::Rgb(PURPLE));
    let mut encoded = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(source)
        .write_to(&mut encoded, ImageFormat::Png)
        .unwrap();
    let resource = Resource::new("Image", "image/png", encoded.into_inner()).unwrap();
    thinoda_core::images::set_image(&mut sheet, topic, Some((resource, Vec2::new(64.0, 64.0))))
        .unwrap();
    let zone = sheet
        .create_zone("Frame", vec![topic], rect(0.0, 0.0, 100.0, 120.0))
        .unwrap();
    sheet.zones.iter_mut().find(|z| z.id == zone).unwrap().clip = true;
    let book = Workbook::new(sheet);
    let options = ExportOptions::default();
    let svg = String::from_utf8(export_bytes(&book, Format::Svg, &options).unwrap()).unwrap();
    assert!(svg.contains("data:image/png;base64,"));
    let image = image::load_from_memory(&export_bytes(&book, Format::Png, &options).unwrap())
        .unwrap()
        .to_rgba8();
    let view = view_box(&svg);
    let mut inside = 0;
    let mut outside = 0;
    for (x, _, pixel) in image.enumerate_pixels() {
        if pixel.0[..3] == PURPLE {
            let sheet_x = view[0] + (x as f32 + 0.5) * view[2] / image.width() as f32;
            if sheet_x < 100.0 {
                inside += 1;
            } else {
                outside += 1;
            }
        }
    }
    assert!(inside > 0, "the image's visible part was dropped");
    assert_eq!(outside, 0, "pixels escaped the frame");
}

/// Optional end-to-end PDF renderer check; no PDF rasterizer is required by CI.
#[test]
#[ignore = "requires Python 3, pypdfium2 and Pillow"]
fn pdf_and_print_pages_rasterize_with_visible_frame_clipping() {
    use std::process::{Command, Stdio};

    fn check(
        pdf: &[u8],
        page: usize,
        view: [f32; 4],
        samples: &[([f32; 2], [u8; 3], bool)],
        tolerance: u8,
    ) {
        let parameters = serde_json::json!({
            "page": page,
            "view": view,
            "samples": samples,
            "tolerance": tolerance
        });
        let script = r#"
import json, sys
import pypdfium2 as pdfium
params = json.loads(sys.argv[1])
doc = pdfium.PdfDocument(sys.stdin.buffer.read())
image = doc[params["page"]].render(scale=3).to_pil().convert("RGB")
x0, y0, w, h = params["view"]
for (x, y), expected, should_match in params["samples"]:
    px = int((x - x0) / w * image.width)
    py = int((y - y0) / h * image.height)
    actual = image.getpixel((px, py))
    match = all(abs(a - b) <= params["tolerance"] for a, b in zip(actual, expected))
    if match != should_match:
        raise AssertionError(f"page {params['page']} at {(x, y)}: {actual} vs {expected}")
"#;
        let mut child = Command::new("python")
            .args(["-c", script, &parameters.to_string()])
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
    }

    let (book, _) = objects_board();
    let options = ExportOptions::default();
    let svg = String::from_utf8(export_bytes(&book, Format::Svg, &options).unwrap()).unwrap();
    let pdf = export_bytes(&book, Format::Pdf, &options).unwrap();
    check(
        &pdf,
        0,
        view_box(&svg),
        &[
            ([80.0, 30.0], RED, true),
            ([110.0, 30.0], RED, false),
            ([80.0, 100.0], PURPLE, true),
            ([110.0, 100.0], PURPLE, false),
            ([80.0, 140.0], CYAN, true),
            ([110.0, 140.0], CYAN, false),
            ([130.0, 170.0], GREEN, true),
        ],
        15,
    );

    let (mut sheet, partial, _) = cards_board();
    let zone = sheet.zones[0].id;
    let frame = rect(-20.0, -20.0, 400.0, 160.0);
    for source in [
        PresentationSource::Viewport,
        PresentationSource::Branch(partial),
        PresentationSource::Zone(zone),
    ] {
        sheet
            .add_presentation_slide("Slide", source, frame)
            .unwrap();
        sheet.presentation.slides.last_mut().unwrap().auto_frame = false;
    }
    let pdf = export_presentation_pdf(&sheet, &options).unwrap();
    for page in 0..3 {
        check(
            &pdf,
            page,
            [-20.0, -20.0, 400.0, 160.0],
            &[
                ([70.0, 55.0], RED, true),
                ([110.0, 55.0], RED, false),
                ([300.0, 55.0], BLUE, false),
            ],
            15,
        );
    }

    let settings = PrintSettings::default();
    let print = render_print(&book, 0, settings).unwrap();
    let print_options = ExportOptions {
        padding: 12.0,
        transparent: true,
        ..ExportOptions::default()
    };
    let svg = String::from_utf8(export_bytes(&book, Format::Svg, &print_options).unwrap()).unwrap();
    let view = view_box(&svg);
    let [x, y, width, height] = settings.placement([view[2], view[3]]).unwrap();
    let point = |sx: f32, sy: f32| {
        [
            x + (sx - view[0]) / view[2] * width,
            y + (sy - view[1]) / view[3] * height,
        ]
    };
    let [page_width, page_height] = settings.size_mm();
    check(
        &print.pdf().unwrap(),
        0,
        [0.0, 0.0, page_width, page_height],
        &[
            (point(80.0, 30.0), RED, true),
            (point(110.0, 30.0), RED, false),
            (point(130.0, 170.0), GREEN, true),
        ],
        50,
    );
}
