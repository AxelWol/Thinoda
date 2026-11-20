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
fn formatted_card_titles_render_in_visual_exports() {
    use thinoda_core::{InlineParagraph, ListKind};
    use thinoda_io::{ExportLoss, export_losses};

    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    for (index, kind) in [CardKind::Sticky, CardKind::Text].into_iter().enumerate() {
        let mut card = Card::new(kind, Vec2::new(220.0, 130.0));
        if kind == CardKind::Sticky {
            card.rotation = 15.0;
        }
        let id = sheet
            .add_card("Bold🙂\nSmall", Vec2::new(index as f32 * 260.0, 0.0), card)
            .unwrap();
        let topic = sheet.topic_mut(id).unwrap();
        topic.style.bold = Some(true);
        topic.inline_text.paragraphs = vec![
            InlineParagraph {
                index: 0,
                list: ListKind::Bulleted,
            },
            InlineParagraph {
                index: 1,
                list: ListKind::Numbered,
            },
        ];
        topic
            .inline_text
            .update_range(&topic.title, 0..5, |style| {
                style.bold = Some(false);
                style.italic = Some(true);
                style.font_size = Some(24.0);
                style.color = Some(Color::rgb(200, 30, 40));
            })
            .unwrap();
        topic
            .inline_text
            .update_range(&topic.title, 6..11, |style| style.underline = Some(true))
            .unwrap();
    }
    let book = Workbook::new(sheet);
    book.validate().unwrap();
    let options = ExportOptions::default();
    let svg =
        String::from_utf8(export_bytes_with_consent(&book, Format::Svg, &options, true).unwrap())
            .unwrap();
    assert_eq!(svg.matches(">Bold🙂</tspan>").count(), 2, "{svg}");
    assert_eq!(svg.matches(">• </tspan>").count(), 2);
    assert_eq!(svg.matches(">1. </tspan>").count(), 2);
    let end = svg.find(">Bold🙂</tspan>").unwrap();
    let start = svg[..end].rfind("<tspan ").unwrap();
    let run = &svg[start..end];
    assert!(run.contains(r#"font-weight="normal""#), "{run}");
    assert!(run.contains(r#"font-style="italic""#), "{run}");
    assert!(run.contains(r#"font-size="24.00""#), "{run}");
    assert!(run.contains(r##"fill="#C81E28""##), "{run}");
    assert!(svg.contains("rotate(15.00"));
    let end = svg.find(">Small</tspan>").unwrap();
    let start = svg[..end].rfind("<tspan ").unwrap();
    assert!(svg[start..end].contains(r#"text-decoration="underline""#));

    let mut plain = book.clone();
    for topic in plain.sheets[0].topics.values_mut() {
        topic.inline_text = Default::default();
    }
    let mut without_color = book.clone();
    for topic in without_color.sheets[0].topics.values_mut() {
        for run in &mut topic.inline_text.runs {
            run.style.color = None;
        }
    }
    let formatted_png = export_bytes_with_consent(&book, Format::Png, &options, true).unwrap();
    let plain_png = export_bytes_with_consent(&plain, Format::Png, &options, true).unwrap();
    let without_color_png =
        export_bytes_with_consent(&without_color, Format::Png, &options, true).unwrap();
    assert_ne!(formatted_png, plain_png);
    let red_pixels = |bytes: &[u8]| {
        image::load_from_memory(bytes)
            .unwrap()
            .to_rgba8()
            .pixels()
            .filter(|pixel| pixel[0] > 150 && pixel[1] < 100 && pixel[2] < 100 && pixel[3] > 0)
            .count()
    };
    let formatted_red = red_pixels(&formatted_png);
    let inherited_red = red_pixels(&without_color_png);
    assert!(
        formatted_red > inherited_red + 10,
        "formatted: {formatted_red}, inherited: {inherited_red}"
    );
    for format in [Format::Svg, Format::Png, Format::Jpeg, Format::Pdf] {
        assert!(
            !export_losses(&book, format, &options)
                .iter()
                .any(|loss| matches!(loss, ExportLoss::InlineTextDropped(_))),
            "{format:?}"
        );
        assert!(
            !export_bytes_with_consent(&book, format, &options, true)
                .unwrap()
                .is_empty()
        );
    }
    assert!(
        export_losses(&book, Format::PowerPoint, &options)
            .iter()
            .any(|loss| matches!(loss, ExportLoss::InlineTextDropped(_)))
    );
}

#[test]
fn card_baseline_overrides_render_in_svg_and_powerpoint() {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let id = sheet
        .add_card(
            "up down",
            Vec2::ZERO,
            Card::new(CardKind::Sticky, Vec2::new(220.0, 130.0)),
        )
        .unwrap();
    let topic = sheet.topic_mut(id).unwrap();
    topic
        .inline_text
        .update_range(&topic.title, 0..2, |style| {
            style.baseline = Some(thinoda_core::InlineBaseline::Superscript);
        })
        .unwrap();
    topic
        .inline_text
        .update_range(&topic.title, 3..7, |style| {
            style.baseline = Some(thinoda_core::InlineBaseline::Subscript);
        })
        .unwrap();
    sheet
        .add_presentation_slide(
            "Card",
            PresentationSource::Branch(id),
            Rect::from_min_size(Vec2::new(-200.0, -200.0), Vec2::new(400.0, 400.0)),
        )
        .unwrap();
    let book = Workbook::new(sheet.clone());
    book.validate().unwrap();
    let svg = String::from_utf8(
        export_bytes_with_consent(&book, Format::Svg, &ExportOptions::default(), true).unwrap(),
    )
    .unwrap();
    for (text, expected_negative) in [("up", true), ("down", false)] {
        let end = svg.find(&format!(">{text}</tspan>")).unwrap();
        let start = svg[..end].rfind("<tspan ").unwrap();
        let shift: f32 = svg[start..end]
            .split("dy=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert!(if expected_negative {
            shift < 0.0
        } else {
            shift > 0.0
        });
    }
    let slide = slide_xml(&export_presentation_pptx(&sheet, &ExportOptions::default()).unwrap());
    assert!(slide.contains(r#"baseline="30000""#));
    assert!(slide.contains(r#"baseline="-25000""#));
}

#[test]
fn formatted_card_overflow_is_inside_the_svg_export_frame() {
    for rotation in [0.0, 90.0] {
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let mut card = Card::new(CardKind::Text, Vec2::new(120.0, 80.0));
        card.rotation = rotation;
        let id = sheet.add_card("A\nB\nC\nD", Vec2::ZERO, card).unwrap();
        let topic = sheet.topic_mut(id).unwrap();
        topic
            .inline_text
            .update_range(&topic.title, 0..7, |style| style.font_size = Some(256.0))
            .unwrap();
        let book = Workbook::new(sheet);
        book.validate().unwrap();
        let svg = String::from_utf8(
            export_bytes_with_consent(&book, Format::Svg, &ExportOptions::default(), true).unwrap(),
        )
        .unwrap();
        let view_box = svg
            .split("viewBox=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap()
            .split_whitespace()
            .map(|part| part.parse::<f32>().unwrap())
            .collect::<Vec<_>>();
        let extent = if rotation == 0.0 {
            view_box[3]
        } else {
            view_box[2]
        };
        assert!(extent > 1000.0, "{rotation}: {view_box:?}");
    }
}

#[test]
fn formatted_card_powerpoint_contains_editable_runs_and_rotated_text() {
    use thinoda_core::{InlineParagraph, ListKind};
    use thinoda_io::{ExportLoss, export_losses};

    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut card = Card::new(CardKind::Sticky, Vec2::new(220.0, 130.0));
    card.rotation = 15.0;
    let id = sheet
        .add_card("Bold plain\nNext", Vec2::ZERO, card)
        .unwrap();
    let topic = sheet.topic_mut(id).unwrap();
    topic.inline_text.paragraphs = vec![InlineParagraph {
        index: 0,
        list: ListKind::Bulleted,
    }];
    topic
        .inline_text
        .update_range(&topic.title, 0..4, |style| {
            style.bold = Some(true);
            style.italic = Some(true);
            style.font_size = Some(24.0);
            style.font_family = Some("Arial".into());
            style.color = Some(Color::rgb(200, 30, 40));
        })
        .unwrap();
    sheet
        .add_presentation_slide(
            "Card",
            PresentationSource::Branch(id),
            Rect::from_min_size(Vec2::new(-200.0, -200.0), Vec2::new(400.0, 400.0)),
        )
        .unwrap();
    let book = Workbook::new(sheet.clone());
    book.validate().unwrap();
    let slide = slide_xml(&export_presentation_pptx(&sheet, &ExportOptions::default()).unwrap());
    let end = slide.find("<a:t>Bold</a:t>").unwrap();
    let start = slide[..end].rfind("<a:r>").unwrap();
    let run = &slide[start..end];
    assert!(run.contains(r#"b="1""#), "{run}");
    assert!(run.contains(r#"i="1""#), "{run}");
    assert!(run.contains(r#"typeface="Arial""#), "{run}");
    assert!(run.contains("C81E28"), "{run}");
    assert!(slide.contains(r#"<a:buChar char="•"/>"#));
    assert!(slide.contains(r#"<a:t xml:space="preserve"> plain</a:t>"#));
    assert!(slide.contains("<a:t>Next</a:t>"));
    let shape_start = slide[..end].rfind("<p:sp>").unwrap();
    assert!(slide[shape_start..end].contains(r#"rot="900000""#));
    assert!(
        export_losses(&book, Format::PowerPoint, &ExportOptions::default())
            .iter()
            .any(|loss| matches!(loss, ExportLoss::InlineTextDropped(_)))
    );
}

#[test]
fn formatted_card_powerpoint_preserves_editable_paragraphs_and_list_numbering() {
    use thinoda_core::{InlineParagraph, ListKind};

    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let id = sheet
        .add_card(
            "A🙂 & B\n\nSecond\nplain\nRestart\n",
            Vec2::ZERO,
            Card::new(CardKind::Text, Vec2::new(220.0, 130.0)),
        )
        .unwrap();
    let topic = sheet.topic_mut(id).unwrap();
    topic.inline_text.paragraphs = [
        (0, ListKind::Bulleted),
        (1, ListKind::Numbered),
        (2, ListKind::Numbered),
        (4, ListKind::Numbered),
    ]
    .into_iter()
    .map(|(index, list)| InlineParagraph { index, list })
    .collect();
    topic
        .inline_text
        .update_range(&topic.title, 1..13, |style| style.bold = Some(true))
        .unwrap();
    sheet
        .add_presentation_slide(
            "Card",
            PresentationSource::Branch(id),
            Rect::from_min_size(Vec2::new(-200.0, -200.0), Vec2::new(400.0, 400.0)),
        )
        .unwrap();
    let slide = slide_xml(&export_presentation_pptx(&sheet, &ExportOptions::default()).unwrap());
    let name = slide.find(r#"name="Card title""#).unwrap();
    let start = slide[..name].rfind("<p:sp>").unwrap();
    let end = slide[name..].find("</p:sp>").unwrap() + name + "</p:sp>".len();
    let shape = &slide[start..end];
    assert_eq!(shape.matches("<a:p>").count(), 6);
    assert_eq!(shape.matches(r#"<a:buChar char="•"/>"#).count(), 1);
    assert_eq!(
        shape.matches(r#"type="arabicPeriod" startAt="1""#).count(),
        2
    );
    assert_eq!(
        shape.matches(r#"type="arabicPeriod" startAt="2""#).count(),
        1
    );
    assert_eq!(
        shape
            .matches(r#"<a:buAutoNum type="arabicPeriod"/>"#)
            .count(),
        0
    );
    assert_eq!(shape.matches("<a:defRPr ").count(), 6);
    assert_eq!(shape.matches("<a:buNone/>").count(), 2);
    assert!(shape.contains("<a:t>A</a:t>"));
    assert!(shape.contains("<a:t>🙂 &amp; B</a:t>"));
    assert!(shape.contains("<a:t>Secon</a:t>"));
    assert!(shape.contains("<a:t>d</a:t>"));
    assert!(shape.contains("<a:t>plain</a:t>"));
    assert!(shape.contains("<a:t>Restart</a:t>"));
    assert!(!shape.contains("<a:t>• "));
    assert!(!shape.contains("<a:t>1. "));
    let mut reader = quick_xml::Reader::from_str(&slide);
    loop {
        if matches!(reader.read_event().unwrap(), quick_xml::events::Event::Eof) {
            break;
        }
    }
}

#[test]
fn formatted_card_numbering_resumes_after_empty_numbered_paragraphs() {
    use thinoda_core::{InlineParagraph, ListKind};

    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let id = sheet
        .add_card(
            "First\n\n\nFourth\nFifth\nplain\nRestart",
            Vec2::ZERO,
            Card::new(CardKind::Sticky, Vec2::new(220.0, 130.0)),
        )
        .unwrap();
    let topic = sheet.topic_mut(id).unwrap();
    topic.inline_text.paragraphs = [0, 1, 2, 3, 4, 6]
        .into_iter()
        .map(|index| InlineParagraph {
            index,
            list: ListKind::Numbered,
        })
        .collect();
    sheet
        .add_presentation_slide(
            "Card",
            PresentationSource::Branch(id),
            Rect::from_min_size(Vec2::new(-200.0, -200.0), Vec2::new(400.0, 400.0)),
        )
        .unwrap();
    let slide = slide_xml(&export_presentation_pptx(&sheet, &ExportOptions::default()).unwrap());
    let name = slide.find(r#"name="Card title""#).unwrap();
    let end = slide[name..].find("</p:sp>").unwrap() + name;
    let shape = &slide[name..end];
    let numbering: Vec<_> = shape
        .split("<a:p>")
        .skip(1)
        .map(|paragraph| {
            let start = paragraph.find("<a:bu").unwrap();
            let end = paragraph[start..].find("/>").unwrap() + start + 2;
            &paragraph[start..end]
        })
        .collect();
    assert_eq!(
        numbering,
        [
            r#"<a:buAutoNum type="arabicPeriod" startAt="1"/>"#,
            r#"<a:buAutoNum type="arabicPeriod"/>"#,
            r#"<a:buAutoNum type="arabicPeriod" startAt="3"/>"#,
            r#"<a:buAutoNum type="arabicPeriod" startAt="4"/>"#,
            r#"<a:buAutoNum type="arabicPeriod" startAt="5"/>"#,
            "<a:buNone/>",
            r#"<a:buAutoNum type="arabicPeriod" startAt="1"/>"#,
        ]
    );
    assert!(!shape.contains("<a:t> </a:t>"));
}

#[test]
fn formatted_card_numbered_list_uses_one_indent_across_double_digit_items() {
    use thinoda_core::{InlineParagraph, ListKind};

    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let id = sheet
        .add_card(
            &["item"; 12].join("\n"),
            Vec2::ZERO,
            Card::new(CardKind::Text, Vec2::new(220.0, 130.0)),
        )
        .unwrap();
    sheet.topic_mut(id).unwrap().inline_text.paragraphs = (0..12)
        .map(|index| InlineParagraph {
            index,
            list: ListKind::Numbered,
        })
        .collect();
    sheet
        .add_presentation_slide(
            "Card",
            PresentationSource::Branch(id),
            Rect::from_min_size(Vec2::new(-200.0, -200.0), Vec2::new(400.0, 400.0)),
        )
        .unwrap();
    let slide = slide_xml(&export_presentation_pptx(&sheet, &ExportOptions::default()).unwrap());
    let start = slide.find(r#"name="Card title""#).unwrap();
    let end = slide[start..].find("</p:sp>").unwrap() + start;
    let shape = &slide[start..end];
    assert_eq!(shape.matches(r#"startAt="1""#).count(), 1);
    assert_eq!(
        shape
            .matches(r#"<a:buAutoNum type="arabicPeriod"/>"#)
            .count(),
        11
    );
    let indents: Vec<_> = shape
        .split(" marL=\"")
        .skip(1)
        .map(|part| part.split('"').next().unwrap())
        .collect();
    assert_eq!(indents.len(), 12);
    assert!(indents.iter().all(|indent| *indent == indents[0]));
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
        rich_text: None,
    });
    let mut extra_rich = InlineText::default();
    extra_rich
        .update_range("Styled", 0..6, |style| style.italic = Some(true))
        .unwrap();
    connector.extra_labels.push(ConnectorLabel {
        text: "Styled".into(),
        placement: ConnectorLabelPlacement {
            orthogonal: -90.0,
            ..Default::default()
        },
        rich_text: Some(Arc::new(extra_rich)),
    });
    connector.extra_labels.push(ConnectorLabel {
        text: String::new(),
        placement: ConnectorLabelPlacement {
            orthogonal: -120.0,
            ..Default::default()
        },
        rich_text: Some(Arc::new(InlineText {
            paragraphs: vec![thinoda_core::InlineParagraph {
                index: 0,
                list: thinoda_core::ListKind::Bulleted,
            }],
            ..Default::default()
        })),
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
    let end = svg.find(">Styled</tspan>").unwrap();
    let start = svg[..end].rfind("<text ").unwrap();
    assert!(svg[start..end].contains("font-style=\"italic\""));
    assert_eq!(svg.matches(">• </tspan>").count(), 2);
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
    let end = slide.find("<a:t>Styled</a:t>").unwrap();
    let start = slide[..end].rfind("<p:sp>").unwrap();
    assert!(slide[start..end].contains("i=\"1\""));
    assert_eq!(
        slide
            .matches(r#"<a:t xml:space="preserve">• </a:t>"#)
            .count(),
        2
    );
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
    use thinoda_core::{
        InlineText,
        board::{ConnectorLabel, ConnectorLabelPlacement},
    };

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
        let extra = if count == 0 {
            "Ribbon extra"
        } else {
            "Unset extra"
        };
        let mut extra_rich = InlineText::default();
        extra_rich
            .update_range(extra, 0..extra.len(), |style| style.bold = Some(true))
            .unwrap();
        connector.extra_labels.push(ConnectorLabel {
            text: extra.into(),
            placement: ConnectorLabelPlacement {
                orthogonal: -80.0,
                ..Default::default()
            },
            rich_text: Some(Arc::new(extra_rich)),
        });
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
    for text in ["Ribbon extra", "Unset extra"] {
        let end = svg.find(&format!(">{text}</tspan>")).unwrap();
        let start = svg[..end].rfind("<tspan ").unwrap();
        assert!(svg[start..end].contains("font-weight=\"bold\""));
        let end = slide.find(&format!("<a:t>{text}</a:t>")).unwrap();
        let start = slide[..end].rfind("<a:r>").unwrap();
        assert!(slide[start..end].contains("b=\"1\""));
    }
    for format in [Format::Png, Format::Pdf] {
        assert!(
            !export_bytes_with_consent(&book, format, &ExportOptions::default(), true)
                .unwrap()
                .is_empty()
        );
    }
}
