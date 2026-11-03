use base64::Engine;
use image::{ExtendedColorType, ImageEncoder};
use std::io::{Cursor, Read};
use thinoda_core::{Vec2, Workbook, resources::Resource};
use thinoda_io::{ExportLoss, ExportOptions, Format, export_bytes, export_losses, import_bytes};
use zip::ZipArchive;

fn sample(format: image::ImageFormat) -> Workbook {
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
        40,
        20,
        image::Rgb([250, 10, 20]),
    ))
    .write_to(&mut bytes, format)
    .unwrap();
    let media = if format == image::ImageFormat::Png {
        "image/png"
    } else {
        "image/jpeg"
    };
    let resource = Resource::new("local.image", media, bytes.into_inner()).unwrap();
    let mut book = Workbook::default();
    let sheet = book.active_mut();
    thinoda_core::images::set_image(sheet, sheet.root, Some((resource, Vec2::new(160.0, 80.0))))
        .unwrap();
    sheet.topic_mut(sheet.root).unwrap().image_below = true;
    book
}

fn oriented_photo(format: image::ImageFormat, orientation: u8) -> Vec<u8> {
    let pixels = image::RgbImage::from_fn(80, 40, |x, y| {
        image::Rgb(match (x < 40, y < 20) {
            (true, true) => [240, 20, 30],
            (false, true) => [20, 230, 40],
            (true, false) => [30, 40, 240],
            (false, false) => [235, 220, 20],
        })
    });
    let exif = vec![
        b'I',
        b'I',
        42,
        0,
        8,
        0,
        0,
        0,
        1,
        0,
        0x12,
        0x01,
        3,
        0,
        1,
        0,
        0,
        0,
        orientation,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    ];
    let mut bytes = Vec::new();
    match format {
        image::ImageFormat::Png => {
            let mut encoder = image::codecs::png::PngEncoder::new(&mut bytes);
            encoder.set_exif_metadata(exif).unwrap();
            encoder
                .write_image(pixels.as_raw(), 80, 40, ExtendedColorType::Rgb8)
                .unwrap();
        }
        image::ImageFormat::Jpeg => {
            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 100);
            encoder.set_exif_metadata(exif).unwrap();
            encoder
                .encode(pixels.as_raw(), 80, 40, ExtendedColorType::Rgb8)
                .unwrap();
        }
        _ => panic!("unsupported fixture"),
    }
    bytes
}

fn photo_book(bytes: Vec<u8>, format: image::ImageFormat) -> Workbook {
    let media = if format == image::ImageFormat::Png {
        "image/png"
    } else {
        "image/jpeg"
    };
    let resource = Resource::new("camera.image", media, bytes).unwrap();
    let mut book = Workbook::default();
    let sheet = book.active_mut();
    thinoda_core::images::set_image(sheet, sheet.root, Some((resource, Vec2::new(160.0, 80.0))))
        .unwrap();
    sheet.topic_mut(sheet.root).unwrap().image_below = true;
    book
}

fn svg_embedded_image(svg: &str) -> (&str, Vec<u8>) {
    let data = svg
        .split("xlink:href=\"data:")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let (media, encoded) = data.split_once(";base64,").unwrap();
    (
        media,
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap(),
    )
}

fn assert_rendered_corners(png: &[u8], corners: [usize; 4]) {
    let image = image::load_from_memory(png).unwrap().into_rgb8();
    let colors = [
        [240_i32, 20, 30],
        [20, 230, 40],
        [30, 40, 240],
        [235, 220, 20],
    ];
    let mut sums = [(0_u64, 0_u64, 0_u64); 4];
    for (x, y, pixel) in image.enumerate_pixels() {
        for (index, color) in colors.iter().enumerate() {
            if pixel
                .0
                .iter()
                .zip(color)
                .all(|(actual, expected)| (i32::from(*actual) - expected).abs() <= 30)
            {
                sums[index].0 += u64::from(x);
                sums[index].1 += u64::from(y);
                sums[index].2 += 1;
            }
        }
    }
    let center = |index: usize| {
        let (x, y, count) = sums[index];
        assert!(count > 20, "missing photo color {index}: {sums:?}");
        (x as f64 / count as f64, y as f64 / count as f64)
    };
    let [top_left, top_right, bottom_left, bottom_right] = corners.map(center);
    assert!(top_left.0 + 5.0 < top_right.0);
    assert!(bottom_left.0 + 5.0 < bottom_right.0);
    assert!(top_left.1 + 5.0 < bottom_left.1);
    assert!(top_right.1 + 5.0 < bottom_right.1);
}

#[test]
fn camera_orientation_matches_raster_svg_and_editable_powerpoint() {
    let expected_corners = [
        [0, 1, 2, 3],
        [1, 0, 3, 2],
        [3, 2, 1, 0],
        [2, 3, 0, 1],
        [0, 2, 1, 3],
        [2, 0, 3, 1],
        [3, 1, 2, 0],
        [1, 3, 0, 2],
    ];
    for format in [image::ImageFormat::Png, image::ImageFormat::Jpeg] {
        for (index, corners) in expected_corners.into_iter().enumerate() {
            let orientation = index as u8 + 1;
            let original = oriented_photo(format, orientation);
            let decoded = thinoda_core::images::decode(&original).unwrap();
            let mut book = photo_book(original.clone(), format);
            let options = ExportOptions::default();

            for visual_format in [
                Format::Svg,
                Format::Png,
                Format::Jpeg,
                Format::Pdf,
                Format::PowerPoint,
            ] {
                let losses = export_losses(&book, visual_format, &options);
                assert_eq!(
                    losses,
                    if orientation == 1 {
                        vec![]
                    } else {
                        vec![ExportLoss::OrientedImageMetadataDropped(1)]
                    },
                    "{format:?} orientation {orientation} for {visual_format:?}"
                );
            }
            assert!(export_losses(&book, Format::Thinoda, &options).is_empty());
            assert!(export_losses(&book, Format::XMind, &options).is_empty());
            let xmind = export_bytes(&book, Format::XMind, &options).unwrap();
            let xmind = import_bytes(Format::XMind, &xmind).unwrap();
            let xmind_reference = xmind
                .active()
                .topic(xmind.active().root)
                .unwrap()
                .image
                .as_ref()
                .unwrap();
            assert_eq!(
                xmind
                    .active()
                    .resources
                    .get(xmind_reference)
                    .unwrap()
                    .bytes(),
                original
            );

            let svg =
                String::from_utf8(export_bytes(&book, Format::Svg, &options).unwrap()).unwrap();
            assert!(
                svg.contains(&format!(
                    "width=\"{}\" height=\"{}\" xlink:href=\"data:",
                    decoded.width, decoded.height
                )),
                "{format:?} orientation {orientation} has incorrect visual dimensions"
            );
            let (media, embedded) = svg_embedded_image(&svg);
            if orientation == 1 {
                assert_eq!(
                    media,
                    if format == image::ImageFormat::Png {
                        "image/png"
                    } else {
                        "image/jpeg"
                    }
                );
                assert_eq!(embedded, original);
            } else {
                assert_eq!(media, "image/png");
                let visual = image::load_from_memory(&embedded).unwrap().into_rgba8();
                assert_eq!(
                    (visual.width(), visual.height()),
                    (decoded.width, decoded.height)
                );
                assert_eq!(visual.as_raw(), &decoded.rgba);
            }
            let png = export_bytes(&book, Format::Png, &options).unwrap();
            assert_rendered_corners(&png, corners);
            assert!(
                export_bytes(&book, Format::Pdf, &options)
                    .unwrap()
                    .starts_with(b"%PDF-")
            );

            let sheet = book.active_mut();
            let image_topic = sheet.topic(sheet.root).unwrap().clone();
            let branch = sheet.add_child(sheet.root, None, "Camera");
            let duplicate = sheet.add_child(branch, None, "Copy");
            for id in [branch, duplicate] {
                let topic = sheet.topic_mut(id).unwrap();
                topic.image = image_topic.image.clone();
                topic.image_size = image_topic.image_size;
            }
            sheet
                .add_presentation_slide(
                    "Camera",
                    thinoda_core::PresentationSource::Branch(branch),
                    thinoda_core::Rect::from_min_size(
                        Vec2::new(-200.0, -150.0),
                        Vec2::new(400.0, 300.0),
                    ),
                )
                .unwrap();
            if orientation != 1 {
                assert_eq!(
                    export_losses(&book, Format::PowerPoint, &options),
                    vec![ExportLoss::OrientedImageMetadataDropped(1)],
                    "shared references should be reported once"
                );
            }
            let pptx = export_bytes(&book, Format::PowerPoint, &options).unwrap();
            let mut archive = ZipArchive::new(Cursor::new(pptx)).unwrap();
            let names: Vec<_> = (0..archive.len())
                .map(|index| archive.by_index(index).unwrap().name().unwrap().to_string())
                .filter(|name| name.starts_with("ppt/media/"))
                .collect();
            assert_eq!(names.len(), 1, "shared image must use one media entry");
            let mut power_point_image = Vec::new();
            archive
                .by_name(&names[0])
                .unwrap()
                .read_to_end(&mut power_point_image)
                .unwrap();
            assert_eq!(
                power_point_image, embedded,
                "{format:?} orientation {orientation}"
            );
            let mut slide = String::new();
            archive
                .by_name("ppt/slides/slide1.xml")
                .unwrap()
                .read_to_string(&mut slide)
                .unwrap();
            assert_eq!(slide.matches("<p:pic>").count(), 2);
            assert_eq!(slide.matches("r:embed=\"rId2\"").count(), 2);
            for picture in slide.split("<p:pic>").skip(1) {
                let picture = picture.split("</p:pic>").next().unwrap();
                let ext = picture.split("<a:ext ").nth(1).unwrap();
                let width: f64 = ext
                    .split("cx=\"")
                    .nth(1)
                    .unwrap()
                    .split('"')
                    .next()
                    .unwrap()
                    .parse()
                    .unwrap();
                let height: f64 = ext
                    .split("cy=\"")
                    .nth(1)
                    .unwrap()
                    .split('"')
                    .next()
                    .unwrap()
                    .parse()
                    .unwrap();
                let aspect = f64::from(decoded.width) / f64::from(decoded.height);
                assert!(
                    (width / height - aspect).abs() < 0.01,
                    "{format:?} orientation {orientation} has distorted PowerPoint dimensions"
                );
            }

            let saved = export_bytes(&book, Format::Thinoda, &options).unwrap();
            let reopened = import_bytes(Format::Thinoda, &saved).unwrap();
            let reference = image_topic.image.unwrap();
            assert_eq!(
                reopened.active().resources.get(&reference).unwrap().bytes(),
                original
            );
            assert_eq!(reopened, book);
            let clipboard = thinoda_core::clipboard::encode_sheet(book.active()).unwrap();
            assert_eq!(
                thinoda_core::clipboard::decode_sheet(&clipboard).unwrap(),
                *book.active()
            );
        }
    }
}

#[test]
fn original_image_and_geometry_survive_native_json_and_clipboard() {
    let book = sample(image::ImageFormat::Png);
    let bytes = export_bytes(&book, Format::Thinoda, &Default::default()).unwrap();
    assert_eq!(import_bytes(Format::Thinoda, &bytes).unwrap(), book);
    let recovered: Workbook = serde_json::from_slice(&serde_json::to_vec(&book).unwrap()).unwrap();
    assert_eq!(recovered, book);
    let copied = thinoda_core::clipboard::encode_sheet(book.active()).unwrap();
    assert_eq!(
        thinoda_core::clipboard::decode_sheet(&copied).unwrap(),
        *book.active()
    );
}

#[test]
fn local_images_render_in_visual_exports_print_and_pitch() {
    for source in [image::ImageFormat::Png, image::ImageFormat::Jpeg] {
        for dark in [false, true] {
            let mut book = sample(source);
            let options = ExportOptions {
                dark,
                ..Default::default()
            };
            let svg = export_bytes(&book, Format::Svg, &options).unwrap();
            let svg = String::from_utf8(svg).unwrap();
            assert!(svg.contains("<image "));
            assert!(svg.contains("base64,"));
            assert!(!svg.contains("file:"));
            let png = export_bytes(&book, Format::Png, &options).unwrap();
            let png = image::load_from_memory(&png).unwrap().into_rgb8();
            assert!(
                png.pixels()
                    .filter(|p| p[0] > 220 && p[1] < 40 && p[2] < 50)
                    .count()
                    > 1000
            );
            let pdf = export_bytes(&book, Format::Pdf, &options).unwrap();
            assert!(pdf.starts_with(b"%PDF-"));
            assert!(String::from_utf8_lossy(&pdf).contains("/Subtype /Image"));
            assert!(
                !export_bytes(&book, Format::Jpeg, &options)
                    .unwrap()
                    .is_empty()
            );
            let settings = thinoda_io::print::PrintSettings {
                dark,
                ..Default::default()
            };
            let print = thinoda_io::print::render_print(&book, 0, settings).unwrap();
            assert!(
                print
                    .image
                    .pixels()
                    .any(|p| p[0] > 220 && p[1] < 40 && p[2] < 50)
            );
            let sheet = book.active_mut();
            let branch = sheet.add_child(sheet.root, None, "Image slide");
            let root_image = sheet.topic(sheet.root).unwrap().clone();
            let topic = sheet.topic_mut(branch).unwrap();
            topic.image = root_image.image;
            topic.image_size = root_image.image_size;
            sheet
                .add_presentation_slide(
                    "Image",
                    thinoda_core::PresentationSource::Branch(branch),
                    thinoda_core::Rect::from_min_size(
                        Vec2::new(-200.0, -150.0),
                        Vec2::new(400.0, 300.0),
                    ),
                )
                .unwrap();
            let svg = thinoda_io::presentation::render_presentation_slide_svg(sheet, 0, &options)
                .unwrap();
            assert!(svg.contains("<image "));
            let png = thinoda_io::presentation::render_presentation_slide_png(sheet, 0, &options)
                .unwrap();
            let png = image::load_from_memory(&png).unwrap().into_rgb8();
            assert!(
                png.pixels()
                    .filter(|p| p[0] > 220 && p[1] < 40 && p[2] < 50)
                    .count()
                    > 1000
            );
            assert!(thinoda_io::presentation::export_presentation_png_zip(sheet, &options).is_ok());
            assert!(thinoda_io::presentation::export_presentation_pdf(sheet, &options).is_ok());
            assert!(
                !export_bytes(&book, Format::PowerPoint, &options)
                    .unwrap()
                    .is_empty()
            );
        }
    }
}

#[test]
fn unsupported_resources_and_editable_exports_remain_explicit() {
    let mut book = sample(image::ImageFormat::Png);
    for format in Format::EXPORTABLE {
        if !matches!(
            format,
            Format::Thinoda
                | Format::XMind
                | Format::Svg
                | Format::Png
                | Format::Jpeg
                | Format::Pdf
                | Format::PowerPoint
        ) {
            assert!(
                export_bytes(&book, *format, &Default::default()).is_err(),
                "{format:?}"
            );
        }
    }
    let bad = Resource::new("document.bin", "application/octet-stream", vec![1, 2, 3]).unwrap();
    book.active_mut().resources.insert(bad).unwrap();
    for format in [Format::Svg, Format::Png, Format::Pdf, Format::PowerPoint] {
        assert!(export_bytes(&book, format, &Default::default()).is_err());
    }
    assert!(export_bytes(&book, Format::XMind, &Default::default()).is_err());
    assert!(export_bytes(&book, Format::Thinoda, &Default::default()).is_ok());
}

#[test]
fn shared_images_are_embedded_only_once_in_svg() {
    let mut book = sample(image::ImageFormat::Png);
    let sheet = book.active_mut();
    for _ in 0..20 {
        sheet.duplicate(sheet.root).unwrap();
    }
    let svg =
        String::from_utf8(export_bytes(&book, Format::Svg, &Default::default()).unwrap()).unwrap();
    assert_eq!(svg.matches("base64,").count(), 1);
    assert_eq!(svg.matches("<use xlink:href=\"#image-").count(), 21);
}
