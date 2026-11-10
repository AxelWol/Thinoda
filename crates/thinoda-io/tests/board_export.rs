//! Freeform board export (phase F0-IO): native versioning, loss report, outline
//! projection, visual scene painting and XMind root handling.

use std::io::{Cursor, Read, Write};
use thinoda_core::{
    CanvasConnector, CanvasObject, Card, CardKind, ConnectorMarker, ConnectorMarkerKind,
    ConnectorPort, DrawioRectangleStencil, Endpoint, InkStroke, InkTool, PresentationSource,
    ShapeKind, ShapeRounding, ShapeRoundingMode, Sheet, SheetRootMode, TopicShape, Workbook,
    board::{CanvasObjectKind, RootProjection},
    geometry::{Color, Rect, Vec2},
    outline::{project_to_outline, to_outline},
    resources::Resource,
};
use thinoda_io::{
    ExportLoss, ExportOptions, Format, IoError, export_bytes, export_bytes_with_consent,
    export_losses, import_bytes, import_bytes_with_report, outline_sheet,
    render_presentation_slide_svg, required_native_version, uses_board_semantics,
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
fn connector_markers_require_v11_and_preserve_private_xmind_content() {
    let mut sheet = Sheet::new("Markers", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut connector = CanvasConnector::new(
        Endpoint::Point(Vec2::ZERO),
        Endpoint::Point(Vec2::new(100.0, 0.0)),
    );
    connector.end_marker = Some(ConnectorMarker {
        kind: ConnectorMarkerKind::Open,
    });
    sheet
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    let mut book = Workbook::default();
    book.sheets.push(sheet);
    book.validate().unwrap();
    assert_eq!(required_native_version(&book), 11);
    let native = export(&book, Format::Thinoda);
    assert_eq!(manifest_version(&native), 11);
    assert_eq!(import_bytes(Format::Thinoda, &native).unwrap(), book);
    assert_eq!(
        export(
            &import_bytes(Format::Thinoda, &native).unwrap(),
            Format::Thinoda
        ),
        native
    );
    let content = serde_json::to_vec(&book).unwrap();
    let forged = zip_with(&[
        (
            "manifest.json",
            br#"{"format":"thinoda","version":10,"resources":[]}"#,
        ),
        ("content.json", &content),
    ]);
    assert!(
        import_bytes(Format::Thinoda, &forged)
            .unwrap_err()
            .to_string()
            .contains("connector markers require native format version 11")
    );
    assert!(
        import_bytes(Format::Thinoda, &content)
            .unwrap_err()
            .to_string()
            .contains("connector markers require a versioned native archive")
    );
    let xmind = export(&book, Format::XMind);
    assert_eq!(import_bytes(Format::XMind, &xmind).unwrap(), book);
    assert!(
        String::from_utf8(entry(&xmind, "metadata.json"))
            .unwrap()
            .contains("THINODA-XMIND-MARKERS-1")
    );
}

#[test]
fn classic_and_thin_markers_require_v12_and_extended_xmind_snapshots() {
    for kind in [
        ConnectorMarkerKind::Classic,
        ConnectorMarkerKind::ClassicThin,
        ConnectorMarkerKind::OpenThin,
        ConnectorMarkerKind::BlockThin,
    ] {
        let mut sheet = Sheet::new("Markers", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.end_marker = Some(ConnectorMarker { kind });
        sheet
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let mut book = Workbook::default();
        book.sheets.push(sheet);
        assert_eq!(required_native_version(&book), 12);
        let native = export(&book, Format::Thinoda);
        assert_eq!(manifest_version(&native), 12);
        assert_eq!(import_bytes(Format::Thinoda, &native).unwrap(), book);
        let content = serde_json::to_vec(&book).unwrap();
        let forged = zip_with(&[
            (
                "manifest.json",
                br#"{"format":"thinoda","version":11,"resources":[]}"#,
            ),
            ("content.json", &content),
        ]);
        assert!(
            import_bytes(Format::Thinoda, &forged)
                .unwrap_err()
                .to_string()
                .contains("classic and thin connector markers require native format version 12")
        );
        let xmind = export(&book, Format::XMind);
        assert_eq!(import_bytes(Format::XMind, &xmind).unwrap(), book);
        assert!(
            String::from_utf8(entry(&xmind, "metadata.json"))
                .unwrap()
                .contains("THINODA-XMIND-MARKERS-2")
        );
    }
}

#[test]
fn marker_sizes_require_v13_and_size_aware_xmind_snapshots() {
    let mut sheet = Sheet::new("Sized markers", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut connector = CanvasConnector::new(
        Endpoint::Point(Vec2::ZERO),
        Endpoint::Point(Vec2::new(100.0, 0.0)),
    );
    connector.end_marker = Some(ConnectorMarker {
        kind: ConnectorMarkerKind::Open,
    });
    connector.end_marker_size = Some(12.5);
    sheet
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    let book = Workbook::new(sheet);
    assert_eq!(required_native_version(&book), 13);
    let native = export(&book, Format::Thinoda);
    assert_eq!(manifest_version(&native), 13);
    assert_eq!(import_bytes(Format::Thinoda, &native).unwrap(), book);
    let content = serde_json::to_vec(&book).unwrap();
    let forged = zip_with(&[
        (
            "manifest.json",
            br#"{"format":"thinoda","version":12,"resources":[]}"#,
        ),
        ("content.json", &content),
    ]);
    assert!(
        import_bytes(Format::Thinoda, &forged)
            .unwrap_err()
            .to_string()
            .contains("connector marker sizes require native format version 13")
    );
    let xmind = export(&book, Format::XMind);
    assert_eq!(import_bytes(Format::XMind, &xmind).unwrap(), book);
    assert!(
        String::from_utf8(entry(&xmind, "metadata.json"))
            .unwrap()
            .contains("THINODA-XMIND-MARKERS-3")
    );
}

#[test]
fn hollow_markers_require_v14_and_fill_aware_xmind_snapshots() {
    let mut sheet = Sheet::new("Hollow markers", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut connector = CanvasConnector::new(
        Endpoint::Point(Vec2::ZERO),
        Endpoint::Point(Vec2::new(100.0, 0.0)),
    );
    connector.arrow_start = true;
    connector.start_marker = Some(ConnectorMarker {
        kind: ConnectorMarkerKind::Block,
    });
    connector.start_marker_size = Some(12.5);
    connector.start_marker_unfilled = true;
    connector.end_marker = Some(ConnectorMarker {
        kind: ConnectorMarkerKind::Classic,
    });
    connector.end_marker_size = Some(12.5);
    connector.end_marker_unfilled = true;
    sheet
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    let book = Workbook::new(sheet);
    assert_eq!(required_native_version(&book), 14);
    let native = export(&book, Format::Thinoda);
    assert_eq!(manifest_version(&native), 14);
    assert_eq!(import_bytes(Format::Thinoda, &native).unwrap(), book);
    let content = serde_json::to_vec(&book).unwrap();
    let forged = zip_with(&[
        (
            "manifest.json",
            br#"{"format":"thinoda","version":13,"resources":[]}"#,
        ),
        ("content.json", &content),
    ]);
    assert!(
        import_bytes(Format::Thinoda, &forged)
            .unwrap_err()
            .to_string()
            .contains("connector marker fills require native format version 14")
    );
    let xmind = export(&book, Format::XMind);
    assert_eq!(import_bytes(Format::XMind, &xmind).unwrap(), book);
    assert!(
        String::from_utf8(entry(&xmind, "metadata.json"))
            .unwrap()
            .contains("THINODA-XMIND-MARKERS-4")
    );
}

#[test]
fn diamond_markers_require_v15_and_diamond_aware_xmind_snapshots() {
    for kind in [
        ConnectorMarkerKind::Diamond,
        ConnectorMarkerKind::DiamondThin,
    ] {
        let mut sheet = Sheet::new("Diamond markers", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.end_marker = Some(ConnectorMarker { kind });
        connector.end_marker_size = Some(12.5);
        connector.end_marker_unfilled = true;
        sheet
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let book = Workbook::new(sheet);
        assert_eq!(required_native_version(&book), 15);
        let native = export(&book, Format::Thinoda);
        assert_eq!(manifest_version(&native), 15);
        assert_eq!(import_bytes(Format::Thinoda, &native).unwrap(), book);
        let content = serde_json::to_vec(&book).unwrap();
        let forged = zip_with(&[
            (
                "manifest.json",
                br#"{"format":"thinoda","version":14,"resources":[]}"#,
            ),
            ("content.json", &content),
        ]);
        assert!(
            import_bytes(Format::Thinoda, &forged)
                .unwrap_err()
                .to_string()
                .contains("diamond connector markers require native format version 15")
        );
        let xmind = export(&book, Format::XMind);
        assert_eq!(import_bytes(Format::XMind, &xmind).unwrap(), book);
        assert!(
            String::from_utf8(entry(&xmind, "metadata.json"))
                .unwrap()
                .contains("THINODA-XMIND-MARKERS-5")
        );
    }
}

#[test]
fn oval_markers_require_v16_and_oval_aware_xmind_snapshots() {
    let (mut book, _) = oval_marker_book();
    let CanvasObjectKind::Connector(connector) = &mut book.sheets[0].canvas.objects[0].kind else {
        panic!("expected connector");
    };
    connector.start_marker = Some(ConnectorMarker {
        kind: ConnectorMarkerKind::Diamond,
    });
    assert_eq!(required_native_version(&book), 16);
    let native = export(&book, Format::Thinoda);
    assert_eq!(manifest_version(&native), 16);
    assert_eq!(import_bytes(Format::Thinoda, &native).unwrap(), book);
    let content = serde_json::to_vec(&book).unwrap();
    let forged = zip_with(&[
        (
            "manifest.json",
            br#"{"format":"thinoda","version":15,"resources":[]}"#,
        ),
        ("content.json", &content),
    ]);
    assert!(
        import_bytes(Format::Thinoda, &forged)
            .unwrap_err()
            .to_string()
            .contains("oval connector markers require native format version 16")
    );
    let xmind = export(&book, Format::XMind);
    assert_eq!(import_bytes(Format::XMind, &xmind).unwrap(), book);
    assert!(
        String::from_utf8(entry(&xmind, "metadata.json"))
            .unwrap()
            .contains("THINODA-XMIND-MARKERS-6")
    );
}

#[test]
fn half_circle_markers_require_v17_and_aware_xmind_snapshots() {
    let (mut book, _) = half_circle_marker_book();
    let CanvasObjectKind::Connector(connector) = &mut book.sheets[0].canvas.objects[0].kind else {
        panic!("expected connector");
    };
    connector.start_marker = Some(ConnectorMarker {
        kind: ConnectorMarkerKind::Oval,
    });
    assert_eq!(required_native_version(&book), 17);
    let native = export(&book, Format::Thinoda);
    assert_eq!(manifest_version(&native), 17);
    assert_eq!(import_bytes(Format::Thinoda, &native).unwrap(), book);
    let content = serde_json::to_vec(&book).unwrap();
    let forged = zip_with(&[
        (
            "manifest.json",
            br#"{"format":"thinoda","version":16,"resources":[]}"#,
        ),
        ("content.json", &content),
    ]);
    assert!(
        import_bytes(Format::Thinoda, &forged)
            .unwrap_err()
            .to_string()
            .contains("half-circle connector markers require native format version 17")
    );
    let xmind = export(&book, Format::XMind);
    assert_eq!(import_bytes(Format::XMind, &xmind).unwrap(), book);
    assert!(
        String::from_utf8(entry(&xmind, "metadata.json"))
            .unwrap()
            .contains("THINODA-XMIND-MARKERS-7")
    );
}

#[test]
fn bar_markers_require_v18_and_aware_xmind_snapshots() {
    for kind in [ConnectorMarkerKind::BaseDash, ConnectorMarkerKind::Dash] {
        for bar_at_start in [false, true] {
            let (mut book, _) = half_circle_marker_book();
            let CanvasObjectKind::Connector(connector) = &mut book.sheets[0].canvas.objects[0].kind
            else {
                panic!("expected connector");
            };
            if bar_at_start {
                connector.start_marker = Some(ConnectorMarker { kind });
            } else {
                connector.end_marker = Some(ConnectorMarker { kind });
            }
            connector.start_marker_size = Some(0.0);
            connector.end_marker_size = Some(12.5);
            connector.start_marker_unfilled = true;
            connector.end_marker_unfilled = true;
            assert_eq!(required_native_version(&book), 18);
            let native = export(&book, Format::Thinoda);
            assert_eq!(manifest_version(&native), 18);
            assert_eq!(import_bytes(Format::Thinoda, &native).unwrap(), book);
            let content = serde_json::to_vec(&book).unwrap();
            for version in 1..18 {
                let manifest =
                    format!(r#"{{"format":"thinoda","version":{version},"resources":[]}}"#);
                let forged = zip_with(&[
                    ("manifest.json", manifest.as_bytes()),
                    ("content.json", &content),
                ]);
                let error = import_bytes(Format::Thinoda, &forged).unwrap_err();
                if version == 17 {
                    assert!(
                        error
                            .to_string()
                            .contains("bar connector markers require native format version 18")
                    );
                }
            }
            let xmind = export(&book, Format::XMind);
            assert_eq!(import_bytes(Format::XMind, &xmind).unwrap(), book);
            assert!(
                String::from_utf8(entry(&xmind, "metadata.json"))
                    .unwrap()
                    .contains("THINODA-XMIND-MARKERS-8")
            );
        }
    }
}

#[test]
fn shape_rounding_requires_v19_and_aware_xmind_snapshots() {
    let (mut book, _) = half_circle_marker_book();
    let CanvasObjectKind::Connector(connector) = &mut book.sheets[0].canvas.objects[0].kind else {
        panic!("expected connector");
    };
    connector.end_marker = Some(ConnectorMarker {
        kind: ConnectorMarkerKind::Dash,
    });
    let mut shape = CanvasObject::shape(
        ShapeKind::RoundedRectangle,
        Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
    );
    shape.rounding = Some(ShapeRounding {
        radius: 17.0,
        stencil: DrawioRectangleStencil::Containers,
        ..Default::default()
    });
    book.active_mut().add_canvas_object(shape).unwrap();
    assert_eq!(required_native_version(&book), 19);
    let native = export(&book, Format::Thinoda);
    assert_eq!(manifest_version(&native), 19);
    assert_eq!(import_bytes(Format::Thinoda, &native).unwrap(), book);
    let content = serde_json::to_vec(&book).unwrap();
    for version in 1..19 {
        let manifest = format!(r#"{{"format":"thinoda","version":{version},"resources":[]}}"#);
        let forged = zip_with(&[
            ("manifest.json", manifest.as_bytes()),
            ("content.json", &content),
        ]);
        let error = import_bytes(Format::Thinoda, &forged).unwrap_err();
        if version == 18 {
            assert!(
                error
                    .to_string()
                    .contains("shape rounding requires native format version 19")
            );
        }
    }
    assert!(import_bytes(Format::Thinoda, &content).is_err());
    let xmind = export(&book, Format::XMind);
    assert_eq!(import_bytes(Format::XMind, &xmind).unwrap(), book);
    assert!(
        String::from_utf8(entry(&xmind, "metadata.json"))
            .unwrap()
            .contains("THINODA-XMIND-ROUNDING-1")
    );
}

#[test]
fn drawio_rectangle_rounding_requires_v23_and_aware_xmind_snapshots() {
    let (mut book, _) = half_circle_marker_book();
    let mut shape = CanvasObject::shape(
        ShapeKind::RoundedRectangle,
        Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
    );
    shape.rounding = Some(ShapeRounding {
        radius: 15.5,
        stencil: DrawioRectangleStencil::Standard,
        mode: ShapeRoundingMode::Relative,
        rounded_perimeter: true,
    });
    let id = book.active_mut().add_canvas_object(shape).unwrap();
    let mut connector =
        CanvasConnector::new(Endpoint::Object(id), Endpoint::Point(Vec2::new(200.0, 0.0)));
    connector.from_port = Some(ConnectorPort {
        position: Vec2::new(1.0, 0.0),
        offset: Vec2::ZERO,
        perimeter: true,
    });
    book.active_mut()
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    assert_eq!(required_native_version(&book), 23);
    let native = export(&book, Format::Thinoda);
    assert_eq!(manifest_version(&native), 23);
    assert_eq!(import_bytes(Format::Thinoda, &native).unwrap(), book);
    let content = serde_json::to_vec(&book).unwrap();
    for version in 1..23 {
        let manifest = format!(r#"{{"format":"thinoda","version":{version},"resources":[]}}"#);
        let forged = zip_with(&[
            ("manifest.json", manifest.as_bytes()),
            ("content.json", &content),
        ]);
        let error = import_bytes(Format::Thinoda, &forged).unwrap_err();
        if version == 22 {
            assert!(
                error.to_string().contains(
                    "standard draw.io rectangle rounding requires native format version 23"
                )
            );
        }
    }
    assert!(import_bytes(Format::Thinoda, &content).is_err());
    let xmind = export(&book, Format::XMind);
    assert_eq!(import_bytes(Format::XMind, &xmind).unwrap(), book);
    assert!(
        String::from_utf8(entry(&xmind, "metadata.json"))
            .unwrap()
            .contains("THINODA-XMIND-DRAWIO-ROUNDING-1")
    );
}

#[test]
fn rectangle_sides_require_v20_and_aware_xmind_snapshots() {
    let (mut book, _) = half_circle_marker_book();
    let mut shape = CanvasObject::shape(
        ShapeKind::Rectangle,
        Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
    );
    shape.rectangle_sides = Some(thinoda_core::RectangleSides {
        top: true,
        right: false,
        bottom: true,
        left: false,
    });
    book.active_mut().add_canvas_object(shape).unwrap();
    let mut rounded = CanvasObject::shape(
        ShapeKind::RoundedRectangle,
        Rect::from_min_size(Vec2::new(110.0, 0.0), Vec2::new(100.0, 60.0)),
    );
    rounded.rounding = Some(ShapeRounding {
        radius: 17.0,
        stencil: DrawioRectangleStencil::Containers,
        ..Default::default()
    });
    book.active_mut().add_canvas_object(rounded).unwrap();
    assert_eq!(required_native_version(&book), 20);
    let native = export(&book, Format::Thinoda);
    assert_eq!(manifest_version(&native), 20);
    assert_eq!(import_bytes(Format::Thinoda, &native).unwrap(), book);
    let content = serde_json::to_vec(&book).unwrap();
    for version in 1..20 {
        let manifest = format!(r#"{{"format":"thinoda","version":{version},"resources":[]}}"#);
        let forged = zip_with(&[
            ("manifest.json", manifest.as_bytes()),
            ("content.json", &content),
        ]);
        let error = import_bytes(Format::Thinoda, &forged).unwrap_err();
        if version == 19 {
            assert!(
                error
                    .to_string()
                    .contains("rectangle outline sides require native format version 20")
            );
        }
    }
    assert!(import_bytes(Format::Thinoda, &content).is_err());
    let xmind = export(&book, Format::XMind);
    assert_eq!(import_bytes(Format::XMind, &xmind).unwrap(), book);
    assert!(
        String::from_utf8(entry(&xmind, "metadata.json"))
            .unwrap()
            .contains("THINODA-XMIND-RECTANGLE-SIDES-1")
    );
}

#[test]
fn fixed_connector_ports_require_v22_and_aware_xmind_snapshots() {
    let mut book = Workbook::new(Sheet::new("Ports", "Root"));
    let shape = book
        .active_mut()
        .add_canvas_object(CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
        ))
        .unwrap();
    let mut connector = CanvasConnector::new(
        Endpoint::Object(shape),
        Endpoint::Point(Vec2::new(200.0, 0.0)),
    );
    connector.from_port = Some(ConnectorPort {
        position: Vec2::new(1.0, 0.25),
        offset: Vec2::new(5.0, 0.0),
        perimeter: false,
    });
    book.active_mut()
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    assert_eq!(required_native_version(&book), 22);
    let native = export(&book, Format::Thinoda);
    assert_eq!(manifest_version(&native), 22);
    assert_eq!(import_bytes(Format::Thinoda, &native).unwrap(), book);
    let content = serde_json::to_vec(&book).unwrap();
    for version in 3..22 {
        let manifest = format!(r#"{{"format":"thinoda","version":{version},"resources":[]}}"#);
        let forged = zip_with(&[
            ("manifest.json", manifest.as_bytes()),
            ("content.json", &content),
        ]);
        assert!(import_bytes(Format::Thinoda, &forged).is_err(), "{version}");
    }
    assert!(import_bytes(Format::Thinoda, &content).is_err());
    let xmind = export(&book, Format::XMind);
    assert_eq!(import_bytes(Format::XMind, &xmind).unwrap(), book);
    assert!(
        String::from_utf8(entry(&xmind, "metadata.json"))
            .unwrap()
            .contains("THINODA-XMIND-FIXED-PORTS-1")
    );
}

#[test]
fn double_ellipses_require_v21_and_aware_xmind_snapshots() {
    for margin in [None, Some(8.5)] {
        let (mut book, _) = half_circle_marker_book();
        let mut ellipse = CanvasObject::shape(
            ShapeKind::DoubleEllipse,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
        );
        ellipse.double_ellipse_margin = margin;
        book.active_mut().add_canvas_object(ellipse).unwrap();
        let mut rectangle = CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::new(120.0, 0.0), Vec2::new(100.0, 60.0)),
        );
        rectangle.rectangle_sides = Some(thinoda_core::RectangleSides::default());
        book.active_mut().add_canvas_object(rectangle).unwrap();

        assert_eq!(required_native_version(&book), 21);
        let native = export(&book, Format::Thinoda);
        assert_eq!(manifest_version(&native), 21);
        assert_eq!(import_bytes(Format::Thinoda, &native).unwrap(), book);
        let content = serde_json::to_vec(&book).unwrap();
        let manifest = br#"{"format":"thinoda","version":20,"resources":[]}"#;
        let forged = zip_with(&[
            ("manifest.json", manifest.as_slice()),
            ("content.json", &content),
        ]);
        assert!(
            import_bytes(Format::Thinoda, &forged)
                .unwrap_err()
                .to_string()
                .contains("double ellipses require native format version 21")
        );
        for version in 3..20 {
            let manifest = format!(r#"{{"format":"thinoda","version":{version},"resources":[]}}"#);
            let forged = zip_with(&[
                ("manifest.json", manifest.as_bytes()),
                ("content.json", &content),
            ]);
            assert!(import_bytes(Format::Thinoda, &forged).is_err(), "{version}");
        }
        assert!(import_bytes(Format::Thinoda, &content).is_err());
        for value in [serde_json::json!(-1), serde_json::json!("invalid")] {
            let mut malformed = serde_json::to_value(&book).unwrap();
            malformed["sheets"][0]["canvas"]["objects"][0]["double_ellipse_margin"] = value;
            let malformed = serde_json::to_vec(&malformed).unwrap();
            let manifest = br#"{"format":"thinoda","version":21,"resources":[]}"#;
            let forged = zip_with(&[
                ("manifest.json", manifest.as_slice()),
                ("content.json", &malformed),
            ]);
            assert!(import_bytes(Format::Thinoda, &forged).is_err());
        }
        let xmind = export(&book, Format::XMind);
        assert_eq!(import_bytes(Format::XMind, &xmind).unwrap(), book);
        assert!(
            String::from_utf8(entry(&xmind, "metadata.json"))
                .unwrap()
                .contains("THINODA-XMIND-DOUBLE-ELLIPSE-1")
        );
    }
}

#[test]
fn double_ellipse_renders_in_svg_raster_pdf_and_powerpoint() {
    let mut sheet = Sheet::new("Double ellipse", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut shape = CanvasObject::shape(
        ShapeKind::DoubleEllipse,
        Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
    );
    shape.style.fill = Some(Color::WHITE);
    shape.style.stroke = Some(Color::rgb(200, 30, 40));
    shape.style.stroke_width = 4.0;
    shape.double_ellipse_margin = Some(10.0);
    let id = sheet.add_canvas_object(shape).unwrap();
    let root = sheet.add_floating("Slide", Vec2::new(50.0, 150.0));
    let frame = Rect::from_min_size(Vec2::new(-40.0, -40.0), Vec2::new(180.0, 220.0));
    sheet
        .add_presentation_slide("Editable", PresentationSource::Branch(root), frame)
        .unwrap();
    sheet
        .add_presentation_slide("Viewport", PresentationSource::Viewport, frame)
        .unwrap();
    let double = Workbook::new(sheet);
    let mut collapsed = double.clone();
    collapsed
        .active_mut()
        .canvas
        .object_mut(id)
        .unwrap()
        .double_ellipse_margin = Some(30.0);
    let double_svg = String::from_utf8(export(&double, Format::Svg)).unwrap();
    let collapsed_svg = String::from_utf8(export(&collapsed, Format::Svg)).unwrap();
    assert!(double_svg.contains("M 90.00,30.00"), "{double_svg}");
    assert!(!collapsed_svg.contains("M 90.00,30.00"), "{collapsed_svg}");
    let presentation =
        render_presentation_slide_svg(double.active(), 0, &ExportOptions::default()).unwrap();
    assert!(presentation.contains("M 90.00,30.00"), "{presentation}");

    let double_png = image::load_from_memory(&export(&double, Format::Png))
        .unwrap()
        .to_rgba8();
    let collapsed_png = image::load_from_memory(&export(&collapsed, Format::Png))
        .unwrap()
        .to_rgba8();
    let view = view_box(&double_svg);
    assert_eq!(view, view_box(&collapsed_svg));
    let sample = |image: &image::RgbaImage, x: f32, y: f32| {
        let px = ((x - view[0]) / view[2] * image.width() as f32).floor() as u32;
        let py = ((y - view[1]) / view[3] * image.height() as f32).floor() as u32;
        *image.get_pixel(px, py)
    };
    assert_ne!(
        sample(&double_png, 50.0, 10.0),
        sample(&collapsed_png, 50.0, 10.0)
    );
    assert_eq!(
        sample(&double_png, 50.0, 30.0),
        sample(&collapsed_png, 50.0, 30.0)
    );
    let double_pdf = export(&double, Format::Pdf);
    let collapsed_pdf = export(&collapsed, Format::Pdf);
    assert!(double_pdf.starts_with(b"%PDF"));
    assert_ne!(double_pdf, collapsed_pdf);
    let double_pptx = export(&double, Format::PowerPoint);
    let collapsed_pptx = export(&collapsed, Format::PowerPoint);
    let double_slide = String::from_utf8(entry(&double_pptx, "ppt/slides/slide1.xml")).unwrap();
    let collapsed_slide =
        String::from_utf8(entry(&collapsed_pptx, "ppt/slides/slide1.xml")).unwrap();
    assert_eq!(
        double_slide.matches(&format!("Canvas object {id}")).count(),
        2
    );
    assert_eq!(
        collapsed_slide
            .matches(&format!("Canvas object {id}"))
            .count(),
        1
    );
    assert_ne!(
        entry(&double_pptx, "ppt/media/slide2-viewport.png"),
        entry(&collapsed_pptx, "ppt/media/slide2-viewport.png")
    );

    let mut no_stroke = double.clone();
    no_stroke
        .active_mut()
        .canvas
        .object_mut(id)
        .unwrap()
        .style
        .stroke = None;
    let mut no_stroke_collapsed = no_stroke.clone();
    no_stroke_collapsed
        .active_mut()
        .canvas
        .object_mut(id)
        .unwrap()
        .double_ellipse_margin = Some(30.0);
    assert_eq!(
        export(&no_stroke, Format::Png),
        export(&no_stroke_collapsed, Format::Png)
    );
}

#[test]
fn shape_rounding_renders_in_svg_raster_pdf_and_powerpoint() {
    let mut sheet = Sheet::new("Rounded", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut shape = CanvasObject::shape(
        ShapeKind::Rectangle,
        Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 100.0)),
    );
    shape.style.fill = Some(Color::rgb(200, 30, 40));
    shape.style.stroke = Some(Color::TRANSPARENT);
    shape.rounding = Some(ShapeRounding {
        radius: 40.0,
        stencil: DrawioRectangleStencil::Mockup,
        ..Default::default()
    });
    let id = sheet.add_canvas_object(shape).unwrap();
    let root = sheet.add_floating("Slide", Vec2::new(50.0, 150.0));
    sheet
        .add_presentation_slide(
            "Rounded",
            PresentationSource::Branch(root),
            Rect::from_min_size(Vec2::new(-40.0, -40.0), Vec2::new(180.0, 220.0)),
        )
        .unwrap();
    let rounded = Workbook::new(sheet);
    let mut square = rounded.clone();
    square
        .active_mut()
        .canvas
        .object_mut(id)
        .unwrap()
        .rounding
        .as_mut()
        .unwrap()
        .radius = 0.0;

    let rounded_svg = String::from_utf8(export(&rounded, Format::Svg)).unwrap();
    let square_svg = String::from_utf8(export(&square, Format::Svg)).unwrap();
    assert!(rounded_svg.contains("M 60.00,0.00"), "{rounded_svg}");
    assert!(square_svg.contains("M 0.00,0.00"), "{square_svg}");
    let presentation =
        render_presentation_slide_svg(rounded.active(), 0, &ExportOptions::default()).unwrap();
    assert!(presentation.contains("M 60.00,0.00"), "{presentation}");

    let rounded_png = image::load_from_memory(&export(&rounded, Format::Png))
        .unwrap()
        .to_rgba8();
    let square_png = image::load_from_memory(&export(&square, Format::Png))
        .unwrap()
        .to_rgba8();
    let view = view_box(&rounded_svg);
    assert_eq!(view, view_box(&square_svg));
    let sample = |image: &image::RgbaImage, x: f32, y: f32| {
        let px = ((x - view[0]) / view[2] * image.width() as f32).floor() as u32;
        let py = ((y - view[1]) / view[3] * image.height() as f32).floor() as u32;
        *image.get_pixel(px, py)
    };
    assert_ne!(
        sample(&rounded_png, 3.0, 3.0),
        sample(&square_png, 3.0, 3.0)
    );
    assert_eq!(
        sample(&rounded_png, 50.0, 50.0),
        sample(&square_png, 50.0, 50.0)
    );
    let rounded_pdf = export(&rounded, Format::Pdf);
    let square_pdf = export(&square, Format::Pdf);
    assert!(rounded_pdf.starts_with(b"%PDF"));
    assert_ne!(rounded_pdf, square_pdf);
    let rounded_pptx = export(&rounded, Format::PowerPoint);
    let square_pptx = export(&square, Format::PowerPoint);
    let rounded_slide = String::from_utf8(entry(&rounded_pptx, "ppt/slides/slide1.xml")).unwrap();
    let square_slide = String::from_utf8(entry(&square_pptx, "ppt/slides/slide1.xml")).unwrap();
    let shape_path = |slide: &str| {
        slide
            .split("<p:sp>")
            .find(|fragment| fragment.contains(&format!("Canvas object {id}")))
            .unwrap()
            .to_owned()
    };
    assert!(
        shape_path(&rounded_slide).matches("<a:lnTo>").count()
            > shape_path(&square_slide).matches("<a:lnTo>").count()
    );
}

#[test]
fn drawio_standard_rounding_renders_across_exporters() {
    let mut sheet = Sheet::new("Rounded", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut shape = CanvasObject::shape(
        ShapeKind::RoundedRectangle,
        Rect::from_min_size(Vec2::ZERO, Vec2::new(600.0, 40.0)),
    );
    shape.style.fill = Some(Color::rgb(200, 30, 40));
    shape.style.stroke = Some(Color::TRANSPARENT);
    shape.rounding = Some(ShapeRounding {
        radius: 15.0,
        stencil: DrawioRectangleStencil::Standard,
        mode: ShapeRoundingMode::Relative,
        rounded_perimeter: false,
    });
    let id = sheet.add_canvas_object(shape).unwrap();
    let root = sheet.add_floating("Slide", Vec2::new(300.0, 150.0));
    sheet
        .add_presentation_slide(
            "Rounded",
            PresentationSource::Branch(root),
            Rect::from_min_size(Vec2::new(-40.0, -40.0), Vec2::new(680.0, 220.0)),
        )
        .unwrap();
    let rounded = Workbook::new(sheet);
    let mut native = rounded.clone();
    native.active_mut().canvas.object_mut(id).unwrap().rounding = None;
    let rounded_svg = String::from_utf8(export(&rounded, Format::Svg)).unwrap();
    let native_svg = String::from_utf8(export(&native, Format::Svg)).unwrap();
    assert!(rounded_svg.contains("M 594.00,0.00"), "{rounded_svg}");
    assert!(native_svg.contains("M 510.00,0.00"), "{native_svg}");
    let presentation =
        render_presentation_slide_svg(rounded.active(), 0, &ExportOptions::default()).unwrap();
    assert!(presentation.contains("M 594.00,0.00"), "{presentation}");
    let rounded_png = image::load_from_memory(&export(&rounded, Format::Png))
        .unwrap()
        .to_rgba8();
    let native_png = image::load_from_memory(&export(&native, Format::Png))
        .unwrap()
        .to_rgba8();
    let view = view_box(&rounded_svg);
    assert_eq!(view, view_box(&native_svg));
    let sample = |image: &image::RgbaImage, x: f32, y: f32| {
        let px = ((x - view[0]) / view[2] * image.width() as f32).floor() as u32;
        let py = ((y - view[1]) / view[3] * image.height() as f32).floor() as u32;
        *image.get_pixel(px, py)
    };
    assert_ne!(
        sample(&rounded_png, 20.0, 2.0),
        sample(&native_png, 20.0, 2.0)
    );
    assert_eq!(
        sample(&rounded_png, 300.0, 20.0),
        sample(&native_png, 300.0, 20.0)
    );
    let rounded_pdf = export(&rounded, Format::Pdf);
    assert!(rounded_pdf.starts_with(b"%PDF"));
    assert_ne!(rounded_pdf, export(&native, Format::Pdf));
    let rounded_pptx = export(&rounded, Format::PowerPoint);
    let native_pptx = export(&native, Format::PowerPoint);
    assert_ne!(
        entry(&rounded_pptx, "ppt/slides/slide1.xml"),
        entry(&native_pptx, "ppt/slides/slide1.xml")
    );
}

#[test]
fn connector_markers_render_in_svg_raster_and_presentation_exports() {
    let mut sheet = Sheet::new("Markers", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut connector = CanvasConnector::new(
        Endpoint::Point(Vec2::ZERO),
        Endpoint::Point(Vec2::new(100.0, 0.0)),
    );
    connector.arrow_start = true;
    connector.start_marker = Some(ConnectorMarker {
        kind: ConnectorMarkerKind::Block,
    });
    connector.end_marker = Some(ConnectorMarker {
        kind: ConnectorMarkerKind::Open,
    });
    connector.color = Some(Color::rgb(20, 40, 60));
    let id = sheet
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    let root = sheet.add_floating("Slide", Vec2::new(50.0, 100.0));
    sheet
        .add_presentation_slide(
            "Markers",
            PresentationSource::Branch(root),
            Rect::from_min_size(Vec2::new(-40.0, -40.0), Vec2::new(180.0, 180.0)),
        )
        .unwrap();
    let book = Workbook::new(sheet);
    let svg = String::from_utf8(export(&book, Format::Svg)).unwrap();
    let open = "M 89.76,4.00 L 97.76,0.00 L 89.76,-4.00";
    let block = "M 10.24,-4.00 L 2.24,0.00 L 10.24,4.00 Z";
    assert!(svg.contains(open), "{svg}");
    assert!(svg.contains(block), "{svg}");
    assert!(svg.contains("M 10.24,0.00 L 95.53,0.00"), "{svg}");
    let slide =
        render_presentation_slide_svg(&book.sheets[0], 0, &ExportOptions::default()).unwrap();
    assert!(slide.contains(open));
    assert!(slide.contains(block));
    let image = image::load_from_memory(&export(&book, Format::Png)).unwrap();
    assert!(image.width() > 0 && image.height() > 0);
    let powerpoint = export(&book, Format::PowerPoint);
    let slide = String::from_utf8(entry(&powerpoint, "ppt/slides/slide1.xml")).unwrap();
    let name = format!("Canvas object {id}");
    assert_eq!(slide.matches(&name).count(), 3, "{slide}");
    assert!(slide.contains("<a:close/>"), "{slide}");
}

#[test]
fn classic_markers_render_notches_in_visual_and_presentation_exports() {
    let mut sheet = Sheet::new("Markers", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut connector = CanvasConnector::new(
        Endpoint::Point(Vec2::ZERO),
        Endpoint::Point(Vec2::new(100.0, 0.0)),
    );
    connector.arrow_start = true;
    connector.start_marker = Some(ConnectorMarker {
        kind: ConnectorMarkerKind::Classic,
    });
    connector.end_marker = Some(ConnectorMarker {
        kind: ConnectorMarkerKind::ClassicThin,
    });
    let id = sheet
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    let root = sheet.add_floating("Slide", Vec2::new(50.0, 100.0));
    sheet
        .add_presentation_slide(
            "Markers",
            PresentationSource::Branch(root),
            Rect::from_min_size(Vec2::new(-40.0, -40.0), Vec2::new(180.0, 180.0)),
        )
        .unwrap();
    let book = Workbook::new(sheet);
    let svg = String::from_utf8(export(&book, Format::Svg)).unwrap();
    let classic = "M 10.24,-4.00 L 2.24,0.00 L 10.24,4.00 L 8.24,0.00 Z";
    let thin = "M 89.76,2.67 L 97.76,0.00 L 89.76,-2.67 L 91.76,0.00 Z";
    assert!(svg.contains(classic), "{svg}");
    assert!(svg.contains(thin), "{svg}");
    assert!(svg.contains("M 8.24,0.00 L 91.76,0.00"), "{svg}");
    let slide =
        render_presentation_slide_svg(&book.sheets[0], 0, &ExportOptions::default()).unwrap();
    assert!(slide.contains(classic));
    assert!(slide.contains(thin));
    let image = image::load_from_memory(&export(&book, Format::Png)).unwrap();
    assert!(image.width() > 0 && image.height() > 0);
    assert!(export(&book, Format::Pdf).starts_with(b"%PDF"));
    let powerpoint = export(&book, Format::PowerPoint);
    let slide = String::from_utf8(entry(&powerpoint, "ppt/slides/slide1.xml")).unwrap();
    assert_eq!(slide.matches(&format!("Canvas object {id}")).count(), 3);
    assert!(slide.matches("<a:close/>").count() >= 2, "{slide}");
}

#[test]
fn custom_marker_sizes_render_in_visual_and_presentation_exports() {
    let mut sheet = Sheet::new("Sized markers", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut connector = CanvasConnector::new(
        Endpoint::Point(Vec2::ZERO),
        Endpoint::Point(Vec2::new(100.0, 0.0)),
    );
    connector.arrow_start = true;
    connector.start_marker = Some(ConnectorMarker {
        kind: ConnectorMarkerKind::Block,
    });
    connector.start_marker_size = Some(0.0);
    connector.end_marker = Some(ConnectorMarker {
        kind: ConnectorMarkerKind::Classic,
    });
    connector.end_marker_size = Some(12.5);
    let id = sheet
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    let root = sheet.add_floating("Slide", Vec2::new(50.0, 100.0));
    sheet
        .add_presentation_slide(
            "Markers",
            PresentationSource::Branch(root),
            Rect::from_min_size(Vec2::new(-40.0, -40.0), Vec2::new(180.0, 180.0)),
        )
        .unwrap();
    let book = Workbook::new(sheet);
    let svg = String::from_utf8(export(&book, Format::Svg)).unwrap();
    let start = "M 4.24,-1.00 L 2.24,0.00 L 4.24,1.00 Z";
    let end = "M 83.26,7.25 L 97.76,0.00 L 83.26,-7.25 L 86.89,0.00 Z";
    assert!(svg.contains(start), "{svg}");
    assert!(svg.contains(end), "{svg}");
    assert!(svg.contains("M 4.24,0.00 L 86.89,0.00"), "{svg}");
    let slide =
        render_presentation_slide_svg(&book.sheets[0], 0, &ExportOptions::default()).unwrap();
    assert!(slide.contains(start));
    assert!(slide.contains(end));
    let sized = export(&book, Format::Png);
    let mut default = book.clone();
    let CanvasObjectKind::Connector(connector) = &mut default.sheets[0].canvas.objects[0].kind
    else {
        panic!("expected connector");
    };
    connector.start_marker_size = None;
    connector.end_marker_size = None;
    assert_ne!(sized, export(&default, Format::Png));
    assert!(export(&book, Format::Pdf).starts_with(b"%PDF"));
    let powerpoint = export(&book, Format::PowerPoint);
    let slide = String::from_utf8(entry(&powerpoint, "ppt/slides/slide1.xml")).unwrap();
    assert_eq!(slide.matches(&format!("Canvas object {id}")).count(), 3);
}

fn hollow_marker_book() -> (Workbook, thinoda_core::CanvasObjectId) {
    let mut sheet = Sheet::new("Hollow markers", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut connector = CanvasConnector::new(
        Endpoint::Point(Vec2::ZERO),
        Endpoint::Point(Vec2::new(100.0, 0.0)),
    );
    connector.arrow_start = true;
    connector.start_marker = Some(ConnectorMarker {
        kind: ConnectorMarkerKind::Block,
    });
    connector.start_marker_size = Some(12.5);
    connector.start_marker_unfilled = true;
    connector.end_marker = Some(ConnectorMarker {
        kind: ConnectorMarkerKind::Classic,
    });
    connector.end_marker_size = Some(12.5);
    connector.end_marker_unfilled = true;
    connector.color = Some(Color::rgb(20, 40, 60));
    let id = sheet
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    let root = sheet.add_floating("Slide", Vec2::new(50.0, 100.0));
    sheet
        .add_presentation_slide(
            "Markers",
            PresentationSource::Branch(root),
            Rect::from_min_size(Vec2::new(-40.0, -40.0), Vec2::new(180.0, 180.0)),
        )
        .unwrap();
    (Workbook::new(sheet), id)
}

fn diamond_marker_book() -> (Workbook, thinoda_core::CanvasObjectId) {
    let (mut book, id) = hollow_marker_book();
    let CanvasObjectKind::Connector(connector) = &mut book.sheets[0].canvas.objects[0].kind else {
        panic!("expected connector");
    };
    connector.start_marker = Some(ConnectorMarker {
        kind: ConnectorMarkerKind::Diamond,
    });
    connector.end_marker = Some(ConnectorMarker {
        kind: ConnectorMarkerKind::DiamondThin,
    });
    (book, id)
}

fn oval_marker_book() -> (Workbook, thinoda_core::CanvasObjectId) {
    let (mut book, id) = hollow_marker_book();
    let CanvasObjectKind::Connector(connector) = &mut book.sheets[0].canvas.objects[0].kind else {
        panic!("expected connector");
    };
    connector.start_marker = Some(ConnectorMarker {
        kind: ConnectorMarkerKind::Oval,
    });
    connector.end_marker = Some(ConnectorMarker {
        kind: ConnectorMarkerKind::Oval,
    });
    connector.end_marker_size = Some(8.0);
    (book, id)
}

fn half_circle_marker_book() -> (Workbook, thinoda_core::CanvasObjectId) {
    let (mut book, id) = oval_marker_book();
    let CanvasObjectKind::Connector(connector) = &mut book.sheets[0].canvas.objects[0].kind else {
        panic!("expected connector");
    };
    connector.start_marker = Some(ConnectorMarker {
        kind: ConnectorMarkerKind::HalfCircle,
    });
    connector.end_marker = Some(ConnectorMarker {
        kind: ConnectorMarkerKind::HalfCircle,
    });
    connector.end_marker_unfilled = false;
    (book, id)
}

fn bar_marker_book() -> (Workbook, thinoda_core::CanvasObjectId) {
    let (mut book, id) = oval_marker_book();
    let CanvasObjectKind::Connector(connector) = &mut book.sheets[0].canvas.objects[0].kind else {
        panic!("expected connector");
    };
    connector.start_marker = Some(ConnectorMarker {
        kind: ConnectorMarkerKind::BaseDash,
    });
    connector.end_marker = Some(ConnectorMarker {
        kind: ConnectorMarkerKind::Dash,
    });
    connector.start_marker_unfilled = true;
    connector.end_marker_unfilled = false;
    (book, id)
}

#[test]
fn bar_markers_render_in_visual_and_editable_exports() {
    let (book, id) = bar_marker_book();
    let svg = String::from_utf8(export(&book, Format::Svg)).unwrap();
    let paths = [
        r##"<path d="M 0.00,-7.75 L 0.00,7.75" fill="none" stroke="#14283C" stroke-width="2.00" stroke-linejoin="round"/>"##,
        r##"<path d="M 94.50,5.50 L 83.50,-5.50" fill="none" stroke="#14283C" stroke-width="2.00" stroke-linejoin="round"/>"##,
    ];
    let presentation =
        render_presentation_slide_svg(&book.sheets[0], 0, &ExportOptions::default()).unwrap();
    for path in paths {
        assert!(svg.contains(path), "{svg}");
        assert!(presentation.contains(path), "{presentation}");
    }
    assert!(svg.contains("M 0.00,0.00 L 100.00,0.00"), "{svg}");
    let options = ExportOptions {
        scale: 4.0,
        ..Default::default()
    };
    let png = image::load_from_memory(
        &export_bytes_with_consent(&book, Format::Png, &options, true).unwrap(),
    )
    .unwrap()
    .to_rgba8();
    let view = view_box(&svg);
    let sample = |x: f32, y: f32| {
        let px = ((x - view[0]) / view[2] * png.width() as f32).floor() as u32;
        let py = ((y - view[1]) / view[3] * png.height() as f32).floor() as u32;
        *png.get_pixel(px, py)
    };
    for (x, y) in [(0.0, 5.0), (91.75, 2.75)] {
        assert_ne!(sample(x, y), sample(x, 20.0), "{x}, {y}");
    }
    assert!(export(&book, Format::Pdf).starts_with(b"%PDF"));
    let powerpoint = export(&book, Format::PowerPoint);
    let slide = String::from_utf8(entry(&powerpoint, "ppt/slides/slide1.xml")).unwrap();
    let paths: Vec<_> = slide
        .split("<p:sp>")
        .filter(|shape| shape.contains(&format!("Canvas object {id}")))
        .collect();
    assert_eq!(paths.len(), 3, "{slide}");
    for path in &paths {
        assert_eq!(path.matches("<a:moveTo>").count(), 1);
        assert_eq!(path.matches("<a:lnTo>").count(), 1);
        assert!(!path.contains("<a:close/>"));
        assert!(path.contains(r#"fill="none""#));
        assert!(path.contains("</a:custGeom><a:noFill/><a:ln"));
    }
    let point = |shape: &str, tag: &str| {
        let (_, point) = shape.split_once(&format!(r#"<a:{tag}><a:pt x=""#)).unwrap();
        let (x, rest) = point.split_once(r#"" y=""#).unwrap();
        let (y, _) = rest.split_once('"').unwrap();
        (x.parse::<f64>().unwrap(), y.parse::<f64>().unwrap())
    };
    let start = point(paths[0], "moveTo");
    let end = point(paths[0], "lnTo");
    let scale = (end.0 - start.0) / 100.0;
    assert!(scale > 0.0);
    let expected = [
        (paths[1], (94.5, 5.5), (83.5, -5.5)),
        (paths[2], (0.0, -7.75), (0.0, 7.75)),
    ];
    for (shape, first, last) in expected {
        for (actual, expected) in [
            (point(shape, "moveTo"), first),
            (point(shape, "lnTo"), last),
        ] {
            assert!((actual.0 - (start.0 + expected.0 * scale)).abs() <= 2.0);
            assert!((actual.1 - (start.1 + expected.1 * scale)).abs() <= 2.0);
        }
    }

    let mut other_fill = book.clone();
    let CanvasObjectKind::Connector(connector) = &mut other_fill.sheets[0].canvas.objects[0].kind
    else {
        panic!("expected connector");
    };
    connector.start_marker_unfilled = false;
    connector.end_marker_unfilled = true;
    assert_eq!(
        String::from_utf8(export(&other_fill, Format::Svg)).unwrap(),
        svg
    );
}

#[test]
fn half_circle_markers_render_in_visual_and_editable_exports() {
    let (book, id) = half_circle_marker_book();
    let svg = String::from_utf8(export(&book, Format::Svg)).unwrap();
    let paths = [
        r##"<path d="M 0.00,-15.50 Q 15.50,-15.50 15.50,0.00 Q 15.50,15.50 0.00,15.50" fill="none" stroke="#14283C" stroke-width="2.00"/>"##,
        r##"<path d="M 100.00,11.00 Q 89.00,11.00 89.00,0.00 Q 89.00,-11.00 100.00,-11.00" fill="none" stroke="#14283C" stroke-width="2.00"/>"##,
    ];
    let presentation =
        render_presentation_slide_svg(&book.sheets[0], 0, &ExportOptions::default()).unwrap();
    for path in paths {
        assert!(svg.contains(path), "{svg}");
        assert!(presentation.contains(path), "{presentation}");
    }
    assert!(svg.contains("M 15.50,0.00 L 89.00,0.00"), "{svg}");
    let options = ExportOptions {
        scale: 4.0,
        ..Default::default()
    };
    let png = image::load_from_memory(
        &export_bytes_with_consent(&book, Format::Png, &options, true).unwrap(),
    )
    .unwrap()
    .to_rgba8();
    let view = view_box(&svg);
    let sample = |x: f32, y: f32| {
        let px = ((x - view[0]) / view[2] * png.width() as f32).floor() as u32;
        let py = ((y - view[1]) / view[3] * png.height() as f32).floor() as u32;
        *png.get_pixel(px, py)
    };
    for x in [0.0, 100.0] {
        assert_eq!(sample(x, 0.0), sample(x, 20.0));
    }
    for (x, y) in [(11.625, -11.625), (91.75, 8.25)] {
        assert_ne!(sample(x, y), sample(x, 20.0));
    }
    assert!(export(&book, Format::Pdf).starts_with(b"%PDF"));
    let powerpoint = export(&book, Format::PowerPoint);
    let slide = String::from_utf8(entry(&powerpoint, "ppt/slides/slide1.xml")).unwrap();
    let curves: Vec<_> = slide
        .split("<p:sp>")
        .filter(|shape| {
            shape.contains(&format!("Canvas object {id}")) && shape.contains("<a:quadBezTo>")
        })
        .collect();
    assert_eq!(curves.len(), 2, "{slide}");
    for curve in curves {
        assert_eq!(curve.matches("<a:quadBezTo>").count(), 2);
        assert!(!curve.contains("<a:close/>"));
        assert!(curve.contains(r#"fill="none""#));
        assert!(curve.contains("</a:custGeom><a:noFill/><a:ln"));
    }
}

#[test]
fn hollow_markers_render_closed_and_transparent_in_visual_exports() {
    let (book, id) = hollow_marker_book();
    let svg = String::from_utf8(export(&book, Format::Svg)).unwrap();
    let paths = [
        r##"<path d="M 16.74,-7.25 L 2.24,0.00 L 16.74,7.25 Z" fill="none" stroke="#14283C" stroke-width="2.00" stroke-linejoin="round"/>"##,
        r##"<path d="M 83.26,7.25 L 97.76,0.00 L 83.26,-7.25 L 86.89,0.00 Z" fill="none" stroke="#14283C" stroke-width="2.00" stroke-linejoin="round"/>"##,
    ];
    let slide =
        render_presentation_slide_svg(&book.sheets[0], 0, &ExportOptions::default()).unwrap();
    for path in paths {
        assert!(svg.contains(path), "{svg}");
        assert!(slide.contains(path), "{slide}");
    }

    let options = ExportOptions {
        scale: 4.0,
        ..Default::default()
    };
    let png = image::load_from_memory(
        &export_bytes_with_consent(&book, Format::Png, &options, true).unwrap(),
    )
    .unwrap()
    .to_rgba8();
    let view = view_box(&svg);
    let sample = |x: f32, y: f32| {
        let px = ((x - view[0]) / view[2] * png.width() as f32).floor() as u32;
        let py = ((y - view[1]) / view[3] * png.height() as f32).floor() as u32;
        *png.get_pixel(px, py)
    };
    for x in [7.0, 93.0] {
        assert_eq!(sample(x, 0.0), sample(x, 12.0));
    }
    let mut filled = book.clone();
    let CanvasObjectKind::Connector(connector) = &mut filled.sheets[0].canvas.objects[0].kind
    else {
        panic!("expected connector");
    };
    connector.start_marker_unfilled = false;
    connector.end_marker_unfilled = false;
    let filled_png = image::load_from_memory(
        &export_bytes_with_consent(&filled, Format::Png, &options, true).unwrap(),
    )
    .unwrap()
    .to_rgba8();
    for x in [7.0, 93.0] {
        let px = ((x - view[0]) / view[2] * filled_png.width() as f32).floor() as u32;
        let py = ((0.0 - view[1]) / view[3] * filled_png.height() as f32).floor() as u32;
        assert_ne!(*filled_png.get_pixel(px, py), sample(x, 0.0));
    }
    assert!(export(&book, Format::Pdf).starts_with(b"%PDF"));
    let powerpoint = export(&book, Format::PowerPoint);
    let slide = String::from_utf8(entry(&powerpoint, "ppt/slides/slide1.xml")).unwrap();
    assert_eq!(
        slide
            .split("<p:sp>")
            .filter(|shape| {
                shape.contains(&format!("Canvas object {id}"))
                    && shape.contains("<a:close/>")
                    && shape.contains(r#"fill="none""#)
                    && shape.contains("</a:custGeom><a:noFill/><a:ln")
            })
            .count(),
        2,
        "{slide}"
    );
}

#[test]
fn diamond_markers_render_in_visual_and_editable_exports() {
    let (book, id) = diamond_marker_book();
    let svg = String::from_utf8(export(&book, Format::Svg)).unwrap();
    let paths = [
        r##"<path d="M 8.66,-7.25 L 1.41,0.00 L 8.66,7.25 L 15.91,0.00 Z" fill="none" stroke="#14283C" stroke-width="2.00" stroke-linejoin="round"/>"##,
        r##"<path d="M 90.78,4.26 L 98.03,0.00 L 90.78,-4.26 L 83.53,0.00 Z" fill="none" stroke="#14283C" stroke-width="2.00" stroke-linejoin="round"/>"##,
    ];
    let slide =
        render_presentation_slide_svg(&book.sheets[0], 0, &ExportOptions::default()).unwrap();
    for path in paths {
        assert!(svg.contains(path), "{svg}");
        assert!(slide.contains(path), "{slide}");
    }
    assert!(svg.contains("M 15.91,0.00 L 83.53,0.00"), "{svg}");

    let options = ExportOptions {
        scale: 4.0,
        ..Default::default()
    };
    let png = image::load_from_memory(
        &export_bytes_with_consent(&book, Format::Png, &options, true).unwrap(),
    )
    .unwrap()
    .to_rgba8();
    let view = view_box(&svg);
    let sample = |image: &image::RgbaImage, x: f32, y: f32| {
        let px = ((x - view[0]) / view[2] * image.width() as f32).floor() as u32;
        let py = ((y - view[1]) / view[3] * image.height() as f32).floor() as u32;
        *image.get_pixel(px, py)
    };
    let mut filled = book.clone();
    let CanvasObjectKind::Connector(connector) = &mut filled.sheets[0].canvas.objects[0].kind
    else {
        panic!("expected connector");
    };
    connector.start_marker_unfilled = false;
    connector.end_marker_unfilled = false;
    let filled_png = image::load_from_memory(
        &export_bytes_with_consent(&filled, Format::Png, &options, true).unwrap(),
    )
    .unwrap()
    .to_rgba8();
    for x in [8.0, 91.0] {
        let background = sample(&png, x, 12.0);
        assert_eq!(sample(&png, x, 0.0), background);
        assert_ne!(sample(&filled_png, x, 0.0), background);
    }
    assert!(export(&book, Format::Pdf).starts_with(b"%PDF"));
    let powerpoint = export(&book, Format::PowerPoint);
    let slide = String::from_utf8(entry(&powerpoint, "ppt/slides/slide1.xml")).unwrap();
    assert_eq!(
        slide
            .split("<p:sp>")
            .filter(|shape| {
                shape.contains(&format!("Canvas object {id}"))
                    && shape.contains("<a:close/>")
                    && shape.contains(r#"fill="none""#)
                    && shape.contains("</a:custGeom><a:noFill/><a:ln")
            })
            .count(),
        2,
        "{slide}"
    );
    let powerpoint = export(&filled, Format::PowerPoint);
    let slide = String::from_utf8(entry(&powerpoint, "ppt/slides/slide1.xml")).unwrap();
    assert_eq!(
        slide
            .split("<p:sp>")
            .filter(|shape| {
                shape.contains(&format!("Canvas object {id}"))
                    && shape.contains("<a:close/>")
                    && shape.contains("</a:custGeom><a:solidFill>")
            })
            .count(),
        2,
        "{slide}"
    );
}

#[test]
fn oval_markers_render_in_visual_and_editable_exports() {
    let (mut book, id) = oval_marker_book();
    let CanvasObjectKind::Connector(connector) = &mut book.sheets[0].canvas.objects[0].kind else {
        panic!("expected connector");
    };
    connector.end_marker_unfilled = false;
    let svg = String::from_utf8(export(&book, Format::Svg)).unwrap();
    let circles = [
        r##"<circle cx="0.00" cy="0.00" r="6.25" fill="none" stroke="#14283C" stroke-width="2.00"/>"##,
        r##"<circle cx="100.00" cy="0.00" r="4" fill="#14283C" stroke="#14283C" stroke-width="2.00"/>"##,
    ];
    let slide =
        render_presentation_slide_svg(&book.sheets[0], 0, &ExportOptions::default()).unwrap();
    for circle in circles {
        assert!(svg.contains(circle), "{svg}");
        assert!(slide.contains(circle), "{slide}");
    }
    assert!(svg.contains("M 6.25,0.00 L 96.00,0.00"), "{svg}");

    let options = ExportOptions {
        scale: 4.0,
        ..Default::default()
    };
    let png = image::load_from_memory(
        &export_bytes_with_consent(&book, Format::Png, &options, true).unwrap(),
    )
    .unwrap()
    .to_rgba8();
    let view = view_box(&svg);
    let sample = |x: f32, y: f32| {
        let px = ((x - view[0]) / view[2] * png.width() as f32).floor() as u32;
        let py = ((y - view[1]) / view[3] * png.height() as f32).floor() as u32;
        *png.get_pixel(px, py)
    };
    assert_eq!(sample(0.0, 0.0), sample(0.0, 12.0));
    assert_ne!(sample(0.0, 6.25), sample(0.0, 12.0));
    assert_ne!(sample(100.0, 0.0), sample(100.0, 12.0));
    assert!(export(&book, Format::Pdf).starts_with(b"%PDF"));
    let powerpoint = export(&book, Format::PowerPoint);
    let slide = String::from_utf8(entry(&powerpoint, "ppt/slides/slide1.xml")).unwrap();
    let shapes: Vec<_> = slide
        .split("<p:sp>")
        .filter(|shape| {
            shape.contains(&format!("Canvas object {id}"))
                && shape.contains(r#"<a:prstGeom prst="ellipse">"#)
        })
        .collect();
    assert_eq!(shapes.len(), 2, "{slide}");
    assert!(
        shapes
            .iter()
            .any(|shape| shape.contains("</a:prstGeom><a:noFill/><a:ln"))
    );
    assert!(
        shapes
            .iter()
            .any(|shape| shape.contains("</a:prstGeom><a:solidFill>"))
    );

    let CanvasObjectKind::Connector(connector) = &mut book.sheets[0].canvas.objects[0].kind else {
        panic!("expected connector");
    };
    connector.start_marker_size = Some(0.0);
    let svg = String::from_utf8(export(&book, Format::Svg)).unwrap();
    assert!(!svg.contains(r#"<circle cx="0.00" cy="0.00""#));
    let powerpoint = export(&book, Format::PowerPoint);
    let slide = String::from_utf8(entry(&powerpoint, "ppt/slides/slide1.xml")).unwrap();
    assert_eq!(
        slide
            .split("<p:sp>")
            .filter(|shape| {
                shape.contains(&format!("Canvas object {id}"))
                    && shape.contains(r#"<a:prstGeom prst="ellipse">"#)
            })
            .count(),
        1,
        "{slide}"
    );
    let CanvasObjectKind::Connector(connector) = &mut book.sheets[0].canvas.objects[0].kind else {
        panic!("expected connector");
    };
    connector.start_marker_size = Some(0.001);
    let svg = String::from_utf8(export(&book, Format::Svg)).unwrap();
    assert!(svg.contains(r#"<circle cx="0.00" cy="0.00" r="0.0005""#));
}

#[test]
#[ignore = "requires Python 3, pypdfium2 and Pillow"]
fn hollow_marker_pdf_rasterizes_an_outline_without_a_fill() {
    use std::process::{Command, Stdio};

    fn rasterize(pdf: &[u8]) -> image::RgbImage {
        let script = r#"
import sys
import pypdfium2 as pdfium
document = pdfium.PdfDocument(sys.stdin.buffer.read())
assert len(document) == 1
document[0].render(scale=4).to_pil().convert("RGB").save(sys.stdout.buffer, format="PNG")
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
        let px = ((x - view[0]) / view[2] * image.width() as f32).floor() as u32;
        let py = ((y - view[1]) / view[3] * image.height() as f32).floor() as u32;
        image.get_pixel(px, py).0
    }

    for (book, positions, outline) in [
        (hollow_marker_book().0, [7.0, 93.0], (90.0, 3.8)),
        (diamond_marker_book().0, [8.0, 91.0], (94.0, 2.4)),
        (oval_marker_book().0, [0.0, 100.0], (100.0, 4.0)),
    ] {
        let svg = String::from_utf8(export(&book, Format::Svg)).unwrap();
        let view = view_box(&svg);
        let hollow = rasterize(&export(&book, Format::Pdf));
        let mut filled = book.clone();
        let CanvasObjectKind::Connector(connector) = &mut filled.sheets[0].canvas.objects[0].kind
        else {
            panic!("expected connector");
        };
        connector.start_marker_unfilled = false;
        connector.end_marker_unfilled = false;
        let filled = rasterize(&export(&filled, Format::Pdf));
        for x in positions {
            let background = sample(&hollow, view, x, 12.0);
            assert_eq!(sample(&hollow, view, x, 0.0), background);
            assert_ne!(sample(&filled, view, x, 0.0), background);
        }
        let background = sample(&hollow, view, positions[1], 12.0);
        assert_ne!(sample(&hollow, view, outline.0, outline.1), background);
    }
    let (book, _) = half_circle_marker_book();
    let svg = String::from_utf8(export(&book, Format::Svg)).unwrap();
    let view = view_box(&svg);
    let open = rasterize(&export(&book, Format::Pdf));
    for x in [0.0, 100.0] {
        let background = sample(&open, view, x, 20.0);
        assert_eq!(sample(&open, view, x, 0.0), background);
    }
    for (x, y) in [(11.625, -11.625), (91.75, 8.25)] {
        assert_ne!(sample(&open, view, x, y), sample(&open, view, x, 20.0));
    }
    let (book, _) = bar_marker_book();
    let svg = String::from_utf8(export(&book, Format::Svg)).unwrap();
    let view = view_box(&svg);
    let bars = rasterize(&export(&book, Format::Pdf));
    for (x, y) in [(0.0, 5.0), (91.75, 2.75)] {
        assert_ne!(sample(&bars, view, x, y), sample(&bars, view, x, 20.0));
    }
}

#[test]
fn connector_waypoints_require_v10_and_render_in_svg() {
    let mut sheet = Sheet::new("Route", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut connector = CanvasConnector::new(
        Endpoint::Point(Vec2::new(0.0, 0.0)),
        Endpoint::Point(Vec2::new(100.0, 0.0)),
    );
    connector.waypoints = vec![Vec2::new(20.0, 40.0), Vec2::new(80.0, 40.0)].into();
    sheet
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    let book = Workbook::new(sheet);
    book.validate().unwrap();
    assert_eq!(required_native_version(&book), 10);
    let native = export(&book, Format::Thinoda);
    assert_eq!(manifest_version(&native), 10);
    assert_eq!(import_bytes(Format::Thinoda, &native).unwrap(), book);
    assert_eq!(
        export(
            &import_bytes(Format::Thinoda, &native).unwrap(),
            Format::Thinoda
        ),
        native
    );

    let content = serde_json::to_vec(&book).unwrap();
    let forged = zip_with(&[
        (
            "manifest.json",
            br#"{"format":"thinoda","version":9,"resources":[]}"#,
        ),
        ("content.json", &content),
    ]);
    assert!(
        import_bytes(Format::Thinoda, &forged)
            .unwrap_err()
            .to_string()
            .contains("connector waypoints require native format version 10")
    );
    assert!(
        import_bytes(Format::Thinoda, &content)
            .unwrap_err()
            .to_string()
            .contains("connector waypoints require a versioned native archive")
    );

    let svg = String::from_utf8(export(&book, Format::Svg)).unwrap();
    assert!(
        svg.contains("M 0.00,0.00 L 20.00,40.00 L 80.00,40.00 L 100.00,0.00"),
        "{svg}"
    );
}

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
fn formatted_card_native_clipboard_native_round_trip_is_undoable() {
    use thinoda_core::{
        ApproxMeasure, Document, EditError, InlineParagraph, ListKind,
        clipboard::{
            copy_board_selection, decode_board_payload, encode_board_payload, paste_board_selection,
        },
        layout_sheet,
    };

    for kind in [CardKind::Sticky, CardKind::Text] {
        let mut source = central();
        let title = "A🦀é\n第二";
        let card = source.sheets[0]
            .add_card(
                title,
                Vec2::new(120.0, 100.0),
                Card::new(kind, Vec2::new(180.0, 120.0)),
            )
            .unwrap();
        let topic = source.sheets[0].topic_mut(card).unwrap();
        topic
            .inline_text
            .update_range(title, 0..3, |style| {
                style.bold = Some(true);
                style.font_size = Some(18.0);
            })
            .unwrap();
        topic.inline_text.paragraphs.push(InlineParagraph {
            index: 1,
            list: ListKind::Numbered,
        });
        let native = export_bytes(&source, Format::Thinoda, &ExportOptions::default()).unwrap();
        assert_eq!(manifest_version(&native), 8);
        let loaded = import_bytes(Format::Thinoda, &native).unwrap();
        let layout = layout_sheet(loaded.active(), &ApproxMeasure, false);
        let payload = copy_board_selection(loaded.active(), &layout, &[card], &[])
            .unwrap()
            .unwrap();
        let encoded = encode_board_payload(&payload).unwrap();
        let decoded = decode_board_payload(&encoded).unwrap();
        let mut destination = Sheet::new("Destination", "Root");
        destination.add_child(destination.root, None, "Existing");
        let mut doc = Document::new(Workbook::new(destination));
        let before = doc.workbook().clone();
        let pasted = doc
            .edit_sheet("Paste", |sheet| {
                paste_board_selection(sheet, &decoded, Vec2::ZERO).map_err(EditError::Invalid)
            })
            .unwrap();
        let copied = doc.sheet().topic(pasted.topics[0]).unwrap();
        assert_eq!(
            copied.inline_text,
            loaded.active().topic(card).unwrap().inline_text
        );
        assert_eq!(copied.title, title);
        assert_ne!(pasted.topics[0], card);
        let after = doc.workbook().clone();
        assert!(doc.undo());
        assert_eq!(doc.workbook(), &before);
        assert!(doc.redo());
        assert_eq!(doc.workbook(), &after);
        let native = export_bytes(&after, Format::Thinoda, &ExportOptions::default()).unwrap();
        assert_eq!(manifest_version(&native), 8);
        assert_eq!(import_bytes(Format::Thinoda, &native).unwrap(), after);
    }
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
    let newer = thinoda_io::NATIVE_FORMAT_VERSION + 1;
    let manifest = serde_json::to_vec(&serde_json::json!({
        "format": "thinoda",
        "version": newer,
        "resources": [],
    }))
    .unwrap();
    let bytes = zip_with(&[("manifest.json", &manifest), ("content.json", &content)]);
    assert!(matches!(
        import_bytes(Format::Thinoda, &bytes),
        Err(IoError::NewerVersion(version)) if version == newer.to_string()
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

    // Plain Central workbooks lose nothing to board export, except draw.io, which has no
    // editable mind-map model and flattens every topic into a shape.
    for format in [Format::DrawIo, Format::DrawIoSvg, Format::DrawIoPng] {
        assert_eq!(
            export_losses(&central(), format, &o),
            vec![ExportLoss::MapElementsFlattened(3)],
            "{format:?}"
        );
    }
    for &format in Format::EXPORTABLE {
        if format.is_drawio() {
            continue;
        }
        assert!(
            export_losses(&central(), format, &o).is_empty(),
            "{format:?}"
        );
    }
}

#[test]
fn translucent_connector_compositing_loss_matches_export_scope() {
    let mut sheet = Sheet::new("First", "");
    for (label, opacity, text_color) in [
        ("Translucent color", 1.0, Some(Color::rgba(30, 60, 90, 128))),
        ("Translucent object", 0.5, Some(Color::BLACK)),
        ("Automatic", 0.5, None),
        ("Opaque", 1.0, Some(Color::BLACK)),
        ("Transparent", 1.0, Some(Color::TRANSPARENT)),
        ("Rounds to transparent", 0.1, Some(Color::rgba(0, 0, 0, 1))),
        ("", 0.5, Some(Color::BLACK)),
        ("Zero opacity", 0.0, Some(Color::BLACK)),
    ] {
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.label = label.into();
        let mut object = CanvasObject::connector(connector);
        object.style.opacity = opacity;
        object.style.text_color = text_color;
        sheet.add_canvas_object(object).unwrap();
    }
    let mut shape = CanvasObject::shape(
        ShapeKind::Rectangle,
        Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
    );
    shape.style.text_color = Some(Color::rgba(0, 0, 0, 128));
    sheet.add_canvas_object(shape).unwrap();

    let mut second = Sheet::new("Second", "");
    let mut connector = CanvasConnector::new(
        Endpoint::Point(Vec2::ZERO),
        Endpoint::Point(Vec2::new(100.0, 0.0)),
    );
    connector.label = "Second sheet".into();
    let mut object = CanvasObject::connector(connector);
    object.style.text_color = Some(Color::rgba(0, 0, 0, 128));
    second.add_canvas_object(object).unwrap();
    let mut book = Workbook::new(sheet);
    book.sheets.push(second);
    book.validate().unwrap();

    let options = ExportOptions::default();
    for format in [Format::Svg, Format::Png, Format::Jpeg, Format::Pdf] {
        assert_eq!(
            export_losses(&book, format, &options),
            vec![ExportLoss::ConnectorLabelCompositingApproximated(2)],
            "{format:?}"
        );
        let second_sheet = ExportOptions {
            sheet: Some(1),
            ..options.clone()
        };
        assert_eq!(
            export_losses(&book, format, &second_sheet),
            vec![ExportLoss::ConnectorLabelCompositingApproximated(1)],
            "{format:?}"
        );
    }
    assert!(export_losses(&book, Format::Thinoda, &options).is_empty());
    assert!(export_losses(&book, Format::PowerPoint, &options).is_empty());

    let frame = Rect::from_min_size(Vec2::ZERO, Vec2::new(300.0, 200.0));
    let root = book.sheets[0].add_floating("Branch", Vec2::new(150.0, 100.0));
    book.sheets[0]
        .add_presentation_slide("Editable", PresentationSource::Branch(root), frame)
        .unwrap();
    assert!(export_losses(&book, Format::PowerPoint, &options).is_empty());
    book.sheets[0]
        .add_presentation_slide("Viewport", PresentationSource::Viewport, frame)
        .unwrap();
    assert_eq!(
        export_losses(&book, Format::PowerPoint, &options),
        vec![ExportLoss::ConnectorLabelCompositingApproximated(2)]
    );
    let second_sheet = ExportOptions {
        sheet: Some(1),
        ..options
    };
    assert!(export_losses(&book, Format::PowerPoint, &second_sheet).is_empty());
}

fn translucent_connector_label_book() -> Workbook {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let mut connector = CanvasConnector::new(
        Endpoint::Point(Vec2::ZERO),
        Endpoint::Point(Vec2::new(120.0, 0.0)),
    );
    connector.label = "MMMMMMMM".into();
    connector.color = Some(Color::BLACK);
    connector.width = 8.0;
    connector.arrow_end = false;
    let mut object = CanvasObject::connector(connector);
    object.style.text_color = Some(Color::rgba(255, 0, 0, 128));
    sheet.add_canvas_object(object).unwrap();
    Workbook::new(sheet)
}

fn grouped_red(pixel: [u8; 3]) -> bool {
    (110..=145).contains(&pixel[0]) && pixel[1] < 15 && pixel[2] < 15
}

#[test]
fn translucent_connector_grouping_survives_jpeg_and_viewport_powerpoint() {
    let mut book = translucent_connector_label_book();
    let options = ExportOptions {
        scale: 4.0,
        ..Default::default()
    };
    let png = image::load_from_memory(
        &export_bytes_with_consent(&book, Format::Png, &options, true).unwrap(),
    )
    .unwrap()
    .to_rgb8();
    let jpeg = image::load_from_memory(
        &export_bytes_with_consent(&book, Format::Jpeg, &options, true).unwrap(),
    )
    .unwrap()
    .to_rgb8();
    assert_eq!(png.dimensions(), jpeg.dimensions());
    let mut expected = 0;
    let mut matched = 0;
    for (x, y, pixel) in png.enumerate_pixels() {
        if grouped_red(pixel.0) {
            expected += 1;
            let actual = jpeg.get_pixel(x, y).0;
            matched +=
                usize::from((80..=175).contains(&actual[0]) && actual[1] < 40 && actual[2] < 40);
        }
    }
    assert!(expected > 20, "the PNG must contain glyphs over the line");
    assert!(
        matched * 2 >= expected,
        "JPEG preserved {matched} of {expected} opaque glyph interiors"
    );

    book.active_mut()
        .add_presentation_slide(
            "Viewport",
            PresentationSource::Viewport,
            Rect::from_min_size(Vec2::new(-20.0, -30.0), Vec2::new(160.0, 90.0)),
        )
        .unwrap();
    let pptx = export(&book, Format::PowerPoint);
    let image = image::load_from_memory(&entry(&pptx, "ppt/media/slide1-viewport.png"))
        .unwrap()
        .to_rgb8();
    assert!(
        image.pixels().filter(|pixel| grouped_red(pixel.0)).count() > 20,
        "the flattened slide must preserve grouped glyphs over the line"
    );
}

#[test]
#[ignore = "requires Python 3, pypdfium2 and Pillow"]
fn translucent_connector_pdf_preserves_grouped_label_compositing() {
    use std::process::{Command, Stdio};

    let book = translucent_connector_label_book();
    let options = ExportOptions {
        scale: 4.0,
        ..Default::default()
    };
    let png = image::load_from_memory(
        &export_bytes_with_consent(&book, Format::Png, &options, true).unwrap(),
    )
    .unwrap()
    .to_rgb8();
    let pdf = export_bytes_with_consent(&book, Format::Pdf, &options, true).unwrap();
    let script = r#"
import sys
import pypdfium2 as pdfium
document = pdfium.PdfDocument(sys.stdin.buffer.read())
assert len(document) == 1
document[0].render(scale=4).to_pil().convert("RGB").save(sys.stdout.buffer, format="PNG")
"#;
    let mut child = Command::new("python")
        .args(["-c", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("install Python, pypdfium2 and Pillow to run this ignored test");
    child.stdin.take().unwrap().write_all(&pdf).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "PDF rasterization failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let raster = image::load_from_memory(&output.stdout).unwrap().to_rgb8();
    assert_eq!(raster.dimensions(), png.dimensions());
    let mut expected = 0;
    let mut matched = 0;
    for (x, y, pixel) in png.enumerate_pixels() {
        if grouped_red(pixel.0) {
            expected += 1;
            matched += usize::from(grouped_red(raster.get_pixel(x, y).0));
        }
    }
    assert!(expected > 20, "the PNG must contain glyphs over the line");
    assert!(
        matched * 2 >= expected,
        "PDF preserved {matched} of {expected} opaque glyph interiors"
    );
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
fn drawio_tutorial_images_export_without_extra_outlines() {
    let wb = import_bytes_with_report(
        Format::DrawIo,
        include_bytes!("../../../docs/draw-io-examples/board-visual-tutorial.drawio"),
    )
    .unwrap()
    .workbook;
    let images: Vec<_> = wb
        .active()
        .content_topics()
        .filter(|topic| {
            topic.card.is_some_and(|card| {
                card.kind == CardKind::Image && card.color.is_some_and(|color| color.a == 0)
            })
        })
        .collect();
    assert_eq!(images.len(), 3);

    for dark in [false, true] {
        let options = ExportOptions {
            dark,
            ..Default::default()
        };
        let svg =
            String::from_utf8(export_bytes_with_consent(&wb, Format::Svg, &options, true).unwrap())
                .unwrap();
        for topic in &images {
            let id = topic
                .image
                .as_deref()
                .unwrap()
                .strip_prefix(thinoda_core::resources::RESOURCE_PREFIX)
                .unwrap();
            let marker = format!("<use xlink:href=\"#image-{id}\"");
            let occurrences: Vec<_> = svg.match_indices(&marker).map(|(index, _)| index).collect();
            assert!(!occurrences.is_empty());
            for image in occurrences {
                let previous_tag = svg[..image].rfind('<').unwrap();
                let background = &svg[previous_tag..image];
                assert!(
                    background.starts_with("<rect class=\"card\" "),
                    "{background}"
                );
                assert!(background.contains("fill=\"#00000000\""), "{background}");
                assert!(background.contains("stroke=\"none\""), "{background}");
            }
        }
        let png = image::load_from_memory(
            &export_bytes_with_consent(&wb, Format::Png, &options, true).unwrap(),
        )
        .unwrap()
        .to_rgb8();
        assert!(
            png.pixels()
                .all(|pixel| !(pixel[0] > 240 && pixel[1] < 110 && pixel[2] < 110)),
            "unexpected red image-card outlines in dark={dark}"
        );
        assert!(
            export_bytes_with_consent(&wb, Format::Pdf, &options, true)
                .unwrap()
                .starts_with(b"%PDF")
        );
    }
}

#[test]
fn transparent_image_cards_and_native_image_cards_keep_their_svg_and_png_appearance() {
    let mut sheet = Sheet::new("Image", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let id = sheet
        .add_card(
            "",
            Vec2::ZERO,
            Card::new(CardKind::Image, Vec2::new(180.0, 130.0)),
        )
        .unwrap();
    let source = image::RgbImage::from_pixel(8, 8, image::Rgb([18, 196, 120]));
    let mut encoded = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(source)
        .write_to(&mut encoded, image::ImageFormat::Png)
        .unwrap();
    let resource = Resource::new("image.png", "image/png", encoded.into_inner()).unwrap();
    thinoda_core::images::set_image(&mut sheet, id, Some((resource, Vec2::new(80.0, 80.0))))
        .unwrap();
    let wb = Workbook::new(sheet);

    for dark in [false, true] {
        for shape in [TopicShape::Ellipse, TopicShape::Underline] {
            for color in [
                None,
                Some(Color::rgb(10, 20, 30)),
                Some(Color::rgba(10, 20, 30, 128)),
                Some(Color::rgba(10, 20, 30, 0)),
            ] {
                let mut wb = wb.clone();
                let topic = wb.active_mut().topic_mut(id).unwrap();
                topic.card.as_mut().unwrap().color = color;
                topic.style.shape = Some(shape);
                topic.style.border_color = Some(Color::rgb(251, 10, 20));
                topic.style.border_width = Some(4.0);
                let before = wb.clone();
                let options = ExportOptions {
                    dark,
                    ..Default::default()
                };
                let svg = String::from_utf8(
                    export_bytes_with_consent(&wb, Format::Svg, &options, true).unwrap(),
                )
                .unwrap();
                let png = image::load_from_memory(
                    &export_bytes_with_consent(&wb, Format::Png, &options, true).unwrap(),
                )
                .unwrap()
                .to_rgb8();
                let red_pixels = png
                    .pixels()
                    .filter(|pixel| pixel[0] > 240 && pixel[1] < 110 && pixel[2] < 110)
                    .count();
                if color.is_some_and(|color| color.a == 0) {
                    assert!(svg.contains("fill=\"#0A141E00\" stroke=\"none\""));
                    assert_eq!(red_pixels, 0, "dark={dark}, shape={shape:?}");
                } else {
                    assert!(!svg.contains("class=\"card\""));
                    assert!(svg.contains("stroke=\"#FB0A14\""), "{svg}");
                    assert!(red_pixels > 0, "dark={dark}, shape={shape:?}");
                }
                assert!(png.pixels().any(|pixel| pixel.0 == [18, 196, 120]));
                assert_eq!(wb, before);
            }
        }
    }
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
fn xmind_private_snapshot_versions_and_preserves_connector_waypoints() {
    let mut sheet = Sheet::new("Routes", "Root");
    let mut connector = CanvasConnector::new(
        Endpoint::Point(Vec2::ZERO),
        Endpoint::Point(Vec2::new(100.0, 0.0)),
    );
    connector.waypoints = vec![Vec2::new(50.0, 40.0)].into();
    sheet
        .add_canvas_object(CanvasObject::connector(connector))
        .unwrap();
    let book = Workbook::new(sheet);
    let bytes = export(&book, Format::XMind);
    let mut metadata: serde_json::Value =
        serde_json::from_slice(&entry(&bytes, "metadata.json")).unwrap();
    let snapshot = metadata["thinodaWorkbook"].as_str().unwrap();
    let mut tagged: serde_json::Value = serde_json::from_str(snapshot).unwrap();
    assert_eq!(tagged[0], "THINODA-XMIND-WAYPOINTS-1");
    assert!(serde_json::from_str::<Workbook>(snapshot).is_err());
    assert_eq!(import_bytes(Format::XMind, &bytes).unwrap(), book);

    tagged[0] = serde_json::json!("THINODA-XMIND-WAYPOINTS-999");
    tagged[1] = serde_json::json!({"sheets": "incompatible future schema"});
    metadata["thinodaWorkbook"] = serde_json::Value::String(tagged.to_string());
    let metadata = serde_json::to_vec(&metadata).unwrap();
    let content = entry(&bytes, "content.json");
    let manifest = entry(&bytes, "manifest.json");
    let legacy = entry(&bytes, "content.xml");
    let future = zip_with(&[
        ("metadata.json", &metadata),
        ("content.json", &content),
        ("manifest.json", &manifest),
        ("content.xml", &legacy),
    ]);
    assert!(matches!(
        import_bytes(Format::XMind, &future),
        Err(thinoda_io::IoError::NewerVersion(version))
            if version == "THINODA-XMIND-WAYPOINTS-999"
    ));
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
