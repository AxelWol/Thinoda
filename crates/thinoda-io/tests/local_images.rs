use std::io::Cursor;
use thinoda_core::{Vec2, Workbook, resources::Resource};
use thinoda_io::{ExportOptions, Format, export_bytes, import_bytes};

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
        }
    }
}

#[test]
fn unsupported_resources_and_editable_exports_remain_explicit() {
    let mut book = sample(image::ImageFormat::Png);
    for format in Format::EXPORTABLE {
        if !matches!(
            format,
            Format::Thinoda | Format::Svg | Format::Png | Format::Jpeg | Format::Pdf
        ) {
            assert!(
                export_bytes(&book, *format, &Default::default()).is_err(),
                "{format:?}"
            );
        }
    }
    let bad = Resource::new("document.bin", "application/octet-stream", vec![1, 2, 3]).unwrap();
    book.active_mut().resources.insert(bad).unwrap();
    for format in [Format::Svg, Format::Png, Format::Pdf] {
        assert!(export_bytes(&book, format, &Default::default()).is_err());
    }
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
