use super::*;
use crate::{ExportLoss, ExportOptions, detect_format, export_bytes, export_losses};
use std::path::Path;
use thinoda_core::{PresentationSource, board::CanvasCardState};

/// Written by draw.io's algorithm (deflateRaw of encodeURIComponent) in Python,
/// independently of this codec.
const GOLDEN_COMPRESSED: &str = "xZTNboMwDMefJndKpqo7rqzdaSeeICUuiRYIMm5L9/RzICtUtPuQJu2AiD/+TvzDQcis6l5QNebVa3AiTUq0WshnkaYLfqIjt+8wONMkeOVGyAy9p2FVdRm4IP6U9knp9k40Fm4UQk0/EahBcFTuEI+Rk0ISQbZUVSPkOhzUD2ktnV1M4zK2adlYn4wlyBtVhMiJG2afocqxteDl3jqXeeex10mtYLUv2N8S+jeYRJbFCnb7fsP+SIAE3d22FhdYTBl8BYRnTomCh6Hr5HxtnqwmE/URTGLAliYWXUafage7vBQeEfIiUrxNdDcjKjIpHrM6vJ62BQ/DDCb6Q61BR2LfAP0DPP/Jp5iPki4hj6ZHMr70tXKb0bse+SS3WIQCX5Pg/fwBC7gaeh70Emjy1ea8EJwie7yu/ovu2Rzvch+b/BHk5gM=";

fn page(cells: &str) -> String {
    format!(
        "<mxfile><diagram name=\"P\"><mxGraphModel><root><mxCell id=\"0\"/><mxCell id=\"1\" parent=\"0\"/>{cells}</root></mxGraphModel></diagram></mxfile>"
    )
}

fn load(xml: &str) -> ImportReport {
    import(xml.as_bytes()).unwrap_or_else(|e| panic!("{e}"))
}

fn clean(xml: &str) -> Sheet {
    let report = load(xml);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    report.workbook.sheets.into_iter().next().unwrap()
}

fn shapes(sheet: &Sheet) -> Vec<&CanvasObject> {
    sheet
        .canvas
        .objects
        .iter()
        .filter(|o| matches!(o.kind, CanvasObjectKind::Shape { .. }))
        .collect()
}

fn connectors(sheet: &Sheet) -> Vec<&CanvasConnector> {
    sheet
        .canvas
        .objects
        .iter()
        .filter_map(|o| match &o.kind {
            CanvasObjectKind::Connector(c) => Some(c),
            _ => None,
        })
        .collect()
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::from_min_size(Vec2::new(x, y), Vec2::new(w, h))
}

fn shape(kind: ShapeKind, frame: Rect, label: &str) -> CanvasObject {
    let mut object = CanvasObject::shape(kind, frame);
    if let CanvasObjectKind::Shape { label: text, .. } = &mut object.kind {
        *text = label.into();
    }
    object
}

/// A board exercising every native draw.io mapping.
fn board() -> Workbook {
    let mut s = Sheet::new("Board", "");
    s.root_mode = SheetRootMode::Hidden;
    s.board.grid = GridKind::Lines;
    s.board.grid_size = 16.0;
    let mut cloud = shape(
        ShapeKind::Cloud,
        rect(0.0, 300.0, 120.0, 80.0),
        "Cloud & <co>\nline 2",
    );
    cloud.style.fill = Some(Color::rgba(0x33, 0x99, 0xFF, 128));
    cloud.style.stroke = Some(Color::rgb(1, 2, 3));
    cloud.style.text_color = Some(Color::WHITE);
    cloud.style.stroke_width = 3.5;
    cloud.style.opacity = 0.5;
    cloud.rotation = 30.0;
    cloud.alt_text = "A cloud".into();
    let cloud = s.add_canvas_object(cloud).unwrap();
    let mut triangle = shape(ShapeKind::Triangle, rect(200.0, 300.0, 60.0, 40.0), "");
    triangle.band = Band::Below;
    let triangle = s.add_canvas_object(triangle).unwrap();
    let mut text = shape(
        ShapeKind::Rectangle,
        rect(400.0, 0.0, 100.0, 30.0),
        "Free text",
    );
    text.style.fill = Some(Color::TRANSPARENT);
    text.style.stroke = Some(Color::TRANSPARENT);
    s.add_canvas_object(text).unwrap();
    for (i, kind) in [
        ShapeKind::Hexagon,
        ShapeKind::Star,
        ShapeKind::Decision,
        ShapeKind::Cylinder,
        ShapeKind::Chevron,
        ShapeKind::RightTriangle,
    ]
    .into_iter()
    .enumerate()
    {
        s.add_canvas_object(shape(kind, rect(i as f32 * 80.0, 600.0, 60.0, 50.0), ""))
            .unwrap();
    }
    let mut card = Card::new(CardKind::Sticky, Vec2::new(160.0, 120.0));
    card.rotation = 8.0;
    card.color = Some(Color::rgb(0xFF, 0xF2, 0xA8));
    let sticky = s
        .add_card("Sticky", Vec2::new(-200.0, 100.0), card)
        .unwrap();
    s.topic_mut(sticky).unwrap().style.fill = card.color;
    s.topic_mut(sticky).unwrap().hyperlink = Some("https://example.com/?a=1&b=2".into());
    let text_card = s
        .add_card(
            "Text card",
            Vec2::new(-200.0, 400.0),
            Card::new(CardKind::Text, Vec2::new(120.0, 40.0)),
        )
        .unwrap();
    s.add_canvas_object(CanvasObject::ink(InkStroke::new(
        InkTool::Highlighter,
        Color::rgb(250, 220, 0),
        12.0,
        vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(40.0, 30.0),
            Vec2::new(90.0, 10.0),
        ],
    )))
    .unwrap();
    let mut connector = CanvasConnector::new(Endpoint::Object(cloud), Endpoint::Topic(sticky));
    connector.label = "links".into();
    connector.route = ConnectorRoute::Elbow;
    connector.arrow_start = true;
    connector.color = Some(Color::rgb(200, 0, 0));
    connector.width = 4.0;
    s.add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    let mut curve = CanvasConnector::new(
        Endpoint::Point(Vec2::new(-50.0, -50.0)),
        Endpoint::Object(triangle),
    );
    curve.route = ConnectorRoute::Curve;
    curve.arrow_end = false;
    s.add_canvas_object(CanvasObject::connector(curve)).unwrap();
    // A locked group of a shape and a card inside a clipped zone.
    let zone = s
        .create_board_frame("Lane", rect(1000.0, 0.0, 600.0, 400.0))
        .unwrap();
    let boxed = s
        .add_canvas_object(shape(
            ShapeKind::RoundedRectangle,
            rect(1100.0, 100.0, 100.0, 60.0),
            "In zone",
        ))
        .unwrap();
    let zone_card = s
        .add_card(
            "Zone card",
            Vec2::new(1400.0, 200.0),
            Card::new(CardKind::Sticky, Vec2::new(100.0, 100.0)),
        )
        .unwrap();
    s.set_zone_members(zone, vec![zone_card], vec![boxed])
        .unwrap();
    {
        let zone = s.zones.iter_mut().find(|z| z.id == zone).unwrap();
        zone.clip = true;
        zone.color = Some(Color::rgb(0x6C, 0x8E, 0xBF));
        zone.background_color = Some(Color::rgb(0xDA, 0xE8, 0xFC));
    }
    let group = s
        .group_board_members(&[BoardMember::Object(boxed), BoardMember::Card(zone_card)])
        .unwrap();
    s.canvas
        .groups
        .iter_mut()
        .find(|g| g.id == group)
        .unwrap()
        .locked = true;
    s.set_board_members_locked(&[BoardMember::Card(text_card)], true)
        .unwrap();
    let mut second = Sheet::new("Second & <page>", "");
    second.root_mode = SheetRootMode::Hidden;
    second
        .add_canvas_object(shape(ShapeKind::Ellipse, rect(0.0, 0.0, 50.0, 50.0), "Two"))
        .unwrap();
    let mut book = Workbook::new(s);
    book.sheets.push(second);
    book.validate().unwrap();
    book
}

fn roundtrip(book: &Workbook) -> Workbook {
    let bytes = export(book).unwrap();
    let report = import(&bytes).unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    report.workbook.validate().unwrap();
    report.workbook
}

type CardSummary = (String, Card, Option<Vec2>, Option<String>, Option<Color>);

fn cards(sheet: &Sheet) -> Vec<CardSummary> {
    sheet
        .topics
        .values()
        .filter(|t| t.card.is_some())
        .map(|t| {
            (
                t.title.clone(),
                t.card.unwrap(),
                t.position,
                t.hyperlink.clone(),
                t.style.fill,
            )
        })
        .collect()
}

fn shape_summaries(sheet: &Sheet) -> Vec<String> {
    let mut out: Vec<String> = shapes(sheet)
        .iter()
        .map(|o| {
            format!(
                "{:?}|{:?}|{}|{:?}|{:?}|{}",
                o.kind, o.rect, o.rotation, o.band, o.style, o.alt_text
            )
        })
        .collect();
    out.sort();
    out
}

#[test]
fn board_round_trips_without_warnings_and_is_idempotent() {
    let original = board();
    let first = roundtrip(&original);
    let second = roundtrip(&first);
    assert_eq!(first, second, "re-export changed the imported board");
    assert_eq!(export(&first).unwrap(), export(&second).unwrap());

    let sheet = &first.sheets[0];
    let source = &original.sheets[0];
    assert_eq!(first.sheets.len(), 2);
    assert_eq!(sheet.title, "Board");
    assert_eq!(first.sheets[1].title, "Second & <page>");
    assert_eq!(sheet.root_mode, SheetRootMode::Hidden);
    assert_eq!(sheet.board.grid, GridKind::Lines);
    assert_eq!(sheet.board.grid_size, 16.0);
    assert_eq!(shape_summaries(sheet), shape_summaries(source));

    let imported_cards = cards(sheet);
    assert_eq!(imported_cards.len(), 3);
    for card in cards(source) {
        assert!(
            imported_cards.contains(&card),
            "missing {card:?} in {imported_cards:?}"
        );
    }

    let links = connectors(sheet);
    assert_eq!(links.len(), 2);
    let elbow = links
        .iter()
        .find(|c| c.route == ConnectorRoute::Elbow)
        .unwrap();
    assert!(matches!(elbow.from, Endpoint::Object(_)));
    assert!(matches!(elbow.to, Endpoint::Topic(_)));
    assert_eq!(
        (elbow.label.as_str(), elbow.arrow_start, elbow.arrow_end),
        ("links", true, true)
    );
    assert_eq!(
        (elbow.color, elbow.width),
        (Some(Color::rgb(200, 0, 0)), 4.0)
    );
    let curve = links
        .iter()
        .find(|c| c.route == ConnectorRoute::Curve)
        .unwrap();
    assert_eq!(curve.from, Endpoint::Point(Vec2::new(-50.0, -50.0)));
    assert!(matches!(curve.to, Endpoint::Object(_)));
    assert!(!curve.arrow_end);

    let ink: Vec<_> = sheet
        .canvas
        .objects
        .iter()
        .filter_map(|o| match &o.kind {
            CanvasObjectKind::Ink(ink) => Some(ink),
            _ => None,
        })
        .collect();
    assert_eq!(ink.len(), 1);
    assert_eq!(ink[0].tool, InkTool::Highlighter);
    assert_eq!(ink[0].width, 12.0);
    assert_eq!(ink[0].points.len(), 3);
    assert_eq!(ink[0].points[1], Vec2::new(40.0, 30.0));

    assert_eq!(sheet.zones.len(), 1);
    let zone = &sheet.zones[0];
    assert_eq!(zone.name, "Lane");
    assert_eq!(zone.frame, rect(1000.0, 0.0, 600.0, 400.0));
    assert!(zone.clip);
    assert_eq!(zone.color, Some(Color::rgb(0x6C, 0x8E, 0xBF)));
    assert_eq!(zone.background_color, Some(Color::rgb(0xDA, 0xE8, 0xFC)));
    assert_eq!((zone.roots.len(), zone.objects.len()), (1, 1));
    assert_eq!(sheet.canvas.groups.len(), 1);
    assert!(sheet.canvas.groups[0].locked);
    assert_eq!(
        sheet.board_group_members(sheet.canvas.groups[0].id).len(),
        2
    );
    let locked_cards: Vec<_> = sheet
        .canvas
        .card_states
        .iter()
        .filter(|s: &&CanvasCardState| s.locked)
        .map(|s| sheet.topic(s.topic).unwrap().title.clone())
        .collect();
    assert_eq!(locked_cards, ["Text card"]);
}

#[test]
fn export_is_well_formed_uncompressed_and_escaped() {
    let text = String::from_utf8(export(&board()).unwrap()).unwrap();
    let root = parse_dom(&text).unwrap();
    assert_eq!(root.name, "mxfile");
    assert_eq!(
        root.children.iter().filter(|c| c.name == "diagram").count(),
        2
    );
    assert!(text.contains(
        "label=\"Cloud &amp;amp; &amp;lt;co&amp;gt;&lt;br&gt;line 2\" id=\"o2\" tooltip=\"A cloud\""
    ));
    assert!(text.contains("name=\"Second &amp; &lt;page&gt;\""));
    assert!(text.contains("shape=cloud;"));
    assert!(text.contains("thinodaCard=sticky;"));
    assert!(text.contains("thinodaInk=highlighter;"));
    assert!(text.contains("thinodaClip=1;"));
    assert!(text.contains("link=\"https://example.com/?a=1&amp;b=2\""));
    let report = load(&text);
    let label = report.workbook.sheets[0]
        .canvas
        .objects
        .iter()
        .map(|o| o.label())
        .find(|l| l.starts_with("Cloud"))
        .map(str::to_owned);
    assert_eq!(label.as_deref(), Some("Cloud & <co>\nline 2"));
}

#[test]
fn golden_compressed_page_imports() {
    let xml = format!(
        "<mxfile host=\"app.diagrams.net\"><diagram id=\"x\" name=\"Flow\">{GOLDEN_COMPRESSED}</diagram></mxfile>"
    );
    let sheet = clean(&xml);
    assert_eq!(sheet.title, "Flow");
    assert_eq!(sheet.board.grid, GridKind::Lines);
    assert_eq!(sheet.board.grid_size, 20.0);
    let objects = shapes(&sheet);
    assert_eq!(objects.len(), 2);
    assert_eq!(objects[0].label(), "Start & go");
    assert_eq!(objects[0].style.fill, Some(Color::rgb(0xDA, 0xE8, 0xFC)));
    assert_eq!(objects[1].label(), "Ünïcode");
    assert!(matches!(
        &objects[1].kind,
        CanvasObjectKind::Shape {
            shape: ShapeKind::RoundedRectangle,
            ..
        }
    ));
    let links = connectors(&sheet);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].route, ConnectorRoute::Elbow);
    assert_eq!(links[0].from, Endpoint::Object(objects[0].id));
}

#[test]
fn published_link_from_the_editor_imports_without_network() {
    // Created using File > Publish > Link > Include a copy of my diagram.
    let published = concat!(
        "https://viewer.diagrams.net/?tags=%7B%7D&lightbox=1&highlight=0000ff",
        "&edit=_blank&layers=1&nav=1&dark=auto#R%3Cmxfile%3E%3Cdiagram%20",
        "name%3D%22Page-1%22%20id%3D%22ccow51MBEuUA37MJHwmB%22%3E",
        "jZLLboMwEEW%2FxstIgFPSbssjXbRSVRZRV5WFJ9iSwciYAvn6mjIEUFopG2vmzMu%2BY0Kjsj8aVos3zUGRwOM9oTEJgsfw4M4RDAj2wQQKI%2FmE%2FAVk8gIIPaSt5NBsEq3Wysp6C3NdVZDbDWPG6G6bdtZqO7VmBdyALGfqlp4ktwJfERwW%2FgKyEPNkP3yaIiWbk%2FEljWBcdytEE0Ijo7WdrLKPQI3azbpMdek%2F0evFDFT2noJLnMafH0l12n0F2Xubvp6jbPdHF0SNHWYNjG4rDmMbj9DnTkgLWc3yMdq5pTsmbKmc5ztzqv5mqsVqBGAs9KsJeMkj6BKsGVyKWOkYomjdork%2FC4ld6AP6%2BLP2HvoMV15cWy%2BqOAOFmd1lAb%2Bx1S%2BmyQ8%3D",
        "%3C%2Fdiagram%3E%3C%2Fmxfile%3E",
    );
    for link in [
        published.to_owned(),
        format!("#{}", published.split_once('#').unwrap().1),
        published.replacen("https://viewer.diagrams.net", "https://example.invalid", 1),
    ] {
        let report = import_link(&link).unwrap();
        assert_eq!(report.format, Format::DrawIo);
        assert_eq!(report.workbook.sheets.len(), 1);
        assert_eq!(report.workbook.sheets[0].title, "Page-1");
        assert_eq!(shapes(&report.workbook.sheets[0]).len(), 1);
        report.workbook.validate().unwrap();
    }
}

#[test]
fn published_links_accept_multiple_pages_and_compressed_graphs() {
    let xml = String::from_utf8(export(&board()).unwrap()).unwrap();
    let link = format!(
        "https://example.invalid/view?mode=test#R{}",
        percent_encode(&xml)
    );
    let report = import_link(&link).unwrap();
    assert_eq!(report.workbook.sheets.len(), 2);
    assert_eq!(report.workbook.sheets[1].title, "Second & <page>");
    assert!(
        shapes(&report.workbook.sheets[0])
            .iter()
            .any(|shape| shape.label() == "Cloud & <co>\nline 2")
    );

    let compressed = format!("#R{}", percent_encode(GOLDEN_COMPRESSED));
    let sheet = &import_link(&compressed).unwrap().workbook.sheets[0];
    assert_eq!(shapes(sheet)[0].label(), "Start & go");
    assert_eq!(shapes(sheet)[1].label(), "Ünïcode");
}

#[test]
fn published_links_reject_invalid_or_external_data() {
    for link in [
        "",
        "https://viewer.diagrams.net/",
        "https://viewer.diagrams.net/#Uhttps%3A%2F%2Fexample.invalid%2Fmap.drawio",
        "https://viewer.diagrams.net/#R",
        "https://viewer.diagrams.net/#R%ZZ",
        "https://viewer.diagrams.net/#R%FF",
        "https://viewer.diagrams.net/#Rnot-base64",
        "file:///tmp/diagram#R%3Cmxfile%2F%3E",
        "https://viewer.diagrams.net/#R%3Cmxfile%3E",
    ] {
        assert!(import_link(link).is_err(), "{link}");
    }
    let dtd = percent_encode("<!DOCTYPE mxfile><mxfile><diagram/></mxfile>");
    assert!(import_link(&format!("#R{dtd}")).is_err());
    assert!(import_link(&format!("#R{}", "x".repeat(MAX_LINK_BYTES))).is_err());
}

#[test]
fn zlib_wrapped_page_and_bare_graph_model_import() {
    let inner = "<mxGraphModel><root><mxCell id=\"0\"/><mxCell id=\"1\" parent=\"0\"/><mxCell id=\"v\" value=\"Z\" vertex=\"1\" parent=\"1\"><mxGeometry width=\"10\" height=\"10\" as=\"geometry\"/></mxCell></root></mxGraphModel>";
    let data = miniz_oxide::deflate::compress_to_vec_zlib(inner.as_bytes(), 6);
    let encoded = base64::engine::general_purpose::STANDARD.encode(data);
    let sheet = clean(&format!(
        "<mxfile><diagram>\n{encoded}\n</diagram></mxfile>"
    ));
    assert_eq!(sheet.title, "Page-1");
    assert_eq!(shapes(&sheet)[0].label(), "Z");
    let bare = clean(inner);
    assert_eq!(shapes(&bare)[0].label(), "Z");
}

fn percent_encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                char::from(b).to_string()
            }
            b => format!("%{b:02X}"),
        })
        .collect()
}

fn png_chunk(png: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
    png.extend((data.len() as u32).to_be_bytes());
    png.extend(kind);
    png.extend(data);
    png.extend([0; 4]);
}

#[test]
fn svg_and_png_containers_are_detected_and_imported() {
    let model = page(
        r#"<mxCell id="v" value="Embedded" style="ellipse;" vertex="1" parent="1"><mxGeometry width="40" height="40" as="geometry"/></mxCell>"#,
    );
    let escaped = model
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;");
    let svg = format!(
        "<?xml version=\"1.0\"?><!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd\"><svg xmlns=\"http://www.w3.org/2000/svg\" content=\"{escaped}\"><rect/></svg>"
    );
    assert_eq!(
        detect_format(Path::new("a.drawio.svg"), svg.as_bytes()),
        Some(Format::DrawIo)
    );
    assert_eq!(shapes(&clean(&svg))[0].label(), "Embedded");
    let plain_svg = "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect/></svg>";
    assert_eq!(
        detect_format(Path::new("a.svg"), plain_svg.as_bytes()),
        Some(Format::Svg)
    );

    let mut png = PNG_SIGNATURE.to_vec();
    png_chunk(&mut png, b"IHDR", &[0; 13]);
    let mut text = b"mxfile\0".to_vec();
    text.extend(percent_encode(&model).bytes());
    png_chunk(&mut png, b"tEXt", &text);
    png_chunk(&mut png, b"IEND", &[]);
    assert_eq!(
        detect_format(Path::new("a.drawio.png"), &png),
        Some(Format::DrawIo)
    );
    let report = import(&png).unwrap();
    assert_eq!(shapes(&report.workbook.sheets[0])[0].label(), "Embedded");
    let mut plain_png = PNG_SIGNATURE.to_vec();
    png_chunk(&mut plain_png, b"IEND", &[]);
    assert_eq!(
        detect_format(Path::new("a.png"), &plain_png),
        Some(Format::Png)
    );

    for name in ["a.xml", "diagram", "a.dio", "a.drawio"] {
        assert_eq!(
            detect_format(Path::new(name), model.as_bytes()),
            Some(Format::DrawIo),
            "{name}"
        );
    }
}

#[test]
fn legacy_svg_uri_and_base64_content_are_detected_and_imported() {
    let label = "Legacy \u{fc}";
    let model = page(&format!(
        r#"<mxCell id="v" value="{label}" style="ellipse;" vertex="1" parent="1"><mxGeometry width="40" height="40" as="geometry"/></mxCell>"#
    ));
    let xml = format!("<?xml version=\"1.0\"?>{model}");
    let uri = percent_encode(&xml);
    let compressed = miniz_oxide::deflate::compress_to_vec(uri.as_bytes(), 6);
    let encoded = [
        ("URI", uri.clone()),
        (
            "base64 XML",
            base64::engine::general_purpose::STANDARD.encode(xml.as_bytes()),
        ),
        (
            "base64 URI",
            base64::engine::general_purpose::STANDARD.encode(uri.as_bytes()),
        ),
        (
            "base64 compressed",
            base64::engine::general_purpose::STANDARD.encode(compressed),
        ),
    ];
    for (encoding, content) in encoded {
        let svg = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" content="{content}"/>"#);
        assert_eq!(
            detect_format(Path::new("legacy.svg"), svg.as_bytes()),
            Some(Format::DrawIo),
            "{encoding}"
        );
        let report = import(svg.as_bytes()).unwrap();
        assert!(
            report.warnings.is_empty(),
            "{encoding}: {:?}",
            report.warnings
        );
        assert_eq!(shapes(&report.workbook.sheets[0])[0].label(), label);
    }

    let unrelated = r#"<svg content="&lt;mxfileUnexpected/&gt;"/>"#;
    assert_eq!(
        detect_format(Path::new("picture.svg"), unrelated.as_bytes()),
        Some(Format::Svg)
    );
    assert!(import(br#"<svg content="%3Cmxfile%ZZ"/>"#).is_err());
}

#[test]
fn legacy_png_keyword_is_detected_and_imported() {
    let label = "Legacy \u{fc}";
    let model = page(&format!(
        r#"<mxCell id="v" value="{label}" style="ellipse;" vertex="1" parent="1"><mxGeometry width="40" height="40" as="geometry"/></mxCell>"#
    ));
    let text = percent_encode(&model);
    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(text.as_bytes(), 6);
    for (kind, flag) in [(b"tEXt", 0), (b"zTXt", 1), (b"iTXt", 0), (b"iTXt", 1)] {
        let mut data = b"mxGraphModel\0".to_vec();
        match kind {
            b"zTXt" => data.push(0),
            b"iTXt" => data.extend_from_slice(&[flag, 0, 0, 0]),
            _ => {}
        }
        data.extend_from_slice(if flag == 1 {
            &compressed
        } else if kind == b"tEXt" {
            text.as_bytes()
        } else {
            model.as_bytes()
        });
        let mut png = PNG_SIGNATURE.to_vec();
        png_chunk(&mut png, b"IHDR", &[0; 13]);
        png_chunk(&mut png, kind, &data);
        png_chunk(&mut png, b"IEND", &[]);
        for name in ["legacy.png", "legacy.drawio.png"] {
            assert_eq!(detect_format(Path::new(name), &png), Some(Format::DrawIo));
        }
        let report = import(&png).unwrap();
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(shapes(&report.workbook.sheets[0])[0].label(), label);
    }

    let bare_model = r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="v" value="Bare" vertex="1" parent="1"><mxGeometry width="40" height="40" as="geometry"/></mxCell></root></mxGraphModel>"#;
    let mut text = b"mxGraphModel\0".to_vec();
    text.extend_from_slice(percent_encode(bare_model).as_bytes());
    let mut png = PNG_SIGNATURE.to_vec();
    png_chunk(&mut png, b"tEXt", &text);
    png_chunk(&mut png, b"IEND", &[]);
    let report = import(&png).unwrap();
    assert_eq!(shapes(&report.workbook.sheets[0])[0].label(), "Bare");

    for (flag, method) in [(2, 0), (0, 1), (1, 1)] {
        let mut data = b"mxGraphModel\0".to_vec();
        data.extend_from_slice(&[flag, method, 0, 0]);
        data.extend_from_slice(model.as_bytes());
        let mut png = PNG_SIGNATURE.to_vec();
        png_chunk(&mut png, b"iTXt", &data);
        png_chunk(&mut png, b"IEND", &[]);
        assert_eq!(
            detect_format(Path::new("invalid.png"), &png),
            Some(Format::Png)
        );
        assert!(import(&png).is_err());
    }
}

#[test]
fn png_compressed_and_international_text_chunks_import_diagrams() {
    let label = "Embedded \u{fc}";
    let model = page(&format!(
        r#"<mxCell id="v" value="{label}" style="ellipse;" vertex="1" parent="1"><mxGeometry width="40" height="40" as="geometry"/></mxCell>"#
    ));
    let text = percent_encode(&model);
    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(text.as_bytes(), 6);
    for (kind, flag) in [(b"zTXt", 1), (b"iTXt", 0), (b"iTXt", 1)] {
        let mut data = b"mxfile\0".to_vec();
        if kind == b"zTXt" {
            data.push(0);
        } else {
            data.extend([flag, 0]);
            data.extend_from_slice(b"en\0Diagram\0");
        }
        data.extend_from_slice(if flag == 1 {
            &compressed
        } else {
            model.as_bytes()
        });
        let mut png = PNG_SIGNATURE.to_vec();
        png_chunk(&mut png, b"IHDR", &[0; 13]);
        png_chunk(&mut png, kind, &data);
        png_chunk(&mut png, b"IEND", &[]);
        assert_eq!(
            detect_format(Path::new("embedded.drawio.png"), &png),
            Some(Format::DrawIo)
        );
        let report = import(&png).unwrap();
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(shapes(&report.workbook.sheets[0])[0].label(), label);
    }
}

#[test]
fn published_drawio_png_export_imports_its_embedded_diagram() {
    let png = include_bytes!("../../tests/fixtures/published-rectangle.drawio.png");
    assert_eq!(
        detect_format(Path::new("published-rectangle.drawio.png"), png),
        Some(Format::DrawIo)
    );
    let report = import(png).unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let sheet = &report.workbook.sheets[0];
    assert_eq!(sheet.title, "Page-1");
    let objects = shapes(sheet);
    assert_eq!(objects.len(), 1);
    assert!(matches!(
        objects[0].kind,
        CanvasObjectKind::Shape {
            shape: ShapeKind::Rectangle,
            ..
        }
    ));
    assert_eq!(objects[0].label(), "Published PNG");
    assert_eq!(objects[0].style.fill, Some(Color::rgb(0xd5, 0xe8, 0xd4)));
    assert_eq!(objects[0].rect, rect(350.0, 380.0, 120.0, 60.0));
}

fn html_viewer(xml: &str) -> String {
    let data = serde_json::json!({ "xml": xml, "edit": "_blank" }).to_string();
    format!(
        r#"<div class="example mxgraph" data-mxgraph="{}"></div>"#,
        html_escape::encode_double_quoted_attribute(&data)
    )
}

fn legacy_html_viewer(data: &str) -> String {
    format!(
        r#"<div class="mxgraph"><div style="width:1px;height:1px;overflow:hidden;">{data}</div></div>"#
    )
}

#[test]
fn published_drawio_html_export_imports_its_embedded_diagram() {
    let html = include_bytes!("../../tests/fixtures/published-rectangle.drawio.html");
    for name in ["published.html", "published.HTM", "published.drawio.html"] {
        assert_eq!(Format::from_path(Path::new(name)), Some(Format::DrawIoHtml));
        assert_eq!(
            detect_format(Path::new(name), html),
            Some(Format::DrawIoHtml)
        );
        let report = crate::import_named_bytes_with_report(Path::new(name), html).unwrap();
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        let sheet = &report.workbook.sheets[0];
        assert_eq!(sheet.title, "Page-1");
        let objects = shapes(sheet);
        assert_eq!(objects.len(), 1);
        assert_eq!(objects[0].label(), "Published PNG");
        assert_eq!(objects[0].style.fill, Some(Color::rgb(0xd5, 0xe8, 0xd4)));
        assert_eq!(objects[0].rect, rect(350.0, 380.0, 120.0, 60.0));
    }
    assert_eq!(
        detect_format(Path::new("without-extension"), html),
        Some(Format::DrawIoHtml)
    );
    assert!(Format::DrawIoHtml.can_import());
    assert!(!Format::DrawIoHtml.can_export());
}

#[test]
fn html_viewer_preserves_entities_pages_and_ignores_inert_markup() {
    let xml = r#"<mxfile><diagram name="First"><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="v" value="A &amp; B &quot;C&quot; ü" vertex="1" parent="1"><mxGeometry width="40" height="40" as="geometry"/></mxCell></root></mxGraphModel></diagram><diagram name="Second"><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/></root></mxGraphModel></diagram></mxfile>"#;
    let fake = r#"<div class="mxgraph" data-mxgraph="not a diagram"></div>"#;
    let html = format!(
        "<!DOCTYPE html><html><head><style>{fake}</style><script>{fake}</script></head><body><!-- {fake} --><textarea>{fake}</textarea><img src=\"https://example.invalid/image\">{}</body></html>",
        html_viewer(xml)
    );
    let report = import_html(html.as_bytes()).unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(report.workbook.sheets.len(), 2);
    assert_eq!(report.workbook.sheets[0].title, "First");
    assert_eq!(report.workbook.sheets[1].title, "Second");
    assert_eq!(
        shapes(&report.workbook.sheets[0])[0].label(),
        "A & B \"C\" ü"
    );
}

#[test]
fn html_viewer_rejects_missing_remote_malformed_and_ambiguous_diagrams() {
    let valid = html_viewer(&page(""));
    for html in [
        r#"<html><body>No diagram</body></html>"#.to_owned(),
        r#"<div class="mxgraph"></div>"#.to_owned(),
        r#"<div class="mxgraph" data-mxgraph="not JSON"></div>"#.to_owned(),
        r#"<div class="mxgraph" data-mxgraph='{"url":"https://example.invalid/diagram"}'></div>"#
            .to_owned(),
        r#"<div class="mxgraph" data-mxgraph='{"xml":42}'></div>"#.to_owned(),
        format!("{valid}{valid}"),
        format!(r#"<div class="mxgraph"></div>{valid}"#),
    ] {
        assert!(import_html(html.as_bytes()).is_err(), "{html}");
    }
    let remote =
        r#"<div class="mxgraph" data-mxgraph='{"url":"https://example.invalid/diagram"}'></div>"#;
    assert!(matches!(
        import_html(remote.as_bytes()),
        Err(IoError::DrawIoHtml(
            crate::DrawIoHtmlError::NoEmbeddedDiagram
        ))
    ));
    assert!(matches!(
        import_html(format!("{valid}{valid}").as_bytes()),
        Err(IoError::DrawIoHtml(crate::DrawIoHtmlError::MultipleViewers))
    ));
    assert!(matches!(
        import_html(b"<div>\xff</div>"),
        Err(IoError::DrawIoHtml(crate::DrawIoHtmlError::InvalidEncoding))
    ));
    let dtd = html_viewer(r#"<!DOCTYPE mxfile [<!ENTITY x SYSTEM "file:///secret">]><mxfile/>"#);
    assert!(import_html(dtd.as_bytes()).is_err());
}

#[test]
fn legacy_html_viewers_import_compressed_and_uri_encoded_multipage_diagrams() {
    let xml = r#"<mxfile><diagram name="First"><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="v" value="A &amp; B &quot;C&quot; ü" vertex="1" parent="1"><mxGeometry width="40" height="40" as="geometry"/></mxCell></root></mxGraphModel></diagram><diagram name="Second"><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/></root></mxGraphModel></diagram></mxfile>"#;
    let uri = percent_encode(xml);
    let compressed = base64::engine::general_purpose::STANDARD
        .encode(miniz_oxide::deflate::compress_to_vec(uri.as_bytes(), 6));
    let escaped_xml = xml
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");

    for data in [uri, compressed, escaped_xml] {
        let html = format!(
            r#"<!doctype html><DIV class="example mx&#103;raph"><section><DIV>{data}</DIV><div>preview only</div></section></DIV>"#
        );
        for name in ["legacy.html", "legacy.HTM", "without-extension"] {
            assert_eq!(
                detect_format(Path::new(name), html.as_bytes()),
                Some(Format::DrawIoHtml),
                "{name}"
            );
            let report =
                crate::import_named_bytes_with_report(Path::new(name), html.as_bytes()).unwrap();
            assert!(report.warnings.is_empty(), "{:?}", report.warnings);
            assert_eq!(report.workbook.sheets.len(), 2);
            assert_eq!(report.workbook.sheets[0].title, "First");
            assert_eq!(report.workbook.sheets[1].title, "Second");
            assert_eq!(
                shapes(&report.workbook.sheets[0])[0].label(),
                "A & B \"C\" ü"
            );
        }
    }
}

#[test]
fn legacy_html_viewer_imports_independently_encoded_bare_model() {
    let html = legacy_html_viewer(GOLDEN_COMPRESSED);
    let report = import_html(html.as_bytes()).unwrap();
    let sheet = &report.workbook.sheets[0];
    assert_eq!(shapes(sheet)[0].label(), "Start & go");
    assert_eq!(shapes(sheet)[1].label(), "Ünïcode");
}

#[test]
fn legacy_html_viewer_decodes_entities_across_streaming_chunks() {
    let xml = page(
        r#"<mxCell id="v" value="Chunk ü" vertex="1" parent="1"><mxGeometry width="40" height="40" as="geometry"/></mxCell>"#,
    );
    let data = percent_encode(&xml).replacen('%', "&#37;", 1);
    let viewer = legacy_html_viewer(&data);
    let entity = viewer.find("&#37;").unwrap();
    let html = format!("{}{viewer}", " ".repeat(16 * 1024 - entity - 2));
    let report = import_html(html.as_bytes()).unwrap();
    assert_eq!(shapes(&report.workbook.sheets[0])[0].label(), "Chunk ü");
}

#[test]
fn legacy_html_viewers_reject_ambiguous_or_malformed_markup() {
    let data = percent_encode(&page(""));
    let valid = legacy_html_viewer(&data);
    let current = html_viewer(&page(""));
    for html in [
        format!(r#"<div class="mxgraph"><div>{data}</div>"#),
        format!(r#"<div class="mxgraph"></div><div>{data}</div>"#),
        format!(r#"<div class="mxgraph"><div></div><div>{data}</div></div>"#),
        format!(r#"<div class="mxgraph"><div><span>{data}</span></div></div>"#),
        format!(r#"<div class="mxgraph"><div><!--comment-->{data}</div></div>"#),
        r#"<div class="mxgraph"><div>%ZZ</div></div>"#.to_owned(),
        r#"<div class="mxgraph"><div>not-compressed</div></div>"#.to_owned(),
        format!("{valid}{valid}"),
        format!("{current}{valid}"),
        format!(r#"<div class="mxgraph" data-mxgraph="invalid"><div>{data}</div></div>"#),
    ] {
        assert!(import_html(html.as_bytes()).is_err(), "{html}");
    }
    assert!(matches!(
        import_html(format!("{valid}{current}").as_bytes()),
        Err(IoError::DrawIoHtml(crate::DrawIoHtmlError::MultipleViewers))
    ));
    let dtd = percent_encode(r#"<!DOCTYPE mxfile [<!ENTITY x SYSTEM "file:///secret">]><mxfile/>"#);
    assert!(import_html(legacy_html_viewer(&dtd).as_bytes()).is_err());
}

#[test]
fn editable_image_exports_embed_only_the_selected_sheet() {
    let mut first = Sheet::new("First", "");
    first.root_mode = SheetRootMode::Hidden;
    first.board.grid = GridKind::Lines;
    first
        .add_canvas_object(shape(
            ShapeKind::Rectangle,
            rect(10.0, 20.0, 120.0, 60.0),
            "First page secret",
        ))
        .unwrap();
    let mut second = Sheet::new("Second", "");
    second.root_mode = SheetRootMode::Hidden;
    second.board.grid = GridKind::Lines;
    let label = "A < B & \"C\" \u{fc}";
    second
        .add_canvas_object(shape(
            ShapeKind::Ellipse,
            rect(40.0, 50.0, 100.0, 80.0),
            label,
        ))
        .unwrap();
    let mut book = Workbook::new(first);
    book.sheets.push(second);
    let opts = ExportOptions {
        sheet: Some(1),
        ..Default::default()
    };
    for (format, name) in [
        (Format::DrawIoSvg, "diagram.drawio.svg"),
        (Format::DrawIoPng, "diagram.drawio.png"),
    ] {
        assert_eq!(Format::from_path(Path::new(name)), Some(format));
        assert!(format.can_import() && format.can_export());
        assert!(export_losses(&book, format, &opts).is_empty());
        let bytes = export_bytes(&book, format, &opts).unwrap();
        assert_eq!(detect_format(Path::new(name), &bytes), Some(Format::DrawIo));
        if format == Format::DrawIoPng {
            let first_chunk_len = u32::from_be_bytes(bytes[8..12].try_into().unwrap()) as usize;
            let second_chunk = 8 + 12 + first_chunk_len;
            assert_eq!(&bytes[second_chunk + 4..second_chunk + 8], b"tEXt");
            assert_eq!(&bytes[second_chunk + 8..second_chunk + 15], b"mxfile\0");
            let preview = image::load_from_memory(&bytes).unwrap().to_rgba8();
            let plain = export_bytes(&book, Format::Png, &opts).unwrap();
            assert_eq!(preview, image::load_from_memory(&plain).unwrap().to_rgba8());
            let other = export_bytes(
                &book,
                Format::Png,
                &ExportOptions {
                    sheet: Some(0),
                    ..opts.clone()
                },
            )
            .unwrap();
            assert_ne!(preview, image::load_from_memory(&other).unwrap().to_rgba8());
        } else {
            let svg = std::str::from_utf8(&bytes).unwrap();
            resvg::usvg::Tree::from_str(svg, &Default::default()).unwrap();
            let (_, visible) = svg
                .strip_prefix("<svg content=\"")
                .unwrap()
                .split_once('"')
                .unwrap();
            let plain = export_bytes(&book, Format::Svg, &opts).unwrap();
            assert_eq!(format!("<svg{visible}").as_bytes(), plain);
            let other = export_bytes(
                &book,
                Format::Svg,
                &ExportOptions {
                    sheet: Some(0),
                    ..opts.clone()
                },
            )
            .unwrap();
            assert_ne!(plain, other);
        }
        let report = import(&bytes).unwrap();
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(report.workbook.sheets.len(), 1);
        assert_eq!(report.workbook.sheets[0].title, "Second");
        let objects = shapes(&report.workbook.sheets[0]);
        assert_eq!(objects.len(), 1);
        assert_eq!(objects[0].label(), label);
        assert!(!String::from_utf8_lossy(&bytes).contains("First page secret"));
    }
}

#[test]
fn editable_image_exports_reject_invalid_render_containers_and_oversized_output() {
    let sheet = Sheet::new("Board", "");
    assert!(embedded_image_fits(MAX_BYTES - 3, 2, 1));
    assert!(!embedded_image_fits(MAX_BYTES - 3, 3, 1));
    assert!(!embedded_image_fits(usize::MAX, 1, 0));
    for svg in [
        "",
        "<svg>",
        "<svg xmlns=\"x\"><g></svg>",
        "<svg content=\"existing\"></svg>",
        "<svg xmlns=\"x\"></svg><svg xmlns=\"x\"></svg>",
    ] {
        assert!(embed_svg(svg.to_owned(), &sheet).is_err(), "{svg}");
    }

    let png = image::DynamicImage::new_rgba8(1, 1);
    let mut buffer = std::io::Cursor::new(Vec::new());
    png.write_to(&mut buffer, image::ImageFormat::Png).unwrap();
    let png = buffer.into_inner();
    assert!(embed_png(png.clone(), &sheet).is_ok());
    for invalid in [
        vec![],
        PNG_SIGNATURE.to_vec(),
        [PNG_SIGNATURE, b"\0\0\0\0IEND\xaeB\x60\x82"].concat(),
        png[..png.len() - 1].to_vec(),
        {
            let mut corrupted = png.clone();
            corrupted[20] ^= 1;
            corrupted
        },
        {
            let mut trailing = png.clone();
            trailing.push(0);
            trailing
        },
    ] {
        assert!(embed_png(invalid, &sheet).is_err());
    }
    let oversized_svg = format!("<svg xmlns=\"x\">{}</svg>", " ".repeat(MAX_BYTES));
    assert!(
        embed_svg(oversized_svg, &sheet)
            .unwrap_err()
            .to_string()
            .contains("size limit")
    );
    let oversized_png = {
        let mut bytes = png;
        bytes.resize(MAX_BYTES + 1, 0);
        bytes
    };
    assert!(
        embed_png(oversized_png, &sheet)
            .unwrap_err()
            .to_string()
            .contains("size limit")
    );
}

#[test]
fn editable_image_guards_apply_to_the_selected_sheet_only() {
    let mut selected = Sheet::new("Selected", "");
    selected.root_mode = SheetRootMode::Hidden;
    selected.board.grid = GridKind::Lines;
    selected.board.snap = false;
    selected
        .add_canvas_object(shape(
            ShapeKind::Rectangle,
            rect(0.0, 0.0, 100.0, 60.0),
            "Selected",
        ))
        .unwrap();
    let mut book = Workbook::new(selected);
    book.sheets.push(Sheet::new("Other", "Root"));
    let selected_opts = ExportOptions {
        sheet: Some(0),
        ..Default::default()
    };
    let other_opts = ExportOptions {
        sheet: Some(1),
        ..Default::default()
    };

    let mut equations = book.clone();
    let sheet = &mut equations.sheets[1];
    let source = thinoda_core::equations::replace_note("", Some("x^{2}")).unwrap();
    sheet.set_markdown_notes(sheet.root, Some(source)).unwrap();

    let mut resources = book.clone();
    let sheet = &mut resources.sheets[1];
    let resource = Resource::new("picture.png", "image/png", tiny_png(4, 4)).unwrap();
    let root = sheet.root;
    images::set_image(sheet, root, Some((resource, Vec2::new(4.0, 4.0)))).unwrap();

    let mut symbols = book.clone();
    let sheet = &mut symbols.sheets[1];
    sheet
        .toggle_marker_color(&[sheet.root], "star-red", Some(Color::rgb(20, 40, 60)))
        .unwrap();

    let fim_xml = format!(
        "<x:xdatenfelder.datenfeld.0104 xmlns:x='{}'><x:header/><x:datenfeld><x:identifikation><x:id>F123</x:id><x:version>1.0.0</x:version></x:identifikation><x:name>Field</x:name></x:datenfeld></x:xdatenfelder.datenfeld.0104>",
        thinoda_core::fim::Dialect::Xdf3.namespace()
    );
    let mut fim = book.clone();
    fim.sheets[1] = thinoda_core::fim::FimDocument::parse(fim_xml.as_bytes())
        .unwrap()
        .into_sheet()
        .unwrap();

    for format in [Format::DrawIoSvg, Format::DrawIoPng] {
        for case in [&equations, &resources, &symbols, &fim] {
            assert!(
                crate::export_bytes_with_consent(case, format, &selected_opts, true).is_ok(),
                "{format:?}: unselected sheet blocked export"
            );
        }
        assert!(matches!(
            crate::export_bytes_with_consent(&equations, format, &other_opts, true),
            Err(IoError::EquationNotesUnsupported)
        ));
        assert!(
            crate::export_bytes_with_consent(&resources, format, &other_opts, true)
                .unwrap_err()
                .to_string()
                .contains("embedded resources")
        );
        assert!(!crate::symbol_color_loss_with_options(
            &symbols,
            format,
            &selected_opts
        ));
        assert!(crate::symbol_color_loss_with_options(
            &symbols,
            format,
            &other_opts
        ));
        assert!(
            crate::export_bytes_with_consent(&symbols, format, &other_opts, true)
                .unwrap_err()
                .to_string()
                .contains("symbol colors")
        );
        let accepted = ExportOptions {
            allow_symbol_color_loss: true,
            ..other_opts.clone()
        };
        assert!(crate::export_bytes_with_consent(&symbols, format, &accepted, true).is_ok());
        assert!(
            crate::export_bytes_with_consent(&fim, format, &other_opts, true)
                .unwrap_err()
                .to_string()
                .contains("FIM")
        );
    }

    assert!(matches!(
        crate::export_bytes_with_consent(&equations, Format::DrawIo, &selected_opts, true),
        Err(IoError::EquationNotesUnsupported)
    ));
}

#[test]
fn nested_connector_waypoints_import_and_round_trip_in_parent_coordinates() {
    let xml = page(
        r#"
        <mxCell id="g" style="group" vertex="1" parent="1"><mxGeometry x="100" y="200" width="300" height="200" as="geometry"/></mxCell>
        <mxCell id="a" value="A" vertex="1" parent="g"><mxGeometry width="20" height="20" as="geometry"/></mxCell>
        <mxCell id="e" value="Route" style="edgeStyle=none;endArrow=none;" edge="1" parent="g"><mxGeometry relative="1" as="geometry"><mxPoint x="50" y="10" as="sourcePoint"/><mxPoint x="150" y="90" as="targetPoint"/><Array as="points"><mxPoint x="80" y="10"/><mxPoint x="80" y="90"/></Array></mxGeometry></mxCell>
        "#,
    );
    let sheet = clean(&xml);
    let connector = connectors(&sheet)[0];
    assert_eq!(connector.from, Endpoint::Point(Vec2::new(150.0, 210.0)));
    assert_eq!(connector.to, Endpoint::Point(Vec2::new(250.0, 290.0)));
    assert_eq!(
        connector.waypoints.as_ref(),
        &[Vec2::new(180.0, 210.0), Vec2::new(180.0, 290.0)]
    );
    let restored = roundtrip(&Workbook::new(sheet));
    let connector = connectors(&restored.sheets[0])[0];
    assert_eq!(connector.from, Endpoint::Point(Vec2::new(150.0, 210.0)));
    assert_eq!(
        connector.waypoints.as_ref(),
        &[Vec2::new(180.0, 210.0), Vec2::new(180.0, 290.0)]
    );
}

#[test]
fn malformed_waypoints_are_reported_and_do_not_create_fabricated_points() {
    let xml = page(
        r#"
        <mxCell id="e" style="edgeStyle=none;endArrow=none;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/><Array as="points"><mxPoint x="5" y="6"/><mxPoint x="NaN" y="3"/><mxPoint x="5" y="invalid"/><mxPoint y="9"/></Array></mxGeometry></mxCell>
        "#,
    );
    let report = load(&xml);
    assert_eq!(report.warnings, vec![ImportWarning::Other]);
    assert_eq!(
        connectors(&report.workbook.sheets[0])[0].waypoints.as_ref(),
        &[Vec2::new(5.0, 6.0), Vec2::new(0.0, 9.0)]
    );
}

#[test]
fn curved_waypoints_round_trip_and_orthogonal_hints_report_approximation() {
    let cell = |style: &str| {
        format!(
            r#"<mxCell id="e" style="{style}" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint x="0" y="0" as="sourcePoint"/><mxPoint x="200" y="0" as="targetPoint"/><Array as="points"><mxPoint x="50" y="80"/><mxPoint x="150" y="80"/></Array></mxGeometry></mxCell>"#
        )
    };
    let curved = clean(&page(&cell("curved=1;edgeStyle=none;endArrow=none;")));
    let connector = connectors(&curved)[0];
    assert_eq!(connector.route, ConnectorRoute::Curve);
    let expected = [Vec2::new(50.0, 80.0), Vec2::new(150.0, 80.0)];
    assert_eq!(connector.waypoints.as_ref(), &expected);
    let restored = roundtrip(&Workbook::new(curved));
    let connector = connectors(&restored.sheets[0])[0];
    assert_eq!(connector.route, ConnectorRoute::Curve);
    assert_eq!(connector.waypoints.as_ref(), &expected);

    let report = load(&page(&cell("edgeStyle=orthogonalEdgeStyle;endArrow=none;")));
    assert_eq!(report.warnings, vec![ImportWarning::Formatting]);
    let connector = connectors(&report.workbook.sheets[0])[0];
    assert_eq!(connector.route, ConnectorRoute::Elbow);
    assert_eq!(connector.waypoints.as_ref(), &expected);
    let bytes = export(&report.workbook).unwrap();
    let restored = import(&bytes).unwrap();
    assert_eq!(restored.warnings, vec![ImportWarning::Formatting]);
    assert_eq!(
        connectors(&restored.workbook.sheets[0])[0]
            .waypoints
            .as_ref(),
        &expected
    );
}

#[test]
fn connector_marker_and_port_approximations_require_confirmation() {
    let edge = |style: &str| {
        page(&format!(
            r#"<mxCell id="e" style="{style}" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"#
        ))
    };
    for style in [
        "endArrow=baseDash;",
        "startFill=0;",
        "endArrow=none;endFill=0;",
        "endArrow=baseDash;endFill=0;",
        "endFill=invalid;",
        "endFill=2;",
        "startSize=12;",
        "endSize=invalid;",
        "endSize=-1;",
        "endSize=1000;",
        "endSize=NaN;",
        "endArrow=none;endSize=12;",
        "exitX=0.25;",
        "exitY=0.5;",
        "exitDx=5;",
        "exitDy=5;",
        "exitPerimeter=0;",
        "entryX=0.75;",
        "entryY=0.5;",
        "entryDx=5;",
        "entryDy=5;",
        "entryPerimeter=0;",
        "sourcePort=port1;",
        "targetPort=port2;",
        "sourcePortConstraint=north;",
        "targetPortConstraint=south;",
        "sourcePerimeterSpacing=5;",
        "targetPerimeterSpacing=5;",
    ] {
        let report = load(&edge(style));
        assert_eq!(report.warnings, vec![ImportWarning::Formatting], "{style}");
        assert_eq!(connectors(&report.workbook.sheets[0]).len(), 1, "{style}");
    }

    for style in [
        "",
        "startArrow=none;endArrow=none;",
        "startArrow=classic;endArrow=classic;",
        "startFill=1;endFill=1;startSize=6;endSize=6;",
        "endFill=0;",
        "startArrow=block;startFill=0;endArrow=open;endFill=0;",
        "startArrow=block;startSize=12.5;endSize=0;",
        "startArrow=diamond;startFill=0;endArrow=diamondThin;endFill=1;",
        "startArrow=oval;startFill=0;endArrow=oval;endFill=1;",
        "startArrow=halfCircle;startFill=0;endArrow=halfCircle;endFill=1;",
        "portConstraintRotation=0;perimeterSpacing=0;sourcePerimeterSpacing=0;targetPerimeterSpacing=0;",
    ] {
        let report = load(&edge(style));
        assert!(report.warnings.is_empty(), "{style}: {:?}", report.warnings);
        assert_eq!(connectors(&report.workbook.sheets[0]).len(), 1, "{style}");
        let restored = import(&export(&report.workbook).unwrap()).unwrap();
        assert!(
            restored.warnings.is_empty(),
            "{style}: {:?}",
            restored.warnings
        );
    }
}

#[test]
fn marker_fills_round_trip_independently_for_every_supported_shape() {
    let names = [
        "classic",
        "classicThin",
        "open",
        "openThin",
        "block",
        "blockThin",
        "diamond",
        "diamondThin",
        "oval",
        "halfCircle",
    ];
    for kind in names {
        for start_fill in [0, 1] {
            for end_fill in [0, 1] {
                let xml = page(&format!(
                    r#"<mxCell id="e" style="startArrow={kind};startSize=12.5;startFill={start_fill};endArrow={kind};endSize=0;endFill={end_fill};" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"#
                ));
                let report = load(&xml);
                assert!(
                    report.warnings.is_empty(),
                    "{kind}, {start_fill}, {end_fill}: {:?}",
                    report.warnings
                );
                let connector = connectors(&report.workbook.sheets[0])[0];
                assert_eq!(connector.start_marker_unfilled, start_fill == 0);
                assert_eq!(connector.end_marker_unfilled, end_fill == 0);
                assert_eq!(connector.start_marker_size, Some(12.5));
                assert_eq!(connector.end_marker_size, Some(0.0));
                assert_eq!(
                    crate::required_native_version(&report.workbook),
                    if kind == "halfCircle" {
                        17
                    } else if kind == "oval" {
                        16
                    } else if kind.starts_with("diamond") {
                        15
                    } else if start_fill == 0 || end_fill == 0 {
                        14
                    } else {
                        13
                    }
                );
                let exported = export(&report.workbook).unwrap();
                let restored = import(&exported).unwrap();
                assert!(
                    restored.warnings.is_empty(),
                    "{kind}: {:?}",
                    restored.warnings
                );
                let restored = connectors(&restored.workbook.sheets[0])[0];
                assert_eq!(
                    restored.start_marker_unfilled,
                    connector.start_marker_unfilled
                );
                assert_eq!(restored.end_marker_unfilled, connector.end_marker_unfilled);
                assert_eq!(restored.start_marker_size, connector.start_marker_size);
                assert_eq!(restored.end_marker_size, connector.end_marker_size);
                assert_eq!(restored.start_marker, connector.start_marker);
                assert_eq!(restored.end_marker, connector.end_marker);
            }
        }
    }
}

#[test]
fn custom_marker_sizes_round_trip_for_every_supported_shape() {
    let names = [
        "classic",
        "classicThin",
        "open",
        "openThin",
        "block",
        "blockThin",
        "diamond",
        "diamondThin",
        "oval",
        "halfCircle",
    ];
    for kind in names {
        for size in [0.0, 6.0, 12.5, 999.0] {
            let xml = page(&format!(
                r#"<mxCell id="e" style="startArrow={kind};startSize={size};endArrow={kind};endSize=12.5;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"#
            ));
            let report = load(&xml);
            assert!(
                report.warnings.is_empty(),
                "{kind}, {size}: {:?}",
                report.warnings
            );
            let connector = connectors(&report.workbook.sheets[0])[0];
            assert_eq!(connector.start_marker_size, (size != 6.0).then_some(size));
            assert_eq!(connector.end_marker_size, Some(12.5));
            assert_eq!(
                crate::required_native_version(&report.workbook),
                if kind == "halfCircle" {
                    17
                } else if kind == "oval" {
                    16
                } else if kind.starts_with("diamond") {
                    15
                } else {
                    13
                }
            );
            let exported = export(&report.workbook).unwrap();
            let restored = import(&exported).unwrap();
            assert!(
                restored.warnings.is_empty(),
                "{kind}, {size}: {:?}",
                restored.warnings
            );
            let restored = connectors(&restored.workbook.sheets[0])[0];
            assert_eq!(restored.start_marker_size, connector.start_marker_size);
            assert_eq!(restored.end_marker_size, connector.end_marker_size);
            assert_eq!(restored.start_marker, connector.start_marker);
            assert_eq!(restored.end_marker, connector.end_marker);
        }
    }
}

#[test]
fn supported_connector_markers_round_trip_with_both_ends() {
    let marker = |name| match name {
        "classic" => Some(ConnectorMarkerKind::Classic),
        "classicThin" => Some(ConnectorMarkerKind::ClassicThin),
        "open" => Some(ConnectorMarkerKind::Open),
        "openThin" => Some(ConnectorMarkerKind::OpenThin),
        "block" => Some(ConnectorMarkerKind::Block),
        "blockThin" => Some(ConnectorMarkerKind::BlockThin),
        "diamond" => Some(ConnectorMarkerKind::Diamond),
        "diamondThin" => Some(ConnectorMarkerKind::DiamondThin),
        "oval" => Some(ConnectorMarkerKind::Oval),
        "halfCircle" => Some(ConnectorMarkerKind::HalfCircle),
        _ => None,
    };
    let names = [
        "none",
        "classic",
        "classicThin",
        "open",
        "openThin",
        "block",
        "blockThin",
        "diamond",
        "diamondThin",
        "oval",
        "halfCircle",
    ];
    for start in names {
        for end in names {
            let xml = page(&format!(
                r#"<mxCell id="e" style="startArrow={start};endArrow={end};startSize=6;endSize=6;startFill=1;endFill=1;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/><Array as="points"><mxPoint x="50" y="20"/></Array></mxGeometry></mxCell>"#
            ));
            let report = load(&xml);
            assert!(
                report.warnings.is_empty(),
                "{start}, {end}: {:?}",
                report.warnings
            );
            let connector = connectors(&report.workbook.sheets[0])[0];
            assert_eq!(connector.arrow_start, start != "none");
            assert_eq!(connector.arrow_end, end != "none");
            assert_eq!(
                connector.start_marker.map(|value| value.kind),
                marker(start)
            );
            assert_eq!(connector.end_marker.map(|value| value.kind), marker(end));
            let native_version = crate::required_native_version(&report.workbook);
            assert_eq!(
                native_version,
                if [marker(start), marker(end)]
                    .into_iter()
                    .flatten()
                    .any(|kind| kind == ConnectorMarkerKind::HalfCircle)
                {
                    17
                } else if [marker(start), marker(end)]
                    .into_iter()
                    .flatten()
                    .any(|kind| kind == ConnectorMarkerKind::Oval)
                {
                    16
                } else if [marker(start), marker(end)]
                    .into_iter()
                    .flatten()
                    .any(ConnectorMarkerKind::is_diamond)
                {
                    15
                } else if [marker(start), marker(end)]
                    .into_iter()
                    .flatten()
                    .any(ConnectorMarkerKind::requires_extended_format)
                {
                    12
                } else if marker(start).is_some() || marker(end).is_some() {
                    11
                } else {
                    10
                }
            );
            let exported = export(&report.workbook).unwrap();
            let exported_text = std::str::from_utf8(&exported).unwrap();
            assert!(exported_text.contains(&format!("endArrow={end};")));
            if start != "none" {
                assert!(exported_text.contains(&format!("startArrow={start};")));
            }
            let restored = import(&exported).unwrap();
            assert!(restored.warnings.is_empty(), "{:?}", restored.warnings);
            let restored = connectors(&restored.workbook.sheets[0])[0];
            assert_eq!(restored.start_marker.map(|value| value.kind), marker(start));
            assert_eq!(restored.end_marker.map(|value| value.kind), marker(end));
            assert_eq!(restored.arrow_start, start != "none");
            assert_eq!(restored.arrow_end, end != "none");
        }
    }
}

#[test]
fn drawio_default_classic_and_legacy_triangle_are_distinct() {
    let cell = |style: &str| {
        page(&format!(
            r#"<mxCell id="e" style="{style}" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"#
        ))
    };
    for style in ["", "defaultEdge", "endArrow=classic;"] {
        let report = load(&cell(style));
        let connector = connectors(&report.workbook.sheets[0])[0];
        assert_eq!(
            connector.end_marker.unwrap().kind,
            ConnectorMarkerKind::Classic,
            "{style}"
        );
        assert_eq!(crate::required_native_version(&report.workbook), 12);
    }
    let no_default = load(&cell(";"));
    let connector = connectors(&no_default.workbook.sheets[0])[0];
    assert!(!connector.arrow_end);
    assert!(connector.end_marker.is_none());

    let mut sheet = Sheet::new("Legacy", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut connector = CanvasConnector::new(
        Endpoint::Point(Vec2::ZERO),
        Endpoint::Point(Vec2::new(100.0, 0.0)),
    );
    connector.arrow_start = true;
    sheet
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    let book = Workbook::new(sheet);
    assert_eq!(
        export_losses(&book, Format::DrawIo, &ExportOptions::default()),
        vec![ExportLoss::LegacyConnectorMarkersApproximated(2)]
    );
    let xml = String::from_utf8(export(&book).unwrap()).unwrap();
    assert!(xml.contains("thinodaLegacyStartArrow=1;"));
    assert!(xml.contains("thinodaLegacyEndArrow=1;"));
    let restored = load(&xml);
    let connector = connectors(&restored.workbook.sheets[0])[0];
    assert!(connector.arrow_start && connector.arrow_end);
    assert!(connector.start_marker.is_none() && connector.end_marker.is_none());

    let changed = xml.replace("endArrow=classic;", "endArrow=block;");
    let restored = load(&changed);
    let connector = connectors(&restored.workbook.sheets[0])[0];
    assert_eq!(
        connector.end_marker.unwrap().kind,
        ConnectorMarkerKind::Block
    );
    let resized = xml.replace(
        "thinodaLegacyEndArrow=1;",
        "thinodaLegacyEndArrow=1;endSize=12;",
    );
    let restored = load(&resized);
    assert!(restored.warnings.is_empty(), "{:?}", restored.warnings);
    let connector = connectors(&restored.workbook.sheets[0])[0];
    assert_eq!(
        connector.end_marker.unwrap().kind,
        ConnectorMarkerKind::Classic
    );
    assert_eq!(connector.end_marker_size, Some(12.0));
}

#[test]
fn supported_connector_markers_survive_link_and_clipboard_conversion() {
    for (start, end, start_kind, end_kind) in [
        (
            "block",
            "open",
            ConnectorMarkerKind::Block,
            ConnectorMarkerKind::Open,
        ),
        (
            "halfCircle",
            "oval",
            ConnectorMarkerKind::HalfCircle,
            ConnectorMarkerKind::Oval,
        ),
    ] {
        let xml = page(&format!(
            r#"<mxCell id="e" style="startArrow={start};startSize=12.5;startFill=0;endArrow={end};endSize=0;endFill=0;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"#
        ));
        let link = format!("#R{}", crate::drawio_clipboard::encode_uri_component(&xml));
        let linked = crate::import_drawio_link(&link).unwrap();
        assert!(linked.warnings.is_empty(), "{:?}", linked.warnings);
        let connector = connectors(&linked.workbook.sheets[0])[0];
        assert_eq!(connector.start_marker.unwrap().kind, start_kind);
        assert_eq!(connector.end_marker.unwrap().kind, end_kind);
        assert_eq!(connector.start_marker_size, Some(12.5));
        assert_eq!(connector.end_marker_size, Some(0.0));
        assert!(connector.start_marker_unfilled);
        assert!(connector.end_marker_unfilled);

        let pasted = crate::drawio_clipboard::decode(&xml).unwrap();
        assert!(pasted.warnings.is_empty(), "{:?}", pasted.warnings);
        let assert_markers = |payload: &thinoda_core::clipboard::BoardClipboardPayload| {
            let mut sheet = Sheet::new("Paste", "");
            sheet.root_mode = SheetRootMode::Hidden;
            thinoda_core::clipboard::paste_board_selection(&mut sheet, payload, Vec2::ZERO)
                .unwrap();
            let connector = connectors(&sheet)[0];
            assert_eq!(connector.start_marker.unwrap().kind, start_kind);
            assert_eq!(connector.end_marker.unwrap().kind, end_kind);
            assert_eq!(connector.start_marker_size, Some(12.5));
            assert_eq!(connector.end_marker_size, Some(0.0));
            assert!(connector.start_marker_unfilled);
            assert!(connector.end_marker_unfilled);
        };
        assert_markers(&pasted.payload);
        let copy = crate::drawio_clipboard::encode(&pasted.payload).unwrap();
        let restored = crate::drawio_clipboard::decode(&copy).unwrap();
        assert_markers(&restored.payload);
    }
}

#[test]
fn editable_drawio_images_preserve_supported_connector_markers() {
    for (start, end, start_kind, end_kind, start_unfilled, end_unfilled) in [
        (
            "block",
            "open",
            ConnectorMarkerKind::Block,
            ConnectorMarkerKind::Open,
            true,
            false,
        ),
        (
            "classicThin",
            "openThin",
            ConnectorMarkerKind::ClassicThin,
            ConnectorMarkerKind::OpenThin,
            false,
            true,
        ),
        (
            "diamond",
            "diamondThin",
            ConnectorMarkerKind::Diamond,
            ConnectorMarkerKind::DiamondThin,
            true,
            false,
        ),
        (
            "oval",
            "oval",
            ConnectorMarkerKind::Oval,
            ConnectorMarkerKind::Oval,
            true,
            false,
        ),
        (
            "halfCircle",
            "oval",
            ConnectorMarkerKind::HalfCircle,
            ConnectorMarkerKind::Oval,
            true,
            false,
        ),
    ] {
        let report = load(&page(&format!(
            r#"<mxCell id="e" style="startArrow={start};startSize=12.5;startFill={};endArrow={end};endSize=0;endFill={};" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"#,
            u8::from(!start_unfilled),
            u8::from(!end_unfilled),
        )));
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        for format in [Format::DrawIoSvg, Format::DrawIoPng] {
            let bytes = export_bytes(&report.workbook, format, &ExportOptions::default()).unwrap();
            let restored = import(&bytes).unwrap();
            assert!(
                restored.warnings.is_empty(),
                "{format:?}: {:?}",
                restored.warnings
            );
            let connector = connectors(&restored.workbook.sheets[0])[0];
            assert_eq!(connector.start_marker.unwrap().kind, start_kind);
            assert_eq!(connector.end_marker.unwrap().kind, end_kind);
            assert_eq!(connector.start_marker_size, Some(12.5));
            assert_eq!(connector.end_marker_size, Some(0.0));
            assert_eq!(connector.start_marker_unfilled, start_unfilled);
            assert_eq!(connector.end_marker_unfilled, end_unfilled);
        }
    }
}

#[test]
fn shape_connection_constraints_report_approximation() {
    for style in [
        "portConstraint=north;",
        "portConstraintRotation=1;",
        "perimeterSpacing=5;",
    ] {
        let xml = page(&format!(
            r#"<mxCell id="v" style="{style}" vertex="1" parent="1"><mxGeometry width="40" height="40" as="geometry"/></mxCell>"#
        ));
        let report = load(&xml);
        assert_eq!(report.warnings, vec![ImportWarning::Formatting], "{style}");
        assert_eq!(shapes(&report.workbook.sheets[0]).len(), 1);
    }
}

#[test]
fn empty_arrow_names_disable_the_corresponding_marker() {
    let xml = page(
        r#"<mxCell id="e" style="startArrow=;endArrow=;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"#,
    );
    let report = load(&xml);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let connector = connectors(&report.workbook.sheets[0])[0];
    assert!(!connector.arrow_start);
    assert!(!connector.arrow_end);
    let restored = import(&export(&report.workbook).unwrap()).unwrap();
    assert!(restored.warnings.is_empty(), "{:?}", restored.warnings);
    assert!(!connectors(&restored.workbook.sheets[0])[0].arrow_end);
}

#[test]
fn edge_default_suppression_and_named_default_control_the_end_marker() {
    for (style, arrow) in [
        ("", true),
        (";edgeStyle=none;", false),
        (";endArrow=classic;", true),
        (";defaultEdge;", true),
        (";defaultEdge;endArrow=none;", false),
        (";endArrow=none;defaultEdge;", true),
    ] {
        let xml = page(&format!(
            r#"<mxCell id="e" style="{style}" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"#
        ));
        let report = load(&xml);
        assert!(report.warnings.is_empty(), "{style}: {:?}", report.warnings);
        assert_eq!(
            connectors(&report.workbook.sheets[0])[0].arrow_end,
            arrow,
            "{style}"
        );
        let restored = import(&export(&report.workbook).unwrap()).unwrap();
        assert!(
            restored.warnings.is_empty(),
            "{style}: {:?}",
            restored.warnings
        );
        assert_eq!(
            connectors(&restored.workbook.sheets[0])[0].arrow_end,
            arrow,
            "{style}"
        );
    }
}

#[test]
fn attached_connectors_warn_before_marker_and_port_data_is_discarded() {
    let xml = page(
        r#"<mxCell id="a" vertex="1" parent="1"><mxGeometry x="10" y="20" width="60" height="40" as="geometry"/></mxCell>
        <mxCell id="b" vertex="1" parent="1"><mxGeometry x="200" y="20" width="60" height="40" as="geometry"/></mxCell>
        <mxCell id="e" style="endArrow=open;endSize=14;exitX=1;exitY=0.5;entryX=0;entryY=0.25;" edge="1" source="a" target="b" parent="1"><mxGeometry relative="1" as="geometry"/></mxCell>"#,
    );
    let report = load(&xml);
    assert_eq!(report.warnings, vec![ImportWarning::Formatting]);
    report.workbook.validate().unwrap();
    let connector = connectors(&report.workbook.sheets[0])[0];
    assert!(matches!(connector.from, Endpoint::Object(_)));
    assert!(matches!(connector.to, Endpoint::Object(_)));
    assert!(connector.arrow_end);
    let paste = crate::drawio_clipboard::decode(&xml).unwrap();
    assert_eq!(paste.warnings, vec![ImportWarning::Formatting]);
    let link = format!("#R{}", crate::drawio_clipboard::encode_uri_component(&xml));
    let linked = crate::import_drawio_link(&link).unwrap();
    assert_eq!(linked.warnings, vec![ImportWarning::Formatting]);

    let exported = String::from_utf8(export(&report.workbook).unwrap()).unwrap();
    assert!(exported.contains("endArrow=open;"));
    assert!(!exported.contains("exitX="));
    assert!(!exported.contains("entryX="));
}

#[test]
fn ink_edges_report_unsupported_connector_appearance() {
    let edge = |style: &str| {
        page(&format!(
            r#"<mxCell id="e" style="thinodaInk=1;{style}" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"#
        ))
    };
    for style in [
        "",
        "endArrow=open;",
        "endArrow=classic;",
        "endArrow=none;startArrow=classic;",
        "endArrow=none;exitX=0.5;",
    ] {
        let report = load(&edge(style));
        assert_eq!(report.warnings, vec![ImportWarning::Formatting], "{style}");
    }
    let report = load(&edge("endArrow=none;"));
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let report = load(&edge("endArrow=;"));
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let suppressed = page(
        r#"<mxCell id="e" style=";thinodaInk=1;" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/></mxGeometry></mxCell>"#,
    );
    let report = load(&suppressed);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
}

#[test]
fn excessive_connector_waypoints_are_reported() {
    let points =
        r#"<mxPoint x="10" y="20"/>"#.repeat(thinoda_core::board::MAX_CONNECTOR_WAYPOINTS + 1);
    let xml = page(&format!(
        r#"<mxCell id="e" edge="1" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint as="sourcePoint"/><mxPoint x="100" as="targetPoint"/><Array as="points">{points}</Array></mxGeometry></mxCell>"#
    ));
    let report = load(&xml);
    assert_eq!(report.warnings, vec![ImportWarning::Other]);
    assert!(connectors(&report.workbook.sheets[0]).is_empty());
}

#[test]
fn waypoint_connector_attachments_export_at_path_midpoint_with_loss_warning() {
    let mut sheet = Sheet::new("Attachments", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut target = CanvasConnector::new(
        Endpoint::Point(Vec2::ZERO),
        Endpoint::Point(Vec2::new(250.0, 100.0)),
    );
    target.waypoints = vec![Vec2::new(200.0, 0.0), Vec2::new(200.0, 100.0)].into();
    target.label = "Target".into();
    let target_id = sheet
        .add_canvas_object(CanvasObject::connector(target))
        .unwrap();
    let mut dependent = CanvasConnector::new(
        Endpoint::Object(target_id),
        Endpoint::Point(Vec2::new(300.0, 200.0)),
    );
    dependent.label = "Dependent".into();
    sheet
        .add_canvas_object(CanvasObject::connector(dependent))
        .unwrap();
    let book = Workbook::new(sheet);
    for format in [Format::DrawIo, Format::DrawIoSvg, Format::DrawIoPng] {
        assert_eq!(
            export_losses(&book, format, &ExportOptions::default()),
            vec![
                ExportLoss::ConnectorAttachmentsDetached(1),
                ExportLoss::LegacyConnectorMarkersApproximated(2),
            ]
        );
        assert!(matches!(
            export_bytes(&book, format, &ExportOptions::default()),
            Err(IoError::DrawioConnectorAttachmentsNeedConfirmation(1))
        ));
        let bytes =
            crate::export_bytes_with_consent(&book, format, &ExportOptions::default(), true)
                .unwrap();
        let restored = import(&bytes).unwrap().workbook;
        let dependent = connectors(&restored.sheets[0])
            .into_iter()
            .find(|connector| connector.label == "Dependent")
            .unwrap();
        assert_eq!(dependent.from, Endpoint::Point(Vec2::new(175.0, 0.0)));
    }
}

#[test]
fn nested_geometry_groups_containers_and_edge_labels() {
    let xml = page(
        r#"
        <mxCell id="g" style="group" vertex="1" connectable="0" parent="1"><mxGeometry x="100" y="100" width="200" height="100" as="geometry"/></mxCell>
        <mxCell id="a" value="A" vertex="1" parent="g"><mxGeometry x="0" y="0" width="50" height="50" as="geometry"/></mxCell>
        <mxCell id="b" value="B" style="ellipse;" vertex="1" parent="g"><mxGeometry x="150" y="50" width="50" height="50" as="geometry"/></mxCell>
        <mxCell id="e" value="" style="endArrow=none;" edge="1" parent="1" source="a" target="later"><mxGeometry relative="1" as="geometry"/></mxCell>
        <mxCell id="l" value="edge label" style="edgeLabel;" vertex="1" connectable="0" parent="e"><mxGeometry x="-0.2" relative="1" as="geometry"/></mxCell>
        <mxCell id="lane" value="&lt;b&gt;&lt;/b&gt;Pool&lt;br&gt;" style="swimlane;html=1;startSize=20;" vertex="1" parent="1"><mxGeometry x="500" y="0" width="300" height="200" as="geometry"/></mxCell>
        <mxCell id="later" value="Inside" vertex="1" parent="lane"><mxGeometry x="20" y="40" width="60" height="30" as="geometry"/></mxCell>
        <mxCell id="pin" value="pin" style="ellipse;" vertex="1" parent="later"><mxGeometry x="1" y="0.5" width="10" height="10" relative="1" as="geometry"><mxPoint x="-5" y="-5" as="offset"/></mxGeometry></mxCell>
        <mxCell id="free" edge="1" parent="lane"><mxGeometry relative="1" as="geometry"><mxPoint x="0" y="0" as="sourcePoint"/><mxPoint x="10" y="10" as="targetPoint"/></mxGeometry></mxCell>
        "#,
    );
    let report = load(&xml);
    // `<b>` is formatting without a native equivalent.
    assert_eq!(report.warnings, vec![ImportWarning::Formatting]);
    let sheet = &report.workbook.sheets[0];
    let find = |label: &str| {
        shapes(sheet)
            .into_iter()
            .find(|o| o.label() == label)
            .unwrap_or_else(|| panic!("{label}"))
    };
    assert_eq!(find("A").rect, rect(100.0, 100.0, 50.0, 50.0));
    assert_eq!(find("B").rect, rect(250.0, 150.0, 50.0, 50.0));
    assert_eq!(find("Inside").rect, rect(520.0, 40.0, 60.0, 30.0));
    assert_eq!(find("pin").rect, rect(575.0, 50.0, 10.0, 10.0));
    let a = find("A").id;
    let b = find("B").id;
    let inside = find("Inside").id;
    let pin = find("pin").id;
    assert_eq!(sheet.canvas.groups.len(), 2);
    let groups: Vec<Vec<BoardMember>> = sheet
        .canvas
        .groups
        .iter()
        .map(|g| sheet.board_group_members(g.id))
        .collect();
    assert!(groups.contains(&vec![BoardMember::Object(a), BoardMember::Object(b)]));
    assert!(groups.contains(&vec![BoardMember::Object(inside), BoardMember::Object(pin)]));
    let zone = &sheet.zones[0];
    assert_eq!(zone.name, "Pool");
    assert_eq!(zone.objects.len(), 3, "inside, pin and the free edge");
    let links = connectors(sheet);
    let deferred = links.iter().find(|c| c.label == "edge label").unwrap();
    assert_eq!(
        (deferred.from, deferred.to),
        (Endpoint::Object(a), Endpoint::Object(inside))
    );
    assert!(!deferred.arrow_end);
    let free = links.iter().find(|c| c.label.is_empty()).unwrap();
    assert_eq!(free.from, Endpoint::Point(Vec2::new(500.0, 0.0)));
    assert_eq!(free.to, Endpoint::Point(Vec2::new(510.0, 10.0)));
}

#[test]
fn notes_text_lines_directions_and_user_objects() {
    let xml = page(
        r#"
        <mxCell id="n" value="Note" style="shape=note;fillColor=#fff2cc;" vertex="1" parent="1"><mxGeometry x="0" y="0" width="100" height="80" as="geometry"/></mxCell>
        <mxCell id="t" value="Label" style="text;html=1;" vertex="1" parent="1"><mxGeometry x="0" y="100" width="60" height="20" as="geometry"/></mxCell>
        <mxCell id="ln" style="line;strokeWidth=2;direction=south;" vertex="1" parent="1"><mxGeometry x="0" y="200" width="10" height="100" as="geometry"/></mxCell>
        <mxCell id="tri" style="triangle;direction=south;" vertex="1" parent="1"><mxGeometry x="0" y="400" width="60" height="40" as="geometry"/></mxCell>
        <UserObject label="Linked" tooltip="Tip" link="https://example.com" id="u"><mxCell style="shape=note;" vertex="1" parent="1"><mxGeometry x="300" y="0" width="100" height="80" as="geometry"/></mxCell></UserObject>
        "#,
    );
    let report = load(&xml);
    // The card tooltip has no native equivalent.
    assert_eq!(report.warnings, vec![ImportWarning::Other]);
    let sheet = &report.workbook.sheets[0];
    let note = sheet.topics.values().find(|t| t.title == "Note").unwrap();
    let card = note.card.unwrap();
    assert_eq!(card.kind, CardKind::Sticky);
    assert_eq!(card.color, Some(Color::rgb(0xFF, 0xF2, 0xCC)));
    assert_eq!(note.position, Some(Vec2::new(50.0, 40.0)));
    let linked = sheet.topics.values().find(|t| t.title == "Linked").unwrap();
    assert_eq!(linked.hyperlink.as_deref(), Some("https://example.com"));
    let label = shapes(sheet)
        .into_iter()
        .find(|o| o.label() == "Label")
        .unwrap();
    assert_eq!(label.style.fill, Some(Color::TRANSPARENT));
    let tri = shapes(sheet)
        .into_iter()
        .find(|o| {
            matches!(
                o.kind,
                CanvasObjectKind::Shape {
                    shape: ShapeKind::Triangle,
                    ..
                }
            )
        })
        .unwrap();
    // draw.io triangles point east and Thinoda's point north; south turns them down.
    assert_eq!(tri.rotation, 180.0);
    assert_eq!(tri.rect, rect(0.0, 400.0, 60.0, 40.0));
    let line = connectors(sheet)[0];
    assert_eq!(line.from, Endpoint::Point(Vec2::new(5.0, 200.0)));
    assert_eq!(line.to, Endpoint::Point(Vec2::new(5.0, 300.0)));
    assert!(!line.arrow_end);
}

#[test]
fn unsupported_content_is_reported_not_dropped_silently() {
    let xml = page(
        r#"
        <mxCell id="d" value="Dashed" style="rounded=0;dashed=1;" vertex="1" parent="1"><mxGeometry width="10" height="10" as="geometry"/></mxCell>
        <mxCell id="k" value="Unknown" style="shape=mxgraph.aws4.lambda;" vertex="1" parent="1"><mxGeometry y="20" width="10" height="10" as="geometry"/></mxCell>
        <mxCell id="i" style="shape=image;image=https://example.com/a.png;" vertex="1" parent="1"><mxGeometry y="40" width="10" height="10" as="geometry"/></mxCell>
        <mxCell id="h" value="Hidden" vertex="1" visible="0" parent="1"><mxGeometry y="60" width="10" height="10" as="geometry"/></mxCell>
        "#,
    );
    let report = load(&xml);
    assert_eq!(
        report.warnings,
        vec![
            ImportWarning::Formatting,
            ImportWarning::Resources,
            ImportWarning::Other
        ]
    );
    assert_eq!(
        shapes(&report.workbook.sheets[0]).len(),
        3,
        "unknown and image shapes become placeholders"
    );
}

fn kinds_by_label(sheet: &Sheet) -> Vec<(String, ShapeKind)> {
    shapes(sheet)
        .iter()
        .filter_map(|o| match &o.kind {
            CanvasObjectKind::Shape { shape, label } => Some((label.clone(), *shape)),
            _ => None,
        })
        .collect()
}

#[test]
fn built_in_shape_aliases_map_to_their_thinoda_equivalents() {
    let cell = |id: &str, style: &str, y: u32| {
        format!(
            r#"<mxCell id="{id}" value="{id}" style="{style}" vertex="1" parent="1"><mxGeometry y="{y}" width="40" height="20" as="geometry"/></mxCell>"#
        )
    };
    let exact = [
        ("pr", "shape=partialRectangle;"),
        ("br", "shape=mxgraph.basic.rect;"),
        ("bs", "shape=mxgraph.bootstrap.rect;"),
        ("bt", "shape=mxgraph.bootstrap.rrect;"),
        ("dl", "shape=delay;"),
    ];
    let cells: String = exact
        .iter()
        .enumerate()
        .map(|(n, (id, style))| cell(id, style, n as u32 * 30))
        .collect();
    let sheet = clean(&page(&cells));
    use ShapeKind as S;
    assert_eq!(
        kinds_by_label(&sheet),
        vec![
            ("pr".into(), S::Rectangle),
            ("br".into(), S::Rectangle),
            ("bs".into(), S::Rectangle),
            ("bt".into(), S::RoundedRectangle),
            ("dl".into(), S::Delay),
        ]
    );

    let approximate = [
        ("fo", "shape=folder;"),
        ("cu", "shape=cube;"),
        ("me", "shape=message;"),
        ("uf", "shape=umlFrame;"),
        ("es", "shape=endState;"),
        ("pc", "shape=mxgraph.basic.partConcEllipse;"),
        ("ar", "shape=mxgraph.arrows2.arrow;"),
        ("ps", "shape=partialRectangle;top=0;left=0;"),
    ];
    let cells: String = approximate
        .iter()
        .enumerate()
        .map(|(n, (id, style))| cell(id, style, n as u32 * 30))
        .collect();
    let report = load(&page(&cells));
    assert_eq!(
        report.warnings,
        vec![ImportWarning::Formatting],
        "approximations are formatting losses, not unknown content"
    );
    let sheet = &report.workbook.sheets[0];
    assert_eq!(
        kinds_by_label(sheet),
        vec![
            ("fo".into(), S::Rectangle),
            ("cu".into(), S::Rectangle),
            ("me".into(), S::Rectangle),
            ("uf".into(), S::Rectangle),
            ("es".into(), S::Ellipse),
            ("pc".into(), S::Ellipse),
            ("ar".into(), S::Arrow),
            ("ps".into(), S::Rectangle),
        ]
    );
    let partial = shapes(sheet).last().copied().unwrap();
    assert_eq!(
        partial.style.stroke,
        Some(Color::TRANSPARENT),
        "a rectangle with hidden sides drops its outline rather than inventing lines"
    );
}

#[test]
fn waypoints_become_small_dots_in_the_stroke_color() {
    let xml = page(
        r##"
        <mxCell id="a" value="A" vertex="1" parent="1"><mxGeometry x="0" y="40" width="40" height="40" as="geometry"/></mxCell>
        <mxCell id="w" style="shape=waypoint;fillStyle=solid;size=6;strokeColor=#FF0000;" vertex="1" parent="1"><mxGeometry x="100" y="50" width="20" height="20" as="geometry"/></mxCell>
        <mxCell id="e" edge="1" parent="1" source="a" target="w"><mxGeometry relative="1" as="geometry"/></mxCell>
        "##,
    );
    let sheet = clean(&xml);
    let dot = shapes(&sheet)
        .into_iter()
        .find(|o| {
            matches!(
                o.kind,
                CanvasObjectKind::Shape {
                    shape: ShapeKind::Ellipse,
                    ..
                }
            )
        })
        .expect("waypoint dot");
    assert_eq!(
        dot.rect,
        Rect::from_center_size(Vec2::new(110.0, 60.0), Vec2::new(6.0, 6.0))
    );
    let red = Color::rgba(255, 0, 0, 255);
    assert_eq!(dot.style.fill, Some(red));
    assert_eq!(dot.style.stroke, Some(red));
    assert_eq!(
        connectors(&sheet).len(),
        1,
        "edges still attach to the waypoint"
    );
}

fn tiny_png(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(width, height, image::Rgba([200, 30, 30, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn image_cards(sheet: &Sheet) -> Vec<&thinoda_core::Topic> {
    sheet
        .topics
        .values()
        .filter(|t| t.card.is_some_and(|c| c.kind == CardKind::Image))
        .collect()
}

#[test]
fn inline_png_and_svg_images_become_transparent_image_cards() {
    let png = tiny_png(40, 20);
    let svg = b64(br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><circle cx="5" cy="5" r="4" fill="blue"/></svg>"#);
    let xml = page(&format!(
        r#"
        <mxCell id="p" value="Logo" style="shape=image;verticalLabelPosition=bottom;image=data:image/png,{};" vertex="1" parent="1"><mxGeometry x="10" y="10" width="80" height="80" as="geometry"/></mxCell>
        <mxCell id="s" style="shape=image;imageAspect=0;image=data:image/svg+xml,{svg};" vertex="1" parent="1"><mxGeometry x="200" y="10" width="60" height="30" as="geometry"/></mxCell>
        <mxCell id="e" style="endArrow=classic;" edge="1" parent="1" source="p" target="s"><mxGeometry relative="1" as="geometry"/></mxCell>
        "#,
        b64(&png)
    ));
    let sheet = clean(&xml);
    let cards = image_cards(&sheet);
    assert_eq!(cards.len(), 2);
    let logo = cards.iter().find(|t| t.title == "Logo").unwrap();
    // 2:1 pixels fitted into an 80x80 cell keep their aspect.
    assert_eq!(logo.image_size, Some(Vec2::new(80.0, 40.0)));
    assert_eq!(logo.card.unwrap().color, Some(Color::TRANSPARENT));
    let resource = sheet.resources.get(logo.image.as_deref().unwrap()).unwrap();
    assert_eq!(resource.media_type, "image/png");
    assert_eq!(resource.bytes(), png.as_slice());
    let vector = cards.iter().find(|t| t.title.is_empty()).unwrap();
    assert_eq!(
        vector.image_size,
        Some(Vec2::new(60.0, 30.0)),
        "imageAspect=0 stretches"
    );
    let raster = sheet
        .resources
        .get(vector.image.as_deref().unwrap())
        .unwrap();
    let decoded = thinoda_core::images::decode(raster.bytes()).unwrap();
    assert_eq!((decoded.width, decoded.height), (120, 120));
    assert!(
        decoded.rgba.chunks(4).any(|p| p[2] > 200 && p[3] == 255),
        "circle rendered"
    );
    assert_eq!(
        connectors(&sheet).len(),
        1,
        "edges still attach to image cards"
    );
    sheet.validate().unwrap();
}

#[test]
fn remote_and_unsupported_images_stay_placeholders_with_their_source() {
    let svgz = b64(&[0x1f, 0x8b, 8, 0, 0, 0, 0, 0]);
    let file_ref = b64(br#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="4" height="4"><image width="4" height="4" xlink:href="Cargo.toml"/></svg>"#);
    let xml = page(&format!(
        r#"
        <mxCell id="r" style="shape=image;image=https://example.com/a.png;" vertex="1" parent="1"><mxGeometry width="10" height="10" as="geometry"/></mxCell>
        <mxCell id="l" style="shape=image;image=img/lib/azure2/compute/VM.svg;" vertex="1" parent="1"><mxGeometry y="20" width="10" height="10" as="geometry"/></mxCell>
        <mxCell id="g" style="shape=image;image=data:image/gif,R0lGODlhAQABAAAAACw=;" vertex="1" parent="1"><mxGeometry y="40" width="10" height="10" as="geometry"/></mxCell>
        <mxCell id="z" style="shape=image;image=data:image/svg+xml,{svgz};" vertex="1" parent="1"><mxGeometry y="60" width="10" height="10" as="geometry"/></mxCell>
        <mxCell id="b" style="shape=image;image=data:image/png,bm90IGEgcG5n;" vertex="1" parent="1"><mxGeometry y="80" width="10" height="10" as="geometry"/></mxCell>
        <mxCell id="f" style="shape=image;image=data:image/svg+xml,{file_ref};" vertex="1" parent="1"><mxGeometry y="100" width="10" height="10" as="geometry"/></mxCell>
        "#
    ));
    let report = load(&xml);
    assert_eq!(report.warnings, vec![ImportWarning::Resources]);
    let sheet = &report.workbook.sheets[0];
    let mut alts: Vec<_> = shapes(sheet).iter().map(|o| o.alt_text.clone()).collect();
    alts.sort();
    assert_eq!(
        alts,
        vec![
            "",
            "",
            "",
            "https://example.com/a.png",
            "img/lib/azure2/compute/VM.svg"
        ],
        "data URIs are not echoed into alt text"
    );
    // The SVG referencing a local file still renders, but without reading the file.
    let cards = image_cards(sheet);
    assert_eq!(cards.len(), 1);
    let raster = sheet
        .resources
        .get(cards[0].image.as_deref().unwrap())
        .unwrap();
    let decoded = thinoda_core::images::decode(raster.bytes()).unwrap();
    assert!(
        decoded.rgba.chunks(4).all(|p| p[3] == 0),
        "file href was not loaded"
    );
}

#[test]
fn image_cards_round_trip_their_pixels_and_size() {
    let png = tiny_png(30, 30);
    let xml = page(&format!(
        r#"<mxCell id="p" value="Pic" style="shape=image;image=data:image/png,{};" vertex="1" parent="1"><mxGeometry x="10" y="10" width="50" height="50" as="geometry"/></mxCell>"#,
        b64(&png)
    ));
    let book = load(&xml).workbook;
    let exported = String::from_utf8(export(&book).unwrap()).unwrap();
    assert!(exported.contains("shape=image;thinodaCard=image;"));
    assert!(exported.contains(&format!("image=data:image/png,{};", b64(&png))));
    let again = roundtrip(&book);
    let before = image_cards(&book.sheets[0]);
    let after = image_cards(&again.sheets[0]);
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].image_size, before[0].image_size);
    assert_eq!(after[0].card, before[0].card);
    let bytes = |sheet: &Sheet, topic: &thinoda_core::Topic| {
        sheet
            .resources
            .get(topic.image.as_deref().unwrap())
            .unwrap()
            .bytes()
            .to_vec()
    };
    assert_eq!(bytes(&again.sheets[0], after[0]), png);
    assert_eq!(bytes(&book.sheets[0], before[0]), png);

    // The public gate accepts card pictures but still refuses resources draw.io would drop.
    let opts = crate::ExportOptions::default();
    for format in [Format::DrawIo, Format::DrawIoSvg, Format::DrawIoPng] {
        let encoded = crate::export_bytes(&book, format, &opts).unwrap();
        let restored = import(&encoded).unwrap().workbook;
        let card = image_cards(&restored.sheets[0])[0];
        assert_eq!(bytes(&restored.sheets[0], card), png, "{format:?}");
    }
    let mut lossy = book.clone();
    let sheet = &mut lossy.sheets[0];
    let reference = before[0].image.clone();
    let root = sheet.root;
    sheet.topic_mut(root).unwrap().image = reference;
    for format in [Format::DrawIo, Format::DrawIoSvg, Format::DrawIoPng] {
        assert!(crate::export_bytes(&lossy, format, &opts).is_err());
    }
}

#[test]
fn hostile_inputs_are_rejected() {
    let dtd = "<!DOCTYPE mxfile [<!ENTITY x \"boom\">]><mxfile><diagram>&x;</diagram></mxfile>";
    assert!(import(dtd.as_bytes()).is_err());
    assert!(import(b"<mxfile><diagram>&unknown;</diagram></mxfile>").is_err());
    let deep = format!(
        "<mxfile>{}{}</mxfile>",
        "<a>".repeat(100),
        "</a>".repeat(100)
    );
    assert!(import(deep.as_bytes()).is_err());
    assert!(import(b"<mxfile></mxfile>").is_err());
    assert!(import(b"<html/>").is_err());
    assert!(import(b"<mxfile><diagram>!!notbase64!!</diagram></mxfile>").is_err());
    assert!(import(PNG_SIGNATURE).is_err());
    let mut truncated = PNG_SIGNATURE.to_vec();
    truncated.extend(u32::MAX.to_be_bytes());
    truncated.extend(b"tEXt");
    assert!(!sniff(&truncated));
    let bomb = miniz_oxide::deflate::compress_to_vec(&vec![b'a'; MAX_BYTES + 1], 9);
    let encoded = base64::engine::general_purpose::STANDARD.encode(bomb);
    let page = format!("<mxfile><diagram>{encoded}</diagram></mxfile>");
    assert!(import(page.as_bytes()).is_err());
    assert!(matches!(
        import_html(legacy_html_viewer(&encoded).as_bytes()),
        Err(IoError::DrawIoHtml(crate::DrawIoHtmlError::InvalidViewer))
    ));
}

#[test]
fn cyclic_and_self_parented_cells_terminate() {
    let xml = page(
        r#"
        <mxCell id="x" value="X" vertex="1" parent="y"><mxGeometry width="10" height="10" as="geometry"/></mxCell>
        <mxCell id="y" value="Y" vertex="1" parent="x"><mxGeometry width="10" height="10" as="geometry"/></mxCell>
        <mxCell id="s" value="S" vertex="1" parent="s"><mxGeometry width="10" height="10" as="geometry"/></mxCell>
        <mxCell id="ok" value="OK" vertex="1" parent="1"><mxGeometry width="10" height="10" as="geometry"/></mxCell>
        "#,
    );
    let report = load(&xml);
    assert!(report.warnings.contains(&ImportWarning::Other));
    assert!(
        shapes(&report.workbook.sheets[0])
            .iter()
            .any(|o| o.label() == "OK")
    );
}

#[test]
fn export_reports_flattened_map_content() {
    let mut book = board();
    assert_eq!(
        export_losses(&book, Format::DrawIo, &ExportOptions::default()),
        vec![ExportLoss::LegacyConnectorMarkersApproximated(2)]
    );
    let sheet = &mut book.sheets[0];
    let idea = sheet.add_floating("Idea", Vec2::new(200.0, -300.0));
    let detail = sheet.add_child(idea, None, "Detail");
    sheet.topic_mut(detail).unwrap().notes = Some("note".into());
    sheet
        .add_presentation_slide(
            "All",
            PresentationSource::Viewport,
            rect(0.0, 0.0, 100.0, 100.0),
        )
        .unwrap();
    assert_eq!(
        export_losses(&book, Format::DrawIo, &ExportOptions::default()),
        vec![
            ExportLoss::MapElementsFlattened(2),
            ExportLoss::LegacyConnectorMarkersApproximated(2),
            ExportLoss::TopicDetailsDropped(1),
            ExportLoss::SlidesDropped(1),
        ]
    );
    let bytes = export_bytes(&book, Format::DrawIo, &ExportOptions::default()).unwrap();
    let report = import(&bytes).unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let sheet = &report.workbook.sheets[0];
    assert!(shapes(sheet).iter().any(|o| o.label() == "Idea"));
    assert!(shapes(sheet).iter().any(|o| o.label() == "Detail"));
    assert_eq!(
        connectors(sheet).len(),
        3,
        "two board connectors and one branch"
    );
}
