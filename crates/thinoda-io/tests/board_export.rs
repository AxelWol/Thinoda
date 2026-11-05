//! Freeform board export (phase F0-IO): native versioning, loss report, outline
//! projection, visual scene painting and XMind root handling.

use std::io::{Cursor, Read, Write};
use thinoda_core::{
    CanvasConnector, CanvasObject, Card, CardKind, Endpoint, InkStroke, InkTool,
    PresentationSource, ShapeKind, Sheet, SheetRootMode, Workbook,
    board::{CanvasObjectKind, RootProjection},
    geometry::{Color, Rect, Vec2},
    outline::{project_to_outline, to_outline},
    resources::Resource,
};
use thinoda_io::{
    ExportLoss, ExportOptions, Format, IoError, export_bytes, export_bytes_with_consent,
    export_losses, import_bytes, outline_sheet, render_presentation_slide_svg,
    required_native_version, uses_board_semantics,
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

const ANCHOR: &str = "ANCHORSECRET";

fn board() -> Workbook {
    let mut s = Sheet::new("Board", ANCHOR);
    s.root_mode = SheetRootMode::Hidden;
    let idea = s.add_floating("Idea", Vec2::new(200.0, 0.0));
    s.add_child(idea, None, "Detail");
    let mut card = Card::new(CardKind::Sticky, Vec2::new(160.0, 120.0));
    card.rotation = 8.0;
    let sticky = s
        .add_card("Sticky note", Vec2::new(-200.0, 100.0), card)
        .unwrap();
    let mut shape = CanvasObject::shape(
        ShapeKind::Cloud,
        Rect::from_min_size(Vec2::new(0.0, 300.0), Vec2::new(120.0, 80.0)),
    );
    if let CanvasObjectKind::Shape { label, .. } = &mut shape.kind {
        *label = "Shapelabel".into();
    }
    shape.style.fill = Some(Color::rgb(0x33, 0x99, 0xFF));
    let shape = s.add_canvas_object(shape).unwrap();
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
    let mut connector = CanvasConnector::new(Endpoint::Object(shape), Endpoint::Topic(sticky));
    connector.label = "Connlabel".into();
    s.add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    s.add_presentation_slide(
        "Overview",
        PresentationSource::Viewport,
        Rect::from_min_size(Vec2::new(-300.0, -200.0), Vec2::new(800.0, 600.0)),
    )
    .unwrap();
    let wb = Workbook::new(s);
    wb.validate().unwrap();
    wb
}

fn central() -> Workbook {
    let mut s = Sheet::new("Plan", "Central");
    let a = s.add_child(s.root, None, "Alpha");
    s.add_child(a, None, "Beta");
    Workbook::new(s)
}

type SheetMutation = Box<dyn Fn(&mut Sheet)>;
type JsonMutation = Box<dyn Fn(&mut serde_json::Value)>;

/// Zone around a new floating topic (zones need at least one root).
fn add_zone(sheet: &mut Sheet, frame: Rect) -> thinoda_core::ZoneId {
    let root = sheet.add_floating("Zoned", frame.center());
    sheet.create_zone("Zone", vec![root], frame).unwrap()
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

fn zip_with(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut c = Cursor::new(Vec::new());
    let mut z = ZipWriter::new(&mut c);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in entries {
        z.start_file(*name, opts).unwrap();
        z.write_all(data).unwrap();
    }
    z.finish().unwrap();
    c.into_inner()
}

fn manifest_version(bytes: &[u8]) -> u64 {
    let manifest: serde_json::Value =
        serde_json::from_slice(&entry(bytes, "manifest.json")).unwrap();
    manifest["version"].as_u64().unwrap()
}

fn export(wb: &Workbook, format: Format) -> Vec<u8> {
    export_bytes_with_consent(wb, format, &ExportOptions::default(), true)
        .unwrap_or_else(|e| panic!("{format:?}: {e}"))
}

/// Text of a file, or of every entry when it is a ZIP package.
fn all_text(bytes: &[u8]) -> String {
    match bytes.starts_with(b"PK") {
        true => {
            let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
            let mut text = String::new();
            for i in 0..archive.len() {
                let mut file = archive.by_index(i).unwrap();
                let mut data = Vec::new();
                file.read_to_end(&mut data).unwrap();
                text.push_str(&String::from_utf8_lossy(&data));
            }
            text
        }
        false => String::from_utf8_lossy(bytes).into_owned(),
    }
}

// --- native versioning --------------------------------------------------------------

#[test]
fn explicit_zone_objects_require_native_v9_and_round_trip() {
    let mut wb = board();
    let sheet = &mut wb.sheets[0];
    let zone = sheet
        .create_board_frame(
            "Mixed",
            Rect::from_min_size(Vec2::ZERO, Vec2::new(500.0, 500.0)),
        )
        .unwrap();
    let root = sheet.floating[0];
    let object = sheet.canvas.objects[0].id;
    sheet
        .set_zone_members(zone, vec![root], vec![object])
        .unwrap();
    wb.validate().unwrap();
    assert_eq!(required_native_version(&wb), 9);
    let bytes = export(&wb, Format::Thinoda);
    assert_eq!(manifest_version(&bytes), 9);
    assert_eq!(import_bytes(Format::Thinoda, &bytes).unwrap(), wb);
    let content = serde_json::to_vec(&wb).unwrap();
    let downgraded = zip_with(&[
        (
            "manifest.json",
            br#"{"format":"thinoda","version":8,"resources":[]}"#,
        ),
        ("content.json", &content),
    ]);
    assert!(
        import_bytes(Format::Thinoda, &downgraded)
            .unwrap_err()
            .to_string()
            .contains("explicit Zone object membership requires native format version 9")
    );
    assert!(
        import_bytes(Format::Thinoda, &content)
            .unwrap_err()
            .to_string()
            .contains("explicit Zone object membership requires a versioned native archive")
    );
    let sheet = &mut wb.sheets[0];
    let other = sheet
        .create_board_frame(
            "Other",
            Rect::from_min_size(Vec2::new(700.0, 0.0), Vec2::new(200.0, 200.0)),
        )
        .unwrap();
    sheet
        .set_zone_members(other, vec![], vec![sheet.canvas.objects[1].id])
        .unwrap();
    let slide = sheet
        .add_presentation_slide(
            "Mixed",
            PresentationSource::Zone(zone),
            sheet.zones[0].frame,
        )
        .unwrap();
    let slide = sheet
        .presentation
        .slides
        .iter_mut()
        .find(|candidate| candidate.id == slide)
        .unwrap();
    slide.scope = thinoda_core::PresentationScope::Source;
    let svg = render_presentation_slide_svg(sheet, 1, &ExportOptions::default()).unwrap();
    assert!(svg.contains(&format!("id=\"object-{}\"", object.0)));
    assert!(!svg.contains(&format!("id=\"object-{}\"", sheet.canvas.objects[1].id.0)));
}

#[test]
fn board_workbooks_write_native_v6_and_round_trip() {
    let wb = board();
    assert_eq!(required_native_version(&wb), 6);
    let bytes = export_bytes(&wb, Format::Thinoda, &ExportOptions::default()).unwrap();
    assert_eq!(manifest_version(&bytes), 6);
    let back = import_bytes(Format::Thinoda, &bytes).unwrap();
    assert_eq!(back, wb);
    // Byte-stable re-export.
    assert_eq!(
        export_bytes(&back, Format::Thinoda, &ExportOptions::default()).unwrap(),
        bytes
    );
}

#[test]
fn independent_zone_background_colors_require_native_v7() {
    let mut wb = central();
    add_zone(
        &mut wb.sheets[0],
        Rect::from_min_size(Vec2::ZERO, Vec2::new(120.0, 100.0)),
    );
    wb.sheets[0].zones[0].background_color = Some(Color::rgb(240, 220, 180));
    assert_eq!(required_native_version(&wb), 7);

    let bytes = export_bytes(&wb, Format::Thinoda, &ExportOptions::default()).unwrap();
    assert_eq!(manifest_version(&bytes), 7);
    assert_eq!(import_bytes(Format::Thinoda, &bytes).unwrap(), wb);

    let content = serde_json::to_vec(&wb).unwrap();
    let bytes = zip_with(&[
        (
            "manifest.json",
            br#"{"format":"thinoda","version":6,"resources":[]}"#,
        ),
        ("content.json", &content),
    ]);
    assert!(
        import_bytes(Format::Thinoda, &bytes)
            .unwrap_err()
            .to_string()
            .contains("independent Zone background colors require native format version 7")
    );
}

#[test]
fn inline_card_text_requires_native_v8_and_reports_non_native_loss() {
    let mut wb = central();
    let id = wb.sheets[0]
        .add_card(
            "Card title",
            Vec2::new(120.0, 100.0),
            Card::new(CardKind::Text, Vec2::new(180.0, 120.0)),
        )
        .unwrap();
    wb.sheets[0]
        .add_card(
            "Unformatted card",
            Vec2::new(360.0, 100.0),
            Card::new(CardKind::Sticky, Vec2::new(180.0, 120.0)),
        )
        .unwrap();
    wb.sheets[0]
        .topics
        .get_mut(&id)
        .unwrap()
        .inline_text
        .update_range("Card title", 0..4, |style| style.bold = Some(true))
        .unwrap();
    wb.validate().unwrap();
    assert_eq!(required_native_version(&wb), 8);

    let bytes = export_bytes(&wb, Format::Thinoda, &ExportOptions::default()).unwrap();
    assert_eq!(manifest_version(&bytes), 8);
    assert_eq!(import_bytes(Format::Thinoda, &bytes).unwrap(), wb);

    let content = serde_json::to_vec(&wb).unwrap();
    let old_archive = zip_with(&[
        (
            "manifest.json",
            br#"{"format":"thinoda","version":7,"resources":[]}"#,
        ),
        ("content.json", &content),
    ]);
    assert!(
        import_bytes(Format::Thinoda, &old_archive)
            .unwrap_err()
            .to_string()
            .contains("inline card text requires native format version 8")
    );
    assert!(
        import_bytes(Format::Thinoda, &content)
            .unwrap_err()
            .to_string()
            .contains("inline card text requires a versioned native archive")
    );
    assert!(
        export_losses(&wb, Format::PlainText, &ExportOptions::default())
            .contains(&ExportLoss::InlineTextDropped(1))
    );
    assert!(
        export_losses(&wb, Format::Svg, &ExportOptions::default())
            .contains(&ExportLoss::InlineTextDropped(1))
    );
}

#[test]
fn localized_board_templates_round_trip_in_native_v6() {
    use thinoda_core::templates::{TEMPLATES, TemplateCategory, instantiate_localized};

    for template in TEMPLATES
        .iter()
        .filter(|template| template.category == TemplateCategory::Boards)
    {
        let wb = instantiate_localized(template.id, &|key| format!("<{key}>"));
        let bytes = export_bytes(&wb, Format::Thinoda, &ExportOptions::default()).unwrap();
        assert_eq!(manifest_version(&bytes), 6, "{}", template.id);
        let restored = import_bytes(Format::Thinoda, &bytes).unwrap();
        assert_eq!(restored, wb, "{}", template.id);
        assert_eq!(
            export_bytes(&restored, Format::Thinoda, &ExportOptions::default()).unwrap(),
            bytes,
            "{}",
            template.id
        );
    }
}

#[test]
fn legacy_v6_hidden_anchor_title_round_trips_without_becoming_board_content() {
    let wb = board();
    let bytes = export_bytes(&wb, Format::Thinoda, &ExportOptions::default()).unwrap();
    assert_eq!(manifest_version(&bytes), 6);
    let content = entry(&bytes, "content.json");
    assert!(String::from_utf8_lossy(&content).contains(ANCHOR));
    assert_eq!(import_bytes(Format::Thinoda, &content).unwrap(), wb);
    let mut restored = import_bytes(Format::Thinoda, &bytes).unwrap();
    assert_eq!(restored, wb);
    assert_eq!(restored.active().root_topic().title, ANCHOR);
    assert!(
        !restored
            .active()
            .content_topics()
            .any(|topic| topic.title == ANCHOR)
    );

    let mut empty_anchor = restored.clone();
    let root = empty_anchor.active().root;
    empty_anchor
        .active_mut()
        .topic_mut(root)
        .unwrap()
        .title
        .clear();
    for format in [Format::Svg, Format::Png, Format::Jpeg] {
        assert_eq!(
            export(&restored, format),
            export(&empty_anchor, format),
            "{format:?} changed when only the hidden title changed"
        );
    }
    let pptx = export(&restored, Format::PowerPoint);
    let empty_pptx = export(&empty_anchor, Format::PowerPoint);
    assert_eq!(
        entry(&pptx, "ppt/media/slide1-viewport.png"),
        entry(&empty_pptx, "ppt/media/slide1-viewport.png")
    );
    assert!(!all_text(&pptx).contains(ANCHOR));
    assert!(!all_text(&export(&restored, Format::Markdown)).contains(ANCHOR));

    let branch = restored.active().floating[0];
    let frame = Rect::from_min_size(Vec2::new(-300.0, -200.0), Vec2::new(800.0, 600.0));
    let sheet = restored.active_mut();
    let zone = sheet.create_zone("Ideas", vec![branch], frame).unwrap();
    let zone_style = sheet
        .zones
        .iter_mut()
        .find(|candidate| candidate.id == zone)
        .unwrap();
    zone_style.color = Some(Color::rgb(0xAA, 0x11, 0x22));
    zone_style.background_color = Some(Color::rgb(0x11, 0x22, 0xAA));
    for source in [
        PresentationSource::Branch(branch),
        PresentationSource::Zone(zone),
    ] {
        sheet
            .add_presentation_slide("Slide", source, frame)
            .unwrap();
    }
    let mut empty_anchor = restored.clone();
    let root = empty_anchor.active().root;
    empty_anchor
        .active_mut()
        .topic_mut(root)
        .unwrap()
        .title
        .clear();
    let options = ExportOptions::default();
    for index in 0..3 {
        let svg = render_presentation_slide_svg(restored.active(), index, &options).unwrap();
        assert!(!svg.contains(ANCHOR), "slide {index}");
        assert_eq!(
            svg,
            render_presentation_slide_svg(empty_anchor.active(), index, &options).unwrap(),
            "slide {index} changed when only the hidden title changed"
        );
    }
    let mixed_pptx = export(&restored, Format::PowerPoint);
    for index in [2, 3] {
        let slide =
            String::from_utf8(entry(&mixed_pptx, &format!("ppt/slides/slide{index}.xml"))).unwrap();
        assert!(slide.contains("Idea"), "editable slide {index}");
        assert!(!slide.contains(ANCHOR), "editable slide {index}");
        if index == 3 {
            assert!(slide.contains("val=\"1122AA\""), "separate frame fill");
            assert!(slide.contains("val=\"AA1122\""), "frame outline");
        }
    }

    restored.active_mut().show_central_topic();
    assert_eq!(restored.active().root_topic().title, ANCHOR);
}

#[test]
fn each_board_semantic_alone_requires_v6() {
    let mutations: Vec<SheetMutation> = vec![
        Box::new(|s| {
            let mut hidden = Sheet::new("Plan", "Central");
            hidden.add_floating("Alpha", Vec2::ZERO);
            hidden.root_mode = SheetRootMode::Hidden;
            *s = hidden;
        }),
        Box::new(|s| {
            s.add_canvas_object(CanvasObject::shape(
                ShapeKind::Rectangle,
                Rect::from_min_size(Vec2::ZERO, Vec2::new(10.0, 10.0)),
            ))
            .unwrap();
        }),
        Box::new(|s| {
            s.add_card(
                "Card",
                Vec2::new(500.0, 0.0),
                Card::new(CardKind::Text, Vec2::new(100.0, 80.0)),
            )
            .unwrap();
        }),
        Box::new(|s| s.board.snap = !s.board.snap),
        Box::new(|s| {
            let zone = add_zone(s, Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 100.0)));
            s.zones.iter_mut().find(|z| z.id == zone).unwrap().color = Some(Color::rgb(1, 2, 3));
        }),
        Box::new(|s| {
            let zone = add_zone(s, Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 100.0)));
            s.zones.iter_mut().find(|z| z.id == zone).unwrap().clip = true;
        }),
        Box::new(|s| {
            s.create_board_frame(
                "Empty frame",
                Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 100.0)),
            )
            .unwrap();
        }),
        Box::new(|s| {
            s.add_presentation_slide(
                "View",
                PresentationSource::Viewport,
                Rect::from_min_size(Vec2::ZERO, Vec2::new(800.0, 600.0)),
            )
            .unwrap();
        }),
    ];
    for (index, mutate) in mutations.iter().enumerate() {
        let mut wb = central();
        assert!(!uses_board_semantics(&wb.sheets[0]));
        assert!(!wb.sheets[0].uses_board_semantics());
        mutate(&mut wb.sheets[0]);
        assert!(uses_board_semantics(&wb.sheets[0]), "mutation {index}");
        assert!(wb.sheets[0].uses_board_semantics(), "mutation {index}");
        assert_eq!(required_native_version(&wb), 6, "mutation {index}");
        let bytes = export_bytes(&wb, Format::Thinoda, &ExportOptions::default()).unwrap();
        assert_eq!(manifest_version(&bytes), 6, "mutation {index}");
        assert_eq!(
            import_bytes(Format::Thinoda, &bytes).unwrap(),
            wb,
            "mutation {index}"
        );
    }
}

#[test]
fn central_workbooks_keep_their_previous_native_version_and_bytes() {
    // v2: identical entries to the pre-board writer (manifest literal + serde content).
    let wb = central();
    assert_eq!(required_native_version(&wb), 2);
    let bytes = export_bytes(&wb, Format::Thinoda, &ExportOptions::default()).unwrap();
    assert_eq!(
        entry(&bytes, "manifest.json"),
        br#"{"format":"thinoda","resources":[],"version":2}"#.to_vec()
    );
    assert_eq!(
        entry(&bytes, "content.json"),
        serde_json::to_vec(&wb).unwrap()
    );
    let content = String::from_utf8(entry(&bytes, "content.json")).unwrap();
    for key in ["root_mode", "canvas", "\"board\"", "\"card\""] {
        assert!(
            !content.contains(key),
            "{key} leaked into a Central v2 file"
        );
    }
    assert_eq!(
        export_bytes(&wb, Format::Thinoda, &ExportOptions::default()).unwrap(),
        bytes
    );

    // v3: resources.
    let mut res = central();
    res.sheets[0]
        .resources
        .insert(Resource::new("a.bin", "application/octet-stream", vec![1, 2, 3]).unwrap())
        .unwrap();
    assert_eq!(required_native_version(&res), 3);
    let bytes = export_bytes(&res, Format::Thinoda, &ExportOptions::default()).unwrap();
    assert_eq!(manifest_version(&bytes), 3);

    // v4: formatted notes.
    let mut notes = central();
    let root = notes.sheets[0].root;
    notes.sheets[0]
        .set_markdown_notes(root, Some("**bold**".into()))
        .unwrap();
    assert_eq!(required_native_version(&notes), 4);
    let bytes = export_bytes(&notes, Format::Thinoda, &ExportOptions::default()).unwrap();
    assert_eq!(manifest_version(&bytes), 4);

    // A plain zone (no color, no clip) and a Branch slide stay below v6.
    let mut zoned = central();
    add_zone(
        &mut zoned.sheets[0],
        Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 100.0)),
    );
    zoned.sheets[0].zones[0].color = None;
    assert_eq!(required_native_version(&zoned), 2);
}

#[test]
fn board_data_under_older_manifests_is_rejected() {
    let content = serde_json::to_vec(&board()).unwrap();
    for manifest in [
        br#"{"format":"thinoda","version":2,"resources":[]}"#.as_slice(),
        br#"{"format":"thinoda","version":5,"resources":[]}"#.as_slice(),
    ] {
        let bytes = zip_with(&[("manifest.json", manifest), ("content.json", &content)]);
        let error = import_bytes(Format::Thinoda, &bytes).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("requires native format version 6"),
            "{error}"
        );
    }
    let bytes = zip_with(&[
        (
            "manifest.json",
            br#"{"format":"thinoda","version":10,"resources":[]}"#,
        ),
        ("content.json", &content),
    ]);
    assert!(matches!(
        import_bytes(Format::Thinoda, &bytes),
        Err(IoError::NewerVersion(_))
    ));
}

#[test]
fn malformed_or_oversized_v6_board_data_is_rejected_without_panicking() {
    let base = serde_json::to_value(board()).unwrap();
    let mutations: Vec<JsonMutation> = vec![
        // Duplicate canvas object id.
        Box::new(|v| {
            let objects = v["sheets"][0]["canvas"]["objects"].as_array_mut().unwrap();
            let copy = objects[0].clone();
            objects.push(copy);
        }),
        // Connector to a missing topic.
        Box::new(|v| {
            for object in v["sheets"][0]["canvas"]["objects"].as_array_mut().unwrap() {
                if let Some(connector) = object["kind"].get_mut("connector") {
                    connector["to"] = serde_json::json!({"topic": 987654});
                }
            }
        }),
        // Coordinates far outside the board.
        Box::new(|v| {
            v["sheets"][0]["canvas"]["objects"][0]["rect"]["min"]["x"] = serde_json::json!(1e12);
        }),
        // Oversized stroke.
        Box::new(|v| {
            for object in v["sheets"][0]["canvas"]["objects"].as_array_mut().unwrap() {
                if let Some(ink) = object["kind"].get_mut("ink") {
                    let points: Vec<_> = (0..25_000)
                        .map(|i| serde_json::json!({"x": i as f32, "y": 0.0}))
                        .collect();
                    ink["points"] = serde_json::Value::Array(points);
                }
            }
        }),
        // Card on a child topic.
        Box::new(|v| {
            let topics = v["sheets"][0]["topics"].as_object_mut().unwrap();
            for topic in topics.values_mut() {
                if topic["title"] == "Detail" {
                    topic["card"] =
                        serde_json::json!({"kind": "sticky", "size": {"x": 10.0, "y": 10.0}});
                }
            }
        }),
    ];
    let manifest = br#"{"format":"thinoda","version":6,"resources":[]}"#;
    for (index, mutate) in mutations.iter().enumerate() {
        let mut value = base.clone();
        mutate(&mut value);
        let content = serde_json::to_vec(&value).unwrap();
        let bytes = zip_with(&[("manifest.json", manifest), ("content.json", &content)]);
        assert!(
            import_bytes(Format::Thinoda, &bytes).is_err(),
            "mutation {index} was accepted"
        );
        // Raw JSON workbooks are validated too.
        assert!(
            import_bytes(Format::Thinoda, &content).is_err(),
            "json mutation {index} was accepted"
        );
    }
}

// --- loss report ----------------------------------------------------------------------

#[test]
fn export_losses_itemise_board_content_per_format() {
    let wb = board();
    let o = ExportOptions::default();
    assert!(export_losses(&wb, Format::Thinoda, &o).is_empty());
    for format in [
        Format::Svg,
        Format::Png,
        Format::Jpeg,
        Format::Pdf,
        Format::PowerPoint,
    ] {
        assert!(export_losses(&wb, format, &o).is_empty(), "{format:?}");
    }
    let expected = |floating: bool| {
        let mut v = vec![
            ExportLoss::ShapesDropped(1),
            ExportLoss::InkDropped(1),
            ExportLoss::ConnectorsDropped(1),
            ExportLoss::CardsConvertedToTopics(1),
            ExportLoss::RotationDropped(1),
            ExportLoss::ViewportSlidesDropped(1),
        ];
        if floating {
            v.push(ExportLoss::FloatingTopicsDropped(2));
        }
        v
    };
    let mut markdown = expected(false);
    markdown.push(ExportLoss::SynthesizedRoot {
        sheet: 0,
        title: "Board".into(),
    });
    assert_eq!(export_losses(&wb, Format::Markdown, &o), markdown);
    assert_eq!(export_losses(&wb, Format::XMind, &o), markdown);

    // Central sheets with floating topics lose them in single-outline formats only.
    let mut central_board = board();
    central_board.sheets[0].root_mode = SheetRootMode::Central;
    let mut central_outline = expected(true);
    central_outline[3] = ExportLoss::CardsDropped(1);
    assert_eq!(
        export_losses(&central_board, Format::Opml, &o),
        central_outline
    );
    assert_eq!(
        export_losses(&central_board, Format::XMind, &o),
        expected(false)
    );

    // SVG-backed formats preserve visual clipping; editable PowerPoint does not.
    let mut clipped = central();
    let zone = add_zone(
        &mut clipped.sheets[0],
        Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 100.0)),
    );
    clipped.sheets[0]
        .zones
        .iter_mut()
        .find(|z| z.id == zone)
        .unwrap()
        .clip = true;
    clipped.sheets[0].board.snap = !clipped.sheets[0].board.snap;
    for format in [Format::Svg, Format::Png, Format::Jpeg, Format::Pdf] {
        assert_eq!(
            export_losses(&clipped, format, &o),
            vec![ExportLoss::BoardSettingsDropped],
            "{format:?}"
        );
    }
    assert_eq!(
        export_losses(&clipped, Format::PowerPoint, &o),
        vec![
            ExportLoss::ZoneClipDropped(1),
            ExportLoss::BoardSettingsDropped
        ]
    );
    assert_eq!(
        export_losses(&clipped, Format::Csv, &o),
        vec![
            ExportLoss::ZonesDropped(1),
            ExportLoss::FloatingTopicsDropped(1),
            ExportLoss::BoardSettingsDropped
        ]
    );

    // Plain Central workbooks lose nothing to board export.
    for &format in Format::EXPORTABLE {
        assert!(
            export_losses(&central(), format, &o).is_empty(),
            "{format:?}"
        );
    }
}

// --- outline projection ---------------------------------------------------------------

#[test]
fn outline_sheet_matches_core_projection() {
    let wb = board();
    let sheet = &wb.sheets[0];
    let projected = outline_sheet(sheet);
    assert_eq!(projected.root_mode, SheetRootMode::Central);
    assert_eq!(
        to_outline(&projected, projected.root),
        project_to_outline(sheet, RootProjection::Synthesize)
    );
    assert!(projected.floating.is_empty());
    assert!(projected.canvas.is_empty());
    assert!(projected.topics.values().all(|t| t.card.is_none()));
    // Central sheets are borrowed unchanged.
    let c = central();
    assert!(matches!(
        outline_sheet(&c.sheets[0]),
        std::borrow::Cow::Borrowed(_)
    ));
}

#[test]
fn outline_exports_synthesize_the_root_and_never_leak_the_anchor() {
    let wb = board();
    for format in [
        Format::Markdown,
        Format::PlainText,
        Format::Opml,
        Format::FreeMind,
        Format::Csv,
        Format::Word,
        Format::Excel,
        Format::TextBundle,
        Format::MindManager,
    ] {
        let text = all_text(&export(&wb, format));
        assert!(
            !text.contains(ANCHOR),
            "{format:?} leaked the hidden anchor"
        );
        for title in ["Board", "Idea", "Detail", "Sticky note"] {
            assert!(text.contains(title), "{format:?} lacks {title}");
        }
    }
    // MindManager asks before flattening a board.
    assert!(matches!(
        export_bytes_with_consent(&wb, Format::MindManager, &ExportOptions::default(), false),
        Err(IoError::MindManagerExportNeedsConfirmation)
    ));
    assert!(
        export_bytes_with_consent(
            &central(),
            Format::MindManager,
            &ExportOptions::default(),
            false
        )
        .is_ok()
    );
}

#[test]
fn outline_order_follows_reading_order_under_the_synthesized_root() {
    let wb = board();
    let markdown = String::from_utf8(export(&wb, Format::Markdown)).unwrap();
    let idea = markdown.find("Idea").unwrap();
    let sticky = markdown.find("Sticky note").unwrap();
    let root = markdown.find("Board").unwrap();
    assert!(root < idea && root < sticky);
    // Sticky (y = 100) reads after Idea (y = 0).
    assert!(idea < sticky, "{markdown}");
}

#[test]
fn central_outline_exports_are_unchanged_by_projection() {
    let wb = central();
    for format in [
        Format::Markdown,
        Format::PlainText,
        Format::Opml,
        Format::FreeMind,
        Format::Csv,
    ] {
        let projected = export(&wb, format);
        let again = export(&wb, format);
        assert_eq!(projected, again, "{format:?}");
        let text = String::from_utf8(projected).unwrap();
        assert!(text.contains("Central"), "{format:?}");
    }
    assert_eq!(
        String::from_utf8(export(&wb, Format::PlainText)).unwrap(),
        "Central\n\tAlpha\n\t\tBeta\n"
    );
}

// --- visual exports -------------------------------------------------------------------

fn view_box(svg: &str) -> [f32; 4] {
    let start = svg.find("viewBox=\"").unwrap() + 9;
    let end = start + svg[start..].find('"').unwrap();
    let v: Vec<f32> = svg[start..end]
        .split_whitespace()
        .map(|n| n.parse().unwrap())
        .collect();
    [v[0], v[1], v[2], v[3]]
}

#[test]
fn svg_paints_the_board_scene() {
    let mut wb = board();
    let far = wb.sheets[0]
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Diamond,
            Rect::from_min_size(Vec2::new(3000.0, 2000.0), Vec2::new(50.0, 50.0)),
        ))
        .unwrap();
    let zone = add_zone(
        &mut wb.sheets[0],
        Rect::from_min_size(Vec2::new(-400.0, -300.0), Vec2::new(200.0, 100.0)),
    );
    wb.sheets[0]
        .zones
        .iter_mut()
        .find(|z| z.id == zone)
        .unwrap()
        .color = Some(Color::rgb(0xAA, 0x11, 0x22));
    wb.sheets[0]
        .zones
        .iter_mut()
        .find(|z| z.id == zone)
        .unwrap()
        .background_color = Some(Color::rgb(0x11, 0x22, 0xAA));
    let svg = String::from_utf8(export(&wb, Format::Svg)).unwrap();
    assert!(!svg.contains(ANCHOR), "hidden anchor painted");
    // Shape fill, label, ink highlighter alpha, connector label, sticky card.
    assert!(svg.contains("fill=\"#3399FF\""), "shape fill");
    assert!(svg.contains("Shapelabel"));
    assert!(svg.contains("#FADC0059"), "highlighter alpha");
    assert!(svg.contains("Connlabel"));
    assert!(svg.contains("class=\"card\""));
    assert!(svg.contains("#FFF29E"), "default sticky fill");
    assert!(svg.contains("rotate(8.00"), "card rotation");
    assert!(svg.contains("Sticky note"));
    assert!(svg.contains(&format!("id=\"object-{}\"", far.0)));
    assert!(svg.contains("stroke=\"#AA1122\""), "zone color");
    assert!(
        svg.contains("fill=\"#1122AA29\""),
        "separate zone background"
    );
    let mut transparent = wb.clone();
    transparent.sheets[0]
        .zones
        .iter_mut()
        .find(|candidate| candidate.id == zone)
        .unwrap()
        .background_color = Some(Color::rgba(0x11, 0x22, 0xAA, 0));
    let transparent_svg = String::from_utf8(export(&transparent, Format::Svg)).unwrap();
    assert!(
        transparent_svg.contains("fill=\"#1122AA00\""),
        "transparent Zone backgrounds remain transparent in SVG"
    );
    // Bounds include far objects and the zone.
    let [x, y, w, h] = view_box(&svg);
    assert!(x + w >= 3050.0 && y + h >= 2050.0, "{x} {y} {w} {h}");
    assert!(x <= -400.0 && y <= -300.0, "{x} {y}");
    // Paint order: Below band before the map, Above band after it.
    let first_topic = svg.find("id=\"topic-").unwrap();
    assert!(svg.find(&format!("id=\"object-{}\"", far.0)).unwrap() > first_topic);
    // No grid.
    assert!(!svg.contains("grid"));
}

#[test]
fn swimlane_exports_frames_shapes_and_handoffs() {
    let wb = thinoda_core::templates::instantiate("swimlane");
    let sheet = wb.active();
    let svg = String::from_utf8(export(&wb, Format::Svg)).unwrap();
    for zone in &sheet.zones {
        assert!(svg.contains(&zone.name), "missing lane {}", zone.name);
    }
    for object in &sheet.canvas.objects {
        let start = svg
            .find(&format!("id=\"object-{}\"", object.id.0))
            .unwrap_or_else(|| panic!("missing object {}", object.id.0));
        let end = start + svg[start..].find("</g>").unwrap();
        assert!(
            svg[start..end].contains("<path "),
            "empty object {}",
            object.id.0
        );
    }
    let [x, y, w, h] = view_box(&svg);
    for zone in &sheet.zones {
        assert!(x <= zone.frame.min.x && y <= zone.frame.min.y);
        assert!(x + w >= zone.frame.max().x && y + h >= zone.frame.max().y);
    }
    let png = image::load_from_memory(&export(&wb, Format::Png))
        .unwrap()
        .to_rgb8();
    for color in [[0xB7, 0xEB, 0x8F], [0xA5, 0xD8, 0xFF], [0xFF, 0xE5, 0x8F]] {
        assert!(
            png.pixels().any(|pixel| pixel.0 == color),
            "shape fill {color:?} missing from raster export"
        );
    }
    assert!(export(&wb, Format::Pdf).starts_with(b"%PDF"));
}

fn storyboard_with_image() -> (Workbook, Vec<u8>) {
    let mut wb = thinoda_core::templates::instantiate("storyboard");
    let sheet = wb.active_mut();
    let image_card = sheet
        .content_topics()
        .find(|topic| topic.title == "Scene 1 image")
        .unwrap()
        .id;
    let source = image::RgbImage::from_pixel(8, 8, image::Rgb([18, 196, 120]));
    let mut encoded = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(source)
        .write_to(&mut encoded, image::ImageFormat::Png)
        .unwrap();
    let image_bytes = encoded.into_inner();
    let resource = Resource::new("scene.png", "image/png", image_bytes.clone()).unwrap();
    thinoda_core::images::set_image(sheet, image_card, Some((resource, Vec2::new(120.0, 100.0))))
        .unwrap();
    wb.validate().unwrap();
    (wb, image_bytes)
}

#[test]
fn storyboard_image_card_round_trips_and_exports_its_embedded_image() {
    let (wb, image_bytes) = storyboard_with_image();
    let native = export(&wb, Format::Thinoda);
    let restored = import_bytes(Format::Thinoda, &native).unwrap();
    assert_eq!(restored, wb);
    assert_eq!(
        restored
            .active()
            .resources
            .entries
            .values()
            .next()
            .unwrap()
            .bytes(),
        image_bytes.as_slice()
    );
    for dark in [false, true] {
        let options = ExportOptions {
            dark,
            ..Default::default()
        };
        let svg =
            String::from_utf8(export_bytes_with_consent(&wb, Format::Svg, &options, true).unwrap())
                .unwrap();
        assert!(svg.contains("data:image/png;base64,"));
        for name in ["Scene 1", "Scene 2", "Scene 3"] {
            assert!(svg.contains(name), "missing {name}");
        }
        let png = image::load_from_memory(
            &export_bytes_with_consent(&wb, Format::Png, &options, true).unwrap(),
        )
        .unwrap()
        .to_rgb8();
        assert!(png.pixels().any(|pixel| pixel.0 == [18, 196, 120]));
        assert!(
            export_bytes_with_consent(&wb, Format::Pdf, &options, true)
                .unwrap()
                .starts_with(b"%PDF")
        );
    }
}

#[test]
fn moodboard_round_trips_two_images_and_exports_them_in_both_themes() {
    let mut wb = thinoda_core::templates::instantiate("moodboard");
    let sheet = wb.active_mut();
    let mut originals = Vec::new();
    for (title, color) in [("Image 2", [18, 196, 120]), ("Image 5", [221, 80, 145])] {
        let id = sheet
            .content_topics()
            .find(|topic| topic.title == title)
            .unwrap()
            .id;
        let source = image::RgbImage::from_pixel(8, 8, image::Rgb(color));
        let mut encoded = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(source)
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let bytes = encoded.into_inner();
        let resource = Resource::new(&format!("{title}.png"), "image/png", bytes.clone()).unwrap();
        thinoda_core::images::set_image(sheet, id, Some((resource, Vec2::new(120.0, 100.0))))
            .unwrap();
        originals.push((id, bytes, color));
    }
    wb.validate().unwrap();
    let native = export(&wb, Format::Thinoda);
    let restored = import_bytes(Format::Thinoda, &native).unwrap();
    assert_eq!(restored, wb);
    for (id, bytes, _) in &originals {
        let sheet = restored.active();
        let reference = sheet.topic(*id).unwrap().image.as_deref().unwrap();
        assert_eq!(sheet.resources.get(reference).unwrap().bytes(), bytes);
    }
    for dark in [false, true] {
        let options = ExportOptions {
            dark,
            ..Default::default()
        };
        let svg =
            String::from_utf8(export_bytes_with_consent(&wb, Format::Svg, &options, true).unwrap())
                .unwrap();
        assert_eq!(svg.matches("data:image/png;base64,").count(), 2);
        let png = image::load_from_memory(
            &export_bytes_with_consent(&wb, Format::Png, &options, true).unwrap(),
        )
        .unwrap()
        .to_rgb8();
        for (_, _, color) in &originals {
            assert!(png.pixels().any(|pixel| pixel.0 == *color), "{color:?}");
        }
        assert!(
            export_bytes_with_consent(&wb, Format::Pdf, &options, true)
                .unwrap()
                .starts_with(b"%PDF")
        );
    }
}

#[test]
#[ignore = "requires Python 3, pypdfium2 and Pillow"]
fn storyboard_pdf_rasterizes_scenes_and_embedded_image_in_both_themes() {
    use std::process::{Command, Stdio};
    use thinoda_core::theme::Theme;

    fn rasterize(pdf: &[u8]) -> image::RgbImage {
        let script = r#"
import sys
import pypdfium2 as pdfium
document = pdfium.PdfDocument(sys.stdin.buffer.read())
assert len(document) == 1, f"expected one page, got {len(document)}"
document[0].render(scale=2).to_pil().convert("RGB").save(sys.stdout.buffer, format="PNG")
"#;
        let mut child = Command::new("python")
            .args(["-c", script])
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
        image::load_from_memory(&output.stdout).unwrap().to_rgb8()
    }

    fn sample(image: &image::RgbImage, view: [f32; 4], x: f32, y: f32) -> [u8; 3] {
        let px = ((x - view[0]) / view[2] * image.width() as f32).floor() as i32;
        let py = ((y - view[1]) / view[3] * image.height() as f32).floor() as i32;
        assert!(
            px >= 0 && py >= 0 && px < image.width() as i32 && py < image.height() as i32,
            "point ({x}, {y}) is outside exported page {view:?}"
        );
        image.get_pixel(px as u32, py as u32).0
    }

    fn near(actual: [u8; 3], expected: [u8; 3], tolerance: u8) -> bool {
        actual
            .iter()
            .zip(expected)
            .all(|(&a, b)| a.abs_diff(b) <= tolerance)
    }

    let (wb, _) = storyboard_with_image();
    let sheet = wb.active();
    assert_eq!(sheet.zones.len(), 3);
    for dark in [false, true] {
        let options = ExportOptions {
            dark,
            ..Default::default()
        };
        let svg =
            String::from_utf8(export_bytes_with_consent(&wb, Format::Svg, &options, true).unwrap())
                .unwrap();
        let view = view_box(&svg);
        let image =
            rasterize(&export_bytes_with_consent(&wb, Format::Pdf, &options, true).unwrap());
        let bg = Theme::for_settings(&sheet.theme).background(&sheet.theme, dark);
        for (index, zone) in sheet.zones.iter().enumerate() {
            assert!(svg.contains(&zone.name), "missing {}", zone.name);
            let tint = zone.color.unwrap();
            let fill = Color::rgba(tint.r, tint.g, tint.b, 20).over(bg);
            let expected = [fill.r, fill.g, fill.b];
            let frame = zone.frame;
            let x = frame.min.x + 10.0;
            let y = frame.min.y + 90.0;
            let actual = sample(&image, view, x, y);
            assert!(
                near(actual, expected, 3),
                "scene {} background in dark={dark}: {actual:?} vs {expected:?}",
                index + 1
            );
            let label_pixels = (5..20)
                .flat_map(|dy| (8..160).map(move |dx| (dx, dy)))
                .filter(|&(dx, dy)| {
                    !near(
                        sample(
                            &image,
                            view,
                            frame.min.x + dx as f32,
                            frame.min.y + dy as f32,
                        ),
                        expected,
                        60,
                    )
                })
                .count();
            assert!(label_pixels > 10, "scene {} title is missing", index + 1);
        }

        for (x, expected) in [
            (115.0, [0xFF, 0xE5, 0x8F]),
            (495.0, [0xA5, 0xD8, 0xFF]),
            (875.0, [0xB7, 0xEB, 0x8F]),
        ] {
            let actual = sample(&image, view, x, 330.0);
            assert!(
                near(actual, expected, 3),
                "action note at x={x} in dark={dark}: {actual:?} vs {expected:?}"
            );
        }

        let first = sheet.zones[0].frame;
        let mut image_pixels = 0;
        for (x, y, pixel) in image.enumerate_pixels() {
            if near(pixel.0, [18, 196, 120], 10) {
                let world_x = view[0] + (x as f32 + 0.5) * view[2] / image.width() as f32;
                let world_y = view[1] + (y as f32 + 0.5) * view[3] / image.height() as f32;
                assert!(
                    world_x >= first.min.x
                        && world_x <= first.max().x
                        && world_y >= first.min.y
                        && world_y <= first.max().y,
                    "embedded image escaped the first scene"
                );
                image_pixels += 1;
            }
        }
        assert!(
            image_pixels > 1_000,
            "embedded image is missing in dark={dark}"
        );
    }
}

#[test]
fn user_journey_map_native_round_trip_retains_independent_edits() {
    use thinoda_core::board::BoardMember;

    let mut document =
        thinoda_core::Document::new(thinoda_core::templates::instantiate("user-journey-map"));
    let original = document.workbook().clone();
    let prompt = document
        .sheet()
        .content_topics()
        .find(|topic| topic.title == "What do they do?")
        .unwrap()
        .id;
    document
        .move_board_members(&[BoardMember::Card(prompt)], Vec2::new(340.0, 0.0))
        .unwrap();
    document
        .edit_sheet("Edit journey prompt", |sheet| {
            sheet.set_title(prompt, "Research the product")
        })
        .unwrap();
    let edited = document.workbook().clone();
    let saved = export(&edited, Format::Thinoda);
    let restored = import_bytes(Format::Thinoda, &saved).unwrap();
    restored.validate().unwrap();
    assert_eq!(restored, edited);
    assert_eq!(restored.active().zones, original.active().zones);
    assert!(
        restored
            .active()
            .zones
            .iter()
            .all(|zone| zone.keep_empty && zone.roots.is_empty())
    );
    assert!(document.undo());
    assert!(document.undo());
    assert_eq!(document.workbook(), &original);
    assert!(document.redo());
    assert!(document.redo());
    assert_eq!(document.workbook(), &restored);
}

#[test]
fn section_boards_export_frames_and_prompt_cards() {
    for id in ["empathy-map", "user-journey-map"] {
        let wb = thinoda_core::templates::instantiate(id);
        let sheet = wb.active();
        for dark in [false, true] {
            let options = ExportOptions {
                dark,
                ..Default::default()
            };
            let svg = String::from_utf8(
                export_bytes_with_consent(&wb, Format::Svg, &options, true).unwrap(),
            )
            .unwrap();
            let [x, y, w, h] = view_box(&svg);
            for zone in &sheet.zones {
                assert!(svg.contains(&zone.name), "missing section {}", zone.name);
                assert!(x <= zone.frame.min.x && y <= zone.frame.min.y);
                assert!(x + w >= zone.frame.max().x && y + h >= zone.frame.max().y);
            }
            for topic in sheet.content_topics() {
                let prefix = format!("<text id=\"topic-{}-line-", topic.id.0);
                let rendered: String = svg
                    .match_indices(&prefix)
                    .flat_map(|(start, _)| {
                        svg[start..]
                            .split_once('>')
                            .unwrap()
                            .1
                            .split_once("</text>")
                            .unwrap()
                            .0
                            .chars()
                            .filter(|character| !character.is_whitespace())
                    })
                    .collect();
                let expected: String = topic
                    .title
                    .chars()
                    .filter(|character| !character.is_whitespace())
                    .collect();
                assert_eq!(rendered, expected, "{id}: missing prompt {}", topic.title);
            }
            let png = image::load_from_memory(
                &export_bytes_with_consent(&wb, Format::Png, &options, true).unwrap(),
            )
            .unwrap()
            .to_rgb8();
            for topic in sheet.content_topics() {
                if let Some(color) = topic.card.and_then(|card| card.color) {
                    assert!(
                        png.pixels()
                            .any(|pixel| pixel.0 == [color.r, color.g, color.b]),
                        "{id}: missing sticky fill for {}",
                        topic.title
                    );
                }
            }
            assert!(
                export_bytes_with_consent(&wb, Format::Pdf, &options, true)
                    .unwrap()
                    .starts_with(b"%PDF")
            );
        }
    }
}

#[test]
fn venn_exports_translucent_intersections_in_light_and_dark() {
    let wb = thinoda_core::templates::instantiate("venn-diagram");
    let sheet = wb.active();
    for dark in [false, true] {
        let options = ExportOptions {
            dark,
            ..Default::default()
        };
        let svg =
            String::from_utf8(export_bytes_with_consent(&wb, Format::Svg, &options, true).unwrap())
                .unwrap();
        let [x, y, w, h] = view_box(&svg);
        let png = image::load_from_memory(
            &export_bytes_with_consent(&wb, Format::Png, &options, true).unwrap(),
        )
        .unwrap()
        .to_rgb8();
        let background =
            thinoda_core::theme::Theme::for_settings(&sheet.theme).background(&sheet.theme, dark);
        let fills: Vec<_> = sheet
            .canvas
            .objects
            .iter()
            .map(|circle| {
                let fill = circle.style.fill.unwrap();
                Color::rgba(
                    fill.r,
                    fill.g,
                    fill.b,
                    (f32::from(fill.a) * circle.style.opacity).round() as u8,
                )
            })
            .collect();
        for (circle, fill) in sheet.canvas.objects.iter().zip(&fills) {
            assert!(svg.contains(&fill.to_hex()));
            let object = svg.find(&format!("id=\"object-{}\"", circle.id.0)).unwrap();
            assert!(object < svg.find("id=\"topic-").unwrap());
        }
        let samples = [
            (Vec2::new(180.0, 360.0), fills[0].over(background)),
            (Vec2::new(600.0, 360.0), fills[1].over(background)),
            (
                Vec2::new(390.0, 360.0),
                fills[1].over(fills[0].over(background)),
            ),
        ];
        for (point, expected) in samples {
            let pixel = png.get_pixel(
                ((point.x - x) / w * png.width() as f32) as u32,
                ((point.y - y) / h * png.height() as f32) as u32,
            );
            for (actual, expected) in pixel
                .0
                .into_iter()
                .zip([expected.r, expected.g, expected.b])
            {
                assert!(
                    actual.abs_diff(expected) <= 2,
                    "dark={dark}, {point:?}: {pixel:?} differs from {expected:?}"
                );
            }
        }
        assert!(
            export_bytes_with_consent(&wb, Format::Pdf, &options, true)
                .unwrap()
                .starts_with(b"%PDF")
        );
    }
}

#[test]
fn svg_rotates_an_offset_shape_label_around_the_shape_center() {
    let mut wb = board();
    let mut triangle = CanvasObject::shape(
        ShapeKind::Triangle,
        Rect::from_min_size(Vec2::new(120.0, 290.0), Vec2::new(180.0, 130.0)),
    );
    if let CanvasObjectKind::Shape { label, .. } = &mut triangle.kind {
        *label = "Triangle".into();
    }
    triangle.rotation = 90.0;
    wb.sheets[0].add_canvas_object(triangle).unwrap();
    let svg = String::from_utf8(export(&wb, Format::Svg)).unwrap();
    assert!(svg.contains("<g transform=\"rotate(90.00 210.00 355.00)\">"));
    assert!(svg.contains("<text x=\"210.00\" y=\"381.00\""));
    assert!(svg.contains("Triangle</text>"));
}

#[test]
fn below_band_objects_paint_before_topics() {
    let mut wb = board();
    let below = {
        let mut o = CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::new(-50.0, -50.0), Vec2::new(500.0, 500.0)),
        );
        o.band = thinoda_core::board::Band::Below;
        wb.sheets[0].add_canvas_object(o).unwrap()
    };
    let svg = String::from_utf8(export(&wb, Format::Svg)).unwrap();
    let below_at = svg.find(&format!("id=\"object-{}\"", below.0)).unwrap();
    assert!(below_at < svg.find("id=\"topic-").unwrap());
}

#[test]
fn central_svg_without_board_content_is_unchanged() {
    let wb = central();
    let svg = String::from_utf8(export(&wb, Format::Svg)).unwrap();
    assert!(!svg.contains("canvas-object"));
    assert!(!svg.contains("class=\"card\""));
    // Frame is the layout bounds plus padding, as before.
    let [_, _, w, h] = view_box(&svg);
    assert!(w > 0.0 && h > 0.0 && w < 2000.0 && h < 2000.0);
}

#[test]
fn raster_pdf_and_powerpoint_render_boards() {
    let wb = board();
    let png = export(&wb, Format::Png);
    assert!(png.starts_with(b"\x89PNG"));
    let jpeg = export(&wb, Format::Jpeg);
    assert!(jpeg.starts_with(&[0xFF, 0xD8]));
    let pdf = export(&wb, Format::Pdf);
    assert!(pdf.starts_with(b"%PDF"));
    let pptx = export(&wb, Format::PowerPoint);
    let text = all_text(&pptx);
    assert!(
        text.contains("Flattened viewport scene"),
        "viewport picture missing in pptx"
    );
    let image = image::load_from_memory(&entry(&pptx, "ppt/media/slide1-viewport.png"))
        .unwrap()
        .to_rgb8();
    assert!(
        image.pixels().any(|pixel| pixel.0 == [0x33, 0x99, 0xFF]),
        "board shape missing from the viewport picture"
    );
    assert!(!text.contains(ANCHOR));
}

#[test]
fn board_only_sheet_without_topics_still_exports() {
    let mut s = Sheet::new("Empty", ANCHOR);
    s.root_mode = SheetRootMode::Hidden;
    s.add_canvas_object(CanvasObject::shape(
        ShapeKind::Ellipse,
        Rect::from_min_size(Vec2::new(1000.0, 1000.0), Vec2::new(100.0, 60.0)),
    ))
    .unwrap();
    let wb = Workbook::new(s);
    let svg = String::from_utf8(export(&wb, Format::Svg)).unwrap();
    let [x, y, w, h] = view_box(&svg);
    assert!(
        x > 900.0 && y > 900.0 && w < 200.0 && h < 200.0,
        "{x} {y} {w} {h}"
    );
    assert!(!svg.contains(ANCHOR));
    assert!(export(&wb, Format::Png).starts_with(b"\x89PNG"));
    let markdown = String::from_utf8(export(&wb, Format::Markdown)).unwrap();
    assert!(markdown.contains("Empty") && !markdown.contains(ANCHOR));
}

// --- XMind ----------------------------------------------------------------------------

#[test]
fn xmind_hidden_root_is_titled_with_the_sheet_and_round_trips() {
    let wb = board();
    let bytes = export(&wb, Format::XMind);
    let content: serde_json::Value =
        serde_json::from_slice(&entry(&bytes, "content.json")).unwrap();
    let root = &content[0]["rootTopic"];
    assert_eq!(root["title"], "Board");
    let public = String::from_utf8(entry(&bytes, "content.json")).unwrap();
    assert!(!public.contains(ANCHOR));
    for title in ["Idea", "Sticky note"] {
        assert!(public.contains(title), "{title} not a detached topic");
    }
    let legacy = String::from_utf8(entry(&bytes, "content.xml")).unwrap();
    assert!(!legacy.contains(ANCHOR));
    // The private snapshot restores the full board.
    let back = import_bytes(Format::XMind, &bytes).unwrap();
    assert_eq!(back, wb);
}

#[test]
fn xmind_with_invalid_private_board_falls_back_without_panicking() {
    let wb = board();
    let bytes = export(&wb, Format::XMind);
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut entries = Vec::new();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).unwrap();
        let mut data = Vec::new();
        file.read_to_end(&mut data).unwrap();
        entries.push((file.name().unwrap().to_string(), data));
    }
    for (name, data) in &mut entries {
        if name == "metadata.json" {
            let mut meta: serde_json::Value = serde_json::from_slice(data).unwrap();
            let mut snapshot: serde_json::Value =
                serde_json::from_str(meta["thinodaWorkbook"].as_str().unwrap()).unwrap();
            let objects = snapshot["sheets"][0]["canvas"]["objects"]
                .as_array_mut()
                .unwrap();
            let copy = objects[0].clone();
            objects.push(copy);
            meta["thinodaWorkbook"] = serde_json::Value::String(snapshot.to_string());
            *data = serde_json::to_vec(&meta).unwrap();
        }
    }
    let refs: Vec<(&str, &[u8])> = entries
        .iter()
        .map(|(n, d)| (n.as_str(), d.as_slice()))
        .collect();
    let back = import_bytes(Format::XMind, &zip_with(&refs)).unwrap();
    back.validate().unwrap();
    assert!(back.sheets[0].canvas.is_empty());
}
