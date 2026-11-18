use std::io::{Cursor, Read};

use thinoda_core::{
    CanvasObject, CanvasObjectKind, Card, CardKind, Color, ObjectStyle, PresentationSource, Rect,
    ShapeKind, Sheet, SheetRootMode, Vec2, Workbook,
};
use thinoda_io::{
    ExportOptions, Format, export_bytes_with_consent, export_presentation_pptx, import_bytes,
};
use zip::ZipArchive;

fn text_element<'a>(svg: &'a str, label: &str) -> &'a str {
    let end = svg.find(&format!(">{label}</text>")).unwrap();
    let start = svg[..end].rfind("<text ").unwrap();
    &svg[start..end]
}

fn slide_xml(bytes: &[u8]) -> String {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    archive
        .by_name("ppt/slides/slide1.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    xml
}

#[test]
fn styled_board_text_round_trips_and_renders_in_svg_and_powerpoint() {
    let mut sheet = Sheet::new("Board", "Anchor");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut sticky = Card::new(CardKind::Sticky, Vec2::new(160.0, 100.0));
    sticky.color = Some(Color::rgb(20, 40, 60));
    let sticky_id = sheet.add_card("Note", Vec2::new(0.0, 0.0), sticky).unwrap();
    sheet.topic_mut(sticky_id).unwrap().style.text_color = Some(Color::rgb(0xA5, 0x5A, 0x11));

    let mut text = Card::new(CardKind::Text, Vec2::new(160.0, 100.0));
    text.color = Some(Color::rgb(220, 230, 240));
    let text_id = sheet.add_card("Box", Vec2::new(220.0, 0.0), text).unwrap();
    sheet.topic_mut(text_id).unwrap().style.text_color = Some(Color::rgb(0x22, 0x77, 0x33));

    let mut shape = CanvasObject::shape(
        ShapeKind::Cloud,
        Rect::from_min_size(Vec2::new(0.0, 150.0), Vec2::new(160.0, 100.0)),
    );
    if let CanvasObjectKind::Shape { label, .. } = &mut shape.kind {
        *label = "Cloud".into();
    }
    shape.style.fill = Some(Color::rgb(80, 90, 100));
    shape.style.text_color = Some(Color::rgb(0x0A, 0x6E, 0xD2));
    shape.style.opacity = 0.5;
    sheet.add_canvas_object(shape).unwrap();
    sheet
        .add_presentation_slide(
            "Note",
            PresentationSource::Branch(sticky_id),
            Rect::from_min_size(Vec2::new(-100.0, -100.0), Vec2::new(400.0, 300.0)),
        )
        .unwrap();
    let book = Workbook::new(sheet.clone());
    book.validate().unwrap();
    for format in [Format::Thinoda, Format::XMind] {
        let native =
            export_bytes_with_consent(&book, format, &ExportOptions::default(), true).unwrap();
        assert_eq!(import_bytes(format, &native).unwrap(), book);
    }

    for dark in [false, true] {
        let options = ExportOptions {
            dark,
            ..Default::default()
        };
        let svg = String::from_utf8(
            export_bytes_with_consent(&book, Format::Svg, &options, true).unwrap(),
        )
        .unwrap();
        assert!(text_element(&svg, "Note").contains("fill=\"#A55A11\""));
        assert!(text_element(&svg, "Box").contains("fill=\"#227733\""));
        assert!(text_element(&svg, "Cloud").contains("fill=\"#0A6ED280\""));

        let pptx = export_presentation_pptx(&sheet, &options).unwrap();
        let xml = slide_xml(&pptx);
        let end = xml.find("<a:t>Note</a:t>").unwrap();
        let start = xml[..end].rfind("<p:sp>").unwrap();
        assert!(xml[start..end].contains("val=\"A55A11\""));
    }
}

#[test]
fn old_shape_styles_without_label_color_keep_the_automatic_default() {
    let style: ObjectStyle = serde_json::from_str(r#"{"stroke_width":2.0,"opacity":1.0}"#).unwrap();
    assert_eq!(style.text_color, None);
    assert!(
        serde_json::to_value(style)
            .unwrap()
            .get("text_color")
            .is_none()
    );
}

#[test]
fn default_card_fills_and_automatic_text_match_the_canvas_in_each_mode() {
    let mut sheet = Sheet::new("Board", "Anchor");
    sheet.root_mode = SheetRootMode::Hidden;
    let sticky = sheet
        .add_card(
            "StickyAuto",
            Vec2::ZERO,
            Card::new(CardKind::Sticky, Vec2::new(160.0, 100.0)),
        )
        .unwrap();
    sheet
        .add_card(
            "TextAuto",
            Vec2::new(220.0, 0.0),
            Card::new(CardKind::Text, Vec2::new(160.0, 100.0)),
        )
        .unwrap();
    sheet
        .add_presentation_slide(
            "Default",
            PresentationSource::Branch(sticky),
            Rect::from_min_size(Vec2::new(-100.0, -100.0), Vec2::new(400.0, 300.0)),
        )
        .unwrap();
    let book = Workbook::new(sheet.clone());

    for (dark, sticky_fill, text_fill) in [
        (
            false,
            Color::rgb(0xFF, 0xF2, 0x9E),
            Color::rgb(0xF5, 0xF7, 0xFA),
        ),
        (
            true,
            Color::rgb(0x9B, 0x7B, 0x24),
            Color::rgb(0x3E, 0x48, 0x58),
        ),
    ] {
        let options = ExportOptions {
            dark,
            ..Default::default()
        };
        let svg = String::from_utf8(
            export_bytes_with_consent(&book, Format::Svg, &options, true).unwrap(),
        )
        .unwrap();
        for (label, fill) in [("StickyAuto", sticky_fill), ("TextAuto", text_fill)] {
            assert!(svg.contains(&format!("fill=\"{}\"", fill.to_hex())));
            let foreground = Color::highest_contrast_on(fill);
            assert!(
                text_element(&svg, label).contains(&format!("fill=\"{}\"", foreground.to_hex()))
            );
        }
        let pptx = export_presentation_pptx(&sheet, &options).unwrap();
        let xml = slide_xml(&pptx);
        assert!(xml.contains(&format!("val=\"{}\"", &sticky_fill.to_hex()[1..])));
    }
}

#[test]
fn shape_effects_render_in_svg_png_pdf_and_powerpoint() {
    use thinoda_core::shape_style::{FillPattern, GradientDirection, ShapeDash, ShapeGradient};
    let build = |effects: bool| {
        let mut sheet = Sheet::new("Board", "Anchor");
        sheet.root_mode = SheetRootMode::Hidden;
        let edits: [fn(&mut ObjectStyle); 6] = [
            |s| s.dash = Some(ShapeDash::default()),
            |s| {
                s.gradient = Some(ShapeGradient {
                    color: Color::rgb(0, 0, 255),
                    direction: GradientDirection::East,
                })
            },
            |s| {
                s.gradient = Some(ShapeGradient {
                    color: Color::rgb(0, 255, 0),
                    direction: GradientDirection::Radial,
                })
            },
            |s| {
                s.shadow = true;
                s.glass = true;
            },
            |s| s.fill_pattern = FillPattern::CrossHatch,
            |s| s.flip_h = true,
        ];
        for (index, edit) in edits.into_iter().enumerate() {
            let mut shape = CanvasObject::shape(
                if index == 5 {
                    ShapeKind::Triangle
                } else {
                    ShapeKind::Rectangle
                },
                Rect::from_min_size(Vec2::new(index as f32 * 150.0, 0.0), Vec2::new(120.0, 80.0)),
            );
            shape.style.fill = Some(Color::rgb(250, 200, 100));
            shape.style.stroke = Some(Color::BLACK);
            if effects {
                edit(&mut shape.style);
            }
            sheet.add_canvas_object(shape).unwrap();
        }
        let card = sheet
            .add_card(
                "Card",
                Vec2::new(0.0, 120.0),
                Card::new(CardKind::Text, Vec2::new(100.0, 40.0)),
            )
            .unwrap();
        sheet
            .add_presentation_slide(
                "All",
                PresentationSource::Branch(card),
                Rect::from_min_size(Vec2::new(-50.0, -50.0), Vec2::new(1000.0, 200.0)),
            )
            .unwrap();
        sheet
    };
    let plain = build(false);
    let styled = build(true);
    let book = Workbook::new(styled.clone());
    book.validate().unwrap();
    let native =
        export_bytes_with_consent(&book, Format::Thinoda, &ExportOptions::default(), true).unwrap();
    assert_eq!(import_bytes(Format::Thinoda, &native).unwrap(), book);

    let svg = |sheet: &Sheet| {
        String::from_utf8(
            export_bytes_with_consent(
                &Workbook::new(sheet.clone()),
                Format::Svg,
                &ExportOptions::default(),
                true,
            )
            .unwrap(),
        )
        .unwrap()
    };
    let (before, after) = (svg(&plain), svg(&styled));
    assert!(after.contains("<linearGradient") && after.contains("<radialGradient"));
    assert!(!before.contains("Gradient"));
    // Dash segments, pattern marks, shadow and glass add primitives.
    assert!(after.matches("<path").count() > before.matches("<path").count() + 20);
    assert!(after.contains("#00000040"), "shadow colour");

    for format in [Format::Png, Format::Pdf] {
        let bytes = |sheet: &Sheet| {
            export_bytes_with_consent(
                &Workbook::new(sheet.clone()),
                format,
                &ExportOptions::default(),
                true,
            )
            .unwrap()
        };
        assert_ne!(bytes(&plain), bytes(&styled), "{format:?}");
    }
    let plain_slide =
        slide_xml(&export_presentation_pptx(&plain, &ExportOptions::default()).unwrap());
    let slide = slide_xml(&export_presentation_pptx(&styled, &ExportOptions::default()).unwrap());
    assert!(slide.contains("<a:gradFill") && !plain_slide.contains("<a:gradFill"));
    assert!(slide.len() > plain_slide.len());
}
#[test]
fn shape_label_text_style_renders_in_svg_png_pdf_and_powerpoint() {
    use std::sync::Arc;
    use thinoda_core::TextAlign;
    use thinoda_core::label_style::{LabelStyle, LabelVAlign};
    let build = |styled: bool| {
        let mut sheet = Sheet::new("Board", "Anchor");
        sheet.root_mode = SheetRootMode::Hidden;
        let mut shape = CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(120.0, 80.0)),
        );
        if let CanvasObjectKind::Shape { label, .. } = &mut shape.kind {
            *label = "Styled".into();
        }

        shape.style.fill = Some(Color::WHITE);
        shape.style.stroke = Some(Color::BLACK);
        if styled {
            shape.label_style = Some(Arc::new(LabelStyle {
                font_family: Some("Courier New".into()),
                font_size: Some(20.0),
                bold: true,
                italic: true,
                underline: true,
                align: TextAlign::Left,
                vertical_position: LabelVAlign::Bottom,
                background: Some(Color::rgb(255, 255, 0)),
                ..LabelStyle::default()
            }));
        }
        sheet.add_canvas_object(shape).unwrap();
        let card = sheet
            .add_card(
                "Card",
                Vec2::new(0.0, 200.0),
                Card::new(CardKind::Text, Vec2::new(100.0, 40.0)),
            )
            .unwrap();
        sheet
            .add_presentation_slide(
                "All",
                PresentationSource::Branch(card),
                Rect::from_min_size(Vec2::new(-50.0, -50.0), Vec2::new(400.0, 300.0)),
            )
            .unwrap();
        sheet
    };
    let plain = build(false);
    let styled = build(true);
    let book = Workbook::new(styled.clone());
    book.validate().unwrap();
    let native =
        export_bytes_with_consent(&book, Format::Thinoda, &ExportOptions::default(), true).unwrap();
    assert_eq!(import_bytes(Format::Thinoda, &native).unwrap(), book);

    let svg = String::from_utf8(
        export_bytes_with_consent(&book, Format::Svg, &ExportOptions::default(), true).unwrap(),
    )
    .unwrap();
    let text = text_element(&svg, "Styled");
    for needle in [
        "font-family=\"'Courier New'",
        "font-weight=\"bold\"",
        "font-style=\"italic\"",
        "underline",
        "text-anchor=\"start\"",
    ] {
        assert!(text.contains(needle), "{needle} in {text}");
    }
    assert!(
        svg.contains("#FFFF00") || svg.contains("#ffff00"),
        "label background"
    );
    // The label sits below the shape.
    let y: f32 = text
        .split(" y=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert!(y > 80.0, "{text}");

    for format in [Format::Png, Format::Pdf] {
        let bytes = |sheet: &Sheet| {
            export_bytes_with_consent(
                &Workbook::new(sheet.clone()),
                format,
                &ExportOptions::default(),
                true,
            )
            .unwrap()
        };
        assert_ne!(bytes(&plain), bytes(&styled), "{format:?}");
    }
    let slide = slide_xml(&export_presentation_pptx(&styled, &ExportOptions::default()).unwrap());
    for needle in [
        "typeface=\"Courier New\"",
        "b=\"1\" i=\"1\" u=\"sng\"",
        "algn=\"l\"",
        "FFFF00",
    ] {
        assert!(slide.contains(needle), "{needle} in {slide}");
    }
}

#[test]
fn rich_shape_runs_flow_together_in_svg_and_editable_powerpoint() {
    use std::sync::Arc;
    use thinoda_core::{
        TextAlign,
        label_style::LabelStyle,
        rich_text::{InlineBaseline, InlineText, InlineTextRun, InlineTextStyle},
    };

    let mut sheet = Sheet::new("Board", "Anchor");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut shape = CanvasObject::shape(
        ShapeKind::Rectangle,
        Rect::from_min_size(Vec2::ZERO, Vec2::new(240.0, 90.0)),
    );
    if let CanvasObjectKind::Shape { label, .. } = &mut shape.kind {
        *label = "WWWWii 2!".into();
    }

    shape.label_style = Some(Arc::new(LabelStyle {
        align: TextAlign::Right,
        ..Default::default()
    }));
    shape.label_rich_text = Some(Arc::new(InlineText {
        runs: vec![
            InlineTextRun {
                start: 0,
                end: 4,
                style: InlineTextStyle {
                    bold: Some(true),
                    ..Default::default()
                },
            },
            InlineTextRun {
                start: 4,
                end: 6,
                style: InlineTextStyle {
                    italic: Some(true),
                    ..Default::default()
                },
            },
            InlineTextRun {
                start: 7,
                end: 8,
                style: InlineTextStyle {
                    baseline: Some(InlineBaseline::Superscript),
                    ..Default::default()
                },
            },
        ],
        ..Default::default()
    }));
    sheet.add_canvas_object(shape).unwrap();
    let card = sheet
        .add_card(
            "Card",
            Vec2::new(0.0, 200.0),
            Card::new(CardKind::Text, Vec2::new(100.0, 40.0)),
        )
        .unwrap();
    sheet
        .add_presentation_slide(
            "All",
            PresentationSource::Branch(card),
            Rect::from_min_size(Vec2::new(-50.0, -50.0), Vec2::new(400.0, 300.0)),
        )
        .unwrap();
    let book = Workbook::new(sheet.clone());
    book.validate().unwrap();
    let svg = String::from_utf8(
        export_bytes_with_consent(&book, Format::Svg, &ExportOptions::default(), true).unwrap(),
    )
    .unwrap();
    let end = svg.find(">WWWW</tspan>").unwrap();
    let start = svg[..end].rfind("<text ").unwrap();
    let stop = end + svg[end..].find("</text>").unwrap() + "</text>".len();
    let label = &svg[start..stop];
    assert!(label.contains("text-anchor=\"end\""), "{label}");
    for text in [">WWWW</tspan>", ">ii</tspan>", ">2</tspan>", ">!</tspan>"] {
        assert!(label.contains(text), "{text}: {label}");
    }
    assert!(label.contains("font-weight=\"bold\""));
    assert!(label.contains("font-style=\"italic\""));
    for format in [Format::Png, Format::Pdf] {
        assert!(
            !export_bytes_with_consent(&book, format, &ExportOptions::default(), true)
                .unwrap()
                .is_empty()
        );
    }

    let slide = slide_xml(&export_presentation_pptx(&sheet, &ExportOptions::default()).unwrap());
    let end = slide.find("<a:t>WWWW</a:t>").unwrap();
    let start = slide[..end].rfind("<p:sp>").unwrap();
    let stop = end + slide[end..].find("</p:sp>").unwrap() + "</p:sp>".len();
    let label = &slide[start..stop];
    for text in [
        "<a:t>WWWW</a:t>",
        "<a:t>ii</a:t>",
        "<a:t>2</a:t>",
        "<a:t>!</a:t>",
    ] {
        assert!(label.contains(text), "{text}: {label}");
    }
    assert!(label.contains(r#"<a:pPr algn="r"/>"#));
    assert!(label.contains(r#"baseline="30000""#));
    assert!(label.contains(r#"<a:t xml:space="preserve"> </a:t>"#));
}

#[test]
fn rich_connector_runs_export_to_svg_png_pdf_and_editable_powerpoint() {
    use std::sync::Arc;
    use thinoda_core::{
        Endpoint, InlineText,
        board::{CanvasConnector, ConnectorLabel, ConnectorLabelPlacement},
    };

    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut connector = CanvasConnector::new(
        Endpoint::Point(Vec2::new(0.0, 0.0)),
        Endpoint::Point(Vec2::new(200.0, 0.0)),
    );
    connector.label = "BOLD plain".into();
    connector.extra_labels.push(ConnectorLabel {
        text: "Extra".into(),
        placement: ConnectorLabelPlacement {
            orthogonal: -60.0,
            ..Default::default()
        },
    });
    let mut object = CanvasObject::connector(connector);
    let mut rich = InlineText::default();
    rich.update_range(object.label(), 0..4, |style| {
        style.bold = Some(true);
        style.color = Some(Color::rgb(200, 30, 40));
    })
    .unwrap();
    object.label_rich_text = Some(Arc::new(rich));
    sheet.add_canvas_object(object).unwrap();
    let mut empty = CanvasObject::connector(CanvasConnector::new(
        Endpoint::Point(Vec2::new(0.0, 120.0)),
        Endpoint::Point(Vec2::new(200.0, 120.0)),
    ));
    empty.label_rich_text = Some(Arc::new(InlineText {
        paragraphs: vec![thinoda_core::InlineParagraph {
            index: 0,
            list: thinoda_core::ListKind::Bulleted,
        }],
        ..Default::default()
    }));
    sheet.add_canvas_object(empty).unwrap();
    let mut list = CanvasObject::connector(CanvasConnector::new(
        Endpoint::Point(Vec2::new(0.0, 180.0)),
        Endpoint::Point(Vec2::new(200.0, 180.0)),
    ));
    list.set_label("Short\nMuch longer").unwrap();
    list.label_rich_text = Some(Arc::new(InlineText {
        paragraphs: (0..2)
            .map(|index| thinoda_core::InlineParagraph {
                index,
                list: thinoda_core::ListKind::Bulleted,
            })
            .collect(),
        ..Default::default()
    }));
    sheet.add_canvas_object(list).unwrap();
    let card = sheet
        .add_card(
            "Anchor",
            Vec2::new(0.0, 200.0),
            Card::new(CardKind::Text, Vec2::new(80.0, 40.0)),
        )
        .unwrap();
    sheet
        .add_presentation_slide(
            "All",
            PresentationSource::Branch(card),
            Rect::from_min_size(Vec2::new(-100.0, -100.0), Vec2::new(400.0, 300.0)),
        )
        .unwrap();
    let book = Workbook::new(sheet.clone());
    book.validate().unwrap();
    let svg = String::from_utf8(
        export_bytes_with_consent(&book, Format::Svg, &ExportOptions::default(), true).unwrap(),
    )
    .unwrap();
    let end = svg.find(">BOLD</tspan>").unwrap();
    let start = svg[..end].rfind("<text ").unwrap();
    let stop = end + svg[end..].find("</text>").unwrap() + "</text>".len();
    let label = &svg[start..stop];
    assert!(label.contains("font-weight=\"bold\""), "{label}");
    assert!(label.contains("fill=\"#C81E28\""), "{label}");
    assert!(label.contains("> plain</tspan>"), "{label}");
    assert!(svg.contains(">Extra</text>"));
    assert!(svg.contains(">• </tspan>"));
    let svg_list_line = |text: &str| {
        let end = svg.find(text).unwrap();
        let start = svg[..end].rfind("<text ").unwrap();
        &svg[start..end]
    };
    let first = svg_list_line(">• Short</tspan>");
    let second = svg_list_line(">• Much longer</tspan>");
    let x = |tag: &str| {
        tag.split("x=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap()
            .to_owned()
    };
    assert!(first.contains("text-anchor=\"start\""));
    assert!(second.contains("text-anchor=\"start\""));
    assert_eq!(x(first), x(second));
    for format in [Format::Png, Format::Pdf] {
        assert!(
            !export_bytes_with_consent(&book, format, &ExportOptions::default(), true)
                .unwrap()
                .is_empty()
        );
    }
    let slide = slide_xml(&export_presentation_pptx(&sheet, &ExportOptions::default()).unwrap());
    let end = slide.find("<a:t>BOLD</a:t>").unwrap();
    let start = slide[..end].rfind("<p:sp>").unwrap();
    let stop = end + slide[end..].find("</p:sp>").unwrap() + "</p:sp>".len();
    let label = &slide[start..stop];
    assert!(label.contains("b=\"1\""), "{label}");
    assert!(label.contains("C81E28"), "{label}");
    assert!(
        label.contains(r#"<a:t xml:space="preserve"> plain</a:t>"#),
        "{label}"
    );
    assert!(slide.contains("<a:t>Extra</a:t>"));
    assert!(slide.contains(r#"<a:t xml:space="preserve">• </a:t>"#));
    let slide_list_line = |text: &str| {
        let end = slide.find(text).unwrap();
        let start = slide[..end].rfind("<p:sp>").unwrap();
        &slide[start..end]
    };
    let first = slide_list_line("<a:t>• Short</a:t>");
    let second = slide_list_line("<a:t>• Much longer</a:t>");
    assert!(first.contains(r#"<a:pPr algn="l"/>"#));
    assert!(second.contains(r#"<a:pPr algn="l"/>"#));
    let offset = |shape: &str| {
        shape
            .split("<a:off x=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap()
            .to_owned()
    };
    assert_eq!(offset(first), offset(second));
}

#[test]
fn rich_sankey_labels_export_formatted_prefix_and_plain_generated_value() {
    use std::sync::Arc;
    use thinoda_core::InlineText;

    let mut sheet = thinoda_core::templates::instantiate("sankey-budget")
        .active()
        .clone();
    let mut count = 0;
    for object in &mut sheet.canvas.objects {
        let CanvasObjectKind::Connector(connector) = &mut object.kind else {
            continue;
        };
        if connector.sankey.is_none() {
            continue;
        }
        let label = if count == 0 { "Ribbon" } else { "Unset" };
        connector.label = label.into();
        if count == 1 {
            connector.sankey.as_mut().unwrap().weight = None;
        }
        let mut rich = InlineText::default();
        rich.update_range(label, 0..label.len(), |style| style.italic = Some(true))
            .unwrap();
        object.label_rich_text = Some(Arc::new(rich));
        count += 1;
        if count == 2 {
            break;
        }
    }
    assert_eq!(count, 2);
    let card = sheet
        .add_card(
            "Anchor",
            Vec2::new(0.0, 300.0),
            Card::new(CardKind::Text, Vec2::new(80.0, 40.0)),
        )
        .unwrap();
    sheet
        .add_presentation_slide(
            "All",
            PresentationSource::Branch(card),
            Rect::from_min_size(Vec2::new(-200.0, -150.0), Vec2::new(900.0, 600.0)),
        )
        .unwrap();
    sheet.presentation.slides.last_mut().unwrap().scope = thinoda_core::PresentationScope::Context;
    let book = Workbook::new(sheet.clone());
    book.validate().unwrap();
    let svg = String::from_utf8(
        export_bytes_with_consent(&book, Format::Svg, &ExportOptions::default(), true).unwrap(),
    )
    .unwrap();
    let slide = slide_xml(&export_presentation_pptx(&sheet, &ExportOptions::default()).unwrap());
    for text in ["Ribbon", "Unset"] {
        let end = svg.find(&format!(">{text}</tspan>")).unwrap();
        let start = svg[..end].rfind("<tspan ").unwrap();
        assert!(svg[start..end].contains("font-style=\"italic\""));
        let suffix = &svg[end..end + svg[end..].find("</text>").unwrap()];
        assert!(suffix.contains("font-style=\"normal\""), "{suffix}");
        let end = slide.find(&format!("<a:t>{text}</a:t>")).unwrap();
        let start = slide[..end].rfind("<a:r>").unwrap();
        assert!(slide[start..end].contains("i=\"1\""));
        let suffix = &slide[end..end + slide[end..].find("</p:sp>").unwrap()];
        assert!(suffix.contains("i=\"0\""), "{suffix}");
    }
}
