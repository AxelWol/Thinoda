//! Board frame-title contrast across SVG, raster and editable presentations.

use std::io::{Cursor, Read};

use thinoda_core::{
    CanvasConnector, CanvasObject, Color, Endpoint, PresentationSource, Rect, Sheet, SheetRootMode,
    Vec2, Workbook, theme::Theme,
};
use thinoda_io::{
    ExportOptions, Format, export_bytes, export_presentation_pptx, render_presentation_slide_svg,
};
use zip::ZipArchive;

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect::from_min_size(Vec2::new(x, y), Vec2::new(width, height))
}

fn entry(bytes: &[u8], name: &str) -> String {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut out = String::new();
    archive
        .by_name(name)
        .unwrap()
        .read_to_string(&mut out)
        .unwrap();
    out
}

fn svg_title_color(svg: &str, title: &str) -> Color {
    let end = svg.find(&format!(">{title}</text>")).unwrap();
    let start = svg[..end].rfind("<text ").unwrap();
    let attrs = &svg[start..end];
    let value = attrs
        .split_once("fill=\"")
        .unwrap()
        .1
        .split('"')
        .next()
        .unwrap();
    Color::from_hex(value).unwrap()
}

fn pptx_title_color(xml: &str, title: &str) -> Color {
    let end = xml.find(&format!("<a:t>{title}</a:t>")).unwrap();
    let run = xml[..end].rsplit_once("<a:rPr").unwrap().1;
    assert!(run.contains("<a:alpha val=\"100000\"/>"));
    let value = run
        .split_once("<a:srgbClr val=\"")
        .unwrap()
        .1
        .split('"')
        .next()
        .unwrap();
    Color::from_hex(value).unwrap()
}

fn frame_sheet(tint: Option<Color>) -> Sheet {
    let mut sheet = Sheet::new("Frames", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let root = sheet.add_floating("Root", Vec2::new(120.0, 100.0));
    let frame = rect(0.0, 0.0, 320.0, 200.0);
    let zone = sheet
        .create_zone("Contrast frame", vec![root], frame)
        .unwrap();
    sheet
        .zones
        .iter_mut()
        .find(|frame| frame.id == zone)
        .unwrap()
        .color = tint;
    for source in [PresentationSource::Zone(zone), PresentationSource::Viewport] {
        sheet
            .add_presentation_slide("Slide", source, frame)
            .unwrap();
        sheet.presentation.slides.last_mut().unwrap().auto_frame = false;
    }
    sheet.validate().unwrap();
    sheet
}

#[test]
fn board_frame_titles_contrast_against_the_exported_fill_in_svg_and_pptx() {
    let cases = [
        (false, None, None, Color::BLACK),
        (true, None, None, Color::WHITE),
        (false, Some(Color::rgb(190, 235, 200)), None, Color::BLACK),
        (
            true,
            Some(Color::rgba(240, 150, 20, 127)),
            None,
            Color::WHITE,
        ),
        (false, Some(Color::rgba(20, 180, 90, 0)), None, Color::BLACK),
        (
            false,
            Some(Color::WHITE),
            Some(Color::rgb(100, 100, 100)),
            Color::WHITE,
        ),
        (
            true,
            Some(Color::rgb(180, 100, 80)),
            Some(Color::rgba(180, 170, 160, 100)),
            Color::WHITE,
        ),
    ];
    for (dark, tint, background, expected) in cases {
        let mut sheet = frame_sheet(tint);
        sheet.theme.background = background;
        let options = ExportOptions {
            dark,
            ..Default::default()
        };
        let tint = tint.unwrap_or(Color::rgb(0x5D, 0x7A, 0x91));
        // Export keeps the existing 20/255 fill alpha, independently of the
        // frame tint's own alpha.
        let fill = Color::rgba(tint.r, tint.g, tint.b, 20);
        let page = Theme::for_settings(&sheet.theme).background(&sheet.theme, dark);
        let painted_background = fill.over(page);

        let svg = String::from_utf8(
            export_bytes(&Workbook::new(sheet.clone()), Format::Svg, &options).unwrap(),
        )
        .unwrap();
        let title = svg_title_color(&svg, "Contrast frame");
        assert_eq!(title, expected, "dark={dark}, tint={tint:?}, bg={page:?}");
        assert!(
            title.contrast_on(painted_background) >= 4.5,
            "dark={dark}, tint={tint:?}, bg={page:?}"
        );
        assert!(svg.contains(&format!(
            "fill=\"{}\" stroke=\"{}\"",
            fill.to_hex(),
            tint.to_hex()
        )));

        for index in 0..2 {
            let slide_svg = render_presentation_slide_svg(&sheet, index, &options).unwrap();
            assert_eq!(svg_title_color(&slide_svg, "Contrast frame"), title);
        }
        let pptx = export_presentation_pptx(&sheet, &options).unwrap();
        let xml = entry(&pptx, "ppt/slides/slide1.xml");
        assert_eq!(pptx_title_color(&xml, "Contrast frame"), title);
        let frame_shape = xml
            .split_once("name=\"Contrast frame\"")
            .unwrap()
            .1
            .split_once("</p:sp>")
            .unwrap()
            .0;
        assert!(frame_shape.contains(&format!(
            "<a:srgbClr val=\"{:02X}{:02X}{:02X}\"><a:alpha val=\"7843\"/>",
            tint.r, tint.g, tint.b
        )));
        assert!(frame_shape.contains(&format!(
            "<a:srgbClr val=\"{:02X}{:02X}{:02X}\"><a:alpha val=\"{}\"/>",
            tint.r,
            tint.g,
            tint.b,
            u32::from(tint.a) * 100_000 / 255
        )));
    }
}

#[test]
fn png_rasterizes_readable_titles_for_clipped_frames() {
    for dark in [false, true] {
        let mut sheet = frame_sheet(Some(Color::rgb(190, 235, 200)));
        sheet.zones[0].clip = true;
        let options = ExportOptions {
            dark,
            ..Default::default()
        };
        let book = Workbook::new(sheet.clone());
        let svg = String::from_utf8(export_bytes(&book, Format::Svg, &options).unwrap()).unwrap();
        let view_box = svg
            .split_once("viewBox=\"")
            .unwrap()
            .1
            .split('"')
            .next()
            .unwrap()
            .split_whitespace()
            .map(|part| part.parse::<f32>().unwrap())
            .collect::<Vec<_>>();
        assert!(svg.contains("scene-clip-topic-"));
        let png = export_bytes(&book, Format::Png, &options).unwrap();
        let image = image::load_from_memory(&png).unwrap().to_rgba8();
        let page = Theme::for_settings(&sheet.theme).background(&sheet.theme, dark);
        let background = Color::rgba(190, 235, 200, 20).over(page);
        let mut readable_pixel = false;
        // The title starts at (8, 17) with 12px text; stay away from the
        // frame outline and all topics to check actual rasterized glyphs.
        for y in 4..17 {
            for x in 8..110 {
                let ix = ((x as f32 - view_box[0]) / view_box[2] * image.width() as f32) as u32;
                let iy = ((y as f32 - view_box[1]) / view_box[3] * image.height() as f32) as u32;
                let [r, g, b, _] = image.get_pixel(ix, iy).0;
                if Color::rgb(r, g, b).contrast_on(background) >= 4.5 {
                    readable_pixel = true;
                }
            }
        }
        assert!(
            readable_pixel,
            "no high-contrast frame text in {dark:?} PNG"
        );
    }
}

#[test]
fn transparent_svg_uses_theme_based_title_color_without_adding_a_background() {
    let sheet = frame_sheet(Some(Color::rgb(190, 235, 200)));
    let book = Workbook::new(sheet);
    let opaque = ExportOptions::default();
    let transparent = ExportOptions {
        transparent: true,
        ..opaque.clone()
    };
    let opaque_svg = String::from_utf8(export_bytes(&book, Format::Svg, &opaque).unwrap()).unwrap();
    let transparent_svg =
        String::from_utf8(export_bytes(&book, Format::Svg, &transparent).unwrap()).unwrap();
    assert_eq!(
        svg_title_color(&opaque_svg, "Contrast frame"),
        svg_title_color(&transparent_svg, "Contrast frame")
    );
    // On a transparent export the downstream backdrop is unknowable. The
    // light-theme choice can be unreadable if later placed over black.
    let title = svg_title_color(&transparent_svg, "Contrast frame");
    let fill_over_black = Color::rgba(190, 235, 200, 20).over(Color::BLACK);
    assert!(title.contrast_on(fill_over_black) < 4.5);
    let png = export_bytes(&book, Format::Png, &transparent).unwrap();
    let image = image::load_from_memory(&png).unwrap().to_rgba8();
    assert_eq!(image.get_pixel(0, 0).0[3], 0);
}

#[test]
fn filtered_editable_slides_use_original_board_semantics_for_frame_titles() {
    let mut sheet = Sheet::new("Presentation", "Root");
    let branch = sheet.add_floating("Branch", Vec2::ZERO);
    let other = sheet.add_child(sheet.root, None, "Other");
    let connector = sheet
        .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
            Endpoint::Topic(branch),
            Endpoint::Topic(other),
        )))
        .unwrap();
    let frame = rect(-300.0, -200.0, 800.0, 600.0);
    let zone = sheet
        .create_zone("Legacy frame", vec![branch], frame)
        .unwrap();
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
    assert!(sheet.uses_board_semantics());

    let mut plain = sheet.clone();
    plain.canvas.objects.clear();
    assert!(!plain.uses_board_semantics());
    let options = ExportOptions::default();
    let board_pptx = export_presentation_pptx(&sheet, &options).unwrap();
    let plain_pptx = export_presentation_pptx(&plain, &options).unwrap();
    for index in 0..2 {
        let board_svg = render_presentation_slide_svg(&sheet, index, &options).unwrap();
        let plain_svg = render_presentation_slide_svg(&plain, index, &options).unwrap();
        assert!(!board_svg.contains(&format!("id=\"object-{}\"", connector.0)));
        assert_eq!(svg_title_color(&board_svg, "Legacy frame"), Color::BLACK);
        assert_eq!(
            svg_title_color(&plain_svg, "Legacy frame"),
            Color::rgb(0x5D, 0x7A, 0x91)
        );
        let path = format!("ppt/slides/slide{}.xml", index + 1);
        assert_eq!(
            pptx_title_color(&entry(&board_pptx, &path), "Legacy frame"),
            Color::BLACK
        );
        assert_eq!(
            pptx_title_color(&entry(&plain_pptx, &path), "Legacy frame"),
            Color::rgb(0x5D, 0x7A, 0x91)
        );
    }
}
